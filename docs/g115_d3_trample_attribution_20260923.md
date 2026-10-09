# D3 baseline drift: bounded trample attribution

September 23, 2026. All 43 observed baseline semantic mismatches are reproduced as equal to their archived stores when only the live-engine trample correction is reversed in an isolated diagnostic build. Both preselected equal-store controls remain equal. This attributes the observed drift to that correction on these 45 conditions. It does not establish parity for the 606 missing baseline conditions, fix the search timeout, or authorize another formal attempt.

Control source `e990740a76726bb05f34bd2dee872f89e55f0671` is the formal native source `cd41885e0ac05586d89bd4b2b7fb1284248689ef` with only the `engine.rs` part of `2b3bc6bf` reversed: one file, nine insertions and 26 deletions. D2 and search code are unchanged. This temporary old-behavior control is not eligible for formal play. Executable SHA-256: `f1069ab05911dc05e5a1a4ccd3ed7a55448bf250feee9bd79326e51307782517`.

The fixed diagnostic includes every mismatch from the invalid collector plus the first two lexicographically ordered equal-store controls. Inputs, seeds and model bytes are preserved; only paths and the declared V3 destination-build provenance are rebound. Comparison uses the frozen two-field normalization, with no additional ignored fields. No aggregate outcomes or strength statistics are computed. All original formal and archive stores remain unchanged.

| Check | Result |
|---|---:|
| Original mismatches now archive-equal | 43/43 |
| Fixed controls archive-equal | 2/2 |
| Serial/parallel qualification stores byte-identical | 4/4 |
| Recovered native stores independently rehashed and archive-compared | 53/53 |
| Four-case serial time | 4.047 s |
| Four-case four-worker time | 1.281 s |
| Full 45-case four-worker time | 14.312 s |

Local admission refused another lane's training. Fresh three-host inventory then found compute host clear, the maintainer's occupied and RunPod HTTP200 with zero Pods using the corrected User-Agent. The bounded diagnostic ran on the compute host through `check-d3-trample-attribution.py`: CPU only, BelowNormal, eight-GiB reserve, 60-second per-job bound, fixed four-case serial/parallel qualification, and refusal if the projected 45-case pass exceeded 60 seconds. No paid allocation. The first offline package preparation followed historical provenance too deeply and failed before dispatch; its directory is retained. The corrected package relocates runtime descriptors only and copies model/evidence bytes exactly.

Evidence under `E:/mtg-g115-lineage-20260923/`:

- `d3-trample-diagnostic-build-001/`: build, source and toolchain receipts.
- `d3-trample-diagnostic-inventory-003.json`: fresh host inventory.
- `d3-trample-portable-002.zip`: SHA `1459e320ea4c968d3bf2fc4a0b2604ca6468caa21250b2f11f8022687fa8ac51`.
- `d3-trample-computehost-controller-001/recovery.zip`: SHA `adddc74eb52be30f159cdfd1b403a08c6d73efe8d8a2ab42596a9258ab125623`.
- `d3-trample-computehost-controller-001/recovered/run/completion.json`: SHA `f6551f81d7682430ac488cbeca04e0c98d8ccaccf0cb58895ea1d8d3e646bd91`.
- `d3-trample-attribution-verification-001.json`: independent local verification of all 53 stores, including the eight qualification executions.

Disposition: the original attempt stays technically invalid. A reviewed correction must reconcile the historical parity requirement with legitimate corrected-engine behavior, without restoring the trample defect, dropping affected conditions or silently broadening normalization. The search timeout still needs diagnosis and an independently reviewed feasible execution contract. No ADVANCE, NO-ADVANCE, power closure or D4 selection follows from this diagnostic.

Separate retained-timing check: `d3-invalid-timing-audit-001.json` reads execution receipts only. The 394 completed search matches have median140.444s and maximum1,538.964s; the three longest completed Elves mirrors took1,165.950/1,267.760/1,538.964s for1,228/1,628/2,039 decisions. A long-trajectory explanation for the1,800s timeout is plausible, but the timed-out trajectory itself remains unknown. These completed-job summaries are censored by cancellation and cannot estimate the full-panel runtime tail. No limit change or formal restart follows from them.
