# Public feature successor: engineering qualification

The corrected runtime exposes active Strands prevention, but the existing model has no dedicated global prevention-color inputs. Visible card rows also lack explicit printed mana costs. This successor tests an additive representation without changing frozen V4/Python V7 identities or the reference checkpoint.

Implemented in this isolated worktree:

- Six public state fields: five active prevention colors and a separate damage-cannot-be-prevented flag.
- Thirty-two fields for each already-visible card token: known card, cost presence, front-face mana value, generic/X counts, colored/phyrexian pips and unordered hybrid pairs. These are printed costs, not actual payment or alternate costs. Hidden hands and libraries never supply tokens.
- Independent Rust compiled-cost and Python printed-cost encoders with a separate versioned contract.
- A Python research model that adds two zero-initialized input projections. Existing parameter names, shapes and accumulation order remain intact. Imported Adam ages remain 32400; the two new matrices start at age zero.

Evidence: `E:/mtg-postboard-campaign-20260920/public-feature-qualification-001`.

Native catalogue and actual V4 fixture tests passed. Twelve fixtures cover both seats, three shield configurations and two hidden-hand/library/allocation variants. The Python qualification in `python-002/qualification.json` passed all 192 catalogue rows, all thirteen base arrays and auxiliary rows. Imported parameters and both Adam moments are bit-identical to g115. Initial Python reference/successor logits and values are bit-identical. Both new matrices receive nonzero gradients; two synthetic updates reproduce parameters and Adam after save/resume. Qualification took 3.42 seconds on torch 2.13.0+cpu.

The earlier `python-001` qualification stopped before checkpoint transport because raw native action semantics lacked the Python legal-action envelope. The runner now constructs explicitly diagnostic operational wrappers; all native/Python tensor comparisons pass. No frozen encoder was modified and no measurement was rerun. Preserve both roots. A subsequent boundary check rejects i64::MIN tokens without arithmetic overflow.

No terminal-reward training, native successor scoring/update integration, CUDA optimizer parity, stronger candidate or human-strength result follows from these tests. The synthetic loss is only a gradient-connectivity probe. Existing human delivery and g115 checkpoint remain unchanged.

The subsequent sections record native scoring and CUDA optimizer qualification. The next learning comparison must use the corrected observation runtime, matched seeds and terminal rewards in both arms. Keep canonical retention checks and independent opposition; do not select from CP7 outcomes.

Fable's September 19 consultation failed HTTP429 before source reads, with reset September 22 at 07:00 EDT. No retry or endorsement is implied. Bounded local work proceeds under the maintainer's explicit authority with this review gap unresolved. No paid compute or broad training campaign is launched.

## Native scorer and complete warm-start games

The native successor now implements the two input projections in the actual Net8 forward calculation. Legacy callers take the existing arithmetic path with no public projection. A distinct `NativePublicInputNetV1` wrapper and `PublicInputPlayPolicyV1` adapter accept only V4 inputs and derive both tensor and public observation from the same bound legal decision. No legacy checkpoint loader automatically admits the successor. The adapter uses the existing per-seat RNG and legal-action sampler exactly once per decision.

Evidence root: `E:/mtg-postboard-campaign-20260920/public-native-forward-001`.

| Check | Result |
| --- | --- |
| g115 native/Python forward, zero/object/state/both projections, 12 fixtures each | Pass, maximum absolute score/value difference 0.000003815 within the fixed 0.001 absolute plus 0.001 relative envelope |
| Native zero projection versus native legacy scores | Bit-identical |
| Hidden-card/allocation pairs under all four projections | Bit-identical scores |
| Nonzero projections connected | Object changed 12/12 fixtures; state 8/12, both 12/12 |
| Malformed/nonfinite weights, old schema, wrong registry | Rejected |
| Corrected-runtime Gates/Affinity full BO3s, both physical seats | All 698 decisions, starts, seeds, game winners and outcomes match baseline |
| Same first match in a fresh native process | Byte-identical output, SHA256 `9e17fc22493ef2f029517bdc6d725b17996797487b525f89a9f778f7d5ed0d08` |
| Existing model/policy regressions | 22 passed, one pre-existing ignored |

Two unique BO3s/four games; three executions/six games including replay. Match tests took 13.43, 9.52 and 13.25 seconds, excluding builds. Debug build times were 43.17 and 44.51 seconds. Both models' added projections were zero in the complete games; no nonzero full-game or learned-model claim follows. The separate numerical probes used fixed signed binary-fraction matrices, not outcomes or training. All runs finished successfully. Existing g115 human delivery remains untouched.

These scorer checks preceded the CUDA work below. The existing `training.rs::ExperimentalDeviceTrainStateV1` assumes 33 tensors and one Adam age, requiring separate successor state rather than admitting the new matrices under its old identity.

## CUDA update and optimizer-state transport

The CUDA core is now implemented separately in `training/public_inputs.rs`. It reuses the established GAE loss and legacy Adam mapper, while the two public matrices have separate moments and a shared new-parameter age. Both projections use separate pre-tanh matmuls. The original forward path passes no additions. Updates prepare the legacy and public candidates before publishing both ages and parameter sets together.

`training/public_inputs/snapshot.rs` serializes all parameters, both moments and both ages under `mtg-kernel-public-input-optimizer-state/v1`, with exact V4, registry and auxiliary contract identities. It validates the legacy state hash and projection dimensions, finiteness and nonnegative second moments. This is optimizer-state transport, not yet a complete campaign checkpoint containing rollout progress, seeds and loss configuration. Existing loaders and the existing 33-tensor optimizer are unchanged.

Final evidence: `E:/mtg-postboard-campaign-20260920/public-cuda-qualification-002/qualification.json`. Earlier `public-cuda-qualification-001` is preserved; it passed the original 12-fixture check before the dummy-card guard was added. The same fixed numerical gates apply in both roots.

| Qualification on physical GPU 1, RTX 3050 | Result |
| --- | --- |
| Exact import of g115 parameters, moments and ages | Pass |
| Initial CUDA legacy/successor logits and values | Bit-identical |
| 35 gradient tensors versus Python | Pass, maximum absolute difference 0.0000023842 |
| Two updates versus Python | Pass, maximum parameter-delta difference 0.00000005961 |
| Both moment sets versus Python | Pass under the fixed absolute/relative envelopes |
| Fresh native process restores step 1 and reproduces step 2 | Byte-identical complete optimizer-state snapshot |
| Dummy-card raw gradient is nonzero, but padding parameters and moments stay zero | Pass, raw gradient L1 18.3531 |
| Invalid projection moments/ages and mismatched/corrupt snapshots | Rejected |
| Existing CUDA empty-relation Adam continuation regression | Pass |

The final ages are 32402 for legacy parameters and 2 for the new matrices. The repeated state SHA256 is `5c1686a9a1063c8f48a62f7d39615e103804aaf7eecfd8121e2020a5603d41dc`. The dummy guard preserves Python's `Embedding(padding_idx=0)` derivative; Burn's embedding operation has no padding parameter. This correction is confined to the new training path. A later matched control must share it with treatment.

The final start suite passed three tests in 18.77 seconds; the fresh-process resume passed in 15.74 seconds; the legacy CUDA regression took 12.21 seconds. The final build took 46.40 seconds, after the first CUDA test build took 2m26s. These are debug qualification timings, not production training throughput. The actual targets and advantages were fixed synthetic numbers on real visible fixtures. No episodes were trained, no new candidate was selected, and no playing-strength result follows.

The following qualification completes the real collection/update loop. The low-level CUDA core's synthetic qualification alone did not establish this. Fable review remains unavailable as recorded above.

## Real terminal-reward collection, update and restart

`public_feature_training_v1` now collects actual games with the successor, derives the existing terminal-reward GAE over complete learner physical decisions, updates on GPU 1, and installs both parameter sets before collecting the next batch. Both arms use the corrected observation runtime and padding guard. The control validates and records the same auxiliary rows, then zeros only the added inputs during scoring and training. The original features, terminal rewards and optimizer settings are identical.

The separate trajectory schema `mtg-kernel-public-input-trajectory/v1` binds the collection configuration, full behavior optimizer state, opponent identity, learner tensor and public rows. The separate checkpoint schema `mtg-kernel-public-input-checkpoint/v1` binds the full configuration, next update, optimizer hash and episode hashes. Relative optimizer filenames make fresh-output-root replay possible without provenance normalization. No existing trajectory or inference loader is relabeled.

Evidence: `E:/mtg-postboard-campaign-20260920/public-learning-engineering-001`. The source parent is `d232de0f`; `manifest.json` records the exact implementation-file hashes compiled before final formatting and tests. Frozen release binary SHA256: `60501745925b1f25204bc0aa0c952a3fd11d6abb53c6d3c1e4bec1ca43d10855`. The final changes after that build are formatting, a narrower internal method visibility and an added alignment rejection test. The frozen binary is preserved; the debug regression build checks the final code.

| Real-game engineering check | Result |
| --- | --- |
| Two arms, four updates and eight games each | 8 distinct arm/update units, 16 arm/game units; all natural |
| Structured arm repeated with restart after update 1 | 4 additional updates and 8 games; all natural |
| First-update trajectories across arms | Equal after removing only configuration identity and the input-enabled flag |
| Uninterrupted versus fresh-process restart | All four optimizer snapshots, checkpoints and all eight trajectories byte-identical |
| Actual learner data per arm | 1,100 physical groups, 1,194 substeps, maximum group width 6 |
| Control projections and both moments | Remain zero |
| Structured cost and prevention projections | Both acquire nonzero weights |
| Final Adam ages | Legacy 32404, public 4 |
| Fixed timing gate | First update 21.70 seconds, projected four-update 86.80 seconds below 180 |
| Actual process times | Control 29.01 seconds; structured 26.84; split replay 22.94 plus 23.94 |
| Shared collection, replay and population regressions | Four passed; public catalogue/alignment tests two passed |

`completion-audit.json` independently checks schedules, hashes, state chains, natural terminals, group boundaries, auxiliary alignment, counts and receipts. Final structured optimizer SHA256 is `09766f6c5d03023874bb6975b2b8a672d7b1a26a00a431769c5ac93fcbce4a9b`. Compilation took 8m15s for the first release binary and 1m09s for the debug regression build. Total native qualification process time was 102.73 seconds. Collection accounted for approximately 5.5 seconds per complete arm; update/replay/publication dominates this tiny workload.

This is eight scheduled cases across three ordered matchups and both learner seats, all preboard against familiar A48. There are 86 learner decisions per arm with active public prevention and none with the cannot-prevent flag. The four-update endpoints are engineering artifacts, not selected candidates. No win-rate comparison, cost-versus-prevention attribution, held-out opponent result, sideboarding competence, human game or competitive-strength claim follows.

The evaluator qualification below completes the next integration step. A matched learning pilot must still start both arms from untouched g115, preserve canonical retention and evaluate against opposition outside the training pool. Do not reuse these four-update endpoints as pilot parents, substitute familiar-A48 engineering games for independent evaluation, or repeat the earlier failed broader-exposure pilot. No new paid compute or broad campaign is authorized. Fable's zero-read quota failure remains an explicit independent-review gap.

## Successor checkpoint BO3 evaluation

`public_feature_evaluation_v1` loads explicitly tagged legacy, zero-projection warm-start or public-checkpoint sources. Public checkpoints bind their training configuration and optimizer state, validate both ages, and install the actual parameters plus projections. Their combined model identity includes legacy weights, both projection matrices and the input-enabled flag. Inference creates no GPU device and performs no update. The source is compiled with the CUDA feature to reuse the qualified snapshot decoder.

The evaluator reuses `run_learned_bo3_session_v1`, the existing physical-seat router, KeepSevenV2, terminal handling, seed resets and sideboard-action validation. Fixed game-two configurations must preserve the registered 75 and carry into game three. The V3 opponent has an explicit unscored singleton opt-in that still consumes its normal physical-seat sampler draw; legacy scoring/training APIs are unchanged. Full decision traces are optional for later timed evaluation.

Evidence: `E:/mtg-postboard-campaign-20260920/public-evaluation-engineering-002`. The evaluator implementation parent is `9c73bc7f`, plus the CLI stack correction. `public-evaluation-engineering-001` is preserved: its first reference process hit Windows stack overflow before writing an output directory or completing any match. A named 16 MiB worker, already used by other native CLIs, fixed the debug stack limit. No gameplay settings, seeds or gates changed.

| Completed engineering condition | BO3 executions | Natural games | Decisions | Sideboard moves |
| --- | ---: | ---: | ---: | ---: |
| Corrected-runtime g115 reference, both seats | 2 | 4 | 698 | 0 |
| Zero-projection warm start, both seats | 2 | 4 | 698 | 0 |
| Four-update structured checkpoint | 2 | 4 | 490 | 40 |
| Four-update zero-input control checkpoint | 2 | 4 | 490 | 40 |
| Structured checkpoint versus independent V3 | 2 | 6 | 1097 | 40 |
| Structured full-match replay in fresh processes | 2 | 4 | 490 | 40 |

All 12 executions completed: ten distinct arm/case units plus two replays, 26 games and 3,963 decisions. Reference/warm starts have exactly equal visible decision hashes, legal indices, seed resets, games and outcomes. Both learned match outputs replay byte for byte. Fixed sideboarding changes were exercised, including carrying the resulting configuration into game three. The V3 path handled eleven forced actions. Swapping in the control configuration for a structured checkpoint and corrupting the checkpoint pin were both rejected before gameplay.

V3 was regenerated from the original checkpoint, registry and initializer. Complete old/new transfer envelopes are equal after changing only `receipt.destination_build_git_head`; weights and Adam remain unchanged. This is explicit build-provenance rebinding, not raw cross-build hash equality. `import-comparison.json` records it. `qualification.json` and `independent-counts.json` audit complete output hashes, counts and decision sequences.

Evaluation process time was 156.47 seconds, below the unchanged 360-second engineering bound; V3 transfer took another 7.88 seconds. These are debug timings with full visible traces and per-process model loading, not production pilot throughput. The two matchup fixtures are Gates/Affinity and Faeries/Terror; these results establish execution and replay, not competitive playing strength. No engineering outcome was used to select a model. The original g115 human package is unchanged.

Six focused regressions passed: actual forced-V3 selection preserves both subsequent sampler streams and rejects V4; postboard preflight rejects changed registrations and unbounded matches; four existing seat-routing and cross-generation tests pass. The first new sampler test incorrectly assumed an empty main-phase menu was a singleton. Its failure is preserved in `regression-build-forced.log`; the corrected test uses the existing actual forced goaded-attacker fixture. Production gameplay code was unchanged by this test correction.

Next: build pinned release training/evaluation tools, qualify useful timing with traces disabled and all canonical ordered matchups, and freeze the matched public-input learning comparison before measurement. Preserve untouched corrected-runtime g115 as reference, zero-input continuation as control, independent V3 evaluation, matched physical seeds, canonical retention and paired analysis. The comparison changes both cost and prevention inputs together, so any eventual gain would not identify their separate effects. Fable remains unavailable until the recorded September 22 reset; no endorsement is implied.

## Explicit V3 spell-target repair in successor evaluation

The successor evaluator now opts into the previously qualified public unique-Spell reference repair for V3 opponents. It first tries unchanged V3 encoding, repairs only InvalidActionReference through a private non-stepping session copy, verifies unchanged executable candidate ordering, and uses the original scorer and sampler. Default V3, V4, training and human transport remain unchanged. Each match counts scoring repairs separately from diagnostic repairs; trace repair is enabled only for the opted-in physical seat.

The first full-match port qualification, public-spell-adapter-qualification-001, failed in diagnostic recording before the scorer reached the repair. The exact historical Wildfire/Faeries seed reached game three and returned StaleEnvironmentBinding: InvalidActionReference. Its outputs remain intact. The diagnostic wrapper had called the unchanged V3 human projection, which validates the rejected cache. It now uses the same public repair on an explicitly opted-in copy, preserving observation/action semantics and no RNG consumption. This is not a new model or relaxed legality rule.

Five focused tests pass, including both executable Pyroblast choices, stale/ambiguous identity rejection, unchanged valid tensors/scores/sampler draws, hidden-hand/library invariance, and actual trace-wrapper opt-in with eight unchanged selections. The fresh seven-match full-game qualification is public-spell-adapter-qualification-002; its result must be checked before claiming the repair is fully qualified. Preliminary compilation logs preserve a missing import and missing E-drive temp-directory error, corrected before gameplay. Fable's known quota failure remains a zero-read review gap; the maintainer's local execution authorization applies.

Follow-up: qualification-002 also failed before scoring because the trace recorder used V3 cache validation for the V4 physical seat. Recording now dispatches by each seat's actual generation, using the existing V4 action validation and observation builder for V4, and the explicit adapter only for opted-in V3. The V4 diagnostic produces the same visible bytes on valid V3-compatible positions and on the repaired public spell fixture, rejects stale decisions, and leaves both generations' sampler sequences unchanged. All five focused tests and nine existing V4 action regressions pass. The next fresh seven-case root is public-spell-adapter-qualification-003. Neither preceding failed root is a success or pooled measurement.

Qualification-003 completed all seven executions, 19 natural games and 4,037 decisions in 148.99 process seconds. Both fresh-process match replays are byte-identical. Valid-control on/off decisions and subsequent RNG draws are unchanged. The historical failed match and its replay each need one scoring and one diagnostic repair. Both learned/V3 seat cases retain every visible trace row and gameplay field from public-evaluation-engineering-002. Three frozen V3 feature-file hashes remain unchanged.

The initial final auditor rejected two historical sideboard resource means because old typed serialization emitted 1.9 and 5.6, while serde_json::Value promoted the same f32 values before emitting longer decimals. The corrected auditor compares exactly the native bits for only own_hand_mean and own_lands_mean, the two schema-declared Option<f32> fields. All other fields remain exact. Actual differing pairs share 3333f33f and 3333b340 little-endian bytes. The original analyzer is preserved, no games were rerun or outputs overwritten, and fresh-process replay remains raw byte equality. qualification.json and independent-completion-audit.json record this limitation and the separate repair counters.

Release coverage runner: python/tools/qualify_public_coverage_v1.py. It reads only the previous seven-list qualification inputs, verifies all 98 ordered matchup/physical-seat cells and legal fixed 60/15 configurations, qualifies a release replay of the exact debug failed case, checks a cheap first-job projection, then runs the other canonical cases with four local workers and one fresh-process replay. It performs no strength interpretation. The four-update engineering checkpoint is never a pilot parent. Release coverage is pending until its own complete receipt exists.

## Public feature extraction must use the V4 actor contract

Release coverage-001 completed 97 of98 canonical matches, then failed in the learned model's feature extraction on Terror/Wildfire, seed11612740169841191669, game3, environmentseed2516014300937541793. This is an incomplete qualification, not strength evidence. The release replay of the earlier debug failure was byte-identical; the first seven-match cost projection was37.76worker seconds versus300. Alloutputs remain preserved, and no training started.

PublicInputPlayPolicyV1::select_with_scores still obtained its auxiliary observation/actions through the V3 diagnostic helper before calling its V4 encoder. This prevented a legal V4 spell-target position from reaching the scorer. It now requests the already validated V4 projection. Added features, weights, reward, optimizer, sampler and hidden-information boundaries are unchanged. A new real spell-target test verifies zero-projection scores/value/eightdraws match the original V4 policy, and nonzero projections plus auxiliary rows are invariant to hidden-hand and library-order variants. It passed in2.26seconds. The same helper is used for learning and inference, so subsequent training must use the corrected binary.

Qualification-004 adds the exact failed public-feature case and fresh-process replay to the prior seven-case qualification. Fresh coverage-002 is required after that passes. Production-tools-003 will use four codegen units for mtg-kernel only, retaining release optimization, thinLTO, pinned toolchain, BelowNormal and at most four Cargo jobs, to reduce the observed single-unit compilation bottleneck. This is a new recorded build configuration, not a change to frozen tools. Actual replay and cost checks remain mandatory; no unmeasured throughput claim follows.

Final corrected-source qualification: public-spell-adapter-qualification-004 completed9matches,25natural games,5869decisions in292.59debug process seconds. The added public-feature failure and replay both complete with916decisions/3games each and identical bytes. All seven earlier cases retain exact gameplay and visible traces. independent-completion-audit.json verifies hashes, bounds, natural-game counts and step sequences. These are execution checks, not training or strength evidence.

Production-tools-003 builds native sourceb8105521c7f30d4c56c3f3b06f4e4391437423e9 with pinned Rust1.94.1/MSVC14.50, release thinLTO and package-specific four codegen units. Build elapsed281.34seconds, versus675.38seconds for the preceding source/build configuration; this is an observed setup comparison, not a controlled general speedup estimate. Frozen evaluator SHA256befd29580b938b648900f27ea6c72b3dd61c4a52748b7cf1cd8815f2cbf60006; trainer15bce7f20ecf8f401ae7c29f05895e5aa96e8990b25ee02112cdbe95eeb1110a.

Final release coverage: public-canonical-coverage-002 is PUBLIC-CANONICAL-COVERAGE-ENGINEERING-PASS. All98canonical matchup/seat cases,218natural games,33311decisions and2416sideboard moves completed. Including the debug/release failure replay and fresh-process coverage replay:100executions,223games,34514decisions. Both replay checks are byte-identical. All97previously completed cases retain exact recorded gameplay fields and decision counts; production traces are disabled, so this is not an all-action comparison. The previously missing Terror/Wildfire case is now complete. Independent audit verifies request/model/output bindings and all98unique cells. No previous outputs were overwritten or pooled.

Canonical evaluation used45.82worker seconds, plus2.35profile replay and0.73fresh replay. The first-job projection29.87was below300; complete2352-match projection1099.57worker seconds is below3600. That is an estimate from a four-update engineering endpoint, not guaranteed final-pilot cost. Allnativeprocesses ended. Next is the bounded matched learning design in public_feature_pilot_20260920.md, which remains preparation until its scripts, analysis gates, full-batch timing and complete g115 reference are frozen and verified. No new training or model-strength comparison has launched. Fable review gap and human/field limitations remain unchanged.
