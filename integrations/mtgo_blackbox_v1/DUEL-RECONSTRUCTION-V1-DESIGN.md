# MTGO duel reconstruction v1

## Objective

Produce one exact `ObservationV5` and complete ordered `ActionSemanticV1` set from player-visible MTGO state, then pass that immutable decision to the native checkpoint scorer. This tranche does not authorize input, human matches, queues, purchases, Leagues, or Challenges.

MTGO does not expose a visible AI opponent or separate training mode in the current client. `Custom Match` can create one-player Solitaire, which is useful for layout and action calibration but cannot supply a two-player model observation. Constructed Specialty exposes human lobbies and event queues.

## Runtime loop

1. Capture one foreground duel frame through the pinned DXGI Desktop Duplication backend.
2. Measure only visible facts: participants, prompt, turn and phase, priority, timers, player totals, zones, cards, counters, stack, combat, choices, enabled controls, and newly visible game-log lines.
3. Reconcile the measurements with the prior accepted visible state and the prior confirmed action receipt.
4. Resolve every visible card name through the compile-bound kernel card correspondence profile. Reject an unknown or not-fully-supported definition.
5. Advance object lineages. A zone change creates a new incarnation and increments the zone-change count. Reject duplicate-card identity ambiguity unless an action receipt or visible transition uniquely identifies the moved object.
6. Derive history-only kernel fields from the continuous accepted ledger. A capture gap, unrecognized log delta, unexplained object delta, or impossible phase transition invalidates the ledger.
7. Generate the complete ordered semantic action set. The action set is complete only when every enabled visible control has exactly one semantic and every generated semantic has exactly one current visible control.
8. Construct `MtgoObservedDecisionV1` with current-frame provenance for every current observation, binding, and action leaf. Existing validation must accept it before scoring.
9. Score through the immutable native checkpoint seam. Selection remains an offline intent.
10. A separately authorized actuator may resolve one selected semantic against a fresh frame, send one input, and then block until an action-specific visible postcondition is confirmed.

## Observation source rules

| Observation content | Required source |
| --- | --- |
| Participants, turn, phase, priority, life, mana, hand and library counts | Current visible frame |
| Battlefield, graveyard, exile, stack, combat, counters, targets, attachments | Current visible frame plus exact card correspondence |
| Acting player's hand and revealed identities | Current visible frame and accepted visible-knowledge history |
| Card characteristics and executable behavior | Compile-bound kernel card database after exact visible-name correspondence |
| Stable object identities and zone-change counts | Continuous accepted visible-object ledger |
| Priority passes, recent stack or mana activity, turn-local counters, pending multi-step choices | Continuous accepted history plus current prompt and controls |
| Schema versions, kernel version, card database hash, local decision counters | Compile-bound local metadata |
| Legal actions | Complete current visible-control inventory reconciled with the reconstructed kernel decision |

No default value may stand in for an unmeasured or unreconstructed field. Uncertainty produces no observation and no score.

## Implemented lineage slice

The current checked-untrusted ledger covers one exact observed chain only: the retained Island PlayLand calibration followed by the retained Island mana-activation calibration. It deterministically seeds adapter-local arena identifiers, preserves the Island's arena identifier through the Hand-to-Battlefield transition, increments its zone-change count exactly once, and leaves the same battlefield reference unchanged when mana is activated. It rejects frame discontinuity, missing or mismatched source identity, wrong owner or controller, wrong source zone, duplicate replacement identifiers, and a replacement identifier on a non-zone-changing mana activation.

This does not yet reconstruct a duel. Seed labels and calibration transitions remain caller-supplied checked-untrusted data. Production object bindings are withheld, and the ledger cannot create an observation, legal-action set, score, or input command. The next lineage step is deriving canonical visible objects from admitted current-frame measurements and reconciling arbitrary visible additions, removals, and zone changes without identity ambiguity.

## Card coverage gate

The current kernel registry has 136 definitions. Only 49 have full support, including four tokens. Exact-name correspondence now rejects unknown names, unsupported registered cards, case or whitespace changes, and malformed labels.

This is a material League and Challenge blocker, not only a perception issue. An open opponent may reveal any legal card in the format. UI wiring can be complete while the kernel still cannot represent or reason about that card. Competitive readiness therefore requires either full relevant-format coverage or a separately trained unknown-card representation with an explicit model contract. The current `ObservationV5` and checkpoint do not provide that fallback.

## Legal-action completeness

Visible highlighting alone is insufficient. Phase shortcuts can collapse several priority passes, disabled controls can remain rendered, identical cards can share art, and a target or payment prompt can require several sub-decisions.

V1 requires a closed correspondence among:

- the current MTGO prompt and enabled controls;
- the reconstructed kernel decision stage;
- the ordered semantic action vector;
- the selected control's visible region and enabled-state evidence.

Any unmatched control, unmatched semantic, duplicate mapping, ambiguous object, aggregate combat action, or unsupported choice family blocks scoring and input.

## First end-to-end rehearsal

The first valid rehearsal should be a no-stakes, two-player private match against a separately authorized non-human counterpart using decks whose every reachable card and token has full kernel support. Capture begins before the opening hand and remains continuous through the terminal screen.

Success requires:

1. zero capture gaps and zero unexplained visible transitions;
2. every reconstructed observation accepted by the existing provenance validator;
3. every scored action mapped to exactly one current enabled control;
4. one input outstanding at a time with an action-specific visible postcondition;
5. timer and disconnect tests that halt safely;
6. replay of the captured ledger producing identical decision commitments, logits, selections, and terminal outcome.

Only after that rehearsal should navigation be connected to a League or Challenge lifecycle. League and Challenge authorization, account identity, deck identity, entry terms, and existing-resource amount remain separate gates. A purchase path is out of scope.
