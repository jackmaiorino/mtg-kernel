# Competitive match-memory kernel handoff, 2026-08-11

## Adapter result

Commit `ae4e11a` adds the adapter-owned per-game public-memory seam. Every
record begins from, or is extended by, one
`CheckedUntrustedMtgoCompetitiveGameplayPostconditionV1`. The record retains
the exact `ObservationV5` that was scored, the selected `ActionSemanticV1`,
the source and after-frame order, and the decision, selection, deployment, and
visible-postcondition commitments. Event, match, game, and checkpoint lineage
must remain exact. Duplicate or nonmonotonic decisions reject.

The resulting
`CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1` is move-only and
coordinate-free. It exposes read-only semantic views but has no scoring,
input, event-entry, or spending authority.

This is an information boundary, not a pixels-only requirement. Daybreak's
approval, as relayed by Jack, permits direct parsing when the extracted facts
are available to the seated player in the UI. Accessibility objects and the
visible game log are therefore eligible future sources. Network decoding,
broad process-memory reads, and internal state dumps remain out of scope
unless a channel can prove that it yields only currently player-visible facts.

## Exact kernel gap

`NativeCheckpointInferenceV1::score_external_observation_v1` accepts only one
current `ObservationV5` and ordered legal-action vector. It cannot accept
public history. The native structured-history implementation is private and
is populated only while a simulator-owned `FastActorSessionV1` advances. It
cannot import an externally observed MTGO decision sequence.

The smallest gameplay bridge should be a kernel-owned public-history DTO and
an external scorer that accepts:

1. the current complete `ObservationV5`;
2. the current canonical ordered `ActionSemanticV1` vector;
3. a bounded ordered sequence of earlier player-visible observations and
   selected semantic actions from the same game;
4. commitments and ordering fields needed to reject duplicate, crossed, or
   nonmonotonic history.

The kernel should reconstruct its normal structured-history entries from
those observations and actions, including the existing physical-substep
coalescing rule, then invoke the unchanged checkpoint tensors. The response
should bind the current-decision commitment, the canonical public-history
commitment, and the exact loaded checkpoint identity. A checkpoint that does
not consume structured history may accept an empty history only. It must not
silently discard a nonempty history supplied to a recurrent checkpoint.

The external history path must be pure inference. It must not create or
advance a simulator session, infer hidden opponent state, generate MTGO input,
or grant event authority.

## Separate cross-game gap

The adapter record is intentionally not a complete best-of-three match
episode. It presently includes our confirmed model decisions only. Opponent
actions that do not coincide with one of our decision observations, revealed
cards between those decisions, and the terminal game result require separate
player-visible collectors. A future completeness binder must combine those
sources before a sideboard or pregame match head consumes the prior game.

The typed sideboard observation, local transfer deliberation, terminal
match-only reward, and checkpoint-head requirements remain those in
`COMPETITIVE_SIDEBOARD_MODEL_INTERFACE_2026-08-11.md`. Do not treat the new
per-game record by itself as permission to train or enable that head.

## Acceptance checks for the gameplay bridge

1. A fixture built from one simulator session scores bit-exactly through the
   native session path and the external observation plus imported-history path.
2. Physical substeps create exactly one completed history entry under both
   paths.
3. Reordered, duplicated, crossed-checkpoint, actor-mismatched, or
   nonmonotonic entries reject.
4. The response commitment changes when any prior public observation,
   selected semantic, or ordering field changes.
5. Empty history preserves current feed-forward checkpoint behavior.
6. No new API exposes model input as event-entry, spending, or UI-input
   authority.

No live MTGO operation, training change, or compute run is part of this
handoff.
