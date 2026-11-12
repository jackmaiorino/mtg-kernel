//! Prompt-local original attacks, public graveyard names and flashback routes.
use super::{
    muldrotha_v71::{href, root, HumanCallRootV62},
    tinybones_v72::{HumanTinybonesRelationV72, Incarnations},
    visible::Handles,
    HumanCardRefV1, HumanDecisionErrorV1 as Error, PlayerSeatV1,
};
use crate::{
    omniscience_public_v1::ManaCostV1,
    optional_trigger_public_v1::{FormV1, IdentityV1, ReferenceV1},
    rl::{ActionSemanticV1 as Action, TargetRefV1},
    sphinx_lore_consumer_v1::ActionV1,
    sphinx_lore_public_v1::{CastStageV1, FactsV1, MeaningV1, OptionV1, RouteV1, StageV1},
    state::CastMethodV4,
    tinybones_public_v72::{CountersV72, IncarnationV72},
};
use serde::Serialize;
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanFactsV1 {
    meaning: MeaningV1,
    battlefield: Vec<HumanPermanentV1>,
    attacks: Vec<HumanAttackV1>,
    grants: Vec<HumanGrantV1>,
    casts: Vec<HumanCastV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanPermanentV1 {
    card: HumanCardRefV1,
    form: FormV1,
    counters: CountersV72,
    tapped: bool,
    summoning_sick: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanOriginalV1 {
    relation: HumanTinybonesRelationV72,
    printed_name: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanTargetV1 {
    original: HumanOriginalV1,
    current_original: Option<HumanCardRefV1>,
    currently_legal: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanAttackV1 {
    source: HumanCallRootV62,
    turn: u32,
    pending: Option<(StageV1, u32)>,
    original_target: Option<HumanTargetV1>,
    installed_grant: Option<u32>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanGrantV1 {
    attack: u32,
    holder: PlayerSeatV1,
    original_card: HumanOriginalV1,
    current_original_graveyard: Option<HumanCardRefV1>,
    until_cleanup_active: bool,
    original_incarnation_has_active_grant: bool,
    mana_cost: ManaCostV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanCastV1 {
    spell: HumanCardRefV1,
    original_graveyard: HumanTinybonesRelationV72,
    stage: CastStageV1,
    native_stack_position: Option<u32>,
    routes: Vec<OptionV1>,
    selected: Option<u16>,
    finalized_method: Option<CastMethodV4>,
}
fn original(card: &ReferenceV1, roles: &mut Incarnations) -> Result<HumanOriginalV1, Error> {
    let IdentityV1::Known { card_id } = card.identity else {
        return Err(Error::InvalidVisibleReference);
    };
    if usize::from(card_id) >= crate::card_def::CARD_DEFS.len() {
        return Err(Error::InvalidVisibleReference);
    }
    Ok(HumanOriginalV1 {
        relation: roles.relation(&IncarnationV72 {
            object: card.object,
            generation: card.generation,
            owner: card.owner,
            controller: card.controller,
            zone: card.zone,
        })?,
        printed_name: crate::rl::card_name(card_id),
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
        .map(|card| {
            Ok(HumanPermanentV1 {
                card: href(&card.card, handles)?,
                form: card.form.clone(),
                counters: card.counters.clone(),
                tapped: card.tapped,
                summoning_sick: card.summoning_sick,
            })
        })
        .collect::<Result<_, Error>>()?;
    let attacks = facts
        .attacks
        .iter()
        .map(|attack| {
            let original_target = attack
                .original_target
                .as_ref()
                .map(|target| {
                    Ok::<_, Error>(HumanTargetV1 {
                        original: original(&target.original, roles)?,
                        current_original: target
                            .current_original
                            .as_ref()
                            .map(|c| href(c, handles))
                            .transpose()?,
                        currently_legal: target.currently_legal,
                    })
                })
                .transpose()?;
            Ok(HumanAttackV1 {
                source: root(&attack.source, handles)?,
                turn: attack.turn,
                pending: attack.pending,
                original_target,
                installed_grant: attack.installed_grant,
            })
        })
        .collect::<Result<_, Error>>()?;
    let grants = facts
        .grants
        .iter()
        .map(|grant| {
            Ok(HumanGrantV1 {
                attack: grant.attack,
                holder: grant.holder,
                original_card: original(&grant.original_card, roles)?,
                current_original_graveyard: grant
                    .current_original_graveyard
                    .as_ref()
                    .map(|c| href(c, handles))
                    .transpose()?,
                until_cleanup_active: grant.until_cleanup_active,
                original_incarnation_has_active_grant: grant.original_incarnation_has_active_grant,
                mana_cost: grant.mana_cost.clone(),
            })
        })
        .collect::<Result<_, Error>>()?;
    let casts = facts
        .casts
        .iter()
        .map(|cast| {
            Ok(HumanCastV1 {
                spell: href(&cast.spell, handles)?,
                original_graveyard: roles.relation(&cast.original_graveyard)?,
                stage: cast.stage,
                native_stack_position: cast.native_stack_position,
                routes: cast.routes.clone(),
                selected: cast.selected,
                finalized_method: cast.finalized_method,
            })
        })
        .collect::<Result<_, Error>>()?;
    Ok(HumanFactsV1 {
        meaning: facts.meaning.clone(),
        battlefield,
        attacks,
        grants,
        casts,
    })
}
pub(super) fn label(
    facts: Option<&FactsV1>,
    action: &Action,
    handles: &Handles,
) -> Result<Option<String>, Error> {
    let typed = crate::sphinx_lore_consumer_v1::action(facts, action)
        .map_err(|_| Error::InvalidVisibleReference)?;
    Ok(match (typed,action) {
        (Some(ActionV1::ChooseAttackTarget),Action::ChooseTarget {target:TargetRefV1::Object {object},..}) =>
            Some(format!("Give {} flashback equal to its mana cost until end of turn",handles.name(object)?)),
        (Some(ActionV1::ChooseCastRoute {route,..}),_) => Some(match route {
            RouteV1::Normal => "Pay the spell's ordinary costs",
            RouteV1::PrintedFlashback => "Pay the printed flashback cost; exile the spell when it leaves the stack",
            RouteV1::Escape => "Pay the printed escape cost",
            RouteV1::Granted {..} => "Use Sphinx's granted flashback cost equal to the mana cost; exile the spell when it leaves the stack",
        }.into()),
        (None,_) => None,
        _ => return Err(Error::InvalidVisibleReference),
    })
}
