//! Layer-3 full-text primitives for the Partial Volrath's Shapeshifter.
//!
//! The physical printing remains the layer-1 authority. Copy effects read
//! `GameObject::card_def` and do not inherit layer-3 text changes (CR 612.6,
//! 707.2). Native activation freezes the donor through payment and resolution.
//! The ordered Premodern successor owns its public views.
#![allow(dead_code)]

use crate::card_def::{
    ActivatedAbilityDef, ActivationTargetFilter, CardDef, CardType, CostComponent, TargetSpec,
    CARD_DEFS,
};
use crate::effect::{EffectOp, PlayerRef};
use crate::eternal_cards_v1::EternalOpV1;
use crate::eternal_premodern_p2c_v1::{self as p2c, P2cOpV1};
use crate::event::CommittedEvent;
use crate::ids::{ObjectId, PlayerId};
use crate::mana::Cost;
use crate::standard_cards_v1::StandardOpV1;
use crate::state::{
    AbilityKindV4, AbilitySourceContractV4, GameState, ObjectLinkV4, StackItem, StackItemKind, Zone,
};
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

const VOLRATH: &str = "Volrath's Shapeshifter";

/// The layer-1 host and layer-3 text donor are deliberately separate. This
/// ephemeral view is not a public observation or a serialized stack record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FullTextV1 {
    host: AbilitySourceContractV4,
    donor: ObjectLinkV4,
    text_definition: u16,
}

/// Reads only a face-up battlefield host and its current controller's public
/// graveyard top. A noncreature top disables the effect; it does not search
/// backward for an older creature. Later-layer ability removal must not
/// suppress this layer-3 query. The caller supplies the ability's exact host.
pub(crate) fn top_creature_text(
    state: &GameState,
    host: AbilitySourceContractV4,
) -> Option<FullTextV1> {
    let live = state.objects.try_get(host.source)?;
    if live.zone != Zone::Battlefield
        || live.v4.face_down_v1.is_some()
        || AbilitySourceContractV4::capture(state, host.source) != host
    {
        return None;
    }
    let (donor, text_definition) = crate::graveyard_order_v1::batch_text_top(
        state,
        ObjectLinkV4 {
            object: host.source,
            zone_change_count: host.zone_change_count,
        },
        live.controller,
    )
    .unwrap_or_else(|| graveyard_text_top(state, live.controller))?;
    Some(FullTextV1 {
        host,
        donor,
        text_definition,
    })
}

pub(crate) fn graveyard_text_top(
    state: &GameState,
    owner: PlayerId,
) -> Option<crate::graveyard_order_v1::TextTopV1> {
    for &donor in state.players[owner.index()].graveyard.iter().rev() {
        let card = state.objects.try_get(donor)?;
        let definition = CARD_DEFS.get(usize::from(card.card_def))?;
        if card.zone != Zone::Graveyard || card.owner != owner || card.v4.face_down_v1.is_some() {
            return None;
        }
        if card.v4.is_token || definition.is_token || card.spell_copy_origin.is_some() {
            continue;
        }
        return definition.types.contains(&CardType::Creature).then_some((
            ObjectLinkV4 {
                object: donor,
                zone_change_count: card.zone_change_count,
            },
            card.card_def,
        ));
    }
    None
}

/// Layer 3 applies to the printed/copiable Shapeshifter permanent. The
/// physical definition remains the authority for copy and source custody.
pub(crate) fn active_definition(state: &GameState, source: ObjectId) -> Option<&'static CardDef> {
    active_text(state, source).map(FullTextV1::definition)
}

/// Select the donated mana recipe where its exact printed action can use
/// the ordinary physical-host tap and payment paths.
pub(crate) fn mana_definition(state: &GameState, source: ObjectId) -> Option<&'static CardDef> {
    let live = state.objects.try_get(source)?;
    let physical = CARD_DEFS.get(usize::from(live.card_def))?;
    Some(
        mana_text(state, source)
            .map(FullTextV1::definition)
            .unwrap_or(physical),
    )
}

pub(crate) fn mana_text(state: &GameState, source: ObjectId) -> Option<FullTextV1> {
    active_text(state, source).filter(|text| {
        matches!(
            text.definition().name,
            "Birds of Paradise" | "Wall of Roots"
        )
    })
}

pub(crate) fn active_text(state: &GameState, source: ObjectId) -> Option<FullTextV1> {
    if !cfg!(feature = "eternal-fixtures") {
        return None;
    }
    let object = state.objects.try_get(source)?;
    if object.zone != Zone::Battlefield || object.v4.face_down_v1.is_some() {
        return None;
    }
    let physical = CARD_DEFS.get(usize::from(object.card_def))?;
    if !owns_definition(physical) {
        return None;
    }
    top_creature_text(state, AbilitySourceContractV4::capture(state, source))
}

pub(crate) fn owns_definition(definition: &CardDef) -> bool {
    cfg!(feature = "eternal-fixtures") && definition.name == VOLRATH && definition.is_executable()
}

impl FullTextV1 {
    /// Printed full text, before layers 4-7, counters or CDA evaluation.
    /// The receiver must evaluate copied self-references on `host`, not donor.
    pub(crate) fn definition(self) -> &'static CardDef {
        &CARD_DEFS[usize::from(self.text_definition)]
    }

    pub(crate) fn host(self) -> AbilitySourceContractV4 {
        self.host
    }

    pub(crate) fn donor(self) -> ObjectLinkV4 {
        self.donor
    }

    pub(crate) fn is_wall_mana(self) -> bool {
        self.definition().name == "Wall of Roots"
    }

    /// CR 602.5c keeps restrictions separate for abilities acquired from
    /// different objects, even when their printed text is identical.
    pub(crate) fn wall_mana_used(self, state: &GameState) -> bool {
        self.restricted_use(state, AbilityKindV4::Mana, 0)
    }

    pub(crate) fn record_wall_mana_use(self, state: &mut GameState) {
        let used = restricted_use_event(state, self.host, self.donor, AbilityKindV4::Mana, 0);
        state.engine.event_history.push(used);
    }

    fn restricted_use(self, state: &GameState, kind: AbilityKindV4, index: u8) -> bool {
        let used = restricted_use_event(state, self.host, self.donor, kind, index);
        state
            .engine
            .event_history
            .scan_for_v1(&[crate::event_history_v1::EventKindV1::FullTextRestrictedUseV1])
            .contains(&used)
    }

    pub(crate) fn activation_allowed(
        self,
        state: &GameState,
        selector: FullTextAbilityV1,
        ability: ActivatedAbilityDef,
    ) -> bool {
        let FullTextAbilityV1::Printed(printed) = selector else {
            return true;
        };
        p2c::statics::activation_allowed(
            state,
            self.host.source,
            self.definition().name,
            usize::from(printed),
        ) && (ability.max_activations_per_turn.is_none()
            || !self.restricted_use(state, AbilityKindV4::Activated, printed))
    }

    /// Existing grants keep their slots. Two positions follow the physical
    /// printed roster for Equipment, and Cauldron can extend that range.
    pub(crate) fn first_donated_slot(self, state: &GameState) -> u8 {
        let first = first_slot_after_equipment(self.host.card_def);
        crate::standard_cards_v1::cauldron_granted_abilities(state, self.host.source)
            .last()
            .map_or(first, |(last, ..)| first.max(last.saturating_add(1)))
    }

    fn freeze_trigger(
        self,
        state: &GameState,
        kind: DonatedTriggerKindV1,
    ) -> Result<DonatedTriggerCaptureV1, String> {
        if top_creature_text(state, self.host) != Some(self)
            || !crate::continuous_characteristics_v1::printed_abilities_active(
                state,
                self.host.source,
            )
        {
            return Err("full-text trigger lost its active text at collection".into());
        }
        if let DonatedTriggerKindV1::DevourerPower { observed_power } = kind {
            if observed_power != crate::engine::effective_power(state, self.host.source) {
                return Err("full-text state trigger power differs from its native scene".into());
            }
        }
        let captured = DonatedTriggerCaptureV1 {
            host: self.host,
            donor: AbilitySourceContractV4::capture(state, self.donor.object),
            kind,
        };
        captured.program(state)?;
        Ok(captured)
    }

    /// Freeze the selector and its native slot before costs can change
    /// the graveyard top or any grants.
    pub(crate) fn freeze_activated(
        self,
        state: &GameState,
        selector: FullTextAbilityV1,
    ) -> Result<FrozenFullTextActivationV1, &'static str> {
        if top_creature_text(state, self.host) != Some(self) {
            return Err("full-text ability has a stale host or graveyard top");
        }
        if !crate::continuous_characteristics_v1::printed_abilities_active(state, self.host.source)
        {
            return Err("full-text ability was removed in a later layer");
        }
        let ability = selector.resolve(self.text_definition)?;
        let first_donated_slot = self.first_donated_slot(state);
        if selector.action_index(first_donated_slot).is_none() {
            return Err("full-text activation has no available action slot");
        }
        Ok(FrozenFullTextActivationV1 {
            host: self.host,
            donor: AbilitySourceContractV4::capture(state, self.donor.object),
            text_definition: self.text_definition,
            selector,
            ability,
            first_donated_slot,
        })
    }
}

fn first_slot_after_equipment(physical_definition: u16) -> u8 {
    let printed = CARD_DEFS[usize::from(physical_definition)]
        .activated_abilities
        .len();
    u8::try_from(printed.saturating_add(2)).unwrap_or(u8::MAX)
}

fn restricted_use_event(
    state: &GameState,
    host: AbilitySourceContractV4,
    donor: ObjectLinkV4,
    kind: AbilityKindV4,
    index: u8,
) -> CommittedEvent {
    CommittedEvent::FullTextRestrictedUseV1 {
        host: ObjectLinkV4 {
            object: host.source,
            zone_change_count: host.zone_change_count,
        },
        donor,
        turn: state.turn_key_v1(),
        active: state.active_player,
        kind,
        index,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum DonatedTriggerKindV1 {
    PalinchronEntered { entry: u64 },
    DevourerPower { observed_power: i32 },
    PalinchronEnteredAfterBatch { entry: u64, marker: u64 },
    KarmicGuideEntered { entry: u64 },
    KarmicGuideEnteredAfterBatch { entry: u64, marker: u64 },
    KarmicGuideEcho { upkeep: u64, control: u64 },
}

impl DonatedTriggerKindV1 {
    fn entry(text: FullTextV1, entry: u64, marker: Option<u64>) -> Option<Self> {
        match (text.definition().name, marker) {
            (p2c::PALINCHRON, None) => Some(Self::PalinchronEntered { entry }),
            (p2c::PALINCHRON, Some(marker)) => {
                Some(Self::PalinchronEnteredAfterBatch { entry, marker })
            }
            (p2c::KARMIC_GUIDE, None) => Some(Self::KarmicGuideEntered { entry }),
            (p2c::KARMIC_GUIDE, Some(marker)) => {
                Some(Self::KarmicGuideEnteredAfterBatch { entry, marker })
            }
            _ => None,
        }
    }

    fn target_spec(self) -> TargetSpec {
        match self {
            Self::KarmicGuideEntered { .. } | Self::KarmicGuideEnteredAfterBatch { .. } => {
                TargetSpec::CreatureCardInOwnGraveyard
            }
            _ => TargetSpec::None,
        }
    }
}

/// Both identities are captured by the native trigger producer. The donor
/// remains public historical text while the effect acts on the physical host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DonatedTriggerCaptureV1 {
    host: AbilitySourceContractV4,
    donor: AbilitySourceContractV4,
    kind: DonatedTriggerKindV1,
}

impl DonatedTriggerCaptureV1 {
    fn program(self, state: &GameState) -> Result<EffectOp, String> {
        crate::engine::validate_historical_ability_source_contract(state, self.host)?;
        crate::engine::validate_historical_ability_source_contract(state, self.donor)?;
        let donor = CARD_DEFS
            .get(usize::from(self.donor.card_def))
            .ok_or("full-text trigger donor definition is missing")?;
        if self.host.zone != Zone::Battlefield
            || self.donor.zone != Zone::Graveyard
            || self.donor.owner != self.host.controller
            || self.donor.source == self.host.source
            || !donor.capability.is_fully_supported()
            || donor.is_token
        {
            return Err("full-text trigger lost its original host or donor".into());
        }
        match self.kind {
            DonatedTriggerKindV1::PalinchronEntered { entry }
            | DonatedTriggerKindV1::PalinchronEnteredAfterBatch { entry, .. }
                if donor.name == p2c::PALINCHRON
                    && usize::try_from(entry).ok().is_some_and(|index| {
                        entry_source(state, index) == Some(self.host.source)
                    }) =>
            {
                Ok(p2c::palinchron_enters())
            }
            DonatedTriggerKindV1::KarmicGuideEntered { entry }
            | DonatedTriggerKindV1::KarmicGuideEnteredAfterBatch { entry, .. }
                if donor.name == p2c::KARMIC_GUIDE
                    && usize::try_from(entry).ok().is_some_and(|index| {
                        entry_source(state, index) == Some(self.host.source)
                    }) =>
            {
                Ok(p2c::guide_enters())
            }
            DonatedTriggerKindV1::DevourerPower { observed_power }
                if donor.name == p2c::PHYREXIAN_DEVOURER && observed_power >= 7 =>
            {
                Ok(p2c::devourer_sacrifice())
            }
            DonatedTriggerKindV1::KarmicGuideEcho { upkeep, control }
                if donor.name == p2c::KARMIC_GUIDE
                    && echo_custody(state, self.host, upkeep) == Some(control) =>
            {
                Ok(crate::eternal_premodern_p2d_v1::donated_guide_echo_trigger())
            }
            _ => Err("full-text trigger does not match its frozen donor and occurrence".into()),
        }
    }
}

fn entry_source(state: &GameState, index: usize) -> Option<ObjectId> {
    match state.engine.event_history.get(index)? {
        CommittedEvent::ZoneChange {
            object,
            from,
            to: Zone::Battlefield,
            ..
        } if *from != Zone::Battlefield => Some(*object),
        CommittedEvent::CreateToken { object, .. } => Some(*object),
        _ => None,
    }
}

/// A real control transfer matters even if the permanent currently has no
/// echo. This record describes only the public transfer, independent of its
/// printing, face and current donor.
pub(crate) fn record_control_gained(state: &mut GameState, object: ObjectId) {
    if !crate::graveyard_order_v1::enabled(state) {
        return;
    }
    let live = state.objects.get(object);
    state
        .engine
        .event_history
        .push(CommittedEvent::OrderedGraveyardControlGainedV1 {
            permanent: ObjectLinkV4 {
                object,
                zone_change_count: live.zone_change_count,
            },
            controller: live.controller,
        });
}

fn echo_custody(state: &GameState, host: AbilitySourceContractV4, upkeep: u64) -> Option<u64> {
    let upkeep = usize::try_from(upkeep).ok()?;
    let history = &state.engine.event_history;
    let player = host.controller;
    if history.get(upkeep) != Some(&CommittedEvent::UpkeepBegan { player }) {
        return None;
    }
    let link = ObjectLinkV4 {
        object: host.source,
        zone_change_count: host.zone_change_count,
    };
    for (index, event) in history[..upkeep].iter().enumerate().rev() {
        match *event {
            CommittedEvent::UpkeepBegan { player: previous } if previous == player => break,
            CommittedEvent::OrderedGraveyardControlGainedV1 {
                permanent,
                controller,
            } if permanent.object == host.source => {
                return (permanent == link && controller == player).then_some(index as u64);
            }
            CommittedEvent::ZoneChange {
                object, from, to, ..
            } if object == host.source => {
                return (from != Zone::Battlefield && to == Zone::Battlefield)
                    .then_some(index as u64);
            }
            CommittedEvent::CreateToken {
                object, controller, ..
            } if object == host.source => {
                return (host.zone_change_count == 0 && controller == player)
                    .then_some(index as u64);
            }
            _ => {}
        }
    }
    None
}

pub(crate) fn queue_upkeep_triggers(
    state: &mut GameState,
    player: PlayerId,
    upkeep: usize,
) -> Result<(), ObjectId> {
    if !crate::graveyard_order_v1::enabled(state) {
        return Ok(());
    }
    let upkeep = upkeep as u64;
    for source in state.players[player.index()].battlefield.clone() {
        let Some(text) = active_text(state, source).filter(|text| {
            text.definition().name == p2c::KARMIC_GUIDE
                && crate::continuous_characteristics_v1::printed_abilities_active(state, source)
        }) else {
            continue;
        };
        if let Some(control) = echo_custody(state, text.host, upkeep) {
            queue_donated_trigger(
                state,
                text,
                DonatedTriggerKindV1::KarmicGuideEcho { upkeep, control },
            )
            .map_err(|_| source)?;
        }
    }
    Ok(())
}

fn queue_donated_trigger(
    state: &mut GameState,
    text: FullTextV1,
    kind: DonatedTriggerKindV1,
) -> Result<(), String> {
    let captured = text.freeze_trigger(state, kind)?;
    let capture = u64::try_from(state.engine.event_history.len())
        .map_err(|_| "full-text trigger capture exceeds u64")?;
    state
        .engine
        .event_history
        .push(CommittedEvent::FullTextTriggerCapturedV1 {
            capture: Box::new(captured),
        });
    let target = kind.target_spec();
    crate::eternal_premodern_p1a_v1::queue_trigger(
        state,
        text.host,
        text.host.controller,
        EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::P2cV1(
            P2cOpV1::DonatedTriggerV1 { capture },
        ))),
        (target != TargetSpec::None).then_some(target),
    );
    Ok(())
}

pub(crate) fn record_entry_trigger(state: &mut GameState, index: usize) -> Result<(), ObjectId> {
    if !cfg!(feature = "eternal-fixtures") {
        return Ok(());
    }
    let Some(source) = entry_source(state, index) else {
        return Ok(());
    };
    if crate::graveyard_order_v1::quoting_entry_scene(state) {
        return Ok(());
    }
    let Some(text) = active_text(state, source) else {
        return Ok(());
    };
    if !crate::continuous_characteristics_v1::printed_abilities_active(state, source) {
        return Ok(());
    }
    let entry = u64::try_from(index).map_err(|_| source)?;
    let Some(kind) = DonatedTriggerKindV1::entry(text, entry, None) else {
        return Ok(());
    };
    if crate::graveyard_order_v1::defer_donated_entry(
        state,
        CommittedEvent::FullTextBatchEntryV1 {
            entry_history_index: entry,
            host: text.host,
            donor: text.donor,
            text_definition: text.text_definition,
        },
    )
    .map_err(|_| source)?
    {
        return Ok(());
    }
    queue_donated_trigger(state, text, kind).map_err(|_| source)
}

pub(crate) fn record_batch_entry_trigger(state: &mut GameState, marker: u64) -> Result<(), String> {
    let (entry, text) = batch_entry(state, marker)?;
    if active_text(state, text.host.source) != Some(text) {
        return Err("mixed entry lost the selected final graveyard text".into());
    }
    let kind = DonatedTriggerKindV1::entry(text, entry, Some(marker))
        .ok_or("mixed entry has no donated trigger for its selected text")?;
    queue_donated_trigger(state, text, kind)
}

fn batch_entry(state: &GameState, marker: u64) -> Result<(u64, FullTextV1), String> {
    let position = usize::try_from(marker).map_err(|_| "batch entry marker index overflow")?;
    let Some(&CommittedEvent::FullTextBatchEntryV1 {
        entry_history_index: entry,
        host,
        donor,
        text_definition,
    }) = state.engine.event_history.get(position)
    else {
        return Err("donated entry lost its original native batch marker".into());
    };
    validate_entry_adjacency(state, entry, marker, host)?;
    Ok((
        entry,
        FullTextV1 {
            host,
            donor,
            text_definition,
        },
    ))
}

fn validate_entry_adjacency(
    state: &GameState,
    entry: u64,
    index: u64,
    host: AbilitySourceContractV4,
) -> Result<(), String> {
    let after_copied_entry = entry.checked_add(2) == Some(index)
        && usize::try_from(index - 1).ok().is_some_and(|at| {
            matches!(state.engine.event_history.get(at),
                Some(CommittedEvent::CopiedTokenAbilitiesEnteredV1 {
                    source, creation_history_index, ..
                }) if *source == host && *creation_history_index == entry)
        });
    if entry.checked_add(1) != Some(index) && !after_copied_entry {
        return Err("full-text entry capture lost its native entry position".into());
    }
    Ok(())
}

pub(crate) fn queue_devourer_trigger(
    state: &mut GameState,
    source: ObjectId,
    observed_power: i32,
) -> Result<(), String> {
    let text = active_text(state, source).ok_or("Devourer trigger lost its text donor")?;
    queue_donated_trigger(
        state,
        text,
        DonatedTriggerKindV1::DevourerPower { observed_power },
    )
}

pub(crate) fn is_devourer_trigger(state: &GameState, source: ObjectId, effect: &EffectOp) -> bool {
    trigger_index(effect).is_some_and(|capture| {
        trigger_record(state, capture).is_ok_and(|row| {
            row.host.source == source
                && matches!(row.kind, DonatedTriggerKindV1::DevourerPower { .. })
        })
    })
}

fn trigger_index(effect: &EffectOp) -> Option<u64> {
    match effect {
        EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::P2cV1(
            P2cOpV1::DonatedTriggerV1 { capture },
        ))) => Some(*capture),
        _ => None,
    }
}

pub(crate) fn is_donated_trigger(item: &StackItem) -> bool {
    item.inline_effect
        .as_ref()
        .and_then(trigger_index)
        .is_some()
}

fn trigger_record(state: &GameState, index: u64) -> Result<DonatedTriggerCaptureV1, String> {
    let position = usize::try_from(index).map_err(|_| "full-text trigger index overflow")?;
    match state.engine.event_history.get(position) {
        Some(CommittedEvent::FullTextTriggerCapturedV1 { capture }) => {
            match capture.kind {
                DonatedTriggerKindV1::PalinchronEntered { entry }
                | DonatedTriggerKindV1::KarmicGuideEntered { entry } => {
                    validate_entry_adjacency(state, entry, index, capture.host)?
                }
                DonatedTriggerKindV1::PalinchronEnteredAfterBatch { entry, marker }
                | DonatedTriggerKindV1::KarmicGuideEnteredAfterBatch { entry, marker } => {
                    let (original_entry, text) = batch_entry(state, marker)?;
                    if marker >= index || entry != original_entry || capture.host != text.host
                        || capture.donor.source != text.donor.object
                        || capture.donor.zone_change_count != text.donor.zone_change_count
                        || capture.donor.card_def != text.text_definition
                        || state.engine.event_history.iter().filter(|event| {
                            matches!(event, CommittedEvent::FullTextTriggerCapturedV1 { capture }
                                if matches!(capture.kind,
                                    DonatedTriggerKindV1::PalinchronEnteredAfterBatch { marker: used, .. }
                                    | DonatedTriggerKindV1::KarmicGuideEnteredAfterBatch { marker: used, .. }
                                    if used == marker))
                        }).count() != 1
                    {
                        return Err("donated entry differs from the native selected text marker".into());
                    }
                }
                DonatedTriggerKindV1::DevourerPower { .. } => {}
                DonatedTriggerKindV1::KarmicGuideEcho { upkeep, .. } if upkeep < index => {}
                DonatedTriggerKindV1::KarmicGuideEcho { .. } => {
                    return Err("donated echo capture precedes its upkeep".into());
                }
            }
            Ok(**capture)
        }
        _ => Err("full-text trigger lost its native capture".into()),
    }
}

pub(crate) fn trigger_target_spec(
    state: &GameState,
    effect: &EffectOp,
) -> Result<Option<TargetSpec>, String> {
    trigger_index(effect)
        .map(|index| trigger_record(state, index).map(|capture| capture.kind.target_spec()))
        .transpose()
}

pub(crate) fn trigger_program(
    state: &GameState,
    item: &StackItem,
) -> Result<Option<(EffectOp, TargetSpec)>, String> {
    let Some(index) = item.inline_effect.as_ref().and_then(trigger_index) else {
        return Ok(None);
    };
    let captured = trigger_record(state, index)?;
    let target = captured.kind.target_spec();
    if !cfg!(feature = "eternal-fixtures")
        || item.kind != StackItemKind::TriggeredAbility
        || item.source != captured.host.source
        || item.controller != captured.host.controller
        || item.v4.ability_source_contract != Some(captured.host)
        || item.v4.source_contract.is_some()
        || item.v4.granted_by.is_some()
        || !item.v4.cauldron_grant.is_empty()
        || item.v4.activated_ability_index.is_some()
        || item.v4.optional_additional_cost_paid.is_some()
        || !item.v4.paid_cost_refs.is_empty()
        || item.v4.target_spec != (target != TargetSpec::None).then_some(target)
        || item.targets.len() != item.v4.target_contracts.len()
        || item.targets.len() > usize::from(target != TargetSpec::None)
        || item.is_copy
        || item.is_flashback
        || item.madness_offer
        || item.kicked
        || item.mode_chosen != 0
        || !item.discarded.is_empty()
        || !CARD_DEFS
            .get(usize::from(captured.host.card_def))
            .is_some_and(owns_definition)
    {
        return Err("full-text stack trigger changed its native producer".into());
    }
    captured
        .program(state)
        .map(|program| Some((program, target)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum FullTextAbilityV1 {
    Printed(u8),
    RetainedDiscard,
}

impl FullTextAbilityV1 {
    pub(crate) fn from_action_index(index: u8, first_donated_slot: u8) -> Option<Self> {
        match index {
            0 => Some(Self::RetainedDiscard),
            u8::MAX => None,
            _ => index.checked_sub(first_donated_slot).map(Self::Printed),
        }
    }

    /// Slot zero is retained discard. Donated indices follow existing grants.
    pub(crate) fn action_index(self, first_donated_slot: u8) -> Option<u8> {
        match self {
            Self::RetainedDiscard => Some(0),
            Self::Printed(index) => first_donated_slot
                .checked_add(index)
                .filter(|&slot| slot != u8::MAX),
        }
    }

    pub(crate) fn owns_payment(self, definition: &CardDef) -> bool {
        self.public_program(definition).is_some()
    }

    pub(crate) fn public_program(
        self,
        definition: &CardDef,
    ) -> Option<crate::premodern_ordered_public_v1::FullTextProgramV1> {
        use crate::premodern_ordered_public_v1::{DonatedActivationV1, FullTextProgramV1};

        match self {
            Self::RetainedDiscard => Some(FullTextProgramV1::RetainedDiscard),
            Self::Printed(printed_index) => {
                let effect = match (definition.name, printed_index) {
                    (p2c::PALINCHRON, 0) => DonatedActivationV1::PalinchronReturnHost,
                    (p2c::PHYREXIAN_DEVOURER, 0) => {
                        DonatedActivationV1::DevourerExileTopForCounters
                    }
                    (p2c::TRISKELION, 0) => DonatedActivationV1::TriskelionRemoveCounterForDamage,
                    (p2c::BATTLEFIELD_SCROUNGER, 0) => DonatedActivationV1::ScroungerThresholdPump,
                    ("Hermit Druid", 0) => DonatedActivationV1::HermitDruidRevealThroughBasic,
                    ("Psychatog", 0) => DonatedActivationV1::PsychatogDiscardPump,
                    ("Psychatog", 1) => DonatedActivationV1::PsychatogExilePump,
                    _ => return None,
                };
                Some(FullTextProgramV1::DonatedActivation {
                    printed_index,
                    effect,
                })
            }
        }
    }

    fn resolve(self, text_definition: u16) -> Result<ActivatedAbilityDef, &'static str> {
        let definition = CARD_DEFS
            .get(usize::from(text_definition))
            .ok_or("full-text definition is absent")?;
        match self {
            Self::RetainedDiscard => Ok(ActivatedAbilityDef {
                cost: &[CostComponent::Mana(Cost {
                    pips: &[],
                    generic: 2,
                    x_count: 0,
                })],
                target_spec: TargetSpec::None,
                effect: retained_discard,
                activation_zone: Zone::Battlefield,
                sorcery_speed_only: false,
                activation_target_filter: ActivationTargetFilter::TargetSpecOnly,
                max_activations_per_turn: None,
                face: None,
            }),
            Self::Printed(index) => {
                if !definition.capability.is_fully_supported() {
                    return Err("full-text definition is not fully supported");
                }
                definition
                    .activated_abilities
                    .get(usize::from(index))
                    .copied()
                    .filter(|ability| {
                        ability.activation_zone == Zone::Battlefield
                            && ability.face.is_none_or(|face| face == 0)
                    })
                    .ok_or("full-text selector is not a front-face battlefield ability")
            }
        }
    }
}

pub(crate) fn retained_discard() -> EffectOp {
    // Discard is the resolving effect, never an activation cost. The action
    // remains legal with an empty hand if the ordinary mana payment is legal.
    EffectOp::DiscardCards {
        player: PlayerRef::Controller,
        count: 1,
    }
}

/// Native prepayment value. An owning activation integration must authenticate
/// the announcement before attaching `stack_record` to the paid item. No
/// current source contract may substitute `text_definition` for
/// the host's layer-1 `card_def`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FrozenFullTextActivationV1 {
    host: AbilitySourceContractV4,
    donor: AbilitySourceContractV4,
    text_definition: u16,
    selector: FullTextAbilityV1,
    ability: ActivatedAbilityDef,
    first_donated_slot: u8,
}

impl FrozenFullTextActivationV1 {
    /// Preserve the announced producer and slot layout independently of
    /// later changes to the graveyard top and native grants.
    pub(crate) fn stack_record(self) -> FullTextActivationRecordV1 {
        FullTextActivationRecordV1(Some(FullTextActivationV1 {
            host: self.host,
            donor: self.donor,
            selector: self.selector,
            first_donated_slot: self.first_donated_slot,
        }))
    }
    pub(crate) fn action_index(self) -> u8 {
        self.selector
            .action_index(self.first_donated_slot)
            .expect("frozen full-text selector has a native action slot")
    }
    pub(crate) fn host(self) -> AbilitySourceContractV4 {
        self.host
    }

    pub(crate) fn text_definition(self) -> u16 {
        self.text_definition
    }

    pub(crate) fn selector(self) -> FullTextAbilityV1 {
        self.selector
    }

    /// CR 113.7a: do not read the current graveyard, source abilities, or
    /// source controller when resolving an already-announced ability.
    pub(crate) fn ability(self) -> ActivatedAbilityDef {
        self.ability
    }
}

/// Internal stack producer only. No current public stack format represents
/// both identities; consumers must call `refuse_legacy` before extraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullTextActivationV1 {
    host: AbilitySourceContractV4,
    donor: AbilitySourceContractV4,
    selector: FullTextAbilityV1,
    first_donated_slot: u8,
}

/// Empty contributes no bytes or Hash domain to preexisting state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FullTextActivationRecordV1(Option<FullTextActivationV1>);

impl FullTextActivationRecordV1 {
    pub fn is_empty(&self) -> bool {
        self.0.is_none()
    }

    /// The native pending owner checks the exact current donor before costs
    /// commit. The paid stack owner validates historical provenance instead.
    pub(crate) fn require_current(self, state: &GameState) -> Result<(), String> {
        let Some(record) = self.0 else {
            return Ok(());
        };
        let text = top_creature_text(state, record.host)
            .ok_or("full-text activation lost its live top creature")?;
        if !record.selector.owns_payment(text.definition()) {
            return Err("full-text activation has no exact native payment owner".into());
        }
        let current = text
            .freeze_activated(state, record.selector)
            .map_err(str::to_owned)?;
        if current.stack_record() != self
            || !text.activation_allowed(state, record.selector, current.ability())
        {
            return Err("full-text activation producer changed before payment".into());
        }
        Ok(())
    }

    pub(crate) fn record_restricted_use(self, state: &mut GameState, ability: ActivatedAbilityDef) {
        if let Some(record) = self
            .0
            .filter(|_| ability.max_activations_per_turn.is_some())
        {
            let FullTextAbilityV1::Printed(printed) = record.selector else {
                return;
            };
            let donor = ObjectLinkV4 {
                object: record.donor.source,
                zone_change_count: record.donor.zone_change_count,
            };
            let event =
                restricted_use_event(state, record.host, donor, AbilityKindV4::Activated, printed);
            state.engine.event_history.push(event);
        }
    }

    pub(crate) fn resolve(
        self,
        state: &GameState,
        host: AbilitySourceContractV4,
        action_index: u8,
        granted_by: Option<AbilitySourceContractV4>,
        cauldron: crate::cauldron_grants_v1::CauldronGrantRecordV1,
    ) -> Result<Option<ActivatedAbilityDef>, String> {
        let Some(record) = self.0 else {
            return Ok(None);
        };
        if !cfg!(feature = "eternal-fixtures")
            || record.host != host
            || record.selector.action_index(record.first_donated_slot) != Some(action_index)
            || host.zone != Zone::Battlefield
            || record.donor.zone != Zone::Graveyard
            || record.donor.owner != host.controller
            || record.donor.source == host.source
            || granted_by.is_some()
            || !cauldron.is_empty()
        {
            return Err("full-text stack producer is inconsistent".into());
        }
        crate::engine::validate_historical_ability_source_contract(state, host)?;
        if record.first_donated_slot < first_slot_after_equipment(host.card_def) {
            return Err("full-text stack producer overlaps physical or Equipment slots".into());
        }
        crate::engine::validate_historical_ability_source_contract(state, record.donor)?;
        let live_donor = state.objects.get(record.donor.source);
        let donor = CARD_DEFS
            .get(usize::from(record.donor.card_def))
            .ok_or("full-text stack definition is absent")?;
        if donor.is_token
            || live_donor.v4.is_token
            || live_donor.spell_copy_origin.is_some()
            || !donor.types.contains(&CardType::Creature)
            || !record.selector.owns_payment(donor)
        {
            return Err("full-text stack producer is not a creature card".into());
        }
        // Historical identity only: the donor can be exiled and the source
        // can change zones or controllers before the ability resolves.
        record
            .selector
            .resolve(record.donor.card_def)
            .map(Some)
            .map_err(str::to_owned)
    }
}

impl Hash for FullTextActivationRecordV1 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        if let Some(record) = self.0 {
            b"full-text-activation/v1".hash(state);
            record.hash(state);
        }
    }
}

pub(crate) fn public_producers(
    state: &GameState,
) -> Result<Vec<crate::premodern_ordered_public_v1::FullTextProducerV1>, String> {
    use crate::premodern_ordered_public_v1::ProducerStageV1 as Stage;

    let mut rows = Vec::new();
    if let Some(pending) = &state.engine.pending_activation {
        if let Some(record) = pending.full_text_activation_v1.0 {
            crate::engine::validate_pending_activation(state, pending)?;
            rows.push(activation_producer(
                record,
                Stage::ActivationAnnouncing,
                pending.ability_index,
            )?);
        }
    }
    for (ordinal, pending) in state.engine.pending_triggers.iter().enumerate() {
        if let Some(index) = trigger_index(&pending.effect) {
            crate::engine::validate_pending_trigger_identity(state, pending)?;
            let record = trigger_record(state, index)?;
            rows.push(trigger_producer(record, Stage::TriggerPending { ordinal }));
        }
    }
    for (position, item) in state.stack.iter().enumerate() {
        if !item.v4.full_text_activation_v1.is_empty() || is_donated_trigger(item) {
            crate::engine::validated_stack_item_target_spec(item, state)?;
            rows.push(stack_producer(state, item, Stage::Stack { position })?);
        }
    }
    if let Some(pending) = &state.engine.pending_effect {
        let item = &pending.resolving_item;
        if !item.v4.full_text_activation_v1.is_empty() || is_donated_trigger(item) {
            crate::effect::validate_pending_effect_choice(state)?;
            rows.push(stack_producer(state, item, Stage::Resolving)?);
        }
    }
    Ok(rows)
}

fn activation_producer(
    record: FullTextActivationV1,
    stage: crate::premodern_ordered_public_v1::ProducerStageV1,
    action_index: u8,
) -> Result<crate::premodern_ordered_public_v1::FullTextProducerV1, String> {
    use crate::premodern_ordered_public_v1::{captured_card, FullTextProducerV1};

    let donor = CARD_DEFS
        .get(usize::from(record.donor.card_def))
        .ok_or("full-text producer donor printing absent")?;
    let program = record
        .selector
        .public_program(donor)
        .ok_or("full-text activation has no public program")?;
    Ok(FullTextProducerV1 {
        stage,
        physical: captured_card(record.host),
        text_donor: captured_card(record.donor),
        native_ability_index: Some(action_index),
        program,
    })
}

fn trigger_producer(
    record: DonatedTriggerCaptureV1,
    stage: crate::premodern_ordered_public_v1::ProducerStageV1,
) -> crate::premodern_ordered_public_v1::FullTextProducerV1 {
    use crate::premodern_ordered_public_v1::{
        captured_card, FullTextProducerV1, FullTextProgramV1,
    };

    let program = match record.kind {
        DonatedTriggerKindV1::PalinchronEntered { .. }
        | DonatedTriggerKindV1::PalinchronEnteredAfterBatch { .. } => {
            FullTextProgramV1::PalinchronUntapLands
        }
        DonatedTriggerKindV1::KarmicGuideEntered { .. }
        | DonatedTriggerKindV1::KarmicGuideEnteredAfterBatch { .. } => {
            FullTextProgramV1::KarmicGuideReturnCreature
        }
        DonatedTriggerKindV1::KarmicGuideEcho { .. } => FullTextProgramV1::KarmicGuideEcho,
        DonatedTriggerKindV1::DevourerPower { .. } => FullTextProgramV1::DevourerSacrifice,
    };
    FullTextProducerV1 {
        stage,
        physical: captured_card(record.host),
        text_donor: captured_card(record.donor),
        native_ability_index: None,
        program,
    }
}

fn stack_producer(
    state: &GameState,
    item: &StackItem,
    stage: crate::premodern_ordered_public_v1::ProducerStageV1,
) -> Result<crate::premodern_ordered_public_v1::FullTextProducerV1, String> {
    if let Some(record) = item.v4.full_text_activation_v1.0 {
        let index = item
            .v4
            .activated_ability_index
            .ok_or("full-text stack ability index absent")?;
        return activation_producer(record, stage, index);
    }
    let index = item
        .inline_effect
        .as_ref()
        .and_then(trigger_index)
        .ok_or("full-text stack trigger capture absent")?;
    Ok(trigger_producer(trigger_record(state, index)?, stage))
}

pub(crate) fn refuse_legacy(state: &GameState) -> Result<(), String> {
    crate::graveyard_order_v1::refuse_legacy(state)?;
    if state.engine.event_history.has_any(&[
        crate::event_history_v1::EventKindV1::FullTextTriggerCapturedV1,
        crate::event_history_v1::EventKindV1::FullTextBatchEntryV1,
        crate::event_history_v1::EventKindV1::FullTextRestrictedUseV1,
        crate::event_history_v1::EventKindV1::OrderedGraveyardControlGainedV1,
    ]) {
        return Err(
            "full-text trigger producers require a complete public consumer successor".into(),
        );
    }
    if cfg!(feature = "eternal-fixtures")
        && state
            .players
            .iter()
            .flat_map(|player| player.battlefield.iter().copied())
            .any(|source| {
                state.objects.try_get(source).is_some_and(|object| {
                    object.v4.face_down_v1.is_none()
                        && CARD_DEFS
                            .get(usize::from(object.card_def))
                            .is_some_and(owns_definition)
                })
            })
    {
        return Err(
            "full-text characteristics require a complete public consumer successor".into(),
        );
    }
    let selected = state
        .stack
        .iter()
        .any(|item| !item.v4.full_text_activation_v1.is_empty() || is_donated_trigger(item))
        || state
            .engine
            .pending_triggers
            .iter()
            .any(|trigger| trigger_index(&trigger.effect).is_some())
        || state
            .engine
            .pending_activation
            .as_ref()
            .is_some_and(|pending| !pending.full_text_activation_v1.is_empty())
        || state.engine.pending_effect.as_ref().is_some_and(|pending| {
            !pending.resolving_item.v4.full_text_activation_v1.is_empty()
                || is_donated_trigger(&pending.resolving_item)
        });
    if selected {
        Err("full-text ability producers require a complete public consumer successor".into())
    } else {
        Ok(())
    }
}

#[cfg(all(test, feature = "eternal-fixtures"))]
mod tests;
