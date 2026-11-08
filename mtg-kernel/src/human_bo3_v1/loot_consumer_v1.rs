//! Prompt-local handles for Loot's public sources and private six-card look.
use super::{muldrotha_v71::{href, root, HumanCallRootV62}, visible::Handles,
    HumanCardRefV1, HumanDecisionErrorV1 as Error, PlayerSeatV1};
use crate::{loot_public_v1::{FactsV1, MeaningV1, LandsV1, StageV1},
    loot_consumer_v1::ActionV1, optional_trigger_public_v1::{ReferenceV1, FormV1},
    tinybones_public_v72::CountersV72, rl::{ActionSemanticV1 as Action, TargetRefV1}};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanFactsV1 { meaning: MeaningV1, battlefield: Vec<HumanPermanentV1>, lands: [LandsV1; 2], abilities: Vec<HumanAbilityV1> }
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanPermanentV1 { card: HumanCardRefV1, form: FormV1, counters: CountersV72, tapped: bool, summoning_sick: bool }
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanAbilityV1 { source: HumanCallRootV62, stage: StageV1, native_stack_position: Option<u32>, choice: Option<HumanChoiceV1> }
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanChoiceV1 { chooser: PlayerSeatV1, looked_in_original_order: Vec<HumanCardRefV1>, legal: Vec<HumanCardRefV1>, selected: Vec<HumanCardRefV1>, maximum_mana_value: u32 }

fn refs(cards: &[ReferenceV1], handles: &mut Handles) -> Result<Vec<HumanCardRefV1>, Error> {
    cards.iter().map(|card| href(card, handles)).collect()
}
pub(super) fn project(facts: &FactsV1, handles: &mut Handles) -> Result<HumanFactsV1, Error> {
    let battlefield = facts.battlefield.iter().map(|card| Ok(HumanPermanentV1 {
        card: href(&card.card, handles)?, form: card.form.clone(), counters: card.counters.clone(),
        tapped: card.tapped, summoning_sick: card.summoning_sick,
    })).collect::<Result<_, Error>>()?;
    let abilities = facts.abilities.iter().map(|row| {
        let source = root(&row.source, handles)?;
        let choice = row.choice.as_ref().map(|choice| Ok::<_, Error>(HumanChoiceV1 {
            chooser: choice.chooser, looked_in_original_order: refs(&choice.looked_in_original_order, handles)?,
            legal: refs(&choice.legal, handles)?, selected: refs(&choice.selected, handles)?,
            maximum_mana_value: choice.maximum_mana_value,
        })).transpose()?;
        Ok(HumanAbilityV1 { source, stage: row.stage, native_stack_position: row.native_stack_position, choice })
    }).collect::<Result<_, Error>>()?;
    Ok(HumanFactsV1 { meaning: facts.meaning.clone(), battlefield, lands: facts.lands, abilities })
}
pub(super) fn label(facts: Option<&FactsV1>, action: &Action, handles: &Handles) -> Result<Option<String>, Error> {
    let typed = crate::loot_consumer_v1::action(facts, action).map_err(|_| Error::InvalidVisibleReference)?;
    Ok(match (typed, action) {
        (Some(ActionV1::ChooseCreature), Action::ChooseEffectTarget { target: TargetRefV1::Object { object }, .. }) =>
            Some(format!("Put {} onto the battlefield with Loot", handles.name(object)?)),
        (Some(ActionV1::DeclineCreature), _) => Some("Decline a creature; put all looked cards on the bottom in random order".into()),
        (Some(ActionV1::ConfirmCreature), _) => Some("Confirm the creature; put the rest on the bottom in random order".into()),
        (None, _) => None,
        _ => return Err(Error::InvalidVisibleReference),
    })
}
