//! Card programs for the eternal catalog's Vintage cards (wave E4: Jewel
//! Shops and Mono-White Initiative, `docs/design/eternal_formats_support_v1.md`).
//!
//! `eternal_cards_v1` forwards its effect leaves, trigger conditions, state
//! and triggered-ability tables here. Every table is keyed by a printed name
//! that occurs only in the eternal registry, so the Pauper, FDN and Standard
//! catalogs never reach it. Behavior was read from each card's current
//! Oracle text and, where present, its XMage source.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{CardType, Supertype, TargetSpec, CARD_DEFS};
use crate::effect::{EffectOp, ExecCtx, PlayerRef};
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId, StackItemId};
use crate::mana::ManaColor;
use crate::standard_cards_v1::{
    StandardChosenActionV1, StandardOpV1, StandardPermanentFilterV1, StandardTargetV1,
};
use crate::state::{AbilitySourceContractV4, GameState, ObjectLinkV4, Target, Zone};
use crate::trigger::{PendingTrigger, TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

/// Per-game state for this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Default (and absent from snapshots)
/// until one of these cards needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VintageStateV1 {
    /// Printed triggers detected outside the event stream (Coveted Jewel's
    /// unblocked attackers, Mana Vault's draw step), waiting for the next
    /// trigger collection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queued_triggers: Vec<QueuedTriggerV1>,
    /// Turns their players chose to skip with a tapped Time Vault, applied
    /// when the turn would begin.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub turn_skips: Vec<TurnSkipV1>,
    /// Permanents with protection from creatures until the end of the turn
    /// with this turn key (Seasoned Dungeoneer).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub protected_from_creatures: Vec<(ObjectLinkV4, u32)>,
    /// How much mana, and whether any colored mana, was spent to cast each
    /// spell still on the stack (Vexing Bauble, Void Mirror).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cast_mana: Vec<CastManaV1>,
}

impl VintageStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// A printed triggered ability queued by an engine hook.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueuedTriggerV1 {
    pub source: AbilitySourceContractV4,
    pub controller: PlayerId,
    pub op: VintageOpV1,
}

/// Time Vault: "If you would begin your turn while this artifact is tapped,
/// you may skip that turn instead. If you do, untap this artifact."
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnSkipV1 {
    pub player: PlayerId,
    pub vault: ObjectLinkV4,
}

/// The mana spent to cast one spell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CastManaV1 {
    pub spell: ObjectLinkV4,
    pub total: u16,
    pub colored: bool,
}

/// Trigger conditions owned by this module, carried as
/// `eternal_cards_v1::EternalTriggerV1::VintageV1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VintageTriggerV1 {
    /// "Whenever a player casts a spell, if no mana was spent to cast it"
    /// (Vexing Bauble).
    SpellCastWithoutManaV1,
    /// "Whenever a player casts a spell, if no colored mana was spent to
    /// cast it" (Void Mirror).
    SpellCastWithoutColoredManaV1,
    /// "At the beginning of each opponent's upkeep" (White Plume
    /// Adventurer).
    OpponentUpkeepV1,
    /// The beginning of the end step of the turn before one of the source's
    /// controller's turns, while the source is tapped: the window in which
    /// Time Vault's controller chooses whether to skip that turn.
    EndStepBeforeYourTurnWhileTappedV1,
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::VintageV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VintageOpV1 {
    /// Mana Crypt: "flip a coin. If you lose the flip, this artifact deals
    /// `damage` damage to you." The flip uses the game's seeded randomness.
    FlipCoinLoseSourceDamagesControllerV1 { damage: u8 },
    /// Coveted Jewel: "that player draws three cards and gains control of
    /// this artifact. Untap it." That player is the attacking opponent of
    /// the Jewel's controller when the ability triggered.
    CovetedJewelChangesHandsV1,
    /// Trigger template: "counter that spell" for the spell whose cast
    /// triggered the ability.
    BindCounterCastSpellV1,
    /// Counter the spell with this exact stack incarnation, if it is still
    /// on the stack.
    CounterCastSpellV1 { spell: StackItemId },
    /// Untap this ability's source incarnation (Mana Vault's upkeep
    /// payment).
    UntapSourceV1,
    /// Mana Vault: "if this artifact is tapped, it deals 1 damage to you."
    SourceDamagesControllerIfTappedV1 { damage: u8 },
    /// Time Vault's chosen skip: record it for the controller's next turn.
    ScheduleTurnSkipV1,
    /// White Plume Adventurer: untap all creatures the controller controls
    /// if they have completed a dungeon.
    UntapAllCreaturesIfCompletedDungeonV1,
    /// Target 0 gains protection from creatures until end of turn (Seasoned
    /// Dungeoneer).
    ProtectTargetFromCreaturesV1,
    /// Paradoxical Outcome: return every still-legal target to its owner's
    /// hand, then draw a card for each card returned to the controller's
    /// hand.
    ParadoxicalOutcomeV1,
}

fn vintage(op: VintageOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(
        crate::eternal_cards_v1::EternalOpV1::VintageV1(op),
    ))
}

const MANA_CRYPT: &str = "Mana Crypt";
const MANA_VAULT: &str = "Mana Vault";
const TIME_VAULT: &str = "Time Vault";
const COVETED_JEWEL: &str = "Coveted Jewel";
const VEXING_BAUBLE: &str = "Vexing Bauble";
const VOID_MIRROR: &str = "Void Mirror";
const TRINISPHERE: &str = "Trinisphere";
const ARCHON_OF_EMERIA: &str = "Archon of Emeria";
const CLARION_CONQUEROR: &str = "Clarion Conqueror";
const KARN: &str = "Karn, the Great Creator";
const WHITE_PLUME_ADVENTURER: &str = "White Plume Adventurer";
const SEASONED_DUNGEONEER: &str = "Seasoned Dungeoneer";
const MISHRAS_WORKSHOP: &str = "Mishra's Workshop";

/// Cards whose "When this creature enters, you take the initiative" makes
/// them the Initiative designation's source (Avenging Hunter in Pauper).
pub(crate) fn initiative_granter(name: &str) -> bool {
    name == "Avenging Hunter"
        || (cfg!(feature = "eternal-fixtures")
            && matches!(name, SEASONED_DUNGEONEER | WHITE_PLUME_ADVENTURER))
}

const fn trigger(condition: TriggerCondition, effect: fn() -> EffectOp) -> TriggeredAbilityDef {
    TriggeredAbilityDef {
        condition,
        home_zone: Zone::Battlefield,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        face_index: 0,
        effect,
    }
}

const fn vintage_trigger(
    condition: VintageTriggerV1,
    effect: fn() -> EffectOp,
) -> TriggeredAbilityDef {
    trigger(
        TriggerCondition::StandardV1(crate::standard_cards_v1::StandardTriggerV1::EternalV1(
            crate::eternal_cards_v1::EternalTriggerV1::VintageV1(condition),
        )),
        effect,
    )
}

/// Mana Crypt: "At the beginning of your upkeep, flip a coin. If you lose
/// the flip, this artifact deals 3 damage to you."
fn mana_crypt_flip() -> EffectOp {
    vintage(VintageOpV1::FlipCoinLoseSourceDamagesControllerV1 { damage: 3 })
}

/// Mana Vault: "At the beginning of your upkeep, you may pay {4}. If you
/// do, untap this artifact."
fn mana_vault_upkeep() -> EffectOp {
    EffectOp::MayPayManaThen {
        player: PlayerRef::Controller,
        colored: Vec::new(),
        generic: 4,
        then: Box::new(vintage(VintageOpV1::UntapSourceV1)),
    }
}

/// Mana Vault: "At the beginning of your draw step, if this artifact is
/// tapped, it deals 1 damage to you."
fn mana_vault_draw_step() -> EffectOp {
    vintage(VintageOpV1::SourceDamagesControllerIfTappedV1 { damage: 1 })
}

/// Time Vault: whether to skip the controller's next turn and untap it then.
fn time_vault_skip_choice() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: vec![
            vintage(VintageOpV1::ScheduleTurnSkipV1),
            EffectOp::Sequence(Vec::new()),
        ],
    }
}

fn draw_three() -> EffectOp {
    EffectOp::DrawCards {
        player: PlayerRef::Controller,
        count: 3,
    }
}

fn coveted_jewel_changes_hands() -> EffectOp {
    vintage(VintageOpV1::CovetedJewelChangesHandsV1)
}

fn counter_cast_spell() -> EffectOp {
    vintage(VintageOpV1::BindCounterCastSpellV1)
}

fn take_initiative() -> EffectOp {
    EffectOp::TakeInitiative {
        player: PlayerRef::Controller,
    }
}

/// White Plume Adventurer: "untap a creature you control. If you've
/// completed a dungeon, untap all creatures you control instead."
fn white_plume_untap() -> EffectOp {
    EffectOp::Sequence(vec![
        vintage(VintageOpV1::UntapAllCreaturesIfCompletedDungeonV1),
        EffectOp::StandardV1(StandardOpV1::PlayerChoosesControlledPermanent {
            player: PlayerRef::Controller,
            filter: StandardPermanentFilterV1::CreatureUnlessCompletedDungeonV1,
            action: StandardChosenActionV1::UntapV1,
        }),
    ])
}

/// Seasoned Dungeoneer: "target attacking Cleric, Rogue, Warrior, or Wizard
/// gains protection from creatures until end of turn. It explores."
fn dungeoneer_attack() -> EffectOp {
    EffectOp::Sequence(vec![
        vintage(VintageOpV1::ProtectTargetFromCreaturesV1),
        EffectOp::ExploreTarget {
            object: crate::effect::ObjectRef::Target(0),
        },
    ])
}

const MANA_CRYPT_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::BeginningOfUpkeep {
        controller_only: true,
    },
    mana_crypt_flip,
)];
const MANA_VAULT_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::BeginningOfUpkeep {
        controller_only: true,
    },
    mana_vault_upkeep,
)];
const TIME_VAULT_TRIGGERS: [TriggeredAbilityDef; 1] = [vintage_trigger(
    VintageTriggerV1::EndStepBeforeYourTurnWhileTappedV1,
    time_vault_skip_choice,
)];
const COVETED_JEWEL_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, draw_three)];
const VEXING_BAUBLE_TRIGGERS: [TriggeredAbilityDef; 1] = [vintage_trigger(
    VintageTriggerV1::SpellCastWithoutManaV1,
    counter_cast_spell,
)];
const VOID_MIRROR_TRIGGERS: [TriggeredAbilityDef; 1] = [vintage_trigger(
    VintageTriggerV1::SpellCastWithoutColoredManaV1,
    counter_cast_spell,
)];
const WHITE_PLUME_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, take_initiative),
    vintage_trigger(VintageTriggerV1::OpponentUpkeepV1, white_plume_untap),
];
const SEASONED_DUNGEONEER_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, take_initiative),
    trigger(TriggerCondition::ControllerAttacks, dungeoneer_attack),
];

/// Triggered abilities of this module's cards.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        MANA_CRYPT => &MANA_CRYPT_TRIGGERS,
        MANA_VAULT => &MANA_VAULT_TRIGGERS,
        TIME_VAULT => &TIME_VAULT_TRIGGERS,
        COVETED_JEWEL => &COVETED_JEWEL_TRIGGERS,
        VEXING_BAUBLE => &VEXING_BAUBLE_TRIGGERS,
        VOID_MIRROR => &VOID_MIRROR_TRIGGERS,
        WHITE_PLUME_ADVENTURER => &WHITE_PLUME_TRIGGERS,
        SEASONED_DUNGEONEER => &SEASONED_DUNGEONEER_TRIGGERS,
        _ => &[],
    }
}

/// Whether `effect` is one of this module's trigger programs or a
/// trigger-time materialization of one, for `card`.
fn own_trigger_effect(name: &str, effect: &EffectOp) -> bool {
    triggers_for(name).iter().any(|trigger| {
        let template = (trigger.effect)();
        template == *effect || template_matches(&template, effect)
    }) || delayed_effect_matches(name, effect)
}

/// Target specs of this module's triggered abilities, per ability.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    if !own_trigger_effect(name, effect) {
        return None;
    }
    Some(
        if name == SEASONED_DUNGEONEER && *effect == dungeoneer_attack() {
            TargetSpec::StandardV1(StandardTargetV1::AttackingClericRogueWarriorOrWizardV1)
        } else {
            TargetSpec::None
        },
    )
}

/// Trigger-time bindings: "that spell" for Vexing Bauble and Void Mirror.
pub(crate) fn materialize_event(
    effect: &EffectOp,
    event: &CommittedEvent,
    state: &GameState,
) -> Option<EffectOp> {
    if *effect != counter_cast_spell() {
        return None;
    }
    let CommittedEvent::SpellCast { spell, .. } = event else {
        return None;
    };
    state
        .stack
        .iter()
        .find(|item| item.source == *spell && item.kind == crate::state::StackItemKind::Spell)
        .map(|item| {
            vintage(VintageOpV1::CounterCastSpellV1 {
                spell: item.v4.stack_item_id,
            })
        })
}

/// Whether `effect` is a trigger-time materialization of `template`.
pub(crate) fn template_matches(template: &EffectOp, effect: &EffectOp) -> bool {
    *template == counter_cast_spell()
        && matches!(
            effect,
            EffectOp::StandardV1(StandardOpV1::EternalV1(
                crate::eternal_cards_v1::EternalOpV1::VintageV1(
                    VintageOpV1::CounterCastSpellV1 { .. }
                )
            ))
        )
}

/// Queued printed triggers owned by this module's cards (Coveted Jewel,
/// Mana Vault's draw step).
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    match name {
        COVETED_JEWEL => *effect == coveted_jewel_changes_hands(),
        MANA_VAULT => *effect == mana_vault_draw_step(),
        _ => false,
    }
}

fn halt(state: &mut GameState, source: ObjectId) {
    state.engine.halted = Some((
        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
        source,
    ));
}

/// The resolving ability's source, if it is still the incarnation that
/// created the ability, on the battlefield.
fn live_ability_source(ctx: &ExecCtx, state: &GameState) -> Option<ObjectId> {
    let contract = ctx.ability_source_contract?;
    let live = state.objects.try_get(contract.source)?;
    (live.zone == Zone::Battlefield && live.zone_change_count == contract.zone_change_count)
        .then_some(contract.source)
}

fn vintage_state(state: &mut GameState) -> &mut VintageStateV1 {
    &mut state
        .eternal_v1
        .get_or_insert_with(Default::default)
        .vintage
}

pub(crate) fn execute(op: &VintageOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        VintageOpV1::FlipCoinLoseSourceDamagesControllerV1 { damage } => {
            match state.flip_coin_v1(ctx.controller) {
                Ok(true) => {}
                Ok(false) => event::propose_and_commit(
                    state,
                    ProposedEvent::damage(
                        ctx.source,
                        Target::Player(ctx.controller),
                        i32::from(*damage),
                    ),
                ),
                Err(_) => halt(state, ctx.source),
            }
        }
        VintageOpV1::CovetedJewelChangesHandsV1 => {
            let player = ctx.controller.opponent();
            crate::effect::execute(
                &EffectOp::DrawCards {
                    player: PlayerRef::Opponent,
                    count: 3,
                },
                ctx,
                state,
            );
            if let Some(jewel) = live_ability_source(ctx, state) {
                crate::standard_cards_v1::gain_control(state, jewel, player);
                crate::engine::attempt_untap(state, jewel);
            }
        }
        VintageOpV1::BindCounterCastSpellV1 => halt(state, ctx.source),
        VintageOpV1::CounterCastSpellV1 { spell } => {
            if crate::engine::counter_stack_item_by_id(state, *spell).is_err() {
                halt(state, ctx.source);
            }
        }
        VintageOpV1::UntapSourceV1 => {
            if let Some(source) = live_ability_source(ctx, state) {
                crate::engine::attempt_untap(state, source);
            }
        }
        VintageOpV1::SourceDamagesControllerIfTappedV1 { damage } => {
            if live_ability_source(ctx, state)
                .is_some_and(|source| state.objects.get(source).tapped)
            {
                event::propose_and_commit(
                    state,
                    ProposedEvent::damage(
                        ctx.source,
                        Target::Player(ctx.controller),
                        i32::from(*damage),
                    ),
                );
            }
        }
        VintageOpV1::ScheduleTurnSkipV1 => {
            let Some(vault) = live_ability_source(ctx, state) else {
                return;
            };
            let live = state.objects.get(vault);
            if !live.tapped || live.controller != ctx.controller {
                return;
            }
            let link = ObjectLinkV4 {
                object: vault,
                zone_change_count: live.zone_change_count,
            };
            let player = ctx.controller;
            let skips = &mut vintage_state(state).turn_skips;
            if !skips
                .iter()
                .any(|skip| skip.player == player && skip.vault == link)
            {
                skips.push(TurnSkipV1 {
                    player,
                    vault: link,
                });
            }
        }
        VintageOpV1::UntapAllCreaturesIfCompletedDungeonV1 => {
            let player = ctx.controller;
            if state.players[player.index()]
                .dungeon
                .completed_dungeons
                .is_empty()
            {
                return;
            }
            let creatures = state.players[player.index()]
                .battlefield
                .iter()
                .copied()
                .filter(|&object| {
                    let live = state.objects.get(object);
                    live.zone == Zone::Battlefield
                        && live.controller == player
                        && crate::engine::object_has_type(state, object, CardType::Creature)
                })
                .collect::<Vec<_>>();
            for creature in creatures {
                crate::engine::attempt_untap(state, creature);
            }
        }
        VintageOpV1::ProtectTargetFromCreaturesV1 => {
            let Some(Target::Object(object)) = ctx.targets.first().copied() else {
                return;
            };
            if !ctx.target_incarnation_matches(0, state)
                || state.objects.get(object).zone != Zone::Battlefield
            {
                return;
            }
            let link = ObjectLinkV4 {
                object,
                zone_change_count: state.objects.get(object).zone_change_count,
            };
            let turn = state.turn_key_v1();
            let protected = &mut vintage_state(state).protected_from_creatures;
            protected.retain(|(_, until)| *until == turn);
            if !protected.contains(&(link, turn)) {
                protected.push((link, turn));
            }
        }
        VintageOpV1::ParadoxicalOutcomeV1 => {
            let Some(spec) = state
                .objects
                .try_get(ctx.source)
                .and_then(|source| CARD_DEFS.get(source.card_def as usize))
                .map(|def| def.target_spec)
            else {
                return;
            };
            let returned = (0..ctx.targets.len())
                .filter(|&index| {
                    ctx.target_incarnation_matches(index, state)
                        && crate::engine::effect_target_is_legal(
                            state,
                            ctx.source,
                            ctx.controller,
                            spec,
                            &ctx.targets,
                            index,
                        )
                })
                .filter_map(|index| match ctx.targets[index] {
                    Target::Object(object) => Some(object),
                    Target::Player(_) | Target::StackItem(_) => None,
                })
                .collect::<Vec<_>>();
            if returned.is_empty() {
                return;
            }
            event::propose_and_commit_batch(
                state,
                returned
                    .iter()
                    .map(|&object| ProposedEvent::zone_change(object, Zone::Hand))
                    .collect(),
            );
            let count = returned
                .iter()
                .filter(|&&object| {
                    state
                        .objects
                        .try_get(object)
                        .is_some_and(|card| card.zone == Zone::Hand && card.owner == ctx.controller)
                })
                .count();
            if count > 0 {
                crate::effect::execute(
                    &EffectOp::DrawCards {
                        player: PlayerRef::Controller,
                        count: u32::try_from(count).unwrap_or(u32::MAX),
                    },
                    ctx,
                    state,
                );
            }
        }
    }
}

/// The player whose turn begins next, after the current one.
fn next_turn_player(state: &GameState) -> PlayerId {
    match state.extra_turn_v1.as_ref() {
        Some(turns) => turns
            .queued
            .last()
            .map_or(turns.next_regular_player, |turn| turn.item.controller),
        None => state.active_player.opponent(),
    }
}

fn cast_mana(state: &GameState, spell: ObjectId) -> Option<CastManaV1> {
    let live = state.objects.try_get(spell)?;
    state.eternal_v1.as_ref().and_then(|eternal| {
        eternal.vintage.cast_mana.iter().copied().find(|record| {
            record.spell.object == spell && record.spell.zone_change_count == live.zone_change_count
        })
    })
}

pub(crate) fn trigger_matches(
    condition: VintageTriggerV1,
    events: &[CommittedEvent],
    index: usize,
    source: ObjectId,
    state: &GameState,
) -> bool {
    let controller = state.objects.get(source).controller;
    match condition {
        VintageTriggerV1::SpellCastWithoutManaV1
        | VintageTriggerV1::SpellCastWithoutColoredManaV1 => {
            let CommittedEvent::SpellCast { spell, .. } = events[index] else {
                return false;
            };
            let record = cast_mana(state, spell);
            if condition == VintageTriggerV1::SpellCastWithoutManaV1 {
                record.is_none_or(|record| record.total == 0)
            } else {
                record.is_none_or(|record| !record.colored)
            }
        }
        VintageTriggerV1::OpponentUpkeepV1 => {
            matches!(events[index], CommittedEvent::UpkeepBegan { player } if player != controller)
        }
        VintageTriggerV1::EndStepBeforeYourTurnWhileTappedV1 => {
            matches!(events[index], CommittedEvent::BeginningEndStep { .. })
                && state.objects.get(source).tapped
                && next_turn_player(state) == controller
        }
    }
}

/// Records the mana spent to cast `spell` (before its cast triggers).
pub(crate) fn note_cast_mana(state: &mut GameState, spell: ObjectId, total: u16, colored: bool) {
    let link = ObjectLinkV4 {
        object: spell,
        zone_change_count: state.objects.get(spell).zone_change_count,
    };
    let on_stack = state
        .stack
        .iter()
        .map(|item| item.source)
        .collect::<Vec<_>>();
    let records = &mut vintage_state(state).cast_mana;
    records
        .retain(|record| record.spell.object != spell && on_stack.contains(&record.spell.object));
    records.push(CastManaV1 {
        spell: link,
        total,
        colored,
    });
}

/// Whether `object` is a front-face battlefield permanent named `name`
/// whose printed abilities apply.
fn active_named(state: &GameState, object: ObjectId, name: &str) -> bool {
    state.objects.try_get(object).is_some_and(|live| {
        live.zone == Zone::Battlefield
            && live.v4.face_index == 0
            && CARD_DEFS
                .get(live.card_def as usize)
                .is_some_and(|def| def.name == name && def.is_executable())
            && crate::continuous_characteristics_v1::printed_abilities_active(state, object)
    })
}

/// The battlefield permanents named `name` whose printed abilities apply,
/// optionally only those `controller` controls.
fn active_permanents<'a>(
    state: &'a GameState,
    name: &'a str,
    controller: Option<PlayerId>,
) -> impl Iterator<Item = ObjectId> + 'a {
    [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(move |player| state.players[player.index()].battlefield.iter().copied())
        .filter(move |&object| {
            active_named(state, object, name)
                && controller.is_none_or(|player| state.objects.get(object).controller == player)
        })
}

fn queue_trigger(state: &mut GameState, source: ObjectId, op: VintageOpV1) {
    let contract = AbilitySourceContractV4::capture(state, source);
    vintage_state(state).queued_triggers.push(QueuedTriggerV1 {
        source: contract,
        controller: contract.controller,
        op,
    });
}

/// Coveted Jewel: "Whenever one or more creatures an opponent controls
/// attack you and aren't blocked". Called once blockers are declared.
pub(crate) fn note_blockers_declared(state: &mut GameState) {
    let attacker_controller = state.active_player;
    let defender = attacker_controller.opponent();
    let unblocked_attack = state.engine.combat.attackers.iter().any(|&attacker| {
        state.objects.try_get(attacker).is_some_and(|live| {
            live.zone == Zone::Battlefield && live.controller == attacker_controller
        }) && crate::attack_target_v1::damage_recipient(state, attacker)
            == Some((Target::Player(defender), None))
            && !state
                .engine
                .combat
                .blocked_by
                .iter()
                .any(|(blocked, blockers)| *blocked == attacker && !blockers.is_empty())
    });
    if !unblocked_attack {
        return;
    }
    let jewels = active_permanents(state, COVETED_JEWEL, Some(defender)).collect::<Vec<_>>();
    for jewel in jewels {
        queue_trigger(state, jewel, VintageOpV1::CovetedJewelChangesHandsV1);
    }
}

/// Mana Vault: "At the beginning of your draw step, if this artifact is
/// tapped". Returns whether a trigger was queued.
pub(crate) fn begin_draw_step(state: &mut GameState, player: PlayerId) -> bool {
    let vaults = active_permanents(state, MANA_VAULT, Some(player))
        .filter(|&vault| state.objects.get(vault).tapped)
        .collect::<Vec<_>>();
    for &vault in &vaults {
        queue_trigger(
            state,
            vault,
            VintageOpV1::SourceDamagesControllerIfTappedV1 { damage: 1 },
        );
    }
    !vaults.is_empty()
}

/// The queued triggers this collection puts on the stack.
pub(crate) fn delayed_triggers(state: &mut GameState) -> Vec<PendingTrigger> {
    let Some(eternal) = state.eternal_v1.as_mut() else {
        return Vec::new();
    };
    std::mem::take(&mut eternal.vintage.queued_triggers)
        .into_iter()
        .map(|queued| {
            crate::eternal_cards_v1::delayed_trigger(
                queued.source,
                queued.controller,
                vintage(queued.op),
            )
        })
        .collect()
}

/// Mana Vault and Time Vault: "This artifact doesn't untap during your
/// untap step."
pub(crate) fn prevents_untap_step(state: &GameState, object: ObjectId) -> bool {
    active_named(state, object, MANA_VAULT) || active_named(state, object, TIME_VAULT)
}

/// Applies a Time Vault skip to the turn about to begin for the now-active
/// player: true when that turn is skipped (and the Vault untapped).
pub(crate) fn take_turn_skip(state: &mut GameState) -> bool {
    let player = state.active_player;
    let Some(eternal) = state.eternal_v1.as_mut() else {
        return false;
    };
    if eternal.vintage.turn_skips.is_empty() {
        return false;
    }
    let (mine, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut eternal.vintage.turn_skips)
        .into_iter()
        .partition(|skip| skip.player == player);
    eternal.vintage.turn_skips = rest;
    let Some(vault) = mine.into_iter().find_map(|skip| {
        state
            .objects
            .try_get(skip.vault.object)
            .is_some_and(|live| {
                live.zone == Zone::Battlefield
                    && live.zone_change_count == skip.vault.zone_change_count
                    && live.controller == player
                    && live.tapped
            })
            .then_some(skip.vault.object)
    }) else {
        return false;
    };
    crate::engine::attempt_untap(state, vault);
    true
}

/// Trinisphere: "As long as this artifact is untapped, each spell that
/// would cost less than three mana to cast costs three mana to cast." The
/// generic increase for a spell whose total cost has `pips` mana symbols
/// and `generic` generic mana.
pub(crate) fn trinisphere_increase(state: &GameState, pips: usize, generic: u32) -> u32 {
    let active = active_permanents(state, TRINISPHERE, None)
        .any(|trinisphere| !state.objects.get(trinisphere).tapped);
    if !active {
        return 0;
    }
    let total = u32::try_from(pips)
        .unwrap_or(u32::MAX)
        .saturating_add(generic);
    3_u32.saturating_sub(total)
}

/// Archon of Emeria: "Each player can't cast more than one spell each turn."
pub(crate) fn cast_prohibited(state: &GameState, player: PlayerId) -> bool {
    state.players[player.index()].spells_cast_this_turn >= 1
        && active_permanents(state, ARCHON_OF_EMERIA, None)
            .next()
            .is_some()
}

/// Archon of Emeria: "Nonbasic lands your opponents control enter tapped."
pub(crate) fn enters_tapped(state: &GameState, object: ObjectId, controller: PlayerId) -> bool {
    let Some(def) = state
        .objects
        .try_get(object)
        .and_then(|live| CARD_DEFS.get(live.card_def as usize))
    else {
        return false;
    };
    let face = state.objects.get(object).v4.face_index;
    def.types_for_face(face).contains(&CardType::Land)
        && !(face == 0 && def.supertypes.contains(&Supertype::Basic))
        && active_permanents(state, ARCHON_OF_EMERIA, Some(controller.opponent()))
            .next()
            .is_some()
}

/// Clarion Conqueror: "Activated abilities of artifacts, creatures, and
/// planeswalkers can't be activated." Karn, the Great Creator: "Activated
/// abilities of artifacts your opponents control can't be activated."
pub(crate) fn activations_prohibited(state: &GameState, object: ObjectId) -> bool {
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    if live.zone != Zone::Battlefield {
        return false;
    }
    let has = |card_type| crate::engine::object_has_type(state, object, card_type);
    let artifact = has(CardType::Artifact);
    if (artifact || has(CardType::Creature) || has(CardType::Planeswalker))
        && active_permanents(state, CLARION_CONQUEROR, None)
            .next()
            .is_some()
    {
        return true;
    }
    artifact
        && active_permanents(state, KARN, Some(live.controller.opponent()))
            .next()
            .is_some()
}

/// Whether `target` has protection from creatures (Seasoned Dungeoneer).
pub(crate) fn protected_from_creatures(state: &GameState, target: ObjectId) -> bool {
    let Some(live) = state.objects.try_get(target) else {
        return false;
    };
    let turn = state.turn_key_v1();
    live.zone == Zone::Battlefield
        && state.eternal_v1.as_ref().is_some_and(|eternal| {
            eternal
                .vintage
                .protected_from_creatures
                .iter()
                .any(|(link, until)| {
                    *until == turn
                        && link.object == target
                        && link.zone_change_count == live.zone_change_count
                })
        })
}

/// Whether a source object is a creature for protection: a creature
/// permanent, or a creature spell.
pub(crate) fn source_is_creature(state: &GameState, source: ObjectId) -> bool {
    state
        .objects
        .try_get(source)
        .is_some_and(|live| match live.zone {
            Zone::Battlefield => crate::engine::object_has_type(state, source, CardType::Creature),
            Zone::Stack => CARD_DEFS
                .get(live.card_def as usize)
                .is_some_and(|def| def.has_type(CardType::Creature)),
            _ => false,
        })
}

/// Protection from creatures prevents damage from creature sources.
pub(crate) fn damage_prevented(state: &GameState, source: ObjectId, target: Target) -> bool {
    matches!(target, Target::Object(object) if protected_from_creatures(state, object))
        && source_is_creature(state, source)
}

/// Protection from creatures: it can't be blocked by creatures.
pub(crate) fn blocker_allowed(state: &GameState, attacker: ObjectId, blocker: ObjectId) -> bool {
    !(protected_from_creatures(state, attacker)
        && crate::engine::object_has_type(state, blocker, CardType::Creature))
}

/// Mishra's Workshop: "Spend this mana only to cast artifact spells." The
/// three mana its ability just added become restricted units.
pub(crate) fn restrict_produced_mana(
    state: &mut GameState,
    player: PlayerId,
    source: ObjectId,
    color: ManaColor,
) {
    let live = state.objects.get(source);
    if CARD_DEFS[live.card_def as usize].name != MISHRAS_WORKSHOP {
        return;
    }
    let unit = crate::mana::RestrictedManaUnitV1 {
        color,
        restriction: crate::card_def::ManaSpendRestrictionDef::ArtifactSpellV1,
        source: ObjectLinkV4 {
            object: source,
            zone_change_count: live.zone_change_count,
        },
        source_card_def: live.card_def,
    };
    let seat = &mut state.players[player.index()];
    let produced = seat.mana_pool[color.pool_index()].min(3);
    seat.mana_pool[color.pool_index()] -= produced;
    for _ in 0..produced {
        seat.restricted_mana_pool.0.push(unit.clone());
    }
}

/// Tinker: "Search your library for an artifact card, put that card onto
/// the battlefield, then shuffle." Its sacrifice is the card's additional
/// cost.
pub fn tinker() -> EffectOp {
    EffectOp::SearchLibraryCardsToDestination {
        player: PlayerRef::Controller,
        filter: crate::effect::LibraryCardFilter::ArtifactV1,
        max_targets: 1,
        destination: crate::effect::LibrarySearchDestinationV1::Battlefield { tapped: false },
    }
}

/// Paradoxical Outcome: "Return any number of target nonland, nontoken
/// permanents you control to their owners' hands. Draw a card for each card
/// returned to your hand this way."
pub fn paradoxical_outcome() -> EffectOp {
    vintage(VintageOpV1::ParadoxicalOutcomeV1)
}
