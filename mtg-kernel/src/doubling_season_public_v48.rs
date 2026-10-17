//! Public replacement relations, without private replay worlds or executable inputs.
use crate::doubling_season_v48 as season;
use crate::event::CommittedEvent;
use crate::event_history_v1::EventKindV1;
use crate::rl::{CardStableRefV1, PlayerSeatV1, RlContractError};
use crate::state::{AbilitySourceContractV4, GameState, Zone};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "mtg-kernel/doubling-season/public/v48";
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncarnationV48 {
    pub object: u32,
    pub generation: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreationV48 {
    pub occurrence: usize,
    pub controller: PlayerSeatV1,
    /// The creation blueprint is public when these face-up tokens enter.
    pub token_definition: u16,
    pub face_index: Option<u8>,
    pub requested: usize,
    pub created: Vec<IncarnationV48>,
    pub replacement_sources: Vec<CardStableRefV1>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CounterKindV48 {
    PlusOnePlusOne,
    Bait,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CounterPlacementV48 {
    pub occurrence: usize,
    pub kind: CounterKindV48,
    /// Historical recipient incarnation only, never its later hidden printing.
    pub recipient: IncarnationV48,
    pub actor: PlayerSeatV1,
    pub recipient_controller: PlayerSeatV1,
    pub original: i32,
    pub placed: i32,
    pub replacement_sources: Vec<CardStableRefV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachmentMenuV48 {
    pub equipment: CardStableRefV1,
    pub chooser: PlayerSeatV1,
    pub ordered_created: Vec<CardStableRefV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachmentV48 {
    pub occurrence: usize,
    pub original_equipment: CardStableRefV1,
    pub created_controller: PlayerSeatV1,
    pub selected: IncarnationV48,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV48 {
    pub schema: String,
    pub producer_sha256: String,
    pub card_db_hash: u64,
    pub meanings: Vec<crate::rules_meaning_doubling_season_v48::MeaningV48>,
    pub active_sources: Vec<CardStableRefV1>,
    pub creations: Vec<CreationV48>,
    pub counter_placements: Vec<CounterPlacementV48>,
    pub attachment_menu: Option<AttachmentMenuV48>,
    pub attachments: Vec<AttachmentV48>,
}
fn original_source(source: AbilitySourceContractV4) -> CardStableRefV1 {
    CardStableRefV1 {
        arena_id: source.source.0,
        card_db_id: source.card_def,
        owner: source.owner.into(),
        controller: source.controller.into(),
        zone: Zone::Battlefield,
        zone_change_count: source.zone_change_count,
    }
}
pub fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut h = Sha256::new();
        for source in [
            SCHEMA,
            include_str!("doubling_season_public_v48.rs"),
            include_str!("doubling_season_v48.rs"),
            include_str!("linked_aura_operation_v1.rs"),
            include_str!("season_token_attachment_v48.rs"),
            include_str!("event.rs"),
            include_str!("state.rs"),
            include_str!("engine.rs"),
            include_str!("continuous_characteristics_v1.rs"),
            include_str!("standard_creatures_v1.rs"),
            include_str!("standard_cards_v1.rs"),
            include_str!("planeswalker_v1.rs"),
            include_str!("rules_meaning_doubling_season_v48.rs"),
            include_str!("card_def.rs"),
            include_str!("bloodthirsty_conqueror_v49/custody.rs"),
            include_str!("counter_target_effects_v1.rs"),
            include_str!("zimone_paradox_sculptor_v1.rs"),
            include_str!("engine/omniscience_cast_v1.rs"),
            include_str!("resolution_cast_v1.rs"),
            include_str!("effect/harvester_v1.rs"),
            include_str!("leyline_axe_v1.rs"),
            include_str!("london_mulligan_v1.rs"),
            include_str!("life_loss_turn_v1.rs"),
            include_str!("soul_shackled_zombie_v1.rs"),
            include_str!("progenitus_v1.rs"),
            include_str!("controlled_creature_grants_v1.rs"),
            include_str!("source_power_activation_v1.rs"),
            include_str!("effect/nonland_copy_v1.rs"),
            include_str!("rl_session/flat_action_v4/search_state/effect_refs.rs"),
            include_str!("effect/inspiration_resolution_v1.rs"),
            include_str!("effect/standard_exile.rs"),
            include_str!("snack_wide_life_v59.rs"),
            include_str!("snack_wide_life_v59/custody.rs"),
            include_str!("bloodthirsty_conqueror_v49.rs"),
            include_str!("fiendish_panda_v1.rs"),
            include_str!("fiendish_panda_v1/custody.rs"),
            include_str!("life_gain_turn_v1.rs"),
            include_str!("midnight_snack_v1.rs"),
            include_str!("mana.rs"),
            include_str!("legend_rule_v1.rs"),
            include_str!("rl.rs"),
            include_str!("standard_cast_convoke.rs"),
            include_str!("engine/mandatory_cast_cost_v1.rs"),
            include_str!("rules_meaning_next_five_v59.rs"),
            include_str!("effect/thousand_year_storm_v1.rs"),
            include_str!("effect/standard_copy.rs"),
            include_str!("standard_spell_copy.rs"),
            include_str!("event_history_v1.rs"),
            crate::engine::time_stop_v1::source_sha256_v1(),
            crate::effect::genesis_wave_v1::source_sha256_v1(),
            include_str!("engine/tinybones_v72.rs"),
        ] {
            h.update((source.len() as u64).to_le_bytes());
            h.update(source.as_bytes());
        }
        h.update(crate::rules_meaning_doubling_season_v48::producer_sha256().as_bytes());
        h.finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    })
    .clone()
}
pub fn meanings(
) -> Result<Vec<crate::rules_meaning_doubling_season_v48::MeaningV48>, RlContractError> {
    use crate::rules_meaning_doubling_season_v48::{self as meaning, RootV48};
    let Some(id) = crate::card_def::card_id_by_name("Doubling Season") else {
        return Ok(Vec::new());
    };
    [
        RootV48::EnchantmentSpell,
        RootV48::EffectTokenCreation,
        RootV48::EffectCounterPlacement,
    ]
    .into_iter()
    .map(|root| meaning::printed_meaning(id, root).map_err(RlContractError))
    .collect()
}
pub fn capture(state: &GameState) -> Result<FactsV48, RlContractError> {
    season::validate_token_journals(state).map_err(RlContractError)?;
    let active_sources = [crate::ids::PlayerId::P0, crate::ids::PlayerId::P1]
        .into_iter()
        .flat_map(|player| season::sources(state, player))
        .map(original_source)
        .collect();
    let mut creations = Vec::new();
    let mut counter_placements = Vec::new();
    let mut attachments = Vec::new();
    for event in state.engine.event_history.scan_for_v1(&[
        EventKindV1::BaitTransferAppliedV48,
        EventKindV1::PlusCounterReplacementV48,
        EventKindV1::TokenAttachmentJournalV48,
        EventKindV1::TokenCreationReplacementV48,
    ]) {
        match event {
            CommittedEvent::TokenCreationReplacementV48 { creation } => {
                let mut cursor = 0;
                for request in &creation.original {
                    let count = request
                        .replacement
                        .replaced_token_count(1)
                        .map_err(RlContractError)?;
                    let created = creation.created[cursor..cursor + count]
                        .iter()
                        .map(|at| match &state.engine.event_history[*at as usize] {
                            CommittedEvent::CreateToken { object, .. } => IncarnationV48 {
                                object: object.0,
                                generation: 0,
                            },
                            _ => unreachable!("validated actual token creation"),
                        })
                        .collect();
                    creations.push(CreationV48 {
                        occurrence: creations.len(),
                        controller: request.template.controller.into(),
                        token_definition: request.template.definition,
                        face_index: request.template.face,
                        requested: 1,
                        created,
                        replacement_sources: request
                            .replacement
                            .seasons
                            .iter()
                            .copied()
                            .map(original_source)
                            .collect(),
                    });
                    cursor += count;
                }
            }
            CommittedEvent::PlusCounterReplacementV48 { placement } => {
                counter_placements.push(CounterPlacementV48 {
                    occurrence: counter_placements.len(),
                    kind: CounterKindV48::PlusOnePlusOne,
                    recipient: IncarnationV48 {
                        object: placement.recipient.object.0,
                        generation: placement.recipient.zone_change_count,
                    },
                    actor: placement.actor.into(),
                    recipient_controller: placement.replacement.controller.into(),
                    original: placement.original,
                    placed: placement
                        .after
                        .checked_sub(placement.before)
                        .ok_or_else(|| RlContractError("counter difference overflow".into()))?,
                    replacement_sources: placement
                        .replacement
                        .seasons
                        .iter()
                        .copied()
                        .map(original_source)
                        .collect(),
                })
            }
            CommittedEvent::BaitTransferAppliedV48 { placement } => {
                let Some(CommittedEvent::BaitTransferInputV48 { input }) =
                    state.engine.event_history.get(placement.input as usize)
                else {
                    unreachable!("validated original bait input")
                };
                let original = season::ReplacementInputV48::capture(
                    &input.world,
                    input.world.objects.get(input.recipient.object).controller,
                );
                counter_placements.push(CounterPlacementV48 {
                    occurrence: counter_placements.len(),
                    kind: CounterKindV48::Bait,
                    recipient: IncarnationV48 {
                        object: input.recipient.object.0,
                        generation: input.recipient.zone_change_count,
                    },
                    actor: input.actor.into(),
                    recipient_controller: original.controller.into(),
                    original: input.original,
                    placed: placement
                        .after
                        .checked_sub(placement.before)
                        .ok_or_else(|| RlContractError("bait placed count overflow".into()))?,
                    replacement_sources: original
                        .seasons
                        .into_iter()
                        .map(original_source)
                        .collect(),
                });
            }
            CommittedEvent::TokenAttachmentJournalV48 { journal } => {
                if let crate::season_token_attachment_v48::JournalV48::Completed {
                    input,
                    selected,
                    ..
                } = journal.as_ref()
                {
                    let Some(CommittedEvent::TokenAttachmentJournalV48 { journal }) =
                        state.engine.event_history.get(*input as usize)
                    else {
                        unreachable!("validated original attachment input")
                    };
                    let crate::season_token_attachment_v48::JournalV48::Input { input } =
                        journal.as_ref()
                    else {
                        unreachable!("validated original attachment input kind")
                    };
                    attachments.push(AttachmentV48 {
                        occurrence: attachments.len(),
                        original_equipment: original_source(input.choice.source),
                        created_controller: input.choice.controller.into(),
                        selected: IncarnationV48 {
                            object: selected.object.0,
                            generation: selected.expected_zone_change_count,
                        },
                    });
                }
            }
            _ => {}
        }
    }
    let attachment_menu = if let Some(pending) = crate::season_token_attachment_v48::pending(state)
    {
        crate::linked_aura_operation_v1::validate(state).map_err(RlContractError)?;
        if let crate::linked_aura_operation_v1::GeneralChoiceV36::TokenEquipmentV48 {
            choice, ..
        } = &pending.choice
        {
            crate::season_token_attachment_v48::validate_choice(state, choice)
                .map_err(RlContractError)?;
            let current = |id| {
                let live = state.objects.get(id);
                CardStableRefV1 {
                    arena_id: id.0,
                    card_db_id: live.card_def,
                    owner: live.owner.into(),
                    controller: live.controller.into(),
                    zone: live.zone,
                    zone_change_count: live.zone_change_count,
                }
            };
            Some(AttachmentMenuV48 {
                equipment: current(choice.source.source),
                chooser: choice.controller.into(),
                ordered_created: choice
                    .candidates
                    .iter()
                    .map(|c| current(c.object))
                    .collect(),
            })
        } else {
            None
        }
    } else {
        None
    };
    Ok(FactsV48 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        meanings: meanings()?,
        active_sources,
        creations,
        counter_placements,
        attachment_menu,
        attachments,
    })
}
