//! Issue 454 game 39 arity rule and the game 56 witness, through the probe's
//! own owner, encode, rule and apply paths.
use super::*;
use crate::{
    ids::ObjectId,
    rl::CardStableRefV1,
    rl_session::{kaito_v75::ProbeScanAnswersV1, FastActorDecisionKindV1 as Kind},
    state::{GameState, Step, Zone},
};

fn card(arena_id: u32, owner: PlayerSeatV1) -> CardStableRefV1 {
    CardStableRefV1 {
        arena_id,
        card_db_id: 0,
        owner,
        controller: owner,
        zone: Zone::Battlefield,
        zone_change_count: 0,
    }
}
fn block(include: bool) -> ActionSemanticV1 {
    ActionSemanticV1::ChooseBlockerInclusion {
        actor: PlayerSeatV1::P1,
        attacker: card(1, PlayerSeatV1::P0),
        blocker: card(2, PlayerSeatV1::P1),
        include,
    }
}
fn header(kind: Kind, legal_action_count: u32) -> FastActorDecisionV1 {
    FastActorDecisionV1 {
        episode_id: 1,
        step: 7,
        environment_revision: 0,
        physical_decision_id: 7,
        substep_index: 1,
        substep_count: 2,
        acting_player: PlayerSeatV1::P1,
        decision_kind: kind,
        legal_action_count,
    }
}
fn scan(original: Vec<ActionSemanticV1>, feasible: [bool; 2]) -> ProbeScanAnswersV1 {
    ProbeScanAnswersV1 { original, feasible }
}

#[test]
fn issue454_scan_arity_rule_accepts_only_exactly_the_recomputed_infeasible_answers() {
    let native = vec![block(false), block(true)];
    let blocker = header(Kind::BlockerInclusion, 2);
    // Equal arity needs no recomputation and drops nothing.
    assert_eq!(dropped_infeasible_scan_answers(&blocker, &native, None), Ok(0));
    // The one accepted case: the dropped original is exactly the infeasible answer.
    let infeasible_include = scan(native.clone(), [true, false]);
    assert_eq!(
        dropped_infeasible_scan_answers(&blocker, &[block(false)], Some(&infeasible_include)),
        Ok(1)
    );
    let infeasible_decline = scan(native.clone(), [false, true]);
    assert_eq!(
        dropped_infeasible_scan_answers(&blocker, &[block(true)], Some(&infeasible_decline)),
        Ok(1)
    );
    let stops = [
        // A feasible answer was dropped.
        (blocker, vec![block(false)], Some(scan(native.clone(), [true, true]))),
        // The kept answer is itself infeasible.
        (blocker, vec![block(true)], Some(infeasible_include)),
        // No recomputed answers.
        (blocker, vec![block(false)], None),
        // Not an inclusion step.
        (
            header(Kind::Surface, 2),
            vec![block(false)],
            Some(scan(native.clone(), [true, false])),
        ),
        // The step's kind differs from the native answers' family.
        (
            header(Kind::AttackerInclusion, 2),
            vec![block(false)],
            Some(scan(native.clone(), [true, false])),
        ),
        // The native menu differs from the header count.
        (
            header(Kind::BlockerInclusion, 3),
            vec![block(false)],
            Some(scan(native.clone(), [true, false])),
        ),
        // A public action that is not in the native menu.
        (
            blocker,
            vec![ActionSemanticV1::Pass {
                actor: PlayerSeatV1::P1,
            }],
            Some(scan(native.clone(), [true, false])),
        ),
        // A longer public menu and an empty one.
        (
            header(Kind::BlockerInclusion, 1),
            native.clone(),
            Some(scan(vec![block(false)], [true, false])),
        ),
        (blocker, Vec::new(), Some(scan(native.clone(), [true, false]))),
        // Repeated native answers.
        (
            blocker,
            vec![block(false)],
            Some(scan(vec![block(false), block(false)], [true, false])),
        ),
    ];
    for (header, public, answers) in stops {
        let result = dropped_infeasible_scan_answers(&header, &public, answers.as_ref());
        assert!(
            result
                .as_ref()
                .is_err_and(|e| e.starts_with("header/action arity differs")),
            "{public:?}: {result:?}"
        );
    }
}

fn deck(text: &str) -> Vec<u16> {
    let value: serde_json::Value = serde_json::from_str(text).unwrap();
    value["cards"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|row| {
            let id = crate::card_def::card_id_by_name(row["name"].as_str().unwrap()).unwrap();
            std::iter::repeat_n(id, row["count"].as_u64().unwrap() as usize)
        })
        .collect()
}
fn take_to_battlefield(state: &mut GameState, player: PlayerId, name: &str) -> ObjectId {
    let card = crate::card_def::card_id_by_name(name).unwrap();
    let index = state.players[player.index()]
        .library
        .iter()
        .position(|&id| state.objects.get(id).card_def == card)
        .unwrap();
    // Initial scene only: move an existing registered object, never add a card.
    let id = state.players[player.index()].library.remove(index);
    let object = state.objects.get_mut(id);
    object.zone = Zone::Battlefield;
    object.summoning_sick = false;
    state.players[player.index()].battlefield.push(id);
    id
}
/// The original 40-card Krenko pair: Krenko for `player`, two Courageous
/// Goblins for the opponent, in the FullPool mode of the panel.
fn combat_owner(player: PlayerId, edit: impl FnOnce(&mut GameState, ObjectId)) -> Owner {
    let wr = deck(include_str!(
        "../../../data/limited/draftzero_sample_v1/decks/FDN_top_08530_WR.json"
    ));
    let rg = deck(include_str!(
        "../../../data/limited/draftzero_sample_v1/decks/FDN_top_26287_RG.json"
    ));
    let decks = if player == PlayerId::P0 {
        [wr, rg]
    } else {
        [rg, wr]
    };
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &decks[0],
        &decks[1],
        crate::rl::card_name,
        454_00039,
        player,
    );
    state.step = Step::Main1;
    let krenko = take_to_battlefield(&mut state, player, "Krenko, Mob Boss");
    for _ in 0..2 {
        take_to_battlefield(&mut state, player.opponent(), "Courageous Goblin");
    }
    edit(&mut state, krenko);
    crate::rl_session::FastActorSessionV1::enable_limited_native_modes_v1(
        &mut state,
        crate::policy_surface_v5::LimitedAppendixV1::FullPool,
    );
    state.reveal_knowledge_v4 = Some(Default::default());
    state.reveal_meaning_v3.enable();
    Owner::Limited(
        KaitoSession::from_session(
            crate::rl_session::FastActorSessionV1::from_kaito_v75_fixture_state_scoped(
                state,
                crate::policy_surface_v5::LimitedAppendixV1::FullPool,
            ),
        )
        .unwrap(),
    )
}

/// Plays the scene through the probe's encode, rule and apply paths until a
/// filtered inclusion step of `kind`; returns the dropped count of that step.
fn first_filtered_step(mut owner: Owner, kind: Kind) -> u64 {
    for _ in 0..96 {
        let FastActorResponseV1::Decision(header) = owner.response().unwrap() else {
            panic!("scene ended before a filtered {kind:?} step");
        };
        let (decision, capture) = owner.encode(header).unwrap();
        let actions = capture.actions;
        let dropped = owner
            .scan_answers(&decision, &header, actions.len())
            .and_then(|scan| dropped_infeasible_scan_answers(&header, &actions, scan.as_ref()))
            .unwrap();
        if dropped > 0 {
            assert_eq!(header.decision_kind, kind);
            assert_eq!(actions.len() as u64 + dropped, u64::from(header.legal_action_count));
            return dropped;
        }
        let selected = actions
            .iter()
            .position(|action| match action {
                ActionSemanticV1::ChooseAttackerInclusion { include, .. } => *include,
                ActionSemanticV1::ChooseBlockerInclusion { include, .. } => !*include,
                ActionSemanticV1::Pass { .. } => true,
                _ => false,
            })
            .unwrap_or(0);
        owner.apply(&decision, selected as u32).unwrap();
    }
    panic!("no filtered {kind:?} step");
}

#[test]
fn issue454_blocker_step_with_two_blocker_minimum_drops_one_counted_answer_both_seats() {
    for player in [PlayerId::P0, PlayerId::P1] {
        // An explicit native two-blocker requirement, the menace rule. After
        // the first goblin is declined, including the second alone cannot
        // complete a legal block, so the public step omits that answer.
        let owner = combat_owner(player, |state, krenko| {
            state.objects.get_mut(krenko).v4.minimum_blockers_override = Some(2);
        });
        assert_eq!(first_filtered_step(owner, Kind::BlockerInclusion), 1);
    }
}

#[test]
fn issue454_goaded_attacker_step_drops_one_counted_answer_both_seats() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let owner = combat_owner(player, |state, krenko| {
            let turn = state.turn;
            state.objects.get_mut(krenko).v4.goaded_by = vec![crate::state::GoadStateV4 {
                player: player.opponent(),
                expires_at_turn: turn + 1,
            }];
        });
        assert_eq!(first_filtered_step(owner, Kind::AttackerInclusion), 1);
    }
}

#[test]
fn issue454_conqueror_witness_reports_the_first_differing_replayed_event_both_seats() {
    use crate::bloodthirsty_conqueror_v49::tests::future_printed::{finish, paid};
    use crate::event::{CommittedEvent, ProposedEvent};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, _) = paid(actor);
        crate::event::propose_and_commit(
            &mut state,
            ProposedEvent::life_loss(actor.opponent(), 4),
        );
        finish(&mut state);
        let lines = conqueror_witness::witness(&state);
        assert_eq!(
            lines,
            vec!["1 Conqueror gain journals; every replay matches the real history".to_string()]
        );
        let gain = state
            .engine
            .event_history
            .iter()
            .find_map(|event| match event {
                CommittedEvent::ConquerorJournalV49 { journal } => match journal.as_ref() {
                    crate::bloodthirsty_conqueror_v49::custody::JournalV49::Gain {
                        gain_history_index,
                        ..
                    } => Some(*gain_history_index as usize),
                    _ => None,
                },
                _ => None,
            })
            .unwrap();
        let CommittedEvent::LifeGain { amount, .. } = &mut state.engine.event_history[gain] else {
            panic!("actual Conqueror gain occurrence missing");
        };
        *amount += 1;
        let lines = conqueror_witness::witness(&state);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].contains(&format!("first difference at history index {gain}:")),
            "{lines:?}"
        );
    }
}
