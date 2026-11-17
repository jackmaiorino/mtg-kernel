//! Card programs for the eternal catalog's Vintage Cube wave C3c
//! (`docs/design/eternal_formats_support_v1.md`): artifact utilities and their tokens.
//!
//! `eternal_cards_v1` forwards this module's effect leaves here, and the
//! recipe tables of `build_eternal_c3c_v1.rs` name its programs. Every
//! table is keyed by a printed name that occurs only in the eternal registry,
//! so the Pauper, FDN and Standard catalogs never reach it. Behavior was read
//! from each card's current Oracle text and, where present, its XMage source.
//!
//! Where each card lives:
//!
//! - Lightning Greaves is Swiftfoot Boots' shape: an `EquipmentDef` (wave
//!   E9a's `equipment_for`) granting haste and shroud and Shuko's "Equip {0}".
//! - Pyrite Spellbomb and Retrofitter Foundry are recipes alone (cost pairs of
//!   Aether Spellbomb, the sacrifice-a-subtype cost of Siege-Gang Commander),
//!   with their tokens as registry rows.
//! - Shorikai, Genesis Engine's loot-and-Pilot is `shorikai_loot` (the loot
//!   with a nonterminal discard in a root sequence, then the Pilot Token whose
//!   crew bonus is `engine::crew_power`); its Crew 8 is Reckoner Bankbuster's.
//! - The Echoverse Fulcrum's entry loot is a printed trigger, its sweep is
//!   Supreme Verdict's program behind an exile-self cost.
//! - Thopter Foundry's nontoken-artifact sacrifice is
//!   `PermanentFilter::NontokenArtifactV1`.
//! - Relic of Sauron's mana is Fixed(2) of one color plus three bundles (wave
//!   E6a's `additional_mana_abilities_for`).
//! - Staff of the Storyteller: story counters are charge counters (Extinguisher
//!   Battleship's station counters are the same stand-in). The "one or more
//!   creature tokens" trigger is `delayed_triggers` over the committed token
//!   events of one trigger collection.
//! - Currency Converter: `note_discard` (called from the engine's discard
//!   commits, as Tourach's is) queues the discards, `delayed_triggers` makes one
//!   "may exile" trigger per Converter, and the cards exiled with each Converter
//!   incarnation are a ledger in `C3cStateV1`.
//! - The Mightstone and Weakstone: a modal entry trigger (`trigger_modes`) and
//!   mana that is restricted by `ManaSpendRestrictionDef::PowerstoneV1`, which
//!   `restrict_produced_mana` applies as the ability resolves.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::{CardType, DynamicValueDef, ManaSpendRestrictionDef, TargetSpec, CARD_DEFS};
use crate::effect::{EffectContinuation, EffectFrame, EffectOp, ExecCtx, PlayerRef, TargetRef};
use crate::eternal_cards_v1::EternalOpV1;
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::{ManaColor, RestrictedManaUnitV1};
use crate::standard_cards_v1::StandardOpV1;
use crate::state::{AbilitySourceContractV4, GameState, ObjectLinkV4, Zone};
use crate::trigger::{PendingTrigger, TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

pub(crate) const ECHOVERSE_FULCRUM: &str = "The Echoverse Fulcrum";
pub(crate) const STAFF_OF_THE_STORYTELLER: &str = "Staff of the Storyteller";
pub(crate) const CURRENCY_CONVERTER: &str = "Currency Converter";
pub(crate) const MIGHTSTONE: &str = "The Mightstone and Weakstone";

const PILOT_TOKEN: &str = "Pilot Token";
const BLUE_THOPTER_TOKEN: &str = "Blue Thopter Token";
const SPIRIT_TOKEN: &str = "White Flying Spirit Token";
const TREASURE_TOKEN: &str = "Treasure Token";
const ROGUE_TOKEN: &str = "Rogue Token";

/// Per-game state for this module's cards, kept inside
/// `eternal_cards_v1::EternalStateV1`. Default (and absent from snapshots)
/// until a card of this wave needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct C3cStateV1 {
    /// Cards discarded by a player who controls a Currency Converter, noted
    /// as they left the hand and waiting for the next trigger collection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub converter_discards: Vec<DiscardedCardV1>,
    /// The cards exiled with each Currency Converter incarnation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub converter_exiled: Vec<LinkedExileV1>,
}

impl C3cStateV1 {
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// One card a player discarded while controlling a Currency Converter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscardedCardV1 {
    pub card: ObjectId,
    pub discarder: PlayerId,
}

/// One card exiled with a Currency Converter: the Converter's incarnation and
/// the exiled card's incarnation in exile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkedExileV1 {
    pub converter: ObjectId,
    pub converter_zone_change_count: u32,
    pub card: ObjectId,
    pub card_zone_change_count: u32,
}

pub(crate) fn c3c_state(state: &mut GameState) -> &mut C3cStateV1 {
    &mut state.eternal_v1.get_or_insert_with(Default::default).c3c
}

/// Effect leaves owned by this module, carried as
/// `eternal_cards_v1::EternalOpV1::C3cV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum C3cOpV1 {
    /// Currency Converter: "Whenever you discard a card, you may exile that
    /// card from your graveyard." Asks the controller, when the card is still
    /// in the graveyard it was discarded to, and runs `ConverterExileV1`.
    ConverterMayExileV1 {
        card: ObjectId,
        zone_change_count: u32,
    },
    /// Exiles the card (if it is still that graveyard card) and links it to
    /// the Converter whose ability this is.
    ConverterExileV1 {
        card: ObjectId,
        zone_change_count: u32,
    },
    /// Currency Converter's "{T}: Put a card exiled with this artifact into
    /// its owner's graveyard...": the controller chooses among the cards
    /// exiled with this Converter incarnation.
    ConverterReleaseV1,
    /// Puts one linked card into its owner's graveyard and creates the token
    /// its type calls for.
    ConverterPutV1 {
        card: ObjectId,
        zone_change_count: u32,
    },
}

impl C3cOpV1 {
    /// Whether the leaf may stop for a player's choice (or a draw that a
    /// replacement can interrupt), so it runs in the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        match self {
            Self::ConverterMayExileV1 { .. } | Self::ConverterReleaseV1 => true,
            Self::ConverterExileV1 { .. } | Self::ConverterPutV1 { .. } => false,
        }
    }
}

fn c3c(op: C3cOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C3cV1(op)))
}

fn token_id(name: &str) -> u16 {
    crate::card_def::card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

fn create_token(name: &str) -> EffectOp {
    EffectOp::CreateToken {
        token_def: token_id(name),
        controller: PlayerRef::Controller,
    }
}

// ---- Programs -----------------------------------------------------------------------

/// Shorikai, Genesis Engine: "{1}, {T}: Draw two cards, then discard a card.
/// Create a 1/1 colorless Pilot creature token with 'This token crews
/// Vehicles as though its power were 2 greater.'" The discard is the
/// resumable interpreter's nonterminal discard of a root sequence.
pub fn shorikai_loot() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 2,
        },
        EffectOp::DiscardCards {
            player: PlayerRef::Controller,
            count: 1,
        },
        create_token(PILOT_TOKEN),
    ])
}

/// The Echoverse Fulcrum: "When The Echoverse Fulcrum enters, draw a card,
/// then discard a card."
fn fulcrum_loot() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        },
        EffectOp::DiscardCards {
            player: PlayerRef::Controller,
            count: 1,
        },
    ])
}

/// Thopter Foundry: "Create a 1/1 blue Thopter artifact creature token with
/// flying. You gain 1 life." (The sacrifice is the ability's cost.)
pub fn thopter_foundry() -> EffectOp {
    EffectOp::Sequence(vec![
        create_token(BLUE_THOPTER_TOKEN),
        EffectOp::GainLife {
            player: PlayerRef::Controller,
            amount: 1,
        },
    ])
}

/// Staff of the Storyteller: "When this artifact enters, create a 1/1 white
/// Spirit creature token with flying."
fn staff_spirit() -> EffectOp {
    create_token(SPIRIT_TOKEN)
}

/// Staff of the Storyteller: "put a story counter on this artifact" (a charge
/// counter; the source must still be the incarnation that triggered).
fn story_counter() -> EffectOp {
    crate::eternal_legacy_e7a_v1::blast_zone_charge_1()
}

/// Currency Converter's "{T}: Put a card exiled with this artifact into its
/// owner's graveyard. If it's a land card, create a Treasure token. If it's a
/// nonland card, create a 2/2 black Rogue creature token."
pub fn converter_release() -> EffectOp {
    c3c(C3cOpV1::ConverterReleaseV1)
}

/// The Mightstone and Weakstone's two modes, each with its target shape.
fn mightstone_modes() -> Vec<(TargetSpec, EffectOp)> {
    vec![
        // "Draw two cards."
        (
            TargetSpec::None,
            EffectOp::Sequence(vec![EffectOp::DrawCards {
                player: PlayerRef::Controller,
                count: 2,
            }]),
        ),
        // "Target creature gets -5/-5 until end of turn."
        (
            TargetSpec::Creature,
            EffectOp::PumpTargetUntilEndOfTurnDynamic {
                target: TargetRef::Target(0),
                power: DynamicValueDef::Fixed(-5),
                toughness: DynamicValueDef::Fixed(-5),
            },
        ),
    ]
}

/// The mode marker of The Mightstone and Weakstone's entry trigger; choosing
/// a mode replaces it with that mode's program.
fn mightstone_entry() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: mightstone_modes()
            .into_iter()
            .map(|(_, effect)| effect)
            .collect(),
    }
}

/// The modes of The Mightstone and Weakstone's unselected entry trigger.
pub(crate) fn trigger_modes(name: &str, effect: &EffectOp) -> Option<Vec<(TargetSpec, EffectOp)>> {
    (name == MIGHTSTONE && *effect == mightstone_entry()).then(mightstone_modes)
}

/// Currency Converter's "may exile that card" trigger for one discard.
fn converter_may_exile(card: ObjectId, zone_change_count: u32) -> EffectOp {
    c3c(C3cOpV1::ConverterMayExileV1 {
        card,
        zone_change_count,
    })
}

// ---- Hooks: engine ------------------------------------------------------------------

/// The Mightstone and Weakstone: "This mana can't be spent to cast nonartifact
/// spells." The two mana its ability just added become restricted units, which
/// artifact spells and every payment that is not part of casting a spell may
/// spend (`ManaSpendRestrictionDef::PowerstoneV1`).
pub(crate) fn restrict_produced_mana(
    state: &mut GameState,
    player: PlayerId,
    source: ObjectId,
    color: ManaColor,
) {
    let live = state.objects.get(source);
    if CARD_DEFS[live.card_def as usize].name != MIGHTSTONE {
        return;
    }
    let unit = RestrictedManaUnitV1 {
        color,
        restriction: ManaSpendRestrictionDef::PowerstoneV1,
        source: ObjectLinkV4 {
            object: source,
            zone_change_count: live.zone_change_count,
        },
        source_card_def: live.card_def,
    };
    let seat = &mut state.players[player.index()];
    let produced = seat.mana_pool[color.pool_index()].min(2);
    seat.mana_pool[color.pool_index()] -= produced;
    for _ in 0..produced {
        seat.restricted_mana_pool.0.push(unit.clone());
    }
}

/// Currency Converter: a card is being discarded from a hand. Called before
/// the card leaves the hand, like Tourach's note; it queues the discard when
/// the discarding player controls a Converter.
pub(crate) fn note_discard(state: &mut GameState, card: ObjectId) {
    let Some(live) = state.objects.try_get(card) else {
        return;
    };
    if live.zone != Zone::Hand {
        return;
    }
    let Some(discarder) = [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .find(|player| state.players[player.index()].hand.contains(&card))
    else {
        return;
    };
    if crate::eternal_e5a_v1::active_permanents(state, CURRENCY_CONVERTER, Some(discarder))
        .next()
        .is_none()
    {
        return;
    }
    c3c_state(state)
        .converter_discards
        .push(DiscardedCardV1 { card, discarder });
}

/// The triggers this collection puts on the stack that no printed trigger
/// condition matches: Staff of the Storyteller's story counters (one per
/// collection in which its controller created a creature token) and Currency
/// Converter's "may exile" triggers (one per Converter for each discard).
pub(crate) fn delayed_triggers(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Vec<PendingTrigger> {
    let mut triggers = Vec::new();
    staff_triggers(state, events, &mut triggers);
    converter_triggers(state, &mut triggers);
    triggers
}

/// Staff of the Storyteller: "Whenever you create one or more creature
/// tokens, put a story counter on this artifact." One trigger per Staff for
/// the creature tokens its controller created since the last collection.
fn staff_triggers(state: &GameState, events: &[CommittedEvent], out: &mut Vec<PendingTrigger>) {
    let mut makers: Vec<PlayerId> = Vec::new();
    for event in events {
        let CommittedEvent::CreateToken {
            object,
            token_def,
            controller,
            ..
        } = event
        else {
            continue;
        };
        let creature = CARD_DEFS
            .get(usize::from(*token_def))
            .is_some_and(|def| def.has_type(CardType::Creature))
            || crate::engine::object_has_type(state, *object, CardType::Creature);
        if creature && !makers.contains(controller) {
            makers.push(*controller);
        }
    }
    for player in makers {
        let staffs =
            crate::eternal_e5a_v1::active_permanents(state, STAFF_OF_THE_STORYTELLER, Some(player))
                .collect::<Vec<_>>();
        for staff in staffs {
            let source = AbilitySourceContractV4::capture(state, staff);
            out.push(crate::eternal_cards_v1::delayed_trigger(
                source,
                player,
                story_counter(),
            ));
        }
    }
}

/// Currency Converter: "Whenever you discard a card, you may exile that card
/// from your graveyard." A card that did not reach a graveyard (a replacement
/// exiled it) has nothing to exile and queues no trigger.
fn converter_triggers(state: &mut GameState, out: &mut Vec<PendingTrigger>) {
    let discards = match state.eternal_v1.as_mut() {
        Some(eternal) if !eternal.c3c.converter_discards.is_empty() => {
            std::mem::take(&mut eternal.c3c.converter_discards)
        }
        _ => return,
    };
    for discarded in discards {
        let Some(live) = state.objects.try_get(discarded.card) else {
            continue;
        };
        if live.zone != Zone::Graveyard {
            continue;
        }
        let count = live.zone_change_count;
        let converters = crate::eternal_e5a_v1::active_permanents(
            state,
            CURRENCY_CONVERTER,
            Some(discarded.discarder),
        )
        .collect::<Vec<_>>();
        for converter in converters {
            let source = AbilitySourceContractV4::capture(state, converter);
            out.push(crate::eternal_cards_v1::delayed_trigger(
                source,
                discarded.discarder,
                converter_may_exile(discarded.card, count),
            ));
        }
    }
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

const FULCRUM_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, fulcrum_loot)];
const STAFF_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(TriggerCondition::Etb, staff_spirit)];
const MIGHTSTONE_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, mightstone_entry)];

/// Triggered abilities of this module's cards, falling through to wave
/// C4a. Staff of the Storyteller's story counters and Currency Converter's
/// discard trigger are delayed triggers.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        ECHOVERSE_FULCRUM => &FULCRUM_TRIGGERS,
        STAFF_OF_THE_STORYTELLER => &STAFF_TRIGGERS,
        MIGHTSTONE => &MIGHTSTONE_TRIGGERS,
        _ => crate::eternal_cube_c4a_v1::triggers_for(name),
    }
}

/// Target specs of this module's printed and queued triggered abilities.
pub(crate) fn trigger_target_spec(name: &str, effect: &EffectOp) -> Option<TargetSpec> {
    if name == MIGHTSTONE {
        if *effect == mightstone_entry() {
            return Some(TargetSpec::None);
        }
        return mightstone_modes()
            .into_iter()
            .find(|(_, branch)| branch == effect)
            .map(|(spec, _)| spec);
    }
    delayed_effect_matches(name, effect).then_some(TargetSpec::None)
}

/// Queued triggers and mode branches owned by this module's cards.
pub(crate) fn delayed_effect_matches(name: &str, effect: &EffectOp) -> bool {
    match name {
        STAFF_OF_THE_STORYTELLER => *effect == story_counter(),
        CURRENCY_CONVERTER => matches!(
            effect,
            EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C3cV1(
                C3cOpV1::ConverterMayExileV1 { .. }
            )))
        ),
        MIGHTSTONE => mightstone_modes()
            .iter()
            .any(|(_, branch)| branch == effect),
        _ => false,
    }
}

// ---- Execution ----------------------------------------------------------------------

/// Whether `card` is still the graveyard incarnation `zone_change_count`
/// names.
fn in_graveyard(state: &GameState, card: ObjectId, zone_change_count: u32) -> bool {
    state.objects.try_get(card).is_some_and(|live| {
        live.zone == Zone::Graveyard && live.zone_change_count == zone_change_count
    })
}

/// Whether `card` is still the exiled incarnation `zone_change_count` names.
fn in_exile(state: &GameState, card: ObjectId, zone_change_count: u32) -> bool {
    state
        .objects
        .try_get(card)
        .is_some_and(|live| live.zone == Zone::Exile && live.zone_change_count == zone_change_count)
}

/// The cards still exiled with the Converter incarnation, in the order they
/// were exiled.
fn converter_candidates(
    state: &GameState,
    converter: ObjectId,
    zone_change_count: u32,
) -> Vec<(ObjectId, u32)> {
    state.eternal_v1.as_ref().map_or_else(Vec::new, |eternal| {
        eternal
            .c3c
            .converter_exiled
            .iter()
            .filter(|link| {
                link.converter == converter
                    && link.converter_zone_change_count == zone_change_count
                    && in_exile(state, link.card, link.card_zone_change_count)
            })
            .map(|link| (link.card, link.card_zone_change_count))
            .collect()
    })
}

/// Runs one of this module's leaves in synchronous execution.
pub(crate) fn execute(op: &C3cOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        // Both ask a player, so they only run in `drive`.
        C3cOpV1::ConverterMayExileV1 { .. } | C3cOpV1::ConverterReleaseV1 => {}
        C3cOpV1::ConverterExileV1 {
            card,
            zone_change_count,
        } => run_converter_exile(ctx, state, *card, *zone_change_count),
        C3cOpV1::ConverterPutV1 {
            card,
            zone_change_count,
        } => run_converter_put(ctx, state, *card, *zone_change_count),
    }
}

/// Currency Converter's "exile that card from your graveyard": the card is
/// exiled only while it is still the card that was discarded, and is linked
/// to the Converter while that Converter is still on the battlefield.
fn run_converter_exile(ctx: &ExecCtx, state: &mut GameState, card: ObjectId, count: u32) {
    if !in_graveyard(state, card, count) {
        return;
    }
    event::propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Exile));
    let Some(contract) = ctx.ability_source_contract else {
        return;
    };
    let Some(moved) = state.objects.try_get(card) else {
        return;
    };
    if moved.zone != Zone::Exile {
        return;
    }
    let card_zone_change_count = moved.zone_change_count;
    let converter_live = state.objects.try_get(contract.source).is_some_and(|live| {
        live.zone == Zone::Battlefield && live.zone_change_count == contract.zone_change_count
    });
    if converter_live {
        c3c_state(state).converter_exiled.push(LinkedExileV1 {
            converter: contract.source,
            converter_zone_change_count: contract.zone_change_count,
            card,
            card_zone_change_count,
        });
    }
}

/// Currency Converter: "Put a card exiled with this artifact into its
/// owner's graveyard. If it's a land card, create a Treasure token. If it's a
/// nonland card, create a 2/2 black Rogue creature token."
fn run_converter_put(ctx: &ExecCtx, state: &mut GameState, card: ObjectId, count: u32) {
    if !in_exile(state, card, count) {
        return;
    }
    let is_land = CARD_DEFS[state.objects.get(card).card_def as usize].is_land;
    c3c_state(state)
        .converter_exiled
        .retain(|link| link.card != card);
    event::propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Graveyard));
    let token = create_token(if is_land { TREASURE_TOKEN } else { ROGUE_TOKEN });
    crate::effect::execute(&token, ctx, state);
}

/// Runs one of this module's leaves in the resumable interpreter.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &C3cOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    match op {
        C3cOpV1::ConverterMayExileV1 {
            card,
            zone_change_count,
        } => {
            // A card that left the graveyard in the meantime is not offered.
            if in_graveyard(state, *card, *zone_change_count) {
                continuation.frames.push(EffectFrame::Program {
                    op: EffectOp::Choice {
                        controller: PlayerRef::Controller,
                        options: vec![
                            c3c(C3cOpV1::ConverterExileV1 {
                                card: *card,
                                zone_change_count: *zone_change_count,
                            }),
                            EffectOp::Sequence(Vec::new()),
                        ],
                    },
                    path,
                });
            }
            Ok(false)
        }
        C3cOpV1::ConverterReleaseV1 => {
            let Some(contract) = continuation.ctx.ability_source_contract else {
                return Ok(false);
            };
            let candidates =
                converter_candidates(state, contract.source, contract.zone_change_count);
            match candidates.as_slice() {
                [] => {}
                [(card, count)] => run_converter_put(&continuation.ctx, state, *card, *count),
                _ => continuation.frames.push(EffectFrame::Program {
                    op: EffectOp::Choice {
                        controller: PlayerRef::Controller,
                        options: candidates
                            .iter()
                            .map(|&(card, zone_change_count)| {
                                c3c(C3cOpV1::ConverterPutV1 {
                                    card,
                                    zone_change_count,
                                })
                            })
                            .collect(),
                    },
                    path,
                }),
            }
            Ok(false)
        }
        C3cOpV1::ConverterExileV1 {
            card,
            zone_change_count,
        } => {
            run_converter_exile(&continuation.ctx, state, *card, *zone_change_count);
            Ok(false)
        }
        C3cOpV1::ConverterPutV1 {
            card,
            zone_change_count,
        } => {
            run_converter_put(&continuation.ctx, state, *card, *zone_change_count);
            Ok(false)
        }
    }
}
