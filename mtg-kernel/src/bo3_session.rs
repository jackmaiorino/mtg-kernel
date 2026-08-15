//! Deterministic composition of registered decks, sideboarding, and BO3 state.
//!
//! Each prepared game carries an exact 60-card configuration for both seats
//! plus the play/draw result. Game 1 uses the registered mainboards; every
//! later physical game independently applies the versioned sideboard policy
//! to the original registered 75. Preparing a game is transactional: policy
//! failure leaves the match phase unchanged.

use crate::bo3_match::{
    BestOfThreeMatchStateV1, GameOutcomeV1, GameStartV1, MatchStateErrorV1, MatchTransitionV1,
    PlayDrawChoiceV1,
};
use crate::ids::PlayerId;
use crate::sideboard::{
    checked_in_pauper_registered_deck_by_id_v1, AppliedSideboardReceiptV1, DeckConfigurationV1,
    DeterministicSideboardPolicyV1, RegisteredDeckV1, SideboardErrorV1,
};
use std::error::Error;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedMatchGameV1 {
    start: GameStartV1,
    configurations: [DeckConfigurationV1; 2],
    sideboard_receipts: [Option<AppliedSideboardReceiptV1>; 2],
}

impl PreparedMatchGameV1 {
    pub fn start(&self) -> GameStartV1 {
        self.start
    }

    pub fn configuration(&self, player: PlayerId) -> Option<&DeckConfigurationV1> {
        self.configurations.get(player.index())
    }

    pub fn sideboard_receipt(&self, player: PlayerId) -> Option<&AppliedSideboardReceiptV1> {
        self.sideboard_receipts
            .get(player.index())
            .and_then(Option::as_ref)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BestOfThreeDeckMatchV1 {
    registered_decks: [RegisteredDeckV1; 2],
    sideboard_policy: DeterministicSideboardPolicyV1,
    match_state: BestOfThreeMatchStateV1,
}

impl BestOfThreeDeckMatchV1 {
    /// Creates an executable match from two completed checked-in Pauper
    /// registrations and the versioned checked-in sideboard policy.
    pub fn checked_in_pauper_v1(
        p0_deck_id: &str,
        p1_deck_id: &str,
        game_one_chooser: PlayerId,
    ) -> Result<Self, Bo3SessionErrorV1> {
        let p0 = checked_in_pauper_registered_deck_by_id_v1(p0_deck_id)?;
        let p1 = checked_in_pauper_registered_deck_by_id_v1(p1_deck_id)?;
        let policy = DeterministicSideboardPolicyV1::checked_in_pauper_v1()?;
        Self::new_v1(p0, p1, policy, game_one_chooser)
    }

    pub fn new_v1(
        p0: RegisteredDeckV1,
        p1: RegisteredDeckV1,
        sideboard_policy: DeterministicSideboardPolicyV1,
        game_one_chooser: PlayerId,
    ) -> Result<Self, Bo3SessionErrorV1> {
        if p0.deck_id() == p1.deck_id()
            && p0.registered_75_sha256_v1() != p1.registered_75_sha256_v1()
        {
            return Err(Bo3SessionErrorV1::MirrorRegistrationMismatch {
                deck_id: p0.deck_id().to_owned(),
            });
        }
        // Validate both ordered matchup keys before accepting the session.
        sideboard_policy.plan_for_v1(p0.deck_id(), p1.deck_id(), 2)?;
        sideboard_policy.plan_for_v1(p1.deck_id(), p0.deck_id(), 2)?;
        Ok(Self {
            registered_decks: [p0, p1],
            sideboard_policy,
            match_state: BestOfThreeMatchStateV1::new_v1(game_one_chooser)?,
        })
    }

    pub fn registered_deck(&self, player: PlayerId) -> Option<&RegisteredDeckV1> {
        self.registered_decks.get(player.index())
    }

    pub fn sideboard_policy(&self) -> &DeterministicSideboardPolicyV1 {
        &self.sideboard_policy
    }

    pub fn match_state(&self) -> &BestOfThreeMatchStateV1 {
        &self.match_state
    }

    pub fn prepare_game_v1(
        &mut self,
        chooser: PlayerId,
        choice: PlayDrawChoiceV1,
    ) -> Result<PreparedMatchGameV1, Bo3SessionErrorV1> {
        // Advance a clone first. Any match-phase or sideboard failure leaves
        // `self.match_state` byte-for-byte unchanged.
        let mut next_match_state = self.match_state.clone();
        let start = next_match_state.choose_play_draw_v1(chooser, choice)?;

        let (configurations, sideboard_receipts) = if start.game_index == 1 {
            (
                [
                    self.registered_decks[0].registered_configuration().clone(),
                    self.registered_decks[1].registered_configuration().clone(),
                ],
                [None, None],
            )
        } else {
            let (p0_configuration, p0_receipt) = self.sideboard_policy.apply_v1(
                &self.registered_decks[0],
                self.registered_decks[1].deck_id(),
                start.game_index,
            )?;
            let (p1_configuration, p1_receipt) = self.sideboard_policy.apply_v1(
                &self.registered_decks[1],
                self.registered_decks[0].deck_id(),
                start.game_index,
            )?;
            (
                [p0_configuration, p1_configuration],
                [Some(p0_receipt), Some(p1_receipt)],
            )
        };

        self.match_state = next_match_state;
        Ok(PreparedMatchGameV1 {
            start,
            configurations,
            sideboard_receipts,
        })
    }

    pub fn record_game_result_v1(
        &mut self,
        outcome: GameOutcomeV1,
    ) -> Result<MatchTransitionV1, Bo3SessionErrorV1> {
        self.match_state
            .record_game_result_v1(outcome)
            .map_err(Into::into)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bo3SessionErrorV1 {
    Match(MatchStateErrorV1),
    Sideboard(SideboardErrorV1),
    MirrorRegistrationMismatch { deck_id: String },
}

impl From<MatchStateErrorV1> for Bo3SessionErrorV1 {
    fn from(error: MatchStateErrorV1) -> Self {
        Self::Match(error)
    }
}

impl From<SideboardErrorV1> for Bo3SessionErrorV1 {
    fn from(error: SideboardErrorV1) -> Self {
        Self::Sideboard(error)
    }
}

impl fmt::Display for Bo3SessionErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Match(error) => write!(formatter, "BO3 match-state error: {error}"),
            Self::Sideboard(error) => write!(formatter, "BO3 sideboard error: {error}"),
            Self::MirrorRegistrationMismatch { deck_id } => write!(
                formatter,
                "mirror deck id {deck_id:?} has two different registered 75-card configurations"
            ),
        }
    }
}

impl Error for Bo3SessionErrorV1 {}
