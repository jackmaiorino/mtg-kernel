//! Card programs for the eternal catalog's Vintage Cube wave C3b
//! (`docs/design/eternal_formats_support_v1.md`): creatures with small new pieces, and exalted.
//!
//! `eternal_cards_v1` forwards this module's effect leaves here, and the
//! recipe tables of `build_eternal_c3b_v1.rs` name its programs. Every
//! table is keyed by a printed name that occurs only in the eternal registry,
//! so the Pauper, FDN and Standard catalogs never reach it. Behavior was read
//! from each card's current Oracle text and, where present, its XMage source.
//!
//! Where each card lives:
//!
//! - Oliphaunt: Trample is a keyword row; Mountaincycling is a recipe of
//!   `build_eternal_c3b_v1.rs` (Eagles of the North's Plainscycling); the
//!   attack trigger is `OliphauntBoostV1`.
//! - Stingcaster Mage: Haste is a keyword row; the entry trigger is Snapcaster
//!   Mage's flashback grant (`eternal_lurrus_v1`).
//! - Third Path Iconoclast: a cast trigger creating the new "Soldier Artifact
//!   Token" (Thraben Inspector's `CreateToken` shape).
//! - Tireless Tracker: Landfall is `ControlledLandEnters` creating a Clue
//!   (Thraben Inspector); the counter is Writhing Chrysalis's sacrifice
//!   trigger with the Clue subtype.
//! - Samut, Hazoret's Champion, Marwyn, the Preserver and Spider-Woman:
//!   continuous effects read through `granted_keywords` (called from wave
//!   E9a's `statics::granted_keywords`) and `enters_tapped` (called from
//!   the two battlefield-entry sites of `event.rs` with the entering controller).
//! - Spellseeker: Heliod's Pilgrim's search with the new at-most filter
//!   `LibraryCardFilter::InstantOrSorceryManaValueAtMostV1`.
//! - Walking Ballista: Stonecoil Serpent's entry program (the engine reads
//!   both X symbols of `{X}{X}`), Hired Claw's counter ability and
//!   Triskelion's counter-removal ping.
//! - Exalted (Noble Hierarch, Ignoble Hierarch): the reusable keyword
//!   program. `EXALTED_SOURCES` lists each card with its instances of
//!   exalted; `note_attackers_declared` (called from
//!   `eternal_cards_v1::note_attackers_declared` as attackers are declared)
//!   queues one delayed trigger per instance among the attacking player's
//!   permanents when exactly one creature was declared as an attacker, and
//!   `ExaltedBoostV1` gives that creature +1/+1 until end of turn. A later
//!   card with exalted needs one row in `EXALTED_SOURCES`.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{card_id_by_name, CardType, Keywords, Subtype, TargetSpec};
use crate::effect::{
    EffectContinuation, EffectObjectBinding, EffectOp, ExecCtx, LibraryCardFilter, PlayerRef,
};
use crate::eternal_cards_v1::EternalOpV1;
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::{StandardOpV1, StandardTargetV1};
use crate::state::{AbilitySourceContractV4, GameState, Target, Zone};
use crate::trigger::{TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

pub(crate) const OLIPHAUNT: &str = "Oliphaunt";
pub(crate) const STINGCASTER_MAGE: &str = "Stingcaster Mage";
pub(crate) const THIRD_PATH_ICONOCLAST: &str = "Third Path Iconoclast";
pub(crate) const TIRELESS_TRACKER: &str = "Tireless Tracker";
pub(crate) const SAMUT: &str = "Samut, Hazoret's Champion";
pub(crate) const MARWYN: &str = "Marwyn, the Preserver";
pub(crate) const SPELLSEEKER: &str = "Spellseeker";
pub(crate) const NOBLE_HIERARCH: &str = "Noble Hierarch";
pub(crate) const IGNOBLE_HIERARCH: &str = "Ignoble Hierarch";
pub(crate) const SPIDER_WOMAN: &str = "Spider-Woman, Stunning Savior";

const SOLDIER_ARTIFACT_TOKEN: &str = "Soldier Artifact Token";
const CLUE_TOKEN: &str = "Clue Token";

/// Every card with exalted and its number of instances of it: "Whenever a
/// creature you control attacks alone, that creature gets +1/+1 until end of
/// turn." A card with this ability needs one row here (and its registry row);
/// each instance among the attacking player's permanents is its own trigger.
pub(crate) const EXALTED_SOURCES: [(&str, u8); 2] = [(NOBLE_HIERARCH, 1), (IGNOBLE_HIERARCH, 1)];

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::C3bV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C3bOpV1 {
    /// Oliphaunt: "another target creature you control gets +2/+0 and gains
    /// trample until end of turn", if the target is still that creature.
    OliphauntBoostV1,
    /// Exalted: the lone attacker, still the incarnation that attacked,
    /// gets +1/+1 until end of turn.
    ExaltedBoostV1 { attacker: EffectObjectBinding },
}

impl C3bOpV1 {
    /// Whether the leaf may stop for a player's choice (or a draw that a
    /// replacement can interrupt), so it runs in the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        match self {
            Self::OliphauntBoostV1 | Self::ExaltedBoostV1 { .. } => false,
        }
    }
}

fn c3b(op: C3bOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C3bV1(op)))
}

fn token_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

// ---- Programs -----------------------------------------------------------------------

/// Oliphaunt: "Whenever this creature attacks, another target creature you
/// control gets +2/+0 and gains trample until end of turn."
pub fn oliphaunt_attacks() -> EffectOp {
    c3b(C3bOpV1::OliphauntBoostV1)
}

/// Stingcaster Mage: "When this creature enters, target instant or sorcery
/// card in your graveyard gains flashback until end of turn. The flashback
/// cost is equal to its mana cost." Snapcaster Mage's grant.
pub fn stingcaster_grant() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::LurrusV1(
        crate::eternal_lurrus_v1::LurrusOpV1::GrantFlashbackV1,
    )))
}

/// Third Path Iconoclast: "Whenever you cast a noncreature spell, create a
/// 1/1 colorless Soldier artifact creature token."
pub fn iconoclast_token() -> EffectOp {
    EffectOp::CreateToken {
        token_def: token_id(SOLDIER_ARTIFACT_TOKEN),
        controller: PlayerRef::Controller,
    }
}

/// Tireless Tracker: "Landfall -- Whenever a land you control enters,
/// investigate." (a Clue token, as Thraben Inspector's investigate).
pub fn tracker_investigate() -> EffectOp {
    EffectOp::CreateToken {
        token_def: token_id(CLUE_TOKEN),
        controller: PlayerRef::Controller,
    }
}

/// Tireless Tracker: "Whenever you sacrifice a Clue, put a +1/+1 counter on
/// this creature." Trigger collection binds the template to the Tracker's
/// own incarnation (Writhing Chrysalis's shape), so a Tracker that left the
/// battlefield gets nothing.
pub fn tracker_counter() -> EffectOp {
    EffectOp::BindPlusOnePlusOneCounterToTriggerSource
}

/// Spellseeker: "When this creature enters, you may search your library for
/// an instant or sorcery card with mana value 2 or less, reveal it, put it
/// into your hand, then shuffle." (Heliod's Pilgrim's search: declining does
/// not shuffle, and a search may find nothing.)
pub fn spellseeker_search() -> EffectOp {
    crate::eternal_cards_v1::may_search(
        PlayerRef::Controller,
        EffectOp::SearchLibraryToHand {
            player: PlayerRef::Controller,
            filter: LibraryCardFilter::InstantOrSorceryManaValueAtMostV1(2),
        },
    )
}

/// Marwyn, the Preserver: "{2}: Return target land card from your graveyard
/// to your hand." (Carnivorous Cultivator's move.)
pub fn marwyn_return() -> EffectOp {
    EffectOp::MoveObject {
        object: crate::effect::ObjectRef::Target(0),
        to_zone: Zone::Hand,
    }
}

/// Walking Ballista: "This creature enters with X +1/+1 counters on it."
/// (Stonecoil Serpent's program; X is the chosen value, so `{X}{X}` costs
/// twice that.)
pub fn walking_ballista() -> EffectOp {
    EffectOp::PutSourceOntoBattlefieldWithXPlusOneCounters
}

/// The queued exalted trigger for `attacker`.
fn exalted_boost(attacker: EffectObjectBinding) -> EffectOp {
    c3b(C3bOpV1::ExaltedBoostV1 { attacker })
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

const OLIPHAUNT_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Attacks, oliphaunt_attacks)];
const STINGCASTER_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, stingcaster_grant)];
const ICONOCLAST_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::CastNoncreatureSpell,
    iconoclast_token,
)];
const TRACKER_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::ControlledLandEnters, tracker_investigate),
    trigger(
        TriggerCondition::SacrificeAnotherWithSubtype(Subtype::Clue),
        tracker_counter,
    ),
];
const SPELLSEEKER_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, spellseeker_search)];

/// Triggered abilities of this module's cards, falling through to wave
/// C3c. Exalted's triggers are delayed triggers.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        OLIPHAUNT => &OLIPHAUNT_TRIGGERS,
        STINGCASTER_MAGE => &STINGCASTER_TRIGGERS,
        THIRD_PATH_ICONOCLAST => &ICONOCLAST_TRIGGERS,
        TIRELESS_TRACKER => &TRACKER_TRIGGERS,
        SPELLSEEKER => &SPELLSEEKER_TRIGGERS,
        _ => crate::eternal_cube_c3c_v1::triggers_for(name),
    }
}

/// Target specs of this module's printed and queued triggered abilities.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    match name {
        OLIPHAUNT if *effect == oliphaunt_attacks() => Some(TargetSpec::AnotherControlledCreature),
        STINGCASTER_MAGE if *effect == stingcaster_grant() => Some(TargetSpec::StandardV1(
            StandardTargetV1::InstantOrSorceryCardInOwnGraveyard,
        )),
        THIRD_PATH_ICONOCLAST | TIRELESS_TRACKER | SPELLSEEKER
            if triggers_for(name)
                .iter()
                .any(|ability| (ability.effect)() == *effect) =>
        {
            Some(TargetSpec::None)
        }
        _ => delayed_effect_matches(name, effect).then_some(TargetSpec::None),
    }
}

/// Queued triggers owned by this module's cards: each exalted instance's
/// boost of the lone attacker.
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    exalted_instances(name) > 0
        && matches!(
            effect,
            EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C3bV1(
                C3bOpV1::ExaltedBoostV1 { .. }
            )))
        )
}

// ---- Execution ----------------------------------------------------------------------

/// Runs one of this module's leaves in synchronous execution.
pub(crate) fn execute(op: &C3bOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        C3bOpV1::OliphauntBoostV1 => run_oliphaunt_boost(ctx, state),
        C3bOpV1::ExaltedBoostV1 { attacker } => {
            crate::effect::install_temporary_boost(state, *attacker, 1, 1, Keywords::NONE);
        }
    }
}

/// Oliphaunt (608.2b): the target must still be the creature announced, on the
/// battlefield and controlled by the trigger's controller; otherwise the
/// ability does nothing.
fn run_oliphaunt_boost(ctx: &ExecCtx, state: &mut GameState) {
    let Some(Target::Object(object)) = ctx.targets.first().copied() else {
        return;
    };
    let Some(live) = state.objects.try_get(object) else {
        return;
    };
    if !ctx.target_incarnation_matches(0, state)
        || live.zone != Zone::Battlefield
        || live.controller != ctx.controller
        || !crate::engine::object_has_type(state, object, CardType::Creature)
    {
        return;
    }
    let binding = EffectObjectBinding {
        object,
        expected_zone: Zone::Battlefield,
        expected_zone_change_count: live.zone_change_count,
    };
    crate::effect::install_temporary_boost(state, binding, 2, 0, Keywords::TRAMPLE);
}

/// Runs one of this module's leaves in the resumable interpreter.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &C3bOpV1,
    _path: Vec<u16>,
) -> Result<bool, String> {
    execute(op, &continuation.ctx, state);
    Ok(false)
}

// ---- Exalted ------------------------------------------------------------------------

/// The instances of exalted `name` has (0 for a card without it).
pub(crate) fn exalted_instances(name: &str) -> u8 {
    EXALTED_SOURCES
        .iter()
        .find(|(source, _)| *source == name)
        .map_or(0, |(_, instances)| *instances)
}

/// Exalted (702.83): called as attackers are declared. When the active
/// player declared exactly one creature as an attacker, each instance of
/// exalted among their permanents queues one trigger that gives that creature
/// +1/+1 until end of turn when it resolves.
pub(crate) fn note_attackers_declared(state: &mut GameState) {
    if !cfg!(feature = "eternal-fixtures") {
        return;
    }
    let [attacker] = state.engine.combat.attackers.as_slice() else {
        return;
    };
    let attacker = *attacker;
    let player = state.active_player;
    let Some(live) = state.objects.try_get(attacker) else {
        return;
    };
    if live.zone != Zone::Battlefield || live.controller != player {
        return;
    }
    let binding = EffectObjectBinding {
        object: attacker,
        expected_zone: Zone::Battlefield,
        expected_zone_change_count: live.zone_change_count,
    };
    let mut sources = Vec::new();
    for (name, instances) in EXALTED_SOURCES {
        for source in crate::eternal_e5a_v1::active_permanents(state, name, Some(player)) {
            sources.push((source, instances));
        }
    }
    for (source, instances) in sources {
        for _ in 0..instances {
            let contract = AbilitySourceContractV4::capture(state, source);
            crate::eternal_premodern_p1a_v1::queue_trigger(
                state,
                contract,
                player,
                exalted_boost(binding),
                None,
            );
        }
    }
}

// ---- Statics ------------------------------------------------------------------------

/// The definition ids of the static-ability cards, once.
fn static_ids() -> &'static [Option<u16>; 3] {
    static IDS: OnceLock<[Option<u16>; 3]> = OnceLock::new();
    IDS.get_or_init(|| {
        [
            card_id_by_name(SAMUT),
            card_id_by_name(MARWYN),
            card_id_by_name(SPIDER_WOMAN),
        ]
    })
}

/// Whether `player` controls a battlefield permanent of the definition `id`
/// (named `name`) whose printed abilities apply.
fn controls_active(state: &GameState, player: PlayerId, id: Option<u16>, name: &str) -> bool {
    let Some(id) = id else {
        return false;
    };
    [PlayerId::P0, PlayerId::P1].into_iter().any(|seat| {
        state.players[seat.index()]
            .battlefield
            .iter()
            .any(|&object| {
                let live = state.objects.get(object);
                live.card_def == id
                    && live.controller == player
                    && crate::eternal_e5a_v1::active_named(state, object, name)
            })
    })
}

/// The keywords this module's statics grant `object`: Samut, Hazoret's
/// Champion ("Creatures you control have haste.") and Marwyn, the Preserver
/// ("Lands you control have hexproof."). Called from wave E9a's
/// `statics::granted_keywords`, which both keyword queries read.
pub(crate) fn granted_keywords(state: &GameState, object: ObjectId) -> Keywords {
    if !cfg!(feature = "eternal-fixtures") {
        return Keywords::NONE;
    }
    let Some(live) = state.objects.try_get(object) else {
        return Keywords::NONE;
    };
    if live.zone != Zone::Battlefield {
        return Keywords::NONE;
    }
    let controller = live.controller;
    let ids = static_ids();
    let mut keywords = Keywords::NONE;
    if controls_active(state, controller, ids[0], SAMUT)
        && crate::engine::object_has_type(state, object, CardType::Creature)
    {
        keywords = keywords | Keywords::HASTE;
    }
    if controls_active(state, controller, ids[1], MARWYN)
        && crate::engine::object_has_type(state, object, CardType::Land)
    {
        keywords = keywords | Keywords::HEXPROOF;
    }
    keywords
}

/// Spider-Woman, Stunning Savior: "Artifacts and creatures your opponents
/// control enter tapped." Whether `object`, entering under `controller`
/// (the player it enters under, who may not be its owner), enters tapped.
/// A permanent entering face down is a creature. Called from the two
/// battlefield-entry sites of `event.rs` once the permanent is on the
/// battlefield, so its current types decide (614.12): a Mycosynth Lattice
/// makes it an artifact.
pub(crate) fn enters_tapped(
    state: &GameState,
    object: ObjectId,
    controller: PlayerId,
    face_down: bool,
) -> bool {
    if !cfg!(feature = "eternal-fixtures") {
        return false;
    }
    if state.objects.try_get(object).is_none() {
        return false;
    }
    (face_down
        || crate::engine::object_has_type(state, object, CardType::Creature)
        || crate::engine::object_has_type(state, object, CardType::Artifact))
        && controls_active(state, controller.opponent(), static_ids()[2], SPIDER_WOMAN)
}
