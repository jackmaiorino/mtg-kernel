//! Distinct Ravenous V51 Human consumer. Executable authority stays backend-only.
use super::visible::{self, Handles};
use super::*;
use crate::ravenous_observation_v51::RavenousObservationV51;
use crate::rl::ActionSemanticV1;
use crate::rl_session::ravenous_v51::RavenousDecisionBindingV51;
use crate::rl_session::{FastActorResponseV1, FastActorSessionSnapshotV1};
use std::collections::BTreeMap;
// These field-only helpers are intentionally instantiated in both parents:
// Fake binds MixedObservation to V50; Ravenous binds it directly to V51.
// Keeping the four inclusions preserves typed family projection without
// converting or admitting a V51 observation through the V50 contract.
// The allowances apply only to these declarations, not their other lints.
#[allow(clippy::duplicate_mod)]
#[path = "fake_v50/conqueror_facts.rs"]
mod conqueror_facts;
#[allow(clippy::duplicate_mod)]
#[path = "fake_v50/fake_facts.rs"]
mod fake_facts;
pub use fake_facts::{
    HumanFakeAuraMenuV50, HumanFakeCopyV50, HumanFakeDeathV50, HumanFakeFactsV50,
    HumanFakeGrantV50, HumanFakeSpellV50, HumanFakeTreasureV50,
};
#[allow(clippy::duplicate_mod)]
#[path = "fake_v50/facts.rs"]
mod facts;
#[allow(clippy::duplicate_mod)]
#[path = "fake_v50/season_facts.rs"]
mod season_facts;
pub use conqueror_facts::{HumanConquerorFactsV49, HumanConquerorOccurrenceV49};
use facts::{panda_label, project_panda};
pub use facts::{HumanPandaFactsV47, HumanPandaOccurrenceV47};
pub use season_facts::HumanSeasonFactsV48;
use season_facts::{project_season, register_season};
type Error = HumanDecisionErrorV1;
// Shared family projectors consume fields from this distinct observation only.
type MixedObservation = RavenousObservationV51;
mod exact_state;
mod ravenous_facts;
pub use exact_state::{HumanPublicCardV51, HumanPublicProjectionV51, HumanVisibleStateRavenousV51};
pub use ravenous_facts::{
    HumanRavenousActivationV51, HumanRavenousFactsV51, HumanRavenousMeaningV51, HumanSoulCountV51,
};

// Backend-only authority survives owning restore. Snapshots never capture it.
#[derive(Clone)]
struct AuthorityV51 {
    owner: std::sync::Arc<()>,
    generation: u64,
}
impl PartialEq for AuthorityV51 {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && std::sync::Arc::ptr_eq(&self.owner, &other.owner)
    }
}
impl Eq for AuthorityV51 {}
pub struct HumanRavenousSessionV51 {
    inner: FastActorSessionV1,
    authority: AuthorityV51,
    human_seat: PlayerSeatV1,
}
#[derive(Clone)]
pub struct HumanRavenousSnapshotV51 {
    snapshot: FastActorSessionSnapshotV1,
    owner: std::sync::Arc<()>,
}
impl HumanRavenousSessionV51 {
    pub fn from_session(
        session: FastActorSessionV1,
        human_seat: PlayerSeatV1,
    ) -> Result<Self, crate::rl::RlContractError> {
        Ok(Self {
            inner: session.into_ravenous_v51()?,
            human_seat,
            authority: AuthorityV51 {
                owner: std::sync::Arc::new(()),
                generation: 0,
            },
        })
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.current_response()
    }
    #[cfg(all(
        test,
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    pub(crate) fn ravenous_human_fixture_state(&self) -> &crate::state::GameState {
        self.inner.ravenous_fixture_state()
    }
    pub fn snapshot_v51(&self) -> HumanRavenousSnapshotV51 {
        HumanRavenousSnapshotV51 {
            snapshot: self.inner.snapshot_v1(),
            owner: self.authority.owner.clone(),
        }
    }
    pub fn restore_v51(&mut self, snapshot: &HumanRavenousSnapshotV51) -> Result<(), Error> {
        if !std::sync::Arc::ptr_eq(&self.authority.owner, &snapshot.owner) {
            return Err(Error::StaleDecision);
        }
        let next = self
            .authority
            .generation
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        self.inner
            .ravenous_restore_v51(&snapshot.snapshot)
            .map_err(|_| Error::StaleDecision)?;
        self.authority.generation = next;
        Ok(())
    }
    /// Public typed transport, not the safe Human DTO and not scorer admission.
    pub fn public_observation(
        &self,
        expected: FastActorDecisionV1,
    ) -> Result<crate::ravenous_observation_v51::RavenousObservationV51, crate::rl::RlContractError>
    {
        if expected.acting_player != self.human_seat {
            return Err(crate::rl::RlContractError(
                "Ravenous Human view is fixed to its selected seat".into(),
            ));
        }
        self.inner
            .ravenous_current_decision_input_v51(expected, self.human_seat)
            .map(|(observation, _, _)| observation)
            .map_err(|_| crate::rl::RlContractError("stale Ravenous public decision".into()))
    }
    /// Host-only policy callback. Opponent private rosters never enter a Human DTO.
    pub fn step_opponent_with<F>(
        &mut self,
        expected: FastActorDecisionV1,
        mut policy: F,
    ) -> Result<FastActorResponseV1, crate::rl::RlContractError>
    where
        F: FnMut(
            &RavenousObservationV51,
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
            .ravenous_current_decision_input_v51(expected, expected.acting_player)
            .map_err(|_| crate::rl::RlContractError("stale opponent input".into()))?;
        let index = policy(&observation, &actions)?;
        if index as usize >= actions.len() {
            return Err(crate::rl::RlContractError(
                "opponent action out of range".into(),
            ));
        }
        self.inner
            .ravenous_validate_bound_decision_v51(expected, expected.acting_player, &binding)
            .map_err(|_| crate::rl::RlContractError("opponent input changed".into()))?;
        self.inner
            .step(expected.episode_id, expected.step, index)
            .map_err(|error| crate::rl::RlContractError(format!("opponent execution:{error:?}")))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanDecisionRavenousV51 {
    pub schema: String,
    pub human_seat: PlayerSeatV1,
    pub prompt_seq: u64,
    pub state: HumanVisibleStateRavenousV51,
    pub actions: Vec<HumanLegalActionV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanRavenousExtensionsV51 {
    pub predecessor: HumanExtensionsV9,
    pub grants: Vec<HumanDivineGrantV44>,
    pub families: Vec<super::akroma_families_v40::HumanFamilyV40>,
    pub color_departures: Vec<HumanColorDepartureV44>,
    pub ajani: HumanAjaniFactsV42,
    pub faebloom: HumanFaebloomFactsV43,
    pub divine: HumanDivineFactsV44,
    pub hunter: HumanHunterFactsV45,
    pub eaten: HumanEatenFactsV46,
    pub panda: HumanPandaFactsV47,
    pub season: HumanSeasonFactsV48,
    pub conqueror: HumanConquerorFactsV49,
    pub fake: HumanFakeFactsV50,
    pub ravenous: HumanRavenousFactsV51,
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
fn parts(observation: &RavenousObservationV51) -> super::life_ward_v8::ProjectionPartsV8<'_> {
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
    observation: &RavenousObservationV51,
    handles: &mut Handles,
    human: PlayerSeatV1,
    families: &[crate::akroma_family_public_v40::FamilyProjectionV40],
) -> Result<HumanVisibleStateV1<HumanRavenousExtensionsV51>, Error> {
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
        extensions: HumanRavenousExtensionsV51 {
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
            panda: project_panda(&observation.panda, handles)?,
            season: project_season(&observation.season, handles)?,
            conqueror: conqueror_facts::project(&observation.conqueror, handles)?,
            fake: fake_facts::project(&observation.fake, handles)?,
            ravenous: ravenous_facts::project(observation, handles)?,
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

#[allow(clippy::too_many_arguments)]
fn label(
    action: &ActionSemanticV1,
    observation: &RavenousObservationV51,
    handles: &Handles,
    human: PlayerSeatV1,
    families: &[crate::akroma_family_public_v40::FamilyProjectionV40],
    next_five: Option<&crate::next_five_public_v59::FactsV59>,
    graveyard_return: Option<
        &crate::graveyard_return_observation_v61::GraveyardReturnObservationV61,
    >,
    call: Option<CompleteObservationV68<'_>>,
) -> Result<String, Error> {
    if let Some(label) = ravenous_facts::label(action, observation, handles, human)? {
        return Ok(label);
    }
    if let Some(label) = fake_facts::label(action, observation, handles, human)? {
        return Ok(label);
    }
    if let Some(label) = conqueror_facts::label(action, observation, handles, human)? {
        return Ok(label);
    }

    if let Some(menu) = &observation.season.attachment_menu {
        if let ActionSemanticV1::ChooseEffectTarget {
            actor,
            source,
            target: crate::rl::TargetRefV1::Object { object },
            selected_count,
            min_targets,
            max_targets,
        } = action
        {
            if source == &menu.equipment {
                if *actor != human || *actor != menu.chooser {
                    return Err(Error::NotHumanTurn);
                }
                if *selected_count != 0
                    || *min_targets != 1
                    || *max_targets != 1
                    || !menu.ordered_created.contains(object)
                {
                    return Err(Error::InvalidVisibleReference);
                }
                return Ok(format!(
                    "Attach {} to newly created {}",
                    handles.name(source)?,
                    handles.name(object)?
                ));
            }
        }
    }
    if let Some(label) = panda_label(action, observation, handles, human)? {
        return Ok(label);
    }
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
    let parts = parts(observation);
    if let Some(obs) = call {
        return obs.label_parts(action, &parts, handles, human);
    }
    if let Some(obs) = graveyard_return {
        return super::labels::label_parts_graveyard_return_v61(
            action, &parts, handles, human, obs,
        );
    }
    match next_five {
        Some(facts) => super::labels::label_parts_v59(action, &parts, handles, human, facts),
        None => super::labels::label_parts_v8(action, &parts, handles, human),
    }
}

pub(super) fn project_base_with_handles(
    observation: &RavenousObservationV51,
    actions: &[ActionSemanticV1],
    human: PlayerSeatV1,
    families: &[crate::akroma_family_public_v40::FamilyProjectionV40],
) -> Result<ProjectionWithHandles, Error> {
    project_with_handles(
        observation,
        actions,
        human,
        families,
        true,
        None,
        None,
        None,
        None,
        None,
    )
}

/// Shared by fixed-seat Human and actor-facing V3 transport. The complete
/// V51 artifact and exact row values are checked before any safe DTO exists.
pub(super) fn project_exact_base_with_handles(
    observation: &RavenousObservationV51,
    actions: &[ActionSemanticV1],
    human: PlayerSeatV1,
    families: &[crate::akroma_family_public_v40::FamilyProjectionV40],
) -> Result<
    (
        HumanVisibleStateRavenousV51,
        Vec<HumanLegalActionV1>,
        Vec<u32>,
        Handles,
    ),
    Error,
> {
    let (state, actions, indexes, handles) =
        project_base_with_handles(observation, actions, human, families)?;
    let state = exact_state::project(state, observation, &handles)?;
    Ok((state, actions, indexes, handles))
}

/// Complete safe inherited fields with V52 semantic traits participating in
/// prompt-local canonical handles. This is private field reuse, not V51 admission.
pub(super) fn project_elenda_base_with_handles(
    observation: &RavenousObservationV51,
    actions: &[ActionSemanticV1],
    human: PlayerSeatV1,
    families: &[crate::akroma_family_public_v40::FamilyProjectionV40],
    elenda: &crate::elenda_public_v52::FactsV52,
) -> Result<
    (
        HumanVisibleStateRavenousV51,
        Vec<HumanLegalActionV1>,
        Vec<u32>,
        Handles,
    ),
    Error,
> {
    let (state, actions, indexes, handles) = project_with_handles(
        observation,
        actions,
        human,
        families,
        true,
        Some(elenda),
        None,
        None,
        None,
        None,
    )?;
    let state = exact_state::project(state, observation, &handles)?;
    Ok((state, actions, indexes, handles))
}

/// Prepared V57 owns its complete artifact contract. Reuse only the inherited
/// layout after that validator, never a predecessor artifact identity.
pub(super) fn project_prepared_base_with_handles(
    observation: &crate::prepared_observation_v57::PreparedObservationV57,
    actions: &[ActionSemanticV1],
    human: PlayerSeatV1,
    families: &[crate::akroma_family_public_v40::FamilyProjectionV40],
) -> Result<
    (
        super::elenda_v52::HumanVisibleStateElendaV52,
        Vec<HumanLegalActionV1>,
        Vec<u32>,
        Handles,
    ),
    Error,
> {
    let (state, actions, indexes, handles) = project_with_handles(
        &observation.common.common,
        actions,
        human,
        families,
        true,
        Some(&observation.common.elenda),
        Some(observation),
        None,
        None,
        None,
    )?;
    let predecessor = exact_state::project(state, &observation.common.common, &handles)?;
    let elenda = super::elenda_v52::facts(&observation.common.elenda, &handles)?;
    Ok((
        super::elenda_v52::HumanVisibleStateElendaV52 {
            predecessor,
            elenda,
        },
        actions,
        indexes,
        handles,
    ))
}

/// NextFive V59 validates its own nested compatibility artifacts and reuses
/// only the inherited typed Human component builders below that boundary.
pub(super) fn project_next_five_base_with_handles(
    observation: &crate::next_five_observation_v59::NextFiveObservationV59,
    actions: &[ActionSemanticV1],
    human: PlayerSeatV1,
    families: &[crate::akroma_family_public_v40::FamilyProjectionV40],
) -> Result<
    (
        super::elenda_v52::HumanVisibleStateElendaV52,
        Vec<HumanLegalActionV1>,
        Vec<u32>,
        Handles,
    ),
    Error,
> {
    let (state, actions, indexes, handles) = project_with_handles(
        &observation.common.common,
        actions,
        human,
        families,
        true,
        Some(&observation.common.elenda),
        None,
        Some(observation),
        None,
        None,
    )?;
    let predecessor = exact_state::project(state, &observation.common.common, &handles)?;
    let elenda = super::elenda_v52::facts(&observation.common.elenda, &handles)?;
    Ok((
        super::elenda_v52::HumanVisibleStateElendaV52 {
            predecessor,
            elenda,
        },
        actions,
        indexes,
        handles,
    ))
}

/// The complete V61 validator owns all compatibility artifacts. No legacy
/// validator or predecessor envelope grants access to revival-bearing state.
pub(super) fn project_call_base_with_handles(
    candidates: &crate::call_candidates_v62::CallCandidatesV62,
    actions: &[ActionSemanticV1],
    human: PlayerSeatV1,
    families: &[crate::akroma_family_public_v40::FamilyProjectionV40],
) -> Result<
    (
        super::elenda_v52::HumanVisibleStateElendaV52,
        Vec<HumanLegalActionV1>,
        Vec<u32>,
        Handles,
    ),
    Error,
> {
    let observation = &candidates.observation;
    let (state, actions, indexes, handles) = project_with_handles(
        &observation.common.common,
        actions,
        human,
        families,
        true,
        Some(&observation.common.elenda),
        None,
        None,
        None,
        Some(CompleteCandidatesV68::Call(candidates)),
    )?;
    let predecessor = exact_state::project(state, &observation.common.common, &handles)?;
    let elenda = super::elenda_v52::facts(&observation.common.elenda, &handles)?;
    Ok((
        super::elenda_v52::HumanVisibleStateElendaV52 {
            predecessor,
            elenda,
        },
        actions,
        indexes,
        handles,
    ))
}

pub(super) fn project_end_turn_base_with_handles(
    candidates: &crate::end_turn_candidates_v68::EndTurnCandidatesV68,
    actions: &[ActionSemanticV1],
    human: PlayerSeatV1,
    families: &[crate::akroma_family_public_v40::FamilyProjectionV40],
) -> Result<
    (
        super::elenda_v52::HumanVisibleStateElendaV52,
        Vec<HumanLegalActionV1>,
        Vec<u32>,
        Handles,
    ),
    Error,
> {
    let observation = &candidates.observation;
    let (state, actions, indexes, handles) = project_with_handles(
        &observation.common.common,
        actions,
        human,
        families,
        true,
        Some(&observation.common.elenda),
        None,
        None,
        None,
        Some(CompleteCandidatesV68::EndTurn(candidates)),
    )?;
    let predecessor = exact_state::project(state, &observation.common.common, &handles)?;
    let elenda = super::elenda_v52::facts(&observation.common.elenda, &handles)?;
    Ok((
        super::elenda_v52::HumanVisibleStateElendaV52 {
            predecessor,
            elenda,
        },
        actions,
        indexes,
        handles,
    ))
}

pub(super) fn project_genesis_wave_base_with_handles(
    candidates: &crate::genesis_wave_candidates_v69::GenesisWaveCandidatesV69,
    actions: &[ActionSemanticV1],
    human: PlayerSeatV1,
    families: &[crate::akroma_family_public_v40::FamilyProjectionV40],
) -> Result<
    (
        super::elenda_v52::HumanVisibleStateElendaV52,
        Vec<HumanLegalActionV1>,
        Vec<u32>,
        Handles,
    ),
    Error,
> {
    let observation = &candidates.observation;
    let (state, actions, indexes, handles) = project_with_handles(
        &observation.common.common,
        actions,
        human,
        families,
        true,
        Some(&observation.common.elenda),
        None,
        None,
        None,
        Some(CompleteCandidatesV68::GenesisWave(candidates)),
    )?;
    let predecessor = exact_state::project(state, &observation.common.common, &handles)?;
    let elenda = super::elenda_v52::facts(&observation.common.elenda, &handles)?;
    Ok((
        super::elenda_v52::HumanVisibleStateElendaV52 {
            predecessor,
            elenda,
        },
        actions,
        indexes,
        handles,
    ))
}
pub(super) fn project_vivien_base_with_handles(
    candidates: &crate::vivien_candidates_v70::VivienCandidatesV70,
    actions: &[ActionSemanticV1],
    human: PlayerSeatV1,
    families: &[crate::akroma_family_public_v40::FamilyProjectionV40],
) -> Result<
    (
        super::elenda_v52::HumanVisibleStateElendaV52,
        Vec<HumanLegalActionV1>,
        Vec<u32>,
        Handles,
    ),
    Error,
> {
    let observation = &candidates.observation;
    let (state, actions, indexes, handles) = project_with_handles(
        &observation.common.common,
        actions,
        human,
        families,
        true,
        Some(&observation.common.elenda),
        None,
        None,
        None,
        Some(CompleteCandidatesV68::Vivien(candidates)),
    )?;
    let predecessor = exact_state::project(state, &observation.common.common, &handles)?;
    let elenda = super::elenda_v52::facts(&observation.common.elenda, &handles)?;
    Ok((
        super::elenda_v52::HumanVisibleStateElendaV52 {
            predecessor,
            elenda,
        },
        actions,
        indexes,
        handles,
    ))
}

pub(super) fn project_graveyard_return_base_with_handles(
    candidates: &crate::graveyard_return_candidates_v61::GraveyardReturnCandidatesV61,
    actions: &[ActionSemanticV1],
    human: PlayerSeatV1,
    families: &[crate::akroma_family_public_v40::FamilyProjectionV40],
) -> Result<
    (
        super::elenda_v52::HumanVisibleStateElendaV52,
        Vec<HumanLegalActionV1>,
        Vec<u32>,
        Handles,
    ),
    Error,
> {
    let observation = &candidates.observation;
    let (state, actions, indexes, handles) = project_with_handles(
        &observation.common.common,
        actions,
        human,
        families,
        true,
        Some(&observation.common.elenda),
        None,
        None,
        Some(candidates),
        None,
    )?;
    let predecessor = exact_state::project(state, &observation.common.common, &handles)?;
    let elenda = super::elenda_v52::facts(&observation.common.elenda, &handles)?;
    Ok((
        super::elenda_v52::HumanVisibleStateElendaV52 {
            predecessor,
            elenda,
        },
        actions,
        indexes,
        handles,
    ))
}

#[derive(Clone, Copy)]
enum CompleteCandidatesV68<'a> {
    Call(&'a crate::call_candidates_v62::CallCandidatesV62),
    EndTurn(&'a crate::end_turn_candidates_v68::EndTurnCandidatesV68),
    GenesisWave(&'a crate::genesis_wave_candidates_v69::GenesisWaveCandidatesV69),
    Vivien(&'a crate::vivien_candidates_v70::VivienCandidatesV70),
}
#[derive(Clone, Copy)]
enum CompleteOriginalV68<'a> {
    Call(&'a crate::call_observation_v62::CallObservationV62),
    EndTurn(&'a crate::end_turn_observation_v68::EndTurnObservationV68),
    GenesisWave(&'a crate::genesis_wave_observation_v69::GenesisWaveObservationV69),
    Vivien(&'a crate::vivien_observation_v70::VivienObservationV70),
}
#[derive(Clone, Copy)]
struct CompleteObservationV68<'a> {
    common: &'a crate::elenda_observation_v52::ElendaObservationV52,
    drake: &'a crate::drake_public_v53::FactsV53,
    spinner: &'a crate::spinner_public_v54::FactsV54,
    quandary: &'a crate::quandary_public_v55::FactsV55,
    phoenix: &'a crate::phoenix_public_v56::FactsV56,
    next_five: &'a crate::next_five_public_v59::FactsV59,
    graveyard_return: &'a crate::graveyard_return_public_v61::FactsV61,
    call: &'a crate::valkyries_call_public_v62::FactsV62,
    end_turn: Option<&'a crate::time_stop_public_v68::FactsV68>,
    genesis_wave: Option<&'a crate::genesis_wave_public_v69::FactsV69>,
    original: CompleteOriginalV68<'a>,
}
impl<'a> CompleteCandidatesV68<'a> {
    fn view(self) -> CompleteObservationV68<'a> {
        match self {
            Self::Call(v) => {
                let o = &v.observation;
                CompleteObservationV68 {
                    common: &o.common,
                    drake: &o.drake,
                    spinner: &o.spinner,
                    quandary: &o.quandary,
                    phoenix: &o.phoenix,
                    next_five: &o.next_five,
                    graveyard_return: &o.graveyard_return,
                    call: &o.call,
                    end_turn: None,
                    genesis_wave: None,
                    original: CompleteOriginalV68::Call(o),
                }
            }
            Self::EndTurn(v) => {
                let o = &v.observation;
                CompleteObservationV68 {
                    common: &o.common,
                    drake: &o.drake,
                    spinner: &o.spinner,
                    quandary: &o.quandary,
                    phoenix: &o.phoenix,
                    next_five: &o.next_five,
                    graveyard_return: &o.graveyard_return,
                    call: &o.call,
                    end_turn: Some(&o.end_turn),
                    genesis_wave: None,
                    original: CompleteOriginalV68::EndTurn(o),
                }
            }
            Self::GenesisWave(v) => {
                let o = &v.observation;
                CompleteObservationV68 {
                    common: &o.common,
                    drake: &o.drake,
                    spinner: &o.spinner,
                    quandary: &o.quandary,
                    phoenix: &o.phoenix,
                    next_five: &o.next_five,
                    graveyard_return: &o.graveyard_return,
                    call: &o.call,
                    end_turn: Some(&o.end_turn),
                    genesis_wave: Some(&o.genesis_wave),
                    original: CompleteOriginalV68::GenesisWave(o),
                }
            }
            Self::Vivien(v) => {
                let o = &v.observation;
                CompleteObservationV68 {
                    common: &o.common,
                    drake: &o.drake,
                    spinner: &o.spinner,
                    quandary: &o.quandary,
                    phoenix: &o.phoenix,
                    next_five: &o.next_five,
                    graveyard_return: &o.graveyard_return,
                    call: &o.call,
                    end_turn: Some(&o.end_turn),
                    genesis_wave: Some(&o.genesis_wave),
                    original: CompleteOriginalV68::Vivien(o),
                }
            }
        }
    }
    fn drake_activation(
        self,
        index: usize,
    ) -> Option<&'a crate::next_five_observation_v59::NextFiveActivationV59> {
        match self {
            Self::Call(v) => v.candidates.get(index)?.drake_activation.as_ref(),
            Self::EndTurn(v) => v.candidates.get(index)?.drake_activation.as_ref(),
            Self::GenesisWave(v) => v.candidates.get(index)?.drake_activation.as_ref(),
            Self::Vivien(v) => v.candidates.get(index)?.drake_activation.as_ref(),
        }
    }
}
impl CompleteObservationV68<'_> {
    fn validate(self) -> Result<(), Error> {
        match self.original {
            CompleteOriginalV68::Call(v) => crate::call_observation_v62::validate_artifact(v),
            CompleteOriginalV68::EndTurn(v) => {
                crate::end_turn_observation_v68::validate_artifact(v)
            }
            CompleteOriginalV68::Vivien(v) => crate::vivien_observation_v70::validate_artifact(v),
            CompleteOriginalV68::GenesisWave(v) => {
                crate::genesis_wave_observation_v69::validate_artifact(v)
            }
        }
        .map_err(|_| Error::InvalidVisibleReference)
    }
    fn choice_label(self, a: &ActionSemanticV1, h: &Handles) -> Result<Option<String>, Error> {
        match self.original {
            CompleteOriginalV68::Call(v) => super::call_v62::choice_label(v, a, h),
            CompleteOriginalV68::EndTurn(v) => super::end_turn_v68::choice_label(v, a, h),
            CompleteOriginalV68::GenesisWave(v) => super::genesis_wave_v69::choice_label(v, a, h),
            CompleteOriginalV68::Vivien(v) => super::vivien_v70::choice_label(v, a, h),
        }
    }
    fn label_parts(
        self,
        a: &ActionSemanticV1,
        p: &super::life_ward_v8::ProjectionPartsV8<'_>,
        h: &Handles,
        human: PlayerSeatV1,
    ) -> Result<String, Error> {
        match self.original {
            CompleteOriginalV68::Call(v) => super::labels::label_parts_call_v62(a, p, h, human, v),
            CompleteOriginalV68::EndTurn(v) => {
                super::labels::label_parts_end_turn_v68(a, p, h, human, v)
            }
            CompleteOriginalV68::Vivien(v) => {
                super::labels::label_parts_vivien_v70(a, p, h, human, v)
            }
            CompleteOriginalV68::GenesisWave(v) => {
                super::labels::label_parts_genesis_wave_v69(a, p, h, human, v)
            }
        }
    }
}

type ProjectionWithHandles = (
    HumanVisibleStateV1<HumanRavenousExtensionsV51>,
    Vec<HumanLegalActionV1>,
    Vec<u32>,
    Handles,
);

#[allow(clippy::too_many_arguments)]
fn project_with_handles(
    observation: &RavenousObservationV51,
    actions: &[ActionSemanticV1],
    human: PlayerSeatV1,
    families: &[crate::akroma_family_public_v40::FamilyProjectionV40],
    limited: bool,
    elenda: Option<&crate::elenda_public_v52::FactsV52>,
    prepared: Option<&crate::prepared_observation_v57::PreparedObservationV57>,
    next_five: Option<&crate::next_five_observation_v59::NextFiveObservationV59>,
    graveyard_candidates: Option<
        &crate::graveyard_return_candidates_v61::GraveyardReturnCandidatesV61,
    >,
    call_candidates: Option<CompleteCandidatesV68<'_>>,
) -> Result<ProjectionWithHandles, Error> {
    let graveyard_return = graveyard_candidates.map(|v| &v.observation);
    let call = call_candidates.map(CompleteCandidatesV68::view);
    if observation.acting_player != human {
        return Err(Error::NotHumanTurn);
    }
    if actions.is_empty() {
        return Err(Error::UnsupportedPrompt);
    }
    if let Some(obs) = call {
        obs.validate()?;
        if &obs.common.common != observation
            || Some(&obs.common.elenda) != elenda
            || graveyard_return.is_some()
            || next_five.is_some()
            || prepared.is_some()
        {
            return Err(Error::InvalidVisibleReference);
        }
    } else if let Some(obs) = graveyard_return {
        crate::graveyard_return_observation_v61::validate_artifact(obs)
            .map_err(|_| Error::InvalidVisibleReference)?;
        if &obs.common.common != observation
            || Some(&obs.common.elenda) != elenda
            || next_five.is_some()
            || prepared.is_some()
        {
            return Err(Error::InvalidVisibleReference);
        }
    } else if let Some(next_five) = next_five {
        crate::next_five_observation_v59::validate_artifact(next_five)
            .map_err(|_| Error::InvalidVisibleReference)?;
        if &next_five.common.common != observation
            || Some(&next_five.common.elenda) != elenda
            || prepared.is_some()
        {
            return Err(Error::InvalidVisibleReference);
        }
    } else if let Some(prepared) = prepared {
        crate::prepared_observation_v57::validate_artifact(prepared)
            .map_err(|_| Error::InvalidVisibleReference)?;
        if &prepared.common.common != observation || Some(&prepared.common.elenda) != elenda {
            return Err(Error::InvalidVisibleReference);
        }
    } else {
        observation
            .validate_artifact()
            .map_err(|_| Error::InvalidVisibleReference)?;
    }
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
    register_season(&observation.season, &mut handles)?;
    conqueror_facts::register(&observation.conqueror, &mut handles)?;
    fake_facts::register(&observation.fake, &mut handles)?;
    ravenous_facts::register(&observation.ravenous, &mut handles)?;
    super::akroma_families_v40::register(families, &mut handles)?;
    let next_five_facts = next_five
        .map(|o| &o.next_five)
        .or_else(|| graveyard_return.map(|o| &o.next_five))
        .or_else(|| call.map(|o| o.next_five))
        .map(|facts| super::next_five_v59::register(facts, &mut handles))
        .transpose()?;
    let graveyard_facts = graveyard_return
        .map(|o| super::graveyard_return_v61::facts(&o.graveyard_return, &mut handles))
        .transpose()?;
    let graveyard_facts = match graveyard_facts {
        Some(v) => Some(v),
        None => call
            .map(|o| super::graveyard_return_v61::facts(o.graveyard_return, &mut handles))
            .transpose()?,
    };
    let call_facts = call
        .map(|obs| super::call_v62::call_facts(obs.call, &mut handles))
        .transpose()?;
    let end_turn_facts = call
        .and_then(|o| o.end_turn)
        .map(|v| super::end_turn_v68::end_turn_facts(v, &mut handles))
        .transpose()?;
    let genesis_wave_facts = call
        .and_then(|o| o.genesis_wave)
        .map(|v| super::genesis_wave_v69::genesis_wave_facts(v, &mut handles))
        .transpose()?;
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
    for row in &observation.panda.occurrences {
        handles.insert(&row.source)?;
        if let Some(source) = &row.current_original_source {
            handles.insert(source)?;
        }
        if let Some(target) = &row.original_target {
            handles.insert(target)?;
        }
    }
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
    let prepared_facts = if let Some(obs) = call {
        Some(super::prepared_v57::facts_parts(
            obs.drake,
            obs.spinner,
            obs.quandary,
            obs.phoenix,
            &mut handles,
            true,
        )?)
    } else if let Some(obs) = graveyard_return {
        Some(super::prepared_v57::facts_parts(
            &obs.drake,
            &obs.spinner,
            &obs.quandary,
            &obs.phoenix,
            &mut handles,
            true,
        )?)
    } else if let Some(obs) = prepared {
        Some(super::prepared_v57::facts(obs, &mut handles, true)?)
    } else if let Some(obs) = next_five {
        Some(super::prepared_v57::facts_parts(
            &obs.drake,
            &obs.spinner,
            &obs.quandary,
            &obs.phoenix,
            &mut handles,
            true,
        )?)
    } else {
        None
    };
    let initial = state(observation, &mut handles, human, families)?;
    let mut unary = BTreeMap::<String, Vec<String>>::new();
    if let Some(row) = &initial.extensions.eaten.pending {
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
    for row in &initial.extensions.panda.occurrences {
        unary
            .entry(row.original_source.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
        if let Some(source) = &row.current_original_source {
            unary
                .entry(source.handle.clone())
                .or_default()
                .push(format!("panda_original_current_source_{}", row.occurrence));
        }
        if let Some(target) = &row.original_target {
            unary
                .entry(target.handle.clone())
                .or_default()
                .push(format!(
                    "panda_original_graveyard_target_{}",
                    row.occurrence
                ));
        }
    }
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
    for row in &initial.extensions.conqueror.occurrences {
        unary
            .entry(row.original_source.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
        if let Some(source) = &row.current_original_source {
            unary
                .entry(source.handle.clone())
                .or_default()
                .push(format!(
                    "conqueror_original_current_source_{}",
                    row.occurrence
                ));
        }
    }
    fake_facts::unary(&initial.extensions.fake, &mut unary)?;
    ravenous_facts::unary(&initial.extensions.ravenous, &mut unary)?;
    exact_state::unary(observation, &handles, &mut unary)?;
    if let Some(elenda) = elenda {
        super::elenda_v52::unary(elenda, &handles, &mut unary)?;
    }
    if let Some(facts) = &prepared_facts {
        super::prepared_v57::unary(facts, &mut unary)?;
    }
    for rows in unary.values_mut() {
        rows.sort();
        rows.dedup();
    }
    if let Some(facts) = &prepared_facts {
        if let Some(next_five) = &next_five_facts {
            if let Some(graveyard) = &graveyard_facts {
                if let Some(call) = &call_facts {
                    if let Some(end_turn) = &end_turn_facts {
                        if let Some(genesis) = &genesis_wave_facts {
                            handles.canonicalize_parts_with_prepared(
                                &initial,
                                unary,
                                &(facts, next_five, graveyard, call, end_turn, genesis),
                            )?;
                        } else {
                            handles.canonicalize_parts_with_prepared(
                                &initial,
                                unary,
                                &(facts, next_five, graveyard, call, end_turn),
                            )?;
                        }
                    } else {
                        handles.canonicalize_parts_with_prepared(
                            &initial,
                            unary,
                            &(facts, next_five, graveyard, call),
                        )?;
                    }
                } else {
                    handles.canonicalize_parts_with_prepared(
                        &initial,
                        unary,
                        &(facts, next_five, graveyard),
                    )?;
                }
            } else {
                handles.canonicalize_parts_with_prepared(&initial, unary, &(facts, next_five))?;
            }
        } else {
            handles.canonicalize_parts_with_prepared(&initial, unary, facts)?;
        }
    } else {
        handles.canonicalize_parts(&initial, unary)?;
    }
    let state = state(observation, &mut handles, human, families)?;
    let mut choices = actions
        .iter()
        .enumerate()
        .map(|(index, action)| {
            Ok((
                if let Some(row)=call_candidates.and_then(|v|v.drake_activation(index)) {
                    let source=handles.name(&row.source)?;
                    format!("Activate {source}: remove {} incubation counters ({} to {}); create a 2/2 blue Drake token with flying",row.count,row.before,row.after)
                } else if let Some(label) = call.map(|obs|obs.choice_label(action,&handles)).transpose()?.flatten() { label
                } else if let Some(row)=graveyard_candidates.and_then(|v|v.candidates.get(index)).and_then(|v|v.drake_activation.as_ref()) {
                    let source=handles.name(&row.source)?;
                    format!("Activate {source}: remove {} incubation counters ({} to {}); create a 2/2 blue Drake token with flying",row.count,row.before,row.after)
                } else if let Some(label) = graveyard_return.map(|obs|super::graveyard_return_v61::choice_label(obs,action,&handles)).transpose()?.flatten() {
                    label
                } else if let Some(label) = prepared
                    .map(|obs| super::prepared_v57::activation_label(obs, index, &handles))
                    .transpose()?
                    .flatten()
                {
                    label
                } else if let Some(label) = next_five
                    .map(|obs| {
                        super::prepared_v57::activation_label_next_five(
                            &obs.action_meanings,
                            index,
                            &handles,
                        )
                    })
                    .transpose()?
                    .flatten()
                {
                    label
                } else if let Some(label) = prepared
                    .map(|obs| super::prepared_v57::choice_label(obs, action, &handles))
                    .transpose()?
                    .flatten()
                {
                    label
                } else if let Some(label) = next_five
                    .map(|obs| super::prepared_v57::choice_label_next_five(obs, action, &handles))
                    .transpose()?
                    .flatten()
                {
                    label
                } else if let Some(label) = fake_facts::label(action, observation, &handles, human)?
                {
                    label
                } else if limited {
                    match super::limited_common_v1::label(action, &handles, human)? {
                        Some(label) => label,
                        None => label(
                            action,
                            observation,
                            &handles,
                            human,
                            families,
                            next_five.map(|value| &value.next_five),
                            graveyard_return,
                            call,
                        )?,
                    }
                } else {
                    label(
                        action,
                        observation,
                        &handles,
                        human,
                        families,
                        next_five.map(|value| &value.next_five),
                        graveyard_return,
                        call,
                    )?
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

struct BoundRavenousV51 {
    authority: AuthorityV51,
    expected: FastActorDecisionV1,
    binding: RavenousDecisionBindingV51,
    public: crate::ravenous_observation_v51::RavenousObservationV51,
    visible: HumanDecisionRavenousV51,
    engine_indexes: Vec<u32>,
}
pub struct HumanRavenousProjectorV51 {
    human_seat: PlayerSeatV1,
    next_prompt_seq: u64,
    pending: Option<BoundRavenousV51>,
    accepted: Option<(HumanActionReceiptV1, AuthorityV51)>,
}
impl HumanRavenousProjectorV51 {
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
        session: &HumanRavenousSessionV51,
        expected: FastActorDecisionV1,
    ) -> Result<HumanDecisionRavenousV51, Error> {
        if expected.acting_player != self.human_seat || self.human_seat != session.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let (observation, actions, binding) = session
            .inner
            .ravenous_current_decision_input_v51(expected, self.human_seat)
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
        let (state, actions, engine_indexes, _) = project_exact_base_with_handles(
            &observation,
            &actions,
            self.human_seat,
            &binding.families,
        )?;
        let visible = HumanDecisionRavenousV51 {
            schema: "mtg-kernel-human-ravenous-decision/v51".into(),
            human_seat: self.human_seat,
            prompt_seq: self.next_prompt_seq,
            state,
            actions,
        };
        self.pending = Some(BoundRavenousV51 {
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
        session: &mut HumanRavenousSessionV51,
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
            .ravenous_validate_bound_decision_v51(
                pending.expected,
                self.human_seat,
                &pending.binding,
            )
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
    observation: &RavenousObservationV51,
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

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[path = "ravenous_v51/tests.rs"]
mod tests;
