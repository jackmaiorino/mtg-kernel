# MTGO visible duel producer v1

This isolated .NET Framework 4.7.2 assembly is the in-process root seam for a
future direct player-visible MTGO projection. It locates exactly one visible
`Shiny.Play.Duel.DuelScene` through the WPF visual tree, requires its public
`DataContext` to be the exact duel view-model type, and checks that all 46
compile-time allowlisted public getters exist on the pinned presentation
assemblies.

Version 1.3 invokes only exact allowlisted getters for the player-visible game
chrome, two player panels, mana display, prompt, visible standard buttons,
public zones, battlefield and stack cards, attachments, and visible counters.
It checks exact declaring types, bounded collections, bounded visible strings,
and basic two-seat consistency. It also performs nine exact private joins from
visible enabled prompt controls and the seated player's visible cards to the
corresponding action objects. Those joins read only visible action labels,
action type, locally-performable classification, cast, activated, and mana
classification, and visible mode labels. Raw action objects, internal action
identifiers, targets, timestamps, flags, and card or player backing objects are
never exported. It never enumerates either library. It
enumerates the seated player's hand, but not the opponent's hidden hand. It
does not read a face-down card name unless that zone is explicitly visible to
the seated player. Getter values remain local and are discarded. The only
output is still a fixed
`MtgoVisibleDuelViewModelBrokerResultV1::Abstained` JSON value. The one public
method writes that bounded result to an exact broker-created local memory
channel and returns only a fixed transport status code. It has no logger, file
access, network access, child-process access, action call, raw-value
serializer, free-form error, model scorer, or input surface. A duel surface
that passes the root, metadata, and visible-chrome checks returns
`projection_incomplete`; every other condition returns one fixed abstention.

The synthetic WPF fixture verifies all intended chrome, zone, card, and private
visible-action join getters
and booby-traps either-library enumeration, opponent hidden-hand enumeration,
closed revealed-zone enumeration, opponent card-action collection inspection,
and face-down name reads. It also verifies
that no fixture value enters the output channel. It does not attest execution
of version 1.3 inside MTGO, a complete visible projection, model scoring,
input, event entry, or spending. The already-qualified live broker pins the
older producer binary and must be rebuilt and requalified after an MTGO client
restart before version 1.3 is used live. The next tranche must complete the
sanitized legal-action mapping, visible-history handling, and output schema,
qualify them against reviewed UI frames, and retain the same first exported
player-visible schema.
