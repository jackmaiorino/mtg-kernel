//! Fixed-seat Human facade for the sealed Tinybones V72 owner.
//! Anonymous historical roles never reconstruct a later hidden printing.
use super::muldrotha_v71::*;
use super::visible::Handles;
use super::*;
use crate::{
    graveyard_return_public_v61 as public,
    rl::{ActionSemanticV1, RlContractError},
    rl_session::{
        FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1, TinybonesDecisionV72,
        TinybonesSessionV72, TinybonesSnapshotV72,
    },
    tinybones_observation_v72 as observation,
};
use std::{collections::BTreeMap, sync::Arc};
type Error = HumanDecisionErrorV1;
macro_rules! record { ($name:ident { $($field:ident:$ty:ty),* $(,)? })=>{#[derive(Debug,Clone,PartialEq,Eq,Serialize)] pub struct $name { $(pub $field:$ty),* }}; }
record!(HumanVisibleStateTinybonesV72 {
    common: super::elenda_v52::HumanVisibleStateElendaV52,
    prepared: super::prepared_v57::HumanPreparedFactsV57,
    next_five: super::next_five_v59::HumanNextFiveFactsV59,
    graveyard_return: HumanCallFactsV62,
    call: HumanNativeCallFactsV62,
    end_turn: HumanEndTurnFactsV68,
    genesis_wave: HumanGenesisWaveCompatibilityV71,
    vivien: HumanLoyaltyFactsV71,
    muldrotha: HumanMuldrothaFactsV71,
    tinybones: HumanTinybonesFactsV72
});
record!(HumanDecisionTinybonesV72 {
    schema: String, human_seat: PlayerSeatV1, prompt_seq: u64,
    state: HumanVisibleStateTinybonesV72,
    actions: Vec<HumanLegalActionV1>, action_meanings: Vec<HumanTinybonesCandidateV72>
});
record!(HumanTinybonesCandidateV72 {
    tinybones: HumanTinybonesActionV72,
    muldrotha_permission: Option<HumanMuldrothaPermissionActionV71>,
    drake_activation: Option<super::next_five_v59::HumanNextFiveActivationV59>,
    programs: Vec<HumanCallTriggerV62>,
    selected_graveyard_target: Option<HumanCallTargetV62>,
    nine_lives_deaths: Vec<HumanCallDeathV62>,
    nine_lives_schedules: Vec<HumanCallScheduleV62>,
    call_returns: Vec<HumanNativeCallReturnV62>,
    end_turn_cleanup: Option<crate::time_stop_public_v68::CleanupV68>,
    disposed_stack_outcomes: Vec<HumanEndTurnDisposalV68>,
    compatibility_terminals: Vec<crate::time_stop_public_v68::CompatibilityTerminalV68>,
    genesis_waves: Vec<HumanWaveContextV71>,
    vivien_choices:Vec<crate::vivien_public_v70::ChoiceContextV70>,
    vivien_activation:Option<HumanLoyaltyCostV71>
});
pub(super) fn choice_label(
    obs: &observation::TinybonesObservationV72,
    a: &ActionSemanticV1,
    h: &Handles,
) -> Result<Option<String>, Error> {
    use crate::rl::TargetRefV1;
    match a {
        ActionSemanticV1::ActivateAbility {
            source,
            ability_index: 0,
            ..
        } if source.card_db_id == 490 => {
            return Ok(Some(format!(
                "Activate {}: pay {{3}}{{B}}, tap; each opponent discards a card (sorcery only)",
                h.name(source)?
            )))
        }
        _ => {}
    }

    if let Some(p) = crate::muldrotha_public_v71::action_permission(&obs.muldrotha, a)
        .map_err(|_| Error::InvalidVisibleReference)?
    {
        return Ok(Some(match p.permission {
            None => "Use the ordinary graveyard play route".into(),
            Some(p) => format!(
                "Use the {:?} permission from {}",
                p.permanent_type,
                h.name(&stable(&p.source.original_source)?)?
            ),
        }));
    }

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
    super::prepared_v57::choice_label_tinybones(obs, a, h)
}
pub(super) fn project_with_handles(
    decision: &TinybonesDecisionV72,
    human: PlayerSeatV1,
    prompt_seq: u64,
) -> Result<(HumanDecisionTinybonesV72, Vec<u32>, Handles), Error> {
    let obs = decision.observation();
    if obs.graveyard_return.viewer != human || obs.common.common.acting_player != human {
        return Err(Error::NotHumanTurn);
    }
    crate::tinybones_candidates_v72::validate_artifact(decision.candidates_v72())
        .map_err(|_| Error::InvalidVisibleReference)?;
    let (common, actions, indexes, mut handles) =
        super::ravenous_v51::project_tinybones_base_with_handles(
            decision.candidates_v72(),
            decision.actions(),
            human,
            decision.family_projections_v72(),
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
    let vivien = loyalty_facts(&obs.vivien, &mut handles)?;
    let muldrotha = muldrotha_facts(&obs.muldrotha, &mut handles)?;
    let tinybones = tinybones_facts(&obs.tinybones, &mut handles)?;
    let action_meanings = indexes
        .iter()
        .map(|i| {
            let r = decision
                .candidates_v72()
                .candidates
                .get(*i as usize)
                .ok_or(Error::InvalidAction)?;
            Ok(HumanTinybonesCandidateV72 {
                tinybones: HumanTinybonesActionV72 {
                    triggers: tinybones.triggers.clone(),
                    plays: tinybones.plays.clone(),
                },
                muldrotha_permission: r
                    .muldrotha_permission
                    .as_ref()
                    .map(|a| {
                        Ok::<_, Error>(HumanMuldrothaPermissionActionV71 {
                            occurrence: a.occurrence,
                            menu_index: a.menu_index,
                            permission: a
                                .permission
                                .as_ref()
                                .map(|p| permission(p, &mut handles))
                                .transpose()?,
                        })
                    })
                    .transpose()?,
                vivien_choices: r.vivien_choices.clone(),
                vivien_activation: r
                    .vivien_activation
                    .as_ref()
                    .map(|c| {
                        Ok(HumanLoyaltyCostV71 {
                            source: handles.get(&c.source)?.clone(),
                            ability_index: c.ability_index,
                            before: c.before,
                            after: c.after,
                            paid_before_publication: c.paid_before_publication,
                            countering_refunds_cost: c.countering_refunds_cost,
                        })
                    })
                    .transpose()?,
                genesis_waves: r
                    .genesis_waves
                    .iter()
                    .map(|w| {
                        Ok(HumanWaveContextV71 {
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
        HumanDecisionTinybonesV72 {
            schema: "mtg-kernel-human-tinybones-decision/v72".into(),
            human_seat: human,
            prompt_seq,
            state: HumanVisibleStateTinybonesV72 {
                common,
                prepared,
                next_five,
                graveyard_return,
                call,
                end_turn,
                genesis_wave,
                vivien,
                muldrotha,
                tinybones,
            },
            actions,
            action_meanings,
        },
        indexes,
        handles,
    ))
}
fn project(
    decision: &TinybonesDecisionV72,
    human: PlayerSeatV1,
    prompt_seq: u64,
) -> Result<(HumanDecisionTinybonesV72, Vec<u32>), Error> {
    project_with_handles(decision, human, prompt_seq)
        .map(|(visible, indexes, _)| (visible, indexes))
}
#[derive(Clone)]
struct AuthorityV72 {
    owner: Arc<()>,
    generation: u64,
}
impl PartialEq for AuthorityV72 {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && Arc::ptr_eq(&self.owner, &other.owner)
    }
}
impl Eq for AuthorityV72 {}

pub struct HumanTinybonesSessionV72 {
    inner: TinybonesSessionV72,
    authority: AuthorityV72,
    human_seat: PlayerSeatV1,
}
#[derive(Clone)]
pub struct HumanTinybonesSnapshotV72 {
    inner: TinybonesSnapshotV72,
    owner: Arc<()>,
}
impl HumanTinybonesSessionV72 {
    pub fn from_session(
        inner: FastActorSessionV1,
        human_seat: PlayerSeatV1,
    ) -> Result<Self, RlContractError> {
        Ok(Self {
            inner: TinybonesSessionV72::from_session(inner)?,
            human_seat,
            authority: AuthorityV72 {
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
        let inner = TinybonesSessionV72::reset_explicit_mainboards_v72(
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
            authority: AuthorityV72 {
                owner: Arc::new(()),
                generation: 0,
            },
        })
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.response()
    }
    pub fn snapshot_v72(&self) -> HumanTinybonesSnapshotV72 {
        HumanTinybonesSnapshotV72 {
            inner: self.inner.snapshot_v72(),
            owner: self.authority.owner.clone(),
        }
    }
    pub fn restore_v72(&mut self, snapshot: &HumanTinybonesSnapshotV72) -> Result<(), Error> {
        if !Arc::ptr_eq(&self.authority.owner, &snapshot.owner) {
            return Err(Error::StaleDecision);
        }
        let generation = self
            .authority
            .generation
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        self.inner
            .restore_v72(&snapshot.inner)
            .map_err(|_| Error::StaleDecision)?;
        self.authority.generation = generation;
        Ok(())
    }
    fn decision(&self, expected: FastActorDecisionV1) -> Result<TinybonesDecisionV72, Error> {
        if expected.acting_player != self.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let decision = self
            .inner
            .decision_v72()
            .map_err(|_| Error::StaleDecision)?;
        if decision.expected() != expected {
            return Err(Error::StaleDecision);
        }
        self.inner
            .validate_decision_v72(&decision)
            .map_err(|_| Error::StaleDecision)?;
        Ok(decision)
    }
    pub fn public_observation(
        &self,
        expected: FastActorDecisionV1,
    ) -> Result<observation::TinybonesObservationV72, Error> {
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
            &observation::TinybonesObservationV72,
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
            .decision_v72()
            .map_err(|_| RlContractError("stale Call opponent decision".into()))?;
        if decision.expected() != expected {
            return Err(RlContractError("stale Call opponent decision".into()));
        }
        self.inner.validate_decision_v72(&decision)?;
        let index = policy(decision.observation(), decision.actions())?;
        self.inner.step_v72(&decision, index)
    }
}

struct BoundTinybonesV72 {
    authority: AuthorityV72,
    decision: TinybonesDecisionV72,
    visible: HumanDecisionTinybonesV72,
    indexes: Vec<u32>,
}
pub struct HumanTinybonesProjectorV72 {
    human_seat: PlayerSeatV1,
    next_prompt_seq: u64,
    pending: Option<BoundTinybonesV72>,
    accepted: Option<(HumanActionReceiptV1, AuthorityV72)>,
}
impl HumanTinybonesProjectorV72 {
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
        session: &HumanTinybonesSessionV72,
        expected: FastActorDecisionV1,
    ) -> Result<HumanDecisionTinybonesV72, Error> {
        if expected.acting_player != self.human_seat || session.human_seat != self.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let decision = session.decision(expected)?;
        if let Some(pending) = &self.pending {
            if pending.authority == session.authority
                && pending.decision.expected() == expected
                && session
                    .inner
                    .validate_decision_v72(&pending.decision)
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
        self.pending = Some(BoundTinybonesV72 {
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
        session: &mut HumanTinybonesSessionV72,
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
            .validate_decision_v72(&pending.decision)
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

record!(HumanTinybonesRelationV72 {
    object_role: u32,
    incarnation_role: u32,
    owner: PlayerSeatV1,
    controller: PlayerSeatV1,
    zone: crate::state::Zone
});
record!(HumanTinybonesCurrentV72 {
    card: HumanCardRefV1,
    relation: HumanTinybonesRelationV72,
    is_token: bool,
    tapped: bool,
    summoning_sick: bool,
    marked_damage: u32,
    counters: crate::tinybones_public_v72::CountersV72,
    form: crate::optional_trigger_public_v1::FormV1
});
record!(HumanTinybonesDiscardV72 {occurrence:u32,original_card:HumanTinybonesRelationV72,discarder:PlayerSeatV1,outcome_zone:crate::state::Zone,current_original_outcome:Option<HumanCardRefV1>,observers:Vec<HumanCallRootV62>});
record!(HumanTinybonesTriggerV72 {discard:u32,observer:u32,source:HumanCallRootV62,stage:crate::tinybones_public_v72::StageV72,native_position:Option<u32>,stashed:Option<HumanTinybonesRelationV72>});
record!(HumanTinybonesStashV72 {card:HumanCardRefV1,counters:crate::tinybones_public_v72::CountersV72,can_play:[bool;2],any_type_mana:[bool;2],current_grant_sources:[Vec<HumanCallRootV62>;2]});
record!(HumanTinybonesPlayV72 {occurrence:u32,original_card:HumanCardRefV1,permission_source:HumanCallRootV62,actor:PlayerSeatV1,original_turn:u32,stash:i32,land:bool,stage:crate::tinybones_public_v72::PlayStageV72,announced_form:Option<crate::state::CastMethodV4>,paid_form:Option<crate::state::CastMethodV4>,paid_x:Option<u16>,variable_targeting_kicked:Option<bool>});
record!(HumanTinybonesActivationV72 {native_position:u32,source:HumanCallRootV62,generic_mana:u8,colored_mana:Vec<crate::mana::ManaColor>,tap_cost:bool,sorcery_only:bool});
record!(HumanTinybonesFactsV72 {viewer:PlayerSeatV1,active_player:PlayerSeatV1,turn_key:u32,meaning:crate::rules_meaning_tinybones_v72::MeaningV72,authorized_printings:Vec<HumanCardRefV1>,battlefield:[Vec<HumanTinybonesCurrentV72>;2],discards:Vec<HumanTinybonesDiscardV72>,triggers:Vec<HumanTinybonesTriggerV72>,stash:Vec<HumanTinybonesStashV72>,plays:Vec<HumanTinybonesPlayV72>,activations:Vec<HumanTinybonesActivationV72>});
record!(HumanTinybonesActionV72 {triggers:Vec<HumanTinybonesTriggerV72>,plays:Vec<HumanTinybonesPlayV72>});
/// This map exists only for one prompt. Its keys never leave the backend.
#[derive(Default)]
struct Incarnations {
    objects: BTreeMap<u32, u32>,
    incarnations: BTreeMap<(u32, u32), u32>,
}
impl Incarnations {
    fn relation(
        &mut self,
        r: &crate::tinybones_public_v72::IncarnationV72,
    ) -> Result<HumanTinybonesRelationV72, Error> {
        let next = u32::try_from(self.objects.len()).map_err(|_| Error::InvalidVisibleReference)?;
        let object_role = *self.objects.entry(r.object).or_insert(next);
        let next =
            u32::try_from(self.incarnations.len()).map_err(|_| Error::InvalidVisibleReference)?;
        let incarnation_role = *self
            .incarnations
            .entry((r.object, r.generation))
            .or_insert(next);
        Ok(HumanTinybonesRelationV72 {
            object_role,
            incarnation_role,
            owner: r.owner,
            controller: r.controller,
            zone: r.zone,
        })
    }
}
fn tinybones_facts(
    v: &crate::tinybones_public_v72::FactsV72,
    h: &mut Handles,
) -> Result<HumanTinybonesFactsV72, Error> {
    crate::tinybones_public_v72::validate(v).map_err(|_| Error::InvalidVisibleReference)?;
    let mut roles = Incarnations::default();
    let authorized_printings = v
        .authorized_printings
        .iter()
        .map(|r| href(r, h))
        .collect::<Result<_, _>>()?;
    let mut battlefield = [vec![], vec![]];
    for (seat, rows) in v.battlefield.iter().enumerate() {
        for r in rows {
            battlefield[seat].push(HumanTinybonesCurrentV72 {
                card: href(&r.card, h)?,
                relation: roles.relation(&crate::tinybones_public_v72::IncarnationV72 {
                    object: r.card.object,
                    generation: r.card.generation,
                    owner: r.card.owner,
                    controller: r.card.controller,
                    zone: r.card.zone,
                })?,
                is_token: r.is_token,
                tapped: r.tapped,
                summoning_sick: r.summoning_sick,
                marked_damage: r.marked_damage,
                counters: r.counters.clone(),
                form: r.form.clone(),
            });
        }
    }
    let discards = v
        .discards
        .iter()
        .map(|r| {
            Ok(HumanTinybonesDiscardV72 {
                occurrence: r.occurrence,
                original_card: roles.relation(&r.original_card)?,
                discarder: r.discarder,
                outcome_zone: r.outcome_zone,
                current_original_outcome: r
                    .current_original_outcome
                    .as_ref()
                    .map(|r| href(r, h))
                    .transpose()?,
                observers: r
                    .observers
                    .iter()
                    .map(|r| root(r, h))
                    .collect::<Result<_, _>>()?,
            })
        })
        .collect::<Result<_, Error>>()?;
    let triggers = v
        .triggers
        .iter()
        .map(|r| {
            Ok(HumanTinybonesTriggerV72 {
                discard: r.discard,
                observer: r.observer,
                source: root(&r.source, h)?,
                stage: r.stage,
                native_position: r.native_position,
                stashed: r.stashed.as_ref().map(|r| roles.relation(r)).transpose()?,
            })
        })
        .collect::<Result<_, Error>>()?;
    let stash = v
        .stash
        .iter()
        .map(|r| {
            let [a, b] = r.current_grant_sources.each_ref().map(|rows| {
                rows.iter()
                    .map(|r| root(r, h))
                    .collect::<Result<Vec<_>, _>>()
            });
            Ok(HumanTinybonesStashV72 {
                card: href(&r.card, h)?,
                counters: r.counters.clone(),
                can_play: r.can_play,
                any_type_mana: r.any_type_mana,
                current_grant_sources: [a?, b?],
            })
        })
        .collect::<Result<_, Error>>()?;
    let plays = v
        .plays
        .iter()
        .map(|r| {
            Ok(HumanTinybonesPlayV72 {
                occurrence: r.occurrence,
                original_card: href(&r.original_card, h)?,
                permission_source: root(&r.permission_source, h)?,
                actor: r.actor,
                original_turn: r.original_turn,
                stash: r.stash,
                land: r.land,
                stage: r.stage,
                announced_form: r.announced_form,
                paid_form: r.paid_form,
                paid_x: r.paid_x,
                variable_targeting_kicked: r.variable_targeting_kicked,
            })
        })
        .collect::<Result<_, Error>>()?;
    let activations = v
        .activations
        .iter()
        .map(|r| {
            Ok(HumanTinybonesActivationV72 {
                native_position: r.native_position,
                source: root(&r.source, h)?,
                generic_mana: r.generic_mana,
                colored_mana: r.colored_mana.clone(),
                tap_cost: r.tap_cost,
                sorcery_only: r.sorcery_only,
            })
        })
        .collect::<Result<_, Error>>()?;
    Ok(HumanTinybonesFactsV72 {
        viewer: v.viewer,
        active_player: v.active_player,
        turn_key: v.turn_key,
        meaning: v.meaning.clone(),
        authorized_printings,
        battlefield,
        discards,
        triggers,
        stash,
        plays,
        activations,
    })
}
#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
pub(crate) mod tests;
