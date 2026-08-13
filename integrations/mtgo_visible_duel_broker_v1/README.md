# MTGO visible duel broker v1

This isolated native broker supplies the first load-and-invoke path for the
managed visible-duel producer. It creates a fixed-size local memory channel,
loads the bootstrap into one explicitly named process, invokes the one managed
producer method through the process's already-loaded CLR v4 runtime, validates
the result against the fixed abstention catalog, erases temporary buffers, and
prints only the validated abstention.

The offline end-to-end test uses a disposable synthetic .NET Framework process.
This build admits only the exact `synthetic_managed_host_v1.exe` file name on a
native x64 process and explicitly rejects MTGO or any other target. It proves
the loader, CLR invocation, bounded channel, and fixed output without touching
MTGO. This tranche does not pin a live MTGO executable, deployment,
account, event, match, game, before or after frame, producer binary, or broker
binary. It does not yet unload the managed assembly from the target process.
It therefore is not an audited live broker runtime and grants no semantic
evidence, model scoring, input, event entry, or spending authority.

The next live qualification must first bind the exact signed MTGO client and
hashed deployment, bracket the call with unchanged visible frames, and use the
still-abstention-only producer. No property getter may be invoked during that
qualification.

The separate `mtgo_visible_duel_live_broker_v1.exe` build is now compile-pinned
to the exact MTGO 3.4.158.4691 executable, `DuelScene.dll`, `Card.dll`,
`WotC.MtGO.Client.Model.Reference.dll`, bootstrap, and producer hashes. It
requires one native x64 MTGO process, the
exact file and product versions, a valid Authenticode signature, and unchanged
process start time and file identities before and after invocation. Its output
validator still accepts only the four fixed abstentions. This is the candidate
for the first live abstention qualification, not an admitted semantic producer.

The first live qualification passed against the exact pinned client and
returned only `duel_surface_unavailable`. The client remained responsive and
the call did not focus, move, capture, click, type, score, or invoke any MTGO
property getter. This attests only the identity-pinned load-and-invoke path.
Because no duel root was present, it does not attest root discovery, visible
projection, legal-action completeness, semantic evidence, model scoring, or
input. The native bootstrap remains loaded until that MTGO process exits.
