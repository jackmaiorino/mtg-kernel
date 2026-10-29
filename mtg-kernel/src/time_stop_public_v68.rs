//! Read-only validated public end-turn outcomes. No private World, interpreter
//! operand, discarded hidden hand, or executable capability is published.
use crate::{
    ids::PlayerId,
    rl::{PlayerSeatV1, StackItemPublicV2},
    state::{GameState, Step},
};
use serde::{Deserialize, Serialize};
pub const SCHEMA: &str = "mtg-kernel/time-stop-public/v68";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisposedV68 {
    /// Public position in the original stack, never an executable index.
    pub original_stack_position: u32,
    #[serde(deserialize_with = "strict_stack")]
    pub original: StackItemPublicV2,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisposalV68 {
    pub occurrence: u32,
    pub ending_spell_position: u32,
    pub original_step: Step,
    pub original_active_player: PlayerSeatV1,
    pub disposed_rows: Vec<DisposedV68>,
}
/// Exact inherited vector roles whose old publication label is historical once disposed.
/// These are public ordinals, never native history or executable indices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum CompatibilityTargetV68 {
    DrakeCombat { occurrence: u32 },
    DrakeActivation { occurrence: u32 },
    RavenousActivation { occurrence: u32 },
    AjaniActivation { position: u32 },
    FishingOperation { position: u32 },
    FaebloomSpell { ordinal: u32 },
    FaebloomReflexive { ordinal: u32 },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TerminalOutcomeV68 {
    Disposed,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompatibilityTerminalV68 {
    pub target: CompatibilityTargetV68,
    pub disposal_occurrence: u32,
    pub original_stack_position: u32,
    pub outcome: TerminalOutcomeV68,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PhaseV68 {
    BeforeCleanup,
    Discard,
    Expiration,
    Priority,
    GameOver,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CleanupV68 {
    pub disposal_occurrence: u32,
    pub turn: u32,
    pub active_player: PlayerSeatV1,
    pub phase: PhaseV68,
    pub discard_count: Option<u32>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV68 {
    pub schema: String,
    pub producer_sha256: String,
    pub viewer: PlayerSeatV1,
    pub meaning: crate::rules_meaning_time_stop_v68::MeaningV68,
    pub visible_printings: Vec<crate::optional_trigger_public_v1::ReferenceV1>,
    pub disposals: Vec<DisposalV68>,
    pub compatibility_terminals: Vec<CompatibilityTerminalV68>,
    #[serde(deserialize_with = "required_optional")]
    pub cleanup: Option<CleanupV68>,
}
pub fn producer_sha256() -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    for source in [
        SCHEMA,
        include_str!("time_stop_public_v68.rs"),
        crate::engine::time_stop_v1::source_sha256_v1(),
    ] {
        h.update((source.len() as u64).to_le_bytes());
        h.update(source.as_bytes());
    }
    format!("{:x}", h.finalize())
}
pub fn capture(state: &GameState, viewer: PlayerId) -> Result<FactsV68, String> {
    crate::engine::time_stop_v1::public_facts_v68(state, viewer)
}
pub(crate) fn capture_genesis_wave_v69(
    state: &GameState,
    viewer: PlayerId,
) -> Result<FactsV68, String> {
    crate::engine::time_stop_v1::public_facts_genesis_wave_v69(state, viewer)
}
pub(crate) fn capture_vivien_v70(state: &GameState, viewer: PlayerId) -> Result<FactsV68, String> {
    crate::engine::time_stop_v1::public_facts_vivien_v70(state, viewer)
}
pub(crate) fn capture_muldrotha_v71(
    state: &GameState,
    viewer: PlayerId,
) -> Result<FactsV68, String> {
    crate::engine::time_stop_v1::public_facts_muldrotha_v71(state, viewer)
}
pub(crate) fn capture_tinybones_v72(
    state: &GameState,
    viewer: PlayerId,
) -> Result<FactsV68, String> {
    crate::engine::time_stop_v1::public_facts_tinybones_v72(state, viewer)
}
/// Closed value and supplied-binding checks only. Native capture establishes
/// inventory completeness; validate_current and sealed owners recapture it.
/// A recomputed public commitment is not native authenticity or completeness.
pub fn validate(value: &FactsV68) -> Result<(), String> {
    if value.schema != SCHEMA || value.producer_sha256 != producer_sha256() {
        return Err("V68 facts identity changed".into());
    }
    crate::rules_meaning_time_stop_v68::validate(&value.meaning)?;
    if value.visible_printings.iter().any(|r| {
        !matches!(
            r.identity,
            crate::optional_trigger_public_v1::IdentityV1::Known { card_id: 486 }
        )
    }) {
        return Err("V68 visible printing is not an authorized Time Stop identity".into());
    }
    for (position, disposal) in value.disposals.iter().enumerate() {
        if usize::try_from(disposal.occurrence).ok() != Some(position)
            || disposal.disposed_rows.is_empty()
            || usize::try_from(disposal.ending_spell_position).ok()
                != disposal.disposed_rows.len().checked_sub(1)
        {
            return Err("V68 disposal lost original complete stack order".into());
        }
        for (position, row) in disposal.disposed_rows.iter().enumerate() {
            if usize::try_from(row.original_stack_position).ok() != Some(position)
                || row.original.stack_index != row.original_stack_position
            {
                return Err("V68 disposed row changed its original public position".into());
            }
        }
        let ending = &disposal.disposed_rows[disposal.ending_spell_position as usize].original;
        if ending.source.card_db_id != 486
            || ending.stack_item_kind != crate::rl::StackItemKindV2::Spell
        {
            return Err("V68 disposal lost its genuine Time Stop spell".into());
        }
    }
    let mut targets = std::collections::BTreeSet::new();
    for terminal in &value.compatibility_terminals {
        if !targets.insert(&terminal.target)
            || value
                .disposals
                .get(terminal.disposal_occurrence as usize)
                .and_then(|d| {
                    d.disposed_rows
                        .get(terminal.original_stack_position as usize)
                })
                .is_none()
        {
            return Err("V68 compatibility terminal changed its exact original public row or duplicated a role".into());
        }
    }
    if let Some(cleanup) = &value.cleanup {
        if usize::try_from(cleanup.disposal_occurrence)
            .ok()
            .is_none_or(|i| i >= value.disposals.len())
            || (cleanup.phase == PhaseV68::Discard) != cleanup.discard_count.is_some()
            || cleanup.discard_count == Some(0)
        {
            return Err("V68 cleanup changed its original disposal or hand obligation".into());
        }
    }
    Ok(())
}

fn required_optional<'de, D, T>(d: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d)
}
fn strict_stack<'de, D>(d: D) -> Result<StackItemPublicV2, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    let value = serde_json::Value::deserialize(d)?;
    let row: StackItemPublicV2 = serde_json::from_value(value.clone()).map_err(D::Error::custom)?;
    if serde_json::to_value(&row).map_err(D::Error::custom)? != value {
        return Err(D::Error::custom(
            "V68 disposed stack row requires the exact closed canonical native public DTO",
        ));
    }
    Ok(row)
}
