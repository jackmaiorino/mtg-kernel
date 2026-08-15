//! Compile-bound cell-zero arm identity.
//!
//! The stock lineage carries `stock-cmma`; the Unit wiring commit patches
//! this single constant to `simple-unit-fork` alongside the crates-io fork
//! patch, so the arm value is bound by each arm commit's tracked-tree SHA
//! and embedded in the binary at compile time. The evidence probe validates
//! the launcher-declared arm against this constant, which makes an
//! executable-argument swap in the launcher unable to relabel a binary's
//! arm: the binary itself refuses to run under the wrong name.

// Reserved for the evidence probe's arm-validation wiring described above;
// not yet read in this build configuration.
#[allow(dead_code)]
pub(crate) const CELL_ZERO_ARM_V1: &str = "stock-cmma";

/// The numerical-mode claim derived from the compiled arm rather than the
/// device manifest's stock-scoped constant.
#[allow(dead_code)]
pub(crate) const CELL_ZERO_ARM_NUMERICAL_MODE_V1: &str =
    "stock-cubecl-auto-cmma-tf32-operand-conversion";
