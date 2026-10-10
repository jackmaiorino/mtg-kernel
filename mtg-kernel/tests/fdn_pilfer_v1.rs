//! Prepared v76 Pilfer cases, pending serial admission.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{card_id_by_name, CardCapability, CardType, TargetSpec, CARD_DEFS};
use mtg_kernel::effect::EffectOp;
use mtg_kernel::engine::{self, Action, Decision, UnsupportedMechanic};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::rl::HarnessSurfaceV2;
use mtg_kernel::rl_session::observe_v2;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

fn ready(player: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |_| "Forest".into(),
        760,
        player,
    );
    state.step = Step::Main1;
    state
}

fn put(state: &mut GameState, player: PlayerId, name: &str) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap();
    let id = state.objects.push(GameObject {
        card_def,
        name: name.into(),
        owner: player,
        controller: player,
        zone: Zone::Hand,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: vec![],
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    state.players[player.index()].hand.push(id);
    id
}

fn next(state: &mut GameState) -> Decision {
    let d = engine::advance_until_decision(state);
    assert!(!matches!(d, Decision::Halted { .. }), "{d:?}");
    d
}

fn refuse(state: &mut GameState, action: Action) {
    let before = serde_json::to_vec(state).unwrap();
    assert!(engine::step(state, action).is_err());
    assert_eq!(serde_json::to_vec(state).unwrap(), before);
}

fn begin(state: &mut GameState, caster: PlayerId) -> ObjectId {
    let source = put(state, caster, "Pilfer");
    state.players[caster.index()].mana_pool[ManaColor::B.pool_index()] = 1;
    state.players[caster.index()].mana_pool[5] = 1;
    next(state);
    engine::step(state, Action::CastSpell(source)).unwrap();
    assert!(matches!(
        next(state),
        Decision::ChooseTargets { remaining: 1, .. }
    ));
    refuse(state, Action::ChooseTarget(Target::Player(caster)));
    engine::step(
        state,
        Action::ChooseTarget(Target::Player(caster.opponent())),
    )
    .unwrap();
    source
}

fn discard_choice(state: &mut GameState) -> Decision {
    for _ in 0..32 {
        let d = next(state);
        if matches!(d, Decision::ChooseEffectTargets { .. }) {
            return d;
        }
        assert!(matches!(d, Decision::CastSpellOrPass { .. }));
        assert!(!state.stack.is_empty() || state.engine.pending_cast.is_some());
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("no revealed-hand choice")
}

fn settle(state: &mut GameState) {
    for _ in 0..32 {
        let d = next(state);
        if matches!(d, Decision::CastSpellOrPass { .. }) && state.stack.is_empty() {
            return;
        }
        assert!(matches!(d, Decision::CastSpellOrPass { .. }));
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("spell did not resolve")
}

#[test]
fn pilfer_printed_contract_includes_creatures_and_costs_one_black_plus_one() {
    let id = card_id_by_name("Pilfer").unwrap();
    assert_eq!(id, 350);
    let def = &CARD_DEFS[id as usize];
    assert_eq!(def.capability, CardCapability::Full);
    assert_eq!(def.types, &[CardType::Sorcery]);
    assert_eq!(def.colors, &[ManaColor::B]);
    assert_eq!(def.mana_value, 2);
    assert_eq!(def.cost.generic, 1);
    assert_eq!(def.cost.pips, &[Pip::Colored(ManaColor::B)]);
    assert_eq!(def.target_spec, TargetSpec::TargetOpponent);
    assert!(matches!(
        (def.spell_effect)(),
        Some(EffectOp::RevealTargetHandChooseNonlandDiscard { .. })
    ));
    let mut state = ready(PlayerId::P0);
    let source = put(&mut state, PlayerId::P0, "Pilfer");
    state.players[0].mana_pool[5] = 2;
    next(&mut state);
    refuse(&mut state, Action::CastSpell(source));
}

#[test]
fn pilfer_reveals_both_seats_full_hand_and_caster_chooses_nonland_after_restore() {
    for caster in [PlayerId::P0, PlayerId::P1] {
        for choose_creature in [false, true] {
            let mut state = ready(caster);
            let opponent = caster.opponent();
            let creature = put(&mut state, opponent, "Faerie Miscreant");
            let spell = put(&mut state, opponent, "Giant Growth");
            let land = state.players[opponent.index()].hand[0];
            let source = begin(&mut state, caster);
            let d = discard_choice(&mut state);
            assert!(matches!(&d, Decision::ChooseEffectTargets {
                player, min_targets: 1, max_targets: 1, can_finish: false,
                legal_targets, ..
            } if *player == caster && legal_targets == &vec![Target::Object(creature), Target::Object(spell)]));
            refuse(&mut state, Action::FinishEffectSelection);
            refuse(&mut state, Action::ChooseEffectTarget(Target::Object(land)));
            for observer in [PlayerId::P0, PlayerId::P1] {
                let observed = observe_v2(&state, &HarnessSurfaceV2::new(), observer, 0).unwrap();
                let encoded = serde_json::to_string(&observed).unwrap();
                for name in ["Faerie Miscreant", "Giant Growth", "Forest"] {
                    let id = card_id_by_name(name).unwrap();
                    assert!(encoded.contains(&format!("\"card_db_id\":{id}")));
                }
            }
            let mut replay: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            assert_eq!(next(&mut replay), d);
            let chosen = if choose_creature { creature } else { spell };
            let retained = if choose_creature { spell } else { creature };
            for branch in [&mut state, &mut replay] {
                engine::step(branch, Action::ChooseEffectTarget(Target::Object(chosen))).unwrap();
                // Snapshot the authenticated answer frame before it commits.
                let mut answered: GameState =
                    serde_json::from_slice(&serde_json::to_vec(branch).unwrap()).unwrap();
                settle(branch);
                settle(&mut answered);
                assert_eq!(branch.state_hash(), answered.state_hash());
                assert_eq!(branch.objects.get(chosen).zone, Zone::Graveyard);
                assert_eq!(branch.objects.get(retained).zone, Zone::Hand);
                assert_eq!(branch.objects.get(land).zone, Zone::Hand);
                assert_eq!(branch.objects.get(source).zone, Zone::Graveyard);
            }
            assert_eq!(state.state_hash(), replay.state_hash());
        }
    }
}

#[test]
fn pilfer_land_only_and_empty_hands_finish_without_discard_choice() {
    for caster in [PlayerId::P0, PlayerId::P1] {
        for empty in [false, true] {
            let mut state = ready(caster);
            let opponent = caster.opponent();
            if empty {
                for card in state.players[opponent.index()].hand.clone() {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(card, Zone::Graveyard),
                    );
                }
            }
            let hand = state.players[opponent.index()].hand.clone();
            let source = begin(&mut state, caster);
            settle(&mut state);
            assert_eq!(state.players[opponent.index()].hand, hand);
            assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
        }
    }
}

#[test]
fn pilfer_rejects_changed_revealed_hand_and_changed_target_incarnation() {
    let mut state = ready(PlayerId::P0);
    let creature = put(&mut state, PlayerId::P1, "Faerie Miscreant");
    begin(&mut state, PlayerId::P0);
    discard_choice(&mut state);
    let mut changed_hand = state.clone();
    put(&mut changed_hand, PlayerId::P1, "Giant Growth");
    let mut changed_incarnation = state.clone();
    changed_incarnation
        .objects
        .get_mut(creature)
        .zone_change_count += 1;
    for branch in [&mut changed_hand, &mut changed_incarnation] {
        assert!(matches!(
            engine::advance_until_decision(branch),
            Decision::Halted {
                mechanic: UnsupportedMechanic::InvalidEffectContinuation,
                ..
            }
        ));
    }
}
