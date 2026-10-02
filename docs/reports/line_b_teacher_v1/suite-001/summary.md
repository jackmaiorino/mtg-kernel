# Release suite, opus/exit-teacher-v1 at 10181834 (2026-09-28)

Toolchain rustc and cargo 1.94.1. Command, run by the lane's BelowNormal wrapper with CARGO_TARGET_DIR D:/cargo-target/opus-exit-teacher, host claim CLAUDE #549, released in CLAUDE #567:

`cargo --config profile.release.package.mtg-kernel.codegen-units=4 test --release --locked -j 4 -p mtg-kernel --features experimental-burn-net8-packed-cuda-v1 --no-fail-fast`

| phase | time (EDT) | result | log SHA-256 |
|---|---|---|---|
| test build (`--no-run`) | 01:37:09 to 02:10:36, 33 min 18 s | exit 0, 100 executables | 0d96e9d47ff1fbc9e990ccf45c1066937423a832ca7c05c32e4db2ca24cf8c0f |
| full suite | 02:12:16 to 02:16:37, 261 s | 101 test binaries: 2,996 passed, 10 failed, 109 ignored (doc tests 62 passed) | 4bfd16a7b534e7ba20c570c9f67dde03f44cc1ad36838b47f4e3d3c760ee4b24 |
| ignored tests, run explicitly (`--lib -- --include-ignored --exact`, GPU 1) | 02:18:30 to about 02:21:20, 170 s | 4 passed, 0 failed | a073b9454041f7a01b18d0117f46fb7abc34f8ebb0141a445d4f8c4afaa00a02 |

The logs stay in the lane's scratch (D:/e-scratch/opus-exit-teacher/suite-build.log, suite-run.log, suite-ignored.log), pinned here by hash.

## The ten failures

Library (8), each listed as a pre-existing failure (frozen training, store and serializer witnesses) in docs/g115_d3_review_revision_20260923.md, which is in base 500cfae9:

- `expanded_deck_training_v1::tests::ordinary_trainer_two_iteration_v3_fixture_state_hash_is_unchanged` (53177e91 against the pinned 28faac99)
- `expanded_deck_training_v1::tests::ordinary_trainer_two_iteration_v4_fixture_stamps_v4_and_restores_cleanly` (53177e91)
- `expanded_deck_training_v1::tests::ordinary_trainer_two_iteration_v4_fixture_fixed_partition_is_topology_invariant_and_pinned` (4da100d0)
- `native_checkpoint_inference_v1::checkpoint_reliance_probe_v1::action_block_gradient_diagnostic_v1::joined_frame_is_preflight_sealed_neutral_and_lineage_complete_v1`
- `native_checkpoint_inference_v1::checkpoint_reliance_probe_v1::fixed_rally_corpus_is_repeatable_without_external_artifacts_v1`
- `native_trainer_v1::tests::real_burn_pair_updates_once_and_is_topology_invariant`
- `native_training_store_run_v2::tests::current_frozen_literal_matches_the_live_build_constant`
- `native_training_store_update_group_v1::tests::sync_path_reproduces_mains_golden_store_hash`

The three ordinary-trainer values equal the base values Codex and panel-export recorded (CODEX #582; CLAUDE #530, a clean detached 500cfae9 worktree).

Integration (2), in code this branch does not change in non-test builds (rl_session.rs gains only four `#[cfg(test)]` functions, neither test file references the line (b) modules, and `mtg-kernel/tests/` and `data/` are untouched against Codex's head d3250f99), so their results are Codex's head's:

- `flat_policy_v1::runtime_typed_row_count_and_digest_goldens_are_exact` (runtime typed digests of five reset fixtures differ from data/flat_policy_v1/goldens_v1.json)
- `rl_contract::rl_contract_combat_projection_preserves_indexed_non_token_history` (left 0, right 1)

Value check (written at 2026-09-28 03:00; committed earlier at 02:56:17): both results equal base 500cfae9 in full. All five expected and actual digests of the flat_policy_v1 failure in this run's log appear in search-opponent's sealed base record, and rl_contract fails at tests/rl_contract.rs:543 with left 0, right 1 there too (docs/reports/line_b_teacher_v1/suite-001/integration-failure-values-500cfae9.json, SHA-256 11c02cb8825354be0459a57a1a4a7e25586ed7033efaa9bb2938f4c15b3353c4, CLAUDE #582; Codex's collector integration run gives the same two failures, CODEX #612).

The compiler errors in the doc-test section (for example "cannot construct NativeTrainingTrajectoryReceiptV2 with struct literal syntax") belong to `compile_fail` examples; all 62 doc tests passed.

## Ignored tests run explicitly

- `expanded_deck_training_v1::tests::line_b_teacher_cuda_update_publishes_packet_telemetry_and_envelope` (with the pinned CUDA goldens, which hold in release as in debug)
- `expanded_deck_training_v1::tests::line_b_head_only_mask_freezes_trunk_bytes_across_real_cuda_updates`
- `expanded_deck_training_v1::tests::ordinary_trainer_two_iteration_gae_fixture_cuda_is_bit_identical_across_two_processes` (the production CUDA path's pinned golden)
- `expanded_deck_training_v1::public_features::search_opponent::collect_tests::search_opponent_games_replay_validate_and_refuse_tampering` (Codex's D3 replay and tamper test, which exercises the merged trajectory validation with search rows)

Durability (Astra takeover, 2026-09-28): the 3,442-byte record above is a byte-identical copy of the search-opponent author's sealed scratch record, SHA-256 11c02cb8825354be0459a57a1a4a7e25586ed7033efaa9bb2938f4c15b3353c4. Astra copied it without changing its content or claiming to have rerun its tests. Change 14(iv) still requires an independent merged-head witness against these full values.


Independent g115 witness (written at 2026-09-28 21:28:10 -0400): change 14(iv) is countersigned in FABLE-REVIEW-20260928 at 21:05:10 EDT. Separate generation16 execution at c3e949af yielded flat_policy_v1 6 pass/1 fail and rl_contract 55 pass/1 fail, both exit101, no filtered tests. Every failure value and panic location equals the sealed 11c02cb8 baseline. Evidence: E:/mtg-g115-lineage-20260923/teacher-merged-witness-001/comparison.json, SHA-256 0f1a20234ca2c776f4d1033e545d1e48288b625b987691e2f58a4e3e7879d857. This supersedes the pending-witness status above for c3e949af only; the later g115 integration tree has not been executed.
