//! Card programs for the eternal catalog's Pauper sideboard cards (wave S1d,
//! `docs/design/eternal_formats_support_v1.md`): the 31 cards that complete
//! the sideboards of the Pauper metagame decks (Vandalblast, Crypt
//! Incursion, Circle of Protection: Blue, Standard Bearer, Electrickery,
//! Spreading Seas and the rest).
//!
//! `eternal_cards_v1` forwards its effect leaves and state here, and the
//! engine calls the hooks below only in `eternal-fixtures` builds. Every
//! table is keyed by a printed name that occurs only in the eternal registry,
//! so the Pauper, FDN and Standard catalogs never reach it. Behavior was read
//! from each card's current Oracle text and, where present, its XMage source.
//!
//! New rules here: overload (the Adventure/split form of a card cast from
//! hand, with the printed mana value kept on the stack), Circle of
//! Protection's source-specific "next damage" shield, Flagbearer's targeting
//! restriction, an opponent skipping their next combat phase, an extra land
//! type until end of turn (Navigator's Compass), "enchanted land is an
//! Island" (Spreading Seas), waterbend (tap artifacts and creatures to pay
//! generic mana), and revealing X white cards from the hand as part of an
//! activation (Martyr of Sands).
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{CardDef, CardType, Keywords, Subtype, TargetSpec, CARD_DEFS};
use crate::effect::{
    CreatureFilter, EffectContinuation, EffectFrame, EffectObjectBinding, EffectOp,
    EffectOptionChoicePurpose, ExecCtx, ObjectRef, PendingEffectChoice, PlayerRef, TargetRef,
};
use crate::eternal_cards_v1::EternalOpV1;
use crate::event::{self, DamageProposed, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::{ManaColor, Pip};
use crate::standard_cards_v1::{StandardChosenActionV1, StandardOpV1, StandardPermanentFilterV1};
use crate::state::{
    GameState, ObjectLinkV4, StackItemKind, StackTargetContractV4, Step, Target, Zone,
};
use crate::trigger::{TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

// ---- State ------------------------------------------------------------------------

/// Per-game state for this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Default (and absent from snapshots)
/// until one of these cards needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct S1dStateV1 {
    /// Circle of Protection: Blue's shields for this turn.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cop_shields: Vec<CopShieldV1>,
    /// Players who skip their next combat phase (Stonehorn Dignitary).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub combat_skips: Vec<PlayerId>,
    /// Lands that became a basic land type in addition to their other types
    /// this turn (Navigator's Compass).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra_land_types: Vec<ExtraLandTypeV1>,
}

impl Hash for S1dStateV1 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        if !self.is_empty() {
            b"eternal-sideboard-s1d/v1".hash(state);
            self.cop_shields.hash(state);
            self.combat_skips.hash(state);
            self.extra_land_types.hash(state);
        }
    }
}

impl S1dStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// The turn an effect was created in: its key and active player.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TurnV1 {
    pub turn: u32,
    pub active: PlayerId,
}

impl TurnV1 {
    fn now(state: &GameState) -> Self {
        Self {
            turn: state.turn_key_v1(),
            active: state.active_player,
        }
    }
}

/// "The next time a blue source of your choice would deal damage to you this
/// turn, prevent that damage."
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CopShieldV1 {
    /// The chosen source, by object incarnation.
    pub source: ObjectLinkV4,
    /// The protected player.
    pub player: PlayerId,
    pub turn: TurnV1,
    /// A chosen permanent spell also protects against the permanent it becomes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spell: Option<CopSpellV1>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CopSpellV1 {
    pub item: crate::ids::StackItemId,
    pub cast_history_index: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permanent: Option<ObjectLinkV4>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_history_index: Option<u64>,
}

impl Hash for CopShieldV1 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.source.hash(state);
        self.player.hash(state);
        self.turn.hash(state);
        if let Some(spell) = self.spell {
            b"circle-permanent-spell/v1".hash(state);
            spell.hash(state);
        }
    }
}

/// One land and the basic land type it gained until end of turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtraLandTypeV1 {
    pub land: ObjectLinkV4,
    pub subtype: Subtype,
    pub turn: TurnV1,
}

fn s1d_state(state: &mut GameState) -> &mut S1dStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).s1d
}

fn read_state(state: &GameState) -> Option<&S1dStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.s1d)
}

fn circle_cast_matches(state: &GameState, source: ObjectLinkV4, spell: CopSpellV1) -> bool {
    let Ok(at) = usize::try_from(spell.cast_history_index) else {
        return false;
    };
    let Some(before) = at.checked_sub(1) else {
        return false;
    };
    let history = &state.engine.event_history;
    let Some(event::CommittedEvent::PublishedCastIdentityV1 { receipt }) = history.get(before)
    else {
        return false;
    };
    receipt.spell_cast_history_index == spell.cast_history_index
        && receipt.spell == source.object
        && receipt.stack_zone_change_count == source.zone_change_count
        && receipt.stack_item_id == spell.item
        && receipt.stack_item_id != crate::ids::StackItemId::default()
        && receipt.origin_zone != Zone::Stack
        && receipt.origin_zone_change_count.checked_add(1) == Some(source.zone_change_count)
        && history.get(at)
            == Some(&event::CommittedEvent::SpellCast {
                spell: source.object,
                controller: receipt.controller,
            })
}

fn circle_entry_matches(state: &GameState, source: ObjectLinkV4, spell: CopSpellV1) -> bool {
    let (Some(permanent), Some(entry)) = (spell.permanent, spell.entry_history_index) else {
        return false;
    };
    if permanent.object != source.object
        || source.zone_change_count.checked_add(1) != Some(permanent.zone_change_count)
        || !circle_cast_matches(state, source, spell)
    {
        return false;
    }
    let (Ok(cast), Ok(entry)) = (
        usize::try_from(spell.cast_history_index),
        usize::try_from(entry),
    ) else {
        return false;
    };
    let history = &state.engine.event_history;
    entry > cast && matches!(history.get(entry), Some(event::CommittedEvent::ZoneChange {
        object, from: Zone::Stack, to: Zone::Battlefield, ..
    }) if *object == source.object)
        && !history.iter().skip(cast + 1).take(entry - cast - 1)
            .any(|event| matches!(event, event::CommittedEvent::ZoneChange { object, .. } if *object == source.object))
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::S1dV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum S1dOpV1 {
    /// Vandalblast's overload: destroy each artifact the controller doesn't
    /// control.
    DestroyOpponentArtifactsV1,
    /// Crypt Incursion: exile all creature cards from target player's
    /// graveyard and gain 3 life for each.
    CryptIncursionV1,
    /// Circle of Protection: Blue: choose a blue source. A choice.
    CopChooseV1,
    /// Circle of Protection: Blue's shield against `source`.
    CopShieldV1 { source: ObjectLinkV4 },
    /// 1 damage to each creature with flying (Scattershot Archer).
    DamageFlyingV1,
    /// Sagittars' Volley: destroy target 0, then 1 damage to each creature
    /// with flying the controller's opponents control.
    SagittarsVolleyV1,
    /// Last Breath: exile target 0; its controller gains 4 life.
    LastBreathV1,
    /// A creature P/T change until end of turn for each creature (or each
    /// nonwhite creature) on the battlefield.
    BoostAllV1 {
        power: i32,
        toughness: i32,
        nonwhite_only: bool,
    },
    /// Nylea's Disciple: gain life equal to the controller's devotion to
    /// green.
    DevotionLifeV1,
    /// Stonehorn Dignitary: target opponent skips their next combat phase.
    SkipCombatV1,
    /// Deglamer: target 0's owner shuffles it into their library.
    DeglamerV1,
    /// Navigator's Compass: choose the basic land type target 0 becomes. A
    /// choice.
    CompassChooseV1,
    /// Navigator's Compass: `land` becomes `subtype` until end of turn.
    CompassBecomeV1 {
        land: ObjectLinkV4,
        subtype: Subtype,
    },
    /// Return to Dust: exile target 0, and if cast during its controller's
    /// main phase, may exile target 1. A choice.
    ReturnToDustV1,
    /// Return to Dust's second exile.
    ExileSecondTargetV1,
    /// Waterbending Lesson: discard a card unless the controller waterbends
    /// {2}. A choice.
    WaterbendOrDiscardV1,
    /// Waterbend: `remaining` generic mana still to pay. A choice.
    WaterbendStepV1 { remaining: u8 },
    /// Waterbend: tap `object` to pay {1}.
    WaterbendTapV1 { object: ObjectLinkV4 },
    /// Waterbend: pay `amount` generic mana from the mana sources.
    WaterbendManaV1 { amount: u8 },
    /// The actual reveal cost binds this single printed Martyr ability.
    MartyrRevealV1,
}

pub(crate) mod martyr;

impl S1dOpV1 {
    /// Whether this leaf can yield a player decision, so its program must
    /// use the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(
            self,
            Self::CopChooseV1
                | Self::CompassChooseV1
                | Self::ReturnToDustV1
                | Self::WaterbendOrDiscardV1
                | Self::WaterbendStepV1 { .. }
        )
    }
}

// ---- Names and helpers --------------------------------------------------------------

const ARASHIN_CLERIC: &str = "Arashin Cleric";
const COALITION_HONOR_GUARD: &str = "Coalition Honor Guard";
const FOOT_NINJAS: &str = "Foot Ninjas";
const GRUUL_TURF: &str = "Gruul Turf";
const NAVIGATORS_COMPASS: &str = "Navigator's Compass";
const NYLEAS_DISCIPLE: &str = "Nylea's Disciple";
const PALACE_SENTINELS: &str = "Palace Sentinels";
const SPREADING_SEAS: &str = "Spreading Seas";
const STANDARD_BEARER: &str = "Standard Bearer";
const STONEHORN_DIGNITARY: &str = "Stonehorn Dignitary";
const VANDALBLAST: &str = "Vandalblast";
const ELECTRICKERY: &str = "Electrickery";

fn s1d(op: S1dOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::S1dV1(op)))
}

pub fn martyr_of_sands() -> EffectOp {
    s1d(S1dOpV1::MartyrRevealV1)
}

fn nothing() -> EffectOp {
    EffectOp::Sequence(Vec::new())
}

fn live_link(state: &GameState, object: ObjectId) -> ObjectLinkV4 {
    ObjectLinkV4 {
        object,
        zone_change_count: state.objects.get(object).zone_change_count,
    }
}

fn link_live_in(state: &GameState, link: ObjectLinkV4, zone: Zone) -> bool {
    state
        .objects
        .try_get(link.object)
        .is_some_and(|live| live.zone == zone && live.zone_change_count == link.zone_change_count)
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

/// Whether `object` is a battlefield permanent named `name` showing its
/// front face, whose printed abilities apply.
fn active_named(state: &GameState, object: ObjectId, name: &str) -> bool {
    state.objects.try_get(object).is_some_and(|live| {
        live.zone == Zone::Battlefield
            && live.v4.face_index == 0
            && CARD_DEFS
                .get(live.card_def as usize)
                .is_some_and(|def| def.name == name && def.is_executable())
            && crate::continuous_characteristics_v1::printed_abilities_active(state, object)
    })
}

/// Every battlefield object, in each player's battlefield order.
fn battlefield(state: &GameState) -> impl Iterator<Item = ObjectId> + '_ {
    [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(move |player| state.players[player.index()].battlefield.iter().copied())
}

fn has_type(state: &GameState, object: ObjectId, card_type: CardType) -> bool {
    crate::engine::object_has_type(state, object, card_type)
}

fn is_creature(state: &GameState, object: ObjectId) -> bool {
    has_type(state, object, CardType::Creature)
}

fn has_flying(state: &GameState, object: ObjectId) -> bool {
    is_creature(state, object)
        && crate::engine::has_effective_keyword(state, object, Keywords::FLYING)
}

/// Target `index` if it still names its announced incarnation, in `zone`.
fn bound_target_in(ctx: &ExecCtx, state: &GameState, index: usize, zone: Zone) -> Option<ObjectId> {
    let Some(Target::Object(object)) = ctx.targets.get(index).copied() else {
        return None;
    };
    (ctx.target_incarnation_matches(index, state) && state.objects.get(object).zone == zone)
        .then_some(object)
}

fn push_frame(continuation: &mut EffectContinuation, op: EffectOp, path: &[u16]) {
    continuation.frames.push(EffectFrame::Program {
        op,
        path: path.to_vec(),
    });
}

fn push_choice(continuation: &mut EffectContinuation, options: Vec<EffectOp>, path: &[u16]) {
    push_frame(
        continuation,
        EffectOp::Choice {
            controller: PlayerRef::Controller,
            options,
        },
        path,
    );
}

fn destroy_target() -> EffectOp {
    EffectOp::DestroyObject {
        object: ObjectRef::Target(0),
    }
}

fn gain_life(amount: i32) -> EffectOp {
    EffectOp::GainLife {
        player: PlayerRef::Controller,
        amount,
    }
}

// ---- Spells -------------------------------------------------------------------------

/// Vandalblast: "Destroy target artifact you don't control."
pub fn vandalblast() -> EffectOp {
    destroy_target()
}

/// Vandalblast's overload {4}{R}: "Destroy each artifact you don't control."
pub fn vandalblast_overload() -> EffectOp {
    s1d(S1dOpV1::DestroyOpponentArtifactsV1)
}

/// Electrickery: "Electrickery deals 1 damage to target creature you don't
/// control."
pub fn electrickery() -> EffectOp {
    EffectOp::DealDamage {
        target: TargetRef::Target(0),
        amount: 1,
    }
}

/// Electrickery's overload {1}{R}: "Electrickery deals 1 damage to each
/// creature you don't control."
pub fn electrickery_overload() -> EffectOp {
    EffectOp::DamageAllCreatures {
        filter: CreatureFilter::OpponentControlled,
        amount: 1,
    }
}

/// Crypt Incursion: "Exile all creature cards from target player's
/// graveyard. You gain 3 life for each card exiled this way."
pub fn crypt_incursion() -> EffectOp {
    s1d(S1dOpV1::CryptIncursionV1)
}

/// Circle of Protection: Blue's ability.
pub fn cop_choose() -> EffectOp {
    s1d(S1dOpV1::CopChooseV1)
}

/// Firebending Lesson: "Firebending Lesson deals 2 damage to target
/// creature. If this spell was kicked, it deals 5 damage to that creature
/// instead."
pub fn firebending_lesson() -> EffectOp {
    EffectOp::Conditional {
        cond: crate::effect::EffectCond::WasKicked,
        then: Box::new(EffectOp::DealDamage {
            target: TargetRef::Target(0),
            amount: 5,
        }),
        else_: Box::new(EffectOp::DealDamage {
            target: TargetRef::Target(0),
            amount: 2,
        }),
    }
}

/// Call Damage Control: "Choose up to two. Return those cards from your
/// graveyard to your hand: target artifact card, target creature card,
/// target enchantment card, target land card." Each chosen mode's target is
/// returned independently.
pub fn call_damage_control() -> EffectOp {
    EffectOp::MoveAllTargets {
        to_zone: Zone::Hand,
    }
}

/// Origin of Metalbending's first mode: "Destroy target artifact or
/// enchantment."
pub fn origin_of_metalbending_destroy() -> EffectOp {
    destroy_target()
}

/// Origin of Metalbending's second mode: "Put a +1/+1 counter on target
/// creature you control. It gains indestructible until end of turn."
pub fn origin_of_metalbending_counter() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::AddPlusOnePlusOneCounters {
            object: ObjectRef::Target(0),
            count: 1,
        },
        EffectOp::GrantKeywordTargetUntilEndOfTurn {
            object: ObjectRef::Target(0),
            keyword: Keywords::INDESTRUCTIBLE,
        },
    ])
}

/// Waterbending Lesson: "Draw three cards. Then discard a card unless you
/// waterbend {2}."
pub fn waterbending_lesson() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 3,
        },
        s1d(S1dOpV1::WaterbendOrDiscardV1),
    ])
}

/// Drown in Sorrow: "All creatures get -2/-2 until end of turn. Scry 1."
pub fn drown_in_sorrow() -> EffectOp {
    EffectOp::Sequence(vec![
        s1d(S1dOpV1::BoostAllV1 {
            power: -2,
            toughness: -2,
            nonwhite_only: false,
        }),
        EffectOp::Scry {
            player: PlayerRef::Controller,
            count: 1,
        },
    ])
}

/// Holy Light: "Nonwhite creatures get -1/-1 until end of turn."
pub fn holy_light() -> EffectOp {
    s1d(S1dOpV1::BoostAllV1 {
        power: -1,
        toughness: -1,
        nonwhite_only: true,
    })
}

/// Deglamer: "Choose target artifact or enchantment. Its owner shuffles it
/// into their library."
pub fn deglamer() -> EffectOp {
    s1d(S1dOpV1::DeglamerV1)
}

/// Last Breath: "Exile target creature with power 2 or less. Its controller
/// gains 4 life."
pub fn last_breath() -> EffectOp {
    s1d(S1dOpV1::LastBreathV1)
}

/// Faerie Trickery: "Counter target non-Faerie spell. If that spell is
/// countered this way, exile it instead of putting it into its owner's
/// graveyard."
pub fn faerie_trickery() -> EffectOp {
    EffectOp::Conditional {
        cond: crate::effect::EffectCond::TargetSpellCanBeCountered(0),
        then: Box::new(EffectOp::MoveObject {
            object: ObjectRef::Target(0),
            to_zone: Zone::Exile,
        }),
        else_: Box::new(nothing()),
    }
}

/// Sagittars' Volley: "Destroy target creature with flying. Sagittars' Volley
/// deals 1 damage to each creature with flying your opponents control."
pub fn sagittars_volley() -> EffectOp {
    s1d(S1dOpV1::SagittarsVolleyV1)
}

/// Ultimate Alliance: "Ultimate Alliance deals damage equal to the number of
/// creatures you control to target creature."
pub fn ultimate_alliance() -> EffectOp {
    EffectOp::DealDamageByControlledCreatureCount {
        target: TargetRef::Target(0),
        multiplier: 1,
    }
}

/// Return to Dust: "Exile target artifact or enchantment. If you cast this
/// spell during your main phase, you may exile up to one other target
/// artifact or enchantment."
pub fn return_to_dust() -> EffectOp {
    s1d(S1dOpV1::ReturnToDustV1)
}

// ---- Activated abilities ------------------------------------------------------------

/// Scattershot Archer: "{T}: Scattershot Archer deals 1 damage to each
/// creature with flying."
pub fn scattershot_archer() -> EffectOp {
    s1d(S1dOpV1::DamageFlyingV1)
}

/// Navigator's Compass: "{T}: Until end of turn, target land you control
/// becomes the basic land type of your choice in addition to its other
/// types."
pub fn navigators_compass() -> EffectOp {
    s1d(S1dOpV1::CompassChooseV1)
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

fn gain_three() -> EffectOp {
    gain_life(3)
}

fn become_monarch() -> EffectOp {
    EffectOp::BecomeMonarch
}

fn draw_one() -> EffectOp {
    EffectOp::DrawCards {
        player: PlayerRef::Controller,
        count: 1,
    }
}

fn devotion_life() -> EffectOp {
    s1d(S1dOpV1::DevotionLifeV1)
}

fn skip_combat() -> EffectOp {
    s1d(S1dOpV1::SkipCombatV1)
}

/// The karoos: "return a land you control to its owner's hand." Not
/// targeted; chosen on resolution, and it may be this land.
fn return_controlled_land() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::PlayerChoosesControlledPermanent {
        player: PlayerRef::Controller,
        filter: StandardPermanentFilterV1::Land,
        action: StandardChosenActionV1::ReturnToOwnersHand,
    })
}

const GAIN_THREE_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, gain_three)];
const MONARCH_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, become_monarch)];
const DRAW_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, draw_one)];
const DEVOTION_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, devotion_life)];
const SKIP_COMBAT_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, skip_combat)];
const KAROO_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, return_controlled_land)];

/// Triggered abilities of this module's cards.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        ARASHIN_CLERIC | FOOT_NINJAS | NAVIGATORS_COMPASS => &GAIN_THREE_TRIGGERS,
        PALACE_SENTINELS => &MONARCH_TRIGGERS,
        SPREADING_SEAS => &DRAW_TRIGGERS,
        NYLEAS_DISCIPLE => &DEVOTION_TRIGGERS,
        STONEHORN_DIGNITARY => &SKIP_COMBAT_TRIGGERS,
        GRUUL_TURF => &KAROO_TRIGGERS,
        _ => crate::eternal_sideboard_s1b_v1::triggers_for(name),
    }
}

/// Target specs of this module's triggered abilities, per ability. `None`
/// for every other card.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    let own = triggers_for(name)
        .iter()
        .any(|trigger| (trigger.effect)() == *effect);
    if !own {
        return None;
    }
    Some(match name {
        STONEHORN_DIGNITARY => TargetSpec::TargetOpponent,
        _ => TargetSpec::None,
    })
}

// ---- Effect leaves ------------------------------------------------------------------

fn blue_mask() -> u8 {
    crate::card_def::mana_color_mask(ManaColor::U)
}

fn white_mask() -> u8 {
    crate::card_def::mana_color_mask(ManaColor::W)
}

/// Devotion to green: the {G} symbols (hybrid and Phyrexian included) in
/// the mana costs of permanents `player` controls. Permanents showing a face
/// without a mana cost count nothing.
fn devotion_to_green(state: &GameState, player: PlayerId) -> i32 {
    let green = |pip: &Pip| match *pip {
        Pip::Colored(color) | Pip::Phyrexian(color) | Pip::PhyrexianAnyColor(color) => {
            color == ManaColor::G
        }
        Pip::Hybrid(first, second) => first == ManaColor::G || second == ManaColor::G,
    };
    let total = state.players[player.index()]
        .battlefield
        .iter()
        .map(|&permanent| {
            let object = state.objects.get(permanent);
            if object.v4.face_index != 0 || object.v4.face_down_v1.is_some() {
                return 0;
            }
            CARD_DEFS.get(object.card_def as usize).map_or(0, |def| {
                def.cost.pips.iter().filter(|pip| green(pip)).count()
            })
        })
        .sum::<usize>();
    i32::try_from(total).unwrap_or(i32::MAX)
}

fn target_source_link(target: StackTargetContractV4) -> Option<ObjectLinkV4> {
    let (object, zone_change_count) = match target {
        StackTargetContractV4::Object {
            object,
            zone_change_count,
            ..
        } => (object, zone_change_count),
        StackTargetContractV4::StackItem {
            source,
            source_zone_change_count,
            ..
        } => (source, source_zone_change_count),
        StackTargetContractV4::Player(_) => return None,
    };
    Some(ObjectLinkV4 {
        object,
        zone_change_count,
    })
}

fn stacked_delayed_source_link(effect: &EffectOp) -> Option<ObjectLinkV4> {
    use crate::eternal_legacy_e5b_v1::E5bOpV1;
    use crate::eternal_legacy_e7a_v1::E7aOpV1;
    use crate::eternal_pauper_e6a_v1::E6aOpV1;
    use crate::eternal_premodern_p1a_v1::P1aOpV1;
    use crate::eternal_vintage_e7b_v1::E7bOpV1;

    let EffectOp::StandardV1(StandardOpV1::EternalV1(op)) = effect else {
        return None;
    };
    match op {
        EternalOpV1::E5bV1(E5bOpV1::PheliaReturnV1 { card })
        | EternalOpV1::E6aV1(E6aOpV1::ReboundV1 { card })
        | EternalOpV1::E7aV1(E7aOpV1::SuspendTickV1 { card })
        | EternalOpV1::E7aV1(E7aOpV1::ShallowGraveExileV1 { creature: card })
        | EternalOpV1::E7bV1(E7bOpV1::EndStepReturnV1 { card, .. })
        | EternalOpV1::P1aV1(P1aOpV1::BrushhopperReturnV1 { card }) => Some(*card),
        _ => None,
    }
}

/// CR 609.7a. A public zone alone does not make a graveyard or exile card a
/// source choice. Require an exact active reference or a direct eligible zone.
fn blue_sources(state: &GameState) -> Vec<ObjectLinkV4> {
    use crate::eternal_legacy_e8a_v1::{link_live_in, E8aOpV1};

    let mask = blue_mask();
    let mut sources = Vec::new();
    let mut include_live = |source: ObjectLinkV4| {
        if state.objects.try_get(source.object).is_some_and(|live| {
            live.zone_change_count == source.zone_change_count
                && match live.zone {
                    Zone::Battlefield | Zone::Stack | Zone::Graveyard => true,
                    Zone::Exile | Zone::Command => live.v4.face_down_v1.is_none(),
                    _ => false,
                }
                && crate::engine::object_color_mask(state, source.object) & mask != 0
        }) {
            sources.push(source);
        }
    };

    let direct = battlefield(state)
        .map(|object| (object, Zone::Battlefield))
        .chain(
            state
                .command
                .iter()
                .copied()
                .map(|object| (object, Zone::Command)),
        );
    for (object, zone) in direct {
        if state
            .objects
            .try_get(object)
            .is_some_and(|live| live.zone == zone)
        {
            include_live(live_link(state, object));
        }
    }
    for item in &state.stack {
        if item.kind == StackItemKind::Spell {
            if let Some(source) = item.v4.source_contract {
                include_live(ObjectLinkV4 {
                    object: source.source,
                    zone_change_count: source.zone_change_count,
                });
            }
        }
        if let Some(source) = item.v4.ability_source_contract {
            include_live(ObjectLinkV4 {
                object: source.source,
                zone_change_count: source.zone_change_count,
            });
        }
        for source in item
            .v4
            .target_contracts
            .iter()
            .copied()
            .filter_map(target_source_link)
        {
            include_live(source);
        }
        if item.kind == StackItemKind::TriggeredAbility {
            if let Some(source) = item
                .inline_effect
                .as_ref()
                .and_then(stacked_delayed_source_link)
            {
                include_live(source);
            }
        }
    }
    let published_returns = state.stack.iter().filter_map(|item| {
        if item.kind != StackItemKind::TriggeredAbility {
            return None;
        }
        match item.inline_effect.as_ref()? {
            EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E8aV1(
                E8aOpV1::ReturnExiledV1 { cards },
            ))) => Some(cards),
            _ => None,
        }
    });
    let waiting_returns = state.eternal_v1.iter().flat_map(|eternal| {
        eternal.e8a.end_step_returns.iter().map(|pending| &pending.cards)
    });
    for &card in waiting_returns.chain(published_returns).flatten() {
        if link_live_in(state, card, Zone::Exile) {
            include_live(card);
        }
    }
    for replacement in &state.engine.active_replacements {
        use event::ReplacementEffectKind as R;
        match replacement.kind {
            R::PreventCombatDamageToObjectUntilEndOfTurn {
                object,
                turn,
                active_player,
            } if turn == state.turn_key_v1() && active_player == state.active_player => {
                include_live(object)
            }
            R::ExileIfDiesUntilEndOfTurnV1 {
                target,
                turn,
                active_player,
                ..
            } if turn == state.turn_key_v1() && active_player == state.active_player => {
                if let Some(source) = target_source_link(target) {
                    include_live(source);
                }
            }
            _ => {}
        }
    }
    if let Some(eternal) = &state.eternal_v1 {
        for rebound in &eternal.e6a.rebounds {
            include_live(rebound.card);
        }
        for delayed in &eternal.e5b.phelia_returns {
            include_live(delayed.card);
        }
        for delayed in &eternal.p1a.end_step_returns {
            include_live(delayed.card);
        }
        for delayed in &eternal.e7b.end_step_returns {
            include_live(delayed.card);
        }
        for delayed in &eternal.e7a.shallow_graves {
            include_live(delayed.creature);
        }
        for &card in &eternal.e7a.suspended {
            include_live(card);
        }
        for shield in eternal
            .s1d
            .cop_shields
            .iter()
            .filter(|shield| shield.turn == TurnV1::now(state))
        {
            include_live(shield.source);
        }
    }

    // A departed damage source keeps its original public incarnation and
    // authenticated last-known color. Do not read a later incarnation.
    for item in &state.stack {
        let Some(source) = item.v4.ability_source_contract else {
            continue;
        };
        if source.zone == Zone::Battlefield
            && state
                .objects
                .try_get(source.source)
                .is_some_and(|live| live.zone_change_count > source.zone_change_count)
            && crate::source_power_activation_v1::damage_source(
                state,
                source.source,
                item.v4.stack_item_id,
            )
            .is_ok_and(|facts| facts.source == source && facts.color_mask & mask != 0)
        {
            sources.push(ObjectLinkV4 {
                object: source.source,
                zone_change_count: source.zone_change_count,
            });
        }
    }
    sources.sort_unstable_by_key(|source| (source.object, source.zone_change_count));
    sources.dedup();
    sources
}

fn circle_options(state: &GameState) -> Vec<EffectOp> {
    blue_sources(state)
        .into_iter()
        .map(|source| s1d(S1dOpV1::CopShieldV1 { source }))
        .collect()
}

pub(crate) fn validate_circle_choice(
    state: &GameState,
    pending: &EffectContinuation,
) -> Result<(), String> {
    if pending.resolving_item.inline_effect.as_ref() != Some(&cop_choose()) {
        return Ok(());
    }
    let options = circle_options(state);
    let valid = match &pending.choice {
        Some(PendingEffectChoice::ChooseOption {
            player,
            path,
            options: selected,
            purpose: EffectOptionChoicePurpose::Generic,
        }) => {
            *player == pending.ctx.controller
                && path.is_empty()
                && pending.frames.is_empty()
                && !options.is_empty()
                && *selected == options
        }
        None => match pending.frames.as_slice() {
            [EffectFrame::Program { op, path }] => match path.as_slice() {
                [index] => options.get(usize::from(*index)) == Some(op),
                _ => false,
            },
            _ => false,
        },
        _ => false,
    };
    if !valid {
        return Err("Circle source choice no longer matches its exact public menu".into());
    }
    Ok(())
}

/// Untapped artifacts and creatures `player` controls, which waterbend may
/// tap.
fn waterbend_permanents(state: &GameState, player: PlayerId) -> Vec<ObjectId> {
    state.players[player.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&object| {
            let live = state.objects.get(object);
            !live.tapped
                && (is_creature(state, object) || has_type(state, object, CardType::Artifact))
        })
        .collect()
}

/// Whether `player` can still pay `remaining` generic mana of a waterbend,
/// tapping artifacts and creatures and spending mana.
fn waterbend_finishable(state: &GameState, player: PlayerId, remaining: u8) -> bool {
    if remaining == 0 {
        return true;
    }
    if crate::engine::can_pay_effect_mana(player, &[], remaining, state) {
        return true;
    }
    waterbend_permanents(state, player)
        .into_iter()
        .any(|object| {
            let mut simulated = state.clone();
            simulated.objects.get_mut(object).tapped = true;
            waterbend_finishable(&simulated, player, remaining - 1)
        })
}

pub(crate) fn execute(op: &S1dOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        S1dOpV1::DestroyOpponentArtifactsV1 => {
            let candidates = battlefield(state)
                .filter(|&object| {
                    state.objects.get(object).controller != ctx.controller
                        && has_type(state, object, CardType::Artifact)
                        && !crate::engine::has_effective_keyword(
                            state,
                            object,
                            Keywords::INDESTRUCTIBLE,
                        )
                })
                .collect::<Vec<_>>();
            let mut destroyed = Vec::new();
            for object in candidates {
                if !crate::eternal_vintage_e7b_v1::statics::regenerate_instead(state, object) {
                    destroyed.push(object);
                }
            }
            event::propose_and_commit_batch(
                state,
                destroyed
                    .into_iter()
                    .map(|object| ProposedEvent::zone_change(object, Zone::Graveyard))
                    .collect(),
            );
        }
        S1dOpV1::CryptIncursionV1 => {
            let Some(&Target::Player(player)) = ctx.targets.first() else {
                return;
            };
            let cards = state.players[player.index()]
                .graveyard
                .iter()
                .copied()
                .filter(|&card| {
                    definition(state, card).is_some_and(|def| def.has_type(CardType::Creature))
                        && !state.objects.get(card).v4.is_token
                })
                .collect::<Vec<_>>();
            if cards.is_empty() {
                return;
            }
            event::propose_and_commit_batch(
                state,
                cards
                    .iter()
                    .map(|&card| ProposedEvent::zone_change(card, Zone::Exile))
                    .collect(),
            );
            let exiled = cards
                .iter()
                .filter(|&&card| state.objects.get(card).zone == Zone::Exile)
                .count();
            let amount = i32::try_from(exiled)
                .unwrap_or(i32::MAX / 3)
                .saturating_mul(3);
            if amount > 0 {
                event::propose_and_commit(state, ProposedEvent::life_gain(ctx.controller, amount));
            }
        }
        S1dOpV1::CopShieldV1 { source } => {
            let now = TurnV1::now(state);
            let spell = state
                .objects
                .try_get(source.object)
                .filter(|live| {
                    live.zone == Zone::Stack && live.zone_change_count == source.zone_change_count
                })
                .and_then(|_| {
                    state.stack.iter().find(|item| {
                        item.source == source.object
                            && item.kind == crate::state::StackItemKind::Spell
                    })
                })
                .and_then(|item| {
                    let mut receipts = state
                        .engine
                        .event_history
                        .scan_for_v1(&[
                            crate::event_history_v1::EventKindV1::PublishedCastIdentityV1,
                        ])
                        .iter()
                        .filter_map(|event| match event {
                            event::CommittedEvent::PublishedCastIdentityV1 { receipt }
                                if receipt.stack_item_id == item.v4.stack_item_id =>
                            {
                                Some(receipt.spell_cast_history_index)
                            }
                            _ => None,
                        });
                    let cast_history_index = receipts.next()?;
                    if receipts.next().is_some() {
                        return None;
                    }
                    let spell = CopSpellV1 {
                        item: item.v4.stack_item_id,
                        cast_history_index,
                        permanent: None,
                        entry_history_index: None,
                    };
                    circle_cast_matches(state, *source, spell).then_some(spell)
                });
            let shields = &mut s1d_state(state).cop_shields;
            shields.retain(|shield| shield.turn == now);
            shields.push(CopShieldV1 {
                source: *source,
                player: ctx.controller,
                turn: now,
                spell,
            });
        }
        S1dOpV1::DamageFlyingV1 => {
            let events = battlefield(state)
                .filter(|&object| has_flying(state, object))
                .map(|object| event::damage_from_exec_ctx_v1(ctx, Target::Object(object), 1))
                .collect::<Vec<_>>();
            event::propose_and_commit_batch(state, events);
        }
        S1dOpV1::SagittarsVolleyV1 => {
            crate::effect::execute(&destroy_target(), ctx, state);
            let events = battlefield(state)
                .filter(|&object| {
                    state.objects.get(object).controller != ctx.controller
                        && has_flying(state, object)
                })
                .map(|object| event::damage_from_exec_ctx_v1(ctx, Target::Object(object), 1))
                .collect::<Vec<_>>();
            event::propose_and_commit_batch(state, events);
        }
        S1dOpV1::LastBreathV1 => {
            let Some(target) = bound_target_in(ctx, state, 0, Zone::Battlefield) else {
                return;
            };
            let controller = state.objects.get(target).controller;
            event::propose_and_commit(state, ProposedEvent::zone_change(target, Zone::Exile));
            event::propose_and_commit(state, ProposedEvent::life_gain(controller, 4));
        }
        S1dOpV1::BoostAllV1 {
            power,
            toughness,
            nonwhite_only,
        } => {
            let white = white_mask();
            let creatures = battlefield(state)
                .filter(|&object| {
                    is_creature(state, object)
                        && (!*nonwhite_only
                            || crate::engine::object_color_mask(state, object) & white == 0)
                })
                .collect::<Vec<_>>();
            for object in creatures {
                let binding = bind(state, object);
                crate::effect::install_temporary_boost(
                    state,
                    binding,
                    *power,
                    *toughness,
                    Keywords::NONE,
                );
            }
        }
        S1dOpV1::DevotionLifeV1 => {
            let amount = devotion_to_green(state, ctx.controller);
            if amount > 0 {
                event::propose_and_commit(state, ProposedEvent::life_gain(ctx.controller, amount));
            }
        }
        S1dOpV1::SkipCombatV1 => {
            let Some(&Target::Player(player)) = ctx.targets.first() else {
                return;
            };
            s1d_state(state).combat_skips.push(player);
        }
        S1dOpV1::DeglamerV1 => {
            let Some(target) = bound_target_in(ctx, state, 0, Zone::Battlefield) else {
                return;
            };
            let owner = state.objects.get(target).owner;
            event::propose_and_commit(
                state,
                ProposedEvent::public_library_insert(target, event::LibraryPlacement::Top),
            );
            if state.shuffle_library(owner).is_err() {
                state.engine.halted = Some((
                    crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                    ctx.source,
                ));
            }
        }
        S1dOpV1::CompassBecomeV1 { land, subtype } => {
            if !link_live_in(state, *land, Zone::Battlefield) {
                return;
            }
            let now = TurnV1::now(state);
            let records = &mut s1d_state(state).extra_land_types;
            records.retain(|record| record.turn == now);
            records.push(ExtraLandTypeV1 {
                land: *land,
                subtype: *subtype,
                turn: now,
            });
        }
        S1dOpV1::ExileSecondTargetV1 => {
            if let Some(second) = bound_target_in(ctx, state, 1, Zone::Battlefield) {
                event::propose_and_commit(state, ProposedEvent::zone_change(second, Zone::Exile));
            }
        }
        S1dOpV1::WaterbendTapV1 { object } => {
            if link_live_in(state, *object, Zone::Battlefield)
                && !state.objects.get(object.object).tapped
            {
                event::propose_and_commit(state, ProposedEvent::tap(object.object));
            }
        }
        S1dOpV1::WaterbendManaV1 { amount } => {
            if !crate::engine::pay_effect_mana(ctx.controller, &[], *amount, state) {
                state.engine.halted = Some((
                    crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                    ctx.source,
                ));
            }
        }
        S1dOpV1::MartyrRevealV1 => martyr::resolve(state, ctx),
        // Choices run in `drive`.
        S1dOpV1::CopChooseV1
        | S1dOpV1::CompassChooseV1
        | S1dOpV1::ReturnToDustV1
        | S1dOpV1::WaterbendOrDiscardV1
        | S1dOpV1::WaterbendStepV1 { .. } => {}
    }
}

/// Runs a leaf inside the resumable interpreter. This module's choices are
/// all option lists, so it never suspends on a selection itself.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &S1dOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    let controller = continuation.ctx.controller;
    match op {
        S1dOpV1::CopChooseV1 => {
            let options = circle_options(state);
            if !options.is_empty() {
                push_choice(continuation, options, &path);
            }
            Ok(false)
        }
        S1dOpV1::CompassChooseV1 => {
            let Some(land) = bound_target_in(&continuation.ctx, state, 0, Zone::Battlefield) else {
                return Ok(false);
            };
            let link = live_link(state, land);
            let options = [
                Subtype::Plains,
                Subtype::Island,
                Subtype::Swamp,
                Subtype::Mountain,
                Subtype::Forest,
            ]
            .into_iter()
            .map(|subtype| {
                s1d(S1dOpV1::CompassBecomeV1 {
                    land: link,
                    subtype,
                })
            })
            .collect::<Vec<_>>();
            push_choice(continuation, options, &path);
            Ok(false)
        }
        S1dOpV1::ReturnToDustV1 => {
            let ctx = continuation.ctx.clone();
            if let Some(first) = bound_target_in(&ctx, state, 0, Zone::Battlefield) {
                event::propose_and_commit(state, ProposedEvent::zone_change(first, Zone::Exile));
            }
            let main_phase = state.active_player == controller
                && matches!(state.step, Step::Main1 | Step::Main2);
            // A copy of the spell wasn't cast, so it exiles only the first
            // target.
            if main_phase
                && !continuation.resolving_item.is_copy
                && bound_target_in(&ctx, state, 1, Zone::Battlefield).is_some()
            {
                push_choice(
                    continuation,
                    vec![nothing(), s1d(S1dOpV1::ExileSecondTargetV1)],
                    &path,
                );
            }
            Ok(false)
        }
        S1dOpV1::WaterbendOrDiscardV1 => {
            let discard = EffectOp::DiscardCards {
                player: PlayerRef::Controller,
                count: 1,
            };
            if waterbend_finishable(state, controller, 2) {
                push_choice(
                    continuation,
                    vec![discard, s1d(S1dOpV1::WaterbendStepV1 { remaining: 2 })],
                    &path,
                );
            } else {
                push_frame(continuation, discard, &path);
            }
            Ok(false)
        }
        S1dOpV1::WaterbendStepV1 { remaining } => {
            if *remaining == 0 {
                return Ok(false);
            }
            let mut options = Vec::new();
            if crate::engine::can_pay_effect_mana(controller, &[], *remaining, state) {
                options.push(s1d(S1dOpV1::WaterbendManaV1 { amount: *remaining }));
            }
            for object in waterbend_permanents(state, controller) {
                let mut simulated = state.clone();
                simulated.objects.get_mut(object).tapped = true;
                if waterbend_finishable(&simulated, controller, remaining - 1) {
                    options.push(EffectOp::Sequence(vec![
                        s1d(S1dOpV1::WaterbendTapV1 {
                            object: live_link(state, object),
                        }),
                        s1d(S1dOpV1::WaterbendStepV1 {
                            remaining: remaining - 1,
                        }),
                    ]));
                }
            }
            if !options.is_empty() {
                push_choice(continuation, options, &path);
            }
            Ok(false)
        }
        other => {
            execute(other, &continuation.ctx, state);
            Ok(false)
        }
    }
}

// ---- Hooks: engine ------------------------------------------------------------------

/// The statics and hooks the engine asks.
pub(crate) mod statics {
    use super::*;

    /// CR 609.7a's single Stack-to-Battlefield continuation. Later zone changes
    /// never retarget the shield to another incarnation of the same card.
    pub(crate) fn circle_spell_entered(state: &mut GameState, entry: usize) {
        let Some(event::CommittedEvent::ZoneChange {
            object,
            from: Zone::Stack,
            to: Zone::Battlefield,
            ..
        }) = state.engine.event_history.get(entry)
        else {
            return;
        };
        let object = *object;
        let live = state.objects.get(object);
        let permanent = ObjectLinkV4 {
            object,
            zone_change_count: live.zone_change_count,
        };
        let Some(previous) = permanent.zone_change_count.checked_sub(1) else {
            return;
        };
        let Some(eternal) = state.eternal_v1.as_ref() else {
            return;
        };
        let updates = eternal
            .s1d
            .cop_shields
            .iter()
            .enumerate()
            .filter_map(|(index, shield)| {
                let mut spell = shield.spell?;
                if shield.source.object != object
                    || shield.source.zone_change_count != previous
                    || spell.permanent.is_some()
                    || spell.entry_history_index.is_some()
                {
                    return None;
                }
                spell.permanent = Some(permanent);
                spell.entry_history_index = u64::try_from(entry).ok();
                circle_entry_matches(state, shield.source, spell).then_some((index, spell))
            })
            .collect::<Vec<_>>();
        for (index, spell) in updates {
            state.eternal_v1.as_mut().unwrap().s1d.cop_shields[index].spell = Some(spell);
        }
    }

    /// Circle of Protection: Blue. Whether a shield prevents `damage`; the
    /// shield is used up.
    pub(crate) fn consume_circle_shield(state: &mut GameState, damage: &DamageProposed) -> bool {
        let Target::Player(player) = damage.target else {
            return false;
        };
        if damage.amount <= 0 {
            return false;
        }
        let Some(s1d) = read_state(state) else {
            return false;
        };
        if s1d.cop_shields.is_empty() {
            return false;
        }
        let (source, color_mask) = if let Some(item) = damage.historical_source {
            let Ok(facts) =
                crate::source_power_activation_v1::damage_source(state, damage.source, item)
            else {
                return false;
            };
            (
                ObjectLinkV4 {
                    object: facts.source.source,
                    zone_change_count: facts.source.zone_change_count,
                },
                facts.color_mask,
            )
        } else {
            let Some(live) = state.objects.try_get(damage.source) else {
                return false;
            };
            (
                ObjectLinkV4 {
                    object: damage.source,
                    zone_change_count: live.zone_change_count,
                },
                crate::engine::object_color_mask(state, damage.source),
            )
        };
        // CR 609.7b: losing blue does not spend the shield. Historical damage
        // uses the exact published source incarnation and its damage-time LKI.
        if color_mask & blue_mask() == 0 {
            return false;
        }
        let now = TurnV1::now(state);
        let position = s1d.cop_shields.iter().position(|shield| {
            shield.turn == now
                && shield.player == player
                && (shield.source == source
                    || shield.spell.is_some_and(|spell| {
                        spell.permanent == Some(source)
                            && circle_entry_matches(state, shield.source, spell)
                    }))
        });
        let Some(position) = position else {
            return false;
        };
        s1d_state(state).cop_shields.remove(position);
        true
    }

    /// Stonehorn Dignitary: whether `player`'s combat phase, about to begin,
    /// is skipped. The skip is used up.
    pub(crate) fn take_combat_skip(state: &mut GameState, player: PlayerId) -> bool {
        let Some(s1d) = read_state(state) else {
            return false;
        };
        let Some(position) = s1d.combat_skips.iter().position(|&skip| skip == player) else {
            return false;
        };
        s1d_state(state).combat_skips.remove(position);
        true
    }

    /// Overload cards: the second form of the card is its overload cost.
    pub(crate) fn is_overload(name: &str) -> bool {
        name == VANDALBLAST || name == ELECTRICKERY
    }

    /// Navigator's Compass: the basic land types `object` has gained until
    /// end of turn.
    pub(crate) fn extra_land_types(state: &GameState, object: ObjectId) -> Vec<Subtype> {
        let Some(s1d) = read_state(state) else {
            return Vec::new();
        };
        if s1d.extra_land_types.is_empty() {
            return Vec::new();
        }
        let Some(live) = state.objects.try_get(object) else {
            return Vec::new();
        };
        if live.zone != Zone::Battlefield {
            return Vec::new();
        }
        let now = TurnV1::now(state);
        s1d.extra_land_types
            .iter()
            .filter(|record| {
                record.turn == now
                    && record.land.object == object
                    && record.land.zone_change_count == live.zone_change_count
            })
            .map(|record| record.subtype)
            .collect()
    }

    /// Navigator's Compass: a land that has a basic land type only through
    /// it has that type's mana ability.
    pub(crate) fn granted_mana_colors(state: &GameState, object: ObjectId) -> Vec<ManaColor> {
        let extra = extra_land_types(state, object);
        if extra.is_empty() {
            return Vec::new();
        }
        let live = state.objects.get(object);
        let def = &CARD_DEFS[live.card_def as usize];
        let mut colors = Vec::new();
        for subtype in extra {
            let color = match subtype {
                Subtype::Plains => ManaColor::W,
                Subtype::Island => ManaColor::U,
                Subtype::Swamp => ManaColor::B,
                Subtype::Mountain => ManaColor::R,
                Subtype::Forest => ManaColor::G,
                _ => continue,
            };
            let own = def
                .primary_mana_ability_choices(live.v4.chosen_color)
                .contains(&color)
                || def
                    .additional_mana_abilities
                    .iter()
                    .any(|ability| ability.colors.contains(&color))
                || def
                    .restricted_mana_abilities
                    .iter()
                    .any(|ability| ability.colors.contains(&color));
            if !own && !colors.contains(&color) {
                colors.push(color);
            }
        }
        colors
    }

    /// Spreading Seas: "Enchanted land is an Island." The timestamp of the
    /// latest Spreading Seas whose ability applies to `object`.
    pub(crate) fn island_aura_timestamp(state: &GameState, object: ObjectId) -> Option<u64> {
        let live = state.objects.try_get(object)?;
        if live.zone != Zone::Battlefield || live.attachments.is_empty() {
            return None;
        }
        let host = Some(live_link(state, object));
        live.attachments
            .iter()
            .copied()
            .filter(|&aura| {
                active_named(state, aura, SPREADING_SEAS)
                    && state.objects.get(aura).v4.attached_to == host
            })
            .map(|aura| state.objects.get(aura).v4.layer_timestamp.unwrap_or(0))
            .max()
    }

    /// Flagbearer (Standard Bearer, Coalition Honor Guard): "While choosing
    /// targets as part of casting a spell or activating an ability, your
    /// opponents must choose at least one Flagbearer on the battlefield if
    /// able." The restriction applies to the last target slot of a spell or
    /// ability being announced by `chooser`, when no earlier target is a
    /// Flagbearer and one is a legal choice.
    pub(crate) fn restrict_to_flagbearers(
        state: &GameState,
        chooser: PlayerId,
        chosen: &[Target],
        count: usize,
        legal: &mut Vec<Target>,
    ) {
        if state.engine.pending_cast.is_none() && state.engine.pending_activation.is_none() {
            return;
        }
        if chosen.len() + 1 < count || legal.is_empty() {
            return;
        }
        if !flagbearer_requirement_applies(state, chooser)
            || chosen.iter().any(|target| is_flagbearer(state, target))
        {
            return;
        }
        if legal.iter().any(|target| is_flagbearer(state, target)) {
            legal.retain(|target| is_flagbearer(state, target));
        }
    }

    pub(crate) fn flagbearer_requirement_applies(state: &GameState, chooser: PlayerId) -> bool {
        state.players[chooser.opponent().index()]
            .battlefield
            .iter()
            .any(|&object| {
                active_named(state, object, STANDARD_BEARER)
                    || active_named(state, object, COALITION_HONOR_GUARD)
            })
    }

    pub(crate) fn is_flagbearer(state: &GameState, target: &Target) -> bool {
        match target {
            Target::Object(object) => {
                state
                    .objects
                    .try_get(*object)
                    .is_some_and(|live| live.zone == Zone::Battlefield)
                    && crate::engine::has_effective_subtype(state, *object, Subtype::Flagbearer)
            }
            _ => false,
        }
    }

    pub(crate) fn uses_flagbearer_count_completion(spec: TargetSpec) -> bool {
        use crate::standard_cards_v1::StandardTargetV1 as S;
        if matches!(
            spec,
            TargetSpec::KickerControlledCreatures
                | TargetSpec::StandardV1(S::ArtifactThenArtifactCardInItsControllersGraveyardV1)
        ) {
            return true;
        }
        let max = crate::engine::target_count(spec);
        // Larger generic menus either have a known simple roster, cannot
        // select a battlefield object, or use a separate damage allocation.
        crate::engine::target_min_count(spec) < max
            && (max <= 2
                || matches!(
                    spec,
                    TargetSpec::StandardV1(
                        S::OneToThreeDistinctAnyTargetsV1
                            | S::AnyNumberControlledNonlandNontokenPermanentsV1
                            | S::CreatureThenGraveyardCardsV1
                    )
                ))
    }

    /// Only a complete assignment for the selected count establishes an
    /// obligation. For optional targeting, the engine interleaves picking the
    /// count and targets even though rule 601.2c fixes the count first.
    pub(crate) fn optional_flagbearer_targets(
        state: &GameState,
        spec: TargetSpec,
        chosen: &[Target],
        min: usize,
        max: usize,
        candidates: impl Fn(&[Target]) -> Vec<Target>,
    ) -> (Vec<Target>, bool) {
        use crate::standard_cards_v1::StandardTargetV1 as S;
        let selected_flagbearer = chosen.iter().any(|target| is_flagbearer(state, target));
        let mut next = if chosen.len() < max {
            candidates(chosen)
        } else {
            Vec::new()
        };
        if selected_flagbearer {
            return (next, chosen.len() >= min);
        }
        match spec {
            TargetSpec::StandardV1(
                S::OneToThreeDistinctAnyTargetsV1
                | S::AnyNumberControlledNonlandNontokenPermanentsV1,
            ) => {
                // Distinct targets come from one independent roster. A
                // non-Flagbearer pick cannot consume an eligible Flagbearer.
                let available = if chosen.len() == max {
                    candidates(&[])
                        .iter()
                        .any(|target| is_flagbearer(state, target))
                } else {
                    next.iter().any(|target| is_flagbearer(state, target))
                };
                if available && chosen.len() + 1 == max {
                    next.retain(|target| is_flagbearer(state, target));
                }
                return (
                    next,
                    chosen.len() >= min && (chosen.is_empty() || !available),
                );
            }
            TargetSpec::StandardV1(S::CreatureThenGraveyardCardsV1) => {
                // Only the first slot can target a permanent. The later
                // graveyard roster is independent of the first choice.
                if chosen.is_empty() {
                    if next.iter().any(|target| is_flagbearer(state, target)) {
                        next.retain(|target| is_flagbearer(state, target));
                    }
                    return (next, false);
                }
                if candidates(&[])
                    .iter()
                    .any(|target| is_flagbearer(state, target))
                {
                    return (Vec::new(), false);
                }
                return (next, true);
            }
            _ => {}
        }
        debug_assert!(max <= 2);
        let obligations: Vec<_> = (min.max(chosen.len())..=max)
            .map(|count| {
                (
                    count,
                    target_assignment_exists(state, &mut Vec::new(), count, true, &candidates),
                )
            })
            .collect();
        let can_finish = obligations
            .iter()
            .any(|&(count, required)| count == chosen.len() && !required);
        let mut prefix = chosen.to_vec();
        next.retain(|&target| {
            prefix.push(target);
            let can_complete = obligations.iter().any(|&(count, required)| {
                count >= prefix.len()
                    && target_assignment_exists(state, &mut prefix, count, required, &candidates)
            });
            prefix.pop();
            can_complete
        });
        (next, can_finish)
    }

    fn target_assignment_exists(
        state: &GameState,
        prefix: &mut Vec<Target>,
        count: usize,
        requires_flagbearer: bool,
        candidates: &impl Fn(&[Target]) -> Vec<Target>,
    ) -> bool {
        if prefix.len() == count {
            return !requires_flagbearer
                || prefix.iter().any(|target| is_flagbearer(state, target));
        }
        candidates(prefix).into_iter().any(|target| {
            prefix.push(target);
            let exists =
                target_assignment_exists(state, prefix, count, requires_flagbearer, candidates);
            prefix.pop();
            exists
        })
    }
}

// ---- Auras --------------------------------------------------------------------------

/// This module's Auras that the Standard Aura entry attaches: Spreading Seas
/// ("Enchant land").
pub(crate) fn is_aura(def: &CardDef) -> bool {
    def.name == SPREADING_SEAS
}

/// Whether this module's Aura `def` may enchant `host`. `None` for every
/// other definition.
pub(crate) fn aura_host_legal(def: &CardDef, host: ObjectId, state: &GameState) -> Option<bool> {
    (def.name == SPREADING_SEAS).then(|| {
        state.objects.get(host).zone == Zone::Battlefield && has_type(state, host, CardType::Land)
    })
}

// ---- Targets ------------------------------------------------------------------------

/// Target filters of this module's cards, carried as
/// `StandardTargetV1::S1dV1`. `standard_cards_v1::legal_targets` and
/// `target_matches` ask these first.
pub(crate) mod targets {
    use super::{has_flying, has_type, is_creature};
    use crate::card_def::{CardType, Subtype, CARD_DEFS};
    use crate::ids::{ObjectId, PlayerId};
    use crate::standard_cards_v1::StandardTargetV1;
    use crate::state::{GameState, StackItemKind, Target, Zone};
    use serde::{Deserialize, Serialize};

    /// The filters of wave S1d.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum S1dTargetV1 {
        /// "Target creature with power N or less" (Last Breath).
        CreaturePowerAtMostV1(u8),
        /// "Target creature with flying" (Sagittars' Volley).
        CreatureWithFlyingV1,
        /// "Target non-Faerie spell" (Faerie Trickery).
        NonFaerieSpellV1,
        /// "Target land you control" (Navigator's Compass).
        ControlledLandV1,
        /// Call Damage Control's "up to two" distinct modes: up to two target
        /// artifact, creature, enchantment or land cards in your graveyard,
        /// each of a different mode.
        GraveyardCardsOfDistinctTypesV1,
        /// "Target artifact or enchantment" then "up to one other target
        /// artifact or enchantment" (Return to Dust).
        ArtifactOrEnchantmentThenUpToOneOtherV1,
    }

    fn kind(target: StandardTargetV1) -> Option<S1dTargetV1> {
        match target {
            StandardTargetV1::S1dV1(kind) => Some(kind),
            _ => None,
        }
    }

    /// How many targets this module's filter takes, and how many at least.
    pub(crate) fn counts(target: StandardTargetV1) -> Option<(u8, u8)> {
        Some(match kind(target)? {
            S1dTargetV1::GraveyardCardsOfDistinctTypesV1 => (2, 0),
            S1dTargetV1::ArtifactOrEnchantmentThenUpToOneOtherV1 => (2, 1),
            _ => (1, 1),
        })
    }

    /// Whether this module's filter takes cards in the graveyard.
    pub(crate) fn in_graveyard(target: StandardTargetV1) -> bool {
        kind(target) == Some(S1dTargetV1::GraveyardCardsOfDistinctTypesV1)
    }

    /// Whether this module's filter takes spells on the stack.
    pub(crate) fn on_stack(target: StandardTargetV1) -> bool {
        kind(target) == Some(S1dTargetV1::NonFaerieSpellV1)
    }

    const MODE_TYPES: [CardType; 4] = [
        CardType::Artifact,
        CardType::Creature,
        CardType::Enchantment,
        CardType::Land,
    ];

    /// The modes (indexes into `MODE_TYPES`) a graveyard card can fill.
    fn modes(state: &GameState, card: ObjectId) -> u8 {
        let def = &CARD_DEFS[state.objects.get(card).card_def as usize];
        MODE_TYPES
            .iter()
            .enumerate()
            .filter(|(_, card_type)| def.has_type(**card_type))
            .fold(0, |mask, (index, _)| mask | (1 << index))
    }

    /// Whether two cards with these mode masks can fill two different modes.
    fn distinct_modes(first: u8, second: u8) -> bool {
        (0..4).any(|a| first & (1 << a) != 0 && (0..4).any(|b| a != b && second & (1 << b) != 0))
    }

    fn graveyard_card(state: &GameState, controller: PlayerId, object: ObjectId) -> bool {
        state.objects.try_get(object).is_some_and(|live| {
            live.zone == Zone::Graveyard
                && live.owner == controller
                && !live.v4.is_token
                && live.spell_copy_origin.is_none()
        }) && modes(state, object) != 0
    }

    fn non_faerie_spell(state: &GameState, object: ObjectId) -> bool {
        if state
            .engine
            .pending_cast
            .as_ref()
            .is_some_and(|pending| pending.spell == object)
        {
            return false;
        }
        state
            .stack
            .iter()
            .any(|item| item.kind == StackItemKind::Spell && item.source == object)
            && !crate::engine::has_effective_subtype(state, object, Subtype::Faerie)
    }

    /// Whether `object` satisfies this module's filter `target` for
    /// `controller`, or `None` for every other filter.
    pub(crate) fn target_matches(
        target: StandardTargetV1,
        controller: PlayerId,
        object: ObjectId,
        state: &GameState,
    ) -> Option<bool> {
        let kind = kind(target)?;
        let Some(live) = state.objects.try_get(object) else {
            return Some(false);
        };
        Some(match kind {
            S1dTargetV1::NonFaerieSpellV1 => non_faerie_spell(state, object),
            S1dTargetV1::GraveyardCardsOfDistinctTypesV1 => {
                graveyard_card(state, controller, object)
            }
            _ if live.zone != Zone::Battlefield => false,
            S1dTargetV1::CreaturePowerAtMostV1(maximum) => {
                is_creature(state, object)
                    && crate::engine::effective_power(state, object) <= i32::from(maximum)
            }
            S1dTargetV1::CreatureWithFlyingV1 => has_flying(state, object),
            S1dTargetV1::ControlledLandV1 => {
                live.controller == controller && has_type(state, object, CardType::Land)
            }
            S1dTargetV1::ArtifactOrEnchantmentThenUpToOneOtherV1 => {
                has_type(state, object, CardType::Artifact)
                    || has_type(state, object, CardType::Enchantment)
            }
        })
    }

    /// The legal choices for this module's filter `target` after the already
    /// chosen `prefix`, or `None` for every other filter.
    pub(crate) fn legal_targets(
        target: StandardTargetV1,
        controller: PlayerId,
        prefix: &[Target],
        state: &GameState,
    ) -> Option<Vec<Target>> {
        let kind = kind(target)?;
        let (maximum, _) = counts(target)?;
        if prefix.len() >= usize::from(maximum) {
            return Some(Vec::new());
        }
        let candidates: Vec<ObjectId> = match kind {
            S1dTargetV1::NonFaerieSpellV1 => state
                .stack
                .iter()
                .filter(|item| item.kind == StackItemKind::Spell)
                .map(|item| item.source)
                .collect(),
            S1dTargetV1::GraveyardCardsOfDistinctTypesV1 => {
                state.players[controller.index()].graveyard.clone()
            }
            _ => [PlayerId::P0, PlayerId::P1]
                .into_iter()
                .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
                .collect(),
        };
        let first_modes = match (kind, prefix.first()) {
            (S1dTargetV1::GraveyardCardsOfDistinctTypesV1, Some(Target::Object(first))) => {
                Some(modes(state, *first))
            }
            _ => None,
        };
        Some(
            candidates
                .into_iter()
                .filter(|&object| target_matches(target, controller, object, state) == Some(true))
                .filter(|&object| !prefix.contains(&Target::Object(object)))
                .filter(|&object| {
                    first_modes.is_none_or(|first| distinct_modes(first, modes(state, object)))
                })
                .map(Target::Object)
                .collect(),
        )
    }
}
