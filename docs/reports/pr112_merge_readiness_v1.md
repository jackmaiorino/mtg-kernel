# PR112 merge preparation

Source repairs and baseline witnesses, 2026-10-01. Engineering checks only. The PR bodies and current checks record final readiness.

The maintainer assigned merge preparation for PR112 and selected a focused teacher PR with main prerequisites prepared separately. PR114 targets main; PR112 targets `codex/g115-main-prerequisites-v1`. Merge order is PR114, then retarget PR112 to main and check the resulting commit. Neither PR is authorized for merge by this preparation request.

Starting identities:

| Input | Commit |
| --- | --- |
| Main after PR111 | d9e0910a41b3a1411cb79b1e53a883f4d983ebae |
| Existing g115 prerequisite base | 29b9a6c876c4651fbc197408f9fcc6729926dc9b |
| Original PR112 teacher head | 70bcce959681da89757d04e59e0fa9876d4e1a5e |

Shared repairs are in PR114. The merge retains the expanded registry and V3 creature-cost support while incorporating main's deferred hash batching, source liveness checks and combat removal fixes. The explicit-deck session constructor now initializes the scan-menu contract. The live card-catalog canary checks the already admitted card-wave profile while preserving historical catalog literals.

The canonical pool and runtime catalog retain their original bytes. Eight existing experimental registry cards remain outside canonical pool membership and are admitted only by exact original id and complete-row SHA256. Unknown, missing, reordered or modified extension rows are rejected. The canonical non-token pool remains 171 cards. The locked Python test extra includes psutil, and generated feature and card-tag files were refreshed from their checked-in authorities.

Formatting applies to the Cargo workspace. Qualification dependencies and generated feature-identity authorities retain their previous bytes. CUDA-only adapters and test helpers now follow their callers' feature gates. Clippy simplifications preserve numerical operation order; explicit numerical/public input signatures retain narrowly explained argument-count allowances. No warning gate was removed.

Windows hosted tests also exposed the virtual-environment Python redirector as a reservation-release deadlock: WMI returned the redirector pid while the supervisor adopted under its child pid. The supervisor now starts with the base interpreter, preserving the requested work interpreter. Mock regression tests cover default/explicit environment selection, foreign interpreter preservation, fallback and alias refusal; the existing real WMI lifecycle tests remain required.

PR112 adds teacher-specific cleanup and portable scratch fixtures. Historical acceptance receipts remain unchanged and apply to their recorded commits. Current CPU fixture hashes were refreshed only from observed hosted failures, with source/run/log bindings in `pr112_merge_readiness_v1/golden_witness_5aff1fa9.json` and `golden_witness_4b37691e.json`. Both checkpoint episode lengths remain unchanged. Fresh GPU1 outputs are recorded in `cuda_ordinary_witness_bba7779a.json` and `cuda_teacher_witness_bba7779a.json`. The exact pins were refreshed from those outputs. The ordinary subprocess fixture now emits its hash on a separate line because serial libtest prefixes uncaptured output with the test name. Equality and numerical bounds are unchanged.

Baseline verification before the final CUDA fixture refresh:

| Check | Result |
| --- | --- |
| Canonical manifests and 70 focused offline Python checks | Pass |
| Locked Python shards, both operating systems | All four passed for each PR |
| Default, Store-feature and CUDA-feature Clippy | All passed for each PR |
| Full release suites, Linux and Windows | Passed at prerequisite 4783460d and teacher 7fee35bd |
| Isolated snapshot timing | Passed on both platforms, unchanged 40us limit |
| Native Store boundaries and host-safe CUDA tests | Passed on both platforms |
| GPU1 teacher numerical envelope | Both directions within 0.001; maximum log-probability discrepancy 9.238e-9 |
| GPU1 frozen trunk mask | Passed |
| GPU1 ordinary V4 restore and same-process determinism | Assertions reached; first witness then failed the historical exact pin |
| GPU1 separate-process fixture | First worker produced the current state; marker formatting prevented the second comparison |
| D3 replay and tamper fixture | Passed, four complete games, 227.95 seconds |

Hosted baselines: [PR114 run 36815972214](https://github.com/jackmaiorino/mtg-kernel/actions/runs/36815972214) and [PR112 run 36816903403](https://github.com/jackmaiorino/mtg-kernel/actions/runs/36816903403). Native witness: source bba7779a, Rust/Cargo 1.94.1, MSVC 14.50.35725.0, GPU1 RTX 3050, driver 596.36. The failed first device check is preserved at `E:/mtg-pr112-merge-readiness-20261001/native-check-001` with its exact binary under `E:/pinned-binaries/9bfeda6b0a490584f17d2e4a1aade177bbb7b1f2b99630121df11f2271c79762/`.

Fresh native repeats passed at prerequisite 87c8271e and teacher 7868e9e4: checkpoint restoration, same-process and separate-process determinism, the full teacher fixture, and prerequisite D3 replay. The earlier teacher frozen-mask and D3 passes remain valid because subsequent edits changed test pins and marker formatting only. `native_completion.json` records the successful exact source/binary/log identities. `PRUNE.json` records removal of owned scratch duplicates after retaining and verifying two complete archival copies, including failed first-run outputs. Hosted CI for the final heads is required before marking ready; its current status appears in the PR checks and bodies. Current CI runs each Store command with explicit exit propagation and reuses the full-suite library binary for the isolated timing check.

The active protected throughput run was allowed to release generation 40 before this lane dispatched generation 41 through `host_reservation_v1.dispatch`. Builds use four BelowNormal jobs and private SSD scratch. Small correctness checks are allowed by the compute policy; formal experiments, training campaigns, model selection, promotion, trainer adoption and playing-strength claims remain outside this assignment.
