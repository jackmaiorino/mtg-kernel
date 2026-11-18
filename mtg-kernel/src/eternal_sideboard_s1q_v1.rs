//! Card programs and static rules for the eternal catalog's sideboard wave S1q
//! (`docs/design/eternal_formats_support_v1.md`): the three sideboard cards
//! that completed Legacy Tron's 75, and the Powerstone token.
//!
//! Oracle text, as read from Scryfall on 2026-11-17:
//!
//! - Cityscape Leveler, {8}, Artifact Creature - Construct 8/8: "Trample. When
//!   you cast this spell and whenever this creature attacks, destroy up to one
//!   target nonland permanent. Its controller creates a tapped Powerstone
//!   token. Unearth {8}"
//! - Powerstone token, Token Artifact - Powerstone: "{T}: Add {C}. This mana
//!   can't be spent to cast a nonartifact spell." Entering tapped is not part
//!   of the token (the Leveler's ruling).
//! - Invasion Submersible, {2}{U}, Artifact - Vehicle 0/0: "When this Vehicle
//!   enters, return up to one other target nonland permanent to its owner's
//!   hand. Exhaust - Waterbend {3}: This Vehicle becomes an artifact creature.
//!   Put three +1/+1 counters on it. (While paying a waterbend cost, you can
//!   tap your artifacts and creatures to help. Each one pays for {1}.
//!   Activate each exhaust ability only once.)"
//! - Summon: Bahamut, {9}, Enchantment Creature - Saga Dragon 9/9: "(As this
//!   Saga enters and after your draw step, add a lore counter. Sacrifice after
//!   IV.) I, II - Destroy up to one target nonland permanent. III - Draw two
//!   cards. IV - Mega Flare - This creature deals damage equal to the total
//!   mana value of other permanents you control to each opponent. Flying"
//!
//! Where each piece lives:
//!
//! - The Leveler's cast trigger functions on the stack (Ugin, Eye of the
//!   Storms' shape); its attack trigger is ordinary. Unearth is Phyrexian
//!   Dragon Engine's recipe and end-step exile.
//! - The Powerstone token's mana ability is a definition the automatic spell
//!   planner never taps, as The Mightstone and Weakstone's is;
//!   `restrict_produced_mana` turns its {C} into a unit restricted by
//!   `ManaSpendRestrictionDef::PowerstoneV1` (#726).
//! - Invasion Submersible's exhaust ability is registered as four activated
//!   abilities, one per number of waterbend taps (zero to three): ability `k`
//!   costs `TapControlled { count: k, ArtifactOrCreature }` and the rest of the
//!   {3} in mana. The taps are chosen through the ordinary cost-target choice.
//!   The four share one exhaust record per object (`S1qStateV1::exhausted`),
//!   so `activation_allowed` offers none of them once one was activated.
//!   `waterbend_mana` sets the mana part after cost reductions, and
//!   `retain_payable_waterbend_taps` keeps a tap from leaving the mana part
//!   unpayable. The animation has no duration: `S1qStateV1::animated` holds
//!   the incarnation, which `engine::object_has_type` reads.
//! - Summon: Bahamut uses the shared Saga rules (714); its chapters are
//!   `bahamut_chapter_one` to `bahamut_chapter_four`.
//!
//! `eternal_cards_v1` forwards this module's effect leaves, triggers and
//! per-game state, wave S1o's recipe tables fall through to
//! `build_eternal_s1q_v1.rs`, and the engine calls the static hooks below only
//! in `eternal-fixtures` builds. Every table is keyed by a printed name that
//! occurs only in the eternal registry.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

pub(crate) mod targets;

use crate::card_def::{CardType, CostComponent, TargetSpec, CARD_DEFS};
use crate::effect::{EffectContinuation, EffectObjectBinding, EffectOp, ExecCtx, ObjectRef};
use crate::eternal_cards_v1::EternalOpV1;
use crate::eternal_legacy_e9a_v1::{link_live_in, live_link};
use crate::event::{self, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::{Cost, ManaColor, RestrictedManaUnitV1};
use crate::standard_cards_v1::{StandardOpV1, StandardTargetV1};
use crate::state::{GameState, ObjectLinkV4, Target, Zone};
use crate::trigger::{TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};
use targets::S1qTargetV1;

pub(crate) const CITYSCAPE_LEVELER: &str = "Cityscape Leveler";
pub(crate) const INVASION_SUBMERSIBLE: &str = "Invasion Submersible";
pub(crate) const SUMMON_BAHAMUT: &str = "Summon: Bahamut";
pub(crate) const POWERSTONE_TOKEN: &str = "Powerstone Token";

/// The generic mana in Invasion Submersible's printed "Waterbend {3}".
const WATERBEND_GENERIC: u8 = 3;

/// "Destroy up to one target nonland permanent" (the Leveler's triggers and
/// Bahamut's first two chapters).
pub(crate) const DESTROY_TARGET: TargetSpec = TargetSpec::StandardV1(StandardTargetV1::S1qV1(
    S1qTargetV1::UpToOneNonlandPermanentV1,
));

/// "Return up to one other target nonland permanent to its owner's hand"
/// (Phelia, Exuberant Shepherd's filter).
pub(crate) const BOUNCE_TARGET: TargetSpec =
    TargetSpec::StandardV1(StandardTargetV1::UpToOneOtherNonlandPermanentV1);

/// Per-game state of this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Absent from every snapshot and state
/// hash until one of them needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct S1qStateV1 {
    /// Invasion Submersible incarnations whose exhaust ability was activated
    /// (its costs paid). A new object after a zone change is not here.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exhausted: Vec<ObjectLinkV4>,
    /// Invasion Submersible incarnations that became artifact creatures. The
    /// effect has no duration: it lasts while that object is on the
    /// battlefield.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub animated: Vec<ObjectLinkV4>,
}

impl S1qStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        self.exhausted.is_empty() && self.animated.is_empty()
    }
}

impl Hash for S1qStateV1 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        if !self.is_empty() {
            b"eternal-s1q/v1".hash(state);
            self.exhausted.hash(state);
            self.animated.hash(state);
        }
    }
}

fn s1q_state(state: &mut GameState) -> &mut S1qStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).s1q
}

pub fn read_state(state: &GameState) -> Option<&S1qStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.s1q)
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::S1qV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum S1qOpV1 {
    /// Cityscape Leveler's cast trigger: destroy target 0, then its
    /// controller creates a tapped Powerstone token.
    LevelerCastV1,
    /// Cityscape Leveler's attack trigger, the same program.
    LevelerAttackV1,
    /// Invasion Submersible's enters trigger: return target 0 to its owner's
    /// hand.
    SubmersibleBounceV1,
    /// Invasion Submersible's exhaust ability: the source incarnation becomes
    /// an artifact creature and gets three +1/+1 counters.
    SubmersibleExhaustV1,
    /// Summon: Bahamut's chapter I or II: destroy target 0.
    BahamutDestroyV1 { chapter: u8 },
    /// Summon: Bahamut's chapter IV, Mega Flare.
    BahamutMegaFlareV1,
}

impl S1qOpV1 {
    /// No leaf of this module stops for a player's choice.
    pub(crate) fn contains_player_choice(&self) -> bool {
        false
    }
}

fn s1q(op: S1qOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::S1qV1(op)))
}

fn named(state: &GameState, object: ObjectId, name: &str) -> bool {
    state
        .objects
        .try_get(object)
        .and_then(|live| CARD_DEFS.get(usize::from(live.card_def)))
        .is_some_and(|definition| definition.name == name && definition.is_executable())
}

fn halt(state: &mut GameState, source: ObjectId) {
    state.engine.halted = Some((
        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
        source,
    ));
}

// ---- Programs -----------------------------------------------------------------------

/// Cityscape Leveler's cast trigger.
pub fn leveler_cast() -> EffectOp {
    s1q(S1qOpV1::LevelerCastV1)
}

/// Cityscape Leveler's attack trigger.
pub fn leveler_attack() -> EffectOp {
    s1q(S1qOpV1::LevelerAttackV1)
}

/// Unearth's "exile it at the beginning of the next end step" (the engine's
/// shared machinery, Phyrexian Dragon Engine's trigger).
fn unearth_exile() -> EffectOp {
    EffectOp::BindWarpExileToTriggerSource
}

/// Invasion Submersible's enters trigger.
pub fn submersible_enters() -> EffectOp {
    s1q(S1qOpV1::SubmersibleBounceV1)
}

/// Invasion Submersible's exhaust ability (each of its four payments).
pub fn submersible_exhaust() -> EffectOp {
    s1q(S1qOpV1::SubmersibleExhaustV1)
}

/// Summon: Bahamut's chapter I.
pub fn bahamut_chapter_one() -> EffectOp {
    s1q(S1qOpV1::BahamutDestroyV1 { chapter: 1 })
}

/// Summon: Bahamut's chapter II.
pub fn bahamut_chapter_two() -> EffectOp {
    s1q(S1qOpV1::BahamutDestroyV1 { chapter: 2 })
}

/// Summon: Bahamut's chapter III: "Draw two cards."
pub fn bahamut_chapter_three() -> EffectOp {
    EffectOp::DrawCards {
        player: crate::effect::PlayerRef::Controller,
        count: 2,
    }
}

/// Summon: Bahamut's chapter IV, Mega Flare.
pub fn bahamut_chapter_four() -> EffectOp {
    s1q(S1qOpV1::BahamutMegaFlareV1)
}

// ---- Triggered abilities ------------------------------------------------------------

const fn trigger(
    condition: TriggerCondition,
    home_zone: Zone,
    effect: fn() -> EffectOp,
) -> TriggeredAbilityDef {
    TriggeredAbilityDef {
        condition,
        home_zone,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index: 0,
        effect,
    }
}

/// The cast trigger functions while the Leveler is on the stack; unearthing it
/// is not casting it, so only the attack trigger can follow an unearth.
const LEVELER_TRIGGERS: [TriggeredAbilityDef; 3] = [
    trigger(TriggerCondition::CastSelf, Zone::Stack, leveler_cast),
    trigger(TriggerCondition::Attacks, Zone::Battlefield, leveler_attack),
    trigger(
        TriggerCondition::BeginningEndStepAfterUnearth,
        Zone::Battlefield,
        unearth_exile,
    ),
];
const SUBMERSIBLE_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::Etb,
    Zone::Battlefield,
    submersible_enters,
)];

/// Triggered abilities of this module's cards, falling through to wave C1a.
/// Bahamut's chapters are its `SagaDef`.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        CITYSCAPE_LEVELER => &LEVELER_TRIGGERS,
        INVASION_SUBMERSIBLE => &SUBMERSIBLE_TRIGGERS,
        _ => crate::eternal_cube_c1a_v1::triggers_for(name),
    }
}

/// Target specs of this module's printed triggered abilities and chapters.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    match name {
        CITYSCAPE_LEVELER if *effect == leveler_cast() || *effect == leveler_attack() => {
            Some(DESTROY_TARGET)
        }
        CITYSCAPE_LEVELER if *effect == unearth_exile() => Some(TargetSpec::None),
        INVASION_SUBMERSIBLE if *effect == submersible_enters() => Some(BOUNCE_TARGET),
        SUMMON_BAHAMUT if *effect == bahamut_chapter_one() || *effect == bahamut_chapter_two() => {
            Some(DESTROY_TARGET)
        }
        SUMMON_BAHAMUT
            if *effect == bahamut_chapter_three() || *effect == bahamut_chapter_four() =>
        {
            Some(TargetSpec::None)
        }
        _ => None,
    }
}

// ---- Powerstone mana ----------------------------------------------------------------

/// A Powerstone token: "{T}: Add {C}. This mana can't be spent to cast a
/// nonartifact spell." The {C} its ability just added becomes one restricted
/// unit, which artifact spells and every payment that is not part of casting
/// a spell may spend (`ManaSpendRestrictionDef::PowerstoneV1`).
pub(crate) fn restrict_produced_mana(
    state: &mut GameState,
    player: PlayerId,
    source: ObjectId,
    color: ManaColor,
) {
    if !named(state, source, POWERSTONE_TOKEN) {
        return;
    }
    let live = state.objects.get(source);
    let unit = RestrictedManaUnitV1 {
        color,
        restriction: crate::card_def::ManaSpendRestrictionDef::PowerstoneV1,
        source: ObjectLinkV4 {
            object: source,
            zone_change_count: live.zone_change_count,
        },
        source_card_def: live.card_def,
    };
    let seat = &mut state.players[player.index()];
    let produced = seat.mana_pool[color.pool_index()].min(1);
    seat.mana_pool[color.pool_index()] -= produced;
    for _ in 0..produced {
        seat.restricted_mana_pool.0.push(unit.clone());
    }
}

// ---- Invasion Submersible: exhaust and waterbend ------------------------------------

/// The number of waterbend taps one of the Submersible's four exhaust
/// payments carries: its `TapControlled` count, 0 for the all-mana payment.
fn waterbend_taps(components: &[CostComponent]) -> u8 {
    components
        .iter()
        .find_map(|component| match component {
            CostComponent::TapControlled { count, .. } => Some(*count),
            _ => None,
        })
        .unwrap_or(0)
}

/// The generic mana of "Waterbend {3}" after cost reductions (Forensic
/// Gadgeteer's "{1} less, never below one mana"). Waterbend taps can pay at
/// most this much (the ruling: only up to the generic mana of the waterbend
/// cost, even when the cost is reduced).
fn waterbend_total(state: &GameState, source: ObjectId) -> u8 {
    crate::eternal_cube_c4a_v1::reduce_artifact_activation(
        state,
        source,
        Cost {
            pips: &[],
            generic: WATERBEND_GENERIC,
            x_count: 0,
        },
    )
    .generic
}

/// The mana part of one of Invasion Submersible's exhaust payments, after cost
/// reductions: the reduced waterbend total less the payment's taps. `None`
/// for every other ability. A cost increase (Anointed Peacekeeper, Gloom) is
/// added by the engine afterwards, to the mana part.
pub(crate) fn waterbend_mana(
    state: &GameState,
    source: ObjectId,
    components: &[CostComponent],
) -> Option<Cost> {
    if !named(state, source, INVASION_SUBMERSIBLE) {
        return None;
    }
    let taps = waterbend_taps(components);
    Some(Cost {
        pips: &[],
        generic: waterbend_total(state, source).saturating_sub(taps),
        x_count: 0,
    })
}

/// The untapped artifacts and creatures `player` controls, outside `chosen`:
/// what a waterbend tap may name (the `TapControlled` candidates of
/// `PermanentFilter::ArtifactOrCreature`, the Submersible itself included;
/// summoning sickness does not matter, as the cost has no {T}).
fn tappable(state: &GameState, player: PlayerId, chosen: &[ObjectId]) -> Vec<ObjectId> {
    state.players[player.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|object| !chosen.contains(object))
        .filter(|&object| {
            let live = state.objects.get(object);
            live.zone == Zone::Battlefield
                && live.controller == player
                && !live.tapped
                && (crate::engine::object_has_type(state, object, CardType::Artifact)
                    || crate::engine::object_has_type(state, object, CardType::Creature))
        })
        .collect()
}

/// Whether `more` further waterbend taps can be chosen, beside the `chosen`
/// ones, so that the payment's mana part (`cost`) stays payable with none of
/// them among the mana sources. Reserving more permanents never helps the
/// payment, so a plan with only `chosen` reserved must exist; when enough
/// tappable permanents lie outside that plan's sources the answer is yes, and
/// otherwise every choice is tried.
fn completion_exists(
    state: &GameState,
    player: PlayerId,
    source: ObjectId,
    cost: &Cost,
    chosen: &[ObjectId],
    more: usize,
) -> bool {
    let Some(plan) = crate::mana::plan_activation_mana_v1(cost, 0, player, state, source, chosen)
    else {
        return false;
    };
    let rest = tappable(state, player, chosen);
    if rest.len() < more {
        return false;
    }
    let outside_plan = rest
        .iter()
        .filter(|&&object| !plan.taps.iter().any(|&(tapped, _)| tapped == object))
        .count();
    outside_plan >= more || completion_among(state, player, source, cost, chosen, &rest, more)
}

/// Whether `more` picks among `among`, taken in order, leave `cost` payable
/// with them and `chosen` reserved.
fn completion_among(
    state: &GameState,
    player: PlayerId,
    source: ObjectId,
    cost: &Cost,
    chosen: &[ObjectId],
    among: &[ObjectId],
    more: usize,
) -> bool {
    if more == 0 {
        return crate::mana::plan_activation_mana_v1(cost, 0, player, state, source, chosen)
            .is_some();
    }
    among.iter().enumerate().any(|(index, &next)| {
        let mut with = chosen.to_vec();
        with.push(next);
        completion_among(state, player, source, cost, &with, &among[index + 1..], more - 1)
    })
}

/// The mana part of the Submersible's exhaust payment `ability_index`, with
/// its cost reductions and increases, as the engine charges it.
fn exhaust_mana(state: &GameState, source: ObjectId, ability_index: usize) -> Option<Cost> {
    let live = state.objects.try_get(source)?;
    let ability = CARD_DEFS[usize::from(live.card_def)]
        .activated_abilities
        .get(ability_index)?;
    crate::engine::activation_mana_payment(ability.cost, live.controller, source, state)
}

/// Invasion Submersible's exhaust ability: none of its four payments once one
/// was activated for this object; a payment with more waterbend taps than the
/// reduced waterbend total allows is not offered; and a payment is offered
/// only if some choice of its taps leaves its mana part payable.
pub(crate) fn activation_allowed(
    state: &GameState,
    source: ObjectId,
    name: &str,
    ability_index: usize,
) -> bool {
    if name != INVASION_SUBMERSIBLE {
        return true;
    }
    let Some(live) = state.objects.try_get(source) else {
        return true;
    };
    if live.zone != Zone::Battlefield {
        return true;
    }
    let link = live_link(state, source);
    if read_state(state).is_some_and(|s1q| s1q.exhausted.contains(&link)) {
        return false;
    }
    let Some(ability) = CARD_DEFS[usize::from(live.card_def)]
        .activated_abilities
        .get(ability_index)
    else {
        return true;
    };
    let taps = waterbend_taps(ability.cost);
    if taps > waterbend_total(state, source) {
        return false;
    }
    let Some(cost) = exhaust_mana(state, source, ability_index) else {
        return true;
    };
    completion_exists(state, live.controller, source, &cost, &[], usize::from(taps))
}

/// Keeps only the waterbend taps after which the rest of the pending exhaust
/// payment can still be completed. Called with the candidates of a pending
/// activation's `TapControlled` cost; any other pending activation is left
/// alone.
pub(crate) fn retain_payable_waterbend_taps(
    state: &GameState,
    source: ObjectId,
    already_chosen: &[EffectObjectBinding],
    candidates: &mut Vec<EffectObjectBinding>,
) {
    let Some(pending) = state
        .engine
        .pending_activation
        .as_ref()
        .filter(|pending| pending.source == source)
    else {
        return;
    };
    if !named(state, source, INVASION_SUBMERSIBLE) {
        return;
    }
    let ability_index = usize::from(pending.ability_index);
    let Some(ability) = CARD_DEFS[usize::from(state.objects.get(source).card_def)]
        .activated_abilities
        .get(ability_index)
    else {
        return;
    };
    let Some(cost) = exhaust_mana(state, source, ability_index) else {
        return;
    };
    let taps = usize::from(waterbend_taps(ability.cost));
    let chosen = already_chosen
        .iter()
        .map(|binding| binding.object)
        .collect::<Vec<_>>();
    let more = taps.saturating_sub(chosen.len() + 1);
    candidates.retain(|candidate| {
        let mut with = chosen.clone();
        with.push(candidate.object);
        completion_exists(state, pending.controller, source, &cost, &with, more)
    });
}

/// Records that an exhaust payment of the Submersible was made, as the
/// ability's costs have been paid.
pub(crate) fn activation_paid(state: &mut GameState, source: ObjectId, ability_index: u8) {
    if !cfg!(feature = "eternal-fixtures") || !named(state, source, INVASION_SUBMERSIBLE) {
        return;
    }
    let live = state.objects.get(source);
    let abilities = CARD_DEFS[usize::from(live.card_def)].activated_abilities;
    if live.zone != Zone::Battlefield || usize::from(ability_index) >= abilities.len() {
        return;
    }
    let link = live_link(state, source);
    let exhausted = &mut s1q_state(state).exhausted;
    if !exhausted.contains(&link) {
        exhausted.push(link);
    }
}

/// Whether `object` is an Invasion Submersible incarnation that became an
/// artifact creature (it already is an artifact).
pub(crate) fn animated(state: &GameState, object: ObjectId) -> bool {
    cfg!(feature = "eternal-fixtures")
        && read_state(state).is_some_and(|s1q| {
            !s1q.animated.is_empty()
                && state.objects.try_get(object).is_some_and(|live| {
                    live.zone == Zone::Battlefield
                        && s1q.animated.contains(&ObjectLinkV4 {
                            object,
                            zone_change_count: live.zone_change_count,
                        })
                })
        })
}

/// Drops exhaust and animation records of objects that left the battlefield:
/// a returning card is a new object.
pub(crate) fn begin_turn(state: &mut GameState, _player: PlayerId) {
    let Some(s1q) = read_state(state) else {
        return;
    };
    if s1q.is_empty() {
        return;
    }
    let keep = |links: &[ObjectLinkV4]| {
        links
            .iter()
            .copied()
            .filter(|&link| link_live_in(state, link, Zone::Battlefield))
            .collect::<Vec<_>>()
    };
    let exhausted = keep(&s1q.exhausted);
    let animated = keep(&s1q.animated);
    if exhausted.len() != s1q.exhausted.len() || animated.len() != s1q.animated.len() {
        let s1q = s1q_state(state);
        s1q.exhausted = exhausted;
        s1q.animated = animated;
    }
}

// ---- Resolution ---------------------------------------------------------------------

/// The object target 0 names if it is still legal for `spec` (608.2b).
fn legal_target(ctx: &ExecCtx, state: &GameState, spec: TargetSpec) -> Option<ObjectId> {
    let Some(Target::Object(object)) = ctx.targets.first().copied() else {
        return None;
    };
    (ctx.target_incarnation_matches(0, state)
        && crate::engine::effect_target_is_legal(
            state,
            ctx.source,
            ctx.controller,
            spec,
            &ctx.targets,
            0,
        ))
    .then_some(object)
}

/// Destroys target 0 if it is still legal; with `powerstone`, the player who
/// controlled it just before then creates a tapped Powerstone token, whether
/// or not it was destroyed (indestructible, regenerated). With no target, or
/// an illegal one, nothing happens.
fn resolve_destroy(ctx: &ExecCtx, state: &mut GameState, powerstone: bool) {
    let Some(target) = legal_target(ctx, state, DESTROY_TARGET) else {
        return;
    };
    let controller = state.objects.get(target).controller;
    crate::effect::execute(
        &EffectOp::DestroyObject {
            object: ObjectRef::Target(0),
        },
        ctx,
        state,
    );
    if !powerstone || state.engine.halted.is_some() {
        return;
    }
    let token = crate::card_def::card_id_by_name(POWERSTONE_TOKEN)
        .unwrap_or_else(|| panic!("{POWERSTONE_TOKEN} in CARD_DEFS"));
    let mut proposal = ProposedEvent::create_token(token, controller);
    if let ProposedEvent::CreateToken(token) = &mut proposal {
        token.entry_tapped = true;
    }
    event::propose_and_commit_batch(state, vec![proposal]);
}

/// Returns target 0 to its owner's hand if it is still legal.
fn resolve_bounce(ctx: &ExecCtx, state: &mut GameState) {
    let Some(target) = legal_target(ctx, state, BOUNCE_TARGET) else {
        return;
    };
    event::propose_and_commit(state, ProposedEvent::zone_change(target, Zone::Hand));
}

/// "This Vehicle becomes an artifact creature. Put three +1/+1 counters on
/// it." Only the incarnation whose ability this is; after a zone change the
/// new object is not affected.
fn resolve_exhaust(ctx: &ExecCtx, state: &mut GameState) {
    let Some(contract) = ctx.ability_source_contract else {
        return;
    };
    let Some(live) = state.objects.try_get(contract.source) else {
        return;
    };
    if live.zone != Zone::Battlefield || live.zone_change_count != contract.zone_change_count {
        return;
    }
    let link = live_link(state, contract.source);
    let animated = &mut s1q_state(state).animated;
    if !animated.contains(&link) {
        animated.push(link);
    }
    if event::add_plus_one_counters(state, contract.source, ctx.controller, 3).is_err() {
        halt(state, ctx.source);
    }
}

/// Mega Flare: the Saga deals damage equal to the total mana value of the
/// other permanents its controller controls as the chapter resolves to each
/// opponent (the ruling). A token's mana value is 0 unless it copies a card.
fn resolve_mega_flare(ctx: &ExecCtx, state: &mut GameState) {
    let amount = state.players[ctx.controller.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&object| {
            object != ctx.source && {
                let live = state.objects.get(object);
                live.zone == Zone::Battlefield && live.controller == ctx.controller
            }
        })
        .map(|object| u32::from(crate::engine::object_mana_value(state, object)))
        .sum::<u32>();
    let amount = i32::try_from(amount).unwrap_or(i32::MAX);
    if amount <= 0 {
        return;
    }
    event::propose_and_commit(
        state,
        event::damage_from_exec_ctx_v1(ctx, Target::Player(ctx.controller.opponent()), amount),
    );
}

// ---- Execution ----------------------------------------------------------------------

/// Runs one of this module's leaves.
pub(crate) fn execute(op: &S1qOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        S1qOpV1::LevelerCastV1 | S1qOpV1::LevelerAttackV1 => resolve_destroy(ctx, state, true),
        S1qOpV1::SubmersibleBounceV1 => resolve_bounce(ctx, state),
        S1qOpV1::SubmersibleExhaustV1 => resolve_exhaust(ctx, state),
        S1qOpV1::BahamutDestroyV1 { .. } => resolve_destroy(ctx, state, false),
        S1qOpV1::BahamutMegaFlareV1 => resolve_mega_flare(ctx, state),
    }
}

/// Runs one of this module's leaves in the resumable interpreter.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &S1qOpV1,
    _path: Vec<u16>,
) -> Result<bool, String> {
    execute(op, &continuation.ctx, state);
    Ok(false)
}
