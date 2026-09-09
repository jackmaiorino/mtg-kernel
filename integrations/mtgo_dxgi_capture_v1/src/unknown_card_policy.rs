//! Unknown-card policy (spec 6.9). Version 1 fails closed: when a required
//! visible card cannot be resolved against the frozen kernel catalog, the
//! agent submits no default action and requests human takeover. A future
//! qualified unknown-card encoding replaces this policy behind the same slot.

use crate::{MtgoDeploymentSlotDescriptorV1, MtgoDeploymentSlotKindV1, MtgoDeploymentSlotV1};
use mtg_kernel::card_def::CARD_DEFS;
use mtgo_blackbox_v1::resolve_checked_untrusted_kernel_card_correspondence_v1;
use serde::{Deserialize, Serialize};

pub const MTGO_UNKNOWN_CARD_FAIL_CLOSED_REASON_V1: &str =
    "unknown_card_fail_closed_human_takeover_v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoUnknownCardPolicyV1 {
    FailClosedHumanTakeover,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoResolvedVisibleCardV1 {
    pub visible_card_name: String,
    pub card_db_id: u16,
    pub is_land: bool,
    pub mana_value: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoUnknownCardAbstentionV1 {
    pub policy: MtgoUnknownCardPolicyV1,
    pub reason: String,
    /// Sorted and deduplicated so the abstention is deterministic.
    pub unknown_visible_card_names: Vec<String>,
}

impl MtgoUnknownCardPolicyV1 {
    /// Resolves every name in input order. Any unresolved name fails the whole
    /// call; the policy never substitutes a default card.
    pub fn resolve_visible_card_names_v1(
        &self,
        visible_card_names: &[&str],
    ) -> Result<Vec<MtgoResolvedVisibleCardV1>, MtgoUnknownCardAbstentionV1> {
        let mut resolved = Vec::with_capacity(visible_card_names.len());
        let mut unknown = Vec::new();
        for name in visible_card_names {
            match resolve_checked_untrusted_kernel_card_correspondence_v1(name) {
                Ok(correspondence) => {
                    let card_db_id = correspondence.card_db_id();
                    let Some(definition) = CARD_DEFS.get(usize::from(card_db_id)) else {
                        unknown.push((*name).to_owned());
                        continue;
                    };
                    let mana_value = u8::try_from(definition.cost.pips.len())
                        .unwrap_or(u8::MAX)
                        .saturating_add(definition.cost.generic);
                    resolved.push(MtgoResolvedVisibleCardV1 {
                        visible_card_name: (*name).to_owned(),
                        card_db_id,
                        is_land: definition.is_land,
                        mana_value,
                    });
                }
                Err(_) => unknown.push((*name).to_owned()),
            }
        }
        if unknown.is_empty() {
            return Ok(resolved);
        }
        unknown.sort();
        unknown.dedup();
        Err(MtgoUnknownCardAbstentionV1 {
            policy: *self,
            reason: MTGO_UNKNOWN_CARD_FAIL_CLOSED_REASON_V1.to_owned(),
            unknown_visible_card_names: unknown,
        })
    }
}

impl MtgoDeploymentSlotV1 for MtgoUnknownCardPolicyV1 {
    fn slot_descriptor_v1(&self, kind: MtgoDeploymentSlotKindV1) -> MtgoDeploymentSlotDescriptorV1 {
        MtgoDeploymentSlotDescriptorV1 {
            kind,
            implementation_id: "unknown_card_policy_fail_closed_human_takeover_v1".to_owned(),
            is_placeholder: true,
            qualified_for_live: false,
            contract_version: 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_pool_cards_resolve_with_land_and_mana_value_facts() {
        let policy = MtgoUnknownCardPolicyV1::FailClosedHumanTakeover;
        let resolved = policy
            .resolve_visible_card_names_v1(&["Mountain", "Lightning Bolt"])
            .unwrap();
        assert_eq!(resolved.len(), 2);
        let mountain = &resolved[0];
        assert_eq!(mountain.visible_card_name, "Mountain");
        assert!(mountain.is_land);
        assert_eq!(mountain.mana_value, 0);
        let bolt = &resolved[1];
        assert_eq!(bolt.visible_card_name, "Lightning Bolt");
        assert!(!bolt.is_land);
        assert_eq!(bolt.mana_value, 1);
    }

    #[test]
    fn unknown_names_abstain_with_the_fixed_reason_and_list_every_unknown_name() {
        let policy = MtgoUnknownCardPolicyV1::FailClosedHumanTakeover;
        let error = policy
            .resolve_visible_card_names_v1(&["Mountain", "Not A Kernel Card", "Also Unknown"])
            .unwrap_err();
        assert_eq!(error.reason, MTGO_UNKNOWN_CARD_FAIL_CLOSED_REASON_V1);
        assert_eq!(
            error.unknown_visible_card_names,
            vec!["Also Unknown".to_owned(), "Not A Kernel Card".to_owned()]
        );
        assert_eq!(
            error.policy,
            MtgoUnknownCardPolicyV1::FailClosedHumanTakeover
        );
    }

    #[test]
    fn policy_reports_itself_as_a_placeholder_slot() {
        let descriptor = MtgoUnknownCardPolicyV1::FailClosedHumanTakeover
            .slot_descriptor_v1(MtgoDeploymentSlotKindV1::UnknownCardPolicy);
        assert_eq!(
            descriptor.implementation_id,
            "unknown_card_policy_fail_closed_human_takeover_v1"
        );
        assert!(descriptor.is_placeholder);
        assert!(!descriptor.qualified_for_live);
    }
}
