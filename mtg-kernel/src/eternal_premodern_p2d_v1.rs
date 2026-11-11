//! Card programs for the eternal catalog's Premodern wave P2d
//! (`docs/design/eternal_formats_support_v1.md`): White Weenie and Mono-Red
//! Control.
//!
//! `eternal_cards_v1` forwards its effect leaves and state here, the wave
//! E9a tables fall through to this module's, and the engine calls the hooks
//! in `statics` only in `eternal-fixtures` builds. Every table is keyed by a
//! printed name that occurs only in the eternal registry. Behavior follows
//! each card's current Oracle text, checked against its XMage source.
//!
//! Primitives added here:
//! - printed protection from creatures (Beloved Chaplain) and from a color
//!   (Soltari Monk and Priest), shadow (702.28), and protection from instant
//!   and sorcery spells until end of turn (Devoted Caretaker);
//! - Mother of Runes' protection from a color chosen on resolution (wave E9a's
//!   protection record);
//! - a creature type chosen as a permanent enters, and its anthem (Shared
//!   Triumph); a spell tax on every noncreature spell (Glowrider);
//! - echo (702.30): the upkeep trigger exists only for a creature that came
//!   under its controller's control since their last upkeep, and pays the
//!   echo cost on resolution or sacrifices the creature;
//! - Veteran Brawlers' attack and block restrictions on untapped lands;
//! - "destroy all enchantments", "X damage to each creature without flying
//!   and each player", "destroy target artifact or land, it can't be
//!   regenerated".
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

pub mod statics;
pub(crate) mod targets;

use crate::card_def::{CardType, Keywords, Subtype, TargetSpec, CARD_DEFS};
use crate::effect::{EffectOp, ExecCtx, PlayerRef};
use crate::eternal_cards_v1::EternalOpV1;
use crate::eternal_legacy_e9a_v1::{battlefield, definition_name, live_link, TurnV1};
use crate::eternal_premodern_p1a_v1::{
    destroy, destroy_batch, forbid_regeneration, halt, has_type, legal_object_target,
    live_ability_source, queue_trigger,
};
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::standard_cards_v1::{StandardOpV1, StandardTargetV1};
use crate::state::{AbilitySourceContractV4, GameState, ObjectLinkV4, Target, Zone};
use crate::trigger::{PendingTrigger, TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

pub(crate) const AVALANCHE_RIDERS: &str = "Avalanche Riders";
pub(crate) const BELOVED_CHAPLAIN: &str = "Beloved Chaplain";
pub(crate) const GLOWRIDER: &str = "Glowrider";
pub(crate) const SHARED_TRIUMPH: &str = "Shared Triumph";
pub(crate) const SOLTARI_MONK: &str = "Soltari Monk";
pub(crate) const SOLTARI_PRIEST: &str = "Soltari Priest";
pub(crate) const VETERAN_BRAWLERS: &str = "Veteran Brawlers";

/// The echo cost of a creature with echo: its colored mana and generic
/// mana. Avalanche Riders {3}{R}; Karmic Guide {3}{W}{W} (Premodern wave
/// P2c).
pub(crate) fn echo_cost(name: &str) -> Option<(&'static [ManaColor], u8)> {
    match name {
        AVALANCHE_RIDERS => Some((&[ManaColor::R], 3)),
        crate::eternal_premodern_p2c_v1::KARMIC_GUIDE => Some((&[ManaColor::W, ManaColor::W], 3)),
        _ => None,
    }
}

// ---- State ------------------------------------------------------------------------

/// Per-game state for this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Default (and absent from snapshots)
/// until one of these cards needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct P2dStateV1 {
    /// Avalanche Riders that came under a player's control since that
    /// player's last upkeep.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub echo_due: Vec<(ObjectLinkV4, PlayerId)>,
    /// Permanents with protection from instant and sorcery spells until end
    /// of turn (Devoted Caretaker).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spell_protection: Vec<(ObjectLinkV4, TurnV1)>,
    /// The creature type each Shared Triumph chose as it entered.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub triumph_types: Vec<(ObjectLinkV4, Subtype)>,
}

impl P2dStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

pub(crate) fn p2d_state(state: &mut GameState) -> &mut P2dStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).p2d
}

pub(crate) fn read_state(state: &GameState) -> Option<&P2dStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.p2d)
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::P2dV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum P2dOpV1 {
    /// Nova Cleric: destroy all enchantments.
    NovaClericV1,
    /// Earthquake: X damage to each creature without flying and each player.
    EarthquakeV1,
    /// Pillage: destroy target 0 (an artifact or land), which can't be
    /// regenerated.
    PillageV1,
    /// Devoted Caretaker: target permanent 0 gains protection from instant
    /// and sorcery spells until end of turn.
    CaretakerV1,
    /// Avalanche Riders: destroy target land 0.
    RidersDestroyV1,
    /// Echo: pay the source's echo cost, or sacrifice the source.
    EchoPayV1,
    /// Echo: sacrifice the source (the unpaid option).
    EchoSacrificeV1,
    /// The borrowed Karmic Guide trigger retains the cost after text changes.
    DonatedGuideEchoPayV1,
}

impl P2dOpV1 {
    /// Whether this leaf can yield a player decision by itself. The echo
    /// choice is an `EffectOp::Choice`, not a leaf.
    pub(crate) fn contains_player_choice(&self) -> bool {
        false
    }
}

fn p2d(op: P2dOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::P2dV1(op)))
}

// ---- Programs -----------------------------------------------------------------------

/// Nova Cleric: "Destroy all enchantments."
pub fn nova_cleric() -> EffectOp {
    p2d(P2dOpV1::NovaClericV1)
}

/// Earthquake: "deals X damage to each creature without flying and each
/// player."
pub fn earthquake() -> EffectOp {
    p2d(P2dOpV1::EarthquakeV1)
}

/// Pillage: "Destroy target artifact or land. It can't be regenerated."
pub fn pillage() -> EffectOp {
    p2d(P2dOpV1::PillageV1)
}

/// Devoted Caretaker: "Target permanent you control gains protection from
/// instant spells and from sorcery spells until end of turn."
pub fn devoted_caretaker() -> EffectOp {
    p2d(P2dOpV1::CaretakerV1)
}

/// Mother of Runes: "Target creature you control gains protection from the
/// color of your choice until end of turn." Wave E9a's protection record
/// (Sejiri Steppe) with one option per color in WUBRG order.
pub fn mother_of_runes() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: crate::eternal_legacy_e9a_v1::statics::COLORS
            .into_iter()
            .map(|color| {
                EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E9aV1(
                    crate::eternal_legacy_e9a_v1::E9aOpV1::ProtectionFromV1 { color },
                )))
            })
            .collect(),
    }
}

/// Avalanche Riders' enters trigger: "destroy target land."
fn riders_enters() -> EffectOp {
    p2d(P2dOpV1::RidersDestroyV1)
}

/// Echo {3}{R}: "sacrifice it unless you pay its echo cost." Paying is the
/// first option; an unpayable cost sacrifices the creature.
fn echo_trigger() -> EffectOp {
    echo_choice(P2dOpV1::EchoPayV1)
}

pub(crate) fn donated_guide_echo_trigger() -> EffectOp {
    echo_choice(P2dOpV1::DonatedGuideEchoPayV1)
}

fn echo_choice(payment: P2dOpV1) -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: vec![p2d(payment), p2d(P2dOpV1::EchoSacrificeV1)],
    }
}

// ---- Triggered abilities ----------------------------------------------------------------

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

const RIDERS_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, riders_enters)];

/// Triggered abilities of this module's cards that use the engine's own
/// trigger conditions. Echo is queued by the upkeep hook.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        AVALANCHE_RIDERS => &RIDERS_TRIGGERS,
        _ => crate::eternal_premodern_p1a_v1::triggers_for(name),
    }
}

/// Target specs of this module's triggered abilities.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    if name == AVALANCHE_RIDERS && *effect == riders_enters() {
        return Some(TargetSpec::Land);
    }
    delayed_effect_matches(name, effect).then_some(TargetSpec::None)
}

/// Queued triggers owned by this module's cards.
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    echo_cost(name).is_some() && *effect == echo_trigger()
}

// ---- Effect leaves ------------------------------------------------------------------

pub(crate) fn execute(op: &P2dOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        P2dOpV1::NovaClericV1 => {
            let enchantments = battlefield(state)
                .filter(|&object| has_type(state, object, CardType::Enchantment))
                .collect::<Vec<_>>();
            destroy_batch(state, enchantments);
        }
        P2dOpV1::EarthquakeV1 => {
            let amount = i32::from(ctx.x_value);
            if amount <= 0 {
                return;
            }
            let mut damage = battlefield(state)
                .filter(|&object| {
                    has_type(state, object, CardType::Creature)
                        && !crate::engine::has_effective_keyword(state, object, Keywords::FLYING)
                })
                .map(|object| event::damage_from_exec_ctx_v1(ctx, Target::Object(object), amount))
                .collect::<Vec<_>>();
            for player in [ctx.controller, ctx.controller.opponent()] {
                damage.push(event::damage_from_exec_ctx_v1(
                    ctx,
                    Target::Player(player),
                    amount,
                ));
            }
            event::propose_and_commit_batch(state, damage);
        }
        P2dOpV1::PillageV1 => {
            let spec = TargetSpec::StandardV1(StandardTargetV1::ArtifactOrLandV1);
            let Some(object) = legal_object_target(ctx, state, 0, spec) else {
                return;
            };
            // "It can't be regenerated" matters to an artifact creature.
            let record = (live_link(state, object), TurnV1::now(state));
            let added =
                !crate::eternal_premodern_p1a_v1::statics::cant_be_regenerated(state, object);
            if added {
                forbid_regeneration(state, object);
            }
            destroy(ctx, state, object);
            if added {
                crate::eternal_premodern_p1a_v1::p1a_state(state)
                    .no_regeneration
                    .retain(|entry| *entry != record);
            }
        }
        P2dOpV1::CaretakerV1 => {
            let spec = TargetSpec::StandardV1(StandardTargetV1::ControlledPermanentV1);
            let Some(object) = legal_object_target(ctx, state, 0, spec) else {
                return;
            };
            let record = (live_link(state, object), TurnV1::now(state));
            let records = &mut p2d_state(state).spell_protection;
            if !records.contains(&record) {
                records.push(record);
            }
        }
        P2dOpV1::RidersDestroyV1 => {
            if let Some(land) = legal_object_target(ctx, state, 0, TargetSpec::Land) {
                destroy(ctx, state, land);
            }
        }
        P2dOpV1::EchoPayV1 => {
            // The ability retains its printed payment after its source leaves.
            let cost = ctx
                .ability_source_contract
                .filter(|source| {
                    source.source == ctx.source
                        && source.controller == ctx.controller
                        && source.zone == Zone::Battlefield
                        && crate::engine::validate_historical_ability_source_contract(
                            state, *source,
                        )
                        .is_ok()
                })
                .and_then(|source| CARD_DEFS.get(usize::from(source.card_def)))
                .and_then(|definition| echo_cost(definition.name));
            pay_or_sacrifice(ctx, state, cost);
        }
        P2dOpV1::EchoSacrificeV1 => echo_sacrifice(ctx, state),
        P2dOpV1::DonatedGuideEchoPayV1 => {
            pay_or_sacrifice(
                ctx,
                state,
                echo_cost(crate::eternal_premodern_p2c_v1::KARMIC_GUIDE),
            );
        }
    }
}

fn pay_or_sacrifice(ctx: &ExecCtx, state: &mut GameState, cost: Option<(&[ManaColor], u8)>) {
    let Some((colors, generic)) = cost else {
        halt(state, ctx.source);
        return;
    };
    if !crate::engine::pay_effect_mana(ctx.controller, colors, generic, state) {
        echo_sacrifice(ctx, state);
    }
}

fn echo_sacrifice(ctx: &ExecCtx, state: &mut GameState) {
    // The trigger's controller can only sacrifice the original permanent they still control.
    let Some(source) = live_ability_source(ctx, state)
        .filter(|&source| state.objects.get(source).controller == ctx.controller)
    else {
        return;
    };
    event::log_sacrifice(state, source);
    event::propose_and_commit(state, ProposedEvent::zone_change(source, Zone::Graveyard));
}

// ---- Hooks ----------------------------------------------------------------------------

/// Called as `object` enters the battlefield: an Avalanche Riders owes echo
/// at its controller's next upkeep.
pub(crate) fn apply_entry(state: &mut GameState, object: ObjectId) {
    let live = state.objects.get(object);
    if live.zone != Zone::Battlefield
        || live.v4.face_index != 0
        || definition_name(state, object).is_none_or(|name| echo_cost(name).is_none())
    {
        return;
    }
    let (link, controller) = (live_link(state, object), live.controller);
    let due = &mut p2d_state(state).echo_due;
    due.retain(|(entry, _)| entry.object != object);
    due.push((link, controller));
}

/// Called as `object` came under `controller`'s control: a creature with echo
/// owes it at that player's next upkeep (702.30a).
pub(crate) fn control_changed(state: &mut GameState, object: ObjectId, controller: PlayerId) {
    let live = state.objects.get(object);
    if live.zone != Zone::Battlefield
        || live.v4.face_index != 0
        || definition_name(state, object).is_none_or(|name| echo_cost(name).is_none())
    {
        return;
    }
    let link = live_link(state, object);
    let due = &mut p2d_state(state).echo_due;
    due.retain(|(entry, _)| entry.object != object);
    due.push((link, controller));
}

/// The triggers this module's hooks set off in this collection.
pub(crate) fn delayed_triggers(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Vec<PendingTrigger> {
    if !cfg!(feature = "eternal-fixtures") {
        return Vec::new();
    }
    for event in events {
        let CommittedEvent::UpkeepBegan { player } = *event else {
            continue;
        };
        let due = read_state(state).map_or_else(Vec::new, |p2d| p2d.echo_due.clone());
        for (link, owner) in due {
            let live = state.objects.try_get(link.object);
            let current = live.is_some_and(|live| {
                live.zone == Zone::Battlefield && live.zone_change_count == link.zone_change_count
            });
            if !current {
                p2d_state(state)
                    .echo_due
                    .retain(|(entry, _)| *entry != link);
                continue;
            }
            // "Came under your control since the beginning of your last
            // upkeep": only the upkeep of the controller that owes it.
            if state.objects.get(link.object).controller != player || owner != player {
                continue;
            }
            p2d_state(state)
                .echo_due
                .retain(|(entry, _)| *entry != link);
            let mut source = AbilitySourceContractV4::capture(state, link.object);
            source.controller = player;
            queue_trigger(state, source, player, echo_trigger(), None);
        }
    }
    prune(state);
    Vec::new()
}

/// The beginning of a turn ends the last turn's records.
pub(crate) fn begin_turn(state: &mut GameState, _player: PlayerId) {
    prune(state);
}

/// Drops records of an earlier turn and of permanents that left.
fn prune(state: &mut GameState) {
    let Some(p2d) = read_state(state) else {
        return;
    };
    if p2d.is_empty() {
        return;
    }
    let now = TurnV1::now(state);
    let live_on_battlefield = |link: &ObjectLinkV4| {
        state.objects.try_get(link.object).is_some_and(|live| {
            live.zone == Zone::Battlefield && live.zone_change_count == link.zone_change_count
        })
    };
    let spell_protection = p2d
        .spell_protection
        .iter()
        .copied()
        .filter(|(link, turn)| *turn == now && live_on_battlefield(link))
        .collect::<Vec<_>>();
    let triumph_types = p2d
        .triumph_types
        .iter()
        .copied()
        .filter(|(link, _)| live_on_battlefield(link))
        .collect::<Vec<_>>();
    let echo_due = p2d
        .echo_due
        .iter()
        .copied()
        .filter(|(link, _)| live_on_battlefield(link))
        .collect::<Vec<_>>();
    if let Some(p2d) = state.eternal_v1.as_mut().map(|eternal| &mut eternal.p2d) {
        p2d.spell_protection = spell_protection;
        p2d.triumph_types = triumph_types;
        p2d.echo_due = echo_due;
    }
}
