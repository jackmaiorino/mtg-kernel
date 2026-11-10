//! Eternal best-of-three: format-aware registrations of a deck's 75 cards, a
//! companion-aware game builder and a seeded random sideboarding policy for
//! the `eternal-fixtures` catalog (`docs/design/eternal_formats_support_v1.md`,
//! stage S2). Match state is `crate::bo3_match`'s. Pauper's exact 60/15
//! registrations and its frozen sideboard policy (`crate::sideboard`,
//! `crate::bo3_session`) are separate and unchanged.
//!
//! Ordinary games use the custom-deck session (`limited_session_v1`). The
//! explicit native opening profile returns a GameState for engine commands.
//! Both game paths seat the starting player first and use the prepared game
//! to map session seats back to match players.

use crate::bo3_match::{
    BestOfThreeMatchStateV1, GameOutcomeV1, GameStartV1, MatchStateErrorV1, MatchTransitionV1,
    PlayDrawChoiceV1,
};
use crate::card_def::{card_id_by_name, Supertype, CARD_DEFS};
use crate::ids::PlayerId;
use crate::limited_session_v1::{CustomCardCountV1, CustomDeckV1, MAX_CUSTOM_DECK_CARDS_V1};
use crate::state::GameState;
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

/// The constructed formats of the eternal catalog's metagame snapshots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EternalFormatV1 {
    Pauper,
    Legacy,
    Vintage,
    Premodern,
}

impl EternalFormatV1 {
    /// The format named by a snapshot's `formats` key.
    pub fn from_snapshot_key(key: &str) -> Option<Self> {
        match key {
            "pauper" => Some(Self::Pauper),
            "legacy" => Some(Self::Legacy),
            "vintage" => Some(Self::Vintage),
            "premodern" => Some(Self::Premodern),
            _ => None,
        }
    }

    /// 100.2a: a constructed deck contains at least sixty cards, before and
    /// after sideboarding.
    pub const fn minimum_mainboard(self) -> usize {
        match self {
            Self::Pauper | Self::Legacy | Self::Vintage | Self::Premodern => 60,
        }
    }

    /// 100.4a: a constructed sideboard has at most fifteen cards, a
    /// companion among them.
    pub const fn maximum_sideboard(self) -> usize {
        match self {
            Self::Pauper | Self::Legacy | Self::Vintage | Self::Premodern => 15,
        }
    }

    /// 100.2a and 100.4a: at most four copies of a card other than a basic
    /// land across the deck and its sideboard together. The snapshot decks
    /// hold no card that lifts the limit, and format legality (ban and
    /// restricted lists) is not checked here.
    pub const fn copy_limit(self) -> usize {
        4
    }
}

/// The cards in one game's deck and sideboard, a configuration of a
/// registered 75. Both zones are kept sorted by card id.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EternalConfigurationV1 {
    mainboard: Vec<u16>,
    sideboard: Vec<u16>,
}

impl EternalConfigurationV1 {
    pub fn new_v1(mut mainboard: Vec<u16>, mut sideboard: Vec<u16>) -> Self {
        mainboard.sort_unstable();
        sideboard.sort_unstable();
        Self {
            mainboard,
            sideboard,
        }
    }

    pub fn mainboard(&self) -> &[u16] {
        &self.mainboard
    }

    pub fn sideboard(&self) -> &[u16] {
        &self.sideboard
    }

    /// Copies of each card across both zones.
    pub fn combined_counts(&self) -> BTreeMap<u16, usize> {
        let mut counts = BTreeMap::new();
        for &id in self.mainboard.iter().chain(&self.sideboard) {
            *counts.entry(id).or_insert(0) += 1;
        }
        counts
    }

    /// The cards that moved between zones relative to `registered`: those
    /// now in the mainboard that were in the sideboard, and the reverse.
    pub fn exchanges_from(&self, registered: &EternalConfigurationV1) -> (Vec<u16>, Vec<u16>) {
        (
            multiset_minus(&self.mainboard, &registered.mainboard),
            multiset_minus(&self.sideboard, &registered.sideboard),
        )
    }
}

/// One player's registered 75 for an eternal best-of-three match. The
/// sideboard includes the companion, as tournament decklists list it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EternalRegisteredDeckV1 {
    format: EternalFormatV1,
    deck_id: String,
    registered: EternalConfigurationV1,
    companion: Option<u16>,
    native_opening: bool,
}

impl EternalRegisteredDeckV1 {
    /// Admits a registration: the format's deck size bounds and copy limit,
    /// every card Full and not a token, and a companion that is in the
    /// sideboard, is a supported companion and whose condition the
    /// registered mainboard meets.
    pub fn new_v1(
        format: EternalFormatV1,
        deck_id: impl Into<String>,
        mainboard: Vec<u16>,
        sideboard: Vec<u16>,
        companion: Option<u16>,
    ) -> Result<Self, EternalBo3ErrorV1> {
        Self::register(format, deck_id.into(), mainboard, sideboard, companion, false)
    }

    /// Explicit native pregame profile. In addition to Full definitions, it
    /// admits the three executable Eternal opening cards while their ordinary
    /// catalog admission remains separate. Both match registrations must opt in.
    pub fn new_with_native_opening_v1(
        format: EternalFormatV1,
        deck_id: impl Into<String>,
        mainboard: Vec<u16>,
        sideboard: Vec<u16>,
        companion: Option<u16>,
    ) -> Result<Self, EternalBo3ErrorV1> {
        if !cfg!(feature = "eternal-fixtures") {
            return Err(EternalBo3ErrorV1::NativeOpeningSetup {
                reason: "the native opening profile requires the Eternal catalog".into(),
            });
        }
        Self::register(format, deck_id.into(), mainboard, sideboard, companion, true)
    }

    fn register(
        format: EternalFormatV1,
        deck_id: String,
        mainboard: Vec<u16>,
        sideboard: Vec<u16>,
        companion: Option<u16>,
        native_opening: bool,
    ) -> Result<Self, EternalBo3ErrorV1> {
        if deck_id.is_empty() {
            return Err(EternalBo3ErrorV1::EmptyDeckId);
        }
        check_bounds(format, mainboard.len(), sideboard.len())?;
        let registered = EternalConfigurationV1::new_v1(mainboard, sideboard);
        for &id in registered.mainboard.iter().chain(&registered.sideboard) {
            let card = CARD_DEFS
                .get(usize::from(id))
                .ok_or(EternalBo3ErrorV1::UnknownCardId { card_id: id })?;
            if native_opening && crate::leyline_axe_v1::definition(id) {
                return Err(EternalBo3ErrorV1::NativeOpeningSetup {
                    reason: "Leyline Axe belongs to its own opening profile".into(),
                });
            }
            let opening_card = native_opening
                && card.is_executable()
                && matches!(card.name, "Leyline of the Void" | "Leyline of Sanctity" | "Chancellor of the Annex");
            if card.is_token || !(card.has_full_support() || opening_card) {
                return Err(EternalBo3ErrorV1::UnsupportedCard {
                    name: card.name.to_owned(),
                });
            }
        }
        for (&id, &count) in &registered.combined_counts() {
            let card = &CARD_DEFS[usize::from(id)];
            if count > format.copy_limit() && !card.supertypes.contains(&Supertype::Basic) {
                return Err(EternalBo3ErrorV1::CopyLimitExceeded {
                    name: card.name.to_owned(),
                    count,
                });
            }
        }
        if let Some(companion) = companion {
            let name = CARD_DEFS
                .get(usize::from(companion))
                .ok_or(EternalBo3ErrorV1::UnknownCardId { card_id: companion })?
                .name
                .to_owned();
            if !registered.sideboard.contains(&companion) {
                return Err(EternalBo3ErrorV1::CompanionNotInSideboard { name });
            }
            match crate::eternal_lurrus_v1::companion_condition_met(
                companion,
                &registered.mainboard,
            ) {
                Some(true) => {}
                Some(false) => return Err(EternalBo3ErrorV1::CompanionConditionNotMet { name }),
                None => return Err(EternalBo3ErrorV1::NotACompanion { name }),
            }
        }
        Ok(Self {
            format,
            deck_id,
            registered,
            companion,
            native_opening,
        })
    }

    /// A registration from decklist rows by card name. A card may take
    /// several rows of one zone; their counts add.
    pub fn from_rows_v1(
        format: EternalFormatV1,
        deck_id: impl Into<String>,
        mainboard: &[CustomCardCountV1],
        sideboard: &[CustomCardCountV1],
        companion: Option<&str>,
    ) -> Result<Self, EternalBo3ErrorV1> {
        check_bounds(format, row_count(mainboard), row_count(sideboard))?;
        let companion = companion
            .map(|name| {
                card_id_by_name(name).ok_or_else(|| EternalBo3ErrorV1::UnknownCard {
                    name: name.to_owned(),
                })
            })
            .transpose()?;
        Self::new_v1(
            format,
            deck_id,
            resolve_rows(mainboard)?,
            resolve_rows(sideboard)?,
            companion,
        )
    }

    pub fn format(&self) -> EternalFormatV1 {
        self.format
    }

    pub fn deck_id(&self) -> &str {
        &self.deck_id
    }

    pub fn registered_configuration(&self) -> &EternalConfigurationV1 {
        &self.registered
    }

    pub fn companion(&self) -> Option<u16> {
        self.companion
    }

    /// Admits a configuration for game `game_index`: the registered 75
    /// exactly, within the format's bounds. Game one uses the registered
    /// deck and sideboard unchanged.
    pub fn admit_configuration_v1(
        &self,
        game_index: u8,
        configuration: &EternalConfigurationV1,
    ) -> Result<(), EternalBo3ErrorV1> {
        if configuration.combined_counts() != self.registered.combined_counts() {
            return Err(EternalBo3ErrorV1::RegisteredCardsChanged {
                deck_id: self.deck_id.clone(),
            });
        }
        check_bounds(
            self.format,
            configuration.mainboard.len(),
            configuration.sideboard.len(),
        )?;
        if game_index == 1 && configuration != &self.registered {
            return Err(EternalBo3ErrorV1::GameOneSideboarded {
                deck_id: self.deck_id.clone(),
            });
        }
        Ok(())
    }

    /// The deck a game uses: the configuration's mainboard, and the
    /// registered companion when it is still in the sideboard and the
    /// mainboard meets its condition (702.139a). Otherwise the companion is
    /// an ordinary sideboard card for this game. The sideboard's other cards
    /// are outside the game.
    pub fn game_deck_v1(
        &self,
        game_index: u8,
        configuration: &EternalConfigurationV1,
    ) -> Result<EternalGameDeckV1, EternalBo3ErrorV1> {
        self.admit_configuration_v1(game_index, configuration)?;
        let companion = self.companion.filter(|&companion| {
            configuration.sideboard.contains(&companion)
                && crate::eternal_lurrus_v1::companion_condition_met(
                    companion,
                    &configuration.mainboard,
                ) == Some(true)
        });
        let mut outside_the_game = configuration.sideboard.clone();
        if let Some(companion) = companion {
            let position = outside_the_game
                .iter()
                .position(|&id| id == companion)
                .expect("the companion is in the sideboard");
            outside_the_game.remove(position);
        }
        Ok(EternalGameDeckV1 {
            deck: CustomDeckV1 {
                cards: rows_from_ids(&configuration.mainboard),
                companion: companion.map(|id| CARD_DEFS[usize::from(id)].name.to_owned()),
            },
            outside_the_game,
        })
    }
}

/// One player's deck for one game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EternalGameDeckV1 {
    /// The session deck: the mainboard, by card id order, and the companion
    /// when it is used this game.
    pub deck: CustomDeckV1,
    /// The sideboard's cards other than a companion in use, sorted by id.
    pub outside_the_game: Vec<u16>,
}

/// A game ready to play: its start, both configurations and both game
/// decks, indexed by match player.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EternalPreparedGameV1 {
    start: GameStartV1,
    configurations: [EternalConfigurationV1; 2],
    game_decks: [EternalGameDeckV1; 2],
}

impl EternalPreparedGameV1 {
    pub fn start(&self) -> GameStartV1 {
        self.start
    }

    pub fn configuration(&self, player: PlayerId) -> &EternalConfigurationV1 {
        &self.configurations[player.index()]
    }

    pub fn game_deck(&self, player: PlayerId) -> &EternalGameDeckV1 {
        &self.game_decks[player.index()]
    }

    /// The match player in session seat `seat`: the starting player sits in
    /// seat 0.
    pub fn player_in_seat(&self, seat: usize) -> PlayerId {
        match seat {
            0 => self.start.starting_player,
            1 => self.start.starting_player.opponent(),
            _ => panic!("a two-player session has seats 0 and 1, not {seat}"),
        }
    }

    /// Both session decks in seat order, the starting player's first.
    pub fn session_decks(&self) -> [CustomDeckV1; 2] {
        [0, 1].map(|seat| self.game_deck(self.player_in_seat(seat)).deck.clone())
    }

    /// Both seats' cards outside the game as decklist rows, in seat order:
    /// the session reset's `outside_the_game`.
    pub fn session_outside_the_game(&self) -> [Vec<CustomCardCountV1>; 2] {
        [0, 1]
            .map(|seat| rows_from_ids(&self.game_deck(self.player_in_seat(seat)).outside_the_game))
    }
}

/// An eternal best-of-three match: two registrations of one format and the
/// match state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EternalBestOfThreeMatchV1 {
    decks: [EternalRegisteredDeckV1; 2],
    state: BestOfThreeMatchStateV1,
}

impl EternalBestOfThreeMatchV1 {
    pub fn new_v1(
        decks: [EternalRegisteredDeckV1; 2],
        game_one_chooser: PlayerId,
    ) -> Result<Self, EternalBo3ErrorV1> {
        if decks[0].format != decks[1].format {
            return Err(EternalBo3ErrorV1::FormatMismatch);
        }
        if decks[0].native_opening != decks[1].native_opening {
            return Err(EternalBo3ErrorV1::OpeningProfileMismatch);
        }
        Ok(Self {
            decks,
            state: BestOfThreeMatchStateV1::new_v1(game_one_chooser)?,
        })
    }

    pub fn deck(&self, player: PlayerId) -> &EternalRegisteredDeckV1 {
        &self.decks[player.index()]
    }

    pub fn match_state(&self) -> &BestOfThreeMatchStateV1 {
        &self.state
    }

    /// Starts the next game with the chooser's play/draw choice and each
    /// player's configuration. Both configurations are admitted before the
    /// match advances, so a refused configuration changes nothing.
    pub fn start_game_v1(
        &mut self,
        chooser: PlayerId,
        choice: PlayDrawChoiceV1,
        configurations: [EternalConfigurationV1; 2],
    ) -> Result<EternalPreparedGameV1, EternalBo3ErrorV1> {
        if self.decks[0].native_opening {
            return Err(EternalBo3ErrorV1::OpeningProfileMismatch);
        }
        let (next, game) = self.prepare_game(chooser, choice, configurations)?;
        self.state = next;
        Ok(game)
    }

    /// Start a game under the two registrations' native opening profile.
    /// Use `engine::advance_until_decision` and `engine::step` on the returned
    /// state for London mulligans, the kept-hand choices and the regular game.
    /// The prepared game maps native seat P0 to the match's starting player.
    pub fn start_game_with_native_opening_v1(
        &mut self,
        chooser: PlayerId,
        choice: PlayDrawChoiceV1,
        configurations: [EternalConfigurationV1; 2],
        seed: u64,
    ) -> Result<(EternalPreparedGameV1, GameState), EternalBo3ErrorV1> {
        if !self.decks[0].native_opening {
            return Err(EternalBo3ErrorV1::OpeningProfileMismatch);
        }
        let (next, game) = self.prepare_game(chooser, choice, configurations)?;
        let decks = [0, 1].map(|seat| game.configuration(game.player_in_seat(seat)).mainboard());
        let mut state = crate::rl::deal_validated_deck_pair_v1(seed, decks, PlayerId::P0);
        crate::combat_damage_v1::enable_foundations_combat_v1(&mut state)
            .and_then(|()| crate::eternal_opening_v1::enable_eternal_opening_v1(&mut state))
            .map_err(|reason| EternalBo3ErrorV1::NativeOpeningSetup { reason })?;
        // Keep the native deal and London ownership ahead of companion exile.
        let companions = [0, 1].map(|seat| {
            game.game_deck(game.player_in_seat(seat)).deck.companion.as_deref().map(|name| {
                card_id_by_name(name).expect("prepared companion has a registered definition")
            })
        });
        crate::eternal_lurrus_v1::add_companions(&mut state, companions);
        let outside = [0, 1].map(|seat| game.game_deck(game.player_in_seat(seat)).outside_the_game.clone());
        crate::eternal_outside_game_v1::set_outside_the_game(&mut state, outside);
        self.state = next;
        Ok((game, state))
    }

    fn prepare_game(
        &self,
        chooser: PlayerId,
        choice: PlayDrawChoiceV1,
        configurations: [EternalConfigurationV1; 2],
    ) -> Result<(BestOfThreeMatchStateV1, EternalPreparedGameV1), EternalBo3ErrorV1> {
        let mut next = self.state.clone();
        let start = next.choose_play_draw_v1(chooser, choice)?;
        let game_decks = [
            self.decks[0].game_deck_v1(start.game_index, &configurations[0])?,
            self.decks[1].game_deck_v1(start.game_index, &configurations[1])?,
        ];
        Ok((next, EternalPreparedGameV1 {
            start,
            configurations,
            game_decks,
        }))
    }

    pub fn record_game_result_v1(
        &mut self,
        outcome: GameOutcomeV1,
    ) -> Result<MatchTransitionV1, EternalBo3ErrorV1> {
        self.state
            .record_game_result_v1(outcome)
            .map_err(Into::into)
    }
}

/// A seeded random sideboarding policy for catalog tests: before games two
/// and three each player exchanges up to `max_exchanges` random sideboard
/// cards, never the companion, for random mainboard cards, starting from the
/// registered configuration every time. It is test scaffolding that makes
/// sideboard cards reach play, not a playing policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RandomSideboardPolicyV1 {
    pub seed: u64,
    pub max_exchanges: usize,
}

impl RandomSideboardPolicyV1 {
    pub fn configure_v1(
        &self,
        deck: &EternalRegisteredDeckV1,
        player: PlayerId,
        game_index: u8,
    ) -> EternalConfigurationV1 {
        let registered = deck.registered_configuration();
        if game_index <= 1 {
            return registered.clone();
        }
        let mut state = self.seed
            ^ (u64::from(player.0) << 56)
            ^ (u64::from(game_index) << 48)
            ^ 0x5349_4445_424f_4152;
        let mut mainboard = registered.mainboard.clone();
        let mut sideboard = registered.sideboard.clone();
        // Sideboard positions that may come in: every card but the companion.
        let mut eligible: Vec<usize> = (0..sideboard.len())
            .filter(|&index| Some(sideboard[index]) != deck.companion)
            .collect();
        let most = self.max_exchanges.min(eligible.len()).min(mainboard.len());
        let exchanges = (splitmix64(&mut state) % (most as u64 + 1)) as usize;
        let mut mainboard_positions: Vec<usize> = (0..mainboard.len()).collect();
        for _ in 0..exchanges {
            let side =
                eligible.swap_remove((splitmix64(&mut state) % eligible.len() as u64) as usize);
            let main = mainboard_positions
                .swap_remove((splitmix64(&mut state) % mainboard_positions.len() as u64) as usize);
            std::mem::swap(&mut mainboard[main], &mut sideboard[side]);
        }
        EternalConfigurationV1::new_v1(mainboard, sideboard)
    }
}

/// Why a registration, configuration or match step was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EternalBo3ErrorV1 {
    EmptyDeckId,
    UnknownCard { name: String },
    UnknownCardId { card_id: u16 },
    ZeroCopies { name: String },
    UnsupportedCard { name: String },
    MainboardTooSmall { actual: usize, minimum: usize },
    MainboardTooLarge { actual: usize, maximum: usize },
    SideboardTooLarge { actual: usize, maximum: usize },
    CopyLimitExceeded { name: String, count: usize },
    CompanionNotInSideboard { name: String },
    CompanionConditionNotMet { name: String },
    NotACompanion { name: String },
    RegisteredCardsChanged { deck_id: String },
    GameOneSideboarded { deck_id: String },
    FormatMismatch,
    OpeningProfileMismatch,
    NativeOpeningSetup { reason: String },
    Match(MatchStateErrorV1),
}

impl From<MatchStateErrorV1> for EternalBo3ErrorV1 {
    fn from(error: MatchStateErrorV1) -> Self {
        Self::Match(error)
    }
}

impl fmt::Display for EternalBo3ErrorV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyDeckId => f.write_str("the deck id is empty"),
            Self::UnknownCard { name } => write!(f, "unknown card {name:?}"),
            Self::UnknownCardId { card_id } => write!(f, "unknown card id {card_id}"),
            Self::ZeroCopies { name } => write!(f, "zero copies of {name:?}"),
            Self::UnsupportedCard { name } => {
                write!(f, "card {name:?} is a token or lacks full engine support")
            }
            Self::MainboardTooSmall { actual, minimum } => {
                write!(f, "the mainboard has {actual} cards, fewer than {minimum}")
            }
            Self::MainboardTooLarge { actual, maximum } => {
                write!(
                    f,
                    "the mainboard has {actual} cards, above the {maximum}-card session limit"
                )
            }
            Self::SideboardTooLarge { actual, maximum } => {
                write!(f, "the sideboard has {actual} cards, more than {maximum}")
            }
            Self::CopyLimitExceeded { name, count } => {
                write!(f, "{count} copies of {name:?} exceed the copy limit")
            }
            Self::CompanionNotInSideboard { name } => {
                write!(f, "companion {name:?} is not in the sideboard")
            }
            Self::CompanionConditionNotMet { name } => write!(
                f,
                "the registered mainboard does not meet companion {name:?}'s condition"
            ),
            Self::NotACompanion { name } => write!(f, "{name:?} is not a supported companion"),
            Self::RegisteredCardsChanged { deck_id } => {
                write!(
                    f,
                    "a configuration of {deck_id:?} changes its registered 75 cards"
                )
            }
            Self::GameOneSideboarded { deck_id } => {
                write!(f, "game one of {deck_id:?} must use the registered deck")
            }
            Self::FormatMismatch => f.write_str("the two registrations are of different formats"),
            Self::OpeningProfileMismatch => f.write_str("both registrations and the game start must use the same opening profile"),
            Self::NativeOpeningSetup { reason } => write!(f, "native opening setup: {reason}"),
            Self::Match(error) => write!(f, "match state: {error}"),
        }
    }
}

impl Error for EternalBo3ErrorV1 {}

fn check_bounds(
    format: EternalFormatV1,
    mainboard: usize,
    sideboard: usize,
) -> Result<(), EternalBo3ErrorV1> {
    if mainboard < format.minimum_mainboard() {
        return Err(EternalBo3ErrorV1::MainboardTooSmall {
            actual: mainboard,
            minimum: format.minimum_mainboard(),
        });
    }
    if mainboard > MAX_CUSTOM_DECK_CARDS_V1 {
        return Err(EternalBo3ErrorV1::MainboardTooLarge {
            actual: mainboard,
            maximum: MAX_CUSTOM_DECK_CARDS_V1,
        });
    }
    if sideboard > format.maximum_sideboard() {
        return Err(EternalBo3ErrorV1::SideboardTooLarge {
            actual: sideboard,
            maximum: format.maximum_sideboard(),
        });
    }
    Ok(())
}

fn row_count(rows: &[CustomCardCountV1]) -> usize {
    rows.iter().fold(0usize, |total, row| {
        total.saturating_add(row.count as usize)
    })
}

fn resolve_rows(rows: &[CustomCardCountV1]) -> Result<Vec<u16>, EternalBo3ErrorV1> {
    let mut ids = Vec::new();
    for row in rows {
        if row.count == 0 {
            return Err(EternalBo3ErrorV1::ZeroCopies {
                name: row.name.clone(),
            });
        }
        let id = card_id_by_name(&row.name).ok_or_else(|| EternalBo3ErrorV1::UnknownCard {
            name: row.name.clone(),
        })?;
        ids.extend(std::iter::repeat_n(id, row.count as usize));
    }
    Ok(ids)
}

/// Decklist rows for sorted card ids, one row per card in id order.
fn rows_from_ids(ids: &[u16]) -> Vec<CustomCardCountV1> {
    let mut rows: Vec<CustomCardCountV1> = Vec::new();
    for &id in ids {
        let name = CARD_DEFS[usize::from(id)].name;
        match rows.last_mut() {
            Some(row) if row.name == name => row.count += 1,
            _ => rows.push(CustomCardCountV1 {
                name: name.to_owned(),
                count: 1,
            }),
        }
    }
    rows
}

/// The cards of sorted `cards` beyond those of sorted `other`, as a sorted
/// multiset difference.
fn multiset_minus(cards: &[u16], other: &[u16]) -> Vec<u16> {
    let mut remaining: BTreeMap<u16, usize> = BTreeMap::new();
    for &id in other {
        *remaining.entry(id).or_insert(0) += 1;
    }
    let mut difference = Vec::new();
    for &id in cards {
        match remaining.get_mut(&id) {
            Some(count) if *count > 0 => *count -= 1,
            _ => difference.push(id),
        }
    }
    difference
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}
