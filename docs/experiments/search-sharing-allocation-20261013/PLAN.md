# Search sharing and allocation experiment

Jack assigned Codex leadership, implementation, execution and monitoring of this experiment on 2026-10-13. Claude retains the separate distillation campaign. Base: main `86e661d2f`, including PR241 interchangeable choices and PR251 key memo. Existing runs and frozen evidence remain intact.

**Question.** Can persistent Stage 4a search produce better continuations for the same CPU cost by sharing equivalent positions or concentrating root exploration? The Spy recording motivates this question; similar pictures and losing rollouts do not establish wasted computation.

| Arm | Treatment |
| --- | --- |
| A | Current UCT with explicit common baseline flags |
| B | A plus conservative continuation-evidence sharing |
| C | Root successive halving after eight actual backups per edge, preserving ties |
| D, conditional | Combine B and C only if the individual evidence warrants it; use fresh seeds |

Keep checkpoints, lawful sampling, rollout policies, terminal reward, depth, refresh scheduling and execution support common. Default behavior stays unchanged pending results.

**Sharing.** Begin with permutations of uninterrupted focal basic-land mana activations, preserving exact pre-run information and equivalent final decision context. Establish engine commutation, including triggers, replacements, pending targets, policy context and remaining depth. Report opportunities after existing copy-orbit merging. Keep incoming histories and actual execution-support counts separate; pool each fresh terminal sample at most once per shared edge. Reused evidence is not a new independent win. Bound memory and reject cycles. Check applicability before a full panel; a restrictive zero-hit result does not refute general transposition sharing.

**Panel.** Reuse the nine outcome-blind roots in collab `LANES/search-efficiency-20261013/evidence/inputs`, with three fresh matched seed repetitions. Keep the published Spy case as a separate mechanism diagnostic. Pin inputs, SHA-256s, flags, toolchain/linker, binary, seed namespaces and GPU ordinal (`none` for CPU work) in one manifest before measurement. Calibrate equal CPU budgets using separate seeds, anchored to baseline 16k and 64k transitions. Rotate arm order and match affinity. Record full process costs, including loading, reconstruction, keys, lookup, rollouts and output; bound and report any final-rollout overshoot. Replay one recorded work limit to a bit-identical deterministic primary output hash.

**Quality.** Evaluate on 128 fresh matched lawful worlds per root/arm/repetition. Execute the resulting frozen search continuation with the existing eight-backup rule and ordinary-policy fallback; no further search or updates. Force C's selected active root edge. Identical first actions do not justify deduplicating different trees. Single-action/plain-policy evaluation is optional secondary evidence because it can erase multi-step discoveries. Retain failures, unknown outcomes and unsupported roots in reporting.

**Analysis.** Primary contrasts are B-A and C-A at the 64k-anchored CPU budget: paired integer win differences and simultaneous 97.5% root-cluster bootstrap intervals. The 16k comparison is secondary. Report actual independent samples, support, stability, sharing opportunities/hits, memory and net cost. Advance on a positive lower interval at equal CPU, or exact output/decision parity with a measured engineering speedup. Otherwise conclude negative or inconclusive. This small development screen establishes neither ordinary-start strength nor learned-policy improvement.

**Resources and ownership.** Initial cap: 12 aggregate process CPU-hours for search/evaluation, calibration and retries; 4 GiB run output; 16 GiB build scratch; preserve 60 GiB volume reserves. Jack permits requesting RunPod allocations; prepare a concrete lease and spending cap before any paid launch. Check Jack, Haley and RunPod; qualify serial/parallel completed-work throughput and use the supported guarded launcher. Sol6.1 xhigh owns sharing; Sol6.1 high owns allocation/evaluation; Sol6.1 high owns placement/dispatch/analysis. The lead reviews interfaces and results, controls launches and one ledger/state record, and monitors existing handles. Preserve failed attempts, pinned binaries and cold evidence under ARTIFACT-LAW. Complete affected checks, the measured result and recommendation, substantive current-diff review, merged PRs and default-branch verification. A budget stop alone is not completion.
