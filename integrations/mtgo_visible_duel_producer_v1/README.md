# MTGO visible duel producer v1

This isolated .NET Framework 4.7.2 assembly is the in-process root seam for a
future direct player-visible MTGO projection. It locates exactly one visible
`Shiny.Play.Duel.DuelScene` through the WPF visual tree, requires its public
`DataContext` to be the exact duel view-model type, and checks that all 46
compile-time allowlisted public getters exist on the pinned presentation
assemblies.

Version 1.1 invokes only 19 exact allowlisted getters for the player-visible
game chrome, two player panels, mana display, prompt, and visible standard
buttons. It checks exact declaring types, bounded collections, bounded visible
strings, and basic two-seat consistency. Getter values remain local and are
discarded. The only output is still a fixed
`MtgoVisibleDuelViewModelBrokerResultV1::Abstained` JSON value. The one public
method writes that bounded result to an exact broker-created local memory
channel and returns only a fixed transport status code. It has no logger, file
access, network access, child-process access, action call, raw-value
serializer, free-form error, model scorer, or input surface. A duel surface
that passes the root, metadata, and visible-chrome checks returns
`projection_incomplete`; every other condition returns one fixed abstention.

The synthetic WPF fixture verifies that all 19 intended getters are invoked and
that no fixture value enters the output channel. It does not attest execution
of version 1.1 inside MTGO, a complete visible projection, model scoring,
input, event entry, or spending. The already-qualified live broker pins the
older producer binary and must be rebuilt and requalified after an MTGO client
restart before version 1.1 is used live. The next tranche must add the visible
zones, cards, counters, and action joins from the exact getter allowlist,
qualify them against reviewed UI frames, and retain the same first exported
player-visible schema.
