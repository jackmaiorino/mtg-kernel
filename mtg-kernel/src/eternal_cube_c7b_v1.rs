//! Card programs for the eternal catalog's Vintage Cube wave C7b
//! (`docs/design/eternal_formats_support_v1.md`): extended mana abilities, damage modifiers and Palace Jailer.
//!
//! `eternal_cards_v1` forwards this module's effect leaves here, and the
//! recipe tables of `build_eternal_c7b_v1.rs` name its programs. Every
//! table is keyed by a printed name that occurs only in the eternal registry,
//! so the Pauper, FDN and Standard catalogs never reach it. Behavior was read
//! from each card's current Oracle text and, where present, its XMage source.
//!
//! Where each card lives:
//!
//! - Orcish Lumberjack: primary `{T}, Sacrifice a Forest` for `RRR`/`GGG` is
//!   in `build_eternal_v1.rs` (`mana_ability_def_for`) and the mixed `RRG`/`RGG`
//!   bundles are in `build_eternal_e6a_v1.rs` (`additional_mana_abilities_for`),
//!   using `ManaAbilityCostDef::TapSelfSacrificeControlledWithSubtypeV1`.
//! - Urza, Lord High Artificer: ETB creates the existing `Karn Construct Token`;
//!   its mana ability uses `ManaAbilityCostDef::TapUntappedControlledArtifactV1`
//!   in `build_eternal_v1.rs`; `{5}` shuffles and exiles the top card with a
//!   free end-of-turn play permission (`UrzaShuffleExileTopPlayFreeV1`,
//!   reusing Golos's exile-and-play pattern).
//! - Vivi Ornitier: its once-per-turn, own-turn-only `{0}` mana ability uses
//!   `ManaAbilityCostDef::NoneOwnTurnOnlyV1` and
//!   `ManaAbilityAmountDef::SourcePowerAnyCombinationV1` in `build_eternal_v1.rs`,
//!   surfacing its U/R split through `entry_choice_decision`/`answer_entry_choice`
//!   without using the stack. Its noncreature-spell trigger adds a +1/+1 counter
//!   to Vivi and deals 1 damage to the opponent.
//! - Black Widow, Super Spy: Menace is a keyword row; combat damage exiles the
//!   opponent's library prefix through the first nonland card, then offers a
//!   choice between putting a +1/+1 counter on Black Widow or granting an
//!   end-of-turn `PlayPermission` with `any_mana_type: AnyManaTypeV1(true)`.
//! - Soul-Scar Mage: Prowess is Monastery Swiftspear's trigger; noncombat
//!   damage from a controlled source to an opponent's creature is replaced with
//!   that many -1/-1 counters in `replace_noncombat_damage`.
//! - Questing Beast: keywords are a keyword row; `blocker_allowed` rejects
//!   blockers with power 2 or less; `combat_damage_cannot_be_prevented` defeats
//!   prevention for combat damage from controlled creatures; `note_combat_damage`
//!   and `delayed_triggers` queue its targeted planeswalker damage trigger.
//! - Palace Jailer: two separate ETB triggers (`EffectOp::BecomeMonarch` and
//!   `PalaceJailerExileV1`). Linked exiles are tracked in `C7bStateV1` and
//!   returned immediately under their owner's control by `note_became_monarch`
//!   whenever an opponent of the Jailer's controller becomes the monarch.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{card_id_by_name, CardType, TargetSpec, CARD_DEFS};
use crate::effect::{EffectContinuation, EffectFrame, EffectOp, ExecCtx, PlayerRef, TargetRef};
use crate::engine::{
    AnyManaTypeV1, Decision, FreeCastV1, PlayOrCast, PlayPermission, PlayPermissionExpiry,
};
use crate::eternal_cards_v1::EternalOpV1;
use crate::event::{self, CommittedEvent, DamageProposed, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::standard_cards_v1::{StandardOpV1, StandardTargetV1};
use crate::state::{AbilitySourceContractV4, GameState, ObjectLinkV4, Target, Zone};
use crate::trigger::{PendingTrigger, TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

pub(crate) const URZA: &str = "Urza, Lord High Artificer";
pub(crate) const VIVI: &str = "Vivi Ornitier";
pub(crate) const BLACK_WIDOW: &str = "Black Widow, Super Spy";
pub(crate) const SOUL_SCAR_MAGE: &str = "Soul-Scar Mage";
pub(crate) const QUESTING_BEAST: &str = "Questing Beast";
pub(crate) const PALACE_JAILER: &str = "Palace Jailer";

const KARN_CONSTRUCT_TOKEN: &str = "Karn Construct Token";

/// A dynamic two-color combination mana ability (Vivi Ornitier) waiting for its
/// controller to choose how many mana of the second color to add.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingComboManaV1 {
    pub player: PlayerId,
    pub source: ObjectId,
    pub first: ManaColor,
    pub second: ManaColor,
    pub amount: u16,
    pub min_second: u16,
    pub max_second: u16,
}

/// Combat damage dealt by Questing Beast to an opponent, captured before SBAs
/// and drained into a targeted planeswalker-damage trigger on collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestingCombatV1 {
    pub source: AbilitySourceContractV4,
    pub controller: PlayerId,
    pub amount: i32,
}

/// A creature card exiled by Palace Jailer's ETB trigger, returning immediately
/// when an opponent of `jailer_controller` becomes the monarch (even if Palace
/// Jailer has already left the battlefield).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JailerExileV1 {
    pub jailer_controller: PlayerId,
    pub exiled: ObjectLinkV4,
}

/// Per-game state for this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Empty instances skip serialization and
/// hash zero bytes so unrelated games keep identical snapshots and hashes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct C7bStateV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_combo_mana: Option<PendingComboManaV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub questing_combat: Vec<QuestingCombatV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub jailer_exiles: Vec<JailerExileV1>,
}

impl C7bStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        self.pending_combo_mana.is_none()
            && self.questing_combat.is_empty()
            && self.jailer_exiles.is_empty()
    }
}

impl Hash for C7bStateV1 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        if let Some(pending) = self.pending_combo_mana {
            b"eternal-cube-c7b/pending-combo-mana/v1".hash(state);
            pending.hash(state);
        }
        if !self.questing_combat.is_empty() {
            b"eternal-cube-c7b/questing-combat/v1".hash(state);
            self.questing_combat.hash(state);
        }
        if !self.jailer_exiles.is_empty() {
            b"eternal-cube-c7b/jailer-exiles/v1".hash(state);
            self.jailer_exiles.hash(state);
        }
    }
}

fn read_state(state: &GameState) -> Option<&C7bStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.c7b)
}

fn c7b_state(state: &mut GameState) -> &mut C7bStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).c7b
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::C7bV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C7bOpV1 {
    /// Urza, Lord High Artificer: "{5}: Shuffle your library, then exile the
    /// top card. Until end of turn, you may play that card without paying its
    /// mana cost."
    UrzaShuffleExileTopPlayFreeV1,
    /// Black Widow, Super Spy: exile from opponent's library until a nonland
    /// card is exiled, then choose between putting a +1/+1 counter on Black
    /// Widow or casting the exiled nonland card until end of turn with mana of
    /// any type.
    BlackWidowTriggerV1,
    /// Accepted +1/+1 counter half of Black Widow's trigger.
    BlackWidowCounterV1,
    /// Declined counter half of Black Widow's trigger: grant cast permission
    /// with `any_mana_type` for the exiled nonland card.
    BlackWidowGrantCastV1 {
        card: ObjectId,
        zone_change_count: u32,
    },
    /// Questing Beast: deals `amount` damage to target planeswalker the damaged
    /// opponent controls.
    QuestingBeastDamagePlaneswalkerV1 { amount: i32 },
    /// Palace Jailer: exile target creature an opponent controls until an
    /// opponent becomes the monarch.
    PalaceJailerExileV1,
}

impl C7bOpV1 {
    /// Whether the leaf may stop for a player's choice (or a draw that a
    /// replacement can interrupt), so it runs in the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(self, Self::BlackWidowTriggerV1)
    }
}

fn c7b(op: C7bOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C7bV1(op)))
}

const fn trigger(condition: TriggerCondition, effect: fn() -> EffectOp) -> TriggeredAbilityDef {
    TriggeredAbilityDef {
        condition,
        face_index: 0,
        effect,
    }
}

// ---- Programs -----------------------------------------------------------------------

/// Urza, Lord High Artificer: "{5}: Shuffle your library, then exile the top
/// card. Until end of turn, you may play that card without paying its mana
/// cost."
pub fn urza_exile() -> EffectOp {
    c7b(C7bOpV1::UrzaShuffleExileTopPlayFreeV1)
}

/// Urza, Lord High Artificer ETB: create a 0/0 colorless Construct artifact
/// creature token with "+1/+1 for each artifact you control" (Karn Construct
/// Token).
fn urza_construct() -> EffectOp {
    EffectOp::CreateToken {
        token_def: card_id_by_name(KARN_CONSTRUCT_TOKEN)
            .expect("Karn Construct Token in CARD_DEFS"),
        controller: PlayerRef::Controller,
    }
}

/// Vivi Ornitier: "Whenever you cast a noncreature spell, put a +1/+1 counter
/// on Vivi Ornitier and it deals 1 damage to each opponent."
fn vivi_noncreature() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::AddPlusOneCounterToAbilitySource,
        EffectOp::DealDamage {
            target: TargetRef::Opponent,
            amount: 1,
        },
    ])
}

/// Black Widow, Super Spy: combat damage trigger.
fn black_widow_trigger() -> EffectOp {
    c7b(C7bOpV1::BlackWidowTriggerV1)
}

/// Soul-Scar Mage: Prowess (Monastery Swiftspear's trigger).
fn prowess() -> EffectOp {
    EffectOp::BindTemporaryBoostToTriggerSource {
        power: 1,
        toughness: 1,
    }
}

/// Palace Jailer: "When this creature enters, you become the monarch."
fn become_monarch() -> EffectOp {
    EffectOp::BecomeMonarch
}

/// Palace Jailer: "When this creature enters, exile target creature an opponent
/// controls until an opponent becomes the monarch."
fn palace_jailer_exile() -> EffectOp {
    c7b(C7bOpV1::PalaceJailerExileV1)
}

fn questing_beast_damage_planeswalker(amount: i32) -> EffectOp {
    c7b(C7bOpV1::QuestingBeastDamagePlaneswalkerV1 { amount })
}

// ---- Triggered abilities ------------------------------------------------------------

const URZA_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, urza_construct)];

const VIVI_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::CastNoncreatureSpell,
    vivi_noncreature,
)];

const BLACK_WIDOW_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::DealsCombatDamageToPlayer,
    black_widow_trigger,
)];

const SOUL_SCAR_MAGE_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::CastNoncreatureSpell, prowess)];

const PALACE_JAILER_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, become_monarch),
    trigger(TriggerCondition::Etb, palace_jailer_exile),
];

/// Triggered abilities of this module's cards, falling through to wave
/// C7c.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        URZA => &URZA_TRIGGERS,
        VIVI => &VIVI_TRIGGERS,
        BLACK_WIDOW => &BLACK_WIDOW_TRIGGERS,
        SOUL_SCAR_MAGE => &SOUL_SCAR_MAGE_TRIGGERS,
        PALACE_JAILER => &PALACE_JAILER_TRIGGERS,
        _ => crate::eternal_cube_c7c_v1::triggers_for(name),
    }
}

fn owns_printed_triggers(name: &str) -> bool {
    matches!(
        name,
        URZA | VIVI | BLACK_WIDOW | SOUL_SCAR_MAGE | PALACE_JAILER
    )
}

/// Target specs of this module's printed and queued triggered abilities.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    match name {
        PALACE_JAILER if *effect == palace_jailer_exile() => {
            Some(TargetSpec::OpponentControlledCreature)
        }
        QUESTING_BEAST
            if matches!(
                effect,
                EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C7bV1(
                    C7bOpV1::QuestingBeastDamagePlaneswalkerV1 { .. }
                )))
            ) =>
        {
            Some(TargetSpec::StandardV1(
                StandardTargetV1::OpponentPlaneswalkerV1,
            ))
        }
        _ if owns_printed_triggers(name)
            && triggers_for(name).iter().any(|ability| {
                crate::trigger::source_bound_trigger_program_matches(&(ability.effect)(), effect)
            }) =>
        {
            Some(TargetSpec::None)
        }
        _ => delayed_effect_matches(name, effect).then_some(TargetSpec::None),
    }
}

/// Queued triggers owned by this module's cards.
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    matches!(
        (name, effect),
        (
            QUESTING_BEAST,
            EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C7bV1(
                C7bOpV1::QuestingBeastDamagePlaneswalkerV1 { .. }
            )))
        )
    )
}

// ---- Hooks: Vivi Ornitier dynamic combination mana ----------------------------------

fn combo_mana_bounds(
    state: &GameState,
    player: PlayerId,
    source: ObjectId,
    first: ManaColor,
    second: ManaColor,
) -> Option<(u16, u16, u16)> {
    let power = crate::engine::effective_power(state, source);
    if power <= 0 {
        return Some((0, 0, 0));
    }
    let amount = u16::try_from(power).ok()?;
    let pool = state.players[player.index()].mana_pool;
    let room_first = u16::from(u8::MAX - pool[first.pool_index()]);
    let room_second = u16::from(u8::MAX - pool[second.pool_index()]);
    if first == second {
        return (amount <= room_first).then_some((amount, 0, 0));
    }
    let min_second = amount.saturating_sub(room_first);
    let max_second = amount.min(room_second);
    (min_second <= max_second).then_some((amount, min_second, max_second))
}

fn combo_colors(
    first: ManaColor,
    second: ManaColor,
    amount: u16,
    second_count: u16,
) -> Vec<ManaColor> {
    let first_count = usize::from(amount.saturating_sub(second_count));
    let second_count = usize::from(second_count);
    let mut colors = Vec::with_capacity(first_count + second_count);
    colors.extend(std::iter::repeat_n(first, first_count));
    colors.extend(std::iter::repeat_n(second, second_count));
    colors
}

/// Whether `source`'s `SourcePowerAnyCombinationV1` mana ability has at least
/// one split that fits in `player`'s mana pool (including X = 0).
pub(crate) fn combo_mana_available(
    state: &GameState,
    player: PlayerId,
    source: ObjectId,
    first: ManaColor,
    second: ManaColor,
) -> bool {
    combo_mana_bounds(state, player, source, first, second).is_some()
}

/// Starts (or finishes immediately when X = 0 or only one split fits) a
/// `SourcePowerAnyCombinationV1` mana ability without using the stack.
pub(crate) fn begin_combo_mana(
    state: &mut GameState,
    player: PlayerId,
    source: ObjectId,
    first: ManaColor,
    second: ManaColor,
) {
    let Some((amount, min_second, max_second)) =
        combo_mana_bounds(state, player, source, first, second)
    else {
        return;
    };
    if amount == 0 {
        return;
    }
    if min_second == max_second {
        let colors = combo_colors(first, second, amount, min_second);
        event::propose_and_commit(state, ProposedEvent::mana_add(player, colors));
        return;
    }
    c7b_state(state).pending_combo_mana = Some(PendingComboManaV1 {
        player,
        source,
        first,
        second,
        amount,
        min_second,
        max_second,
    });
}

/// Surfaces the pending U/R mana combination choice for Vivi Ornitier.
pub(crate) fn entry_choice_decision(state: &GameState) -> Option<Decision> {
    let pending = read_state(state)?.pending_combo_mana?;
    let option_count = pending
        .max_second
        .checked_sub(pending.min_second)?
        .checked_add(1)?;
    Some(Decision::ChooseEffectOption {
        player: pending.player,
        source: pending.source,
        option_count,
    })
}

/// Applies the player's chosen combination for Vivi Ornitier's mana ability.
/// Option `k` adds `(amount - second_count)` of `first` and `second_count` of
/// `second`, where `second_count = min_second + k`.
pub(crate) fn answer_entry_choice(state: &mut GameState, option: u16) -> Result<bool, String> {
    let Some(pending) = read_state(state).and_then(|c7b| c7b.pending_combo_mana) else {
        return Ok(false);
    };
    let max_option = pending.max_second.saturating_sub(pending.min_second);
    if option > max_option {
        return Err("mana combination option is out of range".into());
    }
    c7b_state(state).pending_combo_mana = None;
    let second_count = pending.min_second + option;
    let colors = combo_colors(pending.first, pending.second, pending.amount, second_count);
    event::propose_and_commit(state, ProposedEvent::mana_add(pending.player, colors));
    Ok(true)
}

// ---- Hooks: Soul-Scar Mage ----------------------------------------------------------

/// Soul-Scar Mage: "If a source you control would deal noncombat damage to a
/// creature an opponent controls, put that many -1/-1 counters on that creature
/// instead." Returns `true` when the damage event was replaced and suppressed.
pub(crate) fn replace_noncombat_damage(state: &mut GameState, damage: &DamageProposed) -> bool {
    if damage.is_combat || damage.amount <= 0 {
        return false;
    }
    let Target::Object(victim) = damage.target else {
        return false;
    };
    let Some(victim_obj) = state.objects.try_get(victim) else {
        return false;
    };
    if victim_obj.zone != Zone::Battlefield
        || !crate::engine::object_has_type(state, victim, CardType::Creature)
    {
        return false;
    }
    let victim_controller = victim_obj.controller;
    let source_controller = match damage.historical_facts(state) {
        Ok(Some(facts)) => facts.source.controller,
        Ok(None) => {
            let Some(source) = state.objects.try_get(damage.source) else {
                return false;
            };
            source.controller
        }
        Err(_) => return false,
    };
    if victim_controller != source_controller.opponent() {
        return false;
    }
    if crate::eternal_e5a_v1::active_permanents(state, SOUL_SCAR_MAGE, Some(source_controller))
        .next()
        .is_none()
    {
        return false;
    }
    // 614.6 + Melira/Solemnity: the damage is still replaced even if -1/-1
    // counters cannot be placed on the creature; zero counters are placed.
    if crate::eternal_sideboard_s1n_v1::prevents_minus1_minus1_counters_on(state, victim) {
        return true;
    }
    let placed = if !crate::doubling_season_v48::requires_successor(state) {
        damage.amount
    } else {
        match crate::doubling_season_v48::counter_amount(
            state,
            victim,
            source_controller,
            crate::doubling_season_v48::CounterOriginV48::Effect,
            damage.amount,
        ) {
            Ok(count) => count,
            Err(_) => {
                state.engine.halted = Some((
                    crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                    damage.source,
                ));
                return true;
            }
        }
    };
    let Some(next) = i16::try_from(placed).ok().and_then(|delta| {
        state
            .objects
            .get(victim)
            .counters
            .minus1_minus1
            .checked_add(delta)
    }) else {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            damage.source,
        ));
        return true;
    };
    state.objects.get_mut(victim).counters.minus1_minus1 = next;
    true
}

// ---- Hooks: Questing Beast ----------------------------------------------------------

/// Questing Beast: "Questing Beast can't be blocked by creatures with power 2
/// or less."
pub(crate) fn blocker_allowed(state: &GameState, attacker: ObjectId, blocker: ObjectId) -> bool {
    if !cfg!(feature = "eternal-fixtures")
        || !crate::eternal_e5a_v1::active_named(state, attacker, QUESTING_BEAST)
    {
        return true;
    }
    crate::engine::effective_power(state, blocker) > 2
}

/// Questing Beast: "Combat damage that would be dealt by creatures you control
/// can't be prevented."
pub(crate) fn combat_damage_cannot_be_prevented(
    state: &GameState,
    damage: &DamageProposed,
) -> bool {
    if !cfg!(feature = "eternal-fixtures") || !damage.is_combat {
        return false;
    }
    let Some(source) = state.objects.try_get(damage.source) else {
        return false;
    };
    if source.zone != Zone::Battlefield
        || !crate::engine::object_has_type(state, damage.source, CardType::Creature)
    {
        return false;
    }
    crate::eternal_e5a_v1::active_permanents(state, QUESTING_BEAST, Some(source.controller))
        .next()
        .is_some()
}

/// Captures Questing Beast's combat damage to an opponent before SBAs run so
/// the planeswalker-damage trigger still fires even if Questing Beast dies in
/// combat.
pub(crate) fn note_combat_damage(state: &mut GameState, events: &[CommittedEvent]) {
    let mut hits = Vec::new();
    for event in events {
        let CommittedEvent::Damage {
            source,
            target: Target::Player(player),
            amount,
        } = *event
        else {
            continue;
        };
        if amount <= 0 || !crate::eternal_e5a_v1::active_named(state, source, QUESTING_BEAST) {
            continue;
        }
        let controller = state.objects.get(source).controller;
        if player != controller.opponent() {
            continue;
        }
        hits.push(QuestingCombatV1 {
            source: AbilitySourceContractV4::capture(state, source),
            controller,
            amount,
        });
    }
    if !hits.is_empty() {
        c7b_state(state).questing_combat.extend(hits);
    }
}

/// Drains captured Questing Beast combat-damage events into targeted pending
/// triggers.
pub(crate) fn delayed_triggers(
    state: &mut GameState,
    _events: &[CommittedEvent],
) -> Vec<PendingTrigger> {
    if !cfg!(feature = "eternal-fixtures") {
        return Vec::new();
    }
    let queued = match state.eternal_v1.as_mut() {
        Some(eternal) if !eternal.c7b.questing_combat.is_empty() => {
            std::mem::take(&mut eternal.c7b.questing_combat)
        }
        _ => return Vec::new(),
    };
    queued
        .into_iter()
        .map(|entry| {
            let mut pending = crate::eternal_cards_v1::delayed_trigger(
                entry.source,
                entry.controller,
                questing_beast_damage_planeswalker(entry.amount),
            );
            pending.target_spec = TargetSpec::StandardV1(StandardTargetV1::OpponentPlaneswalkerV1);
            pending
        })
        .collect()
}

// ---- Hooks: Palace Jailer -----------------------------------------------------------

/// Called whenever a player actually becomes the monarch (i.e., `state.monarch`
/// changes to `Some(new_monarch)` from a different value). Returns every
/// creature exiled by an opponent's Palace Jailer immediately under its owner's
/// control without using the stack (610.3).
pub(crate) fn note_became_monarch(state: &mut GameState, new_monarch: PlayerId) {
    let Some(eternal) = state.eternal_v1.as_mut() else {
        return;
    };
    if eternal.c7b.jailer_exiles.is_empty() {
        return;
    }
    let mut released = Vec::new();
    let mut kept = Vec::new();
    for entry in std::mem::take(&mut eternal.c7b.jailer_exiles) {
        if entry.jailer_controller != new_monarch {
            released.push(entry.exiled);
        } else {
            kept.push(entry);
        }
    }
    eternal.c7b.jailer_exiles = kept;
    if released.is_empty() {
        return;
    }
    let returning: Vec<ProposedEvent> = released
        .into_iter()
        .filter(|link| {
            state.objects.try_get(link.object).is_some_and(|live| {
                live.zone == Zone::Exile
                    && live.zone_change_count == link.zone_change_count
                    && !live.v4.is_token
            })
        })
        .map(|link| ProposedEvent::zone_change(link.object, Zone::Battlefield))
        .collect();
    if !returning.is_empty() {
        event::propose_and_commit_batch(state, returning);
    }
}

// ---- Execution ----------------------------------------------------------------------

fn run_urza_exile(ctx: &ExecCtx, state: &mut GameState) {
    let player = ctx.controller;
    if state.shuffle_library(player).is_err() {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            ctx.source,
        ));
        return;
    }
    let Some(card) = state.players[player.index()].library.first().copied() else {
        return;
    };
    event::propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Exile));
    let Some(live) = state.objects.try_get(card) else {
        return;
    };
    if live.zone != Zone::Exile {
        return;
    }
    let zone_change_generation = live.zone_change_count;
    let def = &CARD_DEFS[usize::from(live.card_def)];
    let play_or_cast = if def.is_playable_land() {
        PlayOrCast::Play
    } else if def.is_castable() {
        PlayOrCast::Cast
    } else {
        return;
    };
    state.engine.exile_play_permissions.push(PlayPermission {
        object: card,
        holder: player,
        zone_change_generation,
        play_or_cast,
        expiry: PlayPermissionExpiry::EndOfTurn,
        without_mana_cost: FreeCastV1(true),
        any_mana_type: Default::default(),
    });
}

fn grant_black_widow_cast(
    state: &mut GameState,
    holder: PlayerId,
    card: ObjectId,
    zone_change_count: u32,
) {
    let Some(live) = state.objects.try_get(card) else {
        return;
    };
    if live.zone != Zone::Exile || live.zone_change_count != zone_change_count {
        return;
    }
    let Some(def) = CARD_DEFS.get(usize::from(live.card_def)) else {
        return;
    };
    if !def.is_castable() {
        return;
    }
    state.engine.exile_play_permissions.push(PlayPermission {
        object: card,
        holder,
        zone_change_generation: zone_change_count,
        play_or_cast: PlayOrCast::Cast,
        expiry: PlayPermissionExpiry::EndOfTurn,
        without_mana_cost: FreeCastV1(false),
        any_mana_type: AnyManaTypeV1(true),
    });
}

fn run_black_widow_counter(ctx: &ExecCtx, state: &mut GameState) {
    let Some(contract) = ctx.ability_source_contract else {
        return;
    };
    if state.objects.try_get(contract.source).is_some_and(|live| {
        live.zone == Zone::Battlefield && live.zone_change_count == contract.zone_change_count
    }) && event::add_plus_one_counters(state, contract.source, ctx.controller, 1).is_err()
    {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            ctx.source,
        ));
    }
}

fn run_questing_beast_damage(ctx: &ExecCtx, state: &mut GameState, amount: i32) {
    if amount <= 0 {
        return;
    }
    let Some(Target::Object(planeswalker)) = ctx.targets.first().copied() else {
        return;
    };
    if !ctx.target_incarnation_matches(0, state) {
        return;
    }
    let Some(live) = state.objects.try_get(planeswalker) else {
        return;
    };
    if live.zone != Zone::Battlefield
        || live.controller != ctx.controller.opponent()
        || !crate::engine::object_has_type(state, planeswalker, CardType::Planeswalker)
    {
        return;
    }
    crate::effect::execute(
        &EffectOp::DealDamage {
            target: TargetRef::Target(0),
            amount,
        },
        ctx,
        state,
    );
}

fn run_palace_jailer_exile(ctx: &ExecCtx, state: &mut GameState) {
    let Some(Target::Object(victim)) = ctx.targets.first().copied() else {
        return;
    };
    if !ctx.target_incarnation_matches(0, state) {
        return;
    }
    let Some(live) = state.objects.try_get(victim) else {
        return;
    };
    if live.zone != Zone::Battlefield
        || live.controller != ctx.controller.opponent()
        || !crate::engine::object_has_type(state, victim, CardType::Creature)
    {
        return;
    }
    let is_token = live.v4.is_token;
    event::propose_and_commit(state, ProposedEvent::zone_change(victim, Zone::Exile));
    if is_token {
        return;
    }
    let Some(exiled) = state.objects.try_get(victim) else {
        return;
    };
    if exiled.zone != Zone::Exile {
        return;
    }
    let link = ObjectLinkV4 {
        object: victim,
        zone_change_count: exiled.zone_change_count,
    };
    c7b_state(state).jailer_exiles.push(JailerExileV1 {
        jailer_controller: ctx.controller,
        exiled: link,
    });
}

/// Runs one of this module's leaves in synchronous execution.
pub(crate) fn execute(op: &C7bOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        C7bOpV1::UrzaShuffleExileTopPlayFreeV1 => run_urza_exile(ctx, state),
        // Asks a player, so it runs in `drive`.
        C7bOpV1::BlackWidowTriggerV1 => {}
        C7bOpV1::BlackWidowCounterV1 => run_black_widow_counter(ctx, state),
        C7bOpV1::BlackWidowGrantCastV1 {
            card,
            zone_change_count,
        } => grant_black_widow_cast(state, ctx.controller, *card, *zone_change_count),
        C7bOpV1::QuestingBeastDamagePlaneswalkerV1 { amount } => {
            run_questing_beast_damage(ctx, state, *amount)
        }
        C7bOpV1::PalaceJailerExileV1 => run_palace_jailer_exile(ctx, state),
    }
}

/// Runs one of this module's leaves in the resumable interpreter.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &C7bOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    match op {
        C7bOpV1::BlackWidowTriggerV1 => {
            let controller = continuation.ctx.controller;
            let defender = controller.opponent();
            let mut prefix = Vec::new();
            let mut nonland = None;
            for &card in &state.players[defender.index()].library {
                prefix.push(card);
                let def = &CARD_DEFS[usize::from(state.objects.get(card).card_def)];
                if !def.is_land {
                    nonland = Some(card);
                    break;
                }
            }
            if !prefix.is_empty() {
                event::propose_and_commit_batch(
                    state,
                    prefix
                        .iter()
                        .map(|&card| ProposedEvent::zone_change(card, Zone::Exile))
                        .collect(),
                );
            }
            let castable_exiled = nonland.and_then(|card| {
                let live = state.objects.try_get(card)?;
                let def = CARD_DEFS.get(usize::from(live.card_def))?;
                (live.zone == Zone::Exile && def.is_castable())
                    .then_some((card, live.zone_change_count))
            });
            let source_live = continuation
                .ctx
                .ability_source_contract
                .is_some_and(|contract| {
                    state.objects.try_get(contract.source).is_some_and(|live| {
                        live.zone == Zone::Battlefield
                            && live.zone_change_count == contract.zone_change_count
                    })
                });
            if source_live {
                let decline_op = match castable_exiled {
                    Some((card, zone_change_count)) => c7b(C7bOpV1::BlackWidowGrantCastV1 {
                        card,
                        zone_change_count,
                    }),
                    None => EffectOp::Sequence(Vec::new()),
                };
                continuation.frames.push(EffectFrame::Program {
                    op: EffectOp::Choice {
                        controller: PlayerRef::Controller,
                        options: vec![c7b(C7bOpV1::BlackWidowCounterV1), decline_op],
                    },
                    path,
                });
            } else if let Some((card, zone_change_count)) = castable_exiled {
                grant_black_widow_cast(state, controller, card, zone_change_count);
            }
            Ok(false)
        }
        other => {
            execute(other, &continuation.ctx, state);
            Ok(false)
        }
    }
}
