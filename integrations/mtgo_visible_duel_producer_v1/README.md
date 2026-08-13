# MTGO visible duel producer v1

This isolated .NET Framework 4.7.2 assembly is the in-process root seam for a
future direct player-visible MTGO projection. It locates exactly one visible
`Shiny.Play.Duel.DuelScene` through the WPF visual tree, requires its public
`DataContext` to be the exact duel view-model type, and checks that all 46
compile-time allowlisted public getters exist on the pinned presentation
assemblies.

The current producer does not invoke any MTGO property getter. Its only output
is a fixed `MtgoVisibleDuelViewModelBrokerResultV1::Abstained` JSON value. It
has no logger, file access, network access, child-process access, action call,
raw-value serializer, free-form error, model scorer, or input surface. A duel
surface that passes the root and metadata checks returns
`projection_incomplete`; every other condition returns one fixed abstention.

This proves the smallest root-object route without reading a live game object
or broad heap scanning. It does not attest injection, execution, a complete
visible projection, model scoring, input, event entry, or spending. The next
tranche must implement each projection field from the exact getter allowlist,
qualify it against reviewed composed UI frames, and retain the same first
exported player-visible schema.
