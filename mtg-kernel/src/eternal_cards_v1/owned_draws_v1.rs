//! One replacement choice per draw in selected definition-owned Eternal effects.

use super::EternalOpV1;
use crate::card_def::{TargetSpec, CARD_DEFS};
use crate::effect::{
    EffectContinuation, EffectFrame, EffectOp, EffectOptionChoicePurpose, ExecCtx,
    PendingEffectChoice, PlayerRef,
};
use crate::eternal_e5a_v1::E5aOpV1;
use crate::eternal_legacy_e7a_v1::{self, E7aOpV1};
use crate::eternal_legacy_e8a_v1::E8aOpV1;
use crate::eternal_lurrus_v1::{self, LurrusOpV1};
use crate::eternal_pauper_e8b_v1::{self, E8bOpV1};
use crate::eternal_vintage_e7b_v1::{dredge, minsc, E7bOpV1};
use crate::event::CommittedEvent;
use crate::event_history_v1::EventKindV1;
use crate::ids::{ObjectId, PlayerId, StackItemId};
use crate::standard_cards_v1::StandardOpV1;
use crate::state::{
    AbilitySourceContractV4, GameState, ObjectLinkV4, StackItemKind, StackSourceContractV4,
    StackTargetContractV4, Target, Zone,
};
use serde::{Deserialize, Serialize};

pub(crate) mod fixed;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum InstructionV1 {
    Frog,
    Plunder {
        normal: u8,
        from_graveyard: u8,
    },
    Tamiyo,
    TwinnedVision,
    EndstoneLand,
    KishlaSkimmer,
    ChandraEmblem,
    DiviningTop,
    OneRing,
    VeilOfSummer,
    DragonEngine,
    FlameOfAnor {
        draw: u8,
        artifact: Option<u8>,
        creature: Option<u8>,
        spec: TargetSpec,
    },
    Curie {
        combat_at: usize,
    },
    FixedRoot {
        player: PlayerRef,
        drawer: PlayerId,
        count: u32,
    },
    Minsc {
        sacrifice: usize,
    },
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
            EternalOpV1::LurrusV1(LurrusOpV1::FlameOfAnorV1 {
                draw: Some(draw),
                artifact,
                creature,
                spec,
            }) => Some(Self::FlameOfAnor {
                draw: *draw,
                artifact: *artifact,
                creature: *creature,
                spec: *spec,
            }),
            EternalOpV1::E7aV1(E7aOpV1::CurieDrawBoundV1 { combat_at }) => Some(Self::Curie {
                combat_at: *combat_at,
            }),
            EternalOpV1::E7bV1(E7bOpV1::MinscDamageBoundV1 { sacrifice }) => Some(Self::Minsc {
                sacrifice: *sacrifice,
            }),
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
            Self::FixedRoot { count, .. } => count,
            Self::Minsc { sacrifice } => minsc::count(state, ctx, sacrifice).unwrap_or(0),
            Self::Curie { combat_at } => {
                eternal_legacy_e7a_v1::curie::draw_count(state, ctx, combat_at).unwrap_or(0)
            }
            Self::FlameOfAnor { draw, spec, .. } => {
                if eternal_lurrus_v1::flame_draw_target(state, ctx, draw, spec).is_some() {
                    2
                } else {
                    0
                }
            }
        }
    }

    fn draw_player(self) -> PlayerRef {
        match self {
            Self::FlameOfAnor { draw, .. } => PlayerRef::Target(draw),
            Self::FixedRoot { player, .. } => player,
            _ => PlayerRef::Controller,
        }
    }

    fn drawer(self, ctx: &ExecCtx) -> Option<PlayerId> {
        match self {
            Self::FlameOfAnor { draw, .. } => match ctx.targets.get(usize::from(draw))? {
                Target::Player(player) => Some(*player),
                _ => None,
            },
            Self::FixedRoot { drawer, .. } => Some(drawer),
            _ => Some(ctx.controller),
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

pub(crate) fn starting_draws(
    state: &GameState,
    program: &EffectOp,
    ctx: &ExecCtx,
) -> Option<(PlayerId, u32)> {
    let instruction = InstructionV1::from_program(program)?;
    Some((instruction.drawer(ctx)?, instruction.count(state, ctx)))
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

pub(crate) fn draw_won_game(state: &GameState, player: PlayerId) -> bool {
    let opponent = player.opponent();
    state.players[opponent.index()].has_lost
        && !crate::game_outcome_protection_v1::cannot_lose(state, opponent)
}

/// Draws with no pending replacement choice. A Jace win ends the game
/// immediately, before another attempt or a following instruction.
pub(crate) fn draw_uninterrupted(state: &mut GameState, player: PlayerId, count: u32) -> bool {
    crate::event::draw_instruction(state, player, count)
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
    FlameOfAnor {
        spell: StackSourceContractV4,
        drawer: PlayerId,
        other_target: Option<StackTargetContractV4>,
    },
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
    Completed {
        started_at: usize,
        ordinal: u32,
    },
    CurieCombat {
        combat_at: usize,
        source: AbilitySourceContractV4,
    },
    CurieDeparture {
        at: usize,
        source: AbilitySourceContractV4,
        power: u32,
    },
    CurieDrawCount {
        item: StackItemId,
        combat_at: usize,
        count: u32,
    },
    /// The native start of the same instruction, with its one-time Riddler
    /// expansion separate from the base count authenticated by the source.
    StartedWithRiddlerV1 {
        started: StartedV1,
        extra: u32,
    },
    MinscSacrifice(minsc::SacrificeV1),
    MinscTarget(minsc::PublicationV1),
    MinscDamageCompleted {
        item: StackItemId,
        published_at: usize,
    },
    MinscDeparture {
        at: usize,
        departure: minsc::DepartureV1,
    },
}

impl JournalV1 {
    fn start(self) -> Option<(StartedV1, u32)> {
        match self {
            Self::Started(started)
                if matches!(started.instruction, InstructionV1::FixedRoot { .. }) =>
            {
                None
            }
            Self::Started(started) => Some((started, started.total)),
            Self::StartedWithRiddlerV1 { started, extra } if started.total != 0 && extra != 0 => {
                Some((started, started.total.checked_add(extra)?))
            }
            _ => None,
        }
    }

    fn started_item(self) -> Option<StackItemId> {
        match self {
            Self::Started(started) | Self::StartedWithRiddlerV1 { started, .. } => {
                Some(started.item)
            }
            Self::CurieDrawCount { item, .. } | Self::MinscDamageCompleted { item, .. } => Some(item),
            _ => None,
        }
    }
}

pub(crate) fn record(state: &mut GameState, journal: JournalV1) {
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
            if journal.started_item() == Some(item))
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
            (InstructionV1::Curie { combat_at }, StackItemKind::TriggeredAbility, _) => {
                eternal_legacy_e7a_v1::curie::combat_source(state, combat_at) == Some(source)
            }
            (InstructionV1::Minsc { sacrifice }, StackItemKind::TriggeredAbility, _) => {
                minsc::owns(state, item, sacrifice)
            }
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
        "Plunder the Trollshaws" | "Twinned Vision" | "Veil of Summer" | "Flame of Anor"
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
        (InstructionV1::FlameOfAnor { .. }, "Flame of Anor") => {
            // Every draw-bearing printed mode targets its drawer first.
            // Player targets have no generic Targeted event, so freeze the
            // full original target contracts in this native start receipt.
            let (drawer, other_target) = match item.v4.target_contracts.as_slice() {
                [StackTargetContractV4::Player(drawer)] => (*drawer, None),
                [StackTargetContractV4::Player(drawer), other @ StackTargetContractV4::Object { .. }] => {
                    (*drawer, Some(*other))
                }
                _ => return None,
            };
            Some((
                instruction,
                SourceV1::FlameOfAnor {
                    spell: source,
                    drawer,
                    other_target,
                },
            ))
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
            player: instruction.draw_player(),
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
        (InstructionV1::Curie { combat_at }, SourceV1::Ability(_)) => {
            eternal_legacy_e7a_v1::curie::draw_count(state, &pending.ctx, combat_at)
                .ok_or("Curie draw lost its original source base power")?
        }
        (InstructionV1::Minsc { sacrifice }, SourceV1::Ability(_)) => {
            minsc::damage_once(state, pending, sacrifice)?
        }
        _ => instruction.count(state, &pending.ctx),
    };
    if total == 0 {
        finish_instruction(state, pending, instruction);
        return Ok(());
    }
    if let InstructionV1::Curie { combat_at } = instruction {
        record(
            state,
            JournalV1::CurieDrawCount {
                item: pending.resolving_item.v4.stack_item_id,
                combat_at,
                count: total,
            },
        );
    }
    let drawer = instruction
        .drawer(&pending.ctx)
        .ok_or("Eternal draw lost its original drawer")?;
    let extra = crate::event::draw_instruction_extra(state, drawer, total);
    start_instruction(state, pending, instruction, source, total, extra);
    Ok(())
}

fn start_instruction(
    state: &mut GameState,
    pending: &mut EffectContinuation,
    instruction: InstructionV1,
    source: SourceV1,
    total: u32,
    extra: u32,
) {
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
    let journal = if extra == 0 {
        JournalV1::Started(started)
    } else {
        JournalV1::StartedWithRiddlerV1 { started, extra }
    };
    record(state, journal);
    state.eternal_owned_draw_progress_v1 = Some(ProgressV1 {
        started_at,
        completed: 0,
    });
    stage(pending, instruction, 0);
}

fn progress(
    state: &GameState,
    pending: &EffectContinuation,
) -> Option<(ProgressV1, StartedV1, u32)> {
    let progress = state.eternal_owned_draw_progress_v1?;
    let CommittedEvent::EternalOwnedDrawV1 { journal } =
        state.engine.event_history.get(progress.started_at)?
    else {
        return None;
    };
    let (started, attempts) = journal.start()?;
    let actual = if matches!(started.instruction, InstructionV1::FixedRoot { .. }) {
        fixed::selected(state, pending)
    } else {
        selected(state, pending)
    };
    if let InstructionV1::Minsc { sacrifice } = started.instruction {
        if !minsc::completed_damage(
            state,
            &pending.resolving_item,
            sacrifice,
            progress.started_at,
        ) {
            return None;
        }
    }
    (Some((started.instruction, started.source)) == actual
        && started.item == pending.resolving_item.v4.stack_item_id
        && started.controller == pending.ctx.controller
        && progress.completed < attempts)
        .then_some((progress, started, attempts))
}

pub(crate) fn completion_matches(
    state: &GameState,
    pending: &EffectContinuation,
    ordinal: u32,
    path: &[u16],
) -> bool {
    progress(state, pending).is_some_and(|(progress, started, _)| {
        ordinal == progress.completed && path == started.instruction.completion_path()
    })
}

pub(crate) fn complete(
    state: &mut GameState,
    pending: &mut EffectContinuation,
    ordinal: u32,
    path: &[u16],
) -> Result<(), String> {
    let (progress, started, attempts) =
        progress(state, pending).ok_or("Eternal draw lost its native start")?;
    if ordinal != progress.completed
        || path != started.instruction.completion_path()
        || !pending.frames.is_empty()
    {
        return Err("Eternal draw completed the wrong attempt".into());
    }
    let drawer = started
        .instruction
        .drawer(&pending.ctx)
        .ok_or("Eternal draw lost its original drawer")?;
    let completed = ordinal + 1;
    record(
        state,
        JournalV1::Completed {
            started_at: progress.started_at,
            ordinal,
        },
    );
    let terminal = draw_won_game(state, drawer);
    if terminal || completed == attempts {
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
        InstructionV1::FlameOfAnor {
            artifact,
            creature,
            spec,
            ..
        } => eternal_lurrus_v1::finish_flame_of_anor(state, &pending.ctx, artifact, creature, spec),
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
    attempts: u32,
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
            SourceV1::Spell(_) | SourceV1::FlameOfAnor { .. } => None,
        },
        (InstructionV1::Curie { combat_at }, None) => {
            let count_at = progress
                .started_at
                .checked_sub(1)
                .ok_or("Curie draw lost its native count capture")?;
            match state.engine.event_history.get(count_at) {
                Some(CommittedEvent::EternalOwnedDrawV1 { journal }) => match **journal {
                    JournalV1::CurieDrawCount {
                        item,
                        combat_at: original,
                        count,
                    } if item == started.item && original == combat_at => {
                        let SourceV1::Ability(source) = started.source else {
                            return Err("Curie draw lost its original ability source".into());
                        };
                        match eternal_legacy_e7a_v1::curie::departed_count_before(
                            state, combat_at, source, count_at,
                        ) {
                            Some(None) => Some(count),
                            Some(Some(power)) if power == count => Some(count),
                            _ => None,
                        }
                    }
                    _ => None,
                },
                _ => None,
            }
        }
        (instruction, None) => Some(instruction.count(state, ctx)),
    };
    if total != Some(started.total) {
        return Err("Eternal draw changed its original count".into());
    }
    if progress.completed == 0 {
        let drawer = started
            .instruction
            .drawer(ctx)
            .ok_or("Eternal draw lost its original drawer")?;
        if attempts - started.total
            != crate::event::draw_instruction_extra(state, drawer, started.total)
        {
            return Err("Eternal draw changed its original Riddler adjustment".into());
        }
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
    progress(state, pending).is_some_and(|(progress, started, _)| {
        Some(drawer) == started.instruction.drawer(&pending.ctx)
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
    if state.eternal_owned_draw_progress_v1.is_none() {
        let named = selected(state, pending);
        if (named.is_some() || fixed::selected(state, pending).is_some())
            && has_started(state, pending.resolving_item.v4.stack_item_id)
        {
            return Err("Eternal draw already started its original instruction".into());
        }
        let Some((instruction, _)) = named else {
            return Ok(());
        };
        let before_start = if instruction == InstructionV1::DragonEngine {
            dragon_before_start(pending)
        } else {
            matches!((&pending.choice, pending.frames.as_slice()),
                (None, [EffectFrame::Program { op, path }]) if InstructionV1::from_program(op) == Some(instruction) && path.is_empty())
        };
        if before_start {
            return Ok(());
        }
    } else if selected(state, pending).is_none() && fixed::selected(state, pending).is_none() {
        return Err("Eternal draw progress lost its original source".into());
    }
    let (progress, started, attempts) =
        progress(state, pending).ok_or("Eternal draw lost its native start")?;
    validate_native_progress(state, &pending.ctx, progress, started, attempts)?;
    let (path, remaining) =
        dredge::draw_stage(pending).ok_or("Eternal draw lost its original stage")?;
    let drawer = started
        .instruction
        .drawer(&pending.ctx)
        .ok_or("Eternal draw lost its original drawer")?;
    if !draw_stage_matches(state, pending, drawer, path, remaining) {
        return Err("Eternal draw changed its progress or remaining effects".into());
    }
    dredge::validate_draw_choice(state, pending, drawer, path, remaining, "Eternal draw")
}
