//! Task D16: static owner-view page generator for the M4 cube draft-and-build
//! learner (D0.21, design note sections 11b and 12a, `m4-interfaces/v2`).
//!
//! Renders six independent static HTML pages in namespace `owner-view`, one
//! per shakedown run id (`b1-u0032`, `b1-u0064`, `b1-u0096`, `b1-u0128`,
//! `k1-u0064`, `k1-u0128`), following D0.21's exact sample plan:
//! 1. First-pick ranking over drafts 0 to 99 (800 fresh 15-card packs).
//! 2. 20 sample picks from drafts 100 to 119 (with lead ruling L4 on `m = 8`).
//! 3. 20 sample pools and sampled builds from drafts 120 to 139 (`k = 0`).
//! 4. Distributions of `B` (`0..=40`) and `|T|` (`0..=5`) over 3,200 sampled
//!    builds from drafts 200 to 299 (`k = 0..=3`).
//!
//! No game is played. Every page is a pure function of its checkpoint and its
//! own `owner-view` seeds, never depending on a dose read or on another page.

pub mod checkpoint;
pub mod cli;
pub mod html;
pub mod if2;
pub mod sample_plan;

#[cfg(test)]
mod tests;

pub use checkpoint::{
    format_head_training_label, validate_head_updates_raw_json, CheckpointHeader,
    CheckpointHeaderError, FixtureCheckpointFile, FixtureStandInParams, HeadUpdates,
    OwnerViewSeatFactory, StandInCheckpoint, CHECKPOINT_SCHEMA_V1, FIXTURE_MODEL_IDENTITY_V1,
    HEAD_DISPLAY_NAMES, HEAD_UPDATE_KEYS,
};
pub use html::{format_probability, render_owner_view_html};
pub use sample_plan::{
    cube_pool_from_ordered_ids, generate_owner_view_data, is_valid_token,
    learned_row_option_counts, ordered_pool_sha256, owner_view_build_seed, owner_view_draft_seed,
    resolve_page_spec, sample_pick_spec, sample_pool_spec, seed_deal_v1, summarize_card,
    BuildDistributions, CardSummary, FirstPickRankRow, HeadLabelRow, OwnerViewError,
    OwnerViewPageData, OwnerViewPageSpec, PickerMode, SamplePickEntry, SamplePickOption,
    SamplePickSpec, SamplePoolEntry, SamplePoolSpec, SeedPart, DIST_BUILDS_PER_CHAIR,
    DIST_DRAFT_COUNT, DIST_DRAFT_START, DIST_TOTAL_BUILDS, FIRST_PICK_DRAFT_COUNT,
    FIRST_PICK_DRAFT_START, FIRST_PICK_MIN_PACKS, FIRST_PICK_PACK_COUNT, LAW_BOUNDARY_NOTICE,
    OWNER_VIEW_NAMESPACE, SAMPLED_BUILDS_LABEL, SAMPLE_PICK_COUNT, SAMPLE_PICK_DRAFT_START,
    SAMPLE_POOL_BUILD_K, SAMPLE_POOL_COUNT, SAMPLE_POOL_DRAFT_START, SHAKEDOWN_LABEL,
    SHAKEDOWN_OWNER_VIEW_RUNS, SHAKEDOWN_PAGE_SPECS, UNIFORM_PICKS_LABEL,
};

use crate::cube_draft_v1::CubePool;

/// Resolves `run_id` through [`resolve_page_spec`], validates the checkpoint
/// and pinned ordered pool, runs D0.21's exact sample plan with the declared
/// picker mode, and renders the self-contained static HTML page.
pub fn render_owner_view_page(
    run_id: &str,
    pool: &CubePool,
    ordered_pool_sha256: &str,
    checkpoint: &CheckpointHeader,
    factory: &dyn OwnerViewSeatFactory,
) -> Result<(OwnerViewPageData, String), OwnerViewError> {
    let spec = resolve_page_spec(run_id)?;
    let data = generate_owner_view_data(
        run_id,
        spec.picker_mode,
        pool,
        ordered_pool_sha256,
        checkpoint,
        factory,
    )?;
    let html = render_owner_view_html(&data);
    Ok((data, html))
}
