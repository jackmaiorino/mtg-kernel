//! Card programs for the Pauper, Legacy and Vintage catalog
//! (`data/eternal_v1/cards_v1.json`, `docs/design/eternal_formats_support_v1.md`).
//!
//! Every table here is keyed by a printed name that occurs only in the
//! eternal registry, so the Pauper, FDN and Standard catalogs never reach it.
//! The shared primitives it adds (energy, replicate, two-target spells, fixed
//! two-color mana) are inert until one of these cards uses them. Behavior was
//! read from each card's Oracle text and, where present, its XMage source.

use crate::card_def::{DynamicValueDef, TargetSpec, CARD_DEFS};
use crate::effect::{
    EffectOp, ExecCtx, LibraryLookCount, LibraryPickFilterV1, LibraryPickRule,
    LibraryPickSelectionV1, ObjectRef, PlayerRef, TargetRef,
};
use crate::event::{self, ProposedEvent};
use crate::ids::PlayerId;
use crate::mana::{Cost, ManaColor, Pip};
use crate::standard_cards_v1::{StandardChosenActionV1, StandardOpV1, StandardPermanentFilterV1};
use crate::state::{GameState, Zone};
use crate::trigger::{TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

/// Per-game state for this module's cards. Absent until one of them needs
/// it, so every other catalog keeps its snapshots and hashes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EternalStateV1 {
    /// Energy counters per player (107.14), indexed by `PlayerId::index`.
    #[serde(default)]
    pub energy: [u32; 2],
}

/// Effect leaves owned by this module, carried as
/// `StandardOpV1::EternalV1` so `EffectOp` keeps its shape.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EternalOpV1 {
    /// The controller gets `amount` energy counters ("you get {E}{E}").
    GainEnergy { amount: u8 },
    /// Target `index` gets `pumps[index]` until end of turn, for each target
    /// still legal on resolution (608.2b). Each "target creature" may name
    /// the same creature (115.3), as in Agony Warp.
    PumpEachLegalTargetV1 { pumps: Vec<(i32, i32)> },
    /// Draw `normal` cards, or `from_graveyard` instead if this spell was
    /// cast from a graveyard (Plunder the Trollshaws).
    DrawCastFromGraveyardV1 { normal: u8, from_graveyard: u8 },
    /// The controller shuffles their graveyard into their library
    /// (Campfire). No commanders exist in these games.
    ShuffleGraveyardIntoLibrary,
}

/// The number of energy counters `player` has.
pub fn energy(state: &GameState, player: PlayerId) -> u32 {
    state
        .eternal_v1
        .as_ref()
        .map_or(0, |eternal| eternal.energy[player.index()])
}

/// Pays an energy cost, or returns `None` without change if `player` has
/// fewer than `amount` energy counters.
pub(crate) fn pay_energy(state: &mut GameState, player: PlayerId, amount: u8) -> Option<()> {
    let available = energy(state, player);
    let remaining = available.checked_sub(u32::from(amount))?;
    if amount > 0 {
        state.eternal_v1.get_or_insert_with(Default::default).energy[player.index()] = remaining;
    }
    Some(())
}

/// Replicate (702.56): the cost paid once per copy. The number of payments
/// is announced like X and stored as the spell's X value; the spell's mana
/// cost has no X, so its mana value is unchanged.
pub fn replicate_cost(name: &str) -> Option<Cost> {
    match name {
        "Lose Focus" => Some(Cost {
            pips: &[Pip::Colored(ManaColor::U)],
            generic: 0,
            x_count: 0,
        }),
        _ => None,
    }
}

const fn etb(effect: fn() -> EffectOp) -> TriggeredAbilityDef {
    TriggeredAbilityDef {
        condition: TriggerCondition::Etb,
        home_zone: Zone::Battlefield,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index: 0,
        effect,
    }
}

const MELDED_MOXITE: &str = "Melded Moxite";
const INVENTORS_AXE: &str = "Inventor's Axe";
const BALEFUL_STRIX: &str = "Baleful Strix";
const GIANTS_BOULDER: &str = "Giant's Boulder";
const AUGUR_OF_BOLAS: &str = "Augur of Bolas";
const THORN_OF_THE_BLACK_ROSE: &str = "Thorn of the Black Rose";
const DIMIR_AQUEDUCT: &str = "Dimir Aqueduct";

/// "When this artifact enters, you may discard a card. If you do, draw two
/// cards."
fn may_discard_then_draw_two() -> EffectOp {
    EffectOp::MayPayCostThen {
        discard: 1,
        sacrifice_lands: 0,
        return_permanent: None,
        then: Box::new(EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 2,
        }),
        otherwise: None,
    }
}

/// "When this Equipment enters, you get {E}{E}."
fn get_two_energy() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::GainEnergy {
        amount: 2,
    }))
}

/// "When this Equipment enters, attach it to target creature you control."
fn attach_to_target() -> EffectOp {
    EffectOp::AttachSourceToTarget {
        object: ObjectRef::Target(0),
    }
}

fn draw_one() -> EffectOp {
    EffectOp::DrawCards {
        player: PlayerRef::Controller,
        count: 1,
    }
}

fn scry_two() -> EffectOp {
    EffectOp::Scry {
        player: PlayerRef::Controller,
        count: 2,
    }
}

/// "Look at the top three cards of your library. You may reveal an instant
/// or sorcery card from among them and put it into your hand. Put the rest
/// on the bottom of your library in any order." The look is private; only
/// the revealed card becomes public.
fn augur_look() -> EffectOp {
    EffectOp::LookTopSelectV1 {
        player: PlayerRef::Controller,
        count: LibraryLookCount::Fixed(3),
        rule: LibraryPickRule {
            pick: 1,
            choose_rest_order: true,
            selection: Some(LibraryPickSelectionV1 {
                without_mana_cost: crate::engine::FreeCastV1(false),
                filter: LibraryPickFilterV1::InstantOrSorcery,
                optional: true,
                destination: Zone::Hand,
                reveal_selected: true,
            }),
        },
        pick_x: false,
    }
}

fn become_monarch() -> EffectOp {
    EffectOp::BecomeMonarch
}

/// "When this land enters, return a land you control to its owner's hand."
/// Not targeted; the land is chosen on resolution and may be this one.
fn return_controlled_land() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::PlayerChoosesControlledPermanent {
        player: PlayerRef::Controller,
        filter: StandardPermanentFilterV1::Land,
        action: StandardChosenActionV1::ReturnToOwnersHand,
    })
}

const MELDED_MOXITE_TRIGGERS: [TriggeredAbilityDef; 1] = [etb(may_discard_then_draw_two)];
const INVENTORS_AXE_TRIGGERS: [TriggeredAbilityDef; 2] =
    [etb(get_two_energy), etb(attach_to_target)];
const BALEFUL_STRIX_TRIGGERS: [TriggeredAbilityDef; 1] = [etb(draw_one)];
const GIANTS_BOULDER_TRIGGERS: [TriggeredAbilityDef; 1] = [etb(scry_two)];
const AUGUR_OF_BOLAS_TRIGGERS: [TriggeredAbilityDef; 1] = [etb(augur_look)];
const THORN_OF_THE_BLACK_ROSE_TRIGGERS: [TriggeredAbilityDef; 1] = [etb(become_monarch)];
const DIMIR_AQUEDUCT_TRIGGERS: [TriggeredAbilityDef; 1] = [etb(return_controlled_land)];

/// Triggered abilities of this module's cards.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        MELDED_MOXITE => &MELDED_MOXITE_TRIGGERS,
        INVENTORS_AXE => &INVENTORS_AXE_TRIGGERS,
        BALEFUL_STRIX => &BALEFUL_STRIX_TRIGGERS,
        GIANTS_BOULDER => &GIANTS_BOULDER_TRIGGERS,
        AUGUR_OF_BOLAS => &AUGUR_OF_BOLAS_TRIGGERS,
        THORN_OF_THE_BLACK_ROSE => &THORN_OF_THE_BLACK_ROSE_TRIGGERS,
        DIMIR_AQUEDUCT => &DIMIR_AQUEDUCT_TRIGGERS,
        _ => &[],
    }
}

/// Target specs of this module's triggered abilities, per ability.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    match name {
        INVENTORS_AXE => Some(if *effect == attach_to_target() {
            TargetSpec::ControlledCreature
        } else {
            TargetSpec::None
        }),
        _ => None,
    }
}

/// Agony Warp: "Target creature gets -3/-0 until end of turn. Target
/// creature gets -0/-3 until end of turn."
pub fn agony_warp() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(
        EternalOpV1::PumpEachLegalTargetV1 {
            pumps: vec![(-3, 0), (0, -3)],
        },
    ))
}

/// Plunder the Trollshaws: "Draw a card. If this spell was cast from a
/// graveyard, draw two cards instead." Flashback is the card's own
/// `CardDef::flashback`.
pub fn plunder_the_trollshaws() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(
        EternalOpV1::DrawCastFromGraveyardV1 {
            normal: 1,
            from_graveyard: 2,
        },
    ))
}

/// Campfire's second ability: commanders do not exist in these games, so
/// it shuffles the controller's graveyard into their library.
pub fn campfire_shuffle_graveyard() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(
        EternalOpV1::ShuffleGraveyardIntoLibrary,
    ))
}

pub(crate) fn execute(op: &EternalOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        EternalOpV1::GainEnergy { amount } => {
            let eternal = state.eternal_v1.get_or_insert_with(Default::default);
            let counters = &mut eternal.energy[ctx.controller.index()];
            *counters = counters.saturating_add(u32::from(*amount));
        }
        EternalOpV1::PumpEachLegalTargetV1 { pumps } => {
            let Some(def) = state
                .objects
                .try_get(ctx.source)
                .and_then(|source| CARD_DEFS.get(source.card_def as usize))
            else {
                return;
            };
            let spec = def.target_spec;
            for (index, &(power, toughness)) in pumps.iter().enumerate() {
                let Ok(slot) = u8::try_from(index) else {
                    return;
                };
                if !ctx.target_incarnation_matches(index, state)
                    || !crate::engine::effect_target_is_legal(
                        state,
                        ctx.source,
                        ctx.controller,
                        spec,
                        &ctx.targets,
                        index,
                    )
                {
                    continue;
                }
                crate::effect::execute(
                    &EffectOp::PumpTargetUntilEndOfTurnDynamic {
                        target: TargetRef::Target(slot),
                        power: DynamicValueDef::Fixed(power),
                        toughness: DynamicValueDef::Fixed(toughness),
                    },
                    ctx,
                    state,
                );
            }
        }
        EternalOpV1::DrawCastFromGraveyardV1 {
            normal,
            from_graveyard,
        } => {
            let cast_from_graveyard = state.objects.try_get(ctx.source).is_some_and(|source| {
                source.zone == Zone::Stack
                    && source
                        .v4
                        .spell_cast_origin
                        .is_some_and(|origin| origin.origin_zone == Zone::Graveyard)
            });
            crate::effect::execute(
                &EffectOp::DrawCards {
                    player: PlayerRef::Controller,
                    count: u32::from(if cast_from_graveyard {
                        *from_graveyard
                    } else {
                        *normal
                    }),
                },
                ctx,
                state,
            );
        }
        EternalOpV1::ShuffleGraveyardIntoLibrary => {
            let player = ctx.controller;
            let cards = state.players[player.index()].graveyard.clone();
            if !cards.is_empty() {
                event::propose_and_commit_batch(
                    state,
                    cards
                        .into_iter()
                        .map(|card| ProposedEvent::zone_change(card, Zone::Library))
                        .collect(),
                );
            }
            if state.shuffle_library(player).is_err() {
                state.engine.halted = Some((
                    crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                    ctx.source,
                ));
            }
        }
    }
}
