//! Card programs for the eternal catalog's Premodern sideboard wave P3h
//! (`docs/design/eternal_formats_support_v1.md`): Overgrown Estate and Decree
//! of Pain (completing Premodern Other Control's 75), together with Full
//! English Breakfast's four sideboard cards (Enlightened Tutor, Auramancer,
//! Monk Realist and Ghitu Slinger).
//!
//! `eternal_cards_v1` forwards Decree of Pain's spell leaf, the creature entry
//! triggers and the cycling trigger here, wave S1m's recipe tables fall
//! through to `build_eternal_p3h_v1.rs`, wave P2d's `echo_cost` registers
//! Ghitu Slinger's `{2}{R}` echo, and wave P1b's `activation_paid` queues
//! Decree of Pain's cycling trigger. Every table is keyed by a printed name
//! that occurs only in the eternal registry.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

pub(crate) mod targets;

use crate::card_def::{CardType, Keywords, TargetSpec, CARD_DEFS};
use crate::effect::{
    EffectContinuation, EffectFrame, EffectOp, ExecCtx, LibraryCardFilter,
    LibrarySearchDestinationV1, ObjectRef, PlayerRef, TargetRef,
};
use crate::eternal_cards_v1::EternalOpV1;
use crate::event::{self, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::{StandardOpV1, StandardTargetV1};
use crate::state::{AbilitySourceContractV4, GameState, Zone};
use crate::trigger::{TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

pub(crate) const AURAMANCER: &str = "Auramancer";
pub(crate) const DECREE_OF_PAIN: &str = "Decree of Pain";
pub(crate) const GHITU_SLINGER: &str = "Ghitu Slinger";
pub(crate) const MONK_REALIST: &str = "Monk Realist";

/// Auramancer's entry trigger target: "target enchantment card from your
/// graveyard".
pub(crate) const AURAMANCER_TARGET: TargetSpec =
    TargetSpec::StandardV1(StandardTargetV1::EnchantmentCardInOwnGraveyardV1);

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::P3hV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum P3hOpV1 {
    /// Decree of Pain: destroy all creatures without regeneration, then draw a
    /// card for each creature destroyed this way.
    DecreeOfPainV1,
}

impl P3hOpV1 {
    /// Runs in the resumable interpreter so draws after simultaneous
    /// destruction can offer Dredge or competing draw replacements, including
    /// for a Dredge creature Decree of Pain itself put into the graveyard.
    pub(crate) fn contains_player_choice(&self) -> bool {
        match self {
            Self::DecreeOfPainV1 => true,
        }
    }
}

fn p3h(op: P3hOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::P3hV1(op)))
}

// ---- Programs -----------------------------------------------------------------------

/// Decree of Pain: "Destroy all creatures. They can't be regenerated. Draw a
/// card for each creature destroyed this way."
pub fn decree_of_pain() -> EffectOp {
    p3h(P3hOpV1::DecreeOfPainV1)
}

/// Decree of Pain's cycling trigger: "When you cycle this card, all creatures
/// get -2/-2 until end of turn."
pub fn decree_of_pain_cycled() -> EffectOp {
    crate::eternal_premodern_p3g_v1::infest()
}

/// Enlightened Tutor: "Search your library for an artifact or enchantment
/// card, reveal it, then shuffle and put that card on top."
pub fn enlightened_tutor() -> EffectOp {
    EffectOp::SearchLibraryCardsToDestination {
        player: PlayerRef::Controller,
        filter: LibraryCardFilter::ArtifactOrEnchantmentCardV1,
        max_targets: 1,
        destination: LibrarySearchDestinationV1::LibraryTopAfterShuffle,
    }
}

/// Auramancer: "When this creature enters, you may return target enchantment
/// card from your graveyard to your hand."
pub fn auramancer_enters() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: vec![
            EffectOp::MoveAllTargets {
                to_zone: Zone::Hand,
            },
            EffectOp::Sequence(Vec::new()),
        ],
    }
}

/// Monk Realist: "When this creature enters, destroy target enchantment."
pub fn monk_realist_enters() -> EffectOp {
    EffectOp::DestroyObject {
        object: ObjectRef::Target(0),
    }
}

/// Ghitu Slinger: "When this creature enters, it deals 2 damage to any
/// target."
pub fn ghitu_slinger_enters() -> EffectOp {
    EffectOp::DealDamage {
        target: TargetRef::Target(0),
        amount: 2,
    }
}

// ---- Triggered abilities ------------------------------------------------------------

const fn etb_trigger(effect: fn() -> EffectOp) -> TriggeredAbilityDef {
    TriggeredAbilityDef {
        condition: TriggerCondition::Etb,
        home_zone: Zone::Battlefield,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index: 0,
        effect,
    }
}

const AURAMANCER_TRIGGERS: [TriggeredAbilityDef; 1] = [etb_trigger(auramancer_enters)];
const MONK_REALIST_TRIGGERS: [TriggeredAbilityDef; 1] = [etb_trigger(monk_realist_enters)];
const GHITU_SLINGER_TRIGGERS: [TriggeredAbilityDef; 1] = [etb_trigger(ghitu_slinger_enters)];

/// Triggered abilities of this module's cards, the tail of the trigger tables.
/// Ghitu Slinger's echo is queued by wave P2d's upkeep hook.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        AURAMANCER => &AURAMANCER_TRIGGERS,
        MONK_REALIST => &MONK_REALIST_TRIGGERS,
        GHITU_SLINGER => &GHITU_SLINGER_TRIGGERS,
        _ => &[],
    }
}

/// Target specs of this module's printed and queued triggered abilities.
/// Ghitu Slinger's echo trigger falls through to wave P2d.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    match name {
        AURAMANCER if *effect == auramancer_enters() => Some(AURAMANCER_TARGET),
        MONK_REALIST if *effect == monk_realist_enters() => Some(TargetSpec::EnchantmentPermanent),
        GHITU_SLINGER if *effect == ghitu_slinger_enters() => Some(TargetSpec::AnyTarget),
        _ => delayed_effect_matches(name, effect).then_some(TargetSpec::None),
    }
}

/// Queued triggers owned by this module's cards.
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    name == DECREE_OF_PAIN && *effect == decree_of_pain_cycled()
}

/// Called as an activated ability's costs have been paid: queues Decree of
/// Pain's cycling trigger (702.29c), even when a replacement effect exiled the
/// discarded card instead of putting it into the graveyard.
pub(crate) fn activation_paid(state: &mut GameState, source: ObjectId, ability_index: u8) {
    if !cfg!(feature = "eternal-fixtures") {
        return;
    }
    let Some(live) = state.objects.try_get(source) else {
        return;
    };
    let Some(name) = CARD_DEFS
        .get(usize::from(live.card_def))
        .filter(|def| def.is_executable())
        .map(|def| def.name)
    else {
        return;
    };
    if (name, ability_index) == (DECREE_OF_PAIN, 0) && live.zone != Zone::Hand {
        let owner = live.owner;
        let contract = AbilitySourceContractV4::capture(state, source);
        crate::eternal_premodern_p1a_v1::queue_trigger(
            state,
            contract,
            owner,
            decree_of_pain_cycled(),
            None,
        );
    }
}

// ---- Execution ----------------------------------------------------------------------

/// Simultaneously destroys every non-indestructible creature on the
/// battlefield without regeneration, returning the count of creatures
/// destroyed this way (701.8). Creature tokens and creatures moved to exile by
/// a zone-change replacement effect (Rest in Peace, Leyline of the Void,
/// Dauthi Voidwalker) were still destroyed and count; indestructible creatures
/// and noncreature permanents do not.
fn destroy_all_creatures_no_regeneration(state: &mut GameState) -> u32 {
    let creatures = [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
        .filter(|&object| crate::engine::object_has_type(state, object, CardType::Creature))
        .filter(|&object| {
            !crate::engine::has_effective_keyword(state, object, Keywords::INDESTRUCTIBLE)
        })
        .map(|object| crate::eternal_legacy_e9a_v1::live_link(state, object))
        .collect::<Vec<_>>();
    if creatures.is_empty() {
        return 0;
    }
    event::propose_and_commit_batch(
        state,
        creatures
            .iter()
            .map(|link| ProposedEvent::zone_change(link.object, Zone::Graveyard))
            .collect(),
    );
    if state.engine.halted.is_some() || crate::graveyard_order_v1::paused(state) {
        return 0;
    }
    let destroyed = creatures
        .iter()
        .filter(|&&link| {
            !crate::eternal_legacy_e9a_v1::link_live_in(state, link, Zone::Battlefield)
        })
        .count();
    u32::try_from(destroyed).unwrap_or(u32::MAX)
}

/// Runs one of this module's leaves in synchronous execution.
pub(crate) fn execute(op: &P3hOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        P3hOpV1::DecreeOfPainV1 => {
            let destroyed = destroy_all_creatures_no_regeneration(state);
            if destroyed > 0
                && state.engine.halted.is_none()
                && !crate::graveyard_order_v1::paused(state)
            {
                crate::effect::execute(
                    &EffectOp::DrawCards {
                        player: PlayerRef::Controller,
                        count: destroyed,
                    },
                    ctx,
                    state,
                );
            }
        }
    }
}

/// Runs one of this module's leaves in the resumable interpreter.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &P3hOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    match op {
        P3hOpV1::DecreeOfPainV1 => {
            let destroyed = destroy_all_creatures_no_regeneration(state);
            if destroyed > 0
                && state.engine.halted.is_none()
                && !crate::graveyard_order_v1::paused(state)
            {
                continuation.frames.push(EffectFrame::Program {
                    op: EffectOp::DrawCards {
                        player: PlayerRef::Controller,
                        count: destroyed,
                    },
                    path,
                });
            }
            Ok(false)
        }
    }
}
