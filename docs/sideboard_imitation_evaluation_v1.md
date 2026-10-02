# Read-only sideboard imitation evaluation

`learned_sideboard_v1 --config ABSOLUTE_CONFIG.json` accepts a fixed-checkpoint
`evaluate_imitation` command. It never calls training, changes an optimizer,
selects checkpoints or runs games. Its output directory must be fresh, while
the checkpoint, inventory, split files and original teaching artifacts stay
read-only.

```json
{
  "mode": "evaluate_imitation",
  "play_import": "E:/path/to/play-policy-import.json",
  "output_directory": "E:/path/to/fresh-evaluation",
  "checkpoint": {
    "path": "E:/path/to/sideboard-checkpoint.json",
    "sha256": "EXACT_INPUT_FILE_SHA256"
  },
  "dataset_inventory": {
    "path": "E:/path/to/dataset-development-002/inventory.json",
    "sha256": "EXACT_INVENTORY_FILE_SHA256"
  },
  "split": "imitation_eval"
}
```

The only supported splits are `train` and `imitation_eval`. Training-split
evaluation is descriptive fit diagnostics. The holdout command has no learning
rate, epoch count, early stopping or checkpoint-selection option. Its immutable
model argument and before/after checkpoint state hashes verify that evaluation
does not train. Original checkpoint bytes are reread and checked before success.

The evaluator validates every explicit inventory input/output SHA256 and byte
count, the inventory's complete record/component membership, same-match,
same-teacher-plan and duplicate-example grouping, and both ordered split files.
Every example rejoins its exact original per-match source row. Matchup, seat,
seed and play-weight metadata rejoin the pinned source match. Actual registered
75 and target 60/15 configurations reproduce the declared teacher-plan group.
Invalid or nonterminating teacher actions and any value labels reject. These
checks establish consistency of pinned local files, not independent source
authentication or an audit of a checkpoint's previous teacher exposure.

`evaluation-examples.json` preserves record/component identities, teacher-state
greedy choices, free-running actions, target/predicted final card counts and
errors. Opponent deck labels and grouping metadata are used only for reporting,
never added to model inputs. The policy receives the existing visible-only
example contract.

`evaluation-metrics.json` reports separate measurements:

| Measurement | Definition |
| --- | --- |
| Teacher-forced action agreement | Greedy predicted action at each teacher-replayed state; action-weighted |
| Teacher-forced movement/Done agreement | Separate numerator and denominator for card movement actions and Done |
| Greedy configuration accuracy | Exact final mainboard and sideboard card-count multisets from an autonomous rollout starting at the actual initial configuration |
| Greedy exchange accuracy | Exact removed/added card-count multisets relative to that initial configuration, independent of movement order |
| Exchange-card precision/recall | Micro counts of correctly removed/added copies; an exchange of one card counts two directed movements |
| Legal termination | Predicted actions replay legally, reach Done and submit an unchanged registered 75 split into 60/15 within the existing decision cap |

Greedy failures remain in example-level accuracy denominators. Zero denominators
have null rates. Exact action-sequence accuracy is additionally available and
must not replace exchange/configuration accuracy. Exchange and configuration
equality coincide for fixed initial configurations, but the directed card counts
also describe partial agreement.

Every metric is stratified by Done-only versus movement teaching, game number,
connected component, matchup, match group and teacher-plan group. The report
states the number of independent connected groups. These are descriptive
development diagnostics without confidence intervals or playing-strength gates.
Dataset-development-002 has only three holdout components and no Burn/Rally
holdout coverage; 24 of all 74 examples teach Done only. A fresh fit on its
56 training examples prevents prior sideboard-head teacher exposure, while
the play embeddings remain separately pretrained. Neither imitation agreement
nor legality demonstrates BO3 strength, reinforcement learning or brewing.

The existing `train_imitation`, `run_batch` and `validate_import` command schemas
and behavior are preserved. Evaluation provenance additionally records the
compiled evaluator source hash.
