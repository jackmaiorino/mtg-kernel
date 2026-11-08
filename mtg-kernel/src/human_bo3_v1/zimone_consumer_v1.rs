//! Prompt-local references for Zimone's viewer-visible counter facts.
use super::{
    muldrotha_v71::{href, root, HumanCallRootV62},
    tinybones_v72::{HumanTinybonesRelationV72, Incarnations},
    visible::Handles,
    HumanCardRefV1, HumanDecisionErrorV1 as Error,
};
use crate::{
    optional_trigger_public_v1::FormV1,
    rl::{ActionSemanticV1 as Action, TargetRefV1},
    tinybones_public_v72::CountersV72,
    zimone_consumer_v1::ActionV1,
    zimone_public_v1::{AbilityKindV1, FactsV1, MeaningV1, StageV1},
};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanFactsV1 {
    meaning: MeaningV1,
    visible_printings: Vec<HumanCardRefV1>,
    battlefield: [Vec<HumanPermanentV1>; 2],
    abilities: Vec<HumanAbilityV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanPermanentV1 {
    card: HumanCardRefV1,
    form: FormV1,
    tapped: bool,
    summoning_sick: bool,
    counters: CountersV72,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanTargetV1 {
    original: HumanTinybonesRelationV72,
    current_original: Option<HumanCardRefV1>,
    currently_legal: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanAbilityV1 {
    kind: AbilityKindV1,
    stage: StageV1,
    position: u32,
    source: HumanCallRootV62,
    original_targets: Vec<HumanTargetV1>,
}

pub(super) fn project(
    facts: &FactsV1,
    handles: &mut Handles,
    roles: &mut Incarnations,
) -> Result<HumanFactsV1, Error> {
    let mut battlefield = [vec![], vec![]];
    for (rows, originals) in battlefield.iter_mut().zip(&facts.battlefield) {
        for card in originals {
            rows.push(HumanPermanentV1 {
                card: href(&card.card, handles)?,
                form: card.form.clone(),
                tapped: card.tapped,
                summoning_sick: card.summoning_sick,
                counters: card.counters.clone(),
            });
        }
    }
    let mut abilities = vec![];
    for row in &facts.abilities {
        let mut original_targets = vec![];
        for target in &row.original_targets {
            original_targets.push(HumanTargetV1 {
                original: roles.relation(&target.original)?,
                current_original: target
                    .current_original
                    .as_ref()
                    .map(|card| href(card, handles))
                    .transpose()?,
                currently_legal: target.currently_legal,
            });
        }
        abilities.push(HumanAbilityV1 {
            kind: row.kind,
            stage: row.stage,
            position: row.position,
            source: root(&row.source, handles)?,
            original_targets,
        });
    }
    Ok(HumanFactsV1 {
        meaning: facts.meaning.clone(),
        visible_printings: facts
            .visible_printings
            .iter()
            .map(|card| href(card, handles))
            .collect::<Result<_, _>>()?,
        battlefield,
        abilities,
    })
}

pub(super) fn label(
    facts: Option<&FactsV1>,
    action: &Action,
    handles: &Handles,
) -> Result<Option<String>, Error> {
    let annotated = crate::zimone_consumer_v1::action(facts, action)
        .map_err(|_| Error::InvalidVisibleReference)?;
    let Some(annotated) = annotated else {
        return Ok(None);
    };
    let ActionV1::TargetChoice {
        kind,
        finish,
        selected_count,
        ..
    } = annotated
    else {
        return Ok(Some("Activate Zimone (green, blue, tap)".into()));
    };
    let purpose = match kind {
        AbilityKindV1::CombatPutPlusOne => "Zimone's combat +1/+1 counters",
        AbilityKindV1::ActivateDoubleEveryKind => "Zimone's counter doubling",
    };
    if finish {
        return Ok(Some(format!(
            "Finish choosing {selected_count} targets for {purpose}"
        )));
    }
    let target = match action {
        Action::ChooseTarget { target, .. } | Action::ChooseEffectTarget { target, .. } => target,
        _ => return Err(Error::InvalidVisibleReference),
    };
    let TargetRefV1::Object { object } = target else {
        return Err(Error::InvalidVisibleReference);
    };
    Ok(Some(format!(
        "Choose {} for {purpose}",
        handles.name(object)?
    )))
}
