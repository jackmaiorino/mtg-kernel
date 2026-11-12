//! Card programs for the eternal catalog's Legacy sideboard wave S1g
//! (`docs/design/eternal_formats_support_v1.md`): Damping Sphere, which
//! completes Sewer-Cam, Painter and Mono-Black Midrange, Damping Sphere and
//! Drop of Honey, which complete Lands, and Alpine Moon and Ghost Vacuum,
//! which complete Sneak and Show.
//!
//! `eternal_cards_v1` forwards its effect leaves, triggers and turn
//! bookkeeping here, wave S1f's recipe tables fall through to
//! `build_eternal_s1g_v1.rs`, and the engine calls the hooks below only in
//! `eternal-fixtures` builds. Every table is keyed by a printed name that
//! occurs only in the eternal registry. Behavior follows each card's current
//! Oracle text and rulings, checked against its XMage source.
//!
//! Primitives added here:
//! - a replacement of the mana a land tapped for two or more mana produces
//!   by one {C}, applied by the engine's payment planner and mana
//!   activations, and a spell tax of {1} for each other spell the caster
//!   cast this turn (Damping Sphere);
//! - an upkeep destruction of the creature with the least power, chosen
//!   among ties, and a state trigger while no creature is on the
//!   battlefield (Drop of Honey);
//! - a land name chosen as a permanent enters, and opponents' lands with
//!   that name that lose their land types and abilities and tap for any
//!   color, joined to wave E8a's land-type setter (Alpine Moon);
//! - graveyard exiles recorded with the exiling permanent's incarnation,
//!   and the creature cards among them returned as 1/1 Spirits with a
//!   flying counter (Ghost Vacuum).
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{card_id_by_name, CardType, Keywords, TargetSpec, CARD_DEFS};
use crate::effect::{EffectContinuation, EffectObjectBinding, EffectOp, ExecCtx};
use crate::eternal_cards_v1::e3::{E3ChoiceKindV1, E3ChoiceV1};
use crate::eternal_cards_v1::EternalOpV1;
use crate::eternal_legacy_e9a_v1::TurnV1;
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::{StandardOpV1, StandardTargetV1};
use crate::state::{AbilitySourceContractV4, GameState, ObjectLinkV4, Target, Zone};
use crate::trigger::{PendingTrigger, TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

pub(crate) const DAMPING_SPHERE: &str = "Damping Sphere";
pub(crate) const DROP_OF_HONEY: &str = "Drop of Honey";
pub(crate) const ALPINE_MOON: &str = "Alpine Moon";

/// Per-game state of this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Absent from every snapshot until one
/// of them needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct S1gStateV1 {
    /// The land name each Alpine Moon chose as it entered, by the Moon's
    /// battlefield incarnation: an index into `alpine_name_options`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alpine_names: Vec<(ObjectLinkV4, u16)>,
    /// Ghost Vacuum's exiles: the Vacuum's battlefield incarnation and the
    /// exiled card's exile incarnation (607.2a).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vacuum_exiles: Vec<(ObjectLinkV4, ObjectLinkV4)>,
    /// The creature cards Ghost Vacuum put onto the battlefield, by
    /// battlefield incarnation, with the timestamp of their 1/1 Spirit
    /// characteristics and of their flying counter.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spirits: Vec<(ObjectLinkV4, u64)>,
    /// Drop of Honey incarnations whose state trigger triggered and has not
    /// yet left the stack (603.8).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub honey_triggered: Vec<ObjectLinkV4>,
}

impl S1gStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

pub(crate) fn s1g_state(state: &mut GameState) -> &mut S1gStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).s1g
}

pub fn read_state(state: &GameState) -> Option<&S1gStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.s1g)
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::S1gV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum S1gOpV1 {
    /// Drop of Honey's upkeep trigger: destroy the creature with the least
    /// power, which can't be regenerated. A choice among ties.
    HoneyUpkeepV1,
    /// Drop of Honey's state trigger: sacrifice this enchantment.
    HoneySacrificeV1,
    /// Ghost Vacuum's first ability: exile target card 0 from a graveyard.
    VacuumExileV1,
    /// Ghost Vacuum's second ability: put each creature card exiled with the
    /// sacrificed Vacuum onto the battlefield as a 1/1 Spirit with a flying
    /// counter.
    VacuumReturnV1,
}

impl S1gOpV1 {
    /// Whether this leaf can yield a player decision, so its program must
    /// use the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(self, Self::HoneyUpkeepV1)
    }
}

/// Resolution choices owned by this module, carried as
/// `E3ChoiceKindV1::S1gV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum S1gChoiceV1 {
    /// Drop of Honey: the creature to destroy among those tied for the
    /// least power.
    HoneyDestroyV1,
}

impl S1gChoiceV1 {
    /// Whether the candidates are hidden from the other player: they are
    /// battlefield permanents.
    pub(crate) fn chooser_private(&self) -> bool {
        false
    }

    pub(crate) fn public_purpose(&self) -> crate::rl::TargetSelectionPurposeV4 {
        crate::rl::TargetSelectionPurposeV4::PermanentSelection
    }

    /// Object incarnations this choice refers to beyond its candidates.
    pub(crate) fn referenced_bindings(&self) -> Vec<EffectObjectBinding> {
        Vec::new()
    }
}

fn s1g(op: S1gOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::S1gV1(op)))
}

fn link(state: &GameState, object: ObjectId) -> ObjectLinkV4 {
    ObjectLinkV4 {
        object,
        zone_change_count: state.objects.get(object).zone_change_count,
    }
}

/// `object`'s battlefield incarnation, if it is on the battlefield.
fn battlefield_link(state: &GameState, object: ObjectId) -> Option<ObjectLinkV4> {
    let live = state.objects.try_get(object)?;
    (live.zone == Zone::Battlefield).then_some(ObjectLinkV4 {
        object,
        zone_change_count: live.zone_change_count,
    })
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

fn binding_live(state: &GameState, binding: &EffectObjectBinding) -> bool {
    state.objects.try_get(binding.object).is_some_and(|live| {
        live.zone == binding.expected_zone
            && live.zone_change_count == binding.expected_zone_change_count
    })
}

fn halt(state: &mut GameState, source: ObjectId) {
    state.engine.halted = Some((
        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
        source,
    ));
}

/// Every battlefield object, in each player's battlefield order.
fn battlefield(state: &GameState) -> impl Iterator<Item = ObjectId> + '_ {
    [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(move |player| state.players[player.index()].battlefield.iter().copied())
        .filter(move |&object| state.objects.get(object).zone == Zone::Battlefield)
}

/// Whether `object` is a front-face battlefield permanent named `name`
/// whose printed abilities apply.
fn active_named(state: &GameState, object: ObjectId, name: &str) -> bool {
    state.objects.try_get(object).is_some_and(|live| {
        live.zone == Zone::Battlefield
            && live.v4.face_index == 0
            && CARD_DEFS
                .get(usize::from(live.card_def))
                .is_some_and(|def| def.name == name && def.is_executable())
            && crate::continuous_characteristics_v1::printed_abilities_active(state, object)
    })
}

/// The battlefield permanents with definition `id`, named `name`, whose
/// printed abilities apply. Compares definition ids first.
fn active_with_id<'a>(
    state: &'a GameState,
    id: Option<u16>,
    name: &'a str,
) -> impl Iterator<Item = ObjectId> + 'a {
    battlefield(state).filter(move |&object| {
        id.is_some_and(|id| state.objects.get(object).card_def == id)
            && active_named(state, object, name)
    })
}

/// Target `index` of the resolving spell or ability if it is still the
/// announced incarnation and still legal for `spec` (608.2b).
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

/// The resolving ability's source incarnation as the ability was activated
/// or triggered, whatever zone it is in now.
fn ability_source_link(ctx: &ExecCtx) -> Option<ObjectLinkV4> {
    let contract = ctx.ability_source_contract?;
    Some(ObjectLinkV4 {
        object: contract.source,
        zone_change_count: contract.zone_change_count,
    })
}

struct StaticIds {
    damping_sphere: Option<u16>,
    drop_of_honey: Option<u16>,
    alpine_moon: Option<u16>,
}

fn static_ids() -> &'static StaticIds {
    static IDS: std::sync::OnceLock<StaticIds> = std::sync::OnceLock::new();
    IDS.get_or_init(|| StaticIds {
        damping_sphere: card_id_by_name(DAMPING_SPHERE),
        drop_of_honey: card_id_by_name(DROP_OF_HONEY),
        alpine_moon: card_id_by_name(ALPINE_MOON),
    })
}

// ---- Programs -----------------------------------------------------------------------

/// Ghost Vacuum: "{T}: Exile target card from a graveyard."
pub fn vacuum_exile() -> EffectOp {
    s1g(S1gOpV1::VacuumExileV1)
}

/// Ghost Vacuum: "{6}, {T}, Sacrifice this artifact: Put each creature card
/// exiled with this artifact onto the battlefield under your control with a
/// flying counter on it. Each of them is a 1/1 Spirit in addition to its
/// other types. Activate only as a sorcery."
pub fn vacuum_return() -> EffectOp {
    s1g(S1gOpV1::VacuumReturnV1)
}

fn honey_upkeep() -> EffectOp {
    s1g(S1gOpV1::HoneyUpkeepV1)
}

fn honey_sacrifice() -> EffectOp {
    s1g(S1gOpV1::HoneySacrificeV1)
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

/// Drop of Honey: "At the beginning of your upkeep, destroy the creature
/// with the least power. It can't be regenerated. If two or more creatures
/// are tied for least power, you choose one of them." Its state trigger is
/// queued by `note_honey_state` below.
const HONEY_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::BeginningOfUpkeep {
        controller_only: true,
    },
    honey_upkeep,
)];

/// Triggered abilities of this module's cards.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        DROP_OF_HONEY => &HONEY_TRIGGERS,
        _ => &[],
    }
}

/// Target specs of this module's triggered abilities: none of them
/// targets (its 2021-03-19 ruling).
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    let printed = triggers_for(name)
        .iter()
        .any(|trigger| (trigger.effect)() == *effect);
    (printed || delayed_effect_matches(name, effect)).then_some(TargetSpec::None)
}

/// Queued triggers owned by this module's cards: Drop of Honey's state
/// trigger.
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    cfg!(feature = "eternal-fixtures") && name == DROP_OF_HONEY && *effect == honey_sacrifice()
}

// ---- Damping Sphere -----------------------------------------------------------------

/// The Damping Spheres whose abilities apply.
fn spheres(state: &GameState) -> usize {
    active_with_id(state, static_ids().damping_sphere, DAMPING_SPHERE).count()
}

/// Damping Sphere: "If a land is tapped for two or more mana, it produces
/// {C} instead of any other type and amount." Whether a Sphere's abilities
/// apply and `source` is a land, so a mana ability of it with {T} in its
/// cost that would add two or more mana adds one {C} instead. The engine's
/// payment planner and mana activations apply the replacement.
pub(crate) fn damps(state: &GameState, source: ObjectId) -> bool {
    cfg!(feature = "eternal-fixtures")
        && state
            .objects
            .try_get(source)
            .is_some_and(|live| live.zone == Zone::Battlefield)
        && crate::engine::object_has_type(state, source, CardType::Land)
        && spheres(state) > 0
}

/// Damping Sphere: "Each spell a player casts costs {1} more to cast for
/// each other spell that player has cast this turn." The generic increase
/// for a spell `caster` casts, counted from the turn's cast history whether
/// or not a Sphere was on the battlefield then (its ruling); the spell being
/// cast is counted once its casting finishes.
pub(crate) fn spell_tax(state: &GameState, caster: PlayerId) -> u32 {
    if !cfg!(feature = "eternal-fixtures") {
        return 0;
    }
    let cast = state.players[caster.index()].spells_cast_this_turn;
    if cast == 0 {
        return 0;
    }
    u32::try_from(spheres(state))
        .unwrap_or(u32::MAX)
        .saturating_mul(u32::from(cast))
}

// ---- Drop of Honey ------------------------------------------------------------------

/// Whether any creature is on the battlefield.
fn any_creature(state: &GameState) -> bool {
    battlefield(state)
        .any(|object| crate::engine::object_has_type(state, object, CardType::Creature))
}

/// Destroys `creature`, which can't be regenerated (wave P1a's record, for
/// this destruction only).
fn destroy_unregenerable(ctx: &ExecCtx, state: &mut GameState, creature: ObjectId) {
    let record = (link(state, creature), TurnV1::now(state));
    let added = !crate::eternal_premodern_p1a_v1::statics::cant_be_regenerated(state, creature);
    if added {
        crate::eternal_premodern_p1a_v1::forbid_regeneration(state, creature);
    }
    crate::eternal_premodern_p1a_v1::destroy(ctx, state, creature);
    if added {
        crate::eternal_premodern_p1a_v1::p1a_state(state)
            .no_regeneration
            .retain(|entry| *entry != record);
    }
}

/// Drop of Honey's upkeep trigger on resolution: the creatures with the
/// least power. Those with indestructible are passed over while another of
/// them can be destroyed, and none is destroyed when all of them are
/// indestructible (its 2021-03-19 rulings).
fn honey_candidates(state: &GameState) -> Vec<ObjectId> {
    let creatures = battlefield(state)
        .filter(|&object| crate::engine::object_has_type(state, object, CardType::Creature))
        .map(|object| (object, crate::engine::effective_power(state, object)))
        .collect::<Vec<_>>();
    let Some(least) = creatures.iter().map(|&(_, power)| power).min() else {
        return Vec::new();
    };
    creatures
        .into_iter()
        .filter(|&(object, power)| {
            power == least
                && !crate::engine::has_effective_keyword(state, object, Keywords::INDESTRUCTIBLE)
        })
        .map(|(object, _)| object)
        .collect()
}

/// Drop of Honey: "When there are no creatures on the battlefield,
/// sacrifice this enchantment." A state trigger (603.8), as wave P2c's
/// Phyrexian Devourer's is: it triggers while no creature is on the
/// battlefield and not again until that instance leaves the stack. Checked
/// as triggers are collected.
fn note_honey_state(state: &mut GameState) {
    let drops =
        active_with_id(state, static_ids().drop_of_honey, DROP_OF_HONEY).collect::<Vec<_>>();
    if drops.is_empty() && read_state(state).is_none_or(|s1g| s1g.honey_triggered.is_empty()) {
        return;
    }
    if !drops.is_empty() {
        let empty = !any_creature(state);
        for drop in drops {
            let entry = link(state, drop);
            let known = read_state(state).is_some_and(|s1g| s1g.honey_triggered.contains(&entry));
            let outstanding = known && honey_trigger_outstanding(state, drop);
            if known && !outstanding {
                // 603.8: the previous instance left the stack, so the ability
                // may trigger again.
                s1g_state(state)
                    .honey_triggered
                    .retain(|seen| *seen != entry);
            }
            if empty && !outstanding {
                let controller = state.objects.get(drop).controller;
                let contract = AbilitySourceContractV4::capture(state, drop);
                crate::eternal_premodern_p1a_v1::queue_trigger(
                    state,
                    contract,
                    controller,
                    honey_sacrifice(),
                    None,
                );
                s1g_state(state).honey_triggered.push(entry);
            }
        }
    }
    // Forget incarnations that left the battlefield.
    let live = read_state(state)
        .map(|s1g| s1g.honey_triggered.clone())
        .unwrap_or_default()
        .into_iter()
        .filter(|entry| link_live_in(state, *entry, Zone::Battlefield))
        .collect::<Vec<_>>();
    if read_state(state).is_some_and(|s1g| s1g.honey_triggered != live) {
        s1g_state(state).honey_triggered = live;
    }
}

/// Whether an instance of Drop of Honey's state trigger for `drop` is
/// queued, waiting to be put on the stack, or on the stack.
fn honey_trigger_outstanding(state: &GameState, drop: ObjectId) -> bool {
    let effect = honey_sacrifice();
    state.stack.iter().any(|item| {
        item.kind == crate::state::StackItemKind::TriggeredAbility
            && item.source == drop
            && item.inline_effect.as_ref() == Some(&effect)
    }) || state
        .engine
        .pending_triggers
        .iter()
        .any(|pending| pending.source == drop && pending.effect == effect)
        || state.eternal_v1.as_ref().is_some_and(|eternal| {
            eternal
                .p1a
                .queued_triggers
                .iter()
                .any(|queued| queued.source.source == drop && queued.effect == effect)
        })
}

// ---- Alpine Moon --------------------------------------------------------------------

/// The names an Alpine Moon may choose: every nonbasic land name in the
/// executable registry, front faces and land back faces, one per name in
/// definition order. Each is a definition id and a face index. The list
/// doesn't depend on the game, so it reveals nothing hidden, and a land not
/// yet seen can be named.
pub fn alpine_name_options() -> &'static [(u16, u8)] {
    static OPTIONS: std::sync::OnceLock<Vec<(u16, u8)>> = std::sync::OnceLock::new();
    OPTIONS.get_or_init(|| {
        let mut names: Vec<&str> = Vec::new();
        let mut options = Vec::new();
        for (index, def) in CARD_DEFS.iter().enumerate() {
            if !def.is_executable() || def.is_token {
                continue;
            }
            let id = u16::try_from(index).expect("card ids fit u16");
            let front = def.is_land && !def.supertypes.contains(&crate::card_def::Supertype::Basic);
            if front && !def.object_name.is_empty() && !names.contains(&def.object_name) {
                names.push(def.object_name);
                options.push((id, 0));
            }
            if let Some(face) = &def.transform_face {
                if face.types.contains(&CardType::Land)
                    && !face.name.is_empty()
                    && !names.contains(&face.name)
                {
                    names.push(face.name);
                    options.push((id, 1));
                }
            }
        }
        options
    })
}

/// The name of Alpine Moon option `option`.
pub fn alpine_option_name(option: u16) -> Option<&'static str> {
    let &(id, face) = alpine_name_options().get(usize::from(option))?;
    let def = CARD_DEFS.get(usize::from(id))?;
    Some(if face == 1 {
        def.transform_face.as_ref()?.name
    } else {
        def.object_name
    })
}

/// The Alpine Moon that entered without a chosen name.
fn moon_needing_name(state: &GameState) -> Option<ObjectId> {
    let id = static_ids().alpine_moon?;
    battlefield(state).find(|&object| {
        let live = state.objects.get(object);
        live.card_def == id
            && live.v4.face_index == 0
            && !read_state(state).is_some_and(|s1g| {
                s1g.alpine_names
                    .iter()
                    .any(|(moon, _)| *moon == link(state, object))
            })
    })
}

/// "As this enchantment enters, choose a nonbasic land card name": the
/// pending choice, one option per name.
pub(crate) fn entry_choice_decision(state: &GameState) -> Option<crate::engine::Decision> {
    if !cfg!(feature = "eternal-fixtures") {
        return None;
    }
    let moon = moon_needing_name(state)?;
    Some(crate::engine::Decision::ChooseEffectOption {
        player: state.objects.get(moon).controller,
        source: moon,
        option_count: u16::try_from(alpine_name_options().len()).unwrap_or(u16::MAX),
    })
}

/// Answers the pending Alpine Moon choice. `Ok(false)` when none is
/// choosing.
pub(crate) fn answer_entry_choice(state: &mut GameState, option: u16) -> Result<bool, String> {
    if !cfg!(feature = "eternal-fixtures") {
        return Ok(false);
    }
    let Some(moon) = moon_needing_name(state) else {
        return Ok(false);
    };
    if usize::from(option) >= alpine_name_options().len() {
        return Err("land name option is out of range".to_string());
    }
    let entry = link(state, moon);
    let names = &mut s1g_state(state).alpine_names;
    names.retain(|(host, _)| host.object != moon);
    names.push((entry, option));
    Ok(true)
}

/// The name `moon`'s current incarnation chose, if it has.
pub fn alpine_chosen_name(state: &GameState, moon: ObjectId) -> Option<&'static str> {
    let entry = battlefield_link(state, moon)?;
    let option = read_state(state)?
        .alpine_names
        .iter()
        .find(|(host, _)| *host == entry)
        .map(|&(_, option)| option)?;
    alpine_option_name(option)
}

/// Alpine Moon: "Lands your opponents control with the chosen name lose all
/// land types and abilities, and they gain '{T}: Add one mana of any
/// color.'" The timestamp of the latest Alpine Moon whose abilities apply to
/// `object`, a land on the battlefield with that name controlled by an
/// opponent of the Moon's controller. Wave E8a's land-type setter applies
/// it: abilities are removed in layer 6 at this timestamp, and land types in
/// layer 4 unless a later setter sets them again (613.7). Whether `object`
/// is a land reads its printed face, so the query asks no layer 4 or 6
/// result.
pub(crate) fn alpine_timestamp(state: &GameState, object: ObjectId) -> Option<u64> {
    if !cfg!(feature = "eternal-fixtures") {
        return None;
    }
    let s1g = read_state(state)?;
    if s1g.alpine_names.is_empty() {
        return None;
    }
    let live = state.objects.try_get(object)?;
    if live.zone != Zone::Battlefield || live.v4.face_down_v1.is_some() {
        return None;
    }
    let def = CARD_DEFS.get(usize::from(live.card_def))?;
    if !def
        .types_for_face(live.v4.face_index)
        .contains(&CardType::Land)
    {
        return None;
    }
    let name = crate::engine::effective_name(state, object);
    if name.is_empty() {
        return None;
    }
    s1g.alpine_names
        .iter()
        .filter(|&&(moon, option)| {
            link_live_in(state, moon, Zone::Battlefield)
                && state.objects.get(moon.object).controller != live.controller
                && alpine_option_name(option).is_some_and(|named| {
                    name == named || name.split(" // ").any(|part| part == named)
                })
                && active_named(state, moon.object, ALPINE_MOON)
        })
        .map(|&(moon, _)| {
            state
                .objects
                .get(moon.object)
                .v4
                .layer_timestamp
                .unwrap_or(0)
        })
        .max()
}

// ---- Ghost Vacuum -------------------------------------------------------------------

/// The timestamp of Ghost Vacuum's "1/1 Spirit in addition to its other
/// types" and flying counter on `object`'s battlefield incarnation.
fn spirit_timestamp(state: &GameState, object: ObjectId) -> Option<u64> {
    if !cfg!(feature = "eternal-fixtures") {
        return None;
    }
    let s1g = read_state(state)?;
    if s1g.spirits.is_empty() {
        return None;
    }
    let host = battlefield_link(state, object)?;
    s1g.spirits
        .iter()
        .filter(|(entry, _)| *entry == host)
        .map(|&(_, timestamp)| timestamp)
        .max()
}

/// Ghost Vacuum's returned creatures are Spirits in addition to their other
/// types (layer 4), until a later type-setting effect sets their types again
/// (613.7): an Elk of Oko's, a creature Aura's override or a land Aura's
/// (wave S1f's list). The timestamp of that addition; the engine adds the
/// subtype while the object is a creature and checks its own type
/// replacements' timestamps against it.
pub(crate) fn spirit_type_timestamp(state: &GameState, object: ObjectId) -> Option<u64> {
    let timestamp = spirit_timestamp(state, object)?;
    crate::eternal_sideboard_s1f_v1::type_settings(state, object)
        .all(|setting| setting < timestamp)
        .then_some(timestamp)
}

/// Ghost Vacuum's returned creatures have base power and toughness 1/1 in
/// layer 7b, so counters and modifications still apply.
pub(crate) fn base_power_toughness(state: &GameState, object: ObjectId) -> Option<(i16, i16, u64)> {
    spirit_timestamp(state, object).map(|timestamp| (1, 1, timestamp))
}

/// The flying a returned creature's flying counter gives it (122.1b), while
/// no later effect removed its abilities. The engine has no flying counter
/// kind, so the counter is this module's record of the incarnation.
pub(crate) fn granted_keywords(state: &GameState, object: ObjectId) -> Keywords {
    match spirit_timestamp(state, object) {
        Some(timestamp)
            if crate::continuous_characteristics_v1::grant_survives(state, object, timestamp) =>
        {
            Keywords::FLYING
        }
        _ => Keywords::NONE,
    }
}

// ---- Queued triggers and turn bookkeeping -------------------------------------------

/// The triggers this module's cards set off in this collection, queued into
/// wave P1a's queue: Drop of Honey's state trigger.
pub(crate) fn delayed_triggers(
    state: &mut GameState,
    _events: &[CommittedEvent],
) -> Vec<PendingTrigger> {
    if cfg!(feature = "eternal-fixtures") {
        note_honey_state(state);
    }
    Vec::new()
}

/// As each turn begins the stack is empty, so the records of incarnations
/// that left their zones are dropped: Alpine Moon's names, Ghost Vacuum's
/// exiles and Spirits, and Drop of Honey's triggered state once its trigger
/// left the stack.
pub(crate) fn begin_turn(state: &mut GameState, _player: PlayerId) {
    let Some(s1g) = read_state(state) else {
        return;
    };
    if s1g.is_empty() {
        return;
    }
    let alpine_names = s1g
        .alpine_names
        .iter()
        .copied()
        .filter(|&(moon, _)| link_live_in(state, moon, Zone::Battlefield))
        .collect::<Vec<_>>();
    let vacuum_exiles = s1g
        .vacuum_exiles
        .iter()
        .copied()
        .filter(|&(vacuum, card)| {
            link_live_in(state, vacuum, Zone::Battlefield) && link_live_in(state, card, Zone::Exile)
        })
        .collect::<Vec<_>>();
    let spirits = s1g
        .spirits
        .iter()
        .copied()
        .filter(|&(entry, _)| link_live_in(state, entry, Zone::Battlefield))
        .collect::<Vec<_>>();
    let honey_triggered = s1g
        .honey_triggered
        .iter()
        .copied()
        .filter(|&entry| {
            link_live_in(state, entry, Zone::Battlefield)
                && honey_trigger_outstanding(state, entry.object)
        })
        .collect::<Vec<_>>();
    let s1g = s1g_state(state);
    s1g.alpine_names = alpine_names;
    s1g.vacuum_exiles = vacuum_exiles;
    s1g.spirits = spirits;
    s1g.honey_triggered = honey_triggered;
}

// ---- Effect leaves ------------------------------------------------------------------

/// Drives this module's choice-bearing leaf in the resumable interpreter.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &S1gOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    match op {
        S1gOpV1::HoneyUpkeepV1 => {
            // The creatures with the least power are found on resolution; the
            // trigger doesn't target (its 2021-03-19 rulings).
            let candidates = honey_candidates(state);
            match candidates.as_slice() {
                [] => Ok(false),
                [creature] => {
                    let ctx = continuation.ctx.clone();
                    destroy_unregenerable(&ctx, state, *creature);
                    Ok(false)
                }
                _ => {
                    // "You choose one of them", and no player acts between
                    // the choice and the destruction.
                    let bindings = candidates
                        .iter()
                        .map(|&object| bind(state, object))
                        .collect::<Vec<_>>();
                    let remaining = continuation.frames.clone();
                    crate::eternal_cards_v1::e3::offer(
                        continuation,
                        E3ChoiceV1 {
                            kind: E3ChoiceKindV1::S1gV1(S1gChoiceV1::HoneyDestroyV1),
                            player: continuation.ctx.controller,
                            candidates: bindings
                                .iter()
                                .map(|binding| Target::Object(binding.object))
                                .collect(),
                            bindings,
                            min: 1,
                            max: 1,
                            ordered: false,
                            path,
                            remaining,
                        },
                    );
                    Ok(true)
                }
            }
        }
        _ => {
            let ctx = continuation.ctx.clone();
            execute(op, &ctx, state);
            Ok(false)
        }
    }
}

/// Applies the answer to one of this module's selections. `Ok(true)` when it
/// staged another selection.
pub(crate) fn finish_selection(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    _choice: &E3ChoiceV1,
    kind: &S1gChoiceV1,
    bindings: Vec<EffectObjectBinding>,
) -> Result<bool, String> {
    match kind {
        S1gChoiceV1::HoneyDestroyV1 => {
            if let Some(creature) = bindings.first() {
                if binding_live(state, creature) {
                    let ctx = continuation.ctx.clone();
                    destroy_unregenerable(&ctx, state, creature.object);
                }
            }
            Ok(false)
        }
    }
}

pub(crate) fn execute(op: &S1gOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        // Staged by `drive`; the program always runs in the resumable
        // interpreter.
        S1gOpV1::HoneyUpkeepV1 => halt(state, ctx.source),
        S1gOpV1::HoneySacrificeV1 => {
            // "Sacrifice this enchantment", even if a creature is on the
            // battlefield by now (its 2021-03-19 ruling). Only its controller
            // can sacrifice it.
            let Some(source) = crate::eternal_premodern_p1a_v1::live_ability_source(ctx, state)
            else {
                return;
            };
            if state.objects.get(source).controller != ctx.controller {
                return;
            }
            event::log_sacrifice(state, source);
            event::propose_and_commit(state, ProposedEvent::zone_change(source, Zone::Graveyard));
        }
        S1gOpV1::VacuumExileV1 => {
            let spec = TargetSpec::StandardV1(StandardTargetV1::CardInAGraveyard);
            let Some(card) = legal_object_target(ctx, state, 0, spec) else {
                return;
            };
            let Some(vacuum) = ability_source_link(ctx) else {
                halt(state, ctx.source);
                return;
            };
            event::propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Exile));
            let exiled = state
                .objects
                .try_get(card)
                .is_some_and(|live| live.zone == Zone::Exile);
            if exiled {
                let card = link(state, card);
                s1g_state(state).vacuum_exiles.push((vacuum, card));
            }
        }
        S1gOpV1::VacuumReturnV1 => {
            let Some(vacuum) = ability_source_link(ctx) else {
                halt(state, ctx.source);
                return;
            };
            // The records of the sacrificed incarnation (607.2a): each card
            // still in exile that is a creature card.
            let cards = read_state(state)
                .map(|s1g| {
                    s1g.vacuum_exiles
                        .iter()
                        .filter(|(source, _)| *source == vacuum)
                        .map(|&(_, card)| card)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
                .into_iter()
                .filter(|&card| {
                    link_live_in(state, card, Zone::Exile)
                        && crate::engine::object_has_type(state, card.object, CardType::Creature)
                })
                .collect::<Vec<_>>();
            s1g_state(state)
                .vacuum_exiles
                .retain(|(source, _)| *source != vacuum);
            if cards.is_empty() {
                return;
            }
            // "Each of them is a 1/1 Spirit" with "a flying counter on it"
            // as it enters (611.2e): the incarnation each card enters as is
            // recorded first, so entry triggers and statics see it.
            let timestamp = crate::engine::next_timestamp(state);
            let entering = cards
                .iter()
                .map(|card| {
                    (
                        ObjectLinkV4 {
                            object: card.object,
                            zone_change_count: card.zone_change_count.saturating_add(1),
                        },
                        timestamp,
                    )
                })
                .collect::<Vec<_>>();
            s1g_state(state).spirits.extend(entering.iter().copied());
            event::propose_and_commit_batch(
                state,
                cards
                    .iter()
                    .map(|card| {
                        ProposedEvent::zone_change_to_battlefield_under_controller(
                            card.object,
                            ctx.controller,
                        )
                    })
                    .collect(),
            );
            // A card that went elsewhere instead keeps no record.
            let missed = entering
                .into_iter()
                .filter(|&(entry, _)| !link_live_in(state, entry, Zone::Battlefield))
                .collect::<Vec<_>>();
            if !missed.is_empty() {
                s1g_state(state)
                    .spirits
                    .retain(|record| !missed.contains(record));
            }
        }
    }
}
