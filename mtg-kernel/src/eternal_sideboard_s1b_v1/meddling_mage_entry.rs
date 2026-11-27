//! Native as-enters name choice for Meddling Mage.
use super::{nameable_nonland, s1b_state, MEDDLING_MAGE};
use crate::card_def::{card_id_by_name, CardDef, CardType, CARD_DEFS};
use crate::engine::Decision;
use crate::event::{CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId, StackItemId};
use crate::linked_aura_operation_v1::{GeneralAnswerV36, GeneralChoiceV36, OperationV1};
use crate::state::{AbilitySourceContractV4, GameState, ObjectLinkV4, Zone};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::hash::{Hash, Hasher};

pub(crate) mod show_and_tell;
pub(crate) mod vial;

pub(crate) fn owns_definition(id: u16) -> bool {
    cfg!(feature = "eternal-fixtures")
        && card_id_by_name(MEDDLING_MAGE) == Some(id)
        && CARD_DEFS[usize::from(id)].is_executable()
}

pub(crate) fn potentially_enters(state: &GameState) -> bool {
    cfg!(feature = "eternal-fixtures")
        && state
            .objects
            .iter()
            .any(|(_, object)| owns_definition(object.card_def))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChoiceV1 {
    pub(crate) ordinal: u32,
    pub(crate) proposal: ProposedEvent,
    pub(crate) controller: PlayerId,
    pub(crate) card_def: u16,
    pub(crate) original: Option<AbilitySourceContractV4>,
    pub(crate) suppressed: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) names_v2: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) metamorph_copy: Option<MetamorphCopyV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetamorphCopyV1 {
    pub(crate) physical: AbilitySourceContractV4,
    pub(crate) copied: AbilitySourceContractV4,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) values: Option<crate::copy_entry_v1::CopiableValuesV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) entry_answer: Option<crate::copy_entry_v1::EntryAnswer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) prepared_at: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) discarded_at: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cast_at: Option<usize>,
    pub(crate) stack_item: StackItemId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reanimated_by: Option<AbilitySourceContractV4>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    vial: Option<vial::CopyOriginV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    show_and_tell: Option<show_and_tell::CopyOriginV1>,
}

impl Hash for MetamorphCopyV1 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.physical.hash(state);
        self.copied.hash(state);
        if let Some(at) = self.prepared_at {
            b"noncast-copy-prepared/v1".hash(state);
            at.hash(state);
        }
        if let Some(at) = self.discarded_at {
            b"noncast-copy-discard/v1".hash(state);
            at.hash(state);
        }
        if let Some(answer) = self.entry_answer {
            b"metamorph-entry-answer/v1".hash(state);
            answer.hash(state);
        }
        if let Some(values) = &self.values {
            b"metamorph-copy-values/v1".hash(state);
            values.hash(state);
        }
        if let Some(cast_at) = self.cast_at {
            cast_at.hash(state);
        }
        self.stack_item.hash(state);
        if let Some(source) = self.reanimated_by {
            b"metamorph-reanimate-copy/v1".hash(state);
            source.hash(state);
        }
        if let Some(vial) = self.vial {
            b"metamorph-vial-copy/v1".hash(state);
            vial.hash(state);
        }
        if let Some(show) = self.show_and_tell {
            b"metamorph-show-and-tell-copy/v1".hash(state);
            show.hash(state);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoncastCopyChoiceV1 {
    pub(crate) physical: AbilitySourceContractV4,
    pub(crate) producer: AbilitySourceContractV4,
    pub(crate) candidates: Vec<AbilitySourceContractV4>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub(crate) values: crate::copy_entry_v1::CandidateValues,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cast_at: Option<usize>,
    pub(crate) stack_item: StackItemId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    vial: Option<vial::CopyOriginV1>,
}

pub(crate) fn physical_copy_entrant(state: &GameState, object: ObjectId, zone: Zone) -> bool {
    cfg!(feature = "eternal-fixtures")
        && state.objects.try_get(object).is_some_and(|live| {
            live.zone == zone
                && zone != Zone::Stack
                && zone != Zone::Battlefield
                && live.v4.face_index == 0
                && !live.v4.is_token
                && live.v4.face_down_v1.is_none()
                && live.spell_copy_origin.is_none()
                && card_id_by_name("Phyrexian Metamorph") == Some(live.card_def)
        })
}

pub(crate) fn resolving_copy_route(item: &crate::state::StackItem) -> Option<(Zone, bool)> {
    let source = item.v4.source_contract?;
    if item.kind != crate::state::StackItemKind::Spell || item.is_copy || source.zone != Zone::Stack
    {
        return None;
    }
    match crate::copy_entry_v1::registered_route(source.card_def)? {
        crate::copy_entry_v1::Route::ResolvingSpell { from, targeted, .. } => {
            Some((from, targeted))
        }
        _ => None,
    }
}

pub(crate) fn resolving_copy_waiting(state: &GameState, item: &crate::state::StackItem) -> bool {
    let Some((zone, targeted)) = resolving_copy_route(item) else {
        return false;
    };
    if targeted {
        item.targets.iter().any(|target| matches!(target, crate::state::Target::Object(object) if physical_copy_entrant(state, *object, zone)))
    } else {
        state
            .objects
            .iter()
            .any(|(object, _)| physical_copy_entrant(state, object, zone))
    }
}

impl NoncastCopyChoiceV1 {
    pub(crate) fn capture(
        state: &GameState,
        ctx: &crate::effect::ExecCtx,
        object: ObjectId,
        item: &crate::state::StackItem,
    ) -> Result<Option<Self>, String> {
        let (zone, targeted) = resolving_copy_route(item).ok_or("unknown copy-entry producer")?;
        if !physical_copy_entrant(state, object, zone)
            || targeted
                && !ctx.targets.iter().enumerate().any(|(i, target)| {
                    *target == crate::state::Target::Object(object)
                        && ctx.target_incarnation_matches(i, state)
                })
            || ctx.stack_item_id != Some(item.v4.stack_item_id)
            || ctx.source != item.source
            || ctx.controller != item.controller
            || ctx.targets != item.targets
            || ctx.target_contracts != item.v4.target_contracts
        {
            return Err("noncast Metamorph lost its actual paid Reanimate target".into());
        }
        let physical = AbilitySourceContractV4::capture(state, object);
        let producer = AbilitySourceContractV4::capture(state, ctx.source);
        let player = crate::copy_entry_v1::entering_controller(producer, physical);
        let (candidates, values) = crate::copy_entry_v1::capture_menu(state, object, player)?;
        if candidates.is_empty() {
            return Ok(None);
        }
        let choice = Self {
            physical,
            producer,
            candidates,
            values,
            cast_at: Some(paid_cast_index(state, item.v4.stack_item_id)?),
            stack_item: item.v4.stack_item_id,
            vial: None,
        };
        if !choice.copy(choice.candidates[0]).cast_matches(state) {
            return Err("noncast Metamorph lost its actual paid Reanimate publication".into());
        }
        Ok(Some(choice))
    }

    pub(crate) fn chooser(&self) -> PlayerId {
        crate::copy_entry_v1::entering_controller(self.producer, self.physical)
    }

    pub(crate) fn decision_source(&self) -> ObjectId {
        if self.vial.is_some() {
            self.producer.source
        } else {
            self.physical.source
        }
    }

    pub(crate) fn copy(&self, copied: AbilitySourceContractV4) -> MetamorphCopyV1 {
        MetamorphCopyV1 {
            physical: self.physical,
            copied,
            prepared_at: None,
            discarded_at: None,
            entry_answer: None,
            values: self.values.get(&copied.source).cloned(),
            cast_at: self.cast_at,
            stack_item: self.stack_item,
            reanimated_by: self.cast_at.map(|_| self.producer),
            vial: self.vial,
            show_and_tell: None,
        }
    }
}

fn paid_cast_index(state: &GameState, stack_item: StackItemId) -> Result<usize, String> {
    let mut casts = state
        .engine
        .event_history
        .scan_for_v1(&[crate::event_history_v1::EventKindV1::PublishedCastIdentityV1])
        .iter()
        .enumerate()
        .filter_map(|(index, event)| match event {
            CommittedEvent::PublishedCastIdentityV1 { receipt }
                if receipt.stack_item_id == stack_item =>
            {
                Some(index + 1)
            }
            _ => None,
        });
    let cast = casts
        .next()
        .ok_or("Metamorph copy has no paid cast receipt")?;
    if casts.next().is_some() {
        return Err("Metamorph copy has duplicate cast receipts".into());
    }
    Ok(cast)
}

fn cast_receipt_matches(
    state: &GameState,
    cast_at: usize,
    stack_item: StackItemId,
    source: AbilitySourceContractV4,
) -> bool {
    use crate::cast_identity_v1::PublicIdentityV1;
    let history = &state.engine.event_history;
    let Some(cast_receipt) = cast_at.checked_sub(1).and_then(|at| history.get(at)) else {
        return false;
    };
    source.zone == Zone::Stack
        && matches!(cast_receipt, CommittedEvent::PublishedCastIdentityV1 { receipt }
            if receipt.stack_item_id == stack_item
                && receipt.spell_cast_history_index == cast_at as u64
                && receipt.spell == source.source && receipt.owner == source.owner
                && receipt.controller == source.controller
                && receipt.stack_zone_change_count == source.zone_change_count
                && receipt.origin_zone_change_count.checked_add(1) == Some(source.zone_change_count)
                && receipt.public_identity == (PublicIdentityV1::Known { card_def: source.card_def }))
        && matches!(history.get(cast_at), Some(CommittedEvent::SpellCast { spell, controller })
            if *spell == source.source && *controller == source.controller)
}

impl MetamorphCopyV1 {
    pub(crate) fn refresh_show_origin(&mut self, state: &GameState) {
        if let Some(origin) = &mut self.show_and_tell {
            origin.chosen_at = state.engine.event_history.len();
        }
    }

    pub(crate) fn capture(
        state: &GameState,
        ctx: &crate::effect::ExecCtx,
        selected: crate::effect::EffectObjectBinding,
    ) -> Result<Option<Self>, String> {
        let copied = state.objects.get(selected.object);
        let physical = state.objects.get(ctx.source);
        let Some(stack_item) = ctx.stack_item_id else {
            return Ok(None);
        };
        if !owns_definition(copied.card_def)
            || copied.zone != Zone::Battlefield
            || copied.v4.face_down_v1.is_some()
            || copied.v4.face_index != 0
            || copied.zone_change_count != selected.expected_zone_change_count
            || physical.zone != Zone::Stack
            || physical.spell_copy_origin.is_some()
            || physical.v4.face_down_v1.is_some()
            || physical.v4.is_token
            || physical
                .v4
                .spell_cast_origin
                .is_none_or(|origin| origin.finalized_method.is_none())
            || physical.controller != ctx.controller
            || card_id_by_name("Phyrexian Metamorph") != Some(physical.card_def)
        {
            return Ok(None);
        }
        let cast_at = paid_cast_index(state, stack_item)?;
        let copy = Self {
            physical: AbilitySourceContractV4::capture(state, ctx.source),
            copied: AbilitySourceContractV4::capture(state, selected.object),
            prepared_at: None,
            discarded_at: None,
            entry_answer: None,
            values: None,
            cast_at: Some(cast_at),
            stack_item,
            reanimated_by: None,
            vial: None,
            show_and_tell: None,
        };
        if !copy.cast_matches(state) {
            return Err("Metamorph copy changed its paid physical cast".into());
        }
        Ok(Some(copy))
    }

    /// Earlier Reanimate and Vial menus had no separate choice receipt for these
    /// ordinary forms. Keep their byte streams: substituting an equally legal
    /// indistinguishable donor can describe another valid legacy history.
    fn needs_prepared_receipt(&self) -> bool {
        if self.physical.zone == Zone::Stack || self.show_and_tell.is_some() {
            return false;
        }
        let name = CARD_DEFS
            .get(usize::from(self.copied.card_def))
            .map(|d| d.name);
        self.values.is_some()
            || self.entry_answer.is_some()
            || !(matches!(name, Some("Meddling Mage" | "Wall of Blossoms"))
                || self.vial.is_some() && matches!(name, Some("Standstill" | "Aether Vial")))
    }

    pub(crate) fn record_prepared(&mut self, state: &mut GameState) {
        if self.needs_prepared_receipt() {
            self.prepared_at = Some(state.engine.event_history.len());
            record(state, JournalV1::NoncastCopyPrepared { copy: self.clone() });
        }
    }

    fn cast_matches(&self, state: &GameState) -> bool {
        let Some(cast_at) = self.cast_at.filter(|_| self.vial.is_none()) else {
            return false;
        };
        let (source, expected, entrant_zone) = match (self.reanimated_by, self.show_and_tell) {
            (Some(source), None) => {
                let Some(crate::copy_entry_v1::Route::ResolvingSpell { from, .. }) =
                    crate::copy_entry_v1::registered_route(source.card_def)
                else {
                    return false;
                };
                (source, CARD_DEFS[usize::from(source.card_def)].name, from)
            }
            (None, Some(show)) => (show.producer, "Show and Tell", Zone::Hand),
            (None, None) => (self.physical, "Phyrexian Metamorph", Zone::Stack),
            (Some(_), Some(_)) => return false,
        };
        card_id_by_name(expected) == Some(source.card_def)
            && card_id_by_name("Phyrexian Metamorph") == Some(self.physical.card_def)
            && self.physical.zone == entrant_zone
            && cast_receipt_matches(state, cast_at, self.stack_item, source)
    }

    fn copied_incarnation_at_entry(&self, state: &GameState, entry: usize) -> bool {
        let Some(live) = state.objects.try_get(self.copied.source) else {
            return false;
        };
        if live.owner != self.copied.owner || live.zone_change_count < self.copied.zone_change_count
        {
            return false;
        }
        let later =
            state.engine.event_history[entry + 1..]
                .iter()
                .filter_map(|event| match event {
                    CommittedEvent::ZoneChange { object, from, .. }
                        if *object == self.copied.source =>
                    {
                        Some(*from)
                    }
                    _ => None,
                });
        let expected_changes = live.zone_change_count - self.copied.zone_change_count;
        let mut changes = later.peekable();
        changes.peek().copied().unwrap_or(live.zone) == Zone::Battlefield
            && changes.count() == expected_changes as usize
            && (live.card_def == self.copied.card_def
                || crate::standard_cards_v1::had_card_def(
                    state,
                    self.copied.source,
                    self.copied.zone_change_count,
                    self.copied.card_def,
                ))
    }

    fn is_discard_companion_event(ev: &CommittedEvent, discard_at: usize) -> bool {
        matches!(
            ev,
            CommittedEvent::TinybonesV72 { journal }
                if matches!(
                    journal.as_ref(),
                    crate::engine::tinybones_v72::JournalV72::Discarded(record)
                        if record.move_history_index == discard_at as u64
                )
        )
    }

    fn discard_matches(&self, state: &GameState, entry: usize, controller: PlayerId) -> bool {
        use crate::copy_entry_v1::EntryAnswer;
        let Some(EntryAnswer::Discard(selected)) = self.entry_answer else {
            return self.discarded_at.is_none();
        };
        let (Some(land), Some(at)) = (selected, self.discarded_at) else {
            return false;
        };
        let Some(live) = state.objects.try_get(land.source) else {
            return false;
        };
        let history = &state.engine.event_history;
        // CR 614.12: positively bind the Mox Diamond land discard to this exact
        // producer's resolution rather than an earlier unrelated discard while
        // the producer sat on the stack.
        let receipt_at = if let Some(show) = self.show_and_tell {
            let Some(cast_at) = self.cast_at else {
                return false;
            };
            if cast_at >= at || at >= show.chosen_at || show.chosen_at >= entry {
                return false;
            }
            show.chosen_at
        } else if let Some(vial) = self.vial {
            let Some(prepared_at) = self.prepared_at else {
                return false;
            };
            if vial.chosen_at.checked_add(1) != Some(at)
                || at >= prepared_at
                || prepared_at.checked_add(1) != Some(entry)
            {
                return false;
            }
            prepared_at
        } else {
            let (Some(cast_at), Some(prepared_at)) = (self.cast_at, self.prepared_at) else {
                return false;
            };
            if cast_at >= at || at >= prepared_at || prepared_at.checked_add(1) != Some(entry) {
                return false;
            }
            prepared_at
        };
        if land.zone != Zone::Hand
            || land.owner != controller
            || land.controller != controller
            || live.owner != land.owner
            || !CARD_DEFS
                .get(usize::from(land.card_def))
                .is_some_and(|def| def.has_type(CardType::Land))
            || !crate::engine::source_card_def_matches(state, land, live.card_def)
            || !matches!(
                history.get(at),
                Some(CommittedEvent::ZoneChange {
                    object,
                    from: Zone::Hand,
                    to: Zone::Graveyard | Zone::Exile,
                    controller_before,
                }) if *object == land.source && *controller_before == controller
            )
        {
            return false;
        }
        // Require the exact `CopyDiscardPaid` provenance receipt committed by
        // `pay_copy_entry_cost` immediately before the preparation / Show and
        // Tell choice receipt, with only companion receipts for this exact
        // discard between the land move and `paid_at`.
        let Some(paid_at) = receipt_at.checked_sub(1) else {
            return false;
        };
        if self.stack_item.0 == 0
            || self.stack_item.0 > state.engine.next_stack_item_id
            || at >= paid_at
            || !matches!(
                history.get(paid_at),
                Some(CommittedEvent::CopyEntryProvenanceV1 { journal })
                    if matches!(
                        journal.as_ref(),
                        crate::copy_entry_v1::ProvenanceV1::CopyDiscardPaid {
                            stack_item,
                            physical,
                            land: paid_land,
                            discard_at,
                        } if *stack_item == self.stack_item
                            && *physical == self.physical
                            && *paid_land == land
                            && *discard_at == at
                    )
            )
            || history.iter().enumerate().any(|(i, ev)| {
                i != paid_at
                    && matches!(
                        ev,
                        CommittedEvent::CopyEntryProvenanceV1 { journal }
                            if matches!(
                                journal.as_ref(),
                                crate::copy_entry_v1::ProvenanceV1::CopyDiscardPaid {
                                    stack_item,
                                    physical,
                                    discard_at,
                                    ..
                                } if (*stack_item == self.stack_item && *physical == self.physical)
                                    || *discard_at == at
                            )
                    )
            })
            || !history[at + 1..paid_at]
                .iter()
                .all(|ev| Self::is_discard_companion_event(ev, at))
        {
            return false;
        }
        let changes = history[at..]
            .iter()
            .filter(
                |ev| matches!(ev, CommittedEvent::ZoneChange { object, .. } if *object == land.source),
            )
            .count();
        live.zone_change_count
            .checked_sub(land.zone_change_count)
            .is_some_and(|delta| delta as usize == changes)
    }

    fn entrant_matches(&self, entrant: AbilitySourceContractV4) -> bool {
        let controller = if let Some(vial) = self.vial {
            vial.source.controller
        } else if let Some(source) = self.reanimated_by {
            crate::copy_entry_v1::entering_controller(source, self.physical)
        } else if self.show_and_tell.is_some() {
            self.physical.owner
        } else {
            self.physical.controller
        };
        self.copied.zone == Zone::Battlefield
            && self.copied.source != self.physical.source
            && (self.physical.zone != Zone::Stack || owns_definition(self.copied.card_def))
            && entrant.source == self.physical.source
            && entrant.card_def == self.copied.card_def
            && entrant.zone == Zone::Battlefield
            && entrant.controller == controller
            && entrant.owner == self.physical.owner
            && self.physical.zone_change_count.checked_add(1) == Some(entrant.zone_change_count)
    }
}

impl Hash for ChoiceV1 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        serde_json::to_vec(self)
            .expect("native Meddling Mage entry choice serializes")
            .hash(state);
    }
}

pub(crate) fn choice(
    state: &GameState,
    proposal: &ProposedEvent,
    ordinal: u32,
    names_v2: bool,
) -> Option<ChoiceV1> {
    let mut metamorph_copy = None;
    let (controller, card_def, original) = match proposal {
        ProposedEvent::ZoneChange(change) if change.to_zone == Zone::Battlefield => {
            let object = state.objects.try_get(change.object)?;
            metamorph_copy =
                crate::linked_aura_operation_v1::metamorph_mage_copy(state, change.object);
            let card_def = metamorph_copy
                .as_ref()
                .map_or(object.card_def, |copy| copy.copied.card_def);
            if object.zone == Zone::Battlefield
                || !owns_definition(card_def)
                || change.battlefield_manifested_v1
                || change.battlefield_face_index.is_some_and(|face| face != 0)
                || (object.zone == Zone::Stack && object.v4.face_down_v1.is_some())
            {
                return None;
            }
            let mut final_change = change.clone();
            crate::eternal_sideboard_s1a_v1::statics::replace_zone_change(state, &mut final_change);
            if final_change.to_zone != Zone::Battlefield {
                return None;
            }
            let controller =
                change
                    .battlefield_controller
                    .unwrap_or(if object.zone == Zone::Stack {
                        object.controller
                    } else {
                        object.owner
                    });
            (
                controller,
                card_def,
                Some(AbilitySourceContractV4::capture(state, change.object)),
            )
        }
        ProposedEvent::CreateToken(token)
            if owns_definition(token.token_def)
                && token.entry_face_index.is_none_or(|face| face == 0) =>
        {
            (token.controller, token.token_def, None)
        }
        _ => return None,
    };
    Some(ChoiceV1 {
        ordinal,
        proposal: proposal.clone(),
        controller,
        card_def,
        original,
        names_v2,
        metamorph_copy,
        suppressed: crate::eternal_legacy_e7a_v1::active_dress_down_timestamp(
            state,
            original.map(|source| source.source),
        )
        .is_some(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointV1 {
    pub(crate) original: Box<GameState>,
    pub(crate) operation: OperationV1,
    pub(crate) answers: Vec<GeneralAnswerV36>,
    // A private hand answer has no visible world change before the reveal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) confirmed_show_answers_sha256: Option<String>,
    pub(crate) choice: GeneralChoiceV36,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) operation_history_index: Option<u32>,
    pub(crate) used_banner: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum JournalV1 {
    Started {
        operation: Box<OperationV1>,
        original_sha256: String,
    },
    Named {
        operation: u32,
        chosen: u32,
        entry_history_index: u32,
        source: AbilitySourceContractV4,
        named: NameV1,
    },
    Completed {
        operation: u32,
    },
    Chosen {
        operation: u32,
        choice: Box<ChoiceV1>,
        group: u16,
        index: u16,
    },
    StartedWithNamesV2 {
        operation: Box<OperationV1>,
        original_sha256: String,
    },
    MetamorphCopied {
        entry_history_index: usize,
        entrant: AbilitySourceContractV4,
        copy: MetamorphCopyV1,
    },
    VialPaid {
        source: AbilitySourceContractV4,
        item: StackItemId,
        tap_at: usize,
    },
    VialMetamorphChosen {
        paid_at: usize,
        physical: AbilitySourceContractV4,
    },
    ShowAndTellMetamorphChosen {
        copy: MetamorphCopyV1,
    },
    NoncastCopyPrepared {
        copy: MetamorphCopyV1,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum NameV1 {
    Front(u16),
    Face(PrintedFaceV1),
}

impl Hash for NameV1 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Self::Front(card) => card.hash(state),
            Self::Face(face) => {
                b"meddling-mage-face/v1".hash(state);
                face.hash(state);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrintedFaceV1 {
    card: u16,
    face: FaceV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum FaceV1 {
    LeftHalf,
    RightHalf,
    AlternateSpell,
    Back,
}

pub(super) fn split_names(def: &CardDef) -> Option<(&'static str, &'static str)> {
    def.name.split_once(" // ").filter(|_| {
        crate::eternal_premodern_p1a_v1::statics::split_card(def.name)
            || crate::standard_cards_v1::room_cast_mana_value(
                def,
                crate::state::CastMethodV4::Normal,
            )
            .is_some()
    })
}

pub(super) fn alternate_spell_name(def: &CardDef) -> Option<&'static str> {
    if crate::eternal_sideboard_s1d_v1::statics::is_overload(def.name) {
        return None;
    }
    if let Some(adventure) = &def.adventure {
        return (!adventure.types.contains(&CardType::Land)).then_some(adventure.name);
    }
    // OmenDef has no name field; map the generated Omen cards by name.
    if def.omen.is_some() {
        return match def.name {
            "Sagu Wildling" => Some("Roost Seek"),
            "Stormshriek Feral" => Some("Flush Out"),
            _ => None,
        };
    }
    None
}

impl NameV1 {
    pub(super) fn printed_name(self) -> Option<&'static str> {
        let (card, face) = match self {
            Self::Front(card) => (card, None),
            Self::Face(PrintedFaceV1 { card, face }) => (card, Some(face)),
        };
        let def = CARD_DEFS.get(usize::from(card))?;
        if !def.is_executable() || def.is_token {
            return None;
        }
        match face {
            None => (!def.types.contains(&CardType::Land)).then_some(def.object_name),
            Some(FaceV1::LeftHalf) => split_names(def).map(|(left, _)| left),
            Some(FaceV1::RightHalf) => split_names(def).map(|(_, right)| right),
            Some(FaceV1::AlternateSpell) if split_names(def).is_none() => alternate_spell_name(def),
            Some(FaceV1::AlternateSpell) => None,
            Some(FaceV1::Back) => def
                .transform_face
                .as_ref()
                .and_then(|back| (!back.types.contains(&CardType::Land)).then_some(back.name)),
        }
    }
}

fn name_groups(names_v2: bool) -> &'static [Vec<NameV1>] {
    static LEGACY: std::sync::OnceLock<Vec<Vec<NameV1>>> = std::sync::OnceLock::new();
    static PRINTED: std::sync::OnceLock<Vec<Vec<NameV1>>> = std::sync::OnceLock::new();
    if !names_v2 {
        return LEGACY.get_or_init(|| {
            nameable_nonland()
                .iter()
                .map(|group| group.iter().copied().map(NameV1::Front).collect())
                .collect()
        });
    }
    PRINTED.get_or_init(|| {
        let mut groups: BTreeMap<char, Vec<NameV1>> = BTreeMap::new();
        let mut seen = HashSet::new();
        for (index, def) in CARD_DEFS.iter().enumerate() {
            let Ok(card) = u16::try_from(index) else {
                continue;
            };
            let face = |face| NameV1::Face(PrintedFaceV1 { card, face });
            let forms = if split_names(def).is_some() {
                [
                    face(FaceV1::LeftHalf),
                    face(FaceV1::RightHalf),
                    face(FaceV1::Back),
                ]
            } else {
                [
                    NameV1::Front(card),
                    face(FaceV1::AlternateSpell),
                    face(FaceV1::Back),
                ]
            };
            for form in forms {
                if let Some(name) = form.printed_name().filter(|name| seen.insert(*name)) {
                    let letter = name
                        .chars()
                        .next()
                        .map_or(' ', |letter| letter.to_ascii_uppercase());
                    groups.entry(letter).or_default().push(form);
                }
            }
        }
        groups.into_values().collect()
    })
}

pub(crate) fn journal(state: &GameState, at: u32) -> Option<&JournalV1> {
    match state.engine.event_history.get(at as usize)? {
        CommittedEvent::MeddlingMageEntryV1 { journal } => Some(journal),
        _ => None,
    }
}

pub(crate) fn record(state: &mut GameState, journal: JournalV1) {
    state
        .engine
        .event_history
        .push(CommittedEvent::MeddlingMageEntryV1 {
            journal: Box::new(journal),
        });
}

/// The paid producer must still be resolving during the noncast entry.
fn producer_resolution_matches(
    state: &GameState,
    copy: &MetamorphCopyV1,
    entry: usize,
    archived: bool,
) -> bool {
    let Some(producer) = copy
        .reanimated_by
        .or(copy.show_and_tell.map(|show| show.producer))
    else {
        return true;
    };
    let Some(after_cast) = copy.cast_at.and_then(|at| at.checked_add(1)) else {
        return false;
    };
    let first_departure = state.engine.event_history[after_cast..]
        .iter()
        .enumerate()
        .find_map(|(offset, event)| match event {
            CommittedEvent::ZoneChange {
                object,
                from: Zone::Stack,
                to,
                controller_before,
            } if *object == producer.source => Some((after_cast + offset, *to, *controller_before)),
            _ => None,
        });
    match first_departure {
        Some((at, to, controller)) => {
            archived
                && at > entry
                && to != Zone::Stack
                && controller == producer.controller
                && !state.engine.event_history[entry + 1..at]
                    .iter()
                    .any(|event| {
                        matches!(
                            event,
                            CommittedEvent::ZoneChange {
                                from: Zone::Stack,
                                ..
                            }
                        ) || copy.show_and_tell.is_some()
                            && matches!(event, CommittedEvent::ZoneChange { object, .. }
                                if *object == copy.copied.source)
                    })
        }
        None => {
            !archived
                && state.objects.try_get(producer.source).is_some()
                && AbilitySourceContractV4::capture(state, producer.source) == producer
        }
    }
}

fn copy_entry_matches(
    state: &GameState,
    copy: &MetamorphCopyV1,
    entrant: AbilitySourceContractV4,
    entry: usize,
    archived: bool,
) -> bool {
    let history = &state.engine.event_history;
    let anchor = match (copy.cast_at, copy.vial) {
        (Some(cast_at), None)
            if copy.cast_matches(state)
                && producer_resolution_matches(state, copy, entry, archived) =>
        {
            if let Some(origin) = copy.show_and_tell {
                if !show_and_tell::entry_origin_matches(state, copy, origin, entry) {
                    return false;
                }
                origin.chosen_at
            } else {
                cast_at
            }
        }
        (None, Some(vial))
            if copy.reanimated_by.is_none()
                && copy.show_and_tell.is_none()
                && vial::entry_origin_matches(state, copy, vial) =>
        {
            vial.chosen_at
        }
        _ => return false,
    };
    let prepared_matches = match copy.prepared_at {
        Some(at) => copy.needs_prepared_receipt() && anchor <= at && at.checked_add(1) == Some(entry) &&
            matches!(history.get(at), Some(CommittedEvent::MeddlingMageEntryV1 { journal })
                if matches!(journal.as_ref(), JournalV1::NoncastCopyPrepared { copy: original } if original == copy))
            && !history[at + 1..entry].iter().any(|event| matches!(event, CommittedEvent::ZoneChange { object, .. } if *object == copy.copied.source)),
        None => !copy.needs_prepared_receipt(),
    };
    anchor < entry
        && prepared_matches
        && copy.discard_matches(state, entry, entrant.controller)
        && copy.entrant_matches(entrant)
        && crate::copy_entry_v1::validate_completed_copy(state, copy, entry)
        && (copy.reanimated_by.is_none() && copy.vial.is_none() && copy.show_and_tell.is_none()
            || crate::standard_cards_v1::had_card_def(
                state,
                entrant.source,
                entrant.zone_change_count,
                entrant.card_def,
            ))
        && matches!(history.get(entry),
            Some(CommittedEvent::ZoneChange { object, from, to: Zone::Battlefield, controller_before })
                if *object == entrant.source && *from == copy.physical.zone
                    && *controller_before == copy.physical.controller)
        && !history[anchor + 1..entry].iter().any(|event| {
            matches!(event,
            CommittedEvent::ZoneChange { object, from, .. }
                if *object == entrant.source && *from == copy.physical.zone)
        })
}

pub(crate) fn record_metamorph_copy(
    state: &mut GameState,
    copy: MetamorphCopyV1,
    entry_history_index: usize,
) -> Result<(), String> {
    let entrant = AbilitySourceContractV4::capture(state, copy.physical.source);
    if !copy_entry_matches(state, &copy, entrant, entry_history_index, false)
        || copy.physical.zone == Zone::Stack
            && AbilitySourceContractV4::capture(state, copy.copied.source) != copy.copied
        || !crate::eternal_vintage_v1::active_copy(state, entrant.source)
    {
        return Err("Metamorph lost the exact public copy on entry".into());
    }
    record(
        state,
        JournalV1::MetamorphCopied {
            entry_history_index,
            entrant,
            copy,
        },
    );
    Ok(())
}

fn named_copy_matches(
    state: &GameState,
    choice: &ChoiceV1,
    entrant: AbilitySourceContractV4,
    entry: usize,
) -> bool {
    let Some(copy) = &choice.metamorph_copy else {
        return true;
    };
    let mut expected = copy.clone();
    if expected.needs_prepared_receipt() {
        expected.prepared_at = entry.checked_sub(1);
    }
    matches!(state.engine.event_history.get(entry + 1), Some(CommittedEvent::MeddlingMageEntryV1 { journal })
            if matches!(journal.as_ref(), JournalV1::MetamorphCopied {
                entry_history_index, entrant: recorded, copy: actual,
            } if *entry_history_index == entry && *recorded == entrant && actual == &expected))
}

pub(crate) fn decision(choice: &ChoiceV1, group: Option<u16>) -> Result<Decision, String> {
    let groups = name_groups(choice.names_v2);
    let options = if let Some(group) = group {
        groups
            .get(usize::from(group))
            .ok_or("Meddling Mage name group absent")?
            .iter()
            .map(|name| name.printed_name().expect("valid Mage name").to_owned())
            .collect()
    } else {
        groups
            .iter()
            .map(|ids| {
                ids[0]
                    .printed_name()
                    .expect("valid Mage name")
                    .chars()
                    .next()
                    .expect("nonempty catalog name")
                    .to_string()
            })
            .collect()
    };
    Ok(Decision::ChooseMeddlingMageNameV1 {
        player: choice.controller,
        entry_ordinal: choice.ordinal,
        group,
        options,
    })
}

pub(crate) fn valid_index(choice: &ChoiceV1, group: Option<u16>, index: u16) -> bool {
    let groups = name_groups(choice.names_v2);
    match group {
        None => groups.get(usize::from(index)).is_some(),
        Some(group) => groups
            .get(usize::from(group))
            .is_some_and(|names| names.get(usize::from(index)).is_some()),
    }
}

pub(crate) fn name(choice: &ChoiceV1, group: u16, index: u16) -> Option<NameV1> {
    name_groups(choice.names_v2)
        .get(usize::from(group))?
        .get(usize::from(index))
        .copied()
}

pub(crate) fn requires_successor(state: &GameState) -> bool {
    state
        .engine
        .event_history
        .scan_for_v1(&[crate::event_history_v1::EventKindV1::MeddlingMageEntryV1])
        .iter()
        .any(|event| matches!(event, CommittedEvent::MeddlingMageEntryV1 { .. }))
        || state
            .engine
            .linked_aura_operation_v1
            .pending_meddling_mage_v1
            .as_ref()
            .is_some_and(|pending| {
                !matches!(&pending.choice, GeneralChoiceV36::ShowAndTellV1 { choice, .. }
                    if choice.physical.is_none())
            })
        || super::read_state(state).is_some_and(|s1b| !s1b.mage_names.is_empty())
}

/// Schema 5 projects the registered copy and name checkpoints. Refuse other kinds.
pub(crate) fn requires_copy_successor(state: &GameState) -> bool {
    state
        .engine
        .linked_aura_operation_v1
        .pending_meddling_mage_v1
        .as_ref()
        .is_some_and(|pending| {
            !matches!(
                &pending.choice,
                GeneralChoiceV36::MeddlingMageV1 { .. }
                    | GeneralChoiceV36::CopyEntryOptionV1 { .. }
                    | GeneralChoiceV36::ShowAndTellV1 { .. }
                    | GeneralChoiceV36::ReanimateMetamorphV1 { .. }
                    | GeneralChoiceV36::VialMetamorphV1 { .. }
            )
        })
}

fn entry_matches(
    choice: &ChoiceV1,
    actual: Option<&CommittedEvent>,
    source: AbilitySourceContractV4,
) -> bool {
    if choice.suppressed
        || source.zone != Zone::Battlefield
        || !owns_definition(source.card_def)
        || source.card_def != choice.card_def
        || source.controller != choice.controller
    {
        return false;
    }
    match (choice.original, &choice.proposal, actual) {
        (
            Some(original),
            ProposedEvent::ZoneChange(proposed),
            Some(CommittedEvent::ZoneChange {
                object,
                from,
                to: Zone::Battlefield,
                controller_before,
            }),
        ) => {
            *object == source.source
                && proposed.object == source.source
                && proposed.to_zone == Zone::Battlefield
                && !proposed.battlefield_manifested_v1
                && original.source == source.source
                && original.zone == *from
                && choice.metamorph_copy.as_ref().map_or(
                    original.card_def == source.card_def,
                    |copy| {
                        copy.physical == original
                            && !choice.suppressed
                            && copy.entrant_matches(source)
                    },
                )
                && original.owner == source.owner
                && original.controller == *controller_before
                && original.zone_change_count.checked_add(1) == Some(source.zone_change_count)
        }
        (
            None,
            ProposedEvent::CreateToken(proposed),
            Some(CommittedEvent::CreateToken {
                object,
                token_def,
                controller,
                ..
            }),
        ) => {
            *object == source.source
                && *token_def == source.card_def
                && *controller == source.controller
                && source.owner == source.controller
                && source.zone_change_count == 0
                && proposed.token_def == *token_def
                && proposed.controller == *controller
        }
        _ => false,
    }
}

pub(crate) fn validate_retained(state: &GameState) -> Result<(), String> {
    vial::validate_live_paid(state)?;
    use crate::linked_aura_operation_v1 as owner;
    if owner::locally_owned(state) {
        return Ok(());
    }
    if state.engine.pending_effect.as_ref().is_some_and(|pending| {
        pending.resolving_item.kind == crate::state::StackItemKind::Spell
            && pending.choice.is_some()
            && state
                .objects
                .try_get(pending.resolving_item.source)
                .is_some_and(|source| owns_definition(source.card_def))
    }) {
        return Err("a legacy Meddling Mage choice cannot resume under the pre-entry owner".into());
    }
    if !requires_successor(state) {
        return Ok(());
    }
    let names = super::read_state(state).map_or(&[][..], |s1b| s1b.mage_names.as_slice());
    let mut operations = BTreeMap::new();
    let mut choices = BTreeMap::new();
    let mut entrants = HashSet::new();
    let mut actual_entries = HashSet::new();
    let mut copies_needing_names = HashSet::new();
    let mut vial_activations = HashSet::new();
    let mut vial_copies = HashSet::new();
    let mut prepared_entries = HashSet::new();
    let mut show_choices = HashSet::new();
    let mut show_copies = HashSet::new();
    let mut used_discards = HashSet::new();
    let mut paid_discard_keys = HashSet::new();
    let mut paid_discard_indices = HashSet::new();
    let mut paid_discard_stack_items = BTreeMap::new();
    for (index, event) in state.engine.event_history.iter().enumerate() {
        if let CommittedEvent::CopyEntryProvenanceV1 { journal } = event {
            match journal.as_ref() {
                crate::copy_entry_v1::ProvenanceV1::CopyDiscardPaid {
                    stack_item,
                    physical,
                    discard_at,
                    ..
                } => {
                    if stack_item.0 == 0
                        || stack_item.0 > state.engine.next_stack_item_id
                        || !paid_discard_keys.insert((*stack_item, *physical))
                        || !paid_discard_indices.insert(*discard_at)
                    {
                        return Err("duplicate or out-of-bounds copy-entry discard receipt".into());
                    }
                    paid_discard_stack_items.insert(*discard_at, (*stack_item, physical.owner));
                }
                crate::copy_entry_v1::ProvenanceV1::AbilityCountered { stack_item, kind } => {
                    if !crate::copy_entry_v1::validate_ability_countered_receipt(
                        state,
                        index,
                        *stack_item,
                        kind,
                    ) {
                        return Err("countered-ability provenance receipt is invalid".into());
                    }
                }
                _ => {}
            }
            continue;
        }
        let CommittedEvent::MeddlingMageEntryV1 { journal } = event else {
            continue;
        };
        let index = u32::try_from(index).map_err(|_| "Meddling Mage event index overflow")?;
        match journal.as_ref() {
            JournalV1::VialPaid { source, item, .. } => {
                if !crate::eternal_opening_v1::enabled(state)
                    || !vial_activations.insert(*item)
                    || !vial::paid_matches(state, index as usize, *source, *item)
                    || !vial::live_item_matches(state, index as usize, *item, *source)
                {
                    return Err("Aether Vial lost its actual paid activation".into());
                }
            }
            JournalV1::VialMetamorphChosen { .. } => {}
            JournalV1::NoncastCopyPrepared { copy } => {
                if !prepared_entries.insert(index as usize)
                    || copy.prepared_at != Some(index as usize)
                    || !copy.needs_prepared_receipt()
                    || !copy.cast_matches(state)
                        && !copy
                            .vial
                            .is_some_and(|vial| vial::entry_origin_matches(state, copy, vial))
                {
                    return Err("noncast copy choice changed its producer".into());
                }
            }
            JournalV1::ShowAndTellMetamorphChosen { copy } => {
                if copy
                    .show_and_tell
                    .is_none_or(|show| show.chosen_at != index as usize)
                    || !copy.cast_matches(state)
                    || !show_choices.insert((copy.stack_item, copy.physical.owner))
                {
                    return Err("Show and Tell lost its exact Metamorph choice".into());
                }
            }
            JournalV1::Started { .. } | JournalV1::StartedWithNamesV2 { .. } => {
                operations.insert(
                    index,
                    (
                        matches!(journal.as_ref(), JournalV1::StartedWithNamesV2 { .. }),
                        false,
                    ),
                );
            }
            JournalV1::Chosen {
                operation,
                choice,
                group,
                index: name_index,
            } => {
                if operations
                    .get(operation)
                    .is_none_or(|(names_v2, _)| *names_v2 != choice.names_v2)
                    || choice.suppressed
                    || !owns_definition(choice.card_def)
                    || !valid_index(choice, Some(*group), *name_index)
                {
                    return Err(
                        "Meddling Mage choice has no open operation or valid name index".into(),
                    );
                }
                choices.insert(index, *operation);
            }
            JournalV1::Named {
                operation,
                chosen,
                entry_history_index,
                source,
                named,
            } => {
                let actual = state
                    .engine
                    .event_history
                    .get(*entry_history_index as usize);
                let Some(JournalV1::Chosen {
                    choice,
                    group,
                    index: name_index,
                    ..
                }) = self::journal(state, *chosen)
                else {
                    return Err("Meddling Mage name lost its actual choice".into());
                };
                let link = ObjectLinkV4 {
                    object: source.source,
                    zone_change_count: source.zone_change_count,
                };
                let Some((_, has_name)) = operations.get_mut(operation) else {
                    return Err("Meddling Mage name has no open operation".into());
                };
                if choices.remove(chosen) != Some(*operation)
                    || *entry_history_index <= *chosen
                    || *entry_history_index >= index
                    || !entry_matches(choice, actual, *source)
                    || !named_copy_matches(state, choice, *source, *entry_history_index as usize)
                    || (choice.metamorph_copy.is_some()
                        && !copies_needing_names.remove(&(*entry_history_index as usize)))
                    || name(choice, *group, *name_index) != Some(*named)
                    || !actual_entries.insert(*entry_history_index)
                    || !entrants.insert(link)
                    || !names
                        .iter()
                        .copied()
                        .filter(|(saved, _)| *saved == link)
                        .eq([(link, *named)])
                {
                    return Err("Meddling Mage name or entry incarnation changed".into());
                }
                *has_name = true;
            }
            JournalV1::MetamorphCopied {
                entry_history_index,
                entrant,
                copy,
            } => {
                if copy.prepared_at.is_some_and(|at| !prepared_entries.remove(&at))
                    || copy.discarded_at.is_some_and(|at| !used_discards.insert(at))
                    || entry_history_index.checked_add(1) != Some(index as usize)
                    || !copy_entry_matches(state, copy, *entrant, *entry_history_index, true)
                    // Old ordinary copies have no chosen-source receipt. A different
                    // equally legal, byte-indistinguishable donor is a valid history.
                    || copy.physical.zone != Zone::Stack && !copy.copied_incarnation_at_entry(state, *entry_history_index)
                    || copy
                        .values
                        .as_ref()
                        .is_some_and(|values| values.card_def != copy.copied.card_def)
                    || copy.physical.zone != Zone::Stack && !crate::copy_entry_v1::installed_values_match(state, *entrant, copy)
                    || copy
                        .vial
                        .is_some_and(|_| !vial_copies.insert(copy.stack_item))
                    || copy.show_and_tell.is_some_and(|_| {
                        !show_copies.insert((copy.stack_item, copy.physical.owner))
                    })
                    || (owns_definition(entrant.card_def)
                        && !copies_needing_names.insert(*entry_history_index))
                {
                    return Err("Metamorph copy lost its actual producer or entry".into());
                }
                if state.objects.try_get(entrant.source).is_some_and(|live| {
                    live.zone_change_count == entrant.zone_change_count
                        && live.zone == Zone::Battlefield
                }) && !state.eternal_v1.as_ref().is_some_and(|eternal| {
                    eternal.vintage.copies.iter().any(|record| {
                        record.host.object == entrant.source
                            && record.host.zone_change_count == entrant.zone_change_count
                            && record.card_def == copy.physical.card_def
                            && (copy.physical.zone == Zone::Stack || {
                                let printed = &CARD_DEFS[usize::from(copy.physical.card_def)];
                                let base = crate::state::ObjectStateV4::from_card_def(
                                    copy.physical.card_def,
                                );
                                record.physical_face == Default::default()
                                    && record.name == printed.object_name
                                    && record.color_mask == base.effective_color_mask
                                    && record.subtype_ids == base.effective_subtype_ids
                            })
                    })
                }) {
                    return Err("Metamorph lost its physical printing for departure".into());
                }
            }
            JournalV1::Completed { operation } => {
                if !operations
                    .remove(operation)
                    .is_some_and(|(_, has_name)| has_name)
                    || choices.values().any(|pending| pending == operation)
                {
                    return Err(
                        "Meddling Mage operation ended without an actual named entry".into(),
                    );
                }
            }
        }
    }
    for (item, _) in show_choices.difference(&show_copies) {
        if !owner::pending_original_operation(state).is_some_and(|(original, _)| {
            original
                .stack
                .last()
                .is_some_and(|row| show_and_tell::printed(row) && row.v4.stack_item_id == *item)
        }) {
            return Err("Show and Tell copy choice has no matching entry".into());
        }
    }
    for (discard_at, (item, owner_player)) in paid_discard_stack_items {
        if used_discards.contains(&discard_at) {
            continue;
        }
        let pending_show = show_choices.contains(&(item, owner_player))
            && !show_copies.contains(&(item, owner_player))
            && owner::pending_original_operation(state).is_some_and(|(original, _)| {
                original
                    .stack
                    .last()
                    .is_some_and(|row| show_and_tell::printed(row) && row.v4.stack_item_id == item)
            });
        if !pending_show {
            return Err("copy-entry discard receipt has no matching copy entry".into());
        }
    }
    validate_same_turn_discard_conservation(state)?;
    if !prepared_entries.is_empty() {
        return Err("noncast copy preparation has no matching entry".into());
    }
    if names.len() != entrants.len() || !copies_needing_names.is_empty() {
        return Err(
            "Meddling Mage state carries a name without actual chosen-entry evidence".into(),
        );
    }
    if !operations.is_empty() {
        let Some((original, operation)) = owner::pending_original_operation(state) else {
            return Err("Meddling Mage entry has no live operation owner".into());
        };
        if operations.len() != 1
            || !operations.keys().all(|&index| {
                matches!(
                    journal(state, index),
                    Some(JournalV1::Started { operation: recorded, original_sha256 }
                       | JournalV1::StartedWithNamesV2 { operation: recorded, original_sha256 })
                        if recorded.as_ref() == operation
                            && owner::origin_digest_matches(original_sha256, original)
                )
            })
        {
            return Err("Meddling Mage entry does not match its current operation".into());
        }
    }
    Ok(())
}

fn activated_discard_uses_for_object(
    state: &GameState,
    id: ObjectId,
    live: &crate::state::GameObject,
    controller: PlayerId,
    turn_slice: &[CommittedEvent],
) -> usize {
    use crate::card_def::CostComponent;
    use crate::state::AbilityKindV4;
    if live.controller != controller
        || live.v4.ability_uses_this_turn.is_empty()
        || crate::eternal_cube_c8b_v1::has_saheeli_copy(state, id)
        || crate::standard_cards_v1::active_aegis_copy(state, id)
        || crate::eternal_cube_c8c_v1::is_absorbing_copy(state, id)
        || crate::eternal_legacy_e7a_v1::active_copy(state, id)
    {
        return 0;
    }
    let stolen_this_turn = turn_slice.iter().any(|ev| {
        matches!(
            ev,
            CommittedEvent::TemporaryControlInstalledV1 { installation }
                if matches!(
                    installation.target,
                    crate::state::StackTargetContractV4::Object { object, .. }
                        if object == id
                )
        )
    });
    if stolen_this_turn {
        return 0;
    }
    let Some(def) = CARD_DEFS.get(usize::from(live.card_def)) else {
        return 0;
    };
    let mut total: usize = 0;
    for entry in &live.v4.ability_uses_this_turn {
        if entry.ability_kind != AbilityKindV4::Activated || entry.uses == 0 {
            continue;
        }
        let Some(ability) = def
            .activated_abilities
            .get(usize::from(entry.ability_index))
        else {
            continue;
        };
        if ability.activation_zone != Zone::Battlefield {
            continue;
        }
        let per_use: usize = ability
            .cost
            .iter()
            .map(|component| match component {
                CostComponent::DiscardCards(n) | CostComponent::DiscardLandCardsV1(n) => {
                    usize::from(*n)
                }
                CostComponent::DiscardIslandAndAnotherCardV1 => 2,
                _ => 0,
            })
            .sum();
        total = total.saturating_add(per_use.saturating_mul(usize::from(entry.uses)));
    }
    total
}

#[cfg(feature = "eternal-fixtures")]
pub(crate) fn note_departing_discard_uses(state: &mut GameState, id: ObjectId, from_zone: Zone) {
    if from_zone != Zone::Battlefield || state.step == crate::state::Step::Untap {
        return;
    }
    let Some(live) = state.objects.try_get(id) else {
        return;
    };
    if live.v4.ability_uses_this_turn.is_empty() {
        return;
    }
    let controller = live.controller;
    let link = ObjectLinkV4 {
        object: id,
        zone_change_count: live.zone_change_count,
    };
    let history = &state.engine.event_history;
    let turn_start = history
        .iter()
        .rposition(|ev| matches!(ev, CommittedEvent::UpkeepBegan { .. }))
        .map_or(0, |idx| idx + 1);
    let uses =
        activated_discard_uses_for_object(state, id, live, controller, &history[turn_start..]);
    if uses > 0 {
        let uses_u16 = u16::try_from(uses).unwrap_or(u16::MAX);
        let list = &mut s1b_state(state).departed_discard_uses;
        if !list.iter().any(|(existing, _, _)| *existing == link) {
            list.push((link, controller, uses_u16));
        }
    }
}

#[cfg(feature = "eternal-fixtures")]
pub(crate) fn note_departing_effect_discard_uses(
    state: &mut GameState,
    id: ObjectId,
    from_zone: Zone,
) {
    use crate::state::AbilityKindV4;
    if from_zone != Zone::Battlefield || state.step == crate::state::Step::Untap {
        return;
    }
    let Some(live) = state.objects.try_get(id) else {
        return;
    };
    if live.v4.ability_uses_this_turn.is_empty()
        || crate::eternal_cube_c8b_v1::has_saheeli_copy(state, id)
        || crate::standard_cards_v1::active_aegis_copy(state, id)
        || crate::eternal_cube_c8c_v1::is_absorbing_copy(state, id)
        || crate::eternal_legacy_e7a_v1::active_copy(state, id)
    {
        return;
    }
    let Some(def) = CARD_DEFS.get(usize::from(live.card_def)) else {
        return;
    };
    let source_controller = live.controller;
    let link = ObjectLinkV4 {
        object: id,
        zone_change_count: live.zone_change_count,
    };
    let history = &state.engine.event_history;
    let turn_start = history
        .iter()
        .rposition(|ev| matches!(ev, CommittedEvent::UpkeepBegan { .. }))
        .map_or(0, |idx| idx + 1);
    let turn_slice = &history[turn_start..];
    let stolen_this_turn = turn_slice.iter().any(|ev| {
        matches!(
            ev,
            CommittedEvent::TemporaryControlInstalledV1 { installation }
                if matches!(
                    installation.target,
                    crate::state::StackTargetContractV4::Object { object, .. }
                        if object == id
                )
        )
    });
    let mut to_record = Vec::new();
    for entry in &live.v4.ability_uses_this_turn {
        if entry.ability_kind != AbilityKindV4::Activated || entry.uses == 0 {
            continue;
        }
        let ability_index = usize::from(entry.ability_index);
        let Some(ability) = def.activated_abilities.get(ability_index) else {
            continue;
        };
        if ability.activation_zone != Zone::Battlefield {
            continue;
        }
        let effect = (ability.effect)();
        let countered = countered_effect_discard_uses(turn_slice, link, ability_index);
        if usize::from(entry.uses) <= countered {
            continue;
        }
        for player in [PlayerId::P0, PlayerId::P1] {
            let per_use = if stolen_this_turn {
                mandatory_each_player_discards(&effect)
            } else {
                mandatory_effect_discards_for(&effect, source_controller, player)
            };
            if per_use > 0 {
                to_record.push((link, player, ability_index, u32::from(entry.uses)));
            }
        }
    }
    if !to_record.is_empty() {
        let list = &mut s1b_state(state).departed_effect_discard_uses;
        for record in to_record {
            if !list
                .iter()
                .any(|&(l, p, idx, _)| l == record.0 && p == record.1 && idx == record.2)
            {
                list.push(record);
            }
        }
    }
}

pub(crate) fn animate_ability_uses_for_object(
    live: &crate::state::GameObject,
    card_def: u16,
) -> u32 {
    let Some(def) = CARD_DEFS.get(usize::from(card_def)) else {
        return 0;
    };
    def.activated_abilities
        .iter()
        .enumerate()
        .filter(|(_, a)| {
            matches!(
                (a.effect)(),
                crate::effect::EffectOp::AnimateSource
                    | crate::effect::EffectOp::AnimateSourcePermanentlyV1
            )
        })
        .map(|(idx, _)| {
            live.v4
                .ability_uses_this_turn
                .iter()
                .filter(|u| {
                    u.ability_kind == crate::state::AbilityKindV4::Activated
                        && u.ability_index == idx as u16
                })
                .map(|u| u32::from(u.uses))
                .sum::<u32>()
        })
        .sum()
}

#[cfg(feature = "eternal-fixtures")]
pub(crate) fn note_departing_animate_uses(state: &mut GameState, id: ObjectId, from_zone: Zone) {
    if from_zone != Zone::Battlefield || state.step == crate::state::Step::Untap {
        return;
    }
    let Some(live) = state.objects.try_get(id) else {
        return;
    };
    if live.v4.ability_uses_this_turn.is_empty() {
        return;
    }
    let link = ObjectLinkV4 {
        object: id,
        zone_change_count: live.zone_change_count,
    };
    let mut uses = animate_ability_uses_for_object(live, live.card_def);
    if uses == 0 {
        if let Some(saved_def) = crate::eternal_cube_c8b_v1::read_state(state)
            .and_then(|c8b| c8b.saheeli_copies.iter().find(|c| c.host == link))
            .map(|c| c.card_def)
        {
            uses = animate_ability_uses_for_object(live, saved_def);
        }
    }
    if uses > 0 {
        let list = &mut s1b_state(state).departed_animate_uses;
        if !list.iter().any(|(existing, _)| *existing == link) {
            list.push((link, uses));
        }
    }
}

#[cfg(feature = "eternal-fixtures")]
pub(crate) fn note_resolution_effect_discards(state: &mut GameState, history_before: usize) {
    if state.step == crate::state::Step::Untap {
        return;
    }
    let new_entries: Vec<(PlayerId, usize, ObjectId)> = state
        .engine
        .event_history
        .get(history_before..)
        .into_iter()
        .flatten()
        .enumerate()
        .filter_map(|(offset, ev)| match ev {
            CommittedEvent::ZoneChange {
                object,
                from: Zone::Hand,
                to,
                controller_before,
            } if !matches!(to, Zone::Hand | Zone::Battlefield | Zone::Stack) => {
                Some((*controller_before, history_before + offset, *object))
            }
            _ => None,
        })
        .collect();
    if !new_entries.is_empty() {
        let list = &mut s1b_state(state).resolution_effect_discards;
        for entry in new_entries {
            if !list.contains(&entry) {
                list.push(entry);
            }
        }
    }
}

fn player_ref_can_match(
    pref: crate::effect::PlayerRef,
    caster: PlayerId,
    target_player: PlayerId,
) -> bool {
    match pref {
        crate::effect::PlayerRef::Controller => caster == target_player,
        crate::effect::PlayerRef::Opponent => caster != target_player,
        crate::effect::PlayerRef::Target(_) | crate::effect::PlayerRef::ObjectController(_) => true,
    }
}

fn effect_resolution_discard_cap(
    op: &crate::effect::EffectOp,
    caster: PlayerId,
    target_player: PlayerId,
) -> (usize, bool) {
    use crate::effect::EffectOp;
    match op {
        EffectOp::DiscardCards { player, count } => {
            if player_ref_can_match(*player, caster, target_player) {
                (*count as usize, false)
            } else {
                (0, false)
            }
        }
        EffectOp::DiscardUpToThenDraw { player, maximum } => {
            if player_ref_can_match(*player, caster, target_player) {
                (*maximum as usize, false)
            } else {
                (0, false)
            }
        }
        EffectOp::DiscardBasicLandOrCards { player, otherwise } => {
            if player_ref_can_match(*player, caster, target_player) {
                ((*otherwise as usize).max(1), false)
            } else {
                (0, false)
            }
        }
        EffectOp::Sequence(ops) => {
            let mut saw_draw = false;
            let mut total: usize = 0;
            let mut after_draw = false;
            for step in ops {
                if matches!(
                    step,
                    EffectOp::DrawCards { .. } | EffectOp::DrawCardsDynamic { .. }
                ) {
                    saw_draw = true;
                }
                let (cap, inner_after_draw) =
                    effect_resolution_discard_cap(step, caster, target_player);
                if cap > 0 {
                    total = total.saturating_add(cap);
                    if saw_draw || inner_after_draw {
                        after_draw = true;
                    }
                }
            }
            (total, after_draw)
        }
        EffectOp::Choice { options, .. } => {
            let mut best: usize = 0;
            let mut any_after_draw = false;
            for opt in options {
                let (cap, inner_after_draw) =
                    effect_resolution_discard_cap(opt, caster, target_player);
                if cap > best {
                    best = cap;
                    any_after_draw = inner_after_draw;
                }
            }
            (best, any_after_draw)
        }
        _ => (0, false),
    }
}

fn spell_resolution_discard_cap(
    def: &CardDef,
    caster: PlayerId,
    target_player: PlayerId,
) -> (usize, bool) {
    let mut best: usize = 0;
    let mut requires_draw = false;
    let mut consider = |op: &crate::effect::EffectOp| {
        let (cap, after_draw) = effect_resolution_discard_cap(op, caster, target_player);
        if cap > best {
            best = cap;
            requires_draw = after_draw;
        }
    };
    if let Some(op) = (def.spell_effect)() {
        consider(&op);
    }
    for mode in def
        .mode2
        .iter()
        .chain(def.mode3.iter())
        .chain(def.additional_modes.iter())
    {
        consider(&(mode.effect)());
    }
    (best, requires_draw)
}

fn is_controller_hand_discard(ev: &CommittedEvent, controller: PlayerId) -> bool {
    matches!(
        ev,
        CommittedEvent::ZoneChange {
            from: Zone::Hand,
            to,
            controller_before,
            ..
        } if *controller_before == controller
            && !matches!(to, Zone::Hand | Zone::Battlefield | Zone::Stack)
    )
}

pub(crate) fn mandatory_effect_discards_for(
    op: &crate::effect::EffectOp,
    source_controller: PlayerId,
    target_player: PlayerId,
) -> usize {
    use crate::effect::{EffectOp, PlayerRef};
    use crate::standard_cards_v1::StandardOpV1;
    match op {
        EffectOp::StandardV1(StandardOpV1::EachPlayerDiscards { count }) => *count as usize,
        EffectOp::DiscardCards {
            player: PlayerRef::Controller,
            count,
        } if source_controller == target_player => *count as usize,
        EffectOp::DiscardBasicLandOrCards {
            player: PlayerRef::Controller,
            otherwise,
        } if source_controller == target_player => (*otherwise as usize).min(1),
        EffectOp::Sequence(ops) => ops.iter().fold(0usize, |acc, step| {
            acc.saturating_add(mandatory_effect_discards_for(
                step,
                source_controller,
                target_player,
            ))
        }),
        _ => 0,
    }
}

fn mandatory_each_player_discards(op: &crate::effect::EffectOp) -> usize {
    use crate::effect::EffectOp;
    use crate::standard_cards_v1::StandardOpV1;
    match op {
        EffectOp::StandardV1(StandardOpV1::EachPlayerDiscards { count }) => *count as usize,
        EffectOp::Sequence(ops) => ops.iter().fold(0usize, |acc, step| {
            acc.saturating_add(mandatory_each_player_discards(step))
        }),
        _ => 0,
    }
}

pub(crate) fn has_mandatory_effect_discard(op: &crate::effect::EffectOp) -> bool {
    mandatory_effect_discards_for(op, PlayerId::P0, PlayerId::P0) > 0
}

fn countered_effect_discard_uses(
    turn_slice: &[CommittedEvent],
    source: ObjectLinkV4,
    ability_index: usize,
) -> usize {
    let mut countered_items = HashSet::new();
    for ev in turn_slice {
        if let CommittedEvent::CopyEntryProvenanceV1 { journal } = ev {
            if let crate::copy_entry_v1::ProvenanceV1::AbilityCountered {
                stack_item,
                kind:
                    crate::copy_entry_v1::CounteredAbilityKindV1::EffectDiscard {
                        source: src,
                        ability_index: idx,
                    },
            } = journal.as_ref()
            {
                if *src == source && *idx == ability_index {
                    countered_items.insert(*stack_item);
                }
            }
        }
    }
    countered_items.len()
}

fn pending_effect_discard_stack_items(
    state: &GameState,
    source: ObjectLinkV4,
    ability_index: usize,
    effect: Option<&crate::effect::EffectOp>,
) -> usize {
    use crate::state::StackItemKind;
    state
        .stack
        .iter()
        .filter(|item| {
            if item.kind != StackItemKind::ActivatedAbility || item.source != source.object {
                return false;
            }
            let item_zcc = item
                .v4
                .ability_source_contract
                .map(|c| c.zone_change_count)
                .or_else(|| {
                    state
                        .objects
                        .try_get(item.source)
                        .map(|o| o.zone_change_count)
                });
            if item_zcc != Some(source.zone_change_count) {
                return false;
            }
            match item.v4.activated_ability_index {
                Some(idx) => usize::from(idx) == ability_index,
                None => effect.is_some_and(|e| item.inline_effect.as_ref() == Some(e)),
            }
        })
        .count()
}

fn departed_effect_discard_per_use(
    state: &GameState,
    link: ObjectLinkV4,
    ability_index: usize,
    player: PlayerId,
) -> usize {
    let from_def = |card_def: u16| -> Option<usize> {
        let ability = CARD_DEFS
            .get(usize::from(card_def))?
            .activated_abilities
            .get(ability_index)?;
        if ability.activation_zone != Zone::Battlefield {
            return None;
        }
        let count = mandatory_effect_discards_for(&(ability.effect)(), player, player);
        (count > 0).then_some(count)
    };
    if let Some(live) = state.objects.try_get(link.object) {
        if let Some(count) = from_def(live.card_def) {
            return count;
        }
        if let Some(count) = crate::standard_cards_v1::find_had_card_def(
            state,
            link.object,
            link.zone_change_count,
            from_def,
        ) {
            return count;
        }
    }
    1
}

fn activated_effect_discard_uses_for_object(
    state: &GameState,
    id: ObjectId,
    live: &crate::state::GameObject,
    controller: PlayerId,
    turn_slice: &[CommittedEvent],
) -> usize {
    use crate::state::AbilityKindV4;
    if live.zone != Zone::Battlefield
        || live.v4.ability_uses_this_turn.is_empty()
        || crate::eternal_cube_c8b_v1::has_saheeli_copy(state, id)
        || crate::standard_cards_v1::active_aegis_copy(state, id)
        || crate::eternal_cube_c8c_v1::is_absorbing_copy(state, id)
        || crate::eternal_legacy_e7a_v1::active_copy(state, id)
    {
        return 0;
    }
    let stolen_this_turn = turn_slice.iter().any(|ev| {
        matches!(
            ev,
            CommittedEvent::TemporaryControlInstalledV1 { installation }
                if matches!(
                    installation.target,
                    crate::state::StackTargetContractV4::Object { object, .. }
                        if object == id
                )
        )
    });
    let Some(def) = CARD_DEFS.get(usize::from(live.card_def)) else {
        return 0;
    };
    let link = ObjectLinkV4 {
        object: id,
        zone_change_count: live.zone_change_count,
    };
    let mut total: usize = 0;
    for entry in &live.v4.ability_uses_this_turn {
        if entry.ability_kind != AbilityKindV4::Activated || entry.uses == 0 {
            continue;
        }
        let ability_index = usize::from(entry.ability_index);
        let Some(ability) = def.activated_abilities.get(ability_index) else {
            continue;
        };
        if ability.activation_zone != Zone::Battlefield {
            continue;
        }
        let effect = (ability.effect)();
        let per_use = if stolen_this_turn {
            mandatory_each_player_discards(&effect)
        } else {
            mandatory_effect_discards_for(&effect, live.controller, controller)
        };
        if per_use == 0 {
            continue;
        }
        let pending_on_stack =
            pending_effect_discard_stack_items(state, link, ability_index, Some(&effect));
        let countered = countered_effect_discard_uses(turn_slice, link, ability_index);
        let resolved_uses = usize::from(entry.uses)
            .saturating_sub(pending_on_stack)
            .saturating_sub(countered);
        total = total.saturating_add(per_use.saturating_mul(resolved_uses));
    }
    total
}

/// Same-turn hand-discard conservation law: every activated ability with a
/// fixed hand-discard cost (live or departed on the current turn, such as
/// Putrid Imp), every spell-resolution / spell-cost discard (such as Faithless
/// Looting or `ResolutionDiscardWaitsV1`), every live or departed mandatory
/// ability-resolution discard (such as Liliana of the Veil's +1, for every
/// affected player), and every Mox Diamond `CopyDiscardPaid` receipt in the
/// current turn consumes a distinct `Hand -> Graveyard | Exile | Library` move
/// by that player.
///
/// Threat-model boundary: snapshots are produced and stored by our own pinned
/// code, so stateless restore validation guards against corruption and engine
/// bugs, not an adversary. Direct edits of `S1bStateV1` departed-use counters
/// (`departed_animate_uses`, `departed_discard_uses`,
/// `departed_effect_discard_uses`) that produce a state byte-for-byte
/// identical to a legal alternative game are out of scope for stateless
/// restore validation; integrity of stored snapshots rests on the record
/// hashes in manifests and receipts.
///
/// Residual boundary: once Untap clears `ability_uses_this_turn` and Cleanup
/// expires temporary boosts on a subsequent turn, an unauthenticated JSON
/// snapshot retains no permanent per-activation marker distinguishing an
/// untargeted discard-cost activation from `pay_copy_entry_cost` in a prior
/// turn beyond `CopyDiscardPaid` uniqueness and stack-item anchoring.
fn validate_same_turn_discard_conservation(state: &GameState) -> Result<(), String> {
    use crate::card_def::CostComponent;
    use crate::cast_identity_v1::PublicIdentityV1;
    if state.step == crate::state::Step::Untap {
        return Ok(());
    }
    let history = &state.engine.event_history;
    let turn_start = history
        .iter()
        .rposition(|ev| matches!(ev, CommittedEvent::UpkeepBegan { .. }))
        .map_or(0, |idx| idx + 1);
    let turn_slice = &history[turn_start..];

    for controller in [PlayerId::P0, PlayerId::P1] {
        let mut copy_discards = HashSet::new();
        for ev in turn_slice {
            match ev {
                CommittedEvent::CopyEntryProvenanceV1 { journal } => {
                    if let crate::copy_entry_v1::ProvenanceV1::CopyDiscardPaid {
                        land,
                        discard_at,
                        ..
                    } = journal.as_ref()
                    {
                        if land.controller == controller && *discard_at >= turn_start {
                            copy_discards.insert(*discard_at);
                        }
                    }
                }
                CommittedEvent::MeddlingMageEntryV1 { journal } => {
                    if let JournalV1::MetamorphCopied { entrant, copy, .. } = journal.as_ref() {
                        if entrant.controller == controller {
                            if let Some(at) = copy.discarded_at {
                                if at >= turn_start {
                                    copy_discards.insert(at);
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        if copy_discards.is_empty() {
            continue;
        }
        let hand_discard_indices: HashSet<usize> = turn_slice
            .iter()
            .enumerate()
            .filter_map(|(offset, ev)| {
                is_controller_hand_discard(ev, controller).then_some(turn_start + offset)
            })
            .collect();

        // Collect all hand-discard indices attributable to spell resolutions,
        // `ResolutionDiscardWaitsV1` windows, or spell additional discard costs
        // so unrelated spell-based discards (e.g. Faithless Looting) cannot
        // bankroll a forged `CopyDiscardPaid` receipt.
        let mut spell_discard_indices = HashSet::new();
        for (offset, ev) in turn_slice.iter().enumerate() {
            let abs_idx = turn_start + offset;
            match ev {
                CommittedEvent::ResolutionDiscardWaitsV1 { player, hand, .. }
                    if *player == controller =>
                {
                    let mut cursor = abs_idx + 1;
                    while let Some(next_ev) = history.get(cursor) {
                        if let CommittedEvent::ZoneChange {
                            object,
                            from: Zone::Hand,
                            to,
                            controller_before,
                        } = next_ev
                        {
                            if *controller_before == controller
                                && !matches!(to, Zone::Hand | Zone::Battlefield | Zone::Stack)
                                && hand.iter().any(|link| link.object == *object)
                            {
                                spell_discard_indices.insert(cursor);
                                cursor += 1;
                                continue;
                            }
                        }
                        if matches!(
                            next_ev,
                            CommittedEvent::TinybonesV72 { journal }
                                if matches!(
                                    journal.as_ref(),
                                    crate::engine::tinybones_v72::JournalV72::Discarded(_)
                                )
                        ) {
                            cursor += 1;
                            continue;
                        }
                        break;
                    }
                }
                CommittedEvent::PublishedCastIdentityV1 { receipt } => {
                    let PublicIdentityV1::Known { card_def } = receipt.public_identity else {
                        continue;
                    };
                    let Some(def) = CARD_DEFS.get(usize::from(card_def)) else {
                        continue;
                    };
                    // Additional discard costs paid immediately before the cast receipt.
                    if receipt.controller == controller {
                        let cast_cost_discards: usize = def
                            .additional_cost
                            .unwrap_or(&[])
                            .iter()
                            .map(|c| match c {
                                CostComponent::DiscardCards(n)
                                | CostComponent::DiscardLandCardsV1(n) => usize::from(*n),
                                CostComponent::DiscardIslandAndAnotherCardV1 => 2,
                                _ => 0,
                            })
                            .sum();
                        if cast_cost_discards > 0 {
                            let mut cursor = abs_idx;
                            let mut found = 0usize;
                            while cursor > turn_start && found < cast_cost_discards {
                                cursor -= 1;
                                let prev = &history[cursor];
                                if is_controller_hand_discard(prev, controller) {
                                    spell_discard_indices.insert(cursor);
                                    found += 1;
                                } else if matches!(
                                    prev,
                                    CommittedEvent::TinybonesV72 { journal }
                                        if matches!(
                                            journal.as_ref(),
                                            crate::engine::tinybones_v72::JournalV72::Discarded(_)
                                        )
                                ) {
                                    continue;
                                } else {
                                    break;
                                }
                            }
                        }
                    }
                    // Resolution discards committed immediately before the spell leaves the stack.
                    let (res_cap, requires_draw) =
                        spell_resolution_discard_cap(def, receipt.controller, controller);
                    if res_cap == 0 {
                        continue;
                    }
                    let Some(leave_off) = history[abs_idx + 1..].iter().position(|later| {
                        matches!(
                            later,
                            CommittedEvent::ZoneChange {
                                object,
                                from: Zone::Stack,
                                to,
                                ..
                            } if *object == receipt.spell && *to != Zone::Stack
                        )
                    }) else {
                        continue;
                    };
                    let leave_idx = abs_idx + 1 + leave_off;
                    let mut run = Vec::new();
                    let mut cursor = leave_idx;
                    while cursor > abs_idx + 1 && run.len() < res_cap {
                        cursor -= 1;
                        let prev = &history[cursor];
                        if is_controller_hand_discard(prev, controller) {
                            run.push(cursor);
                        } else if matches!(
                            prev,
                            CommittedEvent::TinybonesV72 { journal }
                                if matches!(
                                    journal.as_ref(),
                                    crate::engine::tinybones_v72::JournalV72::Discarded(_)
                                )
                        ) {
                            continue;
                        } else {
                            break;
                        }
                    }
                    if let Some(&earliest) = run.last() {
                        let draw_ok = !requires_draw || {
                            let mut saw_draw = false;
                            for mid in history[abs_idx + 1..earliest].iter().rev() {
                                if matches!(
                                    mid,
                                    CommittedEvent::Draw { player, .. } if *player == controller
                                ) {
                                    saw_draw = true;
                                    break;
                                }
                                if matches!(
                                    mid,
                                    CommittedEvent::SpellCast { .. }
                                        | CommittedEvent::PublishedCastIdentityV1 { .. }
                                ) {
                                    break;
                                }
                            }
                            saw_draw
                        };
                        if draw_ok {
                            spell_discard_indices.extend(run);
                        }
                    }
                }
                _ => {}
            }
        }

        let mut resolution_effect_discard_indices = HashSet::new();
        if let Some(s1b) = super::read_state(state) {
            for &(ctrl, idx, obj) in &s1b.resolution_effect_discards {
                if ctrl != controller {
                    continue;
                }
                if idx < turn_start
                    || !matches!(
                        history.get(idx),
                        Some(CommittedEvent::ZoneChange {
                            object,
                            from: Zone::Hand,
                            to,
                            controller_before,
                        }) if *object == obj
                            && *controller_before == ctrl
                            && !matches!(to, Zone::Hand | Zone::Battlefield | Zone::Stack)
                    )
                {
                    return Err(
                        "resolution-effect discard record does not match event history".into(),
                    );
                }
                resolution_effect_discard_indices.insert(idx);
            }
        }

        if copy_discards.iter().any(|idx| {
            spell_discard_indices.contains(idx) || resolution_effect_discard_indices.contains(idx)
        }) {
            return Err("copy-entry discard overlaps a non-copy-entry discard".into());
        }

        let mut activated_discards: usize = 0;
        let mut resolved_effect_discard_uses: usize = 0;
        let mut counted_links = HashSet::new();
        let mut live_battlefield_links = HashSet::new();
        for (id, live) in state.objects.iter() {
            if live.zone == Zone::Battlefield {
                live_battlefield_links.insert(ObjectLinkV4 {
                    object: id,
                    zone_change_count: live.zone_change_count,
                });
            }
            let uses = activated_discard_uses_for_object(state, id, live, controller, turn_slice);
            if uses > 0 {
                counted_links.insert(ObjectLinkV4 {
                    object: id,
                    zone_change_count: live.zone_change_count,
                });
                activated_discards = activated_discards.saturating_add(uses);
            }
            let effect_uses =
                activated_effect_discard_uses_for_object(state, id, live, controller, turn_slice);
            resolved_effect_discard_uses = resolved_effect_discard_uses.saturating_add(effect_uses);
        }
        if let Some(s1b) = super::read_state(state) {
            let mut counted_departed_effect_keys = HashSet::new();
            for &(link, affected_player, ability_index, raw_uses) in
                &s1b.departed_effect_discard_uses
            {
                if affected_player != controller
                    || live_battlefield_links.contains(&link)
                    || !counted_departed_effect_keys.insert((link, ability_index))
                {
                    continue;
                }
                let per_use =
                    departed_effect_discard_per_use(state, link, ability_index, controller);
                let pending_on_stack =
                    pending_effect_discard_stack_items(state, link, ability_index, None);
                let countered = countered_effect_discard_uses(turn_slice, link, ability_index);
                let resolved_uses = (raw_uses as usize)
                    .saturating_sub(pending_on_stack)
                    .saturating_sub(countered);
                resolved_effect_discard_uses = resolved_effect_discard_uses
                    .saturating_add(per_use.saturating_mul(resolved_uses));
            }
        }
        let pending_effect_discard = state
            .engine
            .pending_discard
            .as_ref()
            .filter(|p| {
                p.player == controller
                    && matches!(
                        p.resume,
                        crate::engine::DiscardResume::None
                            | crate::engine::DiscardResume::FinishAbilityResolution { .. }
                            | crate::engine::DiscardResume::FinishEffectContinuation { .. }
                    )
            })
            .map_or(0usize, |p| p.count as usize);
        resolved_effect_discard_uses =
            resolved_effect_discard_uses.saturating_sub(pending_effect_discard);
        if let Some(s1b) = super::read_state(state) {
            for &(link, owner_ctrl, uses) in &s1b.departed_discard_uses {
                if owner_ctrl == controller && counted_links.insert(link) {
                    activated_discards = activated_discards.saturating_add(usize::from(uses));
                }
            }
        }
        let non_spell_discards = hand_discard_indices
            .difference(&spell_discard_indices)
            .count();
        let extra_resolution_discards = resolution_effect_discard_indices
            .difference(&spell_discard_indices)
            .count();
        let effective_resolution_discards =
            extra_resolution_discards.max(resolved_effect_discard_uses);
        if non_spell_discards
            < effective_resolution_discards
                .saturating_add(activated_discards)
                .saturating_add(copy_discards.len())
        {
            return Err("copy-entry discard violates same-turn hand-discard conservation".into());
        }
    }
    Ok(())
}

pub(crate) fn commit(
    state: &mut GameState,
    choice: &ChoiceV1,
    operation: u32,
    chosen: u32,
    entry: usize,
    object: ObjectId,
    named: NameV1,
) -> Result<(), String> {
    let source = AbilitySourceContractV4::capture(state, object);
    let actual = state.engine.event_history.get(entry);
    if !entry_matches(choice, actual, source)
        || !named_copy_matches(state, choice, source, entry)
        || named.printed_name().is_none()
        || (choice.original.is_none() && !state.objects.get(object).v4.is_token)
    {
        return Err("Meddling Mage name has no matching actual entry".into());
    }
    let link = ObjectLinkV4 {
        object,
        zone_change_count: source.zone_change_count,
    };
    s1b_state(state).mage_names.push((link, named));
    record(
        state,
        JournalV1::Named {
            operation,
            chosen,
            entry_history_index: u32::try_from(entry)
                .map_err(|_| "Meddling Mage entry index overflow")?,
            source,
            named,
        },
    );
    Ok(())
}
