# PR112 merge preparation

Status: in progress, 2026-09-30. Engineering checks only.

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

PR112 adds teacher-specific cleanup and portable scratch fixtures. Historical acceptance receipts remain unchanged and apply to their recorded commits. Exact CPU/CUDA goldens are retained pending fresh results; failures will be investigated before updating any current-build golden.

Verification so far:

| Check | Result |
| --- | --- |
| Canonical manifest generator check | Pass |
| Manifest unit tests | 17 passed |
| Teacher combined-check offline tests | 16 passed |
| HaleysPC check-only runner offline tests | 24 passed |
| Held-spawn offline tests | 7 passed |
| Card-tag generator tests | 3 passed |
| Pinned Rust formatting | Pass |
| Full hosted Linux/Windows Rust/Python matrix | Pending |
| CUDA feature lint and host-safe tests | Pending |

Small offline tests above ran on the workstation's installed Python. Hosted CI uses the repository's locked Python/toolchain. Native builds and tests stay on hosted CI while Jack's PC has a protected throughput run and HaleysPC has another active build. No training, formal measurement, GPU test, model selection, promotion, trainer adoption or playing-strength claim is part of this work.
