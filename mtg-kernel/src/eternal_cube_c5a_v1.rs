//! Card programs for the eternal catalog's Vintage Cube wave C5a
//! (`docs/design/eternal_formats_support_v1.md`): statics, mana and counter-state creatures.
//!
//! `eternal_cards_v1` forwards this module's effect leaves here, and the
//! recipe tables of `build_eternal_c5a_v1.rs` name its programs. Every
//! table is keyed by a printed name that occurs only in the eternal registry,
//! so the Pauper, FDN and Standard catalogs never reach it. Behavior was read
//! from each card's current Oracle text and, where present, its XMage source.
//!
//! Where each card lives:
//!
//! - Delighted Halfling: "{T}: Add {C}." is the registry's mana ability; "{T}:
//!   Add one mana of any color. Spend this mana only to cast a legendary spell,
//!   and that spell can't be countered." is a `RestrictedManaAbilityDef` with
//!   `ManaSpendRestrictionDef::LegendarySpell` (Plaza of Heroes). A colored
//!   tap of the Halfling is always the restricted ability (its other ability
//!   makes only {C}), so `note_restricted_payment`, called as a spell's payment
//!   is finalized, marks a spell paid with that mana (tapped as it was paid or
//!   floating in the restricted pool) as uncounterable, as Cavern of Souls
//!   does (`e3::mark_spell_uncounterable`).
//! - Fanatic of Rhonas: "{T}: Add {G}." is the registry's mana ability. The
//!   ferocious ability is an additional mana ability whose action carries
//!   {C} (the primary ability already owns {G}, and an action names its
//!   ability by source and color, as Relic of Sauron's mixed pairs do) and adds
//!   {G}{G}{G}{G}, gated by `ManaAbilityConditionDef::
//!   ControllerControlsCreatureWithPowerAtLeastV1(4)`. Eternalize is an
//!   activated ability from the graveyard that exiles the card and creates the
//!   "Fanatic of Rhonas Eternalized Token" (Sacred Cat's embalm token): a 4/4
//!   black Zombie Snake Druid with both mana abilities.
//! - Carnage Interpreter: the entry trigger is Ox of Agonas's discard of the
//!   hand followed by four Clue tokens (Thraben Inspector's investigate); "as
//!   long as you have one or fewer cards in hand, +2/+2 and menace" is read
//!   live by `controlled_boost` and `conditional_keywords`.
//! - Keen-Eyed Curator: "{1}: Exile target card from a graveyard." records each
//!   exile with the Curator's battlefield incarnation (Ghost Vacuum's ledger);
//!   "four or more card types among cards exiled with this creature" is read
//!   live from the ledger, counting only cards still in exile.
//! - Baloth Prime: enters tapped (registry flag) with six stun counters
//!   (`apply_entry`, Moonshadow's entry counters; none when it enters without
//!   its abilities). "Whenever you sacrifice a
//!   land" is `delayed_triggers` over the committed `Sacrificed` events. The
//!   untap goes through the engine's untap, so a stun counter is removed
//!   instead (122.1g), whatever untaps it.
//! - Alpha Deathclaw: Monstrosity keeps a per-incarnation "monstrous" mark in
//!   `C5aStateV1`, puts the counters on through the counter replacement path
//!   and queues the "becomes monstrous" trigger for the creature's controller
//!   at that moment, if it still has its abilities; its entry trigger and that
//!   trigger are the same program and each chooses its own target.
//! - Ginger, Queen of Sweets: the entry trigger is Thorn of the Black Rose's
//!   monarch; "if you're the monarch" at each upkeep is checked as the trigger
//!   is put on the stack (`delayed_triggers`) and again as it resolves.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{
    card_id_by_name, CardType, DynamicValueDef, Keywords, TargetSpec, CARD_DEFS,
};
use crate::effect::{EffectContinuation, EffectOp, ExecCtx, PlayerRef};
use crate::eternal_cards_v1::EternalOpV1;
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::standard_cards_v1::{StandardOpV1, StandardTargetV1};
use crate::state::{AbilitySourceContractV4, GameState, ObjectLinkV4, Zone};
use crate::trigger::{PendingTrigger, TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

pub(crate) const DELIGHTED_HALFLING: &str = "Delighted Halfling";
pub(crate) const CARNAGE_INTERPRETER: &str = "Carnage Interpreter";
pub(crate) const KEEN_EYED_CURATOR: &str = "Keen-Eyed Curator";
pub(crate) const BALOTH_PRIME: &str = "Baloth Prime";
pub(crate) const ALPHA_DEATHCLAW: &str = "Alpha Deathclaw";
pub(crate) const GINGER: &str = "Ginger, Queen of Sweets";

const CLUE_TOKEN: &str = "Clue Token";
const BEAST_TOKEN: &str = "Baloth Beast Token";
const GINGERBRUTE_TOKEN: &str = "Gingerbrute Token";

/// Per-game state for this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Absent from every snapshot until a card
/// of this wave needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct C5aStateV1 {
    /// Keen-Eyed Curator's exiles: the Curator's battlefield incarnation and
    /// the exiled card's exile incarnation (607.2a).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub curator_exiles: Vec<(ObjectLinkV4, ObjectLinkV4)>,
    /// The Alpha Deathclaw incarnations that are monstrous (701.37).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub monstrous: Vec<ObjectLinkV4>,
}

impl C5aStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

pub(crate) fn c5a_state(state: &mut GameState) -> &mut C5aStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).c5a
}

fn read_state(state: &GameState) -> Option<&C5aStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.c5a)
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::C5aV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C5aOpV1 {
    /// Keen-Eyed Curator: exile target 0 (a card in a graveyard) and record it
    /// as exiled with the Curator.
    CuratorExileV1,
    /// Alpha Deathclaw: "Monstrosity 4."
    MonstrosityV1,
    /// Alpha Deathclaw: destroy target 0 (a permanent), if it is still legal.
    DestroyTargetV1,
    /// Baloth Prime: untap this creature, if it is still the incarnation that
    /// triggered.
    UntapSourceV1,
    /// Ginger: create a Gingerbrute token if the controller is the monarch.
    GingerbruteV1,
}

impl C5aOpV1 {
    /// Whether the leaf may stop for a player's choice (or a draw that a
    /// replacement can interrupt), so it runs in the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        match self {
            Self::CuratorExileV1
            | Self::MonstrosityV1
            | Self::DestroyTargetV1
            | Self::UntapSourceV1
            | Self::GingerbruteV1 => false,
        }
    }
}

fn c5a(op: C5aOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C5aV1(op)))
}

fn token_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

/// The definition ids of the cards whose statics are read live, once.
struct StaticIds {
    halfling: Option<u16>,
    carnage: Option<u16>,
    curator: Option<u16>,
    baloth: Option<u16>,
}

fn static_ids() -> &'static StaticIds {
    static IDS: OnceLock<StaticIds> = OnceLock::new();
    IDS.get_or_init(|| StaticIds {
        halfling: card_id_by_name(DELIGHTED_HALFLING),
        carnage: card_id_by_name(CARNAGE_INTERPRETER),
        curator: card_id_by_name(KEEN_EYED_CURATOR),
        baloth: card_id_by_name(BALOTH_PRIME),
    })
}

fn link(state: &GameState, object: ObjectId) -> ObjectLinkV4 {
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

fn halt(state: &mut GameState, source: ObjectId) {
    state.engine.halted = Some((
        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
        source,
    ));
}

/// Whether `object` is a front-face battlefield permanent whose printed
/// abilities apply.
fn active_front_face(state: &GameState, object: ObjectId) -> bool {
    state.objects.try_get(object).is_some_and(|live| {
        live.zone == Zone::Battlefield
            && live.v4.face_index == 0
            && crate::continuous_characteristics_v1::printed_abilities_active(state, object)
    })
}

// ---- Programs -----------------------------------------------------------------------

/// Carnage Interpreter: "When this creature enters, discard your hand, then
/// investigate four times." (Ox of Agonas's discard, then Thraben Inspector's
/// Clue four times.)
pub fn carnage_enters() -> EffectOp {
    let investigate = || EffectOp::CreateToken {
        token_def: token_id(CLUE_TOKEN),
        controller: PlayerRef::Controller,
    };
    EffectOp::Sequence(vec![
        EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E7aV1(
            crate::eternal_legacy_e7a_v1::E7aOpV1::DiscardHandV1,
        ))),
        investigate(),
        investigate(),
        investigate(),
        investigate(),
    ])
}

/// Keen-Eyed Curator: "{1}: Exile target card from a graveyard."
pub fn curator_exile() -> EffectOp {
    c5a(C5aOpV1::CuratorExileV1)
}

/// Alpha Deathclaw: "{5}{B}{G}: Monstrosity 4."
pub fn monstrosity_4() -> EffectOp {
    c5a(C5aOpV1::MonstrosityV1)
}

/// Alpha Deathclaw: "When this creature enters or becomes monstrous, destroy
/// target permanent."
pub fn deathclaw_destroy() -> EffectOp {
    c5a(C5aOpV1::DestroyTargetV1)
}

/// Baloth Prime: "Whenever you sacrifice a land, create a tapped 4/4 green
/// Beast creature token and untap this creature."
pub fn baloth_sacrifice_land() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::CreateTokensDynamic {
            token_def: token_id(BEAST_TOKEN),
            controller: PlayerRef::Controller,
            count: DynamicValueDef::Fixed(1),
            tapped: true,
        },
        c5a(C5aOpV1::UntapSourceV1),
    ])
}

/// Ginger, Queen of Sweets: "When Ginger enters, you become the monarch."
pub fn ginger_monarch() -> EffectOp {
    EffectOp::BecomeMonarch
}

/// Ginger, Queen of Sweets: "At the beginning of each upkeep, if you're the
/// monarch, create a Gingerbrute token."
pub fn ginger_upkeep() -> EffectOp {
    c5a(C5aOpV1::GingerbruteV1)
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

const CARNAGE_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, carnage_enters)];
const DEATHCLAW_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, deathclaw_destroy)];
const GINGER_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, ginger_monarch)];

/// This module's own printed triggers (without the fall-through).
fn own_triggers(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        CARNAGE_INTERPRETER => &CARNAGE_TRIGGERS,
        ALPHA_DEATHCLAW => &DEATHCLAW_TRIGGERS,
        GINGER => &GINGER_TRIGGERS,
        _ => &[],
    }
}

/// Triggered abilities of this module's cards, falling through to wave
/// C5b. Baloth Prime's and Ginger's triggers and Alpha Deathclaw's
/// "becomes monstrous" trigger are queued by `delayed_triggers` and the
/// leaf `MonstrosityV1`.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match own_triggers(name) {
        [] => crate::eternal_cube_c5b_v1::triggers_for(name),
        triggers => triggers,
    }
}

/// Target specs of this module's printed and queued triggered abilities.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    if name == ALPHA_DEATHCLAW && *effect == deathclaw_destroy() {
        return Some(TargetSpec::AnyPermanent);
    }
    if own_triggers(name)
        .iter()
        .any(|ability| (ability.effect)() == *effect)
    {
        return Some(TargetSpec::None);
    }
    delayed_effect_matches(name, effect).then_some(TargetSpec::None)
}

/// Queued triggers owned by this module's cards: Baloth Prime's, Ginger's and
/// Alpha Deathclaw's "becomes monstrous".
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    match name {
        BALOTH_PRIME => *effect == baloth_sacrifice_land(),
        GINGER => *effect == ginger_upkeep(),
        ALPHA_DEATHCLAW => *effect == deathclaw_destroy(),
        _ => false,
    }
}

/// The triggers this collection sets off that no printed condition matches:
/// Baloth Prime's (one for each land its controller sacrificed) and Ginger's
/// (one for each Ginger whose controller is the monarch as an upkeep begins).
pub(crate) fn delayed_triggers(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Vec<PendingTrigger> {
    let mut triggers = Vec::new();
    if !cfg!(feature = "eternal-fixtures") {
        return triggers;
    }
    ginger_triggers(state, events, &mut triggers);
    baloth_triggers(state, events, &mut triggers);
    triggers
}

/// Ginger: "At the beginning of each upkeep, if you're the monarch, ..." The
/// intervening "if" holds as the trigger would be put on the stack.
fn ginger_triggers(state: &GameState, events: &[CommittedEvent], out: &mut Vec<PendingTrigger>) {
    if !events
        .iter()
        .any(|event| matches!(event, CommittedEvent::UpkeepBegan { .. }))
    {
        return;
    }
    let Some(monarch) = state.monarch else {
        return;
    };
    for ginger in crate::eternal_e5a_v1::active_permanents(state, GINGER, Some(monarch)) {
        out.push(crate::eternal_cards_v1::delayed_trigger(
            AbilitySourceContractV4::capture(state, ginger),
            monarch,
            ginger_upkeep(),
        ));
    }
}

/// Baloth Prime: "Whenever you sacrifice a land, ..." One trigger per Baloth
/// the sacrificing player controls for each land sacrificed. The land is a
/// card (or token) that has just left the battlefield, so its printed types
/// are read.
fn baloth_triggers(state: &GameState, events: &[CommittedEvent], out: &mut Vec<PendingTrigger>) {
    for event in events {
        let CommittedEvent::Sacrificed {
            object,
            controller_before,
            ..
        } = event
        else {
            continue;
        };
        let is_land = state.objects.try_get(*object).is_some_and(|live| {
            CARD_DEFS
                .get(usize::from(live.card_def))
                .is_some_and(|def| def.has_type(CardType::Land))
        });
        if !is_land {
            continue;
        }
        for baloth in
            crate::eternal_e5a_v1::active_permanents(state, BALOTH_PRIME, Some(*controller_before))
        {
            out.push(crate::eternal_cards_v1::delayed_trigger(
                AbilitySourceContractV4::capture(state, baloth),
                *controller_before,
                baloth_sacrifice_land(),
            ));
        }
    }
}

// ---- Execution ----------------------------------------------------------------------

/// Runs one of this module's leaves in synchronous execution.
pub(crate) fn execute(op: &C5aOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        C5aOpV1::CuratorExileV1 => curator_exile_effect(ctx, state),
        C5aOpV1::MonstrosityV1 => monstrosity_effect(ctx, state),
        C5aOpV1::DestroyTargetV1 => {
            if crate::eternal_premodern_p1a_v1::legal_object_target(
                ctx,
                state,
                0,
                TargetSpec::AnyPermanent,
            )
            .is_some()
            {
                crate::effect::execute(
                    &EffectOp::DestroyObject {
                        object: crate::effect::ObjectRef::Target(0),
                    },
                    ctx,
                    state,
                );
            }
        }
        C5aOpV1::UntapSourceV1 => {
            if let Some(source) = crate::eternal_premodern_p1a_v1::live_ability_source(ctx, state) {
                crate::engine::attempt_untap(state, source);
            }
        }
        C5aOpV1::GingerbruteV1 => {
            if state.monarch == Some(ctx.controller) {
                crate::effect::execute(
                    &EffectOp::CreateToken {
                        token_def: token_id(GINGERBRUTE_TOKEN),
                        controller: PlayerRef::Controller,
                    },
                    ctx,
                    state,
                );
            }
        }
    }
}

/// Runs one of this module's leaves in the resumable interpreter.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &C5aOpV1,
    _path: Vec<u16>,
) -> Result<bool, String> {
    execute(op, &continuation.ctx, state);
    Ok(false)
}

/// Keen-Eyed Curator: the target card, if it is still in a graveyard, is
/// exiled and recorded with the Curator incarnation that activated the ability
/// (the records of a Curator that left are never read again).
fn curator_exile_effect(ctx: &ExecCtx, state: &mut GameState) {
    let spec = TargetSpec::StandardV1(StandardTargetV1::CardInAGraveyard);
    let Some(card) = crate::eternal_premodern_p1a_v1::legal_object_target(ctx, state, 0, spec)
    else {
        return;
    };
    let Some(contract) = ctx.ability_source_contract else {
        halt(state, ctx.source);
        return;
    };
    let curator = ObjectLinkV4 {
        object: contract.source,
        zone_change_count: contract.zone_change_count,
    };
    event::propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Exile));
    if !link_live_in(state, curator, Zone::Battlefield)
        || state
            .objects
            .try_get(card)
            .is_none_or(|live| live.zone != Zone::Exile)
    {
        return;
    }
    let exiled = link(state, card);
    let mut records: Vec<(ObjectLinkV4, ObjectLinkV4)> = read_state(state)
        .map(|c5a| c5a.curator_exiles.clone())
        .unwrap_or_default();
    records.retain(|(source, card)| {
        link_live_in(state, *source, Zone::Battlefield) && link_live_in(state, *card, Zone::Exile)
    });
    records.push((curator, exiled));
    c5a_state(state).curator_exiles = records;
}

/// The number of card types among the cards exiled with this Curator that are
/// still in exile.
fn curator_type_count(state: &GameState, curator: ObjectId) -> usize {
    let me = link(state, curator);
    let cards: Vec<ObjectId> = read_state(state)
        .map(|c5a| {
            c5a.curator_exiles
                .iter()
                .filter(|(source, card)| *source == me && link_live_in(state, *card, Zone::Exile))
                .map(|(_, card)| card.object)
                .collect()
        })
        .unwrap_or_default();
    if cards.is_empty() {
        return 0;
    }
    crate::eternal_cards_v1::card_type_count(state, &cards)
}

/// Alpha Deathclaw: "Monstrosity 4." (701.37a) If the creature isn't monstrous,
/// four +1/+1 counters go on it and it becomes monstrous, which triggers.
fn monstrosity_effect(ctx: &ExecCtx, state: &mut GameState) {
    let Some(source) = crate::eternal_premodern_p1a_v1::live_ability_source(ctx, state) else {
        return;
    };
    let me = link(state, source);
    let mut monstrous: Vec<ObjectLinkV4> = read_state(state)
        .map(|c5a| c5a.monstrous.clone())
        .unwrap_or_default();
    if monstrous.contains(&me) {
        return;
    }
    monstrous.retain(|record| link_live_in(state, *record, Zone::Battlefield));
    monstrous.push(me);
    c5a_state(state).monstrous = monstrous;
    let _ = event::add_plus_one_counters(state, source, ctx.controller, 4);
    // "When this creature ... becomes monstrous" is the creature's own
    // triggered ability. A creature that has lost its abilities has none, and
    // the player who controls the creature as it becomes monstrous controls
    // the trigger, whoever activated the ability.
    if !active_front_face(state, source) {
        return;
    }
    let controller = state.objects.get(source).controller;
    crate::eternal_premodern_p1a_v1::queue_trigger(
        state,
        AbilitySourceContractV4::capture(state, source),
        controller,
        deathclaw_destroy(),
        Some(TargetSpec::AnyPermanent),
    );
}

// ---- Hooks: statics -----------------------------------------------------------------

/// Power and toughness this module's static abilities give `object`: Carnage
/// Interpreter's "as long as you have one or fewer cards in hand, this creature
/// gets +2/+2" and Keen-Eyed Curator's "as long as there are four or more card
/// types among cards exiled with this creature, it gets +4/+4". Called from
/// `standard_cards_v1::controlled_boost`.
pub(crate) fn controlled_boost(state: &GameState, object: ObjectId) -> (i128, i128) {
    if !cfg!(feature = "eternal-fixtures") {
        return (0, 0);
    }
    let Some(live) = state.objects.try_get(object) else {
        return (0, 0);
    };
    let ids = static_ids();
    if Some(live.card_def) == ids.carnage {
        if active_front_face(state, object)
            && state.players[live.controller.index()].hand.len() <= 1
        {
            return (2, 2);
        }
    } else if Some(live.card_def) == ids.curator
        && active_front_face(state, object)
        && curator_type_count(state, object) >= 4
    {
        return (4, 4);
    }
    (0, 0)
}

/// The keywords this module's statics give `object`: Carnage Interpreter's menace
/// (same condition as its boost) and Keen-Eyed Curator's trample (same
/// condition as its boost). Called from both keyword queries of the engine.
pub(crate) fn conditional_keywords(state: &GameState, object: ObjectId) -> Keywords {
    if !cfg!(feature = "eternal-fixtures") {
        return Keywords::NONE;
    }
    let Some(live) = state.objects.try_get(object) else {
        return Keywords::NONE;
    };
    if live.zone != Zone::Battlefield {
        return Keywords::NONE;
    }
    let ids = static_ids();
    if Some(live.card_def) == ids.carnage {
        if active_front_face(state, object)
            && state.players[live.controller.index()].hand.len() <= 1
        {
            return Keywords::MENACE;
        }
    } else if Some(live.card_def) == ids.curator
        && active_front_face(state, object)
        && curator_type_count(state, object) >= 4
    {
        return Keywords::TRAMPLE;
    }
    Keywords::NONE
}

// ---- Hooks: entry and payment --------------------------------------------------------

/// Baloth Prime: "This creature enters tapped with six stun counters on it."
/// (The tapped entry is the registry flag.) Called from
/// `eternal_cards_v1::apply_entry_counters`.
pub(crate) fn apply_entry(
    state: &mut GameState,
    object: ObjectId,
    input: Option<&crate::doubling_season_v48::ReplacementInputV48>,
) {
    let Some(live) = state.objects.try_get(object) else {
        return;
    };
    if Some(live.card_def) != static_ids().baloth
        || live.zone != Zone::Battlefield
        || live.v4.face_index != 0
    {
        return;
    }
    // The entry replacement is a printed ability: a Baloth Prime that enters
    // without its abilities (Dress Down) gets no stun counters.
    if !crate::continuous_characteristics_v1::printed_abilities_active(state, object) {
        return;
    }
    let controller = state.objects.get(object).controller;
    match crate::doubling_season_v48::entry_amount(state, input, controller, 6)
        .and_then(|count| i16::try_from(count).map_err(|error| error.to_string()))
    {
        Ok(count) => {
            let stun = &mut state.objects.get_mut(object).counters.stun;
            *stun = stun.saturating_add(count);
        }
        Err(_) => halt(state, object),
    }
}

/// Delighted Halfling: "that spell can't be countered." A spell whose payment
/// spends mana of the Halfling's restricted ability (a source tapped for a
/// color, which only that ability makes, or a unit of its mana floating in the
/// restricted pool) can't be countered. Called as the payment is finalized,
/// before the units leave the pool.
pub(crate) fn note_restricted_payment(
    state: &mut GameState,
    player: PlayerId,
    spell: ObjectId,
    pool_used: &[usize],
    taps: &[(ObjectId, ManaColor)],
) {
    let Some(halfling) = static_ids().halfling else {
        return;
    };
    let tapped = taps.iter().any(|&(source, color)| {
        color != ManaColor::C
            && state
                .objects
                .try_get(source)
                .is_some_and(|live| live.card_def == halfling)
    });
    let floated = pool_used.iter().any(|&index| {
        state.players[player.index()]
            .restricted_mana_pool
            .0
            .get(index)
            .is_some_and(|unit| unit.source_card_def == halfling)
    });
    if (tapped || floated)
        && state
            .objects
            .try_get(spell)
            .is_some_and(|live| live.zone == Zone::Stack)
    {
        crate::eternal_cards_v1::e3::mark_spell_uncounterable(state, spell);
    }
}
