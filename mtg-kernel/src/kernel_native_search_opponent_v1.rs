//! Deterministic kernel-native information-set search opponent.
//!
//! This module is the disjoint stage-1 core from the countersigned
//! `KERNEL-NATIVE-SEARCH-OPPONENT-V1-DESIGN.md`. It deliberately does not
//! wire the searcher into the science loop, Store records, population
//! dispatch, or a scorer bridge. The hidden-zone audit inherited by the
//! design covers Rally and Burn only. The all-nine-deck transactional tests
//! in this module close that audit gap for the checked runtime pool.

use crate::card_def::{CardType, Keywords, CARD_DEFS, KERNEL_CARDDB_HASH};
use crate::engine::{effective_power, effective_toughness, has_effective_keyword};
use crate::ids::PlayerId;
use crate::rl::{
    ActionSemanticV1, ObservationV5, PlayerSeatV1, TerminalClassificationV1, TerminalOutcomeV1,
};
use crate::rl_session::{
    FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1, FlatActionDecisionSliceErrorV1,
    RlSessionError,
};
use crate::runtime_decks::RUNTIME_DECK_CATALOG_FILE_SHA256;
use crate::state::{GameState, ObjectStateV4, SplitMix64, UndercityRoomV1, Zone};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

pub const KERNEL_NATIVE_SEARCH_AUTHORITY_SCHEMA_V1: &str =
    "kernel_native_search_opponent_authority/v1";
pub const KERNEL_NATIVE_SEARCH_AUTHORITY_KIND_V1: &str = "kernel-native-search-opponent-v1";
pub const KERNEL_NATIVE_SEARCH_ALGORITHM_V1: &str =
    "deterministic-per-simulation-redeterminized-is-mcts-integer-ucb/v1";
pub const KERNEL_NATIVE_SEARCH_NODE_KEY_V1: &str =
    "sha256-serde-json-observation-v5-actions-remaining-depth/v1";
pub const KERNEL_NATIVE_SEARCH_SEED_DOMAIN_V1: &str =
    "sha256-kernel-native-search-authority-episode-decision-substep-simulation-player/v1";
pub const KERNEL_NATIVE_SEARCH_EVALUATOR_IDENTITY_V1: &str =
    "kernel-native-opponent-only-integer-evaluator/v1";
pub const KERNEL_NATIVE_SEARCH_DEPTH_CAP_V1: u16 = 64;

/// Compile-bound seed authority for the first throughput and calibration
/// panels. `KernelNativeSearchAuthorityV1::validate` checks the embedded
/// `action_seed` against this array with a single `.contains()` check; that
/// one structural check is re-run at every call site that accepts an
/// authority (construction, digest, and the start of every action
/// selection), a temporal re-verification of one compile-bound check, not
/// two distinct checks.
pub const KERNEL_NATIVE_SEARCH_AUTHORIZED_SEEDS_V1: [u64; 4] =
    [1_987_001, 1_988_001, 1_989_001, 1_990_001];

/// Diagnostic candidate-generation allowlist for the throughput screen and
/// calibration panels (design "Calibration after implementation"): the
/// search authority has no checkpoint generation to select, so its analog
/// of the response-exploiter lineage's `candidate_gen` allowlist is simply
/// "every tier the design defines is admitted, in the fixed order the
/// design lists them." Stage 2 registers this allowlist; it does not run
/// any panel (calibration remains the coordinator's job).
pub const KERNEL_NATIVE_SEARCH_DIAGNOSTIC_TIER_ALLOWLIST_V1: [KernelNativeSearchTierV1; 4] = [
    KernelNativeSearchTierV1::T512,
    KernelNativeSearchTierV1::T2048,
    KernelNativeSearchTierV1::T8192,
    KernelNativeSearchTierV1::T32768,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KernelNativeSearchTierV1 {
    T512,
    T2048,
    T8192,
    T32768,
}

impl KernelNativeSearchTierV1 {
    pub const fn transition_budget(self) -> u32 {
        match self {
            Self::T512 => 512,
            Self::T2048 => 2_048,
            Self::T8192 => 8_192,
            Self::T32768 => 32_768,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KernelNativeSearchAuthorityV1 {
    pub schema: String,
    pub authority_kind: String,
    pub algorithm_identity: String,
    pub node_key_identity: String,
    pub tier: KernelNativeSearchTierV1,
    pub transition_budget: u32,
    pub policy_step_depth_cap: u16,
    pub seed_domain: String,
    pub evaluator_identity: String,
    pub evaluator_sha256: String,
    pub engine_commit: String,
    pub card_db_hash: u64,
    pub runtime_deck_catalog_sha256: String,
    pub private_diagnostic_identity: String,
    pub action_seed: u64,
}

impl KernelNativeSearchAuthorityV1 {
    pub fn current(
        tier: KernelNativeSearchTierV1,
        action_seed: u64,
        private_diagnostic_identity: &str,
    ) -> Result<Self, KernelNativeSearchErrorV1> {
        let record = Self {
            schema: KERNEL_NATIVE_SEARCH_AUTHORITY_SCHEMA_V1.to_string(),
            authority_kind: KERNEL_NATIVE_SEARCH_AUTHORITY_KIND_V1.to_string(),
            algorithm_identity: KERNEL_NATIVE_SEARCH_ALGORITHM_V1.to_string(),
            node_key_identity: KERNEL_NATIVE_SEARCH_NODE_KEY_V1.to_string(),
            tier,
            transition_budget: tier.transition_budget(),
            policy_step_depth_cap: KERNEL_NATIVE_SEARCH_DEPTH_CAP_V1,
            seed_domain: KERNEL_NATIVE_SEARCH_SEED_DOMAIN_V1.to_string(),
            evaluator_identity: KERNEL_NATIVE_SEARCH_EVALUATOR_IDENTITY_V1.to_string(),
            evaluator_sha256: evaluator_digest_hex_v1(),
            engine_commit: env!("MTG_KERNEL_BUILD_GIT_HEAD").to_string(),
            card_db_hash: KERNEL_CARDDB_HASH,
            runtime_deck_catalog_sha256: RUNTIME_DECK_CATALOG_FILE_SHA256.to_string(),
            private_diagnostic_identity: private_diagnostic_identity.to_string(),
            action_seed,
        };
        record.validate()?;
        Ok(record)
    }

    pub fn validate(&self) -> Result<(), KernelNativeSearchErrorV1> {
        let valid_diagnostic = self.private_diagnostic_identity
            == crate::state::DIAGNOSTIC_STATE_HASH_ALGORITHM
            || self.private_diagnostic_identity
                == crate::state::DIAGNOSTIC_STATE_HASH_ALGORITHM_ENVIRONMENT_V2;
        if self.schema != KERNEL_NATIVE_SEARCH_AUTHORITY_SCHEMA_V1
            || self.authority_kind != KERNEL_NATIVE_SEARCH_AUTHORITY_KIND_V1
            || self.algorithm_identity != KERNEL_NATIVE_SEARCH_ALGORITHM_V1
            || self.node_key_identity != KERNEL_NATIVE_SEARCH_NODE_KEY_V1
            || self.transition_budget != self.tier.transition_budget()
            || self.policy_step_depth_cap != KERNEL_NATIVE_SEARCH_DEPTH_CAP_V1
            || self.seed_domain != KERNEL_NATIVE_SEARCH_SEED_DOMAIN_V1
            || self.evaluator_identity != KERNEL_NATIVE_SEARCH_EVALUATOR_IDENTITY_V1
            || self.evaluator_sha256 != evaluator_digest_hex_v1()
            || self.engine_commit != env!("MTG_KERNEL_BUILD_GIT_HEAD")
            || !is_lower_hex_v1(&self.engine_commit, 40)
            || self.card_db_hash != KERNEL_CARDDB_HASH
            || self.runtime_deck_catalog_sha256 != RUNTIME_DECK_CATALOG_FILE_SHA256
            || !valid_diagnostic
            || !KERNEL_NATIVE_SEARCH_AUTHORIZED_SEEDS_V1.contains(&self.action_seed)
        {
            return Err(KernelNativeSearchErrorV1::InvalidAuthority);
        }
        Ok(())
    }

    pub fn digest(&self) -> Result<[u8; 32], KernelNativeSearchErrorV1> {
        self.validate()?;
        let bytes =
            serde_json::to_vec(self).map_err(|_| KernelNativeSearchErrorV1::InvalidAuthority)?;
        Ok(Sha256::digest(bytes).into())
    }

    /// Independently reconstructs a fresh authority from just this record's
    /// three minimal inputs (tier, action seed, private diagnostic
    /// identity) via `Self::current`, then compares raw canonical-JSON
    /// SHA-256 digests.
    ///
    /// This is a genuinely distinct verification STRATEGY from `validate`,
    /// not a repeat of the same field checks under a different name:
    /// `validate` inspects each field of `self` against a live/const
    /// expected value, term by term; this method never inspects `self`'s
    /// other fields at all -- it rebuilds the entire canonical record from
    /// scratch and compares the resulting digest, which covers every byte
    /// of the canonical JSON, including any field a future `validate` edit
    /// forgets to check explicitly.
    ///
    /// Deliberately hashes `self` directly with `serde_json::to_vec`
    /// (mirroring `digest`'s own byte-production step) rather than calling
    /// `self.digest()`: `digest()` calls `self.validate()` first and
    /// returns `Err` before ever serializing a record `validate` would
    /// reject, which would make this method's own verdict fully gated by
    /// `validate` again -- not independent of it, just re-wrapped. Hashing
    /// the raw bytes here means a record `validate` is buggy enough to
    /// accept can still be caught by this method on its own, because the
    /// comparison never asks `validate` for an opinion at all.
    pub(crate) fn matches_fresh_reconstruction_v1(&self) -> bool {
        let Ok(reconstructed) = Self::current(
            self.tier,
            self.action_seed,
            &self.private_diagnostic_identity,
        ) else {
            return false;
        };
        match (serde_json::to_vec(&reconstructed), serde_json::to_vec(self)) {
            (Ok(a), Ok(b)) => Sha256::digest(a) == Sha256::digest(b),
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KernelNativeSearchErrorV1 {
    InvalidAuthority,
    InvalidDecision,
    UnsupportedFlatActionContract,
    BudgetSmallerThanRootActionCount { budget: u32, root_actions: u32 },
    HiddenStateContract,
    StaleOrTamperedBinding,
    NonNaturalTerminal,
    CorruptTree,
}

impl fmt::Display for KernelNativeSearchErrorV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for KernelNativeSearchErrorV1 {}

impl From<FlatActionDecisionSliceErrorV1> for KernelNativeSearchErrorV1 {
    fn from(_: FlatActionDecisionSliceErrorV1) -> Self {
        Self::StaleOrTamperedBinding
    }
}

impl From<RlSessionError> for KernelNativeSearchErrorV1 {
    fn from(_: RlSessionError) -> Self {
        Self::StaleOrTamperedBinding
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KernelNativeSearchActionStatV1 {
    pub flat_action_index: u32,
    pub visits: u32,
    pub value_sum: i64,
    pub mean_value: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KernelNativeSearchDecisionV1 {
    pub selected_index: u32,
    pub transitions_used: u32,
    pub simulations: u32,
    pub tree_node_count: u32,
    pub root_action_stats: Vec<KernelNativeSearchActionStatV1>,
}

#[derive(Debug, Clone)]
pub struct KernelNativeSearchOpponentV1 {
    authority: KernelNativeSearchAuthorityV1,
}

impl KernelNativeSearchOpponentV1 {
    pub fn new(
        authority: KernelNativeSearchAuthorityV1,
    ) -> Result<Self, KernelNativeSearchErrorV1> {
        authority.validate()?;
        Ok(Self { authority })
    }

    pub fn authority(&self) -> &KernelNativeSearchAuthorityV1 {
        &self.authority
    }

    pub fn select_action(
        &self,
        session: &FastActorSessionV1,
        expected: FastActorDecisionV1,
    ) -> Result<KernelNativeSearchDecisionV1, KernelNativeSearchErrorV1> {
        self.select_action_with_budget_v1(
            session,
            expected,
            self.authority.transition_budget,
            self.authority.policy_step_depth_cap,
        )
    }

    fn select_action_with_budget_v1(
        &self,
        session: &FastActorSessionV1,
        expected: FastActorDecisionV1,
        transition_budget: u32,
        depth_cap: u16,
    ) -> Result<KernelNativeSearchDecisionV1, KernelNativeSearchErrorV1> {
        self.authority.validate()?;
        let FastActorResponseV1::Decision(live) = session.current_response() else {
            return Err(KernelNativeSearchErrorV1::InvalidDecision);
        };
        if live != expected
            || session.kernel_search_private_diagnostic_identity_v1()
                != self.authority.private_diagnostic_identity
        {
            return Err(KernelNativeSearchErrorV1::InvalidDecision);
        }
        if transition_budget < expected.legal_action_count {
            return Err(
                KernelNativeSearchErrorV1::BudgetSmallerThanRootActionCount {
                    budget: transition_budget,
                    root_actions: expected.legal_action_count,
                },
            );
        }

        let root_player = player_id_v1(expected.acting_player);
        let root_key = search_node_key_v1(session, expected, depth_cap)?;
        let mut tree = SearchTreeV1::new(root_key, root_player, expected.legal_action_count)?;
        let authority_digest = self.authority.digest()?;
        let authoritative_hash_before = session.privileged_core_environment_hash();
        let authoritative_response_before = session.current_response();
        let mut transitions_used = 0u32;
        let mut simulations = 0u32;

        while transitions_used < transition_budget {
            let simulation_seed = derive_simulation_seed_v1(
                authority_digest,
                expected,
                u64::from(simulations),
                root_player,
            );
            let mut sampled = session.kernel_search_redeterminized_clone_v1(simulation_seed)?;
            if search_node_key_v1(&sampled, expected, depth_cap)? != root_key {
                return Err(KernelNativeSearchErrorV1::HiddenStateContract);
            }
            let forced_root_action =
                (simulations < expected.legal_action_count).then_some(simulations);
            run_simulation_v1(
                &mut tree,
                &mut sampled,
                root_player,
                depth_cap,
                transition_budget,
                &mut transitions_used,
                forced_root_action,
            )?;
            simulations = simulations
                .checked_add(1)
                .ok_or(KernelNativeSearchErrorV1::CorruptTree)?;
        }

        if session.privileged_core_environment_hash() != authoritative_hash_before
            || session.current_response() != authoritative_response_before
        {
            return Err(KernelNativeSearchErrorV1::HiddenStateContract);
        }
        let root = tree
            .nodes
            .first()
            .ok_or(KernelNativeSearchErrorV1::CorruptTree)?;
        if root.actions.iter().any(|action| action.visits == 0) {
            return Err(KernelNativeSearchErrorV1::CorruptTree);
        }
        let selected_index = select_final_root_action_v1(&root.actions)?;
        let root_action_stats = root
            .actions
            .iter()
            .enumerate()
            .map(|(index, stat)| KernelNativeSearchActionStatV1 {
                flat_action_index: index as u32,
                visits: stat.visits,
                value_sum: stat.value_sum,
                mean_value: stat.mean(),
            })
            .collect();
        Ok(KernelNativeSearchDecisionV1 {
            selected_index,
            transitions_used,
            simulations,
            tree_node_count: u32::try_from(tree.nodes.len())
                .map_err(|_| KernelNativeSearchErrorV1::CorruptTree)?,
            root_action_stats,
        })
    }
}

/// `pub(crate)` (not private): `CLAUDE-MODEL-GUIDED-SEARCHER-DESIGN-V1.md`
/// Section 1.1 inherits this action-statistics record verbatim ("Truncating
/// signed integer division for means... no floating-point comparison
/// controls an action, anywhere, including in the new terms this design
/// adds"). `model_guided_search_core_v1` reuses this exact type as the
/// element type of its own node's action array, adding only a parallel
/// `prior: Vec<u32>` array alongside it (the new design's node needs a
/// cached PUCT prior per action, which this v1-owned type has no field for);
/// no field, method, or behavior here changes. This is visibility-only
/// sharing, not a fork: v1's own tests (this module's `#[cfg(test)]`)
/// exercise the identical type unchanged.
#[derive(Debug, Clone)]
pub(crate) struct SearchActionStatV1 {
    pub(crate) visits: u32,
    pub(crate) value_sum: i64,
    /// Multiple information-set successors may follow one action across
    /// determinizations. The vector is insertion-ordered and contains no
    /// duplicates; no hash map participates in search.
    pub(crate) child_nodes: Vec<usize>,
}

impl SearchActionStatV1 {
    pub(crate) fn mean(&self) -> i64 {
        if self.visits == 0 {
            0
        } else {
            self.value_sum / i64::from(self.visits)
        }
    }
}

#[derive(Debug, Clone)]
struct SearchNodeV1 {
    key: [u8; 32],
    actor: PlayerId,
    visits: u32,
    actions: Vec<SearchActionStatV1>,
}

impl SearchNodeV1 {
    fn new(
        key: [u8; 32],
        actor: PlayerId,
        action_count: u32,
    ) -> Result<Self, KernelNativeSearchErrorV1> {
        let action_count =
            usize::try_from(action_count).map_err(|_| KernelNativeSearchErrorV1::CorruptTree)?;
        if action_count == 0 {
            return Err(KernelNativeSearchErrorV1::CorruptTree);
        }
        Ok(Self {
            key,
            actor,
            visits: 0,
            actions: (0..action_count)
                .map(|_| SearchActionStatV1 {
                    visits: 0,
                    value_sum: 0,
                    child_nodes: Vec::new(),
                })
                .collect(),
        })
    }
}

#[derive(Debug, Clone)]
struct SearchTreeV1 {
    /// Vec-only storage is a cross-host determinism property, not an
    /// optimization choice. Node lookup is an ordered linear scan.
    nodes: Vec<SearchNodeV1>,
}

impl SearchTreeV1 {
    fn new(
        root_key: [u8; 32],
        root_actor: PlayerId,
        root_action_count: u32,
    ) -> Result<Self, KernelNativeSearchErrorV1> {
        Ok(Self {
            nodes: vec![SearchNodeV1::new(root_key, root_actor, root_action_count)?],
        })
    }

    fn find_node(&self, key: [u8; 32]) -> Option<usize> {
        self.nodes.iter().position(|node| node.key == key)
    }
}

fn run_simulation_v1(
    tree: &mut SearchTreeV1,
    session: &mut FastActorSessionV1,
    root_player: PlayerId,
    depth_cap: u16,
    transition_budget: u32,
    transitions_used: &mut u32,
    forced_root_action: Option<u32>,
) -> Result<(), KernelNativeSearchErrorV1> {
    let mut node_index = 0usize;
    let mut remaining_depth = depth_cap;
    let mut node_path = vec![0usize];
    let mut edge_path: Vec<(usize, usize)> = Vec::new();
    let value;

    loop {
        if remaining_depth == 0 || *transitions_used >= transition_budget {
            value = evaluate_state_v1(session.kernel_search_state_v1(), root_player);
            break;
        }
        let FastActorResponseV1::Decision(decision) = session.current_response() else {
            return Err(KernelNativeSearchErrorV1::CorruptTree);
        };
        let node = tree
            .nodes
            .get(node_index)
            .ok_or(KernelNativeSearchErrorV1::CorruptTree)?;
        let expected_key = search_node_key_v1(session, decision, remaining_depth)?;
        if node.key != expected_key || node.actor != player_id_v1(decision.acting_player) {
            return Err(KernelNativeSearchErrorV1::CorruptTree);
        }

        let action_index = if node_index == 0 {
            if let Some(forced) = forced_root_action {
                usize::try_from(forced).map_err(|_| KernelNativeSearchErrorV1::CorruptTree)?
            } else {
                select_tree_action_v1(node, root_player)?
            }
        } else {
            select_tree_action_v1(node, root_player)?
        };
        if action_index >= node.actions.len() {
            return Err(KernelNativeSearchErrorV1::CorruptTree);
        }
        let binding = session.native_full_trajectory_current_binding_v2(decision)?;
        let response =
            session.consume_current_flat_action_slice_v2(binding, action_index as u32)?;
        *transitions_used = transitions_used
            .checked_add(1)
            .ok_or(KernelNativeSearchErrorV1::CorruptTree)?;
        edge_path.push((node_index, action_index));
        remaining_depth -= 1;

        match response {
            FastActorResponseV1::Terminal(terminal) => {
                value = terminal_value_v1(session, &terminal, root_player)?;
                break;
            }
            FastActorResponseV1::Decision(next_decision) => {
                if remaining_depth == 0 || *transitions_used >= transition_budget {
                    value = evaluate_state_v1(session.kernel_search_state_v1(), root_player);
                    break;
                }
                let next_key = search_node_key_v1(session, next_decision, remaining_depth)?;
                let next_actor = player_id_v1(next_decision.acting_player);
                let next_index = if let Some(existing) = tree.find_node(next_key) {
                    existing
                } else {
                    let created = tree.nodes.len();
                    tree.nodes.push(SearchNodeV1::new(
                        next_key,
                        next_actor,
                        next_decision.legal_action_count,
                    )?);
                    tree.nodes[node_index].actions[action_index]
                        .child_nodes
                        .push(created);
                    node_path.push(created);
                    value = evaluate_state_v1(session.kernel_search_state_v1(), root_player);
                    break;
                };
                if !tree.nodes[node_index].actions[action_index]
                    .child_nodes
                    .contains(&next_index)
                {
                    tree.nodes[node_index].actions[action_index]
                        .child_nodes
                        .push(next_index);
                }
                node_index = next_index;
                node_path.push(next_index);

                // The initial coverage simulations spend exactly one
                // transition apiece, guaranteeing every root action a visit
                // whenever the tier passes the root-count preflight.
                if forced_root_action.is_some() {
                    value = evaluate_state_v1(session.kernel_search_state_v1(), root_player);
                    break;
                }
            }
        }
    }

    for index in node_path {
        let node = tree
            .nodes
            .get_mut(index)
            .ok_or(KernelNativeSearchErrorV1::CorruptTree)?;
        node.visits = node
            .visits
            .checked_add(1)
            .ok_or(KernelNativeSearchErrorV1::CorruptTree)?;
    }
    for (node_index, action_index) in edge_path {
        let action = tree
            .nodes
            .get_mut(node_index)
            .and_then(|node| node.actions.get_mut(action_index))
            .ok_or(KernelNativeSearchErrorV1::CorruptTree)?;
        action.visits = action
            .visits
            .checked_add(1)
            .ok_or(KernelNativeSearchErrorV1::CorruptTree)?;
        action.value_sum = action
            .value_sum
            .checked_add(i64::from(value))
            .ok_or(KernelNativeSearchErrorV1::CorruptTree)?;
    }
    Ok(())
}

fn select_tree_action_v1(
    node: &SearchNodeV1,
    root_player: PlayerId,
) -> Result<usize, KernelNativeSearchErrorV1> {
    if let Some(index) = node.actions.iter().position(|action| action.visits == 0) {
        return Ok(index);
    }
    let maximizing = node.actor == root_player;
    let mut best_index = 0usize;
    let mut best_score = if maximizing { i64::MIN } else { i64::MAX };
    for (index, action) in node.actions.iter().enumerate() {
        let bonus = i64::try_from(integer_ucb_bonus_v1(node.visits, action.visits))
            .map_err(|_| KernelNativeSearchErrorV1::CorruptTree)?;
        let score = if maximizing {
            action.mean().saturating_add(bonus)
        } else {
            action.mean().saturating_sub(bonus)
        };
        let better = if maximizing {
            score > best_score
        } else {
            score < best_score
        };
        if better {
            best_index = index;
            best_score = score;
        }
    }
    Ok(best_index)
}

/// `pub(crate)`, and takes the action-statistics slice directly rather than
/// `&SearchNodeV1`: `CLAUDE-MODEL-GUIDED-SEARCHER-DESIGN-V1.md` Section 1.1
/// inherits this final-selection rule verbatim ("the selected index is the
/// most visited root action, then the higher fixed-point mean, then the
/// lower stable index"), and `model_guided_search_core_v1`'s own root node
/// type is not `SearchNodeV1` (it carries an extra cached-prior field v1's
/// node has no room for). Narrowing the parameter to `&[SearchActionStatV1]`
/// lets both node types call the identical function with zero duplicated
/// logic; this is a signature-only change; the algorithm, tie rule, and every
/// existing call site's observable behavior are byte-for-byte unchanged (the
/// one production call site below is updated from `select_final_root_action_v1(root)`
/// to `select_final_root_action_v1(&root.actions)`, which reads the exact
/// same data).
pub(crate) fn select_final_root_action_v1(
    actions: &[SearchActionStatV1],
) -> Result<u32, KernelNativeSearchErrorV1> {
    let mut best = None::<(usize, u32, i64)>;
    for (index, action) in actions.iter().enumerate() {
        let candidate = (index, action.visits, action.mean());
        if best.is_none_or(|current| {
            candidate.1 > current.1 || (candidate.1 == current.1 && candidate.2 > current.2)
        }) {
            best = Some(candidate);
        }
    }
    u32::try_from(best.ok_or(KernelNativeSearchErrorV1::CorruptTree)?.0)
        .map_err(|_| KernelNativeSearchErrorV1::CorruptTree)
}

/// `pub(crate)`: `CLAUDE-MODEL-GUIDED-SEARCHER-DESIGN-V1.md` Section 1.2's
/// selection-bonus formula is defined as v1's own `bonus(a)` (this function,
/// unchanged) multiplied by the cached PUCT prior fraction:
/// `bonus_PUCT(a) = floor(bonus(a) * P_int(a) / 1,000,000)`. Reused directly
/// by `model_guided_search_core_v1` and by
/// `model_guided_search_prior_quantization_v1::puct_bonus_v1`'s own doc
/// comment, which names this exact function as the unchanged core it wraps.
/// Visibility-only change; the arithmetic is untouched.
pub(crate) fn integer_ucb_bonus_v1(parent_visits: u32, child_visits: u32) -> u64 {
    debug_assert!(child_visits > 0);
    let log_term = integer_ilog2_v1(u64::from(parent_visits) + 1) + 1;
    let radicand = 1_000_000u64
        .saturating_mul(log_term)
        .checked_div(u64::from(child_visits))
        .unwrap_or(0);
    1_414u64.saturating_mul(integer_isqrt_v1(radicand)) / 1_000
}

/// Genuine integer floor(log2(value)); no float conversion is permitted on
/// the search path.
fn integer_ilog2_v1(value: u64) -> u64 {
    debug_assert!(value > 0);
    63 - u64::from(value.leading_zeros())
}

/// Genuine integer floor(sqrt(value)) using the overflow-safe restoring
/// algorithm. No float conversion is permitted on the search path.
fn integer_isqrt_v1(mut value: u64) -> u64 {
    let mut result = 0u64;
    let mut bit = 1u64 << 62;
    while bit > value {
        bit >>= 2;
    }
    while bit != 0 {
        if value >= result + bit {
            value -= result + bit;
            result = (result >> 1) + bit;
        } else {
            result >>= 1;
        }
        bit >>= 2;
    }
    result
}

#[derive(Serialize)]
struct SearchNodeKeyEnvelopeV1<'a> {
    schema: &'static str,
    observation: &'a ObservationV5,
    ordered_legal_action_semantics: &'a [ActionSemanticV1],
    remaining_depth: u16,
}

/// `pub(crate)`: the tree-key computation is inherited verbatim by
/// `CLAUDE-MODEL-GUIDED-SEARCHER-DESIGN-V1.md` Section 1.1 ("The tree key
/// commits to the acting-player Observation V5 information set, ordered
/// legal-action semantics, and remaining depth"); `model_guided_search_authority_v1`
/// already reuses the `KERNEL_NATIVE_SEARCH_NODE_KEY_V1` string identity
/// unchanged, and `model_guided_search_core_v1` reuses this exact function
/// (not a re-derivation) so the two algorithms' tree keys are structurally
/// guaranteed identical, not just identically labeled. Visibility-only
/// change; behavior is untouched.
pub(crate) fn search_node_key_v1(
    session: &FastActorSessionV1,
    decision: FastActorDecisionV1,
    remaining_depth: u16,
) -> Result<[u8; 32], KernelNativeSearchErrorV1> {
    let observation = session.flat_policy_observation_v2(decision)?;
    let semantics = session
        .diagnostic_current_action_semantics()
        .ok_or(KernelNativeSearchErrorV1::InvalidDecision)?;
    let bytes = serde_json::to_vec(&SearchNodeKeyEnvelopeV1 {
        schema: KERNEL_NATIVE_SEARCH_NODE_KEY_V1,
        observation: &observation,
        ordered_legal_action_semantics: &semantics,
        remaining_depth,
    })
    .map_err(|_| KernelNativeSearchErrorV1::CorruptTree)?;
    Ok(Sha256::digest(bytes).into())
}

/// `pub(crate)`: the seed-domain-separation formula is inherited verbatim by
/// `CLAUDE-MODEL-GUIDED-SEARCHER-DESIGN-V1.md` Section 1.1 ("Seeds
/// domain-separated from (authority digest, episode id, physical decision
/// id, substep index, simulation ordinal, player to act)"). This function
/// already takes `authority_digest` as a plain parameter, decoupled from
/// which authority TYPE produced it, so `model_guided_search_core_v1` reuses
/// it directly by passing `ModelGuidedSearchAuthorityV1::digest()` in place
/// of v1's own digest; `model_guided_search_authority_v1` already requires
/// `seed_domain == KERNEL_NATIVE_SEARCH_SEED_DOMAIN_V1` byte-for-byte
/// (`validate`'s `InvalidSeedDomain` check), so this is the same domain
/// string, not a new one. Visibility-only change; behavior is untouched.
pub(crate) fn derive_simulation_seed_v1(
    authority_digest: [u8; 32],
    decision: FastActorDecisionV1,
    simulation_ordinal: u64,
    player: PlayerId,
) -> u64 {
    let mut hash = Sha256::new();
    hash.update(KERNEL_NATIVE_SEARCH_SEED_DOMAIN_V1.as_bytes());
    hash.update(authority_digest);
    hash.update(decision.episode_id.to_le_bytes());
    hash.update(decision.physical_decision_id.to_le_bytes());
    hash.update(decision.substep_index.to_le_bytes());
    hash.update(simulation_ordinal.to_le_bytes());
    hash.update([player.0]);
    let digest = hash.finalize();
    u64::from_le_bytes(digest[..8].try_into().expect("SHA-256 has eight bytes"))
}

fn terminal_value_v1(
    session: &FastActorSessionV1,
    terminal: &crate::rl_session::RlSessionTerminalV1,
    root_player: PlayerId,
) -> Result<i32, KernelNativeSearchErrorV1> {
    match terminal.terminal_classification {
        TerminalClassificationV1::Natural => {
            natural_terminal_value_v1(terminal.terminal_outcome, root_player)
        }
        TerminalClassificationV1::Truncated => Ok(evaluate_state_v1(
            session.kernel_search_state_v1(),
            root_player,
        )),
        TerminalClassificationV1::Halted => Err(KernelNativeSearchErrorV1::NonNaturalTerminal),
    }
}

/// `pub(crate)`: `CLAUDE-MODEL-GUIDED-SEARCHER-DESIGN-V1.md` Section 1.3,
/// site 1 ("Natural terminal... Unchanged: scored exactly +10,000, 0, or
/// -10,000 for root win, draw, or loss. The value head is never invoked
/// here"). `model_guided_search_core_v1` reuses this exact function for
/// natural terminals; visibility-only change, behavior untouched.
pub(crate) fn natural_terminal_value_v1(
    outcome: TerminalOutcomeV1,
    root_player: PlayerId,
) -> Result<i32, KernelNativeSearchErrorV1> {
    Ok(match outcome {
        TerminalOutcomeV1::P0Win if root_player == PlayerId::P0 => 10_000,
        TerminalOutcomeV1::P1Win if root_player == PlayerId::P1 => 10_000,
        TerminalOutcomeV1::P0Win | TerminalOutcomeV1::P1Win => -10_000,
        TerminalOutcomeV1::Draw => 0,
        TerminalOutcomeV1::Truncated | TerminalOutcomeV1::Halted => {
            return Err(KernelNativeSearchErrorV1::NonNaturalTerminal)
        }
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
struct EvaluatorWeightsV1 {
    life: i32,
    hand_card: i32,
    library_card: i32,
    mana_unit_or_untapped_source: i32,
    land: i32,
    noncreature_nonland_base: i32,
    permanent_mana_value: i32,
    creature_base: i32,
    creature_power: i32,
    creature_toughness: i32,
    flying: i32,
    double_strike: i32,
    first_strike: i32,
    deathtouch: i32,
    lifelink: i32,
    indestructible: i32,
    hexproof: i32,
    ward: i32,
    protection_from_monocolored: i32,
    menace: i32,
    trample: i32,
    vigilance: i32,
    reach: i32,
    haste: i32,
    defender: i32,
    initiative: i32,
    completed_dungeon: i32,
    dungeon_room_depth: i32,
    graveyard_castable: i32,
    clamp: i32,
}

const EVALUATOR_WEIGHTS_V1: EvaluatorWeightsV1 = EvaluatorWeightsV1 {
    life: 32,
    hand_card: 48,
    library_card: 4,
    mana_unit_or_untapped_source: 8,
    land: 24,
    noncreature_nonland_base: 48,
    permanent_mana_value: 16,
    creature_base: 96,
    creature_power: 24,
    creature_toughness: 16,
    flying: 32,
    double_strike: 32,
    first_strike: 16,
    deathtouch: 24,
    lifelink: 24,
    indestructible: 32,
    hexproof: 24,
    ward: 16,
    protection_from_monocolored: 24,
    menace: 16,
    trample: 16,
    vigilance: 12,
    reach: 8,
    haste: 8,
    defender: -24,
    initiative: 96,
    completed_dungeon: 64,
    dungeon_room_depth: 16,
    graveyard_castable: 16,
    clamp: 9_000,
};

fn evaluator_digest_hex_v1() -> String {
    let bytes = serde_json::to_vec(&EVALUATOR_WEIGHTS_V1)
        .expect("compile-bound evaluator weights serialize");
    hex_lower_v1(&Sha256::digest(bytes))
}

fn evaluate_state_v1(state: &GameState, root_player: PlayerId) -> i32 {
    let root = evaluate_player_v1(state, root_player);
    let opponent = evaluate_player_v1(state, root_player.opponent());
    (root - opponent).clamp(-EVALUATOR_WEIGHTS_V1.clamp, EVALUATOR_WEIGHTS_V1.clamp)
}

fn evaluate_player_v1(state: &GameState, player: PlayerId) -> i32 {
    let player_state = &state.players[player.index()];
    let mut total = player_state.life.saturating_mul(EVALUATOR_WEIGHTS_V1.life);
    total = total.saturating_add(
        i32::try_from(player_state.hand.len())
            .unwrap_or(i32::MAX)
            .saturating_mul(EVALUATOR_WEIGHTS_V1.hand_card),
    );
    total = total.saturating_add(
        i32::try_from(player_state.library.len())
            .unwrap_or(i32::MAX)
            .saturating_mul(EVALUATOR_WEIGHTS_V1.library_card),
    );
    let mana_units: u32 = player_state
        .mana_pool
        .iter()
        .map(|&mana| u32::from(mana))
        .sum();
    total = total.saturating_add(
        i32::try_from(mana_units)
            .unwrap_or(i32::MAX)
            .saturating_mul(EVALUATOR_WEIGHTS_V1.mana_unit_or_untapped_source),
    );

    for &id in &player_state.battlefield {
        let object = state.objects.get(id);
        if object.controller != player || object.zone != Zone::Battlefield {
            continue;
        }
        let definition = &CARD_DEFS[object.card_def as usize];
        let types = definition.types_for_face(object.v4.face_index);
        let is_land = types.contains(&CardType::Land);
        let is_creature = types.contains(&CardType::Creature);
        if is_land {
            total = total.saturating_add(EVALUATOR_WEIGHTS_V1.land);
        }
        if is_creature {
            total = total
                .saturating_add(EVALUATOR_WEIGHTS_V1.creature_base)
                .saturating_add(
                    i32::from(definition.mana_value)
                        .saturating_mul(EVALUATOR_WEIGHTS_V1.permanent_mana_value),
                )
                .saturating_add(
                    effective_power(state, id)
                        .max(0)
                        .saturating_mul(EVALUATOR_WEIGHTS_V1.creature_power),
                )
                .saturating_add(
                    effective_toughness(state, id)
                        .max(0)
                        .saturating_mul(EVALUATOR_WEIGHTS_V1.creature_toughness),
                );
            for (keyword, weight) in [
                (Keywords::FLYING, EVALUATOR_WEIGHTS_V1.flying),
                (Keywords::DOUBLE_STRIKE, EVALUATOR_WEIGHTS_V1.double_strike),
                (Keywords::FIRST_STRIKE, EVALUATOR_WEIGHTS_V1.first_strike),
                (Keywords::DEATHTOUCH, EVALUATOR_WEIGHTS_V1.deathtouch),
                (Keywords::LIFELINK, EVALUATOR_WEIGHTS_V1.lifelink),
                (
                    Keywords::INDESTRUCTIBLE,
                    EVALUATOR_WEIGHTS_V1.indestructible,
                ),
                (Keywords::HEXPROOF, EVALUATOR_WEIGHTS_V1.hexproof),
                (
                    Keywords::PROTECTION_FROM_MONOCOLORED,
                    EVALUATOR_WEIGHTS_V1.protection_from_monocolored,
                ),
                (Keywords::MENACE, EVALUATOR_WEIGHTS_V1.menace),
                (Keywords::TRAMPLE, EVALUATOR_WEIGHTS_V1.trample),
                (Keywords::VIGILANCE, EVALUATOR_WEIGHTS_V1.vigilance),
                (Keywords::REACH, EVALUATOR_WEIGHTS_V1.reach),
                (Keywords::HASTE, EVALUATOR_WEIGHTS_V1.haste),
                (Keywords::DEFENDER, EVALUATOR_WEIGHTS_V1.defender),
            ] {
                if has_effective_keyword(state, id, keyword) {
                    total = total.saturating_add(weight);
                }
            }
            if object.v4.ward_generic > 0 {
                total = total.saturating_add(EVALUATOR_WEIGHTS_V1.ward);
            }
        } else if !is_land {
            total = total
                .saturating_add(EVALUATOR_WEIGHTS_V1.noncreature_nonland_base)
                .saturating_add(
                    i32::from(definition.mana_value)
                        .saturating_mul(EVALUATOR_WEIGHTS_V1.permanent_mana_value),
                );
        }
        if !object.tapped && definition.has_mana_ability() {
            total = total.saturating_add(EVALUATOR_WEIGHTS_V1.mana_unit_or_untapped_source);
        }
    }

    if state.initiative == Some(player) {
        total = total.saturating_add(EVALUATOR_WEIGHTS_V1.initiative);
    }
    total = total.saturating_add(
        i32::try_from(player_state.dungeon.completed_dungeons.len())
            .unwrap_or(i32::MAX)
            .saturating_mul(EVALUATOR_WEIGHTS_V1.completed_dungeon),
    );
    if let Some(room_id) = player_state.dungeon.room_id {
        total = total.saturating_add(
            dungeon_room_depth_v1(room_id).saturating_mul(EVALUATOR_WEIGHTS_V1.dungeon_room_depth),
        );
    }
    for &id in &player_state.graveyard {
        let object = state.objects.get(id);
        let definition = &CARD_DEFS[object.card_def as usize];
        let supported_graveyard_method = definition.flashback.is_some()
            || definition.escape.is_some()
            || definition
                .activated_abilities
                .iter()
                .any(|ability| ability.activation_zone == Zone::Graveyard);
        if supported_graveyard_method {
            total = total.saturating_add(EVALUATOR_WEIGHTS_V1.graveyard_castable);
        }
    }
    total
}

fn dungeon_room_depth_v1(room_id: u16) -> i32 {
    match UndercityRoomV1::from_stable_id(room_id) {
        Some(UndercityRoomV1::SecretEntrance) => 1,
        Some(UndercityRoomV1::Forge | UndercityRoomV1::LostWell) => 2,
        Some(UndercityRoomV1::Trap | UndercityRoomV1::Arena) => 3,
        Some(UndercityRoomV1::Stash | UndercityRoomV1::Archives | UndercityRoomV1::Catacombs) => 4,
        Some(UndercityRoomV1::ThroneOfTheDeadThree) => 5,
        None => 0,
    }
}

pub(crate) fn redeterminize_hidden_zones_v1(
    state: &mut GameState,
    root_actor: PlayerId,
    simulation_seed: u64,
) -> Result<(), KernelNativeSearchErrorV1> {
    if root_actor != PlayerId::P0 && root_actor != PlayerId::P1 {
        return Err(KernelNativeSearchErrorV1::HiddenStateContract);
    }
    let mut rng = SplitMix64::seed(simulation_seed);
    for owner in [PlayerId::P0, PlayerId::P1] {
        let mut unknown_slots = Vec::new();
        if owner != root_actor {
            for &id in &state.players[owner.index()].hand {
                let object = state.objects.get(id);
                let known = state
                    .known_hand_cards(root_actor, owner)
                    .iter()
                    .any(|entry| {
                        entry.object == id && entry.zone_change_count == object.zone_change_count
                    });
                if !known {
                    unknown_slots.push(id);
                }
            }
        }
        for (position, &id) in state.players[owner.index()].library.iter().enumerate() {
            let object = state.objects.get(id);
            let known = state
                .known_library_cards(root_actor, owner)
                .iter()
                .any(|entry| {
                    usize::try_from(entry.position).ok() == Some(position)
                        && entry.object == id
                        && entry.zone_change_count == object.zone_change_count
                });
            if !known {
                unknown_slots.push(id);
            }
        }

        let mut definitions: Vec<u16> = unknown_slots
            .iter()
            .map(|&id| state.objects.get(id).card_def)
            .collect();
        definitions.sort_unstable();
        for index in (1..definitions.len()).rev() {
            let bound = u64::try_from(index + 1)
                .map_err(|_| KernelNativeSearchErrorV1::HiddenStateContract)?;
            let swap = usize::try_from(unbiased_index_v1(&mut rng, bound))
                .map_err(|_| KernelNativeSearchErrorV1::HiddenStateContract)?;
            definitions.swap(index, swap);
        }
        for (id, card_def) in unknown_slots.into_iter().zip(definitions) {
            let object = state.objects.get_mut(id);
            if !matches!(object.zone, Zone::Hand | Zone::Library)
                || object.spell_copy_origin.is_some()
                || !object.attachments.is_empty()
            {
                return Err(KernelNativeSearchErrorV1::HiddenStateContract);
            }
            object.card_def = card_def;
            object.name = CARD_DEFS[card_def as usize].name.to_string();
            object.v4 = ObjectStateV4::from_card_def(card_def);
        }
    }
    Ok(())
}

fn unbiased_index_v1(rng: &mut SplitMix64, bound: u64) -> u64 {
    debug_assert!(bound > 0);
    let threshold = bound.wrapping_neg() % bound;
    loop {
        let value = rng.next_u64();
        if value >= threshold {
            return value % bound;
        }
    }
}

/// `pub(crate)`: trivial `PlayerSeatV1` -> `PlayerId` mapping, reused by
/// `model_guided_search_core_v1` so both algorithms agree on the identical
/// mapping by construction rather than by copy. Visibility-only change.
pub(crate) fn player_id_v1(player: PlayerSeatV1) -> PlayerId {
    match player {
        PlayerSeatV1::P0 => PlayerId::P0,
        PlayerSeatV1::P1 => PlayerId::P1,
    }
}

fn hex_lower_v1(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn is_lower_hex_v1(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_decks::RUNTIME_DECKS;

    fn authority_v1(tier: KernelNativeSearchTierV1) -> KernelNativeSearchAuthorityV1 {
        KernelNativeSearchAuthorityV1::current(
            tier,
            KERNEL_NATIVE_SEARCH_AUTHORIZED_SEEDS_V1[0],
            crate::state::DIAGNOSTIC_STATE_HASH_ALGORITHM,
        )
        .unwrap()
    }

    fn v2_session_v1(p0: &str, p1: &str, seed: u64) -> FastActorSessionV1 {
        FastActorSessionV1::reset_with_decks_and_limits_flat_action_v2(
            50_001,
            seed,
            512,
            65_536,
            [p0.to_string(), p1.to_string()],
        )
        .unwrap()
    }

    #[test]
    fn integer_isqrt_and_ilog2_are_exact_without_float_shortcuts() {
        for value in 1u64..=100_000 {
            let root = integer_isqrt_v1(value);
            assert!(root <= value / root);
            assert!(root + 1 > value / (root + 1));
            let log = integer_ilog2_v1(value);
            assert!(1u64 << log <= value);
            if log < 63 {
                assert!(value < (1u64 << (log + 1)));
            }
        }
        assert_eq!(integer_isqrt_v1(u64::MAX), u32::MAX as u64);
    }

    #[test]
    fn authority_record_checks_compile_bound_and_embedded_seed_separately() {
        assert_eq!(
            KERNEL_NATIVE_SEARCH_AUTHORIZED_SEEDS_V1,
            [1_987_001, 1_988_001, 1_989_001, 1_990_001]
        );
        let authority = authority_v1(KernelNativeSearchTierV1::T512);
        assert!(authority.validate().is_ok());
        assert_eq!(authority.digest().unwrap(), authority.digest().unwrap());
        let mut wrong = authority.clone();
        wrong.action_seed = 1_987_000;
        assert_eq!(
            wrong.validate(),
            Err(KernelNativeSearchErrorV1::InvalidAuthority)
        );
        let mut wrong = authority;
        wrong.transition_budget += 1;
        assert_eq!(
            wrong.validate(),
            Err(KernelNativeSearchErrorV1::InvalidAuthority)
        );
        assert_eq!(KernelNativeSearchTierV1::T512.transition_budget(), 512);
        assert_eq!(KernelNativeSearchTierV1::T2048.transition_budget(), 2_048);
        assert_eq!(KernelNativeSearchTierV1::T8192.transition_budget(), 8_192);
        assert_eq!(KernelNativeSearchTierV1::T32768.transition_budget(), 32_768);
    }

    /// `matches_fresh_reconstruction_v1` (COUNTERSIGN amendment 3's second,
    /// genuinely distinct runtime-boundary check, native_checkpoint_runner_v1.rs)
    /// accepts a genuine record and rejects a directly-tampered one, for
    /// every one of the four record-shaped fields it can meaningfully
    /// distinguish: `schema`, `algorithm_identity`, `seed_domain`, and
    /// `evaluator_sha256`. (`action_seed`, `tier`, and
    /// `private_diagnostic_identity` are the three inputs reconstruction
    /// itself is keyed on, so tampering those and then reconstructing from
    /// the TAMPERED values would just reproduce a different, still-self-
    /// consistent record; those three are exactly what `.validate()`'s
    /// explicit allowlist/consistency checks cover instead. This test
    /// deliberately covers fields validate() checks by simple equality
    /// against a live/const value, proving reconstruction independently
    /// reaches the same verdict via a different mechanism.)
    #[test]
    fn fresh_reconstruction_check_accepts_genuine_and_rejects_direct_field_tamper() {
        let authority = authority_v1(KernelNativeSearchTierV1::T512);
        assert!(authority.matches_fresh_reconstruction_v1());

        macro_rules! assert_tamper_rejected {
            ($field:ident, $value:expr) => {{
                let mut tampered = authority.clone();
                tampered.$field = $value;
                assert!(
                    !tampered.matches_fresh_reconstruction_v1(),
                    "tampering {} must be caught by reconstruction",
                    stringify!($field)
                );
            }};
        }
        assert_tamper_rejected!(schema, "wrong-schema/v1".to_owned());
        assert_tamper_rejected!(algorithm_identity, "wrong-algorithm/v1".to_owned());
        assert_tamper_rejected!(seed_domain, "wrong-seed-domain/v1".to_owned());
        assert_tamper_rejected!(evaluator_sha256, "0".repeat(64));
    }

    #[test]
    fn evaluator_digest_binds_every_weight_and_values_are_antisymmetric() {
        let baseline = evaluator_digest_hex_v1();
        let mut fields = Vec::new();
        macro_rules! perturb {
            ($field:ident) => {{
                let mut changed = EVALUATOR_WEIGHTS_V1;
                changed.$field += 1;
                fields.push(hex_lower_v1(&Sha256::digest(
                    serde_json::to_vec(&changed).unwrap(),
                )));
            }};
        }
        perturb!(life);
        perturb!(hand_card);
        perturb!(library_card);
        perturb!(mana_unit_or_untapped_source);
        perturb!(land);
        perturb!(noncreature_nonland_base);
        perturb!(permanent_mana_value);
        perturb!(creature_base);
        perturb!(creature_power);
        perturb!(creature_toughness);
        perturb!(flying);
        perturb!(double_strike);
        perturb!(first_strike);
        perturb!(deathtouch);
        perturb!(lifelink);
        perturb!(indestructible);
        perturb!(hexproof);
        perturb!(ward);
        perturb!(protection_from_monocolored);
        perturb!(menace);
        perturb!(trample);
        perturb!(vigilance);
        perturb!(reach);
        perturb!(haste);
        perturb!(defender);
        perturb!(initiative);
        perturb!(completed_dungeon);
        perturb!(dungeon_room_depth);
        perturb!(graveyard_castable);
        perturb!(clamp);
        assert_eq!(fields.len(), 30);
        assert!(fields.iter().all(|digest| digest != &baseline));

        let session = v2_session_v1("Rally", "Burn", 7);
        let p0 = evaluate_state_v1(session.kernel_search_state_v1(), PlayerId::P0);
        let p1 = evaluate_state_v1(session.kernel_search_state_v1(), PlayerId::P1);
        assert_eq!(p0, -p1);
        assert!((-9_000..=9_000).contains(&p0));

        assert_eq!(
            natural_terminal_value_v1(TerminalOutcomeV1::P0Win, PlayerId::P0),
            Ok(10_000)
        );
        assert_eq!(
            natural_terminal_value_v1(TerminalOutcomeV1::P0Win, PlayerId::P1),
            Ok(-10_000)
        );
        assert_eq!(
            natural_terminal_value_v1(TerminalOutcomeV1::P1Win, PlayerId::P1),
            Ok(10_000)
        );
        assert_eq!(
            natural_terminal_value_v1(TerminalOutcomeV1::Draw, PlayerId::P0),
            Ok(0)
        );
        assert_eq!(
            natural_terminal_value_v1(TerminalOutcomeV1::Truncated, PlayerId::P0),
            Err(KernelNativeSearchErrorV1::NonNaturalTerminal)
        );

        let mut changed = session.kernel_search_state_v1().clone();
        let baseline = evaluate_state_v1(&changed, PlayerId::P0);
        changed.players[0].life += 1;
        assert_eq!(evaluate_state_v1(&changed, PlayerId::P0) - baseline, 32);
        changed.players[0].life -= 1;
        changed.players[0].mana_pool[0] += 1;
        assert_eq!(evaluate_state_v1(&changed, PlayerId::P0) - baseline, 8);
        changed.players[0].mana_pool[0] -= 1;
        changed.initiative = Some(PlayerId::P0);
        assert_eq!(evaluate_state_v1(&changed, PlayerId::P0) - baseline, 96);
    }

    #[test]
    fn all_nine_decks_redeterminize_transactionally_without_root_drift() {
        for p0 in RUNTIME_DECKS {
            for p1 in RUNTIME_DECKS {
                let session = v2_session_v1(p0.id, p1.id, 70_000 + p0.canonical_pool_order as u64);
                let response = session.current_response();
                let hash = session.privileged_core_environment_hash();
                let sampled = session
                    .kernel_search_redeterminized_clone_v1(90_000 + p1.canonical_pool_order as u64)
                    .unwrap();
                assert_eq!(
                    session.current_response(),
                    response,
                    "{} / {}",
                    p0.id,
                    p1.id
                );
                assert_eq!(session.privileged_core_environment_hash(), hash);
                assert_eq!(
                    sampled.current_response(),
                    response,
                    "{} / {}",
                    p0.id,
                    p1.id
                );
                let FastActorResponseV1::Decision(decision) = response else {
                    panic!("reset terminated")
                };
                let searcher =
                    KernelNativeSearchOpponentV1::new(authority_v1(KernelNativeSearchTierV1::T512))
                        .unwrap();
                let searched = searcher
                    .select_action_with_budget_v1(
                        &session,
                        decision,
                        decision.legal_action_count,
                        1,
                    )
                    .unwrap();
                assert_eq!(searched.transitions_used, decision.legal_action_count);
                assert_eq!(searched.tree_node_count, 1, "{} / {}", p0.id, p1.id);
                assert!(searched
                    .root_action_stats
                    .iter()
                    .all(|stat| stat.visits == 1));
            }
        }

        for deck in RUNTIME_DECKS {
            let session = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v2_with_starting_player_v1(
                50_002,
                80_000 + u64::from(deck.canonical_pool_order),
                512,
                65_536,
                [deck.id.to_string(), deck.id.to_string()],
                PlayerId::P1,
            )
            .unwrap();
            let FastActorResponseV1::Decision(decision) = session.current_response() else {
                panic!("P1 reset terminated")
            };
            assert_eq!(decision.acting_player, PlayerSeatV1::P1);
            session
                .kernel_search_redeterminized_clone_v1(
                    190_000 + u64::from(deck.canonical_pool_order),
                )
                .unwrap();
        }
    }

    #[test]
    fn unknown_arrangement_is_canonicalized_before_sampling_and_known_cards_lock() {
        let mut a = v2_session_v1("Rally", "Burn", 81_001);
        let mut b = a.clone();
        let actor = player_id_v1(match a.current_response() {
            FastActorResponseV1::Decision(decision) => decision.acting_player,
            FastActorResponseV1::Terminal(_) => panic!("reset terminated"),
        });
        let opponent = actor.opponent();
        let unknown_hand = a.kernel_search_state_v1().players[opponent.index()].hand[0];
        let unknown_library = a.kernel_search_state_v1().players[opponent.index()].library[0];
        let a_hand_def = a
            .kernel_search_state_v1()
            .objects
            .get(unknown_hand)
            .card_def;
        let a_library_def = a
            .kernel_search_state_v1()
            .objects
            .get(unknown_library)
            .card_def;
        assert_ne!(
            a_hand_def, a_library_def,
            "the arrangement-independence witness must swap distinct definitions"
        );
        {
            let state = b.kernel_search_state_mut_for_test_v1();
            state.objects.get_mut(unknown_hand).card_def = a_library_def;
            state.objects.get_mut(unknown_hand).name =
                CARD_DEFS[a_library_def as usize].name.to_string();
            state.objects.get_mut(unknown_hand).v4 = ObjectStateV4::from_card_def(a_library_def);
            state.objects.get_mut(unknown_library).card_def = a_hand_def;
            state.objects.get_mut(unknown_library).name =
                CARD_DEFS[a_hand_def as usize].name.to_string();
            state.objects.get_mut(unknown_library).v4 = ObjectStateV4::from_card_def(a_hand_def);
        }
        let sampled_a = a.kernel_search_redeterminized_clone_v1(123_456).unwrap();
        let sampled_b = b.kernel_search_redeterminized_clone_v1(123_456).unwrap();
        let sampled_defs = |session: &FastActorSessionV1| {
            let state = session.kernel_search_state_v1();
            state.players[opponent.index()]
                .hand
                .iter()
                .chain(&state.players[opponent.index()].library)
                .map(|&id| state.objects.get(id).card_def)
                .collect::<Vec<_>>()
        };
        assert_eq!(sampled_defs(&sampled_a), sampled_defs(&sampled_b));
        let FastActorResponseV1::Decision(decision_a) = a.current_response() else {
            panic!("reset terminated")
        };
        let FastActorResponseV1::Decision(decision_b) = b.current_response() else {
            panic!("reset terminated")
        };
        assert_eq!(decision_a, decision_b);
        let searcher =
            KernelNativeSearchOpponentV1::new(authority_v1(KernelNativeSearchTierV1::T512))
                .unwrap();
        assert_eq!(
            searcher
                .select_action_with_budget_v1(&a, decision_a, 64, 8)
                .unwrap(),
            searcher
                .select_action_with_budget_v1(&b, decision_b, 64, 8)
                .unwrap()
        );

        let known = a.kernel_search_state_v1().players[opponent.index()].hand[1];
        let known_definition = a.kernel_search_state_v1().objects.get(known).card_def;
        a.kernel_search_state_mut_for_test_v1()
            .reveal_hand_card(actor, opponent, known)
            .unwrap();
        let known_library = a.kernel_search_state_v1().players[opponent.index()].library[2];
        let known_library_definition = a
            .kernel_search_state_v1()
            .objects
            .get(known_library)
            .card_def;
        a.kernel_search_state_mut_for_test_v1()
            .reveal_library_top(actor, opponent, 3);
        let randomness_before = serde_json::to_value(a.kernel_search_state_v1()).unwrap();
        let sampled = a.kernel_search_redeterminized_clone_v1(987_654).unwrap();
        assert_eq!(
            sampled.kernel_search_state_v1().objects.get(known).card_def,
            known_definition
        );
        assert_eq!(
            sampled
                .kernel_search_state_v1()
                .objects
                .get(known_library)
                .card_def,
            known_library_definition
        );
        let randomness_after = serde_json::to_value(sampled.kernel_search_state_v1()).unwrap();
        for key in ["rng", "environment_randomization_v2"] {
            assert_eq!(randomness_before.get(key), randomness_after.get(key));
        }
    }

    #[test]
    fn redeterminized_clone_rejects_a_genuine_post_redetermination_hand_corruption() {
        // Every other tamper test in this module attacks the authority level
        // (seed, budget, revision). This test manufactures a real mismatch
        // inside the boundary's own rebuilt observation/semantics so the
        // HiddenStateContract rejection in
        // `kernel_search_redeterminized_clone_v1` actually fires, using the
        // cfg(test)-only corruption hook to perturb one card identity in the
        // acting player's own hand after the honest hidden-zone resample
        // (`redeterminize_hidden_zones_v1`) has already run. The acting
        // player's own hand is never touched by that resample, so this
        // corruption is not something legitimate redetermination could ever
        // produce; the boundary must still catch it before returning the
        // sample.
        let session = v2_session_v1("Rally", "Burn", 93_001);
        let response = session.current_response();
        let hash = session.privileged_core_environment_hash();

        let corrupted =
            session.kernel_search_redeterminized_clone_for_test_v1(123_789, |state, actor| {
                let &hand_object = state.players[actor.index()]
                    .hand
                    .first()
                    .expect("acting player holds at least one card at game start");
                let object = state.objects.get_mut(hand_object);
                let corrupted_def = (object.card_def + 1) % CARD_DEFS.len() as u16;
                object.card_def = corrupted_def;
                object.name = CARD_DEFS[corrupted_def as usize].name.to_string();
                object.v4 = ObjectStateV4::from_card_def(corrupted_def);
            });
        match corrupted {
            Err(err) => assert_eq!(err, KernelNativeSearchErrorV1::HiddenStateContract),
            Ok(_) => {
                panic!("a post-redetermination hand corruption must be rejected, not admitted")
            }
        }

        // The rejected sample must never have touched the authoritative
        // episode: the boundary operates on `self.clone()`.
        assert_eq!(session.current_response(), response);
        assert_eq!(session.privileged_core_environment_hash(), hash);
    }

    #[test]
    fn fixed_budget_search_is_repeatable_and_never_mutates_the_episode() {
        let session = v2_session_v1("Rally", "Burn", 91_001);
        let FastActorResponseV1::Decision(decision) = session.current_response() else {
            panic!("reset terminated")
        };
        let hash = session.privileged_core_environment_hash();
        let searcher =
            KernelNativeSearchOpponentV1::new(authority_v1(KernelNativeSearchTierV1::T512))
                .unwrap();
        let first = searcher
            .select_action_with_budget_v1(&session, decision, 64, 8)
            .unwrap();
        let second = searcher
            .select_action_with_budget_v1(&session, decision, 64, 8)
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(first.transitions_used, 64);
        assert!(first.root_action_stats.iter().all(|stat| stat.visits > 0));
        assert_eq!(session.privileged_core_environment_hash(), hash);
        assert_eq!(
            session.current_response(),
            FastActorResponseV1::Decision(decision)
        );
        let tier_result = searcher.select_action(&session, decision).unwrap();
        assert_eq!(tier_result.transitions_used, 512);
    }

    #[test]
    fn budget_smaller_than_root_count_and_tampered_decisions_fail_closed() {
        let session = v2_session_v1("Rally", "Burn", 92_001);
        let FastActorResponseV1::Decision(decision) = session.current_response() else {
            panic!("reset terminated")
        };
        let searcher =
            KernelNativeSearchOpponentV1::new(authority_v1(KernelNativeSearchTierV1::T512))
                .unwrap();
        assert_eq!(
            searcher.select_action_with_budget_v1(
                &session,
                decision,
                decision.legal_action_count - 1,
                8,
            ),
            Err(
                KernelNativeSearchErrorV1::BudgetSmallerThanRootActionCount {
                    budget: decision.legal_action_count - 1,
                    root_actions: decision.legal_action_count,
                }
            )
        );
        let mut tampered = decision;
        tampered.environment_revision += 1;
        assert_eq!(
            searcher.select_action_with_budget_v1(&session, tampered, 64, 8),
            Err(KernelNativeSearchErrorV1::InvalidDecision)
        );
    }

    #[test]
    fn vec_tree_storage_is_flat_index_aligned_and_ties_choose_lower_index() {
        let key = [7; 32];
        let mut node = SearchNodeV1::new(key, PlayerId::P0, 3).unwrap();
        assert_eq!(node.actions.len(), 3);
        for action in &mut node.actions {
            action.visits = 4;
            action.value_sum = 20;
        }
        node.visits = 12;
        assert_eq!(select_tree_action_v1(&node, PlayerId::P0).unwrap(), 0);
        assert_eq!(select_tree_action_v1(&node, PlayerId::P1).unwrap(), 0);
        assert_eq!(select_final_root_action_v1(&node.actions).unwrap(), 0);
        let tree = SearchTreeV1::new(key, PlayerId::P0, 3).unwrap();
        assert_eq!(tree.find_node(key), Some(0));
        assert_eq!(tree.find_node([8; 32]), None);
    }

    #[test]
    fn diagnostic_tier_allowlist_covers_every_tier_exactly_once_in_design_order() {
        assert_eq!(
            KERNEL_NATIVE_SEARCH_DIAGNOSTIC_TIER_ALLOWLIST_V1,
            [
                KernelNativeSearchTierV1::T512,
                KernelNativeSearchTierV1::T2048,
                KernelNativeSearchTierV1::T8192,
                KernelNativeSearchTierV1::T32768,
            ]
        );
        let mut budgets: Vec<u32> = KERNEL_NATIVE_SEARCH_DIAGNOSTIC_TIER_ALLOWLIST_V1
            .iter()
            .map(|tier| tier.transition_budget())
            .collect();
        budgets.sort_unstable();
        budgets.dedup();
        assert_eq!(
            budgets.len(),
            4,
            "every registered tier must be distinct and monotone by budget"
        );
    }

    /// Registration-only validation of the "Rust mixture-seed whitelist"
    /// layer (design "Calibration after implementation": the four base
    /// seeds 1,987,001 / 1,988,001 / 1,989,001 / 1,990,001, pair indices
    /// `0..15` for the throughput screen and `0..255` for a matched panel).
    /// This checks the ENV-DRIVEN VALIDATION SURFACE ONLY (tier string,
    /// pair-count bound, and eval-seed allowlist membership) that a future
    /// calibration launcher resolves against; it does not run a search, a
    /// rollout, or any panel, and it is `#[ignore]`-gated so `cargo test`
    /// never executes it by default. Running an actual calibration panel is
    /// explicitly out of stage-2 scope (KERNEL-NATIVE-SEARCH-OPPONENT-V1-
    /// DESIGN.md "Calibration after implementation" is a separate,
    /// coordinator-run phase).
    #[test]
    #[ignore]
    fn kernel_native_search_diagnostic_env_surface_is_registered_and_fails_closed() {
        fn required_env_v1(name: &str) -> String {
            std::env::var(name).unwrap_or_else(|_| panic!("{name} must be set"))
        }
        let tier_name = required_env_v1("KERNEL_SEARCH_DIAGNOSTIC_TIER");
        let tier = match tier_name.as_str() {
            "T512" => KernelNativeSearchTierV1::T512,
            "T2048" => KernelNativeSearchTierV1::T2048,
            "T8192" => KernelNativeSearchTierV1::T8192,
            "T32768" => KernelNativeSearchTierV1::T32768,
            _ => panic!("KERNEL_SEARCH_DIAGNOSTIC_TIER must name one of the four registered tiers"),
        };
        assert!(
            KERNEL_NATIVE_SEARCH_DIAGNOSTIC_TIER_ALLOWLIST_V1.contains(&tier),
            "diagnostic tier must be on the registered allowlist"
        );
        let pairs: u64 = required_env_v1("KERNEL_SEARCH_DIAGNOSTIC_PAIRS")
            .parse()
            .expect("KERNEL_SEARCH_DIAGNOSTIC_PAIRS");
        assert!(
            (1..=256).contains(&pairs),
            "diagnostic panel must be the 16-pair throughput screen or the 256-pair matched panel, or a smaller smoke pair count"
        );
        let eval_seed: u64 = required_env_v1("KERNEL_SEARCH_DIAGNOSTIC_EVAL_SEED")
            .parse()
            .expect("KERNEL_SEARCH_DIAGNOSTIC_EVAL_SEED");
        assert!(
            KERNEL_NATIVE_SEARCH_AUTHORIZED_SEEDS_V1.contains(&eval_seed),
            "diagnostic eval seed must be one of the four countersigned calibration seeds"
        );
    }
}
