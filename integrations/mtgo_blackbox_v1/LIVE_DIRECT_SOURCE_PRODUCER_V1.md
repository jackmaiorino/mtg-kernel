# Live direct-source producer v1

## Outcome

Direct parsing is eligible for MTGO League and Challenge wiring under the
reported Daybreak permission. The constraint is on information, not transport:
neither the model nor an operator-facing artifact may receive a fact that the
seated player could not obtain from the UI. A direct source is useful only when
it produces the same `MtgoPlayerVisibleDuelDecisionInputV1` as the reviewed UI
frame, with hidden and transport-only data discarded before the adapter
boundary.

Commit `26ae16e` supplies the offline output-measurement gate. It does not yet
attest a live producer.

## Eligible sources

V1 admits only sources whose connection to the seated player's UI is explicit:

1. composed pixels from an admitted acting-player duel frame;
2. on-screen, in-client Windows UI Automation properties corroborated by the
   same composed pixels;
3. the persisted Game Log representation, after the existing parser retains
   only text and card names rendered by MTGO and discards UUIDs, timestamps,
   flags, markup, numeric identifiers, paths, and process metadata;
4. a later documented or reviewed client export only after every retained field
   has a demonstrated UI counterpart.

V1 does not admit process memory, debugger or injection access, intercepted
network protocol, private object-model reflection, caches containing unrendered
facts, replay-only facts, hidden cards, RNG state, future draws, or internal
identifiers. Discovering that a field exists does not establish that it is
eligible.

## Trusted-broker boundary

The live producer must not receive an unrestricted client-state object. A
small audited Windows broker owns the acquisition channel and passes only one
of the eligible source forms above to a pinned producer. For every decision:

1. capture an admitted acting-player UI frame and retain it privately;
2. collect only eligible visible UIA or rendered Game Log values;
3. invoke the exact hashed producer with bounded private input and output;
4. accept only a complete `MtgoPlayerVisibleDuelDecisionInputV1` or an explicit
   abstention;
5. capture a second admitted frame and reject window, process, game, geometry,
   visibility, or source-lineage drift;
6. validate the visible payload strictly and bind it to the two frame and
   source commitments;
7. retain raw acquisition values only inside the opaque transaction and erase
   temporary buffers on every return path.

The producer stdout is the visible schema only. Stderr is bounded and must not
be forwarded to the model, operator diagnostics, logs, or training. The broker
must reject unexpected files, child processes, network access, debug privilege,
MTGO process handles, extra output fields, unknown action variants, incomplete
legal actions, and any attempt to persist raw input.

## Trust root and qualification

Runtime booleans cannot attest correct filtering. Production admission requires
all of the following in one reviewed commit:

- audited broker and producer source plus pinned reproducible binary hashes;
- an import and capability audit showing only the declared acquisition path;
- exact agreement on the checked reviewed-frame corpus through
  `evaluate_untrusted_player_visible_direct_duel_projection_v1`;
- full coverage of the action families enabled for the deployment;
- adversarial fixtures containing hidden and internal-only fields, proving they
  are absent from output, errors, logs, and retained artifacts;
- a production ratification entry binding the broker, producer, protocol,
  corpus, annotation, and evaluation commitments.

An admitted direct result may replace only the perception source inside the
existing opaque gameplay transaction. It must still bind the exact competitive
lifecycle, complete visible control set, model deployment, selected action,
fresh pre-input frame, input receipt, newer postcondition frame, visible Game
Log corroboration where available, and confirmed player-visible history.

## Nonclaims

This contract does not identify an additional clean MTGO source, attest a
producer, admit a live decision, authorize event entry or spending, or send
input. Until the reviewed producer and corpus exist, DXGI remains the only
candidate source for complete board and legal-control reconstruction; direct
Game Log parsing remains a supplemental player-visible history source.
