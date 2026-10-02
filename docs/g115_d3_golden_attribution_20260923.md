# D3 E1 golden-failure attribution

September 23, 2026. The requested checks are complete. All eight retained failures also fail under the reverse-only trample source e990740a. At the specified merge base 75406ffe, five tests pass and three do not exist. These results do **not** establish that the eight failures predate this branch, and the earlier blanket pre-existing description is withdrawn. Their precise causes among the intervening branch changes remain unidentified. Reversing trample alone does not resolve them.

| Test, shortened here | Formal cd418 | Trample reversal e990 | Merge base 75406ffe |
|---|---|---|---|
| ordinary trainer V3 fixture | FAIL | FAIL | absent |
| ordinary trainer V4 fixture | FAIL | FAIL | absent |
| ordinary trainer V4 fixed partition | FAIL | FAIL | absent |
| joined frame preflight/lineage | FAIL | FAIL | PASS |
| fixed Rally corpus | FAIL | FAIL | PASS |
| real Burn pair update/topology | FAIL | FAIL | PASS |
| current frozen literal | FAIL | FAIL | PASS |
| synchronous main golden store | FAIL | FAIL | PASS |

Each exact test name ran once per source. A zero-test match is recorded as absent, never a pass. Full names, exit codes, timings, assertion output, binary digests and toolchains are in:

- `E:/mtg-g115-lineage-20260923/d3-golden-attribution-reversal-001/`: clean e990740a76726bb05f34bd2dee872f89e55f0671; eight failures.
- `E:/mtg-g115-lineage-20260923/d3-golden-attribution-mergebase-001/`: clean 75406ffe3a5363491e889d3df177c50f5e29faf5; five passes, three absent.
- The original formal-source suite remains `d3-review-full-lib-002/`: 2,312 passed, eight failed, 105 ignored. No original result was replaced.

Both builds used four BelowNormal compiler jobs, the guarded build-resource check and the E: cache, preserving the 32-GiB workstation reserve plus eight-GiB build headroom. The merge-base build reused the now-idle cache after the reversal test executable was copied and hashed into its evidence root. No GPU, fixture update, test deletion, training campaign or formal D3 match was involved.

The independent full D3 baseline check remains 962 archive-equal and 62 explained by the pinned trample reversal over all 1,024 baseline conditions. It establishes parity for that inference workload, not correctness of the failing trainer/store fixtures. The E1 findings accompany the existing D3 review entry. No playing-strength result follows from them.
