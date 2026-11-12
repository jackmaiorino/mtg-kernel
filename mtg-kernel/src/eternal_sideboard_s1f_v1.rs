//! Card programs for the eternal catalog's Vintage sideboard wave S1f
//! (`docs/design/eternal_formats_support_v1.md`): Mycosynth Lattice, which
//! completes Raker Shops and Sphere Shops, and Overencumbered, which
//! completes Lurrus Breach. The Junk token Overencumbered creates is a
//! card-database recipe on the engine's impulse draw.
//!
//! `eternal_cards_v1` forwards its effect leaves, queued triggers and turn
//! bookkeeping here, wave S1e's recipe tables fall through to
//! `build_eternal_s1f_v1.rs`, and the engine calls the hooks below only in
//! `eternal-fixtures` builds. Every table is keyed by a printed name that
//! occurs only in the eternal registry. Behavior follows each card's current
//! Oracle text and rulings, checked against its XMage source.
//!
//! Primitives added here:
//! - a static ability that makes every permanent an artifact (layer 4) and
//!   every object colorless (layer 5) at its timestamp, and lets every
//!   player spend mana as though it were mana of any color (Mycosynth
//!   Lattice);
//! - an Aura that enchants a player, recorded with that player, and its
//!   state-based action (Overencumbered);
//! - a payment of {1} for each artifact a player controls, which that player
//!   may make, or creatures can't attack this turn (Overencumbered).
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{card_id_by_name, CardType, TargetSpec, CARD_DEFS};
use crate::effect::{EffectContinuation, EffectFrame, EffectOp, ExecCtx, PlayerRef};
use crate::eternal_cards_v1::EternalOpV1;
use crate::eternal_legacy_e9a_v1::TurnV1;
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::StandardOpV1;
use crate::state::{AbilitySourceContractV4, GameState, ObjectLinkV4, Target, Zone};
use crate::trigger::PendingTrigger;
use serde::{Deserialize, Serialize};

pub(crate) const MYCOSYNTH_LATTICE: &str = "Mycosynth Lattice";
pub(crate) const OVERENCUMBERED: &str = "Overencumbered";
pub(crate) const JUNK_TOKEN: &str = "Junk Token";

/// Overencumbered: "enchanted opponent creates a Clue token, a Food token,
/// and a Junk token."
const OVERENCUMBERED_TOKENS: [&str; 3] = ["Clue Token", "Food Token", JUNK_TOKEN];

/// Per-game state of this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Absent from every snapshot until one
/// of them needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct S1fStateV1 {
    /// The player each Overencumbered enchants, by the Aura's battlefield
    /// incarnation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enchanted: Vec<(ObjectLinkV4, PlayerId)>,
    /// Overencumbered's "creatures can't attack this combat": the Aura whose
    /// combat trigger wasn't paid, and that turn.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attack_bans: Vec<(ObjectId, TurnV1)>,
}

impl S1fStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

pub(crate) fn s1f_state(state: &mut GameState) -> &mut S1fStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).s1f
}

pub fn read_state(state: &GameState) -> Option<&S1fStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.s1f)
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::S1fV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum S1fOpV1 {
    /// Overencumbered's resolution: the Aura enters enchanting target
    /// opponent 0.
    OverencumberedV1,
    /// Overencumbered's entry trigger: `player`, the enchanted opponent,
    /// creates a Clue token, a Food token and a Junk token.
    OverencumberedTokensV1 { player: PlayerId },
    /// Overencumbered's combat trigger: `player` may pay {1} for each
    /// artifact they control; if they don't, creatures can't attack this
    /// turn. A choice.
    OverencumberedCombatV1 { player: PlayerId },
    /// `player` pays the combat trigger's {`generic`}.
    OverencumberedPayV1 { player: PlayerId, generic: u8 },
    /// The combat trigger unpaid: creatures can't attack this turn.
    OverencumberedBanV1,
}

impl S1fOpV1 {
    /// Whether this leaf can yield a player decision, so its program must
    /// use the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(self, Self::OverencumberedCombatV1 { .. })
    }
}

fn s1f(op: S1fOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::S1fV1(op)))
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

/// `player` as seen from the resolving ability's controller.
fn player_ref(ctx: &ExecCtx, player: PlayerId) -> PlayerRef {
    if player == ctx.controller {
        PlayerRef::Controller
    } else {
        PlayerRef::Opponent
    }
}

// ---- Programs -----------------------------------------------------------------------

/// Overencumbered: "Enchant opponent." Cast targeting an opponent, it
/// enters enchanting that player.
pub fn overencumbered() -> EffectOp {
    s1f(S1fOpV1::OverencumberedV1)
}

// ---- Mycosynth Lattice --------------------------------------------------------------

fn lattice_id() -> Option<u16> {
    static LATTICE_ID: std::sync::OnceLock<Option<u16>> = std::sync::OnceLock::new();
    *LATTICE_ID.get_or_init(|| card_id_by_name(MYCOSYNTH_LATTICE))
}

/// The timestamp of the latest Mycosynth Lattice whose abilities apply.
/// Whether a Lattice applies reads only its printed characteristics and the
/// layer 6 ability removals, none of which ask an artifact type or a color,
/// so the type and color queries below may call this.
pub(crate) fn lattice_timestamp(state: &GameState) -> Option<u64> {
    if !cfg!(feature = "eternal-fixtures") {
        return None;
    }
    let id = lattice_id()?;
    [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
        .filter(|&object| {
            state.objects.get(object).card_def == id
                && active_named(state, object, MYCOSYNTH_LATTICE)
        })
        .map(|object| state.objects.get(object).v4.layer_timestamp.unwrap_or(0))
        .max()
}

/// The layer 4 type-setting effects on `object` and their timestamps: an
/// Elk of Oko's (wave E5a), a creature Aura's override, and a land Aura's
/// ("loses all other card types").
fn type_settings(state: &GameState, object: ObjectId) -> impl Iterator<Item = u64> {
    [
        crate::eternal_e5a_v1::statics::elk_timestamp(state, object),
        crate::continuous_characteristics_v1::creature_override(state, object)
            .map(|(_, timestamp)| timestamp),
        crate::continuous_characteristics_v1::land_override_timestamp(state, object),
    ]
    .into_iter()
    .flatten()
}

/// Mycosynth Lattice: "All permanents are artifacts in addition to their
/// other types" (layer 4). Whether `object`, a permanent, is an artifact
/// because of it: a type-setting effect with a later timestamp sets its
/// types again (613.7). Face-down permanents and tokens are permanents;
/// spells and cards in other zones aren't (its 2004-12-01 ruling).
pub(crate) fn adds_artifact(state: &GameState, object: ObjectId) -> bool {
    let Some(lattice) = lattice_timestamp(state) else {
        return false;
    };
    state
        .objects
        .try_get(object)
        .is_some_and(|live| live.zone == Zone::Battlefield)
        && type_settings(state, object).all(|timestamp| timestamp < lattice)
}

/// Mycosynth Lattice: "All cards that aren't on the battlefield, spells,
/// and permanents are colorless" (layer 5), a color-setting effect at its
/// timestamp. That timestamp if the Lattice's is the latest color setting
/// on `object`: `setter` is Wild Mongrel's (wave P1a), and an Elk's green
/// or a creature Aura's colors, set later, also overwrite it (613.7).
pub(crate) fn colorless_timestamp(
    state: &GameState,
    object: ObjectId,
    setter: Option<u64>,
) -> Option<u64> {
    let lattice = lattice_timestamp(state)?;
    let overwritten = setter
        .into_iter()
        .chain(crate::eternal_e5a_v1::statics::elk_timestamp(state, object))
        .chain(
            crate::continuous_characteristics_v1::creature_override(state, object)
                .map(|(_, timestamp)| timestamp),
        )
        .any(|timestamp| timestamp > lattice);
    (!overwritten).then_some(lattice)
}

/// Mycosynth Lattice: "Players may spend mana as though it were mana of any
/// color." Whether a Lattice's abilities apply; the mana solver then lets
/// any mana pay a colored symbol.
pub(crate) fn any_color_spending(state: &GameState) -> bool {
    lattice_timestamp(state).is_some()
}

// ---- Overencumbered -----------------------------------------------------------------

fn overencumbered_id() -> Option<u16> {
    static OVERENCUMBERED_ID: std::sync::OnceLock<Option<u16>> = std::sync::OnceLock::new();
    *OVERENCUMBERED_ID.get_or_init(|| card_id_by_name(OVERENCUMBERED))
}

/// The player `aura`, an Overencumbered on the battlefield, enchants: the
/// one it entered enchanting, or its controller's opponent (303.4f).
fn enchanted_player(state: &GameState, aura: ObjectId) -> Option<PlayerId> {
    let host = battlefield_link(state, aura)?;
    let recorded = read_state(state).and_then(|s1f| {
        s1f.enchanted
            .iter()
            .find(|(entry, _)| *entry == host)
            .map(|&(_, player)| player)
    });
    Some(recorded.unwrap_or_else(|| state.objects.get(aura).controller.opponent()))
}

/// Whether `def` is an Aura that enchants a player, Overencumbered ("Enchant
/// opponent"). Put onto the battlefield other than by resolving, it enchants
/// its controller's opponent (303.4f), so effects that put Auras onto the
/// battlefield without a host may offer it.
pub(crate) fn enchants_player(def: &crate::card_def::CardDef) -> bool {
    cfg!(feature = "eternal-fixtures") && def.name == OVERENCUMBERED
}

/// 704.5m for Overencumbered ("Enchant opponent"): whether `aura` goes to
/// its owner's graveyard because the player it enchants is no longer an
/// opponent of its controller. `None` for every other object.
pub(crate) fn invalid_player_aura(state: &GameState, aura: ObjectId) -> Option<bool> {
    if !cfg!(feature = "eternal-fixtures") {
        return None;
    }
    let id = overencumbered_id()?;
    let live = state.objects.try_get(aura)?;
    if live.card_def != id || live.zone != Zone::Battlefield {
        return None;
    }
    Some(enchanted_player(state, aura) == Some(live.controller))
}

/// Overencumbered: "If they don't, creatures can't attack this combat." The
/// engine has one combat phase a turn, so the ban lasts the turn.
pub(crate) fn cant_attack(state: &GameState, _object: ObjectId) -> bool {
    read_state(state).is_some_and(|s1f| {
        !s1f.attack_bans.is_empty() && {
            let now = TurnV1::now(state);
            s1f.attack_bans.iter().any(|(_, turn)| *turn == now)
        }
    })
}

fn ban_attacks(state: &mut GameState, source: ObjectId) {
    let ban = (source, TurnV1::now(state));
    let bans = &mut s1f_state(state).attack_bans;
    if !bans.contains(&ban) {
        bans.push(ban);
    }
}

// ---- Queued triggers and turn bookkeeping -------------------------------------------

/// Target specs of this module's queued triggers: none of them targets.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    delayed_effect_matches(name, effect).then_some(TargetSpec::None)
}

/// Queued triggers owned by this module's cards: Overencumbered's two.
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    cfg!(feature = "eternal-fixtures")
        && name == OVERENCUMBERED
        && matches!(
            effect,
            EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::S1fV1(
                S1fOpV1::OverencumberedTokensV1 { .. } | S1fOpV1::OverencumberedCombatV1 { .. }
            )))
        )
}

/// Overencumbered entering: the player it enchants, then its trigger, "When
/// this Aura enters, enchanted opponent creates a Clue token, a Food token,
/// and a Junk token." Cast, it entered enchanting its target; put onto the
/// battlefield any other way, it enchants its controller's opponent
/// (303.4f).
fn note_entries(state: &mut GameState, events: &[CommittedEvent]) {
    let Some(id) = overencumbered_id() else {
        return;
    };
    for event in events {
        let CommittedEvent::ZoneChange {
            object,
            to: Zone::Battlefield,
            ..
        } = *event
        else {
            continue;
        };
        let Some(live) = state.objects.try_get(object) else {
            continue;
        };
        if live.card_def != id || live.zone != Zone::Battlefield {
            continue;
        }
        let controller = live.controller;
        let host = link(state, object);
        let recorded = read_state(state).and_then(|s1f| {
            s1f.enchanted
                .iter()
                .find(|(entry, _)| *entry == host)
                .map(|&(_, player)| player)
        });
        let player = recorded.unwrap_or_else(|| {
            let player = controller.opponent();
            s1f_state(state).enchanted.push((host, player));
            player
        });
        if active_named(state, object, OVERENCUMBERED) {
            let source = AbilitySourceContractV4::capture(state, object);
            crate::eternal_premodern_p1a_v1::queue_trigger(
                state,
                source,
                controller,
                s1f(S1fOpV1::OverencumberedTokensV1 { player }),
                None,
            );
        }
    }
}

/// Overencumbered: "At the beginning of combat on enchanted opponent's
/// turn, that player may pay {1} for each artifact they control." The
/// Aura's controller controls the trigger.
fn note_combat(state: &mut GameState, events: &[CommittedEvent]) {
    for event in events {
        let CommittedEvent::BeginningOfCombat { active_player } = *event else {
            continue;
        };
        let auras = battlefield(state)
            .filter(|&aura| {
                active_named(state, aura, OVERENCUMBERED)
                    && enchanted_player(state, aura) == Some(active_player)
            })
            .collect::<Vec<_>>();
        for aura in auras {
            let source = AbilitySourceContractV4::capture(state, aura);
            let controller = state.objects.get(aura).controller;
            crate::eternal_premodern_p1a_v1::queue_trigger(
                state,
                source,
                controller,
                s1f(S1fOpV1::OverencumberedCombatV1 {
                    player: active_player,
                }),
                None,
            );
        }
    }
}

/// The triggers this module's cards set off in this collection, queued into
/// wave P1a's queue.
pub(crate) fn delayed_triggers(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Vec<PendingTrigger> {
    if cfg!(feature = "eternal-fixtures") {
        note_entries(state, events);
        note_combat(state, events);
    }
    Vec::new()
}

/// As each turn begins the last turn's attack bans are over, and the
/// records of Auras that left the battlefield are dropped.
pub(crate) fn begin_turn(state: &mut GameState, _player: PlayerId) {
    let Some(s1f) = read_state(state) else {
        return;
    };
    if s1f.is_empty() {
        return;
    }
    let enchanted = s1f
        .enchanted
        .iter()
        .copied()
        .filter(|&(host, _)| link_live_in(state, host, Zone::Battlefield))
        .collect::<Vec<_>>();
    let s1f = s1f_state(state);
    s1f.enchanted = enchanted;
    s1f.attack_bans.clear();
}

// ---- Effect leaves ------------------------------------------------------------------

/// The artifacts `player` controls.
fn artifacts_controlled(state: &GameState, player: PlayerId) -> usize {
    state.players[player.index()]
        .battlefield
        .iter()
        .filter(|&&object| {
            state.objects.get(object).zone == Zone::Battlefield
                && crate::engine::object_has_type(state, object, CardType::Artifact)
        })
        .count()
}

/// Drives this module's choice-bearing leaf in the resumable interpreter.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &S1fOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    match op {
        S1fOpV1::OverencumberedCombatV1 { player } => {
            // "That player may pay {1} for each artifact they control", counted
            // on resolution. {0} costs nothing, so it is paid.
            let player = *player;
            let artifacts = artifacts_controlled(state, player);
            if artifacts == 0 {
                return Ok(false);
            }
            let payable = u8::try_from(artifacts)
                .ok()
                .filter(|&generic| crate::engine::can_pay_effect_mana(player, &[], generic, state));
            let Some(generic) = payable else {
                ban_attacks(state, continuation.ctx.source);
                return Ok(false);
            };
            continuation.frames.push(EffectFrame::Program {
                op: EffectOp::Choice {
                    controller: player_ref(&continuation.ctx, player),
                    options: vec![
                        s1f(S1fOpV1::OverencumberedPayV1 { player, generic }),
                        s1f(S1fOpV1::OverencumberedBanV1),
                    ],
                },
                path,
            });
            Ok(false)
        }
        _ => {
            let ctx = continuation.ctx.clone();
            execute(op, &ctx, state);
            Ok(false)
        }
    }
}

pub(crate) fn execute(op: &S1fOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        S1fOpV1::OverencumberedV1 => {
            // The resolution checked the target (608.2b); the Aura enters
            // enchanting that opponent.
            let Some(Target::Player(player)) = ctx.targets.first().copied() else {
                halt(state, ctx.source);
                return;
            };
            if player == ctx.controller || state.objects.get(ctx.source).zone != Zone::Stack {
                halt(state, ctx.source);
                return;
            }
            event::propose_and_commit(
                state,
                ProposedEvent::zone_change(ctx.source, Zone::Battlefield),
            );
            if let Some(host) = battlefield_link(state, ctx.source) {
                let enchanted = &mut s1f_state(state).enchanted;
                enchanted.retain(|(entry, _)| *entry != host);
                enchanted.push((host, player));
            }
        }
        S1fOpV1::OverencumberedTokensV1 { player } => {
            let creator = player_ref(ctx, *player);
            for name in OVERENCUMBERED_TOKENS {
                let Some(token_def) = card_id_by_name(name) else {
                    halt(state, ctx.source);
                    return;
                };
                crate::effect::execute(
                    &EffectOp::CreateToken {
                        token_def,
                        controller: creator,
                    },
                    ctx,
                    state,
                );
            }
        }
        // Staged by `drive`; the program always runs in the resumable
        // interpreter.
        S1fOpV1::OverencumberedCombatV1 { .. } => halt(state, ctx.source),
        S1fOpV1::OverencumberedPayV1 { player, generic } => {
            if !crate::engine::pay_effect_mana(*player, &[], *generic, state) {
                ban_attacks(state, ctx.source);
            }
        }
        S1fOpV1::OverencumberedBanV1 => ban_attacks(state, ctx.source),
    }
}
