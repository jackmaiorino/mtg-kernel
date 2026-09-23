# Qualified concurrent launcher for independent training runs (v1)

Status: branch `opus/multirun-qualified-v1`, PR #107. Fable review
2026-09-23 (`collab/FABLE-REVIEW-20260923.md`) countersigned the mechanism
and required M1 to M9; this revision applies them (map at the end).

## What it does

An experiment is a fixed set of independent runs: one seed and one Store per
run, optionally grouped into arms with per-arm knobs, optionally a resumed
segment of existing Stores. `python/tools/multirun_launcher_v1.py` runs them
concurrently, one process per run, across placement slots
(`<capacity>@<host>:<device>`, for example `2@0+2@1+2@haleyspc:0`), and
enforces `C:/Users/Jack/COMPUTE-POLICY.md` items 2 to 5 at the launch point:

1. `inventory` records Jack's PC, HaleysPC (SSH over Tailscale) and RunPod
   (read-only account query) with a timestamp, eligibility, reason and GPU
   identities.
2. `qualify` runs every run for a short prefix, first one at a time (the
   serial goldens, plus a serial repeat whose digests are stored), then under
   each candidate allocation in increasing concurrency, comparing every
   concurrent run byte for byte with its serial golden and projecting the
   completion time (a simulation of the scheduler with each host's measured
   per-run duration plus staging overhead). It then runs the **full-length
   sentinel** on the fastest qualified allocation: each arm's first run alone
   at full length, and every placement x arm at full length and full width;
   any byte difference, or any device (local or remote) peaking inside the
   512 MiB fit margin, disqualifies that allocation and the next fastest is
   tried. Candidates are explicit (`--allocation`) or adaptive
   (`--auto 0,1`). The receipt binds the executable, the runtime `data/` tree,
   the launcher's own hash, the workload (knobs, arms, seeds, length, segment,
   parent Stores by content) and the inventory.
3. `launch` spawns nothing unless the receipt matches this exact executable,
   data, launcher and workload; the inventory is under 24 hours old and covers
   all three hosts; serial and parallel collection were both measured; every
   eligible host was measured; the goldens, serial repeat and sentinel outputs
   on disk still hash to the receipt; the sentinel passed on every placement
   and arm of the selected allocation; the selected allocation is the fastest
   qualified one; each GPU it uses is the device the receipt measured (name
   and UUID, local and remote); and enough GPU memory is free now. It writes
   one entry per run in `experiment-manifest.json` plus a `run-manifest.json`
   per run, samples CPU and GPU every 5 seconds (flagging two consecutive
   60-second windows with runs waiting and the CPU under 60 percent busy), and
   audits each run's qualification prefix against its serial golden. A run is
   complete only if it exited cleanly, reached its target generation and
   passed that audit; otherwise the run and the experiment fail and the
   command exits non-zero.
4. `verify` reruns chosen launched runs alone at full length and compares
   every output byte (optional extra evidence; the sentinel is the enforced
   check).

## The ticket gate inside the harness

Each process receives `MULTIRUN_LAUNCH_TICKET`. `multirun_pilot_v1` checks it
before touching any Store, device or thread
(`mtg-kernel/src/multirun_launch_ticket_v1.rs`). The limit is on planned
updates **summed across the process's runs** (run count x record length,
whatever the stop point): at most 64 runs ticket-free as a small correctness or
timing check; 65 or more needs a ticket, so a long record stopped early cannot
escape it. Every ticket binds the executable hash, seed, record length,
resume generation and stop generation, and one run per process. Kinds:
`qualification` (no receipt yet; stop within 32 updates past the resume
point), `sentinel` (no receipt yet; the full-length reference runs), `launch`
(binds the receipt hash). Neither this nor the Python check is a security
boundary against a deliberately forged receipt or ticket.

CI note: the harness test that calls the gate is ignored, CUDA-gated and
Windows-gated, so CI never executes the hook. CI runs the gate's unit tests
(including the 64/65 boundary); the hook itself is exercised only by local
launches, and the refusal receipt below is that local evidence.

## Semantics preserved

Concurrency is between processes only. Each run keeps its own seed, Store,
weights, batch order and optimizer state; nothing is shared between runs.
Qualification uses the production record stopped at a checkpoint boundary, so
a serial golden is a true prefix of the production run. A resumed segment
starts every execution from a private copy of its parent Store. The harness
honours `MULTIRUN_STOP_AFTER_GENERATION` only at checkpoint boundaries (every 4
updates; any other value silently trains the whole record), so prefixes are
multiples of 4 and at least 8, and a watchdog (local and remote) kills a
process that passes its stop generation. The harness exits 0 when its filter
matches nothing, so a run counts only when its log shows the test passed.

## Qualification receipts

Committed under `docs/reports/multirun_qualified_v1/` (each round has a
README). Round 2 is the evidence for this revision; round 1 is the pre-verdict
record.

Round 2 (launcher `136443d3`): 10 runs, 2 arms x 5 seeds x 128 updates, on
Jack's PC and HaleysPC.

| Allocation | Episodes/s | Byte-identical to serial golden |
| --- | --- | --- |
| 1@gpu0 (serial) | 11.04 | golden + stored repeat |
| 2@gpu0 + 2@gpu1 | 30.65 | 10/10 |
| 2@gpu0 + 2@gpu1 + 2@HaleysPC (selected) | 40.12 | 10/10 |
| 2@gpu0 + 2@gpu1 + 3@HaleysPC | 42.56 | 10/10 |

The full-length sentinel on the selected allocation matched on all 6
placement x arm entries (233 outputs each) with every device's peak memory
outside the margin. The guarded launch finished all 10 runs in 14.5 min (94.3
episodes/s) with every prefix audit identical, against about 42.3 min for the
same runs serially (measured serial full length about 254 s per run): 2.9x. A
resumed segment (generation 12 to 28, both arms) reproduced the uninterrupted
runs' outputs byte for byte. Measured serial time is the reference: the
prefix-based projection is conservative for every allocation but most for the
serial one (77 min projected, 42 min measured), so it overstates the speedup
(3.8x projected, 2.9x measured) while its ranking of the parallel candidates
held.

Superseded round-2 passes are kept: one at `cc3036db` selected 3 processes on
HaleysPC from the prefix, and at full length that 8 GB card crowded (7,867 of
8,188 MiB, 100 percent), its runs took about 2,800 s against about 350 s
locally, and the experiment took 46.9 min. The remote memory-fit rule and the
sentinel's full-length memory check came from that run.

## Binding constraint and next lever

GPU memory, not CPU, caps local concurrency: each process holds about 2.95 GB
on the 12 GB card and 2.6 GB on the 6 GB card while the CPU averaged 37
percent at the round-1 selected width. cubecl-cuda 0.10 sets
`max_page_size = total_memory / 4` (`runtime.rs:103`), and an allocation above
the largest sub-slice pool reserves a whole page. Capping it per process, as a
narrow vendored patch like the existing `burn-cubecl` one, is the next lever;
it must pass this same byte-identity gate and is not part of this PR.

## Launch paths: guarded, migrated and blocked

Guarded (refuse missing or incompatible evidence before spawning):
`multirun_launcher_v1.py launch`; and `multirun_pilot_v1` itself above 64
summed planned updates (ticket).

Legacy callers of `multirun_pilot_v1` on main: 39 launch statements in 20 files
across 6 campaign families. Every one planning at most 64 updates keeps
working. Those above 64 are refused on a rebuild from this branch until they go
through the launcher:

| Family (helper) | Refused statements | Shape | Status |
| --- | --- | --- | --- |
| `macro_selfplay_envrand_v2_rung_v1` (`Invoke-MacroTrainingRun`) | `formal.ps1:45` (512 updates, 3 seeds) | fresh run, ladder and envrand knobs | launchable now: one workload, 3 runs, knobs `MULTIRUN_LADDER`, `MULTIRUN_ENVIRONMENT_RANDOMIZATION_V2`; wrapper not rewritten |
| `regularized_continuation_retest_v1` (`Invoke-NativePilot`) | `full-horizon-training.ps1:246` (512, anchor beta, waves) | fresh or resumed, anchor beta | launchable now: knobs plus `segment`/`parents` for resumed waves; wrapper not rewritten |
| `scaled_selfplay_population_v1` (`Invoke-ScaledNativePilot`) | `correct-throughput-screen.ps1:34`, `preflight-screen.ps1:58,61,131`, `run-replay.ps1:58` | successor or retest segments (stop and resume) | launchable now as segments; wrappers not rewritten |
| same family | `run-initial-population-interval.ps1:39`, `run-population-interval.ps1:43` | population runtime | **blocked**: the harness asserts `stop == resume + 128` for this runtime (`native_science_loop_v1.rs:1644`), so no prefix qualification is possible without changing campaign validation |
| same family | `run-response-exploiter-build.ps1:186,191,203`, `run-response-exploiter-retry.ps1:315,320,332`, `run-response-exploiter-screen.ps1:174,187,192` | response-exploiter runtime | **blocked**: the harness permits only a stop at 4 or none for this runtime (`native_science_loop_v1.rs:1658`) |
| `response_exploiter_v2_campaign_v1` | `run-response-exploiter-v2-build.ps1:135,140,151,164`, `-preflight.ps1:101,116,122`, `-retry-build.ps1:152` | response-exploiter runtime | **blocked** (same assertion) |
| `response_exploiter_denovo_screen_v1` | `run-denovo-screen-build.ps1:70`, `run-denovo-512-screen-build.ps1:79` | response-exploiter runtime | **blocked** (same assertion) |
| `exploiter_probe_v3` (`launch_probe.py:1735`) | arm runs (3 runs x 512 per process) | multi-run process | migrate as one run per process (seed = base + offset + ordinal); byte equivalence with the in-process form is expected, not yet verified |

`run-native.ps1:23` in the population family forwards arguments for every lane
above and follows their status. The blocked rows need a ruling before this
gate ships on main: either Jack retires those finished campaign families, or a
follow-up lets those runtimes stop early for qualification only (a change to
campaign validation, reviewed separately). The question is posted in
`collab/FOR-JACK.md`.

Unguarded, need migration before substantial use (no ticket check at all):
`src/bin/cycle4_arm_v1.rs` via `run-cycle4-arm.ps1` (training); the other
ignored harness tests (`learning_smoke_k64_uniform_delta_v1`,
`learning_smoke_k64_failure_diagnostic_v1`, `pathfinding_run_k64_deep_v1`,
`qualification_measurement_k64_depth256_v1`,
`timing_probe_gpu_k_scaling_throughput` train;
`s1_mirror_saturation_eval_v1`, `ladder_saturation_eval_v1`,
`ladder_head_to_head_eval_v1`, `response_exploiter_mixture_eval_v1`,
`pathfinding_saturation_eval_v1` evaluate); the evaluation wrappers under
`scripts/experiments/population_v2_cycle4_v1/`; `checkpoint_shadow_stdio_v1`,
`tts_s1_corpus_v1`, `tts_s1_replay_v1`; the training examples; the
`python/mtg_kernel_rl` CLI. Off main and not covered: Codex's public trainer
launcher (its own `compute_throughput_v1.require_choice`), scripts under
`E:/mtg-meta-recovery-20260921`, and the IdeaProjects root runners. The
`command-template-v1` adapter is the migration route for any trainer that
takes a seed, an output directory, a stop point and optionally a resume
point, and publishes generation-named outputs.

## Placement notes

- HaleysPC has only a C: drive and no CUDA toolkit. `SshPowerShellExecutor`
  mirrors every local drive path under `C:/mtg-node/multirun-mirror/<letter>`
  and maps the drive per session with `subst`, stages the executable, the
  nvrtc/cudart DLLs and the CUDA headers (nvrtc kernel JIT reads
  `$CUDA_PATH/include`) with hash checks, runs through `cmd` redirection (so
  the log is not UTF-16), applies the same overrun watchdog and log check as
  local runs, returns outputs as tar and deletes the remote copy. Its GPU is
  sampled over SSH during every leg and launch, its per-process footprint is
  measured like a local device's, and the same fit rule applies.
- RunPod: the account is queried read-only. Pods are Linux, and the
  repository pins different training bytes per target (the per-target
  `train_state_sha256` witnesses in `mtg-kernel/src/native_trainer_v1.rs`), so
  a pod cannot reproduce a Windows serial golden. Qualifying RunPod needs a
  Linux build with Linux goldens and a lease guard.

## Verdict map (FABLE-REVIEW-20260923, PR #107)

| Item | Where |
| --- | --- |
| M1 full-length sentinel per placement and arm, enforced | `qualify` sentinel stage, `require_sentinel`; tests `test_m1_*` |
| M2 prefix-audit mismatch fails run and experiment | `launch`; `test_m2_*` |
| M3 serial repeat digests stored and compared | `golden.serial_repeat`; `test_m3_*` |
| M4 launcher hash and GPU identity checked | `require_choice`, `check_gpu_identity`; `test_m4_*` |
| M5 blast radius listed; knobs admitted; resume-aware tickets and segments | table above; `test_m5_*`; ticket tests `resumed_segments_*`; blocked rows in FOR-JACK |
| M6 ticket wording, 65 boundary test, CI note | this doc; `small_checks_need_no_ticket_and_substantial_runs_do` |
| M7 one countersign target; HaleysPC receipts, watchdog and log check | all round-2 receipts from launcher `136443d3`; SSH executor |
| M8 committed RunPod evidence | launcher reason text and placement notes |
| M9 refusal log committed | `docs/reports/multirun_qualified_v1/round2/raw-harness-refusal.txt` (round 1's log is `round1/raw-harness-refusal.txt`) |
