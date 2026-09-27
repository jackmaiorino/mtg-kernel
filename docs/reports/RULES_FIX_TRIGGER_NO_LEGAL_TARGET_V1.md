# Rules fix: drop a triggered ability with no legal target (v1)

Branch `claude/rules-fix-trigger-no-legal-target` from `main` 54725398: a port
of Codex's Phase 1 fix `008379d8` (`codex/learned-sideboarding-integration-v1`).
Not merged; the lane owner (Codex) and Jack decide.

## Root cause

`trigger::collect_and_process` already removes a trigger with no complete
legal target assignment (603.3d), but it checks without the trigger's source.
`legal_targets_for_controller_from_source` has two source-dependent filters:
protection from monocolored against a monocolored source, and
`CreatureOtherThanSource`. Journey to Nowhere is mono-white and its ETB
trigger uses `CreatureOtherThanSource`. When Guardian of the Guildpact is the
only creature, the source-less check keeps the trigger, and
`drain_pending_triggers_or_decide` then returned `ChooseTargets` with no legal
target and `can_finish: false`. `RlEpisodeSessionV1` halts with
`fail_closed:nonterminal decision produced zero legal actions`.

## Fix

`engine.rs`: before exposing a trigger's target decision, the drain checks
`target_prefix_can_complete_for_controller_and_source` with the trigger's
source and drops the trigger when no completion exists (603.3d), as
`drain_pending_cast_or_decide` already reverts an impossible cast. The code is
`008379d8`'s; its comment cited 603.3c (modal triggers), this one cites 603.3d.

## What moves

Only decisions that had zero legal actions, where every consumer halted. A
trajectory that never reached one is unchanged. No golden or pinned test
changed (full suite below).

## Census (not a golden)

In-process `RlEpisodeSessionV1` CawGates mirrors, uniform random choice over
the offered actions (splitmix64; episode 7000 + seed, policy state
`0x5EED_0F5B_0000_0000 ^ seed`, the driver of the rules-fix branch's mirror
sweep), 300 environment seeds, 600-decision cap. Episodes by how they ended:

| Build | Game over | Decision cap | Linked-exile halt | Zero-legal-actions halt |
|---|---|---|---|---|
| `main` 54725398 | 109 | 118 | 65 | 8 |
| `main` + this fix | 115 | 120 | 65 | 0 |
| rules-fix branch dce845cd | 122 | 168 | 0 | 10 |
| rules-fix branch + this fix | 130 | 170 | 0 | 0 |

Every mirror halt came right after a Journey to Nowhere resolved. The other
eight runtime deck mirrors on `main` had none, and their outcomes and decision
totals are identical before and after the fix (2,400 episodes). The rules-fix
note's census (3 and 4 halts) used different driver constants.

CawGates against each other runtime deck on `main`, both seat orders, same
driver (4,800 episodes). Zero-legal-actions halts, before and after the fix:

| Pairing (seat 0 vs seat 1) | Before | After |
|---|---|---|
| CawGates vs Wildfire | 14 | 0 |
| Wildfire vs CawGates | 8 | 0 |
| Affinity vs CawGates | 7 | 0 |
| Faeries vs CawGates | 6 | 0 |
| Burn vs CawGates, CawGates vs Affinity, CawGates vs Faeries | 3 each | 0 |
| Spy vs CawGates | 2 | 0 |
| CawGates vs Spy, CawGates vs Terror | 1 each | 0 |
| CawGates vs Burn, Elves or Rally; Elves, Rally or Terror vs CawGates | 0 | 0 |

Before the fix, 37 of the 48 followed a Journey to Nowhere cast and 9 a
Humbling Elder cast (Faeries; its monocolored ETB trigger can only target an
opponent's creature). The last two followed floated mana with no cast among
the four recorded picks; both are gone after the fix. The six pairings with
no halt have identical decision totals before and after, so the Elves
initiative case (Fable point 2) did not occur in 600 Elves-CawGates games.

## Fable cross-examination

Fresh read-only Fable reviewer, brief with the port, root cause, census, tests
and five questions. Verdicts: Q1 to Q4 accept, Q5 accept with a change.
Material points and dispositions:

1. The check is false exactly when the old decision had no legal action:
   min-0 specs return true before any search, a chosen prefix is validated
   against the same source-aware set, and every trigger in the pool has one
   target. **Accepted**, no change.
2. Pre-existing: initiative room abilities target with Avenging Hunter
   (mono-green) as their source (`event::log_initiative_trigger` requires
   it), but a room ability's source is the colorless dungeon, so Forge and
   Arena should be able to target a Guardian of the Guildpact. With another
   creature present the Guardian is already silently excluded; with this fix
   a lone Guardian drops the trigger instead of halting. Only Elves against
   CawGates can reach it. **Accepted as a follow-up** (separate rules fix),
   not a blocker; measured in the cross-matchup census above.
3. Nothing pins, replays, or tolerates an empty trigger `ChooseTargets`
   (RL projections, surfaces, search opponent, goldens). The search opponent
   now continues CawGates lookaheads it used to abandon. **Accepted**.
4. Keep the collection filter source-less: 603.3b ordering precedes 603.3d
   removal, so the drain is the faithful checkpoint. Cost: an `OrderTriggers`
   holding a doomed trigger offers equivalent permutations. **Accepted**;
   follow-up for the lane owner: one completability helper taking
   `&PendingTrigger` for both sites so they cannot drift.
5. Keep 603.3d (603.3c is the modal rule; `trigger.rs` already cites 603.3d
   here). The test hunk differs from `008379d8` anyway. **Accepted**: Codex
   can adopt this comment and test text on the Phase 1 branch so the hunks
   merge clean.
6. Cross-matchups were not censused. **Accepted**: census above.

## Evidence

- Failing first on `main` 54725398, each for the stated reason, passing
  after: `engine::tests::journey_to_nowhere_etb_trigger_is_dropped_when_its_only_possible_target_has_protection_from_monocolored`
  (ported; `ChooseTargets` with no legal target),
  `caw_gates_completion_v1::journey_trigger_without_a_legal_target_is_removed_after_the_cast_resolves`
  (the same decision after a real cast) and
  `caw_gates_completion_v1::cawgates_mirror_census_seeds_never_halt_on_a_decision_without_legal_actions`
  (main's 8 census seeds halted).
- Full release suite on 778ff336 (Windows): 2,281 passed, 0 failed, 50
  ignored across 80 binaries (2,278 on `main` per the rules-fix note, plus
  the 3 new tests). A first run under heavy machine load failed only
  `snapshot::tests::snapshot_clone_cost_is_bounded`, a wall-clock budget
  (85 µs per call against 40 µs; about 10 µs in three isolated runs) in code
  this change does not touch; the rerun passed.
- `cargo fmt --check` clean; clippy `-D warnings` clean for the workspace and
  the native-store feature. The CUDA feature clippy and the feature test
  steps were not run (no feature-gated code changed).

## Merge notes

- Phase 1 branch: same code as `008379d8`; the comment and the ported test's
  doc comment differ, a textual conflict when the branches meet unless the
  Phase 1 branch adopts this text. `008379d8`'s real-game test
  (`expanded_deck_training_v1`) needs Phase 1-only infrastructure and is not
  ported.
- Rules-fix branch (`claude/rules-fix-goad-menace-linked-exile`): no shared
  hunks. The new `caw_gates_completion_v1.rs` tests sit away from its
  insertion, and the session test keeps its own `splitmix64` out of
  `tests/rl_session.rs`, where that branch adds one.
