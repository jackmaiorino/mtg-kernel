# cubecl page-size cap probe (2026-09-23): the lever is weak

Branch `opus/cubecl-page-cap-v1` vendors cubecl-cuda 0.10.0 with an opt-in cap
on the memory pool page size (`MTG_KERNEL_CUBECL_MAX_PAGE_MIB`; unset is
upstream behaviour). Probe: one pilot run (seed 5500001, 128-update record,
stopped at 12) alone on GPU 0, per cap, compared with the PR #107 serial golden.

| Cap (MiB) | Exit | Peak GPU memory above idle | Bytes vs golden |
| --- | --- | --- | --- |
| unset (3,072) | 0 | 2,915 MiB | identical |
| 512 | 0 | 2,947 MiB | identical |
| 256 | 0 | 2,465 MiB | identical |
| 128, 64, 32 | panic at the first update | n/a | n/a |

Findings: the patch is numerically transparent, but the page size is not what
fills the device. The largest single allocation is between 128 and 256 MiB,
and most of the roughly 2.9 GB per process is working set plus the CUDA
context. The best viable cap saves about 15 percent, enough for a third
process on the 12 GB card (5 local instead of 4), not the 2x to 3x the
PR #107 doc anticipated.

Why one serial run's wall time for 4 or 5 runs per arm stays out of reach on
this hardware: even with memory for every run, concurrent runs are slower than
a run alone. In round 2, local runs took 322 to 464 s concurrently against
about 254 s alone (1.3x to 1.8x), and HaleysPC runs about 645 s. A 5-run arm
therefore takes at least about 1.3x to 1.8x one serial run however it is
placed. Getting to about 1x needs cheaper runs (the trainer's per-run CPU
cost), not more placement.

Not proposed for merge as is: the saving does not justify a new patch to
every CUDA path's build graph. If a third process on GPU 0 is wanted, this
branch plus a byte-identity requalification through the launcher is the route.
