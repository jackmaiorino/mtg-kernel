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

Native Limited validation passed all six `issue454_` tests on the isolated
composition `384c8a4e5103f40384928adb69720268e4ecd8cb`, including all 647
finite-difference gradients and exact optimizer continuation. This composition
contains the learner and coverage changes through `b5cc1e4b2`, plus the owned
root #576/#583 integration changes and the early public supplement. The
release test binary SHA-256 is
`9dd77e1e512809cd9401faa39b4ac085272b5f4cb24beedfbb3864d3d4c7f9e3`.
Rust 1.94.1, MSVC linker 14.50.35717 and Python 3.13.14 were used through the
four-core BelowNormal host-slot guard. Exact-head CI remains required.

The same Limited-feature binary passed nine widening tests, the owned Drake
enclosing replay witness and the Banner pending-original witness. It failed
the #576 one-step screen-fit fixture's known heldout-frequency configuration,
and two existing V7/V8 collection tests refused `Action(CorruptCurrentBinding)`
before their new assertions. Those are retained Limited-feature compatibility
failures, not failures of the separate default-feature Linux checks. Ignored
measurement and slow Drake diagnostics were not run. Durable local receipts
are under `D:/mtg-kernel-actor454-target/receipts/`; no bulk artifacts are
committed. No experiment, playing-strength claim or production admission
follows from this correctness validation. Eternal validation and complete
transport/coverage still keep #454 and #552 open.

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

The supplement additionally carries live Prepare copy and Animate Dead links.
Departed linked sources refuse until their historical public identity and
delayed lifecycle are represented. The first-spell fact is derived from public
cast events rather than the E8a ledger, whose updates depend on hidden card
presence. Tests cover actual Cavern/Needle handlers, concealed-source refusal,
and hidden-card variants of the public cast fact.

`eternal_actor_printed_v1` extracts the generated CardDef, ordered costs,
programs, activated abilities, and triggers for the 62-name union of the exact
legacy-reanimator, legacy-doomsday, and vintage-red-prison mainboards, plus
their generated Karn Construct Token and Enroot printings. Its exhaustive
CardDef destructuring rejects unrepresented fields. Finite external static
and replacement rules are explicit descriptors, including chosen-name/type
effects, restricted mana, tax effects and Saga abilities.

`eternal_actor_choice_v1` binds the finite chooser-only selection operands,
without saved continuations or another player's saved answer. Retargeting
uses the public stack position instead of a native stack ID. Ordinary public
observation knowledge remains responsible for already revealed Show and Tell
choices. `eternal_actor_stack_v1` binds exact printed spell, activation and
trigger programs, including normalized captured-counter targets. Entry menu
indices have explicit subtype, named-printing or life-payment meanings.

These components are not an admission gate. Complete resolving/pending
program binding, canonical public-reference integration and sealed Eternal
scoring remain work in progress. All 64 printing extractions and the new
Eternal source/choice/stack tests still require native Eternal validation.
