//! Public saved-power companion. The selected V9 transport keeps its identity.
//! New historical facts refer only to its public stack ordinal, never raw IDs.
use crate::policy_observation_v9::ObservationV9;
use crate::rl::{PlayerSeatV1, RlContractError, StackItemKindV2};
use crate::rules_meaning_heartfire_v1::{self, HeartfireRootMeaningV1};
use crate::state::GameState;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/public-heartfire/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeartfireSavedDamageV1 {
    pub stack_index: u32,
    pub root: HeartfireRootMeaningV1,
    pub signed_departing_power: i32,
    pub damage_if_target_legal_before_prevention: i32,
    pub captured_controller: PlayerSeatV1,
    pub captured_color_mask: u8,
    pub captured_lifelink: bool,
    pub captured_deathtouch: bool,
    pub captured_werewolf: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeartfirePublicObservationV1 {
    pub schema: String,
    pub source_sha256: String,
    pub predecessor: ObservationV9,
    pub activation_roots: Vec<HeartfireRootMeaningV1>,
    pub saved_damage: Vec<HeartfireSavedDamageV1>,
    pub projection_sha256: String,
}
pub fn source_sha256() -> String {
    let mut hash = Sha256::new();
    hash.update(rules_meaning_heartfire_v1::source_sha256().as_bytes());
    for source in [
        include_str!("heartfire_public_v1.rs"),
        include_str!("human_bo3_v1/heartfire_v12.rs"),
        include_str!("human_bo3_v1/labels.rs"),
        include_str!("human_bo3_v1/life_ward_v8.rs"),
        include_str!("policy_observation_v9.rs"),
        include_str!("rl_session/human_v8.rs"),
        include_str!("rl_session/raid_v9.rs"),
        include_str!("rl_session/heartfire_v12.rs"),
    ] {
        hash.update([0]);
        hash.update(source.as_bytes());
    }
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn digest(record: &HeartfirePublicObservationV1) -> Result<String, RlContractError> {
    let bytes = serde_json::to_vec(&(
        &record.schema,
        &record.source_sha256,
        &record.predecessor,
        &record.activation_roots,
        &record.saved_damage,
    ))?;
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}
pub(crate) fn from_authenticated_v9(
    state: &GameState,
    base: ObservationV9,
) -> Result<HeartfirePublicObservationV1, RlContractError> {
    if base.schema_version != 9 || base.card_db_hash != crate::card_def::KERNEL_CARDDB_HASH {
        return Err(RlContractError(
            "Heartfire predecessor identity mismatch".into(),
        ));
    }
    crate::source_power_activation_v1::validate_state(state).map_err(RlContractError)?;
    let mut saved_damage = Vec::new();
    for (index, item) in state.stack.iter().enumerate() {
        if !matches!(
            item.inline_effect.as_ref(),
            Some(crate::effect::EffectOp::StandardV1(
                crate::standard_cards_v1::StandardOpV1::SourceLastPowerDamageV1
            ))
        ) {
            continue;
        }
        crate::source_power_activation_v1::validate_item(state, item).map_err(RlContractError)?;
        let source = item
            .v4
            .ability_source_contract
            .ok_or_else(|| RlContractError("Heartfire missing source contract".into()))?;
        let local = item
            .v4
            .activated_ability_index
            .ok_or_else(|| RlContractError("Heartfire missing printed selector".into()))?;
        let root = rules_meaning_heartfire_v1::root_meaning(source.card_def, u32::from(local))
            .map_err(|_| RlContractError("Heartfire unsupported selected root".into()))?;
        let facts = crate::source_power_activation_v1::damage_source(
            state,
            item.source,
            item.v4.stack_item_id,
        )
        .map_err(RlContractError)?;
        let power = state
            .engine
            .event_history
            .iter()
            .find_map(|event| match event {
                crate::event::CommittedEvent::SourcePowerActivationCommittedV1 {
                    item: saved,
                    power,
                    ..
                } if saved.v4.stack_item_id == item.v4.stack_item_id => Some(*power),
                _ => None,
            })
            .ok_or_else(|| RlContractError("Heartfire lost authenticated power receipt".into()))?;
        let ordinal = u32::try_from(index)
            .map_err(|_| RlContractError("Heartfire stack ordinal exceeds u32".into()))?;
        let row = base
            .projection
            .surface
            .stack
            .get(index)
            .ok_or_else(|| RlContractError("Heartfire stack projection mismatch".into()))?;
        if row.stack_index != ordinal
            || row.controller != item.controller.into()
            || row.stack_item_kind != StackItemKindV2::ActivatedAbility
        {
            return Err(RlContractError("Heartfire public producer mismatch".into()));
        }
        saved_damage.push(HeartfireSavedDamageV1 {
            stack_index: ordinal,
            root,
            signed_departing_power: power,
            damage_if_target_legal_before_prevention: power.max(0),
            captured_controller: facts.source.controller.into(),
            captured_color_mask: facts.color_mask,
            captured_lifelink: facts.lifelink,
            captured_deathtouch: facts.deathtouch,
            captured_werewolf: facts.werewolf,
        });
    }
    let mut record = HeartfirePublicObservationV1 {
        schema: SCHEMA.into(),
        source_sha256: source_sha256(),
        predecessor: base,
        activation_roots: rules_meaning_heartfire_v1::catalog_roots(),
        saved_damage,
        projection_sha256: String::new(),
    };
    record.projection_sha256 = digest(&record)?;
    validate(&record)?;
    Ok(record)
}
/// Public artifact shape/identity validation, not proof of committed history.
/// Only the sealed session reconstructs captured facts and execution authority.
pub fn validate(record: &HeartfirePublicObservationV1) -> Result<(), RlContractError> {
    if record.schema != SCHEMA
        || record.source_sha256 != source_sha256()
        || record.predecessor.schema_version != 9
        || record.predecessor.card_db_hash != crate::card_def::KERNEL_CARDDB_HASH
        || record.activation_roots != rules_meaning_heartfire_v1::catalog_roots()
        || record.projection_sha256 != digest(record)?
    {
        return Err(RlContractError("Heartfire public identity mismatch".into()));
    }
    let mut previous = None;
    for saved in &record.saved_damage {
        rules_meaning_heartfire_v1::validate(&saved.root)
            .map_err(|_| RlContractError("Heartfire saved root mismatch".into()))?;
        let row = record
            .predecessor
            .projection
            .surface
            .stack
            .get(saved.stack_index as usize)
            .ok_or_else(|| RlContractError("Heartfire missing public stack ordinal".into()))?;
        if previous.is_some_and(|index| saved.stack_index <= index)
            || row.stack_index != saved.stack_index
            || row.stack_item_kind != StackItemKindV2::ActivatedAbility
            || row.controller != saved.captured_controller
            || saved.damage_if_target_legal_before_prevention != saved.signed_departing_power.max(0)
        {
            return Err(RlContractError(
                "Heartfire saved public relation mismatch".into(),
            ));
        }
        previous = Some(saved.stack_index);
    }
    Ok(())
}
