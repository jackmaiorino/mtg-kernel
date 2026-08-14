# Player-visible competitive auxiliary heads v1

## Finding

The adapter already owns the exact League and Challenge event session across
pregame and sideboarding. It also exposes coordinate-free checked scoring
contracts for Keep, Mulligan, London bottoming, and a bounded sequential
sideboard deliberation.
Those contracts are not checkpoint implementations. The loaded native
deployment has no pregame head, no sideboard head, and no opaque model-owned
return that can resume the retained event session.

The existing auxiliary payloads are safe but incomplete for competitive
strategy. They include our visible hand or deck and the public match score, but
they omit the opponent cards and actions legitimately observed in completed
games. Games two and three therefore need one shared player-visible match
history input. A commitment alone is insufficient because the model must be
able to consume the visible facts it binds.

## Shared kernel-owned history

`mtg-kernel` must define a dependency-free
`ExternalPlayerVisibleCompletedGameV1` equivalent to the adapter's existing
public-history visitor. One value contains:

- the completed game number and player-relative winner;
- an ordered stream of confirmed player-visible decisions, each containing
  only a sanitized visible state and selected visible action;
- a separate ordered stream of rendered Game Log semantic events containing
  only player-relative actors, public counts, turn number, event kind, and
  visible card names.

The two streams have independent clocks. The kernel must preserve order within
each source and must not invent a total order between them. A bounded
`ExternalPlayerVisibleCompletedMatchHistoryV1` contains games in game-number
order, at most games one and two, with no gaps or duplicates.

This history excludes raw Game Log text or markup, paths, pixels, coordinates,
capture and classifier lineage, event and match identifiers, account data,
card-database IDs, stable kernel or MTGO object IDs, opponent hidden zones,
future draws, RNG, client protocol fields, and inferred opponent deck lists.
The model may infer strategy from the retained visible facts.

The adapter needs one move-only match-history accumulator. It imports each
completed `OpaqueMtgoCompetitivePlayerVisibleGameMemoryV1` through
`MtgoCompetitiveExternalPublicHistoryConsumerV1`, records the visible winner,
and retains the source lineage privately. The same accumulator lends an exact
kernel-owned history snapshot to game-two or game-three pregame scoring and to
the intervening sideboard decision. It remains inside the event operator until
the visible match ends.

## Pregame head

The kernel-owned pregame input is the exact semantic equivalent of
`MtgoCompetitiveNativePregameModelInputV1`, plus the completed-match history:

- game number, play or draw, and public match score;
- player-known current mainboard and sideboard as visible name/count
  multisets;
- the seven ordered visible cards and their decision-local slots;
- Mulligan or London-bottoming stage and counts;
- confirmed bottom selections in visible order;
- the exact ordered legal actions.

Game one requires empty completed-match history. Games two and three require
all earlier completed games. The scorer returns one finite logit per exact
ordered action plus a finite value. Equal logits select the earliest action.
Unknown or ambiguous visible card names fail closed. The output carries the
exact input, checkpoint, auxiliary-package, and deployment commitments, but no
adapter request, event session, coordinate, or input capability.

Suggested kernel surface:

```rust
pub struct NativeCompetitivePregameScorerV1<'a> { /* private */ }

impl NativeCompetitivePregameScorerV1<'_> {
    pub fn score_v1(
        &mut self,
        input: &ExternalPlayerVisiblePregameDecisionV1,
    ) -> Result<NativeCompetitivePregameScoreV1, NativeCompetitiveAuxErrorV1>;
}
```

## Sideboard head

The kernel-owned sideboard input contains the exact player-known current
configuration, next game number, public match score, and complete earlier-game
public history. The adapter drives a pure local bounded deliberation through
the exact retained checkpoint scorer:

1. move one named card from sideboard to mainboard;
2. move one named card from mainboard to sideboard;
3. submit the current candidate configuration when legal.

Every local step exposes a canonical ordered legal-action vector. Card order is
the frozen exact visible-name order. The native scorer returns one finite logit
per ordered action plus a finite value and binds the exact decision and loaded
deployment. The adapter selects the first maximum, applies the selected action
only to the local candidate, and emits no client input. The deliberation stops
after at most 64 decisions; failure to submit returns an abstention. Submitting
the initial configuration is the explicit unchanged decision. Only explicit
Submit creates an opaque checked final target. The adapter independently
validates inventory conservation and legal partition sizes.

Suggested kernel surface:

```rust
pub struct NativeCompetitiveSideboardScorerV1<'a> { /* private */ }

impl NativeCompetitiveSideboardScorerV1<'_> {
    pub fn score_step_v1(
        &mut self,
        decision: &ExternalPlayerVisibleSideboardStepV1,
    ) -> Result<NativeCompetitiveSideboardScoreV1, NativeCompetitiveAuxErrorV1>;
}
```

## Deployment and training identity

The auxiliary heads must be loaded from an immutable package bound to the exact
gameplay checkpoint identity, both visible input schemas, both head parameter
digests, and the encoder implementation digest. The adapter's deployment
commitment must include that package before its capability inventory may claim
either head. A public trait implementation or caller-supplied booleans cannot
mint that provenance.

Pregame and sideboard training use terminal match win or loss as the only
reward and promotion measure. An estimator may propagate that terminal result.
Hand-strength labels, matchup tables, game-one outcome, card-quality scores,
and sideboard heuristics are not rewards. A legacy gameplay-only checkpoint is
not auxiliary-ready merely because it can score in-game decisions.

## Canonical conformance source

`../mtgo_dxgi_capture_v1/fixtures/player_visible_competitive_auxiliary_heads_conformance_source_v1.json`
is the legacy executable game-two handoff fixture. It freezes one complete
prior game as separate confirmed-decision and rendered-Game-Log streams, the
exact seven-card Keep or Mulligan input, and both unchanged and
inventory-conserving changed sideboard targets. Its whole-target sideboard
responses are structural compatibility cases, not native model provenance.
`../mtgo_dxgi_capture_v1/fixtures/player_visible_competitive_london_bottoming_conformance_source_v1.json`
adds the required game-one empty-history replacement plus exact bottom-one
Select and Submit states. Together their tests recompute the existing adapter
input and selection commitments plus source-only commitments that bind each
current decision to the complete prior-game history.

The fixture deployment digest is a fixture-only sentinel. The file is not a
checkpoint package, production ratification, event session, entry decision, or
input authority. A conforming kernel implementation must consume the history
payload as model information, preserve the two independent clocks, and return
the same semantic responses without importing adapter lineage.

## Adapter return path

After the kernel types exist, the adapter should:

1. convert its checked visible payload and completed-match history into the
   kernel-owned types without reconstructing `ObservationV5`;
2. borrow the exact auxiliary scorer from the retained loaded deployment;
3. bind the returned checkpoint and auxiliary-package identities to that
   deployment;
4. privately map the selected pregame index or opaque submitted sideboard
   deliberation back to the retained exact semantic request;
5. resume only through the existing fresh-capture, visible-control,
   postcondition, one-input, and sideboard inventory gates;
6. require the separate attended launch and event authorization already owned
   by the operator. Model output never supplies that authority.

## Acceptance tests

1. Game one accepts only empty history; games two and three require every prior
   completed game in exact order.
2. Swapped, duplicated, missing, or cross-match history rejects.
3. Separate decision and Game Log stream order is preserved without a claimed
   cross-source clock.
4. Serialization and debug output contain no adapter lineage, raw log text,
   numeric card identity, hidden opponent facts, or authority.
5. Pregame output width equals the exact legal-action count and all outputs are
   finite.
6. Changing one visible card, bottom slot, score, action order, or public
   history fact changes the pregame input commitment.
7. Sideboard local moves conserve inventory at every step and submission is
   bounded to 64 decisions.
8. Both unchanged and changed final targets validate; arbitrary or
   inventory-changing targets reject.
9. A crossed gameplay checkpoint, auxiliary package, encoder, head, or input
   schema rejects before an operator session can resume.
10. Compile-fail checks prove no score or selection exposes an event session,
    coordinate, input command, entry, or spending conversion.

These heads are wiring dependencies, not evidence that competitive training is
complete. League and Challenge dispatch stays closed until their concrete
terminal-outcome-trained deployments and opaque adapter return paths exist.
