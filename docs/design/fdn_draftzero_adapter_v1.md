# DraftZero integration: actor-facing transport prerequisite

Issue110 remains open. The Python client in
[limited_common_session_v1.py](../../python/tools/limited_common_session_v1.py)
uses the existing trusted two-seat common server. It accepts ordered mainboard
rows from the .dck importer, checks the running catalog identity, returns the
current actor's safe DTO and submits its exact episode, generation, seat, prompt
and action index. Sideboards never enter reset. Labels are presentation, not
executable action identities.

Snapshots are opaque handles owned by the current game. Restore returns a fresh
prompt; a prior prompt is rejected before transport. A failed engine request
preserves the last successful prompt. An unsupported state remains explicit and
cannot select a fallback action. Malformed replies and timeouts close the owned
process. A frontend must send each reply only to its acting seat.

## Upstream contract

Inspected DraftZero commit
[32f916fe45490675bd4aede4a8092c2086a79625](https://github.com/danieljbrooks/draft-zero/tree/32f916fe45490675bd4aede4a8092c2086a79625)
on October14. Its
[runner](https://github.com/danieljbrooks/draft-zero/blob/32f916fe45490675bd4aede4a8092c2086a79625/src/draftzero/engine.py)
launches MageZero inference/training scripts and XMage JVMs. Its separate
[StateSpec bridge](https://github.com/danieljbrooks/draft-zero/blob/32f916fe45490675bd4aede4a8092c2086a79625/src/draftzero/gameplay/bridge.py)
builds positions and returns decisions. Neither interface is implemented by this
client.

The next adapter must preserve card-copy order, bind every selected option to
the current kernel prompt, and map complete typed rule facts into a distinct
model contract. MageZero feature IDs and action slots are not kernel identities.
StateSpec position import needs an explicit fidelity check for history, entry
choices, ownership, counters, attachments and pending actions. Missing facts
must remain unsupported. Actual opposing hidden composition cannot supply belief
inputs or policy features.

## Verification and remaining work

Eighteen affected Python cases pass, including eleven new subprocess/client cases.
They establish transport and client behavior, not native gameplay. CI adds a
real common-server check: one land-only seed replayed twice, opaque restore,
stale-prompt rejection, natural terminals and identical transcript hashes.
Its result remains pending.

Complete family consumers, full286-name gameplay coverage, drafting/deck
construction, fair belief sampling, model adaptation, parity comparisons and
representative completed-work throughput remain separate acceptance criteria.
The land-only check supplies no playing-strength or training evidence.
