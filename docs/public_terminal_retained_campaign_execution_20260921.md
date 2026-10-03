# Retained-teacher campaign: training complete, evaluation pending

All three frozen arms completed 32 updates from g115 Adam age 32,400 to 32,432. All 96 checkpoint files were recovered and verified. No new tactical or reserved retention validation was scored; no effectiveness, advancement or human-strength conclusion is available yet.

Implementation `b405f312` adds `retained_train` in `campaign/retained_campaign.rs`. It binds arm, semantic-label file, teacher data, retention data, scientific design and full optimizer continuation. The semantic targets are recomputed from actor-relative action records, not accepted blindly from the export. Correct-label beta 0 avoids retention scoring/backward during updates; beta 0.5 uses the validated full batch. The first update is checked against the matching teacher-only update, where parent KL is exactly zero.

Substantive native execution rejects a missing, stale or incompatible throughput receipt before loading training data. Its checks include executable/input/design identities and the allowed host, arm and worker count. The supported Python launcher additionally verifies pinned helpers, engineering results, selected timing evidence, output/recovery fingerprints and current ownership. These guards protect these launch paths; they are not operating-system enforcement over every legacy executable. The two-update engineering path is still bounded and must not be chained into a campaign.

## Placement and actual cost

Nine CPU placements were measured using identical two-update jobs for all three arms. Both PCs were inventoried, including CPUs, memory, GPUs and disks. Every CPU checkpoint was byte-identical across placements. The GPUs have no qualified retained-imitation CUDA implementation, so the old teacher-only CUDA path was not silently substituted. No GPU speed claim is made. A fresh read-only RunPod query at 19:45 UTC returned HTTP403; no paid allocation was authorized or created.

| Placement | Projected complete campaign |
| --- | ---: |
| Jack: one worker, serial arms | 62.95 s |
| Jack: four workers, serial arms | 54.69 s |
| Jack: one worker, three concurrent arms | 34.63 s |
| Jack: four workers, three concurrent arms | **30.13 s** |
| HaleysPC: one worker, serial arms | 160.90 s |
| HaleysPC: four workers, serial arms | 149.96 s |
| HaleysPC: one worker, three concurrent arms | 122.50 s |
| HaleysPC: four workers, three concurrent arms | 115.25 s |
| Both PCs: four workers, up to two arms per host | 106.40 s |

The selected allocation used Jack's D-drive Samsung SSD with three concurrent arms and four native backward workers each. Actual staging was 2.973 seconds, native execution 18.402 seconds, and recovery 9.641 seconds: **31.016 seconds total**, excluding separate ownership checks. Each learner preserves fixed reduction order and sequential updates. Once an arm finishes, there is no extra planned independent work to launch merely to raise utilization.

Qualification took 193.93 seconds in its completed root, including 3.16 seconds of remote setup. It cost more than the short formal campaign; preserve this evidence when compatible rather than repeating the grid without cause. Native build cost was 191.86 seconds with four BelowNormal Cargo jobs on E drive. All execution was BelowNormal. Final inventories found no active owned native jobs on either PC and preserved seven existing idle human sessions.

## Completion and integrity

| Arm | Updates | Final Adam age | Final state SHA-256 |
| --- | ---: | ---: | --- |
| Correct, beta 0 | 32 | 32,432 | `d2f87a3d7c3e9294789a8690385234bfe10468019a45243fdba2c5e3e7f5229a` |
| Correct, beta 0.5 | 32 | 32,432 | `c9c8968185f2bcf3b526266689aea091309e0f270cbc974d81f5bae961b385e2` |
| Semantic control, beta 0.5 | 32 | 32,432 | `7d28ebc2ac82b82de08075523b036f1f316a2f8dc4a610c73d31c248612e9129` |

The unretained endpoint exactly reproduces the previous correct-teacher endpoint: every parameter, first/second moment, optimizer age and gauge anchor matches. This is a numerical regression result, not a reuse or reversal of the previous failed evaluation gate.

Engineering verified exact resume and agreement with the prior full-batch retained result, and rejected an unqualified 32-update launch, corrupted semantic labels and a changed arm on resume. The first orchestration attempt stopped before qualification because a deliberately corrupted label file shared a filename with its command. The corrected helper uses a fresh root and reuses the already-passed native replay/guard checks with identical executable and input identities. Old outputs are preserved; no formal run was interrupted or repeated. A shell-quoting error while preparing that helper created no new helper or execution and was corrected via a script file.

Evidence:

- `E:/mtg-meta-recovery-20260921/retained-campaign-002/completion.json`: all three completed arms and recovered output references.
- `E:/mtg-meta-recovery-20260921/retained-campaign-compute-002/choice.json`: nine exact-replay placements, selected allocation, hardware/API inventories and engineering receipt.
- `E:/mtg-meta-recovery-20260921/retained-campaign-v2.py`: pinned supported launcher. Original attempt roots `retained-campaign-001` and `retained-campaign-compute-001` remain preserved.
- `E:/mtg-meta-recovery-20260921/retained-campaign-tools-001`: binary SHA `6cf463d6581509ba09bb58cd6f5e0430163e5c886a72d6d2ccbd26d2368f580f`.

Next, implement the final-only evaluator for these distinct checkpoint identities and apply the already-frozen comparison plan to the 48 tactical cases and 100 reserved retention games. Do not decode these checkpoints as the old teacher objective, change the arms/gates, select intermediate endpoints, or infer improvement from training loss. Independent Fable review remains missing under the recorded zero-read quota error until September 22 at 07:00 EDT. Execution proceeded under Jack's explicit authority with that review gap recorded. No CP7 outcome selection, model promotion or human-preview change occurred.
