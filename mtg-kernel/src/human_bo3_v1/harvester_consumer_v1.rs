//! Harvester's visible target menu and historical Nightmare relations.
use super::{
    muldrotha_v71::href,
    tinybones_v72::{HumanTinybonesRelationV72, Incarnations},
    visible::Handles,
    HumanCardRefV1, HumanDecisionErrorV1 as Error, PlayerSeatV1,
};
use crate::{
    harvester_consumer_v1::ActionV1,
    harvester_public_v1::{FactsV1, MeaningV1, MovementV1, OutcomeV1},
    optional_trigger_public_v1::{FormV1, IdentityV1, ReferenceV1},
    rl::{ActionSemanticV1 as Action, TargetRefV1},
    state::Zone,
    tinybones_public_v72::{CountersV72, IncarnationV72},
};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanFactsV1 {
    meaning: MeaningV1,
    sources: Vec<HumanCardRefV1>,
    eligible_graveyard_cards: Vec<HumanCardRefV1>,
    chooser: Option<HumanChoiceV1>,
    activations: Vec<HumanActivationV1>,
    nightmare_copies: Vec<HumanNightmareV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanChoiceV1 {
    source: HumanCardRefV1,
    legal: Vec<HumanCardRefV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanActivationV1 {
    original_source: HumanTinybonesRelationV72,
    current_original_source: Option<HumanCardRefV1>,
    stack_position: Option<u32>,
    original_target: HumanTinybonesRelationV72,
    original_target_name: String,
    entered_turn: u32,
    entered_active_player: PlayerSeatV1,
    target_currently_legal: Option<bool>,
    outcome: Option<OutcomeV1>,
    target_movement: Option<HumanMoveV1>,
    created: Vec<HumanBirthV1>,
    other_nightmare_movements: Vec<HumanMoveV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanBirthV1 {
    original: HumanTinybonesRelationV72,
    copied_name: String,
    added_copiable_nightmare: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanMoveV1 {
    original: HumanTinybonesRelationV72,
    identity: IdentityV1,
    actual_destination: Zone,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanNightmareV1 {
    card: HumanCardRefV1,
    form: FormV1,
    counters: CountersV72,
    tapped: bool,
    summoning_sick: bool,
    copiable_nightmare: bool,
}

fn name(id: u16) -> Result<String, Error> {
    crate::card_def::CARD_DEFS.get(usize::from(id))
        .map(|definition| definition.name.to_string()).ok_or(Error::InvalidVisibleReference)
}

fn relation(card: &ReferenceV1, roles: &mut Incarnations) -> Result<HumanTinybonesRelationV72, Error> {
    roles.relation(&IncarnationV72 {
        object: card.object, generation: card.generation, owner: card.owner,
        controller: card.controller, zone: card.zone,
    })
}

fn movement(row: &MovementV1, roles: &mut Incarnations) -> Result<HumanMoveV1, Error> {
    Ok(HumanMoveV1 {
        original: roles.relation(&row.original)?, identity: row.identity.clone(),
        actual_destination: row.actual_destination,
    })
}

pub(super) fn project(
    facts: &FactsV1, handles: &mut Handles, roles: &mut Incarnations,
) -> Result<HumanFactsV1, Error> {
    let sources = facts.sources.iter().map(|row| href(&row.source, handles)).collect::<Result<_, _>>()?;
    let eligible_graveyard_cards = facts.eligible_graveyard_cards.iter()
        .map(|row| href(row, handles)).collect::<Result<_, _>>()?;
    let chooser = facts.chooser.as_ref().map(|row| Ok::<_, Error>(HumanChoiceV1 {
        source: href(&row.source, handles)?,
        legal: row.legal.iter().map(|card| href(card, handles)).collect::<Result<_, _>>()?,
    })).transpose()?;
    let activations = facts.activations.iter().map(|row| {
        let IdentityV1::Known { card_id } = row.original_target.identity else {
            return Err(Error::InvalidVisibleReference);
        };
        Ok(HumanActivationV1 {
            original_source: relation(&row.source.original_source, roles)?,
            current_original_source: row.source.current_original_source.as_ref()
                .map(|card| href(card, handles)).transpose()?,
            stack_position: row.stack_position, original_target: relation(&row.original_target, roles)?,
            original_target_name: name(card_id)?, entered_turn: row.entered_turn,
            entered_active_player: row.entered_active_player, target_currently_legal: row.target_currently_legal,
            outcome: row.outcome, target_movement: row.target_movement.as_ref()
                .map(|move_row| movement(move_row, roles)).transpose()?,
            created: row.created.iter().map(|birth| Ok(HumanBirthV1 {
                original: roles.relation(&birth.original)?, copied_name: name(birth.copied_printing)?,
                added_copiable_nightmare: birth.added_copiable_nightmare,
            })).collect::<Result<_, Error>>()?,
            other_nightmare_movements: row.other_nightmare_movements.iter()
                .map(|move_row| movement(move_row, roles)).collect::<Result<_, _>>()?,
        })
    }).collect::<Result<_, Error>>()?;
    let nightmare_copies = facts.nightmare_copies.iter().map(|row| Ok(HumanNightmareV1 {
        card: href(&row.card, handles)?, form: row.form.clone(), counters: row.counters.clone(),
        tapped: row.tapped, summoning_sick: row.summoning_sick, copiable_nightmare: row.copiable_nightmare,
    })).collect::<Result<_, Error>>()?;
    Ok(HumanFactsV1 { meaning: facts.meaning.clone(), sources, eligible_graveyard_cards,
        chooser, activations, nightmare_copies })
}

pub(super) fn label(facts: Option<&FactsV1>, action: &Action, handles: &Handles) -> Result<Option<String>, Error> {
    let annotated = crate::harvester_consumer_v1::action(facts, action).map_err(|_| Error::InvalidVisibleReference)?;
    Ok(match (annotated, action) {
        (None, _) => None,
        (Some(ActionV1::AnnounceTapAbility), _) => Some("Activate Abyssal Harvester's tap ability".into()),
        (Some(ActionV1::ChooseOriginalGraveyardTarget), Action::ChooseTarget { target: TargetRefV1::Object { object }, .. }) =>
            Some(format!("Choose {} for Abyssal Harvester", handles.name(object)?)),
        _ => return Err(Error::InvalidVisibleReference),
    })
}
