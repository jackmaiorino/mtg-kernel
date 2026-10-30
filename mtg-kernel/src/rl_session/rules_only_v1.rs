//! Rules-only Limited self-play (issue #110; the maintainer chose a rules-only
//! tier on 2026-10-26). The engine, the policy surface and the newest
//! cumulative candidate builder run without any consumer contract: no public
//! observation, Human, model or transport validator is consulted, so cards
//! whose rules are executable but whose consumers are unfinished can play.
//! It is for self-play, search and DraftZero-style evaluation only. It never
//! changes a card's Full or Partial capability, and no observation leaves it.
//!
//! Admission is explicit: a deck may hold Full cards plus executable Partial
//! cards that `admit` accepts (the driver uses
//! `crate::limited_rules_only_v1::admitted_v1`).
use super::*;

/// The newest cumulative core candidate chain (Kaito V75) without the
/// consumer `validate_state` calls its public wrappers make. A combat scan
/// step offers only the answers the surface accepts ([`flat_scan_menu_v2`]),
/// as the public projections do: declining an attacker that must attack, or
/// a blocker answer that leaves no legal block, is not offered (#376).
fn rules_only_candidates_v1(
    origin: &PolicyDecisionV5,
    state: &crate::state::GameState,
    surface: &PolicySurfaceV5,
) -> Result<Vec<CorePolicyActionCandidateV1>, RlContractError> {
    let candidates = rules_only_origin_candidates_v1(origin, state, surface)?;
    flat_scan_menu_v2(surface, state, origin, candidates).map_err(RlContractError)
}

fn rules_only_origin_candidates_v1(
    origin: &PolicyDecisionV5,
    state: &crate::state::GameState,
    surface: &PolicySurfaceV5,
) -> Result<Vec<CorePolicyActionCandidateV1>, RlContractError> {
    if let Some((player, pending, prefix)) = surface
        .hunter_order_parts_v45(state)
        .map_err(RlContractError)?
    {
        return crate::rl::hunter_trigger_order_candidates_v45(player, pending, prefix, state);
    }
    if let Some(candidates) = crate::divine_actions_v44::candidates_current_scope_kaito_v75(
        origin,
        state,
        surface,
        (true, true, true, true),
        true,
        true,
        true,
    )? {
        return Ok(candidates);
    }
    if matches!(
        origin,
        PolicyDecisionV5::Surface(SurfaceDecision::Decision(
            Decision::ChooseEntryColorV36 { .. }
        ))
    ) {
        banner_v36::actual_candidates(origin, state)
    } else if inspiration_v37::is_mill(state) {
        inspiration_v37::actual_candidates(origin, state)
    } else {
        crate::rl::core_policy_action_candidates_next_five_scoped_v59(
            origin,
            state,
            true,
            true,
            true,
            true,
            crate::rl::CommonProjectionScopeV42::KaitoV75,
        )
    }
}

/// `crate::rl::build_deck_pair_state_environment_v2_with_starting_player_v1`
/// without its Full-support preflight (admission is checked by the reset):
/// the same environment-v2 shuffles, library construction and opening hands.
fn rules_only_deck_pair_state_v1(
    pair_environment_seed: u64,
    mainboards: &[Vec<u16>; 2],
    starting_player: PlayerId,
) -> Result<crate::state::GameState, crate::rl::DeckPairBuildErrorV2> {
    use crate::environment_randomization_v2 as env2;
    let mut libraries = Vec::with_capacity(2);
    for (owner, deck) in [env2::PhysicalOwnerV2::P0, env2::PhysicalOwnerV2::P1]
        .into_iter()
        .zip(mainboards)
    {
        let seed = env2::derive_environment_randomization_seed_v2(
            pair_environment_seed,
            owner,
            env2::ShufflePurposeV2::InitialLibraryShuffle,
            0,
        )
        .map_err(crate::rl::DeckPairBuildErrorV2::EnvironmentRandomization)?;
        libraries.push(env2::permutation_v2(seed, deck));
    }
    let mut state =
        crate::state::GameState::new_from_libraries_environment_v2_with_starting_player_v1(
            &libraries[0],
            &libraries[1],
            crate::rl::card_name,
            pair_environment_seed,
            starting_player,
        );
    for _ in 0..7 {
        crate::event::propose_and_commit(
            &mut state,
            crate::event::ProposedEvent::draw(PlayerId::P0),
        );
        crate::event::propose_and_commit(
            &mut state,
            crate::event::ProposedEvent::draw(PlayerId::P1),
        );
    }
    Ok(state)
}

impl FastActorSessionV1 {
    /// A Limited game (40 or more cards per seat) whose every card is Full or
    /// accepted by `admit`. Same state, combat, mulligans, priority windows
    /// and trigger ordering as the owning Kaito V75 session.
    pub(crate) fn reset_rules_only_v1(
        episode_id: u64,
        env_seed: u64,
        limits: (u64, u64),
        deck_ids: SessionDeckIdsV1,
        mainboards: [Vec<u16>; 2],
        starting_player: PlayerId,
        admit: impl Fn(u16) -> bool,
    ) -> Result<Self, RlSessionError> {
        if !cfg!(all(
            feature = "limited-fdn-fixtures",
            not(feature = "standard-magezero-fixtures")
        )) {
            return Err(session_error(
                RlSessionErrorCode::UnsupportedDeck,
                "the rules-only session requires the Foundations catalog",
            ));
        }
        if !matches!(starting_player, PlayerId::P0 | PlayerId::P1)
            || mainboards.iter().any(|deck| {
                deck.len() < 40 || deck.len() > crate::limited_session_v1::MAX_CUSTOM_DECK_CARDS_V1
            })
            || limits.0 == 0
            || limits.1 == 0
        {
            return Err(session_error(
                RlSessionErrorCode::UnsupportedDeck,
                "invalid rules-only reset",
            ));
        }
        if let Some(&card) = mainboards.iter().flatten().find(|&&card| {
            let def = crate::card_def::CARD_DEFS.get(usize::from(card));
            !def.is_some_and(|def| {
                !def.is_token && (def.has_full_support() || (def.is_executable() && admit(card)))
            })
        }) {
            return Err(session_error(
                RlSessionErrorCode::UnsupportedDeck,
                &format!("card {card} is not admitted to rules-only play"),
            ));
        }
        let mut state = rules_only_deck_pair_state_v1(env_seed, &mainboards, starting_player)
            .map_err(map_deck_pair_build_error_v2)?;
        crate::combat_damage_v1::enable_foundations_combat_v1(&mut state)
            .map_err(|error| session_error(RlSessionErrorCode::UnsupportedDeck, &error))?;
        crate::london_mulligan_v1::enable_london_mulligans_v1(&mut state)
            .map_err(|error| session_error(RlSessionErrorCode::UnsupportedDeck, &error))?;
        let mut session = Self {
            deck_ids,
            deck_hashes: mainboards
                .each_ref()
                .map(|deck| explicit_deck_hash_v1(deck)),
            episode_id,
            max_physical_decisions: limits.0,
            max_policy_steps: limits.1,
            state,
            surface: PolicySurfaceV5::new_with_priority_mode_v1(
                crate::surface_v2::PriorityModeV1::EngineWindowsV1,
                SuppressionAuditMode::Off,
            ),
            environment_revision: 0,
            policy_step_count: 0,
            physical_decision_count: 0,
            current: None,
            flat_action_contract_mode: FlatActionContractModeV1::RulesOnlyV1,
            v3_spell_target_reference_adapter: false,
            #[cfg(feature = "gameplay-decision-trace-v1")]
            trace_pending: crate::gameplay_trace_v1::TraceSlot::default(),
            flat_action_cache_spare: None,
            flat_action_cache_spare_v2: None,
            pauper_london_v1: None,
            terminal: None,
        };
        session.surface.enable_hunter_order_v45();
        session.advance_to_decision_or_terminal();
        Ok(session)
    }

    /// `advance_to_decision_or_terminal` for `RulesOnlyV1`: the engine and
    /// surface steps, terminals and caps of the owning sessions, with no
    /// consumer guard or projection.
    pub(super) fn advance_rules_only_v1(&mut self) {
        self.current = None;
        let halt = |session: &mut Self, reason: String| {
            session.terminal = Some(halted_terminal(
                &session.deck_ids,
                session.deck_hashes,
                session.episode_id,
                reason,
                session.policy_step_count,
                session.physical_decision_count,
            ));
        };
        let surfaced = match self
            .surface
            .next_decision_owned(&mut self.state, self.environment_revision)
        {
            Ok(decision) => decision,
            Err(_) => return halt(self, "fail_closed:policy_surface_environment".into()),
        };
        match &surfaced {
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::GameOver { winner })) => {
                self.terminal = Some(terminal_from_winner(
                    &self.deck_ids,
                    self.deck_hashes,
                    self.episode_id,
                    *winner,
                    "game_over".to_string(),
                    self.policy_step_count,
                    self.physical_decision_count,
                ));
                return;
            }
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::Halted {
                mechanic,
                source,
            })) => {
                return halt(
                    self,
                    format!("engine_halted:{mechanic:?}:source:{}", source.0),
                );
            }
            _ => {}
        }
        let (substep_index, substep_count) =
            match self.surface.hunter_order_substeps_v45(&self.state) {
                Ok(Some(substeps)) => substeps,
                Ok(None) => surfaced.substep(),
                Err(_) => return halt(self, "fail_closed:Hunter trigger ordering".into()),
            };
        if substep_index == 0 && self.physical_decision_count >= self.max_physical_decisions {
            let _ = self.surface.discard_unanswered_scan();
            self.terminal = Some(truncated_terminal(
                &self.deck_ids,
                self.deck_hashes,
                self.episode_id,
                format!(
                    "physical_decision_cap_reached:{}",
                    self.max_physical_decisions
                ),
                self.policy_step_count,
                self.physical_decision_count,
            ));
            return;
        }
        let remaining_group_steps = u64::from(substep_count - substep_index);
        if self.policy_step_count.saturating_add(remaining_group_steps) > self.max_policy_steps {
            if substep_index == 0 {
                let _ = self.surface.discard_unanswered_scan();
            }
            self.terminal = Some(truncated_terminal(
                &self.deck_ids,
                self.deck_hashes,
                self.episode_id,
                format!("policy_step_cap_reached:{}", self.max_policy_steps),
                self.policy_step_count,
                self.physical_decision_count,
            ));
            return;
        }
        let Some(actor) = surfaced.actor(&self.state) else {
            return halt(
                self,
                "fail_closed:nonterminal decision without acting player".into(),
            );
        };
        let candidates = match rules_only_candidates_v1(&surfaced, &self.state, &self.surface) {
            Ok(candidates) => candidates,
            Err(error) => return halt(self, format!("fail_closed:{error}")),
        };
        if candidates.is_empty() {
            return halt(
                self,
                "fail_closed:nonterminal decision produced zero legal actions".into(),
            );
        }
        if u32::try_from(candidates.len()).is_err() {
            return halt(self, "fail_closed:policy action index exceeds u32".into());
        }
        let decision_kind = match &surfaced {
            PolicyDecisionV5::Surface(_) => FastActorDecisionKindV1::Surface,
            PolicyDecisionV5::AttackerInclusion { .. } => {
                FastActorDecisionKindV1::AttackerInclusion
            }
            PolicyDecisionV5::BlockerInclusion { .. } => FastActorDecisionKindV1::BlockerInclusion,
        };
        self.flat_action_cache_spare = None;
        self.flat_action_cache_spare_v2 = None;
        self.current = Some(FastActorCurrentDecisionV1 {
            actor,
            decision_kind,
            origin_decision: surfaced,
            physical_decision_id: self.physical_decision_count,
            substep_index,
            substep_count,
            candidates,
            environment_revision: self.environment_revision,
            bound_policy_step_count: self.policy_step_count,
            bound_physical_decision_count: self.physical_decision_count,
            flat_action_cache: None,
            flat_action_cache_error: None,
            flat_action_cache_v2: None,
            flat_action_cache_error_v2: None,
            liliana_projection_v73: None,
        });
    }
}
