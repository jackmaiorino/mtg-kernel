//! Ignored, test-only empirical probe for the model-guided-searcher
//! deterministic-CPU-forward audit
//! (`docs/audits/model_guided_forward_determinism_audit_v1.md`,
//! `CLAUDE-MODEL-GUIDED-SEARCHER-DESIGN-V1.md` Section 1.5 and Section 5.3
//! item 3).
//!
//! This module adds no production path, no training path, and no store
//! schema. It loads one real checkpoint (the de-novo screen store, generation
//! 256) through the exact same public checkpoint-inference boundary the
//! checkpoint runner and search-opponent dispatch already use
//! (`native_checkpoint_inference_v1::NativeCheckpointInferenceV1::score_decision_v1`,
//! itself backed by `native_policy_value_net_v1::NativePolicyValueNetV1::forward_v1`),
//! and hashes the forward outputs over a fixed input battery under three
//! empirical conditions the audit report requires: repeated in-process calls,
//! separate process invocations of the same compiled test binary, and
//! concurrent calls from multiple OS threads sharing one loaded model. A
//! fourth test reads the MXCSR control/status register immediately before and
//! after a forward call to check the FTZ/DAZ and rounding-mode state
//! empirically.
//!
//! All tests are `#[ignore]`d: they require the real de-novo store on `D:`,
//! which does not exist in a clean checkout or in CI, and they are slow
//! (1000 in-process repeats plus subprocess spawns). Run explicitly with
//! `cargo test --release -- --ignored native_forward_determinism_probe_v1`.

#[cfg(test)]
mod tests {
    use crate::flat_policy_v2::{
        FlatGlobalsV2, FlatPhaseV2, FlatPlayerGlobalsV2, FlatRelativePlayerV2,
        FlatScorerActionCoreV2, FlatScoringDecisionViewV2,
    };
    use crate::native_checkpoint_inference_v1::NativeCheckpointInferenceV1;
    use crate::native_ladder_pool_resolution_v1::{
        resolve_ladder_checkpoint_ref_v1, stage_ladder_checkpoint_ref_v1,
    };
    use sha2::{Digest, Sha256};
    use std::path::Path;
    use std::sync::Arc;

    /// Real de-novo screen store, attempt 002, run 0. Fixed by the audit
    /// task, not configurable: the point is to exercise one real, on-disk,
    /// trained checkpoint, not a synthetic fixture.
    const DENOVO_STORE_ROOT_V1: &str =
        r"D:\mtg-kernel-denovo-screen-v1\denovo-screen-build\attempt-002\denovo-store\run-0\store";
    const DENOVO_GENERATION_V1: u64 = 256;

    /// Env var the parent test process sets so a spawned copy of this same
    /// test binary knows to run as a one-shot worker instead of the full
    /// `cargo test` sweep. This is how property (b) (bit-identity across
    /// process invocations) is checked: the parent spawns
    /// `std::env::current_exe()` (the compiled test binary itself) three
    /// times with libtest's own `--exact --ignored --nocapture` selecting
    /// just the worker test below, and compares the printed hash lines.
    const SUBPROCESS_MARKER_ENV_V1: &str = "MGS_FORWARD_PROBE_SUBPROCESS_V1";
    const SUBPROCESS_HASH_PREFIX_V1: &str = "MGS_FORWARD_PROBE_HASH_V1=";

    fn load_probe_checkpoint_v1() -> NativeCheckpointInferenceV1 {
        let base_dir = Path::new(DENOVO_STORE_ROOT_V1);
        let checkpoint_ref = stage_ladder_checkpoint_ref_v1(base_dir, DENOVO_GENERATION_V1)
            .unwrap_or_else(|error| {
                panic!(
                    "stage checkpoint ref from real de-novo store at {DENOVO_STORE_ROOT_V1} \
                     generation {DENOVO_GENERATION_V1}: {error:?}"
                )
            });
        resolve_ladder_checkpoint_ref_v1(base_dir, &checkpoint_ref).unwrap_or_else(|error| {
            panic!("resolve checkpoint authority from real de-novo store: {error:?}")
        })
    }

    fn player_globals_v1(
        life: i32,
        mana: [u8; 6],
        hand_count: u64,
        library_count: u64,
    ) -> FlatPlayerGlobalsV2 {
        FlatPlayerGlobalsV2 {
            life,
            mana,
            hand_count,
            library_count,
            draws_this_turn: 1,
            spells_cast_this_turn: 0,
            lands_played_this_turn: 0,
            ..FlatPlayerGlobalsV2::default()
        }
    }

    /// Fixed input battery: several distinct (globals, actions) decisions
    /// spanning a minimal fixture, moderate realistic-shaped values, many
    /// actions, and boundary-ish integer globals. No objects, relations, or
    /// non-`Pass` actions are populated: `native_flat_tensorizer_v2`'s
    /// action-kind and relation/action-reference projection has its own
    /// extensive cross-validation (confirmed empirically, see the item-2
    /// comment below) that is orthogonal to this audit's target (the net's
    /// forward arithmetic, not the tensorizer), so this battery stays inside
    /// the always-valid zero-object, `Pass`-only shape and instead gets its
    /// input diversity from varied global scalar fields and action counts.
    /// This still drives real, non-degenerate float traffic through
    /// `state_encoder`, `action_encoder`, `scorer`, and `value_head`, which
    /// use the exact same `linear_rows_v1` / `tanh_in_place_v1` primitives as
    /// `object_encoder`, `edge_encoder`, and `node_update` (verified by
    /// reading `native_policy_value_net_v1.rs`; see the audit report's
    /// implementation enumeration). The audit report records the missing
    /// object/edge/node-update exercise as a documented scope limit covered
    /// by code reading instead of direct empirical exercise, not a silent
    /// gap.
    fn battery_v1() -> Vec<(FlatGlobalsV2, Vec<FlatScorerActionCoreV2>)> {
        vec![
            // 1. Minimal: the same shape the existing crate-internal fixture
            //    (`native_checkpoint_inference_v1.rs`'s own tests) already
            //    proves valid.
            (
                FlatGlobalsV2 {
                    acting_player: FlatRelativePlayerV2::SelfPlayer,
                    ..FlatGlobalsV2::default()
                },
                vec![FlatScorerActionCoreV2::default()],
            ),
            // 2. Moderate, realistic-shaped globals with several legal-shaped
            //    Pass actions. Non-Pass action kinds (`PlayLand`, `CastSpell`,
            //    `ActivateAbility`, `ChooseEffectNumber`, ...) turned out to
            //    require a nonempty `action_refs` binding (a source/target
            //    object reference) to pass `native_flat_tensorizer_v2`'s own
            //    decision-shape validation; this battery deliberately stays
            //    inside the always-valid zero-`action_refs` shape (see the
            //    battery's own doc comment above), so every action here uses
            //    `FlatScorerActionKindV2::Pass`, which needs no reference.
            //    This was confirmed empirically: an earlier revision of this
            //    battery that used `PlayLand`/`CastSpell`/`ActivateAbility`
            //    with `ref_len: 0` failed `score_decision_v1` with
            //    `NativeCheckpointInferenceErrorKindV1::DecisionInvalid`,
            //    which is itself a fact about the tensorizer's contract, not
            //    about forward-pass determinism.
            (
                FlatGlobalsV2 {
                    acting_player: FlatRelativePlayerV2::SelfPlayer,
                    active_player: FlatRelativePlayerV2::SelfPlayer,
                    priority_player: FlatRelativePlayerV2::SelfPlayer,
                    initiative: FlatRelativePlayerV2::Opponent,
                    phase: FlatPhaseV2::Main1,
                    players: [
                        player_globals_v1(20, [1, 0, 2, 0, 0, 3], 5, 34),
                        player_globals_v1(17, [0, 1, 0, 1, 1, 0], 6, 31),
                    ],
                    ..FlatGlobalsV2::default()
                },
                vec![
                    FlatScorerActionCoreV2::default(),
                    FlatScorerActionCoreV2::default(),
                    FlatScorerActionCoreV2::default(),
                    FlatScorerActionCoreV2::default(),
                ],
            ),
            // 3. Many actions (still all `Pass`), against item 2's exact
            //    `acting_player`/`active_player`/`priority_player`/
            //    `initiative`/`phase` shape (only the leaf numeric player
            //    fields and action count vary). An earlier revision of this
            //    entry independently varied `acting_player` away from
            //    `priority_player` and dropped `initiative`, and that also
            //    failed `score_decision_v1` with `DecisionInvalid` --
            //    confirming the tensorizer validates cross-field consistency
            //    among the player-role globals, not just per-field ranges.
            //    This battery deliberately does not chase that validation
            //    surface further: it is orthogonal to the forward-pass
            //    arithmetic this audit targets.
            (
                FlatGlobalsV2 {
                    acting_player: FlatRelativePlayerV2::SelfPlayer,
                    active_player: FlatRelativePlayerV2::SelfPlayer,
                    priority_player: FlatRelativePlayerV2::SelfPlayer,
                    initiative: FlatRelativePlayerV2::Opponent,
                    phase: FlatPhaseV2::Main1,
                    players: [
                        player_globals_v1(1, [3, 2, 1, 0, 4, 0], 2, 15),
                        player_globals_v1(30, [0, 0, 0, 0, 0, 2], 8, 22),
                    ],
                    ..FlatGlobalsV2::default()
                },
                (0..24).map(|_| FlatScorerActionCoreV2::default()).collect(),
            ),
            // 4. A modestly different life/mana/hand/library spread on a
            //    small action set, again staying inside item 2's exact
            //    proven-valid globals field shape.
            (
                FlatGlobalsV2 {
                    acting_player: FlatRelativePlayerV2::SelfPlayer,
                    active_player: FlatRelativePlayerV2::SelfPlayer,
                    priority_player: FlatRelativePlayerV2::SelfPlayer,
                    initiative: FlatRelativePlayerV2::Opponent,
                    phase: FlatPhaseV2::Main1,
                    players: [
                        player_globals_v1(5, [0, 0, 0, 0, 0, 0], 0, 40),
                        player_globals_v1(40, [4, 4, 4, 4, 4, 4], 7, 20),
                    ],
                    ..FlatGlobalsV2::default()
                },
                vec![
                    FlatScorerActionCoreV2::default(),
                    FlatScorerActionCoreV2::default(),
                ],
            ),
        ]
    }

    /// Scores every decision in the battery in order and folds the raw
    /// output bits (both logits and value, via `to_bits()`, never via any
    /// float comparison) into one SHA-256 digest. Byte-identical inputs and
    /// byte-identical forward arithmetic produce a byte-identical digest;
    /// anything else is the finding.
    fn hash_battery_v1(
        inference: &NativeCheckpointInferenceV1,
        battery: &[(FlatGlobalsV2, Vec<FlatScorerActionCoreV2>)],
    ) -> [u8; 32] {
        let mut hasher = Sha256::new();
        for (globals, actions) in battery {
            let view = FlatScoringDecisionViewV2::new(
                globals,
                &[],
                &[],
                &[],
                &[],
                &[],
                &[],
                &[],
                &[],
                actions,
                &[],
            );
            let output = inference
                .score_decision_v1(view)
                .expect("forward must succeed on every fixed battery input");
            for logit in output.action_logits() {
                hasher.update(logit.to_bits().to_be_bytes());
            }
            hasher.update(output.value().to_bits().to_be_bytes());
        }
        hasher.finalize().into()
    }

    /// Search-scoped sibling of [`hash_battery_v1`]: identical battery,
    /// identical hashing technique, but scores through
    /// `NativeCheckpointInferenceV1::score_decision_search_deterministic_v1`
    /// (the new search-scoped forward variant,
    /// `native_policy_value_net_v1::NativePolicyValueNetV1::forward_search_deterministic_v1`,
    /// which routes activation through
    /// `deterministic_math_v1::tanh_f32_v1` instead of `f32::tanh()`)
    /// instead of `score_decision_v1`. Proves the variant's OWN bit
    /// stability; it is not compared against `hash_battery_v1`'s hash here
    /// (the two are expected to differ in general, since the variant uses
    /// a deliberately different, libm-free `tanh` -- see the max-ULP
    /// comparison test below for that measurement).
    fn hash_battery_search_deterministic_v1(
        inference: &NativeCheckpointInferenceV1,
        battery: &[(FlatGlobalsV2, Vec<FlatScorerActionCoreV2>)],
    ) -> [u8; 32] {
        let mut hasher = Sha256::new();
        for (globals, actions) in battery {
            let view = FlatScoringDecisionViewV2::new(
                globals,
                &[],
                &[],
                &[],
                &[],
                &[],
                &[],
                &[],
                &[],
                actions,
                &[],
            );
            let output = inference
                .score_decision_search_deterministic_v1(view)
                .expect("search-deterministic forward must succeed on every fixed battery input");
            for logit in output.action_logits() {
                hasher.update(logit.to_bits().to_be_bytes());
            }
            hasher.update(output.value().to_bits().to_be_bytes());
        }
        hasher.finalize().into()
    }

    /// Flattens every logit and the value from every battery decision, in
    /// battery order, scored through either the production libm path or
    /// the search-scoped deterministic-math variant. Used only by the
    /// max-ULP comparison test below.
    fn battery_outputs_v1(
        inference: &NativeCheckpointInferenceV1,
        battery: &[(FlatGlobalsV2, Vec<FlatScorerActionCoreV2>)],
        use_search_deterministic_variant: bool,
    ) -> Vec<f32> {
        let mut values = Vec::new();
        for (globals, actions) in battery {
            let view = FlatScoringDecisionViewV2::new(
                globals,
                &[],
                &[],
                &[],
                &[],
                &[],
                &[],
                &[],
                &[],
                actions,
                &[],
            );
            let output = if use_search_deterministic_variant {
                inference.score_decision_search_deterministic_v1(view)
            } else {
                inference.score_decision_v1(view)
            }
            .expect("forward must succeed on every fixed battery input");
            values.extend_from_slice(output.action_logits());
            values.push(output.value());
        }
        values
    }

    /// Maps an `f32` to a signed integer key such that the usual `f32`
    /// total order (ignoring NaN) corresponds exactly to the integer order
    /// of the keys, and adjacent representable `f32` values map to keys
    /// exactly 1 apart. Standard technique (Bruce Dawson's "comparing
    /// floating point numbers"): non-negative floats' bit patterns are
    /// already monotonic as signed integers; negative floats' bit patterns
    /// sort backwards, so they are remapped by reflecting through
    /// `i32::MIN`.
    fn ordered_key_v1(value: f32) -> i64 {
        let bits = value.to_bits() as i32;
        let ordered = if bits < 0 {
            i32::MIN.wrapping_sub(bits)
        } else {
            bits
        };
        ordered as i64
    }

    /// Distance, in ULPs, between two (non-NaN) `f32` values, via
    /// [`ordered_key_v1`].
    fn ulp_distance_v1(a: f32, b: f32) -> u64 {
        (ordered_key_v1(a) - ordered_key_v1(b)).unsigned_abs()
    }

    fn hex_v1(bytes: &[u8]) -> String {
        let mut out = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            out.push_str(&format!("{byte:02x}"));
        }
        out
    }

    // ----------------------------------------------------------------
    // Property (a): repeated calls in one process.
    // ----------------------------------------------------------------
    #[test]
    #[ignore = "requires the real de-novo checkpoint store on D:; run explicitly for the forward-determinism audit"]
    fn forward_is_byte_identical_across_1000_repeated_calls_in_one_process_v1() {
        let inference = load_probe_checkpoint_v1();
        let battery = battery_v1();
        let first = hash_battery_v1(&inference, &battery);
        for iteration in 0..1000u32 {
            let hash = hash_battery_v1(&inference, &battery);
            assert_eq!(
                hash, first,
                "in-process repeat {iteration} diverged from the first call's hash"
            );
        }
        println!("in_process_1000x_battery_hash_hex={}", hex_v1(&first));
    }

    // ----------------------------------------------------------------
    // Property (b): across separate process invocations.
    // ----------------------------------------------------------------
    #[test]
    #[ignore = "requires the real de-novo checkpoint store on D:; run explicitly for the forward-determinism audit"]
    fn forward_is_byte_identical_across_three_process_invocations_v1() {
        let inference = load_probe_checkpoint_v1();
        let battery = battery_v1();
        let parent_hash = hash_battery_v1(&inference, &battery);
        println!("parent_process_battery_hash_hex={}", hex_v1(&parent_hash));

        let exe = std::env::current_exe().expect("current_exe must resolve for the test binary");
        for attempt in 0..3u32 {
            let output = std::process::Command::new(&exe)
                .arg("--ignored")
                .arg("--exact")
                .arg("--nocapture")
                .arg("native_forward_determinism_probe_v1::tests::forward_probe_subprocess_worker_v1")
                .env(SUBPROCESS_MARKER_ENV_V1, "1")
                .output()
                .expect("subprocess spawn must succeed");
            assert!(
                output.status.success(),
                "subprocess {attempt} exited non-zero: stdout={} stderr={}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            let stdout = String::from_utf8_lossy(&output.stdout);
            let hash_line = stdout
                .lines()
                .find(|line| line.starts_with(SUBPROCESS_HASH_PREFIX_V1))
                .unwrap_or_else(|| {
                    panic!("subprocess {attempt} printed no hash line; stdout was: {stdout}")
                });
            let subprocess_hash_hex = &hash_line[SUBPROCESS_HASH_PREFIX_V1.len()..];
            assert_eq!(
                subprocess_hash_hex,
                hex_v1(&parent_hash),
                "subprocess {attempt} hash diverged from the parent process hash"
            );
        }
    }

    /// One-shot worker invoked as a subprocess by the test above. Not a
    /// real assertion test on its own: it just loads the checkpoint,
    /// scores the same fixed battery, and prints the hash for the parent
    /// to compare. Left `#[ignore]`d and gated on the marker env var so a
    /// normal `cargo test --release -- --ignored` sweep (which runs every
    /// ignored test once, including this file's own directly-selected
    /// tests) does not also try to run this as a bare, unselected
    /// duplicate measurement; it is only meant to be invoked with
    /// `--exact`.
    #[test]
    #[ignore = "subprocess worker for forward_is_byte_identical_across_three_process_invocations_v1; do not run directly outside that harness"]
    fn forward_probe_subprocess_worker_v1() {
        if std::env::var_os(SUBPROCESS_MARKER_ENV_V1).is_none() {
            panic!(
                "this test is a subprocess worker; run it via \
                 forward_is_byte_identical_across_three_process_invocations_v1"
            );
        }
        let inference = load_probe_checkpoint_v1();
        let battery = battery_v1();
        let hash = hash_battery_v1(&inference, &battery);
        println!("{SUBPROCESS_HASH_PREFIX_V1}{}", hex_v1(&hash));
    }

    // ----------------------------------------------------------------
    // Property (c): concurrent load across threads, one shared model.
    // ----------------------------------------------------------------
    #[test]
    #[ignore = "requires the real de-novo checkpoint store on D:; run explicitly for the forward-determinism audit"]
    fn forward_is_byte_identical_under_four_concurrent_threads_v1() {
        let inference = Arc::new(load_probe_checkpoint_v1());
        let battery = Arc::new(battery_v1());
        let reference_hash = hash_battery_v1(&inference, &battery);

        let mut handles = Vec::new();
        for thread_index in 0..4u32 {
            let inference = Arc::clone(&inference);
            let battery = Arc::clone(&battery);
            handles.push(std::thread::spawn(move || {
                let mut last = [0u8; 32];
                for _ in 0..250u32 {
                    last = hash_battery_v1(&inference, &battery);
                    assert_eq!(
                        last, reference_hash,
                        "thread {thread_index} diverged from the single-threaded reference hash"
                    );
                }
                last
            }));
        }
        for (thread_index, handle) in handles.into_iter().enumerate() {
            let last = handle.join().unwrap_or_else(|_| {
                panic!("thread {thread_index} panicked during concurrent forward calls")
            });
            assert_eq!(last, reference_hash);
        }
        println!(
            "four_thread_concurrent_battery_hash_hex={}",
            hex_v1(&reference_hash)
        );
    }

    // ----------------------------------------------------------------
    // FTZ/DAZ and rounding-mode empirical audit (MXCSR).
    // ----------------------------------------------------------------
    #[cfg(target_arch = "x86_64")]
    fn read_mxcsr_v1() -> u32 {
        // `core::arch::x86_64::_mm_getcsr` is deprecated in favor of inline
        // assembly (the intrinsic form is not guaranteed not to be reordered
        // by the compiler relative to surrounding floating-point operations,
        // which would silently invalidate a before/after measurement like
        // this one). `stmxcsr` with no special `options(...)` is treated by
        // the compiler as a full side effect, so it cannot be hoisted or
        // sunk across the forward call this function brackets. SSE2, and
        // therefore MXCSR, is the mandatory baseline on every `x86_64`
        // target this crate builds for.
        let mut mxcsr: u32 = 0;
        unsafe {
            std::arch::asm!(
                "stmxcsr [{0}]",
                in(reg) &mut mxcsr,
            );
        }
        mxcsr
    }

    #[cfg(target_arch = "x86_64")]
    fn decode_mxcsr_v1(mxcsr: u32) -> (bool, bool, u32) {
        let daz = (mxcsr >> 6) & 1 == 1;
        let flush_to_zero = (mxcsr >> 15) & 1 == 1;
        let rounding_control = (mxcsr >> 13) & 0b11;
        (flush_to_zero, daz, rounding_control)
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    #[ignore = "requires the real de-novo checkpoint store on D:; run explicitly for the forward-determinism audit"]
    fn mxcsr_ftz_daz_and_rounding_mode_around_forward_calls_v1() {
        let inference = load_probe_checkpoint_v1();
        let battery = battery_v1();

        let before_thread_start = read_mxcsr_v1();
        let (ftz_start, daz_start, rc_start) = decode_mxcsr_v1(before_thread_start);
        println!(
            "mxcsr_before_any_forward_call raw=0x{before_thread_start:08x} ftz={ftz_start} daz={daz_start} rounding_control={rc_start}"
        );

        for (index, (globals, actions)) in battery.iter().enumerate() {
            let before = read_mxcsr_v1();
            let view = FlatScoringDecisionViewV2::new(
                globals,
                &[],
                &[],
                &[],
                &[],
                &[],
                &[],
                &[],
                &[],
                actions,
                &[],
            );
            let output = inference
                .score_decision_v1(view)
                .expect("forward must succeed on every fixed battery input");
            let after = read_mxcsr_v1();
            let (ftz_before, daz_before, rc_before) = decode_mxcsr_v1(before);
            let (ftz_after, daz_after, rc_after) = decode_mxcsr_v1(after);
            println!(
                "battery[{index}] mxcsr_before=0x{before:08x} (ftz={ftz_before} daz={daz_before} rc={rc_before}) mxcsr_after=0x{after:08x} (ftz={ftz_after} daz={daz_after} rc={rc_after}) value_bits={:08x}",
                output.value().to_bits()
            );
            assert_eq!(
                before, after,
                "battery[{index}]: MXCSR changed across the forward_v1 call (before=0x{before:08x} after=0x{after:08x}); this is itself a finding, not an expected pass"
            );
        }

        let after_all = read_mxcsr_v1();
        assert_eq!(
            before_thread_start, after_all,
            "MXCSR at end of probe differs from MXCSR before the first forward call"
        );
    }

    // ----------------------------------------------------------------
    // Search-scoped forward variant (Design v1 Section 1.5, Option S):
    // extends this probe to prove the deterministic-tanh variant's own
    // bit-stability, and to record its distance from the production libm
    // path. Nothing on the production path calls the variant; these tests
    // exercise it only through
    // `NativeCheckpointInferenceV1::score_decision_search_deterministic_v1`.
    // ----------------------------------------------------------------

    /// Search-scoped sibling of property (a):
    /// `forward_is_byte_identical_across_1000_repeated_calls_in_one_process_v1`.
    #[test]
    #[ignore = "requires the real de-novo checkpoint store on D:; run explicitly for the forward-determinism audit"]
    fn search_deterministic_forward_is_byte_identical_across_1000_repeated_calls_in_one_process_v1()
    {
        let inference = load_probe_checkpoint_v1();
        let battery = battery_v1();
        let first = hash_battery_search_deterministic_v1(&inference, &battery);
        for iteration in 0..1000u32 {
            let hash = hash_battery_search_deterministic_v1(&inference, &battery);
            assert_eq!(
                hash, first,
                "search-deterministic in-process repeat {iteration} diverged from the first call's hash"
            );
        }
        println!(
            "search_deterministic_in_process_1000x_battery_hash_hex={}",
            hex_v1(&first)
        );
    }

    /// Search-scoped sibling of property (c):
    /// `forward_is_byte_identical_under_four_concurrent_threads_v1`.
    #[test]
    #[ignore = "requires the real de-novo checkpoint store on D:; run explicitly for the forward-determinism audit"]
    fn search_deterministic_forward_is_byte_identical_under_four_concurrent_threads_v1() {
        let inference = Arc::new(load_probe_checkpoint_v1());
        let battery = Arc::new(battery_v1());
        let reference_hash = hash_battery_search_deterministic_v1(&inference, &battery);

        let mut handles = Vec::new();
        for thread_index in 0..4u32 {
            let inference = Arc::clone(&inference);
            let battery = Arc::clone(&battery);
            handles.push(std::thread::spawn(move || {
                let mut last = [0u8; 32];
                for _ in 0..250u32 {
                    last = hash_battery_search_deterministic_v1(&inference, &battery);
                    assert_eq!(
                        last, reference_hash,
                        "search-deterministic thread {thread_index} diverged from the single-threaded reference hash"
                    );
                }
                last
            }));
        }
        for (thread_index, handle) in handles.into_iter().enumerate() {
            let last = handle.join().unwrap_or_else(|_| {
                panic!("search-deterministic thread {thread_index} panicked during concurrent forward calls")
            });
            assert_eq!(last, reference_hash);
        }
        println!(
            "search_deterministic_four_thread_concurrent_battery_hash_hex={}",
            hex_v1(&reference_hash)
        );
    }

    /// Informational only, not a gate (see this file's module doc and the
    /// implementation task this test was written for): measures, and
    /// prints, the maximum ULP distance between the production libm
    /// forward path (`score_decision_v1`) and the search-scoped
    /// deterministic-math variant (`score_decision_search_deterministic_v1`)
    /// over every logit and value in the fixed battery. This is expected
    /// to be nonzero -- the whole point of the variant is a different,
    /// libm-free `tanh` -- and documents roughly how far apart the two
    /// paths land, not a pass/fail bound. Design v1 Section 1.5 explicitly
    /// accepts last-bit differences between the two paths, since search
    /// values are action-selection internals.
    #[test]
    #[ignore = "requires the real de-novo checkpoint store on D:; run explicitly for the forward-determinism audit"]
    fn search_deterministic_forward_max_ulp_deviation_from_libm_forward_v1() {
        let inference = load_probe_checkpoint_v1();
        let battery = battery_v1();

        let libm_values = battery_outputs_v1(&inference, &battery, false);
        let deterministic_values = battery_outputs_v1(&inference, &battery, true);
        assert_eq!(
            libm_values.len(),
            deterministic_values.len(),
            "libm and search-deterministic batteries produced different output shapes"
        );

        let mut max_ulp = 0u64;
        let mut max_ulp_index = 0usize;
        let mut sum_ulp: u128 = 0;
        for (index, (&libm_value, &deterministic_value)) in libm_values
            .iter()
            .zip(deterministic_values.iter())
            .enumerate()
        {
            let distance = ulp_distance_v1(libm_value, deterministic_value);
            sum_ulp += distance as u128;
            if distance > max_ulp {
                max_ulp = distance;
                max_ulp_index = index;
            }
        }
        let mean_ulp = sum_ulp as f64 / libm_values.len() as f64;
        println!(
            "search_deterministic_vs_libm max_ulp={max_ulp} at flattened_index={max_ulp_index} \
             (libm={:?} bits=0x{:08x}, search_deterministic={:?} bits=0x{:08x}) \
             mean_ulp={mean_ulp:.3} over {} values -- informational bound only, not a gate",
            libm_values[max_ulp_index],
            libm_values[max_ulp_index].to_bits(),
            deterministic_values[max_ulp_index],
            deterministic_values[max_ulp_index].to_bits(),
            libm_values.len(),
        );
    }
}
