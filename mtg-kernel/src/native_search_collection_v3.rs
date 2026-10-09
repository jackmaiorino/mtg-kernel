//! Opt-in native collection with V4 scorer/action authority and fresh receipts.
//! This API does not feed the frozen V2 trainer or Store pipeline.
use crate::async_flat_scored_rollout_v1::{
    run_async_flat_scored_rollout_core_with_population_v1, AsyncFlatScoredObservedRunErrorV1,
    AsyncFlatScoredRolloutErrorV1, AsyncFlatScoredRolloutResultV1, FlatBatchScorerCore,
    FlatBatchScorerErrorV1, FlatScoredExecutionScheduleV1, FlatScoredFamilyCore,
    FlatScoredSelectedEventCore, FlatScoredSessionEnvironmentV1, FlatScoredTerminalEventV1,
    FlatScoredTrajectoryObserverCore, RoundDecisionCore,
};
use crate::async_rollout_v2::AsyncRolloutConfigV2;
use crate::flat_policy_v2::*;
use crate::flat_policy_v4::{FlatDecisionEncoderV4, FlatDecisionV4};
pub use crate::flat_policy_v4::{
    FlatScoringDecisionViewV4 as NativeSearchDecisionViewV3,
    FlatScoringExtensionsV4 as NativeSearchScoringExtensionsV3,
};
use crate::kernel_native_search_opponent_v1::{
    KernelNativeSearchAuthorityV1, KernelNativeSearchErrorV1, KernelNativeSearchOpponentV1,
};
use crate::native_population_opponent_v1::{
    PopulationOpponentEngineV1, PopulationSlotOccupantV1, PopulationWeightVectorV1,
};
use crate::native_search_trajectory_v3::{
    NativeLaneTrajectoryReceiptV3, NativeSearchTrajectoryReceiptV3,
};
use crate::rl_session::{
    FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1, V4SearchActionTokenV1,
};
use std::sync::Arc;
#[cfg(test)]
mod tests;

pub const NATIVE_SEARCH_COLLECTION_IDENTITY_V3: &str =
    "mtg-kernel-native-search-collection-v4-visible-first/v3";

/// Explicitly constructed fresh eight-slot population. No frozen manifest is
/// silently upgraded, and checkpoint/V1 search occupants cannot enter this API.
#[derive(Clone)]
pub struct NativeSearchPopulationV3(Arc<PopulationOpponentEngineV1>);

impl NativeSearchPopulationV3 {
    pub fn new(
        weights: [u64; 8],
        authorities: [KernelNativeSearchAuthorityV1; 8],
    ) -> Result<Self, KernelNativeSearchErrorV1> {
        let total = weights
            .iter()
            .try_fold(0u64, |sum, weight| sum.checked_add(*weight))
            .ok_or(KernelNativeSearchErrorV1::InvalidAuthority)?;
        let weights = PopulationWeightVectorV1::new_v1(weights, total)
            .map_err(|_| KernelNativeSearchErrorV1::InvalidAuthority)?;
        let mut handles = Vec::with_capacity(8);
        for authority in authorities {
            if !authority.uses_v4_contract_v3() {
                return Err(KernelNativeSearchErrorV1::InvalidAuthority);
            }
            handles.push(PopulationSlotOccupantV1::Search(Arc::new(
                KernelNativeSearchOpponentV1::new(authority)?,
            )));
        }
        let handles = handles
            .try_into()
            .map_err(|_| KernelNativeSearchErrorV1::InvalidAuthority)?;
        Ok(Self(Arc::new(PopulationOpponentEngineV1::new_v1(
            weights, handles,
        ))))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeSearchScorerContractV3 {
    pub identity: &'static str,
    pub scorer_packet_version: u32,
    pub card_db_hash: u64,
}

/// A scorer receives only validated V4 model rows. `common()` is a reused row
/// storage layout; the supplied contract asserts V4 semantics, never V2.
pub trait NativeSearchBatchScorerV3 {
    fn score_batch(
        &mut self,
        contract: NativeSearchScorerContractV3,
        decisions: &[NativeSearchDecisionViewV3<'_>],
        action_offsets: &[usize],
        action_logits: &mut [f32],
        values: &mut [f32],
    ) -> Result<(), FlatBatchScorerErrorV1>;
}

#[derive(Clone, Default)]
pub(crate) struct Packet {
    expected: Option<FastActorDecisionV1>,
    decision: Option<FlatDecisionV4>,
    token: Option<V4SearchActionTokenV1>,
    objects: Vec<FlatObjectCoreV2>,
    relations: Vec<FlatRelationV2>,
    object_subtypes: Vec<FlatObjectSubtypeV2>,
    ability_uses: Vec<FlatObjectAbilityUseV2>,
    goads: Vec<FlatObjectGoadV2>,
    dungeons: Vec<FlatCompletedDungeonV2>,
    changes: Vec<FlatEffectSubtypeChangeV2>,
    paths: Vec<FlatContextPathElementV2>,
    actions: Vec<FlatScorerActionCoreV2>,
    refs: Vec<FlatScorerActionRefV2>,
}

impl Packet {
    fn view(&self) -> NativeSearchDecisionViewV3<'_> {
        let decision = self.decision.as_ref().expect("validated V4 packet");
        NativeSearchDecisionViewV3::new(
            FlatScoringDecisionViewV2::new(
                &decision.globals,
                &self.objects,
                &self.relations,
                &self.object_subtypes,
                &self.ability_uses,
                &self.goads,
                &self.dungeons,
                &self.changes,
                &self.paths,
                &self.actions,
                &self.refs,
            ),
            &decision.extensions,
        )
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Family;

impl FlatScoredFamilyCore for Family {
    type Encoder = FlatDecisionEncoderV4;
    type OwnedPacket = Packet;
    type ValidatedPacket = Packet;
    type Binding = V4SearchActionTokenV1;
    type Contract = NativeSearchScorerContractV3;
    type Decision = FastActorDecisionV1;
    type DecisionView<'a> = NativeSearchDecisionViewV3<'a>;
    const WORKER_NAME: &'static str = "native-search-v3";
    const MEMBERSHIP_DIGEST_DOMAIN: &'static [u8] = b"mtg-kernel/native-search-collection/v3\0";
    const SEARCH_TRAJECTORY_V3: bool = true;

    fn reset_session(
        config: &AsyncRolloutConfigV2,
        episode: u64,
        seed: u64,
        environment: FlatScoredSessionEnvironmentV1,
    ) -> Result<FastActorSessionV1, ()> {
        if environment != FlatScoredSessionEnvironmentV1::Legacy {
            return Err(());
        }
        let session = match config.starting_player {
            Some(player) => FastActorSessionV1::reset_with_decks_and_limits_flat_action_v2_with_starting_player_v1(
                episode, seed, config.max_physical_decisions, config.max_policy_steps, config.deck_ids.clone(), player,
            ),
            None => FastActorSessionV1::reset_with_decks_and_limits_flat_action_v2(
                episode, seed, config.max_physical_decisions, config.max_policy_steps, config.deck_ids.clone(),
            ),
        };
        session
            .map(FastActorSessionV1::into_flat_action_v3)
            .map_err(|_| ())
    }

    fn encode_packet(
        session: &FastActorSessionV1,
        expected: FastActorDecisionV1,
        encoder: &mut Self::Encoder,
        mut packet: Packet,
    ) -> Result<Packet, ()> {
        packet.decision = Some(
            session
                .encode_current_flat_scoring_decision_owned_v4(
                    expected,
                    encoder,
                    &mut FlatScoringOwnedBuffersV2 {
                        objects: &mut packet.objects,
                        relations: &mut packet.relations,
                        object_subtypes: &mut packet.object_subtypes,
                        ability_uses: &mut packet.ability_uses,
                        goads: &mut packet.goads,
                        completed_dungeons: &mut packet.dungeons,
                        effect_subtype_changes: &mut packet.changes,
                        context_path_elements: &mut packet.paths,
                        actions: &mut packet.actions,
                        action_refs: &mut packet.refs,
                    },
                )
                .map_err(|_| ())?,
        );
        packet.expected = Some(expected);
        packet.token = Some(
            session
                .kernel_search_action_token_v4(expected)
                .map_err(|_| ())?,
        );
        Ok(packet)
    }
    fn packet_contract(_: &Packet) -> Self::Contract {
        NativeSearchScorerContractV3 {
            identity: NATIVE_SEARCH_COLLECTION_IDENTITY_V3,
            scorer_packet_version: 4,
            card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        }
    }
    fn packet_binding(packet: &Packet) -> Self::Binding {
        // Packets need the exact V4 token. It is populated during encoding.
        packet.token.expect("validated V4 packet token")
    }
    fn packet_decision(packet: &Packet) -> FastActorDecisionV1 {
        packet.expected.expect("validated packet")
    }
    fn packet_view(packet: &Packet) -> NativeSearchDecisionViewV3<'_> {
        packet.view()
    }
    fn packet_action_count(packet: &Packet) -> u32 {
        packet.actions.len() as u32
    }
    fn into_owned_packet(packet: Packet) -> Packet {
        packet
    }
    fn expected_matches_binding(
        expected: FastActorDecisionV1,
        decision: FastActorDecisionV1,
    ) -> bool {
        expected == decision
    }
    #[cfg(test)]
    fn test_safe_packet_payload(packet: &Packet) -> String {
        format!(
            "{:?}|{:?}|{:?}|{:?}",
            packet.view().common().globals(),
            packet.objects,
            packet.actions,
            packet.refs
        )
    }
    fn consume(
        session: &mut FastActorSessionV1,
        token: Self::Binding,
        index: u32,
    ) -> Result<FastActorResponseV1, ()> {
        let FastActorResponseV1::Decision(expected) = session.current_response() else {
            return Err(());
        };
        session
            .kernel_search_consume_v4(expected, token, index)
            .map_err(|_| ())
    }
    fn native_full_trajectory_commitment(token: Self::Binding) -> Result<[u8; 16], ()> {
        Ok(token.commitment())
    }
    fn native_full_trajectory_opponent_commitment(
        session: &FastActorSessionV1,
        expected: FastActorDecisionV1,
    ) -> Result<[u8; 16], ()> {
        session
            .kernel_search_action_token_v4(expected)
            .map(|token| token.commitment())
            .map_err(|_| ())
    }
}

struct Scorer<'a, S>(&'a mut S);
impl<S: NativeSearchBatchScorerV3> FlatBatchScorerCore<Family> for Scorer<'_, S> {
    fn score_batch_core(
        &mut self,
        contract: NativeSearchScorerContractV3,
        decisions: &[RoundDecisionCore<Family>],
        offsets: &[usize],
        logits: &mut [f32],
        values: &mut [f32],
    ) -> Result<(), FlatBatchScorerErrorV1> {
        let views: Vec<_> = decisions
            .iter()
            .map(|decision| Family::packet_view(&decision.packet))
            .collect();
        self.0
            .score_batch(contract, &views, offsets, logits, values)
    }
}

#[derive(Debug, Clone)]
pub struct NativeSearchCollectedDecisionV3 {
    pub episode_index: u64,
    pub policy_step: u64,
    pub selected_index: u32,
    pub action_seed: u64,
    pub raw_logits: Vec<f32>,
    pub predicted_value_bits: u32,
    pub menu_commitment: [u8; 16],
}

#[derive(Default)]
struct Observer {
    rows: Vec<NativeSearchCollectedDecisionV3>,
    receipts: Vec<NativeSearchTrajectoryReceiptV3>,
}
impl FlatScoredTrajectoryObserverCore<Family> for Observer {
    type Error = ();
    type Output = Self;
    fn observe_selected_core(
        &mut self,
        event: FlatScoredSelectedEventCore<'_, Family>,
    ) -> Result<(), ()> {
        self.rows.push(NativeSearchCollectedDecisionV3 {
            episode_index: event.expected.episode_id,
            policy_step: event.expected.step,
            selected_index: event.selected_index,
            action_seed: event.action_seed,
            raw_logits: event.raw_action_logits.to_vec(),
            predicted_value_bits: event.predicted_value_bits,
            menu_commitment: event.binding.commitment(),
        });
        Ok(())
    }
    fn observe_terminal_core(&mut self, event: FlatScoredTerminalEventV1) -> Result<(), ()> {
        let Some(NativeLaneTrajectoryReceiptV3::Search(receipt)) =
            event.native_full_trajectory_receipt
        else {
            return Err(());
        };
        if receipt.episode_index() != event.terminal.episode_id
            || receipt.policy_step_count() != event.terminal.policy_step_count
            || receipt.physical_decision_count() != event.terminal.physical_decision_count
            || receipt.learner_policy_step_count() != event.learner_action_count
            || self
                .receipts
                .iter()
                .any(|previous| previous.episode_index() == receipt.episode_index())
        {
            return Err(());
        }
        self.receipts.push(receipt);
        Ok(())
    }
    fn finish_core(self) -> Result<Self, ()> {
        Ok(self)
    }
}

pub struct NativeSearchCollectionV3 {
    pub result: AsyncFlatScoredRolloutResultV1,
    pub learner_decisions: Vec<NativeSearchCollectedDecisionV3>,
    pub receipts: Vec<NativeSearchTrajectoryReceiptV3>,
}

pub fn collect_native_search_population_v3(
    config: AsyncRolloutConfigV2,
    base_seed: u64,
    population: NativeSearchPopulationV3,
    scorer: &mut impl NativeSearchBatchScorerV3,
) -> Result<NativeSearchCollectionV3, AsyncFlatScoredRolloutErrorV1> {
    match run_async_flat_scored_rollout_core_with_population_v1::<Family, _, _>(
        config,
        FlatScoredExecutionScheduleV1::NativeTrainerV1 { base_seed },
        None,
        None,
        Some(population.0),
        None,
        &mut Scorer(scorer),
        Observer::default(),
    ) {
        Ok((result, mut observer)) => {
            observer
                .rows
                .sort_by_key(|row| (row.episode_index, row.policy_step));
            observer
                .receipts
                .sort_by_key(NativeSearchTrajectoryReceiptV3::episode_index);
            Ok(NativeSearchCollectionV3 {
                result,
                learner_decisions: observer.rows,
                receipts: observer.receipts,
            })
        }
        Err(AsyncFlatScoredObservedRunErrorV1::Rollout(error)) => Err(error),
        Err(_) => Err(AsyncFlatScoredRolloutErrorV1::BrokerProtocolViolation),
    }
}
