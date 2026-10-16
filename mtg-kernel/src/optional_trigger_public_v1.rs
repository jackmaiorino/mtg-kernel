//! Shared safe public references and current form. No private history or executable DTO.
use crate::{
    card_def::{CardType, Keywords},
    ids::{ObjectId, PlayerId},
    rl::PlayerSeatV1,
    state::{AbilitySourceContractV4, Counters, GameState, Zone},
};
use serde::{Deserialize, Serialize};
#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[path = "optional_trigger_public_v1_tests.rs"]
mod tests;
fn error(message: impl Into<String>) -> String {
    message.into()
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum IdentityV1 {
    Known { card_id: u16 },
    FaceDown { disguised: bool },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceV1 {
    pub object: u32,
    pub generation: u32,
    pub owner: PlayerSeatV1,
    pub controller: PlayerSeatV1,
    pub zone: Zone,
    pub identity: IdentityV1,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum SupertypeV1 {
    Basic,
    Snow,
    Legendary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CountersV1 {
    pub plus_one: i32,
    pub minus_one: i16,
    pub minus_toughness: i16,
    pub stun: i16,
    pub lore: i16,
    pub oil: i16,
    pub charge: i32,
    pub net: i32,
    pub bait: i32,
    pub soul: i32,
    pub incubation: i32,
    pub lifelink: i16,
    pub time: u8,
    pub finality: u16,
    pub loyalty: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormV1 {
    pub names: Vec<String>,
    pub mana_value: u16,
    pub types: Vec<CardType>,
    pub supertypes: Vec<SupertypeV1>,
    pub subtype_ids: Vec<u16>,
    pub color_mask: u8,
    pub base_power: Option<i32>,
    pub base_toughness: Option<i32>,
    pub power: Option<i32>,
    pub toughness: Option<i32>,
    pub keyword_bits: u32,
    pub ward_generic: u16,
    pub minimum_blockers: usize,
    pub printed_abilities_active: bool,
    pub abilities_removed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentV1 {
    pub source: ReferenceV1,
    pub is_token: bool,
    pub tapped: bool,
    pub summoning_sick: bool,
    pub marked_damage: u32,
    pub counters: CountersV1,
    pub form: FormV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootV1 {
    pub original_source: ReferenceV1,
    pub current_original_source: Option<ReferenceV1>,
    pub captured_controller: PlayerSeatV1,
}
pub(crate) fn reference(
    state: &GameState,
    id: ObjectId,
    viewer: Option<PlayerId>,
) -> Option<ReferenceV1> {
    let o = state.objects.try_get(id)?;
    let visible = match o.zone {
        Zone::Hand => viewer.is_some_and(|v| {
            v == o.owner
                || state
                    .known_hand_cards(v, o.owner)
                    .iter()
                    .any(|k| k.object == id && k.zone_change_count == o.zone_change_count)
        }),
        Zone::Library => viewer.is_some_and(|v| {
            state
                .known_library_cards(v, o.owner)
                .iter()
                .any(|k| k.object == id && k.zone_change_count == o.zone_change_count)
        }),
        _ => true,
    };
    if !visible {
        return None;
    }
    Some(ReferenceV1 {
        object: id.0,
        generation: o.zone_change_count,
        owner: o.owner.into(),
        controller: o.controller.into(),
        zone: o.zone,
        identity: o.v4.face_down_v1.map_or(
            IdentityV1::Known {
                card_id: o.card_def,
            },
            |f| IdentityV1::FaceDown {
                disguised: f.disguised,
            },
        ),
    })
}
pub(crate) fn root(
    state: &GameState,
    s: AbilitySourceContractV4,
    viewer: Option<PlayerId>,
) -> Result<RootV1, String> {
    if s.zone != Zone::Battlefield {
        return Err("optional root original source is not battlefield".into());
    }
    crate::engine::validate_historical_ability_source_contract(state, s)?;
    // This definition-owned trigger was public when captured. Current hidden zones
    // and a later incarnation do not become a new identity or generation oracle.
    let original_source = ReferenceV1 {
        object: s.source.0,
        generation: s.zone_change_count,
        owner: s.owner.into(),
        controller: s.controller.into(),
        zone: s.zone,
        identity: state
            .objects
            .try_get(s.source)
            .filter(|o| o.zone_change_count == s.zone_change_count)
            .and_then(|o| o.v4.face_down_v1)
            .map_or(
                IdentityV1::Known {
                    card_id: s.card_def,
                },
                |face| IdentityV1::FaceDown {
                    disguised: face.disguised,
                },
            ),
    };
    let current_original_source = state
        .objects
        .try_get(s.source)
        .filter(|o| o.zone_change_count == s.zone_change_count)
        .and_then(|_| reference(state, s.source, viewer));
    Ok(RootV1 {
        original_source,
        current_original_source,
        captured_controller: s.controller.into(),
    })
}
pub(crate) fn current(state: &GameState, id: u16) -> Result<Vec<CurrentV1>, String> {
    current_with_form(state, id, form)
}
/// Successor-only current form query. The roster, concealment and printed-ID
/// selection stay identical to the frozen helper; only selected public forms
/// use the engine's shared checked final PT result.
pub(crate) fn current_checked(state: &GameState, id: u16) -> Result<Vec<CurrentV1>, String> {
    current_with_form(state, id, checked_form)
}
fn current_with_form(
    state: &GameState,
    id: u16,
    form: fn(&GameState, ObjectId) -> Result<FormV1, String>,
) -> Result<Vec<CurrentV1>, String> {
    let mut rows = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for (seat, p) in state.players.iter().enumerate() {
        for &object in &p.battlefield {
            let o = state
                .objects
                .try_get(object)
                .ok_or("public battlefield object absent")?;
            if o.zone != Zone::Battlefield || o.controller.index() != seat || !seen.insert(object) {
                return Err("public battlefield roster changed".into());
            }
            // Concealment is tested before the underlying printing.
            if o.v4.face_down_v1.is_none() && o.v4.face_index == 0 && o.card_def == id {
                rows.push(CurrentV1 {
                    source: reference(state, object, None).ok_or("public source absent")?,
                    is_token: o.v4.is_token,
                    tapped: o.tapped,
                    summoning_sick: o.summoning_sick,
                    marked_damage: o.damage,
                    counters: counters(state, object)?,
                    form: form(state, object)?,
                });
            }
        }
    }
    if state
        .objects
        .iter()
        .any(|(id, o)| o.zone == Zone::Battlefield && !seen.contains(&id))
    {
        return Err("public battlefield roster incomplete".into());
    }
    Ok(rows)
}
pub(crate) fn held(state: &GameState) -> Vec<&crate::trigger::PendingTrigger> {
    state
        .engine
        .pending_triggers
        .iter()
        .chain(state.legend_pending_v1.iter().flatten())
        .chain(
            state
                .pending_legend_rule_v1
                .iter()
                .flat_map(|p| p.waiting_triggers()),
        )
        .chain(
            state
                .standard_v1
                .iter()
                .filter_map(|s| s.resolution_play.as_ref())
                .flat_map(|p| p.deferred_triggers.iter()),
        )
        .collect()
}
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    validate_state_with_pt(state, false)
}
/// Keep all native root/pending checks while using the current engine's checked
/// final PT envelope. Existing predecessor validators keep the frozen prefix.
pub(crate) fn validate_state_checked(state: &GameState) -> Result<(), String> {
    validate_state_with_pt(state, true)
}
fn validate_state_with_pt(state: &GameState, checked: bool) -> Result<(), String> {
    if crate::linked_aura_operation_v1::locally_owned(state) {
        return Err("public facts cannot project a private synchronous operation tail".into());
    }
    for (id, o) in state.objects.iter() {
        if o.zone == Zone::Battlefield {
            if checked {
                let has_pt = crate::engine::effective_base_power(state, id).is_some()
                    || crate::engine::effective_base_toughness(state, id).is_some();
                if has_pt {
                    crate::engine::checked_effective_pt(state, id)?;
                }
            } else {
                native_pt_prefix(state, id)?;
            }
        }
    }
    crate::effect::validate_pending_effect_choice(state)?;
    for item in &state.stack {
        crate::engine::validated_stack_item_target_spec(item, state)?;
    }
    for p in held(state) {
        crate::engine::validate_pending_trigger_identity(state, p)?;
    }
    Ok(())
}
fn counters(state: &GameState, id: ObjectId) -> Result<CountersV1, String> {
    let live = state.objects.get(id);
    // Exhaustive destructuring requires a deliberate optional trigger decision for any
    // newly added storage family, rather than silently losing its counters.
    let Counters {
        plus1_plus1,
        minus1_minus1,
        minus0_minus1,
        stun,
        lore,
        oil,
        charge,
        net,
        bait,
        soul,
        incubation,
    } = live.counters;
    let lifelink = live.v4.lifelink_keyword_counters;
    if [
        plus1_plus1,
        i32::from(minus1_minus1),
        i32::from(minus0_minus1),
        i32::from(stun),
        i32::from(lore),
        i32::from(oil),
        charge,
        net,
        bait,
        soul,
        incubation,
        i32::from(lifelink),
    ]
    .iter()
    .any(|n| *n < 0)
    {
        return Err(error(
            "optional trigger counter underflow in public battlefield",
        ));
    }
    Ok(CountersV1 {
        plus_one: plus1_plus1,
        minus_one: minus1_minus1,
        minus_toughness: minus0_minus1,
        stun,
        lore,
        oil,
        charge,
        net,
        bait,
        soul,
        incubation,
        lifelink,
        time: live.v4.time_counters_v1,
        finality: live.v4.creature_upgrade.as_ref().map_or(0, |u| u.finality),
        loyalty: crate::planeswalker_v1::loyalty(state, id).unwrap_or(0),
    })
}
/// Guard the exact unsafe base/counter prefix in the current native query.
/// Query the public effective base, including face-down/copy/Aura settings.
/// This is a refusal, not a separate evaluator or a saturation policy.
fn native_pt_prefix(state: &GameState, id: ObjectId) -> Result<(Option<i32>, Option<i32>), String> {
    let live = state.objects.get(id);
    let power = crate::engine::effective_base_power(state, id);
    let toughness = crate::engine::effective_base_toughness(state, id);
    if power.is_some() || toughness.is_some() {
        let checked = |base: i32, minus_toughness: i16| {
            base.checked_add(live.counters.plus1_plus1)
                .and_then(|n| n.checked_sub(i32::from(live.counters.minus1_minus1)))
                .and_then(|n| n.checked_sub(i32::from(minus_toughness)))
        };
        if checked(power.unwrap_or(0), 0).is_none()
            || checked(toughness.unwrap_or(0), live.counters.minus0_minus1).is_none()
        {
            return Err(error("optional trigger current effective base/counter PT exceeds native engine range; shared checked query required"));
        }
    }
    Ok((power, toughness))
}
fn form(state: &GameState, id: ObjectId) -> Result<FormV1, String> {
    form_inner(state, id, false)
}
fn checked_form(state: &GameState, id: ObjectId) -> Result<FormV1, String> {
    form_inner(state, id, true)
}
fn form_inner(state: &GameState, id: ObjectId, checked: bool) -> Result<FormV1, String> {
    let live = state.objects.get(id);
    let (base_power, base_toughness) = if checked {
        (
            crate::engine::effective_base_power(state, id),
            crate::engine::effective_base_toughness(state, id),
        )
    } else {
        native_pt_prefix(state, id)?
    };
    let has_pt = base_power.is_some() || base_toughness.is_some();
    let checked_pt = if checked && has_pt {
        Some(crate::engine::checked_effective_pt(state, id)?)
    } else {
        None
    };
    let printed = crate::continuous_characteristics_v1::printed_abilities_active(state, id);
    let removed = crate::continuous_characteristics_v1::removal_timestamp(state, id).is_some();
    let keyword_bits = (0..u32::BITS).fold(0, |bits, bit| {
        let keyword = Keywords(1_u32 << bit);
        if crate::engine::has_effective_keyword(state, id, keyword) {
            bits | keyword.0
        } else {
            bits
        }
    });
    Ok(FormV1 {
        names: crate::engine::effective_names(state, id)
            .into_iter()
            .map(str::to_owned)
            .collect(),
        mana_value: crate::engine::object_mana_value(state, id),
        types: [
            CardType::Land,
            CardType::Creature,
            CardType::Instant,
            CardType::Sorcery,
            CardType::Artifact,
            CardType::Enchantment,
            CardType::Planeswalker,
        ]
        .into_iter()
        .filter(|kind| crate::engine::object_has_type(state, id, *kind))
        .collect(),
        supertypes: crate::engine::effective_supertypes(state, id)
            .iter()
            .map(|kind| match kind {
                crate::card_def::Supertype::Basic => SupertypeV1::Basic,
                crate::card_def::Supertype::Snow => SupertypeV1::Snow,
                crate::card_def::Supertype::Legendary => SupertypeV1::Legendary,
            })
            .collect(),
        subtype_ids: crate::engine::effective_subtype_ids(state, id),
        color_mask: crate::engine::object_color_mask(state, id),
        base_power,
        base_toughness,
        power: has_pt.then(|| {
            checked_pt.map_or_else(|| crate::engine::effective_power(state, id), |pt| pt.0)
        }),
        toughness: has_pt.then(|| {
            checked_pt.map_or_else(|| crate::engine::effective_toughness(state, id), |pt| pt.1)
        }),
        keyword_bits,
        ward_generic: if live.v4.face_down_v1.is_some_and(|face| face.disguised) && !removed {
            2
        } else if printed {
            live.v4.ward_generic
        } else {
            0
        },
        minimum_blockers: if crate::engine::object_has_type(state, id, CardType::Creature) {
            crate::engine::minimum_blockers_required(state, id)
        } else {
            0
        },
        printed_abilities_active: printed,
        abilities_removed: removed,
    })
}
