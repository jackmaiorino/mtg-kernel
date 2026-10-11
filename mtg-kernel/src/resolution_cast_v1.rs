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
    let Some(object) = state.objects.try_get(source) else {
        return false;
    };
    free_cast_for(
        state,
        source,
        if object.zone == Zone::Stack {
            object.controller
        } else {
            state.priority_player
        },
    )
}
pub(super) fn free_cast_for(state: &GameState, source: ObjectId, controller: PlayerId) -> bool {
    let Some(object) = state.objects.try_get(source) else {
        return false;
    };
    match object.zone {
        Zone::Exile => {
            state.exile.contains(&source)
                && active_permission_for(controller, source, state).is_some_and(|permission| {
                    permission.play_or_cast == PlayOrCast::Cast && permission.without_mana_cost.0
                })
        }
        Zone::Stack => object.v4.spell_cast_origin.is_some_and(|origin| {
            origin.origin_zone == Zone::Exile
                && origin.origin_zone_change_count.checked_add(1) == Some(object.zone_change_count)
                && match origin.route {
                    SpellCastRouteV4::ResolvingEffectV1 { holder, .. } => holder == controller,
                    SpellCastRouteV4::ExileFreePermission {
                        holder,
                        permission_zone_change_count,
                    } => {
                        holder == controller
                            && permission_zone_change_count == origin.origin_zone_change_count
                    }
                    _ => false,
                }
        }),
        _ => false,
    }
}
/// Only an instruction during resolution waives ordinary casting timing.
pub(super) fn timing_override(state: &GameState, source: ObjectId) -> bool {
    state.objects.try_get(source).is_some_and(|object| {
        object.zone == Zone::Stack
            && object.v4.spell_cast_origin.is_some_and(|origin| {
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
    form_allowed_for(
        state,
        source,
        method,
        if o.zone == Zone::Stack {
            o.controller
        } else {
            state.priority_player
        },
    )
}
pub(super) fn form_allowed_for(
    state: &GameState,
    source: ObjectId,
    method: CastMethodV4,
    controller: PlayerId,
) -> bool {
    if !free_cast_for(state, source, controller) {
        return true;
    }
    let o = state.objects.get(source);
    let maximum_mana_value =
        o.v4.spell_cast_origin
            .and_then(|origin| match origin.route {
                SpellCastRouteV4::ResolvingEffectV1 {
                    maximum_mana_value, ..
                } => maximum_mana_value,
                _ => None,
            });
    let def = &card_def::CARD_DEFS[o.card_def as usize];
    let value = match method {
        CastMethodV4::Normal => {
            crate::standard_cards_v1::room_cast_mana_value(def, method).unwrap_or(def.mana_value)
        }
        CastMethodV4::Alternative => {
            let Some(value) = crate::standard_cards_v1::room_cast_mana_value(def, method) else {
                return false;
            };
            value
        }
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

/// Complete legal targets before quoting all mandatory payments. The same
/// predicate filters actual free-cast form choices, so an offer cannot lead
/// into a form whose remaining mandatory choices are impossible.
pub(super) fn form_completable(
    state: &GameState,
    pending: &PendingCast,
    form: u8,
    kicked: bool,
) -> bool {
    let def = &card_def::CARD_DEFS[state.objects.get(pending.spell).card_def as usize];
    let room = crate::standard_cards_v1::room_cast_mana_value(def, CastMethodV4::Normal).is_some();
    if room && pending.cast_mode.is_none() {
        return [CastMode::Normal, CastMode::Alternative]
            .into_iter()
            .any(|mode| {
                let mut selected = pending.clone();
                selected.cast_mode = Some(mode);
                form_completable(state, &selected, form, kicked)
            });
    }
    let (method, types, keywords) =
        if form == 1 && (supported_adventure(def).is_some() || supported_omen(def).is_some()) {
            let types = supported_adventure(def)
                .map(|a| a.types)
                .or_else(|| supported_omen(def).map(|o| o.types))
                .unwrap();
            (CastMethodV4::Omen, types, Keywords::NONE)
        } else if form == 1 && supported_bestow(def).is_some() {
            return false;
        } else if room && pending.cast_mode == Some(CastMode::Alternative) {
            (CastMethodV4::Alternative, def.types, def.keywords)
        } else {
            (CastMethodV4::Normal, def.types, def.keywords)
        };
    if !form_allowed(state, pending.spell, method)
        || !pending_cast_form_timing_ok(types, keywords, pending, state)
    {
        return false;
    }
    let Some(spec) = selected_spell_target_spec(def, form, kicked) else {
        return false;
    };
    let mut selected = pending.clone();
    selected.mode_chosen = Some(form);
    selected.kicked = Some(kicked);
    fn complete(
        state: &GameState,
        def: &card_def::CardDef,
        pending: &mut PendingCast,
        method: CastMethodV4,
        spec: TargetSpec,
    ) -> bool {
        if pending.targets_chosen.len() >= usize::from(target_min_count(spec))
            && pending_cast_quote_v1(def, pending, method, pending.kicked == Some(true), 0, state)
                .is_some()
        {
            return true;
        }
        if pending.targets_chosen.len() >= usize::from(target_count(spec)) {
            return false;
        }
        for target in completable_next_cast_targets(def, pending, spec, state) {
            pending.targets_chosen.push(target);
            let payable = complete(state, def, pending, method, spec);
            pending.targets_chosen.pop();
            if payable {
                return true;
            }
        }
        false
    }
    complete(state, def, &mut selected, method, spec)
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
    let Some(pending) = projected.engine.pending_cast.as_ref() else {
        return false;
    };
    for kicked in [false, true] {
        if kicked && def.kicker_cost.is_none() {
            continue;
        }
        for form in 0..printed_spell_form_count(def) {
            if form_completable(&projected, pending, form, kicked) {
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
    // Capture the parent's preceding events before a child cast can change
    // their trigger conditions. Placement still waits for the whole parent.
    let captures = trigger::capture_triggers_without_sba(state);
    if state.engine.halted.is_some() {
        return Err("resolution play could not capture preceding triggers".into());
    }
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
    context.deferred_triggers.extend(captures);
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
    let captures = trigger::capture_triggers_without_sba(state);
    let triggers = std::mem::take(&mut state.engine.pending_triggers);
    let context = context_mut(state).unwrap();
    context.deferred_triggers.extend(triggers);
    context.deferred_triggers.extend(captures);
    state.engine.pending_effect = Some(*parent);
    true
}
pub(super) fn defer_triggers(state: &mut GameState) -> bool {
    if !active(state) {
        return false;
    }
    let captures = trigger::capture_triggers_without_sba(state);
    let triggers = std::mem::take(&mut state.engine.pending_triggers);
    let context = context_mut(state).unwrap();
    context.deferred_triggers.extend(triggers);
    context.deferred_triggers.extend(captures);
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

#[cfg(all(test, feature = "standard-magezero-fixtures"))]
mod tests {
    use super::*;

    fn ready() -> GameState {
        let forest = card_def::card_id_by_name("Forest").unwrap();
        let mut state = GameState::new_from_libraries(
            &[forest; 12],
            &[forest; 12],
            |_| "Forest".into(),
            0x6082,
        );
        state.step = Step::Main1;
        state
    }
    fn put(state: &mut GameState, name: &str, zone: Zone) -> ObjectId {
        let def = card_def::card_id_by_name(name).unwrap();
        let object = state.objects.push(crate::state::GameObject {
            card_def: def,
            name: card_def::CARD_DEFS[def as usize].object_name.into(),
            owner: PlayerId::P0,
            controller: PlayerId::P0,
            zone,
            tapped: false,
            summoning_sick: false,
            damage: 0,
            counters: Default::default(),
            attachments: vec![],
            v4: ObjectStateV4::from_card_def(def),
            spell_copy_origin: None,
            plotted_turn: None,
            zone_change_count: 0,
        });
        match zone {
            Zone::Exile => state.exile.push(object),
            Zone::Battlefield => state.players[0].battlefield.push(object),
            Zone::Hand => state.players[0].hand.push(object),
            _ => panic!("helper zone"),
        }
        object
    }
    fn grant(state: &mut GameState, source: ObjectId) {
        state.engine.exile_play_permissions.push(PlayPermission {
            object: source,
            holder: PlayerId::P0,
            zone_change_generation: state.objects.get(source).zone_change_count,
            play_or_cast: PlayOrCast::Cast,
            expiry: PlayPermissionExpiry::EndOfTurn,
            without_mana_cost: FreeCastV1(true),
        });
    }
    fn bind(state: &GameState, source: ObjectId) -> EffectObjectBinding {
        EffectObjectBinding {
            object: source,
            expected_zone: Zone::Exile,
            expected_zone_change_count: state.objects.get(source).zone_change_count,
        }
    }

    #[test]
    fn persistent_permission_is_free_with_exact_holder_and_generation_but_ordinary_timing() {
        let mut state = ready();
        let hajar = put(&mut state, "Hajar, Loyal Bodyguard", Zone::Exile);
        grant(&mut state, hajar);
        assert!(free_cast_for(&state, hajar, PlayerId::P0));
        assert!(!free_cast_for(&state, hajar, PlayerId::P1));
        state.engine.exile_play_permissions[0].zone_change_generation += 1;
        assert!(!free_cast_for(&state, hajar, PlayerId::P0));
        state.engine.exile_play_permissions[0].zone_change_generation -= 1;
        state.step = Step::DeclareAttackers;
        assert!(!is_castable_now(
            PlayerId::P0,
            hajar,
            CastMethodV4::Normal,
            &state
        ));
        state.step = Step::Main2;
        assert!(is_castable_now(
            PlayerId::P0,
            hajar,
            CastMethodV4::Normal,
            &state
        ));
        begin_cast_ex(&mut state, PlayerId::P0, hajar, Some(CastMethodV4::Normal));
        assert!(free_cast(&state, hajar));
        assert!(!timing_override(&state, hajar));
        let mut restored: GameState =
            serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
        assert_eq!(
            advance_until_decision(&mut state),
            advance_until_decision(&mut restored)
        );
        assert_eq!(state, restored);
        assert!(state.engine.pending_cast.is_none());
        assert!(matches!(
            state
                .stack
                .last()
                .unwrap()
                .v4
                .source_contract
                .unwrap()
                .spell_cast_origin
                .unwrap()
                .route,
            SpellCastRouteV4::ExileFreePermission { .. }
        ));
        assert_eq!(state.players[0].mana_pool, [0; 6]);
    }

    #[test]
    fn persistent_free_cast_keeps_only_the_instant_adventure_form_at_instant_timing() {
        let mut state = ready();
        state.step = Step::End;
        let druid = put(&mut state, "Questing Druid", Zone::Exile);
        grant(&mut state, druid);
        assert!(is_castable_now(
            PlayerId::P0,
            druid,
            CastMethodV4::Normal,
            &state
        ));
        begin_cast_ex(&mut state, PlayerId::P0, druid, Some(CastMethodV4::Normal));
        let pending = state.engine.pending_cast.as_ref().unwrap();
        let def = &card_def::CARD_DEFS[state.objects.get(druid).card_def as usize];
        assert_eq!(viable_pending_spell_forms(def, pending, &state), vec![1]);
        assert!(matches!(
            advance_until_decision(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
        assert_eq!(
            state.stack.last().unwrap().v4.cast_method,
            Some(CastMethodV4::Omen)
        );
    }

    #[test]
    fn free_cast_fixes_x_at_zero_and_disallows_bestow_even_with_spare_mana() {
        let mut state = ready();
        let hydra = put(&mut state, "Nyxborn Hydra", Zone::Exile);
        grant(&mut state, hydra);
        state.players[0].mana_pool = [0, 0, 0, 0, 3, 9];
        assert!(!form_allowed_for(
            &state,
            hydra,
            CastMethodV4::Bestow,
            PlayerId::P0
        ));
        begin_cast_ex(&mut state, PlayerId::P0, hydra, Some(CastMethodV4::Normal));
        let pending = state.engine.pending_cast.as_ref().unwrap();
        let def = &card_def::CARD_DEFS[state.objects.get(hydra).card_def as usize];
        assert_eq!(pending.x_value, Some(0));
        assert_eq!(viable_pending_spell_forms(def, pending, &state), vec![0]);
        assert!(
            pending_cast_quote_v1(def, pending, CastMethodV4::Normal, false, 1, &state).is_none()
        );
        assert!(matches!(
            advance_until_decision(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
        assert_eq!(state.stack.last().unwrap().v4.x_value, 0);
        assert_eq!(state.players[0].mana_pool, [0, 0, 0, 0, 3, 9]);
    }

    #[test]
    fn resolution_preflight_requires_mandatory_targets_sacrifices_and_discards() {
        let mut state = ready();
        state.step = Step::DeclareBlockers;
        let fatal = put(&mut state, "Fatal Push", Zone::Exile);
        let fling = put(&mut state, "Fling", Zone::Exile);
        let thrill = put(&mut state, "Thrill of Possibility", Zone::Exile);
        for source in [fatal, fling, thrill] {
            assert!(!can_cast_exiled_without_mana(
                &state,
                PlayerId::P0,
                bind(&state, source),
                None
            ));
        }
        put(&mut state, "Llanowar Elves", Zone::Battlefield);
        put(&mut state, "Forest", Zone::Hand);
        for source in [fatal, fling, thrill] {
            assert!(can_cast_exiled_without_mana(
                &state,
                PlayerId::P0,
                bind(&state, source),
                None
            ));
        }
    }

    #[test]
    fn resolution_preflight_requires_spree_surcharges_and_respects_discover_spell_value() {
        let mut state = ready();
        let raid = put(&mut state, "Requisition Raid", Zone::Exile);
        assert!(!can_cast_exiled_without_mana(
            &state,
            PlayerId::P0,
            bind(&state, raid),
            None
        ));
        state.players[0].mana_pool[5] = 1;
        assert!(can_cast_exiled_without_mana(
            &state,
            PlayerId::P0,
            bind(&state, raid),
            Some(1)
        ));
        assert!(!can_cast_exiled_without_mana(
            &state,
            PlayerId::P0,
            bind(&state, raid),
            Some(0)
        ));
    }

    #[test]
    fn discover_cast_limit_checks_the_selected_room_door() {
        // Discover searches using the card's combined value (8). Its cast
        // instruction independently constrains the selected spell's value.
        for (limit, expected) in [
            (Some(2), vec![]),
            (Some(3), vec![CastMode::Normal]),
            (Some(4), vec![CastMode::Normal]),
            (Some(5), vec![CastMode::Normal, CastMode::Alternative]),
            (None, vec![CastMode::Normal, CastMode::Alternative]),
        ] {
            let mut state = ready();
            state.step = Step::DeclareBlockers;
            let room = put(&mut state, "Unholy Annex // Ritual Chamber", Zone::Exile);
            let card = bind(&state, room);
            assert_eq!(object_mana_value(&state, room), 8);
            assert_eq!(
                can_cast_exiled_without_mana(&state, PlayerId::P0, card, limit),
                !expected.is_empty()
            );
            state
                .standard_v1
                .get_or_insert_with(Default::default)
                .resolution_play = Some(ResolutionPlayV1 {
                parent: StackItemId(0),
                parent_index: 0,
                suspended: None,
                permission: Some(ResolutionPermissionV1 {
                    card,
                    controller: PlayerId::P0,
                    maximum_mana_value: limit,
                    land: false,
                }),
                deferred_triggers: Vec::new(),
                kicked_source: None,
            });
            begin_cast_ex(&mut state, PlayerId::P0, room, Some(CastMethodV4::Normal));
            let pending = state.engine.pending_cast.as_ref().unwrap();
            let def = &card_def::CARD_DEFS[state.objects.get(room).card_def as usize];
            assert_eq!(pending.cast_mode, None);
            assert_eq!(pending.x_value, Some(0));
            assert_eq!(payable_cast_modes(def, pending, &state), expected);
            for (mode, method) in [
                (CastMode::Normal, CastMethodV4::Normal),
                (CastMode::Alternative, CastMethodV4::Alternative),
            ] {
                let mut selected = pending.clone();
                selected.cast_mode = Some(mode);
                assert_eq!(
                    form_completable(&state, &selected, 0, false),
                    expected.contains(&mode)
                );
                assert!(pending_cast_quote_v1(def, &selected, method, false, 1, &state).is_none());
            }
        }
    }

    #[test]
    fn persistent_free_permission_allows_either_room_door_at_normal_timing() {
        let mut state = ready();
        let room = put(&mut state, "Unholy Annex // Ritual Chamber", Zone::Exile);
        grant(&mut state, room);
        state.step = Step::DeclareBlockers;
        assert!(!is_castable_now(
            PlayerId::P0,
            room,
            CastMethodV4::Normal,
            &state
        ));
        state.step = Step::Main2;
        assert!(is_castable_now(
            PlayerId::P0,
            room,
            CastMethodV4::Normal,
            &state
        ));
        begin_cast_ex(&mut state, PlayerId::P0, room, Some(CastMethodV4::Normal));
        assert!(matches!(advance_until_decision(&mut state),
            Decision::ChooseCastMode { ref options, .. }
                if options == &[CastMode::Normal, CastMode::Alternative]));
        step(&mut state, Action::ChooseCastMode(CastMode::Alternative)).unwrap();
        assert!(matches!(
            advance_until_decision(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
        let item = state.stack.last().unwrap();
        assert_eq!(item.v4.cast_method, Some(CastMethodV4::Alternative));
        assert_eq!(object_mana_value(&state, room), 5);
        validate_spell_stack_source(&state, item).unwrap();
        assert_eq!(state.players[0].mana_pool, [0; 6]);
    }
}
