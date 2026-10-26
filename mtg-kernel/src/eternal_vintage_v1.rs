//! Card programs for the eternal catalog's Vintage cards (wave E4: Jewel
//! Shops and Mono-White Initiative, `docs/design/eternal_formats_support_v1.md`).
//!
//! `eternal_cards_v1` forwards its effect leaves, trigger conditions, state
//! and triggered-ability tables here. Every table is keyed by a printed name
//! that occurs only in the eternal registry, so the Pauper, FDN and Standard
//! catalogs never reach it. Behavior was read from each card's current
//! Oracle text and, where present, its XMage source.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::TargetSpec;
use crate::effect::{EffectOp, ExecCtx};
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::ObjectId;
use crate::standard_cards_v1::StandardOpV1;
use crate::state::{GameState, Target, Zone};
use crate::trigger::{TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

/// Per-game state for this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Default (and absent from snapshots)
/// until one of these cards needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VintageStateV1 {}

impl VintageStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Trigger conditions owned by this module, carried as
/// `eternal_cards_v1::EternalTriggerV1::VintageV1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VintageTriggerV1 {}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::VintageV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VintageOpV1 {
    /// Mana Crypt: "flip a coin. If you lose the flip, this artifact deals
    /// `damage` damage to you." The flip uses the game's seeded randomness.
    FlipCoinLoseSourceDamagesControllerV1 { damage: u8 },
}

fn vintage(op: VintageOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(
        crate::eternal_cards_v1::EternalOpV1::VintageV1(op),
    ))
}

const MANA_CRYPT: &str = "Mana Crypt";

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

/// Mana Crypt: "At the beginning of your upkeep, flip a coin. If you lose
/// the flip, this artifact deals 3 damage to you."
fn mana_crypt_flip() -> EffectOp {
    vintage(VintageOpV1::FlipCoinLoseSourceDamagesControllerV1 { damage: 3 })
}

const MANA_CRYPT_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::BeginningOfUpkeep {
        controller_only: true,
    },
    mana_crypt_flip,
)];

/// Triggered abilities of this module's cards.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        MANA_CRYPT => &MANA_CRYPT_TRIGGERS,
        _ => &[],
    }
}

/// Target specs of this module's triggered abilities, per ability.
pub(crate) fn trigger_target_spec(name: &str, _effect: &EffectOp) -> Option<TargetSpec> {
    match name {
        MANA_CRYPT => Some(TargetSpec::None),
        _ => None,
    }
}

pub(crate) fn execute(op: &VintageOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        VintageOpV1::FlipCoinLoseSourceDamagesControllerV1 { damage } => {
            match state.flip_coin_v1(ctx.controller) {
                Ok(true) => {}
                Ok(false) => event::propose_and_commit(
                    state,
                    ProposedEvent::damage(
                        ctx.source,
                        Target::Player(ctx.controller),
                        i32::from(*damage),
                    ),
                ),
                Err(_) => {
                    state.engine.halted = Some((
                        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                        ctx.source,
                    ));
                }
            }
        }
    }
}

pub(crate) fn trigger_matches(
    condition: VintageTriggerV1,
    _events: &[CommittedEvent],
    _index: usize,
    _source: ObjectId,
    _state: &GameState,
) -> bool {
    match condition {}
}
