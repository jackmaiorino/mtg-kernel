# Custom-deck game interface v1

Issue 110, implementation milestone 2. `kernel_limited_env` is a separate
JSONL binary. It resolves exact card names against its compiled registry and
uses the existing policy V5 session core. The catalog-only V5/V6 interface
in `kernel_rl_env` retains its existing request schemas and deck resolution.
The default schema-1 process retains this behavior. The separate opt-in
schema-2 priority mode is documented in `limited_priority_windows_v1.md`.

## Request and response

Each UTF-8 line contains one request. A reset supplies two mainboards, with
positive counts, at least 40 cards per mainboard, and a 10,000-card process
input bound. More than four copies and more than 40 cards are accepted.
Sideboards are excluded by the adapter; the engine reset schema accepts only
mainboards. Tokens and cards without full engine capability are refused.
Both decks resolve before an active session can be replaced.

```json
{"request_type":"reset","schema_version":1,"request_id":"reset-0","decks":[{"cards":[{"name":"Forest","count":40}]},{"cards":[{"name":"Island","count":40}]}],"episode_id":0,"env_seed":123,"max_physical_decisions":4096,"max_policy_steps":8192}
```

Replies identify `protocol: kernel_limited_jsonl`, outer `schema_version: 1`,
request id, kernel version and card database hash. `response_type` is
`decision`, `terminal` or `error`. The `decision` and `terminal` payloads use
the existing policy V5 types, so their nested schema version is 5.
Decisions contain redacted observations and ordered legal actions.

A step supplies `schema_version`, `request_id`, `episode_id`, `expected_step`,
`selected_index` and `selected_action_id`, plus `request_type: step`.
The current episode, step and selected action's stable id are checked by the
existing session core before applying the action. Failed validation preserves
the active game. Duplicate JSON keys, unknown fields and mismatched versions
are refused. The line-size bound is 8 MiB.

Each custom deck's identity is SHA-256 over a version domain, compiled card
database hash, expanded array length and ordered u16 card ids. Equivalent row
chunking has the same identity; changing contents, order or registry changes
it. Responses also carry the existing FNV-1a card-array hashes. Immediate
request retries return the same bytes; changing a payload while reusing its
immediate request id is refused.

## External adapter and smoke

`python/tools/limited_session_v1.py` offers `LimitedClientV1.reset` and `step`,
using the first milestone's `.dck` importer. It sends names/counts, verifies
the response protocol and action identities, and closes its owned process on
transport errors or timeouts. Engine validation errors preserve the client
and its last successful decision. No failed request is silently retried.

```powershell
cargo build --locked -p mtg-kernel --bin kernel_limited_env
py -3.11 python/tools/limited_session_v1.py --binary target/debug/kernel_limited_env.exe --deck data/limited/smoke_v1/forest40.dck --deck data/limited/smoke_v1/island40.dck --seed 123
```

The smoke chooses Pass when available and the first legal choice otherwise.
It must reach a natural terminal, and reports the full request/response
transcript SHA-256 and terminal counts. The two land-only fixtures test deck
plumbing, shuffle/draw, choices, terminal reporting and replay determinism.
They are not FDN gameplay or playing-strength evidence.

The current interface still uses unconditional seven-card opening hands and
policy V5's suppressed priority windows. Mulligans, complete Limited combat,
planeswalkers, the FDN cards and fair unknown-deck sampling remain subsequent
milestones. No existing trainer or evaluation launcher is wired to this binary.

## Verification

Rust integration tests cover content identity, count/order rules, token and
unknown-card refusal, failed-second-seat and invalid-step nonmutation, strict
wire decoding, legacy-protocol separation, immediate retries, and a complete
same-seed subprocess game run twice with byte-identical transcripts. Python
tests cover the adapter's reset/step flow, server errors, malformed replies,
timeouts, action bounds and deterministic smoke transcripts.

Build and test placement is HaleysPC, which had no active build/training/eval
processes at preparation. The dedicated checkout and Cargo target belong to
this task. Jack's PC retains the lead's Q6 timing reservation. This is bounded
engineering verification with no GPU use or formal measurement.
