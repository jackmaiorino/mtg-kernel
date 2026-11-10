//! Prompt-local Kykar sources, original target relations and return schedules.
use super::{
    muldrotha_v71::{href, root, HumanCallRootV62},
    tinybones_v72::{HumanTinybonesRelationV72, Incarnations},
    visible::Handles,
    HumanCardRefV1, HumanDecisionErrorV1 as Error, PlayerSeatV1,
};
use crate::{
    kykar_consumer_v1::ActionV1,
    kykar_public_v1::{FactsV1, MeaningV1, ModeV1, StageV1},
    optional_trigger_public_v1::FormV1,
    rl::{ActionSemanticV1 as Action, TargetRefV1},
    tinybones_public_v72::CountersV72,
};
use serde::Serialize;
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanFactsV1 {
    meaning: MeaningV1,
    battlefield: Vec<HumanPermanentV1>,
    abilities: Vec<HumanAbilityV1>,
    schedules: Vec<HumanScheduleV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanPermanentV1 {
    card: HumanCardRefV1,
    form: FormV1,
    counters: CountersV72,
    is_token: bool,
    tapped: bool,
    summoning_sick: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanTargetV1 {
    original: HumanTinybonesRelationV72,
    current_original: Option<HumanCardRefV1>,
    currently_legal: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanAbilityV1 {
    source: HumanCallRootV62,
    mode: ModeV1,
    stage: StageV1,
    position: u32,
    target: Option<HumanTargetV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanScheduleV1 {
    source: HumanCallRootV62,
    original_exile: HumanTinybonesRelationV72,
    physical_owner: PlayerSeatV1,
    current_original_exile: Option<HumanCardRefV1>,
    collected_at_first_end_step: bool,
}
pub(super) fn project(
    facts: &FactsV1,
    handles: &mut Handles,
    roles: &mut Incarnations,
) -> Result<HumanFactsV1, Error> {
    let battlefield = facts
        .battlefield
        .iter()
        .map(|card| {
            Ok(HumanPermanentV1 {
                card: href(&card.card, handles)?,
                form: card.form.clone(),
                counters: card.counters.clone(),
                is_token: card.is_token,
                tapped: card.tapped,
                summoning_sick: card.summoning_sick,
            })
        })
        .collect::<Result<_, Error>>()?;
    let abilities = facts
        .abilities
        .iter()
        .map(|ability| {
            let target = ability
                .target
                .as_ref()
                .map(|target| {
                    Ok::<_, Error>(HumanTargetV1 {
                        original: roles.relation(&target.original)?,
                        current_original: target
                            .current_original
                            .as_ref()
                            .map(|card| href(card, handles))
                            .transpose()?,
                        currently_legal: target.currently_legal,
                    })
                })
                .transpose()?;
            Ok(HumanAbilityV1 {
                source: root(&ability.source, handles)?,
                mode: ability.mode,
                stage: ability.stage,
                position: ability.position,
                target,
            })
        })
        .collect::<Result<_, Error>>()?;
    let schedules = facts
        .schedules
        .iter()
        .map(|row| {
            Ok(HumanScheduleV1 {
                source: root(&row.source, handles)?,
                original_exile: roles.relation(&row.original_exile)?,
                physical_owner: row.physical_owner,
                current_original_exile: row
                    .current_original_exile
                    .as_ref()
                    .map(|card| href(card, handles))
                    .transpose()?,
                collected_at_first_end_step: row.collected_at_first_end_step,
            })
        })
        .collect::<Result<_, Error>>()?;
    Ok(HumanFactsV1 {
        meaning: facts.meaning.clone(),
        battlefield,
        abilities,
        schedules,
    })
}
pub(super) fn label(
    facts: Option<&FactsV1>,
    action: &Action,
    handles: &Handles,
) -> Result<Option<String>, Error> {
    let typed = crate::kykar_consumer_v1::action(facts, action)
        .map_err(|_| Error::InvalidVisibleReference)?;
    Ok(match (typed, action) {
        (Some(ActionV1::ChooseExileMode), _) => Some("Exile another creature you control; return it under its owner's control at the next end step".into()),
        (Some(ActionV1::ChooseSpiritMode), _) => Some("Create a 1/1 white Spirit creature token with flying".into()),
        (Some(ActionV1::ChooseExileTarget), Action::ChooseTarget { target: TargetRefV1::Object { object }, .. }) => Some(format!("Exile {} with Kykar", handles.name(object)?)),
        (None, _) => None,
        _ => return Err(Error::InvalidVisibleReference),
    })
}
