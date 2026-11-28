use super::*;

/// Recover an unmarked prefix when loading a save written before per-attempt
/// receipts. A previous choice offer identifies its site. The physical Peer
/// and Decree are single-instruction spells with an independently recorded cast.
pub(super) fn attempts(
    state: &GameState,
    pending: &EffectContinuation,
    path: &[u16],
    end: usize,
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
        let drawer = match crate::card_def::CARD_DEFS.get(usize::from(def))?.name {
            "Peer into the Abyss" => match pending.ctx.targets.first()? {
                Target::Player(player) => *player,
                _ => return None,
            },
            _ => return None,
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
    let Some((boundary, drawer)) = offer
        .or_else(inputs)
        .or_else(cast)
        .or_else(turn_draw)
        .or_else(stop)
    else {
        return (None, 0);
    };
    let mut start = None;
    let mut turn_attempts = None;
    let mut answered = false;
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
                start.get_or_insert(boundary);
                count += 1;
                answered = true;
                continue;
            }
            CommittedEvent::Draw { player, .. } if *player == drawer => Some(at),
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
            } else if !answered {
                start.get_or_insert(at);
                count += 1;
            }
            answered = false;
        }
        mill = None;
        milled = 0;
    }
    (start, count)
}

/// Old Peer records identify the cast, but never identified the beginning of
/// its resolution. A later stack activation can draw or dredge in response
/// without moving a spell off the stack. Its attempts cannot be attributed to
/// Peer. New saves mark the generated instruction after all responses resolve.
#[cfg(feature = "eternal-fixtures")]
pub(crate) fn check_legacy_boundary(
    state: &GameState,
    pending: &EffectContinuation,
    spent: u32,
    current_draw: bool,
) -> Result<(), String> {
    let item = &pending.resolving_item;
    if spent <= u32::from(current_draw) || super::generated_path(state, pending).is_some() {
        return Ok(());
    }
    // Old Library saves marked upkeep, but not the draw-step instruction or
    // the accepted Library option. Surplus attempts in this prefix might
    // belong to another upkeep/step draw. Once the first Library draw is
    // authenticated, later attempts are all inside this resolution.
    if item.kind == crate::state::StackItemKind::TriggeredAbility
        && item.v4.ability_source_contract.is_some_and(|source| {
            crate::card_def::CARD_DEFS[usize::from(source.card_def)].name == "Sylvan Library"
        })
    {
        let history = &state.engine.event_history;
        let stop = history
            .of_kinds_v2(&[EventKindV1::MiracleDrawSuspendedV1])
            .find_map(|(at, row)| match row {
                CommittedEvent::MiracleDrawSuspendedV1 {
                    stack_item_id,
                    drawn,
                    path,
                    ..
                } if *stack_item_id == item.v4.stack_item_id && path == &[0] => {
                    Some((at, drawn.object))
                }
                _ => None,
            });
        let before = stop.and_then(|(at,card)| history[..at].iter().rposition(|row| matches!(row,CommittedEvent::Draw {player,object:Some(object)} if *player == item.controller && *object == card))).unwrap_or(history.len());
        if attempts(state, pending, &[0], before).1 > 0 {
            return Err(
                "Legacy Sylvan Library draw has no boundary for its earlier attempts".into(),
            );
        }
    }
    if state.engine.next_stack_item_id > item.v4.stack_item_id.0
        && item.kind == crate::state::StackItemKind::Spell
        && state.objects.try_get(item.source).is_some_and(|card| {
            crate::card_def::CARD_DEFS[usize::from(card.card_def)].name == "Peer into the Abyss"
        })
    {
        return Err(
            "Legacy Peer draw cannot distinguish attempts from an intervening stack item".into(),
        );
    }
    Ok(())
}
