//! Native Harvester prerequisites. Registration is deliberately separate.
//! A copy exception belongs to layer 1, not a timestamped subtype grant.
use super::*;
use crate::state::AbilitySourceContractV4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NightmareV1 {
    pub source: AbilitySourceContractV4,
    pub creation_history_index: u64,
}

/// The source is the actual copy-layer form after the native layer change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NightmareLayerV1 {
    pub source: AbilitySourceContractV4,
    pub nightmare: bool,
}

pub(crate) fn record_entry(state: &mut GameState, creation: usize) {
    let Some(event::CommittedEvent::CreateToken { object, .. }) =
        state.engine.event_history.get(creation)
    else {
        return;
    };
    let object = *object;
    if !state.objects.get(object).v4.copied_nightmare_v1 {
        return;
    }
    let source = AbilitySourceContractV4::capture(state, object);
    state
        .engine
        .event_history
        .push(event::CommittedEvent::NightmareCopyEnteredV1 {
            entry: NightmareV1 {
                source,
                creation_history_index: creation as u64,
            },
        });
}

pub(crate) fn record_layer(state: &mut GameState, object: ObjectId, prior: bool) {
    let live = state.objects.get(object);
    if live.v4.copied_nightmare_v1 == prior {
        return;
    }
    let layer = NightmareLayerV1 {
        source: AbilitySourceContractV4::capture(state, object),
        nightmare: live.v4.copied_nightmare_v1,
    };
    state
        .engine
        .event_history
        .push(event::CommittedEvent::NightmareCopyLayerV1 { layer });
}

pub(crate) fn historical_nightmare(
    state: &GameState,
    source: AbilitySourceContractV4,
    before: usize,
) -> Result<bool, String> {
    let mut current = false;
    let mut entry_seen = false;
    for (at, event) in state.engine.event_history.iter().enumerate().take(before) {
        match event {
            event::CommittedEvent::NightmareCopyEnteredV1 { entry }
                if entry.source.source == source.source
                    && entry.source.zone_change_count == source.zone_change_count =>
            {
                if entry_seen
                    || entry.source.owner != source.owner
                    || entry.source.zone != Zone::Battlefield
                    || entry.source.zone_change_count != 0
                    || entry.creation_history_index >= at as u64
                    || !matches!(state.engine.event_history.get(entry.creation_history_index as usize),
                        Some(event::CommittedEvent::CreateToken { object, token_def, controller, .. })
                        if *object == source.source && *token_def == entry.source.card_def && *controller == entry.source.controller)
                {
                    return Err("Nightmare copy lost its original native creation".into());
                }
                crate::engine::validate_historical_ability_source_contract(state, entry.source)?;
                entry_seen = true;
                current = true;
            }
            event::CommittedEvent::NightmareCopyLayerV1 { layer }
                if layer.source.source == source.source
                    && layer.source.zone_change_count == source.zone_change_count =>
            {
                if !entry_seen
                    || layer.source.owner != source.owner
                    || layer.source.zone != Zone::Battlefield
                    || layer.nightmare == current
                {
                    return Err("Nightmare copy-layer sequence changed".into());
                }
                crate::engine::validate_historical_ability_source_contract(state, layer.source)?;
                current = layer.nightmare;
            }
            _ => {}
        }
    }
    Ok(current)
}

pub(crate) fn copiable_nightmare(state: &GameState, object: ObjectId) -> Result<bool, String> {
    let live = state
        .objects
        .try_get(object)
        .ok_or("Nightmare copy source absent")?;
    let expected = historical_nightmare(
        state,
        AbilitySourceContractV4::capture(state, object),
        state.engine.event_history.len(),
    )?;
    if expected != live.v4.copied_nightmare_v1
        || expected && (live.zone != Zone::Battlefield || !live.v4.is_token)
    {
        return Err("Nightmare copy component changed from its native entry/layers".into());
    }
    Ok(expected && live.v4.face_down_v1.is_none())
}

pub(crate) fn require_legacy_absence(state: &GameState) -> Result<(), String> {
    if crate::standard_cards_v1::has_battlefield_copy_origin_v1(state, definition)
        || state.objects.iter().any(|(_, o)| {
            matches!(o.zone, Zone::Battlefield | Zone::Stack | Zone::Graveyard)
                && o.v4.face_down_v1.is_none()
                && definition(o.card_def)
        })
        || state.stack.iter().any(|row| {
            row.v4
                .ability_source_contract
                .is_some_and(|source| definition(source.card_def))
        })
        || state
            .engine
            .event_history
            .iter()
            .any(|e| matches!(e, event::CommittedEvent::HarvesterResolvedV1 { .. }))
    {
        return Err("Abyssal Harvester requires a distinct complete consumer".into());
    }

    if state
        .objects
        .iter()
        .any(|(_, o)| o.zone == Zone::Battlefield && o.v4.copied_nightmare_v1)
        || state.engine.event_history.iter().any(|e| {
            matches!(
                e,
                event::CommittedEvent::NightmareCopyEnteredV1 { .. }
                    | event::CommittedEvent::NightmareCopyLayerV1 { .. }
            )
        })
    {
        return Err("copiable Nightmare exception requires a distinct complete consumer".into());
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum GraveyardOriginV1 {
    ZoneChange {
        history_index: u64,
    },
    /// A reversed cast retains its original graveyard eligibility. It is not
    /// a new card put into the graveyard during this turn.
    AbortedCast {
        original_receipt: u64,
        reversal: u64,
    },
    AbortedMadness {
        reversal: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraveyardEntryV1 {
    pub card: EffectObjectBinding,
    pub card_def: u16,
    pub owner: PlayerId,
    pub turn: u32,
    pub active_player: PlayerId,
    pub origin: GraveyardOriginV1,
}

pub(crate) fn catalog_enabled() -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && card_def::card_id_by_name("Abyssal Harvester").is_some()
}

pub(crate) fn record_graveyard_entry(state: &mut GameState, history_index: usize) {
    if !catalog_enabled() {
        return;
    }
    let Some(event::CommittedEvent::ZoneChange {
        object,
        to: Zone::Graveyard,
        ..
    }) = state.engine.event_history.get(history_index)
    else {
        return;
    };
    let object = *object;
    let live = state.objects.get(object);
    if live.v4.is_token || live.spell_copy_origin.is_some() {
        return;
    }
    let entry = GraveyardEntryV1 {
        card: EffectObjectBinding {
            object,
            expected_zone: Zone::Graveyard,
            expected_zone_change_count: live.zone_change_count,
        },
        card_def: live.card_def,
        owner: live.owner,
        turn: state.turn_key_v1(),
        active_player: state.active_player,
        origin: GraveyardOriginV1::ZoneChange {
            history_index: history_index as u64,
        },
    };
    state
        .engine
        .event_history
        .push(event::CommittedEvent::HarvesterGraveyardEntryV1 { entry });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CastV1 {
    pub source: ObjectId,
    pub card_def: u16,
    pub owner: PlayerId,
    pub controller: PlayerId,
    pub stack_generation: u32,
    pub origin: crate::state::SpellCastOriginV4,
    pub madness: bool,
}

pub(crate) fn record_cast(state: &mut GameState, source: crate::state::StackSourceContractV4) {
    if !catalog_enabled()
        || source.spell_copy_origin.is_some()
        || state.objects.get(source.source).v4.is_token
        || !card_def::CARD_DEFS[source.card_def as usize].has_type(CardType::Creature)
    {
        return;
    }
    let Some(origin) = source.spell_cast_origin else {
        return;
    };
    let cast = CastV1 {
        source: source.source,
        card_def: source.card_def,
        owner: source.owner,
        controller: source.controller,
        stack_generation: source.zone_change_count,
        origin,
        madness: source.cast_method == crate::state::CastMethodV4::Madness,
    };
    state
        .engine
        .event_history
        .push(event::CommittedEvent::HarvesterCastAnnouncedV1 { cast });
}

fn generation_after(state: &GameState, object: ObjectId, index: usize) -> Result<u32, String> {
    let later = state
        .engine
        .event_history
        .iter()
        .skip(index + 1)
        .filter(|event| match event {
            event::CommittedEvent::ZoneChange { object: id, .. } => *id == object,
            event::CommittedEvent::HarvesterCastAnnouncedV1 { cast } => cast.source == object,
            event::CommittedEvent::HarvesterCastReversedV1 { restored, .. } => {
                restored.object == object
            }
            _ => false,
        })
        .count();
    state
        .objects
        .try_get(object)
        .ok_or("Harvester physical card absent")?
        .zone_change_count
        .checked_sub(u32::try_from(later).map_err(|_| "Harvester move count width")?)
        .ok_or_else(|| "Harvester physical generation underflow".into())
}
fn turn_at(state: &GameState, index: usize) -> Result<(u32, PlayerId), String> {
    state
        .engine
        .event_history
        .iter()
        .take(index)
        .rev()
        .find_map(|event| match event {
            event::CommittedEvent::LifeLossTurnBeganV1 {
                turn,
                active_player,
            } => Some((*turn, *active_player)),
            _ => None,
        })
        .ok_or_else(|| "Harvester graveyard move lost actual native turn boundary".into())
}
fn cast_at(state: &GameState, index: u64) -> Result<CastV1, String> {
    let at = usize::try_from(index).map_err(|_| "Harvester cast index width")?;
    let Some(event::CommittedEvent::HarvesterCastAnnouncedV1 { cast }) =
        state.engine.event_history.get(at)
    else {
        return Err("Harvester cast occurrence absent".into());
    };
    let live = state
        .objects
        .try_get(cast.source)
        .ok_or("Harvester cast card absent")?;
    if cast.owner != live.owner
        || cast.origin.finalized_method.is_some()
        || !matches!(
            cast.origin.origin_zone,
            Zone::Hand | Zone::Graveyard | Zone::Exile
        )
        || cast.origin.origin_zone_change_count.checked_add(1) != Some(cast.stack_generation)
        || generation_after(state, cast.source, at)? != cast.stack_generation
    {
        return Err("Harvester original cast incarnation changed".into());
    }
    Ok(*cast)
}
fn reversal_at(
    state: &GameState,
    index: u64,
) -> Result<(u64, CastV1, EffectObjectBinding), String> {
    let at = usize::try_from(index).map_err(|_| "Harvester reversal index width")?;
    let Some(event::CommittedEvent::HarvesterCastReversedV1 {
        announcement,
        restored,
    }) = state.engine.event_history.get(at)
    else {
        return Err("Harvester cast reversal absent".into());
    };
    let cast = cast_at(state, *announcement)?;
    if *announcement >= index || restored.object != cast.source
        || restored.expected_zone != if cast.madness { Zone::Graveyard } else { cast.origin.origin_zone }
        || cast.stack_generation.checked_add(1) != Some(restored.expected_zone_change_count)
        || generation_after(state, cast.source, at)? != restored.expected_zone_change_count
        || state.engine.event_history.iter().filter(|event| matches!(event,
            event::CommittedEvent::HarvesterCastReversedV1 { announcement: other, .. } if other == announcement)).count() != 1
    { return Err("Harvester native cast reversal changed".into()); }
    Ok((*announcement, cast, *restored))
}
fn graveyard_entry(state: &GameState, at: usize) -> Result<GraveyardEntryV1, String> {
    let Some(event::CommittedEvent::HarvesterGraveyardEntryV1 { entry }) =
        state.engine.event_history.get(at)
    else {
        return Err("Harvester graveyard occurrence absent".into());
    };
    let original = *entry;
    let mut cursor = at;
    let mut current = original;
    loop {
        let live = state
            .objects
            .try_get(current.card.object)
            .ok_or("Harvester graveyard card absent")?;
        if current.card.expected_zone != Zone::Graveyard
            || current.owner != live.owner
            || (current.card.expected_zone_change_count == live.zone_change_count
                && current.card_def != live.card_def)
            || generation_after(state, current.card.object, cursor)?
                != current.card.expected_zone_change_count
        {
            return Err("Harvester graveyard entry changed physical card/incarnation".into());
        }
        match current.origin {
            GraveyardOriginV1::ZoneChange { history_index } => {
                let index =
                    usize::try_from(history_index).map_err(|_| "Harvester entry index width")?;
                if index >= cursor
                    || !matches!(state.engine.event_history.get(index),
                    Some(event::CommittedEvent::ZoneChange { object, to: Zone::Graveyard, .. }) if *object == current.card.object)
                    || generation_after(state, current.card.object, index)?
                        != current.card.expected_zone_change_count
                    || turn_at(state, index)? != (current.turn, current.active_player)
                {
                    return Err("Harvester entry lost its actual turn/move/incarnation".into());
                }
                break;
            }
            GraveyardOriginV1::AbortedCast {
                original_receipt,
                reversal,
            } => {
                let (_, cast, restored) = reversal_at(state, reversal)?;
                if original_receipt >= reversal
                    || reversal >= cursor as u64
                    || restored != current.card
                    || cast.owner != current.owner
                    || cast.card_def != current.card_def
                    || cast.origin.origin_zone != Zone::Graveyard
                {
                    return Err("Harvester rollback lost original cast source".into());
                }
                let next = usize::try_from(original_receipt)
                    .map_err(|_| "Harvester rollback index width")?;
                let Some(event::CommittedEvent::HarvesterGraveyardEntryV1 { entry }) =
                    state.engine.event_history.get(next)
                else {
                    return Err("Harvester rollback input absent".into());
                };
                if entry.card.object != current.card.object
                    || entry.owner != current.owner
                    || entry.card_def != current.card_def
                    || entry.turn != current.turn
                    || entry.active_player != current.active_player
                    || cast.origin.origin_zone_change_count != entry.card.expected_zone_change_count
                {
                    return Err("Harvester rollback invented new graveyard eligibility".into());
                }
                cursor = next;
                current = *entry;
            }
            GraveyardOriginV1::AbortedMadness { reversal } => {
                let (_, cast, restored) = reversal_at(state, reversal)?;
                if reversal >= cursor as u64
                    || !cast.madness
                    || restored != current.card
                    || cast.owner != current.owner
                    || cast.card_def != current.card_def
                    || turn_at(state, reversal as usize)? != (current.turn, current.active_player)
                {
                    return Err("Harvester Madness return lost actual occurrence".into());
                }
                break;
            }
        }
    }
    Ok(original)
}

pub(crate) fn rollback_input(
    state: &GameState,
    pending: &crate::engine::PendingCast,
) -> Option<(u64, GraveyardEntryV1)> {
    if !catalog_enabled() || pending.origin_zone != Zone::Graveyard {
        return None;
    }
    let generation = pending
        .source_contract
        .spell_cast_origin?
        .origin_zone_change_count;
    state
        .engine
        .event_history
        .iter()
        .enumerate()
        .rev()
        .find_map(|(at, event)| match event {
            event::CommittedEvent::HarvesterGraveyardEntryV1 { entry }
                if entry.card.object == pending.spell
                    && entry.card.expected_zone_change_count == generation =>
            {
                Some((at as u64, *entry))
            }
            _ => None,
        })
}
pub(crate) fn record_rollback(
    state: &mut GameState,
    pending: &crate::engine::PendingCast,
    input: Option<(u64, GraveyardEntryV1)>,
) {
    if !catalog_enabled() {
        return;
    }
    let source = pending.source_contract;
    let announcement = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .rev()
        .find_map(|(at, event)| match event {
            event::CommittedEvent::HarvesterCastAnnouncedV1 { cast }
                if cast.source == source.source
                    && cast.stack_generation == source.zone_change_count =>
            {
                Some(at as u64)
            }
            _ => None,
        });
    let Some(announcement) = announcement else {
        return;
    };
    let live = state.objects.get(pending.spell);
    let restored = EffectObjectBinding {
        object: pending.spell,
        expected_zone: live.zone,
        expected_zone_change_count: live.zone_change_count,
    };
    let reversal = state.engine.event_history.len() as u64;
    state
        .engine
        .event_history
        .push(event::CommittedEvent::HarvesterCastReversedV1 {
            announcement,
            restored,
        });
    if restored.expected_zone != Zone::Graveyard {
        return;
    }
    let entry = match input {
        Some((original_receipt, mut entry)) => {
            entry.card = restored;
            entry.origin = GraveyardOriginV1::AbortedCast {
                original_receipt,
                reversal,
            };
            entry
        }
        None if source.cast_method == crate::state::CastMethodV4::Madness => GraveyardEntryV1 {
            card: restored,
            card_def: source.card_def,
            owner: source.owner,
            turn: state.turn_key_v1(),
            active_player: state.active_player,
            origin: GraveyardOriginV1::AbortedMadness { reversal },
        },
        None => return,
    };
    state
        .engine
        .event_history
        .push(event::CommittedEvent::HarvesterGraveyardEntryV1 { entry });
}

pub(crate) fn eligible(state: &GameState, object: ObjectId) -> bool {
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    if live.zone != Zone::Graveyard
        || live.v4.is_token
        || live.spell_copy_origin.is_some()
        || !card_def::CARD_DEFS[live.card_def as usize].has_type(CardType::Creature)
    {
        return false;
    }
    let mut matches = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(at, event)| match event {
            event::CommittedEvent::HarvesterGraveyardEntryV1 { entry }
                if entry.card.object == object
                    && entry.card.expected_zone_change_count == live.zone_change_count =>
            {
                Some(at)
            }
            _ => None,
        });
    let Some(at) = matches.next() else {
        return false;
    };
    matches.next().is_none()
        && graveyard_entry(state, at).is_ok_and(|entry| {
            entry.turn == state.turn_key_v1() && entry.active_player == state.active_player
        })
}

pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    let mut copied = std::collections::BTreeSet::new();
    let mut entries = std::collections::BTreeSet::new();
    let mut casts = std::collections::BTreeSet::new();
    for (at, event) in state.engine.event_history.iter().enumerate() {
        match event {
            event::CommittedEvent::NightmareCopyEnteredV1 { entry } => {
                copied.insert(entry.source.source);
            }
            event::CommittedEvent::NightmareCopyLayerV1 { layer } => {
                copied.insert(layer.source.source);
            }
            _ => {}
        }
        match event {
            event::CommittedEvent::HarvesterGraveyardEntryV1 { entry } => {
                crate::life_loss_turn_v1::flags(state)?;
                if !entries.insert((entry.card.object, entry.card.expected_zone_change_count)) {
                    return Err("Harvester duplicate graveyard incarnation".into());
                }
                graveyard_entry(state, at)?;
            }
            event::CommittedEvent::HarvesterCastAnnouncedV1 { cast } => {
                if !casts.insert((cast.source, cast.stack_generation)) {
                    return Err("Harvester duplicate cast announcement".into());
                }
                cast_at(state, at as u64)?;
            }
            event::CommittedEvent::HarvesterCastReversedV1 { .. } => {
                reversal_at(state, at as u64)?;
            }
            _ => {}
        }
        if matches!(
            event,
            event::CommittedEvent::NightmareCopyEnteredV1 { .. }
                | event::CommittedEvent::NightmareCopyLayerV1 { .. }
                | event::CommittedEvent::HarvesterGraveyardEntryV1 { .. }
                | event::CommittedEvent::HarvesterCastAnnouncedV1 { .. }
                | event::CommittedEvent::HarvesterCastReversedV1 { .. }
        ) && state.engine.event_log.contains(event)
        {
            return Err("private Nightmare copy journal exposed as event".into());
        }
    }
    for (id, live) in state.objects.iter() {
        if live.zone == Zone::Battlefield && (live.v4.copied_nightmare_v1 || copied.contains(&id)) {
            copiable_nightmare(state, id)?;
            if live.v4.copied_nightmare_v1
                && !live
                    .v4
                    .effective_subtype_ids
                    .contains(&card_def::Subtype::Nightmare.stable_id())
            {
                return Err("copiable Nightmare lost its base subtype".into());
            }
        } else if live.v4.copied_nightmare_v1 {
            return Err("copiable Nightmare survived a zone change".into());
        }
    }
    Ok(())
}

pub(crate) fn marker() -> EffectOp {
    EffectOp::HarvesterExileAndCopyV1
}
pub(crate) fn recipe(op: &EffectOp) -> bool {
    matches!(op, EffectOp::HarvesterExileAndCopyV1)
}
pub(crate) fn target_spec() -> card_def::TargetSpec {
    card_def::TargetSpec::StandardV1(
        crate::standard_cards_v1::StandardTargetV1::CreatureCardPutIntoGraveyardThisTurnV1,
    )
}
fn family(item: &StackItem) -> bool {
    item.v4.granted_by.is_none()
        && item
            .v4
            .ability_source_contract
            .is_some_and(|s| definition(s.card_def))
}
pub(crate) fn selected(state: &GameState, item: &StackItem) -> bool {
    if item.inline_effect.as_ref().is_some_and(recipe) {
        return true;
    }
    let actual = state
        .stack
        .iter()
        .find(|row| row.v4.stack_item_id == item.v4.stack_item_id);
    if actual.is_some_and(|row| row.inline_effect.as_ref().is_some_and(recipe)) {
        return true;
    }
    if !family(item) && !actual.is_some_and(family) {
        return false;
    }
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
                || ctx.stack_item_id == Some(item.v4.stack_item_id)
                || item.v4.ability_source_contract.is_some()
                    && item.v4.ability_source_contract == ctx.ability_source_contract)
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolutionV1 {
    pub item: Box<StackItem>,
    pub created: Vec<EffectObjectBinding>,
    pub exiled_others: Vec<EffectObjectBinding>,
}

pub(super) fn execute(
    state: &mut GameState,
    ctx: &ExecCtx,
    proof: &NativeExecutionProofV1<'_>,
) -> Result<(), String> {
    let owner = proof.owner;
    owner.validate_entry(state, &owner.item, ctx, &owner.root)?;
    let item = &owner.item;
    let source = ctx
        .ability_source_contract
        .ok_or("Harvester original source absent")?;
    if proof.ctx != ctx
        || !recipe(owner.root.as_ref())
        || item.inline_effect.as_ref() != Some(owner.root.as_ref())
        || item.kind != StackItemKind::ActivatedAbility
        || item.v4.activated_ability_index != Some(0)
        || item.v4.granted_by.is_some()
        || item.v4.ability_source_contract != Some(source)
        || !definition(source.card_def)
        || source.source != ctx.source
        || source.controller != ctx.controller
        || ctx.targets.len() != 1
        || ctx.target_contracts.len() != 1
    {
        return Err("Harvester changed its actual native activation/context".into());
    }
    crate::engine::validate_historical_ability_source_contract(state, source)?;
    if state.engine.event_history.iter().any(|e| {
        matches!(e,event::CommittedEvent::HarvesterResolvedV1 { resolution }
        if resolution.item.v4.stack_item_id == item.v4.stack_item_id)
    }) {
        return Err("Harvester activation replayed".into());
    }
    let mut created = Vec::new();
    let mut exiled_others = Vec::new();
    let Target::Object(target) = ctx.targets[0] else {
        return Err("Harvester target is not a card".into());
    };
    // One target: if it left/reentered or is otherwise illegal, the whole
    // ability fizzles. No token or other-Nightmare cleanup occurs.
    if crate::engine::target_contract_matches_live(state, ctx.targets[0], ctx.target_contracts[0])
        && crate::engine::effect_target_is_legal_from_ability_source(
            state,
            source,
            ctx.controller,
            target_spec(),
            &ctx.targets,
            0,
        )
        && eligible(state, target)
    {
        let definition = state.objects.get(target).card_def;
        let mut proposal = event::ProposedEvent::create_token(definition, ctx.controller);
        if let event::ProposedEvent::CreateToken(token) = &mut proposal {
            token.initialization_v48 = Some(crate::doubling_season_v48::TokenEntryV48 {
                copied: true,
                copied_nightmare_v1: true,
                ..Default::default()
            });
        }
        // Check representability/width before the first real move. These
        // preflights do not select the creation replacement roster early.
        crate::doubling_season_v48::preflight_token_initialization(
            state,
            std::slice::from_ref(&proposal),
        )?;
        crate::doubling_season_v48::preflight_entry_roster(state, std::slice::from_ref(&proposal))?;
        event::propose_and_commit(
            state,
            event::ProposedEvent::zone_change(target, Zone::Exile),
        );
        if state.engine.halted.is_some() {
            return Err("Harvester exile halted".into());
        }
        // Copy regardless of the exile's actual destination. Only this
        // completed, original ability owns the new creation batch.
        let start = state.engine.event_history.len();
        event::propose_and_commit_batch(state, vec![proposal]);
        if state.engine.halted.is_some() {
            return Err("Harvester token entry halted".into());
        }
        for event in &state.engine.event_history[start..] {
            if let event::CommittedEvent::CreateToken {
                object,
                token_def,
                controller,
                ..
            } = event
            {
                if *token_def != definition || *controller != ctx.controller {
                    return Err("Harvester creation batch changed".into());
                }
                let o = state.objects.get(*object);
                created.push(EffectObjectBinding {
                    object: *object,
                    expected_zone: Zone::Battlefield,
                    expected_zone_change_count: o.zone_change_count,
                });
            }
        }
        exiled_others = state.players[ctx.controller.index()]
            .battlefield
            .iter()
            .filter_map(|id| {
                let o = state.objects.get(*id);
                (o.zone == Zone::Battlefield
                    && o.controller == ctx.controller
                    && o.v4.is_token
                    && !created.iter().any(|new| new.object == *id)
                    && crate::engine::has_effective_subtype(
                        state,
                        *id,
                        card_def::Subtype::Nightmare,
                    ))
                .then_some(EffectObjectBinding {
                    object: *id,
                    expected_zone: Zone::Battlefield,
                    expected_zone_change_count: o.zone_change_count,
                })
            })
            .collect();
        super::commit_zone_change_batch(state, &exiled_others, Zone::Exile, false)?;
        if state.engine.halted.is_some() {
            return Err("Harvester other Nightmare exile halted".into());
        }
    }
    state
        .engine
        .event_history
        .push(event::CommittedEvent::HarvesterResolvedV1 {
            resolution: ResolutionV1 {
                item: Box::new(item.clone()),
                created,
                exiled_others,
            },
        });
    Ok(())
}

pub(crate) fn definition(id: u16) -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && id == 479
        && card_def::CARD_DEFS.get(usize::from(id)).is_some_and(|d| {
            let card_def::CardDef {
                name: _,
                capability: _,
                cost: _,
                generic_cost_reduction: _,
                ward_cost: _,
                spell_cannot_be_countered: _,
                records_mana_spent: _,
                equipment: _,
                types: _,
                subtypes: _,
                supertypes: _,
                power: _,
                toughness: _,
                starting_loyalty: _,
                enters_with_plus_one_counters: _,
                controlled_counter_keyword: _,
                game_outcome_protection_v1: _,
                controlled_creature_grant: _,
                is_land: _,
                produces_mana: _,
                colors: _,
                target_spec: _,
                keywords: _,
                spell_effect: _,
                mana_ability: _,
                alt_cost: _,
                kicker_cost: _,
                additional_cost: _,
                flashback: _,
                activated_abilities: _,
                plot_cost: _,
                madness_cost: _,
                mode2: _,
                mode3: _,
                additional_modes: _,
                kicked_target_spec: _,
                is_token: _,
                escape: _,
                mana_ability_choices: _,
                enters_battlefield_tapped: _,
                mana_ability_def: _,
                minimum_blockers: _,
                omen: _,
                mana_value: _,
                mana_ability_includes_chosen_color: _,
                as_enters_choose_color_other_than: _,
                additional_mana_abilities: _,
                object_name: _,
                enters_battlefield_tapped_unless: _,
                attachment: _,
                transform_face: _,
                saga: _,
                optional_additional_cost: _,
                changeling: _,
                bestow: _,
                delve: _,
                adventure: _,
                cant_be_blocked_by_monarchs_creatures: _,
                conditional_tap_yield: _,
                enters_tapped_unless_controller: _,
                additional_mana_ability_conditions: _,
                restricted_mana_abilities: _,
                animation: _,
                activated_ability_generic_reductions: _,
                combat_static_rules: _,
                as_enters_color_choice_v1: _,
                chosen_color_creature_boost_v1: _,
                damage_replacement_v1: _,
                token_counter_replacement_v48: _,
                starting_life_self_bonuses_v1: _,
                source_type_hexproof_v1: _,
                other_controlled_nontoken_death_v1: _,
                creature_type_entry_v1: _,
            } = d;
            d.name == "Abyssal Harvester"
                && d.capability == card_def::CardCapability::Partial
                && d.cost
                    == crate::mana::Cost {
                        pips: &[
                            crate::mana::Pip::Colored(crate::mana::ManaColor::B),
                            crate::mana::Pip::Colored(crate::mana::ManaColor::B),
                        ],
                        generic: 1,
                        x_count: 0,
                    }
                && d.generic_cost_reduction.is_none()
                && d.ward_cost.is_none()
                && !d.spell_cannot_be_countered
                && !d.records_mana_spent
                && d.equipment.is_none()
                && d.types == [CardType::Creature]
                && d.subtypes == [card_def::Subtype::Demon, card_def::Subtype::Warlock]
                && d.supertypes.is_empty()
                && d.power == Some(3)
                && d.toughness == Some(2)
                && d.starting_loyalty.is_none()
                && d.enters_with_plus_one_counters.is_none()
                && d.controlled_counter_keyword.is_none()
                && d.game_outcome_protection_v1.is_none()
                && d.controlled_creature_grant.is_none()
                && !d.is_land
                && d.produces_mana.is_empty()
                && d.colors == [crate::mana::ManaColor::B]
                && d.target_spec == card_def::TargetSpec::None
                && d.keywords == Keywords::NONE
                && (d.spell_effect)()
                    == Some(EffectOp::MoveObject {
                        object: ObjectRef::ThisSource,
                        to_zone: Zone::Battlefield,
                    })
                && (d.mana_ability)().is_none()
                && d.alt_cost.is_none()
                && d.kicker_cost.is_none()
                && d.additional_cost.is_none()
                && d.flashback.is_none()
                && d.activated_abilities.len() == 1
                && d.activated_abilities[0].cost == [card_def::CostComponent::Tap]
                && d.activated_abilities[0].target_spec == target_spec()
                && (d.activated_abilities[0].effect)() == marker()
                && d.plot_cost.is_none()
                && d.madness_cost.is_none()
                && d.mode2.is_none()
                && d.mode3.is_none()
                && d.additional_modes.is_empty()
                && d.kicked_target_spec.is_none()
                && !d.is_token
                && d.escape.is_none()
                && d.mana_ability_choices.is_empty()
                && !d.enters_battlefield_tapped
                && d.mana_ability_def.is_none()
                && d.minimum_blockers == 1
                && d.omen.is_none()
                && d.mana_value == 3
                && !d.mana_ability_includes_chosen_color
                && d.as_enters_choose_color_other_than.is_none()
                && d.additional_mana_abilities.is_empty()
                && d.object_name == d.name
                && d.enters_battlefield_tapped_unless.is_none()
                && d.attachment.is_none()
                && d.transform_face.is_none()
                && d.saga.is_none()
                && d.optional_additional_cost.is_none()
                && !d.changeling
                && d.bestow.is_none()
                && !d.delve
                && d.adventure.is_none()
                && !d.cant_be_blocked_by_monarchs_creatures
                && d.conditional_tap_yield.is_none()
                && d.enters_tapped_unless_controller.is_none()
                && d.additional_mana_ability_conditions.is_empty()
                && d.restricted_mana_abilities.is_empty()
                && d.animation.is_none()
                && d.activated_ability_generic_reductions.is_empty()
                && d.combat_static_rules == card_def::CombatStaticRulesDef::NONE
                && d.as_enters_color_choice_v1.is_none()
                && d.chosen_color_creature_boost_v1.is_none()
                && d.damage_replacement_v1.is_none()
                && d.token_counter_replacement_v48.is_none()
                && d.starting_life_self_bonuses_v1.is_empty()
                && d.source_type_hexproof_v1.is_none()
                && d.other_controlled_nontoken_death_v1.is_none()
                && d.creature_type_entry_v1.is_none()
        })
}
