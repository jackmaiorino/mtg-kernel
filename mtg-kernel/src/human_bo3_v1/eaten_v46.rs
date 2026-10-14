//! Sealed Hunter V45 mixed successor. Complete executable authority stays backend-only.
use super::visible::{self, Handles};
use super::*;
use crate::eaten_observation_v46::EatenObservationV46;
use crate::rl::ActionSemanticV1;
use crate::rl_session::eaten_v46::EatenDecisionBindingV46;
use crate::rl_session::{FastActorResponseV1, FastActorSessionSnapshotV1};
use std::collections::BTreeMap;
type Error = HumanDecisionErrorV1;

// Backend-only authority survives owning restore. Snapshots never capture it.
#[derive(Clone)]
struct AuthorityV46 {
    owner: std::sync::Arc<()>,
    generation: u64,
}
impl PartialEq for AuthorityV46 {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && std::sync::Arc::ptr_eq(&self.owner, &other.owner)
    }
}
impl Eq for AuthorityV46 {}
pub struct HumanEatenSessionV46 {
    inner: FastActorSessionV1,
    authority: AuthorityV46,
    human_seat: PlayerSeatV1,
}
#[derive(Clone)]
pub struct HumanEatenSnapshotV46 {
    snapshot: FastActorSessionSnapshotV1,
    owner: std::sync::Arc<()>,
}
impl HumanEatenSessionV46 {
    pub fn from_session(
        session: FastActorSessionV1,
        human_seat: PlayerSeatV1,
    ) -> Result<Self, crate::rl::RlContractError> {
        Ok(Self {
            inner: session.into_eaten_v46()?,
            human_seat,
            authority: AuthorityV46 {
                owner: std::sync::Arc::new(()),
                generation: 0,
            },
        })
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.current_response()
    }
    pub fn snapshot_v46(&self) -> HumanEatenSnapshotV46 {
        HumanEatenSnapshotV46 {
            snapshot: self.inner.snapshot_v1(),
            owner: self.authority.owner.clone(),
        }
    }
    pub fn restore_v46(&mut self, snapshot: &HumanEatenSnapshotV46) -> Result<(), Error> {
        if !std::sync::Arc::ptr_eq(&self.authority.owner, &snapshot.owner) {
            return Err(Error::StaleDecision);
        }
        let next = self
            .authority
            .generation
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        self.inner
            .eaten_restore_v46(&snapshot.snapshot)
            .map_err(|_| Error::StaleDecision)?;
        self.authority.generation = next;
        Ok(())
    }
    /// Public typed transport, not the safe Human DTO and not scorer admission.
    pub fn public_observation(
        &self,
        expected: FastActorDecisionV1,
    ) -> Result<crate::eaten_observation_v46::EatenObservationV46, crate::rl::RlContractError> {
        if expected.acting_player != self.human_seat {
            return Err(crate::rl::RlContractError(
                "Hunter Human view is fixed to its selected seat".into(),
            ));
        }
        self.inner
            .eaten_current_decision_input_v46(expected, self.human_seat)
            .map(|(observation, _, _)| observation)
            .map_err(|_| crate::rl::RlContractError("stale Hunter public decision".into()))
    }
    /// Host-only policy callback. Opponent private rosters never enter a Human DTO.
    pub fn step_opponent_with<F>(
        &mut self,
        expected: FastActorDecisionV1,
        mut policy: F,
    ) -> Result<FastActorResponseV1, crate::rl::RlContractError>
    where
        F: FnMut(
            &EatenObservationV46,
            &[ActionSemanticV1],
        ) -> Result<u32, crate::rl::RlContractError>,
    {
        if expected.acting_player == self.human_seat {
            return Err(crate::rl::RlContractError(
                "opponent callback cannot act for the Human seat".into(),
            ));
        }
        let (observation, actions, binding) = self
            .inner
            .eaten_current_decision_input_v46(expected, expected.acting_player)
            .map_err(|_| crate::rl::RlContractError("stale opponent input".into()))?;
        let index = policy(&observation, &actions)?;
        if index as usize >= actions.len() {
            return Err(crate::rl::RlContractError(
                "opponent action out of range".into(),
            ));
        }
        self.inner
            .eaten_validate_bound_decision_v46(expected, expected.acting_player, &binding)
            .map_err(|_| crate::rl::RlContractError("opponent input changed".into()))?;
        self.inner
            .step(expected.episode_id, expected.step, index)
            .map_err(|error| crate::rl::RlContractError(format!("opponent execution:{error:?}")))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanDecisionEatenV46 {
    pub schema: String,
    pub human_seat: PlayerSeatV1,
    pub prompt_seq: u64,
    pub state: HumanVisibleStateV1<HumanEatenExtensionsV46>,
    pub actions: Vec<HumanLegalActionV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanEatenExtensionsV46 {
    pub predecessor: HumanExtensionsV9,
    pub grants: Vec<HumanDivineGrantV44>,
    pub families: Vec<super::akroma_families_v40::HumanFamilyV40>,
    pub color_departures: Vec<HumanColorDepartureV44>,
    pub ajani: HumanAjaniFactsV42,
    pub faebloom: HumanFaebloomFactsV43,
    pub divine: HumanDivineFactsV44,
    pub hunter: HumanHunterFactsV45,
    pub eaten: HumanEatenFactsV46,
    pub trigger_order: Option<crate::policy_surface_v5::TriggerOrderContextV45>,
    pub creature_target_traits: Vec<HumanCreatureTargetTraitsV43>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanHunterOccurrenceV45 {
    pub occurrence: usize,
    pub original_source: HumanCardRefV1,
    pub current_original_source: Option<HumanCardRefV1>,
    pub original_controller: PlayerSeatV1,
    pub root: crate::rules_meaning_high_society_hunter_v1::RootV1,
    pub phase: crate::high_society_hunter_public_v45::PhaseV45,
    pub deceased: Option<String>,
    pub sacrificed: Option<String>,
    pub sacrifice_paid: bool,
    pub source_counter_added: bool,
    pub draw_attempted: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanHunterMenuV45 {
    pub occurrence: usize,
    pub chooser: PlayerSeatV1,
    pub original_source: HumanCardRefV1,
    pub ordered_legal: Vec<HumanCardRefV1>,
    pub may_decline: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanHunterFactsV45 {
    pub occurrences: Vec<HumanHunterOccurrenceV45>,
    pub menu: Option<HumanHunterMenuV45>,
    pub trigger_order: Vec<crate::high_society_hunter_public_v45::TriggerOrderV45>,
}
fn project_hunter(
    facts: &crate::high_society_hunter_public_v45::FactsV45,
    handles: &Handles,
) -> Result<HumanHunterFactsV45, Error> {
    let mut incarnations = Vec::new();
    let mut role = |original: Option<crate::high_society_hunter_public_v45::IncarnationV45>| {
        original.map(|original| {
            let n = incarnations
                .iter()
                .position(|row| *row == original)
                .unwrap_or_else(|| {
                    let n = incarnations.len();
                    incarnations.push(original);
                    n
                });
            format!("hunter_incarnation{}", n + 1)
        })
    };
    let occurrences = facts
        .occurrences
        .iter()
        .map(|row| {
            Ok(HumanHunterOccurrenceV45 {
                occurrence: row.occurrence,
                original_source: handles.get(&row.source)?.clone(),
                current_original_source: row
                    .current_original_source
                    .as_ref()
                    .map(|source| handles.get(source).cloned())
                    .transpose()?,
                original_controller: row.original_controller,
                root: row.root,
                phase: row.phase,
                deceased: role(row.deceased),
                sacrificed: role(row.sacrificed),
                sacrifice_paid: row.sacrifice_paid,
                source_counter_added: row.source_counter_added,
                draw_attempted: row.draw_attempted,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let menu = facts
        .menu
        .as_ref()
        .map(|row| -> Result<HumanHunterMenuV45, Error> {
            Ok(HumanHunterMenuV45 {
                occurrence: row.occurrence,
                chooser: row.chooser,
                original_source: handles.get(&row.source)?.clone(),
                ordered_legal: row
                    .ordered_legal
                    .iter()
                    .map(|source| handles.get(source).cloned())
                    .collect::<Result<Vec<_>, Error>>()?,
                may_decline: row.may_finish,
            })
        })
        .transpose()?;
    Ok(HumanHunterFactsV45 {
        occurrences,
        menu,
        trigger_order: facts.trigger_order.clone(),
    })
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanDivineGrantV44 {
    pub source: HumanCardRefV1,
    pub controller: PlayerSeatV1,
    pub recipients: Vec<HumanCardRefV1>,
    pub keyword_mask: u32,
    pub protection_color_mask: u8,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanColorDepartureV44 {
    pub source_at_departure: HumanCardRefV1,
    pub event_order: u64,
    pub original_face_index: u8,
    pub was_face_down: bool,
    pub color_mask: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanAjaniPermanentV42 {
    pub source: HumanCardRefV1,
    pub loyalty: u32,
    pub used_this_turn: bool,
    pub printed_abilities_active: bool,
    pub controller_life: i32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanAjaniActivationV42 {
    pub occurrence: u64,
    pub source_at_payment: HumanCardRefV1,
    pub current_original_source: Option<HumanCardRefV1>,
    pub ability_index: u8,
    pub loyalty_before: u32,
    pub loyalty_after: u32,
    pub stage: crate::ajani_public_v42::StageV42,
    pub historical_target: Option<String>,
    pub current_original_target: Option<HumanCardRefV1>,
    pub target_legal_at_resolution: Option<bool>,
    pub controller_life_at_resolution: Option<i32>,
    pub created_cat_count: u64,
    pub keyword_grant_expired: Option<bool>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanAjaniFactsV42 {
    pub permanents: Vec<HumanAjaniPermanentV42>,
    pub activations: Vec<HumanAjaniActivationV42>,
}
fn project_ajani_facts(
    facts: &crate::ajani_public_v42::FactsV42,
    handles: &Handles,
) -> Result<HumanAjaniFactsV42, Error> {
    let mut targets = Vec::new();
    let permanents = facts
        .permanents
        .iter()
        .map(|p| {
            Ok(HumanAjaniPermanentV42 {
                source: handles.get(&p.source)?.clone(),
                loyalty: p.loyalty,
                used_this_turn: p.used_this_turn,
                printed_abilities_active: p.printed_abilities_active,
                controller_life: p.controller_life,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let activations = facts
        .activations
        .iter()
        .map(|p| {
            let historical_target = p.target.map(|t| {
                let n = targets.iter().position(|x| *x == t).unwrap_or_else(|| {
                    let n = targets.len();
                    targets.push(t);
                    n
                });
                format!("incarnation{}", n + 1)
            });
            Ok(HumanAjaniActivationV42 {
                occurrence: p.occurrence,
                source_at_payment: handles.get(&p.source_at_payment)?.clone(),
                current_original_source: p
                    .current_original_source
                    .as_ref()
                    .map(|s| handles.get(s).cloned())
                    .transpose()?,
                ability_index: p.ability_index,
                loyalty_before: p.loyalty_before,
                loyalty_after: p.loyalty_after,
                stage: p.stage,
                historical_target,
                current_original_target: p
                    .current_original_target
                    .as_ref()
                    .map(|s| handles.get(s).cloned())
                    .transpose()?,
                target_legal_at_resolution: p.target_legal_at_resolution,
                controller_life_at_resolution: p.controller_life_at_resolution,
                created_cat_count: u64::try_from(p.created_tokens.len())
                    .map_err(|_| Error::UnsupportedPrompt)?,
                keyword_grant_expired: p.keyword_grant_expired,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(HumanAjaniFactsV42 {
        permanents,
        activations,
    })
}
fn parts(observation: &EatenObservationV46) -> super::life_ward_v8::ProjectionPartsV8<'_> {
    super::life_ward_v8::ProjectionPartsV8 {
        projection: &observation.projection,
        own_hand: &observation.own_hand,
        known_library_cards: &observation.known_library_cards,
        known_hand_cards: &observation.known_hand_cards,
        known_face_down_cards: &observation.known_face_down_cards,
        extensions: &observation.extensions,
    }
}
fn state(
    observation: &EatenObservationV46,
    handles: &mut Handles,
    human: PlayerSeatV1,
    families: &[crate::akroma_family_public_v40::FamilyProjectionV40],
) -> Result<HumanVisibleStateV1<HumanEatenExtensionsV46>, Error> {
    let old = super::life_ward_v8::project_state_parts(&parts(observation), handles)?;
    let seat = if human == PlayerSeatV1::P0 { 0 } else { 1 };
    let mut grants = observation
        .grants
        .iter()
        .map(|row| {
            let mut recipients = row
                .recipients
                .iter()
                .map(|card| handles.get(card).cloned())
                .collect::<Result<Vec<_>, Error>>()?;
            recipients.sort_by(|a, b| a.handle.cmp(&b.handle));
            Ok(HumanDivineGrantV44 {
                source: handles.get(&row.source)?.clone(),
                controller: row.controller,
                recipients,
                keyword_mask: row.keyword_mask,
                protection_color_mask: row.protection_color_mask,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    grants.sort_by(|a, b| a.source.handle.cmp(&b.source.handle));
    let color_departures = observation
        .color_departures
        .iter()
        .map(|row| {
            Ok(HumanColorDepartureV44 {
                source_at_departure: handles.get(&row.source_at_departure)?.clone(),
                event_order: row.event_order,
                original_face_index: row.original_face_index,
                was_face_down: row.was_face_down,
                color_mask: row.color_mask,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(HumanVisibleStateV1 {
        public: old.public,
        own_hand: old.own_hand,
        known_library_cards: old.known_library_cards,
        known_hand_cards: old.known_hand_cards,
        extensions: HumanEatenExtensionsV46 {
            predecessor: HumanExtensionsV9 {
                predecessor: old.extensions,
                own_attacked_this_turn: observation.attacked_this_turn[seat],
                opponent_attacked_this_turn: observation.attacked_this_turn[1 - seat],
            },
            grants,
            families: super::akroma_families_v40::states(families, handles, human)?,
            color_departures,
            ajani: project_ajani_facts(&observation.ajani, handles)?,
            faebloom: project_faebloom(&observation.faebloom, handles)?,
            divine: project_divine(&observation.divine, handles)?,
            hunter: project_hunter(&observation.hunter, handles)?,
            eaten: project_eaten(&observation.eaten, handles)?,
            trigger_order: observation.trigger_order.clone(),
            creature_target_traits: observation
                .creature_target_traits
                .iter()
                .map(|row| {
                    Ok(HumanCreatureTargetTraitsV43 {
                        creature: handles.get(&row.creature)?.clone(),
                        shroud: row.shroud,
                        hexproof: row.hexproof,
                        protection_from_monocolored: row.protection_from_monocolored,
                    })
                })
                .collect::<Result<Vec<_>, Error>>()?,
        },
        policy_context: old.policy_context,
        known_face_down_cards: old.known_face_down_cards,
        poison_counters: old.poison_counters,
        poison_prevention: old.poison_prevention,
        creatures_attacked_this_turn: old.creatures_attacked_this_turn,
        ninja_emblems: old.ninja_emblems,
        restricted_mana: old.restricted_mana,
    })
}

fn action_source_id(action: &ActionSemanticV1) -> Option<u16> {
    use ActionSemanticV1::*;
    match action {
        PlayLand { source, .. }
        | CastSpell { source, .. }
        | ActivateManaAbility { source, .. }
        | ActivateAbility { source, .. }
        | PlotSpell { source, .. }
        | ChooseTarget { source, .. }
        | ChooseCostTarget { source, .. }
        | ChooseCastMode { source, .. }
        | ChooseKicker { source, .. }
        | ChooseSpellMode { source, .. }
        | ChooseEffectOption { source, .. }
        | ChooseEffectTarget { source, .. }
        | FinishEffectSelection { source, .. }
        | ChooseEffectColor { source, .. }
        | ChooseEffectNumber { source, .. }
        | ChooseEffectBoolean { source, .. }
        | FinishTargetSelection { source, .. }
        | ChooseSpellCopyPayment { source, .. }
        | ChooseSpellCopyRetarget { source, .. }
        | ChooseCombatDamageRange { source, .. }
        | TurnFaceUp { source, .. } => Some(source.card_db_id),
        #[cfg(feature = "limited-fdn-fixtures")]
        ChooseTriggerOrderNext { source, .. } => Some(source.card_db_id),
        _ => None,
    }
}

fn label(
    action: &ActionSemanticV1,
    observation: &EatenObservationV46,
    handles: &Handles,
    human: PlayerSeatV1,
    families: &[crate::akroma_family_public_v40::FamilyProjectionV40],
) -> Result<String, Error> {
    if let Some(label) = eaten_label(action, observation, handles, human)? {
        return Ok(label);
    }
    if let ActionSemanticV1::CastSpell { actor, source } = action {
        if crate::high_society_hunter_v1::owns_definition(source.card_db_id) {
            if *actor != human {
                return Err(Error::NotHumanTurn);
            }
            return Ok(format!("Cast {} ({{3}}{{B}}{{B}}): 5/3 flying; on attack optionally sacrifice another creature for its +1/+1 counter; each other nontoken death draws",handles.name(source)?));
        }
    }
    if let Some(menu) = &observation.hunter.menu {
        match action {
            ActionSemanticV1::ChooseEffectTarget {
                actor,
                source,
                target: crate::rl::TargetRefV1::Object { object },
                selected_count,
                min_targets,
                max_targets,
            } if source == &menu.source => {
                if *actor != human || *actor != menu.chooser {
                    return Err(Error::NotHumanTurn);
                }
                if *selected_count != 0
                    || *min_targets != 0
                    || *max_targets != 1
                    || !menu.ordered_legal.contains(object)
                {
                    return Err(Error::InvalidVisibleReference);
                }
                return Ok(format!("{} attack trigger #{}: sacrifice {} to put a +1/+1 counter on the original Hunter if it remains",handles.name(source)?,menu.occurrence+1,handles.name(object)?));
            }
            ActionSemanticV1::FinishEffectSelection {
                actor,
                source,
                selected_count,
            } if source == &menu.source => {
                if *actor != human || *actor != menu.chooser {
                    return Err(Error::NotHumanTurn);
                }
                if *selected_count != 0 || !menu.may_finish {
                    return Err(Error::InvalidVisibleReference);
                }
                return Ok(format!(
                    "{} attack trigger #{}: do not sacrifice another creature",
                    handles.name(source)?,
                    menu.occurrence + 1
                ));
            }
            _ => {}
        }
    }
    if let ActionSemanticV1::OrderTriggers {
        actor,
        pending_sources,
        order,
    } = action
    {
        if observation
            .hunter
            .trigger_order
            .iter()
            .any(|row| row.controller == *actor)
        {
            if *actor != human {
                return Err(Error::NotHumanTurn);
            }
            let labels=order.iter().map(|position| {
                let source=pending_sources.get(*position).ok_or(Error::InvalidVisibleReference)?;
                let name=handles.name(source)?;
                if let Some(row)=observation.hunter.trigger_order.iter().find(|row|row.controller==*actor && row.group_position==*position) {
                    let occurrence=observation.hunter.occurrences.get(row.occurrence).ok_or(Error::InvalidVisibleReference)?;
                    let instruction=match occurrence.root {crate::rules_meaning_high_society_hunter_v1::RootV1::Attack=>"optional sacrifice for counter",crate::rules_meaning_high_society_hunter_v1::RootV1::OtherNontokenDeath=>"draw one",_=>return Err(Error::InvalidVisibleReference)};
                    Ok(format!("{name} {instruction} occurrence #{}",row.occurrence+1))
                } else {Ok(format!("{name} trigger #{}",position+1))}
            }).collect::<Result<Vec<_>,Error>>()?;
            return Ok(format!(
                "Put triggers on the stack in this order: {}",
                labels.join(", ")
            ));
        }
    }
    #[cfg(feature = "limited-fdn-fixtures")]
    if let ActionSemanticV1::ChooseTriggerOrderNext {
        actor,
        source,
        trigger_index,
        ordered_prefix,
        pending_count,
    } = action
    {
        if let Some(stage) = observation.trigger_order.as_ref() {
            if *actor != human {
                return Err(Error::NotHumanTurn);
            }
            if *trigger_index >= *pending_count || ordered_prefix.contains(trigger_index) {
                return Err(Error::InvalidVisibleReference);
            }
            if stage.controller != *actor
                || stage.pending_count != *pending_count
                || stage.ordered_prefix != *ordered_prefix
            {
                return Err(Error::InvalidVisibleReference);
            }
            let suffix = observation
                .hunter
                .trigger_order
                .iter()
                .find(|row| row.controller == *actor && row.group_position == *trigger_index)
                .map(|row| {
                    let instruction = match observation.hunter.occurrences[row.occurrence].root {
                        crate::rules_meaning_high_society_hunter_v1::RootV1::Attack => {
                            "optional sacrifice for counter"
                        }
                        crate::rules_meaning_high_society_hunter_v1::RootV1::OtherNontokenDeath => {
                            "draw one"
                        }
                        _ => "printed trigger",
                    };
                    format!("Hunter {instruction} occurrence #{}", row.occurrence + 1)
                })
                .unwrap_or_else(|| format!("trigger #{}", trigger_index + 1));
            return Ok(format!(
                "Place {} {suffix} next ({} already ordered; last placed resolves first)",
                handles.name(source)?,
                ordered_prefix.len()
            ));
        }
    }
    match action {
        ActionSemanticV1::ChooseDivineKickerV44 { actor, source, pay } => {
            if *actor != human {
                return Err(Error::NotHumanTurn);
            }
            return Ok(format!(
                "{}: {}",
                handles.name(source)?,
                if *pay {
                    "pay kicker {2}{W}; choose any finite number of your creatures, including zero"
                } else {
                    "do not kick; choose exactly one of your creatures"
                }
            ));
        }
        ActionSemanticV1::ChooseDivineTargetV44 {
            actor,
            source,
            kicked,
            selected_count,
            target,
        } => {
            if *actor != human {
                return Err(Error::NotHumanTurn);
            }
            return Ok(format!(
                "{}: select {} as target #{}{}",
                handles.name(source)?,
                handles.name(target)?,
                selected_count
                    .checked_add(1)
                    .ok_or(Error::UnsupportedPrompt)?,
                if *kicked { "; kicked" } else { "" }
            ));
        }
        ActionSemanticV1::FinishDivineTargetsV44 {
            actor,
            source,
            selected_count,
        } => {
            if *actor != human {
                return Err(Error::NotHumanTurn);
            }
            return Ok(format!(
                "{}: finish with {selected_count} targets and pay the captured kicked cost",
                handles.name(source)?
            ));
        }
        ActionSemanticV1::CastSpell { actor, source, .. }
            if crate::card_def::CARD_DEFS
                .get(usize::from(source.card_db_id))
                .is_some_and(|d| {
                    d.target_spec == crate::card_def::TargetSpec::KickerControlledCreatures
                }) =>
        {
            if *actor != human {
                return Err(Error::NotHumanTurn);
            }
            return Ok(format!("Begin casting {} (printed cost {{W}}, kicker {{2}}{{W}}): indestructible until end of turn",handles.name(source)?));
        }
        _ => {}
    }
    if let ActionSemanticV1::CastSpell { source, actor, .. } = action {
        if crate::faebloom_trick_v43::selected_definition(source.card_db_id) {
            if *actor != human {
                return Err(Error::NotHumanTurn);
            }
            return Ok(format!("Begin casting {} (printed cost {{2}}{{U}}): create two blue 1/1 flying Faeries, then a separate trigger targets an opponent's creature to tap", handles.name(source)?));
        }
    }
    if let ActionSemanticV1::ChooseTarget {
        source,
        target,
        actor,
        ..
    } = action
    {
        if crate::faebloom_trick_v43::selected_definition(source.card_db_id) {
            if *actor != human {
                return Err(Error::NotHumanTurn);
            }
            let crate::rl::TargetRefV1::Object { object } = target else {
                return Err(Error::UnsupportedPrompt);
            };
            return Ok(format!(
                "{} reflexive trigger: target opponent's {} to tap",
                handles.name(source)?,
                handles.name(object)?
            ));
        }
    }
    match action {
        ActionSemanticV1::CastSpell { source, actor, .. }
            if crate::ajani_full_v42::selected_definition(source.card_db_id) =>
        {
            if *actor != human {
                return Err(Error::NotHumanTurn);
            }
            return Ok(format!(
                "Begin casting {} (printed cost {{1}}{{W}}{{W}}): enters with four loyalty",
                handles.name(source)?
            ));
        }
        ActionSemanticV1::ActivateAbility {
            source,
            ability_index,
            actor,
            ..
        } if crate::ajani_full_v42::selected_definition(source.card_db_id) => {
            if *actor != human {
                return Err(Error::NotHumanTurn);
            }
            let rule = match *ability_index {
                0 => "+1: put a +1/+1 counter on up to one target creature",
                1 => "-3: target creature gains flying and double strike until end of turn",
                2 => "-8: create 2/2 white Cat tokens equal to your life total at resolution",
                _ => return Err(Error::UnsupportedPrompt),
            };
            return Ok(format!("{}: {rule}", handles.name(source)?));
        }
        _ => {}
    }

    if let ActionSemanticV1::CastSpell { source, actor, .. } = action {
        if crate::card_def::CARD_DEFS
            .get(usize::from(source.card_db_id))
            .is_some_and(|d| d.controlled_creature_grant.is_some())
        {
            if *actor != human {
                return Err(Error::NotHumanTurn);
            }
            let selector = crate::rules_meaning_akroma_v1::live_selector()
                .map_err(|_| Error::UnsupportedPrompt)?;
            crate::rules_meaning_akroma_v1::meaning(
                &selector,
                source.card_db_id,
                crate::rules_meaning_akroma_v1::RootV1::ControlledCreatureGrant,
            )
            .map_err(|_| Error::UnsupportedPrompt)?;
            return Ok(format!("Begin casting {} (printed cost {{7}}): your creatures have flying, first strike, vigilance, trample, haste and protection from black and red",handles.name(source)?));
        }
    }
    if let Some(label) = super::akroma_families_v40::label(
        action_source_id(action),
        action,
        families,
        handles,
        human,
    )? {
        return Ok(label);
    }
    super::labels::label_parts_v8(action, &parts(observation), handles, human)
}

type ProjectedFaebloomV43 = (
    HumanVisibleStateV1<HumanEatenExtensionsV46>,
    Vec<HumanLegalActionV1>,
    Vec<u32>,
);

pub(super) fn project_base(
    observation: &EatenObservationV46,
    actions: &[ActionSemanticV1],
    human: PlayerSeatV1,
    families: &[crate::akroma_family_public_v40::FamilyProjectionV40],
) -> Result<ProjectedFaebloomV43, Error> {
    project_base_with_handles(observation, actions, human, families)
        .map(|(state, actions, indexes, _)| (state, actions, indexes))
}

pub(super) fn project_base_with_handles(
    observation: &EatenObservationV46,
    actions: &[ActionSemanticV1],
    human: PlayerSeatV1,
    families: &[crate::akroma_family_public_v40::FamilyProjectionV40],
) -> Result<ProjectionWithHandles, Error> {
    project_with_handles(observation, actions, human, families, true)
}

type ProjectionWithHandles = (
    HumanVisibleStateV1<HumanEatenExtensionsV46>,
    Vec<HumanLegalActionV1>,
    Vec<u32>,
    Handles,
);

fn project_with_handles(
    observation: &EatenObservationV46,
    actions: &[ActionSemanticV1],
    human: PlayerSeatV1,
    families: &[crate::akroma_family_public_v40::FamilyProjectionV40],
    limited: bool,
) -> Result<ProjectionWithHandles, Error> {
    if observation.acting_player != human {
        return Err(Error::NotHumanTurn);
    }
    if actions.is_empty() {
        return Err(Error::UnsupportedPrompt);
    }
    observation
        .validate_artifact()
        .map_err(|_| Error::InvalidVisibleReference)?;
    let mut handles = Handles::from_parts(
        &observation.projection,
        &observation.own_hand,
        &observation.known_hand_cards,
        &observation.known_library_cards,
        observation.extensions.decision_local_library.as_ref(),
    )?;
    for row in &observation.projection.surface.stack {
        handles.insert(&row.source)?;
    }
    for row in &observation.extensions.historical_public_sources {
        handles.insert(&row.source)?;
    }
    for row in &observation.hunter.occurrences {
        handles.insert(&row.source)?;
        if let Some(source) = &row.current_original_source {
            handles.insert(source)?;
        }
    }
    if let Some(menu) = &observation.hunter.menu {
        for source in &menu.ordered_legal {
            handles.insert(source)?;
        }
    }
    for row in &observation.grants {
        handles.insert(&row.source)?;
        for recipient in &row.recipients {
            handles.insert(recipient)?;
        }
    }
    for row in &observation.color_departures {
        handles.insert(&row.source_at_departure)?;
    }
    super::akroma_families_v40::register(families, &mut handles)?;
    for row in &observation.ajani.permanents {
        handles.insert(&row.source)?;
    }
    for row in &observation.ajani.activations {
        handles.insert(&row.source_at_payment)?;
        if let Some(source) = &row.current_original_source {
            handles.insert(source)?;
        }
        if let Some(target) = &row.current_original_target {
            handles.insert(target)?;
        }
    }

    for row in &observation.faebloom.spells {
        handles.insert(&row.original_source)?;
        for token in row.created_tokens.iter().flatten() {
            handles.insert(token)?;
        }
    }
    for row in &observation.faebloom.reflexives {
        handles.insert(&row.original_source)?;
        if let Some(target) = &row.target_at_placement {
            handles.insert(target)?;
        }
        if let Some(target) = &row.current_target {
            handles.insert(target)?;
        }
    }
    for row in &observation.creature_target_traits {
        handles.insert(&row.creature)?;
    }
    register_divine(&observation.divine, &mut handles)?;
    for row in observation.eaten.pending.iter() {
        handles.insert(&row.source)?;
        for card in &row.sacrifice_candidates {
            handles.insert(card)?;
        }
        if let Some(card) = &row.selected_creature {
            handles.insert(card)?;
        }
    }
    for row in &observation.eaten.payments {
        handles.insert(&row.source_at_payment)?;
    }
    let initial = state(observation, &mut handles, human, families)?;
    let mut unary = BTreeMap::<String, Vec<String>>::new();
    for row in &initial.extensions.eaten.pending {
        unary
            .entry(row.source.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
        for (index, card) in row.sacrifice_candidates.iter().enumerate() {
            unary
                .entry(card.handle.clone())
                .or_default()
                .push(format!("eaten_cost_candidate_{index}"));
        }
    }
    for row in &initial.extensions.eaten.payments {
        unary
            .entry(row.source_at_payment.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
    }
    for row in &initial
        .extensions
        .predecessor
        .predecessor
        .ward_life_card_traits
    {
        unary
            .entry(row.source.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
    }
    for row in &initial.extensions.grants {
        unary
            .entry(row.source.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
        for recipient in &row.recipients {
            unary
                .entry(recipient.handle.clone())
                .or_default()
                .push(format!(
                    "akroma_grant_{}_{}",
                    row.keyword_mask, row.protection_color_mask
                ));
        }
    }
    for row in &initial.extensions.color_departures {
        unary
            .entry(row.source_at_departure.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
    }
    super::akroma_families_v40::unary(&initial.extensions.families, &mut unary)?;
    for row in &initial.extensions.ajani.permanents {
        unary
            .entry(row.source.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
    }
    for row in &initial.extensions.ajani.activations {
        unary
            .entry(row.source_at_payment.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
        if let Some(target) = &row.current_original_target {
            unary
                .entry(target.handle.clone())
                .or_default()
                .push(format!(
                    "ajani_target_{}_{}",
                    row.occurrence, row.ability_index
                ));
        }
    }

    for row in &initial.extensions.faebloom.spells {
        unary
            .entry(row.original_source.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
    }
    for row in &initial.extensions.faebloom.reflexives {
        unary
            .entry(row.original_source.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
        if let Some(target) = &row.target_at_placement {
            unary
                .entry(target.handle.clone())
                .or_default()
                .push(format!("faebloom_original_target_{}", row.ordinal));
        }
    }
    for row in &initial.extensions.creature_target_traits {
        unary
            .entry(row.creature.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
    }
    if limited {
        if let Some(combat) = &observation.projection.foundations_combat {
            for (index, assignment) in combat.assignments.iter().enumerate() {
                unary
                    .entry(handles.get(&assignment.source)?.handle.clone())
                    .or_default()
                    .push(format!(
                        "limited_damage_source_{index}_{}",
                        assignment.amount
                    ));
                if let crate::rl::TargetRefV1::Object { object } = &assignment.recipient {
                    unary
                        .entry(handles.get(object)?.handle.clone())
                        .or_default()
                        .push(format!(
                            "limited_damage_recipient_{index}_{}",
                            assignment.amount
                        ));
                }
            }
        }
    }
    divine_unary(&initial.extensions.divine, &mut unary)?;
    for row in &initial.extensions.hunter.occurrences {
        unary
            .entry(row.original_source.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
        if let Some(source) = &row.current_original_source {
            unary
                .entry(source.handle.clone())
                .or_default()
                .push(format!("hunter_original_current_source_{}", row.occurrence));
        }
    }
    if let Some(menu) = &initial.extensions.hunter.menu {
        unary
            .entry(menu.original_source.handle.clone())
            .or_default()
            .push(visible::without_handles(menu)?);
        for (position, source) in menu.ordered_legal.iter().enumerate() {
            unary
                .entry(source.handle.clone())
                .or_default()
                .push(format!("hunter_sacrifice_{}_{}", menu.occurrence, position));
        }
    }
    for rows in unary.values_mut() {
        rows.sort();
        rows.dedup();
    }
    handles.canonicalize_parts(&initial, unary)?;
    let state = state(observation, &mut handles, human, families)?;
    let mut choices = actions
        .iter()
        .enumerate()
        .map(|(index, action)| {
            Ok((
                if limited {
                    match super::limited_common_v1::label(action, &handles, human)? {
                        Some(label) => label,
                        None => label(action, observation, &handles, human, families)?,
                    }
                } else {
                    label(action, observation, &handles, human, families)?
                },
                u32::try_from(index).map_err(|_| Error::InvalidAction)?,
            ))
        })
        .collect::<Result<Vec<_>, Error>>()?;
    choices.sort_by(|a, b| a.0.cmp(&b.0));
    if choices.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err(Error::UnsupportedPrompt);
    }
    let indexes = choices.iter().map(|(_, index)| *index).collect();
    let actions = choices
        .into_iter()
        .enumerate()
        .map(|(index, (label, _))| HumanLegalActionV1 {
            action_index: index as u32,
            label,
        })
        .collect();
    Ok((state, actions, indexes, handles))
}

struct BoundEatenV46 {
    authority: AuthorityV46,
    expected: FastActorDecisionV1,
    binding: EatenDecisionBindingV46,
    public: crate::eaten_observation_v46::EatenObservationV46,
    visible: HumanDecisionEatenV46,
    engine_indexes: Vec<u32>,
}
pub struct HumanEatenProjectorV46 {
    human_seat: PlayerSeatV1,
    next_prompt_seq: u64,
    pending: Option<BoundEatenV46>,
    accepted: Option<(HumanActionReceiptV1, AuthorityV46)>,
}
impl HumanEatenProjectorV46 {
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
        session: &HumanEatenSessionV46,
        expected: FastActorDecisionV1,
    ) -> Result<HumanDecisionEatenV46, Error> {
        if expected.acting_player != self.human_seat || self.human_seat != session.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let (observation, actions, binding) = session
            .inner
            .eaten_current_decision_input_v46(expected, self.human_seat)
            .map_err(|_| Error::StaleDecision)?;
        let public = session
            .public_observation(expected)
            .map_err(|_| Error::StaleDecision)?;
        if let Some(pending) = &self.pending {
            if pending.authority == session.authority
                && pending.expected == expected
                && pending.binding == binding
                && pending.public == public
            {
                return Ok(pending.visible.clone());
            }
        }
        let next = self
            .next_prompt_seq
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        let (state, actions, engine_indexes) =
            project_base(&observation, &actions, self.human_seat, &binding.families)?;
        let visible = HumanDecisionEatenV46 {
            schema: "mtg-kernel-human-eaten-decision/v46".into(),
            human_seat: self.human_seat,
            prompt_seq: self.next_prompt_seq,
            state,
            actions,
        };
        self.pending = Some(BoundEatenV46 {
            authority: session.authority.clone(),
            expected,
            binding,
            public,
            visible: visible.clone(),
            engine_indexes,
        });
        self.next_prompt_seq = next;
        Ok(visible)
    }
    pub fn submit(
        &mut self,
        session: &mut HumanEatenSessionV46,
        request: HumanActionRequestV1,
    ) -> Result<HumanActionReceiptV1, Error> {
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
        let index = *pending
            .engine_indexes
            .get(request.action_index as usize)
            .ok_or(Error::InvalidAction)?;
        session
            .inner
            .eaten_validate_bound_decision_v46(pending.expected, self.human_seat, &pending.binding)
            .map_err(|_| Error::StaleDecision)?;
        if session
            .public_observation(pending.expected)
            .map_err(|_| Error::StaleDecision)?
            != pending.public
        {
            return Err(Error::StaleDecision);
        }
        let result = session
            .inner
            .step(pending.expected.episode_id, pending.expected.step, index);
        self.pending = None;
        result.map_err(|_| Error::ExecutionFailed)?;
        let receipt = HumanActionReceiptV1 {
            prompt_seq: request.prompt_seq,
            action_index: request.action_index,
        };
        self.accepted = Some((receipt.clone(), session.authority.clone()));
        Ok(receipt)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanFaebloomSpellV43 {
    pub ordinal: u32,
    pub original_source: HumanCardRefV1,
    pub caster: PlayerSeatV1,
    pub copied_spell: bool,
    pub stage: crate::faebloom_public_v43::StageV43,
    pub stack_ordinal: Option<u32>,
    pub current_tokens: Vec<Option<HumanCardRefV1>>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanFaebloomReflexiveV43 {
    pub ordinal: u32,
    pub original_source: HumanCardRefV1,
    pub controller: PlayerSeatV1,
    pub stage: crate::faebloom_public_v43::StageV43,
    pub stack_ordinal: Option<u32>,
    pub target_at_placement: Option<HumanCardRefV1>,
    pub current_target: Option<HumanCardRefV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanFaebloomFactsV43 {
    pub spells: Vec<HumanFaebloomSpellV43>,
    pub reflexives: Vec<HumanFaebloomReflexiveV43>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanCreatureTargetTraitsV43 {
    pub creature: HumanCardRefV1,
    pub shroud: bool,
    pub hexproof: bool,
    pub protection_from_monocolored: bool,
}
fn project_faebloom(
    facts: &crate::faebloom_public_v43::FactsV43,
    handles: &Handles,
) -> Result<HumanFaebloomFactsV43, Error> {
    let spells = facts
        .spells
        .iter()
        .map(|row| {
            Ok(HumanFaebloomSpellV43 {
                ordinal: row.ordinal,
                original_source: handles.get(&row.original_source)?.clone(),
                caster: row.caster,
                copied_spell: row.copied_spell,
                stage: row.stage,
                stack_ordinal: row.stack_ordinal,
                current_tokens: row
                    .created_tokens
                    .iter()
                    .map(|token| token.as_ref().map(|r| handles.get(r).cloned()).transpose())
                    .collect::<Result<Vec<_>, Error>>()?,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let reflexives = facts
        .reflexives
        .iter()
        .map(|row| {
            Ok(HumanFaebloomReflexiveV43 {
                ordinal: row.ordinal,
                original_source: handles.get(&row.original_source)?.clone(),
                controller: row.controller,
                stage: row.stage,
                stack_ordinal: row.stack_ordinal,
                target_at_placement: row
                    .target_at_placement
                    .as_ref()
                    .map(|r| handles.get(r).cloned())
                    .transpose()?,
                current_target: row
                    .current_target
                    .as_ref()
                    .map(|r| handles.get(r).cloned())
                    .transpose()?,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(HumanFaebloomFactsV43 { spells, reflexives })
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanDivineQuoteV44 {
    pub floating_mana_used: [u8; 6],
    pub tapped_sources: Vec<(HumanCardRefV1, crate::mana::ManaColor)>,
    pub restricted_mana_units_used: u64,
    pub life_paid: i32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanDivinePendingV44 {
    pub source: HumanCardRefV1,
    pub controller: PlayerSeatV1,
    pub kicked: Option<bool>,
    pub selected: Vec<HumanCardRefV1>,
    pub minimum: Option<u64>,
    pub maximum: Option<u64>,
    pub legal_choices: Vec<HumanCardRefV1>,
    pub legal_kicker_choices: Vec<bool>,
    pub can_finish: bool,
    pub unkicked_quote: Option<HumanDivineQuoteV44>,
    pub kicked_quote: Option<HumanDivineQuoteV44>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanDivineFinalizedV44 {
    pub stack_ordinal: u32,
    pub source: HumanCardRefV1,
    pub controller: PlayerSeatV1,
    pub kicked: bool,
    pub targets: Vec<HumanCardRefV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanDivineFactsV44 {
    pub pending: Option<HumanDivinePendingV44>,
    pub finalized: Vec<HumanDivineFinalizedV44>,
}
fn register_divine(
    facts: &crate::divine_public_v44::FactsV44,
    handles: &mut Handles,
) -> Result<(), Error> {
    if let Some(p) = &facts.pending {
        handles.insert(&p.source)?;
        for target in &p.selected {
            handles.insert(target)?;
        }
        if let Some(crate::policy_observation_variable_cast_v1::VariableCastContextV1::Targets {
            legal_choices,
            ..
        }) = &p.context
        {
            for target in legal_choices {
                handles.insert(target)?;
            }
        }
        for q in p.unkicked_quote.iter().chain(&p.kicked_quote) {
            for (source, _) in &q.tapped_sources {
                handles.insert(source)?;
            }
        }
    }
    for p in &facts.finalized {
        handles.insert(&p.source)?;
        for target in &p.targets {
            handles.insert(target)?;
        }
    }
    Ok(())
}
fn project_divine(
    facts: &crate::divine_public_v44::FactsV44,
    h: &Handles,
) -> Result<HumanDivineFactsV44, Error> {
    let refs = |rows: &[crate::rl::CardStableRefV1]| {
        rows.iter()
            .map(|r| h.get(r).cloned())
            .collect::<Result<Vec<_>, Error>>()
    };
    let quote = |q: &crate::divine_public_v44::QuoteV44| -> Result<HumanDivineQuoteV44, Error> {
        Ok(HumanDivineQuoteV44 {
            floating_mana_used: q.floating_mana_used,
            tapped_sources: q
                .tapped_sources
                .iter()
                .map(|(r, c)| Ok((h.get(r)?.clone(), *c)))
                .collect::<Result<Vec<_>, Error>>()?,
            restricted_mana_units_used: q.restricted_mana_units_used,
            life_paid: q.life_paid,
        })
    };
    let pending = facts
        .pending
        .as_ref()
        .map(|p| -> Result<HumanDivinePendingV44, Error> {
            let (legal_choices, legal_kicker_choices, can_finish) = match &p.context {
                Some(
                    crate::policy_observation_variable_cast_v1::VariableCastContextV1::Targets {
                        legal_choices,
                        can_finish,
                        ..
                    },
                ) => (refs(legal_choices)?, Vec::new(), *can_finish),
                Some(
                    crate::policy_observation_variable_cast_v1::VariableCastContextV1::Kicker {
                        legal_pay_choices,
                        ..
                    },
                ) => (Vec::new(), legal_pay_choices.clone(), false),
                None => (Vec::new(), Vec::new(), false),
            };
            Ok(HumanDivinePendingV44 {
                source: h.get(&p.source)?.clone(),
                controller: p.controller,
                kicked: p.kicked,
                selected: refs(&p.selected)?,
                minimum: p.minimum,
                maximum: p.maximum,
                legal_choices,
                legal_kicker_choices,
                can_finish,
                unkicked_quote: p.unkicked_quote.as_ref().map(quote).transpose()?,
                kicked_quote: p.kicked_quote.as_ref().map(quote).transpose()?,
            })
        })
        .transpose()?;
    let finalized = facts
        .finalized
        .iter()
        .map(|p| {
            Ok(HumanDivineFinalizedV44 {
                stack_ordinal: p.stack_ordinal,
                source: h.get(&p.source)?.clone(),
                controller: p.controller,
                kicked: p.kicked,
                targets: refs(&p.targets)?,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(HumanDivineFactsV44 { pending, finalized })
}
fn divine_unary(
    facts: &HumanDivineFactsV44,
    unary: &mut BTreeMap<String, Vec<String>>,
) -> Result<(), Error> {
    if let Some(p) = &facts.pending {
        unary
            .entry(p.source.handle.clone())
            .or_default()
            .push(visible::without_handles(p)?);
        for (i, t) in p.selected.iter().enumerate() {
            unary
                .entry(t.handle.clone())
                .or_default()
                .push(format!("divine_selected_{i}"));
        }
        for (i, t) in p.legal_choices.iter().enumerate() {
            unary
                .entry(t.handle.clone())
                .or_default()
                .push(format!("divine_legal_{i}"));
        }
    }
    for p in &facts.finalized {
        unary
            .entry(p.source.handle.clone())
            .or_default()
            .push(visible::without_handles(p)?);
        for (i, t) in p.targets.iter().enumerate() {
            unary
                .entry(t.handle.clone())
                .or_default()
                .push(format!("divine_stack_{}_target_{i}", p.stack_ordinal));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanEatenPendingV46 {
    pub source: HumanCardRefV1,
    pub controller: PlayerSeatV1,
    pub selected_branch: Option<crate::eaten_public_v46::BranchV46>,
    pub in_progress_payment: bool,
    pub selected_sacrifice: Option<String>,
    pub payable_branches: Vec<crate::eaten_public_v46::BranchV46>,
    pub selected_creature: Option<HumanCardRefV1>,
    pub sacrifice_candidates: Vec<HumanCardRefV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanEatenPaymentV46 {
    pub ordinal: u32,
    pub source_at_payment: HumanCardRefV1,
    pub controller: PlayerSeatV1,
    pub branch: crate::eaten_public_v46::BranchV46,
    pub sacrificed_incarnation: Option<String>,
    pub sacrifice_destination: Option<crate::state::Zone>,
    pub stack_ordinal: Option<u32>,
    pub original_spell_departed: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanEatenFactsV46 {
    pub pending: Option<HumanEatenPendingV46>,
    pub payments: Vec<HumanEatenPaymentV46>,
}
fn project_eaten(
    facts: &crate::eaten_public_v46::FactsV46,
    handles: &Handles,
) -> Result<HumanEatenFactsV46, Error> {
    let pending = facts
        .pending
        .as_ref()
        .map(|row| -> Result<HumanEatenPendingV46, Error> {
            Ok(HumanEatenPendingV46 {
                source: handles.get(&row.source)?.clone(),
                controller: row.controller,
                selected_branch: row.selected_branch,
                in_progress_payment: row.in_progress_payment,
                selected_sacrifice: row
                    .selected_sacrifice
                    .as_ref()
                    .map(|_| "selected_sacrifice".into()),
                payable_branches: row.payable_branches.clone(),
                selected_creature: row
                    .selected_creature
                    .as_ref()
                    .map(|r| handles.get(r).cloned())
                    .transpose()?,
                sacrifice_candidates: row
                    .sacrifice_candidates
                    .iter()
                    .map(|r| handles.get(r).cloned())
                    .collect::<Result<Vec<_>, Error>>()?,
            })
        })
        .transpose()?;
    let mut historical = Vec::new();
    let payments = facts
        .payments
        .iter()
        .map(|row| {
            let sacrificed_incarnation = row.sacrificed_incarnation.as_ref().map(|r| {
                let index = historical
                    .iter()
                    .position(|prior| prior == r)
                    .unwrap_or_else(|| {
                        let index = historical.len();
                        historical.push(r.clone());
                        index
                    });
                format!("sacrifice{}", index + 1)
            });
            Ok(HumanEatenPaymentV46 {
                ordinal: row.ordinal,
                source_at_payment: handles.get(&row.source_at_payment)?.clone(),
                controller: row.controller,
                branch: row.branch,
                sacrificed_incarnation,
                sacrifice_destination: row.sacrifice_destination,
                stack_ordinal: row.stack_ordinal,
                original_spell_departed: row.original_spell_departed,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(HumanEatenFactsV46 { pending, payments })
}
fn eaten_label(
    action: &ActionSemanticV1,
    observation: &EatenObservationV46,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<Option<String>, Error> {
    use ActionSemanticV1 as A;
    let result = match action {
        A::CastSpell { source, actor, .. }
            if crate::eaten_public_v46::selected(source.card_db_id) =>
        {
            if *actor != human {
                return Err(Error::NotHumanTurn);
            }
            Some(format!("Begin casting {} (printed cost {{B}}): additionally sacrifice a creature or pay {{3}}{{B}}, then exile target creature or planeswalker", handles.name(source)?))
        }
        A::ChooseEffectOption {
            source,
            actor,
            option_index,
            option_count,
        } => {
            if let Some(pending) = observation
                .eaten
                .pending
                .as_ref()
                .filter(|p| p.source.arena_id == source.arena_id && p.selected_branch.is_none())
            {
                if *actor != human
                    || pending.controller != human
                    || usize::from(*option_count) != pending.payable_branches.len()
                {
                    return Err(Error::UnsupportedPrompt);
                }
                let text = match pending
                    .payable_branches
                    .get(usize::from(*option_index))
                    .ok_or(Error::UnsupportedPrompt)?
                {
                    crate::eaten_public_v46::BranchV46::SacrificeCreature => {
                        "additional cost: sacrifice one controlled creature"
                    }
                    crate::eaten_public_v46::BranchV46::AdditionalMana3B => {
                        "additional cost: choose printed {3}{B} plus printed {B}, applying current cost modifiers"
                    }
                };
                Some(format!("{}: {text}", handles.name(source)?))
            } else {
                None
            }
        }
        A::ChooseCostTarget {
            source,
            actor,
            candidate,
            ..
        } => {
            if let Some(pending) = observation
                .eaten
                .pending
                .as_ref()
                .filter(|p| p.source.arena_id == source.arena_id)
            {
                if *actor != human
                    || pending.controller != human
                    || pending.selected_branch
                        != Some(crate::eaten_public_v46::BranchV46::SacrificeCreature)
                    || !pending.sacrifice_candidates.contains(candidate)
                {
                    return Err(Error::UnsupportedPrompt);
                }
                Some(format!(
                    "{} additional cost: sacrifice {}",
                    handles.name(source)?,
                    handles.name(candidate)?
                ))
            } else {
                None
            }
        }
        _ => None,
    };
    Ok(result)
}

#[cfg(test)]
mod tests;
