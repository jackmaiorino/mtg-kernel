//! Card programs for the eternal catalog's Vintage Cube wave C6b
//! (`docs/design/eternal_formats_support_v1.md`): discard counts, dies copies and graveyard plays.
//!
//! `eternal_cards_v1` forwards this module's effect leaves here, and the
//! recipe tables of `build_eternal_c6b_v1.rs` name its programs. Every
//! table is keyed by a printed name that occurs only in the eternal registry,
//! so the Pauper, FDN and Standard catalogs never reach it. Behavior was read
//! from each card's current Oracle text and, where present, its XMage source.
//!
//! Sphinx reuses Wurmcoil Engine's battlefield-to-graveyard death condition
//! (no separate last-known creature-type check) and Homunculus Horde's copy
//! representation. As with Horde, extra copy
//! modifications not represented by that operation (such as Metamorph's
//! added artifact type) are not propagated. No new token or subtype IDs.
//!
//! Malcolm remains no_effect: resolution_cast_v1 only supports paid graveyard
//! casts. Its free-cast checks and E5b answer authorization accept hand/exile,
//! not the exact graveyard incarnation of the card just discarded.
//!
//! Vaultborn Tyrant remains no_effect: source_token_copy_v1 and TokenEntryV48
//! cannot carry an added artifact copy type. Metamorph's separate copy record
//! is not a token-creation initialization or a propagated copiable value.
//! Call 4 reserves Doctor subtype ID 710; no new token rows are needed.
//!
//! Fanatic is restored to no_effect. Its retained, inactive program bypasses
//! Metamorph's entry copy choice and places the lifelink counter after entry.
//! Reanimate's prepare_reanimated_metamorph is not a generic return helper:
//! it requires a paid Reanimate spell and restricts the copy menu to supported
//! entry forms (excluding Fanatic itself). Extending its replay ownership and
//! entry initialization is outside the small additive shared-edit scope.
//! Re-enable only with a trigger-owned copy choice and an entry-time counter.
//!
//! Six follows Full Wrenn and Six's retrace conventions: another graveyard
//! permission takes precedence over retrace, and spell_costs_v1 declines a
//! quote requiring two independent object-cost selections. Thus retrace plus
//! delve, Solitude's pitch-evoke or Abhorrent Oculus's graveyard-exile cost
//! is not offered. Wrenn's emblem shares this guard and cannot retrace
//! Force of Will's pitch cost either.
//! Native flashback/escape routes retain the same precedence as with Wrenn.
//! No new payment subsystem is introduced.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{card_id_by_name, CardType, TargetSpec, CARD_DEFS};
use crate::effect::{
    EffectContinuation, EffectObjectBinding, EffectOp, ExecCtx, ObjectRef, PlayerRef,
};
use crate::eternal_cards_v1::e3::{E3ChoiceKindV1, E3ChoiceV1};
use crate::eternal_cards_v1::EternalOpV1;
use crate::eternal_legacy_e9a_v1::TurnV1;
use crate::ids::{ObjectId, PlayerId, StackItemId};
use crate::standard_cards_v1::{StandardOpV1, StandardTargetV1};
use crate::state::{GameState, Target, Zone};
use crate::trigger::{TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

pub(crate) mod targets;
pub(crate) mod ursine;

const CRYOMANCER: &str = "Seasoned Cryomancer";
const PYROMANCER: &str = "Seasoned Pyromancer";
const SPHINX: &str = "Sphinx of False Conclusions";
const FANATIC: &str = "Metamorphosis Fanatic";
const NURSE: &str = "Night Nurse, Healer of Heroes";
const SIX: &str = "Six";
// Elemental already exists; the token row required no new subtype ID.
const ELEMENTAL: &str = "1/1 red Elemental Token";

/// Pyromancer's count survives its replaceable draw without changing the
/// definition-owned continuation frames. Entries belong to one resolution.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct C6bStateV1 {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub discarded_nonlands: Vec<(StackItemId, u16)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub graveyard_entries: Vec<(ObjectId, u32, TurnV1)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub six_mills: Vec<(StackItemId, Vec<EffectObjectBinding>)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub six_casts: Vec<(ObjectId, u32, PlayerId)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ursine_combats: Vec<ursine::CombatV1>,
}

impl C6bStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        self.discarded_nonlands.is_empty()
            && self.graveyard_entries.is_empty()
            && self.six_mills.is_empty()
            && self.six_casts.is_empty()
            && self.ursine_combats.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C6bOpV1 {
    /// Mandatory discard, as many as possible, with a resolution-local count.
    DiscardTwoV1 {
        cryomancer: bool,
    },
    /// Pyromancer's final instruction, after its static draw instruction.
    DiscardedElementalsV1,
    /// Cryomancer's separate reflexive trigger, with one or two target slots.
    CryomancerStunV1 {
        count: u16,
    },
    /// Zoraline's exact-successor counter placement, using a lifelink counter.
    FanaticReturnedLifelinkV1,
    SixBindMillV1,
    SixReturnLandV1,
    UrsineCombatV1,
}

impl C6bOpV1 {
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(self, Self::DiscardTwoV1 { .. } | Self::SixReturnLandV1)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C6bChoiceV1 {
    DiscardTwoV1 { cryomancer: bool },
    SixReturnLandV1,
}

pub(crate) fn permanent_card(card_def: u16) -> bool {
    CARD_DEFS.get(usize::from(card_def)).is_some_and(|card| {
        [
            CardType::Land,
            CardType::Creature,
            CardType::Artifact,
            CardType::Enchantment,
            CardType::Planeswalker,
        ]
        .into_iter()
        .any(|kind| card.has_type(kind))
    })
}

/// After the committed zone change, record the actual destination incarnation.
/// This must also observe entries while Night Nurse is in a library or hand.
pub(crate) fn record_graveyard_entry(state: &mut GameState, object: ObjectId, from: Zone) {
    let live = state.objects.get(object);
    if from == Zone::Graveyard
        || live.zone != Zone::Graveyard
        || live.v4.is_token
        || live.spell_copy_origin.is_some()
        || !permanent_card(live.card_def)
    {
        return;
    }
    if !state
        .objects
        .iter()
        .any(|(_, card)| CARD_DEFS[usize::from(card.card_def)].name == NURSE)
    {
        return;
    }
    let generation = live.zone_change_count;
    let now = TurnV1::now(state);
    // Entry history is needed even before the granting creature is drawn.
    let entries = &mut state
        .eternal_v1
        .get_or_insert_with(Default::default)
        .c6b
        .graveyard_entries;
    entries.retain(|(id, _, turn)| *turn == now && *id != object);
    entries.push((object, generation, now));
}

pub(crate) fn entered_graveyard_this_turn(state: &GameState, object: ObjectId) -> bool {
    state.objects.try_get(object).is_some_and(|live| {
        state.eternal_v1.as_ref().is_some_and(|eternal| {
            eternal.c6b.graveyard_entries.contains(&(
                object,
                live.zone_change_count,
                TurnV1::now(state),
            ))
        })
    })
}

/// Like Wrenn's emblem, this is also queried after the spell reaches the stack
/// while its costs are being assembled. The caller validates the origin zone.
pub(crate) fn six_retrace_permission(
    state: &GameState,
    object: ObjectId,
    holder: PlayerId,
) -> bool {
    if state.objects.try_get(object).is_some_and(|live| {
        live.zone == Zone::Stack
            && state.eternal_v1.as_ref().is_some_and(|eternal| {
                eternal
                    .c6b
                    .six_casts
                    .contains(&(object, live.zone_change_count, holder))
            })
    }) {
        return true;
    }
    state.active_player == holder
        && state.objects.try_get(object).is_some_and(|live| {
            live.owner == holder
                && !live.v4.is_token
                && live.spell_copy_origin.is_none()
                && permanent_card(live.card_def)
                && !CARD_DEFS[usize::from(live.card_def)].has_type(CardType::Land)
        })
        && crate::eternal_legacy_e9a_v1::active_permanents(state, SIX, Some(holder))
            .next()
            .is_some()
}

/// Freeze the announced stack incarnation as Underworld Breach does for escape.
/// Removing Six in response cannot invalidate a spell already cast legally.
pub(crate) fn note_six_cast(state: &mut GameState, object: ObjectId, player: PlayerId) {
    let live = state.objects.get(object);
    if live.zone != Zone::Graveyard || !six_retrace_permission(state, object, player) {
        return;
    }
    let Some(generation) = live.zone_change_count.checked_add(1) else {
        return;
    };
    let casts = &mut state
        .eternal_v1
        .get_or_insert_with(Default::default)
        .c6b
        .six_casts;
    casts.retain(|(id, _, _)| *id != object);
    casts.push((object, generation, player));
}

pub fn nurse_entry() -> EffectOp {
    EffectOp::Conditional {
        cond: crate::effect::EffectCond::TargetIsLegalForAbility {
            index: 0,
            spec: nurse_target(),
        },
        then: Box::new(EffectOp::MoveAllTargets {
            to_zone: Zone::Hand,
        }),
        else_: Box::new(EffectOp::Sequence(Vec::new())),
    }
}

fn nurse_target() -> TargetSpec {
    TargetSpec::StandardV1(StandardTargetV1::PermanentCardInOwnGraveyardThisTurnV1)
}

/// Native mill preserves private graveyard ordering. The final choice follows
/// only exact successor incarnations, including public replacement destinations.
pub fn six_attack() -> EffectOp {
    EffectOp::Sequence(vec![
        c6b(C6bOpV1::SixBindMillV1),
        EffectOp::MillCards {
            player: PlayerRef::Controller,
            count: 3,
        },
        c6b(C6bOpV1::SixReturnLandV1),
    ])
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

/// Kiora's mandatory loot shape, with a root draw so competing replacements
/// can suspend before the discard. The discard is the final instruction.
pub fn sphinx_loot() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        },
        crate::eternal_cube_c1a_v1::stop_if_won(),
        EffectOp::DiscardCards {
            player: PlayerRef::Controller,
            count: 1,
        },
    ])
}

/// Reuse Homunculus Horde's native current-or-departure copy operation.
/// A death trigger always binds the departed battlefield incarnation.
pub fn nontoken_death_copy() -> EffectOp {
    EffectOp::CreateTokenCopyOfSourceV1
}

pub fn fanatic_entry() -> EffectOp {
    EffectOp::Conditional {
        cond: crate::effect::EffectCond::TargetIsLegalForAbility {
            index: 0,
            spec: fanatic_target(),
        },
        then: Box::new(EffectOp::Sequence(vec![
            EffectOp::ReturnTargetPermanentToBattlefield { target_index: 0 },
            c6b(C6bOpV1::FanaticReturnedLifelinkV1),
        ])),
        else_: Box::new(EffectOp::Sequence(Vec::new())),
    }
}

fn fanatic_target() -> TargetSpec {
    TargetSpec::StandardV1(StandardTargetV1::UpToOneCreatureCardInOwnGraveyardV1)
}

/// Extend the source-copy departure recorder and root validator, without
/// selecting this creature's unrelated attack ability as a copy operation.
pub(crate) fn has_nontoken_death_copy(card_def: u16) -> bool {
    CARD_DEFS
        .get(usize::from(card_def))
        .is_some_and(|card| card.has_full_support() && card.name == SPHINX)
}

/// Token status survives zone changes and cannot change during a resolution.
/// Checking before publication also prevents a token from producing a stack
/// item, rather than merely making that item's resolution do nothing.
pub(crate) fn suppress_token_death_copy(
    name: &str,
    condition: TriggerCondition,
    is_token: bool,
) -> bool {
    name == SPHINX && is_token && condition == TriggerCondition::LeftBattlefieldToGraveyard
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
const FANATIC_TRIGGERS: [TriggeredAbilityDef; 1] = [etb(fanatic_entry)];
const NURSE_TRIGGERS: [TriggeredAbilityDef; 1] = [etb(nurse_entry)];
const SIX_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Attacks,
    ..etb(six_attack)
}];
const URSINE_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::BeginningOfControllerCombat,
    ..etb(ursine_combat)
}];

pub fn ursine_combat() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::MillCards {
            player: PlayerRef::Controller,
            count: 1,
        },
        c6b(C6bOpV1::UrsineCombatV1),
    ])
}
const SPHINX_TRIGGERS: [TriggeredAbilityDef; 2] = [
    TriggeredAbilityDef {
        condition: TriggerCondition::Attacks,
        ..etb(sphinx_loot)
    },
    TriggeredAbilityDef {
        condition: TriggerCondition::LeftBattlefieldToGraveyard,
        ..etb(nontoken_death_copy)
    },
];

pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        CRYOMANCER => &CRYOMANCER_TRIGGERS,
        PYROMANCER => &PYROMANCER_TRIGGERS,
        SPHINX => &SPHINX_TRIGGERS,
        FANATIC => &FANATIC_TRIGGERS,
        NURSE => &NURSE_TRIGGERS,
        SIX => &SIX_TRIGGERS,
        "Ursine Monstrosity" => &URSINE_TRIGGERS,
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
    if name == "Ursine Monstrosity" && *effect == ursine_combat() {
        return Some(TargetSpec::None);
    }
    if name == NURSE && *effect == nurse_entry() {
        return Some(nurse_target());
    }
    if name == SIX && *effect == six_attack() {
        return Some(TargetSpec::None);
    }
    if name == FANATIC && *effect == fanatic_entry() {
        return Some(fanatic_target());
    }
    if (name == CRYOMANCER && *effect == cryomancer_entry())
        || (name == PYROMANCER && *effect == pyromancer_entry())
        || (name == SPHINX && (*effect == sphinx_loot() || *effect == nontoken_death_copy()))
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
        C6bOpV1::UrsineCombatV1 => ursine::resolve(ctx, state),
        C6bOpV1::SixBindMillV1 => {
            if let Some(key) = ctx.stack_item_id {
                let cards = state.players[ctx.controller.index()]
                    .library
                    .iter()
                    .take(3)
                    .map(|&object| EffectObjectBinding {
                        object,
                        expected_zone: Zone::Library,
                        expected_zone_change_count: state.objects.get(object).zone_change_count,
                    })
                    .collect();
                let mills = &mut state
                    .eternal_v1
                    .get_or_insert_with(Default::default)
                    .c6b
                    .six_mills;
                mills.retain(|(item, _)| *item != key);
                mills.push((key, cards));
            }
        }
        C6bOpV1::FanaticReturnedLifelinkV1 => {
            let Some(crate::state::StackTargetContractV4::Object {
                object,
                zone: Zone::Graveyard,
                zone_change_count,
                ..
            }) = ctx.target_contracts.first().copied()
            else {
                return;
            };
            let Some(live) = state.objects.try_get(object) else {
                return;
            };
            if live.zone != Zone::Battlefield
                || zone_change_count.checked_add(1) != Some(live.zone_change_count)
            {
                return;
            }
            // The returned permanent need not still be a creature (devotion,
            // for example). The general counter helper also sets timestamps
            // and applies counter replacements on the actual recipient.
            let _ = crate::standard_creatures_v1::place_counters_checked_v1(
                state,
                object,
                ctx.controller,
                crate::standard_creatures_v1::CounterTransferV1 {
                    extras: crate::standard_creatures_v1::CounterExtrasV1 {
                        lifelink: 1,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            );
        }
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
        C6bOpV1::DiscardTwoV1 { .. } | C6bOpV1::SixReturnLandV1 => {}
    }
}

pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &C6bOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    if *op == C6bOpV1::SixReturnLandV1 {
        let key = continuation.resolving_item.v4.stack_item_id;
        let cards = state
            .eternal_v1
            .as_mut()
            .and_then(|eternal| {
                let mills = &mut eternal.c6b.six_mills;
                let index = mills.iter().position(|(item, _)| *item == key)?;
                Some(mills.remove(index).1)
            })
            .unwrap_or_default();
        let bindings: Vec<_> = cards
            .into_iter()
            .filter_map(|old| {
                let live = state.objects.try_get(old.object)?;
                (old.expected_zone_change_count.checked_add(1) == Some(live.zone_change_count)
                    && live.v4.face_down_v1.is_none()
                    && matches!(
                        live.zone,
                        Zone::Graveyard | Zone::Exile | Zone::Battlefield | Zone::Stack
                    )
                    && CARD_DEFS[usize::from(live.card_def)].has_type(CardType::Land))
                .then_some(EffectObjectBinding {
                    object: old.object,
                    expected_zone: live.zone,
                    expected_zone_change_count: live.zone_change_count,
                })
            })
            .collect();
        if bindings.is_empty() {
            return Ok(false);
        }
        let choice = E3ChoiceV1 {
            kind: E3ChoiceKindV1::C6bV1(C6bChoiceV1::SixReturnLandV1),
            player: continuation.ctx.controller,
            candidates: bindings.iter().map(|b| Target::Object(b.object)).collect(),
            bindings,
            min: 0,
            max: 1,
            ordered: false,
            path,
            remaining: continuation.frames.clone(),
        };
        crate::eternal_cards_v1::e3::offer(continuation, choice);
        return Ok(true);
    }
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
    match kind {
        C6bChoiceV1::DiscardTwoV1 { cryomancer } => {
            discard_selected(state, continuation, *cryomancer, &bindings);
        }
        C6bChoiceV1::SixReturnLandV1 => {
            if let Some(binding) = bindings.first() {
                if state.objects.try_get(binding.object).is_some_and(|live| {
                    live.zone == binding.expected_zone
                        && live.zone_change_count == binding.expected_zone_change_count
                }) {
                    crate::event::propose_and_commit(
                        state,
                        crate::event::ProposedEvent::zone_change(binding.object, Zone::Hand),
                    );
                }
            }
        }
    }
    Ok(false)
}
