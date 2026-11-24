//! The schema 5 public contract's turn schedule
//! (`PublicObservationProjectionV5::turn_schedule`) on hand-built positions:
//! Time Vault's activated ability, whose scheduled turn carries an ability
//! source capture rather than a spell's, and a goad's duration once extra
//! turns exist.

use super::{
    act, activate, cast, islands, pass_until, pass_until_answered, pool, put, settled, P0, P1,
};
use mtg_kernel::engine::{Action, Decision};
use mtg_kernel::ids::ObjectId;
use mtg_kernel::mana::ManaColor;
use mtg_kernel::policy_surface_v5::PolicySurfaceV5;
use mtg_kernel::rl::{enable_public_shroud_v1, observe_policy_v5, ObservationV5, PlayerSeatV1};
use mtg_kernel::state::{GameState, GoadStateV4, Step, Zone};

/// The policy observation of the next decision, under full engine priority
/// so the decision is the very next priority window.
fn observe(state: &mut GameState) -> Result<ObservationV5, String> {
    let mut surface = PolicySurfaceV5::new_with_engine_priority_v1();
    let decision = surface.next_decision(state)?;
    let actor = decision.actor(state).expect("a player decides");
    let (substep_index, substep_count) = decision.substep();
    observe_policy_v5(state, &surface, actor, 0, 0, substep_index, substep_count)
        .map_err(|error| error.to_string())
}

/// The goads published on `creature`, as (player, expires_at_turn).
fn published_goads(observation: &ObservationV5, creature: ObjectId) -> Vec<(PlayerSeatV1, u32)> {
    observation
        .projection
        .surface
        .battlefield
        .iter()
        .flatten()
        .find(|card| card.stable.arena_id == creature.0)
        .expect("the creature is on the battlefield")
        .goaded_by
        .iter()
        .map(|goad| (goad.player, goad.expires_at_turn))
        .collect()
}

#[test]
fn time_vaults_extra_turn_is_on_the_public_schedule() {
    for public in [true, false] {
        let mut state = islands();
        if public {
            enable_public_shroud_v1(&mut state);
        }
        let vault = put(&mut state, P0, "Time Vault", Zone::Hand);
        pool(&mut state, P0, &[], 2);
        cast(&mut state, vault, &[]);
        settled(&mut state);
        // P1's end step: P0 skips their next turn, which untaps the Vault.
        pass_until_answered(&mut state, |state, decision| {
            if !matches!(decision, Decision::ChooseEffectOption { .. }) {
                return false;
            }
            act(state, Action::ChooseEffectOption(0));
            true
        });
        pass_until(&mut state, |state| state.step == Step::Main1);
        assert_eq!(state.active_player, P1, "P0's turn was skipped");
        assert!(!state.objects.get(vault).tapped, "the skip untapped it");
        state.priority_player = P0;
        activate(&mut state, vault, 0, &[]);
        settled(&mut state);
        assert!(state.extra_turn_v1.is_some());

        let observed = observe(&mut state);
        if !public {
            // Every other observation keeps its refusal.
            assert_eq!(
                observed.unwrap_err(),
                "extra turns require their selected public/Human successor"
            );
            continue;
        }
        let observation = observed.unwrap();
        let schedule = observation.projection.turn_schedule.expect("a schedule");
        assert_eq!(schedule.current_extra, None);
        assert_eq!(schedule.next_regular_player, PlayerSeatV1::P0);
        assert_eq!(schedule.queued_next_first.len(), 1);
        let queued = &schedule.queued_next_first[0];
        assert_eq!(queued.creation_ordinal, 0);
        assert_eq!(queued.captured_controller, PlayerSeatV1::P0);
        assert_eq!(queued.physical_owner, PlayerSeatV1::P0);
        assert!(!queued.spell_copy);
    }
}

#[test]
fn a_goad_is_published_until_its_players_next_turn_once_a_schedule_exists() {
    let mut state = islands();
    enable_public_shroud_v1(&mut state);
    let dungeoneer = put(&mut state, P1, "Seasoned Dungeoneer", Zone::Battlefield);
    // P0 goads it during P0's own turn: "until your next turn", stored as the
    // next round.
    let stored = state.turn + 1;
    state.objects.get_mut(dungeoneer).v4.goaded_by = vec![GoadStateV4 {
        player: P0,
        expires_at_turn: stored,
    }];

    // With no schedule the stored round is published, as before.
    let before = observe(&mut state).unwrap();
    assert!(before.projection.turn_schedule.is_none());
    assert_eq!(
        published_goads(&before, dungeoneer),
        vec![(PlayerSeatV1::P0, stored)]
    );

    // P0 takes an extra turn. P0's next turn is now that extra turn, in the
    // same round, so the stored round names no boundary and is not published.
    let walk = put(&mut state, P0, "Time Walk", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, walk, &[]);
    settled(&mut state);
    let scheduled = observe(&mut state).unwrap();
    assert!(scheduled.projection.turn_schedule.is_some());
    assert_eq!(
        published_goads(&scheduled, dungeoneer),
        vec![(PlayerSeatV1::P0, 0)]
    );
    assert_eq!(
        state.objects.get(dungeoneer).v4.goaded_by,
        vec![GoadStateV4 {
            player: P0,
            expires_at_turn: stored
        }],
        "the engine's record is unchanged"
    );

    // The goad ends as P0's extra turn begins, in the round it was made.
    let round = state.turn;
    pass_until(&mut state, |state| {
        state.step == Step::Main1
            && state
                .extra_turn_v1
                .as_ref()
                .is_some_and(|turn| turn.current_extra.is_some())
    });
    assert_eq!(state.active_player, P0);
    assert_eq!(state.turn, round, "an extra turn keeps the round");
    assert!(state.objects.get(dungeoneer).v4.goaded_by.is_empty());
    let extra = observe(&mut state).unwrap();
    assert!(published_goads(&extra, dungeoneer).is_empty());
}
