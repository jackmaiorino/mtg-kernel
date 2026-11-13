//! Prompt-local Fiend Artisan payment and search facts.
use super::{
    muldrotha_v71::{href, root, HumanCallRootV62},
    visible::Handles,
    HumanCardRefV1, HumanDecisionErrorV1 as Error,
};
use crate::{
    fiend_artisan_consumer_v1::{self as consumer, ActionV1},
    fiend_artisan_public_v1::{ChoiceV1, FactsV1, MeaningV1},
    optional_trigger_public_v1::{FormV1, ReferenceV1},
    rl::{ActionSemanticV1 as Action, TargetRefV1},
    tinybones_public_v72::CountersV72,
};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanFactsV1 {
    meaning: MeaningV1,
    battlefield: Vec<HumanPermanentV1>,
    activations: Vec<HumanActivationV1>,
    chooser: Option<HumanChoiceV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanPermanentV1 {
    card: HumanCardRefV1,
    is_token: bool,
    tapped: bool,
    summoning_sick: bool,
    marked_damage: u32,
    form: FormV1,
    counters: CountersV72,
    graveyard_creature_bonus: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanActivationV1 {
    source: HumanCallRootV62,
    stack_position: Option<u32>,
    paid_x: u8,
    original_sacrifice: HumanCardRefV1,
    searching: bool,
    shuffled: bool,
    countered: bool,
    actual_public_arrival: Option<HumanCardRefV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
enum HumanChoiceV1 {
    PayX {
        source: HumanCardRefV1,
        maximum: u8,
    },
    Sacrifice {
        source: HumanCardRefV1,
        x: u8,
        legal: Vec<HumanCardRefV1>,
    },
    Search {
        source: HumanCardRefV1,
        paid_x: u8,
        selected: Vec<HumanCardRefV1>,
        legal: Vec<HumanCardRefV1>,
    },
}

fn refs(cards: &[ReferenceV1], handles: &mut Handles) -> Result<Vec<HumanCardRefV1>, Error> {
    cards.iter().map(|card| href(card, handles)).collect()
}

pub(super) fn project(facts: &FactsV1, handles: &mut Handles) -> Result<HumanFactsV1, Error> {
    let battlefield = facts
        .battlefield
        .iter()
        .map(|row| {
            let card = &row.current;
            Ok(HumanPermanentV1 {
                card: href(&card.card, handles)?,
                is_token: card.is_token,
                tapped: card.tapped,
                summoning_sick: card.summoning_sick,
                marked_damage: card.marked_damage,
                form: card.form.clone(),
                counters: card.counters.clone(),
                graveyard_creature_bonus: row.graveyard_creature_bonus,
            })
        })
        .collect::<Result<_, Error>>()?;
    let activations = facts
        .activations
        .iter()
        .map(|row| {
            Ok(HumanActivationV1 {
                source: root(&row.source, handles)?,
                stack_position: row.stack_position,
                paid_x: row.paid_x,
                original_sacrifice: href(&row.original_sacrifice, handles)?,
                searching: row.searching,
                shuffled: row.shuffled,
                countered: row.countered,
                actual_public_arrival: row
                    .actual_public_arrival
                    .as_ref()
                    .map(|card| href(card, handles))
                    .transpose()?,
            })
        })
        .collect::<Result<_, Error>>()?;
    let chooser = facts
        .chooser
        .as_ref()
        .map(|choice| {
            Ok::<_, Error>(match choice {
                ChoiceV1::PayX { source, maximum } => HumanChoiceV1::PayX {
                    source: href(source, handles)?,
                    maximum: *maximum,
                },
                ChoiceV1::Sacrifice { source, x, legal } => HumanChoiceV1::Sacrifice {
                    source: href(source, handles)?,
                    x: *x,
                    legal: refs(legal, handles)?,
                },
                ChoiceV1::Search {
                    source,
                    paid_x,
                    selected,
                    legal,
                } => HumanChoiceV1::Search {
                    source: href(source, handles)?,
                    paid_x: *paid_x,
                    selected: refs(selected, handles)?,
                    legal: refs(legal, handles)?,
                },
            })
        })
        .transpose()?;
    Ok(HumanFactsV1 {
        meaning: facts.meaning.clone(),
        battlefield,
        activations,
        chooser,
    })
}

pub(super) fn label(
    facts: Option<&FactsV1>,
    action: &Action,
    handles: &Handles,
) -> Result<Option<String>, Error> {
    Ok(match (consumer::action(facts, action), action) {
        (Some(ActionV1::Activate), _) => Some("Activate Fiend Artisan".into()),
        (Some(ActionV1::PayX { x }), _) => Some(format!("Choose X = {x} for Fiend Artisan")),
        (Some(ActionV1::Sacrifice), Action::ChooseCostTarget { candidate, .. }) => Some(format!(
            "Sacrifice {} to Fiend Artisan",
            handles.name(candidate)?
        )),
        (
            Some(ActionV1::ChooseCreature),
            Action::ChooseEffectTarget {
                target: TargetRefV1::Object { object },
                ..
            },
        ) => Some(format!(
            "Choose {} for Fiend Artisan",
            handles.name(object)?
        )),
        (
            Some(ActionV1::FinishSearch {
                selected_creature: true,
            }),
            _,
        ) => Some("Put the chosen creature onto the battlefield, then shuffle".into()),
        (
            Some(ActionV1::FinishSearch {
                selected_creature: false,
            }),
            _,
        ) => Some("Find no creature and shuffle".into()),
        (None | Some(ActionV1::Cast), _) => None,
        _ => return Err(Error::InvalidVisibleReference),
    })
}
