//! Public original-source budgets and actual graveyard play occurrences.
//! Native history indices, worlds and executable contracts never cross this DTO.
use crate::{
    card_def::CardType, ids::PlayerId, optional_trigger_public_v1 as public, rl::PlayerSeatV1,
    state::GameState,
};
use serde::{Deserialize, Serialize};
pub const SCHEMA: &str = "mtg-kernel/muldrotha-public/v71";
pub const TYPES: [CardType; 5] = [
    CardType::Land,
    CardType::Creature,
    CardType::Artifact,
    CardType::Enchantment,
    CardType::Planeswalker,
];
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StageV71 {
    PermissionChoice,
    SelectedPayment,
    PlayedLand,
    CastPublished,
    NoLongerPublished,
    OrdinaryRoute,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermissionV71 {
    pub source: public::RootV1,
    pub permanent_type: CardType,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetV71 {
    pub source: public::RootV1,
    pub turn: u32,
    pub active_own_turn: bool,
    pub printed_abilities_active: bool,
    pub unused: Vec<CardType>,
    pub spent: Vec<CardType>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayV71 {
    pub occurrence: u32,
    pub card: public::ReferenceV1,
    #[serde(deserialize_with = "required_optional")]
    pub current_original: Option<public::ReferenceV1>,
    pub controller: PlayerSeatV1,
    pub turn: u32,
    pub land: bool,
    pub ordinary_available: bool,
    pub permissions: Vec<PermissionV71>,
    /// The original public menu includes ordinary at0 only when available.
    #[serde(deserialize_with = "required_optional")]
    pub selected_menu_index: Option<u16>,
    pub stage: StageV71,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV71 {
    pub schema: String,
    pub producer_sha256: String,
    pub viewer: PlayerSeatV1,
    pub meaning: crate::rules_meaning_muldrotha_v71::MeaningV71,
    pub visible_printings: Vec<public::ReferenceV1>,
    pub current: Vec<crate::valkyries_call_public_v62::CurrentV62>,
    pub budgets: Vec<BudgetV71>,
    pub plays: Vec<PlayV71>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionPermissionV71 {
    pub occurrence: u32,
    pub menu_index: u16,
    #[serde(deserialize_with = "required_optional")]
    pub permission: Option<PermissionV71>,
}
pub fn action_permission(
    f: &FactsV71,
    a: &crate::rl::ActionSemanticV1,
) -> Result<Option<ActionPermissionV71>, String> {
    let crate::rl::ActionSemanticV1::ChooseEffectOption {
        source,
        option_index,
        ..
    } = a
    else {
        return Ok(None);
    };
    let Some(p) = f.plays.iter().find(|p| {
        p.stage == StageV71::PermissionChoice
            && p.card.object == source.arena_id
            && p.card.generation == source.zone_change_count
    }) else {
        return Ok(None);
    };
    let n = *option_index as usize;
    let permission = if p.ordinary_available && n == 0 {
        None
    } else {
        Some(
            p.permissions
                .get(
                    n.checked_sub(usize::from(p.ordinary_available))
                        .ok_or("V71 permission index underflow")?,
                )
                .ok_or("V71 permission index absent")?
                .clone(),
        )
    };
    Ok(Some(ActionPermissionV71 {
        occurrence: p.occurrence,
        menu_index: *option_index,
        permission,
    }))
}
pub fn producer_sha256() -> String {
    use sha2::{Digest, Sha256};
    static H: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    H.get_or_init(|| {
        let mut h = Sha256::new();
        for s in [
            SCHEMA,
            include_str!("muldrotha_public_v71.rs"),
            include_str!("engine/muldrotha_v1.rs"),
            crate::engine::time_stop_v1::source_sha256_v1(),
        ] {
            h.update((s.len() as u64).to_le_bytes());
            h.update(s.as_bytes());
        }
        format!("{:x}", h.finalize())
    })
    .clone()
}
pub fn capture(s: &GameState, v: PlayerId) -> Result<FactsV71, String> {
    crate::engine::muldrotha_v1::public_facts_v71(s, v)
}
pub fn validate(f: &FactsV71) -> Result<(), String> {
    use crate::state::Zone;
    use public::IdentityV1;
    if f.schema != SCHEMA || f.producer_sha256 != producer_sha256() {
        return Err("V71 facts identity changed".into());
    }
    crate::rules_meaning_muldrotha_v71::validate(&f.meaning)?;
    let source = |s: &public::RootV1| {
        s.current_original_source.as_ref().is_none_or(|current| {
            current.object == s.original_source.object
                && current.generation == s.original_source.generation
        }) && s.original_source.zone == Zone::Battlefield
            && s.original_source.controller == s.captured_controller
            && matches!(
                s.original_source.identity,
                IdentityV1::Known { card_id: 489 }
            )
    };
    if f.visible_printings
        .iter()
        .any(|r| !matches!(r.identity, IdentityV1::Known { card_id: 489 }))
        || f.current.iter().any(|r| {
            r.card.zone != Zone::Battlefield
                || !matches!(r.card.identity, IdentityV1::Known { card_id: 489 })
        })
    {
        return Err("V71 source printing changed".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    for b in &f.budgets {
        if !source(&b.source)
            || !seen.insert((
                b.source.original_source.object,
                b.source.original_source.generation,
            ))
            || TYPES.iter().any(|t| {
                b.unused.iter().filter(|v| *v == t).count()
                    + b.spent.iter().filter(|v| *v == t).count()
                    != 1
            })
            || b.unused.len() + b.spent.len() != 5
            || b.unused
                != TYPES
                    .into_iter()
                    .filter(|t| b.unused.contains(t))
                    .collect::<Vec<_>>()
            || b.spent
                != TYPES
                    .into_iter()
                    .filter(|t| b.spent.contains(t))
                    .collect::<Vec<_>>()
        {
            return Err("V71 exact source budget changed".into());
        }
    }
    if f.plays
        .iter()
        .filter(|p| p.stage == StageV71::PermissionChoice)
        .count()
        > 1
    {
        return Err("V71 multiple current permission scenes".into());
    }
    for (i, p) in f.plays.iter().enumerate() {
        let mut permissions = std::collections::BTreeSet::new();
        if p.occurrence as usize != i
            || p.card.zone != Zone::Graveyard
            || p.card.owner != p.controller
            || p.permissions.is_empty()
            || p.permissions.iter().any(|a| {
                !source(&a.source)
                    || a.source.captured_controller != p.controller
                    || !TYPES.contains(&a.permanent_type)
                    || p.land != (a.permanent_type == CardType::Land)
                    || !permissions.insert((
                        a.source.original_source.object,
                        a.source.original_source.generation,
                        TYPES.iter().position(|t| *t == a.permanent_type),
                    ))
            })
        {
            return Err("V71 original permission scene changed".into());
        }
        if let Some(n) = p.selected_menu_index {
            if n as usize >= p.permissions.len() + usize::from(p.ordinary_available) {
                return Err("V71 selected route absent".into());
            }
        }
        if (p.stage == StageV71::PermissionChoice) != p.selected_menu_index.is_none() {
            return Err("V71 permission stage changed".into());
        }
        if (p.stage == StageV71::OrdinaryRoute)
            != (p.selected_menu_index == Some(0) && p.ordinary_available)
        {
            return Err("V71 ordinary route changed".into());
        }
        if p.stage == StageV71::PlayedLand && !p.land
            || p.stage == StageV71::CastPublished && p.land
        {
            return Err("V71 play kind changed".into());
        }
        if let Some(c) = &p.current_original {
            if c != &p.card {
                return Err("V71 original card incarnation changed".into());
            }
        }
    }
    Ok(())
}
pub fn validate_current(s: &GameState, v: PlayerId, f: &FactsV71) -> Result<(), String> {
    if capture(s, v)? != *f {
        return Err("V71 current facts changed".into());
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

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
pub(crate) mod tests;
