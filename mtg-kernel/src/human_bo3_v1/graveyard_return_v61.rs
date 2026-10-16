//! Fixed-seat Human facade for the sealed GraveyardReturn V61 owner.
//! Human handles and ordinals are prompt-local; executable native pairs stay private.
use super::visible::Handles;
use super::*;
use crate::{
    graveyard_return_observation_v61 as observation, graveyard_return_public_v61 as public,
    rl::{ActionSemanticV1, CardStableRefV1, RlContractError},
    rl_session::{
        FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1, GraveyardReturnDecisionV61,
        GraveyardReturnSessionV61, GraveyardReturnSnapshotV61,
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
record!(HumanVisibleStateGraveyardReturnV61 {
    common: super::elenda_v52::HumanVisibleStateElendaV52,
    prepared: super::prepared_v57::HumanPreparedFactsV57,
    next_five: super::next_five_v59::HumanNextFiveFactsV59,
    graveyard_return: HumanGraveyardReturnFactsV61
});
record!(HumanDecisionGraveyardReturnV61 {
    schema: String, human_seat: PlayerSeatV1, prompt_seq: u64,
    state: HumanVisibleStateGraveyardReturnV61,
    actions: Vec<HumanLegalActionV1>, action_meanings: Vec<HumanGraveyardReturnCandidateV61>
});
record!(HumanGraveyardReturnMeaningV61 {
    family: crate::rules_meaning_graveyard_return_v61::FamilyV61,
    printed: crate::rules_meaning_graveyard_return_v61::PrintedV61,
    ordered_rules: Vec<crate::rules_meaning_graveyard_return_v61::InstructionV61>
});
record!(HumanGraveyardReturnRootV61 {
    original_source: HumanCardRefV1, current_original_source: Option<HumanCardRefV1>,
    captured_controller: PlayerSeatV1
});
record!(HumanGraveyardReturnTargetV61 {
    original: HumanCardRefV1, current_exact_graveyard_card: Option<HumanCardRefV1>, legal_now: bool
});
record!(HumanGraveyardReturnTriggerV61 {
    family: crate::rules_meaning_graveyard_return_v61::FamilyV61,
    stage: public::StageV61, position: u32, source: HumanGraveyardReturnRootV61,
    program: public::ProgramV61, targets: Vec<HumanGraveyardReturnTargetV61>
});
record!(HumanGraveyardReturnCurrentV61 {
    family: crate::rules_meaning_graveyard_return_v61::FamilyV61, card: HumanCardRefV1,
    printed_abilities_active: bool, power: Option<i32>, toughness: Option<i32>
});
record!(HumanGraveyardReturnAuthorizedV61 {
    family: crate::rules_meaning_graveyard_return_v61::FamilyV61,
    card: HumanCardRefV1
});
record!(HumanGraveyardReturnCountersV61 {
    card: HumanCardRefV1,
    counters: public::CountersV61
});
record!(HumanGraveyardReturnCastV61 {
    family: crate::rules_meaning_graveyard_return_v61::FamilyV61,
    position: u32,
    source: HumanCardRefV1,
    original_controller: PlayerSeatV1,
    is_uncast_copy: bool,
    payment_pending: bool,
    genuine_cast_finalized: bool
});
record!(HumanGraveyardReturnDepartureV61 {
    occurrence: u32,
    source: HumanGraveyardReturnRootV61,
    power: i32
});
record!(HumanGraveyardReturnDeathV61 {
    occurrence: u32, source: HumanGraveyardReturnRootV61, departure_revival: i32,
    return_base_before_replacements: i32, collection_completed: bool,
    exact_graveyard_card_now: Option<HumanCardRefV1>
});
record!(HumanGraveyardReturnScheduleV61 {
    occurrence: u32, source: HumanGraveyardReturnRootV61, departure_revival: i32,
    return_base_before_replacements: i32, next_future_any_player_end_step: bool,
    exact_graveyard_card_now: Option<HumanCardRefV1>
});
record!(HumanGraveyardReturnFactsV61 {
    viewer: PlayerSeatV1, meanings: Vec<HumanGraveyardReturnMeaningV61>,
    authorized: Vec<HumanGraveyardReturnAuthorizedV61>, current: Vec<HumanGraveyardReturnCurrentV61>,
    battlefield_counters: [Vec<HumanGraveyardReturnCountersV61>;2],
    casts: Vec<HumanGraveyardReturnCastV61>, triggers: Vec<HumanGraveyardReturnTriggerV61>,
    alesha_departures: Vec<HumanGraveyardReturnDepartureV61>,
    nine_lives_deaths: Vec<HumanGraveyardReturnDeathV61>,
    nine_lives_schedules: Vec<HumanGraveyardReturnScheduleV61>
});
record!(HumanGraveyardReturnCandidateV61 {
    drake_activation: Option<super::next_five_v59::HumanNextFiveActivationV59>,
    programs: Vec<HumanGraveyardReturnTriggerV61>,
    selected_graveyard_target: Option<HumanGraveyardReturnTargetV61>,
    nine_lives_deaths: Vec<HumanGraveyardReturnDeathV61>,
    nine_lives_schedules: Vec<HumanGraveyardReturnScheduleV61>
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
) -> Result<HumanGraveyardReturnRootV61, Error> {
    Ok(HumanGraveyardReturnRootV61 {
        original_source: href(&value.original_source, h)?,
        current_original_source: value
            .current_original_source
            .as_ref()
            .map(|r| href(r, h))
            .transpose()?,
        captured_controller: value.captured_controller,
    })
}
fn target(v: &public::TargetV61, h: &mut Handles) -> Result<HumanGraveyardReturnTargetV61, Error> {
    Ok(HumanGraveyardReturnTargetV61 {
        original: href(&v.original, h)?,
        current_exact_graveyard_card: v
            .current_exact_graveyard_card
            .as_ref()
            .map(|r| href(r, h))
            .transpose()?,
        legal_now: v.legal_now,
    })
}
fn trigger(
    v: &public::TriggerV61,
    h: &mut Handles,
) -> Result<HumanGraveyardReturnTriggerV61, Error> {
    Ok(HumanGraveyardReturnTriggerV61 {
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
fn death(v: &public::DeathV61, h: &mut Handles) -> Result<HumanGraveyardReturnDeathV61, Error> {
    Ok(HumanGraveyardReturnDeathV61 {
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
fn schedule(
    v: &public::ScheduleV61,
    h: &mut Handles,
) -> Result<HumanGraveyardReturnScheduleV61, Error> {
    Ok(HumanGraveyardReturnScheduleV61 {
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

/// Field mapper only, called below the complete V61 validator. Registering every
/// historical role before canonicalization preserves original/current relations.
pub(super) fn facts(
    v: &public::FactsV61,
    h: &mut Handles,
) -> Result<HumanGraveyardReturnFactsV61, Error> {
    let battlefield_counters = v.battlefield_counters.each_ref().map(|rows| {
        rows.iter()
            .map(|r| {
                Ok(HumanGraveyardReturnCountersV61 {
                    card: href(&r.card, h)?,
                    counters: r.counters.clone(),
                })
            })
            .collect::<Result<Vec<_>, Error>>()
    });
    let [left, right] = battlefield_counters;
    Ok(HumanGraveyardReturnFactsV61 {
        viewer: v.viewer,
        meanings: v
            .meanings
            .iter()
            .map(|r| HumanGraveyardReturnMeaningV61 {
                family: r.family,
                printed: r.printed.clone(),
                ordered_rules: r.ordered_rules.clone(),
            })
            .collect(),
        authorized: v
            .authorized
            .iter()
            .map(|r| {
                Ok(HumanGraveyardReturnAuthorizedV61 {
                    family: r.family,
                    card: href(&r.card, h)?,
                })
            })
            .collect::<Result<_, Error>>()?,
        current: v
            .current
            .iter()
            .map(|r| {
                Ok(HumanGraveyardReturnCurrentV61 {
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
                Ok(HumanGraveyardReturnCastV61 {
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
                Ok(HumanGraveyardReturnDepartureV61 {
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
    obs: &observation::GraveyardReturnObservationV61,
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
    super::prepared_v57::choice_label_graveyard_return(obs, a, h)
}
pub(super) fn project_with_handles(
    decision: &GraveyardReturnDecisionV61,
    human: PlayerSeatV1,
    prompt_seq: u64,
) -> Result<(HumanDecisionGraveyardReturnV61, Vec<u32>, Handles), Error> {
    let obs = decision.observation();
    if obs.graveyard_return.viewer != human || obs.common.common.acting_player != human {
        return Err(Error::NotHumanTurn);
    }
    crate::graveyard_return_candidates_v61::validate_artifact(decision.candidates_v61())
        .map_err(|_| Error::InvalidVisibleReference)?;
    let (common, actions, indexes, mut handles) =
        super::ravenous_v51::project_graveyard_return_base_with_handles(
            decision.candidates_v61(),
            decision.actions(),
            human,
            decision.family_projections_v61(),
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
    let graveyard_return = facts(&obs.graveyard_return, &mut handles)?;
    let action_meanings = indexes
        .iter()
        .map(|i| {
            let r = decision
                .candidates_v61()
                .candidates
                .get(*i as usize)
                .ok_or(Error::InvalidAction)?;
            Ok(HumanGraveyardReturnCandidateV61 {
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
        HumanDecisionGraveyardReturnV61 {
            schema: "mtg-kernel-human-graveyard-return-decision/v61".into(),
            human_seat: human,
            prompt_seq,
            state: HumanVisibleStateGraveyardReturnV61 {
                common,
                prepared,
                next_five,
                graveyard_return,
            },
            actions,
            action_meanings,
        },
        indexes,
        handles,
    ))
}
fn project(
    decision: &GraveyardReturnDecisionV61,
    human: PlayerSeatV1,
    prompt_seq: u64,
) -> Result<(HumanDecisionGraveyardReturnV61, Vec<u32>), Error> {
    project_with_handles(decision, human, prompt_seq)
        .map(|(visible, indexes, _)| (visible, indexes))
}
#[derive(Clone)]
struct AuthorityV61 {
    owner: Arc<()>,
    generation: u64,
}
impl PartialEq for AuthorityV61 {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && Arc::ptr_eq(&self.owner, &other.owner)
    }
}
impl Eq for AuthorityV61 {}

pub struct HumanGraveyardReturnSessionV61 {
    inner: GraveyardReturnSessionV61,
    authority: AuthorityV61,
    human_seat: PlayerSeatV1,
}
#[derive(Clone)]
pub struct HumanGraveyardReturnSnapshotV61 {
    inner: GraveyardReturnSnapshotV61,
    owner: Arc<()>,
}
impl HumanGraveyardReturnSessionV61 {
    pub fn from_session(
        inner: FastActorSessionV1,
        human_seat: PlayerSeatV1,
    ) -> Result<Self, RlContractError> {
        Ok(Self {
            inner: GraveyardReturnSessionV61::from_session(inner)?,
            human_seat,
            authority: AuthorityV61 {
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
        let inner = GraveyardReturnSessionV61::reset_explicit_mainboards_v61(
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
            authority: AuthorityV61 {
                owner: Arc::new(()),
                generation: 0,
            },
        })
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.response()
    }
    pub fn snapshot_v61(&self) -> HumanGraveyardReturnSnapshotV61 {
        HumanGraveyardReturnSnapshotV61 {
            inner: self.inner.snapshot_v61(),
            owner: self.authority.owner.clone(),
        }
    }
    pub fn restore_v61(&mut self, snapshot: &HumanGraveyardReturnSnapshotV61) -> Result<(), Error> {
        if !Arc::ptr_eq(&self.authority.owner, &snapshot.owner) {
            return Err(Error::StaleDecision);
        }
        let generation = self
            .authority
            .generation
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        self.inner
            .restore_v61(&snapshot.inner)
            .map_err(|_| Error::StaleDecision)?;
        self.authority.generation = generation;
        Ok(())
    }
    fn decision(&self, expected: FastActorDecisionV1) -> Result<GraveyardReturnDecisionV61, Error> {
        if expected.acting_player != self.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let decision = self
            .inner
            .decision_v61()
            .map_err(|_| Error::StaleDecision)?;
        if decision.expected() != expected {
            return Err(Error::StaleDecision);
        }
        self.inner
            .validate_decision_v61(&decision)
            .map_err(|_| Error::StaleDecision)?;
        Ok(decision)
    }
    pub fn public_observation(
        &self,
        expected: FastActorDecisionV1,
    ) -> Result<observation::GraveyardReturnObservationV61, Error> {
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
            &observation::GraveyardReturnObservationV61,
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
            .decision_v61()
            .map_err(|_| RlContractError("stale GraveyardReturn opponent decision".into()))?;
        if decision.expected() != expected {
            return Err(RlContractError(
                "stale GraveyardReturn opponent decision".into(),
            ));
        }
        self.inner.validate_decision_v61(&decision)?;
        let index = policy(decision.observation(), decision.actions())?;
        self.inner.step_v61(&decision, index)
    }
}

struct BoundGraveyardReturnV61 {
    authority: AuthorityV61,
    decision: GraveyardReturnDecisionV61,
    visible: HumanDecisionGraveyardReturnV61,
    indexes: Vec<u32>,
}
pub struct HumanGraveyardReturnProjectorV61 {
    human_seat: PlayerSeatV1,
    next_prompt_seq: u64,
    pending: Option<BoundGraveyardReturnV61>,
    accepted: Option<(HumanActionReceiptV1, AuthorityV61)>,
}
impl HumanGraveyardReturnProjectorV61 {
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
        session: &HumanGraveyardReturnSessionV61,
        expected: FastActorDecisionV1,
    ) -> Result<HumanDecisionGraveyardReturnV61, Error> {
        if expected.acting_player != self.human_seat || session.human_seat != self.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let decision = session.decision(expected)?;
        if let Some(pending) = &self.pending {
            if pending.authority == session.authority
                && pending.decision.expected() == expected
                && session
                    .inner
                    .validate_decision_v61(&pending.decision)
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
        self.pending = Some(BoundGraveyardReturnV61 {
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
        session: &mut HumanGraveyardReturnSessionV61,
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
            .validate_decision_v61(&pending.decision)
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

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;
