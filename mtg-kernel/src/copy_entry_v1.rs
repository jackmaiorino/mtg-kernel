//! Copiable characteristics frozen before a simultaneous entry batch (CR 614.12, 707.2).
use crate::card_def::{card_id_by_name, Subtype};
use crate::eternal_cube_c8b_v1::{current_copy_mods, read_state, C8bCopyModsV1};
use crate::event;
use crate::ids::{ObjectId, PlayerId};
use crate::state::{AbilitySourceContractV4, GameState, ObjectLinkV4};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Only these native resolution paths own a noncast copy menu in this phase.
/// Selection programs supply their authenticated hand answer. Ordinary resolving
/// effects are intercepted at the zone proposal, including simultaneous batches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Route {
    ResolvingSpell {
        from: crate::state::Zone,
        targeted: bool,
    },
    ShowAndTell,
    PaidHandAbility,
}

pub(crate) fn registered_route(definition: u16) -> Option<Route> {
    if !cfg!(feature = "eternal-fixtures") {
        return None;
    }
    match crate::card_def::CARD_DEFS
        .get(usize::from(definition))?
        .name
    {
        "Reanimate" => Some(Route::ResolvingSpell {
            from: crate::state::Zone::Graveyard,
            targeted: true,
        }),
        "Show and Tell" => Some(Route::ShowAndTell),
        "Aether Vial" => Some(Route::PaidHandAbility),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CopiableValuesV1 {
    pub(crate) card_def: u16,
    pub(crate) face_index: u8,
    pub(crate) mods: C8bCopyModsV1,
    pub(crate) copied_end_step_sacrifices_v1: u32,
    pub(crate) copied_nightmare_v1: bool,
    pub(crate) prototyped: bool,
}

/// Reads the CR 707.2 copiable values of a battlefield permanent `object`,
/// propagating any existing C8b copy modifications, Phyrexian Metamorph's
/// added artifact type, and face-down anonymous blueprint.
pub(crate) fn capture_battlefield_copiable_values(
    state: &GameState,
    object: ObjectId,
) -> Option<CopiableValuesV1> {
    let live = state.objects.try_get(object)?;
    if live.v4.face_down_v1.is_some() {
        let anonymous = card_id_by_name(crate::self_reflection_v1::ANONYMOUS_BLUEPRINT)
            .filter(|id| crate::self_reflection_v1::anonymous_blueprint_is_exact(*id))?;
        return Some(CopiableValuesV1 {
            card_def: anonymous,
            face_index: 0,
            mods: C8bCopyModsV1::default(),
            copied_end_step_sacrifices_v1: 0,
            copied_nightmare_v1: false,
            prototyped: false,
        });
    }
    let link = ObjectLinkV4 {
        object,
        zone_change_count: live.zone_change_count,
    };
    let has_saheeli = read_state(state)
        .is_some_and(|c8b| c8b.saheeli_copies.iter().any(|copy| copy.host == link));
    let mut mods = if !has_saheeli && crate::eternal_cube_c8c_v1::is_absorbing_copy(state, object) {
        let mut absorbing = C8bCopyModsV1 {
            added_artifact: crate::eternal_cube_c8c_v1::absorbing_copy_adds_artifact(state, object),
            override_name: Some(crate::eternal_cube_c8c_v1::ABSORBING_MAN.to_string()),
            base_pt: Some((4, 4)),
            added_creature: true,
            added_legendary: true,
            added_vigilance: true,
            ..Default::default()
        };
        absorbing.merge_subtypes(&[Subtype::Human, Subtype::Villain]);
        absorbing
    } else {
        let mut current = current_copy_mods(state, link).unwrap_or_default();
        if crate::eternal_vintage_v1::active_copy(state, object) {
            current.added_artifact = true;
        }
        current
    };
    if has_saheeli && crate::eternal_vintage_v1::active_copy(state, object) {
        mods.added_artifact = true;
    }
    let copied_end_step_sacrifices_v1 =
        event::copiable_token_abilities_v1(state, object).unwrap_or(0);
    let copied_nightmare_v1 =
        crate::effect::harvester_v1::copiable_nightmare(state, object).unwrap_or(false);
    let prototyped = crate::eternal_pauper_pf2c_v1::is_prototyped_permanent(state, object);
    Some(CopiableValuesV1 {
        card_def: live.card_def,
        face_index: live.v4.face_index,
        mods,
        copied_end_step_sacrifices_v1,
        copied_nightmare_v1,
        prototyped,
    })
}

pub(crate) type CandidateValues = BTreeMap<ObjectId, CopiableValuesV1>;

pub(crate) fn capture_menu(
    state: &GameState,
    entrant: ObjectId,
    controller: PlayerId,
) -> Result<(Vec<AbilitySourceContractV4>, CandidateValues), String> {
    let mut contracts = Vec::new();
    let mut values = CandidateValues::new();
    if crate::eternal_legacy_e7a_v1::active_dress_down_timestamp(state, Some(entrant)).is_none() {
        for candidate in crate::effect::metamorph_copy_candidates(state, controller)? {
            let object = candidate.object;
            if state.objects.get(object).v4.face_down_v1.is_some() {
                let mut contract = AbilitySourceContractV4::capture(state, object);
                // CR 708.2: the public contract must not disclose the face-down printing.
                contract.card_def =
                    crate::rl::projected_card_def(state, object, contract.zone_change_count);
                contracts.push(contract);
                continue;
            }
            // Freeze every value observable on this incarnation (CR 707.2).
            // Older Saheeli and Phantasmal Image records omit inherited sacrifices
            // and nightmare. Their original donor cannot be recovered here.
            let captured = capture_battlefield_copiable_values(state, object)
                .ok_or("copy candidate lost its copiable characteristics")?;
            let mut contract = AbilitySourceContractV4::capture(state, object);
            contract.card_def = captured.card_def;
            contracts.push(contract);
            if captured.face_index != 0
                || captured.mods != C8bCopyModsV1::default()
                || captured.prototyped
                || captured.copied_end_step_sacrifices_v1 != 0
                || captured.copied_nightmare_v1
            {
                values.insert(object, captured);
            }
        }
    }
    Ok((contracts, values))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicMenuV1 {
    pub player: PlayerId,
    pub step: String,
    pub can_decline: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<EntryAnswer>,
    pub candidates: Vec<AbilitySourceContractV4>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub values: CandidateValues,
}

/// The choice itself contains the public menu. Never inspect the original world or
/// the players' prior Show and Tell hand answers to build this projection.
pub(crate) fn public_menu(state: &GameState, viewer: PlayerId) -> Option<PublicMenuV1> {
    use crate::linked_aura_operation_v1::GeneralChoiceV36 as Choice;
    let checkpoint = state
        .engine
        .linked_aura_operation_v1
        .pending_meddling_mage_v1
        .as_ref()?;
    if let Choice::CopyEntryOptionV1 { choice, .. } = &checkpoint.choice {
        let (step, can_decline) = match choice.options.first()? {
            EntryAnswer::Color(_) => ("color", false),
            EntryAnswer::ProtectionType(_) => ("card_type", false),
            EntryAnswer::RevokerName(_) | EntryAnswer::NeedleName(_) => ("name", false),
            EntryAnswer::CreatureType(_) => ("creature_type", false),
            EntryAnswer::Discard(_) => ("discard", true),
        };
        return Some(PublicMenuV1 {
            player: choice.player,
            step: step.into(),
            can_decline,
            candidates: Vec::new(),
            values: Default::default(),
            options: if choice.player == viewer {
                choice.options.clone()
            } else {
                Vec::new()
            },
        });
    }
    let (player, candidates, values) = match &checkpoint.choice {
        Choice::ReanimateMetamorphV1 { choice, .. } | Choice::VialMetamorphV1 { choice, .. } => (
            choice.producer.controller,
            &choice.candidates,
            &choice.values,
        ),
        Choice::ShowAndTellV1 { choice, .. } if choice.physical.is_some() => {
            (choice.player, &choice.candidates, &choice.values)
        }
        _ => return None,
    };
    Some(PublicMenuV1 {
        player,
        step: "copy".into(),
        can_decline: true,
        options: Vec::new(),
        candidates: candidates.clone(),
        values: values.clone(),
    })
}

pub(crate) fn flat_requires_successor(state: &GameState) -> bool {
    use crate::eternal_sideboard_s1b_v1::meddling_mage_entry::{self as mage, JournalV1};
    use crate::linked_aura_operation_v1::GeneralChoiceV36;
    if state.engine.linked_aura_operation_v1.pending_meddling_mage_v1.as_ref().is_some_and(
        |p| !matches!(&p.choice, GeneralChoiceV36::ShowAndTellV1 { choice, .. } if choice.physical.is_none())) {
        return true;
    }
    state.engine.event_history.of_kinds_v2(&[crate::event_history_v1::EventKindV1::MeddlingMageEntryV1])
        .any(|(_, event)| matches!(event, crate::event::CommittedEvent::MeddlingMageEntryV1 { journal }
            if match journal.as_ref() {
                JournalV1::MetamorphCopied { copy, .. } | JournalV1::ShowAndTellMetamorphChosen { copy } => mage::owns_definition(copy.copied.card_def),
                JournalV1::VialMetamorphChosen { .. } | JournalV1::NoncastCopyPrepared { .. } => false,
                _ => true,
            }))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EntryAnswer {
    Color(crate::mana::ManaColor),
    ProtectionType(u8),
    NeedleName(u16),
    RevokerName(u16),
    CreatureType(u16),
    Discard(Option<AbilitySourceContractV4>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Requirement {
    pub(crate) physical: AbilitySourceContractV4,
    pub(crate) copied: AbilitySourceContractV4,
    pub(crate) player: PlayerId,
    pub(crate) options: Vec<EntryAnswer>,
}

pub(crate) fn requirement(
    state: &GameState,
    player: PlayerId,
    copy: &crate::eternal_sideboard_s1b_v1::meddling_mage_entry::MetamorphCopyV1,
) -> Result<Option<Requirement>, String> {
    use crate::card_def::CARD_DEFS;
    let contract = copy.copied;
    let live = state.objects.get(contract.source);
    if live.v4.face_down_v1.is_some() {
        return Err("Eternal has no nameless face-up blueprint for a face-down copy".into());
    }
    let name = CARD_DEFS[usize::from(contract.card_def)].object_name;
    if copy
        .values
        .as_ref()
        .is_some_and(|values| values.face_index != 0)
    {
        return Ok(None);
    }
    let options = match name {
        "Painter's Servant" => crate::eternal_legacy_e9a_v1::statics::COLORS
            .into_iter()
            .map(EntryAnswer::Color)
            .collect(),
        "Serra's Emissary" => (0..crate::eternal_e5a_v1::statics::PROTECTION_CHOICE_COUNT as u8)
            .map(EntryAnswer::ProtectionType)
            .collect(),
        "Pithing Needle" => crate::eternal_lurrus_v1::needle_name_options(state, player)
            .into_iter()
            .map(EntryAnswer::NeedleName)
            .collect(),
        "Phyrexian Revoker" => {
            crate::eternal_vintage_e7b_v1::statics::revoker_name_options(state, player)
                .into_iter()
                .map(EntryAnswer::RevokerName)
                .collect()
        }
        "Cavern of Souls" => crate::eternal_cards_v1::e3::cavern_type_options(state, player)
            .into_iter()
            .map(|ty| EntryAnswer::CreatureType(ty.stable_id()))
            .collect(),
        "Mox Diamond" => std::iter::once(EntryAnswer::Discard(None))
            .chain(
                crate::eternal_legacy_e7a_v1::hand_land_cards(state, player)
                    .into_iter()
                    .map(|land| {
                        EntryAnswer::Discard(Some(AbilitySourceContractV4::capture(state, land)))
                    }),
            )
            .collect(),
        "Anointed Peacekeeper"
        | "Disruptor Flute"
        | "Heraldic Banner"
        | "Banner of Kinship"
        | "Phantasmal Image"
        | "Superior Spider-Man"
        | "Phyrexian Metamorph"
        | "Valgavoth's Lair"
        | "Alpine Moon"
        | "Echoing Deeps"
        | "Secluded Courtyard"
        | "Engineered Plague"
        | "Phyrexian Processor"
        | "Shared Triumph" => {
            return Err("chosen copy requires an unsupported entry replacement".into())
        }
        other if crate::eternal_cube_c2a_v1::as_enters_life_cost(other).is_some() => {
            return Err("copy requires an entry life payment".into())
        }
        _ => return Ok(None),
    };
    Ok(Some(Requirement {
        physical: copy.physical,
        copied: contract,
        player,
        options,
    }))
}

pub(crate) fn install_answer(state: &mut GameState, host: ObjectLinkV4, answer: EntryAnswer) {
    match answer {
        EntryAnswer::Color(color) => {
            state.objects.get_mut(host.object).v4.chosen_color = Some(color)
        }
        EntryAnswer::ProtectionType(index) => crate::eternal_e5a_v1::e5a_state(state)
            .emissary_types
            .push((host, index)),
        EntryAnswer::NeedleName(named) => crate::eternal_lurrus_v1::lurrus_state(state)
            .needle_names
            .push((host, named)),
        EntryAnswer::RevokerName(named) => crate::eternal_vintage_e7b_v1::e7b_state(state)
            .revoker_names
            .push((host, named)),
        EntryAnswer::CreatureType(id) => crate::eternal_cards_v1::e3::e3_mut(state)
            .cavern_types
            .push((
                host,
                Subtype::from_stable_id(id).expect("validated creature type"),
            )),
        EntryAnswer::Discard(_) => {}
    }
}

/// CR 701.28: a single-faced physical card cannot transform. Keep the
/// battlefield face copied at entry when the day/night designation changes.
pub(crate) fn is_noncast_copy(state: &GameState, object: ObjectId) -> bool {
    if !crate::eternal_vintage_v1::active_copy(state, object) {
        return false;
    }
    let generation = state.objects.get(object).zone_change_count;
    state.engine.event_history.of_kinds_v2(&[crate::event_history_v1::EventKindV1::MeddlingMageEntryV1])
        .any(|(_, e)| matches!(e, crate::event::CommittedEvent::MeddlingMageEntryV1 { journal }
            if matches!(journal.as_ref(), crate::eternal_sideboard_s1b_v1::meddling_mage_entry::JournalV1::MetamorphCopied {entrant,copy,..}
                if entrant.source == object && entrant.zone_change_count == generation && copy.physical.zone != crate::state::Zone::Stack)))
}

/// Recover non-token inherited copy abilities from the committed entry. The regular
/// token receipt continues to authenticate actual token creations.
pub(crate) fn inherited_values(
    state: &GameState,
    source: AbilitySourceContractV4,
    before: usize,
) -> Option<&CopiableValuesV1> {
    use crate::eternal_sideboard_s1b_v1::meddling_mage_entry::JournalV1;
    if !cfg!(feature = "eternal-fixtures") {
        return None;
    }
    state
        .engine
        .event_history
        .of_kinds_v2(&[crate::event_history_v1::EventKindV1::MeddlingMageEntryV1])
        .take_while(|(at, _)| *at < before)
        .find_map(|(at, event)| {
            let crate::event::CommittedEvent::MeddlingMageEntryV1 { journal } = event else {
                return None;
            };
            let JournalV1::MetamorphCopied {
                entrant,
                copy,
                entry_history_index,
            } = journal.as_ref()
            else {
                return None;
            };
            (entry_history_index.checked_add(1) == Some(at)
                && entrant.source == source.source
                && entrant.owner == source.owner
                && entrant.zone_change_count == source.zone_change_count
                && entrant.card_def == source.card_def
                && copy.physical.zone != crate::state::Zone::Stack)
                .then_some(copy.values.as_ref())
                .flatten()
        })
}

pub(crate) fn installed_values_match(
    state: &GameState,
    entrant: AbilitySourceContractV4,
    copy: &crate::eternal_sideboard_s1b_v1::meddling_mage_entry::MetamorphCopyV1,
) -> bool {
    let Some(live) = state.objects.try_get(entrant.source) else {
        return false;
    };
    if live.zone_change_count != entrant.zone_change_count
        || live.zone != crate::state::Zone::Battlefield
        || crate::eternal_cube_c8b_v1::has_saheeli_copy(state, entrant.source)
        || crate::eternal_cube_c8c_v1::is_absorbing_copy(state, entrant.source)
        || crate::eternal_legacy_e7a_v1::active_copy(state, entrant.source)
    {
        return true;
    }
    let host = ObjectLinkV4 {
        object: entrant.source,
        zone_change_count: entrant.zone_change_count,
    };
    let values = copy.values.as_ref();
    let mods = current_copy_mods(state, host);
    let expected_mods = values
        .map(|values| &values.mods)
        .filter(|mods| !mods.is_empty());
    let installed_answer = match copy.entry_answer {
        None | Some(EntryAnswer::Discard(Some(_))) => true,
        Some(EntryAnswer::Discard(None)) => false,
        Some(EntryAnswer::Color(color)) => live.v4.chosen_color == Some(color),
        Some(EntryAnswer::ProtectionType(index)) => state
            .eternal_v1
            .as_ref()
            .is_some_and(|et| et.e5a.emissary_types.contains(&(host, index))),
        Some(EntryAnswer::NeedleName(name)) => state
            .eternal_v1
            .as_ref()
            .is_some_and(|et| et.lurrus.needle_names.contains(&(host, name))),
        Some(EntryAnswer::RevokerName(name)) => state
            .eternal_v1
            .as_ref()
            .is_some_and(|et| et.e7b.revoker_names.contains(&(host, name))),
        Some(EntryAnswer::CreatureType(id)) => state
            .eternal_v1
            .as_ref()
            .and_then(|et| et.e3.as_ref())
            .is_some_and(|e3| {
                e3.cavern_types
                    .iter()
                    .any(|(source, ty)| *source == host && ty.stable_id() == id)
            }),
    };
    installed_answer
        && live.card_def == entrant.card_def
        && mods.as_ref() == expected_mods
        && live.v4.face_index == values.map_or(0, |values| values.face_index)
        && live.v4.copied_nightmare_v1 == values.is_some_and(|values| values.copied_nightmare_v1)
        && live.v4.copied_end_step_sacrifices_v1
            == values.map_or(0, |values| values.copied_end_step_sacrifices_v1)
        && crate::eternal_pauper_pf2c_v1::is_prototyped_permanent(state, entrant.source)
            == values.is_some_and(|values| values.prototyped)
}
