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
    PendingEffectChoice, PlayerRef,
};
use crate::eternal_cards_v1::{
    delayed_upkeep_draw,
    e3::{E3ChoiceKindV1, E3ChoiceV1},
    EternalOpV1,
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
        _ => false,
    }
}

/// Whether a synchronous program should run in the resumable interpreter
/// so the affected player can choose a draw replacement.
pub(crate) fn forces_resumable(state: &GameState, program: &EffectOp) -> bool {
    cfg!(feature = "eternal-fixtures")
        && draws(program)
        && [PlayerId::P0, PlayerId::P1]
            .into_iter()
            .any(|player| may_need_draw_choice(state, player))
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
            event::propose_and_commit(state, ProposedEvent::draw(drawer));
            count -= 1;
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

fn replacement_options(
    state: &GameState,
    pending: &EffectContinuation,
    drawer: PlayerId,
    path: &[u16],
    remaining: &[EffectFrame],
) -> Option<(E7bChoiceV1, Vec<ObjectId>)> {
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
    let (kind, cards) = replacement_options(state, pending, drawer, &[], &[])
        .ok_or_else(|| format!("{owner} lost its replacement choices"))?;
    let expected = choice(state, drawer, kind, &cards, Vec::new(), Vec::new());
    let valid = match (&pending.choice, pending.frames.as_slice()) {
        (
            Some(PendingEffectChoice::SelectTargets {
                purpose: EffectTargetSelectionPurpose::EternalV1 { choice },
                selected,
                legal,
                min_targets: 0,
                ..
            }),
            [],
        ) => {
            **choice == expected
                && selected.is_empty()
                && legal
                    .iter()
                    .map(|candidate| candidate.target)
                    .eq(expected.candidates)
        }
        (None, [EffectFrame::EternalSelectionV1 { choice, selected }]) => {
            **choice == expected
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
        {
            pending.frames.clear();
        }
    } else {
        event::propose_and_commit(state, ProposedEvent::draw(player));
    }
    Ok(())
}

/// Applies a dredge choice: no card draws, a card dredges.
pub(crate) fn finish(state: &mut GameState, player: PlayerId, chosen: Option<EffectObjectBinding>) {
    let Some(card) = chosen else {
        event::propose_and_commit(state, ProposedEvent::draw(player));
        return;
    };
    if !dredge(state, player, card.object, binding_link(&card)) {
        event::propose_and_commit(state, ProposedEvent::draw(player));
    }
}

/// Mills the card's dredge number, then returns it from the graveyard to
/// its owner's hand. `false` (and nothing happens) if it can no longer
/// dredge.
fn dredge(
    state: &mut GameState,
    player: PlayerId,
    card: ObjectId,
    link: crate::state::ObjectLinkV4,
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

/// The draw step's draw: whether `player` may dredge instead, so the draw
/// waits for their choice (option 0 draws, option N dredges their Nth
/// dredge card).
pub(crate) fn defer_draw_step_draw(state: &mut GameState, player: PlayerId) -> bool {
    if dredge_cards(state, player).is_empty() {
        return false;
    }
    e7b_state(state).dredge_draw = Some(player);
    true
}

/// The pending draw-step dredge choice, if any.
pub(crate) fn pending_decision(state: &GameState) -> Option<crate::engine::Decision> {
    let player = read_state(state)?.dredge_draw?;
    let cards = dredge_cards(state, player);
    Some(crate::engine::Decision::ChooseEffectOption {
        player,
        source: cards.first().copied()?,
        option_count: u16::try_from(cards.len() + 1).ok()?,
    })
}

/// Answers the pending draw-step dredge choice. `Ok(false)` when none is
/// pending.
pub(crate) fn answer_pending_decision(state: &mut GameState, option: u16) -> Result<bool, String> {
    let Some(player) = read_state(state).and_then(|e7b| e7b.dredge_draw) else {
        return Ok(false);
    };
    let cards = dredge_cards(state, player);
    e7b_state(state).dredge_draw = None;
    let dredged = match usize::from(option) {
        0 => false,
        index => {
            let card = *cards
                .get(index - 1)
                .ok_or("the dredge option is out of range")?;
            let link = super::live_link(state, card);
            dredge(state, player, card, link)
        }
    };
    if !dredged {
        event::propose_and_commit(state, ProposedEvent::draw(player));
    }
    Ok(true)
}
