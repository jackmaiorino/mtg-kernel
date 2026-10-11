//! Cast/play instructions executed inside an existing resolution (608.2g).
//! The parent remains below newly cast spells until all its instructions finish.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResolutionPlayV1 {
    pub parent: StackItemId,
    pub parent_index: usize,
    pub suspended: Option<Box<effect::EffectContinuation>>,
    pub permission: Option<ResolutionPermissionV1>,
    pub deferred_triggers: Vec<PendingTrigger>,
    pub kicked_source: Option<ObjectId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResolutionPermissionV1 {
    pub card: EffectObjectBinding,
    pub controller: PlayerId,
    pub maximum_mana_value: Option<u16>,
    pub land: bool,
}

fn context(state: &GameState) -> Option<&ResolutionPlayV1> {
    state.standard_v1.as_ref()?.resolution_play.as_ref()
}
fn context_mut(state: &mut GameState) -> Option<&mut ResolutionPlayV1> {
    state.standard_v1.as_mut()?.resolution_play.as_mut()
}
pub(super) fn active(state: &GameState) -> bool {
    context(state).is_some()
}
pub(super) fn permit(state: &GameState, object: ObjectId) -> Option<ResolutionPermissionV1> {
    context(state)?
        .permission
        .filter(|p| p.card.object == object)
}
pub(crate) fn free_cast(state: &GameState, source: ObjectId) -> bool {
    state.objects.try_get(source).is_some_and(|o| {
        o.v4.spell_cast_origin.is_some_and(|origin| {
            matches!(origin.route, SpellCastRouteV4::ResolvingEffectV1 { .. })
        })
    })
}
fn live(state: &GameState, binding: EffectObjectBinding) -> bool {
    binding.expected_zone == Zone::Exile
        && state.exile.contains(&binding.object)
        && state.objects.try_get(binding.object).is_some_and(|o| {
            o.zone == Zone::Exile && o.zone_change_count == binding.expected_zone_change_count
        })
}
pub(super) fn form_allowed(state: &GameState, source: ObjectId, method: CastMethodV4) -> bool {
    let Some(o) = state.objects.try_get(source) else {
        return false;
    };
    let Some(origin) = o.v4.spell_cast_origin else {
        return true;
    };
    let SpellCastRouteV4::ResolvingEffectV1 {
        maximum_mana_value, ..
    } = origin.route
    else {
        return true;
    };
    let def = &card_def::CARD_DEFS[o.card_def as usize];
    let value = match method {
        CastMethodV4::Normal => def.mana_value,
        CastMethodV4::Omen => {
            let cost = if let Some(a) = supported_adventure(def) {
                a.cost
            } else if let Some(o) = supported_omen(def) {
                o.cost
            } else {
                return false;
            };
            u16::from(cost.generic) + cost.pips.len() as u16
        }
        _ => return false,
    };
    maximum_mana_value.is_none_or(|limit| value <= limit)
}

pub(crate) fn can_cast_exiled_without_mana(
    state: &GameState,
    controller: PlayerId,
    card: EffectObjectBinding,
    maximum_mana_value: Option<u16>,
) -> bool {
    if !live(state, card) {
        return false;
    }
    let def = &card_def::CARD_DEFS[state.objects.get(card.object).card_def as usize];
    if !def.is_executable() || !def.is_castable() || def.has_type(CardType::Land) {
        return false;
    }
    let mut projected = state.clone();
    projected.engine.pending_effect = None;
    projected.engine.pending_cast = None;
    projected.engine.pending_triggers.clear();
    projected
        .standard_v1
        .get_or_insert_with(Default::default)
        .resolution_play = Some(ResolutionPlayV1 {
        parent: StackItemId(0),
        parent_index: projected.stack.len(),
        suspended: None,
        permission: Some(ResolutionPermissionV1 {
            card,
            controller,
            maximum_mana_value,
            land: false,
        }),
        deferred_triggers: Vec::new(),
        kicked_source: None,
    });
    begin_cast_ex(
        &mut projected,
        controller,
        card.object,
        Some(CastMethodV4::Normal),
    );
    let mut pending = projected.engine.pending_cast.as_ref().unwrap().clone();
    for kicked in [false, true] {
        if kicked && def.kicker_cost.is_none() {
            continue;
        }
        pending.kicked = Some(kicked);
        for form in viable_pending_spell_forms(def, &pending, &projected) {
            pending.mode_chosen = Some(form);
            let method = if form == 1
                && (supported_adventure(def).is_some() || supported_omen(def).is_some())
            {
                CastMethodV4::Omen
            } else {
                CastMethodV4::Normal
            };
            if form_allowed(&projected, card.object, method)
                && pending_cast_quote_v1(def, &pending, method, kicked, 0, &projected).is_some()
            {
                return true;
            }
        }
    }
    false
}

pub(crate) fn stage(
    state: &mut GameState,
    continuation: effect::EffectContinuation,
    card: EffectObjectBinding,
    maximum_mana_value: Option<u16>,
    land: bool,
) -> Result<(), String> {
    let controller = continuation.ctx.controller;
    if !live(state, card) {
        return Err("resolution play lost its exact exile card".into());
    }
    if land {
        if !can_play_exiled_land(state, controller, card) {
            return Err("resolution land play has no remaining legal land drop".into());
        }
    } else if !can_cast_exiled_without_mana(state, controller, card, maximum_mana_value) {
        return Err("resolution cast has no payable legal form".into());
    }
    let parent = continuation.resolving_item.v4.stack_item_id;
    let parent_index = state.stack.len();
    let kicked_source = state.engine.pending_kicked_source.take();
    let context = state
        .standard_v1
        .get_or_insert_with(Default::default)
        .resolution_play
        .get_or_insert_with(|| ResolutionPlayV1 {
            parent,
            parent_index,
            suspended: None,
            permission: None,
            deferred_triggers: Vec::new(),
            kicked_source,
        });
    if context.parent != parent || context.suspended.is_some() || context.permission.is_some() {
        return Err("resolution play overlaps another casting instruction".into());
    }
    context.suspended = Some(Box::new(continuation));
    context.permission = Some(ResolutionPermissionV1 {
        card,
        controller,
        maximum_mana_value,
        land,
    });
    context
        .deferred_triggers
        .append(&mut state.engine.pending_triggers);
    state.priority_player = controller;
    if land {
        let def = &card_def::CARD_DEFS[state.objects.get(card.object).card_def as usize];
        if let Some(excluded_color) = def.as_enters_choose_color_other_than {
            state.engine.pending_land_play = Some(PendingLandPlay {
                source: card.object,
                source_zone_change_count: card.expected_zone_change_count,
                controller,
                origin_zone: Zone::Exile,
                excluded_color,
            });
        } else {
            play_land(state, controller, card.object, None);
        }
    } else {
        begin_cast_ex(state, controller, card.object, Some(CastMethodV4::Normal));
    }
    Ok(())
}

pub(crate) fn can_play_exiled_land(
    state: &GameState,
    controller: PlayerId,
    card: EffectObjectBinding,
) -> bool {
    live(state, card)
        && state.active_player == controller
        && state.players[controller.index()].lands_played_this_turn == 0
        && card_def::CARD_DEFS[state.objects.get(card.object).card_def as usize].is_playable_land()
}

pub(super) fn finish_child(state: &mut GameState) -> bool {
    let Some(context) = context_mut(state) else {
        return false;
    };
    let Some(parent) = context.suspended.take() else {
        return false;
    };
    context.permission = None;
    let triggers = std::mem::take(&mut state.engine.pending_triggers);
    context_mut(state)
        .unwrap()
        .deferred_triggers
        .extend(triggers);
    state.engine.pending_effect = Some(*parent);
    true
}
pub(super) fn defer_triggers(state: &mut GameState) -> bool {
    if !active(state) {
        return false;
    }
    let triggers = std::mem::take(&mut state.engine.pending_triggers);
    context_mut(state)
        .unwrap()
        .deferred_triggers
        .extend(triggers);
    true
}
pub(super) fn finish_parent(state: &mut GameState, parent: StackItemId) {
    if context(state).is_some_and(|c| c.parent == parent) {
        let context = state
            .standard_v1
            .as_mut()
            .unwrap()
            .resolution_play
            .take()
            .unwrap();
        state
            .engine
            .pending_triggers
            .extend(context.deferred_triggers);
        state.engine.pending_kicked_source = context.kicked_source;
    }
}
pub(crate) fn resolving_index(state: &GameState, parent: StackItemId) -> Option<usize> {
    context(state)
        .filter(|c| c.parent == parent)
        .map(|c| c.parent_index)
        .or_else(|| state.stack.len().checked_sub(1))
}
pub(super) fn restore_parent(state: &mut GameState, item: StackItem) {
    let index = context(state)
        .filter(|c| c.parent == item.v4.stack_item_id)
        .map_or(state.stack.len(), |c| c.parent_index);
    state.stack.insert(index.min(state.stack.len()), item);
}
