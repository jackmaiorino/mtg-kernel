//! Card programs and static rules for the eternal catalog's sideboard wave S1o
//! (`docs/design/eternal_formats_support_v1.md`): Stronghold Gambit (completing
//! Legacy Rakdos Reanimator's 75), Eowyn, Fearless Knight (completing Legacy
//! Naya Initiative's 75), Unlicensed Hearse (completing Legacy Dimir
//! Control's 75) and Outland Liberator (Legacy Selesnya Depths, whose 75 also
//! needs Keen-Eyed Curator).
//!
//! Oracle text, as read from Scryfall on 2026-11-17 (the snapshot and the
//! registry spell Eowyn without the accent):
//!
//! - Stronghold Gambit, {1}{R}, Sorcery: "Each player chooses a card in their
//!   hand. Then each player reveals their chosen card. The owner of each
//!   creature card revealed this way with the lowest mana value puts it onto
//!   the battlefield."
//! - Eowyn, Fearless Knight, {2}{R}{W}, Legendary Creature - Human Knight 3/4:
//!   "Haste. When Eowyn enters, exile target creature an opponent controls
//!   with greater power. Legendary creatures you control gain protection from
//!   each of that creature's colors until end of turn."
//! - Unlicensed Hearse, {2}, Artifact - Vehicle */*: "{T}: Exile up to two
//!   target cards from a single graveyard. Unlicensed Hearse's power and
//!   toughness are each equal to the number of cards exiled with it. Crew 2"
//! - Outland Liberator, {1}{G}, Creature - Human Werewolf 2/2: "{1},
//!   Sacrifice this creature: Destroy target artifact or enchantment.
//!   Daybound". Back face Frenzied Trapbreaker, Creature - Werewolf 3/3: "{1},
//!   Sacrifice this creature: Destroy target artifact or enchantment. Whenever
//!   this creature attacks, destroy target artifact or enchantment defending
//!   player controls. Nightbound"
//!
//! `eternal_cards_v1` forwards this module's effect leaves, choices, triggers
//! and per-game state, wave S1n's recipe tables fall through to
//! `build_eternal_s1o_v1.rs`, and the engine calls the static hooks below only
//! in `eternal-fixtures` builds. Every table is keyed by a printed name that
//! occurs only in the eternal registry.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

pub(crate) mod targets;

use crate::card_def::{mana_color_mask, CardType, Supertype, TargetSpec, CARD_DEFS};
use crate::effect::{EffectContinuation, EffectObjectBinding, EffectOp, ExecCtx, ObjectRef};
use crate::eternal_cards_v1::e3::{E3ChoiceKindV1, E3ChoiceV1};
use crate::eternal_cards_v1::EternalOpV1;
use crate::eternal_legacy_e9a_v1::{link_live_in, live_link};
use crate::event::{self, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::{StandardOpV1, StandardTargetV1};
use crate::state::{GameState, ObjectLinkV4, Target, Zone};
use crate::trigger::{TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};
use targets::S1oTargetV1;

pub(crate) const EOWYN: &str = "Eowyn, Fearless Knight";
pub(crate) const UNLICENSED_HEARSE: &str = "Unlicensed Hearse";
pub(crate) const OUTLAND_LIBERATOR: &str = "Outland Liberator";

/// Eowyn's enters trigger's target.
pub(crate) const EOWYN_TARGET: TargetSpec = TargetSpec::StandardV1(StandardTargetV1::S1oV1(
    S1oTargetV1::OpponentCreatureGreaterPowerV1,
));

/// Unlicensed Hearse's {T} ability's targets.
pub(crate) const HEARSE_TARGET: TargetSpec = TargetSpec::StandardV1(StandardTargetV1::S1oV1(
    S1oTargetV1::UpToTwoCardsFromOneGraveyardV1,
));

/// Frenzied Trapbreaker's attack trigger's target: "target artifact or
/// enchantment defending player controls" (the attacker's opponent).
pub(crate) const TRAPBREAKER_TARGET: TargetSpec =
    TargetSpec::OpponentArtifactOrEnchantmentPermanent;

/// One card exiled by an Unlicensed Hearse incarnation's ability: the Hearse
/// incarnation and the card's exile incarnation (607.2a).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HearseExileV1 {
    pub hearse: ObjectLinkV4,
    pub card: ObjectLinkV4,
}

/// Per-game state of this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Absent from every snapshot and state
/// hash until one of them needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct S1oStateV1 {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hearse_exiles: Vec<HearseExileV1>,
    /// The hand incarnations Stronghold Gambit is putting onto the
    /// battlefield, whose destinations were decided from the battlefield
    /// before any of them moved. Kept while they wait in hand (an entry that
    /// pauses for an as-enters choice, such as Meddling Mage's name), pruned
    /// once they have moved.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gambit_entries: Vec<ObjectLinkV4>,
}

impl S1oStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        self.hearse_exiles.is_empty() && self.gambit_entries.is_empty()
    }
}

impl Hash for S1oStateV1 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        if !self.is_empty() {
            b"eternal-s1o/v1".hash(state);
            self.hearse_exiles.hash(state);
            self.gambit_entries.hash(state);
        }
    }
}

fn s1o_state(state: &mut GameState) -> &mut S1oStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).s1o
}

pub fn read_state(state: &GameState) -> Option<&S1oStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.s1o)
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::S1oV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum S1oOpV1 {
    /// Stronghold Gambit: each player with a card in hand chooses one in
    /// APNAP order, then the lowest mana value creature cards among the
    /// chosen cards enter at once. A choice.
    StrongholdGambitV1,
    /// Eowyn's enters trigger: exile target 0, then legendary creatures the
    /// controller controls gain protection from each of its colors until end
    /// of turn.
    EowynEntersV1,
    /// Unlicensed Hearse's {T} ability: exile each still-legal target card,
    /// linked to the Hearse incarnation.
    HearseExileV1,
    /// Frenzied Trapbreaker's attack trigger: destroy target 0.
    TrapbreakerAttackV1,
}

impl S1oOpV1 {
    /// Whether the leaf may stop for a player's choice, so it runs in the
    /// resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(self, Self::StrongholdGambitV1)
    }
}

/// A choice of this module, carried as `E3ChoiceKindV1::S1oV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum S1oChoiceV1 {
    /// Stronghold Gambit: exactly one card of the chooser's hand. `chosen`
    /// are the earlier players' cards, `later` the players still to choose.
    /// The earlier cards stay private until every player has chosen: a
    /// prompt only shows its own candidates.
    GambitV1 {
        chosen: Vec<EffectObjectBinding>,
        later: Vec<PlayerId>,
    },
}

impl S1oChoiceV1 {
    /// Whether the candidates are hidden from the other player.
    pub(crate) fn chooser_private(&self) -> bool {
        true
    }

    pub(crate) fn public_purpose(&self) -> crate::rl::TargetSelectionPurposeV4 {
        crate::rl::TargetSelectionPurposeV4::CardSelection
    }

    /// Object incarnations this choice refers to beyond its candidates.
    pub(crate) fn referenced_bindings(&self) -> Vec<EffectObjectBinding> {
        match self {
            Self::GambitV1 { chosen, .. } => chosen.clone(),
        }
    }
}

fn s1o(op: S1oOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::S1oV1(op)))
}

fn halt(state: &mut GameState, source: ObjectId) {
    state.engine.halted = Some((
        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
        source,
    ));
}

fn bind(state: &GameState, object: ObjectId) -> EffectObjectBinding {
    let live = state.objects.get(object);
    EffectObjectBinding {
        object,
        expected_zone: live.zone,
        expected_zone_change_count: live.zone_change_count,
    }
}

fn binding_live(state: &GameState, binding: EffectObjectBinding) -> bool {
    state.objects.try_get(binding.object).is_some_and(|live| {
        live.zone == binding.expected_zone
            && live.zone_change_count == binding.expected_zone_change_count
    })
}

fn named(state: &GameState, object: ObjectId, name: &str) -> bool {
    state
        .objects
        .try_get(object)
        .and_then(|live| CARD_DEFS.get(usize::from(live.card_def)))
        .is_some_and(|definition| definition.name == name && definition.is_executable())
}

// ---- Programs -----------------------------------------------------------------------

/// Stronghold Gambit's spell program.
pub fn stronghold_gambit() -> EffectOp {
    s1o(S1oOpV1::StrongholdGambitV1)
}

/// Eowyn's enters trigger.
pub fn eowyn_enters() -> EffectOp {
    s1o(S1oOpV1::EowynEntersV1)
}

/// Unlicensed Hearse's {T} ability.
pub fn hearse_exile() -> EffectOp {
    s1o(S1oOpV1::HearseExileV1)
}

/// Frenzied Trapbreaker's attack trigger.
pub fn trapbreaker_attack() -> EffectOp {
    s1o(S1oOpV1::TrapbreakerAttackV1)
}

// ---- Triggered abilities ------------------------------------------------------------

const fn trigger(
    condition: TriggerCondition,
    face_index: u8,
    effect: fn() -> EffectOp,
) -> TriggeredAbilityDef {
    TriggeredAbilityDef {
        condition,
        home_zone: Zone::Battlefield,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index,
        effect,
    }
}

const EOWYN_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, 0, eowyn_enters)];
/// Only the back face, Frenzied Trapbreaker, has the attack trigger.
const LIBERATOR_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Attacks, 1, trapbreaker_attack)];

/// Triggered abilities of this module's cards, falling through to wave S1q.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        EOWYN => &EOWYN_TRIGGERS,
        OUTLAND_LIBERATOR => &LIBERATOR_TRIGGERS,
        _ => crate::eternal_sideboard_s1q_v1::triggers_for(name),
    }
}

/// Target specs of this module's printed triggered abilities.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    match name {
        EOWYN if *effect == eowyn_enters() => Some(EOWYN_TARGET),
        OUTLAND_LIBERATOR if *effect == trapbreaker_attack() => Some(TRAPBREAKER_TARGET),
        _ => None,
    }
}

// ---- Statics and turn cleanup -------------------------------------------------------

/// Unlicensed Hearse's characteristic-defining power and toughness (604.3):
/// the number of cards exiled with this incarnation that are still in exile.
/// It applies in every zone; off the battlefield the object is new and no
/// card is exiled with it, so the value is 0.
pub(crate) fn characteristic_defining_power(state: &GameState, object: ObjectId) -> Option<i32> {
    if !named(state, object, UNLICENSED_HEARSE) {
        return None;
    }
    let live = state.objects.get(object);
    if live.zone != Zone::Battlefield {
        return Some(0);
    }
    let hearse = ObjectLinkV4 {
        object,
        zone_change_count: live.zone_change_count,
    };
    let count = read_state(state).map_or(0, |s1o| {
        s1o.hearse_exiles
            .iter()
            .filter(|entry| entry.hearse == hearse && link_live_in(state, entry.card, Zone::Exile))
            .count()
    });
    Some(i32::try_from(count).unwrap_or(i32::MAX))
}

/// Drops Hearse links whose Hearse left the battlefield or whose card left
/// exile: neither can count again.
pub(crate) fn begin_turn(state: &mut GameState, _player: PlayerId) {
    let Some(s1o) = read_state(state) else {
        return;
    };
    if s1o.hearse_exiles.is_empty() {
        return;
    }
    let kept = s1o
        .hearse_exiles
        .iter()
        .copied()
        .filter(|entry| {
            link_live_in(state, entry.hearse, Zone::Battlefield)
                && link_live_in(state, entry.card, Zone::Exile)
        })
        .collect::<Vec<_>>();
    if kept.len() != s1o.hearse_exiles.len() {
        s1o_state(state).hearse_exiles = kept;
    }
}

/// Whether `object` is a Stronghold Gambit entrant whose battlefield
/// destination was already decided before any entrant moved, so a
/// Containment Priest entering earlier in the same batch does not exile it
/// (614.12). A batch moves its members one at a time, and the Priest's
/// replacement is applied as each member moves, so without this record the
/// second entrant would see a Priest that entered first. Read by
/// `eternal_sideboard_s1a_v1::statics::replace_zone_change`.
pub(crate) fn gambit_entry_prepared(state: &GameState, object: ObjectId) -> bool {
    read_state(state).is_some_and(|s1o| {
        !s1o.gambit_entries.is_empty()
            && state.objects.try_get(object).is_some_and(|live| {
                live.zone == Zone::Hand
                    && s1o.gambit_entries.contains(&ObjectLinkV4 {
                        object,
                        zone_change_count: live.zone_change_count,
                    })
            })
    })
}

/// Replaces the prepared Gambit entries, keeping only those still waiting in
/// hand, and drops the record when none is left.
fn set_gambit_entries(state: &mut GameState, entries: Vec<ObjectLinkV4>) {
    let entries = entries
        .into_iter()
        .filter(|&link| link_live_in(state, link, Zone::Hand))
        .collect::<Vec<_>>();
    if entries.is_empty() {
        if let Some(eternal) = state.eternal_v1.as_mut() {
            eternal.s1o.gambit_entries.clear();
        }
    } else {
        s1o_state(state).gambit_entries = entries;
    }
}

// ---- Stronghold Gambit --------------------------------------------------------------

/// Stages the Gambit choice of the first player from `players` with a card
/// in hand, or finishes the spell with the `chosen` cards. 101.4: the active
/// player chooses first, then the other player; neither sees the other's
/// card until both are revealed.
fn stage_gambit(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    players: &[PlayerId],
    chosen: Vec<EffectObjectBinding>,
    path: Vec<u16>,
) -> Result<bool, String> {
    for (index, &player) in players.iter().enumerate() {
        let cards = state.players[player.index()]
            .hand
            .iter()
            .copied()
            .filter(|&card| {
                let live = state.objects.get(card);
                live.zone == Zone::Hand && live.owner == player
            })
            .map(|card| bind(state, card))
            .collect::<Vec<_>>();
        if cards.is_empty() {
            continue;
        }
        let remaining = continuation.frames.clone();
        crate::eternal_cards_v1::e3::offer(
            continuation,
            E3ChoiceV1 {
                kind: E3ChoiceKindV1::S1oV1(S1oChoiceV1::GambitV1 {
                    chosen,
                    later: players[index + 1..].to_vec(),
                }),
                player,
                candidates: cards
                    .iter()
                    .map(|card| Target::Object(card.object))
                    .collect(),
                bindings: cards,
                min: 1,
                max: 1,
                ordered: false,
                path,
                remaining,
            },
        );
        return Ok(true);
    }
    finish_gambit(state, &continuation.ctx, chosen)?;
    Ok(false)
}

/// Whether the hand card `object` would enter as Phyrexian Metamorph with a
/// copy menu to offer. Its owner may then choose a copy as it enters, which
/// this noncast route does not model (the native pre-entry copy owner
/// serves only paid Show and Tell, Reanimate and Aether Vial).
fn metamorph_copy_offered(state: &GameState, object: ObjectId) -> Result<bool, String> {
    if !crate::eternal_sideboard_s1b_v1::meddling_mage_entry::vial::is_physical_metamorph_in_hand(
        state, object,
    ) || crate::eternal_legacy_e7a_v1::active_dress_down_timestamp(state, Some(object)).is_some()
    {
        return Ok(false);
    }
    let owner = state.objects.get(object).owner;
    Ok(!crate::effect::metamorph_copy_candidates(state, owner)?.is_empty())
}

/// Reveals every chosen card, then puts the creature cards with the lowest
/// mana value among them onto the battlefield under their owners' control,
/// all at once. Their destinations are decided from the battlefield before
/// any of them moves.
fn finish_gambit(
    state: &mut GameState,
    ctx: &ExecCtx,
    chosen: Vec<EffectObjectBinding>,
) -> Result<(), String> {
    if chosen
        .iter()
        .any(|binding| binding.expected_zone != Zone::Hand || !binding_live(state, *binding))
    {
        return Err("a Stronghold Gambit card left its owner's hand".to_string());
    }
    let creature = |state: &GameState, object: ObjectId| {
        CARD_DEFS[usize::from(state.objects.get(object).card_def)].has_type(CardType::Creature)
    };
    let mana_value = |state: &GameState, object: ObjectId| {
        CARD_DEFS[usize::from(state.objects.get(object).card_def)].mana_value
    };
    let lowest = chosen
        .iter()
        .filter(|binding| creature(state, binding.object))
        .map(|binding| mana_value(state, binding.object))
        .min();
    let entrants = chosen
        .iter()
        .map(|binding| binding.object)
        .filter(|&object| creature(state, object) && Some(mana_value(state, object)) == lowest)
        .collect::<Vec<_>>();
    // Every destination is decided from the battlefield as it is now, before
    // any entrant moves (Containment Priest already there exiles them).
    set_gambit_entries(state, Vec::new());
    let mut proposals = Vec::with_capacity(entrants.len());
    let mut prepared = Vec::new();
    for &object in &entrants {
        let ProposedEvent::ZoneChange(mut proposal) =
            ProposedEvent::zone_change(object, Zone::Battlefield)
        else {
            unreachable!("Stronghold Gambit moves chosen hand cards");
        };
        crate::eternal_sideboard_s1a_v1::statics::replace_zone_change(state, &mut proposal);
        if proposal.to_zone == Zone::Battlefield {
            // A Metamorph that would enter with something to copy needs its
            // owner's as-enters copy choice, which this route does not model.
            if metamorph_copy_offered(state, object)? {
                halt(state, ctx.source);
                return Ok(());
            }
            prepared.push(live_link(state, object));
        }
        proposals.push(ProposedEvent::ZoneChange(proposal));
    }
    for binding in &chosen {
        let owner = state.objects.get(binding.object).owner;
        // spellbench-public-reveal: Stronghold Gambit reveals every chosen card once both players have chosen.
        state.reveal_hand_card(owner.opponent(), owner, binding.object)?;
    }
    if proposals.is_empty() {
        return Ok(());
    }
    set_gambit_entries(state, prepared.clone());
    event::propose_and_commit_batch(state, proposals);
    // Entrants that moved leave the record; an entry still waiting in hand
    // for an as-enters choice keeps it.
    set_gambit_entries(state, prepared);
    Ok(())
}

// ---- Eowyn, Fearless Knight ---------------------------------------------------------

/// Exiles a still-legal target (its power is compared with Eowyn's current
/// or last-known power again, 608.2b), then gives each legendary creature
/// its controller controls protection from each color the exiled creature
/// had as it last existed on the battlefield, until end of turn. The set of
/// creatures is fixed as the ability resolves (611.2c).
fn resolve_eowyn(ctx: &ExecCtx, state: &mut GameState) {
    let (Some(source), Some(Target::Object(target))) =
        (ctx.ability_source_contract, ctx.targets.first().copied())
    else {
        return;
    };
    if !ctx.target_incarnation_matches(0, state)
        || !crate::engine::effect_target_is_legal_from_ability_source(
            state,
            source,
            ctx.controller,
            EOWYN_TARGET,
            &ctx.targets,
            0,
        )
    {
        return;
    }
    let colors = crate::engine::object_color_mask(state, target);
    event::propose_and_commit(state, ProposedEvent::zone_change(target, Zone::Exile));
    if state.engine.halted.is_some() {
        return;
    }
    let colors = crate::eternal_legacy_e9a_v1::statics::COLORS
        .into_iter()
        .filter(|&color| colors & mana_color_mask(color) != 0)
        .collect::<Vec<_>>();
    if colors.is_empty() {
        return;
    }
    let hosts = state.players[ctx.controller.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&object| {
            let live = state.objects.get(object);
            live.zone == Zone::Battlefield
                && live.controller == ctx.controller
                && crate::engine::object_has_type(state, object, CardType::Creature)
                && crate::engine::effective_supertypes(state, object)
                    .contains(&Supertype::Legendary)
        })
        .map(|object| live_link(state, object))
        .collect::<Vec<_>>();
    crate::eternal_legacy_e9a_v1::grant_protections(state, &hosts, &colors);
}

// ---- Unlicensed Hearse --------------------------------------------------------------

/// Exiles each still-legal target card at once and links the cards that
/// reached exile to the Hearse incarnation that activated the ability. A
/// card exiled after that Hearse left the battlefield is linked to an
/// incarnation that no longer exists, so no Hearse counts it.
fn resolve_hearse(ctx: &ExecCtx, state: &mut GameState) {
    let Some(source) = ctx.ability_source_contract else {
        return;
    };
    let cards = ctx
        .targets
        .iter()
        .enumerate()
        .filter_map(|(index, target)| {
            let Target::Object(object) = *target else {
                return None;
            };
            (ctx.target_incarnation_matches(index, state)
                && crate::engine::effect_target_is_legal_from_ability_source(
                    state,
                    source,
                    ctx.controller,
                    HEARSE_TARGET,
                    &ctx.targets,
                    index,
                ))
            .then_some(object)
        })
        .collect::<Vec<_>>();
    if cards.is_empty() {
        return;
    }
    event::propose_and_commit_batch(
        state,
        cards
            .iter()
            .map(|&card| ProposedEvent::zone_change(card, Zone::Exile))
            .collect(),
    );
    if state.engine.halted.is_some() {
        return;
    }
    let hearse = ObjectLinkV4 {
        object: source.source,
        zone_change_count: source.zone_change_count,
    };
    let exiled = cards
        .into_iter()
        .filter(|&card| state.objects.get(card).zone == Zone::Exile)
        .map(|card| HearseExileV1 {
            hearse,
            card: live_link(state, card),
        })
        .collect::<Vec<_>>();
    if !exiled.is_empty() {
        s1o_state(state).hearse_exiles.extend(exiled);
    }
}

// ---- Frenzied Trapbreaker -----------------------------------------------------------

/// Destroys target 0 if it is still legal (608.2b).
fn resolve_trapbreaker(ctx: &ExecCtx, state: &mut GameState) {
    if !matches!(ctx.targets.first(), Some(Target::Object(_)))
        || !ctx.target_incarnation_matches(0, state)
        || !crate::engine::effect_target_is_legal(
            state,
            ctx.source,
            ctx.controller,
            TRAPBREAKER_TARGET,
            &ctx.targets,
            0,
        )
    {
        return;
    }
    crate::effect::execute(
        &EffectOp::DestroyObject {
            object: ObjectRef::Target(0),
        },
        ctx,
        state,
    );
}

// ---- Execution ----------------------------------------------------------------------

/// Runs one of this module's leaves in synchronous execution.
pub(crate) fn execute(op: &S1oOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        S1oOpV1::EowynEntersV1 => resolve_eowyn(ctx, state),
        S1oOpV1::HearseExileV1 => resolve_hearse(ctx, state),
        S1oOpV1::TrapbreakerAttackV1 => resolve_trapbreaker(ctx, state),
        // Choices run in `drive`.
        S1oOpV1::StrongholdGambitV1 => halt(state, ctx.source),
    }
}

/// Runs one of this module's leaves in the resumable interpreter.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &S1oOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    match op {
        S1oOpV1::StrongholdGambitV1 => {
            let first = state.active_player;
            stage_gambit(
                state,
                continuation,
                &[first, first.opponent()],
                Vec::new(),
                path,
            )
        }
        other => {
            execute(other, &continuation.ctx, state);
            Ok(false)
        }
    }
}

/// Applies an answered choice of this module.
pub(crate) fn finish_selection(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    choice: &E3ChoiceV1,
    kind: &S1oChoiceV1,
    bindings: Vec<EffectObjectBinding>,
) -> Result<bool, String> {
    match kind {
        S1oChoiceV1::GambitV1 { chosen, later } => {
            let mut chosen = chosen.clone();
            chosen.extend(bindings.first().copied());
            stage_gambit(state, continuation, later, chosen, choice.path.clone())
        }
    }
}
