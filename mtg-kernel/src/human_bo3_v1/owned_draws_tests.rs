//! Ordinary owned draw triggers retain live Human request/apply compatibility.
use super::*;
use crate::card_def::{card_id_by_name, CARD_DEFS};
use crate::engine::{self, Action, Decision};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use crate::rl_session::FastActorResponseV1;
const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;
fn zone(state: &GameState, source: ObjectId) -> Zone { state.objects.get(source).zone }
fn id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name}"))
}

fn game(library: &[&str]) -> GameState {
    let ids: Vec<u16> = library.iter().map(|name| id(name)).collect();
    let mut state = GameState::new_from_libraries(
        &ids,
        &ids,
        |id| CARD_DEFS[id as usize].object_name.into(),
        0x4539_4221_0001,
    );
    state.active_player = P0;
    state.priority_player = P0;
    state.step = Step::Main1;
    state
}

fn plains() -> GameState {
    game(&["Plains"; 20])
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = id(name);
    let object = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.into(),
        owner: player,
        controller: player,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: Vec::new(),
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    let seat = &mut state.players[player.index()];
    match zone {
        Zone::Hand => seat.hand.push(object),
        Zone::Battlefield => seat.battlefield.push(object),
        Zone::Graveyard => seat.graveyard.push(object),
        Zone::Exile => state.exile.push(object),
        Zone::Library => seat.library.insert(0, object),
        _ => panic!("helper zone"),
    }
    object
}

fn pool(state: &mut GameState, player: PlayerId, colored: &[ManaColor], colorless: u8) {
    let pool = &mut state.players[player.index()].mana_pool;
    for color in colored {
        pool[color.pool_index()] += 1;
    }
    pool[ManaColor::C.pool_index()] += colorless;
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(
        !matches!(decision, Decision::Halted { .. }),
        "{decision:?}\nstack: {:#?}\npending triggers: {:#?}",
        state.stack,
        state.engine.pending_triggers
    );
    decision
}

fn act(state: &mut GameState, action: Action) {
    engine::step(state, action.clone()).unwrap_or_else(|error| panic!("{action:?}: {error}"));
}

fn settle(state: &mut GameState) -> Option<Decision> {
    for _ in 0..200 {
        match next(state) {
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return None
            }
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            other => return Some(other),
        }
    }
    panic!("did not settle");
}

fn settled(state: &mut GameState) {
    if let Some(other) = settle(state) {
        panic!("unexpected choice: {other:?}");
    }
}

fn restored(state: &GameState) -> GameState {
    let restored: GameState = serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap();
    assert_eq!(state.state_hash(), restored.state_hash());
    crate::effect::validate_pending_effect_choice(&restored).unwrap();
    restored
}

fn cast(
    state: &mut GameState,
    actor: PlayerId,
    name: &str,
    target: Option<PlayerId>,
    colored: &[ManaColor],
    colorless: u8,
) -> ObjectId {
    let card = put(state, actor, name, Zone::Hand);
    pool(state, actor, colored, colorless);
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { player, castable_spells, .. } if player == actor && castable_spells.contains(&card))
    );
    act(state, Action::CastSpell(card));
    for _ in 0..12 {
        match next(state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                let target = Target::Player(target.expect("printed target"));
                assert!(legal_targets.contains(&target));
                act(state, Action::ChooseTarget(target));
            }
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            Decision::CastSpellOrPass { .. } => {
                assert!(state.engine.pending_cast.is_none());
                assert!(state.stack.iter().any(|row| row.source == card
                    && !row.is_copy
                    && row
                        .v4
                        .source_contract
                        .is_some_and(|source| source.spell_cast_origin.is_some())));
                assert_eq!(zone(state, card), Zone::Stack);
                return card;
            }
            other => panic!("cast {name}: {other:?}"),
        }
    }
    panic!("paid cast did not finish");
}

fn portent(state: &mut GameState, actor: PlayerId) -> ObjectId {
    cast(
        state,
        actor,
        "Portent",
        Some(actor.opponent()),
        &[ManaColor::U],
        0,
    )
}

fn finish_parent(state: &mut GameState, shuffle: bool) {
    for _ in 0..24 {
        match settle(state) {
            None => return,
            Some(Decision::ChooseEffectTargets {
                can_finish: true, ..
            }) => act(state, Action::FinishEffectSelection),
            Some(Decision::ChooseEffectTargets { legal_targets, .. }) => {
                act(state, Action::ChooseEffectTarget(legal_targets[0]))
            }
            Some(Decision::ChooseEffectOption {
                option_count: 2, ..
            }) => act(state, Action::ChooseEffectOption(u16::from(!shuffle))),
            other => panic!("Portent parent: {other:?}"),
        }
    }
    panic!("Portent parent did not finish");
}

fn next_upkeep(state: &mut GameState, caster: PlayerId) {
    for _ in 0..160 {
        let decision = next(state);
        if state.active_player == caster.opponent() && state.step == Step::Upkeep {
            assert!(!state.stack.is_empty() || !state.engine.pending_triggers.is_empty());
            return;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            Decision::DeclareAttackers { .. } => act(state, Action::DeclareAttackers(vec![])),
            Decision::DeclareBlockers { .. } => act(state, Action::DeclareBlockers(vec![])),
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            other => panic!("next upkeep: {other:?}"),
        }
    }
    panic!("next upkeep absent");
}

#[test]
fn paid_portent_and_standstill_ordinary_human_pass_both_seats_restore_and_retry() {
    for actor in [P0, P1] {
        for delayed in [false, true] {
            let mut state = plains();
            state.active_player = actor;
            state.priority_player = actor;
            let source = if delayed {
                let source = portent(&mut state, actor);
                finish_parent(&mut state, false);
                next_upkeep(&mut state, actor);
                source
            } else {
                let source = cast(&mut state, actor, "Standstill", None, &[ManaColor::U], 1);
                settled(&mut state);
                cast(&mut state, actor, "Lightning Bolt", Some(actor.opponent()), &[ManaColor::R], 0);
                source
            };
            assert!(state.stack.iter().any(|row| row.source == source && row.kind == crate::state::StackItemKind::TriggeredAbility));
            let mut replay = None;
            for state in [state.clone(), restored(&state)] {
                let mut session = FastActorSessionV1::from_v3_fixture_state(state);
                let expected = match session.current_response() {
                    FastActorResponseV1::Decision(decision) => decision,
                    other => panic!("ordinary owned trigger: {other:?}"),
                };
                let mut projector = HumanDecisionProjectorV1::new_v4(expected.acting_player);
                let before = session.privileged_core_environment_hash();
                let visible = projector.project_current(&session, expected).unwrap();
                assert_eq!(visible, projector.project_current(&session, expected).unwrap());
                assert_eq!(before, session.privileged_core_environment_hash());
                let pass = visible.actions.iter().find(|action| action.label == "Pass priority").unwrap_or_else(|| panic!("{visible:?}"));
                let request = HumanActionRequestV1 { prompt_seq: visible.prompt_seq, action_index: pass.action_index };
                let receipt = projector.submit(&mut session, request.clone()).unwrap();
                let after = session.privileged_core_environment_hash();
                assert_eq!(projector.submit(&mut session, request).unwrap(), receipt);
                assert_eq!(after, session.privileged_core_environment_hash());
                let result = (serde_json::to_vec(&visible).unwrap(), serde_json::to_vec(&receipt).unwrap(), after);
                if let Some(previous) = &replay { assert_eq!(&result, previous); }
                replay = Some(result);
            }
        }
    }
}
