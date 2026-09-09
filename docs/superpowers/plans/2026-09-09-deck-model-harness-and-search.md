# Deck Model Harness Implementation Plan (W1-W5)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the five infrastructure work items the deck-model-sideboarding design depends on before any search campaign runs: an explicit-deck injection seam on both session types (W1), a paired BO1 win-rate estimator plus a real calibration run (W2), a requires-target/is-counterspell tag file with a live-engine validator (W4), the `GameSummaryV1` game-summary extractor (W3), and the candidate generator, search driver, and hashed search-campaign manifest (W5). This plan stops at the manifest and driver: it does not execute a search campaign (W6), does not touch the runtime catalog (W7), and does not build the deck model itself (W8/W8a/W9).

**Architecture:** `RlEpisodeSessionV1` and `FastActorSessionV1` (`mtg-kernel/src/rl_session.rs`) currently reset only against the compiled-in `RUNTIME_DECKS` catalog by string id (`resolve_runtime_decks`, line 6992). W1 adds a parallel injection seam, `resolve_explicit_decks`, that takes raw `Vec<u16>` mainboards straight from a `SideboardPlanV1`-applied `DeckConfigurationV1` (`src/sideboard.rs`) with no catalog lookup, so a candidate post-board 60 never needs a `RUNTIME_DECKS` promotion to be measured. W2's paired estimator and W5's search driver both build on W1's constructors, always through `ResetRandomization::EnvironmentV2 { pair_environment_seed }` so a candidate and its incumbent share identical shuffles. W3's extractor watermarks `state.engine.event_history` (`src/event.rs` `CommittedEvent`, `src/engine.rs` `EngineState`) against `state.turn` at each decision boundary inside the same episode-driving loop W2 and W5 use, and folds the full history once at episode end. W4 is offline data (`data/`) plus a Rust test that cross-checks it against the live `CARD_DEFS[card_id].target_spec` (`src/card_def.rs`) so a hand-authored tag cannot silently drift from engine truth. W5's manifest is committed and sha256-hashed before any candidate is scored; the driver refuses to run against a manifest whose file hash does not match its own recorded hash.

**Tech Stack:** Rust 2021 (mtg-kernel crate), Python 3 tools under `python/tools` run through `uvx uv@0.11.29 run --no-sync python ...`, no new external dependencies (the crate already depends on `serde`, `serde_json`, `sha2`).

**Spec:** `docs/superpowers/specs/2026-09-09-deck-model-sideboarding-and-brewing-design.md` (revision 4), sections 2, 3, 4, 8, 9, 10. Parent design: `docs/superpowers/specs/2026-09-09-pauper-meta-cards-and-sideboarding-design.md` section 6.

## Global Constraints

- Branch `lead/pauper-meta-cards-v1` only; never commit to `lead/cycle4-refresh-manifest-v1` or `main`. Push after every commit (`push-after-commit` memory: push the branch with `-u origin` if it is not yet tracked).
- Never edit `mtg-kernel/src/flat_policy_v2.rs`. It encodes off `GameState`, never off a deck id or session type, so nothing in W1-W5 has a reason to touch it (design section 2: "so it is untouched by anything below"); if any step in this plan appears to require a `flat_policy_v2.rs` change, stop and escalate rather than editing it.
- `data/runtime_decks_v1.json` must not change in W1-W5 (its SHA-256 is pinned by the training store; W7, the catalog-promotion work, is explicitly out of scope for this plan). Verify with `sha256sum data/runtime_decks_v1.json` before every commit that touches `data/` and confirm the hash is unchanged from the value recorded at the start of Task A.
- No card-specific pending state (ROADMAP rule, restated in the parent design section 3): every new mechanism in this plan (the explicit-deck seam, the estimator, the tag file, the extractor, the search driver) is card-neutral and deck-neutral by construction; none of these tasks add a card program, so this constraint is a guardrail, not a to-do.
- Registry identity is append-only: this plan never edits `data/cards_v1.json`.
- N (BO1 stage) and M (BO3 stage) sample counts, the bootstrap resample count and seed, and every other pre-registered constant this plan's code accepts as a parameter are never hardcoded as "reasonable defaults" inside library code; they are always caller-supplied parameters, with any concrete number appearing only in a test or in the search-campaign manifest produced by Task E. This is the single-shot discipline the design's section 4 acceptance rule depends on ("no repeated testing at partial N, no stopping early on a trend") and it starts at the estimator, not only at the campaign.
- No candidate is ever scored against an unhashed or stale search-campaign manifest (design section 4): Task E's driver reads the manifest file, recomputes its sha256, and refuses to run if the recomputed hash does not match the hash recorded alongside it.
- Cargo builds into `E:/cargo-target-pauper-meta` (`CARGO_TARGET_DIR=E:/cargo-target-pauper-meta`) at below-normal process priority, pinned to the E-cores: a training campaign owns the P-cores on this machine for the duration of this work, so every `cargo build`/`cargo test` invocation in this plan is run through `cmd /c start /belownormal /affinity <E-core-mask> cargo ...` or the PowerShell equivalent (`Start-Process -Verb open -WindowStyle Hidden -ArgumentList ... ; (Get-Process cargo).PriorityClass = 'BelowNormal'`), never bare `cargo build`.
- Python entry point: `uvx uv@0.11.29 run --no-sync python python/tools/<tool>.py` (or `-m unittest ...`) from the worktree root.
- No em-dashes in any file, comment, commit message, or this plan.
- Commit message bodies for any step that changes a hash-bearing file (the search-campaign manifest, the tag file, any test that pins a concrete hash literal) record the old and new hash values.

---

## Task A (W1): explicit-deck injection seam

**Files:**
- Modify: `mtg-kernel/src/rl_session.rs`
  - New free functions, inserted directly after `fn supported_runtime_deck_ids()` (ends line 7020) and before `mod tests` (line 7102): `resolve_explicit_decks`, `explicit_deck_hash_v1`, `build_session_deck_pair_state_from_explicit_decks`.
  - New `impl RlEpisodeSessionV1` methods, inserted directly after `reset_with_decks_and_limits_environment_v2_profiled` (ends line 4062) and before `reset_with_decks_and_limits_profiled_in_audit_mode_with_randomization` (line 4064): `reset_with_explicit_decks_and_limits`, `reset_with_explicit_decks_and_limits_with_starting_player_v1`, `reset_with_explicit_decks_and_limits_with_randomization`.
  - New `impl FastActorSessionV1` methods, inserted directly after `reset_with_decks_and_limits_flat_action_v2_environment_v2_with_starting_player_v1` (ends line 4741) and before `reset_with_decks_and_limits_in_flat_action_mode` (line 4743): `reset_with_explicit_decks_and_limits_flat_action_v2_environment_v2`, `..._with_starting_player_v1`, `reset_with_explicit_decks_and_limits_flat_action_v2_environment_v2_with_randomization`.
  - New unit tests inside `mod tests` (after line 7102), placed near the existing `canonical_v2_full_reset`/`canonical_v2_fast_reset` helpers and the `definition_order` helper (around line 11848, verified above).

**Interfaces:**
- Consumes: `crate::sideboard::REGISTERED_MAINBOARD_SIZE_V1` (`sideboard.rs:15`, `= 60`), `crate::card_def::preflight_fully_supported_deck` (`card_def.rs:1359`, `fn(&[u16]) -> Result<(), DeckPreflightError>`), `crate::rl::build_deck_pair_state_environment_v2` / `build_deck_pair_state_environment_v2_with_starting_player_v1` (`rl.rs:1429, 1449`, both `fn(u64, &[u16], &[u16], ...) -> Result<GameState, DeckPairBuildErrorV2>`), the module-private `fn fnv1a64(bytes: &[u8]) -> u64` already defined at `rl_session.rs:5966`, `map_deck_pair_build_error_v2` (`rl_session.rs:6979`), `session_error`/`RlSessionErrorCode::UnsupportedDeck` (`rl_session.rs:6867, 517`).
- Produces: `resolve_explicit_decks(mainboards: &[Vec<u16>; 2]) -> Result<[Vec<u16>; 2], RlSessionError>`; `RlEpisodeSessionV1::reset_with_explicit_decks_and_limits(episode_id: u64, pair_environment_seed: u64, max_physical_decisions: u64, max_policy_steps: u64, deck_ids: SessionDeckIdsV1, mainboards: [Vec<u16>; 2]) -> Result<Self, RlSessionError>` and its `_with_starting_player_v1(..., starting_player: PlayerId) -> Result<Self, RlSessionError>` sibling; the same two constructors on `FastActorSessionV1` named `reset_with_explicit_decks_and_limits_flat_action_v2_environment_v2[_with_starting_player_v1]`. `deck_ids` on every new constructor is a caller-supplied label pair for receipts/logging only (Task E supplies real labels there); it is never resolved against `RUNTIME_DECKS`. `SessionDeckHashesV1` on the resulting session is computed by the `sorted-explicit` convention (sort the mainboard, `serde_json::to_vec`, `fnv1a64`), which the design (section 2) confirms does not equal the catalog's `xmage_xml_row_then_copy_ordinal/v1` materialization hash for the same 60-card multiset.

- [ ] **Step 1: Confirm the exact anchors before editing anything.**
```
grep -n "^fn supported_runtime_deck_ids\|^mod tests\|^fn resolve_runtime_decks\|^fn session_error\b" mtg-kernel/src/rl_session.rs
grep -n "pub fn reset_with_decks_and_limits_environment_v2\b\|fn reset_with_decks_and_limits_profiled_in_audit_mode_with_randomization\b" mtg-kernel/src/rl_session.rs
grep -n "fn reset_with_decks_and_limits_flat_action_v2_environment_v2_with_starting_player_v1\b\|fn reset_with_decks_and_limits_in_flat_action_mode\b" mtg-kernel/src/rl_session.rs
```
Expected line numbers (re-verified 2026-09-09 against the current tree): `supported_runtime_deck_ids` at 7014, `mod tests` at 7102, `resolve_runtime_decks` at 6992, `session_error` at 6867, `reset_with_decks_and_limits_environment_v2` at 4023, `reset_with_decks_and_limits_profiled_in_audit_mode_with_randomization` at 4064, `reset_with_decks_and_limits_flat_action_v2_environment_v2_with_starting_player_v1` at 4722, `reset_with_decks_and_limits_in_flat_action_mode` at 4743, `fn definition_order` at 11848. If any of these has moved by the time this step runs, use the actual line and update every anchor reference below before continuing; do not silently proceed against stale line numbers.

- [ ] **Step 2: Write the failing tests first**, appended inside `mod tests { ... }` near the existing `fn definition_order` helper (verified present at line 11848, re-grep before inserting since Task A through D's edits may have shifted it):
```rust
    fn burn_and_rally_explicit_mainboards() -> [Vec<u16>; 2] {
        [
            runtime_deck_by_id("Burn").expect("Burn is catalog-registered").card_ids.to_vec(),
            runtime_deck_by_id("Rally").expect("Rally is catalog-registered").card_ids.to_vec(),
        ]
    }

    #[test]
    fn explicit_deck_hash_uses_the_sorted_convention_and_differs_from_the_catalog_hash() {
        let mainboards = burn_and_rally_explicit_mainboards();
        let burn_catalog_hash = runtime_deck_by_id("Burn").unwrap().runtime_deck_hash;
        let rally_catalog_hash = runtime_deck_by_id("Rally").unwrap().runtime_deck_hash;
        let burn_explicit_hash = explicit_deck_hash_v1(&mainboards[0]);
        let rally_explicit_hash = explicit_deck_hash_v1(&mainboards[1]);
        // PASTE_REAL_VALUES_HERE (filled by Step 5 below, empirically, after
        // explicit_deck_hash_v1 exists): this block starts as four eprintln
        // lines, not hardcoded asserts, because none of these four values
        // can be derived by hand and explicit_deck_hash_v1 is new code
        // written in this same task. Step 5 runs this test once with
        // --nocapture, reads the four printed hex values, and replaces this
        // whole block with `assert_eq!(burn_catalog_hash, 0x....);` etc.
        eprintln!("burn_catalog_hash={burn_catalog_hash:#x}");
        eprintln!("rally_catalog_hash={rally_catalog_hash:#x}");
        eprintln!("burn_explicit_hash={burn_explicit_hash:#x}");
        eprintln!("rally_explicit_hash={rally_explicit_hash:#x}");
        assert_ne!(
            burn_explicit_hash, burn_catalog_hash,
            "sorted-explicit and catalog-materialized hashes must not silently coincide"
        );
        assert_ne!(rally_explicit_hash, rally_catalog_hash);
        // Order-insensitivity: a permuted view of the same multiset hashes the same.
        let mut reversed = mainboards[0].clone();
        reversed.reverse();
        assert_eq!(explicit_deck_hash_v1(&reversed), burn_explicit_hash);
    }

    #[test]
    fn resolve_explicit_decks_rejects_the_wrong_mainboard_size() {
        let mut mainboards = burn_and_rally_explicit_mainboards();
        mainboards[1].pop();
        let error = resolve_explicit_decks(&mainboards).expect_err("59 cards must be rejected");
        assert_eq!(error.code, RlSessionErrorCode::UnsupportedDeck);
        assert!(
            error.message.contains("seat 1") && error.message.contains("59"),
            "message identifies the failing seat and actual count: {}",
            error.message
        );
    }

    #[test]
    fn resolve_explicit_decks_rejects_an_unsupported_card_id() {
        let mut mainboards = burn_and_rally_explicit_mainboards();
        mainboards[0][0] = u16::MAX;
        let error = resolve_explicit_decks(&mainboards).expect_err("out-of-range card id must fail preflight");
        assert_eq!(error.code, RlSessionErrorCode::UnsupportedDeck);
        assert!(error.message.contains("seat 0"), "{}", error.message);
    }

    #[test]
    fn explicit_deck_reset_full_episode_completes_with_paired_seed_and_both_starting_players() {
        let mainboards = burn_and_rally_explicit_mainboards();
        let deck_ids = ["ExplicitBurn".to_owned(), "ExplicitRally".to_owned()];
        let root = 0x51de_51de_51de_51de;

        let p0_starts = RlEpisodeSessionV1::reset_with_explicit_decks_and_limits_with_starting_player_v1(
            1, root, 2000, 200_000, deck_ids.clone(), mainboards.clone(), PlayerId::P0,
        )
        .expect("P0-starting explicit-deck reset succeeds");
        assert_eq!(p0_starts.state.active_player, PlayerId::P0);
        let p1_starts = RlEpisodeSessionV1::reset_with_explicit_decks_and_limits_with_starting_player_v1(
            1, root, 2000, 200_000, deck_ids.clone(), mainboards.clone(), PlayerId::P1,
        )
        .expect("P1-starting explicit-deck reset succeeds");
        assert_eq!(p1_starts.state.active_player, PlayerId::P1);
        let plain = RlEpisodeSessionV1::reset_with_explicit_decks_and_limits(
            1, root, 2000, 200_000, deck_ids, mainboards,
        )
        .expect("plain explicit-deck reset succeeds");
        assert_eq!(
            plain.state.active_player, PlayerId::P0,
            "the plain constructor defaults to P0, matching build_deck_pair_state_environment_v2's own default"
        );

        for mut session in [p0_starts, p1_starts] {
            let mut policy_rng = crate::state::SplitMix64::seed(0x9a9a_9a9a_9a9a_9a9a);
            let mut steps = 0u32;
            loop {
                match session.current_response() {
                    RlSessionResponseV1::Terminal(terminal) => {
                        assert!(
                            matches!(
                                terminal.terminal_outcome,
                                TerminalOutcomeV1::P0Win | TerminalOutcomeV1::P1Win | TerminalOutcomeV1::Draw
                            ),
                            "episode reaches a well-formed terminal, not a halt"
                        );
                        break;
                    }
                    RlSessionResponseV1::Decision(decision) => {
                        assert!(steps < 200_000, "episode must terminate inside the policy-step cap");
                        steps += 1;
                        let selected_index = (policy_rng.next_u64() as usize) % decision.legal_actions.len();
                        let selected_action_id = decision.legal_actions[selected_index].stable_id.clone();
                        session
                            .step(decision.episode_id, decision.step, selected_index as u32, &selected_action_id)
                            .expect("random-policy step succeeds");
                    }
                }
            }
        }
    }

    #[test]
    fn explicit_deck_reset_shares_the_identical_shuffle_for_a_fixed_pair_environment_seed() {
        let mainboards = burn_and_rally_explicit_mainboards();
        let deck_ids = ["ExplicitBurn".to_owned(), "ExplicitRally".to_owned()];
        let a = RlEpisodeSessionV1::reset_with_explicit_decks_and_limits(
            1, 0x1234_5678_9abc_def0, 8, 1024, deck_ids.clone(), mainboards.clone(),
        )
        .unwrap();
        let b = RlEpisodeSessionV1::reset_with_explicit_decks_and_limits(
            1, 0x1234_5678_9abc_def0, 8, 1024, deck_ids.clone(), mainboards.clone(),
        )
        .unwrap();
        assert_eq!(definition_order(&a.state, PlayerId::P0), definition_order(&b.state, PlayerId::P0));
        assert_eq!(definition_order(&a.state, PlayerId::P1), definition_order(&b.state, PlayerId::P1));
        let c = RlEpisodeSessionV1::reset_with_explicit_decks_and_limits(
            1, 0x1234_5678_9abc_def1, 8, 1024, deck_ids, mainboards,
        )
        .unwrap();
        assert_ne!(
            definition_order(&a.state, PlayerId::P0), definition_order(&c.state, PlayerId::P0),
            "a different pair_environment_seed must not coincidentally reproduce the same shuffle"
        );
    }
```

- [ ] **Step 3: Run the new tests and confirm they fail to compile** (the functions and constructors do not exist yet):
```
cmd /c start /belownormal /affinity FF /wait cargo test --locked -p mtg-kernel --lib rl_session::tests::explicit_deck 2>&1 | tail -40
```
Expected: `error[E0433]` / `error[E0599]` naming `resolve_explicit_decks`, `explicit_deck_hash_v1`, `reset_with_explicit_decks_and_limits*` as not found.

- [ ] **Step 4: Implement the three free functions**, inserted after line 7020 (`fn supported_runtime_deck_ids`):
```rust
fn resolve_explicit_decks(
    mainboards: &[Vec<u16>; 2],
) -> Result<[Vec<u16>; 2], RlSessionError> {
    for (seat, mainboard) in mainboards.iter().enumerate() {
        if mainboard.len() != crate::sideboard::REGISTERED_MAINBOARD_SIZE_V1 {
            return Err(session_error(
                RlSessionErrorCode::UnsupportedDeck,
                &format!(
                    "explicit deck for seat {seat} must contain exactly {} cards, got {}",
                    crate::sideboard::REGISTERED_MAINBOARD_SIZE_V1,
                    mainboard.len()
                ),
            ));
        }
        crate::card_def::preflight_fully_supported_deck(mainboard).map_err(|error| {
            session_error(
                RlSessionErrorCode::UnsupportedDeck,
                &format!("seat {seat} explicit deck failed full-support preflight: {error}"),
            )
        })?;
    }
    Ok([mainboards[0].clone(), mainboards[1].clone()])
}

/// The `sorted-explicit` hash convention (design section 2): sort the
/// mainboard, serialize as a JSON u16 array, FNV-1a it with the same
/// algorithm and constant `build.rs` uses for the catalog's
/// `fnv1a64-serde-json-u16-array/v1` convention. Deliberately does not
/// reuse `RuntimeDeckDefinition::runtime_deck_hash`: that hash is over the
/// unsorted `materialized_mainboard` walk order, a different convention.
fn explicit_deck_hash_v1(mainboard: &[u16]) -> u64 {
    let mut sorted = mainboard.to_vec();
    sorted.sort_unstable();
    let serialized = serde_json::to_vec(&sorted).expect("a u16 vector always serializes as JSON");
    fnv1a64(&serialized)
}

/// Explicit-deck sibling of `build_session_deck_pair_state`: identical
/// dispatch to the environment-v2 deck-pair builders, sourced from two
/// caller-supplied 60-card mainboards instead of a `RUNTIME_DECKS` lookup.
/// Always builds on `ResetRandomization::EnvironmentV2`; there is no legacy
/// explicit-deck path because every consumer of this seam (the paired
/// estimator, the search driver) needs the paired-seed mechanism.
fn build_session_deck_pair_state_from_explicit_decks(
    mainboards: &[Vec<u16>; 2],
    pair_environment_seed: u64,
    starting_player: Option<PlayerId>,
) -> Result<(SessionDeckHashesV1, crate::state::GameState), RlSessionError> {
    let resolved = resolve_explicit_decks(mainboards)?;
    let deck_hashes = [
        explicit_deck_hash_v1(&resolved[0]),
        explicit_deck_hash_v1(&resolved[1]),
    ];
    let state = match starting_player {
        None => crate::rl::build_deck_pair_state_environment_v2(
            pair_environment_seed,
            &resolved[0],
            &resolved[1],
        )
        .map_err(map_deck_pair_build_error_v2)?,
        Some(starting_player) => {
            crate::rl::build_deck_pair_state_environment_v2_with_starting_player_v1(
                pair_environment_seed,
                &resolved[0],
                &resolved[1],
                starting_player,
            )
            .map_err(map_deck_pair_build_error_v2)?
        }
    };
    Ok((deck_hashes, state))
}
```

- [ ] **Step 5: Derive the four hash literals empirically and finalize the test** (mirrors Task B's Step 4-5 calibration pattern: measure first, hardcode only the measured value). Run just the hash test with output captured:
```
cmd /c start /belownormal /affinity FF /wait cargo test --locked -p mtg-kernel --lib rl_session::tests::explicit_deck_hash_uses_the_sorted_convention_and_differs_from_the_catalog_hash -- --nocapture 2>&1 | tail -20
```
Expected: the test passes (the `assert_ne!`/`assert_eq!` structural checks already in the test body hold) and prints four lines, `burn_catalog_hash=0x...`, `rally_catalog_hash=0x...`, `burn_explicit_hash=0x...`, `rally_explicit_hash=0x...`. Copy those four printed values and replace the `PASTE_REAL_VALUES_HERE` block from Step 2 (the four `eprintln!` lines and this comment) with:
```rust
        assert_eq!(burn_catalog_hash, 0x_____________); // pasted from the eprintln above
        assert_eq!(rally_catalog_hash, 0x_____________);
        assert_eq!(burn_explicit_hash, 0x_____________);
        assert_eq!(rally_explicit_hash, 0x_____________);
```
using the real printed hex digits in place of the underscores. Rerun the same command (without `--nocapture` is fine now) and confirm PASS with the hardcoded asserts in place. Record the four values in this step's commit-adjacent notes; Step 10's commit message body cites them as newly-pinned hashes per the Global Constraints rule on hash-bearing commits.

- [ ] **Step 6: Implement the `RlEpisodeSessionV1` constructors**, inserted after line 4062 (immediately before `reset_with_decks_and_limits_profiled_in_audit_mode_with_randomization`):
```rust
    /// Explicit-deck sibling of `reset_with_decks_and_limits_environment_v2`
    /// (design section 2, W1): the harness's deck-injection seam. `deck_ids`
    /// is a caller-supplied label pair for receipts and logs only; it is
    /// never resolved against `RUNTIME_DECKS`. Defaults to P0 starting,
    /// matching `build_deck_pair_state_environment_v2`'s own default.
    pub fn reset_with_explicit_decks_and_limits(
        episode_id: u64,
        pair_environment_seed: u64,
        max_physical_decisions: u64,
        max_policy_steps: u64,
        deck_ids: SessionDeckIdsV1,
        mainboards: [Vec<u16>; 2],
    ) -> Result<Self, RlSessionError> {
        Self::reset_with_explicit_decks_and_limits_with_randomization(
            episode_id,
            pair_environment_seed,
            max_physical_decisions,
            max_policy_steps,
            deck_ids,
            mainboards,
            None,
        )
    }

    /// Starting-player-aware sibling of the constructor above
    /// (`P1-METAMORPHIC-AUDIT-DESIGN-V4.md` Section 1.2's discipline, design
    /// section 2). W6's BO3 ratification stage and W8a's self-play driver
    /// pass `PreparedMatchGameV1::start().starting_player` here, never the
    /// plain constructor.
    pub fn reset_with_explicit_decks_and_limits_with_starting_player_v1(
        episode_id: u64,
        pair_environment_seed: u64,
        max_physical_decisions: u64,
        max_policy_steps: u64,
        deck_ids: SessionDeckIdsV1,
        mainboards: [Vec<u16>; 2],
        starting_player: PlayerId,
    ) -> Result<Self, RlSessionError> {
        Self::reset_with_explicit_decks_and_limits_with_randomization(
            episode_id,
            pair_environment_seed,
            max_physical_decisions,
            max_policy_steps,
            deck_ids,
            mainboards,
            Some(starting_player),
        )
    }

    fn reset_with_explicit_decks_and_limits_with_randomization(
        episode_id: u64,
        pair_environment_seed: u64,
        max_physical_decisions: u64,
        max_policy_steps: u64,
        deck_ids: SessionDeckIdsV1,
        mainboards: [Vec<u16>; 2],
        starting_player: Option<PlayerId>,
    ) -> Result<Self, RlSessionError> {
        let (deck_hashes, state) = build_session_deck_pair_state_from_explicit_decks(
            &mainboards,
            pair_environment_seed,
            starting_player,
        )?;
        let mut session = RlEpisodeSessionV1 {
            deck_ids,
            deck_hashes,
            episode_id,
            max_physical_decisions,
            max_policy_steps,
            state,
            surface: PolicySurfaceV5::new_for_session(),
            environment_revision: 0,
            policy_step_count: 0,
            physical_decision_count: 0,
            current: None,
            terminal: None,
        };
        session.advance_to_decision_or_terminal_profiled(None);
        Ok(session)
    }
```

- [ ] **Step 7: Implement the `FastActorSessionV1` constructors**, inserted after line 4741 (immediately before `reset_with_decks_and_limits_in_flat_action_mode`):
```rust
    /// Explicit-deck sibling of
    /// `reset_with_decks_and_limits_flat_action_v2_environment_v2` (W1).
    /// Always `FlatActionContractModeV1::V2`: every explicit-deck consumer
    /// (the paired estimator, the search driver) drives this session type
    /// for speed and has no reason to select the legacy flat-action
    /// contract.
    pub fn reset_with_explicit_decks_and_limits_flat_action_v2_environment_v2(
        episode_id: u64,
        pair_environment_seed: u64,
        max_physical_decisions: u64,
        max_policy_steps: u64,
        deck_ids: SessionDeckIdsV1,
        mainboards: [Vec<u16>; 2],
    ) -> Result<Self, RlSessionError> {
        Self::reset_with_explicit_decks_and_limits_flat_action_v2_environment_v2_with_randomization(
            episode_id,
            pair_environment_seed,
            max_physical_decisions,
            max_policy_steps,
            deck_ids,
            mainboards,
            None,
        )
    }

    pub fn reset_with_explicit_decks_and_limits_flat_action_v2_environment_v2_with_starting_player_v1(
        episode_id: u64,
        pair_environment_seed: u64,
        max_physical_decisions: u64,
        max_policy_steps: u64,
        deck_ids: SessionDeckIdsV1,
        mainboards: [Vec<u16>; 2],
        starting_player: PlayerId,
    ) -> Result<Self, RlSessionError> {
        Self::reset_with_explicit_decks_and_limits_flat_action_v2_environment_v2_with_randomization(
            episode_id,
            pair_environment_seed,
            max_physical_decisions,
            max_policy_steps,
            deck_ids,
            mainboards,
            Some(starting_player),
        )
    }

    fn reset_with_explicit_decks_and_limits_flat_action_v2_environment_v2_with_randomization(
        episode_id: u64,
        pair_environment_seed: u64,
        max_physical_decisions: u64,
        max_policy_steps: u64,
        deck_ids: SessionDeckIdsV1,
        mainboards: [Vec<u16>; 2],
        starting_player: Option<PlayerId>,
    ) -> Result<Self, RlSessionError> {
        let (deck_hashes, state) = build_session_deck_pair_state_from_explicit_decks(
            &mainboards,
            pair_environment_seed,
            starting_player,
        )?;
        let mut session = FastActorSessionV1 {
            deck_ids,
            deck_hashes,
            episode_id,
            max_physical_decisions,
            max_policy_steps,
            state,
            surface: PolicySurfaceV5::new_for_session(),
            environment_revision: 0,
            policy_step_count: 0,
            physical_decision_count: 0,
            current: None,
            flat_action_contract_mode: FlatActionContractModeV1::V2,
            flat_action_cache_spare: None,
            flat_action_cache_spare_v2: None,
            terminal: None,
        };
        session.advance_to_decision_or_terminal();
        Ok(session)
    }
```

- [ ] **Step 8: Run the new tests.**
```
cmd /c start /belownormal /affinity FF /wait cargo test --locked -p mtg-kernel --lib rl_session::tests::explicit_deck 2>&1 | tail -60
```
Expected: 5 tests pass: `explicit_deck_hash_uses_the_sorted_convention_and_differs_from_the_catalog_hash`, `resolve_explicit_decks_rejects_the_wrong_mainboard_size`, `resolve_explicit_decks_rejects_an_unsupported_card_id`, `explicit_deck_reset_full_episode_completes_with_paired_seed_and_both_starting_players`, `explicit_deck_reset_shares_the_identical_shuffle_for_a_fixed_pair_environment_seed`.

- [ ] **Step 9: Run the full crate suite** to confirm nothing existing regressed (the new code adds functions and methods; it does not modify any existing one):
```
cmd /c start /belownormal /affinity FF /wait cargo test --locked -p mtg-kernel 2>&1 | tail -30
sha256sum data/runtime_decks_v1.json
```
Expected: full suite PASS; the runtime-decks hash is unchanged from the value recorded before this task started.

- [ ] **Step 10: Commit and push**, with the four hash literals from Step 5 in the body:
```
git add mtg-kernel/src/rl_session.rs
git commit -m "rl_session: explicit-deck injection seam (W1), paired-seed and starting-player-aware

burn_catalog_hash, rally_catalog_hash, burn_explicit_hash, rally_explicit_hash: <values from Step 5>"
git push -u origin lead/pauper-meta-cards-v1
```

---

## Task B (W2): paired BO1 estimator and calibration run

**Files:**
- Create: `mtg-kernel/src/paired_bo1_harness_v1.rs` (new library module; add `pub mod paired_bo1_harness_v1;` to `mtg-kernel/src/lib.rs` in whatever alphabetical/grouped position the existing `pub mod` list uses, verified by `grep -n "^pub mod " mtg-kernel/src/lib.rs` before editing)
- Create: `python/tools/paired_sideboard_estimator_v1.py`
- Create: `python/tests/test_paired_sideboard_estimator_v1.py`

**Interfaces:**
- Consumes: Task A's `FastActorSessionV1::reset_with_explicit_decks_and_limits_flat_action_v2_environment_v2_with_starting_player_v1`, `FastActorResponseV1::{Decision, Terminal}`, `RlSessionTerminalV1.winner: Option<PlayerSeatV1>`, `crate::state::SplitMix64`.
- Produces (Rust): `pub struct PairedTrialOutcomeV1 { pub delta: i8 }` (`delta` in `{-1, 0, 1}`, `win(candidate) - win(incumbent)`); `pub fn run_paired_bo1_trial_v1(candidate_mainboard: &[u16], incumbent_mainboard: &[u16], opponent_mainboard: &[u16], candidate_and_incumbent_seat: PlayerId, pair_environment_seed: u64, starting_player: PlayerId, max_physical_decisions: u64, policy_fn: &mut dyn FnMut(&FastActorDecisionV1) -> u32) -> Result<PairedTrialOutcomeV1, RlSessionError>`; `pub fn paired_mean_delta_v1(outcomes: &[PairedTrialOutcomeV1]) -> f64`; `pub enum BootstrapSidednessV1 { OneSidedLower, TwoSided }`; `pub struct PairedBootstrapResultV1 { pub mean: f64, pub lower: f64, pub upper: f64, pub resample_count: u32, pub seed: u64, pub sidedness: BootstrapSidednessV1 }`; `pub fn paired_bootstrap_ci_v1(deltas: &[i8], resample_count: u32, seed: u64, sidedness: BootstrapSidednessV1) -> PairedBootstrapResultV1`.
- Produces (Python): `paired_sideboard_estimator_v1.py --deltas-jsonl <path> --resample-count N --seed S --sidedness one_sided_lower|two_sided` prints a JSON report `{"mean": ..., "lower": ..., "upper": ..., "resample_count": N, "seed": S, "sidedness": ...}` to stdout, an independent (not shared-code) reimplementation of the same case-resampling algorithm, so the Rust and Python numbers can be cross-checked against each other on real receipts data in W6.

- [ ] **Step 1: Confirm the module list anchor and write the failing Rust tests.**
```
grep -n "^pub mod " mtg-kernel/src/lib.rs | tail -20
```
Create `mtg-kernel/src/paired_bo1_harness_v1.rs`:
```rust
//! Paired BO1 win-rate estimator (design section 2, W2): a candidate and
//! its incumbent are played against the same opponent under the identical
//! `pair_environment_seed`, so shared randomness cancels in the paired
//! difference. This module owns only the estimator (the trial runner and
//! the bootstrap), not the search loop that calls it (Task E) and not the
//! candidate generator (also Task E).

use crate::ids::PlayerId;
use crate::rl_session::{FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1, RlSessionError};
use crate::state::SplitMix64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PairedTrialOutcomeV1 {
    pub delta: i8,
}

pub fn run_paired_bo1_trial_v1(
    candidate_mainboard: &[u16],
    incumbent_mainboard: &[u16],
    opponent_mainboard: &[u16],
    candidate_and_incumbent_seat: PlayerId,
    pair_environment_seed: u64,
    starting_player: PlayerId,
    max_physical_decisions: u64,
    policy_fn: &mut dyn FnMut(&FastActorDecisionV1) -> u32,
) -> Result<PairedTrialOutcomeV1, RlSessionError> {
    let candidate_win = play_one_side_v1(
        candidate_mainboard,
        opponent_mainboard,
        candidate_and_incumbent_seat,
        pair_environment_seed,
        starting_player,
        max_physical_decisions,
        policy_fn,
    )?;
    let incumbent_win = play_one_side_v1(
        incumbent_mainboard,
        opponent_mainboard,
        candidate_and_incumbent_seat,
        pair_environment_seed,
        starting_player,
        max_physical_decisions,
        policy_fn,
    )?;
    let delta = i8::from(candidate_win) - i8::from(incumbent_win);
    Ok(PairedTrialOutcomeV1 { delta })
}

fn play_one_side_v1(
    self_mainboard: &[u16],
    opponent_mainboard: &[u16],
    self_seat: PlayerId,
    pair_environment_seed: u64,
    starting_player: PlayerId,
    max_physical_decisions: u64,
    policy_fn: &mut dyn FnMut(&FastActorDecisionV1) -> u32,
) -> Result<bool, RlSessionError> {
    let mainboards = match self_seat {
        PlayerId::P0 => [self_mainboard.to_vec(), opponent_mainboard.to_vec()],
        PlayerId::P1 => [opponent_mainboard.to_vec(), self_mainboard.to_vec()],
        other => panic!("unsupported seat {}", other.0),
    };
    let deck_ids = ["candidate_or_incumbent".to_owned(), "opponent".to_owned()];
    let mut session = FastActorSessionV1::reset_with_explicit_decks_and_limits_flat_action_v2_environment_v2_with_starting_player_v1(
        1,
        pair_environment_seed,
        max_physical_decisions,
        max_physical_decisions.saturating_mul(128).max(1),
        deck_ids,
        mainboards,
        starting_player,
    )?;
    loop {
        match session.current_response() {
            FastActorResponseV1::Terminal(terminal) => {
                return Ok(terminal.winner == Some(self_seat.into()));
            }
            FastActorResponseV1::Decision(decision) => {
                let selected_index = policy_fn(&decision) % decision.legal_action_count;
                session.step(decision.episode_id, decision.step, selected_index)?;
            }
        }
    }
}

pub fn paired_mean_delta_v1(outcomes: &[PairedTrialOutcomeV1]) -> f64 {
    assert!(!outcomes.is_empty(), "mean delta is undefined over zero trials");
    outcomes.iter().map(|outcome| f64::from(outcome.delta)).sum::<f64>() / outcomes.len() as f64
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BootstrapSidednessV1 {
    OneSidedLower,
    TwoSided,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PairedBootstrapResultV1 {
    pub mean: f64,
    pub lower: f64,
    pub upper: f64,
    pub resample_count: u32,
    pub seed: u64,
    pub sidedness: BootstrapSidednessV1,
}

/// Case resampling with replacement over the paired per-seed deltas
/// (design section 4's manifest field: "resampling method"). `resample_count`
/// and `seed` are always caller-supplied, never a library default, per this
/// plan's Global Constraints (single-shot discipline).
pub fn paired_bootstrap_ci_v1(
    deltas: &[i8],
    resample_count: u32,
    seed: u64,
    sidedness: BootstrapSidednessV1,
) -> PairedBootstrapResultV1 {
    assert!(!deltas.is_empty(), "bootstrap is undefined over zero deltas");
    assert!(resample_count > 0, "resample_count must be positive");
    let mean = deltas.iter().map(|&delta| f64::from(delta)).sum::<f64>() / deltas.len() as f64;
    let mut rng = SplitMix64::seed(seed);
    let mut resample_means: Vec<f64> = Vec::with_capacity(resample_count as usize);
    for _ in 0..resample_count {
        let mut sum = 0.0f64;
        for _ in 0..deltas.len() {
            let index = (rng.next_u64() as usize) % deltas.len();
            sum += f64::from(deltas[index]);
        }
        resample_means.push(sum / deltas.len() as f64);
    }
    resample_means.sort_by(|a, b| a.partial_cmp(b).expect("resample means are never NaN"));
    let (lower, upper) = match sidedness {
        BootstrapSidednessV1::OneSidedLower => {
            let alpha_index = ((resample_count as f64) * 0.05).floor() as usize;
            (resample_means[alpha_index.min(resample_means.len() - 1)], f64::INFINITY)
        }
        BootstrapSidednessV1::TwoSided => {
            let lower_index = ((resample_count as f64) * 0.025).floor() as usize;
            let upper_index = ((resample_count as f64) * 0.975).floor() as usize;
            (
                resample_means[lower_index.min(resample_means.len() - 1)],
                resample_means[upper_index.min(resample_means.len() - 1)],
            )
        }
    };
    PairedBootstrapResultV1 { mean, lower, upper, resample_count, seed, sidedness }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_decks::runtime_deck_by_id;

    #[test]
    fn paired_mean_delta_matches_hand_computed_value() {
        let outcomes = [1, 1, 0, -1, 1].map(|delta| PairedTrialOutcomeV1 { delta });
        assert!((paired_mean_delta_v1(&outcomes) - 0.4).abs() < 1e-12);
    }

    #[test]
    fn paired_bootstrap_ci_one_sided_lower_bound_is_below_the_sample_mean_and_reproducible() {
        let deltas = [1i8, 1, 1, 0, -1, 1, 1, 0, 1, 1];
        let a = paired_bootstrap_ci_v1(&deltas, 2000, 42, BootstrapSidednessV1::OneSidedLower);
        let b = paired_bootstrap_ci_v1(&deltas, 2000, 42, BootstrapSidednessV1::OneSidedLower);
        assert_eq!(a, b, "same seed and resample_count must reproduce bit-identically");
        assert!(a.lower <= a.mean, "the one-sided lower bound never exceeds the sample mean");
        assert!(a.upper.is_infinite());
        let c = paired_bootstrap_ci_v1(&deltas, 2000, 43, BootstrapSidednessV1::OneSidedLower);
        assert_ne!(a.lower, c.lower, "a different seed must not coincidentally reproduce the same bound");
    }

    #[test]
    fn paired_bootstrap_ci_two_sided_brackets_the_mean_for_a_clearly_positive_sample() {
        let deltas = [1i8; 20];
        let result = paired_bootstrap_ci_v1(&deltas, 2000, 7, BootstrapSidednessV1::TwoSided);
        assert_eq!(result.mean, 1.0);
        assert_eq!(result.lower, 1.0);
        assert_eq!(result.upper, 1.0);
    }

    #[test]
    fn run_paired_bo1_trial_completes_and_delta_is_in_range() {
        let candidate = runtime_deck_by_id("Burn").unwrap().card_ids.to_vec();
        let incumbent = runtime_deck_by_id("Burn").unwrap().card_ids.to_vec();
        let opponent = runtime_deck_by_id("Rally").unwrap().card_ids.to_vec();
        let mut rng = SplitMix64::seed(0x1111_2222_3333_4444);
        let mut policy = |decision: &FastActorDecisionV1| {
            (rng.next_u64() as u32) % decision.legal_action_count.max(1)
        };
        let outcome = run_paired_bo1_trial_v1(
            &candidate, &incumbent, &opponent, PlayerId::P0,
            0x5050_5050_5050_5050, PlayerId::P0, 2000, &mut policy,
        )
        .expect("paired trial completes");
        assert!((-1..=1).contains(&outcome.delta));
    }
}
```

- [ ] **Step 2: Register the module and run the failing build.**
```
grep -n "^pub mod paired_bo1_harness_v1" mtg-kernel/src/lib.rs || sed -i '/^pub mod pauper_meta_gap_v1;/a pub mod paired_bo1_harness_v1;' mtg-kernel/src/lib.rs
```
(Use whatever real neighboring `pub mod` line the Step 1 grep found; the sed target above is illustrative and must be replaced with an actual line from this crate's `lib.rs`.)
```
cmd /c start /belownormal /affinity FF /wait cargo test --locked -p mtg-kernel --lib paired_bo1_harness_v1 2>&1 | tail -60
```
Expected: compiles and the four tests run (the module is new so there is no separate "red" state beyond normal TDD inside this one file; treat a first `cargo check` failure on a typo as the red step and fix it, then confirm all four tests pass).

- [ ] **Step 3: Fix any real compile errors** (an incorrect `use` path, `FastActorDecisionV1`/`FastActorResponseV1` not `pub` from `rl_session.rs`: check with `grep -n "pub struct FastActorDecisionV1\|pub enum FastActorResponseV1" mtg-kernel/src/rl_session.rs` and adjust the `use` line to match, adding `pub` to either type in `rl_session.rs` if either is not already public) and rerun until Step 2's command exits 0.

- [ ] **Step 4: Write the calibration run as an `--ignored` test** in the same file (below the `mod tests` block from Step 1), following this codebase's existing convention for measurement harnesses (`kernel_native_search_calibration_runner_v1.rs`'s module doc: an `--ignored`-test launcher, invoked by exact name, not a new `[[bin]]` target, "every other checkpoint-runner-driven measurement probe in this codebase... is an `--ignored` test invoked by exact name"):
```rust
#[cfg(test)]
mod calibration {
    use super::*;
    use crate::runtime_decks::runtime_deck_by_id;
    use std::time::Instant;

    /// Real per-game wall-clock calibration for the paired BO1 harness
    /// (design section 2's "one calibration run measures real per-game wall
    /// clock under the frozen checkpoint before any campaign-wide N... is
    /// set"). Drives a random policy (no trained checkpoint is wired by
    /// this plan; W6 substitutes the real inference policy_fn) so the
    /// number this produces is a harness-cost floor, not the final N input;
    /// the manifest's N still comes from the formula in Task E, not from
    /// this number alone.
    #[test]
    #[ignore]
    fn calibration_measures_real_per_game_wall_clock_v1() {
        let candidate = runtime_deck_by_id("Burn").unwrap().card_ids.to_vec();
        let opponent = runtime_deck_by_id("Rally").unwrap().card_ids.to_vec();
        let game_count = 50usize;
        let mut rng = SplitMix64::seed(0xC001_C001_C001_C001);
        let started = Instant::now();
        for episode in 0..game_count {
            let mut policy_rng = SplitMix64::seed(rng.next_u64());
            let mut policy = |decision: &FastActorDecisionV1| {
                (policy_rng.next_u64() as u32) % decision.legal_action_count.max(1)
            };
            run_paired_bo1_trial_v1(
                &candidate, &candidate, &opponent, PlayerId::P0,
                rng.next_u64(), PlayerId::P0, 2000, &mut policy,
            )
            .unwrap_or_else(|error| panic!("calibration episode {episode} failed: {error}"));
        }
        let elapsed = started.elapsed();
        let mean_seconds_per_paired_trial = elapsed.as_secs_f64() / game_count as f64;
        let report = serde_json::json!({
            "schema": "paired_bo1_calibration/v1",
            "paired_trial_count": game_count,
            "total_seconds": elapsed.as_secs_f64(),
            "mean_seconds_per_paired_trial": mean_seconds_per_paired_trial,
            "mean_seconds_per_game": mean_seconds_per_paired_trial / 2.0,
        });
        let path = std::path::Path::new("docs/research/paired_bo1_calibration_2026-09-09.json");
        std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap())
            .expect("calibration report writes");
        eprintln!("wrote {}", path.display());
    }
}
```

- [ ] **Step 5: Run the calibration and inspect the artifact.**
```
cmd /c start /belownormal /affinity FF /wait cargo test --locked -p mtg-kernel --lib paired_bo1_harness_v1::calibration::calibration_measures_real_per_game_wall_clock_v1 -- --ignored --nocapture
cat docs/research/paired_bo1_calibration_2026-09-09.json
```
Expected: the test prints `wrote docs/research/paired_bo1_calibration_2026-09-09.json`, and the file contains a positive `mean_seconds_per_paired_trial`. Record this number in the commit body for Step 9; it is the real per-game cost input the design's N-derivation formula (Task E) needs, not a placeholder.

- [ ] **Step 6: Write the Python analysis's failing test.** Create `python/tests/test_paired_sideboard_estimator_v1.py`:
```python
import json
import subprocess
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TOOL = ROOT / "python/tools/paired_sideboard_estimator_v1.py"


class PairedSideboardEstimator(unittest.TestCase):
    def _run(self, deltas, resample_count, seed, sidedness):
        deltas_path = Path(self.tmp) / "deltas.jsonl"
        deltas_path.write_text("\n".join(json.dumps({"delta": d}) for d in deltas), encoding="utf-8")
        result = subprocess.run(
            [
                sys.executable, str(TOOL),
                "--deltas-jsonl", str(deltas_path),
                "--resample-count", str(resample_count),
                "--seed", str(seed),
                "--sidedness", sidedness,
            ],
            cwd=ROOT, capture_output=True, text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return json.loads(result.stdout)

    def setUp(self):
        import tempfile
        self.tmp = tempfile.mkdtemp()

    def test_mean_matches_hand_computed_value(self):
        report = self._run([1, 1, 0, -1, 1], resample_count=500, seed=1, sidedness="one_sided_lower")
        self.assertAlmostEqual(report["mean"], 0.4, places=12)

    def test_two_sided_brackets_a_constant_series_exactly(self):
        report = self._run([1] * 20, resample_count=500, seed=1, sidedness="two_sided")
        self.assertEqual(report["mean"], 1.0)
        self.assertEqual(report["lower"], 1.0)
        self.assertEqual(report["upper"], 1.0)

    def test_same_seed_reproduces_bit_identically(self):
        a = self._run([1, 0, -1, 1, 1, 0, 1], resample_count=1000, seed=99, sidedness="one_sided_lower")
        b = self._run([1, 0, -1, 1, 1, 0, 1], resample_count=1000, seed=99, sidedness="one_sided_lower")
        self.assertEqual(a, b)


if __name__ == "__main__":
    unittest.main()
```
Run it: `uvx uv@0.11.29 run --no-sync python -m unittest python.tests.test_paired_sideboard_estimator_v1 -v`. Expected: FAIL (`python/tools/paired_sideboard_estimator_v1.py` does not exist).

- [ ] **Step 7: Implement the Python tool.** Create `python/tools/paired_sideboard_estimator_v1.py`:
```python
#!/usr/bin/env python3
"""Independent Python cross-check of the Rust paired-bootstrap estimator
(mtg-kernel/src/paired_bo1_harness_v1.rs). Reimplements the same
case-resampling algorithm from scratch so a Rust bug and a Python bug are
unlikely to agree; the two are compared against each other on real receipts
in W6, not merged into one shared implementation.
"""
from __future__ import annotations

import argparse
import json
import sys


def splitmix64(seed: int):
    state = seed & 0xFFFFFFFFFFFFFFFF

    def next_u64() -> int:
        nonlocal state
        state = (state + 0x9E3779B97F4A7C15) & 0xFFFFFFFFFFFFFFFF
        z = state
        z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & 0xFFFFFFFFFFFFFFFF
        z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & 0xFFFFFFFFFFFFFFFF
        return z ^ (z >> 31)

    return next_u64


def paired_bootstrap_ci(deltas: list[int], resample_count: int, seed: int, sidedness: str) -> dict:
    if not deltas:
        raise ValueError("bootstrap is undefined over zero deltas")
    if resample_count <= 0:
        raise ValueError("resample_count must be positive")
    mean = sum(deltas) / len(deltas)
    next_u64 = splitmix64(seed)
    resample_means = []
    n = len(deltas)
    for _ in range(resample_count):
        total = 0
        for _ in range(n):
            index = next_u64() % n
            total += deltas[index]
        resample_means.append(total / n)
    resample_means.sort()
    if sidedness == "one_sided_lower":
        alpha_index = min(int(resample_count * 0.05), resample_count - 1)
        lower = resample_means[alpha_index]
        upper = float("inf")
    elif sidedness == "two_sided":
        lower_index = min(int(resample_count * 0.025), resample_count - 1)
        upper_index = min(int(resample_count * 0.975), resample_count - 1)
        lower = resample_means[lower_index]
        upper = resample_means[upper_index]
    else:
        raise ValueError(f"unknown sidedness {sidedness!r}")
    return {
        "mean": mean,
        "lower": lower,
        "upper": upper,
        "resample_count": resample_count,
        "seed": seed,
        "sidedness": sidedness,
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--deltas-jsonl", required=True)
    parser.add_argument("--resample-count", type=int, required=True)
    parser.add_argument("--seed", type=int, required=True)
    parser.add_argument("--sidedness", choices=["one_sided_lower", "two_sided"], required=True)
    args = parser.parse_args(argv)

    deltas = []
    with open(args.deltas_jsonl, encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if not line:
                continue
            deltas.append(int(json.loads(line)["delta"]))

    report = paired_bootstrap_ci(deltas, args.resample_count, args.seed, args.sidedness)
    json.dump(report, sys.stdout)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
```

- [ ] **Step 8: Run the Python test.** `uvx uv@0.11.29 run --no-sync python -m unittest python.tests.test_paired_sideboard_estimator_v1 -v`. Expected: PASS.

- [ ] **Step 9: Commit and push**, with the calibration number from Step 5 in the body:
```
git add mtg-kernel/src/lib.rs mtg-kernel/src/paired_bo1_harness_v1.rs python/tools/paired_sideboard_estimator_v1.py python/tests/test_paired_sideboard_estimator_v1.py docs/research/paired_bo1_calibration_2026-09-09.json
git commit -m "harness: paired BO1 estimator and bootstrap CI (W2), calibration run

mean_seconds_per_paired_trial <value from Step 5>"
git push
```

---

## Task C (W4): requires-target/is-counterspell tag file and validator

**Files:**
- Create: `data/pauper_removal_counterspell_tags_v1.json` (generated output, committed)
- Create: `python/tools/generate_removal_counterspell_tags_v1.py`
- Create: `python/tests/test_generate_removal_counterspell_tags_v1.py`
- Create: `mtg-kernel/tests/removal_counterspell_tags_v1.rs` (the live-engine validator: the design's ratified defense against "an unreviewed or drifted tag silently corrupts the field with no failing test", section 9 risk)

**Interfaces:**
- Consumes: `data/cards_v1.json` `cards[].mechanics: list[str]` (verified format: e.g. Annul carries `["counter_target", "artifact_hate"]`); `mtg_kernel::card_def::CARD_DEFS[card_id].target_spec: TargetSpec` and the `TargetSpec` enum (`card_def.rs:357-`), specifically the eight `*SpellOnStack` variants (`AnySpellOnStack`, `InstantSpellOnStack`, `BlueSpellOnStack`, `RedSpellOnStack`, `ArtifactOrEnchantmentSpellOnStack`, `SorcerySpellOnStack`, `NoncreatureSpellOnStack`, `ArtifactSpellOnStack`) as the ground truth for "is a counterspell shape".
- Produces: `data/pauper_removal_counterspell_tags_v1.json`, schema `kernel_removal_counterspell_tags/v1`, one row per card whose `mechanics` list matches the requires-target or counterspell tag sets below: `{"schema": "kernel_removal_counterspell_tags/v1", "generated_from": "data/cards_v1.json", "cards": [{"card_id": int, "name": str, "requires_target": bool, "is_counterspell": bool, "source_mechanics": [str]}]}`. Only cards with `requires_target` or `is_counterspell` true are listed (a card absent from the file is `requires_target: false, is_counterspell: false` by omission, matching `GameSummaryV1`'s use in Task D: an absent card_id means neither field applies).

- [ ] **Step 1: Confirm the mechanics vocabulary this generator keys on.**
```
uvx uv@0.11.29 run --no-sync python -c "
import json, collections
cards = json.load(open('data/cards_v1.json', encoding='utf-8'))['cards']
mech = collections.Counter(m for c in cards for m in c.get('mechanics', []))
for k in sorted(mech): print(k, mech[k])
"
```
Confirm `destroy_target`, `exile_target`, `graveyard_target`, `target_creature`, `target_opponent`, `target_player`, `up_to_two_targets`, `counter_target`, `counter_spell` are present with nonzero counts (all nine were present at this plan's writing: `destroy_target 10`, `exile_target 8`, `graveyard_target 2`, `target_creature 1`, `target_opponent 2`, `target_player 1`, `up_to_two_targets 1`, `counter_target 10`, `counter_spell 2`). If any is absent, the REQUIRES_TARGET_MECHANICS / COUNTERSPELL_MECHANICS sets in Step 3 must be revised to match what is actually present before continuing.

- [ ] **Step 2: Write the failing Python test.** Create `python/tests/test_generate_removal_counterspell_tags_v1.py`:
```python
import json
import subprocess
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TOOL = ROOT / "python/tools/generate_removal_counterspell_tags_v1.py"
OUTPUT = ROOT / "data/pauper_removal_counterspell_tags_v1.json"


class GenerateRemovalCounterspellTags(unittest.TestCase):
    def test_write_produces_a_schema_valid_file_with_known_cards(self):
        result = subprocess.run(
            [sys.executable, str(TOOL), "--write"], cwd=ROOT, capture_output=True, text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        document = json.loads(OUTPUT.read_text(encoding="utf-8"))
        self.assertEqual(document["schema"], "kernel_removal_counterspell_tags/v1")
        by_name = {row["name"]: row for row in document["cards"]}
        self.assertIn("Annul", by_name)
        self.assertTrue(by_name["Annul"]["is_counterspell"])
        self.assertFalse(by_name["Annul"]["requires_target"] and not by_name["Annul"]["is_counterspell"])
        cards = json.load(open(ROOT / "data/cards_v1.json", encoding="utf-8"))["cards"]
        removal_names = {c["name"] for c in cards if "destroy_target" in c.get("mechanics", [])}
        self.assertTrue(removal_names, "the live registry must have at least one destroy_target card to test against")
        for name in removal_names:
            self.assertIn(name, by_name, f"{name} has destroy_target but is missing from the tag file")
            self.assertTrue(by_name[name]["requires_target"])

    def test_check_mode_is_idempotent_against_a_freshly_written_file(self):
        subprocess.run([sys.executable, str(TOOL), "--write"], cwd=ROOT, check=True)
        result = subprocess.run(
            [sys.executable, str(TOOL), "--check"], cwd=ROOT, capture_output=True, text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
```
Run: `uvx uv@0.11.29 run --no-sync python -m unittest python.tests.test_generate_removal_counterspell_tags_v1 -v`. Expected: FAIL (tool missing).

- [ ] **Step 3: Implement the generator.** Create `python/tools/generate_removal_counterspell_tags_v1.py`:
```python
#!/usr/bin/env python3
"""Generates data/pauper_removal_counterspell_tags_v1.json from the
mechanics tags in data/cards_v1.json (design section 3, W4). This is a
disclosed, reviewable mapping from mechanics tags to two boolean fields;
mtg-kernel/tests/removal_counterspell_tags_v1.rs cross-checks every row
against the live CARD_DEFS[card_id].target_spec so a hand-authored tag
cannot silently drift from engine ground truth (design section 9 risk).
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
REGISTRY_PATH = ROOT / "data/cards_v1.json"
OUTPUT_PATH = ROOT / "data/pauper_removal_counterspell_tags_v1.json"
SCHEMA = "kernel_removal_counterspell_tags/v1"

# A card whose mechanics list intersects this set targets something
# (creature, land, player, artifact, spell on the stack, ...) as part of
# its own effect. This is deliberately narrower than "removal" (mechanics
# tag "removal" also covers untargeted, batch, or triggered removal such as
# Suffocating Fumes' mass pump, which is not a target-requiring spell).
REQUIRES_TARGET_MECHANICS = frozenset({
    "destroy_target",
    "exile_target",
    "graveyard_target",
    "target_creature",
    "target_opponent",
    "target_player",
    "up_to_two_targets",
    "counter_target",
})

# A card whose mechanics list intersects this set counters a spell on the
# stack. Every counterspell also requires a target (the spell it counters),
# so COUNTERSPELL_MECHANICS is always a subset of the cards
# REQUIRES_TARGET_MECHANICS also flags.
COUNTERSPELL_MECHANICS = frozenset({
    "counter_target",
    "counter_spell",
})


def build_document() -> dict:
    registry = json.loads(REGISTRY_PATH.read_text(encoding="utf-8"))
    rows = []
    for card_id, card in enumerate(registry["cards"]):
        mechanics = set(card.get("mechanics", []))
        requires_target = bool(mechanics & REQUIRES_TARGET_MECHANICS)
        is_counterspell = bool(mechanics & COUNTERSPELL_MECHANICS)
        if not requires_target and not is_counterspell:
            continue
        source_mechanics = sorted(mechanics & (REQUIRES_TARGET_MECHANICS | COUNTERSPELL_MECHANICS))
        rows.append({
            "card_id": card_id,
            "name": card["name"],
            "requires_target": requires_target,
            "is_counterspell": is_counterspell,
            "source_mechanics": source_mechanics,
        })
    rows.sort(key=lambda row: row["card_id"])
    return {"schema": SCHEMA, "generated_from": "data/cards_v1.json", "cards": rows}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--write", action="store_true")
    group.add_argument("--check", action="store_true")
    args = parser.parse_args(argv)

    document = build_document()
    serialized = json.dumps(document, indent=2, sort_keys=False) + "\n"

    if args.write:
        OUTPUT_PATH.write_text(serialized, encoding="utf-8")
        print(f"wrote {OUTPUT_PATH} ({len(document['cards'])} cards)")
        return 0

    if not OUTPUT_PATH.exists():
        print(f"{OUTPUT_PATH} does not exist; run --write first", file=sys.stderr)
        return 1
    current = OUTPUT_PATH.read_text(encoding="utf-8")
    if current != serialized:
        print(f"{OUTPUT_PATH} is stale relative to data/cards_v1.json; rerun --write", file=sys.stderr)
        return 1
    print(f"{OUTPUT_PATH} is up to date ({len(document['cards'])} cards)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
```

- [ ] **Step 4: Run the Python test.** `uvx uv@0.11.29 run --no-sync python -m unittest python.tests.test_generate_removal_counterspell_tags_v1 -v`. Expected: PASS.

- [ ] **Step 5: Write the failing Rust validator.** Create `mtg-kernel/tests/removal_counterspell_tags_v1.rs`:
```rust
//! Live-engine cross-check for `data/pauper_removal_counterspell_tags_v1.json`
//! (design section 3, W4). The tag file is hand-authored data derived from
//! `cards_v1.json` mechanics tags; this test is the ratified defense
//! against it silently drifting from what the engine actually implements
//! (design section 9 risk: "an unreviewed or drifted tag silently corrupts
//! the field with no failing test").

use mtg_kernel::card_def::{CardCapability, TargetSpec, CARD_DEFS};

#[derive(serde::Deserialize)]
struct TagFileV1 {
    schema: String,
    cards: Vec<TagRowV1>,
}

#[derive(serde::Deserialize)]
struct TagRowV1 {
    card_id: usize,
    name: String,
    requires_target: bool,
    is_counterspell: bool,
}

fn load_tag_file() -> TagFileV1 {
    let raw = std::fs::read_to_string("../data/pauper_removal_counterspell_tags_v1.json")
        .or_else(|_| std::fs::read_to_string("data/pauper_removal_counterspell_tags_v1.json"))
        .expect("pauper_removal_counterspell_tags_v1.json is readable from the crate or repo root");
    serde_json::from_str(&raw).expect("tag file parses as TagFileV1")
}

fn is_counterspell_shaped(target_spec: TargetSpec) -> bool {
    matches!(
        target_spec,
        TargetSpec::AnySpellOnStack
            | TargetSpec::InstantSpellOnStack
            | TargetSpec::BlueSpellOnStack
            | TargetSpec::RedSpellOnStack
            | TargetSpec::ArtifactOrEnchantmentSpellOnStack
            | TargetSpec::SorcerySpellOnStack
            | TargetSpec::NoncreatureSpellOnStack
            | TargetSpec::ArtifactSpellOnStack
    )
}

#[test]
fn schema_is_the_expected_version() {
    let document = load_tag_file();
    assert_eq!(document.schema, "kernel_removal_counterspell_tags/v1");
}

#[test]
fn every_requires_target_row_matches_a_nontrivial_live_target_spec() {
    let document = load_tag_file();
    let mut checked = 0usize;
    for row in &document.cards {
        let Some(def) = CARD_DEFS.get(row.card_id) else {
            panic!("tag file row {:?} (card_id {}) has no CARD_DEFS entry", row.name, row.card_id);
        };
        assert_eq!(def.name, row.name, "card_id {} name drifted between the registry and the tag file", row.card_id);
        if def.capability != CardCapability::Full {
            // A not-yet-implemented card's target_spec is not yet meaningful
            // ground truth; the mechanics-tag claim is unverifiable until
            // the card is implemented, so it is skipped rather than failed.
            continue;
        }
        if row.requires_target {
            assert_ne!(
                def.target_spec, TargetSpec::None,
                "{} is tagged requires_target but CARD_DEFS reports TargetSpec::None",
                row.name
            );
            checked += 1;
        }
    }
    assert!(checked > 0, "at least one fully-implemented requires_target row must exist to exercise this check");
}

#[test]
fn every_is_counterspell_row_has_a_spell_on_stack_target_spec() {
    let document = load_tag_file();
    let mut checked = 0usize;
    for row in &document.cards {
        let def = &CARD_DEFS[row.card_id];
        if def.capability != CardCapability::Full {
            continue;
        }
        if row.is_counterspell {
            assert!(
                is_counterspell_shaped(def.target_spec),
                "{} is tagged is_counterspell but its live target_spec {:?} is not a *SpellOnStack variant",
                row.name, def.target_spec
            );
            checked += 1;
        }
    }
    assert!(checked > 0, "at least one fully-implemented is_counterspell row must exist to exercise this check");
}

#[test]
fn no_fully_implemented_full_targeting_spell_is_missing_from_the_tag_file() {
    // The converse direction: every implemented, non-trivially-targeting
    // spell must appear in the tag file with requires_target true, so the
    // generator's mechanics-tag set (python/tools/generate_removal_counterspell_tags_v1.py)
    // has not under-covered the registry.
    let document = load_tag_file();
    let tagged: std::collections::BTreeSet<usize> = document
        .cards
        .iter()
        .filter(|row| row.requires_target)
        .map(|row| row.card_id)
        .collect();
    let counterspell_specs = [
        TargetSpec::AnySpellOnStack,
        TargetSpec::InstantSpellOnStack,
        TargetSpec::BlueSpellOnStack,
        TargetSpec::RedSpellOnStack,
        TargetSpec::ArtifactOrEnchantmentSpellOnStack,
        TargetSpec::SorcerySpellOnStack,
        TargetSpec::NoncreatureSpellOnStack,
        TargetSpec::ArtifactSpellOnStack,
    ];
    for (card_id, def) in CARD_DEFS.iter().enumerate() {
        if def.capability != CardCapability::Full || def.is_token {
            continue;
        }
        if counterspell_specs.contains(&def.target_spec) && !tagged.contains(&card_id) {
            panic!(
                "{} (card_id {card_id}) has a counterspell-shaped target_spec {:?} but is absent from the tag file",
                def.name, def.target_spec
            );
        }
    }
}
```

- [ ] **Step 6 (Rust, precondition): confirm `TargetSpec`, `CardCapability`, and `CARD_DEFS` are `pub` from `mtg_kernel::card_def`.**
```
grep -n "^pub enum TargetSpec\|^pub enum CardCapability\|CARD_DEFS:" mtg-kernel/src/card_def.rs
```
If `CARD_DEFS` is generated by `include!` and not re-exported with `pub`, check `grep -n "pub static CARD_DEFS\|pub const CARD_DEFS" mtg-kernel/target/*/build/mtg-kernel-*/out/card_defs.rs` (or the equivalent path under `$CARGO_TARGET_DIR`) for its real visibility; it must already be `pub` since other integration tests (`tests/pauper_meta_w1_spells.rs` in the wave-1 plan) reference it directly.

- [ ] **Step 7: Run the failing Rust tests.**
```
cmd /c start /belownormal /affinity FF /wait cargo test --locked -p mtg-kernel --test removal_counterspell_tags_v1 2>&1 | tail -60
```
Expected: compiles (Step 6's precondition holds) and all four tests PASS immediately, because Step 3's generator and Step 4's run already produced a file whose `requires_target`/`is_counterspell` rows are constructed directly from the same mechanics tags the design ties to real `TargetSpec` shapes. If any test fails, the failure is real: either `REQUIRES_TARGET_MECHANICS`/`COUNTERSPELL_MECHANICS` in the Python generator over- or under-covers the registry, or a card's `mechanics` tag in `cards_v1.json` does not match what its Rust program actually does. Fix the mismatch at its source (the Python mechanics-set constants, not this test) and rerun Steps 3, 4, 7 until green; do not weaken the Rust assertions to make them pass.

- [ ] **Step 8: Commit and push.**
```
git add data/pauper_removal_counterspell_tags_v1.json python/tools/generate_removal_counterspell_tags_v1.py python/tests/test_generate_removal_counterspell_tags_v1.py mtg-kernel/tests/removal_counterspell_tags_v1.rs
git commit -m "data: requires-target/is-counterspell tag file (W4) with a live-engine validator"
git push
```

---

## Task D (W3): `GameSummaryV1` extractor

**Files:**
- Modify: `mtg-kernel/src/rl_session.rs`
  - Add `pub fn game_state(&self) -> &crate::state::GameState` to both `impl RlEpisodeSessionV1` (after `diagnostic_state_hash` at line 4138-4140) and `impl FastActorSessionV1` (after its own `diagnostic_state_hash` at line 5299-5301). This is the one new accessor Task D needs: `GameState.turn` (`state.rs:1157`, `pub turn: u32`) and `GameState.engine.event_history` (`engine.rs:64` `EngineState`, `event_history` field, both `pub`) are otherwise unreachable from outside `rl_session.rs`.
- Create: `mtg-kernel/src/game_summary_v1.rs` (new library module; add `pub mod game_summary_v1;` to `mtg-kernel/src/lib.rs`)

**Interfaces:**
- Consumes: `RlEpisodeSessionV1::game_state(&self) -> &GameState` (new, this task); `RlSessionResponseV1::{Decision, Terminal}`, `RlSessionDecisionV1.legal_actions: Vec<LegalActionV5>` where `LegalActionV5.semantic: ActionSemanticV1` (`rl.rs:1059-1065, 873-884`) carries `ActionSemanticV1::CastSpell { actor, source: CardStableRefV1 { card_db_id, zone, .. } }` (`rl.rs:881-884, 204-211`) directly, so the "was this hand card ever offered as a legal cast" hook needs no separate call into `legal_action_candidates_v5`; `state.engine.event_history: Vec<CommittedEvent>` (`event.rs:322-427`, every variant: `Damage`, `ZoneChange`, `LifeLoss`, `LifeGain`, `Draw`, `Tap`, `ManaAdded`, `CreateToken`, `SpellCast`, `Targeted`, `Sacrificed`, `CombatDamageToPlayer`, `SagaChapter`, `OptionalAdditionalCostPaid`, `InitiativeTrigger`, fifteen at this plan's writing), `state.turn: u32` (`state.rs:1157`), `GameObject.card_def: u16` (`state.rs:213`), `GameObject.owner`/`.controller`/`.zone`, `Arena<T>::iter(&self) -> impl Iterator<Item = (ObjectId, &T)>` (`ids.rs:96-101`, ascending-id order, and never shrinks: every `ObjectId` ever allocated this game is still present in `state.objects` at the terminal state, so the extractor builds its `ObjectId -> card_def` map by one pass over the terminal `state.objects` rather than incrementally per event).
- Produces: `pub struct GameSummaryV1 { schema_version: u32, checkpoint_weights_hash: String, checkpoint_git_head: String, winner: Option<PlayerId>, opponent_evidence: [Vec<OpponentEvidenceRowV1>; 2], own_card_outcomes: [BTreeMap<u16, OwnCardOutcomeV1>; 2], resource_curve: ResourceCurveV1 }` (all component structs `Serialize`; field order and shapes exactly match Step 4's struct definition, including the per-seat 2-arrays for `opponent_evidence`/`own_card_outcomes` and `winner: Option<PlayerId>`, not `Option<PlayerSeatV1>`: `RlSessionTerminalV1.winner` is `Option<PlayerSeatV1>` (`rl_session.rs:136`), and this task converts it explicitly, since `PlayerSeatV1` (`rl.rs:179`) is a real, distinct type from `PlayerId` with only a `From<PlayerId> for PlayerSeatV1` impl in the crate (`rl.rs:184`) and no impl in the reverse direction); `pub fn run_episode_with_summary_v1(session: &mut RlEpisodeSessionV1, checkpoint_weights_hash: &str, requires_target_tags: &RemovalCounterspellTagsV1, policy_fn: &mut dyn FnMut(&RlSessionDecisionV1) -> (u32, String)) -> GameSummaryV1` (drives the already-reset session to terminal, watermarking as it goes); `pub fn append_game_summary_jsonl_v1(path: &std::path::Path, summary: &GameSummaryV1) -> std::io::Result<()>`.

- [ ] **Step 1: Confirm the accessor anchors.**
```
grep -n "pub fn diagnostic_state_hash" mtg-kernel/src/rl_session.rs
```
Expected: two hits, one inside `impl RlEpisodeSessionV1` (line 4138) and one inside `impl FastActorSessionV1` (line 5299). If either method's surrounding `impl` block differs, adjust the insertion point to the real block boundary; do not insert the new accessor into the wrong `impl`.

- [ ] **Step 2: Add the two accessors** (real code, not a placeholder), immediately after each `diagnostic_state_hash` method body closes:
```rust
    /// Read-only access to the underlying game state, for the `GameSummaryV1`
    /// extractor (design section 3, W3), which needs `state.turn` and
    /// `state.engine.event_history` directly. Deliberately `&GameState`,
    /// never `&mut`: the extractor watches, it does not drive.
    pub fn game_state(&self) -> &crate::state::GameState {
        &self.state
    }
```
Add this once inside `impl RlEpisodeSessionV1` (after line 4140) and once, identically, inside `impl FastActorSessionV1` (after its own `diagnostic_state_hash`, at line 5301). Run `grep -n "pub fn game_state" mtg-kernel/src/rl_session.rs` afterward and confirm exactly two hits.

- [ ] **Step 3: Run a quick compile check** before writing the new module, to isolate any accessor mistake from the module's own errors:
```
cmd /c start /belownormal /affinity FF /wait cargo check --locked -p mtg-kernel 2>&1 | tail -30
```
Expected: PASS (this step only adds two public getters; nothing else changed).

- [ ] **Step 4: Write the failing test for the extractor**, inside a `#[cfg(test)] mod tests` block at the bottom of the new `mtg-kernel/src/game_summary_v1.rs` (create the file with just this test module first, following strict TDD):
```rust
//! `GameSummaryV1` extractor (design section 3, W3): folds
//! `state.engine.event_history` plus the legal-cast hook into one
//! self-describing, checkpoint-identified row per game per side. Runs
//! in-loop with the same episode driver W2 and W5 use.

use crate::event::CommittedEvent;
use crate::ids::PlayerId;
use crate::rl_session::{RlEpisodeSessionV1, RlSessionDecisionV1, RlSessionResponseV1};
use crate::rl::ActionSemanticV1;
use crate::state::Zone;
use serde::Serialize;
use std::collections::BTreeMap;

pub const GAME_SUMMARY_SCHEMA_V1: u32 = 1;

#[derive(Debug, Clone, Serialize)]
pub struct OpponentEvidenceRowV1 {
    pub card_id: u16,
    pub first_seen_turn: u32,
    pub end_of_game_zone: Zone,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct OwnCardOutcomeV1 {
    pub times_drawn: u8,
    pub cast: bool,
    pub stuck_in_hand: bool,
    pub died_without_dealing_damage: bool,
    pub removal_no_target: bool,
    pub counterspell_held: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ResourceCurveV1 {
    pub lands_by_turn: Vec<u32>,
    pub hand_size_by_turn: [Vec<u32>; 2],
    /// `i32`, matching `PlayerState.life: i32` exactly (`state.rs:353`) so
    /// the driver loop can push `state.players[seat].life` with no cast and
    /// no truncation risk; design section 3 writes this as `[Vec<i16>; 2]`,
    /// but the live field it is sourced from is `i32`, so this plan widens
    /// the type to match the real source rather than truncating on push.
    pub life_by_turn: [Vec<i32>; 2],
    pub first_attack_turn: Option<u32>,
    pub damage_dealt_total: [i64; 2],
    pub damage_taken_total: [i64; 2],
}

#[derive(Debug, Clone, Serialize)]
pub struct GameSummaryV1 {
    pub schema_version: u32,
    pub checkpoint_weights_hash: String,
    pub checkpoint_git_head: String,
    pub winner: Option<PlayerId>,
    pub opponent_evidence: [Vec<OpponentEvidenceRowV1>; 2],
    pub own_card_outcomes: [BTreeMap<u16, OwnCardOutcomeV1>; 2],
    pub resource_curve: ResourceCurveV1,
}

/// The requires-target/is-counterspell ground truth (Task C, W4). A small
/// slice type so `game_summary_v1.rs` does not need to know how the tag
/// file is loaded from disk; the caller (Task E's driver) loads it once
/// per campaign, not once per game.
pub struct RemovalCounterspellTagsV1 {
    pub requires_target: std::collections::BTreeSet<u16>,
    pub is_counterspell: std::collections::BTreeSet<u16>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_decks::runtime_deck_by_id;
    use crate::state::SplitMix64;

    fn burn_and_rally() -> [Vec<u16>; 2] {
        [
            runtime_deck_by_id("Burn").unwrap().card_ids.to_vec(),
            runtime_deck_by_id("Rally").unwrap().card_ids.to_vec(),
        ]
    }

    #[test]
    fn run_episode_with_summary_produces_a_well_formed_row_for_both_sides() {
        let mainboards = burn_and_rally();
        let deck_ids = ["Burn".to_owned(), "Rally".to_owned()];
        let mut session = RlEpisodeSessionV1::reset_with_explicit_decks_and_limits(
            1, 0x7777_7777_7777_7777, 2000, 200_000, deck_ids, mainboards,
        )
        .unwrap();
        let tags = RemovalCounterspellTagsV1 {
            requires_target: Default::default(),
            is_counterspell: Default::default(),
        };
        let mut rng = SplitMix64::seed(0x8888_8888_8888_8888);
        let mut policy = |decision: &RlSessionDecisionV1| {
            let index = (rng.next_u64() as usize) % decision.legal_actions.len();
            (index as u32, decision.legal_actions[index].stable_id.clone())
        };
        let summary = run_episode_with_summary_v1(&mut session, "deadbeefdeadbeef", &tags, &mut policy);
        assert_eq!(summary.schema_version, GAME_SUMMARY_SCHEMA_V1);
        assert_eq!(summary.checkpoint_weights_hash, "deadbeefdeadbeef");
        assert!(!summary.checkpoint_git_head.is_empty());
        assert!(summary.resource_curve.lands_by_turn.iter().sum::<u32>() > 0, "some land entered play over a full game");
        // own_card_outcomes must only ever key cards from that seat's own
        // registered mainboard, never the opponent's.
        let burn_ids: std::collections::BTreeSet<u16> = runtime_deck_by_id("Burn").unwrap().card_ids.iter().copied().collect();
        for &card_id in summary.own_card_outcomes[0].keys() {
            assert!(burn_ids.contains(&card_id), "P0 own_card_outcomes leaked a non-Burn card_id {card_id}");
        }
    }

    #[test]
    fn legal_cast_hook_marks_a_card_offered_as_a_cast_even_if_never_cast() {
        // A minimal, direct check of the hook logic in isolation: build a
        // synthetic legal_actions list with one CastSpell semantic for a
        // known card_id in Zone::Hand and confirm the folding function
        // records it, independent of a full episode run.
        let semantic = ActionSemanticV1::CastSpell {
            actor: PlayerId::P0.into(),
            source: crate::rl::CardStableRefV1 {
                arena_id: 1,
                card_db_id: 42,
                owner: PlayerId::P0.into(),
                controller: PlayerId::P0.into(),
                zone: Zone::Hand,
                zone_change_count: 0,
            },
        };
        let offered = was_card_offered_as_hand_cast_v1(&semantic, 42);
        assert!(offered);
        let not_offered = was_card_offered_as_hand_cast_v1(&semantic, 43);
        assert!(!not_offered);
    }
}
```

- [ ] **Step 5: Run and confirm the failure is "not found" for the not-yet-written functions**, not a type error in the test itself:
```
cmd /c start /belownormal /affinity FF /wait cargo test --locked -p mtg-kernel --lib game_summary_v1 2>&1 | tail -60
```
Expected: `error[E0433]` / `error[E0425]` for `run_episode_with_summary_v1` and `was_card_offered_as_hand_cast_v1` (not defined yet); no other errors. If there are other errors (a wrong field name, a missing `Serialize` bound), fix those first so the only remaining errors are the two missing functions, then proceed.

- [ ] **Step 6: Implement the folding functions**, added to `game_summary_v1.rs` above the `#[cfg(test)]` block. This is the full real body of every helper `run_episode_with_summary_v1` calls; none is left to prose:
```rust
fn was_card_offered_as_hand_cast_v1(semantic: &ActionSemanticV1, card_id: u16) -> bool {
    matches!(
        semantic,
        ActionSemanticV1::CastSpell { source, .. }
            if source.card_db_id == card_id && source.zone == Zone::Hand
    )
}

/// `ActionSemanticV1::CastSpell.actor` (`rl.rs:882`) and `CardStableRefV1`'s
/// own `owner`/`controller` (`rl.rs:207-208`) are `PlayerSeatV1`, the
/// RL-facing seat type; every `CommittedEvent` field that names a player
/// (`Draw.player`, `SpellCast.controller`, `ZoneChange.controller_before`)
/// is `PlayerId`, the engine-internal type (`ids.rs:35`, `event.rs`). The
/// two are related only by `impl From<PlayerId> for PlayerSeatV1`
/// (`rl.rs:184`) and have no reverse impl, so this task keeps two small,
/// separately named conversions rather than silently coercing one into the
/// other at a call site.
fn seat_index_v1(seat: crate::rl::PlayerSeatV1) -> usize {
    match seat {
        crate::rl::PlayerSeatV1::P0 => 0,
        crate::rl::PlayerSeatV1::P1 => 1,
    }
}

fn seat_index_from_player_id_v1(player: PlayerId) -> usize {
    player.0 as usize
}

fn count_lands_v1(state: &crate::state::GameState) -> u32 {
    state
        .objects
        .iter()
        .filter(|(_, object)| {
            object.zone == Zone::Battlefield
                && crate::card_def::CARD_DEFS[object.card_def as usize].is_land
        })
        .count() as u32
}

/// `Arena::push` (`ids.rs:72-76`) always assigns a new, strictly increasing
/// id and nothing ever removes an entry, so every `ObjectId` allocated this
/// game -- drawn, cast, milled, or created as a token mid-game -- is still
/// present in the terminal state's `objects` arena, only possibly in a
/// different zone, and `card_def` never changes for a given `ObjectId`
/// after creation. One pass over the terminal arena is therefore exactly
/// equivalent to incrementally rebuilding the map on every
/// `CreateToken`/`ZoneChange` event, and simpler.
fn build_object_card_def_map_v1(
    state: &crate::state::GameState,
) -> BTreeMap<crate::ids::ObjectId, u16> {
    state.objects.iter().map(|(id, object)| (id, object.card_def)).collect()
}

/// Every non-token object this game whose `owner` is `seat`: exactly the
/// distinct card ids of that seat's own registered 60 (a copy count > 1
/// collapses to one entry, which is what `own_card_outcomes`, keyed by
/// `card_id`, needs). Tokens are excluded because they are not registered
/// deck cards.
fn own_registered_card_ids_v1(
    state: &crate::state::GameState,
    seat: usize,
) -> std::collections::BTreeSet<u16> {
    let seat_player = PlayerId(seat as u8);
    state
        .objects
        .iter()
        .filter(|(_, object)| {
            object.owner == seat_player && !crate::card_def::CARD_DEFS[object.card_def as usize].is_token
        })
        .map(|(_, object)| object.card_def)
        .collect()
}

/// No `CommittedEvent` variant carries a turn stamp (design section 3), so
/// `run_episode_with_summary_v1` builds `turn_watermarks`, one
/// `(event_history.len(), turn)` pair recorded the first time each turn
/// reaches a decision. `event_index` is attributed to the smallest recorded
/// turn whose watermark index exceeds it (the earliest turn boundary
/// reached after the event committed); an event at or after the last
/// watermark is attributed to `final_turn`, the turn the game ended on.
fn turn_for_event_index_v1(turn_watermarks: &[(usize, u32)], final_turn: u32, event_index: usize) -> u32 {
    turn_watermarks
        .iter()
        .find(|&&(watermark_index, _)| watermark_index > event_index)
        .map(|&(_, turn)| turn)
        .unwrap_or(final_turn)
}

/// This task's operational proxy for "this specific cast resolved without
/// ever choosing a target": no `CommittedEvent` variant links a `Targeted`
/// event back to the spell that caused it (`Targeted::targeting_stack_item`
/// is a `StackItemId`, not this spell's `ObjectId`), so the proxy is
/// windowed on the casting object's own time on the stack (from its
/// `SpellCast` event at `cast_index` to its next `ZoneChange` away from
/// `Zone::Stack`) rather than a direct causal link. True iff no `Targeted`
/// event appears anywhere in that window.
fn spell_resolved_with_no_traced_target_v1(
    event_history: &[CommittedEvent],
    cast_index: usize,
    spell: crate::ids::ObjectId,
) -> bool {
    let leave_index = event_history
        .iter()
        .enumerate()
        .skip(cast_index + 1)
        .find(|(_, event)| {
            matches!(
                event,
                CommittedEvent::ZoneChange { object, from, .. }
                    if *object == spell && *from == Zone::Stack
            )
        })
        .map(|(index, _)| index)
        .unwrap_or(event_history.len());
    !event_history[cast_index..leave_index]
        .iter()
        .any(|event| matches!(event, CommittedEvent::Targeted { .. }))
}

/// Every card_def in `seat`'s hand at the state this is called against
/// (the terminal state, in `run_episode_with_summary_v1`'s real call site;
/// a hand-built set in the exact-value unit tests below).
fn end_of_game_hand_card_ids_v1(
    state: &crate::state::GameState,
    object_card_def: &BTreeMap<crate::ids::ObjectId, u16>,
    seat: usize,
) -> std::collections::BTreeSet<u16> {
    state.players[seat]
        .hand
        .iter()
        .filter_map(|object_id| object_card_def.get(object_id).copied())
        .collect()
}

/// Folds the full `event_history` plus the live legal-cast hook
/// (`offered_as_cast`, built by `run_episode_with_summary_v1`'s driver loop)
/// into `opponent_evidence` and `own_card_outcomes` (design section 3).
/// Runs once, at episode end. Deliberately takes plain data
/// (`event_history`, the `ObjectId -> card_def` map, each seat's own
/// registered ids, each seat's end-of-game hand) rather than `&GameState`
/// directly: `run_episode_with_summary_v1`'s real call site builds these
/// from the terminal state (via `build_object_card_def_map_v1`,
/// `own_registered_card_ids_v1`, `end_of_game_hand_card_ids_v1` above), and
/// this module's tests build them by hand from a synthetic event list, with
/// no need to construct a full `GameState`.
fn fold_event_history_v1(
    event_history: &[CommittedEvent],
    final_turn: u32,
    turn_watermarks: &[(usize, u32)],
    object_card_def: &BTreeMap<crate::ids::ObjectId, u16>,
    own_registered_ids: &[std::collections::BTreeSet<u16>; 2],
    end_of_game_hand_card_ids: &[std::collections::BTreeSet<u16>; 2],
    offered_as_cast: &[std::collections::BTreeSet<u16>; 2],
    tags: &RemovalCounterspellTagsV1,
) -> ([Vec<OpponentEvidenceRowV1>; 2], [BTreeMap<u16, OwnCardOutcomeV1>; 2]) {
    let mut opponent_first_seen: [BTreeMap<u16, u32>; 2] = Default::default();
    let mut opponent_end_of_game_zone: [BTreeMap<u16, Zone>; 2] = Default::default();
    let mut times_drawn: [BTreeMap<u16, u8>; 2] = Default::default();
    let mut cast_ids: [std::collections::BTreeSet<u16>; 2] = Default::default();
    let mut cast_no_target: [std::collections::BTreeSet<u16>; 2] = Default::default();
    let mut ever_dealt_damage: std::collections::BTreeSet<crate::ids::ObjectId> = Default::default();
    let mut died_without_damage: [std::collections::BTreeSet<u16>; 2] = Default::default();

    for (index, event) in event_history.iter().enumerate() {
        match event {
            CommittedEvent::Draw { player, object: Some(object_id) } => {
                let seat = seat_index_from_player_id_v1(*player);
                if let Some(&card_id) = object_card_def.get(object_id) {
                    if own_registered_ids[seat].contains(&card_id) {
                        *times_drawn[seat].entry(card_id).or_insert(0) += 1;
                    }
                }
            }
            CommittedEvent::SpellCast { spell, controller } => {
                let seat = seat_index_from_player_id_v1(*controller);
                if let Some(&card_id) = object_card_def.get(spell) {
                    if own_registered_ids[seat].contains(&card_id) {
                        cast_ids[seat].insert(card_id);
                        if spell_resolved_with_no_traced_target_v1(event_history, index, *spell) {
                            cast_no_target[seat].insert(card_id);
                        }
                    }
                }
            }
            CommittedEvent::Damage { source, .. } => {
                ever_dealt_damage.insert(*source);
            }
            CommittedEvent::CombatDamageToPlayer { source, .. } => {
                ever_dealt_damage.insert(*source);
            }
            CommittedEvent::ZoneChange { object, from, to, controller_before } => {
                let Some(&card_id) = object_card_def.get(object) else { continue };
                let is_public = matches!(to, Zone::Battlefield | Zone::Graveyard | Zone::Stack | Zone::Exile);
                let is_died = matches!(to, Zone::Graveyard | Zone::Exile)
                    && !matches!(from, Zone::Graveyard | Zone::Exile);

                // Opponent evidence, from each seat's own point of view:
                // `object`'s owner (not `controller_before`, so a stolen
                // permanent still reveals its true owner's card) belonging
                // to the *other* seat, made public.
                for seat in 0..2 {
                    let opponent_seat = 1 - seat;
                    if own_registered_ids[opponent_seat].contains(&card_id) && is_public {
                        opponent_first_seen[seat]
                            .entry(card_id)
                            .or_insert_with(|| turn_for_event_index_v1(turn_watermarks, final_turn, index));
                        opponent_end_of_game_zone[seat].insert(card_id, *to);
                    }
                }

                if is_died {
                    let owner_seat = seat_index_from_player_id_v1(*controller_before);
                    if own_registered_ids[owner_seat].contains(&card_id) && !ever_dealt_damage.contains(object) {
                        died_without_damage[owner_seat].insert(card_id);
                    }
                }
            }
            _ => {}
        }
    }

    let mut opponent_evidence: [Vec<OpponentEvidenceRowV1>; 2] = Default::default();
    for seat in 0..2 {
        for (&card_id, &first_seen_turn) in &opponent_first_seen[seat] {
            opponent_evidence[seat].push(OpponentEvidenceRowV1 {
                card_id,
                first_seen_turn,
                end_of_game_zone: opponent_end_of_game_zone[seat][&card_id],
            });
        }
    }

    let mut own_card_outcomes: [BTreeMap<u16, OwnCardOutcomeV1>; 2] = Default::default();
    for seat in 0..2 {
        for &card_id in &own_registered_ids[seat] {
            let stuck_in_hand = end_of_game_hand_card_ids[seat].contains(&card_id);
            let cast = cast_ids[seat].contains(&card_id);
            let removal_no_target = tags.requires_target.contains(&card_id)
                && cast
                && cast_no_target[seat].contains(&card_id);
            let counterspell_held = tags.is_counterspell.contains(&card_id)
                && !offered_as_cast[seat].contains(&card_id)
                && stuck_in_hand;
            own_card_outcomes[seat].insert(
                card_id,
                OwnCardOutcomeV1 {
                    times_drawn: *times_drawn[seat].get(&card_id).unwrap_or(&0),
                    cast,
                    stuck_in_hand,
                    died_without_dealing_damage: died_without_damage[seat].contains(&card_id),
                    removal_no_target,
                    counterspell_held,
                },
            );
        }
    }

    (opponent_evidence, own_card_outcomes)
}

pub fn run_episode_with_summary_v1(
    session: &mut RlEpisodeSessionV1,
    checkpoint_weights_hash: &str,
    tags: &RemovalCounterspellTagsV1,
    policy_fn: &mut dyn FnMut(&RlSessionDecisionV1) -> (u32, String),
) -> GameSummaryV1 {
    let mut offered_as_cast: [std::collections::BTreeSet<u16>; 2] = Default::default();
    let mut resource_curve = ResourceCurveV1::default();
    let mut turn_watermarks: Vec<(usize, u32)> = Vec::new();
    let mut last_turn = session.game_state().turn;

    loop {
        match session.current_response() {
            RlSessionResponseV1::Terminal(terminal) => {
                let final_state = session.game_state();
                let object_card_def = build_object_card_def_map_v1(final_state);
                let own_registered_ids = [
                    own_registered_card_ids_v1(final_state, 0),
                    own_registered_card_ids_v1(final_state, 1),
                ];
                let end_of_game_hand_card_ids = [
                    end_of_game_hand_card_ids_v1(final_state, &object_card_def, 0),
                    end_of_game_hand_card_ids_v1(final_state, &object_card_def, 1),
                ];
                let (opponent_evidence, own_card_outcomes) = fold_event_history_v1(
                    &final_state.engine.event_history,
                    final_state.turn,
                    &turn_watermarks,
                    &object_card_def,
                    &own_registered_ids,
                    &end_of_game_hand_card_ids,
                    &offered_as_cast,
                    tags,
                );
                let winner = match terminal.winner {
                    None => None,
                    Some(crate::rl::PlayerSeatV1::P0) => Some(PlayerId::P0),
                    Some(crate::rl::PlayerSeatV1::P1) => Some(PlayerId::P1),
                };
                return GameSummaryV1 {
                    schema_version: GAME_SUMMARY_SCHEMA_V1,
                    checkpoint_weights_hash: checkpoint_weights_hash.to_owned(),
                    checkpoint_git_head: env!("MTG_KERNEL_BUILD_GIT_HEAD").to_owned(),
                    winner,
                    opponent_evidence,
                    own_card_outcomes,
                    resource_curve,
                };
            }
            RlSessionResponseV1::Decision(decision) => {
                let turn = session.game_state().turn;
                if turn != last_turn || turn_watermarks.is_empty() {
                    let state = session.game_state();
                    turn_watermarks.push((state.engine.event_history.len(), turn));
                    resource_curve.lands_by_turn.push(count_lands_v1(state));
                    for seat in 0..2 {
                        resource_curve.hand_size_by_turn[seat]
                            .push(state.players[seat].hand.len() as u32);
                        // `PlayerState.life: i32` (`state.rs:353`) matches
                        // `ResourceCurveV1.life_by_turn: [Vec<i32>; 2]`
                        // exactly; no cast, no truncation.
                        resource_curve.life_by_turn[seat].push(state.players[seat].life);
                    }
                    last_turn = turn;
                }
                for legal_action in &decision.legal_actions {
                    if let ActionSemanticV1::CastSpell { actor, source } = &legal_action.semantic {
                        if source.zone == Zone::Hand {
                            let seat_index = seat_index_v1(*actor);
                            offered_as_cast[seat_index].insert(source.card_db_id);
                        }
                    }
                }
                let (selected_index, selected_action_id) = policy_fn(&decision);
                session
                    .step(decision.episode_id, decision.step, selected_index, &selected_action_id)
                    .expect("policy-selected action is legal by construction");
            }
        }
    }
}
```

- [ ] **Step 7: Run the tests.**
```
cmd /c start /belownormal /affinity FF /wait cargo test --locked -p mtg-kernel --lib game_summary_v1 2>&1 | tail -60
```
Expected: both tests PASS. Debug any assertion failure against the real folded values (print `summary.resource_curve` with `--nocapture` if `lands_by_turn`'s sum is zero, which would mean the turn-boundary watermark never fired).

- [ ] **Step 8: Add exact-value fold tests against a synthetic event history**, appended to `mod tests`. Step 4/7's tests only check schema fields and that `own_card_outcomes` stays scoped to the right seat; this step exercises `fold_event_history_v1`'s actual per-field logic directly, with no game engine involved, so the exact values are hand-computable and asserted, not merely "present":
```rust
    #[test]
    fn fold_event_history_records_opponent_evidence_both_directions_and_a_no_target_removal_cast() {
        use crate::ids::ObjectId;

        // A four-event synthetic history: P0 draws its own removal spell
        // (card_id 200, requires_target), P1's creature (card_id 100)
        // enters the battlefield turn 2 (revealing it to P0), P0 casts the
        // removal spell, and it resolves straight to the graveyard with no
        // `Targeted` event ever logged in its casting window (no legal
        // target was ever chosen).
        let event_history = vec![
            CommittedEvent::Draw { player: PlayerId::P0, object: Some(ObjectId(20)) }, // index 0
            CommittedEvent::ZoneChange {
                object: ObjectId(10),
                from: Zone::Library,
                to: Zone::Battlefield,
                controller_before: PlayerId::P1,
            }, // index 1: P1's card_id 100 becomes public
            CommittedEvent::SpellCast { spell: ObjectId(20), controller: PlayerId::P0 }, // index 2
            CommittedEvent::ZoneChange {
                object: ObjectId(20),
                from: Zone::Stack,
                to: Zone::Graveyard,
                controller_before: PlayerId::P0,
            }, // index 3: resolves with no Targeted event anywhere in [2, 3)
        ];
        // Turn 1 starts at history length 0; turn 2 starts at history
        // length 2 (after the Draw and the opponent's ZoneChange, before
        // the SpellCast); the game ends on turn 3.
        let turn_watermarks = vec![(0usize, 1u32), (2usize, 2u32)];
        let final_turn = 3u32;
        let object_card_def: BTreeMap<ObjectId, u16> =
            [(ObjectId(10), 100u16), (ObjectId(20), 200u16)].into_iter().collect();
        let own_registered_ids: [std::collections::BTreeSet<u16>; 2] = [
            [200u16].into_iter().collect(),
            [100u16].into_iter().collect(),
        ];
        let end_of_game_hand_card_ids: [std::collections::BTreeSet<u16>; 2] = Default::default();
        let offered_as_cast: [std::collections::BTreeSet<u16>; 2] =
            [[200u16].into_iter().collect(), Default::default()];
        let tags = RemovalCounterspellTagsV1 {
            requires_target: [200u16].into_iter().collect(),
            is_counterspell: Default::default(),
        };

        let (opponent_evidence, own_card_outcomes) = fold_event_history_v1(
            &event_history,
            final_turn,
            &turn_watermarks,
            &object_card_def,
            &own_registered_ids,
            &end_of_game_hand_card_ids,
            &offered_as_cast,
            &tags,
        );

        // P0's evidence about the opponent (P1's card_id 100): first seen
        // turn 2, the earliest turn boundary reached after index 1.
        assert_eq!(opponent_evidence[0].len(), 1);
        assert_eq!(opponent_evidence[0][0].card_id, 100);
        assert_eq!(opponent_evidence[0][0].first_seen_turn, 2);
        assert_eq!(opponent_evidence[0][0].end_of_game_zone, Zone::Battlefield);

        // P1's evidence about the opponent (P0's card_id 200): first seen
        // turn 3 (no watermark index exceeds 3, so it falls back to
        // final_turn), ending in the graveyard.
        assert_eq!(opponent_evidence[1].len(), 1);
        assert_eq!(opponent_evidence[1][0].card_id, 200);
        assert_eq!(opponent_evidence[1][0].first_seen_turn, 3);
        assert_eq!(opponent_evidence[1][0].end_of_game_zone, Zone::Graveyard);

        // P0's own_card_outcomes for its removal spell: drawn once, cast,
        // not stuck in hand, and removal_no_target true (it was cast, it
        // is tagged requires_target, and no Targeted event ever fired in
        // its casting window).
        let outcome = &own_card_outcomes[0][&200];
        assert_eq!(outcome.times_drawn, 1);
        assert!(outcome.cast);
        assert!(!outcome.stuck_in_hand);
        assert!(outcome.removal_no_target, "cast with no traced target must be recorded");
        assert!(!outcome.counterspell_held, "not tagged is_counterspell, so this field is always false here");
    }
```
Run:
```
cmd /c start /belownormal /affinity FF /wait cargo test --locked -p mtg-kernel --lib game_summary_v1::tests::fold_event_history_records 2>&1 | tail -40
```
Expected: PASS. If any assertion fails, the failure is real: recompute the expected value by hand against `fold_event_history_v1`'s actual logic and fix the implementation (Step 6), never the test's expected values, unless this step's own hand-computation above is shown to be wrong.

- [ ] **Step 9: Write the JSONL writer and its test**, appended to `game_summary_v1.rs`:
```rust
pub fn append_game_summary_jsonl_v1(
    path: &std::path::Path,
    summary: &GameSummaryV1,
) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    let line = serde_json::to_string(summary).expect("GameSummaryV1 always serializes");
    writeln!(file, "{line}")
}
```
Test (inside `mod tests`):
```rust
    #[test]
    fn append_game_summary_jsonl_writes_one_line_per_call_and_round_trips() {
        let dir = std::env::temp_dir().join(format!("game_summary_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("summaries.jsonl");
        let _ = std::fs::remove_file(&path);

        let mainboards = burn_and_rally();
        let deck_ids = ["Burn".to_owned(), "Rally".to_owned()];
        let mut session = RlEpisodeSessionV1::reset_with_explicit_decks_and_limits(
            2, 0x2222_2222_2222_2222, 2000, 200_000, deck_ids, mainboards,
        )
        .unwrap();
        let tags = RemovalCounterspellTagsV1 { requires_target: Default::default(), is_counterspell: Default::default() };
        let mut rng = SplitMix64::seed(0x3333_3333_3333_3333);
        let mut policy = |decision: &RlSessionDecisionV1| {
            let index = (rng.next_u64() as usize) % decision.legal_actions.len();
            (index as u32, decision.legal_actions[index].stable_id.clone())
        };
        let summary = run_episode_with_summary_v1(&mut session, "cafef00dcafef00d", &tags, &mut policy);
        append_game_summary_jsonl_v1(&path, &summary).unwrap();
        append_game_summary_jsonl_v1(&path, &summary).unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 2);
        let round_tripped: GameSummaryRoundTripCheckV1 = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(round_tripped.schema_version, GAME_SUMMARY_SCHEMA_V1);
        assert_eq!(round_tripped.checkpoint_weights_hash, "cafef00dcafef00d");
    }
```
Add a minimal `#[derive(serde::Deserialize)] struct GameSummaryRoundTripCheckV1 { schema_version: u32, checkpoint_weights_hash: String }` above `mod tests` (a read-back type distinct from the write-only `GameSummaryV1`, so the round-trip test proves the JSONL is externally readable, not only that `Serialize` compiles).

- [ ] **Step 10: Run the full module test suite.**
```
cmd /c start /belownormal /affinity FF /wait cargo test --locked -p mtg-kernel --lib game_summary_v1 2>&1 | tail -60
```
Expected: all tests PASS.

- [ ] **Step 11: Register the module and run the full crate suite.**
```
grep -n "^pub mod " mtg-kernel/src/lib.rs | tail -5
sed -i '/^pub mod game_summary_v1;/!{/^pub mod paired_bo1_harness_v1;/a pub mod game_summary_v1;
}' mtg-kernel/src/lib.rs
cmd /c start /belownormal /affinity FF /wait cargo test --locked -p mtg-kernel 2>&1 | tail -30
```
Expected: full suite PASS.

- [ ] **Step 12: Commit and push.**
```
git add mtg-kernel/src/rl_session.rs mtg-kernel/src/game_summary_v1.rs mtg-kernel/src/lib.rs
git commit -m "harness: GameSummaryV1 extractor (W3), in-loop event-history fold with the legal-cast hook"
git push
```

---

## Task E (W5): candidate generator, search driver, search-campaign manifest

**Files:**
- Create: `mtg-kernel/src/sideboard_search_campaign_v1.rs` (new library module; add `pub mod sideboard_search_campaign_v1;` to `mtg-kernel/src/lib.rs`)
- Create: `sideboard_search_campaign_manifest_v1.json` (a real, hashed example manifest committed for the tests to read; W6's actual campaign run will produce its own instance from the same schema)
- Create: `sideboard_search_campaign_manifest_v1.json.sha256` (written by Step 18, the sha256 sidecar Step 15's committed manifest is hash-locked against)
- Create: `docs/research/sideboard_plan_inputs_2026-09/plan_table_draft_nine_v1.json` (the warm-start fixture Step 9 creates; W6 substitutes a fuller researched table under the same path and re-hashes per Step 19)
- Create: `python/tools/validate_search_campaign_manifest_v1.py`
- Create: `python/tests/test_validate_search_campaign_manifest_v1.py`
- Modify (conditionally): `mtg-kernel/src/sideboard.rs` (`cards_in()`/`cards_out()` accessors on `SideboardPlanV1`; Step 5 greps for them first and only adds them if they are not already present, per that step's existing conditional instructions)

**Interfaces:**
- Consumes: Task A's `resolve_explicit_decks`-backed constructors (via Task B's `run_paired_bo1_trial_v1`); `mtg_kernel::sideboard::{checked_in_pauper_registered_deck_by_id_v1, RegisteredDeckV1, SideboardPlanV1, CardCountV1, DeckConfigurationV1, DeterministicSideboardPolicyV1}` (`sideboard.rs:94-183, 229, 297-, 406-`); `mtg_kernel::bo3_session::{BestOfThreeDeckMatchV1, PreparedMatchGameV1}` and `mtg_kernel::bo3_match::{PlayDrawChoiceV1, GameOutcomeV1}` (`bo3_session.rs:44-98, 21-41`); `mtg_kernel::runtime_decks::RUNTIME_DECKS` (for `embedding_status`); Task B's `paired_bootstrap_ci_v1`, `BootstrapSidednessV1`, `PairedBootstrapResultV1`; `env!("MTG_KERNEL_BUILD_GIT_HEAD")`.
- Produces: `pub struct SearchCampaignManifestV1 { .. }` (every field design section 4 lists as fixed: `checkpoint_weights_hash`, `checkpoint_git_head`, `k_max_candidates_per_cell: u32`, `max_iterations_per_cell: u32`, `traversal_order: String`, `master_seed: u64`, `n_per_cell: u32`, `n_derivation: NDerivationV1`, `m_bo3_matches_per_ratification: u32`, `warm_start_plan_inputs_sha256: String`, `bo1_one_sided_alpha: f64`, `bo3_confidence_level: f64`, `bootstrap_resample_count: u32`, `bootstrap_seed: u64`, `bootstrap_resampling_method: String`, `bo1_bootstrap_sidedness: BootstrapSidednessV1`, `bo3_bootstrap_sidedness: BootstrapSidednessV1` }`, the last three per design section 4's "resampling method... and sidedness (one-sided for BO1... two-sided for BO3)" requirement) plus `pub fn manifest_sha256_v1(manifest: &SearchCampaignManifestV1) -> String` (used only to construct a manifest programmatically, e.g. Step 2's synthetic fixtures) and `pub fn load_and_verify_manifest_v1(path: &std::path::Path) -> Result<SearchCampaignManifestV1, SearchCampaignErrorV1>` (hashes the raw file bytes it already read, the same convention the Python sidecar tool uses, and refuses to load on any mismatch against a sibling `.sha256` file); `pub fn candidate_seed_v1(master_seed: u64, self_deck_id: &str, opponent_deck_id: &str, game_index: u8, candidate_index: u32) -> u64` (the pure seed function design section 4 requires); `pub fn embedding_status_v1(card_ids: &[u16]) -> Vec<(u16, bool)>` (`true` iff the card_id appears in any `RUNTIME_DECKS[i].card_ids`); `pub fn opponent_embedding_status_v1(opponent_plan_cards_in: &[u16]) -> Vec<(u16, bool)>` (same trained-check, design section 4's disclosure requirement for the accepted-opponent-plan variant); `pub struct CandidateReceiptFieldsV1 { hash_convention: String, embedding_status: Vec<(u16, bool)>, opponent_embedding_status: Option<Vec<(u16, bool)>>, opponent_resolution_variant: OpponentResolutionVariantV1 }`; `pub enum OpponentResolutionVariantV1 { RegisteredMainboard, Bo3RatifiedPlan }` and `pub fn resolve_opponent_mainboard_for_cell_v1(opponent_deck_id: &str, self_deck_id: &str, game_index: u8, sideboard_policy: &DeterministicSideboardPolicyV1) -> Result<(Vec<u16>, OpponentResolutionVariantV1), SideboardErrorV1>` (design section 2's opponent-side resolution rule: `game_index` 1 is always the registered mainboard, `game_index` 2/3 uses the opponent's ratified post-board plan when one exists for `(opponent_deck_id, self_deck_id)`, else falls back to the registered mainboard); `pub fn derive_n_per_cell_v1(minimum_win_rate_delta: f64, target_power: f64, one_sided_alpha: f64) -> u32` (the executable counterpart of `NDerivationV1.formula`, a normal-approximation sample-size calculation); `pub fn generate_one_swap_candidates_v1` and `pub fn generate_multi_swap_candidates_v1` (bounded 1-to-4-card swaps restricted to the registered 75, deterministic traversal order, no scoring) plus `pub fn hill_climb_search_v1` (the bounded, warm-started local search over them) and the two-stage acceptance driver `pub fn run_bo1_provisional_search_for_cell_v1` / `pub fn run_bo3_ratification_v1`, all specified in full where each is implemented below.

- [ ] **Step 1: Confirm the manifest's structural building blocks compile against real types**, before writing any manifest code, by writing a throwaway scratch check:
```
cmd /c start /belownormal /affinity FF /wait cargo doc --locked -p mtg-kernel --no-deps 2>&1 | tail -5
grep -n "pub fn checked_in_pauper_registered_deck_by_id_v1\|pub struct SideboardPlanV1\|pub struct CardCountV1" mtg-kernel/src/sideboard.rs
grep -n "pub fn checked_in_pauper_v1\|pub struct BestOfThreeDeckMatchV1\|pub fn prepare_game_v1\|pub fn configuration\b\|pub fn record_game_result_v1\b\|pub struct PreparedMatchGameV1" mtg-kernel/src/bo3_session.rs
```
Confirms the exact signatures this task's code will call, matching the earlier grounding: `checked_in_pauper_registered_deck_by_id_v1(deck_id: &str) -> Result<RegisteredDeckV1, SideboardErrorV1>` (`sideboard.rs:229`), `SideboardPlanV1::new_v1(self_deck_id, opponent_deck_id, game_index, cards_in, cards_out) -> Result<Self, SideboardErrorV1>` (`sideboard.rs:305`), `BestOfThreeDeckMatchV1::checked_in_pauper_v1(p0_deck_id, p1_deck_id, game_one_chooser) -> Result<Self, Bo3SessionErrorV1>` (`bo3_session.rs:52`), `prepare_game_v1(&mut self, chooser: PlayerId, choice: PlayDrawChoiceV1) -> Result<PreparedMatchGameV1, Bo3SessionErrorV1>` (`bo3_session.rs:100`), `PreparedMatchGameV1::configuration(&self, player: PlayerId) -> Option<&DeckConfigurationV1>` (`bo3_session.rs:33`), `BestOfThreeDeckMatchV1::record_game_result_v1(&mut self, outcome: GameOutcomeV1) -> Result<_, Bo3SessionErrorV1>` (`bo3_session.rs:143`).

- [ ] **Step 2: Write the failing tests for the manifest and its hash guard.** Create `mtg-kernel/src/sideboard_search_campaign_v1.rs` with the struct and test module first:
```rust
//! Search-campaign manifest, candidate generator, and BO1/BO3 driver
//! (design section 4, W5). This module builds the manifest and the
//! candidate/scoring machinery; it does not run a campaign (W6 is out of
//! scope for this plan) and it never mutates
//! `data/pauper_sideboard_policy_v1.json`.

use crate::ids::PlayerId;
use crate::paired_bo1_harness_v1::BootstrapSidednessV1;
use crate::sideboard::{CardCountV1, DeckConfigurationV1, RegisteredDeckV1, SideboardErrorV1, SideboardPlanV1};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SEARCH_CAMPAIGN_MANIFEST_SCHEMA_V1: &str = "kernel_sideboard_search_campaign_manifest/v1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NDerivationV1 {
    pub minimum_win_rate_delta: f64,
    pub target_power: f64,
    pub calibration_mean_seconds_per_game: f64,
    pub formula: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchCampaignManifestV1 {
    pub schema: String,
    pub checkpoint_weights_hash: String,
    pub checkpoint_git_head: String,
    pub k_max_candidates_per_cell: u32,
    pub max_iterations_per_cell: u32,
    pub traversal_order: String,
    pub master_seed: u64,
    pub n_per_cell: u32,
    pub n_derivation: NDerivationV1,
    pub m_bo3_matches_per_ratification: u32,
    pub warm_start_plan_inputs_sha256: String,
    pub bo1_one_sided_alpha: f64,
    pub bo3_confidence_level: f64,
    pub bootstrap_resample_count: u32,
    pub bootstrap_seed: u64,
    /// Design section 4's manifest field: "resampling method (case
    /// resampling with replacement over the paired per-seed deltas)".
    /// A disclosed string, not a code path selector; `paired_bootstrap_ci_v1`
    /// (Task B) only ever implements case resampling with replacement, so
    /// this field's value is always `"case_resampling_with_replacement"`
    /// today, but it is a manifest-recorded fact, not an assumption.
    pub bootstrap_resampling_method: String,
    /// Design section 4: "sidedness (one-sided for BO1, matching its
    /// alpha; two-sided for BO3's CI-excludes-0 test)". Two separate
    /// fields because the two stages use different sidedness values within
    /// the same manifest; the driver (Step 14 below) always uses
    /// `bo1_bootstrap_sidedness` for the BO1 stage and
    /// `bo3_bootstrap_sidedness` for the BO3 stage, never an ad hoc choice
    /// at analysis time.
    pub bo1_bootstrap_sidedness: BootstrapSidednessV1,
    pub bo3_bootstrap_sidedness: BootstrapSidednessV1,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SearchCampaignErrorV1 {
    Io(String),
    Json(String),
    HashMismatch { expected: String, actual: String },
    MissingHashFile(String),
}

impl std::fmt::Display for SearchCampaignErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(message) => write!(f, "io error: {message}"),
            Self::Json(message) => write!(f, "json error: {message}"),
            Self::HashMismatch { expected, actual } => write!(
                f, "manifest sha256 mismatch: recorded {expected}, recomputed {actual}"
            ),
            Self::MissingHashFile(path) => write!(f, "missing sidecar hash file: {path}"),
        }
    }
}

pub fn manifest_canonical_bytes_v1(manifest: &SearchCampaignManifestV1) -> Vec<u8> {
    serde_json::to_vec(manifest).expect("SearchCampaignManifestV1 always serializes")
}

pub fn manifest_sha256_v1(manifest: &SearchCampaignManifestV1) -> String {
    let digest = Sha256::digest(manifest_canonical_bytes_v1(manifest));
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Loads a manifest JSON file plus its sidecar `<path>.sha256` (one line,
/// the lower-hex digest) and refuses to return the manifest unless the
/// recomputed hash matches exactly (design section 4: "no candidate scores
/// before this manifest is committed and hashed").
pub fn load_and_verify_manifest_v1(
    path: &std::path::Path,
) -> Result<SearchCampaignManifestV1, SearchCampaignErrorV1> {
    let raw = std::fs::read_to_string(path).map_err(|error| SearchCampaignErrorV1::Io(error.to_string()))?;
    let hash_path = path.with_extension("json.sha256");
    let recorded = std::fs::read_to_string(&hash_path)
        .map_err(|_| SearchCampaignErrorV1::MissingHashFile(hash_path.display().to_string()))?;
    let recorded = recorded.trim().to_owned();
    // Hash the raw bytes exactly as read from disk, not a freshly
    // re-serialized copy of the parsed struct: `validate_search_campaign_manifest_v1.py`'s
    // `sha256_hex` (Step 16) hashes `path.read_bytes()`, the raw on-disk
    // bytes of a pretty-printed, 2-space-indented file (Step 15). Hashing a
    // compact `serde_json::to_vec` re-serialization here, as
    // `manifest_sha256_v1` does for constructing a manifest
    // programmatically, would never agree with the Python sidecar for a
    // real committed file, since the two encodings are byte-different even
    // when they parse to the same struct.
    let actual = {
        let digest = Sha256::digest(raw.as_bytes());
        digest.iter().map(|byte| format!("{byte:02x}")).collect::<String>()
    };
    if recorded != actual {
        return Err(SearchCampaignErrorV1::HashMismatch { expected: recorded, actual });
    }
    let manifest: SearchCampaignManifestV1 =
        serde_json::from_str(&raw).map_err(|error| SearchCampaignErrorV1::Json(error.to_string()))?;
    Ok(manifest)
}

pub fn candidate_seed_v1(
    master_seed: u64,
    self_deck_id: &str,
    opponent_deck_id: &str,
    game_index: u8,
    candidate_index: u32,
) -> u64 {
    let mut hasher = Sha256::new();
    hasher.update(b"kernel-sideboard-search-candidate-seed/v1");
    hasher.update(master_seed.to_be_bytes());
    hasher.update(self_deck_id.as_bytes());
    hasher.update([0u8]);
    hasher.update(opponent_deck_id.as_bytes());
    hasher.update([0u8]);
    hasher.update([game_index]);
    hasher.update(candidate_index.to_be_bytes());
    let digest = hasher.finalize();
    u64::from_be_bytes(digest[0..8].try_into().expect("sha256 digest is at least 8 bytes"))
}

pub fn embedding_status_v1(card_ids: &[u16]) -> Vec<(u16, bool)> {
    let trained: std::collections::BTreeSet<u16> = crate::runtime_decks::RUNTIME_DECKS
        .iter()
        .flat_map(|deck| deck.card_ids.iter().copied())
        .collect();
    card_ids.iter().map(|&card_id| (card_id, trained.contains(&card_id))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_manifest() -> SearchCampaignManifestV1 {
        SearchCampaignManifestV1 {
            schema: SEARCH_CAMPAIGN_MANIFEST_SCHEMA_V1.to_owned(),
            checkpoint_weights_hash: "deadbeefdeadbeefdeadbeefdeadbeef".to_owned(),
            checkpoint_git_head: "0".repeat(40),
            k_max_candidates_per_cell: 32,
            max_iterations_per_cell: 64,
            traversal_order: "ascending_card_id_one_swap_then_two_swap".to_owned(),
            master_seed: 0x5150_5150_5150_5150,
            n_per_cell: 400,
            n_derivation: NDerivationV1 {
                minimum_win_rate_delta: 0.05,
                target_power: 0.8,
                calibration_mean_seconds_per_game: 0.42,
                formula: "n = ceil((z_alpha + z_power)^2 * variance / minimum_win_rate_delta^2)".to_owned(),
            },
            m_bo3_matches_per_ratification: 60,
            warm_start_plan_inputs_sha256: "0".repeat(64),
            bo1_one_sided_alpha: 0.05,
            bo3_confidence_level: 0.95,
            bootstrap_resample_count: 10_000,
            bootstrap_seed: 0xC0FF_EEC0_FFEE_C0FF,
            bootstrap_resampling_method: "case_resampling_with_replacement".to_owned(),
            bo1_bootstrap_sidedness: BootstrapSidednessV1::OneSidedLower,
            bo3_bootstrap_sidedness: BootstrapSidednessV1::TwoSided,
        }
    }

    /// `sample_manifest()` with `n_per_cell` overridden, for the driver
    /// tests (Step 13) that need a small, fast trial count.
    fn sample_manifest_with_n_per_cell_v1(n_per_cell: u32) -> SearchCampaignManifestV1 {
        SearchCampaignManifestV1 { n_per_cell, ..sample_manifest() }
    }

    #[test]
    fn manifest_sha256_is_deterministic_and_sensitive_to_every_field() {
        let a = sample_manifest();
        let mut b = sample_manifest();
        assert_eq!(manifest_sha256_v1(&a), manifest_sha256_v1(&b));
        b.n_per_cell += 1;
        assert_ne!(manifest_sha256_v1(&a), manifest_sha256_v1(&b));
    }

    #[test]
    fn load_and_verify_manifest_rejects_a_tampered_file() {
        let dir = std::env::temp_dir().join(format!("search_manifest_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let manifest = sample_manifest();
        let path = dir.join("manifest.json");
        std::fs::write(&path, manifest_canonical_bytes_v1(&manifest)).unwrap();
        std::fs::write(path.with_extension("json.sha256"), manifest_sha256_v1(&manifest)).unwrap();
        let loaded = load_and_verify_manifest_v1(&path).expect("untampered manifest loads");
        assert_eq!(loaded, manifest);

        // Tamper with the file on disk without updating the sidecar hash.
        let mut tampered_bytes = manifest_canonical_bytes_v1(&manifest);
        let mut tampered: SearchCampaignManifestV1 = serde_json::from_slice(&tampered_bytes).unwrap();
        tampered.k_max_candidates_per_cell += 1;
        tampered_bytes = manifest_canonical_bytes_v1(&tampered);
        std::fs::write(&path, tampered_bytes).unwrap();
        let error = load_and_verify_manifest_v1(&path).expect_err("tampered manifest must be refused");
        assert!(matches!(error, SearchCampaignErrorV1::HashMismatch { .. }));
    }

    #[test]
    fn candidate_seed_is_a_pure_function_reproducible_from_the_manifest_alone() {
        let a = candidate_seed_v1(0x1234, "RallyV2", "Wildfire", 2, 7);
        let b = candidate_seed_v1(0x1234, "RallyV2", "Wildfire", 2, 7);
        assert_eq!(a, b);
        let different_candidate = candidate_seed_v1(0x1234, "RallyV2", "Wildfire", 2, 8);
        assert_ne!(a, different_candidate);
        let different_cell = candidate_seed_v1(0x1234, "RallyV2", "Wildfire", 3, 7);
        assert_ne!(a, different_cell);
    }

    #[test]
    fn embedding_status_reports_pulse_of_murasa_as_trained_via_wildfire_mainboard() {
        // design section 1's cross-deck example: Pulse of Murasa is
        // sideboard-only for Elves but mainboard for Wildfire, so its row
        // is trained by the global, catalog-wide definition.
        let pulse_of_murasa_id = crate::card_def::card_id_by_name("Pulse of Murasa")
            .expect("Pulse of Murasa is registered");
        let status = embedding_status_v1(&[pulse_of_murasa_id]);
        assert_eq!(status, vec![(pulse_of_murasa_id, true)]);
    }
}
```

- [ ] **Step 3: Run the tests.**
```
cmd /c start /belownormal /affinity FF /wait cargo test --locked -p mtg-kernel --lib sideboard_search_campaign_v1 2>&1 | tail -60
```
Expected: PASS (this step's code has no forward references to not-yet-written functions, unlike Task D's Step 4/5 split, so there is no separate red state beyond fixing real compile errors against the actual `sideboard.rs`/`runtime_decks.rs` signatures confirmed in Step 1). If `card_id_by_name("Pulse of Murasa")` returns `None`, confirm the exact registered name with `grep -n "Pulse of Murasa" data/cards_v1.json` and correct the literal.

- [ ] **Step 4: Write the candidate generator's failing test**, appended to `mod tests`:
```rust
    use crate::sideboard::checked_in_pauper_registered_deck_by_id_v1;

    #[test]
    fn one_swap_candidates_are_legal_plans_restricted_to_the_registered_75_and_deterministic() {
        let registered = checked_in_pauper_registered_deck_by_id_v1("Burn").expect("Burn is checked in");
        // k = 2 is provably smaller than Burn's total legal one-swap count:
        // Burn's registered 75 has 60 mainboard and 15 sideboard cards, so
        // mainboard_ids.len() * sideboard_ids.len() (an upper bound on
        // distinct card_id pairs, before deduplication) is far larger than
        // 2. `assert_eq!(a.len(), k)` therefore actually fails if the cap
        // in `generate_one_swap_candidates_v1` is removed or broken, unlike
        // an `a.len() == k.min(a.len())` tautology.
        let k = 2;
        let a = generate_one_swap_candidates_v1(&registered, "Rally", 2, k);
        let b = generate_one_swap_candidates_v1(&registered, "Rally", 2, k);
        assert_eq!(a.len(), k as usize, "the cap must actually fire at exactly k candidates");
        for (candidate_a, candidate_b) in a.iter().zip(b.iter()) {
            assert_eq!(
                candidate_a.cards_in(), candidate_b.cards_in(),
                "same traversal order and k must reproduce the identical candidate list"
            );
        }
        let sideboard_ids: std::collections::BTreeSet<u16> =
            registered.registered_configuration().sideboard().iter().copied().collect();
        let mainboard_ids: std::collections::BTreeSet<u16> =
            registered.registered_configuration().mainboard().iter().copied().collect();
        for plan in &a {
            for row in plan.cards_in() {
                assert!(
                    sideboard_ids.contains(&row.card_id),
                    "a candidate's cards_in must come from Burn's own registered sideboard, card_id {}", row.card_id
                );
            }
            for row in plan.cards_out() {
                assert!(
                    mainboard_ids.contains(&row.card_id),
                    "a candidate's cards_out must come from Burn's own registered mainboard, card_id {}", row.card_id
                );
            }
            // apply_plan_v1's conservation rule is satisfiable by
            // construction: applying every candidate must succeed.
            registered
                .apply_plan_v1(plan, "search-candidate-check/v1", [0u8; 32])
                .expect("every generated candidate applies cleanly");
        }
    }
```
Run: `cmd /c start /belownormal /affinity FF /wait cargo test --locked -p mtg-kernel --lib sideboard_search_campaign_v1::tests::one_swap_candidates 2>&1 | tail -40`. Expected: FAIL (`generate_one_swap_candidates_v1` and `plan.cards_in()`/`plan.cards_out()` accessors do not exist yet).

- [ ] **Step 5: Check `SideboardPlanV1`'s accessors before adding new ones.**
```
grep -n "impl SideboardPlanV1" -A 40 mtg-kernel/src/sideboard.rs | grep -n "pub fn"
```
If `cards_in`/`cards_out` accessors already exist under different names (the struct's fields are private per the earlier grounding read), use those real names in Step 4's test instead of inventing `cards_in()`/`cards_out()`; if no accessor exists at all, add `pub fn cards_in(&self) -> &[CardCountV1] { &self.cards_in }` and `pub fn cards_out(&self) -> &[CardCountV1] { &self.cards_out }` to `impl SideboardPlanV1` in `sideboard.rs` as a small, justified addition (needed by this task's generator and its test; not used to bypass any existing validation), and record this file in this task's Files list.

- [ ] **Step 6: Implement `generate_one_swap_candidates_v1`**, added to `sideboard_search_campaign_v1.rs` above `mod tests`:
```rust
/// Bounded 1-card swaps: for every mainboard card_id (ascending) and every
/// sideboard card_id (ascending) not already equal to it, a candidate that
/// swaps exactly one copy out for exactly one copy in. Restricted to cards
/// already present in the deck's own registered 75 (design section 4), so
/// `apply_plan_v1`'s conservation rule is satisfiable by construction.
/// Deterministic ascending-card-id traversal order, capped at `k`
/// candidates; this function only enumerates, it never scores.
pub fn generate_one_swap_candidates_v1(
    registered: &RegisteredDeckV1,
    opponent_deck_id: &str,
    game_index: u8,
    k: u32,
) -> Vec<SideboardPlanV1> {
    let configuration = registered.registered_configuration();
    let mut mainboard_ids: Vec<u16> = configuration.mainboard().to_vec();
    mainboard_ids.sort_unstable();
    mainboard_ids.dedup();
    let mut sideboard_ids: Vec<u16> = configuration.sideboard().to_vec();
    sideboard_ids.sort_unstable();
    sideboard_ids.dedup();

    let mut candidates = Vec::new();
    'outer: for &out_id in &mainboard_ids {
        for &in_id in &sideboard_ids {
            if in_id == out_id {
                continue;
            }
            let plan = SideboardPlanV1::new_v1(
                registered.deck_id().to_owned(),
                opponent_deck_id.to_owned(),
                game_index,
                vec![CardCountV1 { card_id: in_id, count: 1 }],
                vec![CardCountV1 { card_id: out_id, count: 1 }],
            );
            if let Ok(plan) = plan {
                candidates.push(plan);
                if candidates.len() as u32 >= k {
                    break 'outer;
                }
            }
        }
    }
    candidates
}
```

- [ ] **Step 7: Run the test.**
```
cmd /c start /belownormal /affinity FF /wait cargo test --locked -p mtg-kernel --lib sideboard_search_campaign_v1 2>&1 | tail -60
```
Expected: all tests in the module PASS.

- [ ] **Step 8: Write failing tests for multi-swap candidates, warm-start loading, and the bounded hill-climb**, appended to `mod tests`. Design section 4 requires "warm-start from `docs/research/sideboard_plan_inputs_2026-09/plan_table_draft_nine_v1.json`, then local search: bounded 1-to-4-card swaps... Greedy hill-climbing, incumbent replaced only on acceptance, bounded by a fixed max-iteration count per cell"; `generate_one_swap_candidates_v1` alone (Step 6) only covers the 1-card case, so this step adds the rest:
```rust
    #[test]
    fn multi_swap_candidates_swap_exactly_degree_cards_and_stay_within_the_registered_75() {
        let registered = checked_in_pauper_registered_deck_by_id_v1("Burn").expect("Burn is checked in");
        for degree in [2usize, 3, 4] {
            let candidates = generate_multi_swap_candidates_v1(&registered, "Rally", 2, degree, 3);
            assert!(!candidates.is_empty(), "degree {degree} must produce at least one candidate");
            for plan in &candidates {
                assert_eq!(plan.cards_in().len(), degree);
                assert_eq!(plan.cards_out().len(), degree);
                registered
                    .apply_plan_v1(plan, "multi-swap-check/v1", [0u8; 32])
                    .unwrap_or_else(|error| panic!("degree-{degree} candidate must apply cleanly: {error}"));
            }
        }
    }

    #[test]
    fn hill_climb_search_terminates_within_the_iteration_bound_and_converges_on_a_monotonic_score() {
        // Generic over a plain `i32` "score" rather than `SideboardPlanV1`:
        // this exercises the search loop's own termination and acceptance
        // logic in isolation, independent of any deck-specific scoring.
        let always_climb_to_ten = |current: &i32, candidate: &i32| *candidate <= 10 && *candidate > *current;
        let neighborhood = |current: &i32| vec![current + 1];
        let converged = hill_climb_search_v1(0i32, neighborhood, 100, always_climb_to_ten);
        assert_eq!(converged, 10, "hill-climb converges to the ceiling the acceptance function allows");

        let always_accept = |_current: &i32, _candidate: &i32| true;
        let bounded = hill_climb_search_v1(0i32, |current: &i32| vec![current + 1], 3, always_accept);
        assert_eq!(bounded, 3, "with max_iterations = 3 and an always-accept function, exactly 3 steps run");
    }

    #[test]
    fn warm_start_plan_inputs_load_and_verify_against_their_own_sha256() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../docs/research/sideboard_plan_inputs_2026-09/plan_table_draft_nine_v1.json");
        let raw = std::fs::read_to_string(&path).expect("warm-start plan-inputs file exists (Step 9 creates it)");
        let expected = {
            let digest = Sha256::digest(raw.as_bytes());
            digest.iter().map(|byte| format!("{byte:02x}")).collect::<String>()
        };
        let plans = load_warm_start_plan_inputs_v1(&path, &expected)
            .expect("the real file must load against its own freshly computed hash");
        assert!(!plans.is_empty(), "the warm-start file must seed at least one cell");
        let wrong_hash = "0".repeat(64);
        let error = load_warm_start_plan_inputs_v1(&path, &wrong_hash).expect_err("a wrong hash must be refused");
        assert!(matches!(error, SearchCampaignErrorV1::HashMismatch { .. }));
    }
```
Run: `cmd /c start /belownormal /affinity FF /wait cargo test --locked -p mtg-kernel --lib sideboard_search_campaign_v1 2>&1 | tail -60`. Expected: FAIL (`generate_multi_swap_candidates_v1`, `hill_climb_search_v1`, `load_warm_start_plan_inputs_v1` not defined; the warm-start fixture file also does not exist yet).

- [ ] **Step 9: Create the warm-start fixture and implement multi-swap candidates, the hill-climb, and the warm-start loader.** Create `docs/research/sideboard_plan_inputs_2026-09/plan_table_draft_nine_v1.json`, a minimal real fixture (W6's actual campaign substitutes a fuller researched table under the same path and re-hashes per Step 19; this plan only needs a schema-conformant, loadable file for the tests above):
```json
{
  "schema": "kernel_sideboard_plan_inputs/v1",
  "rows": [
    {
      "self_deck_id": "Burn",
      "opponent_deck_id": "Rally",
      "game_index": 2,
      "cards_in": [],
      "cards_out": []
    }
  ]
}
```
Add to `sideboard_search_campaign_v1.rs` above `mod tests`, directly below `generate_one_swap_candidates_v1`:
```rust
/// Bounded 2-to-4-card swaps, same restriction and traversal discipline as
/// `generate_one_swap_candidates_v1`: swaps of `degree` cards (2, 3, or 4)
/// out for `degree` cards in, both multisets drawn only from the deck's own
/// registered mainboard/sideboard, enumerated in ascending-card-id
/// combination order, capped at `k`. Enumerates only; never scores.
pub fn generate_multi_swap_candidates_v1(
    registered: &RegisteredDeckV1,
    opponent_deck_id: &str,
    game_index: u8,
    degree: usize,
    k: u32,
) -> Vec<SideboardPlanV1> {
    assert!((2..=4).contains(&degree), "degree must be 2, 3, or 4");
    let configuration = registered.registered_configuration();
    let mut mainboard_ids: Vec<u16> = configuration.mainboard().to_vec();
    mainboard_ids.sort_unstable();
    mainboard_ids.dedup();
    let mut sideboard_ids: Vec<u16> = configuration.sideboard().to_vec();
    sideboard_ids.sort_unstable();
    sideboard_ids.dedup();

    let mut candidates = Vec::new();
    for out_combo in ascending_combinations_v1(&mainboard_ids, degree) {
        for in_combo in ascending_combinations_v1(&sideboard_ids, degree) {
            if out_combo.iter().any(|id| in_combo.contains(id)) {
                continue;
            }
            let plan = SideboardPlanV1::new_v1(
                registered.deck_id().to_owned(),
                opponent_deck_id.to_owned(),
                game_index,
                in_combo.iter().map(|&card_id| CardCountV1 { card_id, count: 1 }).collect(),
                out_combo.iter().map(|&card_id| CardCountV1 { card_id, count: 1 }).collect(),
            );
            if let Ok(plan) = plan {
                candidates.push(plan);
                if candidates.len() as u32 >= k {
                    return candidates;
                }
            }
        }
    }
    candidates
}

/// Ascending-order combinations of size `degree` from a sorted,
/// deduplicated slice, smallest index-tuple first (the standard
/// next-combination algorithm). Hand-rolled so this task adds no new
/// external dependency (Global Constraints).
fn ascending_combinations_v1(items: &[u16], degree: usize) -> Vec<Vec<u16>> {
    if degree == 0 || degree > items.len() {
        return Vec::new();
    }
    let mut result = Vec::new();
    let mut indices: Vec<usize> = (0..degree).collect();
    loop {
        result.push(indices.iter().map(|&i| items[i]).collect());
        let mut cursor = degree;
        loop {
            if cursor == 0 {
                return result;
            }
            cursor -= 1;
            if indices[cursor] != cursor + items.len() - degree {
                break;
            }
        }
        indices[cursor] += 1;
        for next in (cursor + 1)..degree {
            indices[next] = indices[next - 1] + 1;
        }
    }
}

/// Greedy hill-climbing local search bounded by `max_iterations` (design
/// section 4: "incumbent replaced only on acceptance, bounded by a fixed
/// max-iteration count per cell"), warm-started from `incumbent`. Generic
/// over the candidate type so it is unit-testable against a synthetic,
/// deterministic `accept_fn` independent of any deck-specific scoring; the
/// real driver (Step 11) instantiates it at `T = SideboardPlanV1` with an
/// `accept_fn` backed by `paired_bootstrap_ci_v1`.
pub fn hill_climb_search_v1<T: Clone>(
    incumbent: T,
    neighborhood: impl Fn(&T) -> Vec<T>,
    max_iterations: u32,
    mut accept_fn: impl FnMut(&T, &T) -> bool,
) -> T {
    let mut current = incumbent;
    for _ in 0..max_iterations {
        let candidates = neighborhood(&current);
        let mut replaced = false;
        for candidate in candidates {
            if accept_fn(&current, &candidate) {
                current = candidate;
                replaced = true;
                break;
            }
        }
        if !replaced {
            break;
        }
    }
    current
}

#[derive(Debug, Clone, Deserialize)]
struct WarmStartPlanRowV1 {
    self_deck_id: String,
    opponent_deck_id: String,
    game_index: u8,
    cards_in: Vec<CardCountV1>,
    cards_out: Vec<CardCountV1>,
}

#[derive(Debug, Clone, Deserialize)]
struct WarmStartPlanInputsV1 {
    rows: Vec<WarmStartPlanRowV1>,
}

/// Loads and sha256-verifies the warm-start plan-inputs file (design
/// section 4) against `expected_sha256` (the manifest's
/// `warm_start_plan_inputs_sha256`), refusing to proceed on any mismatch,
/// then parses it into one warm-start `SideboardPlanV1` per row. A row
/// whose `SideboardPlanV1::new_v1` construction fails is skipped rather
/// than aborting the whole load, since a malformed single row should not
/// block every other cell's warm start.
pub fn load_warm_start_plan_inputs_v1(
    path: &std::path::Path,
    expected_sha256: &str,
) -> Result<Vec<SideboardPlanV1>, SearchCampaignErrorV1> {
    let raw = std::fs::read_to_string(path).map_err(|error| SearchCampaignErrorV1::Io(error.to_string()))?;
    let actual = {
        let digest = Sha256::digest(raw.as_bytes());
        digest.iter().map(|byte| format!("{byte:02x}")).collect::<String>()
    };
    if actual != expected_sha256 {
        return Err(SearchCampaignErrorV1::HashMismatch { expected: expected_sha256.to_owned(), actual });
    }
    let parsed: WarmStartPlanInputsV1 =
        serde_json::from_str(&raw).map_err(|error| SearchCampaignErrorV1::Json(error.to_string()))?;
    let mut plans = Vec::new();
    for row in parsed.rows {
        if let Ok(plan) =
            SideboardPlanV1::new_v1(row.self_deck_id, row.opponent_deck_id, row.game_index, row.cards_in, row.cards_out)
        {
            plans.push(plan);
        }
    }
    Ok(plans)
}
```
Run: `cmd /c start /belownormal /affinity FF /wait cargo test --locked -p mtg-kernel --lib sideboard_search_campaign_v1 2>&1 | tail -60`. Expected: all tests, including Step 8's three new ones, PASS. Commit the warm-start fixture in this task's final commit (Step 21); note that `sha256sum docs/research/sideboard_plan_inputs_2026-09/plan_table_draft_nine_v1.json` is what the manifest's `warm_start_plan_inputs_sha256` field must equal once W6 substitutes real values (Step 19 documents the exact procedure for filling in every such placeholder).

- [ ] **Step 10: Write failing tests for the opponent-side resolution rule, opponent embedding disclosure, and the N-derivation formula**, appended to `mod tests`. Design section 4's "Freezing and disclosure" paragraph requires a `hash_convention` field, an `opponent_embedding_status` list "whenever the accepted-opponent-plan variant was used," which opponent-side resolution variant was used, and an executable N-derivation, none of which existed in this task before this step:
```rust
    #[test]
    fn resolve_opponent_mainboard_uses_the_registered_mainboard_unconditionally_at_game_index_one() {
        let policy = DeterministicSideboardPolicyV1::checked_in_pauper_v1().unwrap();
        let (mainboard, variant) =
            resolve_opponent_mainboard_for_cell_v1("Rally", "Burn", 1, &policy).unwrap();
        let expected = checked_in_pauper_registered_deck_by_id_v1("Rally")
            .unwrap()
            .registered_configuration()
            .mainboard()
            .to_vec();
        assert_eq!(mainboard, expected);
        assert_eq!(variant, OpponentResolutionVariantV1::RegisteredMainboard);
    }

    #[test]
    fn resolve_opponent_mainboard_at_game_index_two_is_consistent_with_its_own_reported_variant() {
        // Whether Burn-vs-Rally has a BO3-ratified game-2 plan checked in
        // at this plan's writing is unknown (no search campaign has run;
        // W6 is out of scope here), so this test asserts internal
        // consistency between the returned mainboard and the variant that
        // explains it, not one specific outcome.
        let policy = DeterministicSideboardPolicyV1::checked_in_pauper_v1().unwrap();
        let (mainboard, variant) =
            resolve_opponent_mainboard_for_cell_v1("Rally", "Burn", 2, &policy).unwrap();
        match variant {
            OpponentResolutionVariantV1::RegisteredMainboard => {
                let expected = checked_in_pauper_registered_deck_by_id_v1("Rally")
                    .unwrap()
                    .registered_configuration()
                    .mainboard()
                    .to_vec();
                assert_eq!(mainboard, expected);
            }
            OpponentResolutionVariantV1::Bo3RatifiedPlan => {
                assert_eq!(mainboard.len(), crate::sideboard::REGISTERED_MAINBOARD_SIZE_V1);
            }
        }
    }

    #[test]
    fn opponent_embedding_status_reports_pulse_of_murasa_as_trained_via_wildfire_mainboard() {
        let pulse_of_murasa_id = crate::card_def::card_id_by_name("Pulse of Murasa")
            .expect("Pulse of Murasa is registered");
        let status = opponent_embedding_status_v1(&[pulse_of_murasa_id]);
        assert_eq!(status, vec![(pulse_of_murasa_id, true)]);
    }

    #[test]
    fn derive_n_per_cell_matches_a_hand_computed_value_at_a_concrete_delta_power_alpha_triple() {
        // z_{0.95} ~ 1.6448536, z_{0.8} ~ 0.8416212 (standard normal
        // quantiles); n = ceil((1.6448536 + 0.8416212)^2 * 0.25 / 0.05^2)
        // = ceil(618.25...) = 619.
        let n = derive_n_per_cell_v1(0.05, 0.8, 0.05);
        assert_eq!(n, 619);
    }
```
Run: `cmd /c start /belownormal /affinity FF /wait cargo test --locked -p mtg-kernel --lib sideboard_search_campaign_v1 2>&1 | tail -60`. Expected: FAIL (`resolve_opponent_mainboard_for_cell_v1`, `OpponentResolutionVariantV1`, `opponent_embedding_status_v1`, `derive_n_per_cell_v1` not defined).

- [ ] **Step 11: Implement the opponent-side resolution rule, opponent embedding disclosure, the receipt-fields type, and the N-derivation formula**, added above `mod tests`:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpponentResolutionVariantV1 {
    RegisteredMainboard,
    Bo3RatifiedPlan,
}

/// Design section 2's opponent-side resolution rule: `game_index` 1 always
/// uses the opponent's registered mainboard unconditionally; `game_index`
/// 2 or 3 uses the opponent's best BO3-ratified post-board plan for
/// `(opponent_deck_id, self_deck_id)` when `sideboard_policy` has one,
/// falling back to the registered mainboard otherwise. `plan_for_v1`
/// (`sideboard.rs:523`) already returns a `keep_registered_v1`-shaped plan
/// (empty `cards_in`/`cards_out`) when no specific plan matches, via its
/// own `SideboardDefaultPlanV1::KeepRegisteredConfiguration` fallback, so
/// an empty plan is this function's own signal to report
/// `RegisteredMainboard` rather than `Bo3RatifiedPlan`.
pub fn resolve_opponent_mainboard_for_cell_v1(
    opponent_deck_id: &str,
    self_deck_id: &str,
    game_index: u8,
    sideboard_policy: &crate::sideboard::DeterministicSideboardPolicyV1,
) -> Result<(Vec<u16>, OpponentResolutionVariantV1), SideboardErrorV1> {
    let opponent_registered = crate::sideboard::checked_in_pauper_registered_deck_by_id_v1(opponent_deck_id)?;
    if game_index == 1 {
        return Ok((
            opponent_registered.registered_configuration().mainboard().to_vec(),
            OpponentResolutionVariantV1::RegisteredMainboard,
        ));
    }
    let plan = sideboard_policy.plan_for_v1(opponent_deck_id, self_deck_id, game_index)?;
    if plan.cards_in().is_empty() && plan.cards_out().is_empty() {
        return Ok((
            opponent_registered.registered_configuration().mainboard().to_vec(),
            OpponentResolutionVariantV1::RegisteredMainboard,
        ));
    }
    let (configuration, _receipt) =
        opponent_registered.apply_plan_v1(&plan, "search-driver-opponent-resolution/v1", [0u8; 32])?;
    Ok((configuration.mainboard().to_vec(), OpponentResolutionVariantV1::Bo3RatifiedPlan))
}

/// Design section 4's disclosure requirement: "an `opponent_embedding_status`
/// list computed the same way as `embedding_status`... whenever the
/// accepted-opponent-plan variant was used." Identical trained-check logic
/// to `embedding_status_v1`, kept as a separately named function so a
/// receipt row's two lists are never confused with each other by call site.
pub fn opponent_embedding_status_v1(opponent_plan_cards_in: &[u16]) -> Vec<(u16, bool)> {
    embedding_status_v1(opponent_plan_cards_in)
}

/// The hash convention that produced a candidate's identity hash (design
/// section 4: "a `hash_convention` field recording which convention...").
/// Every candidate this task's generators produce is applied through
/// `apply_plan_v1` onto a `RegisteredDeckV1` sourced from the checked-in
/// pool, then, when scored, resolved into an explicit deck via Task A's
/// `resolve_explicit_decks` seam, which always hashes with
/// `explicit_deck_hash_v1`'s `sorted-explicit` convention (Task A). This
/// harness therefore only ever produces `sorted-explicit` receipts; the
/// `catalog-materialized` convention exists only for already-catalog-
/// resolved decks, never search candidates.
pub const CANDIDATE_HASH_CONVENTION_SORTED_EXPLICIT_V1: &str = "sorted-explicit";

#[derive(Debug, Clone, Serialize)]
pub struct CandidateReceiptFieldsV1 {
    pub hash_convention: String,
    pub embedding_status: Vec<(u16, bool)>,
    pub opponent_embedding_status: Option<Vec<(u16, bool)>>,
    pub opponent_resolution_variant: OpponentResolutionVariantV1,
}

/// Rational approximation of the inverse standard normal CDF (Peter
/// Acklam's algorithm, accurate to about 1.15e-9), used only by
/// `derive_n_per_cell_v1`. Self-contained: no new external dependency
/// (Global Constraints).
fn inverse_normal_cdf_v1(p: f64) -> f64 {
    assert!(p > 0.0 && p < 1.0, "inverse_normal_cdf_v1 is defined on (0, 1)");
    const A: [f64; 6] = [-3.969683028665376e+01, 2.209460984245205e+02, -2.759285104469687e+02, 1.383577518672690e+02, -3.066479806614716e+01, 2.506628277459239e+00];
    const B: [f64; 5] = [-5.447609879822406e+01, 1.615858368580409e+02, -1.556989798598866e+02, 6.680131188771972e+01, -1.328068155288572e+01];
    const C: [f64; 6] = [-7.784894002430293e-03, -3.223964580411365e-01, -2.400758277161838e+00, -2.549732539343734e+00, 4.374664141464968e+00, 2.938163982698783e+00];
    const D: [f64; 4] = [7.784695709041462e-03, 3.224671290700398e-01, 2.445134137142996e+00, 3.754408661907416e+00];
    let p_low = 0.02425;
    let p_high = 1.0 - p_low;
    if p < p_low {
        let q = (-2.0 * p.ln()).sqrt();
        (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    } else if p <= p_high {
        let q = p - 0.5;
        let r = q * q;
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q
            / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
    } else {
        let q = (-2.0 * (1.0 - p).ln()).sqrt();
        -(((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    }
}

/// Executable counterpart of `NDerivationV1.formula`
/// ("n = ceil((z_alpha + z_power)^2 * variance / minimum_win_rate_delta^2),
/// variance = p(1-p) at p = 0.5"): design section 4, "N is a power
/// calculation, not a budget-driven guess."
pub fn derive_n_per_cell_v1(minimum_win_rate_delta: f64, target_power: f64, one_sided_alpha: f64) -> u32 {
    assert!(minimum_win_rate_delta > 0.0);
    assert!(target_power > 0.0 && target_power < 1.0);
    assert!(one_sided_alpha > 0.0 && one_sided_alpha < 1.0);
    let z_alpha = inverse_normal_cdf_v1(1.0 - one_sided_alpha);
    let z_power = inverse_normal_cdf_v1(target_power);
    let variance = 0.5 * 0.5;
    let n = (z_alpha + z_power).powi(2) * variance / minimum_win_rate_delta.powi(2);
    n.ceil() as u32
}
```
Run: `cmd /c start /belownormal /affinity FF /wait cargo test --locked -p mtg-kernel --lib sideboard_search_campaign_v1 2>&1 | tail -60`. Expected: all tests PASS. If `card_id_by_name("Pulse of Murasa")` returns `None`, confirm the exact registered name with `grep -n "Pulse of Murasa" data/cards_v1.json` and correct the literal (the same caveat Step 3 already carries for its own `embedding_status_v1` test).

- [ ] **Step 12: Write the BO1-provisional / BO3-ratification driver's failing test**, appended to `mod tests`. This wires Task A/B's paired estimator to `apply_plan_v1` (for a candidate's mainboard) and to `BestOfThreeDeckMatchV1::prepare_game_v1` (for BO3 ratification), matching design section 4's two-stage acceptance rule and its `PreparedMatchGameV1::start().starting_player` binding:
```rust
    use crate::bo3_session::BestOfThreeDeckMatchV1;
    use crate::bo3_match::{GameOutcomeV1, PlayDrawChoiceV1};
    use crate::ids::PlayerId;
    use crate::paired_bo1_harness_v1::{run_paired_bo1_trial_v1, PairedTrialOutcomeV1};
    use crate::rl_session::FastActorDecisionV1;
    use crate::state::SplitMix64;

    fn random_policy_v1(seed: u64) -> impl FnMut(&FastActorDecisionV1) -> u32 {
        let mut rng = SplitMix64::seed(seed);
        move |decision: &FastActorDecisionV1| (rng.next_u64() as u32) % decision.legal_action_count.max(1)
    }

    #[test]
    fn a_candidate_mainboard_from_apply_plan_v1_drives_a_real_paired_bo1_trial() {
        let registered = checked_in_pauper_registered_deck_by_id_v1("Burn").unwrap();
        let candidates = generate_one_swap_candidates_v1(&registered, "Rally", 2, 1);
        let candidate_plan = candidates.first().expect("at least one candidate exists");
        let (candidate_configuration, _receipt) = registered
            .apply_plan_v1(candidate_plan, "search-driver-check/v1", [0u8; 32])
            .unwrap();
        let incumbent_mainboard = registered.registered_configuration().mainboard().to_vec();
        let opponent_mainboard = checked_in_pauper_registered_deck_by_id_v1("Rally")
            .unwrap()
            .registered_configuration()
            .mainboard()
            .to_vec();
        let seed = candidate_seed_v1(0xABCD, "Burn", "Rally", 2, 0);
        let mut policy = random_policy_v1(seed);
        let outcome: PairedTrialOutcomeV1 = run_paired_bo1_trial_v1(
            candidate_configuration.mainboard(),
            &incumbent_mainboard,
            &opponent_mainboard,
            PlayerId::P0,
            seed,
            PlayerId::P0,
            2000,
            &mut policy,
        )
        .expect("paired trial completes on a real generated candidate");
        assert!((-1..=1).contains(&outcome.delta));
    }

    #[test]
    fn bo3_ratification_uses_the_bo3_assigned_starting_player_for_every_game() {
        let match_session_result = BestOfThreeDeckMatchV1::checked_in_pauper_v1("Burn", "Rally", PlayerId::P0);
        let mut match_session = match match_session_result {
            Ok(session) => session,
            Err(error) => panic!(
                "Burn vs Rally must have a ratified game-2 sideboard plan for this test to run: {error}"
            ),
        };
        let game = match_session
            .prepare_game_v1(PlayerId::P0, PlayDrawChoiceV1::Play)
            .expect("game 1 prepares");
        let start = game.start();
        assert_eq!(start.game_index, 1);
        assert_eq!(start.starting_player, PlayerId::P0, "PlayDrawChoiceV1::Play by the chooser starts that player");
        // The binding this task's driver must honor: every BO3-ratification
        // game is constructed from exactly this starting_player value via
        // Task A's starting-player-aware explicit-deck constructor, never
        // the plain P0-only one.
        let p0_mainboard = game.configuration(PlayerId::P0).unwrap().mainboard().to_vec();
        let p1_mainboard = game.configuration(PlayerId::P1).unwrap().mainboard().to_vec();
        let deck_ids = ["Burn".to_owned(), "Rally".to_owned()];
        let mut session = crate::rl_session::RlEpisodeSessionV1::reset_with_explicit_decks_and_limits_with_starting_player_v1(
            1, 0x9999, 2000, 200_000, deck_ids, [p0_mainboard, p1_mainboard], start.starting_player,
        )
        .expect("bo3-bound explicit-deck reset succeeds");
        assert_eq!(session.game_state().active_player, start.starting_player);
        match_session
            .record_game_result_v1(GameOutcomeV1::Win { winner: PlayerId::P0 })
            .expect("recording game 1's result advances the match");
    }
```

- [ ] **Step 13: Implement the two-stage acceptance driver.** Design section 4/section 8's Global Constraint ("the driver refuses to run against a manifest whose file hash does not match its own recorded hash") and section 4's acceptance rule name two concrete functions that must exist as reusable driver logic, not only as isolated test bodies: the BO1-provisional stage and the BO3-ratification stage. Added above `mod tests`:
```rust
/// Runs `manifest.n_per_cell` paired BO1 trials per candidate (candidate
/// mainboard vs the search's current working-best mainboard, starting from
/// `incumbent`), computes the one-sided paired-bootstrap CI on the
/// resulting deltas, and replaces the working best only when its lower
/// bound exceeds 0 at the manifest's pre-registered discipline (design
/// section 4, BO1 provisional accept). Gated on `load_and_verify_manifest_v1`
/// first: this function never scores a single trial against an unverified
/// manifest.
pub fn run_bo1_provisional_search_for_cell_v1(
    manifest: &SearchCampaignManifestV1,
    manifest_path: &std::path::Path,
    registered: &RegisteredDeckV1,
    incumbent: &SideboardPlanV1,
    candidates: &[SideboardPlanV1],
    opponent_mainboard: &[u16],
    policy_fn: &mut dyn FnMut(&crate::rl_session::FastActorDecisionV1) -> u32,
) -> Result<Option<SideboardPlanV1>, SearchCampaignErrorV1> {
    load_and_verify_manifest_v1(manifest_path)?;
    let mainboard_for = |plan: &SideboardPlanV1| -> Result<Vec<u16>, SearchCampaignErrorV1> {
        let (configuration, _receipt) = registered
            .apply_plan_v1(plan, "search-driver-bo1/v1", [0u8; 32])
            .map_err(|error| SearchCampaignErrorV1::Io(error.to_string()))?;
        Ok(configuration.mainboard().to_vec())
    };
    let mut working_best: Option<SideboardPlanV1> = None;
    let mut working_best_mainboard = mainboard_for(incumbent)?;

    for (candidate_index, candidate) in candidates.iter().enumerate() {
        let candidate_mainboard = mainboard_for(candidate)?;
        let mut deltas: Vec<i8> = Vec::with_capacity(manifest.n_per_cell as usize);
        for trial in 0..manifest.n_per_cell {
            let seed = candidate_seed_v1(
                manifest.master_seed,
                candidate.self_deck_id(),
                candidate.opponent_deck_id(),
                candidate.game_index(),
                candidate_index as u32 * manifest.n_per_cell + trial,
            );
            let outcome = crate::paired_bo1_harness_v1::run_paired_bo1_trial_v1(
                &candidate_mainboard, &working_best_mainboard, opponent_mainboard,
                PlayerId::P0, seed, PlayerId::P0, 2000, policy_fn,
            )
            .map_err(|error| SearchCampaignErrorV1::Io(error.to_string()))?;
            deltas.push(outcome.delta);
        }
        let result = crate::paired_bo1_harness_v1::paired_bootstrap_ci_v1(
            &deltas, manifest.bootstrap_resample_count, manifest.bootstrap_seed, manifest.bo1_bootstrap_sidedness,
        );
        if result.lower > 0.0 {
            working_best_mainboard = candidate_mainboard;
            working_best = Some(candidate.clone());
        }
    }
    Ok(working_best)
}

/// Plays one full BO3 match for `self_mainboard` (seated P0) against
/// `opponent_mainboard` (seated P1): each game is constructed from
/// `PreparedMatchGameV1::start().starting_player` via Task A's
/// starting-player-aware explicit-deck constructor, driven to terminal by
/// `policy_fn`, and its result fed back through `record_game_result_v1`
/// until `MatchTransitionV1::Complete`. Returns whether the `self_mainboard`
/// seat won the match.
fn play_bo3_match_for_seat_v1(
    self_deck_id: &str,
    opponent_deck_id: &str,
    self_mainboard: &[u16],
    opponent_mainboard: &[u16],
    sideboard_policy: &crate::sideboard::DeterministicSideboardPolicyV1,
    pair_environment_seed: u64,
    policy_fn: &mut dyn FnMut(&crate::rl_session::RlSessionDecisionV1) -> (u32, String),
) -> Result<bool, SearchCampaignErrorV1> {
    let self_registered = crate::sideboard::checked_in_pauper_registered_deck_by_id_v1(self_deck_id)
        .map_err(|error| SearchCampaignErrorV1::Io(error.to_string()))?;
    let opponent_registered = crate::sideboard::checked_in_pauper_registered_deck_by_id_v1(opponent_deck_id)
        .map_err(|error| SearchCampaignErrorV1::Io(error.to_string()))?;
    let mut match_session = crate::bo3_session::BestOfThreeDeckMatchV1::new_v1(
        self_registered, opponent_registered, sideboard_policy.clone(), PlayerId::P0,
    )
    .map_err(|error| SearchCampaignErrorV1::Io(format!("{error:?}")))?;

    loop {
        let game = match_session
            .prepare_game_v1(PlayerId::P0, crate::bo3_match::PlayDrawChoiceV1::Play)
            .map_err(|error| SearchCampaignErrorV1::Io(format!("{error:?}")))?;
        let start = game.start();
        let deck_ids = [self_deck_id.to_owned(), opponent_deck_id.to_owned()];
        let mut session = crate::rl_session::RlEpisodeSessionV1::reset_with_explicit_decks_and_limits_with_starting_player_v1(
            u64::from(start.game_index),
            pair_environment_seed ^ u64::from(start.game_index),
            2000,
            200_000,
            deck_ids,
            [self_mainboard.to_vec(), opponent_mainboard.to_vec()],
            start.starting_player,
        )
        .map_err(|error| SearchCampaignErrorV1::Io(error.to_string()))?;

        let winner = loop {
            match session.current_response() {
                crate::rl_session::RlSessionResponseV1::Terminal(terminal) => break terminal.winner,
                crate::rl_session::RlSessionResponseV1::Decision(decision) => {
                    let (selected_index, selected_action_id) = policy_fn(&decision);
                    session
                        .step(decision.episode_id, decision.step, selected_index, &selected_action_id)
                        .map_err(|error| SearchCampaignErrorV1::Io(error.to_string()))?;
                }
            }
        };
        let outcome = match winner {
            Some(crate::rl::PlayerSeatV1::P0) => crate::bo3_match::GameOutcomeV1::Win { winner: PlayerId::P0 },
            Some(crate::rl::PlayerSeatV1::P1) => crate::bo3_match::GameOutcomeV1::Win { winner: PlayerId::P1 },
            None => crate::bo3_match::GameOutcomeV1::Draw,
        };
        match match_session
            .record_game_result_v1(outcome)
            .map_err(|error| SearchCampaignErrorV1::Io(format!("{error:?}")))?
        {
            crate::bo3_match::MatchTransitionV1::NextGameChoice { .. } => continue,
            crate::bo3_match::MatchTransitionV1::Complete { outcome } => {
                return Ok(matches!(outcome, crate::bo3_match::MatchOutcomeV1::Winner { winner: PlayerId::P0 }));
            }
        }
    }
}

/// Runs `manifest.m_bo3_matches_per_ratification` paired BO3 matches
/// (candidate mainboard vs incumbent mainboard, same opponent and shared
/// seed per pair) and ratifies the candidate only when the two-sided
/// paired-bootstrap CI on match-win delta excludes 0 at
/// `manifest.bo3_confidence_level` (design section 4, BO3 ratification).
/// Gated on `load_and_verify_manifest_v1`, exactly like the BO1 stage.
pub fn run_bo3_ratification_v1(
    manifest: &SearchCampaignManifestV1,
    manifest_path: &std::path::Path,
    self_deck_id: &str,
    opponent_deck_id: &str,
    candidate_mainboard: &[u16],
    incumbent_mainboard: &[u16],
    opponent_mainboard: &[u16],
    sideboard_policy: &crate::sideboard::DeterministicSideboardPolicyV1,
    policy_fn: &mut dyn FnMut(&crate::rl_session::RlSessionDecisionV1) -> (u32, String),
) -> Result<bool, SearchCampaignErrorV1> {
    load_and_verify_manifest_v1(manifest_path)?;
    let mut deltas: Vec<i8> = Vec::with_capacity(manifest.m_bo3_matches_per_ratification as usize);
    for match_index in 0..manifest.m_bo3_matches_per_ratification {
        let seed = candidate_seed_v1(manifest.master_seed, self_deck_id, opponent_deck_id, 2, match_index);
        let candidate_won = play_bo3_match_for_seat_v1(
            self_deck_id, opponent_deck_id, candidate_mainboard, opponent_mainboard, sideboard_policy, seed, policy_fn,
        )?;
        let incumbent_won = play_bo3_match_for_seat_v1(
            self_deck_id, opponent_deck_id, incumbent_mainboard, opponent_mainboard, sideboard_policy, seed, policy_fn,
        )?;
        deltas.push(if candidate_won { 1i8 } else { 0i8 } - if incumbent_won { 1i8 } else { 0i8 });
    }
    let result = crate::paired_bo1_harness_v1::paired_bootstrap_ci_v1(
        &deltas, manifest.bootstrap_resample_count, manifest.bootstrap_seed, manifest.bo3_bootstrap_sidedness,
    );
    Ok(result.lower > 0.0 || result.upper < 0.0)
}
```
Tests (appended to `mod tests`), per this plan's rule that every function needs a real test asserting the accept/reject boundary on synthetic data, not only an expensive end-to-end game run:
```rust
    #[test]
    fn bo1_accept_boundary_on_synthetic_deltas() {
        let clearly_positive = [1i8; 20];
        let accept = crate::paired_bo1_harness_v1::paired_bootstrap_ci_v1(
            &clearly_positive, 2000, 1, BootstrapSidednessV1::OneSidedLower,
        );
        assert!(accept.lower > 0.0, "a uniformly positive delta series must clear the BO1 accept boundary");

        let clearly_mixed = [1i8, -1, 1, -1, 1, -1, 1, -1, 1, -1];
        let reject = crate::paired_bo1_harness_v1::paired_bootstrap_ci_v1(
            &clearly_mixed, 2000, 1, BootstrapSidednessV1::OneSidedLower,
        );
        assert!(reject.lower <= 0.0, "a zero-mean delta series must not clear the BO1 accept boundary");
    }

    #[test]
    fn bo3_ratify_boundary_on_synthetic_deltas() {
        let clearly_positive = [1i8; 20];
        let ratify = crate::paired_bo1_harness_v1::paired_bootstrap_ci_v1(
            &clearly_positive, 2000, 1, BootstrapSidednessV1::TwoSided,
        );
        assert!(ratify.lower > 0.0 || ratify.upper < 0.0, "a uniformly positive series must exclude 0 at the BO3 two-sided CI");

        let clearly_mixed = [1i8, -1, 1, -1, 1, -1, 1, -1, 1, -1];
        let reject = crate::paired_bo1_harness_v1::paired_bootstrap_ci_v1(
            &clearly_mixed, 2000, 1, BootstrapSidednessV1::TwoSided,
        );
        assert!(!(reject.lower > 0.0 || reject.upper < 0.0), "a zero-mean series must not exclude 0 at the BO3 two-sided CI");
    }

    #[test]
    fn run_bo1_provisional_search_completes_end_to_end_against_the_committed_template_manifest() {
        let manifest = sample_manifest_with_n_per_cell_v1(3);
        let dir = std::env::temp_dir().join(format!("bo1_driver_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let manifest_path = dir.join("manifest.json");
        std::fs::write(&manifest_path, manifest_canonical_bytes_v1(&manifest)).unwrap();
        std::fs::write(manifest_path.with_extension("json.sha256"), manifest_sha256_v1(&manifest)).unwrap();

        let registered = checked_in_pauper_registered_deck_by_id_v1("Burn").unwrap();
        let incumbent = SideboardPlanV1::keep_registered_v1("Burn", "Rally", 2).unwrap();
        let candidates = generate_one_swap_candidates_v1(&registered, "Rally", 2, 1);
        let opponent_mainboard = checked_in_pauper_registered_deck_by_id_v1("Rally")
            .unwrap()
            .registered_configuration()
            .mainboard()
            .to_vec();
        let mut rng = SplitMix64::seed(0x4141_4141_4141_4141);
        let mut policy = |decision: &crate::rl_session::FastActorDecisionV1| {
            (rng.next_u64() as u32) % decision.legal_action_count.max(1)
        };
        let result = run_bo1_provisional_search_for_cell_v1(
            &manifest, &manifest_path, &registered, &incumbent, &candidates, &opponent_mainboard, &mut policy,
        )
        .expect("the driver runs end to end against a real, small candidate set");
        if let Some(accepted) = result {
            assert_eq!(accepted.self_deck_id(), "Burn");
        }
    }
```
`sample_manifest_with_n_per_cell_v1(n_per_cell)` is `sample_manifest()` (Step 2) with `n_per_cell` overridden, added as a small test helper next to `sample_manifest()`. Run: `cmd /c start /belownormal /affinity FF /wait cargo test --locked -p mtg-kernel --lib sideboard_search_campaign_v1 2>&1 | tail -80`. Expected: all tests PASS.

- [ ] **Step 14: Run and fix.**
```
cmd /c start /belownormal /affinity FF /wait cargo test --locked -p mtg-kernel --lib sideboard_search_campaign_v1 2>&1 | tail -80
```
If `bo3_ratification_uses_the_bo3_assigned_starting_player_for_every_game` fails because `checked_in_pauper_v1("Burn", "Rally", ...)` errors with `SideboardErrorV1::UnknownPolicyDeckId` or a missing plan (the checked-in `pauper_sideboard_policy_v1.json` may have zero plans at this point, per the parent design section 2's "zero plans and a keep-registered default" note), that is a real, disclosed precondition, not a bug in this task: replace the checked-in match construction with `BestOfThreeDeckMatchV1::new_v1` using a locally-built `DeterministicSideboardPolicyV1` (matching the pattern already verified in `tests/bo3_session_v1.rs`'s `policy_v1()` helper) so the test is self-contained and does not depend on the campaign this plan does not execute. Rewrite the test's setup accordingly, keep every assertion, and rerun until green.

- [ ] **Step 15: Write the manifest JSON file and its Python validator's failing test.** Create `sideboard_search_campaign_manifest_v1.json` at the repo root (not under `data/`, since design section 4 calls it "a new companion manifest file... committed and hashed alongside the receipts file", separate from the frozen `data/pauper_sideboard_policy_v1.json`):
```json
{
  "schema": "kernel_sideboard_search_campaign_manifest/v1",
  "checkpoint_weights_hash": "PLACEHOLDER_FILLED_BY_W6_AT_CAMPAIGN_START",
  "checkpoint_git_head": "PLACEHOLDER_FILLED_BY_W6_AT_CAMPAIGN_START",
  "k_max_candidates_per_cell": 32,
  "max_iterations_per_cell": 64,
  "traversal_order": "ascending_card_id_one_swap_then_multi_swap_degree_two_to_four",
  "master_seed": 5850241847850241,
  "n_per_cell": 0,
  "n_derivation": {
    "minimum_win_rate_delta": 0.05,
    "target_power": 0.8,
    "calibration_mean_seconds_per_game": 0.0,
    "formula": "n = ceil((z_alpha + z_power)^2 * variance / minimum_win_rate_delta^2), variance = p(1-p) at p = 0.5"
  },
  "m_bo3_matches_per_ratification": 60,
  "warm_start_plan_inputs_sha256": "PLACEHOLDER_FILLED_BY_W6_AT_CAMPAIGN_START",
  "bo1_one_sided_alpha": 0.05,
  "bo3_confidence_level": 0.95,
  "bootstrap_resample_count": 10000,
  "bootstrap_seed": 13893676993274437887,
  "bootstrap_resampling_method": "case_resampling_with_replacement",
  "bo1_bootstrap_sidedness": "one_sided_lower",
  "bo3_bootstrap_sidedness": "two_sided"
}
```
`traversal_order` names the real traversal this task implements end to end: `generate_one_swap_candidates_v1` (Step 6) for the 1-card case, `generate_multi_swap_candidates_v1` (Step 9) for the 2-to-4-card case, both in ascending-card-id order; there is no unimplemented traversal phase this string could be mistaken for. This is a template instance, not a live campaign manifest: `n_per_cell`, `calibration_mean_seconds_per_game`, `checkpoint_weights_hash`, `checkpoint_git_head`, and `warm_start_plan_inputs_sha256` are placeholders that W6 fills from Task B's real calibration output, `derive_n_per_cell_v1`'s (Step 11) real output, and the checkpoint under test before that campaign's own manifest is hashed and frozen; Step 19 documents the exact procedure for replacing these placeholders and re-hashing. This file exists so Task E's driver and validator have a real, schema-conformant fixture to test against. Create `python/tests/test_validate_search_campaign_manifest_v1.py`:
```python
import hashlib
import json
import subprocess
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TOOL = ROOT / "python/tools/validate_search_campaign_manifest_v1.py"
MANIFEST = ROOT / "sideboard_search_campaign_manifest_v1.json"


class ValidateSearchCampaignManifest(unittest.TestCase):
    def test_write_hash_then_check_passes(self):
        result = subprocess.run(
            [sys.executable, str(TOOL), "--write-hash", str(MANIFEST)],
            cwd=ROOT, capture_output=True, text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        hash_path = MANIFEST.with_suffix(".json.sha256")
        self.assertTrue(hash_path.exists())
        raw = MANIFEST.read_bytes()
        expected = hashlib.sha256(raw).hexdigest()
        self.assertEqual(hash_path.read_text(encoding="utf-8").strip(), expected)

        check = subprocess.run(
            [sys.executable, str(TOOL), "--check", str(MANIFEST)],
            cwd=ROOT, capture_output=True, text=True,
        )
        self.assertEqual(check.returncode, 0, check.stdout + check.stderr)

    def test_check_fails_on_a_tampered_manifest(self):
        subprocess.run([sys.executable, str(TOOL), "--write-hash", str(MANIFEST)], cwd=ROOT, check=True)
        original = MANIFEST.read_text(encoding="utf-8")
        document = json.loads(original)
        document["k_max_candidates_per_cell"] += 1
        MANIFEST.write_text(json.dumps(document, indent=2), encoding="utf-8")
        try:
            check = subprocess.run(
                [sys.executable, str(TOOL), "--check", str(MANIFEST)],
                cwd=ROOT, capture_output=True, text=True,
            )
            self.assertNotEqual(check.returncode, 0)
        finally:
            MANIFEST.write_text(original, encoding="utf-8")
            subprocess.run([sys.executable, str(TOOL), "--write-hash", str(MANIFEST)], cwd=ROOT, check=True)


if __name__ == "__main__":
    unittest.main()
```
Run it: `uvx uv@0.11.29 run --no-sync python -m unittest python.tests.test_validate_search_campaign_manifest_v1 -v`. Expected: FAIL (tool missing).

- [ ] **Step 16: Implement the Python validator.** Create `python/tools/validate_search_campaign_manifest_v1.py`:
```python
#!/usr/bin/env python3
"""Writes or checks the sha256 sidecar for a sideboard search-campaign
manifest (design section 4, W5). Matches the byte-for-byte convention the
Rust loader (mtg-kernel/src/sideboard_search_campaign_v1.rs,
load_and_verify_manifest_v1) uses: sha256 over the raw manifest file bytes,
recorded in a sibling <name>.json.sha256 file as one lower-hex line.
"""
from __future__ import annotations

import argparse
import hashlib
import sys
from pathlib import Path


def sha256_hex(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--write-hash", metavar="MANIFEST_JSON")
    group.add_argument("--check", metavar="MANIFEST_JSON")
    args = parser.parse_args(argv)

    manifest_path = Path(args.write_hash or args.check)
    if not manifest_path.exists():
        print(f"{manifest_path} does not exist", file=sys.stderr)
        return 1
    hash_path = manifest_path.with_suffix(".json.sha256")
    digest = sha256_hex(manifest_path)

    if args.write_hash:
        hash_path.write_text(digest + "\n", encoding="utf-8")
        print(f"wrote {hash_path}: {digest}")
        return 0

    if not hash_path.exists():
        print(f"{hash_path} does not exist; run --write-hash first", file=sys.stderr)
        return 1
    recorded = hash_path.read_text(encoding="utf-8").strip()
    if recorded != digest:
        print(f"hash mismatch: recorded {recorded}, recomputed {digest}", file=sys.stderr)
        return 1
    print(f"{manifest_path} matches its recorded hash: {digest}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
```

- [ ] **Step 17: Run the Python test.** `uvx uv@0.11.29 run --no-sync python -m unittest python.tests.test_validate_search_campaign_manifest_v1 -v`. Expected: PASS.

- [ ] **Step 18: Cross-check the Python and Rust hash conventions agree** (both must hash the identical raw bytes for the same manifest content, since one campaign's manifest is written once and read by both toolchains; Step 2 above already implements `load_and_verify_manifest_v1` hashing the raw file bytes it reads, matching the Python tool's `sha256_hex`, rather than a freshly re-serialized copy of the parsed struct, so this step is a real cross-check, not a fix):
```
uvx uv@0.11.29 run --no-sync python python/tools/validate_search_campaign_manifest_v1.py --write-hash sideboard_search_campaign_manifest_v1.json
cat sideboard_search_campaign_manifest_v1.json.sha256
```
Add one more Rust test to `sideboard_search_campaign_v1.rs`'s `mod tests` that reads the same committed file and its sidecar and asserts `load_and_verify_manifest_v1` accepts it:
```rust
    #[test]
    fn the_committed_template_manifest_loads_against_its_committed_sha256_sidecar() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../sideboard_search_campaign_manifest_v1.json");
        let loaded = load_and_verify_manifest_v1(&path)
            .expect("the committed template manifest must match its committed sidecar hash");
        assert_eq!(loaded.schema, SEARCH_CAMPAIGN_MANIFEST_SCHEMA_V1);
    }
```
Run: `cmd /c start /belownormal /affinity FF /wait cargo test --locked -p mtg-kernel --lib sideboard_search_campaign_v1 2>&1 | tail -60`. Expected: PASS. If the path in `CARGO_MANIFEST_DIR` join is wrong for this crate's actual layout (confirm with `grep -n "name = " mtg-kernel/Cargo.toml | head -1` and `pwd` from the crate root), adjust the relative path so the file is found; do not hardcode an absolute Windows path.

- [ ] **Step 19: Document the exact re-hash procedure W6 must run.** This task commits `sideboard_search_campaign_manifest_v1.json` (Step 15) with four literal `PLACEHOLDER_FILLED_BY_W6_AT_CAMPAIGN_START` strings (`checkpoint_weights_hash`, `checkpoint_git_head`, `warm_start_plan_inputs_sha256`) and `n_per_cell: 0`/`calibration_mean_seconds_per_game: 0.0`, hash-locked against its own sidecar so Step 18's Rust test and Step 17's Python test both pass today. That is deliberate: this file is a schema-conformant fixture for this task's own tests, not the manifest a real campaign scores candidates against (Global Constraint: "no candidate is ever scored against an unhashed or stale search-campaign manifest"). W6, before it scores a single candidate, must:
  1. Copy `sideboard_search_campaign_manifest_v1.json` to the real campaign's own manifest path (never overwrite the committed template in place; the template stays a fixture for this task's tests forever).
  2. Replace `checkpoint_weights_hash` and `checkpoint_git_head` with the real values for the checkpoint under test, `warm_start_plan_inputs_sha256` with the output of `sha256sum docs/research/sideboard_plan_inputs_2026-09/plan_table_draft_nine_v1.json`, `calibration_mean_seconds_per_game` with Task B's real calibration output (`docs/research/paired_bo1_calibration_2026-09-09.json`), and `n_per_cell` with `derive_n_per_cell_v1(n_derivation.minimum_win_rate_delta, n_derivation.target_power, bo1_one_sided_alpha)`'s real output for those substituted values.
  3. Re-hash the new file: `uvx uv@0.11.29 run --no-sync python python/tools/validate_search_campaign_manifest_v1.py --write-hash <new manifest path>`, producing `<new manifest path>.sha256` alongside it.
  4. Commit both files together, off the launch branch, with the new sha256 in the commit body (Global Constraints: hash-bearing commits record old and new values).
  Only after all four steps does `load_and_verify_manifest_v1`/`run_bo1_provisional_search_for_cell_v1` accept the real manifest; scoring against the committed template itself (still carrying the placeholders) is refused by construction, since no real checkpoint or warm-start file has a weights hash or sha256 literally equal to the string `PLACEHOLDER_FILLED_BY_W6_AT_CAMPAIGN_START`.

- [ ] **Step 20: Register the module, run the full crate suite, and verify the runtime-decks hash guard once more.**
```
sed -i '/^pub mod game_summary_v1;/a pub mod sideboard_search_campaign_v1;' mtg-kernel/src/lib.rs
cmd /c start /belownormal /affinity FF /wait cargo test --locked -p mtg-kernel 2>&1 | tail -40
sha256sum data/runtime_decks_v1.json
```
Expected: full suite PASS; the runtime-decks hash is still unchanged from the value recorded at the start of Task A (this task, like A through D, never edits `data/runtime_decks_v1.json`, matching this plan's Global Constraints).

- [ ] **Step 21: Commit and push**, with the manifest's committed sha256 in the body:
```
git add mtg-kernel/src/sideboard_search_campaign_v1.rs mtg-kernel/src/sideboard.rs mtg-kernel/src/lib.rs sideboard_search_campaign_manifest_v1.json sideboard_search_campaign_manifest_v1.json.sha256 docs/research/sideboard_plan_inputs_2026-09/plan_table_draft_nine_v1.json python/tools/validate_search_campaign_manifest_v1.py python/tests/test_validate_search_campaign_manifest_v1.py
git commit -m "harness: candidate generator (1-to-4 card swaps, warm start, hill-climb), two-stage BO1/BO3 acceptance driver, hashed search-campaign manifest (W5)

sideboard_search_campaign_manifest_v1.json.sha256 <value from Step 18>"
git push
```

---

## Sequencing note

Tasks A through E are ordered by dependency, matching the design's own W1→W2→W3/W4→W5 ordering (section 8): Task A is a hard prerequisite for B, D, and E (all three drive sessions through Task A's explicit-deck constructors); Task C (the tag file) has no code dependency on any other task and can run in parallel with A/B once someone is free; Task D depends on A for its episode driver and independently on C for the `RemovalCounterspellTagsV1` type it accepts as a parameter (though Task D's own tests pass an empty tag set, so C is not a hard build dependency, only a data dependency for a real campaign); Task E depends on A and B (the paired trial runner) and on the real Task B calibration number for its manifest's `n_derivation.calibration_mean_seconds_per_game` field once a live campaign manifest is produced. None of these five tasks executes a search campaign, promotes anything into `data/runtime_decks_v1.json`, or writes into `data/pauper_sideboard_policy_v1.json`; that is W6 and W7, out of scope here.
