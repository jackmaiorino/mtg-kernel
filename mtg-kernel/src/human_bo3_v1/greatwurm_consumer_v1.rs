//! Fixed-seat original Greatwurm receipts with prompt-local visible references.
use super::{
    muldrotha_v71::{href, stable},
    tinybones_v72::{HumanTinybonesRelationV72, Incarnations},
    visible::Handles,
    HumanCardRefV1, HumanDecisionErrorV1 as Error,
};
use crate::{
    effect::greatwurm_v1::CounterKindV1,
    greatwurm_consumer_v1::ActionV1,
    greatwurm_public_v1::{
        ChoiceV1, CombatStageV1, FactsV1, GraveyardRouteV1, MeaningV1, UnitFactV1,
    },
    optional_trigger_public_v1::{IdentityV1, ReferenceV1},
    rl::ActionSemanticV1 as Action,
    state::Zone,
    tinybones_public_v72::IncarnationV72,
};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanFactsV1 {
    meaning: MeaningV1,
    graveyard: Vec<HumanAvailableV1>,
    casts: Vec<HumanCastV1>,
    chooser: Option<HumanChoiceV1>,
    paid: Vec<HumanPaidV1>,
    combat: Vec<HumanCombatV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanAvailableV1 {
    source: HumanCardRefV1,
    printed_counter_cost_available: bool,
    castable_now: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanCastV1 {
    source: HumanCardRefV1,
    stack_position: u32,
    origin_zone: Option<Zone>,
    announcing: bool,
    uncast_copy: bool,
    finalized_graveyard_route: Option<GraveyardRouteV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
enum HumanChoiceV1 {
    Route {
        source: HumanCardRefV1,
        routes: [GraveyardRouteV1; 2],
    },
    Unit {
        source: HumanCardRefV1,
        selected: Vec<HumanUnitV1>,
        legal: Vec<HumanUnitV1>,
        remaining: u8,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanUnitV1 {
    permanent: HumanCardRefV1,
    kind: CounterKindV1,
    before_payment: u32,
    available_after_selected: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanPaidV1 {
    original_spell: HumanTinybonesRelationV72,
    removals: Vec<HumanDebitV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanDebitV1 {
    original: HumanTinybonesRelationV72,
    identity: IdentityV1,
    kind: CounterKindV1,
    original_count: u32,
    remaining_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanCombatV1 {
    original_observer: HumanTinybonesRelationV72,
    current_original_observer: Option<HumanCardRefV1>,
    original_creature: HumanTinybonesRelationV72,
    current_original_creature: Option<HumanCardRefV1>,
    final_combat_damage: u32,
    stage: CombatStageV1,
}

fn original(
    card: &ReferenceV1,
    roles: &mut Incarnations,
) -> Result<HumanTinybonesRelationV72, Error> {
    roles.relation(&IncarnationV72 {
        object: card.object,
        generation: card.generation,
        owner: card.owner,
        controller: card.controller,
        zone: card.zone,
    })
}

fn units(rows: &[UnitFactV1], handles: &mut Handles) -> Result<Vec<HumanUnitV1>, Error> {
    rows.iter()
        .map(|row| {
            Ok(HumanUnitV1 {
                permanent: href(&row.permanent, handles)?,
                kind: row.kind,
                before_payment: row.before_payment,
                available_after_selected: row.available_after_selected,
            })
        })
        .collect()
}

pub(super) fn project(
    facts: &FactsV1,
    handles: &mut Handles,
    roles: &mut Incarnations,
) -> Result<HumanFactsV1, Error> {
    let graveyard = facts
        .graveyard
        .iter()
        .map(|row| {
            Ok(HumanAvailableV1 {
                source: href(&row.source, handles)?,
                printed_counter_cost_available: row.printed_counter_cost_available,
                castable_now: row.castable_now,
            })
        })
        .collect::<Result<_, Error>>()?;
    let casts = facts
        .casts
        .iter()
        .map(|row| {
            Ok(HumanCastV1 {
                source: href(&row.source, handles)?,
                stack_position: row.stack_position,
                origin_zone: row.origin_zone,
                announcing: row.announcing,
                uncast_copy: row.uncast_copy,
                finalized_graveyard_route: row.finalized_graveyard_route,
            })
        })
        .collect::<Result<_, Error>>()?;
    let chooser = facts
        .chooser
        .as_ref()
        .map(|row| -> Result<_, Error> {
            match row {
                ChoiceV1::Route { source, routes } => Ok(HumanChoiceV1::Route {
                    source: href(source, handles)?,
                    routes: *routes,
                }),
                ChoiceV1::Unit {
                    source,
                    selected,
                    legal,
                    remaining,
                } => Ok(HumanChoiceV1::Unit {
                    source: href(source, handles)?,
                    selected: units(selected, handles)?,
                    legal: units(legal, handles)?,
                    remaining: *remaining,
                }),
            }
        })
        .transpose()?;
    let paid = facts
        .paid
        .iter()
        .map(|row| {
            Ok(HumanPaidV1 {
                original_spell: roles.relation(&row.original_spell)?,
                removals: row
                    .removals
                    .iter()
                    .map(|debit| {
                        Ok(HumanDebitV1 {
                            original: roles.relation(&debit.original)?,
                            identity: debit.identity.clone(),
                            kind: debit.kind,
                            original_count: debit.original_count,
                            remaining_count: debit.remaining_count,
                        })
                    })
                    .collect::<Result<_, Error>>()?,
            })
        })
        .collect::<Result<_, Error>>()?;
    let combat = facts
        .combat
        .iter()
        .map(|row| {
            Ok(HumanCombatV1 {
                original_observer: original(&row.observer.original_source, roles)?,
                current_original_observer: row
                    .observer
                    .current_original_source
                    .as_ref()
                    .map(|card| href(card, handles))
                    .transpose()?,
                original_creature: roles.relation(&row.original_creature)?,
                current_original_creature: row
                    .current_original_creature
                    .as_ref()
                    .map(|card| href(card, handles))
                    .transpose()?,
                final_combat_damage: row.final_combat_damage,
                stage: row.stage.clone(),
            })
        })
        .collect::<Result<_, Error>>()?;
    Ok(HumanFactsV1 {
        meaning: facts.meaning.clone(),
        graveyard,
        casts,
        chooser,
        paid,
        combat,
    })
}

fn kind_name(kind: CounterKindV1) -> &'static str {
    match kind {
        CounterKindV1::PlusOne => "+1/+1",
        CounterKindV1::MinusOne => "-1/-1",
        CounterKindV1::MinusToughness => "-0/-1",
        CounterKindV1::Stun => "stun",
        CounterKindV1::Lore => "lore",
        CounterKindV1::Oil => "oil",
        CounterKindV1::Charge => "charge",
        CounterKindV1::Net => "net",
        CounterKindV1::Bait => "bait",
        CounterKindV1::Soul => "soul",
        CounterKindV1::Incubation => "incubation",
        CounterKindV1::Revival => "revival",
        CounterKindV1::Fellowship => "fellowship",
        CounterKindV1::Loyalty => "loyalty",
        CounterKindV1::Lifelink => "lifelink",
        CounterKindV1::Time => "time",
        CounterKindV1::Finality => "finality",
        CounterKindV1::Stash => "stash",
    }
}

pub(super) fn label(
    facts: Option<&FactsV1>,
    action: &Action,
    handles: &Handles,
) -> Result<Option<String>, Error> {
    let annotated = crate::greatwurm_consumer_v1::action(facts, action)
        .map_err(|_| Error::InvalidVisibleReference)?;
    Ok(match annotated {
        None => None,
        Some(ActionV1::ChooseGraveyardRoute { route, .. }) => Some(
            match route {
                GraveyardRouteV1::GrantedPermission => {
                    "Cast Quilled Greatwurm using the granted graveyard permission"
                }
                GraveyardRouteV1::PrintedSixCounterCost => {
                    "Cast Quilled Greatwurm by also removing six counters"
                }
            }
            .into(),
        ),
        Some(ActionV1::SelectCounter { index, kind, .. }) => {
            let Some(ChoiceV1::Unit { legal, .. }) = facts.and_then(|facts| facts.chooser.as_ref())
            else {
                return Err(Error::InvalidVisibleReference);
            };
            let unit = legal
                .get(usize::from(index))
                .ok_or(Error::InvalidVisibleReference)?;
            Some(format!(
                "Select a {} counter on {}",
                kind_name(kind),
                handles.name(&stable(&unit.permanent)?)?
            ))
        }
    })
}
