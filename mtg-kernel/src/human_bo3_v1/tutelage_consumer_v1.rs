//! Prompt-local handles for Tutelage's draw observers and mill order.
use super::{
    muldrotha_v71::{href, root, HumanCallRootV62},
    visible::Handles,
    HumanCardRefV1, HumanDecisionErrorV1 as Error, PlayerSeatV1,
};
use crate::{
    rl::{ActionSemanticV1 as Action, TargetRefV1},
    tutelage_consumer_v1::ActionV1,
    tutelage_public_v1::{FactsV1, MeaningV1, StageV1},
};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct HumanFactsV1 {
    meaning: MeaningV1,
    draws: Vec<HumanDrawV1>,
    current_order: Option<HumanOrderV1>,
    activation_discard: Option<HumanCallRootV62>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanDrawV1 {
    player: PlayerSeatV1,
    observers: Vec<HumanTriggerV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanTriggerV1 {
    source: HumanCallRootV62,
    stage: StageV1,
    native_position: Option<u32>,
    original_target: Option<PlayerSeatV1>,
    batches: Vec<HumanBatchV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanBatchV1 {
    attempted_count: u8,
    visible_milled_in_move_order: Vec<HumanCardRefV1>,
    repeated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanOrderV1 {
    draw_ordinal: u32,
    observer_ordinal: u32,
    chooser: PlayerSeatV1,
    selected: Vec<HumanCardRefV1>,
    legal: Vec<HumanCardRefV1>,
}

fn refs(
    cards: &[crate::optional_trigger_public_v1::ReferenceV1],
    handles: &mut Handles,
) -> Result<Vec<HumanCardRefV1>, Error> {
    cards.iter().map(|card| href(card, handles)).collect()
}

pub(super) fn project(facts: &FactsV1, handles: &mut Handles) -> Result<HumanFactsV1, Error> {
    let mut draws = vec![];
    for draw in &facts.draws {
        let mut observers = vec![];
        for row in &draw.observers {
            let batches = row
                .batches
                .iter()
                .map(|batch| {
                    Ok(HumanBatchV1 {
                        attempted_count: batch.attempted_count,
                        visible_milled_in_move_order: refs(
                            &batch.visible_milled_in_move_order,
                            handles,
                        )?,
                        repeated: batch.repeated,
                    })
                })
                .collect::<Result<_, Error>>()?;
            observers.push(HumanTriggerV1 {
                source: root(&row.source, handles)?,
                stage: row.stage,
                native_position: row.native_position,
                original_target: row.original_target,
                batches,
            });
        }
        draws.push(HumanDrawV1 {
            player: draw.player,
            observers,
        });
    }
    let current_order = facts
        .current_order
        .as_ref()
        .map(|order| {
            Ok::<_, Error>(HumanOrderV1 {
                draw_ordinal: order.draw_ordinal,
                observer_ordinal: order.observer_ordinal,
                chooser: order.chooser,
                selected: refs(&order.selected, handles)?,
                legal: refs(&order.legal, handles)?,
            })
        })
        .transpose()?;
    Ok(HumanFactsV1 {
        meaning: facts.meaning.clone(),
        draws,
        current_order,
        activation_discard: facts
            .activation_discard
            .as_ref()
            .map(|source| root(source, handles))
            .transpose()?,
    })
}

pub(super) fn label(
    facts: Option<&FactsV1>,
    action: &Action,
    handles: &Handles,
) -> Result<Option<String>, Error> {
    let annotated = crate::tutelage_consumer_v1::action(facts, action)
        .map_err(|_| Error::InvalidVisibleReference)?;
    Ok(match annotated {
        Some(ActionV1::ActivateDrawDiscard) => {
            Some("Activate Sphinx's Tutelage (5 generic, 1 blue)".into())
        }
        Some(ActionV1::TargetOpponent { .. }) => {
            Some("Target opponent with Sphinx's Tutelage".into())
        }
        Some(ActionV1::OrderMilledCard { .. }) => {
            let Action::ChooseEffectTarget {
                target: TargetRefV1::Object { object },
                ..
            } = action
            else {
                return Err(Error::InvalidVisibleReference);
            };
            Some(format!(
                "Put {} next in the graveyard order",
                handles.name(object)?
            ))
        }
        Some(ActionV1::DiscardAfterDraw) => {
            let Action::Discard { cards, .. } = action else {
                return Err(Error::InvalidVisibleReference);
            };
            Some(format!(
                "Discard {} after drawing with Sphinx's Tutelage",
                handles.name(&cards[0])?
            ))
        }
        None => None,
    })
}
