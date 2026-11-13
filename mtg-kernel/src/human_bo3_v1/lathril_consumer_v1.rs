use super::{
    muldrotha_v71::href,
    tinybones_v72::{HumanTinybonesRelationV72, Incarnations},
    visible::Handles,
    HumanCardRefV1, HumanDecisionErrorV1 as Error, PlayerSeatV1,
};
use crate::{
    lathril_consumer_v1::ActionV1,
    lathril_public_v1::{CombatStageV1, DrainStageV1, FactsV1, MeaningV1},
    optional_trigger_public_v1::{FormV1, RootV1},
    rl::ActionSemanticV1 as Action,
    state::{CastMethodV4, Zone},
    tinybones_public_v72::{CountersV72, IncarnationV72},
};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanFactsV1 {
    meaning: MeaningV1,
    battlefield: Vec<HumanCurrentV1>,
    casts: Vec<HumanCastV1>,
    combat: Vec<HumanCombatV1>,
    payment: Option<HumanPaymentV1>,
    drains: Vec<HumanDrainV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanCurrentV1 {
    card: HumanCardRefV1,
    token: bool,
    tapped: bool,
    summoning_sick: bool,
    counters: CountersV72,
    form: FormV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanCastV1 {
    stack_position: u32,
    spell: HumanCardRefV1,
    announcing: bool,
    uncast_copy: bool,
    origin_zone: Option<Zone>,
    finalized_method: Option<CastMethodV4>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanSourceV1 {
    original: HumanTinybonesRelationV72,
    current_original: Option<HumanCardRefV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
enum HumanCombatStageV1 {
    Held {
        position: u32,
    },
    Published {
        stack_position: u32,
    },
    Resolved {
        tokens: Vec<HumanTinybonesRelationV72>,
    },
    Countered,
    RemovedWithoutResolution,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanCombatV1 {
    source: HumanSourceV1,
    damaged_player: PlayerSeatV1,
    final_damage: u32,
    stage: HumanCombatStageV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanPaymentV1 {
    source: HumanCardRefV1,
    chooser: PlayerSeatV1,
    selected: Vec<HumanCardRefV1>,
    remaining: u8,
    candidates: Vec<HumanCardRefV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanDrainV1 {
    source: HumanSourceV1,
    original_paid_elves: Vec<HumanTinybonesRelationV72>,
    stage: DrainStageV1,
}

fn source(
    root: &RootV1,
    handles: &mut Handles,
    roles: &mut Incarnations,
) -> Result<HumanSourceV1, Error> {
    let source = &root.original_source;
    Ok(HumanSourceV1 {
        original: roles.relation(&IncarnationV72 {
            object: source.object,
            generation: source.generation,
            owner: source.owner,
            controller: source.controller,
            zone: source.zone,
        })?,
        current_original: root
            .current_original_source
            .as_ref()
            .map(|card| href(card, handles))
            .transpose()?,
    })
}

pub(super) fn project(
    facts: &FactsV1,
    handles: &mut Handles,
    roles: &mut Incarnations,
) -> Result<HumanFactsV1, Error> {
    let battlefield = facts
        .battlefield
        .iter()
        .map(|row| {
            Ok(HumanCurrentV1 {
                card: href(&row.card, handles)?,
                token: row.token,
                tapped: row.tapped,
                summoning_sick: row.summoning_sick,
                counters: row.counters.clone(),
                form: row.form.clone(),
            })
        })
        .collect::<Result<_, Error>>()?;
    let casts = facts
        .casts
        .iter()
        .map(|row| {
            Ok(HumanCastV1 {
                stack_position: row.stack_position,
                spell: href(&row.spell, handles)?,
                announcing: row.announcing,
                uncast_copy: row.uncast_copy,
                origin_zone: row.origin_zone,
                finalized_method: row.finalized_method,
            })
        })
        .collect::<Result<_, Error>>()?;
    let combat = facts
        .combat
        .iter()
        .map(|row| {
            let source = source(&row.source, handles, roles)?;
            let stage = match &row.stage {
                CombatStageV1::Held { position } => HumanCombatStageV1::Held {
                    position: *position,
                },
                CombatStageV1::Published { stack_position } => HumanCombatStageV1::Published {
                    stack_position: *stack_position,
                },
                CombatStageV1::Resolved { tokens } => HumanCombatStageV1::Resolved {
                    tokens: tokens
                        .iter()
                        .map(|token| roles.relation(token))
                        .collect::<Result<_, _>>()?,
                },
                CombatStageV1::Countered => HumanCombatStageV1::Countered,
                CombatStageV1::RemovedWithoutResolution => {
                    HumanCombatStageV1::RemovedWithoutResolution
                }
            };
            Ok(HumanCombatV1 {
                source,
                damaged_player: row.damaged_player,
                final_damage: row.final_damage,
                stage,
            })
        })
        .collect::<Result<_, Error>>()?;
    let payment = facts
        .payment
        .as_ref()
        .map(|row| {
            Ok::<_, Error>(HumanPaymentV1 {
                source: href(&row.source, handles)?,
                chooser: row.chooser,
                selected: row
                    .selected
                    .iter()
                    .map(|card| href(card, handles))
                    .collect::<Result<_, _>>()?,
                remaining: row.remaining,
                candidates: row
                    .candidates
                    .iter()
                    .map(|card| href(card, handles))
                    .collect::<Result<_, _>>()?,
            })
        })
        .transpose()?;
    let drains = facts
        .drains
        .iter()
        .map(|row| {
            Ok(HumanDrainV1 {
                source: source(&row.source, handles, roles)?,
                original_paid_elves: row
                    .original_paid_elves
                    .iter()
                    .map(|elf| roles.relation(elf))
                    .collect::<Result<_, _>>()?,
                stage: row.stage.clone(),
            })
        })
        .collect::<Result<_, Error>>()?;
    Ok(HumanFactsV1 {
        meaning: facts.meaning.clone(),
        battlefield,
        casts,
        combat,
        payment,
        drains,
    })
}

pub(super) fn label(
    facts: Option<&FactsV1>,
    action: &Action,
    handles: &Handles,
) -> Result<Option<String>, Error> {
    let annotated = crate::lathril_consumer_v1::action(facts, action)
        .map_err(|_| Error::InvalidVisibleReference)?;
    Ok(match (annotated, action) {
        (Some(ActionV1::ActivateDrain { other_elves }), _) => Some(format!(
            "Activate Lathril by tapping it and {other_elves} other Elves"
        )),
        (Some(ActionV1::ChooseElfPayment { .. }), Action::ChooseCostTarget { candidate, .. }) => {
            Some(format!(
                "Tap {} to pay for Lathril",
                handles.name(candidate)?
            ))
        }
        (None, _) => None,
        _ => return Err(Error::InvalidVisibleReference),
    })
}
