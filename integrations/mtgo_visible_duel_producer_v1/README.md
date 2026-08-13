# MTGO visible duel producer v1

This isolated .NET Framework 4.7.2 assembly is the in-process root seam for a
direct player-visible MTGO projection. It locates exactly one visible
`Shiny.Play.Duel.DuelScene` through the WPF visual tree, requires its public
`DataContext` to be the exact duel view-model type, and checks that all 46
compile-time allowlisted public getters exist on the pinned presentation
assemblies.

Version 1.5 invokes only exact allowlisted getters for the player-visible game
chrome, two player panels, mana display, prompt, visible standard buttons,
public zones, battlefield and stack cards, attachments, and visible counters.
It checks exact declaring types, bounded collections, bounded visible strings,
and basic two-seat consistency. It also performs fifteen exact private joins
from visible enabled prompt controls and the seated player's visible cards to
the corresponding action objects. Those joins read only visible action labels,
action type, locally-performable classification, cast, activated, and mana
classification, visible mode labels, the current turn, and the displayed mana
color. Raw action objects, internal action identifiers, targets, timestamps,
flags, and card or player backing objects are never exported.

It never enumerates either library. It enumerates the seated player's hand,
but not the opponent's hidden hand. It never reads or exports the retained
name of a face-down exiled card for either player. Getter objects
remain local and are discarded. The producer can serialize a bounded
`MtgoPlayerVisibleDuelDecisionInputV1` slice containing only sanitized
player-visible values, or return one of the fixed abstentions. The slice is
limited to an untouched 60-card Turn 1 Main 1 opening with seated-player
priority, base life, zero mana, and empty public zones, stack, combat,
revealed zone, active prompt, and modal choice. Its supported actions
are source-bound cast, play-land, activated-ability, and mana-ability actions.

The observation method writes that bounded result to an exact broker-created
local memory channel and returns only a fixed transport status code. A second
offline-qualified method accepts only the SHA-256 of that exact serialized
visible decision plus one selected visible action index. It rebuilds the same
sanitized decision, requires an exact hash match, resolves the same current
private action binding, and calls the public `IGame.ExecuteAction` method once.
The same game object rejects every later index for the same exact decision, and
the decision is marked consumed before the client call. Only a fixed submitted or
rejected receipt leaves the producer. The client action object never leaves.

The synthetic WPF fixture verifies all intended chrome, zone, card, and private
visible-action join getters, then passes the serialized result through the
strict Rust producer-result validator. It booby-traps either-library
enumeration, opponent hidden-hand enumeration, closed revealed-zone
enumeration, opponent card-action collection inspection, and face-down name
reads. It also verifies that no hidden fixture value enters the output. The
native offline broker test observes the opening, dispatches its Pass index,
and proves neither that dispatch nor another index for the same decision can be
replayed.

The fixture does not attest execution of version 1.5 inside MTGO, a complete
visible projection, model scoring, live input, event entry, or spending. In
particular, the synthetic slice has not yet confirmed its priority-pass control
against a real duel. Its untouched-opening restriction makes a null Initiative
complete before any game action has occurred, but later states remain
unsupported. The slice is therefore not live-authorized. The
live broker build explicitly rejects the dispatch command until it is joined to
the attended competitive authorization and confirmed-postcondition chain. A
clean MTGO client restart is required before observing with the new binary.
The next tranche must confirm the exact client pass control, add later special
visible game state, and validate the producer result inside the live broker
transaction.

The v1 replay key is deliberately conservative: a byte-identical visible
decision later in the same game also rejects. A production broker must replace
that limitation with a private observation-generation token, without exposing
the token to the model or operator, before live dispatch is enabled.
