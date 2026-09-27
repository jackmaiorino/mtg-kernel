# D3 search wrapper as a training-time opponent (public-feature collector)

Status: draft (2026-09-27); qualification results pending. Lane opus-search-opponent, branch `opus/search-opponent-v1`, base `500cfae9`. Engineering only: nothing here is strength evidence. Goal: collab/GOALS/opus-search-opponent-20260927.md with the director's R5 amendment (DIRECTOR-RULINGS-20260927.md). Engine contract countersigned by Codex (CODEX #521, C1 to C5).

## What changed

The line (a) template (`a.json`, SHA256 `4352f9cc...`) is consumed by the public-feature trainer (`expanded_deck_training_v1/public_features.rs`, binary `public_feature_training_v1`). An episode may now pin the reviewed D3 descriptor:

```json
{"id": "...", "opponent": {"checkpoint": {"path": ".../block115/.../checkpoint.json", "sha256": "88c0b997..."}, "...": "..."},
 "opponent_search": {"path": "E:/mtg-g115-lineage-20260923/d3-search-descriptor-reviewed.json",
                     "sha256": "5eb1d55d13b78b341f8ff0c4df2589f8ee725974dc9fcadd691db6e12b2133d7"}}
```

- The field is omitted when absent, so existing configs, trajectories and hashes are byte-identical (`contract_tests.rs`; net-opponent golden below).
- Every episode consumer except the public collector refuses such an episode: the shared `ExpandedEpisodeV1::configurations()` rejects it; only the collector calls `configurations_for_public_collector_v1()`. The stack trainer (`stack_features.rs`) and the native expanded trainer use `configurations()`.
- A trajectory with a search opponent carries its own schema, `mtg-kernel-public-input-search-opponent-trajectory/v1`, so any reader that checks the ordinary public schema refuses it. The public replay audit also refuses any trajectory that carries a search record (`replay_audit.rs`, `admits_public_trajectory`). Python readers: census below.
- The collector checks the descriptor SHA256 (`5eb1d55d...`) and the g115 checkpoint SHA256 (`88c0b997...`) itself, independently of `SearchPlayV3::new`, which in turn refuses any other budget, algorithm, seed or model identity (`search_opponent.rs`, `SearchOpponentV1::load`).
- On the opponent seat the unchanged evaluation wrapper `SearchPlayV3` chooses every action (S128, T1024, depth 8, seed 20260922). Only its visibility changed (pub(crate)); no search, sampler or descriptor code changed.
- The stored row keeps its ordinary shape: the actor-visible tensor and the frozen net's logits and value, scored without sampling, with the search's action and the sampler identity `mtg-kernel-v4-information-set-estimate-search/v3`. The search seat draws no random number.
- Each search trajectory carries compact per-decision records (actor, menu width, decision binding, root key, selected action, simulations, transitions, outcome SHA256) plus the descriptor and build (HEAD, clean flag, tracked-tree SHA256). A digest names the outcome but does not reconstruct the search tree; determinism is shown by replay.
- Validation: learner rows replay the behavior sampler exactly as before; each search row must match its record in order, with simulations and transitions within the descriptor budget and the descriptor itself at the D3 budget; ordinary validation refuses a search trajectory.
- The update is unchanged and reads learner rows only. No opponent action, visit count or value enters any learner target.
- A typed search error aborts the run: no fallback opponent, no derived-seed retry.
- A run whose schedule has search games publishes `search-opponent-receipt.json`: config SHA256, descriptor and checkpoint SHA256, search episode ids, build and executable SHA256.

## Python readers of public trajectories (census)

Only `public_feature_training_v1` writes the opt-in opponent schemas. Opponent rows of every kind record another policy: D3 rows carry the frozen net's logits but the search's action, Legacy rows carry V3 tensors (empty for forced singletons), and public-checkpoint rows carry public rows (`auxiliary`), which an ordinary trajectory has for learner rows only. Rule: a reader run on treatment outputs asserts the ordinary schema or selects the learner seat. Census of tracked Python: files found by searching for `decisions`, `auxiliary` and `logits` indexing and trainer `episode-NNN.json` paths; every file that reads public-trainer rows was read.

| Reader | Rows it reads | On treatment trajectories |
|---|---|---|
| `qualify_public_state_only_v1.py:136-140`, `qualify_state_prevention_compute_v1.py:46-49` | every row with a public row | now assert the ordinary schema |
| `prevention_feature_mask_check_v1.py:47-52` | archive rows picked by a replay panel, any seat | now asserts the ordinary schema; the replay audit that builds the panel already refuses other schemas |
| `audit_public_pilot_behavior_v1.py:31-32`, `prevention_policy_drift_v1.py:85,249`, `audit_archived_gae_batch_v1.py:51-53`, `public_learning_localization_v1.py:148-160`, `g115_d4_learning_signal_v1.py:56-73`, `read_control_entropy_snapshot_v1.py:38-39`, `broader_readout_retention_v1.py:32-33`, `link_burn_credit_v1.py:37` | learner seat only | unaffected; learner rows keep the ordinary form |
| `public_feature_pilot_v1.py`, `qualify_public_learning_v1.py` and the launchers that pin `episode-NNN.json` hashes | no row contents (counts, hashes, byte identity, terminal class) | unaffected |
| `public_policy_replay_audit_v1.py`; `prepare_terminal_retention_v1.py` (replay results, both seats) | replay audit input or output | refused by `admits_public_trajectory` before scoring |
| `g115_d4_audit_manifest_v1.py` (`canonical_episode`) | episode fields; `order()` drops unknown keys | now refuses the opt-in fields instead of dropping them |

The other readers of `decisions` read evaluation match documents, diagnostic captures (`collected.trajectory.games`), stack trajectories (`mtg-kernel-public-stack-trajectory/v1`, asserted by `public_stack_exposure_v1.py`) or native expanded trajectories, none of which can carry these opponents.

## Information boundary

Code path: `search_v3.rs:66-103` calls `paired_bo1_harness_v1.rs:76-83`, then `search_leaf_v4.rs:110-131`. In `model_guided_search_core_v4.rs` every simulation seed derives from the actor-visible root key (`:64-68`, `:130`; the key hashes the actor's observation, `key_step.rs:24`), and every simulation runs on a fresh resampled clone (`:139`). `search_state.rs:98-158` resamples whole hidden objects (`sampler.rs:66-121`), replaces the future random stream (`:133`, `state.rs:2098`) and requires the visible boundary to be unchanged (`:157`).

Tests (`search_opponent/boundary_tests.rs`, both seats, legacy and environment-v2 randomness, reviewed budget): roots that differ only in which unseen learner cards are in hand versus library, in either player's unobserved library order, or in the future random stream give identical visible keys, identical resampled worlds for eight seeds and identical D3 actions and records, including a library-choice root. Power checks: the Legacy sample mode, which keeps the true random stream, is caught; a visible life change alters the estimates.

Scope: the sampler reshuffles the learner's true unseen cards, so the search knows the learner's remaining card multiset (registered or postboard list minus seen cards). This is D3's reviewed known-decklist convention, unchanged here.

## Determinism

Search simulation seeds derive from engine encodings, so a search game reproduces only at its recorded commit and executable; the trajectory records the build and the run receipt the executable SHA256. Cross-commit reproduction is not attempted.

## Opponent kinds interface v1 (proposal, director scope extension of 2026-09-27)

Status: countersigned with bindings (CODEX #558); implemented on the branch (829d8c4e, 549dd680, 6b1e91a9, 523e1bf3). Design entry for Fable filed separately (CLAUDE #474). Adds two opt-in opponent kinds to the public collector beside ordinary V4 and the D3 wrapper.

**Declaration.** One new optional episode field, `opponent_kind`, omitted when absent (every existing byte unchanged). The public_checkpoint JSON is byte-identical to the evaluator's `ModelSource` for that kind, so a launcher can copy an evaluation source; the legacy JSON is the evaluator's shape plus `admission`, so it is evaluator-compatible, not byte-identical:

```json
{"kind": "public_checkpoint", "config": {"path": "...", "sha256": "..."}, "checkpoint": {"path": "...", "sha256": "..."}}
{"kind": "legacy", "source": {"play_import": {}, "feature_transfer": {}, "checkpoint": null},
 "v3_forced_actions": true, "v3_spell_target_reference_adapter": true, "admission": {"path": "...", "sha256": "..."}}
```

| Episode fields | Opponent |
|---|---|
| `opponent` | ordinary V4 net (unchanged, V4 guard kept) |
| `opponent` + `opponent_search` | D3 wrapper on frozen g115 (countersigned, merged) |
| `opponent_kind` public_checkpoint, no `opponent` | recent public-input checkpoint |
| `opponent_kind` legacy, no `opponent` | frozen V3 or a declared V3-transfer import |

Any other combination, an unknown kind or an unknown field fails closed before collection; every consumer but the public collector refuses `opponent_kind` (as for `opponent_search`). A failed opponent is never substituted, skipped or retried.

**Admission.**
- public_checkpoint: loads through `public_features::load_for_evaluation` unchanged (checkpoint schema `mtg-kernel-public-input-checkpoint/v1`, the semantic config-hash check, optimizer pin, ages, projection mode). The effective identity keeps the public projection, `inputs_enabled` and projection mode, never base weights alone. Pins come from `line-a-recent-bindings-v1.json` (CODEX #529).
- legacy: `admission` pins a per-member receipt in the schema opus-panel-export writes for its imports (`mtg-kernel-line-a-panel-admission/v1`, CLAUDE #472): member, route (`strict` or `v3-frozen`), the model source bound by its pinned descriptor file (which must parse to exactly the declared source), the import descriptor (equal to the source's play import), the expected identity (identity schema, model-parameter and weights digests, V3 generation, observation successor, feature digests) and the adapter flags. After loading, the collector requires generation V3, the observation successor and the receipt's model identity; a true/true receipt alone is not authorization. The R14 registry route is refused until its acceptance is bound: a matching receipt alone cannot admit it. The frozen V3 receipt comes from a path relocation of the D3 envelope's source (every referenced input keeps its SHA256); panel receipts come from opus-panel-export.

**Opponent seat interface.** One lane-owned enum behind the collector (reset, effective identity, decide). Legacy calls the evaluator's own adapter functions in the evaluator's precedence (forced singleton first, then the spell-target adapter, then ordinary V3 scoring). `public_evaluation.rs` is not edited, so no evaluator golden is owed; instead an equivalence test replays collected Legacy games through the evaluator's dispatch and requires the same actions. Worker assignment never sets seeds or stream state.

**Versioned decision record.** `DecisionRecordV1` keeps its type for every kind, so ordinary trajectories stay byte-identical. A trajectory with a new kind carries its own outer schema (`mtg-kernel-public-input-legacy-opponent-trajectory/v1` or `mtg-kernel-public-input-public-checkpoint-opponent-trajectory/v1`) and one record `mtg-kernel-public-opponent-record/v1`: kind, seat, effective identity, adapter version, and one row per opponent decision with step, physical decision, substep, actor, menu width, selected action, the ordered legal menu's SHA256, and a form:
- `scored`: generation (v3 or v4), feature contract and encoding digests, observation (`original` or `spell_target_repaired`), sampler identity. The row holds the tensor, logits and value; a public checkpoint also stores its public auxiliary row.
- `unscored_singleton` (V3 forced): the row has empty logits and tensor and sampler identity `mtg-kernel-v3-forced-singleton/v1`. Validation draws once from that seat's stream over one logit and requires action 0. No scored row is fabricated and no tensor is reused.

Validation extends the existing seat hook: exact ordered one-to-one correspondence of records and opponent decisions, with seat, selected action, generation, the generation's feature digests and the menu hash format checked; every sampled opponent row, singletons included, replays exactly one draw from its physical seat's continuing stream; diagnostic scoring never samples again. Learner rows replay unchanged. The menu hash itself is reproduced by same-seed replay, not recomputed by the validator.

**Learner isolation.** At the same learner-visible state, weights and sampler position, learner tensors, auxiliary rows and logits are identical whatever the opponent kind (test). Opponent values, auxiliary rows, search statistics and decisions enter no target, normalization, entropy or gradient weight; learner-group filtering order is unchanged.

**Receipts.** A run with any new kind publishes `opponent-kinds-receipt.json`: per kind, source, config, checkpoint and optimizer hashes, resulting weights and feature identity, adapter version, registry route, sampler identity and executable SHA256.

**Per-kind blockers.**
- public_checkpoint: none beyond implementation and countersign; four recent pins exist.
- legacy: admission receipts. The frozen V3 is build-bound: its transfer envelope records the destination build commit and the loader reproduces the envelope, so a V3 source loads only in a binary of that commit. The established rebinding (`g115_d3_payload_v1.prepare`, also used by line-a-launcher) changes only `receipt.destination_build_git_head`. An admission receipt binds that build's model source, so each training build needs its own rebind and receipt; the expected identity is build-independent: `mtg-kernel-registry-transferred-play/v1`, model parameters `8474051a...` (the transfer receipt's destination), weights `2fa88edf...`, V3, observation successor (probe at 13759e4d; receipt `e46fd8ac...` admits it through the collector path). Panel members come from opus-panel-export: current-1 on the strict route now; seven refresh members need the R14 route; the six v3b endpoints also lack Store authority and stay later-version candidates. V3 rows inside a V4-learner trajectory are countersigned under the new outer schema and kind-aware validator (CODEX #558).
