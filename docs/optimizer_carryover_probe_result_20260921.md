# One-step carryover score probe: complete

The actual first teaching update moves the policy more than a zero-new-gradient Adam step on these fixed inputs. On the303 retention rows, mean production-Q64 total variation from g115 is0.00740935 for the saved actual update versus0.00074092 for momentum alone. The roughly tenfold difference argues against treating inherited momentum as the main explanation of this first step's average policy movement. It does not attribute the128-update BO3 regression or establish that optimizer resetting would help.

| Fixed input set | Model compared with g115 | Row mean TV | Equal-group mean TV | Maximum row TV | Changed argmax |
| --- | --- | ---: | ---: | ---: | ---: |
| 32 teaching rows /32 groups | Momentum only | 0.00129929 | 0.00129929 | 0.00286357 | 0/32 |
| 32 teaching rows /32 groups | Actual first update | 0.00902745 | 0.00902745 | 0.03220675 | 6/32 |
| 303 retention rows /256 groups | Momentum only | 0.00074092 | 0.00077789 | 0.02250857 | 2/303 |
| 303 retention rows /256 groups | Actual first update | 0.00740935 | 0.00657916 | 0.13979808 | 6/303 |

TV compares exact integer production action masses with a2^64 denominator. Equal-group means average within each physical group before averaging groups. The mean TV sums per retention group are0.00087695 and0.00876966 respectively; these sums are not TV of a joint policy. Different argmax choices are not error labels. The mean-distance ratio is not a percentage of the training effect explained by either mechanism, since policy changes are not additive.

Implementation f996fc38 adds `carryover_probe` to public_terminal_teacher_v1. It restores the full original V4 g115 state and the saved actual first retained engineering update, then constructs exactly one in-memory zero-gradient successor using production Adam at the existing learning rate0.0001. Original age32400 becomes32401 in the counterfactual; moments and canonical scorer bias follow the existing optimizer. The original parent remains unchanged. The actual first retained update has zero initial parent KL and was previously verified identical to the teacher-only step. This probe does not fit new labels, sample actions, run matches, reset an optimizer or export a playable checkpoint. Its command has no resume/end-update/checkpoint-output options.

All335 consumed input rows were scored under all three models. Verification completed:

- All99 tensor hashes, covering every parameter and both moment arrays, exactly match the independent float32 arithmetic audit for the native counterfactual.
- Parent and actual-first-update logits/value bits exactly replay64 archived teaching scores; parent logits/value bits replay303 archived retention rows.
- A fresh native process reproduces the full diagnostic result byte-for-byte. Every action mass vector is nonnegative, has the correct menu width and sums exactly to2^64.
- The actual native command rejects wrong checkpoint age, wrong ancestry, heldout teaching rows, changed input pins and a forbidden resume field before result publication.
- Two native unit tests verify zero-moment parameter identity and warm-moment movement, deterministic repetition, one-step age advancement and original-state immutability.

The unit-test command completed in300.9208s, mostly compilation; the two tests themselves took0.09s. The clean executable build took174.0031s with four BelowNormal Cargo jobs and E-drive target/temp storage. Executable SHA25630121e26c373a2384305f1320917286998cf0ae609f3ab1df2a9fcc00b23dea6. The first complete diagnostic took less than3.97694s, using the following native process's start as a conservative upper bound. Its exact wall time was not durably recorded before the controller interruption and is not reconstructed as a precise timing.

The local resource guard stopped two wrappers when another owner's Cargo tests appeared. The first stop followed the positive comparison/replay and age/ancestry rejection checks; the second followed the heldout/pin checks. All completed files were preserved, and no successful native check was rerun. The remaining schema-only rejection ran on freshly verified idle Haley using the identical27,030,528-byte executable and malformed request. It required no model transfer: deserialization rejects the unknown field before dispatch or source loading. The native rejection took0.24750s; inventory, staging, launch and recovery totaled13.8420s. No paid compute, new training campaign or frozen measurement was used or altered. These are bounded correctness checks, not a qualification for substantial scoring or training.

Final evidence is E:/mtg-meta-recovery-20260921/carryover-probe-complete-001/completion.json and analysis.json. The finalizer rechecked the complete oracle, all prior score replays, request identities and five rejection cases across the preserved roots. The original two local roots remain visibly incomplete controller runs; the final receipt explicitly identifies their recovered checks and the separate remote schema test. Positive native result SHA256 and all input/build/script pins are in that receipt. Per-row TV and argmax changes remain available without outcome-based filtering.

The next learning investigation should address interference from narrow teaching across shared parameters and the coverage of retention inputs, while keeping optimizer ancestry intact. This result supplies no basis to reset Adam, increase training duration, tune retention beta on the consumed outcomes, or claim that narrow teaching generally fails. Any new mechanism needs a separate bounded design with matched controls and fresh strength evaluation. A targeted intervention should first demonstrate where it preserves the existing policy before spending on whole-match tests.

Fable's independent review remains missing under the known zero-read quota failure until September22 at07:00EDT. Engineering proceeded under Jack's authority, with no retry or endorsement claimed. The completed negative result remains103/256 versus123/256 BO3 wins and no advancement. g115 and its repaired human preview remain the baseline. Human and league competitiveness remain unproven; CP7 remains excluded.
