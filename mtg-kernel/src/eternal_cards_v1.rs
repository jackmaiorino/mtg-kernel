//! Card programs for the Pauper, Legacy and Vintage catalog
//! (`data/eternal_v1/cards_v1.json`, `docs/design/eternal_formats_support_v1.md`).
//!
//! Every table here is keyed by a printed name that occurs only in the
//! eternal registry, so the Pauper, FDN and Standard catalogs never reach it.
//! The shared primitives it adds (energy, replicate, two-target spells, fixed
//! two-color mana) are inert until one of these cards uses them. Behavior was
//! read from each card's Oracle text and, where present, its XMage source.
//!
//! Engine hooks into this module are compiled only in `eternal-fixtures`
//! builds, so the other catalogs pay nothing for them.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{
    card_id_by_name, CardType, CombatStaticRulesDef, DynamicValueDef, Keywords, Subtype,
    TargetSpec, CARD_DEFS,
};
use crate::effect::{
    EffectOp, ExecCtx, LibraryLookCount, LibraryPickFilterV1, LibraryPickRule,
    LibraryPickSelectionV1, ObjectRef, PlayerRef, TargetRef,
};
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::{Cost, ManaColor, Pip};
use crate::standard_cards_v1::{
    StandardChosenActionV1, StandardOpV1, StandardPermanentFilterV1, StandardTriggerV1,
};
use crate::state::{AbilitySourceContractV4, GameState, ObjectLinkV4, Target, Zone};
use crate::trigger::{PendingTrigger, TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

pub mod e3;
pub use e3::{
    dark_ritual, doomsday, edge_of_autumn, flusterstorm, jace_minus_eight, jace_plus_one,
    misdirection, personal_tutor, show_and_tell, sink_into_stupor, sneak_attack, the_one_ring_draw,
    veil_of_summer,
};

/// Per-game state for this module's cards. Absent until one of them needs
/// it, so every other catalog keeps its snapshots and hashes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EternalStateV1 {
    /// Energy counters per player (107.14), indexed by `PlayerId::index`.
    #[serde(default)]
    pub energy: [u32; 2],
    /// "Draw a card at the beginning of the next turn's upkeep" (the
    /// Baubles), waiting for the next `UpkeepBegan`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub upkeep_draws: Vec<DelayedUpkeepDrawV1>,
    /// Murktide Regent spells and how many instant and sorcery cards their
    /// Delve exiled, keyed by the spell's stack incarnation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub delve_instants_and_sorceries: Vec<(ObjectLinkV4, u32)>,
    /// The active player's turn-based draw this turn (Orcish Bowmasters'
    /// "except the first one they draw in each of their draw steps").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draw_step_draw: Option<DrawStepDrawV1>,
    /// Tamiyo, Seasoned Scholar's +2, until its controller's next turn:
    /// "whenever a creature attacks you or a planeswalker you control, it
    /// gets -1/-0 until end of turn".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attack_shrinks: Vec<DelayedUpkeepDrawV1>,
    /// Attackers just declared against an `attack_shrinks` controller,
    /// waiting for the next trigger collection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shrinking_attackers: Vec<ObjectLinkV4>,
    /// Emblems with "You have no maximum hand size", per player.
    #[serde(default, skip_serializing_if = "is_no_emblem")]
    pub no_maximum_hand_size: [bool; 2],
    /// State of the wave-E3 cards (`e3::E3StateV1`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub e3: Option<Box<e3::E3StateV1>>,
    /// The Vintage cards' state (`eternal_vintage_v1`).
    #[serde(
        default,
        skip_serializing_if = "crate::eternal_vintage_v1::VintageStateV1::is_empty"
    )]
    pub vintage: crate::eternal_vintage_v1::VintageStateV1,
    /// The Vintage Lurrus lists' state (`eternal_lurrus_v1`).
    #[serde(
        default,
        skip_serializing_if = "crate::eternal_lurrus_v1::LurrusStateV1::is_empty"
    )]
    pub lurrus: crate::eternal_lurrus_v1::LurrusStateV1,
    /// The Legacy aggro, Eldrazi and control lists' state
    /// (`eternal_legacy_e5b_v1`).
    #[serde(
        default,
        skip_serializing_if = "crate::eternal_legacy_e5b_v1::E5bStateV1::is_empty"
    )]
    pub e5b: crate::eternal_legacy_e5b_v1::E5bStateV1,
}

fn is_no_emblem(emblems: &[bool; 2]) -> bool {
    !emblems[0] && !emblems[1]
}

/// One delayed upkeep draw: the activated ability's source incarnation and
/// its controller, who draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DelayedUpkeepDrawV1 {
    pub controller: PlayerId,
    pub source: AbilitySourceContractV4,
}

/// The turn-based draw of one draw step: the turn, the drawing player and
/// that draw's ordinal among their draws this turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrawStepDrawV1 {
    pub turn: u32,
    pub player: PlayerId,
    pub ordinal: u32,
}

/// Trigger conditions owned by this module, carried as
/// `StandardTriggerV1::EternalV1`. Each matcher checks the source's own
/// battlefield presence, since Standard conditions skip the generic check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EternalTriggerV1 {
    /// "Whenever an instant or sorcery card leaves your graveyard"
    /// (Murktide Regent).
    InstantOrSorceryCardLeavesYourGraveyard,
    /// "Whenever one or more permanent cards are put into your graveyard
    /// from anywhere while this creature has a -1/-1 counter on it"
    /// (Moonshadow). One trigger per collected event batch.
    PermanentCardsToYourGraveyardWhileMinusCounter,
    /// "Whenever an opponent draws a card except the first one they draw in
    /// each of their draw steps" (Orcish Bowmasters).
    OpponentDrawsExceptFirstInDrawStep,
    /// "When this enters, if you cast it" (The One Ring): it entered from
    /// the stack as a resolving permanent spell.
    EntersAfterCastV1,
    /// "When this is put into a graveyard from anywhere" (Emrakul, the
    /// Aeons Torn). Functions from the graveyard.
    PutIntoGraveyardFromAnywhereV1,
    /// "When this enters, if you haven't completed Tomb of Annihilation"
    /// (Acererak the Archlich).
    EntersIfTombNotCompletedV1,
    /// A condition owned by the Vintage cards' module.
    VintageV1(crate::eternal_vintage_v1::VintageTriggerV1),
    /// A condition owned by the Vintage Lurrus lists' module.
    LurrusV1(crate::eternal_lurrus_v1::LurrusTriggerV1),
    /// A condition owned by the Legacy aggro, Eldrazi and control lists'
    /// module.
    E5bV1(crate::eternal_legacy_e5b_v1::E5bTriggerV1),
}

/// Effect leaves owned by this module, carried as
/// `StandardOpV1::EternalV1` so `EffectOp` keeps its shape.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EternalOpV1 {
    /// The controller gets `amount` energy counters ("you get {E}{E}").
    GainEnergy { amount: u8 },
    /// Target `index` gets `pumps[index]` until end of turn, for each target
    /// still legal on resolution (608.2b). Each "target creature" may name
    /// the same creature (115.3), as in Agony Warp.
    PumpEachLegalTargetV1 { pumps: Vec<(i32, i32)> },
    /// Draw `normal` cards, or `from_graveyard` instead if this spell was
    /// cast from a graveyard (Plunder the Trollshaws).
    DrawCastFromGraveyardV1 { normal: u8, from_graveyard: u8 },
    /// The controller shuffles their graveyard into their library
    /// (Campfire). No commanders exist in these games.
    ShuffleGraveyardIntoLibrary,
    /// Destroy the target creature if its mana value is at most `normal`,
    /// or at most `revolt` if a permanent left the battlefield under the
    /// controller's control this turn (Fatal Push).
    DestroyTargetCreatureManaValueAtMostRevoltV1 { normal: u16, revolt: u16 },
    /// Deal `normal` damage to the target, or `delirium` if the controller
    /// has four or more card types among cards in their graveyard (Unholy
    /// Heat).
    DamageTargetDeliriumV1 { normal: i32, delirium: i32 },
    /// Deal `amount` damage to each target still legal under `spec`, as one
    /// simultaneous batch (Prismari Charm).
    DamageEachLegalTargetV1 { amount: i32, spec: TargetSpec },
    /// The controller looks at the top card of the target player's library
    /// (Mishra's Bauble). Only the controller learns it.
    LookAtTopOfTargetLibraryV1,
    /// The controller looks at a card at random in the target player's
    /// hand (Urza's Bauble). Only the controller learns it.
    LookAtRandomCardInTargetHandV1,
    /// "Draw a card at the beginning of the next turn's upkeep": registers
    /// the delayed trigger for this ability's source incarnation.
    DrawAtNextUpkeepV1,
    /// The delayed trigger itself: its controller draws a card.
    DelayedUpkeepDrawV1,
    /// Amass's first half: create a 0/0 black Orc Army creature token if the
    /// controller controls no Army (701.47a).
    CreateOrcArmyIfNoneV1,
    /// Remove a -1/-1 counter from this ability's source incarnation, if it
    /// has one (Moonshadow).
    RemoveMinusOneCounterFromSourceV1,
    /// Exile this ability's source incarnation, then return it to the
    /// battlefield transformed under its owner's control (Tamiyo).
    ExileThenReturnTransformedV1,
    /// Tamiyo's +2: until the controller's next turn, each creature that
    /// attacks them or a planeswalker they control gets -1/-0 until end of
    /// turn.
    ShrinkAttackersUntilYourNextTurnV1,
    /// The delayed trigger itself: `attacker` gets -1/-0 until end of turn.
    ShrinkAttackerV1 { attacker: ObjectLinkV4 },
    /// Tamiyo's -7: "Draw cards equal to half the number of cards in your
    /// library, rounded up. You get an emblem with 'You have no maximum hand
    /// size.'"
    DrawHalfLibraryThenNoMaximumHandSizeV1,
    /// The controller gains protection from everything until their next
    /// turn (The One Ring).
    ProtectionFromEverythingUntilYourNextTurnV1,
    /// The controller loses 1 life for each burden counter on this
    /// ability's source incarnation (The One Ring).
    LoseLifePerBurdenCounterV1,
    /// Put a burden counter on the source, then draw a card for each burden
    /// counter on it (The One Ring).
    BurdenCounterThenDrawV1,
    /// "You may exile a nonartifact, nonland card from your hand" linked to
    /// the source (Chrome Mox's imprint). A choice.
    MayImprintFromHandV1,
    /// "Change the target of target spell with a single target"
    /// (Misdirection). A choice.
    ChangeTargetOfTargetSpellV1,
    /// "If you control `lands` or fewer lands, search your library for a
    /// basic land card, put it onto the battlefield tapped, then shuffle"
    /// (Edge of Autumn). A choice.
    SearchBasicLandIfAtMostLandsV1 { lands: u8 },
    /// Sacrifice this ability's source incarnation if its controller still
    /// controls it (City of Traitors).
    SacrificeSourceV1,
    /// Veil of Summer's effect.
    VeilOfSummerV1,
    /// Doomsday's pile: exactly five (or all) cards of the controller's
    /// library and graveyard become the library in a chosen order; the rest
    /// are exiled. A choice.
    DoomsdayPileV1,
    /// Thassa's Oracle's entry look and devotion win. A choice.
    ThassasOracleV1,
    /// "If your library has no cards in it, you win the game" (Jace's -8).
    WinIfLibraryEmptyV1,
    /// Show and Tell's effect. A choice.
    ShowAndTellV1,
    /// Sneak Attack's ability. A choice.
    SneakAttackV1,
    /// "Defending player sacrifices `count` permanents" (annihilator). A
    /// choice.
    AnnihilatorV1 { count: u8 },
    /// Atraxa, Grand Unifier's entry reveal. A choice.
    AtraxaRevealV1,
    /// Raph & Mikey, Troublemakers' attack trigger.
    RevealUntilCreatureAttackingV1,
    /// Move the controller's venture marker into a dungeon room
    /// (`e3::dungeons`).
    EnterRoomV1 { dungeon: u16, room: u16 },
    /// A dungeon room's ability. May be a choice.
    RoomAbilityV1 { dungeon: u16, room: u16 },
    /// Acererak the Archlich's entry trigger. A choice.
    AcererakEntersV1,
    /// Acererak the Archlich's attack trigger. A choice.
    AcererakAttacksV1,
    /// A leaf owned by the Vintage cards' module.
    VintageV1(crate::eternal_vintage_v1::VintageOpV1),
    /// A leaf owned by the Vintage Lurrus lists' module.
    LurrusV1(crate::eternal_lurrus_v1::LurrusOpV1),
    /// A leaf owned by the Legacy aggro, Eldrazi and control lists' module.
    E5bV1(crate::eternal_legacy_e5b_v1::E5bOpV1),
}

/// Whether `condition` triggers on its source's own arrival in the zone it
/// now occupies (entering the battlefield, or Emrakul put into a
/// graveyard), so the trigger's source is that new incarnation rather than
/// the one that left.
pub(crate) fn arrival_condition(condition: crate::trigger::TriggerCondition) -> bool {
    cfg!(feature = "eternal-fixtures")
        && match condition {
            crate::trigger::TriggerCondition::StandardV1(
                crate::standard_cards_v1::StandardTriggerV1::EternalV1(condition),
            ) => match condition {
                EternalTriggerV1::EntersAfterCastV1
                | EternalTriggerV1::EntersIfTombNotCompletedV1
                | EternalTriggerV1::PutIntoGraveyardFromAnywhereV1 => true,
                EternalTriggerV1::E5bV1(condition) => {
                    crate::eternal_legacy_e5b_v1::arrival_condition(condition)
                }
                _ => false,
            },
            _ => false,
        }
}

impl EternalOpV1 {
    /// Whether this leaf can yield a player decision, so its program must use
    /// the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        e3::contains_player_choice(self)
    }
}

/// The number of energy counters `player` has.
pub fn energy(state: &GameState, player: PlayerId) -> u32 {
    state
        .eternal_v1
        .as_ref()
        .map_or(0, |eternal| eternal.energy[player.index()])
}

/// Pays an energy cost, or returns `None` without change if `player` has
/// fewer than `amount` energy counters.
pub(crate) fn pay_energy(state: &mut GameState, player: PlayerId, amount: u8) -> Option<()> {
    let available = energy(state, player);
    let remaining = available.checked_sub(u32::from(amount))?;
    if amount > 0 {
        state.eternal_v1.get_or_insert_with(Default::default).energy[player.index()] = remaining;
    }
    Some(())
}

/// Replicate (702.56): the cost paid once per copy. The number of payments
/// is announced like X and stored as the spell's X value; the spell's mana
/// cost has no X, so its mana value is unchanged.
pub fn replicate_cost(name: &str) -> Option<Cost> {
    match name {
        "Lose Focus" => Some(Cost {
            pips: &[Pip::Colored(ManaColor::U)],
            generic: 0,
            x_count: 0,
        }),
        _ => crate::eternal_legacy_e5b_v1::replicate_cost(name),
    }
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

const MELDED_MOXITE: &str = "Melded Moxite";
const INVENTORS_AXE: &str = "Inventor's Axe";
const BALEFUL_STRIX: &str = "Baleful Strix";
const GIANTS_BOULDER: &str = "Giant's Boulder";
const AUGUR_OF_BOLAS: &str = "Augur of Bolas";
const THORN_OF_THE_BLACK_ROSE: &str = "Thorn of the Black Rose";
const DIMIR_AQUEDUCT: &str = "Dimir Aqueduct";
const DRAGONS_RAGE_CHANNELER: &str = "Dragon's Rage Channeler";
const MURKTIDE_REGENT: &str = "Murktide Regent";
const MOONSHADOW: &str = "Moonshadow";
const ORCISH_BOWMASTERS: &str = "Orcish Bowmasters";
const NETHERGOYF: &str = "Nethergoyf";
const BRAZEN_BORROWER: &str = "Brazen Borrower";
const MISHRAS_BAUBLE: &str = "Mishra's Bauble";
const URZAS_BAUBLE: &str = "Urza's Bauble";
const ORC_ARMY_TOKEN: &str = "Orc Army Token";
const TAMIYO: &str = "Tamiyo, Inquisitive Student";
const BILBO: &str = "Bilbo, Thief in the Night";

const fn trigger(condition: TriggerCondition, effect: fn() -> EffectOp) -> TriggeredAbilityDef {
    TriggeredAbilityDef {
        condition,
        ..etb(effect)
    }
}

const fn eternal_trigger(
    condition: EternalTriggerV1,
    effect: fn() -> EffectOp,
) -> TriggeredAbilityDef {
    trigger(
        TriggerCondition::StandardV1(StandardTriggerV1::EternalV1(condition)),
        effect,
    )
}

fn eternal(op: EternalOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(op))
}

/// "Whenever you cast a noncreature spell, surveil 1."
fn surveil_one() -> EffectOp {
    EffectOp::Surveil {
        player: PlayerRef::Controller,
        count: 1,
    }
}

/// "Put a +1/+1 counter on this creature."
fn plus_one_counter_on_source() -> EffectOp {
    EffectOp::AddPlusOneCounterToAbilitySource
}

fn remove_minus_one_counter_from_source() -> EffectOp {
    eternal(EternalOpV1::RemoveMinusOneCounterFromSourceV1)
}

/// Orcish Bowmasters: "this creature deals 1 damage to any target. Then
/// amass Orcs 1." Amass (701.47a): if you control no Army, create a 0/0
/// black Orc Army creature token; then put a +1/+1 counter on an Army you
/// control, chosen if there are several.
fn bowmasters_ping_then_amass() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::DealDamage {
            target: TargetRef::Target(0),
            amount: 1,
        },
        eternal(EternalOpV1::CreateOrcArmyIfNoneV1),
        EffectOp::StandardV1(StandardOpV1::PlayerChoosesControlledPermanent {
            player: PlayerRef::Controller,
            filter: StandardPermanentFilterV1::ArmyV1,
            action: StandardChosenActionV1::PutPlusOneCountersV1(1),
        }),
    ])
}

fn delayed_upkeep_draw() -> EffectOp {
    eternal(EternalOpV1::DelayedUpkeepDrawV1)
}

/// "Whenever Bilbo attacks, you may cast an artifact, instant, or sorcery
/// spell from your graveyard." The cast happens during the resolution and
/// pays its costs; `exiled_instead_of_graveyard` applies the rest.
fn bilbo_cast_from_graveyard() -> EffectOp {
    EffectOp::SelectObjectsV1 {
        rule: crate::effect::ObjectSelectionRuleV1 {
            player: PlayerRef::Controller,
            zone: Zone::Graveyard,
            any_player: false,
            filter: crate::effect::ObjectSelectionFilterV1::CastableArtifactInstantOrSorceryV1,
            min: 0,
            max: 1,
            action: crate::effect::ObjectSelectionActionV1::CastFromGraveyardV1,
        },
    }
}

/// "Whenever Tamiyo attacks, investigate."
fn tamiyo_investigate() -> EffectOp {
    EffectOp::CreateToken {
        token_def: card_id_by_name("Clue Token").expect("Clue Token in CARD_DEFS"),
        controller: PlayerRef::Controller,
    }
}

/// "When you draw your third card in a turn, exile Tamiyo, then return her
/// to the battlefield transformed under her owner's control."
fn tamiyo_transform() -> EffectOp {
    eternal(EternalOpV1::ExileThenReturnTransformedV1)
}

/// "When this artifact enters, you may discard a card. If you do, draw two
/// cards."
fn may_discard_then_draw_two() -> EffectOp {
    EffectOp::MayPayCostThen {
        discard: 1,
        sacrifice_lands: 0,
        return_permanent: None,
        then: Box::new(EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 2,
        }),
        otherwise: None,
    }
}

/// "When this Equipment enters, you get {E}{E}."
fn get_two_energy() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::GainEnergy {
        amount: 2,
    }))
}

/// "When this Equipment enters, attach it to target creature you control."
fn attach_to_target() -> EffectOp {
    EffectOp::AttachSourceToTarget {
        object: ObjectRef::Target(0),
    }
}

fn draw_one() -> EffectOp {
    EffectOp::DrawCards {
        player: PlayerRef::Controller,
        count: 1,
    }
}

fn scry_two() -> EffectOp {
    EffectOp::Scry {
        player: PlayerRef::Controller,
        count: 2,
    }
}

/// "Look at the top three cards of your library. You may reveal an instant
/// or sorcery card from among them and put it into your hand. Put the rest
/// on the bottom of your library in any order." The look is private; only
/// the revealed card becomes public.
fn augur_look() -> EffectOp {
    EffectOp::LookTopSelectV1 {
        player: PlayerRef::Controller,
        count: LibraryLookCount::Fixed(3),
        rule: LibraryPickRule {
            pick: 1,
            choose_rest_order: true,
            selection: Some(LibraryPickSelectionV1 {
                without_mana_cost: crate::engine::FreeCastV1(false),
                filter: LibraryPickFilterV1::InstantOrSorcery,
                optional: true,
                destination: Zone::Hand,
                reveal_selected: true,
            }),
        },
        pick_x: false,
    }
}

fn become_monarch() -> EffectOp {
    EffectOp::BecomeMonarch
}

/// "When this land enters, return a land you control to its owner's hand."
/// Not targeted; the land is chosen on resolution and may be this one.
fn return_controlled_land() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::PlayerChoosesControlledPermanent {
        player: PlayerRef::Controller,
        filter: StandardPermanentFilterV1::Land,
        action: StandardChosenActionV1::ReturnToOwnersHand,
    })
}

const MELDED_MOXITE_TRIGGERS: [TriggeredAbilityDef; 1] = [etb(may_discard_then_draw_two)];
const INVENTORS_AXE_TRIGGERS: [TriggeredAbilityDef; 2] =
    [etb(get_two_energy), etb(attach_to_target)];
const BALEFUL_STRIX_TRIGGERS: [TriggeredAbilityDef; 1] = [etb(draw_one)];
const GIANTS_BOULDER_TRIGGERS: [TriggeredAbilityDef; 1] = [etb(scry_two)];
const AUGUR_OF_BOLAS_TRIGGERS: [TriggeredAbilityDef; 1] = [etb(augur_look)];
const THORN_OF_THE_BLACK_ROSE_TRIGGERS: [TriggeredAbilityDef; 1] = [etb(become_monarch)];
const DIMIR_AQUEDUCT_TRIGGERS: [TriggeredAbilityDef; 1] = [etb(return_controlled_land)];
const DRAGONS_RAGE_CHANNELER_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::CastNoncreatureSpell, surveil_one)];
const MURKTIDE_REGENT_TRIGGERS: [TriggeredAbilityDef; 1] = [eternal_trigger(
    EternalTriggerV1::InstantOrSorceryCardLeavesYourGraveyard,
    plus_one_counter_on_source,
)];
const MOONSHADOW_TRIGGERS: [TriggeredAbilityDef; 1] = [eternal_trigger(
    EternalTriggerV1::PermanentCardsToYourGraveyardWhileMinusCounter,
    remove_minus_one_counter_from_source,
)];
const BILBO_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::Attacks,
    bilbo_cast_from_graveyard,
)];
const TAMIYO_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Attacks, tamiyo_investigate),
    trigger(TriggerCondition::DrawNth(3), tamiyo_transform),
];
const ORCISH_BOWMASTERS_TRIGGERS: [TriggeredAbilityDef; 2] = [
    etb(bowmasters_ping_then_amass),
    eternal_trigger(
        EternalTriggerV1::OpponentDrawsExceptFirstInDrawStep,
        bowmasters_ping_then_amass,
    ),
];

/// Triggered abilities of this module's cards.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        MELDED_MOXITE => &MELDED_MOXITE_TRIGGERS,
        INVENTORS_AXE => &INVENTORS_AXE_TRIGGERS,
        BALEFUL_STRIX => &BALEFUL_STRIX_TRIGGERS,
        GIANTS_BOULDER => &GIANTS_BOULDER_TRIGGERS,
        AUGUR_OF_BOLAS => &AUGUR_OF_BOLAS_TRIGGERS,
        THORN_OF_THE_BLACK_ROSE => &THORN_OF_THE_BLACK_ROSE_TRIGGERS,
        DIMIR_AQUEDUCT => &DIMIR_AQUEDUCT_TRIGGERS,
        DRAGONS_RAGE_CHANNELER => &DRAGONS_RAGE_CHANNELER_TRIGGERS,
        MURKTIDE_REGENT => &MURKTIDE_REGENT_TRIGGERS,
        MOONSHADOW => &MOONSHADOW_TRIGGERS,
        ORCISH_BOWMASTERS => &ORCISH_BOWMASTERS_TRIGGERS,
        TAMIYO => &TAMIYO_TRIGGERS,
        BILBO => &BILBO_TRIGGERS,
        _ => match e3::triggers_for(name) {
            [] => crate::eternal_vintage_v1::triggers_for(name),
            triggers => triggers,
        },
    }
}

/// Target specs of this module's triggered abilities, per ability.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    match name {
        INVENTORS_AXE => Some(if *effect == attach_to_target() {
            TargetSpec::ControlledCreature
        } else {
            TargetSpec::None
        }),
        ORCISH_BOWMASTERS => Some(if *effect == bowmasters_ping_then_amass() {
            TargetSpec::AnyTarget
        } else {
            TargetSpec::None
        }),
        DRAGONS_RAGE_CHANNELER
        | MURKTIDE_REGENT
        | MOONSHADOW
        | MISHRAS_BAUBLE
        | URZAS_BAUBLE
        | TAMIYO
        | BILBO => Some(TargetSpec::None),
        _ => e3::trigger_target_spec(name, effect)
            .or_else(|| crate::eternal_vintage_v1::trigger_target_spec(name, effect)),
    }
}

/// Agony Warp: "Target creature gets -3/-0 until end of turn. Target
/// creature gets -0/-3 until end of turn."
pub fn agony_warp() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(
        EternalOpV1::PumpEachLegalTargetV1 {
            pumps: vec![(-3, 0), (0, -3)],
        },
    ))
}

/// Plunder the Trollshaws: "Draw a card. If this spell was cast from a
/// graveyard, draw two cards instead." Flashback is the card's own
/// `CardDef::flashback`.
pub fn plunder_the_trollshaws() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(
        EternalOpV1::DrawCastFromGraveyardV1 {
            normal: 1,
            from_graveyard: 2,
        },
    ))
}

/// Thoughtseize: "Target player reveals their hand. You choose a nonland
/// card from it. That player discards that card. You lose 2 life." The
/// target may be the caster.
pub fn thoughtseize() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::RevealTargetHandChooseNonlandDiscard {
            player: PlayerRef::Target(0),
        },
        EffectOp::LoseLife {
            player: PlayerRef::Controller,
            amount: 2,
        },
    ])
}

/// Fatal Push: "Destroy target creature if it has mana value 2 or less.
/// Revolt -- Destroy that creature if it has mana value 4 or less instead
/// if a permanent left the battlefield under your control this turn."
pub fn fatal_push() -> EffectOp {
    eternal(EternalOpV1::DestroyTargetCreatureManaValueAtMostRevoltV1 {
        normal: 2,
        revolt: 4,
    })
}

/// Unholy Heat: "Unholy Heat deals 2 damage to target creature or
/// planeswalker. Delirium -- Unholy Heat deals 6 damage instead if there are
/// four or more card types among cards in your graveyard."
pub fn unholy_heat() -> EffectOp {
    eternal(EternalOpV1::DamageTargetDeliriumV1 {
        normal: 2,
        delirium: 6,
    })
}

/// Prismari Charm's first mode: "Surveil 2, then draw a card."
pub fn prismari_charm_surveil_draw() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::Surveil {
            player: PlayerRef::Controller,
            count: 2,
        },
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        },
    ])
}

/// Prismari Charm's second mode: "Prismari Charm deals 1 damage to each of
/// one or two targets."
pub fn prismari_charm_damage() -> EffectOp {
    eternal(EternalOpV1::DamageEachLegalTargetV1 {
        amount: 1,
        spec: TargetSpec::StandardV1(
            crate::standard_cards_v1::StandardTargetV1::OneOrTwoAnyTargetsV1,
        ),
    })
}

/// "Return target ... permanent to its owner's hand" (Prismari Charm's third
/// mode, Petty Theft).
pub fn return_target_to_owners_hand() -> EffectOp {
    EffectOp::MoveAllTargets {
        to_zone: Zone::Hand,
    }
}

/// Mishra's Bauble's ability: "Look at the top card of target player's
/// library. Draw a card at the beginning of the next turn's upkeep."
pub fn mishras_bauble() -> EffectOp {
    EffectOp::Sequence(vec![
        eternal(EternalOpV1::LookAtTopOfTargetLibraryV1),
        eternal(EternalOpV1::DrawAtNextUpkeepV1),
    ])
}

/// Urza's Bauble's ability: "Look at a card at random in target player's
/// hand. You draw a card at the beginning of the next turn's upkeep."
pub fn urzas_bauble() -> EffectOp {
    EffectOp::Sequence(vec![
        eternal(EternalOpV1::LookAtRandomCardInTargetHandV1),
        eternal(EternalOpV1::DrawAtNextUpkeepV1),
    ])
}

/// Tamiyo, Seasoned Scholar's +2: "Until your next turn, whenever a creature
/// attacks you or a planeswalker you control, it gets -1/-0 until end of
/// turn."
pub fn tamiyo_plus_two() -> EffectOp {
    eternal(EternalOpV1::ShrinkAttackersUntilYourNextTurnV1)
}

/// Tamiyo, Seasoned Scholar's -3: "Return target instant or sorcery card
/// from your graveyard to your hand. If it's a green card, add one mana of
/// any color."
pub fn tamiyo_minus_three() -> EffectOp {
    let return_it = EffectOp::MoveAllTargets {
        to_zone: Zone::Hand,
    };
    let any_color = EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: [
            ManaColor::W,
            ManaColor::U,
            ManaColor::B,
            ManaColor::R,
            ManaColor::G,
        ]
        .into_iter()
        .map(|color| EffectOp::AddMana {
            player: PlayerRef::Controller,
            colors: vec![color],
        })
        .collect(),
    };
    EffectOp::Conditional {
        cond: crate::effect::EffectCond::TargetIsColor(0, ManaColor::G),
        then: Box::new(EffectOp::Sequence(vec![return_it.clone(), any_color])),
        else_: Box::new(return_it),
    }
}

/// Tamiyo, Seasoned Scholar's -7.
pub fn tamiyo_minus_seven() -> EffectOp {
    eternal(EternalOpV1::DrawHalfLibraryThenNoMaximumHandSizeV1)
}

/// Campfire's second ability: commanders do not exist in these games, so
/// it shuffles the controller's graveyard into their library.
pub fn campfire_shuffle_graveyard() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(
        EternalOpV1::ShuffleGraveyardIntoLibrary,
    ))
}

pub(crate) fn execute(op: &EternalOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        EternalOpV1::VintageV1(op) => crate::eternal_vintage_v1::execute(op, ctx, state),
        EternalOpV1::LurrusV1(op) => crate::eternal_lurrus_v1::execute(op, ctx, state),
        EternalOpV1::E5bV1(op) => crate::eternal_legacy_e5b_v1::execute(op, ctx, state),
        EternalOpV1::GainEnergy { amount } => {
            let eternal = state.eternal_v1.get_or_insert_with(Default::default);
            let counters = &mut eternal.energy[ctx.controller.index()];
            *counters = counters.saturating_add(u32::from(*amount));
        }
        EternalOpV1::PumpEachLegalTargetV1 { pumps } => {
            let Some(def) = state
                .objects
                .try_get(ctx.source)
                .and_then(|source| CARD_DEFS.get(source.card_def as usize))
            else {
                return;
            };
            let spec = def.target_spec;
            for (index, &(power, toughness)) in pumps.iter().enumerate() {
                let Ok(slot) = u8::try_from(index) else {
                    return;
                };
                if !ctx.target_incarnation_matches(index, state)
                    || !crate::engine::effect_target_is_legal(
                        state,
                        ctx.source,
                        ctx.controller,
                        spec,
                        &ctx.targets,
                        index,
                    )
                {
                    continue;
                }
                crate::effect::execute(
                    &EffectOp::PumpTargetUntilEndOfTurnDynamic {
                        target: TargetRef::Target(slot),
                        power: DynamicValueDef::Fixed(power),
                        toughness: DynamicValueDef::Fixed(toughness),
                    },
                    ctx,
                    state,
                );
            }
        }
        EternalOpV1::DrawCastFromGraveyardV1 {
            normal,
            from_graveyard,
        } => {
            let cast_from_graveyard = state.objects.try_get(ctx.source).is_some_and(|source| {
                source.zone == Zone::Stack
                    && source
                        .v4
                        .spell_cast_origin
                        .is_some_and(|origin| origin.origin_zone == Zone::Graveyard)
            });
            crate::effect::execute(
                &EffectOp::DrawCards {
                    player: PlayerRef::Controller,
                    count: u32::from(if cast_from_graveyard {
                        *from_graveyard
                    } else {
                        *normal
                    }),
                },
                ctx,
                state,
            );
        }
        EternalOpV1::ShuffleGraveyardIntoLibrary => {
            let player = ctx.controller;
            let cards = state.players[player.index()].graveyard.clone();
            if !cards.is_empty() {
                event::propose_and_commit_batch(
                    state,
                    cards
                        .into_iter()
                        .map(|card| ProposedEvent::zone_change(card, Zone::Library))
                        .collect(),
                );
            }
            if state.shuffle_library(player).is_err() {
                state.engine.halted = Some((
                    crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                    ctx.source,
                ));
            }
        }
        EternalOpV1::DestroyTargetCreatureManaValueAtMostRevoltV1 { normal, revolt } => {
            let Some(Target::Object(object)) = ctx.targets.first().copied() else {
                return;
            };
            if !ctx.target_incarnation_matches(0, state)
                || state.objects.get(object).zone != Zone::Battlefield
            {
                return;
            }
            let maximum = if permanent_left_battlefield_this_turn(state, ctx.controller) {
                *revolt
            } else {
                *normal
            };
            if crate::engine::object_mana_value(state, object) <= maximum {
                crate::effect::execute(
                    &EffectOp::DestroyObject {
                        object: ObjectRef::Target(0),
                    },
                    ctx,
                    state,
                );
            }
        }
        EternalOpV1::DamageTargetDeliriumV1 { normal, delirium } => {
            let amount = if controller_has_delirium(ctx.controller, state) {
                *delirium
            } else {
                *normal
            };
            crate::effect::execute(
                &EffectOp::DealDamage {
                    target: TargetRef::Target(0),
                    amount,
                },
                ctx,
                state,
            );
        }
        EternalOpV1::DamageEachLegalTargetV1 { amount, spec } => {
            let events = (0..ctx.targets.len())
                .filter(|&index| {
                    ctx.target_incarnation_matches(index, state)
                        && crate::engine::effect_target_is_legal(
                            state,
                            ctx.source,
                            ctx.controller,
                            *spec,
                            &ctx.targets,
                            index,
                        )
                })
                .map(|index| ProposedEvent::damage(ctx.source, ctx.targets[index], *amount))
                .collect::<Vec<_>>();
            if !events.is_empty() {
                event::propose_and_commit_batch(state, events);
            }
        }
        EternalOpV1::LookAtTopOfTargetLibraryV1 => {
            if let Some(Target::Player(player)) = ctx.targets.first().copied() {
                state.reveal_library_top(ctx.controller, player, 1);
            }
        }
        EternalOpV1::LookAtRandomCardInTargetHandV1 => {
            let Some(Target::Player(player)) = ctx.targets.first().copied() else {
                return;
            };
            match state.random_hand_card_v1(player) {
                Ok(Some(card)) => {
                    if state
                        .reveal_hand_card(ctx.controller, player, card)
                        .is_err()
                    {
                        state.engine.halted = Some((
                            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                            ctx.source,
                        ));
                    }
                }
                Ok(None) => {}
                Err(_) => {
                    state.engine.halted = Some((
                        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                        ctx.source,
                    ));
                }
            }
        }
        EternalOpV1::DrawAtNextUpkeepV1 => {
            let Some(source) = ctx.ability_source_contract else {
                state.engine.halted = Some((
                    crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                    ctx.source,
                ));
                return;
            };
            state
                .eternal_v1
                .get_or_insert_with(Default::default)
                .upkeep_draws
                .push(DelayedUpkeepDrawV1 {
                    controller: ctx.controller,
                    source,
                });
        }
        EternalOpV1::DelayedUpkeepDrawV1 => crate::effect::execute(
            &EffectOp::DrawCards {
                player: PlayerRef::Controller,
                count: 1,
            },
            ctx,
            state,
        ),
        EternalOpV1::CreateOrcArmyIfNoneV1 => {
            let controls_army = state.players[ctx.controller.index()]
                .battlefield
                .iter()
                .any(|&object| crate::engine::has_effective_subtype(state, object, Subtype::Army));
            if controls_army {
                return;
            }
            let Some(token_def) = card_id_by_name(ORC_ARMY_TOKEN) else {
                state.engine.halted = Some((
                    crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                    ctx.source,
                ));
                return;
            };
            crate::effect::execute(
                &EffectOp::CreateToken {
                    token_def,
                    controller: PlayerRef::Controller,
                },
                ctx,
                state,
            );
        }
        EternalOpV1::ExileThenReturnTransformedV1 => {
            let Some(contract) = ctx.ability_source_contract else {
                state.engine.halted = Some((
                    crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                    ctx.source,
                ));
                return;
            };
            let Some(live) = state.objects.try_get(contract.source) else {
                return;
            };
            if live.zone != Zone::Battlefield
                || live.zone_change_count != contract.zone_change_count
                || live.v4.face_index != 0
                || CARD_DEFS[live.card_def as usize].transform_face.is_none()
            {
                return;
            }
            let owner = live.owner;
            event::propose_and_commit(
                state,
                ProposedEvent::zone_change(contract.source, Zone::Exile),
            );
            if state.objects.get(contract.source).zone == Zone::Exile {
                event::propose_and_commit(
                    state,
                    ProposedEvent::transformed_battlefield_return(contract.source, 1, owner),
                );
            }
        }
        EternalOpV1::ShrinkAttackersUntilYourNextTurnV1 => {
            let Some(source) = ctx.ability_source_contract else {
                state.engine.halted = Some((
                    crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                    ctx.source,
                ));
                return;
            };
            state
                .eternal_v1
                .get_or_insert_with(Default::default)
                .attack_shrinks
                .push(DelayedUpkeepDrawV1 {
                    controller: ctx.controller,
                    source,
                });
        }
        EternalOpV1::ShrinkAttackerV1 { attacker } => {
            let Some(live) = state.objects.try_get(attacker.object) else {
                return;
            };
            if live.zone == Zone::Battlefield
                && live.zone_change_count == attacker.zone_change_count
                && crate::engine::object_has_type(state, attacker.object, CardType::Creature)
            {
                crate::effect::install_temporary_boost(
                    state,
                    crate::effect::EffectObjectBinding {
                        object: attacker.object,
                        expected_zone: Zone::Battlefield,
                        expected_zone_change_count: attacker.zone_change_count,
                    },
                    -1,
                    0,
                    Keywords::NONE,
                );
            }
        }
        EternalOpV1::DrawHalfLibraryThenNoMaximumHandSizeV1 => {
            let library = state.players[ctx.controller.index()].library.len();
            let count = u32::try_from(library.div_ceil(2)).unwrap_or(u32::MAX);
            crate::effect::execute(
                &EffectOp::DrawCards {
                    player: PlayerRef::Controller,
                    count,
                },
                ctx,
                state,
            );
            state
                .eternal_v1
                .get_or_insert_with(Default::default)
                .no_maximum_hand_size[ctx.controller.index()] = true;
        }
        EternalOpV1::ProtectionFromEverythingUntilYourNextTurnV1
        | EternalOpV1::LoseLifePerBurdenCounterV1
        | EternalOpV1::BurdenCounterThenDrawV1
        | EternalOpV1::MayImprintFromHandV1
        | EternalOpV1::ChangeTargetOfTargetSpellV1
        | EternalOpV1::SearchBasicLandIfAtMostLandsV1 { .. }
        | EternalOpV1::SacrificeSourceV1
        | EternalOpV1::VeilOfSummerV1
        | EternalOpV1::DoomsdayPileV1
        | EternalOpV1::ThassasOracleV1
        | EternalOpV1::WinIfLibraryEmptyV1
        | EternalOpV1::ShowAndTellV1
        | EternalOpV1::SneakAttackV1
        | EternalOpV1::AnnihilatorV1 { .. }
        | EternalOpV1::AtraxaRevealV1
        | EternalOpV1::RevealUntilCreatureAttackingV1
        | EternalOpV1::EnterRoomV1 { .. }
        | EternalOpV1::RoomAbilityV1 { .. }
        | EternalOpV1::AcererakEntersV1
        | EternalOpV1::AcererakAttacksV1 => e3::execute(op, ctx, state),
        EternalOpV1::RemoveMinusOneCounterFromSourceV1 => {
            let Some(contract) = ctx.ability_source_contract else {
                return;
            };
            let Some(live) = state.objects.try_get(contract.source) else {
                return;
            };
            if live.zone == Zone::Battlefield
                && live.zone_change_count == contract.zone_change_count
                && live.counters.minus1_minus1 > 0
            {
                state
                    .objects
                    .get_mut(contract.source)
                    .counters
                    .minus1_minus1 -= 1;
            }
        }
    }
}

/// The number of distinct card types among `cards` (delirium, Nethergoyf).
/// Cards outside the stack have only their front-face characteristics.
pub fn card_type_count(state: &GameState, cards: &[ObjectId]) -> usize {
    const TYPES: [CardType; 7] = [
        CardType::Land,
        CardType::Creature,
        CardType::Instant,
        CardType::Sorcery,
        CardType::Artifact,
        CardType::Enchantment,
        CardType::Planeswalker,
    ];
    TYPES
        .iter()
        .filter(|&&card_type| {
            cards.iter().any(|&card| {
                state.objects.try_get(card).is_some_and(|object| {
                    CARD_DEFS
                        .get(object.card_def as usize)
                        .is_some_and(|def| def.has_type(card_type))
                })
            })
        })
        .count()
}

/// The cards in `player`'s graveyard.
fn graveyard_cards(state: &GameState, player: PlayerId) -> Vec<ObjectId> {
    state.players[player.index()]
        .graveyard
        .iter()
        .copied()
        .filter(|&card| {
            let object = state.objects.get(card);
            !object.v4.is_token && object.spell_copy_origin.is_none()
        })
        .collect()
}

/// Delirium: four or more card types among cards in `player`'s graveyard.
pub fn controller_has_delirium(player: PlayerId, state: &GameState) -> bool {
    card_type_count(state, &graveyard_cards(state, player)) >= 4
}

/// Revolt: whether a permanent left the battlefield under `player`'s
/// control this turn. The turn begins at its `UpkeepBegan` marker; nothing
/// leaves the battlefield during an untap step.
pub fn permanent_left_battlefield_this_turn(state: &GameState, player: PlayerId) -> bool {
    for event in state.engine.event_history.iter().rev() {
        match event {
            CommittedEvent::UpkeepBegan { .. } => return false,
            CommittedEvent::ZoneChange {
                from: Zone::Battlefield,
                controller_before,
                ..
            } if *controller_before == player => return true,
            _ => {}
        }
    }
    false
}

fn definition_name(state: &GameState, object: ObjectId) -> Option<&'static str> {
    let live = state.objects.try_get(object)?;
    CARD_DEFS.get(live.card_def as usize).map(|def| def.name)
}

/// Whether `source` is a front-face battlefield permanent named `name`
/// whose printed abilities apply.
fn printed_battlefield_source(state: &GameState, source: ObjectId, name: &str) -> bool {
    state.objects.try_get(source).is_some_and(|live| {
        live.zone == Zone::Battlefield
            && live.v4.face_index == 0
            && CARD_DEFS
                .get(live.card_def as usize)
                .is_some_and(|def| def.name == name && def.is_executable())
            && crate::continuous_characteristics_v1::printed_abilities_active(state, source)
    })
}

/// Dragon's Rage Channeler's delirium flying.
pub(crate) fn conditional_self_keywords(state: &GameState, id: ObjectId) -> Keywords {
    if printed_battlefield_source(state, id, DRAGONS_RAGE_CHANNELER)
        && controller_has_delirium(state.objects.get(id).controller, state)
    {
        Keywords::FLYING
    } else {
        Keywords::NONE
    }
}

/// Dragon's Rage Channeler's delirium "attacks each combat if able". `None`
/// for every other card, which keeps its printed combat statics.
pub(crate) fn combat_static_rules(
    state: &GameState,
    source: ObjectId,
) -> Option<CombatStaticRulesDef> {
    if definition_name(state, source)? != DRAGONS_RAGE_CHANNELER {
        return None;
    }
    let active = printed_battlefield_source(state, source, DRAGONS_RAGE_CHANNELER)
        && crate::engine::object_has_type(state, source, CardType::Creature)
        && controller_has_delirium(state.objects.get(source).controller, state);
    Some(if active {
        CombatStaticRulesDef {
            must_attack_each_combat: true,
            cant_be_blocked_by_subtype: None,
        }
    } else {
        CombatStaticRulesDef::NONE
    })
}

/// Brazen Borrower: "This creature can block only creatures with flying."
pub(crate) fn blocker_allowed(state: &GameState, attacker: ObjectId, blocker: ObjectId) -> bool {
    (!printed_battlefield_source(state, blocker, BRAZEN_BORROWER)
        || crate::engine::has_effective_keyword(state, attacker, Keywords::FLYING))
        && e3::blocker_allowed(state, attacker, blocker)
        && crate::eternal_vintage_v1::blocker_allowed(state, attacker, blocker)
}

/// Nethergoyf: "Nethergoyf's power is equal to the number of card types
/// among cards in your graveyard and its toughness is equal to that number
/// plus 1." A characteristic-defining ability works in every zone.
pub(crate) fn characteristic_defining_power(state: &GameState, id: ObjectId) -> Option<i32> {
    if definition_name(state, id)? != NETHERGOYF {
        return crate::eternal_vintage_v1::characteristic_defining_power(state, id);
    }
    let controller = state.objects.get(id).controller;
    i32::try_from(card_type_count(state, &graveyard_cards(state, controller))).ok()
}

pub(crate) fn characteristic_defining_toughness(state: &GameState, id: ObjectId) -> Option<i32> {
    if definition_name(state, id)? != NETHERGOYF {
        // Karn's and the Tezzeret emblem's animations set both stats.
        return crate::eternal_vintage_v1::characteristic_defining_power(state, id);
    }
    characteristic_defining_power(state, id).map(|power| power + 1)
}

/// "Enters with" counters: Murktide Regent's +1/+1 counter for each
/// instant and sorcery card its Delve exiled, Moonshadow's six -1/-1
/// counters.
pub(crate) fn apply_entry_counters(
    state: &mut GameState,
    object: ObjectId,
    input: Option<&crate::doubling_season_v48::ReplacementInputV48>,
) {
    let Some(name) = definition_name(state, object) else {
        return;
    };
    let live = state.objects.get(object);
    if live.zone != Zone::Battlefield || live.v4.face_index != 0 {
        return;
    }
    let (controller, zone_change_count) = (live.controller, live.zone_change_count);
    match name {
        MURKTIDE_REGENT => {
            let Some(eternal) = state.eternal_v1.as_mut() else {
                return;
            };
            let Some(position) =
                eternal
                    .delve_instants_and_sorceries
                    .iter()
                    .position(|(spell, _)| {
                        spell.object == object
                            && spell.zone_change_count.checked_add(1) == Some(zone_change_count)
                    })
            else {
                return;
            };
            let (_, count) = eternal.delve_instants_and_sorceries.remove(position);
            if count == 0 {
                return;
            }
            let count = i32::try_from(count).unwrap_or(i32::MAX);
            match crate::doubling_season_v48::entry_amount(state, input, controller, count)
                .and_then(|amount| {
                    state
                        .objects
                        .get(object)
                        .counters
                        .plus1_plus1
                        .checked_add(amount)
                        .ok_or_else(|| "entry counter overflow".to_string())
                }) {
                Ok(total) => state.objects.get_mut(object).counters.plus1_plus1 = total,
                Err(_) => {
                    state.engine.halted = Some((
                        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                        object,
                    ))
                }
            }
        }
        MOONSHADOW => match crate::doubling_season_v48::entry_amount(state, input, controller, 6)
            .and_then(|count| i16::try_from(count).map_err(|error| error.to_string()))
        {
            Ok(count) => state.objects.get_mut(object).counters.minus1_minus1 = count,
            Err(_) => {
                state.engine.halted = Some((
                    crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                    object,
                ))
            }
        },
        _ => {}
    }
}

/// Records how many instant and sorcery cards a Murktide Regent spell's
/// Delve exiled, for its "enters with" counters.
pub(crate) fn note_delve_exile(state: &mut GameState, spell: ObjectId, exiled: &[ObjectId]) {
    if definition_name(state, spell) != Some(MURKTIDE_REGENT) {
        return;
    }
    let count = exiled
        .iter()
        .filter(|&&card| {
            state.objects.try_get(card).is_some_and(|object| {
                let def = &CARD_DEFS[object.card_def as usize];
                def.has_type(CardType::Instant) || def.has_type(CardType::Sorcery)
            })
        })
        .count();
    let link = ObjectLinkV4 {
        object: spell,
        zone_change_count: state.objects.get(spell).zone_change_count,
    };
    let eternal = state.eternal_v1.get_or_insert_with(Default::default);
    eternal
        .delve_instants_and_sorceries
        .retain(|(recorded, _)| recorded.object != spell);
    eternal
        .delve_instants_and_sorceries
        .push((link, u32::try_from(count).unwrap_or(u32::MAX)));
}

/// Marks the active player's turn-based draw, which Orcish Bowmasters
/// ignores. Recorded only in games with an Orcish Bowmasters.
pub(crate) fn note_draw_step_draw(state: &mut GameState, player: PlayerId) {
    let Some(bowmasters) = card_id_by_name(ORCISH_BOWMASTERS) else {
        return;
    };
    if !state
        .objects
        .iter()
        .any(|(_, object)| object.card_def == bowmasters)
    {
        return;
    }
    let marker = DrawStepDrawV1 {
        turn: state.turn_key_v1(),
        player,
        ordinal: state.players[player.index()].draws_this_turn,
    };
    state
        .eternal_v1
        .get_or_insert_with(Default::default)
        .draw_step_draw = Some(marker);
}

pub(crate) fn delayed_trigger(
    source: AbilitySourceContractV4,
    controller: PlayerId,
    effect: EffectOp,
) -> PendingTrigger {
    PendingTrigger {
        controller,
        source: source.source,
        effect,
        is_madness_offer: false,
        kicked: false,
        target_spec: TargetSpec::None,
        targets: Vec::new(),
        target_contracts: Vec::new(),
        placement_ordered: false,
        source_contract: Some(source),
        granted_by: None,
        optional_additional_cost_paid: None,
        paid_cost_refs: Vec::new(),
    }
}

/// The delayed triggers this collection sets off (603.7): the Baubles'
/// draws at the beginning of an upkeep and Tamiyo's attack shrinks.
pub(crate) fn delayed_triggers(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Vec<PendingTrigger> {
    let mut triggers = legacy_delayed_triggers(state, events);
    triggers.extend(crate::eternal_vintage_v1::delayed_triggers(state, events));
    triggers.extend(crate::eternal_lurrus_v1::delayed_triggers(state, events));
    triggers.extend(crate::eternal_legacy_e5b_v1::delayed_triggers(
        state, events,
    ));
    triggers
}

fn legacy_delayed_triggers(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Vec<PendingTrigger> {
    let upkeep = events
        .iter()
        .any(|event| matches!(event, CommittedEvent::UpkeepBegan { .. }));
    let active = state.active_player;
    let mut triggers = e3::delayed_triggers(state, events);
    let Some(eternal_state) = state.eternal_v1.as_mut() else {
        return triggers;
    };
    if upkeep {
        triggers.extend(
            std::mem::take(&mut eternal_state.upkeep_draws)
                .into_iter()
                .map(|draw| delayed_trigger(draw.source, draw.controller, delayed_upkeep_draw())),
        );
    }
    let attackers = std::mem::take(&mut eternal_state.shrinking_attackers);
    for shrink in eternal_state
        .attack_shrinks
        .iter()
        .filter(|shrink| shrink.controller != active)
    {
        for &attacker in &attackers {
            triggers.push(delayed_trigger(
                shrink.source,
                shrink.controller,
                eternal(EternalOpV1::ShrinkAttackerV1 { attacker }),
            ));
        }
    }
    triggers
}

/// Delayed triggers this module creates, owned by their source's card.
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    if e3::delayed_effect_matches(name, effect) {
        return true;
    }
    match name {
        MISHRAS_BAUBLE | URZAS_BAUBLE => *effect == delayed_upkeep_draw(),
        TAMIYO => matches!(
            effect,
            EffectOp::StandardV1(StandardOpV1::EternalV1(
                EternalOpV1::ShrinkAttackerV1 { .. }
            ))
        ),
        _ => crate::eternal_vintage_v1::delayed_effect_matches(name, effect),
    }
}

/// Records the attackers just declared against a player with an active
/// Tamiyo +2, for the next trigger collection. Every attacker of the
/// defending player attacks them or a planeswalker they control.
pub(crate) fn note_attackers_declared(state: &mut GameState) {
    let active = state.active_player;
    let attackers = state
        .engine
        .combat
        .attackers
        .iter()
        .map(|&object| ObjectLinkV4 {
            object,
            zone_change_count: state.objects.get(object).zone_change_count,
        })
        .collect::<Vec<_>>();
    let Some(eternal) = state.eternal_v1.as_mut() else {
        return;
    };
    if eternal
        .attack_shrinks
        .iter()
        .any(|shrink| shrink.controller != active)
    {
        eternal.shrinking_attackers = attackers;
    }
}

/// The beginning of `player`'s turn ends their "until your next turn"
/// effects.
pub(crate) fn begin_turn(state: &mut GameState, player: PlayerId) {
    crate::eternal_vintage_v1::begin_turn(state, player);
    crate::eternal_lurrus_v1::begin_turn(state, player);
    crate::eternal_legacy_e5b_v1::begin_turn(state, player);
    if let Some(eternal) = state.eternal_v1.as_mut() {
        eternal
            .attack_shrinks
            .retain(|shrink| shrink.controller != player);
    }
    e3::begin_turn(state, player);
}

/// Whether `player` has an emblem with "You have no maximum hand size".
pub(crate) fn no_maximum_hand_size(state: &GameState, player: PlayerId) -> bool {
    state
        .eternal_v1
        .as_ref()
        .is_some_and(|eternal| eternal.no_maximum_hand_size[player.index()])
}

/// `player`'s draws this turn up to and including `events[index]`.
fn draw_ordinal_at(
    events: &[CommittedEvent],
    index: usize,
    player: PlayerId,
    state: &GameState,
) -> u32 {
    let is_draw = |event: &CommittedEvent| matches!(event, CommittedEvent::Draw { player: drawer, object: Some(_) } if *drawer == player);
    let in_batch = events.iter().filter(|event| is_draw(event)).count() as u32;
    let through = events[..=index]
        .iter()
        .filter(|event| is_draw(event))
        .count() as u32;
    state.players[player.index()]
        .draws_this_turn
        .saturating_sub(in_batch)
        .saturating_add(through)
}

/// Whether `event` puts a permanent card into `player`'s graveyard.
fn permanent_card_to_graveyard(
    state: &GameState,
    event: &CommittedEvent,
    player: PlayerId,
) -> bool {
    let CommittedEvent::ZoneChange {
        object,
        to: Zone::Graveyard,
        ..
    } = event
    else {
        return false;
    };
    state.objects.try_get(*object).is_some_and(|card| {
        let def = &CARD_DEFS[card.card_def as usize];
        card.owner == player
            && !card.v4.is_token
            && !def.is_token
            && card.spell_copy_origin.is_none()
            && [
                CardType::Artifact,
                CardType::Creature,
                CardType::Enchantment,
                CardType::Land,
                CardType::Planeswalker,
            ]
            .into_iter()
            .any(|card_type| def.has_type(card_type))
    })
}

pub(crate) fn trigger_matches(
    condition: EternalTriggerV1,
    events: &[CommittedEvent],
    index: usize,
    source: ObjectId,
    state: &GameState,
) -> bool {
    if condition == EternalTriggerV1::PutIntoGraveyardFromAnywhereV1 {
        return e3::trigger_matches(condition, events, index, source, state);
    }
    if let EternalTriggerV1::E5bV1(condition) = condition {
        if crate::eternal_legacy_e5b_v1::functions_on_stack(condition) {
            return crate::eternal_legacy_e5b_v1::trigger_matches(
                condition, events, index, source, state,
            );
        }
    }
    let Some(name) = definition_name(state, source) else {
        return false;
    };
    if !printed_battlefield_source(state, source, name) {
        return false;
    }
    let controller = state.objects.get(source).controller;
    match condition {
        EternalTriggerV1::VintageV1(condition) => {
            crate::eternal_vintage_v1::trigger_matches(condition, events, index, source, state)
        }
        EternalTriggerV1::LurrusV1(condition) => {
            crate::eternal_lurrus_v1::trigger_matches(condition, events, index, source, state)
        }
        EternalTriggerV1::E5bV1(condition) => {
            crate::eternal_legacy_e5b_v1::trigger_matches(condition, events, index, source, state)
        }
        EternalTriggerV1::InstantOrSorceryCardLeavesYourGraveyard => {
            let CommittedEvent::ZoneChange {
                object,
                from: Zone::Graveyard,
                ..
            } = events[index]
            else {
                return false;
            };
            state.objects.try_get(object).is_some_and(|card| {
                let def = &CARD_DEFS[card.card_def as usize];
                card.owner == controller
                    && !card.v4.is_token
                    && card.spell_copy_origin.is_none()
                    && (def.has_type(CardType::Instant) || def.has_type(CardType::Sorcery))
            })
        }
        EternalTriggerV1::PermanentCardsToYourGraveyardWhileMinusCounter => {
            state.objects.get(source).counters.minus1_minus1 > 0
                && permanent_card_to_graveyard(state, &events[index], controller)
                && !events[..index]
                    .iter()
                    .any(|event| permanent_card_to_graveyard(state, event, controller))
        }
        EternalTriggerV1::EntersAfterCastV1
        | EternalTriggerV1::PutIntoGraveyardFromAnywhereV1
        | EternalTriggerV1::EntersIfTombNotCompletedV1 => {
            e3::trigger_matches(condition, events, index, source, state)
        }
        EternalTriggerV1::OpponentDrawsExceptFirstInDrawStep => {
            let CommittedEvent::Draw {
                player,
                object: Some(_),
            } = events[index]
            else {
                return false;
            };
            if player == controller {
                return false;
            }
            let ordinal = draw_ordinal_at(events, index, player, state);
            !state
                .eternal_v1
                .as_ref()
                .and_then(|eternal| eternal.draw_step_draw)
                .is_some_and(|marker| {
                    marker.turn == state.turn_key_v1()
                        && marker.player == player
                        && marker.ordinal == ordinal
                })
        }
    }
}

/// Bilbo, Thief in the Night: "Spells you cast from anywhere other than your
/// hand cost {1} less to cast." The generic reduction `player` gets for
/// casting `source`, one per Bilbo they control.
pub(crate) fn outside_hand_cast_reduction(
    state: &GameState,
    source: ObjectId,
    player: PlayerId,
) -> u32 {
    let Some(object) = state.objects.try_get(source) else {
        return 0;
    };
    let origin = if object.zone == Zone::Stack {
        object.v4.spell_cast_origin.map(|origin| origin.origin_zone)
    } else {
        Some(object.zone)
    };
    if origin.is_none_or(|zone| zone == Zone::Hand) {
        return 0;
    }
    let bilbos = state.players[player.index()]
        .battlefield
        .iter()
        .filter(|&&permanent| {
            state.objects.get(permanent).controller == player
                && printed_battlefield_source(state, permanent, BILBO)
        })
        .count();
    u32::try_from(bilbos).unwrap_or(u32::MAX)
}

/// Bilbo's "If an instant or sorcery spell cast this way would be put into
/// your graveyard, exile it instead": a spell cast from its owner's graveyard
/// by a resolving instruction.
pub(crate) fn exiled_instead_of_graveyard(
    state: &GameState,
    item: &crate::state::StackItem,
) -> bool {
    !item.is_copy
        && item
            .v4
            .source_contract
            .and_then(|contract| contract.spell_cast_origin)
            .is_some_and(|origin| {
                origin.origin_zone == Zone::Graveyard
                    && matches!(
                        origin.route,
                        crate::state::SpellCastRouteV4::ResolvingEffectV1 { .. }
                    )
            })
        && (crate::engine::stack_spell_has_type(state, item, CardType::Instant)
            || crate::engine::stack_spell_has_type(state, item, CardType::Sorcery))
}
