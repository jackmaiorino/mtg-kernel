//! Read-only native facts. Public occurrences carry no journal indices or worlds.
use crate::{
    card_def::CardType, ids::PlayerId, optional_trigger_public_v1 as public, rl::PlayerSeatV1,
    state::GameState,
};
use serde::{Deserialize, Serialize};
pub const SCHEMA: &str = "mtg-kernel/liliana-public/v73";
pub use crate::tinybones_public_v72::{
    CountersV72 as CountersV73, CurrentV72 as CurrentV73, IncarnationV72 as IncarnationV73,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActivationStageV73 {
    Paid,
    Published,
    Resolving,
    NativeCompleted,
    Disposed,
    NoLongerPublished,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationV73 {
    pub occurrence: u32,
    pub source: public::RootV1,
    pub turn: u32,
    pub ability_index: u8,
    pub loyalty_before: u32,
    pub loyalty_after: u32,
    pub stage: ActivationStageV73,
    pub created_zombies: Vec<IncarnationV73>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneObjectV73 {
    pub original: IncarnationV73,
    pub permanent_types: Vec<CardType>,
    pub current_original: Option<public::ReferenceV1>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SelectionKindV73 {
    SacrificeCreatures,
    KeepPermanent(CardType),
    OrderGraveyard,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeeperV73 {
    pub permanent_type: CardType,
    pub permanent: IncarnationV73,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionV73 {
    pub occurrence: u32,
    pub phase: u8,
    pub actor: PlayerSeatV1,
    pub kind: SelectionKindV73,
    pub candidates: Vec<IncarnationV73>,
    pub exact_count: u16,
    pub ordered: bool,
    pub selected_prefix: Vec<IncarnationV73>,
    pub current: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolutionV73 {
    pub activation: u32,
    pub active_player: PlayerSeatV1,
    pub controller: PlayerSeatV1,
    pub ultimate: bool,
    pub rosters: [Vec<SceneObjectV73>; 2],
    pub selections: Vec<SelectionV73>,
    pub chosen_sacrifices: [Vec<IncarnationV73>; 2],
    pub keepers: Vec<KeeperV73>,
    pub graveyard_orders: [Vec<IncarnationV73>; 2],
    pub completed_sacrifices: Vec<IncarnationV73>,
    pub finished: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeathV73 {
    pub occurrence: u32,
    pub observer: public::RootV1,
    pub creature: IncarnationV73,
    pub collected: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DrawStageV73 {
    Held,
    Published,
    Resolving,
}
/// Identical draw triggers from the same observer have no invented one-to-one
/// association with individual deaths. Their actual current positions are public.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrawV73 {
    pub source: public::RootV1,
    pub stage: DrawStageV73,
    pub native_position: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV73 {
    pub schema: String,
    pub producer_sha256: String,
    pub viewer: PlayerSeatV1,
    pub active_player: PlayerSeatV1,
    pub turn_key: u32,
    pub meaning: crate::rules_meaning_liliana_v73::MeaningV73,
    #[serde(deserialize_with = "strict")]
    pub authorized_printings: Vec<public::ReferenceV1>,
    #[serde(deserialize_with = "strict")]
    pub battlefield: [Vec<CurrentV73>; 2],
    #[serde(deserialize_with = "strict")]
    pub activations: Vec<ActivationV73>,
    #[serde(deserialize_with = "strict")]
    pub resolutions: Vec<ResolutionV73>,
    #[serde(deserialize_with = "strict")]
    pub deaths: Vec<DeathV73>,
    #[serde(deserialize_with = "strict")]
    pub current_draws: Vec<DrawV73>,
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
            "V73 noncanonical or unknown nested field",
        ));
    }
    Ok(v)
}
pub fn producer_sha256() -> String {
    use sha2::{Digest, Sha256};
    static H: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    H.get_or_init(|| {
        let mut h = Sha256::new();
        let meaning = crate::rules_meaning_liliana_v73::producer_sha256();
        for s in [
            SCHEMA,
            include_str!("liliana_public_v73.rs"),
            meaning.as_str(),
        ] {
            h.update((s.len() as u64).to_le_bytes());
            h.update(s.as_bytes());
        }
        format!("{:x}", h.finalize())
    })
    .clone()
}
pub fn capture(s: &GameState, v: PlayerId) -> Result<FactsV73, String> {
    crate::effect::liliana_dreadhorde_v1::public_v73::capture(s, v)
}
/// Supplied values are validated only by exact native recapture. A valid appendix
/// never grants ordinary observation, candidate issuance, or execution authority.
pub fn validate_current(s: &GameState, v: PlayerId, f: &FactsV73) -> Result<(), String> {
    if *f != capture(s, v)? {
        return Err("V73 facts changed from exact native recapture".into());
    }
    Ok(())
}

/// Supplemental shape and printed-meaning checks. Native recapture remains the
/// authority for complete history, original scenes and current legal actions.
pub fn validate(f: &FactsV73) -> Result<(), String> {
    crate::rules_meaning_liliana_v73::validate(&f.meaning)?;
    if f.schema != SCHEMA
        || f.producer_sha256 != producer_sha256()
        || f.complete_ordinary_observation
        || f.owning_consumer_admitted
    {
        return Err("V73 native facts identity changed".into());
    }
    for (i, a) in f.activations.iter().enumerate() {
        if usize::try_from(a.occurrence).ok() != Some(i)
            || a.ability_index >= 3
            || a.source.original_source.identity != (public::IdentityV1::Known { card_id: 491 })
            || a.source.original_source.zone != crate::state::Zone::Battlefield
        {
            return Err("V73 original paid activation role changed".into());
        }
        let after = match a.ability_index {
            0 => a.loyalty_before.checked_add(1),
            1 => a.loyalty_before.checked_sub(4),
            2 => a.loyalty_before.checked_sub(9),
            _ => None,
        };
        if after != Some(a.loyalty_after) {
            return Err("V73 original paid loyalty changed".into());
        }
    }
    Ok(())
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationCostV73 {
    #[serde(deserialize_with = "strict")]
    pub source: crate::rl::CardStableRefV1,
    pub ability_index: u8,
    pub before: u32,
    pub after: u32,
    pub paid_before_publication: bool,
    pub countering_refunds_cost: bool,
}
pub(crate) fn activation_cost(
    f: &FactsV73,
    a: &crate::rl::ActionSemanticV1,
) -> Result<Option<ActivationCostV73>, String> {
    let crate::rl::ActionSemanticV1::ActivateAbility {
        actor,
        source,
        ability_index,
        granted_ability,
    } = a
    else {
        return Ok(None);
    };
    if source.card_db_id != 491 {
        return Ok(None);
    }
    if !granted_ability.is_none()
        || *actor != source.controller
        || source.zone != crate::state::Zone::Battlefield
    {
        return Err("V73 activation changed its original source/grant".into());
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
        .ok_or("V73 loyalty candidate has no exact current permanent")?;
    if !c.form.printed_abilities_active
        || c.card.identity != (public::IdentityV1::Known { card_id: 491 })
    {
        return Err("V73 loyalty printed ability inactive".into());
    }
    let before = c.counters.loyalty;
    let after = match ability_index {
        0 => before.checked_add(1),
        1 => before.checked_sub(4),
        2 => before.checked_sub(9),
        _ => None,
    }
    .ok_or("V73 loyalty quote exceeds native u32")?;
    Ok(Some(ActivationCostV73 {
        source: source.clone(),
        ability_index: *ability_index,
        before,
        after,
        paid_before_publication: true,
        countering_refunds_cost: false,
    }))
}
pub(crate) fn choice_contexts(f: &FactsV73) -> Vec<SelectionV73> {
    f.resolutions
        .iter()
        .flat_map(|r| r.selections.iter().filter(|s| s.current).cloned())
        .collect()
}
