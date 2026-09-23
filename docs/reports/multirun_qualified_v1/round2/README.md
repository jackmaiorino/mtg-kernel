# Multirun qualification receipts, round 2 (2026-09-23, after the Fable verdict)

Non-evidence engineering receipts. Every file here was produced by the
launcher at commit `136443d3` (its SHA-256 is stamped in the receipts and the
guard refuses any other launcher) and the executable `mtg_kernel` lib-test
binary `62bf66b1...5504`, built from this branch's Rust sources (resume-aware
and sentinel tickets). Bulk Stores stay under
`D:/multirun-qualified-v1-work/q6` and `q6-segment` (not committed).

## Workload and hosts

10 runs: 2 arms (`base`, `envrand` with environment randomization v2) x 5
seeds, 128-update records, pilot topology 2x32, broker target 16. Hosts
inventoried: Jack's PC eligible (RTX 4070 SUPER + RTX 3050); HaleysPC eligible
(RTX 4060 8 GB, 16 logical CPUs, reached over Tailscale on the LAN path);
RunPod reachable, excluded (Linux training bytes differ from Windows).

## Qualification (`compute-choice.json`)

| Allocation | Episodes/s | Projected, 10 runs |
| --- | --- | --- |
| 1@gpu0 (serial golden) | 11.04 | 77 min |
| 1@gpu0 + 1@gpu1 | 19.40 | 43 min |
| 2@gpu0 + 1@gpu1 | 25.38 | 37 min |
| 2@gpu0 + 2@gpu1 | 30.65 | 30 min |
| 2@gpu0 + 2@gpu1 + 1@HaleysPC | 28.25 | 27 min |
| **2@gpu0 + 2@gpu1 + 2@HaleysPC (selected)** | **40.12** | **20 min** |
| 2@gpu0 + 2@gpu1 + 3@HaleysPC | 42.56 | 20 min |

Every concurrent run of every leg is byte-identical to its serial golden
(12-update prefix), including runs placed on HaleysPC; the serial repeat
digests equal the golden's. Full-length sentinel on the selected allocation:
each arm's first run alone on GPU 0 for all 128 updates, then both arms on
each of the three devices at full width; all 6 entries byte-identical to the
serial references (233 Store outputs each). Peak GPU memory in the sentinel
stayed outside the 512 MiB margin (HaleysPC 5,689 of 8,188 MiB; GPU 0 9,218
of 12,282; GPU 1 5,526 of 6,144).

## Launch (`experiment-manifest.json`, `runs/`)

All 10 runs complete at generation 128 in **868 s (14.5 min), 94.3
episodes/s**; every prefix audit identical; launch-time GPU identity (name and
UUID, local and remote) matched the receipt. Measured serial full length is
about 254 s per run (`serial-walls.json`), so the 10 runs take about 42.3 min
serially: **2.9x**. Local runs took 322 to 464 s and HaleysPC runs about
645 s: a 5-run arm fits in one wave at about 2.5x one serial run, and a
4-run arm fits locally in one wave at about 1.3 to 1.8x.

## Resumed segment (`segment/`)

Two runs resumed from their generation-12 serial goldens and trained to
generation 28 through the launcher (qualification, sentinel and launch all at
the segment). `resume-equivalence.json`: all 56 Store outputs through
generation 28 are byte-identical to the uninterrupted launched runs, 28 of
them trained after the resume point.

## Superseded (`superseded/`)

- `q3-*`: the first round-2 pass at `201f09b6` (launch 15.3 min, valid),
  superseded because later fixes changed the launcher.
- `q4-*`: at `cc3036db` the projection selected 3 processes on HaleysPC; at
  full length its 8 GB card sat at 7,867 MiB and 100 percent and those runs
  took about 2,800 s against about 350 s locally (46.9 min for the
  experiment). This led to the remote memory-fit rule and the sentinel's
  full-length memory check (`20e86496`, `136443d3`).
- `qualify-aborted.err`: a sweep stopped because stale GPU readings right
  after a leg skipped allocations that fit (fixed in `201f09b6`).

## Other receipts

- `raw-harness-refusal.txt`: a raw 65-update harness run is refused in under a
  second with no Store created (the 64/65 boundary).
- `monitor/`: 5-second CPU, local GPU and (every 15 s) HaleysPC GPU samples.
- `throughput.png`: the qualification table as a chart.
