# Terminal imitation with parent-policy retention: CPU engineering check

Implementation `e4e0d965` adds a separate CPU training method for terminal teaching plus parent-policy retention. Fourteen focused tests pass. No training campaign, candidate promotion or playing-strength measurement was performed.

The previous 32-update teacher fitted its training labels but failed its frozen evaluation gate. A subsequent saved-input audit found changed top actions on 2,991 of 15,791 natural development choices. That establishes broad policy movement, not worse play or the cause of the failed gate. Retention is a testable proposed constraint, not a demonstrated remedy.

## Objective and implementation

`native_policy_train_step_v1/retention_v1.rs` computes mean terminal-teacher cross entropy plus beta times mean `KL(parent || current)`. Each term uses its own physical-decision denominator; rows within a physical decision are summed. Retention rows contain visible V4 tensors, frozen parent logits and current-score replay bits. They do not invent rewards, value targets or selected actions. The existing `native_policy_anchor_v1` supplies legal-action KL and logit derivatives.

Both derivatives are evaluated at the original parameters and combined before one Adam update from the original moments and age. Four fixed logical partitions preserve reduction order across one, two and four execution workers. Current logits and value must replay exactly before updating. Validation failures leave the original state unchanged. Beta zero delegates directly to the existing terminal-imitation method and ignores retention inputs.

The initial implementation obtains teacher gradients through a disposable cloned teacher update. Its extra Adam calculation is overhead, not a second published update. No end-to-end throughput qualification has been performed for this new method. There is no new CUDA retention path or supported campaign launcher yet.

## Verification

Evidence: `E:/mtg-meta-recovery-20260921/retained-imitation-engineering-001`.

| Check | Result |
| --- | --- |
| New retention tests | 3 passed |
| Existing legal-action KL tests | 8 passed |
| Existing GAE regression tests | 3 passed |

The new fixture uses the actual V4 encoder and tensorizer for a Map choice. Synthetic parent-logit offsets deliberately create nonzero KL. An independent f64 loss calculation checks separate normalization using two retention groups containing two and one rows. Central differences check nontrivial parameter gradients. One, two and four workers produce identical returned results and full optimizer snapshots. Zero beta exactly matches the legacy update, including warm moments. Invalid beta, worker count, empty positive-beta retention and a one-bit current-logit mismatch are rejected without state mutation.

Compilation took 286.86 seconds; compilation and test processes together took 288.92 seconds. Four BelowNormal Cargo jobs used E-drive target and temporary storage. This was a bounded correctness check, not a substantial training run. Test executable SHA-256: `968a5a61c3674ac1bad76bc1f8ec58cd5d8de1abe12a1ea3cca58ccb3d5f8103`.

## Remaining work and limits

The subsequent [real-checkpoint integration check](public_terminal_retained_imitation_integration_20260921.md) verifies two g115 updates and fresh-process resume on a small development fixture. Its f32 softmax KL is not the production Hamilton sampling distribution. Zero value-loss coefficient does not freeze value predictions or existing optimizer momentum. Neither check establishes hidden-state invariance, broad natural-policy retention or improved play.

Next, select development retention inputs by declared deck, seat and decision-kind coverage, without outcomes, and keep certified teaching positions out of blind parent imitation. Before any substantive comparison, define fresh validation, a semantic permutation control, frozen analysis gates and measured allocation under `C:/Users/user/COMPUTE-POLICY.md`. Do not tune on the consumed 24-position evaluation or reuse the flawed raw-index control. No beta or campaign schedule is selected by this engineering check.

Independent Fable review remains missing: the recorded consultation failed HTTP429 with zero source reads and a September 22, 07:00 EDT reset. No repeated retry or endorsement is implied. Reversible engineering continued under the maintainer's explicit research/execution assignment; objective validity and experimental design retain that review gap. CP7 outcomes remain excluded. The g115 human preview and Kimi's separate Escape branch remain unchanged.
