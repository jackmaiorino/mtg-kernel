//! Card programs and static rules for the eternal catalog's sideboard wave S1n
//! (`docs/design/eternal_formats_support_v1.md`): Apostle's Blessing
//! (completing Vintage Scam's 75), Doorkeeper Thrull (completing Legacy Bant
//! Control's 75), Darksteel Angel (Vintage Paradoxical Outcome), From the
//! Ashes (completing Legacy Jeskai Control's 75) and Kiora's Dismissal
//! (completing Legacy LED Dredge's 75), alongside promoting Martyr of Sands to
//! Full.
//!
//! `eternal_cards_v1` forwards this module's effect leaves and per-game state,
//! wave P3h's recipe tables fall through to `build_eternal_s1n_v1.rs`, and the
//! engine calls the static hooks below only in `eternal-fixtures` builds.
//! Every table is keyed by a printed name that occurs only in the eternal
//! registry.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

pub(crate) mod targets;

use crate::card_def::{
    card_id_by_name, mana_color_mask, CardType, Keywords, Supertype, TargetSpec,
};
use crate::effect::{
    EffectContinuation, EffectFrame, EffectOp, ExecCtx, LibraryCardFilter,
    LibrarySearchDestinationV1, PlayerRef,
};
use crate::eternal_cards_v1::EternalOpV1;
use crate::eternal_legacy_e9a_v1::{link_live_in, live_link, TurnV1};
use crate::event::{self, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::standard_cards_v1::{StandardOpV1, StandardTargetV1};
use crate::state::{GameState, ObjectLinkV4, Target, Zone};
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

pub(crate) const DARKSTEEL_ANGEL: &str = "Darksteel Angel";
pub(crate) const KIORAS_DISMISSAL: &str = "Kiora's Dismissal";

/// Apostle's Blessing's target: "target artifact or creature you control".
pub(crate) const APOSTLES_BLESSING_TARGET: TargetSpec =
    TargetSpec::StandardV1(StandardTargetV1::ControlledArtifactOrCreatureV1);

/// Kiora's Dismissal's target: "any number of target enchantments".
pub(crate) const KIORAS_DISMISSAL_TARGET: TargetSpec =
    TargetSpec::StandardV1(StandardTargetV1::AnyNumberEnchantmentsV1);

/// The quality Apostle's Blessing grants protection from until end of turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum S1nProtectionV1 {
    Artifacts,
    Color(ManaColor),
}

/// One resolved Apostle's Blessing protection grant on a permanent incarnation
/// until the end of `turn`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlessingV1 {
    pub object: ObjectLinkV4,
    pub protection: S1nProtectionV1,
    pub turn: TurnV1,
}

/// Per-game state of this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Absent from every snapshot and state
/// hash until one of them needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct S1nStateV1 {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blessings: Vec<BlessingV1>,
}

impl S1nStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        self.blessings.is_empty()
    }
}

impl Hash for S1nStateV1 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        if !self.is_empty() {
            b"eternal-s1n/v1".hash(state);
            self.blessings.hash(state);
        }
    }
}

pub(crate) fn s1n_state(state: &mut GameState) -> &mut S1nStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).s1n
}

pub fn read_state(state: &GameState) -> Option<&S1nStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.s1n)
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::S1nV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum S1nOpV1 {
    /// Apostle's Blessing's chosen protection on target 0 until end of turn.
    ApostlesBlessingV1 { protection: S1nProtectionV1 },
    /// From the Ashes: destroy all nonbasic lands, then in APNAP order each
    /// controller of one or more lands destroyed this way may search their
    /// library for up to that many basic land cards, put them onto the
    /// battlefield, then shuffle if they searched.
    FromTheAshesV1,
    /// Kiora's Dismissal: return each still-legal target enchantment to its
    /// owner's hand simultaneously.
    KiorasDismissalV1,
}

impl S1nOpV1 {
    /// `FromTheAshesV1` stages per-player optional basic-land searches in the
    /// resumable interpreter after simultaneous destruction commits.
    pub(crate) fn contains_player_choice(&self) -> bool {
        match self {
            Self::ApostlesBlessingV1 { .. } | Self::KiorasDismissalV1 => false,
            Self::FromTheAshesV1 => true,
        }
    }
}

fn s1n(op: S1nOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::S1nV1(op)))
}

// ---- Programs -----------------------------------------------------------------------

/// Apostle's Blessing: "Target artifact or creature you control gains
/// protection from artifacts or from the color of your choice until end of
/// turn."
pub fn apostles_blessing() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: [
            S1nProtectionV1::Artifacts,
            S1nProtectionV1::Color(ManaColor::W),
            S1nProtectionV1::Color(ManaColor::U),
            S1nProtectionV1::Color(ManaColor::B),
            S1nProtectionV1::Color(ManaColor::R),
            S1nProtectionV1::Color(ManaColor::G),
        ]
        .into_iter()
        .map(|protection| s1n(S1nOpV1::ApostlesBlessingV1 { protection }))
        .collect(),
    }
}

/// From the Ashes: "Destroy all nonbasic lands. For each land destroyed this
/// way, its controller may search their library for a basic land card and put
/// it onto the battlefield. Then each player who searched their library this
/// way shuffles."
pub fn from_the_ashes() -> EffectOp {
    s1n(S1nOpV1::FromTheAshesV1)
}

/// Kiora's Dismissal: "Return any number of target enchantments to their
/// owners' hands." Strive (`{U}` per target beyond the first) is priced in
/// `spell_costs_v1::selected_spell_mana_costs_v1`.
pub fn kioras_dismissal() -> EffectOp {
    s1n(S1nOpV1::KiorasDismissalV1)
}

// ---- Statics and turn cleanup -------------------------------------------------------

/// Prunes expired Apostle's Blessing grants as a new turn begins.
pub(crate) fn begin_turn(state: &mut GameState, _player: PlayerId) {
    let now = TurnV1::now(state);
    let Some(eternal) = state.eternal_v1.as_mut() else {
        return;
    };
    if eternal.s1n.blessings.is_empty() {
        return;
    }
    eternal.s1n.blessings.retain(|entry| entry.turn == now);
}

/// Color protection mask Apostle's Blessing currently grants `host`.
pub(crate) fn protection_mask(state: &GameState, host: ObjectId) -> u8 {
    let Some(s1n) = read_state(state) else {
        return 0;
    };
    if s1n.blessings.is_empty() {
        return 0;
    }
    let Some(live) = state.objects.try_get(host) else {
        return 0;
    };
    if live.zone != Zone::Battlefield {
        return 0;
    }
    let link = ObjectLinkV4 {
        object: host,
        zone_change_count: live.zone_change_count,
    };
    let now = TurnV1::now(state);
    s1n.blessings
        .iter()
        .filter(|entry| entry.object == link && entry.turn == now)
        .fold(0, |mask, entry| match entry.protection {
            S1nProtectionV1::Artifacts => mask,
            S1nProtectionV1::Color(color) => mask | mana_color_mask(color),
        })
}

/// Whether Apostle's Blessing currently grants `host` protection from
/// artifacts.
pub(crate) fn has_protection_from_artifacts(state: &GameState, host: ObjectId) -> bool {
    let Some(s1n) = read_state(state) else {
        return false;
    };
    if s1n.blessings.is_empty() {
        return false;
    }
    let Some(live) = state.objects.try_get(host) else {
        return false;
    };
    if live.zone != Zone::Battlefield {
        return false;
    }
    let link = ObjectLinkV4 {
        object: host,
        zone_change_count: live.zone_change_count,
    };
    let now = TurnV1::now(state);
    s1n.blessings.iter().any(|entry| {
        entry.object == link && entry.turn == now && entry.protection == S1nProtectionV1::Artifacts
    })
}

/// Whether `player` controls a battlefield Darksteel Angel whose printed
/// abilities are active (not removed by Oko, Dress Down or Humility).
pub(crate) fn controls_darksteel_angel(state: &GameState, player: PlayerId) -> bool {
    if !cfg!(feature = "eternal-fixtures") {
        return false;
    }
    let Some(angel_id) = card_id_by_name(DARKSTEEL_ANGEL) else {
        return false;
    };
    state.players[player.index()]
        .battlefield
        .iter()
        .copied()
        .any(|object| {
            state
                .objects
                .try_get(object)
                .is_some_and(|live| live.zone == Zone::Battlefield && live.card_def == angel_id)
                && crate::continuous_characteristics_v1::printed_abilities_active(state, object)
        })
}

/// Darksteel Angel: "Creatures you control can't have -1/-1 counters put on
/// them."
pub(crate) fn prevents_minus1_minus1_counters_on(state: &GameState, object: ObjectId) -> bool {
    if !cfg!(feature = "eternal-fixtures") {
        return false;
    }
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    live.zone == Zone::Battlefield
        && crate::engine::object_has_type(state, object, CardType::Creature)
        && controls_darksteel_angel(state, live.controller)
}

// ---- Execution ----------------------------------------------------------------------

fn resolve_apostles_blessing(ctx: &ExecCtx, state: &mut GameState, protection: S1nProtectionV1) {
    let Some(Target::Object(object)) = ctx.targets.first().copied() else {
        return;
    };
    if !ctx.target_incarnation_matches(0, state)
        || !crate::engine::effect_target_is_legal(
            state,
            ctx.source,
            ctx.controller,
            APOSTLES_BLESSING_TARGET,
            &ctx.targets,
            0,
        )
    {
        return;
    }
    let record = BlessingV1 {
        object: live_link(state, object),
        protection,
        turn: TurnV1::now(state),
    };
    let blessings = &mut s1n_state(state).blessings;
    if !blessings.contains(&record) {
        blessings.push(record);
    }
}

/// Simultaneously destroys every non-indestructible nonbasic land on the
/// battlefield (allowing regeneration shields per 701.8), returning how many
/// lands each player controlled among those actually destroyed (`[P0, P1]`).
fn destroy_all_nonbasic_lands(state: &mut GameState) -> [usize; 2] {
    let candidates = [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
        .filter(|&object| crate::engine::object_has_type(state, object, CardType::Land))
        .filter(|&object| {
            !crate::engine::effective_supertypes(state, object).contains(&Supertype::Basic)
        })
        .filter(|&object| {
            !crate::engine::has_effective_keyword(state, object, Keywords::INDESTRUCTIBLE)
        })
        .collect::<Vec<_>>();
    let doomed = crate::effect::without_regenerated_v1(state, candidates);
    if doomed.is_empty() {
        return [0, 0];
    }
    let tracked = doomed
        .iter()
        .map(|&object| {
            (
                live_link(state, object),
                state.objects.get(object).controller,
            )
        })
        .collect::<Vec<_>>();
    event::propose_and_commit_batch(
        state,
        doomed
            .into_iter()
            .map(|object| ProposedEvent::zone_change(object, Zone::Graveyard))
            .collect(),
    );
    if state.engine.halted.is_some() || crate::graveyard_order_v1::paused(state) {
        return [0, 0];
    }
    let mut destroyed = [0usize; 2];
    for (link, controller) in tracked {
        if !link_live_in(state, link, Zone::Battlefield) {
            destroyed[controller.index()] += 1;
        }
    }
    destroyed
}

/// Returns each still-legal target enchantment of Kiora's Dismissal to its
/// owner's hand simultaneously (608.2b).
fn resolve_kioras_dismissal(ctx: &ExecCtx, state: &mut GameState) {
    let events = ctx
        .targets
        .iter()
        .enumerate()
        .filter_map(|(index, target)| {
            let Target::Object(object) = *target else {
                return None;
            };
            (ctx.target_incarnation_matches(index, state)
                && crate::engine::effect_target_is_legal(
                    state,
                    ctx.source,
                    ctx.controller,
                    KIORAS_DISMISSAL_TARGET,
                    &ctx.targets,
                    index,
                ))
            .then(|| ProposedEvent::zone_change_preserving_known_identity(object, Zone::Hand))
        })
        .collect::<Vec<_>>();
    if !events.is_empty() {
        event::propose_and_commit_batch(state, events);
    }
}

/// Runs one of this module's leaves in synchronous execution.
pub(crate) fn execute(op: &S1nOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        S1nOpV1::ApostlesBlessingV1 { protection } => {
            resolve_apostles_blessing(ctx, state, *protection);
        }
        S1nOpV1::KiorasDismissalV1 => {
            resolve_kioras_dismissal(ctx, state);
        }
        S1nOpV1::FromTheAshesV1 => {
            state.engine.halted = Some((
                crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                ctx.source,
            ));
        }
    }
}

/// Runs one of this module's leaves in the resumable interpreter.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &S1nOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    match op {
        S1nOpV1::ApostlesBlessingV1 { protection } => {
            resolve_apostles_blessing(&continuation.ctx, state, *protection);
            Ok(false)
        }
        S1nOpV1::KiorasDismissalV1 => {
            resolve_kioras_dismissal(&continuation.ctx, state);
            Ok(false)
        }
        S1nOpV1::FromTheAshesV1 => {
            let destroyed = destroy_all_nonbasic_lands(state);
            if state.engine.halted.is_some() || crate::graveyard_order_v1::paused(state) {
                return Ok(false);
            }
            let active = state.active_player;
            let nonactive = active.opponent();
            let controller = continuation.ctx.controller;
            // Stage choices in APNAP order (101.4); `EffectOp::Sequence` pushes
            // frames in reverse so the active player chooses first.
            let mut staged = Vec::new();
            for player in [active, nonactive] {
                let count = destroyed[player.index()];
                if count == 0 {
                    continue;
                }
                let player_ref = if player == controller {
                    PlayerRef::Controller
                } else {
                    PlayerRef::Opponent
                };
                let max_targets = u16::try_from(count).unwrap_or(u16::MAX);
                staged.push(EffectOp::Choice {
                    controller: player_ref,
                    options: vec![
                        EffectOp::SearchLibraryCardsToDestination {
                            player: player_ref,
                            filter: LibraryCardFilter::BasicLand,
                            max_targets,
                            destination: LibrarySearchDestinationV1::Battlefield { tapped: false },
                        },
                        EffectOp::Sequence(Vec::new()),
                    ],
                });
            }
            if !staged.is_empty() {
                continuation.frames.push(EffectFrame::Program {
                    op: EffectOp::Sequence(staged),
                    path,
                });
            }
            Ok(false)
        }
    }
}
