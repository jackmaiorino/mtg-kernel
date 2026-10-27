//! Standalone current Elenda facts. No historical targeting inputs or backend admission.
use crate::{
    card_def::{CardType, Keywords},
    ids::ObjectId,
    rl::{CardStableRefV1, RlContractError},
    rules_meaning_elenda_v52::{self as meaning, MeaningV52, RootV52, SupertypeV52},
    state::{GameState, Zone},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/elenda-public/v52";

/// Predecessors cannot publish these mechanics through a generic keyword row.
/// Only the actor's authorized identities and public zones select this refusal.
pub(crate) fn require_legacy_visible(
    state: &GameState,
    actor: Option<crate::ids::PlayerId>,
) -> Result<(), String> {
    for (id, object) in state.objects.iter() {
        if object.v4.face_index != 0 {
            continue;
        }
        // Only an absent definition or one with these mechanics can refuse,
        // so visibility is worked out for those objects alone: the same
        // first refusal, in the same object order.
        let definition = crate::card_def::CARD_DEFS.get(usize::from(object.card_def));
        if definition.is_some_and(|definition| {
            !(definition.is_executable()
                && (!definition.starting_life_self_bonuses_v1.is_empty()
                    || definition.source_type_hexproof_v1.is_some()))
        }) {
            continue;
        }
        let visible = match object.zone {
            Zone::Hand => actor.is_some_and(|actor| {
                actor == object.owner
                    || state
                        .known_hand_cards(actor, object.owner)
                        .iter()
                        .any(|known| {
                            known.object == id
                                && known.zone_change_count == object.zone_change_count
                        })
            }),
            Zone::Library => actor.is_some_and(|actor| {
                state
                    .known_library_cards(actor, object.owner)
                    .iter()
                    .any(|known| {
                        known.object == id && known.zone_change_count == object.zone_change_count
                    })
            }),
            _ => true,
        };
        let known_form = object.v4.face_down_v1.is_none()
            || actor.is_some_and(|actor| {
                object
                    .v4
                    .face_down_v1
                    .is_some_and(|face| face.lookers & (1 << actor.index()) != 0)
            });
        if visible && known_form {
            definition.ok_or("visible Elenda boundary definition absent")?;
            return Err("starting-life bonuses and source-type hexproof require Elenda V52".into());
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BonusV52 {
    pub minimum_above_start: u16,
    pub condition_satisfied: bool,
    /// A satisfied condition grants nothing while the printed ability is lost.
    pub applied: bool,
    pub power: i16,
    pub toughness: i16,
    pub grants_menace: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentV52 {
    pub source: CardStableRefV1,
    pub token: bool,
    pub effective_name: String,
    pub effective_types: Vec<CardType>,
    pub effective_supertypes: Vec<SupertypeV52>,
    pub effective_subtype_ids: Vec<u16>,
    pub effective_color_mask: u8,
    pub starting_life: i32,
    pub controller_life: i32,
    pub printed_abilities_active: bool,
    pub ordered_bonuses: Vec<BonusV52>,
    pub applied_power_bonus: i32,
    pub applied_toughness_bonus: i32,
    pub effective_power: Option<i32>,
    pub effective_toughness: Option<i32>,
    pub effective_lifelink: bool,
    pub effective_menace: bool,
    /// The immutable meaning specifies opponent-only protection, effective
    /// selected source types, and announcement plus resolution checks.
    /// This flag describes the current protection, never a targeting receipt.
    pub hexproof_from_opponents_instants: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV52 {
    pub schema: String,
    pub producer_sha256: String,
    pub card_db_hash: u64,
    pub meaning: MeaningV52,
    /// Public battlefield sources only, in physical roster order. A face-down
    /// permanent never reveals its underlying Elenda printing through this list.
    pub current: Vec<CurrentV52>,
}

pub fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        let meaning_producer = meaning::producer_sha256();
        for part in [
            SCHEMA,
            include_str!("elenda_public_v52.rs"),
            meaning_producer.as_str(),
        ] {
            hash.update((part.len() as u64).to_le_bytes());
            hash.update(part.as_bytes());
        }
        hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
    })
    .clone()
}

fn current(state: &GameState, object: ObjectId) -> Result<CurrentV52, RlContractError> {
    let live = state.objects.get(object);
    let facts = crate::starting_life_statics_v1::characteristics(state, object)
        .ok_or_else(|| RlContractError("Elenda current rules are absent".into()))?;
    let effective_types = [
        CardType::Land,
        CardType::Creature,
        CardType::Instant,
        CardType::Sorcery,
        CardType::Artifact,
        CardType::Enchantment,
        CardType::Planeswalker,
    ]
    .into_iter()
    .filter(|kind| crate::engine::object_has_type(state, object, *kind))
    .collect::<Vec<_>>();
    let creature = effective_types.contains(&CardType::Creature);
    let pt = creature
        .then(|| crate::engine::checked_effective_pt(state, object))
        .transpose()
        .map_err(RlContractError)?;
    Ok(CurrentV52 {
        source: crate::rl::card_ref(state, object)?,
        token: live.v4.is_token,
        effective_name: crate::engine::effective_name(state, object).into(),
        effective_types,
        effective_supertypes: crate::engine::effective_supertypes(state, object)
            .iter()
            .copied()
            .map(SupertypeV52::from)
            .collect(),
        effective_subtype_ids: crate::engine::effective_subtype_ids(state, object),
        effective_color_mask: crate::engine::object_color_mask(state, object),
        starting_life: facts.starting_life,
        controller_life: facts.controller_life,
        printed_abilities_active: facts.printed_abilities_active,
        ordered_bonuses: facts
            .rules
            .iter()
            .map(|rule| {
                let condition_satisfied = i64::from(facts.controller_life)
                    - i64::from(facts.starting_life)
                    >= i64::from(rule.minimum_above_start);
                BonusV52 {
                    minimum_above_start: rule.minimum_above_start,
                    condition_satisfied,
                    applied: condition_satisfied && facts.printed_abilities_active,
                    power: rule.power,
                    toughness: rule.toughness,
                    grants_menace: rule.keywords.has(Keywords::MENACE),
                }
            })
            .collect(),
        applied_power_bonus: facts.bonus.power,
        applied_toughness_bonus: facts.bonus.toughness,
        effective_power: pt.map(|value| value.0),
        effective_toughness: pt.map(|value| value.1),
        effective_lifelink: crate::engine::has_effective_keyword(state, object, Keywords::LIFELINK),
        effective_menace: crate::engine::has_effective_keyword(state, object, Keywords::MENACE),
        hexproof_from_opponents_instants: crate::source_type_hexproof_v1::characteristics(
            state, object,
        )
        .is_some(),
    })
}

pub fn capture(state: &GameState) -> Result<FactsV52, RlContractError> {
    crate::engine::validate_battlefield_pt(state)
        .map_err(|id| RlContractError(format!("native PT envelope exceeded for {id:?}")))?;
    let id = crate::card_def::card_id_by_name("Elenda, Saint of Dusk")
        .ok_or_else(|| RlContractError("Elenda public facts require the actual printing".into()))?;
    let meaning = meaning::printed_meaning(id, RootV52::CreatureSpellAndPrintedStatics)
        .map_err(|e| RlContractError(format!("Elenda public meaning: {e:?}")))?;
    let mut seen = std::collections::BTreeSet::new();
    let mut rows = Vec::new();
    for (seat, player) in state.players.iter().enumerate() {
        for object in &player.battlefield {
            let live = state
                .objects
                .try_get(*object)
                .ok_or_else(|| RlContractError("Elenda public battlefield object absent".into()))?;
            if live.zone != Zone::Battlefield
                || live.controller.index() != seat
                || !seen.insert(*object)
            {
                return Err(RlContractError(
                    "Elenda public battlefield roster invalid".into(),
                ));
            }
            // Check visibility before the underlying printing. No face-down
            // knowledge permission changes a public-only source roster.
            if live.v4.face_down_v1.is_none() && live.v4.face_index == 0 && live.card_def == id {
                rows.push(current(state, *object)?);
            }
        }
    }
    if state
        .objects
        .iter()
        .any(|(object, live)| live.zone == Zone::Battlefield && !seen.contains(&object))
    {
        return Err(RlContractError(
            "Elenda public battlefield roster incomplete".into(),
        ));
    }
    Ok(FactsV52 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        meaning,
        current: rows,
    })
}

/// A recomputed digest or plausible DTO never substitutes for current owned state.
pub fn validate(state: &GameState, value: &FactsV52) -> Result<(), RlContractError> {
    if capture(state)? == *value {
        Ok(())
    } else {
        Err(RlContractError(
            "Elenda public facts differ from current state".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn catalog_selection_preserves_default_and_standard_refusal() {
        let state = crate::state::GameState::new_from_libraries(&[], &[], |_| String::new(), 0);
        if !cfg!(all(
            feature = "limited-fdn-fixtures",
            not(feature = "standard-magezero-fixtures")
        )) {
            assert!(super::capture(&state).is_err());
        } else {
            let facts = super::capture(&state).unwrap();
            assert!(facts.current.is_empty());
            assert!(!facts.meaning.model_admitted);
            super::validate(&state, &facts).unwrap();
        }
    }
}
