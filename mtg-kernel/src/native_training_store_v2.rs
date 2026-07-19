//! Native trainer persistence boundary shared by the executor and the strict
//! generation store.
//!
//! This first seam intentionally exposes only the move-only generation receipt.
//! Production construction remains private to this module, so the executor
//! cannot advance merely because a caller knows the expected digest values.
//! The durable publication/read/recovery implementation will construct this
//! receipt only from independently recaptured published bytes.

/// Non-forgeable witness that one exact native training generation was
/// durably published and independently recaptured by the V2 store.
///
/// The type deliberately implements neither [`Clone`] nor a public constructor.
/// Read-only accessors support the executor's final receipt comparison and
/// audit diagnostics without allowing a caller to manufacture a witness.
///
/// External construction is rejected because every field is private:
///
/// ```compile_fail
/// use mtg_kernel::native_training_store_v2::NativeTrainingPersistenceReceiptV2;
///
/// let _forged = NativeTrainingPersistenceReceiptV2 {
///     generation_index: 1,
///     checkpoint_payload_sha256: [0; 32],
///     checkpoint_manifest_sha256: [0; 32],
/// };
/// ```
///
/// The witness is move-only:
///
/// ```compile_fail
/// use mtg_kernel::native_training_store_v2::NativeTrainingPersistenceReceiptV2;
///
/// fn duplicate(receipt: NativeTrainingPersistenceReceiptV2) {
///     let _copy = receipt.clone();
/// }
/// ```
#[derive(Debug, Eq, PartialEq)]
#[must_use = "a persistence receipt must be consumed by the prepared update commit"]
pub struct NativeTrainingPersistenceReceiptV2 {
    generation_index: u64,
    checkpoint_payload_sha256: [u8; 32],
    checkpoint_manifest_sha256: [u8; 32],
}

impl NativeTrainingPersistenceReceiptV2 {
    pub const fn generation_index(&self) -> u64 {
        self.generation_index
    }

    pub const fn checkpoint_payload_sha256(&self) -> [u8; 32] {
        self.checkpoint_payload_sha256
    }

    pub const fn checkpoint_manifest_sha256(&self) -> [u8; 32] {
        self.checkpoint_manifest_sha256
    }
}

// The production constructor lands in this module with the generation-store
// publisher. Until then, only crate tests can exercise the executor's receipt
// comparison; no production caller can construct this type.
#[cfg(test)]
pub(crate) const fn test_persistence_receipt_v2(
    generation_index: u64,
    checkpoint_payload_sha256: [u8; 32],
    checkpoint_manifest_sha256: [u8; 32],
) -> NativeTrainingPersistenceReceiptV2 {
    NativeTrainingPersistenceReceiptV2 {
        generation_index,
        checkpoint_payload_sha256,
        checkpoint_manifest_sha256,
    }
}
