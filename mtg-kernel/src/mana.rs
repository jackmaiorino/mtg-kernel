//! EXACT backtracking mana payment solver.
//!
//! `Cost` describes a spell/ability's mana requirement as a set of pips
//! (colored / hybrid / phyrexian) plus a generic amount. `solve` finds a
//! `PaymentPlan` -- which floating pool mana and which untapped mana
//! sources pay for which part of the cost -- by exact backtracking over
//! pip/source assignment, not a greedy heuristic. See
//! `backtracking_is_required_for_modal_sources` below for a case greedy
//! gets wrong: a dual source assigned to the wrong pip first can strand a
//! later pip only that dual could pay.

use crate::ids::{ObjectId, PlayerId};
use crate::state::GameState;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ManaColor {
    W,
    U,
    B,
    R,
    G,
    C,
}

impl ManaColor {
    pub const ALL: [ManaColor; 6] = [
        ManaColor::W,
        ManaColor::U,
        ManaColor::B,
        ManaColor::R,
        ManaColor::G,
        ManaColor::C,
    ];

    /// Index into `PlayerState::mana_pool` ([W, U, B, R, G, C]).
    pub fn pool_index(self) -> usize {
        match self {
            ManaColor::W => 0,
            ManaColor::U => 1,
            ManaColor::B => 2,
            ManaColor::R => 3,
            ManaColor::G => 4,
            ManaColor::C => 5,
        }
    }

    pub fn from_code(c: &str) -> Option<ManaColor> {
        match c {
            "W" => Some(ManaColor::W),
            "U" => Some(ManaColor::U),
            "B" => Some(ManaColor::B),
            "R" => Some(ManaColor::R),
            "G" => Some(ManaColor::G),
            "C" => Some(ManaColor::C),
            _ => None,
        }
    }
}

/// Fixed-capacity, non-allocating set of mana colors. Bounded by
/// `ManaColor::ALL.len()`, which every printed mana ability stays within, so
/// capacity is never exceeded in practice. Exists so hot paths that must not
/// touch the heap (such as flat-action validation) can compute the same
/// small color sets that `CardDef::primary_mana_ability_choices` and
/// `engine::available_mana_ability_choices` return as an owned `Vec`
/// elsewhere.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ManaColorSetV1 {
    colors: [ManaColor; ManaColorSetV1::CAPACITY],
    len: u8,
}

impl ManaColorSetV1 {
    const CAPACITY: usize = ManaColor::ALL.len();

    pub(crate) fn new() -> Self {
        Self {
            colors: [ManaColor::W; ManaColorSetV1::CAPACITY],
            len: 0,
        }
    }

    /// Appends `color`. Every real caller stays within `CAPACITY` colors
    /// (see the type doc); the bounds guard only prevents a panic if that
    /// invariant is ever violated instead of silently corrupting state.
    pub(crate) fn push(&mut self, color: ManaColor) {
        if usize::from(self.len) < ManaColorSetV1::CAPACITY {
            self.colors[usize::from(self.len)] = color;
            self.len += 1;
        }
    }

    pub(crate) fn as_slice(&self) -> &[ManaColor] {
        &self.colors[..usize::from(self.len)]
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub(crate) fn len(&self) -> usize {
        usize::from(self.len)
    }

    pub(crate) fn contains(&self, color: ManaColor) -> bool {
        self.as_slice().contains(&color)
    }
}

/// Deterministic policy for spending already-floating mana on generic
/// requirements. Magic permits any color, so the choice is strategically
/// observable whenever multiple colors remain. XMage's `ManaCostImpl`
/// checks colorless, black, blue, white, green, then red; matching that
/// order keeps automatic kernel payments aligned with the reference runner
/// and, importantly for Rally, preserves red before green when possible.
const GENERIC_POOL_PAYMENT_ORDER: [ManaColor; 6] = [
    ManaColor::C,
    ManaColor::B,
    ManaColor::U,
    ManaColor::W,
    ManaColor::G,
    ManaColor::R,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum Pip {
    Colored(ManaColor),
    Hybrid(ManaColor, ManaColor),
    Phyrexian(ManaColor),
}

/// A spell/ability's mana requirement. `pips` is `'static` because every
/// `Cost` this increment constructs comes from the generated `CARD_DEFS`
/// table (see `card_def.rs` / `build.rs`); nothing here prevents a future
/// increment adding an owned-slice variant for runtime-built costs (e.g. an
/// alternative cost).
/// Derives `Serialize` so the diagnostic full-state hash includes a pending
/// cast's exact override. It deliberately does not derive `Deserialize`:
/// rebuilding a `&'static [Pip]` requires registry ownership that the current
/// snapshot API does not need.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct Cost {
    pub pips: &'static [Pip],
    pub generic: u8,
    pub x_count: u8,
}

impl Cost {
    pub const fn zero() -> Cost {
        Cost {
            pips: &[],
            generic: 0,
            x_count: 0,
        }
    }
}

/// An untapped mana-producing permanent available to pay a cost. `choices`
/// lists every color it could produce (basics have exactly one; this shape
/// is what makes multi-choice producers -- and therefore backtracking --
/// meaningful).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManaSource {
    pub id: ObjectId,
    pub choices: Vec<ManaColor>,
    /// How many mana of the chosen color one activation of this source
    /// adds. One for every printed mana ability in the pool except the
    /// three Urza lands, whose yield is 3 (Tower) or 2 (Mine, Power Plant)
    /// while their controller has Tron assembled. `gather_sources` samples
    /// it from `CardDef::conditional_tap_yield` at the moment the plan is
    /// solved; every other construction site (including the test helper
    /// below) uses one, which makes this field behavior-neutral for every
    /// pre-existing source. Clamped to `1..=8` at the sampling site so an
    /// evaluator fault can never make a source free or unboundedly rich.
    pub yield_per_tap: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PaymentPlan {
    /// Newly tapped sources and the color each was tapped for, in the order
    /// they were committed (provenance).
    pub taps: Vec<(ObjectId, ManaColor)>,
    /// How much of the already-floating pool was spent, by color ([W, U,
    /// B, R, G, C], matching `PlayerState::mana_pool`).
    pub pool_used: [u8; 6],
    /// Life paid for phyrexian pips.
    pub life_paid: i32,
    /// Mana this plan's taps added beyond what their own tap paid for, by
    /// color. Always `[0; 6]` for a plan built only from `yield_per_tap ==
    /// 1` sources, which is every source in the pool except assembled Tron,
    /// so existing plans are bit-identical to their pre-yield shape.
    ///
    /// Accounting contract, relied on by `engine::pay_plan`. Writing
    /// `taps(c)` for the number of taps whose color is `c`:
    ///
    /// * the taps add `taps(c) + surplus[c]` mana of color `c` in total
    ///   (one "primary" mana per tap plus whatever the extra yield credited
    ///   here);
    /// * the cost consumes `taps(c) + pool_used[c]` of color `c` (each
    ///   tap's primary mana, plus every unit drawn out of the running
    ///   pool, whether that unit was already floating or was credited by an
    ///   earlier tap in this same plan);
    /// * so the net effect on the controller's floating pool is exactly
    ///   `surplus[c] - pool_used[c]`, and `pool_used[c]` never exceeds
    ///   `floating[c] + surplus[c]` because the running pool is seeded with
    ///   the former and credited only the latter.
    ///
    /// A tap whose extra yield is consumed inside the same payment (an
    /// assembled Tower paying a three-generic spell by itself) therefore
    /// records no surplus: the mana was added and spent atomically and
    /// never reaches the pool.
    pub surplus: [u8; 6],
}

/// Parallel source alternatives may refer to the same physical permanent:
/// one can convoke or activate its mana ability, but cannot do both.
struct SpellSourceChoicesV1 {
    aliases: Vec<ObjectId>,
    convoke: Vec<bool>,
}

fn set_spell_source_used_v1(
    used: &mut [bool],
    index: usize,
    value: bool,
    choices: Option<&SpellSourceChoicesV1>,
) {
    if let Some(choices) = choices {
        for (other, alias) in choices.aliases.iter().enumerate() {
            if *alias == choices.aliases[index] {
                used[other] = value;
            }
        }
    } else {
        used[index] = value;
    }
}

/// Rule 119.4: a player may pay life only if their life total is at least
/// the amount paid, and paying zero life is always allowed. The naive
/// `life_paid <= life` comparison wrongly rejected every zero-life payment
/// plan once a player's life went negative mid-resolution (Chain Lightning's
/// copy payment after lethal damage, before state-based actions), which made
/// the kernel finish resolution and reach lethal SBA while XMage was still
/// waiting on the copy retarget: the dominant CP7 panel void class.
pub(crate) fn life_payment_affordable(life_paid: i32, life: i32) -> bool {
    life_paid == 0 || life_paid <= life
}

/// Gathers `player`'s floating pool and untapped mana sources from `state`
/// and calls `solve`.
pub fn can_pay(
    cost: &Cost,
    x_value: u8,
    player: PlayerId,
    state: &GameState,
) -> Option<PaymentPlan> {
    can_pay_spell(cost, x_value, player, state, false)
}

/// `can_pay` for the total cost of a spell. While `creature_spell` holds,
/// mana restricted to creature spells (`CardDef::restricted_mana_abilities`,
/// Rockface Village) joins its source's choices. That mana is never floated,
/// so it can only ever be spent inside the payment it was produced for.
pub fn can_pay_spell(
    cost: &Cost,
    x_value: u8,
    player: PlayerId,
    state: &GameState,
    creature_spell: bool,
) -> Option<PaymentPlan> {
    let sources = gather_sources_for_spell(player, state, creature_spell);
    let pool = state.players[player.index()].mana_pool;
    solve(cost, x_value, pool, &sources)
        .filter(|plan| life_payment_affordable(plan.life_paid, state.players[player.index()].life))
}

/// Owned-pip counterpart to `can_pay` for serialized resolution-time costs.
/// Card definitions keep `Cost` pips static, while a suspended effect owns
/// its color vector and therefore borrows it only for this solver call.
pub fn can_pay_owned(
    pips: &[Pip],
    generic: u8,
    player: PlayerId,
    state: &GameState,
) -> Option<PaymentPlan> {
    let sources = gather_sources(player, state);
    let pool = state.players[player.index()].mana_pool;
    let mut plan = PaymentPlan::default();
    let mut pool_remaining = pool;
    let mut used = vec![false; sources.len()];
    if !solve_pips(pips, 0, &sources, &mut used, &mut pool_remaining, &mut plan)
        || !pay_generic(
            u32::from(generic),
            &sources,
            &mut used,
            &mut pool_remaining,
            &mut plan,
        )
    {
        return None;
    }
    life_payment_affordable(plan.life_paid, state.players[player.index()].life).then_some(plan)
}

/// Like `can_pay`, but checks/solves 2+ costs *together* against the same
/// pool of mana sources -- Goblin Bushwhacker's base cost + its optional
/// Kicker cost, paid as one combined announcement (601.2b/f), never as two
/// independent affordability checks that could double-count a source. All
/// `pips` are concatenated (order doesn't matter to the solver) and
/// `generic`/`x_count` are summed.
pub fn can_pay_combined(
    costs: &[&Cost],
    x_value: u8,
    player: PlayerId,
    state: &GameState,
) -> Option<PaymentPlan> {
    can_pay_combined_spell(costs, x_value, player, state, false)
}

/// `can_pay_combined` for a spell's total cost; see `can_pay_spell`.
pub fn can_pay_combined_spell(
    costs: &[&Cost],
    x_value: u8,
    player: PlayerId,
    state: &GameState,
    creature_spell: bool,
) -> Option<PaymentPlan> {
    can_pay_combined_spell_with_generic_modifiers_v1(
        costs,
        x_value,
        player,
        state,
        creature_spell,
        0,
        0,
    )
}

/// Determine a spell's complete mana payment after total-cost increases and
/// reductions (CR 601.2f). Callers supply every selected base/additional mana
/// cost together. X contributes before the final generic floor, while colored,
/// hybrid and Phyrexian pips retain their ordinary payment requirements.
/// This does not apply spell modifiers to activation or resolution payments.
pub(crate) fn can_pay_combined_spell_with_generic_modifiers_v1(
    costs: &[&Cost],
    x_value: u8,
    player: PlayerId,
    state: &GameState,
    creature_spell: bool,
    generic_increase: u32,
    generic_reduction: u32,
) -> Option<PaymentPlan> {
    let combined_pips: Vec<Pip> = costs.iter().flat_map(|c| c.pips.iter().copied()).collect();
    let generic: u32 = costs.iter().map(|c| c.generic as u32).sum();
    let x_count: u32 = costs.iter().map(|c| c.x_count as u32).sum();
    plan_spell_mana_total_v1(
        &combined_pips,
        (generic + x_count * u32::from(x_value))
            .saturating_add(generic_increase)
            .saturating_sub(generic_reduction),
        player,
        state,
        creature_spell,
        &[],
        0,
    )
}

/// Solve an already determined spell mana total. Alternate mana payments
/// remove requirements before this call; reserved tap-cost objects cannot
/// also produce mana, and all additional life costs share one life budget.
#[allow(clippy::too_many_arguments)]
pub(crate) fn plan_spell_mana_total_v1(
    pips: &[Pip],
    generic: u32,
    player: PlayerId,
    state: &GameState,
    creature_spell: bool,
    excluded: &[ObjectId],
    additional_life: u32,
) -> Option<PaymentPlan> {
    let life = i64::from(state.players[player.index()].life);
    if additional_life != 0 && i64::from(additional_life) > life {
        return None;
    }
    let mana_life_budget = (life - i64::from(additional_life)).max(0);
    let sources = gather_sources_for_spell(player, state, creature_spell)
        .into_iter()
        .filter(|source| !excluded.contains(&source.id))
        .collect::<Vec<_>>();
    let pool = state.players[player.index()].mana_pool;

    let mut plan = PaymentPlan::default();
    let mut pool_remaining = pool;
    let mut used = vec![false; sources.len()];
    if !solve_pips_with_life_budget_v1(
        pips,
        0,
        &sources,
        &mut used,
        &mut pool_remaining,
        &mut plan,
        mana_life_budget,
        Some(generic),
        None,
    ) {
        return None;
    }
    let total_life = i64::from(plan.life_paid) + i64::from(additional_life);
    (total_life == 0 || total_life <= i64::from(state.players[player.index()].life)).then_some(plan)
}

/// Solve a determined spell total with Convoke and ordinary mana together.
/// Planning does not tap creatures or produce mana. The returned tap lists
/// distinguish Convoke payment from mana-ability activation.
#[allow(clippy::too_many_arguments)]
pub(crate) fn plan_spell_mana_total_with_convoke_v1(
    pips: &[Pip],
    generic: u32,
    player: PlayerId,
    state: &GameState,
    creature_spell: bool,
    excluded: &[ObjectId],
    additional_life: u32,
) -> Option<(PaymentPlan, Vec<ObjectId>)> {
    let life = i64::from(state.players[player.index()].life);
    if additional_life != 0 && i64::from(additional_life) > life {
        return None;
    }
    let mut creatures = state.players[player.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|id| {
            let object = state.objects.get(*id);
            object.controller == player
                && object.zone == crate::state::Zone::Battlefield
                && !object.tapped
                && !excluded.contains(id)
                && crate::engine::object_has_type(state, *id, crate::card_def::CardType::Creature)
        })
        .collect::<Vec<_>>();
    creatures.sort_by_key(|id| !state.objects.get(*id).summoning_sick);
    let mut sources = Vec::new();
    let mut source_choices = SpellSourceChoicesV1 {
        aliases: Vec::new(),
        convoke: Vec::new(),
    };
    for creature in creatures {
        let colors = state.objects.get(creature).v4.effective_color_mask;
        // Colorless is not a creature color. A colorless creature can pay
        // generic via Convoke, but cannot pay a {C} pip.
        let choices = [
            ManaColor::W,
            ManaColor::U,
            ManaColor::B,
            ManaColor::R,
            ManaColor::G,
        ]
        .into_iter()
        .filter(|color| colors & crate::card_def::mana_color_mask(*color) != 0)
        .collect();
        sources.push(ManaSource {
            id: ObjectId(u32::try_from(sources.len()).ok()?),
            choices,
            yield_per_tap: 1,
        });
        source_choices.aliases.push(creature);
        source_choices.convoke.push(true);
    }
    for mut source in gather_sources_for_spell(player, state, creature_spell) {
        if excluded.contains(&source.id) {
            continue;
        }
        source_choices.aliases.push(source.id);
        source_choices.convoke.push(false);
        // These local indices are planner tokens only. Restore physical IDs
        // before returning a PaymentPlan to the engine.
        source.id = ObjectId(u32::try_from(sources.len()).ok()?);
        sources.push(source);
    }
    let mut plan = PaymentPlan::default();
    let mut pool = state.players[player.index()].mana_pool;
    let mut used = vec![false; sources.len()];
    if !solve_pips_with_life_budget_v1(
        pips,
        0,
        &sources,
        &mut used,
        &mut pool,
        &mut plan,
        (life - i64::from(additional_life)).max(0),
        Some(generic),
        Some(&source_choices),
    ) {
        return None;
    }
    let mut convoked = Vec::new();
    plan.taps.retain_mut(|(id, _)| {
        let index = id.0 as usize;
        *id = source_choices.aliases[index];
        if source_choices.convoke[index] {
            convoked.push(*id);
            false
        } else {
            true
        }
    });
    Some((plan, convoked))
}

/// Delve (702.65a): "For each generic mana in this spell's total cost, you
/// may exile a card from your graveyard rather than pay that mana." Delve
/// is not an alternative or additional cost -- it applies to `cost`'s total
/// generic amount exactly as `can_pay` would see it (after any
/// `CardDef::generic_cost_reduction` the caller already folded in), and
/// exiling is simply another way to pay, atomic with the rest of the mana
/// payment (matching XMage's `DelveAbility`/`AlternateManaPaymentAbility`,
/// not `CostComponent::ExileOtherCardsFromOwnGraveyard`'s separate-component
/// shape used by Escape).
///
/// Mana payment plans are already an opaque, engine-chosen detail with no
/// dedicated RL decision (`can_pay`/`solve` pick one deterministic plan, not
/// a menu the policy selects from) -- delve counts are chosen the same way,
/// deterministically, rather than opening a new decision/action kind:
///
/// Enumerates `k` (cards exiled) from `0` to `min(graveyard size, printed
/// generic)` in ascending order and returns the first plan whose remaining
/// cost -- every colored/hybrid/phyrexian pip untouched, generic reduced by
/// `k` -- is payable from ordinary mana sources. This prefers paying with
/// mana over the graveyard (a `k=0` plan wins whenever the printed cost is
/// already affordable outright) and only increases `k` when a smaller one
/// can't be paid, so a spell is offered as castable via the *smallest*
/// delve count that makes it affordable -- never a larger, unnecessary one.
///
/// The `k` cards actually exiled are always the *oldest* `k` entries of the
/// caster's graveyard (index `0..k`, i.e. `PlayerState::graveyard`'s
/// existing push-order/insertion order -- the first card that entered the
/// graveyard is exiled first). This fixed, card-name-free order keeps the
/// plan space to exactly one candidate per `k` instead of `C(graveyard, k)`,
/// and is deterministic and reproducible across an offer-time affordability
/// check and the later payment-time re-derivation (both call this function
/// against the live graveyard, same as every other cast cost in this
/// engine).
pub fn delve_payment_plan(
    cost: &Cost,
    x_value: u8,
    player: PlayerId,
    state: &GameState,
    creature_spell: bool,
) -> Option<(PaymentPlan, Vec<ObjectId>)> {
    let graveyard = &state.players[player.index()].graveyard;
    let max_k = graveyard.len().min(usize::from(cost.generic));
    for k in 0..=max_k {
        let reduced = Cost {
            pips: cost.pips,
            generic: cost.generic - k as u8,
            x_count: cost.x_count,
        };
        if let Some(plan) = can_pay_spell(&reduced, x_value, player, state, creature_spell) {
            return Some((plan, graveyard[..k].to_vec()));
        }
    }
    None
}

/// Lowest and highest per-tap yield the planner will admit. One keeps a
/// source from ever being free; eight bounds the pool arithmetic (the pool
/// is `[u8; 6]`) well below overflow for any realistic board, and is far
/// above the largest printed yield in this pool (3, an assembled Urza's
/// Tower).
const MIN_YIELD_PER_TAP: i32 = 1;
const MAX_YIELD_PER_TAP: i32 = 8;

/// Samples `CardDef::conditional_tap_yield` for a source whose controller is
/// `controller`. `None` (every card but the three Urza lands) is one, which
/// is the pre-existing behavior of every mana source in the pool.
fn conditional_tap_yield(
    def: &crate::card_def::CardDef,
    controller: PlayerId,
    state: &GameState,
) -> u8 {
    let Some(value) = def.conditional_tap_yield else {
        return 1;
    };
    let evaluated = crate::engine::evaluate_dynamic_value(state, value, controller);
    evaluated.clamp(MIN_YIELD_PER_TAP, MAX_YIELD_PER_TAP) as u8
}

pub fn gather_sources(player: PlayerId, state: &GameState) -> Vec<ManaSource> {
    gather_sources_for_spell(player, state, false)
}

/// `gather_sources`, adding creature-spell-only mana when `creature_spell`.
pub fn gather_sources_for_spell(
    player: PlayerId,
    state: &GameState,
    creature_spell: bool,
) -> Vec<ManaSource> {
    let mut sources = Vec::new();
    for &id in &state.players[player.index()].battlefield {
        let obj = state.objects.get(id);
        if obj.tapped || !crate::continuous_characteristics_v1::printed_abilities_active(state, id)
        {
            continue;
        }
        let def = &crate::card_def::CARD_DEFS[obj.card_def as usize];
        // `produces_mana` also covers one-shot production such as
        // Burning-Tree Emissary's ETB trigger. The generated
        // primary mana-choice contract is the narrower repeatable tap-source
        // contract, including an incarnation-local chosen color. A creature
        // source also obeys summoning sickness for the tap symbol in its
        // activation cost.
        if crate::engine::object_has_type(state, id, crate::card_def::CardType::Creature)
            && obj.summoning_sick
        {
            continue;
        }
        let mut choices = if def.is_automatic_payment_mana_source() {
            def.primary_mana_ability_choices(obj.v4.chosen_color)
        } else {
            Vec::new()
        };
        if creature_spell {
            for restricted in def.restricted_mana_abilities {
                match restricted.restriction {
                    crate::card_def::ManaSpendRestrictionDef::CreatureSpell => {}
                }
                for &color in restricted.colors {
                    if !choices.contains(&color) {
                        choices.push(color);
                    }
                }
            }
        }
        if !choices.is_empty() {
            sources.push(ManaSource {
                id,
                choices,
                yield_per_tap: conditional_tap_yield(def, obj.controller, state),
            });
        }
    }
    sources
}

/// Solves a mana cost while reserving permanents for other components of the
/// same cost. Heap Gate uses this to reserve both itself and the other Gate
/// that must remain untapped until the complete cost is paid.
pub fn can_pay_excluding_sources(
    cost: &Cost,
    x_value: u8,
    player: PlayerId,
    state: &GameState,
    excluded: &[ObjectId],
) -> Option<PaymentPlan> {
    let sources = gather_sources(player, state)
        .into_iter()
        .filter(|source| !excluded.contains(&source.id))
        .collect::<Vec<_>>();
    solve(
        cost,
        x_value,
        state.players[player.index()].mana_pool,
        &sources,
    )
    .filter(|plan| life_payment_affordable(plan.life_paid, state.players[player.index()].life))
}

/// One-source convenience wrapper for paid mana abilities whose own tap cost
/// must not also pay their mana component.
pub fn can_pay_excluding_source(
    cost: &Cost,
    x_value: u8,
    player: PlayerId,
    state: &GameState,
    excluded: ObjectId,
) -> Option<PaymentPlan> {
    can_pay_excluding_sources(cost, x_value, player, state, &[excluded])
}

/// Exact backtracking solve. Colored/hybrid/phyrexian pips are satisfied
/// first (pool mana preferred over tapping a new source, since spending
/// pool never removes a source), then any leftover pool + untapped sources
/// pay the generic amount. Paying pips before generic is what makes the
/// generic pass safe to do greedily: by the time it runs, every colored
/// requirement is already locked in, so which specific leftover
/// source/color pays generic can never strand a pip.
pub fn solve(
    cost: &Cost,
    x_value: u8,
    pool: [u8; 6],
    sources: &[ManaSource],
) -> Option<PaymentPlan> {
    let mut plan = PaymentPlan::default();
    let mut pool_remaining = pool;
    let mut used = vec![false; sources.len()];

    if !solve_pips(
        cost.pips,
        0,
        sources,
        &mut used,
        &mut pool_remaining,
        &mut plan,
    ) {
        return None;
    }

    let generic_needed = u32::from(cost.generic) + u32::from(cost.x_count) * u32::from(x_value);
    if !pay_generic(
        generic_needed,
        sources,
        &mut used,
        &mut pool_remaining,
        &mut plan,
    ) {
        return None;
    }

    Some(plan)
}

fn solve_pips(
    pips: &[Pip],
    idx: usize,
    sources: &[ManaSource],
    used: &mut [bool],
    pool_remaining: &mut [u8; 6],
    plan: &mut PaymentPlan,
) -> bool {
    solve_pips_with_life_budget_v1(
        pips,
        idx,
        sources,
        used,
        pool_remaining,
        plan,
        i64::MAX,
        None,
        None,
    )
}

/// A selected spell's remaining life budget participates in pip backtracking,
/// so rejecting a Phyrexian life branch can still find a legal mana allocation.
#[allow(clippy::too_many_arguments)]
fn solve_pips_with_life_budget_v1(
    pips: &[Pip],
    idx: usize,
    sources: &[ManaSource],
    used: &mut [bool],
    pool_remaining: &mut [u8; 6],
    plan: &mut PaymentPlan,
    max_life_payment: i64,
    generic_needed: Option<u32>,
    source_choices: Option<&SpellSourceChoicesV1>,
) -> bool {
    let Some(pip) = pips.get(idx) else {
        let Some(generic) = generic_needed else {
            return true;
        };
        // A failed generic branch must not alter the pip search's state.
        let mut generic_plan = plan.clone();
        let mut generic_pool = *pool_remaining;
        let mut generic_used = used.to_vec();
        if !pay_generic_with_source_choices_v1(
            generic,
            sources,
            &mut generic_used,
            &mut generic_pool,
            &mut generic_plan,
            source_choices,
        ) {
            return false;
        }
        *plan = generic_plan;
        *pool_remaining = generic_pool;
        used.copy_from_slice(&generic_used);
        return true;
    };

    let candidate_colors: Vec<ManaColor> = match *pip {
        Pip::Colored(c) => vec![c],
        Pip::Hybrid(a, b) => vec![a, b],
        Pip::Phyrexian(c) => vec![c],
    };

    // Prefer floating mana, but backtrack if Phyrexian life payment must
    // preserve that mana for the spell's generic requirement.
    for &c in &candidate_colors {
        let pi = c.pool_index();
        if pool_remaining[pi] > 0 {
            pool_remaining[pi] -= 1;
            plan.pool_used[pi] += 1;
            if solve_pips_with_life_budget_v1(
                pips,
                idx + 1,
                sources,
                used,
                pool_remaining,
                plan,
                max_life_payment,
                generic_needed,
                source_choices,
            ) {
                return true;
            }
            pool_remaining[pi] += 1;
            plan.pool_used[pi] -= 1;
        }
    }

    // Try tapping each untapped source capable of one of the candidate
    // colors. This is the branch that requires real backtracking: a wrong
    // choice here can strand a later pip.
    for i in 0..sources.len() {
        if used[i] {
            continue;
        }
        for &c in &candidate_colors {
            if !sources[i].choices.contains(&c) {
                continue;
            }
            // One mana of the tap pays this pip; a multi-yield source's
            // remaining mana joins the running pool (so later pips and the
            // generic pass can spend it) and is recorded as surplus (so
            // `engine::pay_plan` can float whatever survives). Both are
            // undone on backtrack, exactly like the tap itself. `extra` is
            // zero for every single-yield source, which keeps this branch
            // bit-identical to its pre-yield form for the whole pool.
            let extra = sources[i].yield_per_tap.saturating_sub(1);
            let pi = c.pool_index();
            set_spell_source_used_v1(used, i, true, source_choices);
            plan.taps.push((sources[i].id, c));
            pool_remaining[pi] += extra;
            plan.surplus[pi] += extra;
            if solve_pips_with_life_budget_v1(
                pips,
                idx + 1,
                sources,
                used,
                pool_remaining,
                plan,
                max_life_payment,
                generic_needed,
                source_choices,
            ) {
                return true;
            }
            plan.surplus[pi] -= extra;
            pool_remaining[pi] -= extra;
            plan.taps.pop();
            set_spell_source_used_v1(used, i, false, source_choices);
        }
    }

    // Phyrexian pips may also be paid with 2 life instead of mana.
    if matches!(pip, Pip::Phyrexian(_)) && i64::from(plan.life_paid) + 2 <= max_life_payment {
        plan.life_paid += 2;
        if solve_pips_with_life_budget_v1(
            pips,
            idx + 1,
            sources,
            used,
            pool_remaining,
            plan,
            max_life_payment,
            generic_needed,
            source_choices,
        ) {
            return true;
        }
        plan.life_paid -= 2;
    }

    false
}

fn pay_generic(
    needed: u32,
    sources: &[ManaSource],
    used: &mut [bool],
    pool_remaining: &mut [u8; 6],
    plan: &mut PaymentPlan,
) -> bool {
    pay_generic_with_source_choices_v1(needed, sources, used, pool_remaining, plan, None)
}

fn pay_generic_with_source_choices_v1(
    mut needed: u32,
    sources: &[ManaSource],
    used: &mut [bool],
    pool_remaining: &mut [u8; 6],
    plan: &mut PaymentPlan,
    source_choices: Option<&SpellSourceChoicesV1>,
) -> bool {
    for color in GENERIC_POOL_PAYMENT_ORDER {
        let pi = color.pool_index();
        let amt = &mut pool_remaining[pi];
        while needed > 0 && *amt > 0 {
            *amt -= 1;
            plan.pool_used[pi] += 1;
            needed -= 1;
        }
    }
    let order = source_choices.map(|_| {
        let mut order = (0..sources.len()).collect::<Vec<_>>();
        // With colored pips already solved, the largest yield for an alias
        // dominates its one-unit Convoke alternative for generic payment.
        order.sort_by_key(|&index| std::cmp::Reverse(sources[index].yield_per_tap));
        order
    });
    for i in (0..sources.len()).map(|index| order.as_ref().map_or(index, |order| order[index])) {
        if needed == 0 {
            break;
        }
        if used[i] {
            continue;
        }
        let color = sources[i].choices.first().copied().or_else(|| {
            source_choices
                .filter(|choices| choices.convoke[i])
                .map(|_| ManaColor::C)
        });
        if let Some(c) = color {
            set_spell_source_used_v1(used, i, true, source_choices);
            plan.taps.push((sources[i].id, c));
            // Generic mana is colorless in requirement, not in production:
            // one tap of a multi-yield source pays up to `yield` of the
            // outstanding generic at once. Anything it produces past the
            // remaining requirement joins the running pool and the plan's
            // surplus. For a single-yield source `spent` is 1 and `extra`
            // is 0, which is the pre-yield behavior exactly.
            let produced = u32::from(sources[i].yield_per_tap.max(1));
            let spent = produced.min(needed);
            needed -= spent;
            let extra = produced - spent;
            if extra > 0 {
                let pi = c.pool_index();
                let extra = u8::try_from(extra).expect("yield_per_tap is clamped to 1..=8");
                pool_remaining[pi] += extra;
                plan.surplus[pi] += extra;
            }
        }
    }
    needed == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn src(id: u32, choices: &[ManaColor]) -> ManaSource {
        ManaSource {
            id: ObjectId(id),
            choices: choices.to_vec(),
            yield_per_tap: 1,
        }
    }

    #[test]
    fn a_multi_yield_source_pays_several_pips_with_one_tap() {
        let mut tower = src(0, &[ManaColor::C]);
        tower.yield_per_tap = 3;
        let cost = Cost {
            pips: &[],
            generic: 3,
            x_count: 0,
        };
        let plan = solve(&cost, 0, [0; 6], &[tower]).expect("payable");
        assert_eq!(plan.taps.len(), 1);
        assert_eq!(plan.surplus, [0; 6]);
    }

    #[test]
    fn unspent_multi_yield_mana_is_recorded_as_surplus() {
        let mut tower = src(0, &[ManaColor::C]);
        tower.yield_per_tap = 3;
        let cost = Cost {
            pips: &[],
            generic: 1,
            x_count: 0,
        };
        let plan = solve(&cost, 0, [0; 6], &[tower]).expect("payable");
        assert_eq!(plan.taps.len(), 1);
        assert_eq!(plan.surplus[ManaColor::C.pool_index()], 2);
    }

    #[test]
    fn simple_same_color_cost_is_satisfied() {
        let cost = Cost {
            pips: &[Pip::Colored(ManaColor::R)],
            generic: 0,
            x_count: 0,
        };
        let sources = vec![src(0, &[ManaColor::R])];
        let plan = solve(&cost, 0, [0; 6], &sources).expect("should pay");
        assert_eq!(plan.taps, vec![(ObjectId(0), ManaColor::R)]);
    }

    #[test]
    fn insufficient_mana_returns_none() {
        let cost = Cost {
            pips: &[Pip::Colored(ManaColor::R), Pip::Colored(ManaColor::R)],
            generic: 0,
            x_count: 0,
        };
        let sources = vec![src(0, &[ManaColor::R])];
        assert_eq!(solve(&cost, 0, [0; 6], &sources), None);
    }

    #[test]
    fn generic_paid_by_leftover_any_color_source() {
        let cost = Cost {
            pips: &[Pip::Colored(ManaColor::R)],
            generic: 1,
            x_count: 0,
        };
        let sources = vec![src(0, &[ManaColor::R]), src(1, &[ManaColor::G])];
        let plan = solve(&cost, 0, [0; 6], &sources).expect("should pay");
        assert_eq!(plan.taps.len(), 2);
    }

    #[test]
    fn generic_pool_payment_matches_xmage_and_preserves_red_before_green() {
        let cost = Cost {
            pips: &[Pip::Colored(ManaColor::R)],
            generic: 2,
            x_count: 0,
        };
        let mut pool = [0u8; 6];
        pool[ManaColor::R.pool_index()] = 3;
        pool[ManaColor::G.pool_index()] = 1;

        let plan = solve(&cost, 0, pool, &[]).expect("R3 G1 pays {2}{R}");
        assert_eq!(plan.pool_used[ManaColor::R.pool_index()], 2);
        assert_eq!(plan.pool_used[ManaColor::G.pool_index()], 1);
    }

    #[test]
    fn pool_mana_is_used_before_tapping_new_sources() {
        let cost = Cost {
            pips: &[Pip::Colored(ManaColor::R)],
            generic: 0,
            x_count: 0,
        };
        let mut pool = [0u8; 6];
        pool[ManaColor::R.pool_index()] = 1;
        let sources = vec![src(0, &[ManaColor::R])];
        let plan = solve(&cost, 0, pool, &sources).expect("should pay");
        assert!(
            plan.taps.is_empty(),
            "should have used pool mana, not tapped a source"
        );
        assert_eq!(plan.pool_used[ManaColor::R.pool_index()], 1);
    }

    #[test]
    fn hybrid_pip_can_be_paid_by_either_color() {
        let cost = Cost {
            pips: &[Pip::Hybrid(ManaColor::R, ManaColor::G)],
            generic: 0,
            x_count: 0,
        };
        let sources = vec![src(0, &[ManaColor::G])];
        let plan = solve(&cost, 0, [0; 6], &sources).expect("should pay via G");
        assert_eq!(plan.taps, vec![(ObjectId(0), ManaColor::G)]);
    }

    #[test]
    fn phyrexian_pip_can_be_paid_with_life() {
        let cost = Cost {
            pips: &[Pip::Phyrexian(ManaColor::B)],
            generic: 0,
            x_count: 0,
        };
        let plan = solve(&cost, 0, [0; 6], &[]).expect("should pay via life");
        assert_eq!(plan.life_paid, 2);
        assert!(plan.taps.is_empty());
    }

    /// The scenario greedy gets wrong: pip 0 (W) is satisfiable by either
    /// source; pip 1 (U) is satisfiable ONLY by the dual. A first-fit
    /// greedy that considers sources in declaration order [dual, w_only]
    /// assigns the dual to the W pip (since it's tried first and can
    /// produce W), stranding the U pip. Exact backtracking must retry with
    /// w_only for the W pip, freeing the dual for U.
    #[test]
    fn backtracking_is_required_for_modal_sources() {
        let cost = Cost {
            pips: &[Pip::Colored(ManaColor::W), Pip::Colored(ManaColor::U)],
            generic: 0,
            x_count: 0,
        };
        let dual = src(0, &[ManaColor::W, ManaColor::U]);
        let w_only = src(1, &[ManaColor::W]);
        let sources = vec![dual, w_only];

        let plan = solve(&cost, 0, [0; 6], &sources).expect("a valid assignment exists");
        let paid_colors: Vec<ManaColor> = plan.taps.iter().map(|(_, c)| *c).collect();
        assert!(paid_colors.contains(&ManaColor::W));
        assert!(paid_colors.contains(&ManaColor::U));
        // The dual (id 0) must have been the one to pay U, since w_only
        // (id 1) cannot.
        assert!(plan.taps.contains(&(ObjectId(0), ManaColor::U)));
    }

    #[test]
    fn x_cost_adds_to_generic_requirement() {
        let cost = Cost {
            pips: &[],
            generic: 0,
            x_count: 1,
        };
        let sources = vec![src(0, &[ManaColor::R]), src(1, &[ManaColor::R])];
        assert!(solve(&cost, 2, [0; 6], &sources).is_some());
        assert!(solve(&cost, 3, [0; 6], &sources).is_none());
    }

    #[test]
    fn repeated_x_symbols_each_add_the_chosen_value() {
        let cost = Cost {
            pips: &[],
            generic: 1,
            x_count: 2,
        };
        let sources = (0..5)
            .map(|id| src(id, &[ManaColor::R]))
            .collect::<Vec<_>>();
        assert!(solve(&cost, 2, [0; 6], &sources).is_some());
        assert!(solve(&cost, 3, [0; 6], &sources).is_none());
    }

    #[test]
    fn spell_total_generic_modifiers_include_kicker_and_x_before_one_floor() {
        let forest = crate::card_def::card_id_by_name("Forest").unwrap();
        let mut state = GameState::new_from_libraries(&[forest], &[forest], |_| "Forest".into(), 1);
        let base = Cost {
            pips: &[Pip::Colored(ManaColor::U)],
            generic: 0,
            x_count: 2,
        };
        let kicker = Cost {
            pips: &[Pip::Colored(ManaColor::R)],
            generic: 4,
            x_count: 0,
        };
        for (x, increase, reduction, needed) in
            [(0, 0, 1, 3), (2, 0, 1, 7), (2, 3, 5, 6), (0, 2, 9, 0)]
        {
            state.players[0].mana_pool = [0, 1, 0, 1, 0, needed];
            let payment = can_pay_combined_spell_with_generic_modifiers_v1(
                &[&base, &kicker],
                x,
                PlayerId::P0,
                &state,
                false,
                increase,
                reduction,
            )
            .expect("complete selected cost should be exactly payable");
            assert_eq!(payment.pool_used, state.players[0].mana_pool);
            if needed > 0 {
                state.players[0].mana_pool[5] -= 1;
                assert!(can_pay_combined_spell_with_generic_modifiers_v1(
                    &[&base, &kicker],
                    x,
                    PlayerId::P0,
                    &state,
                    false,
                    increase,
                    reduction,
                )
                .is_none());
            }
            // Generic reduction cannot replace either colored requirement.
            state.players[0].mana_pool = [0, 0, 0, 1, 0, 255];
            assert!(can_pay_combined_spell_with_generic_modifiers_v1(
                &[&base, &kicker],
                x,
                PlayerId::P0,
                &state,
                false,
                increase,
                reduction,
            )
            .is_none());
        }
    }

    #[test]
    fn spell_total_generic_modifiers_do_not_truncate_large_combined_costs() {
        let forest = crate::card_def::card_id_by_name("Forest").unwrap();
        let mut state = GameState::new_from_libraries(&[forest], &[forest], |_| "Forest".into(), 2);
        state.players[0].mana_pool[5] = 254;
        let cost = Cost {
            pips: &[],
            generic: 255,
            x_count: 0,
        };
        let payment = can_pay_combined_spell_with_generic_modifiers_v1(
            &[&cost, &cost],
            0,
            PlayerId::P0,
            &state,
            false,
            0,
            256,
        )
        .expect("510 minus256 generic needs254");
        assert_eq!(payment.pool_used[5], 254);
        assert!(can_pay_combined_spell_with_generic_modifiers_v1(
            &[&cost, &cost],
            0,
            PlayerId::P0,
            &state,
            false,
            0,
            255,
        )
        .is_none());
    }

    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    fn ready_convoke_elf() -> (GameState, ObjectId) {
        let elf = crate::card_def::card_id_by_name("Llanowar Elves").unwrap();
        let forest = crate::card_def::card_id_by_name("Forest").unwrap();
        let mut state = GameState::new_from_libraries(&[elf], &[forest], |_| "card".into(), 946);
        let object = state.draw_card(PlayerId::P0).unwrap();
        assert!(state.move_hand_to_battlefield(PlayerId::P0, object));
        (state, object)
    }

    #[test]
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    fn convoke_can_pay_one_of_several_colored_pips_with_a_summoning_sick_creature() {
        let (mut state, elf) = ready_convoke_elf();
        assert!(state.objects.get(elf).summoning_sick);
        state.players[0].mana_pool[4] = 1;
        let pips = [Pip::Colored(ManaColor::G), Pip::Colored(ManaColor::G)];
        let (plan, convoked) =
            plan_spell_mana_total_with_convoke_v1(&pips, 0, PlayerId::P0, &state, false, &[], 0)
                .unwrap();
        assert_eq!(convoked, vec![elf]);
        assert!(plan.taps.is_empty());
        assert_eq!(plan.pool_used[4], 1);
        assert!(!state.objects.get(elf).tapped);
        assert_eq!(state.players[0].mana_pool[4], 1);
    }

    #[test]
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    fn convoke_cannot_tap_one_creature_for_both_mana_and_another_requirement() {
        let (mut state, elf) = ready_convoke_elf();
        state.objects.get_mut(elf).summoning_sick = false;
        assert!(gather_sources(PlayerId::P0, &state)
            .iter()
            .any(|source| source.id == elf));
        let pips = [Pip::Colored(ManaColor::G)];
        assert!(plan_spell_mana_total_with_convoke_v1(
            &pips,
            1,
            PlayerId::P0,
            &state,
            false,
            &[],
            0
        )
        .is_none());
        state.players[0].mana_pool[5] = 1;
        let (plan, convoked) =
            plan_spell_mana_total_with_convoke_v1(&pips, 1, PlayerId::P0, &state, false, &[], 0)
                .unwrap();
        assert_eq!(convoked, vec![elf]);
        assert!(plan.taps.is_empty());
        assert_eq!(plan.pool_used[5], 1);
        assert!(plan_spell_mana_total_with_convoke_v1(
            &pips,
            1,
            PlayerId::P0,
            &state,
            false,
            &[elf],
            0
        )
        .is_none());
    }

    #[test]
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    fn colorless_creature_convokes_generic_but_cannot_pay_a_colorless_pip() {
        let (mut state, elf) = ready_convoke_elf();
        state.objects.get_mut(elf).v4.effective_color_mask = 0;
        let (plan, convoked) =
            plan_spell_mana_total_with_convoke_v1(&[], 1, PlayerId::P0, &state, false, &[], 0)
                .unwrap();
        assert_eq!(convoked, vec![elf]);
        assert!(plan.taps.is_empty());
        assert_eq!(plan.pool_used, [0; 6]);
        for color in [ManaColor::C, ManaColor::G] {
            assert!(plan_spell_mana_total_with_convoke_v1(
                &[Pip::Colored(color)],
                0,
                PlayerId::P0,
                &state,
                false,
                &[],
                0
            )
            .is_none());
        }
    }

    #[test]
    fn convoke_alias_generic_payment_uses_a_multi_yield_mana_alternative() {
        let mut mana_source = src(1, &[ManaColor::G]);
        mana_source.yield_per_tap = 3;
        let sources = [src(0, &[ManaColor::G]), mana_source];
        let alternatives = SpellSourceChoicesV1 {
            aliases: vec![ObjectId(50), ObjectId(50)],
            convoke: vec![true, false],
        };
        let mut plan = PaymentPlan::default();
        let mut pool = [0; 6];
        let mut used = [false; 2];
        assert!(solve_pips_with_life_budget_v1(
            &[],
            0,
            &sources,
            &mut used,
            &mut pool,
            &mut plan,
            20,
            Some(3),
            Some(&alternatives)
        ));
        assert_eq!(plan.taps, vec![(ObjectId(1), ManaColor::G)]);
        assert_eq!(used, [true; 2]);
        assert_eq!(plan.surplus, [0; 6]);
        let mut plan = PaymentPlan::default();
        let mut used = [false; 2];
        assert!(!solve_pips_with_life_budget_v1(
            &[],
            0,
            &sources,
            &mut used,
            &mut pool,
            &mut plan,
            20,
            Some(4),
            Some(&alternatives)
        ));
        assert_eq!(used, [false; 2]);
        assert_eq!(plan, PaymentPlan::default());
    }

    #[test]
    fn spell_total_backtracks_to_life_when_floating_red_must_pay_generic() {
        let forest = crate::card_def::card_id_by_name("Forest").unwrap();
        let mut state = GameState::new_from_libraries(&[forest], &[forest], |_| "Forest".into(), 5);
        state.players[0].life = 2;
        state.players[0].mana_pool[3] = 1;
        let pips = [Pip::Phyrexian(ManaColor::R)];
        let plan = plan_spell_mana_total_v1(&pips, 1, PlayerId::P0, &state, false, &[], 0)
            .expect("Two life pays the pip and floating R pays the generic tax");
        assert_eq!(plan.life_paid, 2);
        assert_eq!(plan.pool_used[3], 1);
        assert!(plan_spell_mana_total_v1(&pips, 1, PlayerId::P0, &state, false, &[], 1).is_none());
        state.players[0].life = 3;
        assert!(plan_spell_mana_total_v1(&pips, 1, PlayerId::P0, &state, false, &[], 1).is_some());
        assert_eq!(state.players[0].mana_pool[3], 1, "planning never pays mana");
        assert_eq!(state.players[0].life, 3, "planning never pays life");
    }

    #[test]
    #[cfg(not(feature = "standard-magezero-fixtures"))]
    fn spell_life_budget_backtracks_from_phyrexian_life_to_dual_source_mana() {
        let aquifer = crate::card_def::card_id_by_name("Contaminated Aquifer").unwrap();
        let swamp = crate::card_def::card_id_by_name("Swamp").unwrap();
        let mut state =
            GameState::new_from_libraries(&[aquifer, swamp], &[swamp], |_| "land".into(), 4);
        let first = state.draw_card(PlayerId::P0).unwrap();
        let second = state.draw_card(PlayerId::P0).unwrap();
        state.move_hand_to_battlefield(PlayerId::P0, first);
        state.move_hand_to_battlefield(PlayerId::P0, second);
        let (aquifer, swamp) = if state.objects.get(first).card_def == aquifer {
            (first, second)
        } else {
            (second, first)
        };
        state.objects.get_mut(aquifer).tapped = false;
        state.players[0].battlefield = vec![aquifer, swamp];
        state.players[0].life = 2;
        let pips = [Pip::Colored(ManaColor::B), Pip::Phyrexian(ManaColor::U)];
        let plan = plan_spell_mana_total_v1(&pips, 0, PlayerId::P0, &state, false, &[], 1)
            .expect("Swamp pays B, Aquifer pays U, and one life pays the additional cost");
        assert_eq!(plan.life_paid, 0);
        assert_eq!(
            plan.taps,
            vec![(swamp, ManaColor::B), (aquifer, ManaColor::U)]
        );
    }

    #[test]
    fn spell_total_plan_reserves_tap_sources_and_combines_life_requirements() {
        let forest = crate::card_def::card_id_by_name("Forest").unwrap();
        let mut state = GameState::new_from_libraries(&[forest], &[forest], |_| "Forest".into(), 3);
        let land = state.draw_card(PlayerId::P0).unwrap();
        state.move_hand_to_battlefield(PlayerId::P0, land);
        let green = [Pip::Colored(ManaColor::G)];
        assert!(plan_spell_mana_total_v1(&green, 0, PlayerId::P0, &state, false, &[], 0).is_some());
        assert!(
            plan_spell_mana_total_v1(&green, 0, PlayerId::P0, &state, false, &[land], 0).is_none()
        );

        let phyrexian = [Pip::Phyrexian(ManaColor::B)];
        state.players[0].life = 5;
        let plan =
            plan_spell_mana_total_v1(&phyrexian, 0, PlayerId::P0, &state, false, &[], 3).unwrap();
        assert_eq!(plan.life_paid, 2);
        assert!(
            plan_spell_mana_total_v1(&phyrexian, 0, PlayerId::P0, &state, false, &[], 4).is_none()
        );
        state.players[0].life = -1;
        assert!(plan_spell_mana_total_v1(&[], 0, PlayerId::P0, &state, false, &[], 0).is_some());
        assert!(plan_spell_mana_total_v1(&[], 0, PlayerId::P0, &state, false, &[], 1).is_none());
    }

    #[test]
    fn can_pay_combined_needs_both_costs_paid_from_the_same_pool() {
        // Goblin Bushwhacker's shape: base {R}, Kicker {R} -- exactly 2
        // untapped Mountains covers both combined; 1 Mountain covers
        // neither the combined check nor a double-count of the same source.
        use crate::state::GameState;
        let mountain = crate::card_def::card_id_by_name("Mountain").expect("Mountain in CARD_DEFS");
        let base = Cost {
            pips: &[Pip::Colored(ManaColor::R)],
            generic: 0,
            x_count: 0,
        };
        let kicker = Cost {
            pips: &[Pip::Colored(ManaColor::R)],
            generic: 0,
            x_count: 0,
        };

        let mut one_mountain =
            GameState::new_from_libraries(&[mountain], &[mountain], |_| "Mountain".to_string(), 1);
        let land = one_mountain.draw_card(PlayerId::P0).unwrap();
        one_mountain.move_hand_to_battlefield(PlayerId::P0, land);
        assert!(
            can_pay_combined(&[&base, &kicker], 0, PlayerId::P0, &one_mountain).is_none(),
            "1 Mountain can't pay 2 {{R}} pips at once"
        );

        let mut two_mountains = GameState::new_from_libraries(
            &[mountain, mountain],
            &[mountain],
            |_| "Mountain".to_string(),
            1,
        );
        let l0 = two_mountains.draw_card(PlayerId::P0).unwrap();
        let l1 = two_mountains.draw_card(PlayerId::P0).unwrap();
        two_mountains.move_hand_to_battlefield(PlayerId::P0, l0);
        two_mountains.move_hand_to_battlefield(PlayerId::P0, l1);
        let plan = can_pay_combined(&[&base, &kicker], 0, PlayerId::P0, &two_mountains)
            .expect("2 Mountains should pay both {R} pips");
        assert_eq!(plan.taps.len(), 2);
    }
}
