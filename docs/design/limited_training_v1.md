# Original-deck Limited training check

Issue #552 uses the authenticated #454 Kaito tree learner on exact DraftZero
registrations. The four engineering fixtures below are 40-card lists. They
have only Full definitions, have named typed printing recipes in source, and
do not contain Drake Hatcher. The actor can still refuse a decision during
play. The roster and seeds are fixed before native execution.

| Stage | Original decks | Environment seed |
| --- | --- | --- |
| Train | `FDN_top_09703_UB`, `FDN_top_22247_BR` | `552001` |
| Evaluate | `FDN_top_25562_WG`, `FDN_top_30836_UR` | `552002` |

`limited_training_inventory_v1.py` emits an 80-deck inventory with original
deck hashes, exact sizes, Full-only registration checks and Drake Hatcher
membership. The 80 lists contain 76 with 40 cards, three with 41 and one with
49. The inventory reports registration eligibility separately from native
actor coverage. The evaluation roster is disjoint from training.

| Source split | Decks | Declared Full-only | Contains a Partial printing |
| --- | ---: | ---: | ---: |
| Train | 40 | 20 | 20 |
| Evaluation | 40 | 19 | 21 |

Every listed card name resolves in the declared registries. Decks outside the
four fixtures remain in the inventory and outside this bounded run.

The two train games start with opposite seats and sample from one immutable
tree model. The learner uses the first 32 non-forced decisions per seat per
game, retaining the full action-index trace. Every example gets that seat's
natural terminal return of +1, 0 or -1. One synchronous Adam update minimizes
mean `-return * log p(action)`. This is terminal REINFORCE at gamma 1 with a
zero critic, a bounded form of the Pauper terminal-return objective. Both
games must finish naturally and the update must change a parameter. No
frozen campaign objective or checkpoint is modified.

The two learned-policy seats face the same uniform legal-action opponent on
the original evaluation pair. The opening player and environment seed are
fixed across legs and snapshots. The initial and trained snapshots each
play both seats. Natural wins, draws and losses, action traces and any stage
failure are saved for both snapshots. This is an engineering evaluation of
one update, with no playing-strength threshold.

Build `kernel_limited_training_v1` with `--features limited-fdn-fixtures` and
the repository's pinned toolchain. Under the coordinator's bounded native
slot, run `kernel_limited_training_v1 OUT_DIR 1|2`. The output directory must
be fresh. The checkpoint binds the exact decks, seeds, model feature identity,
objective, Adam configuration and all optimizer bits. Evaluation reloads the
atomic `training.json` file before either snapshot plays. A second process can
replay the evaluation from that file with `kernel_limited_training_v1 OUT_DIR
1|2 TRAINING_JSON SHA256`. Compare `evaluation.json` byte for byte. A replay
of the whole batch at the other worker count also compares `training.json`.
Keep the outer run manifest's exact commit, binary hash, Rust and linker
versions, host slot and both output hashes alongside these files.

Native execution and exact replay are pending. The bounded binary does not
accept a custom roster, game count or additional updates. A substantial
training or evaluation launcher needs current resource inventory and matched
one-worker and parallel full-path qualification before it can dispatch work.
