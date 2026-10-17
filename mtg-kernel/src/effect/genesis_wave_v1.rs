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
fn validate_data(state: &GameState, p: &EffectContinuation, c: &ChoiceV1) -> Result<(), String> {
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
            if bind_library_exact(state, player) != *remaining_library
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
            if bind_library_exact(state, player) != c.original_library {
                return Err(
                    "Genesis Wave original library changed before simultaneous entry".into(),
                );
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
                PhaseV1::Graveyard { .. } => unreachable!(),
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
        .last()
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
        if *remaining_library
            != c.original_library
                .iter()
                .copied()
                .filter(|b| !entering.contains(b))
                .collect::<Vec<_>>()
        {
            return Err("Genesis Wave entry changed an unrelated library member".into());
        }
    }
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
            event::CommittedEvent::GenesisWaveFinishedV1 { stack_item_id } if *stack_item_id == p.resolving_item.v4.stack_item_id))
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
    issue(
        state,
        p,
        ChoiceV1 {
            original_library,
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
        PhaseV1::Subset => c.selected = canonical_subset(&c.prefix, &cards)?,
        PhaseV1::Aura { index } => c
            .attachments
            .push((c.selected[usize::from(index)], cards[0])),
        PhaseV1::Graveyard {
            cards: ref original,
            ..
        } => {
            // The order menu contains only actual would-GY entrants. Keep
            // replaced members in their original relative order and delegate
            // every actual replacement and mandatory shuffle to the batch.
            let mut all = cards.clone();
            all.extend(original.iter().copied().filter(|b| !cards.contains(b)));
            commit_zone_change_batch(state, &all, Zone::Graveyard, true)?;
            if state.engine.halted.is_some() {
                return Err("Genesis Wave graveyard batch halted".into());
            }
            state
                .engine
                .event_history
                .push(event::CommittedEvent::GenesisWaveFinishedV1 {
                    stack_item_id: p.resolving_item.v4.stack_item_id,
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
        std::collections::BTreeMap::<StackItemId, (u64, &StackItem, &ChoiceV1, bool)>::new();
    for event in &state.engine.event_history {
        match event {
            event::CommittedEvent::GenesisWaveProgressV1 { item, choice } => {
                let id = item.v4.stack_item_id;
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
                if let Some((ordinal, original, old, finished)) = progress.get(&id) {
                    if *finished
                        || item.as_ref() != *original
                        || choice.ordinal != ordinal + 1
                        || choice.original_library != old.original_library
                        || choice.prefix != old.prefix
                        || matches!(choice.phase, PhaseV1::Subset)
                        || matches!(old.phase, PhaseV1::Graveyard { .. })
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
                progress.insert(id, (choice.ordinal, item, choice, false));
            }
            event::CommittedEvent::GenesisWaveFinishedV1 { stack_item_id } => {
                let Some((_, _, choice, finished)) = progress.get_mut(stack_item_id) else {
                    return Err("Genesis Wave completion has no original progress".into());
                };
                if *finished || !matches!(choice.phase, PhaseV1::Graveyard { .. }) {
                    return Err("Genesis Wave completion repeated or premature".into());
                }
                *finished = true;
            }
            _ => {}
        }
    }
    for (id, (_, item, _, finished)) in progress {
        if !finished {
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

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[path = "genesis_wave_v1_tests.rs"]
mod tests;
