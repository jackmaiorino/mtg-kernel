//! Card programs for the eternal catalog's Premodern sideboard wave P3e
//! (`docs/design/eternal_formats_support_v1.md`): the sideboard cards that
//! complete Madness Threshold.
//!
//! `eternal_cards_v1` forwards its effect leaves and turn bookkeeping here,
//! wave P3d's recipe tables fall through to `build_eternal_p3e_v1.rs`, and
//! the engine calls the hooks below only in `eternal-fixtures` builds. Every
//! table is keyed by a printed name that occurs only in the eternal
//! registry. Behavior follows each card's current Oracle text and rulings.
//!
//! Primitives added here:
//! - all creatures have protection from a color (Absolute Law);
//! - prevention of the combat damage one attacking creature would deal this
//!   turn, and not of the damage dealt to it (Kor Haven);
//! - an untap step that untaps at most one land, the active player choosing
//!   which (Winter Orb).
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{mana_color_mask, CardType, TargetSpec};
use crate::effect::{EffectOp, ExecCtx};
use crate::eternal_cards_v1::EternalOpV1;
use crate::eternal_legacy_e9a_v1::{active_named, battlefield, link_live_in, TurnV1};
use crate::eternal_premodern_p1a_v1::legal_object_target;
use crate::event::DamageProposed;
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::standard_cards_v1::StandardOpV1;
use crate::state::{GameState, ObjectLinkV4, Zone};
use serde::{Deserialize, Serialize};

pub(crate) const ABSOLUTE_LAW: &str = "Absolute Law";
pub(crate) const WINTER_ORB: &str = "Winter Orb";

/// Per-game state of this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Absent from every snapshot until one
/// of them needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct P3eStateV1 {
    /// Each attacking creature whose combat damage a Kor Haven prevents,
    /// as its battlefield incarnation, with the turn it lasts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub havens: Vec<(ObjectLinkV4, TurnV1)>,
    /// The untap step's pending Winter Orb choice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orb_untap: Option<OrbUntapV1>,
}

impl P3eStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// An untap step that Winter Orb limited to one land, waiting for the
/// active player to choose it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrbUntapV1 {
    /// The active player, who chooses.
    pub player: PlayerId,
    /// The Winter Orb whose restriction applies: the decision's source.
    pub orb: ObjectId,
    /// The lands that would have untapped, in the step's order. Option `i`
    /// untaps the `i`th.
    pub lands: Vec<ObjectLinkV4>,
}

pub(crate) fn p3e_state(state: &mut GameState) -> &mut P3eStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).p3e
}

pub fn read_state(state: &GameState) -> Option<&P3eStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.p3e)
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::P3eV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum P3eOpV1 {
    /// Kor Haven: prevent all combat damage that would be dealt by target
    /// attacking creature 0 this turn.
    KorHavenV1,
}

impl P3eOpV1 {
    /// Whether this leaf can yield a player decision or pushes frames. None
    /// of this module's leaves does.
    pub(crate) fn contains_player_choice(&self) -> bool {
        false
    }
}

fn p3e(op: P3eOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::P3eV1(op)))
}

fn link(state: &GameState, object: ObjectId) -> ObjectLinkV4 {
    ObjectLinkV4 {
        object,
        zone_change_count: state.objects.get(object).zone_change_count,
    }
}

struct StaticIds {
    absolute_law: Option<u16>,
    winter_orb: Option<u16>,
}

fn static_ids() -> &'static StaticIds {
    static IDS: std::sync::OnceLock<StaticIds> = std::sync::OnceLock::new();
    IDS.get_or_init(|| StaticIds {
        absolute_law: crate::card_def::card_id_by_name(ABSOLUTE_LAW),
        winter_orb: crate::card_def::card_id_by_name(WINTER_ORB),
    })
}

/// The battlefield permanents with definition `id`, named `name`, whose
/// printed abilities apply, whoever controls them. Compares definition ids
/// first.
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

// ---- Programs -----------------------------------------------------------------------

/// Kor Haven: "{1}{W}, {T}: Prevent all combat damage that would be dealt
/// by target attacking creature this turn."
pub fn kor_haven() -> EffectOp {
    p3e(P3eOpV1::KorHavenV1)
}

// ---- Statics: Absolute Law ----------------------------------------------------------

/// Absolute Law: "All creatures have protection from red." The red mask for
/// a creature on the battlefield while an Absolute Law's abilities apply,
/// whoever controls either (its 2004-10-04 ruling). Wave E6b's checks apply
/// it to targeting, blocking, damage and attachments.
pub(crate) fn protection_mask(state: &GameState, host: ObjectId) -> u8 {
    let ids = static_ids();
    if ids.absolute_law.is_none()
        || state
            .objects
            .try_get(host)
            .is_none_or(|live| live.zone != Zone::Battlefield)
        || active_with_id(state, ids.absolute_law, ABSOLUTE_LAW)
            .next()
            .is_none()
        || !crate::engine::object_has_type(state, host, CardType::Creature)
    {
        return 0;
    }
    mana_color_mask(ManaColor::R)
}

// ---- Statics: Kor Haven -------------------------------------------------------------

/// Kor Haven: whether `damage` is combat damage dealt by a creature that a
/// Kor Haven chose this turn, in the incarnation it chose. Damage dealt to
/// that creature is not prevented.
pub(crate) fn prevents_damage(state: &GameState, damage: &DamageProposed) -> bool {
    if !damage.is_combat {
        return false;
    }
    let Some(p3e) = read_state(state) else {
        return false;
    };
    if p3e.havens.is_empty() || state.objects.try_get(damage.source).is_none() {
        return false;
    }
    let source = link(state, damage.source);
    let now = TurnV1::now(state);
    p3e.havens
        .iter()
        .any(|(creature, turn)| *creature == source && *turn == now)
}

// ---- Statics: Winter Orb ------------------------------------------------------------

/// Winter Orb: "As long as Winter Orb is untapped, players can't untap more
/// than one land during their untap steps." The untapped Winter Orb whose
/// restriction applies to the untap step about to untap permanents, read
/// before anything untaps: an Orb that is tapped as the step begins untaps
/// with everything else, and so do all lands (its 2016-06-08 ruling).
pub(crate) fn untap_limited(state: &GameState) -> Option<ObjectId> {
    active_with_id(state, static_ids().winter_orb, WINTER_ORB)
        .find(|&orb| !state.objects.get(orb).tapped)
}

/// Whether the limited untap step holds `object` back: a tapped land that
/// would otherwise untap now.
pub(crate) fn holds_untap(state: &GameState, object: ObjectId) -> bool {
    state.objects.get(object).tapped
        && crate::engine::object_has_type(state, object, CardType::Land)
}

/// Untaps one of the `lands` the limited step held back for `player`: the
/// only one, or the one `player` chooses (`entry_choice_decision`) before
/// the step ends. A chosen land with a stun counter loses a counter instead.
pub(crate) fn untap_one_land(
    state: &mut GameState,
    player: PlayerId,
    orb: ObjectId,
    lands: Vec<ObjectId>,
) {
    match lands.as_slice() {
        [] => {}
        [land] => {
            crate::engine::attempt_untap(state, *land);
        }
        _ => {
            let lands = lands.into_iter().map(|land| link(state, land)).collect();
            p3e_state(state).orb_untap = Some(OrbUntapV1 { player, orb, lands });
        }
    }
}

/// Winter Orb's pending untap choice, as a decision for the active player:
/// an option for each held land, in the step's order.
pub(crate) fn entry_choice_decision(state: &GameState) -> Option<crate::engine::Decision> {
    let pending = read_state(state)?.orb_untap.as_ref()?;
    Some(crate::engine::Decision::ChooseEffectOption {
        player: pending.player,
        source: pending.orb,
        option_count: u16::try_from(pending.lands.len()).unwrap_or(u16::MAX),
    })
}

/// Answers Winter Orb's pending untap choice by untapping the chosen land.
/// `Ok(false)` when no such choice is pending.
pub(crate) fn answer_entry_choice(state: &mut GameState, option: u16) -> Result<bool, String> {
    let Some(pending) = read_state(state).and_then(|p3e| p3e.orb_untap.as_ref()) else {
        return Ok(false);
    };
    let land = *pending
        .lands
        .get(usize::from(option))
        .ok_or("the land option is out of range")?;
    p3e_state(state).orb_untap = None;
    if link_live_in(state, land, Zone::Battlefield) {
        crate::engine::attempt_untap(state, land.object);
    }
    Ok(true)
}

// ---- Turn bookkeeping ---------------------------------------------------------------

/// The beginning of a turn ends the earlier turns' Kor Haven preventions.
pub(crate) fn begin_turn(state: &mut GameState, _player: PlayerId) {
    let now = TurnV1::now(state);
    if read_state(state).is_none_or(|p3e| p3e.havens.iter().all(|(_, turn)| *turn == now)) {
        return;
    }
    p3e_state(state).havens.retain(|(_, turn)| *turn == now);
}

// ---- Effect leaves ------------------------------------------------------------------

pub(crate) fn execute(op: &P3eOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        P3eOpV1::KorHavenV1 => {
            let Some(creature) = legal_object_target(ctx, state, 0, TargetSpec::AttackingCreature)
            else {
                return;
            };
            let record = (link(state, creature), TurnV1::now(state));
            let havens = &mut p3e_state(state).havens;
            if !havens.contains(&record) {
                havens.push(record);
            }
        }
    }
}
