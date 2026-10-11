//! Discover and hideaway keep exact incarnations through their private/public choices.
use super::*;
use crate::state::{FaceDownV1, ObjectLinkV4};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HideawayChoice {
    pub count: u8,
    pub prefix: Vec<EffectObjectBinding>,
    pub path: Vec<u16>,
    pub remaining: Vec<EffectFrame>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExilePlayChoice {
    pub card: Option<EffectObjectBinding>,
    /// Some identifies discover. None identifies the source's linked hideaway card.
    pub limit: Option<u16>,
    pub rejected: Vec<EffectObjectBinding>,
    pub original_library: Vec<EffectObjectBinding>,
    pub path: Vec<u16>,
    pub remaining: Vec<EffectFrame>,
}

fn bind(state: &GameState, object: ObjectId) -> EffectObjectBinding {
    let live = state.objects.get(object);
    EffectObjectBinding {
        object,
        expected_zone: live.zone,
        expected_zone_change_count: live.zone_change_count,
    }
}
fn source(pending: &EffectContinuation) -> Result<ObjectLinkV4, String> {
    let source = pending
        .ctx
        .ability_source_contract
        .as_ref()
        .ok_or("hideaway lost its source contract")?;
    Ok(ObjectLinkV4 {
        object: source.source,
        zone_change_count: source.zone_change_count,
    })
}
fn check_root(
    state: &GameState,
    pending: &EffectContinuation,
    path: &[u16],
    op: EffectOp,
) -> Result<(), String> {
    let root = validated_definition_owned_root_effect(state, pending)?;
    if effect_op_at_path(&root, path) != Some(&op) {
        return Err("exile continuation changed definition-owned root".into());
    }
    Ok(())
}
pub(super) fn validate_hideaway(
    state: &GameState,
    pending: &EffectContinuation,
    choice: &HideawayChoice,
) -> Result<(), String> {
    check_root(
        state,
        pending,
        &choice.path,
        EffectOp::Hideaway {
            count: choice.count,
        },
    )?;
    if pending.frames != choice.remaining
        || bind_library_top(state, pending.ctx.controller, choice.count) != choice.prefix
    {
        return Err("hideaway prefix or remaining frames changed".into());
    }
    validate_bound_library_prefix(state, &choice.prefix)?;
    source(pending)?;
    Ok(())
}
pub(super) fn hideaway(
    state: &mut GameState,
    pending: &mut EffectContinuation,
    count: u8,
    path: Vec<u16>,
) -> Result<bool, String> {
    let choice = HideawayChoice {
        count,
        prefix: bind_library_top(state, pending.ctx.controller, count),
        path: path.clone(),
        remaining: pending.frames.clone(),
    };
    validate_hideaway(state, pending, &choice)?;
    if choice.prefix.is_empty() {
        return Ok(false);
    }
    state.reveal_library_top(
        pending.ctx.controller,
        pending.ctx.controller,
        choice.prefix.len(),
    );
    pending.choice = Some(PendingEffectChoice::SelectTargets {
        player: pending.ctx.controller,
        path,
        selected: vec![],
        legal: choice
            .prefix
            .iter()
            .map(|b| EffectTargetCandidate {
                target: Target::Object(b.object),
                expected_object: Some(*b),
            })
            .collect(),
        min_targets: 1,
        max_targets: 1,
        ordered: false,
        purpose: EffectTargetSelectionPurpose::Hideaway { choice },
    });
    Ok(true)
}
pub(super) fn finish_hideaway(
    state: &mut GameState,
    pending: &EffectContinuation,
    choice: &HideawayChoice,
    selected: &[EffectObjectBinding],
) -> Result<(), String> {
    validate_hideaway(state, pending, choice)?;
    if selected.len() != 1 || !choice.prefix.contains(&selected[0]) {
        return Err("hideaway must choose one exact looked-at card".into());
    }
    let mut staged = state.clone();
    let card = selected[0];
    event::propose_and_commit(
        &mut staged,
        event::ProposedEvent::zone_change(card.object, Zone::Exile),
    );
    if staged.objects.get(card.object).zone == Zone::Exile {
        staged.objects.get_mut(card.object).v4.face_down_v1 = Some(FaceDownV1 {
            disguised: false,
            lookers: 1u8 << pending.ctx.controller.index(),
            hidden_by: Some(source(pending)?),
        });
    }
    let rest = choice
        .prefix
        .iter()
        .filter(|b| **b != card)
        .copied()
        .collect::<Vec<_>>();
    bottom(&mut staged, pending.ctx.controller, &rest)?;
    *state = staged;
    Ok(())
}
fn qualifies(state: &GameState, card: ObjectId, limit: u16) -> bool {
    let def = &crate::card_def::CARD_DEFS[state.objects.get(card).card_def as usize];
    !def.types.contains(&CardType::Land) && def.mana_value <= limit
}
pub(super) fn discover(
    state: &mut GameState,
    pending: &mut EffectContinuation,
    limit: u16,
    path: Vec<u16>,
) -> Result<bool, String> {
    check_root(state, pending, &path, EffectOp::Discover { limit })?;
    let original_library = state.players[pending.ctx.controller.index()]
        .library
        .iter()
        .map(|id| bind(state, *id))
        .collect::<Vec<_>>();
    let mut staged = state.clone();
    let mut choice = ExilePlayChoice {
        card: None,
        limit: Some(limit),
        rejected: vec![],
        original_library,
        path: path.clone(),
        remaining: pending.frames.clone(),
    };
    for original in &choice.original_library {
        event::propose_and_commit(
            &mut staged,
            event::ProposedEvent::zone_change(original.object, Zone::Exile),
        );
        let exiled = bind(&staged, original.object);
        if exiled.expected_zone != Zone::Exile {
            return Err("discover exile was replaced by unsupported destination".into());
        }
        if qualifies(&staged, exiled.object, limit) {
            choice.card = Some(exiled);
            break;
        }
        choice.rejected.push(exiled);
    }
    if can_play(&staged, pending.ctx.controller, &choice) {
        *state = staged;
        pending.choice = Some(PendingEffectChoice::ChooseBoolean {
            player: pending.ctx.controller,
            path,
            default: Some(false),
            purpose: EffectBooleanChoicePurpose::ExilePlay { choice },
        });
        Ok(true)
    } else {
        finish_play(&mut staged, pending, &choice, false)?;
        *state = staged;
        Ok(false)
    }
}
fn linked_card(
    state: &GameState,
    pending: &EffectContinuation,
) -> Result<Option<EffectObjectBinding>, String> {
    let source = source(pending)?;
    Ok(state
        .objects
        .iter()
        .find(|(_, o)| {
            o.zone == Zone::Exile
                && o.v4
                    .face_down_v1
                    .is_some_and(|face| face.hidden_by == Some(source))
        })
        .map(|(id, _)| bind(state, id)))
}
fn three_powers(state: &GameState, controller: PlayerId) -> bool {
    let mut powers = Vec::new();
    for (id, o) in state.objects.iter() {
        if o.zone == Zone::Battlefield
            && o.controller == controller
            && crate::engine::object_has_type(state, id, CardType::Creature)
        {
            let power = crate::engine::effective_power(state, id);
            if !powers.contains(&power) {
                powers.push(power);
            }
        }
    }
    powers.len() >= 3
}
pub(super) fn offer_hideaway_play(
    state: &mut GameState,
    pending: &mut EffectContinuation,
    path: Vec<u16>,
) -> Result<bool, String> {
    check_root(
        state,
        pending,
        &path,
        EffectOp::PlayHideawayIfThreeDistinctPowers,
    )?;
    if !three_powers(state, pending.ctx.controller) {
        return Ok(false);
    }
    let choice = ExilePlayChoice {
        card: linked_card(state, pending)?,
        limit: None,
        rejected: vec![],
        original_library: vec![],
        path: path.clone(),
        remaining: pending.frames.clone(),
    };
    if !can_play(state, pending.ctx.controller, &choice) {
        return Ok(false);
    }
    if let Some(card) = choice.card {
        state
            .objects
            .get_mut(card.object)
            .v4
            .face_down_v1
            .as_mut()
            .expect("linked hideaway card")
            .lookers |= 1u8 << pending.ctx.controller.index();
    }
    pending.choice = Some(PendingEffectChoice::ChooseBoolean {
        player: pending.ctx.controller,
        path,
        default: Some(false),
        purpose: EffectBooleanChoicePurpose::ExilePlay { choice },
    });
    Ok(true)
}
pub(super) fn can_play(state: &GameState, controller: PlayerId, choice: &ExilePlayChoice) -> bool {
    choice.card.is_some_and(|card| {
        crate::engine::can_cast_exiled_without_mana(state, controller, card, choice.limit)
            || (choice.limit.is_none()
                && crate::engine::can_play_exiled_land(state, controller, card))
    })
}
pub(super) fn validate_play(
    state: &GameState,
    pending: &EffectContinuation,
    choice: &ExilePlayChoice,
    hit_may_have_moved: bool,
) -> Result<(), String> {
    if pending.frames != choice.remaining {
        return Err("exile play continuation changed".into());
    }
    if let Some(limit) = choice.limit {
        check_root(state, pending, &choice.path, EffectOp::Discover { limit })?;
        let scanned = choice.rejected.len() + usize::from(choice.card.is_some());
        if scanned > choice.original_library.len()
            || (choice.card.is_none() && scanned != choice.original_library.len())
        {
            return Err("discover scan extent changed".into());
        }
        let current = state.players[pending.ctx.controller.index()]
            .library
            .iter()
            .map(|id| bind(state, *id))
            .collect::<Vec<_>>();
        if current != choice.original_library[scanned..] {
            return Err("discover remaining library changed".into());
        }
        for (index, exiled) in choice.rejected.iter().chain(choice.card.iter()).enumerate() {
            let prior = choice.original_library[index];
            if prior.expected_zone != Zone::Library
                || exiled.expected_zone != Zone::Exile
                || prior.object != exiled.object
                || prior.expected_zone_change_count.checked_add(1)
                    != Some(exiled.expected_zone_change_count)
            {
                return Err("discover incarnation history changed".into());
            }
            if qualifies(state, exiled.object, limit) != (index == choice.rejected.len()) {
                return Err("discover skipped an eligible card".into());
            }
            if index < choice.rejected.len() || !hit_may_have_moved {
                validate_effect_object_binding(state, *exiled)?;
            }
        }
    } else {
        check_root(
            state,
            pending,
            &choice.path,
            EffectOp::PlayHideawayIfThreeDistinctPowers,
        )?;
        if !choice.original_library.is_empty()
            || !choice.rejected.is_empty()
            || choice.card != linked_card(state, pending)?
            || !three_powers(state, pending.ctx.controller)
        {
            return Err("hideaway linked card or powers changed".into());
        }
        if let Some(card) = choice.card {
            validate_effect_object_binding(state, card)?;
        }
    }
    Ok(())
}
pub(super) fn finish_play(
    state: &mut GameState,
    pending: &mut EffectContinuation,
    choice: &ExilePlayChoice,
    play: bool,
) -> Result<(), String> {
    validate_play(state, pending, choice, false)?;
    if play {
        if !can_play(state, pending.ctx.controller, choice) {
            return Err("chosen exiled card cannot be played".into());
        }
        let card = choice.card.ok_or("exile choice lost its card")?;
        if choice.limit.is_some() {
            pending.frames.push(EffectFrame::DiscoverRemainder {
                choice: choice.clone(),
            });
        }
        let op = if choice.limit.is_none()
            && crate::engine::can_play_exiled_land(state, pending.ctx.controller, card)
        {
            EffectOp::PlayExiledLand { card }
        } else {
            EffectOp::CastExiledWithoutMana {
                card,
                maximum_mana_value: choice.limit,
            }
        };
        pending.frames.push(EffectFrame::Program {
            op,
            path: choice.path.clone(),
        });
    } else if choice.limit.is_some() {
        let mut staged = state.clone();
        if let Some(card) = choice.card {
            event::propose_and_commit(
                &mut staged,
                event::ProposedEvent::zone_change(card.object, Zone::Hand),
            );
            let owner = staged.objects.get(card.object).owner;
            if staged.objects.get(card.object).zone == Zone::Hand {
                for observer in [PlayerId::P0, PlayerId::P1] {
                    staged
                        .reveal_hand_card(observer, owner, card.object)
                        .map_err(|e| e.to_string())?;
                }
            }
        }
        bottom(&mut staged, pending.ctx.controller, &choice.rejected)?;
        *state = staged;
    }
    Ok(())
}
pub(super) fn finish_remainder(
    state: &mut GameState,
    pending: &EffectContinuation,
    choice: &ExilePlayChoice,
) -> Result<(), String> {
    if choice.limit.is_none() {
        return Err("hideaway cannot carry a discover remainder".into());
    }
    validate_play(state, pending, choice, true)?;
    let mut staged = state.clone();
    bottom(&mut staged, pending.ctx.controller, &choice.rejected)?;
    *state = staged;
    Ok(())
}
fn bottom(
    state: &mut GameState,
    player: PlayerId,
    cards: &[EffectObjectBinding],
) -> Result<(), String> {
    for card in cards {
        validate_effect_object_binding(state, *card)?;
    }
    for card in cards {
        event::propose_and_commit(
            state,
            event::ProposedEvent::private_bottom_library_insert(card.object),
        );
    }
    state
        .randomize_library_bottom_v1(player, cards.len())
        .map_err(|e| e.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExileScanV1 {
    pub player: PlayerId,
    pub original_library: Vec<EffectObjectBinding>,
    pub exiled: Vec<EffectObjectBinding>,
    pub hit: Option<EffectObjectBinding>,
    pub history_start: usize,
    pub history_end: usize,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExileBatchChoice {
    pub players: Vec<PlayerRef>,
    pub predicate: ExileCastPredicateV1,
    pub return_rest_to_bottom: bool,
    pub scans: Vec<ExileScanV1>,
    pub cast: Vec<EffectObjectBinding>,
    pub path: Vec<u16>,
    pub remaining: Vec<EffectFrame>,
}
fn batch_qualifies(state: &GameState, id: ObjectId, predicate: ExileCastPredicateV1) -> bool {
    let definition = &crate::card_def::CARD_DEFS[state.objects.get(id).card_def as usize];
    !definition.types.contains(&CardType::Land)
        && match predicate {
            ExileCastPredicateV1::Nonland => true,
            ExileCastPredicateV1::LegendaryNonlandManaValueLessThan(limit) => {
                definition
                    .supertypes
                    .contains(&crate::card_def::Supertype::Legendary)
                    && definition.mana_value < limit
            }
        }
}
pub(super) fn start_batch(
    state: &mut GameState,
    pending: &mut EffectContinuation,
    players: Vec<PlayerRef>,
    predicate: ExileCastPredicateV1,
    return_rest_to_bottom: bool,
    path: Vec<u16>,
) -> Result<bool, String> {
    check_root(
        state,
        pending,
        &path,
        EffectOp::ExileUntilThenCastV1 {
            players: players.clone(),
            predicate,
            return_rest_to_bottom,
        },
    )?;
    let mut staged = state.clone();
    let mut scans = Vec::new();
    for player in &players {
        let player = pending.ctx.resolve_player(*player, &staged);
        if scans.iter().any(|scan: &ExileScanV1| scan.player == player) {
            return Err("exile scan repeats a player".into());
        }
        let original_library = staged.players[player.index()]
            .library
            .iter()
            .map(|id| bind(&staged, *id))
            .collect::<Vec<_>>();
        let mut scan = ExileScanV1 {
            player,
            original_library,
            exiled: vec![],
            hit: None,
            history_start: staged.engine.event_history.len(),
            history_end: 0,
        };
        for original in &scan.original_library {
            event::propose_and_commit(
                &mut staged,
                event::ProposedEvent::zone_change(original.object, Zone::Exile),
            );
            let card = bind(&staged, original.object);
            if card.expected_zone != Zone::Exile {
                return Err("exile-until encountered unsupported replacement".into());
            }
            scan.exiled.push(card);
            if batch_qualifies(&staged, card.object, predicate) {
                scan.hit = Some(card);
                break;
            }
        }
        scan.history_end = staged.engine.event_history.len();
        scans.push(scan);
    }
    let choice = ExileBatchChoice {
        players,
        predicate,
        return_rest_to_bottom,
        scans,
        cast: vec![],
        path,
        remaining: pending.frames.clone(),
    };
    let suspended = offer_batch(&mut staged, pending, choice)?;
    *state = staged;
    Ok(suspended)
}
pub(super) fn validate_batch(
    state: &GameState,
    pending: &EffectContinuation,
    choice: &ExileBatchChoice,
) -> Result<(), String> {
    check_root(
        state,
        pending,
        &choice.path,
        EffectOp::ExileUntilThenCastV1 {
            players: choice.players.clone(),
            predicate: choice.predicate,
            return_rest_to_bottom: choice.return_rest_to_bottom,
        },
    )?;
    if pending.frames != choice.remaining || choice.players.len() != choice.scans.len() {
        return Err("exile batch continuation changed".into());
    }
    let hits = choice
        .scans
        .iter()
        .filter_map(|scan| scan.hit)
        .collect::<Vec<_>>();
    for (index, card) in choice.cast.iter().enumerate() {
        if !hits.contains(card) || choice.cast[..index].contains(card) {
            return Err("exile batch cast provenance changed".into());
        }
        let live = state.objects.get(card.object);
        if live.zone_change_count<=card.expected_zone_change_count || !state.engine.event_history.iter().skip(choice.scans.last().map_or(0,|scan|scan.history_end)).any(|event|matches!(event,event::CommittedEvent::SpellCast{spell,..} if *spell==card.object)){return Err("exile batch recorded an uncast card".into());}
    }
    for (index, scan) in choice.scans.iter().enumerate() {
        if scan.player != pending.ctx.resolve_player(choice.players[index], state)
            || choice.scans[..index]
                .iter()
                .any(|prior| prior.player == scan.player)
            || scan.exiled.len() > scan.original_library.len()
        {
            return Err("exile batch player or extent changed".into());
        }
        if scan.hit.is_some() && scan.hit != scan.exiled.last().copied() {
            return Err("exile batch hit was not final".into());
        }
        if scan.hit.is_none() && scan.exiled.len() != scan.original_library.len() {
            return Err("exile batch stopped before a hit".into());
        }
        let history = state
            .engine
            .event_history
            .get(scan.history_start..scan.history_end)
            .ok_or("exile batch lost event history")?;
        let moves = history
            .iter()
            .filter_map(|event| match event {
                event::CommittedEvent::ZoneChange {
                    object,
                    from: Zone::Library,
                    to: Zone::Exile,
                    ..
                } => Some(*object),
                _ => None,
            })
            .collect::<Vec<_>>();
        if moves
            != scan
                .exiled
                .iter()
                .map(|card| card.object)
                .collect::<Vec<_>>()
        {
            return Err("exile batch transition history changed".into());
        }
        for (index, card) in scan.exiled.iter().enumerate() {
            let original = scan.original_library[index];
            if original.object != card.object
                || original.expected_zone != Zone::Library
                || card.expected_zone != Zone::Exile
                || original.expected_zone_change_count.checked_add(1)
                    != Some(card.expected_zone_change_count)
                || state.objects.get(card.object).owner != scan.player
                || batch_qualifies(state, card.object, choice.predicate)
                    != (scan.hit == Some(*card))
            {
                return Err("exile batch stop predicate or incarnation changed".into());
            }
            if !choice.cast.contains(card) {
                validate_effect_object_binding(state, *card)?;
            }
        }
    }
    Ok(())
}
pub(super) fn batch_candidates(
    state: &GameState,
    pending: &EffectContinuation,
    choice: &ExileBatchChoice,
) -> Vec<EffectObjectBinding> {
    choice
        .scans
        .iter()
        .filter_map(|scan| scan.hit)
        .filter(|card| {
            !choice.cast.contains(card)
                && crate::engine::can_cast_exiled_without_mana(
                    state,
                    pending.ctx.controller,
                    *card,
                    None,
                )
        })
        .collect()
}
pub(super) fn offer_batch(
    state: &mut GameState,
    pending: &mut EffectContinuation,
    choice: ExileBatchChoice,
) -> Result<bool, String> {
    validate_batch(state, pending, &choice)?;
    let candidates = batch_candidates(state, pending, &choice);
    if candidates.is_empty() {
        finish_batch(state, &choice)?;
        return Ok(false);
    }
    pending.choice = Some(PendingEffectChoice::SelectTargets {
        player: pending.ctx.controller,
        path: choice.path.clone(),
        selected: vec![],
        legal: candidates
            .into_iter()
            .map(|card| EffectTargetCandidate {
                target: Target::Object(card.object),
                expected_object: Some(card),
            })
            .collect(),
        min_targets: 0,
        max_targets: 1,
        ordered: false,
        purpose: EffectTargetSelectionPurpose::ExileBatch { choice },
    });
    Ok(true)
}
pub(super) fn finish_batch_selection(
    state: &mut GameState,
    pending: &mut EffectContinuation,
    mut choice: ExileBatchChoice,
    selected: &[EffectObjectBinding],
) -> Result<(), String> {
    validate_batch(state, pending, &choice)?;
    if selected.len() > 1
        || selected
            .first()
            .is_some_and(|card| !batch_candidates(state, pending, &choice).contains(card))
    {
        return Err("invalid next free spell choice".into());
    }
    if let Some(card) = selected.first().copied() {
        choice.cast.push(card);
        let path = choice.path.clone();
        pending
            .frames
            .push(EffectFrame::ExileBatchResume { choice });
        pending.frames.push(EffectFrame::Program {
            op: EffectOp::CastExiledWithoutMana {
                card,
                maximum_mana_value: None,
            },
            path,
        });
    } else {
        finish_batch(state, &choice)?;
    }
    Ok(())
}
fn finish_batch(state: &mut GameState, choice: &ExileBatchChoice) -> Result<(), String> {
    if !choice.return_rest_to_bottom {
        return Ok(());
    }
    let mut staged = state.clone();
    for scan in &choice.scans {
        let remaining = scan
            .exiled
            .iter()
            .filter(|card| !choice.cast.contains(card))
            .copied()
            .collect::<Vec<_>>();
        bottom(&mut staged, scan.player, &remaining)?;
    }
    *state = staged;
    Ok(())
}
