//! Native target faces retained before any later hidden incarnation exists.
use super::{append, at, capture, cast_at, copies, validate_published, JournalV1};
use crate::{event::CommittedEvent as E, ids::StackItemId, state::{GameState, StackItem, StackTargetContractV4 as C}};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaceV1 {
    contract: C,
    public_card_def: Option<u16>,
}

pub(crate) struct CopyOwnerV1 {
    trigger: StackItemId,
    cast: u64,
    ordinal: u16,
}

pub(crate) fn copy_owner(state: &GameState, pending: &super::EffectContinuation, path: &[u16]) -> Result<CopyOwnerV1, String> {
    if !state.thousand_year_storm_consumer_v1 || !super::selected(state, &pending.resolving_item) {
        return Err("Storm target publication has no owning continuation".into());
    }
    validate_published(state, &pending.resolving_item)?;
    let cast = capture(pending.resolving_item.inline_effect.as_ref().ok_or("Storm root absent")?).ok_or("Storm root absent")?;
    let ordinal = copies(state, &pending.resolving_item)?.checked_add(1).ok_or("Storm copy ordinal overflow")?;
    if path != [ordinal] || ordinal > super::count(state, cast)? {
        return Err("Storm copy publication is not the next native slot".into());
    }
    Ok(CopyOwnerV1 { trigger: pending.resolving_item.v4.stack_item_id, cast, ordinal })
}

fn object(contract: C) -> Option<(crate::ids::ObjectId, u16, u32)> {
    match contract {
        C::Player(_) => None,
        C::Object { object, card_def, zone_change_count, .. } => Some((object, card_def, zone_change_count)),
        C::StackItem { source, card_def, source_zone_change_count, .. } => Some((source, card_def, source_zone_change_count)),
    }
}

fn capture_face(state: &GameState, contract: C) -> Result<FaceV1, String> {
    let public_card_def = match object(contract) {
        None => None,
        Some((id, card_def, generation)) => {
            let live = state.objects.try_get(id).ok_or("Storm target object absent")?;
            if live.zone_change_count != generation || live.card_def != card_def {
                return Err("Storm new target face is not current".into());
            }
            Some(crate::rl::projected_card_def(state, id, generation))
        }
    };
    Ok(FaceV1 { contract, public_card_def })
}

pub(super) fn record_cast(state: &mut GameState, spell: &StackItem) -> Result<(), String> {
    let faces = spell.v4.target_contracts.iter().map(|&c| capture_face(state, c)).collect::<Result<Vec<_>, _>>()?;
    append(state, JournalV1::TargetFaces { item: spell.v4.stack_item_id, faces }, false);
    Ok(())
}

fn faces_at(state: &GameState, index: usize, spell: &StackItem) -> Result<Vec<FaceV1>, String> {
    let Some(E::ThousandYearStormV1 { journal: JournalV1::TargetFaces { item, faces } }) = state.engine.event_history.get(index) else {
        return Err("Storm native target faces absent".into());
    };
    if *item != spell.v4.stack_item_id || faces.len() != spell.targets.len()
        || faces.iter().map(|f| f.contract).ne(spell.v4.target_contracts.iter().copied())
        || faces.iter().map(|f| f.contract.target()).ne(spell.targets.iter().copied())
    { return Err("Storm target face witness changed".into()); }
    for face in faces {
        match (object(face.contract), face.public_card_def) {
            (None, None) => {},
            (Some((id, card_def, generation)), Some(public)) => {
                let sentinel = ["Face-down creature", "Face-down card"].iter().any(|name| crate::card_def::card_id_by_name(name) == Some(public));
                if usize::from(card_def) >= crate::card_def::CARD_DEFS.len() || (public != card_def && !sentinel) {
                    return Err("Storm retained target face changed printing".into());
                }
                let live = state.objects.try_get(id).ok_or("Storm historical target absent")?;
                if live.zone_change_count < generation || (live.zone_change_count == generation && live.card_def != card_def) {
                    return Err("Storm retained target face changed incarnation".into());
                }
            },
            _ => return Err("Storm target face kind changed".into()),
        }
    }
    Ok(faces.clone())
}

pub(crate) fn for_snapshot(state: &GameState, spell: &StackItem) -> Result<Vec<FaceV1>, String> {
    if !state.thousand_year_storm_consumer_v1 { return Err("Storm target faces have no owning profile".into()); }
    let mut found = None;
    for (index, event) in state.engine.event_history.iter().enumerate() {
        let offset = match event {
            E::ThousandYearStormV1 { journal: JournalV1::Cast { spell: captured, .. } } if !spell.is_copy && captured.as_ref() == spell => 2,
            E::ThousandYearStormV1 { journal: JournalV1::Copy { item: captured, .. } } if spell.is_copy && captured.as_ref() == spell => 1,
            _ => continue,
        };
        if found.is_some() { return Err("Storm target faces have duplicate snapshot owners".into()); }
        found = Some(faces_at(state, index + offset, spell)?);
    }
    found.ok_or_else(|| "Storm target faces lost native snapshot".into())
}

/// Called only inside transactional native materialization, before ordinary
/// publication validates historical targets. Any failure discards the clone.
pub(crate) fn publish_copy(state: &mut GameState, owner: CopyOwnerV1, original: &StackItem, copy: &StackItem) -> Result<(), String> {
    if cast_at(state, owner.cast)?.0 != original { return Err("Storm copy target owner changed original spell".into()); }
    let original_faces = for_snapshot(state, original)?;
    let mut faces = Vec::with_capacity(copy.targets.len());
    for (index, &contract) in copy.v4.target_contracts.iter().enumerate() {
        faces.push(if original.v4.target_contracts.get(index) == Some(&contract) {
            original_faces.get(index).ok_or("Storm original target face absent")?.clone()
        } else { capture_face(state, contract)? });
    }
    append(state, JournalV1::Copy { trigger: owner.trigger, ordinal: owner.ordinal, item: Box::new(copy.clone()) }, false);
    append(state, JournalV1::TargetFaces { item: copy.v4.stack_item_id, faces }, false);
    Ok(())
}

pub(super) fn validate_state(state: &GameState) -> Result<(), String> {
    for (index, event) in state.engine.event_history.iter().enumerate() {
        match event {
            E::ThousandYearStormV1 { journal: JournalV1::TargetFaces { item, .. } } => {
                if !state.thousand_year_storm_consumer_v1 { return Err("Storm target face has no owning profile".into()); }
                let prior = index.checked_sub(1).and_then(|i| state.engine.event_history.get(i));
                let snapshot = match prior {
                    Some(E::ThousandYearStormV1 { journal: JournalV1::Copy { item: copy, .. } }) => copy.as_ref(),
                    Some(E::ThousandYearStormV1 { journal: JournalV1::CastForm { .. } }) => {
                        let JournalV1::Cast { spell, .. } = at(state, index.checked_sub(2).ok_or("Storm face position")? as u64)? else { return Err("Storm cast face lost native snapshot".into()); };
                        spell.as_ref()
                    },
                    _ => return Err("Storm target face is orphaned".into()),
                };
                if snapshot.v4.stack_item_id != *item { return Err("Storm face snapshot identity changed".into()); }
                for_snapshot(state, snapshot)?;
            },
            E::ThousandYearStormV1 { journal: JournalV1::Cast { spell, .. } | JournalV1::Copy { item: spell, .. } } if state.thousand_year_storm_consumer_v1 => { for_snapshot(state, spell)?; },
            _ => {},
        }
    }
    Ok(())
}

pub(crate) fn public_refs(state: &GameState, spell: &StackItem) -> Result<Vec<crate::rl::TargetRefV1>, String> {
    use crate::rl::{TargetRefV1 as R, CardStableRefV1};
    for_snapshot(state, spell)?.into_iter().map(|face| Ok(match face.contract {
        C::Player(player) => R::Player { player: player.into() },
        C::Object { object, owner, controller, zone, zone_change_count, .. } => R::Object {
            object: CardStableRefV1 { arena_id: object.0, card_db_id: face.public_card_def.ok_or("Storm object public face absent")?, owner: owner.into(), controller: controller.into(), zone, zone_change_count },
        },
        C::StackItem { stack_item_id, source, owner, controller, source_zone, source_zone_change_count, kind, .. } => R::StackItem {
            stack_item_id: stack_item_id.0,
            stack_index: state.stack.iter().position(|s| s.v4.stack_item_id == stack_item_id).map(|i| i as u32),
            source: CardStableRefV1 { arena_id: source.0, card_db_id: face.public_card_def.ok_or("Storm stack target public face absent")?, owner: owner.into(), controller: controller.into(), zone: source_zone, zone_change_count: source_zone_change_count },
            controller: controller.into(), kind: kind.into(),
        },
    })).collect()
}
