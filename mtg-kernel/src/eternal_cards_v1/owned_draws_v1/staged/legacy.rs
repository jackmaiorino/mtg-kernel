use super::*;

/// Recover an unmarked prefix when loading a save written before per-attempt
/// receipts. A previous choice offer identifies its site. The physical Peer
/// and Decree are single-instruction spells with an independently recorded cast.
pub(crate) fn visit_attempts(
    state: &GameState,
    pending: &EffectContinuation,
    path: &[u16],
    end: usize,
    replacement_offer: bool,
    mut visit: impl FnMut(usize),
) -> (Option<usize>, u32) {
    let history = &state.engine.event_history[..end];
    let item = &pending.resolving_item;
    let offer = history.iter().enumerate().find_map(|(at, row)| match row {
        CommittedEvent::ResolutionDrawChoiceOfferedV1 {
            stack_item_id,
            path: recorded,
            player,
            ..
        } if *stack_item_id == item.v4.stack_item_id && recorded == path => Some((at, *player)),
        _ => None,
    });
    let stop = || {
        let (drawer, card) = history.iter().find_map(|row| match row {
            CommittedEvent::MiracleDrawSuspendedV1 {
                stack_item_id,
                path: recorded,
                player,
                drawn,
                ..
            } if *stack_item_id == item.v4.stack_item_id && recorded == path => {
                Some((*player, drawn.object))
            }
            _ => None,
        })?;
        let at = history.iter().rposition(|row| matches!(row,CommittedEvent::Draw {player,object:Some(drawn)} if *player == drawer && *drawn == card))?;
        Some((at, drawer))
    };
    let inputs = || {
        use crate::effect::EffectOp;
        use crate::standard_cards_v1::StandardOpV1;
        let root = crate::effect::validated_definition_owned_root_effect(state, pending).ok()?;
        let (from, drawer) = match crate::effect::effect_op_at_structural_path(&root, path)? {
            EffectOp::DiscardUpToThenDraw { player, .. }
            | EffectOp::StandardV1(StandardOpV1::MayDiscardUpToThenDraw { player, .. }) => {
                (Zone::Hand, pending.ctx.resolve_player(*player, state))
            }
            EffectOp::StandardV1(StandardOpV1::EternalV1(
                crate::eternal_cards_v1::EternalOpV1::P3hV1(
                    crate::eternal_premodern_p3h_v1::P3hOpV1::DecreeOfPainV1,
                ),
            )) => (Zone::Battlefield, item.controller),
            _ => return None,
        };
        let at = history.iter().rposition(|row|matches!(row,CommittedEvent::ZoneChange {object,from: origin,to: Zone::Graveyard | Zone::Exile,..} if *origin == from &&
            (from == Zone::Battlefield || state.objects.try_get(*object).is_some_and(|card| card.owner == drawer))))?;
        Some((at + 1, drawer))
    };
    let cast = || {
        if !path.is_empty() || item.is_copy || item.kind != crate::state::StackItemKind::Spell {
            return None;
        }
        let def = state.objects.try_get(item.source)?.card_def;
        if crate::card_def::CARD_DEFS.get(usize::from(def))?.name != "Peer into the Abyss" {
            return None;
        }
        let Target::Player(drawer) = *pending.ctx.targets.first()? else {
            return None;
        };
        let cast = history.iter().rposition(
            |row| matches!(row,CommittedEvent::SpellCast{spell,..} if *spell == item.source),
        )?;
        // All responses have left the stack before this spell draws.
        let last_response = history[cast + 1..].iter().rposition(|row| {
            matches!(
                row,
                CommittedEvent::ZoneChange {
                    from: Zone::Stack,
                    ..
                }
            )
        });
        Some((last_response.map_or(cast + 1, |i| cast + 2 + i), drawer))
    };
    let sylvan = item.kind == crate::state::StackItemKind::TriggeredAbility
        && path == [0]
        && item.v4.ability_source_contract.is_some_and(|source| {
            crate::card_def::CARD_DEFS[usize::from(source.card_def)].name == "Sylvan Library"
        });
    let turn_draw = || {
        if !sylvan {
            return None;
        }
        let at = history.iter().rposition(
            |row| matches!(row,CommittedEvent::UpkeepBegan {player} if *player == item.controller),
        )?;
        Some((at + 1, item.controller))
    };
    let treasure = |row: &CommittedEvent, drawer| {
        replacement_offer
            && matches!(row, CommittedEvent::CreateToken { token_def,controller,.. }
            if crate::card_def::CARD_DEFS.get(usize::from(*token_def)).is_some_and(|def| def.name == "Treasure Token")
                && Some(*controller) == crate::eternal_sideboard_s1c_v1::statics::hullbreacher_controller(state, drawer))
    };
    // A printed draw may be nested or copied. Walk back from its first
    // actual Miracle draw (or its still-open choice) over completed draws
    // and whole dredges. A graveyard-to-hand move alone proves no attempt.
    let printed = || {
        use crate::effect::EffectOp;
        let stopped = stop();
        if !replacement_offer && stopped.is_none() {
            return None;
        }
        let root = crate::effect::validated_definition_owned_root_effect(state, pending).ok()?;
        let (EffectOp::DrawCards { player, .. } | EffectOp::DrawCardsDynamic { player, .. }) =
            crate::effect::effect_op_at_structural_path(&root, path)?
        else {
            return None;
        };
        let drawer = pending.ctx.resolve_player(*player, state);
        // No later stack item and no earlier instruction: the physical cast
        // also bounds a prefix containing other event-time bookkeeping.
        if !item.is_copy
            && item.kind == crate::state::StackItemKind::Spell
            && item.v4.stack_item_id.0 == state.engine.next_stack_item_id
            && path
                .iter()
                .try_fold(root.as_ref(), |op, index| match op {
                    EffectOp::Sequence(ops) if *index == 0 => ops.first(),
                    _ => None,
                })
                .is_some()
        {
            if let Some(cast) = history.iter().rposition(|row| {
                matches!(row,
                CommittedEvent::SpellCast { spell, .. } if *spell == item.source)
            }) {
                return Some((cast + 1, drawer));
            }
        }
        let anchor = stopped.map_or(end, |(draw_at, _)| draw_at + 1);
        let unrelated = |row: &CommittedEvent| {
            let same_draw = match row {
                CommittedEvent::Draw { player, .. } => *player == drawer,
                CommittedEvent::ZoneChange {
                    object,
                    from: Zone::Library,
                    to: Zone::Graveyard | Zone::Exile,
                    ..
                }
                | CommittedEvent::ZoneChange {
                    object,
                    from: Zone::Graveyard,
                    to: Zone::Hand,
                    ..
                } => state
                    .objects
                    .try_get(*object)
                    .is_some_and(|card| card.owner == drawer),
                _ => treasure(row, drawer),
            };
            !same_draw
        };
        let cursor = history[..anchor]
            .iter()
            .rposition(unrelated)
            .map_or(0, |at| at + 1);
        Some((cursor, drawer))
    };
    let offer = offer.filter(|_| crate::effect::definition_has_nonterminal_discard(state, pending));
    let Some((boundary, drawer)) = offer
        .or_else(inputs)
        .or_else(cast)
        .or_else(turn_draw)
        .or_else(printed)
        .or_else(stop)
    else {
        return (None, 0);
    };
    let mut start = None;
    let mut turn_attempts = None;
    let mut answered = false;
    let mut extra_results = 0;
    let mut count = 0;
    let mut mill = None;
    let mut milled = 0usize;
    for (at, row) in history.iter().enumerate().skip(boundary) {
        let attempt = match row {
            CommittedEvent::ResolutionDrawChoiceOfferedV1 {
                stack_item_id,
                path: recorded,
                ..
            } if *stack_item_id == item.v4.stack_item_id && recorded == path => {
                answered = false;
                continue;
            }
            CommittedEvent::ResolutionDrawChoiceAnsweredV1 {
                stack_item_id,
                path: recorded,
            } if *stack_item_id == item.v4.stack_item_id && recorded == path => {
                visit(boundary);
                start.get_or_insert(boundary);
                count += 1;
                answered = true;
                continue;
            }
            CommittedEvent::Draw { player, .. } if *player == drawer => Some(at),
            row if treasure(row, drawer) => Some(at),
            CommittedEvent::ZoneChange {
                object, from, to, ..
            } if state
                .objects
                .try_get(*object)
                .is_some_and(|card| card.owner == drawer) =>
            {
                if *from == Zone::Library && matches!(to, Zone::Graveyard | Zone::Exile) {
                    mill.get_or_insert(at);
                    milled += 1;
                    continue;
                }
                if *from == Zone::Graveyard
                    && *to == Zone::Hand
                    && dredge::dredge_count(
                        crate::card_def::CARD_DEFS
                            [usize::from(state.objects.get(*object).card_def)]
                        .name,
                    )
                    .is_some_and(|n| usize::from(n) == milled)
                {
                    mill
                } else {
                    None
                }
            }
            _ => continue,
        };
        if let Some(at) = attempt {
            // The draw step completes its own instruction before the Library
            // trigger resolves. Its first successful draw can itself be dredged.
            let skip = turn_attempts.get_or_insert_with(|| {
                if sylvan {
                    super::super::initial_hand_len(state, drawer, at).map_or(1, |hand| {
                        1 + event::draw_instruction_extra_at_hand_size(state, drawer, 1, hand)
                    })
                } else {
                    0
                }
            });
            if *skip > 0 {
                *skip -= 1;
            } else {
                // The released non-native loop proposed one draw per unanswered
                // replacement. Riddler could produce several Draws or Treasures
                // for that one attempt. A dredge is already a complete attempt.
                let ordinary = replacement_offer
                    && (matches!(row, CommittedEvent::Draw { .. }) || treasure(row, drawer));
                if ordinary && extra_results > 0 {
                    extra_results -= 1;
                } else {
                    extra_results = if ordinary {
                        super::super::initial_hand_len(state, drawer, at).map_or(0, |hand| {
                            event::draw_instruction_extra_at_hand_size(state, drawer, 1, hand)
                        })
                    } else {
                        0
                    };
                    if !answered {
                        visit(at);
                        start.get_or_insert(at);
                        count += 1;
                    }
                }
            }
            answered = false;
        }
        mill = None;
        milled = 0;
    }
    (start, count)
}

/// The released engine could pause these generators at replacement choices,
/// but did not suspend their generated draws with a Miracle remainder.
pub(crate) fn requires_generator_receipt(state: &GameState, pending: &EffectContinuation) -> bool {
    let item = &pending.resolving_item;
    let definition = match item.kind {
        crate::state::StackItemKind::Spell => {
            state.objects.try_get(item.source).map(|c| c.card_def)
        }
        crate::state::StackItemKind::TriggeredAbility => {
            item.v4.ability_source_contract.map(|c| c.card_def)
        }
        _ => None,
    };
    definition.is_some_and(|def| {
        matches!(
            crate::card_def::CARD_DEFS[usize::from(def)].name,
            "Peer into the Abyss" | "Decree of Pain" | "Sylvan Library"
        )
    })
}
