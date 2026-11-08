//! Card programs for the eternal catalog's sideboard wave S1a
//! (`docs/design/eternal_formats_support_v1.md`): the cards that complete the
//! most Legacy sideboards per card (graveyard hate, mass removal, artifact
//! and enchantment hate, taxing statics).
//!
//! `eternal_cards_v1` forwards its effect leaves and state here, and the
//! engine calls the hooks in `statics` only in `eternal-fixtures` builds.
//! Every table is keyed by a printed name that occurs only in the eternal
//! registry, so the Pauper, FDN and Standard catalogs never reach it.
//! Behavior follows each card's current Oracle text, checked against its
//! XMage source.
//!
//! Primitives added here:
//! - mass effects with a flying filter (Rough // Tumble), damage to every
//!   creature and planeswalker (Brotherhood's End), destroying every
//!   artifact up to a mana value (Meltdown, Brotherhood's End), destroying
//!   every artifact and enchantment (Serenity) and a mass shrink (Massacre);
//! - the zone-change replacements of Rest in Peace and Leyline of the Void
//!   (exile instead of a graveyard) and Containment Priest (exile a nontoken
//!   creature that enters without being cast), joined to the replacement
//!   hook Dauthi Voidwalker uses;
//! - Leyline of the Void's opening-hand permission, offered as the game's
//!   first upkeep begins as Chancellor of the Annex's reveal is;
//! - Disruptor Flute's chosen name: spells with that name cost {3} more and
//!   its non-mana activated abilities cannot be activated;
//! - Choke's untap restriction, Defense Grid's spell tax, Dread of Night's
//!   power/toughness modifier and Massacre's alternative cost condition.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

pub mod statics;
pub(crate) mod targets;

use crate::card_def::{CardType, Keywords, TargetSpec, CARD_DEFS};
use crate::effect::{
    CreatureFilter, EffectCond, EffectOp, ExecCtx, ObjectRef, PlayerRef, TargetRef,
};
use crate::eternal_cards_v1::EternalOpV1;
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId, StackItemId};
use crate::mana::ManaColor;
use crate::standard_cards_v1::StandardOpV1;
use crate::state::{AbilitySourceContractV4, GameState, ObjectLinkV4, StackItemKind, Target, Zone};
use crate::trigger::{PendingTrigger, TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

// ---- State ------------------------------------------------------------------------

/// Per-game state for this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Default (and absent from snapshots)
/// until one of these cards needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct S1aStateV1 {
    /// Disruptor Flute incarnations and the definition whose name each chose.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flute_names: Vec<(ObjectLinkV4, u16)>,
    /// Whether the opening-hand Leyline permission was offered.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub leylines_offered: bool,
    /// Spells cast with no mana spent while a Lavinia was in play.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unpaid_casts: Vec<ObjectLinkV4>,
    /// The turn each Carpet of Flowers incarnation last added mana.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub carpet_turns: Vec<(ObjectLinkV4, u32)>,
}

impl Hash for S1aStateV1 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        if !self.is_empty() {
            b"eternal-sideboard-s1a/v1".hash(state);
            self.flute_names.hash(state);
            self.leylines_offered.hash(state);
            self.unpaid_casts.hash(state);
            self.carpet_turns.hash(state);
        }
    }
}

impl S1aStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

pub(crate) fn s1a_state(state: &mut GameState) -> &mut S1aStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).s1a
}

pub fn read_state(state: &GameState) -> Option<&S1aStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.s1a)
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::S1aV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum S1aOpV1 {
    /// `amount` damage to each creature with (`flying`) or without flying.
    DamageByFlyingV1 { amount: i32, flying: bool },
    /// `amount` damage to each creature and each planeswalker.
    DamageCreaturesAndPlaneswalkersV1 { amount: i32 },
    /// Destroy each artifact with mana value `limit` or less.
    DestroyArtifactsAtMostV1 { limit: u16 },
    /// Destroy each artifact with mana value X or less (the announced X).
    DestroyArtifactsAtMostXV1,
    /// Destroy all artifacts and enchantments.
    DestroyArtifactsAndEnchantmentsV1,
    /// All creatures get -2/-2 until end of turn.
    MassacreV1,
    /// The resolving Disruptor Flute enters naming definition `named`.
    FluteNamedV1 { named: u16 },
    /// Leyline of the Void: put the opening-hand card onto the battlefield.
    LeylineEnterV1,
    /// Lavinia, Azorius Renegade: counter the spell `spell` an opponent cast
    /// without spending mana.
    CounterSpellItemV1 { spell: StackItemId },
    /// Surgical Extraction: exile target 0 and every card with its name from
    /// its owner's graveyard, hand and library, then shuffle that library.
    SurgicalExtractionV1,
    /// Carpet of Flowers: add up to `amount` mana of `color`, no more than
    /// the Islands the opponent controls.
    CarpetManaV1 { color: ManaColor, amount: u8 },
}

impl S1aOpV1 {
    /// Whether this leaf can yield a player decision or pushes frames, so
    /// its program must use the resumable interpreter. None of these do:
    /// their choices are `EffectOp::Choice` branches around them.
    pub(crate) fn contains_player_choice(&self) -> bool {
        false
    }
}

fn s1a(op: S1aOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::S1aV1(op)))
}

// ---- Names ------------------------------------------------------------------------

pub(crate) const CHOKE: &str = "Choke";
pub(crate) const CONTAINMENT_PRIEST: &str = "Containment Priest";
pub(crate) const DEFENSE_GRID: &str = "Defense Grid";
pub(crate) const DISRUPTOR_FLUTE: &str = "Disruptor Flute";
pub(crate) const DREAD_OF_NIGHT: &str = "Dread of Night";
pub(crate) const CARPET_OF_FLOWERS: &str = "Carpet of Flowers";
pub(crate) const HAYWIRE_MITE: &str = "Haywire Mite";
pub(crate) const LAVINIA: &str = "Lavinia, Azorius Renegade";
pub(crate) const LEYLINE_OF_THE_VOID: &str = "Leyline of the Void";
pub(crate) const ROUGH_TUMBLE: &str = "Rough // Tumble";
pub(crate) const REST_IN_PEACE: &str = "Rest in Peace";
pub(crate) const SERENITY: &str = "Serenity";
pub(crate) const TWINSHOT_SNIPER: &str = "Twinshot Sniper";
const SWAN_SONG_BIRD_TOKEN: &str = "Swan Song Bird Token";

// ---- Helpers ----------------------------------------------------------------------

fn halt(state: &mut GameState, source: ObjectId) {
    state.engine.halted = Some((
        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
        source,
    ));
}

pub(crate) fn live_link(state: &GameState, object: ObjectId) -> ObjectLinkV4 {
    ObjectLinkV4 {
        object,
        zone_change_count: state.objects.get(object).zone_change_count,
    }
}

pub(crate) fn link_live_in(state: &GameState, link: ObjectLinkV4, zone: Zone) -> bool {
    state
        .objects
        .try_get(link.object)
        .is_some_and(|live| live.zone == zone && live.zone_change_count == link.zone_change_count)
}

/// Whether `object` is a front-face battlefield permanent named `name`
/// whose printed abilities apply.
pub(crate) fn active_named(state: &GameState, object: ObjectId, name: &str) -> bool {
    state.objects.try_get(object).is_some_and(|live| {
        live.zone == Zone::Battlefield
            && live.v4.face_index == 0
            && CARD_DEFS
                .get(usize::from(live.card_def))
                .is_some_and(|def| def.name == name && def.is_executable())
            && crate::continuous_characteristics_v1::printed_abilities_active(state, object)
    })
}

/// Every battlefield object, in each player's battlefield order.
pub(crate) fn battlefield(state: &GameState) -> impl Iterator<Item = ObjectId> + '_ {
    [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(move |player| state.players[player.index()].battlefield.iter().copied())
        .filter(move |&object| state.objects.get(object).zone == Zone::Battlefield)
}

fn has_type(state: &GameState, object: ObjectId, card_type: CardType) -> bool {
    crate::engine::object_has_type(state, object, card_type)
}

fn indestructible(state: &GameState, object: ObjectId) -> bool {
    crate::engine::has_effective_keyword(state, object, Keywords::INDESTRUCTIBLE)
}

/// Destroys `objects` together (regeneration shields apply).
fn destroy_all(state: &mut GameState, objects: Vec<ObjectId>) {
    let destroyed = crate::effect::without_regenerated_v1(state, objects);
    event::propose_and_commit_batch(
        state,
        destroyed
            .into_iter()
            .map(|object| ProposedEvent::zone_change(object, Zone::Graveyard))
            .collect(),
    );
}

// ---- Spell programs ---------------------------------------------------------------

/// Pyroclasm: "Pyroclasm deals 2 damage to each creature."
pub fn pyroclasm() -> EffectOp {
    EffectOp::DamageAllCreatures {
        filter: CreatureFilter::All,
        amount: 2,
    }
}

/// Rip Apart's first mode: "Rip Apart deals 3 damage to target creature or
/// planeswalker."
pub fn rip_apart_damage() -> EffectOp {
    EffectOp::DealDamage {
        target: TargetRef::Target(0),
        amount: 3,
    }
}

/// Destroys target 0 if it is still on the battlefield (Rip Apart's second
/// mode: "Destroy target artifact or enchantment").
pub fn destroy_target() -> EffectOp {
    EffectOp::Conditional {
        cond: EffectCond::TargetInZone(0, Zone::Battlefield),
        then: Box::new(EffectOp::DestroyObject {
            object: ObjectRef::Target(0),
        }),
        else_: Box::new(EffectOp::Sequence(vec![])),
    }
}

/// Rough: "Rough deals 2 damage to each creature without flying."
pub fn rough() -> EffectOp {
    s1a(S1aOpV1::DamageByFlyingV1 {
        amount: 2,
        flying: false,
    })
}

/// Tumble: "Tumble deals 6 damage to each creature with flying."
pub fn tumble() -> EffectOp {
    s1a(S1aOpV1::DamageByFlyingV1 {
        amount: 6,
        flying: true,
    })
}

/// Brotherhood's End's first mode: "3 damage to each creature and each
/// planeswalker."
pub fn brotherhoods_end_damage() -> EffectOp {
    s1a(S1aOpV1::DamageCreaturesAndPlaneswalkersV1 { amount: 3 })
}

/// Brotherhood's End's second mode: "Destroy all artifacts with mana value 3
/// or less."
pub fn brotherhoods_end_destroy() -> EffectOp {
    s1a(S1aOpV1::DestroyArtifactsAtMostV1 { limit: 3 })
}

/// Meltdown: "Destroy each artifact with mana value X or less."
pub fn meltdown() -> EffectOp {
    s1a(S1aOpV1::DestroyArtifactsAtMostXV1)
}

/// Massacre: "All creatures get -2/-2 until end of turn."
pub fn massacre() -> EffectOp {
    s1a(S1aOpV1::MassacreV1)
}

/// Exiles target 0 if it is still on the battlefield (Celestial Purge:
/// "Exile target black or red permanent"; Warping Wail's first mode).
pub fn exile_target() -> EffectOp {
    EffectOp::Conditional {
        cond: EffectCond::TargetInZone(0, Zone::Battlefield),
        then: Box::new(EffectOp::MoveObject {
            object: ObjectRef::Target(0),
            to_zone: Zone::Exile,
        }),
        else_: Box::new(EffectOp::Sequence(vec![])),
    }
}

/// Force of Vigor: "Destroy up to two target artifacts and/or
/// enchantments." Each target that is still on the battlefield is destroyed.
pub fn force_of_vigor() -> EffectOp {
    EffectOp::Sequence(
        (0..2)
            .map(|index| EffectOp::Conditional {
                cond: EffectCond::TargetInZone(index, Zone::Battlefield),
                then: Box::new(EffectOp::DestroyObject {
                    object: ObjectRef::Target(index),
                }),
                else_: Box::new(EffectOp::Sequence(vec![])),
            })
            .collect(),
    )
}

/// Swan Song: "Counter target enchantment, instant, or sorcery spell. Its
/// controller creates a 2/2 blue Bird creature token with flying."
pub fn swan_song() -> EffectOp {
    EffectOp::CounterTargetSpellThenCreateTokens {
        target_index: 0,
        token_def: crate::card_def::card_id_by_name(SWAN_SONG_BIRD_TOKEN)
            .unwrap_or_else(|| panic!("{SWAN_SONG_BIRD_TOKEN} in CARD_DEFS")),
        count: 1,
    }
}

/// Warping Wail's second mode: "Counter target sorcery spell."
pub fn warping_wail_counter() -> EffectOp {
    EffectOp::Conditional {
        cond: EffectCond::TargetSpellCanBeCountered(0),
        then: Box::new(EffectOp::MoveObject {
            object: ObjectRef::Target(0),
            to_zone: Zone::Graveyard,
        }),
        else_: Box::new(EffectOp::Sequence(vec![])),
    }
}

/// Warping Wail's third mode: a 1/1 colorless Eldrazi Scion creature token
/// with "Sacrifice this token: Add {C}."
pub fn warping_wail_scion() -> EffectOp {
    EffectOp::CreateToken {
        token_def: crate::card_def::card_id_by_name("Eldrazi Scion Token")
            .unwrap_or_else(|| panic!("Eldrazi Scion Token in CARD_DEFS")),
        controller: PlayerRef::Controller,
    }
}

/// Surgical Extraction: "Choose target card in a graveyard other than a
/// basic land card. Search its owner's graveyard, hand, and library for any
/// number of cards with the same name as that card and exile them. Then that
/// player shuffles."
pub fn surgical_extraction() -> EffectOp {
    s1a(S1aOpV1::SurgicalExtractionV1)
}

/// Disruptor Flute: "As this artifact enters, choose a card name." The name
/// is chosen by first letter, then from the names with that letter, as
/// Anointed Peacekeeper's is.
pub fn disruptor_flute() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: crate::eternal_vintage_v1::nameable_definitions()
            .iter()
            .map(|ids| EffectOp::Choice {
                controller: PlayerRef::Controller,
                options: ids
                    .iter()
                    .map(|&named| s1a(S1aOpV1::FluteNamedV1 { named }))
                    .collect(),
            })
            .collect(),
    }
}

// ---- Triggered abilities -------------------------------------------------------------

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

const YOUR_UPKEEP: TriggerCondition = TriggerCondition::BeginningOfUpkeep {
    controller_only: true,
};

/// Rest in Peace: "When this enchantment enters, exile all cards from all
/// graveyards."
fn rest_in_peace_enters() -> EffectOp {
    EffectOp::ExileAllGraveyards
}

/// Haywire Mite: "When this creature dies, you gain 2 life."
fn haywire_mite_dies() -> EffectOp {
    EffectOp::GainLife {
        player: PlayerRef::Controller,
        amount: 2,
    }
}

/// Serenity: "At the beginning of your upkeep, destroy all artifacts and
/// enchantments. They can't be regenerated."
fn serenity_upkeep() -> EffectOp {
    s1a(S1aOpV1::DestroyArtifactsAndEnchantmentsV1)
}

/// Twinshot Sniper: "When this creature enters, it deals 2 damage to any
/// target."
fn twinshot_sniper_enters() -> EffectOp {
    EffectOp::DealDamage {
        target: TargetRef::Target(0),
        amount: 2,
    }
}

const REST_IN_PEACE_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, rest_in_peace_enters)];
const HAYWIRE_MITE_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::LeftBattlefieldToGraveyard,
    haywire_mite_dies,
)];
const SERENITY_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(YOUR_UPKEEP, serenity_upkeep)];
const TWINSHOT_SNIPER_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, twinshot_sniper_enters)];

/// This module's own triggers, without the fallthrough to other waves.
fn own_triggers_of(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        REST_IN_PEACE => &REST_IN_PEACE_TRIGGERS,
        HAYWIRE_MITE => &HAYWIRE_MITE_TRIGGERS,
        SERENITY => &SERENITY_TRIGGERS,
        TWINSHOT_SNIPER => &TWINSHOT_SNIPER_TRIGGERS,
        _ => &[],
    }
}

/// Triggered abilities of this module's cards.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match own_triggers_of(name) {
        [] => crate::eternal_e9b_v1::triggers_for(name),
        triggers => triggers,
    }
}

/// Leyline of the Void's opening-hand choice: "If this card is in your
/// opening hand, you may begin the game with it on the battlefield."
fn leyline_choice() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: vec![s1a(S1aOpV1::LeylineEnterV1), EffectOp::Sequence(Vec::new())],
    }
}

fn own_trigger_effect(name: &str, effect: &EffectOp) -> bool {
    own_triggers_of(name)
        .iter()
        .any(|trigger| (trigger.effect)() == *effect)
        || delayed_effect_matches(name, effect)
}

/// Target specs of this module's triggered abilities, per ability: only
/// Twinshot Sniper's takes a target. `None` for every other card's.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    if !own_trigger_effect(name, effect) {
        return None;
    }
    Some(match name {
        TWINSHOT_SNIPER if *effect == twinshot_sniper_enters() => TargetSpec::AnyTarget,
        _ => TargetSpec::None,
    })
}

/// Queued and delayed triggers owned by this module's cards.
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    match name {
        LEYLINE_OF_THE_VOID => *effect == leyline_choice(),
        LAVINIA => matches!(
            effect,
            EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::S1aV1(
                S1aOpV1::CounterSpellItemV1 { .. }
            )))
        ),
        CARPET_OF_FLOWERS => is_carpet_choice(effect),
        _ => false,
    }
}

/// Carpet of Flowers' "you may add up to X mana of any one color": a choice
/// among adding nothing and each color and amount.
fn carpet_choice(islands: u8) -> EffectOp {
    let mut options = vec![EffectOp::Sequence(Vec::new())];
    for color in [
        ManaColor::W,
        ManaColor::U,
        ManaColor::B,
        ManaColor::R,
        ManaColor::G,
    ] {
        for amount in 1..=islands {
            options.push(s1a(S1aOpV1::CarpetManaV1 { color, amount }));
        }
    }
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options,
    }
}

fn is_carpet_choice(effect: &EffectOp) -> bool {
    let EffectOp::Choice { options, .. } = effect else {
        return false;
    };
    options.len() > 1
        && options[1..].iter().all(|option| {
            matches!(
                option,
                EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::S1aV1(
                    S1aOpV1::CarpetManaV1 { .. }
                )))
            )
        })
}

// ---- Effect leaves ------------------------------------------------------------------

pub(crate) fn execute(op: &S1aOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        S1aOpV1::DamageByFlyingV1 { amount, flying } => {
            let events = battlefield(state)
                .filter(|&object| has_type(state, object, CardType::Creature))
                .filter(|&object| {
                    crate::engine::has_effective_keyword(state, object, Keywords::FLYING) == *flying
                })
                .map(|object| event::damage_from_exec_ctx_v1(ctx, Target::Object(object), *amount))
                .collect::<Vec<_>>();
            event::propose_and_commit_batch(state, events);
        }
        S1aOpV1::DamageCreaturesAndPlaneswalkersV1 { amount } => {
            let events = battlefield(state)
                .filter(|&object| {
                    has_type(state, object, CardType::Creature)
                        || has_type(state, object, CardType::Planeswalker)
                })
                .map(|object| event::damage_from_exec_ctx_v1(ctx, Target::Object(object), *amount))
                .collect::<Vec<_>>();
            event::propose_and_commit_batch(state, events);
        }
        S1aOpV1::DestroyArtifactsAtMostV1 { limit } => destroy_artifacts(state, *limit),
        S1aOpV1::DestroyArtifactsAtMostXV1 => destroy_artifacts(state, ctx.x_value),
        S1aOpV1::DestroyArtifactsAndEnchantmentsV1 => {
            let victims = battlefield(state)
                .filter(|&object| {
                    has_type(state, object, CardType::Artifact)
                        || has_type(state, object, CardType::Enchantment)
                })
                .filter(|&object| !indestructible(state, object))
                .collect::<Vec<_>>();
            destroy_all(state, victims);
        }
        S1aOpV1::MassacreV1 => {
            let creatures = battlefield(state)
                .filter(|&object| has_type(state, object, CardType::Creature))
                .collect::<Vec<_>>();
            if creatures.is_empty() {
                return;
            }
            let timestamp = crate::engine::next_timestamp(state);
            for object in creatures {
                let zone_change_count = state.objects.get(object).zone_change_count;
                state.engine.until_end_of_turn.push(
                    crate::engine::UntilEndOfTurnEffect::ResolvedObjectEffect {
                        object_id: object,
                        object_zone_change_count: zone_change_count,
                        layer: crate::engine::Layers::POWER_TOUGHNESS,
                        timestamp,
                        duration: crate::engine::EffectDuration::EndOfTurn,
                        power: -2,
                        toughness: -2,
                        grant_haste: false,
                    },
                );
            }
        }
        S1aOpV1::FluteNamedV1 { named } => {
            // As it enters: the resolving permanent spell enters, then
            // records its name.
            if state
                .objects
                .try_get(ctx.source)
                .is_some_and(|live| live.zone == Zone::Stack)
            {
                crate::effect::execute(
                    &EffectOp::MoveObject {
                        object: ObjectRef::ThisSource,
                        to_zone: Zone::Battlefield,
                    },
                    ctx,
                    state,
                );
            }
            let Some(live) = state.objects.try_get(ctx.source) else {
                return;
            };
            if live.zone != Zone::Battlefield {
                return;
            }
            let link = live_link(state, ctx.source);
            s1a_state(state).flute_names.push((link, *named));
        }
        S1aOpV1::CounterSpellItemV1 { spell } => counter_spell_item(state, *spell, ctx.source),
        S1aOpV1::SurgicalExtractionV1 => surgical_extraction_resolve(ctx, state),
        S1aOpV1::CarpetManaV1 { color, amount } => {
            let Some(contract) = ctx.ability_source_contract else {
                halt(state, ctx.source);
                return;
            };
            let islands = statics::islands_controlled_by(state, ctx.controller.opponent());
            let amount = (*amount).min(islands);
            if amount == 0 {
                return;
            }
            event::propose_and_commit(
                state,
                ProposedEvent::mana_add(ctx.controller, vec![*color; usize::from(amount)]),
            );
            let link = ObjectLinkV4 {
                object: contract.source,
                zone_change_count: contract.zone_change_count,
            };
            let turn = state.turn;
            let turns = &mut s1a_state(state).carpet_turns;
            turns.retain(|(carpet, _)| *carpet != link);
            turns.push((link, turn));
        }
        S1aOpV1::LeylineEnterV1 => {
            let Some(contract) = ctx.ability_source_contract else {
                halt(state, ctx.source);
                return;
            };
            let in_hand = state.objects.try_get(contract.source).is_some_and(|live| {
                live.zone == Zone::Hand && live.zone_change_count == contract.zone_change_count
            });
            if in_hand {
                event::propose_and_commit(
                    state,
                    ProposedEvent::zone_change(contract.source, Zone::Battlefield),
                );
            }
        }
    }
}

/// Destroys each artifact with mana value `limit` or less.
fn destroy_artifacts(state: &mut GameState, limit: u16) {
    let victims = battlefield(state)
        .filter(|&object| has_type(state, object, CardType::Artifact))
        .filter(|&object| crate::engine::object_mana_value(state, object) <= limit)
        .filter(|&object| !indestructible(state, object))
        .collect::<Vec<_>>();
    destroy_all(state, victims);
}

/// Counters `item` without targeting it, if it is still a spell on the
/// stack that can be countered.
fn counter_spell_item(state: &mut GameState, item: StackItemId, source: ObjectId) {
    let Some(spell) = state
        .stack
        .iter()
        .find(|entry| entry.v4.stack_item_id == item && entry.kind == StackItemKind::Spell)
        .map(|entry| entry.source)
    else {
        return;
    };
    let uncounterable = state
        .objects
        .try_get(spell)
        .and_then(|live| CARD_DEFS.get(usize::from(live.card_def)))
        .is_some_and(|def| def.spell_cannot_be_countered)
        || crate::eternal_cards_v1::e3::spell_cannot_be_countered(state, spell);
    if uncounterable {
        return;
    }
    if crate::engine::counter_stack_item_by_id(state, item).is_err() {
        halt(state, source);
    }
}

/// Surgical Extraction's resolution: the target is a card in a graveyard
/// other than a basic land card. Every card with its name in its owner's
/// graveyard, hand and library is exiled (the searcher takes them all), then
/// that library is shuffled.
fn surgical_extraction_resolve(ctx: &ExecCtx, state: &mut GameState) {
    let spec = TargetSpec::StandardV1(
        crate::standard_cards_v1::StandardTargetV1::NonbasicLandCardInAGraveyardV1,
    );
    let Some(Target::Object(target)) = ctx.targets.first().copied() else {
        return;
    };
    if !(ctx.target_incarnation_matches(0, state)
        && crate::engine::effect_target_is_legal(
            state,
            ctx.source,
            ctx.controller,
            spec,
            &ctx.targets,
            0,
        ))
    {
        return;
    }
    let live = state.objects.get(target);
    let owner = live.owner;
    let Some(name) = CARD_DEFS
        .get(usize::from(live.card_def))
        .map(|def| def.object_name)
    else {
        return;
    };
    let seat = &state.players[owner.index()];
    let cards = seat
        .graveyard
        .iter()
        .chain(seat.hand.iter())
        .chain(seat.library.iter())
        .copied()
        .filter(|&card| {
            let live = state.objects.get(card);
            !live.v4.is_token
                && live.spell_copy_origin.is_none()
                && CARD_DEFS
                    .get(usize::from(live.card_def))
                    .is_some_and(|def| def.object_name == name)
        })
        .collect::<Vec<_>>();
    event::propose_and_commit_batch(
        state,
        cards
            .into_iter()
            .map(|card| ProposedEvent::zone_change(card, Zone::Exile))
            .collect(),
    );
    if state.shuffle_library(owner).is_err() {
        halt(state, ctx.source);
    }
}

// ---- Delayed triggers ---------------------------------------------------------------

/// The triggers this collection puts on the stack that no printed trigger
/// condition matches: Leyline of the Void's opening-hand permission, offered
/// for each Leyline in a hand as the game's first upkeep begins, before
/// anyone could have cast a spell, and Lavinia's counter for a spell cast
/// with no mana spent.
pub(crate) fn delayed_triggers(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Vec<PendingTrigger> {
    let mut triggers = leyline_triggers(state, events);
    triggers.extend(lavinia_triggers(state, events));
    triggers
}

fn leyline_triggers(state: &mut GameState, events: &[CommittedEvent]) -> Vec<PendingTrigger> {
    let first_upkeep = state.turn == 1
        && state.active_player == state.starting_player
        && state.extra_turn_v1.is_none()
        && events
            .iter()
            .any(|event| matches!(event, CommittedEvent::UpkeepBegan { .. }));
    if !first_upkeep || read_state(state).is_some_and(|s1a| s1a.leylines_offered) {
        return Vec::new();
    }
    let leylines = [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(|player| state.players[player.index()].hand.iter().copied())
        .filter(|&card| {
            CARD_DEFS
                .get(usize::from(state.objects.get(card).card_def))
                .is_some_and(|def| def.name == LEYLINE_OF_THE_VOID && def.is_executable())
        })
        .collect::<Vec<_>>();
    if leylines.is_empty() {
        return Vec::new();
    }
    s1a_state(state).leylines_offered = true;
    leylines
        .into_iter()
        .map(|card| {
            let source = AbilitySourceContractV4::capture(state, card);
            crate::eternal_cards_v1::delayed_trigger(source, source.owner, leyline_choice())
        })
        .collect()
}

/// Lavinia, Azorius Renegade: "Whenever an opponent casts a spell, if no
/// mana was spent to cast it, counter that spell."
fn lavinia_triggers(state: &mut GameState, events: &[CommittedEvent]) -> Vec<PendingTrigger> {
    let Some(recorded) = read_state(state) else {
        return Vec::new();
    };
    if recorded.unpaid_casts.is_empty() {
        return Vec::new();
    }
    let unpaid = recorded.unpaid_casts.clone();
    let mut triggers = Vec::new();
    for event in events {
        let CommittedEvent::SpellCast {
            spell,
            controller: caster,
        } = event
        else {
            continue;
        };
        let Some(live) = state.objects.try_get(*spell) else {
            continue;
        };
        let link = ObjectLinkV4 {
            object: *spell,
            zone_change_count: live.zone_change_count,
        };
        if !unpaid.contains(&link) {
            continue;
        }
        let Some(item) = state
            .stack
            .iter()
            .find(|item| item.source == *spell && item.kind == StackItemKind::Spell)
            .map(|item| item.v4.stack_item_id)
        else {
            continue;
        };
        let lavinias = statics::active_controlled_by(state, LAVINIA, caster.opponent());
        for lavinia in lavinias {
            let source = AbilitySourceContractV4::capture(state, lavinia);
            triggers.push(crate::eternal_cards_v1::delayed_trigger(
                source,
                source.owner,
                s1a(S1aOpV1::CounterSpellItemV1 { spell: item }),
            ));
        }
    }
    // Spells that left the stack no longer matter.
    let on_stack = state
        .stack
        .iter()
        .filter(|item| item.kind == StackItemKind::Spell)
        .map(|item| item.source)
        .collect::<Vec<_>>();
    let kept = unpaid
        .iter()
        .copied()
        .filter(|link| {
            on_stack.contains(&link.object)
                && state
                    .objects
                    .try_get(link.object)
                    .is_some_and(|live| live.zone_change_count == link.zone_change_count)
        })
        .collect::<Vec<_>>();
    if kept.len() != unpaid.len() {
        s1a_state(state).unpaid_casts = kept;
    }
    triggers
}

/// A main phase began for the active player: Carpet of Flowers' trigger for
/// each Carpet they control that has not added mana this turn.
pub(crate) fn begin_main_phase(state: &mut GameState) -> Vec<PendingTrigger> {
    let player = state.active_player;
    let islands = statics::islands_controlled_by(state, player.opponent());
    if islands == 0 {
        return Vec::new();
    }
    let turn = state.turn;
    let carpets = statics::active_controlled_by(state, CARPET_OF_FLOWERS, player)
        .into_iter()
        .filter(|&carpet| {
            let link = live_link(state, carpet);
            !read_state(state).is_some_and(|s1a| s1a.carpet_turns.contains(&(link, turn)))
        })
        .collect::<Vec<_>>();
    carpets
        .into_iter()
        .map(|carpet| {
            let source = AbilitySourceContractV4::capture(state, carpet);
            crate::eternal_cards_v1::delayed_trigger(source, player, carpet_choice(islands))
        })
        .collect()
}
