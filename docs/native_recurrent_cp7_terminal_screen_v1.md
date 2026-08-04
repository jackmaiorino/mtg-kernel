# Native recurrent CP7 deployment and terminal screen v1

## Question

Does the recurrent policy that generalized to fresh CP7 action labels improve
natural terminal outcomes when deployed against the exact qualified parent?

CP7 imitation selected the candidate, but it cannot promote it. Natural terminal
win or loss is the only playing-strength outcome. The candidate, parent, XMage
CP7 opponent, environment seeds, and seat schedule are fixed within each matched
pair.

## Deployment

The live scorer tensorizes the exact native structured decision, preserves the
last 32 completed public physical decisions, and sends the state, history,
parent logits, parent value, acting player, and physical-decision substep count
to one persistent CPU PyTorch worker. The worker loads the exact width-128,
two-layer GRU model, applies the same 16-step hard log-ratio projection with
budget `0.49`, then applies the fixed `0.97` deployment scale. It returns only
candidate policy logits. The exact parent value remains unchanged.

The package rejects extra root files, validates the model, model-state, worker,
model-definition, parent, manifest, and composite hashes, disables Python bytecode
writes, and requires exact request sequence numbers. One-row CPU inference took
`1.78 ms`. The release scorer SHA-256 is
`5f8d46a5470daf7b5e8e8b03e86adb3b3fbbee15130b61f7f028712f5e91b71c`.

Package identities:

- Manifest: `55130977d8e5a4d98060e8e436169356205b4a7e1ba47fe567fde487ad233e50`.
- Model file: `6c33f6d449b76e24c00bc7d46052b04488ddb9ec574009831d2fa90ea01bd55d`.
- Model state: `d736296425de2c438bb9be02ab6c89e51da4c17c1408de6ff3309029b2d06dca`.
- Composite model parameters: `397e2576fe71edba2e31a15da654b219e04318c8fe71be3867e333fdf7989dda`.

Two direct scorer resets were bit-identical. A real seat-swapped XMage pair
completed with one candidate win and one loss. A parallel two-pair preflight then
completed all four games in `39.4` seconds with zero exclusions. Its report
SHA-256 is
`71a8c3589fcdc38e2cefca4844a0b6934a67f72e20621a25e1ef6f2849a81c60`.

## Rapid terminal gate

Run 16 fresh seat-swapped pairs at base seed `2020001`. Advance only if candidate
gains are at least candidate losses plus three, both candidate-seat nets are at
least `-2`, and all 16 pairs complete in both arms. The fixed eight-worker
topology runs four matched pairs per batch. No CP7 labels enter adjudication.

All 16 pairs and 32 games completed without exclusions in `205.08` seconds.
Candidate and parent each won 16 games. Paired gain, loss, and tie counts were
`0/0/32`; P0 and P1 nets were both zero. The paired-gain gate failed. Report
SHA-256 is
`c75a15d6aaf18478abbc4c18aaf957c19ebae8f698000c848bb37a8252bd1716`.

The candidate was active. Exact normalized leg comparison found trajectory
changes in 15 of 32 games across 12 of 16 pairs, including 11 P0 games and four
P1 games. None changed a winner. Trace-analysis SHA-256 is
`6adaa85112b04742f9c40154513b993764f622d9f08986ddf5e165ed8c7cb292`.
This rejects the unchanged CP7-imitation deployment on this terminal panel. It
does not establish that the changed decisions were individually worse, and it
does not reject recurrent structured policies trained from terminal outcomes.
A larger gate for this unchanged candidate is not justified.

## Database note

One manual integration pair invoked XMage against the source H2 file instead of
a private copy. Its byte hash changed from the previously pinned
`b833d6a7b44ad1f7bd6aef9a21d1f2498136ef61e44db0e48e60e5ec471ce09d`
to
`1defa6420bcf02b0f79c3313e964efce3b401838231e7ffe86c7c7ee6724e0b1`.
Logical content equality was not inferred. The preflight and rapid gate pinned
the new hash before play and copied that same source into private worker
databases, so treatment and control remain matched. This run should not be
treated as byte-identical to older anchor campaigns.
