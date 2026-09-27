# Line (b) terminal ExIt teacher machinery (v1)

Engineering record of lane opus-exit-teacher (goal `collab/GOALS/opus-exit-teacher-20260927.md`) for the recipe in `collab/CODEX-G115-EXIT-PROPOSAL-20260927.md` (v0.2 contract, v0.4 summary, 11:23 contract dispositions). Base `500cfae9`. Nothing here is playing-strength evidence. T = 0.25, c = 0.1, K = 16 and the rollout caps are parameters with the proposal's defaults.

Status: section 1 implemented and tested (9 unit tests, debug profile, feature `experimental-burn-net8-packed-cuda-v1`); sections 2 to 6 pending (design in `collab/FABLE-QUEUE.md`, "Opus lane exit-teacher: design review"; Codex dispositions in CODEX #522).

## 1. Collection sampler `unclamped-softmax-f64-icdf-u53-v1`

Code: `mtg-kernel/src/unclamped_softmax_sampler_v1.rs`. Replica: `python/tools/unclamped_softmax_sampler_v1_replica.py`. `fast_sampler.rs` is unchanged: its Q8 gap quantization and 16-nat clamp (`fast_sampler.rs:29`, `:44`) remain the deployment and evaluation samplers. The law is a finite-precision approximation of the unclamped softmax, not an exact one.

Rule (canonical contract JSON in the module, SHA-256 `61e91239...2b00`), for 1 to 65,536 finite binary32 logits:

1. Gaps `d_i = f64(z_i) - f64(z_max)` from the first maximal logit. Every maximal logit gets weight exactly 1.
2. Weight `w_i = exp_v1(d_i)` for `d_i >= -708`, else `+0.0`. `exp_v1` uses no platform math library: two-part ln 2 reduction, Horner over `RN(1/j!)` for j = 13..0, exact `2^k` scaling.
3. Sequential prefix sums `C_i` in legal-action order, `S = C_(n-1)`.
4. Draw: exactly one `next_u64` of the acting seat's continuing SplitMix64 stream, never reseeded; `u = (x >> 11) * 2^-53`, `t = u*S`.
5. Select the smallest index with `C_i > t`. `t < S` for every draw, so a selection always exists; the check fails closed. Zero-weight actions are never selected.
6. Every call verifies the pinned MXCSR (round-to-nearest-even, FTZ and DAZ off) and fails closed otherwise.

### Envelope and its derivation

Claim: `|P(i) - p_i| <= (4n + 16) u` for every action, with `u = 2^-53`, `n` the menu width, `P(i)` the selection probability over the `2^53` draw grid and `p_i` the real-valued softmax of the same binary32 logits.

| Source | Bound (first order in u) |
|---|---|
| Gap subtraction: one rounding, `d~ = d(1+delta)`, `abs(delta) <= u` | relative `abs(d) u` on `w_i`; since `p_i abs(d_i) <= 1/e`, at most `u/e` on `p_i`, and `u D` on `S` with `D = sum p_j abs(d_j) <= (n-1)/e` |
| `exp_v1` on exact `d` in [-708, 0] | relative `eta <= 4u`: Horner term j carries 2j+1 roundings, so `u e^x (2x+1) = 2.40u` absolute for `abs(r) <= x = ln2/2`, i.e. `3.39u` relative; reduction subtraction `0.35u`; ln 2 split error `1.2e-26` times `abs(k) <= 1021`; truncation `0.08u`; coefficient rounding `0.01u`; total `3.83u` |
| Sequential sums | increment `C_i - C_(i-1) = w_i + delta_i`, `abs(delta_i) <= u C_i`: `u` absolute per action; `abs(S - sum w) <= (n-1) u S` |
| Threshold product and boundary comparisons | for a boundary `c`, `fl` is monotone with relative error `u`, so the grid fraction below `c` is within `u` of `c/S`; two boundaries give `2u` |
| Tail floor | actions with `d < -708` get zero mass; all such mass is below `n e^-708 < 2.2e-303` |

Total: `(11n/8 + 10) u + n e^-708` (second-order terms are below `n^2 u^2 < 5.3e-23`). The declared `(4n + 16) u` covers it with margin for every `n >= 1`.

Measurements:

| Check | Result |
|---|---|
| `exp_v1` against a 60-digit decimal oracle, 203,069 points on [-708, 0] including both sides of every reduction boundary (replica, bit-identical to the Rust code) | max relative error 1.53 u (contract bound 4 u) |
| `exp_v1` against the platform `exp`, 2,000,007 points (Rust test) | max 1 ulp (test bound 3 ulp) |
| Grid measure per action (selection is monotone in u, so each action's grid interval is found by binary search) against a reference softmax; gaps 0 to 710, widths 1 to 65,536 | all inside the envelope; largest observed error 2.2e-16 (1.7e-16 at n = 65,536), bounds 2.7e-15 (n = 2) to 2.9e-11 (n = 65,536) |
| Power check: action at gap 19.37 | reproduced within 1e-6 relative; the legacy sampler gives it 29x its softmax mass, and the same check rejects it |
| Golden battery: 512 menus (widths 1 to 12, 64, 65, 300; gaps to 40 nats; repeated values), one seat-stream draw each | stream SHA-256 `9dee7479...ba8d`, pinned in Rust and reproduced by the replica |
| Underflow, ties, invalid input | zero-weight actions get zero grid mass; four equal logits split the grid exactly; empty, non-finite and over-width menus fail closed |

The fixture measurements check the implementation; the table above is the general argument. Still open (Codex #522 item 1): consistency with the probabilities of the actual loss backend (the CUDA device-1 f32 log-softmax), measured on collected rows once the sampler is wired in. The u53 argument does not cover f32 or CUDA differences.

## 2. Head-only optimizer mask (pending)

## 3. Root selection (pending)

## 4. Coupled terminal action search (pending)

## 5. Target and loss (pending)

## 6. Receipts (pending)
