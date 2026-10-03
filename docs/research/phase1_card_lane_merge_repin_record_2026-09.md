# Phase 1 card lane merge repin record (2026-09)

Branch `lead/phase1-card-lane-merge-v1`, worktree `E:/mtg-kernel-phase1-cards-merge`, HEAD
`2f9ccfa7` at the start of this task: the merge of `lead/pauper-meta-cards-v1` (wave 1 plus
wave 2/Urzatron) into the Phase 1 branch (merge commit `420c0ec3`), a build fix for the new
`ManaAbilityCostDef::None` variant in the BO3 mana-ability label (`59b4a3a1`), and regenerated
`flat_policy_v1`/`v2` goldens for the merged catalog (`2f9ccfa7`).

## Identity move this task covers

`KERNEL_CARDDB_HASH`: `0xde59_c501_e943_f3fd` (pauper-meta-cards-v1 wave 1's final identity,
see `docs/research/pauper_meta_wave1_record_2026-09.md`) -> `0x064a_7c98_9255_ab3c` (this
merge's identity, after folding in wave 2/Urzatron's 8 new cards and the Phase 1 branch's own
prior history).

The primary pin sites `python/tools/repin_card_db_identity_v1.py` tracks (`card_def.rs`'s
frozen-literal assertion, `native_training_store_run_v2.rs`'s
`FROZEN_CARD_DB_HASH_U64_HEX_PAUPER_META_W1`, the `CARD_DEFS.len()` literals, and the rest of
`REQUIRED_PIN_SITES_V1`) were already re-pinned to `0x064a_7c98_9255_ab3c` before this task
started, landed in the merge commit `420c0ec3` and confirmed unchanged by `2f9ccfa7`'s own
commit message. This task's scope is the second tier: hand-authored hash/golden literals over
real gameplay/rollout bytes that embed the identity indirectly, the same class of literal
wave 1's own Task 13 re-pinned (see that doc's "Re-pinned hash/golden literals" table). This
is a second re-pin of a subset of that same table, now that the identity has moved again.

## Methodology

Every candidate site was found by `grep -rln "pauper-meta-cards-v1 card lane's wave 1"
mtg-kernel/src mtg-kernel/tests python/tests python/tools`: every site wave 1 re-pinned is a
candidate for a second re-pin now, since the underlying cause (`KERNEL_CARDDB_HASH` mixed into
a digest, directly or via a live-tracking fixture) is unchanged. 13 files matched. For each
candidate the assertion and its surrounding test were read; the exact source line mixing
`KERNEL_CARDDB_HASH` (or a value derived from it) into the literal's computation was confirmed
by direct inspection; the new value was obtained by running that single failing test and
reading the actual value from the assertion's `left`/`right` panic output (never hand-derived,
never copied from wave 1's table); every other (non-literal) assertion in the same test was
confirmed to still pass (`ok`) both when the test failed on the targeted literal and in the
final green re-run, so a re-pin is never a cover for a behavior change. Two constants
(`rl_session.rs`'s "core" environment hashes) were found, by actually running the test with
their wave-1 value still in place, to be cause B (already resolved before this merge) rather
than cause A -- confirmed, not assumed, since the test passed unedited. A failure whose cause
is not the catalog identity move would be reported separately below and left unpinned; none
was found in this pass.

**Out of scope for this task** (present in the wave-1 marker grep, but not part of the `--lib`
suite or the specific `--test` targets this task names): `mtg-kernel/tests/blue_blasts.rs`,
`mtg-kernel/tests/brainstorm.rs`, `mtg-kernel/tests/rl_contract.rs`,
`python/tests/test_write_dek_from_mtgo_list_v1.py`. All four carried a wave-1 re-pin of the
same cause-A/B class and very likely need a second re-pin now too, but that is a disclosed gap
for a follow-up task, not attempted here (none of these run under `--lib`, and none is one of
the `--test`/module targets this task's step 4 names).

## Re-pin audit table

Cause key (same as wave 1's table): **(A)** `KERNEL_CARDDB_HASH` is mixed directly into the
literal's computation (a SHA-256 domain separator, an embedded `card_db_hash` field, or a
digest over bytes that embed one of those). **(B)** A `GameState`/`ObjectStateV4` struct-shape
change unrelated to card content -- none is active in this second pass; every wave-1 cause-B
literal (`GameState::monarch`, `ObjectStateV4::on_adventure`) was already fully resolved by
wave 1's own Task 13 and is confirmed unaffected by this merge (see "Confirmed unaffected"
below).

| Test | File | Old literal (wave-1 value) | New literal | Cause |
|---|---|---|---|---|
| `rl_session::tests::flat_action_candidate_commitment_matches_independent_pass_vector` | `mtg-kernel/src/rl_session.rs` | `[0x45,0x8c,0xce,0x40,0xb7,0x14,0x6d,0x98,0x81,0x18,0xc5,0x49,0x46,0xeb,0x92,0x40]` | `[0x57,0x66,0x98,0x62,0x89,0xda,0x70,0x3d,0x25,0xaf,0x03,0x20,0x52,0x38,0x0c,0xce]` | A |
| `rl_session::tests::flat_action_v2_token_domain_and_commitment_goldens_are_independent` (`v1`) | `mtg-kernel/src/rl_session.rs` | `[0xc3,0xa8,0x8d,0xde,0xbc,0xbc,0xc3,0xf3,0xcb,0xac,0x00,0x74,0xda,0xcd,0x75,0x5f]` | `[0xf8,0xca,0x13,0xd1,0xd2,0x16,0xb6,0x5a,0xd7,0x4d,0x38,0xf7,0x46,0x0b,0x59,0x21]` | A |
| same test (`common_v2`) | `mtg-kernel/src/rl_session.rs` | `[0xf4,0x6a,0x56,0x15,0x52,0x37,0x5e,0xcf,0xaa,0x0b,0x8a,0xbc,0x76,0xa4,0xcd,0x05]` | `[0xd3,0x9d,0x49,0xbd,0xdb,0x9a,0xef,0x4a,0xb9,0xfb,0x0f,0xec,0xd0,0x9b,0x5e,0x74]` | A |
| same test (`commitment_v2(65_536)`) | `mtg-kernel/src/rl_session.rs` | `[0x1f,0x17,0x3b,0x7e,0x1d,0xa5,0x8e,0x0e,0xd3,0xc6,0xb9,0xb3,0x21,0x78,0xd1,0xa0]` | `[0x4f,0x39,0xd7,0xa1,0xaf,0xbb,0x29,0x35,0x2b,0x44,0x84,0x9c,0xf4,0x20,0xb1,0xa8]` | A |
| `rl_session::tests::jsonl_v6_frozen_v5_bytes_and_api` (`V5_TRANSCRIPT_SHA256`) | `mtg-kernel/src/rl_session.rs` | `d0494851dbd7d944dab4cbca34e443ceedaf6a7269a0125cd1a7533c7b38f110` | `62208a3deec93510fe8595f23625d6cc4e604965d064350a2c1350955ac5266a` | A |
| `rl_session::tests::environment_hashes_are_diagnostic_dispatched_with_exact_goldens` (legacy policy env; shared verbatim by `v2_reset_preexisting_entry_points_remain_legacy_randomness`) | `mtg-kernel/src/rl_session.rs` | `0x6908_0ca5_c012_7e2b` | `0x2ad8_a53b_ecdc_221f` | A |
| same test (legacy full-session core; shared) | `mtg-kernel/src/rl_session.rs` | `0x3c6d_b17f_22d6_0e43` | **unchanged** (confirmed by a passing run with the old literal still in place) | B, already resolved pre-merge |
| same test (v2 policy; shared with `v2_reset_reuses_pre_constructor_pins_and_is_root_sensitive`) | `mtg-kernel/src/rl_session.rs` | `0xda58_b63d_08f6_6b99` | `0x3901_7f97_042a_d80a` | A |
| same test (v2 core; shared) | `mtg-kernel/src/rl_session.rs` | `0xa1c5_ca41_1ee8_1a2d` | **unchanged** (confirmed by a passing run with the old literal still in place) | B, already resolved pre-merge |
| `async_flat_scored_rollout_v1::tests::first_five_scorer_packets_have_exact_safe_golden` | `mtg-kernel/src/async_flat_scored_rollout_v1.rs` | `78289df65db7f4464e1107fdcbb7703c9ad2512aa3246fc9b5bb82475d103c81` | `f4468293c68ef2b62557aabf03afdd8056a9fae1a9d4258ec4cc708c7f6b84a4` | A |
| `native_checkpoint_inference_v1::checkpoint_reliance_probe_v1::action_block_gradient_diagnostic_v1::joined_frame_is_preflight_sealed_neutral_and_lineage_complete_v1` | `.../action_block_gradient_diagnostic_v1.rs` | `ae853cabe8cb59c0aa44e93e142d11963d4cc57893a1d9b27ffe7fedc6366a71` | `9a19af3bb5dadbbedce5648443c7ab4a7163e389ff7cfff7559f67fdedc452c9` | A |
| `native_checkpoint_runner_v1::tests::starting_player_unset_reproduces_parent_commit_checkpoint_eval_bytes_v1` (`logical_state_sha256`) | `mtg-kernel/src/native_checkpoint_runner_v1.rs` | `4a3d928eb8471a85700680aa2d97e20ace04b8bf8550991af9399450b474e3f5` | `d77a82c7a9c4928803be6af2d42c494a37c93091dbafbcdd897d8fc9f2918fae` | A |
| same test (`bindings[0].trajectory_sha256`) | `mtg-kernel/src/native_checkpoint_runner_v1.rs` | `a9e2d7d620e2b921bda94ebdc77db9bd3f1a211fae4835bf650ee3aea9347bad` | `39640e9adf5630473c3c07fd19fd1b5b4e85ef1d790cc4a0641f24dc6f159c5a` | A |
| same test (`bindings[1].trajectory_sha256`) | `mtg-kernel/src/native_checkpoint_runner_v1.rs` | `91020a9d924d1a53eeddc9cc59c72c72d7fff44cc10ddf61d775be00d4186778` | `af2343421fc8e9af763c9333e1b3ca6a980d303c45ec54b0577baff857ff264d` | A |
| `native_trainer_v1::tests::genuine_environment_v2_pair_executes_with_distinct_decks_and_reconstructible_outer` (pair 0) | `mtg-kernel/src/native_trainer_v1.rs` | `("7076afcbed3e4901263986cf49dec2aef0fc524298035a1303d9ee5c18ac391c","99d5adb203cee530600150364f452a38312d0b1e3ec1a452484be04a892083af")` | `("720cc813789de134346f2f7fcd5ed36694ca595e5668509d880a691d9d8a0863","0ed70bf6763dbec93ee0693b8f2ddc039bcd536b9976e5c8e22cc0c5031c77d7")` | A |
| same test (pair 1) | `mtg-kernel/src/native_trainer_v1.rs` | `("fb36f8a7c6c81938f6f93a47bd097da4b784ca42470082ce3376319dbb51ec68","42e736f84023f82d3c193d7baca5ac2d4247c2a13365fb05ead5b9c2362341d1")` | `("c840b7f4bfa23e06573a6b88a5734307b832035890d2bf5176c1132362878c4d","c0a6fe9722461e91d73acd7d6f72a579345b3e5f079f1b461724223e85a9f762")` | A |
| `native_trainer_v1::tests::real_burn_pair_updates_once_and_is_topology_invariant` (`episodes[0].trajectory_sha256`) | `mtg-kernel/src/native_trainer_v1.rs` | `[146,96,71,19,161,67,106,182,228,70,255,159,173,185,114,122,177,100,70,139,150,215,245,17,209,229,50,14,103,162,8,48]` | `[33,73,6,167,119,116,213,206,57,62,147,196,186,57,62,235,83,80,106,205,110,99,182,119,48,244,83,73,117,44,234,11]` | A |
| same test (`episodes[1].trajectory_sha256`) | `mtg-kernel/src/native_trainer_v1.rs` | `[98,192,36,61,165,126,250,1,161,202,78,70,100,50,198,91,242,42,198,17,126,28,170,1,127,206,41,245,145,161,230,14]` | `[179,245,74,214,194,31,195,235,179,28,91,229,205,207,193,4,112,78,182,65,228,90,136,211,33,14,77,133,114,160,69,206]` | A |
| `native_training_store_checkpoint_v3::tests::genesis_authority_roundtrips_and_matches_frozen_goldens` (`GENESIS_MANIFEST_SHA256_GOLDEN_V3`) | `mtg-kernel/src/native_training_store_checkpoint_v3.rs` | `41041620bb9bcdb656571a49f9e9da915c59569b29d8946cfc827f7bef93984b` | `01f95f369f0c496e834232e4c138c7b77dd4b8b5ac848645d42bc41a5c038462` | A |
| same test (`GENESIS_LOGICAL_STATE_SHA256_GOLDEN_V1`; shares value with `native_checkpoint_runner_v1.rs`'s `logical_state_sha256` above) | `mtg-kernel/src/native_training_store_checkpoint_v3.rs` | `4a3d928eb8471a85700680aa2d97e20ace04b8bf8550991af9399450b474e3f5` | `d77a82c7a9c4928803be6af2d42c494a37c93091dbafbcdd897d8fc9f2918fae` | A |
| `native_training_store_run_v2::tests::independent_digest_references_and_goldens_match` (`semantics`) | `mtg-kernel/src/native_training_store_run_v2.rs` | `2ad70a88949312e7df8f26926f0430bc9be93a89a3094cfd15e815bc4ef4665d` | `db709ac73893a382be26544c766847165c1e493c0d66295ee470327e5945b790` | A |
| same test (`identity`) | `mtg-kernel/src/native_training_store_run_v2.rs` | `3374ce8012db4d9c9200c34f996290a3d7d0ceb5f35c11c82b5c5c2b0e164cb3` | `8b79805463cf89dca30566f78fccd4d6bf44121f7c0f3eec7c1f7c6b31078949` | A |
| same test (`run_bytes` sha256; shared verbatim by `population_program_absence_preserves_legacy_bytes_and_run_hash` and `response_exploiter_absence_preserves_existing_bytes_and_population_behavior`) | `mtg-kernel/src/native_training_store_run_v2.rs` | `4c8ae8a6954236ff558ceadcc9f9b1ebcae0bd3bccfb8d1a6e6122617f6cfcd2` | `45a792caf3ea9782f25977043dfa968fa296a39e52f8fa43716442901e0df5ea` | A |
| `native_training_store_update_group_v1::tests::sync_path_reproduces_mains_golden_store_hash` (`MAIN_GOLDEN_SHA256_V1`, Windows-arm only; the Linux-gnu arm stays unverifiable from this host and is left untouched, same limitation as wave 1) | `mtg-kernel/src/native_training_store_update_group_v1.rs` | `a833a6e04bec9416f9fc71d9df1fcfd04dbe6af49cc9a5aef979a2eb5fe480cf` | `3dfbce637a6925e8d7dbc7842170ec4554f927e4971d13bc67b2851e53a84aef` | A |
| `native_training_store_update_group_v1::tests::legacy_update_group_matches_pre_c2_projection_with_linux_gnu_full_byte_pin` (`episodes_sha256`) | `mtg-kernel/src/native_training_store_update_group_v1.rs` | `0fd11d97f708f191e89c0039a0d65a48c2b63a38b68cbeaa86c0dc152c16498f` | `b8f1f3ea6b911cb7cdf977e685416601db0681257c8ccbe6878b082a558746ef` | A |

Every byte-LENGTH / structural / count assertion in each of these tests (`scorer.counts`,
`MAIN_GOLDEN_LEN_V1` = 78,190, `deck_hashes()`, `episode_index()`, `environment_seed()`,
`learner_seat()`, `policy_step_count()`/`physical_decision_count()`,
`changed_non_gauge_parameter_count`, `model_digest_before`,
`recorded_burn_pair_numerical_witness_v1`, `learner_group_count` = 112, etc.) was confirmed
still passing at every step (both when the test failed only on the targeted literal, and in
the final green re-run) -- none of these tests failed on anything but the named literal(s).

`native_trainer_v1::tests::real_burn_pair_updates_once_and_is_topology_invariant` was not in
the task's named failing-test list (it surfaced only from this task's own static analysis of
every wave-1 marker site); confirmed actually failing by a live run before touching it, per
the task's "never repin speculatively" instruction.

## Confirmed unaffected (checked, not re-pinned)

- `rl_session.rs`'s "core" environment hashes (`0x3c6d_b17f_22d6_0e43` legacy,
  `0xa1c5_ca41_1ee8_1a2d` v2), shared by `environment_hashes_are_diagnostic_dispatched_with_exact_goldens`,
  `v2_reset_preexisting_entry_points_remain_legacy_randomness`, and
  `v2_reset_reuses_pre_constructor_pins_and_is_root_sensitive`: wave 1's own table classifies
  these two as cause B (`GameState::monarch`, Task 11), not cause A. Confirmed empirically,
  not assumed: a live run of `environment_hashes_are_diagnostic_dispatched_with_exact_goldens`
  with the wave-1 "core" literals still in place, after the two cause-A "policy" literals were
  already re-pinned, passed clean. All three tests are green with these two constants
  untouched.
- `policy_surface_v5::tests::surface_binding_envelopes_are_diagnostic_dispatched_with_exact_goldens`'s
  four literals (`0x3313_5945_dcb9_4ed1`, `0x6940_0c4c_9fdc_2b49`, `0x9689_f972_2063_c266`,
  `0xc8e2_f885_b7e8_8615`): cause B only (same `GameState::monarch` root cause as above,
  already resolved by wave 1's own Task 13). Confirmed directly against
  `mtg-kernel/src/state.rs` (`state_hash()`/`diagnostic_state_hash()` hash the `GameState`
  struct itself; `Object` stores `card_def: u16`, an index, never an embedded `CardDef`, and
  this test's fixture uses synthetic debug card names, never touching real `CARD_DEFS`
  content) and confirmed green by an actual run, untouched.
- `native_checkpoint_runner_v1.rs`'s `deck_hashes()` literals (e.g.
  `[909_447_583_901_160_127, 909_447_583_901_160_127]`), `model_parameter_sha256`,
  `train_state_sha256`, and every `policy_step_count`/`physical_decision_count` literal in the
  same test: `data/runtime_decks_v1.json` is confirmed byte-identical between the wave-1 start
  commit `ce1a69f0` and this branch's HEAD (`git diff ce1a69f0 HEAD --
  data/runtime_decks_v1.json` is empty), so deck hashes do not move; confirmed green,
  untouched.
- `native_training_store_checkpoint_v3.rs`'s `GENESIS_PAYLOAD_SHA256_GOLDEN_V1` and
  `GENESIS_TRAIN_STATE_SHA256_GOLDEN_V1`: not flagged by a re-pin comment in source in either
  wave 1 or this pass; confirmed green, untouched.
- `native_trainer_v1.rs`'s `model_digest_before`, `changed_non_gauge_parameter_count`,
  `recorded_burn_pair_numerical_witness_v1`, and every step/count literal: not flagged for
  re-pin, confirmed green, untouched.
- `fix: handle ManaAbilityCostDef::None in the BO3 mana-ability label` (`59b4a3a1`): its own
  commit message states no test in the merged tree pins the affected label string; confirmed
  by reading the diff (`mtg-kernel/src/human_bo3_v1/labels.rs`, 10 lines). No re-pin implicated
  by this change, and no such test failure was observed.
- `native_training_store_run_v2::tests::current_frozen_literal_matches_the_live_build_constant`:
  fails by design on this branch (asserts the live `KERNEL_CARDDB_HASH` equals
  `FROZEN_CARD_DB_HASH_U64_HEX_CURRENT_V1`, the main-tree-only literal); this is the same
  intentional canary wave 1's Task 13 documented at length ("the canary is doing its job
  correctly by failing here"). Left red by design, not re-pinned, not a regression.

## Commands run (from the worktree root, `E:/mtg-kernel-phase1-cards-merge`)

Environment for every command: `CARGO_TARGET_DIR=E:/cargo-target-phase1-merge`,
`TEMP=E:/tmp/lead`, `TMP=E:/tmp/lead`, `RUST_MIN_STACK=16777216`, `--jobs 4` always.

Per-test value discovery and verification used a scratch clone of the target directory
(`E:/cargo-target-phase1-merge-scratch`, a `robocopy /MIR` of the official one, seeded so
dependency crates did not need rebuilding) so individual `cargo test <filter>` invocations
could compile and link without contending for the official target directory's output
executable, which the full-suite background run (below) held open for its entire ~1.5-hour
run. Every literal in the table above was captured this way: run the single failing test,
read the live-computed value from its `left`/`right` panic output, edit the literal in place,
re-run to confirm green (or, for multi-assertion tests, to reveal the next literal). The
scratch directory was deleted after use; it held no independent state, only a build-cache
clone.

1. `cargo test --offline --locked --jobs 4 -p mtg-kernel --lib -- --test-threads=1` (background,
   official target dir, log `E:/tmp/lead/logs/merge-full-lib-001.log`) -- the complete-suite
   run for the full pass/fail/ignored tally, per task step 1/3. This run recorded 1,730 passing
   tests and the 12 expected `FAILED` lines (11 catalog-identity literals across the modules
   in the audit table above, plus the one intentional design canary) before it ended
   abnormally (no final `test result:` line) mid-test at
   `phase1_registry_transfer_v1::fresh::tests::phase1_fresh_registry_rejects_feature_contract_encoding_source_or_descriptor_mismatches`,
   roughly 1h50m into the run, coincident with unrelated heavy CPU contention on the host
   (a separate formal evaluation panel). Re-running `phase1_registry_transfer_v1::` alone
   afterward passed clean (21/21, including the exact test the truncated run stopped at,
   `finished in 81.33s`), so this was an environmental interruption, not a code defect in that
   module or in this task's changes.
2. For each test in the audit table above (and the two "confirmed unaffected" checks), in a
   scratch target dir (`E:/cargo-target-phase1-merge-scratch`, deleted after use): `cargo test
   --offline --locked --jobs 4 -p mtg-kernel --lib <module>::tests::<test_name> -- --test-threads=1 --exact`, iterated per literal for multi-assertion tests.

## Full re-verify (task step 3) and remaining-coverage sweep

All re-pins landed; every module touched by this task, plus every module the original
background run never reached, was independently re-run to completion in the official target
dir (`E:/cargo-target-phase1-merge`, `--jobs 4`, `BelowNormal` priority via `cmd /c "start /b
/wait /belownormal cargo test ..."`, `TEMP`/`TMP` on `E:/tmp/lead`). Commands and per-module
counts:

| Module (`--lib <module>::`) | Result |
|---|---:|
| `rl_session` | 84 passed, 0 failed |
| `async_flat_scored_rollout_v1` | 37 passed, 0 failed |
| `native_checkpoint_runner_v1` | 12 passed, 0 failed |
| `native_checkpoint_inference_v1::checkpoint_reliance_probe_v1::action_block_gradient_diagnostic_v1::joined_frame_is_preflight_sealed_neutral_and_lineage_complete_v1` (single test) | 1 passed |
| `native_trainer_v1` | 28 passed, 0 failed |
| `native_training_store_checkpoint_v3` | 23 passed, 0 failed |
| `native_training_store_run_v2` | 124 passed, 1 failed (intentional design canary, see below), 3 ignored |
| `native_training_store_update_group_v1` | 49 passed, 0 failed, 1 ignored |
| `policy_surface_v5` | 12 passed, 0 failed |
| `phase1_registry_transfer_v1` (isolated re-run, see above) | 21 passed, 0 failed |
| `phase_profile` | 2 passed |
| `policy_observation_v6` | 13 passed |
| `private_physical_trajectory_v1` | 15 passed |
| `private_physical_trajectory_v2` | 2 passed |
| `runtime_decks` | 2 passed |
| `snapshot` | 2 passed |
| `state` | 28 passed |
| `store_v2_resume_walk_timing_harness_v1` | 0 passed, 1 ignored |
| `strict_source_tree_attestation_v1` | 11 passed |
| `surface` | 6 passed |
| `surface_v2` | 20 passed |
| `trace` | 7 passed |
| `trigger` | 4 passed |
| `xmage_observed_inference_v1` | 4 passed |
| `rl` | 8 passed |
| `sideboard_play_policy_v1` | 10 passed, 1 ignored |
| `sideboard_search_campaign_v1` | 29 passed |

Cross-checked for completeness: every one of the 2,064 tests `cargo test --lib -- --list`
reports was matched against the union of every log this task produced (the original
background run, the isolated `phase1_registry_transfer_v1` run, and every module run above);
zero tests are unaccounted for. The **only** failure anywhere in `--lib` scope is the single
intentional design canary,
`native_training_store_run_v2::tests::current_frozen_literal_matches_the_live_build_constant`
(asserts the live `KERNEL_CARDDB_HASH` equals the main-tree-only
`FROZEN_CARD_DB_HASH_U64_HEX_CURRENT_V1` literal; documented at length in wave 1's own record
as failing by design on any branch that has moved off the `Current` catalog profile). No
failure in this pass was caused by anything other than the catalog identity move or (for the
one canary) the pre-existing, by-design profile divergence; no catalog-identity failure was
left unpinned.

A stray false positive surfaced in `python/tools/repin_card_db_identity_v1.py --check` after
the re-pins: one of this task's own explanatory comments in
`native_training_store_run_v2.rs` spelled out the live hash's grouped hex form in prose next
to the `run_bytes`/`semantics`/`identity` literals, and since that file is the tool's one
`PROTECTED_FILE_V1`, the tool's line-classifier treats any bare occurrence of the hash that
does not name one of its known identifiers as a hard error. Fixed by rewording that one
comment to reference `FROZEN_CARD_DB_HASH_U64_HEX_PAUPER_META_W1` by name instead of
respelling its value; `--check` now exits 0 clean, and the module was re-run afterward
(124 passed, 1 failed, same canary) to confirm the comment-only edit changed nothing.

## Wave-2 integration tests and fresh-origin filters (task step 4)

| Command | Result |
|---|---:|
| `cargo test --offline --locked --jobs 4 -p mtg-kernel --test pauper_meta_w2_tron --test pauper_meta_w2_lands_and_rocks` | `pauper_meta_w2_tron`: 8 passed; `pauper_meta_w2_lands_and_rocks`: 7 passed; 0 failed |
| `cargo test --offline --locked --jobs 4 -p mtg-kernel --lib phase1_registry_transfer_v1:: -- --test-threads=1` | 21 passed, 0 failed |
| `cargo test --offline --locked --jobs 4 -p mtg-kernel --lib expanded_deck_training_v1:: -- --test-threads=1` | 34 passed, 0 failed, 2 ignored |

`phase1_registry_transfer_v1` and `expanded_deck_training_v1` are `--lib` modules
(`pub mod phase1_registry_transfer_v1;` / `pub mod expanded_deck_training_v1;` in
`mtg-kernel/src/lib.rs`), not separate `--test` integration binaries; no such files exist
under `mtg-kernel/tests/`. All four commands are clean.

## Do the two re-pinned goldens depend on the git tree hash?

No, for both `async_flat_scored_rollout_v1::tests::first_five_scorer_packets_have_exact_safe_golden`
and
`native_checkpoint_inference_v1::checkpoint_reliance_probe_v1::action_block_gradient_diagnostic_v1::joined_frame_is_preflight_sealed_neutral_and_lineage_complete_v1`,
confirmed by direct source inspection, not assumed:

- `mtg-kernel/build.rs`'s `configure_commit_tree_binding` computes `MTG_KERNEL_BUILD_GIT_TREE`
  (and `MTG_KERNEL_BUILD_GIT_HEAD`/`MTG_KERNEL_BUILD_GIT_CLEAN`/
  `MTG_KERNEL_BUILD_TRACKED_TREE_SHA256`) from `git rev-parse HEAD` and `HEAD^{tree}` -- the
  **committed** HEAD's tree, never the working tree's uncommitted state. It only changes when
  a new commit moves HEAD.
- `async_flat_scored_rollout_v1.rs` contains zero references to `MTG_KERNEL_BUILD_GIT_TREE`,
  `git_tree`, or any of the other `MTG_KERNEL_BUILD_GIT_*` env-baked constants anywhere in the
  file (`grep` confirms no match). Its digest is computed purely from `run_async_flat_scored_rollout_v1(config(1,1,1,1), ...)`'s observation bytes, which embed
  `KERNEL_CARDDB_HASH` (a build-script output derived from `data/cards_v1.json`, unrelated to
  the git tree/commit identity constants).
- `joined_frame_is_preflight_sealed_neutral_and_lineage_complete_v1`'s digest comes from
  `frame_joined_tape_v1(...).sha256_v1()`, whose dependency chain
  (`joined_fixture_v1` -> `join_fixture_v1`/`join_rollout_v1` -> `frame_joined_tape_side_v1`)
  only touches `PreflightSeed949999AuthorityV1` (whose `DiagnosticTapeAuthorityV1` impl
  exposes only `seed_v1()`, `expected_counts_v1()`, `allows_update_v1()` -- no git-derived
  field at all) and the tape's own deck ids/hashes. The `git_tree`/`git_commit` fields that do
  exist elsewhere in this same source file belong to an unrelated provenance-checking test
  group later in the file and are never constructed or referenced anywhere in this test's call
  graph.
- Corroborating precedent: wave 1's own record documents re-pinning both of these same two
  tests, then landing three commits (`0c47c4bc`/`af4f4574`/`ad77ddf9`) and re-running the full
  suite afterward ("Addendum: final confirmation `--lib` sweep after commit"), with both
  tests still green post-commit -- i.e. these exact two goldens already survived a HEAD/tree
  move once, under the identical mechanism.

Consequently, this task's re-pins of these two goldens will **not** need to move again on the
next commit for tree-hash reasons; they will only move again if `data/cards_v1.json` (the card
catalog) changes again.

## Addendum: merging the Phase 1 branch tip forward (2026-09-16)

A second task merged the Phase 1 branch tip, `codex/learned-sideboarding-integration-v1` at
`7dc364d7`, into this same branch (`lead/phase1-card-lane-merge-v1`, worktree
`E:/mtg-kernel-phase1-cards-merge`), starting from this doc's own HEAD `b6b916f0`, so the card
wave can later fast-forward into Phase 1. Merge base: `2d8d564c`.

### Conflicts and resolution

`git merge codex/learned-sideboarding-integration-v1` produced exactly one textual conflict,
`data/flat_policy_v2/goldens_v2.json`, both sides having independently regenerated
`payload_sha256` (ours `59c0b96d...` for this branch's own re-pin work, theirs
`6da80043...` for their own unrelated V3/V4 additions). `feature_inventory_v2.json` auto-merged
cleanly (the two sides' edits landed on non-overlapping lines). Resolved by keeping HEAD's value
verbatim as a placeholder, then regenerating both files with
`python/tools/generate_flat_policy_v2_goldens.py` (no `--check`) once the merge's other 37 files
were staged, so the payload hash reflects the merged `flat_policy_v1.rs`/`flat_policy_v2.rs`
bytes and the merged catalog. `python/tools/generate_flat_policy_v1_goldens.py --check` was run
first and reported clean (`flat_policy_v1.rs` is untouched by the Phase 1 branch, so no v1
regeneration was needed). Both generators report clean (`--check`, exit 0) after regeneration.
No conflicts appeared in `rl_session.rs`, `trigger.rs`, or any other source file: `git diff
--diff-filter=U` was empty after the one resolution above, and a source-tree grep for leftover
`<<<<<<<`/`=======`/`>>>>>>>` markers found none. `rl_session.rs` carries 889 lines of diff on
the source branch relative to the merge base and this branch's own prior re-pin diff relative to
the same base; both landed on non-overlapping lines and merged automatically, confirmed correct
by the module's own test run below (99/99 passing).

### `repin_card_db_identity_v1.py --check`

Clean after building the merged crate (`cargo build --offline --locked --jobs 4 -p mtg-kernel`,
`MTG_KERNEL_CARGO_TARGET_DIR=E:\cargo-target-phase1-merge`): `card_db_hash 064a7c989255ab3c`,
matching this branch's already-finalised identity. The tool's own report lists
`native_training_store_run_v2.rs`'s `FROZEN_CARD_DB_HASH_U64_HEX_V2` and
`FROZEN_CARD_DB_HASH_U64_HEX_CURRENT_V1` as stale, annotated "not rewritten (Task 3 owns
re-pinning it)" -- these are the main-tree-only design canary sites, expected and unchanged by
this task.

### `--lib` test sweep: every module covered, per-module counts

Ran in four grouped `cargo test --offline --locked --jobs 4 -p mtg-kernel --lib -- <mod1>::
<mod2>:: ... --test-threads=N` invocations (multiple `module::` filters after `--` are OR'd by
the libtest harness), covering all 2,106 tests `--lib -- --list` reports (cross-checked
name-for-name against the combined logs; the five apparent gaps found by an early substring-only
cross-check were `#[should_panic]` tests, whose result line reads `test <name> - should panic
... ok` and simply didn't match that first regex -- confirmed present and green in the raw
logs). `policy_observation_v7` (new this merge) carries zero `#[test]` functions, confirmed by
direct inspection; nothing to run there.

| Group | Modules | Result |
|---|---|---:|
| Priority (Phase 1-changed) | `rl_session`, `flat_policy_v2`, `flat_policy_v3`, `flat_policy_v4`, `policy_observation_v6`, `trigger`, `sideboard_play_policy_v1`, `learned_sideboard_v1`, `phase1_w8a_live_swap_self_play_v1`, `bo3_session`, `fast_sampler`, `expanded_deck_training_v1`, `phase1_registry_transfer_v1` | 257 passed, 0 failed, 4 ignored |
| Chunk 00 (debug, `--test-threads=4`) | 33 modules incl. `card_def`, `engine`, `flat_policy_v1`, `human_bo3_v1`, `model_guided_search_*` | 628 passed, 0 failed, 9 ignored |
| Chunk 01 (debug, `--test-threads=4`) | 24 modules incl. `native_checkpoint_inference_v1`, `native_checkpoint_runner_v1`, `native_trainer_v1` (schedule-only at this point), `phase1_agent_v1` family | 445 passed, **1 failed** (see below), 21 ignored |
| Remaining (release, `--test-threads=4`) | 52 modules: the rest of chunk 02/03 plus `native_trainer_v1`, `native_science_loop_v1` | 727 passed, **1 failed** (known canary), 15 ignored |

Grand total across all four groups: 2,057 passed, 2 failed, 49 ignored (2,106 accounted for).
Per-module pass/ignored/failed breakdowns for every one of the ~120 modules were computed from
the raw logs and match this table's group totals exactly (a handful of modules' raw `grep`
counts were inflated by cargo's own "has been running for over 60 seconds" watchdog line
repeating a test's full path -- confirmed benign by inspecting the raw lines; the harness's own
`test result:` summary line is authoritative and was used for every total above).

**Debug-mode slowdown, not a hang or lock contention.** Chunks 00-01 and the priority group ran
fine in debug mode. Chunk 02 (`native_trainer_v1`, `native_science_loop_v1`,
`native_training_store_checkpoint_v3`, and others) stalled for 30+ minutes under debug with
`--test-threads=4`, then stalled again single-threaded on individual tests (confirmed by
`Get-Process` CPU-time deltas matching wall-clock elapsed almost exactly the whole time --
continuously and legitimately computing, not deadlocked or idle). Rather than continue paying
that cost, the crate and its tests were rebuilt with `cargo build --release` (17 minutes,
one-time) and the remaining 52 modules re-run under `--release --test-threads=4`: 295 seconds
total, 727 passed, 1 failed (the known canary), 0 hangs. This was a build-profile choice made
under the standing time-efficiency authorization, not a correctness change; released and debug
binaries were never mixed within one comparison (every module's pass/fail read from a single
consistent run).

### The one non-canary failure: pre-existing on the source branch, not caused by this merge

`native_checkpoint_inference_v1::checkpoint_reliance_probe_v1::action_block_gradient_diagnostic_v1::joined_frame_is_preflight_sealed_neutral_and_lineage_complete_v1`
failed on the merged tree:

```
left:  "ba4b3568f7d93f9485b2a6cb4a00f71372f99f912ae3bb1c41aee31a65ab7c59"
right: "9a19af3bb5dadbbedce5648443c7ab4a7163e389ff7cfff7559f67fdedc452c9"
```

`right` is exactly this doc's own catalog-identity re-pin from the first task (the row for this
test in the audit table above), still green on this branch's pre-merge HEAD `b6b916f0`. Per this
task's brief, a catalog-identity literal is only eligible for re-pinning when the failure is in
code the Phase 1 side *added*; this file
(`action_block_gradient_diagnostic_v1.rs`) is pre-existing, not new, so this failure was
investigated rather than re-pinned:

- Every input and transitive dependency of this test's fixture (`joined_fixture_v1`,
  `join_rollout_v1`, `frame_joined_tape_v1`, `envelope_probe_receipt_for_test_v2`,
  `zero_learner_envelope_probe_receipt_for_test_v2`, `validate_start_v2`, `envelope_sha256_v2`,
  `runtime_deck_by_id`, `ladder_pool_member_for_episode_v1`,
  `native_trainer_episode_schedule_v1`, `derive_native_trainer_learner_action_seed_v1`,
  `selected_log_softmax`, `synthetic_action_tensor_v1`, `PreflightSeed949999AuthorityV1`) lives
  either in this same unchanged file or in `native_full_episode_trajectory_v2.rs`,
  `native_ladder_opponent_v1.rs`, `native_trainer_schedule_v1.rs`,
  `native_policy_train_step_v1.rs`, `runtime_decks.rs`,
  `checkpoint_reliance_probe_v1/action_ingress_admission_v2.rs`, `rl_session.rs` (type aliases
  only, unchanged shape) -- every one of these files, `data/cards_v1.json`,
  `data/runtime_decks_v1.json`, `Cargo.lock`, and `mtg-kernel/build.rs` is confirmed
  byte-identical between the merge base `2d8d564c` and the source branch tip `7dc364d7`
  (`git diff --stat`, empty). The fixture is fully synthetic (fixed seeds and a hard-coded
  `[0x3c; 32]` inner digest), not a live gameplay rollout, so trigger.rs/rl.rs/engine.rs changes
  elsewhere cannot reach it through this call graph.
- Direct diagnostic: a disposable detached worktree (`git worktree add --detach
  E:/tmp/lead/diag-codex-branch 7dc364d7`, its own scratch `CARGO_TARGET_DIR`) ran this exact
  test, in isolation, on the **unmerged source branch alone**. It also failed, against its own
  inherited (pre-catalog-move) pin `ae853cabe8cb...`, computing a **third** distinct value
  (`cd71f6a0ed8b1f515ffaf4c1ef6fa10a195804f7b8b60602afa11cd71a1d7bb6`, matching neither this
  branch's pin nor the merged-tree value). This proves the golden is already stale on
  `codex/learned-sideboarding-integration-v1` by itself, independent of this merge or the card
  catalog; the merged-tree value differs again only because the merge additionally carries this
  branch's own catalog move. The diagnostic worktree and its scratch target dir were removed
  after use.
- The two new Phase 1 docs added by this merge (`docs/PHASE1-W8A-LIVE-SWAP-MATCH.md`,
  `docs/research/phase1_v3_contract_frozen_pins_2026-09.md`) do not mention this test, so it is
  not a documented, already-accepted gap on the Phase 1 side either.

Per this task's explicit instruction, a failure that is not a catalog-identity literal in
Phase-1-added code is left unpinned and reported rather than silently re-pinned. **No re-pin was
made for this test.** It should be reported to whoever owns the Phase 1 branch's own test health
(likely Codex/the Phase 1 lead) as a pre-existing, undocumented golden drift in
`action_block_gradient_diagnostic_v1.rs`, unrelated to the card lane.

### Python suites

`PYTHONPATH=<worktree>/python;<worktree>/python/tests python -B -m pytest
python/tests/test_features_v6.py python/tests/test_features_v7.py -q`: 29 passed, 11 subtests
passed, 0 failed. `python -B -m unittest discover -s python/tools/phase1_cloud -p "test_*.py"`
(21 files): `Ran 294 tests ... OK (skipped=12)`.

### No re-pins were needed

Unlike the first task, this merge produced no new catalog-identity literal drift: the only two
`--lib` failures are the pre-existing main-tree-only design canary
(`native_training_store_run_v2::tests::current_frozen_literal_matches_the_live_build_constant`,
explicitly accepted by this task's brief) and the pre-existing, unrelated golden drift on the
source branch documented above. Nothing in this addendum required editing a frozen literal, so
there is no separate re-pin commit for this task; the merge commit carries only the merge
resolution (the two regenerated `flat_policy_v2` files) and this addendum.
