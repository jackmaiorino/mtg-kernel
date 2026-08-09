//! Deterministic, versioned sideboarding over an exact registered 60/15 deck.
//!
//! This is a sibling of the frozen BO1 runtime-deck catalog. It does not alter
//! `RuntimeDeckDefinition`, game state, reset, or evaluation schemas. A plan is
//! always applied to the registered configuration, rather than cumulatively to
//! the preceding post-board configuration. That makes every game-indexed plan
//! independently reproducible and preserves the registered 75-card multiset.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

pub const REGISTERED_MAINBOARD_SIZE_V1: usize = 60;
pub const REGISTERED_SIDEBOARD_SIZE_V1: usize = 15;
pub const REGISTERED_DECK_SIZE_V1: usize =
    REGISTERED_MAINBOARD_SIZE_V1 + REGISTERED_SIDEBOARD_SIZE_V1;
pub const SIDEBOARD_POLICY_SCHEMA_V1: &str = "kernel_pauper_sideboard_policy/v1";
pub const SIDEBOARD_RECEIPT_SCHEMA_V1: &str = "kernel_sideboard_receipt/v1";
pub const PAUPER_POOL_SCHEMA_V1: &str = "kernel_pauper_pool/v1";
pub const SIDEBOARD_POLICY_MATCHUP_COVERAGE_V1: &str = "all_registered_deck_pairs";
pub const PAUPER_SIDEBOARD_POLICY_JSON_V1: &str =
    include_str!("../../data/pauper_sideboard_policy_v1.json");
pub const PAUPER_POOL_JSON_V1: &str = include_str!("../../data/pauper_pool_v1.json");

const DECK_CONFIGURATION_HASH_DOMAIN_V1: &str = "kernel-deck-configuration-sha256/v1";
const REGISTERED_DECK_HASH_DOMAIN_V1: &str = "kernel-registered-75-sha256/v1";
const SIDEBOARD_POLICY_HASH_DOMAIN_V1: &str = "kernel-sideboard-policy-sha256/v1";
const SIDEBOARD_RECEIPT_HASH_DOMAIN_V1: &str = "kernel-sideboard-receipt-sha256/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardCountV1 {
    pub card_id: u16,
    pub count: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeckConfigurationV1 {
    mainboard: Vec<u16>,
    sideboard: Vec<u16>,
}

impl DeckConfigurationV1 {
    pub fn new_exact_v1(
        mut mainboard: Vec<u16>,
        mut sideboard: Vec<u16>,
    ) -> Result<Self, SideboardErrorV1> {
        if mainboard.len() != REGISTERED_MAINBOARD_SIZE_V1 {
            return Err(SideboardErrorV1::WrongMainboardSize {
                actual: mainboard.len(),
            });
        }
        if sideboard.len() != REGISTERED_SIDEBOARD_SIZE_V1 {
            return Err(SideboardErrorV1::WrongSideboardSize {
                actual: sideboard.len(),
            });
        }
        mainboard.sort_unstable();
        sideboard.sort_unstable();
        Ok(Self {
            mainboard,
            sideboard,
        })
    }

    pub fn mainboard(&self) -> &[u16] {
        &self.mainboard
    }

    pub fn sideboard(&self) -> &[u16] {
        &self.sideboard
    }

    pub fn mainboard_sha256_v1(&self) -> [u8; 32] {
        configuration_zone_sha256_v1(b"mainboard", &self.mainboard)
    }

    pub fn sideboard_sha256_v1(&self) -> [u8; 32] {
        configuration_zone_sha256_v1(b"sideboard", &self.sideboard)
    }

    pub fn combined_card_counts_v1(&self) -> Vec<CardCountV1> {
        let mut counts = counts_from_cards_v1(&self.mainboard);
        for card_id in &self.sideboard {
            *counts.entry(*card_id).or_insert(0) += 1;
        }
        counts_to_rows_v1(&counts)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredDeckV1 {
    deck_id: String,
    configuration: DeckConfigurationV1,
}

impl RegisteredDeckV1 {
    pub fn new_exact_v1(
        deck_id: impl Into<String>,
        mainboard: Vec<u16>,
        sideboard: Vec<u16>,
    ) -> Result<Self, SideboardErrorV1> {
        let deck_id = deck_id.into();
        validate_identifier_v1("deck_id", &deck_id)?;
        Ok(Self {
            deck_id,
            configuration: DeckConfigurationV1::new_exact_v1(mainboard, sideboard)?,
        })
    }

    /// Admits a registered deck for executable BO3 play. The structural
    /// constructor above remains available for isolated exchange-policy
    /// tests, while this constructor additionally rejects unknown ids,
    /// tokens, and any card whose rules implementation is not fully
    /// supported. This is the sideboard sibling of BO1 deck preflight.
    pub fn new_fully_supported_v1(
        deck_id: impl Into<String>,
        mainboard: Vec<u16>,
        sideboard: Vec<u16>,
    ) -> Result<Self, SideboardErrorV1> {
        let deck = Self::new_exact_v1(deck_id, mainboard, sideboard)?;
        validate_registered_cards_v1(
            deck.configuration.mainboard(),
            SideboardZoneV1::RegisteredMainboard,
        )?;
        validate_registered_cards_v1(
            deck.configuration.sideboard(),
            SideboardZoneV1::RegisteredSideboard,
        )?;
        Ok(deck)
    }

    pub fn deck_id(&self) -> &str {
        &self.deck_id
    }

    pub fn registered_configuration(&self) -> &DeckConfigurationV1 {
        &self.configuration
    }

    pub fn registered_75_sha256_v1(&self) -> [u8; 32] {
        let mut bytes = Vec::new();
        push_string_v1(&mut bytes, REGISTERED_DECK_HASH_DOMAIN_V1);
        push_string_v1(&mut bytes, &self.deck_id);
        push_cards_v1(&mut bytes, self.configuration.mainboard());
        push_cards_v1(&mut bytes, self.configuration.sideboard());
        sha256_v1(&bytes)
    }

    pub fn apply_plan_v1(
        &self,
        plan: &SideboardPlanV1,
        policy_id: &str,
        policy_sha256: [u8; 32],
    ) -> Result<(DeckConfigurationV1, AppliedSideboardReceiptV1), SideboardErrorV1> {
        if plan.self_deck_id != self.deck_id {
            return Err(SideboardErrorV1::PlanDeckMismatch {
                registered: self.deck_id.clone(),
                plan: plan.self_deck_id.clone(),
            });
        }
        validate_identifier_v1("policy_id", policy_id)?;

        let mut mainboard_counts = counts_from_cards_v1(self.configuration.mainboard());
        let mut sideboard_counts = counts_from_cards_v1(self.configuration.sideboard());

        for row in &plan.cards_out {
            remove_count_v1(
                &mut mainboard_counts,
                *row,
                SideboardZoneV1::RegisteredMainboard,
            )?;
            add_count_v1(&mut sideboard_counts, *row);
        }
        for row in &plan.cards_in {
            remove_count_v1(
                &mut sideboard_counts,
                *row,
                SideboardZoneV1::RegisteredSideboard,
            )?;
            add_count_v1(&mut mainboard_counts, *row);
        }

        let configuration = DeckConfigurationV1::new_exact_v1(
            expand_counts_v1(&mainboard_counts),
            expand_counts_v1(&sideboard_counts),
        )?;
        if configuration.combined_card_counts_v1() != self.configuration.combined_card_counts_v1() {
            return Err(SideboardErrorV1::RegisteredMultisetChanged);
        }

        let receipt = AppliedSideboardReceiptV1 {
            schema: SIDEBOARD_RECEIPT_SCHEMA_V1,
            policy_id: policy_id.to_owned(),
            policy_sha256,
            self_deck_id: self.deck_id.clone(),
            opponent_deck_id: plan.opponent_deck_id.clone(),
            game_index: plan.game_index,
            registered_75_sha256: self.registered_75_sha256_v1(),
            before_mainboard_sha256: self.configuration.mainboard_sha256_v1(),
            before_sideboard_sha256: self.configuration.sideboard_sha256_v1(),
            cards_in: plan.cards_in.clone(),
            cards_out: plan.cards_out.clone(),
            after_mainboard_sha256: configuration.mainboard_sha256_v1(),
            after_sideboard_sha256: configuration.sideboard_sha256_v1(),
        };
        Ok((configuration, receipt))
    }
}

/// Parses the checked-in nine-deck pool into exact executable 60/15
/// registrations. It deliberately fails until every referenced card exists
/// in the registry and has full rules support, making completion of the card
/// pool an executable BO3 admission gate rather than a documentation claim.
pub fn checked_in_pauper_registered_decks_v1() -> Result<Vec<RegisteredDeckV1>, SideboardErrorV1> {
    let document = checked_in_pauper_pool_document_v1()?;
    document
        .decks
        .into_iter()
        .map(register_pauper_pool_deck_v1)
        .collect()
}

/// Loads one checked-in 60/15 registration by id and admits it as soon as
/// that exact deck is fully executable. This lets completed decks enter BO3
/// sessions without weakening the all-nine admission gate above.
pub fn checked_in_pauper_registered_deck_by_id_v1(
    deck_id: &str,
) -> Result<RegisteredDeckV1, SideboardErrorV1> {
    validate_identifier_v1("pool deck id", deck_id)?;
    let document = checked_in_pauper_pool_document_v1()?;
    let deck = document
        .decks
        .into_iter()
        .find(|deck| deck.id == deck_id)
        .ok_or_else(|| SideboardErrorV1::UnknownPoolDeckId {
            deck_id: deck_id.to_owned(),
        })?;
    register_pauper_pool_deck_v1(deck)
}

fn checked_in_pauper_pool_document_v1() -> Result<PauperPoolDocumentV1, SideboardErrorV1> {
    let document: PauperPoolDocumentV1 = serde_json::from_str(PAUPER_POOL_JSON_V1)
        .map_err(|error| SideboardErrorV1::PoolJson(error.to_string()))?;
    if document.schema != PAUPER_POOL_SCHEMA_V1 {
        return Err(SideboardErrorV1::PoolSchemaMismatch {
            actual: document.schema,
        });
    }
    if document.decks.len() != 9 {
        return Err(SideboardErrorV1::PoolDeckCount {
            actual: document.decks.len(),
        });
    }

    let mut deck_ids = document
        .decks
        .iter()
        .map(|deck| deck.id.clone())
        .collect::<Vec<_>>();
    for deck_id in &deck_ids {
        validate_identifier_v1("pool deck id", deck_id)?;
    }
    deck_ids.sort();
    for pair in deck_ids.windows(2) {
        if pair[0] == pair[1] {
            return Err(SideboardErrorV1::DuplicatePoolDeckId {
                deck_id: pair[0].clone(),
            });
        }
    }
    let policy = DeterministicSideboardPolicyV1::checked_in_pauper_v1()?;
    if deck_ids != policy.deck_ids {
        return Err(SideboardErrorV1::PoolPolicyDeckSetMismatch);
    }

    Ok(document)
}

fn register_pauper_pool_deck_v1(
    deck: PauperPoolDeckDocumentV1,
) -> Result<RegisteredDeckV1, SideboardErrorV1> {
    let mainboard = expand_pool_zone_v1(
        &deck.id,
        SideboardZoneV1::RegisteredMainboard,
        deck.mainboard,
    )?;
    let sideboard = expand_pool_zone_v1(
        &deck.id,
        SideboardZoneV1::RegisteredSideboard,
        deck.sideboard,
    )?;
    RegisteredDeckV1::new_fully_supported_v1(deck.id, mainboard, sideboard)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SideboardPlanV1 {
    self_deck_id: String,
    opponent_deck_id: String,
    game_index: u8,
    cards_in: Vec<CardCountV1>,
    cards_out: Vec<CardCountV1>,
}

impl SideboardPlanV1 {
    pub fn new_v1(
        self_deck_id: impl Into<String>,
        opponent_deck_id: impl Into<String>,
        game_index: u8,
        mut cards_in: Vec<CardCountV1>,
        mut cards_out: Vec<CardCountV1>,
    ) -> Result<Self, SideboardErrorV1> {
        let self_deck_id = self_deck_id.into();
        let opponent_deck_id = opponent_deck_id.into();
        validate_identifier_v1("self_deck_id", &self_deck_id)?;
        validate_identifier_v1("opponent_deck_id", &opponent_deck_id)?;
        validate_postboard_game_index_v1(game_index)?;
        canonicalize_card_counts_v1(&mut cards_in)?;
        canonicalize_card_counts_v1(&mut cards_out)?;

        let cards_in_total = count_total_v1(&cards_in);
        let cards_out_total = count_total_v1(&cards_out);
        if cards_in_total != cards_out_total {
            return Err(SideboardErrorV1::UnequalExchangeCounts {
                cards_in: cards_in_total,
                cards_out: cards_out_total,
            });
        }
        if cards_in_total > REGISTERED_SIDEBOARD_SIZE_V1 {
            return Err(SideboardErrorV1::ExchangeTooLarge {
                count: cards_in_total,
            });
        }
        let cards_out_ids = cards_out
            .iter()
            .map(|row| row.card_id)
            .collect::<BTreeSet<_>>();
        if let Some(card_id) = cards_in
            .iter()
            .map(|row| row.card_id)
            .find(|card_id| cards_out_ids.contains(card_id))
        {
            return Err(SideboardErrorV1::CardListedBothInAndOut { card_id });
        }

        Ok(Self {
            self_deck_id,
            opponent_deck_id,
            game_index,
            cards_in,
            cards_out,
        })
    }

    pub fn keep_registered_v1(
        self_deck_id: impl Into<String>,
        opponent_deck_id: impl Into<String>,
        game_index: u8,
    ) -> Result<Self, SideboardErrorV1> {
        Self::new_v1(
            self_deck_id,
            opponent_deck_id,
            game_index,
            Vec::new(),
            Vec::new(),
        )
    }

    pub fn self_deck_id(&self) -> &str {
        &self.self_deck_id
    }

    pub fn opponent_deck_id(&self) -> &str {
        &self.opponent_deck_id
    }

    pub fn game_index(&self) -> u8 {
        self.game_index
    }

    pub fn cards_in(&self) -> &[CardCountV1] {
        &self.cards_in
    }

    pub fn cards_out(&self) -> &[CardCountV1] {
        &self.cards_out
    }

    fn append_canonical_bytes_v1(&self, bytes: &mut Vec<u8>) {
        push_string_v1(bytes, &self.self_deck_id);
        push_string_v1(bytes, &self.opponent_deck_id);
        bytes.push(self.game_index);
        push_card_counts_v1(bytes, &self.cards_in);
        push_card_counts_v1(bytes, &self.cards_out);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SideboardDefaultPlanV1 {
    KeepRegisteredConfiguration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeterministicSideboardPolicyV1 {
    policy_id: String,
    deck_ids: Vec<String>,
    default_plan: SideboardDefaultPlanV1,
    plans: Vec<SideboardPlanV1>,
}

impl DeterministicSideboardPolicyV1 {
    pub fn new_v1(
        policy_id: impl Into<String>,
        mut deck_ids: Vec<String>,
        default_plan: SideboardDefaultPlanV1,
        mut plans: Vec<SideboardPlanV1>,
    ) -> Result<Self, SideboardErrorV1> {
        let policy_id = policy_id.into();
        validate_identifier_v1("policy_id", &policy_id)?;
        if deck_ids.is_empty() {
            return Err(SideboardErrorV1::EmptyPolicyDeckSet);
        }
        for deck_id in &deck_ids {
            validate_identifier_v1("policy deck_id", deck_id)?;
        }
        deck_ids.sort();
        for pair in deck_ids.windows(2) {
            if pair[0] == pair[1] {
                return Err(SideboardErrorV1::DuplicatePolicyDeckId {
                    deck_id: pair[0].clone(),
                });
            }
        }
        let supported = deck_ids.iter().cloned().collect::<BTreeSet<_>>();
        for plan in &plans {
            if !supported.contains(&plan.self_deck_id) {
                return Err(SideboardErrorV1::UnknownPolicyDeckId {
                    deck_id: plan.self_deck_id.clone(),
                });
            }
            if !supported.contains(&plan.opponent_deck_id) {
                return Err(SideboardErrorV1::UnknownPolicyDeckId {
                    deck_id: plan.opponent_deck_id.clone(),
                });
            }
        }
        plans.sort_by(|left, right| {
            (&left.self_deck_id, &left.opponent_deck_id, left.game_index).cmp(&(
                &right.self_deck_id,
                &right.opponent_deck_id,
                right.game_index,
            ))
        });
        for pair in plans.windows(2) {
            if plan_key_v1(&pair[0]) == plan_key_v1(&pair[1]) {
                return Err(SideboardErrorV1::DuplicatePlanKey {
                    self_deck_id: pair[0].self_deck_id.clone(),
                    opponent_deck_id: pair[0].opponent_deck_id.clone(),
                    game_index: pair[0].game_index,
                });
            }
        }
        Ok(Self {
            policy_id,
            deck_ids,
            default_plan,
            plans,
        })
    }

    pub fn from_json_v1(json: &str) -> Result<Self, SideboardErrorV1> {
        let document: SideboardPolicyDocumentV1 = serde_json::from_str(json)
            .map_err(|error| SideboardErrorV1::PolicyJson(error.to_string()))?;
        if document.schema != SIDEBOARD_POLICY_SCHEMA_V1 {
            return Err(SideboardErrorV1::PolicySchemaMismatch {
                actual: document.schema,
            });
        }
        if document.coverage.ordered_matchups != SIDEBOARD_POLICY_MATCHUP_COVERAGE_V1
            || document.coverage.game_indices != [2, 3]
        {
            return Err(SideboardErrorV1::PolicyCoverageMismatch);
        }
        let default_plan = match document.default_plan.as_str() {
            "keep_registered_configuration" => SideboardDefaultPlanV1::KeepRegisteredConfiguration,
            other => {
                return Err(SideboardErrorV1::UnknownDefaultPlan {
                    actual: other.to_owned(),
                })
            }
        };
        let mut plans = Vec::with_capacity(document.plans.len());
        for plan in document.plans {
            plans.push(SideboardPlanV1::new_v1(
                plan.self_deck_id,
                plan.opponent_deck_id,
                plan.game_index,
                plan.cards_in,
                plan.cards_out,
            )?);
        }
        Self::new_v1(document.policy_id, document.deck_ids, default_plan, plans)
    }

    pub fn checked_in_pauper_v1() -> Result<Self, SideboardErrorV1> {
        Self::from_json_v1(PAUPER_SIDEBOARD_POLICY_JSON_V1)
    }

    pub fn policy_id(&self) -> &str {
        &self.policy_id
    }

    pub fn deck_ids(&self) -> &[String] {
        &self.deck_ids
    }

    pub fn plans(&self) -> &[SideboardPlanV1] {
        &self.plans
    }

    pub fn plan_for_v1(
        &self,
        self_deck_id: &str,
        opponent_deck_id: &str,
        game_index: u8,
    ) -> Result<SideboardPlanV1, SideboardErrorV1> {
        validate_postboard_game_index_v1(game_index)?;
        for deck_id in [self_deck_id, opponent_deck_id] {
            if self
                .deck_ids
                .binary_search_by(|candidate| candidate.as_str().cmp(deck_id))
                .is_err()
            {
                return Err(SideboardErrorV1::UnknownPolicyDeckId {
                    deck_id: deck_id.to_owned(),
                });
            }
        }
        if let Some(plan) = self.plans.iter().find(|plan| {
            plan.self_deck_id == self_deck_id
                && plan.opponent_deck_id == opponent_deck_id
                && plan.game_index == game_index
        }) {
            return Ok(plan.clone());
        }
        match self.default_plan {
            SideboardDefaultPlanV1::KeepRegisteredConfiguration => {
                SideboardPlanV1::keep_registered_v1(self_deck_id, opponent_deck_id, game_index)
            }
        }
    }

    pub fn apply_v1(
        &self,
        registered_deck: &RegisteredDeckV1,
        opponent_deck_id: &str,
        game_index: u8,
    ) -> Result<(DeckConfigurationV1, AppliedSideboardReceiptV1), SideboardErrorV1> {
        let plan = self.plan_for_v1(registered_deck.deck_id(), opponent_deck_id, game_index)?;
        registered_deck.apply_plan_v1(&plan, &self.policy_id, self.policy_sha256_v1())
    }

    pub fn canonical_bytes_v1(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        push_string_v1(&mut bytes, SIDEBOARD_POLICY_HASH_DOMAIN_V1);
        push_string_v1(&mut bytes, SIDEBOARD_POLICY_SCHEMA_V1);
        push_string_v1(&mut bytes, &self.policy_id);
        push_strings_v1(&mut bytes, &self.deck_ids);
        bytes.push(match self.default_plan {
            SideboardDefaultPlanV1::KeepRegisteredConfiguration => 0,
        });
        push_len_v1(&mut bytes, self.plans.len());
        for plan in &self.plans {
            plan.append_canonical_bytes_v1(&mut bytes);
        }
        bytes
    }

    pub fn policy_sha256_v1(&self) -> [u8; 32] {
        sha256_v1(&self.canonical_bytes_v1())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppliedSideboardReceiptV1 {
    schema: &'static str,
    policy_id: String,
    policy_sha256: [u8; 32],
    self_deck_id: String,
    opponent_deck_id: String,
    game_index: u8,
    registered_75_sha256: [u8; 32],
    before_mainboard_sha256: [u8; 32],
    before_sideboard_sha256: [u8; 32],
    cards_in: Vec<CardCountV1>,
    cards_out: Vec<CardCountV1>,
    after_mainboard_sha256: [u8; 32],
    after_sideboard_sha256: [u8; 32],
}

impl AppliedSideboardReceiptV1 {
    pub fn schema(&self) -> &'static str {
        self.schema
    }

    pub fn policy_id(&self) -> &str {
        &self.policy_id
    }

    pub fn policy_sha256(&self) -> [u8; 32] {
        self.policy_sha256
    }

    pub fn self_deck_id(&self) -> &str {
        &self.self_deck_id
    }

    pub fn opponent_deck_id(&self) -> &str {
        &self.opponent_deck_id
    }

    pub fn game_index(&self) -> u8 {
        self.game_index
    }

    pub fn registered_75_sha256(&self) -> [u8; 32] {
        self.registered_75_sha256
    }

    pub fn before_mainboard_sha256(&self) -> [u8; 32] {
        self.before_mainboard_sha256
    }

    pub fn before_sideboard_sha256(&self) -> [u8; 32] {
        self.before_sideboard_sha256
    }

    pub fn cards_in(&self) -> &[CardCountV1] {
        &self.cards_in
    }

    pub fn cards_out(&self) -> &[CardCountV1] {
        &self.cards_out
    }

    pub fn after_mainboard_sha256(&self) -> [u8; 32] {
        self.after_mainboard_sha256
    }

    pub fn after_sideboard_sha256(&self) -> [u8; 32] {
        self.after_sideboard_sha256
    }

    pub fn canonical_bytes_v1(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        push_string_v1(&mut bytes, SIDEBOARD_RECEIPT_HASH_DOMAIN_V1);
        push_string_v1(&mut bytes, self.schema);
        push_string_v1(&mut bytes, &self.policy_id);
        bytes.extend_from_slice(&self.policy_sha256);
        push_string_v1(&mut bytes, &self.self_deck_id);
        push_string_v1(&mut bytes, &self.opponent_deck_id);
        bytes.push(self.game_index);
        bytes.extend_from_slice(&self.registered_75_sha256);
        bytes.extend_from_slice(&self.before_mainboard_sha256);
        bytes.extend_from_slice(&self.before_sideboard_sha256);
        push_card_counts_v1(&mut bytes, &self.cards_in);
        push_card_counts_v1(&mut bytes, &self.cards_out);
        bytes.extend_from_slice(&self.after_mainboard_sha256);
        bytes.extend_from_slice(&self.after_sideboard_sha256);
        bytes
    }

    pub fn receipt_sha256_v1(&self) -> [u8; 32] {
        sha256_v1(&self.canonical_bytes_v1())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SideboardErrorV1 {
    InvalidIdentifier {
        field: &'static str,
    },
    WrongMainboardSize {
        actual: usize,
    },
    WrongSideboardSize {
        actual: usize,
    },
    InvalidPostboardGameIndex {
        actual: u8,
    },
    ZeroCardCount {
        card_id: u16,
    },
    DuplicateCardCount {
        card_id: u16,
    },
    UnequalExchangeCounts {
        cards_in: usize,
        cards_out: usize,
    },
    ExchangeTooLarge {
        count: usize,
    },
    CardListedBothInAndOut {
        card_id: u16,
    },
    CardUnavailable {
        zone: SideboardZoneV1,
        card_id: u16,
        requested: u8,
        available: u8,
    },
    PlanDeckMismatch {
        registered: String,
        plan: String,
    },
    RegisteredMultisetChanged,
    UnknownRegisteredCardId {
        zone: SideboardZoneV1,
        card_id: u16,
    },
    TokenInRegisteredDeck {
        zone: SideboardZoneV1,
        card_id: u16,
        card_name: String,
    },
    CardNotFullySupported {
        zone: SideboardZoneV1,
        card_id: u16,
        card_name: String,
    },
    PoolJson(String),
    PoolSchemaMismatch {
        actual: String,
    },
    PoolDeckCount {
        actual: usize,
    },
    DuplicatePoolDeckId {
        deck_id: String,
    },
    UnknownPoolDeckId {
        deck_id: String,
    },
    PoolPolicyDeckSetMismatch,
    PoolCopyCountMismatch {
        deck_id: String,
        zone: SideboardZoneV1,
        declared: usize,
        actual: usize,
    },
    DuplicatePoolCardName {
        deck_id: String,
        zone: SideboardZoneV1,
        card_name: String,
    },
    ZeroPoolCardCount {
        deck_id: String,
        zone: SideboardZoneV1,
        card_name: String,
    },
    UnregisteredPoolCard {
        deck_id: String,
        zone: SideboardZoneV1,
        card_name: String,
    },
    EmptyPolicyDeckSet,
    DuplicatePolicyDeckId {
        deck_id: String,
    },
    UnknownPolicyDeckId {
        deck_id: String,
    },
    DuplicatePlanKey {
        self_deck_id: String,
        opponent_deck_id: String,
        game_index: u8,
    },
    PolicyJson(String),
    PolicySchemaMismatch {
        actual: String,
    },
    PolicyCoverageMismatch,
    UnknownDefaultPlan {
        actual: String,
    },
}

impl fmt::Display for SideboardErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidIdentifier { field } => write!(formatter, "invalid {field}"),
            Self::WrongMainboardSize { actual } => write!(
                formatter,
                "mainboard must contain exactly {REGISTERED_MAINBOARD_SIZE_V1} cards, got {actual}"
            ),
            Self::WrongSideboardSize { actual } => write!(
                formatter,
                "sideboard must contain exactly {REGISTERED_SIDEBOARD_SIZE_V1} cards, got {actual}"
            ),
            Self::InvalidPostboardGameIndex { actual } => {
                write!(formatter, "sideboarding is only valid for games 2 and 3, got {actual}")
            }
            Self::ZeroCardCount { card_id } => {
                write!(formatter, "card {card_id} has a zero exchange count")
            }
            Self::DuplicateCardCount { card_id } => {
                write!(formatter, "card {card_id} occurs twice in one exchange list")
            }
            Self::UnequalExchangeCounts {
                cards_in,
                cards_out,
            } => write!(
                formatter,
                "exact 60/15 exchange requires equal cards in and out, got {cards_in} and {cards_out}"
            ),
            Self::ExchangeTooLarge { count } => {
                write!(formatter, "cannot exchange {count} cards from a 15-card sideboard")
            }
            Self::CardListedBothInAndOut { card_id } => write!(
                formatter,
                "card {card_id} cannot be listed in both sides of one canonical exchange"
            ),
            Self::CardUnavailable {
                zone,
                card_id,
                requested,
                available,
            } => write!(
                formatter,
                "card {card_id} requested {requested} copies from {zone}, only {available} available"
            ),
            Self::PlanDeckMismatch { registered, plan } => write!(
                formatter,
                "sideboard plan deck {plan:?} does not match registered deck {registered:?}"
            ),
            Self::RegisteredMultisetChanged => {
                formatter.write_str("sideboarding changed the registered 75-card multiset")
            }
            Self::UnknownRegisteredCardId { zone, card_id } => {
                write!(formatter, "unknown card id {card_id} in {zone}")
            }
            Self::TokenInRegisteredDeck {
                zone,
                card_id,
                card_name,
            } => write!(
                formatter,
                "token {card_name:?} ({card_id}) cannot be registered in {zone}"
            ),
            Self::CardNotFullySupported {
                zone,
                card_id,
                card_name,
            } => write!(
                formatter,
                "card {card_name:?} ({card_id}) in {zone} is not fully supported"
            ),
            Self::PoolJson(error) => write!(formatter, "invalid Pauper pool JSON: {error}"),
            Self::PoolSchemaMismatch { actual } => {
                write!(formatter, "unexpected Pauper pool schema {actual:?}")
            }
            Self::PoolDeckCount { actual } => {
                write!(formatter, "Pauper pool must contain 9 decks, got {actual}")
            }
            Self::DuplicatePoolDeckId { deck_id } => {
                write!(formatter, "duplicate Pauper pool deck id {deck_id:?}")
            }
            Self::UnknownPoolDeckId { deck_id } => {
                write!(formatter, "unknown Pauper pool deck id {deck_id:?}")
            }
            Self::PoolPolicyDeckSetMismatch => {
                formatter.write_str("Pauper pool and sideboard policy deck sets differ")
            }
            Self::PoolCopyCountMismatch {
                deck_id,
                zone,
                declared,
                actual,
            } => write!(
                formatter,
                "Pauper pool deck {deck_id:?} {zone} declares {declared} copies but expands to {actual}"
            ),
            Self::DuplicatePoolCardName {
                deck_id,
                zone,
                card_name,
            } => write!(
                formatter,
                "Pauper pool deck {deck_id:?} {zone} lists {card_name:?} twice"
            ),
            Self::ZeroPoolCardCount {
                deck_id,
                zone,
                card_name,
            } => write!(
                formatter,
                "Pauper pool deck {deck_id:?} {zone} gives {card_name:?} zero copies"
            ),
            Self::UnregisteredPoolCard {
                deck_id,
                zone,
                card_name,
            } => write!(
                formatter,
                "Pauper pool deck {deck_id:?} {zone} references unregistered card {card_name:?}"
            ),
            Self::EmptyPolicyDeckSet => formatter.write_str("policy deck set is empty"),
            Self::DuplicatePolicyDeckId { deck_id } => {
                write!(formatter, "duplicate policy deck id {deck_id:?}")
            }
            Self::UnknownPolicyDeckId { deck_id } => {
                write!(formatter, "unknown policy deck id {deck_id:?}")
            }
            Self::DuplicatePlanKey {
                self_deck_id,
                opponent_deck_id,
                game_index,
            } => write!(
                formatter,
                "duplicate sideboard plan for {self_deck_id:?} versus {opponent_deck_id:?} game {game_index}"
            ),
            Self::PolicyJson(error) => write!(formatter, "invalid sideboard policy JSON: {error}"),
            Self::PolicySchemaMismatch { actual } => {
                write!(formatter, "unexpected sideboard policy schema {actual:?}")
            }
            Self::PolicyCoverageMismatch => formatter.write_str(
                "sideboard policy must cover all ordered matchups for games 2 and 3",
            ),
            Self::UnknownDefaultPlan { actual } => {
                write!(formatter, "unknown sideboard default plan {actual:?}")
            }
        }
    }
}

impl Error for SideboardErrorV1 {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SideboardZoneV1 {
    RegisteredMainboard,
    RegisteredSideboard,
}

impl fmt::Display for SideboardZoneV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::RegisteredMainboard => "registered mainboard",
            Self::RegisteredSideboard => "registered sideboard",
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SideboardPolicyDocumentV1 {
    schema: String,
    policy_id: String,
    deck_ids: Vec<String>,
    coverage: SideboardPolicyCoverageDocumentV1,
    default_plan: String,
    plans: Vec<SideboardPlanDocumentV1>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SideboardPolicyCoverageDocumentV1 {
    ordered_matchups: String,
    game_indices: Vec<u8>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SideboardPlanDocumentV1 {
    self_deck_id: String,
    opponent_deck_id: String,
    game_index: u8,
    cards_in: Vec<CardCountV1>,
    cards_out: Vec<CardCountV1>,
}

#[derive(Debug, Deserialize)]
struct PauperPoolDocumentV1 {
    schema: String,
    decks: Vec<PauperPoolDeckDocumentV1>,
}

#[derive(Debug, Deserialize)]
struct PauperPoolDeckDocumentV1 {
    id: String,
    mainboard: PauperPoolZoneDocumentV1,
    sideboard: PauperPoolZoneDocumentV1,
}

#[derive(Debug, Deserialize)]
struct PauperPoolZoneDocumentV1 {
    copy_count: usize,
    cards: Vec<PauperPoolCardDocumentV1>,
}

#[derive(Debug, Deserialize)]
struct PauperPoolCardDocumentV1 {
    name: String,
    count: u8,
}

fn validate_registered_cards_v1(
    cards: &[u16],
    zone: SideboardZoneV1,
) -> Result<(), SideboardErrorV1> {
    for &card_id in cards {
        let Some(definition) = crate::card_def::CARD_DEFS.get(card_id as usize) else {
            return Err(SideboardErrorV1::UnknownRegisteredCardId { zone, card_id });
        };
        if definition.is_token {
            return Err(SideboardErrorV1::TokenInRegisteredDeck {
                zone,
                card_id,
                card_name: definition.name.to_owned(),
            });
        }
        if !definition.has_full_support() || !definition.is_executable() {
            return Err(SideboardErrorV1::CardNotFullySupported {
                zone,
                card_id,
                card_name: definition.name.to_owned(),
            });
        }
    }
    Ok(())
}

fn expand_pool_zone_v1(
    deck_id: &str,
    zone: SideboardZoneV1,
    document: PauperPoolZoneDocumentV1,
) -> Result<Vec<u16>, SideboardErrorV1> {
    let mut names = BTreeSet::new();
    let mut cards = Vec::with_capacity(document.copy_count);
    for row in document.cards {
        if !names.insert(row.name.clone()) {
            return Err(SideboardErrorV1::DuplicatePoolCardName {
                deck_id: deck_id.to_owned(),
                zone,
                card_name: row.name,
            });
        }
        if row.count == 0 {
            return Err(SideboardErrorV1::ZeroPoolCardCount {
                deck_id: deck_id.to_owned(),
                zone,
                card_name: row.name,
            });
        }
        let Some(card_id) = crate::card_def::card_id_by_name(&row.name) else {
            return Err(SideboardErrorV1::UnregisteredPoolCard {
                deck_id: deck_id.to_owned(),
                zone,
                card_name: row.name,
            });
        };
        cards.extend(std::iter::repeat_n(card_id, row.count as usize));
    }
    if cards.len() != document.copy_count {
        return Err(SideboardErrorV1::PoolCopyCountMismatch {
            deck_id: deck_id.to_owned(),
            zone,
            declared: document.copy_count,
            actual: cards.len(),
        });
    }
    Ok(cards)
}

fn validate_identifier_v1(field: &'static str, value: &str) -> Result<(), SideboardErrorV1> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_graphic() || byte == b' ')
    {
        return Err(SideboardErrorV1::InvalidIdentifier { field });
    }
    Ok(())
}

fn validate_postboard_game_index_v1(game_index: u8) -> Result<(), SideboardErrorV1> {
    if !(2..=3).contains(&game_index) {
        return Err(SideboardErrorV1::InvalidPostboardGameIndex { actual: game_index });
    }
    Ok(())
}

fn canonicalize_card_counts_v1(rows: &mut Vec<CardCountV1>) -> Result<(), SideboardErrorV1> {
    rows.sort_unstable();
    for row in rows.iter() {
        if row.count == 0 {
            return Err(SideboardErrorV1::ZeroCardCount {
                card_id: row.card_id,
            });
        }
    }
    for pair in rows.windows(2) {
        if pair[0].card_id == pair[1].card_id {
            return Err(SideboardErrorV1::DuplicateCardCount {
                card_id: pair[0].card_id,
            });
        }
    }
    Ok(())
}

fn count_total_v1(rows: &[CardCountV1]) -> usize {
    rows.iter().map(|row| usize::from(row.count)).sum()
}

fn counts_from_cards_v1(cards: &[u16]) -> BTreeMap<u16, u8> {
    let mut counts = BTreeMap::new();
    for card_id in cards {
        *counts.entry(*card_id).or_insert(0) += 1;
    }
    counts
}

fn counts_to_rows_v1(counts: &BTreeMap<u16, u8>) -> Vec<CardCountV1> {
    counts
        .iter()
        .map(|(card_id, count)| CardCountV1 {
            card_id: *card_id,
            count: *count,
        })
        .collect()
}

fn expand_counts_v1(counts: &BTreeMap<u16, u8>) -> Vec<u16> {
    let mut cards = Vec::new();
    for (card_id, count) in counts {
        cards.extend(std::iter::repeat_n(*card_id, usize::from(*count)));
    }
    cards
}

fn remove_count_v1(
    counts: &mut BTreeMap<u16, u8>,
    row: CardCountV1,
    zone: SideboardZoneV1,
) -> Result<(), SideboardErrorV1> {
    let available = counts.get(&row.card_id).copied().unwrap_or(0);
    if available < row.count {
        return Err(SideboardErrorV1::CardUnavailable {
            zone,
            card_id: row.card_id,
            requested: row.count,
            available,
        });
    }
    if available == row.count {
        counts.remove(&row.card_id);
    } else {
        counts.insert(row.card_id, available - row.count);
    }
    Ok(())
}

fn add_count_v1(counts: &mut BTreeMap<u16, u8>, row: CardCountV1) {
    *counts.entry(row.card_id).or_insert(0) += row.count;
}

fn configuration_zone_sha256_v1(zone: &[u8], cards: &[u16]) -> [u8; 32] {
    let mut bytes = Vec::new();
    push_string_v1(&mut bytes, DECK_CONFIGURATION_HASH_DOMAIN_V1);
    push_bytes_v1(&mut bytes, zone);
    push_cards_v1(&mut bytes, cards);
    sha256_v1(&bytes)
}

fn plan_key_v1(plan: &SideboardPlanV1) -> (&str, &str, u8) {
    (
        plan.self_deck_id.as_str(),
        plan.opponent_deck_id.as_str(),
        plan.game_index,
    )
}

fn sha256_v1(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn push_len_v1(bytes: &mut Vec<u8>, len: usize) {
    let len = u32::try_from(len).expect("bounded match contracts fit in u32");
    bytes.extend_from_slice(&len.to_be_bytes());
}

fn push_bytes_v1(bytes: &mut Vec<u8>, value: &[u8]) {
    push_len_v1(bytes, value.len());
    bytes.extend_from_slice(value);
}

fn push_string_v1(bytes: &mut Vec<u8>, value: &str) {
    push_bytes_v1(bytes, value.as_bytes());
}

fn push_strings_v1(bytes: &mut Vec<u8>, values: &[String]) {
    push_len_v1(bytes, values.len());
    for value in values {
        push_string_v1(bytes, value);
    }
}

fn push_cards_v1(bytes: &mut Vec<u8>, cards: &[u16]) {
    push_len_v1(bytes, cards.len());
    for card_id in cards {
        bytes.extend_from_slice(&card_id.to_be_bytes());
    }
}

fn push_card_counts_v1(bytes: &mut Vec<u8>, rows: &[CardCountV1]) {
    push_len_v1(bytes, rows.len());
    for row in rows {
        bytes.extend_from_slice(&row.card_id.to_be_bytes());
        bytes.push(row.count);
    }
}
