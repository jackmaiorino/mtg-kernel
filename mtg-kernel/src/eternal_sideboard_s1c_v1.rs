//! Card programs for the eternal catalog's Vintage-only sideboard cards
//! (wave S1c of issue #365, `docs/design/eternal_formats_support_v1.md`).
//!
//! `eternal_cards_v1` forwards its effect leaves here, and the engine calls
//! the hooks below only in `eternal-fixtures` builds. Every table is keyed by
//! a printed name that occurs only in the eternal registry, so the Pauper,
//! FDN and Standard catalogs never reach it. Behavior was read from each
//! card's current Oracle text and, where present, its XMage source.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::Keywords;
use crate::card_def::{card_id_by_name, CardType, TargetSpec, CARD_DEFS};
use crate::effect::{
    EffectContinuation, EffectObjectBinding, EffectOp, ExecCtx, ObjectRef, PlayerRef,
};
use crate::eternal_cards_v1::e3::{E3ChoiceKindV1, E3ChoiceV1};
use crate::eternal_cards_v1::EternalOpV1;
use crate::event::{self, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::standard_cards_v1::StandardOpV1;
use crate::state::{GameState, ObjectLinkV4, Target, Zone};
use crate::trigger::{TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::S1cV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum S1cOpV1 {
    /// Hurkyl's Recall: return all artifacts target player 0 owns to their
    /// owner's hand.
    ReturnTargetPlayersArtifactsV1,
    /// Rebuild: return all artifacts to their owners' hands.
    ReturnAllArtifactsV1,
    /// Virtue's Ruin: destroy all white creatures.
    DestroyWhiteCreaturesV1,
    /// Feed the Swarm: destroy target 0; the spell's controller loses life
    /// equal to its mana value.
    FeedTheSwarmV1,
    /// Lithomantic Barrage: 1 damage to target 0, or 5 if it is white and/or
    /// blue.
    BarrageV1,
    /// Dark Petition's spell mastery: add {B}{B}{B} with two or more instant
    /// and/or sorcery cards in the controller's graveyard.
    DarkPetitionManaV1,
    /// Argentum Masticore's upkeep trigger: sacrifice it unless its
    /// controller discards a card; when they do, destroy a target nonland
    /// permanent an opponent controls with mana value at most the discarded
    /// card's. A choice.
    MasticoreUpkeepV1,
}

impl S1cOpV1 {
    /// Whether this leaf can yield a player decision, so its program must
    /// use the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(self, Self::MasticoreUpkeepV1)
    }
}

/// Resolution choices owned by this module, carried as
/// `E3ChoiceKindV1::S1cV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum S1cChoiceV1 {
    /// Argentum Masticore: up to one card to discard from the hand.
    MasticoreDiscardV1 { source: ObjectLinkV4 },
    /// Argentum Masticore's reflexive trigger: the permanent to destroy.
    MasticoreDestroyV1,
}

impl S1cChoiceV1 {
    /// Whether the candidates are hidden from the other player.
    pub(crate) fn chooser_private(&self) -> bool {
        matches!(self, Self::MasticoreDiscardV1 { .. })
    }

    pub(crate) fn public_purpose(&self) -> crate::rl::TargetSelectionPurposeV4 {
        match self {
            Self::MasticoreDiscardV1 { .. } => crate::rl::TargetSelectionPurposeV4::CardSelection,
            Self::MasticoreDestroyV1 => crate::rl::TargetSelectionPurposeV4::PermanentSelection,
        }
    }

    /// Object incarnations this choice refers to beyond its candidates.
    pub(crate) fn referenced_bindings(&self) -> Vec<EffectObjectBinding> {
        Vec::new()
    }
}

// ---- Names and helpers --------------------------------------------------------------

pub(crate) const LIGHT_OF_DAY: &str = "Light of Day";
pub(crate) const URBORG: &str = "Urborg, Tomb of Yawgmoth";
pub(crate) const HULLBREACHER: &str = "Hullbreacher";
pub(crate) const SPHINX_OF_THE_STEEL_WIND: &str = "Sphinx of the Steel Wind";
pub(crate) const WURMCOIL_ENGINE: &str = "Wurmcoil Engine";
pub(crate) const ARGENTUM_MASTICORE: &str = "Argentum Masticore";
const DEATHTOUCH_WURM_TOKEN: &str = "Deathtouch Wurm Token";
const LIFELINK_WURM_TOKEN: &str = "Lifelink Wurm Token";
const TREASURE_TOKEN: &str = "Treasure Token";

fn s1c(op: S1cOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::S1cV1(op)))
}

fn token_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

/// Every battlefield object, in each player's battlefield order.
fn battlefield(state: &GameState) -> impl Iterator<Item = ObjectId> + '_ {
    [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(move |player| state.players[player.index()].battlefield.iter().copied())
}

fn is_creature(state: &GameState, object: ObjectId) -> bool {
    crate::engine::object_has_type(state, object, CardType::Creature)
}

/// Whether `object` is a battlefield permanent named `name` whose printed
/// abilities apply.
fn active_named(state: &GameState, object: ObjectId, name: &str) -> bool {
    state.objects.try_get(object).is_some_and(|live| {
        live.zone == Zone::Battlefield
            && live.v4.face_index == 0
            && live.v4.face_down_v1.is_none()
            && CARD_DEFS
                .get(live.card_def as usize)
                .is_some_and(|def| def.name == name && def.is_executable())
            && crate::continuous_characteristics_v1::printed_abilities_active(state, object)
    })
}

fn any_active(state: &GameState, name: &str) -> bool {
    battlefield(state).any(|object| active_named(state, object, name))
}

/// Target `index` if it still names its announced incarnation, in `zone`.
fn bound_target_in(ctx: &ExecCtx, state: &GameState, index: usize, zone: Zone) -> Option<ObjectId> {
    let Some(Target::Object(object)) = ctx.targets.get(index).copied() else {
        return None;
    };
    (ctx.target_incarnation_matches(index, state) && state.objects.get(object).zone == zone)
        .then_some(object)
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

fn sacrifice(state: &mut GameState, object: ObjectId) {
    event::log_sacrifice(state, object);
    event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Graveyard));
}

/// Offers one of this module's selections; the frames still to run after it
/// are the continuation's current frames.
fn offer(
    continuation: &mut EffectContinuation,
    kind: S1cChoiceV1,
    player: PlayerId,
    bindings: Vec<EffectObjectBinding>,
    (min, max): (u16, u16),
    path: Vec<u16>,
) {
    let remaining = continuation.frames.clone();
    crate::eternal_cards_v1::e3::offer(
        continuation,
        E3ChoiceV1 {
            kind: E3ChoiceKindV1::S1cV1(kind),
            player,
            candidates: bindings
                .iter()
                .map(|binding| Target::Object(binding.object))
                .collect(),
            bindings,
            min,
            max,
            ordered: false,
            path,
            remaining,
        },
    );
}

// ---- Spell programs -------------------------------------------------------------------

/// Hurkyl's Recall: "Return all artifacts target player owns to their
/// hand."
pub fn hurkyls_recall() -> EffectOp {
    s1c(S1cOpV1::ReturnTargetPlayersArtifactsV1)
}

/// Rebuild: "Return all artifacts to their owners' hands."
pub fn rebuild() -> EffectOp {
    s1c(S1cOpV1::ReturnAllArtifactsV1)
}

/// Virtue's Ruin: "Destroy all white creatures."
pub fn virtues_ruin() -> EffectOp {
    s1c(S1cOpV1::DestroyWhiteCreaturesV1)
}

/// Feed the Swarm: "Destroy target creature or enchantment an opponent
/// controls. You lose life equal to that permanent's mana value."
pub fn feed_the_swarm() -> EffectOp {
    s1c(S1cOpV1::FeedTheSwarmV1)
}

/// Lithomantic Barrage: "deals 1 damage to target creature or planeswalker.
/// It deals 5 damage instead if that target is white and/or blue."
pub fn lithomantic_barrage() -> EffectOp {
    s1c(S1cOpV1::BarrageV1)
}

/// Long Goodbye: "Destroy target creature or planeswalker with mana value 3
/// or less."
pub fn long_goodbye() -> EffectOp {
    EffectOp::DestroyObject {
        object: ObjectRef::Target(0),
    }
}

/// Snapback: "Return target creature to its owner's hand."
pub fn snapback() -> EffectOp {
    EffectOp::MoveObject {
        object: ObjectRef::Target(0),
        to_zone: Zone::Hand,
    }
}

/// Ravenous Trap: "Exile all cards from target player's graveyard."
pub fn ravenous_trap() -> EffectOp {
    EffectOp::ExilePlayersGraveyard {
        player: PlayerRef::Target(0),
    }
}

/// Dark Petition: "Search your library for a card, put that card into your
/// hand, then shuffle. Spell mastery: If there are two or more instant
/// and/or sorcery cards in your graveyard, add {B}{B}{B}."
pub fn dark_petition() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::SearchLibraryToHand {
            player: PlayerRef::Controller,
            filter: crate::effect::LibraryCardFilter::AnyCard,
        },
        s1c(S1cOpV1::DarkPetitionManaV1),
    ])
}

// ---- Triggered abilities ----------------------------------------------------------------

/// Wurmcoil Engine: "When this creature dies, create a 3/3 colorless Phyrexian
/// Wurm artifact creature token with deathtouch and a 3/3 colorless Phyrexian
/// Wurm artifact creature token with lifelink."
fn wurmcoil_tokens() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::CreateToken {
            token_def: token_id(DEATHTOUCH_WURM_TOKEN),
            controller: PlayerRef::Controller,
        },
        EffectOp::CreateToken {
            token_def: token_id(LIFELINK_WURM_TOKEN),
            controller: PlayerRef::Controller,
        },
    ])
}

/// Argentum Masticore: "At the beginning of your upkeep, sacrifice this
/// creature unless you discard a card. When you discard a card this way,
/// destroy target nonland permanent an opponent controls with mana value
/// less than or equal to the mana value of the discarded card."
fn masticore_upkeep() -> EffectOp {
    s1c(S1cOpV1::MasticoreUpkeepV1)
}

const ARGENTUM_MASTICORE_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::BeginningOfUpkeep {
        controller_only: true,
    },
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: masticore_upkeep,
}];

const WURMCOIL_ENGINE_TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::LeftBattlefieldToGraveyard,
    home_zone: Zone::Graveyard,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: wurmcoil_tokens,
}];

/// Triggered abilities of this module's cards.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        WURMCOIL_ENGINE => &WURMCOIL_ENGINE_TRIGGERS,
        ARGENTUM_MASTICORE => &ARGENTUM_MASTICORE_TRIGGERS,
        _ => crate::eternal_sideboard_s1a_v1::triggers_for(name),
    }
}

/// Target specs of this module's triggered abilities, per ability.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    match name {
        WURMCOIL_ENGINE if *effect == wurmcoil_tokens() => Some(TargetSpec::None),
        ARGENTUM_MASTICORE if *effect == masticore_upkeep() => Some(TargetSpec::None),
        _ => crate::eternal_e9b_v1::trigger_target_spec(name, effect),
    }
}

// ---- Effect leaves ------------------------------------------------------------------

pub(crate) fn execute(op: &S1cOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        S1cOpV1::ReturnTargetPlayersArtifactsV1 => {
            let Some(&Target::Player(player)) = ctx.targets.first() else {
                return;
            };
            if !ctx.target_incarnation_matches(0, state) {
                return;
            }
            return_artifacts(state, Some(player));
        }
        S1cOpV1::ReturnAllArtifactsV1 => return_artifacts(state, None),
        S1cOpV1::DestroyWhiteCreaturesV1 => {
            let white = crate::card_def::mana_color_mask(ManaColor::W);
            let candidates = battlefield(state)
                .filter(|&object| {
                    is_creature(state, object)
                        && crate::engine::object_color_mask(state, object) & white != 0
                        && !crate::engine::has_effective_keyword(
                            state,
                            object,
                            crate::card_def::Keywords::INDESTRUCTIBLE,
                        )
                })
                .collect::<Vec<_>>();
            let destroyed = candidates
                .into_iter()
                .filter(|&object| {
                    !crate::eternal_vintage_e7b_v1::statics::regenerate_instead(state, object)
                })
                .collect::<Vec<_>>();
            event::propose_and_commit_batch(
                state,
                destroyed
                    .into_iter()
                    .map(|object| ProposedEvent::zone_change(object, Zone::Graveyard))
                    .collect(),
            );
        }
        S1cOpV1::FeedTheSwarmV1 => {
            let Some(target) = bound_target_in(ctx, state, 0, Zone::Battlefield) else {
                return;
            };
            let value = i32::from(crate::engine::object_mana_value(state, target));
            crate::effect::execute(
                &EffectOp::DestroyObject {
                    object: ObjectRef::Target(0),
                },
                ctx,
                state,
            );
            if value > 0 && state.engine.halted.is_none() {
                event::propose_and_commit(state, ProposedEvent::life_loss(ctx.controller, value));
            }
        }
        S1cOpV1::BarrageV1 => {
            let Some(target) = bound_target_in(ctx, state, 0, Zone::Battlefield) else {
                return;
            };
            let white_or_blue = crate::card_def::mana_color_mask(ManaColor::W)
                | crate::card_def::mana_color_mask(ManaColor::U);
            let amount = if crate::engine::object_color_mask(state, target) & white_or_blue != 0 {
                5
            } else {
                1
            };
            let damage = event::damage_from_exec_ctx_v1(ctx, Target::Object(target), amount);
            event::propose_and_commit_batch(state, vec![damage]);
        }
        // Choices run in `drive`; the plain executor never reaches them.
        S1cOpV1::MasticoreUpkeepV1 => {
            state.engine.halted = Some((
                crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                ctx.source,
            ));
        }
        S1cOpV1::DarkPetitionManaV1 => {
            let spells = state.players[ctx.controller.index()]
                .graveyard
                .iter()
                .filter(|&&card| {
                    CARD_DEFS
                        .get(state.objects.get(card).card_def as usize)
                        .is_some_and(|def| {
                            def.has_type(CardType::Instant) || def.has_type(CardType::Sorcery)
                        })
                })
                .count();
            if spells >= 2 {
                crate::effect::execute(
                    &EffectOp::AddMana {
                        player: PlayerRef::Controller,
                        colors: vec![ManaColor::B, ManaColor::B, ManaColor::B],
                    },
                    ctx,
                    state,
                );
            }
        }
    }
}

/// Returns every battlefield artifact `owner` owns (every artifact when
/// `owner` is `None`) to its owner's hand.
fn return_artifacts(state: &mut GameState, owner: Option<PlayerId>) {
    let artifacts = battlefield(state)
        .filter(|&object| {
            crate::engine::object_has_type(state, object, CardType::Artifact)
                && owner.is_none_or(|player| state.objects.get(object).owner == player)
        })
        .collect::<Vec<_>>();
    event::propose_and_commit_batch(
        state,
        artifacts
            .into_iter()
            .map(|object| ProposedEvent::zone_change(object, Zone::Hand))
            .collect(),
    );
}

/// Runs a leaf inside the resumable interpreter. `Ok(true)` when it staged a
/// choice and the continuation must suspend.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &S1cOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    let controller = continuation.ctx.controller;
    match op {
        S1cOpV1::MasticoreUpkeepV1 => {
            let Some(contract) = continuation.ctx.ability_source_contract else {
                return Ok(false);
            };
            let source = ObjectLinkV4 {
                object: contract.source,
                zone_change_count: contract.zone_change_count,
            };
            if !link_live_in(state, source, Zone::Battlefield) {
                return Ok(false);
            }
            let hand = state.players[controller.index()]
                .hand
                .iter()
                .map(|&card| bind(state, card))
                .collect::<Vec<_>>();
            if hand.is_empty() {
                sacrifice(state, source.object);
                return Ok(false);
            }
            offer(
                continuation,
                S1cChoiceV1::MasticoreDiscardV1 { source },
                controller,
                hand,
                (0, 1),
                path,
            );
            Ok(true)
        }
        _ => {
            let ctx = continuation.ctx.clone();
            execute(op, &ctx, state);
            Ok(false)
        }
    }
}

/// Applies the answer to one of this module's selections. `Ok(true)` when it
/// staged another selection.
pub(crate) fn finish_selection(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    choice: &E3ChoiceV1,
    kind: &S1cChoiceV1,
    bindings: Vec<EffectObjectBinding>,
) -> Result<bool, String> {
    match kind {
        S1cChoiceV1::MasticoreDiscardV1 { source } => {
            let Some(card) = bindings.first() else {
                // "Sacrifice this creature unless you discard a card."
                if link_live_in(state, *source, Zone::Battlefield) {
                    sacrifice(state, source.object);
                }
                return Ok(false);
            };
            crate::engine::commit_discarded_card(state, card.object);
            if state.objects.get(card.object).zone == Zone::Hand {
                return Ok(false);
            }
            // The reflexive trigger: "destroy target nonland permanent an
            // opponent controls with mana value less than or equal to the
            // mana value of the discarded card."
            let value = crate::engine::object_mana_value(state, card.object).min(255) as u8;
            let spec = TargetSpec::StandardV1(
                crate::standard_cards_v1::StandardTargetV1::OpponentNonlandPermanentManaValueAtMost(
                    value,
                ),
            );
            let candidates =
                crate::engine::legal_targets_for_controller(spec, &[], choice.player, state)
                    .into_iter()
                    .filter_map(|target| match target {
                        Target::Object(object) => Some(bind(state, object)),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
            if candidates.is_empty() {
                return Ok(false);
            }
            offer(
                continuation,
                S1cChoiceV1::MasticoreDestroyV1,
                choice.player,
                candidates,
                (1, 1),
                choice.path.clone(),
            );
            Ok(true)
        }
        S1cChoiceV1::MasticoreDestroyV1 => {
            if let Some(target) = bindings.first() {
                if state.objects.get(target.object).zone == Zone::Battlefield
                    && !crate::engine::has_effective_keyword(
                        state,
                        target.object,
                        Keywords::INDESTRUCTIBLE,
                    )
                    && !crate::eternal_vintage_e7b_v1::statics::regenerate_instead(
                        state,
                        target.object,
                    )
                {
                    event::propose_and_commit(
                        state,
                        ProposedEvent::zone_change(target.object, Zone::Graveyard),
                    );
                }
            }
            Ok(false)
        }
    }
}
// ---- Statics and hooks ------------------------------------------------------------------

pub(crate) mod statics {
    use super::*;

    /// Light of Day: "Black creatures can't attack or block." Whether
    /// `object` is a black creature while a Light of Day is on the
    /// battlefield.
    fn banned_by_light_of_day(state: &GameState, object: ObjectId) -> bool {
        let black = crate::card_def::mana_color_mask(ManaColor::B);
        any_active(state, LIGHT_OF_DAY)
            && is_creature(state, object)
            && crate::engine::object_color_mask(state, object) & black != 0
    }

    /// Whether Light of Day keeps `object` from attacking.
    pub(crate) fn cant_attack(state: &GameState, object: ObjectId) -> bool {
        banned_by_light_of_day(state, object)
    }

    /// Whether Light of Day lets `blocker` block.
    pub(crate) fn blocker_allowed(state: &GameState, blocker: ObjectId) -> bool {
        !banned_by_light_of_day(state, blocker)
    }

    /// Urborg, Tomb of Yawgmoth: "Each land is a Swamp in addition to its
    /// other land types."
    pub(crate) fn is_extra_swamp(state: &GameState, object: ObjectId) -> bool {
        state.objects.try_get(object).is_some_and(|live| {
            live.zone == Zone::Battlefield
                && live.v4.face_down_v1.is_none()
                && CARD_DEFS.get(live.card_def as usize).is_some_and(|def| {
                    def.types_for_face(live.v4.face_index)
                        .contains(&CardType::Land)
                })
        }) && any_active(state, URBORG)
    }

    /// Urborg: a land that is a Swamp only through it has "{T}: Add {B}."
    pub(crate) fn granted_mana_colors(state: &GameState, object: ObjectId) -> Vec<ManaColor> {
        if !is_extra_swamp(state, object) {
            return Vec::new();
        }
        let live = state.objects.get(object);
        let def = &CARD_DEFS[live.card_def as usize];
        let own_black = def
            .primary_mana_ability_choices(live.v4.chosen_color)
            .contains(&ManaColor::B)
            || def
                .additional_mana_abilities
                .iter()
                .any(|ability| ability.colors.contains(&ManaColor::B))
            || def
                .restricted_mana_abilities
                .iter()
                .any(|ability| ability.colors.contains(&ManaColor::B));
        if own_black {
            return Vec::new();
        }
        vec![ManaColor::B]
    }

    /// Sphinx of the Steel Wind: "Protection from red and from green", as a
    /// color mask.
    pub(crate) fn protection_mask(state: &GameState, host: ObjectId) -> u8 {
        if active_named(state, host, SPHINX_OF_THE_STEEL_WIND) {
            crate::card_def::mana_color_mask(ManaColor::R)
                | crate::card_def::mana_color_mask(ManaColor::G)
        } else {
            0
        }
    }

    /// Whether the draw-step marker must be recorded in this game: a
    /// Hullbreacher is somewhere in the game.
    pub(crate) fn needs_draw_step_marker(state: &GameState) -> bool {
        let Some(hullbreacher) = card_id_by_name(HULLBREACHER) else {
            return false;
        };
        state
            .objects
            .iter()
            .any(|(_, object)| object.card_def == hullbreacher)
    }

    /// Hullbreacher: "If an opponent would draw a card except the first one
    /// they draw in each of their draw steps, instead you create a Treasure
    /// token." The Hullbreacher controller who gets the Treasure when
    /// `drawer`'s draw is replaced.
    pub(crate) fn hullbreacher_replaces_draw(
        state: &GameState,
        drawer: PlayerId,
    ) -> Option<PlayerId> {
        let hullbreacher = card_id_by_name(HULLBREACHER)?;
        let controller = battlefield(state).find_map(|object| {
            let live = state.objects.get(object);
            (live.card_def == hullbreacher
                && live.controller != drawer
                && active_named(state, object, HULLBREACHER))
            .then_some(live.controller)
        })?;
        (!crate::eternal_cards_v1::is_first_draw_in_draw_step(state, drawer))
            .then_some(controller)
    }

    /// Ravenous Trap: how many cards `player` owns that were put into a
    /// graveyard from anywhere this turn (since the last upkeep began).
    pub(crate) fn opponent_cards_to_graveyard_this_turn(
        state: &GameState,
        player: PlayerId,
    ) -> usize {
        let history = &state.engine.event_history;
        let start = history
            .iter()
            .rposition(|event| matches!(event, crate::event::CommittedEvent::UpkeepBegan { .. }))
            .map_or(0, |index| index + 1);
        history[start..]
            .iter()
            .filter(|event| {
                let crate::event::CommittedEvent::ZoneChange { object, to, .. } = event else {
                    return false;
                };
                *to == Zone::Graveyard
                    && state.objects.try_get(*object).is_some_and(|live| {
                        live.owner == player
                            && !live.v4.is_token
                            && live.spell_copy_origin.is_none()
                    })
            })
            .count()
    }

    /// Creates the Treasure token Hullbreacher's replacement gives `player`.
    pub(crate) fn create_treasure(state: &mut GameState, player: PlayerId) {
        event::propose_and_commit(
            state,
            ProposedEvent::create_token(token_id(TREASURE_TOKEN), player),
        );
    }
}

// ---- Targets ------------------------------------------------------------------------

/// Target filters of this module's cards, carried as `StandardTargetV1`
/// variants. `standard_cards_v1::legal_targets` and `target_matches` ask
/// these first.
pub(crate) mod targets {
    use crate::card_def::CardType;
    use crate::ids::{ObjectId, PlayerId};
    use crate::standard_cards_v1::StandardTargetV1;
    use crate::state::{GameState, Target, Zone};

    /// Whether `target` is one of this module's filters.
    pub(crate) fn owns(target: StandardTargetV1) -> bool {
        matches!(
            target,
            StandardTargetV1::CreatureOrPlaneswalkerManaValueAtMostV1(_)
                | StandardTargetV1::OpponentCreatureOrEnchantmentV1
        )
    }

    /// Whether `object` satisfies this module's filter `target` for
    /// `controller`, or `None` for every other filter.
    pub(crate) fn target_matches(
        target: StandardTargetV1,
        controller: PlayerId,
        object: ObjectId,
        state: &GameState,
    ) -> Option<bool> {
        if !owns(target) {
            return None;
        }
        let Some(live) = state.objects.try_get(object) else {
            return Some(false);
        };
        if live.zone != Zone::Battlefield {
            return Some(false);
        }
        let has = |card_type| crate::engine::object_has_type(state, object, card_type);
        Some(match target {
            StandardTargetV1::CreatureOrPlaneswalkerManaValueAtMostV1(maximum) => {
                (has(CardType::Creature) || has(CardType::Planeswalker))
                    && crate::engine::object_mana_value(state, object) <= u16::from(maximum)
            }
            StandardTargetV1::OpponentCreatureOrEnchantmentV1 => {
                live.controller != controller
                    && (has(CardType::Creature) || has(CardType::Enchantment))
            }
            _ => return None,
        })
    }

    /// The legal choices for this module's filter `target` after the already
    /// chosen `prefix`, or `None` for every other filter.
    pub(crate) fn legal_targets(
        target: StandardTargetV1,
        controller: PlayerId,
        prefix: &[Target],
        state: &GameState,
    ) -> Option<Vec<Target>> {
        if !owns(target) {
            return None;
        }
        if !prefix.is_empty() {
            return Some(Vec::new());
        }
        Some(
            [PlayerId::P0, PlayerId::P1]
                .into_iter()
                .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
                .filter(|&object| target_matches(target, controller, object, state) == Some(true))
                .map(Target::Object)
                .collect(),
        )
    }
}
