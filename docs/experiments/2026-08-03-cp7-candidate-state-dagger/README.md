# CP7 candidate-state DAgger screen

## Question

Does CP7 supervision transfer when CP7 labels states visited by the retained candidate, rather than states visited by CP7 itself?

This is a narrow covariate-shift test. It does not claim that CP7 is professional strength or that imitation accuracy is playing strength.

## Fixed collection

- Behavior policy: exact retained outcome parent rooted at `D:\mtg-kernel-policy-only-structured-successor-v1\candidate\parent`.
- Environment: XMage Mono-Red Rally mirror against deterministic CP7 skill 7.
- Data: 256 previously consumed matched seat-swapped pairs starting at base seed 1,400,001.
- Label surface: candidate-controlled priority surface decisions only.
- Teacher query: one-shot CP7 plan on an internal XMage simulation copy, capped at five seconds.
- Training reward: natural terminal win, draw, or loss remains the only reward. CP7 choices are supervised labels only.
- Collection topology: eight independent JVM workers with one simulation thread each, subject to a bounded throughput screen.

Every label must join exactly to one candidate outcome row by episode, step, physical decision, substep, and model-input SHA-256. At least 95 percent of labels must be unambiguous and in range. The live candidate action and trajectory must not be changed by the teacher query.

## Preflight result

The one-pair seed 1,400,001 preflight produced 75 labels over 102 candidate outcome decisions. All 75 joined exactly, with 30 mapped CP7 actions, 45 CP7 passes, zero timeouts, zero ambiguous labels, and zero errors. Candidate and CP7 disagreed on 36 of 75 labels. The baseline and shadow-query outcome streams were byte-identical at SHA-256 `4ada2a0e7011f00f033f57892d2c3e6fc6c1824ec78b90841bcf91675e4eedff`.

An exact shadow replay reproduced every semantic label and candidate selection. Only elapsed-time diagnostic fields differed. Game time rose from 5.08 seconds to 6.98 seconds for the pair; JVM-inclusive wall time rose from 28.77 seconds to 30.56 seconds.

The bounded eight-worker screen completed 16 games in 72.41 seconds, or 0.221 games per second end-to-end. All 623 labels were usable and 288, or 46.2 percent, disagreed with the candidate. Sampled total CPU utilization was 77.5, 96.7, and 94.5 percent. Each JVM used about 0.7 to 1.3 GB of resident memory. Eight workers are selected as the practical topology knee. The conservative 256-pair wall-time estimate is 39 minutes before shard-size amortization.

## Learner and terminal gate

Fit only a zero-initialized structured policy residual. Keep the retained parent value fixed. Use whole-pair held-out folds and report CP7-label NLL/top-1 only as diagnostics. Candidate eligibility requires mean total variation at most 0.03, p90 total variation at most 0.10, maximum joint log-ratio at most 0.50, and improvement on held-out candidate-state labels.

The playing-strength gate uses fresh natural games only: 128 new matched seat-swapped CP7 pairs for candidate and retained parent. Promotion requires gains at least losses plus 8, each seat's candidate-minus-parent wins at least -4, and zero protocol or identity failures. If this fails, close priority-only DAgger and move to an action-conditioned counterfactual learner.
