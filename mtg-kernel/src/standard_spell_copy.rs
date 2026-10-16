//! Copy a frozen spell independently of its original stack lifetime.
use super::*;

pub(crate) fn validate_copy_snapshot(state: &GameState, spell: &StackItem) -> Result<(), String> {
    let source = spell
        .v4
        .source_contract
        .ok_or("copy snapshot lost spell source")?;
    let live = state
        .objects
        .try_get(source.source)
        .ok_or("copy snapshot source missing")?;
    if spell.kind != StackItemKind::Spell
        || spell.inline_effect.is_some()
        || spell.madness_offer
        || spell.v4.activated_ability_index.is_some()
        || spell.v4.ability_source_contract.is_some()
        || spell.v4.madness_source_contract.is_some()
        || spell.v4.hidden_ability_source.is_some()
        || spell.v4.granted_by.is_some()
        || !spell.v4.cauldron_grant.is_empty()
        || spell.source != source.source
        || spell.controller != source.controller
        || source.card_def != live.card_def
        || source.owner != live.owner
        || source.zone != Zone::Stack
        || spell.is_copy != source.spell_copy_origin.is_some()
        || source.spell_copy_origin != live.spell_copy_origin
        || spell.v4.cast_method != Some(source.cast_method)
        || spell.is_flashback != (source.cast_method == CastMethodV4::Flashback)
        || source
            .finalized_cast_binding
            .is_some_and(|binding| binding.x_value != spell.v4.x_value)
        || live.zone_change_count < source.zone_change_count
        || (live.zone_change_count == source.zone_change_count
            && (live.zone != Zone::Stack || live.controller != source.controller))
        || spell.v4.stack_item_id == StackItemId::default()
        || spell.targets.len() != spell.v4.target_contracts.len()
        || !spell
            .targets
            .iter()
            .zip(&spell.v4.target_contracts)
            .all(|(target, contract)| *target == contract.target())
    {
        return Err("copy snapshot changed its spell source or targets".into());
    }
    if let Some(current) = state
        .stack
        .iter()
        .find(|item| item.v4.stack_item_id == spell.v4.stack_item_id)
    {
        if current != spell {
            return Err("copy snapshot differs from its original spell".into());
        }
    }
    let def = card_def::CARD_DEFS
        .get(source.card_def as usize)
        .filter(|def| def.is_executable() && def.is_castable())
        .ok_or("copy snapshot definition is not an executable spell")?;
    if let Some(origin) = source.spell_copy_origin {
        if source.spell_cast_origin.is_some()
            || source.finalized_cast_binding.is_some()
            || !matches!(
                source.cast_method,
                CastMethodV4::Normal | CastMethodV4::Omen | CastMethodV4::Bestow
            )
        {
            return Err("copy snapshot mixes virtual-copy and physical-cast provenance".into());
        }
        validate_spell_copy_origin_chain(state, spell.source, source.card_def, origin)?;
    } else {
        let origin = source
            .spell_cast_origin
            .ok_or("copy snapshot lost its physical cast origin")?;
        if origin.origin_zone_change_count.checked_add(1) != Some(source.zone_change_count)
            || origin.finalized_method != Some(source.cast_method)
        {
            return Err("copy snapshot changed its physical cast incarnation".into());
        }
    }
    let spec = if spell.v4.cast_method == Some(CastMethodV4::Omen) {
        if spell.mode_chosen != 0 {
            return Err("copy snapshot alternative form has a mode".into());
        }
        supported_adventure(def)
            .map(|x| x.target_spec)
            .or_else(|| supported_omen(def).map(|x| x.target_spec))
            .ok_or("copy snapshot lost alternative spell")?
    } else if spell.v4.cast_method == Some(CastMethodV4::Bestow) {
        if spell.mode_chosen != 0 {
            return Err("copy snapshot Bestow form has a mode".into());
        }
        supported_bestow(def)
            .ok_or("copy snapshot lost Bestow form")?
            .target_spec
    } else {
        def.printed_mode_target(spell.mode_chosen, spell.kicked)
            .ok_or("copy snapshot mode missing")?
    };
    let cardinality_complete = if spec == TargetSpec::KickerControlledCreatures {
        let (min, max) = cast_target_bounds(spec, spell.kicked);
        spell.targets.len() >= min && max.is_none_or(|max| spell.targets.len() <= max)
    } else {
        target_cardinality_is_complete(spec, spell.targets.len())
    };
    let unique = spec != TargetSpec::KickerControlledCreatures
        || spell.v4.target_contracts.iter().enumerate().all(|(i, c)| {
            !spell.v4.target_contracts[..i]
                .iter()
                .any(|old| copy_target_identity_eq(*old, *c))
        }) && (spell.is_copy
            || spell
                .targets
                .iter()
                .enumerate()
                .all(|(i, t)| !spell.targets[..i].contains(t)));
    if spell.v4.target_spec != Some(spec) || !cardinality_complete || !unique {
        return Err("copy snapshot changed its definition-owned targets".into());
    }
    if let Some(kind) = spell.v4.optional_additional_cost_paid {
        if spell.is_copy || def.optional_additional_cost != Some(kind) {
            return Err("copy snapshot additional cost changed".into());
        }
        validate_optional_additional_paid_refs(kind, spell.controller, &spell.v4.paid_cost_refs)?;
    }
    for (index, (&target, &contract)) in spell
        .targets
        .iter()
        .zip(&spell.v4.target_contracts)
        .enumerate()
    {
        if !stack_target_contract_is_structurally_valid(state, spec, index, target, contract) {
            return Err("copy snapshot target contract is structurally malformed".into());
        }
    }
    Ok(())
}

pub(crate) fn copy_snapshot_legal_targets(
    state: &GameState,
    spell: &StackItem,
    controller: PlayerId,
    prefix: &[Target],
) -> Result<Vec<Target>, String> {
    validate_copy_snapshot(state, spell)?;
    let source = spell
        .v4
        .source_contract
        .ok_or("copy snapshot source missing")?;
    Ok(legal_targets_for_controller_from_source(
        spell.v4.target_spec.ok_or("copy target spec missing")?,
        prefix,
        controller,
        Some(TargetingSource {
            types_override: None,
            object: spell.source,
            card_def: source.card_def,
            zone: Zone::Stack,
            zone_change_count: source.zone_change_count,
            power_lki: None,
            other_than: None,
            original_source_incarnation_only: false,
        }),
        state,
    ))
}

/// Target identity follows CR 400.7, not mutable controller metadata or just
/// the arena slot. A retained departed object and its returned incarnation are
/// different targets; two bindings of the same incarnation are the same target.
pub(super) fn copy_target_identity_eq(a: StackTargetContractV4, b: StackTargetContractV4) -> bool {
    match (a, b) {
        (StackTargetContractV4::Player(a), StackTargetContractV4::Player(b)) => a == b,
        (
            StackTargetContractV4::Object {
                object: a,
                zone_change_count: ag,
                ..
            },
            StackTargetContractV4::Object {
                object: b,
                zone_change_count: bg,
                ..
            },
        ) => a == b && ag == bg,
        (
            StackTargetContractV4::StackItem {
                stack_item_id: a, ..
            },
            StackTargetContractV4::StackItem {
                stack_item_id: b, ..
            },
        ) => a == b,
        _ => false,
    }
}

/// A replacement slot is fresh; an unchanged slot retains its exact old contract.
/// Legality is evaluated against the final assignment, including dependent slots.
fn copy_assignment_slot_legal(
    state: &GameState,
    spell: &StackItem,
    controller: PlayerId,
    targets: &[Target],
    contracts: &[StackTargetContractV4],
    index: usize,
) -> Result<bool, String> {
    if !target_contract_matches_live(state, targets[index], contracts[index]) {
        return Ok(false);
    }
    let spec = spell.v4.target_spec.ok_or("copy target spec absent")?;
    if spec == TargetSpec::TwoCreaturesWithDifferentControllers {
        if targets.len() != 2 || targets[0] == targets[1] {
            return Ok(false);
        }
        let controllers = std::array::from_fn::<_, 2, _>(|i| {
            if target_contract_matches_live(state, targets[i], contracts[i]) {
                if let Target::Object(object) = targets[i] {
                    if battlefield_objects(state).any(|id| id == object) {
                        return Some(state.objects.get(object).controller);
                    }
                }
            }
            let mut found = state.engine.event_history.iter().filter_map(|e| match e {
                event::CommittedEvent::DifferentControllerTargetDepartureV1 {
                    target,
                    controller_before,
                } if *target == contracts[i] => Some(*controller_before),
                _ => None,
            });
            let first = found.next()?;
            found.next().is_none().then_some(first)
        });
        if !matches!(controllers,[Some(a),Some(b)] if a!=b) {
            return Ok(false);
        }
        return Ok(
            copy_snapshot_legal_targets(state, spell, controller, &[])?.contains(&targets[index])
        );
    }
    let prefix = if spec == TargetSpec::KickerControlledCreatures {
        &[][..]
    } else {
        &targets[..index]
    };
    Ok(copy_snapshot_legal_targets(state, spell, controller, prefix)?.contains(&targets[index]))
}
pub(crate) fn copy_snapshot_assignment_is_legal(
    state: &GameState,
    spell: &StackItem,
    controller: PlayerId,
    targets: &[Target],
    contracts: &[StackTargetContractV4],
) -> Result<bool, String> {
    validate_copy_snapshot(state, spell)?;
    if targets.iter().enumerate().any(|(i, t)| {
        let unchanged =
            spell.targets.get(i) == Some(t) && spell.v4.target_contracts.get(i) == contracts.get(i);
        !unchanged
            && match t {
                Target::Object(id) => state.objects.try_get(*id).is_none(),
                Target::StackItem(id) => !state.stack.iter().any(|r| r.v4.stack_item_id == *id),
                Target::Player(_) => false,
            }
    }) {
        return Ok(false);
    }
    if targets.len() != spell.targets.len()
        || contracts.len() != targets.len()
        || !targets.iter().zip(contracts).all(|(t, c)| *t == c.target())
    {
        return Ok(false);
    }
    if spell.v4.target_spec == Some(TargetSpec::KickerControlledCreatures)
        && contracts.iter().enumerate().any(|(i, c)| {
            contracts[..i]
                .iter()
                .any(|old| copy_target_identity_eq(*old, *c))
        })
    {
        return Ok(false);
    }
    for i in 0..targets.len() {
        let unchanged =
            targets[i] == spell.targets[i] && contracts[i] == spell.v4.target_contracts[i];
        let now = copy_assignment_slot_legal(state, spell, controller, targets, contracts, i)?;
        if unchanged {
            // An unchanged illegal original may remain. New selections must not
            // make an originally legal unchanged target illegal.
            if copy_assignment_slot_legal(
                state,
                spell,
                controller,
                &spell.targets,
                &spell.v4.target_contracts,
                i,
            )? && !now
            {
                return Ok(false);
            }
        } else if contracts[i] != StackTargetContractV4::capture(state, targets[i]) || !now {
            return Ok(false);
        }
    }
    Ok(true)
}
pub(crate) fn copy_snapshot_replacement_targets(
    state: &GameState,
    spell: &StackItem,
    controller: PlayerId,
    prefix: &[Target],
) -> Result<Vec<Target>, String> {
    // The different-controller relation depends on the final pair, not the
    // stale live arena slot of a retained first target. Final-set validation
    // below uses the exact native departure LKI for that original incarnation.
    let prefix = if matches!(
        spell.v4.target_spec,
        Some(
            TargetSpec::TwoCreaturesWithDifferentControllers
                | TargetSpec::KickerControlledCreatures
        )
    ) {
        &[][..]
    } else {
        prefix
    };
    copy_snapshot_legal_targets(state, spell, controller, prefix)
}
pub(crate) fn copy_snapshot_prefix_can_complete(
    state: &GameState,
    spell: &StackItem,
    controller: PlayerId,
    prefix: &[Target],
    contracts: &[StackTargetContractV4],
) -> Result<bool, String> {
    validate_copy_snapshot(state, spell)?;
    if prefix.iter().enumerate().any(|(i, t)| {
        let unchanged =
            spell.targets.get(i) == Some(t) && spell.v4.target_contracts.get(i) == contracts.get(i);
        !unchanged
            && match t {
                Target::Object(id) => state.objects.try_get(*id).is_none(),
                Target::StackItem(id) => !state.stack.iter().any(|r| r.v4.stack_item_id == *id),
                Target::Player(_) => false,
            }
    }) {
        return Ok(false);
    }
    if prefix.len() > spell.targets.len()
        || prefix.len() != contracts.len()
        || !prefix.iter().zip(contracts).all(|(t, c)| *t == c.target())
    {
        return Ok(false);
    }
    if prefix.len() == spell.targets.len() {
        return copy_snapshot_assignment_is_legal(state, spell, controller, prefix, contracts);
    }
    if spell.v4.target_spec == Some(TargetSpec::KickerControlledCreatures) {
        // Exact matching specialization: every replacement edge shares the
        // current legal incarnation pool, plus each slot's unique original keep
        // edge. Controller metadata does not change object identity (CR400.7).
        let originals = &spell.v4.target_contracts;
        if originals.iter().enumerate().any(|(i, c)| {
            originals[..i]
                .iter()
                .any(|old| copy_target_identity_eq(*old, *c))
        }) || contracts.iter().enumerate().any(|(i, c)| {
            contracts[..i]
                .iter()
                .any(|old| copy_target_identity_eq(*old, *c))
        }) {
            return Ok(false);
        }
        let legal = copy_snapshot_legal_targets(state, spell, controller, &[])?;
        for (i, target) in prefix.iter().enumerate() {
            if (*target != spell.targets[i] || contracts[i] != originals[i])
                && (contracts[i] != StackTargetContractV4::capture(state, *target)
                    || !legal.contains(target))
            {
                return Ok(false);
            }
        }
        let mut pool = legal
            .into_iter()
            .map(|t| StackTargetContractV4::capture(state, t))
            .collect::<Vec<_>>();
        pool.retain(|c| {
            !contracts
                .iter()
                .any(|used| copy_target_identity_eq(*used, *c))
        });
        let own_outside = originals[prefix.len()..]
            .iter()
            .filter(|c| {
                !contracts
                    .iter()
                    .any(|used| copy_target_identity_eq(*used, **c))
                    && !pool
                        .iter()
                        .any(|fresh| copy_target_identity_eq(*fresh, **c))
            })
            .count();
        return Ok(spell.targets.len() - prefix.len() - own_outside <= pool.len());
    }
    let i = prefix.len();
    let mut selected = prefix.to_vec();
    let mut bindings = contracts.to_vec();
    selected.push(spell.targets[i]);
    bindings.push(spell.v4.target_contracts[i]);
    if copy_snapshot_prefix_can_complete(state, spell, controller, &selected, &bindings)? {
        return Ok(true);
    }
    for target in copy_snapshot_replacement_targets(state, spell, controller, prefix)? {
        let binding = StackTargetContractV4::capture(state, target);
        if target == spell.targets[i] && binding == spell.v4.target_contracts[i] {
            continue;
        }
        selected[i] = target;
        bindings[i] = binding;
        if copy_snapshot_prefix_can_complete(state, spell, controller, &selected, &bindings)? {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(crate) fn materialize_copy_snapshot(
    state: &mut GameState,
    spell: &StackItem,
    controller: PlayerId,
    targets: Vec<Target>,
    contracts: Vec<StackTargetContractV4>,
) -> Result<(), String> {
    validate_copy_snapshot(state, spell)?;
    if targets.len() != spell.targets.len()
        || contracts.len() != targets.len()
        || !targets
            .iter()
            .zip(&contracts)
            .all(|(t, c)| *t == c.target())
    {
        return Err("copy target cardinality changed".into());
    }
    if !copy_snapshot_assignment_is_legal(state, spell, controller, &targets, &contracts)? {
        return Err("copy final target assignment is not legal".into());
    }
    // Final validation needs the new object and stack membership. Build both
    // privately so any source, target, or stack-ID rejection leaves the arena,
    // ID allocator, stack, and targeting event log unchanged.
    let mut staged = state.clone();
    materialize_validated_copy_snapshot(&mut staged, spell, controller, targets, contracts)?;
    *state = staged;
    Ok(())
}

fn materialize_validated_copy_snapshot(
    state: &mut GameState,
    spell: &StackItem,
    controller: PlayerId,
    targets: Vec<Target>,
    contracts: Vec<StackTargetContractV4>,
) -> Result<(), String> {
    let origin = spell.v4.source_contract.ok_or("copy source missing")?;
    let source = state.objects.push(crate::state::GameObject {
        card_def: origin.card_def,
        name: state.objects.get(origin.source).name.clone(),
        owner: controller,
        controller,
        zone: Zone::Stack,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: Vec::new(),
        v4: ObjectStateV4::from_card_def(origin.card_def),
        spell_copy_origin: Some(SpellCopyOriginV4 {
            parent: origin.source,
            parent_card_def: origin.card_def,
            parent_owner: origin.owner,
            parent_controller: origin.controller,
            parent_stack_zone_change_count: origin.zone_change_count,
            parent_was_copy: spell.is_copy,
            source_types_v1: crate::source_type_hexproof_v1::copy_projection(
                origin.card_def,
                spell.v4.cast_method.unwrap_or_default(),
            ),
        }),
        plotted_turn: None,
        zone_change_count: 0,
    });
    let mut copy = spell.clone();
    copy.source = source;
    copy.controller = controller;
    copy.targets = targets;
    copy.is_copy = true;
    copy.is_flashback = false;
    copy.v4.stack_item_id = next_stack_item_id(state);
    // Alternative characteristics are copied; the original payment route is not.
    let method = match spell.v4.cast_method {
        Some(CastMethodV4::Omen) => CastMethodV4::Omen,
        Some(CastMethodV4::Bestow) => CastMethodV4::Bestow,
        _ => CastMethodV4::Normal,
    };
    copy.v4.cast_method = Some(method);
    copy.v4.source_contract = Some(StackSourceContractV4::capture(state, source, method));
    copy.v4.target_contracts = contracts;
    copy.v4.optional_additional_cost_paid = None;
    // Additional costs are not paid again; their choices and X are copied.
    let id = copy.v4.stack_item_id;
    // The object and source contract exist now; stack membership does not
    // exist until the insertion below. The targeting logger then checks the
    // complete stack contract, including that unique membership.
    validate_spell_source_contract_fields(state, &copy)?;
    state.stack.push(copy);
    log_final_targeting_events(state, id)?;
    crate::extra_turn_v1::record_publication(state, id);
    Ok(())
}

#[cfg(all(test, feature = "standard-magezero-fixtures"))]
mod tests {
    use super::*;

    fn countered_spell_snapshot() -> (GameState, StackItem) {
        let mountain = card_def::card_id_by_name("Mountain").unwrap();
        let mut state = GameState::new_from_libraries(
            &[mountain; 4],
            &[mountain; 4],
            |_| "Mountain".into(),
            0xC0F1,
        );
        state.step = Step::Main1;
        state.active_player = PlayerId::P0;
        state.priority_player = PlayerId::P0;
        let card_def = card_def::card_id_by_name("Lightning Strike").unwrap();
        let source = state.objects.push(crate::state::GameObject {
            card_def,
            name: "Lightning Strike".into(),
            owner: PlayerId::P0,
            controller: PlayerId::P0,
            zone: Zone::Hand,
            tapped: false,
            summoning_sick: false,
            damage: 0,
            counters: Default::default(),
            attachments: Vec::new(),
            v4: ObjectStateV4::from_card_def(card_def),
            spell_copy_origin: None,
            plotted_turn: None,
            zone_change_count: 0,
        });
        state.players[0].hand.push(source);
        state.players[0].mana_pool[mana::ManaColor::R.pool_index()] = 1;
        state.players[0].mana_pool[5] = 1;
        step(&mut state, Action::CastSpell(source)).unwrap();
        assert!(matches!(
            advance_until_decision(&mut state),
            Decision::ChooseTargets { .. }
        ));
        step(
            &mut state,
            Action::ChooseTarget(Target::Player(PlayerId::P1)),
        )
        .unwrap();
        assert!(matches!(
            advance_until_decision(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
        let spell = state.stack.last().unwrap().clone();
        counter_stack_item_by_id(&mut state, spell.v4.stack_item_id)
            .unwrap()
            .unwrap();
        assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
        assert!(state.stack.is_empty());
        validate_copy_snapshot(&state, &spell).unwrap();
        (state, spell)
    }

    #[test]
    fn countered_snapshot_rejects_copy_flag_and_activation_metadata_without_mutation() {
        let (state, original) = countered_spell_snapshot();
        for corrupt_copy_flag in [true, false] {
            let mut state: GameState =
                serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
            let mut spell = original.clone();
            if corrupt_copy_flag {
                spell.is_copy = true;
            } else {
                spell.v4.activated_ability_index = Some(0);
            }
            let before = state.clone();
            assert!(validate_copy_snapshot(&state, &spell).is_err());
            assert!(materialize_copy_snapshot(
                &mut state,
                &spell,
                PlayerId::P0,
                spell.targets.clone(),
                spell.v4.target_contracts.clone(),
            )
            .is_err());
            assert_eq!(state, before, "rejected historical snapshot must be atomic");
        }
    }

    #[test]
    fn countered_snapshot_late_target_rejection_is_atomic_and_valid_copy_still_resolves() {
        let (mut state, spell) = countered_spell_snapshot();
        let library_card = state.players[0].library[0];
        let invalid_target = Target::Object(library_card);
        let invalid_contract = StackTargetContractV4::capture(&state, invalid_target);
        let before = state.clone();
        assert!(materialize_copy_snapshot(
            &mut state,
            &spell,
            PlayerId::P0,
            vec![invalid_target],
            vec![invalid_contract],
        )
        .is_err());
        assert_eq!(
            state, before,
            "final stack validation cannot leave an object, ID, or targeting event"
        );

        let mana_before = state.players[0].mana_pool;
        materialize_copy_snapshot(
            &mut state,
            &spell,
            PlayerId::P0,
            spell.targets.clone(),
            spell.v4.target_contracts.clone(),
        )
        .unwrap();
        let copy = state.stack.last().unwrap();
        assert!(copy.is_copy);
        assert_ne!(copy.source, spell.source);
        assert_eq!(state.objects.get(spell.source).zone, Zone::Graveyard);
        validated_stack_item_target_spec(copy, &state).unwrap();
        let mut restored: GameState =
            serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
        for state in [&mut state, &mut restored] {
            step(state, Action::Pass).unwrap();
            assert!(matches!(
                advance_until_decision(state),
                Decision::CastSpellOrPass { .. }
            ));
            step(state, Action::Pass).unwrap();
            assert!(matches!(
                advance_until_decision(state),
                Decision::CastSpellOrPass { .. }
            ));
            assert!(state.stack.is_empty());
            assert_eq!(state.players[1].life, 17);
            assert_eq!(state.players[0].mana_pool, mana_before);
        }
        assert_eq!(state, restored);
    }
}
