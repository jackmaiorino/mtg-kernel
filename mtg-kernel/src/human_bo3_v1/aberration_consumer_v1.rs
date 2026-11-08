//! Prompt-local Aberration CDA, reveal and order references.
use super::{
    muldrotha_v71::href, visible::Handles, HumanCardRefV1, HumanDecisionErrorV1 as Error,
    PlayerSeatV1,
};
use crate::{
    aberration_public_v1::{FactsV1, MeaningV1, StageV1},
    optional_trigger_public_v1::{FormV1, ReferenceV1},
    rl::{ActionSemanticV1 as Action, TargetRefV1},
};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanFactsV1 {
    meaning: MeaningV1,
    characteristics: Vec<HumanCdaV1>,
    triggers: Vec<HumanTriggerV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanCdaV1 {
    card: HumanCardRefV1,
    relative_player: PlayerSeatV1,
    base_from_cda: Option<i32>,
    form: FormV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanTriggerV1 {
    original_source: HumanCardRefV1,
    current_original_source: Option<HumanCardRefV1>,
    stage: StageV1,
    stack_position: Option<u32>,
    library_owner: PlayerSeatV1,
    revealed_prefix: Vec<HumanCardRefV1>,
    order: Option<HumanOrderV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanOrderV1 {
    chooser: PlayerSeatV1,
    selected: Vec<HumanCardRefV1>,
    legal: Vec<HumanCardRefV1>,
}

fn refs(cards: &[ReferenceV1], handles: &mut Handles) -> Result<Vec<HumanCardRefV1>, Error> {
    cards.iter().map(|card| href(card, handles)).collect()
}

pub(super) fn project(facts: &FactsV1, handles: &mut Handles) -> Result<HumanFactsV1, Error> {
    let characteristics = facts
        .characteristics
        .iter()
        .map(|row| {
            Ok(HumanCdaV1 {
                card: href(&row.card, handles)?,
                relative_player: row.relative_player,
                base_from_cda: row.base_from_cda,
                form: row.form.clone(),
            })
        })
        .collect::<Result<_, Error>>()?;
    let triggers = facts
        .triggers
        .iter()
        .map(|trigger| {
            let order = trigger
                .order
                .as_ref()
                .map(|order| {
                    Ok::<_, Error>(HumanOrderV1 {
                        chooser: order.chooser,
                        selected: refs(&order.selected, handles)?,
                        legal: refs(&order.legal, handles)?,
                    })
                })
                .transpose()?;
            Ok(HumanTriggerV1 {
                original_source: href(&trigger.source.original_source, handles)?,
                current_original_source: trigger
                    .source
                    .current_original_source
                    .as_ref()
                    .map(|card| href(card, handles))
                    .transpose()?,
                stage: trigger.stage,
                stack_position: trigger.stack_position,
                library_owner: trigger.library_owner,
                revealed_prefix: refs(&trigger.revealed_prefix, handles)?,
                order,
            })
        })
        .collect::<Result<_, Error>>()?;
    Ok(HumanFactsV1 {
        meaning: facts.meaning.clone(),
        characteristics,
        triggers,
    })
}

pub(super) fn label(
    facts: Option<&FactsV1>,
    action: &Action,
    handles: &Handles,
) -> Result<Option<String>, Error> {
    if crate::aberration_consumer_v1::action(facts, action)
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
