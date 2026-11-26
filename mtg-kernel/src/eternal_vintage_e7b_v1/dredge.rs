//! Dredge (702.52): "If you would draw a card, you may mill N cards
//! instead. If you do, return this card from your graveyard to your hand."
//!
//! Dredge replaces a draw while the player's library holds at least N
//! cards (702.52b). The choice is offered for every draw an effect makes
//! inside the resumable interpreter (each card of a multi-card draw is its
//! own draw) and for the draw step's draw, which waits as an as-enters
//! style decision (`defer_draw_step_draw`). Effects that resolve through the
//! synchronous interpreter are moved to the resumable one while a dredge
//! choice exists (`forces_resumable`).

use super::{
    binding_link, definition, definition_name, e7b_state, graveyard_cards, link_live_in,
    read_state, E7bChoiceV1, GOLGARI_GRAVE_TROLL, GOLGARI_THUG, SHAMBLING_SHELL, STINKWEED_IMP,
};
use crate::effect::{
    EffectContinuation, EffectFrame, EffectObjectBinding, EffectOp, EffectTargetSelectionPurpose,
    ExecCtx, PendingEffectChoice, PlayerRef,
};
use crate::eternal_cards_v1::{
    delayed_upkeep_draw,
    e3::{E3ChoiceKindV1, E3ChoiceV1},
    owned_draws_v1, EternalOpV1,
};
use crate::eternal_vintage_v1::{
    abandon_coveted_jewel_draws, coveted_jewel_changes_hands, coveted_jewel_draw_stage_matches,
    is_coveted_jewel_entry_draw, paradoxical_outcome, VintageOpV1,
};
use crate::event::{self, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::StandardOpV1;
use crate::state::{GameState, MonarchTriggerBindingV1, Target, Zone};

/// Life from the Loam (wave E7a): "Dredge 3".
const LIFE_FROM_THE_LOAM: &str = "Life from the Loam";

/// A card's dredge number.
pub(crate) fn dredge_count(name: &str) -> Option<u8> {
    match name {
        // Shenanigans (wave S1b).
        "Shenanigans" => Some(1),
        GOLGARI_GRAVE_TROLL => Some(6),
        STINKWEED_IMP => Some(5),
        GOLGARI_THUG => Some(4),
        SHAMBLING_SHELL | LIFE_FROM_THE_LOAM => Some(3),
        _ => None,
    }
}

/// The dredge cards in `player`'s graveyard they could dredge now.
pub(crate) fn dredge_cards(state: &GameState, player: PlayerId) -> Vec<ObjectId> {
    if crate::eternal_cube_c6c_v1::prevents_draw(state, player) {
        return Vec::new();
    }
    let library = state.players[player.index()].library.len();
    graveyard_cards(state, player)
        .into_iter()
        .filter(|&card| {
            definition_name(state, card)
                .and_then(dredge_count)
                .is_some_and(|count| library >= usize::from(count))
        })
        .collect()
}

pub(crate) fn may_need_draw_choice(state: &GameState, player: PlayerId) -> bool {
    if crate::eternal_cube_c6c_v1::prevents_draw(state, player) {
        return false;
    }
    !dredge_cards(state, player).is_empty()
        || (crate::eternal_cards_v1::e3::jace_draw_replacement_source(state, player).is_some()
            && crate::eternal_sideboard_s1c_v1::statics::hullbreacher_controller(state, player)
                .is_some())
}

/// Whether `op` draws cards anywhere in its tree.
fn draws(op: &EffectOp) -> bool {
    match op {
        EffectOp::DrawCards { .. }
        | EffectOp::DrawCardsDynamic { .. }
        | EffectOp::ResolveMonarchTrigger { .. }
        | EffectOp::NivMizzetVisionaryDrawV1 { .. }
        | EffectOp::EachPlayerControllingDefinitionDrawsCard { .. } => true,
        EffectOp::Sequence(ops) => ops.iter().any(draws),
        EffectOp::Conditional { then, else_, .. } => draws(then) || draws(else_),
        EffectOp::Choice { options, .. } => options.iter().any(draws),
        // Premodern wave P1a's leaves that draw (Ice, Skeletal Scrying,
        // Graveborn Muse, Portent's delayed draw).
        EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::P1aV1(op))) => op.draws(),
        EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::DelayedUpkeepDrawV1)) => true,
        EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E7aV1(
            crate::eternal_legacy_e7a_v1::E7aOpV1::EchoOfEonsV1,
        ))) => true,
        EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::VintageV1(
            VintageOpV1::CovetedJewelChangesHandsV1 | VintageOpV1::ParadoxicalOutcomeV1,
        ))) => true,
        EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::P3hV1(
            crate::eternal_premodern_p3h_v1::P3hOpV1::DecreeOfPainV1,
        ))) => true,
        EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E8bV1(
            crate::eternal_pauper_e8b_v1::E8bOpV1::RingLootV1,
        ))) => true,
        _ => false,
    }
}

pub(crate) fn owned_draw_needs_choice(state: &GameState, player: PlayerId, count: u32) -> bool {
    use crate::eternal_sideboard_s1c_v1::statics::{
        hullbreacher_controller, hullbreacher_replaces_draw,
    };

    if crate::eternal_cube_c6c_v1::prevents_draw(state, player) {
        return false;
    }

    if count == 0 {
        return false;
    }
    let count = count + event::draw_instruction_extra(state, player, count);
    if !dredge_cards(state, player).is_empty() {
        return true;
    }
    if crate::eternal_cards_v1::e3::jace_draw_replacement_source(state, player).is_none() {
        return false;
    }
    match state.players[player.index()].library.len() {
        0 => hullbreacher_replaces_draw(state, player).is_some(),
        // The draw step's first draw can consume the last card while Hullbreacher
        // is inactive for that draw. It can then compete with Jace on the next.
        1 if count > 1 => {
            crate::eternal_cards_v1::is_first_draw_in_draw_step(state, player)
                && hullbreacher_controller(state, player).is_some()
        }
        _ => false,
    }
}

/// Whether an owned draw instruction of `count` printed draws must make
/// its attempts one at a time: an attempt may need its drawer's replacement
/// choice, or its first attempt may draw the drawer's first card this turn,
/// a miracle card, while a later attempt remains. That card is revealed as
/// it is drawn, before any later card is drawn (702.94a).
pub(crate) fn owned_draw_needs_stage(state: &GameState, player: PlayerId, count: u32) -> bool {
    owned_draw_needs_choice(state, player, count)
        || (count > 0
            && count.saturating_add(event::draw_instruction_extra(state, player, count)) > 1
            && crate::eternal_legacy_e5b_v1::next_draw_may_offer_miracle(state, player))
}

/// Whether `player` may draw a miracle card as their first draw this turn
/// after their library is shuffled: they have drawn no card this turn and
/// own a miracle card that the shuffle can put on top.
fn may_draw_miracle_after_shuffle(state: &GameState, player: PlayerId) -> bool {
    let owner = &state.players[player.index()];
    owner.draws_this_turn == 0
        && owner
            .library
            .iter()
            .chain(&owner.hand)
            .chain(&owner.graveyard)
            .any(|&card| crate::eternal_legacy_e5b_v1::miracle_cost(state, card).is_some())
}

/// Whether a synchronous program should run in the resumable interpreter
/// so the affected player can choose a draw replacement, or answer a
/// miracle reveal before a later card is drawn.
pub(crate) fn forces_resumable(
    state: &GameState,
    program: &EffectOp,
    ctx: &ExecCtx,
    is_copy: bool,
) -> bool {
    if !cfg!(feature = "eternal-fixtures") {
        return false;
    }
    // Bonder's APNAP order and immediate terminal stop matter even without
    // competing replacements. Its paid native owner stages both instructions.
    if matches!(
        program,
        EffectOp::EachPlayerControllingDefinitionDrawsCard { .. }
    ) {
        return crate::effect::resumable_program_supported(program);
    }
    if owned_draws_v1::knowledge::recipe(program) {
        return crate::effect::resumable_program_supported(program);
    }
    let starting_draws = match program {
        EffectOp::DrawCards { player, count }
            if !is_copy && is_coveted_jewel_entry_draw(ctx, *player, *count) =>
        {
            Some((ctx.controller, *count))
        }
        op if !is_copy && *op == coveted_jewel_changes_hands() => {
            Some((ctx.controller.opponent(), 3))
        }
        op if !is_copy && *op == paradoxical_outcome() => {
            // A return can change which Riddlers apply. Route for a possible
            // second attempt, then check the actual count after the batch.
            let possible_attempts = if ctx.targets.is_empty() { 0 } else { 2 };
            Some((ctx.controller, possible_attempts))
        }
        _ => owned_draws_v1::starting_draws(state, program, ctx),
    };
    let needs_choice = match starting_draws {
        Some((drawer, count)) => owned_draw_needs_stage(state, drawer, count),
        None => {
            (draws(program)
                && [PlayerId::P0, PlayerId::P1]
                    .into_iter()
                    .any(|player| may_need_draw_choice(state, player)))
                // Echo shuffles both libraries before drawing seven.
                || (*program == crate::eternal_legacy_e7a_v1::echo_of_eons()
                    && [PlayerId::P0, PlayerId::P1]
                        .into_iter()
                        .any(|player| may_draw_miracle_after_shuffle(state, player)))
                // A copied Paradoxical Outcome stages one draw of one card
                // per returned permanent, so it is staged for a miracle only
                // while no draw-count replacement applies.
                || (is_copy
                    && *program == paradoxical_outcome()
                    && !ctx.targets.is_empty()
                    && event::draw_instruction_extra(state, ctx.controller, 2) == 0
                    && owned_draw_needs_stage(state, ctx.controller, 2))
        }
    };
    (needs_choice || crate::effect::program_may_draw_miracle(state, program, ctx))
        && crate::effect::resumable_program_supported(program)
}

pub(crate) fn draw_may_need_choice(
    state: &GameState,
    continuation: &EffectContinuation,
    player: PlayerRef,
) -> bool {
    may_need_draw_choice(state, continuation.ctx.resolve_player(player, state))
}

/// Draws one card at a time, suspending when the affected player has a
/// replacement choice. A later draw may qualify after a prior draw or dredge.
pub(crate) fn drive_draws(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    player: PlayerRef,
    mut count: u32,
    path: Vec<u16>,
) -> Result<bool, String> {
    let drawer = continuation.ctx.resolve_player(player, state);
    while count != 0 {
        let Some((kind, cards)) =
            replacement_options(state, continuation, drawer, &path, &continuation.frames)
        else {
            commit_resolution_draw(state, continuation, drawer, &path);
            count -= 1;
            if !owned_draws_v1::draw_stage_matches(
                state,
                continuation,
                drawer,
                &path,
                &continuation.frames,
            ) && crate::eternal_legacy_e5b_v1::uncollected_miracle_draw(state).is_some()
            {
                // A printed draw stops through its authenticated remainder,
                // which offers each later attempt's replacement again.
                #[cfg(feature = "eternal-fixtures")]
                if crate::effect::suspend_miracle_draw(state, continuation, drawer, &path, count) {
                    return Ok(true);
                }
                if count > 0 {
                    continuation.frames.push(EffectFrame::Program {
                        op: EffectOp::DrawCards { player, count },
                        path,
                    });
                }
                return Ok(true);
            }
            continue;
        };
        if count > 1 {
            continuation.frames.push(EffectFrame::Program {
                op: EffectOp::DrawCards {
                    player,
                    count: count - 1,
                },
                path: path.clone(),
            });
        }
        let choice = choice(
            state,
            drawer,
            kind,
            &cards,
            path,
            continuation.frames.clone(),
        );
        crate::eternal_cards_v1::e3::offer(continuation, choice);
        return Ok(true);
    }
    Ok(false)
}

fn commit_resolution_draw(
    state: &mut GameState,
    continuation: &EffectContinuation,
    drawer: PlayerId,
    path: &[u16],
) {
    if owned_draws_v1::draw_stage_matches(state, continuation, drawer, path, &continuation.frames) {
        event::propose_draw_attempt(state, drawer);
    } else {
        event::propose_and_commit(state, ProposedEvent::draw(drawer));
    }
}

fn replacement_options(
    state: &GameState,
    pending: &EffectContinuation,
    drawer: PlayerId,
    path: &[u16],
    remaining: &[EffectFrame],
) -> Option<(E7bChoiceV1, Vec<ObjectId>)> {
    if crate::eternal_cube_c6c_v1::prevents_draw(state, drawer) {
        return None;
    }
    let cards = dredge_cards(state, drawer);
    if !cards.is_empty() {
        return Some((E7bChoiceV1::DredgeV1, cards));
    }
    if !state.players[drawer.index()].library.is_empty()
        || crate::eternal_sideboard_s1c_v1::statics::hullbreacher_replaces_draw(state, drawer)
            .is_none()
    {
        return None;
    }
    let jace = crate::eternal_cards_v1::e3::jace_draw_replacement_source(state, drawer)?;
    crate::effect::jace_draw_replacement_origin(state, pending, drawer, path, remaining)
        .then(|| (E7bChoiceV1::JaceOrHullbreacherV1, vec![jace]))
}

fn choice(
    state: &GameState,
    drawer: PlayerId,
    kind: E7bChoiceV1,
    cards: &[ObjectId],
    path: Vec<u16>,
    remaining: Vec<EffectFrame>,
) -> E3ChoiceV1 {
    E3ChoiceV1 {
        kind: E3ChoiceKindV1::E7bV1(kind),
        player: drawer,
        candidates: cards.iter().copied().map(Target::Object).collect(),
        bindings: cards.iter().map(|&card| super::bind(state, card)).collect(),
        min: 0,
        max: 1,
        ordered: false,
        path,
        remaining,
    }
}

pub(crate) fn validate_monarch_draw(
    state: &GameState,
    pending: &EffectContinuation,
    binding: MonarchTriggerBindingV1,
) -> Result<(), String> {
    let item = &pending.resolving_item;
    if pending.ctx.controller != binding.player
        || pending.ctx.source != item.source
        || pending.ctx.ability_source_contract != Some(binding.source)
        || pending.ctx.stack_item_id != Some(item.v4.stack_item_id)
        || pending.ctx.targets != item.targets
        || pending.ctx.target_contracts != item.v4.target_contracts
    {
        return Err("Monarch draw changed its original trigger".into());
    }
    validate_single_draw(
        state,
        pending,
        binding.player,
        &EffectOp::ResolveMonarchTrigger { binding },
        "Monarch draw",
    )
}

pub(crate) fn validate_bauble_draw(
    state: &GameState,
    pending: &EffectContinuation,
) -> Result<(), String> {
    let root = delayed_upkeep_draw();
    if pending.resolving_item.inline_effect.as_ref() != Some(&root) {
        return Ok(());
    }
    let item = &pending.resolving_item;
    if item.kind != crate::state::StackItemKind::TriggeredAbility
        || !item.v4.ability_source_contract.is_some_and(|source| {
            source.zone == Zone::Battlefield && source.controller == item.controller
        })
    {
        return Err("Bauble draw changed its original trigger".into());
    }
    validate_single_draw(state, pending, item.controller, &root, "Bauble draw")
}

fn validate_single_draw(
    state: &GameState,
    pending: &EffectContinuation,
    drawer: PlayerId,
    root: &EffectOp,
    owner: &str,
) -> Result<(), String> {
    if matches!((&pending.choice, pending.frames.as_slice()),
        (None, [EffectFrame::Program { op, path }]) if op == root && path.is_empty())
    {
        return Ok(());
    }
    validate_draw_choice(state, pending, drawer, &[], &[], owner)
}

pub(crate) fn validate_coveted_jewel_draw(
    state: &GameState,
    pending: &EffectContinuation,
) -> Result<(), String> {
    let root = coveted_jewel_changes_hands();
    if pending.resolving_item.inline_effect.as_ref() != Some(&root) {
        return if state.coveted_jewel_draw_progress_v1.is_none() {
            Ok(())
        } else {
            Err("Coveted Jewel draw progress lost its original combat trigger".into())
        };
    }
    let item = &pending.resolving_item;
    if item.kind != crate::state::StackItemKind::TriggeredAbility
        || !item.v4.ability_source_contract.is_some_and(|source| {
            source.zone == Zone::Battlefield && source.controller == item.controller
        })
    {
        return Err("Coveted Jewel changed its original combat trigger".into());
    }
    if owned_draws_v1::has_started(state, item.v4.stack_item_id) {
        if state.eternal_owned_draw_progress_v1.is_some()
            && state.coveted_jewel_draw_progress_v1.is_none()
            && !item.is_copy
        {
            return Ok(()); // The shared owned-draw validator checks the native instruction.
        }
        return Err("Coveted Jewel already started its original draw".into());
    }
    if state.coveted_jewel_draw_progress_v1.is_none()
        && matches!((&pending.choice, pending.frames.as_slice()),
        (None, [EffectFrame::Program { op, path }]) if *op == root && path.is_empty())
    {
        return Ok(());
    }
    let (path, remaining) =
        draw_stage(pending).ok_or("Coveted Jewel lost its original draw stage")?;
    if !coveted_jewel_draw_stage_matches(state, &pending.ctx, path, remaining) {
        return Err("Coveted Jewel draw changed its progress or remaining effects".into());
    }
    validate_draw_choice(
        state,
        pending,
        item.controller.opponent(),
        path,
        remaining,
        "Coveted Jewel draw",
    )
}

pub(crate) fn draw_stage(pending: &EffectContinuation) -> Option<(&[u16], &[EffectFrame])> {
    match (&pending.choice, pending.frames.split_last()) {
        (Some(PendingEffectChoice::SelectTargets { path, .. }), _) => {
            Some((path.as_slice(), pending.frames.as_slice()))
        }
        (None, Some((EffectFrame::EternalSelectionV1 { choice, .. }, rest))) => {
            Some((choice.path.as_slice(), rest))
        }
        _ => None,
    }
}

pub(crate) fn validate_draw_choice(
    state: &GameState,
    pending: &EffectContinuation,
    drawer: PlayerId,
    path: &[u16],
    remaining: &[EffectFrame],
    owner: &str,
) -> Result<(), String> {
    let (kind, cards) = replacement_options(state, pending, drawer, path, remaining)
        .ok_or_else(|| format!("{owner} lost its replacement choices"))?;
    let expected = choice(
        state,
        drawer,
        kind,
        &cards,
        path.to_vec(),
        remaining.to_vec(),
    );
    let valid = match (&pending.choice, pending.frames.as_slice()) {
        (
            Some(PendingEffectChoice::SelectTargets {
                purpose: EffectTargetSelectionPurpose::EternalV1 { choice },
                selected,
                legal,
                min_targets: 0,
                ..
            }),
            frames,
        ) => {
            frames == remaining
                && **choice == expected
                && selected.is_empty()
                && legal
                    .iter()
                    .map(|candidate| candidate.target)
                    .eq(expected.candidates)
        }
        (None, [frames @ .., EffectFrame::EternalSelectionV1 { choice, selected }]) => {
            frames == remaining
                && **choice == expected
                && selected.len() <= 1
                && selected
                    .iter()
                    .all(|target| expected.candidates.contains(target))
        }
        _ => false,
    };
    if !valid {
        return Err(format!("{owner} changed its native replacement choice"));
    }
    Ok(())
}

pub(crate) fn validate_jace_or_hullbreacher(
    state: &GameState,
    pending: &EffectContinuation,
) -> Result<(), String> {
    let (choice, remaining) = match (&pending.choice, pending.frames.split_last()) {
        (
            Some(PendingEffectChoice::SelectTargets {
                purpose: EffectTargetSelectionPurpose::EternalV1 { choice },
                selected,
                min_targets,
                ..
            }),
            _,
        ) if jace_choice_kind(choice) => {
            if !selected.is_empty() || *min_targets != 0 {
                return Err("Jace draw replacement prompt changed shape".into());
            }
            (choice, pending.frames.as_slice())
        }
        (None, Some((EffectFrame::EternalSelectionV1 { choice, .. }, rest)))
            if jace_choice_kind(choice) =>
        {
            (choice, rest)
        }
        _ => return Ok(()),
    };
    validate_jace_choice(state, pending, choice, remaining)
}

fn jace_choice_kind(choice: &E3ChoiceV1) -> bool {
    matches!(
        choice.kind,
        E3ChoiceKindV1::E7bV1(E7bChoiceV1::JaceOrHullbreacherV1)
    )
}

fn validate_jace_choice(
    state: &GameState,
    pending: &EffectContinuation,
    offered: &E3ChoiceV1,
    remaining: &[EffectFrame],
) -> Result<(), String> {
    let (kind, cards) =
        replacement_options(state, pending, offered.player, &offered.path, remaining)
            .ok_or("Jace draw replacement lost its original draw or source")?;
    let expected = choice(
        state,
        offered.player,
        kind,
        &cards,
        offered.path.clone(),
        remaining.to_vec(),
    );
    if offered != &expected {
        return Err("Jace draw replacement changed its native choices".into());
    }
    Ok(())
}

pub(crate) fn finish_jace_or_hullbreacher(
    state: &mut GameState,
    pending: &mut EffectContinuation,
    choice: &E3ChoiceV1,
    chosen: Option<EffectObjectBinding>,
) -> Result<(), String> {
    validate_jace_choice(state, pending, choice, &pending.frames)?;
    let player = choice.player;
    if chosen.is_some() {
        // The chooser selected the other replacement. The ordinary commit
        // applies Jace instead of running Hullbreacher's proposal hook.
        event::commit(state, ProposedEvent::draw(player));
        let opponent = player.opponent();
        if state.players[opponent.index()].has_lost
            && !crate::game_outcome_protection_v1::cannot_lose(state, opponent)
            && state.eternal_owned_draw_progress_v1.is_none()
        {
            pending.frames.clear();
            abandon_coveted_jewel_draws(state, &pending.ctx);
            crate::eternal_vintage_v1::paradoxical_outcome_v1::abandon(state, &pending.ctx);
        }
    } else {
        commit_resolution_draw(state, pending, player, &choice.path);
    }
    Ok(())
}

pub(crate) fn deferring_captures(state: &GameState) -> bool {
    read_state(state).is_some_and(|e7b| e7b.dredge_deferring)
}

pub(crate) fn waiting_triggers(state: &GameState) -> &[crate::trigger::PendingTrigger] {
    read_state(state).map_or(&[], |e7b| e7b.dredge_captures.as_slice())
}

pub(crate) fn defer_captures(state: &mut GameState, captures: Vec<crate::trigger::PendingTrigger>) {
    let mut combined = std::mem::take(&mut state.engine.pending_triggers);
    combined.extend(captures);
    e7b_state(state).dredge_deferring = true;
    #[cfg(feature = "eternal-fixtures")]
    let combined = match crate::engine::resolution_cast_v1::defer_dredge_captures(state, combined) {
        Ok(()) => return,
        Err(combined) => combined,
    };
    e7b_state(state).dredge_captures.extend(combined);
}

pub(crate) fn take_captures_keep_deferring(
    state: &mut GameState,
) -> Vec<crate::trigger::PendingTrigger> {
    let Some(eternal) = state.eternal_v1.as_mut() else {
        return Vec::new();
    };
    std::mem::take(&mut eternal.e7b.dredge_captures)
}

pub(crate) fn prepend_captures(
    state: &mut GameState,
    mut earlier: Vec<crate::trigger::PendingTrigger>,
) {
    let e7b = e7b_state(state);
    e7b.dredge_deferring = true;
    earlier.append(&mut e7b.dredge_captures);
    e7b.dredge_captures = earlier;
}

pub(crate) fn take_captures(state: &mut GameState) -> Vec<crate::trigger::PendingTrigger> {
    let Some(eternal) = state.eternal_v1.as_mut() else {
        return Vec::new();
    };
    eternal.e7b.dredge_deferring = false;
    std::mem::take(&mut eternal.e7b.dredge_captures)
}

pub(crate) fn stop_deferring(state: &mut GameState) {
    if let Some(eternal) = state.eternal_v1.as_mut() {
        eternal.e7b.dredge_deferring = false;
    }
}

pub(crate) fn prepend_checkpoint_captures(
    state: &mut GameState,
    mut earlier: Vec<crate::trigger::PendingTrigger>,
) {
    if earlier.is_empty() {
        return;
    }
    let e7b = e7b_state(state);
    e7b.dredge_deferring = false;
    earlier.append(&mut e7b.dredge_captures);
    e7b.dredge_captures = earlier;
}

pub(crate) fn store_checkpoint_captures(
    state: &mut GameState,
    captures: Vec<crate::trigger::PendingTrigger>,
) {
    if captures.is_empty() {
        return;
    }
    let e7b = e7b_state(state);
    e7b.dredge_deferring = false;
    e7b.dredge_captures = captures;
}

/// Applies a dredge choice: no card draws, a card dredges.
pub(crate) fn finish(
    state: &mut GameState,
    pending: &EffectContinuation,
    choice: &E3ChoiceV1,
    chosen: Option<EffectObjectBinding>,
) {
    let player = choice.player;
    let Some(card) = chosen else {
        commit_resolution_draw(state, pending, player, &choice.path);
        return;
    };
    if !dredge(
        state,
        player,
        card.object,
        binding_link(&card),
        Some(&pending.resolving_item),
    ) && state.engine.halted.is_none()
    {
        commit_resolution_draw(state, pending, player, &choice.path);
    }
}

/// After a replacement choice's draw: when the card drawn is its drawer's
/// first this turn and has miracle, a printed draw stops through its
/// authenticated remainder (702.94a), which takes over the rest of the
/// instruction that the choice staged below it. An owned draw stage stops
/// at its own staged next attempt instead.
#[cfg(feature = "eternal-fixtures")]
pub(crate) fn stop_after_choice_draw(
    state: &mut GameState,
    pending: &mut EffectContinuation,
    drawer: PlayerId,
    path: &[u16],
) {
    if crate::eternal_legacy_e5b_v1::uncollected_miracle_draw(state).is_none()
        || owned_draws_v1::draw_stage_matches(state, pending, drawer, path, &pending.frames)
    {
        return;
    }
    let rest = match pending.frames.last() {
        Some(EffectFrame::Program {
            op: EffectOp::DrawCards { count, .. },
            path: rest_path,
        }) if rest_path.as_slice() == path => Some(*count),
        _ => None,
    };
    let staged = rest.and_then(|_| pending.frames.pop());
    if !crate::effect::suspend_miracle_draw(state, pending, drawer, path, rest.unwrap_or(0)) {
        pending.frames.extend(staged);
    }
}

/// Mills the card's dredge number, then returns it from the graveyard to
/// its owner's hand. `false` (and nothing happens) if it can no longer
/// dredge. `resolving` is the spell or ability whose resolution draws, held
/// by the interpreter outside the public stack; the draw step has none.
fn dredge(
    state: &mut GameState,
    player: PlayerId,
    card: ObjectId,
    link: crate::state::ObjectLinkV4,
    resolving: Option<&crate::state::StackItem>,
) -> bool {
    let Some(count) = definition(state, card).and_then(|def| dredge_count(def.name)) else {
        return false;
    };
    let library = state.players[player.index()].library.clone();
    if !link_live_in(state, link, Zone::Graveyard)
        || state.objects.get(card).owner != player
        || library.len() < usize::from(count)
    {
        return false;
    }
    let captures = if state.engine.event_log.is_empty() {
        Vec::new()
    } else {
        let captures = match resolving {
            Some(resolving) => {
                crate::trigger::capture_triggers_without_sba_owning(state, resolving)
            }
            None => crate::trigger::capture_triggers_without_sba(state),
        };
        if state.engine.halted.is_some() {
            return false;
        }
        captures
    };
    // Defer mid-resolution captures until the resolution checkpoint so all
    // simultaneous triggers order together under APNAP (CR 603.2, 603.3b).
    defer_captures(state, captures);
    let milled = library
        .into_iter()
        .take(usize::from(count))
        .map(|object| ProposedEvent::zone_change(object, Zone::Graveyard))
        .collect::<Vec<_>>();
    event::propose_and_commit_batch(state, milled);
    if link_live_in(state, link, Zone::Graveyard) {
        event::propose_and_commit(
            state,
            ProposedEvent::zone_change_preserving_known_identity(card, Zone::Hand),
        );
    }
    true
}

// ---- The draw step ------------------------------------------------------------------

enum DrawStepChoiceV1 {
    Dredge(Vec<ObjectId>),
    JaceOrHullbreacher(ObjectId),
}

fn draw_step_choice(state: &GameState, player: PlayerId) -> Option<DrawStepChoiceV1> {
    if crate::eternal_cube_c6c_v1::prevents_draw(state, player) {
        return None;
    }
    let cards = dredge_cards(state, player);
    if !cards.is_empty() {
        return Some(DrawStepChoiceV1::Dredge(cards));
    }
    if !state.players[player.index()].library.is_empty()
        || crate::eternal_sideboard_s1c_v1::statics::hullbreacher_replaces_draw(state, player)
            .is_none()
    {
        return None;
    }
    let jace = crate::eternal_cards_v1::e3::jace_draw_replacement_source(state, player)?;
    Some(DrawStepChoiceV1::JaceOrHullbreacher(jace))
}

fn drive_draw_step_draws(state: &mut GameState, player: PlayerId, mut remaining: u32) {
    while remaining != 0
        && state.engine.halted.is_none()
        && !owned_draws_v1::draw_won_game(state, player)
    {
        if draw_step_choice(state, player).is_some() {
            let e7b = e7b_state(state);
            e7b.dredge_draw = Some(player);
            e7b.draw_step_remaining =
                super::OptionalRemainingV1((remaining > 1).then_some(remaining));
            return;
        }
        event::propose_draw_attempt(state, player);
        remaining -= 1;
        if wait_for_miracle(state, remaining) {
            return;
        }
    }
}

/// A miracle card drawn as its owner's first draw this turn is revealed as
/// it is drawn (702.94a), so the rest of a multi-card turn-based draw (Quantum
/// Riddler's) waits for that reveal. The wait is the remaining count without
/// a replacement choice (`dredge_draw` is `None`).
fn wait_for_miracle(state: &mut GameState, remaining: u32) -> bool {
    if remaining == 0 || crate::eternal_legacy_e5b_v1::uncollected_miracle_draw(state).is_none() {
        return false;
    }
    e7b_state(state).draw_step_remaining = super::OptionalRemainingV1(Some(remaining));
    true
}

/// The rest of a turn-based draw waiting for a miracle reveal, if any.
fn miracle_draw_step_wait(state: &GameState) -> Option<u32> {
    read_state(state)
        .filter(|e7b| e7b.dredge_draw.is_none())
        .and_then(|e7b| e7b.draw_step_remaining.0)
}

/// Authenticates a turn-based draw waiting for a miracle reveal: only during
/// the active player's draw step, with nothing resolving, while that
/// player's first draw this turn is a miracle card whose reveal is pending,
/// and no more draws remain than its draw-count replacements added for the
/// hand it had before the draw (121.2b). Each attempt made so far put at
/// most one card into that hand (a draw, or a dredged card's return), so
/// some count of attempts made, the miracle's among them, accounts for
/// the rest: the hand before the draw held at least the current hand less
/// those attempts, and a larger hand adds no more draws.
pub(crate) fn validate_miracle_draw_step_wait(state: &GameState) -> Result<(), String> {
    let Some(remaining) = miracle_draw_step_wait(state) else {
        return Ok(());
    };
    let player = state.active_player;
    let hand = state.players[player.index()].hand.len();
    let accounted = (1..=hand.max(1)).any(|made| {
        let extra = event::draw_instruction_extra_at_hand_size(
            state,
            player,
            1,
            hand.saturating_sub(made),
        );
        u64::from(remaining) + u64::try_from(made).unwrap_or(u64::MAX) <= 1 + u64::from(extra)
    });
    let revealing = crate::eternal_legacy_e5b_v1::pending_decision(state).is_some_and(|decision| {
        matches!(decision, crate::engine::Decision::ChooseEffectOption { player: owner, source, .. }
            if owner == player
                && crate::eternal_legacy_e5b_v1::first_miracle_draw_at(state, player)
                    .is_some_and(|(_, card)| card == source))
    });
    if state.step != crate::state::Step::Draw
        || remaining == 0
        || !accounted
        || !state.stack.is_empty()
        || state.engine.pending_effect.is_some()
        || !revealing
    {
        return Err("the draw step's remaining draws lost their miracle reveal".into());
    }
    Ok(())
}

/// Continues a turn-based draw once its miracle reveal is answered.
pub(crate) fn resume_draw_step_after_miracle(state: &mut GameState) {
    let Some(remaining) = miracle_draw_step_wait(state) else {
        return;
    };
    if crate::eternal_legacy_e5b_v1::pending_decision(state).is_some() {
        return;
    }
    e7b_state(state).draw_step_remaining = super::OptionalRemainingV1(None);
    let player = state.active_player;
    drive_draw_step_draws(state, player, remaining);
}

/// The draw step's draw: whether `player` has (or will reach across extra
/// draws such as Quantum Riddler's) a dredge or competing draw-replacement
/// choice, so the draw runs through the resumable turn-draw driver.
pub(crate) fn defer_draw_step_draw(state: &mut GameState, player: PlayerId) -> bool {
    let total = 1 + event::draw_instruction_extra(state, player, 1);
    if !(owned_draw_needs_choice(state, player, 1)
        || total > 1 && crate::eternal_legacy_e5b_v1::next_draw_may_offer_miracle(state, player))
    {
        return false;
    }
    drive_draw_step_draws(state, player, total);
    true
}

/// Whether the draw step's draw is waiting for a replacement choice or a
/// miracle reveal.
pub(crate) fn draw_step_pending(state: &GameState) -> bool {
    read_state(state)
        .is_some_and(|e7b| e7b.dredge_draw.is_some() || e7b.draw_step_remaining.0.is_some())
}

/// The pending draw-step replacement choice (dredge or Jace vs Hullbreacher),
/// if any.
pub(crate) fn pending_decision(state: &GameState) -> Option<crate::engine::Decision> {
    let player = read_state(state)?.dredge_draw?;
    match draw_step_choice(state, player)? {
        DrawStepChoiceV1::Dredge(cards) => Some(crate::engine::Decision::ChooseEffectOption {
            player,
            source: cards.first().copied()?,
            option_count: u16::try_from(cards.len() + 1).ok()?,
        }),
        DrawStepChoiceV1::JaceOrHullbreacher(jace) => {
            Some(crate::engine::Decision::ChooseEffectOption {
                player,
                source: jace,
                option_count: 2,
            })
        }
    }
}

/// Answers the pending draw-step replacement choice. `Ok(false)` when none is
/// pending.
pub(crate) fn answer_pending_decision(state: &mut GameState, option: u16) -> Result<bool, String> {
    let Some(player) = read_state(state).and_then(|e7b| e7b.dredge_draw) else {
        return Ok(false);
    };
    let Some(choice) = draw_step_choice(state, player) else {
        return Err("the draw-step replacement choice is no longer available".into());
    };
    let remaining = read_state(state)
        .and_then(|e7b| e7b.draw_step_remaining.0)
        .unwrap_or(1);
    match choice {
        DrawStepChoiceV1::Dredge(cards) => {
            let card = match usize::from(option) {
                0 => None,
                index => Some(
                    *cards
                        .get(index - 1)
                        .ok_or("the dredge option is out of range")?,
                ),
            };
            let e7b = e7b_state(state);
            e7b.dredge_draw = None;
            e7b.draw_step_remaining = super::OptionalRemainingV1(None);
            let dredged = card.is_some_and(|card| {
                let link = super::live_link(state, card);
                dredge(state, player, card, link, None)
            });
            if !dredged && state.engine.halted.is_none() {
                event::propose_draw_attempt(state, player);
            }
        }
        DrawStepChoiceV1::JaceOrHullbreacher(_) => {
            if option > 1 {
                return Err("the draw replacement option is out of range".into());
            }
            let e7b = e7b_state(state);
            e7b.dredge_draw = None;
            e7b.draw_step_remaining = super::OptionalRemainingV1(None);
            if option == 1 {
                event::commit(state, ProposedEvent::draw(player));
            } else {
                event::propose_draw_attempt(state, player);
            }
        }
    }
    if remaining > 1 && !wait_for_miracle(state, remaining - 1) {
        drive_draw_step_draws(state, player, remaining - 1);
    }
    Ok(true)
}
