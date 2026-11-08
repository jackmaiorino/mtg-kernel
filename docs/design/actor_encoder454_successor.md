# Actor encoder successor for Limited and Eternal

Issue #454 remains open. This work preserves the frozen FlatV4 actor, FlatV7/V8
wrappers and their feature/model identities. Engineering acceptance is separate
from the training and fixed-opponent evaluation required by #552.

## Reuse and limits

The historical big-network branches `claude/bignet-tensor-dump-20261031`
(`c40d3e7a6`) and `claude/bignet-tensor-dump-a0ea-20261031` (`5463da64e`)
record and replay scored V4 tensors. Neither supplies the successor actor.

The existing `current_mixed_policy_v1` is a useful distinct consumer:

- `kaito.rs` obtains complete validated V75 candidate/observation packets from
  the sealed session, preserves original action order, and authenticates apply.
- `normalize.rs` expands supported typed printed meanings, renames visible
  references, removes digest metadata and preserves Boolean operands.
- `numeric.rs` losslessly represents the normalized state and actions as
  numeric trees. `network.rs` scores every candidate using a small recurrent
  tree model, independently of the FlatV4/Net8 checkpoint loaders.

This is not yet the production successor. V75 reset is Foundations-only and
Full-only. Its original numeric model has forward inference but no learner,
optimizer snapshot or production collection interface. The printed-meaning registry and
public transport must cover Eternal before accepting that catalog. Removing
the catalog gate or treating unsupported roots as card IDs would discard
requirements of this consumer.

## Implementation stages and acceptance

1. Preserve the original 40-card multisets of `FDN_top_08530_WR` and
   `FDN_top_26287_RG` in deterministic initial-scene fixtures. With active
   Krenko, encode and score all legal actions through the sealed numeric owner;
   require the frozen observation's specific Krenko refusal. Exercise both
   legal include values for attacker and blocker decisions from either seat,
   require distinct numeric rows, and compare selected steps to native steps.
   These constructed scenes do not claim natural-game reachability or results.
2. Bind the production successor to six named action flags (`pay`,
   `change_target`, `use_cost`, `cast_it`, `value`, `include`) and explicit
   public state/action tensors with no digest input. Add a distinct learner
   contract and actual sealed-packet ingestion, including complete parameter
   and optimizer persistence. Existing frozen checkpoints remain incompatible.
   Validate gradients, deterministic continuation and malformed-input refusal
   before any run; choose architecture with the coordinating research owner.
3. Extend the owning public transport and typed printing coverage to Eternal,
   retaining player-visible reference checks. Run the reported Legacy
   Reanimator, Legacy Doomsday and Vintage Red Prison fixtures and the original
   pool coverage probe through this successor. Report configuration, session,
   observation, action and tensor failures separately with exact decision
   witnesses. Acceptance requires every declared fixture, not only successful
   episodes or an aggregate count of skipped games.

The prospective `current_tree_learner_v1` implements stage 2's learner core:
647 f64 parameters, 20 input limbs per tree node, 16 tanh units, both parent
and preceding-node recurrence, mean pooling, state/action/interaction readout,
and six additional named Boolean coefficients. It backpropagates through all
tree nodes and both recurrent edges. An update minimizes mean cross entropy
against supplied normalized target distributions using synchronous Adam in
fixed example order. This defines a new engineering contract, not a change
to the campaign's objective, sampling, optimizer or architecture selection.

Only the current sealed Kaito owner can expose its public training input;
stale and foreign decisions refuse. A capture contains no executable authority.
Scoring uses the same captured input and applies through the existing owner.
Snapshots retain exact f64 parameter/moment bits, optimizer configuration,
step and feature identity. Invalid inputs and nonfinite updates are atomic
failures. Durable publication and production collection remain caller work.

Native validation is pending. No experiment, playing-strength claim or
production admission follows from these source tests. The remaining Eternal
transport/coverage and production collection work keep #454 and #552 open.

## Eternal dynamic supplement

`eternal_actor_public_v1` is an additive partial projection, not a model
admission permit. It selects only public energy/city-blessing counters,
Cavern's chosen subtype, Needle's publicly named printing, current stack
uncounterability and mana spent, and Saga gained chapters. The engine writes
the chosen-name/type ledgers at publicly observable entry choices. A named
printing is a public choice, never evidence of a physical hidden card.

References require the same incarnation in the relevant public zone before
the public reference producer runs. Departed or recycled objects are omitted
before reading their current definition. A current face-down source refuses
until correspondence continuity is validated separately. A retained choice
after a face-up transform records a public historical choice, not activation
of an old printed ability. The remaining Eternal delayed effects, selections,
static meanings and public continuations must be represented before complete
consumer admission; serializing the entire Eternal state is forbidden because
it includes unrevealed private choices.
