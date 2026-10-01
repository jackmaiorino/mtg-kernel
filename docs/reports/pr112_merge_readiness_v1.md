# PR112 merge preparation

Status: in progress, 2026-10-01. Engineering checks only.

Jack assigned merge preparation for PR112 and selected a focused teacher PR with main prerequisites prepared separately. PR114 targets main; PR112 targets `codex/g115-main-prerequisites-v1`. Merge order is PR114, then retarget PR112 to main and check the resulting commit. Neither PR is authorized for merge by this preparation request.

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

PR112 adds teacher-specific cleanup and portable scratch fixtures. Historical acceptance receipts remain unchanged and apply to their recorded commits. Current CPU fixture hashes were refreshed only from observed hosted failures, with source/run/log bindings in `pr112_merge_readiness_v1/golden_witness_5aff1fa9.json` and `golden_witness_4b37691e.json`. Both checkpoint episode lengths remain unchanged. CUDA fixture pins await a fresh device witness.

Verification so far:

| Check | Result |
| --- | --- |
| Canonical manifest generator check | Pass |
| Manifest unit tests | 17 passed |
| Teacher combined-check offline tests | 16 passed |
| HaleysPC check-only runner offline tests | 24 passed |
| Held-spawn offline tests | 7 passed |
| Card-tag generator tests | 3 passed |
| Supervisor interpreter and alias regression checks | 3 passed |
| Pinned Rust formatting | Pass |
| Full hosted Linux/Windows Rust/Python matrix | Pending |
| Default, Store-feature and CUDA-feature Clippy | Passed at prerequisite d084d172, run 36814595564 |
| Linux Python shards | Both passed at prerequisite d084d172 |
| Windows Python shards and real WMI lifecycle | Both passed at teacher 4b37691e, run 36812673206 |
| Teacher CPU tests | Passed at teacher 4b37691e (Linux) |
| Current release suite, Store boundaries and host-safe CUDA tests | Pending at prerequisite 4783460d / teacher b5ff3dd6 |
| Teacher CUDA update, envelope and frozen-mask correctness | Pending on GPU 1 |

Small offline tests above ran on the workstation's installed Python. Hosted CI uses the repository's locked Python/toolchain. Native builds and tests stay on hosted CI while Jack's PC has a protected throughput run and HaleysPC has another active build. The remaining native packet is limited to existing GPU-1 teacher-update/envelope and frozen-mask correctness fixtures plus the affected V4 ordinary-update determinism fixtures. Small correctness checks are allowed by the compute policy; a formal run, training campaign, model selection, promotion, trainer adoption and playing-strength claim remain outside this assignment. Hosted CUDA lint compiles the feature but cannot exercise a device.

At 2026-10-01 00:45 EDT, Jack's protected run-s3 window remains active until about 03:35. HaleysPC has an active CUDA build and only 55.4 GiB free on C, below the 60 GiB reserve. No native build or GPU test was dispatched by this lane.
