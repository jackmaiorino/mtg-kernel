# Qualified concurrent launcher for independent training runs (v1)

Status: implemented on branch `opus/multirun-qualified-v1`. Design review
requested in `collab/FABLE-QUEUE.md`.

## What it does

An experiment is a fixed set of independent runs: one seed and one Store per
run, optionally grouped into arms with per-arm knobs. `python/tools/multirun_launcher_v1.py`
runs them concurrently, one process per run, across placement slots
(`<capacity>@<host>:<device>`, for example `2@0+1@1`), and enforces
`C:/Users/Jack/COMPUTE-POLICY.md` items 2 to 5 at the launch point:

1. `inventory` records Jack's PC, HaleysPC (SSH over Tailscale) and RunPod
   (read-only account query) with a timestamp, eligibility and reason.
2. `qualify` runs every run of the experiment for a short prefix, first one at
   a time (the serial goldens, plus a serial repeat of the first run), then
   under each candidate allocation in increasing concurrency. It records
   completed-work throughput, CPU and GPU use, and the projected completion
   time of the full experiment, checks every concurrent run byte-for-byte
   against its serial golden, and writes a compute-choice receipt naming the
   fastest qualified allocation. Candidates are explicit (`--allocation`) or
   adaptive (`--auto 0,1`, below); allocations that cannot fit in free GPU
   memory are recorded as capacity-skipped. The receipt also binds the
   executable hash, a digest of the runtime `data/` tree the executable reads
   by path, and the launcher's own hash.
3. `launch` spawns nothing unless the receipt matches this exact executable
   (SHA-256), workload (knobs, arms, seeds, length) and run set; the inventory
   is under 24 hours old and covers all three hosts; serial and parallel
   collection were both measured; every eligible host was measured; the golden
   outputs on disk still hash to the receipt; and the selected allocation is
   the fastest qualified one. It then runs the experiment, writes one entry
   per run in `experiment-manifest.json` plus a `run-manifest.json` in each run
   root, samples CPU and GPU every 5 seconds (flagging two consecutive
   60-second windows with runs waiting and the CPU under 60 percent busy), and
   audits each finished run's first qualification-prefix generations against
   its serial golden.
4. `verify` reruns chosen launched runs alone at full length (under launch
   tickets bound to the same receipt) and compares every output byte.

Each process also receives a ticket (`MULTIRUN_LAUNCH_TICKET`) binding the
executable hash, seed, planned length, stop point and, for a launch, the
receipt hash. `multirun_pilot_v1` checks it before touching any Store, device
or thread (`mtg-kernel/src/multirun_launch_ticket_v1.rs`): a raw invocation
planning more than 64 updates in one process is refused. Runs of at most 64
planned updates stay ticket-free as small correctness and timing checks. A
long record stopped early still counts its full record length, so a campaign
cannot be split into short stops to avoid the check. Neither check is a
security boundary against a deliberately forged receipt or ticket.

## Semantics preserved

Concurrency is between processes only. Each run keeps its own seed, Store,
weights, batch order and optimizer state; no forward call, batch or update is
shared between runs. Qualification uses the production record (same planned
length) stopped at a checkpoint boundary, so a serial golden is a true prefix
of the production run, and the post-launch audit compares the same bytes.

Measured on this host (probe before qualification, seed 424242, 8-update
prefix, 23 Store outputs): a serial repeat on GPU 0, the same run on GPU 1
(RTX 3050 versus RTX 4070 SUPER) and two concurrent copies on GPU 0 were all
byte-identical to the first serial run, including `run.json`, `latest.json`
and the lock file. No output needs exclusion.

Two harness facts the launcher now enforces:
- the harness honours `MULTIRUN_STOP_AFTER_GENERATION` only at a checkpoint
  boundary (every 4 updates); any other value silently trains the whole
  record. Qualification prefixes must be a multiple of 4 and at least 8, and
  a watchdog kills any process that passes its stop generation;
- the harness exits 0 when its test filter matches nothing, so a run only
  counts as complete when its log shows the test passed.

## Qualification receipts

Committed under `docs/reports/multirun_qualified_v1/` (see its README).
Workload: 2 arms x 4 seeds, 128-update records, qualified on a 12-update
prefix with `qualify --auto 0,1`.

| Allocation | Episodes/s | Projected, 8 runs | Byte-identical |
| --- | --- | --- | --- |
| 1@gpu0 (serial) | 7.00 | 107 min | golden + repeat |
| 1@gpu0 + 1@gpu1 | 12.56 | 57 min | 8/8 |
| 2@gpu0 + 1@gpu1 | 16.31 | 47 min | 8/8 |
| 2@gpu0 + 2@gpu1 (selected) | 18.57 | 39 min | 8/8 |

The guarded launch on 2+2 completed all 8 runs in 24.4 min (44.7
episodes/s) with every prefix audit identical. Two runs (one per arm) rerun
alone at full length matched the concurrent runs on all 233 Store outputs;
serial full length is about 530 s per run, so the experiment took 2.9x less
wall time than serial. One 4-run arm fits in a single wave (about 1.4x one
serial run); two arms take about 2.8x one serial run.

## Binding constraint and next lever

GPU memory, not CPU, caps concurrency on this host: each process holds about
2.95 GB on the 12 GB card and 2.6 GB on the 6 GB card while the CPU averaged
37 percent at the selected width, and the launch monitor flagged the idle CPU
capacity. The footprint comes from the CUDA runtime's memory pools: cubecl-cuda
0.10 sets `max_page_size = total_memory / 4`, and any allocation above the
largest sub-slice pool reserves a whole page (3 GB or 1.5 GB). Capping that
page size per process, as a narrow vendored patch like the existing
`burn-cubecl` one, would likely double or triple the width before the CPU
saturates. That is a bounded follow-up, not part of this PR: it changes the
build graph of every CUDA path and needs its own review, a byte-identity
requalification and the CUDA golden tests.

Adaptive sweep rule: serial on the first device, one process on every listed
device (measuring each device's footprint), then one more process on the
device with the most spare memory until nothing fits or two steps in a row add
less than 5 percent throughput. The launch refuses if other work currently
occupies the memory the allocation needs. Device ordinals assume CUDA and
`nvidia-smi` enumerate GPUs in the same order, which held here (each
ordinal's memory rose on the matching index).

## Launch paths: guarded and needing migration

Guarded (refuse missing or incompatible throughput evidence before spawning):

| Path | Guard |
| --- | --- |
| `python/tools/multirun_launcher_v1.py launch` | receipt validation (`require_choice`) plus per-run tickets |
| `multirun_pilot_v1` run raw with more than 64 planned updates per process | ticket check inside the harness |

Partially guarded (pre-policy, campaign-specific):

| Path | State |
| --- | --- |
| `scripts/experiments/regularized_continuation_retest_v1/full-horizon-training.ps1` | pins one campaign throughput manifest by SHA-256; not the policy's serial versus parallel comparison. Its pilot-harness runs above 64 updates now also need a ticket when rebuilt from this branch. |
| `scripts/experiments/scaled_selfplay_population_v1/run-population-interval.ps1`, `run-initial-population-interval.ps1` | same pattern; population and resume knobs are not yet launcher knobs |

Unguarded, need migration before substantial use:

| Path | Kind |
| --- | --- |
| `src/bin/cycle4_arm_v1.rs` via `scripts/experiments/population_v2_cycle4_v1/run-cycle4-arm.ps1` | training (per-arm pinned seeds) |
| ignored harness tests `learning_smoke_k64_uniform_delta_v1`, `learning_smoke_k64_failure_diagnostic_v1`, `pathfinding_run_k64_deep_v1`, `qualification_measurement_k64_depth256_v1`, `timing_probe_gpu_k_scaling_throughput` | training |
| ignored harness tests `s1_mirror_saturation_eval_v1`, `ladder_saturation_eval_v1`, `ladder_head_to_head_eval_v1`, `response_exploiter_mixture_eval_v1`, `pathfinding_saturation_eval_v1` | evaluation |
| `scripts/experiments/population_v2_cycle4_v1/run_payoff_panel_v1.py`, `run_m2_common_root_panel_v1.py`, `run-m1-cp7-panel*.sh`, other `scripts/experiments/**` wrappers | evaluation or training wrappers |
| `src/bin/checkpoint_shadow_stdio_v1.rs`, `tts_s1_corpus_v1.rs`, `tts_s1_replay_v1.rs` | evaluation |
| `examples/experimental_burn_net8_*`, `examples/native_trainer_true_k512_loss_capture_v1.rs` | training probes |
| `python/mtg_kernel_rl` CLI (`mtg-kernel-rl train`, `evaluate`) | Python trainer and evaluator |

Off main, not covered by this PR: Codex's public trainer launcher
(`python/tools/public_feature_pilot_v1.py` with its own
`compute_throughput_v1.require_choice`, branches `codex/public-*`), scripts
under `E:/mtg-meta-recovery-20260921`, and the runner scripts in the
IdeaProjects root. The generic `command-template-v1` adapter is the migration
route for any trainer that takes a seed, an output directory and a stop
point, and publishes generation-named outputs.

## Placement notes

- HaleysPC: `SshPowerShellExecutor` stages the executable (hash-checked on the
  remote) and the repository `data/` directory at the same absolute paths,
  because the test executable bakes its data paths at compile time, then
  archives each run's outputs back for the same byte comparison. The host was
  offline (Tailscale last seen 23 hours earlier, SSH timeout) throughout this
  lane, so this path is unit-tested only and unqualified.
- RunPod: the inventory queries the account read-only. Pods are Linux, and
  this repository documents Windows and Linux training bytes as different by
  design, so a pod cannot reproduce a Windows serial golden. Qualifying RunPod
  needs a Linux build with Linux goldens and a lease guard.
