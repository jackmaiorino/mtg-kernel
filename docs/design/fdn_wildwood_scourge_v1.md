# Wildwood Scourge prepared source v1

Owned base cf0ea5a0133fcfc5dff1d19f655f178baedca016. Frozen FDN booster
membership includes Wildwood Scourge; the actual combined base+FDN registry
does not. No existing Wildwood prepared filename/source/worktree was found.
Vampire Gourmand was considered but an existing owner has that source root.

Pinned primary Mage a5c90fe180021e70e2a644ade00eeab07f857a40,
Mage.Sets/src/mage/cards/w/WildwoodScourge.java: XG, green 0/0 Hydra.
EntersBattlefieldWithXCountersEffect supplies X counters. Its COUNTERS_ADDED
trigger checks P1P1, another recipient, creature, not effective Hydra and
recipient control, independent of which player placed the counters. One
event adding several counters makes one trigger, whose effect adds one to
the exact captured source.

## Core

Reuse PutSourceOntoBattlefieldWithXPlusOneCounters, the qualified normal
Nyxborn Hydra entry operation. The generated StandardProgram recipe names a
pure existing-op factory, with ordinary XG metadata parsed by the existing
quote/X choice/payment/finalization/restore paths. No bestow, reach or trample
is inherited. Actual X is in the finalized stack context, not current mana
or a new pending field. Zero enters and then dies through ordinary SBA.

Append AnotherControlledNonHydraCreatureGetsPlusOneCounters to the existing
trigger enum. Reuse BindPlusOnePlusOneCounterToTriggerSource and its exact
BF/ZCC materialization. Counter commitment calls the small new producer after
a positive existing PlusOneCountersAdded record. Only actual executable,
active printed Wildwood sources in Limited without Standard are captured.
The event's actual recipient BF/ZCC, controller, creature type and effective
Hydra subtype are read immediately. The placing player's identity is not a
control predicate. The pending trigger captures the actual source controller
and definition-owned program using the existing source contract.

Immediate capture stores existing PendingTrigger objects in an optional
wildwood_waiting_v1 unpublished queue, separate from the placement queue.
capture_triggers_without_sba transfers them into the resolving owner's normal
waiting batch; the final checkpoint merges ordinary events and SBA captures,
then applies order_apnap once. It runs no SBA and places no stack item during
counter commitment. Already ordered/target-suspended placement rows are never
sorted, reset or extracted. Later collection skips
this condition so source/recipient departure or control/type changes later
in the same instruction sequence cannot erase/reinterpret or duplicate it.
Ordinary placement still handles APNAP and simultaneous controller ordering.
A removed-ability source cannot capture; a captured source remains bound
despite later ability removal/control changes, and departure/reentry causes
the existing counter leaf to skip that older incarnation.

The new optional unpublished queue is omitted from serialized bytes and
Hash input while absent, including after its batch is consumed. No history
event changes. When no qualifying source is present, old events, pending
arrays, serialized bytes and state hash input retain their prior contents. Default and Standard capture hooks
return without mutation. Accepted subtype/target IDs and choice arrays are
unchanged. The appended condition is unadmitted source; parent must compose
it sequentially with other prepared appended conditions before qualification,
preserving the accepted prefix. No numeric condition ordinal is guessed.

## Checks and consumers

Four registered typed primitive groups test recipient class/incarnation and
placing-player independence, existing bound counter blink/control/restore,
absent-source readonly transport and genuine registered Nyxborn normal-X
casting/finalization/replay for both seats. Matcher definition changes are
explicit operand setups, not fabricated printed copy history or an admitted
Wildwood root.

Ten future printed groups require genuine root-owned registration. They use
actual Wildwood X announcement/completion, X0 SBA, actual Felidar Savior
targets and optional counter choices, both-seat grouped/independent counter
placement, exclusions, immediate recipient control/departure capture,
captured source blink/control, actual Witness suppression and effective
recipient subtype changes, restored source/root forgery refusal and
opponent-safe private hand observations. Additional counter placements use
the existing typed public effect producer and are labeled as such; Felidar
Savior is the actual printed counter-producer traversal. No admission hook,
synthetic printed definition or production root-validation bypass is added.

The frozen vector retains Opaque for the new condition. Existing X entry
and bound counter facets alone do not encode recipient-at-event-time
non-Hydra/control, grouped placement, captured source identity or the waiting
capture lifecycle. Frozen Human trigger labeling may describe the existing
counter leaf but cannot claim the exact trigger filter; duplicate-source
and oversized ordering limits remain. No public/Human/model successor or
admitted Full pool card is claimed. A precise companion must bind actual
catalog/source/root identity, finalized X entry, event-time recipient
controller/type/subtype and source BF/ZCC, once per placement event. Public
state needs an authenticated public event/capture representation before any
model or Human completeness claim. Existing engine X choice uses the frozen
u8 X/mana representation; broader printed unbounded-X transport is a separate
successor limitation, not silently solved here.

Pinned rustfmt, JSON parsing and git diff checks are the permitted checks.
Native typecheck, actual gameplay, sampler, registration/catalog/profile,
precise meaning/public/Human/model qualification remain parent-owned and unrun.

## APNAP prerequisite repair

Read-only source review found the original ac6c34f3 capture wrote directly
to pending_triggers, outside order_apnap's newly collected batch. Reversed
NAP/AP counter events and mixed ordinary triggers could therefore publish
in incorrect controller order. This repair introduces only the unpublished
queue and routes it through the existing normal checkpoint. Partial captures
travel with the resolution owner, and immediate captures generated by SBA
entry work join the same final batch. The existing legend suspension protects
the local waiting batch using its established transport.

Three additional future printed groups cover reversed NAP/AP commitment
with post-capture source control changes, mixed real Dwynen's Elite ETB
roots plus counter captures with actual OrderTriggers choices, and a typed
checkpoint probe preserving a genuine ordered partially targeted Felidar
stage. All run both seats and compare restored replay. An additional
registered unit group checks absent queue serde/Hash compatibility. These
fixtures remain unexecuted pending serial root-owned registration and native
qualification. No new consumer or public historical authority is claimed.
