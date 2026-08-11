# Competitive sideboard model interface, 2026-08-11

## Finding

The current native checkpoint cannot choose a changed sideboard configuration.
`ObservationV5`, `ActionSemanticV1`, `FlatScoringDecisionViewV2`, and
`NativeCheckpointInferenceV1::score_decision_v1` describe decisions inside one
game. The kernel has no best-of-three match episode, between-game observation,
sideboard action vocabulary, or sideboard checkpoint head. References to
sideboard cards in `card_def.rs` and the fixed Burn mainboard provenance in
`rl.rs` do not provide those surfaces.

The competitive adapter already implements the downstream UI mechanics. It can
reconstruct one exact visible mainboard and sideboard configuration, validate a
coordinate-free target configuration against the exact deck inventory, derive
deterministic transfers, perform one-card drags with immediate visible
confirmation, and submit only a newer exact target configuration. The event
coordinator deliberately has no production constructor that turns an arbitrary
application-supplied target configuration into that changed-sideboard chain.
The driver also always stops at sideboard resolution, and generic lifecycle
preparation rejects Submit Deck unless the control retains exact target-ready
provenance. Unchanged submission remains unavailable until a future opaque
native model decision supplies its proof.

## Required mtg-kernel match surface

Add a versioned best-of-three match session that owns consecutive games and the
between-game transition. Its model-visible sideboard observation must contain:

1. the exact deck-list, canonical deck-manifest, current mainboard, and current
   sideboard commitments;
2. the current public match score and the completed game number;
3. the acting player's play-or-draw status for the next game when the rules make
   it known;
4. a canonical public-history commitment covering prior-game terminal outcomes
   and only cards, actions, zones, and facts that were visible to the acting
   player during those games;
5. stable deck-local card identities and counts for every mainboard and
   sideboard card kind;
6. the exact policy deployment, sideboard encoder, sideboard head, and match
   rules identities;
7. one decision commitment binding the complete observation and canonical
   ordered action vector.

The observation must not contain the opponent's hidden deck list, hand,
library, unrevealed sideboard, client memory, network state, replay-only facts,
or any other information unavailable to the seated player. A model may infer an
opponent strategy from public history, but the input must preserve the source
information boundary.

## Required sideboard action surface

Use a pure local deliberation session before any UI action. The minimum action
vocabulary is:

1. `MoveOneToMainboard` for one deck-local card identity currently available in
   the sideboard;
2. `MoveOneToSideboard` for one deck-local card identity currently available in
   the mainboard;
3. `SubmitConfiguration` when the candidate configuration satisfies the exact
   format size constraints.

Each decision exposes the complete ordered legal action vector. Canonical order
must be fixed by action kind and then stable card identity. Applying a move
updates only the local candidate configuration and emits no MTGO input. The
session is bounded to at most 64 model decisions; failure to select
`SubmitConfiguration` stops with no input. Submitting the initial configuration
is the model-owned no-change decision.

This sequential surface avoids enumerating every legal 75-card partition while
still producing one exact final target configuration. The native output must
return one finite logit per ordered action plus a finite value and preserve the
same run, checkpoint, parameter, generation, encoder, and head identities as
the loaded deployment.

## Training contract

Training requires simulator-owned best-of-three episodes, including sideboard
transitions and next-game play-or-draw rules. Terminal match win or loss is the
only reward and the only promotion measure. A value estimator may propagate
that terminal result, but card-quality labels, matchup tables, material scores,
game-one outcomes, sideboard heuristics, and shaped intermediate bonuses are
not rewards.

Training and evaluation must cover both seats, play and draw, every legal match
score, unchanged and changed configurations, supported decks, opponent public
histories, and every supported sideboard card. A deck containing a card the
kernel cannot represent remains ineligible for competitive deployment.

## Adapter binding

The native checkpoint scorer must be the only production source of a target configuration.

Once the native surface exists, the adapter should:

1. consume the move-only `OpaqueMtgoMeasuredCompetitiveEventSideboardV1` and
   the exact event-scoped public-history accumulator without exposing its pixels
   or control rectangles;
2. construct the typed kernel observation from the exact visible configuration,
   prior model-visible game history, score, play-or-draw fact, deck manifest,
   and policy deployment;
3. call only the concrete checkpoint-bound sideboard scorer and retain every
   intermediate decision and selected-action commitment;
4. convert the submitted native target into
   `MtgoCompetitiveSideboardSelectionV1`, then reuse
   `validate_competitive_sideboard_selection_v1` for exact inventory and size
   validation;
5. consume the measured event runtime and validated native selection into the
   existing private `OpaqueMtgoPlannedCompetitiveEventSideboardV1` constructor;
6. reuse the existing deterministic transfer order, immediate recapture,
   one-drag shared gate, exact visible inventory confirmation, and proof-specific
   Submit Deck path without changing their input behavior;
7. add a proof-specific unchanged binder that consumes the opaque native
   `SubmitConfiguration` result while retaining the model decision commitment;
8. remove any production route in which application code can supply
   `MtgoCompetitiveSideboardSelectionV1` without the opaque native scorer result.

The current public coordinate-free sideboard validator may remain available for
offline tests. It must not become model provenance or production authority.

## Acceptance gates

The changed-sideboard path is not wiring-complete until all of the following are
true:

- the kernel implements best-of-three match episodes and the typed sideboard
  observation and action surface;
- a checkpoint package commits a terminal-outcome-trained sideboard head and
  exposes it through a concrete native inference handle;
- the adapter accumulates only prior acting-player-visible game history across
  the exact event and match lineage;
- deterministic fixtures prove legal-action ordering, local deliberation state
  transitions, bounded termination, and checkpoint identity parity;
- the model can select both an unchanged and a changed target configuration and
  each reaches the existing exact visible transfer and submission gates;
- arbitrary externally supplied target configurations cannot enter the event
  coordinator's production sideboard path;
- heldout evaluation covers supported decks, every public match context, both
  modes, and changed and unchanged configurations;
- terminal match win or loss remains the only reward and promotion criterion;
- production perception, sideboard-evaluation, automation, correspondence, and
  event authorization roots remain empty until the exact artifacts are reviewed.

This contract changes no training code and performs no live MTGO operation.
