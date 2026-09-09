use crate::MtgoContractErrorV1;
use mtg_kernel::card_def::{card_id_by_name, CardCapability, CARD_DEFS, KERNEL_CARDDB_HASH};
use sha2::{Digest, Sha256};

const MTGO_KERNEL_CARD_PROFILE_DOMAIN_V1: &[u8] = b"mtgo-kernel-card-profile-v1";
const MTGO_KERNEL_CARD_CORRESPONDENCE_DOMAIN_V1: &[u8] = b"mtgo-kernel-card-correspondence-v1";
const MAX_VISIBLE_CARD_NAME_BYTES_V1: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MtgoKernelCardCorrespondenceDispositionV1 {
    FullySupportedDeckCard,
    FullySupportedToken,
}

/// Exact-name lookup against the compile-bound kernel card database.
///
/// The caller-supplied name is not visual evidence. This checked-untrusted
/// value therefore cannot create a stable object reference, an ObservationV5,
/// a legal action, or input authority. A future perception composition must
/// separately prove the visible card identity and object incarnation.
pub struct CheckedUntrustedMtgoKernelCardCorrespondenceV1 {
    visible_card_name: String,
    card_db_id: u16,
    disposition: MtgoKernelCardCorrespondenceDispositionV1,
    kernel_card_db_hash: u64,
    supported_profile_commitment_sha256: String,
    correspondence_commitment_sha256: String,
}

impl CheckedUntrustedMtgoKernelCardCorrespondenceV1 {
    pub fn visible_card_name(&self) -> &str {
        &self.visible_card_name
    }

    pub fn card_db_id(&self) -> u16 {
        self.card_db_id
    }

    pub fn disposition(&self) -> MtgoKernelCardCorrespondenceDispositionV1 {
        self.disposition
    }

    pub fn kernel_card_db_hash(&self) -> u64 {
        self.kernel_card_db_hash
    }

    pub fn supported_profile_commitment_sha256(&self) -> &str {
        &self.supported_profile_commitment_sha256
    }

    pub fn correspondence_commitment_sha256(&self) -> &str {
        &self.correspondence_commitment_sha256
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

pub fn resolve_checked_untrusted_kernel_card_correspondence_v1(
    visible_card_name: &str,
) -> Result<CheckedUntrustedMtgoKernelCardCorrespondenceV1, MtgoContractErrorV1> {
    validate_exact_visible_name_v1(visible_card_name)?;
    let card_db_id = card_id_by_name(visible_card_name)
        .ok_or_else(|| MtgoContractErrorV1::new("kernel_card_name_unknown", visible_card_name))?;
    let definition = CARD_DEFS.get(usize::from(card_db_id)).ok_or_else(|| {
        MtgoContractErrorV1::new(
            "kernel_card_definition_missing",
            format!("name={visible_card_name},id={card_db_id}"),
        )
    })?;
    if definition.name != visible_card_name {
        return Err(MtgoContractErrorV1::new(
            "kernel_card_name_round_trip_mismatch",
            visible_card_name,
        ));
    }
    // Unreachable with the frozen registry (every definition is Full); covered again when a partial-capability card lands.
    if definition.capability != CardCapability::Full {
        return Err(MtgoContractErrorV1::new(
            "kernel_card_not_fully_supported",
            visible_card_name,
        ));
    }
    let disposition = if definition.is_token {
        MtgoKernelCardCorrespondenceDispositionV1::FullySupportedToken
    } else {
        MtgoKernelCardCorrespondenceDispositionV1::FullySupportedDeckCard
    };
    let supported_profile_commitment_sha256 = mtgo_kernel_supported_card_profile_commitment_v1()?;
    let correspondence_commitment_sha256 = correspondence_commitment_v1(
        visible_card_name,
        card_db_id,
        disposition,
        &supported_profile_commitment_sha256,
    );

    Ok(CheckedUntrustedMtgoKernelCardCorrespondenceV1 {
        visible_card_name: visible_card_name.to_owned(),
        card_db_id,
        disposition,
        kernel_card_db_hash: KERNEL_CARDDB_HASH,
        supported_profile_commitment_sha256,
        correspondence_commitment_sha256,
    })
}

pub fn mtgo_kernel_supported_card_profile_commitment_v1() -> Result<String, MtgoContractErrorV1> {
    let mut hasher = Sha256::new();
    hasher.update(MTGO_KERNEL_CARD_PROFILE_DOMAIN_V1);
    hasher.update(KERNEL_CARDDB_HASH.to_le_bytes());
    let full_count = CARD_DEFS
        .iter()
        .filter(|definition| definition.capability == CardCapability::Full)
        .count();
    hasher.update(
        u64::try_from(full_count)
            .map_err(|_| {
                MtgoContractErrorV1::new(
                    "kernel_supported_card_count_overflow",
                    full_count.to_string(),
                )
            })?
            .to_le_bytes(),
    );
    for (index, definition) in CARD_DEFS.iter().enumerate() {
        if definition.capability != CardCapability::Full {
            continue;
        }
        let card_db_id = u16::try_from(index)
            .map_err(|_| MtgoContractErrorV1::new("kernel_card_id_overflow", index.to_string()))?;
        update_hash_part_v1(&mut hasher, &card_db_id.to_le_bytes());
        update_hash_part_v1(&mut hasher, definition.name.as_bytes());
        update_hash_part_v1(&mut hasher, &[u8::from(definition.is_token)]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn validate_exact_visible_name_v1(value: &str) -> Result<(), MtgoContractErrorV1> {
    if value.is_empty()
        || value.len() > MAX_VISIBLE_CARD_NAME_BYTES_V1
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(MtgoContractErrorV1::new("visible_card_name_invalid", value));
    }
    Ok(())
}

fn correspondence_commitment_v1(
    visible_card_name: &str,
    card_db_id: u16,
    disposition: MtgoKernelCardCorrespondenceDispositionV1,
    profile_commitment_sha256: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(MTGO_KERNEL_CARD_CORRESPONDENCE_DOMAIN_V1);
    update_hash_part_v1(&mut hasher, profile_commitment_sha256.as_bytes());
    update_hash_part_v1(&mut hasher, visible_card_name.as_bytes());
    update_hash_part_v1(&mut hasher, &card_db_id.to_le_bytes());
    update_hash_part_v1(
        &mut hasher,
        &[match disposition {
            MtgoKernelCardCorrespondenceDispositionV1::FullySupportedDeckCard => 0,
            MtgoKernelCardCorrespondenceDispositionV1::FullySupportedToken => 1,
        }],
    );
    format!("{:x}", hasher.finalize())
}

fn update_hash_part_v1(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}
