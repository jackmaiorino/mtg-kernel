//! Typed Flat V5 scorer packet for ObservationV8. Shared common row layouts
//! carry data; the V5 binding and feature pins define the separate identity.

use crate::flat_policy_v2::{
    FlatDecisionEncoderV2, FlatDecisionErrorV2, FlatGlobalsV2, FlatRelativePlayerV2,
    FlatScoringDecisionViewV2, FlatScoringOwnedBuffersV2,
};
use crate::flat_policy_v3::{
    FlatDecisionLocalLibraryV3, FlatFinalizedChosenCreatureCostV3, FlatPendingCastObjectCostV3,
    FlatPendingChosenCreatureCostV3, FlatQueuedWardPaymentV3, FlatWardPaymentV3,
};
use crate::flat_policy_v4::FlatHistoricalPublicSourceV4;
use crate::policy_observation_v8::WardLifePrintedFaceV8;
use crate::rl_session::{FastActorDecisionV1, FastActorSessionV1, FlatActionDecisionBindingV5};

pub const FLAT_POLICY_TYPED_LAYOUT_VERSION_V5: u32 = 5;
pub const FLAT_SCORER_PACKET_VERSION_V5: u32 = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlatWardLifePaymentV5 {
    pub targeting_stack_index: u32,
    pub targeting_source_object: u32,
    pub ward_source_object: u32,
    pub payer: FlatRelativePlayerV2,
    pub life: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlatQueuedWardLifePaymentV5 {
    pub stack_index: u32,
    pub payment: FlatWardLifePaymentV5,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlatWardLifeCardTraitV5 {
    /// Exact already-authorized row. A captured source can have several
    /// aliases, including a separate row for each hidden trigger position.
    pub model_object_indices: Vec<u32>,
    pub life: u8,
    pub printed_face: WardLifePrintedFaceV8,
    pub effective: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FlatScoringExtensionsV5 {
    pub appended_object_indices: Vec<u32>,
    pub pending_cast_object_cost: Option<FlatPendingCastObjectCostV3>,
    pub decision_local_library: Option<FlatDecisionLocalLibraryV3>,
    pub historical_public_sources: Vec<FlatHistoricalPublicSourceV4>,
    pub pending_chosen_creature_cost: Option<FlatPendingChosenCreatureCostV3>,
    pub finalized_chosen_creature_costs: Vec<FlatFinalizedChosenCreatureCostV3>,
    pub pending_ward_payment: Option<FlatWardPaymentV3>,
    pub queued_ward_payments: Vec<FlatQueuedWardPaymentV3>,
    pub pending_ward_life_payment: Option<FlatWardLifePaymentV5>,
    pub queued_ward_life_payments: Vec<FlatQueuedWardLifePaymentV5>,
    pub ward_life_card_traits: Vec<FlatWardLifeCardTraitV5>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FlatDecisionV5 {
    pub binding: FlatActionDecisionBindingV5,
    pub globals: FlatGlobalsV2,
    pub extensions: FlatScoringExtensionsV5,
}

#[derive(Clone, Copy)]
pub struct FlatScoringDecisionViewV5<'a> {
    common: FlatScoringDecisionViewV2<'a>,
    extensions: &'a FlatScoringExtensionsV5,
}

impl<'a> FlatScoringDecisionViewV5<'a> {
    pub(crate) fn new(
        common: FlatScoringDecisionViewV2<'a>,
        extensions: &'a FlatScoringExtensionsV5,
    ) -> Self {
        Self { common, extensions }
    }

    pub fn common(self) -> FlatScoringDecisionViewV2<'a> {
        self.common
    }
    pub fn extensions(self) -> &'a FlatScoringExtensionsV5 {
        self.extensions
    }
}

#[derive(Default)]
pub(crate) struct FlatDecisionEncoderV5 {
    pub(crate) common: FlatDecisionEncoderV2,
}

impl FastActorSessionV1 {
    pub(crate) fn encode_current_flat_scoring_decision_owned_v5(
        &self,
        expected: FastActorDecisionV1,
        encoder: &mut FlatDecisionEncoderV5,
        buffers: &mut FlatScoringOwnedBuffersV2<'_>,
    ) -> Result<FlatDecisionV5, FlatDecisionErrorV2> {
        encoder
            .common
            .build_scoring_owned_v5(self, expected, buffers)
    }
}
