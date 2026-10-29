# EX1 Stage P placement plan (proposal, 2026-10-29 18:10 EDT)

For the lead's approval. Nothing is launched. The run manifest is `D:/e-scratch/ex1-stagep-20261029/PLAN.json`, written by `tools/ex1/stagep.py plan`. It pins:
- the qualified binary `E:/pinned-binaries/2d68c061…`;
- the qualification manifest, c* (r2 0.0387, r1 0.0470) and kappa 1;
- 300 updates per chain;
- the parents and RT1 bases;
- the episode and placement keys: namespace `ex1-stagep-20261029`, run `ex1-stagep-<chain>`, global updates 0 to 299.

## Basis

- **Wall per update** is bounded by the longest episode's sequential searches (`QUALIFICATION.json` scaling), not by CPU-h divided by rate. Calibration chains at 7 to 12 cores averaged 295 s per update for r2 (median 264, p90 490) and 214 s for r1 (median 178, p90 300).
- **Desktop at 8 cores:** the guard's throughput record ran update 100 in 282 s, against 218 s at 10 cores. Projected per update: about 330 s (r2) and 245 s (r1), uncertain by about ±20%.
- **HaleysPC is not measured.** The plan assumes a Ryzen 7 3700X thread runs at about 0.65 of a desktop P-core, so r1 takes about 375 s per update. Its throughput record replaces this before launch, and the guard requires that record.
- **RunPod cpu3c:** 0.03 USD per vCPU-hour (catalog 2026-10-25). The ordinary-training qualification measured a vCPU at about 0.83 of a desktop core, so r1 takes about 295 s per update and r2 about 400 s. A pod chain trains on Linux, where libm makes hashes differ from Windows, so it is a self-contained chain. Before launch it needs a Linux build at 63a6e55df, a Linux resume check (about 1.5 h of setup and about 0.4 USD), and Jack's approval of the lease.
- **Storage:** 53 MB per update, so about 16 GB per chain. Every volume keeps a 60 GiB reserve. Jack's PC D: has 316 GB free. HaleysPC C: has 84.7 GB free, which fits one chain.
- **Launch assumed at 20:00 EDT 10-29.** The HaleysPC chain starts only when Track B's current reservation ends.

## Options

| Option | Host | Chains | Workers (cores) | Projected wall | End (EDT) | USD |
|---|---|---|---|---|---|---|
| **A (recommended)** | Jack's PC | r2-c1, r2-c2 | 10 (8 each) | about 27.5 h | 10-30 about 23:30 | 0 |
| | HaleysPC | r1-c1 | 10 (8) | about 31 h | about 31 h after Track B frees it | 0 |
| | RunPod 8 vCPU | r1-c2 | 10 (8) | about 25 h + 1.5 h setup | 10-30 about 22:00 | about 6.3 |
| B (no spend) | Jack's PC | r2-c1, r2-c2, r1-c1 | 10 (8 each, all 24 cores) | 27.5 / 27.5 / 20.5 h | 10-30 23:30, 10-30 16:30 | 0 |
| | HaleysPC | r1-c2 | 10 (8) | about 31 h | after Track B | 0 |
| C (no HaleysPC) | Jack's PC | r2-c1, r2-c2 | 10 (8 each) | about 27.5 h | 10-30 about 23:30 | 0 |
| | RunPod, 2 x 8 vCPU | r1-c1, r1-c2 | 10 (8 each) | about 26.5 h | 10-30 about 22:00 | about 12.6 |
| D (all paid) | RunPod, 4 x 8 vCPU | all | 10 (8 each) | 26 to 35 h | 10-31 morning | about 29: **over 25** |
| | RunPod, 1 x 32 vCPU | all | 10 (32 shared) | about 33 h | 10-31 morning | about 32: **over 25** |

All paid placements (D) cannot finish four chains within the 25 USD ceiling. The cheapest feasible split keeps the two longer r2 chains on Jack's PC and places at most the two r1 chains on pods (C, about 12.6 USD).

**Recommendation: A.**
- It finishes in about 27.5 h.
- It spends about 6 USD.
- It leaves 8 of Jack's PC's cores for G(i) and Limited.

**Trade-offs:**
- **B** costs nothing but holds all 24 desktop cores for about a day.
- **If HaleysPC stays held:** move r1-c1 to a second 8 vCPU pod (C).

The plan's allotment for paid chains is 8 USD for r1-c2, within the 25 USD ceiling. Each paid chain stops at its allotment and records Stage P inconclusive on budget.

## Launch steps (after approval)

1. **Jack's PC:** `stagep.py run --plan PLAN.json --chain r2-c1`, then the same for `r2-c2`.
   - Run each WMI-detached, outside any host_slots claim. The wrapper takes an 8-core host_slots claim per chain.
   - The desktop throughput record exists (`throughput/THROUGHPUT-desktop.json`; 282 s on 8 cores, state equal).
2. **HaleysPC:** when its reservation is free, do the following in order:
   - `stagep.py localize --host haleyspc --map D:/e-scratch=C:/mtg-node/e-scratch ...`;
   - copy `COPY.json`'s files, keeping the 60 GiB reserve;
   - add the never-preempt reservation wrapper to the localized plan;
   - run `stagep.py throughput` there, then `run --chain r1-c1`.
3. **RunPod**, after Jack approves the lease:
   - a Linux build at 63a6e55df;
   - `localize --host runpod`;
   - `throughput` on the pod (state recorded; the equality check is Windows-only);
   - `run --chain r1-c2` under the lease guard.
4. **Watch:** `stagep.py status --plan PLAN.json` gives each chain's completed updates, metered USD and projected end. A `STOP` file in the root stops every chain. An interrupted iteration is moved aside and rerun fresh on the next `run` (mtg-kernel#336).

## Byte budget (ARTIFACT-LAW clause 1)

- **Projected:** 64 GB over four chains.
- **Cap:** 70 GB.
- **Reserve:** 60 GiB on every volume. `run` refuses below the reserve or past a chain's share of the cap.
- **Catalog:** the root is registered in `ARTIFACTS/catalog.jsonl` (mtg-kernel-collab#366, merged).
