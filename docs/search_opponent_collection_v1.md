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
- Every consumer except the public collector refuses such an episode: the shared `ExpandedEpisodeV1::configurations()` rejects it; only the collector calls `configurations_admitting_search_v1()`.
- The collector checks the descriptor SHA256 (`5eb1d55d...`) and the g115 checkpoint SHA256 (`88c0b997...`) itself, independently of `SearchPlayV3::new`, which in turn refuses any other budget, algorithm, seed or model identity (`search_opponent.rs`, `SearchOpponentV1::load`).
- On the opponent seat the unchanged evaluation wrapper `SearchPlayV3` chooses every action (S128, T1024, depth 8, seed 20260922). Only its visibility changed (pub(crate)); no search, sampler or descriptor code changed.
- The stored row keeps its ordinary shape: the actor-visible tensor and the frozen net's logits and value, scored without sampling, with the search's action and the sampler identity `mtg-kernel-v4-information-set-estimate-search/v3`. The search seat draws no random number.
- Each search trajectory carries compact per-decision records (actor, menu width, decision binding, root key, selected action, simulations, transitions, outcome SHA256) plus the descriptor and build (HEAD, clean flag, tracked-tree SHA256). A digest names the outcome but does not reconstruct the search tree; determinism is shown by replay.
- Validation: learner rows replay the behavior sampler exactly as before; each search row must match its record in order; ordinary validation refuses a search trajectory.
- The update is unchanged and reads learner rows only. No opponent action, visit count or value enters any learner target.
- A typed search error aborts the run: no fallback opponent, no derived-seed retry.
- A run whose schedule has search games publishes `search-opponent-receipt.json`: config SHA256, descriptor and checkpoint SHA256, search episode ids, build and executable SHA256.

## Information boundary

Code path: `search_v3.rs:66-103` calls `paired_bo1_harness_v1.rs:76-83`, then `search_leaf_v4.rs:110-131`. In `model_guided_search_core_v4.rs` every simulation seed derives from the actor-visible root key (`:64-68`, `:130`; the key hashes the actor's observation, `key_step.rs:24`), and every simulation runs on a fresh resampled clone (`:139`). `search_state.rs:98-158` resamples whole hidden objects (`sampler.rs:66-121`), replaces the future random stream (`:133`, `state.rs:2098`) and requires the visible boundary to be unchanged (`:157`).

Tests (`search_opponent/boundary_tests.rs`, both seats, legacy and environment-v2 randomness, reviewed budget): roots that differ only in which unseen learner cards are in hand versus library, in either player's unobserved library order, or in the future random stream give identical visible keys, identical resampled worlds for eight seeds and identical D3 actions and records, including a library-choice root. Power checks: the Legacy sample mode, which keeps the true random stream, is caught; a visible life change alters the estimates.

Scope: the sampler reshuffles the learner's true unseen cards, so the search knows the learner's remaining card multiset (registered or postboard list minus seen cards). This is D3's reviewed known-decklist convention, unchanged here.

## Determinism

Search simulation seeds derive from engine encodings, so a search game reproduces only at its recorded commit and executable; the trajectory records the build and the run receipt the executable SHA256. Cross-commit reproduction is not attempted.
