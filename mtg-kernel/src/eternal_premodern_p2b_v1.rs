//! Card programs for the eternal catalog's Premodern wave P2b
//! (`docs/design/eternal_formats_support_v1.md`): Survival, Madness
//! Threshold, the cards Elves shares with them, and the Elves follow-up
//! for Birchlore Rangers and Thornscape Battlemage.
//!
//! `eternal_cards_v1` forwards its effect leaves and state here, wave P1a's
//! tables fall through to this module's, and the engine calls the hooks in
//! `statics` only in `eternal-fixtures` builds. Every table is keyed by a
//! printed name that occurs only in the eternal registry, so the Pauper, FDN
//! and Standard catalogs never reach it. Behavior follows each card's current
//! Oracle text, checked against its XMage source.
//!
//! Primitives added here:
//! - echo (702.30): owed from the turn a permanent enters until its
//!   controller's next upkeep, where it is paid or the permanent sacrificed;
//! - fading 3 (Blastoderm; the counters are this module's own, as in P1b);
//! - Gaea's Cradle's board-dependent mana, Yavimaya Hollow's targeted
//!   regeneration, Glory's activation from the graveyard and its protection
//!   for every creature its controller controls;
//! - "Discard a creature card" as Survival of the Fittest's cost (the cost is
//!   an ordinary discard, restricted to creature cards where the discard is
//!   chosen), and "Return an Elf you control" as Wirewood Symbiote's cost;
//! - a count of the nontoken creatures put into each graveyard from the
//!   battlefield this turn (Caller of the Claw);
//! - more morph cards (Nantuko Vigilante and Birchlore Rangers) and the
//!   trigger for turning Vigilante face up;
//! - Birchlore's two-Elf mana payment and Battlemage's independent kickers;
//! - Deranged Hermit's Squirrel lord, Vintara Snapper's conditional shroud
//!   and Masticore's upkeep cost.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

pub mod statics;

use crate::card_def::{card_id_by_name, CardType, TargetSpec};
use crate::effect::{
    EffectContinuation, EffectFrame, EffectOp, ExecCtx, LibraryCardFilter, PlayerRef,
};
use crate::eternal_cards_v1::EternalOpV1;
use crate::eternal_legacy_e9a_v1::{
    active_named, definition_name, left_as_creature, link_live_in, live_link, TurnV1,
};
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::standard_cards_v1::StandardOpV1;
use crate::state::{AbilitySourceContractV4, GameState, ObjectLinkV4, Target, Zone};
use crate::trigger::{PendingTrigger, TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

pub(crate) const THORNSCAPE_BATTLEMAGE: &str = "Thornscape Battlemage";
pub(crate) const THORNSCAPE_WHITE_KICKER: crate::mana::Cost = crate::mana::Cost {
    pips: &[crate::mana::Pip::Colored(ManaColor::W)],
    generic: 0,
    x_count: 0,
};

pub(crate) fn is_thornscape(def: &crate::card_def::CardDef) -> bool {
    cfg!(feature = "eternal-fixtures") && def.name == THORNSCAPE_BATTLEMAGE
}

/// The two printed choices travel with a resolved or copied spell.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BattlemageKickersV1(pub u8);

impl BattlemageKickersV1 {
    pub fn is_empty(&self) -> bool {
        self.0 == 0
    }
}

impl std::hash::Hash for BattlemageKickersV1 {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        if !self.is_empty() {
            std::hash::Hash::hash(&("battlemage-kickers/v1", self.0), state);
        }
    }
}

/// One resolution's kicker choices, available through its entry checkpoint.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BattlemageEntryV1(pub Option<(ObjectId, BattlemageKickersV1)>);

impl BattlemageEntryV1 {
    pub fn is_empty(&self) -> bool {
        self.0.is_none()
    }
}

impl std::hash::Hash for BattlemageEntryV1 {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        if let Some((object, kickers)) = self.0 {
            std::hash::Hash::hash(&("battlemage-entry/v1", object, kickers), state);
        }
    }
}

// ---- State ------------------------------------------------------------------------

/// Per-game state for this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Default (and absent from snapshots)
/// until one of these cards needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct P2bStateV1 {
    /// Triggers detected by hooks, waiting for the next trigger collection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queued_triggers: Vec<QueuedTriggerV1>,
    /// Echo permanents (battlefield incarnations) whose echo cost is due at
    /// their controller's next upkeep.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub echo_owed: Vec<ObjectLinkV4>,
    /// Fade counters on each Blastoderm incarnation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fade_counters: Vec<(ObjectLinkV4, u8)>,
    /// The nontoken creatures put into each player's graveyard from the
    /// battlefield during one turn, per owner.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graveyard_arrivals: Option<GraveyardArrivalsV1>,
}

/// The nontoken creatures put into graveyards from the battlefield in `turn`,
/// by their owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraveyardArrivalsV1 {
    pub turn: TurnV1,
    pub counts: [u16; 2],
}

impl P2bStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// A triggered ability queued by a hook.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueuedTriggerV1 {
    pub source: AbilitySourceContractV4,
    pub controller: PlayerId,
    pub effect: EffectOp,
    /// The target the ability's controller announces, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_spec: Option<TargetSpec>,
}

pub(crate) fn p2b_state(state: &mut GameState) -> &mut P2bStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).p2b
}

pub fn read_state(state: &GameState) -> Option<&P2bStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.p2b)
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::P2bV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum P2bOpV1 {
    /// Survival of the Fittest: search for a creature card. A choice.
    SurvivalSearchV1,
    /// Yavimaya Hollow: regenerate target creature 0.
    RegenerateTargetV1,
    /// Glory: creatures its controller controls gain protection from `color`
    /// until end of turn.
    GloryProtectV1 { color: ManaColor },
    /// Deranged Hermit: four 1/1 Squirrels.
    HermitSquirrelsV1,
    /// Caller of the Claw: a 2/2 Bear for each nontoken creature put into
    /// its controller's graveyard from the battlefield this turn.
    CallerBearsV1,
    /// Echo: pay the echo cost or sacrifice the source. May be a choice.
    EchoUpkeepV1,
    /// Echo: pay the source's echo cost.
    EchoPayV1,
    /// Sacrifice the resolving ability's source.
    SacrificeSourceV1,
    /// Masticore: sacrifice it unless its controller discards a card. May be
    /// a choice.
    MasticoreUpkeepV1,
    /// Fading: remove a fade counter, or sacrifice the source.
    FadeUpkeepV1,
    /// Nantuko Vigilante: destroy target artifact or enchantment 0.
    VigilanteDestroyV1,
}

impl P2bOpV1 {
    /// Whether this leaf can yield a player decision or pushes frames, so its
    /// program must use the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(
            self,
            Self::SurvivalSearchV1 | Self::EchoUpkeepV1 | Self::MasticoreUpkeepV1
        )
    }
}

// ---- Names and helpers --------------------------------------------------------------

pub(crate) const BLASTODERM: &str = "Blastoderm";
pub(crate) const CALLER_OF_THE_CLAW: &str = "Caller of the Claw";
pub(crate) const DERANGED_HERMIT: &str = "Deranged Hermit";
pub(crate) const MASTICORE: &str = "Masticore";
pub(crate) const MULTANIS_ACOLYTE: &str = "Multani's Acolyte";
pub(crate) const NANTUKO_VIGILANTE: &str = "Nantuko Vigilante";
pub(crate) const SURVIVAL_OF_THE_FITTEST: &str = "Survival of the Fittest";
pub(crate) const VINTARA_SNAPPER: &str = "Vintara Snapper";
pub(crate) const YAVIMAYA_GRANGER: &str = "Yavimaya Granger";
const BEAR_TOKEN: &str = "Bear Token";
const SQUIRREL_TOKEN: &str = "Squirrel Token";

/// Blastoderm: "Fading 3".
pub(crate) const FADE_COUNTERS: u8 = 3;

fn p2b(op: P2bOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::P2bV1(op)))
}

fn halt(state: &mut GameState, source: ObjectId) {
    state.engine.halted = Some((
        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
        source,
    ));
}

fn token_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

/// The resolving ability's source, if it is still the incarnation that
/// created the ability, on the battlefield.
fn live_ability_source(ctx: &ExecCtx, state: &GameState) -> Option<ObjectId> {
    let contract = ctx.ability_source_contract?;
    let live = state.objects.try_get(contract.source)?;
    (live.zone == Zone::Battlefield && live.zone_change_count == contract.zone_change_count)
        .then_some(contract.source)
}

/// Target `index`, if it is an object still legal for the resolving spell
/// or ability under `spec`.
fn legal_object_target(
    ctx: &ExecCtx,
    state: &GameState,
    index: usize,
    spec: TargetSpec,
) -> Option<ObjectId> {
    let Some(Target::Object(object)) = ctx.targets.get(index).copied() else {
        return None;
    };
    (ctx.target_incarnation_matches(index, state)
        && crate::engine::effect_target_is_legal(
            state,
            ctx.source,
            ctx.controller,
            spec,
            &ctx.targets,
            index,
        ))
    .then_some(object)
}

fn queue_trigger(
    state: &mut GameState,
    source: AbilitySourceContractV4,
    controller: PlayerId,
    effect: EffectOp,
    target_spec: Option<TargetSpec>,
) {
    p2b_state(state).queued_triggers.push(QueuedTriggerV1 {
        source,
        controller,
        effect,
        target_spec,
    });
}

/// `player` sacrifices `object` if they control it on the battlefield.
fn sacrifice(state: &mut GameState, object: ObjectId, player: PlayerId) -> bool {
    let live = state.objects.get(object);
    if live.zone != Zone::Battlefield || live.controller != player {
        return false;
    }
    event::log_sacrifice(state, object);
    event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Graveyard));
    state.objects.get(object).zone != Zone::Battlefield
}

/// Creates `count` tokens named `name` for the ability's controller.
fn create_tokens(ctx: &ExecCtx, state: &mut GameState, name: &str, count: u16) {
    let token = token_id(name);
    for _ in 0..count {
        crate::effect::execute(
            &EffectOp::CreateToken {
                token_def: token,
                controller: PlayerRef::Controller,
            },
            ctx,
            state,
        );
    }
}

fn push_frame(continuation: &mut EffectContinuation, op: EffectOp, path: &[u16]) {
    continuation.frames.push(EffectFrame::Program {
        op,
        path: path.to_vec(),
    });
}

// ---- Spells and abilities -------------------------------------------------------------

/// Chatter of the Squirrel: "Create a 1/1 green Squirrel creature token."
/// Its flashback is the card's own `CardDef::flashback`.
pub fn chatter_of_the_squirrel() -> EffectOp {
    EffectOp::CreateToken {
        token_def: token_id(SQUIRREL_TOKEN),
        controller: PlayerRef::Controller,
    }
}

/// Survival of the Fittest: "Search your library for a creature card, reveal
/// that card, put it into your hand. Then shuffle." The discard is the
/// ability's cost.
pub fn survival_search() -> EffectOp {
    p2b(P2bOpV1::SurvivalSearchV1)
}

/// Yavimaya Hollow: "Regenerate target creature."
pub fn regenerate_target() -> EffectOp {
    p2b(P2bOpV1::RegenerateTargetV1)
}

/// Glory: "Choose a color. Creatures you control gain protection from the
/// chosen color until end of turn." One option per color, in WUBRG order.
pub fn glory_protection() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: crate::eternal_legacy_e9a_v1::statics::COLORS
            .into_iter()
            .map(|color| p2b(P2bOpV1::GloryProtectV1 { color }))
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

/// Caller of the Claw: "put a 2/2 green Bear creature token onto the
/// battlefield for each nontoken creature put into your graveyard from the
/// battlefield this turn."
fn caller_enters() -> EffectOp {
    p2b(P2bOpV1::CallerBearsV1)
}

/// Deranged Hermit: "put four 1/1 green Squirrel creature tokens onto the
/// battlefield."
fn hermit_enters() -> EffectOp {
    p2b(P2bOpV1::HermitSquirrelsV1)
}

/// Multani's Acolyte: "draw a card."
fn acolyte_enters() -> EffectOp {
    EffectOp::DrawCards {
        player: PlayerRef::Controller,
        count: 1,
    }
}

/// Yavimaya Granger: "you may search your library for a basic land card, put
/// that card onto the battlefield tapped, then shuffle." The search must be
/// the trigger's own root effect for its choice to authenticate.
fn granger_enters() -> EffectOp {
    EffectOp::SearchLibraryToBattlefieldTapped {
        player: PlayerRef::Controller,
        filter: LibraryCardFilter::BasicLand,
    }
}

/// Fading: "remove a fade counter from it. If you can't, sacrifice it."
fn fading_upkeep() -> EffectOp {
    p2b(P2bOpV1::FadeUpkeepV1)
}

/// Masticore: "sacrifice Masticore unless you discard a card."
fn masticore_upkeep() -> EffectOp {
    p2b(P2bOpV1::MasticoreUpkeepV1)
}

const CALLER_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, caller_enters)];
const HERMIT_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, hermit_enters)];
const ACOLYTE_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, acolyte_enters)];
const GRANGER_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, granger_enters)];
const BLASTODERM_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(YOUR_UPKEEP, fading_upkeep)];
const MASTICORE_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(YOUR_UPKEEP, masticore_upkeep)];

fn thornscape_damage() -> EffectOp {
    EffectOp::DealDamage {
        target: crate::effect::TargetRef::Target(0),
        amount: 2,
    }
}

fn thornscape_destroy() -> EffectOp {
    EffectOp::DestroyObject {
        object: crate::effect::ObjectRef::Target(0),
    }
}

const THORNSCAPE_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, thornscape_damage),
    trigger(TriggerCondition::Etb, thornscape_destroy),
];

pub(crate) fn thornscape_etb_is_enabled(
    entry: BattlemageEntryV1,
    object: ObjectId,
    effect: &EffectOp,
) -> bool {
    let Some((source, kickers)) = entry.0 else {
        return false;
    };
    source == object
        && ((kickers.0 & 1 != 0 && *effect == thornscape_damage())
            || (kickers.0 & 2 != 0 && *effect == thornscape_destroy()))
}

/// Triggered abilities of this module's cards that use the engine's own
/// trigger conditions. The rest are queued by the hooks below.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        BLASTODERM => &BLASTODERM_TRIGGERS,
        CALLER_OF_THE_CLAW => &CALLER_TRIGGERS,
        DERANGED_HERMIT => &HERMIT_TRIGGERS,
        MASTICORE => &MASTICORE_TRIGGERS,
        MULTANIS_ACOLYTE => &ACOLYTE_TRIGGERS,
        THORNSCAPE_BATTLEMAGE => &THORNSCAPE_TRIGGERS,
        YAVIMAYA_GRANGER => &GRANGER_TRIGGERS,
        // Premodern sideboard wave P3b's printed triggers.
        _ => crate::eternal_premodern_p3b_v1::triggers_for(name),
    }
}

/// Target specs of this module's triggered abilities, per ability.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    if !triggers_for(name)
        .iter()
        .any(|trigger| (trigger.effect)() == *effect)
        && !delayed_effect_matches(name, effect)
    {
        return None;
    }
    Some(match name {
        NANTUKO_VIGILANTE => TargetSpec::ArtifactOrEnchantmentPermanent,
        THORNSCAPE_BATTLEMAGE if *effect == thornscape_damage() => TargetSpec::AnyTarget,
        THORNSCAPE_BATTLEMAGE => TargetSpec::ArtifactPermanent,
        _ => TargetSpec::None,
    })
}

/// Queued triggers owned by this module's cards.
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    let EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::P2bV1(op))) = effect else {
        return false;
    };
    match name {
        NANTUKO_VIGILANTE => matches!(op, P2bOpV1::VigilanteDestroyV1),
        DERANGED_HERMIT | MULTANIS_ACOLYTE | YAVIMAYA_GRANGER => {
            matches!(op, P2bOpV1::EchoUpkeepV1)
        }
        _ => false,
    }
}

// ---- Effect leaves ------------------------------------------------------------------

pub(crate) fn execute(op: &P2bOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        P2bOpV1::RegenerateTargetV1 => {
            if let Some(creature) = legal_object_target(ctx, state, 0, TargetSpec::Creature) {
                crate::eternal_vintage_e7b_v1::statics::add_regeneration_shield(state, creature);
            }
        }
        P2bOpV1::GloryProtectV1 { color } => {
            let creatures = crate::eternal_legacy_e9a_v1::battlefield(state)
                .filter(|&object| {
                    state.objects.get(object).controller == ctx.controller
                        && crate::engine::object_has_type(state, object, CardType::Creature)
                })
                .collect::<Vec<_>>();
            let now = TurnV1::now(state);
            for creature in creatures {
                let record = (live_link(state, creature), *color, now);
                let protections = &mut crate::eternal_legacy_e9a_v1::e9a_state(state).protections;
                if !protections.contains(&record) {
                    protections.push(record);
                }
            }
        }
        P2bOpV1::HermitSquirrelsV1 => create_tokens(ctx, state, SQUIRREL_TOKEN, 4),
        P2bOpV1::CallerBearsV1 => {
            let now = TurnV1::now(state);
            let count = read_state(state)
                .and_then(|p2b| p2b.graveyard_arrivals)
                .filter(|arrivals| arrivals.turn == now)
                .map_or(0, |arrivals| arrivals.counts[ctx.controller.index()]);
            create_tokens(ctx, state, BEAR_TOKEN, count);
        }
        P2bOpV1::EchoPayV1 => {
            let Some(source) = live_ability_source(ctx, state) else {
                return;
            };
            let Some((colors, generic)) =
                definition_name(state, source).and_then(statics::echo_cost)
            else {
                return;
            };
            crate::engine::pay_effect_mana(ctx.controller, colors, generic, state);
        }
        P2bOpV1::SacrificeSourceV1 => {
            if let Some(source) = live_ability_source(ctx, state) {
                sacrifice(state, source, ctx.controller);
            }
        }
        P2bOpV1::FadeUpkeepV1 => {
            let Some(source) = live_ability_source(ctx, state) else {
                return;
            };
            let link = live_link(state, source);
            let removed = p2b_state(state)
                .fade_counters
                .iter_mut()
                .find(|(host, count)| *host == link && *count > 0)
                .map(|entry| entry.1 -= 1)
                .is_some();
            if !removed {
                sacrifice(state, source, ctx.controller);
            }
        }
        P2bOpV1::VigilanteDestroyV1 => {
            if let Some(object) =
                legal_object_target(ctx, state, 0, TargetSpec::ArtifactOrEnchantmentPermanent)
            {
                let mut ctx = ctx.clone();
                ctx.targets = vec![Target::Object(object)];
                ctx.target_contracts = Vec::new();
                crate::effect::execute(
                    &EffectOp::DestroyObject {
                        object: crate::effect::ObjectRef::Target(0),
                    },
                    &ctx,
                    state,
                );
            }
        }
        // Choices and frame-pushing leaves run in `drive`.
        P2bOpV1::SurvivalSearchV1 | P2bOpV1::EchoUpkeepV1 | P2bOpV1::MasticoreUpkeepV1 => {
            halt(state, ctx.source)
        }
    }
}

/// Runs a leaf inside the resumable interpreter. This module's leaves stage
/// no choice of their own: each pushes frames of generic leaves.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &P2bOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    let controller = continuation.ctx.controller;
    match op {
        P2bOpV1::SurvivalSearchV1 => {
            push_frame(
                continuation,
                EffectOp::SearchLibraryToHand {
                    player: PlayerRef::Controller,
                    filter: LibraryCardFilter::CardWithTypeV1(CardType::Creature),
                },
                &path,
            );
            Ok(false)
        }
        P2bOpV1::EchoUpkeepV1 => {
            let ctx = continuation.ctx.clone();
            let Some(source) = live_ability_source(&ctx, state) else {
                return Ok(false);
            };
            let Some((colors, generic)) =
                definition_name(state, source).and_then(statics::echo_cost)
            else {
                return Ok(false);
            };
            if !crate::engine::can_pay_effect_mana(controller, colors, generic, state) {
                sacrifice(state, source, controller);
                return Ok(false);
            }
            // "unless you pay its echo cost": paying is the first option.
            push_frame(
                continuation,
                EffectOp::Choice {
                    controller: PlayerRef::Controller,
                    options: vec![p2b(P2bOpV1::EchoPayV1), p2b(P2bOpV1::SacrificeSourceV1)],
                },
                &path,
            );
            Ok(false)
        }
        P2bOpV1::MasticoreUpkeepV1 => {
            let ctx = continuation.ctx.clone();
            let Some(source) = live_ability_source(&ctx, state) else {
                return Ok(false);
            };
            if state.players[controller.index()].hand.is_empty() {
                sacrifice(state, source, controller);
                return Ok(false);
            }
            push_frame(
                continuation,
                EffectOp::Choice {
                    controller: PlayerRef::Controller,
                    options: vec![
                        p2b(P2bOpV1::SacrificeSourceV1),
                        EffectOp::DiscardCards {
                            player: PlayerRef::Controller,
                            count: 1,
                        },
                    ],
                },
                &path,
            );
            Ok(false)
        }
        other => {
            execute(other, &continuation.ctx, state);
            Ok(false)
        }
    }
}

// ---- Hooks: triggers ----------------------------------------------------------------

/// The triggers this module's hooks set off in this collection, and the
/// bookkeeping of its cards.
pub(crate) fn delayed_triggers(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Vec<PendingTrigger> {
    if !cfg!(feature = "eternal-fixtures") {
        return Vec::new();
    }
    note_arrivals(state, events);
    note_turned_face_up(state, events);
    note_upkeep(state, events);
    prune(state);
    let Some(eternal) = state.eternal_v1.as_mut() else {
        return Vec::new();
    };
    if eternal.p2b.queued_triggers.is_empty() {
        return Vec::new();
    }
    std::mem::take(&mut eternal.p2b.queued_triggers)
        .into_iter()
        .map(|queued| {
            let mut pending = crate::eternal_cards_v1::delayed_trigger(
                queued.source,
                queued.controller,
                queued.effect,
            );
            if let Some(spec) = queued.target_spec {
                pending.target_spec = spec;
            }
            pending
        })
        .collect()
}

/// Caller of the Claw: counts the nontoken creatures put into a graveyard
/// from the battlefield in this batch, by owner. Called as each batch commits,
/// while the departed creature's last-known stats are on record.
fn note_arrivals(state: &mut GameState, events: &[CommittedEvent]) {
    let mut arrived = [0u16; 2];
    for event in events {
        let CommittedEvent::ZoneChange {
            object,
            from: Zone::Battlefield,
            to: Zone::Graveyard,
            ..
        } = *event
        else {
            continue;
        };
        let Some(live) = state.objects.try_get(object) else {
            continue;
        };
        if live.v4.is_token
            || live.zone != Zone::Graveyard
            || !left_as_creature(state, object, live.zone_change_count.saturating_sub(1))
        {
            continue;
        }
        let owner = live.owner.index();
        arrived[owner] = arrived[owner].saturating_add(1);
    }
    // Games without a Caller of the Claw keep no state of this module.
    if arrived == [0, 0] || !caller_in_game(state) {
        return;
    }
    let now = TurnV1::now(state);
    let record = &mut p2b_state(state).graveyard_arrivals;
    match record {
        Some(arrivals) if arrivals.turn == now => {
            for (total, more) in arrivals.counts.iter_mut().zip(arrived) {
                *total = total.saturating_add(more);
            }
        }
        _ => {
            *record = Some(GraveyardArrivalsV1 {
                turn: now,
                counts: arrived,
            })
        }
    }
}

/// Whether a Caller of the Claw is in any zone of the game, either player's.
fn caller_in_game(state: &GameState) -> bool {
    static CALLER: std::sync::OnceLock<Option<u16>> = std::sync::OnceLock::new();
    let Some(caller) = *CALLER.get_or_init(|| card_id_by_name(CALLER_OF_THE_CLAW)) else {
        return false;
    };
    let is_caller = |object: &ObjectId| {
        state
            .objects
            .try_get(*object)
            .is_some_and(|live| live.card_def == caller)
    };
    state.players.iter().any(|seat| {
        seat.hand.iter().any(is_caller)
            || seat.library.iter().any(is_caller)
            || seat.graveyard.iter().any(is_caller)
            || seat.battlefield.iter().any(is_caller)
    }) || state.exile.iter().any(is_caller)
}

/// Nantuko Vigilante: "When this creature is turned face up, destroy target
/// artifact or enchantment."
fn note_turned_face_up(state: &mut GameState, events: &[CommittedEvent]) {
    for event in events {
        let CommittedEvent::TurnedFaceUp {
            object,
            zone_change_count,
        } = *event
        else {
            continue;
        };
        let Some(live) = state.objects.try_get(object) else {
            continue;
        };
        if live.zone != Zone::Battlefield
            || live.zone_change_count != zone_change_count
            || !active_named(state, object, NANTUKO_VIGILANTE)
        {
            continue;
        }
        let controller = live.controller;
        let source = AbilitySourceContractV4::capture(state, object);
        queue_trigger(
            state,
            source,
            controller,
            p2b(P2bOpV1::VigilanteDestroyV1),
            Some(TargetSpec::ArtifactOrEnchantmentPermanent),
        );
    }
}

/// Echo (702.30a): "At the beginning of your upkeep, if this permanent came
/// under your control since the beginning of your last upkeep, sacrifice it
/// unless you pay its echo cost." The permanents owed are recorded as they
/// enter.
fn note_upkeep(state: &mut GameState, events: &[CommittedEvent]) {
    for event in events {
        let CommittedEvent::UpkeepBegan { player } = *event else {
            continue;
        };
        let Some(owed) = read_state(state).map(|record| record.echo_owed.clone()) else {
            continue;
        };
        if owed.is_empty() {
            continue;
        }
        let (due, waiting): (Vec<_>, Vec<_>) = owed.into_iter().partition(|link| {
            link_live_in(state, *link, Zone::Battlefield)
                && state.objects.get(link.object).controller == player
        });
        p2b_state(state).echo_owed = waiting;
        for link in due {
            let source = AbilitySourceContractV4::capture(state, link.object);
            queue_trigger(state, source, player, p2b(P2bOpV1::EchoUpkeepV1), None);
        }
    }
}

/// Drops records whose objects left the battlefield and arrivals from an
/// earlier turn.
fn prune(state: &mut GameState) {
    let Some(p2b) = read_state(state) else {
        return;
    };
    if p2b.is_empty() {
        return;
    }
    let now = TurnV1::now(state);
    let on_battlefield = |link: &ObjectLinkV4| link_live_in(state, *link, Zone::Battlefield);
    let echo_owed = p2b
        .echo_owed
        .iter()
        .copied()
        .filter(on_battlefield)
        .collect::<Vec<_>>();
    let fade_counters = p2b
        .fade_counters
        .iter()
        .copied()
        .filter(|(host, _)| on_battlefield(host))
        .collect::<Vec<_>>();
    let arrivals = p2b.graveyard_arrivals.filter(|record| record.turn == now);
    if echo_owed.len() != p2b.echo_owed.len()
        || fade_counters.len() != p2b.fade_counters.len()
        || arrivals != p2b.graveyard_arrivals
    {
        let p2b = p2b_state(state);
        p2b.echo_owed = echo_owed;
        p2b.fade_counters = fade_counters;
        p2b.graveyard_arrivals = arrivals;
    }
}

/// The beginning of a turn ends the last turn's records.
pub(crate) fn begin_turn(state: &mut GameState, _player: PlayerId) {
    prune(state);
}
