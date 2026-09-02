# Checkpoint shadow stdio protocol v2

`mtg-kernel-checkpoint-shadow-stdio/v2` adds the kernel's own game clock to the
shadow scorer's decision bodies, and an optional fail-closed rendezvous guard on
`step`. It exists because v1 carries no game clock at all: the Java bridge
aligns XMage callbacks to kernel decisions by menu shape and decision counter
only, so a callback from one turn can be mapped onto a kernel decision from
another without either side noticing. That is the mechanism behind the CP7
panel void of reference pair 43 / episode 86 (seed `46caf355c867fb96`), where a
T12 precombat-main `cast Lightning Bolt` was mapped onto the kernel's leftover
T11 postcombat-main window.

Rust side: `mtg-kernel/src/native_checkpoint_shadow_stdio_v1.rs`.
Binary: `mtg-kernel/src/bin/checkpoint_shadow_stdio_v1.rs`.

## Selecting the version

v1 is the default and is unchanged. v2 is opt-in through one explicit flag:

```
checkpoint_shadow_stdio_v1 --population-store-root ROOT --generation 2048 --protocol v2
```

`--protocol` accepts exactly `v1` or `v2`; any other value, a repeat, or the
flag on its own is a usage error. Omitting it selects v1, so every existing
invocation and every already-built scorer executable keeps its current
behaviour byte for byte.

In-process callers use
`run_checkpoint_shadow_stdio_with_protocol_and_exports_v1(authority, protocol,
teacher_jsonl, outcome_jsonl)`. Every other public runner delegates with
`ShadowStdioProtocolV1::V1`.

## What changes on the wire

Only three things:

1. Every response envelope carries `"protocol": "mtg-kernel-checkpoint-shadow-stdio/v2"`
   and `"schema_version": 2`.
2. Every **decision body** gains a `kernel_clock` object. This covers the
   `reset`, `score_current` and `step` responses, since all three build the same
   decision body.
3. `step` requests may carry an optional `expected_clock` object.

Nothing else moves: request grammar for `reset` and `score_current` is
unchanged, terminal bodies are unchanged, and no existing field is renamed,
reordered or removed.

Under v1 the `kernel_clock` field is omitted entirely (not emitted as `null`),
and a `step` carrying `expected_clock` in any form, object or `null`, is
rejected with the pre-existing `malformed_request` error code, which is exactly
what a v1 service returned for that request before the field existed.

## `kernel_clock` (response, v2 only)

```json
"kernel_clock": {
  "turn": 6,
  "phase_step": "DeclareAttackers",
  "active_player": "p0",
  "priority_player": "p1",
  "stack_depth": 1
}
```

| Field | Type | Meaning |
| --- | --- | --- |
| `turn` | unsigned integer | `GameState::turn`, the kernel's round counter. It advances when the active player comes back around to the starting player, so one kernel turn spans both players' XMage turns. |
| `phase_step` | string | `GameState::step`, the kernel's turn-structure step. |
| `active_player` | `"p0"` or `"p1"` | `GameState::active_player`, whose turn it is. |
| `priority_player` | `"p0"` or `"p1"` | `GameState::priority_player`. For a `surface` (priority) decision this equals the body's `acting_player`; for the `attacker_inclusion` / `blocker_inclusion` kinds the acting player is the declaring player and can differ, so compare against `acting_player`, not against this. |
| `stack_depth` | unsigned integer | `GameState::stack.len()`. |

`phase_step` is one of exactly these twelve values, written out explicitly in
the Rust source so a kernel-side rename cannot silently move the wire contract:

`Untap`, `Upkeep`, `Draw`, `Main1`, `BeginCombat`, `DeclareAttackers`,
`DeclareBlockers`, `CombatDamage`, `EndCombat`, `Main2`, `End`, `Cleanup`

Two mapping notes for the Java side:

- The kernel's decision-visibility surface never surfaces a priority decision at
  `Untap`, `Upkeep`, `Draw`, `BeginCombat`, `CombatDamage`, `EndCombat`, `End`
  or `Cleanup` (`surface.rs`'s `harness_never_offers_priority`). A kernel
  decision therefore always reports one of `Main1`, `DeclareAttackers`,
  `DeclareBlockers` or `Main2`, matching XMage's `PRECOMBAT_MAIN`,
  `DECLARE_ATTACKERS`, `DECLARE_BLOCKERS` and `POSTCOMBAT_MAIN`.
- `turn` is a round counter, not XMage's `game.getTurnNum()`. Compare
  `(turn, active_player)` rather than trying to equate the two numbers.

## `expected_clock` (request, v2 only)

Optional on `step`. When present, the scorer checks it against the kernel clock
of the decision the step is answering, **before** applying anything.

```json
{"request_type":"step","request_id":"cp7-86-17","episode_id":86,
 "expected_step":58,"selected_index":1,
 "expected_clock":{"turn":6,"phase_step":"Main2","active_player":"p1"}}
```

The object has exactly three required fields (`turn`, `phase_step`,
`active_player`), with the same types and spellings as in `kernel_clock`.
Unknown fields inside it are a schema violation (`malformed_request`), so do not
send `priority_player` or `stack_depth` here: they are deliberately not part of
the assertion, because the caller can legitimately be uncertain about the stack
depth at the moment it answers.

**Omit the key entirely to skip the guard. Do not send `"expected_clock": null`.**
Presence is what arms the check, so an explicit null is treated as carrying the
field with an unusable value and is rejected under both versions. If your JSON
writer emits nulls for unset fields (Jackson's and Gson's defaults do), configure
it to omit them instead: a null that was read as absence would silently disarm
the very check this version exists to provide, and v1 has always rejected the key.

Outcomes:

| Case | Result |
| --- | --- |
| key absent | Step proceeds exactly as in v1. |
| present and agrees | Step proceeds. |
| present and disagrees | Response `error_code: "clock_mismatch"`, message `expected_clock does not match the kernel clock of the current decision`. **The session does not advance**: the same decision is still pending and can be re-read with `score_current`. |
| present as `null` | `malformed_request` under both v1 and v2, session unchanged. |
| present under `--protocol v1` | `malformed_request`, session unchanged. |
| present with an unknown inner field | `malformed_request`, session unchanged. |

The guard is checked after `episode_id` and `expected_step` and before the
candidate-seat `selected_index_not_model_choice` check, so a caller that is both
misaligned and passing the wrong index sees `clock_mismatch` first.

`score_current` intentionally does not take an `expected_clock`: it is
side-effect free, and a caller that wants to verify alignment can simply read
`kernel_clock` off the response.

## Recommended Java use

Record the `kernel_clock` from every decision body. Before answering a CP7
priority action or pass, send `expected_clock` built from XMage's own
`game.getTurnNum()`-derived round, `game.getTurnStepType()` and active player.
A `clock_mismatch` then names the first divergence instead of letting the
rendezvous slide silently until an unmappable action voids the pair.

When you do not want the guard on a particular step, leave the key out of the
JSON object rather than writing a null. `XMageRallyBridgeJsonCodec` builds
request lines explicitly, so this is a matter of not adding the property.

Note that `XMageRallyBridgeJsonCodec` rejects unknown response fields, so the
Java client must learn `kernel_clock` before any scorer is launched with
`--protocol v2`. Until then, keep launching without the flag.

## Error codes

v2 adds exactly one code, `clock_mismatch`. Every other code
(`malformed_json`, `malformed_request`, `request_too_large`,
`no_active_session`, `episode_id_mismatch`, `expected_step_mismatch`,
`selected_index_not_model_choice`, `selected_index_out_of_range`,
`episode_already_terminal`, `export_poisoned`, and the session-error
passthroughs) keeps its v1 meaning.
