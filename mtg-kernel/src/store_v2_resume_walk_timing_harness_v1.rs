//! MEASUREMENT HARNESS ONLY -- not part of the product surface.
//!
//! Throughput remeasure task (2026-08-25, branch
//! `fable/throughput-remeasure-v1`): prices the wall-clock cost of the
//! StoreV2 genesis-to-latest resume/validation walk
//! (`walk_complete_store_v2` in `native_training_store_resume_v2.rs`) as a
//! function of store depth (`generation_index`).
//!
//! This calls the exact production read-only entry point
//! `validate_native_training_store_v2`, which itself does nothing but
//! recapture the root, take the shared (non-mutating) reader lock, and run
//! one full `walk_complete_store_v2` pass -- the identical validation chain
//! `resume_native_training_store_v2` runs internally on every training
//! window (see `native_science_loop_v1.rs`, the `loop { resume_native_...
//! }` around line 704). Using the shared-lock reader entry point instead of
//! the mutator keeps this harness provably read-only with respect to the
//! Store on disk: no file is ever written, renamed, or deleted by this
//! module.
//!
//! Not exercised: the `reconstruct_executor_v2` cost `resume_...`'s
//! `Continue` branch pays on top of one walk, and the extra confirmatory
//! `reread` walk its `Complete` branch pays on top of the first walk. Both
//! are depth-independent (roughly O(1) in `generation_index`), so excluding
//! them does not distort the depth-scaling question this harness exists to
//! answer, but it does mean the numbers here underestimate a real
//! `resume_native_training_store_v2` call by one extra walk (the `Complete`
//! no-op path) or by one reconstruction (the `Continue` path).
//!
//! Invocation (ignored by default; run explicitly):
//! ```text
//! set MTG_KERNEL_TIMING_HARNESS_STORE_ROOT=D:\path\to\store
//! set MTG_KERNEL_TIMING_HARNESS_REPEATS=3
//! cargo test --release --features native-training-store-v2-production \
//!     --lib store_v2_resume_walk_timing_harness_v1 -- --ignored --nocapture
//! ```

#[cfg(test)]
mod tests {
    use crate::native_training_store_resume_v2::validate_native_training_store_v2;
    use crate::native_training_store_root_v2::ValidatedNativeTrainingStoreRootV2;
    use crate::native_training_store_run_v2::decode_train_run_v2;
    use std::time::Instant;

    /// Reads `MTG_KERNEL_TIMING_HARNESS_STORE_ROOT`, opens that path as a
    /// Store root, decodes its `run.json`, then calls the real read-only
    /// `validate_native_training_store_v2` walk `MTG_KERNEL_TIMING_HARNESS_REPEATS`
    /// times (default 3), printing each wall time in microseconds plus the
    /// proven `latest_generation_index` so the caller can bind the timing to
    /// a depth without trusting an external label.
    #[test]
    #[ignore = "measurement harness: needs MTG_KERNEL_TIMING_HARNESS_STORE_ROOT set to a real Store copy"]
    fn measure_walk_complete_store_v2_wall_time_v1() {
        let root_path = std::env::var("MTG_KERNEL_TIMING_HARNESS_STORE_ROOT")
            .expect("set MTG_KERNEL_TIMING_HARNESS_STORE_ROOT to a Store root directory");
        let repeats: u32 = std::env::var("MTG_KERNEL_TIMING_HARNESS_REPEATS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(3);

        println!("harness_store_root={root_path}");

        let run_json_path = std::path::Path::new(&root_path).join("run.json");
        let run_bytes = std::fs::read(&run_json_path).unwrap_or_else(|error| {
            panic!("harness_error=read_run_json path={run_json_path:?} error={error}")
        });
        let run = decode_train_run_v2(&run_bytes).unwrap_or_else(|error| {
            panic!("harness_error=decode_train_run_v2 error={error}")
        });

        let root = ValidatedNativeTrainingStoreRootV2::open_v2(&root_path).unwrap_or_else(|error| {
            panic!("harness_error=open_v2 code={} error={error}", error.code())
        });

        for repeat_index in 0..repeats {
            let started = Instant::now();
            let state = validate_native_training_store_v2(&root, &run).unwrap_or_else(|error| {
                panic!(
                    "harness_error=validate_native_training_store_v2 code={} error={error}",
                    error.kind().code()
                )
            });
            let elapsed = started.elapsed();
            println!(
                "harness_result repeat={repeat_index} latest_generation_index={} elapsed_micros={} elapsed_secs={:.6}",
                state.latest_generation_index(),
                elapsed.as_micros(),
                elapsed.as_secs_f64(),
            );
        }
    }
}
