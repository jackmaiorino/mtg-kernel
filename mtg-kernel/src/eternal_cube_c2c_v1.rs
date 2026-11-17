//! Card programs for the eternal catalog's Vintage Cube wave C2c
//! (`docs/design/eternal_formats_support_v1.md`): mana artifacts and Fastbond.
//!
//! `eternal_cards_v1` forwards this module's effect leaves here, and the
//! recipe tables of `build_eternal_c2c_v1.rs` name its programs. Every
//! table is keyed by a printed name that occurs only in the eternal registry,
//! so the Pauper, FDN and Standard catalogs never reach it. Behavior was read
//! from each card's current Oracle text and, where present, its XMage source.
//!
//! Where each card lives:
//!
//! - The four Talismans are rows of `standard_lands_v1.rs`'s painland table
//!   ("{T}: Add {C}" plus one damaging ability per color), the same shape
//!   with an artifact as the source.
//! - Mind Stone's draw ability is a recipe of `build_eternal_c2c_v1.rs`;
//!   Basalt Monolith's yield rows are wave E5a's (Grim Monolith's shape) and
//!   its "doesn't untap" is `prevents_untap_step` below, called from wave
//!   E5a's aggregator in `eternal_vintage_v1`.
//! - Candelabra of Tawnos enumerates X from 0 to 8, one ability each; ability
//!   `n` targets exactly `n` lands (`targets`).
//! - Coalition Relic's first-main-phase release is `begin_main_phase`, called
//!   from wave Lurrus's main-phase hook, with `RelicReleaseV1`.
//! - Fastbond: `extra_land_plays` (called from wave E7a's
//!   `additional_land_plays`) and `note_land_played` (called from wave E5a's
//!   land-play hook) queue its damage trigger, collected by
//!   `delayed_triggers`.
//! - Pentad Prism: sunburst records its colors through wave Lurrus's
//!   `note_colors_spent`, and its mana ability (cost "remove a charge
//!   counter") is an additional mana ability with no tap cost whose counter
//!   is removed by `note_mana_ability_used` and which is hidden at zero
//!   counters by `mana_ability_blocked`.
//! - Everflowing Chalice: `multikicker_cost` is the reusable multikicker
//!   piece (see its documentation), and the engine's
//!   `ManaAbilityAmountDef::ChargeCountersOnSourceV1` is its yield.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

pub(crate) mod targets;

use crate::card_def::TargetSpec;
use crate::effect::{
    EffectContinuation, EffectFrame, EffectOp, ExecCtx, ObjectRef, PlayerRef, TargetRef,
};
use crate::eternal_cards_v1::EternalOpV1;
use crate::event::CommittedEvent;
use crate::ids::{ObjectId, PlayerId};
use crate::mana::{Cost, ManaColor};
use crate::standard_cards_v1::{StandardOpV1, StandardTargetV1};
use crate::state::{AbilitySourceContractV4, GameState, Step, Target, Zone};
use crate::trigger::{PendingTrigger, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

pub(crate) const BASALT_MONOLITH: &str = "Basalt Monolith";
pub(crate) const COALITION_RELIC: &str = "Coalition Relic";
pub(crate) const EVERFLOWING_CHALICE: &str = "Everflowing Chalice";
pub(crate) const FASTBOND: &str = "Fastbond";
pub(crate) const PENTAD_PRISM: &str = "Pentad Prism";

/// Per-game state for this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Default (and absent from snapshots)
/// until a card of this wave needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct C2cStateV1 {
    /// Fastbond triggers detected as a land was played, waiting for the next
    /// trigger collection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fastbond_triggers: Vec<QueuedFastbondV1>,
}

impl C2cStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// One Fastbond trigger: the Fastbond that triggered and its controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueuedFastbondV1 {
    pub source: AbilitySourceContractV4,
    pub controller: PlayerId,
}

pub(crate) fn c2c_state(state: &mut GameState) -> &mut C2cStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).c2c
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::C2cV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C2cOpV1 {
    /// Coalition Relic: "remove all charge counters from this artifact. Add
    /// one mana of any color for each charge counter removed this way."
    RelicReleaseV1,
    /// Pentad Prism: sunburst. The artifact enters with a charge counter for
    /// each color of mana spent to cast it.
    PrismEnterV1,
    /// Candelabra of Tawnos: untap each target land that is still the land
    /// announced and still a legal target.
    UntapTargetLandsV1,
}

impl C2cOpV1 {
    /// Whether the leaf may stop for a player's choice (or a draw that a
    /// replacement can interrupt), so it runs in the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        match self {
            // One color choice per counter removed.
            Self::RelicReleaseV1 => true,
            Self::PrismEnterV1 | Self::UntapTargetLandsV1 => false,
        }
    }
}

fn c2c(op: C2cOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C2cV1(op)))
}

/// A battlefield ability source that is still the incarnation the ability
/// came from.
fn live_ability_source(ctx: &ExecCtx, state: &GameState) -> Option<ObjectId> {
    let contract = ctx.ability_source_contract?;
    let live = state.objects.try_get(contract.source)?;
    (live.zone == Zone::Battlefield && live.zone_change_count == contract.zone_change_count)
        .then_some(contract.source)
}

// ---- Programs -----------------------------------------------------------------------

/// Candelabra of Tawnos: "{X}, {T}: Untap X target lands." Ability `k` of the
/// recipe table is X = k, with exactly `k` target lands
/// (`targets::C2cTargetV1::LandsV1`).
fn candelabra_untap() -> EffectOp {
    c2c(C2cOpV1::UntapTargetLandsV1)
}

/// Candelabra of Tawnos with X = 0: it only taps.
pub fn candelabra_untap_0() -> EffectOp {
    EffectOp::Sequence(Vec::new())
}

pub fn candelabra_untap_1() -> EffectOp {
    candelabra_untap()
}

pub fn candelabra_untap_2() -> EffectOp {
    candelabra_untap()
}

pub fn candelabra_untap_3() -> EffectOp {
    candelabra_untap()
}

pub fn candelabra_untap_4() -> EffectOp {
    candelabra_untap()
}

pub fn candelabra_untap_5() -> EffectOp {
    candelabra_untap()
}

pub fn candelabra_untap_6() -> EffectOp {
    candelabra_untap()
}

pub fn candelabra_untap_7() -> EffectOp {
    candelabra_untap()
}

pub fn candelabra_untap_8() -> EffectOp {
    candelabra_untap()
}

/// Pentad Prism: "Sunburst".
pub fn pentad_prism() -> EffectOp {
    c2c(C2cOpV1::PrismEnterV1)
}

/// Everflowing Chalice: "This artifact enters with a charge counter on it for
/// each time it was kicked." The number of kicks is the spell's announced
/// count (`multikicker_cost`), which Chalice of the Void's program stores as
/// charge counters.
pub fn everflowing_chalice() -> EffectOp {
    crate::eternal_e5a_v1::chalice_of_the_void()
}

/// Fastbond's trigger: "this enchantment deals 1 damage to you."
fn fastbond_damage() -> EffectOp {
    EffectOp::DealDamage {
        target: TargetRef::Controller,
        amount: 1,
    }
}

/// Coalition Relic's trigger.
fn relic_release() -> EffectOp {
    c2c(C2cOpV1::RelicReleaseV1)
}

/// "Add one mana of any color."
fn any_color_mana() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: [
            ManaColor::W,
            ManaColor::U,
            ManaColor::B,
            ManaColor::R,
            ManaColor::G,
        ]
        .into_iter()
        .map(|color| EffectOp::AddMana {
            player: PlayerRef::Controller,
            colors: vec![color],
        })
        .collect(),
    }
}

// ---- Multikicker --------------------------------------------------------------------

/// Multikicker (702.33c): the kicker cost, which may be paid any number of
/// times as the spell is cast. This is the reusable piece. A multikicker card
/// is a row of this table plus a spell program that reads
/// `ExecCtx::x_value`, the number of times the cost was paid.
///
/// The count is announced like Replicate's payment count (601.2b): the
/// engine treats a card with a multikicker cost as announcing a count after
/// its modes and targets, offers every count the caster can pay for (the
/// decision is the X value decision), adds the cost once per kick to the
/// mana payment (`spell_costs_v1`, the `replicate` field), and stores the
/// count as the spell's X value. The card's mana cost has no X, so its mana
/// value is unchanged. Unlike Replicate, nothing is copied.
pub fn multikicker_cost(name: &str) -> Option<Cost> {
    match name {
        EVERFLOWING_CHALICE => Some(Cost {
            pips: &[],
            generic: 2,
            x_count: 0,
        }),
        _ => None,
    }
}

// ---- Hooks: engine ------------------------------------------------------------------

/// Basalt Monolith: "This artifact doesn't untap during your untap step."
pub(crate) fn prevents_untap_step(state: &GameState, object: ObjectId) -> bool {
    crate::eternal_e5a_v1::active_named(state, object, BASALT_MONOLITH)
}

/// Fastbond: "You may play any number of lands on each of your turns." The
/// number of additional land plays `player` has (the engine's count is a
/// `u8`, so this is as many as it can count).
pub(crate) fn extra_land_plays(state: &GameState, player: PlayerId) -> u8 {
    if crate::eternal_e5a_v1::active_permanents(state, FASTBOND, Some(player))
        .next()
        .is_some()
    {
        u8::MAX
    } else {
        0
    }
}

/// Fastbond: "Whenever you play a land, if it wasn't the first land you
/// played this turn, this enchantment deals 1 damage to you." Called as
/// `player` plays a land (a special action), after the turn's land count
/// includes it; lands put onto the battlefield by effects are not played.
pub(crate) fn note_land_played(state: &mut GameState, player: PlayerId) {
    if state.players[player.index()].lands_played_this_turn < 2 {
        return;
    }
    let fastbonds =
        crate::eternal_e5a_v1::active_permanents(state, FASTBOND, Some(player)).collect::<Vec<_>>();
    for fastbond in fastbonds {
        let source = AbilitySourceContractV4::capture(state, fastbond);
        c2c_state(state).fastbond_triggers.push(QueuedFastbondV1 {
            source,
            controller: player,
        });
    }
}

/// Pentad Prism's mana ability costs "Remove a charge counter from this
/// artifact": without one it can't be activated.
pub(crate) fn mana_ability_blocked(state: &GameState, source: ObjectId) -> bool {
    crate::eternal_e5a_v1::active_named(state, source, PENTAD_PRISM)
        && state.objects.get(source).counters.charge <= 0
}

/// Pentad Prism's mana ability was activated: the cost's charge counter is
/// removed.
pub(crate) fn note_mana_ability_used(state: &mut GameState, source: ObjectId) {
    if crate::eternal_e5a_v1::active_named(state, source, PENTAD_PRISM) {
        let counters = &mut state.objects.get_mut(source).counters.charge;
        *counters = (*counters - 1).max(0);
    }
}

/// A main phase began for the active player: Coalition Relic's "At the
/// beginning of your first main phase" trigger for each Relic they control.
/// It triggers with no counters too, since a counter put on in response
/// resolves first and is then removed.
pub(crate) fn begin_main_phase(state: &mut GameState) -> Vec<PendingTrigger> {
    if state.step != Step::Main1 {
        return Vec::new();
    }
    let player = state.active_player;
    let relics = crate::eternal_e5a_v1::active_permanents(state, COALITION_RELIC, Some(player))
        .collect::<Vec<_>>();
    relics
        .into_iter()
        .map(|relic| {
            let source = AbilitySourceContractV4::capture(state, relic);
            crate::eternal_cards_v1::delayed_trigger(source, player, relic_release())
        })
        .collect()
}

/// The triggers this collection puts on the stack that no printed trigger
/// condition matches: Fastbond's land-play damage.
pub(crate) fn delayed_triggers(
    state: &mut GameState,
    _events: &[CommittedEvent],
) -> Vec<PendingTrigger> {
    let Some(eternal) = state.eternal_v1.as_mut() else {
        return Vec::new();
    };
    if eternal.c2c.fastbond_triggers.is_empty() {
        return Vec::new();
    }
    std::mem::take(&mut eternal.c2c.fastbond_triggers)
        .into_iter()
        .map(|queued| {
            crate::eternal_cards_v1::delayed_trigger(
                queued.source,
                queued.controller,
                fastbond_damage(),
            )
        })
        .collect()
}

// ---- Triggered abilities ------------------------------------------------------------

/// Triggered abilities of this module's cards, falling through to wave
/// C3a. Fastbond's and Coalition Relic's triggers are delayed triggers.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    crate::eternal_cube_c3a_v1::triggers_for(name)
}

/// Target specs of this module's printed and queued triggered abilities.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    delayed_effect_matches(name, effect).then_some(TargetSpec::None)
}

/// Queued triggers owned by this module's cards.
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    match name {
        FASTBOND => *effect == fastbond_damage(),
        COALITION_RELIC => *effect == relic_release(),
        _ => false,
    }
}

// ---- Execution ----------------------------------------------------------------------

/// Runs one of this module's leaves in synchronous execution.
pub(crate) fn execute(op: &C2cOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        // Asks a player for a color per counter, so it only runs in `drive`.
        C2cOpV1::RelicReleaseV1 => {}
        C2cOpV1::PrismEnterV1 => run_prism_enter(ctx, state),
        C2cOpV1::UntapTargetLandsV1 => run_untap_target_lands(ctx, state),
    }
}

/// Candelabra of Tawnos (608.2b): each target that is still the announced
/// land and still legal for the ability untaps; the others do nothing.
fn run_untap_target_lands(ctx: &ExecCtx, state: &mut GameState) {
    let Some(contract) = ctx.ability_source_contract else {
        return;
    };
    let Ok(count) = u8::try_from(ctx.targets.len()) else {
        return;
    };
    let spec = TargetSpec::StandardV1(StandardTargetV1::C2cV1(targets::C2cTargetV1::LandsV1(
        count,
    )));
    for (index, target) in ctx.targets.iter().enumerate() {
        let Target::Object(land) = *target else {
            continue;
        };
        if ctx.target_incarnation_matches(index, state)
            && crate::engine::effect_target_is_legal_from_ability_source(
                state,
                contract,
                ctx.controller,
                spec,
                &ctx.targets,
                index,
            )
        {
            crate::engine::attempt_untap(state, land);
        }
    }
}

/// Pentad Prism: the colors of mana spent to cast it were recorded while it
/// was on the stack; a Prism put onto the battlefield without being cast has
/// no counters.
fn run_prism_enter(ctx: &ExecCtx, state: &mut GameState) {
    if state
        .objects
        .try_get(ctx.source)
        .is_none_or(|live| live.zone != Zone::Stack)
    {
        return;
    }
    let colors = crate::eternal_lurrus_v1::colors_spent_on(state, ctx.source);
    crate::effect::execute(
        &EffectOp::MoveObject {
            object: ObjectRef::ThisSource,
            to_zone: Zone::Battlefield,
        },
        ctx,
        state,
    );
    if state
        .objects
        .try_get(ctx.source)
        .is_some_and(|live| live.zone == Zone::Battlefield)
    {
        state.objects.get_mut(ctx.source).counters.charge = i32::from(colors);
    }
}

/// Runs one of this module's leaves in the resumable interpreter.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &C2cOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    match op {
        C2cOpV1::RelicReleaseV1 => {
            // A Relic that left the battlefield has no counters to remove.
            let Some(relic) = live_ability_source(&continuation.ctx, state) else {
                return Ok(false);
            };
            let counters = &mut state.objects.get_mut(relic).counters.charge;
            let removed = usize::try_from(*counters).unwrap_or(0);
            if removed == 0 {
                return Ok(false);
            }
            *counters = 0;
            continuation.frames.push(EffectFrame::Program {
                op: EffectOp::Sequence(vec![any_color_mana(); removed]),
                path,
            });
            Ok(false)
        }
        C2cOpV1::PrismEnterV1 => {
            run_prism_enter(&continuation.ctx, state);
            Ok(false)
        }
        C2cOpV1::UntapTargetLandsV1 => {
            run_untap_target_lands(&continuation.ctx, state);
            Ok(false)
        }
    }
}
