//! Etali's native batch and cast permissions in prompt-local handles.
use super::{
    muldrotha_v71::{href, root, HumanCallRootV62},
    visible::Handles,
    HumanCardRefV1, HumanDecisionErrorV1 as Error,
};
use crate::{
    etali_primal_storm_consumer_v1::{self as consumer, ActionV1},
    etali_primal_storm_public_v1::{ChoiceV1, FactsV1, MeaningV1},
    optional_trigger_public_v1::ReferenceV1,
    rl::{ActionSemanticV1 as Action, TargetRefV1},
    state::CastMethodV4,
};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanFactsV1 {
    meaning: MeaningV1,
    held: Vec<HumanCallRootV62>,
    attacks: Vec<HumanAttackV1>,
    chooser: Option<HumanChoiceV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanAttackV1 {
    source: HumanCallRootV62,
    stack_position: Option<u32>,
    batch_started: bool,
    original_exiles: Vec<HumanCardRefV1>,
    actual_casts: Vec<HumanCastV1>,
    finished: Option<bool>,
    countered: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanCastV1 {
    original_exile: HumanCardRefV1,
    spell: HumanCardRefV1,
    method: CastMethodV4,
    x: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
enum HumanChoiceV1 {
    CastOrDecline { source: HumanCardRefV1, legal: Vec<HumanCardRefV1> },
    ChildCast {
        source: HumanCardRefV1,
        original_exile: HumanCardRefV1,
        placeholder: HumanCardRefV1,
    },
}

fn refs(cards: &[ReferenceV1], handles: &mut Handles) -> Result<Vec<HumanCardRefV1>, Error> {
    cards.iter().map(|card| href(card, handles)).collect()
}

pub(super) fn project(facts: &FactsV1, handles: &mut Handles) -> Result<HumanFactsV1, Error> {
    let held = facts.held.iter().map(|source| root(source, handles)).collect::<Result<_, _>>()?;
    let attacks = facts.attacks.iter().map(|row| {
        let actual_casts = row.actual_casts.iter().map(|cast| {
            Ok(HumanCastV1 {
                original_exile: href(&cast.original_exile, handles)?,
                spell: href(&cast.spell, handles)?,
                method: cast.method,
                x: cast.x,
            })
        }).collect::<Result<_, Error>>()?;
        Ok(HumanAttackV1 {
            source: root(&row.source, handles)?, stack_position: row.stack_position,
            batch_started: row.batch_started, original_exiles: refs(&row.original_exiles, handles)?,
            actual_casts, finished: row.finished, countered: row.countered,
        })
    }).collect::<Result<_, Error>>()?;
    let chooser = facts.chooser.as_ref().map(|choice| {
        Ok::<_, Error>(match choice {
            ChoiceV1::CastOrDecline { source, legal } => HumanChoiceV1::CastOrDecline {
                source: href(source, handles)?, legal: refs(legal, handles)?,
            },
            ChoiceV1::ChildCast { source, original_exile, placeholder } =>
                HumanChoiceV1::ChildCast {
                    source: href(source, handles)?, original_exile: href(original_exile, handles)?,
                    placeholder: href(placeholder, handles)?,
                },
        })
    }).transpose()?;
    Ok(HumanFactsV1 { meaning: facts.meaning.clone(), held, attacks, chooser })
}

pub(super) fn label(
    facts: Option<&FactsV1>,
    action: &Action,
    handles: &Handles,
) -> Result<Option<String>, Error> {
    Ok(match (consumer::action(facts, action), action) {
        (Some(ActionV1::CastOriginalExile), Action::ChooseEffectTarget {
            target: TargetRefV1::Object { object }, ..
        }) => Some(format!("Cast {} without paying its mana cost", handles.name(object)?)),
        (Some(ActionV1::DeclineRemaining), _) => Some("Leave the remaining cards in exile".into()),
        (None | Some(ActionV1::CastEtali | ActionV1::ChildCastStep), _) => None,
        _ => return Err(Error::InvalidVisibleReference),
    })
}
