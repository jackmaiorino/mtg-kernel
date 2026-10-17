//! Fixed-seat Human facade for the sealed Time Stop V68 owner.
//! Human handles and ordinals are prompt-local; executable native pairs stay private.
use super::visible::Handles;
use super::*;
use crate::{
    end_turn_observation_v68 as observation, graveyard_return_public_v61 as public,
    rl::{ActionSemanticV1, CardStableRefV1, RlContractError},
    rl_session::{
        EndTurnDecisionV68, EndTurnSessionV68, EndTurnSnapshotV68, FastActorDecisionV1,
        FastActorResponseV1, FastActorSessionV1,
    },
};
use std::sync::Arc;
type Error = HumanDecisionErrorV1;
macro_rules! record {
    ($name:ident { $($field:ident : $ty:ty),* $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
        pub struct $name { $(pub $field: $ty),* }
    };
}
record!(HumanVisibleStateEndTurnV68 {
    common: super::elenda_v52::HumanVisibleStateElendaV52,
    prepared: super::prepared_v57::HumanPreparedFactsV57,
    next_five: super::next_five_v59::HumanNextFiveFactsV59,
    graveyard_return: HumanCallFactsV62,
    call: HumanNativeCallFactsV62,
    end_turn: HumanEndTurnFactsV68
});
record!(HumanDecisionEndTurnV68 {
    schema: String, human_seat: PlayerSeatV1, prompt_seq: u64,
    state: HumanVisibleStateEndTurnV68,
    actions: Vec<HumanLegalActionV1>, action_meanings: Vec<HumanEndTurnCandidateV68>
});
record!(HumanCallMeaningV62 {
    family: crate::rules_meaning_graveyard_return_v61::FamilyV61,
    printed: crate::rules_meaning_graveyard_return_v61::PrintedV61,
    ordered_rules: Vec<crate::rules_meaning_graveyard_return_v61::InstructionV61>
});
record!(HumanCallRootV62 {
    original_source: HumanCardRefV1, current_original_source: Option<HumanCardRefV1>,
    captured_controller: PlayerSeatV1
});
record!(HumanCallTargetV62 {
    original: HumanCardRefV1, current_exact_graveyard_card: Option<HumanCardRefV1>, legal_now: bool
});
record!(HumanCallTriggerV62 {
    family: crate::rules_meaning_graveyard_return_v61::FamilyV61,
    stage: public::StageV61, position: u32, source: HumanCallRootV62,
    program: public::ProgramV61, targets: Vec<HumanCallTargetV62>
});
record!(HumanCallCurrentV62 {
    family: crate::rules_meaning_graveyard_return_v61::FamilyV61, card: HumanCardRefV1,
    printed_abilities_active: bool, power: Option<i32>, toughness: Option<i32>
});
record!(HumanCallAuthorizedV62 {
    family: crate::rules_meaning_graveyard_return_v61::FamilyV61,
    card: HumanCardRefV1
});
record!(HumanCallCountersV62 {
    card: HumanCardRefV1,
    counters: public::CountersV61
});
record!(HumanCallCastV62 {
    family: crate::rules_meaning_graveyard_return_v61::FamilyV61,
    position: u32,
    source: HumanCardRefV1,
    original_controller: PlayerSeatV1,
    is_uncast_copy: bool,
    payment_pending: bool,
    genuine_cast_finalized: bool
});
record!(HumanCallDepartureV62 {
    occurrence: u32,
    source: HumanCallRootV62,
    power: i32
});
record!(HumanCallDeathV62 {
    occurrence: u32, source: HumanCallRootV62, departure_revival: i32,
    return_base_before_replacements: i32, collection_completed: bool,
    exact_graveyard_card_now: Option<HumanCardRefV1>
});
record!(HumanCallScheduleV62 {
    occurrence: u32, source: HumanCallRootV62, departure_revival: i32,
    return_base_before_replacements: i32, next_future_any_player_end_step: bool,
    exact_graveyard_card_now: Option<HumanCardRefV1>
});
record!(HumanCallFactsV62 {
    viewer: PlayerSeatV1, meanings: Vec<HumanCallMeaningV62>,
    authorized: Vec<HumanCallAuthorizedV62>, current: Vec<HumanCallCurrentV62>,
    battlefield_counters: [Vec<HumanCallCountersV62>;2],
    casts: Vec<HumanCallCastV62>, triggers: Vec<HumanCallTriggerV62>,
    alesha_departures: Vec<HumanCallDepartureV62>,
    nine_lives_deaths: Vec<HumanCallDeathV62>,
    nine_lives_schedules: Vec<HumanCallScheduleV62>
});
record!(HumanEndTurnCandidateV68 {
    drake_activation: Option<super::next_five_v59::HumanNextFiveActivationV59>,
    programs: Vec<HumanCallTriggerV62>,
    selected_graveyard_target: Option<HumanCallTargetV62>,
    nine_lives_deaths: Vec<HumanCallDeathV62>,
    nine_lives_schedules: Vec<HumanCallScheduleV62>,
    call_returns: Vec<HumanNativeCallReturnV62>,
    end_turn_cleanup: Option<crate::time_stop_public_v68::CleanupV68>,
    disposed_stack_outcomes: Vec<HumanEndTurnDisposalV68>,
    compatibility_terminals: Vec<crate::time_stop_public_v68::CompatibilityTerminalV68>
});
fn stable(
    value: &crate::optional_trigger_public_v1::ReferenceV1,
) -> Result<CardStableRefV1, Error> {
    use crate::optional_trigger_public_v1::IdentityV1;
    let card_db_id = match value.identity {
        IdentityV1::Known { card_id } => card_id,
        IdentityV1::FaceDown { disguised } => crate::card_def::card_id_by_name(if disguised {
            "Face-down creature"
        } else {
            "Face-down card"
        })
        .ok_or(Error::InvalidVisibleReference)?,
    };
    Ok(CardStableRefV1 {
        arena_id: value.object,
        card_db_id,
        owner: value.owner,
        controller: value.controller,
        zone: value.zone,
        zone_change_count: value.generation,
    })
}
fn href(
    value: &crate::optional_trigger_public_v1::ReferenceV1,
    h: &mut Handles,
) -> Result<HumanCardRefV1, Error> {
    h.insert(&stable(value)?)
}
fn root(
    value: &crate::optional_trigger_public_v1::RootV1,
    h: &mut Handles,
) -> Result<HumanCallRootV62, Error> {
    Ok(HumanCallRootV62 {
        original_source: href(&value.original_source, h)?,
        current_original_source: value
            .current_original_source
            .as_ref()
            .map(|r| href(r, h))
            .transpose()?,
        captured_controller: value.captured_controller,
    })
}
fn target(v: &public::TargetV61, h: &mut Handles) -> Result<HumanCallTargetV62, Error> {
    Ok(HumanCallTargetV62 {
        original: href(&v.original, h)?,
        current_exact_graveyard_card: v
            .current_exact_graveyard_card
            .as_ref()
            .map(|r| href(r, h))
            .transpose()?,
        legal_now: v.legal_now,
    })
}
fn trigger(v: &public::TriggerV61, h: &mut Handles) -> Result<HumanCallTriggerV62, Error> {
    Ok(HumanCallTriggerV62 {
        family: v.family,
        stage: v.stage,
        position: v.position,
        source: root(&v.source, h)?,
        program: v.program.clone(),
        targets: v
            .targets
            .iter()
            .map(|r| target(r, h))
            .collect::<Result<_, _>>()?,
    })
}
fn death(v: &public::DeathV61, h: &mut Handles) -> Result<HumanCallDeathV62, Error> {
    Ok(HumanCallDeathV62 {
        occurrence: v.occurrence,
        source: root(&v.source, h)?,
        departure_revival: v.departure_revival,
        return_base_before_replacements: v.return_base_before_replacements,
        collection_completed: v.collection_completed,
        exact_graveyard_card_now: v
            .exact_graveyard_card_now
            .as_ref()
            .map(|r| href(r, h))
            .transpose()?,
    })
}
fn schedule(v: &public::ScheduleV61, h: &mut Handles) -> Result<HumanCallScheduleV62, Error> {
    Ok(HumanCallScheduleV62 {
        occurrence: v.occurrence,
        source: root(&v.source, h)?,
        departure_revival: v.departure_revival,
        return_base_before_replacements: v.return_base_before_replacements,
        next_future_any_player_end_step: v.next_future_any_player_end_step,
        exact_graveyard_card_now: v
            .exact_graveyard_card_now
            .as_ref()
            .map(|r| href(r, h))
            .transpose()?,
    })
}

/// Field mapper only, called below the complete V62 validator. Registering every
/// historical role before canonicalization preserves original/current relations.
pub(super) fn inherited_facts(
    v: &public::FactsV61,
    h: &mut Handles,
) -> Result<HumanCallFactsV62, Error> {
    let battlefield_counters = v.battlefield_counters.each_ref().map(|rows| {
        rows.iter()
            .map(|r| {
                Ok(HumanCallCountersV62 {
                    card: href(&r.card, h)?,
                    counters: r.counters.clone(),
                })
            })
            .collect::<Result<Vec<_>, Error>>()
    });
    let [left, right] = battlefield_counters;
    Ok(HumanCallFactsV62 {
        viewer: v.viewer,
        meanings: v
            .meanings
            .iter()
            .map(|r| HumanCallMeaningV62 {
                family: r.family,
                printed: r.printed.clone(),
                ordered_rules: r.ordered_rules.clone(),
            })
            .collect(),
        authorized: v
            .authorized
            .iter()
            .map(|r| {
                Ok(HumanCallAuthorizedV62 {
                    family: r.family,
                    card: href(&r.card, h)?,
                })
            })
            .collect::<Result<_, Error>>()?,
        current: v
            .current
            .iter()
            .map(|r| {
                Ok(HumanCallCurrentV62 {
                    family: r.family,
                    card: href(&r.card, h)?,
                    printed_abilities_active: r.printed_abilities_active,
                    power: r.power,
                    toughness: r.toughness,
                })
            })
            .collect::<Result<_, Error>>()?,
        battlefield_counters: [left?, right?],
        casts: v
            .casts
            .iter()
            .map(|r| {
                Ok(HumanCallCastV62 {
                    family: r.family,
                    position: r.position,
                    source: href(&r.source, h)?,
                    original_controller: r.original_controller,
                    is_uncast_copy: r.is_uncast_copy,
                    payment_pending: r.payment_pending,
                    genuine_cast_finalized: r.genuine_cast_finalized,
                })
            })
            .collect::<Result<_, Error>>()?,
        triggers: v
            .triggers
            .iter()
            .map(|r| trigger(r, h))
            .collect::<Result<_, Error>>()?,
        alesha_departures: v
            .alesha_departures
            .iter()
            .map(|r| {
                Ok(HumanCallDepartureV62 {
                    occurrence: r.occurrence,
                    source: root(&r.source, h)?,
                    power: r.power,
                })
            })
            .collect::<Result<_, Error>>()?,
        nine_lives_deaths: v
            .nine_lives_deaths
            .iter()
            .map(|r| death(r, h))
            .collect::<Result<_, Error>>()?,
        nine_lives_schedules: v
            .nine_lives_schedules
            .iter()
            .map(|r| schedule(r, h))
            .collect::<Result<_, Error>>()?,
    })
}
pub(super) fn choice_label(
    obs: &observation::EndTurnObservationV68,
    a: &ActionSemanticV1,
    h: &Handles,
) -> Result<Option<String>, Error> {
    if let ActionSemanticV1::ChooseEffectBoolean {
        actor,
        source,
        value,
    } = a
    {
        let matching = obs
            .graveyard_return
            .triggers
            .iter()
            .filter(|r| {
                r.stage == public::StageV61::Resolving
                    && r.source.captured_controller == *actor
                    && stable(&r.source.original_source).is_ok_and(|s| s == *source)
                    && matches!(
                        r.program,
                        public::ProgramV61::AleshaRaid {
                            may_return_at_resolution: true,
                            optional_choice: public::AleshaChoiceV61::AwaitingCostlessReturnChoice,
                            ..
                        }
                    )
            })
            .collect::<Vec<_>>();
        if matching.len() == 1 {
            let [target] = matching[0].targets.as_slice() else {
                return Err(Error::InvalidVisibleReference);
            };
            return Ok(Some(format!(
                "{} returning {} with {} (no mana payment)",
                if *value { "Accept" } else { "Decline" },
                h.name(&stable(&target.original)?)?,
                h.name(source)?
            )));
        }
    }
    super::prepared_v57::choice_label_end_turn(obs, a, h)
}
pub(super) fn project_with_handles(
    decision: &EndTurnDecisionV68,
    human: PlayerSeatV1,
    prompt_seq: u64,
) -> Result<(HumanDecisionEndTurnV68, Vec<u32>, Handles), Error> {
    let obs = decision.observation();
    if obs.graveyard_return.viewer != human || obs.common.common.acting_player != human {
        return Err(Error::NotHumanTurn);
    }
    crate::end_turn_candidates_v68::validate_artifact(decision.candidates_v68())
        .map_err(|_| Error::InvalidVisibleReference)?;
    let (common, actions, indexes, mut handles) =
        super::ravenous_v51::project_end_turn_base_with_handles(
            decision.candidates_v68(),
            decision.actions(),
            human,
            decision.family_projections_v68(),
        )?;
    let prepared = super::prepared_v57::facts_parts(
        &obs.drake,
        &obs.spinner,
        &obs.quandary,
        &obs.phoenix,
        &mut handles,
        false,
    )?;
    let next_five = super::next_five_v59::register(&obs.next_five, &mut handles)?;
    let graveyard_return = inherited_facts(&obs.graveyard_return, &mut handles)?;
    let call = call_facts(&obs.call, &mut handles)?;
    let end_turn = end_turn_facts(&obs.end_turn, &mut handles)?;
    let action_meanings = indexes
        .iter()
        .map(|i| {
            let r = decision
                .candidates_v68()
                .candidates
                .get(*i as usize)
                .ok_or(Error::InvalidAction)?;
            Ok(HumanEndTurnCandidateV68 {
                call_returns: call.returns.clone(),
                end_turn_cleanup: end_turn.cleanup.clone(),
                disposed_stack_outcomes: end_turn.disposals.clone(),
                compatibility_terminals: end_turn.compatibility_terminals.clone(),
                drake_activation: r
                    .drake_activation
                    .as_ref()
                    .map(|v| {
                        Ok(super::next_five_v59::HumanNextFiveActivationV59 {
                            source: handles.get(&v.source)?.clone(),
                            kind: v.kind,
                            count: v.count,
                            before: v.before,
                            after: v.after,
                            any_priority: v.any_priority,
                            costs_paid_before_stack: v.costs_paid_before_stack,
                            countering_refunds_costs: v.countering_refunds_costs,
                        })
                    })
                    .transpose()?,
                programs: r
                    .programs
                    .iter()
                    .map(|r| trigger(r, &mut handles))
                    .collect::<Result<_, Error>>()?,
                selected_graveyard_target: r
                    .selected_graveyard_target
                    .as_ref()
                    .map(|r| target(r, &mut handles))
                    .transpose()?,
                nine_lives_deaths: r
                    .nine_lives_deaths
                    .iter()
                    .map(|r| death(r, &mut handles))
                    .collect::<Result<_, Error>>()?,
                nine_lives_schedules: r
                    .nine_lives_schedules
                    .iter()
                    .map(|r| schedule(r, &mut handles))
                    .collect::<Result<_, Error>>()?,
            })
        })
        .collect::<Result<_, Error>>()?;
    Ok((
        HumanDecisionEndTurnV68 {
            schema: "mtg-kernel-human-time-stop-decision/v68".into(),
            human_seat: human,
            prompt_seq,
            state: HumanVisibleStateEndTurnV68 {
                common,
                prepared,
                next_five,
                graveyard_return,
                call,
                end_turn,
            },
            actions,
            action_meanings,
        },
        indexes,
        handles,
    ))
}
fn project(
    decision: &EndTurnDecisionV68,
    human: PlayerSeatV1,
    prompt_seq: u64,
) -> Result<(HumanDecisionEndTurnV68, Vec<u32>), Error> {
    project_with_handles(decision, human, prompt_seq)
        .map(|(visible, indexes, _)| (visible, indexes))
}
#[derive(Clone)]
struct AuthorityV68 {
    owner: Arc<()>,
    generation: u64,
}
impl PartialEq for AuthorityV68 {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && Arc::ptr_eq(&self.owner, &other.owner)
    }
}
impl Eq for AuthorityV68 {}

pub struct HumanEndTurnSessionV68 {
    inner: EndTurnSessionV68,
    authority: AuthorityV68,
    human_seat: PlayerSeatV1,
}
#[derive(Clone)]
pub struct HumanEndTurnSnapshotV68 {
    inner: EndTurnSnapshotV68,
    owner: Arc<()>,
}
impl HumanEndTurnSessionV68 {
    pub fn from_session(
        inner: FastActorSessionV1,
        human_seat: PlayerSeatV1,
    ) -> Result<Self, RlContractError> {
        Ok(Self {
            inner: EndTurnSessionV68::from_session(inner)?,
            human_seat,
            authority: AuthorityV68 {
                owner: Arc::new(()),
                generation: 0,
            },
        })
    }
    pub fn reset(
        episode_id: u64,
        env_seed: u64,
        limits: (u64, u64),
        deck_ids: crate::rl_session::SessionDeckIdsV1,
        mainboards: [Vec<u16>; 2],
        starting_player: crate::ids::PlayerId,
        human_seat: PlayerSeatV1,
    ) -> Result<Self, crate::rl_session::RlSessionError> {
        let inner = EndTurnSessionV68::reset_explicit_mainboards_v68(
            episode_id,
            env_seed,
            limits,
            deck_ids,
            mainboards,
            starting_player,
        )?;
        Ok(Self {
            inner,
            human_seat,
            authority: AuthorityV68 {
                owner: Arc::new(()),
                generation: 0,
            },
        })
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.response()
    }
    pub fn snapshot_v68(&self) -> HumanEndTurnSnapshotV68 {
        HumanEndTurnSnapshotV68 {
            inner: self.inner.snapshot_v68(),
            owner: self.authority.owner.clone(),
        }
    }
    pub fn restore_v68(&mut self, snapshot: &HumanEndTurnSnapshotV68) -> Result<(), Error> {
        if !Arc::ptr_eq(&self.authority.owner, &snapshot.owner) {
            return Err(Error::StaleDecision);
        }
        let generation = self
            .authority
            .generation
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        self.inner
            .restore_v68(&snapshot.inner)
            .map_err(|_| Error::StaleDecision)?;
        self.authority.generation = generation;
        Ok(())
    }
    fn decision(&self, expected: FastActorDecisionV1) -> Result<EndTurnDecisionV68, Error> {
        if expected.acting_player != self.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let decision = self
            .inner
            .decision_v68()
            .map_err(|_| Error::StaleDecision)?;
        if decision.expected() != expected {
            return Err(Error::StaleDecision);
        }
        self.inner
            .validate_decision_v68(&decision)
            .map_err(|_| Error::StaleDecision)?;
        Ok(decision)
    }
    pub fn public_observation(
        &self,
        expected: FastActorDecisionV1,
    ) -> Result<observation::EndTurnObservationV68, Error> {
        Ok(self.decision(expected)?.observation().clone())
    }
    /// Host-only opponent callback. It receives the actor-authorized public
    /// observation and legal actions, not a Human DTO or opponent private data.
    pub fn step_opponent_with<F>(
        &mut self,
        expected: FastActorDecisionV1,
        mut policy: F,
    ) -> Result<FastActorResponseV1, RlContractError>
    where
        F: FnMut(
            &observation::EndTurnObservationV68,
            &[ActionSemanticV1],
        ) -> Result<u32, RlContractError>,
    {
        if expected.acting_player == self.human_seat {
            return Err(RlContractError(
                "opponent callback cannot act for Human seat".into(),
            ));
        }
        let decision = self
            .inner
            .decision_v68()
            .map_err(|_| RlContractError("stale Call opponent decision".into()))?;
        if decision.expected() != expected {
            return Err(RlContractError("stale Call opponent decision".into()));
        }
        self.inner.validate_decision_v68(&decision)?;
        let index = policy(decision.observation(), decision.actions())?;
        self.inner.step_v68(&decision, index)
    }
}

struct BoundEndTurnV68 {
    authority: AuthorityV68,
    decision: EndTurnDecisionV68,
    visible: HumanDecisionEndTurnV68,
    indexes: Vec<u32>,
}
pub struct HumanEndTurnProjectorV68 {
    human_seat: PlayerSeatV1,
    next_prompt_seq: u64,
    pending: Option<BoundEndTurnV68>,
    accepted: Option<(HumanActionReceiptV1, AuthorityV68)>,
}
impl HumanEndTurnProjectorV68 {
    pub fn new(human_seat: PlayerSeatV1) -> Self {
        Self {
            human_seat,
            next_prompt_seq: 1,
            pending: None,
            accepted: None,
        }
    }
    pub fn project_current(
        &mut self,
        session: &HumanEndTurnSessionV68,
        expected: FastActorDecisionV1,
    ) -> Result<HumanDecisionEndTurnV68, Error> {
        if expected.acting_player != self.human_seat || session.human_seat != self.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let decision = session.decision(expected)?;
        if let Some(pending) = &self.pending {
            if pending.authority == session.authority
                && pending.decision.expected() == expected
                && session
                    .inner
                    .validate_decision_v68(&pending.decision)
                    .is_ok()
            {
                let (visible, indexes) =
                    project(&decision, self.human_seat, pending.visible.prompt_seq)?;
                if visible == pending.visible && indexes == pending.indexes {
                    return Ok(visible);
                }
            }
        }
        let next = self
            .next_prompt_seq
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        let (visible, indexes) = project(&decision, self.human_seat, self.next_prompt_seq)?;
        self.pending = Some(BoundEndTurnV68 {
            authority: session.authority.clone(),
            decision,
            visible: visible.clone(),
            indexes,
        });
        self.next_prompt_seq = next;
        Ok(visible)
    }
    pub fn submit(
        &mut self,
        session: &mut HumanEndTurnSessionV68,
        request: HumanActionRequestV1,
    ) -> Result<HumanActionReceiptV1, Error> {
        if session.human_seat != self.human_seat {
            return Err(Error::NotHumanTurn);
        }
        if let Some((receipt, authority)) = &self.accepted {
            if receipt.prompt_seq == request.prompt_seq {
                if *authority != session.authority {
                    return Err(Error::StaleDecision);
                }
                return if receipt.action_index == request.action_index {
                    Ok(receipt.clone())
                } else {
                    Err(Error::ConflictingRetry)
                };
            }
        }
        let pending = self.pending.as_ref().ok_or(Error::StaleDecision)?;
        if pending.authority != session.authority
            || request.prompt_seq != pending.visible.prompt_seq
        {
            return Err(Error::StaleDecision);
        }
        if request.action_index as usize >= pending.visible.actions.len() {
            return Err(Error::InvalidAction);
        }
        session
            .inner
            .validate_decision_v68(&pending.decision)
            .map_err(|_| Error::StaleDecision)?;
        let engine_index = *pending
            .indexes
            .get(request.action_index as usize)
            .ok_or(Error::InvalidAction)?;
        session.inner.step_with_current_check(
            &pending.decision,
            engine_index,
            |_, _| {
                let (visible, indexes) = project(
                    &pending.decision,
                    self.human_seat,
                    pending.visible.prompt_seq,
                )?;
                if visible != pending.visible || indexes != pending.indexes {
                    return Err(Error::StaleDecision);
                }
                Ok(())
            },
            |_| Error::StaleDecision,
            |_| Error::ExecutionFailed,
        )?;
        self.pending = None;
        let receipt = HumanActionReceiptV1 {
            prompt_seq: request.prompt_seq,
            action_index: request.action_index,
        };
        self.accepted = Some((receipt.clone(), session.authority.clone()));
        Ok(receipt)
    }
}

record!(HumanCallRelationV62 {
    object_role: u32,
    incarnation_role: u32,
    owner: PlayerSeatV1,
    controller: PlayerSeatV1,
    zone: crate::state::Zone
});
record!(HumanNativeCallCurrentV62 {
    card: HumanCardRefV1,
    relation: HumanCallRelationV62,
    is_token: bool,
    tapped: bool,
    summoning_sick: bool,
    marked_damage: u32,
    counters: crate::valkyries_call_public_v62::CountersV62,
    form: crate::optional_trigger_public_v1::FormV1
});
record!(HumanNativeCallDeathV62 { occurrence: u32, source: HumanCallRootV62, departure: HumanCallRelationV62, original_graveyard: HumanCallRelationV62, collection_completed: bool, current_exact_graveyard_card: Option<HumanCardRefV1> });
record!(HumanNativeCallReturnV62 { stage: crate::valkyries_call_public_v62::StageV62, position: u32, source: HumanCallRootV62, death_occurrence: u32, original_graveyard: HumanCallRelationV62, current_exact_graveyard_card: Option<HumanCardRefV1>, return_eligible_now: bool, under_physical_owner: PlayerSeatV1 });
record!(HumanNativeCallCastV62 {
    position: u32,
    source: HumanCardRefV1,
    original_controller: PlayerSeatV1,
    is_uncast_copy: bool,
    payment_pending: bool,
    genuine_cast_finalized: bool
});
record!(HumanNativeCallFactsV62 { viewer: PlayerSeatV1, meaning: crate::rules_meaning_valkyries_call_v62::MeaningV62, visible_printings: Vec<HumanCardRefV1>, battlefield: [Vec<HumanNativeCallCurrentV62>;2], casts: Vec<HumanNativeCallCastV62>, deaths: Vec<HumanNativeCallDeathV62>, returns: Vec<HumanNativeCallReturnV62> });
pub(super) fn call_facts(
    v: &crate::valkyries_call_public_v62::FactsV62,
    h: &mut Handles,
) -> Result<HumanNativeCallFactsV62, Error> {
    use crate::valkyries_call_public_v62::IncarnationV62;
    let mut objects = std::collections::BTreeMap::new();
    let mut incarnations = std::collections::BTreeMap::new();
    let mut relation = |r: &IncarnationV62| -> Result<HumanCallRelationV62, Error> {
        let next_object =
            u32::try_from(objects.len()).map_err(|_| Error::InvalidVisibleReference)?;
        let object_role = *objects.entry(r.object).or_insert(next_object);
        let next_incarnation =
            u32::try_from(incarnations.len()).map_err(|_| Error::InvalidVisibleReference)?;
        let incarnation_role = *incarnations
            .entry((r.object, r.generation))
            .or_insert(next_incarnation);
        Ok(HumanCallRelationV62 {
            object_role,
            incarnation_role,
            owner: r.owner,
            controller: r.controller,
            zone: r.zone,
        })
    };
    let visible_printings = v
        .visible_printings
        .iter()
        .map(|r| href(r, h))
        .collect::<Result<_, _>>()?;
    let mut battlefield = [vec![], vec![]];
    for (seat, rows) in v.battlefield.iter().enumerate() {
        for r in rows {
            let bound = IncarnationV62 {
                object: r.card.object,
                generation: r.card.generation,
                owner: r.card.owner,
                controller: r.card.controller,
                zone: r.card.zone,
            };
            battlefield[seat].push(HumanNativeCallCurrentV62 {
                card: href(&r.card, h)?,
                relation: relation(&bound)?,
                is_token: r.is_token,
                tapped: r.tapped,
                summoning_sick: r.summoning_sick,
                marked_damage: r.marked_damage,
                counters: r.counters.clone(),
                form: r.form.clone(),
            });
        }
    }
    let casts = v
        .casts
        .iter()
        .map(|r| {
            Ok(HumanNativeCallCastV62 {
                position: r.position,
                source: href(&r.source, h)?,
                original_controller: r.original_controller,
                is_uncast_copy: r.is_uncast_copy,
                payment_pending: r.payment_pending,
                genuine_cast_finalized: r.genuine_cast_finalized,
            })
        })
        .collect::<Result<_, Error>>()?;
    let deaths = v
        .deaths
        .iter()
        .map(|r| {
            Ok(HumanNativeCallDeathV62 {
                occurrence: r.occurrence,
                source: root(&r.source, h)?,
                departure: relation(&r.departure)?,
                original_graveyard: relation(&r.original_graveyard)?,
                collection_completed: r.collection_completed,
                current_exact_graveyard_card: r
                    .current_exact_graveyard_card
                    .as_ref()
                    .map(|r| href(r, h))
                    .transpose()?,
            })
        })
        .collect::<Result<_, Error>>()?;
    let returns = v
        .returns
        .iter()
        .map(|r| {
            Ok(HumanNativeCallReturnV62 {
                stage: r.stage,
                position: r.position,
                source: root(&r.source, h)?,
                death_occurrence: r.death_occurrence,
                original_graveyard: relation(&r.original_graveyard)?,
                current_exact_graveyard_card: r
                    .current_exact_graveyard_card
                    .as_ref()
                    .map(|r| href(r, h))
                    .transpose()?,
                return_eligible_now: r.return_eligible_now,
                under_physical_owner: r.under_physical_owner,
            })
        })
        .collect::<Result<_, Error>>()?;
    Ok(HumanNativeCallFactsV62 {
        viewer: v.viewer,
        meaning: v.meaning.clone(),
        visible_printings,
        battlefield,
        casts,
        deaths,
        returns,
    })
}

record!(HumanEndTurnDisposedV68 {
    original_stack_position: u32,
    original: HumanStackItemV1
});
record!(HumanEndTurnDisposalV68 {
    occurrence: u32, ending_spell_position: u32, original_step: crate::state::Step,
    original_active_player: PlayerSeatV1, disposed_rows: Vec<HumanEndTurnDisposedV68>
});
record!(HumanEndTurnFactsV68 {
    viewer: PlayerSeatV1, meaning: crate::rules_meaning_time_stop_v68::MeaningV68,
    visible_printings: Vec<HumanCardRefV1>, disposals: Vec<HumanEndTurnDisposalV68>,
    compatibility_terminals: Vec<crate::time_stop_public_v68::CompatibilityTerminalV68>,
    cleanup: Option<crate::time_stop_public_v68::CleanupV68>
});
pub(super) fn end_turn_facts(
    v: &crate::time_stop_public_v68::FactsV68,
    h: &mut Handles,
) -> Result<HumanEndTurnFactsV68, Error> {
    use super::visible::Project;
    crate::time_stop_public_v68::validate(v).map_err(|_| Error::InvalidVisibleReference)?;
    Ok(HumanEndTurnFactsV68 {
        viewer: v.viewer,
        meaning: v.meaning.clone(),
        visible_printings: v
            .visible_printings
            .iter()
            .map(|r| href(r, h))
            .collect::<Result<_, _>>()?,
        disposals: v
            .disposals
            .iter()
            .map(|d| {
                Ok(HumanEndTurnDisposalV68 {
                    occurrence: d.occurrence,
                    ending_spell_position: d.ending_spell_position,
                    original_step: d.original_step,
                    original_active_player: d.original_active_player,
                    disposed_rows: d
                        .disposed_rows
                        .iter()
                        .map(|r| {
                            Ok(HumanEndTurnDisposedV68 {
                                original_stack_position: r.original_stack_position,
                                original: r.original.project(h)?,
                            })
                        })
                        .collect::<Result<_, Error>>()?,
                })
            })
            .collect::<Result<_, Error>>()?,
        compatibility_terminals: v.compatibility_terminals.clone(),
        cleanup: v.cleanup.clone(),
    })
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;
