//! Card programs for the eternal catalog's Pauper field wave Pf1c
//! (`docs/design/eternal_formats_support_v1.md`): four cards that each need a new engine piece.
//!
//! `eternal_cards_v1` forwards this module's effect leaves here, and the
//! recipe tables of `build_eternal_pf1c_v1.rs` name its programs. Every
//! table is keyed by a printed name that occurs only in the eternal registry,
//! so the Pauper, FDN and Standard catalogs never reach it. Behavior was read
//! from each card's current Oracle text and, where present, its XMage source.
//!
//! Where each card lives:
//!
//! - Fling: the additional cost is Arbiter of Woe's `SacrificeControlled`.
//!   The cast payment calls [`note_sacrifice_power`] after its mana is
//!   produced and just before it sacrifices the creature, so the power it
//!   last had on the battlefield is
//!   kept for the spell's stack incarnation (and read by its copies, which
//!   copy the cost paid for the original, 707.10).
//! - Boarding Action creates a token. Its Beam me up alternative returns a creature
//!   you control and exiles the spell when it leaves the stack.
//! - Kaervek's Torch: Traumatic Critique's X damage leaf; the tax on spells
//!   that target it is [`torch_target_tax`], added to the casting spell's own
//!   generic increase from its chosen targets, and [`is_torch_target`] lets
//!   the cast pipeline offer a Torch as a target only while that cast stays
//!   payable.
//! - Hallow: a shield bound to the target spell's stack incarnation, and to
//!   the permanent that spell becomes (CR 400.7c), for the rest of the turn,
//!   consumed by the damage pipeline ([`replace_damage`]).
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{CardDef, TargetSpec, CARD_DEFS};
use crate::effect::{EffectContinuation, EffectOp, ExecCtx, TargetRef};
use crate::eternal_cards_v1::EternalOpV1;
use crate::eternal_cube_c1c_v1::C1cOpV1;
pub mod ordinary_damage;
mod redirect_source;
use crate::eternal_sideboard_s1d_v1::{statics as circles, CopShieldV1};
use crate::event::{self, DamageProposed, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::StandardOpV1;
use crate::state::{GameState, ObjectLinkV4, StackItemKind, Target, Zone};
use crate::trigger::TriggeredAbilityDef;
use ordinary_damage::{OrdinaryV1, Packet};
use serde::{Deserialize, Serialize};

pub(crate) fn beam_me_up_cost(def: &CardDef) -> Option<&'static [crate::card_def::CostComponent]> {
    def.alt_cost
        .filter(|alt| alt.condition == crate::card_def::AltCostCondition::BeamMeUpV1)
        .map(|alt| alt.components)
}

pub(crate) const FLING: &str = "Fling";
pub(crate) const KAERVEKS_TORCH: &str = "Kaervek's Torch";

/// The generic mana a spell pays for each Kaervek's Torch it targets.
const TORCH_TAX: u32 = 2;

/// How far a chain of copies of copies is followed back to its cast spell.
const COPY_DEPTH: usize = 8;

// ---- State --------------------------------------------------------------------------

/// Per-game state for this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Default (and absent from snapshots)
/// until one of these cards needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pf1cStateV1 {
    /// Fling spells on the stack and the power their sacrificed creature
    /// last had on the battlefield.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fling_powers: Vec<FlingPowerV1>,
    /// Hallow's shields.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hallow_shields: Vec<HallowShieldV1>,
    /// Life a Hallow shield's controller gains for damage it just
    /// prevented, gained as soon as the event that proposed the damage
    /// finishes ([`gain_hallow_life`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hallow_life_owed: Vec<(PlayerId, i32)>,
}

impl Pf1cStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        self.fling_powers.is_empty()
            && self.hallow_shields.is_empty()
            && self.hallow_life_owed.is_empty()
    }
}

/// An empty state hashes to nothing, so every game without these cards keeps
/// its state hash.
impl std::hash::Hash for Pf1cStateV1 {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        if !self.is_empty() {
            b"eternal-pauper-pf1c/v1".hash(state);
            self.fling_powers.hash(state);
            self.hallow_shields.hash(state);
            self.hallow_life_owed.hash(state);
        }
    }
}

/// A cast Fling, by its stack incarnation, and the sacrificed creature's
/// last-known power.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlingPowerV1 {
    pub spell: ObjectLinkV4,
    pub power: i32,
}

/// "Prevent all damage target spell would deal this turn. You gain life
/// equal to the damage prevented this way."
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HallowShieldV1 {
    /// The target spell, by its stack incarnation.
    pub spell: ObjectLinkV4,
    /// Hallow's controller, who gains the life.
    pub controller: PlayerId,
    /// The turn the shield lasts: its round key and active player.
    pub turn: u32,
    pub active: PlayerId,
}

/// Private exact effective damage packet. Choosing a beneficiary applies one
/// of the equivalent full-prevention shields, without targeting that player.
/// General complete-prevention ordering reuses the same private binding with
/// no beneficiaries; that choice can never award Hallow life.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HallowChoiceV1 {
    /// Original batch, event and replacement-step ordinals. Private
    /// identity survives a newly revealed choice changing the APNAP frontier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cohort: Option<(u32, u32, u32)>,
    pub damage: DamageProposed,
    pub source: ObjectLinkV4,
    pub source_zone: Zone,
    pub chooser: PlayerId,
    pub beneficiaries: Vec<PlayerId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub circle: Option<CopShieldV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ordinary: Vec<OrdinaryV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redirect_sources: Option<redirect_source::SourceChoiceV1>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HallowPreventionV1 {
    Hallow(PlayerId),
    Circle,
    Ordinary(u16),
    RedirectSource(ObjectLinkV4),
}

impl HallowChoiceV1 {
    pub(crate) fn mixed(&self) -> bool {
        self.circle.is_some() || !self.ordinary.is_empty()
    }

    pub(crate) fn count(&self) -> usize {
        self.beneficiaries.len() + usize::from(self.circle.is_some()) + self.ordinary.len()
    }

    pub(crate) fn option(&self, option: u16) -> Option<HallowPreventionV1> {
        let index = usize::from(option);
        if let Some(&player) = self.beneficiaries.get(index) {
            return Some(HallowPreventionV1::Hallow(player));
        }
        let offset = index.checked_sub(self.beneficiaries.len())?;
        if offset == 0 && self.circle.is_some() {
            return Some(HallowPreventionV1::Circle);
        }
        let ordinary = offset.checked_sub(usize::from(self.circle.is_some()))?;
        self.ordinary
            .get(ordinary)
            .map(|_| HallowPreventionV1::Ordinary(ordinary as u16))
    }

    pub(crate) fn allows(&self, prevention: HallowPreventionV1) -> bool {
        match prevention {
            HallowPreventionV1::Hallow(player) => self.beneficiaries.contains(&player),
            HallowPreventionV1::Circle => self.circle.is_some(),
            HallowPreventionV1::Ordinary(index) => usize::from(index) < self.ordinary.len(),
            HallowPreventionV1::RedirectSource(source) => self
                .redirect_sources
                .as_ref()
                .is_some_and(|choice| choice.sources.contains(&source)),
        }
    }
}

/// A potential second prevention effect enables replay. The exact options
/// are determined at the effective damage packet, after redirection.
pub(crate) fn may_need_hallow_choice(state: &GameState) -> bool {
    if !cfg!(feature = "eternal-fixtures") {
        return false;
    }
    let Some(pf1c) = read_state(state) else {
        return false;
    };
    let turn = state.turn_key_v1();
    pf1c.hallow_shields
        .iter()
        .any(|shield| shield.turn == turn && shield.active == state.active_player)
}

fn hallow_beneficiaries(state: &GameState, source: ObjectLinkV4, zone: Zone) -> Vec<PlayerId> {
    let Some(pf1c) = read_state(state) else {
        return Vec::new();
    };
    [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .filter(|player| {
            pf1c.hallow_shields.iter().any(|shield| {
                shield.controller == *player
                    && shield.turn == state.turn_key_v1()
                    && shield.active == state.active_player
                    && shield_covers(shield, source, zone)
            })
        })
        .collect()
}

fn pf1c_state(state: &mut GameState) -> &mut Pf1cStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).pf1c
}

fn read_state(state: &GameState) -> Option<&Pf1cStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.pf1c)
}

/// How many Hallow shields the game holds (live or expired), for tests.
pub fn hallow_shield_count(state: &GameState) -> usize {
    read_state(state).map_or(0, |pf1c| pf1c.hallow_shields.len())
}

// ---- Effect leaves ------------------------------------------------------------------

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::Pf1cV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Pf1cOpV1 {
    /// Fling: damage equal to the sacrificed creature's power to target 0.
    FlingDamageV1,
    /// Hallow: a shield against the damage target spell 0 would deal this
    /// turn.
    HallowShieldV1,
}

impl Pf1cOpV1 {
    /// Whether the leaf may stop for a player's choice (or a draw that a
    /// replacement can interrupt), so it runs in the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        match self {
            Self::FlingDamageV1 | Self::HallowShieldV1 => false,
        }
    }
}

fn pf1c(op: Pf1cOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::Pf1cV1(op)))
}

fn definition(state: &GameState, object: ObjectId) -> Option<&'static CardDef> {
    let live = state.objects.try_get(object)?;
    CARD_DEFS.get(live.card_def as usize)
}

fn link(state: &GameState, object: ObjectId) -> Option<ObjectLinkV4> {
    state.objects.try_get(object).map(|live| ObjectLinkV4 {
        object,
        zone_change_count: live.zone_change_count,
    })
}

/// The incarnations `object` is or copies (through copies of copies), the
/// cast spell last: a copy shares the costs paid for that spell (707.10).
fn cast_ancestors(state: &GameState, object: ObjectId) -> Vec<ObjectLinkV4> {
    let mut chain = Vec::new();
    let Some(mut current) = link(state, object) else {
        return chain;
    };
    for _ in 0..COPY_DEPTH {
        chain.push(current);
        let Some(origin) = state
            .objects
            .try_get(current.object)
            .and_then(|live| live.spell_copy_origin.as_ref())
        else {
            break;
        };
        current = ObjectLinkV4 {
            object: origin.parent,
            zone_change_count: origin.parent_stack_zone_change_count,
        };
    }
    chain
}

// ---- Programs -----------------------------------------------------------------------

/// Fling: "As an additional cost to cast this spell, sacrifice a creature.
/// Fling deals damage equal to the sacrificed creature's power to any
/// target."
pub fn fling() -> EffectOp {
    pf1c(Pf1cOpV1::FlingDamageV1)
}

/// Kaervek's Torch: "Kaervek's Torch deals X damage to any target."
/// Traumatic Critique's leaf. Its tax on spells that target it is
/// [`torch_target_tax`].
pub fn kaerveks_torch() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C1cV1(
        C1cOpV1::DamageTargetXV1,
    )))
}

/// Hallow: "Prevent all damage target spell would deal this turn. You gain
/// life equal to the damage prevented this way."
pub fn hallow() -> EffectOp {
    pf1c(Pf1cOpV1::HallowShieldV1)
}

// ---- Fling --------------------------------------------------------------------------

/// Called by the cast payment after its mana abilities resolved (a Gemstone
/// Mine's last counter may have put a land into a graveyard) and just before
/// its object costs are paid, with the objects its costs chose. For a Fling,
/// records the
/// sacrificed creature's power as it last exists on the battlefield (the
/// card's ruling). Free for every other spell.
pub(crate) fn note_sacrifice_power(state: &mut GameState, spell: ObjectId, chosen: &[ObjectId]) {
    if definition(state, spell).is_none_or(|def| def.name != FLING) {
        return;
    }
    let Some(spell_link) = link(state, spell) else {
        return;
    };
    let Some(creature) = chosen.iter().copied().find(|&object| {
        state
            .objects
            .try_get(object)
            .is_some_and(|live| live.zone == Zone::Battlefield)
    }) else {
        return;
    };
    let power = crate::engine::effective_power(state, creature);
    // Keep only the records a spell or copy still on the stack can read.
    let mut live: Vec<FlingPowerV1> = read_state(state)
        .map(|pf1c| {
            pf1c.fling_powers
                .iter()
                .copied()
                .filter(|record| {
                    record.spell != spell_link
                        && state
                            .stack
                            .iter()
                            .any(|item| cast_ancestors(state, item.source).contains(&record.spell))
                })
                .collect()
        })
        .unwrap_or_default();
    live.push(FlingPowerV1 {
        spell: spell_link,
        power,
    });
    pf1c_state(state).fling_powers = live;
}

/// The power the resolving Fling (or the cast Fling a copy copies) recorded
/// for its sacrificed creature.
fn fling_power(ctx: &ExecCtx, state: &GameState) -> Option<i32> {
    let records = &read_state(state)?.fling_powers;
    cast_ancestors(state, ctx.source)
        .into_iter()
        .find_map(|ancestor| {
            records
                .iter()
                .find(|record| record.spell == ancestor)
                .map(|record| record.power)
        })
}

fn fling_damage(ctx: &ExecCtx, state: &mut GameState) {
    // A cast Fling always records its power as its cost is paid; without a
    // record there is no sacrificed creature to read, and no damage.
    let Some(power) = fling_power(ctx, state) else {
        return;
    };
    if power <= 0 || !ctx.target_incarnation_matches(0, state) {
        return;
    }
    let target = ctx.resolve_target(TargetRef::Target(0));
    event::propose_and_commit(state, event::damage_from_exec_ctx_v1(ctx, target, power));
}

// ---- Kaervek's Torch ----------------------------------------------------------------

/// Whether `object` is a Kaervek's Torch spell (or a copy of one) on the
/// stack.
fn is_torch_spell(state: &GameState, object: ObjectId) -> bool {
    state.objects.try_get(object).is_some_and(|live| {
        live.zone == Zone::Stack
            && CARD_DEFS
                .get(live.card_def as usize)
                .is_some_and(|def| def.name == KAERVEKS_TORCH && def.is_executable())
    })
}

/// Whether a Kaervek's Torch is on the stack, so spells that target it cost
/// more.
pub(crate) fn torch_on_stack(state: &GameState) -> bool {
    state
        .stack
        .iter()
        .any(|item| item.kind == StackItemKind::Spell && is_torch_spell(state, item.source))
}

/// Whether `target` is a Kaervek's Torch on the stack.
pub(crate) fn is_torch_target(state: &GameState, target: Target) -> bool {
    matches!(target, Target::Object(object) if is_torch_spell(state, object))
}

/// Kaervek's Torch: "As long as Kaervek's Torch is on the stack, spells
/// that target it cost {2} more to cast." The generic increase of a spell
/// cast with `targets`: {2} for each distinct Torch among them. Abilities
/// never reach this; it is added only to a casting spell's quote, after its
/// targets are chosen (601.2c before 601.2f).
pub(crate) fn torch_target_tax(state: &GameState, targets: &[Target]) -> u32 {
    let mut torches: Vec<ObjectId> = Vec::new();
    for target in targets {
        if let Target::Object(object) = *target {
            if !torches.contains(&object) && is_torch_spell(state, object) {
                torches.push(object);
            }
        }
    }
    u32::try_from(torches.len())
        .unwrap_or(u32::MAX)
        .saturating_mul(TORCH_TAX)
}

// ---- Hallow -------------------------------------------------------------------------

/// Hallow's resolution: if its target spell is still legal, a shield against
/// the damage that spell incarnation deals this turn.
///
/// Rules reading: CR 400.7c, "Prevention effects that apply to damage from
/// a permanent spell on the stack continue to apply to damage from the
/// permanent that spell becomes." So when the shielded spell resolves into a
/// permanent, the shield also covers damage from that permanent incarnation
/// (the battlefield incarnation right after the stack one) for the rest of
/// the turn, including damage it deals as it last existed after leaving;
/// a later incarnation (once the permanent leaves and returns) is a new
/// object and is not covered.
fn hallow_shield(ctx: &ExecCtx, state: &mut GameState) {
    let Some(Target::Object(spell)) = ctx.targets.first().copied() else {
        return;
    };
    if !ctx.target_incarnation_matches(0, state) || state.objects.get(spell).zone != Zone::Stack {
        return;
    }
    let Some(spell) = link(state, spell) else {
        return;
    };
    let turn = state.turn_key_v1();
    let active = state.active_player;
    let shields = &mut pf1c_state(state).hallow_shields;
    shields.retain(|shield| shield.turn == turn && shield.active == active);
    shields.push(HallowShieldV1 {
        spell,
        controller: ctx.controller,
        turn,
        active,
    });
}

/// The incarnation that deals `damage` and the zone it was in: its
/// historical source, else the source of the resolving ability that deals
/// it as that source last existed (113.7a), else the live object.
fn damage_source_incarnation(
    state: &GameState,
    damage: &DamageProposed,
) -> Option<(ObjectLinkV4, Zone)> {
    if let Some(facts) = damage.historical_facts(state).ok()? {
        return Some((
            ObjectLinkV4 {
                object: facts.source.source,
                zone_change_count: facts.source.zone_change_count,
            },
            facts.source.zone,
        ));
    }
    if let Some(source) = crate::eternal_premodern_p3d_v1::resolving_ability_source(state)
        .filter(|source| source.source == damage.source)
    {
        return Some((
            ObjectLinkV4 {
                object: source.source,
                zone_change_count: source.zone_change_count,
            },
            source.zone,
        ));
    }
    let live = state.objects.try_get(damage.source)?;
    Some((link(state, damage.source)?, live.zone))
}

/// Whether `shield` covers damage from `source` in `zone`: the shielded
/// spell itself, or the permanent that spell became (400.7c), the
/// battlefield incarnation right after its stack incarnation.
fn shield_covers(shield: &HallowShieldV1, source: ObjectLinkV4, zone: Zone) -> bool {
    shield.spell == source
        || (zone == Zone::Battlefield
            && source.object == shield.spell.object
            && shield.spell.zone_change_count.checked_add(1) == Some(source.zone_change_count))
}

/// A split may reach Nishoba without the original packet naming it. Reserve its
/// existing once-per-simultaneous-event counter bookkeeping for both portions.
pub(crate) fn redirect_may_hit_nishoba(state: &GameState, damage: &DamageProposed) -> bool {
    use crate::eternal_legacy_e9a_v1 as e9a;
    let Some(records) = e9a::read_state(state) else {
        return false;
    };
    !records.redirects.is_empty()
        && e9a::statics::applicable_redirects(state, damage)
            .next()
            .is_some()
        && records.redirects.iter().any(|record| {
            e9a::link_live_in(state, record.to, Zone::Battlefield)
                && e9a::active_named(
                    state,
                    record.to.object,
                    crate::eternal_premodern_p2c_v1::PHANTOM_NISHOBA,
                )
        })
}

pub(crate) fn batch_redirect_may_hit_nishoba(state: &GameState, damage: &DamageProposed) -> bool {
    redirect_may_hit_nishoba(state, damage)
        && (!crate::linked_aura_operation_v1::legacy_damage_order_v1()
            || damage_source_incarnation(state, damage).is_some_and(|(source, zone)| {
                !hallow_beneficiaries(state, source, zone).is_empty()
            }))
}

/// An unshielded redirection may split a single proposal into several
/// simultaneous survivors. Keep Hallow's established single-event checkpoint.
pub(crate) fn needs_redirect_batch(state: &GameState, damage: &DamageProposed) -> bool {
    crate::eternal_prevention_order_v1::nomads_v1::needs_frontier(state, damage)
        && damage_source_incarnation(state, damage).is_some_and(|(source, zone)| {
            crate::eternal_prevention_order_v1::cannot_prevent_damage(state, damage)
                || hallow_beneficiaries(state, source, zone).is_empty()
        })
}

/// One original damage event or one redirected portion. The interpreter's
/// operation replay retains the actual mutation prefix at every choice.
struct HallowPacketV1 {
    shared_redirect: bool,
    packet: Packet,
    event: usize,
    source: ObjectLinkV4,
    source_zone: Zone,
}

fn covered_packet(
    state: &GameState,
    damage: &DamageProposed,
    event: usize,
    shared_redirect: bool,
) -> Option<HallowPacketV1> {
    if damage.amount <= 0 {
        return None;
    }
    let (source, source_zone) = damage_source_incarnation(state, damage)?;
    if !shared_redirect
        && !crate::eternal_prevention_order_v1::nomads_v1::needs_frontier(state, damage)
        && !crate::eternal_prevention_order_v1::eligible(state, damage)
        && (crate::eternal_prevention_order_v1::cannot_prevent_damage(state, damage)
            || (hallow_beneficiaries(state, source, source_zone).is_empty()
                && !crate::eternal_prevention_order_v1::residual_order_applies(state, damage)))
    {
        return None;
    }
    Some(HallowPacketV1 {
        shared_redirect,
        packet: Packet::new(damage),
        event,
        source,
        source_zone,
    })
}

fn packet_chooser(state: &GameState, damage: &DamageProposed) -> Option<PlayerId> {
    match damage.target {
        Target::Object(object) => Some(state.objects.get(object).controller),
        Target::Player(player) => Some(player),
        Target::StackItem(_) => None,
    }
}

/// Finish each portion by prevention, zero reduction or an exact residual.
/// State-bearing effects apply immediately in choice chronology; the next frontier is built
/// from the resulting live state, never from a reordered speculative replay.
fn prevent_packets(
    state: &mut GameState,
    mut work: Vec<HallowPacketV1>,
    batch: Option<u32>,
    residuals: &mut Vec<(usize, event::DamageProposed)>,
) -> Result<bool, String> {
    let mut steps = vec![0u32; work.iter().map(|item| item.event + 1).max().unwrap_or(0)];
    while !work.is_empty() {
        let at = work
            .iter()
            .enumerate()
            .min_by_key(|(at, item)| {
                let chooser = packet_chooser(state, &item.packet.damage);
                (
                    usize::from(chooser != Some(state.active_player)),
                    item.event,
                    *at,
                )
            })
            .map(|(at, _)| at)
            .expect("nonempty Hallow frontier");
        let at = if let Some(batch) = batch {
            let Some(selected) =
                redirect_source::choose(state, &work, at, batch, steps[work[at].event])?
            else {
                return Ok(false);
            };
            selected
        } else {
            at
        };
        let item = work.remove(at);
        let damage = &item.packet.damage;
        if damage.amount <= 0 {
            continue;
        }
        let chooser =
            packet_chooser(state, damage).ok_or("Hallow packet has no affected player")?;
        let beneficiaries =
            if crate::eternal_prevention_order_v1::cannot_prevent_damage(state, damage) {
                Vec::new()
            } else {
                hallow_beneficiaries(state, item.source, item.source_zone)
            };
        let ordinary_only = beneficiaries.is_empty();
        let (circle, ordinary) = if ordinary_only {
            let competing_source = item.shared_redirect
                && work.iter().any(|other| {
                    other.shared_redirect
                        && other.packet.damage.target == damage.target
                        && other.source != item.source
                });
            (
                None,
                crate::eternal_prevention_order_v1::packet_options(
                    state,
                    &item.packet,
                    competing_source,
                ),
            )
        } else {
            let mut ordinary = item.packet.options(state);
            let mut matching = circles::matching_circle_shields(state, damage);
            let first = matching.next().map(|(_, shield)| shield);
            let mut distinct = Vec::new();
            for (position, shield) in
                matching.take_while(|_| !crate::linked_aura_operation_v1::legacy_damage_order_v1())
            {
                if Some(shield) != first && !distinct.contains(&shield) {
                    distinct.push(shield);
                    ordinary.push(OrdinaryV1::Circle { position, shield });
                }
            }
            (first, ordinary)
        };
        let step = steps[item.event];
        steps[item.event] = step
            .checked_add(1)
            .ok_or("Hallow packet step ordinal overflow")?;
        let choice = HallowChoiceV1 {
            cohort: batch.map(|batch| {
                (
                    batch,
                    u32::try_from(item.event).expect("batch event ordinal fits u32"),
                    step,
                )
            }),
            damage: damage.clone(),
            source: item.source,
            source_zone: item.source_zone,
            chooser,
            beneficiaries,
            circle,
            ordinary,
            redirect_sources: None,
        };
        let selected = if choice.count() == 0 {
            residuals.push((item.event, damage.clone()));
            continue;
        } else if choice.count() == 1 {
            if ordinary_only {
                HallowPreventionV1::Ordinary(0)
            } else {
                HallowPreventionV1::Hallow(choice.beneficiaries[0])
            }
        } else {
            if choice.count() > usize::from(u16::MAX) {
                return Err("Hallow replacement options exceed the native decision width".into());
            }
            let Some(effect) =
                crate::linked_aura_operation_v1::choose_hallow(state, choice.clone())?
            else {
                return Ok(false);
            };
            effect
        };
        match selected {
            HallowPreventionV1::RedirectSource(_) => {
                return Err("unexpected source in a packet effect choice".into())
            }
            HallowPreventionV1::Hallow(controller) => pf1c_state(state)
                .hallow_life_owed
                .push((controller, damage.amount)),
            HallowPreventionV1::Circle => {
                circles::consume_circle_shield(state, damage);
            }
            HallowPreventionV1::Ordinary(index) => {
                let original_target = item.packet.damage.target;
                let mut children = Vec::new();
                item.packet.apply(
                    state,
                    choice.ordinary[usize::from(index)].clone(),
                    &mut children,
                );
                work.splice(
                    at..at,
                    children.into_iter().rev().map(|packet| HallowPacketV1 {
                        shared_redirect: item.shared_redirect
                            && packet.damage.target == original_target,
                        packet,
                        event: item.event,
                        source: item.source,
                        source_zone: item.source_zone,
                    }),
                );
            }
        }
    }
    Ok(true)
}

/// Whether this batch needs Hallow's scoped replacement phase. Its caller
/// owns the Nishoba cohort across this phase and all remaining ordinary events.
pub(crate) fn batch_has_covered_damage(state: &GameState, events: &[ProposedEvent]) -> bool {
    let legacy = crate::linked_aura_operation_v1::legacy_damage_order_v1();
    (may_need_hallow_choice(state)
        || (!legacy && crate::eternal_prevention_order_v1::may_need_choice(state)))
        && !(legacy && crate::eternal_prevention_order_v1::cannot_prevent(state))
        && (!redirect_source::shared_targets(state, events).is_empty()
            || events.iter().any(|event| {
                matches!(event,
                ProposedEvent::Damage(damage) if covered_packet(state, damage, 0, false).is_some())
            }))
}

/// Process all covered portions of one simultaneous batch as a live frontier.
/// Covered portions either finish prevention or return a typed residual. The
/// remaining events retain the ordinary replacement/commit path.
pub(crate) fn prevent_hallow_batch(
    state: &mut GameState,
    events: &mut Vec<ProposedEvent>,
    batch: u32,
) -> Result<crate::eternal_prevention_order_v1::BatchDamageResultV1, String> {
    use crate::eternal_prevention_order_v1::{
        BatchDamageResultV1 as ResultV1, PreparedDamageBatchV1,
    };
    let mut work = Vec::new();
    let mut covered = vec![false; events.len()];
    let shared_targets = redirect_source::shared_targets(state, events);
    for (at, event) in events.iter().enumerate() {
        if let ProposedEvent::Damage(damage) = event {
            if let Some(packet) =
                covered_packet(state, damage, at, shared_targets.contains(&damage.target))
            {
                if u32::try_from(at).is_err() || !event::preflight_hallow_damage_v1(state, event) {
                    return Err("damage cohort rejected its original historical damage".into());
                }
                work.push(packet);
                covered[at] = true;
            }
        }
    }
    if work.is_empty() {
        return Ok(ResultV1::Complete(PreparedDamageBatchV1::default()));
    }
    let mut residuals = Vec::new();
    if !prevent_packets(state, work, Some(batch), &mut residuals)? {
        return Ok(ResultV1::Suspended);
    }
    let mut prepared = Vec::new();
    let mut next = Vec::new();
    for (at, event) in std::mem::take(events).into_iter().enumerate() {
        if !covered[at] {
            next.push(event);
            prepared.push(None);
        } else {
            for (_, damage) in residuals.iter().filter(|(original, _)| *original == at) {
                next.push(ProposedEvent::Damage(damage.clone()));
                prepared.push(Some(damage.clone()));
            }
        }
    }
    *events = next;
    Ok(ResultV1::Complete(PreparedDamageBatchV1::new(prepared)))
}

/// The operation owns the complete chosen mutation prefix. A surviving packet
/// returns to the normal commit path with its replacements already finished.
pub(crate) fn replace_damage(
    state: &mut GameState,
    damage: &event::DamageProposed,
) -> crate::eternal_prevention_order_v1::DamageResultV1 {
    use crate::eternal_prevention_order_v1::DamageResultV1;
    let Some(packet) = covered_packet(state, damage, 0, false) else {
        return DamageResultV1::Uncovered;
    };
    let cohort = redirect_may_hit_nishoba(state, damage)
        && crate::eternal_premodern_p2c_v1::statics::begin_nishoba_cohort(state);
    let mut residuals = Vec::new();
    let result = prevent_packets(state, vec![packet], None, &mut residuals);
    crate::eternal_premodern_p2c_v1::statics::end_damage_batch(state, cohort);
    if result.is_err() || residuals.len() > 1 {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            damage.source,
        ));
        return DamageResultV1::Prevented;
    }
    match residuals.pop() {
        Some((_, damage)) => DamageResultV1::Residual(damage),
        None => DamageResultV1::Prevented,
    }
}

/// Called at the end of `event::propose_and_commit` and
/// `event::propose_and_commit_batch`: each controller gains the life its
/// Hallow shields are owed for that event or batch, as one life-gain event
/// per controller (simultaneous damage prevented at once is one gain), in
/// the order the damage was prevented. Free when nothing is owed.
pub(crate) fn gain_hallow_life(state: &mut GameState) {
    // Direct Event/Batch wrappers may return a public saved checkpoint. Its
    // original operation, not this already-returned wrapper, owns the gain.
    if crate::linked_aura_operation_v1::has_pending(state) {
        return;
    }
    let owed = match state.eternal_v1.as_mut() {
        Some(eternal) if !eternal.pf1c.hallow_life_owed.is_empty() => {
            std::mem::take(&mut eternal.pf1c.hallow_life_owed)
        }
        _ => return,
    };
    if state.engine.halted.is_some() {
        return;
    }
    let mut totals: Vec<(PlayerId, i32)> = Vec::new();
    for (controller, amount) in owed {
        match totals.iter_mut().find(|(player, _)| *player == controller) {
            Some((_, total)) => *total = total.saturating_add(amount),
            None => totals.push((controller, amount)),
        }
    }
    for (controller, amount) in totals {
        event::propose_and_commit(state, ProposedEvent::life_gain(controller, amount));
    }
}

// ---- Triggered abilities ------------------------------------------------------------

/// Triggered abilities of this module's cards, falling through to wave
/// C7a.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    crate::eternal_cube_c7a_v1::triggers_for(name)
}

/// Target specs of this module's printed and queued triggered abilities.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    delayed_effect_matches(name, effect).then_some(TargetSpec::None)
}

/// Queued triggers owned by this module's cards.
pub(crate) fn delayed_effect_matches(_name: &str, _effect: &EffectOp) -> bool {
    false
}

// ---- Execution ----------------------------------------------------------------------

/// Runs one of this module's leaves in synchronous execution.
pub(crate) fn execute(op: &Pf1cOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        Pf1cOpV1::FlingDamageV1 => fling_damage(ctx, state),
        Pf1cOpV1::HallowShieldV1 => hallow_shield(ctx, state),
    }
}

/// Runs one of this module's leaves in the resumable interpreter. None of
/// them stops for a choice.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &Pf1cOpV1,
    _path: Vec<u16>,
) -> Result<bool, String> {
    execute(op, &continuation.ctx, state);
    Ok(false)
}
