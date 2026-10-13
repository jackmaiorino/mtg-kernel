use super::*;
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use crate::engine::{self, Action, Decision};
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use crate::mana::ManaColor;
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use crate::policy_observation_v6::tests::put;
use crate::policy_observation_v6::tests::ready_state_for;

#[test]
fn catalog_without_outcome_protection_keeps_state_and_loss_rules_both_seats() {
    if CARD_DEFS
        .iter()
        .any(|def| def.game_outcome_protection_v1.is_some())
    {
        return;
    }
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        let before = serde_json::to_vec(&state).unwrap();
        let hash = state.state_hash();
        assert!(!cannot_lose(&state, actor));
        assert!(!cannot_win(&state, actor));
        let protected = protected_players(&state);
        assert_eq!(protected, [false, false]);
        assert!(!consume_protected_empty_draws(&mut state, protected));
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        assert_eq!(state.state_hash(), hash);
        state.players[actor.index()].life = 0;
        crate::trigger::sba_fixed_point(&mut state);
        assert!(state.players[actor.index()].has_lost);
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn cast_herald(actor: PlayerId) -> (GameState, ObjectId) {
    let mut state = ready_state_for(actor);
    let source = put(&mut state, actor, "Herald of Eternal Dawn", Zone::Hand);
    let def = &CARD_DEFS[usize::from(state.objects.get(source).card_def)];
    assert!(def.is_executable());
    assert_eq!(def.cost.generic, 4);
    assert_eq!(def.power, Some(6));
    assert_eq!(def.toughness, Some(6));
    assert!(def.keywords.contains(crate::card_def::Keywords::FLASH));
    assert!(def.keywords.contains(crate::card_def::Keywords::FLYING));
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 4;
    state.players[actor.index()].mana_pool[ManaColor::W.pool_index()] = 3;
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    engine::step(&mut state, Action::CastSpell(source)).unwrap();
    for _ in 0..12 {
        let decision = engine::advance_until_decision(&mut state);
        if state.objects.get(source).zone == Zone::Battlefield {
            assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
            assert!(active(&state, source));
            return (state, source);
        }
        assert!(
            matches!(decision, Decision::CastSpellOrPass { .. }),
            "{decision:?}"
        );
        engine::step(&mut state, Action::Pass).unwrap();
    }
    panic!("genuine Herald cast did not resolve");
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[ignore = "requires genuine Herald registration and native qualification"]
fn actual_paid_herald_prevents_life_poison_and_empty_draw_losses_with_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for cause in 0..3 {
            let (mut state, source) = cast_herald(actor);
            match cause {
                0 => state.players[actor.index()].life = 0,
                1 => state.players[actor.index()].poison_counters.0 = 10,
                2 => {
                    crate::event::propose_and_commit(
                        &mut state,
                        crate::event::ProposedEvent::draw(actor),
                    );
                    assert!(state.players[actor.index()].drew_from_empty);
                }
                _ => unreachable!(),
            }
            let mut replay: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for branch in [&mut state, &mut replay] {
                crate::trigger::sba_fixed_point(branch);
                assert!(!branch.players[actor.index()].has_lost);
                assert!(cannot_lose(branch, actor));
                assert!(cannot_win(branch, actor.opponent()));
                assert!(!cannot_win(branch, actor));
                assert!(!branch.players[actor.index()].drew_from_empty);
                crate::event::propose_and_commit(
                    branch,
                    crate::event::ProposedEvent::zone_change(source, Zone::Graveyard),
                );
                crate::trigger::sba_fixed_point(branch);
                assert_eq!(branch.players[actor.index()].has_lost, cause != 2);
                assert!(!cannot_lose(branch, actor));
                assert!(!cannot_win(branch, actor.opponent()));
            }
            assert_eq!(state, replay);
        }
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[ignore = "requires genuine Herald registration and native qualification"]
fn actual_herald_control_and_simultaneous_lethal_departure_preserve_loss_scope_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source) = cast_herald(actor);
        // Named physical control prerequisite: keep both battlefield indexes
        // consistent while exercising the live controller query.
        state.players[actor.index()]
            .battlefield
            .retain(|id| *id != source);
        state.players[actor.opponent().index()]
            .battlefield
            .push(source);
        state.objects.get_mut(source).controller = actor.opponent();
        assert!(!cannot_lose(&state, actor));
        assert!(cannot_lose(&state, actor.opponent()));
        state.players[actor.index()].life = 0;
        state.players[actor.opponent().index()].life = 0;
        crate::trigger::sba_fixed_point(&mut state);
        assert!(state.players[actor.index()].has_lost);
        assert!(!state.players[actor.opponent().index()].has_lost);
        state.objects.get_mut(source).damage = 6;
        crate::trigger::sba_fixed_point(&mut state);
        assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
        assert!(state.players[actor.opponent().index()].has_lost);
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[ignore = "requires genuine Herald registration and native qualification"]
fn actual_empty_draw_and_herald_lethal_damage_in_one_sba_pass_do_not_replay_the_prevented_loss() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source) = cast_herald(actor);
        crate::event::propose_and_commit(&mut state, crate::event::ProposedEvent::draw(actor));
        assert!(state.players[actor.index()].drew_from_empty);
        // Named physical lethal-damage prerequisite. Both conditions exist
        // before the first SBA pass, rather than removing the source later.
        state.objects.get_mut(source).damage = 6;
        let mut replay: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        for branch in [&mut state, &mut replay] {
            crate::trigger::sba_fixed_point(branch);
            assert_eq!(branch.objects.get(source).zone, Zone::Graveyard);
            assert!(!branch.players[actor.index()].drew_from_empty);
            assert!(!branch.players[actor.index()].has_lost);
            assert!(!cannot_lose(branch, actor));
            // A new empty draw after the protection ends is a real new loss.
            crate::event::propose_and_commit(branch, crate::event::ProposedEvent::draw(actor));
            crate::trigger::sba_fixed_point(branch);
            assert!(branch.players[actor.index()].has_lost);
        }
        assert_eq!(state, replay);
    }
}
