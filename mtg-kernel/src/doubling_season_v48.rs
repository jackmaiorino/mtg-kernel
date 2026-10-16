//! Shared replacement inputs. No registry entry or consumer admission.
use crate::{
    card_def::{
        CardCapability, CardType, Keywords, TargetSpec, TokenCounterReplacementDefV48, CARD_DEFS,
    },
    effect::{EffectOp, ObjectRef},
    ids::{ObjectId, PlayerId},
    mana::{ManaColor, Pip},
    state::{AbilitySourceContractV4, GameState, Zone},
};
use serde::{Deserialize, Serialize};

pub const MAGE_COMMIT: &str = "a5c90fe180021e70e2a644ade00eeab07f857a40";
pub const MAGE_PATH: &str = "Mage.Sets/src/mage/cards/d/DoublingSeason.java";

pub fn owns_definition(id: u16) -> bool {
    let Some(d) = CARD_DEFS.get(usize::from(id)) else {
        return false;
    };
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && exact_definition(d)
        && crate::trigger::triggers_for(id).is_empty()
}

pub(crate) fn exact_definition(d: &crate::card_def::CardDef) -> bool {
    d.name == "Doubling Season"
        && matches!(d.capability, CardCapability::Partial | CardCapability::Full)
        && d.types == [CardType::Enchantment]
        && d.subtypes.is_empty()
        && d.supertypes.is_empty()
        && d.cost.generic == 4
        && d.cost.x_count == 0
        && d.cost.pips == [Pip::Colored(ManaColor::G)]
        && d.colors == [ManaColor::G]
        && d.mana_value == 5
        && d.power.is_none()
        && d.toughness.is_none()
        && !d.is_land
        && !d.is_token
        && d.keywords == Keywords::NONE
        && d.target_spec == TargetSpec::None
        && d.alt_cost.is_none()
        && d.kicker_cost.is_none()
        && d.additional_cost.is_none()
        && d.flashback.is_none()
        && d.ward_cost.is_none()
        && d.starting_loyalty.is_none()
        && d.object_name == "Doubling Season"
        && !d.spell_cannot_be_countered
        && !d.records_mana_spent
        && d.generic_cost_reduction.is_none()
        && d.equipment.is_none()
        && d.enters_with_plus_one_counters.is_none()
        && d.controlled_counter_keyword.is_none()
        && d.game_outcome_protection_v1.is_none()
        && d.controlled_creature_grant.is_none()
        && (d.mana_ability)().is_none()
        && d.produces_mana.is_empty()
        && d.plot_cost.is_none()
        && d.madness_cost.is_none()
        && d.escape.is_none()
        && d.mode2.is_none()
        && d.mode3.is_none()
        && d.additional_modes.is_empty()
        && d.kicked_target_spec.is_none()
        && d.mana_ability_choices.is_empty()
        && !d.enters_battlefield_tapped
        && d.enters_battlefield_tapped_unless.is_none()
        && d.mana_ability_def.is_none()
        && d.minimum_blockers == 1
        && d.omen.is_none()
        && d.bestow.is_none()
        && d.adventure.is_none()
        && !d.mana_ability_includes_chosen_color
        && d.as_enters_choose_color_other_than.is_none()
        && d.additional_mana_abilities.is_empty()
        && d.additional_mana_ability_conditions.is_empty()
        && d.attachment.is_none()
        && d.transform_face.is_none()
        && d.saga.is_none()
        && d.optional_additional_cost.is_none()
        && !d.changeling
        && !d.delve
        && !d.cant_be_blocked_by_monarchs_creatures
        && d.conditional_tap_yield.is_none()
        && d.enters_tapped_unless_controller.is_none()
        && d.restricted_mana_abilities.is_empty()
        && d.animation.is_none()
        && d.activated_ability_generic_reductions.is_empty()
        && d.combat_static_rules == crate::card_def::CombatStaticRulesDef::NONE
        && d.as_enters_color_choice_v1.is_none()
        && d.chosen_color_creature_boost_v1.is_none()
        && d.damage_replacement_v1.is_none()
        && d.starting_life_self_bonuses_v1.is_empty()
        && d.source_type_hexproof_v1.is_none()
        && d.other_controlled_nontoken_death_v1.is_none()
        && d.activated_abilities.is_empty()
        && d.token_counter_replacement_v48
            == Some(TokenCounterReplacementDefV48::DoubleEffectTokensAndControlledPermanentCounters)
        && (d.spell_effect)()
            == Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
}

/// Every source is sampled before the event, in physical battlefield order.
/// The recipient controller and placing actor remain distinct.
pub(crate) fn sources(state: &GameState, controller: PlayerId) -> Vec<AbilitySourceContractV4> {
    state
        .objects
        .iter()
        .filter(|(id, o)| {
            o.zone == Zone::Battlefield
                && o.controller == controller
                && o.v4.face_index == 0
                && owns_definition(o.card_def)
                && crate::continuous_characteristics_v1::printed_abilities_active(state, *id)
        })
        .map(|(id, _)| AbilitySourceContractV4::capture(state, id))
        .collect()
}

pub(crate) fn entry_input(
    state: &GameState,
    event: &crate::event::ProposedEvent,
) -> Option<ReplacementInputV48> {
    match event {
        crate::event::ProposedEvent::CreateToken(t) => {
            let mut input = ReplacementInputV48::capture(state, t.controller);
            input.subtype_entries_v1 =
                crate::giada_v1::capture_entry_replacements(state, t.controller, None);
            Some(input)
        }
        crate::event::ProposedEvent::ZoneChange(z) if z.to_zone == Zone::Battlefield => {
            let mut input = ReplacementInputV48::capture(
                state,
                z.battlefield_controller.unwrap_or_else(|| {
                    let live = state.objects.get(z.object);
                    if live.zone == Zone::Stack {
                        live.controller
                    } else {
                        live.owner
                    }
                }),
            );
            if z.counter_origin_v48 == Some(CounterOriginV48::Rule)
                && input.entry_talent_doublings == Some(0)
            {
                input.seasons.clear();
            }
            input.subtype_entries_v1 = crate::giada_v1::capture_entry_replacements(
                state,
                input.controller,
                Some(z.object),
            );
            Some(input)
        }
        _ => None,
    }
}

pub(crate) fn entry_amount(
    state: &GameState,
    input: Option<&ReplacementInputV48>,
    controller: PlayerId,
    amount: i32,
) -> Result<i32, String> {
    let original =
        if requires_successor(state) || input.is_some_and(|input| !input.seasons.is_empty()) {
            if amount < 0 {
                return Err("negative entry counter input".into());
            }
            let doublings = input
                .and_then(|input| input.entry_talent_doublings)
                .unwrap_or_else(|| {
                    crate::standard_cards_v1::counter_modifier_doublings_v48(state, controller)
                });
            (0..doublings).try_fold(amount, |amount, _| {
                amount.checked_mul(2).ok_or("entry modifier overflow")
            })?
        } else {
            crate::standard_cards_v1::scale_counters(state, controller, amount)
        };
    match input {
        Some(input) => input.replaced_count(original),
        None => ReplacementInputV48::capture(state, controller).replaced_count(original),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CounterOriginV48 {
    Effect,
    Cost,
    /// Turn-based counters and played lands do not become effects merely
    /// because they use the same placement primitive.
    Rule,
}

/// Original effect placement, sampled by the genuine entry driver before entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectXEntryV48 {
    pub original: i32,
    pub host: Option<crate::state::ObjectLinkV4>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplacementInputV48 {
    pub controller: PlayerId,
    pub seasons: Vec<AbilitySourceContractV4>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_controls_big: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_talent_doublings: Option<u32>,
    /// Already-active entry replacements and prebatch counts, not later entrants.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subtype_entries_v1: Vec<crate::giada_v1::EntryReplacementV1>,
}
impl std::hash::Hash for ReplacementInputV48 {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        use std::hash::Hash;
        self.controller.hash(state);
        self.seasons.hash(state);
        self.entry_controls_big.hash(state);
        self.entry_talent_doublings.hash(state);
        if !self.subtype_entries_v1.is_empty() {
            "subtype-entry-replacements/v1".hash(state);
            self.subtype_entries_v1.hash(state);
        }
    }
}
impl ReplacementInputV48 {
    pub(crate) fn capture(state: &GameState, controller: PlayerId) -> Self {
        Self {
            controller,
            seasons: sources(state, controller),
            subtype_entries_v1: Vec::new(),
            entry_talent_doublings: Some(crate::standard_cards_v1::counter_modifier_doublings_v48(
                state, controller,
            )),
            entry_controls_big: Some(
                state.players[controller.index()]
                    .battlefield
                    .iter()
                    .any(|id| crate::engine::object_mana_value(state, *id) >= 4),
            ),
        }
    }
    /// The same nonconsuming replacements all commute; each applies once.
    pub fn replaced_count(&self, original: i32) -> Result<i32, String> {
        if original < 0 {
            return Err("negative replacement count".into());
        }
        let mut result = original;
        for _ in &self.seasons {
            result = result
                .checked_mul(2)
                .ok_or("token/counter replacement overflow")?;
        }
        Ok(result)
    }
    pub(crate) fn replaced_loyalty(&self, original: u32) -> Result<u32, String> {
        self.seasons.iter().try_fold(original, |count, _| {
            count
                .checked_mul(2)
                .ok_or_else(|| "loyalty entry overflow".into())
        })
    }
    pub fn replaced_token_count(&self, original: usize) -> Result<usize, String> {
        self.seasons.iter().try_fold(original, |count, _| {
            count
                .checked_mul(2)
                .ok_or_else(|| "token roster count overflow".into())
        })
    }
}

pub(crate) fn entry_loyalty_amount(
    state: &GameState,
    input: Option<&ReplacementInputV48>,
    controller: PlayerId,
    amount: u16,
) -> Result<u32, String> {
    let original =
        if requires_successor(state) || input.is_some_and(|input| !input.seasons.is_empty()) {
            let doublings = input
                .and_then(|input| input.entry_talent_doublings)
                .unwrap_or_else(|| {
                    crate::standard_cards_v1::counter_modifier_doublings_v48(state, controller)
                });
            (0..doublings).try_fold(u32::from(amount), |amount, _| {
                amount
                    .checked_mul(2)
                    .ok_or("starting loyalty modifier overflow")
            })?
        } else {
            u32::try_from(crate::standard_cards_v1::scale_counters(
                state,
                controller,
                i32::from(amount),
            ))
            .map_err(|_| "negative starting loyalty")?
        };
    let replacement = input
        .cloned()
        .unwrap_or_else(|| ReplacementInputV48::capture(state, controller));
    replacement.replaced_loyalty(original)
}

/// Check only placements the actual ordinary entry requests. A source entering
/// simultaneously does not join the replacement roster for another member.
pub(crate) fn preflight_entry_definition(
    state: &GameState,
    definition: u16,
    input: &ReplacementInputV48,
    attributes: (bool, Option<i32>, bool, Option<u8>),
) -> Result<(), String> {
    if input.seasons.is_empty() {
        return Ok(());
    }
    let (kicked, x, face_down, time) = attributes;
    if face_down {
        return Ok(());
    }
    let def = CARD_DEFS
        .get(usize::from(definition))
        .ok_or("entry definition absent")?;
    let fixed = x.unwrap_or_else(|| {
        def.enters_with_plus_one_counters
            .filter(|entry| !entry.if_kicked || kicked)
            .map_or(0, |entry| entry.count)
    });
    let mut plus = entry_amount(state, Some(input), input.controller, fixed)?;
    if x.is_none() && state.player_attacked_this_turn_v1(input.controller) {
        if let Some(count) = crate::fdn_raid_v1::entry_counter_count_for(def.name) {
            plus = plus
                .checked_add(entry_amount(
                    state,
                    Some(input),
                    input.controller,
                    i32::from(count),
                )?)
                .ok_or("combined entry counters overflow")?;
        }
    }
    if def.name == "Ascendant Packleader" && input.entry_controls_big == Some(true) {
        plus.checked_add(entry_amount(state, Some(input), input.controller, 1)?)
            .ok_or("combined conditional entry plus counters overflow")?;
    }
    if def.name == "Evolving Adaptive" {
        i16::try_from(entry_amount(state, Some(input), input.controller, 1)?)
            .map_err(|_| "entry oil counter storage overflow")?;
    }
    if let Some(time) = time {
        u8::try_from(entry_amount(
            state,
            Some(input),
            input.controller,
            i32::from(time),
        )?)
        .map_err(|_| "entry time counter storage overflow")?;
    }
    if let Some(loyalty) = def.starting_loyalty {
        entry_loyalty_amount(state, Some(input), input.controller, loyalty)?;
    }
    if matches!(def.name, "Braided Net" | "Reckoner Bankbuster") {
        entry_amount(state, Some(input), input.controller, 3)?;
    }
    if def.saga.is_some() {
        i16::try_from(entry_amount(state, Some(input), input.controller, 1)?)
            .map_err(|_| "entry lore storage overflow")?;
    }
    Ok(())
}
pub(crate) fn preflight_entry_roster(
    state: &GameState,
    events: &[crate::event::ProposedEvent],
) -> Result<(), String> {
    for event in events {
        let Some(input) = entry_input(state, event) else {
            continue;
        };
        let (definition, kicked, face_down) = match event {
            crate::event::ProposedEvent::CreateToken(token) => (token.token_def, false, false),
            crate::event::ProposedEvent::ZoneChange(change) => {
                let live = state.objects.get(change.object);
                (
                    live.card_def,
                    live.zone == Zone::Stack
                        && state.engine.pending_kicked_source == Some(change.object),
                    false,
                )
            }
            _ => unreachable!(),
        };
        preflight_entry_definition(state, definition, &input, (kicked, None, face_down, None))?;
    }
    Ok(())
}

/// Counter effects use the live recipient's controller. Talent's distinct
/// actor-relative modifier is deliberately applied separately, as before.
pub(crate) fn counter_amount(
    state: &GameState,
    recipient: ObjectId,
    actor: PlayerId,
    origin: CounterOriginV48,
    amount: i32,
) -> Result<i32, String> {
    if amount < 0 {
        return Err("negative counter placement".into());
    }
    let live = state
        .objects
        .try_get(recipient)
        .ok_or("missing counter recipient")?;
    if live.zone != Zone::Battlefield {
        return Err("counter recipient is not a permanent".into());
    }
    let scaled = if requires_successor(state) {
        crate::standard_cards_v1::scale_counters_checked_v48(state, actor, amount)?
    } else {
        crate::standard_cards_v1::scale_counters(state, actor, amount)
    };
    // 614.16: another replacement effect can make an original cost/rule
    // placement eligible. Unmodified costs and rule actions remain excluded.
    if origin != CounterOriginV48::Effect
        && crate::standard_cards_v1::counter_modifier_doublings_v48(state, actor) == 0
    {
        return Ok(scaled);
    }
    ReplacementInputV48::capture(state, live.controller).replaced_count(scaled)
}

pub(crate) fn require_legacy_visible(state: &GameState, actor: PlayerId) -> Result<(), String> {
    require_legacy_supported(state)?;
    // Consult only this observer's genuine current knowledge. An unknown
    // opponent hand/library definition must never select a public refusal.
    let visible_selected = state.objects.iter().any(|(id, object)| {
        let visible = match object.zone {
            Zone::Hand => {
                object.owner == actor
                    || state
                        .known_hand_cards(actor, object.owner)
                        .iter()
                        .any(|entry| {
                            entry.object == id
                                && entry.zone_change_count == object.zone_change_count
                        })
            }
            Zone::Library => state
                .known_library_cards(actor, object.owner)
                .iter()
                .any(|entry| {
                    entry.object == id
                        && entry.zone_change_count == object.zone_change_count
                        && state.players[object.owner.index()]
                            .library
                            .get(entry.position as usize)
                            == Some(&id)
                }),
            Zone::Battlefield | Zone::Graveyard | Zone::Exile | Zone::Command | Zone::Stack => {
                object
                    .v4
                    .face_down_v1
                    .is_none_or(|face| face.lookers & (1 << actor.index()) != 0)
            }
        };
        visible && owns_definition(object.card_def)
    });
    if visible_selected {
        return Err("visible Doubling Season printing requires V48".into());
    }
    Ok(())
}
pub(crate) fn require_legacy_supported(state: &GameState) -> Result<(), String> {
    if requires_successor(state) {
        Err("Doubling Season requires its distinct V48 producer".into())
    } else {
        Ok(())
    }
}
pub(crate) fn requires_successor(state: &GameState) -> bool {
    state.objects.iter().any(|(_, o)| {
        o.zone == Zone::Battlefield && o.v4.face_down_v1.is_none() && owns_definition(o.card_def)
    }) || state.engine.event_history.iter().any(|event| {
        matches!(
            event,
            crate::event::CommittedEvent::TokenCreationReplacementV48 { .. }
                | crate::event::CommittedEvent::PlusCounterReplacementV48 { .. }
                | crate::event::CommittedEvent::PlusCounterInputV48 { .. }
                | crate::event::CommittedEvent::TokenCreationInputV48 { .. }
                | crate::event::CommittedEvent::TokenAttachmentJournalV48 { .. }
                | crate::event::CommittedEvent::BaitTransferInputV48 { .. }
                | crate::event::CommittedEvent::BaitTransferAppliedV48 { .. }
        )
    }) || state.engine.event_history.iter().any(|event| {
        matches!(event,
        crate::event::CommittedEvent::PlusOneCountersAdded { replacement_input_v48, .. }
        if !replacement_input_v48.is_none())
    }) || state.engine.event_history.iter().any(|event| {
        matches!(event,
        crate::event::CommittedEvent::CreateToken { replacement_input_v48, .. }
        if !replacement_input_v48.is_none())
    }) || state.stack.iter().any(|item| {
        item.v4
            .source_contract
            .is_some_and(|source| source.zone == Zone::Stack && owns_definition(source.card_def))
            || state.objects.try_get(item.source).is_some_and(|object| {
                object.zone == Zone::Stack && owns_definition(object.card_def)
            })
    })
}

/// Original low-level effect input for the previously omitted bait family.
/// It is private, sampled before any transferred family changes the board.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaitTransferInputV48 {
    pub index: u64,
    pub world: Box<GameState>,
    pub recipient: crate::state::ObjectLinkV4,
    pub actor: PlayerId,
    pub original: i32,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaitTransferAppliedV48 {
    pub input: u64,
    pub before: i32,
    pub after: i32,
}
pub(crate) fn record_bait_transfer_input(
    state: &mut GameState,
    recipient: ObjectId,
    actor: PlayerId,
    original: i32,
) -> Result<Option<u64>, String> {
    if original == 0 {
        return Ok(None);
    }
    let live = state.objects.get(recipient);
    let placed = counter_amount(state, recipient, actor, CounterOriginV48::Effect, original)?;
    live.counters
        .bait
        .checked_add(placed)
        .ok_or("bait transfer overflow")?;
    let link = crate::state::ObjectLinkV4 {
        object: recipient,
        zone_change_count: live.zone_change_count,
    };
    let mut world = state.clone();
    world.engine.event_history.clear();
    world.engine.event_log.clear();
    let world: GameState =
        serde_json::from_slice(&serde_json::to_vec(&world).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let index = state.engine.event_history.len() as u64;
    state
        .engine
        .event_history
        .push(crate::event::CommittedEvent::BaitTransferInputV48 {
            input: Box::new(BaitTransferInputV48 {
                index,
                world: Box::new(world),
                recipient: link,
                actor,
                original,
            }),
        });
    Ok(Some(index))
}
pub(crate) fn record_bait_transfer_applied(
    state: &mut GameState,
    input: Option<u64>,
) -> Result<(), String> {
    let Some(index) = input else { return Ok(()) };
    let Some(crate::event::CommittedEvent::BaitTransferInputV48 { input }) =
        state.engine.event_history.get(index as usize)
    else {
        return Err("bait transfer lost original input".into());
    };
    let before = input
        .world
        .objects
        .get(input.recipient.object)
        .counters
        .bait;
    let after = state.objects.get(input.recipient.object).counters.bait;
    state
        .engine
        .event_history
        .push(crate::event::CommittedEvent::BaitTransferAppliedV48 {
            placement: Box::new(BaitTransferAppliedV48 {
                input: index,
                before,
                after,
            }),
        });
    Ok(())
}
pub(crate) fn bait_transfer_balance(
    state: &GameState,
    index: usize,
) -> Result<Option<(crate::state::ObjectLinkV4, i32, i32)>, String> {
    let Some(crate::event::CommittedEvent::BaitTransferAppliedV48 { placement }) =
        state.engine.event_history.get(index)
    else {
        return Ok(None);
    };
    let Some(crate::event::CommittedEvent::BaitTransferInputV48 { input }) =
        state.engine.event_history.get(placement.input as usize)
    else {
        return Err("bait transfer lacks independent original world".into());
    };
    let live = input
        .world
        .objects
        .try_get(input.recipient.object)
        .ok_or("bait transfer original recipient absent")?;
    let amount = counter_amount(
        &input.world,
        input.recipient.object,
        input.actor,
        CounterOriginV48::Effect,
        input.original,
    )?;
    let after = live
        .counters
        .bait
        .checked_add(amount)
        .ok_or("bait transfer total overflow")?;
    if input.index != placement.input
        || placement.input >= index as u64
        || input.original <= 0
        || !input.world.engine.event_history.is_empty()
        || !input.world.engine.event_log.is_empty()
        || live.zone != Zone::Battlefield
        || live.zone_change_count != input.recipient.zone_change_count
        || live.counters.bait != placement.before
        || after != placement.after
    {
        return Err("bait transfer changed original board, recipient, or effect count".into());
    }
    Ok(Some((input.recipient, placement.before, placement.after)))
}
fn validate_bait_transfers(state: &GameState) -> Result<(), String> {
    for (at, event) in state.engine.event_history.iter().enumerate() {
        match event {
            crate::event::CommittedEvent::BaitTransferInputV48 {input} => {
                if input.index!=at as u64 || state.engine.event_history.iter().filter(|event|matches!(event,crate::event::CommittedEvent::BaitTransferAppliedV48 {placement} if placement.input==at as u64)).count()!=1 {
                    return Err("bait transfer input lost unique actual placement".into());
                }
            }
            crate::event::CommittedEvent::BaitTransferAppliedV48 {..} => {bait_transfer_balance(state,at)?;}
            _=>{}
        }
    }
    if state.engine.event_log.iter().any(|event| {
        matches!(
            event,
            crate::event::CommittedEvent::BaitTransferInputV48 { .. }
                | crate::event::CommittedEvent::BaitTransferAppliedV48 { .. }
        )
    }) {
        return Err("private bait transfer injected as trigger input".into());
    }
    Ok(())
}

/// Properties of the creation effect, installed on every expanded token
/// before the first entry observer is captured. None leaves old proposals exact.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenEntryV48 {
    pub copied: bool,
    pub haste_until_end_of_turn: bool,
    pub attacking: bool,
    pub attached_to: Option<crate::state::ObjectLinkV4>,
    pub plus_one_counters: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sacrifice_at_end_step: Option<AbilitySourceContractV4>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenTemplateV48 {
    pub definition: u16,
    pub controller: PlayerId,
    pub face: Option<u8>,
    pub tapped: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initialization: Option<TokenEntryV48>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenRequestV48 {
    pub template: TokenTemplateV48,
    pub replacement: ReplacementInputV48,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenCreationV48 {
    pub index: u64,
    pub input: u64,
    pub original: Vec<TokenRequestV48>,
    pub created: Vec<u64>,
}

/// Independent original replacement world. Only the board needed to evaluate
/// continuous source abilities is retained; prior event logs are excluded so
/// each input does not recursively embed every older input. This is private.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenInputV48 {
    pub index: u64,
    pub world: Box<GameState>,
    pub requests: Vec<TokenTemplateV48>,
}
impl TokenInputV48 {
    pub(crate) fn capture(
        state: &GameState,
        requests: &[TokenRequestV48],
    ) -> Result<Option<Self>, String> {
        if requests.is_empty() {
            return Ok(None);
        }
        // Strip earlier journals before serializing. Serialization then removes
        // private runtime loans without granting replay authority to this world.
        let mut original = state.clone();
        original.engine.event_history.clear();
        original.engine.event_log.clear();
        let bytes = serde_json::to_vec(&original).map_err(|error| error.to_string())?;
        let world: GameState = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        Ok(Some(Self {
            index: 0,
            world: Box::new(world),
            requests: requests
                .iter()
                .map(|request| request.template.clone())
                .collect(),
        }))
    }
    fn original_requests(&self) -> Vec<TokenRequestV48> {
        self.requests
            .iter()
            .map(|template| TokenRequestV48 {
                replacement: ReplacementInputV48::capture(&self.world, template.controller),
                template: template.clone(),
            })
            .collect()
    }
}
pub(crate) fn record_token_input(state: &mut GameState, mut input: TokenInputV48) -> u64 {
    input.index = state.engine.event_history.len() as u64;
    let index = input.index;
    state
        .engine
        .event_history
        .push(crate::event::CommittedEvent::TokenCreationInputV48 {
            input: Box::new(input),
        });
    index
}

/// Intrinsic input binding on the actual issued counter occurrence. The
/// absent extension emits no serde field and contributes no bytes to Hash,
/// preserving the predecessor occurrence's derived hash exactly.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CounterInputRefV48(pub Option<u64>);
impl CounterInputRefV48 {
    pub fn is_none(&self) -> bool {
        self.0.is_none()
    }
}
impl std::hash::Hash for CounterInputRefV48 {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        if let Some(input) = self.0 {
            std::hash::Hash::hash(b"counter_input_v48", state);
            std::hash::Hash::hash(&input, state);
        }
    }
}

/// Intrinsic binding for an actual created member of a replaced token batch.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TokenInputRefV48(pub Option<u64>);
impl TokenInputRefV48 {
    pub fn is_none(&self) -> bool {
        self.0.is_none()
    }
}
impl std::hash::Hash for TokenInputRefV48 {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        if let Some(input) = self.0 {
            std::hash::Hash::hash(b"token_input_v48", state);
            std::hash::Hash::hash(&input, state);
        }
    }
}

/// Original actor/recipient replacement world, captured before placement.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlusCounterInputV48 {
    pub index: u64,
    pub world: Box<GameState>,
    pub recipient: crate::state::ObjectLinkV4,
    pub actor: PlayerId,
    pub original: i32,
}
impl PlusCounterInputV48 {
    pub(crate) fn capture(
        state: &GameState,
        recipient: ObjectId,
        actor: PlayerId,
        original: i32,
    ) -> Result<Self, String> {
        let live = state.objects.get(recipient);
        let link = crate::state::ObjectLinkV4 {
            object: recipient,
            zone_change_count: live.zone_change_count,
        };
        let mut world = state.clone();
        world.engine.event_history.clear();
        world.engine.event_log.clear();
        let bytes = serde_json::to_vec(&world).map_err(|error| error.to_string())?;
        let world = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        Ok(Self {
            index: 0,
            world: Box::new(world),
            recipient: link,
            actor,
            original,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlusCounterPlacementV48 {
    pub index: u64,
    pub input: u64,
    pub recipient: crate::state::ObjectLinkV4,
    pub actor: PlayerId,
    pub replacement: ReplacementInputV48,
    pub original: i32,
    pub before: i32,
    pub after: i32,
    pub counter_event: u64,
}

pub(crate) fn expected_plus_at(
    state: &GameState,
    at: u64,
    recipient: ObjectId,
    actor: PlayerId,
    original: i32,
) -> Result<i32, String> {
    let matches = state
        .engine
        .event_history
        .iter()
        .filter_map(|event| match event {
            crate::event::CommittedEvent::PlusCounterReplacementV48 { placement: p }
                if p.counter_event == at =>
            {
                Some(p.as_ref())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [] => {
            if matches!(state.engine.event_history.get(at as usize),
                Some(crate::event::CommittedEvent::PlusOneCountersAdded { replacement_input_v48, .. })
                if !replacement_input_v48.is_none())
            {
                return Err("issued replaced counter lost mandatory original proof".into());
            }
            Ok(original)
        }
        [p] if p.recipient.object == recipient && p.actor == actor && p.original == original => {
            let Some(crate::event::CommittedEvent::PlusCounterInputV48 { input }) =
                state.engine.event_history.get(p.input as usize)
            else {
                return Err("owned counter placement lacks original actor world".into());
            };
            let Some(live) = input.world.objects.try_get(recipient) else {
                return Err("owned counter original recipient absent".into());
            };
            if input.index != p.input
                || p.input >= at
                || at >= p.index
                || input.actor != actor
                || input.original != original
                || input.recipient != p.recipient
                || live.zone != Zone::Battlefield
                || live.zone_change_count != p.recipient.zone_change_count
                || live.counters.plus1_plus1 != p.before
                || p.replacement != ReplacementInputV48::capture(&input.world, live.controller)
            {
                return Err("owned counter original placement changed".into());
            }
            // The original actor-relative modifier runs before the recipient's
            // Season replacements. Reapplying only Season to the printed amount
            // would lose an authenticated Talent multiplier in a joint catalog.
            let count = counter_amount(
                &input.world,
                recipient,
                actor,
                CounterOriginV48::Effect,
                original,
            )?;
            if p.before.checked_add(count) != Some(p.after)
                || !matches!(state.engine.event_history.get(at as usize),
                    Some(crate::event::CommittedEvent::PlusOneCountersAdded { object, zone_change_count, player, count: actual, replacement_input_v48 })
                    if replacement_input_v48.0 == Some(p.input) && *object == recipient && *zone_change_count == p.recipient.zone_change_count
                        && *player == actor && *actual == count)
            {
                return Err("owned counter actual placement changed".into());
            }
            Ok(count)
        }
        _ => Err("counter replacement original operation changed".into()),
    }
}

pub(crate) fn preflight_token_initialization(
    state: &GameState,
    events: &[crate::event::ProposedEvent],
) -> Result<(), String> {
    for event in events {
        let crate::event::ProposedEvent::CreateToken(token) = event else {
            continue;
        };
        let Some(init) = &token.initialization_v48 else {
            continue;
        };
        let def = CARD_DEFS
            .get(usize::from(token.token_def))
            .ok_or("token blueprint absent")?;
        if init.plus_one_counters < 0 {
            return Err("negative initialized token counters".into());
        }
        let input = ReplacementInputV48::capture(state, token.controller);
        let fixed = def
            .enters_with_plus_one_counters
            .filter(|entry| !entry.if_kicked)
            .map_or(0, |entry| entry.count);
        let fixed = entry_amount(state, Some(&input), token.controller, fixed)?;
        let additional = entry_amount(
            state,
            Some(&input),
            token.controller,
            init.plus_one_counters,
        )?;
        fixed
            .checked_add(additional)
            .ok_or("initialized token counters overflow")?;
        if let Some(host) = init.attached_to {
            let live = state
                .objects
                .try_get(host.object)
                .ok_or("token Aura host absent")?;
            let attachment = def
                .attachment
                .filter(|a| a.is_creature_aura())
                .ok_or("attached token is not Aura")?;
            if live.zone != Zone::Battlefield
                || live.zone_change_count != host.zone_change_count
                || !crate::engine::object_has_type(state, host.object, attachment.enchanted_type())
                || (def.colors.len() == 1
                    && crate::engine::has_effective_keyword(
                        state,
                        host.object,
                        Keywords::PROTECTION_FROM_MONOCOLORED,
                    ))
            {
                return Err("token Aura original attachment is illegal".into());
            }
        }
        if init.attacking && !def.types.contains(&CardType::Creature) {
            return Err("noncreature token cannot enter attacking".into());
        }
    }
    Ok(())
}

/// Expand against one unchanged pre-event board. Entry of an additional
/// replacement source cannot change later requests in this roster.
pub(crate) fn expand_tokens(
    state: &GameState,
    events: Vec<crate::event::ProposedEvent>,
) -> Result<(Vec<crate::event::ProposedEvent>, Vec<TokenRequestV48>), String> {
    use crate::event::ProposedEvent;
    let mut result = Vec::new();
    let mut original = Vec::new();
    for event in events {
        if let ProposedEvent::CreateToken(t) = &event {
            let replacement = ReplacementInputV48::capture(state, t.controller);
            let count = replacement.replaced_token_count(1)?;
            original.push(TokenRequestV48 {
                template: TokenTemplateV48 {
                    definition: t.token_def,
                    controller: t.controller,
                    face: t.entry_face_index,
                    tapped: t.entry_tapped,
                    initialization: t.initialization_v48.clone(),
                },
                replacement,
            });
            result
                .try_reserve(count)
                .map_err(|_| "token roster allocation failed")?;
            for _ in 0..count {
                result.push(event.clone());
            }
        } else {
            result.push(event);
        }
    }
    if original
        .iter()
        .all(|request| request.replacement.seasons.is_empty())
    {
        original.clear();
    }
    Ok((result, original))
}

pub(crate) fn record_token_creation(
    state: &mut GameState,
    start: usize,
    input: u64,
    original: Vec<TokenRequestV48>,
) -> Result<(), String> {
    if original.is_empty() {
        return Ok(());
    }
    let created = state.engine.event_history[start..]
        .iter()
        .enumerate()
        .filter_map(|(i, event)| {
            matches!(event, crate::event::CommittedEvent::CreateToken { .. })
                .then_some((start + i) as u64)
        })
        .collect();
    let index = state.engine.event_history.len() as u64;
    state
        .engine
        .event_history
        .push(crate::event::CommittedEvent::TokenCreationReplacementV48 {
            creation: Box::new(TokenCreationV48 {
                index,
                input,
                original,
                created,
            }),
        });
    validate_token_journals(state)
}

pub(crate) fn validate_token_journals(state: &GameState) -> Result<(), String> {
    crate::soul_counters_v1::validate(state)?;
    crate::season_token_attachment_v48::validate_journals(state)?;
    validate_bait_transfers(state)?;

    use crate::event::CommittedEvent;
    if state.engine.event_log.iter().any(|event| {
        matches!(
            event,
            CommittedEvent::TokenCreationReplacementV48 { .. }
                | CommittedEvent::PlusCounterReplacementV48 { .. }
                | CommittedEvent::PlusCounterInputV48 { .. }
                | CommittedEvent::TokenCreationInputV48 { .. }
        )
    }) {
        return Err("private replacement replayed as trigger input".into());
    }
    for (index, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::TokenCreationInputV48 { input } = event else {
            continue;
        };
        let closures = state.engine.event_history.iter().filter(|event| matches!(event,
            CommittedEvent::TokenCreationReplacementV48 {creation} if creation.input == index as u64)).count();
        if input.index != index as u64
            || input.requests.is_empty()
            || !input.world.engine.event_history.is_empty()
            || !input.world.engine.event_log.is_empty()
            || closures != 1
        {
            return Err("token creation lost unique original-world input/completion".into());
        }
    }
    for (index, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::PlusOneCountersAdded {
            replacement_input_v48: CounterInputRefV48(Some(input_index)),
            ..
        } = event
        else {
            continue;
        };
        if *input_index >= index as u64
            || !matches!(state.engine.event_history.get(*input_index as usize),
                Some(CommittedEvent::PlusCounterInputV48 { input }) if input.index == *input_index)
            || state
                .engine
                .event_history
                .iter()
                .filter(|event| {
                    matches!(event,
                CommittedEvent::PlusCounterReplacementV48 { placement }
                if placement.input == *input_index && placement.counter_event == index as u64)
                })
                .count()
                != 1
        {
            return Err("issued replaced counter lost original input/completion".into());
        }
    }
    for (index, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::PlusCounterInputV48 { input } = event else {
            continue;
        };
        if input.index != index as u64 || input.original <= 0
            || !input.world.engine.event_history.is_empty() || !input.world.engine.event_log.is_empty()
            || state.engine.event_history.iter().filter(|event| matches!(event,
                CommittedEvent::PlusCounterReplacementV48 { placement } if placement.input == input.index)).count() != 1 {
            return Err("counter placement lost its unique original world/completion".into());
        }
    }
    let mut counter_events = Vec::new();
    for (index, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::PlusCounterReplacementV48 { placement: p } = event else {
            continue;
        };
        let Some(CommittedEvent::PlusCounterInputV48 { input }) =
            state.engine.event_history.get(p.input as usize)
        else {
            return Err("effect counter original actor world absent".into());
        };
        let original_recipient = input
            .world
            .objects
            .try_get(input.recipient.object)
            .ok_or("counter original recipient absent")?;
        if input.index != p.input
            || p.input >= p.counter_event
            || input.recipient != p.recipient
            || input.actor != p.actor
            || input.original != p.original
            || original_recipient.zone != Zone::Battlefield
            || original_recipient.zone_change_count != p.recipient.zone_change_count
            || original_recipient.counters.plus1_plus1 != p.before
            || p.replacement
                != ReplacementInputV48::capture(&input.world, original_recipient.controller)
        {
            return Err("counter actor/recipient replacement input changed".into());
        }
        let count = counter_amount(
            &input.world,
            p.recipient.object,
            p.actor,
            CounterOriginV48::Effect,
            p.original,
        )?;
        if p.index != index as u64
            || p.original <= 0
            || p.replacement.seasons.is_empty()
            || p.counter_event >= p.index
            || p.before.checked_add(count) != Some(p.after)
            || counter_events.contains(&p.counter_event)
            || p.replacement.seasons.iter().enumerate().any(|(i, s)| {
                s.zone != Zone::Battlefield
                    || s.controller != p.replacement.controller
                    || !owns_definition(s.card_def)
                    || p.replacement.seasons[..i]
                        .iter()
                        .any(|other| other.source == s.source)
            })
            || !matches!(state.engine.event_history.get(p.counter_event as usize),Some(CommittedEvent::PlusOneCountersAdded {object,zone_change_count,player,count:actual,replacement_input_v48}) if replacement_input_v48.0==Some(p.input) && *object==p.recipient.object && *zone_change_count==p.recipient.zone_change_count && *player==p.actor && *actual==count)
        {
            return Err("actual effect counter replacement changed".into());
        }
        counter_events.push(p.counter_event);
    }
    for (index, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::CreateToken {
            replacement_input_v48: TokenInputRefV48(Some(input_index)),
            ..
        } = event
        else {
            continue;
        };
        if *input_index >= index as u64
            || !matches!(state.engine.event_history.get(*input_index as usize),
                Some(CommittedEvent::TokenCreationInputV48 { input }) if input.index == *input_index)
            || state
                .engine
                .event_history
                .iter()
                .filter(|event| {
                    matches!(event,
                CommittedEvent::TokenCreationReplacementV48 { creation }
                if creation.input == *input_index && creation.created.contains(&(index as u64)))
                })
                .count()
                != 1
        {
            return Err("issued replaced token lost original input/completion".into());
        }
    }
    let mut all_created = Vec::new();
    for (index, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::TokenCreationReplacementV48 { creation: c } = event else {
            continue;
        };
        if c.index != index as u64
            || c.original.is_empty()
            || c.original.iter().all(|r| r.replacement.seasons.is_empty())
        {
            return Err("token replacement original roster changed".into());
        }
        let Some(CommittedEvent::TokenCreationInputV48 { input }) =
            state.engine.event_history.get(c.input as usize)
        else {
            return Err("token creation original replacement world absent".into());
        };
        if input.index != c.input
            || c.input >= c.index
            || input.original_requests() != c.original
            || c.created.first().is_none_or(|first| *first <= c.input)
        {
            return Err("token creation source/roster changed from original world".into());
        }
        let mut cursor = 0;
        for request in &c.original {
            if request.replacement.controller != request.template.controller
                || request
                    .replacement
                    .seasons
                    .iter()
                    .enumerate()
                    .any(|(i, s)| {
                        s.zone != Zone::Battlefield
                            || s.controller != request.template.controller
                            || !owns_definition(s.card_def)
                            || request.replacement.seasons[..i]
                                .iter()
                                .any(|other| other.source == s.source)
                    })
            {
                return Err("token replacement captured source changed".into());
            }
            let count = request.replacement.replaced_token_count(1)?;
            for _ in 0..count {
                let at = *c
                    .created
                    .get(cursor)
                    .ok_or("token replacement omitted created member")?;
                if at >= c.index
                    || cursor > 0 && c.created[cursor - 1] >= at
                    || all_created.contains(&at)
                    || !matches!(state.engine.event_history.get(at as usize),Some(CommittedEvent::CreateToken {token_def,controller,replacement_input_v48,..}) if replacement_input_v48.0==Some(c.input) && *token_def==request.template.definition && *controller==request.template.controller)
                {
                    return Err("token replacement actual creation changed".into());
                }
                all_created.push(at);
                cursor += 1;
            }
        }
        if cursor != c.created.len() {
            return Err("token replacement added unrequested member".into());
        }
    }
    Ok(())
}

/// Operation owners use the original creation roster, not the later board.
pub(crate) fn expected_tokens(
    state: &GameState,
    start: usize,
    end: usize,
    definition: u16,
    controller: PlayerId,
    original: usize,
) -> Result<usize, String> {
    validate_token_journals(state)?;
    let mut expected = original;
    let mut seen = 0;
    for event in &state.engine.event_history[start..end] {
        let crate::event::CommittedEvent::TokenCreationReplacementV48 { creation: c } = event
        else {
            continue;
        };
        for request in &c.original {
            if request.template.definition == definition
                && request.template.controller == controller
            {
                seen += 1;
                expected = expected
                    .checked_add(request.replacement.replaced_token_count(1)? - 1)
                    .ok_or("token count overflow")?;
            }
        }
    }
    if seen > original {
        return Err("token replacement original request count changed".into());
    }
    Ok(expected)
}

#[cfg(test)]
mod tests;

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod paid_counter_coexistence_tests;
