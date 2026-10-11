//! Explicit typed companion over a genuine authenticated Raid V9 observation.
use crate::policy_observation_v9::ObservationV9;
use crate::rl::{CardStableRefV1, PlayerSeatV1, RlContractError};
use crate::rules_meaning_soul_shackled_zombie_v1::{self as meaning, ZombieRootMeaningV1};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "mtg-kernel/public-soul-shackled-zombie/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum TriggerStageV1 {
    Pending { ordinal: u64 },
    Stack { ordinal: u64 },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SingleGraveyardContextV1 {
    pub stage: TriggerStageV1,
    pub source: CardStableRefV1,
    pub original_controller: PlayerSeatV1,
    pub captured_definition_colors: Vec<crate::mana::ManaColor>,
    pub selected: Vec<CardStableRefV1>,
    pub selected_graveyard_owner: Option<PlayerSeatV1>,
    pub selected_live_legal: Vec<bool>,
    pub minimum: u8,
    pub maximum: u8,
    pub legal_choices: Vec<CardStableRefV1>,
    pub can_finish: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ZombiePublicObservationV1 {
    pub schema: String,
    pub source_sha256: String,
    pub predecessor: ObservationV9,
    pub etb_roots: Vec<ZombieRootMeaningV1>,
    pub contexts: Vec<SingleGraveyardContextV1>,
    pub projection_sha256: String,
}
pub fn source_sha256() -> String {
    let mut h = Sha256::new();
    h.update(meaning::source_sha256().as_bytes());
    for s in [
        include_str!("soul_shackled_zombie_public_v1.rs"),
        include_str!("rl_session/soul_shackled_zombie_v13.rs"),
        include_str!("human_bo3_v1/soul_shackled_zombie_v13.rs"),
        include_str!("policy_observation_v9.rs"),
        include_str!("rl_session/human_v8.rs"),
    ] {
        h.update([0]);
        h.update(s.as_bytes());
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}
fn digest(r: &ZombiePublicObservationV1) -> Result<String, RlContractError> {
    Ok(Sha256::digest(serde_json::to_vec(&(
        &r.schema,
        &r.source_sha256,
        &r.predecessor,
        &r.etb_roots,
        &r.contexts,
    ))?)
    .iter()
    .map(|b| format!("{b:02x}"))
    .collect())
}
pub(crate) fn from_authenticated_v9(
    base: ObservationV9,
    contexts: Vec<SingleGraveyardContextV1>,
) -> Result<ZombiePublicObservationV1, RlContractError> {
    let mut r = ZombiePublicObservationV1 {
        schema: SCHEMA.into(),
        source_sha256: source_sha256(),
        predecessor: base,
        etb_roots: meaning::catalog_roots(),
        contexts,
        projection_sha256: String::new(),
    };
    r.projection_sha256 = digest(&r)?;
    validate(&r)?;
    Ok(r)
}
/// Artifact consistency only. Submission rederives the owning private state and menu.
pub fn validate(r: &ZombiePublicObservationV1) -> Result<(), RlContractError> {
    if r.schema != SCHEMA
        || r.source_sha256 != source_sha256()
        || r.predecessor.schema_version != 9
        || r.predecessor.card_db_hash != crate::card_def::KERNEL_CARDDB_HASH
        || r.etb_roots != meaning::catalog_roots()
        || r.projection_sha256 != digest(r)?
    {
        return Err(RlContractError("Zombie public identity mismatch".into()));
    }
    for c in &r.contexts {
        if meaning::root_meaning(c.source.card_db_id, 0).is_err()
            || c.source.zone != crate::state::Zone::Battlefield
            || c.captured_definition_colors
                != crate::card_def::CARD_DEFS[c.source.card_db_id as usize].colors
            || r.contexts
                .iter()
                .filter(|other| other.stage == c.stage)
                .count()
                != 1
            || c.legal_choices.iter().enumerate().any(|(i, choice)| {
                c.legal_choices[..i]
                    .iter()
                    .any(|other| other.arena_id == choice.arena_id)
            })
            || c.minimum != 0
            || c.maximum != 2
            || c.selected.len() > 2
            || c.selected.len() != c.selected_live_legal.len()
            || c.source.controller != c.original_controller
            || c.selected_graveyard_owner != c.selected.first().map(|r| r.owner)
            || c.selected.iter().any(|r| {
                r.zone != crate::state::Zone::Graveyard
                    || Some(r.owner) != c.selected_graveyard_owner
            })
            || c.selected
                .iter()
                .enumerate()
                .any(|(i, r)| c.selected[..i].iter().any(|p| p.arena_id == r.arena_id))
            || c.legal_choices.iter().any(|r| {
                r.zone != crate::state::Zone::Graveyard
                    || c.selected_graveyard_owner.is_some_and(|o| o != r.owner)
                    || c.selected.iter().any(|p| p.arena_id == r.arena_id)
            })
            || (matches!(c.stage, TriggerStageV1::Stack { .. })
                && (!c.legal_choices.is_empty() || c.can_finish))
        {
            return Err(RlContractError("Zombie public relation mismatch".into()));
        }
    }
    Ok(())
}
