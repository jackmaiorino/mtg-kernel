//! Fixed-seat Human facade for the sealed Chandra V74 owner.
//! Anonymous historical roles never reconstruct a later hidden printing.
use super::liliana_v73::{
    liliana_facts, liliana_root, HumanLilianaActionV73, HumanLilianaFactsV73, HumanLilianaRootV73,
};
use super::muldrotha_v71::*;
use super::tinybones_v72::{HumanTinybonesCurrentV72, HumanTinybonesRelationV72, Incarnations};
use super::visible::Handles;
use super::*;
use crate::{
    chandra_observation_v74 as observation, graveyard_return_public_v61 as public,
    rl::{ActionSemanticV1, RlContractError},
    rl_session::{
        ChandraDecisionV74, ChandraSessionV74, ChandraSnapshotV74, FastActorDecisionV1,
        FastActorResponseV1, FastActorSessionV1,
    },
};
use std::sync::Arc;
type Error = HumanDecisionErrorV1;
macro_rules! record { ($name:ident { $($field:ident:$ty:ty),* $(,)? })=>{#[derive(Debug,Clone,PartialEq,Eq,Serialize)] pub struct $name { $(pub $field:$ty),* }}; }
record!(HumanVisibleStateChandraV74 {
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
    liliana: HumanLilianaFactsV73,
    chandra: HumanChandraFactsV74
});
record!(HumanDecisionChandraV74 {
    schema: String, human_seat: PlayerSeatV1, prompt_seq: u64,
    state: HumanVisibleStateChandraV74,
    actions: Vec<HumanLegalActionV1>, action_meanings: Vec<HumanChandraCandidateV74>
});
record!(HumanChandraCandidateV74 {
    tinybones: super::tinybones_v72::HumanTinybonesActionV72,
    liliana: HumanLilianaActionV73,
    chandra: HumanChandraActionV74,
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
    obs: &observation::ChandraObservationV74,
    a: &ActionSemanticV1,
    h: &Handles,
) -> Result<Option<String>, Error> {
    use crate::rl::TargetRefV1;
    if let Some(c) = crate::chandra_public_v74::activation_cost(&obs.chandra, a)
        .map_err(|_| Error::InvalidVisibleReference)?
    {
        let delta = [2, 1, -4][usize::from(c.ability_index)];
        let effect=["add three red mana; exile the top three cards and choose one to play this turn with ordinary costs","copy a creature you control with copiable haste and an end-step sacrifice ability","divide eight damage among zero through eight creatures or planeswalkers before payment"][usize::from(c.ability_index)];
        return Ok(Some(format!(
            "Activate {}: {delta:+} loyalty ({} to {}); {effect}",
            h.name(&c.source)?,
            c.before,
            c.after
        )));
    }
    match a {
        ActionSemanticV1::ChooseTarget {
            source,
            target: TargetRefV1::Object { object },
            ..
        }
        | ActionSemanticV1::ChooseEffectTarget {
            source,
            target: TargetRefV1::Object { object },
            ..
        } if source.card_db_id == 493 => {
            if let Some(allocation) = &obs.chandra.pending_allocation {
                return Ok(Some(format!(
                    "Allocate one damage to {} ({} units remaining before payment)",
                    h.name(object)?,
                    allocation.remaining_units
                )));
            }
            if obs.chandra.exiles.iter().any(|e| e.choice_current) {
                return Ok(Some(format!(
                    "Choose {} to play this turn with ordinary costs and timing",
                    h.name(object)?
                )));
            }
            return Ok(Some(format!(
                "Copy {} with haste and an end-step sacrifice ability",
                h.name(object)?
            )));
        }
        ActionSemanticV1::FinishEffectSelection { source, .. }
            if source.card_db_id == 493 && obs.chandra.pending_allocation.is_some() =>
        {
            return Ok(Some("Choose zero damage targets and pay -4 loyalty".into()))
        }
        ActionSemanticV1::ChooseEffectOption {
            source,
            option_index,
            ..
        } if source.card_db_id == 493 && obs.chandra.pending_damage.is_some() => {
            let damage = obs
                .chandra
                .pending_damage
                .as_ref()
                .ok_or(Error::InvalidVisibleReference)?;
            let option = damage
                .options
                .iter()
                .find(|r| r.menu_index == *option_index)
                .ok_or(Error::InvalidVisibleReference)?;
            return Ok(Some(format!(
                "Apply {:?} to {} ({} damage remains)",
                option.kind,
                h.name(&stable(&damage.recipient)?)?,
                damage.remaining_amount
            )));
        }
        _ => {}
    }
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
    super::prepared_v57::choice_label_chandra(obs, a, h)
}
pub(super) fn project_with_handles(
    decision: &ChandraDecisionV74,
    human: PlayerSeatV1,
    prompt_seq: u64,
) -> Result<(HumanDecisionChandraV74, Vec<u32>, Handles), Error> {
    let obs = decision.observation();
    if obs.graveyard_return.viewer != human || obs.common.common.acting_player != human {
        return Err(Error::NotHumanTurn);
    }
    crate::chandra_candidates_v74::validate_artifact(decision.candidates_v74())
        .map_err(|_| Error::InvalidVisibleReference)?;
    let (common, actions, indexes, mut handles) =
        super::ravenous_v51::project_chandra_base_with_handles(
            decision.candidates_v74(),
            decision.actions(),
            human,
            decision.family_projections_v74(),
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
    let chandra = chandra_facts(
        &obs.chandra,
        &tinybones.battlefield,
        &mut handles,
        &mut roles,
    )?;
    let action_meanings = indexes
        .iter()
        .map(|i| {
            let r = decision
                .candidates_v74()
                .candidates
                .get(*i as usize)
                .ok_or(Error::InvalidAction)?;
            Ok(HumanChandraCandidateV74 {
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
                chandra: HumanChandraActionV74 {
                    selected_damage_option: match &r.action {
                        ActionSemanticV1::ChooseEffectOption {
                            source,
                            option_index,
                            ..
                        } if source.card_db_id == 493 && obs.chandra.pending_damage.is_some() => {
                            Some(*option_index)
                        }
                        _ => None,
                    },
                    selected_allocation_target: match &r.action {
                        ActionSemanticV1::ChooseEffectTarget {
                            source,
                            target: crate::rl::TargetRefV1::Object { object },
                            ..
                        }
                        | ActionSemanticV1::ChooseTarget {
                            source,
                            target: crate::rl::TargetRefV1::Object { object },
                            ..
                        } if source.card_db_id == 493
                            && obs.chandra.pending_allocation.is_some() =>
                        {
                            Some(handles.get(object)?.clone())
                        }
                        _ => None,
                    },
                    selected_exile: match &r.action {
                        ActionSemanticV1::ChooseEffectTarget {
                            source,
                            target: crate::rl::TargetRefV1::Object { object },
                            ..
                        } if source.card_db_id == 493 => obs
                            .chandra
                            .exiles
                            .iter()
                            .find(|e| e.choice_current)
                            .and_then(|e| {
                                e.cards.iter().position(|c| {
                                    c.original.object == object.arena_id
                                        && c.original.generation == object.zone_change_count
                                })
                            })
                            .map(|i| u16::try_from(i).map_err(|_| Error::InvalidVisibleReference))
                            .transpose()?,
                        _ => None,
                    },
                    copy_target: match &r.action {
                        ActionSemanticV1::ChooseTarget {
                            source,
                            target: crate::rl::TargetRefV1::Object { object },
                            ..
                        } if source.card_db_id == 493
                            && obs.chandra.pending_allocation.is_none() =>
                        {
                            Some(handles.get(object)?.clone())
                        }
                        _ => None,
                    },

                    activation: r
                        .chandra_activation
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
                    allocation: chandra.pending_allocation.clone(),
                    exiles: chandra
                        .exiles
                        .iter()
                        .filter(|e| e.choice_current)
                        .cloned()
                        .collect(),
                    damage: chandra.pending_damage.clone(),
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
        HumanDecisionChandraV74 {
            schema: "mtg-kernel-human-chandra-decision/v74".into(),
            human_seat: human,
            prompt_seq,
            state: HumanVisibleStateChandraV74 {
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
                chandra,
            },
            actions,
            action_meanings,
        },
        indexes,
        handles,
    ))
}
fn project(
    decision: &ChandraDecisionV74,
    human: PlayerSeatV1,
    prompt_seq: u64,
) -> Result<(HumanDecisionChandraV74, Vec<u32>), Error> {
    project_with_handles(decision, human, prompt_seq)
        .map(|(visible, indexes, _)| (visible, indexes))
}
#[derive(Clone)]
struct AuthorityV74 {
    owner: Arc<()>,
    generation: u64,
}
impl PartialEq for AuthorityV74 {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && Arc::ptr_eq(&self.owner, &other.owner)
    }
}
impl Eq for AuthorityV74 {}

pub struct HumanChandraSessionV74 {
    inner: ChandraSessionV74,
    authority: AuthorityV74,
    human_seat: PlayerSeatV1,
}
#[derive(Clone)]
pub struct HumanChandraSnapshotV74 {
    inner: ChandraSnapshotV74,
    owner: Arc<()>,
}
impl HumanChandraSessionV74 {
    pub fn from_session(
        inner: FastActorSessionV1,
        human_seat: PlayerSeatV1,
    ) -> Result<Self, RlContractError> {
        Ok(Self {
            inner: ChandraSessionV74::from_session(inner)?,
            human_seat,
            authority: AuthorityV74 {
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
        let inner = ChandraSessionV74::reset_explicit_mainboards_v74(
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
            authority: AuthorityV74 {
                owner: Arc::new(()),
                generation: 0,
            },
        })
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.response()
    }
    pub fn snapshot_v74(&self) -> HumanChandraSnapshotV74 {
        HumanChandraSnapshotV74 {
            inner: self.inner.snapshot_v74(),
            owner: self.authority.owner.clone(),
        }
    }
    pub fn restore_v74(&mut self, snapshot: &HumanChandraSnapshotV74) -> Result<(), Error> {
        if !Arc::ptr_eq(&self.authority.owner, &snapshot.owner) {
            return Err(Error::StaleDecision);
        }
        let generation = self
            .authority
            .generation
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        self.inner
            .restore_v74(&snapshot.inner)
            .map_err(|_| Error::StaleDecision)?;
        self.authority.generation = generation;
        Ok(())
    }
    fn decision(&self, expected: FastActorDecisionV1) -> Result<ChandraDecisionV74, Error> {
        if expected.acting_player != self.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let decision = self
            .inner
            .decision_v74()
            .map_err(|_| Error::StaleDecision)?;
        if decision.expected() != expected {
            return Err(Error::StaleDecision);
        }
        self.inner
            .validate_decision_v74(&decision)
            .map_err(|_| Error::StaleDecision)?;
        Ok(decision)
    }
    pub fn public_observation(
        &self,
        expected: FastActorDecisionV1,
    ) -> Result<observation::ChandraObservationV74, Error> {
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
            &observation::ChandraObservationV74,
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
            .decision_v74()
            .map_err(|_| RlContractError("stale Call opponent decision".into()))?;
        if decision.expected() != expected {
            return Err(RlContractError("stale Call opponent decision".into()));
        }
        self.inner.validate_decision_v74(&decision)?;
        let index = policy(decision.observation(), decision.actions())?;
        self.inner.step_v74(&decision, index)
    }
}

struct BoundChandraV74 {
    authority: AuthorityV74,
    decision: ChandraDecisionV74,
    visible: HumanDecisionChandraV74,
    indexes: Vec<u32>,
}
pub struct HumanChandraProjectorV74 {
    human_seat: PlayerSeatV1,
    next_prompt_seq: u64,
    pending: Option<BoundChandraV74>,
    accepted: Option<(HumanActionReceiptV1, AuthorityV74)>,
}
impl HumanChandraProjectorV74 {
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
        session: &HumanChandraSessionV74,
        expected: FastActorDecisionV1,
    ) -> Result<HumanDecisionChandraV74, Error> {
        if expected.acting_player != self.human_seat || session.human_seat != self.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let decision = session.decision(expected)?;
        if let Some(pending) = &self.pending {
            if pending.authority == session.authority
                && pending.decision.expected() == expected
                && session
                    .inner
                    .validate_decision_v74(&pending.decision)
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
        self.pending = Some(BoundChandraV74 {
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
        session: &mut HumanChandraSessionV74,
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
            .validate_decision_v74(&pending.decision)
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

pub type HumanChandraRootV74 = HumanLilianaRootV73;
record!(HumanChandraTargetV74 {original:HumanTinybonesRelationV72,current_original:Option<HumanCardRefV1>,allocated_damage:Option<u8>,legal_now:bool});
record!(HumanChandraAllocationV74 {source:HumanChandraRootV74,loyalty_before:u32,targets:Vec<HumanChandraTargetV74>,remaining_units:u8,finished:bool,can_finish_zero:bool,candidates:Vec<HumanCardRefV1>});
record!(HumanChandraDamageSourceV74 {
    source: HumanChandraRootV74,
    historical: bool,
    color_mask: u8,
    lifelink: bool,
    deathtouch: bool,
    werewolf: bool
});
record!(HumanChandraActivationV74 {occurrence:u32,source:HumanChandraRootV74,ability_index:u8,loyalty_before:u32,loyalty_after:u32,targets:Vec<HumanChandraTargetV74>,stage:crate::chandra_public_v74::ActivationStageV74,damage_source:Option<HumanChandraDamageSourceV74>});
record!(HumanChandraExiledCardV74 {original:HumanCardRefV1,current_original:Option<HumanCardRefV1>});
record!(HumanChandraExileV74 {activation:u32,cards:Vec<HumanChandraExiledCardV74>,choice_current:bool,selected_prefix:Vec<u16>,chosen:Option<u16>,grant_for_land:Option<bool>,permission_present:bool,expired:bool});
record!(HumanChandraPreventionV74 {menu_index:u16,kind:crate::chandra_public_v74::PreventionKindV74,source:Option<HumanCardRefV1>,shield_remaining:Option<i32>});
record!(HumanChandraDamageChoiceV74 {activation:u32,chooser:PlayerSeatV1,recipient:HumanCardRefV1,original_amount:i32,remaining_amount:i32,simultaneous_waiting:usize,options:Vec<HumanChandraPreventionV74>});
record!(HumanChandraCopiedTokenV74 {occurrence:u32,source:HumanChandraRootV74,sacrifice_instances:u32,current_haste:Option<bool>});
record!(HumanChandraEndStepObserverV74 {
    token: u32,
    source: HumanChandraRootV74,
    instances: u32
});
record!(HumanChandraEndStepV74 {occurrence:u32,observers:Vec<HumanChandraEndStepObserverV74>,collected:bool,published_count:u32,finished_count:u32});
record!(HumanChandraFactsV74 {viewer:PlayerSeatV1,active_player:PlayerSeatV1,turn_key:u32,meaning:crate::rules_meaning_chandra_v74::MeaningV74,authorized_printings:Vec<HumanCardRefV1>,battlefield:[Vec<HumanTinybonesCurrentV72>;2],pending_allocation:Option<HumanChandraAllocationV74>,activations:Vec<HumanChandraActivationV74>,exiles:Vec<HumanChandraExileV74>,pending_damage:Option<HumanChandraDamageChoiceV74>,copied_tokens:Vec<HumanChandraCopiedTokenV74>,end_steps:Vec<HumanChandraEndStepV74>});
record!(HumanChandraActionV74 {selected_damage_option:Option<u16>,selected_allocation_target:Option<HumanCardRefV1>,selected_exile:Option<u16>,copy_target:Option<HumanCardRefV1>,activation:Option<HumanLoyaltyCostV71>,allocation:Option<HumanChandraAllocationV74>,exiles:Vec<HumanChandraExileV74>,damage:Option<HumanChandraDamageChoiceV74>});
fn chandra_target(
    v: &crate::chandra_public_v74::TargetV74,
    h: &mut Handles,
    roles: &mut Incarnations,
) -> Result<HumanChandraTargetV74, Error> {
    Ok(HumanChandraTargetV74 {
        original: roles.relation(&v.original)?,
        current_original: v
            .current_original
            .as_ref()
            .map(|r| href(r, h))
            .transpose()?,
        allocated_damage: v.allocated_damage,
        legal_now: v.legal_now,
    })
}
fn chandra_allocation(
    v: &crate::chandra_public_v74::AllocationV74,
    h: &mut Handles,
    roles: &mut Incarnations,
) -> Result<HumanChandraAllocationV74, Error> {
    Ok(HumanChandraAllocationV74 {
        source: liliana_root(&v.source, h, roles)?,
        loyalty_before: v.loyalty_before,
        targets: v
            .targets
            .iter()
            .map(|r| chandra_target(r, h, roles))
            .collect::<Result<_, _>>()?,
        remaining_units: v.remaining_units,
        finished: v.finished,
        can_finish_zero: v.can_finish_zero,
        candidates: v
            .candidates
            .iter()
            .map(|r| href(r, h))
            .collect::<Result<_, _>>()?,
    })
}
fn chandra_exile(
    v: &crate::chandra_public_v74::ExileV74,
    h: &mut Handles,
) -> Result<HumanChandraExileV74, Error> {
    Ok(HumanChandraExileV74 {
        activation: v.activation,
        cards: v
            .cards
            .iter()
            .map(|r| {
                Ok(HumanChandraExiledCardV74 {
                    original: href(&r.original, h)?,
                    current_original: r
                        .current_original
                        .as_ref()
                        .map(|r| href(r, h))
                        .transpose()?,
                })
            })
            .collect::<Result<_, Error>>()?,
        choice_current: v.choice_current,
        selected_prefix: v.selected_prefix.clone(),
        chosen: v.chosen,
        grant_for_land: v.grant_for_land,
        permission_present: v.permission_present,
        expired: v.expired,
    })
}
fn chandra_damage(
    v: &crate::chandra_public_v74::DamageChoiceV74,
    h: &mut Handles,
) -> Result<HumanChandraDamageChoiceV74, Error> {
    Ok(HumanChandraDamageChoiceV74 {
        activation: v.activation,
        chooser: v.chooser,
        recipient: href(&v.recipient, h)?,
        original_amount: v.original_amount,
        remaining_amount: v.remaining_amount,
        simultaneous_waiting: v.simultaneous_waiting,
        options: v
            .options
            .iter()
            .map(|r| {
                Ok(HumanChandraPreventionV74 {
                    menu_index: r.menu_index,
                    kind: r.kind,
                    source: r.source.as_ref().map(|r| href(r, h)).transpose()?,
                    shield_remaining: r.shield_remaining,
                })
            })
            .collect::<Result<_, Error>>()?,
    })
}
pub(super) fn chandra_facts(
    v: &crate::chandra_public_v74::FactsV74,
    battlefield: &[Vec<HumanTinybonesCurrentV72>; 2],
    h: &mut Handles,
    roles: &mut Incarnations,
) -> Result<HumanChandraFactsV74, Error> {
    crate::chandra_public_v74::validate(v).map_err(|_| Error::InvalidVisibleReference)?;
    let authorized_printings = v
        .authorized_printings
        .iter()
        .map(|r| href(r, h))
        .collect::<Result<_, _>>()?;
    let pending_allocation = v
        .pending_allocation
        .as_ref()
        .map(|r| chandra_allocation(r, h, roles))
        .transpose()?;
    let activations = v
        .activations
        .iter()
        .map(|r| {
            Ok(HumanChandraActivationV74 {
                occurrence: r.occurrence,
                source: liliana_root(&r.source, h, roles)?,
                ability_index: r.ability_index,
                loyalty_before: r.loyalty_before,
                loyalty_after: r.loyalty_after,
                targets: r
                    .targets
                    .iter()
                    .map(|r| chandra_target(r, h, roles))
                    .collect::<Result<_, _>>()?,
                stage: r.stage,
                damage_source: r
                    .damage_source
                    .as_ref()
                    .map(|d| {
                        Ok::<_, Error>(HumanChandraDamageSourceV74 {
                            source: liliana_root(&d.source, h, roles)?,
                            historical: d.historical,
                            color_mask: d.color_mask,
                            lifelink: d.lifelink,
                            deathtouch: d.deathtouch,
                            werewolf: d.werewolf,
                        })
                    })
                    .transpose()?,
            })
        })
        .collect::<Result<_, Error>>()?;
    let exiles = v
        .exiles
        .iter()
        .map(|r| chandra_exile(r, h))
        .collect::<Result<_, _>>()?;
    let pending_damage = v
        .pending_damage
        .as_ref()
        .map(|r| chandra_damage(r, h))
        .transpose()?;
    let copied_tokens = v
        .copied_tokens
        .iter()
        .map(|r| {
            Ok(HumanChandraCopiedTokenV74 {
                occurrence: r.occurrence,
                source: liliana_root(&r.source, h, roles)?,
                sacrifice_instances: r.sacrifice_instances,
                current_haste: r.current_haste,
            })
        })
        .collect::<Result<_, Error>>()?;
    let end_steps = v
        .end_steps
        .iter()
        .map(|r| {
            Ok(HumanChandraEndStepV74 {
                occurrence: r.occurrence,
                observers: r
                    .observers
                    .iter()
                    .map(|r| {
                        Ok(HumanChandraEndStepObserverV74 {
                            token: r.token,
                            source: liliana_root(&r.source, h, roles)?,
                            instances: r.instances,
                        })
                    })
                    .collect::<Result<_, Error>>()?,
                collected: r.collected,
                published_count: r.published_count,
                finished_count: r.finished_count,
            })
        })
        .collect::<Result<_, Error>>()?;
    Ok(HumanChandraFactsV74 {
        viewer: v.viewer,
        active_player: v.active_player,
        turn_key: v.turn_key,
        meaning: v.meaning.clone(),
        authorized_printings,
        battlefield: battlefield.clone(),
        pending_allocation,
        activations,
        exiles,
        pending_damage,
        copied_tokens,
        end_steps,
    })
}
#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
pub(crate) mod tests;
