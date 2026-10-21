//! Native Genesis Wave's revealed subset and simultaneous permanent entry.
//!
//! The journal records a choice issued by the lexical native row owner. It is
//! not a public execution permit. Existing entry replay owns the entire native
//! Step/Advance, including Aura hosts and simultaneous as-enters choices.
use super::*;

pub(crate) fn definition(id: u16) -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && id == 487
        && card_def::CARD_DEFS.get(usize::from(id)).is_some_and(|d| {
            crate::rules_meaning_graveyard_return_v61::ordinary_foreign_fields_absent(d)
                && d.name == "Genesis Wave"
                && d.capability == card_def::CardCapability::Partial
                && d.cost
                    == crate::mana::Cost {
                        pips: &[
                            crate::mana::Pip::Colored(ManaColor::G),
                            crate::mana::Pip::Colored(ManaColor::G),
                            crate::mana::Pip::Colored(ManaColor::G),
                        ],
                        generic: 0,
                        x_count: 1,
                    }
                && d.types == [CardType::Sorcery]
                && d.subtypes.is_empty()
                && d.supertypes.is_empty()
                && d.power.is_none()
                && d.toughness.is_none()
                && d.mana_value == 3
                && !d.is_token
                && d.produces_mana.is_empty()
                && d.colors == [ManaColor::G]
                && d.target_spec == card_def::TargetSpec::None
                && d.keywords == Keywords::NONE
                && (d.spell_effect)() == Some(EffectOp::GenesisWaveV1)
                && crate::trigger::triggers_for(id).is_empty()
        })
}
pub(crate) fn recipe(op: &EffectOp) -> bool {
    *op == EffectOp::GenesisWaveV1
}
pub(crate) fn selected(state: &GameState, item: &StackItem) -> bool {
    item.inline_effect.as_ref().is_some_and(recipe)
        || item.kind == crate::state::StackItemKind::Spell
            && item
                .v4
                .source_contract
                .is_some_and(|c| definition(c.card_def))
        || state
            .engine
            .event_history
            .has_any(&[crate::event_history_v1::EventKindV1::GenesisWaveProgressV1])
            && state.engine.event_history.iter().any(|e| {
                matches!(e,
            event::CommittedEvent::GenesisWaveProgressV1 { item: original, .. }
            if original.v4.stack_item_id == item.v4.stack_item_id)
            })
}
pub(crate) fn waiting(state: &GameState, ctx: &ExecCtx) -> bool {
    state.stack.iter().any(|item| {
        selected(state, item)
            && (item.source == ctx.source || Some(item.v4.stack_item_id) == ctx.stack_item_id)
    })
}
pub(crate) fn pending_selected(state: &GameState, pending: &EffectContinuation) -> bool {
    selected(state, &pending.resolving_item) || waiting(state, &pending.ctx)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChoiceV1 {
    original_library: Vec<EffectObjectBinding>,
    prefix: Vec<EffectObjectBinding>,
    revealed: Vec<crate::genesis_wave_public_v69::RevealedV69>,
    attachment_sources: Vec<crate::rl::CardStableRefV1>,
    selected: Vec<EffectObjectBinding>,
    attachments: Vec<(EffectObjectBinding, EffectObjectBinding)>,
    // A selected Aura without a legal host cannot enter (303.4g).
    failed_auras: Vec<EffectObjectBinding>,
    phase: PhaseV1,
    ordinal: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum PhaseV1 {
    Subset,
    Aura {
        index: u16,
    },
    // The BF batch has completed. These are the exact revealed incarnations
    // still in the library, not an assumed destination for all other cards.
    Graveyard {
        remaining_library: Vec<EffectObjectBinding>,
        cards: Vec<EffectObjectBinding>,
        entry_start: u64,
        entry_end: u64,
    },
    // A separate occurrence after the actual remainder batch. It is not a
    // terminal marker: the engine must still dispose of the resolving row.
    RemainderCommitted {
        ordered: Vec<EffectObjectBinding>,
        start: u64,
        end: u64,
    },
}

fn row_context(state: &GameState, p: &EffectContinuation) -> Result<(), String> {
    let item = &p.resolving_item;
    if !printed_row_shape(item) {
        return Err("Genesis Wave lost its exact printed spell root".into());
    }
    let mut expected = crystal_prevention_v1::context(item);
    expected.niv_damage_origin_v1 = event::niv_damage_origin_for_saved_item_v1(state, item);
    if p.ctx != expected {
        return Err("Genesis Wave captured context changed".into());
    }
    // The actual top row was fully validated before removal by the consumed
    // native owner. Published pending boundaries run the common complete row
    // validator before this module. Do not demand that a privately popped row
    // still occur in the public stack during its own instruction.
    Ok(())
}
fn printed_row_shape(item: &StackItem) -> bool {
    !(item.kind != crate::state::StackItemKind::Spell
        || item.inline_effect.is_some()
        || item.v4.source_contract.is_none_or(|source| {
            !definition(source.card_def)
                || source.source != item.source
                || source.controller != item.controller
                || source.zone != Zone::Stack
                || Some(source.cast_method) != item.v4.cast_method
                || item.is_copy != source.spell_copy_origin.is_some()
                || item.is_flashback
                    != (source.cast_method == crate::state::CastMethodV4::Flashback)
                || source
                    .finalized_cast_binding
                    .is_some_and(|binding| binding.x_value != item.v4.x_value)
        })
        || item.v4.stack_item_id == StackItemId::default()
        || item.v4.target_spec != Some(card_def::TargetSpec::None)
        || item.kicked
        || item.madness_offer
        || item.mode_chosen != 0
        || item.v4.face_index != 0
        || item.v4.activated_ability_index.is_some()
        || item.v4.ability_source_contract.is_some()
        || item.v4.hidden_ability_source.is_some()
        || item.v4.granted_by.is_some()
        || item.v4.madness_source_contract.is_some()
        || item.v4.cauldron_grant.0.is_some()
        || item.v4.optional_additional_cost_paid.is_some()
        || !item.v4.paid_cost_refs.is_empty()
        || !item.discarded.is_empty()
        || !item.targets.is_empty()
        || !item.v4.target_contracts.is_empty())
}
fn eligible(state: &GameState, b: EffectObjectBinding, x: u16) -> Result<bool, String> {
    validate_effect_object_binding(state, b)?;
    let live = state.objects.get(b.object);
    let d = &card_def::CARD_DEFS[usize::from(live.card_def)];
    Ok(b.expected_zone == Zone::Library
        && [
            CardType::Artifact,
            CardType::Creature,
            CardType::Enchantment,
            CardType::Land,
            CardType::Planeswalker,
        ]
        .iter()
        .any(|t| d.has_type(*t))
        && d.mana_value <= x)
}
fn aura(state: &GameState, b: EffectObjectBinding) -> bool {
    card_def::CARD_DEFS[usize::from(state.objects.get(b.object).card_def)]
        .attachment
        .is_some()
}
fn canonical_subset(
    original: &[EffectObjectBinding],
    selected: &[EffectObjectBinding],
) -> Result<Vec<EffectObjectBinding>, String> {
    canonicalize_binding_subset(original, selected)
}
fn candidates(
    state: &GameState,
    p: &EffectContinuation,
    c: &ChoiceV1,
) -> Result<Vec<EffectObjectBinding>, String> {
    match &c.phase {
        PhaseV1::RemainderCommitted { .. } => {
            Err("Genesis Wave committed batch is not a choice".into())
        }
        PhaseV1::Subset => c
            .prefix
            .iter()
            .copied()
            .filter_map(|b| match eligible(state, b, p.ctx.x_value) {
                Ok(true) => Some(Ok(b)),
                Ok(false) => None,
                Err(e) => Some(Err(e)),
            })
            .collect(),
        PhaseV1::Aura { index } => returning_aura_hosts_in_zone(
            state,
            *c.selected
                .get(usize::from(*index))
                .ok_or("Genesis Wave Aura cursor moved")?,
            Zone::Library,
        ),
        PhaseV1::Graveyard { cards, .. } => {
            Ok(crate::progenitus_v1::graveyard_members(state, cards)?
                .unwrap_or_else(|| cards.clone()))
        }
    }
}
fn candidate(b: EffectObjectBinding) -> EffectTargetCandidate {
    EffectTargetCandidate {
        target: Target::Object(b.object),
        expected_object: Some(b),
    }
}
// The journal's original future library is a factual input, not a current
// instruction operand. Only revealed slots can affect this spell's remaining
// choices. Keep their exact positions/incarnations and the complete cardinality;
// unknown future slots remain free current hidden input for determinization.
fn same_revealed_layout(
    expected: &[EffectObjectBinding],
    actual: &[EffectObjectBinding],
    revealed: &[EffectObjectBinding],
) -> bool {
    expected.len() == actual.len()
        && actual
            .iter()
            .map(|b| b.object)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            == actual.len()
        && expected.iter().zip(actual).all(|(before, now)| {
            before.expected_zone == Zone::Library
                && now.expected_zone == Zone::Library
                && (!(revealed.contains(before) || revealed.contains(now)) || before == now)
        })
}
fn current_library_matches(
    state: &GameState,
    player: PlayerId,
    expected: &[EffectObjectBinding],
    revealed: &[EffectObjectBinding],
) -> bool {
    let actual = bind_library_exact(state, player);
    same_revealed_layout(expected, &actual, revealed)
        && state
            .known_library_cards(player, player)
            .iter()
            .all(|known| {
                let i = known.position as usize;
                expected
                    .get(i)
                    .zip(actual.get(i))
                    .is_some_and(|(before, now)| before == now)
            })
        && actual.iter().all(|b| {
            state.objects.try_get(b.object).is_some_and(|o| {
                o.owner == player
                    && o.zone == Zone::Library
                    && o.zone_change_count == b.expected_zone_change_count
            })
        })
}
/// Only active public operands select exclusions. Immutable historical future
/// libraries never filter the current whole-object permutation.
pub(crate) fn sampler_bindings_v69(choice: &ChoiceV1) -> Vec<EffectObjectBinding> {
    choice
        .prefix
        .iter()
        .copied()
        .chain(choice.attachments.iter().map(|(_, host)| *host))
        .collect()
}
fn validate_data(state: &GameState, p: &EffectContinuation, c: &ChoiceV1) -> Result<(), String> {
    validate_public_snapshots(c, p.ctx.x_value)?;
    row_context(state, p)?;
    let player = p.ctx.controller;
    let prefix_len = usize::from(p.ctx.x_value).min(c.original_library.len());
    if c.original_library.iter().any(|b| {
        b.expected_zone != Zone::Library
            || state
                .objects
                .try_get(b.object)
                .is_none_or(|o| o.owner != player)
    }) || c
        .original_library
        .iter()
        .map(|b| b.object)
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        != c.original_library.len()
        || c.prefix != c.original_library[..prefix_len]
        || canonical_subset(&c.prefix, &c.selected)? != c.selected
        || canonical_subset(&c.selected, &c.failed_auras)? != c.failed_auras
    {
        return Err("Genesis Wave original prefix/subset changed".into());
    }
    match &c.phase {
        PhaseV1::RemainderCommitted { .. } => {
            return Err("Genesis Wave committed batch has no pending choice".into())
        }
        PhaseV1::Graveyard {
            remaining_library,
            cards,
            entry_start,
            entry_end,
        } => {
            validate_entry_record(
                state,
                p.ctx.controller,
                p.resolving_item.v4.stack_item_id,
                c,
                *entry_start,
                *entry_end,
            )?;
            if !current_library_matches(state, player, remaining_library, &c.prefix)
                || canonical_subset(&c.prefix, cards)? != *cards
                || *cards
                    != c.prefix
                        .iter()
                        .copied()
                        .filter(|b| remaining_library.contains(b))
                        .collect::<Vec<_>>()
            {
                return Err("Genesis Wave actual remaining revealed cards changed".into());
            }
        }
        _ => {
            if !current_library_matches(state, player, &c.original_library, &c.prefix) {
                return Err(
                    "Genesis Wave original library changed before simultaneous entry".into(),
                );
            }
            if c.prefix
                .iter()
                .zip(&c.revealed)
                .any(|(b, r)| revealed_snapshot(state, *b, p.ctx.x_value).as_ref() != Ok(r))
                || c.attachments
                    .iter()
                    .zip(&c.attachment_sources)
                    .any(|((_, b), r)| public_host_snapshot(state, *b).as_ref() != Ok(r))
            {
                return Err("Genesis Wave original public reveal/host snapshot changed".into());
            }
            for b in &c.selected {
                if !eligible(state, *b, p.ctx.x_value)? {
                    return Err("Genesis Wave selected an ineligible permanent".into());
                }
            }
            for (source, host) in &c.attachments {
                if !c.selected.contains(source)
                    || c.failed_auras.contains(source)
                    || !returning_aura_hosts_in_zone(state, *source, Zone::Library)?.contains(host)
                {
                    return Err("Genesis Wave Aura host changed".into());
                }
            }
            let processed = match c.phase {
                PhaseV1::Subset => {
                    if !c.selected.is_empty()
                        || !c.attachments.is_empty()
                        || !c.failed_auras.is_empty()
                    {
                        return Err("Genesis Wave subset contains a future answer".into());
                    }
                    0
                }
                PhaseV1::Aura { index } => {
                    let at = usize::from(index);
                    if c.selected.get(at).is_none_or(|b| !aura(state, *b)) {
                        return Err(
                            "Genesis Wave Aura cursor is not an original selected Aura".into()
                        );
                    }
                    at
                }
                PhaseV1::Graveyard { .. } | PhaseV1::RemainderCommitted { .. } => unreachable!(),
            };
            let mut expected_failed = Vec::new();
            let mut expected_attached = Vec::new();
            for b in c
                .selected
                .iter()
                .take(processed)
                .copied()
                .filter(|b| aura(state, *b))
            {
                if returning_aura_hosts_in_zone(state, b, Zone::Library)?.is_empty() {
                    expected_failed.push(b);
                } else {
                    expected_attached.push(b);
                }
            }
            if c.failed_auras != expected_failed
                || c.attachments.iter().map(|(b, _)| *b).collect::<Vec<_>>() != expected_attached
            {
                return Err("Genesis Wave prior Aura answers or failures changed".into());
            }
        }
    }
    if c.attachments
        .iter()
        .map(|(a, _)| a.object)
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        != c.attachments.len()
    {
        return Err("Genesis Wave Aura host chosen twice".into());
    }
    Ok(())
}
fn validate_entry_record(
    state: &GameState,
    player: PlayerId,
    stack_item_id: StackItemId,
    c: &ChoiceV1,
    start: u64,
    end: u64,
) -> Result<(), String> {
    let start = usize::try_from(start).map_err(|_| "Genesis Wave entry history width")?;
    let end = usize::try_from(end).map_err(|_| "Genesis Wave entry history width")?;
    let events = state
        .engine
        .event_history
        .get(start..end)
        .ok_or("Genesis Wave original entry segment missing")?;
    let previous = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .take(end)
        .filter_map(|(at, e)| match e {
            event::CommittedEvent::GenesisWaveProgressV1 { item, .. }
                if item.v4.stack_item_id == stack_item_id =>
            {
                Some(at)
            }
            _ => None,
        })
        .next_back()
        .ok_or("Genesis Wave entry has no earlier native progress")?;
    if start <= previous
        || state.engine.event_history.len() != end
            && !matches!(state.engine.event_history.get(end),
        Some(event::CommittedEvent::GenesisWaveProgressV1{item,choice}) if item.v4.stack_item_id==stack_item_id && choice.as_ref()==c)
    {
        return Err("Genesis Wave original entry segment moved outside its native progress".into());
    }

    let entering = c
        .selected
        .iter()
        .copied()
        .filter(|b| !c.failed_auras.contains(b))
        .collect::<Vec<_>>();
    let actual = events
        .iter()
        .filter_map(|e| match e {
            event::CommittedEvent::ZoneChange {
                object,
                from: Zone::Library,
                to: Zone::Battlefield,
                controller_before,
            } => Some((*object, *controller_before)),
            _ => None,
        })
        .collect::<Vec<_>>();
    if actual
        != entering
            .iter()
            .map(|b| (b.object, player))
            .collect::<Vec<_>>()
    {
        return Err("Genesis Wave chosen entry differs from its original native batch".into());
    }
    if let PhaseV1::Graveyard {
        remaining_library, ..
    } = &c.phase
    {
        if !same_revealed_layout(
            &c.original_library
                .iter()
                .copied()
                .filter(|b| !entering.contains(b))
                .collect::<Vec<_>>(),
            remaining_library,
            &c.prefix,
        ) {
            return Err("Genesis Wave entry changed an unrelated library member".into());
        }
    }
    Ok(())
}
// The independent batch occurrence is checked from actual native events, not
// from a terminal flag or from the cards' later physical identities.
fn validate_remainder_record(
    state: &GameState,
    item: &StackItem,
    previous_index: usize,
    previous: &ChoiceV1,
    committed: &ChoiceV1,
    at: usize,
) -> Result<(), String> {
    let PhaseV1::Graveyard {
        cards,
        entry_start,
        entry_end,
        ..
    } = &previous.phase
    else {
        return Err("Genesis Wave remainder has no issued graveyard instruction".into());
    };
    let PhaseV1::RemainderCommitted {
        ordered,
        start,
        end,
    } = &committed.phase
    else {
        return Err("Genesis Wave missing committed remainder".into());
    };
    validate_entry_record(
        state,
        item.controller,
        item.v4.stack_item_id,
        previous,
        *entry_start,
        *entry_end,
    )?;
    if committed.ordinal != previous.ordinal + 1
        || committed.original_library != previous.original_library
        || committed.prefix != previous.prefix
        || committed.revealed != previous.revealed
        || committed.attachment_sources != previous.attachment_sources
        || committed.selected != previous.selected
        || committed.attachments != previous.attachments
        || committed.failed_auras != previous.failed_auras
        || canonical_subset(cards, ordered)? != *cards
    {
        return Err("Genesis Wave remainder changed its original issued inputs".into());
    }
    let start = usize::try_from(*start).map_err(|_| "Genesis Wave remainder width")?;
    let end = usize::try_from(*end).map_err(|_| "Genesis Wave remainder width")?;
    if previous_index >= start || end != at {
        return Err("Genesis Wave remainder occurrence moved outside its progress".into());
    }
    let segment = state
        .engine
        .event_history
        .get(start..end)
        .ok_or("Genesis Wave remainder segment absent")?;
    let mut members = Vec::new();
    let mut replacements = Vec::new();
    let mut shuffles = Vec::new();
    let mut last_member = None;
    let mut first_shuffle = None;
    for (offset, event) in segment.iter().enumerate() {
        match event {
            event::CommittedEvent::ZoneChange {
                object,
                from: Zone::Library,
                to,
                controller_before,
            } => {
                if *to != Zone::Graveyard || *controller_before != item.controller {
                    return Err("Genesis Wave remainder has an unrelated library move".into());
                }
                members.push(*object);
                last_member = Some(start + offset);
            }
            event::CommittedEvent::ProgenitusInputV1 { input } => {
                let source = input.source;
                let expected = event::ProposedEvent::zone_change_preserving_known_identity(
                    source.source,
                    Zone::Graveyard,
                );
                let event::ProposedEvent::ZoneChange(expected) = expected else {
                    unreachable!()
                };
                if source.zone != Zone::Library
                    || source.controller != item.controller
                    || source.owner != item.controller
                    || !crate::progenitus_v1::definition(source.card_def)
                    || input.original != expected
                    || !ordered.iter().any(|b| {
                        b.object == source.source
                            && b.expected_zone == source.zone
                            && b.expected_zone_change_count == source.zone_change_count
                    })
                {
                    return Err(
                        "Genesis Wave remainder replacement lost its original library source"
                            .into(),
                    );
                }
                members.push(source.source);
                last_member = Some(start + offset);
                replacements.push(((start + offset) as u64, source));
            }
            event::CommittedEvent::ProgenitusShuffledV1 {
                input_history_index,
                move_history_index,
                source,
            } => {
                if move_history_index.is_some() {
                    return Err("Genesis Wave library replacement invented a zone move".into());
                }
                first_shuffle.get_or_insert(start + offset);
                shuffles.push((*input_history_index, *source));
            }
            event::CommittedEvent::GenesisWaveProgressV1 { .. }
            | event::CommittedEvent::GenesisWaveFinishedV1 { .. } => {
                return Err("Genesis Wave remainder contains another owning occurrence".into());
            }
            _ => {}
        }
    }
    if members != ordered.iter().map(|b| b.object).collect::<Vec<_>>()
        || replacements != shuffles
        || first_shuffle.is_some_and(|shuffle| last_member.is_none_or(|member| shuffle <= member))
    {
        return Err(
            "Genesis Wave remainder lacks its exact native moves or replacement shuffles".into(),
        );
    }
    if !replacements.is_empty() {
        crate::progenitus_v1::validate(state)?;
    }
    Ok(())
}

/// Linear local handoff from the engine's completed row to its existing spell
/// departure. It is neither serialized nor constructible from a public flag.
pub(crate) struct TerminalOwnerV1 {
    item: StackItem,
    choice: ChoiceV1,
    remaining_stack: Vec<StackItem>,
    remainder: usize,
    departure_start: usize,
}
pub(crate) fn prepare_terminal(
    state: &GameState,
    item: &StackItem,
) -> Result<Option<TerminalOwnerV1>, String> {
    if !item
        .v4
        .source_contract
        .is_some_and(|s| definition(s.card_def))
    {
        return Ok(None);
    }
    let Some((at, original, committed)) = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .rev()
        .find_map(|(at, e)| match e {
            event::CommittedEvent::GenesisWaveProgressV1 {
                item: original,
                choice,
            } if original.v4.stack_item_id == item.v4.stack_item_id => {
                Some((at, original.as_ref(), choice.as_ref()))
            }
            _ => None,
        })
    else {
        return Ok(None);
    }; // Countering before resolution has no progress.
    let Some((previous_index, previous_item, previous)) = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .take(at)
        .rev()
        .find_map(|(at, e)| match e {
            event::CommittedEvent::GenesisWaveProgressV1 {
                item: original,
                choice,
            } if original.v4.stack_item_id == item.v4.stack_item_id => {
                Some((at, original.as_ref(), choice.as_ref()))
            }
            _ => None,
        })
    else {
        return Err("Genesis Wave terminal lost its original remainder instruction".into());
    };
    if original != item || previous_item != item || !printed_row_shape(item)
        || state.stack.iter().any(|r|r.v4.stack_item_id==item.v4.stack_item_id)
        || state.engine.pending_effect.as_ref().is_some_and(|p|p.resolving_item.v4.stack_item_id==item.v4.stack_item_id)
        || state.engine.event_history.iter().any(|e|matches!(e,event::CommittedEvent::GenesisWaveFinishedV1{stack_item_id,..}if *stack_item_id==item.v4.stack_item_id))
    { return Err("Genesis Wave completed row still has public ownership or changed identity".into()); }
    validate_remainder_record(state, item, previous_index, previous, committed, at)?;
    Ok(Some(TerminalOwnerV1 {
        item: item.clone(),
        choice: committed.clone(),
        remaining_stack: state.stack.clone(),
        remainder: at,
        departure_start: state.engine.event_history.len(),
    }))
}
fn validate_terminal_record(
    state: &GameState,
    item: &StackItem,
    remainder: usize,
    phase: &ChoiceV1,
    departure: (u64, u64, Option<Zone>),
    at: usize,
) -> Result<(), String> {
    let (departure_start, departure_end, departure) = departure;
    if !matches!(phase.phase, PhaseV1::RemainderCommitted { .. })
        || remainder >= at
        || usize::try_from(departure_end).ok() != Some(at)
        || usize::try_from(departure_start)
            .ok()
            .is_none_or(|start| start <= remainder)
    {
        return Err("Genesis Wave terminal has no completed native remainder occurrence".into());
    }
    let start = usize::try_from(departure_start).map_err(|_| "Genesis Wave departure width")?;
    let segment = state
        .engine
        .event_history
        .get(start..at)
        .ok_or("Genesis Wave native departure segment absent")?;
    if item.is_copy {
        if departure.is_some()
            || !segment.is_empty()
            || item
                .v4
                .source_contract
                .is_none_or(|s| s.spell_copy_origin.is_none())
        {
            return Err("Genesis Wave virtual row invented a physical departure".into());
        }
    } else {
        let expected = if item.is_flashback {
            Zone::Exile
        } else {
            Zone::Graveyard
        };
        let moves = segment
            .iter()
            .filter_map(|e| match e {
                event::CommittedEvent::ZoneChange {
                    object,
                    from,
                    to,
                    controller_before,
                } if *object == item.source => Some((*from, *to, *controller_before)),
                _ => None,
            })
            .collect::<Vec<_>>();
        if departure != Some(expected) || moves != [(Zone::Stack, expected, item.controller)] {
            return Err("Genesis Wave terminal lost its actual physical spell departure".into());
        }
    }
    Ok(())
}
pub(crate) fn finish_terminal(
    state: &mut GameState,
    owner: Option<TerminalOwnerV1>,
    departure: Option<Zone>,
) -> Result<(), String> {
    let Some(owner) = owner else { return Ok(()) };
    if state.engine.halted.is_some()
        || state.stack != owner.remaining_stack
        || state
            .engine
            .pending_effect
            .as_ref()
            .is_some_and(|p| p.resolving_item.v4.stack_item_id == owner.item.v4.stack_item_id)
    {
        return Err("Genesis Wave terminal handoff changed after native departure".into());
    }
    let Some(event::CommittedEvent::GenesisWaveProgressV1 { item, choice }) =
        state.engine.event_history.get(owner.remainder)
    else {
        return Err("Genesis Wave terminal remainder was erased".into());
    };
    if item.as_ref() != &owner.item || choice.as_ref() != &owner.choice {
        return Err("Genesis Wave terminal remainder belongs to another row".into());
    }
    let end = state.engine.event_history.len();
    validate_terminal_record(
        state,
        item,
        owner.remainder,
        choice,
        (owner.departure_start as u64, end as u64, departure),
        end,
    )?;
    state
        .engine
        .event_history
        .push(event::CommittedEvent::GenesisWaveFinishedV1 {
            stack_item_id: owner.item.v4.stack_item_id,
            remainder_history_index: owner.remainder as u64,
            departure_start: owner.departure_start as u64,
            departure_end: end as u64,
            departure,
        });
    Ok(())
}

fn validate_issued(state: &GameState, p: &EffectContinuation, c: &ChoiceV1) -> Result<(), String> {
    validate_data(state, p, c)?;
    let rows = state
        .engine
        .event_history
        .iter()
        .filter_map(|e| match e {
            event::CommittedEvent::GenesisWaveProgressV1 { item, choice }
                if item.v4.stack_item_id == p.resolving_item.v4.stack_item_id =>
            {
                Some((item, choice))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if rows.last().is_none_or(|(item, choice)| item.as_ref() != &p.resolving_item || choice.as_ref() != c)
        || rows.iter().enumerate().any(|(i,(item,old))| item.as_ref() != &p.resolving_item
            || old.ordinal != i as u64 || old.original_library != c.original_library || old.prefix != c.prefix)
        || c.ordinal != rows.len() as u64 - 1
        || state.engine.event_history.iter().any(|e| matches!(e,
            event::CommittedEvent::GenesisWaveFinishedV1 { stack_item_id, .. } if *stack_item_id == p.resolving_item.v4.stack_item_id))
    { return Err("Genesis Wave lost its issued native progress".into()); }
    Ok(())
}
fn issue(state: &mut GameState, p: &mut EffectContinuation, c: ChoiceV1) -> Result<bool, String> {
    validate_data(state, p, &c)?;
    let count = state.engine.event_history.iter().filter(|e| matches!(e,
        event::CommittedEvent::GenesisWaveProgressV1 { item, .. } if item.v4.stack_item_id == p.resolving_item.v4.stack_item_id)).count();
    if c.ordinal != count as u64 {
        return Err("Genesis Wave progress repeated/skipped".into());
    }
    state
        .engine
        .event_history
        .push(event::CommittedEvent::GenesisWaveProgressV1 {
            item: Box::new(p.resolving_item.clone()),
            choice: Box::new(c.clone()),
        });
    let rows = candidates(state, p, &c)?;
    let count = u16::try_from(rows.len()).map_err(|_| "Genesis Wave choice exceeds paid u16 X")?;
    if matches!(c.phase, PhaseV1::Graveyard { .. })
        && (count < 2 || !crate::engine::graveyard_order_choice_exposed(state, p.ctx.controller))
    {
        return apply(state, p, c, rows);
    }
    p.choice = Some(PendingEffectChoice::SelectTargets {
        player: p.ctx.controller,
        path: Vec::new(),
        selected: Vec::new(),
        legal: rows.into_iter().map(candidate).collect(),
        min_targets: if matches!(c.phase, PhaseV1::Subset) {
            0
        } else if matches!(c.phase, PhaseV1::Aura { .. }) {
            1
        } else {
            count
        },
        max_targets: if matches!(c.phase, PhaseV1::Aura { .. }) {
            1
        } else {
            count
        },
        ordered: matches!(c.phase, PhaseV1::Graveyard { .. }),
        purpose: EffectTargetSelectionPurpose::GenesisWaveCardsV1 { choice: c },
    });
    Ok(true)
}
fn answer_valid(
    state: &GameState,
    p: &EffectContinuation,
    c: &ChoiceV1,
    cards: &[EffectObjectBinding],
) -> Result<(), String> {
    let rows = candidates(state, p, c)?;
    canonical_subset(&rows, cards)?;
    if match c.phase {
        PhaseV1::Subset => false,
        PhaseV1::Aura { .. } => cards.len() != 1,
        PhaseV1::Graveyard { .. } => rows.len() != cards.len(),
        PhaseV1::RemainderCommitted { .. } => {
            return Err("Genesis Wave committed batch cannot be answered".into())
        }
    } {
        return Err("Genesis Wave answer cardinality changed".into());
    }
    Ok(())
}
pub(crate) fn answer(
    p: &mut EffectContinuation,
    choice: ChoiceV1,
    cards: Vec<EffectObjectBinding>,
) {
    let frame = EffectFrame::GenesisWaveV1 { choice, cards };
    p.answered_choice_guard = Some(EffectAnsweredChoiceGuard::StandardSelection {
        frame: Box::new(frame.clone()),
    });
    p.frames.push(frame);
}
pub(crate) fn start(state: &mut GameState, p: &mut EffectContinuation) -> Result<bool, String> {
    row_context(state, p)?;
    let player = p.ctx.controller;
    let original_library = bind_library_exact(state, player);
    let prefix = original_library
        .iter()
        .take(usize::from(p.ctx.x_value))
        .copied()
        .collect::<Vec<_>>();
    for viewer in [player, player.opponent()] {
        state.reveal_library_top(viewer, player, prefix.len());
    }
    let revealed = prefix
        .iter()
        .map(|b| revealed_snapshot(state, *b, p.ctx.x_value))
        .collect::<Result<_, _>>()?;
    issue(
        state,
        p,
        ChoiceV1 {
            original_library,
            revealed,
            attachment_sources: Vec::new(),
            prefix,
            selected: Vec::new(),
            attachments: Vec::new(),
            failed_auras: Vec::new(),
            phase: PhaseV1::Subset,
            ordinal: 0,
        },
    )
}
pub(crate) fn apply(
    state: &mut GameState,
    p: &mut EffectContinuation,
    mut c: ChoiceV1,
    cards: Vec<EffectObjectBinding>,
) -> Result<bool, String> {
    validate_issued(state, p, &c)?;
    answer_valid(state, p, &c, &cards)?;
    match c.phase {
        PhaseV1::RemainderCommitted { .. } => {
            return Err("Genesis Wave remainder cannot commit twice".into())
        }
        PhaseV1::Subset => c.selected = canonical_subset(&c.prefix, &cards)?,
        PhaseV1::Aura { index } => {
            c.attachment_sources
                .push(public_host_snapshot(state, cards[0])?);
            c.attachments
                .push((c.selected[usize::from(index)], cards[0]));
        }
        PhaseV1::Graveyard {
            cards: ref original,
            ..
        } => {
            // The order menu contains only actual would-GY entrants. Keep
            // replaced members in their original relative order and delegate
            // every actual replacement and mandatory shuffle to the batch.
            let mut all = cards.clone();
            all.extend(original.iter().copied().filter(|b| !cards.contains(b)));
            let previous_index = state.engine.event_history.iter().rposition(|e| matches!(e,
                event::CommittedEvent::GenesisWaveProgressV1 {item,..} if item.v4.stack_item_id==p.resolving_item.v4.stack_item_id))
                .ok_or("Genesis Wave remainder lost original progress")?;
            let start = state.engine.event_history.len() as u64;
            commit_zone_change_batch(state, &all, Zone::Graveyard, true)?;
            if state.engine.halted.is_some() {
                return Err("Genesis Wave graveyard batch halted".into());
            }
            let mut committed = c.clone();
            committed.ordinal += 1;
            committed.phase = PhaseV1::RemainderCommitted {
                ordered: all,
                start,
                end: state.engine.event_history.len() as u64,
            };
            validate_remainder_record(
                state,
                &p.resolving_item,
                previous_index,
                &c,
                &committed,
                state.engine.event_history.len(),
            )?;
            state
                .engine
                .event_history
                .push(event::CommittedEvent::GenesisWaveProgressV1 {
                    item: Box::new(p.resolving_item.clone()),
                    choice: Box::new(committed),
                });
            return Ok(false);
        }
    }
    let first = if let PhaseV1::Aura { index } = c.phase {
        usize::from(index) + 1
    } else {
        0
    };
    for index in first..c.selected.len() {
        let b = c.selected[index];
        if !aura(state, b) {
            continue;
        }
        if returning_aura_hosts_in_zone(state, b, Zone::Library)?.is_empty() {
            c.failed_auras.push(b);
            continue;
        }
        c.phase = PhaseV1::Aura {
            index: u16::try_from(index).map_err(|_| "Genesis Wave Aura index")?,
        };
        c.ordinal += 1;
        return issue(state, p, c);
    }
    let entering = c
        .selected
        .iter()
        .copied()
        .filter(|b| !c.failed_auras.contains(b))
        .collect::<Vec<_>>();
    let events = entering
        .iter()
        .map(|b| {
            event::ProposedEvent::zone_change_preserving_known_identity(b.object, Zone::Battlefield)
        })
        .collect();
    let entry_start = u64::try_from(state.engine.event_history.len())
        .map_err(|_| "Genesis Wave entry history width")?;
    event::batch_with_linked_aura_attachments_v1(state, events, c.attachments.clone());
    if state.engine.halted.is_some() {
        return Err("Genesis Wave simultaneous entry halted".into());
    }
    // An as-enters prompt retains the exact enclosing operation before any
    // entry. Its replay repeats this complete batch after the real answer.
    if crate::linked_aura_operation_v1::entry_checkpoint_in_flight_v1(state) {
        return Ok(false);
    }
    let remaining_library = bind_library_exact(state, p.ctx.controller);
    let entry_end = u64::try_from(state.engine.event_history.len())
        .map_err(|_| "Genesis Wave entry history width")?;
    let cards = c
        .prefix
        .iter()
        .copied()
        .filter(|b| remaining_library.contains(b))
        .collect();
    c.phase = PhaseV1::Graveyard {
        remaining_library,
        cards,
        entry_start,
        entry_end,
    };
    c.ordinal += 1;
    issue(state, p, c)
}
pub(crate) fn validate(state: &GameState, p: &EffectContinuation) -> Result<(), String> {
    row_context(state, p)?;
    match (&p.choice, &p.answered_choice_guard, p.frames.as_slice()) {
        (None, None, [EffectFrame::Program {op: EffectOp::GenesisWaveV1, path}])
            if path.is_empty() && !state.engine.event_history.iter().any(|e| matches!(e,
                event::CommittedEvent::GenesisWaveProgressV1 {item,..} if item.v4.stack_item_id == p.resolving_item.v4.stack_item_id)) => Ok(()),
        (Some(PendingEffectChoice::SelectTargets {player,path,selected,legal,min_targets,max_targets,ordered,
            purpose:EffectTargetSelectionPurpose::GenesisWaveCardsV1 {choice:c}}),None,[]) => {
            validate_issued(state,p,c)?;
            let rows=candidates(state,p,c)?;
            let chosen=selected.iter().map(|r| r.expected_object.ok_or("Genesis Wave binding absent")).collect::<Result<Vec<_>,_>>()?;
            canonical_subset(&rows,&chosen)?;
            let max=if matches!(c.phase,PhaseV1::Aura{..}) {1} else {u16::try_from(rows.len()).map_err(|_|"Genesis Wave choice width")?};
            let min=if matches!(c.phase,PhaseV1::Subset) {0} else {max};
            if *player!=p.ctx.controller || !path.is_empty() || *min_targets!=min || *max_targets!=max
                || *ordered!=matches!(c.phase,PhaseV1::Graveyard{..}) || chosen.len()>usize::from(max)
                || *legal!=rows.iter().copied().filter(|b|!chosen.contains(b)).map(candidate).collect::<Vec<_>>()
                || selected.iter().any(|r|r.expected_object.is_none_or(|b|r.target!=Target::Object(b.object)))
            {return Err("Genesis Wave issued menu changed".into());} Ok(())
        }
        (None,Some(EffectAnsweredChoiceGuard::StandardSelection{frame}),[actual@EffectFrame::GenesisWaveV1{choice,cards}]) if frame.as_ref()==actual => {
            validate_issued(state,p,choice)?; answer_valid(state,p,choice,cards)
        }
        _=>Err("Genesis Wave choice or native frame erased/changed".into()),
    }
}
pub(crate) fn refuse_legacy(state: &GameState, viewer: Option<PlayerId>) -> Result<(), String> {
    use crate::optional_trigger_public_v1 as public;
    let visible = state.objects.iter().any(|(id, o)| {
        public::reference(state, id, viewer).is_some_and(
            |r| matches!(r.identity,public::IdentityV1::Known{card_id} if definition(card_id)),
        ) || viewer.is_some_and(|v| {
            o.v4.face_down_v1
                .is_some_and(|f| f.lookers & (1 << v.index()) != 0)
                && definition(o.card_def)
        })
    });
    if visible
        || state.engine.event_history.has_any(&[
            crate::event_history_v1::EventKindV1::GenesisWaveProgressV1,
            crate::event_history_v1::EventKindV1::GenesisWaveFinishedV1,
        ])
    {
        return Err("Genesis Wave requires a distinct complete revealed-entry consumer".into());
    }
    Ok(())
}

/// Validate sparse journals without selecting concealed physical card names.
/// Full native pending and linked-owner checks still run independently.
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    if state.engine.event_log.iter().any(|e| {
        matches!(
            e,
            event::CommittedEvent::GenesisWaveProgressV1 { .. }
                | event::CommittedEvent::GenesisWaveFinishedV1 { .. }
        )
    }) {
        return Err("Genesis Wave owning history cannot enter the event log".into());
    }
    use crate::event_history_v1::EventKindV1;
    if !state.engine.event_history.has_any(&[
        EventKindV1::GenesisWaveProgressV1,
        EventKindV1::GenesisWaveFinishedV1,
    ]) {
        return Ok(());
    }
    let mut progress =
        std::collections::BTreeMap::<StackItemId, (u64, &StackItem, &ChoiceV1, bool, usize)>::new();
    for (at, event) in state.engine.event_history.iter().enumerate() {
        match event {
            event::CommittedEvent::GenesisWaveProgressV1 { item, choice } => {
                let id = item.v4.stack_item_id;
                validate_public_snapshots(choice, item.v4.x_value)?;
                if !printed_row_shape(item) {
                    return Err("Genesis Wave journal lost its original printed row".into());
                }
                if let PhaseV1::Graveyard {
                    entry_start,
                    entry_end,
                    ..
                } = choice.phase
                {
                    validate_entry_record(
                        state,
                        item.controller,
                        item.v4.stack_item_id,
                        choice,
                        entry_start,
                        entry_end,
                    )?;
                }
                if let Some((ordinal, original, old, finished, previous_index)) = progress.get(&id)
                {
                    if matches!(choice.phase, PhaseV1::RemainderCommitted { .. }) {
                        validate_remainder_record(state, item, *previous_index, old, choice, at)?;
                    }
                    if *finished
                        || item.as_ref() != *original
                        || choice.ordinal != ordinal + 1
                        || choice.original_library != old.original_library
                        || choice.prefix != old.prefix
                        || choice.revealed != old.revealed
                        || !choice
                            .attachment_sources
                            .starts_with(&old.attachment_sources)
                        || matches!(choice.phase, PhaseV1::Subset)
                        || matches!(old.phase, PhaseV1::RemainderCommitted { .. })
                        || matches!(old.phase, PhaseV1::Graveyard { .. })
                            && !matches!(choice.phase, PhaseV1::RemainderCommitted { .. })
                        || matches!(old.phase, PhaseV1::Aura { .. })
                            && choice.selected != old.selected
                        || !choice.attachments.starts_with(&old.attachments)
                        || !choice.failed_auras.starts_with(&old.failed_auras)
                        || matches!(old.phase,PhaseV1::Aura{index} if choice.attachments.len()!=old.attachments.len()+1
                            || choice.attachments.last().is_none_or(|(b,_)|old.selected.get(usize::from(index))!=Some(b)))
                    {
                        return Err("Genesis Wave journal transition changed".into());
                    }
                } else if choice.ordinal != 0
                    || !matches!(choice.phase, PhaseV1::Subset)
                    || !choice.selected.is_empty()
                    || !choice.attachments.is_empty()
                    || !choice.failed_auras.is_empty()
                {
                    return Err("Genesis Wave initial progress changed".into());
                }
                progress.insert(id, (choice.ordinal, item, choice, false, at));
            }
            event::CommittedEvent::GenesisWaveFinishedV1 {
                stack_item_id,
                remainder_history_index,
                departure_start,
                departure_end,
                departure,
            } => {
                let Some((_, item, choice, finished, remainder)) = progress.get_mut(stack_item_id)
                else {
                    return Err("Genesis Wave completion has no original progress".into());
                };
                if *finished || usize::try_from(*remainder_history_index).ok() != Some(*remainder) {
                    return Err(
                        "Genesis Wave completion repeated or references a foreign remainder".into(),
                    );
                }
                validate_terminal_record(
                    state,
                    item,
                    *remainder,
                    choice,
                    (*departure_start, *departure_end, *departure),
                    at,
                )?;
                *finished = true;
            }
            _ => {}
        }
    }
    for (id, (_, item, choice, finished, _)) in progress {
        if finished {
            if state.stack.iter().any(|row| row.v4.stack_item_id == id)
                || state
                    .engine
                    .pending_effect
                    .as_ref()
                    .is_some_and(|p| p.resolving_item.v4.stack_item_id == id)
            {
                return Err("Genesis Wave terminal still has a public row or pending owner".into());
            }
        } else {
            if matches!(choice.phase, PhaseV1::RemainderCommitted { .. }) {
                return Err("Genesis Wave committed remainder lost native row completion".into());
            }
            if let Some(p) = state
                .engine
                .pending_effect
                .as_ref()
                .filter(|p| p.resolving_item.v4.stack_item_id == id)
            {
                if &p.resolving_item != item {
                    return Err("Genesis Wave pending original row changed".into());
                }
                validate(state, p)?;
            } else if !crate::linked_aura_operation_v1::owns_entry_row_checkpoint_v1(state, item) {
                return Err("Genesis Wave issued continuation erased".into());
            }
        }
    }
    Ok(())
}

/// Pure immutable source bytes only. No observation, proof or world is cached.
pub(crate) fn source_sha256_v1() -> &'static str {
    use sha2::{Digest, Sha256};
    static VALUE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    VALUE
        .get_or_init(|| {
            let mut hash = Sha256::new();
            hash.update(b"genesis-wave-native-semantic-source/v1");
            for (name, source) in [
                (
                    "mtg-kernel/src/effect/genesis_wave_v1.rs",
                    include_str!("genesis_wave_v1.rs"),
                ),
                (
                    "mtg-kernel/src/rules_meaning_graveyard_return_v61.rs",
                    include_str!("../rules_meaning_graveyard_return_v61.rs"),
                ),
                ("mtg-kernel/src/effect.rs", include_str!("../effect.rs")),
                ("mtg-kernel/src/engine.rs", include_str!("../engine.rs")),
                ("mtg-kernel/src/event.rs", include_str!("../event.rs")),
                (
                    "mtg-kernel/src/event_history_v1.rs",
                    include_str!("../event_history_v1.rs"),
                ),
                (
                    "mtg-kernel/src/linked_aura_operation_v1.rs",
                    include_str!("../linked_aura_operation_v1.rs"),
                ),
                (
                    "mtg-kernel/src/progenitus_v1.rs",
                    include_str!("../progenitus_v1.rs"),
                ),
                (
                    "mtg-kernel/src/optional_trigger_public_v1.rs",
                    include_str!("../optional_trigger_public_v1.rs"),
                ),
                (
                    "mtg-kernel/src/genesis_wave_public_v69.rs",
                    include_str!("../genesis_wave_public_v69.rs"),
                ),
                (
                    "mtg-kernel/src/rules_meaning_genesis_wave_v69.rs",
                    include_str!("../rules_meaning_genesis_wave_v69.rs"),
                ),
                (
                    "mtg-kernel/src/genesis_wave_observation_v69.rs",
                    include_str!("../genesis_wave_observation_v69.rs"),
                ),
                (
                    "mtg-kernel/src/genesis_wave_candidates_v69.rs",
                    include_str!("../genesis_wave_candidates_v69.rs"),
                ),
                (
                    "mtg-kernel/src/rl_session/genesis_wave_v69.rs",
                    include_str!("../rl_session/genesis_wave_v69.rs"),
                ),
                (
                    "mtg-kernel/src/current_mixed_policy_v1/genesis_wave.rs",
                    include_str!("../current_mixed_policy_v1/genesis_wave.rs"),
                ),
                (
                    "mtg-kernel/src/human_bo3_v1/genesis_wave_v69.rs",
                    include_str!("../human_bo3_v1/genesis_wave_v69.rs"),
                ),
                (
                    "mtg-kernel/src/human_bo3_v1/limited_genesis_wave_v10.rs",
                    include_str!("../human_bo3_v1/limited_genesis_wave_v10.rs"),
                ),
                (
                    "mtg-kernel/src/limited_genesis_wave_session_v10.rs",
                    include_str!("../limited_genesis_wave_session_v10.rs"),
                ),
                (
                    "mtg-kernel/src/bin/kernel_limited_genesis_wave_env_v10.rs",
                    include_str!("../bin/kernel_limited_genesis_wave_env_v10.rs"),
                ),
                ("mtg-kernel/src/rl.rs", include_str!("../rl.rs")),
                (
                    "mtg-kernel/src/rl_session.rs",
                    include_str!("../rl_session.rs"),
                ),
                ("mtg-kernel/src/lib.rs", include_str!("../lib.rs")),
                (
                    "mtg-kernel/src/ravenous_observation_v51.rs",
                    include_str!("../ravenous_observation_v51.rs"),
                ),
                (
                    "mtg-kernel/src/ravenous_public_v51.rs",
                    include_str!("../ravenous_public_v51.rs"),
                ),
                (
                    "mtg-kernel/src/akroma_family_public_v40.rs",
                    include_str!("../akroma_family_public_v40.rs"),
                ),
                (
                    "mtg-kernel/src/divine_actions_v44.rs",
                    include_str!("../divine_actions_v44.rs"),
                ),
                (
                    "mtg-kernel/src/rl_variable_cast_v1.rs",
                    include_str!("../rl_variable_cast_v1.rs"),
                ),
                (
                    "mtg-kernel/src/rl_session/ravenous_v51.rs",
                    include_str!("../rl_session/ravenous_v51.rs"),
                ),
                (
                    "mtg-kernel/src/rl_session/limited_common_v1.rs",
                    include_str!("../rl_session/limited_common_v1.rs"),
                ),
                (
                    "mtg-kernel/src/current_mixed_policy_v1.rs",
                    include_str!("../current_mixed_policy_v1.rs"),
                ),
                (
                    "mtg-kernel/src/current_mixed_policy_v1/normalize.rs",
                    include_str!("../current_mixed_policy_v1/normalize.rs"),
                ),
                (
                    "mtg-kernel/src/human_bo3_v1.rs",
                    include_str!("../human_bo3_v1.rs"),
                ),
                (
                    "mtg-kernel/src/human_bo3_v1/ravenous_v51.rs",
                    include_str!("../human_bo3_v1/ravenous_v51.rs"),
                ),
                (
                    "mtg-kernel/src/human_bo3_v1/labels.rs",
                    include_str!("../human_bo3_v1/labels.rs"),
                ),
                (
                    "mtg-kernel/src/human_bo3_v1/prepared_v57.rs",
                    include_str!("../human_bo3_v1/prepared_v57.rs"),
                ),
                (
                    "mtg-kernel/src/limited_session_v1.rs",
                    include_str!("../limited_session_v1.rs"),
                ),
                (
                    "mtg-kernel/src/engine/time_stop_v1.rs",
                    include_str!("../engine/time_stop_v1.rs"),
                ),
                (
                    "mtg-kernel/src/time_stop_public_v68.rs",
                    include_str!("../time_stop_public_v68.rs"),
                ),
                (
                    "python/tools/limited_genesis_wave_session_v10.py",
                    include_str!("../../../python/tools/limited_genesis_wave_session_v10.py"),
                ),
                (
                    "python/tools/limited_end_turn_session_v9.py",
                    include_str!("../../../python/tools/limited_end_turn_session_v9.py"),
                ),
                (
                    "mtg-kernel/src/cast_identity_v1.rs",
                    include_str!("../cast_identity_v1.rs"),
                ),
                (
                    "mtg-kernel/src/rl_session/flat_action_v4.rs",
                    include_str!("../rl_session/flat_action_v4.rs"),
                ),
                (
                    "mtg-kernel/src/rl_session/flat_action_v4/search_state.rs",
                    include_str!("../rl_session/flat_action_v4/search_state.rs"),
                ),
                (
                    "mtg-kernel/src/rl_session/flat_action_v4/search_state/sampler.rs",
                    include_str!("../rl_session/flat_action_v4/search_state/sampler.rs"),
                ),
                (
                    "mtg-kernel/src/rl_session/flat_action_v4/search_state/effect_refs.rs",
                    include_str!("../rl_session/flat_action_v4/search_state/effect_refs.rs"),
                ),
                (
                    "mtg-kernel/src/linked_aura_operation_v1/genesis_current_slots_v69.rs",
                    include_str!("../linked_aura_operation_v1/genesis_current_slots_v69.rs"),
                ),
                (
                    "mtg-kernel/src/engine/muldrotha_v1.rs",
                    include_str!("../engine/muldrotha_v1.rs"),
                ),
                (
                    "mtg-kernel/src/current_mixed_policy_v1/canonical_semantic_booleans.rs",
                    include_str!("../current_mixed_policy_v1/canonical_semantic_booleans.rs"),
                ),
                (
                    "mtg-kernel/src/current_mixed_policy_v1/roots.rs",
                    include_str!("../current_mixed_policy_v1/roots.rs"),
                ),
                (
                    "mtg-kernel/src/current_mixed_policy_v1/ordinary_removal_printing.rs",
                    include_str!("../current_mixed_policy_v1/ordinary_removal_printing.rs"),
                ),
                (
                    "mtg-kernel/src/engine/tinybones_v72.rs",
                    include_str!("../engine/tinybones_v72.rs"),
                ),
                (
                    "mtg-kernel/src/effect/liliana_dreadhorde_v1.rs",
                    include_str!("liliana_dreadhorde_v1.rs"),
                ),
                (
                    "mtg-kernel/src/creature_death_capture_v1.rs",
                    include_str!("../creature_death_capture_v1.rs"),
                ),
                (
                    "mtg-kernel/src/planeswalker_v1.rs",
                    include_str!("../planeswalker_v1.rs"),
                ),
                ("mtg-kernel/src/trigger.rs", include_str!("../trigger.rs")),
                (
                    "mtg-kernel/src/muldrotha_public_v71.rs",
                    include_str!("../muldrotha_public_v71.rs"),
                ),
                (
                    "mtg-kernel/src/rules_meaning_muldrotha_v71.rs",
                    include_str!("../rules_meaning_muldrotha_v71.rs"),
                ),
                (
                    "mtg-kernel/src/muldrotha_observation_v71.rs",
                    include_str!("../muldrotha_observation_v71.rs"),
                ),
                (
                    "mtg-kernel/src/muldrotha_candidates_v71.rs",
                    include_str!("../muldrotha_candidates_v71.rs"),
                ),
                (
                    "mtg-kernel/src/rl_session/muldrotha_v71.rs",
                    include_str!("../rl_session/muldrotha_v71.rs"),
                ),
                (
                    "mtg-kernel/src/current_mixed_policy_v1/muldrotha.rs",
                    include_str!("../current_mixed_policy_v1/muldrotha.rs"),
                ),
                (
                    "mtg-kernel/src/human_bo3_v1/muldrotha_v71.rs",
                    include_str!("../human_bo3_v1/muldrotha_v71.rs"),
                ),
                (
                    "mtg-kernel/src/human_bo3_v1/limited_muldrotha_v12.rs",
                    include_str!("../human_bo3_v1/limited_muldrotha_v12.rs"),
                ),
                (
                    "mtg-kernel/src/limited_muldrotha_session_v12.rs",
                    include_str!("../limited_muldrotha_session_v12.rs"),
                ),
                (
                    "mtg-kernel/src/bin/kernel_limited_muldrotha_env_v12.rs",
                    include_str!("../bin/kernel_limited_muldrotha_env_v12.rs"),
                ),
                (
                    "python/tools/limited_muldrotha_session_v12.py",
                    include_str!("../../../python/tools/limited_muldrotha_session_v12.py"),
                ),
                (
                    "mtg-kernel/src/rules_meaning_tinybones_v72.rs",
                    include_str!("../rules_meaning_tinybones_v72.rs"),
                ),
                (
                    "mtg-kernel/src/tinybones_public_v72.rs",
                    include_str!("../tinybones_public_v72.rs"),
                ),
                (
                    "mtg-kernel/src/rules_meaning_liliana_v73.rs",
                    include_str!("../rules_meaning_liliana_v73.rs"),
                ),
                (
                    "mtg-kernel/src/liliana_public_v73.rs",
                    include_str!("../liliana_public_v73.rs"),
                ),
                (
                    "mtg-kernel/src/effect/liliana_dreadhorde_public_v73.rs",
                    include_str!("liliana_dreadhorde_public_v73.rs"),
                ),
                (
                    "mtg-kernel/src/rules_meaning_chandra_v74.rs",
                    include_str!("../rules_meaning_chandra_v74.rs"),
                ),
                (
                    "mtg-kernel/src/chandra_public_v74.rs",
                    include_str!("../chandra_public_v74.rs"),
                ),
                (
                    "mtg-kernel/src/effect/chandra_flameshaper_public_v74.rs",
                    include_str!("chandra_flameshaper_public_v74.rs"),
                ),
                (
                    "mtg-kernel/src/liliana_observation_v73.rs",
                    include_str!("../liliana_observation_v73.rs"),
                ),
                (
                    "mtg-kernel/src/liliana_candidates_v73.rs",
                    include_str!("../liliana_candidates_v73.rs"),
                ),
                (
                    "mtg-kernel/src/rl_session/liliana_v73.rs",
                    include_str!("../rl_session/liliana_v73.rs"),
                ),
                (
                    "mtg-kernel/src/current_mixed_policy_v1/liliana.rs",
                    include_str!("../current_mixed_policy_v1/liliana.rs"),
                ),
                (
                    "mtg-kernel/src/chandra_observation_v74.rs",
                    include_str!("../chandra_observation_v74.rs"),
                ),
                (
                    "mtg-kernel/src/chandra_candidates_v74.rs",
                    include_str!("../chandra_candidates_v74.rs"),
                ),
                (
                    "mtg-kernel/src/rl_session/chandra_v74.rs",
                    include_str!("../rl_session/chandra_v74.rs"),
                ),
                (
                    "mtg-kernel/src/current_mixed_policy_v1/chandra.rs",
                    include_str!("../current_mixed_policy_v1/chandra.rs"),
                ),
                (
                    "mtg-kernel/src/kaito_observation_v75.rs",
                    include_str!("../kaito_observation_v75.rs"),
                ),
                (
                    "mtg-kernel/src/kaito_candidates_v75.rs",
                    include_str!("../kaito_candidates_v75.rs"),
                ),
                (
                    "mtg-kernel/src/rl_session/kaito_v75.rs",
                    include_str!("../rl_session/kaito_v75.rs"),
                ),
                (
                    "mtg-kernel/src/current_mixed_policy_v1/kaito.rs",
                    include_str!("../current_mixed_policy_v1/kaito.rs"),
                ),
                (
                    "mtg-kernel/src/human_bo3_v1/liliana_v73.rs",
                    include_str!("../human_bo3_v1/liliana_v73.rs"),
                ),
                (
                    "mtg-kernel/src/human_bo3_v1/limited_liliana_v14.rs",
                    include_str!("../human_bo3_v1/limited_liliana_v14.rs"),
                ),
                (
                    "mtg-kernel/src/limited_liliana_session_v14.rs",
                    include_str!("../limited_liliana_session_v14.rs"),
                ),
                (
                    "mtg-kernel/src/bin/kernel_limited_liliana_env_v14.rs",
                    include_str!("../bin/kernel_limited_liliana_env_v14.rs"),
                ),
                (
                    "python/tools/limited_liliana_session_v14.py",
                    include_str!("../../../python/tools/limited_liliana_session_v14.py"),
                ),
                (
                    "mtg-kernel/src/human_bo3_v1/chandra_v74.rs",
                    include_str!("../human_bo3_v1/chandra_v74.rs"),
                ),
                (
                    "mtg-kernel/src/human_bo3_v1/limited_chandra_v15.rs",
                    include_str!("../human_bo3_v1/limited_chandra_v15.rs"),
                ),
                (
                    "mtg-kernel/src/limited_chandra_session_v15.rs",
                    include_str!("../limited_chandra_session_v15.rs"),
                ),
                (
                    "mtg-kernel/src/bin/kernel_limited_chandra_env_v15.rs",
                    include_str!("../bin/kernel_limited_chandra_env_v15.rs"),
                ),
                (
                    "python/tools/limited_chandra_session_v15.py",
                    include_str!("../../../python/tools/limited_chandra_session_v15.py"),
                ),
                (
                    "mtg-kernel/src/effect/chandra_flameshaper_v1.rs",
                    include_str!("chandra_flameshaper_v1.rs"),
                ),
                (
                    "mtg-kernel/src/human_bo3_v1/kaito_v75.rs",
                    include_str!("../human_bo3_v1/kaito_v75.rs"),
                ),
                (
                    "mtg-kernel/src/human_bo3_v1/limited_kaito_v16.rs",
                    include_str!("../human_bo3_v1/limited_kaito_v16.rs"),
                ),
                (
                    "mtg-kernel/src/limited_kaito_session_v16.rs",
                    include_str!("../limited_kaito_session_v16.rs"),
                ),
                (
                    "mtg-kernel/src/bin/kernel_limited_kaito_env_v16.rs",
                    include_str!("../bin/kernel_limited_kaito_env_v16.rs"),
                ),
                (
                    "python/tools/limited_kaito_session_v16.py",
                    include_str!("../../../python/tools/limited_kaito_session_v16.py"),
                ),
                (
                    "mtg-kernel/src/effect/kaito_cunning_v75.rs",
                    include_str!("kaito_cunning_v75.rs"),
                ),
                (
                    "mtg-kernel/src/continuous_source_frame_v1.rs",
                    include_str!("../continuous_source_frame_v1.rs"),
                ),
                (
                    "mtg-kernel/src/retained_world_parse_v1.rs",
                    include_str!("../retained_world_parse_v1.rs"),
                ),
            ] {
                hash.update((name.len() as u64).to_le_bytes());
                hash.update(name.as_bytes());
                hash.update((source.len() as u64).to_le_bytes());
                hash.update(source.as_bytes());
            }
            format!("{:x}", hash.finalize())
        })
        .as_str()
}

/// Snapshot only an actually revealed current library incarnation. The complete
/// private library remains exclusively in ChoiceV1 and is never projected.
fn revealed_snapshot(
    state: &GameState,
    b: EffectObjectBinding,
    x: u16,
) -> Result<crate::genesis_wave_public_v69::RevealedV69, String> {
    validate_effect_object_binding(state, b)?;
    let o = state.objects.get(b.object);
    let card = crate::optional_trigger_public_v1::reference(state, b.object, Some(o.owner))
        .ok_or("Genesis Wave revealed card is not known to its controller")?;
    if card.zone != Zone::Library
        || !matches!(
            card.identity,
            crate::optional_trigger_public_v1::IdentityV1::Known { .. }
        )
    {
        return Err("Genesis Wave reveal lost original library identity".into());
    }
    let d = &card_def::CARD_DEFS[usize::from(o.card_def)];
    let permanent = [
        CardType::Artifact,
        CardType::Creature,
        CardType::Enchantment,
        CardType::Land,
        CardType::Planeswalker,
    ]
    .iter()
    .any(|t| d.has_type(*t));
    Ok(crate::genesis_wave_public_v69::RevealedV69 {
        card: crate::rl::CardStableRefV1 {
            arena_id: b.object.0,
            card_db_id: o.card_def,
            owner: o.owner.into(),
            controller: o.controller.into(),
            zone: Zone::Library,
            zone_change_count: b.expected_zone_change_count,
        },
        mana_value: d.mana_value,
        permanent,
        eligible: permanent && d.mana_value <= x,
        aura: d.attachment.is_some(),
    })
}
fn public_host_snapshot(
    state: &GameState,
    b: EffectObjectBinding,
) -> Result<crate::rl::CardStableRefV1, String> {
    validate_effect_object_binding(state, b)?;
    let r = crate::optional_trigger_public_v1::reference(state, b.object, None)
        .ok_or("Genesis Wave Aura host is not public")?;
    let card_db_id = match r.identity {
        crate::optional_trigger_public_v1::IdentityV1::Known { card_id } => card_id,
        crate::optional_trigger_public_v1::IdentityV1::FaceDown { disguised } => {
            card_def::card_id_by_name(if disguised {
                "Face-down creature"
            } else {
                "Face-down card"
            })
            .ok_or("Genesis Wave neutral host definition missing")?
        }
    };
    if r.zone != Zone::Battlefield {
        return Err("Genesis Wave Aura host left battlefield".into());
    }
    Ok(crate::rl::CardStableRefV1 {
        arena_id: r.object,
        card_db_id,
        owner: r.owner,
        controller: r.controller,
        zone: r.zone,
        zone_change_count: r.generation,
    })
}
fn validate_public_snapshots(c: &ChoiceV1, x: u16) -> Result<(), String> {
    if c.revealed.len() != c.prefix.len() || c.attachment_sources.len() != c.attachments.len() {
        return Err("Genesis Wave public snapshot cardinality changed".into());
    }
    for (b, r) in c.prefix.iter().zip(&c.revealed) {
        let d = card_def::CARD_DEFS
            .get(usize::from(r.card.card_db_id))
            .ok_or("Genesis Wave revealed definition missing")?;
        let permanent = [
            CardType::Artifact,
            CardType::Creature,
            CardType::Enchantment,
            CardType::Land,
            CardType::Planeswalker,
        ]
        .iter()
        .any(|t| d.has_type(*t));
        if r.card.arena_id != b.object.0
            || r.card.zone_change_count != b.expected_zone_change_count
            || r.card.zone != b.expected_zone
            || r.card.zone != Zone::Library
            || r.card.owner != r.card.controller
            || r.mana_value != d.mana_value
            || r.permanent != permanent
            || r.eligible != (permanent && d.mana_value <= x)
            || r.aura != d.attachment.is_some()
        {
            return Err("Genesis Wave original public reveal snapshot changed".into());
        }
    }
    for ((_, b), r) in c.attachments.iter().zip(&c.attachment_sources) {
        if r.arena_id != b.object.0
            || r.zone_change_count != b.expected_zone_change_count
            || r.zone != b.expected_zone
            || r.zone != Zone::Battlefield
        {
            return Err("Genesis Wave original public Aura host changed".into());
        }
    }
    Ok(())
}
pub(crate) fn selection_purpose_v69(c: &ChoiceV1) -> crate::rl::TargetSelectionPurposeV4 {
    match c.phase {
        PhaseV1::Subset => crate::rl::TargetSelectionPurposeV4::CardSelection,
        PhaseV1::Aura { .. } => crate::rl::TargetSelectionPurposeV4::PermanentSelection,
        PhaseV1::Graveyard { .. } | PhaseV1::RemainderCommitted { .. } => {
            crate::rl::TargetSelectionPurposeV4::LibraryOrder
        }
    }
}
/// Native journal validation precedes sanitation. No original/remaining private
/// library, operation world, history position or executable stack ID is returned.
pub(crate) fn public_facts_v69(
    state: &GameState,
    viewer: PlayerId,
) -> Result<crate::genesis_wave_public_v69::FactsV69, String> {
    use crate::genesis_wave_public_v69::{self as public, AttachmentV69, StageV69, WaveV69};
    validate_state(state)?;
    // The native cast placeholder already has its source contract, but X and
    // payment are still pending. It belongs to the common cast observation,
    // not the inventory of paid, published Wave operations below.
    let announcing = state
        .engine
        .pending_cast
        .as_ref()
        .filter(|pending| definition(pending.source_contract.card_def));
    if let Some(pending) = announcing {
        crate::engine::validate_pending_cast(state, pending)?;
    }
    // A genuine as-enters checkpoint holds the original answered continuation
    // under the existing full enclosing operation. Recover only its public
    // subset/host values; never re-execute it or export that private world.
    let entry_choice = if let Some((original, _)) =
        crate::linked_aura_operation_v1::pending_original_operation(state)
    {
        if let Some(p) = original
            .engine
            .pending_effect
            .as_ref()
            .filter(|p| selected(original, &p.resolving_item))
        {
            if !crate::linked_aura_operation_v1::owns_entry_row_checkpoint_v1(
                state,
                &p.resolving_item,
            ) {
                return Err("V69 entry checkpoint row changed".into());
            }
            crate::linked_aura_operation_v1::validate(state)?;
            validate(original, p)?;
            if let [EffectFrame::GenesisWaveV1 { choice, cards }] = p.frames.as_slice() {
                let mut c = choice.clone();
                match c.phase {
                    PhaseV1::Subset => c.selected = canonical_subset(&c.prefix, cards)?,
                    PhaseV1::Aura { index } => {
                        let host = *cards.first().ok_or("V69 entry host answer absent")?;
                        c.attachments.push((c.selected[usize::from(index)], host));
                        c.attachment_sources
                            .push(public_host_snapshot(original, host)?);
                    }
                    _ => {
                        return Err("V69 entry checkpoint has no answered entry instruction".into())
                    }
                }
                let first = if let PhaseV1::Aura { index } = c.phase {
                    usize::from(index) + 1
                } else {
                    0
                };
                for b in c
                    .selected
                    .iter()
                    .copied()
                    .skip(first)
                    .filter(|b| aura(original, *b))
                {
                    if !returning_aura_hosts_in_zone(original, b, Zone::Library)?.is_empty() {
                        return Err(
                            "V69 entry checkpoint skipped a mandatory Aura host answer".into()
                        );
                    }
                    c.failed_auras.push(b);
                }
                Some((p.resolving_item.v4.stack_item_id, c))
            } else {
                return Err("V69 entry checkpoint erased original answer".into());
            }
        } else {
            None
        }
    } else {
        None
    };
    let mut rows = Vec::<(&StackItem, Option<&ChoiceV1>, bool, Option<Zone>)>::new();
    let mut ids = std::collections::BTreeMap::<StackItemId, usize>::new();
    for e in state.engine.event_history.iter() {
        match e {
            event::CommittedEvent::GenesisWaveProgressV1 { item, choice } => {
                if let Some(i) = ids.get(&item.v4.stack_item_id) {
                    rows[*i] = (item, Some(choice), false, None);
                } else {
                    ids.insert(item.v4.stack_item_id, rows.len());
                    rows.push((item, Some(choice), false, None));
                }
            }
            event::CommittedEvent::GenesisWaveFinishedV1 {
                stack_item_id,
                departure,
                ..
            } => {
                let i = *ids
                    .get(stack_item_id)
                    .ok_or("V69 completion has no native progress")?;
                rows[i].2 = true;
                rows[i].3 = *departure;
            }
            _ => {}
        }
    }
    for item in &state.stack {
        if announcing.is_some_and(|pending| {
            item.kind == crate::state::StackItemKind::Spell
                && item.source == pending.spell
                && item.v4.source_contract == Some(pending.source_contract)
        }) {
            if ids.contains_key(&item.v4.stack_item_id) {
                return Err("V69 announcing cast reused a published Wave occurrence".into());
            }
            continue;
        }
        if selected(state, item) && !ids.contains_key(&item.v4.stack_item_id) {
            crate::engine::validated_stack_item_target_spec(item, state)?;
            if !printed_row_shape(item) {
                return Err("V69 published Genesis row changed".into());
            }
            ids.insert(item.v4.stack_item_id, rows.len());
            rows.push((item, None, false, None));
        }
    }
    if let Some(p) = state
        .engine
        .pending_effect
        .as_ref()
        .filter(|p| pending_selected(state, p))
    {
        validate(state, p)?;
        if let std::collections::btree_map::Entry::Vacant(entry) =
            ids.entry(p.resolving_item.v4.stack_item_id)
        {
            entry.insert(rows.len());
            rows.push((&p.resolving_item, None, false, None));
        }
    }
    let index = |c: &ChoiceV1, b: EffectObjectBinding| -> Result<u16, String> {
        c.prefix
            .iter()
            .position(|r| *r == b)
            .and_then(|i| u16::try_from(i).ok())
            .ok_or("V69 public original revealed slot missing".into())
    };
    let mut waves = Vec::new();
    for (occurrence, (item, c, finished, departure)) in rows.into_iter().enumerate() {
        let c = entry_choice
            .as_ref()
            .filter(|(id, _)| *id == item.v4.stack_item_id)
            .map(|(_, c)| c)
            .or(c);
        let source = item
            .v4
            .source_contract
            .ok_or("V69 original spell source absent")?;
        let mut w = WaveV69 {
            occurrence: u64::try_from(occurrence).map_err(|_| "V69 occurrence width")?,
            stage: StageV69::Published,
            source: crate::rl::CardStableRefV1 {
                arena_id: source.source.0,
                card_db_id: source.card_def,
                owner: source.owner.into(),
                controller: source.controller.into(),
                zone: source.zone,
                zone_change_count: source.zone_change_count,
            },
            controller: item.controller.into(),
            x_value: item.v4.x_value,
            is_copy: item.is_copy,
            is_flashback: item.is_flashback,
            cast_method: source.cast_method,
            revealed: vec![],
            selected_indices: vec![],
            attachments: vec![],
            failed_aura_indices: vec![],
            remainder_indices: vec![],
            answer_indices: vec![],
            answer_host: None,
            answer_waits_for_advance: false,
            spell_departure: departure,
        };
        if let Some(c) = c {
            w.revealed = c.revealed.clone();
            w.selected_indices = c
                .selected
                .iter()
                .map(|b| index(c, *b))
                .collect::<Result<_, _>>()?;
            w.failed_aura_indices = c
                .failed_auras
                .iter()
                .map(|b| index(c, *b))
                .collect::<Result<_, _>>()?;
            w.attachments = c
                .attachments
                .iter()
                .zip(&c.attachment_sources)
                .map(|((b, _), r)| {
                    Ok(AttachmentV69 {
                        revealed_index: index(c, *b)?,
                        original_host: r.clone(),
                    })
                })
                .collect::<Result<_, String>>()?;
            w.stage = match &c.phase {
                PhaseV1::Subset => StageV69::Subset,
                PhaseV1::Aura { index } => StageV69::AuraHost {
                    selected_index: *index,
                },
                PhaseV1::Graveyard { cards, .. } => {
                    w.remainder_indices = cards
                        .iter()
                        .map(|b| index(c, *b))
                        .collect::<Result<_, _>>()?;
                    StageV69::GraveyardOrder
                }
                PhaseV1::RemainderCommitted { ordered, .. } => {
                    w.remainder_indices = ordered
                        .iter()
                        .map(|b| index(c, *b))
                        .collect::<Result<_, _>>()?;
                    StageV69::RemainderCommitted
                }
            };
            if finished {
                w.stage = StageV69::Finished;
            }
            if entry_choice
                .as_ref()
                .is_some_and(|(id, _)| *id == item.v4.stack_item_id)
            {
                w.stage = StageV69::EntryChoices;
            }
            if let Some(p) = state
                .engine
                .pending_effect
                .as_ref()
                .filter(|p| p.resolving_item.v4.stack_item_id == item.v4.stack_item_id)
            {
                let answer = match (&p.choice, &p.answered_choice_guard, p.frames.as_slice()) {
                    (Some(PendingEffectChoice::SelectTargets { selected, .. }), _, _) => selected
                        .iter()
                        .filter_map(|r| r.expected_object)
                        .collect::<Vec<_>>(),
                    (
                        None,
                        Some(EffectAnsweredChoiceGuard::StandardSelection { .. }),
                        [EffectFrame::GenesisWaveV1 { cards, .. }],
                    ) => {
                        w.answer_waits_for_advance = true;
                        cards.clone()
                    }
                    _ => vec![],
                };
                // Aura host selections are not original revealed slots. Their
                // exact native target semantic remains in the complete menu.
                if matches!(c.phase, PhaseV1::Aura { .. }) {
                    w.answer_host = answer
                        .first()
                        .map(|b| public_host_snapshot(state, *b))
                        .transpose()?;
                } else {
                    w.answer_indices = answer
                        .iter()
                        .map(|b| index(c, *b))
                        .collect::<Result<_, _>>()?;
                }
            }
        }
        waves.push(w);
    }
    let facts=public::FactsV69 {
        schema:public::SCHEMA.into(),producer_sha256:public::producer_sha256(),viewer:viewer.into(),
        meaning:crate::rules_meaning_genesis_wave_v69::printed_meaning()?,
        visible_printings:state.objects.iter().filter_map(|(id,_)|crate::optional_trigger_public_v1::reference(state,id,Some(viewer))).filter(|r|matches!(r.identity,crate::optional_trigger_public_v1::IdentityV1::Known{card_id}if definition(card_id))).collect(),
        waves,
    };
    public::validate(&facts)?;
    Ok(facts)
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[path = "genesis_wave_v1_tests.rs"]
pub(crate) mod tests;
