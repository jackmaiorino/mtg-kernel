# Rules fix: Undercity room abilities target from the colorless dungeon (v1)

Branch `claude/rules-fix-room-ability-colorless-source` from `main` 54725398.
Not merged; the lane owner (Codex) and Jack decide.

## Root cause

Initiative and Undercity triggers record Avenging Hunter's source contract as
the designation's provenance (`event::log_initiative_trigger` requires it).
Three sites used that mono-green contract as the targeting source:
`drain_pending_triggers_or_decide` (the `ChooseTargets` set),
`apply_choose_target`'s trigger branch, and `stack_targets_still_legal` (the
608.2b recheck). `legal_targets_for_controller_from_source` removes creatures
with protection from monocolored for a monocolored source, so Forge ("two
+1/+1 counters on target creature") and Arena ("goad target creature") could
never target Guardian of the Guildpact. Beside other creatures it was silently
missing; alone, it left a `ChooseTargets` with no legal action (the RL session
halts fail-closed; on the port branch the trigger would be dropped instead).

A room ability's source is the dungeon card (CR 309.4c), which has no mana
cost or color indicator and so is colorless (105.2c). The initiative's own
triggers have no source (722.2 in the current numbering).

## Fix

`engine.rs`: `triggered_ability_targeting_source` gives every
`EffectOp::ResolveInitiativeTrigger` no targeting source and returns the old
source for every other trigger; the three sites call it. The only
source-dependent filters (protection from monocolored,
`CreatureOtherThanSource`) are vacuous for a colorless source that is not a
permanent, so "no source" is equivalent today. Identity is unchanged:
`ChooseTargets.spell`, `StackItem.source` and `ability_source_contract` stay
the Hunter. `trigger::collect_and_process`'s 603.3d filter and
`validate_pending_trigger` were already source-less, so all five target
checks now agree for room abilities.

## What moves

Only Forge and Arena target decisions with a Guardian on the battlefield. The
Hunter is only in the Elves runtime deck and the Guardian only in CawGates, so
only those two pairings move. The RL projection and the V5 surface copy the
engine's `legal_targets`; observation, the search opponent and the Python
features read protection only as a keyword feature. No golden or pinned test
changed (suite below).

## Census (not a golden)

In-process `RlEpisodeSessionV1`, uniform random choice over the offered
actions (splitmix64; episode 7000 + seed, policy state
`0x5EED_0F5B_0000_0000 ^ seed`, the driver of the trigger-no-legal-target
census), 300 seeds per seat order, 600-decision cap. A room target decision
is one whose target actions name Avenging Hunter as their source.

| Pairing (seat 0 vs 1) | Build | Room target decisions | Guardian offered (decisions / games) | Guardian chosen | Game over / cap / linked-exile halt / refused scan |
|---|---|---|---|---|---|
| Elves vs CawGates | `main` | 440 | 0 / 0 | 0 | 205 / 32 / 10 / 53 |
| Elves vs CawGates | fix | 435 | 77 / 36 | 6 | 209 / 32 / 9 / 50 |
| CawGates vs Elves | `main` | 395 | 0 / 0 | 0 | 202 / 41 / 9 / 48 |
| CawGates vs Elves | fix | 393 | 97 / 48 | 9 | 202 / 42 / 9 / 47 |
| Elves mirror | both | 586 | 0 / 0 | 0 | 212 / 24 / 0 / 64 |

No build had a zero-legal-action halt: the lone-Guardian case did not occur in
these 900 games. The linked-exile halts and refused scan answers are the known
defects fixed on the unmerged `claude/rules-fix-goad-menace-linked-exile`.

Per game (bucket, step counts and a digest of every chosen action's stable
id): every game that differs between the builds had a Guardian offer, 30 of 36
offer games for Elves vs CawGates and 44 of 48 for CawGates vs Elves (a
changed menu can still yield the same pick). The other 270, 256 and 300
(mirror) games are identical.

## Fable cross-examination

Fresh read-only Fable reviewer on 47847512, with a brief covering the root
cause, rules basis, fix, tests, census and six questions. Verdicts: Q1, Q3, Q4
and Q6 accept, Q2 accept with an optional follow-up, Q5 accept with a change
to the merge procedure. Material points and dispositions:

1. The rules reading holds (309.4c and 309.6 name the dungeon card as the
   source, 105.2c makes it colorless, the initiative's triggers have no
   source), and keying on every `ResolveInitiativeTrigger` is harmless because
   only Forge, Arena and Trap carry targets. The behavioral delta is exactly
   the Guardian's admission. **Accepted**, no change.
2. `None` also means "source unknown" in the source-less checks, so the
   overload is real, but no current filter distinguishes it from a colorless
   dungeon, and an explicit dungeon source needs an enum refactor of every
   `Option<TargetingSource>` caller. Do that only when a third
   source-dependent filter lands. **Accepted**: 452fcc32 states the invariant
   in the helper's doc comment; the explicit variant is a follow-up for the
   lane owner.
3. No missed site: the effect leaves check zone and type only, Ward keys on
   the targeting controller, and the RL, V5, flat, tensorizer, Python and
   search consumers never recompute legality. **Accepted**.
4. Nothing in observation, projection or goldens depends on the old
   behavior, and the Hunter must stay the identity source
   (`validate_initiative_trigger_binding` requires it). Pre-existing and
   unchanged: after a combat transfer, the new holder's room decision still
   names the other player's Hunter. **Accepted**; the suite below confirms no
   golden moved.
5. The port's `let trigger_source = pending.source_contract.map(...)` hunk
   auto-merges silently; only the `legal_targets` argument conflicts, so a
   naive resolution keeps a Hunter-sourced 603.3d check. **Accepted**: see
   the merge notes; the resolution was built and tested.
6. Nits: the beside test pins the engine's target order; the new
   `inline_effect` `ok_or` is unreachable; pre-existing, a spell-sourced
   targeted trigger would fail the 608.2b recheck (none exists). **Reasoned
   disagreement** on the order: the file's tests already pin exact engine
   order (Trap, Lost Well) and the order is what the RL projection exposes, so
   a reorder should be a deliberate test update. The `ok_or` stays for
   fail-closed symmetry with the adjacent contract check. The spell-sourced
   case is left to the lane owner.

## Evidence

- Failing first on `main` 54725398, passing after:
  `forge_and_arena_can_target_a_lone_guardian_of_the_guildpact` (Forge offered
  no legal target) and
  `forge_and_arena_offer_guardian_of_the_guildpact_beside_other_creatures`
  (Forge offered the Hunter and Elvish Mystic but not the Guardian), both in
  `tests/final_pool_initiative_v1.rs`, reached through a real venture. The
  contrast `a_monocolored_creature_trigger_still_cannot_target_guardian_of_the_guildpact`
  (Humbling Elder) passes before and after.
- With only the drain and `apply_choose_target` fixed, both new tests still
  failed: the ability fizzled at the 608.2b recheck (no counters).
- Full release suite (`cargo test --release --locked --workspace
  --all-targets --no-fail-fast`, Windows) on the committed tree: 2,281
  passed, 0 failed, 50 ignored across 80 binaries, on both 47847512 and
  452fcc32 (`main`'s 2,278 plus the 3 new tests). No golden or pinned test
  changed.
- `cargo fmt --check` clean. `cargo clippy --release --locked --workspace
  --all-targets -- -D warnings` clean on 452fcc32, also with
  `--features native-training-store-v2-production`.

## Merge notes

- `claude/rules-fix-trigger-no-legal-target` (778ff336) and Codex's Phase 1
  `008379d8` change the same drain lines. Git marks only the `legal_targets`
  argument and silently keeps the port's
  `let trigger_source = pending.source_contract.map(...)`. Resolve by
  defining `trigger_source` with `triggered_ability_targeting_source(
  pending.source, pending.source_contract, &pending.effect)` and passing it
  to both the 603.3d check and `ChooseTargets`. Taking either side of the
  marker alone leaves the 603.3d check on the Hunter, which drops a lone
  Guardian's room ability; the lone-Guardian test expects a target decision
  there. A scratch merge of 452fcc32 and 778ff336 resolved this way passes
  `final_pool_initiative_v1` (13), `caw_gates_completion_v1` (9, with the
  port's two tests) and the port's engine test.
- `claude/rules-fix-goad-menace-linked-exile`: no shared files.
