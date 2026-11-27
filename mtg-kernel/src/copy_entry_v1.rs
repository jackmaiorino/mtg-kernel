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
        under_owner: bool,
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
            under_owner: false,
        }),
        "Show and Tell" => Some(Route::ShowAndTell),
        "Aether Vial" => Some(Route::PaidHandAbility),
        _ => None,
    }
}

pub(crate) fn entering_controller(
    producer: AbilitySourceContractV4,
    physical: AbilitySourceContractV4,
) -> PlayerId {
    match registered_route(producer.card_def) {
        Some(Route::ResolvingSpell {
            under_owner: true, ..
        }) => physical.owner,
        _ => producer.controller,
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

/// History-only provenance receipts emitted exclusively in Eternal builds
/// (`cfg!(feature = "eternal-fixtures")`) to bind completed noncast copy
/// validation to exact entry-time animations, Saheeli host applications,
/// donor copy-mod lineage, and Mox Diamond copy-discard payments.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ProvenanceV1 {
    LandAnimated {
        host: ObjectLinkV4,
        card_def: u16,
        until_end_of_turn: bool,
    },
    SaheeliApplied {
        source: AbilitySourceContractV4,
        stack_item: crate::ids::StackItemId,
        host: ObjectLinkV4,
        model: ObjectLinkV4,
        values: CopiableValuesV1,
    },
    EntryCopyApplied {
        host: ObjectLinkV4,
        donor: ObjectLinkV4,
        values: CopiableValuesV1,
    },
    AbsorbingCopyApplied {
        host: ObjectLinkV4,
        donor: ObjectLinkV4,
        until_turn_of: PlayerId,
        card_def: u16,
        face_index: u8,
        added_artifact: bool,
    },
    TokenCopyCreated {
        token: ObjectLinkV4,
        create_at: usize,
        producer: AbilitySourceContractV4,
        donor: ObjectLinkV4,
        values: CopiableValuesV1,
    },
    CopyDiscardPaid {
        stack_item: crate::ids::StackItemId,
        physical: AbilitySourceContractV4,
        land: AbilitySourceContractV4,
        discard_at: usize,
    },
}

fn push_provenance(state: &mut GameState, journal: ProvenanceV1) {
    if !cfg!(feature = "eternal-fixtures") {
        return;
    }
    state
        .engine
        .event_history
        .push(event::CommittedEvent::CopyEntryProvenanceV1 {
            journal: Box::new(journal),
        });
}

#[cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]
pub(crate) fn record_land_animated(
    state: &mut GameState,
    host: ObjectLinkV4,
    card_def: u16,
    until_end_of_turn: bool,
) {
    push_provenance(
        state,
        ProvenanceV1::LandAnimated {
            host,
            card_def,
            until_end_of_turn,
        },
    );
}

#[cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]
pub(crate) fn record_saheeli_applied(
    state: &mut GameState,
    source: AbilitySourceContractV4,
    stack_item: crate::ids::StackItemId,
    host: ObjectLinkV4,
    model: ObjectLinkV4,
    values: CopiableValuesV1,
) {
    push_provenance(
        state,
        ProvenanceV1::SaheeliApplied {
            source,
            stack_item,
            host,
            model,
            values,
        },
    );
}

#[cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]
pub(crate) fn record_entry_copy_applied(
    state: &mut GameState,
    host: ObjectLinkV4,
    donor: ObjectLinkV4,
    values: CopiableValuesV1,
) {
    push_provenance(
        state,
        ProvenanceV1::EntryCopyApplied {
            host,
            donor,
            values,
        },
    );
}

#[cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]
pub(crate) fn record_absorbing_copy_applied(
    state: &mut GameState,
    host: ObjectLinkV4,
    donor: ObjectLinkV4,
    until_turn_of: PlayerId,
    card_def: u16,
    face_index: u8,
    added_artifact: bool,
) {
    push_provenance(
        state,
        ProvenanceV1::AbsorbingCopyApplied {
            host,
            donor,
            until_turn_of,
            card_def,
            face_index,
            added_artifact,
        },
    );
}

#[cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]
pub(crate) fn record_token_copy_created(
    state: &mut GameState,
    token: ObjectLinkV4,
    create_at: usize,
    producer: AbilitySourceContractV4,
    donor: ObjectLinkV4,
    values: CopiableValuesV1,
) {
    push_provenance(
        state,
        ProvenanceV1::TokenCopyCreated {
            token,
            create_at,
            producer,
            donor,
            values,
        },
    );
}

#[cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]
pub(crate) fn record_copy_discard_paid(
    state: &mut GameState,
    stack_item: crate::ids::StackItemId,
    physical: AbilitySourceContractV4,
    land: AbilitySourceContractV4,
    discard_at: usize,
) {
    push_provenance(
        state,
        ProvenanceV1::CopyDiscardPaid {
            stack_item,
            physical,
            land,
            discard_at,
        },
    );
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicStepV1 {
    Copy,
    Color,
    CardType,
    Name,
    Discard,
    CreatureType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicMenuV1 {
    pub player: PlayerId,
    pub step: PublicStepV1,
    /// Printed names in menu order, as on the schema-5 Meddling Mage name menu.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub option_text: Vec<String>,
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
            EntryAnswer::Color(_) => (PublicStepV1::Color, false),
            EntryAnswer::ProtectionType(_) => (PublicStepV1::CardType, false),
            EntryAnswer::RevokerName(_) | EntryAnswer::NeedleName(_) => (PublicStepV1::Name, false),
            EntryAnswer::CreatureType(_) => (PublicStepV1::CreatureType, false),
            EntryAnswer::Discard(_) => (PublicStepV1::Discard, true),
        };
        return Some(PublicMenuV1 {
            player: choice.player,
            step,
            option_text: if viewer == choice.player && step == PublicStepV1::Name {
                choice
                    .options
                    .iter()
                    .filter_map(|option| match option {
                        EntryAnswer::NeedleName(id) | EntryAnswer::RevokerName(id) => {
                            crate::card_def::CARD_DEFS
                                .get(usize::from(*id))
                                .map(|def| def.object_name.to_owned())
                        }
                        _ => None,
                    })
                    .collect()
            } else {
                Vec::new()
            },
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
        Choice::ReanimateMetamorphV1 { choice, .. } | Choice::VialMetamorphV1 { choice, .. } => {
            (choice.chooser(), &choice.candidates, &choice.values)
        }
        Choice::ShowAndTellV1 { choice, .. } if choice.physical.is_some() => {
            (choice.player, &choice.candidates, &choice.values)
        }
        _ => return None,
    };
    Some(PublicMenuV1 {
        player,
        step: PublicStepV1::Copy,
        option_text: Vec::new(),
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

fn unsupported_copy_face(
    def: &crate::card_def::CardDef,
    face_index: u8,
    face_name: &str,
) -> Option<&'static str> {
    if matches!(
        face_name,
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
            | "Shared Triumph"
    ) {
        return Some("chosen copy requires an unsupported entry replacement");
    }
    if crate::eternal_cube_c2a_v1::as_enters_life_cost(face_name).is_some()
        || (face_index == 1 && crate::eternal_cards_v1::e3::has_land_back_face(def.name))
    {
        return Some("copy requires an entry life payment");
    }
    None
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
    let definition = &CARD_DEFS[usize::from(contract.card_def)];
    let face_index = copy.values.as_ref().map_or(0, |values| values.face_index);
    let name = if face_index != 0 {
        definition
            .transform_face
            .as_ref()
            .ok_or("copy lost its battlefield face")?
            .name
    } else {
        definition.object_name
    };
    if let Some(reason) = unsupported_copy_face(definition, face_index, name) {
        return Err(reason.into());
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

/// CR 701.28a: a single-faced physical card cannot transform in place,
/// including when its copied ability or the day/night designation directs it.
pub(crate) fn is_noncast_copy(state: &GameState, object: ObjectId) -> bool {
    if !cfg!(feature = "eternal-fixtures") || !crate::eternal_vintage_v1::active_copy(state, object)
    {
        return false;
    }
    let source = AbilitySourceContractV4::capture(state, object);
    noncast_entry(state, source, state.engine.event_history.len()).is_some()
}

/// CR 701.28a: a single-faced physical card copying a double-faced permanent
/// cannot transform in place (covers noncast Metamorph copies as well as cast
/// Metamorph, Phantasmal Image, and Superior Spider-Man copies).
pub(crate) fn cannot_transform_in_place(state: &GameState, object: ObjectId) -> bool {
    if is_noncast_copy(state, object) {
        return true;
    }
    if !cfg!(feature = "eternal-fixtures") {
        return false;
    }
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    if live.zone != crate::state::Zone::Battlefield {
        return false;
    }
    let link = ObjectLinkV4 {
        object,
        zone_change_count: live.zone_change_count,
    };
    crate::standard_cards_v1::archived_copy_printing(state, link, live.card_def).is_some_and(
        |physical| {
            physical != live.card_def
                && crate::card_def::CARD_DEFS[usize::from(physical)]
                    .transform_face
                    .is_none()
        },
    )
}

fn zone_and_zcc_from(
    state: &GameState,
    object: ObjectId,
    start: usize,
) -> Option<(crate::state::Zone, u32)> {
    let live = state.objects.try_get(object)?;
    let after = state.engine.event_history.get(start..)?;
    if after.iter().any(|ev| {
        matches!(ev, event::CommittedEvent::CreateToken { object: created, .. } if *created == object)
    }) {
        return None;
    }
    let mut first_from = None;
    let mut changes: u32 = 0;
    for ev in after {
        if let event::CommittedEvent::ZoneChange {
            object: moved,
            from,
            ..
        } = ev
        {
            if *moved == object {
                if first_from.is_none() {
                    first_from = Some(*from);
                }
                changes = changes.checked_add(1)?;
            }
        }
    }
    let zcc_at = live.zone_change_count.checked_sub(changes)?;
    Some((first_from.unwrap_or(live.zone), zcc_at))
}

fn zone_and_zcc_at(
    state: &GameState,
    object: ObjectId,
    at: usize,
) -> Option<(crate::state::Zone, u32)> {
    zone_and_zcc_from(state, object, at.checked_add(1)?)
}

fn zone_and_zcc_before(
    state: &GameState,
    object: ObjectId,
    at: usize,
) -> Option<(crate::state::Zone, u32)> {
    zone_and_zcc_from(state, object, at)
}

fn object_was_named_on_battlefield_at(
    state: &GameState,
    object: ObjectId,
    name: &str,
    at: usize,
) -> bool {
    let Some(target_id) = card_id_by_name(name) else {
        return false;
    };
    let Some((crate::state::Zone::Battlefield, zcc_at)) = zone_and_zcc_at(state, object, at) else {
        return false;
    };
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    live.card_def == target_id
        || crate::standard_cards_v1::had_card_def(state, object, zcc_at, target_id)
}

fn any_permanent_named_on_battlefield_at(
    state: &GameState,
    name: &str,
    exclude: Option<ObjectId>,
    at: usize,
) -> bool {
    let Some(target_id) = card_id_by_name(name) else {
        return false;
    };
    state.objects.iter().any(|(id, live)| {
        if Some(id) == exclude {
            return false;
        }
        let Some((crate::state::Zone::Battlefield, zcc_at)) = zone_and_zcc_at(state, id, at) else {
            return false;
        };
        live.card_def == target_id
            || crate::standard_cards_v1::had_card_def(state, id, zcc_at, target_id)
    })
}

fn has_cleanup_boundary_after(state: &GameState, entry: usize) -> bool {
    state.engine.event_history[entry + 1..].iter().any(|ev| {
        matches!(
            ev,
            event::CommittedEvent::BeginningEndStep { .. }
                | event::CommittedEvent::UpkeepBegan { .. }
        )
    })
}

fn has_upkeep_between(state: &GameState, start: usize, end: usize) -> bool {
    start < end
        && state.engine.event_history[start..end]
            .iter()
            .any(|ev| matches!(ev, event::CommittedEvent::UpkeepBegan { .. }))
}

fn has_player_upkeep_between(
    state: &GameState,
    player: PlayerId,
    start: usize,
    end: usize,
) -> bool {
    start < end
        && state.engine.event_history[start..end].iter().any(
            |ev| matches!(ev, event::CommittedEvent::UpkeepBegan { player: p } if *p == player),
        )
}

/// Reconstructs the donor's exact `face_index` as of `entry` by inverting each
/// committed in-place `Transformed` event for the same battlefield incarnation
/// after `entry` (CR 707.2, 712.8).
fn reconstruct_face_at_entry(
    state: &GameState,
    donor: ObjectId,
    current_face: u8,
    entry: usize,
) -> Option<u8> {
    if current_face > 1 {
        return None;
    }
    let mut face = current_face;
    for ev in state.engine.event_history[entry + 1..].iter().rev() {
        if let event::CommittedEvent::Transformed { object, face_index } = ev {
            if *object == donor {
                if *face_index != face || *face_index > 1 {
                    return None;
                }
                face = 1 - face;
            }
        }
    }
    Some(face)
}

fn face_after_transforms_between(
    state: &GameState,
    donor: ObjectId,
    start_face: u8,
    start_exclusive: usize,
    end_exclusive: usize,
) -> Option<u8> {
    if start_face > 1 || start_exclusive > end_exclusive {
        return None;
    }
    let mut face = start_face;
    for ev in &state.engine.event_history[start_exclusive..end_exclusive] {
        if let event::CommittedEvent::Transformed { object, face_index } = ev {
            if *object == donor {
                if *face_index > 1 || *face_index == face {
                    return None;
                }
                face = *face_index;
            }
        }
    }
    Some(face)
}

fn validate_saheeli_provenance(
    state: &GameState,
    saheeli_at: usize,
    source: AbilitySourceContractV4,
    stack_item: crate::ids::StackItemId,
    host: ObjectLinkV4,
    model: ObjectLinkV4,
    values: &CopiableValuesV1,
) -> bool {
    if host.object == model.object
        || !values.mods.added_artifact
        || card_id_by_name(crate::eternal_cube_c8b_v1::SAHEELI) != Some(source.card_def)
        || zone_and_zcc_at(state, host.object, saheeli_at)
            != Some((crate::state::Zone::Battlefield, host.zone_change_count))
        || zone_and_zcc_at(state, model.object, saheeli_at)
            != Some((crate::state::Zone::Battlefield, model.zone_change_count))
        || !crate::standard_cards_v1::had_card_def(
            state,
            host.object,
            host.zone_change_count,
            values.card_def,
        )
    {
        return false;
    }
    // Bind slot 0 = host and slot 1 = model via the exact slot-ordered
    // `Targeted` pair committed when this Saheeli activation announced/retargeted.
    let targets: Vec<(usize, ObjectId, u32)> = state.engine.event_history[..saheeli_at]
        .iter()
        .enumerate()
        .filter_map(|(at, ev)| match ev {
            event::CommittedEvent::Targeted {
                target,
                target_zone_change_count,
                targeting_stack_item,
                targeting_controller,
            } if *targeting_stack_item == stack_item
                && *targeting_controller == source.controller =>
            {
                Some((at, *target, *target_zone_change_count))
            }
            _ => None,
        })
        .collect();
    if targets.len() < 2 || !targets.len().is_multiple_of(2) {
        return false;
    }
    let slot0 = targets[targets.len() - 2];
    let slot1 = targets[targets.len() - 1];
    if slot0.1 != host.object
        || slot0.2 != host.zone_change_count
        || slot1.1 != model.object
        || slot1.2 != model.zone_change_count
        || slot0.0.checked_add(1) != Some(slot1.0)
        || has_upkeep_between(state, slot0.0 + 1, saheeli_at)
        || !object_was_named_on_battlefield_at(
            state,
            source.source,
            crate::eternal_cube_c8b_v1::SAHEELI,
            slot0.0,
        )
    {
        return false;
    }
    let host_still_same = state.objects.try_get(host.object).is_some_and(|live| {
        live.zone == crate::state::Zone::Battlefield
            && live.zone_change_count == host.zone_change_count
    });
    if host_still_same
        && !has_cleanup_boundary_after(state, saheeli_at)
        && !crate::eternal_cube_c8b_v1::has_saheeli_copy(state, host.object)
    {
        return false;
    }
    true
}

/// Authenticates the latest Saheeli, Sublime Artificer `-2` resolution covering
/// exact slot-0 `host_link` as of `before` (same turn, no intervening upkeep).
/// Never accepts a post-entry record or a slot-1 model target.
fn saheeli_applied_at_entry(
    state: &GameState,
    host_link: ObjectLinkV4,
    before: usize,
) -> Option<(usize, &CopiableValuesV1)> {
    for (at, ev) in state.engine.event_history[..before]
        .iter()
        .enumerate()
        .rev()
    {
        if matches!(ev, event::CommittedEvent::UpkeepBegan { .. }) {
            break;
        }
        let event::CommittedEvent::CopyEntryProvenanceV1 { journal } = ev else {
            continue;
        };
        let ProvenanceV1::SaheeliApplied {
            source,
            stack_item,
            host,
            model,
            values,
        } = journal.as_ref()
        else {
            continue;
        };
        if *host != host_link {
            continue;
        }
        return validate_saheeli_provenance(state, at, *source, *stack_item, *host, *model, values)
            .then_some((at, values));
    }
    None
}

fn has_valid_post_entry_saheeli(state: &GameState, host_link: ObjectLinkV4, entry: usize) -> bool {
    for (offset, ev) in state.engine.event_history[entry + 1..]
        .iter()
        .enumerate()
        .rev()
    {
        if matches!(ev, event::CommittedEvent::UpkeepBegan { .. }) {
            break;
        }
        let event::CommittedEvent::CopyEntryProvenanceV1 { journal } = ev else {
            continue;
        };
        let ProvenanceV1::SaheeliApplied {
            source,
            stack_item,
            host,
            model,
            values,
        } = journal.as_ref()
        else {
            continue;
        };
        if *host != host_link {
            continue;
        }
        let at = entry + 1 + offset;
        return validate_saheeli_provenance(state, at, *source, *stack_item, *host, *model, values);
    }
    false
}

fn link_had_name(state: &GameState, link: ObjectLinkV4, name: &str) -> bool {
    let Some(target_id) = card_id_by_name(name) else {
        return false;
    };
    crate::standard_cards_v1::had_card_def(state, link.object, link.zone_change_count, target_id)
        || state
            .objects
            .try_get(link.object)
            .is_some_and(|live| live.card_def == target_id)
}

fn noncast_entry_link(
    state: &GameState,
    link: ObjectLinkV4,
    before: usize,
) -> Option<&crate::eternal_sideboard_s1b_v1::meddling_mage_entry::MetamorphCopyV1> {
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
                && entrant.source == link.object
                && entrant.zone_change_count == link.zone_change_count
                && copy.physical.zone != crate::state::Zone::Stack)
                .then_some(copy)
        })
}

fn absorbing_mods(added_artifact: bool) -> C8bCopyModsV1 {
    let mut mods = C8bCopyModsV1 {
        added_artifact,
        override_name: Some(crate::eternal_cube_c8c_v1::ABSORBING_MAN.to_string()),
        base_pt: Some((4, 4)),
        added_creature: true,
        added_legendary: true,
        added_vigilance: true,
        ..Default::default()
    };
    mods.merge_subtypes(&[Subtype::Human, Subtype::Villain]);
    mods
}

fn spider_man_mods() -> C8bCopyModsV1 {
    let mut mods = C8bCopyModsV1 {
        override_name: Some(crate::eternal_cube_c8b_v1::SUPERIOR_SPIDER_MAN.to_string()),
        base_pt: Some((4, 4)),
        ..Default::default()
    };
    mods.merge_subtypes(&[Subtype::Spider, Subtype::Human, Subtype::Hero]);
    mods
}

/// Computes the exact authenticated `C8bCopyModsV1` for `link` as of `before`
/// strictly from committed copy/token receipts and donor lineage, never from
/// global battlefield presence of unrelated cards.
fn authenticated_mods_for_link(
    state: &GameState,
    link: ObjectLinkV4,
    before: usize,
    depth: u8,
) -> Option<C8bCopyModsV1> {
    if depth > 8 {
        return None;
    }
    let live = state.objects.try_get(link.object)?;

    if let Some((_, applied)) = saheeli_applied_at_entry(state, link, before) {
        return Some(applied.mods.clone());
    }

    let history = &state.engine.event_history[..before];

    // Latest active Absorbing Man copy on `link` before `before`.
    for (at, ev) in history.iter().enumerate().rev() {
        let event::CommittedEvent::CopyEntryProvenanceV1 { journal } = ev else {
            continue;
        };
        let ProvenanceV1::AbsorbingCopyApplied {
            host,
            donor,
            until_turn_of,
            card_def,
            added_artifact,
            ..
        } = journal.as_ref()
        else {
            continue;
        };
        if *host != link {
            continue;
        }
        if has_player_upkeep_between(state, *until_turn_of, at + 1, before) {
            break;
        }
        if !link_had_name(state, link, crate::eternal_cube_c8c_v1::ABSORBING_MAN)
            || zone_and_zcc_at(state, host.object, at)
                != Some((crate::state::Zone::Battlefield, host.zone_change_count))
            || !crate::standard_cards_v1::had_card_def(
                state,
                host.object,
                host.zone_change_count,
                *card_def,
            )
            || authenticated_mods_for_link(state, *donor, at, depth + 1)
                .is_none_or(|donor_mods| donor_mods.added_artifact != *added_artifact)
        {
            return None;
        }
        return Some(absorbing_mods(*added_artifact));
    }

    // Phantasmal Image or Superior Spider-Man as-enters copy on `link` (token or nontoken).
    for (at, ev) in history.iter().enumerate().rev() {
        let event::CommittedEvent::CopyEntryProvenanceV1 { journal } = ev else {
            continue;
        };
        let ProvenanceV1::EntryCopyApplied {
            host,
            donor,
            values,
        } = journal.as_ref()
        else {
            continue;
        };
        if *host != link {
            continue;
        }
        if zone_and_zcc_at(state, host.object, at)
            != Some((crate::state::Zone::Battlefield, host.zone_change_count))
        {
            return None;
        }
        if link_had_name(state, link, crate::eternal_cube_c8b_v1::PHANTASMAL_IMAGE) {
            if zone_and_zcc_at(state, donor.object, at)
                != Some((crate::state::Zone::Battlefield, donor.zone_change_count))
            {
                return None;
            }
            let mut expected = authenticated_mods_for_link(state, *donor, at, depth + 1)?;
            expected.merge_subtypes(&[Subtype::Illusion]);
            expected.illusion_sacrifice = true;
            return (values.mods == expected).then_some(expected);
        }
        if link_had_name(state, link, crate::eternal_cube_c8b_v1::SUPERIOR_SPIDER_MAN) {
            if zone_and_zcc_at(state, donor.object, at)
                != Some((crate::state::Zone::Graveyard, donor.zone_change_count))
            {
                return None;
            }
            let expected = spider_man_mods();
            return (values.mods == expected).then_some(expected);
        }
        return None;
    }

    // Token copy created by Vaultborn Tyrant or Springheart Nantuko.
    if live.v4.is_token {
        for (at, ev) in history.iter().enumerate().rev() {
            let event::CommittedEvent::CopyEntryProvenanceV1 { journal } = ev else {
                continue;
            };
            let ProvenanceV1::TokenCopyCreated {
                token,
                create_at,
                producer,
                donor,
                values,
            } = journal.as_ref()
            else {
                continue;
            };
            if *token != link {
                continue;
            }
            if token.zone_change_count != 0
                || *create_at >= at
                || !matches!(
                    history.get(*create_at),
                    Some(event::CommittedEvent::CreateToken {
                        object,
                        token_def,
                        ..
                    }) if *object == token.object && *token_def == values.card_def
                )
            {
                return None;
            }
            if card_id_by_name(crate::eternal_cube_c8b_v1::VAULTBORN_TYRANT)
                == Some(producer.card_def)
            {
                if *donor
                    != (ObjectLinkV4 {
                        object: producer.source,
                        zone_change_count: producer.zone_change_count,
                    })
                    || !link_had_name(state, *donor, crate::eternal_cube_c8b_v1::VAULTBORN_TYRANT)
                {
                    return None;
                }
                let died_at =
                    history[..*create_at]
                        .iter()
                        .enumerate()
                        .rev()
                        .find_map(|(i, ev)| {
                            matches!(
                                ev,
                                event::CommittedEvent::ZoneChange {
                                    object,
                                    from: crate::state::Zone::Battlefield,
                                    to: crate::state::Zone::Graveyard,
                                    ..
                                } if *object == donor.object
                                    && zone_and_zcc_before(state, donor.object, i)
                                        == Some((
                                            crate::state::Zone::Battlefield,
                                            donor.zone_change_count,
                                        ))
                            )
                            .then_some(i)
                        })?;
                let mut expected = authenticated_mods_for_link(state, *donor, died_at, depth + 1)?;
                expected.added_artifact = true;
                return (values.mods == expected).then_some(expected);
            }
            if card_id_by_name(crate::eternal_cube_c8b_v1::SPRINGHEART_NANTUKO)
                == Some(producer.card_def)
            {
                if !object_was_named_on_battlefield_at(
                    state,
                    producer.source,
                    crate::eternal_cube_c8b_v1::SPRINGHEART_NANTUKO,
                    *create_at,
                ) || zone_and_zcc_at(state, donor.object, *create_at)
                    != Some((crate::state::Zone::Battlefield, donor.zone_change_count))
                {
                    return None;
                }
                let expected = authenticated_mods_for_link(state, *donor, *create_at, depth + 1)?;
                return (values.mods == expected).then_some(expected);
            }
            return None;
        }
        return Some(C8bCopyModsV1::default());
    }

    // Noncast Phyrexian Metamorph donor: exact inherited values + added_artifact.
    if let Some(prior) = noncast_entry_link(state, link, before) {
        let mut expected = prior
            .values
            .as_ref()
            .map(|v| v.mods.clone())
            .unwrap_or_default();
        expected.added_artifact = true;
        return Some(expected);
    }

    // Cast Phyrexian Metamorph donor: only `added_artifact: true` when copying.
    if link_had_name(state, link, "Phyrexian Metamorph") {
        let same_incarnation = live.zone == crate::state::Zone::Battlefield
            && live.zone_change_count == link.zone_change_count;
        let is_uncopied =
            same_incarnation && !crate::eternal_vintage_v1::active_copy(state, link.object);
        if is_uncopied {
            return Some(C8bCopyModsV1::default());
        }
        return Some(C8bCopyModsV1 {
            added_artifact: true,
            ..Default::default()
        });
    }

    // Phantasmal Image / Superior Spider-Man that recorded a copy definition
    // must have an `EntryCopyApplied` receipt; only an uncopied instance has
    // empty mods.
    for name in [
        crate::eternal_cube_c8b_v1::PHANTASMAL_IMAGE,
        crate::eternal_cube_c8b_v1::SUPERIOR_SPIDER_MAN,
    ] {
        if let Some(id) = card_id_by_name(name) {
            if crate::standard_cards_v1::had_card_def(
                state,
                link.object,
                link.zone_change_count,
                id,
            ) {
                return None;
            }
        }
    }

    Some(C8bCopyModsV1::default())
}

/// Checks that `mods` is a combination reachable from registered copy-modifier
/// producers (Plain, Superior Spider-Man, Absorbing Man, Phantasmal Image,
/// Metamorph/Saheeli/Vaultborn Tyrant).
fn copy_mods_structurally_valid(
    def: &crate::card_def::CardDef,
    face_index: u8,
    mods: &C8bCopyModsV1,
) -> bool {
    let mut expected_subtypes: Vec<u16> = match mods.override_name.as_deref() {
        None => {
            if mods.base_pt.is_some()
                || mods.added_creature
                || mods.added_legendary
                || mods.added_vigilance
            {
                return false;
            }
            Vec::new()
        }
        Some(crate::eternal_cube_c8b_v1::SUPERIOR_SPIDER_MAN) => {
            if mods.base_pt != Some((4, 4))
                || mods.added_creature
                || mods.added_legendary
                || mods.added_vigilance
                || face_index != 0
                || !crate::eternal_cube_c9c_v1::is_nonbattlefield_creature_def(def)
            {
                return false;
            }
            vec![
                Subtype::Spider.stable_id(),
                Subtype::Human.stable_id(),
                Subtype::Hero.stable_id(),
            ]
        }
        Some(crate::eternal_cube_c8c_v1::ABSORBING_MAN) => {
            if mods.base_pt != Some((4, 4))
                || !mods.added_creature
                || !mods.added_legendary
                || !mods.added_vigilance
            {
                return false;
            }
            vec![Subtype::Human.stable_id(), Subtype::Villain.stable_id()]
        }
        Some(_) => return false,
    };
    if mods.illusion_sacrifice {
        expected_subtypes.push(Subtype::Illusion.stable_id());
    }
    expected_subtypes.sort_unstable();
    expected_subtypes.dedup();
    mods.added_subtypes == expected_subtypes
}

/// Authenticates that `donor` actually resolved `AnimateSource` or
/// `AnimateSourcePermanentlyV1` on this exact battlefield incarnation before
/// `entry` (and in the same turn for until-end-of-turn animations). A card
/// definition having `animation: Some(_)` is never provenance by itself.
fn donor_land_animated_at_entry(
    state: &GameState,
    donor: AbilitySourceContractV4,
    def: &crate::card_def::CardDef,
    donor_same_incarnation: bool,
    can_have_expired_eot: bool,
    entry: usize,
) -> bool {
    let Some(anim) = def.animation else {
        return false;
    };
    let host_link = ObjectLinkV4 {
        object: donor.source,
        zone_change_count: donor.zone_change_count,
    };
    state.engine.event_history[..entry]
        .iter()
        .enumerate()
        .rev()
        .any(|(at, ev)| {
            let event::CommittedEvent::CopyEntryProvenanceV1 { journal } = ev else {
                return false;
            };
            let ProvenanceV1::LandAnimated {
                host,
                card_def,
                until_end_of_turn,
            } = journal.as_ref()
            else {
                return false;
            };
            if *host != host_link
                || *card_def != donor.card_def
                || *until_end_of_turn != anim.until_end_of_turn
            {
                return false;
            }
            if *until_end_of_turn {
                if has_upkeep_between(state, at + 1, entry) {
                    return false;
                }
                if !can_have_expired_eot
                    && state
                        .objects
                        .try_get(donor.source)
                        .is_none_or(|live| live.v4.animation_timestamp.is_none())
                {
                    return false;
                }
            } else if donor_same_incarnation
                && state
                    .objects
                    .try_get(donor.source)
                    .is_none_or(|live| live.v4.animation_timestamp.is_none())
            {
                return false;
            }
            zone_and_zcc_at(state, host.object, at)
                == Some((crate::state::Zone::Battlefield, host.zone_change_count))
                && state.objects.try_get(host.object).is_some_and(|live| {
                    live.card_def == *card_def
                        || crate::standard_cards_v1::had_card_def(
                            state,
                            host.object,
                            host.zone_change_count,
                            *card_def,
                        )
                })
        })
}

/// Authenticates that a non-artifact, non-creature donor was actually granted
/// Artifact or Creature at `entry` by a real battlefield effect or same-turn
/// targeting activation (never by an unused card in a library or hand, and
/// never by mere animation capability without an actual animation resolution).
fn donor_type_granted_at_entry(
    state: &GameState,
    donor: AbilitySourceContractV4,
    def: &crate::card_def::CardDef,
    face_index: u8,
    donor_same_incarnation: bool,
    entry: usize,
) -> bool {
    use crate::card_def::CardType;
    let can_have_expired_eot = !donor_same_incarnation || has_cleanup_boundary_after(state, entry);

    // Static battlefield permanents that were on the battlefield at `entry`
    // (covers cases where the static permanent left play after `entry`).
    if any_permanent_named_on_battlefield_at(state, "Mycosynth Lattice", None, entry) {
        return true;
    }
    let types = def.types_for_face(face_index);
    if types.contains(&CardType::Enchantment)
        && !def.subtypes_for_face(face_index).contains(&Subtype::Aura)
        && any_permanent_named_on_battlefield_at(state, "Opalescence", Some(donor.source), entry)
    {
        return true;
    }
    if any_permanent_named_on_battlefield_at(state, "Unable to Scream", None, entry)
        || any_permanent_named_on_battlefield_at(state, "Swift Reconfiguration", None, entry)
    {
        let aura_could_have_enchanted_donor = ["Unable to Scream", "Swift Reconfiguration"]
            .into_iter()
            .filter_map(card_id_by_name)
            .any(|aura_id| {
                state.objects.iter().any(|(id, live)| {
                    let Some((crate::state::Zone::Battlefield, zcc_at)) =
                        zone_and_zcc_at(state, id, entry)
                    else {
                        return false;
                    };
                    let is_aura = live.card_def == aura_id
                        || crate::standard_cards_v1::had_card_def(state, id, zcc_at, aura_id);
                    is_aura
                        && (!donor_same_incarnation
                            || live.zone != crate::state::Zone::Battlefield
                            || live.v4.attached_to.is_some_and(|host| {
                                host.object == donor.source
                                    && host.zone_change_count == donor.zone_change_count
                            }))
                })
            });
        if aura_could_have_enchanted_donor {
            return true;
        }
    }

    // Until-end-of-turn Liquimetal Coating: requires an actual Liquimetal
    // Coating on the battlefield that tapped and targeted this exact donor
    // incarnation in the same turn before `entry`, and could only have worn off
    // if the donor left play or a cleanup boundary passed after `entry`.
    let history = &state.engine.event_history[..entry];
    if can_have_expired_eot
        && history.iter().enumerate().any(|(tap_at, ev)| {
            let event::CommittedEvent::Tap { object: coating } = ev else {
                return false;
            };
            tap_at + 1 < entry
                && !has_upkeep_between(state, tap_at + 2, entry)
                && object_was_named_on_battlefield_at(state, *coating, "Liquimetal Coating", tap_at)
                && matches!(
                    history.get(tap_at + 1),
                    Some(event::CommittedEvent::Targeted {
                        target,
                        target_zone_change_count,
                        ..
                    }) if *target == donor.source && *target_zone_change_count == donor.zone_change_count
                )
        })
    {
        return true;
    }

    // Permanent incarnation-bound type changes (Oko Elk, Lurking Skirge,
    // Badgermole Cub earthbend, Nissa +1) remain live while `donor_same_incarnation`
    // is true, so they only need historical fallback once the donor has left play.
    if !donor_same_incarnation {
        if history.iter().enumerate().any(|(at, ev)| {
            matches!(
                ev,
                event::CommittedEvent::Targeted {
                    target,
                    target_zone_change_count,
                    ..
                } if *target == donor.source && *target_zone_change_count == donor.zone_change_count
            ) && (any_permanent_named_on_battlefield_at(state, "Oko, Thief of Crowns", None, at)
                || (types.contains(&CardType::Land)
                    && (any_permanent_named_on_battlefield_at(state, "Badgermole Cub", None, at)
                        || any_permanent_named_on_battlefield_at(
                            state,
                            "Nissa, Who Shakes the World",
                            None,
                            at,
                        ))))
        }) {
            return true;
        }
        if types.contains(&CardType::Enchantment) && face_index == 0 && def.name == "Lurking Skirge"
        {
            return true;
        }
    }

    if types.contains(&CardType::Planeswalker)
        && face_index == 0
        && def.name == "Kaito, Bane of Nightmares"
    {
        let active_at_entry = history
            .iter()
            .rev()
            .find_map(|ev| match ev {
                event::CommittedEvent::UpkeepBegan { player } => Some(*player),
                _ => None,
            })
            .unwrap_or(state.active_player);
        if active_at_entry == donor.controller {
            return true;
        }
    }

    if def.animation.is_some()
        && donor_land_animated_at_entry(
            state,
            donor,
            def,
            donor_same_incarnation,
            can_have_expired_eot,
            entry,
        )
    {
        return true;
    }

    if types.contains(&CardType::Land) && can_have_expired_eot {
        if let Some(life_id) = card_id_by_name("Life // Death") {
            let life_cast = history.iter().enumerate().any(|(at, ev)| {
                matches!(
                    ev,
                    event::CommittedEvent::SpellCast { spell, controller }
                        if *controller == donor.controller
                            && state.objects.try_get(*spell).is_some_and(|o| o.card_def == life_id)
                ) && !has_upkeep_between(state, at + 1, entry)
            });
            if life_cast {
                return true;
            }
        }
    }

    false
}

/// Authenticates a completed `MetamorphCopyV1` against the copied face's entry
/// replacement contract, `CopiableValuesV1` bounds and provenance, and donor
/// artifact/creature legality (CR 614.12, 707.2, 707.9).
pub(crate) fn validate_completed_copy(
    state: &GameState,
    copy: &crate::eternal_sideboard_s1b_v1::meddling_mage_entry::MetamorphCopyV1,
    entry: usize,
) -> bool {
    use crate::card_def::{CardType, CARD_DEFS};
    if copy.physical.zone == crate::state::Zone::Stack {
        return copy.values.is_none()
            && copy.entry_answer.is_none()
            && copy.prepared_at.is_none()
            && copy.discarded_at.is_none()
            && crate::eternal_sideboard_s1b_v1::meddling_mage_entry::owns_definition(
                copy.copied.card_def,
            );
    }
    let Some(def) = CARD_DEFS.get(usize::from(copy.copied.card_def)) else {
        return false;
    };
    if def.object_name.is_empty()
        || !(def.is_executable() || def.is_token)
        || def.name == crate::self_reflection_v1::ANONYMOUS_BLUEPRINT
    {
        return false;
    }
    if let Some(values) = &copy.values {
        if values.card_def != copy.copied.card_def
            || (values.face_index == 0
                && values.mods.is_empty()
                && !values.prototyped
                && values.copied_end_step_sacrifices_v1 == 0
                && !values.copied_nightmare_v1)
        {
            return false;
        }
    }
    let face_index = copy.values.as_ref().map_or(0, |v| v.face_index);
    let face_name = match face_index {
        0 => def.object_name,
        1 => {
            let Some(back) = def.transform_face.as_ref() else {
                return false;
            };
            back.name
        }
        _ => return false,
    };
    if unsupported_copy_face(def, face_index, face_name).is_some() {
        return false;
    }
    let answer_ok = match (face_name, copy.entry_answer) {
        ("Painter's Servant", Some(EntryAnswer::Color(color))) => {
            crate::eternal_legacy_e9a_v1::statics::COLORS.contains(&color)
        }
        ("Serra's Emissary", Some(EntryAnswer::ProtectionType(index))) => {
            u16::from(index) < crate::eternal_e5a_v1::statics::PROTECTION_CHOICE_COUNT
        }
        ("Pithing Needle", Some(EntryAnswer::NeedleName(id))) => {
            CARD_DEFS.get(usize::from(id)).is_some_and(|named| {
                !named.object_name.is_empty() && (named.is_executable() || named.is_token)
            })
        }
        ("Phyrexian Revoker", Some(EntryAnswer::RevokerName(id))) => {
            CARD_DEFS.get(usize::from(id)).is_some_and(|named| {
                !named.object_name.is_empty()
                    && (named.is_executable() || named.is_token)
                    && !named.has_type(CardType::Land)
            })
        }
        ("Cavern of Souls", Some(EntryAnswer::CreatureType(id))) => {
            Subtype::from_stable_id(id).is_some_and(Subtype::is_creature_type)
        }
        ("Mox Diamond", Some(EntryAnswer::Discard(Some(_)))) => true,
        (
            "Painter's Servant" | "Serra's Emissary" | "Pithing Needle" | "Phyrexian Revoker"
            | "Cavern of Souls" | "Mox Diamond",
            _,
        ) => false,
        (_, None) => true,
        (_, Some(_)) => false,
    };
    if !answer_ok {
        return false;
    }

    let default_mods = C8bCopyModsV1::default();
    let mods = copy.values.as_ref().map_or(&default_mods, |v| &v.mods);
    let prototyped = copy.values.as_ref().is_some_and(|v| v.prototyped);
    let sacrifices = copy
        .values
        .as_ref()
        .map_or(0, |v| v.copied_end_step_sacrifices_v1);
    let nightmare = copy.values.as_ref().is_some_and(|v| v.copied_nightmare_v1);

    if prototyped
        && (face_index != 0 || crate::eternal_pauper_pf2c_v1::prototype_profile(def).is_none())
    {
        return false;
    }
    if !copy_mods_structurally_valid(def, face_index, mods) {
        return false;
    }
    let Some(donor_live) = state.objects.try_get(copy.copied.source) else {
        return false;
    };
    let donor_link = ObjectLinkV4 {
        object: copy.copied.source,
        zone_change_count: copy.copied.zone_change_count,
    };
    let donor_same_incarnation = donor_live.zone == crate::state::Zone::Battlefield
        && donor_live.zone_change_count == copy.copied.zone_change_count;
    let donor_saheeli_live = donor_same_incarnation
        && crate::eternal_cube_c8b_v1::has_saheeli_copy(state, copy.copied.source);
    let saheeli_at_entry = saheeli_applied_at_entry(state, donor_link, entry);
    let saheeli_covered = saheeli_at_entry.is_some();

    if let Some((saheeli_at, applied)) = saheeli_at_entry {
        if copy.copied.card_def != applied.card_def
            || face_after_transforms_between(
                state,
                copy.copied.source,
                applied.face_index,
                saheeli_at + 1,
                entry,
            ) != Some(face_index)
            || mods != &applied.mods
            || prototyped != applied.prototyped
            || sacrifices != applied.copied_end_step_sacrifices_v1
            || nightmare != applied.copied_nightmare_v1
        {
            return false;
        }
    } else {
        let Some(expected_mods) = authenticated_mods_for_link(state, donor_link, entry, 0) else {
            return false;
        };
        if mods != &expected_mods {
            return false;
        }

        if donor_same_incarnation
            && !crate::standard_cards_v1::active_aegis_copy(state, copy.copied.source)
            && !crate::eternal_legacy_e7a_v1::active_copy(state, copy.copied.source)
        {
            if donor_live.v4.face_down_v1.is_some() {
                return false;
            }
            if donor_saheeli_live {
                // Donor was NOT covered by Saheeli at `entry`, so a live
                // `saheeli_copies` record must belong to a post-entry Saheeli
                // resolution. Compare against its saved pre-Saheeli values.
                if !has_valid_post_entry_saheeli(state, donor_link, entry) {
                    return false;
                }
                let Some(saved) = read_state(state)
                    .and_then(|c8b| c8b.saheeli_copies.iter().find(|c| c.host == donor_link))
                else {
                    return false;
                };
                if saved.card_def == copy.copied.card_def
                    && !crate::eternal_cube_c8c_v1::active_copy(state, copy.copied.source)
                {
                    let mut saved_mods = saved.prev_mods.clone().unwrap_or_default();
                    if crate::eternal_vintage_v1::active_copy(state, copy.copied.source) {
                        saved_mods.added_artifact = true;
                    }
                    if mods != &saved_mods
                        || reconstruct_face_at_entry(
                            state,
                            copy.copied.source,
                            saved.face_index,
                            entry,
                        ) != Some(face_index)
                    {
                        return false;
                    }
                }
            } else if donor_live.card_def == copy.copied.card_def {
                let Some(live_vals) =
                    capture_battlefield_copiable_values(state, copy.copied.source)
                else {
                    return false;
                };
                if prototyped != live_vals.prototyped {
                    return false;
                }
                let donor_was_absorbing =
                    link_had_name(state, donor_link, crate::eternal_cube_c8c_v1::ABSORBING_MAN);
                if donor_was_absorbing {
                    if mods.override_name != live_vals.mods.override_name
                        || mods.base_pt != live_vals.mods.base_pt
                        || mods.added_subtypes != live_vals.mods.added_subtypes
                        || mods.illusion_sacrifice != live_vals.mods.illusion_sacrifice
                        || mods.added_creature != live_vals.mods.added_creature
                        || mods.added_legendary != live_vals.mods.added_legendary
                        || mods.added_vigilance != live_vals.mods.added_vigilance
                    {
                        return false;
                    }
                } else if mods != &live_vals.mods {
                    return false;
                }
                if !crate::eternal_cube_c8c_v1::is_absorbing_copy(state, copy.copied.source)
                    && reconstruct_face_at_entry(
                        state,
                        copy.copied.source,
                        donor_live.v4.face_index,
                        entry,
                    ) != Some(face_index)
                {
                    return false;
                }
            }
        }
    }

    match event::historical_copiable_token_abilities_v1(state, copy.copied) {
        Ok(expected) => {
            if sacrifices != expected && !(sacrifices == 0 && saheeli_covered) {
                return false;
            }
        }
        Err(_) => return false,
    }
    match crate::effect::harvester_v1::historical_nightmare(state, copy.copied, entry) {
        Ok(expected) => {
            if (!saheeli_covered || nightmare) && nightmare != expected {
                return false;
            }
        }
        Err(_) => return false,
    }

    let face_types = def.types_for_face(face_index);
    let naturally_eligible = face_types.contains(&CardType::Artifact)
        || face_types.contains(&CardType::Creature)
        || mods.added_artifact
        || mods.added_creature;
    if !naturally_eligible {
        let live_eligible = donor_same_incarnation
            && (crate::engine::object_has_type(state, copy.copied.source, CardType::Artifact)
                || crate::engine::object_has_type(state, copy.copied.source, CardType::Creature));
        if !live_eligible
            && !donor_type_granted_at_entry(
                state,
                copy.copied,
                def,
                face_index,
                donor_same_incarnation,
                entry,
            )
        {
            return false;
        }
    }

    true
}

/// Recover non-token inherited copy abilities from the committed entry.
/// Ordinary token creations have a separate receipt.
pub(crate) fn inherited_values(
    state: &GameState,
    source: AbilitySourceContractV4,
    before: usize,
) -> Option<&CopiableValuesV1> {
    noncast_entry(state, source, before)?.values.as_ref()
}

fn noncast_entry(
    state: &GameState,
    source: AbilitySourceContractV4,
    before: usize,
) -> Option<&crate::eternal_sideboard_s1b_v1::meddling_mage_entry::MetamorphCopyV1> {
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
                && copy.physical.zone != crate::state::Zone::Stack)
                .then_some(copy)
        })
}

/// Saheeli can cover this new nontoken entry until cleanup. Its existing
/// saved record already restores the underlying subtype list. Recover the
/// underlying ability flags from the entry instead of extending that record.
pub(crate) fn set_covered_by_saheeli(state: &mut GameState, object: ObjectId, covered: bool) {
    if !crate::eternal_vintage_v1::active_copy(state, object) {
        return;
    }
    let source = AbilitySourceContractV4::capture(state, object);
    let Some(values) = inherited_values(state, source, state.engine.event_history.len()) else {
        return;
    };
    let count = values.copied_end_step_sacrifices_v1;
    let nightmare = values.copied_nightmare_v1;
    if count == 0 && !nightmare {
        return;
    }
    let live = state.objects.get_mut(object);
    let prior = live.v4.copied_nightmare_v1;
    live.v4.copied_end_step_sacrifices_v1 = if covered { 0 } else { count };
    live.v4.copied_nightmare_v1 = !covered && nightmare;
    crate::effect::harvester_v1::record_layer(state, object, prior);
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
        || crate::standard_cards_v1::active_aegis_copy(state, entrant.source)
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

#[cfg(all(test, feature = "eternal-fixtures"))]
mod tests {
    use super::*;
    use crate::engine::{advance_until_decision, step, Action, Decision};
    use crate::state::{Step, Target, Zone};

    #[test]
    fn flat_actor_observes_completed_nonmage_copies_and_refuses_pending_or_named_copies() {
        for player in [PlayerId::P0, PlayerId::P1] {
            for donor_name in ["Mind Stone", "Meddling Mage"] {
                let cards = ["Reanimate", "Phyrexian Metamorph", donor_name, "Island"]
                    .map(|name| card_id_by_name(name).unwrap());
                let mut state = GameState::new_from_libraries(
                    &cards,
                    &cards,
                    |id| {
                        crate::card_def::CARD_DEFS[usize::from(id)]
                            .object_name
                            .into()
                    },
                    137,
                );
                state.active_player = player;
                state.priority_player = player;
                state.step = Step::Main1;
                let mut place = |name: &str, zone| {
                    let seat = &mut state.players[player.index()];
                    let pos = seat
                        .library
                        .iter()
                        .position(|id| {
                            state.objects.get(*id).card_def == card_id_by_name(name).unwrap()
                        })
                        .unwrap();
                    let object = seat.library.remove(pos);
                    state.objects.get_mut(object).zone = zone;
                    match zone {
                        Zone::Hand => seat.hand.push(object),
                        Zone::Graveyard => seat.graveyard.push(object),
                        Zone::Battlefield => seat.battlefield.push(object),
                        _ => unreachable!(),
                    }
                    object
                };
                let spell = place("Reanimate", Zone::Hand);
                let entrant = place("Phyrexian Metamorph", Zone::Graveyard);
                let donor = place(donor_name, Zone::Battlefield);
                state.players[player.index()].mana_pool[crate::mana::ManaColor::B.pool_index()] = 1;
                let surface = crate::policy_surface_v5::PolicySurfaceV5::new();
                let flat = |state: &GameState| {
                    crate::rl::observe_policy_v5_unhashed_for_flat_policy(
                        state, &surface, player, 0, 0, 0, 1,
                    )
                };
                advance_until_decision(&mut state);
                step(&mut state, Action::CastSpell(spell)).unwrap();
                advance_until_decision(&mut state);
                step(&mut state, Action::ChooseTarget(Target::Object(entrant))).unwrap();
                let mut chose = false;
                for _ in 0..24 {
                    let response = advance_until_decision(&mut state);
                    let action = match response {
                        Decision::ChooseEffectTargets { source, .. } if source == entrant => {
                            assert!(flat(&state).is_err());
                            chose = true;
                            Action::ChooseEffectTarget(Target::Object(donor))
                        }
                        Decision::ChooseMeddlingMageNameV1 { .. } => Action::ChooseEffectOption(0),
                        Decision::CastSpellOrPass { .. } if chose && state.stack.is_empty() => {
                            break
                        }
                        Decision::CastSpellOrPass { .. } => Action::Pass,
                        other => panic!("unexpected copy decision: {other:?}"),
                    };
                    step(&mut state, action).unwrap();
                }
                assert!(chose);
                assert_eq!(state.objects.get(entrant).zone, Zone::Battlefield);
                assert_eq!(
                    state.objects.get(entrant).card_def,
                    card_id_by_name(donor_name).unwrap()
                );
                assert_eq!(flat(&state).is_ok(), donor_name == "Mind Stone");
            }
        }
    }
}
