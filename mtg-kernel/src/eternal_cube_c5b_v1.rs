//! Card programs for the eternal catalog's Vintage Cube wave C5b
//! (`docs/design/eternal_formats_support_v1.md`): token makers and mana creatures.
//!
//! `eternal_cards_v1` forwards this module's effect leaves here, and the
//! recipe tables of `build_eternal_c5b_v1.rs` name its programs. Every
//! table is keyed by a printed name that occurs only in the eternal registry,
//! so the Pauper, FDN and Standard catalogs never reach it. Behavior was read
//! from each card's current Oracle text and, where present, its XMage source.
//!
//! Where each card lives:
//!
//! - Worldspine Wurm: its dies trigger is `TriggerCondition::LeftBattlefieldToGraveyard`
//!   (Wurmcoil Engine's, which does not check that the permanent was a
//!   creature as it left) over `CreateTokensDynamic` (Esika's Chariot's
//!   simultaneous token batch).
//!   "When put into a graveyard from anywhere, shuffle it into its owner's
//!   library" is `C5bTriggerV1::PutIntoGraveyardFromAnywhereV1` (Emrakul's
//!   arrival-style trigger, which functions from the graveyard) and a leaf
//!   that reads the frozen graveyard incarnation (Lembas's shuffle).
//! - Myr Battlesphere: four Myr tokens on entry; the attack trigger is a
//!   multi-object choice of this module (`C5bChoiceV1::BattlesphereTapV1`),
//!   then the pump (`install_temporary_boost`) and the damage to what it
//!   attacks (`attack_target_v1::damage_recipient`).
//! - Mole Man: landfall creates a Moloid token (whose own attack trigger
//!   mills a card on request); "play lands from your graveyard" is an extra
//!   land-play source (`extra_land_candidates`, called next to Crucible of
//!   Worlds's).
//! - Torsten: the reveal of the top seven is a multi-select choice
//!   (`C5bChoiceV1::TorstenPickV1`, Atraxa's shape), the rest go to the bottom
//!   in a random order; the dies trigger makes seven Soldier tokens.
//! - Titania: the entry trigger returns a land card from the graveyard
//!   (`TitaniaReturnV1`); "a land you control is put into a graveyard from the
//!   battlefield" is `C5bTriggerV1::ControlledLandToGraveyardV1`, which also
//!   sees a land that leaves together with Titania (603.10a).
//! - Otharri: experience counters are `C5bStateV1::experience`; the attack
//!   trigger is `OtharriAttackV1`; the graveyard ability is a recipe row in
//!   `build_eternal_c5b_v1.rs`.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{CardType, DynamicValueDef, Keywords, Subtype, TargetSpec, CARD_DEFS};
use crate::effect::{
    EffectContinuation, EffectFrame, EffectObjectBinding, EffectOp, ExecCtx, PlayerRef,
};
use crate::eternal_cards_v1::e3::{E3ChoiceKindV1, E3ChoiceV1};
use crate::eternal_cards_v1::{EternalOpV1, EternalTriggerV1};
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::standard_cards_v1::{StandardOpV1, StandardTargetV1, StandardTriggerV1};
use crate::state::{GameState, ObjectLinkV4, Target, Zone};
use crate::trigger::{TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

pub(crate) const WORLDSPINE_WURM: &str = "Worldspine Wurm";
pub(crate) const MYR_BATTLESPHERE: &str = "Myr Battlesphere";
pub(crate) const MOLE_MAN: &str = "Mole Man, Moloid Master";
pub(crate) const TORSTEN: &str = "Torsten, Founder of Benalia";
pub(crate) const TITANIA: &str = "Titania, Protector of Argoth";
pub(crate) const OTHARRI: &str = "Otharri, Suns' Glory";

const WURM_TOKEN: &str = "Green Wurm Token";
const MYR_TOKEN: &str = "Myr Token";
const MOLOID_TOKEN: &str = "Moloid Token";
const SOLDIER_TOKEN: &str = "Soldier Token";
const ELEMENTAL_TOKEN: &str = "Green Elemental Token";
const REBEL_TOKEN: &str = "Rebel Token";

// ---- State ---------------------------------------------------------------------------

/// Per-game state for this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Default (and absent from snapshots)
/// until a card of this wave needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct C5bStateV1 {
    /// Each player's experience counters (Otharri). They never go away.
    #[serde(default, skip_serializing_if = "is_no_experience")]
    pub experience: [u32; 2],
}

fn is_no_experience(experience: &[u32; 2]) -> bool {
    *experience == [0, 0]
}

impl C5bStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

pub(crate) fn c5b_state(state: &mut GameState) -> &mut C5bStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).c5b
}

/// `player`'s experience counters.
pub fn experience_counters(state: &GameState, player: PlayerId) -> u32 {
    state
        .eternal_v1
        .as_ref()
        .map_or(0, |eternal| eternal.c5b.experience[player.index()])
}

// ---- Trigger conditions, leaves and choices -----------------------------------------

/// Trigger conditions owned by this module, carried as
/// `eternal_cards_v1::EternalTriggerV1::C5bV1`. Each matcher checks its own
/// source, since it may function from the graveyard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum C5bTriggerV1 {
    /// "When this is put into a graveyard from anywhere" (Worldspine Wurm).
    /// Functions from the graveyard it was just put into.
    PutIntoGraveyardFromAnywhereV1,
    /// "Whenever a land you control is put into a graveyard from the
    /// battlefield" (Titania).
    ControlledLandToGraveyardV1,
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::C5bV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C5bOpV1 {
    /// Worldspine Wurm: "shuffle it into its owner's library", only while the
    /// card is still the graveyard incarnation that triggered.
    ShuffleSourceIntoLibraryV1,
    /// Myr Battlesphere: "you may tap X untapped Myr you control. If you do,
    /// this creature gets +X/+0 until end of turn and deals X damage to the
    /// player or planeswalker it's attacking." A choice.
    BattlesphereAttackV1,
    /// Titania: "return target land card from your graveyard to the
    /// battlefield".
    TitaniaReturnV1,
    /// Torsten: "reveal the top seven cards of your library. Put any number
    /// of creature and/or land cards from among them into your hand and the
    /// rest on the bottom of your library in a random order." A choice.
    TorstenRevealV1,
    /// Otharri: "you get an experience counter. Then create a 2/2 red Rebel
    /// creature token that's tapped and attacking for each experience
    /// counter you have."
    OtharriAttackV1,
}

impl C5bOpV1 {
    /// Whether the leaf may stop for a player's choice (or a draw that a
    /// replacement can interrupt), so it runs in the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(self, Self::BattlesphereAttackV1 | Self::TorstenRevealV1)
    }
}

/// A choice of this module, carried as `E3ChoiceKindV1::C5bV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C5bChoiceV1 {
    /// Myr Battlesphere: any number of the untapped Myr to tap.
    BattlesphereTapV1,
    /// Torsten: any number of the revealed creature and land cards to put
    /// into the hand. `revealed` is the whole revealed prefix, in library
    /// order, so the rest can go to the bottom.
    TorstenPickV1 { revealed: Vec<EffectObjectBinding> },
}

impl C5bChoiceV1 {
    pub(crate) fn public_purpose(&self) -> crate::rl::TargetSelectionPurposeV4 {
        match self {
            Self::BattlesphereTapV1 => crate::rl::TargetSelectionPurposeV4::PermanentSelection,
            Self::TorstenPickV1 { .. } => crate::rl::TargetSelectionPurposeV4::CardSelection,
        }
    }

    /// Object incarnations this choice refers to beyond its candidates.
    pub(crate) fn referenced_bindings(&self) -> Vec<EffectObjectBinding> {
        match self {
            Self::BattlesphereTapV1 => Vec::new(),
            Self::TorstenPickV1 { revealed } => revealed.clone(),
        }
    }
}

fn c5b(op: C5bOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C5bV1(op)))
}

fn token_id(name: &str) -> u16 {
    crate::card_def::card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
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

fn definition(state: &GameState, object: ObjectId) -> Option<&'static crate::card_def::CardDef> {
    let live = state.objects.try_get(object)?;
    CARD_DEFS.get(usize::from(live.card_def))
}

// ---- Programs -----------------------------------------------------------------------

/// "Create `count` `token` tokens" as one simultaneous batch (Esika's
/// Chariot's shape).
fn tokens(token: &str, count: i32) -> EffectOp {
    EffectOp::CreateTokensDynamic {
        token_def: token_id(token),
        controller: PlayerRef::Controller,
        count: DynamicValueDef::Fixed(count),
        tapped: false,
    }
}

/// Worldspine Wurm: "When this creature dies, create three 5/5 green Wurm
/// creature tokens with trample."
fn wurm_tokens() -> EffectOp {
    tokens(WURM_TOKEN, 3)
}

/// Worldspine Wurm: "When Worldspine Wurm is put into a graveyard from
/// anywhere, shuffle it into its owner's library."
fn wurm_shuffle() -> EffectOp {
    c5b(C5bOpV1::ShuffleSourceIntoLibraryV1)
}

/// Myr Battlesphere: "When this creature enters, create four 1/1 colorless
/// Myr artifact creature tokens."
fn battlesphere_tokens() -> EffectOp {
    tokens(MYR_TOKEN, 4)
}

/// Myr Battlesphere's attack trigger.
fn battlesphere_attack() -> EffectOp {
    c5b(C5bOpV1::BattlesphereAttackV1)
}

/// Mole Man: "Landfall -- Whenever a land you control enters, create a 1/1
/// green Minion creature token named Moloid".
fn moloid_token() -> EffectOp {
    EffectOp::CreateToken {
        token_def: token_id(MOLOID_TOKEN),
        controller: PlayerRef::Controller,
    }
}

/// The Moloid token's "Whenever this token attacks, you may mill a card."
fn moloid_mill() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: vec![
            EffectOp::Sequence(Vec::new()),
            EffectOp::MillCards {
                player: PlayerRef::Controller,
                count: 1,
            },
        ],
    }
}

/// Torsten's entry trigger.
fn torsten_reveal() -> EffectOp {
    c5b(C5bOpV1::TorstenRevealV1)
}

/// Torsten: "When Torsten dies, create seven 1/1 white Soldier creature
/// tokens."
fn torsten_soldiers() -> EffectOp {
    tokens(SOLDIER_TOKEN, 7)
}

/// Titania's entry trigger.
fn titania_return() -> EffectOp {
    c5b(C5bOpV1::TitaniaReturnV1)
}

/// Titania: "create a 5/3 green Elemental creature token."
fn titania_elemental() -> EffectOp {
    tokens(ELEMENTAL_TOKEN, 1)
}

/// Otharri's attack trigger.
fn otharri_attack() -> EffectOp {
    c5b(C5bOpV1::OtharriAttackV1)
}

// ---- Triggered abilities ------------------------------------------------------------

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

const fn c5b_trigger(condition: C5bTriggerV1, effect: fn() -> EffectOp) -> TriggeredAbilityDef {
    trigger(
        TriggerCondition::StandardV1(StandardTriggerV1::EternalV1(EternalTriggerV1::C5bV1(
            condition,
        ))),
        effect,
    )
}

/// A dies trigger that functions from the graveyard (Wurmcoil Engine's
/// condition: the creature cards of this wave are creatures as they leave).
const fn dies_trigger(effect: fn() -> EffectOp) -> TriggeredAbilityDef {
    TriggeredAbilityDef {
        home_zone: Zone::Graveyard,
        ..trigger(TriggerCondition::LeftBattlefieldToGraveyard, effect)
    }
}

const WORLDSPINE_TRIGGERS: [TriggeredAbilityDef; 2] = [
    dies_trigger(wurm_tokens),
    TriggeredAbilityDef {
        home_zone: Zone::Graveyard,
        ..c5b_trigger(C5bTriggerV1::PutIntoGraveyardFromAnywhereV1, wurm_shuffle)
    },
];
const BATTLESPHERE_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, battlesphere_tokens),
    trigger(TriggerCondition::Attacks, battlesphere_attack),
];
const MOLE_MAN_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::ControlledLandEnters,
    moloid_token,
)];
const MOLOID_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Attacks, moloid_mill)];
const TORSTEN_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, torsten_reveal),
    dies_trigger(torsten_soldiers),
];
const TITANIA_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, titania_return),
    c5b_trigger(C5bTriggerV1::ControlledLandToGraveyardV1, titania_elemental),
];
const OTHARRI_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Attacks, otharri_attack)];

/// Triggered abilities of this module's cards, falling through to wave
/// C5c.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        WORLDSPINE_WURM => &WORLDSPINE_TRIGGERS,
        MYR_BATTLESPHERE => &BATTLESPHERE_TRIGGERS,
        MOLE_MAN => &MOLE_MAN_TRIGGERS,
        MOLOID_TOKEN => &MOLOID_TRIGGERS,
        TORSTEN => &TORSTEN_TRIGGERS,
        TITANIA => &TITANIA_TRIGGERS,
        OTHARRI => &OTHARRI_TRIGGERS,
        _ => crate::eternal_cube_c5c_v1::triggers_for(name),
    }
}

fn is_own_name(name: &str) -> bool {
    matches!(
        name,
        WORLDSPINE_WURM | MYR_BATTLESPHERE | MOLE_MAN | MOLOID_TOKEN | TORSTEN | TITANIA | OTHARRI
    )
}

/// Target specs of this module's printed and queued triggered abilities.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    if !is_own_name(name) {
        return delayed_effect_matches(name, effect).then_some(TargetSpec::None);
    }
    if name == TITANIA && *effect == titania_return() {
        return Some(TargetSpec::StandardV1(
            StandardTargetV1::LandCardInOwnGraveyardV1,
        ));
    }
    triggers_for(name)
        .iter()
        .any(|ability| (ability.effect)() == *effect)
        .then_some(TargetSpec::None)
}

/// Queued triggers owned by this module's cards.
pub(crate) fn delayed_effect_matches(_name: &str, _effect: &EffectOp) -> bool {
    false
}

/// Whether `condition` triggers on its source's arrival in the zone it now
/// occupies, so the trigger's source is that new incarnation rather than the
/// one that left.
pub(crate) fn arrival_condition(condition: C5bTriggerV1) -> bool {
    condition == C5bTriggerV1::PutIntoGraveyardFromAnywhereV1
}

pub(crate) fn trigger_matches(
    condition: C5bTriggerV1,
    events: &[CommittedEvent],
    index: usize,
    source: ObjectId,
    state: &GameState,
) -> bool {
    let Some(live) = state.objects.try_get(source) else {
        return false;
    };
    match condition {
        C5bTriggerV1::PutIntoGraveyardFromAnywhereV1 => {
            // Worldspine Wurm: functions from the graveyard it was just put
            // into, the way Emrakul's does.
            matches!(
                events[index],
                CommittedEvent::ZoneChange {
                    object,
                    to: Zone::Graveyard,
                    ..
                } if object == source
            ) && live.zone == Zone::Graveyard
                && state.players[live.owner.index()]
                    .graveyard
                    .contains(&source)
                && !live.v4.is_token
                && live.spell_copy_origin.is_none()
                && definition(state, source).is_some_and(|def| def.name == WORLDSPINE_WURM)
        }
        C5bTriggerV1::ControlledLandToGraveyardV1 => {
            let CommittedEvent::ZoneChange {
                object,
                from: Zone::Battlefield,
                to: Zone::Graveyard,
                controller_before,
            } = events[index]
            else {
                return false;
            };
            let land_card = state
                .objects
                .try_get(object)
                .is_some_and(|card| CARD_DEFS[usize::from(card.card_def)].has_type(CardType::Land));
            if object == source
                || !land_card
                || !definition(state, source).is_some_and(|def| def.name == TITANIA)
            {
                return false;
            }
            // Titania sees the land if it is on the battlefield, or if it
            // left in this same batch (603.10a), under the same controller.
            let titania_controller = if live.zone == Zone::Battlefield {
                if live.v4.face_index != 0
                    || !crate::continuous_characteristics_v1::printed_abilities_active(
                        state, source,
                    )
                {
                    return false;
                }
                live.controller
            } else {
                let Some(controller) = events.iter().find_map(|event| match *event {
                    CommittedEvent::ZoneChange {
                        object: leaver,
                        from: Zone::Battlefield,
                        controller_before: before,
                        ..
                    } if leaver == source => Some(before),
                    _ => None,
                }) else {
                    return false;
                };
                // The trigger is controlled by the card's current controller.
                if controller != live.controller {
                    return false;
                }
                controller
            };
            titania_controller == controller_before
        }
    }
}

// ---- Engine hooks -------------------------------------------------------------------

/// Whether `object` is a battlefield permanent named `name` whose printed
/// abilities apply.
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

/// Mole Man: "You may play lands from your graveyard." The land cards
/// `player` may play from their graveyard (Icetill Explorer's list).
pub(crate) fn extra_land_candidates(state: &GameState, player: PlayerId) -> Vec<ObjectId> {
    let has_mole_man = state.players[player.index()]
        .battlefield
        .iter()
        .any(|&object| {
            state.objects.get(object).controller == player && active_named(state, object, MOLE_MAN)
        });
    if !has_mole_man {
        return Vec::new();
    }
    state.players[player.index()]
        .graveyard
        .iter()
        .copied()
        .filter(|&card| {
            let live = state.objects.get(card);
            live.owner == player
                && !live.v4.is_token
                && CARD_DEFS[usize::from(live.card_def)].is_playable_land()
        })
        .collect()
}

// ---- Execution ----------------------------------------------------------------------

/// Worldspine Wurm: shuffle the card into its owner's library if it is
/// still the graveyard incarnation that triggered (Lembas's shuffle).
fn run_shuffle_source(ctx: &ExecCtx, state: &mut GameState) {
    let Some(source) = ctx.ability_source_contract else {
        halt(state, ctx.source);
        return;
    };
    let Some(live) = state.objects.try_get(source.source) else {
        return;
    };
    if live.card_def != source.card_def || live.owner != source.owner {
        halt(state, ctx.source);
        return;
    }
    if live.zone != Zone::Graveyard || live.zone_change_count != source.zone_change_count {
        return;
    }
    let mut staged = state.clone();
    let Ok(shuffle_token) = staged.preflight_library_shuffle(source.owner) else {
        halt(state, ctx.source);
        return;
    };
    event::propose_and_commit(
        &mut staged,
        ProposedEvent::public_library_insert(source.source, event::LibraryPlacement::Top),
    );
    if staged
        .commit_library_shuffle(source.owner, shuffle_token)
        .is_err()
    {
        halt(state, ctx.source);
        return;
    }
    *state = staged;
}

/// Titania: return target land card from your graveyard to the battlefield,
/// if it is still that card in that graveyard.
fn run_titania_return(ctx: &ExecCtx, state: &mut GameState) {
    let Some(Target::Object(land)) = ctx.targets.first().copied() else {
        return;
    };
    if !ctx.target_incarnation_matches(0, state) {
        return;
    }
    let live = state.objects.get(land);
    if live.zone != Zone::Graveyard
        || live.owner != ctx.controller
        || !CARD_DEFS[usize::from(live.card_def)].has_type(CardType::Land)
    {
        return;
    }
    event::propose_and_commit(
        state,
        ProposedEvent::zone_change_to_battlefield_under_controller(land, ctx.controller),
    );
}

/// Otharri: the experience counter, then one Rebel token tapped and
/// attacking for each counter (Adeline's tokens, one creation each).
fn run_otharri_attack(ctx: &ExecCtx, state: &mut GameState) {
    let slot = &mut c5b_state(state).experience[ctx.controller.index()];
    *slot = slot.saturating_add(1);
    let count = *slot;
    let rebel = token_id(REBEL_TOKEN);
    for _ in 0..count {
        crate::effect::execute(
            &EffectOp::CreateTokenTappedAndAttacking { token_def: rebel },
            ctx,
            state,
        );
    }
}

/// Runs one of this module's leaves in synchronous execution.
pub(crate) fn execute(op: &C5bOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        C5bOpV1::ShuffleSourceIntoLibraryV1 => run_shuffle_source(ctx, state),
        C5bOpV1::TitaniaReturnV1 => run_titania_return(ctx, state),
        C5bOpV1::OtharriAttackV1 => run_otharri_attack(ctx, state),
        // These ask a player, so they only run in `drive`.
        C5bOpV1::BattlesphereAttackV1 | C5bOpV1::TorstenRevealV1 => {}
    }
}

/// The untapped Myr `player` controls, in battlefield order.
fn untapped_myr(state: &GameState, player: PlayerId) -> Vec<ObjectId> {
    state.players[player.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&object| {
            let live = state.objects.get(object);
            live.controller == player
                && live.zone == Zone::Battlefield
                && !live.tapped
                && crate::engine::has_effective_subtype(state, object, Subtype::Myr)
        })
        .collect()
}

/// Offers `kind` to `player` over `candidates`.
#[allow(clippy::too_many_arguments)]
fn offer(
    continuation: &mut EffectContinuation,
    kind: C5bChoiceV1,
    player: PlayerId,
    candidates: Vec<Target>,
    bindings: Vec<EffectObjectBinding>,
    min: u16,
    max: u16,
    path: Vec<u16>,
) {
    let remaining: Vec<EffectFrame> = continuation.frames.clone();
    crate::eternal_cards_v1::e3::offer(
        continuation,
        E3ChoiceV1 {
            kind: E3ChoiceKindV1::C5bV1(kind),
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

/// Puts `cards`, the current top of `player`'s library in order, on the
/// bottom in a random order (Atraxa's).
fn bottom_in_random_order(
    state: &mut GameState,
    player: PlayerId,
    cards: &[ObjectLinkV4],
) -> Result<(), String> {
    if cards.is_empty() {
        return Ok(());
    }
    let objects = cards.iter().map(|card| card.object).collect::<Vec<_>>();
    state.apply_scry_result(player, cards, &[], &objects)?;
    state
        .randomize_library_bottom_v1(player, objects.len())
        .map_err(|error| error.to_string())
}

fn links(bindings: &[EffectObjectBinding]) -> Vec<ObjectLinkV4> {
    bindings
        .iter()
        .map(|binding| ObjectLinkV4 {
            object: binding.object,
            zone_change_count: binding.expected_zone_change_count,
        })
        .collect()
}

/// Runs one of this module's leaves in the resumable interpreter.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &C5bOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    match op {
        C5bOpV1::BattlesphereAttackV1 => {
            let player = continuation.ctx.controller;
            let candidates = untapped_myr(state, player);
            if candidates.is_empty() {
                return Ok(false);
            }
            let bindings = candidates
                .iter()
                .map(|&object| bind(state, object))
                .collect::<Vec<_>>();
            let max = u16::try_from(candidates.len()).unwrap_or(u16::MAX);
            offer(
                continuation,
                C5bChoiceV1::BattlesphereTapV1,
                player,
                candidates.into_iter().map(Target::Object).collect(),
                bindings,
                0,
                max,
                path,
            );
            Ok(true)
        }
        C5bOpV1::TorstenRevealV1 => {
            let player = continuation.ctx.controller;
            let library = state.players[player.index()].library.clone();
            let count = library.len().min(7);
            if count == 0 {
                return Ok(false);
            }
            // spellbench-public-reveal: public reveal to both players.
            for observer in [PlayerId::P0, PlayerId::P1] {
                state.reveal_library_top(observer, player, count);
            }
            let revealed = library[..count]
                .iter()
                .map(|&card| bind(state, card))
                .collect::<Vec<_>>();
            let eligible = revealed
                .iter()
                .copied()
                .filter(|binding| {
                    definition(state, binding.object).is_some_and(|def| {
                        def.has_type(CardType::Creature) || def.has_type(CardType::Land)
                    })
                })
                .collect::<Vec<_>>();
            if eligible.is_empty() {
                bottom_in_random_order(state, player, &links(&revealed))?;
                return Ok(false);
            }
            let max = u16::try_from(eligible.len()).unwrap_or(u16::MAX);
            offer(
                continuation,
                C5bChoiceV1::TorstenPickV1 { revealed },
                player,
                eligible
                    .iter()
                    .map(|binding| Target::Object(binding.object))
                    .collect(),
                eligible,
                0,
                max,
                path,
            );
            Ok(true)
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
    kind: &C5bChoiceV1,
    bindings: Vec<EffectObjectBinding>,
) -> Result<bool, String> {
    match kind {
        C5bChoiceV1::BattlesphereTapV1 => {
            let ctx = continuation.ctx.clone();
            // "Tap X untapped Myr you control": each must still be one.
            let mut tapped = 0_i32;
            for binding in &bindings {
                let Some(live) = state.objects.try_get(binding.object) else {
                    continue;
                };
                if live.zone != Zone::Battlefield
                    || live.zone_change_count != binding.expected_zone_change_count
                    || live.tapped
                    || live.controller != choice.player
                {
                    continue;
                }
                event::propose_and_commit(state, ProposedEvent::tap(binding.object));
                tapped += 1;
            }
            if tapped == 0 {
                return Ok(false);
            }
            let Some(contract) = ctx.ability_source_contract else {
                return Ok(false);
            };
            let Some(live) = state.objects.try_get(contract.source) else {
                return Ok(false);
            };
            if live.zone != Zone::Battlefield
                || live.zone_change_count != contract.zone_change_count
            {
                return Ok(false);
            }
            crate::effect::install_temporary_boost(
                state,
                EffectObjectBinding {
                    object: contract.source,
                    expected_zone: Zone::Battlefield,
                    expected_zone_change_count: contract.zone_change_count,
                },
                tapped,
                0,
                Keywords::NONE,
            );
            // "The player or planeswalker it's attacking": nothing while it
            // is no longer attacking, or once the planeswalker is gone.
            if state.engine.combat.attackers.contains(&contract.source) {
                if let Some((target, _)) =
                    crate::attack_target_v1::damage_recipient(state, contract.source)
                {
                    event::propose_and_commit(
                        state,
                        event::damage_from_exec_ctx_v1(&ctx, target, tapped),
                    );
                }
            }
            Ok(false)
        }
        C5bChoiceV1::TorstenPickV1 { revealed } => {
            let player = choice.player;
            let live_prefix = revealed.iter().all(|binding| {
                state.objects.try_get(binding.object).is_some_and(|live| {
                    live.zone == Zone::Library
                        && live.zone_change_count == binding.expected_zone_change_count
                })
            });
            if !live_prefix {
                return Err("a Torsten card left the library".to_string());
            }
            if !bindings.is_empty() {
                event::propose_and_commit_batch(
                    state,
                    bindings
                        .iter()
                        .map(|binding| ProposedEvent::zone_change(binding.object, Zone::Hand))
                        .collect(),
                );
            }
            let rest = revealed
                .iter()
                .copied()
                .filter(|binding| !bindings.contains(binding))
                .collect::<Vec<_>>();
            bottom_in_random_order(state, player, &links(&rest))?;
            Ok(false)
        }
    }
}
