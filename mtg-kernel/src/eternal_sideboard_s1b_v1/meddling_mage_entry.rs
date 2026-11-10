//! Native as-enters name choice for Meddling Mage.
use super::{nameable_nonland, s1b_state, MEDDLING_MAGE};
use crate::card_def::{card_id_by_name, CardType, CARD_DEFS};
use crate::engine::Decision;
use crate::event::{CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::linked_aura_operation_v1::{GeneralAnswerV36, GeneralChoiceV36, OperationV1};
use crate::state::{AbilitySourceContractV4, GameState, ObjectLinkV4, Zone};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::hash::{Hash, Hasher};

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
) -> Option<ChoiceV1> {
    let (controller, card_def, original) = match proposal {
        ProposedEvent::ZoneChange(change) if change.to_zone == Zone::Battlefield => {
            let object = state.objects.try_get(change.object)?;
            if object.zone == Zone::Battlefield
                || !owns_definition(object.card_def)
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
                object.card_def,
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
    pub(crate) choice: GeneralChoiceV36,
    pub(crate) operation_history_index: u32,
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
        named: u16,
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

pub(crate) fn decision(choice: &ChoiceV1, group: Option<u16>) -> Result<Decision, String> {
    let groups = nameable_nonland();
    let options = if let Some(group) = group {
        groups
            .get(usize::from(group))
            .ok_or("Meddling Mage name group absent")?
            .iter()
            .map(|&id| CARD_DEFS[usize::from(id)].object_name.to_owned())
            .collect()
    } else {
        groups
            .iter()
            .map(|ids| {
                CARD_DEFS[usize::from(ids[0])]
                    .object_name
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

pub(crate) fn valid_index(group: Option<u16>, index: u16) -> bool {
    let groups = nameable_nonland();
    match group {
        None => groups.get(usize::from(index)).is_some(),
        Some(group) => groups
            .get(usize::from(group))
            .is_some_and(|names| names.get(usize::from(index)).is_some()),
    }
}

pub(crate) fn name(group: u16, index: u16) -> Option<u16> {
    nameable_nonland()
        .get(usize::from(group))?
        .get(usize::from(index))
        .copied()
}

fn is_nameable(named: u16) -> bool {
    CARD_DEFS.get(usize::from(named)).is_some_and(|def| {
        def.is_executable() && !def.is_token && !def.types.contains(&CardType::Land)
    })
}

pub(crate) fn requires_successor(state: &GameState) -> bool {
    state
        .engine
        .event_history
        .scan_for_v1(&[crate::event_history_v1::EventKindV1::MeddlingMageEntryV1])
        .iter()
        .any(|event| matches!(event, CommittedEvent::MeddlingMageEntryV1 { .. }))
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
                && original.card_def == source.card_def
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
    for (index, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::MeddlingMageEntryV1 { journal } = event else {
            continue;
        };
        let index = u32::try_from(index).map_err(|_| "Meddling Mage event index overflow")?;
        match journal.as_ref() {
            JournalV1::Started { .. } => {
                operations.insert(index, false);
            }
            JournalV1::Chosen {
                operation,
                choice,
                group,
                index: name_index,
            } => {
                if !operations.contains_key(operation)
                    || choice.suppressed
                    || !owns_definition(choice.card_def)
                    || !valid_index(Some(*group), *name_index)
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
                let Some(has_name) = operations.get_mut(operation) else {
                    return Err("Meddling Mage name has no open operation".into());
                };
                if choices.remove(chosen) != Some(*operation)
                    || *entry_history_index <= *chosen
                    || *entry_history_index >= index
                    || !entry_matches(choice, actual, *source)
                    || name(*group, *name_index) != Some(*named)
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
            JournalV1::Completed { operation } => {
                if operations.remove(operation) != Some(true)
                    || choices.values().any(|pending| pending == operation)
                {
                    return Err(
                        "Meddling Mage operation ended without an actual named entry".into(),
                    );
                }
            }
        }
    }
    if names.len() != entrants.len() {
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
                    Some(JournalV1::Started { operation: recorded, original_sha256 })
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
    named: u16,
) -> Result<(), String> {
    let source = AbilitySourceContractV4::capture(state, object);
    let actual = state.engine.event_history.get(entry);
    if !entry_matches(choice, actual, source)
        || !is_nameable(named)
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
