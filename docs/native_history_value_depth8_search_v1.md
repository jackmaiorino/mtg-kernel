# Native history-value depth-8 search v1

Status: predeclared, not run

## Question

Does short-horizon information-set lookahead improve the complete-history candidate where its one-step value selector did not?

## Fixed inputs

- Candidate and parent: the exact packages from `native_history_value_search_live_v3`.
- Runtime opponent: XMage CP7 skill 7.
- Reward and promotion measure: terminal win or loss only.
- Eligibility: candidate-controlled surface decisions at physical-decision ordinal 20 or later, with one substep and 2 through 8 legal actions.
- Hidden-state treatment: four deterministic current-actor information-set redeterminizations shared across every legal root action.
- Override rule: retain the seeded policy action unless another action's mean candidate-seat value is at least 0.25 higher.
- Formal topology: headless GPU 1, four matched pairs concurrently per batch.

## Single changed mechanism

For every eligible root action and redeterminization, apply the root action, then take at most eight additional policy decisions. Both seats use deterministic model-logit argmax with lower-index tie breaking. A terminal branch receives its exact candidate-seat terminal reward. A nonterminal branch after eight continuation decisions receives the complete-history model value at that state, signed to the candidate seat.

The prior live screen bootstrapped immediately after the root action. This screen changes only that continuation depth. It does not increase the hidden-state sample count or tune the eligibility window or margin.

## Preflight

Build and test the scorer and harness. Run one nonfresh matched pair at base seed 950001. Require:

- both arms complete;
- at least one eligible root for each candidate seat;
- exactly four distinct sampled privileged hashes at every eligible root;
- at least one override overall;
- practical projected formal wall time from measured throughput.

Preflight is freely retryable for implementation or environmental defects.

## Fresh formal gate

- Base seed: 1400001.
- Target: 8 mutually successful matched pairs, 16 games.
- Maximum: 32 attempted pairs, with outcome-blind exclusion of pairs where either arm fails.
- Batch size: 4 pairs.
- Pair result: gain if search wins and parent loses, loss if parent wins and search loses, tie otherwise.

Pass only if all are true:

- gains are at least losses plus 2;
- candidate-seat P0 net is at least -1;
- candidate-seat P1 net is at least -1;
- search overrides at least one eligible root from each candidate seat;
- every accepted diagnostic has four distinct information-set samples.
- every accepted diagnostic reports exactly eight continuation decisions.

On pass, run a fresh 16-pair extension at base seed 1410001 before promotion. On fail, retire this exact depth-8 selector. A failure does not reject other search depths, learned search policies, or better belief models.
