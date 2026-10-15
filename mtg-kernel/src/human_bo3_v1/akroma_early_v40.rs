//! Safe early-family supplement for the owning V40 projection.
use super::{visible::Handles, *};
use crate::{akroma_early_public_v40::EarlyProjectionV40, rl::ActionSemanticV1};
type Error = HumanDecisionErrorV1;
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanEarlyV40 {
    pub zombie_contexts: Vec<HumanZombieContextV40>,
    pub keep_one: Option<crate::raid_entry_public_v1::KeepOneContextV1>,
    pub gorehorn_sources: Vec<super::raid_entry_v21::HumanGorehornSourceV21>,
    pub saved_heartfire_damage: Vec<super::heartfire_v12::HumanHeartfireSavedDamageV12>,
    pub mana_bundles: Vec<super::bloom_tender_v24::HumanBloomTenderBundleV24>,
}
pub(super) fn register(o: &EarlyProjectionV40, handles: &mut Handles) -> Result<(), Error> {
    for row in &o.facts.zombie_contexts {
        handles.insert(&row.source)?;
        for card in &row.selected {
            handles.insert(card)?;
        }
    }
    for row in &o.facts.mana_bundles {
        handles.insert(&row.source)?;
    }
    Ok(())
}
pub(super) fn state(o: &EarlyProjectionV40, handles: &Handles) -> Result<HumanEarlyV40, Error> {
    let gorehorn_sources = o
        .facts
        .gorehorn_sources
        .iter()
        .map(|row| {
            let d = crate::card_def::CARD_DEFS
                .get(usize::from(row.printed_card_db_id))
                .ok_or(Error::InvalidVisibleReference)?;
            if d.name != "Gorehorn Raider" {
                return Err(Error::InvalidVisibleReference);
            }
            Ok(super::raid_entry_v21::HumanGorehornSourceV21 {
                stack_ordinal: row.stack_ordinal,
                printed_card_name: d.name.into(),
                ability_controller: row.ability_controller,
                damage_controller: row.damage_controller,
                historical: row.historical,
                color_mask: row.color_mask,
                lifelink: row.lifelink,
                deathtouch: row.deathtouch,
            })
        })
        .collect::<Result<_, Error>>()?;
    let saved_heartfire_damage = o
        .facts
        .saved_heartfire_damage
        .iter()
        .map(|saved| {
            let row = o
                .projection
                .surface
                .stack
                .get(saved.stack_index as usize)
                .ok_or(Error::InvalidVisibleReference)?;
            if row.source.card_db_id != saved.root.card_id
                || row.controller != saved.captured_controller
                || row.stack_index != saved.stack_index
                || row.stack_item_kind != crate::rl::StackItemKindV2::ActivatedAbility
            {
                return Err(Error::InvalidVisibleReference);
            }
            Ok(super::heartfire_v12::HumanHeartfireSavedDamageV12 {
                source: handles.get(&row.source)?.clone(),
                stack_index: saved.stack_index,
                signed_departing_power: saved.signed_departing_power,
                damage_if_target_legal_before_prevention: saved
                    .damage_if_target_legal_before_prevention,
                captured_controller: saved.captured_controller,
                captured_color_mask: saved.captured_color_mask,
                captured_lifelink: saved.captured_lifelink,
                captured_deathtouch: saved.captured_deathtouch,
                captured_werewolf: saved.captured_werewolf,
            })
        })
        .collect::<Result<_, Error>>()?;
    let mana_bundles = o
        .facts
        .mana_bundles
        .iter()
        .map(|row| {
            Ok(super::bloom_tender_v24::HumanBloomTenderBundleV24 {
                source: handles.get(&row.source)?.clone(),
                colors: row.colors.clone(),
            })
        })
        .collect::<Result<_, Error>>()?;
    let zombie_contexts = o
        .facts
        .zombie_contexts
        .iter()
        .map(|row| {
            Ok(HumanZombieContextV40 {
                stage: row.stage.clone(),
                source: handles.get(&row.source)?.clone(),
                original_controller: row.original_controller,
                captured_definition_colors: row.captured_definition_colors.clone(),
                selected: row
                    .selected
                    .iter()
                    .map(|card| handles.get(card).cloned())
                    .collect::<Result<_, Error>>()?,
                selected_graveyard_owner: row.selected_graveyard_owner,
                selected_live_legal: row.selected_live_legal.clone(),
            })
        })
        .collect::<Result<_, Error>>()?;
    Ok(HumanEarlyV40 {
        zombie_contexts,
        keep_one: o.facts.keep_one.clone(),
        gorehorn_sources,
        saved_heartfire_damage,
        mana_bundles,
    })
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanZombieContextV40 {
    pub stage: crate::soul_shackled_zombie_public_v1::TriggerStageV1,
    pub source: HumanCardRefV1,
    pub original_controller: PlayerSeatV1,
    pub captured_definition_colors: Vec<crate::mana::ManaColor>,
    pub selected: Vec<HumanCardRefV1>,
    pub selected_graveyard_owner: Option<PlayerSeatV1>,
    pub selected_live_legal: Vec<bool>,
}
pub(super) fn unary(
    o: &HumanEarlyV40,
    unary: &mut std::collections::BTreeMap<String, Vec<String>>,
) -> Result<(), Error> {
    for row in &o.mana_bundles {
        unary
            .entry(row.source.handle.clone())
            .or_default()
            .push(super::visible::without_handles(row)?);
    }
    for row in &o.saved_heartfire_damage {
        unary
            .entry(row.source.handle.clone())
            .or_default()
            .push(super::visible::without_handles(row)?);
    }
    for row in &o.zombie_contexts {
        let relation = super::visible::without_handles(row)?;
        unary
            .entry(row.source.handle.clone())
            .or_default()
            .push(format!("zombie-source:{relation}"));
        for (i, card) in row.selected.iter().enumerate() {
            unary
                .entry(card.handle.clone())
                .or_default()
                .push(format!("zombie-selected:{i}:{relation}"));
        }
    }
    Ok(())
}
pub(super) fn label(
    action: &ActionSemanticV1,
    o: &EarlyProjectionV40,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<Option<String>, Error> {
    let selected_source = match action {
        ActionSemanticV1::ChooseTarget { source, .. }
        | ActionSemanticV1::FinishTargetSelection { source, .. } => Some(source),
        _ => None,
    };
    if let Some(row) = selected_source.and_then(|source| {
        o.facts.zombie_contexts.iter().find(|row| {
            matches!(
                row.stage,
                crate::soul_shackled_zombie_public_v1::TriggerStageV1::Pending { ordinal: 0 }
            ) && row.source.arena_id == source.arena_id
        })
    }) {
        if row.original_controller != human {
            return Err(Error::NotHumanTurn);
        }
        return match action {
            ActionSemanticV1::ChooseTarget {
                target: crate::rl::TargetRefV1::Object { object },
                ..
            } => Ok(Some(format!(
                "Exile {} with {} (choose up to two from one graveyard; {} selected)",
                handles.name(object)?,
                handles.name(&row.source)?,
                row.selected.len()
            ))),
            ActionSemanticV1::FinishTargetSelection { selected_count, .. }
                if usize::from(*selected_count) == row.selected.len() =>
            {
                Ok(Some(format!(
                    "Finish {} exile selection with {} selected",
                    handles.name(&row.source)?,
                    row.selected.len()
                )))
            }
            _ => Err(Error::UnsupportedPrompt),
        };
    }
    if let ActionSemanticV1::ActivateAbility {
        actor,
        source,
        ability_index,
        granted_ability,
        ..
    } = action
    {
        let (id, index) = granted_ability
            .as_ref()
            .map(|(donor, local)| (donor.card_db_id, u32::from(*local)))
            .unwrap_or((source.card_db_id, u32::from(*ability_index)));
        let ooze = crate::rules_meaning_ooze_v1::root_meaning(id, index).is_ok();
        let heartfire = granted_ability.is_none()
            && crate::rules_meaning_heartfire_v1::root_meaning(id, index).is_ok();
        if ooze || heartfire {
            if *actor != human || source.zone != crate::state::Zone::Battlefield {
                return Err(Error::UnsupportedPrompt);
            }
            let name = handles.name(source)?;
            return Ok(Some(if ooze {
                format!("Activate {name}: pay {{G}}: exile one target card from either graveyard; if it was a creature card before the move, put a +1/+1 counter on this same permanent and gain 1 life. You still gain life if this permanent has left the battlefield")
            } else {
                format!("Activate {name}: pay {{R}} and sacrifice this permanent: it deals damage equal to its power as it left the battlefield to one target creature or planeswalker. Negative power deals no damage; use its captured controller, color and damage keywords")
            }));
        }
    }
    if let ActionSemanticV1::ActivateManaAbility { actor, source, .. } = action {
        if let Some(bundle) = o
            .facts
            .mana_bundles
            .iter()
            .find(|row| row.source == *source)
        {
            if *actor != human {
                return Err(Error::NotHumanTurn);
            }
            let mana = bundle
                .colors
                .iter()
                .map(|color| format!("{{{color:?}}}"))
                .collect::<String>();
            return Ok(Some(format!(
                "Tap {}: add {} (one of each color among permanents you control)",
                handles.name(source)?,
                if mana.is_empty() { "no mana" } else { &mana }
            )));
        }
    }
    Ok(None)
}
