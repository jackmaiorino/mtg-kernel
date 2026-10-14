//! Hunter-specific public facts. Private library/input journals never cross this boundary.
use crate::effect::PendingEffectChoice;
use crate::event::CommittedEvent;
use crate::high_society_hunter_custody_v45::{self as custody, JournalV45};
use crate::rl::{CardStableRefV1, PlayerSeatV1, RlContractError};
use crate::rules_meaning_high_society_hunter_v1 as meaning;
use crate::state::{AbilitySourceContractV4, GameState, Zone};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "mtg-kernel/high-society-hunter/public/v45";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PhaseV45 {
    Fresh,
    Held,
    Published,
    ChoosingSacrifice,
    AcceptedSacrifice,
    Completed,
    Countered,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncarnationV45 {
    pub object: u32,
    pub generation: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OccurrenceV45 {
    /// Ordinal among public Hunter occurrences, never a private journal index.
    pub occurrence: usize,
    pub source: CardStableRefV1,
    pub current_original_source: Option<CardStableRefV1>,
    pub original_controller: PlayerSeatV1,
    pub root: meaning::RootV1,
    pub phase: PhaseV45,
    /// Exact public death incarnation without guessing its hidden definition.
    pub deceased: Option<IncarnationV45>,
    pub sacrificed: Option<IncarnationV45>,
    pub sacrifice_paid: bool,
    pub source_counter_added: bool,
    /// An attempt includes drawing from an empty library. No drawn identity is exported.
    pub draw_attempted: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MenuV45 {
    pub occurrence: usize,
    pub chooser: PlayerSeatV1,
    pub source: CardStableRefV1,
    pub ordered_legal: Vec<CardStableRefV1>,
    pub may_finish: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV45 {
    pub schema: String,
    pub producer_sha256: String,
    pub card_db_hash: u64,
    pub meanings: Vec<meaning::MeaningV1>,
    pub occurrences: Vec<OccurrenceV45>,
    pub menu: Option<MenuV45>,
    pub trigger_order: Vec<TriggerOrderV45>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerOrderV45 {
    pub controller: PlayerSeatV1,
    pub group_position: usize,
    pub occurrence: usize,
}
pub fn producer_sha256() -> String {
    let mut hash = Sha256::new();
    for source in [
        SCHEMA,
        include_str!("high_society_hunter_public_v45.rs"),
        include_str!("high_society_hunter_custody_v45.rs"),
        include_str!("high_society_hunter_v1.rs"),
        include_str!("creature_death_capture_v1.rs"),
        include_str!("sower_restriction_capture_v1.rs"),
        include_str!("effect/standard_selection_v1.rs"),
        include_str!("effect.rs"),
        include_str!("engine.rs"),
        include_str!("event.rs"),
        include_str!("state.rs"),
        include_str!("standard_cards_v1.rs"),
        include_str!("trigger.rs"),
    ] {
        hash.update([0]);
        hash.update(source.as_bytes());
    }
    hash.update([1]);
    hash.update(meaning::producer_sha256().as_bytes());
    hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
}
pub(crate) fn captured_source(source: AbilitySourceContractV4) -> CardStableRefV1 {
    // The real capture hook required face-up active printed abilities.
    // This is original public identity, independent of any later hidden incarnation.
    CardStableRefV1 {
        arena_id: source.source.0,
        card_db_id: source.card_def,
        owner: source.owner.into(),
        controller: source.controller.into(),
        zone: source.zone,
        zone_change_count: source.zone_change_count,
    }
}
pub(crate) fn stack_source_reference(
    state: &GameState,
    item: &crate::state::StackItem,
) -> Result<Option<CardStableRefV1>, RlContractError> {
    if !custody::selected(item) {
        return Ok(None);
    }
    custody::validate(state).map_err(RlContractError)?;
    if state
        .stack
        .iter()
        .filter(|row| row.v4.stack_item_id == item.v4.stack_item_id)
        .count()
        != 1
        || !state.stack.iter().any(|row| row == item)
    {
        return Err(RlContractError(
            "Hunter public source requires its genuine full live row".into(),
        ));
    }
    let source = item
        .v4
        .ability_source_contract
        .ok_or_else(|| RlContractError("Hunter source contract absent".into()))?;
    custody::normalize_root(
        state,
        item.inline_effect
            .as_ref()
            .ok_or_else(|| RlContractError("Hunter root absent".into()))?,
        Some(source),
    )
    .map_err(RlContractError)?;
    Ok(Some(captured_source(source)))
}
pub(crate) fn pending_source_reference(
    state: &GameState,
    pending: &crate::trigger::PendingTrigger,
) -> Result<Option<CardStableRefV1>, RlContractError> {
    let Some((capture, attack)) = custody::binding(&pending.effect) else {
        return Ok(None);
    };
    custody::validate(state).map_err(RlContractError)?;
    let (source, actual) = custody::source_at(state, capture).map_err(RlContractError)?;
    if attack != actual
        || pending.source_contract != Some(source)
        || pending.controller != source.controller
        || pending.source != source.source
    {
        return Err(RlContractError(
            "Hunter waiting source changed original occurrence".into(),
        ));
    }
    Ok(Some(captured_source(source)))
}
pub fn capture(state: &GameState) -> Result<FactsV45, RlContractError> {
    let selector = meaning::live_selector()
        .map_err(|_| RlContractError("Hunter V45 requires Foundations".into()))?;
    custody::validate(state).map_err(RlContractError)?;
    let mut meanings = Vec::new();
    for (id, _) in crate::card_def::CARD_DEFS.iter().enumerate() {
        let id =
            u16::try_from(id).map_err(|_| RlContractError("Hunter catalog exceeds u16".into()))?;
        if !crate::high_society_hunter_v1::owns_definition(id) {
            continue;
        }
        for root in [
            meaning::RootV1::CreatureSpell,
            meaning::RootV1::Attack,
            meaning::RootV1::OtherNontokenDeath,
        ] {
            meanings.push(
                meaning::meaning(&selector, id, root)
                    .map_err(|_| RlContractError("Hunter printed meaning changed".into()))?,
            );
        }
    }
    let mut occurrences = Vec::new();
    let mut menu = None;
    for (occurrence, capture) in custody::captures(state).into_iter().enumerate() {
        let (source, attack) = custody::source_at(state, capture).map_err(RlContractError)?;
        let deceased = match &state.engine.event_history[capture as usize] {
            CommittedEvent::ControlledCreatureDeathCaptureV1 { died, .. } => Some(IncarnationV45 {
                object: died.object.0,
                generation: died.zone_change_count,
            }),
            _ => None,
        };
        let publication = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .find_map(|(at, event)| match event {
                CommittedEvent::HighSocietyHunterJournalV45 { journal } => match journal.as_ref() {
                    JournalV45::Published {
                        capture: original, ..
                    } if *original == capture => Some(at as u64),
                    _ => None,
                },
                _ => None,
            });
        let collected=state.engine.event_history.iter().any(|event|matches!(event,CommittedEvent::HighSocietyHunterAttackCollectedV1{capture_history_index}|CommittedEvent::ControlledCreatureDeathCaptureCollectedV1{capture_history_index}if *capture_history_index==capture));
        let mut row = OccurrenceV45 {
            occurrence,
            source: captured_source(source),
            current_original_source: state
                .objects
                .try_get(source.source)
                .filter(|live| {
                    live.zone == Zone::Battlefield
                        && live.owner == source.owner
                        && live.zone_change_count == source.zone_change_count
                })
                .map(|live| crate::sower_restriction_capture_v1::masked_ref(source.source, live))
                .transpose()?,
            original_controller: source.controller.into(),
            root: if attack {
                meaning::RootV1::Attack
            } else {
                meaning::RootV1::OtherNontokenDeath
            },
            phase: if publication.is_some() {
                PhaseV45::Published
            } else if collected {
                PhaseV45::Held
            } else {
                PhaseV45::Fresh
            },
            deceased,
            sacrificed: None,
            sacrifice_paid: false,
            source_counter_added: false,
            draw_attempted: false,
        };
        if let Some(publication) = publication {
            let removal = state
                .engine
                .event_history
                .iter()
                .enumerate()
                .find_map(|(at, event)| match event {
                    CommittedEvent::HighSocietyHunterJournalV45 { journal } => {
                        match journal.as_ref() {
                            JournalV45::RemovalInput {
                                publication: original,
                                countered,
                                ..
                            } if *original == publication => Some((at as u64, *countered)),
                            _ => None,
                        }
                    }
                    _ => None,
                });
            if let Some((removal, countered)) = removal {
                let closed = state
                    .engine
                    .event_history
                    .iter()
                    .find_map(|event| match event {
                        CommittedEvent::HighSocietyHunterJournalV45 { journal } => {
                            match journal.as_ref() {
                                JournalV45::Closed {
                                    removal: original,
                                    operation,
                                } if *original == removal => Some(*operation),
                                _ => None,
                            }
                        }
                        _ => None,
                    });
                if let Some(operation) = closed {
                    row.phase = if countered {
                        PhaseV45::Countered
                    } else {
                        PhaseV45::Completed
                    };
                    if let Some(operation) = operation {
                        let CommittedEvent::HighSocietyHunterJournalV45 { journal } =
                            &state.engine.event_history[operation as usize]
                        else {
                            return Err(RlContractError("Hunter operation absent".into()));
                        };
                        match journal.as_ref() {
                            JournalV45::CostOperation {
                                input,
                                departure,
                                counter,
                                ..
                            } => {
                                let CommittedEvent::HighSocietyHunterJournalV45 { journal } =
                                    &state.engine.event_history[*input as usize]
                                else {
                                    return Err(RlContractError("Hunter cost input absent".into()));
                                };
                                let JournalV45::CostInput { pending, .. } = journal.as_ref() else {
                                    return Err(RlContractError(
                                        "Hunter cost input changed".into(),
                                    ));
                                };
                                let [crate::effect::EffectFrame::ApplySelectedObjectsV1 {
                                    selected,
                                    ..
                                }] = pending.frames.as_slice()
                                else {
                                    return Err(RlContractError(
                                        "Hunter accepted consequence absent".into(),
                                    ));
                                };
                                row.sacrificed = selected.first().map(|b| IncarnationV45 {
                                    object: b.object.0,
                                    generation: b.expected_zone_change_count,
                                });
                                row.sacrifice_paid = departure.is_some();
                                row.source_counter_added = counter.is_some();
                            }
                            JournalV45::DrawOperation { .. } => row.draw_attempted = true,
                            _ => {
                                return Err(RlContractError("Hunter operation changed kind".into()))
                            }
                        }
                    }
                } else if let Some(pending) = state.engine.pending_effect.as_ref().filter(|p| {
                    p.resolving_item.v4.high_society_hunter_started_v45.0 == Some(publication)
                }) {
                    row.phase = if pending.choice.is_some() {
                        PhaseV45::ChoosingSacrifice
                    } else {
                        PhaseV45::AcceptedSacrifice
                    };
                    if let Some(paid) =
                        custody::linked_paid_incarnation(state, pending).map_err(RlContractError)?
                    {
                        row.sacrifice_paid = true;
                        row.sacrificed = Some(IncarnationV45 {
                            object: paid.object.0,
                            generation: paid.expected_zone_change_count,
                        });
                    }
                    if let Some(PendingEffectChoice::SelectTargets {
                        player,
                        legal,
                        min_targets,
                        selected,
                        ..
                    }) = &pending.choice
                    {
                        let ordered_legal = legal
                            .iter()
                            .map(|candidate| {
                                let crate::state::Target::Object(object) = candidate.target else {
                                    return Err(RlContractError(
                                        "Hunter sacrifice target is not a public object".into(),
                                    ));
                                };
                                let live = state.objects.try_get(object).ok_or_else(|| {
                                    RlContractError("Hunter sacrifice recipient departed".into())
                                })?;
                                if live.zone != Zone::Battlefield {
                                    return Err(RlContractError(
                                        "Hunter sacrifice recipient is not public".into(),
                                    ));
                                }
                                crate::sower_restriction_capture_v1::masked_ref(object, live)
                            })
                            .collect::<Result<Vec<_>, _>>()?;
                        menu = Some(MenuV45 {
                            occurrence,
                            chooser: (*player).into(),
                            source: row.source.clone(),
                            ordered_legal,
                            may_finish: selected.len() >= usize::from(*min_targets),
                        });
                    }
                }
            }
        }
        occurrences.push(row);
    }
    let all = custody::captures(state);
    let mut positions = [0usize; 2];
    let mut trigger_order = Vec::new();
    for pending in &state.engine.pending_triggers {
        let group_position = positions[pending.controller.index()];
        positions[pending.controller.index()] += 1;
        if let Some((capture, _)) = custody::binding(&pending.effect) {
            let occurrence = all
                .iter()
                .position(|original| *original == capture)
                .ok_or_else(|| RlContractError("Hunter ordering occurrence absent".into()))?;
            trigger_order.push(TriggerOrderV45 {
                controller: pending.controller.into(),
                group_position,
                occurrence,
            });
        }
    }
    Ok(FactsV45 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        meanings,
        occurrences,
        menu,
        trigger_order,
    })
}
pub fn validate(state: &GameState, facts: &FactsV45) -> Result<(), RlContractError> {
    if capture(state)? != *facts {
        return Err(RlContractError(
            "Hunter V45 full public facts changed".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
