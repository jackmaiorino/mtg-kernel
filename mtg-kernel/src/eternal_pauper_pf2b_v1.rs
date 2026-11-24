//! Card programs for the eternal catalog's Pauper field wave Pf2b
//! (`docs/design/eternal_formats_support_v1.md`): cycling, replicate,
//! graveyard multicolored triggers, changeling, suspend, adventure and omen.
//!
//! `eternal_cards_v1` forwards this module's effect leaves and triggers here,
//! and the recipe tables of `build_eternal_pf2b_v1.rs` name its programs. Every
//! table is keyed by a printed name that occurs only in the eternal registry,
//! so the Pauper, FDN and Standard catalogs never reach it. Behavior follows
//! each card's current Oracle text.
//!
//! Cards:
//! - Memory Leak: target opponent reveals their hand; choose a nonland card
//!   from that player's graveyard or hand and exile it; Cycling `{1}`.
//! - Gigadrowse: Replicate `{U}`; tap target permanent.
//! - Aurora Eidolon: `{W}, Sacrifice: Prevent the next 3 damage that would be
//!   dealt to any target this turn`; graveyard trigger on casting a
//!   multicolored spell.
//! - Sandstorm Eidolon: `{R}, Sacrifice: Target creature can't block this
//!   turn`; graveyard trigger on casting a multicolored spell.
//! - Rooftop Percher: Changeling, Flying; ETB exile up to two target cards
//!   from graveyards and gain 3 life.
//! - Rift Sower: `{T}: Add one mana of any color`; Suspend 2--`{G}` (handled
//!   through `eternal_legacy_e7a_v1::suspend` and `eternal_sideboard_s1e_v1`'s
//!   suspend haste).
//! - Pegasus Guardian // Rescue the Foal (+ Pegasus Token): Flying, end-step
//!   revolt Pegasus token trigger; Adventure instant flickering target
//!   creature you control under its owner's control.
//! - Stormshriek Feral // Flush Out: Flying, haste, firebreathing; Omen
//!   sorcery discarding a card to draw two, then shuffling into its owner's
//!   library.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{card_id_by_name, CardDef, TargetSpec, CARD_DEFS};
use crate::effect::{
    EffectContinuation, EffectFrame, EffectObjectBinding, EffectOp, ExecCtx, PlayerRef,
};
use crate::eternal_cards_v1::e3::{E3ChoiceKindV1, E3ChoiceV1};
use crate::eternal_cards_v1::{delayed_trigger, permanent_left_battlefield_this_turn, EternalOpV1};
use crate::eternal_legacy_e9a_v1::{active_named, live_link};
use crate::event::{self, CommittedEvent, DamageProposed, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::StandardOpV1;
use crate::state::{AbilitySourceContractV4, GameState, ObjectLinkV4, Target, Zone};
use crate::trigger::{PendingTrigger, TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

pub(crate) const AURORA_EIDOLON: &str = "Aurora Eidolon";
pub(crate) const SANDSTORM_EIDOLON: &str = "Sandstorm Eidolon";
pub(crate) const ROOFTOP_PERCHER: &str = "Rooftop Percher";
pub(crate) const RIFT_SOWER: &str = "Rift Sower";
pub(crate) const PEGASUS_GUARDIAN: &str = "Pegasus Guardian";
pub(crate) const PEGASUS_TOKEN: &str = "Pegasus Token";
pub(crate) const STORMSHRIEK_FERAL: &str = "Stormshriek Feral";

// ---- State: Aurora Eidolon shields --------------------------------------------------

/// "Prevent the next N damage that would be dealt to any target this turn."
/// Bound to a battlefield permanent's exact incarnation if targeting an object
/// (CR 400.7) and scoped to the current turn (round key + active player).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuroraShieldV1 {
    pub id: u64,
    pub target: Target,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object_link: Option<ObjectLinkV4>,
    pub remaining: i32,
    pub turn: u32,
    pub active: PlayerId,
}

/// Per-game state for wave Pf2b: Aurora Eidolon's finite prevention shields.
/// Default (and absent from snapshots and state hashes) until used.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pf2bStateV1 {
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub next_shield_id: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aurora_shields: Vec<AuroraShieldV1>,
}

fn is_zero_u64(value: &u64) -> bool {
    *value == 0
}

impl Pf2bStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        self.next_shield_id == 0 && self.aurora_shields.is_empty()
    }
}

impl std::hash::Hash for Pf2bStateV1 {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        if !self.is_empty() {
            b"eternal-pauper-pf2b/v1".hash(state);
            self.next_shield_id.hash(state);
            self.aurora_shields.hash(state);
        }
    }
}

fn pf2b_state(state: &mut GameState) -> &mut Pf2bStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).pf2b
}

fn read_state(state: &GameState) -> Option<&Pf2bStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.pf2b)
}

fn shield_matches(state: &GameState, shield: &AuroraShieldV1, damage: &DamageProposed) -> bool {
    if shield.remaining <= 0
        || damage.amount <= 0
        || shield.turn != state.turn_key_v1()
        || shield.active != state.active_player
        || shield.target != damage.target
    {
        return false;
    }
    match (shield.target, shield.object_link) {
        (Target::Player(_), None) => true,
        (Target::Object(id), Some(link)) => {
            id == link.object && link_live_in(state, link, Zone::Battlefield)
        }
        _ => false,
    }
}

/// Installs an Aurora Eidolon finite prevention shield for `amount` damage on
/// `target` until end of turn.
pub(crate) fn install_aurora_shield(state: &mut GameState, target: Target, amount: i32) {
    if amount <= 0 {
        return;
    }
    let object_link = match target {
        Target::Player(_) => None,
        Target::Object(id) => {
            let Some(live) = state.objects.try_get(id) else {
                return;
            };
            if live.zone != Zone::Battlefield {
                return;
            }
            Some(live_link(state, id))
        }
    };
    let turn = state.turn_key_v1();
    let active = state.active_player;
    let pf2b = pf2b_state(state);
    pf2b.aurora_shields
        .retain(|shield| shield.remaining > 0 && shield.turn == turn && shield.active == active);
    let id = pf2b.next_shield_id.saturating_add(1);
    pf2b.next_shield_id = id;
    pf2b.aurora_shields.push(AuroraShieldV1 {
        id,
        target,
        object_link,
        remaining: amount,
        turn,
        active,
    });
}

/// Live Aurora Eidolon shields matching `damage` (caller checks
/// `!cannot_prevent_damage`).
pub(crate) fn applicable_aurora_shields(
    state: &GameState,
    damage: &DamageProposed,
) -> Vec<AuroraShieldV1> {
    let Some(pf2b) = read_state(state) else {
        return Vec::new();
    };
    pf2b.aurora_shields
        .iter()
        .copied()
        .filter(|shield| shield_matches(state, shield, damage))
        .collect()
}

/// Whether any live Aurora Eidolon shield is active this turn.
pub(crate) fn has_live_aurora_shield(state: &GameState) -> bool {
    let Some(pf2b) = read_state(state) else {
        return false;
    };
    let turn = state.turn_key_v1();
    let active = state.active_player;
    pf2b.aurora_shields
        .iter()
        .any(|shield| shield.remaining > 0 && shield.turn == turn && shield.active == active)
}

/// Remaining prevention capacity of the Aurora Eidolon shield `shield_id`, if live.
pub(crate) fn aurora_shield_remaining(state: &GameState, shield_id: u64) -> Option<i32> {
    read_state(state)?
        .aurora_shields
        .iter()
        .find(|shield| shield.id == shield_id)
        .map(|shield| shield.remaining)
}

/// Applies the specific Aurora Eidolon shield `shield_id` to `damage`. Returns
/// `true` if `damage` was completely prevented.
pub(crate) fn apply_aurora_shield_by_id(
    state: &mut GameState,
    shield_id: u64,
    damage: &mut DamageProposed,
) -> bool {
    let Some(pf2b) = read_state(state) else {
        return false;
    };
    let Some(shield) = pf2b
        .aurora_shields
        .iter()
        .copied()
        .find(|s| s.id == shield_id)
    else {
        return false;
    };
    if !shield_matches(state, &shield, damage) {
        return false;
    }
    let prevented = shield.remaining.min(damage.amount);
    damage.amount -= prevented;
    if let Some(eternal) = state.eternal_v1.as_mut() {
        if let Some(slot) = eternal
            .pf2b
            .aurora_shields
            .iter_mut()
            .find(|s| s.id == shield_id)
        {
            slot.remaining -= prevented;
        }
        eternal.pf2b.aurora_shields.retain(|s| s.remaining > 0);
    }
    damage.amount <= 0
}

/// Direct prevention pass in `event.rs` (caller checks
/// `!damage_cannot_be_prevented`): consumes matching Aurora Eidolon shields in
/// installation order. Returns `true` if all damage was prevented.
pub(crate) fn aurora_prevents_damage(state: &mut GameState, damage: &mut DamageProposed) -> bool {
    let matching = applicable_aurora_shields(state, damage);
    for shield in matching {
        if apply_aurora_shield_by_id(state, shield.id, damage) {
            return true;
        }
    }
    false
}

/// Expires Aurora Eidolon's until-end-of-turn shields during cleanup.
pub(crate) fn cleanup_end_of_turn(state: &mut GameState) {
    if let Some(eternal) = state.eternal_v1.as_mut() {
        eternal.pf2b.aurora_shields.clear();
    }
}

/// Prunes expired Aurora Eidolon shields at turn boundaries.
pub(crate) fn begin_turn(state: &mut GameState, _player: PlayerId) {
    let turn = state.turn_key_v1();
    let active = state.active_player;
    if let Some(eternal) = state.eternal_v1.as_mut() {
        eternal
            .pf2b
            .aurora_shields
            .retain(|s| s.remaining > 0 && s.turn == turn && s.active == active);
    }
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::Pf2bV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Pf2bOpV1 {
    /// Memory Leak: target opponent reveals their hand; choose a nonland card
    /// from that player's graveyard or hand and exile it.
    MemoryLeakV1,
    /// Gigadrowse: tap target permanent.
    GigadrowseTapV1,
    /// Aurora Eidolon: prevent the next 3 damage that would be dealt to any
    /// target this turn.
    AuroraPreventThreeV1,
    /// Aurora/Sandstorm Eidolon graveyard trigger: offer to return this exact
    /// graveyard incarnation to its owner's hand.
    EidolonGraveyardReturnV1,
    /// Aurora/Sandstorm Eidolon accepted return: move the exact graveyard
    /// incarnation to its owner's hand.
    EidolonReturnMoveV1,
    /// Pegasus Guardian end-step intervening-if trigger: if a permanent you
    /// controlled left the battlefield this turn, create a 1/1 white Pegasus
    /// creature token with flying.
    PegasusGuardianTokenV1,
    /// Rescue the Foal (Pegasus Guardian's Adventure): exile target creature
    /// you control, then return that card to the battlefield under its owner's
    /// control.
    RescueTheFoalV1,
    /// Flush Out (Stormshriek Feral's Omen): discard a card; if you do, draw
    /// two cards.
    FlushOutV1,
}

impl Pf2bOpV1 {
    /// Whether this leaf may suspend for a player choice or stage a draw frame.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(
            self,
            Self::MemoryLeakV1 | Self::EidolonGraveyardReturnV1 | Self::FlushOutV1
        )
    }
}

/// Resolution choices owned by this module, carried as
/// `E3ChoiceKindV1::Pf2bV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Pf2bChoiceV1 {
    /// Memory Leak: choose one nonland card from the target opponent's
    /// revealed hand or graveyard to exile.
    MemoryLeakV1,
    /// Flush Out: choose one card in the controller's hand to discard.
    FlushOutDiscardV1,
}

impl Pf2bChoiceV1 {
    pub(crate) fn chooser_private(&self) -> bool {
        matches!(self, Self::FlushOutDiscardV1)
    }

    pub(crate) fn public_purpose(&self) -> crate::rl::TargetSelectionPurposeV4 {
        crate::rl::TargetSelectionPurposeV4::CardSelection
    }
}

fn pf2b(op: Pf2bOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::Pf2bV1(op)))
}

fn nothing() -> EffectOp {
    EffectOp::Sequence(Vec::new())
}

fn halt(state: &mut GameState, source: ObjectId) {
    state.engine.halted = Some((
        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
        source,
    ));
}

fn bind(state: &GameState, object: ObjectId) -> EffectObjectBinding {
    let live = state.objects.get(object);
    EffectObjectBinding {
        object,
        expected_zone: live.zone,
        expected_zone_change_count: live.zone_change_count,
    }
}

fn definition(state: &GameState, object: ObjectId) -> Option<&'static CardDef> {
    let live = state.objects.try_get(object)?;
    CARD_DEFS.get(live.card_def as usize)
}

fn link_live_in(state: &GameState, link: ObjectLinkV4, zone: Zone) -> bool {
    state
        .objects
        .try_get(link.object)
        .is_some_and(|live| live.zone == zone && live.zone_change_count == link.zone_change_count)
}

fn target_is_legal(ctx: &ExecCtx, state: &GameState, spec: TargetSpec, index: usize) -> bool {
    if !ctx.target_incarnation_matches(index, state) {
        return false;
    }
    if let Some(source) = ctx.ability_source_contract {
        source.source == ctx.source
            && source.controller == ctx.controller
            && crate::engine::effect_target_is_legal_from_ability_source(
                state,
                source,
                ctx.controller,
                spec,
                &ctx.targets,
                index,
            )
    } else {
        crate::engine::effect_target_is_legal(
            state,
            ctx.source,
            ctx.controller,
            spec,
            &ctx.targets,
            index,
        )
    }
}

fn offer(
    continuation: &mut EffectContinuation,
    kind: Pf2bChoiceV1,
    player: PlayerId,
    bindings: Vec<EffectObjectBinding>,
    (min, max): (u16, u16),
    path: Vec<u16>,
) {
    let remaining = continuation.frames.clone();
    crate::eternal_cards_v1::e3::offer(
        continuation,
        E3ChoiceV1 {
            kind: E3ChoiceKindV1::Pf2bV1(kind),
            player,
            candidates: bindings
                .iter()
                .map(|binding| Target::Object(binding.object))
                .collect(),
            bindings,
            min,
            max,
            ordered: false,
            path,
            remaining,
        },
    );
}

fn push_choice(
    continuation: &mut EffectContinuation,
    controller: PlayerRef,
    options: Vec<EffectOp>,
    path: Vec<u16>,
) {
    continuation.frames.push(EffectFrame::Program {
        op: EffectOp::Choice {
            controller,
            options,
        },
        path,
    });
}

fn token_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("eternal token missing: {name}"))
}

fn eidolon_in_own_graveyard(ctx: &ExecCtx, state: &GameState) -> Option<ObjectId> {
    let source = ctx.ability_source_contract?;
    let live = state.objects.try_get(source.source)?;
    (live.zone == Zone::Graveyard
        && live.zone_change_count == source.zone_change_count
        && live.owner == ctx.controller
        && !live.v4.is_token)
        .then_some(source.source)
}

/// Exile `object`, then return it to the battlefield under its owner's control
/// if it is a card still in exile (tokens cease to exist in exile).
fn flicker_under_owner(state: &mut GameState, object: ObjectId) {
    event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Exile));
    let live = state.objects.get(object);
    if live.zone != Zone::Exile || live.v4.is_token {
        return;
    }
    let link = live_link(state, object);
    if !link_live_in(state, link, Zone::Exile) {
        return;
    }
    let owner = state.objects.get(object).owner;
    event::propose_and_commit(
        state,
        ProposedEvent::zone_change_to_battlefield_under_controller(object, owner),
    );
}

// ---- Programs -----------------------------------------------------------------------

/// Memory Leak: "Target opponent reveals their hand. You choose a nonland card
/// from that player's graveyard or hand and exile it."
pub fn memory_leak() -> EffectOp {
    pf2b(Pf2bOpV1::MemoryLeakV1)
}

/// Gigadrowse: "Tap target permanent."
pub fn gigadrowse() -> EffectOp {
    pf2b(Pf2bOpV1::GigadrowseTapV1)
}

/// Aurora Eidolon: "{W}, Sacrifice this creature: Prevent the next 3 damage
/// that would be dealt to any target this turn."
pub fn aurora_eidolon_prevent() -> EffectOp {
    pf2b(Pf2bOpV1::AuroraPreventThreeV1)
}

/// Rooftop Percher ETB: "exile up to two target cards from graveyards. You
/// gain 3 life."
fn rooftop_percher_etb() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::MoveAllTargets {
            to_zone: Zone::Exile,
        },
        EffectOp::GainLife {
            player: PlayerRef::Controller,
            amount: 3,
        },
    ])
}

/// Rescue the Foal (Pegasus Guardian's Adventure): "Exile target creature you
/// control, then return that card to the battlefield under its owner's
/// control."
pub fn rescue_the_foal() -> EffectOp {
    pf2b(Pf2bOpV1::RescueTheFoalV1)
}

/// Flush Out (Stormshriek Feral's Omen): "Discard a card. If you do, draw two
/// cards."
pub fn flush_out() -> EffectOp {
    pf2b(Pf2bOpV1::FlushOutV1)
}

// ---- Triggered abilities ------------------------------------------------------------

const ROOFTOP_PERCHER_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::Etb,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: rooftop_percher_etb,
}];

fn own_triggers(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        ROOFTOP_PERCHER => &ROOFTOP_PERCHER_TRIGGERS,
        _ => &[],
    }
}

/// Triggered abilities of this module's cards, falling through to wave Pf2c.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match own_triggers(name) {
        [] => crate::eternal_pauper_pf2c_v1::triggers_for(name),
        triggers => triggers,
    }
}

/// Target specs of this module's printed and queued triggered abilities.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    if name == ROOFTOP_PERCHER && *effect == rooftop_percher_etb() {
        return Some(TargetSpec::UpToTwoCardsInGraveyards);
    }
    delayed_effect_matches(name, effect).then_some(TargetSpec::None)
}

/// Queued triggers owned by this module's cards: Aurora/Sandstorm Eidolon's
/// graveyard cast trigger and Pegasus Guardian's end-step revolt trigger.
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    let EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::Pf2bV1(op))) = effect else {
        return false;
    };
    match name {
        AURORA_EIDOLON | SANDSTORM_EIDOLON => matches!(op, Pf2bOpV1::EidolonGraveyardReturnV1),
        PEGASUS_GUARDIAN => matches!(op, Pf2bOpV1::PegasusGuardianTokenV1),
        _ => false,
    }
}

/// Queued triggers from committed events:
/// - Aurora Eidolon & Sandstorm Eidolon: "Whenever you cast a multicolored
///   spell, you may return this card from your graveyard to your hand."
/// - Pegasus Guardian: "At the beginning of your end step, if a permanent you
///   controlled left the battlefield this turn, create a 1/1 white Pegasus
///   creature token with flying."
pub(crate) fn delayed_triggers(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Vec<PendingTrigger> {
    if !cfg!(feature = "eternal-fixtures") {
        return Vec::new();
    }
    let mut triggers = Vec::new();
    for event in events {
        match *event {
            CommittedEvent::SpellCast { spell, controller } => {
                // Multicolored (CR 105.2b): two or more of the five colors WUBRG.
                const WUBRG_MASK: u8 = (1 << 5) - 1;
                if (crate::engine::object_color_mask(state, spell) & WUBRG_MASK).count_ones() < 2 {
                    continue;
                }
                let eidolons: Vec<ObjectId> = state.players[controller.index()]
                    .graveyard
                    .iter()
                    .copied()
                    .filter(|&card| {
                        let live = state.objects.get(card);
                        live.owner == controller
                            && !live.v4.is_token
                            && definition(state, card).is_some_and(|def| {
                                matches!(def.name, AURORA_EIDOLON | SANDSTORM_EIDOLON)
                                    && def.is_executable()
                            })
                    })
                    .collect();
                for card in eidolons {
                    let source = AbilitySourceContractV4::capture(state, card);
                    triggers.push(delayed_trigger(
                        source,
                        controller,
                        pf2b(Pf2bOpV1::EidolonGraveyardReturnV1),
                    ));
                }
            }
            CommittedEvent::BeginningEndStep { player } => {
                if !permanent_left_battlefield_this_turn(state, player) {
                    continue;
                }
                let guardians: Vec<ObjectId> = [PlayerId::P0, PlayerId::P1]
                    .into_iter()
                    .flat_map(|seat| state.players[seat.index()].battlefield.iter().copied())
                    .filter(|&permanent| {
                        state.objects.get(permanent).controller == player
                            && active_named(state, permanent, PEGASUS_GUARDIAN)
                    })
                    .collect();
                for guardian in guardians {
                    let source = AbilitySourceContractV4::capture(state, guardian);
                    triggers.push(delayed_trigger(
                        source,
                        player,
                        pf2b(Pf2bOpV1::PegasusGuardianTokenV1),
                    ));
                }
            }
            _ => {}
        }
    }
    triggers
}

// ---- Execution ----------------------------------------------------------------------

pub(crate) fn execute(op: &Pf2bOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        Pf2bOpV1::GigadrowseTapV1 => {
            if !target_is_legal(ctx, state, TargetSpec::AnyPermanent, 0) {
                return;
            }
            if let Some(Target::Object(object)) = ctx.targets.first().copied() {
                if state.objects.get(object).zone == Zone::Battlefield {
                    event::propose_and_commit(state, ProposedEvent::tap(object));
                }
            }
        }
        Pf2bOpV1::AuroraPreventThreeV1 => {
            if !target_is_legal(ctx, state, TargetSpec::AnyTarget, 0) {
                return;
            }
            let Some(&target) = ctx.targets.first() else {
                return;
            };
            install_aurora_shield(state, target, 3);
        }
        Pf2bOpV1::EidolonReturnMoveV1 => {
            let Some(card) = eidolon_in_own_graveyard(ctx, state) else {
                return;
            };
            event::propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Hand));
        }
        Pf2bOpV1::PegasusGuardianTokenV1 => {
            // Intervening-if recheck on resolution (CR 603.4).
            if !permanent_left_battlefield_this_turn(state, ctx.controller) {
                return;
            }
            crate::effect::execute(
                &EffectOp::CreateToken {
                    token_def: token_id(PEGASUS_TOKEN),
                    controller: PlayerRef::Controller,
                },
                ctx,
                state,
            );
        }
        Pf2bOpV1::RescueTheFoalV1 => {
            if !target_is_legal(ctx, state, TargetSpec::ControlledCreature, 0) {
                return;
            }
            if let Some(Target::Object(creature)) = ctx.targets.first().copied() {
                flicker_under_owner(state, creature);
            }
        }
        Pf2bOpV1::MemoryLeakV1 | Pf2bOpV1::EidolonGraveyardReturnV1 | Pf2bOpV1::FlushOutV1 => {
            halt(state, ctx.source);
        }
    }
}

pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &Pf2bOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    let controller = continuation.ctx.controller;
    match op {
        Pf2bOpV1::MemoryLeakV1 => {
            if !target_is_legal(&continuation.ctx, state, TargetSpec::Opponent, 0) {
                return Ok(false);
            }
            let Some(&Target::Player(opponent)) = continuation.ctx.targets.first() else {
                return Ok(false);
            };
            let hand = state.players[opponent.index()].hand.clone();
            for &card in &hand {
                // spellbench-public-reveal: hand card revealed by the card rules.
                state.reveal_distinct_hand_card(opponent.opponent(), opponent, card)?;
            }
            let mut candidates: Vec<EffectObjectBinding> = hand
                .into_iter()
                .filter(|&card| definition(state, card).is_some_and(|def| !def.is_land))
                .map(|card| bind(state, card))
                .collect();
            candidates.extend(
                state.players[opponent.index()]
                    .graveyard
                    .iter()
                    .copied()
                    .filter(|&card| {
                        let live = state.objects.get(card);
                        !live.v4.is_token && definition(state, card).is_some_and(|def| !def.is_land)
                    })
                    .map(|card| bind(state, card)),
            );
            if candidates.is_empty() {
                return Ok(false);
            }
            offer(
                continuation,
                Pf2bChoiceV1::MemoryLeakV1,
                controller,
                candidates,
                (1, 1),
                path,
            );
            Ok(true)
        }
        Pf2bOpV1::EidolonGraveyardReturnV1 => {
            if eidolon_in_own_graveyard(&continuation.ctx, state).is_none() {
                return Ok(false);
            }
            push_choice(
                continuation,
                PlayerRef::Controller,
                vec![pf2b(Pf2bOpV1::EidolonReturnMoveV1), nothing()],
                path,
            );
            Ok(false)
        }
        Pf2bOpV1::FlushOutV1 => {
            let hand: Vec<EffectObjectBinding> = state.players[controller.index()]
                .hand
                .iter()
                .map(|&card| bind(state, card))
                .collect();
            if hand.is_empty() {
                return Ok(false);
            }
            offer(
                continuation,
                Pf2bChoiceV1::FlushOutDiscardV1,
                controller,
                hand,
                (1, 1),
                path,
            );
            Ok(true)
        }
        other => {
            execute(other, &continuation.ctx, state);
            Ok(false)
        }
    }
}

pub(crate) fn finish_selection(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    choice: &E3ChoiceV1,
    kind: &Pf2bChoiceV1,
    bindings: Vec<EffectObjectBinding>,
) -> Result<bool, String> {
    match kind {
        Pf2bChoiceV1::MemoryLeakV1 => {
            if let Some(card) = bindings.first() {
                event::propose_and_commit(
                    state,
                    ProposedEvent::zone_change(card.object, Zone::Exile),
                );
            }
            Ok(false)
        }
        Pf2bChoiceV1::FlushOutDiscardV1 => {
            let Some(card) = bindings.first() else {
                return Err("Flush Out requires discarding a card".to_string());
            };
            let live = state.objects.get(card.object);
            if live.zone != Zone::Hand || live.owner != choice.player {
                return Err("Flush Out discard left its owner's hand".to_string());
            }
            crate::engine::commit_discarded_card(state, card.object);
            continuation.frames.push(EffectFrame::Program {
                op: EffectOp::DrawCards {
                    player: PlayerRef::Controller,
                    count: 2,
                },
                path: choice.path.clone(),
            });
            Ok(false)
        }
    }
}
