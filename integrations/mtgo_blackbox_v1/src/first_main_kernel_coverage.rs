use crate::{
    mtgo_kernel_supported_card_profile_commitment_v1,
    resolve_checked_untrusted_kernel_card_correspondence_v1,
    CheckedUntrustedMtgoOfflineFirstMainVisibleCardIdentityCandidateV1, MtgoContractErrorV1,
    MtgoKernelCardCorrespondenceDispositionV1, MtgoOfflineVisibleCardIdentityClassificationV1,
};
use mtg_kernel::card_def::KERNEL_CARDDB_HASH;
use sha2::{Digest, Sha256};

const FIRST_MAIN_KERNEL_COVERAGE_DOMAIN_V1: &[u8] = b"mtgo-first-main-kernel-card-coverage-v1";
const FIRST_MAIN_VISIBLE_HAND_COUNT_V1: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MtgoFirstMainKernelCardCoverageDispositionV1 {
    FullySupportedDeckCard,
    FullySupportedToken,
    MissingFromKernelRegistry,
    RegisteredButNotFullySupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoFirstMainKernelCardCoverageEntryV1 {
    ordinal: u8,
    visible_card_name: String,
    disposition: MtgoFirstMainKernelCardCoverageDispositionV1,
    card_db_id: Option<u16>,
    correspondence_commitment_sha256: Option<String>,
}

impl MtgoFirstMainKernelCardCoverageEntryV1 {
    pub fn ordinal(&self) -> u8 {
        self.ordinal
    }

    pub fn visible_card_name(&self) -> &str {
        &self.visible_card_name
    }

    pub fn disposition(&self) -> MtgoFirstMainKernelCardCoverageDispositionV1 {
        self.disposition
    }

    pub fn card_db_id(&self) -> Option<u16> {
        self.card_db_id
    }

    pub fn correspondence_commitment_sha256(&self) -> Option<&str> {
        self.correspondence_commitment_sha256.as_deref()
    }

    pub fn has_full_kernel_correspondence(&self) -> bool {
        matches!(
            self.disposition,
            MtgoFirstMainKernelCardCoverageDispositionV1::FullySupportedDeckCard
                | MtgoFirstMainKernelCardCoverageDispositionV1::FullySupportedToken
        )
    }
}

/// Fail-closed coverage report over one complete checked-untrusted first-main
/// visible-hand identity result.
///
/// The report preserves ordinal and exact-name coverage status only. It does
/// not create object incarnations, stable references, observations, policy
/// inputs, legal actions, or input authority.
pub struct CheckedUntrustedMtgoFirstMainKernelCardCoverageV1 {
    source_identity_candidate_commitment_sha256: String,
    source_manifest_sha256: String,
    source_frame_sha256: String,
    kernel_card_db_hash: u64,
    supported_profile_commitment_sha256: String,
    fully_supported_count: u8,
    entries: Vec<MtgoFirstMainKernelCardCoverageEntryV1>,
    coverage_commitment_sha256: String,
}

impl CheckedUntrustedMtgoFirstMainKernelCardCoverageV1 {
    pub fn source_identity_candidate_commitment_sha256(&self) -> &str {
        &self.source_identity_candidate_commitment_sha256
    }

    pub fn source_manifest_sha256(&self) -> &str {
        &self.source_manifest_sha256
    }

    pub fn source_frame_sha256(&self) -> &str {
        &self.source_frame_sha256
    }

    pub fn kernel_card_db_hash(&self) -> u64 {
        self.kernel_card_db_hash
    }

    pub fn supported_profile_commitment_sha256(&self) -> &str {
        &self.supported_profile_commitment_sha256
    }

    pub fn fully_supported_count(&self) -> u8 {
        self.fully_supported_count
    }

    pub fn entries(&self) -> &[MtgoFirstMainKernelCardCoverageEntryV1] {
        &self.entries
    }

    pub fn coverage_complete(&self) -> bool {
        usize::from(self.fully_supported_count) == FIRST_MAIN_VISIBLE_HAND_COUNT_V1
    }

    pub fn coverage_commitment_sha256(&self) -> &str {
        &self.coverage_commitment_sha256
    }

    pub fn safe_for_object_binding(&self) -> bool {
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

pub fn check_untrusted_first_main_kernel_card_coverage_v1(
    identity_candidate: &CheckedUntrustedMtgoOfflineFirstMainVisibleCardIdentityCandidateV1,
) -> Result<CheckedUntrustedMtgoFirstMainKernelCardCoverageV1, MtgoContractErrorV1> {
    if identity_candidate.classification() != MtgoOfflineVisibleCardIdentityClassificationV1::Match
        || identity_candidate.visible_hand_count() != Some(FIRST_MAIN_VISIBLE_HAND_COUNT_V1 as u8)
        || identity_candidate.matched_identity_count() != FIRST_MAIN_VISIBLE_HAND_COUNT_V1 as u8
        || identity_candidate.identities().len() != FIRST_MAIN_VISIBLE_HAND_COUNT_V1
    {
        return Err(MtgoContractErrorV1::new(
            "first_main_visible_identity_incomplete",
            "kernel coverage requires one complete eight-card identity result",
        ));
    }

    let supported_profile_commitment_sha256 = mtgo_kernel_supported_card_profile_commitment_v1()?;
    let mut entries = Vec::with_capacity(FIRST_MAIN_VISIBLE_HAND_COUNT_V1);
    for (expected_ordinal, identity) in identity_candidate.identities().iter().enumerate() {
        let expected_ordinal = u8::try_from(expected_ordinal).map_err(|_| {
            MtgoContractErrorV1::new(
                "first_main_visible_identity_ordinal_overflow",
                expected_ordinal.to_string(),
            )
        })?;
        if identity.ordinal() != expected_ordinal {
            return Err(MtgoContractErrorV1::new(
                "first_main_visible_identity_ordinal",
                format!("expected={expected_ordinal},actual={}", identity.ordinal()),
            ));
        }

        let (disposition, card_db_id, correspondence_commitment_sha256) =
            match resolve_checked_untrusted_kernel_card_correspondence_v1(
                identity.visible_card_name(),
            ) {
                Ok(correspondence) => {
                    if correspondence.supported_profile_commitment_sha256()
                        != supported_profile_commitment_sha256
                    {
                        return Err(MtgoContractErrorV1::new(
                            "first_main_kernel_profile_mismatch",
                            identity.visible_card_name(),
                        ));
                    }
                    let disposition = match correspondence.disposition() {
                        MtgoKernelCardCorrespondenceDispositionV1::FullySupportedDeckCard => {
                            MtgoFirstMainKernelCardCoverageDispositionV1::FullySupportedDeckCard
                        }
                        MtgoKernelCardCorrespondenceDispositionV1::FullySupportedToken => {
                            MtgoFirstMainKernelCardCoverageDispositionV1::FullySupportedToken
                        }
                    };
                    (
                        disposition,
                        Some(correspondence.card_db_id()),
                        Some(correspondence.correspondence_commitment_sha256().to_owned()),
                    )
                }
                Err(error) if error.code() == "kernel_card_name_unknown" => (
                    MtgoFirstMainKernelCardCoverageDispositionV1::MissingFromKernelRegistry,
                    None,
                    None,
                ),
                Err(error) if error.code() == "kernel_card_not_fully_supported" => (
                    MtgoFirstMainKernelCardCoverageDispositionV1::RegisteredButNotFullySupported,
                    None,
                    None,
                ),
                Err(error) => return Err(error),
            };
        entries.push(MtgoFirstMainKernelCardCoverageEntryV1 {
            ordinal: identity.ordinal(),
            visible_card_name: identity.visible_card_name().to_owned(),
            disposition,
            card_db_id,
            correspondence_commitment_sha256,
        });
    }

    let fully_supported_count = u8::try_from(
        entries
            .iter()
            .filter(|entry| entry.has_full_kernel_correspondence())
            .count(),
    )
    .map_err(|_| {
        MtgoContractErrorV1::new(
            "first_main_kernel_coverage_count_overflow",
            entries.len().to_string(),
        )
    })?;
    let coverage_commitment_sha256 = coverage_commitment_v1(
        identity_candidate.candidate_commitment_sha256(),
        &supported_profile_commitment_sha256,
        &entries,
    );

    Ok(CheckedUntrustedMtgoFirstMainKernelCardCoverageV1 {
        source_identity_candidate_commitment_sha256: identity_candidate
            .candidate_commitment_sha256()
            .to_owned(),
        source_manifest_sha256: identity_candidate.source_manifest_sha256().to_owned(),
        source_frame_sha256: identity_candidate.source_frame_sha256().to_owned(),
        kernel_card_db_hash: KERNEL_CARDDB_HASH,
        supported_profile_commitment_sha256,
        fully_supported_count,
        entries,
        coverage_commitment_sha256,
    })
}

fn coverage_commitment_v1(
    source_identity_candidate_commitment_sha256: &str,
    supported_profile_commitment_sha256: &str,
    entries: &[MtgoFirstMainKernelCardCoverageEntryV1],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(FIRST_MAIN_KERNEL_COVERAGE_DOMAIN_V1);
    update_hash_part_v1(
        &mut hasher,
        source_identity_candidate_commitment_sha256.as_bytes(),
    );
    update_hash_part_v1(&mut hasher, supported_profile_commitment_sha256.as_bytes());
    update_hash_part_v1(&mut hasher, &KERNEL_CARDDB_HASH.to_le_bytes());
    update_hash_part_v1(&mut hasher, &(entries.len() as u64).to_le_bytes());
    for entry in entries {
        update_hash_part_v1(&mut hasher, &[entry.ordinal]);
        update_hash_part_v1(&mut hasher, entry.visible_card_name.as_bytes());
        update_hash_part_v1(
            &mut hasher,
            &[match entry.disposition {
                MtgoFirstMainKernelCardCoverageDispositionV1::FullySupportedDeckCard => 0,
                MtgoFirstMainKernelCardCoverageDispositionV1::FullySupportedToken => 1,
                MtgoFirstMainKernelCardCoverageDispositionV1::MissingFromKernelRegistry => 2,
                MtgoFirstMainKernelCardCoverageDispositionV1::RegisteredButNotFullySupported => 3,
            }],
        );
        update_hash_part_v1(
            &mut hasher,
            &entry.card_db_id.unwrap_or(u16::MAX).to_le_bytes(),
        );
        update_hash_part_v1(
            &mut hasher,
            entry
                .correspondence_commitment_sha256
                .as_deref()
                .unwrap_or("")
                .as_bytes(),
        );
    }
    format!("{:x}", hasher.finalize())
}

fn update_hash_part_v1(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}
