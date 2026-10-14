use super::*;
use crate::fake_your_own_death_v50::public_v50 as public;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanFakeSpellV50 {
    pub occurrence: usize,
    pub original_source: HumanCardRefV1,
    pub original_target: HumanCardRefV1,
    pub copied: bool,
    pub copy_occurrence: Option<usize>,
    pub phase: public::SpellPhaseV50,
    pub grant: Option<usize>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanFakeCopyV50 {
    pub occurrence: usize,
    pub original_source: HumanCardRefV1,
    pub original_target: HumanCardRefV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanFakeGrantV50 {
    pub occurrence: usize,
    pub spell: usize,
    pub original_recipient: HumanCardRefV1,
    pub power_modifier_retained: bool,
    pub death_ability_active: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanFakeTreasureV50 {
    pub role: String,
    pub name: String,
    pub controller: PlayerSeatV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanFakeDeathV50 {
    pub occurrence: usize,
    pub grant: usize,
    pub original_creature: HumanCardRefV1,
    pub graveyard_incarnation: String,
    pub controller_at_death: PlayerSeatV1,
    pub was_token_at_death: bool,
    pub phase: public::DeathPhaseV50,
    pub returned_incarnation: Option<String>,
    pub returned_controller: Option<PlayerSeatV1>,
    pub attached_host: Option<String>,
    pub treasure_controller: Option<PlayerSeatV1>,
    pub treasures: Vec<HumanFakeTreasureV50>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanFakeAuraMenuV50 {
    pub death: usize,
    pub original_source: HumanCardRefV1,
    pub owner: PlayerSeatV1,
    pub ordered_legal_hosts: Vec<HumanCardRefV1>,
    pub mandatory: bool,
    pub targeted: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanFakeFactsV50 {
    pub spells: Vec<HumanFakeSpellV50>,
    pub unfinalized_copies: Vec<HumanFakeCopyV50>,
    pub grants: Vec<HumanFakeGrantV50>,
    pub deaths: Vec<HumanFakeDeathV50>,
    pub trigger_order: Vec<public::TriggerOrderV50>,
    pub aura_menu: Option<HumanFakeAuraMenuV50>,
}

pub(super) fn register(facts: &public::FactsV50, handles: &mut Handles) -> Result<(), Error> {
    for row in &facts.spells {
        handles.insert(&row.source)?;
        handles.insert(&row.original_target)?;
    }
    for row in &facts.unfinalized_copies {
        handles.insert(&row.source)?;
        handles.insert(&row.original_target)?;
    }
    for row in &facts.grants {
        handles.insert(&row.original_recipient)?;
    }
    for row in &facts.deaths {
        handles.insert(&row.original_creature)?;
    }
    if let Some(menu) = &facts.aura_menu {
        handles.insert(&menu.source)?;
        for host in &menu.legal_hosts {
            handles.insert(host)?;
        }
    }
    Ok(())
}
fn incarnation_role(
    roles: &mut BTreeMap<(u32, u32), String>,
    incarnation: public::IncarnationV50,
) -> String {
    let next = roles.len() + 1;
    roles
        .entry((incarnation.object, incarnation.generation))
        .or_insert_with(|| format!("fake_incarnation_{next}"))
        .clone()
}
pub(super) fn project(
    facts: &public::FactsV50,
    handles: &Handles,
) -> Result<HumanFakeFactsV50, Error> {
    let spells = facts
        .spells
        .iter()
        .map(|row| {
            Ok(HumanFakeSpellV50 {
                occurrence: row.occurrence,
                original_source: handles.get(&row.source)?.clone(),
                original_target: handles.get(&row.original_target)?.clone(),
                copied: row.copied,
                copy_occurrence: row.copy_occurrence,
                phase: row.phase,
                grant: row.grant,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let unfinalized_copies = facts
        .unfinalized_copies
        .iter()
        .map(|row| {
            Ok(HumanFakeCopyV50 {
                occurrence: row.occurrence,
                original_source: handles.get(&row.source)?.clone(),
                original_target: handles.get(&row.original_target)?.clone(),
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let grants = facts
        .grants
        .iter()
        .map(|row| {
            Ok(HumanFakeGrantV50 {
                occurrence: row.occurrence,
                spell: row.spell,
                original_recipient: handles.get(&row.original_recipient)?.clone(),
                power_modifier_retained: row.power_modifier_retained,
                death_ability_active: row.death_ability_active,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let mut incarnations = BTreeMap::new();
    let mut token_roles = BTreeMap::new();
    let deaths = facts
        .deaths
        .iter()
        .map(|row| {
            let treasures = row
                .treasures
                .iter()
                .map(|token| {
                    let next = token_roles.len() + 1;
                    let role = token_roles
                        .entry(token.object)
                        .or_insert_with(|| format!("fake_treasure_{next}"))
                        .clone();
                    let definition = crate::card_def::CARD_DEFS
                        .get(usize::from(token.card_db_id))
                        .ok_or(Error::InvalidVisibleReference)?;
                    Ok(HumanFakeTreasureV50 {
                        role,
                        name: definition.name.into(),
                        controller: token.controller,
                    })
                })
                .collect::<Result<Vec<_>, Error>>()?;
            Ok(HumanFakeDeathV50 {
                occurrence: row.occurrence,
                grant: row.grant,
                original_creature: handles.get(&row.original_creature)?.clone(),
                graveyard_incarnation: incarnation_role(
                    &mut incarnations,
                    row.graveyard_incarnation,
                ),
                controller_at_death: row.controller_at_death,
                was_token_at_death: row.was_token_at_death,
                phase: row.phase,
                returned_incarnation: row
                    .returned_incarnation
                    .map(|value| incarnation_role(&mut incarnations, value)),
                returned_controller: row.returned_controller,
                attached_host: row
                    .attached_host
                    .map(|value| incarnation_role(&mut incarnations, value)),
                treasure_controller: row.treasure_controller,
                treasures,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let aura_menu = facts
        .aura_menu
        .as_ref()
        .map(|menu| {
            Ok::<_, Error>(HumanFakeAuraMenuV50 {
                death: menu.death,
                original_source: handles.get(&menu.source)?.clone(),
                owner: menu.owner,
                ordered_legal_hosts: menu
                    .legal_hosts
                    .iter()
                    .map(|host| handles.get(host).cloned())
                    .collect::<Result<Vec<_>, Error>>()?,
                mandatory: menu.mandatory,
                targeted: menu.targeted,
            })
        })
        .transpose()?;
    Ok(HumanFakeFactsV50 {
        spells,
        unfinalized_copies,
        grants,
        deaths,
        trigger_order: facts.trigger_order.clone(),
        aura_menu,
    })
}

pub(super) fn unary(
    facts: &HumanFakeFactsV50,
    rows: &mut BTreeMap<String, Vec<String>>,
) -> Result<(), Error> {
    for spell in &facts.spells {
        rows.entry(spell.original_source.handle.clone())
            .or_default()
            .push(visible::without_handles(spell)?);
        rows.entry(spell.original_target.handle.clone())
            .or_default()
            .push(format!("fake_spell_target_{}", spell.occurrence));
    }
    for copy in &facts.unfinalized_copies {
        rows.entry(copy.original_source.handle.clone())
            .or_default()
            .push(visible::without_handles(copy)?);
        rows.entry(copy.original_target.handle.clone())
            .or_default()
            .push(format!("fake_copy_target_{}", copy.occurrence));
    }
    for grant in &facts.grants {
        rows.entry(grant.original_recipient.handle.clone())
            .or_default()
            .push(visible::without_handles(grant)?);
    }
    for death in &facts.deaths {
        rows.entry(death.original_creature.handle.clone())
            .or_default()
            .push(visible::without_handles(death)?);
    }
    if let Some(menu) = &facts.aura_menu {
        rows.entry(menu.original_source.handle.clone())
            .or_default()
            .push(visible::without_handles(menu)?);
        for (position, host) in menu.ordered_legal_hosts.iter().enumerate() {
            rows.entry(host.handle.clone())
                .or_default()
                .push(format!("fake_aura_host_{}_{}", menu.death, position));
        }
    }
    Ok(())
}
pub(super) fn label(
    action: &ActionSemanticV1,
    observation: &FakeObservationV50,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<Option<String>, Error> {
    let order_label = |actor: PlayerSeatV1,
                       position: usize,
                       source: &crate::rl::CardStableRefV1|
     -> Result<String, Error> {
        if actor != human {
            return Err(Error::NotHumanTurn);
        }
        let name = handles.name(source)?;
        if let Some(row) = observation
            .fake
            .trigger_order
            .iter()
            .find(|row| row.controller == actor && row.group_position == position)
        {
            let death = observation
                .fake
                .deaths
                .get(row.death)
                .ok_or(Error::InvalidVisibleReference)?;
            if death.original_creature != *source
                || death.controller_at_death != actor
                || death.phase != public::DeathPhaseV50::Held
            {
                return Err(Error::InvalidVisibleReference);
            }
            Ok(format!("{name} Fake dies occurrence #{} from grant #{}: return tapped, then create Treasure", death.occurrence + 1, death.grant + 1))
        } else {
            Ok(format!("{name} trigger #{}", position + 1))
        }
    };
    if let ActionSemanticV1::OrderTriggers {
        actor,
        pending_sources,
        order,
    } = action
    {
        if observation
            .fake
            .trigger_order
            .iter()
            .any(|row| row.controller == *actor)
        {
            if order.len() != pending_sources.len() {
                return Err(Error::InvalidVisibleReference);
            }
            let mut seen = std::collections::BTreeSet::new();
            let labels = order
                .iter()
                .map(|position| {
                    if !seen.insert(*position) {
                        return Err(Error::InvalidVisibleReference);
                    }
                    order_label(
                        *actor,
                        *position,
                        pending_sources
                            .get(*position)
                            .ok_or(Error::InvalidVisibleReference)?,
                    )
                })
                .collect::<Result<Vec<_>, Error>>()?;
            return Ok(Some(format!(
                "Put triggers on the stack in this order: {}",
                labels.join(", ")
            )));
        }
    }
    #[cfg(feature = "limited-fdn-fixtures")]
    if let ActionSemanticV1::ChooseTriggerOrderNext {
        actor,
        source,
        trigger_index,
        ordered_prefix,
        pending_count,
    } = action
    {
        if observation
            .fake
            .trigger_order
            .iter()
            .any(|row| row.controller == *actor)
        {
            let stage = observation
                .trigger_order
                .as_ref()
                .ok_or(Error::InvalidVisibleReference)?;
            if stage.controller != *actor
                || stage.pending_count != *pending_count
                || stage.ordered_prefix != *ordered_prefix
                || *trigger_index >= *pending_count
                || ordered_prefix.contains(trigger_index)
            {
                return Err(Error::InvalidVisibleReference);
            }
            return Ok(Some(format!(
                "Place {} next ({} already ordered; last placed resolves first)",
                order_label(*actor, *trigger_index, source)?,
                ordered_prefix.len()
            )));
        }
    }
    if let Some(menu) = &observation.fake.aura_menu {
        if let ActionSemanticV1::ChooseEffectTarget {
            actor,
            source,
            target: crate::rl::TargetRefV1::Object { object },
            selected_count,
            min_targets,
            max_targets,
        } = action
        {
            if source == &menu.source {
                if *actor != human || *actor != menu.owner {
                    return Err(Error::NotHumanTurn);
                }
                if *selected_count != 0
                    || *min_targets != 1
                    || *max_targets != 1
                    || !menu.mandatory
                    || menu.targeted
                    || !menu.legal_hosts.contains(object)
                {
                    return Err(Error::InvalidVisibleReference);
                }
                return Ok(Some(format!(
                    "Return {} tapped attached to {} (mandatory, not targeted)",
                    handles.name(source)?,
                    handles.name(object)?
                )));
            }
        }
    }
    let selected = |source: &crate::rl::CardStableRefV1| {
        crate::card_def::CARD_DEFS
            .get(usize::from(source.card_db_id))
            .is_some_and(|definition| definition.name == "Fake Your Own Death")
    };
    match action {
        ActionSemanticV1::CastSpell { actor, source } if selected(source) => {
            if *actor != human {
                return Err(Error::NotHumanTurn);
            }
            Ok(Some(format!("Begin casting {} ({{1}}{{B}}): target creature gets +2/+0 and gains the dies ability until end of turn", handles.name(source)?)))
        }
        ActionSemanticV1::ChooseTarget {
            actor,
            source,
            target: crate::rl::TargetRefV1::Object { object },
            ..
        } if selected(source) => {
            if *actor != human {
                return Err(Error::NotHumanTurn);
            }
            Ok(Some(format!(
                "{}: target {} for +2/+0 and the dies ability until end of turn",
                handles.name(source)?,
                handles.name(object)?
            )))
        }
        _ => Ok(None),
    }
}
