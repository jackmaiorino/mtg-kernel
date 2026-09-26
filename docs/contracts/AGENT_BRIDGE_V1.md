# Agent Bridge v1: Spellbench `spellbench/v1` Environment Role

Status: ACTIVE wire contract for the mtg-kernel native Spellbench adapter.
The cross-engine protocol authority is the external
`spellbench/spec/SPELLBENCH_PROTOCOL_V1.md`; this document is the mtg-kernel
adapter contract. Any conflict with the protocol authority is a bug in this
adapter. The adapter is purely additive: it changes no `kernel_rl_jsonl`
v5/v6 behavior, no frozen contract, and no roadmap gate.

## 1. Surface

- Module: `mtg-kernel/src/agent_bridge_v1.rs` (`SpellbenchBridgeServerV1`).
- Binary: `mtg-kernel/src/bin/agent_bridge_v1.rs` (`agent_bridge_v1`,
  auto-discovered, not feature-gated). One compact JSON object per stdin
  line produces exactly one compact JSON object per stdout line, in order;
  stderr is diagnostics only; closing stdin exits 0. There is no quit
  message.
- Tests: `mtg-kernel/tests/agent_bridge_v1.rs` (in-process and real-process
  coverage) plus the module's own unit tests.
- Goldens: `data/agent_bridge_v1/` produced by
  `python/tools/generate_agent_bridge_goldens.py` driving the real binary;
  `--check` byte-verifies and runs in CI python-tests shard 0.

## 2. Versioned identities

| Identity | Value |
|---|---|
| `protocol` | `"spellbench/v1"` |
| `engine.name` | `"mtg-kernel"` |
| `engine.version` | `KERNEL_VERSION` (currently `0.0.4-spike`) |
| `engine.source_revision` | env `SPELLBENCH_SOURCE_REVISION`; unset or empty → `null` |
| `engine.rules_snapshot_id` | `mtg-kernel-rules/<KERNEL_VERSION>/carddb-<KERNEL_CARDDB_HASH:016x>` |
| `engine.card_pool_identity` | `mtg-kernel-pauper-pool-v1/carddb-<KERNEL_CARDDB_HASH:016x>` |
| `formats` | `["pauper-bo1"]` |
| `capabilities` | `{"decklists_as_data": false}` |
| `extensions` | `["x_kernel_v5"]` |
| `game_id` → `episode_id` | `fnv1a64-utf8-game-id/v1`: FNV-1a 64 over the game id's UTF-8 bytes |
| object id derivation | `kernel-arena-id-zone-change-count/v1`: `obj-<arena_id:06>-z<zone_change_count:04>` |
| candidates hash | spec Section 4.3 canonical JSON of `[{"candidate_id", "semantic"}]` in decision order, SHA-256 lowercase hex, `display_text` excluded |

`KERNEL_CARDDB_HASH` is the build-time card-registry hash from
`mtg-kernel/src/card_def.rs`; both opaque engine identities derive from it, so
any card-database drift moves both strings. The single-entry retry cache keys
on `request_id` plus the byte-exact request line (line terminators stripped);
a same-id/different-bytes request fails `request_id_reuse_mismatch`, and
requests that fail parsing or decoding are never cached.

## 3. Framing and envelope (spec Section 2/4)

- NDJSON over stdio, UTF-8, `\n` terminator, one tolerated trailing `\r`.
- A line longer than 8 MiB is rejected with `malformed_json` after draining
  through its newline, so framing resynchronizes for the next line.
- Strict parse failures (invalid JSON, duplicate object keys, non-finite
  numbers) and fractional numbers anywhere in the line are `malformed_json`.
- A non-object top level is `malformed_request`. Unknown fields (including
  `x_`-prefixed ones, since requests have no designated extension point) are
  `malformed_request`. A `protocol` string other than `spellbench/v1` is
  `protocol_mismatch`; a missing or non-string `protocol` is
  `malformed_request`.
- Request ids are echoed when the envelope carries a string `request_id`;
  otherwise the error response's `request_id` is `""` (spec Section 4.1:
  every response carries a string `request_id`).
- Protocol numbers are integers; u64/i64-typed fields accept their full typed
  range (the `2^53` bound constrains senders, not this receiver).

## 4. reset

- `format` must be `"pauper-bo1"` exactly, else `unsupported_format`.
- `seats` must be exactly `p0` then `p1`, else `malformed_request`.
- A deck must carry exactly one of `catalog_id` / `decklist` (both or neither
  is `malformed_request`). `decklist` is `unsupported_deck`; this engine
  never substitutes. An unknown `catalog_id` is `unsupported_deck`, and so
  is `Terror`: it stages Escape, which policy surface v5 cannot represent,
  so most of its games would halt and the rest would bias any rating
  (`SPELLBENCH_POLICY_V5_UNPLAYABLE_DECKS_V1`).
- `game_seed` maps to the session `env_seed`; `max_decisions` maps to the
  session's physical-decision cap; `max_steps` maps to the session's
  policy-step cap.
- At most one active game per process: a `reset` while a game is active is
  `game_already_active`; once a terminal was sent the next `reset` starts a
  new game. `game_id` is unique per process: reusing one is
  `malformed_request`.
- The response is the first `decision` (step 0) or a `terminal` for a
  degenerate game (e.g. `max_decisions: 0` truncates immediately).

## 5. decision (response)

- `step` is the kernel policy step count (0 on reset, +1 per answered step).
- `acting_seat` is the kernel acting player.
- `group` is `{group_id: physical_decision_id, substep_index, substep_count}`
  from the kernel decision; the kernel guarantees group ids are dense from 0
  and that a terminal never interrupts a partial group.
- `state_summary` projects `ObservationV5.projection`: `turn`, `phase_step`
  (12-step mapping below), `active_seat`, `priority_seat`, per-seat
  `life`/`hand_count`/`library_count`/`graveyard_count`/`battlefield_count`,
  and `stack_count`. It carries public scalars and counts only.
- `candidates` is nonempty; `candidate_id` is the dense index into the kernel
  legal-action list; `display_text` passes through (currently always `null`).
- `provenance` repeats the Section 2 engine identity minus `source_revision`.
- `extensions.x_kernel_v5` carries the raw `ObservationV5` and
  `LegalActionV5` payloads as JSON text,
  `{"observation_json": "<ObservationV5>", "legal_actions_json": "<[LegalActionV5]>"}`,
  each the kernel's own `serde_json` serialization, so kernel-native models
  can play without a neutral re-encoding. The payloads travel as strings
  because they contain u64 hashes (`card_db_hash`,
  `visible_projection_hash`) beyond the spec's `2^53` integer bound; every
  integer the bridge emits as a JSON number satisfies `|x| <= 2^53`.

### Phase-step mapping (`ZoneIndependentStepV1` → spec Section 7.3)

| Kernel | Neutral |
|---|---|
| `Untap` | `untap` |
| `Upkeep` | `upkeep` |
| `Draw` | `draw` |
| `Main1` | `precombat_main` |
| `BeginCombat` | `beginning_of_combat` |
| `DeclareAttackers` | `declare_attackers` |
| `DeclareBlockers` | `declare_blockers` |
| `CombatDamage` | `combat_damage` |
| `EndCombat` | `end_of_combat` |
| `Main2` | `postcombat_main` |
| `End` | `end_step` |
| `Cleanup` | `cleanup` |

### Object references

`CardStableRefV1` maps to `{object_id, card_name, owner_seat, controller_seat,
zone}` with `object_id` per Section 2, seats `"p0"`/`"p1"`, and `zone` the
lowercase zone name. `card_name` is what the acting seat knows, read from
its own `ObservationV5`: the name the observation shows for every card it
lists (battlefield, graveyards, exile, own hand, known library and hand
cards, so a transformed double-faced card reads as its current face). An
object the observation does not list takes its registry name
(`CARD_DEFS[card_db_id].name`) unless it sits in a hidden zone (a library, or
a hand the observer does not own) and is not a target the pending effect
offers the observer (a search or look), in which case `card_name` is
`null`. A `card_db_id` outside the registry, or an observation that names
one object two ways, is an engine contract failure (Section 8).

Target references map `TargetRefV1::Player` → `{"player": seat}` and
`TargetRefV1::Object` → `{"object": <object reference>}`.

## 6. Neutral candidate semantics (`ActionSemanticV1` → spec Section 6)

The kernel `actor` field is dropped everywhere: the envelope's `acting_seat`
carries it, and the neutral schema allows no other fields. Integer widening
(`u8`/`u16`/`usize` → `u32`) is lossless.

| Kernel variant | Neutral `kind` | Field mapping |
|---|---|---|
| `Pass` | `pass` | (none) |
| `PlayLand` | `play_land` | `source` |
| `CastSpell` | `cast_spell` | `source` |
| `ActivateManaAbility` | `activate_mana_ability` | `source`; `mana_choice`: `"W".."C"` or `null`; `cost_target`: `{"object": ref}` or `null` |
| `ActivateAbility` | `activate_ability` | `source`, `ability_index` |
| `PlotSpell` | `plot_spell` | `source` |
| `ChooseTarget` | `choose_target` | `source`, `remaining`, `target` |
| `ChooseCostTarget` | `choose_cost_target` | `source`, `cost_kind` (snake_case `CostKind`), `remaining`, `candidate` |
| `ChooseCastMode` | `choose_cast_mode` | `source`, `mode`: `"normal"`/`"alternative"` |
| `ChooseKicker` | `choose_kicker` | `source`, `pay` |
| `ChooseSpellMode` | `choose_spell_mode` | `source`, `mode_index`, `mode_count` |
| `ChooseEffectOption` | `choose_option` | `source`, `option_index`, `option_count` |
| `ChooseEffectTarget` | `choose_effect_target` | `source`, `target`, `selected_count`, `min_targets`, `max_targets` |
| `FinishEffectSelection` | `finish_effect_selection` | `source`, `selected_count` |
| `ChooseEffectColor` | `choose_color` | `source`, `color`: `"white".."green"`; `C` is not a color and fails closed |
| `ChooseEffectNumber` | `choose_number` | `source`, `value: number`, `minimum`, `maximum` |
| `ChooseEffectBoolean` | `choose_boolean` | `source`, `value` |
| `FinishTargetSelection` | `finish_target_selection` | `source`, `selected_count` |
| `ChooseOptionalCostUse` | `choose_optional_cost_use` | `use_cost` |
| `ChooseOptionalCostWhich` | `choose_optional_cost_which` | `choice`: `"decline"`/`"discard"`/`"sacrifice_land"` |
| `ChooseSpellCopyPayment` | `choose_spell_copy_payment` | `source`, `pay` |
| `ChooseSpellCopyRetarget` | `choose_spell_copy_retarget` | `source`, `change_target` |
| `ChooseMadnessCast` | `choose_madness_cast` | `card`, `cast_it` |
| `Discard` | `discard` | `cards` (exactly one entry; the kernel H2 reshape guarantees this) |
| `ChooseAttackerInclusion` | `choose_attacker_inclusion` | `attacker`, `include` |
| `ChooseBlockerInclusion` | `choose_blocker_inclusion` | `attacker`, `blocker`, `include` |
| `OrderTriggers` | `order_triggers` | `pending_sources`, `order` |
| `Ambiguous`, `DeclareAttackers`, `DeclareBlockersForAttacker` | (none) | refused: never emitted; an occurrence fails the game as `halted` |

## 7. step

Validation order: `step_before_reset` (no game) → `game_id_mismatch` →
`game_already_terminal` → `expected_step_mismatch` →
`candidate_id_out_of_range` → `semantic_echo_mismatch`. The echo is compared
field-for-field by canonical-JSON equality against the recomputed neutral
semantic of the selected candidate (the neutral mapping is a deterministic
function of the pending kernel decision, which cannot drift between the
decision and its answer). Only after every check does the bridge apply the
selection to the session by dense index plus the kernel `stable_id`, so the
double binding cannot desynchronize. A `step` that passes validation but is
then rejected by the session with an unmappable session error is an engine
contract failure and halts the game (Section 8) with `reason`
`engine_contract_failure:session_step_rejected_after_validation:<code>:<message>`:
the session error code in snake_case, as the kernel's JSONL servers spell it
(for example `stale_environment_binding`), and the kernel's message. Session
step messages are fixed text, so the reason is deterministic for a given
game. The known source is a kernel defect: policy surface v5's combat scans
offer inclusion answers whose final declaration the engine refuses (a
declined goaded attacker, a single blocker for a menace attacker). The bridge
reports the refusal rather than guessing another answer.

## 8. Fail-closed terminal synthesis

A `decision` whose candidates cannot be represented in neutral semantics
(`Ambiguous`, legacy aggregate combat variants, colorless `choose_color`,
multi-card `discard`, unresolvable `card_db_id`) is never partially emitted:
the game ends immediately as `terminal` with `outcome`/`classification`
`halted`, `winner: null`, `reason` an `engine_contract_failure:*` string
naming the cause (a `step` that finds the session already ended reports
`engine_contract_failure:decision_expected_before_step:<session reason>`), and
the game's counts: in answer to a `step`, the answered decision counts as a
step and its physical decision counts once that substep closes its group;
otherwise the session's current `step_count`/`decision_count`. Policy surface v5
already refuses `Ambiguous` and aggregate combat semantics
(`validate_policy_action_semantic_v5`), so this path is defense in depth.
After a terminal response of any origin, further `step` requests are
`game_already_terminal`; a `reset` with a fresh `game_id` starts the next
game.

## 9. terminal (response)

| Kernel `RlSessionTerminalV1` | Neutral |
|---|---|
| `terminal_outcome` | `outcome`: `p0_win`/`p1_win`/`draw`/`truncated`/`halted` |
| `terminal_classification` | `classification`: `natural`/`truncated`/`halted` |
| `winner` | `winner`: `"p0"`/`"p1"`/`null` |
| `terminal_reason` | `reason` (human-facing, passed through) |
| `policy_step_count` | `step_count` |
| `physical_decision_count` | `decision_count` |

## 10. Closed error-code coverage

| Code | Bridge trigger |
|---|---|
| `malformed_json` | invalid/duplicate-key/non-finite/fractional JSON; non-UTF-8 or oversized line |
| `malformed_request` | non-object top level; missing/mistyped/unknown fields; bad `request_type`; bad seat shape; deck with both/neither selector; reused `game_id` |
| `protocol_mismatch` | string `protocol` other than `spellbench/v1` |
| `request_id_reuse_mismatch` | cached `request_id` with different request bytes |
| `step_before_reset` | `step` before any `reset` |
| `game_already_active` | `reset` while a game is active |
| `game_id_mismatch` | `step` naming a different game id |
| `expected_step_mismatch` | `expected_step` ≠ pending decision `step` |
| `candidate_id_out_of_range` | `candidate_id` outside the current candidate list |
| `semantic_echo_mismatch` | echo not field-for-field equal to the recomputed semantic |
| `unsupported_format` | `format` other than `pauper-bo1` |
| `unsupported_deck` | `decklist` decks or unknown `catalog_id` |
| `game_already_terminal` | `step` after any terminal response |
