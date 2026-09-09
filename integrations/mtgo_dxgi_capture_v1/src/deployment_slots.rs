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
    D: MtgoDeploymentSlotV1,
    P: MtgoDeploymentSlotV1,
    S: MtgoDeploymentSlotV1,
    U: MtgoDeploymentSlotV1,
    R: MtgoDeploymentSlotV1,
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
        let all_slots_wired = slots
            .iter()
            .zip(expected_kinds.iter())
            .all(|(slot, kind)| slot.kind == *kind && !slot.implementation_id.trim().is_empty());
        let placeholder_slot_count =
            u32::try_from(slots.iter().filter(|slot| slot.is_placeholder).count())
                .unwrap_or(u32::MAX);
        MtgoDeploymentSlotReportV1 {
            schema_version: MTGO_DEPLOYMENT_SLOT_REPORT_SCHEMA_V1,
            purpose: "non_actuating_deployment_slot_report_v1".to_owned(),
            slots,
            all_slots_wired,
            any_placeholder_active: placeholder_slot_count > 0,
            placeholder_slot_count,
            grants_live_authority: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Stub(&'static str, bool);

    impl MtgoDeploymentSlotV1 for Stub {
        fn slot_descriptor_v1(
            &self,
            kind: MtgoDeploymentSlotKindV1,
        ) -> MtgoDeploymentSlotDescriptorV1 {
            MtgoDeploymentSlotDescriptorV1 {
                kind,
                implementation_id: self.0.to_owned(),
                is_placeholder: self.1,
                qualified_for_live: false,
                contract_version: 1,
            }
        }
    }

    #[test]
    fn slot_report_names_every_slot_once_and_never_grants_live_authority() {
        let slots = MtgoDeploymentSlotsV1 {
            duel_scorer: Stub("duel", true),
            pregame_controller: Stub("pregame", true),
            sideboard_controller: Stub("sideboard", false),
            unknown_card_policy: Stub("unknown", true),
            search_root_provider: Stub("search", true),
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
        assert!(!report.grants_live_authority);
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"grants_live_authority\":false"));
        assert!(json.contains("\"kind\":\"pregame_controller\""));
    }
}
