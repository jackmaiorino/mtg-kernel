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
/// The smallest mainboard `DeckFormatV1::Limited40V1` admits.
pub const LIMITED40_MIN_MAINBOARD_SIZE_V1: usize = 40;
/// The largest mainboard `DeckFormatV1::Limited40V1` admits: the Limited
/// session's own input ceiling (`limited_session_v1::MAX_CUSTOM_DECK_CARDS_V1`).
pub const LIMITED40_MAX_MAINBOARD_SIZE_V1: usize =
    crate::limited_session_v1::MAX_CUSTOM_DECK_CARDS_V1;
/// The largest sideboard `DeckFormatV1::Limited40V1` admits (it may be empty).
pub const LIMITED40_MAX_SIDEBOARD_SIZE_V1: usize = REGISTERED_SIDEBOARD_SIZE_V1;
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

/// An opt-in deck format: an expanded-training episode's `deck_format`.
/// Absent (`None` wherever a format is taken) is the exact registered 60/15
/// contract, unchanged. The format changes deck-size admission only: never
/// the deal, the game or any other check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DeckFormatV1 {
    /// A Limited deck: at least 40 mainboard cards and a sideboard of 0 to
    /// 15 cards.
    #[serde(rename = "limited40-v1")]
    Limited40V1,
}

impl DeckFormatV1 {
    /// The format's serialized name.
    pub fn name_v1(self) -> &'static str {
        match self {
            Self::Limited40V1 => "limited40-v1",
        }
    }
}

/// The mainboard-size refusal for `format`, or None when `len` is admitted.
/// `None` is the exact 60-card rule.
pub fn mainboard_size_error_v1(format: Option<DeckFormatV1>, len: usize) -> Option<String> {
    match format {
        None => (len != REGISTERED_MAINBOARD_SIZE_V1).then(|| {
            format!("must contain exactly {REGISTERED_MAINBOARD_SIZE_V1} cards, got {len}")
        }),
        Some(format @ DeckFormatV1::Limited40V1) => {
            (!(LIMITED40_MIN_MAINBOARD_SIZE_V1..=LIMITED40_MAX_MAINBOARD_SIZE_V1).contains(&len))
                .then(|| {
                    format!(
                        "must contain {LIMITED40_MIN_MAINBOARD_SIZE_V1} to {LIMITED40_MAX_MAINBOARD_SIZE_V1} cards under {}, got {len}",
                        format.name_v1()
                    )
                })
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeckConfigurationV1 {
    mainboard: Vec<u16>,
    sideboard: Vec<u16>,
    /// The size rule this configuration was admitted under; a plan applied
    /// to it is admitted under the same rule.
    format: Option<DeckFormatV1>,
}

impl DeckConfigurationV1 {
    pub fn new_exact_v1(
        mainboard: Vec<u16>,
        sideboard: Vec<u16>,
    ) -> Result<Self, SideboardErrorV1> {
        Self::new_for_format_v1(mainboard, sideboard, None)
    }

    /// `new_exact_v1` under an opt-in deck format; `None` is exactly
    /// `new_exact_v1`.
    pub fn new_for_format_v1(
        mut mainboard: Vec<u16>,
        mut sideboard: Vec<u16>,
        format: Option<DeckFormatV1>,
    ) -> Result<Self, SideboardErrorV1> {
        match format {
            None => {
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
            }
            Some(format) => {
                if mainboard_size_error_v1(Some(format), mainboard.len()).is_some() {
                    return Err(SideboardErrorV1::FormatMainboardSize {
                        format,
                        actual: mainboard.len(),
                    });
                }
                if sideboard.len() > LIMITED40_MAX_SIDEBOARD_SIZE_V1 {
                    return Err(SideboardErrorV1::FormatSideboardSize {
                        format,
                        actual: sideboard.len(),
                    });
                }
            }
        }
        mainboard.sort_unstable();
        sideboard.sort_unstable();
        Ok(Self {
            mainboard,
            sideboard,
            format,
        })
    }

    /// The deck format this configuration was admitted under.
    pub fn format_v1(&self) -> Option<DeckFormatV1> {
        self.format
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

    /// The registered multiset as `CardCountV1` rows, the 60/15 contract's
    /// row form (receipts, sideboard inputs, BO3). A configuration admitted
    /// without a deck format holds at most 75 cards, so its rows are always
    /// exact.
    ///
    /// # Panics
    ///
    /// When one card has more than 255 copies, which only a deck-format
    /// configuration can hold. Use `combined_card_count_map_v1` there.
    pub fn combined_card_counts_v1(&self) -> Vec<CardCountV1> {
        counts_to_rows_v1(&self.combined_card_count_map_v1())
    }

    /// The registered multiset, exact for every admissible configuration:
    /// each card's copies across mainboard and sideboard, at full width.
    pub fn combined_card_count_map_v1(&self) -> BTreeMap<u16, u32> {
        let mut counts = counts_from_cards_v1(&self.mainboard);
        for card_id in &self.sideboard {
            *counts.entry(*card_id).or_insert(0) += 1;
        }
        counts
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

    /// Shared executable-registration admission for expanded training and live
    /// BO3. The four-copy limit covers the entire registered 75; basic lands
    /// are the supported exception. This does not assert format or ban legality.
    pub fn new_executable_v1(
        deck_id: impl Into<String>,
        mainboard: Vec<u16>,
        sideboard: Vec<u16>,
    ) -> Result<Self, SideboardErrorV1> {
        Self::new_executable_for_format_v1(deck_id, mainboard, sideboard, None)
    }

    /// `new_executable_v1` with the size rule of an opt-in deck format; every
    /// other check is the same. `None` is exactly `new_executable_v1`.
    pub fn new_executable_for_format_v1(
        deck_id: impl Into<String>,
        mainboard: Vec<u16>,
        sideboard: Vec<u16>,
        format: Option<DeckFormatV1>,
    ) -> Result<Self, SideboardErrorV1> {
        let deck_id = deck_id.into();
        validate_identifier_v1("deck_id", &deck_id)?;
        let deck = Self {
            deck_id,
            configuration: DeckConfigurationV1::new_for_format_v1(mainboard, sideboard, format)?,
        };
        deck.validate_executable_v1()?;
        Ok(deck)
    }

    /// Revalidate even a structurally constructed registration before live
    /// play. Keeping this on the registration type gives training and BO3 one
    /// support, token, size and copy-limit contract.
    pub fn validate_executable_v1(&self) -> Result<(), SideboardErrorV1> {
        validate_registered_cards_v1(
            self.configuration.mainboard(),
            SideboardZoneV1::RegisteredMainboard,
        )?;
        validate_registered_cards_v1(
            self.configuration.sideboard(),
            SideboardZoneV1::RegisteredSideboard,
        )?;
        for (card_id, count) in self.configuration.combined_card_count_map_v1() {
            if count > 4
                && !crate::card_def::CARD_DEFS[usize::from(card_id)]
                    .supertypes
                    .contains(&crate::card_def::Supertype::Basic)
            {
                return Err(SideboardErrorV1::NonbasicCopyLimitExceeded { card_id, count });
            }
        }
        Ok(())
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

        let configuration = DeckConfigurationV1::new_for_format_v1(
            expand_counts_v1(&mainboard_counts),
            expand_counts_v1(&sideboard_counts),
            self.configuration.format,
        )?;
        if configuration.combined_card_count_map_v1()
            != self.configuration.combined_card_count_map_v1()
        {
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
    if document.decks.is_empty() {
        return Err(SideboardErrorV1::EmptyPool);
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
            || document.coverage.postboard_game_index_minimum != 2
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
    FormatMainboardSize {
        format: DeckFormatV1,
        actual: usize,
    },
    FormatSideboardSize {
        format: DeckFormatV1,
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
        /// Copies in the zone: full width, since a deck-format zone may hold
        /// more than 255 of one card.
        available: u32,
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
    NonbasicCopyLimitExceeded {
        card_id: u16,
        /// Full width, so 256 or more copies are reported, never wrapped.
        count: u32,
    },
    PoolJson(String),
    PoolSchemaMismatch {
        actual: String,
    },
    PoolDeckCount {
        actual: usize,
    },
    EmptyPool,
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
            Self::FormatMainboardSize { format, actual } => write!(
                formatter,
                "mainboard must contain {LIMITED40_MIN_MAINBOARD_SIZE_V1} to {LIMITED40_MAX_MAINBOARD_SIZE_V1} cards under {}, got {actual}",
                format.name_v1()
            ),
            Self::FormatSideboardSize { format, actual } => write!(
                formatter,
                "sideboard must contain at most {LIMITED40_MAX_SIDEBOARD_SIZE_V1} cards under {}, got {actual}",
                format.name_v1()
            ),
            Self::InvalidPostboardGameIndex { actual } => {
                write!(formatter, "sideboarding is only valid after game 1, got game {actual}")
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
            Self::NonbasicCopyLimitExceeded { card_id, count } => write!(
                formatter,
                "registered 75 contains {count} copies of nonbasic card {card_id}, exceeding four"
            ),
            Self::PoolJson(error) => write!(formatter, "invalid Pauper pool JSON: {error}"),
            Self::PoolSchemaMismatch { actual } => {
                write!(formatter, "unexpected Pauper pool schema {actual:?}")
            }
            Self::PoolDeckCount { actual } => {
                write!(formatter, "Pauper pool must contain 9 decks, got {actual}")
            }
            Self::EmptyPool => {
                formatter.write_str("Pauper pool must contain at least one deck")
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
            Self::PolicyCoverageMismatch => {
                formatter.write_str("sideboard policy coverage declaration is not supported")
            }
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
    postboard_game_index_minimum: u8,
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
    // Drawn games do not count toward either player's two required wins, so
    // a best-of-three match can have a fourth or later physical game. Every
    // physical game after the first remains a postboard game.
    if game_index < 2 {
        return Err(SideboardErrorV1::InvalidPostboardGameIndex { actual: game_index });
    }
    Ok(())
}

fn canonicalize_card_counts_v1(rows: &mut [CardCountV1]) -> Result<(), SideboardErrorV1> {
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

/// Per-card copies at full width: an admitted zone holds at most
/// `LIMITED40_MAX_MAINBOARD_SIZE_V1` plus a sideboard, far below `u32::MAX`,
/// so no count can overflow (a `u8` wrapped at 256 copies).
fn counts_from_cards_v1(cards: &[u16]) -> BTreeMap<u16, u32> {
    let mut counts = BTreeMap::new();
    for card_id in cards {
        *counts.entry(*card_id).or_insert(0) += 1;
    }
    counts
}

fn counts_to_rows_v1(counts: &BTreeMap<u16, u32>) -> Vec<CardCountV1> {
    counts
        .iter()
        .map(|(card_id, count)| CardCountV1 {
            card_id: *card_id,
            count: u8::try_from(*count).unwrap_or_else(|_| {
                panic!(
                    "card {card_id} has {count} copies, more than a CardCountV1 row holds; use combined_card_count_map_v1"
                )
            }),
        })
        .collect()
}

fn expand_counts_v1(counts: &BTreeMap<u16, u32>) -> Vec<u16> {
    let mut cards = Vec::new();
    for (card_id, count) in counts {
        let count = usize::try_from(*count).expect("a card count fits in usize");
        cards.extend(std::iter::repeat_n(*card_id, count));
    }
    cards
}

fn remove_count_v1(
    counts: &mut BTreeMap<u16, u32>,
    row: CardCountV1,
    zone: SideboardZoneV1,
) -> Result<(), SideboardErrorV1> {
    let requested = u32::from(row.count);
    let available = counts.get(&row.card_id).copied().unwrap_or(0);
    if available < requested {
        return Err(SideboardErrorV1::CardUnavailable {
            zone,
            card_id: row.card_id,
            requested: row.count,
            available,
        });
    }
    if available == requested {
        counts.remove(&row.card_id);
    } else {
        counts.insert(row.card_id, available - requested);
    }
    Ok(())
}

fn add_count_v1(counts: &mut BTreeMap<u16, u32>, row: CardCountV1) {
    *counts.entry(row.card_id).or_insert(0) += u32::from(row.count);
}

/// Hashes a bare mainboard `card_id` slice with the exact same domain tag
/// and convention `DeckConfigurationV1::mainboard_sha256_v1` uses
/// (`configuration_zone_sha256_v1(b"mainboard", ...)`), for call sites that
/// have a resolved mainboard but not a full `DeckConfigurationV1` (no
/// paired sideboard to construct one), such as
/// `sideboard_search_campaign_v1::resolve_bo3_game_mainboards_v1`'s
/// opponent-side and carried-forward-checked-in branches (fix round 2,
/// item 1: the BO3 trace's mainboard hashes must be the same convention as
/// `AppliedSideboardReceiptV1.after_mainboard_sha256`, not an ad hoc third
/// hash). Byte-identical to `DeckConfigurationV1::mainboard_sha256_v1()`
/// for the same card sequence, since both call the same private helper.
pub(crate) fn mainboard_slice_sha256_v1(mainboard: &[u16]) -> [u8; 32] {
    configuration_zone_sha256_v1(b"mainboard", mainboard)
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

#[cfg(test)]
mod deck_format_tests {
    //! `DeckFormatV1`: the absent format is the exact 60/15 rule; the
    //! Limited format admits 40 or more mainboard cards and a sideboard of 0
    //! to 15, and changes nothing else.

    use super::*;

    const LIMITED: Option<DeckFormatV1> = Some(DeckFormatV1::Limited40V1);

    fn burn() -> (Vec<u16>, Vec<u16>) {
        let deck = checked_in_pauper_registered_deck_by_id_v1("Burn").unwrap();
        let configuration = deck.registered_configuration();
        (
            configuration.mainboard().to_vec(),
            configuration.sideboard().to_vec(),
        )
    }

    fn is_basic(card_id: u16) -> bool {
        crate::card_def::CARD_DEFS[usize::from(card_id)]
            .supertypes
            .contains(&crate::card_def::Supertype::Basic)
    }

    #[test]
    fn deck_format_absent_keeps_the_exact_60_15_rule() {
        let (main, side) = burn();
        let forty = main[..40].to_vec();
        let error =
            RegisteredDeckV1::new_executable_v1("burn40", forty.clone(), Vec::new()).unwrap_err();
        assert_eq!(error, SideboardErrorV1::WrongMainboardSize { actual: 40 });
        assert!(
            error.to_string().contains("must contain exactly 60 cards"),
            "{error}"
        );
        assert_eq!(
            RegisteredDeckV1::new_executable_for_format_v1("burn40", forty, Vec::new(), None)
                .unwrap_err(),
            error
        );
        assert_eq!(
            RegisteredDeckV1::new_executable_for_format_v1(
                "burn",
                main.clone(),
                side[..14].to_vec(),
                None
            )
            .unwrap_err(),
            SideboardErrorV1::WrongSideboardSize { actual: 14 }
        );
        let exact =
            RegisteredDeckV1::new_executable_v1("burn", main.clone(), side.clone()).unwrap();
        let spelled =
            RegisteredDeckV1::new_executable_for_format_v1("burn", main, side, None).unwrap();
        assert_eq!(spelled, exact);
        assert_eq!(exact.registered_configuration().format_v1(), None);
    }

    #[test]
    fn deck_format_limited40_admits_forty_or_more_cards_and_a_short_sideboard() {
        let (main, side) = burn();
        for main_len in [40, 41, 60] {
            for side_len in [0, 1, 15] {
                let deck = RegisteredDeckV1::new_executable_for_format_v1(
                    "burn",
                    main[..main_len].to_vec(),
                    side[..side_len].to_vec(),
                    LIMITED,
                )
                .unwrap();
                let configuration = deck.registered_configuration();
                assert_eq!(configuration.mainboard().len(), main_len);
                assert_eq!(configuration.sideboard().len(), side_len);
                assert_eq!(configuration.format_v1(), LIMITED);
            }
        }
    }

    #[test]
    fn deck_format_limited40_refuses_39_cards_and_a_16_card_sideboard() {
        let (main, side) = burn();
        let error = RegisteredDeckV1::new_executable_for_format_v1(
            "burn39",
            main[..39].to_vec(),
            Vec::new(),
            LIMITED,
        )
        .unwrap_err();
        assert_eq!(
            error,
            SideboardErrorV1::FormatMainboardSize {
                format: DeckFormatV1::Limited40V1,
                actual: 39
            }
        );
        assert_eq!(
            error.to_string(),
            "mainboard must contain 40 to 10000 cards under limited40-v1, got 39"
        );
        assert_eq!(
            mainboard_size_error_v1(LIMITED, 39).unwrap(),
            "must contain 40 to 10000 cards under limited40-v1, got 39"
        );
        assert!(RegisteredDeckV1::new_executable_for_format_v1(
            "burn",
            Vec::new(),
            Vec::new(),
            LIMITED
        )
        .is_err());
        let mut sixteen = side.clone();
        sixteen.push(main[0]);
        assert_eq!(
            RegisteredDeckV1::new_executable_for_format_v1(
                "burn",
                main[..40].to_vec(),
                sixteen,
                LIMITED
            )
            .unwrap_err(),
            SideboardErrorV1::FormatSideboardSize {
                format: DeckFormatV1::Limited40V1,
                actual: 16
            }
        );
        assert_eq!(
            SideboardErrorV1::FormatSideboardSize {
                format: DeckFormatV1::Limited40V1,
                actual: 16
            }
            .to_string(),
            "sideboard must contain at most 15 cards under limited40-v1, got 16"
        );
        assert!(mainboard_size_error_v1(LIMITED, LIMITED40_MAX_MAINBOARD_SIZE_V1 + 1).is_some());
        assert_eq!(mainboard_size_error_v1(LIMITED, 40), None);
        assert_eq!(mainboard_size_error_v1(None, 60), None);
        assert!(mainboard_size_error_v1(None, 40)
            .unwrap()
            .contains("must contain exactly 60 cards"));
    }

    /// The format is a size rule only: support, token and copy-limit checks
    /// still apply to a 40-card deck.
    #[test]
    fn deck_format_limited40_keeps_every_other_registration_check() {
        let (main, _) = burn();
        let basic = *main.iter().find(|&&id| is_basic(id)).unwrap();
        let nonbasic = *main.iter().find(|&&id| !is_basic(id)).unwrap();
        let mut five_copies = vec![basic; 35];
        five_copies.extend([nonbasic; 5]);
        assert_eq!(
            RegisteredDeckV1::new_executable_for_format_v1(
                "five",
                five_copies,
                Vec::new(),
                LIMITED
            )
            .unwrap_err(),
            SideboardErrorV1::NonbasicCopyLimitExceeded {
                card_id: nonbasic,
                count: 5
            }
        );
        let mut unknown = main[..40].to_vec();
        unknown[0] = u16::MAX;
        assert!(RegisteredDeckV1::new_executable_for_format_v1(
            "unknown",
            unknown,
            Vec::new(),
            LIMITED
        )
        .is_err());
    }

    /// A plan applied to a Limited registration is admitted under the same
    /// rule, so its result may keep a 40-card mainboard.
    #[test]
    fn deck_format_limited40_plan_keeps_the_registration_format() {
        let (main, side) = burn();
        let deck = RegisteredDeckV1::new_executable_for_format_v1(
            "burn",
            main[..40].to_vec(),
            side[..2].to_vec(),
            LIMITED,
        )
        .unwrap();
        let configuration = deck.registered_configuration();
        let cards_out = vec![CardCountV1 {
            card_id: configuration.mainboard()[0],
            count: 1,
        }];
        let cards_in = vec![CardCountV1 {
            card_id: configuration.sideboard()[0],
            count: 1,
        }];
        let plan = SideboardPlanV1::new_v1("burn", "other", 2, cards_in, cards_out).unwrap();
        let (after, _) = deck.apply_plan_v1(&plan, "policy", [0; 32]).unwrap();
        assert_eq!(after.mainboard().len(), 40);
        assert_eq!(after.sideboard().len(), 2);
        assert_eq!(after.format_v1(), LIMITED);
    }

    #[test]
    fn deck_format_names_only_limited40_v1() {
        assert_eq!(
            serde_json::to_value(DeckFormatV1::Limited40V1).unwrap(),
            serde_json::json!("limited40-v1")
        );
        assert_eq!(
            serde_json::from_value::<DeckFormatV1>(serde_json::json!("limited40-v1")).unwrap(),
            DeckFormatV1::Limited40V1
        );
        assert_eq!(DeckFormatV1::Limited40V1.name_v1(), "limited40-v1");
        for bad in [
            "limited40-v2",
            "Limited40V1",
            "limited40",
            "",
            "constructed60-v1",
        ] {
            assert!(serde_json::from_value::<DeckFormatV1>(serde_json::json!(bad)).is_err());
        }
    }

    fn card(name: &str) -> u16 {
        crate::card_def::card_id_by_name(name).unwrap_or_else(|| panic!("{name}"))
    }

    /// Review finding 1: per-card counts are full width. A `u8` count
    /// wrapped at 256 copies, so 256 copies of a nonbasic counted as zero
    /// and passed the four-copy limit in release builds (and panicked in
    /// checked builds). Each case here was admitted or panicked before.
    #[test]
    fn deck_format_limited40_copy_limit_counts_past_255_copies() {
        let bolt = card("Lightning Bolt");
        let mountain = card("Mountain");
        assert!(!is_basic(bolt) && is_basic(mountain));
        for copies in [255_u32, 256, 260, 511, 512, 1_000] {
            let mut mainboard = vec![bolt; copies as usize];
            mainboard.extend([mountain; 40]);
            assert_eq!(
                RegisteredDeckV1::new_executable_for_format_v1(
                    "bolts",
                    mainboard,
                    Vec::new(),
                    LIMITED
                )
                .unwrap_err(),
                SideboardErrorV1::NonbasicCopyLimitExceeded {
                    card_id: bolt,
                    count: copies
                },
                "{copies} copies"
            );
        }
        // Split across the zones, the limit still sees the whole multiset.
        let mut mainboard = vec![bolt; 250];
        mainboard.extend([mountain; 40]);
        let error = RegisteredDeckV1::new_executable_for_format_v1(
            "bolts",
            mainboard,
            vec![bolt; 6],
            LIMITED,
        )
        .unwrap_err();
        assert_eq!(
            error,
            SideboardErrorV1::NonbasicCopyLimitExceeded {
                card_id: bolt,
                count: 256
            }
        );
        assert!(error.to_string().contains("256"), "{error}");
        // Basic lands remain the exception at any count.
        let deck = RegisteredDeckV1::new_executable_for_format_v1(
            "mountains",
            vec![mountain; 256],
            vec![mountain; 15],
            LIMITED,
        )
        .unwrap();
        assert_eq!(
            deck.registered_configuration()
                .combined_card_count_map_v1()
                .into_iter()
                .collect::<Vec<_>>(),
            vec![(mountain, 271)]
        );
    }

    /// Review finding 1: the registered-multiset comparison is exact. With
    /// `u8` counts, 40 Forests and 296 Forests (296 = 40 + 256) compared as
    /// the same multiset.
    #[test]
    fn deck_format_limited40_multiset_counts_are_exact() {
        let forest = card("Forest");
        let configuration = |count: usize| {
            DeckConfigurationV1::new_for_format_v1(vec![forest; count], Vec::new(), LIMITED)
                .unwrap()
        };
        let forty = configuration(40);
        for other in [296, 552, 41] {
            assert_ne!(
                configuration(other).combined_card_count_map_v1(),
                forty.combined_card_count_map_v1(),
                "{other} Forests"
            );
        }
        assert_eq!(
            configuration(296).combined_card_count_map_v1()[&forest],
            296
        );
        // Within 255 copies the row form is the exact map, as before.
        assert_eq!(
            forty.combined_card_counts_v1(),
            vec![CardCountV1 {
                card_id: forest,
                count: 40
            }]
        );
    }

    /// The row form cannot hold 256 copies: it refuses loudly rather than
    /// wrapping. Only a deck-format configuration can hold that many, and the
    /// trainer's checks on those read `combined_card_count_map_v1`.
    #[test]
    #[should_panic(expected = "more than a CardCountV1 row holds")]
    fn deck_format_limited40_row_counts_refuse_to_wrap() {
        DeckConfigurationV1::new_for_format_v1(vec![card("Forest"); 256], Vec::new(), LIMITED)
            .unwrap()
            .combined_card_counts_v1();
    }

    /// Review finding 1: the format's upper bound, 10,000 mainboard cards,
    /// with a full sideboard; one more card is refused. A plan applied to the
    /// largest registration conserves its multiset (with `u8` counts, the
    /// 10,000 Mountains counted as 16).
    #[test]
    fn deck_format_limited40_admits_exactly_the_10000_card_boundary() {
        assert_eq!(LIMITED40_MAX_MAINBOARD_SIZE_V1, 10_000);
        let mountain = card("Mountain");
        let forest = card("Forest");
        let deck = RegisteredDeckV1::new_executable_for_format_v1(
            "boundary",
            vec![mountain; 10_000],
            vec![forest; 15],
            LIMITED,
        )
        .unwrap();
        let configuration = deck.registered_configuration();
        assert_eq!(configuration.mainboard().len(), 10_000);
        assert_eq!(configuration.sideboard().len(), 15);
        assert_eq!(
            configuration.combined_card_count_map_v1(),
            BTreeMap::from([(mountain, 10_000), (forest, 15)])
        );
        assert_eq!(
            RegisteredDeckV1::new_executable_for_format_v1(
                "boundary",
                vec![mountain; 10_001],
                Vec::new(),
                LIMITED,
            )
            .unwrap_err(),
            SideboardErrorV1::FormatMainboardSize {
                format: DeckFormatV1::Limited40V1,
                actual: 10_001
            }
        );

        let plan = SideboardPlanV1::new_v1(
            "boundary",
            "other",
            2,
            vec![CardCountV1 {
                card_id: forest,
                count: 3,
            }],
            vec![CardCountV1 {
                card_id: mountain,
                count: 3,
            }],
        )
        .unwrap();
        let (after, _) = deck.apply_plan_v1(&plan, "policy", [0; 32]).unwrap();
        assert_eq!(after.mainboard().len(), 10_000);
        assert_eq!(after.sideboard().len(), 15);
        assert_eq!(
            after
                .mainboard()
                .iter()
                .filter(|&&id| id == mountain)
                .count(),
            9_997
        );
        assert_eq!(
            after.combined_card_count_map_v1(),
            configuration.combined_card_count_map_v1()
        );

        // Exactly 256 copies: a `u8` count read zero available here, so the
        // plan was refused as `CardUnavailable`.
        let wrapped = RegisteredDeckV1::new_executable_for_format_v1(
            "boundary",
            vec![mountain; 256],
            vec![forest; 15],
            LIMITED,
        )
        .unwrap();
        let (after, _) = wrapped.apply_plan_v1(&plan, "policy", [0; 32]).unwrap();
        assert_eq!(after.mainboard().len(), 256);
        assert_eq!(
            after.combined_card_count_map_v1(),
            BTreeMap::from([(mountain, 256), (forest, 15)])
        );
        // A removal beyond what the zone holds is still refused.
        let missing = SideboardPlanV1::new_v1(
            "boundary",
            "other",
            2,
            vec![CardCountV1 {
                card_id: mountain,
                count: 1,
            }],
            vec![CardCountV1 {
                card_id: forest,
                count: 1,
            }],
        )
        .unwrap();
        assert_eq!(
            wrapped
                .apply_plan_v1(&missing, "policy", [0; 32])
                .unwrap_err(),
            SideboardErrorV1::CardUnavailable {
                zone: SideboardZoneV1::RegisteredMainboard,
                card_id: forest,
                requested: 1,
                available: 0,
            }
        );
    }
}
