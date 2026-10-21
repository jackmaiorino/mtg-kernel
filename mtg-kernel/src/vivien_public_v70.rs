//! Sanitized native loyalty/publication/emblem facts. Recapture proves inventory;
//! supplied-value validation alone never grants completeness or execution.
use crate::{
    ids::PlayerId, optional_trigger_public_v1 as public, rl::PlayerSeatV1, state::GameState,
};
use serde::{Deserialize, Serialize};
pub const SCHEMA: &str = "mtg-kernel/vivien-public/v70";
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StageV70 {
    Published,
    Looking,
    AnsweredLook,
    LookFinished,
    EmblemCreated,
    Disposed,
    NoLongerPublished,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationV70 {
    pub occurrence: u32,
    #[serde(deserialize_with = "strict_root")]
    pub source: public::RootV1,
    pub ability_index: u8,
    pub loyalty_before: u32,
    pub loyalty_after: u32,
    pub original_targets: Vec<TargetV70>,
    pub stage: StageV70,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmblemV70 {
    pub occurrence: u32,
    pub activation: u32,
    pub controller: PlayerSeatV1,
    pub timestamp: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV70 {
    pub schema: String,
    pub producer_sha256: String,
    pub viewer: PlayerSeatV1,
    pub meaning: crate::rules_meaning_vivien_v70::MeaningV70,
    #[serde(deserialize_with = "strict_references")]
    pub visible_printings: Vec<public::ReferenceV1>,
    #[serde(deserialize_with = "strict_current")]
    pub current: Vec<crate::valkyries_call_public_v62::CurrentV62>,
    pub activations: Vec<ActivationV70>,
    pub emblems: Vec<EmblemV70>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetV70 {
    #[serde(deserialize_with = "strict_reference")]
    pub original: public::ReferenceV1,
    #[serde(deserialize_with = "strict_optional_reference")]
    pub current_original: Option<public::ReferenceV1>,
    pub legal_now: bool,
}
/// Per-action references to the full facts, never a repeated private library prefix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChoiceContextV70 {
    pub occurrence: u32,
    pub ability_index: u8,
    pub stage: StageV70,
}
pub fn choice_contexts(v: &FactsV70) -> Vec<ChoiceContextV70> {
    v.activations
        .iter()
        .filter(|a| matches!(a.stage, StageV70::Looking | StageV70::AnsweredLook))
        .map(|a| ChoiceContextV70 {
            occurrence: a.occurrence,
            ability_index: a.ability_index,
            stage: a.stage,
        })
        .collect()
}
pub fn producer_sha256() -> String {
    use sha2::{Digest, Sha256};
    static H: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    H.get_or_init(|| {
        let mut h = Sha256::new();
        for s in [
            SCHEMA,
            include_str!("vivien_public_v70.rs"),
            crate::engine::time_stop_v1::source_sha256_v1(),
        ] {
            h.update((s.len() as u64).to_le_bytes());
            h.update(s.as_bytes());
        }
        format!("{:x}", h.finalize())
    })
    .clone()
}
pub fn capture(s: &GameState, v: PlayerId) -> Result<FactsV70, String> {
    crate::effect::vivien_reid_v1::public_facts_v70(s, v)
}
pub(crate) fn capture_tinybones_compat_v72(s: &GameState, v: PlayerId) -> Result<FactsV70, String> {
    Ok(tinybones_schema(
        crate::effect::vivien_reid_v1::public_facts_tinybones_compat_v72(s, v)?,
    ))
}
pub(crate) fn capture_tinybones_for_consumer_v72(
    s: &GameState,
    v: PlayerId,
    consumer: crate::optional_trigger_public_v1::ConsumerV1,
) -> Result<FactsV70, String> {
    Ok(tinybones_schema(
        crate::effect::vivien_reid_v1::public_facts_tinybones_for_consumer_v72(s, v, consumer)?,
    ))
}
fn tinybones_schema(mut facts: FactsV70) -> FactsV70 {
    facts.schema = format!(
        "{}/compat/vivien_reid_v1",
        crate::tinybones_observation_v72::SCHEMA
    );
    facts
}
pub fn validate(v: &FactsV70) -> Result<(), String> {
    validate_with_schema_v72(v, SCHEMA)
}
pub(crate) fn validate_tinybones_compat_v72(v: &FactsV70) -> Result<(), String> {
    validate_with_schema_v72(
        v,
        &format!(
            "{}/compat/vivien_reid_v1",
            crate::tinybones_observation_v72::SCHEMA
        ),
    )
}
fn validate_with_schema_v72(v: &FactsV70, schema: &str) -> Result<(), String> {
    if v.schema != schema || v.producer_sha256 != producer_sha256() {
        return Err("V70 facts identity changed".into());
    }
    crate::rules_meaning_vivien_v70::validate(&v.meaning)?;
    if v.visible_printings
        .iter()
        .any(|r| !matches!(r.identity, public::IdentityV1::Known { card_id: 488 }))
    {
        return Err("V70 visible printing changed".into());
    }
    let mut current_seen = std::collections::BTreeSet::new();
    for c in &v.current {
        if !matches!(c.card.identity, public::IdentityV1::Known { card_id: 488 })
            || c.card.zone != crate::state::Zone::Battlefield
            || !current_seen.insert((c.card.object, c.card.generation))
            || serde_json::to_value(&c.counters)
                .map_err(|e| e.to_string())?
                .as_object()
                .ok_or("V70 counter record missing")?
                .values()
                .any(|n| n.as_u64().is_none())
        {
            return Err("V70 current Vivien roster/counters changed".into());
        }
    }
    for (i, a) in v.activations.iter().enumerate() {
        let after = match a.ability_index {
            0 => a.loyalty_before.checked_add(1),
            1 => a.loyalty_before.checked_sub(3),
            2 => a.loyalty_before.checked_sub(8),
            _ => None,
        };
        if usize::try_from(a.occurrence).ok() != Some(i)
            || after != Some(a.loyalty_after)
            || a.source.original_source.zone != crate::state::Zone::Battlefield
            || a.source.original_source.controller != a.source.captured_controller
            || !matches!(
                a.source.original_source.identity,
                public::IdentityV1::Known { card_id: 488 }
            )
            || a.original_targets.len() != usize::from(a.ability_index == 1)
            || matches!(
                a.stage,
                StageV70::Looking | StageV70::AnsweredLook | StageV70::LookFinished
            ) && a.ability_index != 0
            || a.stage == StageV70::EmblemCreated && a.ability_index != 2
        {
            return Err("V70 paid activation shape changed".into());
        }
    }
    for a in &v.activations {
        for t in &a.original_targets {
            if t.original.zone != crate::state::Zone::Battlefield
                || t.legal_now && t.current_original.is_none()
                || t.current_original.as_ref().is_some_and(|c| {
                    c.object != t.original.object
                        || c.generation != t.original.generation
                        || c.owner != t.original.owner
                        || c.zone != t.original.zone
                })
            {
                return Err("V70 original/current target relation changed".into());
            }
        }
    }
    let mut stamps = std::collections::BTreeSet::new();
    let mut activations = std::collections::BTreeSet::new();
    for (i, e) in v.emblems.iter().enumerate() {
        let a = v
            .activations
            .get(e.activation as usize)
            .ok_or("V70 emblem activation absent")?;
        if usize::try_from(e.occurrence).ok() != Some(i)
            || !stamps.insert(e.timestamp)
            || !activations.insert(e.activation)
            || a.stage != StageV70::EmblemCreated
            || e.controller != a.source.captured_controller
        {
            return Err("V70 emblem relation changed".into());
        }
    }
    if v.activations
        .iter()
        .any(|a| a.stage == StageV70::EmblemCreated && !activations.contains(&a.occurrence))
    {
        return Err("V70 emblem omitted".into());
    }
    if choice_contexts(v).len() > 1 {
        return Err("V70 multiple resolving looks".into());
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationCostV70 {
    #[serde(deserialize_with = "strict_stable")]
    pub source: crate::rl::CardStableRefV1,
    pub ability_index: u8,
    pub before: u32,
    pub after: u32,
    pub paid_before_publication: bool,
    pub countering_refunds_cost: bool,
}
pub(crate) fn activation_cost(
    v: &FactsV70,
    a: &crate::rl::ActionSemanticV1,
) -> Result<Option<ActivationCostV70>, String> {
    let crate::rl::ActionSemanticV1::ActivateAbility {
        source,
        actor,
        ability_index,
        granted_ability,
    } = a
    else {
        return Ok(None);
    };
    if source.card_db_id != 488 {
        return Ok(None);
    }
    if !granted_ability.is_none()
        || source.zone != crate::state::Zone::Battlefield
        || *actor != source.controller
    {
        return Err("V70 loyalty activation changed source/grant".into());
    }
    let c = v
        .current
        .iter()
        .find(|c| {
            c.card.object == source.arena_id
                && c.card.generation == source.zone_change_count
                && c.card.owner == source.owner
                && c.card.controller == source.controller
        })
        .ok_or("V70 loyalty source missing from public current roster")?;
    if !c.form.printed_abilities_active || c.card.zone != source.zone {
        return Err("V70 loyalty printed ability inactive".into());
    }
    let before = c.counters.loyalty;
    let after = match ability_index {
        0 => before.checked_add(1),
        1 => before.checked_sub(3),
        2 => before.checked_sub(8),
        _ => None,
    }
    .ok_or("V70 loyalty quote exceeds native u32")?;
    Ok(Some(ActivationCostV70 {
        source: source.clone(),
        ability_index: *ability_index,
        before,
        after,
        paid_before_publication: true,
        countering_refunds_cost: false,
    }))
}

fn canonical<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned + serde::Serialize,
{
    use serde::de::Error;
    let value = serde_json::Value::deserialize(d)?;
    let typed: T = serde_json::from_value(value.clone()).map_err(D::Error::custom)?;
    if serde_json::to_value(&typed).map_err(D::Error::custom)? != value {
        return Err(D::Error::custom(
            "V70 noncanonical reference or current fields",
        ));
    }
    Ok(typed)
}
fn strict_root<'de, D: serde::Deserializer<'de>>(d: D) -> Result<public::RootV1, D::Error> {
    canonical(d)
}
fn strict_reference<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<public::ReferenceV1, D::Error> {
    canonical(d)
}
fn strict_references<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<public::ReferenceV1>, D::Error> {
    canonical(d)
}
fn strict_current<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<crate::valkyries_call_public_v62::CurrentV62>, D::Error> {
    canonical(d)
}
fn strict_optional_reference<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<public::ReferenceV1>, D::Error> {
    canonical(d)
}

fn strict_stable<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<crate::rl::CardStableRefV1, D::Error> {
    canonical(d)
}
