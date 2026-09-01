# g896 strong-neural-pressure A/B v1

Status: revision 3 frozen before training. This is a bounded native development experiment, not a CP7 test. Revision 2 preserved all slot weights and replaced only two opponent identities because the production manifest contract requires every slot to have positive weight. Revision 3 records that the experimental manifest chain removes stale, commit-bound search-authority links; only active refresh 025 controls these training updates.

## Question

Generation 896 was the only cycle-3 checkpoint to pass both native accumulation gates. After generation 896, the learner's training-mixture win rate increased while fixed-opponent strength did not. Critic error, advantage magnitude, and policy entropy stayed stable. Test the narrow causal explanation that about 21 percent of the cycle-3 pool was occupied by weak neural exploiters and supplied insufficient pressure.

## Fixed source and arms

Both arms start from an independently reconstructed, fully validated copy of the exact generation-896 Store prefix in `E:\mtg-kernel-population-v2-cycle3\lineage\real-attempt-003\run-0\store`, including its optimizer state. Both run 128 updates of 64 complete natural-terminal episodes with the Store-bound base seed `977002`, the unchanged cycle-3 environment, learner-seat schedule, model, optimizer, `terminal_reinforce_value/v3` objective, and every non-pool hyperparameter.

Both arms use the same integer weights `[129340, 133940, 123504, 125965, 136138, 141419, 107799, 101895]`, totaling 1,000,000. Slots 0 through 4 are copied from refresh 025 and slot 5 is frozen generation 896. No kernel searcher, CP7, human data, heuristic target, shaped reward, or nonterminal reward is used.

- `CONTROL`: slots 6 and 7 are the weak non-search neural occupants from refresh 024, model SHA-256 values `6b42f88e...` and `10ae7b2f...`.
- `PRESSURE`: at the same slots and weights, replace only those occupants with cycle-3 generations 768 and 640, model SHA-256 values `0ef4d2db...` and `9e3bd33b...`.

Because the base seed and weight thresholds are identical, every episode selects the same slot in both arms. Only episodes assigned to slots 6 or 7 see a different opponent policy. The generation-768 and generation-640 checkpoints predate generation 896 and are never training targets.

Both experimental chains are rebuilt as all-neural from historical refresh 020 onward because archived search-authority records are valid only for the executable commit that created them. Those earlier links exist only to establish chain continuity for the production decoder. Training resolves active refresh 025, where CONTROL and PRESSURE differ only at slots 6 and 7 as declared above.

## Measurement and gates

Preflight is freely retryable. It must prove clean checkout, pinned Rust/Cargo/linker, exact g896 and opponent identities, identical integer weights and slot schedules, crash-consistent reconstructed Stores ending exactly at generation 896, and one excluded four-update continuation under the original refresh-025 chain that bit-matches the already published generation-900 Store state. Training and native evaluation use exclusive headless physical GPU 1 whenever the existing qualified CUDA path is selected.

After both 128-update Stores complete, acquire CONTROL, PRESSURE, and frozen g896 on common seat-swapped Rally roots against fixed promoted(2). The initial seed family begins at `4100000000`, uses 128-cluster chunks, and stops at the first valid decision or 4,096 clusters. The analysis uses the existing bounded-mean confidence-sequence reference with two predeclared comparisons:

1. PRESSURE minus CONTROL.
2. PRESSURE minus frozen g896.

The initial passes only if both comparisons have confidence-sequence lower endpoint above zero, paired terminal-order net at least `max(16, ceil(0.01 * N))`, and nonnegative net in each PRESSURE physical seat. Any numerical, identity, schedule, natural-terminal, Store, or GPU violation invalidates the run.

If the initial passes, repeat the same two comparisons once on the disjoint seed family beginning at `4200000000`. Both comparisons must pass the same gates again. No checkpoint, weight, horizon, seed, or threshold is selected from outcomes.

## Route

- Pass both looks: retain the PRESSURE descendant as the next native anchor and continue with a separately designed stronger-opponent construction step.
- Fail PRESSURE versus CONTROL: reject weak-opponent quality as the next lever and move to a representation-level experiment.
- Beat CONTROL but not g896: the pool change affects learning direction but does not create strength, so do not extend it.

CP7 remains untouched. This experiment cannot establish 60 percent versus CP7, broad MTG strength, BO3 strength, or a general claim about population learning.
