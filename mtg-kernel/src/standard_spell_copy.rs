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
        || spell.source != source.source
        || spell.controller != source.controller
        || source.card_def != live.card_def
        || source.owner != live.owner
        || source.zone != Zone::Stack
        || spell.v4.cast_method != Some(source.cast_method)
        || source
            .finalized_cast_binding
            .is_some_and(|binding| binding.x_value != spell.v4.x_value)
        || live.zone_change_count < source.zone_change_count
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
    let def = &card_def::CARD_DEFS[source.card_def as usize];
    let spec = if spell.v4.cast_method == Some(CastMethodV4::Omen) {
        if spell.mode_chosen != 0 {
            return Err("copy snapshot alternative form has a mode".into());
        }
        supported_adventure(def)
            .map(|x| x.target_spec)
            .or_else(|| supported_omen(def).map(|x| x.target_spec))
            .ok_or("copy snapshot lost alternative spell")?
    } else if spell.v4.cast_method == Some(CastMethodV4::Bestow) {
        supported_bestow(def)
            .ok_or("copy snapshot lost Bestow form")?
            .target_spec
    } else {
        def.printed_mode_target(spell.mode_chosen, spell.kicked)
            .ok_or("copy snapshot mode missing")?
    };
    if spell.v4.target_spec != Some(spec) || spell.targets.len() > usize::from(target_count(spec)) {
        return Err("copy snapshot changed its definition-owned targets".into());
    }
    if let Some(kind) = spell.v4.optional_additional_cost_paid {
        if def.optional_additional_cost != Some(kind) {
            return Err("copy snapshot additional cost changed".into());
        }
        validate_optional_additional_paid_refs(kind, spell.controller, &spell.v4.paid_cost_refs)?;
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
            object: spell.source,
            card_def: source.card_def,
            zone: Zone::Stack,
            zone_change_count: source.zone_change_count,
            other_than: None,
        }),
        state,
    ))
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
    validate_spell_stack_source(state, &copy)?;
    state.stack.push(copy);
    log_final_targeting_events(state, id)?;
    Ok(())
}
