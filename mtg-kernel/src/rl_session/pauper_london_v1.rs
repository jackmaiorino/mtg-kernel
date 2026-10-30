//! Opt-in London mulligans for the default Pauper game
//! (`docs/design/pauper_london_mulligans_v1.md`).
//!
//! The engine's two-player London module is reused unchanged. The session
//! owns the pregame boundary: while London is pending it drives
//! `london_mulligan_v1::drain_or_decide` itself, before any engine advance,
//! so completion never runs into turn 1 inside one advance. Completion
//! applies the `keep_seven_v2` handoff (zero turn-1 draw counters, clear the
//! pending event log), writes the opening record and removes the London
//! state, so every in-game state has the shape of a keep-seven game.
//!
//! Exposed decisions (a `learned` seat's announcements and bottoms) are
//! ordinary physical decisions and policy steps. Automatic keeps of a
//! `keep_seven` seat and forced zero-card keeps consume no counter, revision
//! or random draw. Any other pregame decision fails closed.

use super::*;
use crate::engine::Action;
use crate::london_mulligan_v1;

#[cfg(test)]
mod flat_v7_tests;
#[cfg(test)]
mod tests;

/// Opening protocol of an episode. Absent keeps every existing byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PauperOpeningV1 {
    #[serde(rename = "london-v1")]
    LondonV1,
}

/// Per-seat mulligan mode under `PauperOpeningV1::LondonV1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PauperMulliganModeV1 {
    /// The seat's policy answers every London decision.
    #[serde(rename = "learned")]
    Learned,
    /// The session keeps for the seat at its first announcement (a declared
    /// frozen opponent whose pregame rows are untrained). Never a learned
    /// seat's fallback.
    #[serde(rename = "keep_seven")]
    KeepSeven,
}

/// One seat of the opening record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PauperOpeningSeatRecordV1 {
    pub mode: PauperMulliganModeV1,
    pub mulligans_taken: u8,
    pub kept_hand_size: u8,
    /// Pregame decisions exposed to this seat's policy (announcements and
    /// bottoms); automatic and forced keeps are not counted.
    pub exposed_pregame_decisions: u32,
    pub forced_zero_card_keep: bool,
}

/// Written at completion, before the first engine advance into turn 1.
/// Indexed by physical seat.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PauperOpeningRecordV1 {
    pub opening: PauperOpeningV1,
    pub seats: [PauperOpeningSeatRecordV1; 2],
}

/// Session metadata of a London opening. Cloned with the session, so
/// `snapshot_v1`/`restore_v1` cover every pregame phase.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PauperLondonSessionV1 {
    modes: [PauperMulliganModeV1; 2],
    exposed: [u32; 2],
    record: Option<PauperOpeningRecordV1>,
}

/// Phase of a pending London opening.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LondonPregamePhaseV1 {
    Announce,
    Bottom,
}

/// Public facts of a pending London opening, read from the engine's own
/// serialized London state (its fields are private to that module, which is
/// shared with Limited and stays unchanged). Indexed by physical seat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LondonPregameViewV1 {
    pub(crate) phase: LondonPregamePhaseV1,
    pub(crate) starting_player: PlayerId,
    pub(crate) counts: [u8; 2],
    pub(crate) kept: [bool; 2],
    /// Announced a mulligan in the current round.
    pub(crate) mulligan_this_round: [bool; 2],
}

impl LondonPregameViewV1 {
    /// Cards `player` still has to bottom in the current bottom phase: its
    /// hand size minus its kept size (seven minus mulligans taken).
    pub(crate) fn cards_to_bottom(&self, state: &crate::state::GameState, player: PlayerId) -> u8 {
        let kept = 7_u8.saturating_sub(self.counts[player.index()]);
        let hand = u8::try_from(state.players[player.index()].hand.len()).unwrap_or(u8::MAX);
        if self.phase == LondonPregamePhaseV1::Bottom {
            hand.saturating_sub(kept)
        } else {
            0
        }
    }
}

#[derive(Deserialize)]
struct LondonStateMirrorV1 {
    starting_player: PlayerId,
    counts: [u8; 2],
    kept: [bool; 2],
    mulligan_this_round: [bool; 2],
    stage: LondonStageMirrorV1,
}

#[derive(Deserialize)]
enum LondonStageMirrorV1 {
    Announce {
        #[allow(dead_code)]
        next: u8,
    },
    Bottom {
        #[allow(dead_code)]
        next: u8,
        #[allow(dead_code)]
        remaining: u8,
    },
    Complete,
}

/// The pending London opening of `state`, or None when London is absent or
/// complete. Every in-game state has none.
pub(crate) fn london_pregame_view_v1(
    state: &crate::state::GameState,
) -> Result<Option<LondonPregameViewV1>, String> {
    let Some(pregame) = state.london_mulligans_v1.as_ref() else {
        return Ok(None);
    };
    let value = serde_json::to_value(pregame).map_err(|error| error.to_string())?;
    let mirror: LondonStateMirrorV1 =
        serde_json::from_value(value).map_err(|error| error.to_string())?;
    let phase = match mirror.stage {
        LondonStageMirrorV1::Announce { .. } => LondonPregamePhaseV1::Announce,
        LondonStageMirrorV1::Bottom { .. } => LondonPregamePhaseV1::Bottom,
        LondonStageMirrorV1::Complete => return Ok(None),
    };
    Ok(Some(LondonPregameViewV1 {
        phase,
        starting_player: mirror.starting_player,
        counts: mirror.counts,
        kept: mirror.kept,
        mulligan_this_round: mirror.mulligan_this_round,
    }))
}

impl FastActorSessionV1 {
    /// The V4-style training reset (`..._flat_action_v3_environment_v2_with_starting_player_v1`)
    /// with London mulligans enabled before any advance. The deal is that
    /// reset's deal; `modes` is indexed by physical seat.
    #[allow(clippy::too_many_arguments)]
    pub fn reset_pauper_london_v1(
        episode_id: u64,
        pair_environment_seed: u64,
        max_physical_decisions: u64,
        max_policy_steps: u64,
        deck_ids: SessionDeckIdsV1,
        mainboards: [Vec<u16>; 2],
        starting_player: PlayerId,
        modes: [PauperMulliganModeV1; 2],
    ) -> Result<Self, RlSessionError> {
        let (deck_hashes, mut state) = build_session_deck_pair_state_from_explicit_decks(
            &mainboards,
            pair_environment_seed,
            Some(starting_player),
        )?;
        london_mulligan_v1::enable_london_mulligans_v1(&mut state)
            .map_err(|error| session_error(RlSessionErrorCode::UnsupportedDeck, &error))?;
        let mut session = FastActorSessionV1 {
            deck_ids,
            deck_hashes,
            episode_id,
            max_physical_decisions,
            max_policy_steps,
            state,
            surface: PolicySurfaceV5::new_for_session(),
            environment_revision: 0,
            policy_step_count: 0,
            physical_decision_count: 0,
            current: None,
            flat_action_contract_mode: FlatActionContractModeV1::V3,
            v3_spell_target_reference_adapter: false,
            #[cfg(feature = "gameplay-decision-trace-v1")]
            trace_pending: crate::gameplay_trace_v1::TraceSlot::default(),
            flat_action_cache_spare: None,
            flat_action_cache_spare_v2: None,
            pauper_london_v1: Some(Box::new(PauperLondonSessionV1 {
                modes,
                exposed: [0; 2],
                record: None,
            })),
            terminal: None,
        };
        session.advance_to_decision_or_terminal();
        Ok(session)
    }

    /// Per-seat modes of a London session; None for every other session.
    pub fn pauper_london_modes_v1(&self) -> Option<[PauperMulliganModeV1; 2]> {
        self.pauper_london_v1.as_ref().map(|london| london.modes)
    }

    /// The opening record, once both seats have kept; None before completion
    /// and for every other session.
    pub fn pauper_opening_record_v1(&self) -> Option<&PauperOpeningRecordV1> {
        self.pauper_london_v1
            .as_ref()
            .and_then(|london| london.record.as_ref())
    }

    /// Whether this is a London session whose opening is still pending.
    pub fn pauper_london_pending_v1(&self) -> bool {
        self.pauper_london_v1.is_some() && london_mulligan_v1::has_pending(&self.state)
    }

    /// The session-owned pregame boundary, run at the start of every advance
    /// while London is pending. Returns with the state either at an exposed
    /// London decision (the ordinary advance then publishes it) or completed
    /// and canonicalized (the ordinary advance then enters turn 1).
    pub(super) fn drive_pauper_london_v1(&mut self) -> Result<(), String> {
        let Some(london) = self.pauper_london_v1.as_deref_mut() else {
            return Ok(());
        };
        if !london_mulligan_v1::has_pending(&self.state) {
            return Ok(());
        }
        loop {
            match london_mulligan_v1::drain_or_decide(&mut self.state)? {
                Some(Decision::ChooseLondonMulligan { player, .. }) => {
                    match london.modes[player.index()] {
                        PauperMulliganModeV1::KeepSeven => {
                            // Automatic progression: no counter, revision or
                            // random draw, applied through the engine.
                            crate::engine::step(
                                &mut self.state,
                                Action::ChooseLondonMulligan { mulligan: false },
                            )?;
                        }
                        PauperMulliganModeV1::Learned => {
                            london.exposed[player.index()] += 1;
                            return Ok(());
                        }
                    }
                }
                Some(Decision::ChooseLondonBottom { player, .. }) => {
                    if london.modes[player.index()] != PauperMulliganModeV1::Learned {
                        return Err("a keep-seven seat cannot reach a London bottom".into());
                    }
                    london.exposed[player.index()] += 1;
                    return Ok(());
                }
                Some(other) => {
                    return Err(format!(
                        "pregame decision other than a London choice: {other:?}"
                    ));
                }
                None => {
                    let pregame = self
                        .state
                        .london_mulligans_v1
                        .as_ref()
                        .ok_or("London state vanished before completion")?;
                    if !pregame.is_complete() || pregame.kept() != [true, true] {
                        return Err("London opening ended without both keeps".into());
                    }
                    let counts = pregame.counts();
                    let seats = [PlayerId::P0, PlayerId::P1].map(|player| {
                        let seat = player.index();
                        PauperOpeningSeatRecordV1 {
                            mode: london.modes[seat],
                            mulligans_taken: counts[seat],
                            kept_hand_size: u8::try_from(self.state.players[seat].hand.len())
                                .unwrap_or(u8::MAX),
                            exposed_pregame_decisions: london.exposed[seat],
                            forced_zero_card_keep: counts[seat] == 7,
                        }
                    });
                    if seats.iter().any(|seat| {
                        u16::from(seat.kept_hand_size) + u16::from(seat.mulligans_taken) != 7
                    }) {
                        return Err("kept hand size differs from seven minus mulligans".into());
                    }
                    // The `keep_seven_v2` handoff (`human_opening_v1.rs`):
                    // opening hands and London redraws are not turn-1 draws,
                    // and the setup event batch is not a turn-1 event batch.
                    // Pregame zone changes and draws stay in `event_history`.
                    for player in &mut self.state.players {
                        player.draws_this_turn = 0;
                    }
                    self.state.engine.event_log.clear();
                    self.state.london_mulligans_v1 = None;
                    london.record = Some(PauperOpeningRecordV1 {
                        opening: PauperOpeningV1::LondonV1,
                        seats,
                    });
                    return Ok(());
                }
            }
        }
    }
}

impl FastActorSessionV1 {
    /// The FlatV7 admission of a London pregame decision: the V4 flat action
    /// slice's rows for its exact engine menu. Only a London session reaches
    /// it (`encode_current_flat_action_slice_v4`); every other flat path keeps
    /// refusing London semantics, and the V3 cache and the V4 tensorizer
    /// refuse it too. Rows reuse action-kind column 15 (`london_opening`):
    /// an announcement row carries the actor's mulligans (`selected_count`),
    /// the opponent's mulligans (`number`), whether the opponent has kept
    /// (`option_index`) and announced a mulligan this round (`option_count`),
    /// and the mulligan flag (`VALUE`); a bottom row carries the cards still
    /// to bottom (`remaining`), the actor's mulligans and a `Card` reference
    /// to the exact own hand object.
    pub(super) fn encode_london_flat_action_slice_v7(
        &self,
        expected: FastActorDecisionV1,
        buffers: &mut FlatActionDecisionSliceBuffersV2<'_>,
    ) -> Result<FlatActionDecisionSliceV3, FlatActionDecisionSliceErrorV1> {
        use crate::surface::SurfaceAction;
        use FlatActionDecisionSliceErrorV1 as E;
        let current = self.current.as_ref().ok_or(E::NoCurrentDecision)?;
        flat_validate_expected_decision_v1(self, current, expected)?;
        let view = london_pregame_view_v1(&self.state)
            .map_err(|_| E::InvalidDecisionRelation)?
            .ok_or(E::InvalidDecisionRelation)?;
        let actor = current.actor;
        let seat: PlayerSeatV1 = actor.into();
        let (me, opponent) = (actor.index(), actor.opponent().index());
        let raw = core_policy_action_candidates_v5(&current.origin_decision, &self.state)
            .map_err(|_| E::InvalidDecisionRelation)?;
        if raw != current.candidates || view.kept[me] {
            return Err(E::InvalidDecisionRelation);
        }
        let menu_matches = match &current.origin_decision {
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(
                Decision::ChooseLondonMulligan {
                    player,
                    mulligan_count,
                },
            )) => {
                *player == actor
                    && view.phase == LondonPregamePhaseV1::Announce
                    && *mulligan_count == view.counts[me]
                    && current.candidates.len() == 2
            }
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(
                Decision::ChooseLondonBottom {
                    player,
                    remaining,
                    candidates,
                },
            )) => {
                *player == actor
                    && view.phase == LondonPregamePhaseV1::Bottom
                    && *remaining > 0
                    && *remaining == view.cards_to_bottom(&self.state, actor)
                    && *candidates == self.state.players[me].hand
                    && current.candidates.len() == candidates.len()
            }
            _ => false,
        };
        if !menu_matches {
            return Err(E::InvalidDecisionRelation);
        }
        let extension = crate::rl::policy_observation_extensions_v6(&self.state, actor)
            .map_err(|_| E::InvalidDecisionRelation)?;
        let mut actions_out = Vec::with_capacity(current.candidates.len());
        let mut card_refs = Vec::new();
        let mut objects_out: Vec<FlatActionObjectV2> = Vec::new();
        for (action_index, candidate) in current.candidates.iter().enumerate() {
            let mut core = FlatActionCoreV1 {
                kind: FlatActionKindV1::ChooseEffectNumber,
                selected_count: u16::from(view.counts[me]),
                ref_start: u32::try_from(card_refs.len()).map_err(|_| E::CheckedIntegerRange)?,
                ..FlatActionCoreV1::default()
            };
            match (&candidate.semantic, &candidate.policy_action) {
                (
                    ActionSemanticV1::ChooseLondonMulligan {
                        actor: row_actor,
                        mulligan_count,
                        mulligan,
                    },
                    PolicyActionV5::Surface(SurfaceAction::Action(Action::ChooseLondonMulligan {
                        mulligan: applied,
                    })),
                ) if *row_actor == seat
                    && mulligan == applied
                    && *mulligan_count == view.counts[me]
                    && view.phase == LondonPregamePhaseV1::Announce =>
                {
                    core.option_index = u16::from(view.kept[opponent]);
                    core.option_count = u16::from(view.mulligan_this_round[opponent]);
                    core.number = i32::from(view.counts[opponent]);
                    if *mulligan {
                        core.flags |= FLAT_ACTION_FLAG_VALUE_V1;
                    }
                }
                (
                    ActionSemanticV1::ChooseLondonBottom {
                        actor: row_actor,
                        remaining,
                        card,
                    },
                    PolicyActionV5::Surface(SurfaceAction::Action(Action::ChooseLondonBottom(
                        object,
                    ))),
                ) if *row_actor == seat
                    && flat_ref_matches_object_v1(card, *object)
                    && self.state.players[me].hand.get(action_index) == Some(object)
                    && view.phase == LondonPregamePhaseV1::Bottom =>
                {
                    core.remaining = *remaining;
                    core.ref_len = 1;
                    let (resolved, _) =
                        flat_action_v4::flat_visible_action_object_extension_aware_v4(
                            &self.state,
                            actor,
                            0,
                            FlatActionRefRoleV1::Card,
                            card,
                            &extension,
                        )?;
                    if !objects_out.contains(&resolved) {
                        if objects_out
                            .iter()
                            .any(|row| row.canonical_key() == resolved.canonical_key())
                        {
                            return Err(E::DuplicateCanonicalObject);
                        }
                        objects_out.push(resolved);
                    }
                    card_refs.push((
                        u32::try_from(action_index).map_err(|_| E::CheckedIntegerRange)?,
                        resolved,
                    ));
                }
                _ => return Err(E::UnsupportedActionSemantic),
            }
            actions_out.push(core);
        }
        objects_out.sort_unstable_by_key(|object| object.canonical_key());
        let mut refs_out = Vec::with_capacity(card_refs.len());
        for (action_index, object) in card_refs {
            let object_index = objects_out
                .binary_search_by_key(&object.canonical_key(), |row| row.canonical_key())
                .map_err(|_| E::InvalidActionReference)?;
            refs_out.push(FlatActionRefV2 {
                action_index,
                role: FlatActionRefRoleV1::Card,
                order_index: 0,
                associated_order: 0,
                card_token: object.card_token,
                object_index: u16::try_from(object_index).map_err(|_| E::CheckedIntegerRange)?,
            });
        }
        let action_count = u32::try_from(actions_out.len()).map_err(|_| E::CheckedIntegerRange)?;
        let commitment =
            flat_action_commitment_from_rows_v2(seat, &actions_out, &refs_out, &objects_out)?;
        let binding = flat_action_binding_v2(self, current, action_count, commitment);
        let (a, r, o) = (actions_out.len(), refs_out.len(), objects_out.len());
        if buffers.actions.len() < a {
            return Err(E::InsufficientActionCapacity {
                required: a,
                available: buffers.actions.len(),
            });
        }
        if buffers.refs.len() < r {
            return Err(E::InsufficientRefCapacity {
                required: r,
                available: buffers.refs.len(),
            });
        }
        if buffers.objects.len() < o {
            return Err(E::InsufficientObjectCapacity {
                required: o,
                available: buffers.objects.len(),
            });
        }
        buffers.actions[..a].copy_from_slice(&actions_out);
        buffers.refs[..r].copy_from_slice(&refs_out);
        buffers.objects[..o].copy_from_slice(&objects_out);
        Ok(FlatActionDecisionSliceV3 {
            binding: FlatActionDecisionBindingV3(binding),
            active_action_count: action_count,
            active_ref_count: u32::try_from(r).map_err(|_| E::CheckedIntegerRange)?,
            active_object_count: u16::try_from(o).map_err(|_| E::CheckedIntegerRange)?,
        })
    }

    /// The London facts of the acting player's pending pregame decision,
    /// actor-relative, for the FlatV7 encoder; None for every in-game
    /// decision and every other session.
    pub(crate) fn flat_london_pregame_v7(
        &self,
    ) -> Result<Option<crate::flat_policy_v4::FlatLondonPregameV7>, String> {
        if !self.pauper_london_pending_v1() {
            return Ok(None);
        }
        let Some(view) = london_pregame_view_v1(&self.state)? else {
            return Ok(None);
        };
        let actor = self
            .current
            .as_ref()
            .ok_or("no current London decision")?
            .actor;
        let order = [actor.index(), actor.opponent().index()];
        Ok(Some(crate::flat_policy_v4::FlatLondonPregameV7 {
            bottom: view.phase == LondonPregamePhaseV1::Bottom,
            mulligans_taken: order.map(|seat| view.counts[seat]),
            kept: order.map(|seat| view.kept[seat]),
            announced_mulligan: order.map(|seat| view.mulligan_this_round[seat]),
        }))
    }
}
