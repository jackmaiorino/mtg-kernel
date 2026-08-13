# MTGO visible duel broker v1

This isolated native broker supplies the first load-and-invoke path for the
managed visible-duel producer. It creates a fixed-size local memory channel,
loads the bootstrap into one explicitly named process, invokes the exact
selected managed producer method through the process's already-loaded CLR v4 runtime, validates
fixed abstentions in-process, sends a data-bearing candidate only to the strict
Rust schema validator through an anonymous pipe, erases temporary buffers, and
prints only the exact accepted copy.

The offline end-to-end test uses a disposable synthetic .NET Framework process.
This build admits only the exact `synthetic_managed_host_v1.exe` file name on a
native x64 process and explicitly rejects MTGO or any other target. A second
visible-chrome fixture proves that the native loader releases a synthetic
opening decision only after the strict validator accepts it, and refuses to
release it when the validator cannot execute. These tests do not touch MTGO.
The offline build does not pin a live MTGO executable, deployment,
account, event, match, game, before or after frame, producer binary, or broker
binary. It does not yet unload the managed assembly from the target process.
It therefore is not an audited live broker runtime and grants no semantic
evidence, model scoring, input, event entry, or spending authority.

The next live qualification must bind the exact signed MTGO client and hashed
deployment, then run the sanitizer against an actual no-stakes duel. Only the
sanitizer's seated-player-visible projection may cross the broker boundary.

The separate `mtgo_visible_duel_live_broker_v1.exe` build is now compile-pinned
to the exact MTGO 3.4.158.4691 executable, `DuelScene.dll`, `Card.dll`,
`WotC.MtGO.Client.Model.Reference.dll`, bootstrap, producer, and release
validator hashes. It requires one native x64 MTGO process, the exact file and
product versions, a valid Authenticode signature, and unchanged process start
time and file identities before and after invocation. It may release one of
four fixed abstentions or a data-bearing projection accepted by the strict
validator.

The earlier abstention-only live qualification passed against the exact pinned client and
returned only `duel_surface_unavailable`. The client remained responsive and
the call did not focus, move, capture, click, type, score, or invoke any MTGO
property getter. This attests only the identity-pinned load-and-invoke path.
Because no duel root was present, it does not attest root discovery, visible
projection, legal-action completeness, semantic evidence, model scoring, or
input. The old native bootstrap remains loaded until that MTGO process exits.
A clean client restart is required before qualifying the new producer and
validator.

The offline broker also proves a sealed synthetic action call. It binds one
selected index to the SHA-256 of the exact strictly validated visible decision,
returns only a fixed submitted or rejected receipt, and rejects every later
index for the consumed decision. The observe-only live broker compile-disables
that command. A separately named live dispatch broker is built with the same
exact client, deployment, producer, and validator identity checks, but includes
the sealed dispatch command. Its binary is not sufficient authority to act.
The Rust runtime's independent ratification root remains empty until the scorer,
attended authorization, fresh-state, and newer visible-postcondition checks are
joined and qualified together.

The offline broker also accepts the separately typed attacker transaction. Its
command carries only the exact current sanitized selection hash, candidate
count, desired-mask commitment, and model-plan commitment. It invokes the
producer's attacker-step method, which rebuilds the complete visible state and
submits at most one toggle or a fully reconciled `Done`. The adversarial WPF
test proves the native observe plus sealed-step path and proves that replaying
the unchanged selection is rejected. The observe-only live build rejects this
command at the same compile-separated gate as ordinary dispatch. No live
attacker input has been attempted or authorized.

The broker now also carries the dormant blocker-step command. Its payload is
limited to the exact sanitized source hash, selected visible index, blocker
stage and visible ordinals, model-selection commitment, and execution-step
commitment. It contains no client object, target-set content, coordinate, or
input primitive. The native broker compiles this route with warnings as errors;
the producer's synthetic WPF fixture covers its staged semantics. The running
MTGO process still has the older producer loaded, so this new path has not been
loaded into or invoked against the live client.

The producer v1.5 lobby-only clean-restart result and exact binary manifest are
recorded in `LIVE_QUALIFICATION_2026-08-13.md`. It returned the expected fixed
duel-surface abstention. A successful no-stakes duel projection remains open.
