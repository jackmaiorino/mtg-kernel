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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum CounteredAbilityKindV1 {
    AbsorbingMan,
    SaheeliCopy,
    VaultbornDeathCopy,
    SpringheartLandfall {
        producer: AbilitySourceContractV4,
    },
    LandAnimate {
        host: ObjectLinkV4,
    },
    EffectDiscard {
        source: ObjectLinkV4,
        ability_index: usize,
    },
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
        stack_item: crate::ids::StackItemId,
        animation_timestamp: u64,
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
        stack_item: crate::ids::StackItemId,
    },
    TokenCopyCreated {
        token: ObjectLinkV4,
        create_at: usize,
        producer: AbilitySourceContractV4,
        donor: ObjectLinkV4,
        values: CopiableValuesV1,
        stack_item: crate::ids::StackItemId,
    },
    CopyDiscardPaid {
        stack_item: crate::ids::StackItemId,
        physical: AbilitySourceContractV4,
        land: AbilitySourceContractV4,
        discard_at: usize,
    },
    AbilityCountered {
        stack_item: crate::ids::StackItemId,
        kind: CounteredAbilityKindV1,
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
    stack_item: crate::ids::StackItemId,
    animation_timestamp: u64,
) {
    push_provenance(
        state,
        ProvenanceV1::LandAnimated {
            host,
            card_def,
            until_end_of_turn,
            stack_item,
            animation_timestamp,
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
#[allow(clippy::too_many_arguments)]
pub(crate) fn record_absorbing_copy_applied(
    state: &mut GameState,
    host: ObjectLinkV4,
    donor: ObjectLinkV4,
    until_turn_of: PlayerId,
    card_def: u16,
    face_index: u8,
    added_artifact: bool,
    stack_item: crate::ids::StackItemId,
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
            stack_item,
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
    stack_item: crate::ids::StackItemId,
) {
    push_provenance(
        state,
        ProvenanceV1::TokenCopyCreated {
            token,
            create_at,
            producer,
            donor,
            values,
            stack_item,
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

#[cfg(feature = "eternal-fixtures")]
pub(crate) fn record_ability_countered_if_relevant(
    state: &mut GameState,
    item: &crate::state::StackItem,
) {
    use crate::effect::EffectOp;
    use crate::eternal_cards_v1::EternalOpV1;
    use crate::eternal_cube_c8b_v1::C8bOpV1;
    use crate::eternal_cube_c8c_v1::C8cOpV1;
    use crate::standard_cards_v1::StandardOpV1;

    if item.kind == crate::state::StackItemKind::Spell {
        return;
    }
    let kind = match item.inline_effect.as_ref() {
        Some(EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C8bV1(
            C8bOpV1::SpringheartLandfallV1,
        )))) => {
            let producer = item
                .v4
                .ability_source_contract
                .unwrap_or_else(|| AbilitySourceContractV4::capture(state, item.source));
            CounteredAbilityKindV1::SpringheartLandfall { producer }
        }
        Some(EffectOp::AnimateSource | EffectOp::AnimateSourcePermanentlyV1) => {
            let Some(host) = item
                .v4
                .ability_source_contract
                .map(|c| ObjectLinkV4 {
                    object: c.source,
                    zone_change_count: c.zone_change_count,
                })
                .or_else(|| {
                    state.objects.try_get(item.source).map(|live| ObjectLinkV4 {
                        object: item.source,
                        zone_change_count: live.zone_change_count,
                    })
                })
            else {
                return;
            };
            CounteredAbilityKindV1::LandAnimate { host }
        }
        Some(EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C8bV1(
            C8bOpV1::SaheeliCopyV1,
        )))) => CounteredAbilityKindV1::SaheeliCopy,
        Some(EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C8bV1(
            C8bOpV1::VaultbornDeathCopyV1,
        )))) => CounteredAbilityKindV1::VaultbornDeathCopy,
        Some(EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::C8cV1(
            C8cOpV1::AbsorbingManCopyV1,
        )))) => CounteredAbilityKindV1::AbsorbingMan,
        Some(effect)
            if item.kind == crate::state::StackItemKind::ActivatedAbility
                && crate::eternal_sideboard_s1b_v1::meddling_mage_entry::has_mandatory_effect_discard(
                    effect,
                ) =>
        {
            let Some(ability_index) = item.v4.activated_ability_index.map(usize::from) else {
                return;
            };
            let Some(source) = item
                .v4
                .ability_source_contract
                .map(|c| ObjectLinkV4 {
                    object: c.source,
                    zone_change_count: c.zone_change_count,
                })
                .or_else(|| {
                    state.objects.try_get(item.source).map(|live| ObjectLinkV4 {
                        object: item.source,
                        zone_change_count: live.zone_change_count,
                    })
                })
            else {
                return;
            };
            CounteredAbilityKindV1::EffectDiscard {
                source,
                ability_index,
            }
        }
        _ => return,
    };
    push_provenance(
        state,
        ProvenanceV1::AbilityCountered {
            stack_item: item.v4.stack_item_id,
            kind,
        },
    );
}

fn object_was_link_on_battlefield_prior_to(
    state: &GameState,
    link: ObjectLinkV4,
    at: usize,
) -> bool {
    if zone_and_zcc_before(state, link.object, at)
        == Some((crate::state::Zone::Battlefield, link.zone_change_count))
    {
        return true;
    }
    state.engine.event_history[..at]
        .iter()
        .enumerate()
        .any(|(i, ev)| {
            matches!(
                ev,
                event::CommittedEvent::ZoneChange {
                    object,
                    from: crate::state::Zone::Battlefield,
                    ..
                } if *object == link.object
            ) && zone_and_zcc_before(state, link.object, i)
                == Some((crate::state::Zone::Battlefield, link.zone_change_count))
        })
}

fn any_object_named_on_battlefield_prior_to(state: &GameState, name: &str, at: usize) -> bool {
    let Some(target_id) = card_id_by_name(name) else {
        return false;
    };
    let matches_def = |id: ObjectId, live: &crate::state::GameObject, zcc: u32| -> bool {
        live.card_def == target_id
            || crate::standard_cards_v1::had_card_def(state, id, zcc, target_id)
    };
    if state.objects.iter().any(|(id, live)| {
        matches!(
            zone_and_zcc_before(state, id, at),
            Some((crate::state::Zone::Battlefield, zcc)) if matches_def(id, live, zcc)
        )
    }) {
        return true;
    }
    state.engine.event_history[..at]
        .iter()
        .enumerate()
        .any(|(i, ev)| {
            let event::CommittedEvent::ZoneChange {
                object,
                from: crate::state::Zone::Battlefield,
                ..
            } = ev
            else {
                return false;
            };
            let Some(live) = state.objects.try_get(*object) else {
                return false;
            };
            matches!(
                zone_and_zcc_before(state, *object, i),
                Some((crate::state::Zone::Battlefield, zcc)) if matches_def(*object, live, zcc)
            )
        })
}

pub(crate) fn validate_ability_countered_receipt(
    state: &GameState,
    at: usize,
    stack_item: crate::ids::StackItemId,
    kind: &CounteredAbilityKindV1,
) -> bool {
    if at >= state.engine.event_history.len()
        || !stack_item_in_bounds(state, stack_item)
        || stack_item_collides_in_history(state, stack_item, at, true)
    {
        return false;
    }
    let history_before = &state.engine.event_history[..at];
    match kind {
        CounteredAbilityKindV1::SpringheartLandfall { producer } => {
            let link = ObjectLinkV4 {
                object: producer.source,
                zone_change_count: producer.zone_change_count,
            };
            card_id_by_name(crate::eternal_cube_c8b_v1::SPRINGHEART_NANTUKO)
                == Some(producer.card_def)
                && producer.zone == crate::state::Zone::Battlefield
                && state.objects.try_get(producer.source).is_some_and(|live| {
                    live.owner == producer.owner
                        && live.zone_change_count >= producer.zone_change_count
                        && (live.card_def == producer.card_def
                            || crate::standard_cards_v1::had_card_def(
                                state,
                                producer.source,
                                producer.zone_change_count,
                                producer.card_def,
                            ))
                })
                && object_was_link_on_battlefield_prior_to(state, link, at)
        }
        CounteredAbilityKindV1::LandAnimate { host } => {
            let has_animate = |card_def: u16| -> bool {
                crate::card_def::CARD_DEFS
                    .get(usize::from(card_def))
                    .is_some_and(|def| {
                        def.activated_abilities.iter().any(|a| {
                            matches!(
                                (a.effect)(),
                                crate::effect::EffectOp::AnimateSource
                                    | crate::effect::EffectOp::AnimateSourcePermanentlyV1
                            )
                        })
                    })
            };
            let saheeli_saved = read_state(state)
                .and_then(|c8b| c8b.saheeli_copies.iter().find(|c| c.host == *host))
                .is_some_and(|c| has_animate(c.card_def));
            state.objects.try_get(host.object).is_some_and(|live| {
                live.zone_change_count >= host.zone_change_count
                    && (has_animate(live.card_def)
                        || saheeli_saved
                        || crate::standard_cards_v1::had_card_def_matching(
                            state,
                            host.object,
                            host.zone_change_count,
                            has_animate,
                        ))
            }) && object_was_link_on_battlefield_prior_to(state, *host, at)
        }
        CounteredAbilityKindV1::AbsorbingMan => {
            let targeted_count = history_before
                .iter()
                .filter(|ev| {
                    matches!(
                        ev,
                        event::CommittedEvent::Targeted {
                            targeting_stack_item,
                            ..
                        } if *targeting_stack_item == stack_item
                    )
                })
                .count();
            targeted_count >= 1
                && any_object_named_on_battlefield_prior_to(
                    state,
                    crate::eternal_cube_c8c_v1::ABSORBING_MAN,
                    at,
                )
        }
        CounteredAbilityKindV1::SaheeliCopy => {
            let targeted_count = history_before
                .iter()
                .filter(|ev| {
                    matches!(
                        ev,
                        event::CommittedEvent::Targeted {
                            targeting_stack_item,
                            ..
                        } if *targeting_stack_item == stack_item
                    )
                })
                .count();
            targeted_count >= 2
                && any_object_named_on_battlefield_prior_to(
                    state,
                    crate::eternal_cube_c8b_v1::SAHEELI,
                    at,
                )
        }
        CounteredAbilityKindV1::VaultbornDeathCopy => {
            let Some(vaultborn_id) = card_id_by_name(crate::eternal_cube_c8b_v1::VAULTBORN_TYRANT)
            else {
                return false;
            };
            history_before.iter().enumerate().any(|(i, ev)| {
                let event::CommittedEvent::ZoneChange {
                    object,
                    from: crate::state::Zone::Battlefield,
                    to: crate::state::Zone::Graveyard,
                    ..
                } = ev
                else {
                    return false;
                };
                let Some(live) = state.objects.try_get(*object) else {
                    return false;
                };
                matches!(
                    zone_and_zcc_before(state, *object, i),
                    Some((crate::state::Zone::Battlefield, zcc))
                        if live.card_def == vaultborn_id
                            || crate::standard_cards_v1::had_card_def(
                                state,
                                *object,
                                zcc,
                                vaultborn_id,
                            )
                )
            })
        }
        CounteredAbilityKindV1::EffectDiscard {
            source,
            ability_index,
        } => {
            let has_discard = |card_def: u16| -> bool {
                crate::card_def::CARD_DEFS
                    .get(usize::from(card_def))
                    .and_then(|def| def.activated_abilities.get(*ability_index))
                    .is_some_and(|ability| {
                        ability.activation_zone == crate::state::Zone::Battlefield
                            && crate::eternal_sideboard_s1b_v1::meddling_mage_entry::has_mandatory_effect_discard(
                                &(ability.effect)(),
                            )
                    })
            };
            state.objects.try_get(source.object).is_some_and(|live| {
                live.zone_change_count >= source.zone_change_count
                    && (has_discard(live.card_def)
                        || crate::standard_cards_v1::had_card_def_matching(
                            state,
                            source.object,
                            source.zone_change_count,
                            has_discard,
                        ))
            }) && object_was_link_on_battlefield_prior_to(state, *source, at)
        }
    }
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

fn stack_item_in_bounds(state: &GameState, stack_item: crate::ids::StackItemId) -> bool {
    stack_item.0 > 0
        && stack_item.0 <= state.engine.next_stack_item_id
        && !state
            .stack
            .iter()
            .any(|row| row.v4.stack_item_id == stack_item)
}

fn event_uses_stack_item(
    ev: &event::CommittedEvent,
    stack_item: crate::ids::StackItemId,
    allow_targeted: bool,
) -> bool {
    use crate::eternal_sideboard_s1b_v1::meddling_mage_entry::JournalV1 as MageJournal;
    match ev {
        event::CommittedEvent::PublishedCastIdentityV1 { receipt } => {
            receipt.stack_item_id == stack_item
        }
        event::CommittedEvent::Targeted {
            targeting_stack_item,
            ..
        }
        | event::CommittedEvent::CrimeCommitted {
            targeting_stack_item,
            ..
        } => !allow_targeted && *targeting_stack_item == stack_item,
        event::CommittedEvent::CastLifeXPaidV1 {
            stack_item: item, ..
        }
        | event::CommittedEvent::MartyrRevealPaidV1 { item, .. } => *item == stack_item,
        event::CommittedEvent::SpellCopyCostPaidV1 { parent, .. } => *parent == stack_item,
        event::CommittedEvent::MiracleDrawSuspendedV1 { stack_item_id, .. }
        | event::CommittedEvent::MiracleDrawResumedV1 { stack_item_id, .. }
        | event::CommittedEvent::ResolutionDiscardWaitsV1 { stack_item_id, .. }
        | event::CommittedEvent::ResolutionDrawChoiceOfferedV1 { stack_item_id, .. }
        | event::CommittedEvent::ResolutionDrawChoiceAnsweredV1 { stack_item_id, .. }
        | event::CommittedEvent::NivMizzetVisionaryDrawEndedV1 { stack_item_id, .. }
        | event::CommittedEvent::CopiedTokenSacrificeFinishedV1 { stack_item_id, .. } => {
            *stack_item_id == stack_item
        }
        event::CommittedEvent::CopiedTokenSacrificePublishedV1 { item, .. } => {
            item.v4.stack_item_id == stack_item
        }
        event::CommittedEvent::MeddlingMageEntryV1 { journal } => match journal.as_ref() {
            MageJournal::VialPaid { item, .. } => *item == stack_item,
            MageJournal::NoncastCopyPrepared { copy }
            | MageJournal::ShowAndTellMetamorphChosen { copy }
            | MageJournal::MetamorphCopied { copy, .. } => copy.stack_item == stack_item,
            _ => false,
        },
        event::CommittedEvent::CopyEntryProvenanceV1 { journal } => match journal.as_ref() {
            ProvenanceV1::LandAnimated {
                stack_item: item, ..
            }
            | ProvenanceV1::SaheeliApplied {
                stack_item: item, ..
            }
            | ProvenanceV1::AbsorbingCopyApplied {
                stack_item: item, ..
            }
            | ProvenanceV1::TokenCopyCreated {
                stack_item: item, ..
            }
            | ProvenanceV1::CopyDiscardPaid {
                stack_item: item, ..
            }
            | ProvenanceV1::AbilityCountered {
                stack_item: item, ..
            } => *item == stack_item,
            ProvenanceV1::EntryCopyApplied { .. } => false,
        },
        _ => false,
    }
}

fn stack_item_collides_in_history(
    state: &GameState,
    stack_item: crate::ids::StackItemId,
    exclude_at: usize,
    allow_targeted: bool,
) -> bool {
    state
        .engine
        .event_history
        .iter()
        .enumerate()
        .any(|(i, ev)| i != exclude_at && event_uses_stack_item(ev, stack_item, allow_targeted))
}

fn turn_window_containing(state: &GameState, at: usize) -> (usize, usize) {
    let history = &state.engine.event_history;
    let start = history[..at]
        .iter()
        .rposition(|ev| matches!(ev, event::CommittedEvent::UpkeepBegan { .. }))
        .map_or(0, |idx| idx + 1);
    let end = history[at + 1..]
        .iter()
        .position(|ev| matches!(ev, event::CommittedEvent::UpkeepBegan { .. }))
        .map_or(history.len(), |offset| at + 1 + offset);
    (start, end)
}

fn link_could_have_card_def(state: &GameState, link: ObjectLinkV4, card_def: u16) -> bool {
    state.objects.try_get(link.object).is_some_and(|live| {
        live.card_def == card_def
            || crate::standard_cards_v1::had_card_def(
                state,
                link.object,
                link.zone_change_count,
                card_def,
            )
    })
}

#[allow(clippy::too_many_arguments)]
fn validate_absorbing_provenance(
    state: &GameState,
    at: usize,
    host: ObjectLinkV4,
    donor: ObjectLinkV4,
    until_turn_of: PlayerId,
    card_def: u16,
    face_index: u8,
    added_artifact: bool,
    stack_item: crate::ids::StackItemId,
    depth: u8,
) -> bool {
    if depth > 8
        || host.object == donor.object
        || !stack_item_in_bounds(state, stack_item)
        || stack_item_collides_in_history(state, stack_item, at, true)
        || !link_had_name(state, host, crate::eternal_cube_c8c_v1::ABSORBING_MAN)
        || zone_and_zcc_at(state, host.object, at)
            != Some((crate::state::Zone::Battlefield, host.zone_change_count))
        || zone_and_zcc_at(state, donor.object, at)
            != Some((crate::state::Zone::Battlefield, donor.zone_change_count))
        || !crate::standard_cards_v1::had_card_def(
            state,
            host.object,
            host.zone_change_count,
            card_def,
        )
    {
        return false;
    }
    let history = &state.engine.event_history;
    // No `Targeted` event for `stack_item` may appear after `at`, nor with a
    // mismatched controller anywhere before `at`.
    if history[at + 1..].iter().any(|ev| {
        matches!(
            ev,
            event::CommittedEvent::Targeted {
                targeting_stack_item,
                ..
            } if *targeting_stack_item == stack_item
        )
    }) || history[..at].iter().any(|ev| {
        matches!(
            ev,
            event::CommittedEvent::Targeted {
                targeting_stack_item,
                targeting_controller,
                ..
            } if *targeting_stack_item == stack_item && *targeting_controller != until_turn_of
        )
    }) {
        return false;
    }
    let targets: Vec<(usize, ObjectId, u32)> = history[..at]
        .iter()
        .enumerate()
        .filter_map(|(t_at, ev)| match ev {
            event::CommittedEvent::Targeted {
                target,
                target_zone_change_count,
                targeting_stack_item,
                targeting_controller,
            } if *targeting_stack_item == stack_item && *targeting_controller == until_turn_of => {
                Some((t_at, *target, *target_zone_change_count))
            }
            _ => None,
        })
        .collect();
    let (Some(&(first_target_at, _, _)), Some(&(_, last_target, last_zcc))) =
        (targets.first(), targets.last())
    else {
        return false;
    };
    if last_target != donor.object
        || last_zcc != donor.zone_change_count
        || has_upkeep_between(state, first_target_at + 1, at)
        || zone_and_zcc_at(state, host.object, first_target_at)
            != Some((crate::state::Zone::Battlefield, host.zone_change_count))
    {
        return false;
    }
    // Enforce monotonic stack-item allocation order against spell casts before
    // the trigger went on the stack and after it resolved.
    for (cast_idx, ev) in history.iter().enumerate() {
        let event::CommittedEvent::PublishedCastIdentityV1 { receipt } = ev else {
            continue;
        };
        if cast_idx < first_target_at && receipt.stack_item_id >= stack_item {
            return false;
        }
        if cast_idx > at && receipt.stack_item_id <= stack_item {
            return false;
        }
    }
    if !donor_def_and_face_match(state, donor, at, card_def, face_index, depth + 1) {
        return false;
    }
    let Some(live) = state.objects.try_get(host.object) else {
        return false;
    };
    let host_still_same = live.zone == crate::state::Zone::Battlefield
        && live.zone_change_count == host.zone_change_count;
    if host_still_same
        && !has_player_upkeep_between(state, until_turn_of, at + 1, history.len())
        && !crate::eternal_cube_c8c_v1::active_copy(state, host.object)
    {
        return false;
    }
    authenticated_mods_for_link(state, donor, at, depth + 1)
        .is_some_and(|donor_mods| donor_mods.added_artifact == added_artifact)
}

fn has_unexpired_absorbing_before(state: &GameState, link: ObjectLinkV4, before: usize) -> bool {
    state.engine.event_history[..before]
        .iter()
        .enumerate()
        .rev()
        .any(|(i, ev)| {
            let event::CommittedEvent::CopyEntryProvenanceV1 { journal } = ev else {
                return false;
            };
            let ProvenanceV1::AbsorbingCopyApplied {
                host,
                until_turn_of,
                ..
            } = journal.as_ref()
            else {
                return false;
            };
            *host == link && !has_player_upkeep_between(state, *until_turn_of, i + 1, before)
        })
}

fn donor_def_and_face_match(
    state: &GameState,
    donor: ObjectLinkV4,
    at: usize,
    card_def: u16,
    face_index: u8,
    depth: u8,
) -> bool {
    if depth > 8 || face_index > 1 {
        return false;
    }
    let Some(def) = crate::card_def::CARD_DEFS.get(usize::from(card_def)) else {
        return false;
    };
    if face_index == 1 && def.transform_face.is_none() {
        return false;
    }
    if !link_could_have_card_def(state, donor, card_def) {
        return false;
    }
    if let Some((s_at, applied)) = saheeli_applied_at_entry(state, donor, at, depth + 1) {
        return applied.card_def == card_def
            && face_after_transforms_between(
                state,
                donor.object,
                applied.face_index,
                s_at + 1,
                at,
            ) == Some(face_index);
    }
    let history = &state.engine.event_history[..at];
    // If `donor` has an active `AbsorbingCopyApplied` at `at`, authenticate it
    // and bind `card_def` and `face_index` to that exact copy.
    for (abs_at, ev) in history.iter().enumerate().rev() {
        let event::CommittedEvent::CopyEntryProvenanceV1 { journal } = ev else {
            continue;
        };
        let ProvenanceV1::AbsorbingCopyApplied {
            host,
            donor: abs_donor,
            until_turn_of,
            card_def: abs_def,
            face_index: abs_face,
            added_artifact,
            stack_item,
        } = journal.as_ref()
        else {
            continue;
        };
        if *host != donor {
            continue;
        }
        if has_player_upkeep_between(state, *until_turn_of, abs_at + 1, at) {
            break;
        }
        if !validate_absorbing_provenance(
            state,
            abs_at,
            *host,
            *abs_donor,
            *until_turn_of,
            *abs_def,
            *abs_face,
            *added_artifact,
            *stack_item,
            depth + 1,
        ) {
            return false;
        }
        return *abs_def == card_def
            && face_after_transforms_between(state, donor.object, *abs_face, abs_at + 1, at)
                == Some(face_index);
    }
    // If `donor` had an `EntryCopyApplied` (Phantasmal Image / Superior
    // Spider-Man) before `at`, bind `card_def` and `face_index` to that entry copy.
    if let Some((entry_at, entry_values)) = history.iter().enumerate().rev().find_map(|(i, ev)| {
        let event::CommittedEvent::CopyEntryProvenanceV1 { journal } = ev else {
            return None;
        };
        let ProvenanceV1::EntryCopyApplied { host, values, .. } = journal.as_ref() else {
            return None;
        };
        (*host == donor).then_some((i, values))
    }) {
        if entry_values.card_def != card_def
            || face_after_transforms_between(
                state,
                donor.object,
                entry_values.face_index,
                entry_at + 1,
                at,
            ) != Some(face_index)
        {
            return false;
        }
    }
    let Some(live) = state.objects.try_get(donor.object) else {
        return false;
    };
    if live.v4.is_token {
        if let Some((tok_create_at, tok_values)) =
            history.iter().enumerate().rev().find_map(|(_, ev)| {
                let event::CommittedEvent::CopyEntryProvenanceV1 { journal } = ev else {
                    return None;
                };
                let ProvenanceV1::TokenCopyCreated {
                    token,
                    create_at,
                    values,
                    ..
                } = journal.as_ref()
                else {
                    return None;
                };
                (*token == donor).then_some((*create_at, values))
            })
        {
            if tok_values.card_def != card_def
                || face_after_transforms_between(
                    state,
                    donor.object,
                    tok_values.face_index,
                    tok_create_at + 1,
                    at,
                ) != Some(face_index)
            {
                return false;
            }
        }
    }
    let same_incarnation = live.zone == crate::state::Zone::Battlefield
        && live.zone_change_count == donor.zone_change_count;
    if same_incarnation
        && !crate::standard_cards_v1::active_aegis_copy(state, donor.object)
        && !crate::eternal_legacy_e7a_v1::active_copy(state, donor.object)
        && !crate::eternal_cube_c8c_v1::active_copy(state, donor.object)
    {
        if crate::eternal_cube_c8b_v1::has_saheeli_copy(state, donor.object) {
            let Some(saved) = read_state(state)
                .and_then(|c8b| c8b.saheeli_copies.iter().find(|c| c.host == donor))
            else {
                return false;
            };
            if saved.card_def != card_def
                || reconstruct_face_at_entry(state, donor.object, saved.face_index, at)
                    != Some(face_index)
            {
                return false;
            }
        } else if live.card_def != card_def
            || reconstruct_face_at_entry(state, donor.object, live.v4.face_index, at)
                != Some(face_index)
        {
            return false;
        }
    }
    true
}

#[allow(clippy::too_many_arguments)]
fn validate_saheeli_provenance(
    state: &GameState,
    saheeli_at: usize,
    source: AbilitySourceContractV4,
    stack_item: crate::ids::StackItemId,
    host: ObjectLinkV4,
    model: ObjectLinkV4,
    values: &CopiableValuesV1,
    depth: u8,
) -> bool {
    if depth > 8
        || host.object == model.object
        || !values.mods.added_artifact
        || !stack_item_in_bounds(state, stack_item)
        || stack_item_collides_in_history(state, stack_item, saheeli_at, true)
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
    let Some(def) = crate::card_def::CARD_DEFS.get(usize::from(values.card_def)) else {
        return false;
    };
    if !copy_mods_structurally_valid(def, values.face_index, &values.mods) {
        return false;
    }
    // Planeswalker loyalty abilities can be activated at most once per turn per
    // battlefield incarnation (CR 606.3). Reject duplicate SaheeliApplied
    // receipts for the same Saheeli incarnation in the same turn.
    let (turn_start, turn_end) = turn_window_containing(state, saheeli_at);
    if state.engine.event_history[turn_start..turn_end]
        .iter()
        .enumerate()
        .any(|(offset, ev)| {
            let idx = turn_start + offset;
            idx != saheeli_at
                && matches!(
                    ev,
                    event::CommittedEvent::CopyEntryProvenanceV1 { journal }
                        if matches!(
                            journal.as_ref(),
                            ProvenanceV1::SaheeliApplied {
                                source: other_source,
                                ..
                            } if other_source.source == source.source
                                && other_source.zone_change_count == source.zone_change_count
                        )
                )
        })
    {
        return false;
    }
    // No `Targeted` event for `stack_item` may appear after `saheeli_at`, nor
    // with a mismatched controller anywhere in history.
    if state.engine.event_history[saheeli_at + 1..]
        .iter()
        .any(|ev| {
            matches!(
                ev,
                event::CommittedEvent::Targeted {
                    targeting_stack_item,
                    ..
                } if *targeting_stack_item == stack_item
            )
        }) || state.engine.event_history[..saheeli_at].iter().any(|ev| {
        matches!(
            ev,
            event::CommittedEvent::Targeted {
                targeting_stack_item,
                targeting_controller,
                ..
            } if *targeting_stack_item == stack_item && *targeting_controller != source.controller
        )
    }) {
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
    let first_target_at = targets[0].0;
    let slot0 = targets[targets.len() - 2];
    let slot1 = targets[targets.len() - 1];
    if slot0.1 != host.object
        || slot0.2 != host.zone_change_count
        || slot1.1 != model.object
        || slot1.2 != model.zone_change_count
        || slot0.0.checked_add(1) != Some(slot1.0)
        || has_upkeep_between(state, first_target_at + 1, saheeli_at)
        || zone_and_zcc_at(state, source.source, first_target_at)
            != Some((crate::state::Zone::Battlefield, source.zone_change_count))
        || !object_was_named_on_battlefield_at(
            state,
            source.source,
            crate::eternal_cube_c8b_v1::SAHEELI,
            first_target_at,
        )
    {
        return false;
    }
    // Enforce monotonic stack-item allocation order against all spell casts,
    // and enforce sorcery-speed timing (CR 606.3: Saheeli's loyalty ability
    // cannot be activated while a cast spell is still on the stack).
    for (cast_idx, ev) in state.engine.event_history.iter().enumerate() {
        let event::CommittedEvent::PublishedCastIdentityV1 { receipt } = ev else {
            continue;
        };
        if cast_idx < first_target_at {
            if receipt.stack_item_id >= stack_item {
                return false;
            }
            let spell_left_before = state.engine.event_history[cast_idx + 1..first_target_at]
                .iter()
                .any(|later| {
                    matches!(
                        later,
                        event::CommittedEvent::ZoneChange {
                            object,
                            from: crate::state::Zone::Stack,
                            ..
                        } if *object == receipt.spell
                    )
                });
            if !spell_left_before {
                return false;
            }
        } else if cast_idx > first_target_at && receipt.stack_item_id <= stack_item {
            return false;
        }
    }
    // Bind `values` to `model` at `saheeli_at`.
    if !donor_def_and_face_match(
        state,
        model,
        saheeli_at,
        values.card_def,
        values.face_index,
        depth + 1,
    ) {
        return false;
    }
    let Some(mut expected_mods) = authenticated_mods_for_link(state, model, saheeli_at, depth + 1)
    else {
        return false;
    };
    expected_mods.added_artifact = true;
    if values.mods != expected_mods {
        return false;
    }
    // Same-turn live activation check on Saheeli herself.
    if !has_upkeep_between(state, saheeli_at + 1, state.engine.event_history.len())
        && state.step != crate::state::Step::Untap
    {
        if let Some(saheeli_live) = state.objects.try_get(source.source) {
            if saheeli_live.zone == crate::state::Zone::Battlefield
                && saheeli_live.zone_change_count == source.zone_change_count
                && !saheeli_live.v4.ability_uses_this_turn.iter().any(|entry| {
                    entry.ability_kind == crate::state::AbilityKindV4::Activated
                        && entry.ability_index == 0
                        && entry.uses > 0
                })
            {
                return false;
            }
        }
    }
    let host_still_same = state.objects.try_get(host.object).is_some_and(|live| {
        live.zone == crate::state::Zone::Battlefield
            && live.zone_change_count == host.zone_change_count
    });
    if host_still_same && !has_cleanup_boundary_after(state, saheeli_at) {
        if !crate::eternal_cube_c8b_v1::has_saheeli_copy(state, host.object) {
            return false;
        }
        let overwritten_later = state.engine.event_history[saheeli_at + 1..]
            .iter()
            .enumerate()
            .any(|(offset, ev)| {
                let later_at = saheeli_at + 1 + offset;
                let event::CommittedEvent::CopyEntryProvenanceV1 { journal } = ev else {
                    return false;
                };
                let ProvenanceV1::SaheeliApplied {
                    source: ls,
                    stack_item: li,
                    host: lh,
                    model: lm,
                    values: lv,
                } = journal.as_ref()
                else {
                    return false;
                };
                *lh == host
                    && validate_saheeli_provenance(
                        state,
                        later_at,
                        *ls,
                        *li,
                        *lh,
                        *lm,
                        lv,
                        depth + 1,
                    )
            });
        if !overwritten_later
            && !crate::standard_cards_v1::active_aegis_copy(state, host.object)
            && !crate::eternal_cube_c8c_v1::is_absorbing_copy(state, host.object)
            && !crate::eternal_legacy_e7a_v1::active_copy(state, host.object)
        {
            let Some(live_vals) = capture_battlefield_copiable_values(state, host.object) else {
                return false;
            };
            if live_vals.card_def != values.card_def
                || live_vals.mods != values.mods
                || live_vals.prototyped != values.prototyped
                || live_vals.copied_end_step_sacrifices_v1 != values.copied_end_step_sacrifices_v1
                || live_vals.copied_nightmare_v1 != values.copied_nightmare_v1
                || reconstruct_face_at_entry(state, host.object, live_vals.face_index, saheeli_at)
                    != Some(values.face_index)
            {
                return false;
            }
        }
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
    depth: u8,
) -> Option<(usize, &CopiableValuesV1)> {
    if depth > 8 {
        return None;
    }
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
        return validate_saheeli_provenance(
            state,
            at,
            *source,
            *stack_item,
            *host,
            *model,
            values,
            depth,
        )
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
        return validate_saheeli_provenance(
            state,
            at,
            *source,
            *stack_item,
            *host,
            *model,
            values,
            0,
        );
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

    if let Some((_, applied)) = saheeli_applied_at_entry(state, link, before, depth + 1) {
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
            face_index,
            added_artifact,
            stack_item,
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
        if !validate_absorbing_provenance(
            state,
            at,
            *host,
            *donor,
            *until_turn_of,
            *card_def,
            *face_index,
            *added_artifact,
            *stack_item,
            depth,
        ) {
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
        // Each battlefield incarnation enters at most once, so `EntryCopyApplied`
        // must be unique per `host` link across all history.
        let duplicate_entry_copy =
            state
                .engine
                .event_history
                .iter()
                .enumerate()
                .any(|(i, other)| {
                    i != at
                        && matches!(
                            other,
                            event::CommittedEvent::CopyEntryProvenanceV1 { journal }
                                if matches!(
                                    journal.as_ref(),
                                    ProvenanceV1::EntryCopyApplied { host: h, .. } if *h == *host
                                )
                        )
                });
        let copied_def = crate::card_def::CARD_DEFS.get(usize::from(values.card_def))?;
        let host_still_same = live.zone == crate::state::Zone::Battlefield
            && live.zone_change_count == host.zone_change_count;
        if duplicate_entry_copy
            || host.object == donor.object
            || zone_and_zcc_at(state, host.object, at)
                != Some((crate::state::Zone::Battlefield, host.zone_change_count))
            || !crate::standard_cards_v1::had_card_def(
                state,
                host.object,
                host.zone_change_count,
                values.card_def,
            )
            || !copy_mods_structurally_valid(copied_def, values.face_index, &values.mods)
            || (host_still_same
                && !read_state(state)
                    .is_some_and(|c8b| c8b.permanent_copies.iter().any(|s| s.host == *host)))
        {
            return None;
        }
        if link_had_name(state, link, crate::eternal_cube_c8b_v1::PHANTASMAL_IMAGE) {
            if zone_and_zcc_at(state, donor.object, at)
                != Some((crate::state::Zone::Battlefield, donor.zone_change_count))
                || !donor_def_and_face_match(
                    state,
                    *donor,
                    at,
                    values.card_def,
                    values.face_index,
                    depth + 1,
                )
            {
                return None;
            }
            let mut expected = authenticated_mods_for_link(state, *donor, at, depth + 1)?;
            expected.merge_subtypes(&[Subtype::Illusion]);
            expected.illusion_sacrifice = true;
            return (values.mods == expected).then_some(expected);
        }
        if link_had_name(state, link, crate::eternal_cube_c8b_v1::SUPERIOR_SPIDER_MAN) {
            if values.face_index != 0
                || zone_and_zcc_at(state, donor.object, at)
                    != Some((crate::state::Zone::Graveyard, donor.zone_change_count))
                || state
                    .objects
                    .try_get(donor.object)
                    .is_none_or(|d| d.card_def != values.card_def)
                || !crate::eternal_cube_c9c_v1::is_nonbattlefield_creature_def(copied_def)
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
                stack_item,
            } = journal.as_ref()
            else {
                continue;
            };
            if *token != link {
                continue;
            }
            // Each token and each `CreateToken` event can be claimed by at most
            // one `TokenCopyCreated` receipt across all history.
            let duplicate_token_receipt =
                state
                    .engine
                    .event_history
                    .iter()
                    .enumerate()
                    .any(|(i, other)| {
                        i != at
                            && matches!(
                                other,
                                event::CommittedEvent::CopyEntryProvenanceV1 { journal }
                                    if matches!(
                                        journal.as_ref(),
                                        ProvenanceV1::TokenCopyCreated {
                                            token: other_tok,
                                            create_at: other_create,
                                            ..
                                        } if other_tok.object == token.object
                                            || *other_create == *create_at
                                    )
                            )
                    });
            // `stack_item` must be in bounds, not on the live stack, not
            // countered, and must not collide with any non-sibling history item
            // (siblings from the same doubled resolution share producer+donor).
            let stack_item_collision =
                state
                    .engine
                    .event_history
                    .iter()
                    .enumerate()
                    .any(|(i, other)| {
                        if i == at || !event_uses_stack_item(other, *stack_item, false) {
                            return false;
                        }
                        !matches!(
                            other,
                            event::CommittedEvent::CopyEntryProvenanceV1 { journal }
                                if matches!(
                                    journal.as_ref(),
                                    ProvenanceV1::TokenCopyCreated {
                                        producer: other_prod,
                                        donor: other_donor,
                                        stack_item: other_item,
                                        ..
                                    } if *other_item == *stack_item
                                        && *other_prod == *producer
                                        && *other_donor == *donor
                                )
                        )
                    });
            let bad_cast_order = state.engine.event_history[at + 1..].iter().any(|ev| {
                matches!(
                    ev,
                    event::CommittedEvent::PublishedCastIdentityV1 { receipt }
                        if *stack_item >= receipt.stack_item_id
                )
            });
            let broken_create_window = (*create_at + 1..at).any(|idx| {
                matches!(
                    history.get(idx),
                    Some(
                        event::CommittedEvent::PublishedCastIdentityV1 { .. }
                            | event::CommittedEvent::SpellCast { .. }
                            | event::CommittedEvent::UpkeepBegan { .. }
                            | event::CommittedEvent::BeginningEndStep { .. }
                    )
                )
            });
            let copied_def = crate::card_def::CARD_DEFS.get(usize::from(values.card_def))?;
            if duplicate_token_receipt
                || !stack_item_in_bounds(state, *stack_item)
                || stack_item_collision
                || bad_cast_order
                || broken_create_window
                || token.zone_change_count != 0
                || *create_at >= at
                || !copy_mods_structurally_valid(copied_def, values.face_index, &values.mods)
                || !matches!(
                    history.get(*create_at),
                    Some(event::CommittedEvent::CreateToken {
                        object,
                        token_def,
                        controller,
                        ..
                    }) if *object == token.object
                        && *token_def == values.card_def
                        && *controller == producer.controller
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
                if !donor_def_and_face_match(
                    state,
                    *donor,
                    died_at,
                    values.card_def,
                    values.face_index,
                    depth + 1,
                ) {
                    return None;
                }
                let mut expected = authenticated_mods_for_link(state, *donor, died_at, depth + 1)?;
                expected.added_artifact = true;
                return (values.mods == expected).then_some(expected);
            }
            if card_id_by_name(crate::eternal_cube_c8b_v1::SPRINGHEART_NANTUKO)
                == Some(producer.card_def)
            {
                if producer.source == donor.object
                    || zone_and_zcc_at(state, producer.source, *create_at)
                        != Some((crate::state::Zone::Battlefield, producer.zone_change_count))
                    || !object_was_named_on_battlefield_at(
                        state,
                        producer.source,
                        crate::eternal_cube_c8b_v1::SPRINGHEART_NANTUKO,
                        *create_at,
                    )
                    || zone_and_zcc_at(state, donor.object, *create_at)
                        != Some((crate::state::Zone::Battlefield, donor.zone_change_count))
                    || !donor_def_and_face_match(
                        state,
                        *donor,
                        *create_at,
                        values.card_def,
                        values.face_index,
                        depth + 1,
                    )
                {
                    return None;
                }
                // Enforce Springheart Nantuko landfall-trigger conservation:
                // every resolved Springheart landfall trigger either emits a
                // `TokenCopyCreated` (distinct `stack_item`) or creates a
                // `Green Insect Token` (unique to Springheart Nantuko). Check
                // both that this producer itself witnessed a landfall before
                // `*create_at`, and that total Springheart landfall triggers
                // for `producer.controller` are >= token copies + declined
                // Green Insect tokens (both prefix up to `at` and globally).
                let springheart_id =
                    card_id_by_name(crate::eternal_cube_c8b_v1::SPRINGHEART_NANTUKO);
                let insect_id = card_id_by_name(crate::eternal_cube_c8b_v1::GREEN_INSECT_TOKEN);
                let is_controller_landfall = |ev: &event::CommittedEvent| -> bool {
                    match ev {
                        event::CommittedEvent::ZoneChange {
                            object: land,
                            to: crate::state::Zone::Battlefield,
                            controller_before,
                            ..
                        } => state.objects.try_get(*land).is_some_and(|land_live| {
                            let is_land = crate::card_def::CARD_DEFS
                                .get(usize::from(land_live.card_def))
                                .is_some_and(|d| {
                                    d.has_type(crate::card_def::CardType::Land)
                                        || d.transform_face.as_ref().is_some_and(|back| {
                                            back.types.contains(&crate::card_def::CardType::Land)
                                        })
                                });
                            is_land
                                && (*controller_before == producer.controller
                                    || land_live.controller == producer.controller)
                        }),
                        event::CommittedEvent::CreateToken {
                            token_def,
                            controller,
                            ..
                        } => {
                            *controller == producer.controller
                                && crate::card_def::CARD_DEFS
                                    .get(usize::from(*token_def))
                                    .is_some_and(|d| d.has_type(crate::card_def::CardType::Land))
                        }
                        _ => false,
                    }
                };
                let count_springheart_triggers = |end: usize| -> (bool, usize) {
                    let mut producer_saw_landfall = false;
                    let mut triggers: usize = 0;
                    for (lf_idx, ev) in state.engine.event_history[..end].iter().enumerate() {
                        if !is_controller_landfall(ev) {
                            continue;
                        }
                        if zone_and_zcc_at(state, producer.source, lf_idx)
                            == Some((crate::state::Zone::Battlefield, producer.zone_change_count))
                        {
                            producer_saw_landfall = true;
                        }
                        let active_springhearts = state
                            .objects
                            .iter()
                            .filter(|&(obj_id, obj_live)| {
                                (obj_live.controller == producer.controller
                                    || obj_live.owner == producer.controller)
                                    && object_was_named_on_battlefield_at(
                                        state,
                                        obj_id,
                                        crate::eternal_cube_c8b_v1::SPRINGHEART_NANTUKO,
                                        lf_idx,
                                    )
                            })
                            .count();
                        triggers = triggers.saturating_add(active_springhearts);
                    }
                    (producer_saw_landfall, triggers)
                };
                let count_springheart_outcomes =
                    |copy_end: usize, insect_end: usize, countered_end: usize| -> usize {
                        let mut copy_items = std::collections::BTreeSet::new();
                        for ev in &state.engine.event_history[..copy_end] {
                            if let event::CommittedEvent::CopyEntryProvenanceV1 { journal } = ev {
                                if let ProvenanceV1::TokenCopyCreated {
                                    producer: p,
                                    stack_item: item,
                                    ..
                                } = journal.as_ref()
                                {
                                    if Some(p.card_def) == springheart_id
                                        && p.controller == producer.controller
                                    {
                                        copy_items.insert(*item);
                                    }
                                }
                            }
                        }
                        let mut countered_items = std::collections::BTreeSet::new();
                        for ev in &state.engine.event_history[..countered_end] {
                            if let event::CommittedEvent::CopyEntryProvenanceV1 { journal } = ev {
                                if let ProvenanceV1::AbilityCountered {
                                    stack_item: item,
                                    kind:
                                        CounteredAbilityKindV1::SpringheartLandfall { producer: p },
                                } = journal.as_ref()
                                {
                                    if Some(p.card_def) == springheart_id
                                        && p.controller == producer.controller
                                    {
                                        countered_items.insert(*item);
                                    }
                                }
                            }
                        }
                        let mut insect_triggers: usize = 0;
                        let mut prev_was_insect = false;
                        for ev in &state.engine.event_history[..insect_end] {
                            let is_insect = matches!(
                                ev,
                                event::CommittedEvent::CreateToken {
                                    token_def,
                                    controller,
                                    ..
                                } if Some(*token_def) == insect_id
                                    && *controller == producer.controller
                            );
                            if is_insect && !prev_was_insect {
                                insect_triggers = insect_triggers.saturating_add(1);
                            }
                            prev_was_insect = is_insect;
                        }
                        copy_items
                            .len()
                            .saturating_add(insect_triggers)
                            .saturating_add(countered_items.len())
                    };
                let (producer_saw_landfall, prefix_triggers) =
                    count_springheart_triggers(*create_at);
                let prefix_outcomes = count_springheart_outcomes(at + 1, *create_at, *create_at);
                let full_len = state.engine.event_history.len();
                let (_, total_triggers) = count_springheart_triggers(full_len);
                let total_outcomes = count_springheart_outcomes(full_len, full_len, full_len);
                if !producer_saw_landfall
                    || prefix_triggers < prefix_outcomes
                    || total_triggers < total_outcomes
                {
                    return None;
                }
                // Require Springheart Nantuko to be attached to `donor` (either live
                // on the same incarnation, or via a committed Bestow cast targeting
                // exact `donor` before `*create_at`).
                let producer_live = state.objects.try_get(producer.source)?;
                let donor_live = state.objects.try_get(donor.object)?;
                let producer_same = producer_live.zone == crate::state::Zone::Battlefield
                    && producer_live.zone_change_count == producer.zone_change_count;
                let donor_same = donor_live.zone == crate::state::Zone::Battlefield
                    && donor_live.zone_change_count == donor.zone_change_count;
                let live_attached = producer_same && producer_live.v4.attached_to == Some(*donor);
                if producer_same
                    && donor_same
                    && crate::engine::object_has_type(
                        state,
                        donor.object,
                        crate::card_def::CardType::Creature,
                    )
                    && !live_attached
                {
                    return None;
                }
                let bestow_cast_attached =
                    history[..*create_at]
                        .iter()
                        .enumerate()
                        .any(|(cast_at, ev)| {
                            let event::CommittedEvent::PublishedCastIdentityV1 { receipt } = ev
                            else {
                                return false;
                            };
                            if receipt.spell != producer.source
                                || receipt.stack_zone_change_count.checked_add(1)
                                    != Some(producer.zone_change_count)
                            {
                                return false;
                            }
                            history[cast_at + 1..*create_at].iter().any(|t_ev| {
                                matches!(
                                    t_ev,
                                    event::CommittedEvent::Targeted {
                                        target,
                                        target_zone_change_count,
                                        targeting_stack_item,
                                        ..
                                    } if *targeting_stack_item == receipt.stack_item_id
                                        && *target == donor.object
                                        && *target_zone_change_count == donor.zone_change_count
                                )
                            })
                        });
                if !live_attached && !bestow_cast_attached {
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

fn animation_timestamp_valid_and_unique(
    state: &GameState,
    at: usize,
    host_link: ObjectLinkV4,
    timestamp: u64,
) -> bool {
    if timestamp >= state.engine.next_effect_timestamp {
        return false;
    }
    // Enforce uniqueness and chronological monotonicity across all `LandAnimated` receipts.
    for (i, ev) in state.engine.event_history.iter().enumerate() {
        if i == at {
            continue;
        }
        let event::CommittedEvent::CopyEntryProvenanceV1 { journal } = ev else {
            continue;
        };
        let ProvenanceV1::LandAnimated {
            animation_timestamp: other_ts,
            ..
        } = journal.as_ref()
        else {
            continue;
        };
        if *other_ts == timestamp
            || (i < at && *other_ts > timestamp)
            || (i > at && *other_ts < timestamp)
        {
            return false;
        }
    }
    // Must not collide with another permanent's live animation timestamp or any
    // live `until_end_of_turn` continuous-effect timestamp.
    if state
        .objects
        .iter()
        .any(|(id, o)| id != host_link.object && o.v4.animation_timestamp == Some(timestamp))
    {
        return false;
    }
    if state.engine.until_end_of_turn.iter().any(|eff| match eff {
        crate::engine::UntilEndOfTurnEffect::ResolvedSetEffect { timestamp: t, .. }
        | crate::engine::UntilEndOfTurnEffect::ResolvedObjectEffect { timestamp: t, .. }
        | crate::engine::UntilEndOfTurnEffect::ResolvedObjectKeywordEffect {
            timestamp: t, ..
        } => *t == timestamp,
        _ => false,
    }) {
        return false;
    }
    true
}

/// Authenticates that `donor` actually resolved `AnimateSource` or
/// `AnimateSourcePermanentlyV1` on this exact battlefield incarnation before
/// `entry` (and in the same turn for until-end-of-turn animations), bound to
/// an in-bounds unique `stack_item` and `animation_timestamp`. A card
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
                stack_item,
                animation_timestamp,
            } = journal.as_ref()
            else {
                return false;
            };
            if *host != host_link
                || *card_def != donor.card_def
                || *until_end_of_turn != anim.until_end_of_turn
                || !stack_item_in_bounds(state, *stack_item)
                || stack_item_collides_in_history(state, *stack_item, at, false)
                || !animation_timestamp_valid_and_unique(state, at, host_link, *animation_timestamp)
            {
                return false;
            }
            // Enforce sound stack-item allocation ordering without rejecting
            // legal instant-speed LIFO stacking:
            // 1. A LandAnimated that already resolved at `at` was allocated
            //    before any spell cast at `cast_idx > at`.
            // 2. If a spell cast at `cast_idx < at` is still on the stack at
            //    `at` (leaves at `leave_idx > at`), LIFO requires `stack_item`
            //    to sit above it (`*stack_item > receipt.stack_item_id`).
            // 3. If that spell resolves across `entry` (`at < entry < leave_idx`),
            //    any later `LandAnimated` at `later_at > leave_idx` could not
            //    have been allocated between the spell cast and `stack_item`
            //    (`receipt.stack_item_id < later_item < *stack_item` is
            //    impossible under LIFO).
            // 4. Any later `LandAnimated` separated by an upkeep or end-step
            //    boundary was allocated after an empty stack (`*stack_item < later_item`).
            let full_history = &state.engine.event_history;
            for (cast_idx, hist_ev) in full_history.iter().enumerate() {
                let event::CommittedEvent::PublishedCastIdentityV1 { receipt } = hist_ev else {
                    continue;
                };
                if cast_idx > at {
                    if *stack_item >= receipt.stack_item_id {
                        return false;
                    }
                } else if cast_idx < at {
                    let leave_idx = full_history[cast_idx + 1..]
                        .iter()
                        .position(|later| {
                            matches!(
                                later,
                                event::CommittedEvent::ZoneChange {
                                    object,
                                    from: crate::state::Zone::Stack,
                                    ..
                                } if *object == receipt.spell
                            )
                        })
                        .map_or(full_history.len(), |off| cast_idx + 1 + off);
                    if at < leave_idx {
                        if *stack_item <= receipt.stack_item_id {
                            return false;
                        }
                        if entry < leave_idx {
                            let lifo_inversion =
                                full_history[leave_idx + 1..].iter().any(|later| {
                                    matches!(
                                        later,
                                        event::CommittedEvent::CopyEntryProvenanceV1 { journal }
                                            if matches!(
                                                journal.as_ref(),
                                                ProvenanceV1::LandAnimated {
                                                    stack_item: later_item,
                                                    ..
                                                } if *later_item > receipt.stack_item_id
                                                    && *later_item < *stack_item
                                            )
                                    )
                                });
                            if lifo_inversion {
                                return false;
                            }
                        }
                    }
                }
            }
            for (offset, later_ev) in full_history[at + 1..].iter().enumerate() {
                let later_at = at + 1 + offset;
                let event::CommittedEvent::CopyEntryProvenanceV1 { journal } = later_ev else {
                    continue;
                };
                let ProvenanceV1::LandAnimated {
                    stack_item: later_item,
                    ..
                } = journal.as_ref()
                else {
                    continue;
                };
                let stack_emptied_between = full_history[at + 1..later_at].iter().any(|mid| {
                    matches!(
                        mid,
                        event::CommittedEvent::UpkeepBegan { .. }
                            | event::CommittedEvent::BeginningEndStep { .. }
                    )
                });
                if stack_emptied_between && *stack_item >= *later_item {
                    return false;
                }
            }
            let latest_later_ts =
                state.engine.event_history[at + 1..]
                    .iter()
                    .rev()
                    .find_map(|later_ev| {
                        let event::CommittedEvent::CopyEntryProvenanceV1 { journal } = later_ev
                        else {
                            return None;
                        };
                        match journal.as_ref() {
                            ProvenanceV1::LandAnimated {
                                host: h,
                                animation_timestamp: ts,
                                ..
                            } if *h == host_link => Some(*ts),
                            _ => None,
                        }
                    });
            let expected_live_ts = latest_later_ts.unwrap_or(*animation_timestamp);
            if *until_end_of_turn {
                if has_upkeep_between(state, at + 1, entry) {
                    return false;
                }
                if !can_have_expired_eot
                    && state
                        .objects
                        .try_get(donor.source)
                        .is_none_or(|live| live.v4.animation_timestamp != Some(expected_live_ts))
                {
                    return false;
                }
            } else if donor_same_incarnation
                && state
                    .objects
                    .try_get(donor.source)
                    .is_none_or(|live| live.v4.animation_timestamp != Some(expected_live_ts))
            {
                return false;
            }
            if !has_upkeep_between(state, at + 1, state.engine.event_history.len())
                && state.step != crate::state::Step::Untap
            {
                let (turn_start, _) = turn_window_containing(state, at);
                let same_turn_consumed = state.engine.event_history[turn_start..]
                    .iter()
                    .filter(|ev| {
                        matches!(
                            ev,
                            event::CommittedEvent::CopyEntryProvenanceV1 { journal }
                                if matches!(
                                    journal.as_ref(),
                                    ProvenanceV1::LandAnimated { host: h, .. } if *h == host_link
                                ) || matches!(
                                    journal.as_ref(),
                                    ProvenanceV1::AbilityCountered {
                                        kind: CounteredAbilityKindV1::LandAnimate { host: h },
                                        ..
                                    } if *h == host_link
                                )
                        )
                    })
                    .count();
                let has_animate_ability = def.activated_abilities.iter().any(|a| {
                    matches!(
                        (a.effect)(),
                        crate::effect::EffectOp::AnimateSource
                            | crate::effect::EffectOp::AnimateSourcePermanentlyV1
                    )
                });
                if has_animate_ability {
                    if donor_same_incarnation {
                        if state.objects.try_get(donor.source).is_none_or(|live| {
                            if live.card_def != *card_def {
                                return false;
                            }
                            let total_uses =
                                crate::eternal_sideboard_s1b_v1::meddling_mage_entry::animate_ability_uses_for_object(
                                    live, *card_def,
                                ) as usize;
                            total_uses < same_turn_consumed
                        }) {
                            return false;
                        }
                    } else {
                        let departed_uses =
                            crate::eternal_sideboard_s1b_v1::departed_animate_uses_for(
                                state, host_link,
                            ) as usize;
                        if departed_uses < same_turn_consumed {
                            return false;
                        }
                    }
                }
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
    // incarnation in the same turn before `entry`.
    let history = &state.engine.event_history[..entry];
    if history.iter().enumerate().any(|(tap_at, ev)| {
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
    }) {
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

    if types.contains(&CardType::Land) {
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
///
/// Threat-model boundary: snapshots are produced and stored by our own pinned
/// code, so stateless restore validation guards against corruption and engine
/// bugs, not an adversary. Direct edits of `S1bStateV1` departed-use counters
/// (`departed_animate_uses`, `departed_discard_uses`,
/// `departed_effect_discard_uses`) that produce a state
/// byte-for-byte identical to a legal alternative game are out of scope for
/// stateless restore validation; integrity of stored snapshots rests on the
/// record hashes in manifests and receipts.
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
    let saheeli_at_entry = saheeli_applied_at_entry(state, donor_link, entry, 0);
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
        if !donor_def_and_face_match(
            state,
            donor_link,
            entry,
            copy.copied.card_def,
            face_index,
            0,
        ) {
            return false;
        }
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
                // resolution. Compare unconditionally against its saved
                // pre-Saheeli definition, modifications, and face unless an
                // Absorbing Man copy covered the donor.
                if !has_valid_post_entry_saheeli(state, donor_link, entry) {
                    return false;
                }
                let Some(saved) = read_state(state)
                    .and_then(|c8b| c8b.saheeli_copies.iter().find(|c| c.host == donor_link))
                else {
                    return false;
                };
                if !crate::eternal_cube_c8c_v1::active_copy(state, copy.copied.source)
                    && !has_unexpired_absorbing_before(state, donor_link, entry)
                {
                    if saved.card_def != copy.copied.card_def {
                        return false;
                    }
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
        // Never allow a live post-entry land animation (e.g. Mishra's Factory
        // activated after Metamorph entered) to bypass `donor_land_animated_at_entry`.
        let animated_live = donor_same_incarnation
            && def.animation.is_some()
            && donor_live.v4.animation_timestamp.is_some();
        let live_eligible = !animated_live
            && donor_same_incarnation
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
