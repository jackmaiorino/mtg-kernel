//! Prompt-local original targets, exact Aura hosts, and native mana facts.
use super::{
    muldrotha_v71::href,
    tinybones_v72::{HumanTinybonesRelationV72, Incarnations},
    visible::Handles,
    HumanCardRefV1, HumanDecisionErrorV1 as Error, PlayerSeatV1,
};
use crate::{
    moon_consumer_v1::ActionV1,
    moon_public_v1::{AttachmentKindV1, CastStageV1, FactsV1, MeaningV1},
    optional_trigger_public_v1::{FormV1, ReferenceV1},
    rl::{ActionSemanticV1 as Action, TargetRefV1},
    state::{CastMethodV4, Zone},
    tinybones_public_v72::CountersV72,
};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanFactsV1 {
    meaning: MeaningV1,
    casts: Vec<HumanCastV1>,
    hosts: Vec<HumanHostV1>,
    attachment_menu: Option<HumanAttachmentMenuV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanCastV1 {
    stack_position: u32,
    spell: HumanCardRefV1,
    stage: CastStageV1,
    origin_zone: Option<Zone>,
    finalized_method: Option<CastMethodV4>,
    target: Option<HumanTargetV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanTargetV1 {
    original: HumanTinybonesRelationV72,
    current_original: Option<HumanCardRefV1>,
    currently_legal: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanHostV1 {
    card: HumanCardRefV1,
    auras: Vec<HumanAuraV1>,
    form: FormV1,
    counters: CountersV72,
    tapped: bool,
    summoning_sick: bool,
    moon_sets_land: bool,
    moon_sets_colorless: bool,
    moon_grants_tap_for_colorless: bool,
    granted_mana_available_now: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanAuraV1 {
    card: HumanCardRefV1,
    token: bool,
    layer_order: Option<usize>,
    supplies_current_override: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanAttachmentMenuV1 {
    kind: AttachmentKindV1,
    chooser: PlayerSeatV1,
    aura: HumanCardRefV1,
    selected_hosts: Vec<HumanCardRefV1>,
    total_hosts: u32,
    legal_hosts: Vec<HumanCardRefV1>,
}

fn refs(cards: &[ReferenceV1], handles: &mut Handles) -> Result<Vec<HumanCardRefV1>, Error> {
    cards.iter().map(|card| href(card, handles)).collect()
}

pub(super) fn project(
    facts: &FactsV1,
    handles: &mut Handles,
    roles: &mut Incarnations,
) -> Result<HumanFactsV1, Error> {
    let casts = facts
        .casts
        .iter()
        .map(|cast| {
            let target = cast
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
            Ok(HumanCastV1 {
                stack_position: cast.stack_position,
                spell: href(&cast.spell, handles)?,
                stage: cast.stage,
                origin_zone: cast.origin_zone,
                finalized_method: cast.finalized_method,
                target,
            })
        })
        .collect::<Result<_, Error>>()?;
    let mut hosts = vec![];
    for host in &facts.hosts {
        let mut timestamps: Vec<_> = host
            .auras
            .iter()
            .filter_map(|aura| aura.layer_timestamp)
            .collect();
        timestamps.sort_unstable();
        timestamps.dedup();
        let auras = host
            .auras
            .iter()
            .map(|aura| {
                Ok(HumanAuraV1 {
                    card: href(&aura.card, handles)?,
                    token: aura.token,
                    layer_order: aura
                        .layer_timestamp
                        .and_then(|stamp| timestamps.binary_search(&stamp).ok()),
                    supplies_current_override: aura.supplies_current_override,
                })
            })
            .collect::<Result<_, Error>>()?;
        hosts.push(HumanHostV1 {
            card: href(&host.card, handles)?,
            auras,
            form: host.form.clone(),
            counters: host.counters.clone(),
            tapped: host.tapped,
            summoning_sick: host.summoning_sick,
            moon_sets_land: host.moon_sets_land,
            moon_sets_colorless: host.moon_sets_colorless,
            moon_grants_tap_for_colorless: host.moon_grants_tap_for_colorless,
            granted_mana_available_now: host.granted_mana_available_now,
        });
    }
    let attachment_menu = facts
        .attachment_menu
        .as_ref()
        .map(|menu| {
            Ok::<_, Error>(HumanAttachmentMenuV1 {
                kind: menu.kind,
                chooser: menu.chooser,
                aura: href(&menu.aura, handles)?,
                selected_hosts: refs(&menu.selected_hosts, handles)?,
                total_hosts: menu.total_hosts,
                legal_hosts: refs(&menu.legal_hosts, handles)?,
            })
        })
        .transpose()?;
    Ok(HumanFactsV1 {
        meaning: facts.meaning.clone(),
        casts,
        hosts,
        attachment_menu,
    })
}

pub(super) fn label(
    facts: Option<&FactsV1>,
    action: &Action,
    handles: &Handles,
) -> Result<Option<String>, Error> {
    let annotated = crate::moon_consumer_v1::action(facts, action)
        .map_err(|_| Error::InvalidVisibleReference)?;
    Ok(match (annotated, action) {
        (
            Some(ActionV1::ChooseCastTarget),
            Action::ChooseTarget {
                target: TargetRefV1::Object { object },
                ..
            },
        ) => Some(format!(
            "Enchant {} with Imprisoned in the Moon",
            handles.name(object)?
        )),
        (
            Some(ActionV1::ChooseAttachmentHost { kind }),
            Action::ChooseEffectTarget {
                target: TargetRefV1::Object { object },
                ..
            },
        ) => {
            let aura = match kind {
                AttachmentKindV1::CopiedAura => "the Imprisoned in the Moon copy",
                AttachmentKindV1::ReturningAura => "the returning Imprisoned in the Moon",
            };
            Some(format!("Attach {aura} to {}", handles.name(object)?))
        }
        (Some(ActionV1::TapForColorless), Action::ActivateManaAbility { source, .. }) => {
            Some(format!("Tap {} for colorless mana", handles.name(source)?))
        }
        (None, _) => None,
        _ => return Err(Error::InvalidVisibleReference),
    })
}
