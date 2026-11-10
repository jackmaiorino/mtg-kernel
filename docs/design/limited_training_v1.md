# Original-deck Limited training check

Issue #552 uses the authenticated #454 Kaito tree learner on exact DraftZero
registrations. The four engineering fixtures below are 40-card lists without
Drake Hatcher. The original Return-mode attempt at `08243cba` stopped in the
legacy Full-only deck parser before opening a game. The Return owner has
exact Alesha support, while the evaluation roster includes Arbiter of Woe
and Scrawling Crawler, whose exact numeric meanings are in the FullPool owner.
The opt-in `full-pool` learner resolves the unchanged rosters against the
frozen Foundations set and uses that profile's sealed V3 owner. The actor
can still refuse a decision during play.

| Stage | Original decks | Environment seed |
| --- | --- | --- |
| Train | `FDN_top_15157_BR` (Alesha), `FDN_top_09703_UB` | `552001` |
| Evaluate | `FDN_top_26941_WB` (Valkyrie's Call), `FDN_top_30836_UR` | `552002` |

`limited_training_inventory_v1.py` emits an 80-deck inventory with original
deck hashes, exact sizes, Full-only and typed-return registration checks and
Drake Hatcher membership. The 80 lists contain 76 with 40 cards, three with 41
and one with 49. The inventory reports registration eligibility separately
from native actor coverage. The evaluation roster is disjoint from training.

| Source split | Decks | Declared Full-only | Contains a Partial printing |
| --- | ---: | ---: | ---: |
| Train | 40 | 20 | 20 |
| Evaluation | 40 | 19 | 21 |

Every listed card name resolves in the declared registries. Decks outside the
four fixtures remain in the inventory and outside this bounded run.

The opt-in learner owner projects sorted name minima from the shared reveal
reducer. Opponent-hand copy rows and unresolved physical action bindings do
not enter the learner input. The fixture compares authenticated inputs, action
flags and logits for both seats after two identical public Swamp reveals. It
compares equal-size hands with one or two Swamps and also varies which native
copies the two reveals touch. The rules-only closed-list V3
search and typed learner use separate decision entry points.

The two train games start with opposite seats and sample from one immutable
tree model. The learner uses the first 32 non-forced decisions per seat per
game, retaining the full action-index trace. The trace width and fixed opponent's
uniform choice use the sealed numeric menu, which omits infeasible combat
answers. Every example gets that seat's
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

## Guarded qualification

1. Pin the exact composition, Rust, linker and host slot. Build with
   `--features limited-fdn-fixtures`. Run `closed_list_v3`, `issue552_` and
   `limited_return_consumer_v1::tests` on the same native test binary. Keep
   each filter's listed count and result, including failures.
2. Build `kernel_limited_match_v1`, `kernel_limited_return_env_v1` and
   `kernel_limited_training_v1` from that composition. Exercise the V3 match
   profile against the pinned disjoint belief pool in a bounded replay. The
   tests and replay preserve search refusals separately from sampled actions.
3. Run `kernel_limited_training_v1 -session full-pool OUT_DIR 1|2` in a fresh directory once
   the bounded training run is admitted. The checkpoint binds the exact
   decks, seeds, actor and reveal producer identities, feature contract,
   objective and complete Adam state. Evaluation reloads `training.json`.
   Replay the batch at the other worker count and compare `training.json`
   and `evaluation.json` byte for byte. Verify eval-only resume using
   `kernel_limited_training_v1 -session full-pool OUT_DIR 1|2 TRAINING_JSON SHA256`.
   Omitting the selector retains the historical Return CLI. Both modes
   require the same source and actor profile when resuming.
4. Save input and output hashes with the binary hash. Report natural
   outcomes, any refusal and whether deterministic replay matched. The
   bounded run covers those exact recipes. Active fair-search coverage of
   all 286 printings has its own acceptance work.

Native execution and exact replay are pending. The bounded binary does not
accept a custom roster, game count or additional updates. A substantial
training or evaluation launcher needs current resource inventory and matched
one-worker and parallel full-path qualification before it can dispatch work.
