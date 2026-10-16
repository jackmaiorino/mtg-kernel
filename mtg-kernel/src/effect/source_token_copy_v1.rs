//! Native source-copy semantics shared by the Horde printing and its copies.
//! Copy-layer values are read at resolution, or at the original incarnation's
//! actual last battlefield departure. Counters and continuous grants are absent.
use super::*;
use crate::state::{AbilitySourceContractV4, StackItemKind};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ValuesV1 {
    FaceDown,
    FaceUp {
        card_def: u16,
        face_index: u8,
        copied_sacrifices: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DepartureV1 {
    source: AbilitySourceContractV4,
    values: ValuesV1,
    departure_history_index: u64,
}

fn definition(id: u16) -> bool {
    card_def::CARD_DEFS.get(usize::from(id)).is_some_and(|d| {
        d.has_full_support() && matches!(d.name, "Homunculus Horde" | "Homunculus Horde Token")
    })
}
pub(crate) fn recipe(op: &EffectOp) -> bool {
    matches!(op, EffectOp::CreateTokenCopyOfSourceV1)
}
pub(crate) fn selected(state: &GameState, item: &StackItem) -> bool {
    if item.inline_effect.as_ref().is_some_and(recipe) {
        return true;
    }
    if item.v4.granted_by.is_some()
        || !item
            .v4
            .ability_source_contract
            .is_some_and(|s| definition(s.card_def))
    {
        return false;
    }
    // A different genuine ability on this same creature has its own original
    // native provenance. Recognizing an operation alone is no exemption.
    if super::aberration_observed_row_matches_v1(state, item)
        && matches!(
            item.inline_effect,
            Some(EffectOp::CounterUnlessPaysGeneric { .. })
        )
        && super::validated_ward_observation_targeter(state, item).is_ok()
    {
        return false;
    }
    if event::copied_token_item_selected_v1(state, item)
        && event::validate_published_copied_token_v1(state, item).is_ok()
    {
        return false;
    }
    true
}
pub(crate) fn waiting(state: &GameState, ctx: &ExecCtx) -> bool {
    state.stack.iter().any(|item| {
        selected(state, item)
            && (item.source == ctx.source
                || item.v4.ability_source_contract.is_some()
                    && item.v4.ability_source_contract == ctx.ability_source_contract)
    })
}
fn values(state: &GameState, source: ObjectId) -> Result<ValuesV1, String> {
    let o = state
        .objects
        .try_get(source)
        .ok_or("source-copy object missing")?;
    if o.zone != Zone::Battlefield {
        return Err("source-copy values require the battlefield incarnation".into());
    }
    if o.v4.face_down_v1.is_some() {
        return Ok(ValuesV1::FaceDown);
    }
    let value = ValuesV1::FaceUp {
        card_def: o.card_def,
        face_index: o.v4.face_index,
        copied_sacrifices: event::copiable_token_abilities_v1(state, source)?,
    };
    validate_values(&value)?;
    Ok(value)
}
fn validate_values(values: &ValuesV1) -> Result<(), String> {
    if let ValuesV1::FaceUp {
        card_def,
        face_index,
        ..
    } = values
    {
        let d = card_def::CARD_DEFS
            .get(usize::from(*card_def))
            .ok_or("copy definition missing")?;
        if !d.has_full_support()
            || *face_index > 1
            || *face_index == 1 && d.transform_face.is_none()
        {
            return Err("source copy needs a supported exact copiable face".into());
        }
    }
    Ok(())
}
pub(crate) fn departure_input(
    state: &GameState,
    event: &event::ProposedEvent,
) -> Result<Option<DepartureV1>, String> {
    let event::ProposedEvent::ZoneChange(change) = event else {
        return Ok(None);
    };
    let Some(o) = state.objects.try_get(change.object) else {
        return Ok(None);
    };
    if o.zone != Zone::Battlefield || change.to_zone == Zone::Battlefield {
        return Ok(None);
    }
    // An unknown face-down printing does not select a receipt. A previously
    // published face-up source ability may still need its now-neutral LKI.
    let printed = definition(o.card_def)
        && o.v4.face_down_v1.is_none()
        && crate::continuous_characteristics_v1::printed_abilities_active(state, change.object);
    let same = |s: AbilitySourceContractV4| {
        s.source == change.object && s.zone_change_count == o.zone_change_count
    };
    let pending = state.stack.iter().any(|item| {
        item.inline_effect.as_ref().is_some_and(recipe)
            && item.v4.ability_source_contract.is_some_and(same)
    }) || state
        .engine
        .pending_triggers
        .iter()
        .any(|p| recipe(&p.effect) && p.source_contract.is_some_and(same));
    if !printed && !pending {
        return Ok(None);
    }
    Ok(Some(DepartureV1 {
        source: AbilitySourceContractV4::capture(state, change.object),
        values: values(state, change.object)?,
        departure_history_index: 0,
    }))
}
pub(crate) fn record_departure(
    state: &mut GameState,
    input: Option<DepartureV1>,
    index: usize,
) -> Result<(), String> {
    let Some(mut input) = input else {
        return Ok(());
    };
    if !matches!(state.engine.event_history.get(index),
        Some(event::CommittedEvent::ZoneChange { object, from: Zone::Battlefield, to, controller_before })
        if *object == input.source.source && *to != Zone::Battlefield
            && *controller_before == input.source.controller)
        || state.objects.try_get(input.source.source).is_none_or(|o| {
            o.owner != input.source.owner
                || input.source.zone_change_count.checked_add(1) != Some(o.zone_change_count)
                || o.zone == Zone::Battlefield
        })
    {
        return Err("source-copy LKI lost its actual departure".into());
    }
    input.departure_history_index = u64::try_from(index).map_err(|_| "copy departure width")?;
    // Keep all established death-capture adjacency intact. This private LKI
    // marker follows the original event and its preexisting capture records.
    state
        .engine
        .event_history
        .push(event::CommittedEvent::SourceCopyDepartureV1 {
            departure: Box::new(input),
        });
    Ok(())
}
pub(crate) fn validate_root(
    state: &GameState,
    op: &EffectOp,
    source: Option<AbilitySourceContractV4>,
) -> Result<(), String> {
    if !recipe(op) {
        return Ok(());
    }
    let source = source.ok_or("source copy lost original ability contract")?;
    if !definition(source.card_def) || source.zone != Zone::Battlefield {
        return Err("source copy changed original printed source".into());
    }
    crate::engine::validate_historical_ability_source_contract(state, source)?;
    original_values(state, source)?;
    Ok(())
}
fn original_values(state: &GameState, source: AbilitySourceContractV4) -> Result<ValuesV1, String> {
    if state.objects.try_get(source.source).is_some_and(|o| {
        o.zone == Zone::Battlefield && o.zone_change_count == source.zone_change_count
    }) {
        // A same-incarnation copy change, face-down state or control change
        // after triggering is observed now. The captured trigger controller stays.
        return values(state, source.source);
    }
    let rows = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(at, e)| match e {
            event::CommittedEvent::SourceCopyDepartureV1 { departure }
                if departure.source.source == source.source
                    && departure.source.zone_change_count == source.zone_change_count =>
            {
                Some((at, departure.as_ref()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(at, row)] = rows.as_slice() else {
        return Err("source copy lost unique last-known battlefield values".into());
    };
    let i = usize::try_from(row.departure_history_index).map_err(|_| "copy departure width")?;
    if i >= *at
        || row.source.zone != Zone::Battlefield
        || row.source.owner != source.owner
        || !matches!(state.engine.event_history.get(i),
            Some(event::CommittedEvent::ZoneChange { object, from: Zone::Battlefield, to, controller_before })
            if *object == source.source && *to != Zone::Battlefield
                && *controller_before == row.source.controller)
        || state.objects.try_get(source.source).is_none_or(|o| {
            o.owner != source.owner || o.zone_change_count <= source.zone_change_count
        })
    {
        return Err("source copy changed original departure/incarnation".into());
    }
    crate::engine::validate_historical_ability_source_contract(state, row.source)?;
    validate_values(&row.values)?;
    if let ValuesV1::FaceUp {
        card_def,
        copied_sacrifices,
        ..
    } = row.values
    {
        if card_def != row.source.card_def
            || event::historical_copiable_token_abilities_v1(state, row.source)?
                != copied_sacrifices
        {
            return Err("source-copy LKI changed copiable values or original abilities".into());
        }
    }
    Ok(row.values)
}
pub(super) fn execute(
    state: &mut GameState,
    ctx: &ExecCtx,
    proof: &NativeExecutionProofV1<'_>,
) -> Result<(), String> {
    let item = &proof.owner.item;
    if proof.ctx != ctx
        || !recipe(proof.owner.root.as_ref())
        || item.inline_effect.as_ref() != Some(proof.owner.root.as_ref())
        || item.kind != StackItemKind::TriggeredAbility
        || item.v4.granted_by.is_some()
        || item.v4.ability_source_contract != ctx.ability_source_contract
        || item.source != ctx.source
        || item.controller != ctx.controller
        || !ctx.targets.is_empty()
        || !ctx.target_contracts.is_empty()
    {
        return Err("source copy requires its original native row/context".into());
    }
    validate_root(
        state,
        proof.owner.root.as_ref(),
        ctx.ability_source_contract,
    )?;
    let source = ctx
        .ability_source_contract
        .ok_or("copy source contract missing")?;
    let (definition, face, copied_sacrifices) = match original_values(state, source)? {
        ValuesV1::FaceDown => (
            card_def::card_id_by_name(crate::self_reflection_v1::ANONYMOUS_BLUEPRINT)
                .filter(|id| crate::self_reflection_v1::anonymous_blueprint_is_exact(*id))
                .ok_or("neutral source-copy blueprint missing")?,
            0,
            0,
        ),
        ValuesV1::FaceUp {
            card_def,
            face_index,
            copied_sacrifices,
        } => (card_def, face_index, copied_sacrifices),
    };
    let mut proposal = event::ProposedEvent::create_token(definition, ctx.controller);
    if let event::ProposedEvent::CreateToken(token) = &mut proposal {
        token.entry_face_index = (face != 0).then_some(face);
        token.initialization_v48 = Some(crate::doubling_season_v48::TokenEntryV48 {
            copied: true,
            copied_end_step_sacrifices_v1: copied_sacrifices,
            ..Default::default()
        });
    }
    crate::doubling_season_v48::preflight_token_initialization(
        state,
        std::slice::from_ref(&proposal),
    )?;
    crate::doubling_season_v48::preflight_entry_roster(state, std::slice::from_ref(&proposal))?;
    event::propose_and_commit_batch(state, vec![proposal]);
    Ok(())
}
