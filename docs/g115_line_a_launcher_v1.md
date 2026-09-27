# g115 line (a) guarded launcher (v1)

Engineering lane opus-line-a-launcher (goal `collab/GOALS/opus-line-a-launcher-20260927.md`). Builds and qualifies the launch path for the line (a) calibration, screen training and screen evaluation. It launches no measurement and reads no outcomes.

## Pieces

| Module (python/tools) | Role |
|---|---|
| `g115_line_a_seeds_v1.py` | Frozen seed manifest (`e3b01058`) read by hash; screen-evaluation labels; the block-seed episode schedule |
| `g115_line_a_manifest_v1.py` | Dry-run manifests from the roster manifest (`g115-line-a-roster/v1`) and the exposure tables (`g115-line-a-exposure-table/v1`) |
| `g115_line_a_guard_v1.py` | Every admission refusal before any spawn |
| `g115_line_a_windows_launch_v1.py` | Launch entry, reachable only through `g115_d3_windows_dispatch.ps1` (allowlisted by name, schema `g115-line-a-launch/v1`, Windows hosts) |
| `g115_line_a_prepare_v1.py` | Launch manifests from pinned inputs (throughput mode so far) |
| `g115_line_a_build_v1.py` | Engineering build of both native binaries from a clean detached worktree, pinned into `E:/pinned-binaries/<sha256>/` |

## Episode schedule (countersigned, CODEX #523 C1)

Rule `g115-line-a-episode-schedule/v1`: episode e = 10u + s (update u, slot s, e = 0..1999) receives the (e+1)-th `next_u64` of the kernel SplitMix64 (`mtg-kernel/src/state.rs`) seeded with the frozen block seed. This is the same expansion the BO3 evaluator applies to its run seed. Episode ids `la-b{bb}-i{uuu}-s{ss}` are shared by the matched arms of a block.

| Block | Block seed | Canonical schedule SHA256 |
|---|---|---|
| screen-training-pair/block/00 | 5129419802126035267 | `01d9885bf9f3fca240de30dbbc325f74f4b4ac6a2a68c9697621b1715f384438` |
| screen-training-pair/block/01 | 11192956680141436156 | `c425cce8dd953fc995dd84a902e896e5f8a09fb06b643c6187376b7b3a54759a` |

## Counts (director ruling 2026-09-27, R10)

| Job set | Composition E | Staged fallback B |
|---|---|---|
| Calibration (untouched g115), BO3 | 3,200 | 2,688 |
| Screen training, games | 8,000 (4 runs x 2,000) | 8,000 |
| Screen evaluation, BO3 | 16,000 | 13,440 |
| with the R7 holdout, BO3 | 18,240 | 15,680 |

Full-scope members (V3, D3 wrapper, four recent endpoints) play 448 cells per evaluated endpoint. The archival roles play only the 64 Rally-opponent cells. The latest dry run is in `docs/reports/g115_line_a_launcher_v1/dry-run/`. Every manifest there is non-launchable and lists its reasons.

## What the guard refuses

- **Throughput evidence.** Refused if missing or bound to another launcher, executable, data tree or work class. The Jack, HaleysPC and RunPod inventory must be under 24 h old. Serial and parallel runs must both be measured, with outputs identical to serial, and every eligible host measured. The shortest projected feasible completion is admitted; an older time bound is recorded, never a veto.
- **Byte worksheet.** Refused if unmeasured, over the job-set cap (12 GB calibration, 48 GB screen), over the joint 60 GB, or leaving less than 60 GiB free on the volume. Each job also reserves bytes in the D4 storage `Ledger` before it runs.
- **Scratch.** No scratch use without a scratch manifest first. No receipt may cite scratch as an input of record. The storage lock `collab/LOCKS/e-io.json` guards scratch fills; a waiter waits and never breaks it.
- **Order and identity.** Screen training needs a calibration completion record and a passed usability record. The composition comes only from the director's scope ruling. Calibration and screen evaluation run one pinned yardstick executable. Binaries run only from their pinned copies.

## Launch paths: guarded and not yet guarded

| Path | Status |
|---|---|
| Throughput timing check (ordinary BO3) | Guarded launcher and transport; bounded to 64 BO3 |
| Qualification BO3 set, calibration, screen evaluation | Admission checks implemented and tested; dispatch lands with the qualification unit |
| Screen training (public trainer) | Not yet migrated; the manifests are non-launchable |
| Line (b) screen | Not started (goal amendment of 2026-09-27 11:20) |
| Raw `public_feature_evaluation_v1` / `public_feature_training_v1` calls | Unguarded by design; this doc is the supported path |

## Open prerequisites (owners)

- **Training opponents.** At `500cfae9` no canonical-slot member is a trainable treatment opponent. V3 hits the V4-only guard and has no adapters. The recent role uses public-input checkpoints, which are evaluation-only. Both are in Codex's ownerless-blocker entry, which recommends opus-search-opponent. The D3 wrapper belongs to opus-search-opponent.
- **Frozen roster, exposure tables and weights.** Codex, in the C6 formats; the declaration's science is due 2026-09-28 18:00 EDT.
- **Archival imports.** opus-panel-export.
