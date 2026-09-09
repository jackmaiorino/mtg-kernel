//! Deployment plug-in slots for the MTGO League operator.
//!
//! One registry names every decision surface the deployed agent needs and
//! reports, per slot, which implementation is bound and whether it is a
//! placeholder. Placeholders are deterministic stand-ins that keep the wiring
//! complete; they never grant live authority. A trained head, an expanded
//! catalog, or the ratified search root replaces a placeholder by implementing
//! the same slot trait.

use serde::{Deserialize, Serialize};

pub const MTGO_DEPLOYMENT_SLOT_REPORT_SCHEMA_V1: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoDeploymentSlotKindV1 {
    DuelScorer,
    PregameController,
    SideboardController,
    UnknownCardPolicy,
    SearchRootProvider,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoDeploymentSlotDescriptorV1 {
    pub kind: MtgoDeploymentSlotKindV1,
    pub implementation_id: String,
    pub is_placeholder: bool,
    pub qualified_for_live: bool,
    pub contract_version: u32,
}

/// Every slot implementation describes itself. The registry passes the slot
/// kind so one type can serve more than one slot without lying about which
/// slot it fills.
pub trait MtgoDeploymentSlotV1 {
    fn slot_descriptor_v1(&self, kind: MtgoDeploymentSlotKindV1) -> MtgoDeploymentSlotDescriptorV1;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoDeploymentSlotReportV1 {
    pub schema_version: u32,
    pub purpose: String,
    pub slots: Vec<MtgoDeploymentSlotDescriptorV1>,
    pub all_slots_wired: bool,
    pub any_placeholder_active: bool,
    pub placeholder_slot_count: u32,
    pub all_slots_qualified_for_live: bool,
    pub grants_live_authority: bool,
}

pub struct MtgoDeploymentSlotsV1<D, P, S, U, R> {
    pub duel_scorer: D,
    pub pregame_controller: P,
    pub sideboard_controller: S,
    pub unknown_card_policy: U,
    pub search_root_provider: R,
}

impl<D, P, S, U, R> MtgoDeploymentSlotsV1<D, P, S, U, R>
where
    D: mtgo_blackbox_v1::MtgoPlayerVisibleDuelScorerV1 + MtgoDeploymentSlotV1,
    P: crate::MtgoCompetitiveNativePregameScorerV1 + MtgoDeploymentSlotV1,
    S: crate::MtgoCompetitiveNativeSideboardScorerV1
        + crate::MtgoCompetitiveNativeSideboardDeliberationScorerV1
        + crate::MtgoCompetitiveExternalCompletedMatchHistoryConsumerV1<Output = ()>
        + MtgoDeploymentSlotV1,
    U: MtgoDeploymentSlotV1,
    R: crate::MtgoSearchRootProviderV1 + MtgoDeploymentSlotV1,
{
    pub fn slot_report_v1(&self) -> MtgoDeploymentSlotReportV1 {
        let slots = vec![
            self.duel_scorer
                .slot_descriptor_v1(MtgoDeploymentSlotKindV1::DuelScorer),
            self.pregame_controller
                .slot_descriptor_v1(MtgoDeploymentSlotKindV1::PregameController),
            self.sideboard_controller
                .slot_descriptor_v1(MtgoDeploymentSlotKindV1::SideboardController),
            self.unknown_card_policy
                .slot_descriptor_v1(MtgoDeploymentSlotKindV1::UnknownCardPolicy),
            self.search_root_provider
                .slot_descriptor_v1(MtgoDeploymentSlotKindV1::SearchRootProvider),
        ];
        let expected_kinds = [
            MtgoDeploymentSlotKindV1::DuelScorer,
            MtgoDeploymentSlotKindV1::PregameController,
            MtgoDeploymentSlotKindV1::SideboardController,
            MtgoDeploymentSlotKindV1::UnknownCardPolicy,
            MtgoDeploymentSlotKindV1::SearchRootProvider,
        ];
        // A placeholder claiming live qualification contradicts the slot
        // invariant (no placeholder ever qualifies for live use), so it fails
        // the wiring check outright rather than merely failing to qualify.
        let has_placeholder_claiming_qualified = slots
            .iter()
            .any(|slot| slot.is_placeholder && slot.qualified_for_live);
        let all_slots_wired =
            slots.iter().zip(expected_kinds.iter()).all(|(slot, kind)| {
                slot.kind == *kind && !slot.implementation_id.trim().is_empty()
            }) && !has_placeholder_claiming_qualified;
        let placeholder_slot_count =
            u32::try_from(slots.iter().filter(|slot| slot.is_placeholder).count())
                .unwrap_or(u32::MAX);
        let all_slots_qualified_for_live =
            slots.iter().all(|slot| slot.qualified_for_live) && !has_placeholder_claiming_qualified;
        MtgoDeploymentSlotReportV1 {
            schema_version: MTGO_DEPLOYMENT_SLOT_REPORT_SCHEMA_V1,
            purpose: "non_actuating_deployment_slot_report_v1".to_owned(),
            slots,
            all_slots_wired,
            any_placeholder_active: placeholder_slot_count > 0,
            placeholder_slot_count,
            all_slots_qualified_for_live,
            grants_live_authority: false,
        }
    }
}

pub type MtgoPlaceholderDeploymentSlotsV1 = MtgoDeploymentSlotsV1<
    crate::MtgoPlaceholderVisibleDuelScorerV1,
    crate::MtgoPlaceholderPregameControllerV1,
    crate::MtgoPlaceholderSideboardControllerV1,
    crate::MtgoUnknownCardPolicyV1,
    crate::MtgoRawPolicyOnlySearchRootProviderV1,
>;

/// Builds the all-placeholder deployment for one deck. Every slot is wired;
/// none is qualified for live use.
pub fn build_placeholder_deployment_slots_v1(
    deck: &crate::MtgoCompetitiveNativeSideboardConfigurationV1,
) -> Result<MtgoPlaceholderDeploymentSlotsV1, String> {
    let policy = crate::MtgoUnknownCardPolicyV1::FailClosedHumanTakeover;
    Ok(MtgoDeploymentSlotsV1 {
        duel_scorer: crate::MtgoPlaceholderVisibleDuelScorerV1::new_v1(policy),
        pregame_controller: crate::MtgoPlaceholderPregameControllerV1::new_v1(deck, policy)?,
        sideboard_controller: crate::MtgoPlaceholderSideboardControllerV1,
        unknown_card_policy: policy,
        search_root_provider: crate::MtgoRawPolicyOnlySearchRootProviderV1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtgo_blackbox_v1::{
        MtgoPlayerVisibleDuelDecisionInputV1, MtgoPlayerVisibleDuelScoreResponseV1,
    };

    /// Name, is_placeholder, qualified_for_live.
    struct Stub(&'static str, bool, bool);

    impl MtgoDeploymentSlotV1 for Stub {
        fn slot_descriptor_v1(
            &self,
            kind: MtgoDeploymentSlotKindV1,
        ) -> MtgoDeploymentSlotDescriptorV1 {
            MtgoDeploymentSlotDescriptorV1 {
                kind,
                implementation_id: self.0.to_owned(),
                is_placeholder: self.1,
                qualified_for_live: self.2,
                contract_version: 1,
            }
        }
    }

    impl mtgo_blackbox_v1::MtgoPlayerVisibleDuelScorerV1 for Stub {
        fn score_player_visible_duel_v1(
            &mut self,
            _model_input: &MtgoPlayerVisibleDuelDecisionInputV1,
        ) -> Result<MtgoPlayerVisibleDuelScoreResponseV1, String> {
            Err("stub".to_owned())
        }
    }

    impl crate::MtgoCompetitiveNativePregameScorerV1 for Stub {
        fn score_pregame_v1(
            &mut self,
            _model_input: &crate::MtgoCompetitiveNativePregameModelInputV1,
            _model_input_commitment_sha256: &str,
            _deployment_commitment_sha256: &str,
        ) -> Result<crate::MtgoCompetitiveNativePregameScoreResponseV1, String> {
            Err("stub".to_owned())
        }
    }

    impl crate::MtgoCompetitiveNativeSideboardScorerV1 for Stub {
        fn score_sideboard_v1(
            &mut self,
            _model_input: &crate::MtgoCompetitiveNativeSideboardModelInputV1,
            _model_input_commitment_sha256: &str,
            _deployment_commitment_sha256: &str,
        ) -> Result<crate::MtgoCompetitiveNativeSideboardScoreResponseV1, String> {
            Err("stub".to_owned())
        }
    }

    impl crate::MtgoCompetitiveNativeSideboardDeliberationScorerV1 for Stub {
        fn score_sideboard_deliberation_v1(
            &mut self,
            _decision: &crate::MtgoCompetitiveNativeSideboardDeliberationDecisionV1,
        ) -> Result<crate::MtgoCompetitiveNativeSideboardDeliberationScoreResponseV1, String>
        {
            Err("stub".to_owned())
        }
    }

    impl crate::MtgoSearchRootProviderV1 for Stub {
        fn search_root_v1(
            &self,
            _decision: &MtgoPlayerVisibleDuelDecisionInputV1,
        ) -> crate::MtgoSearchRootDecisionV1 {
            crate::MtgoSearchRootDecisionV1::RawPolicyOnly {
                reason: "stub".to_owned(),
            }
        }
    }

    impl crate::MtgoCompetitiveExternalCompletedMatchHistoryConsumerV1 for Stub {
        type Output = ();

        fn begin_completed_match_history_v1(
            &mut self,
            _header: crate::MtgoCompetitiveExternalCompletedMatchHistoryHeaderV1,
        ) -> Result<(), String> {
            Ok(())
        }

        fn begin_completed_game_v1(
            &mut self,
            _header: crate::MtgoCompetitiveExternalCompletedGameHeaderV1,
        ) -> Result<(), String> {
            Ok(())
        }

        fn consume_confirmed_decision_v1(
            &mut self,
            _decision: crate::MtgoCompetitiveExternalConfirmedDecisionV1,
        ) -> Result<(), String> {
            Ok(())
        }

        fn consume_confirmed_combat_decision_v1(
            &mut self,
            _decision: crate::MtgoCompetitiveExternalConfirmedCombatDecisionV1,
        ) -> Result<(), String> {
            Ok(())
        }

        fn finish_confirmed_decision_stream_v1(&mut self) -> Result<(), String> {
            Ok(())
        }

        fn consume_public_game_log_event_v1(
            &mut self,
            _event: crate::MtgoCompetitiveExternalPublicGameLogEventV1<'_>,
        ) -> Result<(), String> {
            Ok(())
        }

        fn finish_public_game_log_stream_v1(&mut self) -> Result<(), String> {
            Ok(())
        }

        fn finish_completed_game_v1(&mut self) -> Result<(), String> {
            Ok(())
        }

        fn finish_completed_match_history_v1(&mut self) -> Result<Self::Output, String> {
            Ok(())
        }
    }

    #[test]
    fn slot_report_names_every_slot_once_and_never_grants_live_authority() {
        let slots = MtgoDeploymentSlotsV1 {
            duel_scorer: Stub("duel", true, false),
            pregame_controller: Stub("pregame", true, false),
            sideboard_controller: Stub("sideboard", false, false),
            unknown_card_policy: Stub("unknown", true, false),
            search_root_provider: Stub("search", true, false),
        };
        let report = slots.slot_report_v1();
        assert_eq!(report.schema_version, MTGO_DEPLOYMENT_SLOT_REPORT_SCHEMA_V1);
        assert_eq!(report.slots.len(), 5);
        let kinds: Vec<MtgoDeploymentSlotKindV1> =
            report.slots.iter().map(|slot| slot.kind).collect();
        assert_eq!(
            kinds,
            vec![
                MtgoDeploymentSlotKindV1::DuelScorer,
                MtgoDeploymentSlotKindV1::PregameController,
                MtgoDeploymentSlotKindV1::SideboardController,
                MtgoDeploymentSlotKindV1::UnknownCardPolicy,
                MtgoDeploymentSlotKindV1::SearchRootProvider,
            ]
        );
        assert!(report.all_slots_wired);
        assert!(report.any_placeholder_active);
        assert_eq!(report.placeholder_slot_count, 4);
        assert!(!report.all_slots_qualified_for_live);
        assert!(!report.grants_live_authority);
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"grants_live_authority\":false"));
        assert!(json.contains("\"kind\":\"pregame_controller\""));
    }

    #[test]
    fn a_placeholder_claiming_live_qualification_fails_wiring() {
        let slots = MtgoDeploymentSlotsV1 {
            duel_scorer: Stub("duel", true, true),
            pregame_controller: Stub("pregame", false, true),
            sideboard_controller: Stub("sideboard", false, true),
            unknown_card_policy: Stub("unknown", false, true),
            search_root_provider: Stub("search", false, true),
        };
        let report = slots.slot_report_v1();
        assert!(!report.all_slots_wired);
        assert!(!report.all_slots_qualified_for_live);
        assert!(!report.grants_live_authority);
    }
}
