//! Closed, sanitized revealed-entry facts. Native recapture establishes completeness;
//! standalone validation checks supplied values and never grants execution.
use crate::{
    ids::PlayerId,
    rl::{CardStableRefV1, PlayerSeatV1},
    state::{CastMethodV4, GameState, Zone},
};
use serde::{Deserialize, Serialize};
pub const SCHEMA: &str = "mtg-kernel/genesis-wave-public/v69";
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevealedV69 {
    #[serde(deserialize_with = "strict_reference")]
    pub card: CardStableRefV1,
    pub mana_value: u16,
    pub permanent: bool,
    pub eligible: bool,
    pub aura: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum StageV69 {
    Published,
    Subset,
    AuraHost { selected_index: u16 },
    GraveyardOrder,
    EntryChoices,
    RemainderCommitted,
    Finished,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachmentV69 {
    pub revealed_index: u16,
    #[serde(deserialize_with = "strict_reference")]
    pub original_host: CardStableRefV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WaveV69 {
    pub occurrence: u64,
    pub stage: StageV69,
    #[serde(deserialize_with = "strict_reference")]
    pub source: CardStableRefV1,
    pub controller: PlayerSeatV1,
    pub x_value: u16,
    pub is_copy: bool,
    pub is_flashback: bool,
    pub cast_method: CastMethodV4,
    pub revealed: Vec<RevealedV69>,
    pub selected_indices: Vec<u16>,
    pub attachments: Vec<AttachmentV69>,
    pub failed_aura_indices: Vec<u16>,
    /// Only original revealed slots, never private remaining library contents.
    pub remainder_indices: Vec<u16>,
    /// Actual selected answer prefix at the current native prompt. Indices bind
    /// the revealed prefix; an Aura host is separately represented above.
    pub answer_indices: Vec<u16>,
    #[serde(deserialize_with = "strict_optional_reference")]
    pub answer_host: Option<CardStableRefV1>,
    pub answer_waits_for_advance: bool,
    #[serde(deserialize_with = "required_optional")]
    pub spell_departure: Option<Zone>,
}
/// Small per-candidate context. Complete reveal/subset/host facts occur once
/// in the observation, avoiding a quadratic repeated prefix in wide menus.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChoiceContextV69 {
    pub occurrence: u64,
    pub stage: StageV69,
    #[serde(deserialize_with = "strict_reference")]
    pub source: CardStableRefV1,
    pub x_value: u16,
}
pub fn choice_contexts(value: &FactsV69) -> Vec<ChoiceContextV69> {
    value
        .waves
        .iter()
        .filter(|w| {
            matches!(
                w.stage,
                StageV69::Subset
                    | StageV69::AuraHost { .. }
                    | StageV69::EntryChoices
                    | StageV69::GraveyardOrder
            )
        })
        .map(|w| ChoiceContextV69 {
            occurrence: w.occurrence,
            stage: w.stage,
            source: w.source.clone(),
            x_value: w.x_value,
        })
        .collect()
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV69 {
    pub schema: String,
    pub producer_sha256: String,
    pub viewer: PlayerSeatV1,
    pub meaning: crate::rules_meaning_genesis_wave_v69::MeaningV69,
    pub visible_printings: Vec<crate::optional_trigger_public_v1::ReferenceV1>,
    pub waves: Vec<WaveV69>,
}
pub fn producer_sha256() -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    for s in [
        SCHEMA,
        include_str!("genesis_wave_public_v69.rs"),
        crate::effect::genesis_wave_v1::source_sha256_v1(),
    ] {
        h.update((s.len() as u64).to_le_bytes());
        h.update(s.as_bytes());
    }
    format!("{:x}", h.finalize())
}
pub fn capture(state: &GameState, viewer: PlayerId) -> Result<FactsV69, String> {
    crate::effect::genesis_wave_v1::public_facts_v69(state, viewer)
}
pub fn validate(value: &FactsV69) -> Result<(), String> {
    if value.schema != SCHEMA || value.producer_sha256 != producer_sha256() {
        return Err("V69 facts identity changed".into());
    }
    crate::rules_meaning_genesis_wave_v69::validate(&value.meaning)?;
    if value.visible_printings.iter().any(|r| {
        !matches!(
            r.identity,
            crate::optional_trigger_public_v1::IdentityV1::Known { card_id: 487 }
        )
    }) {
        return Err("V69 visible printing changed".into());
    }
    if choice_contexts(value).len() > 1 {
        return Err("V69 has more than one active resolving wave".into());
    }
    for (i, w) in value.waves.iter().enumerate() {
        if usize::try_from(w.occurrence).ok() != Some(i)
            || w.source.card_db_id != 487
            || w.source.controller != w.controller
            || w.source.zone != Zone::Stack
            || w.revealed.len() > usize::from(w.x_value)
            || w.is_flashback != (w.cast_method == CastMethodV4::Flashback)
        {
            return Err("V69 wave source/paid-X shape changed".into());
        }
        let mut seen = std::collections::BTreeSet::new();
        for r in &w.revealed {
            let d = crate::card_def::CARD_DEFS
                .get(usize::from(r.card.card_db_id))
                .ok_or("V69 revealed definition missing")?;
            let permanent = [
                crate::card_def::CardType::Artifact,
                crate::card_def::CardType::Creature,
                crate::card_def::CardType::Enchantment,
                crate::card_def::CardType::Land,
                crate::card_def::CardType::Planeswalker,
            ]
            .iter()
            .any(|t| d.has_type(*t));
            if r.card.zone != Zone::Library
                || r.card.controller != w.controller
                || r.mana_value != d.mana_value
                || r.permanent != permanent
                || r.eligible != (permanent && d.mana_value <= w.x_value)
                || r.aura != d.attachment.is_some()
                || !seen.insert((r.card.arena_id, r.card.zone_change_count))
            {
                return Err("V69 revealed snapshot changed".into());
            }
        }
        let valid_indices = |v: &[u16]| {
            let mut seen = std::collections::BTreeSet::new();
            v.iter()
                .all(|i| usize::from(*i) < w.revealed.len() && seen.insert(*i))
        };
        if !valid_indices(&w.selected_indices)
            || !valid_indices(&w.failed_aura_indices)
            || !valid_indices(&w.remainder_indices)
            || !valid_indices(&w.answer_indices)
            || w.selected_indices.windows(2).any(|p| p[0] >= p[1])
            || w.selected_indices
                .iter()
                .any(|i| !w.revealed[usize::from(*i)].eligible)
            || w.failed_aura_indices
                .iter()
                .any(|i| !w.selected_indices.contains(i) || !w.revealed[usize::from(*i)].aura)
        {
            return Err("V69 original revealed subset relation changed".into());
        }
        if matches!(w.stage,StageV69::AuraHost{selected_index} if w.selected_indices.get(usize::from(selected_index)).is_none_or(|i|!w.revealed[usize::from(*i)].aura))
        {
            return Err("V69 Aura cursor changed".into());
        }
        let mut attached = std::collections::BTreeSet::new();
        if w.attachments.iter().any(|a| {
            !w.selected_indices.contains(&a.revealed_index)
                || !w.revealed[usize::from(a.revealed_index)].aura
                || w.failed_aura_indices.contains(&a.revealed_index)
                || !attached.insert(a.revealed_index)
                || a.original_host.zone != Zone::Battlefield
        }) {
            return Err("V69 original Aura-host relation changed".into());
        }
        if w.answer_host
            .as_ref()
            .is_some_and(|r| r.zone != Zone::Battlefield)
            || w.answer_host.is_some() && !matches!(w.stage, StageV69::AuraHost { .. })
        {
            return Err("V69 current Aura answer changed".into());
        }
        if matches!(w.stage, StageV69::Published)
            && (!w.revealed.is_empty()
                || !w.selected_indices.is_empty()
                || !w.answer_indices.is_empty())
            || w.spell_departure.is_some() && w.stage != StageV69::Finished
        {
            return Err("V69 stage/departure changed".into());
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

fn strict_reference<'de, D: serde::Deserializer<'de>>(d: D) -> Result<CardStableRefV1, D::Error> {
    use serde::de::Error;
    let value = serde_json::Value::deserialize(d)?;
    let typed: CardStableRefV1 = serde_json::from_value(value.clone()).map_err(D::Error::custom)?;
    if serde_json::to_value(&typed).map_err(D::Error::custom)? != value {
        return Err(D::Error::custom(
            "V69 requires exact canonical public reference fields",
        ));
    }
    Ok(typed)
}
fn strict_optional_reference<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<CardStableRefV1>, D::Error> {
    use serde::de::Error;
    let value = serde_json::Value::deserialize(d)?;
    let typed: Option<CardStableRefV1> =
        serde_json::from_value(value.clone()).map_err(D::Error::custom)?;
    if serde_json::to_value(&typed).map_err(D::Error::custom)? != value {
        return Err(D::Error::custom(
            "V69 requires exact optional reference fields",
        ));
    }
    Ok(typed)
}
