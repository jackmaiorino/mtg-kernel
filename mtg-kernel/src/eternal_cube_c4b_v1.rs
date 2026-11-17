//! Card programs for the eternal catalog's Vintage Cube wave C4b
//! (`docs/design/eternal_formats_support_v1.md`): magecraft, per-turn resolution counters and cast triggers.
//!
//! `eternal_cards_v1` forwards this module's effect leaves here, and the
//! recipe tables of `build_eternal_c4b_v1.rs` name its programs. Every
//! table is keyed by a printed name that occurs only in the eternal registry,
//! so the Pauper, FDN and Standard catalogs never reach it. Behavior was read
//! from each card's current Oracle text and, where present, its XMage source.
//!
//! Where each card lives:
//!
//! - Sedgemoor Witch and Witherbloom Apprentice: the "cast" half of magecraft
//!   is `TriggerCondition::CastInstantOrSorcery` (Archmage of Runes). The
//!   "copy" half is `delayed_triggers`: it notices a spell copy of an instant
//!   or sorcery that appeared on the stack under a player's control (a copy is
//!   not cast, so no cast event exists) and queues the same program once per
//!   magecraft permanent that player controls. `C4bStateV1::reported_copies`
//!   remembers which copies were already reported. Menace is a keyword row and
//!   Ward-Pay 3 life is `ward_cost_for` (Hexing Squelcher's Ward-Pay 2 life).
//! - Omnath, Locus of Creation and Scythecat Cub: Landfall is
//!   `TriggerCondition::ControlledLandEnters`; "the first, second, third time
//!   this ability has resolved this turn" is `bump_resolution`, a per-turn
//!   count keyed by the ability's source incarnation in `C4bStateV1`. Only a
//!   resolution counts (a countered or fizzled trigger never runs its leaf).
//! - Pompous Battlemage: Prowess is Monastery Swiftspear's trigger; "enters
//!   prepared" is Carnivorous Cultivator's (`eternal_vintage_e7b_v1::prepare`,
//!   which chooses the printing by creature); Improvised Act is Pursue the
//!   Past's may-discard-then-draw.
//! - Sage of the Skies: the cast trigger is `C4bTriggerV1::
//!   CastSelfAfterAnotherSpellV1` (Wastescape Battlemage's kicked cast
//!   trigger shape, functioning on the stack; its intervening "if you've cast
//!   another spell this turn" is read when the cast is collected and again as
//!   the trigger resolves); the copy is `engine::copy_spell_for`, and a
//!   resolving permanent spell copy becomes a token (707.10f, in `event.rs`).
//! - Okoye, Mighty and Adored: the entry trigger is Palace Sentinels'; the
//!   combat trigger puts the counter and marks the creature in `C4bStateV1`;
//!   `delayed_triggers` turns a marked creature's attack declaration against
//!   the monarch into a grant of double strike and trample.
//! - Ouroboroid: the combat trigger reads its power as it resolves (the power
//!   of a departed Ouroboroid is last-known information, `note_departure`).
//! - Screaming Nemesis: Haste is a keyword row; "whenever this creature is
//!   dealt damage" is Jackal Pup's damage hook, read from the committed damage
//!   events; "any other target" is `other_target_only` (a filter in
//!   `engine::legal_targets_for_controller_from_source_unframed`); "can't gain
//!   life" is a per-player flag read by `cannot_gain_life`, called from the
//!   life-gain commit (Sulfuric Vortex's hook).
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{CardType, Keywords, TargetSpec, CARD_DEFS};
use crate::effect::{
    EffectContinuation, EffectObjectBinding, EffectOp, ExecCtx, PlayerRef, TargetRef,
};
use crate::eternal_cards_v1::{EternalOpV1, EternalTriggerV1};
use crate::event::{self, CommittedEvent};
use crate::ids::{ObjectId, PlayerId, StackItemId};
use crate::mana::ManaColor;
use crate::standard_cards_v1::{StandardOpV1, StandardTriggerV1};
use crate::state::{AbilitySourceContractV4, GameState, StackItemKind, Target, Zone};
use crate::trigger::{PendingTrigger, TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

pub(crate) const SEDGEMOOR_WITCH: &str = "Sedgemoor Witch";
pub(crate) const WITHERBLOOM_APPRENTICE: &str = "Witherbloom Apprentice";
pub(crate) const OMNATH: &str = "Omnath, Locus of Creation";
pub(crate) const SCYTHECAT_CUB: &str = "Scythecat Cub";
pub(crate) const POMPOUS_BATTLEMAGE: &str = "Pompous Battlemage";
pub(crate) const SAGE_OF_THE_SKIES: &str = "Sage of the Skies";
pub(crate) const OKOYE: &str = "Okoye, Mighty and Adored";
pub(crate) const OUROBOROID: &str = "Ouroboroid";
pub(crate) const SCREAMING_NEMESIS: &str = "Screaming Nemesis";

const PEST_TOKEN: &str = "Pest Token";

/// Per-game state for this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Default (and absent from snapshots)
/// until a card of this wave needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct C4bStateV1 {
    /// The spell copies on the stack that magecraft already saw.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reported_copies: Vec<StackItemId>,
    /// How many times each landfall ability resolved this turn.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resolutions: Vec<ResolutionCountV1>,
    /// Creatures Okoye's combat trigger chose this turn.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub okoye_marks: Vec<OkoyeMarkV1>,
    /// Players who were dealt damage by Screaming Nemesis's trigger: they
    /// can't gain life for the rest of the game.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lifegain_locked: Vec<PlayerId>,
    /// The power each Ouroboroid had as it left the battlefield (last-known
    /// information), until its turn ends.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub departed_power: Vec<DepartedPowerV1>,
}

impl C4bStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// One landfall ability's resolutions in one turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolutionCountV1 {
    pub source: ObjectId,
    pub zone_change_count: u32,
    pub turn: u32,
    /// The active player of that turn: the turn key counts rounds, so both
    /// players' turns of one round share it.
    pub active: PlayerId,
    pub count: u8,
}

/// A creature Okoye's trigger chose: "whenever that creature attacks the
/// monarch this turn" is watched for it until the turn ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OkoyeMarkV1 {
    pub creature: ObjectId,
    pub zone_change_count: u32,
    pub turn: u32,
    pub controller: PlayerId,
    pub source: AbilitySourceContractV4,
}

/// The power an Ouroboroid incarnation had as it left the battlefield.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DepartedPowerV1 {
    pub source: ObjectId,
    pub zone_change_count: u32,
    pub turn: u32,
    pub power: i32,
}

pub(crate) fn c4b_state(state: &mut GameState) -> &mut C4bStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).c4b
}

fn read_state(state: &GameState) -> Option<&C4bStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.c4b)
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::C4bV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C4bOpV1 {
    /// Omnath's landfall: the first resolution this turn gains 4 life, the
    /// second adds {R}{G}{W}{U}, the third deals 4 damage to each opponent
    /// and each planeswalker the controller doesn't control.
    OmnathLandfallV1,
    /// Scythecat Cub's landfall: a +1/+1 counter on the target creature, or
    /// on the second resolution this turn double that creature's counters.
    CubLandfallV1,
    /// Ouroboroid's combat trigger: X +1/+1 counters on each creature the
    /// controller controls, X being this creature's power.
    OuroboroidCountersV1,
    /// Okoye's combat trigger: a +1/+1 counter on the target creature, which
    /// is then watched for attacking the monarch this turn.
    OkoyeCombatV1,
    /// Okoye's delayed trigger: the marked creature gains double strike and
    /// trample until end of turn.
    OkoyeGrantV1 { creature: EffectObjectBinding },
    /// Sage of the Skies' cast trigger: copy the spell.
    SageCopyV1,
    /// Screaming Nemesis' trigger: it deals `amount` damage to target 0, and
    /// a player dealt damage this way can't gain life for the rest of the game.
    NemesisDamageV1 { amount: i32 },
}

impl C4bOpV1 {
    /// Whether the leaf may stop for a player's choice (or a draw that a
    /// replacement can interrupt), so it runs in the resumable interpreter.
    /// None of this module's leaves asks anything or draws.
    pub(crate) fn contains_player_choice(&self) -> bool {
        false
    }
}

fn c4b(op: C4bOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C4bV1(op)))
}

fn token_id(name: &str) -> u16 {
    crate::card_def::card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

fn definition_name(state: &GameState, object: ObjectId) -> Option<&'static str> {
    state
        .objects
        .try_get(object)
        .and_then(|live| CARD_DEFS.get(usize::from(live.card_def)))
        .map(|def| def.name)
}

// ---- Programs -----------------------------------------------------------------------

/// Sedgemoor Witch: "create a 1/1 black and green Pest creature token with
/// 'When this token dies, you gain 1 life.'" (the Pest Token of Pest
/// Infestation).
fn witch_pest() -> EffectOp {
    EffectOp::CreateToken {
        token_def: token_id(PEST_TOKEN),
        controller: PlayerRef::Controller,
    }
}

/// Witherbloom Apprentice: "each opponent loses 1 life and you gain 1 life"
/// (Arbiter of Woe's drain).
fn apprentice_drain() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::LoseLife {
            player: PlayerRef::Opponent,
            amount: 1,
        },
        EffectOp::GainLife {
            player: PlayerRef::Controller,
            amount: 1,
        },
    ])
}

/// Omnath's "When Omnath enters, draw a card." The draw is a root element of
/// the program, so a draw replacement can interrupt it.
fn omnath_draw() -> EffectOp {
    EffectOp::Sequence(vec![EffectOp::DrawCards {
        player: PlayerRef::Controller,
        count: 1,
    }])
}

fn omnath_landfall() -> EffectOp {
    c4b(C4bOpV1::OmnathLandfallV1)
}

fn cub_landfall() -> EffectOp {
    c4b(C4bOpV1::CubLandfallV1)
}

/// Pompous Battlemage's Prowess (Monastery Swiftspear's trigger).
fn prowess() -> EffectOp {
    EffectOp::BindTemporaryBoostToTriggerSource {
        power: 1,
        toughness: 1,
    }
}

/// Improvised Act, Pompous Battlemage's prepare spell: "You may discard a
/// card. If you do, draw a card." (Pursue the Past's may-discard-then-draw.
/// The draw is conditional on the discard, so it is not a root element, as in
/// that card.)
pub fn improvised_act() -> EffectOp {
    EffectOp::MayPayCostThen {
        discard: 1,
        sacrifice_lands: 0,
        return_permanent: None,
        then: Box::new(EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        }),
        otherwise: None,
    }
}

fn sage_copy() -> EffectOp {
    c4b(C4bOpV1::SageCopyV1)
}

/// Okoye's "When Okoye enters, you become the monarch." (Palace Sentinels).
fn okoye_monarch() -> EffectOp {
    EffectOp::BecomeMonarch
}

fn okoye_combat() -> EffectOp {
    c4b(C4bOpV1::OkoyeCombatV1)
}

fn okoye_grant(creature: EffectObjectBinding) -> EffectOp {
    c4b(C4bOpV1::OkoyeGrantV1 { creature })
}

fn ouroboroid_counters() -> EffectOp {
    c4b(C4bOpV1::OuroboroidCountersV1)
}

fn nemesis_damage(amount: i32) -> EffectOp {
    c4b(C4bOpV1::NemesisDamageV1 { amount })
}

// ---- Hooks: engine ------------------------------------------------------------------

/// Whether `card_def`'s trigger targets "any other target": its own source is
/// not a legal target (Screaming Nemesis).
pub(crate) fn other_target_only(card_def: u16) -> bool {
    cfg!(feature = "eternal-fixtures")
        && CARD_DEFS
            .get(usize::from(card_def))
            .is_some_and(|def| def.name == SCREAMING_NEMESIS)
}

/// Whether `effect` is Screaming Nemesis's own damage trigger, the only
/// ability of it that targets "any other target".
pub(crate) fn is_nemesis_trigger(effect: &EffectOp) -> bool {
    matches!(
        effect,
        EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C4bV1(
            C4bOpV1::NemesisDamageV1 { .. }
        )))
    )
}

/// Screaming Nemesis: "If a player is dealt damage this way, they can't gain
/// life for the rest of the game." Called from the life-gain commit.
pub(crate) fn cannot_gain_life(state: &GameState, player: PlayerId) -> bool {
    cfg!(feature = "eternal-fixtures")
        && read_state(state).is_some_and(|c4b| c4b.lifegain_locked.contains(&player))
}

/// Whether a declaration of attackers must log each declared attacker
/// (`CommittedEvent::DeclaredAttacker`) for Okoye's watch: a creature her
/// combat trigger chose is waiting to attack the monarch.
pub(crate) fn observes_attacks(state: &GameState) -> bool {
    cfg!(feature = "eternal-fixtures")
        && read_state(state).is_some_and(|c4b| !c4b.okoye_marks.is_empty())
}

/// Ouroboroid's departure: its power is last-known information for a trigger
/// that resolves after it left. Called before a permanent leaves the
/// battlefield.
pub(crate) fn note_departure(state: &mut GameState, object: ObjectId) {
    if !cfg!(feature = "eternal-fixtures") || definition_name(state, object) != Some(OUROBOROID) {
        return;
    }
    let live = state.objects.get(object);
    if live.zone != Zone::Battlefield {
        return;
    }
    let entry = DepartedPowerV1 {
        source: object,
        zone_change_count: live.zone_change_count,
        turn: state.turn_key_v1(),
        power: crate::engine::effective_power(state, object),
    };
    let departed = &mut c4b_state(state).departed_power;
    if !departed.iter().any(|known| {
        known.source == entry.source && known.zone_change_count == entry.zone_change_count
    }) {
        departed.push(entry);
    }
}

/// Counts one resolution of the ability that is resolving, this turn, and
/// returns how many have resolved (this one included).
fn bump_resolution(state: &mut GameState, ctx: &ExecCtx) -> u8 {
    let (source, zone_change_count) = ability_source(state, ctx);
    let (turn, active) = (state.turn_key_v1(), state.active_player);
    let resolutions = &mut c4b_state(state).resolutions;
    resolutions.retain(|entry| entry.turn == turn && entry.active == active);
    if let Some(entry) = resolutions
        .iter_mut()
        .find(|entry| entry.source == source && entry.zone_change_count == zone_change_count)
    {
        entry.count = entry.count.saturating_add(1);
        return entry.count;
    }
    resolutions.push(ResolutionCountV1 {
        source,
        zone_change_count,
        turn,
        active,
        count: 1,
    });
    1
}

/// The source incarnation of the ability that is resolving.
fn ability_source(state: &GameState, ctx: &ExecCtx) -> (ObjectId, u32) {
    match ctx.ability_source_contract {
        Some(contract) => (contract.source, contract.zone_change_count),
        None => (
            ctx.source,
            state
                .objects
                .try_get(ctx.source)
                .map_or(0, |live| live.zone_change_count),
        ),
    }
}

// ---- Hooks: queued triggers ---------------------------------------------------------

/// The triggers this collection sets off that no printed trigger condition
/// matches: magecraft's "or copy" half, Screaming Nemesis' damage trigger and
/// Okoye's attack watch.
pub(crate) fn delayed_triggers(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Vec<PendingTrigger> {
    if !cfg!(feature = "eternal-fixtures") {
        return Vec::new();
    }
    let mut triggers = Vec::new();
    magecraft_copy_triggers(state, &mut triggers);
    nemesis_triggers(state, events, &mut triggers);
    okoye_triggers(state, events, &mut triggers);
    prune(state);
    triggers
}

/// Forgets what only mattered earlier in the turn.
fn prune(state: &mut GameState) {
    let turn = state.turn_key_v1();
    let Some(eternal) = state.eternal_v1.as_mut() else {
        return;
    };
    let c4b = &mut eternal.c4b;
    if c4b.okoye_marks.iter().any(|mark| mark.turn != turn) {
        c4b.okoye_marks.retain(|mark| mark.turn == turn);
    }
    if c4b.departed_power.iter().any(|entry| entry.turn != turn) {
        c4b.departed_power.retain(|entry| entry.turn == turn);
    }
}

/// Magecraft: "Whenever you cast or copy an instant or sorcery spell". A copy
/// of an instant or sorcery spell that appeared on the stack since the last
/// collection, under a player's control, triggers each of that player's
/// magecraft permanents once (a copy is not cast, so the cast half does not
/// see it).
fn magecraft_copy_triggers(state: &mut GameState, out: &mut Vec<PendingTrigger>) {
    let copies: Vec<(StackItemId, ObjectId, PlayerId)> = state
        .stack
        .iter()
        .filter(|item| item.kind == StackItemKind::Spell && item.is_copy)
        .map(|item| (item.v4.stack_item_id, item.source, item.controller))
        .collect();
    if copies.is_empty() {
        if let Some(eternal) = state.eternal_v1.as_mut() {
            if !eternal.c4b.reported_copies.is_empty() {
                eternal.c4b.reported_copies.clear();
            }
        }
        return;
    }
    // Games without a magecraft card keep their state untouched.
    let has_magecraft = state.objects.iter().any(|(_, object)| {
        CARD_DEFS
            .get(usize::from(object.card_def))
            .is_some_and(|def| def.name == SEDGEMOOR_WITCH || def.name == WITHERBLOOM_APPRENTICE)
    });
    if !has_magecraft {
        return;
    }
    let seen = read_state(state)
        .map(|c4b| c4b.reported_copies.clone())
        .unwrap_or_default();
    for &(id, copy, controller) in &copies {
        if seen.contains(&id) {
            continue;
        }
        let types = crate::trigger::selected_spell_types(state, copy);
        if !(types.contains(&CardType::Instant) || types.contains(&CardType::Sorcery)) {
            continue;
        }
        for (name, effect) in [
            (SEDGEMOOR_WITCH, witch_pest()),
            (WITHERBLOOM_APPRENTICE, apprentice_drain()),
        ] {
            let permanents =
                crate::eternal_e5a_v1::active_permanents(state, name, Some(controller))
                    .collect::<Vec<_>>();
            for permanent in permanents {
                out.push(crate::eternal_cards_v1::delayed_trigger(
                    AbilitySourceContractV4::capture(state, permanent),
                    controller,
                    effect.clone(),
                ));
            }
        }
    }
    let now: Vec<StackItemId> = copies.iter().map(|&(id, _, _)| id).collect();
    if now != seen {
        c4b_state(state).reported_copies = now;
    }
}

/// Trigger conditions owned by this module, carried as
/// `eternal_cards_v1::EternalTriggerV1::C4bV1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum C4bTriggerV1 {
    /// "When you cast this spell, if you've cast another spell this turn"
    /// (Sage of the Skies). Functions on the stack, and is checked as the
    /// cast is collected (the intervening if of 603.4).
    CastSelfAfterAnotherSpellV1,
}

/// Whether the cast `events[index]` is the controller's first-or-later spell
/// cast this turn with another spell cast before it: the turn's counter
/// already holds every cast of the collected batch.
fn another_spell_cast_before(
    state: &GameState,
    events: &[CommittedEvent],
    index: usize,
    caster: PlayerId,
) -> bool {
    let casts_by = |slice: &[CommittedEvent]| {
        slice
            .iter()
            .filter(|other| {
                matches!(
                    other,
                    CommittedEvent::SpellCast { controller, .. } if *controller == caster
                )
            })
            .count()
    };
    let before_collection = usize::from(state.players[caster.index()].spells_cast_this_turn)
        .saturating_sub(casts_by(events));
    before_collection + casts_by(&events[..index]) > 0
}

pub(crate) fn trigger_matches(
    condition: C4bTriggerV1,
    events: &[CommittedEvent],
    index: usize,
    source: ObjectId,
    state: &GameState,
) -> bool {
    match condition {
        C4bTriggerV1::CastSelfAfterAnotherSpellV1 => {
            let CommittedEvent::SpellCast { spell, controller } = events[index] else {
                return false;
            };
            spell == source
                && state.stack.iter().any(|item| {
                    item.kind == StackItemKind::Spell
                        && item.source == source
                        && item.controller == controller
                        && !item.is_copy
                })
                && another_spell_cast_before(state, events, index, controller)
        }
    }
}

/// Whether `object` is a face-up Screaming Nemesis whose printed abilities
/// applied: on the battlefield now, or a card that left it in the same event
/// batch (a creature dealt lethal damage still triggers).
fn printed_nemesis(state: &GameState, object: ObjectId) -> bool {
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    if live.zone == Zone::Battlefield {
        return crate::eternal_e5a_v1::active_named(state, object, SCREAMING_NEMESIS);
    }
    live.v4.face_down_v1.is_none()
        && !live.v4.is_token
        && definition_name(state, object) == Some(SCREAMING_NEMESIS)
}

/// Screaming Nemesis: "Whenever this creature is dealt damage, it deals that
/// much damage to any other target." One trigger per damage event.
fn nemesis_triggers(state: &GameState, events: &[CommittedEvent], out: &mut Vec<PendingTrigger>) {
    for event in events {
        let CommittedEvent::Damage {
            target: Target::Object(object),
            amount,
            ..
        } = *event
        else {
            continue;
        };
        if amount <= 0 || !printed_nemesis(state, object) {
            continue;
        }
        let controller = state.objects.get(object).controller;
        let mut pending = crate::eternal_cards_v1::delayed_trigger(
            AbilitySourceContractV4::capture(state, object),
            controller,
            nemesis_damage(amount),
        );
        pending.target_spec = TargetSpec::AnyTarget;
        out.push(pending);
    }
}

/// Okoye: "Whenever that creature attacks the monarch this turn, it gains
/// double strike and trample until end of turn." A creature chosen by the
/// combat trigger attacks the monarch when it is declared as an attacker of
/// the player who is the monarch (not of a planeswalker).
fn okoye_triggers(state: &GameState, events: &[CommittedEvent], out: &mut Vec<PendingTrigger>) {
    let Some(c4b) = read_state(state).filter(|c4b| !c4b.okoye_marks.is_empty()) else {
        return;
    };
    let turn = state.turn_key_v1();
    for event in events {
        let CommittedEvent::DeclaredAttacker {
            source,
            source_zone_change_count,
            controller,
        } = *event
        else {
            continue;
        };
        let attacking_the_monarch = state.monarch == Some(controller.opponent())
            && crate::attack_target_v1::attacked_planeswalker(state, source).is_none();
        if !attacking_the_monarch
            || !state.objects.try_get(source).is_some_and(|live| {
                live.zone == Zone::Battlefield && live.zone_change_count == source_zone_change_count
            })
        {
            continue;
        }
        for mark in c4b.okoye_marks.iter().filter(|mark| {
            mark.turn == turn
                && mark.creature == source
                && mark.zone_change_count == source_zone_change_count
        }) {
            out.push(crate::eternal_cards_v1::delayed_trigger(
                mark.source,
                mark.controller,
                okoye_grant(EffectObjectBinding {
                    object: source,
                    expected_zone: Zone::Battlefield,
                    expected_zone_change_count: source_zone_change_count,
                }),
            ));
        }
    }
}

// ---- Triggered abilities ------------------------------------------------------------

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

const WITCH_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::CastInstantOrSorcery, witch_pest)];
const APPRENTICE_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::CastInstantOrSorcery,
    apprentice_drain,
)];
const OMNATH_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, omnath_draw),
    trigger(TriggerCondition::ControlledLandEnters, omnath_landfall),
];
const CUB_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::ControlledLandEnters,
    cub_landfall,
)];
const POMPOUS_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::CastNoncreatureSpell, prowess)];
const OKOYE_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, okoye_monarch),
    trigger(TriggerCondition::BeginningOfControllerCombat, okoye_combat),
];
const SAGE_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::StandardV1(StandardTriggerV1::EternalV1(EternalTriggerV1::C4bV1(
        C4bTriggerV1::CastSelfAfterAnotherSpellV1,
    ))),
    home_zone: Zone::Stack,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: sage_copy,
}];
const OUROBOROID_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::BeginningOfControllerCombat,
    ouroboroid_counters,
)];

/// Triggered abilities of this module's cards, falling through to wave
/// C4c. Magecraft's copy half, Screaming Nemesis' damage trigger and Okoye's
/// attack watch are delayed triggers.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        SEDGEMOOR_WITCH => &WITCH_TRIGGERS,
        WITHERBLOOM_APPRENTICE => &APPRENTICE_TRIGGERS,
        OMNATH => &OMNATH_TRIGGERS,
        SCYTHECAT_CUB => &CUB_TRIGGERS,
        POMPOUS_BATTLEMAGE => &POMPOUS_TRIGGERS,
        OKOYE => &OKOYE_TRIGGERS,
        SAGE_OF_THE_SKIES => &SAGE_TRIGGERS,
        OUROBOROID => &OUROBOROID_TRIGGERS,
        _ => crate::eternal_cube_c4c_v1::triggers_for(name),
    }
}

/// Whether `name` is one of the cards with printed triggers in this module.
fn owns_printed_triggers(name: &str) -> bool {
    matches!(
        name,
        SEDGEMOOR_WITCH
            | WITHERBLOOM_APPRENTICE
            | OMNATH
            | SCYTHECAT_CUB
            | POMPOUS_BATTLEMAGE
            | OKOYE
            | SAGE_OF_THE_SKIES
            | OUROBOROID
    )
}

/// Target specs of this module's printed and queued triggered abilities.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    match name {
        SCYTHECAT_CUB if *effect == cub_landfall() => Some(TargetSpec::ControlledCreature),
        OKOYE if *effect == okoye_combat() => Some(TargetSpec::Creature),
        SCREAMING_NEMESIS
            if matches!(
                effect,
                EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C4bV1(
                    C4bOpV1::NemesisDamageV1 { .. }
                )))
            ) =>
        {
            Some(TargetSpec::AnyTarget)
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
    match name {
        SEDGEMOOR_WITCH => *effect == witch_pest(),
        WITHERBLOOM_APPRENTICE => *effect == apprentice_drain(),
        OKOYE => matches!(
            effect,
            EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C4bV1(
                C4bOpV1::OkoyeGrantV1 { .. }
            )))
        ),
        SCREAMING_NEMESIS => matches!(
            effect,
            EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C4bV1(
                C4bOpV1::NemesisDamageV1 { .. }
            )))
        ),
        _ => false,
    }
}

// ---- Execution ----------------------------------------------------------------------

/// Runs one of this module's leaves in synchronous execution.
pub(crate) fn execute(op: &C4bOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        C4bOpV1::OmnathLandfallV1 => run_omnath_landfall(ctx, state),
        C4bOpV1::CubLandfallV1 => run_cub_landfall(ctx, state),
        C4bOpV1::OuroboroidCountersV1 => run_ouroboroid(ctx, state),
        C4bOpV1::OkoyeCombatV1 => run_okoye_combat(ctx, state),
        C4bOpV1::OkoyeGrantV1 { creature } => {
            crate::effect::install_temporary_boost(
                state,
                *creature,
                0,
                0,
                Keywords::DOUBLE_STRIKE | Keywords::TRAMPLE,
            );
        }
        C4bOpV1::SageCopyV1 => run_sage_copy(ctx, state),
        C4bOpV1::NemesisDamageV1 { amount } => run_nemesis_damage(ctx, state, *amount),
    }
}

/// Runs one of this module's leaves in the resumable interpreter.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &C4bOpV1,
    _path: Vec<u16>,
) -> Result<bool, String> {
    execute(op, &continuation.ctx, state);
    Ok(false)
}

/// Omnath, Locus of Creation: "Landfall - Whenever a land you control enters,
/// you gain 4 life if this is the first time this ability has resolved this
/// turn. If it's the second time, add {R}{G}{W}{U}. If it's the third time,
/// Omnath deals 4 damage to each opponent and each planeswalker you don't
/// control."
fn run_omnath_landfall(ctx: &ExecCtx, state: &mut GameState) {
    match bump_resolution(state, ctx) {
        1 => crate::effect::execute(
            &EffectOp::GainLife {
                player: PlayerRef::Controller,
                amount: 4,
            },
            ctx,
            state,
        ),
        2 => crate::effect::execute(
            &EffectOp::AddMana {
                player: PlayerRef::Controller,
                colors: vec![ManaColor::R, ManaColor::G, ManaColor::W, ManaColor::U],
            },
            ctx,
            state,
        ),
        3 => {
            // One simultaneous batch, as End the Festivities' damage is.
            let opponent = ctx.controller.opponent();
            let mut events = vec![event::damage_from_exec_ctx_v1(
                ctx,
                Target::Player(opponent),
                4,
            )];
            events.extend(
                state.players[opponent.index()]
                    .battlefield
                    .iter()
                    .copied()
                    .filter(|&id| {
                        state.objects.get(id).controller != ctx.controller
                            && crate::engine::object_has_type(state, id, CardType::Planeswalker)
                    })
                    .map(|id| event::damage_from_exec_ctx_v1(ctx, Target::Object(id), 4)),
            );
            event::propose_and_commit_batch(state, events);
        }
        _ => {}
    }
}

/// The creature target 0 names when it is still the controller's creature on
/// the battlefield.
fn controlled_creature_target(ctx: &ExecCtx, state: &GameState) -> Option<ObjectId> {
    let Some(Target::Object(object)) = ctx.targets.first().copied() else {
        return None;
    };
    let live = state.objects.try_get(object)?;
    (ctx.target_incarnation_matches(0, state)
        && live.zone == Zone::Battlefield
        && live.controller == ctx.controller
        && crate::engine::object_has_type(state, object, CardType::Creature))
    .then_some(object)
}

/// Scythecat Cub: "Landfall - Whenever a land you control enters, put a +1/+1
/// counter on target creature you control. If this is the second time this
/// ability has resolved this turn, double the number of +1/+1 counters on that
/// creature instead." A trigger whose target is gone does not resolve, so it
/// is not counted.
fn run_cub_landfall(ctx: &ExecCtx, state: &mut GameState) {
    let Some(creature) = controlled_creature_target(ctx, state) else {
        return;
    };
    let amount = if bump_resolution(state, ctx) == 2 {
        state.objects.get(creature).counters.plus1_plus1
    } else {
        1
    };
    // Doubling a creature with no counters adds none.
    if amount > 0 && event::add_plus_one_counters(state, creature, ctx.controller, amount).is_err()
    {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            ctx.source,
        ));
    }
}

/// Ouroboroid: "At the beginning of combat on your turn, put X +1/+1 counters
/// on each creature you control, where X is this creature's power." X is fixed
/// before any counter is placed; a departed Ouroboroid's power is its
/// last-known power.
fn run_ouroboroid(ctx: &ExecCtx, state: &mut GameState) {
    let (source, zone_change_count) = ability_source(state, ctx);
    let on_battlefield = state.objects.try_get(source).is_some_and(|live| {
        live.zone == Zone::Battlefield && live.zone_change_count == zone_change_count
    });
    let power = if on_battlefield {
        crate::engine::effective_power(state, source)
    } else {
        read_state(state)
            .and_then(|c4b| {
                c4b.departed_power.iter().find(|entry| {
                    entry.source == source && entry.zone_change_count == zone_change_count
                })
            })
            .map_or(0, |entry| entry.power)
    };
    if power <= 0 {
        return;
    }
    let creatures: Vec<ObjectId> = state.players[ctx.controller.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&id| {
            state.objects.get(id).controller == ctx.controller
                && crate::engine::object_has_type(state, id, CardType::Creature)
        })
        .collect();
    for creature in creatures {
        if state.objects.get(creature).zone != Zone::Battlefield {
            continue;
        }
        if event::add_plus_one_counters(state, creature, ctx.controller, power).is_err() {
            state.engine.halted = Some((
                crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                ctx.source,
            ));
            return;
        }
    }
}

/// Okoye, Mighty and Adored: "At the beginning of combat on your turn, put a
/// +1/+1 counter on target creature. Whenever that creature attacks the
/// monarch this turn, it gains double strike and trample until end of turn."
fn run_okoye_combat(ctx: &ExecCtx, state: &mut GameState) {
    let Some(Target::Object(creature)) = ctx.targets.first().copied() else {
        return;
    };
    let Some(live) = state.objects.try_get(creature) else {
        return;
    };
    if !ctx.target_incarnation_matches(0, state)
        || live.zone != Zone::Battlefield
        || !crate::engine::object_has_type(state, creature, CardType::Creature)
    {
        return;
    }
    let zone_change_count = live.zone_change_count;
    if event::add_plus_one_counters(state, creature, ctx.controller, 1).is_err() {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            ctx.source,
        ));
        return;
    }
    let source = ctx
        .ability_source_contract
        .unwrap_or_else(|| AbilitySourceContractV4::capture(state, ctx.source));
    let mark = OkoyeMarkV1 {
        creature,
        zone_change_count,
        turn: state.turn_key_v1(),
        controller: ctx.controller,
        source,
    };
    let marks = &mut c4b_state(state).okoye_marks;
    if !marks.contains(&mark) {
        marks.push(mark);
    }
}

/// Sage of the Skies: "When you cast this spell, if you've cast another spell
/// this turn, copy this spell." The intervening condition is read again as
/// the trigger resolves (603.4); a spell that is no longer on the stack has
/// nothing to copy.
fn run_sage_copy(ctx: &ExecCtx, state: &mut GameState) {
    if state.players[ctx.controller.index()].spells_cast_this_turn < 2 {
        return;
    }
    let Some((_, copy)) = crate::engine::copy_spell_for(state, ctx.source, ctx.controller) else {
        return;
    };
    if crate::engine::finish_spell_copy_targets(state, copy, None).is_err() {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            ctx.source,
        ));
    }
}

/// Screaming Nemesis: deals `amount` damage to its target, and a player who
/// is actually dealt damage by it can't gain life for the rest of the game.
fn run_nemesis_damage(ctx: &ExecCtx, state: &mut GameState, amount: i32) {
    let Some(&target) = ctx.targets.first() else {
        return;
    };
    if !ctx.target_incarnation_matches(0, state) || target == Target::Object(ctx.source) {
        return;
    }
    let start = state.engine.event_history.len();
    crate::effect::execute(
        &EffectOp::DealDamage {
            target: TargetRef::Target(0),
            amount,
        },
        ctx,
        state,
    );
    let Target::Player(player) = target else {
        return;
    };
    let end = state.engine.event_history.len();
    let dealt = (start..end)
        .filter_map(|at| state.engine.event_history.get(at))
        .any(|event| {
            matches!(
                event,
                CommittedEvent::Damage { source, target: Target::Player(hit), amount }
                    if *source == ctx.source && *hit == player && *amount > 0
            )
        });
    if dealt && !c4b_state(state).lifegain_locked.contains(&player) {
        c4b_state(state).lifegain_locked.push(player);
    }
}
