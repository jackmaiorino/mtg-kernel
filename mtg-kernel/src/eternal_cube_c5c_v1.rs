//! Card programs for the eternal catalog's Vintage Cube wave C5c
//! (`docs/design/eternal_formats_support_v1.md`): reanimation spells and enchantments, a sweeper and a land.
//!
//! `eternal_cards_v1` forwards this module's effect leaves here, and the
//! recipe tables of `build_eternal_c5c_v1.rs` name its programs. Every
//! table is keyed by a printed name that occurs only in the eternal registry,
//! so the Pauper, FDN and Standard catalogs never reach it. Behavior was read
//! from each card's current Oracle text and, where present, its XMage source.
//!
//! - Sunfall exiles every creature as one batch and incubates for the number
//!   that left the battlefield (Decree of Pain's counting).
//! - Corpse Dance reuses the kicker slot for buyback: a paid cast marks the
//!   resolving spell, and the departure hook in `engine::plan_spell_departure`
//!   puts a marked spell into its owner's hand instead of the graveyard. The
//!   returned creature's end-step exile is this module's own delayed trigger.
//! - Necromancy's cast-time bookkeeping is a `delayed_triggers` hook, its
//!   cleanup sacrifice is queued from `engine::expire_cleanup_effects_v1`, and
//!   once its entry trigger resolves it is an Aura for state-based actions
//!   (`invalid_necromancy_aura`, called by `trigger::invalid_sba_aura`).
//! - Recurring Nightmare's "return this to hand" cost is
//!   `CostComponent::ReturnSelfToOwnersHandV1`.
//! - Aang's Iceberg's waterbend is four activated abilities, one for each
//!   number of permanents tapped to help pay.
//! - Shifting Woodland's copy is wave E7a's `become_copy`, ended at cleanup.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{CardDef, CardType, Keywords, TargetSpec, CARD_DEFS};
use crate::effect::{
    EffectContinuation, EffectFrame, EffectObjectBinding, EffectOp, ExecCtx, ObjectRef, PlayerRef,
};
use crate::eternal_cards_v1::EternalOpV1;
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::{StandardOpV1, StandardTargetV1};
use crate::state::{
    AbilitySourceContractV4, GameState, ObjectLinkV4, StackItem, StackItemKind, Step, Target, Zone,
};
use crate::trigger::{PendingTrigger, TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

const AANGS_ICEBERG: &str = "Aang's Iceberg";
const NECROMANCY: &str = "Necromancy";
const SHIFTING_WOODLAND: &str = "Shifting Woodland";

/// Per-game state for this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Default (and absent from snapshots)
/// until a card of this wave needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct C5cStateV1 {
    /// Corpse Dance spells that resolved after a buyback payment, by stack
    /// incarnation: they go to their owner's hand instead of the graveyard.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub buybacks: Vec<ObjectLinkV4>,
    /// Creatures Corpse Dance returned, with the controller of the spell, to
    /// be exiled at the beginning of the next end step.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dance_exiles: Vec<(ObjectLinkV4, PlayerId)>,
    /// Necromancy spells cast any time a sorcery couldn't have been cast, by
    /// stack incarnation: the permanent is sacrificed at the beginning of the
    /// next cleanup step.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub instant_casts: Vec<ObjectLinkV4>,
    /// Necromancies that became Auras, with the creature each put onto the
    /// battlefield (none when it did not arrive).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub necromancies: Vec<(ObjectLinkV4, Option<ObjectLinkV4>)>,
    /// Shifting Woodlands that became a copy until end of turn.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub woodland_copies: Vec<ObjectLinkV4>,
}

impl C5cStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

fn c5c_state(state: &mut GameState) -> &mut C5cStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).c5c
}

fn read_state(state: &GameState) -> Option<&C5cStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.c5c)
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::C5cV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C5cOpV1 {
    /// Sunfall: exile all creatures, then incubate X where X is the number of
    /// creatures exiled this way.
    SunfallV1,
    /// Corpse Dance: return the top creature card of the controller's
    /// graveyard to the battlefield with haste until end of turn, to be
    /// exiled at the beginning of the next end step; a buyback payment puts
    /// the spell into its owner's hand as it resolves.
    CorpseDanceV1,
    /// Corpse Dance's delayed trigger: exile `creature`.
    CorpseDanceExileV1 { creature: ObjectLinkV4 },
    /// Necromancy's entry trigger: if it is on the battlefield, it becomes an
    /// Aura, the target creature card from a graveyard enters under the
    /// controller's control and Necromancy attaches to it.
    NecromancyEntersV1,
    /// Necromancy's leave trigger: the creature's controller sacrifices it.
    NecromancyLeavesV1,
    /// Aang's Iceberg: sacrifice it, and if you do, scry 2. A choice.
    IcebergSacrificeScryV1,
    /// Shifting Woodland: this land becomes a copy of target permanent card
    /// in its controller's graveyard until end of turn.
    WoodlandCopyV1,
}

impl C5cOpV1 {
    /// Whether the leaf may stop for a player's choice (or a draw that a
    /// replacement can interrupt), so it runs in the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(self, Self::IcebergSacrificeScryV1)
    }
}

fn c5c(op: C5cOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C5cV1(op)))
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
    CARD_DEFS.get(usize::from(live.card_def))
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
/// resolving spell or ability under `spec` (608.2b).
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

/// Puts `card` onto the battlefield under `controller`'s control.
fn put_onto_battlefield(state: &mut GameState, card: ObjectId, controller: PlayerId) {
    event::propose_and_commit(
        state,
        ProposedEvent::zone_change_to_battlefield_under_controller(card, controller),
    );
}

/// Sacrifices `object` if it is still on the battlefield.
fn sacrifice(state: &mut GameState, object: ObjectId) {
    if state.objects.get(object).zone != Zone::Battlefield {
        return;
    }
    event::log_sacrifice(state, object);
    event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Graveyard));
}

/// "Sacrifice this permanent" for a delayed trigger's own source (wave E3's
/// City of Traitors leaf).
fn sacrifice_source() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::SacrificeSourceV1))
}

// ---- Programs -----------------------------------------------------------------------

/// Sunfall: "Exile all creatures. Incubate X, where X is the number of
/// creatures exiled this way."
pub fn sunfall() -> EffectOp {
    c5c(C5cOpV1::SunfallV1)
}

/// Corpse Dance: "Return the top creature card of your graveyard to the
/// battlefield. That creature gains haste until end of turn. Exile it at the
/// beginning of the next end step." Buyback {2}.
pub fn corpse_dance() -> EffectOp {
    c5c(C5cOpV1::CorpseDanceV1)
}

fn corpse_dance_exile(creature: ObjectLinkV4) -> EffectOp {
    c5c(C5cOpV1::CorpseDanceExileV1 { creature })
}

/// Necromancy's entry trigger.
pub fn necromancy_enters() -> EffectOp {
    c5c(C5cOpV1::NecromancyEntersV1)
}

/// Necromancy's leave trigger: "When this enchantment leaves the battlefield,
/// that creature's controller sacrifices it."
pub fn necromancy_leaves() -> EffectOp {
    c5c(C5cOpV1::NecromancyLeavesV1)
}

/// Recurring Nightmare: "Return target creature card from your graveyard to
/// the battlefield." The sacrifice and the return of the enchantment are
/// costs.
pub fn nightmare_return() -> EffectOp {
    EffectOp::ReturnTargetPermanentToBattlefield { target_index: 0 }
}

/// Aang's Iceberg: "When this enchantment enters, exile up to one other
/// target nonland permanent until this enchantment leaves the battlefield."
pub fn iceberg_enters() -> EffectOp {
    EffectOp::ExileTargetLinkedToSource {
        object: ObjectRef::Target(0),
    }
}

/// Aang's Iceberg: "Sacrifice this enchantment. If you do, scry 2."
pub fn iceberg_waterbend() -> EffectOp {
    c5c(C5cOpV1::IcebergSacrificeScryV1)
}

/// Shifting Woodland: "This land becomes a copy of target permanent card in
/// your graveyard until end of turn."
pub fn woodland_copy() -> EffectOp {
    c5c(C5cOpV1::WoodlandCopyV1)
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

const NECROMANCY_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, necromancy_enters),
    trigger(TriggerCondition::LeftBattlefield, necromancy_leaves),
];
const ICEBERG_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, iceberg_enters)];

/// Triggered abilities of this module's cards, falling through to wave
/// C5d.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        NECROMANCY => &NECROMANCY_TRIGGERS,
        AANGS_ICEBERG => &ICEBERG_TRIGGERS,
        _ => crate::eternal_cube_c5d_v1::triggers_for(name),
    }
}

/// Target specs of this module's printed and queued triggered abilities.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    match name {
        NECROMANCY if *effect == necromancy_enters() => Some(TargetSpec::StandardV1(
            StandardTargetV1::CreatureCardInAGraveyardV1,
        )),
        NECROMANCY if *effect == necromancy_leaves() => Some(TargetSpec::None),
        AANGS_ICEBERG if *effect == iceberg_enters() => Some(TargetSpec::StandardV1(
            StandardTargetV1::UpToOneOtherNonlandPermanentV1,
        )),
        _ => delayed_effect_matches(name, effect).then_some(TargetSpec::None),
    }
}

/// Queued triggers owned by this module's cards: Corpse Dance's exile (whose
/// source is the returned creature, whatever its name) and Necromancy's
/// cleanup sacrifice.
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    if matches!(
        effect,
        EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C5cV1(
            C5cOpV1::CorpseDanceExileV1 { .. }
        )))
    ) {
        return true;
    }
    name == NECROMANCY && *effect == sacrifice_source()
}

// ---- Hooks ----------------------------------------------------------------------------

/// Buyback: whether the resolving spell `item` was marked by Corpse Dance's
/// payment and goes to its owner's hand rather than the graveyard.
pub(crate) fn returns_to_hand_on_departure(state: &GameState, item: &StackItem) -> bool {
    if item.is_copy {
        return false;
    }
    let Some(live) = state.objects.try_get(item.source) else {
        return false;
    };
    live.zone == Zone::Stack
        && read_state(state).is_some_and(|c5c| {
            c5c.buybacks.contains(&ObjectLinkV4 {
                object: item.source,
                zone_change_count: live.zone_change_count,
            })
        })
}

/// Shifting Woodland's delirium: "Activate only if there are four or more
/// card types among cards in your graveyard."
pub(crate) fn activation_allowed(
    state: &GameState,
    source: ObjectId,
    name: &str,
    ability_index: usize,
) -> bool {
    if name != SHIFTING_WOODLAND || ability_index != 0 {
        return true;
    }
    let controller = state.objects.get(source).controller;
    crate::eternal_cards_v1::controller_has_delirium(controller, state)
}

/// Necromancy once its entry trigger has resolved: "it becomes an Aura". Read
/// by the engine's effective subtypes while that incarnation is on the
/// battlefield.
pub(crate) fn is_necromancy_aura(state: &GameState, id: ObjectId) -> bool {
    read_state(state).is_some_and(|c5c| {
        c5c.necromancies
            .iter()
            .any(|(aura, _)| aura.object == id && link_live_in(state, *aura, Zone::Battlefield))
    })
}

/// 704.5m for Necromancy once it became an Aura: whether it goes to its
/// owner's graveyard because it is not attached to the creature it put onto
/// the battlefield, or that creature is no longer a legal host. `None` for
/// every other object.
pub(crate) fn invalid_necromancy_aura(state: &GameState, id: ObjectId) -> Option<bool> {
    let c5c = read_state(state)?;
    if c5c.necromancies.is_empty() {
        return None;
    }
    let live = state.objects.try_get(id)?;
    if live.zone != Zone::Battlefield {
        return None;
    }
    let aura = ObjectLinkV4 {
        object: id,
        zone_change_count: live.zone_change_count,
    };
    let (_, creature) = c5c.necromancies.iter().find(|(link, _)| *link == aura)?;
    let valid = match (creature, live.v4.attached_to) {
        (Some(creature), Some(host)) => {
            host == *creature
                && state.objects.try_get(host.object).is_some_and(|host_live| {
                    host_live.zone == Zone::Battlefield
                        && host_live.zone_change_count == host.zone_change_count
                        && host_live.attachments.contains(&id)
                })
                && crate::engine::object_has_type(state, host.object, CardType::Creature)
                && !crate::progenitus_v1::attachment_forbidden(state, id, host.object)
                && !crate::controlled_creature_grants_v1::protects_from_colors(
                    state,
                    host.object,
                    crate::engine::object_color_mask(state, id),
                )
        }
        _ => false,
    };
    Some(!valid)
}

/// The beginning of a cleanup step: Shifting Woodland copies end ("until end
/// of turn"), and a Necromancy cast at instant speed this turn is sacrificed
/// by a trigger ("at the beginning of the next cleanup step").
pub(crate) fn begin_cleanup(state: &mut GameState) {
    if !cfg!(feature = "eternal-fixtures") || read_state(state).is_none_or(C5cStateV1::is_empty) {
        return;
    }
    let copies = std::mem::take(&mut c5c_state(state).woodland_copies);
    for link in copies {
        if link_live_in(state, link, Zone::Battlefield) {
            crate::eternal_legacy_e7a_v1::end_copy_before_departure(state, link.object);
        }
    }
    let casts = std::mem::take(&mut c5c_state(state).instant_casts);
    let mut waiting = Vec::new();
    for link in casts {
        let Some(live) = state.objects.try_get(link.object) else {
            continue;
        };
        if live.zone == Zone::Stack && live.zone_change_count == link.zone_change_count {
            // Still resolving (cast during cleanup): the next cleanup step.
            waiting.push(link);
        } else if live.zone == Zone::Battlefield
            && Some(live.zone_change_count) == link.zone_change_count.checked_add(1)
        {
            let controller = live.controller;
            let source = AbilitySourceContractV4::capture(state, link.object);
            state
                .engine
                .pending_triggers
                .push(crate::eternal_cards_v1::delayed_trigger(
                    source,
                    controller,
                    sacrifice_source(),
                ));
        }
    }
    c5c_state(state).instant_casts = waiting;
}

/// The triggers this module's hooks set off in this collection, and its
/// bookkeeping.
pub(crate) fn delayed_triggers(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Vec<PendingTrigger> {
    if !cfg!(feature = "eternal-fixtures") {
        return Vec::new();
    }
    note_necromancy_casts(state, events);
    let mut triggers = Vec::new();
    if events
        .iter()
        .any(|event| matches!(event, CommittedEvent::BeginningEndStep { .. }))
    {
        corpse_dance_exile_triggers(state, &mut triggers);
    }
    prune(state);
    triggers
}

/// Necromancy: "If you cast it any time a sorcery couldn't have been cast":
/// the cast was not by the active player in a main phase with an empty stack.
fn note_necromancy_casts(state: &mut GameState, events: &[CommittedEvent]) {
    for event in events {
        let CommittedEvent::SpellCast { spell, controller } = event else {
            continue;
        };
        let (spell, controller) = (*spell, *controller);
        let Some(live) = state.objects.try_get(spell) else {
            continue;
        };
        if live.zone != Zone::Stack
            || CARD_DEFS
                .get(usize::from(live.card_def))
                .is_none_or(|def| def.name != NECROMANCY)
        {
            continue;
        }
        let at_sorcery_timing = controller == state.active_player
            && matches!(state.step, Step::Main1 | Step::Main2)
            && state
                .stack
                .iter()
                .position(|item| item.kind == StackItemKind::Spell && item.source == spell)
                == Some(0);
        if !at_sorcery_timing {
            let link = live_link(state, spell);
            let casts = &mut c5c_state(state).instant_casts;
            if !casts.contains(&link) {
                casts.push(link);
            }
        }
    }
}

/// "Exile it at the beginning of the next end step" (Corpse Dance): one
/// trigger for each returned creature still on the battlefield, with the
/// creature as its source.
fn corpse_dance_exile_triggers(state: &mut GameState, triggers: &mut Vec<PendingTrigger>) {
    let records = state
        .eternal_v1
        .as_mut()
        .map(|eternal| std::mem::take(&mut eternal.c5c.dance_exiles))
        .unwrap_or_default();
    for (creature, controller) in records {
        if !link_live_in(state, creature, Zone::Battlefield) {
            continue;
        }
        let mut source = AbilitySourceContractV4::capture(state, creature.object);
        source.controller = controller;
        triggers.push(crate::eternal_cards_v1::delayed_trigger(
            source,
            controller,
            corpse_dance_exile(creature),
        ));
    }
}

/// Drops records whose objects moved on.
fn prune(state: &mut GameState) {
    let Some(c5c) = read_state(state) else {
        return;
    };
    if c5c.is_empty() {
        return;
    }
    let buybacks = c5c
        .buybacks
        .iter()
        .copied()
        .filter(|&link| link_live_in(state, link, Zone::Stack))
        .collect::<Vec<_>>();
    let dance_exiles = c5c
        .dance_exiles
        .iter()
        .copied()
        .filter(|&(link, _)| link_live_in(state, link, Zone::Battlefield))
        .collect::<Vec<_>>();
    let instant_casts = c5c
        .instant_casts
        .iter()
        .copied()
        .filter(|&link| {
            link_live_in(state, link, Zone::Stack)
                || state.objects.try_get(link.object).is_some_and(|live| {
                    live.zone == Zone::Battlefield
                        && Some(live.zone_change_count) == link.zone_change_count.checked_add(1)
                })
        })
        .collect::<Vec<_>>();
    let necromancies = c5c
        .necromancies
        .iter()
        .copied()
        .filter(|&(aura, creature)| {
            link_live_in(state, aura, Zone::Battlefield)
                || creature.is_some_and(|link| link_live_in(state, link, Zone::Battlefield))
        })
        .collect::<Vec<_>>();
    let woodland_copies = c5c
        .woodland_copies
        .iter()
        .copied()
        .filter(|&link| link_live_in(state, link, Zone::Battlefield))
        .collect::<Vec<_>>();
    let c5c = c5c_state(state);
    c5c.buybacks = buybacks;
    c5c.dance_exiles = dance_exiles;
    c5c.instant_casts = instant_casts;
    c5c.necromancies = necromancies;
    c5c.woodland_copies = woodland_copies;
}

// ---- Execution ----------------------------------------------------------------------

/// Simultaneously exiles every creature on the battlefield, returning the
/// number that left it (tokens included: they were exiled, then ceased to
/// exist). Exile ignores indestructible and regeneration, and no creature
/// dies.
fn exile_all_creatures(state: &mut GameState) -> u32 {
    let creatures = [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
        .filter(|&object| crate::engine::object_has_type(state, object, CardType::Creature))
        .map(|object| live_link(state, object))
        .collect::<Vec<_>>();
    if creatures.is_empty() {
        return 0;
    }
    event::propose_and_commit_batch(
        state,
        creatures
            .iter()
            .map(|link| ProposedEvent::zone_change(link.object, Zone::Exile))
            .collect(),
    );
    if state.engine.halted.is_some() || crate::graveyard_order_v1::paused(state) {
        return 0;
    }
    let exiled = creatures
        .iter()
        .filter(|&&link| !link_live_in(state, link, Zone::Battlefield))
        .count();
    u32::try_from(exiled).unwrap_or(u32::MAX)
}

/// Marks the resolving Corpse Dance spell as bought back.
fn mark_buyback(ctx: &ExecCtx, state: &mut GameState) {
    let Some(live) = state.objects.try_get(ctx.source) else {
        return;
    };
    if live.zone != Zone::Stack || live.spell_copy_origin.is_some() {
        return;
    }
    let link = live_link(state, ctx.source);
    let buybacks = &mut c5c_state(state).buybacks;
    if !buybacks.contains(&link) {
        buybacks.push(link);
    }
}

fn corpse_dance_resolve(ctx: &ExecCtx, state: &mut GameState) {
    // Buyback is paid whether or not a creature card is there to return.
    if ctx.kicked {
        mark_buyback(ctx, state);
    }
    let controller = ctx.controller;
    // The top of a graveyard is its most recently added card.
    let Some(card) = state.players[controller.index()]
        .graveyard
        .iter()
        .rev()
        .copied()
        .find(|&card| {
            let live = state.objects.get(card);
            live.owner == controller
                && !live.v4.is_token
                && definition(state, card).is_some_and(|def| def.has_type(CardType::Creature))
        })
    else {
        return;
    };
    put_onto_battlefield(state, card, controller);
    if state.objects.get(card).zone != Zone::Battlefield {
        return;
    }
    let binding = bind(state, card);
    crate::effect::install_temporary_boost(state, binding, 0, 0, Keywords::HASTE);
    let creature = live_link(state, card);
    c5c_state(state).dance_exiles.push((creature, controller));
}

fn necromancy_resolve_enters(ctx: &ExecCtx, state: &mut GameState) {
    // "If it's on the battlefield": the whole ability does nothing otherwise.
    let Some(necromancy) = live_ability_source(ctx, state) else {
        return;
    };
    let spec = TargetSpec::StandardV1(StandardTargetV1::CreatureCardInAGraveyardV1);
    // A target that is gone leaves Necromancy as it was, an Enchantment.
    let Some(card) = legal_target(ctx, state, spec, 0, Zone::Graveyard) else {
        return;
    };
    let aura = live_link(state, necromancy);
    put_onto_battlefield(state, card, ctx.controller);
    let creature =
        (state.objects.get(card).zone == Zone::Battlefield).then(|| live_link(state, card));
    if let Some(creature) = creature {
        // A host that cannot be enchanted leaves Necromancy unattached, an
        // Aura that state-based actions put into the graveyard.
        let _ = state.attach_object_exact(
            aura.object,
            aura.zone_change_count,
            creature.object,
            creature.zone_change_count,
        );
    }
    c5c_state(state).necromancies.push((aura, creature));
}

fn necromancy_resolve_leaves(ctx: &ExecCtx, state: &mut GameState) {
    let Some(contract) = ctx.ability_source_contract else {
        return;
    };
    let aura = ObjectLinkV4 {
        object: contract.source,
        zone_change_count: contract.zone_change_count,
    };
    let Some(position) = read_state(state)
        .and_then(|c5c| c5c.necromancies.iter().position(|(link, _)| *link == aura))
    else {
        return;
    };
    let (_, creature) = c5c_state(state).necromancies.remove(position);
    if let Some(creature) = creature.filter(|&link| link_live_in(state, link, Zone::Battlefield)) {
        sacrifice(state, creature.object);
    }
}

fn woodland_resolve_copy(ctx: &ExecCtx, state: &mut GameState) {
    let Some(host) = live_ability_source(ctx, state) else {
        return;
    };
    let Some(card) = legal_target(
        ctx,
        state,
        TargetSpec::PermanentCardInOwnGraveyard,
        0,
        Zone::Graveyard,
    ) else {
        return;
    };
    let copied = state.objects.get(card).card_def;
    crate::eternal_legacy_e7a_v1::become_copy(state, host, copied, 0);
    let link = live_link(state, host);
    let copies = &mut c5c_state(state).woodland_copies;
    if !copies.contains(&link) {
        copies.push(link);
    }
}

/// Runs one of this module's leaves in synchronous execution.
pub(crate) fn execute(op: &C5cOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        C5cOpV1::SunfallV1 => {
            let exiled = exile_all_creatures(state);
            if state.engine.halted.is_none() && !crate::graveyard_order_v1::paused(state) {
                // Incubate happens even for X = 0.
                crate::effect::execute(
                    &EffectOp::Incubate {
                        amount: u16::try_from(exiled).unwrap_or(u16::MAX),
                    },
                    ctx,
                    state,
                );
            }
        }
        C5cOpV1::CorpseDanceV1 => corpse_dance_resolve(ctx, state),
        C5cOpV1::CorpseDanceExileV1 { creature } => {
            if link_live_in(state, *creature, Zone::Battlefield) {
                event::propose_and_commit(
                    state,
                    ProposedEvent::zone_change(creature.object, Zone::Exile),
                );
            }
        }
        C5cOpV1::NecromancyEntersV1 => necromancy_resolve_enters(ctx, state),
        C5cOpV1::NecromancyLeavesV1 => necromancy_resolve_leaves(ctx, state),
        C5cOpV1::WoodlandCopyV1 => woodland_resolve_copy(ctx, state),
        // The scry runs in `drive`.
        C5cOpV1::IcebergSacrificeScryV1 => halt(state, ctx.source),
    }
}

/// Runs one of this module's leaves in the resumable interpreter.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &C5cOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    match op {
        C5cOpV1::IcebergSacrificeScryV1 => {
            // "Sacrifice this enchantment. If you do, scry 2": the Iceberg
            // must still be there, under its controller's control.
            let Some(iceberg) = live_ability_source(&continuation.ctx, state) else {
                return Ok(false);
            };
            if state.objects.get(iceberg).controller != continuation.ctx.controller {
                return Ok(false);
            }
            sacrifice(state, iceberg);
            if state.engine.halted.is_none()
                && !crate::graveyard_order_v1::paused(state)
                && state.objects.get(iceberg).zone != Zone::Battlefield
            {
                continuation.frames.push(EffectFrame::Program {
                    op: EffectOp::Scry {
                        player: PlayerRef::Controller,
                        count: 2,
                    },
                    path,
                });
            }
            Ok(false)
        }
        other => {
            execute(other, &continuation.ctx, state);
            Ok(false)
        }
    }
}
