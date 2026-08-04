# CP7 candidate-state DAgger screen

Status: complete, rejected before held-out evaluation

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

The full collection completed 512 natural games in 584.9 seconds, or 0.875 games per second. It produced 14,525 usable labels, all unambiguous and in range. Candidate and CP7 disagreed on 6,917 labels, or 47.62 percent. Six games had no candidate-controlled priority choice and therefore correctly contributed zero labels; all six retained complete natural terminal records.

The historical complete-history cache did not contain these candidate states: its first keyed row had three legal actions and selected index one, while the new candidate-state row had two legal actions and selected index zero. Reuse was rejected before fitting. A synchronized 8-worker replay completed in 588.8 seconds and exactly reproduced all 14,525 shadow labels while also exporting 33,090 actual opponent CP7 decisions. The resulting exact cache covers all 512 episodes, 56,273 policy steps, and 46,063 physical decisions at SHA-256 `98babc28617a57d3053bf178ba1d1084f943339f69d75b918402f2e4dd10d1df`.

## Learner and terminal gate

Fit only a structured policy residual initialized from the retained policy-only successor and keep the retained parent value fixed. The split is fixed by pair index: residues 1 and 2 modulo 4 are training, residue 3 selects the residual clip, and residue 0 is touched once for the held-out gate. The held-out gate requires at least 5 percent overall relative NLL improvement, at least 3 percentage points of top-1 improvement, and no per-seat NLL regression. Candidate eligibility also requires mean total variation at most 0.03, p90 total variation at most 0.10, and maximum joint log-ratio at most 0.50 for each seat and overall. CP7-label metrics are eligibility diagnostics only. If eligible, refit the selected configuration on all 256 pairs before transport qualification.

The playing-strength gate uses fresh natural games only: 128 new matched seat-swapped CP7 pairs for candidate and retained parent. Promotion requires gains at least losses plus 8, each seat's candidate-minus-parent wins at least -4, and zero protocol or identity failures. If this fails, close priority-only DAgger and move to an action-conditioned counterfactual learner.

## Result

The exact-history fit completed in 209.01 seconds. It used 6,934 training decisions and 3,698 selection decisions. The 3,893 held-out decisions were not touched because no clip passed the selection movement gate.

At the smallest clip, `0.03`, overall CP7-label NLL improved by `0.7427%` and top-1 accuracy improved by `0.0539` percentage points. Mean TV was `0.0102640` and p90 TV was `0.0271627`, but maximum physical-decision joint log-ratio was `0.547092`, above the fixed `0.50` limit. Larger clips increased NLL improvement but remained unsafe; even at clip `0.40`, selection top-1 improved by only `0.3773` percentage points.

The screen therefore rejected priority-only candidate-state DAgger. No held-out result, full-data refit, candidate package, transport qualification, or natural-terminal strength gate was produced. This is a negative result for the exact CP7 selected-index objective and movement envelope, not evidence that candidate-state supervision or action-conditioned learning is generally ineffective.

Fit report: `D:\mtg-kernel-cp7-dagger-residual-v2\fit.json`, SHA-256 `bca3391a2f1c744fc66c126003ed6327851866b3b3f2542e503902ce0acf52f2`.
