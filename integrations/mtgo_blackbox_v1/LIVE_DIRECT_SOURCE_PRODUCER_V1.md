# Live direct-source producer v1

## Outcome

Direct parsing is eligible for MTGO League and Challenge wiring under the
reported Daybreak permission. The constraint is on information, not transport:
neither the model nor an operator-facing artifact may receive a fact that the
seated player could not obtain from the UI. The sealed in-process producer may
inspect client presentation or backing objects transiently, including values
used only to join visible controls to visible semantics, provided no raw value,
hidden fact, internal identifier, transport detail, or unrestricted object can
leave that boundary. A direct source is useful only when its first exported
success value is the same `MtgoPlayerVisibleDuelDecisionInputV1` as the reviewed
UI frame.

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
4. exact client presentation or backing-model access inside a sealed producer,
   provided every exported field has a demonstrated UI counterpart and all
   non-visible values are discarded before the first exported value.

The permission is transport-neutral, so process attachment, in-process loading,
reflection, or another direct route is not rejected merely because of its
mechanism. Those mechanisms are eligible only inside the sealed producer. Raw
objects and values must never be serialized, logged, diagnosed, retained, or
returned. Replay-only facts, hidden cards, RNG state, future draws, private
opponent state, and internal identifiers are never eligible outputs.
Discovering that a field exists does not establish that it has a UI-equivalent
meaning.

## Trusted-broker boundary

The live producer may temporarily receive or locate client objects, but no raw
representation may cross its first exported boundary. A small audited Windows
broker owns loading, identity checks, invocation, and output validation. For
every decision:

1. capture an admitted acting-player UI frame and retain it privately;
2. invoke the exact hashed producer, which uses only audited access paths and
   discards all non-visible values before returning;
3. require the producer's first returned value to be bounded visible-schema
   JSON or a fixed abstention;
4. accept only a complete `MtgoPlayerVisibleDuelDecisionInputV1` or an explicit
   abstention;
5. capture a second admitted frame and reject window, process, game, geometry,
   visibility, or source-lineage drift;
6. validate the visible payload strictly and bind it to the two frame and
   source commitments;
7. retain raw acquisition values only inside the opaque transaction and erase
   temporary buffers on every return path.

The producer output is the visible schema only. It has no raw diagnostic or
free-form error channel. The broker must reject unexpected files, child
processes, network access, extra output fields, unknown action variants,
incomplete legal actions, and any attempt to persist raw input. Process access
needed to load or invoke the reviewed producer is held by the broker and never
becomes model or operator data.

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

The first managed root seam identifies the exact public WPF `DuelScene`,
requires its `DataContext` to be the exact duel view-model type, and checks the
46-getter candidate surface. The identity-pinned native broker has now loaded
and invoked that producer once against the exact client, receiving only the
fixed `duel_surface_unavailable` abstention while no duel was open. It
deliberately invoked no MTGO getter in that qualified version. Version 1.2 now
has an offline-qualified layer for player-visible chrome, player panels,
public zones, cards, attachments, and counters. It never enumerates either
library or an opponent's hidden hand, bounds and type checks temporary values,
discards the values, and still emits only `projection_incomplete`. Version 1.2 has not been
loaded into MTGO. This work does not implement a complete projection, admit a
live decision, authorize event entry or spending, or send input. Until the
reviewed complete producer and corpus exist, DXGI remains a candidate source
for board and legal-control reconstruction; direct Game Log parsing remains a
supplemental player-visible history source.
