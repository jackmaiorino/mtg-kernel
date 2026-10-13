//! Future printed acceptance: requires serial registration of the prepared definition.
//! No fixture admission hook or source-definition substitution is used.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use mtg_kernel::card_def::{card_id_by_name, CardType, Keywords, Subtype, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::policy_observation_v9::observe_policy_v9;
use mtg_kernel::policy_surface_v5::PolicySurfaceV5;
use mtg_kernel::rules_meaning_searslicer_v1::{self as meaning, ErrorV1};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

fn put(state: &mut GameState, owner: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let def = card_id_by_name(name)
        .expect("future printed definition must be registered by the admission owner");
    let id = state.objects.push(GameObject {
        card_def: def,
        name: CARD_DEFS[usize::from(def)].object_name.into(),
        owner,
        controller: owner,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: vec![],
        v4: ObjectStateV4::from_card_def(def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[owner.index()].hand.push(id),
        Zone::Battlefield => state.players[owner.index()].battlefield.push(id),
        _ => panic!("unsupported fixture placement"),
    }
    id
}
fn ready(actor: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |_| "Forest".into(),
        1126,
        actor,
    );
    state.step = Step::Main1;
    for player in [PlayerId::P0, PlayerId::P1] {
        put(&mut state, player, "Mountain", Zone::Battlefield);
        put(&mut state, player, "Lightning Bolt", Zone::Hand);
    }
    state
}
fn next(state: &mut GameState) -> Decision {
    let d = engine::advance_until_decision(state);
    assert!(!matches!(d, Decision::Halted { .. }), "{d:?}");
    d
}
fn pass(state: &mut GameState) {
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    engine::step(state, Action::Pass).unwrap();
}
fn priority(state: &mut GameState, actor: PlayerId) {
    for _ in 0..4 {
        if matches!(next(state), Decision::CastSpellOrPass { player, .. } if player == actor) {
            return;
        }
        pass(state);
    }
    panic!("missing priority for {actor:?}");
}
fn finish(state: &mut GameState) {
    for _ in 0..30 {
        let d = next(state);
        if matches!(d, Decision::CastSpellOrPass { .. })
            && state.stack.is_empty()
            && state.engine.pending_triggers.is_empty()
        {
            return;
        }
        assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("resolution did not finish");
}
fn cast(state: &mut GameState, actor: PlayerId, card: ObjectId) {
    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 20;
    priority(state, actor);
    engine::step(state, Action::CastSpell(card)).unwrap();
    finish(state);
    assert_eq!(state.objects.get(card).zone, Zone::Battlefield);
}
fn source(state: &mut GameState, actor: PlayerId) -> ObjectId {
    let card = put(state, actor, "Searslicer Goblin", Zone::Hand);
    cast(state, actor, card);
    card
}
fn combat(state: &mut GameState, actor: PlayerId, declaration: u8) -> ObjectId {
    let attacker = put(state, actor, "Llanowar Elves", Zone::Battlefield);
    state.step = Step::DeclareAttackers;
    if declaration == 2 {
        let before = serde_json::to_vec(&state).unwrap();
        assert!(engine::step(state, Action::DeclareAttackers(vec![attacker, attacker])).is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
    }
    engine::step(
        state,
        Action::DeclareAttackers(if declaration == 1 {
            vec![attacker]
        } else {
            vec![]
        }),
    )
    .unwrap();
    for _ in 0..40 {
        let d = next(state);
        if state.step == Step::Main2
            && matches!(d, Decision::CastSpellOrPass { player, .. } if player == actor)
        {
            assert!(state.engine.combat.attackers.is_empty());
            return attacker;
        }
        match d {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::DeclareBlockers { .. } => {
                engine::step(state, Action::DeclareBlockers(vec![])).unwrap()
            }
            other => panic!("unexpected combat decision {other:?}"),
        }
    }
    panic!("combat did not reach Main2");
}
fn end(state: &mut GameState) {
    for _ in 0..24 {
        let d = next(state);
        if state.step == Step::End {
            assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
            return;
        }
        assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("no actual end-step event");
}
fn tokens(state: &GameState, player: PlayerId) -> Vec<ObjectId> {
    state.players[player.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&id| state.objects.get(id).card_def == card_id_by_name("Goblin Token").unwrap())
        .collect()
}
fn restore(state: &GameState) -> GameState {
    let bytes = serde_json::to_vec(state).unwrap();
    let restored = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(serde_json::to_vec(&restored).unwrap(), bytes);
    restored
}
/// Controlled board mutation is a labeled control-change primitive, not a claim
/// that an unavailable printed control spell was cast in this catalog.
fn set_controller_primitive(state: &mut GameState, object: ObjectId, controller: PlayerId) {
    let old = state.objects.get(object).controller;
    assert_eq!(state.objects.get(object).zone, Zone::Battlefield);
    state.players[old.index()]
        .battlefield
        .retain(|&id| id != object);
    state.players[controller.index()].battlefield.push(object);
    state.objects.get_mut(object).controller = controller;
}
fn witness(state: &mut GameState, actor: PlayerId, host: ObjectId) {
    let aura = put(state, actor, "Witness Protection", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 1;
    priority(state, actor);
    engine::step(state, Action::CastSpell(aura)).unwrap();
    assert!(
        matches!(next(state), Decision::ChooseTargets { legal_targets, .. } if legal_targets.contains(&Target::Object(host)))
    );
    engine::step(state, Action::ChooseTarget(Target::Object(host))).unwrap();
    finish(state);
    assert_eq!(state.objects.get(aura).zone, Zone::Battlefield);
}

#[test]
fn future_printed_metadata_and_exact_definition_owned_meaning() {
    let id = card_id_by_name("Searslicer Goblin").expect("future registration prerequisite");
    let def = &CARD_DEFS[usize::from(id)];
    assert!(def.is_executable());
    assert_eq!(
        (def.mana_value, def.power, def.toughness),
        (2, Some(2), Some(1))
    );
    assert_eq!(def.cost.generic, 1);
    assert_eq!(def.cost.pips, &[Pip::Colored(ManaColor::R)]);
    assert_eq!(def.types, &[CardType::Creature]);
    assert_eq!(def.colors, &[ManaColor::R]);
    assert_eq!(def.subtypes, &[Subtype::Goblin, Subtype::Warrior]);
    assert_eq!(def.keywords, Keywords::NONE);
    let selector = meaning::live_selector_v1().unwrap();
    let m = meaning::meaning_v1(&selector, id, 0).unwrap();
    assert!(m.capture_controller && m.recheck_captured_controller_on_resolution);
    assert!(m.declaration_round_and_active_seat_bound && !m.surviving_attacker_required);
    assert!(m.untargeted && !m.optional && m.token_summoning_sick);
    assert_eq!(m.token_count, 1);
    assert!(!m.token_tapped && !m.token_attacking && !m.token_has_haste);
    assert_eq!(
        meaning::meaning_v1(&selector, id, 1),
        Err(ErrorV1::WrongRoot)
    );
}
#[test]
fn actual_successful_declined_and_failed_declarations_drive_end_step_raid_for_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for declaration in 0..3 {
            let mut state = ready(actor);
            source(&mut state, actor);
            let attacker = combat(&mut state, actor, declaration);
            // A departed attacker and cleared combat cannot erase a success.
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(attacker, Zone::Graveyard),
            );
            assert_eq!(state.player_attacked_this_turn_v1(actor), declaration == 1);
            end(&mut state);
            assert_eq!(state.stack.len(), usize::from(declaration == 1));
            let mut replay = restore(&state);
            finish(&mut state);
            finish(&mut replay);
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
            assert_eq!(tokens(&state, actor).len(), usize::from(declaration == 1));
            assert!(tokens(&state, actor.opponent()).is_empty());
            if declaration == 1 {
                let token = tokens(&state, actor)[0];
                let object = state.objects.get(token);
                assert!(object.v4.is_token && object.summoning_sick);
                assert_eq!((object.owner, object.controller), (actor, actor));
                assert!(!object.tapped);
                assert!(!engine::has_effective_keyword(
                    &state,
                    token,
                    Keywords::HASTE
                ));
                assert_eq!(
                    (
                        engine::effective_power(&state, token),
                        engine::effective_toughness(&state, token)
                    ),
                    (1, 1)
                );
            }
        }
    }
}
#[test]
fn captured_controller_survives_current_control_source_departure_and_reentry() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for mode in 0..3 {
            let mut state = ready(actor);
            let card = source(&mut state, actor);
            combat(&mut state, actor, 1);
            end(&mut state);
            assert_eq!(state.stack.len(), 1);
            assert_eq!(state.stack[0].controller, actor);
            match mode {
                0 => set_controller_primitive(&mut state, card, actor.opponent()),
                _ => {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(card, Zone::Graveyard),
                    );
                    if mode == 2 {
                        event::propose_and_commit(
                            &mut state,
                            ProposedEvent::zone_change(card, Zone::Battlefield),
                        );
                    }
                }
            }
            let mut replay = restore(&state);
            finish(&mut state);
            finish(&mut replay);
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
            assert_eq!(tokens(&state, actor).len(), 1);
            assert!(tokens(&state, actor.opponent()).is_empty());
        }
    }
}
#[test]
fn pre_capture_control_ability_suppression_and_post_capture_suppression_are_distinct() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for mode in 0..3 {
            let mut state = ready(actor);
            let card = source(&mut state, actor);
            combat(&mut state, actor, 1);
            if mode == 0 {
                set_controller_primitive(&mut state, card, actor.opponent());
            }
            if mode == 1 {
                witness(&mut state, actor, card);
            }
            if mode == 2 {
                // Witness is an enchantment; this actual registered static
                // permission makes the end-step response legal.
                put(&mut state, actor, "High Fae Trickster", Zone::Battlefield);
            }
            end(&mut state);
            assert_eq!(state.stack.len(), usize::from(mode == 2));
            if mode == 2 {
                witness(&mut state, actor, card);
            }
            finish(&mut state);
            assert_eq!(tokens(&state, actor).len(), usize::from(mode == 2));
            assert!(tokens(&state, actor.opponent()).is_empty());
        }
    }
}
#[test]
fn restored_intervening_condition_rechecks_round_active_seat_and_missing_history() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for mode in 0..3 {
            let mut state = ready(actor);
            source(&mut state, actor);
            combat(&mut state, actor, 1);
            end(&mut state);
            let mut state = restore(&state);
            match mode {
                0 => state.turn += 1,
                1 => state.active_player = actor.opponent(),
                _ => state.attack_declaration_turn_v1 = None,
            }
            if mode < 2 {
                super::fixture_turn_v1::stale_turn_refuses_readonly(&mut state);
            } else {
                finish(&mut state);
            }
            assert!(tokens(&state, actor).is_empty());
        }
    }
}
#[test]
fn tokens_emit_real_entry_events_and_cleanup_resets_history_on_actual_player_flip() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        source(&mut state, actor);
        let squire = put(&mut state, actor, "Skyknight Squire", Zone::Battlefield);
        combat(&mut state, actor, 1);
        end(&mut state);
        let counters = state.objects.get(squire).counters.plus1_plus1;
        finish(&mut state);
        assert_eq!(tokens(&state, actor).len(), 1);
        assert_eq!(state.objects.get(squire).counters.plus1_plus1, counters + 1);
        let mut replay = restore(&state);
        for _ in 0..24 {
            let d = next(&mut state);
            let rd = next(&mut replay);
            assert_eq!(d, rd);
            if state.active_player != actor {
                break;
            }
            assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
            engine::step(&mut state, Action::Pass).unwrap();
            engine::step(&mut replay, Action::Pass).unwrap();
        }
        assert_eq!(state.active_player, actor.opponent());
        assert!(state.attack_declaration_turn_v1.is_none());
        assert_eq!(tokens(&state, actor).len(), 1);
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}
#[test]
fn existing_public_raid_transport_preserves_foreign_hidden_identity_privacy_and_v8_omission() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        source(&mut state, actor);
        combat(&mut state, actor, 1);
        end(&mut state);
        let surface = PolicySurfaceV5::new();
        let view = |s: &GameState| observe_policy_v9(s, &surface, actor, 0, 0, 0, 1).unwrap();
        let public = view(&state);
        assert!(public.extensions.attacked_this_turn[actor.index()]);
        let mut hidden = state.clone();
        let hand = hidden.players[actor.opponent().index()].hand[0];
        let replacement = card_id_by_name("Forest").unwrap();
        hidden.objects.get_mut(hand).card_def = replacement;
        hidden.objects.get_mut(hand).name = "Forest".into();
        hidden.objects.get_mut(hand).v4 = ObjectStateV4::from_card_def(replacement);
        assert_eq!(view(&hidden), public);
        let mut absent = state.clone();
        absent.attack_declaration_turn_v1 = None;
        let old = |s| mtg_kernel::rl::observe_policy_v8(s, &surface, actor, 0, 0, 0, 1).unwrap();
        assert_eq!(old(&state), old(&absent));
        assert_ne!(
            view(&absent).visible_projection_hash,
            public.visible_projection_hash
        );
    }
}
