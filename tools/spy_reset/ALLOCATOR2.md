# Allocator v2 workflow

Implements collab `LANES/allocator-redesign-20261012/DESIGN.md` (Spy only, arms `current`,
`q`, `S`, `H`). Nothing here launches qualification or formal runs by itself.

## Engine

- Mode `s4a-allocator-v2` (`regret_census_v1/stage4a/allocator2.rs`): env `ALLOC2_CASES`
  (JSONL of `{id, arm, controller_seed, case}`), `ALLOC2_WEIGHTS` (S/H controller bound to
  this process's parent), `ALLOC2_TOTAL` (default 2,048,000), `BUDGET_KEY_MODE=repaired`,
  plus the `s4a-allocator` bank settings (`BANK_SPEC`, `OPPONENTS`, `S4A_MODEL`).
- Mode `s4a-allocator-v2-probe`: the same with probe cases only (supported-node refresh
  probe; prepared, not run).
- Rows (`allocator2_game`) log per consultation the shared features `s`, the pooled player
  representation `rep` (state, action mean, action max; 192 values), executable amounts and
  members, coalesced `q`, `pi`, `mu`, the critic value, the draw `u` and the choice. Every
  focal decision carries `focal_physical` and `nonforced`; every search its `discovery`
  counters; the row carries `player_after` (frozen player hashes).

## Python (`tools/spy_reset/allocator2.py`)

1. `schedule --out DIR --exclude-seeds ...`: every domain (qualification 18, smoke 2,
   dev-c1/dev-c2 324 each, devval 108, fresh-panel 216, fresh-train-rep1..3 648 each).
2. Qualification (q and current arms; the initialized S/H controllers behave exactly as q)
   via `queue`. Then `freeze-norm` per parent and arm from qualification rows, and `init`
   per parent and arm (`--label dev`; fresh replications use `fresh-rep1..3`).
3. `train PLAN.json`: `{root, binary, binary_sha256, host, arm, schedule, domain: "dev-c1",
   cycles: 1, out_dir, slots, init: {r1, r2}, formal, throughput_evidence}`. A second cycle is
   `domain: "dev-c2"` with `init` the first cycle's endpoints and `init_adam: {r1, r2}`
   their matching Adam files, declared before collection. Global update indices continue
   from the endpoints. A resumed output directory must retain its original case, budget,
   binary, optimizer and credit configuration; incompatible rows are never reused.
4. `panel-jobs --panel devval --arms current q S H --weights S:r1=... ...`, then `queue`.
5. `allocator2_report.py --schedule DIR --panel devval --rows DIR --arms current q S H`
   (`--learned L1 L2 L3` with `--arms current q L1 L2 L3` for the fresh panel).
6. `replay --ledger TRAIN-LEDGER.jsonl` recomputes every update byte for byte.
7. `probe-select --rows DEV_ROWS --out probe.jsonl`, then `queue` with `probe_cases`.

## Development constants

PPO: lr 3e-3 (design 3e-4 failed calibration; see `calibration/ALLOCATOR2-CALIBRATION.md`),
Adam betas 0.9/0.999, eps 1e-8, 4 epochs, 3 six-game minibatches, clip 0.2, gradient-norm
cap 1, critic coefficient 0.5, KL stop 0.02, no entropy bonus. Base q: `p = 0.5 r`, split
0.5/0.4/0.1; behavior `0.8 pi + 0.2 q`. Controller: 64 tanh units, Xavier hidden and critic
weights (seeded), zero biases and zero actor output. Normalization frozen per parent and arm
from qualification consultations (fixed scalings before that).

## Paired terminal credit

The prospective amendment is collab `LANES/allocator-redesign-20261012/PAIRED-CREDIT.md`.
Set `credit_roots: 1` or `3` in a new training plan after paired qualification. A native
case adds `credit: {max_roots: K, sample_seed: U64}`. Each factual game keeps a uniform
reservoir of K consultations, then resumes exact private snapshots with just that request
forced to skip. Future searches remain enabled with the saved budget. The terminal
difference, divided by reservoir inclusion probability, replaces the critic advantage.
Critic fitting is disabled in this mode. Missing/nonterminal suffixes refuse an update.

Set `credit_roots: 0` in q/S/H evaluation queues to use the same event-keyed sampling
without forks. Omission uses the historical sampler; the `current` arm and supported-node
probes remain separate queues. Never put paired cases in `probe_cases`. Rows record
`credit_config` and `game.paired_credit`, including suffix costs. The process ledger
charges full CPU/wall time for factual play, cloning and all continuations.

Formal paired queues/training require throughput evidence with workload
`allocator-paired-v1/s4a-allocator-v2`, matching `credit_roots`, binary hash, host and slots.
The old qualification does not qualify this new workload. Use the supported compute
launcher, current reservations and a prospectively bounded engineering plan.

`allocator_credit_calibrate.py --out FILE` runs synthetic known-signal/zero/harmful-search
tasks through the real update. `allocator_credit_report.py --weights W --rows R... --ledger
LEDGER.jsonl --out FILE` measures whole-game actor-gradient variance, pair discordance and
variance times measured CPU at a frozen controller. Include all failed attempts in the
ledger. Compare with a matched no-fork collection; this report is not an efficacy test.
