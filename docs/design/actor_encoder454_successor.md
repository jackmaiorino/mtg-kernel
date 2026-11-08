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
Full-only. Its numeric model has forward inference but no learner, optimizer
snapshot or production collection interface. The printed-meaning registry and
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

Only stage 1 is implemented in the initial coverage patch. Native validation
is pending; no training, evaluation, playing-strength claim or production
admission follows from the source tests. A successful coverage-only PR does
not close #454 or #552.
