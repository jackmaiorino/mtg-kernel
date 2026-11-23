//! Card programs for the eternal catalog's Vintage Cube wave C7a
//! (`docs/design/eternal_formats_support_v1.md`): partial promotions, deferred
//! C6 cards, and Brain Freeze Storm fix.
//!
//! `eternal_cards_v1` forwards this module's effect leaves here, and the
//! recipe tables of `build_eternal_c7a_v1.rs` name its programs. Every
//! table is keyed by a printed name that occurs only in the eternal registry,
//! so the Pauper, FDN and Standard catalogs never reach it. Behavior was read
//! from each card's current Oracle text and, where present, its XMage source.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{CardType, Keywords, Subtype, TargetSpec, CARD_DEFS};
use crate::effect::{
    EffectContinuation, EffectFrame, EffectObjectBinding, EffectOp, ExecCtx, ImpulseDuration,
    PlayerRef, TargetRef,
};
use crate::engine::{FreeCastV1, PlayOrCast, PlayPermission, PlayPermissionExpiry};
use crate::eternal_cards_v1::e3::{E3ChoiceKindV1, E3ChoiceV1};
use crate::eternal_cards_v1::EternalOpV1;
use crate::eternal_cube_c6c_v1::EmblemV1;
use crate::eternal_legacy_e9a_v1::TurnV1;
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::standard_cards_v1::{StandardOpV1, StandardTargetV1};
use crate::state::{
    AbilitySourceContractV4, FaceDownV1, GameState, ObjectLinkV4, SpellCastRouteV4, StackItemKind,
    Target, Zone,
};
use crate::trigger::{PendingTrigger, TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

const SCARLETT: &str = "Headliner Scarlett";
const GANDALF: &str = "Gandalf, Goblins' Bane";
const INTI: &str = "Inti, Seneschal of the Sun";
const MALCOLM: &str = "Malcolm, Alluring Scoundrel";
const CHANDRA: &str = "Chandra, Torch of Defiance";
const METAMORPH: &str = "Phyrexian Metamorph";

/// One face-down exiled card granted a play permission by Scarlett or
/// Gandalf's Flameshape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConcealedExileV1 {
    pub card: ObjectLinkV4,
    pub holder: PlayerId,
    pub flameshape: bool,
}

/// Per-game state owned by wave C7a. Implements `Hash` manually so an empty
/// struct hashes zero bytes and preserves all existing snapshot hashes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct C7aStateV1 {
    /// Headliner Scarlett's ETB: creatures controlled by `player` cannot
    /// block during `turn`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cant_block_players: Vec<(PlayerId, TurnV1)>,
    /// Face-down exiled cards tracked for abort restoration and Flameshape's
    /// Wizard-control dormancy check.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub concealed_exile_cards: Vec<ConcealedExileV1>,
    /// Metamorphosis Fanatic's returning creature graveyard incarnation,
    /// receiving its lifelink counter as it enters in `apply_entry_counters`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_lifelink_entry: Option<ObjectLinkV4>,
    /// Metamorphosis Fanatic's returning Phyrexian Metamorph graveyard
    /// incarnation and chosen copied `card_def`, applied on entry before
    /// entry observers and copied ETB triggers run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_metamorph_copy: Option<(ObjectLinkV4, u16)>,
    /// Malcolm's chorus counters, keyed by battlefield incarnation (kept for
    /// LKI if Malcolm leaves before its combat-damage trigger resolves).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chorus_counters: Vec<(ObjectLinkV4, u32)>,
    /// Exact graveyard incarnations authorized for Malcolm's free resolution
    /// cast.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub malcolm_free_casts: Vec<ObjectLinkV4>,
    /// Exact exile incarnations authorized for Chandra's paid resolution cast.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chandra_paid_casts: Vec<ObjectLinkV4>,
    /// Chandra, Torch of Defiance's -7 emblems.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chandra_emblems: Vec<EmblemV1>,
}

impl C7aStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        self.cant_block_players.is_empty()
            && self.concealed_exile_cards.is_empty()
            && self.pending_lifelink_entry.is_none()
            && self.pending_metamorph_copy.is_none()
            && self.chorus_counters.is_empty()
            && self.malcolm_free_casts.is_empty()
            && self.chandra_paid_casts.is_empty()
            && self.chandra_emblems.is_empty()
    }
}

impl std::hash::Hash for C7aStateV1 {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        if self.is_empty() {
            return;
        }
        self.cant_block_players.hash(state);
        self.concealed_exile_cards.hash(state);
        self.pending_lifelink_entry.hash(state);
        self.pending_metamorph_copy.hash(state);
        self.chorus_counters.hash(state);
        self.malcolm_free_casts.hash(state);
        self.chandra_paid_casts.hash(state);
        self.chandra_emblems.hash(state);
    }
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::C7aV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C7aOpV1 {
    /// Headliner Scarlett ETB: creatures target player controls can't block
    /// this turn.
    ScarlettCantBlockV1,
    /// Headliner Scarlett upkeep: exile top of library face down, may look at
    /// and play this turn.
    ScarlettUpkeepExileV1,
    /// Gandalf, Goblins' Bane front-face trigger: +1/+1 EOT and 1 damage to
    /// each opponent.
    GandalfNoncreatureV1,
    /// Flameshape (Gandalf's Adventure): look at top 2, exile face down, may
    /// play while exiled if you control a Wizard.
    FlameshapeV1,
    /// Metamorphosis Fanatic ETB: return up to one target creature card from
    /// own graveyard with an entry-time lifelink counter (and as-enters copy
    /// choice for Phyrexian Metamorph).
    FanaticEntryV1,
    /// Inti attack trigger: may discard a card; when you do, push reflexive
    /// trigger.
    IntiAttackV1,
    /// Inti reflexive trigger: put a +1/+1 counter on target attacking
    /// creature; it gains trample until end of turn.
    IntiReflexiveV1,
    /// Malcolm combat-damage trigger step 1: put a chorus counter on Malcolm
    /// if its battlefield incarnation is still live.
    MalcolmCounterV1,
    /// Malcolm combat-damage trigger step 3: discard a card; if >= 4 chorus
    /// counters (LKI), may cast that exact discarded card without paying its
    /// mana cost.
    MalcolmDiscardAndMaybeCastV1,
    /// Chandra +1 (exile): exile top card; may cast it paying its mana cost;
    /// otherwise Chandra deals 2 damage to each opponent.
    ChandraExileCastOrBurnV1,
    /// Follow-up frame for Chandra +1: deal 2 damage to each opponent if the
    /// exiled card was not cast.
    ChandraBurnIfNotCastV1 { card: ObjectLinkV4 },
    /// Chandra -7: create persistent spell-cast damage emblem.
    ChandraEmblemV1,
    /// Chandra -7 emblem trigger: deal 5 damage to any target.
    ChandraEmblemDamageV1,
}

impl C7aOpV1 {
    /// Whether the leaf may stop for a player's choice (or a draw that a
    /// replacement can interrupt), so it runs in the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(
            self,
            Self::FanaticEntryV1
                | Self::IntiAttackV1
                | Self::MalcolmDiscardAndMaybeCastV1
                | Self::ChandraExileCastOrBurnV1
                | Self::ChandraBurnIfNotCastV1 { .. }
        )
    }
}

/// Resolution choices owned by this module, carried as
/// `E3ChoiceKindV1::C7aV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C7aChoiceV1 {
    /// Metamorphosis Fanatic returning Phyrexian Metamorph: optional copy of
    /// an artifact or creature on the battlefield.
    FanaticMetamorphCopyV1 { card: EffectObjectBinding },
    /// Inti's attack trigger: optional private discard from hand.
    IntiAttackDiscardV1,
    /// Malcolm's combat-damage trigger: mandatory private discard from hand.
    MalcolmDiscardV1,
    /// Malcolm's >= 4 chorus counters optional free graveyard cast of the
    /// exact discarded card.
    MalcolmFreeCastV1,
    /// Chandra +1's optional paid cast of the exact exiled card.
    ChandraPaidCastV1,
}

impl C7aChoiceV1 {
    pub(crate) fn chooser_private(&self) -> bool {
        matches!(self, Self::IntiAttackDiscardV1 | Self::MalcolmDiscardV1)
    }

    pub(crate) fn public_purpose(&self) -> crate::rl::TargetSelectionPurposeV4 {
        match self {
            Self::FanaticMetamorphCopyV1 { .. } => {
                crate::rl::TargetSelectionPurposeV4::PermanentSelection
            }
            Self::IntiAttackDiscardV1
            | Self::MalcolmDiscardV1
            | Self::MalcolmFreeCastV1
            | Self::ChandraPaidCastV1 => crate::rl::TargetSelectionPurposeV4::CardSelection,
        }
    }

    pub(crate) fn referenced_bindings(&self) -> Vec<EffectObjectBinding> {
        match *self {
            Self::FanaticMetamorphCopyV1 { card } => vec![card],
            Self::IntiAttackDiscardV1
            | Self::MalcolmDiscardV1
            | Self::MalcolmFreeCastV1
            | Self::ChandraPaidCastV1 => Vec::new(),
        }
    }
}

// ---- Helpers ------------------------------------------------------------------------

fn leaf(op: C7aOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C7aV1(op)))
}

fn read_state(state: &GameState) -> Option<&C7aStateV1> {
    state.eternal_v1.as_ref().map(|eternal| &eternal.c7a)
}

fn c7a_state(state: &mut GameState) -> &mut C7aStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).c7a
}

fn live_link(state: &GameState, object: ObjectId) -> ObjectLinkV4 {
    ObjectLinkV4 {
        object,
        zone_change_count: state.objects.get(object).zone_change_count,
    }
}

fn bind(state: &GameState, object: ObjectId) -> EffectObjectBinding {
    let live = state.objects.get(object);
    EffectObjectBinding {
        object,
        expected_zone: live.zone,
        expected_zone_change_count: live.zone_change_count,
    }
}

fn link_live_in(state: &GameState, link: ObjectLinkV4, zone: Zone) -> bool {
    state
        .objects
        .try_get(link.object)
        .is_some_and(|live| live.zone == zone && live.zone_change_count == link.zone_change_count)
}

fn hand_bindings(state: &GameState, player: PlayerId) -> Vec<EffectObjectBinding> {
    state.players[player.index()]
        .hand
        .iter()
        .map(|&card| bind(state, card))
        .collect()
}

fn offer(
    continuation: &mut EffectContinuation,
    kind: C7aChoiceV1,
    player: PlayerId,
    bindings: Vec<EffectObjectBinding>,
    min: u16,
    max: u16,
    path: Vec<u16>,
) {
    let candidates = bindings.iter().map(|b| Target::Object(b.object)).collect();
    let remaining = continuation.frames.clone();
    crate::eternal_cards_v1::e3::offer(
        continuation,
        E3ChoiceV1 {
            kind: E3ChoiceKindV1::C7aV1(kind),
            player,
            candidates,
            bindings,
            min,
            max,
            ordered: false,
            path,
            remaining,
        },
    );
}

fn controls_wizard(state: &GameState, player: PlayerId) -> bool {
    state.players[player.index()]
        .battlefield
        .iter()
        .any(|&object| crate::engine::has_effective_subtype(state, object, Subtype::Wizard))
}

fn prune_cast_markers(state: &mut GameState) {
    let malcolm_keep = read_state(state)
        .map(|c7a| {
            c7a.malcolm_free_casts
                .iter()
                .copied()
                .filter(|&link| {
                    link_live_in(state, link, Zone::Graveyard)
                        || link.zone_change_count.checked_add(1).is_some_and(|gen| {
                            link_live_in(
                                state,
                                ObjectLinkV4 {
                                    object: link.object,
                                    zone_change_count: gen,
                                },
                                Zone::Stack,
                            )
                        })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let chandra_keep = read_state(state)
        .map(|c7a| {
            c7a.chandra_paid_casts
                .iter()
                .copied()
                .filter(|&link| {
                    link_live_in(state, link, Zone::Exile)
                        || link.zone_change_count.checked_add(1).is_some_and(|gen| {
                            link_live_in(
                                state,
                                ObjectLinkV4 {
                                    object: link.object,
                                    zone_change_count: gen,
                                },
                                Zone::Stack,
                            )
                        })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if let Some(eternal) = state.eternal_v1.as_mut() {
        eternal.c7a.malcolm_free_casts = malcolm_keep;
        eternal.c7a.chandra_paid_casts = chandra_keep;
    }
}

/// Exiles up to `count` cards from the top of `player`'s library face down
/// with concealed event visibility, private looker access for `player` only,
/// and a play permission with `expiry`.
fn exile_top_face_down_playable(
    state: &mut GameState,
    player: PlayerId,
    count: usize,
    expiry: PlayPermissionExpiry,
    flameshape: bool,
) {
    for _ in 0..count {
        let Some(&card) = state.players[player.index()].library.first() else {
            break;
        };
        let history_start = state.engine.event_history.len();
        state.reveal_meaning_v3.set_concealed_exile(Some(card));
        event::propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Exile));
        state.reveal_meaning_v3.set_concealed_exile(None);
        if state.engine.halted.is_some() || state.objects.get(card).zone != Zone::Exile {
            continue;
        }
        let link = live_link(state, card);
        state.objects.get_mut(card).v4.face_down_v1 = Some(FaceDownV1 {
            disguised: false,
            lookers: 1 << player.index(),
            hidden_by: None,
            hidden_by_source: None,
        });
        for index in history_start..state.engine.event_history.len() {
            if matches!(
                state.engine.event_history[index],
                CommittedEvent::ZoneChange {
                    object,
                    from: Zone::Library,
                    to: Zone::Exile,
                    ..
                } if object == card
            ) {
                state
                    .observation_journal
                    .record_visibility_v2(index, card, 1 << player.index());
            }
        }
        let def = &CARD_DEFS[state.objects.get(card).card_def as usize];
        let play_or_cast = if def.is_playable_land() {
            PlayOrCast::Play
        } else if def.is_castable() {
            PlayOrCast::Cast
        } else {
            continue;
        };
        state.engine.exile_play_permissions.push(PlayPermission {
            without_mana_cost: FreeCastV1::default(),
            any_mana_type: Default::default(),
            object: card,
            holder: player,
            zone_change_generation: link.zone_change_count,
            play_or_cast,
            expiry,
        });
        c7a_state(state)
            .concealed_exile_cards
            .push(ConcealedExileV1 {
                card: link,
                holder: player,
                flameshape,
            });
    }
}

// ---- Card programs ------------------------------------------------------------------

pub fn scarlett_etb() -> EffectOp {
    leaf(C7aOpV1::ScarlettCantBlockV1)
}

pub fn scarlett_upkeep() -> EffectOp {
    leaf(C7aOpV1::ScarlettUpkeepExileV1)
}

pub fn gandalf_trigger() -> EffectOp {
    leaf(C7aOpV1::GandalfNoncreatureV1)
}

pub fn flameshape() -> EffectOp {
    leaf(C7aOpV1::FlameshapeV1)
}

pub fn fanatic_entry() -> EffectOp {
    leaf(C7aOpV1::FanaticEntryV1)
}

pub fn inti_attack() -> EffectOp {
    leaf(C7aOpV1::IntiAttackV1)
}

fn inti_reflexive() -> EffectOp {
    leaf(C7aOpV1::IntiReflexiveV1)
}

fn inti_impulse() -> EffectOp {
    EffectOp::ImpulseDraw {
        count: 1,
        duration: ImpulseDuration::UntilOwnersNextEndStep,
    }
}

/// Malcolm's draw is a direct root `Sequence` element so Dredge, Jace,
/// Hullbreacher, Narset, and Quantum Riddler all apply natively.
pub fn malcolm_hit() -> EffectOp {
    EffectOp::Sequence(vec![
        leaf(C7aOpV1::MalcolmCounterV1),
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        },
        leaf(C7aOpV1::MalcolmDiscardAndMaybeCastV1),
    ])
}

pub fn chandra_plus_one_exile() -> EffectOp {
    leaf(C7aOpV1::ChandraExileCastOrBurnV1)
}

pub fn chandra_plus_one_mana() -> EffectOp {
    EffectOp::AddMana {
        player: PlayerRef::Controller,
        colors: vec![ManaColor::R, ManaColor::R],
    }
}

pub fn chandra_minus_three() -> EffectOp {
    EffectOp::DealDamage {
        target: TargetRef::Target(0),
        amount: 4,
    }
}

pub fn chandra_minus_seven() -> EffectOp {
    leaf(C7aOpV1::ChandraEmblemV1)
}

fn chandra_emblem_damage() -> EffectOp {
    leaf(C7aOpV1::ChandraEmblemDamageV1)
}

fn fanatic_target() -> TargetSpec {
    TargetSpec::StandardV1(StandardTargetV1::UpToOneCreatureCardInOwnGraveyardV1)
}

// ---- Triggered abilities ------------------------------------------------------------

const SCARLETT_TRIGGERS: [TriggeredAbilityDef; 2] = [
    TriggeredAbilityDef {
        condition: TriggerCondition::Etb,
        home_zone: Zone::Battlefield,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index: 0,
        effect: scarlett_etb,
    },
    TriggeredAbilityDef {
        condition: TriggerCondition::BeginningOfUpkeep {
            controller_only: true,
        },
        home_zone: Zone::Battlefield,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index: 0,
        effect: scarlett_upkeep,
    },
];

const GANDALF_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::CastNoncreatureSpell,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: gandalf_trigger,
}];

const INTI_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::ControllerAttacks,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: inti_attack,
}];

const MALCOLM_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::DealsCombatDamageToPlayer,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: malcolm_hit,
}];

/// Triggered abilities of this module's cards, falling through to wave C7b.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        SCARLETT => &SCARLETT_TRIGGERS,
        GANDALF => &GANDALF_TRIGGERS,
        INTI => &INTI_TRIGGERS,
        MALCOLM => &MALCOLM_TRIGGERS,
        _ => crate::eternal_cube_c7b_v1::triggers_for(name),
    }
}

/// Target specs of this module's printed and queued triggered abilities.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    if name == SCARLETT && *effect == scarlett_etb() {
        return Some(TargetSpec::AnyPlayer);
    }
    if name == SCARLETT && *effect == scarlett_upkeep() {
        return Some(TargetSpec::None);
    }
    if name == GANDALF && *effect == gandalf_trigger() {
        return Some(TargetSpec::None);
    }
    if name == INTI && *effect == inti_attack() {
        return Some(TargetSpec::None);
    }
    if name == INTI && *effect == inti_reflexive() {
        return Some(TargetSpec::AttackingCreature);
    }
    if name == INTI && *effect == inti_impulse() {
        return Some(TargetSpec::None);
    }
    if name == MALCOLM && *effect == malcolm_hit() {
        return Some(TargetSpec::None);
    }
    if name == CHANDRA && *effect == chandra_emblem_damage() {
        return Some(TargetSpec::AnyTarget);
    }
    None
}

/// Queued triggers owned by this module's cards.
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    (name == INTI && (*effect == inti_reflexive() || *effect == inti_impulse()))
        || (name == CHANDRA && *effect == chandra_emblem_damage())
}

// ---- Engine hooks -------------------------------------------------------------------

/// Headliner Scarlett's blocking ban: creatures controlled by a player tagged
/// this turn cannot block.
pub(crate) fn blocker_allowed(state: &GameState, blocker: ObjectId) -> bool {
    let Some(c7a) = read_state(state) else {
        return true;
    };
    if c7a.cant_block_players.is_empty() {
        return true;
    }
    let Some(live) = state.objects.try_get(blocker) else {
        return true;
    };
    let now = TurnV1::now(state);
    !c7a.cant_block_players
        .iter()
        .any(|&(player, turn)| player == live.controller && turn == now)
}

/// Gandalf's Flameshape grants play permission only while the holder controls
/// a Wizard.
pub(crate) fn permission_dormant(state: &GameState, permission: &PlayPermission) -> bool {
    let Some(c7a) = read_state(state) else {
        return false;
    };
    let is_flameshape = c7a.concealed_exile_cards.iter().any(|entry| {
        entry.flameshape
            && entry.holder == permission.holder
            && entry.card.object == permission.object
            && entry.card.zone_change_count == permission.zone_change_generation
    });
    is_flameshape && !controls_wizard(state, permission.holder)
}

/// An aborted cast of a Scarlett or Flameshape face-down exiled card returns
/// it to exile as a new object: keep it face down with its looker mask and
/// updated permission generation.
pub(crate) fn restore_aborted_concealed_exile(state: &mut GameState, card: ObjectId) {
    let live_count = state.objects.get(card).zone_change_count;
    let Some(exile_count) = live_count.checked_sub(2) else {
        return;
    };
    let old = ObjectLinkV4 {
        object: card,
        zone_change_count: exile_count,
    };
    let Some(c7a) = state.eternal_v1.as_mut().map(|eternal| &mut eternal.c7a) else {
        return;
    };
    let Some(position) = c7a
        .concealed_exile_cards
        .iter()
        .position(|entry| entry.card == old)
    else {
        return;
    };
    let holder = c7a.concealed_exile_cards[position].holder;
    c7a.concealed_exile_cards[position].card = ObjectLinkV4 {
        object: card,
        zone_change_count: live_count,
    };
    state.objects.get_mut(card).v4.face_down_v1 = Some(FaceDownV1 {
        disguised: false,
        lookers: 1 << holder.index(),
        hidden_by: None,
        hidden_by_source: None,
    });
    for permission in &mut state.engine.exile_play_permissions {
        if permission.object == card && permission.zone_change_generation == exile_count {
            permission.zone_change_generation = live_count;
        }
    }
}

/// Applies Metamorphosis Fanatic's entry-time Metamorph copy and lifelink
/// counter during `apply_entry_counters`, before entry observers, copied entry
/// counters/lore, and ETB triggers run.
pub(crate) fn apply_entry_counters(
    state: &mut GameState,
    object: ObjectId,
    input: Option<&crate::doubling_season_v48::ReplacementInputV48>,
) {
    let Some(live) = state.objects.try_get(object) else {
        return;
    };
    if live.zone != Zone::Battlefield {
        return;
    }
    let Some(prev_gen) = live.zone_change_count.checked_sub(1) else {
        return;
    };
    let controller = live.controller;
    let expected = ObjectLinkV4 {
        object,
        zone_change_count: prev_gen,
    };
    let copied_def = state.eternal_v1.as_mut().and_then(|eternal| {
        if eternal
            .c7a
            .pending_metamorph_copy
            .is_some_and(|(link, _)| link == expected)
        {
            eternal
                .c7a
                .pending_metamorph_copy
                .take()
                .map(|(_, def)| def)
        } else {
            None
        }
    });
    if let Some(copied_def) = copied_def {
        crate::eternal_vintage_v1::apply_metamorph_entry_copy(state, object, copied_def);
    }
    let add_lifelink = state.eternal_v1.as_mut().is_some_and(|eternal| {
        if eternal.c7a.pending_lifelink_entry == Some(expected) {
            eternal.c7a.pending_lifelink_entry = None;
            true
        } else {
            false
        }
    });
    if add_lifelink {
        match crate::doubling_season_v48::entry_amount(state, input, controller, 1)
            .and_then(|amount| i16::try_from(amount).map_err(|err| err.to_string()))
            .and_then(|amount| {
                state
                    .objects
                    .get(object)
                    .v4
                    .lifelink_keyword_counters
                    .checked_add(amount)
                    .ok_or_else(|| "lifelink counter overflow".to_string())
            }) {
            Ok(total) => {
                if total > state.objects.get(object).v4.lifelink_keyword_counters {
                    let timestamp = crate::engine::next_timestamp(state);
                    let live = state.objects.get_mut(object);
                    live.v4.lifelink_keyword_counters = total;
                    live.v4.lifelink_counter_timestamp = Some(timestamp);
                }
            }
            Err(_) => {
                state.engine.halted = Some((
                    crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                    object,
                ));
            }
        }
    }
}

/// Inti, Seneschal of the Sun: "Whenever you discard one or more cards, exile
/// the top card of your library. You may play that card until your next end
/// step." One trigger per active Inti per discard instruction batch.
pub(crate) fn note_discards(state: &mut GameState, cards: &[ObjectId]) {
    if cards.is_empty() {
        return;
    }
    for player in [PlayerId::P0, PlayerId::P1] {
        let sources = crate::eternal_e5a_v1::active_permanents(state, INTI, Some(player))
            .map(|object| AbilitySourceContractV4::capture(state, object))
            .collect::<Vec<_>>();
        if sources.is_empty() {
            continue;
        }
        let discarded_any = cards.iter().any(|&card| {
            state.players[player.index()].hand.contains(&card)
                && state.objects.get(card).zone == Zone::Hand
        });
        if !discarded_any {
            continue;
        }
        for source in sources {
            state
                .engine
                .pending_triggers
                .push(crate::eternal_cards_v1::delayed_trigger(
                    source,
                    player,
                    inti_impulse(),
                ));
        }
    }
}

/// Chandra, Torch of Defiance's -7 emblems: "Whenever you cast a spell, this
/// emblem deals 5 damage to any target."
pub(crate) fn delayed_triggers(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Vec<PendingTrigger> {
    let Some(emblems) = read_state(state).map(|c7a| c7a.chandra_emblems.clone()) else {
        return Vec::new();
    };
    if emblems.is_empty() {
        return Vec::new();
    }
    let mut triggers = Vec::new();
    for event in events {
        let CommittedEvent::SpellCast { controller, .. } = *event else {
            continue;
        };
        for emblem in &emblems {
            if emblem.controller == controller {
                let mut trigger = crate::eternal_cards_v1::delayed_trigger(
                    emblem.source,
                    emblem.controller,
                    chandra_emblem_damage(),
                );
                trigger.target_spec = TargetSpec::AnyTarget;
                triggers.push(trigger);
            }
        }
    }
    triggers
}

/// Prunes expired turn-scoped markers and departed cast authorizations at the
/// start of each turn.
pub(crate) fn begin_turn(state: &mut GameState, _player: PlayerId) {
    let now = TurnV1::now(state);
    let concealed_keep = read_state(state)
        .map(|c7a| {
            c7a.concealed_exile_cards
                .iter()
                .copied()
                .filter(|entry| link_live_in(state, entry.card, Zone::Exile))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if let Some(eternal) = state.eternal_v1.as_mut() {
        eternal
            .c7a
            .cant_block_players
            .retain(|&(_, turn)| turn == now);
        eternal.c7a.concealed_exile_cards = concealed_keep;
    }
    prune_cast_markers(state);
}

/// Whether `source` is authorized for Malcolm's free graveyard resolution cast.
pub(crate) fn is_malcolm_free_cast(state: &GameState, source: ObjectId) -> bool {
    let Some(c7a) = read_state(state) else {
        return false;
    };
    if c7a.malcolm_free_casts.is_empty() {
        return false;
    }
    let Some(object) = state.objects.try_get(source) else {
        return false;
    };
    match object.zone {
        Zone::Graveyard => c7a.malcolm_free_casts.iter().any(|link| {
            link.object == source && link.zone_change_count == object.zone_change_count
        }),
        Zone::Stack => object.v4.spell_cast_origin.is_some_and(|origin| {
            origin.origin_zone == Zone::Graveyard
                && matches!(origin.route, SpellCastRouteV4::ResolvingEffectV1 { .. })
                && c7a.malcolm_free_casts.iter().any(|link| {
                    link.object == source
                        && link.zone_change_count == origin.origin_zone_change_count
                })
        }),
        _ => false,
    }
}

/// Whether `source` is authorized for Chandra's paid exile resolution cast.
pub(crate) fn is_chandra_paid_cast(state: &GameState, source: ObjectId) -> bool {
    let Some(c7a) = read_state(state) else {
        return false;
    };
    if c7a.chandra_paid_casts.is_empty() {
        return false;
    }
    let Some(object) = state.objects.try_get(source) else {
        return false;
    };
    match object.zone {
        Zone::Exile => c7a.chandra_paid_casts.iter().any(|link| {
            link.object == source && link.zone_change_count == object.zone_change_count
        }),
        Zone::Stack => object.v4.spell_cast_origin.is_some_and(|origin| {
            origin.origin_zone == Zone::Exile
                && matches!(origin.route, SpellCastRouteV4::ResolvingEffectV1 { .. })
                && c7a.chandra_paid_casts.iter().any(|link| {
                    link.object == source
                        && link.zone_change_count == origin.origin_zone_change_count
                })
        }),
        _ => false,
    }
}

/// The number of chorus counters on `malcolm`'s battlefield incarnation (used
/// by tests and Malcolm's resolution).
pub fn chorus_counters(state: &GameState, malcolm: ObjectId) -> u32 {
    let Some(live) = state.objects.try_get(malcolm) else {
        return 0;
    };
    if live.zone != Zone::Battlefield {
        return 0;
    }
    chorus_counters_for_link(
        state,
        ObjectLinkV4 {
            object: malcolm,
            zone_change_count: live.zone_change_count,
        },
    )
}

fn chorus_counters_for_link(state: &GameState, link: ObjectLinkV4) -> u32 {
    read_state(state)
        .and_then(|c7a| {
            c7a.chorus_counters
                .iter()
                .find(|&&(recorded, _)| recorded == link)
                .map(|&(_, count)| count)
        })
        .unwrap_or(0)
}

// ---- Resolution casts ---------------------------------------------------------------

pub(crate) fn resolution_cast_card(
    choice: &E3ChoiceV1,
    selected: &[Target],
) -> Option<EffectObjectBinding> {
    if !matches!(
        choice.kind,
        E3ChoiceKindV1::C7aV1(C7aChoiceV1::MalcolmFreeCastV1 | C7aChoiceV1::ChandraPaidCastV1)
    ) {
        return None;
    }
    let &Target::Object(object) = selected.first()? else {
        return None;
    };
    choice
        .bindings
        .iter()
        .copied()
        .find(|binding| binding.object == object)
}

pub(crate) fn validate_resolution_cast(
    suspended: &EffectContinuation,
    choice: &E3ChoiceV1,
    selected: &[Target],
    permission: crate::engine::resolution_cast_v1::ResolutionPermissionV1,
) -> Result<(), String> {
    let expected_zone = match choice.kind {
        E3ChoiceKindV1::C7aV1(C7aChoiceV1::MalcolmFreeCastV1) => Zone::Graveyard,
        E3ChoiceKindV1::C7aV1(C7aChoiceV1::ChandraPaidCastV1) => Zone::Exile,
        _ => return Err("C7a choice does not authorize a resolution cast".into()),
    };
    if resolution_cast_card(choice, selected) != Some(permission.card)
        || selected.len() != 1
        || permission.card.expected_zone != expected_zone
        || permission.land
        || permission.maximum_mana_value.is_some()
        || permission.controller != choice.player
        || suspended.ctx.controller != choice.player
        || suspended.frames != choice.remaining
    {
        return Err("C7a resolution cast answer does not authorize this cast".into());
    }
    Ok(())
}

// ---- Execution ----------------------------------------------------------------------

fn fanatic_return(
    state: &mut GameState,
    controller: PlayerId,
    card: ObjectId,
    copied_def: Option<u16>,
) {
    let gy_link = live_link(state, card);
    {
        let c7a = c7a_state(state);
        c7a.pending_lifelink_entry = Some(gy_link);
        c7a.pending_metamorph_copy = copied_def.map(|def| (gy_link, def));
    }
    event::propose_and_commit(
        state,
        ProposedEvent::zone_change_to_battlefield_under_controller(card, controller),
    );
    if let Some(eternal) = state.eternal_v1.as_mut() {
        eternal.c7a.pending_lifelink_entry = None;
        eternal.c7a.pending_metamorph_copy = None;
    }
}

fn chandra_card_was_cast(state: &GameState, card: ObjectLinkV4) -> bool {
    state.stack.iter().any(|item| {
        item.kind == StackItemKind::Spell
            && item.source == card.object
            && item
                .v4
                .source_contract
                .and_then(|contract| contract.spell_cast_origin)
                .is_some_and(|origin| {
                    origin.origin_zone == Zone::Exile
                        && origin.origin_zone_change_count == card.zone_change_count
                })
    })
}

fn chandra_deal_two(ctx: &ExecCtx, state: &mut GameState) {
    event::propose_and_commit(
        state,
        ProposedEvent::damage(ctx.source, Target::Player(ctx.controller.opponent()), 2),
    );
}

/// Runs one of this module's leaves in synchronous execution.
pub(crate) fn execute(op: &C7aOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        C7aOpV1::ScarlettCantBlockV1 => {
            let Some(Target::Player(player)) = ctx.targets.first().copied() else {
                return;
            };
            if !ctx.ability_source_contract.is_some_and(|source| {
                crate::engine::effect_target_is_legal_from_ability_source(
                    state,
                    source,
                    ctx.controller,
                    TargetSpec::AnyPlayer,
                    &ctx.targets,
                    0,
                )
            }) {
                return;
            }
            let turn = TurnV1::now(state);
            let entries = &mut c7a_state(state).cant_block_players;
            if !entries.contains(&(player, turn)) {
                entries.push((player, turn));
            }
        }
        C7aOpV1::ScarlettUpkeepExileV1 => {
            exile_top_face_down_playable(
                state,
                ctx.controller,
                1,
                PlayPermissionExpiry::EndOfTurn,
                false,
            );
        }
        C7aOpV1::GandalfNoncreatureV1 => {
            if let Some(contract) = ctx.ability_source_contract {
                let link = ObjectLinkV4 {
                    object: contract.source,
                    zone_change_count: contract.zone_change_count,
                };
                if link_live_in(state, link, Zone::Battlefield) {
                    crate::effect::install_temporary_boost(
                        state,
                        EffectObjectBinding {
                            object: link.object,
                            expected_zone: Zone::Battlefield,
                            expected_zone_change_count: link.zone_change_count,
                        },
                        1,
                        1,
                        Keywords::NONE,
                    );
                }
            }
            event::propose_and_commit(
                state,
                ProposedEvent::damage(ctx.source, Target::Player(ctx.controller.opponent()), 1),
            );
        }
        C7aOpV1::FlameshapeV1 => {
            exile_top_face_down_playable(
                state,
                ctx.controller,
                2,
                PlayPermissionExpiry::AsLongAsExiled,
                true,
            );
        }
        C7aOpV1::IntiReflexiveV1 => {
            let Some(Target::Object(creature)) = ctx.targets.first().copied() else {
                return;
            };
            if !ctx.target_incarnation_matches(0, state)
                || !ctx.ability_source_contract.is_some_and(|source| {
                    crate::engine::effect_target_is_legal_from_ability_source(
                        state,
                        source,
                        ctx.controller,
                        TargetSpec::AttackingCreature,
                        &ctx.targets,
                        0,
                    )
                })
            {
                return;
            }
            let _ = event::add_plus_one_counters(state, creature, ctx.controller, 1);
            if state.objects.get(creature).zone == Zone::Battlefield {
                crate::effect::install_temporary_boost(
                    state,
                    bind(state, creature),
                    0,
                    0,
                    Keywords::TRAMPLE,
                );
            }
        }
        C7aOpV1::MalcolmCounterV1 => {
            let Some(contract) = ctx.ability_source_contract else {
                return;
            };
            let link = ObjectLinkV4 {
                object: contract.source,
                zone_change_count: contract.zone_change_count,
            };
            if !link_live_in(state, link, Zone::Battlefield) {
                return;
            }
            let counters = &mut c7a_state(state).chorus_counters;
            if let Some((_, count)) = counters.iter_mut().find(|(recorded, _)| *recorded == link) {
                *count = count.saturating_add(1);
            } else {
                counters.push((link, 1));
            }
        }
        C7aOpV1::ChandraBurnIfNotCastV1 { card } => {
            if !chandra_card_was_cast(state, *card) {
                if let Some(eternal) = state.eternal_v1.as_mut() {
                    eternal.c7a.chandra_paid_casts.retain(|link| link != card);
                }
                chandra_deal_two(ctx, state);
            }
        }
        C7aOpV1::ChandraEmblemV1 => {
            if let Some(source) = ctx.ability_source_contract {
                c7a_state(state).chandra_emblems.push(EmblemV1 {
                    controller: ctx.controller,
                    source,
                });
            }
        }
        C7aOpV1::ChandraEmblemDamageV1 => {
            let Some(target) = ctx.targets.first().copied() else {
                return;
            };
            if !ctx.target_incarnation_matches(0, state)
                || !ctx.ability_source_contract.is_some_and(|source| {
                    crate::engine::effect_target_is_legal_from_ability_source(
                        state,
                        source,
                        ctx.controller,
                        TargetSpec::AnyTarget,
                        &ctx.targets,
                        0,
                    )
                })
            {
                return;
            }
            event::propose_and_commit(state, ProposedEvent::damage(ctx.source, target, 5));
        }
        C7aOpV1::FanaticEntryV1
        | C7aOpV1::IntiAttackV1
        | C7aOpV1::MalcolmDiscardAndMaybeCastV1
        | C7aOpV1::ChandraExileCastOrBurnV1 => {
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
    op: &C7aOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    let controller = continuation.ctx.controller;
    match op {
        C7aOpV1::FanaticEntryV1 => {
            let ctx = &continuation.ctx;
            let Some(Target::Object(card)) = ctx.targets.first().copied() else {
                return Ok(false);
            };
            if !ctx.target_incarnation_matches(0, state)
                || !ctx.ability_source_contract.is_some_and(|source| {
                    crate::engine::effect_target_is_legal_from_ability_source(
                        state,
                        source,
                        controller,
                        fanatic_target(),
                        &ctx.targets,
                        0,
                    )
                })
            {
                return Ok(false);
            }
            let is_metamorph = CARD_DEFS
                .get(usize::from(state.objects.get(card).card_def))
                .is_some_and(|def| def.name == METAMORPH);
            let entry_proposal =
                ProposedEvent::zone_change_to_battlefield_under_controller(card, controller);
            let entry_prevented = match &entry_proposal {
                ProposedEvent::ZoneChange(change) => {
                    crate::eternal_legacy_e7a_v1::prevents_zone_change(state, change)
                }
                _ => false,
            };
            if is_metamorph
                && !entry_prevented
                && crate::eternal_legacy_e7a_v1::active_dress_down_timestamp(state, None).is_none()
            {
                let candidates = [PlayerId::P0, PlayerId::P1]
                    .into_iter()
                    .flat_map(|p| state.players[p.index()].battlefield.iter().copied())
                    .filter(|&object| {
                        crate::engine::object_has_type(state, object, CardType::Artifact)
                            || crate::engine::object_has_type(state, object, CardType::Creature)
                    })
                    .map(|object| bind(state, object))
                    .collect::<Vec<_>>();
                if !candidates.is_empty() {
                    let card_binding = bind(state, card);
                    offer(
                        continuation,
                        C7aChoiceV1::FanaticMetamorphCopyV1 { card: card_binding },
                        controller,
                        candidates,
                        0,
                        1,
                        path,
                    );
                    return Ok(true);
                }
            }
            fanatic_return(state, controller, card, None);
            Ok(false)
        }
        C7aOpV1::IntiAttackV1 => {
            let hand = hand_bindings(state, controller);
            if hand.is_empty() {
                return Ok(false);
            }
            offer(
                continuation,
                C7aChoiceV1::IntiAttackDiscardV1,
                controller,
                hand,
                0,
                1,
                path,
            );
            Ok(true)
        }
        C7aOpV1::MalcolmDiscardAndMaybeCastV1 => {
            let hand = hand_bindings(state, controller);
            if hand.is_empty() {
                return Ok(false);
            }
            offer(
                continuation,
                C7aChoiceV1::MalcolmDiscardV1,
                controller,
                hand,
                1,
                1,
                path,
            );
            Ok(true)
        }
        C7aOpV1::ChandraExileCastOrBurnV1 => {
            let Some(&card) = state.players[controller.index()].library.first() else {
                chandra_deal_two(&continuation.ctx, state);
                return Ok(false);
            };
            event::propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Exile));
            if state.engine.halted.is_some() {
                return Ok(false);
            }
            if state.objects.get(card).zone != Zone::Exile {
                chandra_deal_two(&continuation.ctx, state);
                return Ok(false);
            }
            let link = live_link(state, card);
            let binding = bind(state, card);
            prune_cast_markers(state);
            c7a_state(state).chandra_paid_casts.push(link);
            if crate::engine::resolution_cast_v1::can_cast_exiled_without_mana(
                state, controller, binding, None,
            ) {
                continuation.frames.push(EffectFrame::Program {
                    op: leaf(C7aOpV1::ChandraBurnIfNotCastV1 { card: link }),
                    path: path.clone(),
                });
                offer(
                    continuation,
                    C7aChoiceV1::ChandraPaidCastV1,
                    controller,
                    vec![binding],
                    0,
                    1,
                    path,
                );
                return Ok(true);
            }
            if let Some(eternal) = state.eternal_v1.as_mut() {
                eternal
                    .c7a
                    .chandra_paid_casts
                    .retain(|recorded| *recorded != link);
            }
            chandra_deal_two(&continuation.ctx, state);
            Ok(false)
        }
        other => {
            execute(other, &continuation.ctx, state);
            Ok(false)
        }
    }
}

/// Applies an answered choice of this module. `Ok(true)` when it staged a
/// further choice.
pub(crate) fn finish_selection(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    choice: &E3ChoiceV1,
    kind: &C7aChoiceV1,
    bindings: Vec<EffectObjectBinding>,
) -> Result<bool, String> {
    let player = choice.player;
    match kind {
        C7aChoiceV1::FanaticMetamorphCopyV1 { card } => {
            if !link_live_in(
                state,
                ObjectLinkV4 {
                    object: card.object,
                    zone_change_count: card.expected_zone_change_count,
                },
                Zone::Graveyard,
            ) {
                return Ok(false);
            }
            let copied_def = bindings
                .first()
                .map(|chosen| state.objects.get(chosen.object).card_def);
            fanatic_return(state, player, card.object, copied_def);
            Ok(false)
        }
        C7aChoiceV1::IntiAttackDiscardV1 => {
            let Some(card) = bindings.first() else {
                return Ok(false);
            };
            crate::engine::commit_discarded_card(state, card.object);
            if let Some(source) = continuation
                .ctx
                .ability_source_contract
                .filter(|_| state.engine.halted.is_none())
            {
                let mut pending =
                    crate::eternal_cards_v1::delayed_trigger(source, player, inti_reflexive());
                pending.target_spec = TargetSpec::AttackingCreature;
                state.engine.pending_triggers.push(pending);
            }
            Ok(false)
        }
        C7aChoiceV1::MalcolmDiscardV1 => {
            let Some(card) = bindings.first() else {
                return Err("Malcolm's trigger requires discarding a card".into());
            };
            let gen_before = state.objects.get(card.object).zone_change_count;
            crate::engine::commit_discarded_card(state, card.object);
            if state.engine.halted.is_some() {
                return Ok(false);
            }
            let Some(contract) = continuation.ctx.ability_source_contract else {
                return Ok(false);
            };
            let malcolm_link = ObjectLinkV4 {
                object: contract.source,
                zone_change_count: contract.zone_change_count,
            };
            if chorus_counters_for_link(state, malcolm_link) < 4 {
                return Ok(false);
            }
            let Some(expected_gy_gen) = gen_before.checked_add(1) else {
                return Ok(false);
            };
            let gy_link = ObjectLinkV4 {
                object: card.object,
                zone_change_count: expected_gy_gen,
            };
            if !link_live_in(state, gy_link, Zone::Graveyard)
                || state.objects.get(card.object).owner != player
            {
                return Ok(false);
            }
            let gy_binding = EffectObjectBinding {
                object: card.object,
                expected_zone: Zone::Graveyard,
                expected_zone_change_count: expected_gy_gen,
            };
            prune_cast_markers(state);
            c7a_state(state).malcolm_free_casts.push(gy_link);
            if crate::engine::resolution_cast_v1::can_cast_from_graveyard_v1(
                state, player, gy_binding,
            ) {
                offer(
                    continuation,
                    C7aChoiceV1::MalcolmFreeCastV1,
                    player,
                    vec![gy_binding],
                    0,
                    1,
                    choice.path.clone(),
                );
                return Ok(true);
            }
            if let Some(eternal) = state.eternal_v1.as_mut() {
                eternal
                    .c7a
                    .malcolm_free_casts
                    .retain(|recorded| *recorded != gy_link);
            }
            Ok(false)
        }
        C7aChoiceV1::MalcolmFreeCastV1 => {
            // Only reached when the player declined casting; an affirmative
            // choice is intercepted by `resolution_cast_card`.
            if let Some(card) = choice.bindings.first() {
                let link = ObjectLinkV4 {
                    object: card.object,
                    zone_change_count: card.expected_zone_change_count,
                };
                if let Some(eternal) = state.eternal_v1.as_mut() {
                    eternal
                        .c7a
                        .malcolm_free_casts
                        .retain(|recorded| *recorded != link);
                }
            }
            Ok(false)
        }
        C7aChoiceV1::ChandraPaidCastV1 => {
            // Only reached when the player declined casting; the follow-up
            // `ChandraBurnIfNotCastV1` frame deals 2 damage and clears the
            // cast marker.
            Ok(false)
        }
    }
}
