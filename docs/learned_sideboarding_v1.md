# Learned sideboarding research workflow

`learned_sideboard_v1` runs a frozen scalar Net8 play policy with an independent
sideboard model between games. It supports cross-deck BO3 matches, explicit
static teaching tables, and supervised imitation training. These are research
tools. Training loss and completed matches do not establish better playing
strength, BO3 promotion, MTGO readiness, or a trained brewing model.
Deck registration alone does not establish frozen-policy compatibility; the
known mechanic gaps below can still stop a batch.

The play model receives the existing actor-relative flat observation. The
sideboard model receives its own registered 75, its own completed-game card
outcomes, public opponent evidence, available resource summaries, game number,
and match score. The runner does not pass the opponent's registration or private
card outcomes to the learned sideboard model. Static teacher tables may use a
declared matchup key, bound by the caller before the policy is invoked.

## Build and invoke

From this checkout:

```powershell
cargo build --locked -p mtg-kernel --bin learned_sideboard_v1
& "$env:CARGO_TARGET_DIR/debug/learned_sideboard_v1.exe" --config C:/path/to/config.json
```

With Cargo's default target directory, the executable is
`target/debug/learned_sideboard_v1.exe`. Release builds use the `release` directory.
Every configured file and output path must be absolute. Configs reject unknown
fields. The output directory must not exist; its parent must already exist.
The program does not resume or replace an earlier run.

## Verify the actual play export

This concrete config uses the prepared engineering import. Its source is replica
0's ordinary-RL generation-2304 export, chosen as an engineering input. It is not
a checkpoint selected using CP7 outcomes.

```json
{
  "mode": "validate_import",
  "play_import": "C:/Users/Jack/IdeaProjects/sideboarding-integration-20260912/engineering-001/play-policy-import.json",
  "output_directory": "C:/Users/Jack/IdeaProjects/sideboarding-integration-20260912/validate-cli-001"
}
```

Validation loads the parameter export but does not run inference or training.
`play-transfer.json` records the export metadata and parameter hashes, original
run and generation, source and destination registry hashes, preserved card count,
and changed card database identity. The loader checks the original registry
entries at their existing ids, allowing deck-membership changes and appended
cards. Other changes to an existing registry entry are rejected. Feature,
architecture, parameter-layout, finite-value, padding-row, and named-weight
checks remain active. The source registry snapshot must name the exporter's
exact Git commit and match its independent file pin.

The original Store is not altered or granted new continuation authority. The
reader verifies the exported bytes and source-run hash; it does not repeat the
exporter's full Store-chain validation. Card embedding token zero remains the
zero padding row, and card id `n` uses row `n + 1`. Appended rows are preserved
exactly; their presence is not evidence that the play policy trained on them.

## Run a BO3 batch

This baseline keeps each seat's current configuration between games:

```json
{
  "mode": "run_batch",
  "play_import": "C:/Users/Jack/IdeaProjects/sideboarding-integration-20260912/engineering-001/play-policy-import.json",
  "output_directory": "C:/Users/Jack/IdeaProjects/sideboarding-integration-20260912/keep-batch-001",
  "policies": [{ "kind": "keep" }, { "kind": "keep" }],
  "matches": [
    {
      "deck_ids": ["Rally", "Burn"],
      "seed": 19001,
      "game_one_chooser": 0,
      "max_physical_games": 8,
      "max_physical_decisions": 1024,
      "max_policy_steps": 2048
    }
  ],
  "collect_static_imitation_examples": false
}
```

Both seats use the frozen play checkpoint with separate deterministic sampling
streams. Every game resets the scorer cache and both streams. Sideboard policies
have distinct seat instances. The match runner currently always chooses to play
when a seat is the play/draw chooser. Drawn games can extend the physical match;
reaching the physical-game cap produces an error rather than a fabricated match
reward. Any session or scorer failure aborts that match.

Use a saved sideboard model by replacing one seat policy with:

```json
{
  "kind": "learned",
  "checkpoint": {
    "path": "C:/path/to/sideboard-checkpoint.json",
    "sha256": "REPLACE_WITH_ACTUAL_FILE_SHA256"
  }
}
```

Both seat policies validate their frozen play identity and embedding-table hash
before the batch's first game. An incompatible learned checkpoint aborts during
that preflight, without playing game 1. The model emits
legal card movements followed by `Done`; the runner replays that trace and
checks the resulting 60/15 configuration. It never repairs an invalid selected
index with modulo or silently falls back to keeping the deck.

## Use explicit static teaching rows

The table format matches the harness's warm-start row shape:

```json
{
  "rows": [
    { "self_deck_id": "Rally", "opponent_deck_id": "Burn", "game_index": 2, "cards_in": [], "cards_out": [] },
    { "self_deck_id": "Rally", "opponent_deck_id": "Burn", "game_index": 3, "cards_in": [], "cards_out": [] }
  ]
}
```

Those example rows deliberately request no exchange. Actual exchange lists
contain objects such as `{"card_id": 95, "count": 1}` with ids from the pinned
destination registry. The row must be legal for that deck's registered 75.

Select the table for a seat with:

```json
{
  "kind": "static_plan_rows",
  "table": { "path": "C:/path/to/teacher-rows.json", "sha256": "REPLACE_WITH_ACTUAL_FILE_SHA256" },
  "teacher_provenance": {
    "source_kind": "hand_authored_warm_start",
    "description": "Explicit draft teaching table; not ratified search data.",
    "artifact": { "path": "C:/path/to/teacher-source-note.md", "sha256": "REPLACE_WITH_ACTUAL_FILE_SHA256" }
  },
  "carry_game_three_forward": true
}
```

There must be exactly one matching row for every possible requested postboard
game. With `carry_game_three_forward: true`, physical games 4 and later reuse
the game-3 row. With `false`, each later game needs its own row. A missing or
duplicate row is an error checked before the batch's first game, including when
the intended plan is to keep the registration.

Table plans specify a target relative to the registered configuration. At each
sideboard boundary the driver computes the legal net change from the actual
current configuration to that target. Game 3 therefore does not repeat stale
game-2 removals.

Set `collect_static_imitation_examples` to `true` to collect teaching examples
from completed matches. Only decisions made by static teacher seats are included.
Each example contains the visible input, actual initial 60/15 configuration, and
the action trace that was applied. `target_value` is always `null`: winning one
match is not a measured improvement over another plan. The output includes
per-match example files, `imitation-examples.jsonl`, and a provenance document
binding those examples to the exact table, policy specifications, input hashes,
match configurations, and play export.

## Train the separate sideboard model

Use the collected JSONL or another independently pinned set of
`SideboardImitationExampleV1` records. The following config gives the exact field
shape; replace the paths and SHA placeholders with actual generated artifacts:

```json
{
  "mode": "train_imitation",
  "play_import": "C:/Users/Jack/IdeaProjects/sideboarding-integration-20260912/engineering-001/play-policy-import.json",
  "output_directory": "C:/Users/Jack/IdeaProjects/sideboarding-integration-20260912/imitation-001",
  "examples": { "path": "C:/path/to/imitation-examples.jsonl", "sha256": "REPLACE_WITH_ACTUAL_FILE_SHA256" },
  "teacher_provenance": {
    "source_kind": "hand_authored_warm_start",
    "description": "Completed static teacher traces from the declared engineering batch.",
    "artifact": { "path": "C:/path/to/imitation-examples.provenance.json", "sha256": "REPLACE_WITH_ACTUAL_FILE_SHA256" }
  },
  "measured_value_provenance": null,
  "initialization": { "kind": "fresh", "seed": 41001 },
  "training": { "epochs": 4, "learning_rate": 0.003, "value_loss_weight": 0.0 }
}
```

To continue an existing sideboard checkpoint, use
`"initialization": {"kind":"checkpoint","checkpoint":{"path":"C:/path/to/sideboard-checkpoint.json","sha256":"REPLACE_WITH_ACTUAL_FILE_SHA256"}}`.
The sideboard head learns by supervised imitation. Play weights and the copied
embedding table remain immutable. This command is not an RL-refinement run.

An example may supply `target_value` only as a measured BO3 win-rate difference
in `[-1, 1]`. Any non-null value requires a pinned `measured_value_provenance`
artifact. The CLI records this supplied provenance; it does not certify the
underlying experiment. Missing labels remain absent rather than becoming zero.

Training writes `sideboard-checkpoint.json`, `training-metrics.json`, and a
completion receipt. Reported loss and action accuracy describe the supplied
training set. Evaluate on separately declared held-out seeds and matchups before
making playing-strength claims.

## Known frozen-input blocker: Map Token Explore

The real learned/keep replay of Affinity versus Affinity, match seed
`2026091214`, stops in game 2 at environment seed `11157630480151790292`,
policy step 36, acting player 0, with two legal actions and
`Action(InvalidActionReference)`. The preceding three requested matches in
`learned-two-deck-001` completed. This is an incomplete batch.

The source is Map Token, card id 147. Activating the Map sacrifices the token;
token cleanup removes it from the graveyard while its activated ability remains
on the stack. Exploring a nonland creates the two-option
`PendingEffectChoice::ChooseOption` with purpose `ExploreNonlandTop`. The rules
engine accepts either choice. Its action semantic uses the source's current
stable reference, which says Graveyard, and the V2 action encoder requires that
reference to occupy a live graveyard position. A ceased token has no such
position. The encoder correctly rejects that unsupported representation.

The existing contracts cannot be repaired by inventing a graveyard position:

- `mtg-kernel/src/rl_session.rs`, `flat_visible_action_object_components_v1`,
  requires current incarnation fields and live zone membership. Its action
  object groups have no historical ability-source group.
- `mtg-kernel/src/rl.rs`, `pending_effect_semantic_v4` and `stack_source_ref`,
  preserve the publicly revealed, frozen ability-source incarnation, while
  effect-choice actions currently call `card_ref` on the live arena object.
- `python/mtg_kernel_rl/features.py`, `_validate_engine_context`, requires
  `pending_effect.source` to have zone Stack, match the controller, and equal the
  top resolving stack source. That assumption excludes a sacrificed token's
  historical ability source. The public-stack cross-check also requires the
  exact same stable source key.
- `mtg-kernel/src/flat_policy_v2.rs` projects action source rows by group, zone,
  and incarnation. Its existing detached-source exception is for Stack action
  rows, not a deceased token reclassified as a live graveyard card.

A successor must distinguish the public resolving stack item from its historical
card source; bind effect-choice action sources to that validated frozen source;
provide an explicit historical-source action mapping and scorer-visible context
row; and update the Python and Rust validation, crosswalks, versions, and digests
together. It must preserve current meanings for live sources and reject forged
source incarnations and hidden references. Reusing frozen Net8 weights then
requires an explicit source/destination feature-transfer receipt, with unchanged
tensor dimensions verified. It must not claim the original feature identity is
unchanged. The present loader continues to enforce the original contract.

The bounded regression
`rl_session::tests::flat_v2_rejects_valid_map_explore_after_source_token_ceases`
constructs a real Map activation, reaches the nonland choice, proves both engine
actions are accepted, and asserts that the current V2 encoder rejects its absent
source. Passing this test records a known limitation, not model support. The
existing rules test is
`mtg-kernel/tests/affinity_wildfire_value.rs::map_explore_handles_land_nonland_and_empty_libraries`.

Exact engineering reproduction config:
`C:/Users/Jack/IdeaProjects/sideboarding-integration-20260912/engineering-001/diagnose-affinity-002.json`.
Its failure is retained under
`E:/mtg-kernel-learned-sideboarding-evidence/engineering-001/diagnose-affinity-002/failure.json`,
with the short identifying diagnostic in the sibling C-drive
`diagnose-affinity-002.log`. The temporary reference diagnostics were removed
after identifying the Map; ordinary failures retain only bounded decision
metadata. A rerun needs a fresh output directory. No candidate or action is
silently removed to get past this blocker.

## Other frozen-input blockers

**Escape:** Terror registers Sleep of the Dead. In
`mtg-kernel/src/rl.rs::build_policy_observation_v5`, a staged Escape cast
explicitly fails with `policy schema v5 cannot represent a staged Escape
graveyard-exile prefix; a versioned object-cost successor is required`.
`pending_cast_semantic_v2` preserves `sacrifice_chosen` as sacrifice-only; it
does not relabel Escape's already selected graveyard-exile cards as sacrifices.
Candidate exclusion cannot replace that missing state in the value input.
The rules regression
`mtg-kernel/tests/sleep_of_the_dead.rs::escape_choices_round_trip_and_commit_exact_provenance`
proves the existing engine continuation and intentional V5 rejection. A
successor needs an explicit typed object-cost kind, selected-card prefix, and
remaining count in the public policy and value features. This is a separate gap
from the corrected hand-activation zone check.

`mtg-kernel/examples/sideboard_terror_probe_v1.rs` reproduced this exact Escape
halt for Terror versus Terror, environment seed 1, the declared SplitMix policy
seed `1 ^ 0x123456789abcdef0`, and limits 2,000 physical decisions / 200,000
policy steps. The flat scorer reaches the same guard through
`observe_policy_v5_unhashed_for_flat_policy`; the earlier real CLI Terror batch
stopped at the separate activation-zone problem.

**Library search:** the retry after that zone correction reaches
`Action(HiddenActionReference)` in Rally versus Elves, match seed
`2026091230`, game 1, environment seed `5920882550572668060`, step 18,
acting player 1, with 11 legal actions. Evidence is
`E:/mtg-kernel-learned-sideboarding-evidence/engineering-001/static-teacher-elves-002/failure.json`;
the pinned config is
`C:/Users/Jack/IdeaProjects/sideboarding-integration-20260912/engineering-001/run-static-teacher-elves-002.json`.
V5's `pending_effect_semantic_v4` already grants only the chooser temporary
access to search candidates. Flat V2's
`rl_session.rs::flat_visible_action_object_components_v1` instead requires
persistent `library_knowledge` and binds each candidate to its true library
position. Installing those entries to pass the guard would reveal hidden order
and retain knowledge beyond the search prompt. The current flat object table
also has no decision-local search group, so bypassing the first guard is
insufficient. The existing rules test
`mtg-kernel/tests/lorien_revealed.rs::search_candidates_are_physical_sorted_private_stable_and_finish_last`
covers the chooser-only search behavior; Forestcycling is the likely trigger
for this Elves replay, not a separately identified card in its failure record.

The next integration lane therefore needs a successor to the rich V5 policy
contract plus a versioned flat V3 mapping: unordered search objects valid only
for the current chooser and decision, historical public ability sources, and
the explicit Escape cost prefix. Library permutations must not expose absolute
positions; the search identities must expire at the decision boundary. Match
and training records must pin the new feature hash. Existing play checkpoints
may supply an explicitly documented warm start after shape and mapping checks;
their original schema and feature identity must remain part of the provenance.
No such migration or frozen-contract relaxation is implemented here.

## Outputs and bounds

The run records exact config/import/input hashes, source/export/checkpoint
identities, compiled integration-source hashes, binary hash, checked-in tag-file
hash, and embedding-table hash. Each completed match is published independently
as `match-NNNNNN.json`. Publication syncs a complete staging file and creates an
exclusive hard link in the same directory; existing results are never replaced.
This requires a filesystem with hard-link support, including ordinary NTFS.
It protects complete files against process interruption, without claiming
portable power-loss durability.

A failed run retains already completed match files and writes `failure.json`
when the failure reaches the execution handler. `completion.json` exists only
after the requested operation completes. Staging files can remain after a
process crash and are not treated as completed results.

CLI limits: 1 to 1,024 matches per batch; 3 to 32 physical games per match;
1 to 10,000 physical decisions and 1 to 1,000,000 policy steps per game;
16 MiB for config/table/checkpoint JSON; 128 MiB and 100,000 records for an
imitation dataset. These are implementation bounds, not recommended measurement
budgets. The sampler retains its existing 64-action bound and fails explicitly
if an observation exceeds it.

Training metrics describe fit to the supplied teacher traces only. A weak or
unchanged fit remains a weak or unchanged fit; writing a checkpoint does not
establish useful sideboarding. Completed engineering matches validate those
paths and seeds. They do not establish held-out win-rate improvement or support
for every card in a deck. Failed batches retain completed files and require a
fresh directory for a new run.

If collected examples exceed 128 MiB, the batch stops after preserving the
completed match and its per-match examples, before starting another match.
