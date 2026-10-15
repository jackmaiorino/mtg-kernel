use super::*;
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanConquerorOccurrenceV49 {
    pub occurrence: usize,
    pub loss_operation: usize,
    pub original_source: HumanCardRefV1,
    pub current_original_source: Option<HumanCardRefV1>,
    pub original_controller: PlayerSeatV1,
    pub losing_opponent: PlayerSeatV1,
    pub saved_actual_amount: i32,
    pub phase: crate::conqueror_public_v49::PhaseV49,
    pub actual_gain: Option<i32>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanConquerorFactsV49 {
    pub losses: Vec<crate::conqueror_public_v49::LossOccurrenceV49>,
    pub occurrences: Vec<HumanConquerorOccurrenceV49>,
    pub trigger_order: Vec<crate::conqueror_public_v49::TriggerOrderV49>,
}
pub(super) fn register(
    facts: &crate::conqueror_public_v49::FactsV49,
    handles: &mut Handles,
) -> Result<(), Error> {
    for row in &facts.occurrences {
        handles.insert(&row.source)?;
        if let Some(current) = &row.current_original_source {
            handles.insert(current)?;
        }
    }
    Ok(())
}
pub(super) fn project(
    facts: &crate::conqueror_public_v49::FactsV49,
    handles: &Handles,
) -> Result<HumanConquerorFactsV49, Error> {
    let occurrences = facts
        .occurrences
        .iter()
        .map(|row| {
            Ok(HumanConquerorOccurrenceV49 {
                occurrence: row.occurrence,
                loss_operation: row.loss_operation,
                original_source: handles.get(&row.source)?.clone(),
                current_original_source: row
                    .current_original_source
                    .as_ref()
                    .map(|source| handles.get(source).cloned())
                    .transpose()?,
                original_controller: row.original_controller,
                losing_opponent: row.losing_opponent,
                saved_actual_amount: row.saved_actual_amount,
                phase: row.phase,
                actual_gain: row.actual_gain,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(HumanConquerorFactsV49 {
        losses: facts.losses.clone(),
        occurrences,
        trigger_order: facts.trigger_order.clone(),
    })
}
fn instruction(
    observation: &MixedObservation,
    actor: PlayerSeatV1,
    position: usize,
    source: &crate::rl::CardStableRefV1,
    handles: &Handles,
) -> Result<String, Error> {
    let handle = handles.get(source)?;
    if let Some(order) = observation
        .conqueror
        .trigger_order
        .iter()
        .find(|row| row.controller == actor && row.group_position == position)
    {
        let row = observation
            .conqueror
            .occurrences
            .get(order.occurrence)
            .ok_or(Error::StaleDecision)?;
        if &row.source != source {
            return Err(Error::StaleDecision);
        }
        Ok(format!(
            "gain {} life, loss operation {}, Conqueror occurrence {}, {}",
            row.saved_actual_amount,
            row.loss_operation + 1,
            row.occurrence + 1,
            handle.handle
        ))
    } else {
        // Position is public in the complete ordered roster, distinguishing
        // unrelated same-source roots without exposing their native IDs.
        Ok(format!(
            "trigger position {} {}",
            position + 1,
            handle.handle
        ))
    }
}
pub(super) fn label(
    action: &ActionSemanticV1,
    observation: &MixedObservation,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<Option<String>, Error> {
    use ActionSemanticV1 as A;
    if let A::CastSpell { actor, source } = action {
        if observation.conqueror.meanings.iter().any(|row| {
            row.card_id == source.card_db_id
                && row.root
                    == crate::rules_meaning_bloodthirsty_conqueror_v49::RootV49::CreatureSpell
        }) {
            if *actor != human {
                return Err(Error::NotHumanTurn);
            }
            return Ok(Some(format!("Cast {} ({{3}}{{B}}{{B}}): 5/5 Vampire Knight, flying and deathtouch; each actual opponent life-loss batch gains its saved amount", handles.name(source)?)));
        }
    }
    let actor = match action {
        A::OrderTriggers { actor, .. } => *actor,
        #[cfg(feature = "limited-fdn-fixtures")]
        A::ChooseTriggerOrderNext { actor, .. } => *actor,
        _ => return Ok(None),
    };
    if actor != human {
        return Err(Error::NotHumanTurn);
    }
    if !observation
        .conqueror
        .trigger_order
        .iter()
        .any(|row| row.controller == actor)
    {
        return Ok(None);
    }
    match action {
        A::OrderTriggers {
            pending_sources,
            order,
            ..
        } => {
            let mut instructions = Vec::new();
            for position in order {
                let source = pending_sources.get(*position).ok_or(Error::StaleDecision)?;
                instructions.push(instruction(observation, actor, *position, source, handles)?);
            }
            Ok(Some(format!(
                "Order triggers (last resolves first): {}",
                instructions.join("; ")
            )))
        }
        #[cfg(feature = "limited-fdn-fixtures")]
        A::ChooseTriggerOrderNext {
            source,
            trigger_index,
            ..
        } => {
            let stage = observation
                .trigger_order
                .as_ref()
                .ok_or(Error::StaleDecision)?;
            if stage.controller != actor {
                return Err(Error::StaleDecision);
            }
            Ok(Some(format!(
                "Next trigger {} of {}: {}",
                stage.ordered_prefix.len() + 1,
                stage.pending_count,
                instruction(observation, actor, *trigger_index, source, handles)?
            )))
        }
        _ => Ok(None),
    }
}
