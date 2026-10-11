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
