//! Card programs for the eternal catalog's Vintage Cube wave C4a
//! (`docs/design/eternal_formats_support_v1.md`): spell-cast, draw and copy trigger creatures.
//!
//! `eternal_cards_v1` forwards this module's effect leaves here, and the
//! recipe tables of `build_eternal_c4a_v1.rs` name its programs. Every
//! table is keyed by a printed name that occurs only in the eternal registry,
//! so the Pauper, FDN and Standard catalogs never reach it. Behavior was read
//! from each card's current Oracle text and, where present, its XMage source.
//!
//! - Displacer Kitten: Dragon's Rage Channeler's cast trigger
//!   (`TriggerCondition::CastNoncreatureSpell`) with Ephemerate's flicker
//!   (`eternal_pauper_e6a_v1`), over the new filter `targets::C4aTargetV1`
//!   ("up to one target nonland permanent you control").
//! - Faerie Mastermind and Ledger Shredder: the per-player counters the
//!   engine already keeps (`PlayerState::draws_this_turn` and
//!   `spells_cast_this_turn`) read by two new trigger conditions, one
//!   trigger per qualifying event. The Shredder's connive is Raffine's
//!   Informant's program (`eternal_pauper_e6b_v1`).
//! - Forensic Gadgeteer: Patchwork Automaton's artifact cast trigger
//!   (`eternal_legacy_e7a_v1`) creating Thraben Inspector's Clue, and a
//!   static cost reduction read where an activation's mana cost is computed
//!   (`reduce_artifact_activation`).
//! - Jacked Rabbit: Stonecoil Serpent's entry program with a record of the
//!   announced X, read by the "if X is 5 or more" trigger, and an attack
//!   trigger creating tokens equal to its power.
//! - Bristly Bill: Spitfire Lagac's landfall trigger with a counter on a
//!   target creature, and a leaf doubling the counters of each creature.
//! - Lutri, the Spellchaser: The One Ring's "if you cast it" trigger, the
//!   companion special action of Lurrus and Yorion, and a spell copy built
//!   through the engine's copy snapshot with an optional new target per
//!   slot.
//! - Caustic Bronco: Dark Confidant's reveal with the attack trigger, and
//!   Saddle as Crew's cost with a per-turn marker.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

pub(crate) mod targets;

use crate::card_def::{CardType, DynamicValueDef, TargetSpec, CARD_DEFS};
use crate::effect::{
    EffectContinuation, EffectFrame, EffectObjectBinding, EffectOp, ExecCtx, ObjectRef, PlayerRef,
};
use crate::eternal_cards_v1::{EternalOpV1, EternalTriggerV1};
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::Cost;
use crate::standard_cards_v1::{StandardOpV1, StandardTargetV1, StandardTriggerV1};
use crate::state::{
    GameState, ObjectLinkV4, StackItem, StackItemKind, StackTargetContractV4, Target, Zone,
};
use crate::trigger::{TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};
use targets::C4aTargetV1;

const DISPLACER_KITTEN: &str = "Displacer Kitten";
const FAERIE_MASTERMIND: &str = "Faerie Mastermind";
const LEDGER_SHREDDER: &str = "Ledger Shredder";
const FORENSIC_GADGETEER: &str = "Forensic Gadgeteer";
const LUTRI: &str = "Lutri, the Spellchaser";
const CAUSTIC_BRONCO: &str = "Caustic Bronco";
const JACKED_RABBIT: &str = "Jacked Rabbit";
const BRISTLY_BILL: &str = "Bristly Bill, Spine Sower";
const CLUE_TOKEN: &str = "Clue Token";
const RABBIT_TOKEN: &str = "Rabbit Token";

/// Per-game state for this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Default (and absent from snapshots)
/// until a card of this wave needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct C4aStateV1 {
    /// Jacked Rabbits that entered with an announced X of 5 or more, by the
    /// battlefield incarnation that entered (Ravenous' "if X is 5 or more").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ravenous_entries: Vec<ObjectLinkV4>,
    /// Mounts that became saddled, by incarnation, and the turn they became
    /// saddled until the end of ("until end of turn").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub saddled: Vec<(ObjectLinkV4, u32)>,
}

impl C4aStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

pub(crate) fn c4a_state(state: &mut GameState) -> &mut C4aStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).c4a
}

fn read_state(state: &GameState) -> Option<&C4aStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.c4a)
}

/// Trigger conditions owned by this module, carried as
/// `eternal_cards_v1::EternalTriggerV1::C4aV1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum C4aTriggerV1 {
    /// "Whenever an opponent draws their second card each turn" (Faerie
    /// Mastermind): the draw that is that opponent's second this turn.
    OpponentDrawsSecondCardEachTurnV1,
    /// "Whenever a player casts their second spell each turn" (Ledger
    /// Shredder): either player's second cast of the turn.
    PlayerCastsSecondSpellEachTurnV1,
    /// "If X is 5 or more, draw a card when it enters" (Jacked Rabbit's
    /// Ravenous): the Rabbit entered with a recorded announced X of 5 or
    /// more.
    EntersWithXAtLeastFiveV1,
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::C4aV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C4aOpV1 {
    /// Displacer Kitten: exile target 0 (if one was chosen and is still
    /// legal), then return that card to the battlefield under its owner's
    /// control.
    KittenFlickerV1,
    /// Jacked Rabbit: Stonecoil Serpent's entry (X +1/+1 counters), and a
    /// record of an announced X of 5 or more for Ravenous' draw.
    RabbitEntersV1,
    /// Jacked Rabbit's attack trigger: create a 1/1 white Rabbit creature
    /// token for each point of the Rabbit's power (last known).
    RabbitTokensV1,
    /// Bristly Bill's second ability: each creature the controller controls
    /// gets as many +1/+1 counters as it has.
    BillDoubleV1,
    /// Saddle: the ability's source Mount becomes saddled until end of turn.
    SaddleV1,
    /// Caustic Bronco's attack trigger: reveal the top card of the
    /// controller's library and put it into their hand, then the controller
    /// loses life equal to its mana value, or each opponent does when the
    /// Bronco is saddled.
    BroncoRevealV1,
    /// Lutri's enters trigger: copy target 0, an instant or sorcery spell its
    /// controller controls. A choice of new targets follows.
    LutriCopyV1,
    /// One step of Lutri's copy: `targets` and `contracts` are the slots of
    /// the copy decided so far, the next slot is offered keep-or-new.
    LutriCopyStepV1 {
        spell: Box<StackItem>,
        targets: Vec<Target>,
        contracts: Vec<StackTargetContractV4>,
    },
}

impl C4aOpV1 {
    /// Whether the leaf may stop for a player's choice (or a draw that a
    /// replacement can interrupt), so it runs in the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(self, Self::LutriCopyV1 | Self::LutriCopyStepV1 { .. })
    }
}

fn c4a(op: C4aOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C4aV1(op)))
}

fn halt(state: &mut GameState, source: ObjectId) {
    state.engine.halted = Some((
        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
        source,
    ));
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

/// The resolving ability's source, if it is still the incarnation that
/// created the ability, on the battlefield.
fn live_ability_source(ctx: &ExecCtx, state: &GameState) -> Option<ObjectId> {
    let contract = ctx.ability_source_contract?;
    let live = state.objects.try_get(contract.source)?;
    (live.zone == Zone::Battlefield && live.zone_change_count == contract.zone_change_count)
        .then_some(contract.source)
}

/// Target `index`, if it is an object in `zone` still legal for the
/// resolving ability under `spec` (608.2b).
fn legal_target(
    ctx: &ExecCtx,
    state: &GameState,
    spec: TargetSpec,
    index: usize,
    zone: Zone,
) -> Option<ObjectId> {
    let Target::Object(object) = ctx.targets.get(index).copied()? else {
        return None;
    };
    (ctx.target_incarnation_matches(index, state)
        && state.objects.get(object).zone == zone
        && crate::engine::effect_target_is_legal(
            state,
            ctx.source,
            ctx.controller,
            spec,
            &ctx.targets[index..=index],
            0,
        ))
    .then_some(object)
}

fn token_id(name: &str) -> u16 {
    crate::card_def::card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

// ---- Programs -----------------------------------------------------------------------

/// Displacer Kitten: "Exile up to one target nonland permanent you control,
/// then return that card to the battlefield under its owner's control."
pub fn kitten_flicker() -> EffectOp {
    c4a(C4aOpV1::KittenFlickerV1)
}

/// Faerie Mastermind's trigger: "you draw a card". The draw is its own
/// instruction, so a draw replacement can interrupt it.
pub fn mastermind_draw_trigger() -> EffectOp {
    EffectOp::DrawCards {
        player: PlayerRef::Controller,
        count: 1,
    }
}

/// Faerie Mastermind: "{3}{U}: Each player draws a card." Each draw is its
/// own instruction; a draw that won the game ends it before the next. Wheel
/// of Fortune's order (the controller first) is followed.
pub fn mastermind_each_player_draws() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        },
        crate::eternal_cube_c1a_v1::stop_if_won(),
        EffectOp::DrawCards {
            player: PlayerRef::Opponent,
            count: 1,
        },
    ])
}

/// Ledger Shredder: "this creature connives. (Draw a card, then discard a
/// card. If you discarded a nonland card, put a +1/+1 counter on this
/// creature.)" Raffine's Informant's program.
pub fn shredder_connive() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        },
        EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E6bV1(
            crate::eternal_pauper_e6b_v1::E6bOpV1::ConniveDiscardV1,
        ))),
    ])
}

/// Forensic Gadgeteer: "investigate" (create a Clue token).
pub fn gadgeteer_investigate() -> EffectOp {
    EffectOp::CreateToken {
        token_def: token_id(CLUE_TOKEN),
        controller: PlayerRef::Controller,
    }
}

/// Jacked Rabbit: "Ravenous (This creature enters with X +1/+1 counters on
/// it. ...)" Stonecoil Serpent's entry, recording X.
pub fn jacked_rabbit() -> EffectOp {
    c4a(C4aOpV1::RabbitEntersV1)
}

/// Jacked Rabbit: "If X is 5 or more, draw a card when it enters."
pub fn rabbit_ravenous_draw() -> EffectOp {
    EffectOp::DrawCards {
        player: PlayerRef::Controller,
        count: 1,
    }
}

/// Jacked Rabbit: "Whenever this creature attacks, create a number of 1/1
/// white Rabbit creature tokens equal to this creature's power."
pub fn rabbit_tokens() -> EffectOp {
    c4a(C4aOpV1::RabbitTokensV1)
}

/// Bristly Bill: "Landfall -- Whenever a land you control enters, put a
/// +1/+1 counter on target creature."
pub fn bill_counter() -> EffectOp {
    EffectOp::PutPlusOnePlusOneCounter {
        object: ObjectRef::Target(0),
    }
}

/// Bristly Bill: "{3}{G}{G}: Double the number of +1/+1 counters on each
/// creature you control."
pub fn bill_double() -> EffectOp {
    c4a(C4aOpV1::BillDoubleV1)
}

/// Caustic Bronco's Saddle: "This Mount becomes saddled until end of turn."
pub fn saddle() -> EffectOp {
    c4a(C4aOpV1::SaddleV1)
}

/// Caustic Bronco's attack trigger.
pub fn bronco_reveal() -> EffectOp {
    c4a(C4aOpV1::BroncoRevealV1)
}

/// Lutri, the Spellchaser: "copy target instant or sorcery spell you
/// control. You may choose new targets for the copy."
pub fn lutri_copy() -> EffectOp {
    c4a(C4aOpV1::LutriCopyV1)
}

// ---- Helpers of the leaves ------------------------------------------------------------

/// Exile `object`, then return the card to the battlefield under its owner's
/// control. A token ceases to exist in exile (Ephemerate's flicker).
fn flicker(state: &mut GameState, object: ObjectId) {
    event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Exile));
    let live = state.objects.get(object);
    if live.zone != Zone::Exile || live.v4.is_token {
        return;
    }
    let owner = live.owner;
    event::propose_and_commit(
        state,
        ProposedEvent::zone_change_to_battlefield_under_controller(object, owner),
    );
}

/// The top card of `player`'s library goes to their hand, revealed to both
/// players; its printed mana value, or `None` when the library is empty or
/// the reveal failed.
fn reveal_top_card_to_hand(
    state: &mut GameState,
    player: PlayerId,
    source: ObjectId,
) -> Option<u16> {
    let &top = state.players[player.index()].library.first()?;
    let mana_value = CARD_DEFS[state.objects.get(top).card_def as usize].mana_value;
    event::propose_and_commit(state, ProposedEvent::zone_change(top, Zone::Hand));
    if state.objects.get(top).zone == Zone::Hand {
        // spellbench-public-reveal: hand card revealed by the card rules.
        for observer in [PlayerId::P0, PlayerId::P1] {
            if state
                .reveal_distinct_hand_card(observer, player, top)
                .is_err()
            {
                halt(state, source);
                return None;
            }
        }
    }
    Some(mana_value)
}

/// Whether the Mount incarnation `link` is saddled this turn, as the ability
/// that asks last knew it (a Mount that left still counts).
fn saddled_this_turn(state: &GameState, link: ObjectLinkV4) -> bool {
    let turn = state.turn_key_v1();
    read_state(state).is_some_and(|c4a| {
        c4a.saddled
            .iter()
            .any(|(saddled, saddled_turn)| *saddled == link && *saddled_turn == turn)
    })
}

// ---- Static: Forensic Gadgeteer ---------------------------------------------------------

/// Forensic Gadgeteer: "Activated abilities of artifacts you control cost {1}
/// less to activate. This effect can't reduce the mana in that cost to less
/// than one mana." `cost` is the mana cost of an ability of `source`; each
/// Forensic Gadgeteer its controller controls takes {1} of generic mana off,
/// never leaving less than one mana in total. Colored and hybrid pips are
/// not reduced, and a cost with an X is left alone. A mana ability with a
/// mana cost (Energy Refractor's {2}) is reduced the same way.
pub(crate) fn reduce_artifact_activation(state: &GameState, source: ObjectId, cost: Cost) -> Cost {
    if !cfg!(feature = "eternal-fixtures") || cost.generic == 0 || cost.x_count != 0 {
        return cost;
    }
    let Some(live) = state.objects.try_get(source) else {
        return cost;
    };
    if live.zone != Zone::Battlefield
        || !crate::engine::object_has_type(state, source, CardType::Artifact)
    {
        return cost;
    }
    let controller = live.controller;
    let gadgeteers = [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
        .filter(|&object| {
            let permanent = state.objects.get(object);
            permanent.controller == controller
                && permanent.v4.face_index == 0
                && CARD_DEFS
                    .get(usize::from(permanent.card_def))
                    .is_some_and(|def| def.name == FORENSIC_GADGETEER && def.is_executable())
                && crate::continuous_characteristics_v1::printed_abilities_active(state, object)
        })
        .count();
    let mut reduced = cost;
    for _ in 0..gadgeteers {
        let total = usize::from(reduced.generic).saturating_add(reduced.pips.len());
        if reduced.generic == 0 || total <= 1 {
            break;
        }
        reduced.generic -= 1;
    }
    reduced
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

const fn eternal_trigger(
    condition: EternalTriggerV1,
    effect: fn() -> EffectOp,
) -> TriggeredAbilityDef {
    trigger(
        TriggerCondition::StandardV1(StandardTriggerV1::EternalV1(condition)),
        effect,
    )
}

const fn c4a_trigger(condition: C4aTriggerV1, effect: fn() -> EffectOp) -> TriggeredAbilityDef {
    eternal_trigger(EternalTriggerV1::C4aV1(condition), effect)
}

const KITTEN_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::CastNoncreatureSpell,
    kitten_flicker,
)];
const MASTERMIND_TRIGGERS: [TriggeredAbilityDef; 1] = [c4a_trigger(
    C4aTriggerV1::OpponentDrawsSecondCardEachTurnV1,
    mastermind_draw_trigger,
)];
const SHREDDER_TRIGGERS: [TriggeredAbilityDef; 1] = [c4a_trigger(
    C4aTriggerV1::PlayerCastsSecondSpellEachTurnV1,
    shredder_connive,
)];
const GADGETEER_TRIGGERS: [TriggeredAbilityDef; 1] = [eternal_trigger(
    EternalTriggerV1::E7aV1(
        crate::eternal_legacy_e7a_v1::E7aTriggerV1::ControllerCastsArtifactSpellV1,
    ),
    gadgeteer_investigate,
)];
const RABBIT_TRIGGERS: [TriggeredAbilityDef; 2] = [
    c4a_trigger(C4aTriggerV1::EntersWithXAtLeastFiveV1, rabbit_ravenous_draw),
    trigger(TriggerCondition::Attacks, rabbit_tokens),
];
const BILL_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::ControlledLandEnters,
    bill_counter,
)];
const BRONCO_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Attacks, bronco_reveal)];
const LUTRI_TRIGGERS: [TriggeredAbilityDef; 1] = [eternal_trigger(
    EternalTriggerV1::EntersAfterCastV1,
    lutri_copy,
)];

/// Triggered abilities of this module's cards, falling through to wave
/// C4b.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        DISPLACER_KITTEN => &KITTEN_TRIGGERS,
        FAERIE_MASTERMIND => &MASTERMIND_TRIGGERS,
        LEDGER_SHREDDER => &SHREDDER_TRIGGERS,
        FORENSIC_GADGETEER => &GADGETEER_TRIGGERS,
        JACKED_RABBIT => &RABBIT_TRIGGERS,
        BRISTLY_BILL => &BILL_TRIGGERS,
        CAUSTIC_BRONCO => &BRONCO_TRIGGERS,
        LUTRI => &LUTRI_TRIGGERS,
        _ => crate::eternal_cube_c4b_v1::triggers_for(name),
    }
}

/// Target specs of this module's printed and queued triggered abilities.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    match name {
        DISPLACER_KITTEN if *effect == kitten_flicker() => Some(TargetSpec::StandardV1(
            StandardTargetV1::C4aV1(C4aTargetV1::UpToOneControlledNonlandPermanentV1),
        )),
        LUTRI if *effect == lutri_copy() => Some(TargetSpec::StandardV1(StandardTargetV1::C4aV1(
            C4aTargetV1::ControlledInstantOrSorcerySpellV1,
        ))),
        BRISTLY_BILL if *effect == bill_counter() => Some(TargetSpec::Creature),
        FAERIE_MASTERMIND if *effect == mastermind_draw_trigger() => Some(TargetSpec::None),
        LEDGER_SHREDDER if *effect == shredder_connive() => Some(TargetSpec::None),
        FORENSIC_GADGETEER if *effect == gadgeteer_investigate() => Some(TargetSpec::None),
        JACKED_RABBIT if *effect == rabbit_ravenous_draw() || *effect == rabbit_tokens() => {
            Some(TargetSpec::None)
        }
        CAUSTIC_BRONCO if *effect == bronco_reveal() => Some(TargetSpec::None),
        _ => delayed_effect_matches(name, effect).then_some(TargetSpec::None),
    }
}

/// Queued triggers owned by this module's cards.
pub(crate) fn delayed_effect_matches(_name: &str, _effect: &EffectOp) -> bool {
    false
}

/// Whether `condition` triggers on its source's arrival on the battlefield.
pub(crate) fn arrival_condition(condition: C4aTriggerV1) -> bool {
    condition == C4aTriggerV1::EntersWithXAtLeastFiveV1
}

/// `player`'s draws this turn up to and including `events[index]` (Orcish
/// Bowmasters' ordinal): the turn's counter already holds every draw of the
/// collected batch.
fn draw_ordinal_at(
    events: &[CommittedEvent],
    index: usize,
    player: PlayerId,
    state: &GameState,
) -> u32 {
    let is_draw = |event: &CommittedEvent| {
        matches!(
            event,
            CommittedEvent::Draw { player: drawer, object: Some(_) } if *drawer == player
        )
    };
    let in_batch =
        u32::try_from(events.iter().filter(|event| is_draw(event)).count()).unwrap_or(u32::MAX);
    let through = u32::try_from(
        events[..=index]
            .iter()
            .filter(|event| is_draw(event))
            .count(),
    )
    .unwrap_or(u32::MAX);
    state.players[player.index()]
        .draws_this_turn
        .saturating_sub(in_batch)
        .saturating_add(through)
}

pub(crate) fn trigger_matches(
    condition: C4aTriggerV1,
    events: &[CommittedEvent],
    index: usize,
    source: ObjectId,
    state: &GameState,
) -> bool {
    let controller = state.objects.get(source).controller;
    match condition {
        C4aTriggerV1::OpponentDrawsSecondCardEachTurnV1 => {
            let CommittedEvent::Draw {
                player,
                object: Some(_),
            } = events[index]
            else {
                return false;
            };
            player != controller && draw_ordinal_at(events, index, player, state) == 2
        }
        C4aTriggerV1::PlayerCastsSecondSpellEachTurnV1 => {
            matches!(
                events[index],
                CommittedEvent::SpellCast { controller: caster, .. }
                    if state.players[caster.index()].spells_cast_this_turn == 2
            )
        }
        C4aTriggerV1::EntersWithXAtLeastFiveV1 => {
            matches!(
                events[index],
                CommittedEvent::ZoneChange {
                    object,
                    to: Zone::Battlefield,
                    ..
                } if object == source
            ) && read_state(state)
                .is_some_and(|c4a| c4a.ravenous_entries.contains(&live_link(state, source)))
        }
    }
}

// ---- Execution ----------------------------------------------------------------------

/// Runs one of this module's leaves in synchronous execution.
pub(crate) fn execute(op: &C4aOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        C4aOpV1::KittenFlickerV1 => {
            // "Up to one": with no target chosen, nothing happens.
            let spec = TargetSpec::StandardV1(StandardTargetV1::C4aV1(
                C4aTargetV1::UpToOneControlledNonlandPermanentV1,
            ));
            if let Some(object) = legal_target(ctx, state, spec, 0, Zone::Battlefield) {
                flicker(state, object);
            }
        }
        C4aOpV1::RabbitEntersV1 => {
            crate::effect::execute(
                &EffectOp::PutSourceOntoBattlefieldWithXPlusOneCounters,
                ctx,
                state,
            );
            if ctx.x_value >= 5
                && state.engine.halted.is_none()
                && state.objects.get(ctx.source).zone == Zone::Battlefield
            {
                let link = live_link(state, ctx.source);
                let mut entries = std::mem::take(&mut c4a_state(state).ravenous_entries);
                entries.retain(|&entry| link_live_in(state, entry, Zone::Battlefield));
                entries.push(link);
                c4a_state(state).ravenous_entries = entries;
            }
        }
        C4aOpV1::RabbitTokensV1 => {
            let Some(contract) = ctx.ability_source_contract else {
                return;
            };
            let binding = EffectObjectBinding {
                object: contract.source,
                expected_zone: Zone::Battlefield,
                expected_zone_change_count: contract.zone_change_count,
            };
            // The power as the Rabbit last had it (it may have left).
            let power =
                crate::creature_stats_lki_v1::current_or_last_creature_stats(state, binding)
                    .map_or(0, |(power, _)| power);
            if power > 0 {
                crate::effect::execute(
                    &EffectOp::CreateTokensDynamic {
                        token_def: token_id(RABBIT_TOKEN),
                        controller: PlayerRef::Controller,
                        count: DynamicValueDef::Fixed(power),
                        tapped: false,
                    },
                    ctx,
                    state,
                );
            }
        }
        C4aOpV1::BillDoubleV1 => {
            // The counts are read before any counters are put, so each
            // creature gets as many as it had.
            let creatures = [PlayerId::P0, PlayerId::P1]
                .into_iter()
                .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
                .filter(|&object| {
                    state.objects.get(object).controller == ctx.controller
                        && crate::engine::object_has_type(state, object, CardType::Creature)
                })
                .map(|object| (object, state.objects.get(object).counters.plus1_plus1))
                .filter(|&(_, counters)| counters > 0)
                .collect::<Vec<_>>();
            for (object, counters) in creatures {
                if state.objects.get(object).zone != Zone::Battlefield {
                    continue;
                }
                if event::add_plus_one_counters(state, object, ctx.controller, counters).is_err() {
                    halt(state, ctx.source);
                    return;
                }
            }
        }
        C4aOpV1::SaddleV1 => {
            let Some(mount) = live_ability_source(ctx, state) else {
                return;
            };
            let link = live_link(state, mount);
            let turn = state.turn_key_v1();
            let saddled = &mut c4a_state(state).saddled;
            saddled.retain(|(_, saddled_turn)| *saddled_turn == turn);
            if !saddled.iter().any(|(old, _)| *old == link) {
                saddled.push((link, turn));
            }
        }
        C4aOpV1::BroncoRevealV1 => {
            let Some(contract) = ctx.ability_source_contract else {
                return;
            };
            let saddled = saddled_this_turn(
                state,
                ObjectLinkV4 {
                    object: contract.source,
                    zone_change_count: contract.zone_change_count,
                },
            );
            let Some(mana_value) = reveal_top_card_to_hand(state, ctx.controller, ctx.source)
            else {
                return;
            };
            if mana_value > 0 {
                let loser = if saddled {
                    ctx.controller.opponent()
                } else {
                    ctx.controller
                };
                event::propose_and_commit(
                    state,
                    ProposedEvent::life_loss(loser, i32::from(mana_value)),
                );
            }
        }
        // Lutri's copy runs in `drive`.
        C4aOpV1::LutriCopyV1 | C4aOpV1::LutriCopyStepV1 { .. } => halt(state, ctx.source),
    }
}

/// Whether the copy slots `targets` (with their `contracts`) can still be
/// completed into a legal copy of `spell`.
fn copy_can_complete(
    state: &GameState,
    spell: &StackItem,
    controller: PlayerId,
    targets: &[Target],
    contracts: &[StackTargetContractV4],
) -> bool {
    crate::engine::copy_snapshot_prefix_can_complete(state, spell, controller, targets, contracts)
        .unwrap_or(false)
}

/// Runs one of this module's leaves in the resumable interpreter.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &C4aOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    match op {
        C4aOpV1::LutriCopyV1 => {
            // The target is checked as the trigger resolves: a spell that
            // is gone, or no longer an instant or sorcery spell its
            // controller controls, leaves nothing to copy.
            let spec = TargetSpec::StandardV1(StandardTargetV1::C4aV1(
                C4aTargetV1::ControlledInstantOrSorcerySpellV1,
            ));
            let Some(spell) = legal_target(&continuation.ctx, state, spec, 0, Zone::Stack) else {
                return Ok(false);
            };
            let Some(item) = state
                .stack
                .iter()
                .find(|item| item.kind == StackItemKind::Spell && item.source == spell)
                .cloned()
            else {
                return Ok(false);
            };
            continuation.frames.push(EffectFrame::Program {
                op: c4a(C4aOpV1::LutriCopyStepV1 {
                    spell: Box::new(item),
                    targets: Vec::new(),
                    contracts: Vec::new(),
                }),
                path,
            });
            Ok(false)
        }
        C4aOpV1::LutriCopyStepV1 {
            spell,
            targets,
            contracts,
        } => {
            let controller = continuation.ctx.controller;
            let slot = targets.len();
            if slot >= spell.targets.len() {
                // Every slot is decided: the copy goes on the stack. A copy
                // that cannot be made (a target gone for good) is not made.
                let _ = crate::engine::materialize_copy_snapshot(
                    state,
                    spell,
                    controller,
                    targets.clone(),
                    contracts.clone(),
                );
                return Ok(false);
            }
            let step = |target: Target, contract: StackTargetContractV4| {
                let mut next_targets = targets.clone();
                next_targets.push(target);
                let mut next_contracts = contracts.clone();
                next_contracts.push(contract);
                (next_targets, next_contracts)
            };
            let mut slots = Vec::new();
            // "You may choose new targets": keeping the target comes first.
            let (keep_targets, keep_contracts) =
                step(spell.targets[slot], spell.v4.target_contracts[slot]);
            if copy_can_complete(state, spell, controller, &keep_targets, &keep_contracts) {
                slots.push((keep_targets, keep_contracts));
            }
            let replacements = crate::engine::copy_snapshot_replacement_targets(
                state, spell, controller, targets,
            )?;
            for target in replacements {
                let contract = StackTargetContractV4::capture(state, target);
                if target == spell.targets[slot] && contract == spell.v4.target_contracts[slot] {
                    continue;
                }
                let (next_targets, next_contracts) = step(target, contract);
                if copy_can_complete(state, spell, controller, &next_targets, &next_contracts) {
                    slots.push((next_targets, next_contracts));
                }
            }
            let mut options = slots
                .into_iter()
                .map(|(next_targets, next_contracts)| {
                    c4a(C4aOpV1::LutriCopyStepV1 {
                        spell: spell.clone(),
                        targets: next_targets,
                        contracts: next_contracts,
                    })
                })
                .collect::<Vec<_>>();
            match options.len() {
                0 => {}
                1 => continuation.frames.push(EffectFrame::Program {
                    op: options.remove(0),
                    path,
                }),
                _ => continuation.frames.push(EffectFrame::Program {
                    op: EffectOp::Choice {
                        controller: PlayerRef::Controller,
                        options,
                    },
                    path,
                }),
            }
            Ok(false)
        }
        other => {
            execute(other, &continuation.ctx, state);
            Ok(false)
        }
    }
}
