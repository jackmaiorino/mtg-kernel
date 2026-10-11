//! Sealed public Raid successor composed with the unchanged V5 producer.
//! This owns its session and binds the complete V9 public projection before
//! applying an inherited candidate. No model or training runtime is admitted.
use crate::native_flat_tensorizer_v5::{NativeFlatPolicyDecisionV5, NativeFlatPolicyEncoderV5};
use crate::policy_observation_v9::ObservationV9;
use crate::rl::{PlayerSeatV1, RlContractError};
use crate::rl_session::{
    FastActorDecisionV1, FastActorResponseV1, FastActorSessionSnapshotV1, FastActorSessionV1,
    FlatActionDecisionBindingV5,
};
use sha2::{Digest, Sha256};

include!("../../data/flat_policy_v6/feature_identity.rs");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeFeatureIdentityV6 {
    pub schema_version: &'static str,
    pub registry_version: &'static str,
    pub contract_digest: &'static str,
    pub encoding_digest: &'static str,
    pub source_sha256: &'static str,
    pub descriptor_sha256: &'static str,
    pub model_contract_version: &'static str,
}

pub fn feature_identity_v6() -> NativeFeatureIdentityV6 {
    NativeFeatureIdentityV6 {
        schema_version: FEATURE_SCHEMA_VERSION_V6,
        registry_version: FEATURE_REGISTRY_VERSION_V6,
        contract_digest: FEATURE_CONTRACT_DIGEST_V6,
        encoding_digest: FEATURE_ENCODING_DIGEST_V6,
        source_sha256: FEATURES_SOURCE_SHA256_V6,
        descriptor_sha256: FEATURE_DESCRIPTOR_SHA256_V6,
        model_contract_version: MODEL_CONTRACT_VERSION_V6,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlatActionDecisionBindingV6 {
    predecessor: FlatActionDecisionBindingV5,
    public_projection: [u8; 32],
}

fn public_commitment(observation: &ObservationV9) -> Result<[u8; 32], RlContractError> {
    let bytes = serde_json::to_vec(observation)?;
    Ok(Sha256::digest(bytes).into())
}

/// No inner-session accessor or predecessor binding is public. Constructing
/// this type consumes the previous session and selects the successor API.
pub struct RaidFlatSessionV6 {
    inner: FastActorSessionV1,
}

#[derive(Clone)]
pub struct RaidFlatSessionSnapshotV6(FastActorSessionSnapshotV1);

impl RaidFlatSessionV6 {
    pub fn from_session(session: FastActorSessionV1) -> Result<Self, RlContractError> {
        Ok(Self {
            inner: session.into_flat_policy_v5()?,
        })
    }

    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.current_response()
    }

    pub fn snapshot_v6(&self) -> RaidFlatSessionSnapshotV6 {
        RaidFlatSessionSnapshotV6(self.inner.snapshot_v1())
    }

    pub fn restore_v6(&mut self, snapshot: &RaidFlatSessionSnapshotV6) {
        self.inner.restore_v1(&snapshot.0);
    }

    pub fn step_v6(
        &mut self,
        expected: FastActorDecisionV1,
        binding: FlatActionDecisionBindingV6,
        selected_index: u32,
    ) -> Result<FastActorResponseV1, RlContractError> {
        let observation = self
            .inner
            .flat_raid_observation_v9(expected)
            .map_err(|error| RlContractError(format!("flat_v6:{error:?}")))?;
        if public_commitment(&observation)? != binding.public_projection {
            return Err(RlContractError("flat_v6 public binding changed".into()));
        }
        self.inner
            .step_flat_action_v5(expected, binding.predecessor, selected_index)
    }
}

/// Sealed output. Only typed identity, tensor slices and the successor binding
/// are exposed; the rich observation and executable rows remain backend-only.
#[derive(Clone, Debug, PartialEq)]
pub struct NativeFlatPolicyDecisionV6 {
    binding: FlatActionDecisionBindingV6,
    predecessor: NativeFlatPolicyDecisionV5,
    state: Vec<f32>,
}

macro_rules! inherited_tensor {
    ($name:ident, $item:ty) => {
        pub fn $name(&self) -> &[$item] {
            self.predecessor.$name()
        }
    };
}

impl NativeFlatPolicyDecisionV6 {
    pub fn binding(&self) -> FlatActionDecisionBindingV6 {
        self.binding
    }
    pub fn feature_identity(&self) -> NativeFeatureIdentityV6 {
        feature_identity_v6()
    }
    pub fn state(&self) -> &[f32] {
        &self.state
    }
    inherited_tensor!(object_features, f32);
    inherited_tensor!(object_card_ids, i64);
    inherited_tensor!(object_groups, i64);
    inherited_tensor!(object_node_ids, i64);
    inherited_tensor!(edge_features, f32);
    inherited_tensor!(edge_source_indices, i64);
    inherited_tensor!(edge_target_indices, i64);
    inherited_tensor!(action_features, f32);
    inherited_tensor!(action_ref_features, f32);
    inherited_tensor!(action_ref_card_ids, i64);
    inherited_tensor!(action_ref_action_indices, i64);
    inherited_tensor!(action_ref_node_indices, i64);
}

#[derive(Default)]
pub struct NativeFlatPolicyEncoderV6 {
    predecessor: NativeFlatPolicyEncoderV5,
}

impl NativeFlatPolicyEncoderV6 {
    pub fn encode(
        &mut self,
        session: &RaidFlatSessionV6,
        expected: FastActorDecisionV1,
    ) -> Result<NativeFlatPolicyDecisionV6, RlContractError> {
        let observation = session
            .inner
            .flat_raid_observation_v9(expected)
            .map_err(|error| RlContractError(format!("flat_v6:{error:?}")))?;
        let predecessor = self
            .predecessor
            .encode(&session.inner, expected)
            .map_err(|error| RlContractError(format!("native_v6:{error}")))?;
        let binding = FlatActionDecisionBindingV6 {
            predecessor: predecessor.binding(),
            public_projection: public_commitment(&observation)?,
        };
        let flags = observation.extensions.attacked_this_turn;
        let actor = usize::from(observation.acting_player == PlayerSeatV1::P1);
        let mut state = predecessor.state().to_vec();
        state.extend([f32::from(flags[actor]), f32::from(flags[1 - actor])]);
        Ok(NativeFlatPolicyDecisionV6 {
            binding,
            predecessor,
            state,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{self, Action};
    use crate::ids::PlayerId;
    use crate::policy_observation_v6::tests::{put, ready_state};
    use crate::state::{GameState, Step, Zone};

    fn declared(actor: PlayerId) -> GameState {
        let mut state = ready_state();
        state.turn = 3;
        state.active_player = actor;
        state.priority_player = actor;
        state.step = Step::DeclareAttackers;
        let attacker = put(&mut state, actor, "Guttersnipe", Zone::Battlefield);
        engine::step(&mut state, Action::DeclareAttackers(vec![attacker])).unwrap();
        state.step = Step::Main2;
        state.engine.combat = Default::default();
        put(&mut state, actor, "Mountain", Zone::Hand);
        state
    }

    fn expected(session: &RaidFlatSessionV6) -> FastActorDecisionV1 {
        let FastActorResponseV1::Decision(value) = session.current_response() else {
            panic!("fixture is not a live decision");
        };
        value
    }

    #[test]
    fn real_declaration_changes_only_suffix_and_authenticates_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let state = declared(actor);
            let mut absent = state.clone();
            absent.attack_declaration_turn_v1 = None;
            let session =
                RaidFlatSessionV6::from_session(FastActorSessionV1::from_v5_fixture_state(state))
                    .unwrap();
            let absent =
                RaidFlatSessionV6::from_session(FastActorSessionV1::from_v5_fixture_state(absent))
                    .unwrap();
            let mut encoder = NativeFlatPolicyEncoderV6::default();
            let output = encoder.encode(&session, expected(&session)).unwrap();
            let other = encoder.encode(&absent, expected(&absent)).unwrap();
            assert_eq!(output.predecessor, other.predecessor);
            assert_eq!(output.state.len(), output.predecessor.state().len() + 2);
            assert_eq!(&output.state[output.state.len() - 2..], &[1.0, 0.0]);
            assert_eq!(&other.state[other.state.len() - 2..], &[0.0, 0.0]);
            assert_ne!(output.binding(), other.binding());
            assert_ne!(
                output.feature_identity().schema_version,
                output.predecessor.feature_identity().schema_version
            );
        }
    }

    #[test]
    fn forged_history_binding_refuses_without_mutation_and_restores_exactly() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut session = RaidFlatSessionV6::from_session(
                FastActorSessionV1::from_v5_fixture_state(declared(actor)),
            )
            .unwrap();
            let e = expected(&session);
            let mut encoder = NativeFlatPolicyEncoderV6::default();
            let packet = encoder.encode(&session, e).unwrap();
            let snapshot = session.snapshot_v6();
            let before = session.inner.privileged_core_environment_hash();
            let mut forged = packet.binding();
            forged.public_projection[0] ^= 1;
            assert!(session.step_v6(e, forged, 0).is_err());
            assert_eq!(session.inner.privileged_core_environment_hash(), before);
            assert_eq!(encoder.encode(&session, e).unwrap(), packet);
            let response = session.step_v6(e, packet.binding(), 0).unwrap();
            let after = session.inner.privileged_core_environment_hash();
            session.restore_v6(&snapshot);
            assert_eq!(encoder.encode(&session, e).unwrap(), packet);
            assert_eq!(session.step_v6(e, packet.binding(), 0).unwrap(), response);
            assert_eq!(session.inner.privileged_core_environment_hash(), after);
        }
    }

    #[test]
    fn v6_exports_all_tensors_for_real_declarations_and_both_observers() {
        let mut cases = Vec::new();
        for declaring in [PlayerId::P0, PlayerId::P1] {
            for actor in [PlayerId::P0, PlayerId::P1] {
                let mut state = declared(declaring);
                state.priority_player = actor;
                if actor != declaring {
                    put(&mut state, actor, "Lightning Bolt", Zone::Hand);
                    state.players[actor.index()].mana_pool
                        [crate::mana::ManaColor::R.pool_index()] = 1;
                }
                let session = RaidFlatSessionV6::from_session(
                    FastActorSessionV1::from_v5_fixture_state(state),
                )
                .unwrap();
                let e = expected(&session);
                assert_eq!(e.acting_player, PlayerSeatV1::from(actor));
                let observation = session.inner.flat_raid_observation_v9(e).unwrap();
                let (_, semantics) = session.inner.flat_rich_current_input_v5(e).unwrap();
                let actions = semantics
                    .into_iter()
                    .enumerate()
                    .map(|(index, semantic)| {
                        crate::rl::make_legal_action_v5(index as u32, semantic, None).unwrap()
                    })
                    .collect::<Vec<_>>();
                let output = NativeFlatPolicyEncoderV6::default()
                    .encode(&session, e)
                    .unwrap();
                assert_eq!(
                    &output.state()[output.state().len() - 2..],
                    if actor == declaring {
                        &[1.0, 0.0]
                    } else {
                        &[0.0, 1.0]
                    }
                );
                cases.push(serde_json::json!({
                    "name": format!("declared_cleared_p{}_observer_p{}", declaring.index(), actor.index()),
                    "observation": observation, "legal_actions": actions,
                    "tensors": {
                        "state": output.state(), "object_features": output.object_features(),
                        "object_card_ids": output.object_card_ids(), "object_groups": output.object_groups(),
                        "object_node_ids": output.object_node_ids(), "edge_features": output.edge_features(),
                        "edge_source_indices": output.edge_source_indices(), "edge_target_indices": output.edge_target_indices(),
                        "action_features": output.action_features(), "action_ref_features": output.action_ref_features(),
                        "action_ref_card_ids": output.action_ref_card_ids(),
                        "action_ref_action_indices": output.action_ref_action_indices(),
                        "action_ref_node_indices": output.action_ref_node_indices()
                    }
                }));
            }
        }
        if let Ok(path) = std::env::var("MTG_FLAT_V6_PARITY_EXPORT") {
            use std::io::Write;
            let artifact = serde_json::json!({
                "schema": "flat-v6-native-python-parity-1",
                "feature_contract_digest": FEATURE_CONTRACT_DIGEST_V6,
                "feature_encoding_digest": FEATURE_ENCODING_DIGEST_V6,
                "feature_schema_version": FEATURE_SCHEMA_VERSION_V6,
                "feature_registry_version": FEATURE_REGISTRY_VERSION_V6,
                "features_source_sha256": FEATURES_SOURCE_SHA256_V6,
                "feature_descriptor_sha256": FEATURE_DESCRIPTOR_SHA256_V6,
                "model_contract_version": MODEL_CONTRACT_VERSION_V6,
                "cases": cases
            });
            let mut file = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(path)
                .unwrap();
            file.write_all(&serde_json::to_vec(&artifact).unwrap())
                .unwrap();
            file.sync_all().unwrap();
        }
    }
}
