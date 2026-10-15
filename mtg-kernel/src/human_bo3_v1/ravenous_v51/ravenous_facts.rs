//! Explicit safe Amulet fields. Incarnation roles never include a sacrificed
//! creature's printing or an engine object or generation number.
use super::*;
use crate::ravenous_public_v51 as public;
use crate::rules_meaning_ravenous_amulet_v51 as meaning;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanRavenousMeaningV51 {
    pub root: meaning::RootV51,
    pub ordered_costs: Vec<meaning::CostV51>,
    pub ordered_instructions: Vec<meaning::InstructionV51>,
    pub sorcery_speed_only: bool,
    pub untargeted: bool,
    pub ability_controller_is_original_controller: bool,
    pub loss_uses_original_generation_lki: bool,
    pub placement_uses_resolution_world: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanSoulCountV51 {
    pub permanent: HumanCardRefV1,
    pub count: i32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanRavenousActivationV51 {
    pub occurrence: usize,
    pub original_source: HumanCardRefV1,
    pub current_original_source: Option<HumanCardRefV1>,
    pub original_controller: PlayerSeatV1,
    pub root: meaning::RootV51,
    /// Prompt-local incarnation role, not the underlying sacrificed printing.
    pub sacrificed: String,
    pub self_sacrifice_soul_count: Option<i32>,
    pub outcome: public::OutcomeV51,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanRavenousFactsV51 {
    pub meanings: Vec<HumanRavenousMeaningV51>,
    pub soul_counts: Vec<HumanSoulCountV51>,
    pub activations: Vec<HumanRavenousActivationV51>,
}

pub(super) fn register(facts: &public::FactsV51, handles: &mut Handles) -> Result<(), Error> {
    for row in &facts.activations {
        handles.insert(&row.source)?;
        if let Some(source) = &row.current_original_source {
            handles.insert(source)?;
        }
    }
    Ok(())
}

pub(super) fn project(
    observation: &RavenousObservationV51,
    handles: &Handles,
) -> Result<HumanRavenousFactsV51, Error> {
    let facts = &observation.ravenous;
    let meanings = facts
        .meanings
        .iter()
        .map(|row| HumanRavenousMeaningV51 {
            root: row.root,
            ordered_costs: row.ordered_costs.clone(),
            ordered_instructions: row.ordered_instructions.clone(),
            sorcery_speed_only: row.sorcery_speed_only,
            untargeted: row.untargeted,
            ability_controller_is_original_controller: row
                .ability_controller_is_original_controller,
            loss_uses_original_generation_lki: row.loss_uses_original_generation_lki,
            placement_uses_resolution_world: row.placement_uses_resolution_world,
        })
        .collect();
    let soul_counts = facts
        .soul_counts
        .iter()
        .map(|row| {
            let mut cards = observation.battlefield.iter().flatten().filter(|card| {
                card.card.stable.arena_id == row.permanent.object
                    && card.card.stable.zone_change_count == row.permanent.generation
            });
            let card = cards.next().ok_or(Error::InvalidVisibleReference)?;
            if cards.next().is_some() || card.soul != row.count {
                return Err(Error::InvalidVisibleReference);
            }
            Ok(HumanSoulCountV51 {
                permanent: handles.get(&card.card.stable)?.clone(),
                count: row.count,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let mut incarnations = Vec::new();
    let activations = facts
        .activations
        .iter()
        .map(|row| {
            let position = incarnations
                .iter()
                .position(|value| *value == row.sacrificed)
                .unwrap_or_else(|| {
                    let position = incarnations.len();
                    incarnations.push(row.sacrificed);
                    position
                });
            Ok(HumanRavenousActivationV51 {
                occurrence: row.occurrence,
                original_source: handles.get(&row.source)?.clone(),
                current_original_source: row
                    .current_original_source
                    .as_ref()
                    .map(|source| handles.get(source).cloned())
                    .transpose()?,
                original_controller: row.original_controller,
                root: row.root,
                sacrificed: format!("ravenous_incarnation{}", position + 1),
                self_sacrifice_soul_count: row.self_sacrifice_soul_count,
                outcome: row.outcome.clone(),
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(HumanRavenousFactsV51 {
        meanings,
        soul_counts,
        activations,
    })
}

pub(super) fn unary(
    facts: &HumanRavenousFactsV51,
    traits: &mut BTreeMap<String, Vec<String>>,
) -> Result<(), Error> {
    for row in &facts.activations {
        traits
            .entry(row.original_source.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
        if let Some(source) = &row.current_original_source {
            traits
                .entry(source.handle.clone())
                .or_default()
                .push(format!(
                    "ravenous_current_original_source_{}",
                    row.occurrence
                ));
        }
    }
    Ok(())
}

pub(super) fn label(
    action: &ActionSemanticV1,
    observation: &RavenousObservationV51,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<Option<String>, Error> {
    match action {
        ActionSemanticV1::ActivateAbility {
            actor,
            source,
            ability_index,
            granted_ability,
            ..
        } if crate::ravenous_amulet_v1::definition(source.card_db_id)
            && granted_ability.is_none() =>
        {
            if *actor != human {
                return Err(Error::NotHumanTurn);
            }
            let root = match ability_index {
                0 => meaning::RootV51::DrawAndSoul,
                1 => meaning::RootV51::SoulLoss,
                _ => return Err(Error::UnsupportedPrompt),
            };
            let descriptor = observation
                .ravenous
                .meanings
                .iter()
                .find(|row| row.root == root)
                .ok_or(Error::InvalidVisibleReference)?;
            meaning::validate(descriptor).map_err(|_| Error::InvalidVisibleReference)?;
            let name = handles.name(source)?;
            Ok(Some(match root {
                meaning::RootV51::DrawAndSoul => format!("Activate {name}: {{1}}, tap, sacrifice a creature: draw one card, then put a soul counter on {name}. Activate only as a sorcery."),
                meaning::RootV51::SoulLoss => format!("Activate {name}: {{4}}, tap, sacrifice {name}: each opponent loses life equal to the number of soul counters it had before being sacrificed."),
            }))
        }
        ActionSemanticV1::ChooseCostTarget {
            actor,
            source,
            candidate,
            ..
        } if crate::ravenous_amulet_v1::definition(source.card_db_id) => {
            if *actor != human {
                return Err(Error::NotHumanTurn);
            }
            Ok(Some(format!(
                "{} activation cost: sacrifice {}",
                handles.name(source)?,
                handles.name(candidate)?
            )))
        }
        _ => Ok(None),
    }
}
