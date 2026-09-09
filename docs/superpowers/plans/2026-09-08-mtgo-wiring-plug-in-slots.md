# MTGO Wiring Plug-in Slots Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire every MTGO decision stage end to end behind stable plug-in slots, with deterministic placeholder implementations for the pregame controller, the sideboard controller, the unknown-card policy, the search-root provider, and the duel scorer, so a trained head, an expanded catalog, or the ratified search root drops in later without rewiring.

**Architecture:** A new `deployment_slots` module in the `mtgo-dxgi-capture-v1` crate defines one registry generic over the five slot traits (four already exist in the adapter; the search-root provider is new) and emits a JSON slot report naming which slot is a placeholder. Each placeholder is its own module with its own tests and plugs into the adapter's existing checked-untrusted scoring entry points unchanged. The readiness report gains explicit placeholder fields; no existing "trained head present" or "model-owned path present" flag changes meaning. Nothing here grants live authority.

**Tech Stack:** Rust 1.94 (stable), serde, serde_json, sha2; crates `mtgo-dxgi-capture-v1` (path `integrations/mtgo_dxgi_capture_v1`) and `mtgo-blackbox-v1`; the kernel card registry `mtg_kernel::card_def::CARD_DEFS`.

**Spec:** `docs/design/MTGO_LEAGUE_INTEGRATION_RESUMPTION_V1.md` (revision 5), sections 6.2 to 6.9 and Jack's 2026-09-08 direction: "focus on getting the wiring for everything present even if it involves placeholders for now for future cards and sideboarding so once the model is trained for such it's plug and play".

**Execution record (2026-09-09):** executed by subagent-driven development on `lead/mtgo-integration-v1`, commits 4d4900d6..3d529867 (a preliminary compatibility task 0b was ruled in first because the integration crates did not compile against the merged kernel); every task review and the final whole-branch review closed clean after fix rounds; the ledger with every ruling is archived at `C:/Users/Jack/mtg-kernel-gae-lane/mtgo-wiring-slots-sdd-2026-09-08/progress.md`.

## Global Constraints

- Branch: `lead/mtgo-integration-v1` (deployment branch). Never commit to the main tree; never touch `mtg-kernel/src/flat_policy_v2.rs` in this plan.
- No placeholder may set `qualified_for_live` to true, and every report this plan emits must carry `grants_live_authority: false`.
- Unknown visible card names fail closed (spec 6.9): no default action, a fixed abstention reason, human takeover.
- The placeholder pregame rule table is provisional (spec 6.5) and is named as such in its implementation id; Jack's rule-table ruling replaces it.
- No em-dashes anywhere in code comments, docs, or commit messages.
- Run every cargo command from `integrations/mtgo_dxgi_capture_v1` (its own lockfile and target dir), on E-cores at low priority when the Codex campaign is running: prefix with `cmd //c "start /low /affinity FF0000 /b /wait ..."` in Git Bash.
- Commit after each task; push after each commit (`git push origin lead/mtgo-integration-v1`).

---

### File structure

- Create `integrations/mtgo_dxgi_capture_v1/src/deployment_slots.rs`: slot kinds, descriptors, the generic registry, the JSON slot report, and the placeholder builder (Task 1, extended in Task 6).
- Create `integrations/mtgo_dxgi_capture_v1/src/unknown_card_policy.rs`: the fail-closed unknown-card policy and kernel-backed card facts (Task 2).
- Create `integrations/mtgo_dxgi_capture_v1/src/placeholder_pregame_controller.rs`: the provisional mulligan and London-bottoming rule table (Task 3).
- Create `integrations/mtgo_dxgi_capture_v1/src/placeholder_sideboard_controller.rs`: unchanged-sideboard controller for both sideboard traits (Task 4).
- Create `integrations/mtgo_dxgi_capture_v1/src/search_root_provider.rs`: the search-root provider trait and its raw-policy-only placeholder (Task 5).
- Create `integrations/mtgo_dxgi_capture_v1/src/placeholder_visible_duel_scorer.rs`: the duel-scorer slot placeholder that resolves visible names through the unknown-card policy and otherwise fails closed (Task 6).
- Modify `integrations/mtgo_dxgi_capture_v1/src/lib.rs`: module declarations and re-exports (each task).
- Modify `integrations/mtgo_dxgi_capture_v1/src/competitive_model_decision_readiness.rs`: five new placeholder readiness fields (Task 7).
- Create `integrations/mtgo_dxgi_capture_v1/src/bin/check_mtgo_deployment_slots_v1.rs`: prints the slot report (Task 7).
- Create `integrations/ci_commands_v1.ps1`: the CI command list for both crates and both policy suites (Task 8).
- Create `integrations/mtgo_dxgi_capture_v1/tests/deployment_slots_wiring_v1.rs`: end-to-end wiring test through the existing checked-untrusted entry points (Task 9).

### Existing interfaces every task builds on (exact, from the current tree)

```rust
// mtgo_dxgi_capture_v1::competitive_auxiliary_model_scoring
pub const MTGO_COMPETITIVE_AUXILIARY_MODEL_SCORING_SCHEMA_V1: u32 = 1;
pub struct MtgoCompetitiveNativePregameScoreResponseV1 {
    pub schema_version: u32,
    pub model_input_commitment_sha256: String,
    pub deployment_commitment_sha256: String,
    pub ordered_action_logits_f32_bits: Vec<u32>,
    pub value_f32_bits: u32,
}
pub trait MtgoCompetitiveNativePregameScorerV1 {
    fn score_pregame_v1(&mut self, model_input: &MtgoCompetitiveNativePregameModelInputV1,
        model_input_commitment_sha256: &str, deployment_commitment_sha256: &str)
        -> Result<MtgoCompetitiveNativePregameScoreResponseV1, String>;
}
pub struct MtgoCompetitiveNativeSideboardScoreResponseV1 {
    pub schema_version: u32,
    pub model_input_commitment_sha256: String,
    pub deployment_commitment_sha256: String,
    pub selection: MtgoCompetitiveNativeSideboardModelSelectionV1,
    pub value_f32_bits: u32,
}
pub trait MtgoCompetitiveNativeSideboardScorerV1 {
    fn score_sideboard_v1(&mut self, model_input: &MtgoCompetitiveNativeSideboardModelInputV1,
        model_input_commitment_sha256: &str, deployment_commitment_sha256: &str)
        -> Result<MtgoCompetitiveNativeSideboardScoreResponseV1, String>;
}
pub fn score_checked_untrusted_competitive_native_pregame_v1<S: MtgoCompetitiveNativePregameScorerV1>(
    model_input: &MtgoCompetitiveNativePregameModelInputV1, deployment_commitment_sha256: &str, scorer: &mut S)
    -> Result<CheckedUntrustedMtgoCompetitiveNativePregameModelSelectionV1, String>;
pub fn score_checked_untrusted_competitive_native_sideboard_v1<S: MtgoCompetitiveNativeSideboardScorerV1>(
    model_input: &MtgoCompetitiveNativeSideboardModelInputV1, deployment_commitment_sha256: &str, scorer: &mut S)
    -> Result<CheckedUntrustedMtgoCompetitiveNativeSideboardModelSelectionV1, String>;

// mtgo_dxgi_capture_v1 (actuator.rs)
pub struct MtgoCompetitiveNativePregameCardV1 { pub card_slot: u8, pub visible_card_name: String, pub selected_for_bottom: bool }
pub enum MtgoCompetitiveNativePregameActionV1 { KeepOpeningHand, Mulligan { next_hand_size: u8 }, SelectForBottom { card_slot: u8 }, SubmitBottoming }
pub enum MtgoCompetitivePregameStageV1 { MulliganChoice { prospective_keep_size: u8 }, LondonBottoming { required_bottom_count: u8, selected_bottom_count: u8 }, GameplayReady }
pub struct MtgoCompetitiveNativePregameModelInputV1 {
    pub game_number: u8, pub play_draw: mtgo_blackbox_v1::MtgoCompetitivePregamePlayDrawV1,
    pub acting_player_games_won: u8, pub opponent_games_won: u8,
    pub player_known_deck_configuration: MtgoCompetitiveNativeSideboardConfigurationV1,
    pub stage: MtgoCompetitivePregameStageV1, pub prospective_keep_size: Option<u8>,
    pub required_bottom_count: u8, pub selected_bottom_count: u8,
    pub ordered_visible_cards: Vec<MtgoCompetitiveNativePregameCardV1>,
    pub ordered_confirmed_bottom_slots: Vec<u8>,
    pub ordered_actions: Vec<MtgoCompetitiveNativePregameActionV1>,
}

// mtgo_dxgi_capture_v1::competitive_native_sideboard
pub struct MtgoCompetitiveNativeSideboardCardCountV1 { pub visible_card_name: String, pub count: u16 }
pub struct MtgoCompetitiveNativeSideboardConfigurationV1 { pub mainboard: Vec<MtgoCompetitiveNativeSideboardCardCountV1>, pub sideboard: Vec<MtgoCompetitiveNativeSideboardCardCountV1> }
pub struct MtgoCompetitiveNativeSideboardModelInputV1 { pub next_game_number: u8, pub acting_player_games_won: u8, pub opponent_games_won: u8, pub current_configuration: MtgoCompetitiveNativeSideboardConfigurationV1 }
pub struct MtgoCompetitiveNativeSideboardModelSelectionV1 { pub target_configuration: MtgoCompetitiveNativeSideboardConfigurationV1 }

// mtgo_dxgi_capture_v1::competitive_native_sideboard_deliberation
pub const MTGO_COMPETITIVE_NATIVE_SIDEBOARD_DELIBERATION_SCHEMA_V1: u32 = 1;
pub enum MtgoCompetitiveNativeSideboardDeliberationActionV1 { MoveOneToMainboard { visible_card_name: String }, MoveOneToSideboard { visible_card_name: String }, SubmitConfiguration }
pub struct MtgoCompetitiveNativeSideboardDeliberationDecisionV1 { pub schema_version: u32, pub decision_number: u8, pub model_input_commitment_sha256: String, pub deployment_commitment_sha256: String, pub prior_trace_commitment_sha256: String, pub candidate_configuration: MtgoCompetitiveNativeSideboardConfigurationV1, pub ordered_actions: Vec<MtgoCompetitiveNativeSideboardDeliberationActionV1>, pub decision_commitment_sha256: String }
pub struct MtgoCompetitiveNativeSideboardDeliberationScoreResponseV1 { pub schema_version: u32, pub decision_commitment_sha256: String, pub deployment_commitment_sha256: String, pub ordered_logits_f32_bits: Vec<u32>, pub value_f32_bits: u32 }
pub trait MtgoCompetitiveNativeSideboardDeliberationScorerV1 { fn score_sideboard_deliberation_v1(&mut self, decision: &MtgoCompetitiveNativeSideboardDeliberationDecisionV1) -> Result<MtgoCompetitiveNativeSideboardDeliberationScoreResponseV1, String>; }

// mtgo_blackbox_v1
pub struct MtgoPlayerVisibleDuelScoreResponseV1 { pub schema_version: u32, pub ordered_action_logits_f32_bits: Vec<u32>, pub value_f32_bits: u32 }
pub trait MtgoPlayerVisibleDuelScorerV1 { fn score_player_visible_duel_v1(&mut self, model_input: &MtgoPlayerVisibleDuelDecisionInputV1) -> Result<MtgoPlayerVisibleDuelScoreResponseV1, String>; }
pub struct MtgoPlayerVisibleDuelDecisionInputV1 { pub current_state: MtgoPlayerVisibleDuelStateV1, pub ordered_legal_actions: Vec<MtgoPlayerVisibleDuelActionV1> }
// MtgoPlayerVisibleDuelStateV1 fields used here: own_hand: Vec<MtgoPlayerVisibleNamedCardV1>, battlefield: [Vec<MtgoPlayerVisibleBattlefieldCardV1>; 2] (card_name), graveyards: [Vec<MtgoPlayerVisibleNamedCardV1>; 2] (card_name), exile: Vec<MtgoPlayerVisibleExileCardV1> (visible_card_name: Option<String>), stack: Vec<MtgoPlayerVisibleStackItemV1> (visible_source_name: Option<String>), known_library_cards: [Vec<MtgoPlayerVisibleKnownLibraryCardV1>; 2] (card.card_name), known_hand_cards: [Vec<MtgoPlayerVisibleNamedCardV1>; 2] (card_name)
pub fn resolve_checked_untrusted_kernel_card_correspondence_v1(visible_card_name: &str) -> Result<CheckedUntrustedMtgoKernelCardCorrespondenceV1, MtgoContractErrorV1>; // .card_db_id() -> u16

// mtg_kernel::card_def
pub struct CardDef { pub name: &'static str, pub cost: mtg_kernel::mana::Cost, pub is_land: bool, /* ... */ }
pub static CARD_DEFS: [CardDef; N]; // index = card_db_id
// mtg_kernel::mana::Cost { pub pips: &'static [Pip], pub generic: u8, pub x_count: u8 }
```

---

### Task 1: Slot registry and slot report

**Files:**
- Create: `integrations/mtgo_dxgi_capture_v1/src/deployment_slots.rs`
- Modify: `integrations/mtgo_dxgi_capture_v1/src/lib.rs` (add `mod deployment_slots;` after the `mod competitive_visible_match_memory;` line, and a `pub use deployment_slots::{...}` block after the `pub use competitive_visible_match_memory::{...}` block)

**Interfaces:**
- Produces: `MtgoDeploymentSlotKindV1`, `MtgoDeploymentSlotDescriptorV1`, `MtgoDeploymentSlotV1` (trait), `MtgoDeploymentSlotReportV1`, `MtgoDeploymentSlotsV1<D, P, S, U, R>`, `MTGO_DEPLOYMENT_SLOT_REPORT_SCHEMA_V1`. Task 5 supplies `MtgoSearchRootProviderV1`; until Task 5 lands the `R` bound is `MtgoDeploymentSlotV1` only (Task 5 tightens it).

- [ ] **Step 1: Write the failing test**

Create `integrations/mtgo_dxgi_capture_v1/src/deployment_slots.rs` with only the test module:

```rust
//! Deployment plug-in slots for the MTGO League operator.
//!
//! One registry names every decision surface the deployed agent needs and
//! reports, per slot, which implementation is bound and whether it is a
//! placeholder. Placeholders are deterministic stand-ins that keep the wiring
//! complete; they never grant live authority. A trained head, an expanded
//! catalog, or the ratified search root replaces a placeholder by implementing
//! the same slot trait.

#[cfg(test)]
mod tests {
    use super::*;

    struct Stub(&'static str, bool);

    impl MtgoDeploymentSlotV1 for Stub {
        fn slot_descriptor_v1(&self, kind: MtgoDeploymentSlotKindV1) -> MtgoDeploymentSlotDescriptorV1 {
            MtgoDeploymentSlotDescriptorV1 {
                kind,
                implementation_id: self.0.to_owned(),
                is_placeholder: self.1,
                qualified_for_live: false,
                contract_version: 1,
            }
        }
    }

    #[test]
    fn slot_report_names_every_slot_once_and_never_grants_live_authority() {
        let slots = MtgoDeploymentSlotsV1 {
            duel_scorer: Stub("duel", true),
            pregame_controller: Stub("pregame", true),
            sideboard_controller: Stub("sideboard", false),
            unknown_card_policy: Stub("unknown", true),
            search_root_provider: Stub("search", true),
        };
        let report = slots.slot_report_v1();
        assert_eq!(report.schema_version, MTGO_DEPLOYMENT_SLOT_REPORT_SCHEMA_V1);
        assert_eq!(report.slots.len(), 5);
        let kinds: Vec<MtgoDeploymentSlotKindV1> = report.slots.iter().map(|slot| slot.kind).collect();
        assert_eq!(
            kinds,
            vec![
                MtgoDeploymentSlotKindV1::DuelScorer,
                MtgoDeploymentSlotKindV1::PregameController,
                MtgoDeploymentSlotKindV1::SideboardController,
                MtgoDeploymentSlotKindV1::UnknownCardPolicy,
                MtgoDeploymentSlotKindV1::SearchRootProvider,
            ]
        );
        assert!(report.all_slots_wired);
        assert!(report.any_placeholder_active);
        assert_eq!(report.placeholder_slot_count, 4);
        assert!(!report.grants_live_authority);
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"grants_live_authority\":false"));
        assert!(json.contains("\"kind\":\"pregame_controller\""));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run (from `integrations/mtgo_dxgi_capture_v1`): `cargo test --lib deployment_slots`
Expected: compile error, `MtgoDeploymentSlotV1` and friends not found.

- [ ] **Step 3: Write minimal implementation**

Insert above the test module in `deployment_slots.rs`:

```rust
use serde::{Deserialize, Serialize};

pub const MTGO_DEPLOYMENT_SLOT_REPORT_SCHEMA_V1: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoDeploymentSlotKindV1 {
    DuelScorer,
    PregameController,
    SideboardController,
    UnknownCardPolicy,
    SearchRootProvider,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoDeploymentSlotDescriptorV1 {
    pub kind: MtgoDeploymentSlotKindV1,
    pub implementation_id: String,
    pub is_placeholder: bool,
    pub qualified_for_live: bool,
    pub contract_version: u32,
}

/// Every slot implementation describes itself. The registry passes the slot
/// kind so one type can serve more than one slot without lying about which
/// slot it fills.
pub trait MtgoDeploymentSlotV1 {
    fn slot_descriptor_v1(&self, kind: MtgoDeploymentSlotKindV1) -> MtgoDeploymentSlotDescriptorV1;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoDeploymentSlotReportV1 {
    pub schema_version: u32,
    pub purpose: String,
    pub slots: Vec<MtgoDeploymentSlotDescriptorV1>,
    pub all_slots_wired: bool,
    pub any_placeholder_active: bool,
    pub placeholder_slot_count: u32,
    pub grants_live_authority: bool,
}

pub struct MtgoDeploymentSlotsV1<D, P, S, U, R> {
    pub duel_scorer: D,
    pub pregame_controller: P,
    pub sideboard_controller: S,
    pub unknown_card_policy: U,
    pub search_root_provider: R,
}

impl<D, P, S, U, R> MtgoDeploymentSlotsV1<D, P, S, U, R>
where
    D: MtgoDeploymentSlotV1,
    P: MtgoDeploymentSlotV1,
    S: MtgoDeploymentSlotV1,
    U: MtgoDeploymentSlotV1,
    R: MtgoDeploymentSlotV1,
{
    pub fn slot_report_v1(&self) -> MtgoDeploymentSlotReportV1 {
        let slots = vec![
            self.duel_scorer.slot_descriptor_v1(MtgoDeploymentSlotKindV1::DuelScorer),
            self.pregame_controller
                .slot_descriptor_v1(MtgoDeploymentSlotKindV1::PregameController),
            self.sideboard_controller
                .slot_descriptor_v1(MtgoDeploymentSlotKindV1::SideboardController),
            self.unknown_card_policy
                .slot_descriptor_v1(MtgoDeploymentSlotKindV1::UnknownCardPolicy),
            self.search_root_provider
                .slot_descriptor_v1(MtgoDeploymentSlotKindV1::SearchRootProvider),
        ];
        let expected_kinds = [
            MtgoDeploymentSlotKindV1::DuelScorer,
            MtgoDeploymentSlotKindV1::PregameController,
            MtgoDeploymentSlotKindV1::SideboardController,
            MtgoDeploymentSlotKindV1::UnknownCardPolicy,
            MtgoDeploymentSlotKindV1::SearchRootProvider,
        ];
        let all_slots_wired = slots
            .iter()
            .zip(expected_kinds.iter())
            .all(|(slot, kind)| slot.kind == *kind && !slot.implementation_id.trim().is_empty());
        let placeholder_slot_count =
            u32::try_from(slots.iter().filter(|slot| slot.is_placeholder).count())
                .unwrap_or(u32::MAX);
        MtgoDeploymentSlotReportV1 {
            schema_version: MTGO_DEPLOYMENT_SLOT_REPORT_SCHEMA_V1,
            purpose: "non_actuating_deployment_slot_report_v1".to_owned(),
            slots,
            all_slots_wired,
            any_placeholder_active: placeholder_slot_count > 0,
            placeholder_slot_count,
            grants_live_authority: false,
        }
    }
}
```

Then in `lib.rs`, after `mod competitive_visible_match_memory;` add:

```rust
mod deployment_slots;
```

and after the `pub use competitive_visible_match_memory::{ ... };` block add:

```rust
pub use deployment_slots::{
    MtgoDeploymentSlotDescriptorV1, MtgoDeploymentSlotKindV1, MtgoDeploymentSlotReportV1,
    MtgoDeploymentSlotV1, MtgoDeploymentSlotsV1, MTGO_DEPLOYMENT_SLOT_REPORT_SCHEMA_V1,
};
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib deployment_slots`
Expected: `test result: ok. 1 passed`

- [ ] **Step 5: Commit**

```bash
git add integrations/mtgo_dxgi_capture_v1/src/deployment_slots.rs integrations/mtgo_dxgi_capture_v1/src/lib.rs
git commit -m "mtgo: add the deployment slot registry and slot report"
git push origin lead/mtgo-integration-v1
```

---

### Task 2: Fail-closed unknown-card policy with kernel card facts

**Files:**
- Create: `integrations/mtgo_dxgi_capture_v1/src/unknown_card_policy.rs`
- Modify: `integrations/mtgo_dxgi_capture_v1/src/lib.rs` (add `mod unknown_card_policy;` after `mod deployment_slots;` and a `pub use unknown_card_policy::{...}` block after the deployment_slots re-export)

**Interfaces:**
- Consumes: `MtgoDeploymentSlotV1`, `MtgoDeploymentSlotKindV1`, `MtgoDeploymentSlotDescriptorV1` (Task 1); `mtgo_blackbox_v1::resolve_checked_untrusted_kernel_card_correspondence_v1`; `mtg_kernel::card_def::CARD_DEFS`.
- Produces: `MtgoUnknownCardPolicyV1::FailClosedHumanTakeover`, `MtgoResolvedVisibleCardV1 { visible_card_name, card_db_id, is_land, mana_value }`, `MtgoUnknownCardAbstentionV1 { reason, unknown_visible_card_names }`, `MtgoUnknownCardPolicyV1::resolve_visible_card_names_v1(&self, &[&str]) -> Result<Vec<MtgoResolvedVisibleCardV1>, MtgoUnknownCardAbstentionV1>`, `MTGO_UNKNOWN_CARD_FAIL_CLOSED_REASON_V1`.

- [ ] **Step 1: Write the failing test**

Create `unknown_card_policy.rs` with the test module:

```rust
//! Unknown-card policy (spec 6.9). Version 1 fails closed: when a required
//! visible card cannot be resolved against the frozen kernel catalog, the
//! agent submits no default action and requests human takeover. A future
//! qualified unknown-card encoding replaces this policy behind the same slot.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_pool_cards_resolve_with_land_and_mana_value_facts() {
        let policy = MtgoUnknownCardPolicyV1::FailClosedHumanTakeover;
        let resolved = policy
            .resolve_visible_card_names_v1(&["Mountain", "Lightning Bolt"])
            .unwrap();
        assert_eq!(resolved.len(), 2);
        let mountain = &resolved[0];
        assert_eq!(mountain.visible_card_name, "Mountain");
        assert!(mountain.is_land);
        assert_eq!(mountain.mana_value, 0);
        let bolt = &resolved[1];
        assert_eq!(bolt.visible_card_name, "Lightning Bolt");
        assert!(!bolt.is_land);
        assert_eq!(bolt.mana_value, 1);
    }

    #[test]
    fn unknown_names_abstain_with_the_fixed_reason_and_list_every_unknown_name() {
        let policy = MtgoUnknownCardPolicyV1::FailClosedHumanTakeover;
        let error = policy
            .resolve_visible_card_names_v1(&["Mountain", "Not A Kernel Card", "Also Unknown"])
            .unwrap_err();
        assert_eq!(error.reason, MTGO_UNKNOWN_CARD_FAIL_CLOSED_REASON_V1);
        assert_eq!(
            error.unknown_visible_card_names,
            vec!["Also Unknown".to_owned(), "Not A Kernel Card".to_owned()]
        );
        assert_eq!(error.policy, MtgoUnknownCardPolicyV1::FailClosedHumanTakeover);
    }

    #[test]
    fn policy_reports_itself_as_a_placeholder_slot() {
        let descriptor = MtgoUnknownCardPolicyV1::FailClosedHumanTakeover
            .slot_descriptor_v1(MtgoDeploymentSlotKindV1::UnknownCardPolicy);
        assert_eq!(descriptor.implementation_id, "unknown_card_policy_fail_closed_human_takeover_v1");
        assert!(descriptor.is_placeholder);
        assert!(!descriptor.qualified_for_live);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib unknown_card_policy`
Expected: compile error, `MtgoUnknownCardPolicyV1` not found.

- [ ] **Step 3: Write minimal implementation**

Insert above the tests:

```rust
use crate::{MtgoDeploymentSlotDescriptorV1, MtgoDeploymentSlotKindV1, MtgoDeploymentSlotV1};
use mtg_kernel::card_def::CARD_DEFS;
use mtgo_blackbox_v1::resolve_checked_untrusted_kernel_card_correspondence_v1;
use serde::{Deserialize, Serialize};

pub const MTGO_UNKNOWN_CARD_FAIL_CLOSED_REASON_V1: &str =
    "unknown_card_fail_closed_human_takeover_v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoUnknownCardPolicyV1 {
    FailClosedHumanTakeover,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoResolvedVisibleCardV1 {
    pub visible_card_name: String,
    pub card_db_id: u16,
    pub is_land: bool,
    pub mana_value: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoUnknownCardAbstentionV1 {
    pub policy: MtgoUnknownCardPolicyV1,
    pub reason: String,
    /// Sorted and deduplicated so the abstention is deterministic.
    pub unknown_visible_card_names: Vec<String>,
}

impl MtgoUnknownCardPolicyV1 {
    /// Resolves every name in input order. Any unresolved name fails the whole
    /// call; the policy never substitutes a default card.
    pub fn resolve_visible_card_names_v1(
        &self,
        visible_card_names: &[&str],
    ) -> Result<Vec<MtgoResolvedVisibleCardV1>, MtgoUnknownCardAbstentionV1> {
        let mut resolved = Vec::with_capacity(visible_card_names.len());
        let mut unknown = Vec::new();
        for name in visible_card_names {
            match resolve_checked_untrusted_kernel_card_correspondence_v1(name) {
                Ok(correspondence) => {
                    let card_db_id = correspondence.card_db_id();
                    let Some(definition) = CARD_DEFS.get(usize::from(card_db_id)) else {
                        unknown.push((*name).to_owned());
                        continue;
                    };
                    let mana_value = u8::try_from(definition.cost.pips.len())
                        .unwrap_or(u8::MAX)
                        .saturating_add(definition.cost.generic);
                    resolved.push(MtgoResolvedVisibleCardV1 {
                        visible_card_name: (*name).to_owned(),
                        card_db_id,
                        is_land: definition.is_land,
                        mana_value,
                    });
                }
                Err(_) => unknown.push((*name).to_owned()),
            }
        }
        if unknown.is_empty() {
            return Ok(resolved);
        }
        unknown.sort();
        unknown.dedup();
        Err(MtgoUnknownCardAbstentionV1 {
            policy: *self,
            reason: MTGO_UNKNOWN_CARD_FAIL_CLOSED_REASON_V1.to_owned(),
            unknown_visible_card_names: unknown,
        })
    }
}

impl MtgoDeploymentSlotV1 for MtgoUnknownCardPolicyV1 {
    fn slot_descriptor_v1(&self, kind: MtgoDeploymentSlotKindV1) -> MtgoDeploymentSlotDescriptorV1 {
        MtgoDeploymentSlotDescriptorV1 {
            kind,
            implementation_id: "unknown_card_policy_fail_closed_human_takeover_v1".to_owned(),
            is_placeholder: true,
            qualified_for_live: false,
            contract_version: 1,
        }
    }
}
```

In `lib.rs` add `mod unknown_card_policy;` and:

```rust
pub use unknown_card_policy::{
    MtgoResolvedVisibleCardV1, MtgoUnknownCardAbstentionV1, MtgoUnknownCardPolicyV1,
    MTGO_UNKNOWN_CARD_FAIL_CLOSED_REASON_V1,
};
```

If `resolve_checked_untrusted_kernel_card_correspondence_v1` rejects a card whose kernel capability is not `Full`, the pool cards used in the tests (Mountain, Lightning Bolt) are fully supported and resolve; keep the tests on those two names.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib unknown_card_policy`
Expected: `test result: ok. 3 passed`

- [ ] **Step 5: Commit**

```bash
git add integrations/mtgo_dxgi_capture_v1/src/unknown_card_policy.rs integrations/mtgo_dxgi_capture_v1/src/lib.rs
git commit -m "mtgo: add the fail-closed unknown-card policy with kernel card facts"
git push origin lead/mtgo-integration-v1
```

---

### Task 3: Placeholder pregame controller (provisional rule table)

**Files:**
- Create: `integrations/mtgo_dxgi_capture_v1/src/placeholder_pregame_controller.rs`
- Modify: `integrations/mtgo_dxgi_capture_v1/src/lib.rs` (add `mod placeholder_pregame_controller;` and a `pub use placeholder_pregame_controller::{...}` block)

**Interfaces:**
- Consumes: Task 1 slot types; Task 2 `MtgoUnknownCardPolicyV1`, `MtgoResolvedVisibleCardV1`; existing `MtgoCompetitiveNativePregameScorerV1`, `MtgoCompetitiveNativePregameScoreResponseV1`, `MTGO_COMPETITIVE_AUXILIARY_MODEL_SCORING_SCHEMA_V1`, `MtgoCompetitiveNativePregameModelInputV1`, `MtgoCompetitiveNativePregameActionV1`, `MtgoCompetitivePregameStageV1`, `MtgoCompetitiveNativeSideboardConfigurationV1`.
- Produces: `MtgoPlaceholderPregameControllerV1::new_v1(&MtgoCompetitiveNativeSideboardConfigurationV1, MtgoUnknownCardPolicyV1) -> Result<Self, String>`, `MtgoPlaceholderPregameControllerV1::rule_table_commitment_sha256_v1(&self) -> &str`, `MTGO_PLACEHOLDER_PREGAME_RULE_TABLE_V1`.

- [ ] **Step 1: Write the failing test**

Create the file with the test module:

```rust
//! Placeholder pregame controller (spec 6.5, provisional). Deterministic
//! mulligan and London-bottoming rules for the seated player's own hand. It
//! is a disclosed non-model component; Jack's ratified rule table replaces
//! it behind the same slot. It never grants live authority.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        score_checked_untrusted_competitive_native_pregame_v1,
        MtgoCompetitiveNativePregameCardV1, MtgoCompetitiveNativeSideboardCardCountV1,
        MtgoCompetitiveNativeSideboardConfigurationV1, MtgoCompetitivePregameStageV1,
    };
    use mtgo_blackbox_v1::MtgoCompetitivePregamePlayDrawV1;

    fn deck_v1() -> MtgoCompetitiveNativeSideboardConfigurationV1 {
        MtgoCompetitiveNativeSideboardConfigurationV1 {
            mainboard: vec![
                MtgoCompetitiveNativeSideboardCardCountV1 {
                    visible_card_name: "Lightning Bolt".to_owned(),
                    count: 4,
                },
                MtgoCompetitiveNativeSideboardCardCountV1 {
                    visible_card_name: "Mountain".to_owned(),
                    count: 56,
                },
            ],
            sideboard: vec![MtgoCompetitiveNativeSideboardCardCountV1 {
                visible_card_name: "Lightning Bolt".to_owned(),
                count: 15,
            }],
        }
    }

    fn hand(names: [&str; 7], selected: &[u8]) -> Vec<MtgoCompetitiveNativePregameCardV1> {
        names
            .iter()
            .enumerate()
            .map(|(slot, name)| MtgoCompetitiveNativePregameCardV1 {
                card_slot: slot as u8,
                visible_card_name: (*name).to_owned(),
                selected_for_bottom: selected.contains(&(slot as u8)),
            })
            .collect()
    }

    fn mulligan_input(names: [&str; 7], prospective_keep_size: u8) -> MtgoCompetitiveNativePregameModelInputV1 {
        MtgoCompetitiveNativePregameModelInputV1 {
            game_number: 1,
            play_draw: MtgoCompetitivePregamePlayDrawV1::OnPlay,
            acting_player_games_won: 0,
            opponent_games_won: 0,
            player_known_deck_configuration: deck_v1(),
            stage: MtgoCompetitivePregameStageV1::MulliganChoice { prospective_keep_size },
            prospective_keep_size: Some(prospective_keep_size),
            required_bottom_count: 0,
            selected_bottom_count: 0,
            ordered_visible_cards: hand(names, &[]),
            ordered_confirmed_bottom_slots: Vec::new(),
            ordered_actions: vec![
                MtgoCompetitiveNativePregameActionV1::KeepOpeningHand,
                MtgoCompetitiveNativePregameActionV1::Mulligan {
                    next_hand_size: prospective_keep_size - 1,
                },
            ],
        }
    }

    fn bottoming_input(names: [&str; 7], selected: &[u8], required: u8) -> MtgoCompetitiveNativePregameModelInputV1 {
        let cards = hand(names, selected);
        let mut actions: Vec<MtgoCompetitiveNativePregameActionV1> = cards
            .iter()
            .filter(|card| !card.selected_for_bottom)
            .map(|card| MtgoCompetitiveNativePregameActionV1::SelectForBottom { card_slot: card.card_slot })
            .collect();
        actions.push(MtgoCompetitiveNativePregameActionV1::SubmitBottoming);
        MtgoCompetitiveNativePregameModelInputV1 {
            game_number: 1,
            play_draw: MtgoCompetitivePregamePlayDrawV1::OnPlay,
            acting_player_games_won: 0,
            opponent_games_won: 0,
            player_known_deck_configuration: deck_v1(),
            stage: MtgoCompetitivePregameStageV1::LondonBottoming {
                required_bottom_count: required,
                selected_bottom_count: selected.len() as u8,
            },
            prospective_keep_size: Some(7 - required),
            required_bottom_count: required,
            selected_bottom_count: selected.len() as u8,
            ordered_visible_cards: cards,
            ordered_confirmed_bottom_slots: selected.to_vec(),
            ordered_actions: actions,
        }
    }

    fn controller() -> MtgoPlaceholderPregameControllerV1 {
        MtgoPlaceholderPregameControllerV1::new_v1(&deck_v1(), MtgoUnknownCardPolicyV1::FailClosedHumanTakeover).unwrap()
    }

    const M: &str = "Mountain";
    const B: &str = "Lightning Bolt";

    #[test]
    fn keeps_seven_with_two_to_five_lands() {
        let input = mulligan_input([M, M, M, B, B, B, B], 7);
        assert_eq!(controller().choose_index_v1(&input).unwrap(), 0);
    }

    #[test]
    fn mulligans_seven_with_one_land_and_with_six_lands() {
        assert_eq!(controller().choose_index_v1(&mulligan_input([M, B, B, B, B, B, B], 7)).unwrap(), 1);
        assert_eq!(controller().choose_index_v1(&mulligan_input([M, M, M, M, M, M, B], 7)).unwrap(), 1);
    }

    #[test]
    fn never_goes_below_six() {
        let input = mulligan_input([B, B, B, B, B, B, B], 6);
        assert_eq!(controller().choose_index_v1(&input).unwrap(), 0);
    }

    #[test]
    fn bottoms_the_highest_mana_value_nonland_then_submits() {
        let first = bottoming_input([M, M, M, B, B, B, B], &[], 1);
        // Non-lands tie at mana value 1, so the highest slot (6) is bottomed.
        let chosen = controller().choose_index_v1(&first).unwrap();
        assert_eq!(first.ordered_actions[chosen], MtgoCompetitiveNativePregameActionV1::SelectForBottom { card_slot: 6 });
        let second = bottoming_input([M, M, M, B, B, B, B], &[6], 1);
        let chosen = controller().choose_index_v1(&second).unwrap();
        assert_eq!(second.ordered_actions[chosen], MtgoCompetitiveNativePregameActionV1::SubmitBottoming);
    }

    #[test]
    fn bottoms_an_excess_land_when_more_than_five_lands_remain() {
        let input = bottoming_input([M, M, M, M, M, M, B], &[], 1);
        let chosen = controller().choose_index_v1(&input).unwrap();
        assert_eq!(input.ordered_actions[chosen], MtgoCompetitiveNativePregameActionV1::SelectForBottom { card_slot: 5 });
    }

    #[test]
    fn hand_card_outside_the_deck_fails_closed() {
        let input = mulligan_input([M, M, "Island", B, B, B, B], 7);
        assert!(controller().choose_index_v1(&input).unwrap_err().contains("pregame_visible_card_outside_deck"));
    }

    #[test]
    fn unknown_deck_card_fails_construction() {
        let mut deck = deck_v1();
        deck.mainboard[0].visible_card_name = "Not A Kernel Card".to_owned();
        assert!(MtgoPlaceholderPregameControllerV1::new_v1(&deck, MtgoUnknownCardPolicyV1::FailClosedHumanTakeover).is_err());
    }

    #[test]
    fn scores_through_the_checked_untrusted_pregame_path() {
        let input = mulligan_input([M, M, M, B, B, B, B], 7);
        let mut scorer = controller();
        let selection = score_checked_untrusted_competitive_native_pregame_v1(&input, &"c".repeat(64), &mut scorer).unwrap();
        assert_eq!(selection.selected_index_v1(), 0);
        assert_eq!(selection.selected_action_v1(), &MtgoCompetitiveNativePregameActionV1::KeepOpeningHand);
        assert!(!selection.safe_for_live_input_v1());
        assert!(!selection.permits_event_entry_v1());
        assert!(!selection.permits_spending_v1());
    }

    #[test]
    fn slot_descriptor_is_a_placeholder_bound_to_the_rule_table_commitment() {
        let controller = controller();
        let descriptor = controller.slot_descriptor_v1(MtgoDeploymentSlotKindV1::PregameController);
        assert!(descriptor.is_placeholder);
        assert!(!descriptor.qualified_for_live);
        assert!(descriptor.implementation_id.starts_with("placeholder_pregame_controller_rule_table_v1:"));
        assert!(descriptor.implementation_id.ends_with(controller.rule_table_commitment_sha256_v1()));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib placeholder_pregame_controller`
Expected: compile error, `MtgoPlaceholderPregameControllerV1` not found.

- [ ] **Step 3: Write minimal implementation**

Insert above the tests:

```rust
use crate::{
    MtgoCompetitiveNativePregameActionV1, MtgoCompetitiveNativePregameModelInputV1,
    MtgoCompetitiveNativePregameScoreResponseV1, MtgoCompetitiveNativePregameScorerV1,
    MtgoCompetitiveNativeSideboardConfigurationV1, MtgoCompetitivePregameStageV1,
    MtgoDeploymentSlotDescriptorV1, MtgoDeploymentSlotKindV1, MtgoDeploymentSlotV1,
    MtgoResolvedVisibleCardV1, MtgoUnknownCardPolicyV1,
    MTGO_COMPETITIVE_AUXILIARY_MODEL_SCORING_SCHEMA_V1,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// The provisional rule table, spelled out so its commitment is auditable.
pub const MTGO_PLACEHOLDER_PREGAME_RULE_TABLE_V1: &str = "keep_seven_with_two_to_five_lands;\
mulligan_once_to_six_otherwise;\
never_below_six;\
bottom_excess_land_highest_slot_when_lands_exceed_five;\
bottom_highest_mana_value_nonland_ties_to_highest_slot;\
bottom_highest_slot_when_no_nonland_remains;\
submit_when_selected_equals_required";

pub struct MtgoPlaceholderPregameControllerV1 {
    card_facts: BTreeMap<String, MtgoResolvedVisibleCardV1>,
    rule_table_commitment_sha256: String,
}

impl MtgoPlaceholderPregameControllerV1 {
    /// Resolves every deck card once at construction so a deck outside the
    /// kernel catalog fails before any decision is scored.
    pub fn new_v1(
        deck: &MtgoCompetitiveNativeSideboardConfigurationV1,
        policy: MtgoUnknownCardPolicyV1,
    ) -> Result<Self, String> {
        let names: Vec<&str> = deck
            .mainboard
            .iter()
            .chain(deck.sideboard.iter())
            .map(|card| card.visible_card_name.as_str())
            .collect();
        let resolved = policy
            .resolve_visible_card_names_v1(&names)
            .map_err(|abstention| {
                format!(
                    "{}:{}",
                    abstention.reason,
                    abstention.unknown_visible_card_names.join(",")
                )
            })?;
        let card_facts = resolved
            .into_iter()
            .map(|card| (card.visible_card_name.clone(), card))
            .collect();
        let mut hasher = Sha256::new();
        hasher.update(b"mtgo-placeholder-pregame-rule-table-v1\0");
        hasher.update(MTGO_PLACEHOLDER_PREGAME_RULE_TABLE_V1.as_bytes());
        let rule_table_commitment_sha256 = hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        Ok(Self {
            card_facts,
            rule_table_commitment_sha256,
        })
    }

    pub fn rule_table_commitment_sha256_v1(&self) -> &str {
        &self.rule_table_commitment_sha256
    }

    fn facts_v1(&self, visible_card_name: &str) -> Result<&MtgoResolvedVisibleCardV1, String> {
        self.card_facts
            .get(visible_card_name)
            .ok_or_else(|| format!("pregame_visible_card_outside_deck:{visible_card_name}"))
    }

    fn action_index_v1(
        input: &MtgoCompetitiveNativePregameModelInputV1,
        wanted: &MtgoCompetitiveNativePregameActionV1,
    ) -> Option<usize> {
        input.ordered_actions.iter().position(|action| action == wanted)
    }

    /// Chooses the index into `ordered_actions` the rule table selects.
    pub fn choose_index_v1(
        &self,
        input: &MtgoCompetitiveNativePregameModelInputV1,
    ) -> Result<usize, String> {
        let mut land_count = 0_usize;
        for card in &input.ordered_visible_cards {
            if self.facts_v1(&card.visible_card_name)?.is_land && !card.selected_for_bottom {
                land_count += 1;
            }
        }
        match input.stage {
            MtgoCompetitivePregameStageV1::MulliganChoice {
                prospective_keep_size,
            } => {
                let keep = Self::action_index_v1(input, &MtgoCompetitiveNativePregameActionV1::KeepOpeningHand)
                    .ok_or_else(|| "placeholder pregame controller requires a keep action".to_owned())?;
                if prospective_keep_size <= 6 || (2..=5).contains(&land_count) {
                    return Ok(keep);
                }
                Ok(Self::action_index_v1(
                    input,
                    &MtgoCompetitiveNativePregameActionV1::Mulligan { next_hand_size: 6 },
                )
                .unwrap_or(keep))
            }
            MtgoCompetitivePregameStageV1::LondonBottoming {
                required_bottom_count,
                selected_bottom_count,
            } => {
                if selected_bottom_count >= required_bottom_count {
                    return Self::action_index_v1(input, &MtgoCompetitiveNativePregameActionV1::SubmitBottoming)
                        .ok_or_else(|| "placeholder pregame controller requires a submit action".to_owned());
                }
                let mut candidates: Vec<(u8, usize, bool, u8)> = Vec::new();
                for (index, action) in input.ordered_actions.iter().enumerate() {
                    if let MtgoCompetitiveNativePregameActionV1::SelectForBottom { card_slot } = action {
                        let card = input
                            .ordered_visible_cards
                            .get(usize::from(*card_slot))
                            .ok_or_else(|| "placeholder pregame controller saw an unknown slot".to_owned())?;
                        let facts = self.facts_v1(&card.visible_card_name)?;
                        candidates.push((*card_slot, index, facts.is_land, facts.mana_value));
                    }
                }
                if candidates.is_empty() {
                    return Err("placeholder pregame controller has no bottoming candidate".to_owned());
                }
                let chosen = if land_count > 5 {
                    candidates.iter().filter(|candidate| candidate.2).max_by_key(|candidate| candidate.0)
                } else {
                    candidates
                        .iter()
                        .filter(|candidate| !candidate.2)
                        .max_by_key(|candidate| (candidate.3, candidate.0))
                };
                let chosen = chosen
                    .or_else(|| candidates.iter().max_by_key(|candidate| candidate.0))
                    .ok_or_else(|| "placeholder pregame controller has no bottoming candidate".to_owned())?;
                Ok(chosen.1)
            }
            MtgoCompetitivePregameStageV1::GameplayReady => {
                Err("placeholder pregame controller has no gameplay-ready decision".to_owned())
            }
        }
    }
}

impl MtgoCompetitiveNativePregameScorerV1 for MtgoPlaceholderPregameControllerV1 {
    fn score_pregame_v1(
        &mut self,
        model_input: &MtgoCompetitiveNativePregameModelInputV1,
        model_input_commitment_sha256: &str,
        deployment_commitment_sha256: &str,
    ) -> Result<MtgoCompetitiveNativePregameScoreResponseV1, String> {
        let chosen = self.choose_index_v1(model_input)?;
        let ordered_action_logits_f32_bits = (0..model_input.ordered_actions.len())
            .map(|index| if index == chosen { 1.0_f32 } else { 0.0_f32 }.to_bits())
            .collect();
        Ok(MtgoCompetitiveNativePregameScoreResponseV1 {
            schema_version: MTGO_COMPETITIVE_AUXILIARY_MODEL_SCORING_SCHEMA_V1,
            model_input_commitment_sha256: model_input_commitment_sha256.to_owned(),
            deployment_commitment_sha256: deployment_commitment_sha256.to_owned(),
            ordered_action_logits_f32_bits,
            value_f32_bits: 0.0_f32.to_bits(),
        })
    }
}

impl MtgoDeploymentSlotV1 for MtgoPlaceholderPregameControllerV1 {
    fn slot_descriptor_v1(&self, kind: MtgoDeploymentSlotKindV1) -> MtgoDeploymentSlotDescriptorV1 {
        MtgoDeploymentSlotDescriptorV1 {
            kind,
            implementation_id: format!(
                "placeholder_pregame_controller_rule_table_v1:{}",
                self.rule_table_commitment_sha256
            ),
            is_placeholder: true,
            qualified_for_live: false,
            contract_version: 1,
        }
    }
}
```

In `lib.rs` add `mod placeholder_pregame_controller;` and:

```rust
pub use placeholder_pregame_controller::{
    MtgoPlaceholderPregameControllerV1, MTGO_PLACEHOLDER_PREGAME_RULE_TABLE_V1,
};
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib placeholder_pregame_controller`
Expected: `test result: ok. 9 passed`. If `score_checked_untrusted_competitive_native_pregame_v1` rejects the test input, read its validator error string and adjust only the test fixture (for example the `prospective_keep_size` field), never the rule.

- [ ] **Step 5: Commit**

```bash
git add integrations/mtgo_dxgi_capture_v1/src/placeholder_pregame_controller.rs integrations/mtgo_dxgi_capture_v1/src/lib.rs
git commit -m "mtgo: add the provisional placeholder pregame controller"
git push origin lead/mtgo-integration-v1
```

---

### Task 4: Placeholder sideboard controller (unchanged submission)

**Files:**
- Create: `integrations/mtgo_dxgi_capture_v1/src/placeholder_sideboard_controller.rs`
- Modify: `integrations/mtgo_dxgi_capture_v1/src/lib.rs` (add `mod placeholder_sideboard_controller;` and `pub use placeholder_sideboard_controller::MtgoPlaceholderSideboardControllerV1;`)

**Interfaces:**
- Consumes: Task 1 slot types; existing sideboard traits and types listed above.
- Produces: `MtgoPlaceholderSideboardControllerV1` (unit struct) implementing `MtgoCompetitiveNativeSideboardScorerV1`, `MtgoCompetitiveNativeSideboardDeliberationScorerV1`, and `MtgoDeploymentSlotV1`.

- [ ] **Step 1: Write the failing test**

```rust
//! Placeholder sideboard controller (spec 6.5): submits the current
//! configuration unchanged before every game after the first, on both the
//! whole-target and the sequential deliberation interfaces. A trained
//! sideboard head replaces it behind the same slot.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        score_checked_untrusted_competitive_native_sideboard_v1,
        MtgoCompetitiveNativeSideboardCardCountV1, MtgoCompetitiveNativeSideboardConfigurationV1,
        MtgoCompetitiveNativeSideboardDeliberationActionV1,
        MtgoCompetitiveNativeSideboardDeliberationDecisionV1,
        MtgoCompetitiveNativeSideboardModelInputV1,
        MTGO_COMPETITIVE_NATIVE_SIDEBOARD_DELIBERATION_SCHEMA_V1,
    };

    fn deck_v1() -> MtgoCompetitiveNativeSideboardConfigurationV1 {
        MtgoCompetitiveNativeSideboardConfigurationV1 {
            mainboard: vec![
                MtgoCompetitiveNativeSideboardCardCountV1 { visible_card_name: "Lightning Bolt".to_owned(), count: 4 },
                MtgoCompetitiveNativeSideboardCardCountV1 { visible_card_name: "Mountain".to_owned(), count: 56 },
            ],
            sideboard: vec![MtgoCompetitiveNativeSideboardCardCountV1 { visible_card_name: "Lightning Bolt".to_owned(), count: 15 }],
        }
    }

    fn input_v1() -> MtgoCompetitiveNativeSideboardModelInputV1 {
        MtgoCompetitiveNativeSideboardModelInputV1 {
            next_game_number: 2,
            acting_player_games_won: 1,
            opponent_games_won: 0,
            current_configuration: deck_v1(),
        }
    }

    #[test]
    fn whole_target_response_is_the_unchanged_current_configuration() {
        let mut controller = MtgoPlaceholderSideboardControllerV1;
        let selection = score_checked_untrusted_competitive_native_sideboard_v1(&input_v1(), &"d".repeat(64), &mut controller).unwrap();
        assert_eq!(selection.selection_v1().target_configuration, deck_v1());
        assert!(selection.no_changes_selected_v1(&input_v1()));
        assert!(!selection.safe_for_live_input_v1());
        assert!(!selection.permits_sideboard_submission_v1());
        assert!(!selection.permits_spending_v1());
    }

    #[test]
    fn deliberation_selects_submit_configuration_and_rejects_its_absence() {
        let mut controller = MtgoPlaceholderSideboardControllerV1;
        let decision = MtgoCompetitiveNativeSideboardDeliberationDecisionV1 {
            schema_version: MTGO_COMPETITIVE_NATIVE_SIDEBOARD_DELIBERATION_SCHEMA_V1,
            decision_number: 1,
            model_input_commitment_sha256: "a".repeat(64),
            deployment_commitment_sha256: "b".repeat(64),
            prior_trace_commitment_sha256: "c".repeat(64),
            candidate_configuration: deck_v1(),
            ordered_actions: vec![
                MtgoCompetitiveNativeSideboardDeliberationActionV1::MoveOneToMainboard { visible_card_name: "Lightning Bolt".to_owned() },
                MtgoCompetitiveNativeSideboardDeliberationActionV1::SubmitConfiguration,
            ],
            decision_commitment_sha256: "e".repeat(64),
        };
        let response = controller.score_sideboard_deliberation_v1(&decision).unwrap();
        assert_eq!(response.ordered_logits_f32_bits, vec![0.0_f32.to_bits(), 1.0_f32.to_bits()]);
        assert_eq!(response.decision_commitment_sha256, "e".repeat(64));
        assert_eq!(response.deployment_commitment_sha256, "b".repeat(64));
        let mut without_submit = decision;
        without_submit.ordered_actions.pop();
        assert!(controller.score_sideboard_deliberation_v1(&without_submit).is_err());
    }

    #[test]
    fn slot_descriptor_is_a_placeholder() {
        let descriptor = MtgoPlaceholderSideboardControllerV1.slot_descriptor_v1(MtgoDeploymentSlotKindV1::SideboardController);
        assert_eq!(descriptor.implementation_id, "placeholder_sideboard_controller_unchanged_v1");
        assert!(descriptor.is_placeholder);
        assert!(!descriptor.qualified_for_live);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib placeholder_sideboard_controller`
Expected: compile error, `MtgoPlaceholderSideboardControllerV1` not found.

- [ ] **Step 3: Write minimal implementation**

```rust
use crate::{
    MtgoCompetitiveNativeSideboardDeliberationActionV1,
    MtgoCompetitiveNativeSideboardDeliberationDecisionV1,
    MtgoCompetitiveNativeSideboardDeliberationScoreResponseV1,
    MtgoCompetitiveNativeSideboardDeliberationScorerV1, MtgoCompetitiveNativeSideboardModelInputV1,
    MtgoCompetitiveNativeSideboardModelSelectionV1, MtgoCompetitiveNativeSideboardScoreResponseV1,
    MtgoCompetitiveNativeSideboardScorerV1, MtgoDeploymentSlotDescriptorV1,
    MtgoDeploymentSlotKindV1, MtgoDeploymentSlotV1,
    MTGO_COMPETITIVE_AUXILIARY_MODEL_SCORING_SCHEMA_V1,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct MtgoPlaceholderSideboardControllerV1;

impl MtgoCompetitiveNativeSideboardScorerV1 for MtgoPlaceholderSideboardControllerV1 {
    fn score_sideboard_v1(
        &mut self,
        model_input: &MtgoCompetitiveNativeSideboardModelInputV1,
        model_input_commitment_sha256: &str,
        deployment_commitment_sha256: &str,
    ) -> Result<MtgoCompetitiveNativeSideboardScoreResponseV1, String> {
        Ok(MtgoCompetitiveNativeSideboardScoreResponseV1 {
            schema_version: MTGO_COMPETITIVE_AUXILIARY_MODEL_SCORING_SCHEMA_V1,
            model_input_commitment_sha256: model_input_commitment_sha256.to_owned(),
            deployment_commitment_sha256: deployment_commitment_sha256.to_owned(),
            selection: MtgoCompetitiveNativeSideboardModelSelectionV1 {
                target_configuration: model_input.current_configuration.clone(),
            },
            value_f32_bits: 0.0_f32.to_bits(),
        })
    }
}

impl MtgoCompetitiveNativeSideboardDeliberationScorerV1 for MtgoPlaceholderSideboardControllerV1 {
    fn score_sideboard_deliberation_v1(
        &mut self,
        decision: &MtgoCompetitiveNativeSideboardDeliberationDecisionV1,
    ) -> Result<MtgoCompetitiveNativeSideboardDeliberationScoreResponseV1, String> {
        let submit = decision
            .ordered_actions
            .iter()
            .position(|action| {
                matches!(action, MtgoCompetitiveNativeSideboardDeliberationActionV1::SubmitConfiguration)
            })
            .ok_or_else(|| {
                "placeholder sideboard controller requires a submit action".to_owned()
            })?;
        Ok(MtgoCompetitiveNativeSideboardDeliberationScoreResponseV1 {
            schema_version: decision.schema_version,
            decision_commitment_sha256: decision.decision_commitment_sha256.clone(),
            deployment_commitment_sha256: decision.deployment_commitment_sha256.clone(),
            ordered_logits_f32_bits: (0..decision.ordered_actions.len())
                .map(|index| if index == submit { 1.0_f32 } else { 0.0_f32 }.to_bits())
                .collect(),
            value_f32_bits: 0.0_f32.to_bits(),
        })
    }
}

impl MtgoDeploymentSlotV1 for MtgoPlaceholderSideboardControllerV1 {
    fn slot_descriptor_v1(&self, kind: MtgoDeploymentSlotKindV1) -> MtgoDeploymentSlotDescriptorV1 {
        MtgoDeploymentSlotDescriptorV1 {
            kind,
            implementation_id: "placeholder_sideboard_controller_unchanged_v1".to_owned(),
            is_placeholder: true,
            qualified_for_live: false,
            contract_version: 1,
        }
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib placeholder_sideboard_controller`
Expected: `test result: ok. 3 passed`.

- [ ] **Step 5: Commit**

```bash
git add integrations/mtgo_dxgi_capture_v1/src/placeholder_sideboard_controller.rs integrations/mtgo_dxgi_capture_v1/src/lib.rs
git commit -m "mtgo: add the unchanged-submission placeholder sideboard controller"
git push origin lead/mtgo-integration-v1
```

---

### Task 5: Search-root provider slot and raw-policy-only placeholder

**Files:**
- Create: `integrations/mtgo_dxgi_capture_v1/src/search_root_provider.rs`
- Modify: `integrations/mtgo_dxgi_capture_v1/src/lib.rs` (add `mod search_root_provider;` and a `pub use search_root_provider::{...}` block)
- Modify: `integrations/mtgo_dxgi_capture_v1/src/deployment_slots.rs` (tighten the `R` bound)

**Interfaces:**
- Consumes: Task 1 slot types; `mtgo_blackbox_v1::MtgoPlayerVisibleDuelDecisionInputV1`.
- Produces: `MtgoSearchRootProviderV1` (trait), `MtgoSearchRootDecisionV1::RawPolicyOnly { reason }`, `MtgoRawPolicyOnlySearchRootProviderV1`, `MTGO_SEARCH_ROOT_NOT_RATIFIED_REASON_V1`.

- [ ] **Step 1: Write the failing test**

```rust
//! Search-root provider slot (spec 6.4). The ratified bounded search needs a
//! full kernel session; a player-visible search root constructor is its own
//! future ratification. Until then this slot answers raw policy only, and
//! every result must be labeled accordingly.

#[cfg(test)]
mod tests {
    use super::*;
    use mtgo_blackbox_v1::{MtgoPlayerVisibleDuelDecisionInputV1, MtgoPlayerVisibleDuelStateV1};

    #[test]
    fn placeholder_answers_raw_policy_only_with_the_fixed_reason() {
        let provider = MtgoRawPolicyOnlySearchRootProviderV1;
        let fixture: serde_json::Value = serde_json::from_str(SAMPLE_INPUT_JSON).unwrap();
        let input: MtgoPlayerVisibleDuelDecisionInputV1 =
            serde_json::from_value(fixture["model_input"].clone()).unwrap();
        assert_eq!(
            provider.search_root_v1(&input),
            MtgoSearchRootDecisionV1::RawPolicyOnly { reason: MTGO_SEARCH_ROOT_NOT_RATIFIED_REASON_V1.to_owned() }
        );
        let descriptor = provider.slot_descriptor_v1(MtgoDeploymentSlotKindV1::SearchRootProvider);
        assert_eq!(descriptor.implementation_id, "search_root_provider_raw_policy_only_v1");
        assert!(descriptor.is_placeholder);
        assert!(!descriptor.qualified_for_live);
        let _ = std::any::type_name::<MtgoPlayerVisibleDuelStateV1>();
    }

    /// Reuse the adapter's conformance fixture rather than hand-building the
    /// large state struct. Its top-level object has the keys schema_version,
    /// fixture_id, model_input, expected_input_commitment_sha256,
    /// synthetic_simulator_setup, expected_flat_v2; the decision input is the
    /// `model_input` object.
    const SAMPLE_INPUT_JSON: &str = include_str!(
        "../../mtgo_blackbox_v1/fixtures/player_visible_flat_v2_conformance_source_v1.json"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib search_root_provider`
Expected: compile error, trait not found.

- [ ] **Step 3: Write minimal implementation**

```rust
use crate::{MtgoDeploymentSlotDescriptorV1, MtgoDeploymentSlotKindV1, MtgoDeploymentSlotV1};
use mtgo_blackbox_v1::MtgoPlayerVisibleDuelDecisionInputV1;
use serde::{Deserialize, Serialize};

pub const MTGO_SEARCH_ROOT_NOT_RATIFIED_REASON_V1: &str = "player_visible_search_root_not_ratified_v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case", deny_unknown_fields)]
pub enum MtgoSearchRootDecisionV1 {
    /// No search root exists for this decision; the raw policy scores it and
    /// the result is labeled raw policy.
    RawPolicyOnly { reason: String },
}

pub trait MtgoSearchRootProviderV1 {
    fn search_root_v1(&self, decision: &MtgoPlayerVisibleDuelDecisionInputV1) -> MtgoSearchRootDecisionV1;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct MtgoRawPolicyOnlySearchRootProviderV1;

impl MtgoSearchRootProviderV1 for MtgoRawPolicyOnlySearchRootProviderV1 {
    fn search_root_v1(&self, _decision: &MtgoPlayerVisibleDuelDecisionInputV1) -> MtgoSearchRootDecisionV1 {
        MtgoSearchRootDecisionV1::RawPolicyOnly {
            reason: MTGO_SEARCH_ROOT_NOT_RATIFIED_REASON_V1.to_owned(),
        }
    }
}

impl MtgoDeploymentSlotV1 for MtgoRawPolicyOnlySearchRootProviderV1 {
    fn slot_descriptor_v1(&self, kind: MtgoDeploymentSlotKindV1) -> MtgoDeploymentSlotDescriptorV1 {
        MtgoDeploymentSlotDescriptorV1 {
            kind,
            implementation_id: "search_root_provider_raw_policy_only_v1".to_owned(),
            is_placeholder: true,
            qualified_for_live: false,
            contract_version: 1,
        }
    }
}
```

In `lib.rs` add `mod search_root_provider;` and:

```rust
pub use search_root_provider::{
    MtgoRawPolicyOnlySearchRootProviderV1, MtgoSearchRootDecisionV1, MtgoSearchRootProviderV1,
    MTGO_SEARCH_ROOT_NOT_RATIFIED_REASON_V1,
};
```

In `deployment_slots.rs` change the registry `where` clause bounds to the slot traits now that they exist:

```rust
where
    D: mtgo_blackbox_v1::MtgoPlayerVisibleDuelScorerV1 + MtgoDeploymentSlotV1,
    P: crate::MtgoCompetitiveNativePregameScorerV1 + MtgoDeploymentSlotV1,
    S: crate::MtgoCompetitiveNativeSideboardScorerV1
        + crate::MtgoCompetitiveNativeSideboardDeliberationScorerV1
        + MtgoDeploymentSlotV1,
    U: MtgoDeploymentSlotV1,
    R: crate::MtgoSearchRootProviderV1 + MtgoDeploymentSlotV1,
```

and update the Task 1 test stub so `Stub` also implements the four scorer traits with bodies that return `Err("stub".to_owned())` for the pregame, sideboard, deliberation, and duel scorer methods and `MtgoSearchRootDecisionV1::RawPolicyOnly { reason: "stub".to_owned() }` for the search root.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib search_root_provider` then `cargo test --lib deployment_slots`
Expected: both `ok`.

- [ ] **Step 5: Commit**

```bash
git add integrations/mtgo_dxgi_capture_v1/src/search_root_provider.rs integrations/mtgo_dxgi_capture_v1/src/deployment_slots.rs integrations/mtgo_dxgi_capture_v1/src/lib.rs
git commit -m "mtgo: add the search-root provider slot with a raw-policy-only placeholder"
git push origin lead/mtgo-integration-v1
```

---

### Task 6: Placeholder visible duel scorer and the placeholder builder

**Files:**
- Create: `integrations/mtgo_dxgi_capture_v1/src/placeholder_visible_duel_scorer.rs`
- Modify: `integrations/mtgo_dxgi_capture_v1/src/deployment_slots.rs` (add `build_placeholder_deployment_slots_v1`)
- Modify: `integrations/mtgo_dxgi_capture_v1/src/lib.rs` (add `mod placeholder_visible_duel_scorer;`, re-export `MtgoPlaceholderVisibleDuelScorerV1`, `visible_card_names_v1`, `MTGO_VISIBLE_KERNEL_SCORER_NOT_QUALIFIED_REASON_V1`, and `build_placeholder_deployment_slots_v1`, `MtgoPlaceholderDeploymentSlotsV1`)

**Interfaces:**
- Consumes: Tasks 1 to 5; `mtgo_blackbox_v1::{MtgoPlayerVisibleDuelScorerV1, MtgoPlayerVisibleDuelScoreResponseV1, MtgoPlayerVisibleDuelDecisionInputV1}`.
- Produces: `visible_card_names_v1(&MtgoPlayerVisibleDuelDecisionInputV1) -> Vec<String>` (sorted, deduplicated), `MtgoPlaceholderVisibleDuelScorerV1::new_v1(MtgoUnknownCardPolicyV1)`, `build_placeholder_deployment_slots_v1(&MtgoCompetitiveNativeSideboardConfigurationV1) -> Result<MtgoPlaceholderDeploymentSlotsV1, String>` where `MtgoPlaceholderDeploymentSlotsV1 = MtgoDeploymentSlotsV1<MtgoPlaceholderVisibleDuelScorerV1, MtgoPlaceholderPregameControllerV1, MtgoPlaceholderSideboardControllerV1, MtgoUnknownCardPolicyV1, MtgoRawPolicyOnlySearchRootProviderV1>`.

- [ ] **Step 1: Write the failing test**

```rust
//! Placeholder duel-scorer slot. It resolves every visible card name through
//! the unknown-card policy and then fails closed with a fixed reason, because
//! the kernel-owned player-visible scorer (spec 6.3) is not built yet. That
//! scorer replaces this placeholder behind the same slot.

#[cfg(test)]
mod tests {
    use super::*;
    use mtgo_blackbox_v1::{MtgoPlayerVisibleDuelDecisionInputV1, MtgoPlayerVisibleDuelScorerV1};

    fn sample_input() -> MtgoPlayerVisibleDuelDecisionInputV1 {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../mtgo_blackbox_v1/fixtures/player_visible_flat_v2_conformance_source_v1.json"
        ))
        .unwrap();
        serde_json::from_value(fixture["model_input"].clone()).unwrap()
    }

    #[test]
    fn collects_every_visible_name_sorted_and_deduplicated() {
        let names = visible_card_names_v1(&sample_input());
        assert!(!names.is_empty());
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(names, sorted);
    }

    #[test]
    fn all_known_names_fail_closed_with_the_not_qualified_reason() {
        let mut scorer = MtgoPlaceholderVisibleDuelScorerV1::new_v1(MtgoUnknownCardPolicyV1::FailClosedHumanTakeover);
        let error = scorer.score_player_visible_duel_v1(&sample_input()).unwrap_err();
        assert_eq!(error, MTGO_VISIBLE_KERNEL_SCORER_NOT_QUALIFIED_REASON_V1);
    }

    #[test]
    fn an_unknown_name_abstains_before_the_not_qualified_reason() {
        let mut input = sample_input();
        input.current_state.battlefield[1].push(mtgo_blackbox_v1::MtgoPlayerVisibleBattlefieldCardV1 {
            object_ref: mtgo_blackbox_v1::MtgoPlayerVisibleObjectRefV1 { visible_ordinal: 9_000 },
            card_name: "Not A Kernel Card".to_owned(),
            tapped: false,
            marked_damage: 0,
            counters: mtgo_blackbox_v1::MtgoPlayerVisibleCounterStateV1 {
                plus_one_plus_one: 0,
                minus_one_minus_one: 0,
                minus_zero_minus_one: 0,
                stun: 0,
                lore: 0,
            },
            is_token: false,
            visible_effective_power: None,
            visible_effective_toughness: None,
        });
        let mut scorer = MtgoPlaceholderVisibleDuelScorerV1::new_v1(MtgoUnknownCardPolicyV1::FailClosedHumanTakeover);
        let error = scorer.score_player_visible_duel_v1(&input).unwrap_err();
        assert!(error.starts_with(MTGO_UNKNOWN_CARD_FAIL_CLOSED_REASON_V1));
        assert!(error.contains("Not A Kernel Card"));
    }

    #[test]
    fn placeholder_builder_wires_all_five_slots_as_placeholders() {
        let deck = crate::MtgoCompetitiveNativeSideboardConfigurationV1 {
            mainboard: vec![
                crate::MtgoCompetitiveNativeSideboardCardCountV1 { visible_card_name: "Lightning Bolt".to_owned(), count: 4 },
                crate::MtgoCompetitiveNativeSideboardCardCountV1 { visible_card_name: "Mountain".to_owned(), count: 56 },
            ],
            sideboard: vec![crate::MtgoCompetitiveNativeSideboardCardCountV1 { visible_card_name: "Lightning Bolt".to_owned(), count: 15 }],
        };
        let slots = crate::build_placeholder_deployment_slots_v1(&deck).unwrap();
        let report = slots.slot_report_v1();
        assert!(report.all_slots_wired);
        assert_eq!(report.placeholder_slot_count, 5);
        assert!(!report.grants_live_authority);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib placeholder_visible_duel_scorer`
Expected: compile error.

- [ ] **Step 3: Write minimal implementation**

```rust
use crate::{
    MtgoDeploymentSlotDescriptorV1, MtgoDeploymentSlotKindV1, MtgoDeploymentSlotV1,
    MtgoUnknownCardPolicyV1,
};
use mtgo_blackbox_v1::{
    MtgoPlayerVisibleDuelDecisionInputV1, MtgoPlayerVisibleDuelScoreResponseV1,
    MtgoPlayerVisibleDuelScorerV1,
};

pub const MTGO_VISIBLE_KERNEL_SCORER_NOT_QUALIFIED_REASON_V1: &str =
    "player_visible_kernel_scorer_not_qualified_v1";

/// Every card name the seated player can see in one decision, sorted and
/// deduplicated. Hidden zones are never enumerated because the input schema
/// cannot carry them.
pub fn visible_card_names_v1(input: &MtgoPlayerVisibleDuelDecisionInputV1) -> Vec<String> {
    let state = &input.current_state;
    let mut names: Vec<String> = Vec::new();
    names.extend(state.own_hand.iter().map(|card| card.card_name.clone()));
    for side in &state.battlefield {
        names.extend(side.iter().map(|card| card.card_name.clone()));
    }
    for side in &state.graveyards {
        names.extend(side.iter().map(|card| card.card_name.clone()));
    }
    names.extend(state.exile.iter().filter_map(|card| card.visible_card_name.clone()));
    names.extend(state.stack.iter().filter_map(|item| item.visible_source_name.clone()));
    for side in &state.known_library_cards {
        names.extend(side.iter().map(|card| card.card.card_name.clone()));
    }
    for side in &state.known_hand_cards {
        names.extend(side.iter().map(|card| card.card_name.clone()));
    }
    names.sort();
    names.dedup();
    names
}

pub struct MtgoPlaceholderVisibleDuelScorerV1 {
    policy: MtgoUnknownCardPolicyV1,
}

impl MtgoPlaceholderVisibleDuelScorerV1 {
    pub fn new_v1(policy: MtgoUnknownCardPolicyV1) -> Self {
        Self { policy }
    }
}

impl MtgoPlayerVisibleDuelScorerV1 for MtgoPlaceholderVisibleDuelScorerV1 {
    fn score_player_visible_duel_v1(
        &mut self,
        model_input: &MtgoPlayerVisibleDuelDecisionInputV1,
    ) -> Result<MtgoPlayerVisibleDuelScoreResponseV1, String> {
        let names = visible_card_names_v1(model_input);
        let borrowed: Vec<&str> = names.iter().map(String::as_str).collect();
        self.policy
            .resolve_visible_card_names_v1(&borrowed)
            .map_err(|abstention| {
                format!(
                    "{}:{}",
                    abstention.reason,
                    abstention.unknown_visible_card_names.join(",")
                )
            })?;
        Err(MTGO_VISIBLE_KERNEL_SCORER_NOT_QUALIFIED_REASON_V1.to_owned())
    }
}

impl MtgoDeploymentSlotV1 for MtgoPlaceholderVisibleDuelScorerV1 {
    fn slot_descriptor_v1(&self, kind: MtgoDeploymentSlotKindV1) -> MtgoDeploymentSlotDescriptorV1 {
        MtgoDeploymentSlotDescriptorV1 {
            kind,
            implementation_id: "placeholder_visible_duel_scorer_fail_closed_v1".to_owned(),
            is_placeholder: true,
            qualified_for_live: false,
            contract_version: 1,
        }
    }
}
```

Append to `deployment_slots.rs` (above its tests):

```rust
pub type MtgoPlaceholderDeploymentSlotsV1 = MtgoDeploymentSlotsV1<
    crate::MtgoPlaceholderVisibleDuelScorerV1,
    crate::MtgoPlaceholderPregameControllerV1,
    crate::MtgoPlaceholderSideboardControllerV1,
    crate::MtgoUnknownCardPolicyV1,
    crate::MtgoRawPolicyOnlySearchRootProviderV1,
>;

/// Builds the all-placeholder deployment for one deck. Every slot is wired;
/// none is qualified for live use.
pub fn build_placeholder_deployment_slots_v1(
    deck: &crate::MtgoCompetitiveNativeSideboardConfigurationV1,
) -> Result<MtgoPlaceholderDeploymentSlotsV1, String> {
    let policy = crate::MtgoUnknownCardPolicyV1::FailClosedHumanTakeover;
    Ok(MtgoDeploymentSlotsV1 {
        duel_scorer: crate::MtgoPlaceholderVisibleDuelScorerV1::new_v1(policy),
        pregame_controller: crate::MtgoPlaceholderPregameControllerV1::new_v1(deck, policy)?,
        sideboard_controller: crate::MtgoPlaceholderSideboardControllerV1,
        unknown_card_policy: policy,
        search_root_provider: crate::MtgoRawPolicyOnlySearchRootProviderV1,
    })
}
```

In `lib.rs` add `mod placeholder_visible_duel_scorer;` and extend the re-exports:

```rust
pub use deployment_slots::{
    build_placeholder_deployment_slots_v1, MtgoDeploymentSlotDescriptorV1, MtgoDeploymentSlotKindV1,
    MtgoDeploymentSlotReportV1, MtgoDeploymentSlotV1, MtgoDeploymentSlotsV1,
    MtgoPlaceholderDeploymentSlotsV1, MTGO_DEPLOYMENT_SLOT_REPORT_SCHEMA_V1,
};
pub use placeholder_visible_duel_scorer::{
    visible_card_names_v1, MtgoPlaceholderVisibleDuelScorerV1,
    MTGO_VISIBLE_KERNEL_SCORER_NOT_QUALIFIED_REASON_V1,
};
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib placeholder_visible_duel_scorer` then `cargo test --lib`
Expected: the four new tests pass and the whole lib suite stays green.

- [ ] **Step 5: Commit**

```bash
git add integrations/mtgo_dxgi_capture_v1/src/placeholder_visible_duel_scorer.rs integrations/mtgo_dxgi_capture_v1/src/deployment_slots.rs integrations/mtgo_dxgi_capture_v1/src/lib.rs
git commit -m "mtgo: add the fail-closed placeholder duel scorer and the placeholder deployment builder"
git push origin lead/mtgo-integration-v1
```

---

### Task 7: Readiness fields and the slot-report bin

**Files:**
- Modify: `integrations/mtgo_dxgi_capture_v1/src/competitive_model_decision_readiness.rs` (struct at lines 10-68, constructor at lines 124-190)
- Create: `integrations/mtgo_dxgi_capture_v1/src/bin/check_mtgo_deployment_slots_v1.rs`
- Modify: `integrations/mtgo_dxgi_capture_v1/README.md` (one paragraph after the paragraph that documents `check_mtgo_competitive_model_decision_readiness_v1`)

**Interfaces:**
- Consumes: Task 6 `build_placeholder_deployment_slots_v1`.
- Produces: five new readiness fields, all `true`; the bin prints `MtgoDeploymentSlotReportV1` as pretty JSON and exits 0.

- [ ] **Step 1: Write the failing test**

Add to the existing `#[cfg(test)]` module of `competitive_model_decision_readiness.rs` (create the module at the end of the file if there is none):

```rust
#[cfg(test)]
mod placeholder_readiness_tests {
    use super::*;

    #[test]
    fn placeholder_slots_are_reported_without_changing_model_owned_flags() {
        let report = check_competitive_model_decision_readiness_v1();
        assert!(report.placeholder_pregame_controller_present);
        assert!(report.placeholder_sideboard_controller_present);
        assert!(report.unknown_card_policy_fail_closed_present);
        assert!(report.search_root_provider_interface_present);
        assert!(report.deployment_slot_report_present);
        assert!(!report.terminal_outcome_trained_pregame_head_present);
        assert!(!report.terminal_outcome_trained_sideboard_head_present);
        assert!(!report.public_model_owned_pregame_action_path_present);
        assert!(!report.all_required_model_decision_surfaces_present);
        assert!(!report.grants_live_authority);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib placeholder_readiness_tests`
Expected: compile error, unknown fields.

- [ ] **Step 3: Write minimal implementation**

In the struct, immediately before `pub all_required_model_decision_surfaces_present: bool,` add:

```rust
    /// Placeholder wiring (spec 6.5, 6.9, 6.4). True means the slot is wired
    /// with a disclosed placeholder; it never implies a trained head.
    pub placeholder_pregame_controller_present: bool,
    pub placeholder_sideboard_controller_present: bool,
    pub unknown_card_policy_fail_closed_present: bool,
    pub search_root_provider_interface_present: bool,
    pub deployment_slot_report_present: bool,
```

In the constructor's struct literal, immediately before `all_required_model_decision_surfaces_present: false,` add:

```rust
        placeholder_pregame_controller_present: true,
        placeholder_sideboard_controller_present: true,
        unknown_card_policy_fail_closed_present: true,
        search_root_provider_interface_present: true,
        deployment_slot_report_present: true,
```

Do not touch `recompute_all_required_model_decision_surfaces_present_v1`; placeholders are not model-owned surfaces.

Create the bin:

```rust
//! Prints the deployment slot report for the placeholder deployment of the
//! canonical Burn mainboard. Non-actuating: reads no MTGO process state,
//! captures no pixels, sends no input, and grants no authority.

use mtgo_dxgi_capture_v1::{
    build_placeholder_deployment_slots_v1, MtgoCompetitiveNativeSideboardCardCountV1,
    MtgoCompetitiveNativeSideboardConfigurationV1,
};

fn main() {
    let deck = MtgoCompetitiveNativeSideboardConfigurationV1 {
        mainboard: vec![
            MtgoCompetitiveNativeSideboardCardCountV1 {
                visible_card_name: "Lightning Bolt".to_owned(),
                count: 4,
            },
            MtgoCompetitiveNativeSideboardCardCountV1 {
                visible_card_name: "Mountain".to_owned(),
                count: 56,
            },
        ],
        sideboard: vec![MtgoCompetitiveNativeSideboardCardCountV1 {
            visible_card_name: "Lightning Bolt".to_owned(),
            count: 15,
        }],
    };
    match build_placeholder_deployment_slots_v1(&deck) {
        Ok(slots) => {
            let report = slots.slot_report_v1();
            println!(
                "{}",
                serde_json::to_string_pretty(&report).expect("slot report serializes")
            );
        }
        Err(error) => {
            eprintln!("MTGO_DEPLOYMENT_SLOTS_V1_REJECTED:{error}");
            std::process::exit(1);
        }
    }
}
```

README paragraph:

```markdown
`check_mtgo_deployment_slots_v1` prints the deployment slot report: one line per decision surface (duel scorer, pregame controller, sideboard controller, unknown-card policy, search-root provider) naming the bound implementation and whether it is a placeholder. All five are placeholders today. It reads no MTGO process state, captures no pixels, sends no input, and grants no authority. Run it with `cargo run --bin check_mtgo_deployment_slots_v1`.
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib placeholder_readiness_tests`, then `cargo run -q --bin check_mtgo_deployment_slots_v1`
Expected: the test passes; the bin prints JSON containing `"placeholder_slot_count": 5` and `"grants_live_authority": false`. Then run `cargo test` for the whole crate (lib, bins, tests) to confirm nothing else pins the readiness field set.

- [ ] **Step 5: Commit**

```bash
git add integrations/mtgo_dxgi_capture_v1/src/competitive_model_decision_readiness.rs integrations/mtgo_dxgi_capture_v1/src/bin/check_mtgo_deployment_slots_v1.rs integrations/mtgo_dxgi_capture_v1/README.md
git commit -m "mtgo: report placeholder slot wiring in readiness and add the slot-report bin"
git push origin lead/mtgo-integration-v1
```

---

### Task 8: CI command list for both crates and both policy suites

**Files:**
- Create: `integrations/ci_commands_v1.ps1`
- Modify: `integrations/mtgo_dxgi_capture_v1/README.md` (one sentence pointing at the script, appended to the paragraph added in Task 7)

**Interfaces:**
- Produces: a PowerShell script that fails on the first failing command and prints `MTGO_INTEGRATION_CI_V1:PASS` on success.

- [ ] **Step 1: Write the script**

```powershell
$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot
$env:PATH = "C:\Program Files (x86)\Microsoft Visual Studio\Installer;" + $env:PATH

function Invoke-Step([string]$label, [string]$directory, [scriptblock]$body) {
    Write-Output "=== $label"
    Push-Location $directory
    try {
        & $body
        if ($LASTEXITCODE -ne 0) { throw "$label failed with exit code $LASTEXITCODE" }
    } finally {
        Pop-Location
    }
}

Invoke-Step 'blackbox crate tests' (Join-Path $root 'mtgo_blackbox_v1') { cargo test --release }
Invoke-Step 'dxgi capture crate tests' (Join-Path $root 'mtgo_dxgi_capture_v1') { cargo test --release }
Invoke-Step 'broker source policy' (Join-Path $root 'mtgo_visible_duel_broker_v1') { & .\tests\source_policy_v1.ps1 }
Invoke-Step 'producer source policy' (Join-Path $root 'mtgo_visible_duel_producer_v1') { & .\tests\source_policy_v1.ps1 }
Invoke-Step 'producer build' (Join-Path $root 'mtgo_visible_duel_producer_v1') { dotnet build -c Release --nologo -v q }
Write-Output 'MTGO_INTEGRATION_CI_V1:PASS'
```

The two policy scripts print `...:PASS` and do not set `$LASTEXITCODE`; wrap each in `& { ...; $global:LASTEXITCODE = 0 }` if the check above reports a null exit code as failure.

- [ ] **Step 2: Run it**

Run from `integrations`: `powershell -NoProfile -ExecutionPolicy Bypass -File .\ci_commands_v1.ps1`
Expected: ends with `MTGO_INTEGRATION_CI_V1:PASS`.

- [ ] **Step 3: Document and commit**

Append to the Task 7 README paragraph: "The complete offline check for both crates and both policy suites is `integrations/ci_commands_v1.ps1`."

```bash
git add integrations/ci_commands_v1.ps1 integrations/mtgo_dxgi_capture_v1/README.md
git commit -m "mtgo: add the offline CI command list for the integration crates"
git push origin lead/mtgo-integration-v1
```

---

### Task 9: End-to-end wiring test through the existing checked-untrusted paths

**Files:**
- Create: `integrations/mtgo_dxgi_capture_v1/tests/deployment_slots_wiring_v1.rs`

**Interfaces:**
- Consumes: everything above through the crate's public re-exports.

- [ ] **Step 1: Write the failing test**

```rust
//! The wiring test: one placeholder deployment drives every decision surface
//! through the adapter's existing checked-untrusted entry points, and every
//! result stays non-authorizing.

use mtgo_dxgi_capture_v1::{
    build_placeholder_deployment_slots_v1, score_checked_untrusted_competitive_native_pregame_v1,
    score_checked_untrusted_competitive_native_sideboard_v1, MtgoCompetitiveNativePregameActionV1,
    MtgoCompetitiveNativePregameCardV1, MtgoCompetitiveNativePregameModelInputV1,
    MtgoCompetitiveNativeSideboardCardCountV1, MtgoCompetitiveNativeSideboardConfigurationV1,
    MtgoCompetitiveNativeSideboardModelInputV1, MtgoCompetitivePregameStageV1,
    MtgoSearchRootDecisionV1, MtgoSearchRootProviderV1, MtgoUnknownCardPolicyV1,
    MTGO_SEARCH_ROOT_NOT_RATIFIED_REASON_V1, MTGO_VISIBLE_KERNEL_SCORER_NOT_QUALIFIED_REASON_V1,
};
use mtgo_blackbox_v1::{
    MtgoCompetitivePregamePlayDrawV1, MtgoPlayerVisibleDuelDecisionInputV1,
    MtgoPlayerVisibleDuelScorerV1,
};

fn deck_v1() -> MtgoCompetitiveNativeSideboardConfigurationV1 {
    MtgoCompetitiveNativeSideboardConfigurationV1 {
        mainboard: vec![
            MtgoCompetitiveNativeSideboardCardCountV1 { visible_card_name: "Lightning Bolt".to_owned(), count: 4 },
            MtgoCompetitiveNativeSideboardCardCountV1 { visible_card_name: "Mountain".to_owned(), count: 56 },
        ],
        sideboard: vec![MtgoCompetitiveNativeSideboardCardCountV1 { visible_card_name: "Lightning Bolt".to_owned(), count: 15 }],
    }
}

fn sample_duel_input() -> MtgoPlayerVisibleDuelDecisionInputV1 {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../mtgo_blackbox_v1/fixtures/player_visible_flat_v2_conformance_source_v1.json"
    ))
    .unwrap();
    serde_json::from_value(fixture["model_input"].clone()).unwrap()
}

#[test]
fn placeholder_deployment_drives_every_surface_without_authority() {
    let mut slots = build_placeholder_deployment_slots_v1(&deck_v1()).unwrap();
    let deployment = "f".repeat(64);

    let pregame = MtgoCompetitiveNativePregameModelInputV1 {
        game_number: 1,
        play_draw: MtgoCompetitivePregamePlayDrawV1::OnPlay,
        acting_player_games_won: 0,
        opponent_games_won: 0,
        player_known_deck_configuration: deck_v1(),
        stage: MtgoCompetitivePregameStageV1::MulliganChoice { prospective_keep_size: 7 },
        prospective_keep_size: Some(7),
        required_bottom_count: 0,
        selected_bottom_count: 0,
        ordered_visible_cards: ["Mountain", "Mountain", "Mountain", "Lightning Bolt", "Lightning Bolt", "Lightning Bolt", "Lightning Bolt"]
            .iter()
            .enumerate()
            .map(|(slot, name)| MtgoCompetitiveNativePregameCardV1 { card_slot: slot as u8, visible_card_name: (*name).to_owned(), selected_for_bottom: false })
            .collect(),
        ordered_confirmed_bottom_slots: Vec::new(),
        ordered_actions: vec![
            MtgoCompetitiveNativePregameActionV1::KeepOpeningHand,
            MtgoCompetitiveNativePregameActionV1::Mulligan { next_hand_size: 6 },
        ],
    };
    let pregame_selection = score_checked_untrusted_competitive_native_pregame_v1(&pregame, &deployment, &mut slots.pregame_controller).unwrap();
    assert_eq!(pregame_selection.selected_action_v1(), &MtgoCompetitiveNativePregameActionV1::KeepOpeningHand);
    assert!(!pregame_selection.safe_for_live_input_v1());

    let sideboard = MtgoCompetitiveNativeSideboardModelInputV1 { next_game_number: 2, acting_player_games_won: 1, opponent_games_won: 0, current_configuration: deck_v1() };
    let sideboard_selection = score_checked_untrusted_competitive_native_sideboard_v1(&sideboard, &deployment, &mut slots.sideboard_controller).unwrap();
    assert_eq!(sideboard_selection.selection_v1().target_configuration, deck_v1());
    assert!(!sideboard_selection.permits_sideboard_submission_v1());

    let duel = sample_duel_input();
    assert_eq!(slots.search_root_provider.search_root_v1(&duel), MtgoSearchRootDecisionV1::RawPolicyOnly { reason: MTGO_SEARCH_ROOT_NOT_RATIFIED_REASON_V1.to_owned() });
    assert_eq!(slots.duel_scorer.score_player_visible_duel_v1(&duel).unwrap_err(), MTGO_VISIBLE_KERNEL_SCORER_NOT_QUALIFIED_REASON_V1);
    assert_eq!(slots.unknown_card_policy, MtgoUnknownCardPolicyV1::FailClosedHumanTakeover);

    let report = slots.slot_report_v1();
    assert!(report.all_slots_wired);
    assert_eq!(report.placeholder_slot_count, 5);
    assert!(!report.grants_live_authority);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test deployment_slots_wiring_v1`
Expected: FAIL only if a name above is not re-exported; fix the re-export in `lib.rs`, never the test's intent.

- [ ] **Step 3: Run test to verify it passes**

Run: `cargo test --test deployment_slots_wiring_v1`
Expected: `test result: ok. 1 passed`

- [ ] **Step 4: Run the full crate and the CI script**

Run: `cargo test --release` in `integrations/mtgo_dxgi_capture_v1`, then `powershell -NoProfile -ExecutionPolicy Bypass -File ..\ci_commands_v1.ps1` from that directory.
Expected: all green; `MTGO_INTEGRATION_CI_V1:PASS`.

- [ ] **Step 5: Commit**

```bash
git add integrations/mtgo_dxgi_capture_v1/tests/deployment_slots_wiring_v1.rs
git commit -m "mtgo: add the end-to-end placeholder deployment wiring test"
git push origin lead/mtgo-integration-v1
```

---

## Follow-on plans (separate documents, in this order)

Each is its own plan with its own tasks; none is started by this document.

1. **Kernel player-visible scorer and validation-only history importer** (spec 6.3): `mtg-kernel/src/external_player_visible_scoring_v1.rs` per `integrations/mtgo_blackbox_v1/PLAYER_VISIBLE_NATIVE_SCORER_V1.md`, its 14 required tests, the frozen raw gate specification, and the adapter implementation of `MtgoPlayerVisibleDuelScorerV1` that replaces `MtgoPlaceholderVisibleDuelScorerV1` behind the duel-scorer slot. On this deployment branch the encoder access is a narrow `pub(crate)` entry point in `flat_policy_v2.rs`, added once and reviewed, so the eventual main-line ruling patch is that same narrow interface.
2. **Producer combat-phase pass windows and refusal codes** (spec 6.2 steps 1 and 2): `SanitizedVisibleDecisionV1.cs` phase admission with seated-player priority and an enabled bound Pass, the closed abstention-reason enum, synthetic WPF fixtures, producer version bump and pin regeneration.
3. **Two-local dispatch broker and generation token** (spec 6.2 steps 3 and 4): `MTGO_LIVE_PINNED_TWO_LOCAL_CLIENTS_V1` plus `MTGO_LIVE_DISPATCH_ADMITTED_V1` build, the private observation-generation token replacing the hash-only replay key, single-use request semantics, Rust runtime verifier and pins.
4. **Postcondition bracket, lifecycle detection, and clock parser** (spec 6.6 and 6.7, B+ retained bracket): joining the existing before-and-after composed-frame bracket to sealed dispatch, the qualified numeric clock parser, lifecycle fault tests.
5. **Reachable-action inventory for the deployed 75 cards** (spec 6.2 step 5): per-family producer mappings, binders, readiness flags, and fixtures, finalized after the deck ruling.
