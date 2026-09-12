# Sideboarding dataset preparation

The completed engineering-003 expansion now contains 74 examples / 622 action
targets across all 25 ordered five-deck matchup cells. Dataset-development-002
has 56 training and 18 imitation holdout examples, with no shared match,
teacher-plan or duplicate-example groups. The historical 46-example preparation
below remains preserved as dataset-development-001. See the
[completed result](learned_sideboarding_engineering_003_20260912.md) for coverage
and limits, including 24 Done-only examples and no Burn/Rally holdout coverage.

`python/mtg_kernel_rl/sideboard_dataset_v1.py` audits completed static-teacher
batches and produces a deterministic development split. It never trains, runs
games, or infers value labels from winners. The split is an imitation diagnostic,
not a BO3 strength evaluation or a set of frozen promotion gates.

The utility checks the aggregate JSONL and provenance SHA256 pins, verifies their
source inputs, and joins each example to its actual per-match acting-seat record.
It checks the original teacher table, registered 75, legal movement trace,
preceding game's submitted mainboard and next game's target mainboard. Game 3
examples therefore retain their actual starting configuration. It rejects missing
teacher rows and any non-null value label. These checks establish consistency of
pinned local artifacts, not independent authentication of their provenance.
Each match's observation contract must also match the batch's V2/V3 mode and,
for V3, the provenance destination and both feature digests. A stale V2 match
cannot inherit a V3 identity merely by being placed in that batch directory.

The inventory records deck and registration pairs, seed/match groups, source
teacher row hashes, semantic teacher-plan groups, play-weight and feature
identities, and per-record output assignments. Opponent registrations and matchup
ids remain in the inventory only. The model-facing JSONL rows retain their
original contents, without added metadata.

Three relationships join examples into indivisible connected components:

- Same ordered registration pair, seed and initial chooser. Rerunning under
  different limits, play weights or feature versions does not create a new group.
- Same own registered configuration and target 60/15 configuration, even when
  opponent, game number, teacher prose or initial postboard configuration differ.
- Identical example contents.

Components are ranked by SHA256 of the split seed and design identities. Their
ranking does not include outcome-bearing example hashes. Mirror components and
components containing cross-deck pairs are separate strata. Each stratum with
at least two components contributes `ceil(fraction * component_count)` holdout
components, capped to retain a training component. A singleton stratum remains
training-only. An unsplittable dataset is reported explicitly; the utility never
cuts a component to manufacture a holdout or an exact example ratio.

Run from the checkout, with absolute input/output paths and a fresh output
directory whose parent exists:

```powershell
$env:PYTHONPATH = 'python'
$env:PYTHONDONTWRITEBYTECODE = '1'
C:/Python314/python.exe -m mtg_kernel_rl.sideboard_dataset_v1 `
  --batch E:/path/to/completed-batch-a `
  --batch E:/path/to/completed-batch-b `
  --registry E:/path/to/checkout/data/cards_v1.json `
  --pool E:/path/to/checkout/data/pauper_pool_v1.json `
  --output E:/path/to/fresh-development-dataset `
  --split-seed 2026091301 --eval-fraction 0.2 `
  --requested-deck Rally --requested-deck Affinity --requested-deck Elves `
  --requested-deck Terror --requested-deck Burn
```

Outputs are `inventory.json`, `all-examples.jsonl`, `train.jsonl` and
`imitation_eval.jsonl`. Every output is created exclusively. The inventory pins
the utility source, runtime, input artifacts and output JSONL bytes. Test with:

```powershell
C:/Python314/python.exe -m unittest discover -s python/tests -p test_sideboard_dataset_v1.py -v
```

Use repeated `--requested-deck` flags to declare the complete coverage universe.
The inventory then includes zero-count cells and entirely absent decks such as
Burn in the original 46-row collection. It rejects unknown registrations,
duplicates, and a universe excluding an observed deck. Without this option,
`coverage_universe.mode` explicitly says `observed_decks_only`; that matrix must
not be read as coverage of every intended project deck.

The September 12 engineering-003 preparation uses the two completed final
engineering-002 static batches, not earlier failed or cancelled attempts. The
actual records contain 46 examples, 346 action targets, 14 matches and 14 semantic
teacher plans. They form nine components. There are 28 game-2 examples and 18
game-3 examples; all 18 game-3 traces are `Done` only. This makes aggregate action
agreement an incomplete diagnostic: report game-2/movement behavior separately.

With seed 2026091301 and fraction 0.2, the proposed split has 34 training examples
(282 actions) and 12 holdout examples (64 actions). The holdout components are
Affinity/Elves in both directions and Terror/Terror. That is only two independent
holdout components, and it does not establish unseen-deck generalization.

The preparation script and pinned configs are under
`C:/Users/Jack/IdeaProjects/sideboarding-integration-20260912/engineering-003`.
The audited JSONL and inventory are under
`E:/mtg-kernel-learned-sideboarding-evidence/engineering-003/dataset-development-001`.
The script validates 22 existing fact-checked game-2/game-3 teacher rows covering
Terror/Elves in both directions and all nine ordered cells involving Burn. It
compares source names to exact registry ids and original registrations to the
active pool, checks copy limits and full engine flags, and verifies legal 60/15
targets with unchanged 75. No missing-plan fallback or card substitution is used.
These checks do not establish complete runtime mechanic coverage or teacher
expertise. The two new batch configs are prepared but unexecuted.

Before training, choose initialization, loss weighting, budgets and any stopping
rule through the project's independent review process. In particular, continuing
a checkpoint previously trained on a held-out teacher plan would contaminate
that imitation holdout; audit prior training data or initialize the sideboard
head fresh. The frozen play model remains separately pretrained. These inspected
development records are not a secret final test set. After a split is accepted,
freeze and extend assignments deliberately; re-running the utility after adding
data can move components or merge earlier train/eval groups.

The current training CLI reports training-set fit; this utility does not add a
read-only checkpoint evaluator. A subsequent evaluator must keep weights fixed,
verify the split assignments, report grouped imitation diagnostics, and separate
them from genuinely new BO3 games. Playing-strength evaluation still needs an
independently reviewed matched-seed design with no-change/static/learned controls,
appropriate matchup weights and integer/statistical gates. Brewing over new
registered 75s remains a separate search layer and objective.
