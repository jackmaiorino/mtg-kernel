//! Prospective Full-reference source coverage, entirely uncompiled/unrun.
//! Only public reset and issued actions mutate the three independent owners.
use super::*;
use crate::human_bo3_v1::prepared_v57::{HumanPreparedProjectorV57, HumanPreparedSessionV57};
use crate::human_bo3_v1::HumanActionRequestV1;
use crate::rl::ActionSemanticV1 as A;

const REFERENCES: [(u16, &str); 7] = [
    (
        444,
        include_str!("../../../../data/limited/fdn_v1/FDN_reference_elenda_partial.dck"),
    ),
    (
        445,
        include_str!("../../../../data/limited/fdn_v1/FDN_reference_flamewake_phoenix_partial.dck"),
    ),
    (
        446,
        include_str!("../../../../data/limited/fdn_v1/FDN_reference_spinner_of_souls_partial.dck"),
    ),
    (
        447,
        include_str!("../../../../data/limited/fdn_v1/FDN_reference_painful_quandary_partial.dck"),
    ),
    (
        448,
        include_str!("../../../../data/limited/fdn_v1/FDN_reference_drake_hatcher_partial.dck"),
    ),
    (
        450,
        include_str!(
            "../../../../data/limited/fdn_v1/FDN_reference_bloodthirsty_conqueror_partial.dck"
        ),
    ),
    (
        451,
        include_str!("../../../../data/limited/fdn_v1/FDN_reference_doubling_season_partial.dck"),
    ),
];

fn cards(text: &str) -> (Vec<u16>, Value) {
    let mut ids = Vec::new();
    let mut rows = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let (count, name) = line.split_once(' ').unwrap();
        let count: usize = count.parse().unwrap();
        let id = crate::card_def::card_id_by_name(name).unwrap();
        assert!(CARD_DEFS[usize::from(id)].has_full_support());
        ids.extend(std::iter::repeat_n(id, count));
        rows.push(json!({"name": name, "count": count}));
    }
    assert_eq!(ids.len(), 40);
    (ids, json!({"cards": rows}))
}

fn reset(episode: u64, actor: PlayerId, ids: &[u16]) -> PreparedSessionV57 {
    // Same actual deck identity derivation as the schema-5 reset implementation.
    let mut digest = Sha256::new();
    digest.update(b"kernel_limited_common_deck/v1");
    digest.update(KERNEL_CARDDB_HASH.to_le_bytes());
    for id in ids {
        digest.update(id.to_le_bytes());
    }
    let identity = format!("limited-common-v1:{:x}", digest.finalize());
    PreparedSessionV57::reset(
        episode,
        123,
        (10000, 10000),
        [identity.clone(), identity],
        [ids.to_vec(), ids.to_vec()],
        actor,
    )
    .unwrap()
}

fn choose(actions: &[A], printing: u16, actor: PlayerId) -> u32 {
    let seat = PlayerSeatV1::from(actor);
    let index = actions.iter().position(|a| matches!(a, A::ChooseLondonMulligan { mulligan: false, .. }))
        .or_else(|| actions.iter().position(|a| matches!(a, A::CastSpell { actor, source } if *actor == seat && source.card_db_id == printing)))
        .or_else(|| actions.iter().position(|a| matches!(a, A::PlayLand { .. })))
        .or_else(|| actions.iter().position(|a| matches!(a, A::DeclareAttackers { attackers, .. } if attackers.is_empty())))
        .or_else(|| actions.iter().position(|a| matches!(a, A::DeclareBlockersForAttacker { blockers, .. } if blockers.is_empty())))
        .or_else(|| actions.iter().position(|a| matches!(a, A::ChooseAttackerInclusion { include: false, .. })))
        .or_else(|| actions.iter().position(|a| matches!(a, A::Pass { .. })))
        // Mandatory choices use an actual offered candidate. This never creates
        // a Pass, hand, payment, target or trigger outside the issued menu.
        .unwrap_or(0);
    assert!(!actions.is_empty());
    u32::try_from(index).unwrap()
}

fn advance_all(
    core: &mut PreparedSessionV57,
    human: &mut HumanPreparedSessionV57,
    projector: &mut HumanPreparedProjectorV57,
    wire: &mut Wire,
    human_seat: PlayerId,
    index: u32,
) -> Option<HumanActionRequestV1> {
    let input = core.decision_v57().unwrap();
    core.validate_decision_v57(&input).unwrap();
    let actor = input.expected().acting_player;
    let (safe, indexes) =
        crate::human_bo3_v1::project_limited_prepared_v5(&input, actor, 1).unwrap();
    assert_eq!(
        wire.menu()["actions"],
        serde_json::to_value(&safe.actions).unwrap()
    );
    let position = indexes
        .iter()
        .position(|original| *original == index)
        .unwrap();
    let mut request = wire.request(|label| label == safe.actions[position].label);
    // Labels can repeat. The issued safe action index is the sole wire operand.
    request["action_index"] = json!(safe.actions[position].action_index);
    let human_request = if actor == PlayerSeatV1::from(human_seat) {
        let visible = projector.project_current(human, input.expected()).unwrap();
        assert_eq!(visible.actions, safe.actions);
        let answer = HumanActionRequestV1 {
            prompt_seq: visible.prompt_seq,
            action_index: visible.actions[position].action_index,
        };
        projector.submit(human, answer.clone()).unwrap();
        Some(answer)
    } else {
        human
            .step_opponent_with(input.expected(), |observation, actions| {
                assert_eq!(observation, input.observation());
                assert_eq!(actions, input.actions());
                Ok(index)
            })
            .unwrap();
        None
    };
    let expected = core.step_v57(&input, index).unwrap();
    assert_eq!(human.current_response(), expected);
    let reply = wire.send(request);
    assert_eq!(reply["response_type"], "state", "{reply}");
    assert!(reply["receipt"].is_object());
    assert_eq!(
        wire.bytes(),
        serde_json::to_vec(core.fixture_state_v57()).unwrap()
    );
    assert_eq!(
        wire.server
            .active
            .as_ref()
            .unwrap()
            .core
            .fixture_counts_v57(),
        core.fixture_counts_v57()
    );
    human_request
}

#[test]
fn seven_actual_reference_full_resets_paid_casts_human_wire_and_owning_replays_both_seats() {
    for (printing, text) in REFERENCES {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (ids, deck) = cards(text);
            assert_eq!(ids.iter().filter(|id| **id == printing).count(), 4);
            let episode = 57000 + u64::from(printing) * 2 + actor.index() as u64;
            let mut core = reset(episode, actor, &ids);
            let mut human =
                HumanPreparedSessionV57::from_session(reset(episode, actor, &ids), actor.into());
            let mut projector = HumanPreparedProjectorV57::new(actor.into());
            let mut wire = Wire::new();
            let reply = wire.send(json!({"request_type":"reset", "episode_id":episode,
                "env_seed":123, "starting_player":PlayerSeatV1::from(actor),
                "expected_card_db_hash":KERNEL_CARDDB_HASH, "decks":[deck.clone(),deck],
                "max_physical_decisions":10000,"max_policy_steps":10000}));
            assert_eq!(reply["response_type"], "state", "{reply}");
            assert_eq!(
                wire.bytes(),
                serde_json::to_vec(core.fixture_state_v57()).unwrap()
            );
            let before = serde_json::to_vec(core.fixture_state_v57()).unwrap();
            assert!(core
                .restore_v57(&reset(episode, actor, &ids).snapshot_v57())
                .is_err());
            assert_eq!(
                serde_json::to_vec(core.fixture_state_v57()).unwrap(),
                before
            );
            let foreign =
                HumanPreparedSessionV57::from_session(reset(episode, actor, &ids), actor.into());
            assert!(human.restore_v57(&foreign.snapshot_v57()).is_err());

            let mut checkpoint = None;
            for _ in 0..2000 {
                let input = core.decision_v57().unwrap();
                let index = choose(input.actions(), printing, actor);
                let casting = matches!(&input.actions()[index as usize], A::CastSpell { source, .. } if source.card_db_id == printing);
                let human_request = advance_all(
                    &mut core,
                    &mut human,
                    &mut projector,
                    &mut wire,
                    actor,
                    index,
                );
                if casting {
                    assert!(core.fixture_state_v57().engine.pending_cast.is_none());
                    assert!(core.fixture_state_v57().stack.iter().any(|item| core
                        .fixture_state_v57()
                        .objects
                        .get(item.source)
                        .card_def
                        == printing
                        && item.controller == actor));
                    checkpoint = Some((
                        core.snapshot_v57(),
                        human.snapshot_v57(),
                        wire.snapshot(),
                        core.decision_v57().unwrap(),
                        human_request.unwrap(),
                    ));
                    break;
                }
            }
            let (core_saved, human_saved, wire_saved, stale_core, stale_human) =
                checkpoint.expect("actual reference draw/land/cast path must reach its printing");
            let mut replay = Vec::new();
            for _ in 0..32 {
                let input = core.decision_v57().unwrap();
                let index = choose(input.actions(), printing, actor);
                advance_all(
                    &mut core,
                    &mut human,
                    &mut projector,
                    &mut wire,
                    actor,
                    index,
                );
                replay.push(index);
                if core.fixture_state_v57().players[actor.index()]
                    .battlefield
                    .iter()
                    .any(|id| core.fixture_state_v57().objects.get(*id).card_def == printing)
                {
                    break;
                }
            }
            assert!(core.fixture_state_v57().players[actor.index()]
                .battlefield
                .iter()
                .any(|id| core.fixture_state_v57().objects.get(*id).card_def == printing));
            // Continue real gameplay beyond entry, rather than checking reset alone.
            for _ in 0..4 {
                let input = core.decision_v57().unwrap();
                let index = choose(input.actions(), printing, actor);
                advance_all(
                    &mut core,
                    &mut human,
                    &mut projector,
                    &mut wire,
                    actor,
                    index,
                );
                replay.push(index);
            }
            let before = serde_json::to_vec(core.fixture_state_v57()).unwrap();
            match printing {
                444 | 448 | 450 | 451 => {
                    assert!(crate::rl::observe_v1(core.fixture_state_v57(), actor, 0).is_err());
                }
                446 => {
                    assert!(std::panic::catch_unwind(|| {
                        crate::rules_vector_v1::sources::card_rules(printing)
                    })
                    .is_err());
                }
                445 | 447 => {
                    assert!(!crate::rules_vector_v1::sources::card_rules(printing)
                        .opaque_rules
                        .is_empty());
                }
                _ => unreachable!(),
            }
            assert_eq!(
                serde_json::to_vec(core.fixture_state_v57()).unwrap(),
                before
            );
            let counts = core.fixture_counts_v57();
            let observation = core.decision_v57().unwrap().observation().clone();
            let revision = core.decision_v57().unwrap().expected().environment_revision;
            core.restore_v57(&core_saved).unwrap();
            human.restore_v57(&human_saved).unwrap();
            wire.restore(wire_saved);
            assert!(core.validate_decision_v57(&stale_core).is_err());
            assert!(projector.submit(&mut human, stale_human).is_err());
            assert!(core.decision_v57().unwrap().expected().environment_revision > revision);
            for index in replay {
                advance_all(
                    &mut core,
                    &mut human,
                    &mut projector,
                    &mut wire,
                    actor,
                    index,
                );
            }
            assert_eq!(
                serde_json::to_vec(core.fixture_state_v57()).unwrap(),
                before
            );
            assert_eq!(core.fixture_counts_v57(), counts);
            assert_eq!(core.decision_v57().unwrap().observation(), &observation);
        }
    }
}
