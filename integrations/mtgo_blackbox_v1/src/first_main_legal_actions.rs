use crate::{
    CheckedUntrustedMtgoFirstMainKernelCardCoverageV1, CheckedUntrustedMtgoVisibleObjectLedgerV1,
    MtgoContractErrorV1, MtgoFirstMainKernelCardCoverageDispositionV1,
};
use mtg_kernel::card_def::KERNEL_CARDDB_HASH;
use mtg_kernel::rl::{ActionSemanticV1, PlayerSeatV1};
use mtg_kernel::state::Zone;
use sha2::{Digest, Sha256};

const FIRST_MAIN_LEGAL_ACTIONS_DOMAIN_V1: &[u8] = b"mtgo-first-main-legal-actions-v1";
const FIRST_MAIN_HAND_COUNT_V1: usize = 8;
const FIRST_MAIN_ACTION_COUNT_V1: usize = FIRST_MAIN_HAND_COUNT_V1 + 1;

/// Checked-untrusted ordered gameplay actions for the supported Kernel Basics
/// turn-one first-main shape.
///
/// Every hand object must be an exact Forest or Island deck card in the local
/// P0 hand, bound to the same complete eight-card coverage result that seeded
/// the unchanged object ledger. The action order matches the kernel surface:
/// eight `PlayLand` candidates in object order, followed by `Pass`.
///
/// The eight-card multiplicity is Solitaire-specific. The kernel correctly
/// skips the starting player's first-turn draw in a duel, so its starting
/// player reaches first main with seven cards. This checked value cannot be
/// reused as a two-player starting-first-main legal-action set.
///
/// This type deliberately exposes only counts and commitments. It is not a
/// complete MTGO decision because no two-player `ObservationV5`, current-frame
/// provenance, or visible control set exists yet.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoFirstMainLegalActionsV1;
/// fn actions_cannot_escape(value: &CheckedUntrustedMtgoFirstMainLegalActionsV1) {
///     let _ = value.ordered_legal_actions_v1();
/// }
/// ```
pub struct CheckedUntrustedMtgoFirstMainLegalActionsV1 {
    source_frame_sha256: String,
    coverage_commitment_sha256: String,
    object_ledger_commitment_sha256: String,
    play_land_action_count: u8,
    ordered_legal_actions: Vec<ActionSemanticV1>,
    action_set_commitment_sha256: String,
}

impl CheckedUntrustedMtgoFirstMainLegalActionsV1 {
    pub fn source_frame_sha256(&self) -> &str {
        &self.source_frame_sha256
    }

    pub fn coverage_commitment_sha256(&self) -> &str {
        &self.coverage_commitment_sha256
    }

    pub fn object_ledger_commitment_sha256(&self) -> &str {
        &self.object_ledger_commitment_sha256
    }

    pub fn play_land_action_count(&self) -> u8 {
        self.play_land_action_count
    }

    pub fn action_count(&self) -> usize {
        self.ordered_legal_actions.len()
    }

    pub fn action_set_commitment_sha256(&self) -> &str {
        &self.action_set_commitment_sha256
    }

    pub fn compatible_with_two_player_starting_first_main(&self) -> bool {
        false
    }

    pub fn safe_for_observation_v5(&self) -> bool {
        false
    }

    pub fn safe_for_policy_scoring(&self) -> bool {
        false
    }

    pub fn safe_for_input(&self) -> bool {
        false
    }
}

pub fn derive_checked_untrusted_first_main_legal_actions_v1(
    coverage: &CheckedUntrustedMtgoFirstMainKernelCardCoverageV1,
    ledger: &CheckedUntrustedMtgoVisibleObjectLedgerV1,
) -> Result<CheckedUntrustedMtgoFirstMainLegalActionsV1, MtgoContractErrorV1> {
    if !coverage.coverage_complete()
        || coverage.entries().len() != FIRST_MAIN_HAND_COUNT_V1
        || coverage.kernel_card_db_hash() != KERNEL_CARDDB_HASH
        || ledger.object_count() != FIRST_MAIN_HAND_COUNT_V1
        || ledger.transition_count() != 0
        || ledger.current_frame_sha256() != coverage.source_frame_sha256()
        || ledger.seed_source_commitment_sha256() != Some(coverage.coverage_commitment_sha256())
    {
        return Err(error_v1(
            "first_main_legal_action_source_mismatch",
            "coverage and unchanged source-bound eight-card ledger must match exactly",
        ));
    }

    let bindings = ledger.current_object_bindings_v1();
    if bindings.len() != FIRST_MAIN_HAND_COUNT_V1 {
        return Err(error_v1(
            "first_main_legal_action_binding_count",
            bindings.len().to_string(),
        ));
    }
    let mut ordered_legal_actions = Vec::with_capacity(FIRST_MAIN_ACTION_COUNT_V1);
    for (ordinal, (entry, binding)) in coverage.entries().iter().zip(&bindings).enumerate() {
        let expected_ordinal = u8::try_from(ordinal).map_err(|_| {
            error_v1(
                "first_main_legal_action_ordinal_overflow",
                ordinal.to_string(),
            )
        })?;
        let expected_arena_id = u32::try_from(ordinal + 1).map_err(|_| {
            error_v1(
                "first_main_legal_action_arena_id_overflow",
                ordinal.to_string(),
            )
        })?;
        if entry.ordinal() != expected_ordinal
            || entry.disposition()
                != MtgoFirstMainKernelCardCoverageDispositionV1::FullySupportedDeckCard
            || !matches!(entry.visible_card_name(), "Forest" | "Island")
            || entry.card_db_id() != Some(binding.kernel_ref.card_db_id)
            || binding.adapter_object_id != format!("first-main:hand-slot-{ordinal}")
            || binding.kernel_ref.arena_id != expected_arena_id
            || binding.kernel_ref.owner != PlayerSeatV1::P0
            || binding.kernel_ref.controller != PlayerSeatV1::P0
            || binding.kernel_ref.zone != Zone::Hand
            || binding.kernel_ref.zone_change_count != 0
        {
            return Err(error_v1(
                "first_main_legal_action_binding_mismatch",
                ordinal.to_string(),
            ));
        }
        ordered_legal_actions.push(ActionSemanticV1::PlayLand {
            actor: PlayerSeatV1::P0,
            source: binding.kernel_ref.clone(),
        });
    }
    ordered_legal_actions.push(ActionSemanticV1::Pass {
        actor: PlayerSeatV1::P0,
    });
    if ordered_legal_actions.len() != FIRST_MAIN_ACTION_COUNT_V1 {
        return Err(error_v1(
            "first_main_legal_action_count",
            ordered_legal_actions.len().to_string(),
        ));
    }

    let encoded_actions = serde_json::to_vec(&ordered_legal_actions)
        .map_err(|error| error_v1("first_main_legal_action_serialization", error.to_string()))?;
    let mut hasher = Sha256::new();
    hasher.update(FIRST_MAIN_LEGAL_ACTIONS_DOMAIN_V1);
    for part in [
        coverage.source_frame_sha256().as_bytes(),
        coverage.coverage_commitment_sha256().as_bytes(),
        ledger.ledger_commitment_sha256().as_bytes(),
        &KERNEL_CARDDB_HASH.to_le_bytes(),
        &encoded_actions,
    ] {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part);
    }

    Ok(CheckedUntrustedMtgoFirstMainLegalActionsV1 {
        source_frame_sha256: coverage.source_frame_sha256().to_owned(),
        coverage_commitment_sha256: coverage.coverage_commitment_sha256().to_owned(),
        object_ledger_commitment_sha256: ledger.ledger_commitment_sha256().to_owned(),
        play_land_action_count: FIRST_MAIN_HAND_COUNT_V1 as u8,
        ordered_legal_actions,
        action_set_commitment_sha256: format!("{:x}", hasher.finalize()),
    })
}

fn error_v1(code: &'static str, detail: impl Into<String>) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        complete_island_coverage_for_test_v1,
        start_checked_untrusted_first_main_hand_object_ledger_v1,
        start_checked_untrusted_visible_object_ledger_v1, MtgoVisibleObjectSeedV1,
    };

    #[test]
    fn supported_first_main_actions_follow_kernel_order_and_exact_bindings() {
        let coverage = complete_island_coverage_for_test_v1();
        let ledger =
            start_checked_untrusted_first_main_hand_object_ledger_v1(&coverage, PlayerSeatV1::P0)
                .unwrap();
        let actions =
            derive_checked_untrusted_first_main_legal_actions_v1(&coverage, &ledger).unwrap();

        assert_eq!(
            actions.source_frame_sha256(),
            coverage.source_frame_sha256()
        );
        assert_eq!(actions.play_land_action_count(), 8);
        assert_eq!(actions.action_count(), 9);
        assert_eq!(actions.ordered_legal_actions.len(), 9);
        for (ordinal, action) in actions.ordered_legal_actions[..8].iter().enumerate() {
            let ActionSemanticV1::PlayLand { actor, source } = action else {
                panic!("action {ordinal} must be PlayLand");
            };
            assert_eq!(*actor, PlayerSeatV1::P0);
            assert_eq!(source.arena_id, u32::try_from(ordinal + 1).unwrap());
            assert_eq!(source.zone, Zone::Hand);
            assert_eq!(source.zone_change_count, 0);
        }
        assert_eq!(
            actions.ordered_legal_actions[8],
            ActionSemanticV1::Pass {
                actor: PlayerSeatV1::P0,
            }
        );
        assert!(!actions.safe_for_observation_v5());
        assert!(!actions.safe_for_policy_scoring());
        assert!(!actions.safe_for_input());
        assert!(!actions.compatible_with_two_player_starting_first_main());
    }

    #[test]
    fn unbound_ledger_cannot_create_first_main_actions() {
        let coverage = complete_island_coverage_for_test_v1();
        let ledger = start_checked_untrusted_visible_object_ledger_v1(
            coverage.source_frame_sha256(),
            (0..8)
                .map(|ordinal| MtgoVisibleObjectSeedV1 {
                    display_ordinal: ordinal,
                    visible_object_id: format!("first-main:hand-slot-{ordinal}"),
                    visible_card_name: "Island".to_owned(),
                    owner: PlayerSeatV1::P0,
                    controller: PlayerSeatV1::P0,
                    zone: Zone::Hand,
                })
                .collect(),
        )
        .unwrap();
        assert!(derive_checked_untrusted_first_main_legal_actions_v1(&coverage, &ledger).is_err());
    }

    #[test]
    fn non_local_actor_cannot_create_first_main_actions() {
        let coverage = complete_island_coverage_for_test_v1();
        let ledger =
            start_checked_untrusted_first_main_hand_object_ledger_v1(&coverage, PlayerSeatV1::P1)
                .unwrap();
        let error = derive_checked_untrusted_first_main_legal_actions_v1(&coverage, &ledger)
            .err()
            .unwrap();
        assert_eq!(error.code(), "first_main_legal_action_binding_mismatch");
    }
}
