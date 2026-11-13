# Foundations Limited training campaign and fixed-opponent evaluation (v1)

Issue: [#657](https://github.com/jackmaiorino/mtg-kernel/issues/657) (split from [#552](https://github.com/jackmaiorino/mtg-kernel/issues/552), bounded harness merged in [#586](https://github.com/jackmaiorino/mtg-kernel/pull/586)).

Source modules and tools referenced by this design:
- `python/tools/limited_training_inventory_v1.py`
- `data/limited/fdn_v1/booster_pool_v1.json` and `data/limited/draftzero_sample_v1/pool.tsv`
- `mtg-kernel/src/limited_full_pool_consumer_v1.rs`
- `mtg-kernel/src/reveal_meaning_v3.rs` and `mtg-kernel/src/reveal_meaning_v3/reducer.rs`
- `mtg-kernel/src/current_tree_learner_v1.rs`
- `mtg-kernel/src/limited_match_v1.rs` and `mtg-kernel/src/limited_match_v1/full_pool_match_v1.rs`
- `mtg-kernel/src/bin/kernel_limited_training_v1.rs` and `mtg-kernel/src/bin/kernel_limited_match_v1.rs`
- `python/tools/host_slots_v1.py`

This document is Deliverable 1 of `#657`: the pre-compute design specification covering the 80-deck DraftZero inventory, session and information boundary, full-episode model and learning contract, checkpoint retention and resume durability, disjoint held-out evaluation protocol, and useful-compute qualification and host placement gates. No multi-iteration campaign compute runs before this design is reviewed and quiet-host completed-work scaling is measured.

---

## 1. Frozen Foundations Pool and 80-Deck DraftZero Inventory

### 1.1 FullPool 286/286 `Full` Coverage and `Drake Hatcher` Runtime Status

The target card pool is the frozen 286-printing Foundations Play Booster manifest in `data/limited/fdn_v1/booster_pool_v1.json` (276 main-set printings plus 10 Special Guests; zero Missing printings). While the shared catalog (`data/cards_v1.json` and `data/limited/fdn_v1/cards_v1.json`) retains `250 Full / 36 Partial` so `KERNEL_CARDDB_HASH` and historical sessions stay unchanged, `-session full-pool` (`SessionKindV1::FullPoolV1`, profile `fdn-full-pool-consumer/v1`, merged in `#586` and recorded in `#658` at `b24a5075e`) provides the complete authenticated consumer stack (owning session, Human projection, JSONL wire, fair closed-list search, and sealed numeric learner input) for all 286 printings. Under `FullPoolV1`, all 286 Foundations printings count and execute as `Full`.

For `Drake Hatcher` per-step history scaling (`#550` / `#627`, following `#474`):
- Merged PRs `#633` (`7d1a13ef6`, shared `Arc<Vec<GameObject>>` checkpoint arena and `borrow_memo_v1` per-validation indexes), `#654` (`6da0bdff0`, leaf event-kind tags for `EventHistoryV1::of_kinds_v2`), and `#659` (`cc2c994fb`, history-attached validator proofs in `entry_observations_v1`) reduced total decision time on the four fixed `Drake Hatcher` stress cases by 69% to 76% with identical winners, decision counts, and final-state hashes.
- Across the 80 DraftZero decks, exactly 6 decks include `Drake Hatcher`:
  - `train` (4 decks): `FDN_top_01766_UR`, `FDN_top_02819_WU`, `FDN_top_10624_UB`, `FDN_top_18039_WUpR`.
  - `eval` (2 decks): `FDN_top_03718_UR`, `FDN_top_13096_WU`.
- **Pre-benchmark implementation note (Drake absence fast-path)**: On `main` (`791701e2f`), `drake_hatcher_v1::custody::relevant` (`mtg-kernel/src/drake_hatcher_v1/custody.rs:1263`) and `drake_public_v53::capture_for_consumer` (`mtg-kernel/src/drake_public_v53.rs:779`) still call raw `event_history.iter()` rather than `event_history.of_kinds_v2(&[EventKindV1::DrakeJournalV1])` (tracked in local preparation branch `51b0cc9a0` and `#627`). Converting those two call sites to `of_kinds_v2` (and registering both in `regret_census_v1/stage4a/history_guard_tests.rs`) complements `#654` by giving `O(1)` `has_any` absence short-circuiting on all 74 non-Drake decks and leaf-tag filtering on the 6 Drake decks. Land that two-site change before recording Deliverable 2's quiet-host throughput baseline.

### 1.2 Exact 80-Deck DraftZero Split and Input Pins

`python/tools/limited_training_inventory_v1.py` audits `data/limited/draftzero_sample_v1/pool.tsv` and `data/limited/draftzero_sample_v1/decks/<deck>.json` under schema `limited-training-deck-inventory/v1`:

- `sample_sha256_in_pool_order`: `c7e9c271e15150c245aeabf5f2d10d73fbc41aa1586645101c64198d5562d23b`
- `data/cards_v1.json` SHA-256: `ef738001c373076039f211582f5fd07b11903788e70641ec7e23c84ba4f69b14`
- `data/limited/fdn_v1/cards_v1.json` SHA-256: `91dcd869bb339450d0adc3e8cfa1a8320f560e53c337f21fb263a7a059839e13`

| Split | Total decks | 40-card | 41-card | 49-card | Catalog `Full`-only | Contains catalog `Partial` | Playable in `FullPoolV1` |
| :--- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `train` | 40 | 39 | 1 | 0 | 20 | 20 | **40 / 40 (100%)** |
| `eval` | 40 | 37 | 2 | 1 | 19 | 21 | **40 / 40 (100%)** |
| **Total** | **80** | **76** | **3** | **1** | **39** | **41** | **80 / 80 (100%)** |

The `train` and `eval` splits are strictly disjoint by both deck name and sorted card-ID multiset (`belief_overlap == 0` under `kernel_limited_match_v1` multiset verification).

### 1.3 Policy for the Four `>40`-Card Registrations (Exact Rules-Legal Registration, Zero Trimming)

Four authentic 17Lands PremierDraft human lists in `draftzero_sample_v1` register more than 40 mainboard cards:

| Deck ID | Split | Cards | File SHA-256 | Notes |
| :--- | :--- | ---: | :--- | :--- |
| `FDN_top_09446_URpG` | `train` | 41 | `4fdce6de44c8e2217b8e43e3381ab9ebc446813dbd71ef4805fb7bbf7385ba42` | Includes `Curator of Destinies` (`Full` in `FullPoolV1`) |
| `FDN_top_03718_UR` | `eval` | 41 | `6c1166822ee4b535c0ecd7f3e7221385da7571ce8b4239f8a85285cdf0a349ee` | Catalog `Full`-only; includes `Drake Hatcher` |
| `FDN_top_24789_BGpW` | `eval` | 41 | `5aabe4f9a807145b806eac130834b526773382e9ba4622002253016b75be2b9b` | Catalog `Full`-only |
| `FDN_top_21511_WUR` | `eval` | 49 | `dc7384f3c03a319c1610ed60153b576f8dcd9f35a0bdd0dfd13d3b0b3012261e` | Includes `Thousand-Year Storm` (`Full` in `FullPoolV1`) |

**Design decision**: **Admit all four lists at their exact original rules-legal card counts (`41`, `41`, `41`, `49`); forbid 40-card trimming, widening, or exclusion.**
1. **Magic rules legality**: Under Comprehensive Rules CR 100.2b, Constructed Limited (Draft / Sealed) minimum mainboard size is 40 cards with no upper limit. Human 41-card and occasional 45-to-49-card registrations are real features of the DraftZero corpus.
2. **Engine and model support**: `DeckFileV1::resolve_for(SessionKindV1::FullPoolV1)` (`limited_match_v1.rs:132`) enforces `ids.len() >= 40`, `limited_full_pool_consumer_v1::preflight` (`limited_full_pool_consumer_v1.rs:133`) admits `40..=MAX_CUSTOM_DECK_CARDS_V1` (`10_000`), `current_tree_learner_v1` scores variable-length `NumericTree` packets without fixed deck-size arrays, and `FullPoolClosedListV1` conditions opponent belief sampling on `total = observed + unknown_slots + face_down` with deterministic `NearestDeck` fallback (`limited_match_v1.rs:368-411`).
3. **Harness change**: Only `#586`'s bounded smoke binary (`kernel_limited_training_v1.rs:298`) rejected `cards.len() != 40` because its four compile-bound fixtures were 40-card decks. The campaign runner validates `cards.len() == expected_pool_tsv_cards && (40..=MAX_CUSTOM_DECK_CARDS_V1).contains(&cards.len())` and pins every deck's exact `cards` count and `source_sha256` from `pool.tsv` so `sample_sha256_in_pool_order` stays immutable (`docs/reports/limited_training_readiness_20261108.md`).

---

## 2. Session, Information Contract, and Paired Seed Schedule

### 2.1 Owning Session and Public Reveal Boundary

Every training and evaluation game executes through `limited_full_pool_consumer_v1::reset_numeric_reveal_facts_v3(&ResetV1)` (`PROFILE_V1 = "fdn-full-pool-consumer/v1"`), returning a sealed `KaitoSession`:
- **Rules surface**: Engine priority windows, Foundations combat damage ordering and trample, London mulligans before turn 1 (`ChooseLondonMulligan` encoded in `ACTION_FLAGS_V1[4]`), public shroud (`LIMITED_PUBLIC_SHROUD_SCHEMA_V1 = 5`), and all six `FullPool` native receipt modes (`curator_unordered_reveal_v1`, `greatwurm_commit_receipts_v1`, `harvester_commit_receipts_v1`, `fiend_artisan_receipts_v1`, `etali_commit_receipts_v1`, `thousand_year_storm_consumer_v1`).
- **Authenticated public hand knowledge (`public-reveal-minima/v3`)**: `reveal_meaning_v3/reducer.rs` reduces the acting seat's viewer-local event transcript into sorted printing-ID minimum counts (`RevealKnowledgeV3`). Opponent-hand physical `ObjectId` copy bindings, unseen library order, and opponent private look/scry selections never enter `Packet` or `AuthenticatedInputV1`.
- **Sealed action menu**: `session.encode(decision)` produces `KaitoDecision` over sealed public ordinals, omitting infeasible combat answers (`ChooseAttackerInclusion` / `ChooseBlockerInclusion` mapped to `ACTION_FLAGS_V1[5]`). Action selection and action traces record the sealed ordinal and menu width `bound.packet().actions().len()`.
- **Action sampler**: Both seats sample via `f64-exp-icdf-u53/v1` (`unclamped_softmax_select_v1` driven by `SplitMix64`), preserving exact deterministic replay across worker counts and platforms.

### 2.2 Seat-Alternating Paired Deal Schedule

Deck-pair favorability and play/draw asymmetry are major variance sources in 40-deck Limited pools. Every collection batch and every evaluation match is organized into **2-game pairs** indexed by `global_pair_index: u64`:

1. **Deal derivation**: Pair `p` calls `deal_pair_v1(schedule_seed, p, 40)` (`limited_match_v1.rs:1158`) over the split's 40 decks in canonical `pool.tsv` order, deterministically drawing:
   - Two distinct deck indices `[d_0, d_1]` (`d_0 != d_1`, uniform over all `40 * 39 = 1,560` ordered pairs),
   - A shared `env_seed: u64` (governing opening library shuffles and environment RNG), and
   - A `starting_player: PlayerId` (`P0` or `P1`).
2. **Asymmetric policy pairs (league training pairs and all fixed-opponent evaluations)**:
   - **Leg 0**: Seat 0 plays Policy A with `d_0`; Seat 1 plays Policy B with `d_1`; `env_seed` and `starting_player` as dealt.
   - **Leg 1**: Seat 0 plays Policy B with `d_0`; Seat 1 plays Policy A with `d_1`; **identical** `env_seed` and `starting_player` (matching `kernel_limited_match_v1::play_one` lines 512-528).
   - Across the two legs, Policy A plays both decks (`d_0` and `d_1`) and both turn orders (on the play once, on the draw once) against the identical opening deal.
3. **Symmetric self-play pairs (`[pi_t, pi_t]`)**:
   - Both seats run the frozen pre-update learner `pi_t` on `[d_0, d_1]` with `env_seed`.
   - **Leg 0** starts `starting_player`; **Leg 1** starts `starting_player.opponent()` (generalizing `kernel_limited_training_v1.rs:329-333`), canceling play/draw bias for each dealt deck pair.
   - Per-seat policy RNG is seeded from `derive_seed(b"mtg-kernel/limited-campaign-actor-rng/v1\0", &[schedule_seed, p, leg as u64, seat as u64])` so Leg 0 and Leg 1 never couple RNG streams across workers.

---

## 3. Model, Full-Episode Learning Contract, and Checkpoint Durability

### 3.1 Full Natural-Terminal Episodes and Explicit Truncation vs Refusal Rules

`#586` capped retention at `RETAIN_PER_SEAT = 32` decisions per seat (`MAX_STEPS = 4_000`) to prove the smoke harness. Full Foundations Limited games in `#586` averaged ~1,050 to 1,166 total actions per game, so a 32-decision prefix discards midgame and endgame combat/resource decisions where Limited games are decided.

The campaign trainer removes the 32-decision retention cap:
- **Retained decisions**: Every decision of a learner-controlled seat with `legal > 1` from pregame London mulligan to episode termination is retained (`retain_all_multi_action_decisions: true`). Forced single-action steps (`legal == 1`) execute and appear in receipt counts without adding zero-gradient `log 1 = 0` examples.
- **Explicit safety ceilings**:
  - `max_turns = 100` public turns,
  - `max_decisions = 4_000` actor decisions per game,
  - `ResetV1` inner bounds: `max_physical_decisions = 16_000`, `max_policy_steps = 2_048_000` (matching `full_pool_match_v1.rs:153-154`).
- **Terminal classification contract (never silently dropped)**:
  1. **`Natural` (`P0Win`, `P1Win`, `Draw`)**: Assigned exact terminal outcome `r in {+1.0, -1.0, 0.0}` (`P0Win -> [+1, -1]`, `P1Win -> [-1, +1]`, `Draw -> [0, 0]`). Natural draws (simultaneous state-based loss or empty-library draw) participate normally with `r = 0.0`.
  2. **`Truncated` / `Stall` (turn or decision safety ceiling reached)**: **Recorded explicitly, never silently dropped, and never misclassified as a natural draw or engine crash.** The game's complete receipt (`pair`, `leg`, `seed`, `decks`, `turns`, `decisions`, `truncation_reason`, and action trace digest) is persisted in the iteration artifact; its captured decisions receive zero policy-gradient weight (`advantage = 0.0`, excluded from gradient normalization count `N_effective` while counted in `truncated_games`). **Batch health gate**: if `truncated_games > 2` in a 32-game update batch (`> 6.25%`) on two consecutive updates, the campaign halts cleanly with status `stopped_excess_truncation` rather than biasing updates toward fast aggro games.
  3. **`Halted` or step/encode/inference error**: **Fail-closed.** Any `TerminalClassificationV1::Halted`, `Natural` without W/D/L, or error from `reset`, `encode`, `training_input_v1`, `forward`, or `apply` writes `failed-iteration-UUUU.json` atomically and aborts with exit code `2`.

### 3.2 Model Contract and GAE Critic vs Paired-REINFORCE Objective

`current_tree_learner_v1` (`CONTRACT_V1 = "current-tree-learner/v1/f64/input20/hidden16/flags6/adam"`, `647` `f64` parameters) encodes state and each legal action via a shared 16-unit recursive `NumericTree` encoder (`state.pooled: [f64; 16]`, `action.pooled: [f64; 16]`) plus bilinear `(state, action)` interaction and 6 explicit semantic action-flag weights (`#454` remains open for a wider production tensorizer/transformer successor).

Assigning an unbaselined terminal `+/-1` REINFORCE weight equally across 250 to 400 multi-action decisions per seat produces high credit-assignment variance. The campaign defines a versioned `v2` actor-critic contract while keeping `CONTRACT_V1` untouched for `#552` replay compatibility:

- **Primary Campaign Learner (`current_tree_learner_v2`, contract `"current-tree-learner/v2/f64/input20/hidden32/flags6/value1/gae-adam"`)**:
  - **Architecture**: Shared recursive `NumericTree` encoder with hidden width `H = 32` (`INPUT = 20`, `RECURRENT = 32 * 32`, `BIAS = 32`, bilinear policy head `3 * 32 + 6 + 1 = 103`) plus a scalar state-value head `V_theta(s) = tanh(w_v . state.pooled + b_v) in (-1, 1)` (`32 + 1 = 33` parameters), totaling **`1,828` `f64` parameters** (4x the representation capacity of `H = 16` at negligible `<2%` forward/backward CPU share relative to engine `apply` + `encode`). An `H = 16` (`664`-parameter) configuration is retained as a compile-checked profile flag for exact width ablation if Gate 2 timing shows `model_forward + update` exceeding 15% of iteration wall time.
  - **Initialization**: Deterministic finite initialization seeded from a fixed contract tag (`step = 0`, zero Adam moments, `w_v = 0`, `b_v = 0` so initial value predictions start at `0.0`).
  - **Per-seat GAE credit assignment**: For each natural game and learner-controlled seat with `T` retained non-forced decisions `(s_0, a_0), ..., (s_{T-1}, a_{T-1})` and natural terminal return `R in {-1.0, 0.0, +1.0}`:
    - Define `V_t = V_theta(s_t)` under pre-update parameters `theta` for `0 <= t < T`, and bootstrap terminal value at `T`:
      $$\delta_t = \begin{cases} \gamma V_{t+1} - V_t & \text{if } t < T - 1 \\ R - V_{T-1} & \text{if } t = T - 1 \end{cases}$$
    - Generalized Advantage Estimation with $\gamma = 0.997$, $\lambda = 0.95$:
      $$A_t = \sum_{k=0}^{T - 1 - t} (\gamma \lambda)^k \delta_{t+k}, \qquad \hat{V}_t^{\text{target}} = A_t + V_t$$
    - Advantages $A_t$ across all retained learner decisions in the synchronous batch are standardized by batch standard deviation ($\tilde{A}_t = A_t / (\sigma_A + 10^{-8})$, without subtracting batch mean when league pairs are asymmetric so net win-rate drift stays unbiased, or mean-centered within symmetric self-play pairs).
  - **Composite synchronous loss** (reduced in canonical `(pair_index, leg_index, seat, decision_index)` order for bit-exact multi-worker determinism):
    $$\mathcal{L}(\theta) = \frac{1}{N} \sum_{n=1}^{N} \left[ -\tilde{A}_n \log \pi_\theta(a_n \mid s_n) + c_v \cdot \frac{1}{2}\left(V_\theta(s_n) - \hat{V}_n^{\text{target}}\right)^2 - c_{\text{ent}} \mathcal{H}(\pi_\theta(\cdot \mid s_n)) \right]$$
    with $c_v = 0.5$ and $c_{\text{ent}} = 0.01$.
- **Diagnostic Control (`pair-centered-reinforce/v1`)**: The runner also supports `c_v = 0, lambda = 1, gamma = 1` with pair-centered baseline $A_{p,\ell,\text{seat}} = R_{p,\ell,\text{seat}} - \bar{R}_{p,\text{deck}(\text{seat})}$ on `CONTRACT_V1` (`647` parameters) as a zero-critic ablation control.

### 3.3 Batch Size, Opponent Schedule, Optimizer, Horizon, and Stopping Rule

| Parameter | Specification | Rationale |
| :--- | :--- | :--- |
| **Batch size per update** | **16 pairs = 32 games** (~8,000-11,000 non-forced learner decisions) | Covers 32 deck seats per step while keeping update cadence responsive |
| **Training opponent mix** | **12 pairs (75%) self-play** (`[pi_u, pi_u]`) + **4 pairs (25%) historical league** (`[pi_u, pi_hist]`, uniform over `{step_0} U {step_10, step_20, ...}`) | Prevents cyclic forgetting and anchors policy improvement against initial/past snapshots |
| **Optimizer & precision** | Synchronous `f64` Adam (`beta1 = 0.9`, `beta2 = 0.999`, `epsilon = 1e-8`), global L2 grad-norm clip `1.0` | Exact IEEE-754 `f64` bit reproducibility matching `LearnerV1` |
| **Learning rate schedule** | Cosine decay `lr_0 = 3e-4 -> lr_end = 1e-4` over `U_max` updates | Smooth late-campaign convergence on noisy Limited outcomes |
| **Campaign horizon `U_max`** | **Nominal `200` updates (`3,200` pairs = `6,400` games)**; minimum admissible horizon `100` updates (`3,200` games) if bounded by Gate 2 cost | Locked in `campaign_manifest.json` after Deliverable 2 quiet-host timing before update 1 runs |
| **Dev-split progress check** | Every **20 updates**: `32` pairs (`64` games) vs `step_0` on **`train`-split decks only** (`dev_seed = 657_050`, disjoint pair range) | Tracks learning and detects collapse without ever touching the 40-deck `eval` split |

**Stopping rules**:
1. **Nominal completion**: Update `u == U_max` completes and publishes `complete.json`.
2. **Dev-split plateau stop**: After update `80`, if the dev-split paired score (`train` decks, `dev_seed = 657_050`) against `step_0` fails to improve upon the running best dev score by at least `+0.02` across **3 consecutive** 20-update checks (60 updates), stop cleanly at that milestone checkpoint with `stop_reason = "dev_plateau"`.
3. **Hard abort / safety stop**: Immediate fail-closed halt on any `Halted` game error, non-finite logit/loss/gradient, checkpoint reload mismatch, or `truncated_games > 2` on two consecutive updates.

### 3.4 Checkpoint Cadence, Provenance Retention, and Cold-Resume Contract

To satisfy `#657` Deliverable 3 (*"with every checkpoint and its source identity retained"*):
- **100% checkpoint retention**: Every update `u in 0..=U_final` writes `checkpoints/step_UUUU.json` (plus `iterations/iter_UUUU/receipt.json` and `iterations/iter_UUUU/complete.json`). At `1,828` `f64` parameters + `2 * 1,828` Adam moments (`~44 KB` raw IEEE-754 `u64` bits per snapshot), retaining all `201` checkpoints uses `< 25 MB` on disk; zero checkpoint pruning is permitted.
- **Bound source identity per checkpoint**: Every `step_UUUU.json` embeds:
  - `schema`: `"limited-campaign-checkpoint/v1"`
  - `git_head`, `binary_sha256`, `binary_source_sha256`
  - `card_db_hash` (`KERNEL_CARDDB_HASH`), `sample_sha256_in_pool_order` (`c7e9c271...`)
  - `actor_profile` (`"fdn-full-pool-consumer/v1"`), `actor_producer_sha256`
  - `hand_knowledge` (`"public-reveal-minima/v3"`), `reveal_producer_sha256`, `reveal_reducer_sha256`
  - `learner_contract`, `feature_sha256`, `optimizer_config`
  - `parent_checkpoint_sha256` (`null` at `step_0000`, exact SHA-256 of `step_(u-1).json` for `u >= 1`), `iteration_receipt_sha256`, and `SnapshotV2` (`step`, `parameter_bits`, `first_moment_bits`, `second_moment_bits`).
- **Transactional publication and cold resume**:
  - Writes use `.staging` + `File::sync_all()` + `fs::rename` + immediate readback `restore()` verification before `complete.json` is published.
  - Resuming a campaign reads the highest contiguous `iter_UUUU/complete.json`, verifies every source/feature/pool digest and the full `parent_checkpoint_sha256` chain back to `step_0000.json`, cleans any uncommitted `iter_(U+1)` staging directory, and continues at `u + 1` with byte-identical outputs to an uninterrupted run.

---

## 4. Disjoint Held-Out Evaluation Protocol (`40` `eval` Decks)

### 4.1 Holdout Hygiene and Candidate Selection

- The 40 `eval` decks in `data/limited/draftzero_sample_v1/pool.tsv` are **never** loaded during training collection, league opponent play, hyperparameter tuning, or dev-split monitoring.
- Before running held-out evaluation, candidate checkpoint selection is fixed by rule from training artifacts alone:
  1. `u_final`: the final committed campaign checkpoint, and
  2. `u_best_dev`: the milestone checkpoint (`u mod 20 == 0`) with the highest `train`-split dev score vs `step_0`, evaluated **only** if `u_best_dev != u_final` and its dev score exceeds `u_final`'s by `>= +0.03`. If two checkpoints are evaluated on `eval`, Holm-Bonferroni multiplicity adjustment (`alpha = 0.025`, i.e., `97.5%` two-sided bootstrap CIs) applies to the better of the two.

### 4.2 Fixed-Opponent Pool and Match Budget

All opponents act through the identical `-session full-pool` (`KaitoSession`) and `public-reveal-minima/v3` information boundary, using seat-swapped paired deals (`deal_pair_v1(opponent_eval_seed, p, 40)`) over all 40 `eval` decks (including the two 41-card lists and one 49-card list):

| Opponent ID | Implementation & Information Boundary | Pairs `K` (Games `2K`) | Seed | Purpose |
| :--- | :--- | ---: | ---: | :--- |
| **`initial-weights/step-0`** | `checkpoints/step_0000.json` under identical `f64-exp-icdf-u53/v1` sampling | `200` (`400` games) | `657_101` | Direct controlled test of out-of-deck generalization from gradient updates |
| **`uniform-legal/v1`** | Uniform choice over the sealed `KaitoDecision` legal menu (`PolicySpecV1::Random`) | `200` (`400` games) | `657_102` | Action-menu sanity floor |
| **`sampled-public-heuristic/v1`** | 1-ply greedy scorer evaluating candidate root actions via `kernel_native_search_opponent_v1::evaluate_state_v1` inside one `sampled_full_pool_closed_list_v1` world (`-belief-split train`, `belief_overlap == 0`) so hidden opponent hand/library cards are never read | `200` (`400` games) | `657_103` | Fast non-random domain heuristic baseline (`life`, `hand.len()`, `library.len()`, mana, battlefield P/T + keywords) |
| **`pimc:sims=16:depth=24:closedlist-full-pool-v1`** | Checked-in fair root-level determinized UCB1 rollout search (`limited_match_v1/full_pool_match_v1.rs`) conditioned on the disjoint 40-deck `train` belief corpus (`world_contract = "limited-closedlist-full-pool/v1"`, `belief_overlap == 0` verified by name and multiset) | `100` (`200` games)* | `657_104` | Stronger fair-search anchor (*`sims in {8, 16}` finalized from Gate 2 per-decision search cost so total search-eval wall time stays `<= 25%` of campaign budget) |

### 4.3 Scoring, Confidence Intervals, and Pass / Claim Rules

For pair $p \in \{0, \dots, K-1\}$ and leg $\ell \in \{0, 1\}$, let candidate score $s_{p,\ell} = 1.0$ on a natural candidate win, $0.5$ on a natural draw or symmetric `Stall` (with a separate sensitivity column scoring candidate stalls as $0.0$), and $0.0$ on a natural candidate loss. Because Leg 0 and Leg 1 share the same deck pair $[d_0, d_1]$, `env_seed`, and `starting_player` with swapped seats, **the independent statistical unit is the pair mean**:
$$y_p = \frac{s_{p,0} + s_{p,1}}{2} \in \{0.00, 0.25, 0.50, 0.75, 1.00\}, \qquad \bar{y} = \frac{1}{K}\sum_{p=0}^{K-1} y_p$$

Every evaluation report publishes:
1. **Primary inference (cluster-correct)**: Mean pair score $\bar{y}$, empirical pair SD $\hat{\sigma}_y$, **95% paired percentile/BCa bootstrap confidence interval** over $y_0, \dots, y_{K-1}$ (`10,000` resamples seeded deterministically with `SplitMix64(657_999)`), and exact two-sided paired sign / Wilcoxon p-value on discordant pairs ($y_p > 0.5$ vs $y_p < 0.5$).
2. **Secondary descriptive interval**: Per-game W/D/L/Stall counts and **95% Wilson score interval** over decisive games and over $2K$ total games (labeled descriptive, since paired legs are negatively correlated on deck bias).
3. **Subgroup audit**: Separate W/D/L breakdown on (a) the 20 catalog-`Full`-only vs 20 catalog-`Partial`-containing `eval` decks, (b) pairs involving the three `>40`-card `eval` decks (`41`, `41`, `49`), and (c) pairs involving the two `Drake Hatcher` `eval` decks.

**Pre-registered integrity and claim gates** (reviewed before any strength claim per `#657` Deliverable 4):
- **Gate E0 - Evaluation Integrity (Hard Prerequisite)**: `errors == 0`, `rollout_errors == 0`, `belief_overlap == 0` (by deck name and card multiset), `stalls / total_games <= 0.02`, and byte-identical replay of pairs `0..4` across two worker counts. Any `E0` failure blocks all strength claims.
- **Gate E1 - Generalizing Learning Claim**: Requires `E0` pass, 95% paired bootstrap CI lower bound **`> 0.52`** vs `initial-weights/step-0` (`K = 200` pairs; at typical $\hat{\sigma}_y \approx 0.35$, $\text{SE} \approx 0.025$, requiring $\bar{y} \ge 0.57$), **and** 95% paired bootstrap CI lower bound **`> 0.60`** vs `uniform-legal/v1` (`K = 200` pairs).
- **Gate E2 - Heuristic-Beating Strength Claim**: Requires `E1` pass **and** 95% paired bootstrap CI lower bound **`> 0.50`** vs `sampled-public-heuristic/v1` (`K = 200` pairs).
- **Gate E3 - Fair-Search Parity / Advantage Claim**: Requires `E2` pass **and** 95% paired bootstrap CI lower bound **`> 0.50`** vs `pimc:sims=16:depth=24:closedlist-full-pool-v1` (`K = 100` pairs). If `E1`/`E2` pass while `E3 <= 0.50`, report the exact paired score and Elo differential against fair search without claiming search superiority.

---

## 5. Compute-Policy Qualification, Host Placement, and Governance Gates

Under `COMPUTE-POLICY.md` and `AGENTS.md`, no multi-iteration campaign run may be launched until the following three gates complete in order.

### 5.1 Gate 1: Pre-Compute Design and Campaign Governance Review

1. **Single-lead campaign coordination (`AGENTS.md` lines 27-33)**: One Claude `ResearchLead` coordinates `mtg-kernel` experimental campaigns and compute allocation across hosts (aligned with the `2026-11-13` compute-allocation Council). Implementation of the campaign runner and the two-site `DrakeJournalV1` absence guard is routine engineering in an isolated worktree, whereas dispatching multi-hour campaign compute requires `ResearchLead` scheduling and reviewed design sign-off.
2. **Independent design review (`#657` Deliverable 1)**: Record substantive review of this document (verifying deck/seed hygiene, GAE/truncation semantics, statistical gates, and placement budget) on the `#657` design PR before running campaign compute.

### 5.2 Gate 2: Quiet-Host Completed-Work Scaling Qualification (`#657` Deliverable 2)

`#586`'s cloud bounded runs (`4,761.2 s` at `1` worker, `2,952.3 s` at `2` workers at `6adee6579`) are **not** valid throughput baselines: they compiled with `LTO off` (`16` codegen units), executed side-by-side on a shared 4-vCPU VM, predated `#654`/`#659`, and ran only two fixed 40-card pairs.

Deliverable 2 executes a representative quiet-host scaling qualification under `COMPUTE-POLICY.md` Rules 2, 3, 4, and 5:
1. **Checked-in release binary**: Build with the repository's production `release` profile (`lto = "thin"`, `codegen-units = 1`, `--features limited-fdn-fixtures`), recording `rustc -Vv`, linker, `git_head`, and binary SHA-256.
2. **Quiet host + `host_slots_v1` reservation**: Verify zero competing training, VM, or build load on the benchmark machine (`COMPUTE-POLICY.md`: *"Never benchmark on a contended machine"*). Acquire physical core slots through `python/tools/host_slots_v1.py`.
3. **Identical representative workload across `W in {1, 2, 4, 8}` workers** (up to eligible physical cores on the quiet host):
   - Run **8 paired deals (`16` natural games)** drawn from `train` (`seed = 657_001`, fixed to include `FDN_top_09446_URpG` [41 cards] and at least one `Drake Hatcher` list) with full natural-terminal retention, plus **1 synchronous `LearnerV2` update**, atomic checkpoint write + reload, and **2 paired evaluation deals (`4` games)** on `train` dev pairs with `--stage-diagnostics`.
4. **Required qualification checks**:
   - **Determinism parity**: `step_0001.json` and `evaluation.json` must match **byte-for-byte** across `W = 1, 2, 4, 8` and between `--stage-diagnostics` on vs off.
   - **Completed-work metrics**: Record `completed_natural_games_per_sec`, `decisions_per_sec`, speedup `S(W) = T(1) / T(W)`, parallel efficiency `E(W) = S(W) / W`, peak RSS per worker, and `--stage-diagnostics` breakdown (`apply`, `authenticated_input`, `encode`, `model_forward`, `sample`, `update`, `io`).
   - **Cost projection**: Compute projected wall-clock time `T_campaign(W) = (6,400 + 1,400 eval games) / games_per_hour(W)` (plus dev checks and fair-search eval cost measured on 4 pairs of `closedlist-full-pool-v1`) and lock `W*` and `U_max` in `qualification_manifest.json`.
5. **Guarded launcher enforcement (`COMPUTE-POLICY.md` Rule 5)**: The campaign CLI subcommand refuses to start `U_max > 1` without `-qualification-manifest <path>` whose `binary_source_sha256`, `feature_sha256`, `card_db_hash`, and host class match the running binary and verify `W >= 2` parallel collection (unless `W = 1` was empirically faster, which cannot hold for independent episode pairs).

### 5.3 Gate 3: Host Inventory and Placement Comparison (`COMPUTE-POLICY.md` Rules 1, 4, 7)

Observed fleet state at design time (`2026-11-12 22:30 ET`, `collab` `c568d889`):

| Placement Option | Hardware & Storage | Live Occupancy (`2026-11-12` evening) | Suitability & Constraints for `#657` |
| :--- | :--- | :--- | :--- |
| **`DESKTOP-DJ1C40R` (primary desktop)** | i7-13700K (16c/24t, 8P+8E), 128 GiB RAM; `D:` `100.4 GiB` free, `E:` `115.6 GiB` free, `C:` `30.6 GiB` free | WSL2 running `fe-c` (`MTG-Forks-LR3-20261110`, 4 cores claimed on 20-23, unpinned guest vCPUs, marks through `2026-11-14 03:40 ET`) | **Best local build host** (128 GiB RAM handles thin-LTO easily; build and store on `D:` or `E:`, never `C:`). **Blocked for quiet-host benchmarking while `fe-c` runs**; eligible for Deliverable 2 timing (`W = 1, 2, 4, 8` P-cores) in a brief `fe-c` pause window or once `fe-c` completes. |
| **`HaleysPC`** | Ryzen 9 5900XT (16c/32t Zen 4), 32 GiB RAM | Occupied by 5 active training chains | Strong homogeneous Zen 4 cores for multi-worker CPU collection once a chain drains, **but 32 GiB RAM is tight** for thin-LTO builds (ship prebuilt/verified binary or cap `CARGO_BUILD_JOBS=2`), and currently 0 quiet benchmark windows. |
| **`node1` / `node2` (Linux cluster)** | Bare-metal Linux multi-core nodes | Fully occupied (`7` boxes each: Chain A `c1..c6` + `ld-a` on `node1`; `fg` + `CS s1..s3` + `fe-a`/`fe-b` + `fg-b` on `node2`; off-box work slows Chain A by `-8.3%`) | Reserved for Expanded population and transformer runs; **do not place `#657` here** while Chain A / CS occupy the nodes. |
| **`RunPod` Dedicated CPU Pod** | Dedicated AMD EPYC / Ryzen CPU instance (16-32 dedicated vCPUs, 64+ GiB RAM, NVMe) | On-demand within existing cloud credit authority | **Zero contention on demand.** If `DESKTOP-DJ1C40R` and `HaleysPC` remain booked by Expanded forks after the `2026-11-13` Council, provision a bounded dedicated-CPU lease, run Gate 2 (`1w/2w/4w/8w/16w`) + Gate 3 cost check, stream every `step_UUUU.json` to durable local cold copy (`D:`/`E:`), and auto-terminate the pod immediately on `complete.json` or stop gate (`COMPUTE-POLICY.md` Rule 7). |

**Execution sequence once this design note is reviewed**:
1. Land the 2-site `DrakeJournalV1` `of_kinds_v2` fast-path and the multi-iteration campaign + evaluation runner (`kernel_limited_training_v1` / `limited_match_v1` integration) with unit tests on small deterministic fixtures.
2. Run Deliverable 2 (`1w`, `2w`, `4w`, `8w` quiet-host completed-work benchmark + byte-identical replay check) on the earliest quiet `DESKTOP-DJ1C40R` window or authorized dedicated-CPU RunPod pod, and publish the timing receipt and projected campaign hours/cost on `#657`.
3. With `ResearchLead` / maintainer go-ahead on the measured cost manifest, execute Deliverable 3 (`U_max` campaign with 100% checkpoint + provenance retention) and Deliverable 4 (disjoint 40-deck `eval` suite against all four fixed baselines), then submit evaluation artifacts for review prior to any playing-strength claim.
