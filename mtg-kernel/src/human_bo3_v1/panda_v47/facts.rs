use super::*;
use crate::rules_meaning_fiendish_panda_v1::RootV1;
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanPandaOccurrenceV47 {
    pub occurrence: usize,
    pub original_source: HumanCardRefV1,
    pub current_original_source: Option<HumanCardRefV1>,
    pub original_controller: PlayerSeatV1,
    pub root: RootV1,
    pub phase: crate::panda_public_v47::PhaseV47,
    pub life_gained: Option<i32>,
    pub captured_power: Option<i32>,
    pub original_target: Option<HumanCardRefV1>,
    pub operation_attempted: bool,
    pub actual_counters_added: i32,
    pub returned_incarnation: Option<String>,
    pub return_destination: Option<crate::state::Zone>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanPandaFactsV47 {
    pub occurrences: Vec<HumanPandaOccurrenceV47>,
    pub trigger_order: Vec<crate::panda_public_v47::TriggerOrderV47>,
}
pub(super) fn project_panda(
    facts: &crate::panda_public_v47::FactsV47,
    handles: &Handles,
) -> Result<HumanPandaFactsV47, Error> {
    let mut roles = BTreeMap::new();
    let mut next = 1usize;
    let occurrences = facts
        .occurrences
        .iter()
        .map(|row| {
            let returned_incarnation = row.returned_incarnation.map(|incarnation| {
                roles
                    .entry((incarnation.object, incarnation.generation))
                    .or_insert_with(|| {
                        let role = format!("panda_returned_incarnation_{next}");
                        next += 1;
                        role
                    })
                    .clone()
            });
            Ok(HumanPandaOccurrenceV47 {
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
                life_gained: row.life_gained,
                captured_power: row.captured_power,
                original_target: row
                    .original_target
                    .as_ref()
                    .map(|target| handles.get(target).cloned())
                    .transpose()?,
                operation_attempted: row.operation_attempted,
                actual_counters_added: row.actual_counters_added,
                returned_incarnation,
                return_destination: row.return_destination,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(HumanPandaFactsV47 {
        occurrences,
        trigger_order: facts.trigger_order.clone(),
    })
}
fn instruction(
    observation: &PandaObservationV47,
    actor: PlayerSeatV1,
    position: usize,
) -> Result<Option<String>, Error> {
    let Some(order) = observation
        .panda
        .trigger_order
        .iter()
        .find(|row| row.controller == actor && row.group_position == position)
    else {
        return Ok(None);
    };
    let row = observation
        .panda
        .occurrences
        .get(order.occurrence)
        .ok_or(Error::InvalidVisibleReference)?;
    let text = match row.root {
        RootV1::ControllerLifeGain => "one +1/+1 counter instruction on the original Panda".into(),
        RootV1::SourceDeath => format!(
            "mandatory other non-Bear creature return with captured power {}",
            row.captured_power.ok_or(Error::InvalidVisibleReference)?
        ),
        RootV1::CreatureSpell => return Err(Error::InvalidVisibleReference),
    };
    Ok(Some(format!(
        "{text}, Panda occurrence #{}",
        row.occurrence + 1
    )))
}
pub(super) fn panda_label(
    action: &ActionSemanticV1,
    observation: &PandaObservationV47,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<Option<String>, Error> {
    if let ActionSemanticV1::CastSpell { actor, source } = action {
        if observation
            .panda
            .meanings
            .iter()
            .any(|row| row.card_id == source.card_db_id && row.root == RootV1::CreatureSpell)
        {
            if *actor != human {
                return Err(Error::NotHumanTurn);
            }
            return Ok(Some(format!("Cast {} ({{2}}{{W}}{{B}}): 3/2 Bear Demon; each controller life-gain event adds a counter; on death return another non-Bear creature card using its last power",handles.name(source)?)));
        }
    }
    if let ActionSemanticV1::ChooseTarget {
        actor,
        source,
        target,
        ..
    } = action
    {
        if let Some(row) = observation.panda.occurrences.iter().find(|row| {
            row.root == RootV1::SourceDeath
                && row.source == *source
                && row.original_controller == *actor
                && row.phase == crate::panda_public_v47::PhaseV47::Held
        }) {
            if *actor != human {
                return Err(Error::NotHumanTurn);
            }
            return Ok(Some(format!("{} death occurrence #{}: return {} from your graveyard (another non-Bear creature card, mana value at most captured power {})",
                handles.name(source)?,row.occurrence+1,labels::target(target,handles,human)?,
                row.captured_power.ok_or(Error::InvalidVisibleReference)?)));
        }
    }
    if let ActionSemanticV1::OrderTriggers {
        actor,
        pending_sources,
        order,
    } = action
    {
        if observation
            .panda
            .trigger_order
            .iter()
            .any(|row| row.controller == *actor)
        {
            if *actor != human {
                return Err(Error::NotHumanTurn);
            }
            let labels=order.iter().map(|position| {
                let source=pending_sources.get(*position).ok_or(Error::InvalidVisibleReference)?;
                let text=match instruction(observation,*actor,*position)? {
                    Some(text)=>text,
                    None=>{
                        if let Some(row)=observation.hunter.trigger_order.iter()
                            .find(|row|row.controller==*actor && row.group_position==*position) {
                            let original=observation.hunter.occurrences.get(row.occurrence)
                                .ok_or(Error::InvalidVisibleReference)?;
                            let text=match original.root {
                                crate::rules_meaning_high_society_hunter_v1::RootV1::Attack=>"optional sacrifice for counter",
                                crate::rules_meaning_high_society_hunter_v1::RootV1::OtherNontokenDeath=>"draw one",
                                _=>return Err(Error::InvalidVisibleReference),
                            };
                            format!("{text}, Hunter occurrence #{}",row.occurrence+1)
                        } else {format!("trigger #{}",position+1)}
                    }
                };
                Ok(format!("{} {text}",handles.name(source)?))
            }).collect::<Result<Vec<_>,Error>>()?;
            return Ok(Some(format!(
                "Put triggers on the stack in this order (last resolves first): {}",
                labels.join(", ")
            )));
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
        if let Some(text) = instruction(observation, *actor, *trigger_index)? {
            if *actor != human {
                return Err(Error::NotHumanTurn);
            }
            let stage = observation
                .trigger_order
                .as_ref()
                .ok_or(Error::InvalidVisibleReference)?;
            if stage.controller != *actor
                || stage.pending_count != *pending_count
                || stage.ordered_prefix != *ordered_prefix
                || *trigger_index >= *pending_count
                || ordered_prefix.contains(trigger_index)
            {
                return Err(Error::InvalidVisibleReference);
            }
            return Ok(Some(format!(
                "Place {} {text} next ({} already ordered; last placed resolves first)",
                handles.name(source)?,
                ordered_prefix.len()
            )));
        }
    }
    Ok(None)
}
