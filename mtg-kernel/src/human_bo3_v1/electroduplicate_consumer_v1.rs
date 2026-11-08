//! Prompt-local copied-token and original cast-target facts.
use super::{muldrotha_v71::href, tinybones_v72::{HumanTinybonesRelationV72, Incarnations},
    visible::Handles, HumanCardRefV1, HumanDecisionErrorV1 as Error};
use crate::{electroduplicate_public_v1::{FactsV1, MeaningV1, CastStageV1},
    optional_trigger_public_v1::FormV1, tinybones_public_v72::CountersV72,
    rl::{ActionSemanticV1 as Action, TargetRefV1}, state::{CastMethodV4, Zone}};
use serde::Serialize;
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanFactsV1 { meaning: MeaningV1, casts: Vec<HumanCastV1>, copied_tokens: Vec<HumanTokenV1> }
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanCastV1 { stack_position: u32, spell: HumanCardRefV1, stage: CastStageV1,
    origin_zone: Option<Zone>, finalized_method: Option<CastMethodV4>, target: Option<HumanTargetV1> }
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanTargetV1 { original: HumanTinybonesRelationV72, current_original: Option<HumanCardRefV1>, currently_legal: bool }
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanTokenV1 { card: HumanCardRefV1, form: FormV1, counters: CountersV72,
    copiable_haste_and_each_end_step_sacrifice_instances: u32, tapped: bool, summoning_sick: bool }

pub(super) fn project(facts: &FactsV1, handles: &mut Handles, roles: &mut Incarnations) -> Result<HumanFactsV1, Error> {
    let casts = facts.casts.iter().map(|cast| {
        let target = cast.target.as_ref().map(|target| Ok::<_, Error>(HumanTargetV1 {
            original: roles.relation(&target.original)?,
            current_original: target.current_original.as_ref().map(|card| href(card, handles)).transpose()?,
            currently_legal: target.currently_legal,
        })).transpose()?;
        Ok(HumanCastV1 { stack_position: cast.stack_position, spell: href(&cast.spell, handles)?,
            stage: cast.stage, origin_zone: cast.origin_zone, finalized_method: cast.finalized_method, target })
    }).collect::<Result<_, Error>>()?;
    let copied_tokens = facts.copied_tokens.iter().map(|token| Ok(HumanTokenV1 {
        card: href(&token.card, handles)?, form: token.form.clone(), counters: token.counters.clone(),
        copiable_haste_and_each_end_step_sacrifice_instances: token.copiable_haste_and_each_end_step_sacrifice_instances,
        tapped: token.tapped, summoning_sick: token.summoning_sick,
    })).collect::<Result<_, Error>>()?;
    Ok(HumanFactsV1 { meaning: facts.meaning.clone(), casts, copied_tokens })
}

pub(super) fn label(facts: Option<&FactsV1>, action: &Action, handles: &Handles) -> Result<Option<String>, Error> {
    if crate::electroduplicate_consumer_v1::action(facts, action).map_err(|_| Error::InvalidVisibleReference)?.is_none() {
        return Ok(None);
    }
    let Action::ChooseTarget { target: TargetRefV1::Object { object }, .. } = action else { return Err(Error::InvalidVisibleReference) };
    Ok(Some(format!("Create an Electroduplicate copy of {}", handles.name(object)?)))
}
