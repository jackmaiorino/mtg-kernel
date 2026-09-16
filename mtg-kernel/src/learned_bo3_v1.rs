//! Executable cross-deck BO3 integration for visible-input sideboard policies.
//! The match runner never gives its sideboard callback the opposing registration.
//! Configuration identity is explicit and separate from the fixed BO1 catalog.

use crate::bo3_match::{
    GameOutcomeV1, GameStartV1, MatchOutcomeV1, MatchPhaseV1, PlayDrawChoiceV1,
};
use crate::bo3_session::BestOfThreeDeckMatchV1;
use crate::expanded_deck_training_v1::ExpandedInferenceIdentityV1;
use crate::game_summary_v1::{
    try_run_fast_episode_with_summary_v1, GameSummaryV1, RemovalCounterspellTagsV1,
};
use crate::human_opening_v1::HumanOpeningV1;
use crate::ids::PlayerId;
use crate::learned_sideboard_v1::{
    LearnedSideboardInputV1, SideboardActionV1, SideboardDeliberationStateV1,
    SideboardGameResourceV1, SideboardOpponentEvidenceV1, SideboardOwnCardOutcomeV1,
    VisibleEvidenceZoneV1,
};
use crate::paired_bo1_harness_v1::{
    paired_policy_seeds_v1, PairedBo1PolicyInputV1, PairedBo1PolicyV1, PlayPolicyGenerationV1,
};
use crate::rl_session::{FastActorSessionV1, RlSessionError};
use crate::sideboard::{DeckConfigurationV1, RegisteredDeckV1};
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use crate::state::SplitMix64;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Opening identity is separate from the observation contract. Omitted values
/// preserve historical fixed-opening behavior and serialized result bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Bo3OpeningProtocolV1 {
    #[default]
    LegacyKeepSevenV1,
    /// Both players keep seven; setup draws do not count as first-turn draws.
    KeepSevenV2,
}

impl Bo3OpeningProtocolV1 {
    fn is_legacy(&self) -> bool {
        *self == Self::LegacyKeepSevenV1
    }

    fn validate_observation_mode(self, uses_v3: bool) -> Result<(), String> {
        if self == Self::KeepSevenV2 && !uses_v3 {
            return Err("keep_seven_v2 requires the V3 observation contract".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LearnedBo3RunConfigV1 {
    pub deck_ids: [String; 2],
    pub seed: u64,
    pub game_one_chooser: PlayerId,
    pub max_physical_games: u8,
    pub max_physical_decisions: u64,
    pub max_policy_steps: u64,
    #[serde(default, skip_serializing_if = "Bo3OpeningProtocolV1::is_legacy")]
    pub opening_protocol: Bo3OpeningProtocolV1,
}

#[derive(Clone, Debug, Serialize)]
pub struct LearnedBo3SideboardRecordV1 {
    pub acting_player: PlayerId,
    pub input: LearnedSideboardInputV1,
    pub initial_mainboard: Vec<u16>,
    pub initial_sideboard: Vec<u16>,
    pub selected_actions: Vec<SideboardActionV1>,
    pub selected_mainboard_sha256: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct LearnedBo3GameRecordV1 {
    pub start: GameStartV1,
    pub environment_seed: u64,
    pub mainboard_sha256: [String; 2],
    pub winner: Option<PlayerId>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LearnedBo3ResultV1 {
    pub schema: String,
    pub config: LearnedBo3RunConfigV1,
    /// Present only for explicit-registration runs. These are the original
    /// registered zones, distinct from each physical game's selected 60.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explicit_registrations: Option<[LearnedBo3RegistrationRecordV1; 2]>,
    pub play_weights_sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub play_observation_contract: Option<String>,
    pub sideboard_policy_identity: String,
    pub outcome: MatchOutcomeV1,
    pub games: Vec<LearnedBo3GameRecordV1>,
    pub sideboard_decisions: Vec<LearnedBo3SideboardRecordV1>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LearnedBo3RegistrationRecordV1 {
    pub label: String,
    pub mainboard: Vec<u16>,
    pub sideboard: Vec<u16>,
    pub registered_75_sha256: String,
    pub mainboard_sha256: String,
    pub sideboard_sha256: String,
}

/// Population results deliberately have no single-player weight identity.
/// Every array is ordered by physical seat, including actual model receipts.
#[derive(Clone, Debug, Serialize)]
pub struct LearnedPopulationBo3ResultV1 {
    pub schema: String,
    pub config: LearnedBo3RunConfigV1,
    pub explicit_registrations: [LearnedBo3RegistrationRecordV1; 2],
    pub play_models: [ExpandedInferenceIdentityV1; 2],
    pub sideboard_policy_identities: [String; 2],
    pub outcome: MatchOutcomeV1,
    pub games: Vec<LearnedBo3GameRecordV1>,
    pub sideboard_decisions: Vec<LearnedBo3SideboardRecordV1>,
}

/// Each policy sees only the input belonging to its assigned physical seat.
/// Both models receive the declared seed pair at a game boundary. A policy's
/// other seat stream is never advanced by the router.
pub struct SeatRoutedBo3PlayPolicyV1<'a> {
    policies: [&'a mut dyn PairedBo1PolicyV1; 2],
    uses_v3: bool,
}

impl<'a> SeatRoutedBo3PlayPolicyV1<'a> {
    pub fn new_v1(policies: [&'a mut dyn PairedBo1PolicyV1; 2]) -> Result<Self, String> {
        // Strict whole-generation equality, not just the wide-vs-narrow
        // boolean: two different wide generations (V3 and V4) both report
        // `true` for `uses_observation_successor_v3`, so comparing only that
        // boolean would silently accept a mixed V3/V4 seat pairing.
        if policies[0].feature_generation_v1() != policies[1].feature_generation_v1() {
            return Err("per-seat BO3 policies require the same feature generation".into());
        }
        let uses_v3 = policies[0].uses_observation_successor_v3();
        Ok(Self { policies, uses_v3 })
    }
}

impl PairedBo1PolicyV1 for SeatRoutedBo3PlayPolicyV1<'_> {
    fn uses_observation_successor_v3(&self) -> bool {
        self.uses_v3
    }

    /// `new_v1` already required both seats to report the same generation,
    /// so either seat's own value is authoritative here.
    fn feature_generation_v1(&self) -> crate::paired_bo1_harness_v1::PlayPolicyGenerationV1 {
        self.policies[0].feature_generation_v1()
    }

    fn reset_for_game_v1(&mut self, seeds: [u64; 2]) -> Result<(), RlSessionError> {
        self.policies[0].reset_for_game_v1(seeds)?;
        self.policies[1].reset_for_game_v1(seeds)
    }

    fn select_action_v1(
        &mut self,
        input: PairedBo1PolicyInputV1<'_>,
    ) -> Result<u32, RlSessionError> {
        let seat = match input.decision().acting_player {
            crate::rl::PlayerSeatV1::P0 => 0,
            crate::rl::PlayerSeatV1::P1 => 1,
        };
        self.policies[seat].select_action_v1(input)
    }
}

#[derive(Clone, Copy)]
enum Bo3ModelProvenanceV1<'a> {
    Shared(&'a str),
    PerSeat([&'a str; 2]),
}

impl<'a> Bo3ModelProvenanceV1<'a> {
    fn weights_for_seat(self, seat: usize) -> &'a str {
        match self {
            Self::Shared(value) => value,
            Self::PerSeat(values) => values[seat],
        }
    }
}

struct Bo3ExecutionV1 {
    config: LearnedBo3RunConfigV1,
    outcome: MatchOutcomeV1,
    games: Vec<LearnedBo3GameRecordV1>,
    sideboard_decisions: Vec<LearnedBo3SideboardRecordV1>,
}

impl Bo3ExecutionV1 {
    fn into_legacy(
        self,
        registrations: Option<[LearnedBo3RegistrationRecordV1; 2]>,
        weights: &str,
        sideboard_identity: &str,
        uses_v3: bool,
    ) -> LearnedBo3ResultV1 {
        LearnedBo3ResultV1 {
            schema: "kernel_learned_bo3/v1".into(),
            config: self.config,
            explicit_registrations: registrations,
            play_weights_sha256: weights.to_owned(),
            play_observation_contract: uses_v3
                .then(|| "rich-v6-flat-v3-explicit-frozen-feature-transfer".into()),
            sideboard_policy_identity: sideboard_identity.to_owned(),
            outcome: self.outcome,
            games: self.games,
            sideboard_decisions: self.sideboard_decisions,
        }
    }
}

impl LearnedBo3RegistrationRecordV1 {
    pub fn from_registered_v1(registered: &RegisteredDeckV1) -> Self {
        let configuration = registered.registered_configuration();
        Self {
            label: registered.deck_id().to_owned(),
            mainboard: configuration.mainboard().to_vec(),
            sideboard: configuration.sideboard().to_vec(),
            registered_75_sha256: hex_v1(&registered.registered_75_sha256_v1()),
            mainboard_sha256: hex_v1(&configuration.mainboard_sha256_v1()),
            sideboard_sha256: hex_v1(&configuration.sideboard_sha256_v1()),
        }
    }
}

/// Project each completed game for the acting player. A raw GameSummaryV1
/// contains both seats' private outcomes and must not be model input directly.
pub fn project_sideboard_input_v1(
    registered: &DeckConfigurationV1,
    actor: PlayerId,
    summaries: &[GameSummaryV1],
    next_game_number: u8,
    wins: [u8; 2],
) -> Result<LearnedSideboardInputV1, String> {
    if actor.0 > 1 || next_game_number < 2 || summaries.len() != usize::from(next_game_number - 1) {
        return Err("invalid visible sideboard history boundary".to_owned());
    }
    let seat = actor.index();
    let mut own_card_outcomes = Vec::new();
    let mut opponent_evidence = Vec::new();
    let mut resource_summaries = Vec::new();
    for (index, summary) in summaries.iter().enumerate() {
        let game_index = u8::try_from(index + 1).map_err(|_| "too many completed games")?;
        for (&card_id, own) in &summary.own_card_outcomes[seat] {
            // Generated tokens are not registered cards and are not policy moves.
            if !registered.mainboard().contains(&card_id)
                && !registered.sideboard().contains(&card_id)
            {
                continue;
            }
            own_card_outcomes.push(SideboardOwnCardOutcomeV1 {
                card_id,
                game_index,
                times_drawn: own.times_drawn,
                cast: own.cast,
                stuck_in_hand: own.stuck_in_hand,
                died_without_dealing_damage: own.died_without_dealing_damage,
                removal_no_target: own.removal_no_target,
                counterspell_held: own.counterspell_held,
            });
        }
        // Presence per public card identity, not object counts that could link
        // indistinguishable copies through hidden hand or library transitions.
        let mut seen = BTreeMap::<u16, u32>::new();
        for evidence in &summary.opponent_evidence[seat] {
            seen.entry(evidence.card_id)
                .and_modify(|turn| *turn = (*turn).min(evidence.first_seen_turn))
                .or_insert(evidence.first_seen_turn);
        }
        for (card_id, first_seen_turn) in seen {
            opponent_evidence.push(SideboardOpponentEvidenceV1 {
                card_id: Some(card_id),
                game_index,
                first_seen_turn,
                // The fold knows current hidden object locations. Intentionally
                // omit that endpoint location even for an earlier public card.
                zone: VisibleEvidenceZoneV1::OtherVisible,
            });
        }
        let resources = &summary.resource_curve;
        let hand = &resources.hand_size_by_turn[seat];
        resource_summaries.push(SideboardGameResourceV1 {
            game_index,
            own_lands_mean: None, // Existing field totals both seats.
            own_hand_mean: (!hand.is_empty())
                .then(|| hand.iter().map(|v| *v as f32).sum::<f32>() / hand.len() as f32),
            // Existing life series is sampled at turn starts, not terminal life.
            own_life_final: None,
            opponent_life_final: None,
            own_damage_dealt: Some(resources.damage_dealt_total[seat]),
            own_damage_taken: Some(resources.damage_taken_total[seat]),
        });
    }
    Ok(LearnedSideboardInputV1 {
        registered_cards: registered.combined_card_counts_v1(),
        own_card_outcomes,
        opponent_evidence,
        resource_summaries,
        next_game_number,
        acting_player_games_won: wins[seat],
        opponent_games_won: wins[1 - seat],
    })
}

pub type SideboardSelectionV1 = (DeckConfigurationV1, Vec<SideboardActionV1>);

pub trait VisibleSideboardPolicyV1 {
    fn select_v1(
        &mut self,
        input: &LearnedSideboardInputV1,
        current: &DeckConfigurationV1,
    ) -> Result<SideboardSelectionV1, String>;
}

impl<F> VisibleSideboardPolicyV1 for F
where
    F: FnMut(
        &LearnedSideboardInputV1,
        &DeckConfigurationV1,
    ) -> Result<SideboardSelectionV1, String>,
{
    fn select_v1(
        &mut self,
        input: &LearnedSideboardInputV1,
        current: &DeckConfigurationV1,
    ) -> Result<SideboardSelectionV1, String> {
        self(input, current)
    }
}

pub fn run_learned_bo3_v1(
    config: LearnedBo3RunConfigV1,
    play_weights_sha256: &str,
    sideboard_policy_identity: &str,
    tags: &RemovalCounterspellTagsV1,
    play_policy: &mut dyn PairedBo1PolicyV1,
    sideboard_policies: [&mut dyn VisibleSideboardPolicyV1; 2],
) -> Result<LearnedBo3ResultV1, String> {
    validate_run_limits_v1(&config)?;
    let match_session = BestOfThreeDeckMatchV1::checked_in_pauper_v1(
        &config.deck_ids[0],
        &config.deck_ids[1],
        config.game_one_chooser,
    )
    .map_err(|error| error.to_string())?;
    let uses_v3 = play_policy.uses_observation_successor_v3();
    Ok(run_learned_bo3_session_v1(
        config,
        match_session,
        Bo3ModelProvenanceV1::Shared(play_weights_sha256),
        tags,
        play_policy,
        sideboard_policies,
    )?
    .into_legacy(
        None,
        play_weights_sha256,
        sideboard_policy_identity,
        uses_v3,
    ))
}

/// Executes the same BO3 loop with the supplied supported registered 75s.
/// Labels must match the configuration metadata, but are never used to look
/// up or substitute a catalog registration. Callback visibility is unchanged.
pub fn run_learned_bo3_with_registrations_v1(
    config: LearnedBo3RunConfigV1,
    registered_decks: [RegisteredDeckV1; 2],
    play_weights_sha256: &str,
    sideboard_policy_identity: &str,
    tags: &RemovalCounterspellTagsV1,
    play_policy: &mut dyn PairedBo1PolicyV1,
    sideboard_policies: [&mut dyn VisibleSideboardPolicyV1; 2],
) -> Result<LearnedBo3ResultV1, String> {
    validate_run_limits_v1(&config)?;
    for (seat, registered) in registered_decks.iter().enumerate() {
        if registered.deck_id() != config.deck_ids[seat] {
            return Err(format!(
                "player {seat} registration label does not match configured deck metadata"
            ));
        }
    }
    let match_session =
        BestOfThreeDeckMatchV1::new_live_v1(registered_decks, config.game_one_chooser)
            .map_err(|error| error.to_string())?;
    let receipts = [PlayerId::P0, PlayerId::P1].map(|seat| {
        LearnedBo3RegistrationRecordV1::from_registered_v1(
            match_session.registered_deck(seat).unwrap(),
        )
    });
    let uses_v3 = play_policy.uses_observation_successor_v3();
    Ok(run_learned_bo3_session_v1(
        config,
        match_session,
        Bo3ModelProvenanceV1::Shared(play_weights_sha256),
        tags,
        play_policy,
        sideboard_policies,
    )?
    .into_legacy(
        Some(receipts),
        play_weights_sha256,
        sideboard_policy_identity,
        uses_v3,
    ))
}

/// Evaluate separately loaded current/historical players without substituting
/// either seat's model or registration. The strict CLI loader supplies the
/// checkpoint receipts; this boundary also checks them against installed model
/// parameters, embeddings and import ancestry before any gameplay.
pub fn run_population_bo3_v1(
    config: LearnedBo3RunConfigV1,
    registered_decks: [RegisteredDeckV1; 2],
    play_models: [ExpandedInferenceIdentityV1; 2],
    sideboard_policy_identities: [String; 2],
    tags: &RemovalCounterspellTagsV1,
    play_policies: &mut [FrozenPlayPolicyV1; 2],
    sideboard_policies: [&mut dyn VisibleSideboardPolicyV1; 2],
) -> Result<LearnedPopulationBo3ResultV1, String> {
    validate_run_limits_v1(&config)?;
    for seat in 0..2 {
        if !play_policies[seat].uses_observation_successor_v3()
            || play_models[seat].feature_schema_version
                != crate::native_flat_tensorizer_v3::FEATURE_SCHEMA_VERSION_V3
            || play_models[seat].feature_registry_version
                != crate::native_flat_tensorizer_v3::FEATURE_REGISTRY_VERSION_V3
            || play_models[seat].features_source_sha256
                != crate::native_flat_tensorizer_v3::FEATURES_SOURCE_SHA256_V3
            || play_models[seat].feature_descriptor_sha256
                != crate::native_flat_tensorizer_v3::FEATURE_DESCRIPTOR_SHA256_V3
        {
            return Err(format!(
                "seat {seat} population model requires the current V3 feature contract"
            ));
        }
        if registered_decks[seat].deck_id() != config.deck_ids[seat] {
            return Err(format!(
                "seat {seat} registration label differs from match metadata"
            ));
        }
        if play_models[seat].model != play_policies[seat].actual_model_identity_v1()
            || &play_models[seat].source_import != play_policies[seat].identity_v1()
        {
            return Err(format!(
                "seat {seat} population receipt differs from installed model"
            ));
        }
    }
    let registrations = registered_decks
        .each_ref()
        .map(LearnedBo3RegistrationRecordV1::from_registered_v1);
    let session = BestOfThreeDeckMatchV1::new_live_v1(registered_decks, config.game_one_chooser)
        .map_err(|error| error.to_string())?;
    let [p0, p1] = play_policies;
    let mut router = SeatRoutedBo3PlayPolicyV1::new_v1([p0, p1])?;
    let played = run_learned_bo3_session_v1(
        config,
        session,
        Bo3ModelProvenanceV1::PerSeat(
            play_models
                .each_ref()
                .map(|r| r.model.weights_sha256.as_str()),
        ),
        tags,
        &mut router,
        sideboard_policies,
    )?;
    Ok(LearnedPopulationBo3ResultV1 {
        schema: "kernel_population_bo3/v1".into(),
        config: played.config,
        explicit_registrations: registrations,
        play_models,
        sideboard_policy_identities,
        outcome: played.outcome,
        games: played.games,
        sideboard_decisions: played.sideboard_decisions,
    })
}

fn validate_run_limits_v1(config: &LearnedBo3RunConfigV1) -> Result<(), String> {
    if config.max_physical_games < 3
        || config.max_physical_decisions == 0
        || config.max_policy_steps == 0
    {
        return Err(
            "positive episode limits and at least three physical games are required".to_owned(),
        );
    }
    Ok(())
}

fn create_bo3_episode_v1(
    config: &LearnedBo3RunConfigV1,
    uses_v3: bool,
    game_index: u8,
    environment_seed: u64,
    mainboards: [Vec<u16>; 2],
    starting_player: PlayerId,
) -> Result<FastActorSessionV1, String> {
    config.opening_protocol.validate_observation_mode(uses_v3)?;
    if config.opening_protocol == Bo3OpeningProtocolV1::KeepSevenV2 {
        // Reuse the human Keep7 transition, including setup-event cleanup.
        // P0 only identifies the seat whose explicit keep is submitted here;
        // the other seat already keeps seven and neither player mulligans.
        let mut opening = HumanOpeningV1::new(
            u64::from(game_index),
            environment_seed,
            config.max_physical_decisions,
            config.max_policy_steps,
            config.deck_ids.clone(),
            mainboards,
            starting_player,
            PlayerId::P0,
        )?;
        opening.keep()?;
        return opening.into_session();
    }
    let constructor = if uses_v3 {
        FastActorSessionV1::reset_with_explicit_decks_and_limits_flat_action_v3_environment_v2_with_starting_player_v1
    } else {
        FastActorSessionV1::reset_with_explicit_decks_and_limits_flat_action_v2_environment_v2_with_starting_player_v1
    };
    constructor(
        u64::from(game_index),
        environment_seed,
        config.max_physical_decisions,
        config.max_policy_steps,
        config.deck_ids.clone(),
        mainboards,
        starting_player,
    )
    .map_err(|error| error.to_string())
}

#[allow(clippy::too_many_arguments)]
fn run_learned_bo3_session_v1(
    config: LearnedBo3RunConfigV1,
    mut match_session: BestOfThreeDeckMatchV1,
    model_provenance: Bo3ModelProvenanceV1<'_>,
    tags: &RemovalCounterspellTagsV1,
    play_policy: &mut dyn PairedBo1PolicyV1,
    sideboard_policies: [&mut dyn VisibleSideboardPolicyV1; 2],
) -> Result<Bo3ExecutionV1, String> {
    let uses_v3 = play_policy.uses_observation_successor_v3();
    config.opening_protocol.validate_observation_mode(uses_v3)?;
    let registered = [PlayerId::P0, PlayerId::P1].map(|seat| {
        match_session
            .registered_deck(seat)
            .unwrap()
            .registered_configuration()
            .clone()
    });
    let mut current = registered.clone();
    let mut summaries: [Vec<GameSummaryV1>; 2] = [Vec::new(), Vec::new()];
    let mut games = Vec::new();
    let mut sideboard_decisions = Vec::new();
    let mut seed_stream = SplitMix64::seed(config.seed);
    loop {
        let (game_index, chooser) = match match_session.match_state().phase() {
            MatchPhaseV1::Complete { outcome } => {
                return Ok(Bo3ExecutionV1 {
                    config,
                    outcome,
                    games,
                    sideboard_decisions,
                })
            }
            MatchPhaseV1::AwaitingPlayDrawChoice {
                game_index,
                chooser,
            } => (game_index, chooser),
            _ => return Err("match unexpectedly awaits an unrecorded game".to_owned()),
        };
        if game_index > config.max_physical_games {
            return Err(
                "match incomplete at physical-game limit; no match reward emitted".to_owned(),
            );
        }
        if game_index >= 2 {
            let wins = [
                match_session.match_state().wins(PlayerId::P0).unwrap(),
                match_session.match_state().wins(PlayerId::P1).unwrap(),
            ];
            // Both callbacks see only the completed prefix, before either
            // opponent's next configuration is revealed or played.
            for seat in [PlayerId::P0, PlayerId::P1] {
                let input = project_sideboard_input_v1(
                    &registered[seat.index()],
                    seat,
                    &summaries[seat.index()],
                    game_index,
                    wins,
                )?;
                let (selected, actions) =
                    sideboard_policies[seat.index()].select_v1(&input, &current[seat.index()])?;
                let mut replay = SideboardDeliberationStateV1::new_v1(&current[seat.index()]);
                for action in &actions {
                    replay
                        .apply_v1(*action)
                        .map_err(|error| error.to_string())?;
                }
                if actions.last() != Some(&SideboardActionV1::Done)
                    || replay
                        .configuration_v1()
                        .map_err(|error| error.to_string())?
                        != selected
                {
                    return Err(
                        "sideboard action trace does not submit the selected configuration"
                            .to_owned(),
                    );
                }
                sideboard_decisions.push(LearnedBo3SideboardRecordV1 {
                    acting_player: seat,
                    input,
                    initial_mainboard: current[seat.index()].mainboard().to_vec(),
                    initial_sideboard: current[seat.index()].sideboard().to_vec(),
                    selected_actions: actions,
                    selected_mainboard_sha256: hex_v1(&selected.mainboard_sha256_v1()),
                });
                current[seat.index()] = selected;
            }
        }
        let prepared = match_session
            .prepare_game_with_configurations_v1(chooser, PlayDrawChoiceV1::Play, current.clone())
            .map_err(|error| error.to_string())?;
        let environment_seed = seed_stream.next_u64();
        play_policy
            .reset_for_game_v1(paired_policy_seeds_v1(environment_seed))
            .map_err(|error| error.to_string())?;
        let mut episode = create_bo3_episode_v1(
            &config,
            uses_v3,
            game_index,
            environment_seed,
            current.each_ref().map(|c| c.mainboard().to_vec()),
            prepared.start().starting_player,
        )?;
        let summary = try_run_fast_episode_with_summary_v1(
            &mut episode,
            model_provenance.weights_for_seat(0),
            tags,
            play_policy,
        )
        .map_err(|error| {
            format!(
                "{} versus {}, match seed {}, game {}, environment seed {}: {}",
                config.deck_ids[0],
                config.deck_ids[1],
                config.seed,
                game_index,
                environment_seed,
                error
            )
        })?;
        games.push(LearnedBo3GameRecordV1 {
            start: prepared.start(),
            environment_seed,
            mainboard_sha256: current.each_ref().map(|c| hex_v1(&c.mainboard_sha256_v1())),
            winner: summary.winner,
        });
        let outcome = summary
            .winner
            .map_or(GameOutcomeV1::Draw, |winner| GameOutcomeV1::Win { winner });
        // Summary provenance belongs to the perspective receiving its own
        // history. Never label a two-player population with a composite hash
        // in a single-model weight field. Raw summaries remain runner-private.
        let mut second_summary = summary.clone();
        second_summary.checkpoint_weights_hash = model_provenance.weights_for_seat(1).to_owned();
        summaries[0].push(summary);
        summaries[1].push(second_summary);
        match_session
            .record_game_result_v1(outcome)
            .map_err(|error| error.to_string())?;
    }
}

fn hex_v1(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_summary_v1::{OpponentEvidenceRowV1, OwnCardOutcomeV1, ResourceCurveV1};
    use crate::state::Zone;

    struct RoutingSpy {
        uses_v3: bool,
        resets: Vec<[u64; 2]>,
        actors: Vec<crate::rl::PlayerSeatV1>,
        rng: [SplitMix64; 2],
    }

    impl RoutingSpy {
        fn new(uses_v3: bool) -> Self {
            Self {
                uses_v3,
                resets: vec![],
                actors: vec![],
                rng: [SplitMix64::seed(0), SplitMix64::seed(0)],
            }
        }
    }

    impl PairedBo1PolicyV1 for RoutingSpy {
        fn uses_observation_successor_v3(&self) -> bool {
            self.uses_v3
        }
        fn reset_for_game_v1(&mut self, seeds: [u64; 2]) -> Result<(), RlSessionError> {
            self.resets.push(seeds);
            self.rng = seeds.map(SplitMix64::seed);
            Ok(())
        }
        fn select_action_v1(
            &mut self,
            input: PairedBo1PolicyInputV1<'_>,
        ) -> Result<u32, RlSessionError> {
            let decision = input.decision();
            self.actors.push(decision.acting_player);
            let seat = match decision.acting_player {
                crate::rl::PlayerSeatV1::P0 => 0,
                crate::rl::PlayerSeatV1::P1 => 1,
            };
            Ok((self.rng[seat].next_u64() as u32) % decision.legal_action_count)
        }
    }

    #[test]
    fn population_router_delivers_only_acting_seat_and_preserves_seed_streams() {
        use crate::rl::PlayerSeatV1::{P0, P1};
        let session = FastActorSessionV1::reset(7, 92, 1);
        let crate::rl_session::FastActorResponseV1::Decision(base) = session.current_response()
        else {
            panic!("initial actor decision required")
        };
        let mut policies = [RoutingSpy::new(true), RoutingSpy::new(true)];
        let seeds = paired_policy_seeds_v1(4242);
        let mut expected_rng = seeds.map(SplitMix64::seed);
        let actors = [P1, P0, P1, P0, P1];
        let expected: Vec<_> = actors
            .iter()
            .map(|actor| {
                let seat = if *actor == P0 { 0 } else { 1 };
                (expected_rng[seat].next_u64() as u32) % 32
            })
            .collect();
        {
            let [p0, p1] = &mut policies;
            let mut router = SeatRoutedBo3PlayPolicyV1::new_v1([p0, p1]).unwrap();
            for _ in 0..2 {
                router.reset_for_game_v1(seeds).unwrap();
                let actual: Vec<_> = actors
                    .iter()
                    .map(|actor| {
                        // This routing spy consumes only decision metadata. The
                        // real adapter remains the sole scorer-input producer.
                        let decision = crate::rl_session::FastActorDecisionV1 {
                            acting_player: *actor,
                            legal_action_count: 32,
                            ..base
                        };
                        router
                            .select_action_v1(PairedBo1PolicyInputV1::new(&session, decision))
                            .unwrap()
                    })
                    .collect();
                assert_eq!(actual, expected);
            }
        }
        assert_eq!(policies[0].actors, vec![P0; 4]);
        assert_eq!(policies[1].actors, vec![P1; 6]);
        assert_eq!(policies[0].resets, vec![seeds; 2]);
        assert_eq!(policies[1].resets, vec![seeds; 2]);
    }

    #[test]
    fn population_router_rejects_mixed_observation_contracts_before_reset() {
        let mut p0 = RoutingSpy::new(false);
        let mut p1 = RoutingSpy::new(true);
        assert!(SeatRoutedBo3PlayPolicyV1::new_v1([&mut p0, &mut p1]).is_err());
        assert!(p0.resets.is_empty());
        assert!(p1.resets.is_empty());
    }

    /// A minimal stand-in that reports only a generation, for gates that
    /// must compare `feature_generation_v1()` and never reach
    /// `reset_for_game_v1`/`select_action_v1` first.
    struct GenerationOnlyStub {
        generation: PlayPolicyGenerationV1,
    }
    impl PairedBo1PolicyV1 for GenerationOnlyStub {
        fn uses_observation_successor_v3(&self) -> bool {
            self.generation != PlayPolicyGenerationV1::V2
        }
        fn feature_generation_v1(&self) -> PlayPolicyGenerationV1 {
            self.generation
        }
        fn reset_for_game_v1(&mut self, _seeds: [u64; 2]) -> Result<(), RlSessionError> {
            unreachable!("generation gate must reject before any reset")
        }
        fn select_action_v1(
            &mut self,
            _input: PairedBo1PolicyInputV1<'_>,
        ) -> Result<u32, RlSessionError> {
            unreachable!("generation gate must reject before any selection")
        }
    }

    #[test]
    fn population_router_rejects_mixed_v3_v4_generation_even_when_the_wide_boolean_matches() {
        // Both V3 and V4 report `true` for the legacy wide-vs-narrow
        // boolean (`uses_observation_successor_v3`), so a pairing check
        // that only compares that boolean would silently accept a mixed
        // V3/V4 seat pairing. `new_v1` must reject this via
        // `feature_generation_v1` instead.
        let mut v3 = GenerationOnlyStub {
            generation: PlayPolicyGenerationV1::V3,
        };
        let mut v4 = GenerationOnlyStub {
            generation: PlayPolicyGenerationV1::V4,
        };
        assert!(v3.uses_observation_successor_v3());
        assert!(v4.uses_observation_successor_v3());
        let policies: [&mut dyn PairedBo1PolicyV1; 2] = [&mut v3, &mut v4];
        assert!(SeatRoutedBo3PlayPolicyV1::new_v1(policies).is_err());

        let mut v3_a = GenerationOnlyStub {
            generation: PlayPolicyGenerationV1::V3,
        };
        let mut v3_b = GenerationOnlyStub {
            generation: PlayPolicyGenerationV1::V3,
        };
        let policies: [&mut dyn PairedBo1PolicyV1; 2] = [&mut v3_a, &mut v3_b];
        assert!(SeatRoutedBo3PlayPolicyV1::new_v1(policies).is_ok());

        let mut v4_a = GenerationOnlyStub {
            generation: PlayPolicyGenerationV1::V4,
        };
        let mut v4_b = GenerationOnlyStub {
            generation: PlayPolicyGenerationV1::V4,
        };
        let policies: [&mut dyn PairedBo1PolicyV1; 2] = [&mut v4_a, &mut v4_b];
        assert!(SeatRoutedBo3PlayPolicyV1::new_v1(policies).is_ok());
    }

    #[test]
    fn legacy_result_serialization_retains_exact_single_model_fields() {
        let output = Bo3ExecutionV1 {
            config: LearnedBo3RunConfigV1 {
                deck_ids: ["Rally".into(), "Burn".into()],
                seed: 17,
                game_one_chooser: PlayerId::P0,
                max_physical_games: 3,
                max_physical_decisions: 4000,
                max_policy_steps: 40000,
                opening_protocol: Bo3OpeningProtocolV1::LegacyKeepSevenV1,
            },
            outcome: MatchOutcomeV1::Winner {
                winner: PlayerId::P0,
            },
            games: vec![],
            sideboard_decisions: vec![],
        }
        .into_legacy(None, "weights", "sideboard", false);
        let expected = serde_json::json!({
            "schema":"kernel_learned_bo3/v1", "config":output.config,
            "play_weights_sha256":"weights", "sideboard_policy_identity":"sideboard",
            "outcome":output.outcome, "games":[], "sideboard_decisions":[]
        });
        assert_eq!(serde_json::to_value(&output).unwrap(), expected);
        assert_eq!(serde_json::to_vec(&output).unwrap(),
            br#"{"schema":"kernel_learned_bo3/v1","config":{"deck_ids":["Rally","Burn"],"seed":17,"game_one_chooser":0,"max_physical_games":3,"max_physical_decisions":4000,"max_policy_steps":40000},"play_weights_sha256":"weights","sideboard_policy_identity":"sideboard","outcome":{"winner":{"winner":0}},"games":[],"sideboard_decisions":[]}"#);
    }

    fn opening_test_config() -> LearnedBo3RunConfigV1 {
        serde_json::from_str(
            r#"{"deck_ids":["Mountain","Island"],"seed":99,"game_one_chooser":0,"max_physical_games":3,"max_physical_decisions":256,"max_policy_steps":8192}"#,
        )
        .unwrap()
    }

    #[test]
    fn opening_protocol_defaults_preserve_legacy_and_reject_unknown_variants() {
        let mut config = opening_test_config();
        assert_eq!(
            config.opening_protocol,
            Bo3OpeningProtocolV1::LegacyKeepSevenV1
        );
        assert!(serde_json::to_value(&config)
            .unwrap()
            .get("opening_protocol")
            .is_none());
        config.opening_protocol = Bo3OpeningProtocolV1::KeepSevenV2;
        let mut encoded = serde_json::to_value(&config).unwrap();
        assert_eq!(encoded["opening_protocol"], "keep_seven_v2");
        let decoded: LearnedBo3RunConfigV1 = serde_json::from_value(encoded.clone()).unwrap();
        assert_eq!(decoded.opening_protocol, Bo3OpeningProtocolV1::KeepSevenV2);
        encoded["opening_protocol"] = "future_unknown".into();
        assert!(serde_json::from_value::<LearnedBo3RunConfigV1>(encoded).is_err());
    }

    #[test]
    fn corrected_keep_seven_matches_human_opening_and_retains_natural_first_draw() {
        use crate::card_def::card_id_by_name;
        use crate::rl::ActionSemanticV1;
        use crate::rl_session::FastActorResponseV1;
        use crate::state::Step;

        let mut config = opening_test_config();
        config.opening_protocol = Bo3OpeningProtocolV1::KeepSevenV2;
        let mainboards = [
            vec![card_id_by_name("Mountain").unwrap(); 60],
            vec![card_id_by_name("Island").unwrap(); 60],
        ];
        for starting in [PlayerId::P0, PlayerId::P1] {
            // Physical game 2 checks that the setting is independent of match
            // position and respects the prepared game's starting player.
            let mut session =
                create_bo3_episode_v1(&config, true, 2, 99, mainboards.clone(), starting).unwrap();
            let mut opening = HumanOpeningV1::new(
                2,
                99,
                256,
                8192,
                config.deck_ids.clone(),
                mainboards.clone(),
                starting,
                PlayerId::P1,
            )
            .unwrap();
            opening.keep().unwrap();
            let human = opening.into_session().unwrap();
            assert_eq!(
                serde_json::to_vec(session.game_state()).unwrap(),
                serde_json::to_vec(human.game_state()).unwrap()
            );
            assert_eq!(session.current_response(), human.current_response());
            assert_eq!(
                session.diagnostic_current_action_semantics(),
                human.diagnostic_current_action_semantics()
            );
            assert_eq!(session.game_state().starting_player, starting);
            assert!(session.game_state().engine.event_log.is_empty());
            for target in [starting, starting.opponent()] {
                let mut reached = false;
                for _ in 0..128 {
                    if session.game_state().active_player == target
                        && session.game_state().step == Step::Main1
                    {
                        reached = true;
                        break;
                    }
                    let FastActorResponseV1::Decision(decision) = session.current_response() else {
                        panic!("unexpected terminal before first main");
                    };
                    let selected = session
                        .diagnostic_current_action_semantics()
                        .unwrap()
                        .iter()
                        .position(|action| matches!(action, ActionSemanticV1::Pass { .. }))
                        .expect("all-land fixture offers pass");
                    session
                        .step(decision.episode_id, decision.step, selected as u32)
                        .unwrap();
                }
                assert!(reached);
                let state = session.game_state();
                assert_eq!(
                    state.players[target.index()].draws_this_turn,
                    u32::from(target != starting)
                );
                assert_eq!(state.players[target.opponent().index()].draws_this_turn, 0);
                assert_eq!(
                    state.players[target.index()].hand.len(),
                    7 + usize::from(target != starting)
                );
                if target == starting {
                    assert_eq!(
                        session.diagnostic_current_action_semantics().unwrap().len(),
                        8
                    );
                }
            }
            config.opening_protocol = Bo3OpeningProtocolV1::LegacyKeepSevenV1;
            for uses_v3 in [false, true] {
                let legacy =
                    create_bo3_episode_v1(&config, uses_v3, 2, 99, mainboards.clone(), starting)
                        .unwrap();
                assert_eq!(legacy.game_state().players[0].draws_this_turn, 7);
                assert_eq!(legacy.game_state().players[1].draws_this_turn, 7);
            }
            config.opening_protocol = Bo3OpeningProtocolV1::KeepSevenV2;
        }
    }

    #[test]
    fn corrected_keep_seven_rejects_v2_before_building_decks() {
        let mut config = opening_test_config();
        config.opening_protocol = Bo3OpeningProtocolV1::KeepSevenV2;
        let error = create_bo3_episode_v1(&config, false, 1, 99, [vec![], vec![]], PlayerId::P0)
            .err()
            .unwrap();
        assert_eq!(error, "keep_seven_v2 requires the V3 observation contract");
    }

    fn summary() -> GameSummaryV1 {
        GameSummaryV1 {
            schema_version: 1,
            checkpoint_weights_hash: "a".repeat(64),
            checkpoint_git_head: "b".repeat(40),
            winner: Some(PlayerId::P0),
            opponent_evidence: [
                vec![OpponentEvidenceRowV1 {
                    card_id: 42,
                    first_seen_turn: 2,
                    end_of_game_zone: Zone::Hand,
                }],
                vec![],
            ],
            own_card_outcomes: [
                BTreeMap::from([(
                    1,
                    OwnCardOutcomeV1 {
                        times_drawn: 2,
                        ..Default::default()
                    },
                )]),
                BTreeMap::new(),
            ],
            resource_curve: ResourceCurveV1::default(),
        }
    }
    #[test]
    fn opponent_private_outcomes_and_hidden_location_cannot_change_projected_input() {
        let registered = DeckConfigurationV1::new_exact_v1(vec![1; 60], vec![2; 15]).unwrap();
        let first = summary();
        let original =
            project_sideboard_input_v1(&registered, PlayerId::P0, &[first.clone()], 2, [1, 0])
                .unwrap();
        let mut changed = first;
        changed.own_card_outcomes[1].insert(
            999,
            OwnCardOutcomeV1 {
                times_drawn: 12,
                cast: true,
                ..Default::default()
            },
        );
        changed.opponent_evidence[1].push(OpponentEvidenceRowV1 {
            card_id: 888,
            first_seen_turn: 1,
            end_of_game_zone: Zone::Battlefield,
        });
        changed.opponent_evidence[0][0].end_of_game_zone = Zone::Library;
        changed.resource_curve.hand_size_by_turn[1] = vec![99; 4];
        assert_eq!(
            original,
            project_sideboard_input_v1(&registered, PlayerId::P0, &[changed], 2, [1, 0]).unwrap()
        );
        assert_eq!(
            original.opponent_evidence[0].zone,
            VisibleEvidenceZoneV1::OtherVisible
        );
    }
    #[test]
    fn identical_public_card_copies_are_collapsed_without_exposing_object_identity() {
        let registered = DeckConfigurationV1::new_exact_v1(vec![1; 60], vec![2; 15]).unwrap();
        let mut history = summary();
        history.opponent_evidence[0].push(OpponentEvidenceRowV1 {
            card_id: 42,
            first_seen_turn: 5,
            end_of_game_zone: Zone::Graveyard,
        });
        let input =
            project_sideboard_input_v1(&registered, PlayerId::P0, &[history], 2, [0, 1]).unwrap();
        assert_eq!(input.opponent_evidence.len(), 1);
        assert_eq!(input.opponent_evidence[0].first_seen_turn, 2);
        assert_eq!(input.acting_player_games_won, 0);
    }
}
