//! Prompt-local Progenitus observations and labels for exact native order rows.
use super::{
    muldrotha_v71::href, visible::Handles, HumanCardRefV1, HumanDecisionErrorV1 as Error,
    PlayerSeatV1,
};
use crate::{
    optional_trigger_public_v1::FormV1,
    progenitus_public_v1::{FactsV1, MeaningV1, ShuffleV1},
    rl::{ActionSemanticV1 as Action, TargetRefV1},
    state::{CastMethodV4, Zone},
};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanFactsV1 {
    meaning: MeaningV1,
    battlefield: Vec<HumanPermanentV1>,
    spells: Vec<HumanSpellV1>,
    completed_reveals_and_shuffles: Vec<ShuffleV1>,
    graveyard_order: Option<HumanOrderV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanPermanentV1 {
    card: HumanCardRefV1,
    token: bool,
    form: FormV1,
    protection_from_everything_active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanSpellV1 {
    card: HumanCardRefV1,
    announcing: bool,
    uncast_copy: bool,
    origin_zone: Option<Zone>,
    finalized_method: Option<CastMethodV4>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanOrderV1 {
    chooser: PlayerSeatV1,
    selected: Vec<HumanCardRefV1>,
    legal: Vec<HumanCardRefV1>,
}

pub(super) fn project(facts: &FactsV1, handles: &mut Handles) -> Result<HumanFactsV1, Error> {
    let battlefield = facts
        .battlefield
        .iter()
        .map(|permanent| {
            Ok(HumanPermanentV1 {
                card: href(&permanent.card, handles)?,
                token: permanent.token,
                form: permanent.form.clone(),
                protection_from_everything_active: permanent.protection_from_everything_active,
            })
        })
        .collect::<Result<_, Error>>()?;
    let spells = facts
        .spells
        .iter()
        .map(|spell| {
            Ok(HumanSpellV1 {
                card: href(&spell.card, handles)?,
                announcing: spell.announcing,
                uncast_copy: spell.uncast_copy,
                origin_zone: spell.origin_zone,
                finalized_method: spell.finalized_method,
            })
        })
        .collect::<Result<_, Error>>()?;
    let graveyard_order = facts
        .graveyard_order
        .as_ref()
        .map(|order| {
            Ok::<_, Error>(HumanOrderV1 {
                chooser: order.chooser,
                selected: order
                    .selected
                    .iter()
                    .map(|card| href(card, handles))
                    .collect::<Result<_, _>>()?,
                legal: order
                    .legal
                    .iter()
                    .map(|card| href(card, handles))
                    .collect::<Result<_, _>>()?,
            })
        })
        .transpose()?;
    Ok(HumanFactsV1 {
        meaning: facts.meaning.clone(),
        battlefield,
        spells,
        completed_reveals_and_shuffles: facts.completed_reveals_and_shuffles.clone(),
        graveyard_order,
    })
}

pub(super) fn label(
    facts: Option<&FactsV1>,
    action: &Action,
    handles: &Handles,
) -> Result<Option<String>, Error> {
    if crate::progenitus_consumer_v1::action(facts, action)
        .map_err(|_| Error::InvalidVisibleReference)?
        .is_none()
    {
        return Ok(None);
    }
    let Action::ChooseEffectTarget {
        target: TargetRefV1::Object { object },
        ..
    } = action
    else {
        return Err(Error::InvalidVisibleReference);
    };
    Ok(Some(format!(
        "Put {} next in the graveyard",
        handles.name(object)?
    )))
}
