//! Public declaration history for Raid. V8 and predecessor model contracts
//! retain their existing representation; this transport requires its own
//! model and Human consumers before admitting Raid cards.

use crate::ids::PlayerId;
use crate::policy_observation_v8::{ObservationV8, PolicyObservationExtensionsV8};
use crate::policy_surface_v5::PolicySurfaceV5;
use crate::rl::{
    CardPrivateV1, KnownLibraryCardV4, PlayerSeatV1, PublicObservationProjectionV5, RlContractError,
};
use crate::state::GameState;
use serde::{Deserialize, Serialize};

pub const OBSERVATION_SCHEMA_VERSION_V9: u32 = 9;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyObservationExtensionsV9 {
    pub predecessor: PolicyObservationExtensionsV8,
    /// Absolute P0/P1 order. Only successful nonempty declarations count.
    /// This public fact survives combat cleanup and departing attackers.
    pub attacked_this_turn: [bool; 2],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationV9 {
    pub schema_version: u32,
    pub kernel_version: String,
    pub surface_version: u32,
    pub policy_surface_version: u32,
    pub card_db_hash: u64,
    pub acting_player: PlayerSeatV1,
    pub step_index: u64,
    pub physical_decision_id: u64,
    pub substep_index: u32,
    pub substep_count: u32,
    pub projection: PublicObservationProjectionV5,
    pub own_hand: Vec<CardPrivateV1>,
    pub known_library_cards: [Vec<KnownLibraryCardV4>; 2],
    pub known_hand_cards: [Vec<CardPrivateV1>; 2],
    pub extensions: PolicyObservationExtensionsV9,
    pub visible_projection_hash: u64,
}

impl ObservationV9 {
    fn from_base(state: &GameState, base: ObservationV8) -> Self {
        Self {
            schema_version: OBSERVATION_SCHEMA_VERSION_V9,
            kernel_version: base.kernel_version,
            surface_version: base.surface_version,
            policy_surface_version: base.policy_surface_version,
            card_db_hash: base.card_db_hash,
            acting_player: base.acting_player,
            step_index: base.step_index,
            physical_decision_id: base.physical_decision_id,
            substep_index: base.substep_index,
            substep_count: base.substep_count,
            projection: base.projection,
            own_hand: base.own_hand,
            known_library_cards: base.known_library_cards,
            known_hand_cards: base.known_hand_cards,
            extensions: PolicyObservationExtensionsV9 {
                predecessor: base.extensions,
                attacked_this_turn: [
                    state.player_attacked_this_turn_v1(PlayerId::P0),
                    state.player_attacked_this_turn_v1(PlayerId::P1),
                ],
            },
            visible_projection_hash: 0,
        }
    }
}

/// Explicit artifact transport. Declaration history contains no hidden card,
/// arena identity, source incarnation, event-log dependency or policy label.
#[allow(clippy::too_many_arguments)]
pub fn observe_policy_v9(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step_index: u64,
    physical_decision_id: u64,
    substep_index: u32,
    substep_count: u32,
) -> Result<ObservationV9, RlContractError> {
    let base = crate::rl::observe_policy_v8(
        state,
        surface,
        actor,
        step_index,
        physical_decision_id,
        substep_index,
        substep_count,
    )?;
    let mut observation = ObservationV9::from_base(state, base);
    let mut value = serde_json::to_value(&observation)?;
    value
        .as_object_mut()
        .ok_or_else(|| RlContractError("V9 observation is not an object".into()))?
        .remove("visible_projection_hash");
    observation.visible_projection_hash = crate::rl::stable_hash_json(&value)?;
    Ok(observation)
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests {
    use super::*;
    use crate::engine::{self, Action};
    use crate::policy_observation_v6::tests::{put, ready_state};
    use crate::state::{Step, Zone};

    fn declared_state(actor: PlayerId) -> GameState {
        let mut state = ready_state();
        state.turn = 3;
        state.active_player = actor;
        state.priority_player = actor;
        state.step = Step::DeclareAttackers;
        let attacker = put(&mut state, actor, "Guttersnipe", Zone::Battlefield);
        engine::step(&mut state, Action::DeclareAttackers(vec![attacker])).unwrap();
        // Model the public state after combat has been cleared. The engine
        // history lifecycle itself is checked by the real engine primitives.
        state.step = Step::Main2;
        state.engine.combat = Default::default();
        state
    }

    fn observe(state: &GameState, actor: PlayerId) -> ObservationV9 {
        observe_policy_v9(state, &PolicySurfaceV5::new(), actor, 0, 0, 0, 1).unwrap()
    }

    #[test]
    fn actual_declaration_remains_visible_to_both_seats_after_combat() {
        for declaring in [PlayerId::P0, PlayerId::P1] {
            let state = declared_state(declaring);
            for actor in [PlayerId::P0, PlayerId::P1] {
                let mut absent = state.clone();
                absent.attack_declaration_turn_v1 = None;
                let old = |s| {
                    crate::rl::observe_policy_v8(s, &PolicySurfaceV5::new(), actor, 0, 0, 0, 1)
                        .unwrap()
                };
                assert_eq!(old(&state), old(&absent));
                let current = observe(&state, actor);
                let mut expected = [false; 2];
                expected[declaring.index()] = true;
                assert_eq!(current.extensions.attacked_this_turn, expected);
                assert_eq!(
                    observe(&absent, actor).extensions.attacked_this_turn,
                    [false; 2]
                );
                assert_ne!(
                    current.visible_projection_hash,
                    observe(&absent, actor).visible_projection_hash
                );
                let restored: GameState =
                    serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
                assert_eq!(observe(&restored, actor), current);
            }
        }
    }

    #[test]
    fn stale_turn_history_never_becomes_current_raid_authority() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = declared_state(actor);
            state.turn += 1;
            assert_eq!(
                observe(&state, actor).extensions.attacked_this_turn,
                [false; 2]
            );
            let mut state = declared_state(actor);
            state.active_player = actor.opponent();
            assert_eq!(
                observe(&state, actor).extensions.attacked_this_turn,
                [false; 2]
            );
        }
    }

    #[test]
    fn hidden_card_identity_cannot_change_raid_observation_or_hash() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let base = declared_state(actor);
            let mut first = base.clone();
            let mut second = base;
            // Both spells are registered in the Limited and Standard catalogs.
            put(&mut first, actor.opponent(), "Snap", Zone::Hand);
            put(&mut second, actor.opponent(), "Lightning Bolt", Zone::Hand);
            put(
                &mut first,
                actor.opponent(),
                "Lightning Bolt",
                Zone::Library,
            );
            put(&mut second, actor.opponent(), "Snap", Zone::Library);
            assert_eq!(observe(&first, actor), observe(&second, actor));
        }
    }

    #[test]
    fn explicit_v9_round_trips_and_predecessor_refuses_the_successor_shape() {
        let observation = observe(&declared_state(PlayerId::P0), PlayerId::P0);
        let bytes = serde_json::to_vec(&observation).unwrap();
        assert_eq!(
            serde_json::from_slice::<ObservationV9>(&bytes).unwrap(),
            observation
        );
        assert!(serde_json::from_slice::<ObservationV8>(&bytes).is_err());
        let mut malformed = serde_json::to_value(&observation).unwrap();
        malformed["extensions"]["hidden_history"] = serde_json::json!(1);
        assert!(serde_json::from_value::<ObservationV9>(malformed).is_err());
    }
}
