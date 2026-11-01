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
    /// Noncreature artifacts animated by Karn, the Great Creator's +1, with
    /// the player whose next turn ends the effect.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub karn_animated: Vec<(ObjectLinkV4, PlayerId)>,
    /// Artifacts a Tezzeret, Cruel Captain emblem made 0/0 Robot artifact
    /// creatures.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub robots: Vec<ObjectLinkV4>,
    /// Tezzeret, Cruel Captain emblems: their owner and the planeswalker
    /// whose ability created each.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tezzeret_emblems: Vec<(PlayerId, AbilitySourceContractV4)>,
    /// Urza's Sagas and the highest chapter ability each has gained.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub saga_abilities: Vec<(ObjectLinkV4, u8)>,
    /// Permanents that entered as a copy (Phyrexian Metamorph), with their
    /// own definition and name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub copies: Vec<CopyV1>,
    /// Anointed Peacekeepers and the definition whose name each chose.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chosen_names: Vec<(ObjectLinkV4, u16)>,
    /// Chancellors of the Annex revealed from an opening hand, waiting for
    /// the revealing player's opponent to cast their first spell.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chancellor_reveals: Vec<AbilitySourceContractV4>,
    /// Whether opening-hand reveals were offered this game.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub opening_hand_offered: bool,
    /// Whether each player has cast a spell this game, tracked once
    /// opening-hand reveals were offered.
    #[serde(default, skip_serializing_if = "no_spells_cast")]
    pub cast_a_spell: [bool; 2],
}

fn no_spells_cast(cast: &[bool; 2]) -> bool {
    !cast[0] && !cast[1]
}

/// A permanent that is a copy of another definition until it leaves the
/// battlefield (Phyrexian Metamorph), with the values it had before.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CopyV1 {
    pub host: ObjectLinkV4,
    pub card_def: u16,
    pub name: String,
    pub color_mask: u8,
    pub subtype_ids: Vec<u16>,
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
    /// "Whenever an artifact you control enters" (Tezzeret, Cruel Captain).
    ControlledArtifactEntersV1,
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
    /// Karn, the Great Creator's +1: target 0, if any, becomes an artifact
    /// creature with power and toughness each equal to its mana value until
    /// the controller's next turn.
    KarnAnimateTargetV1,
    /// Tezzeret, Cruel Captain's 0: untap target 0; if it's an artifact
    /// creature, put a +1/+1 counter on it.
    TezzeretUntapTargetV1,
    /// Tezzeret's -7: the controller gets the emblem.
    TezzeretEmblemV1,
    /// The emblem's ability: three +1/+1 counters on target 0; if it's not a
    /// creature, it becomes a 0/0 Robot artifact creature.
    TezzeretEmblemCountersV1,
    /// An Urza's Saga chapter: the Saga gains its chapter `level` ability.
    SagaGainsAbilityV1 { level: u8 },
    /// The resolving permanent spell enters the battlefield as itself, if
    /// it is still on the stack (Phyrexian Metamorph copying nothing).
    EnterIfStillOnStackV1,
    /// Anointed Peacekeeper: the controller looks at the opponent's hand.
    LookAtOpponentHandV1,
    /// Anointed Peacekeeper enters with the name of definition `named`
    /// chosen.
    EnterNamingV1 { named: u16 },
    /// Chancellor of the Annex is revealed from its owner's opening hand.
    RevealFromOpeningHandV1,
    /// Trigger template: "counter it unless that player pays {1}" for the
    /// spell whose cast triggered the ability.
    BindCounterUnlessPaysOneV1,
    /// The spell's controller pays {1} if able; otherwise the spell is
    /// countered.
    PayOneOrCounterV1 { spell: StackItemId },
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
const TEZZERET: &str = "Tezzeret, Cruel Captain";
const URZAS_SAGA: &str = "Urza's Saga";
const WITCH_ENCHANTER: &str = "Witch Enchanter";
const ANOINTED_PEACEKEEPER: &str = "Anointed Peacekeeper";
const CHANCELLOR: &str = "Chancellor of the Annex";

/// Cards whose "When this creature enters, you take the initiative" makes
/// them the Initiative designation's source (Avenging Hunter in Pauper).
pub(crate) fn initiative_granter(name: &str) -> bool {
    name == "Avenging Hunter"
        || (cfg!(feature = "eternal-fixtures")
            && (matches!(name, SEASONED_DUNGEONEER | WHITE_PLUME_ADVENTURER)
                || crate::eternal_pauper_e8b_v1::initiative_granter(name)
                || crate::eternal_legacy_e9a_v1::initiative_granter(name)))
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

fn add_loyalty() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::AddLoyaltyToSource { amount: 1 })
}

fn chancellor_tax() -> EffectOp {
    vintage(VintageOpV1::BindCounterUnlessPaysOneV1)
}

/// Chancellor of the Annex's opening-hand reveal, offered as the game's
/// first upkeep begins.
fn chancellor_reveal_choice() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: vec![
            vintage(VintageOpV1::RevealFromOpeningHandV1),
            EffectOp::Sequence(Vec::new()),
        ],
    }
}

/// "Counter that spell unless that player pays {1}": the spell's
/// controller chooses whether to pay.
fn counter_unless_pays_one(spell: StackItemId) -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Opponent,
        options: vec![
            vintage(VintageOpV1::PayOneOrCounterV1 { spell }),
            vintage(VintageOpV1::CounterCastSpellV1 { spell }),
        ],
    }
}

/// Witch Enchanter: "When this creature enters, destroy target artifact or
/// enchantment an opponent controls."
fn witch_enchanter_etb() -> EffectOp {
    EffectOp::DestroyObject {
        object: crate::effect::ObjectRef::Target(0),
    }
}

fn tezzeret_emblem_counters() -> EffectOp {
    vintage(VintageOpV1::TezzeretEmblemCountersV1)
}

const TEZZERET_TRIGGERS: [TriggeredAbilityDef; 1] = [vintage_trigger(
    VintageTriggerV1::ControlledArtifactEntersV1,
    add_loyalty,
)];
const CHANCELLOR_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::OpponentCastsSpellV1,
    chancellor_tax,
)];
const WITCH_ENCHANTER_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, witch_enchanter_etb)];

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
        TEZZERET => &TEZZERET_TRIGGERS,
        CHANCELLOR => &CHANCELLOR_TRIGGERS,
        WITCH_ENCHANTER => &WITCH_ENCHANTER_TRIGGERS,
        _ => crate::eternal_lurrus_v1::triggers_for(name),
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
    // `triggers_for` falls through to E4b's cards, so they answer first.
    if let Some(spec) = crate::eternal_lurrus_v1::trigger_target_spec(name, effect) {
        return Some(spec);
    }
    if !own_trigger_effect(name, effect) {
        return None;
    }
    Some(
        if name == SEASONED_DUNGEONEER && *effect == dungeoneer_attack() {
            TargetSpec::StandardV1(StandardTargetV1::AttackingClericRogueWarriorOrWizardV1)
        } else if name == WITCH_ENCHANTER && *effect == witch_enchanter_etb() {
            TargetSpec::OpponentArtifactOrEnchantmentPermanent
        } else if name == TEZZERET && *effect == tezzeret_emblem_counters() {
            TargetSpec::StandardV1(StandardTargetV1::ControlledArtifact)
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
    let chancellor = *effect == chancellor_tax();
    if *effect != counter_cast_spell() && !chancellor {
        return None;
    }
    let CommittedEvent::SpellCast { spell, .. } = event else {
        return None;
    };
    let spell = cast_stack_item(state, *spell)?;
    Some(if chancellor {
        counter_unless_pays_one(spell)
    } else {
        vintage(VintageOpV1::CounterCastSpellV1 { spell })
    })
}

/// The stack incarnation of the spell `source` just cast.
fn cast_stack_item(state: &GameState, source: ObjectId) -> Option<StackItemId> {
    state
        .stack
        .iter()
        .find(|item| item.source == source && item.kind == crate::state::StackItemKind::Spell)
        .map(|item| item.v4.stack_item_id)
}

/// Whether `effect` is Chancellor of the Annex's counter-unless-pays choice
/// bound to some spell.
fn is_counter_unless_pays_one(effect: &EffectOp) -> bool {
    let EffectOp::Choice {
        controller: PlayerRef::Opponent,
        options,
    } = effect
    else {
        return false;
    };
    match options.as_slice() {
        [EffectOp::StandardV1(StandardOpV1::EternalV1(
            crate::eternal_cards_v1::EternalOpV1::VintageV1(VintageOpV1::PayOneOrCounterV1 {
                spell,
            }),
        )), second] => *second == vintage(VintageOpV1::CounterCastSpellV1 { spell: *spell }),
        _ => false,
    }
}

/// Whether `effect` is a trigger-time materialization of `template`.
pub(crate) fn template_matches(template: &EffectOp, effect: &EffectOp) -> bool {
    if *template == chancellor_tax() {
        return is_counter_unless_pays_one(effect);
    }
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
        TEZZERET => *effect == tezzeret_emblem_counters(),
        CHANCELLOR => *effect == chancellor_reveal_choice() || is_counter_unless_pays_one(effect),
        _ => crate::eternal_lurrus_v1::delayed_effect_matches(name, effect),
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
        VintageOpV1::KarnAnimateTargetV1 => {
            let Some(object) = legal_object_target(ctx, state) else {
                return;
            };
            let link = live_link(state, object);
            let controller = ctx.controller;
            let animated = &mut vintage_state(state).karn_animated;
            if !animated.contains(&(link, controller)) {
                animated.push((link, controller));
            }
        }
        VintageOpV1::TezzeretUntapTargetV1 => {
            let Some(object) = legal_object_target(ctx, state) else {
                return;
            };
            crate::engine::attempt_untap(state, object);
            if crate::engine::object_has_type(state, object, CardType::Artifact)
                && crate::engine::object_has_type(state, object, CardType::Creature)
                && event::add_plus_one_counters(state, object, ctx.controller, 1).is_err()
            {
                halt(state, ctx.source);
            }
        }
        VintageOpV1::TezzeretEmblemV1 => {
            let Some(source) = ctx.ability_source_contract else {
                halt(state, ctx.source);
                return;
            };
            let controller = ctx.controller;
            vintage_state(state)
                .tezzeret_emblems
                .push((controller, source));
        }
        VintageOpV1::TezzeretEmblemCountersV1 => {
            let Some(object) = legal_object_target(ctx, state) else {
                return;
            };
            if event::add_plus_one_counters(state, object, ctx.controller, 3).is_err() {
                halt(state, ctx.source);
                return;
            }
            if !crate::engine::object_has_type(state, object, CardType::Creature) {
                let link = live_link(state, object);
                let robot = crate::card_def::Subtype::Robot.stable_id();
                let ids = &mut state.objects.get_mut(object).v4.effective_subtype_ids;
                if let Err(position) = ids.binary_search(&robot) {
                    ids.insert(position, robot);
                }
                vintage_state(state).robots.push(link);
            }
        }
        VintageOpV1::SagaGainsAbilityV1 { level } => {
            let Some(saga) = live_ability_source(ctx, state) else {
                return;
            };
            let link = live_link(state, saga);
            let abilities = &mut vintage_state(state).saga_abilities;
            match abilities.iter_mut().find(|(host, _)| *host == link) {
                Some((_, gained)) => *gained = (*gained).max(*level),
                None => abilities.push((link, *level)),
            }
        }
        VintageOpV1::EnterIfStillOnStackV1 => enter_battlefield(ctx, state),
        VintageOpV1::LookAtOpponentHandV1 => {
            let viewer = ctx.controller;
            let opponent = viewer.opponent();
            for card in state.players[opponent.index()].hand.clone() {
                if state.reveal_hand_card(viewer, opponent, card).is_err() {
                    halt(state, ctx.source);
                    return;
                }
            }
        }
        VintageOpV1::EnterNamingV1 { named } => {
            enter_battlefield(ctx, state);
            let Some(live) = state.objects.try_get(ctx.source) else {
                return;
            };
            if live.zone != Zone::Battlefield {
                return;
            }
            let link = live_link(state, ctx.source);
            vintage_state(state).chosen_names.push((link, *named));
        }
        VintageOpV1::RevealFromOpeningHandV1 => {
            let Some(contract) = ctx.ability_source_contract else {
                halt(state, ctx.source);
                return;
            };
            let in_hand = state.objects.try_get(contract.source).is_some_and(|live| {
                live.zone == Zone::Hand && live.zone_change_count == contract.zone_change_count
            });
            if !in_hand {
                return;
            }
            for observer in [PlayerId::P0, PlayerId::P1] {
                if state
                    .reveal_hand_card(observer, contract.owner, contract.source)
                    .is_err()
                {
                    halt(state, ctx.source);
                    return;
                }
            }
            vintage_state(state).chancellor_reveals.push(contract);
        }
        VintageOpV1::BindCounterUnlessPaysOneV1 => halt(state, ctx.source),
        VintageOpV1::PayOneOrCounterV1 { spell } => {
            let Some(player) = state
                .stack
                .iter()
                .find(|item| item.v4.stack_item_id == *spell)
                .map(|item| item.controller)
            else {
                return;
            };
            let one = crate::mana::Cost {
                pips: &[],
                generic: 1,
                x_count: 0,
            };
            match crate::mana::can_pay(&one, 0, player, state) {
                Some(plan) => {
                    crate::engine::pay_plan(state, player, &plan);
                }
                None => {
                    if crate::engine::counter_stack_item_by_id(state, *spell).is_err() {
                        halt(state, ctx.source);
                    }
                }
            }
        }
    }
}

/// Target 0, if it is an object still legal for the resolving ability.
fn legal_object_target(ctx: &ExecCtx, state: &GameState) -> Option<ObjectId> {
    let Some(Target::Object(object)) = ctx.targets.first().copied() else {
        return None;
    };
    (ctx.target_incarnation_matches(0, state)
        && state.objects.get(object).zone == Zone::Battlefield)
        .then_some(object)
}

fn live_link(state: &GameState, object: ObjectId) -> ObjectLinkV4 {
    ObjectLinkV4 {
        object,
        zone_change_count: state.objects.get(object).zone_change_count,
    }
}

/// The resolving permanent spell enters the battlefield, if it is still on
/// the stack.
fn enter_battlefield(ctx: &ExecCtx, state: &mut GameState) {
    if state
        .objects
        .try_get(ctx.source)
        .is_some_and(|live| live.zone == Zone::Stack)
    {
        crate::effect::execute(
            &EffectOp::MoveObject {
                object: crate::effect::ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            },
            ctx,
            state,
        );
    }
}

/// Phyrexian Metamorph: "You may have this creature enter as a copy of any
/// artifact or creature on the battlefield, except it's an artifact in
/// addition to its other types." The resolving spell enters, then takes
/// the copied definition, name, colors and subtypes until it leaves.
pub(crate) fn enter_as_copy(
    state: &mut GameState,
    ctx: &ExecCtx,
    copied: crate::effect::EffectObjectBinding,
) {
    let Some(original) = state.objects.try_get(copied.object) else {
        return;
    };
    if original.zone != copied.expected_zone
        || original.zone_change_count != copied.expected_zone_change_count
    {
        return;
    }
    let copied_def = original.card_def;
    enter_battlefield(ctx, state);
    let host = ctx.source;
    if state.objects.get(host).zone != Zone::Battlefield {
        return;
    }
    let link = live_link(state, host);
    let live = state.objects.get_mut(host);
    let record = CopyV1 {
        host: link,
        card_def: live.card_def,
        name: std::mem::take(&mut live.name),
        color_mask: live.v4.effective_color_mask,
        subtype_ids: std::mem::take(&mut live.v4.effective_subtype_ids),
    };
    let base = crate::state::ObjectStateV4::from_card_def(copied_def);
    live.card_def = copied_def;
    live.name = CARD_DEFS[copied_def as usize].object_name.into();
    live.v4.effective_color_mask = base.effective_color_mask;
    live.v4.effective_subtype_ids = base.effective_subtype_ids;
    let own_def = record.card_def;
    vintage_state(state).copies.push(record);
    crate::standard_cards_v1::note_copied_definitions(
        state,
        host,
        link.zone_change_count,
        &[own_def, copied_def],
    );
    // A copy of a permanent that enters tapped enters tapped.
    if CARD_DEFS[copied_def as usize].enters_battlefield_tapped {
        state.objects.get_mut(host).tapped = true;
    }
}

fn active_copy_index(state: &GameState, object: ObjectId) -> Option<usize> {
    let live = state.objects.try_get(object)?;
    let link = ObjectLinkV4 {
        object,
        zone_change_count: live.zone_change_count,
    };
    state
        .eternal_v1
        .as_ref()?
        .vintage
        .copies
        .iter()
        .position(|copy| copy.host == link)
}

/// Whether `object` is a battlefield permanent that entered as a copy.
pub(crate) fn active_copy(state: &GameState, object: ObjectId) -> bool {
    active_copy_index(state, object).is_some()
}

/// A copy leaving the battlefield stops being a copy first, so it arrives
/// in its new zone as its own card.
pub(crate) fn end_copy_before_departure(state: &mut GameState, object: ObjectId) {
    let Some(index) = active_copy_index(state, object) else {
        return;
    };
    let record = vintage_state(state).copies.remove(index);
    let live = state.objects.get_mut(object);
    live.card_def = record.card_def;
    live.name = record.name;
    live.v4.effective_color_mask = record.color_mask;
    live.v4.effective_subtype_ids = record.subtype_ids;
}

/// Card-type changes from this module: Karn's and the Tezzeret emblem's
/// animated artifacts are creatures, and copies and Robots are artifacts.
pub(crate) fn has_type_override(
    state: &GameState,
    object: ObjectId,
    card_type: CardType,
) -> Option<bool> {
    // Oko's Elks and The Fantasticar (wave E5a).
    if let Some(answer) =
        crate::eternal_e5a_v1::statics::has_type_override(state, object, card_type)
    {
        return Some(answer);
    }
    // Lion Sash isn't a creature while attached (wave E8a).
    if let Some(answer) =
        crate::eternal_legacy_e8a_v1::statics::has_type_override(state, object, card_type)
    {
        return Some(answer);
    }
    let eternal = state.eternal_v1.as_ref()?;
    let vintage = &eternal.vintage;
    if vintage.karn_animated.is_empty() && vintage.robots.is_empty() && vintage.copies.is_empty() {
        return None;
    }
    let live = state.objects.try_get(object)?;
    if live.zone != Zone::Battlefield {
        return None;
    }
    let link = ObjectLinkV4 {
        object,
        zone_change_count: live.zone_change_count,
    };
    let animated = vintage.karn_animated.iter().any(|(host, _)| *host == link)
        || vintage.robots.contains(&link);
    match card_type {
        CardType::Creature if animated => Some(true),
        CardType::Artifact if animated || vintage.copies.iter().any(|copy| copy.host == link) => {
            Some(true)
        }
        _ => None,
    }
}

/// Base power and toughness from this module: Karn's animated artifacts
/// have their mana value, the Tezzeret emblem's Robots are 0/0.
pub(crate) fn characteristic_defining_power(state: &GameState, object: ObjectId) -> Option<i32> {
    let vintage = &state.eternal_v1.as_ref()?.vintage;
    let live = state.objects.try_get(object)?;
    if live.zone != Zone::Battlefield {
        return None;
    }
    let link = ObjectLinkV4 {
        object,
        zone_change_count: live.zone_change_count,
    };
    if vintage.karn_animated.iter().any(|(host, _)| *host == link) {
        return Some(i32::from(crate::engine::object_mana_value(state, object)));
    }
    vintage.robots.contains(&link).then_some(0)
}

/// The beginning of `player`'s turn ends Karn's animations they created.
pub(crate) fn begin_turn(state: &mut GameState, player: PlayerId) {
    if let Some(eternal) = state.eternal_v1.as_mut() {
        eternal
            .vintage
            .karn_animated
            .retain(|(_, controller)| *controller != player);
    }
}

/// The highest chapter ability an Urza's Saga gained, or 0.
fn saga_level(state: &GameState, saga: ObjectId) -> u8 {
    let Some(live) = state.objects.try_get(saga) else {
        return 0;
    };
    let link = ObjectLinkV4 {
        object: saga,
        zone_change_count: live.zone_change_count,
    };
    state.eternal_v1.as_ref().map_or(0, |eternal| {
        eternal
            .vintage
            .saga_abilities
            .iter()
            .find(|(host, _)| *host == link)
            .map_or(0, |(_, level)| *level)
    })
}

/// Mana abilities this module gates: Urza's Saga has "{T}: Add {C}" once
/// chapter I gave it, Witch Enchanter's {W} belongs to its land face.
pub(crate) fn mana_abilities_active(
    state: &GameState,
    object: ObjectId,
    name: &str,
) -> Option<bool> {
    match name {
        URZAS_SAGA => Some(saga_level(state, object) >= 1),
        WITCH_ENCHANTER => Some(state.objects.get(object).v4.face_index == 1),
        _ => crate::eternal_lurrus_v1::mana_abilities_active(state, object, name),
    }
}

/// Activated abilities this module gates: Urza's Saga's Construct ability
/// exists once chapter II gave it.
pub(crate) fn activation_allowed(
    state: &GameState,
    source: ObjectId,
    name: &str,
    ability_index: usize,
) -> bool {
    (name != URZAS_SAGA || saga_level(state, source) >= 2)
        && crate::eternal_lurrus_v1::activation_allowed(state, source)
        && crate::eternal_legacy_e5b_v1::activation_allowed(state, source)
        && crate::eternal_pauper_e6a_v1::activation_allowed(state, source, name, ability_index)
        && crate::eternal_e5a_v1::statics::activation_allowed(state, source, name)
        && crate::eternal_vintage_e7b_v1::statics::activation_allowed(state, source, name)
        && crate::eternal_pauper_e8b_v1::activation_allowed(state, name, ability_index)
        && crate::eternal_legacy_e8a_v1::statics::activation_allowed(
            state,
            source,
            name,
            ability_index,
        )
        && crate::eternal_legacy_e9a_v1::statics::activation_allowed(
            state,
            source,
            name,
            ability_index,
        )
}

/// Anointed Peacekeeper: definitions named by a Peacekeeper whose printed
/// abilities apply, with that Peacekeeper's controller.
fn chosen_names(state: &GameState) -> Vec<(u16, PlayerId)> {
    let Some(eternal) = state.eternal_v1.as_ref() else {
        return Vec::new();
    };
    eternal
        .vintage
        .chosen_names
        .iter()
        .filter(|(host, _)| {
            state.objects.try_get(host.object).is_some_and(|live| {
                live.zone_change_count == host.zone_change_count
                    && active_named(state, host.object, ANOINTED_PEACEKEEPER)
            })
        })
        .map(|(host, named)| (*named, state.objects.get(host.object).controller))
        .collect()
}

/// "Spells your opponents cast with the chosen name cost {2} more to cast."
/// The generic increase for `player` casting a spell named `name`.
pub(crate) fn spell_tax(state: &GameState, name: &str, player: PlayerId) -> u32 {
    chosen_names(state)
        .into_iter()
        .filter(|(named, controller)| {
            *controller != player && CARD_DEFS[usize::from(*named)].object_name == name
        })
        .map(|_| 2)
        .sum()
}

/// "Activated abilities of sources with the chosen name cost {2} more to
/// activate unless they're mana abilities."
pub(crate) fn activation_tax(state: &GameState, source: ObjectId) -> u8 {
    let names = chosen_names(state);
    if names.is_empty() {
        return 0;
    }
    let source_name = crate::engine::effective_name(state, source);
    let count = names
        .into_iter()
        .filter(|(named, _)| CARD_DEFS[usize::from(*named)].object_name == source_name)
        .count();
    u8::try_from(count.saturating_mul(2)).unwrap_or(u8::MAX)
}

/// Every name a player may choose: each executable nontoken definition's
/// name once, grouped by first letter for a two-step choice.
pub fn nameable_definitions() -> &'static [Vec<u16>] {
    static TABLE: std::sync::OnceLock<Vec<Vec<u16>>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let mut seen: Vec<&str> = Vec::new();
        let mut groups: Vec<(char, Vec<u16>)> = Vec::new();
        for (index, def) in CARD_DEFS.iter().enumerate() {
            if def.is_token || !def.is_executable() || seen.contains(&def.object_name) {
                continue;
            }
            let Ok(id) = u16::try_from(index) else {
                continue;
            };
            seen.push(def.object_name);
            let letter = def
                .object_name
                .chars()
                .next()
                .map_or(' ', |letter| letter.to_ascii_uppercase());
            match groups.iter_mut().find(|(group, _)| *group == letter) {
                Some((_, ids)) => ids.push(id),
                None => groups.push((letter, vec![id])),
            }
        }
        groups.sort_by_key(|(letter, _)| *letter);
        groups.into_iter().map(|(_, ids)| ids).collect()
    })
}

/// Anointed Peacekeeper: "As this creature enters, look at an opponent's
/// hand, then choose any card name." The name is chosen by first letter,
/// then from the names with that letter.
pub fn anointed_peacekeeper() -> EffectOp {
    EffectOp::Sequence(vec![
        vintage(VintageOpV1::LookAtOpponentHandV1),
        EffectOp::Choice {
            controller: PlayerRef::Controller,
            options: nameable_definitions()
                .iter()
                .map(|ids| EffectOp::Choice {
                    controller: PlayerRef::Controller,
                    options: ids
                        .iter()
                        .map(|&named| vintage(VintageOpV1::EnterNamingV1 { named }))
                        .collect(),
                })
                .collect(),
        },
    ])
}

/// Phyrexian Metamorph's resolution: choose up to one artifact or creature
/// on the battlefield to copy, then enter.
pub fn phyrexian_metamorph() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::SelectObjectsV1 {
            rule: crate::effect::ObjectSelectionRuleV1 {
                player: PlayerRef::Controller,
                zone: Zone::Battlefield,
                any_player: true,
                filter: crate::effect::ObjectSelectionFilterV1::ArtifactOrCreatureV1,
                min: 0,
                max: 1,
                action: crate::effect::ObjectSelectionActionV1::EnterAsCopyV1,
            },
        },
        vintage(VintageOpV1::EnterIfStillOnStackV1),
    ])
}

/// Karn, the Great Creator's +1.
pub fn karn_plus_one() -> EffectOp {
    vintage(VintageOpV1::KarnAnimateTargetV1)
}

/// Karn, the Great Creator's -2: "You may reveal an artifact card you own
/// from outside the game or choose a face-up artifact card you own in
/// exile. Put that card into your hand." Best-of-one mainboard games have
/// nothing outside the game, so only exile is searched.
pub fn karn_minus_two() -> EffectOp {
    EffectOp::SelectObjectsV1 {
        rule: crate::effect::ObjectSelectionRuleV1 {
            player: PlayerRef::Controller,
            zone: Zone::Exile,
            any_player: false,
            filter: crate::effect::ObjectSelectionFilterV1::OwnedFaceUpArtifactCardV1,
            min: 0,
            max: 1,
            action: crate::effect::ObjectSelectionActionV1::MoveTo(Zone::Hand),
        },
    }
}

/// Tezzeret, Cruel Captain's 0.
pub fn tezzeret_zero() -> EffectOp {
    vintage(VintageOpV1::TezzeretUntapTargetV1)
}

/// Tezzeret, Cruel Captain's -3: "Search your library for an artifact card
/// with mana value 1 or less, reveal it, put it into your hand, then
/// shuffle."
pub fn tezzeret_minus_three() -> EffectOp {
    EffectOp::SearchLibraryToHand {
        player: PlayerRef::Controller,
        filter: crate::effect::LibraryCardFilter::ArtifactManaValueAtMostV1(1),
    }
}

/// Tezzeret, Cruel Captain's -7.
pub fn tezzeret_minus_seven() -> EffectOp {
    vintage(VintageOpV1::TezzeretEmblemV1)
}

/// Urza's Saga, chapter I: 'This Saga gains "{T}: Add {C}."'
pub fn urzas_saga_one() -> EffectOp {
    vintage(VintageOpV1::SagaGainsAbilityV1 { level: 1 })
}

/// Urza's Saga, chapter II: the Saga gains its Construct ability.
pub fn urzas_saga_two() -> EffectOp {
    vintage(VintageOpV1::SagaGainsAbilityV1 { level: 2 })
}

/// Urza's Saga, chapter III: "Search your library for an artifact card with
/// mana cost {0} or {1}, put it onto the battlefield, then shuffle."
pub fn urzas_saga_three() -> EffectOp {
    EffectOp::SearchLibraryCardsToDestination {
        player: PlayerRef::Controller,
        filter: crate::effect::LibraryCardFilter::ArtifactManaCostZeroOrOneV1,
        max_targets: 1,
        destination: crate::effect::LibrarySearchDestinationV1::Battlefield { tapped: false },
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
        VintageTriggerV1::ControlledArtifactEntersV1 => {
            let entered = match events[index] {
                CommittedEvent::ZoneChange {
                    object,
                    to: Zone::Battlefield,
                    ..
                }
                | CommittedEvent::CreateToken { object, .. } => object,
                _ => return false,
            };
            state.objects.try_get(entered).is_some_and(|live| {
                live.zone == Zone::Battlefield
                    && live.controller == controller
                    && crate::engine::object_has_type(state, entered, CardType::Artifact)
            })
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

/// The triggers this collection puts on the stack that no printed trigger
/// condition matches: queued ones, Chancellor of the Annex's opening-hand
/// reveal and first-spell tax, and Tezzeret emblems.
pub(crate) fn delayed_triggers(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Vec<PendingTrigger> {
    let mut triggers = opening_hand_triggers(state, events);
    triggers.extend(first_spell_triggers(state, events));
    triggers.extend(emblem_triggers(state, events));
    let Some(eternal) = state.eternal_v1.as_mut() else {
        return triggers;
    };
    triggers.extend(
        std::mem::take(&mut eternal.vintage.queued_triggers)
            .into_iter()
            .map(|queued| {
                crate::eternal_cards_v1::delayed_trigger(
                    queued.source,
                    queued.controller,
                    vintage(queued.op),
                )
            }),
    );
    triggers
}

/// "You may reveal this card from your opening hand": offered for each
/// Chancellor of the Annex in a hand as the game's first upkeep begins,
/// before anyone could have cast a spell.
fn opening_hand_triggers(state: &mut GameState, events: &[CommittedEvent]) -> Vec<PendingTrigger> {
    let first_upkeep = state.turn == 1
        && state.active_player == state.starting_player
        && state.extra_turn_v1.is_none()
        && events
            .iter()
            .any(|event| matches!(event, CommittedEvent::UpkeepBegan { .. }));
    if !first_upkeep
        || state
            .eternal_v1
            .as_ref()
            .is_some_and(|eternal| eternal.vintage.opening_hand_offered)
    {
        return Vec::new();
    }
    let chancellors = [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(|player| state.players[player.index()].hand.iter().copied())
        .filter(|&card| {
            CARD_DEFS
                .get(state.objects.get(card).card_def as usize)
                .is_some_and(|def| def.name == CHANCELLOR && def.is_executable())
        })
        .collect::<Vec<_>>();
    if chancellors.is_empty() {
        return Vec::new();
    }
    vintage_state(state).opening_hand_offered = true;
    chancellors
        .into_iter()
        .map(|card| {
            let source = AbilitySourceContractV4::capture(state, card);
            crate::eternal_cards_v1::delayed_trigger(
                source,
                source.owner,
                chancellor_reveal_choice(),
            )
        })
        .collect()
}

/// "When each opponent casts their first spell of the game, counter that
/// spell unless that player pays {1}."
fn first_spell_triggers(state: &mut GameState, events: &[CommittedEvent]) -> Vec<PendingTrigger> {
    if !state
        .eternal_v1
        .as_ref()
        .is_some_and(|eternal| eternal.vintage.opening_hand_offered)
    {
        return Vec::new();
    }
    let mut triggers = Vec::new();
    for event in events {
        let CommittedEvent::SpellCast { spell, controller } = event else {
            continue;
        };
        let caster = *controller;
        let stack_item = cast_stack_item(state, *spell);
        let vintage = vintage_state(state);
        if vintage.cast_a_spell[caster.index()] {
            continue;
        }
        vintage.cast_a_spell[caster.index()] = true;
        let (fired, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut vintage.chancellor_reveals)
            .into_iter()
            .partition(|reveal| reveal.owner == caster.opponent());
        vintage.chancellor_reveals = kept;
        let Some(stack_item) = stack_item else {
            continue;
        };
        triggers.extend(fired.into_iter().map(|reveal| {
            crate::eternal_cards_v1::delayed_trigger(
                reveal,
                reveal.owner,
                counter_unless_pays_one(stack_item),
            )
        }));
    }
    triggers
}

/// Tezzeret, Cruel Captain's emblem: "At the beginning of combat on your
/// turn, put three +1/+1 counters on target artifact you control."
fn emblem_triggers(state: &GameState, events: &[CommittedEvent]) -> Vec<PendingTrigger> {
    let Some(eternal) = state.eternal_v1.as_ref() else {
        return Vec::new();
    };
    if eternal.vintage.tezzeret_emblems.is_empty() {
        return Vec::new();
    }
    let mut triggers = Vec::new();
    for event in events {
        let CommittedEvent::BeginningOfCombat { active_player } = event else {
            continue;
        };
        for (owner, source) in &eternal.vintage.tezzeret_emblems {
            if owner == active_player {
                let mut pending = crate::eternal_cards_v1::delayed_trigger(
                    *source,
                    *owner,
                    tezzeret_emblem_counters(),
                );
                pending.target_spec = TargetSpec::StandardV1(StandardTargetV1::ControlledArtifact);
                triggers.push(pending);
            }
        }
    }
    triggers
}

/// Mana Vault and Time Vault: "This artifact doesn't untap during your
/// untap step."
pub(crate) fn prevents_untap_step(state: &GameState, object: ObjectId) -> bool {
    active_named(state, object, MANA_VAULT)
        || active_named(state, object, TIME_VAULT)
        || crate::eternal_e5a_v1::statics::prevents_untap_step(state, object)
        || crate::eternal_legacy_e8a_v1::statics::prevents_untap_step(state, object)
        || crate::eternal_premodern_p1a_v1::statics::prevents_untap_step(state, object)
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
    (state.players[player.index()].spells_cast_this_turn >= 1
        && active_permanents(state, ARCHON_OF_EMERIA, None)
            .next()
            .is_some())
        || crate::eternal_legacy_e5b_v1::cast_prohibited(state, player)
        || crate::eternal_legacy_e9a_v1::statics::cast_prohibited(state, player)
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
    // Collector Ouphe and Phyrexian Revoker (wave E7b).
    if crate::eternal_vintage_e7b_v1::statics::activations_prohibited(state, object) {
        return true;
    }
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
    (artifact
        && active_permanents(state, KARN, Some(live.controller.opponent()))
            .next()
            .is_some())
        || crate::eternal_lurrus_v1::activations_prohibited(state, object)
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
        filter: crate::effect::LibraryCardFilter::CardWithTypeV1(CardType::Artifact),
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
