//! Executable cross-deck BO3 integration for visible-input sideboard policies.
//! The match runner never gives its sideboard callback the opposing registration.
//! Configuration identity is explicit and separate from the fixed BO1 catalog.

use crate::bo3_match::{
    GameOutcomeV1, GameStartV1, MatchOutcomeV1, MatchPhaseV1, PlayDrawChoiceV1,
};
use crate::bo3_session::BestOfThreeDeckMatchV1;
use crate::game_summary_v1::{
    try_run_fast_episode_with_summary_v1, GameSummaryV1, RemovalCounterspellTagsV1,
};
use crate::ids::PlayerId;
use crate::learned_sideboard_v1::{
    LearnedSideboardInputV1, SideboardActionV1, SideboardDeliberationStateV1,
    SideboardGameResourceV1, SideboardOpponentEvidenceV1, SideboardOwnCardOutcomeV1,
    VisibleEvidenceZoneV1,
};
use crate::paired_bo1_harness_v1::{paired_policy_seeds_v1, PairedBo1PolicyV1};
use crate::rl_session::FastActorSessionV1;
use crate::sideboard::DeckConfigurationV1;
use crate::state::SplitMix64;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LearnedBo3RunConfigV1 {
    pub deck_ids: [String; 2],
    pub seed: u64,
    pub game_one_chooser: PlayerId,
    pub max_physical_games: u8,
    pub max_physical_decisions: u64,
    pub max_policy_steps: u64,
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
    pub play_weights_sha256: String,
    pub sideboard_policy_identity: String,
    pub outcome: MatchOutcomeV1,
    pub games: Vec<LearnedBo3GameRecordV1>,
    pub sideboard_decisions: Vec<LearnedBo3SideboardRecordV1>,
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
    if config.max_physical_games < 3
        || config.max_physical_decisions == 0
        || config.max_policy_steps == 0
    {
        return Err(
            "positive episode limits and at least three physical games are required".to_owned(),
        );
    }
    let mut match_session = BestOfThreeDeckMatchV1::checked_in_pauper_v1(
        &config.deck_ids[0],
        &config.deck_ids[1],
        config.game_one_chooser,
    )
    .map_err(|error| error.to_string())?;
    let registered = [PlayerId::P0, PlayerId::P1].map(|seat| {
        match_session
            .registered_deck(seat)
            .unwrap()
            .registered_configuration()
            .clone()
    });
    let mut current = registered.clone();
    let mut summaries = Vec::new();
    let mut games = Vec::new();
    let mut sideboard_decisions = Vec::new();
    let mut seed_stream = SplitMix64::seed(config.seed);
    loop {
        let (game_index, chooser) = match match_session.match_state().phase() {
            MatchPhaseV1::Complete { outcome } => {
                return Ok(LearnedBo3ResultV1 {
                    schema: "kernel_learned_bo3/v1".to_owned(),
                    config,
                    play_weights_sha256: play_weights_sha256.to_owned(),
                    sideboard_policy_identity: sideboard_policy_identity.to_owned(),
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
                    &summaries,
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
        let mut episode = FastActorSessionV1::reset_with_explicit_decks_and_limits_flat_action_v2_environment_v2_with_starting_player_v1(
            u64::from(game_index), environment_seed, config.max_physical_decisions, config.max_policy_steps,
            config.deck_ids.clone(), current.each_ref().map(|c| c.mainboard().to_vec()), prepared.start().starting_player,
        ).map_err(|error| error.to_string())?;
        let summary = try_run_fast_episode_with_summary_v1(
            &mut episode,
            play_weights_sha256,
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
        summaries.push(summary);
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
