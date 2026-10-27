//! Card programs for the eternal catalog's Legacy aggro, Eldrazi and control
//! lists (wave E5b, `docs/design/eternal_formats_support_v1.md`): Boros
//! Energy, Eldrazi, Tron, Beanstalk Control and the Azorius, Jeskai and
//! Esper tempo and control lists.
//!
//! `eternal_cards_v1` forwards its effect leaves, trigger conditions and
//! state here, and the engine calls the hooks below only in
//! `eternal-fixtures` builds. Every table is keyed by a printed name that
//! occurs only in the eternal registry, so the Pauper, FDN and Standard
//! catalogs never reach it. Behavior was read from each card's current
//! Oracle text and, where present, its XMage source.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::TargetSpec;
use crate::effect::{EffectContinuation, EffectOp, ExecCtx, PlayerRef};
use crate::event::CommittedEvent;
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::StandardOpV1;
use crate::state::{GameState, Target, Zone};
use crate::trigger::{PendingTrigger, TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

/// Per-game state for this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Default (and absent from snapshots)
/// until one of these cards needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct E5bStateV1 {}

impl E5bStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Trigger conditions owned by this module, carried as
/// `eternal_cards_v1::EternalTriggerV1::E5bV1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum E5bTriggerV1 {}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::E5bV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum E5bOpV1 {
    /// Stifle: counter target 0, an activated or triggered ability.
    CounterTargetAbilityV1,
}

impl E5bOpV1 {
    /// Whether this leaf can yield a player decision, so its program must
    /// use the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        false
    }
}

fn e5b(op: E5bOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(
        crate::eternal_cards_v1::EternalOpV1::E5bV1(op),
    ))
}

const UP_THE_BEANSTALK: &str = "Up the Beanstalk";

fn halt(state: &mut GameState, source: ObjectId) {
    state.engine.halted = Some((
        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
        source,
    ));
}

// ---- Spells ----------------------------------------------------------------------

/// Stifle: "Counter target activated or triggered ability. (Mana abilities
/// can't be targeted.)"
pub fn stifle() -> EffectOp {
    e5b(E5bOpV1::CounterTargetAbilityV1)
}

// ---- Triggered abilities ---------------------------------------------------------

const fn trigger(condition: TriggerCondition, effect: fn() -> EffectOp) -> TriggeredAbilityDef {
    TriggeredAbilityDef {
        condition,
        home_zone: Zone::Battlefield,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index: 0,
        effect,
    }
}

fn draw_one() -> EffectOp {
    EffectOp::DrawCards {
        player: PlayerRef::Controller,
        count: 1,
    }
}

/// Up the Beanstalk: "When this enchantment enters and whenever you cast a
/// spell with mana value 5 or greater, draw a card."
const UP_THE_BEANSTALK_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, draw_one),
    trigger(TriggerCondition::CastSpellManaValueAtLeast(5), draw_one),
];

/// Triggered abilities of this module's cards.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        UP_THE_BEANSTALK => &UP_THE_BEANSTALK_TRIGGERS,
        _ => &[],
    }
}

fn own_trigger_effect(name: &str, effect: &EffectOp) -> bool {
    triggers_for(name)
        .iter()
        .any(|trigger| (trigger.effect)() == *effect)
        || delayed_effect_matches(name, effect)
}

/// Target specs of this module's triggered abilities, per ability.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    if !own_trigger_effect(name, effect) {
        return None;
    }
    Some(TargetSpec::None)
}

/// Queued and delayed triggers owned by this module's cards.
pub(crate) fn delayed_effect_matches(_name: &str, _effect: &EffectOp) -> bool {
    false
}

pub(crate) fn trigger_matches(
    condition: E5bTriggerV1,
    _events: &[CommittedEvent],
    _index: usize,
    _source: ObjectId,
    _state: &GameState,
) -> bool {
    match condition {}
}

/// The triggers this collection puts on the stack that no printed trigger
/// condition matches.
pub(crate) fn delayed_triggers(
    _state: &mut GameState,
    _events: &[CommittedEvent],
) -> Vec<PendingTrigger> {
    Vec::new()
}

/// The beginning of `active`'s turn ends this module's turn-bound records.
pub(crate) fn begin_turn(_state: &mut GameState, _active: PlayerId) {}

// ---- Execution -------------------------------------------------------------------

/// Runs a leaf inside the resumable interpreter. `Ok(true)` when it staged a
/// choice and the continuation must suspend.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &E5bOpV1,
    _path: Vec<u16>,
) -> Result<bool, String> {
    execute(op, &continuation.ctx, state);
    Ok(false)
}

pub(crate) fn execute(op: &E5bOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        E5bOpV1::CounterTargetAbilityV1 => {
            let Some(Target::StackItem(item)) = ctx.targets.first().copied() else {
                return;
            };
            if !ctx.target_incarnation_matches(0, state) {
                return;
            }
            let is_ability = state.stack.iter().any(|entry| {
                entry.v4.stack_item_id == item
                    && matches!(
                        entry.kind,
                        crate::state::StackItemKind::ActivatedAbility
                            | crate::state::StackItemKind::TriggeredAbility
                    )
            });
            if is_ability && crate::engine::counter_stack_item_by_id(state, item).is_err() {
                halt(state, ctx.source);
            }
        }
    }
}
