# Competitive pregame model interface, 2026-08-11

## Finding

The current native checkpoint cannot make MTGO pregame decisions. Its public
inference boundary accepts only a gameplay `ObservationV5` and ordered
`ActionSemanticV1` values. `ActionSemanticV1` has no Keep, Mulligan, London
card-selection, or Submit action, and the Flat V2 encoder has no pregame
observation. The existing `MtgoNonModelPregameHeuristicV1` is therefore a
wiring stopgap, not the mtg-kernel RL model.

The competitive adapter now carries the two additional public facts needed by
a serious pregame policy: whether the acting player is on the play or draw,
and the current public match score. Game number alone is not enough.

The adapter has a checked-untrusted structural contract for those facts.
It requires the canonical play-or-draw, acting-player score, and opponent-score
regions in that order, rehashes each region from the supplied BGRA frame,
enforces legal best-of-three score progression, and binds the result to the
same exact pregame classification frame. It intentionally grants no model or
input authority until a heldout visible-context evaluation is admitted. The
adapter now also defines that evaluation over all eight legal combinations of
play or draw and best-of-three game score. It requires unique manually reviewed
source frames, full prediction coverage, and exact labels and commitments in
every state. A second bounded classifier protocol now runs over the same
retained pregame pixels, requires its separately evaluated classifier binary
to equal the verified runtime executable, and consumes both checked results
into one move-only model context. The production ratification root remains
empty, and the context exposes no native-scoring or input conversion.

## Required mtg-kernel surface

Add one typed pregame decision surface with these minimum fields:

1. exact deck and current post-sideboard configuration commitments;
2. seven ordered private opening-hand card identities with stable per-hand
   object identities;
3. prospective keep size;
4. ordered confirmed London-bottom history and current selected slots;
5. play or draw status, game number, and public games-won score;
6. a complete ordered legal-action vector containing Keep, Mulligan with the
   exact next hand size, SelectForBottom with a stable hand object, or Submit;
7. a decision commitment that binds the complete observation and action order.

The native inference API should accept that typed decision and return one
finite logit per ordered action plus a finite value. Its output must retain the
same run, checkpoint, parameter, generation, encoder, and pregame-head
identities already expected from gameplay scoring. The scorer grants no input,
entry, spending, or account authority.

The preferred implementation is a versioned pregame head inside the same
checkpoint package. A checkpoint-bound sidecar is acceptable for an initial
implementation only if its manifest and parameters are committed by the base
checkpoint deployment and cannot be substituted independently.

## Training contract

Pregame examples must come from simulator-owned pregame states and use the
game's terminal win or loss as the only reward and promotion measure. A value
estimator may propagate that terminal result, but mulligan heuristics, hand
quality labels, material bonuses, or shaped intermediate rewards must not be
treated as reward. Training and evaluation must cover play and draw, every
prospective keep size, every London-bottom count, both seats, supported decks,
and post-sideboard configurations.

## Adapter binding

Once the native surface exists, the adapter should:

1. derive the typed pregame observation only from the retained classified
   player-visible frame, exact event session, and visible history;
2. require complete per-field provenance and the canonical legal-action order;
3. call only the concrete checkpoint-bound pregame scorer;
4. carry the response and selected-action commitments into the existing
   coordinate-private action plan;
5. reuse the existing immediate recapture, one-click shared gate, and exact
   visible postcondition checks without changing their input behavior;
6. bind the League or Challenge permission to the exact pregame model
   deployment rather than to the temporary heuristic.

## Acceptance gates

The model path is not wiring-complete until all of the following are true:

- the current checkpoint package exposes the typed pregame head;
- play or draw and match score remain source-bound from the same visible MTGO
  frame through the native pregame scoring call;
- deterministic simulator fixtures prove action ordering and checkpoint
  identity parity;
- heldout pregame evaluation covers every legal stage and supported deck;
- the model-selected action reaches the existing fresh-frame and
  postcondition gates without a heuristic conversion;
- the heuristic path cannot satisfy the model-backed readiness flag;
- production roots remain empty until the exact artifacts are reviewed.

The legacy non-model executor is now crate-private and absent from the capture
crate's public exports. The heuristic may still produce offline comparison
plans, but it cannot send League or Challenge input even if its legacy review
root is populated. A future public executor must consume a non-forgeable opaque
native selection that retains the exact checkpoint and deployment commitment;
plain actions, indices, logits, serialized records, and caller assertions are
not sufficient provenance.

This contract changes no training code and performs no live MTGO operation.
