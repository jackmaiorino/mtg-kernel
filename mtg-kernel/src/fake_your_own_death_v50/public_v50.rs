//! Public family facts reconstructed from private authenticated custody.
//! Replay worlds, execution contexts and history bindings never cross here.
use super::spell_custody::{self, journal, JournalV50, RemovalV50};
use crate::effect::{EffectObjectBinding, EffectOp};
use crate::event::CommittedEvent;
use crate::ids::PlayerId;
use crate::rl::{CardStableRefV1, PlayerSeatV1, RlContractError};
use crate::rules_meaning_fake_your_own_death_v50 as meaning;
use crate::state::{AbilitySourceContractV4, GameState, StackItem, StackTargetContractV4, Zone};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/fake-your-own-death/public/v50";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpellPhaseV50 {
    Published,
    Resolved,
    Fizzled,
    Countered,
    Departed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeathPhaseV50 {
    Captured,
    Held,
    Published,
    ResolvingEntry,
    Resolved,
    Countered,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncarnationV50 {
    pub object: u32,
    pub generation: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpellV50 {
    pub occurrence: usize,
    pub source: CardStableRefV1,
    pub original_target: CardStableRefV1,
    pub copied: bool,
    pub copy_occurrence: Option<usize>,
    pub phase: SpellPhaseV50,
    pub grant: Option<usize>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnfinalizedCopyV50 {
    pub occurrence: usize,
    pub source: CardStableRefV1,
    pub original_target: CardStableRefV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrantV50 {
    pub occurrence: usize,
    pub spell: usize,
    pub original_recipient: CardStableRefV1,
    pub power_modifier_retained: bool,
    pub death_ability_active: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeathV50 {
    pub occurrence: usize,
    pub grant: usize,
    pub original_creature: CardStableRefV1,
    pub graveyard_incarnation: IncarnationV50,
    pub controller_at_death: PlayerSeatV1,
    pub was_token_at_death: bool,
    pub phase: DeathPhaseV50,
    pub returned_incarnation: Option<IncarnationV50>,
    pub returned_controller: Option<PlayerSeatV1>,
    pub attached_host: Option<IncarnationV50>,
    pub treasure_controller: Option<PlayerSeatV1>,
    pub treasures: Vec<TokenV50>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenV50 {
    pub object: u32,
    pub card_db_id: u16,
    pub controller: PlayerSeatV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerOrderV50 {
    pub controller: PlayerSeatV1,
    pub group_position: usize,
    pub death: usize,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuraMenuV50 {
    pub death: usize,
    pub source: CardStableRefV1,
    pub owner: PlayerSeatV1,
    pub legal_hosts: Vec<CardStableRefV1>,
    pub mandatory: bool,
    pub targeted: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV50 {
    pub schema: String,
    pub producer_sha256: String,
    pub card_db_hash: u64,
    pub meanings: Vec<meaning::MeaningV50>,
    pub spells: Vec<SpellV50>,
    pub unfinalized_copies: Vec<UnfinalizedCopyV50>,
    pub grants: Vec<GrantV50>,
    pub deaths: Vec<DeathV50>,
    pub trigger_order: Vec<TriggerOrderV50>,
    pub aura_menu: Option<AuraMenuV50>,
}

pub fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [
            SCHEMA,
            include_str!("public_v50.rs"),
            include_str!("../rl.rs"),
            include_str!("../sower_restriction_capture_v1.rs"),
            include_str!("../linked_aura_operation_v1.rs"),
            include_str!("../effect/thousand_year_storm_v1.rs"),
            include_str!("../effect/standard_copy.rs"),
            include_str!("../standard_spell_copy.rs"),
        ] {
            hash.update((source.len() as u64).to_le_bytes());
            hash.update(source.as_bytes());
        }
        hash.update(meaning::producer_sha256().as_bytes());
        hash.finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    })
    .clone()
}
pub fn meanings() -> Result<Vec<meaning::MeaningV50>, RlContractError> {
    let id = crate::card_def::card_id_by_name("Fake Your Own Death").ok_or_else(|| {
        RlContractError("Fake V50 requires the actual registered printing".into())
    })?;
    [
        meaning::RootV50::InstantSpell,
        meaning::RootV50::GainedDiesAbility,
    ]
    .into_iter()
    .map(|root| meaning::printed_meaning(id, root).map_err(RlContractError))
    .collect()
}
fn at(state: &GameState, index: u64) -> Result<&JournalV50, RlContractError> {
    usize::try_from(index)
        .ok()
        .and_then(|index| state.engine.event_history.get(index))
        .and_then(journal)
        .ok_or_else(|| RlContractError("Fake public occurrence lost its private input".into()))
}
fn ordinal(indices: &[u64], index: u64) -> Result<usize, RlContractError> {
    indices
        .iter()
        .position(|original| *original == index)
        .ok_or_else(|| RlContractError("Fake public ordinal has no original occurrence".into()))
}
fn source(state: &GameState, source: AbilitySourceContractV4) -> CardStableRefV1 {
    CardStableRefV1 {
        arena_id: source.source.0,
        card_db_id: crate::rl::projected_captured_card_def(
            state,
            source.source,
            source.zone_change_count,
            source.zone,
            source.card_def,
        ),
        owner: source.owner.into(),
        controller: source.controller.into(),
        zone: source.zone,
        zone_change_count: source.zone_change_count,
    }
}
fn spell_refs(
    state: &GameState,
    item: &StackItem,
) -> Result<(CardStableRefV1, CardStableRefV1), RlContractError> {
    spell_custody::validate_spell(item).map_err(RlContractError)?;
    let original = item
        .v4
        .source_contract
        .ok_or_else(|| RlContractError("Fake original source absent".into()))?;
    let [StackTargetContractV4::Object {
        object,
        card_def,
        owner,
        controller,
        zone: Zone::Battlefield,
        zone_change_count,
        ..
    }] = item.v4.target_contracts.as_slice()
    else {
        return Err(RlContractError("Fake original public target absent".into()));
    };
    let target = CardStableRefV1 {
        arena_id: object.0,
        card_db_id: crate::rl::projected_captured_card_def(
            state,
            *object,
            *zone_change_count,
            Zone::Battlefield,
            *card_def,
        ),
        owner: (*owner).into(),
        controller: (*controller).into(),
        zone: Zone::Battlefield,
        zone_change_count: *zone_change_count,
    };
    Ok((
        CardStableRefV1 {
            arena_id: original.source.0,
            card_db_id: original.card_def,
            owner: original.owner.into(),
            controller: original.controller.into(),
            zone: Zone::Stack,
            zone_change_count: original.zone_change_count,
        },
        target,
    ))
}
fn incarnation(binding: EffectObjectBinding) -> IncarnationV50 {
    IncarnationV50 {
        object: binding.object.0,
        generation: binding.expected_zone_change_count,
    }
}
fn resolution_death(
    state: &GameState,
    resolution: u64,
    captures: &[u64],
) -> Result<usize, RlContractError> {
    let JournalV50::DeathResolutionInput { publication, .. } = at(state, resolution)? else {
        return Err(RlContractError(
            "Fake public return lost death resolution".into(),
        ));
    };
    let JournalV50::DeathPublished { capture, .. } = at(state, *publication)? else {
        return Err(RlContractError(
            "Fake public return lost death publication".into(),
        ));
    };
    ordinal(captures, *capture)
}

fn death_binding(effect: &EffectOp) -> Option<u64> {
    match effect {
        EffectOp::FakeDeathFromCaptureV50 { capture } => Some(*capture),
        _ => None,
    }
}

/// Shape classification only. References require the real validated owner.
pub fn selected(item: &StackItem) -> bool {
    item.inline_effect
        .as_ref()
        .and_then(death_binding)
        .is_some()
}

fn captured_death_source(
    state: &GameState,
    capture: u64,
) -> Result<CardStableRefV1, RlContractError> {
    let JournalV50::DeathCaptured { lookback, .. } = at(state, capture)? else {
        return Err(RlContractError(
            "Fake source reference lost original death capture".into(),
        ));
    };
    Ok(source(state, lookback.battlefield_source))
}

pub fn stack_source_reference(
    state: &GameState,
    item: &StackItem,
) -> Result<Option<CardStableRefV1>, RlContractError> {
    let Some(capture) = item.inline_effect.as_ref().and_then(death_binding) else {
        return Ok(None);
    };
    spell_custody::validate_retained_grant_history(state).map_err(RlContractError)?;
    if state
        .stack
        .iter()
        .filter(|row| row.v4.stack_item_id == item.v4.stack_item_id)
        .collect::<Vec<_>>()
        .as_slice()
        != [item]
    {
        return Err(RlContractError(
            "Fake source reference requires its complete original live row".into(),
        ));
    }
    Ok(Some(captured_death_source(state, capture)?))
}

pub fn pending_source_reference(
    state: &GameState,
    pending: &crate::trigger::PendingTrigger,
) -> Result<Option<CardStableRefV1>, RlContractError> {
    let Some(capture) = death_binding(&pending.effect) else {
        return Ok(None);
    };
    spell_custody::validate_retained_grant_history(state).map_err(RlContractError)?;
    if !super::death_custody::held_triggers(state).any(|original| original == pending) {
        return Err(RlContractError(
            "Fake source reference requires its complete original held trigger".into(),
        ));
    }
    Ok(Some(captured_death_source(state, capture)?))
}

/// Predecessors cannot erase known printing or gained-ability semantics.
/// Unknown opponent hand and face-down identities do not select this refusal.
pub(crate) fn require_legacy_visible(
    state: &GameState,
    actor: Option<PlayerId>,
) -> Result<(), RlContractError> {
    if super::death_custody::has_unowned_roots(state)
        || state
            .engine
            .event_history
            .iter()
            .any(|event| journal(event).is_some())
    {
        return Err(RlContractError(
            "Fake Your Own Death history requires V50 family facts".into(),
        ));
    }
    let known = state.objects.iter().any(|(id, object)| {
        let hand_known = object.zone == Zone::Hand
            && actor.is_some_and(|actor| {
                actor == object.owner
                    || state
                        .known_hand_cards(actor, object.owner)
                        .iter()
                        .any(|entry| {
                            entry.object == id
                                && entry.zone_change_count == object.zone_change_count
                        })
            });
        let visible = hand_known
            || matches!(
                object.zone,
                Zone::Battlefield | Zone::Graveyard | Zone::Stack | Zone::Exile | Zone::Command
            );
        let definition_known = object.v4.face_down_v1.is_none()
            || actor.is_some_and(|actor| {
                object
                    .v4
                    .face_down_v1
                    .is_some_and(|face| face.lookers & (1 << actor.index()) != 0)
            });
        visible && definition_known && spell_custody::selected_definition(object.card_def)
    });
    if known {
        Err(RlContractError(
            "Fake Your Own Death printing requires V50 family facts".into(),
        ))
    } else {
        Ok(())
    }
}

/// Family projection only. The future mixed consumer must also validate all
/// other supported families and bind these exact facts to its current action.
pub fn capture(state: &GameState) -> Result<FactsV50, RlContractError> {
    let meanings = meanings()?;
    spell_custody::validate_retained_grant_history(state).map_err(RlContractError)?;
    crate::linked_aura_operation_v1::validate(state).map_err(RlContractError)?;
    let indices = |select: fn(&JournalV50) -> bool| {
        state
            .engine
            .event_history
            .iter()
            .enumerate()
            .filter_map(|(index, event)| {
                journal(event)
                    .filter(|entry| select(entry))
                    .map(|_| index as u64)
            })
            .collect::<Vec<_>>()
    };
    let publications = indices(|entry| matches!(entry, JournalV50::Published { .. }));
    let copy_creations = indices(|entry| matches!(entry, JournalV50::CopyCreated { .. }));
    let installations = indices(|entry| matches!(entry, JournalV50::Installed { .. }));
    let captures = indices(|entry| matches!(entry, JournalV50::DeathCaptured { .. }));
    let mut spells = Vec::new();
    for (occurrence, publication) in publications.iter().copied().enumerate() {
        let JournalV50::Published { item } = at(state, publication)? else {
            unreachable!()
        };
        if spell_custody::publication_for(state, item).map_err(RlContractError)?
            != Some(publication)
        {
            return Err(RlContractError(
                "Fake public spell lost unique publication".into(),
            ));
        }
        let (source, original_target) = spell_refs(state, item)?;
        spell_custody::validate_copy_publication(state, item).map_err(RlContractError)?;
        let copy_occurrence = item
            .is_copy
            .then(|| -> Result<usize, RlContractError> {
                let (creation, _) = spell_custody::copy_creation(state, item.v4.stack_item_id)
                    .map_err(RlContractError)?;
                ordinal(&copy_creations, creation)
            })
            .transpose()?;
        let removals = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .filter_map(|(index, event)| match journal(event) {
                Some(JournalV50::RemovalInput {
                    publication: original,
                    removal,
                    ..
                }) if *original == publication => Some((index as u64, *removal)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let mut row = SpellV50 {
            occurrence,
            source,
            original_target,
            copied: item.is_copy,
            copy_occurrence,
            phase: SpellPhaseV50::Published,
            grant: None,
        };
        if let [(input, removal)] = removals.as_slice() {
            row.phase = match removal {
                RemovalV50::Resolving { target_legal: true } => SpellPhaseV50::Resolved,
                RemovalV50::Resolving {
                    target_legal: false,
                } => SpellPhaseV50::Fizzled,
                RemovalV50::Countered => SpellPhaseV50::Countered,
                RemovalV50::OtherDeparture { .. } => SpellPhaseV50::Departed,
            };
            row.grant = installations.iter().position(|index| matches!(at(state, *index), Ok(JournalV50::Installed { input: original, .. }) if original == input));
        } else if !removals.is_empty()
            || state
                .stack
                .iter()
                .filter(|row| row.v4.stack_item_id == item.v4.stack_item_id)
                .collect::<Vec<_>>()
                .as_slice()
                != [item.as_ref()]
        {
            return Err(RlContractError(
                "Fake public unresolved spell lost its live original row".into(),
            ));
        }
        spells.push(row);
    }
    let mut unfinalized_copies = Vec::new();
    for (occurrence, creation) in copy_creations.iter().copied().enumerate() {
        let JournalV50::CopyCreated { item } = at(state, creation)? else {
            unreachable!()
        };
        let actual =
            spell_custody::copy_creation(state, item.v4.stack_item_id).map_err(RlContractError)?;
        if actual != (creation, item.as_ref()) {
            return Err(RlContractError(
                "Fake public copy lost its unique creation".into(),
            ));
        }
        // A retargeted finalized copy has a different full row. Find its actual
        // publication by stack identity, without displaying either private id.
        if publications.iter().any(|index| matches!(at(state, *index), Ok(JournalV50::Published { item: row }) if row.v4.stack_item_id == item.v4.stack_item_id)) { continue; }
        if state
            .stack
            .iter()
            .filter(|row| row.v4.stack_item_id == item.v4.stack_item_id)
            .collect::<Vec<_>>()
            .as_slice()
            != [item.as_ref()]
        {
            return Err(RlContractError(
                "Fake unfinalized copy lost its original live row".into(),
            ));
        }
        let (source, original_target) = spell_refs(state, item)?;
        unfinalized_copies.push(UnfinalizedCopyV50 {
            occurrence,
            source,
            original_target,
        });
    }
    let mut grants = Vec::new();
    for (occurrence, installation) in installations.iter().copied().enumerate() {
        let JournalV50::Installed { input, grant, .. } = at(state, installation)? else {
            unreachable!()
        };
        let JournalV50::RemovalInput {
            publication, item, ..
        } = at(state, *input)?
        else {
            return Err(RlContractError(
                "Fake public grant lost original spell input".into(),
            ));
        };
        grants.push(GrantV50 {
            occurrence,
            spell: ordinal(&publications, *publication)?,
            original_recipient: spell_refs(state, item)?.1,
            power_modifier_retained: grant
                .temporary_modifier_retained(state)
                .map_err(RlContractError)?,
            death_ability_active: grant.active(state).map_err(RlContractError)?,
        });
    }
    let mut deaths = Vec::new();
    for (occurrence, capture) in captures.iter().copied().enumerate() {
        let JournalV50::DeathCaptured {
            installation,
            lookback,
            ..
        } = at(state, capture)?
        else {
            unreachable!()
        };
        let mut row = DeathV50 {
            occurrence,
            grant: ordinal(&installations, *installation)?,
            original_creature: source(state, lookback.battlefield_source),
            graveyard_incarnation: IncarnationV50 {
                object: lookback.battlefield_source.source.0,
                generation: lookback.graveyard_zone_change_count,
            },
            controller_at_death: lookback.battlefield_source.controller.into(),
            was_token_at_death: lookback.was_token_at_death,
            phase: DeathPhaseV50::Captured,
            returned_incarnation: None,
            returned_controller: None,
            attached_host: None,
            treasure_controller: None,
            treasures: Vec::new(),
        };
        for event in &state.engine.event_history {
            match journal(event) {
                Some(JournalV50::DeathCollected {
                    capture: original, ..
                }) if *original == capture => row.phase = DeathPhaseV50::Held,
                Some(JournalV50::DeathPublished {
                    capture: original, ..
                }) if *original == capture => row.phase = DeathPhaseV50::Published,
                _ => {}
            }
        }
        for (input_index, event) in state.engine.event_history.iter().enumerate() {
            match journal(event) {
                Some(JournalV50::DeathCounterInput { publication, .. }) => {
                    if matches!(at(state, *publication)?, JournalV50::DeathPublished { capture: original, .. } if *original == capture)
                    {
                        row.phase = DeathPhaseV50::Countered;
                    }
                }
                Some(JournalV50::DeathResolutionInput { publication, .. }) => {
                    if !matches!(at(state, *publication)?, JournalV50::DeathPublished { capture: original, .. } if *original == capture)
                    {
                        continue;
                    }
                    row.phase = DeathPhaseV50::ResolvingEntry;
                    for event in &state.engine.event_history {
                        if let Some(JournalV50::DeathResolutionClosed {
                            input,
                            returned,
                            created,
                            ..
                        }) = journal(event)
                        {
                            if *input != input_index as u64 {
                                continue;
                            }
                            row.phase = DeathPhaseV50::Resolved;
                            if returned.is_some() {
                                row.returned_incarnation = Some(IncarnationV50 {
                                    object: lookback.battlefield_source.source.0,
                                    generation: lookback
                                        .graveyard_zone_change_count
                                        .checked_add(1)
                                        .ok_or_else(|| {
                                            RlContractError(
                                                "Fake return generation overflow".into(),
                                            )
                                        })?,
                                });
                                row.returned_controller =
                                    Some(lookback.battlefield_source.owner.into());
                            }
                            row.treasure_controller =
                                Some(lookback.battlefield_source.controller.into());
                            for (index, object) in created {
                                let Some(CommittedEvent::CreateToken {
                                    object: actual,
                                    token_def,
                                    controller,
                                    ..
                                }) = usize::try_from(*index)
                                    .ok()
                                    .and_then(|at| state.engine.event_history.get(at))
                                else {
                                    return Err(RlContractError(
                                        "Fake public Treasure lost its actual creation".into(),
                                    ));
                                };
                                if actual != object {
                                    return Err(RlContractError(
                                        "Fake public Treasure changed its actual object".into(),
                                    ));
                                }
                                row.treasures.push(TokenV50 {
                                    object: object.0,
                                    card_db_id: *token_def,
                                    controller: (*controller).into(),
                                });
                            }
                        }
                        if let Some(JournalV50::AuraReturnCompleted {
                            input, selected, ..
                        }) = journal(event)
                        {
                            if matches!(at(state, *input)?, JournalV50::AuraReturnInput { input: original } if original.choice.resolution == input_index as u64)
                            {
                                row.attached_host = selected.map(incarnation);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        deaths.push(row);
    }
    let mut positions = [0usize; 2];
    let mut trigger_order = Vec::new();
    for pending in &state.engine.pending_triggers {
        let group_position = positions[pending.controller.index()];
        positions[pending.controller.index()] += 1;
        if let EffectOp::FakeDeathFromCaptureV50 { capture } = &pending.effect {
            trigger_order.push(TriggerOrderV50 {
                controller: pending.controller.into(),
                group_position,
                death: ordinal(&captures, *capture)?,
            });
        }
    }
    let aura_menu = super::return_aura::pending(state)
        .map(|pending| -> Result<AuraMenuV50, RlContractError> {
            let crate::linked_aura_operation_v1::GeneralChoiceV36::FakeAuraV50 { choice, .. } =
                &pending.choice
            else {
                return Err(RlContractError(
                    "Fake Aura checkpoint has a foreign menu".into(),
                ));
            };
            let mut legal_hosts = Vec::with_capacity(choice.candidates.len());
            for binding in &choice.candidates {
                let object = state
                    .objects
                    .try_get(binding.object)
                    .filter(|object| {
                        object.zone == Zone::Battlefield
                            && object.zone_change_count == binding.expected_zone_change_count
                    })
                    .ok_or_else(|| {
                        RlContractError("Fake public Aura host lost current incarnation".into())
                    })?;
                legal_hosts.push(crate::sower_restriction_capture_v1::masked_ref(
                    binding.object,
                    object,
                )?);
            }
            Ok(AuraMenuV50 {
                death: resolution_death(state, choice.resolution, &captures)?,
                source: source(state, choice.source),
                owner: choice.owner.into(),
                legal_hosts,
                mandatory: true,
                targeted: false,
            })
        })
        .transpose()?;
    Ok(FactsV50 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        meanings,
        spells,
        unfinalized_copies,
        grants,
        deaths,
        trigger_order,
        aura_menu,
    })
}
pub fn validate(state: &GameState, facts: &FactsV50) -> Result<(), RlContractError> {
    if capture(state)? == *facts {
        Ok(())
    } else {
        Err(RlContractError(
            "Fake V50 full public facts differ from current owned state".into(),
        ))
    }
}
