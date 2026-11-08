//! Explicit consumer factories over the same preflighted V75 initialization.
use super::*;
use crate::policy_surface_v5::LimitedAppendixV1;

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
        Self::initialize_limited_consumer_v1(config, reveal_facts, LimitedAppendixV1::Legacy)
    }

    pub(crate) fn reset_limited_v68_v75_v1(
        config: &crate::limited_v68_v75_consumer_v1::ResetV1,
        reveal_facts: bool,
    ) -> Result<Self, RlSessionError> {
        crate::limited_v68_v75_consumer_v1::preflight(config)
            .map_err(|error| session_error(RlSessionErrorCode::UnsupportedDeck, &error))?;
        Self::initialize_limited_consumer_v1(config, reveal_facts, LimitedAppendixV1::Legacy)
    }

    pub(crate) fn reset_limited_artist_v1(
        config: &crate::artist_consumer_v1::ResetV1,
        reveal_facts: bool,
    ) -> Result<Self, RlSessionError> {
        crate::artist_consumer_v1::preflight(config)
            .map_err(|error| session_error(RlSessionErrorCode::UnsupportedDeck, &error))?;
        Self::initialize_limited_consumer_v1(config, reveal_facts, LimitedAppendixV1::Artist)
    }

    pub(crate) fn reset_limited_chosen_type_v1(
        config: &crate::chosen_type_consumer_v1::ResetV1,
        reveal_facts: bool,
    ) -> Result<Self, RlSessionError> {
        crate::chosen_type_consumer_v1::preflight(config)
            .map_err(|error| session_error(RlSessionErrorCode::UnsupportedDeck, &error))?;
        Self::initialize_limited_consumer_v1(config, reveal_facts, LimitedAppendixV1::ChosenType)
    }

    pub(crate) fn reset_limited_niv_v1(
        config: &crate::niv_consumer_v1::ResetV1,
        reveal_facts: bool,
    ) -> Result<Self, RlSessionError> {
        crate::niv_consumer_v1::preflight(config)
            .map_err(|error| session_error(RlSessionErrorCode::UnsupportedDeck, &error))?;
        Self::initialize_limited_consumer_v1(config, reveal_facts, LimitedAppendixV1::Niv)
    }

    pub(crate) fn reset_limited_drakuseth_v1(
        config: &crate::drakuseth_consumer_v1::ResetV1,
        reveal_facts: bool,
    ) -> Result<Self, RlSessionError> {
        crate::drakuseth_consumer_v1::preflight(config)
            .map_err(|error| session_error(RlSessionErrorCode::UnsupportedDeck, &error))?;
        Self::initialize_limited_consumer_v1(
            config,
            reveal_facts,
            crate::policy_surface_v5::LimitedAppendixV1::Drakuseth,
        )
    }

    pub(crate) fn reset_limited_embercleave_v1(
        config: &crate::embercleave_consumer_v1::ResetV1,
        reveal_facts: bool,
    ) -> Result<Self, RlSessionError> {
        crate::embercleave_consumer_v1::preflight(config)
            .map_err(|error| session_error(RlSessionErrorCode::UnsupportedDeck, &error))?;
        Self::initialize_limited_consumer_v1(config, reveal_facts, LimitedAppendixV1::Embercleave)
    }

    pub(crate) fn reset_limited_zimone_v1(
        config: &crate::zimone_consumer_v1::ResetV1,
        reveal_facts: bool,
    ) -> Result<Self, RlSessionError> {
        crate::zimone_consumer_v1::preflight(config)
            .map_err(|error| session_error(RlSessionErrorCode::UnsupportedDeck, &error))?;
        Self::initialize_limited_consumer_v1(config, reveal_facts, LimitedAppendixV1::Zimone)
    }

    pub(crate) fn reset_limited_omniscience_v1(
        config: &crate::omniscience_consumer_v1::ResetV1,
        reveal_facts: bool,
    ) -> Result<Self, RlSessionError> {
        crate::omniscience_consumer_v1::preflight(config)
            .map_err(|error| session_error(RlSessionErrorCode::UnsupportedDeck, &error))?;
        Self::initialize_limited_consumer_v1(config, reveal_facts, LimitedAppendixV1::Omniscience)
    }

    pub(crate) fn reset_limited_tutelage_v1(
        config: &crate::tutelage_consumer_v1::ResetV1,
        reveal_facts: bool,
    ) -> Result<Self, RlSessionError> {
        crate::tutelage_consumer_v1::preflight(config)
            .map_err(|error| session_error(RlSessionErrorCode::UnsupportedDeck, &error))?;
        Self::initialize_limited_consumer_v1(config, reveal_facts, LimitedAppendixV1::Tutelage)
    }

    pub(crate) fn reset_limited_mixed_v1(
        config: &crate::limited_consumer_wire_v1::ResetV1,
        reveal_facts: bool,
    ) -> Result<Self, RlSessionError> {
        crate::limited_mixed_consumer_v1::preflight(config)
            .map_err(|error| session_error(RlSessionErrorCode::UnsupportedDeck, &error))?;
        Self::initialize_limited_consumer_v1(config, reveal_facts, LimitedAppendixV1::Mixed)
    }

    pub(crate) fn reset_limited_moon_v1(
        config: &crate::moon_consumer_v1::ResetV1,
        reveal_facts: bool,
    ) -> Result<Self, RlSessionError> {
        crate::moon_consumer_v1::preflight(config)
            .map_err(|error| session_error(RlSessionErrorCode::UnsupportedDeck, &error))?;
        Self::initialize_limited_consumer_v1(config, reveal_facts, LimitedAppendixV1::Moon)
    }

    pub(crate) fn reset_limited_leyline_axe_v1(
        config: &crate::leyline_axe_consumer_v1::ResetV1,
        reveal_facts: bool,
    ) -> Result<Self, RlSessionError> {
        crate::leyline_axe_consumer_v1::preflight(config)
            .map_err(|error| session_error(RlSessionErrorCode::UnsupportedDeck, &error))?;
        Self::initialize_limited_consumer_v1(config, reveal_facts, LimitedAppendixV1::LeylineAxe)
    }

    pub(crate) fn reset_limited_progenitus_v1(
        config: &crate::progenitus_consumer_v1::ResetV1,
        reveal_facts: bool,
    ) -> Result<Self, RlSessionError> {
        crate::progenitus_consumer_v1::preflight(config)
            .map_err(|error| session_error(RlSessionErrorCode::UnsupportedDeck, &error))?;
        Self::initialize_limited_consumer_v1(config, reveal_facts, LimitedAppendixV1::Progenitus)
    }

    pub(crate) fn reset_limited_aberration_v1(
        config: &crate::aberration_consumer_v1::ResetV1,
        reveal_facts: bool,
    ) -> Result<Self, RlSessionError> {
        crate::aberration_consumer_v1::preflight(config)
            .map_err(|error| session_error(RlSessionErrorCode::UnsupportedDeck, &error))?;
        Self::initialize_limited_consumer_v1(config, reveal_facts, LimitedAppendixV1::Aberration)
    }

    pub(crate) fn reset_limited_lathril_v1(
        config: &crate::lathril_consumer_v1::ResetV1,
        reveal_facts: bool,
    ) -> Result<Self, RlSessionError> {
        crate::lathril_consumer_v1::preflight(config)
            .map_err(|error| session_error(RlSessionErrorCode::UnsupportedDeck, &error))?;
        Self::initialize_limited_consumer_v1(config, reveal_facts, LimitedAppendixV1::Lathril)
    }

    fn initialize_limited_consumer_v1(
        config: &crate::limited_consumer_wire_v1::ResetV1,
        reveal_facts: bool,
        appendix: LimitedAppendixV1,
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
            appendix,
        )
    }
}
