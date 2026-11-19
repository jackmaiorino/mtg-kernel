//! Card programs for the eternal catalog's Vintage Cube wave C6b
//! (`docs/design/eternal_formats_support_v1.md`): discard counts, dies copies and graveyard plays.
//!
//! `eternal_cards_v1` forwards this module's effect leaves here, and the
//! recipe tables of `build_eternal_c6b_v1.rs` name its programs. Every
//! table is keyed by a printed name that occurs only in the eternal registry,
//! so the Pauper, FDN and Standard catalogs never reach it. Behavior was read
//! from each card's current Oracle text and, where present, its XMage source.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{card_id_by_name, CardType, TargetSpec, CARD_DEFS};
use crate::effect::{
    EffectContinuation, EffectObjectBinding, EffectOp, ExecCtx, ObjectRef, PlayerRef,
};
use crate::eternal_cards_v1::e3::{E3ChoiceKindV1, E3ChoiceV1};
use crate::eternal_cards_v1::EternalOpV1;
use crate::ids::StackItemId;
use crate::standard_cards_v1::{StandardOpV1, StandardTargetV1};
use crate::state::{GameState, Target, Zone};
use crate::trigger::{TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

const CRYOMANCER: &str = "Seasoned Cryomancer";
const PYROMANCER: &str = "Seasoned Pyromancer";
// Appended token row. Elemental already exists, so this call uses no
// subtype IDs from the reserved C6b range 710..719.
const ELEMENTAL: &str = "1/1 red Elemental Token";

/// Pyromancer's count survives its replaceable draw without changing the
/// definition-owned continuation frames. Entries belong to one resolution.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct C6bStateV1 {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub discarded_nonlands: Vec<(StackItemId, u16)>,
}

impl C6bStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        self.discarded_nonlands.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C6bOpV1 {
    /// Mandatory discard, as many as possible, with a resolution-local count.
    DiscardTwoV1 { cryomancer: bool },
    /// Pyromancer's final instruction, after its static draw instruction.
    DiscardedElementalsV1,
    /// Cryomancer's separate reflexive trigger, with one or two target slots.
    CryomancerStunV1 { count: u16 },
}

impl C6bOpV1 {
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(self, Self::DiscardTwoV1 { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C6bChoiceV1 {
    DiscardTwoV1 { cryomancer: bool },
}

fn c6b(op: C6bOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C6bV1(op)))
}

pub fn draw_two() -> EffectOp {
    EffectOp::Sequence(vec![EffectOp::DrawCards {
        player: PlayerRef::Controller,
        count: 2,
    }])
}

fn elemental() -> EffectOp {
    EffectOp::CreateToken {
        token_def: card_id_by_name(ELEMENTAL).expect("Elemental token registry row"),
        controller: PlayerRef::Controller,
    }
}

pub fn two_elementals() -> EffectOp {
    EffectOp::Sequence(vec![elemental(), elemental()])
}

pub fn cryomancer_entry() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 2,
        },
        crate::eternal_cube_c1a_v1::stop_if_won(),
        c6b(C6bOpV1::DiscardTwoV1 { cryomancer: true }),
    ])
}

pub fn pyromancer_entry() -> EffectOp {
    EffectOp::Sequence(vec![
        c6b(C6bOpV1::DiscardTwoV1 { cryomancer: false }),
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 2,
        },
        crate::eternal_cube_c1a_v1::stop_if_won(),
        c6b(C6bOpV1::DiscardedElementalsV1),
    ])
}

const fn etb(effect: fn() -> EffectOp) -> TriggeredAbilityDef {
    TriggeredAbilityDef {
        condition: TriggerCondition::Etb,
        home_zone: Zone::Battlefield,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index: 0,
        effect,
    }
}

const CRYOMANCER_TRIGGERS: [TriggeredAbilityDef; 1] = [etb(cryomancer_entry)];
const PYROMANCER_TRIGGERS: [TriggeredAbilityDef; 1] = [etb(pyromancer_entry)];

pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        CRYOMANCER => &CRYOMANCER_TRIGGERS,
        PYROMANCER => &PYROMANCER_TRIGGERS,
        _ => crate::eternal_cube_c6c_v1::triggers_for(name),
    }
}

fn stun_targets(count: u16) -> TargetSpec {
    if count == 1 {
        TargetSpec::StandardV1(StandardTargetV1::UpToOneCreature)
    } else {
        TargetSpec::UpToTwoCreatures
    }
}

pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    if (name == CRYOMANCER && *effect == cryomancer_entry())
        || (name == PYROMANCER && *effect == pyromancer_entry())
    {
        return Some(TargetSpec::None);
    }
    if name == CRYOMANCER {
        if let EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C6bV1(
            C6bOpV1::CryomancerStunV1 {
                count: count @ 1..=2,
            },
        ))) = effect
        {
            return Some(stun_targets(*count));
        }
    }
    None
}

pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    name == CRYOMANCER
        && matches!(
            effect,
            EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C6bV1(
                C6bOpV1::CryomancerStunV1 { count: 1..=2 }
            )))
        )
}

/// Occult Epiphany's batch commit and Raffine's Informant's nonland
/// classification. Replaced destinations, including Madness exile, still
/// count when the selected hand incarnation was actually moved.
fn discard_selected(
    state: &mut GameState,
    continuation: &EffectContinuation,
    cryomancer: bool,
    bindings: &[EffectObjectBinding],
) {
    let player = continuation.ctx.controller;
    let cards: Vec<_> = bindings
        .iter()
        .copied()
        .filter(|binding| {
            state.objects.try_get(binding.object).is_some_and(|live| {
                live.zone == Zone::Hand
                    && live.zone_change_count == binding.expected_zone_change_count
                    && state.players[player.index()].hand.contains(&binding.object)
            })
        })
        .collect();
    let nonlands: Vec<_> = cards
        .iter()
        .filter(|binding| {
            !CARD_DEFS[state.objects.get(binding.object).card_def as usize].has_type(CardType::Land)
        })
        .copied()
        .collect();
    crate::engine::commit_discarded_cards(
        state,
        &cards.iter().map(|card| card.object).collect::<Vec<_>>(),
    );
    let count = u16::try_from(
        nonlands
            .iter()
            .filter(|binding| {
                state.objects.get(binding.object).zone_change_count
                    != binding.expected_zone_change_count
            })
            .count(),
    )
    .unwrap_or(2);
    if cryomancer {
        if count > 0 {
            if let Some(source) = continuation.ctx.ability_source_contract {
                let mut pending = crate::eternal_cards_v1::delayed_trigger(
                    source,
                    player,
                    c6b(C6bOpV1::CryomancerStunV1 { count }),
                );
                pending.target_spec = stun_targets(count);
                state.engine.pending_triggers.push(pending);
            }
        }
    } else if count > 0 {
        let key = continuation.resolving_item.v4.stack_item_id;
        let counts = &mut state
            .eternal_v1
            .get_or_insert_with(Default::default)
            .c6b
            .discarded_nonlands;
        counts.retain(|(item, _)| *item != key);
        counts.push((key, count));
    }
}

pub(crate) fn execute(op: &C6bOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        C6bOpV1::DiscardedElementalsV1 => {
            let count = state
                .eternal_v1
                .as_mut()
                .and_then(|eternal| {
                    let counts = &mut eternal.c6b.discarded_nonlands;
                    let index = counts
                        .iter()
                        .position(|(item, _)| Some(*item) == ctx.stack_item_id)?;
                    Some(counts.remove(index).1)
                })
                .unwrap_or(0);
            for _ in 0..count {
                crate::effect::execute(&elemental(), ctx, state);
            }
        }
        C6bOpV1::CryomancerStunV1 { count } => {
            for index in 0..usize::from(*count).min(ctx.targets.len()) {
                let Some(Target::Object(object)) = ctx.targets.get(index).copied() else {
                    continue;
                };
                if !ctx.target_incarnation_matches(index, state)
                    || state.objects.get(object).zone != Zone::Battlefield
                    || !ctx.ability_source_contract.is_some_and(|source| {
                        crate::engine::effect_target_is_legal_from_ability_source(
                            state,
                            source,
                            ctx.controller,
                            TargetSpec::Creature,
                            &ctx.targets[index..=index],
                            0,
                        )
                    })
                {
                    continue;
                }
                let target_index = u8::try_from(index).unwrap_or(0);
                crate::effect::execute(
                    &EffectOp::TapObject {
                        object: ObjectRef::Target(target_index),
                    },
                    ctx,
                    state,
                );
                crate::effect::execute(
                    &EffectOp::AddCountersToTarget {
                        target_index,
                        optional: true,
                        plus1_plus1: 0,
                        lifelink: 0,
                        stun: 1,
                    },
                    ctx,
                    state,
                );
            }
        }
        // Choice leaves are dispatched to drive by contains_player_choice.
        C6bOpV1::DiscardTwoV1 { .. } => {}
    }
}

pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &C6bOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    if let C6bOpV1::DiscardTwoV1 { cryomancer } = op {
        let player = continuation.ctx.controller;
        let bindings: Vec<_> = state.players[player.index()]
            .hand
            .iter()
            .map(|&object| EffectObjectBinding {
                object,
                expected_zone: Zone::Hand,
                expected_zone_change_count: state.objects.get(object).zone_change_count,
            })
            .collect();
        if bindings.len() <= 2 {
            discard_selected(state, continuation, *cryomancer, &bindings);
            return Ok(false);
        }
        let choice = E3ChoiceV1 {
            kind: E3ChoiceKindV1::C6bV1(C6bChoiceV1::DiscardTwoV1 {
                cryomancer: *cryomancer,
            }),
            player,
            candidates: bindings
                .iter()
                .map(|binding| Target::Object(binding.object))
                .collect(),
            bindings,
            min: 2,
            max: 2,
            ordered: false,
            path,
            remaining: continuation.frames.clone(),
        };
        crate::eternal_cards_v1::e3::offer(continuation, choice);
        Ok(true)
    } else {
        execute(op, &continuation.ctx, state);
        Ok(false)
    }
}

pub(crate) fn finish_selection(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    kind: &C6bChoiceV1,
    bindings: Vec<EffectObjectBinding>,
) -> Result<bool, String> {
    let C6bChoiceV1::DiscardTwoV1 { cryomancer } = kind;
    discard_selected(state, continuation, *cryomancer, &bindings);
    Ok(false)
}
