use super::{
    muldrotha_v71::href,
    tinybones_v72::{HumanTinybonesRelationV72, Incarnations},
    visible::Handles,
    HumanCardRefV1, HumanDecisionErrorV1 as Error,
};
use crate::{
    leyline_axe_consumer_v1::ActionV1,
    leyline_axe_public_v1::{FactsV1, MeaningV1},
    optional_trigger_public_v1::FormV1,
    rl::{ActionSemanticV1 as Action, TargetRefV1},
    state::{CastMethodV4, Zone},
    tinybones_public_v72::CountersV72,
};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct HumanFactsV1 {
    meaning: MeaningV1,
    opening_choice: Option<HumanCardRefV1>,
    opening_entries: Vec<HumanOriginalV1>,
    casts: Vec<HumanCastV1>,
    equip: Vec<HumanEquipV1>,
    battlefield: Vec<HumanEquipmentV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanOriginalV1 {
    original: HumanTinybonesRelationV72,
    current_original: Option<HumanCardRefV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanCastV1 {
    stack_position: u32,
    card: HumanCardRefV1,
    announcing: bool,
    uncast_copy: bool,
    origin_zone: Option<Zone>,
    finalized_method: Option<CastMethodV4>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanEquipV1 {
    stack_position: Option<u32>,
    original_source: HumanTinybonesRelationV72,
    current_source: Option<HumanCardRefV1>,
    target: Option<HumanOriginalV1>,
    target_currently_legal: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanHostV1 {
    card: HumanCardRefV1,
    form: FormV1,
    counters: CountersV72,
    receives_printed_bonus: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanEquipmentV1 {
    card: HumanCardRefV1,
    token: bool,
    host: Option<HumanHostV1>,
}

pub(super) fn project(
    facts: &FactsV1,
    handles: &mut Handles,
    roles: &mut Incarnations,
) -> Result<HumanFactsV1, Error> {
    let opening_choice = facts.opening_choice.as_ref().map(|card| href(card, handles)).transpose()?;
    let opening_entries = facts.opening_entries.iter().map(|entry| {
        Ok(HumanOriginalV1 {
            original: roles.relation(&entry.original)?,
            current_original: entry.current_original.as_ref().map(|card| href(card, handles)).transpose()?,
        })
    }).collect::<Result<_, Error>>()?;
    let casts = facts.casts.iter().map(|cast| {
        Ok(HumanCastV1 {
            stack_position: cast.stack_position,
            card: href(&cast.card, handles)?,
            announcing: cast.announcing,
            uncast_copy: cast.uncast_copy,
            origin_zone: cast.origin_zone,
            finalized_method: cast.finalized_method,
        })
    }).collect::<Result<_, Error>>()?;
    let equip = facts.equip.iter().map(|ability| {
        let source = &ability.source.original_source;
        let original_source = roles.relation(&crate::tinybones_public_v72::IncarnationV72 {
            object: source.object,
            generation: source.generation,
            owner: source.owner,
            controller: ability.source.captured_controller,
            zone: source.zone,
        })?;
        let target = ability.target.as_ref().map(|target| {
            Ok::<_, Error>(HumanOriginalV1 {
                original: roles.relation(&target.original)?,
                current_original: target.current_original.as_ref().map(|card| href(card, handles)).transpose()?,
            })
        }).transpose()?;
        Ok(HumanEquipV1 {
            stack_position: ability.stack_position,
            original_source,
            current_source: ability.source.current_original_source.as_ref().map(|card| href(card, handles)).transpose()?,
            target,
            target_currently_legal: ability.target.as_ref().is_some_and(|target| target.currently_legal),
        })
    }).collect::<Result<_, Error>>()?;
    let battlefield = facts.battlefield.iter().map(|equipment| {
        let host = equipment.host.as_ref().map(|host| {
            Ok::<_, Error>(HumanHostV1 {
                card: href(&host.card, handles)?,
                form: host.form.clone(),
                counters: host.counters.clone(),
                receives_printed_bonus: host.receives_printed_bonus,
            })
        }).transpose()?;
        Ok(HumanEquipmentV1 { card: href(&equipment.card, handles)?, token: equipment.token, host })
    }).collect::<Result<_, Error>>()?;
    Ok(HumanFactsV1 { meaning: facts.meaning.clone(), opening_choice, opening_entries, casts, equip, battlefield })
}

pub(super) fn label(
    facts: Option<&FactsV1>,
    action: &Action,
    handles: &Handles,
) -> Result<Option<String>, Error> {
    let annotated = crate::leyline_axe_consumer_v1::action(facts, action)
        .map_err(|_| Error::InvalidVisibleReference)?;
    Ok(match (annotated, action) {
        (Some(ActionV1::ChooseOpeningEntry { accept: true }), _) =>
            Some("Begin the game with Leyline Axe on the battlefield".into()),
        (Some(ActionV1::ChooseOpeningEntry { accept: false }), _) =>
            Some("Keep Leyline Axe in your hand".into()),
        (Some(ActionV1::ActivateEquip { generic_cost }), _) =>
            Some(format!("Activate Leyline Axe equip for {generic_cost} mana")),
        (Some(ActionV1::ChooseEquipTarget), Action::ChooseTarget { target: TargetRefV1::Object { object }, .. }) =>
            Some(format!("Equip Leyline Axe to {}", handles.name(object)?)),
        (None, _) => None,
        _ => return Err(Error::InvalidVisibleReference),
    })
}
