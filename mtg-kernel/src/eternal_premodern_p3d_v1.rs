//! Card programs for the eternal catalog's Premodern sideboard wave P3d
//! (`docs/design/eternal_formats_support_v1.md`): the sideboard cards that
//! complete Enchantress.
//!
//! `eternal_cards_v1` forwards its effect leaves, queued-trigger checks and
//! turn bookkeeping here, wave P3c's recipe tables fall through to
//! `build_eternal_p3d_v1.rs`, and the engine calls the hooks below only in
//! `eternal-fixtures` builds. Every table is keyed by a printed name that
//! occurs only in the eternal registry. Behavior follows each card's current
//! Oracle text and rulings.
//!
//! Primitives added here:
//! - until end of turn, target player can't cast instant or sorcery spells
//!   or activate abilities that aren't mana abilities (Abeyance);
//! - the departure cause: the controller of the spell or ability whose
//!   effects are running, kept in a runtime scope that is never serialized,
//!   hashed or compared;
//! - triggers on a permanent that an opponent's spell or ability destroys
//!   (Karmic Justice) or puts into your graveyard (Sacred Ground), with a
//!   batch of simultaneous moves read before any of them happens.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{CardDef, CardType, TargetSpec};
use crate::effect::{EffectOp, ExecCtx, PlayerRef};
use crate::eternal_cards_v1::EternalOpV1;
use crate::eternal_legacy_e9a_v1::{active_permanents, link_live_in, TurnV1};
use crate::eternal_premodern_p1a_v1::{
    destroy, legal_object_target, legal_player_target, queue_trigger,
};
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::{StandardOpV1, StandardTargetV1};
use crate::state::{AbilitySourceContractV4, CastMethodV4, GameState, ObjectLinkV4, Zone};
use serde::{Deserialize, Serialize};

pub(crate) const KARMIC_JUSTICE: &str = "Karmic Justice";
pub(crate) const SACRED_GROUND: &str = "Sacred Ground";

/// Karmic Justice's target: "target permanent that opponent controls".
const KARMIC_TARGET: TargetSpec = TargetSpec::StandardV1(StandardTargetV1::OpponentPermanentV1);

/// Per-game state of this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Absent from every snapshot until one
/// of them needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct P3dStateV1 {
    /// Each player an Abeyance affects, with the turn it lasts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub abeyances: Vec<(PlayerId, TurnV1)>,
}

impl P3dStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

pub(crate) fn p3d_state(state: &mut GameState) -> &mut P3dStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).p3d
}

pub fn read_state(state: &GameState) -> Option<&P3dStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.p3d)
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::P3dV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum P3dOpV1 {
    /// Abeyance: until end of turn, target player 0 can't cast instant or
    /// sorcery spells or activate abilities that aren't mana abilities.
    AbeyanceV1,
    /// Karmic Justice: destroy target permanent 0, which an opponent
    /// controls (the trigger's "you may" option).
    KarmicDestroyV1,
    /// Karmic Justice: destroy nothing (the other option).
    NoEffectV1,
    /// Sacred Ground: return `card`, the graveyard incarnation of the land
    /// card that left the battlefield, to the battlefield.
    SacredGroundReturnV1 { card: ObjectLinkV4 },
}

impl P3dOpV1 {
    /// Whether this leaf can yield a player decision or pushes frames. None
    /// of this module's leaves does: Karmic Justice's "you may" is the
    /// engine's own choice around them.
    pub(crate) fn contains_player_choice(&self) -> bool {
        false
    }
}

fn p3d(op: P3dOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::P3dV1(op)))
}

// ---- Programs -----------------------------------------------------------------------

/// Abeyance: "Until end of turn, target player can't cast instant or
/// sorcery spells, and that player can't activate abilities that aren't
/// mana abilities. Draw a card."
pub fn abeyance() -> EffectOp {
    EffectOp::Sequence(vec![
        p3d(P3dOpV1::AbeyanceV1),
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        },
    ])
}

/// Karmic Justice: "you may destroy target permanent that opponent
/// controls." The target is chosen as the trigger is put on the stack, and
/// whether to destroy it on resolution.
fn karmic_trigger() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: vec![p3d(P3dOpV1::KarmicDestroyV1), p3d(P3dOpV1::NoEffectV1)],
    }
}

// ---- Statics: Abeyance --------------------------------------------------------------

/// Abeyance: whether `player` can't cast instant or sorcery spells or
/// activate abilities that aren't mana abilities this turn.
pub(crate) fn abeyance_active(state: &GameState, player: PlayerId) -> bool {
    let now = TurnV1::now(state);
    read_state(state).is_some_and(|p3d| p3d.abeyances.contains(&(player, now)))
}

/// Abeyance: whether it keeps `player` from casting `def` in the form
/// `method` selects. The form's own types decide, so an Adventure's
/// instant is barred and its creature is not.
pub(crate) fn abeyance_prohibits_cast(
    state: &GameState,
    player: PlayerId,
    def: &CardDef,
    method: CastMethodV4,
) -> bool {
    if !abeyance_active(state, player) {
        return false;
    }
    let Ok(types) = crate::source_type_hexproof_v1::spell_types(def, method) else {
        return true;
    };
    types.contains(CardType::Instant) || types.contains(CardType::Sorcery)
}

// ---- Hooks: the departure cause -----------------------------------------------------

/// Whose spell or ability made a permanent leave the battlefield. A runtime
/// scope in `engine::EngineState`, set only while effects run and restored
/// before control returns to a player, so it is never serialized, hashed or
/// compared.
#[derive(Debug, Clone, Default)]
pub(crate) struct DepartureCauseV1 {
    /// The controller of the spell or ability whose effects are running.
    cause: Option<PlayerId>,
    /// The resolving ability's source as its contract froze it when the
    /// ability was activated or triggered. Wave P3g's Circles of Protection
    /// read it to find damage a departed source deals (113.7a).
    ability_source: Option<AbilitySourceContractV4>,
    /// The batch of simultaneous moves being committed, read before any of
    /// them happens.
    batch: Option<DepartureBatchV1>,
}

/// The scope a resolution entered, which `leave_resolution` restores.
#[derive(Debug, Clone, Copy)]
pub(crate) struct EnclosingResolutionV1 {
    cause: Option<PlayerId>,
    ability_source: Option<AbilitySourceContractV4>,
}

impl PartialEq for DepartureCauseV1 {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Eq for DepartureCauseV1 {}

impl std::hash::Hash for DepartureCauseV1 {
    fn hash<H: std::hash::Hasher>(&self, _: &mut H) {}
}

/// The watchers and departing permanents of one batch of simultaneous
/// moves, as they were before the batch began (603.10a).
#[derive(Debug, Clone, Default)]
pub(crate) struct DepartureBatchV1 {
    watchers: Vec<WatcherV1>,
    departing: Vec<DepartingV1>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WatchV1 {
    KarmicJustice,
    SacredGround,
}

/// A Karmic Justice or Sacred Ground on the battlefield.
#[derive(Debug, Clone, Copy)]
struct WatcherV1 {
    source: AbilitySourceContractV4,
    watch: WatchV1,
}

/// A departing permanent's characteristics just before it leaves.
#[derive(Debug, Clone, Copy)]
struct DepartingV1 {
    object: ObjectId,
    controller: PlayerId,
    creature: bool,
    land: bool,
}

fn scope(state: &mut GameState) -> &mut DepartureCauseV1 {
    &mut state.engine.departure_cause_v1
}

fn enclosing(state: &GameState) -> EnclosingResolutionV1 {
    let scope = &state.engine.departure_cause_v1;
    EnclosingResolutionV1 {
        cause: scope.cause,
        ability_source: scope.ability_source,
    }
}

/// Enters the scope of the spell or ability whose effects `ctx` runs, and
/// returns the enclosing scope for `leave_resolution`. Only the outermost
/// context of a stack item sets the cause and the ability's source: an
/// effect nested in it may run for another player but belongs to the same
/// resolution, and a context that doesn't come from a stack item (a mana
/// ability, a cost, a state-based action) sets nothing.
pub(crate) fn enter_resolution(state: &mut GameState, ctx: &ExecCtx) -> EnclosingResolutionV1 {
    let enclosing = enclosing(state);
    if enclosing.cause.is_none() && ctx.stack_item_id.is_some() {
        let scope = scope(state);
        scope.cause = Some(ctx.controller);
        scope.ability_source = ctx.ability_source_contract;
    }
    enclosing
}

/// Enters the scope of the effect continuation the engine is about to
/// drive, and returns the enclosing scope for `leave_resolution`.
pub(crate) fn enter_pending_resolution(state: &mut GameState) -> EnclosingResolutionV1 {
    let enclosing = enclosing(state);
    let resolving = state
        .engine
        .pending_effect
        .as_ref()
        .filter(|pending| pending.ctx.stack_item_id.is_some())
        .map(|pending| (pending.ctx.controller, pending.ctx.ability_source_contract));
    if let (None, Some((controller, ability_source))) = (enclosing.cause, resolving) {
        let scope = scope(state);
        scope.cause = Some(controller);
        scope.ability_source = ability_source;
    }
    enclosing
}

/// Restores the scope `enter_resolution` or `enter_pending_resolution`
/// returned.
pub(crate) fn leave_resolution(state: &mut GameState, enclosing: EnclosingResolutionV1) {
    let scope = scope(state);
    scope.cause = enclosing.cause;
    scope.ability_source = enclosing.ability_source;
}

/// The source contract of the ability whose effects are running, if a
/// stack item's ability resolves (none for a spell).
pub(crate) fn resolving_ability_source(state: &GameState) -> Option<AbilitySourceContractV4> {
    let scope = &state.engine.departure_cause_v1;
    scope.cause.and(scope.ability_source)
}

/// Reads the watchers and departing permanents of a batch of simultaneous
/// moves before any of them happens, so a Karmic Justice that leaves in the
/// batch still sees the others (603.10a). Returns the enclosing batch for
/// `end_departure_batch`.
pub(crate) fn begin_departure_batch(
    state: &mut GameState,
    events: &[ProposedEvent],
) -> Option<DepartureBatchV1> {
    let batch = scope(state).cause.and_then(|_| {
        let watchers = watchers(state);
        (!watchers.is_empty()).then(|| DepartureBatchV1 {
            watchers,
            departing: events
                .iter()
                .filter_map(|event| match event {
                    ProposedEvent::ZoneChange(change) => state
                        .objects
                        .try_get(change.object)
                        .filter(|live| live.zone == Zone::Battlefield)
                        .map(|live| departing(state, change.object, live.controller)),
                    _ => None,
                })
                .collect(),
        })
    });
    let enclosing = scope(state).batch.take();
    // Outside a resolution, or with no watcher, a batch sets nothing off;
    // an empty batch keeps a nested move from reading the live battlefield.
    scope(state).batch = Some(batch.unwrap_or_default());
    enclosing
}

/// Restores the batch `begin_departure_batch` returned.
pub(crate) fn end_departure_batch(state: &mut GameState, enclosing: Option<DepartureBatchV1>) {
    scope(state).batch = enclosing;
}

fn watchers(state: &GameState) -> Vec<WatcherV1> {
    [
        (KARMIC_JUSTICE, WatchV1::KarmicJustice),
        (SACRED_GROUND, WatchV1::SacredGround),
    ]
    .into_iter()
    .flat_map(|(name, watch)| {
        active_permanents(state, name, None).map(move |object| WatcherV1 {
            source: AbilitySourceContractV4::capture(state, object),
            watch,
        })
    })
    .collect()
}

fn departing(state: &GameState, object: ObjectId, controller: PlayerId) -> DepartingV1 {
    DepartingV1 {
        object,
        controller,
        creature: crate::engine::object_has_type(state, object, CardType::Creature),
        land: crate::engine::object_has_type(state, object, CardType::Land),
    }
}

/// Whether the engine's sacrifice marker for `object` precedes its move:
/// the latest marker or move of `object` among this collection's events is
/// a sacrifice.
fn being_sacrificed(state: &GameState, object: ObjectId) -> bool {
    state
        .engine
        .event_log
        .iter()
        .rev()
        .find_map(|event| match event {
            CommittedEvent::Sacrificed {
                object: sacrificed, ..
            } if *sacrificed == object => Some(true),
            CommittedEvent::ZoneChange { object: moved, .. } if *moved == object => Some(false),
            _ => None,
        })
        .unwrap_or(false)
}

/// Called by the zone-change choke point as `object` moves from the
/// battlefield to its owner's graveyard, before the move, with its
/// controller just before it. Inside a resolution, queues the Karmic
/// Justice and Sacred Ground triggers the move sets off.
///
/// Karmic Justice: "Whenever a spell or ability an opponent controls
/// destroys a noncreature permanent you control, you may destroy target
/// permanent that opponent controls." A sacrifice isn't a destruction.
///
/// Sacred Ground: "Whenever a spell or ability an opponent controls causes
/// a land to be put into your graveyard from the battlefield, return that
/// card to the battlefield."
pub(crate) fn note_departure(state: &mut GameState, object: ObjectId, controller_before: PlayerId) {
    let Some(cause) = state.engine.departure_cause_v1.cause else {
        return;
    };
    let (departing, watchers) = match state.engine.departure_cause_v1.batch.as_ref() {
        Some(batch) => (
            batch
                .departing
                .iter()
                .find(|departing| departing.object == object)
                .copied()
                .unwrap_or_else(|| departing(state, object, controller_before)),
            batch.watchers.clone(),
        ),
        None => (departing(state, object, controller_before), watchers(state)),
    };
    if watchers.is_empty() {
        return;
    }
    let live = state.objects.get(object);
    let owner = live.owner;
    // The card's incarnation once the move puts it in the graveyard.
    let card = ObjectLinkV4 {
        object,
        zone_change_count: live.zone_change_count + 1,
    };
    let sacrificed = being_sacrificed(state, object);
    for watcher in watchers {
        let controller = watcher.source.controller;
        if controller == cause {
            continue;
        }
        match watcher.watch {
            WatchV1::KarmicJustice => {
                if !departing.creature && departing.controller == controller && !sacrificed {
                    queue_trigger(
                        state,
                        watcher.source,
                        controller,
                        karmic_trigger(),
                        Some(KARMIC_TARGET),
                    );
                }
            }
            WatchV1::SacredGround => {
                if departing.land && owner == controller {
                    queue_trigger(
                        state,
                        watcher.source,
                        controller,
                        p3d(P3dOpV1::SacredGroundReturnV1 { card }),
                        None,
                    );
                }
            }
        }
    }
}

// ---- Queued triggers ----------------------------------------------------------------

/// Queued triggers owned by this module's cards.
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    match name {
        KARMIC_JUSTICE => *effect == karmic_trigger(),
        SACRED_GROUND => matches!(
            effect,
            EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::P3dV1(
                P3dOpV1::SacredGroundReturnV1 { .. }
            )))
        ),
        _ => false,
    }
}

/// Target specs of this module's queued triggers. `None` for every other
/// card and effect.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    delayed_effect_matches(name, effect).then_some(if name == KARMIC_JUSTICE {
        KARMIC_TARGET
    } else {
        TargetSpec::None
    })
}

/// The beginning of a turn ends the earlier turns' Abeyances.
pub(crate) fn begin_turn(state: &mut GameState, _player: PlayerId) {
    let now = TurnV1::now(state);
    if read_state(state).is_none_or(|p3d| p3d.abeyances.iter().all(|(_, turn)| *turn == now)) {
        return;
    }
    p3d_state(state).abeyances.retain(|(_, turn)| *turn == now);
}

// ---- Effect leaves ------------------------------------------------------------------

pub(crate) fn execute(op: &P3dOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        P3dOpV1::AbeyanceV1 => {
            let Some(player) = legal_player_target(ctx, state, 0, TargetSpec::AnyPlayer) else {
                return;
            };
            let record = (player, TurnV1::now(state));
            let abeyances = &mut p3d_state(state).abeyances;
            if !abeyances.contains(&record) {
                abeyances.push(record);
            }
        }
        P3dOpV1::KarmicDestroyV1 => {
            if let Some(permanent) = legal_object_target(ctx, state, 0, KARMIC_TARGET) {
                destroy(ctx, state, permanent);
            }
        }
        P3dOpV1::NoEffectV1 => {}
        P3dOpV1::SacredGroundReturnV1 { card } => {
            // "That card": a token, or a card that has left the graveyard
            // since, stays where it is.
            if link_live_in(state, *card, Zone::Graveyard) {
                let owner = state.objects.get(card.object).owner;
                event::propose_and_commit(
                    state,
                    ProposedEvent::zone_change_to_battlefield_under_controller(card.object, owner),
                );
            }
        }
    }
}
