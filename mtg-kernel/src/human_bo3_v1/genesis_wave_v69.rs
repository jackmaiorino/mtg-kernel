//! Fixed-seat Human facade for the sealed Genesis Wave V69 owner.
//! Human handles and ordinals are prompt-local; executable native pairs stay private.
use super::visible::Handles;
use super::*;
use crate::{
    genesis_wave_observation_v69 as observation, graveyard_return_public_v61 as public,
    rl::{ActionSemanticV1, CardStableRefV1, RlContractError},
    rl_session::{
        FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1, GenesisWaveDecisionV69,
        GenesisWaveSessionV69, GenesisWaveSnapshotV69,
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
record!(HumanVisibleStateGenesisWaveV69 {
    common: super::elenda_v52::HumanVisibleStateElendaV52,
    prepared: super::prepared_v57::HumanPreparedFactsV57,
    next_five: super::next_five_v59::HumanNextFiveFactsV59,
    graveyard_return: HumanCallFactsV62,
    call: HumanNativeCallFactsV62,
    end_turn: HumanEndTurnFactsV68,
    genesis_wave: HumanGenesisWaveFactsV69
});
record!(HumanDecisionGenesisWaveV69 {
    schema: String, human_seat: PlayerSeatV1, prompt_seq: u64,
    state: HumanVisibleStateGenesisWaveV69,
    actions: Vec<HumanLegalActionV1>, action_meanings: Vec<HumanGenesisWaveCandidateV69>
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
record!(HumanGenesisWaveCandidateV69 {
    drake_activation: Option<super::next_five_v59::HumanNextFiveActivationV59>,
    programs: Vec<HumanCallTriggerV62>,
    selected_graveyard_target: Option<HumanCallTargetV62>,
    nine_lives_deaths: Vec<HumanCallDeathV62>,
    nine_lives_schedules: Vec<HumanCallScheduleV62>,
    call_returns: Vec<HumanNativeCallReturnV62>,
    end_turn_cleanup: Option<crate::time_stop_public_v68::CleanupV68>,
    disposed_stack_outcomes: Vec<HumanEndTurnDisposalV68>,
    compatibility_terminals: Vec<crate::time_stop_public_v68::CompatibilityTerminalV68>,
    genesis_waves: Vec<HumanWaveContextV69>
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
    obs: &observation::GenesisWaveObservationV69,
    a: &ActionSemanticV1,
    h: &Handles,
) -> Result<Option<String>, Error> {
    use crate::rl::TargetRefV1;
    match a {
        ActionSemanticV1::CastSpell{source,..} if source.card_db_id==487=>return Ok(Some(format!("Cast {}: pay X and {{G}}{{G}}{{G}}; reveal X and choose permanent cards with mana value at most X",h.name(source)?))),
        ActionSemanticV1::ChooseEffectOption{source,option_index,..} if source.card_db_id==487=>return Ok(Some(format!("Genesis Wave: choose X={option_index}"))),
        ActionSemanticV1::ChooseFullRangeCastXValueV59{source,x_value,..} if source.card_db_id==487=>return Ok(Some(format!("Genesis Wave: choose X={x_value}"))),
        ActionSemanticV1::ChooseEffectTarget{source,target:TargetRefV1::Object{object},..} if source.card_db_id==487=>return Ok(Some(format!("Genesis Wave: choose {}",h.name(object)?))),
        ActionSemanticV1::FinishEffectSelection{source,..} if source.card_db_id==487=>return Ok(Some("Genesis Wave: finish the current revealed-card selection".into())),
        _=>{}
    }
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
    super::prepared_v57::choice_label_genesis_wave(obs, a, h)
}
pub(super) fn project_with_handles(
    decision: &GenesisWaveDecisionV69,
    human: PlayerSeatV1,
    prompt_seq: u64,
) -> Result<(HumanDecisionGenesisWaveV69, Vec<u32>, Handles), Error> {
    let obs = decision.observation();
    if obs.graveyard_return.viewer != human || obs.common.common.acting_player != human {
        return Err(Error::NotHumanTurn);
    }
    crate::genesis_wave_candidates_v69::validate_artifact(decision.candidates_v69())
        .map_err(|_| Error::InvalidVisibleReference)?;
    let (common, actions, indexes, mut handles) =
        super::ravenous_v51::project_genesis_wave_base_with_handles(
            decision.candidates_v69(),
            decision.actions(),
            human,
            decision.family_projections_v69(),
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
    let genesis_wave = genesis_wave_facts(&obs.genesis_wave, &mut handles)?;
    let action_meanings = indexes
        .iter()
        .map(|i| {
            let r = decision
                .candidates_v69()
                .candidates
                .get(*i as usize)
                .ok_or(Error::InvalidAction)?;
            Ok(HumanGenesisWaveCandidateV69 {
                genesis_waves: r
                    .genesis_waves
                    .iter()
                    .map(|w| {
                        Ok(HumanWaveContextV69 {
                            occurrence: w.occurrence,
                            stage: w.stage,
                            source: handles.get(&w.source)?.clone(),
                            x_value: w.x_value,
                        })
                    })
                    .collect::<Result<_, Error>>()?,
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
        HumanDecisionGenesisWaveV69 {
            schema: "mtg-kernel-human-genesis-wave-decision/v69".into(),
            human_seat: human,
            prompt_seq,
            state: HumanVisibleStateGenesisWaveV69 {
                common,
                prepared,
                next_five,
                graveyard_return,
                call,
                end_turn,
                genesis_wave,
            },
            actions,
            action_meanings,
        },
        indexes,
        handles,
    ))
}
fn project(
    decision: &GenesisWaveDecisionV69,
    human: PlayerSeatV1,
    prompt_seq: u64,
) -> Result<(HumanDecisionGenesisWaveV69, Vec<u32>), Error> {
    project_with_handles(decision, human, prompt_seq)
        .map(|(visible, indexes, _)| (visible, indexes))
}
#[derive(Clone)]
struct AuthorityV69 {
    owner: Arc<()>,
    generation: u64,
}
impl PartialEq for AuthorityV69 {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && Arc::ptr_eq(&self.owner, &other.owner)
    }
}
impl Eq for AuthorityV69 {}

pub struct HumanGenesisWaveSessionV69 {
    inner: GenesisWaveSessionV69,
    authority: AuthorityV69,
    human_seat: PlayerSeatV1,
}
#[derive(Clone)]
pub struct HumanGenesisWaveSnapshotV69 {
    inner: GenesisWaveSnapshotV69,
    owner: Arc<()>,
}
impl HumanGenesisWaveSessionV69 {
    pub fn from_session(
        inner: FastActorSessionV1,
        human_seat: PlayerSeatV1,
    ) -> Result<Self, RlContractError> {
        Ok(Self {
            inner: GenesisWaveSessionV69::from_session(inner)?,
            human_seat,
            authority: AuthorityV69 {
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
        let inner = GenesisWaveSessionV69::reset_explicit_mainboards_v69(
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
            authority: AuthorityV69 {
                owner: Arc::new(()),
                generation: 0,
            },
        })
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.response()
    }
    pub fn snapshot_v69(&self) -> HumanGenesisWaveSnapshotV69 {
        HumanGenesisWaveSnapshotV69 {
            inner: self.inner.snapshot_v69(),
            owner: self.authority.owner.clone(),
        }
    }
    pub fn restore_v69(&mut self, snapshot: &HumanGenesisWaveSnapshotV69) -> Result<(), Error> {
        if !Arc::ptr_eq(&self.authority.owner, &snapshot.owner) {
            return Err(Error::StaleDecision);
        }
        let generation = self
            .authority
            .generation
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        self.inner
            .restore_v69(&snapshot.inner)
            .map_err(|_| Error::StaleDecision)?;
        self.authority.generation = generation;
        Ok(())
    }
    fn decision(&self, expected: FastActorDecisionV1) -> Result<GenesisWaveDecisionV69, Error> {
        if expected.acting_player != self.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let decision = self
            .inner
            .decision_v69()
            .map_err(|_| Error::StaleDecision)?;
        if decision.expected() != expected {
            return Err(Error::StaleDecision);
        }
        self.inner
            .validate_decision_v69(&decision)
            .map_err(|_| Error::StaleDecision)?;
        Ok(decision)
    }
    pub fn public_observation(
        &self,
        expected: FastActorDecisionV1,
    ) -> Result<observation::GenesisWaveObservationV69, Error> {
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
            &observation::GenesisWaveObservationV69,
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
            .decision_v69()
            .map_err(|_| RlContractError("stale Call opponent decision".into()))?;
        if decision.expected() != expected {
            return Err(RlContractError("stale Call opponent decision".into()));
        }
        self.inner.validate_decision_v69(&decision)?;
        let index = policy(decision.observation(), decision.actions())?;
        self.inner.step_v69(&decision, index)
    }
}

struct BoundGenesisWaveV69 {
    authority: AuthorityV69,
    decision: GenesisWaveDecisionV69,
    visible: HumanDecisionGenesisWaveV69,
    indexes: Vec<u32>,
}
pub struct HumanGenesisWaveProjectorV69 {
    human_seat: PlayerSeatV1,
    next_prompt_seq: u64,
    pending: Option<BoundGenesisWaveV69>,
    accepted: Option<(HumanActionReceiptV1, AuthorityV69)>,
}
impl HumanGenesisWaveProjectorV69 {
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
        session: &HumanGenesisWaveSessionV69,
        expected: FastActorDecisionV1,
    ) -> Result<HumanDecisionGenesisWaveV69, Error> {
        if expected.acting_player != self.human_seat || session.human_seat != self.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let decision = session.decision(expected)?;
        if let Some(pending) = &self.pending {
            if pending.authority == session.authority
                && pending.decision.expected() == expected
                && session
                    .inner
                    .validate_decision_v69(&pending.decision)
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
        self.pending = Some(BoundGenesisWaveV69 {
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
        session: &mut HumanGenesisWaveSessionV69,
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
            .validate_decision_v69(&pending.decision)
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

record!(HumanRevealedV69 {
    card: HumanCardRefV1,
    mana_value: u16,
    permanent: bool,
    eligible: bool,
    aura: bool
});
record!(HumanAttachmentV69 {
    revealed_index: u16,
    original_host: HumanCardRefV1
});
record!(HumanWaveV69 {
    occurrence:u64,stage:crate::genesis_wave_public_v69::StageV69,source:HumanCardRefV1,
    controller:PlayerSeatV1,x_value:u16,is_copy:bool,is_flashback:bool,cast_method:crate::state::CastMethodV4,
    revealed:Vec<HumanRevealedV69>,selected_indices:Vec<u16>,attachments:Vec<HumanAttachmentV69>,
    failed_aura_indices:Vec<u16>,remainder_indices:Vec<u16>,answer_indices:Vec<u16>,
    answer_host:Option<HumanCardRefV1>,answer_waits_for_advance:bool,spell_departure:Option<crate::state::Zone>
});
record!(HumanGenesisWaveFactsV69 { viewer:PlayerSeatV1,meaning:crate::rules_meaning_genesis_wave_v69::MeaningV69,visible_printings:Vec<HumanCardRefV1>,waves:Vec<HumanWaveV69> });
pub(super) fn genesis_wave_facts(
    v: &crate::genesis_wave_public_v69::FactsV69,
    h: &mut Handles,
) -> Result<HumanGenesisWaveFactsV69, Error> {
    crate::genesis_wave_public_v69::validate(v).map_err(|_| Error::InvalidVisibleReference)?;
    let waves = v
        .waves
        .iter()
        .map(|w| {
            Ok(HumanWaveV69 {
                occurrence: w.occurrence,
                stage: w.stage,
                source: h.insert(&w.source)?,
                controller: w.controller,
                x_value: w.x_value,
                is_copy: w.is_copy,
                is_flashback: w.is_flashback,
                cast_method: w.cast_method,
                revealed: w
                    .revealed
                    .iter()
                    .map(|r| {
                        Ok(HumanRevealedV69 {
                            card: h.insert(&r.card)?,
                            mana_value: r.mana_value,
                            permanent: r.permanent,
                            eligible: r.eligible,
                            aura: r.aura,
                        })
                    })
                    .collect::<Result<_, Error>>()?,
                selected_indices: w.selected_indices.clone(),
                attachments: w
                    .attachments
                    .iter()
                    .map(|a| {
                        Ok(HumanAttachmentV69 {
                            revealed_index: a.revealed_index,
                            original_host: h.insert(&a.original_host)?,
                        })
                    })
                    .collect::<Result<_, Error>>()?,
                failed_aura_indices: w.failed_aura_indices.clone(),
                remainder_indices: w.remainder_indices.clone(),
                answer_indices: w.answer_indices.clone(),
                answer_host: w.answer_host.as_ref().map(|r| h.insert(r)).transpose()?,
                answer_waits_for_advance: w.answer_waits_for_advance,
                spell_departure: w.spell_departure,
            })
        })
        .collect::<Result<_, Error>>()?;
    Ok(HumanGenesisWaveFactsV69 {
        viewer: v.viewer,
        meaning: v.meaning.clone(),
        visible_printings: v
            .visible_printings
            .iter()
            .map(|r| href(r, h))
            .collect::<Result<_, _>>()?,
        waves,
    })
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
pub(crate) mod tests;

record!(HumanWaveContextV69 {
    occurrence: u64,
    stage: crate::genesis_wave_public_v69::StageV69,
    source: HumanCardRefV1,
    x_value: u16
});
