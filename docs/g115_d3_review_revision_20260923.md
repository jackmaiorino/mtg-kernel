# D3 review revision, September 23, 2026

Later E1 correction: `docs/g115_d3_golden_attribution_20260923.md` reports eight failures under trample reversal, five passes and three absent tests at the specified merge base. The historical pre-existing characterization below is not established and is withdrawn. Original suite results remain preserved.

Fable's verdict in collab/FABLE-REVIEW-20260923.md (348b5d0) countersigns the design and permits launch after R1-R7 are committed and engineering prerequisites pass. This revision preserves the original shared panel, algorithm, model, search budget, gate and power calculation. No formal match has run.

| Review point | Disposition |
|---|---|
| R1 | PRE names native cd41885e0ac05586d89bd4b2b7fb1284248689ef and the exact descriptor SHA. Relative to reviewed e258, only the identical candidate-order guard is extracted into a private helper and tested against a stale stored origin. Windows/Linux builds are clean at cd418. Mixed acceptance now explicitly checks all six retained V3 identities against all 1,024 archived counterparts. |
| R2 | Keep eight seeds per matchup, 1,024 matches/arm. Closure at effects >=3.0 pp requires measured paired SE <=0.87 pp; smaller effects remain unresolved. Higher SE means inconclusive below the explicitly approximate SE-scaled MDE. No expansion after outcomes. |
| R3 | Any typed abort/non-natural match invalidates the entire attempt and withholds statistics/verdict. Reader retains original native failure records, seed/root input and available failing step; missing diagnostic fields stay unknown. A corrected engine requires countersign before another attempt. |
| R4 | Environmental void means a full deterministic rerun into a new bound root, retaining all previous outputs and comparing overlap. No mixing or outcome-driven restart. |
| R5 | Compare every baseline store to the archived shared-panel reference before statistics. Normalize only predeclared build-envelope provenance and additive terminal-audit telemetry. All other fields remain exact; unexplained differences withhold the result. |
| R6 | Treatment is search plus greedy estimate choice versus sampled policy; attribute ADVANCE to the wrapper. Greedy-no-search is a follow-up outside this gate. Correct the collab source pin. The compute host's earlier SSH timeout is superseded by fresh availability; qualify it before selection. |
| R7 | Formal lease plan is five hours, 14,400-second shard plus 300-second recovery and 600-second guard margin, within unchanged $10 increment/$200 cumulative limits. Current three-hour engineering qualification is separate. Never extend beyond an active lease guard. |

## Test receipt and limits

Full library suite: **2,312 passed, 8 failed, 105 ignored**, 557.02 seconds of test execution. Receipt E:/mtg-g115-lineage-20260923/d3-review-full-lib-002; test-log SHA `f6cab32bdba9631adc7e36c4c0ed15587708b2a5fca3cc9431b3d957ce6e0aa6`. Every library, future-chance, search and evaluation regression passed, including `v4_library_stale_stored_origin_fails_live_choice_guard`. The stale-origin test exercises both seats and all three forms against the same production guard.

The suite binary was compiled immediately before committing the test: embedded parent metadata is b6f61018, plus the retained source.patch. That patch is byte-equal after line-ending normalization to git diff b6f61018..cd418, verified in start.json. Native source content is therefore cd418; no claim that the embedded parent string is cd418. The first full-suite attempt hit its 900-second combined build/test timeout; its logs remain in d3-review-full-lib-001. The second ran the unchanged already-built executable without filtering, skipping new failures, repinning goldens, or rebuilding fixtures.

The eight failures are frozen training/store/serializer witnesses, consistent with the pre-existing golden-drift class documented in GUIDANCE and Kimi's human-interface full-suite report (2,246/8, with failures reproduced on its pre-change tree). Their actual/expected values are retained in this log. That prior report is corroboration, not an independently rerun pre-change comparison on this branch. They do not enter the D3 inference-only route; the new production delta changes no ordinary training or store path. Disposition: retain and disclose, no golden updates or clean-suite claim. New-build cross-host qualification and all-1,024 baseline parity remain mandatory protection of the actual inference path.

- `expanded_deck_training_v1::tests::ordinary_trainer_two_iteration_v3_fixture_state_hash_is_unchanged`
- `expanded_deck_training_v1::tests::ordinary_trainer_two_iteration_v4_fixture_stamps_v4_and_restores_cleanly`
- `expanded_deck_training_v1::tests::ordinary_trainer_two_iteration_v4_fixture_fixed_partition_is_topology_invariant_and_pinned`
- `native_checkpoint_inference_v1::checkpoint_reliance_probe_v1::action_block_gradient_diagnostic_v1::joined_frame_is_preflight_sealed_neutral_and_lineage_complete_v1`
- `native_checkpoint_inference_v1::checkpoint_reliance_probe_v1::fixed_rally_corpus_is_repeatable_without_external_artifacts_v1`
- `native_trainer_v1::tests::real_burn_pair_updates_once_and_is_topology_invariant`
- `native_training_store_run_v2::tests::current_frozen_literal_matches_the_live_build_constant`
- `native_training_store_update_group_v1::tests::sync_path_reproduces_mains_golden_store_hash`

Python: ten checks passed, including unchanged paired-analysis arithmetic, deliberate semantic mismatches, unknown provenance rejection, validity failures withholding statistics and retention of typed abort/root/step evidence. Source: python/tools/test_g115_d3_baseline_parity_v1.py and test_g115_d3_preparation_v1.py. Consolidated receipt: E:/mtg-g115-lineage-20260923/d3-review-revision-check-001.json. No power or gate formula changed.

## Compute status

Windows build:147.958 seconds, binary SHA deb637ac2cb836504aad0dfb68c532c6a7a9a053f286e7c9a7de5ace5f471019. Linux build:103.918 seconds, SHA b9d11703407c3b424854b7604a59bb4f5f0b2d2891cbf79f6b2d99e8ca32e336. New builds use clean cd418 checkouts. Old e258 qualification remains preserved, not relabeled. New local 1/8/16/24 and guarded RunPod 1/8/16/32 comparisons use the same 64 consumed conditions, disjoint from D3. They are in progress, not yet qualified.

Inventory006 finds compute host reachable after the previously recorded outage. The maintainer's explicit 32 GiB reservation is workstation-specific; the earlier helper incorrectly copied it to the 32 GiB-total secondary PC. The helper now retains 8 GiB there for OS/user use and enforces that reserve during work; no the maintainer's or paid-lease limit is reduced. The compute host's Python guard dependency is being installed into an isolated environment, with CPU qualification pending. This is placement engineering, not a scientific constant change. Choose the fastest completed, compatible qualification with transfer/recovery overhead, and check competing work again before dispatch.

The launch manifest remains incomplete until qualifications, fresh inventory and the formal guarded lease exist. Kimi duty2 should verify this revision against R1-R7 in the existing queue entry. No new Fable session or extra experiment-review entry was created.
