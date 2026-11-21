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
//! - Boarding Action: Battle Screech's token creation; "Beam me up" is a
//!   flashback cost of mana plus Deprive's "return a creature you control to
//!   its owner's hand" component, staged by the cast pipeline's flashback
//!   branch.
//! - Kaervek's Torch: Traumatic Critique's X damage leaf; the tax on spells
//!   that target it is [`torch_target_tax`], added to the casting spell's own
//!   generic increase from its chosen targets, and [`is_torch_target`] lets
//!   the cast pipeline offer a Torch as a target only while that cast stays
//!   payable.
//! - Hallow: a shield bound to the target spell's stack incarnation, and to
//!   the permanent that spell becomes (CR 400.7c), for the rest of the turn,
//!   consumed by the damage pipeline ([`hallow_prevents`]).
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{CardDef, TargetSpec, CARD_DEFS};
use crate::effect::{EffectContinuation, EffectOp, ExecCtx, TargetRef};
use crate::eternal_cards_v1::EternalOpV1;
use crate::eternal_cube_c1c_v1::C1cOpV1;
pub mod ordinary_damage;
use crate::eternal_sideboard_s1d_v1::{statics as circles, CopShieldV1};
use crate::event::{self, DamageProposed, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::StandardOpV1;
use crate::state::{GameState, ObjectLinkV4, StackItemKind, Target, Zone};
use crate::trigger::TriggeredAbilityDef;
use ordinary_damage::{OrdinaryV1, Packet};
use serde::{Deserialize, Serialize};

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HallowPreventionV1 {
    Hallow(PlayerId),
    Circle,
    Ordinary(u16),
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
fn redirect_may_hit_nishoba(state: &GameState, damage: &DamageProposed) -> bool {
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

pub(crate) fn shielded_redirect_may_hit_nishoba(
    state: &GameState,
    damage: &DamageProposed,
) -> bool {
    if read_state(state).is_none_or(|pf| pf.hallow_shields.is_empty())
        || !redirect_may_hit_nishoba(state, damage)
    {
        return false;
    }
    damage_source_incarnation(state, damage)
        .is_some_and(|(source, zone)| !hallow_beneficiaries(state, source, zone).is_empty())
}

/// Called by the damage pipeline before the damage is dealt. If a Hallow
/// shield from this turn covers `damage`'s source, the damage is prevented
/// and the shield's controller is owed that much life, gained as soon as the
/// proposing event (or batch) finishes, so no event is committed from inside
/// the replacement pass. The shield keeps preventing damage from that spell,
/// and from the permanent it becomes, for the rest of the turn.
pub(crate) fn hallow_prevents(state: &mut GameState, damage: &DamageProposed) -> bool {
    if damage.amount <= 0 || read_state(state).is_none_or(|pf| pf.hallow_shields.is_empty()) {
        return false;
    }
    let Some((source, zone)) = damage_source_incarnation(state, damage) else {
        return false;
    };
    let beneficiaries = hallow_beneficiaries(state, source, zone);
    if beneficiaries.is_empty() {
        return false;
    }
    let cohort = redirect_may_hit_nishoba(state, damage)
        && crate::eternal_premodern_p2c_v1::statics::begin_nishoba_cohort(state);
    let mut work = vec![Packet::new(damage)];
    'packets: while let Some(packet) = work.pop() {
        let damage = &packet.damage;
        let chooser = match damage.target {
            Target::Object(object) => state.objects.get(object).controller,
            Target::Player(player) => player,
            Target::StackItem(_) => break,
        };
        let circle = circles::circle_shield_position(state, damage)
            .map(|at| state.eternal_v1.as_ref().unwrap().s1d.cop_shields[at]);
        let choice = HallowChoiceV1 {
            cohort: None,
            damage: damage.clone(),
            source,
            source_zone: zone,
            chooser,
            beneficiaries: beneficiaries.clone(),
            circle,
            ordinary: packet.options(state),
        };
        let selected = if choice.count() == 1 {
            HallowPreventionV1::Hallow(beneficiaries[0])
        } else {
            let selection = if choice.count() <= usize::from(u16::MAX) {
                crate::linked_aura_operation_v1::choose_hallow(state, choice.clone())
            } else {
                Err("Hallow replacement options exceed the native decision width".into())
            };
            match selection {
                Ok(Some(effect)) => effect,
                Ok(None) => continue 'packets,
                Err(_) => {
                    state.engine.halted = Some((
                        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                        damage.source,
                    ));
                    break;
                }
            }
        };
        match selected {
            HallowPreventionV1::Hallow(controller) => pf1c_state(state)
                .hallow_life_owed
                .push((controller, damage.amount)),
            HallowPreventionV1::Circle => {
                circles::consume_circle_shield(state, damage);
            }
            HallowPreventionV1::Ordinary(index) => packet.apply(
                state,
                choice.ordinary[usize::from(index)].clone(),
                &mut work,
            ),
        }
    }
    crate::eternal_premodern_p2c_v1::statics::end_damage_batch(state, cohort);
    true
}

/// Called at the end of `event::propose_and_commit` and
/// `event::propose_and_commit_batch`: each controller gains the life its
/// Hallow shields are owed for that event or batch, as one life-gain event
/// per controller (simultaneous damage prevented at once is one gain), in
/// the order the damage was prevented. Free when nothing is owed.
pub(crate) fn gain_hallow_life(state: &mut GameState) {
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

/// Triggered abilities of this module's cards, the tail of the trigger
/// tables.
pub(crate) fn triggers_for(_name: &str) -> &'static [TriggeredAbilityDef] {
    &[]
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
