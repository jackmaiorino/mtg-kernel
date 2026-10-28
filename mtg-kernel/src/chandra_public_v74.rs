//! Read-only native Chandra facts. No executable contract or private world DTO.
use crate::{
    ids::PlayerId, optional_trigger_public_v1 as public, rl::PlayerSeatV1, state::GameState,
};
use serde::{Deserialize, Serialize};
pub const SCHEMA: &str = "mtg-kernel/chandra-public/v74";
pub use crate::tinybones_public_v72::{
    CountersV72 as CountersV74, CurrentV72 as CurrentV74, IncarnationV72 as IncarnationV74,
};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetV74 {
    pub original: IncarnationV74,
    pub current_original: Option<public::ReferenceV1>,
    pub allocated_damage: Option<u8>,
    pub legal_now: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AllocationV74 {
    pub source: public::RootV1,
    pub loyalty_before: u32,
    pub targets: Vec<TargetV74>,
    pub remaining_units: u8,
    pub finished: bool,
    pub can_finish_zero: bool,
    pub candidates: Vec<public::ReferenceV1>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActivationStageV74 {
    Paid,
    Published,
    Resolving,
    NativeCompleted,
    Countered,
    Disposed,
    NotResolved,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DamageSourceV74 {
    /// Native current or last-departure source facts bound to this original
    /// damage incarnation. This does not claim a historical dealt-damage packet.
    pub source: public::RootV1,
    pub historical: bool,
    pub color_mask: u8,
    pub lifelink: bool,
    pub deathtouch: bool,
    pub werewolf: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationV74 {
    pub occurrence: u32,
    pub source: public::RootV1,
    pub ability_index: u8,
    pub loyalty_before: u32,
    pub loyalty_after: u32,
    pub targets: Vec<TargetV74>,
    pub stage: ActivationStageV74,
    pub damage_source: Option<DamageSourceV74>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExiledCardV74 {
    pub original: public::ReferenceV1,
    pub current_original: Option<public::ReferenceV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExileV74 {
    pub activation: u32,
    pub cards: Vec<ExiledCardV74>,
    pub choice_current: bool,
    pub selected_prefix: Vec<u16>,
    pub chosen: Option<u16>,
    pub grant_for_land: Option<bool>,
    pub permission_present: bool,
    pub expired: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PreventionKindV74 {
    Multiplier,
    Barricade,
    Protection,
    Shield,
    Color,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreventionV74 {
    pub menu_index: u16,
    pub kind: PreventionKindV74,
    pub source: Option<public::ReferenceV1>,
    pub shield_remaining: Option<i32>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DamageChoiceV74 {
    pub activation: u32,
    pub chooser: PlayerSeatV1,
    pub recipient: public::ReferenceV1,
    pub original_amount: i32,
    pub remaining_amount: i32,
    pub simultaneous_waiting: usize,
    pub options: Vec<PreventionV74>,
}
/// Independent public copied-token entries, including copies of copies. No
/// guessed association with a particular Chandra activation is manufactured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CopiedTokenV74 {
    pub occurrence: u32,
    pub source: public::RootV1,
    pub sacrifice_instances: u32,
    pub current_haste: Option<bool>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndStepObserverV74 {
    pub token: u32,
    pub source: public::RootV1,
    pub instances: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndStepV74 {
    pub occurrence: u32,
    pub observers: Vec<EndStepObserverV74>,
    pub collected: bool,
    pub published_count: u32,
    pub finished_count: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV74 {
    pub schema: String,
    pub producer_sha256: String,
    pub viewer: PlayerSeatV1,
    pub active_player: PlayerSeatV1,
    pub turn_key: u32,
    pub meaning: crate::rules_meaning_chandra_v74::MeaningV74,
    #[serde(deserialize_with = "strict")]
    pub authorized_printings: Vec<public::ReferenceV1>,
    #[serde(deserialize_with = "strict")]
    pub battlefield: [Vec<CurrentV74>; 2],
    #[serde(deserialize_with = "strict")]
    pub pending_allocation: Option<AllocationV74>,
    #[serde(deserialize_with = "strict")]
    pub activations: Vec<ActivationV74>,
    #[serde(deserialize_with = "strict")]
    pub exiles: Vec<ExileV74>,
    #[serde(deserialize_with = "strict")]
    pub pending_damage: Option<DamageChoiceV74>,
    #[serde(deserialize_with = "strict")]
    pub copied_tokens: Vec<CopiedTokenV74>,
    #[serde(deserialize_with = "strict")]
    pub end_steps: Vec<EndStepV74>,
    pub complete_ordinary_observation: bool,
    pub owning_consumer_admitted: bool,
}
fn strict<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned + Serialize,
{
    let raw = serde_json::Value::deserialize(d)?;
    let v: T = serde_json::from_value(raw.clone()).map_err(serde::de::Error::custom)?;
    if serde_json::to_value(&v).map_err(serde::de::Error::custom)? != raw {
        return Err(serde::de::Error::custom(
            "V74 unknown or noncanonical nested field",
        ));
    }
    Ok(v)
}
pub fn producer_sha256() -> String {
    use sha2::{Digest, Sha256};
    static H: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    H.get_or_init(|| {
        let mut h = Sha256::new();
        let m = crate::rules_meaning_chandra_v74::producer_sha256();
        for s in [SCHEMA, include_str!("chandra_public_v74.rs"), m.as_str()] {
            h.update((s.len() as u64).to_le_bytes());
            h.update(s.as_bytes());
        }
        format!("{:x}", h.finalize())
    })
    .clone()
}
pub fn capture(s: &GameState, v: PlayerId) -> Result<FactsV74, String> {
    crate::effect::chandra_flameshaper_v1::public_v74::capture(s, v)
}
pub fn validate_current(s: &GameState, v: PlayerId, f: &FactsV74) -> Result<(), String> {
    if *f != capture(s, v)? {
        return Err("V74 facts differ from exact native recapture".into());
    }
    Ok(())
}

/// Supplemental shape and printed-meaning checks. Native recapture remains the
/// authority for complete history, original scenes and current legal actions.
pub fn validate(f: &FactsV74) -> Result<(), String> {
    crate::rules_meaning_chandra_v74::validate(&f.meaning)?;
    if f.schema != SCHEMA
        || f.producer_sha256 != producer_sha256()
        || f.complete_ordinary_observation
        || f.owning_consumer_admitted
    {
        return Err("V74 native facts identity changed".into());
    }
    for (i, a) in f.activations.iter().enumerate() {
        if usize::try_from(a.occurrence).ok() != Some(i)
            || a.ability_index >= 3
            || a.source.original_source.identity != (public::IdentityV1::Known { card_id: 493 })
            || a.source.original_source.zone != crate::state::Zone::Battlefield
        {
            return Err("V74 original paid activation role changed".into());
        }
        let after = match a.ability_index {
            0 => a.loyalty_before.checked_add(2),
            1 => a.loyalty_before.checked_add(1),
            2 => a.loyalty_before.checked_sub(4),
            _ => None,
        };
        if after != Some(a.loyalty_after) {
            return Err("V74 original paid loyalty changed".into());
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationCostV74 {
    #[serde(deserialize_with = "strict")]
    pub source: crate::rl::CardStableRefV1,
    pub ability_index: u8,
    pub before: u32,
    pub after: u32,
    pub paid_before_publication: bool,
    pub countering_refunds_cost: bool,
}
pub(crate) fn activation_cost(
    f: &FactsV74,
    a: &crate::rl::ActionSemanticV1,
) -> Result<Option<ActivationCostV74>, String> {
    let crate::rl::ActionSemanticV1::ActivateAbility {
        actor,
        source,
        ability_index,
        granted_ability,
    } = a
    else {
        return Ok(None);
    };
    if source.card_db_id != 493 {
        return Ok(None);
    }
    if !granted_ability.is_none()
        || *actor != source.controller
        || source.zone != crate::state::Zone::Battlefield
    {
        return Err("V74 activation changed its original source/grant".into());
    }
    let c = f
        .battlefield
        .iter()
        .flatten()
        .find(|c| {
            c.card.object == source.arena_id
                && c.card.generation == source.zone_change_count
                && c.card.owner == source.owner
                && c.card.controller == source.controller
                && c.card.zone == source.zone
        })
        .ok_or("V74 loyalty candidate has no exact current permanent")?;
    if !c.form.printed_abilities_active
        || c.card.identity != (public::IdentityV1::Known { card_id: 493 })
    {
        return Err("V74 loyalty printed ability inactive".into());
    }
    let before = c.counters.loyalty;
    let after = match ability_index {
        0 => before.checked_add(2),
        1 => before.checked_add(1),
        2 => before.checked_sub(4),
        _ => None,
    }
    .ok_or("V74 loyalty quote exceeds native u32")?;
    Ok(Some(ActivationCostV74 {
        source: source.clone(),
        ability_index: *ability_index,
        before,
        after,
        paid_before_publication: true,
        countering_refunds_cost: false,
    }))
}
