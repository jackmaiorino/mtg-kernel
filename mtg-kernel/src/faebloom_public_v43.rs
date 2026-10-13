//! Public original spell identity and current visible relations, never private journal payloads.
use crate::event::CommittedEvent;
use crate::faebloom_trick_v43::JournalV43;
use crate::rl::{CardStableRefV1, PlayerSeatV1, RlContractError};
use crate::state::{GameState, StackItem, StackSourceContractV4, StackTargetContractV4, Zone};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "mtg-kernel/faebloom-public/v43";
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StageV43 {
    OnStack,
    Created,
    Countered,
    Departed,
    Uncollected,
    Waiting,
    Dropped,
    Tapped,
    NoLegalTarget,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpellV43 {
    pub ordinal: u32,
    pub original_source: CardStableRefV1,
    pub caster: PlayerSeatV1,
    pub copied_spell: bool,
    pub stage: StageV43,
    pub stack_ordinal: Option<u32>,
    pub created_tokens: Vec<Option<CardStableRefV1>>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReflexiveV43 {
    pub ordinal: u32,
    pub original_source: CardStableRefV1,
    pub controller: PlayerSeatV1,
    pub stage: StageV43,
    pub stack_ordinal: Option<u32>,
    pub current_target: Option<CardStableRefV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV43 {
    pub schema: String,
    pub producer_sha256: String,
    pub card_db_hash: u64,
    pub roots: Vec<crate::rules_meaning_faebloom_v43::MeaningV43>,
    pub spells: Vec<SpellV43>,
    pub reflexives: Vec<ReflexiveV43>,
}
fn j(e: &CommittedEvent) -> Option<&JournalV43> {
    match e {
        CommittedEvent::FaebloomJournalV43 { journal } => Some(journal),
        _ => None,
    }
}
fn original_ref(source: StackSourceContractV4) -> CardStableRefV1 {
    CardStableRefV1 {
        arena_id: source.source.0,
        card_db_id: source.card_def,
        owner: source.owner.into(),
        controller: source.controller.into(),
        zone: Zone::Stack,
        zone_change_count: source.zone_change_count,
    }
}
pub fn producer_sha256() -> String {
    let mut h = Sha256::new();
    for source in [
        SCHEMA,
        include_str!("faebloom_public_v43.rs"),
        include_str!("faebloom_trick_v43.rs"),
        include_str!("rules_meaning_faebloom_v43.rs"),
        include_str!("engine.rs"),
        include_str!("event.rs"),
        include_str!("effect.rs"),
        include_str!("trigger.rs"),
        include_str!("rl.rs"),
        include_str!("state.rs"),
        include_str!("sower_restriction_capture_v1.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("controlled_creature_grants_v1.rs"),
        include_str!("standard_cards_v1.rs"),
        include_str!("card_def.rs"),
        include_str!("../build.rs"),
    ] {
        h.update([0]);
        h.update(source.as_bytes());
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}
fn stack_ordinal(state: &GameState, item: &StackItem) -> Option<u32> {
    state
        .stack
        .iter()
        .position(|i| i.v4.stack_item_id == item.v4.stack_item_id)
        .and_then(|i| u32::try_from(i).ok())
}
fn current_target(
    state: &GameState,
    item: &StackItem,
) -> Result<Option<CardStableRefV1>, RlContractError> {
    let Some(StackTargetContractV4::Object {
        object,
        owner,
        zone: Zone::Battlefield,
        zone_change_count,
        ..
    }) = item.v4.target_contracts.first()
    else {
        return Ok(None);
    };
    let Some(live) = state.objects.try_get(*object).filter(|o| {
        o.zone == Zone::Battlefield
            && o.zone_change_count == *zone_change_count
            && o.owner == *owner
    }) else {
        return Ok(None);
    };
    // A current face-down recipient uses the existing neutral sentinel rule.
    // After any departure, this function does not inspect a private definition.
    crate::sower_restriction_capture_v1::masked_ref(*object, live).map(Some)
}
pub fn capture(state: &GameState) -> Result<FactsV43, RlContractError> {
    crate::faebloom_trick_v43::validate_state(state).map_err(RlContractError)?;
    let roots = if let Some(id) = crate::card_def::card_id_by_name("Faebloom Trick") {
        let selector =
            crate::rules_meaning_faebloom_v43::live_selector().map_err(RlContractError)?;
        [
            crate::rules_meaning_faebloom_v43::RootV43::SpellCreation,
            crate::rules_meaning_faebloom_v43::RootV43::ReflexiveTap,
        ]
        .into_iter()
        .map(|root| {
            crate::rules_meaning_faebloom_v43::meaning(&selector, id, root).map_err(RlContractError)
        })
        .collect::<Result<Vec<_>, _>>()?
    } else {
        Vec::new()
    };
    let mut spells = Vec::new();
    let mut reflexives = Vec::new();
    for (index, event) in state.engine.event_history.iter().enumerate() {
        match j(event) {
            Some(JournalV43::Published { item }) => {
                let source = item.v4.source_contract.ok_or_else(|| {
                    RlContractError("Faebloom public spell lost original source".into())
                })?;
                let input =
                    state
                        .engine
                        .event_history
                        .iter()
                        .enumerate()
                        .find_map(|(i, e)| match j(e) {
                            Some(JournalV43::RemovalInput { publication, .. })
                                if *publication == index as u64 =>
                            {
                                Some(i as u64)
                            }
                            _ => None,
                        });
                let mut tokens = Vec::new();
                let mut stage = StageV43::OnStack;
                if let Some(input) = input {
                    for e in &state.engine.event_history {
                        match j(e) {
                            Some(JournalV43::Created {
                                input: owned,
                                token_events,
                            }) if *owned == input => {
                                stage = StageV43::Created;
                                for at in token_events {
                                    let Some(CommittedEvent::CreateToken {
                                        object,
                                        controller,
                                        ..
                                    }) = state.engine.event_history.get(*at as usize)
                                    else {
                                        return Err(RlContractError(
                                            "Faebloom token event changed".into(),
                                        ));
                                    };
                                    // The actual CreateToken allocator starts its new object at generation zero.
                                    let live = state.objects.try_get(*object).filter(|o| {
                                        o.zone == Zone::Battlefield
                                            && o.zone_change_count == 0
                                            && o.owner == *controller
                                    });
                                    tokens.push(
                                        live.map(|o| {
                                            crate::sower_restriction_capture_v1::masked_ref(
                                                *object, o,
                                            )
                                        })
                                        .transpose()?,
                                    );
                                }
                            }
                            Some(JournalV43::Countered { input: owned }) if *owned == input => {
                                stage = StageV43::Countered
                            }
                            Some(JournalV43::OtherDeparted { input: owned }) if *owned == input => {
                                stage = StageV43::Departed
                            }
                            _ => {}
                        }
                    }
                }
                spells.push(SpellV43 {
                    ordinal: u32::try_from(spells.len())
                        .map_err(|_| RlContractError("Faebloom spell ordinal".into()))?,
                    original_source: original_ref(source),
                    caster: item.controller.into(),
                    copied_spell: item.is_copy,
                    stage,
                    stack_ordinal: stack_ordinal(state, item),
                    created_tokens: tokens,
                });
            }
            Some(JournalV43::ReflexiveCaptured { source, .. }) => {
                let mut stage = StageV43::Uncollected;
                let mut stack = None;
                let mut target = None;
                for e in &state.engine.event_history {
                    match j(e) {
                        Some(JournalV43::Collected { capture }) if *capture == index as u64 => {
                            stage = StageV43::Waiting
                        }
                        Some(JournalV43::Dropped { capture, .. }) if *capture == index as u64 => {
                            stage = StageV43::Dropped
                        }
                        Some(JournalV43::TriggerPublished { capture, item })
                            if *capture == index as u64 =>
                        {
                            stage = StageV43::OnStack;
                            stack = stack_ordinal(state, item);
                            target = current_target(state, item)?;
                            if let Some((input,_))=state.engine.event_history.iter().enumerate().find(|(_,e)|matches!(j(e),Some(JournalV43::RemovalInput {item:owned,..}) if owned.v4.stack_item_id==item.v4.stack_item_id)) {
                                for e in &state.engine.event_history {
                                    match j(e) {
                                        Some(JournalV43::Tapped {input:owned,..}) if *owned==input as u64=>stage=StageV43::Tapped,
                                        Some(JournalV43::NoLegalTarget {input:owned}) if *owned==input as u64=>stage=StageV43::NoLegalTarget,
                                        Some(JournalV43::Countered {input:owned}) if *owned==input as u64=>stage=StageV43::Countered,
                                        _=>{}
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                reflexives.push(ReflexiveV43 {
                    ordinal: u32::try_from(reflexives.len())
                        .map_err(|_| RlContractError("Faebloom reflexive ordinal".into()))?,
                    original_source: original_ref(*source),
                    controller: source.controller.into(),
                    stage,
                    stack_ordinal: stack,
                    current_target: target,
                });
            }
            _ => {}
        }
    }
    Ok(FactsV43 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        roots,
        spells,
        reflexives,
    })
}
pub fn validate(state: &GameState, facts: &FactsV43) -> Result<(), RlContractError> {
    if *facts != capture(state)? {
        return Err(RlContractError(
            "forged Faebloom current/public facts".into(),
        ));
    }
    Ok(())
}

/// Only the exact genuine live item can request a historical source relation.
pub fn stack_source_reference(
    state: &GameState,
    item: &StackItem,
) -> Result<Option<CardStableRefV1>, RlContractError> {
    if item
        .inline_effect
        .as_ref()
        .and_then(crate::faebloom_trick_v43::capture_of)
        .is_none()
        && !item
            .v4
            .source_contract
            .is_some_and(|s| crate::faebloom_trick_v43::selected_definition(s.card_def))
    {
        return Ok(None);
    }
    crate::faebloom_trick_v43::validate_state(state).map_err(RlContractError)?;
    if state
        .stack
        .iter()
        .filter(|row| row.v4.stack_item_id == item.v4.stack_item_id && *row == item)
        .count()
        != 1
    {
        return Err(RlContractError(
            "Faebloom public source supplied a detached row".into(),
        ));
    }
    Ok(Some(original_ref(item.v4.source_contract.ok_or_else(
        || RlContractError("Faebloom source contract absent".into()),
    )?)))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn absence_public_facts_are_empty_readonly_and_journal_free_both_seats() {
        for actor in [crate::ids::PlayerId::P0, crate::ids::PlayerId::P1] {
            let state = crate::extra_turn_v1::tests::ready(actor);
            let wire = serde_json::to_vec(&state).unwrap();
            let facts = capture(&state).unwrap();
            assert!(facts.spells.is_empty());
            assert!(facts.reflexives.is_empty());
            assert_eq!(serde_json::to_vec(&state).unwrap(), wire);
            let json = serde_json::to_string(&facts).unwrap();
            assert!(!json.contains("journal"));
            assert!(!json.contains("publication"));
            assert!(!json.contains("source_contract"));
        }
    }
}
