//! Future actual printed acceptance, requiring parent-owned real registration.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use mtg_kernel::card_def::{card_id_by_name, Keywords, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision, EffectDuration, Layers, UntilEndOfTurnEffect};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

fn put(state: &mut GameState, owner: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let def = card_id_by_name(name).expect("true future definition registration is required");
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
        1127,
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

fn priority(state: &mut GameState, actor: PlayerId) {
    for _ in 0..4 {
        let d = next(state);
        if matches!(d, Decision::CastSpellOrPass { player, .. } if player == actor) {
            return;
        }
        match d {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
            }
            other => panic!("unexpected completion decision {other:?}"),
        }
    }
    panic!("missing priority");
}

fn finish(state: &mut GameState) {
    for _ in 0..30 {
        let d = next(state);
        if state.stack.is_empty()
            && state.engine.pending_triggers.is_empty()
            && matches!(d, Decision::CastSpellOrPass { .. })
        {
            return;
        }
        match d {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
            }
            other => panic!("unexpected completion decision {other:?}"),
        }
    }
    panic!("resolution did not complete");
}

fn restore(state: &GameState) -> GameState {
    let bytes = serde_json::to_vec(state).unwrap();
    let value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(serde_json::to_vec(&value).unwrap(), bytes);
    value
}

fn control_primitive(state: &mut GameState, target: ObjectId, controller: PlayerId) {
    let old = state.objects.get(target).controller;
    state.players[old.index()]
        .battlefield
        .retain(|&id| id != target);
    state.players[controller.index()].battlefield.push(target);
    state.objects.get_mut(target).controller = controller;
}

fn granted_target_keyword_primitive(state: &mut GameState, target: ObjectId, keyword: Keywords) {
    let timestamp = state
        .engine
        .until_end_of_turn
        .iter()
        .filter_map(|effect| {
            if let UntilEndOfTurnEffect::ResolvedObjectCantBeBlockedV1 { timestamp, .. } = effect {
                Some(*timestamp)
            } else {
                None
            }
        })
        .max()
        .unwrap_or(0)
        + 100;
    state
        .engine
        .until_end_of_turn
        .push(UntilEndOfTurnEffect::ResolvedObjectKeywordEffect {
            object_id: target,
            object_zone_change_count: state.objects.get(target).zone_change_count,
            layer: Layers::ABILITY_ADDING,
            timestamp,
            duration: EffectDuration::EndOfTurn,
            keywords: keyword,
        });
}

fn source(state: &mut GameState, actor: PlayerId) -> ObjectId {
    let source = put(state, actor, "High-Society Hunter", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::B.pool_index()] = 2;
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 3;
    priority(state, actor);
    engine::step(state, Action::CastSpell(source)).unwrap();
    finish(state);
    assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
    assert!(state.objects.get(source).summoning_sick);
    assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
    // Accepted physical haste grant, not an invented printed ability.
    granted_target_keyword_primitive(state, source, Keywords::HASTE);
    source
}

fn attack(state: &mut GameState, source: ObjectId) {
    for _ in 0..20 {
        let d = next(state);
        if let Decision::DeclareAttackers { eligible, .. } = d {
            assert!(eligible.contains(&source));
            engine::step(state, Action::DeclareAttackers(vec![source])).unwrap();
            return;
        }
        assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("no actual attack declaration");
}

fn choice(state: &mut GameState, actor: PlayerId, source: ObjectId) -> Vec<Target> {
    for _ in 0..25 {
        let d = next(state);
        if let Decision::ChooseEffectTargets {
            player,
            source: s,
            min_targets,
            max_targets,
            selected_count,
            legal_targets,
            can_finish,
        } = d
        {
            assert_eq!(
                (player, s, min_targets, selected_count),
                (actor, source, 0, 0)
            );
            assert!(max_targets <= 1 && can_finish);
            return legal_targets;
        }
        assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("missing genuine Hunter optional sacrifice")
}

fn witness(state: &mut GameState, actor: PlayerId, target: ObjectId) {
    let aura = put(state, actor, "Witness Protection", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 1;
    priority(state, actor);
    engine::step(state, Action::CastSpell(aura)).unwrap();
    assert!(
        matches!(next(state), Decision::ChooseTargets { legal_targets, .. } if legal_targets.contains(&Target::Object(target)))
    );
    engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    for _ in 0..20 {
        let d = next(state);
        if state.objects.get(aura).zone == Zone::Battlefield {
            break;
        }
        assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
        engine::step(state, Action::Pass).unwrap();
    }
    assert_eq!(state.objects.get(aura).zone, Zone::Battlefield);
}

#[test]
#[ignore = "unregistered printed Hunter; parent serial admission and native qualification required"]
fn sacrificed_token_pays_counter_but_no_death_draw_and_second_selection_refuses_readonly() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let hunter = source(&mut state, actor);
        let token = put(&mut state, actor, "Goblin Token", Zone::Battlefield);
        let other = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        attack(&mut state, hunter);
        assert!(choice(&mut state, actor, hunter).contains(&Target::Object(token)));
        let hand = state.players[actor.index()].hand.len();
        let mut replay = restore(&state);
        for current in [&mut state, &mut replay] {
            engine::step(current, Action::ChooseEffectTarget(Target::Object(token))).unwrap();
            let before = serde_json::to_vec(current).unwrap();
            assert!(
                engine::step(current, Action::ChooseEffectTarget(Target::Object(other))).is_err()
            );
            assert_eq!(serde_json::to_vec(current).unwrap(), before);
            finish(current);
            assert_ne!(current.objects.get(token).zone, Zone::Battlefield);
            assert_eq!(current.objects.get(hunter).counters.plus1_plus1, 1);
            assert_eq!(current.players[actor.index()].hand.len(), hand);
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}

#[test]
#[ignore = "unregistered printed Hunter; parent serial admission and native qualification required"]
fn real_witness_suppresses_death_observer_at_commit_but_later_suppression_keeps_attack_counter() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for captured_attack in [false, true] {
            let mut state = ready(actor);
            let hunter = source(&mut state, actor);
            let other = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            if captured_attack {
                put(&mut state, actor, "High Fae Trickster", Zone::Battlefield);
                attack(&mut state, hunter);
            }
            witness(&mut state, actor, hunter);
            let hand = state.players[actor.index()].hand.len();
            if captured_attack {
                assert!(choice(&mut state, actor, hunter).contains(&Target::Object(other)));
                engine::step(
                    &mut state,
                    Action::ChooseEffectTarget(Target::Object(other)),
                )
                .unwrap();
            } else {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(other, Zone::Graveyard),
                );
            }
            let mut replay = restore(&state);
            for current in [&mut state, &mut replay] {
                finish(current);
                assert_eq!(
                    current.objects.get(hunter).counters.plus1_plus1,
                    i32::from(captured_attack)
                );
                assert_eq!(current.players[actor.index()].hand.len(), hand);
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
}

#[test]
#[ignore = "unregistered printed Hunter; parent serial admission and native qualification required"]
fn death_draw_keeps_event_controller_after_source_control_change_and_exile_is_not_death() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for death in [false, true] {
            let mut state = ready(actor);
            let hunter = source(&mut state, actor);
            let other = put(
                &mut state,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            let hand = state.players[actor.index()].hand.len();
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(
                    other,
                    if death { Zone::Graveyard } else { Zone::Exile },
                ),
            );
            control_primitive(&mut state, hunter, actor.opponent());
            let mut replay = restore(&state);
            for current in [&mut state, &mut replay] {
                finish(current);
                assert_eq!(
                    current.players[actor.index()].hand.len(),
                    hand + usize::from(death)
                );
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
}

#[test]
#[ignore = "unregistered printed Hunter; parent serial admission and native qualification required"]
fn paid_printed_attack_decline_or_sacrifice_counter_and_independent_death_draw_restores_both_seats()
{
    for actor in [PlayerId::P0, PlayerId::P1] {
        for pay in [false, true] {
            let mut state = ready(actor);
            let hunter = source(&mut state, actor);
            let other = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let foreign = put(
                &mut state,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            attack(&mut state, hunter);
            let menu = choice(&mut state, actor, hunter);
            assert!(menu.contains(&Target::Object(other)));
            assert!(!menu.contains(&Target::Object(hunter)));
            let hand = state.players[actor.index()].hand.len();
            let top = state.players[actor.index()].library[0];
            let mut replay = restore(&state);
            for current in [&mut state, &mut replay] {
                let before = serde_json::to_vec(current).unwrap();
                for illegal in [hunter, foreign] {
                    assert!(engine::step(
                        current,
                        Action::ChooseEffectTarget(Target::Object(illegal))
                    )
                    .is_err());
                    assert_eq!(serde_json::to_vec(current).unwrap(), before);
                }
                if pay {
                    engine::step(current, Action::ChooseEffectTarget(Target::Object(other)))
                        .unwrap();
                } else {
                    engine::step(current, Action::FinishEffectSelection).unwrap();
                }
                finish(current);
                assert_eq!(
                    current.objects.get(hunter).counters.plus1_plus1,
                    i32::from(pay)
                );
                assert_eq!(
                    current.objects.get(other).zone,
                    if pay {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                assert_eq!(
                    current.players[actor.index()].hand.len(),
                    hand + usize::from(pay)
                );
                if pay {
                    assert!(current.players[actor.index()].hand.contains(&top));
                    assert!(current
                        .known_hand_cards(actor.opponent(), actor)
                        .iter()
                        .all(|k| k.object != top));
                }
                assert!(mtg_kernel::rl::observe_v1(current, actor, 0).is_err());
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
}

#[test]
#[ignore = "unregistered printed Hunter; parent serial admission and native qualification required"]
fn event_time_nontoken_any_controller_and_simultaneous_source_death_draw_each_other_both_orders() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for source_first in [false, true] {
            let mut state = ready(actor);
            let hunter = source(&mut state, actor);
            let own = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let opponent = put(
                &mut state,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            let token = put(
                &mut state,
                actor.opponent(),
                "Goblin Token",
                Zone::Battlefield,
            );
            let objects = if source_first {
                vec![hunter, own, opponent, token]
            } else {
                vec![token, opponent, own, hunter]
            };
            let hand = state.players[actor.index()].hand.len();
            event::propose_and_commit_batch(
                &mut state,
                objects
                    .into_iter()
                    .map(|id| ProposedEvent::zone_change(id, Zone::Graveyard))
                    .collect(),
            );
            assert_eq!(state.objects.get(hunter).zone, Zone::Graveyard);
            let mut replay = restore(&state);
            for current in [&mut state, &mut replay] {
                finish(current);
                assert_eq!(current.players[actor.index()].hand.len(), hand + 2);
                assert_eq!(current.objects.get(hunter).counters.plus1_plus1, 0);
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
}

#[test]
#[ignore = "unregistered printed Hunter; parent serial admission and native qualification required"]
fn captured_attack_controller_and_exact_source_survive_control_change_or_departure_but_not_reentry_counter(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for reenter in [false, true] {
            let mut state = ready(actor);
            let hunter = source(&mut state, actor);
            let other = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            attack(&mut state, hunter);
            // Explicit physical control/departure primitives after genuine attack, not a fake printed spell.
            if reenter {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(hunter, Zone::Hand),
                );
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(hunter, Zone::Battlefield),
                );
            } else {
                control_primitive(&mut state, hunter, actor.opponent());
            }
            assert!(choice(&mut state, actor, hunter).contains(&Target::Object(other)));
            let hand = state.players[actor.index()].hand.len();
            let mut replay = restore(&state);
            for current in [&mut state, &mut replay] {
                engine::step(current, Action::ChooseEffectTarget(Target::Object(other))).unwrap();
                finish(current);
                assert_eq!(
                    current.objects.get(hunter).counters.plus1_plus1,
                    i32::from(!reenter)
                );
                // The death observer uses Hunter's event-time current controller, not its earlier attack controller.
                assert_eq!(
                    current.players[actor.index()].hand.len(),
                    hand + usize::from(reenter)
                );
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
}

#[test]
#[ignore = "unregistered printed Hunter; parent serial admission and native qualification required"]
fn exact_meaning_all_root_facts_selector_and_unknown_fields_refuse_forgery() {
    use mtg_kernel::rules_meaning_high_society_hunter_v1 as meaning;
    let id = card_id_by_name("High-Society Hunter").unwrap();
    let selector = meaning::live_selector().unwrap();
    for root in [
        meaning::RootV1::CreatureSpell,
        meaning::RootV1::Attack,
        meaning::RootV1::OtherNontokenDeath,
    ] {
        let value = meaning::meaning(&selector, id, root).unwrap();
        meaning::validate(&value).unwrap();
        for field in 0..5 {
            let mut forged = value.clone();
            match field {
                0 => forged.root_sha256.push('x'),
                1 => forged.card_id ^= 1,
                2 => forged.ordered_facts.reverse(),
                3 => forged.public_human_model_sampler_admitted = true,
                _ => forged.selector.card_db_hash ^= 1,
            }
            assert!(meaning::validate(&forged).is_err());
        }
        let mut json = serde_json::to_value(&value).unwrap();
        json["unknown"] = true.into();
        assert!(serde_json::from_value::<meaning::MeaningV1>(json).is_err());
    }
}
