use super::{
    muldrotha_v71::href,
    tinybones_v72::{HumanTinybonesRelationV72, Incarnations},
    visible::Handles,
    HumanCardRefV1, HumanDecisionErrorV1 as Error, PlayerSeatV1,
};
use crate::{
    edict_consumer_v1::ActionV1,
    edict_public_v1::{ChoiceStageV1, FactsV1, MeaningV1, PaymentV1, QuoteV1},
    engine::CastMode,
    optional_trigger_public_v1::{IdentityV1, ReferenceV1},
    rl::{ActionSemanticV1 as Action, TargetRefV1},
    state::Zone,
};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct HumanFactsV1 {
    meaning: MeaningV1,
    effective_creatures_now: u32,
    alternate_condition_now: bool,
    casts: Vec<HumanCastV1>,
    chooser_cast_quotes: Option<HumanQuotesV1>,
    chooser_sacrifice_or_order: Option<HumanChoiceV1>,
    resolved: Vec<HumanOutcomeV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanCastV1 {
    stack_position: u32,
    card: HumanCardRefV1,
    announcing: bool,
    resolving: bool,
    uncast_copy: bool,
    origin_zone: Option<Zone>,
    payment: Option<PaymentV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanQuotesV1 {
    card: HumanCardRefV1,
    modes: Vec<QuoteV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanChoiceV1 {
    source: HumanCardRefV1,
    active: PlayerSeatV1,
    chooser: PlayerSeatV1,
    stage: ChoiceStageV1,
    required: u16,
    selected: Vec<HumanCardRefV1>,
    legal: Vec<HumanCardRefV1>,
    completed_controller_choices: [Vec<HumanCardRefV1>; 2],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanVictimV1 {
    original: HumanTinybonesRelationV72,
    identity: IdentityV1,
    committed_destination: Option<Zone>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanOutcomeV1 {
    original_spell: HumanTinybonesRelationV72,
    copied_spell: bool,
    active: PlayerSeatV1,
    payment: Option<PaymentV1>,
    sacrifices_in_apnap_order: Vec<HumanVictimV1>,
    actual_graveyard_order: [Vec<HumanTinybonesRelationV72>; 2],
}

pub(super) fn project(
    facts: &FactsV1,
    handles: &mut Handles,
    roles: &mut Incarnations,
) -> Result<HumanFactsV1, Error> {
    let casts = facts.casts.iter().map(|row| {
        Ok(HumanCastV1 {
            stack_position: row.stack_position,
            card: href(&row.card, handles)?,
            announcing: row.announcing,
            resolving: row.resolving,
            uncast_copy: row.uncast_copy,
            origin_zone: row.origin_zone,
            payment: row.payment,
        })
    }).collect::<Result<_, Error>>()?;
    let chooser_cast_quotes = facts.chooser_cast_quotes.as_ref().map(|row| {
        Ok::<_, Error>(HumanQuotesV1 {
            card: href(&row.card, handles)?,
            modes: row.modes.clone(),
        })
    }).transpose()?;
    let chooser_sacrifice_or_order = facts.chooser_sacrifice_or_order.as_ref().map(|row| {
        let source = href(&row.source, handles)?;
        let mut cards = |list: &[ReferenceV1]| -> Result<Vec<_>, Error> {
            list.iter().map(|card| href(card, handles)).collect()
        };
        Ok::<_, Error>(HumanChoiceV1 {
            source,
            active: row.active,
            chooser: row.chooser,
            stage: row.stage,
            required: row.required,
            selected: cards(&row.selected)?,
            legal: cards(&row.legal)?,
            completed_controller_choices: [
                cards(&row.completed_controller_choices[0])?,
                cards(&row.completed_controller_choices[1])?,
            ],
        })
    }).transpose()?;
    let resolved = facts.resolved.iter().map(|row| {
        let original_spell = roles.relation(&row.original_spell)?;
        let sacrifices_in_apnap_order = row.sacrifices_in_apnap_order.iter().map(|victim| {
            Ok(HumanVictimV1 {
                original: roles.relation(&victim.original)?,
                identity: victim.identity.clone(),
                committed_destination: victim.committed_destination,
            })
        }).collect::<Result<_, Error>>()?;
        let mut actual_graveyard_order = [vec![], vec![]];
        for (output, source) in actual_graveyard_order.iter_mut().zip(&row.actual_graveyard_order) {
            for entry in source {
                output.push(roles.relation(entry)?);
            }
        }
        Ok(HumanOutcomeV1 {
            original_spell,
            copied_spell: row.copied_spell,
            active: row.active,
            payment: row.payment,
            sacrifices_in_apnap_order,
            actual_graveyard_order,
        })
    }).collect::<Result<_, Error>>()?;
    Ok(HumanFactsV1 {
        meaning: facts.meaning.clone(),
        effective_creatures_now: facts.effective_creatures_now,
        alternate_condition_now: facts.alternate_condition_now,
        casts,
        chooser_cast_quotes,
        chooser_sacrifice_or_order,
        resolved,
    })
}

pub(super) fn label(
    facts: Option<&FactsV1>,
    action: &Action,
    handles: &Handles,
) -> Result<Option<String>, Error> {
    let annotated = crate::edict_consumer_v1::action(facts, action)
        .map_err(|_| Error::InvalidVisibleReference)?;
    Ok(match (annotated, action) {
        (Some(ActionV1::ChooseCastPayment { mode, payable_mana_now }), _) => {
            let route = match mode {
                CastMode::Normal => "normal",
                CastMode::Alternative => "alternative",
            };
            Some(format!("Pay {payable_mana_now} mana for Blasphemous Edict's {route} cost"))
        }
        (Some(ActionV1::ChoosePermanent { stage, .. }), Action::ChooseEffectTarget {
            target: TargetRefV1::Object { object }, ..
        }) => {
            let name = handles.name(object)?;
            Some(match stage {
                ChoiceStageV1::SacrificeSelection => format!("Choose {name} to sacrifice"),
                ChoiceStageV1::PhysicalOwnerGraveyardOrder => format!("Put {name} next in your graveyard order"),
            })
        }
        (None, _) => None,
        _ => return Err(Error::InvalidVisibleReference),
    })
}
