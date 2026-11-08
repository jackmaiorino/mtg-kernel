//! Explicit consumer factories over the same preflighted V75 initialization.
use super::*;

impl FastActorSessionV1 {
    pub(crate) fn reset_limited_return_v1(
        config: &crate::limited_return_consumer_v1::ResetV1,
    ) -> Result<Self, RlSessionError> {
        Self::reset_limited_return(config, false)
    }

    pub(crate) fn reset_limited_return_reveal_facts_v3(
        config: &crate::limited_return_consumer_v1::ResetV1,
    ) -> Result<Self, RlSessionError> {
        Self::reset_limited_return(config, true)
    }

    fn reset_limited_return(
        config: &crate::limited_return_consumer_v1::ResetV1,
        reveal_facts: bool,
    ) -> Result<Self, RlSessionError> {
        crate::limited_return_consumer_v1::preflight(config)
            .map_err(|error| session_error(RlSessionErrorCode::UnsupportedDeck, &error))?;
        Self::initialize_limited_consumer_v1(config, reveal_facts, false)
    }

    pub(crate) fn reset_limited_v68_v75_v1(
        config: &crate::limited_v68_v75_consumer_v1::ResetV1,
        reveal_facts: bool,
    ) -> Result<Self, RlSessionError> {
        crate::limited_v68_v75_consumer_v1::preflight(config)
            .map_err(|error| session_error(RlSessionErrorCode::UnsupportedDeck, &error))?;
        Self::initialize_limited_consumer_v1(config, reveal_facts, false)
    }

    pub(crate) fn reset_limited_artist_v1(
        config: &crate::artist_consumer_v1::ResetV1,
        reveal_facts: bool,
    ) -> Result<Self, RlSessionError> {
        crate::artist_consumer_v1::preflight(config)
            .map_err(|error| session_error(RlSessionErrorCode::UnsupportedDeck, &error))?;
        Self::initialize_limited_consumer_v1(config, reveal_facts, true)
    }

    fn initialize_limited_consumer_v1(
        config: &crate::limited_consumer_wire_v1::ResetV1,
        reveal_facts: bool,
        artist: bool,
    ) -> Result<Self, RlSessionError> {
        let mut state = crate::rl::build_preflighted_deck_pair_environment_v2(
            config.env_seed,
            &config.mainboards[0],
            &config.mainboards[1],
            match config.starting_player {
                crate::rl::PlayerSeatV1::P0 => PlayerId::P0,
                crate::rl::PlayerSeatV1::P1 => PlayerId::P1,
            },
        )
        .map_err(map_deck_pair_build_error_v2)?;
        if reveal_facts {
            state.reveal_knowledge_v4 = Some(Default::default());
        }
        Self::initialize_kaito_consumer_v1(
            config.episode_id,
            (config.max_physical_decisions, config.max_policy_steps),
            config.deck_ids.clone(),
            &config.mainboards,
            state,
            artist,
        )
    }
}
