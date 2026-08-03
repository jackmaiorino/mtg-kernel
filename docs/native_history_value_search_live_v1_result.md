# Native history-value search live v1 result

The fixed information-set search arm completed all 16 games at base seed
`1200001`, scoring 6-10 against XMage CP7. It evaluated 840 eligible roots,
made 206 overrides, exercised both candidate seats, and had zero four-sample
distinctness violations.

The retained-parent arm did not complete. Episode 0 finished, then episode 1
failed deterministically at `T15.M1` because the XMage bridge observed a CP7
priority action while Rust still exposed a candidate-controlled decision. A
whole-arm restart failed at the identical episode and mapper guard.

The v1 paired gate is therefore unadjudicated. The 6-10 search score is not a
paired strength result, the extension is unauthorized, and the fixed search
rule is neither accepted nor rejected by v1.

The attempted broad bridge repair was abandoned with no source changes after
it altered unrelated early-game cursor behavior. The rapid successor screen
uses a fresh seed block and predeclares deterministic mapper exclusions before
outcomes are parsed.
