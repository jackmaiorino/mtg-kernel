//! One replacement choice per draw in selected definition-owned Eternal effects.

use super::EternalOpV1;
use crate::card_def::CARD_DEFS;
use crate::effect::{
    EffectContinuation, EffectFrame, EffectOp, EffectOptionChoicePurpose, ExecCtx,
    PendingEffectChoice, PlayerRef,
};
use crate::eternal_e5a_v1::E5aOpV1;
use crate::eternal_legacy_e7a_v1::{self, E7aOpV1};
use crate::eternal_legacy_e8a_v1::E8aOpV1;
use crate::eternal_lurrus_v1::LurrusOpV1;
use crate::eternal_pauper_e8b_v1::{self, E8bOpV1};
use crate::eternal_vintage_e7b_v1::{dredge, E7bOpV1};
use crate::event::CommittedEvent;
use crate::event_history_v1::EventKindV1;
use crate::ids::{ObjectId, PlayerId, StackItemId};
use crate::standard_cards_v1::StandardOpV1;
use crate::state::{
    AbilitySourceContractV4, GameState, ObjectLinkV4, StackItemKind, StackSourceContractV4, Zone,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum InstructionV1 {
    Frog,
    Plunder { normal: u8, from_graveyard: u8 },
    Tamiyo,
    TwinnedVision,
    EndstoneLand,
    KishlaSkimmer,
    ChandraEmblem,
    DiviningTop,
    OneRing,
    VeilOfSummer,
    DragonEngine,
}

impl InstructionV1 {
    fn from_op(op: &EternalOpV1) -> Option<Self> {
        match op {
            EternalOpV1::LurrusV1(LurrusOpV1::FrogDrawV1) => Some(Self::Frog),
            EternalOpV1::DrawCastFromGraveyardV1 {
                normal,
                from_graveyard,
            } => Some(Self::Plunder {
                normal: *normal,
                from_graveyard: *from_graveyard,
            }),
            EternalOpV1::DrawHalfLibraryThenNoMaximumHandSizeV1 => Some(Self::Tamiyo),
            EternalOpV1::E8bV1(E8bOpV1::TwinnedVisionV1) => Some(Self::TwinnedVision),
            EternalOpV1::E5aV1(E5aOpV1::EndstoneDrawV1) => Some(Self::EndstoneLand),
            EternalOpV1::E7bV1(E7bOpV1::DrawOneV1) => Some(Self::KishlaSkimmer),
            EternalOpV1::E8aV1(E8aOpV1::DrawOneV1) => Some(Self::ChandraEmblem),
            EternalOpV1::E5aV1(E5aOpV1::DrawThenSourceOnTopV1) => Some(Self::DiviningTop),
            EternalOpV1::BurdenCounterThenDrawV1 => Some(Self::OneRing),
            EternalOpV1::VeilOfSummerV1 => Some(Self::VeilOfSummer),
            EternalOpV1::E7aV1(E7aOpV1::DiscardHandThenDrawV1 { count: 3 }) => {
                Some(Self::DragonEngine)
            }
            _ => None,
        }
    }

    fn from_program(op: &EffectOp) -> Option<Self> {
        let EffectOp::StandardV1(StandardOpV1::EternalV1(op)) = op else {
            return None;
        };
        Self::from_op(op)
    }

    fn count(self, state: &GameState, ctx: &ExecCtx) -> u32 {
        match self {
            Self::Frog
            | Self::EndstoneLand
            | Self::KishlaSkimmer
            | Self::ChandraEmblem
            | Self::DiviningTop => 1,
            Self::Plunder {
                normal,
                from_graveyard,
            } => graveyard_cast_count(state, ctx.source, normal, from_graveyard),
            Self::Tamiyo => half_library_count(state.players[ctx.controller.index()].library.len()),
            Self::TwinnedVision => {
                eternal_pauper_e8b_v1::twinned_vision_draw_count(state, ctx.source)
            }
            Self::OneRing => ctx
                .ability_source_contract
                .map_or(0, |source| super::e3::quote_ring_draw_count(state, source)),
            Self::VeilOfSummer => u32::from(
                super::e3::opponent_cast_blue_or_black_spell_this_turn(state, ctx.controller),
            ),
            Self::DragonEngine => 3,
        }
    }

    fn program_path(self) -> &'static [u16] {
        match self {
            Self::DragonEngine => &[0],
            _ => &[],
        }
    }

    fn draw_path(self) -> &'static [u16] {
        match self {
            Self::DragonEngine => &[0, 0],
            _ => &[0],
        }
    }

    fn completion_path(self) -> &'static [u16] {
        match self {
            Self::DragonEngine => &[0, 1],
            _ => &[1],
        }
    }
}

pub(crate) fn recognizes(op: &EternalOpV1) -> bool {
    InstructionV1::from_op(op).is_some()
}

pub(crate) fn starting_count(state: &GameState, program: &EffectOp, ctx: &ExecCtx) -> Option<u32> {
    Some(InstructionV1::from_program(program)?.count(state, ctx))
}

pub(crate) fn graveyard_cast_count(
    state: &GameState,
    source: ObjectId,
    normal: u8,
    from_graveyard: u8,
) -> u32 {
    let from_graveyard_cast = state.objects.try_get(source).is_some_and(|source| {
        source.zone == Zone::Stack
            && source
                .v4
                .spell_cast_origin
                .is_some_and(|origin| origin.origin_zone == Zone::Graveyard)
    });
    u32::from(if from_graveyard_cast {
        from_graveyard
    } else {
        normal
    })
}

pub(crate) fn half_library_count(library_len: usize) -> u32 {
    u32::try_from(library_len.div_ceil(2)).unwrap_or(u32::MAX)
}

fn draw_won_game(state: &GameState, player: PlayerId) -> bool {
    let opponent = player.opponent();
    state.players[opponent.index()].has_lost
        && !crate::game_outcome_protection_v1::cannot_lose(state, opponent)
}

/// Draws with no pending replacement choice. A Jace win ends the game
/// immediately, before another attempt or a following instruction.
pub(crate) fn draw_uninterrupted(state: &mut GameState, player: PlayerId, count: u32) -> bool {
    for _ in 0..count {
        crate::event::propose_and_commit(state, crate::event::ProposedEvent::draw(player));
        if draw_won_game(state, player) || state.engine.halted.is_some() {
            return false;
        }
    }
    true
}

pub(crate) fn remove_maximum_hand_size(state: &mut GameState, player: PlayerId) {
    state
        .eternal_v1
        .get_or_insert_with(Default::default)
        .no_maximum_hand_size[player.index()] = true;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum SourceV1 {
    Spell(StackSourceContractV4),
    Ability(AbilitySourceContractV4),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProgressV1 {
    started_at: usize,
    completed: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartedV1 {
    item: StackItemId,
    source: SourceV1,
    controller: PlayerId,
    instruction: InstructionV1,
    total: u32,
    initial_library_len: Option<usize>,
}

/// Native resolution evidence. A completed attempt can produce no Draw or
/// Treasure event because replacement can prevent both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum JournalV1 {
    Started(StartedV1),
    Completed { started_at: usize, ordinal: u32 },
}

fn record(state: &mut GameState, journal: JournalV1) {
    state
        .engine
        .event_history
        .push(CommittedEvent::EternalOwnedDrawV1 {
            journal: Box::new(journal),
        });
}

fn has_started(state: &GameState, item: StackItemId) -> bool {
    state
        .engine
        .event_history
        .scan_for_v1(&[EventKindV1::EternalOwnedDrawV1])
        .iter()
        .any(|event| {
            matches!(event, CommittedEvent::EternalOwnedDrawV1 { journal }
            if matches!(**journal, JournalV1::Started(started) if started.item == item))
        })
}

fn selected(state: &GameState, pending: &EffectContinuation) -> Option<(InstructionV1, SourceV1)> {
    let item = &pending.resolving_item;
    if let Some(inline) = &item.inline_effect {
        let instruction = match inline {
            EffectOp::Choice { .. } => InstructionV1::DragonEngine,
            _ => InstructionV1::from_program(inline)?,
        };
        let source = item.v4.ability_source_contract?;
        if source.source != item.source
            || source.controller != item.controller
            || source.zone != Zone::Battlefield
        {
            return None;
        }
        let name = CARD_DEFS.get(usize::from(source.card_def))?.object_name;
        let authorized = match (instruction, item.kind, name) {
            (InstructionV1::Frog, StackItemKind::TriggeredAbility, "Psychic Frog")
            | (InstructionV1::EndstoneLand, StackItemKind::TriggeredAbility, "The Endstone") => {
                true
            }
            (
                InstructionV1::Tamiyo,
                StackItemKind::ActivatedAbility,
                "Tamiyo, Inquisitive Student",
            ) => item.v4.activated_ability_index == Some(2),
            (
                InstructionV1::DiviningTop,
                StackItemKind::ActivatedAbility,
                "Sensei's Divining Top",
            ) => item.v4.activated_ability_index == Some(1),
            (InstructionV1::OneRing, StackItemKind::ActivatedAbility, "The One Ring") => {
                item.v4.activated_ability_index == Some(0)
            }
            (
                InstructionV1::DragonEngine,
                StackItemKind::TriggeredAbility,
                "Phyrexian Dragon Engine",
            ) => {
                source.owner == source.controller
                    && source.zone_change_count > 0
                    && *inline == eternal_legacy_e7a_v1::dragon_engine_rummage()
            }
            (InstructionV1::KishlaSkimmer, StackItemKind::TriggeredAbility, "Kishla Skimmer") => {
                let source_link = ObjectLinkV4 {
                    object: source.source,
                    zone_change_count: source.zone_change_count,
                };
                state.eternal_v1.as_ref().is_some_and(|eternal| {
                    eternal.e7b.skimmer_uses.iter().any(|(used, turn)| {
                        *used == source_link
                            && turn.player == source.controller
                            && turn.turn == state.turn_key_v1()
                            && turn.active == state.active_player
                    })
                })
            }
            (
                InstructionV1::ChandraEmblem,
                StackItemKind::TriggeredAbility,
                "Chandra, Chill of Compliance",
            ) => state.eternal_v1.as_ref().is_some_and(|eternal| {
                eternal
                    .e8a
                    .chandra_emblems
                    .contains(&(item.controller, source))
            }),
            _ => false,
        };
        return authorized.then_some((instruction, SourceV1::Ability(source)));
    }
    if item.kind != StackItemKind::Spell {
        return None;
    }
    let source = item.v4.source_contract?;
    let def = CARD_DEFS.get(usize::from(source.card_def))?;
    if !matches!(
        def.object_name,
        "Plunder the Trollshaws" | "Twinned Vision" | "Veil of Summer"
    ) {
        return None;
    }
    state.objects.try_get(item.source)?;
    if source.zone != Zone::Stack
        || StackSourceContractV4::capture(state, item.source, source.cast_method) != source
    {
        return None;
    }
    let instruction = InstructionV1::from_program(&def.printed_mode_effect(item.mode_chosen)?)?;
    match (instruction, def.object_name) {
        (InstructionV1::Plunder { .. }, "Plunder the Trollshaws")
        | (InstructionV1::TwinnedVision, "Twinned Vision")
        | (InstructionV1::VeilOfSummer, "Veil of Summer") => {
            Some((instruction, SourceV1::Spell(source)))
        }
        _ => None,
    }
}

fn completion(instruction: InstructionV1, ordinal: u32) -> EffectFrame {
    EffectFrame::Program {
        op: super::eternal(EternalOpV1::OwnedDrawCompletedV1 { ordinal }),
        path: instruction.completion_path().to_vec(),
    }
}

fn stage(pending: &mut EffectContinuation, instruction: InstructionV1, ordinal: u32) {
    pending.frames.push(completion(instruction, ordinal));
    pending.frames.push(EffectFrame::Program {
        op: EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        },
        path: instruction.draw_path().to_vec(),
    });
}

pub(crate) fn begin(
    state: &mut GameState,
    pending: &mut EffectContinuation,
    op: &EternalOpV1,
    path: &[u16],
) -> Result<(), String> {
    let (instruction, source) =
        selected(state, pending).ok_or("Eternal draw lost its original source")?;
    if InstructionV1::from_op(op) != Some(instruction)
        || path != instruction.program_path()
        || !pending.frames.is_empty()
        || state.eternal_owned_draw_progress_v1.is_some()
        || has_started(state, pending.resolving_item.v4.stack_item_id)
    {
        return Err("Eternal draw lost its original instruction".into());
    }
    if instruction == InstructionV1::DragonEngine {
        let player = pending.ctx.controller;
        eternal_legacy_e7a_v1::discard_controller_hand(state, player);
        if state.engine.halted.is_some() {
            return Err("Dragon Engine could not complete its discard".into());
        }
        let count = instruction.count(state, &pending.ctx);
        if !dredge::owned_draw_needs_choice(state, player, count) {
            draw_uninterrupted(state, player, count);
            return Ok(());
        }
    }
    let total = match (instruction, source) {
        (InstructionV1::OneRing, SourceV1::Ability(source)) => {
            super::e3::begin_ring_draws(state, source)
        }
        _ => instruction.count(state, &pending.ctx),
    };
    if total == 0 {
        finish_instruction(state, pending, instruction);
        return Ok(());
    }
    let started_at = state.engine.event_history.len();
    let started = StartedV1 {
        item: pending.resolving_item.v4.stack_item_id,
        source,
        controller: pending.ctx.controller,
        instruction,
        total,
        initial_library_len: (instruction == InstructionV1::Tamiyo)
            .then_some(state.players[pending.ctx.controller.index()].library.len()),
    };
    record(state, JournalV1::Started(started));
    state.eternal_owned_draw_progress_v1 = Some(ProgressV1 {
        started_at,
        completed: 0,
    });
    stage(pending, instruction, 0);
    Ok(())
}

fn progress(state: &GameState, pending: &EffectContinuation) -> Option<(ProgressV1, StartedV1)> {
    let progress = state.eternal_owned_draw_progress_v1?;
    let CommittedEvent::EternalOwnedDrawV1 { journal } =
        state.engine.event_history.get(progress.started_at)?
    else {
        return None;
    };
    let JournalV1::Started(started) = **journal else {
        return None;
    };
    (Some((started.instruction, started.source)) == selected(state, pending)
        && started.item == pending.resolving_item.v4.stack_item_id
        && started.controller == pending.ctx.controller
        && progress.completed < started.total)
        .then_some((progress, started))
}

pub(crate) fn complete(
    state: &mut GameState,
    pending: &mut EffectContinuation,
    ordinal: u32,
    path: &[u16],
) -> Result<(), String> {
    let (progress, started) =
        progress(state, pending).ok_or("Eternal draw lost its native start")?;
    if ordinal != progress.completed
        || path != started.instruction.completion_path()
        || !pending.frames.is_empty()
    {
        return Err("Eternal draw completed the wrong attempt".into());
    }
    let completed = ordinal + 1;
    record(
        state,
        JournalV1::Completed {
            started_at: progress.started_at,
            ordinal,
        },
    );
    let terminal = draw_won_game(state, started.controller);
    if terminal || completed == started.total {
        state.eternal_owned_draw_progress_v1 = None;
        if !terminal {
            finish_instruction(state, pending, started.instruction);
        }
    } else {
        state
            .eternal_owned_draw_progress_v1
            .as_mut()
            .unwrap()
            .completed = completed;
        stage(pending, started.instruction, completed);
    }
    Ok(())
}

fn finish_instruction(
    state: &mut GameState,
    pending: &EffectContinuation,
    instruction: InstructionV1,
) {
    match instruction {
        InstructionV1::Tamiyo => remove_maximum_hand_size(state, pending.ctx.controller),
        InstructionV1::DiviningTop => {
            crate::eternal_e5a_v1::put_draw_source_on_top(&pending.ctx, state);
        }
        InstructionV1::VeilOfSummer => {
            super::e3::apply_veil_protection(state, pending.ctx.controller)
        }
        _ => {}
    }
}

fn initial_library_len(state: &GameState, player: PlayerId, start: usize) -> Option<usize> {
    let current = state.players[player.index()].library.len();
    state
        .engine
        .event_history
        .get(start..)?
        .iter()
        .rev()
        .try_fold(current, |len, event| match event {
            CommittedEvent::Draw {
                player: drawer,
                object: Some(_),
            } if *drawer == player => len.checked_add(1),
            CommittedEvent::ZoneChange {
                object, from, to, ..
            } if from != to
                && (*from == Zone::Library || *to == Zone::Library)
                && state
                    .objects
                    .try_get(*object)
                    .is_some_and(|card| card.owner == player) =>
            {
                if *from == Zone::Library {
                    len.checked_add(1)
                } else {
                    len.checked_sub(1)
                }
            }
            _ => Some(len),
        })
}

fn validate_native_progress(
    state: &GameState,
    ctx: &ExecCtx,
    progress: ProgressV1,
    started: StartedV1,
) -> Result<(), String> {
    let tail = progress.started_at + 1;
    let total = match (started.instruction, started.initial_library_len) {
        (InstructionV1::Tamiyo, Some(library_len))
            if initial_library_len(state, started.controller, tail) == Some(library_len) =>
        {
            Some(half_library_count(library_len))
        }
        (InstructionV1::Tamiyo, _) | (_, Some(_)) => None,
        (InstructionV1::OneRing, None) => match started.source {
            SourceV1::Ability(source) => Some(super::e3::ring_burden_count(state, source)),
            SourceV1::Spell(_) => None,
        },
        (instruction, None) => Some(instruction.count(state, ctx)),
    };
    if total != Some(started.total) {
        return Err("Eternal draw changed its original count".into());
    }
    let completed = state.engine.event_history[tail..]
        .iter()
        .filter_map(|event| match event {
            CommittedEvent::EternalOwnedDrawV1 { journal } => match **journal {
                JournalV1::Completed {
                    started_at,
                    ordinal,
                } if started_at == progress.started_at => Some(ordinal),
                _ => None,
            },
            _ => None,
        })
        .try_fold(0u32, |count, ordinal| {
            if count == ordinal {
                count.checked_add(1)
            } else {
                None
            }
        });
    if completed != Some(progress.completed) {
        return Err("Eternal draw changed its native completed attempts".into());
    }
    Ok(())
}

pub(crate) fn draw_stage_matches(
    state: &GameState,
    pending: &EffectContinuation,
    drawer: PlayerId,
    path: &[u16],
    remaining: &[EffectFrame],
) -> bool {
    progress(state, pending).is_some_and(|(progress, started)| {
        drawer == started.controller
            && path == started.instruction.draw_path()
            && remaining == [completion(started.instruction, progress.completed)]
    })
}

fn dragon_before_start(pending: &EffectContinuation) -> bool {
    if pending.answered_choice_guard.is_some() {
        return false;
    }
    let root = eternal_legacy_e7a_v1::dragon_engine_rummage();
    let EffectOp::Choice {
        options: expected, ..
    } = &root
    else {
        unreachable!("Dragon Engine has one printed choice");
    };
    match (&pending.choice, pending.frames.as_slice()) {
        (None, [EffectFrame::Program { op, path }]) => match path.as_slice() {
            [] => op == &root,
            [index] => expected.get(usize::from(*index)) == Some(op),
            _ => false,
        },
        (
            Some(PendingEffectChoice::ChooseOption {
                player,
                path,
                options,
                purpose: EffectOptionChoicePurpose::Generic,
            }),
            [],
        ) => *player == pending.ctx.controller && path.is_empty() && options == expected,
        _ => false,
    }
}

pub(crate) fn validate(state: &GameState, pending: &EffectContinuation) -> Result<(), String> {
    let Some((instruction, _)) = selected(state, pending) else {
        return if state.eternal_owned_draw_progress_v1.is_none() {
            Ok(())
        } else {
            Err("Eternal draw progress lost its original source".into())
        };
    };
    let before_start = if instruction == InstructionV1::DragonEngine {
        dragon_before_start(pending)
    } else {
        matches!((&pending.choice, pending.frames.as_slice()),
            (None, [EffectFrame::Program { op, path }]) if InstructionV1::from_program(op) == Some(instruction) && path.is_empty())
    };
    if state.eternal_owned_draw_progress_v1.is_none() && before_start {
        return if has_started(state, pending.resolving_item.v4.stack_item_id) {
            Err("Eternal draw already started its original instruction".into())
        } else {
            Ok(())
        };
    }
    let (progress, started) =
        progress(state, pending).ok_or("Eternal draw lost its native start")?;
    validate_native_progress(state, &pending.ctx, progress, started)?;
    let (path, remaining) =
        dredge::draw_stage(pending).ok_or("Eternal draw lost its original stage")?;
    if !draw_stage_matches(state, pending, started.controller, path, remaining) {
        return Err("Eternal draw changed its progress or remaining effects".into());
    }
    dredge::validate_draw_choice(
        state,
        pending,
        started.controller,
        path,
        remaining,
        "Eternal draw",
    )
}
