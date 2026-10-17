//! Fixed-seat Human facade for the sealed Liliana V73 owner.
//! Anonymous historical roles never reconstruct a later hidden printing.
use super::muldrotha_v71::*;
use super::tinybones_v72::{HumanTinybonesCurrentV72, HumanTinybonesRelationV72, Incarnations};
use super::visible::Handles;
use super::*;
use crate::{
    graveyard_return_public_v61 as public, liliana_observation_v73 as observation,
    rl::{ActionSemanticV1, RlContractError},
    rl_session::{
        FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1, LilianaDecisionV73,
        LilianaSessionV73, LilianaSnapshotV73,
    },
};
use std::sync::Arc;
type Error = HumanDecisionErrorV1;
macro_rules! record { ($name:ident { $($field:ident:$ty:ty),* $(,)? })=>{#[derive(Debug,Clone,PartialEq,Eq,Serialize)] pub struct $name { $(pub $field:$ty),* }}; }
record!(HumanVisibleStateLilianaV73 {
    common: super::elenda_v52::HumanVisibleStateElendaV52,
    prepared: super::prepared_v57::HumanPreparedFactsV57,
    next_five: super::next_five_v59::HumanNextFiveFactsV59,
    graveyard_return: HumanCallFactsV62,
    call: HumanNativeCallFactsV62,
    end_turn: HumanEndTurnFactsV68,
    genesis_wave: HumanGenesisWaveCompatibilityV71,
    vivien: HumanLoyaltyFactsV71,
    muldrotha: HumanMuldrothaFactsV71,
    tinybones: super::tinybones_v72::HumanTinybonesFactsV72,
    liliana: HumanLilianaFactsV73
});
record!(HumanDecisionLilianaV73 {
    schema: String, human_seat: PlayerSeatV1, prompt_seq: u64,
    state: HumanVisibleStateLilianaV73,
    actions: Vec<HumanLegalActionV1>, action_meanings: Vec<HumanLilianaCandidateV73>
});
record!(HumanLilianaCandidateV73 {
    tinybones: super::tinybones_v72::HumanTinybonesActionV72,
    liliana: HumanLilianaActionV73,
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
    obs: &observation::LilianaObservationV73,
    a: &ActionSemanticV1,
    h: &Handles,
) -> Result<Option<String>, Error> {
    use crate::rl::TargetRefV1;
    if let Some(c) = crate::liliana_public_v73::activation_cost(&obs.liliana, a)
        .map_err(|_| Error::InvalidVisibleReference)?
    {
        let delta = [1, -4, -9][usize::from(c.ability_index)];
        let effect = [
            "create a black 2/2 Zombie",
            "each player sacrifices two creatures",
            "each opponent keeps one permanent of each permanent type and sacrifices the rest",
        ][usize::from(c.ability_index)];
        return Ok(Some(format!(
            "Activate {}: {delta:+} loyalty ({} to {}); {effect}",
            h.name(&c.source)?,
            c.before,
            c.after
        )));
    }
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
        ActionSemanticV1::ChooseEffectTarget {
            source,
            target: TargetRefV1::Object { object },
            ..
        } if source.card_db_id == 491 => {
            let choices = crate::liliana_public_v73::choice_contexts(&obs.liliana);
            let [choice] = choices.as_slice() else {
                return Err(Error::InvalidVisibleReference);
            };
            if !choice
                .candidates
                .iter()
                .any(|r| r.object == object.arena_id && r.generation == object.zone_change_count)
            {
                return Err(Error::InvalidVisibleReference);
            }
            return Ok(Some(match choice.kind {
                crate::liliana_public_v73::SelectionKindV73::SacrificeCreatures => {
                    format!("Choose {} to sacrifice", h.name(object)?)
                }
                crate::liliana_public_v73::SelectionKindV73::KeepPermanent(t) => {
                    format!("Keep {} for {t:?}", h.name(object)?)
                }
                crate::liliana_public_v73::SelectionKindV73::OrderGraveyard => format!(
                    "Put {} next in its owner's graveyard order",
                    h.name(object)?
                ),
            }));
        }
        ActionSemanticV1::FinishEffectSelection { source, .. } if source.card_db_id == 491 => {
            return Ok(Some("Finish Liliana's current selection".into()))
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
    super::prepared_v57::choice_label_liliana(obs, a, h)
}
pub(super) fn project_with_handles(
    decision: &LilianaDecisionV73,
    human: PlayerSeatV1,
    prompt_seq: u64,
) -> Result<(HumanDecisionLilianaV73, Vec<u32>, Handles), Error> {
    let obs = decision.observation();
    if obs.graveyard_return.viewer != human || obs.common.common.acting_player != human {
        return Err(Error::NotHumanTurn);
    }
    crate::liliana_candidates_v73::validate_artifact(decision.candidates_v73())
        .map_err(|_| Error::InvalidVisibleReference)?;
    let (common, actions, indexes, mut handles) =
        super::ravenous_v51::project_liliana_base_with_handles(
            decision.candidates_v73(),
            decision.actions(),
            human,
            decision.family_projections_v73(),
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
    let (tinybones, mut roles) =
        super::tinybones_v72::tinybones_facts_with_roles(&obs.tinybones, &mut handles)?;
    let liliana = liliana_facts(
        &obs.liliana,
        &tinybones.battlefield,
        &mut handles,
        &mut roles,
    )?;
    let action_meanings = indexes
        .iter()
        .map(|i| {
            let r = decision
                .candidates_v73()
                .candidates
                .get(*i as usize)
                .ok_or(Error::InvalidAction)?;
            Ok(HumanLilianaCandidateV73 {
                tinybones: super::tinybones_v72::HumanTinybonesActionV72 {
                    triggers: tinybones.triggers.clone(),
                    plays: tinybones.plays.clone(),
                },
                liliana: HumanLilianaActionV73 {
                    choices: liliana
                        .resolutions
                        .iter()
                        .flat_map(|r| r.selections.iter().filter(|s| s.current).cloned())
                        .collect(),
                    activation: r
                        .liliana_activation
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
        HumanDecisionLilianaV73 {
            schema: "mtg-kernel-human-liliana-decision/v73".into(),
            human_seat: human,
            prompt_seq,
            state: HumanVisibleStateLilianaV73 {
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
                liliana,
            },
            actions,
            action_meanings,
        },
        indexes,
        handles,
    ))
}
fn project(
    decision: &LilianaDecisionV73,
    human: PlayerSeatV1,
    prompt_seq: u64,
) -> Result<(HumanDecisionLilianaV73, Vec<u32>), Error> {
    project_with_handles(decision, human, prompt_seq)
        .map(|(visible, indexes, _)| (visible, indexes))
}
#[derive(Clone)]
struct AuthorityV73 {
    owner: Arc<()>,
    generation: u64,
}
impl PartialEq for AuthorityV73 {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && Arc::ptr_eq(&self.owner, &other.owner)
    }
}
impl Eq for AuthorityV73 {}

pub struct HumanLilianaSessionV73 {
    inner: LilianaSessionV73,
    authority: AuthorityV73,
    human_seat: PlayerSeatV1,
}
#[derive(Clone)]
pub struct HumanLilianaSnapshotV73 {
    inner: LilianaSnapshotV73,
    owner: Arc<()>,
}
impl HumanLilianaSessionV73 {
    pub fn from_session(
        inner: FastActorSessionV1,
        human_seat: PlayerSeatV1,
    ) -> Result<Self, RlContractError> {
        Ok(Self {
            inner: LilianaSessionV73::from_session(inner)?,
            human_seat,
            authority: AuthorityV73 {
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
        let inner = LilianaSessionV73::reset_explicit_mainboards_v73(
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
            authority: AuthorityV73 {
                owner: Arc::new(()),
                generation: 0,
            },
        })
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.response()
    }
    pub fn snapshot_v73(&self) -> HumanLilianaSnapshotV73 {
        HumanLilianaSnapshotV73 {
            inner: self.inner.snapshot_v73(),
            owner: self.authority.owner.clone(),
        }
    }
    pub fn restore_v73(&mut self, snapshot: &HumanLilianaSnapshotV73) -> Result<(), Error> {
        if !Arc::ptr_eq(&self.authority.owner, &snapshot.owner) {
            return Err(Error::StaleDecision);
        }
        let generation = self
            .authority
            .generation
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        self.inner
            .restore_v73(&snapshot.inner)
            .map_err(|_| Error::StaleDecision)?;
        self.authority.generation = generation;
        Ok(())
    }
    fn decision(&self, expected: FastActorDecisionV1) -> Result<LilianaDecisionV73, Error> {
        if expected.acting_player != self.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let decision = self
            .inner
            .decision_v73()
            .map_err(|_| Error::StaleDecision)?;
        if decision.expected() != expected {
            return Err(Error::StaleDecision);
        }
        self.inner
            .validate_decision_v73(&decision)
            .map_err(|_| Error::StaleDecision)?;
        Ok(decision)
    }
    pub fn public_observation(
        &self,
        expected: FastActorDecisionV1,
    ) -> Result<observation::LilianaObservationV73, Error> {
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
            &observation::LilianaObservationV73,
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
            .decision_v73()
            .map_err(|_| RlContractError("stale Call opponent decision".into()))?;
        if decision.expected() != expected {
            return Err(RlContractError("stale Call opponent decision".into()));
        }
        self.inner.validate_decision_v73(&decision)?;
        let index = policy(decision.observation(), decision.actions())?;
        self.inner.step_v73(&decision, index)
    }
}

struct BoundLilianaV73 {
    authority: AuthorityV73,
    decision: LilianaDecisionV73,
    visible: HumanDecisionLilianaV73,
    indexes: Vec<u32>,
}
pub struct HumanLilianaProjectorV73 {
    human_seat: PlayerSeatV1,
    next_prompt_seq: u64,
    pending: Option<BoundLilianaV73>,
    accepted: Option<(HumanActionReceiptV1, AuthorityV73)>,
}
impl HumanLilianaProjectorV73 {
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
        session: &HumanLilianaSessionV73,
        expected: FastActorDecisionV1,
    ) -> Result<HumanDecisionLilianaV73, Error> {
        if expected.acting_player != self.human_seat || session.human_seat != self.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let decision = session.decision(expected)?;
        if let Some(pending) = &self.pending {
            if pending.authority == session.authority
                && pending.decision.expected() == expected
                && session
                    .inner
                    .validate_decision_v73(&pending.decision)
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
        self.pending = Some(BoundLilianaV73 {
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
        session: &mut HumanLilianaSessionV73,
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
            .validate_decision_v73(&pending.decision)
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

record!(HumanLilianaRootV73 {original_source:HumanCardRefV1,current_original_source:Option<HumanCardRefV1>,captured_controller:PlayerSeatV1,original_source_relation:HumanTinybonesRelationV72});
fn liliana_root(
    v: &crate::optional_trigger_public_v1::RootV1,
    h: &mut Handles,
    roles: &mut Incarnations,
) -> Result<HumanLilianaRootV73, Error> {
    let known = root(v, h)?;
    let r = &v.original_source;
    Ok(HumanLilianaRootV73 {
        original_source: known.original_source,
        current_original_source: known.current_original_source,
        captured_controller: known.captured_controller,
        original_source_relation: roles.relation(&crate::liliana_public_v73::IncarnationV73 {
            object: r.object,
            generation: r.generation,
            owner: r.owner,
            controller: r.controller,
            zone: r.zone,
        })?,
    })
}
record!(HumanLilianaSceneObjectV73 {original:HumanTinybonesRelationV72,permanent_types:Vec<crate::card_def::CardType>,current_original:Option<HumanCardRefV1>});
record!(HumanLilianaSelectionV73 {occurrence:u32,phase:u8,actor:PlayerSeatV1,kind:crate::liliana_public_v73::SelectionKindV73,candidates:Vec<HumanTinybonesRelationV72>,exact_count:u16,ordered:bool,selected_prefix:Vec<HumanTinybonesRelationV72>,current:bool});
record!(HumanLilianaKeeperV73 {
    permanent_type: crate::card_def::CardType,
    permanent: HumanTinybonesRelationV72
});
record!(HumanLilianaResolutionV73 {activation:u32,active_player:PlayerSeatV1,controller:PlayerSeatV1,ultimate:bool,rosters:[Vec<HumanLilianaSceneObjectV73>;2],selections:Vec<HumanLilianaSelectionV73>,chosen_sacrifices:[Vec<HumanTinybonesRelationV72>;2],keepers:Vec<HumanLilianaKeeperV73>,graveyard_orders:[Vec<HumanTinybonesRelationV72>;2],completed_sacrifices:Vec<HumanTinybonesRelationV72>,finished:bool});
record!(HumanLilianaActivationV73 {occurrence:u32,source:HumanLilianaRootV73,turn:u32,ability_index:u8,loyalty_before:u32,loyalty_after:u32,stage:crate::liliana_public_v73::ActivationStageV73,created_zombies:Vec<HumanTinybonesRelationV72>});
record!(HumanLilianaDeathV73 {
    occurrence: u32,
    observer: HumanLilianaRootV73,
    creature: HumanTinybonesRelationV72,
    collected: bool
});
record!(HumanLilianaDrawV73 {
    source: HumanLilianaRootV73,
    stage: crate::liliana_public_v73::DrawStageV73,
    native_position: u32
});
record!(HumanLilianaFactsV73 {viewer:PlayerSeatV1,active_player:PlayerSeatV1,turn_key:u32,meaning:crate::rules_meaning_liliana_v73::MeaningV73,authorized_printings:Vec<HumanCardRefV1>,battlefield:[Vec<HumanTinybonesCurrentV72>;2],activations:Vec<HumanLilianaActivationV73>,resolutions:Vec<HumanLilianaResolutionV73>,deaths:Vec<HumanLilianaDeathV73>,current_draws:Vec<HumanLilianaDrawV73>});
record!(HumanLilianaActionV73 {choices:Vec<HumanLilianaSelectionV73>,activation:Option<HumanLoyaltyCostV71>});
fn relations(
    rows: &[crate::liliana_public_v73::IncarnationV73],
    roles: &mut Incarnations,
) -> Result<Vec<HumanTinybonesRelationV72>, Error> {
    rows.iter().map(|r| roles.relation(r)).collect()
}
fn liliana_selection(
    r: &crate::liliana_public_v73::SelectionV73,
    roles: &mut Incarnations,
) -> Result<HumanLilianaSelectionV73, Error> {
    Ok(HumanLilianaSelectionV73 {
        occurrence: r.occurrence,
        phase: r.phase,
        actor: r.actor,
        kind: r.kind,
        candidates: relations(&r.candidates, roles)?,
        exact_count: r.exact_count,
        ordered: r.ordered,
        selected_prefix: relations(&r.selected_prefix, roles)?,
        current: r.current,
    })
}
fn liliana_facts(
    v: &crate::liliana_public_v73::FactsV73,
    battlefield: &[Vec<HumanTinybonesCurrentV72>; 2],
    h: &mut Handles,
    roles: &mut Incarnations,
) -> Result<HumanLilianaFactsV73, Error> {
    crate::liliana_public_v73::validate(v).map_err(|_| Error::InvalidVisibleReference)?;
    let authorized_printings = v
        .authorized_printings
        .iter()
        .map(|r| href(r, h))
        .collect::<Result<_, _>>()?;
    let activations = v
        .activations
        .iter()
        .map(|r| {
            Ok(HumanLilianaActivationV73 {
                occurrence: r.occurrence,
                source: liliana_root(&r.source, h, roles)?,
                turn: r.turn,
                ability_index: r.ability_index,
                loyalty_before: r.loyalty_before,
                loyalty_after: r.loyalty_after,
                stage: r.stage,
                created_zombies: relations(&r.created_zombies, roles)?,
            })
        })
        .collect::<Result<_, Error>>()?;
    let resolutions = v
        .resolutions
        .iter()
        .map(|r| {
            let [a, b] = r.rosters.each_ref().map(|rows| {
                rows.iter()
                    .map(|s| {
                        Ok(HumanLilianaSceneObjectV73 {
                            original: roles.relation(&s.original)?,
                            permanent_types: s.permanent_types.clone(),
                            current_original: s
                                .current_original
                                .as_ref()
                                .map(|v| href(v, h))
                                .transpose()?,
                        })
                    })
                    .collect::<Result<Vec<_>, Error>>()
            });
            let rosters = [a?, b?];
            let selections = r
                .selections
                .iter()
                .map(|s| liliana_selection(s, roles))
                .collect::<Result<_, _>>()?;
            let [a, b] = r.chosen_sacrifices.each_ref().map(|s| relations(s, roles));
            let chosen_sacrifices = [a?, b?];
            let keepers = r
                .keepers
                .iter()
                .map(|k| {
                    Ok(HumanLilianaKeeperV73 {
                        permanent_type: k.permanent_type,
                        permanent: roles.relation(&k.permanent)?,
                    })
                })
                .collect::<Result<_, Error>>()?;
            let [a, b] = r.graveyard_orders.each_ref().map(|s| relations(s, roles));
            let graveyard_orders = [a?, b?];
            Ok(HumanLilianaResolutionV73 {
                activation: r.activation,
                active_player: r.active_player,
                controller: r.controller,
                ultimate: r.ultimate,
                rosters,
                selections,
                chosen_sacrifices,
                keepers,
                graveyard_orders,
                completed_sacrifices: relations(&r.completed_sacrifices, roles)?,
                finished: r.finished,
            })
        })
        .collect::<Result<_, Error>>()?;
    let deaths = v
        .deaths
        .iter()
        .map(|r| {
            Ok(HumanLilianaDeathV73 {
                occurrence: r.occurrence,
                observer: liliana_root(&r.observer, h, roles)?,
                creature: roles.relation(&r.creature)?,
                collected: r.collected,
            })
        })
        .collect::<Result<_, Error>>()?;
    let current_draws = v
        .current_draws
        .iter()
        .map(|r| {
            Ok(HumanLilianaDrawV73 {
                source: liliana_root(&r.source, h, roles)?,
                stage: r.stage,
                native_position: r.native_position,
            })
        })
        .collect::<Result<_, Error>>()?;
    Ok(HumanLilianaFactsV73 {
        viewer: v.viewer,
        active_player: v.active_player,
        turn_key: v.turn_key,
        meaning: v.meaning.clone(),
        authorized_printings,
        battlefield: battlefield.clone(),
        activations,
        resolutions,
        deaths,
        current_draws,
    })
}
#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
pub(crate) mod tests;
