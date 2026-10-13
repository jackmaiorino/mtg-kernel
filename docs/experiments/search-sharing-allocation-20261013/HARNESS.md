# Native comparison harness

Use the existing census mode `s4a-search-profile` with its checkpoint, bank,
`PROFILE_CASES`, `PROFILE_POSITIONS`, `BUDGET_TOTAL` and `BUDGET_KEY_MODE`
settings. Set `PROFILE_EXPERIMENT=1`. Each job takes one case and exactly one
position, so reconstruction cost belongs to that root. All substantial jobs
must use the qualified guarded launcher owned by the experiment lead.

| Setting | Meaning |
| --- | --- |
| `PROFILE_ARM=A/B/C` | Current UCT, continuation sharing, or root successive halving |
| `PROFILE_SEED` | Fresh matched seed suffix, identical across comparison arms |
| `PROFILE_INTERCHANGE=0/1` | Explicit common interchangeable-choice setting, default 0 |
| `PROFILE_CPU_SECONDS` | Process CPU budget for reconstruction and search; cutoff between simulations |
| `PROFILE_WORK_LIMIT` | Attempted-simulation count for deterministic replay; mutually exclusive with CPU cutoff |
| `PROFILE_EVALUATE=0/1` | Disable fresh evaluation during calibration; default 1 |
| `PROFILE_EVAL_WORLDS` | Fixed scheduled fresh evaluation worlds, default 128 |
| `PROFILE_EVAL_CAP` | Transition cap per fresh continuation, default 65536 |
| `PROFILE_EVAL_CONTINUATION=frozen-tree/plain` | Primary frozen searched continuation (default), or secondary fixed-action/plain continuation |
| `PROFILE_EVAL_ONLY` | Prior selection JSONL for secondary `plain` mode only; reconstruct and validate the root before use |

The selection transition cap remains a safety/work limit. CPU cutoff can
overshoot by one simulation. Replaying the recorded `work_attempts` with the
same transition ceiling reproduces the deterministic primary hash; cutoff
provenance and timings, including sharing cost, are excluded from that hash.
The primary includes a canonical full-tree digest. Zero completed work is a
support failure, not a chosen action. The handler's CPU clock begins before
source reconstruction. The launcher records full process CPU, including
checkpoint loading, startup, final serialization and output writes. Report
both scopes and use full process costs in comparisons.

C covers each nonmerged root edge with eight actual natural terminal backups,
then successively halves the active set and doubles each cumulative quota.
Every tie at the cut survives, so all-zero prefixes preserve exploration.
Within each phase the least-sampled active edge is selected, ties in the
search's seeded order. Descendant allocation, lawful world sampling, frozen
policies, depth limits and terminal backup rules stay common. Final C choice
uses the best mean among active supported edges, avoiding selection by a
stale eliminated edge's mean. No prior probability or deck label enters C.

Rows have kind `search_allocation_experiment`. Root identity is
`(id, position, root_obs_sha256, root_menu_sha256)`, with `seed` the repetition,
`arm` the treatment, and `bank_binding_sha256` the policy/input binding.
`teacher.teacher_live` is the selected live action;
`teacher.selected_backups`, `edge_backups` and `edge_wins` expose its actual
support. `selection`, `work_attempts` and `primary_sha256` record the search.
`search_cpu_seconds` includes reconstruction; `setup_cpu_seconds`,
`evaluation_cpu_seconds`, `total_cpu_seconds`, and wall fields record stages.
`output_serialize_cpu_seconds` measures an additional serialization pass;
the launcher's full process ledger includes actual output I/O.

Primary evaluation uses separate `/fresh-action-evaluation` seed namespaces,
`Purpose::EvalWorld`, and paired policy streams common to all arms. It executes
that arm's frozen searched tree with no updates or new search. Matching
supported descendants use the existing eight-backup execution rule; unseen
or unsupported decisions use the ordinary frozen-policy fallback. Unsupported
roots still run all 128 scheduled worlds through that fallback. C uses a
private execution tree clone whose root skip mask forces its active supported
choice. The original search tree's counts and eligibility remain immutable,
and all descendant nodes and counts are identical. Clone CPU and estimated
payload memory are reported. Rejected, truncated and faulty samples remain
in the fixed scheduled denominator as unknown rather than being resampled.

Optional `plain` secondary evaluation forces only the selected root action
and then uses plain frozen continuation policies. It measures conditional
policy quality and can miss multi-step search lines. Unsupported secondary
roots have no evaluation samples. Evaluation-only jobs require this explicit
secondary continuation setting; primary trees are evaluated inline and are
never serialized as bulk evidence.

`evaluation.samples` contains `(index, world_seed, outcome)` or a sample
rejection; `outcome.w`, `unknown`, `end`, and transition counters are explicit.
`evaluation.wins`, `independent_natural`, `unknown`, and `scheduled` summarize
the fixed panel. Primary `evaluation_id` binds the seed namespace, treatment,
full execution-tree hash, C's forced root edge/mask, count and cap. Two arms
with the same root action still have separate searched continuations and
cannot reuse their primary evaluations. `root_fallbacks` reports support
fallbacks independently of wins. `evaluation_sha256` hashes deterministic
evaluation output. Secondary plain evaluation IDs bind the selected canonical
action, allowing exact reuse only within that explicitly secondary contract.
Evaluation-only jobs verify the prior row's root hashes, step, source ID,
position, seed, arm, bank binding, key mode and interchange setting before use.

This mode provides development-panel action evidence, not ordinary-start
playing strength or learned-policy improvement. Sharing arm B must be
integrated before launch; the allocation-only intermediate commit rejects B.
