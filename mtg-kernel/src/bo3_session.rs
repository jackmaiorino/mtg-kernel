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
use crate::learned_sideboard_v1::{LearnedSideboardInputV1, SideboardActionV1};
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

/// W8a's live per-seat sideboard policy boundary (ratified spec section 6,
/// ruling 9). The evidence type is the whole enforcement mechanism: this
/// trait can never receive a `GameSummaryV1`, the opponent's registration, or
/// any hidden zone contents, only the caller's already-redacted
/// `LearnedSideboardInputV1` (built with `learned_bo3_v1::project_sideboard_input_v1`
/// or an equivalent projector) and this seat's own current 60/15. A policy
/// implementing this trait cannot see anything the deck-model design forbids
/// because there is no field on `LearnedSideboardInputV1` carrying it.
pub trait LiveSideboardSwapPolicyV1 {
    fn select_live_v1(
        &mut self,
        evidence: &LearnedSideboardInputV1,
        current: &DeckConfigurationV1,
    ) -> Result<(DeckConfigurationV1, Vec<SideboardActionV1>), String>;
}

/// One seat's live consultation for a single `prepare_game_with_live_policies_v1`
/// call: the policy plus the exact restricted evidence and current
/// configuration it is entitled to see for this decision. Borrowed, not
/// owned, so the caller keeps its own policy state (e.g. a sampler's decision
/// trace) after the call returns.
pub struct LiveSideboardConsultationV1<'a> {
    pub policy: &'a mut dyn LiveSideboardSwapPolicyV1,
    pub evidence: &'a LearnedSideboardInputV1,
    pub current: &'a DeckConfigurationV1,
}

/// The action trace a live policy submitted for one seat. Deliberately holds
/// only the actions, not sampled probabilities: a sampler's own richer record
/// (e.g. `learned_sideboard_v1::SampledSideboardDecisionV1`) stays with the
/// caller's policy object, which it can still read after the call returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveSideboardSelectionRecordV1 {
    pub actions: Vec<SideboardActionV1>,
}

/// `prepare_game_with_live_policies_v1`'s result: the prepared game, in the
/// exact same `PreparedMatchGameV1` shape every other constructor returns,
/// plus which seats (if any) were live-consulted this call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveGamePreparationV1 {
    pub prepared: PreparedMatchGameV1,
    pub live_selections: [Option<LiveSideboardSelectionRecordV1>; 2],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BestOfThreeDeckMatchV1 {
    registered_decks: [RegisteredDeckV1; 2],
    sideboard_policy: Option<DeterministicSideboardPolicyV1>,
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
            sideboard_policy: Some(sideboard_policy),
            match_state: BestOfThreeMatchStateV1::new_v1(game_one_chooser)?,
        })
    }

    /// Creates a match whose configurations are supplied by live policies.
    /// No static teacher is fabricated or used. Labels are metadata, so two
    /// seats may share a label while registering different actual 75s.
    pub fn new_live_v1(
        registered_decks: [RegisteredDeckV1; 2],
        game_one_chooser: PlayerId,
    ) -> Result<Self, Bo3SessionErrorV1> {
        for deck in &registered_decks {
            deck.validate_executable_v1()?;
        }
        Ok(Self {
            registered_decks,
            sideboard_policy: None,
            match_state: BestOfThreeMatchStateV1::new_v1(game_one_chooser)?,
        })
    }

    pub fn registered_deck(&self, player: PlayerId) -> Option<&RegisteredDeckV1> {
        self.registered_decks.get(player.index())
    }

    pub fn sideboard_policy(&self) -> Option<&DeterministicSideboardPolicyV1> {
        self.sideboard_policy.as_ref()
    }

    pub fn match_state(&self) -> &BestOfThreeMatchStateV1 {
        &self.match_state
    }

    pub fn prepare_game_v1(
        &mut self,
        chooser: PlayerId,
        choice: PlayDrawChoiceV1,
    ) -> Result<PreparedMatchGameV1, Bo3SessionErrorV1> {
        let sideboard_policy = self
            .sideboard_policy
            .as_ref()
            .ok_or(Bo3SessionErrorV1::LiveConfigurationsRequired)?;
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
            let (p0_configuration, p0_receipt) = sideboard_policy.apply_v1(
                &self.registered_decks[0],
                self.registered_decks[1].deck_id(),
                start.game_index,
            )?;
            let (p1_configuration, p1_receipt) = sideboard_policy.apply_v1(
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

    /// Prepares a game using configurations selected by live sideboard policies.
    /// The caller owns policy observations and selection receipts. This method
    /// only admits complete configurations conserving each seat's registered 75.
    /// Both seats are validated before the match phase changes, so one policy's
    /// failure cannot partially advance the match. Game one cannot be sideboarded.
    pub fn prepare_game_with_configurations_v1(
        &mut self,
        chooser: PlayerId,
        choice: PlayDrawChoiceV1,
        configurations: [DeckConfigurationV1; 2],
    ) -> Result<PreparedMatchGameV1, Bo3SessionErrorV1> {
        let mut next_match_state = self.match_state.clone();
        let start = next_match_state.choose_play_draw_v1(chooser, choice)?;
        for (seat, configuration) in configurations.iter().enumerate() {
            let registered = self.registered_decks[seat].registered_configuration();
            if configuration.combined_card_counts_v1() != registered.combined_card_counts_v1() {
                return Err(SideboardErrorV1::RegisteredMultisetChanged.into());
            }
            if start.game_index == 1 && configuration != registered {
                return Err(Bo3SessionErrorV1::GameOneConfigurationChanged {
                    player: PlayerId(seat as u8),
                });
            }
        }
        self.match_state = next_match_state;
        Ok(PreparedMatchGameV1 {
            start,
            configurations,
            sideboard_receipts: [None, None],
        })
    }

    /// W8a's opt-in live-swap match path (ratified spec section 6 and 8,
    /// ruling 9): a `prepare_game_v1` sibling that accepts an optional live
    /// policy per seat. A `None` seat keeps this match's own installed
    /// static/Keep policy (`DeterministicSideboardPolicyV1::apply_v1`,
    /// unchanged); a `Some` seat is consulted through
    /// `LiveSideboardSwapPolicyV1::select_live_v1` instead. Game one is never
    /// sideboarded, live or static, matching every other constructor here.
    ///
    /// When neither seat is live this call is byte-identical to
    /// `prepare_game_v1`: with an installed static policy it delegates to
    /// that exact method (so `PreparedMatchGameV1`, including per-seat
    /// `AppliedSideboardReceiptV1`, matches verbatim); with no installed
    /// static policy (a `new_live_v1` match) it falls through to game one's
    /// registered-configuration branch below, which the byte-identity
    /// regression test in this module's test suite also covers for the
    /// installed-policy case. Whenever at least one seat is live, every
    /// configuration this call resolves (live or static) is validated and
    /// committed through the same `prepare_game_with_configurations_v1` this
    /// struct already exposes, never a separate ad hoc path.
    pub fn prepare_game_with_live_policies_v1(
        &mut self,
        chooser: PlayerId,
        choice: PlayDrawChoiceV1,
        mut live: [Option<LiveSideboardConsultationV1<'_>>; 2],
    ) -> Result<LiveGamePreparationV1, Bo3SessionErrorV1> {
        if live[0].is_none() && live[1].is_none() && self.sideboard_policy.is_some() {
            let prepared = self.prepare_game_v1(chooser, choice)?;
            return Ok(LiveGamePreparationV1 {
                prepared,
                live_selections: [None, None],
            });
        }
        // Probe the upcoming game index on a clone first, exactly like every
        // other method here: any error surfaces before anything is consulted
        // or committed, and `self.match_state` stays untouched until the real
        // (non-probing) advance below.
        let mut probe_state = self.match_state.clone();
        let start = probe_state.choose_play_draw_v1(chooser, choice)?;
        if start.game_index == 1 {
            let registered = [
                self.registered_decks[0].registered_configuration().clone(),
                self.registered_decks[1].registered_configuration().clone(),
            ];
            let prepared = self.prepare_game_with_configurations_v1(chooser, choice, registered)?;
            return Ok(LiveGamePreparationV1 {
                prepared,
                live_selections: [None, None],
            });
        }
        let mut configurations: [Option<DeckConfigurationV1>; 2] = [None, None];
        let mut live_selections: [Option<LiveSideboardSelectionRecordV1>; 2] = [None, None];
        for seat in 0..2 {
            configurations[seat] = Some(match live[seat].take() {
                Some(consultation) => {
                    let (selected, actions) = consultation
                        .policy
                        .select_live_v1(consultation.evidence, consultation.current)
                        .map_err(Bo3SessionErrorV1::LivePolicy)?;
                    live_selections[seat] = Some(LiveSideboardSelectionRecordV1 { actions });
                    selected
                }
                None => {
                    let sideboard_policy = self
                        .sideboard_policy
                        .as_ref()
                        .ok_or(Bo3SessionErrorV1::LiveConfigurationsRequired)?;
                    let opponent_deck_id = self.registered_decks[1 - seat].deck_id().to_owned();
                    let (configuration, _receipt) = sideboard_policy.apply_v1(
                        &self.registered_decks[seat],
                        &opponent_deck_id,
                        start.game_index,
                    )?;
                    configuration
                }
            });
        }
        let [c0, c1] = configurations;
        let prepared = self.prepare_game_with_configurations_v1(
            chooser,
            choice,
            [c0.expect("both seats assigned above"), c1.expect("both seats assigned above")],
        )?;
        Ok(LiveGamePreparationV1 {
            prepared,
            live_selections,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bo3SessionErrorV1 {
    Match(MatchStateErrorV1),
    Sideboard(SideboardErrorV1),
    MirrorRegistrationMismatch { deck_id: String },
    GameOneConfigurationChanged { player: PlayerId },
    LiveConfigurationsRequired,
    /// A `LiveSideboardSwapPolicyV1::select_live_v1` call returned an error.
    LivePolicy(String),
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
            Self::GameOneConfigurationChanged { player } => write!(
                formatter,
                "game one must use player {}'s registered mainboard",
                player.0
            ),
            Self::LiveConfigurationsRequired => formatter.write_str(
                "this match requires live configurations; no static sideboard policy is installed",
            ),
            Self::LivePolicy(message) => write!(formatter, "live sideboard policy error: {message}"),
        }
    }
}

impl Error for Bo3SessionErrorV1 {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sideboard::{CardCountV1, SideboardDefaultPlanV1, SideboardPlanV1};

    fn alpha_deck() -> RegisteredDeckV1 {
        RegisteredDeckV1::new_exact_v1(
            "AlphaDeck",
            [vec![1; 58], vec![2; 2]].concat(),
            vec![3; 15],
        )
        .unwrap()
    }

    fn beta_deck() -> RegisteredDeckV1 {
        RegisteredDeckV1::new_exact_v1("BetaDeck", vec![9; 60], vec![10; 15]).unwrap()
    }

    fn static_policy() -> DeterministicSideboardPolicyV1 {
        let plan = SideboardPlanV1::new_v1(
            "AlphaDeck",
            "BetaDeck",
            2,
            vec![CardCountV1 {
                card_id: 3,
                count: 2,
            }],
            vec![CardCountV1 {
                card_id: 2,
                count: 2,
            }],
        )
        .unwrap();
        DeterministicSideboardPolicyV1::new_v1(
            "w8a-test-policy",
            vec!["AlphaDeck".to_owned(), "BetaDeck".to_owned()],
            SideboardDefaultPlanV1::KeepRegisteredConfiguration,
            vec![plan],
        )
        .unwrap()
    }

    fn new_static_match() -> BestOfThreeDeckMatchV1 {
        BestOfThreeDeckMatchV1::new_v1(alpha_deck(), beta_deck(), static_policy(), PlayerId::P0)
            .unwrap()
    }

    fn sample_evidence() -> LearnedSideboardInputV1 {
        LearnedSideboardInputV1 {
            registered_cards: alpha_deck().registered_configuration().combined_card_counts_v1(),
            own_card_outcomes: Vec::new(),
            opponent_evidence: Vec::new(),
            resource_summaries: Vec::new(),
            next_game_number: 2,
            acting_player_games_won: 0,
            opponent_games_won: 0,
        }
    }

    /// A live policy test double: records every `(evidence, current)` it was
    /// called with and always returns a fixed, caller-supplied selection.
    struct ScriptedLivePolicy {
        calls: Vec<(LearnedSideboardInputV1, DeckConfigurationV1)>,
        selection: Result<(DeckConfigurationV1, Vec<SideboardActionV1>), String>,
    }

    impl ScriptedLivePolicy {
        fn returning(configuration: DeckConfigurationV1, actions: Vec<SideboardActionV1>) -> Self {
            Self {
                calls: Vec::new(),
                selection: Ok((configuration, actions)),
            }
        }
        fn failing(message: &str) -> Self {
            Self {
                calls: Vec::new(),
                selection: Err(message.to_owned()),
            }
        }
    }

    impl LiveSideboardSwapPolicyV1 for ScriptedLivePolicy {
        fn select_live_v1(
            &mut self,
            evidence: &LearnedSideboardInputV1,
            current: &DeckConfigurationV1,
        ) -> Result<(DeckConfigurationV1, Vec<SideboardActionV1>), String> {
            self.calls.push((evidence.clone(), current.clone()));
            self.selection.clone()
        }
    }

    /// Requirement (1): with no live policy supplied at all, every physical
    /// game (one, two, and a decided game three) through the new live-swap
    /// method must byte-match `prepare_game_v1`'s own output, including its
    /// per-seat `AppliedSideboardReceiptV1`, which the general live path
    /// (via `prepare_game_with_configurations_v1`) does not carry.
    #[test]
    fn unchanged_static_path_is_byte_identical_to_prepare_game_v1_across_games() {
        let mut baseline = new_static_match();
        let mut candidate = new_static_match();
        let script = [
            (
                PlayerId::P0,
                PlayDrawChoiceV1::Play,
                GameOutcomeV1::Win { winner: PlayerId::P0 },
            ),
            (
                PlayerId::P1,
                PlayDrawChoiceV1::Draw,
                GameOutcomeV1::Win { winner: PlayerId::P1 },
            ),
            (
                PlayerId::P0,
                PlayDrawChoiceV1::Play,
                GameOutcomeV1::Win { winner: PlayerId::P0 },
            ),
        ];
        for (chooser, choice, outcome) in script {
            let expected = baseline.prepare_game_v1(chooser, choice).unwrap();
            let actual = candidate
                .prepare_game_with_live_policies_v1(chooser, choice, [None, None])
                .unwrap();
            assert_eq!(actual.prepared, expected);
            assert_eq!(actual.live_selections, [None, None]);
            baseline.record_game_result_v1(outcome).unwrap();
            candidate.record_game_result_v1(outcome).unwrap();
        }
        assert_eq!(baseline.match_state().phase(), candidate.match_state().phase());
    }

    /// A `new_live_v1` match (no installed static policy) has no
    /// `prepare_game_v1` to be identical to; game one must still resolve via
    /// the general path's registered-configuration branch, matching what
    /// `prepare_game_with_configurations_v1` alone already produces today.
    #[test]
    fn live_only_match_advances_game_one_without_any_live_policy() {
        let p0 = checked_in_pauper_registered_deck_by_id_v1("Rally").unwrap();
        let p1 = checked_in_pauper_registered_deck_by_id_v1("Burn").unwrap();
        let mut baseline =
            BestOfThreeDeckMatchV1::new_live_v1([p0.clone(), p1.clone()], PlayerId::P0).unwrap();
        let mut candidate = BestOfThreeDeckMatchV1::new_live_v1([p0, p1], PlayerId::P0).unwrap();
        let registered = [
            baseline.registered_deck(PlayerId::P0).unwrap().registered_configuration().clone(),
            baseline.registered_deck(PlayerId::P1).unwrap().registered_configuration().clone(),
        ];
        let expected = baseline
            .prepare_game_with_configurations_v1(PlayerId::P0, PlayDrawChoiceV1::Play, registered)
            .unwrap();
        let actual = candidate
            .prepare_game_with_live_policies_v1(PlayerId::P0, PlayDrawChoiceV1::Play, [None, None])
            .unwrap();
        assert_eq!(actual.prepared, expected);
        assert_eq!(actual.live_selections, [None, None]);
        // Without any live policy or installed static policy, game two would
        // have nothing to resolve a configuration from.
        candidate.record_game_result_v1(GameOutcomeV1::Win { winner: PlayerId::P0 }).unwrap();
        let error = candidate
            .prepare_game_with_live_policies_v1(PlayerId::P1, PlayDrawChoiceV1::Play, [None, None])
            .unwrap_err();
        assert_eq!(error, Bo3SessionErrorV1::LiveConfigurationsRequired);
    }

    /// Requirement (2)/(1): one seat's live policy is consulted and its
    /// selection wins over the static plan; the other seat, left `None`,
    /// still gets the match's own static plan, unchanged, and receipts are
    /// dropped for both seats since this call went through
    /// `prepare_game_with_configurations_v1`.
    #[test]
    fn one_live_seat_overrides_only_its_own_configuration() {
        // Advance both an unmodified baseline and the candidate through game
        // one first: sideboarding, live or static, never happens at game one.
        let mut baseline = new_static_match();
        baseline
            .prepare_game_v1(PlayerId::P0, PlayDrawChoiceV1::Play)
            .unwrap();
        baseline
            .record_game_result_v1(GameOutcomeV1::Win { winner: PlayerId::P0 })
            .unwrap();
        let expected_p1_only = baseline
            .prepare_game_v1(PlayerId::P1, PlayDrawChoiceV1::Play)
            .unwrap();

        let mut candidate = new_static_match();
        candidate
            .prepare_game_with_live_policies_v1(PlayerId::P0, PlayDrawChoiceV1::Play, [None, None])
            .unwrap();
        candidate
            .record_game_result_v1(GameOutcomeV1::Win { winner: PlayerId::P0 })
            .unwrap();

        // A different, still-conserving one-for-one swap than the static
        // plan's own two-for-two swap, so the two are distinguishable.
        let live_configuration = DeckConfigurationV1::new_exact_v1(
            [vec![1; 58], vec![2; 1], vec![3; 1]].concat(),
            [vec![2; 1], vec![3; 14]].concat(),
        )
        .unwrap();
        let evidence = sample_evidence();
        let current = alpha_deck().registered_configuration().clone();
        let mut learner = ScriptedLivePolicy::returning(
            live_configuration.clone(),
            vec![
                SideboardActionV1::MoveOneToSideboard { card_id: 2 },
                SideboardActionV1::MoveOneToMainboard { card_id: 3 },
                SideboardActionV1::Done,
            ],
        );
        let live = [
            Some(LiveSideboardConsultationV1 {
                policy: &mut learner,
                evidence: &evidence,
                current: &current,
            }),
            None,
        ];
        let result = candidate
            .prepare_game_with_live_policies_v1(PlayerId::P1, PlayDrawChoiceV1::Play, live)
            .unwrap();
        assert_eq!(
            result.prepared.configuration(PlayerId::P0).unwrap(),
            &live_configuration
        );
        assert_ne!(
            result.prepared.configuration(PlayerId::P0).unwrap(),
            expected_p1_only.configuration(PlayerId::P0).unwrap()
        );
        assert_eq!(
            result.prepared.configuration(PlayerId::P1).unwrap(),
            expected_p1_only.configuration(PlayerId::P1).unwrap()
        );
        assert_eq!(result.prepared.sideboard_receipt(PlayerId::P0), None);
        assert_eq!(result.prepared.sideboard_receipt(PlayerId::P1), None);
        assert_eq!(learner.calls.len(), 1);
        assert_eq!(learner.calls[0].0, evidence);
        assert_eq!(learner.calls[0].1, current);
        assert_eq!(
            result.live_selections[0].as_ref().unwrap().actions.len(),
            3
        );
        assert!(result.live_selections[1].is_none());
    }

    /// Requirement (2): the evidence a live policy receives is exactly the
    /// value the caller supplied, unchanged, nothing added and nothing from
    /// the opponent's registration or any hidden game state folded in. The
    /// type itself (`LearnedSideboardInputV1`) is the enforcement; this is
    /// the integration check that the pass-through is faithful.
    #[test]
    fn live_policy_receives_exactly_the_supplied_restricted_evidence() {
        let mut candidate = new_static_match();
        candidate
            .prepare_game_with_live_policies_v1(PlayerId::P0, PlayDrawChoiceV1::Play, [None, None])
            .unwrap();
        candidate
            .record_game_result_v1(GameOutcomeV1::Win { winner: PlayerId::P0 })
            .unwrap();
        let evidence = LearnedSideboardInputV1 {
            registered_cards: alpha_deck().registered_configuration().combined_card_counts_v1(),
            own_card_outcomes: Vec::new(),
            opponent_evidence: vec![crate::learned_sideboard_v1::SideboardOpponentEvidenceV1 {
                card_id: Some(42),
                game_index: 1,
                first_seen_turn: 3,
                zone: crate::learned_sideboard_v1::VisibleEvidenceZoneV1::Battlefield,
            }],
            resource_summaries: Vec::new(),
            next_game_number: 2,
            acting_player_games_won: 1,
            opponent_games_won: 0,
        };
        let current = alpha_deck().registered_configuration().clone();
        let mut learner =
            ScriptedLivePolicy::returning(current.clone(), vec![SideboardActionV1::Done]);
        let live = [
            Some(LiveSideboardConsultationV1 {
                policy: &mut learner,
                evidence: &evidence,
                current: &current,
            }),
            None,
        ];
        candidate
            .prepare_game_with_live_policies_v1(PlayerId::P1, PlayDrawChoiceV1::Play, live)
            .unwrap();
        assert_eq!(learner.calls.len(), 1);
        assert_eq!(learner.calls[0].0, evidence);
        assert_eq!(learner.calls[0].1, current);
    }

    /// Requirement (4): starting-player fidelity. `prepare_game_with_live_policies_v1`
    /// must derive `starting_player` from the chooser/choice exactly like
    /// every other constructor: Play keeps the chooser on the play, Draw
    /// hands the play to the opponent.
    #[test]
    fn starting_player_matches_play_draw_choice_across_games() {
        let mut candidate = new_static_match();
        let evidence = sample_evidence();
        // The live seat below is P1 (BetaDeck): its returned configuration
        // must conserve BetaDeck's own registered 75, not AlphaDeck's.
        let current = beta_deck().registered_configuration().clone();

        let g1 = candidate
            .prepare_game_with_live_policies_v1(PlayerId::P0, PlayDrawChoiceV1::Draw, [None, None])
            .unwrap();
        assert_eq!(g1.prepared.start().starting_player, PlayerId::P1);
        candidate
            .record_game_result_v1(GameOutcomeV1::Win { winner: PlayerId::P0 })
            .unwrap();

        let mut learner =
            ScriptedLivePolicy::returning(current.clone(), vec![SideboardActionV1::Done]);
        let live = [
            None,
            Some(LiveSideboardConsultationV1 {
                policy: &mut learner,
                evidence: &evidence,
                current: &current,
            }),
        ];
        let g2 = candidate
            .prepare_game_with_live_policies_v1(PlayerId::P1, PlayDrawChoiceV1::Play, live)
            .unwrap();
        assert_eq!(g2.prepared.start().starting_player, PlayerId::P1);
        assert_eq!(g2.prepared.start().game_index, 2);

        // g1 and g2 above only ever exercise expected starting player P1
        // (g1 through the `prepare_game_v1` delegation, g2 through the
        // probe branch). The probe branch (at least one live seat, so the
        // top-level delegation shortcut is skipped) must equally derive
        // starting player P0: chooser P0 keeps the play, and chooser P1's
        // Draw hands the play to P0. Two fresh matches isolate each chooser
        // without fighting this module's win-count/next-chooser bookkeeping.
        let mut p0_chooses = new_static_match();
        p0_chooses
            .prepare_game_with_live_policies_v1(PlayerId::P0, PlayDrawChoiceV1::Play, [None, None])
            .unwrap();
        p0_chooses.record_game_result_v1(GameOutcomeV1::Draw).unwrap();
        let mut learner_p0_play =
            ScriptedLivePolicy::returning(current.clone(), vec![SideboardActionV1::Done]);
        let live_p0_play = [
            None,
            Some(LiveSideboardConsultationV1 {
                policy: &mut learner_p0_play,
                evidence: &evidence,
                current: &current,
            }),
        ];
        let g3 = p0_chooses
            .prepare_game_with_live_policies_v1(PlayerId::P0, PlayDrawChoiceV1::Play, live_p0_play)
            .unwrap();
        assert_eq!(g3.prepared.start().starting_player, PlayerId::P0);
        assert_eq!(g3.prepared.start().game_index, 2);

        let mut p1_chooses = BestOfThreeDeckMatchV1::new_v1(
            alpha_deck(),
            beta_deck(),
            static_policy(),
            PlayerId::P1,
        )
        .unwrap();
        p1_chooses
            .prepare_game_with_live_policies_v1(PlayerId::P1, PlayDrawChoiceV1::Play, [None, None])
            .unwrap();
        p1_chooses.record_game_result_v1(GameOutcomeV1::Draw).unwrap();
        let mut learner_p1_draw =
            ScriptedLivePolicy::returning(current.clone(), vec![SideboardActionV1::Done]);
        let live_p1_draw = [
            None,
            Some(LiveSideboardConsultationV1 {
                policy: &mut learner_p1_draw,
                evidence: &evidence,
                current: &current,
            }),
        ];
        let g4 = p1_chooses
            .prepare_game_with_live_policies_v1(PlayerId::P1, PlayDrawChoiceV1::Draw, live_p1_draw)
            .unwrap();
        assert_eq!(g4.prepared.start().starting_player, PlayerId::P0);
        assert_eq!(g4.prepared.start().game_index, 2);
    }

    /// Requirement (4): draw handling beyond three games. Every physical
    /// game past game three is still individually sideboarded through the
    /// live path, and the match only completes on two real wins, never on a
    /// game-count ceiling.
    #[test]
    fn live_policy_is_consulted_through_a_draw_extended_match() {
        let mut candidate = new_static_match();
        let current = alpha_deck().registered_configuration().clone();
        let mut consultations = 0usize;
        // game 1: no sideboarding.
        candidate
            .prepare_game_with_live_policies_v1(PlayerId::P0, PlayDrawChoiceV1::Play, [None, None])
            .unwrap();
        candidate.record_game_result_v1(GameOutcomeV1::Draw).unwrap();
        // Games 2..=5 all draw; the match keeps extending and the learner
        // seat is consulted every time.
        for expected_game_index in 2..=5u8 {
            let evidence = sample_evidence();
            let mut learner =
                ScriptedLivePolicy::returning(current.clone(), vec![SideboardActionV1::Done]);
            let live = [
                Some(LiveSideboardConsultationV1 {
                    policy: &mut learner,
                    evidence: &evidence,
                    current: &current,
                }),
                None,
            ];
            // After a draw the original chooser chooses again (bo3_match.rs).
            let prepared = candidate
                .prepare_game_with_live_policies_v1(PlayerId::P0, PlayDrawChoiceV1::Play, live)
                .unwrap();
            assert_eq!(prepared.prepared.start().game_index, expected_game_index);
            assert_eq!(learner.calls.len(), 1);
            consultations += 1;
            candidate.record_game_result_v1(GameOutcomeV1::Draw).unwrap();
        }
        assert_eq!(consultations, 4);
        // Every prior game drew (zero wins for either side), so the match
        // needs two real wins, not one, to complete: one more physical game
        // than three (proving no game-count ceiling), then a second to reach
        // `BEST_OF_THREE_WINS_REQUIRED_V1`.
        // The first win's chooser is still P0 (every prior game drew, so the
        // chooser never changed); after a real win the loser's opponent
        // (P0 again, since P0 wins) chooses next... except the winner's
        // opponent chooses, so after P0 wins game six, P1 chooses game seven.
        for chooser in [PlayerId::P0, PlayerId::P1] {
            let evidence = sample_evidence();
            let mut learner =
                ScriptedLivePolicy::returning(current.clone(), vec![SideboardActionV1::Done]);
            let live = [
                Some(LiveSideboardConsultationV1 {
                    policy: &mut learner,
                    evidence: &evidence,
                    current: &current,
                }),
                None,
            ];
            candidate
                .prepare_game_with_live_policies_v1(chooser, PlayDrawChoiceV1::Play, live)
                .unwrap();
            candidate
                .record_game_result_v1(GameOutcomeV1::Win { winner: PlayerId::P0 })
                .unwrap();
        }
        assert_eq!(
            candidate.match_state().phase(),
            crate::bo3_match::MatchPhaseV1::Complete {
                outcome: crate::bo3_match::MatchOutcomeV1::Winner { winner: PlayerId::P0 }
            }
        );
    }

    /// A live policy's failure must leave the match phase byte-unchanged
    /// (the module's own transactional guarantee), and must surface as
    /// `Bo3SessionErrorV1::LivePolicy`, not a silently swallowed error.
    #[test]
    fn failing_live_policy_leaves_match_state_unchanged() {
        let mut candidate = new_static_match();
        let before = candidate.match_state().clone();
        let evidence = sample_evidence();
        let current = alpha_deck().registered_configuration().clone();
        let mut learner = ScriptedLivePolicy::failing("no legal swap found");
        let live = [
            Some(LiveSideboardConsultationV1 {
                policy: &mut learner,
                evidence: &evidence,
                current: &current,
            }),
            None,
        ];
        candidate
            .prepare_game_with_live_policies_v1(PlayerId::P0, PlayDrawChoiceV1::Play, [None, None])
            .unwrap();
        candidate
            .record_game_result_v1(GameOutcomeV1::Win { winner: PlayerId::P0 })
            .unwrap();
        let before_sideboard = candidate.match_state().clone();
        let error = candidate
            .prepare_game_with_live_policies_v1(PlayerId::P1, PlayDrawChoiceV1::Play, live)
            .unwrap_err();
        assert!(matches!(error, Bo3SessionErrorV1::LivePolicy(message) if message == "no legal swap found"));
        assert_eq!(candidate.match_state(), &before_sideboard);
        assert_ne!(&before, candidate.match_state());
    }

    /// A live policy that returns a configuration failing to conserve the
    /// registered 75 is still rejected, exactly like
    /// `prepare_game_with_configurations_v1` already rejects one directly.
    #[test]
    fn live_policy_cannot_change_the_registered_multiset() {
        let mut candidate = new_static_match();
        candidate
            .prepare_game_with_live_policies_v1(PlayerId::P0, PlayDrawChoiceV1::Play, [None, None])
            .unwrap();
        candidate
            .record_game_result_v1(GameOutcomeV1::Win { winner: PlayerId::P0 })
            .unwrap();
        let evidence = sample_evidence();
        let current = alpha_deck().registered_configuration().clone();
        let invalid_configuration =
            DeckConfigurationV1::new_exact_v1(vec![1; 60], vec![99; 15]).unwrap();
        let mut learner = ScriptedLivePolicy::returning(invalid_configuration, vec![]);
        let live = [
            Some(LiveSideboardConsultationV1 {
                policy: &mut learner,
                evidence: &evidence,
                current: &current,
            }),
            None,
        ];
        let error = candidate
            .prepare_game_with_live_policies_v1(PlayerId::P1, PlayDrawChoiceV1::Play, live)
            .unwrap_err();
        assert_eq!(
            error,
            Bo3SessionErrorV1::Sideboard(SideboardErrorV1::RegisteredMultisetChanged)
        );
    }
}
