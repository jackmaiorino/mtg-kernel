//! Resolve one validated population refresh into immutable runtime handles.
//!
//! Slot roots are positional and carry no authority by themselves. Each root
//! is reopened through its own RunV2 record and complete Store walk, then the
//! named generation and every digest recorded by the refresh manifest are
//! checked before checkpoint inference is constructed.

use crate::kernel_native_search_opponent_v1::{
    KernelNativeSearchOpponentV1, KERNEL_NATIVE_SEARCH_AUTHORITY_KIND_V1,
};
use crate::native_checkpoint_inference_v1::load_native_checkpoint_inference_v1;
use crate::native_population_opponent_v1::{
    PopulationOpponentEngineV1, PopulationSlotOccupantV1, PopulationWeightVectorV1,
    POPULATION_OPPONENT_SLOT_COUNT_V1,
};
use crate::native_population_refresh_manifest_v1::{
    PopulationRefreshManifestV1, PopulationTrancheRefreshManifestV2,
};
use crate::native_training_store_digest_v1::lower_hex_raw32_v1;
use crate::native_training_store_resume_v2::load_native_training_boundary_v2;
use crate::native_training_store_root_v2::ValidatedNativeTrainingStoreRootV2;
use crate::native_training_store_run_v2::decode_train_run_v2;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PopulationRuntimeResolutionErrorKindV1 {
    SlotCount,
    RunRead,
    RunInvalid,
    RootInvalid,
    BoundaryInvalid,
    AuthorityMismatch,
    InferenceInvalid,
    WeightInvalid,
    // CLAUDE-SEARCHER-POOL-AUTHORITY-SHEET-V1.md Section 6 item 3: a
    // search-occupied slot's declared config fails its own two-check
    // re-verification (`.validate()` plus `matches_fresh_reconstruction_v1()`)
    // at resolution time, re-run here the same way a Store slot's SHA
    // fields are re-checked against the actually-loaded artifact, not just
    // trusted from manifest decode.
    SearchAuthorityInvalid,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PopulationRuntimeResolutionErrorV1 {
    kind: PopulationRuntimeResolutionErrorKindV1,
}

impl PopulationRuntimeResolutionErrorV1 {
    const fn new(kind: PopulationRuntimeResolutionErrorKindV1) -> Self {
        Self { kind }
    }

    pub(crate) const fn kind_v1(self) -> PopulationRuntimeResolutionErrorKindV1 {
        self.kind
    }
}

impl Display for PopulationRuntimeResolutionErrorV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:?}", self.kind)
    }
}

impl Error for PopulationRuntimeResolutionErrorV1 {}

type Result<T> = std::result::Result<T, PopulationRuntimeResolutionErrorV1>;

const POPULATION_RESPONSE_TARGET_SLOT_COUNT_V1: usize = 6;

pub(crate) fn resolve_population_opponent_v1(
    manifest: &PopulationRefreshManifestV1,
    slot_store_roots: &[PathBuf],
) -> Result<PopulationOpponentEngineV1> {
    let handles = resolve_population_handles_v1(manifest, slot_store_roots)?;
    let weights = population_manifest_weight_vector_v1(manifest)?;
    Ok(PopulationOpponentEngineV1::new_v1(weights, handles))
}

/// Resolves the frozen response-exploiter target through all eight manifest
/// authorities while restricting runtime selection to slots 0 through 5.
///
/// The six retained weights are the manifest's original integer units. Slots
/// 6 and 7 are assigned exact zero weight, and the declared total is the
/// checked sum of the retained units, so no proportional rounding occurs.
pub(crate) fn resolve_population_response_target_v1(
    manifest: &PopulationRefreshManifestV1,
    slot_store_roots: &[PathBuf],
) -> Result<PopulationOpponentEngineV1> {
    let handles = resolve_population_handles_v1(manifest, slot_store_roots)?;
    let weights = population_response_target_weight_vector_v1(manifest)?;
    Ok(PopulationOpponentEngineV1::new_v1(weights, handles))
}

/// Evaluation-only sibling of [`resolve_population_response_target_v1`].
/// It authenticates the same eight Store authorities and uses the same six
/// retained integer weights, but keeps one selected component fixed across
/// both legs of each seat-swapped pair.
#[cfg(test)]
pub(crate) fn resolve_population_response_target_pairwise_v1(
    manifest: &PopulationRefreshManifestV1,
    slot_store_roots: &[PathBuf],
) -> Result<PopulationOpponentEngineV1> {
    let handles = resolve_population_handles_v1(manifest, slot_store_roots)?;
    let weights = population_response_target_weight_vector_v1(manifest)?;
    Ok(PopulationOpponentEngineV1::new_pairwise_eval_v1(
        weights, handles,
    ))
}

fn resolve_population_handles_v1(
    manifest: &PopulationRefreshManifestV1,
    slot_store_roots: &[PathBuf],
) -> Result<[PopulationSlotOccupantV1; POPULATION_OPPONENT_SLOT_COUNT_V1]> {
    if slot_store_roots.len() != POPULATION_OPPONENT_SLOT_COUNT_V1
        || manifest.slots_v1().len() != POPULATION_OPPONENT_SLOT_COUNT_V1
    {
        return Err(PopulationRuntimeResolutionErrorV1::new(
            PopulationRuntimeResolutionErrorKindV1::SlotCount,
        ));
    }

    let mut handles = Vec::with_capacity(POPULATION_OPPONENT_SLOT_COUNT_V1);
    for (slot, store_root) in manifest.slots_v1().iter().zip(slot_store_roots) {
        // CLAUDE-SEARCHER-POOL-AUTHORITY-SHEET-V1.md Section 6 item 3: a
        // search-occupied slot never reads a Store, never resolves a
        // generation, and positively confirms its own kind rather than ever
        // falling through to a Store read for that index. `store_root` is
        // present (fixed slot-count shape, unchanged) but deliberately never
        // touched in this branch.
        if slot.occupant_class_v1() == KERNEL_NATIVE_SEARCH_AUTHORITY_KIND_V1 {
            let _ = store_root;
            let search_authority = slot.search_authority_v1().ok_or_else(|| {
                PopulationRuntimeResolutionErrorV1::new(
                    PopulationRuntimeResolutionErrorKindV1::SearchAuthorityInvalid,
                )
            })?;
            let authority = search_authority.to_authority_v1();
            if authority.validate().is_err() || !authority.matches_fresh_reconstruction_v1() {
                return Err(PopulationRuntimeResolutionErrorV1::new(
                    PopulationRuntimeResolutionErrorKindV1::SearchAuthorityInvalid,
                ));
            }
            let searcher = KernelNativeSearchOpponentV1::new(authority).map_err(|_| {
                PopulationRuntimeResolutionErrorV1::new(
                    PopulationRuntimeResolutionErrorKindV1::SearchAuthorityInvalid,
                )
            })?;
            handles.push(PopulationSlotOccupantV1::Search(Arc::new(searcher)));
            continue;
        }
        let run_bytes = fs::read(store_root.join("run.json")).map_err(|_| {
            PopulationRuntimeResolutionErrorV1::new(PopulationRuntimeResolutionErrorKindV1::RunRead)
        })?;
        let run = decode_train_run_v2(&run_bytes).map_err(|_| {
            PopulationRuntimeResolutionErrorV1::new(
                PopulationRuntimeResolutionErrorKindV1::RunInvalid,
            )
        })?;
        let root = ValidatedNativeTrainingStoreRootV2::open_v2(store_root).map_err(|_| {
            PopulationRuntimeResolutionErrorV1::new(
                PopulationRuntimeResolutionErrorKindV1::RootInvalid,
            )
        })?;
        let boundary = load_native_training_boundary_v2(&root, &run, slot.source_generation_v1())
            .map_err(|_| {
            PopulationRuntimeResolutionErrorV1::new(
                PopulationRuntimeResolutionErrorKindV1::BoundaryInvalid,
            )
        })?;
        let checkpoint = boundary.checkpoint();
        let matches_authority = run.run_sha256() == slot.source_run_sha256_v1()
            && run.record().schedule.base_seed == slot.source_base_seed_v1()
            && checkpoint.generation_index() == slot.source_generation_v1()
            && lower_hex_raw32_v1(checkpoint.checkpoint_manifest_sha256())
                == slot.checkpoint_sha256_v1()
            && lower_hex_raw32_v1(boundary.boundary().checkpoint_sidecar_sha256())
                == slot.sidecar_sha256_v1()
            && lower_hex_raw32_v1(checkpoint.checkpoint_payload_sha256()) == slot.state_sha256_v1()
            && lower_hex_raw32_v1(checkpoint.model_parameter_sha256())
                == slot.model_parameter_sha256_v1();
        if !matches_authority {
            return Err(PopulationRuntimeResolutionErrorV1::new(
                PopulationRuntimeResolutionErrorKindV1::AuthorityMismatch,
            ));
        }
        handles.push(PopulationSlotOccupantV1::Checkpoint(
            load_native_checkpoint_inference_v1(&run, checkpoint, boundary.payload()).map_err(
                |_| {
                    PopulationRuntimeResolutionErrorV1::new(
                        PopulationRuntimeResolutionErrorKindV1::InferenceInvalid,
                    )
                },
            )?,
        ));
    }

    handles.try_into().map_err(|_| {
        PopulationRuntimeResolutionErrorV1::new(PopulationRuntimeResolutionErrorKindV1::SlotCount)
    })
}

/// Population program V2 tranche/cycle training-side sibling of
/// [`resolve_population_opponent_v1`] above: resolves a validated
/// `PopulationTrancheRefreshManifestV2` link (Amendment 4, ported from
/// commit `8c8d645`) into the same `PopulationOpponentEngineV1` shape, with
/// all eight slots weighted (no exclusion; every slot, frozen or active, is
/// part of the live training pool). Authenticates every Store-backed slot's
/// identity against its own live Store, mirroring `resolve_population_handles_v1`'s
/// discipline minus the sidecar-hash check (the tranche-refresh manifest's
/// own slot shape carries no `sidecar_sha256` field, the same disclosed
/// absence recorded on `PopulationTrancheRefreshSlotV2` itself).
///
/// FIX (found live, before this was used for a real launch, not carried
/// unchanged from `8c8d645` as an earlier revision of this comment claimed):
/// the caller-supplied `slot_store_roots[i]` is `fs::read` directly and is
/// the sole source of PHYSICAL LOCATION for slot `i`; it is never compared
/// against the manifest's own `store_root` field. Authentication is
/// entirely hash-based (`run_sha256`/`source_base_seed`/`source_generation`/
/// `checkpoint_manifest_sha256`/`checkpoint_payload_sha256`/
/// `model_parameter_sha256`, below), exactly mirroring how
/// `resolve_population_handles_v1` authenticates a v1 slot without ever
/// having a `store_root` field on the manifest to compare against in the
/// first place. An earlier draft of this function additionally required
/// `slot_store_roots[i] == slot.store_root_v2()` (byte-for-byte), ported
/// directly from `8c8d645`'s own assumption that a slot's physical location
/// never moves after it is sealed. That assumption is false in practice:
/// this program's own 8/25 evidence cleanup relocated several `C:\` store
/// roots (frozen forever in already-sealed historical manifests) to `E:\`
/// archive paths (and duplicate working copies exist under `D:\` besides).
/// Requiring an exact string match would make every historical slot whose
/// physical copy has since moved permanently unresolvable, even though its
/// content -- and therefore its authenticated identity -- is unchanged and
/// independently verifiable via the hash checks alone. Dropping the string
/// match does not weaken authentication: the manifest's own `store_root`
/// field is retained as the frozen, chain-immutable HISTORICAL label
/// (checked for frozen-slot continuity across links by
/// `identity_matches_v2`, `native_population_refresh_manifest_v1.rs`), and
/// the six hash/seed/generation checks below are the actual authenticator,
/// unconditionally, for every slot, exactly as before this fix. Search-
/// occupied slots (Amendment 4 A4.3(ii), absent from `8c8d645`, added here
/// to match `resolve_population_handles_v1`'s own already-existing branch
/// below) never read a Store at all, mirroring that branch exactly.
pub(crate) fn resolve_population_tranche_refresh_opponent_v2(
    manifest: &PopulationTrancheRefreshManifestV2,
    slot_store_roots: &[PathBuf],
) -> Result<PopulationOpponentEngineV1> {
    let handles = resolve_population_tranche_refresh_handles_v2(manifest, slot_store_roots)?;
    let weights = population_tranche_refresh_weight_vector_v2(manifest)?;
    Ok(PopulationOpponentEngineV1::new_v1(weights, handles))
}

fn resolve_population_tranche_refresh_handles_v2(
    manifest: &PopulationTrancheRefreshManifestV2,
    slot_store_roots: &[PathBuf],
) -> Result<[PopulationSlotOccupantV1; POPULATION_OPPONENT_SLOT_COUNT_V1]> {
    if slot_store_roots.len() != POPULATION_OPPONENT_SLOT_COUNT_V1
        || manifest.slots_v2().len() != POPULATION_OPPONENT_SLOT_COUNT_V1
    {
        return Err(PopulationRuntimeResolutionErrorV1::new(
            PopulationRuntimeResolutionErrorKindV1::SlotCount,
        ));
    }

    // No store_root string cross-check here (fixed; see this function's
    // own doc comment above): slot_store_roots[i] is the sole physical
    // location for slot i, authenticated below entirely by hash/seed/
    // generation, never by comparing it against the manifest's own frozen
    // store_root label.
    let mut handles = Vec::with_capacity(POPULATION_OPPONENT_SLOT_COUNT_V1);
    for (slot, store_root) in manifest.slots_v2().iter().zip(slot_store_roots) {
        // Mirrors resolve_population_handles_v1's own search-occupant
        // branch exactly (never reads a Store, never resolves a
        // generation, positively confirms its own kind).
        if slot.occupant_class_v2() == KERNEL_NATIVE_SEARCH_AUTHORITY_KIND_V1 {
            let _ = store_root;
            let search_authority = slot.search_authority_v2().ok_or_else(|| {
                PopulationRuntimeResolutionErrorV1::new(
                    PopulationRuntimeResolutionErrorKindV1::SearchAuthorityInvalid,
                )
            })?;
            let authority = search_authority.to_authority_v1();
            if authority.validate().is_err() || !authority.matches_fresh_reconstruction_v1() {
                return Err(PopulationRuntimeResolutionErrorV1::new(
                    PopulationRuntimeResolutionErrorKindV1::SearchAuthorityInvalid,
                ));
            }
            let searcher = KernelNativeSearchOpponentV1::new(authority).map_err(|_| {
                PopulationRuntimeResolutionErrorV1::new(
                    PopulationRuntimeResolutionErrorKindV1::SearchAuthorityInvalid,
                )
            })?;
            handles.push(PopulationSlotOccupantV1::Search(Arc::new(searcher)));
            continue;
        }
        let run_bytes = fs::read(store_root.join("run.json")).map_err(|_| {
            PopulationRuntimeResolutionErrorV1::new(PopulationRuntimeResolutionErrorKindV1::RunRead)
        })?;
        let run = decode_train_run_v2(&run_bytes).map_err(|_| {
            PopulationRuntimeResolutionErrorV1::new(
                PopulationRuntimeResolutionErrorKindV1::RunInvalid,
            )
        })?;
        let root = ValidatedNativeTrainingStoreRootV2::open_v2(store_root).map_err(|_| {
            PopulationRuntimeResolutionErrorV1::new(
                PopulationRuntimeResolutionErrorKindV1::RootInvalid,
            )
        })?;
        let boundary = load_native_training_boundary_v2(&root, &run, slot.source_generation_v2())
            .map_err(|_| {
            PopulationRuntimeResolutionErrorV1::new(
                PopulationRuntimeResolutionErrorKindV1::BoundaryInvalid,
            )
        })?;
        let checkpoint = boundary.checkpoint();
        let matches_authority = run.run_sha256() == slot.run_sha256_v2()
            && run.record().schedule.base_seed == slot.source_base_seed_v2()
            && checkpoint.generation_index() == slot.source_generation_v2()
            && lower_hex_raw32_v1(checkpoint.checkpoint_manifest_sha256())
                == slot.checkpoint_manifest_sha256_v2()
            && lower_hex_raw32_v1(checkpoint.checkpoint_payload_sha256())
                == slot.checkpoint_payload_sha256_v2()
            && lower_hex_raw32_v1(checkpoint.model_parameter_sha256())
                == slot.model_parameter_sha256_v2();
        if !matches_authority {
            return Err(PopulationRuntimeResolutionErrorV1::new(
                PopulationRuntimeResolutionErrorKindV1::AuthorityMismatch,
            ));
        }
        handles.push(PopulationSlotOccupantV1::Checkpoint(
            load_native_checkpoint_inference_v1(&run, checkpoint, boundary.payload()).map_err(
                |_| {
                    PopulationRuntimeResolutionErrorV1::new(
                        PopulationRuntimeResolutionErrorKindV1::InferenceInvalid,
                    )
                },
            )?,
        ));
    }

    handles.try_into().map_err(|_| {
        PopulationRuntimeResolutionErrorV1::new(PopulationRuntimeResolutionErrorKindV1::SlotCount)
    })
}

fn population_tranche_refresh_weight_vector_v2(
    manifest: &PopulationTrancheRefreshManifestV2,
) -> Result<PopulationWeightVectorV1> {
    let weights = std::array::from_fn(|index| manifest.slots_v2()[index].weight_units_v2());
    let total = weights
        .iter()
        .try_fold(0_u64, |sum, weight| sum.checked_add(*weight))
        .ok_or_else(|| {
            PopulationRuntimeResolutionErrorV1::new(
                PopulationRuntimeResolutionErrorKindV1::WeightInvalid,
            )
        })?;
    let weights = PopulationWeightVectorV1::new_v1(weights, total).map_err(|_| {
        PopulationRuntimeResolutionErrorV1::new(
            PopulationRuntimeResolutionErrorKindV1::WeightInvalid,
        )
    })?;
    Ok(weights)
}

fn population_manifest_weight_vector_v1(
    manifest: &PopulationRefreshManifestV1,
) -> Result<PopulationWeightVectorV1> {
    population_weight_vector_for_prefix_v1(manifest, POPULATION_OPPONENT_SLOT_COUNT_V1)
}

fn population_response_target_weight_vector_v1(
    manifest: &PopulationRefreshManifestV1,
) -> Result<PopulationWeightVectorV1> {
    population_weight_vector_for_prefix_v1(manifest, POPULATION_RESPONSE_TARGET_SLOT_COUNT_V1)
}

fn population_weight_vector_for_prefix_v1(
    manifest: &PopulationRefreshManifestV1,
    retained_slot_count: usize,
) -> Result<PopulationWeightVectorV1> {
    let weights = std::array::from_fn(|index| {
        if index < retained_slot_count {
            manifest.slots_v1()[index].weight_units_v1()
        } else {
            0
        }
    });
    let total = weights
        .iter()
        .try_fold(0_u64, |sum, weight| sum.checked_add(*weight))
        .ok_or_else(|| {
            PopulationRuntimeResolutionErrorV1::new(
                PopulationRuntimeResolutionErrorKindV1::WeightInvalid,
            )
        })?;
    let weights = PopulationWeightVectorV1::new_v1(weights, total).map_err(|_| {
        PopulationRuntimeResolutionErrorV1::new(
            PopulationRuntimeResolutionErrorKindV1::WeightInvalid,
        )
    })?;
    Ok(weights)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_population_opponent_v1::population_slot_for_episode_v1;
    use crate::native_population_refresh_manifest_v1::{
        build_population_refresh_manifest_v1, PopulationRefreshSlotV1,
    };

    const TEST_WEIGHTS_V1: [u64; POPULATION_OPPONENT_SLOT_COUNT_V1] = [
        125_407, 115_542, 127_252, 127_098, 128_077, 127_916, 122_718, 125_990,
    ];

    fn digest_v1(value: usize) -> String {
        format!("{value:064x}")
    }

    fn manifest_v1() -> PopulationRefreshManifestV1 {
        let roles = [
            "anchor-0",
            "anchor-1",
            "historical-0",
            "historical-1",
            "current-0",
            "current-1",
            "exploiter-0",
            "exploiter-1",
        ];
        let slots = (0..POPULATION_OPPONENT_SLOT_COUNT_V1)
            .map(|index| {
                let (base_seed, generation, run, checkpoint, sidecar, state, model) = match index {
                    0 => (
                        920_012,
                        384,
                        "2c9b7423004428c0e2bb138afafc15ec65957f6bd98c4587bea704fbf9549aae"
                            .to_owned(),
                        "4bd38cf3a9af3fb03fb04428fbc4286d4635007e848c7b9f0740122e430cbba8"
                            .to_owned(),
                        "7511c0377edd4e8d918fa5843f89a0270a8264e5466c329f6b4ef18bbf9e76bb"
                            .to_owned(),
                        "a6c87366b2da9fc33923abab3c0e22d70c884cd9420477df3a475117be6beb99"
                            .to_owned(),
                        "db58dbe3f1f76b5bdf3bae4de657711dc818393b2bf1eeae88c02d8866b4d01d"
                            .to_owned(),
                    ),
                    1 => (
                        920_005,
                        512,
                        "8bc06b6cf2e26df8002b5cece2784e0cd165cdd6bbd199a835e06c17e8d5de5c"
                            .to_owned(),
                        "03f0e226f884f51bf7128f70bec189bd6ac2c8f231ced8886f2cb7d3e936cc90"
                            .to_owned(),
                        "c56a8ba1361ab172c669307084c4522ee06ac79e39b7cf4a306f11effe36b031"
                            .to_owned(),
                        "2904dd7b899c21234c64925440277dbfa8d6f552d8f620b153bc8d16c44f523a"
                            .to_owned(),
                        "0635d2defb8facd700ede34789434956fc4a2fd3b5058cc2df5dd820398b4c22"
                            .to_owned(),
                    ),
                    2 | 3 => (
                        970_003,
                        if index == 2 { 256 } else { 128 },
                        digest_v1(10 + index),
                        digest_v1(20 + index),
                        digest_v1(30 + index),
                        digest_v1(40 + index),
                        digest_v1(50 + index),
                    ),
                    4 | 5 => (
                        970_001 + (index - 4) as u64,
                        512,
                        digest_v1(10 + index),
                        digest_v1(20 + index),
                        digest_v1(30 + index),
                        digest_v1(40 + index),
                        digest_v1(50 + index),
                    ),
                    _ => (
                        980_000 + index as u64,
                        256,
                        digest_v1(10 + index),
                        digest_v1(20 + index),
                        digest_v1(30 + index),
                        digest_v1(40 + index),
                        digest_v1(50 + index),
                    ),
                };
                PopulationRefreshSlotV1::new_v1(
                    index as u64,
                    roles[index],
                    "policy",
                    base_seed,
                    run,
                    generation,
                    512,
                    checkpoint,
                    sidecar,
                    state,
                    model,
                    TEST_WEIGHTS_V1[index],
                )
            })
            .collect();
        build_population_refresh_manifest_v1(0, None, None, slots).unwrap()
    }

    #[test]
    fn response_target_preserves_six_integer_weights_and_exact_total() {
        let weights = population_response_target_weight_vector_v1(&manifest_v1()).unwrap();
        assert_eq!(
            weights.weights_v1(),
            &[125_407, 115_542, 127_252, 127_098, 128_077, 127_916, 0, 0]
        );
        assert_eq!(weights.total_v1(), 751_292);
    }

    #[test]
    fn response_target_never_selects_excluded_slots() {
        let weights = population_response_target_weight_vector_v1(&manifest_v1()).unwrap();
        let mut selected = [false; POPULATION_RESPONSE_TARGET_SLOT_COUNT_V1];
        for episode_index in 0..10_000 {
            let slot = population_slot_for_episode_v1(71_501, episode_index, &weights).unwrap();
            assert!(slot.index_v1() < POPULATION_RESPONSE_TARGET_SLOT_COUNT_V1);
            selected[slot.index_v1()] = true;
        }
        assert!(selected.into_iter().all(|value| value));
    }

    #[test]
    fn response_target_rejects_bad_root_count_before_resolution() {
        let roots = std::array::from_fn::<_, 7, _>(|_| PathBuf::from("unused"));
        assert_eq!(
            resolve_population_response_target_v1(&manifest_v1(), &roots)
                .unwrap_err()
                .kind_v1(),
            PopulationRuntimeResolutionErrorKindV1::SlotCount
        );
        assert_eq!(
            resolve_population_response_target_pairwise_v1(&manifest_v1(), &roots)
                .unwrap_err()
                .kind_v1(),
            PopulationRuntimeResolutionErrorKindV1::SlotCount
        );
    }

    #[test]
    fn ordinary_resolver_weight_path_is_unchanged() {
        let manifest = manifest_v1();
        let weights = population_manifest_weight_vector_v1(&manifest).unwrap();
        assert_eq!(weights.weights_v1(), &TEST_WEIGHTS_V1);
        assert_eq!(weights.total_v1(), 1_000_000);

        let roots = std::array::from_fn::<_, 7, _>(|_| PathBuf::from("unused"));
        assert_eq!(
            resolve_population_opponent_v1(&manifest, &roots)
                .unwrap_err()
                .kind_v1(),
            PopulationRuntimeResolutionErrorKindV1::SlotCount
        );
    }

    // -----------------------------------------------------------------
    // Population program V2 tranche/cycle training resolver
    // (resolve_population_tranche_refresh_opponent_v2, Amendment 4) tests.
    // -----------------------------------------------------------------

    mod tranche_refresh_opponent_v2 {
        use super::*;
        use crate::native_population_refresh_manifest_v1::{
            decode_population_tranche_refresh_manifest_v2, POPULATION_PACKAGE_COMMIT_V2,
            POPULATION_PROGRAM_DOCUMENT_SHA256_V2_PROPOSED,
            POPULATION_TRANCHE_INITIAL_REFRESH_SCHEMA_V2,
        };
        use serde_json::{json, Value};

        fn digest(n: usize) -> String {
            format!("{n:064x}")
        }

        const ROLES: [&str; 8] = [
            "anchor-0",
            "anchor-1",
            "historical-0",
            "historical-1",
            "current-0",
            "current-1",
            "exploiter-0",
            "exploiter-1",
        ];
        const SEEDS: [u64; 8] = [
            920_012, 970_002, 971_221, 971_223, 972_001, 972_002, 971_231, 971_233,
        ];

        fn checkpoint_slot(index: usize, hash_seed: usize) -> serde_json::Value {
            json!({
                "slot_index": index as u64,
                "role": ROLES[index],
                "occupant_class": "policy",
                "source_base_seed": SEEDS[index],
                "source_generation": 384_u64,
                "store_root": format!("D:\\fixture\\tranche-opponent\\slot-{index}"),
                "run_sha256": digest(10 + hash_seed),
                "checkpoint_manifest_sha256": digest(20 + hash_seed),
                "checkpoint_payload_sha256": digest(30 + hash_seed),
                "model_parameter_sha256": digest(40 + hash_seed),
                "weight_units": 125_000_u64,
            })
        }

        fn initial_manifest() -> PopulationTrancheRefreshManifestV2 {
            let slots: Vec<serde_json::Value> = (0..8)
                .map(|index| checkpoint_slot(index, 100 + index))
                .collect();
            let wire = json!({
                "schema": POPULATION_TRANCHE_INITIAL_REFRESH_SCHEMA_V2,
                "program_package_commit_v2": POPULATION_PACKAGE_COMMIT_V2,
                "program_document_sha256_v2_proposed": POPULATION_PROGRAM_DOCUMENT_SHA256_V2_PROPOSED,
                "refresh_index": 0_u64,
                "program_update": 0_u64,
                "global_generation": 0_u64,
                "weight_total_units": 1_000_000_u64,
                "previous_manifest_sha256": Value::Null,
                "pool_manifest_sha256": digest(1),
                "payoff_panel_sha256": Value::Null,
                "slots": slots,
            });
            let bytes = serde_json::to_vec(&wire).unwrap();
            decode_population_tranche_refresh_manifest_v2(&bytes, None).unwrap()
        }

        fn matching_roots(manifest: &PopulationTrancheRefreshManifestV2) -> [PathBuf; 8] {
            std::array::from_fn(|index| PathBuf::from(manifest.slots_v2()[index].store_root_v2()))
        }

        #[test]
        fn weight_vector_retains_all_eight_slots() {
            let manifest = initial_manifest();
            let weights = population_tranche_refresh_weight_vector_v2(&manifest).unwrap();
            assert_eq!(weights.weights_v1(), &[125_000; 8]);
            assert_eq!(weights.total_v1(), 1_000_000);
        }

        #[test]
        fn resolver_rejects_bad_root_count_before_any_filesystem_access() {
            let manifest = initial_manifest();
            let roots = std::array::from_fn::<_, 7, _>(|_| PathBuf::from("unused"));
            assert_eq!(
                resolve_population_tranche_refresh_opponent_v2(&manifest, &roots)
                    .unwrap_err()
                    .kind_v1(),
                PopulationRuntimeResolutionErrorKindV1::SlotCount
            );
        }

        #[test]
        fn a_relocated_store_root_is_not_rejected_before_filesystem_access() {
            // Proves the fix: slot_store_roots is a physical LOCATION,
            // never compared against the manifest's own frozen store_root
            // string (this program's real, live-caught motivation: the
            // 8/25 evidence cleanup relocated several C:\ store roots,
            // permanently frozen in already-sealed historical manifests,
            // to E:\ archive paths). A caller-supplied path that differs
            // from the manifest's own declared string must NOT be rejected
            // by that difference alone -- resolution proceeds to the real
            // Store read (RunRead here, since this relocated-looking path
            // does not actually exist on this host either), never
            // AuthorityMismatch from a string comparison that no longer
            // exists.
            let manifest = initial_manifest();
            let mut roots = matching_roots(&manifest);
            assert_ne!(roots[3].to_str(), Some("D:\\relocated\\root"));
            roots[3] = PathBuf::from("D:\\relocated\\root");
            assert_eq!(
                resolve_population_tranche_refresh_opponent_v2(&manifest, &roots)
                    .unwrap_err()
                    .kind_v1(),
                PopulationRuntimeResolutionErrorKindV1::RunRead
            );
        }

        #[test]
        fn resolver_reaches_live_store_loading_once_cheap_checks_pass() {
            let manifest = initial_manifest();
            let roots = matching_roots(&manifest);
            assert_eq!(
                resolve_population_tranche_refresh_opponent_v2(&manifest, &roots)
                    .unwrap_err()
                    .kind_v1(),
                PopulationRuntimeResolutionErrorKindV1::RunRead
            );
        }

        // A resolver-level "search slot resolves without touching the
        // filesystem, end to end" unit test was attempted here and
        // withdrawn, disclosed rather than silently dropped: this
        // function's loop is sequential and fails closed on the FIRST
        // slot with a problem, and slots 0-5 in a decodable eight-slot
        // manifest must be ordinary Checkpoint occupants (search is
        // restricted to 6/7 by the validator), so proving slot 6 is
        // reached AND handled correctly in one call would require slots
        // 0-5 to resolve against real on-disk Stores -- out of proportion
        // for this unit. The search branch itself is a direct,
        // line-for-line mirror of `resolve_population_handles_v1`'s own
        // already-established branch (same module, reviewed above); the
        // manifest-decoder's own test module already thoroughly covers
        // every search-occupancy validation rule (tier, cap, index,
        // authorized seed, presence); and Task 7's real preflight run
        // exercises this exact code path end to end against a genuine
        // Store and a genuine searcher draw, which is stronger evidence
        // than a synthetic unit test could provide here anyway.
    }

    /// Diagnostic aid (not a gate): the real refresh-19 launch failed at
    /// this module's resolver with a bare `RunInvalid`, which does not say
    /// which of the 8 real slot roots is the offender. Manually replicates
    /// `resolve_population_tranche_refresh_handles_v2`'s own loop against
    /// the real, on-disk manifest index 18 and the real 8 slot roots the
    /// launcher supplies, printing a per-slot outcome instead of failing
    /// closed on the first problem. Run explicitly with --nocapture;
    /// skips (does not fail) if the real paths are absent on this host.
    #[test]
    #[ignore = "diagnostic aid: run explicitly with --nocapture"]
    fn diagnose_refresh19_opponent_pool_resolution_v1() {
        // The real decoder enforces chain continuity (previous_manifest_sha256
        // etc.), so index 18 cannot decode standalone with `previous: None`
        // -- walk the real chain from index 0 exactly like the real
        // dispatch/preflight do, and use the last (index 18) link.
        let dir = r"E:\mtg-kernel-population-v2-cycle3\refresh-manifests";
        let path0 = format!("{dir}\\population-v3-refresh-000.json");
        let Ok(bytes0) = fs::read(&path0) else {
            eprintln!("skipping: {path0} not present on this host");
            return;
        };
        let mut previous = crate::native_population_refresh_manifest_v1::decode_population_tranche_refresh_manifest_v2(&bytes0, None)
            .expect("refresh_index 0 must decode standalone");
        for idx in 1_u64..=18 {
            let path = format!("{dir}\\population-v3-refresh-{idx:03}.json");
            let bytes = fs::read(&path).unwrap_or_else(|_| panic!("{path} must be present"));
            previous = crate::native_population_refresh_manifest_v1::decode_population_tranche_refresh_manifest_v2(&bytes, Some(&previous))
                .unwrap_or_else(|error| panic!("refresh_index {idx} must chain-decode: {error:?}"));
        }
        let manifest = previous;

        let slot_roots: [&str; 8] = [
            r"D:\mtg-kernel-ladder-pilot-20260725\pool3\primary",
            r"D:\mtg-kernel-scaled-selfplay-population-v1\replay\three-lineage-replay\attempt-001\wave-00-seed-970002-gpu1\run-0\store",
            r"D:\mtg-kernel-denovo-campaign-v1\seed-971221\denovo-1024-screen-build\attempt-001\denovo-1024-store\run-0\store",
            r"D:\mtg-kernel-denovo-campaign-v1\seed-971223\denovo-1024-screen-build\attempt-002\denovo-1024-store\run-0\store",
            r"D:\throughput-remeasure-20260825\v2-resume-walk\store-depth2048-cycle2",
            r"E:\mtg-kernel-population-v2-cycle3\parent-import\current-1-seed-975002-store\run-0\store",
            r"D:\mtg-kernel-denovo-campaign-v1\seed-971222\denovo-1024-screen-build\attempt-001\denovo-1024-store\run-0\store",
            r"D:\mtg-kernel-denovo-campaign-v1\seed-971221\denovo-1024-screen-build\attempt-001\denovo-1024-store\run-0\store",
        ];

        for (i, (slot, root)) in manifest
            .slots_v2()
            .iter()
            .zip(slot_roots.iter())
            .enumerate()
        {
            let store_root = PathBuf::from(root);
            let outcome = (|| -> Result<()> {
                let run_bytes = fs::read(store_root.join("run.json")).map_err(|_| {
                    PopulationRuntimeResolutionErrorV1::new(
                        PopulationRuntimeResolutionErrorKindV1::RunRead,
                    )
                })?;
                let run = decode_train_run_v2(&run_bytes).map_err(|_| {
                    PopulationRuntimeResolutionErrorV1::new(
                        PopulationRuntimeResolutionErrorKindV1::RunInvalid,
                    )
                })?;
                let root_opened = ValidatedNativeTrainingStoreRootV2::open_v2(&store_root)
                    .map_err(|_| {
                        PopulationRuntimeResolutionErrorV1::new(
                            PopulationRuntimeResolutionErrorKindV1::RootInvalid,
                        )
                    })?;
                let boundary = load_native_training_boundary_v2(
                    &root_opened,
                    &run,
                    slot.source_generation_v2(),
                )
                .map_err(|_| {
                    PopulationRuntimeResolutionErrorV1::new(
                        PopulationRuntimeResolutionErrorKindV1::BoundaryInvalid,
                    )
                })?;
                let checkpoint = boundary.checkpoint();
                let matches_authority = run.run_sha256() == slot.run_sha256_v2()
                    && run.record().schedule.base_seed == slot.source_base_seed_v2()
                    && checkpoint.generation_index() == slot.source_generation_v2()
                    && lower_hex_raw32_v1(checkpoint.checkpoint_manifest_sha256())
                        == slot.checkpoint_manifest_sha256_v2()
                    && lower_hex_raw32_v1(checkpoint.checkpoint_payload_sha256())
                        == slot.checkpoint_payload_sha256_v2()
                    && lower_hex_raw32_v1(checkpoint.model_parameter_sha256())
                        == slot.model_parameter_sha256_v2();
                if !matches_authority {
                    eprintln!(
                        "slot {i}: run_sha256 match={} base_seed match={} ({} vs {}) generation match={} ({} vs {}) manifest_sha match={} payload_sha match={} model_sha match={}",
                        run.run_sha256() == slot.run_sha256_v2(),
                        run.record().schedule.base_seed == slot.source_base_seed_v2(),
                        run.record().schedule.base_seed,
                        slot.source_base_seed_v2(),
                        checkpoint.generation_index() == slot.source_generation_v2(),
                        checkpoint.generation_index(),
                        slot.source_generation_v2(),
                        lower_hex_raw32_v1(checkpoint.checkpoint_manifest_sha256()) == slot.checkpoint_manifest_sha256_v2(),
                        lower_hex_raw32_v1(checkpoint.checkpoint_payload_sha256()) == slot.checkpoint_payload_sha256_v2(),
                        lower_hex_raw32_v1(checkpoint.model_parameter_sha256()) == slot.model_parameter_sha256_v2(),
                    );
                    return Err(PopulationRuntimeResolutionErrorV1::new(
                        PopulationRuntimeResolutionErrorKindV1::AuthorityMismatch,
                    ));
                }
                Ok(())
            })();
            match outcome {
                Ok(()) => println!("slot {i} ({}): OK", slot.role_v2()),
                Err(error) => println!(
                    "slot {i} ({}): FAILED kind={:?}",
                    slot.role_v2(),
                    error.kind_v1()
                ),
            }
        }
    }

    /// Follow-up diagnostic (not a gate): the resolver-level replay above
    /// still reports `RunInvalid` for the four denovo-1024 slots even after
    /// the Amendment 5 port landed, and manual field-by-field comparison
    /// against a known-working real "denovo-screen" record (seed 971_201,
    /// `D:\mtg-kernel-denovo-screen-v1\...`) found every response-exploiter-
    /// specific and every other contracts/environment/schedule field
    /// byte-identical in shape (only the seed/role/array fields, and
    /// digests inherently derived from them, legitimately differ). Calls
    /// `decode_train_run_v2` directly (bypassing the resolver's blanket
    /// `RunInvalid` mapping) to get the real `TrainRunV2ErrorKind`.
    #[test]
    #[ignore = "diagnostic aid: run explicitly with --nocapture"]
    fn diagnose_denovo_1024_decode_error_kind_v1() {
        let paths = [
            r"D:\mtg-kernel-denovo-campaign-v1\seed-971221\denovo-1024-screen-build\attempt-001\denovo-1024-store\run-0\store\run.json",
            r"D:\mtg-kernel-denovo-campaign-v1\seed-971223\denovo-1024-screen-build\attempt-002\denovo-1024-store\run-0\store\run.json",
            r"D:\mtg-kernel-denovo-campaign-v1\seed-971222\denovo-1024-screen-build\attempt-001\denovo-1024-store\run-0\store\run.json",
        ];
        for path in paths {
            let Ok(bytes) = fs::read(path) else {
                eprintln!("skipping: {path} not present on this host");
                continue;
            };
            match decode_train_run_v2(&bytes) {
                Ok(validated) => {
                    println!("{path}: DECODED OK, run_sha256={}", validated.run_sha256())
                }
                Err(error) => println!(
                    "{path}: FAILED kind={:?} code={}",
                    error.kind(),
                    error.code()
                ),
            }
        }
    }
}
