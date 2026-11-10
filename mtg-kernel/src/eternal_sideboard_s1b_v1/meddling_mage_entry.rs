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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetamorphCopyV1 {
    pub(crate) physical: AbilitySourceContractV4,
    pub(crate) copied: AbilitySourceContractV4,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cast_at: Option<usize>,
    stack_item: StackItemId,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cast_at: Option<usize>,
    stack_item: StackItemId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    vial: Option<vial::CopyOriginV1>,
}

pub(crate) fn physical_graveyard_metamorph(state: &GameState, object: ObjectId) -> bool {
    cfg!(feature = "eternal-fixtures")
        && state.objects.try_get(object).is_some_and(|live| {
            live.zone == Zone::Graveyard
                && !live.v4.is_token
                && live.v4.face_down_v1.is_none()
                && live.spell_copy_origin.is_none()
                && card_id_by_name("Phyrexian Metamorph") == Some(live.card_def)
        })
}

pub(crate) fn reanimate_metamorph_waiting(
    state: &GameState,
    item: &crate::state::StackItem,
) -> bool {
    use crate::state::{StackItemKind, Target};
    item.kind == StackItemKind::Spell
        && !item.is_copy
        && item.v4.source_contract.is_some_and(|source| {
            source.zone == Zone::Stack && card_id_by_name("Reanimate") == Some(source.card_def)
        })
        && matches!(item.targets.as_slice(), [Target::Object(object)]
            if physical_graveyard_metamorph(state, *object))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NoncastProducer {
    Reanimate,
    Vial,
    ShowAndTell,
}

/// Some registered definitions require an as-enters choice or a copy overlay
/// that this entry path cannot install.
fn noncast_copy_definition(card_def: u16, producer: NoncastProducer) -> bool {
    owns_definition(card_def)
        || CARD_DEFS
            .get(usize::from(card_def))
            .is_some_and(|definition| {
                (definition.name == "Wall of Blossoms"
                    || producer == NoncastProducer::Vial
                        && definition.name == crate::eternal_legacy_e8a_v1::AETHER_VIAL
                    || producer == NoncastProducer::ShowAndTell
                        && definition.name == "Containment Priest")
                    && definition.has_full_support()
            })
}

impl NoncastCopyChoiceV1 {
    pub(crate) fn capture(
        state: &GameState,
        ctx: &crate::effect::ExecCtx,
        object: ObjectId,
        item: &crate::state::StackItem,
    ) -> Result<Option<Self>, String> {
        if !reanimate_metamorph_waiting(state, item)
            || ctx.targets.as_slice() != [crate::state::Target::Object(object)]
            || ctx.stack_item_id != Some(item.v4.stack_item_id)
            || ctx.source != item.source
            || ctx.controller != item.controller
            || ctx.targets != item.targets
            || ctx.target_contracts != item.v4.target_contracts
            || !ctx.target_incarnation_matches(0, state)
        {
            return Err("noncast Metamorph lost its actual paid Reanimate target".into());
        }
        let candidates =
            Self::candidates(state, object, ctx.controller, NoncastProducer::Reanimate)?;
        if candidates.is_empty() {
            return Ok(None);
        }
        let choice = Self {
            physical: AbilitySourceContractV4::capture(state, object),
            producer: AbilitySourceContractV4::capture(state, ctx.source),
            candidates,
            cast_at: Some(paid_cast_index(state, item.v4.stack_item_id)?),
            stack_item: item.v4.stack_item_id,
            vial: None,
        };
        if !choice.copy(choice.candidates[0]).cast_matches(state) {
            return Err("noncast Metamorph lost its actual paid Reanimate publication".into());
        }
        Ok(Some(choice))
    }

    fn candidates(
        state: &GameState,
        object: ObjectId,
        controller: PlayerId,
        producer: NoncastProducer,
    ) -> Result<Vec<AbilitySourceContractV4>, String> {
        if crate::eternal_legacy_e7a_v1::active_dress_down_timestamp(state, Some(object)).is_some()
        {
            return Ok(Vec::new());
        }
        let mut candidates = Vec::new();
        for binding in crate::effect::metamorph_copy_candidates(state, controller)? {
            let live = state.objects.get(binding.object);
            if live.zone != Zone::Battlefield
                || live.v4.face_down_v1.is_some()
                || live.v4.face_index != 0
                || live.v4.is_token
                || crate::eternal_vintage_v1::active_copy(state, binding.object)
                || !noncast_copy_definition(live.card_def, producer)
            {
                return Err(
                    "noncast Metamorph copy menu contains an unsupported entry form".into(),
                );
            }
            candidates.push(AbilitySourceContractV4::capture(state, binding.object));
        }
        Ok(candidates)
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

    fn cast_matches(self, state: &GameState) -> bool {
        let Some(cast_at) = self.cast_at.filter(|_| self.vial.is_none()) else {
            return false;
        };
        let (source, expected, entrant_zone) = match (self.reanimated_by, self.show_and_tell) {
            (Some(source), None) => (source, "Reanimate", Zone::Graveyard),
            (None, Some(show)) => (show.producer, "Show and Tell", Zone::Hand),
            (None, None) => (self.physical, "Phyrexian Metamorph", Zone::Stack),
            (Some(_), Some(_)) => return false,
        };
        card_id_by_name(expected) == Some(source.card_def)
            && card_id_by_name("Phyrexian Metamorph") == Some(self.physical.card_def)
            && self.physical.zone == entrant_zone
            && cast_receipt_matches(state, cast_at, self.stack_item, source)
    }

    fn entrant_matches(self, entrant: AbilitySourceContractV4) -> bool {
        let (producer, controller) = if let Some(vial) = self.vial {
            (Some(NoncastProducer::Vial), vial.source.controller)
        } else if let Some(reanimate) = self.reanimated_by {
            (Some(NoncastProducer::Reanimate), reanimate.controller)
        } else if self.show_and_tell.is_some() {
            (Some(NoncastProducer::ShowAndTell), self.physical.owner)
        } else {
            (None, self.physical.controller)
        };
        self.copied.zone == Zone::Battlefield
            && self.copied.source != self.physical.source
            && producer.map_or_else(
                || owns_definition(self.copied.card_def),
                |producer| noncast_copy_definition(self.copied.card_def, producer),
            )
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
            let card_def = metamorph_copy.map_or(object.card_def, |copy| copy.copied.card_def);
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
    // OmenDef has no name field. Roost Seek is the only generated Omen.
    (def.name == "Sagu Wildling" && def.omen.is_some()).then_some("Roost Seek")
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
    copy: MetamorphCopyV1,
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
    copy: MetamorphCopyV1,
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
    anchor < entry
        && copy.entrant_matches(entrant)
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
    if !copy_entry_matches(state, copy, entrant, entry_history_index, false)
        || AbilitySourceContractV4::capture(state, copy.copied.source) != copy.copied
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
    let Some(copy) = choice.metamorph_copy else {
        return true;
    };
    matches!(state.engine.event_history.get(entry + 1), Some(CommittedEvent::MeddlingMageEntryV1 { journal })
            if matches!(journal.as_ref(), JournalV1::MetamorphCopied {
                entry_history_index, entrant: recorded, copy: actual,
            } if *entry_history_index == entry && *recorded == entrant && *actual == copy))
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
                && choice
                    .metamorph_copy
                    .map_or(original.card_def == source.card_def, |copy| {
                        copy.physical == original
                            && !choice.suppressed
                            && copy.entrant_matches(source)
                    })
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
    let mut show_choices = HashSet::new();
    let mut show_copies = HashSet::new();
    for (index, event) in state.engine.event_history.iter().enumerate() {
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
            JournalV1::ShowAndTellMetamorphChosen { copy } => {
                if !copy
                    .show_and_tell
                    .is_some_and(|show| show.chosen_at == index as usize)
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
                if !operations
                    .get(operation)
                    .is_some_and(|(names_v2, _)| *names_v2 == choice.names_v2)
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
                if entry_history_index.checked_add(1) != Some(index as usize)
                    || !copy_entry_matches(state, *copy, *entrant, *entry_history_index, true)
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
