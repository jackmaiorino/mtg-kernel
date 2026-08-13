# Player-visible native duel scorer v1

## Outcome

Add a kernel-owned scoring entry point for the existing native checkpoint that accepts only player-visible game facts. The MTGO adapter must never pass `ObservationV5`, `ActionSemanticV1`, capture lineage, process data, MTGO identifiers, kernel stable references, card-database identifiers, or simulator state to this entry point.

This is a compatibility path for existing weights, not a claim that the checkpoint was trained on the new projection. It grants no input, event-entry, or spending authority. League and Challenge dispatch remain closed until the scorer, public-history importer, operator join, and required ratifications are present.

## Dependency boundary

`mtg-kernel` cannot depend on the adapter crate. Define a kernel-owned `ExternalPlayerVisibleDecisionV1` and related visible structs in a new `external_player_visible_scoring_v1` module. The adapter implements an exact checked conversion from `MtgoPlayerVisibleDuelDecisionInputV1` into that type, then implements `MtgoPlayerVisibleDuelScorerV1` for an opaque scorer borrowing `NativeCheckpointInferenceV1`.

The kernel type is ordinary scoring data. It must contain only:

- relative players: self or opponent;
- turn, phase, active player, priority player, and initiative;
- life, visible mana pools, hand counts, and library counts;
- visible battlefield, graveyard, exile, stack, combat, relations, own hand, and legitimately known cards;
- decision-local visible object ordinals;
- visible card names;
- the exact ordered player-visible legal-action vector.

## Existing checkpoint compatibility encoding

The kernel encoder constructs the private Flat V2 packet directly. It must not reconstruct an `ObservationV5`.

| Flat V2 feature family | v1 source | Rule |
|---|---|---|
| phase, active, priority, initiative, life, mana, hand and library counts | current visible state | Encode relative to the seated player. |
| card token | visible card name | Resolve by exact unique name in the frozen kernel card catalog. Unknown or ambiguous names fail closed. Never accept a numeric ID from the adapter. |
| object identity | decision-local visible ordinal | Use only for within-decision joins. Never treat it as a persistent or kernel object ID. |
| zone, relative controller, tapped, damage, counters, token, effective power and toughness | current visible state | Encode directly. |
| combat and public object relations | current visible state | Encode exact visible order and links. |
| stack controller, kind, visible target order | current visible state | Encode directly. Any unavailable stack detail is neutral as below. |
| legal action kind and visible parameters | ordered visible actions | Encode in exact supplied order. Choice ordinals are the visible order, not recovered kernel indices. |
| action references | visible action object ordinals plus resolved card names | Reject any unresolved or crossed reference. |
| known library and hand cards | current visible state | Encode only explicitly known identities and positions. |
| card characteristics recoverable from printed card identity | exact frozen name lookup | Static printed facts may be derived from the visible name. Do not derive dynamic game state. |
| prior confirmed actions and rendered Game Log facts | separate public-history streams | Import as separate ordered streams. Never invent a total order across their independent clocks. |

The following unavailable simulator features must have one documented fixed neutral encoding and may never be inferred from MTGO internals: priority-pass bookkeeping; stack or mana activity flags; last internal activator; harness and simulator policy stage bookkeeping; internal pending and private context; zone-change generations; arena IDs; face indices not visibly distinguished; internal ability-use counters; entered-turn markers; hidden summoning-sickness state; skip-untap flags; hidden goad expiry; continuous-effect records not reconstructed from the UI; internal play-permission records; completed-dungeon IDs not visibly reconstructed; internal stack copy, cast-method, mode, kicked, flashback, and X fields not visibly reconstructed. Neutral values carry no match information and must be constant across fixtures.

Declare-attackers and the single-attacker declare-blockers slice are the two
explicit local-deliberation cases.
MTGO presents visible attacker toggles plus `Done`, while the trained policy
expects a sequential false-or-true scan. The adapter supplies only the ordered
visible candidate ordinals, current candidate index, selected earlier model
choices, current candidate, and remaining visible candidates. Those values are
derived from visible battlefield order and the model's own local choices, not
from MTGO backing state. The kernel may use them to reproduce the trained
attacker-inclusion stage. It must not substitute simulator pending context or
inspect client metadata. Model deliberation completes before any MTGO input;
the adapter then executes only the visible toggles whose desired states differ
and binds the visible `Done` control separately.

For the single-attacker blocker slice, the adapter supplies the one visible
attacker ordinal, ordered visible blocker candidates, the current candidate,
earlier model choices in the same scan, and exact `false` then `true` choices.
It must not pass an MTGO target identifier or infer a target from hidden state.
This compatibility input is scoring-only until a separately qualified visible
block postcondition and sealed dispatcher exist.

If a legal visible action cannot be encoded without one of those hidden values, the scorer must abstain. It must not inspect the client, accept a numeric substitute, or call the complete-observation scorer.

## Kernel API

The minimum API is:

```rust
pub struct NativeExternalPlayerVisibleScorerV1<'a> { /* private */ }

impl NativeCheckpointInferenceV1 {
    pub fn external_player_visible_scorer_v1(
        &self,
    ) -> NativeExternalPlayerVisibleScorerV1<'_>;
}

impl NativeExternalPlayerVisibleScorerV1<'_> {
    pub fn replace_public_history_v1(
        &mut self,
        history: ExternalPlayerVisibleHistoryV1,
    ) -> Result<(), NativeExternalPlayerVisibleScoringErrorV1>;

    pub fn score_v1(
        &mut self,
        decision: &ExternalPlayerVisibleDecisionV1,
    ) -> Result<NativeExternalPlayerVisibleScoreV1, NativeExternalPlayerVisibleScoringErrorV1>;
}
```

The score owns finite ordered logits and a finite value plus checkpoint and input commitments. It exposes no Flat packet, raw tensor, `ObservationV5`, semantic kernel action, action binding, random seed, or consume method.

For the existing non-recurrent checkpoint, `replace_public_history_v1` validates and commits the two streams but the score must report `public_history_used_by_model_v1() == false`. A future recurrent head can report true only after its training and deployment identity bind that input contract.

## Required tests

1. The scorer type cannot be constructed from public fields and has no input or session conversion.
2. Changing any player-visible fact or legal-action order changes the input commitment.
3. Kernel seats, stable references, zone incarnations, transport metadata, and other excluded bookkeeping cannot appear in the input type or debug output.
4. Unknown and ambiguous card names fail closed. Exact frozen names resolve deterministically.
5. Every neutral feature remains bit-identical when excluded source bookkeeping changes.
6. A visible object reference outside the current decision fails closed.
7. Output width equals the exact visible legal-action count and every output is finite.
8. Separate history streams preserve within-source order and reject duplicate, missing, or out-of-order positions without claiming a cross-source order.
9. A parity fixture built only from facts present in both schemas documents the exact delta from the ordinary Flat V2 tensor. Any non-neutral difference must be enumerated.
10. The existing checkpoint path reports compatibility mode and `public_history_used_by_model_v1() == false`.
11. Attacker inclusion preserves visible battlefield order, always presents
    `false` then `true`, uses only explicit local deliberation context, and
    produces no client action while the scan is pending.
12. Single-attacker blocker inclusion preserves visible battlefield order,
    rejects a second attacker or preexisting assignment, always presents
    `false` then `true`, and has no client target or input conversion.

## Readiness rule

Only after the kernel API and adapter implementation pass these tests may these static facts become true:

- `native_checkpoint_player_visible_only_duel_action_interface_present`;
- `current_duel_scorer_kernel_bookkeeping_withheld`.

`public_model_owned_duel_action_path_present` remains false until the opaque operator dispatch consumes the exact loaded deployment, imports current public history, scores the retained current perception, and returns the selected visible action into the existing control, gesture, input, and confirmed-postcondition chain.
