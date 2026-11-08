//! Explicit public Eternal facts missing from ObservationV5. This is a partial
//! additive supplement, not a complete observation or model-admission permit.
//! Never serialize EternalStateV1: it also contains unrevealed private choices.
use crate::{
    card_def::Subtype,
    rl::{CardStableRefV1, RlContractError},
    state::{GameState, ObjectLinkV4, Zone},
};
use serde::{Deserialize, Serialize};

pub const SCHEMA_V1: &str = "mtg-kernel/eternal-actor-public-choices/v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChosenTypeV1 {
    pub source: CardStableRefV1,
    pub chosen_type: Subtype,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChosenNameV1 {
    pub source: CardStableRefV1,
    /// Publicly named printing, not a reference to any object with that name.
    pub named_card_id: u16,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CastManaV1 {
    pub spell: CardStableRefV1,
    pub total: u16,
    pub colored: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SagaAbilitiesV1 {
    pub source: CardStableRefV1,
    pub highest_gained_chapter: u8,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicChoicesV1 {
    pub schema: String,
    pub energy: [u32; 2],
    pub city_blessing: [bool; 2],
    pub cavern_types: Vec<ChosenTypeV1>,
    pub needle_names: Vec<ChosenNameV1>,
    pub uncounterable_spells: Vec<CardStableRefV1>,
    pub cast_mana: Vec<CastManaV1>,
    pub saga_abilities: Vec<SagaAbilitiesV1>,
}

/// Check public zone and exact incarnation before asking for a projected
/// reference. A departed or recycled physical source never reveals its current
/// hidden definition through an old public choice. A concealed current source
/// refuses until public correspondence continuity has its own validated path.
/// Facts retained across a face-up transform are historical choices, not a
/// claim that the source's old printed ability is currently active.
fn current_public(
    state: &GameState,
    link: ObjectLinkV4,
    zone: Zone,
) -> Result<Option<CardStableRefV1>, RlContractError> {
    if !matches!(zone, Zone::Battlefield | Zone::Stack) {
        return Err(RlContractError("nonpublic Eternal link zone".into()));
    }
    let Some(object) = state.objects.try_get(link.object) else {
        return Ok(None);
    };
    if object.zone != zone || object.zone_change_count != link.zone_change_count {
        return Ok(None);
    }
    if object.v4.face_down_v1.is_some() {
        return Err(RlContractError(
            "concealed Eternal choice source requires public continuity".into(),
        ));
    }
    crate::rl::card_ref(state, link.object).map(Some)
}

/// Public choice operands only. Other Eternal dynamic mechanisms remain
/// unrepresented and must refuse in any consumer seeking complete admission.
pub fn capture_v1(state: &GameState) -> Result<PublicChoicesV1, RlContractError> {
    let mut out = PublicChoicesV1 {
        schema: SCHEMA_V1.into(),
        energy: [0; 2],
        city_blessing: [false; 2],
        cavern_types: Vec::new(),
        needle_names: Vec::new(),
        uncounterable_spells: Vec::new(),
        cast_mana: Vec::new(),
        saga_abilities: Vec::new(),
    };
    let Some(eternal) = &state.eternal_v1 else {
        return Ok(out);
    };
    out.energy = eternal.energy;
    out.city_blessing = eternal.e5b.city_blessing;
    if let Some(e3) = &eternal.e3 {
        for &(link, chosen_type) in &e3.cavern_types {
            if let Some(source) = current_public(state, link, Zone::Battlefield)? {
                out.cavern_types.push(ChosenTypeV1 {
                    source,
                    chosen_type,
                });
            }
        }
        for &link in &e3.uncounterable_spells {
            if let Some(spell) = current_public(state, link, Zone::Stack)? {
                out.uncounterable_spells.push(spell);
            }
        }
    }
    for &(link, named_card_id) in &eternal.lurrus.needle_names {
        if let Some(source) = current_public(state, link, Zone::Battlefield)? {
            if crate::card_def::CARD_DEFS
                .get(usize::from(named_card_id))
                .is_none()
            {
                return Err(RlContractError("invalid public named printing".into()));
            }
            out.needle_names.push(ChosenNameV1 {
                source,
                named_card_id,
            });
        }
    }
    for cast in &eternal.vintage.cast_mana {
        if let Some(spell) = current_public(state, cast.spell, Zone::Stack)? {
            out.cast_mana.push(CastManaV1 {
                spell,
                total: cast.total,
                colored: cast.colored,
            });
        }
    }
    for &(link, highest_gained_chapter) in &eternal.vintage.saga_abilities {
        if let Some(source) = current_public(state, link, Zone::Battlefield)? {
            out.saga_abilities.push(SagaAbilitiesV1 {
                source,
                highest_gained_chapter,
            });
        }
    }
    Ok(out)
}

#[cfg(all(test, feature = "eternal-fixtures"))]
mod tests {
    use super::*;
    use crate::{
        ids::PlayerId,
        policy_observation_v6::tests::{put, ready_state_for},
    };

    #[test]
    fn issue454_entry_handlers_publish_choices_and_concealed_sources_refuse_uniformly() {
        for player in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(player);
            let cavern = put(&mut state, player, "Cavern of Souls", Zone::Battlefield);
            put(&mut state, player, "Goblin Boarders", Zone::Hand);
            let options = crate::eternal_cards_v1::e3::cavern_type_options(&state, player);
            let selected = options.iter().position(|t| *t == Subtype::Goblin).unwrap();
            crate::eternal_cards_v1::e3::answer_entry_choice(&mut state, selected as u16).unwrap();
            let needle = put(&mut state, player, "Pithing Needle", Zone::Battlefield);
            let options = crate::eternal_lurrus_v1::needle_name_options(&state, player);
            let named = crate::card_def::card_id_by_name("Cavern of Souls").unwrap();
            let selected = options.iter().position(|id| *id == named).unwrap();
            assert!(
                crate::eternal_lurrus_v1::answer_entry_choice(&mut state, selected as u16).unwrap()
            );
            let public = capture_v1(&state).unwrap();
            assert_eq!(public.cavern_types[0].chosen_type, Subtype::Goblin);
            assert_eq!(public.needle_names[0].named_card_id, named);
            // No inspection of the concealed source's underlying printing may
            // decide whether this incomplete continuity contract accepts it.
            for source in [cavern, needle] {
                for hidden_name in ["Island", "Forest", "Pithing Needle"] {
                    let mut concealed = state.clone();
                    let object = concealed.objects.get_mut(source);
                    object.v4.face_down_v1 = Some(crate::state::FaceDownV1 {
                        disguised: true,
                        lookers: 0,
                        hidden_by: None,
                        hidden_by_source: None,
                    });
                    object.card_def = crate::card_def::card_id_by_name(hidden_name).unwrap();
                    assert_eq!(
                        capture_v1(&concealed).unwrap_err().0,
                        "concealed Eternal choice source requires public continuity"
                    );
                }
            }
        }
    }

    #[test]
    fn issue454_public_choices_distinguish_names_and_types_without_hidden_definitions() {
        for player in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(player);
            let cavern = put(&mut state, player, "Cavern of Souls", Zone::Battlefield);
            let needle = put(&mut state, player, "Pithing Needle", Zone::Battlefield);
            let hidden = put(&mut state, player.opponent(), "Island", Zone::Hand);
            let named = crate::card_def::card_id_by_name("Forest").unwrap();
            let link = |object| ObjectLinkV4 {
                object,
                zone_change_count: 0,
            };
            let eternal = state.eternal_v1.get_or_insert_with(Default::default);
            eternal
                .e3
                .get_or_insert_with(Default::default)
                .cavern_types
                .push((link(cavern), Subtype::Goblin));
            eternal.lurrus.needle_names.push((link(needle), named));
            let baseline = capture_v1(&state).unwrap();
            assert_eq!(baseline.cavern_types[0].source.arena_id, cavern.0);
            assert_eq!(baseline.needle_names[0].named_card_id, named);
            let mut hidden_variant = state.clone();
            hidden_variant.objects.get_mut(hidden).card_def = named;
            assert_eq!(capture_v1(&hidden_variant).unwrap(), baseline);
            // A public named printing does not assert that a hidden copy exists.
            let mut changed = state.clone();
            changed.eternal_v1.as_mut().unwrap().lurrus.needle_names[0].1 =
                crate::card_def::card_id_by_name("Island").unwrap();
            assert_ne!(capture_v1(&changed).unwrap(), baseline);
            changed = state.clone();
            changed
                .eternal_v1
                .as_mut()
                .unwrap()
                .e3
                .as_mut()
                .unwrap()
                .cavern_types[0]
                .1 = Subtype::Human;
            assert_ne!(capture_v1(&changed).unwrap(), baseline);
            // Old choices never acquire the identity of a hidden new incarnation.
            for source in [cavern, needle] {
                state.objects.get_mut(source).zone = Zone::Hand;
                state.objects.get_mut(source).zone_change_count += 1;
            }
            let departed = capture_v1(&state).unwrap();
            assert!(departed.cavern_types.is_empty() && departed.needle_names.is_empty());
            state.objects.get_mut(cavern).card_def = named;
            state.objects.get_mut(needle).card_def = named;
            assert_eq!(capture_v1(&state).unwrap(), departed);
        }
    }
}
