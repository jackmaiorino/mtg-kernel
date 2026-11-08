//! Exact three-printing consumer factory. Existing Full-only resets stay intact.
use super::*;

impl FastActorSessionV1 {
    pub(crate) fn reset_limited_return_v1(
        config: &crate::limited_return_consumer_v1::ResetV1,
    ) -> Result<Self, RlSessionError> {
        crate::limited_return_consumer_v1::preflight(config)
            .map_err(|error| session_error(RlSessionErrorCode::UnsupportedDeck, &error))?;
        let state = crate::rl::build_preflighted_deck_pair_environment_v2(
            config.env_seed,
            &config.mainboards[0],
            &config.mainboards[1],
            match config.starting_player {
                crate::rl::PlayerSeatV1::P0 => PlayerId::P0,
                crate::rl::PlayerSeatV1::P1 => PlayerId::P1,
            },
        )
        .map_err(map_deck_pair_build_error_v2)?;
        Self::initialize_kaito_v75(
            config.episode_id,
            (config.max_physical_decisions, config.max_policy_steps),
            config.deck_ids.clone(),
            &config.mainboards,
            state,
        )
    }
}
