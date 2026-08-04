# Native recurrent value external check v1

## Question

Does the only recurrent result that replicated, terminal-value prediction, improve
on a disjoint corpus beyond the already confirmed width-48 bounded value model?

This is a value-only decision. Natural terminal win, draw, or loss is the only
target. No policy output, search result, or playing-strength claim is produced.

## Fixed check

- Candidate: equal prediction average of the four exact trust-projected recurrent
  v2 fold states. The policy heads are ignored.
- Corpus: retained 1,024-pair bounded-value confirmation cache SHA-256
  `44eae5bee2b5556faa6293c80a88cb8f67f90d46066ffb5115ced2daac579800`.
  It is disjoint from the v2 training corpus but is reused, not fresh.
- Target and weighting: first row of each complete physical decision, terminal
  candidate reward, equal episode mass, reported overall and by candidate seat.
- Fixed benchmark: confirmed width-48 bounded value MSE `0.4428455914` overall,
  `0.4581749886` at P0, and `0.4275161941` at P1.
- Execution: CUDA GPU 1, batch 256. Repeat the complete evaluation in one process
  and require bit-identical metric JSON.

Pass only if the recurrent ensemble improves at least 5 percent over the width-48
overall MSE, does not regress against width-48 at either seat, every prediction is
finite and in `[-1,1]`, and the repeat is exact. A pass authorizes a value-only
native port and separately frozen search screen. A failure closes the recurrent
branch. It does not authorize policy training, promotion, or a professional-level
claim.
