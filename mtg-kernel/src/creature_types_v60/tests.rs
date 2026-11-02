use super::*;
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::put;
use crate::state::{FaceDownV1, Zone};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

#[test]
fn pinned_legal_domain_is_complete_unique_and_index_stable() {
    let input: serde_json::Value = serde_json::from_str(DOMAIN_INPUT_V60).unwrap();
    assert_eq!(input["schema"], DOMAIN_SCHEMA_V60);
    assert_eq!(input["rules_effective_date"], "2026-09-25");
    assert_eq!(input["rule"], "205.3m");
    assert_eq!(
        input["rules_sha256"],
        "8d860e451f20f38865b725b42d82feb714c725373dd8f3b32b8652b3eeb070ca"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(DOMAIN_INPUT_V60.as_bytes())),
        DOMAIN_SHA256_V60
    );
    let entries = input["types"].as_array().unwrap();
    assert_eq!(entries.len(), 324);
    assert_eq!(CreatureTypeV60::ALL.len(), entries.len());
    let mut names = BTreeSet::new();
    for (index, (&kind, entry)) in CreatureTypeV60::ALL.iter().zip(entries).enumerate() {
        let id = u16::try_from(index).unwrap();
        assert_eq!(entry["id"], index);
        assert_eq!(entry["name"], kind.canonical_name());
        assert_eq!(kind.choice_index(), id);
        assert_eq!(CreatureTypeV60::from_choice_index(id), Some(kind));
        assert_eq!(
            CreatureTypeV60::from_canonical_name(kind.canonical_name()),
            Some(kind)
        );
        assert!(names.insert(kind.canonical_name()));
        assert_eq!(
            serde_json::from_str::<CreatureTypeV60>(&serde_json::to_string(&kind).unwrap())
                .unwrap(),
            kind
        );
    }
    assert_eq!(CreatureTypeV60::TimeLord.choice_index(), 323);
    assert_eq!(CreatureTypeV60::TimeLord.canonical_name(), "Time Lord");
    assert_eq!(
        CreatureTypeV60::AssemblyWorker.canonical_name(),
        "Assembly-Worker"
    );
    assert_eq!(CreatureTypeV60::Ctan.canonical_name(), "C’tan");
    assert_eq!(CreatureTypeV60::Shiar.canonical_name(), "Shi’ar");
    assert!(CreatureTypeV60::from_choice_index(324).is_none());
    assert!(CreatureTypeV60::from_choice_index(u16::MAX).is_none());
}

#[test]
fn every_native_subtype_preserves_its_id_and_canonical_case_alias() {
    let mut canonical = BTreeSet::new();
    let mut noncreature = 0;
    for id in 0..=Subtype::Atog.stable_id() {
        let native = native_subtype(id).expect("all native IDs have explicit mappings");
        assert_eq!(native.stable_id(), id);
        assert_eq!(
            native.is_native_creature_type(),
            canonical_native_type(native).is_some()
        );
        if let Some(kind) = canonical_native_type(native) {
            canonical.insert(kind);
        } else {
            noncreature += 1;
        }
    }
    assert_eq!(Subtype::Drake.stable_id(), 153);
    assert_eq!(Subtype::Sphinx.stable_id(), 170);
    assert_eq!(native_subtype(170), Some(Subtype::Sphinx));
    assert_eq!(
        canonical_native_type(Subtype::Sphinx),
        Some(CreatureTypeV60::Sphinx)
    );
    assert_eq!(CreatureTypeV60::Sphinx.choice_index(), 271);
    assert!(!Subtype::CREATURE_TYPES.contains(&Subtype::Sphinx));
    assert!(!Subtype::Sphinx.is_creature_type());
    assert_eq!(Subtype::Vivien.stable_id(), 194);
    assert_eq!(native_subtype(194), Some(Subtype::Vivien));
    assert_eq!(canonical_native_type(Subtype::Vivien), None);
    assert_eq!(noncreature, 51);
    assert_eq!(Subtype::Ogre.stable_id(), 195);
    assert_eq!(
        canonical_native_type(Subtype::Ogre),
        Some(CreatureTypeV60::Ogre)
    );
    for (subtype, id, kind) in [
        (Subtype::Dauthi, 196, CreatureTypeV60::Dauthi),
        (Subtype::Ally, 197, CreatureTypeV60::Ally),
        (Subtype::Fungus, 198, CreatureTypeV60::Fungus),
        (Subtype::Barbarian, 199, CreatureTypeV60::Barbarian),
        (Subtype::Villain, 200, CreatureTypeV60::Villain),
        (Subtype::Nomad, 201, CreatureTypeV60::Nomad),
        (Subtype::Scarecrow, 202, CreatureTypeV60::Scarecrow),
        (Subtype::Peasant, 203, CreatureTypeV60::Peasant),
        (Subtype::Atog, 204, CreatureTypeV60::Atog),
    ] {
        assert_eq!(subtype.stable_id(), id);
        assert_eq!(canonical_native_type(subtype), Some(kind));
    }
    assert_eq!(canonical.len(), 148);
    assert!(native_subtype(205).is_none());
    assert!(Subtype::from_stable_id(205).is_none());
    assert!(Subtype::from_stable_id(u16::MAX).is_none());
    for (alias, normal, expected) in [
        (Subtype::BirdAllCaps, Subtype::Bird, CreatureTypeV60::Bird),
        (
            Subtype::FaerieAllCaps,
            Subtype::Faerie,
            CreatureTypeV60::Faerie,
        ),
        (
            Subtype::HumanAllCaps,
            Subtype::Human,
            CreatureTypeV60::Human,
        ),
        (
            Subtype::NinjaAllCaps,
            Subtype::Ninja,
            CreatureTypeV60::Ninja,
        ),
        (
            Subtype::RogueAllCaps,
            Subtype::Rogue,
            CreatureTypeV60::Rogue,
        ),
        (
            Subtype::WizardAllCaps,
            Subtype::Wizard,
            CreatureTypeV60::Wizard,
        ),
    ] {
        assert_ne!(alias.stable_id(), normal.stable_id());
        assert_eq!(canonical_native_type(alias), Some(expected));
        assert_eq!(canonical_native_type(normal), Some(expected));
    }
    for &native in Subtype::CREATURE_TYPES {
        assert!(canonical_native_type(native).is_some());
    }
    assert_eq!(
        canonical_native_type(Subtype::AssemblyWorker),
        Some(CreatureTypeV60::AssemblyWorker)
    );
    assert_eq!(
        canonical_native_type(Subtype::Bear),
        Some(CreatureTypeV60::Bear)
    );
    assert_eq!(canonical_native_type(Subtype::Aura), None);
    assert!(!canonical.contains(&CreatureTypeV60::TimeLord));
}

#[test]
fn public_choice_roundtrips_and_rejects_noncanonical_or_invalid_data() {
    for &kind in CreatureTypeV60::ALL {
        let choice = CreatureTypeChoiceV60::new(kind);
        let wire = serde_json::to_value(choice).unwrap();
        assert_eq!(wire["domain_sha256"], DOMAIN_SHA256_V60);
        assert_eq!(wire["chosen"], kind.canonical_name());
        assert_eq!(
            serde_json::from_value::<CreatureTypeChoiceV60>(wire.clone()).unwrap(),
            choice
        );
        for (field, bad) in [
            ("schema", "old"),
            ("domain_sha256", "wrong"),
            ("chosen", "Aura"),
        ] {
            let mut changed = wire.clone();
            changed[field] = bad.into();
            assert!(serde_json::from_value::<CreatureTypeChoiceV60>(changed).is_err());
        }
        let mut extra = wire;
        extra["hidden_pool"] = serde_json::json!([]);
        assert!(serde_json::from_value::<CreatureTypeChoiceV60>(extra).is_err());
    }
    for invalid in [
        "",
        "TimeLord",
        "Time lord",
        "Time",
        "Lord",
        "BIRD",
        "C'tan",
        "Food",
        "Vehicle",
        "NotAType",
    ] {
        assert!(CreatureTypeV60::from_canonical_name(invalid).is_none());
        assert!(serde_json::from_value::<CreatureTypeV60>(serde_json::json!(invalid)).is_err());
    }
    for invalid in [
        serde_json::json!(0),
        serde_json::json!({"TimeLord":null}),
        serde_json::Value::Null,
    ] {
        assert!(serde_json::from_value::<CreatureTypeV60>(invalid).is_err());
    }
}

#[test]
fn represented_creature_and_changeling_match_all_legal_types_without_pool_lookup() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = crate::extra_turn_v1::tests::ready(actor);
        let ordinary = put(&mut state, actor, "Tolarian Terror", Zone::Battlefield);
        let changeling = put(&mut state, actor, "Masked Vandal", Zone::Battlefield);
        let land = put(&mut state, actor, "Forest", Zone::Battlefield);
        assert_eq!(
            matches_effective_creature(&state, ordinary, CreatureTypeV60::Serpent),
            Ok(true)
        );
        assert_eq!(
            matches_effective_creature(&state, ordinary, CreatureTypeV60::TimeLord),
            Ok(false)
        );
        let before = state.state_hash();
        for &kind in CreatureTypeV60::ALL {
            assert_eq!(
                matches_effective_creature(&state, changeling, kind),
                Ok(true)
            );
            assert_eq!(matches_effective_creature(&state, land, kind), Ok(false));
        }
        assert_eq!(state.state_hash(), before);
        let snapshot = state.snapshot();
        state.objects.get_mut(changeling).v4.face_down_v1 = Some(FaceDownV1 {
            disguised: true,
            lookers: 0,
            hidden_by: None,
            hidden_by_source: None,
        });
        assert_eq!(
            matches_effective_creature(&state, changeling, CreatureTypeV60::TimeLord),
            Ok(false)
        );
        assert_eq!(
            matches_effective_creature(&state, changeling, CreatureTypeV60::Shapeshifter),
            Ok(false)
        );
        state.restore(&snapshot);
        assert_eq!(state.state_hash(), before);
        assert_eq!(
            matches_effective_creature(&state, changeling, CreatureTypeV60::TimeLord),
            Ok(true)
        );
        let restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(restored.state_hash(), before);
        assert_eq!(
            matches_effective_creature(&restored, changeling, CreatureTypeV60::TimeLord),
            Ok(true)
        );
    }
}

#[test]
fn invalid_native_effective_subtype_refuses_readonly() {
    let mut state = crate::extra_turn_v1::tests::ready(PlayerId::P0);
    let source = put(&mut state, PlayerId::P0, "Masked Vandal", Zone::Battlefield);
    state
        .objects
        .get_mut(source)
        .v4
        .effective_subtype_ids
        .push(u16::MAX);
    let before = state.state_hash();
    assert_eq!(
        matches_effective_creature(&state, source, CreatureTypeV60::TimeLord),
        Err(CreatureTypeMatchErrorV60::UnknownNativeSubtype(u16::MAX))
    );
    assert_eq!(
        matches_effective_creature(&state, ObjectId(u32::MAX), CreatureTypeV60::TimeLord),
        Err(CreatureTypeMatchErrorV60::MissingObject)
    );
    assert_eq!(state.state_hash(), before);
}

#[test]
fn layer_six_ability_suppression_keeps_changeling_layer_four_types() {
    let mut state = crate::extra_turn_v1::tests::ready(PlayerId::P0);
    let source = put(&mut state, PlayerId::P0, "Masked Vandal", Zone::Battlefield);
    let suppressor = put(
        &mut state,
        PlayerId::P1,
        "Tolarian Terror",
        Zone::Battlefield,
    );
    let link = crate::state::ObjectLinkV4 {
        object: suppressor,
        zone_change_count: state.objects.get(suppressor).zone_change_count,
    };
    state.objects.get_mut(source).v4.creature_upgrade =
        Some(crate::standard_creatures_v1::CreatureUpgradeV1 {
            suppressed_by: vec![crate::standard_creatures_v1::TidebinderSuppressionV1 {
                source: link,
                timestamp: 8,
            }],
            ..Default::default()
        });
    assert!(!crate::continuous_characteristics_v1::printed_abilities_active(&state, source));
    for &kind in CreatureTypeV60::ALL {
        assert_eq!(matches_effective_creature(&state, source, kind), Ok(true));
    }
}

#[cfg(feature = "limited-fdn-fixtures")]
#[test]
fn later_creature_type_replacement_suppresses_printed_changeling() {
    let mut state = crate::extra_turn_v1::tests::ready(PlayerId::P0);
    let source = put(&mut state, PlayerId::P0, "Masked Vandal", Zone::Battlefield);
    state.objects.get_mut(source).v4.creature_upgrade =
        Some(crate::standard_creatures_v1::CreatureUpgradeV1 {
            creature_types: Some((vec![Subtype::Human.stable_id()], 7)),
            ..Default::default()
        });
    assert_eq!(
        matches_effective_creature(&state, source, CreatureTypeV60::Human),
        Ok(true)
    );
    assert_eq!(
        matches_effective_creature(&state, source, CreatureTypeV60::TimeLord),
        Ok(false)
    );
}

#[cfg(feature = "limited-fdn-fixtures")]
#[test]
fn genuine_omitted_creature_types_replace_exactly_and_later_additions_restore() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for (name, native_kind, kind) in [
            (
                "Juggernaut",
                Subtype::Juggernaut,
                CreatureTypeV60::Juggernaut,
            ),
            (
                "Gorehorn Raider",
                Subtype::Minotaur,
                CreatureTypeV60::Minotaur,
            ),
            ("Fiendish Panda", Subtype::Bear, CreatureTypeV60::Bear),
        ] {
            let mut state = crate::extra_turn_v1::tests::ready(actor);
            let source = put(&mut state, actor, name, Zone::Battlefield);
            assert!(crate::engine::has_effective_subtype(
                &state,
                source,
                native_kind
            ));
            assert_eq!(matches_effective_creature(&state, source, kind), Ok(true));
            let original = state.snapshot();
            state.objects.get_mut(source).v4.creature_upgrade =
                Some(crate::standard_creatures_v1::CreatureUpgradeV1 {
                    creature_types: Some((vec![Subtype::Human.stable_id()], 7)),
                    ..Default::default()
                });
            assert_eq!(
                crate::standard_creatures_v1::creature_types(&state, source),
                Some((vec![Subtype::Human.stable_id()], 7))
            );
            assert_eq!(
                crate::engine::effective_subtype_ids(&state, source),
                vec![Subtype::Human.stable_id()]
            );
            assert!(!crate::engine::has_effective_subtype(
                &state,
                source,
                native_kind
            ));
            assert_eq!(matches_effective_creature(&state, source, kind), Ok(false));
            assert_eq!(
                matches_effective_creature(&state, source, CreatureTypeV60::Human),
                Ok(true)
            );
            assert_eq!(
                matches_effective_creature(&state, source, CreatureTypeV60::TimeLord),
                Ok(false)
            );
            let replaced = state.snapshot();
            let restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            assert_eq!(restored.state_hash(), state.state_hash());
            assert_eq!(
                matches_effective_creature(&restored, source, kind),
                Ok(false)
            );
            state.objects.get_mut(source).v4.added_subtype_v1 = Some((native_kind, 6));
            assert_eq!(matches_effective_creature(&state, source, kind), Ok(false));
            state.objects.get_mut(source).v4.added_subtype_v1 = Some((native_kind, 8));
            assert!(crate::engine::has_effective_subtype(
                &state,
                source,
                native_kind
            ));
            assert_eq!(matches_effective_creature(&state, source, kind), Ok(true));
            state.restore(&replaced);
            assert_eq!(matches_effective_creature(&state, source, kind), Ok(false));
            state.restore(&original);
            assert!(crate::engine::has_effective_subtype(
                &state,
                source,
                native_kind
            ));
            assert_eq!(matches_effective_creature(&state, source, kind), Ok(true));
            assert_eq!(
                matches_effective_creature(&state, source, CreatureTypeV60::Human),
                Ok(false)
            );
        }
    }
}

#[test]
fn replacement_removes_both_native_case_aliases_preserves_noncreature_and_unknown_ids() {
    // Typed native-state composition fixtures, not claims of paid card play.
    for (alias, normal) in [
        (Subtype::BirdAllCaps, Subtype::Bird),
        (Subtype::FaerieAllCaps, Subtype::Faerie),
        (Subtype::HumanAllCaps, Subtype::Human),
        (Subtype::NinjaAllCaps, Subtype::Ninja),
        (Subtype::RogueAllCaps, Subtype::Rogue),
        (Subtype::WizardAllCaps, Subtype::Wizard),
    ] {
        let mut state = crate::extra_turn_v1::tests::ready(PlayerId::P0);
        let source = put(
            &mut state,
            PlayerId::P0,
            "Tolarian Terror",
            Zone::Battlefield,
        );
        let object = state.objects.get_mut(source);
        object.v4.effective_subtype_ids = vec![
            alias.stable_id(),
            normal.stable_id(),
            Subtype::Equipment.stable_id(),
            Subtype::Aura.stable_id(),
            Subtype::Forest.stable_id(),
        ];
        object.v4.creature_upgrade = Some(crate::standard_creatures_v1::CreatureUpgradeV1 {
            creature_types: Some((vec![Subtype::Goblin.stable_id()], 7)),
            ..Default::default()
        });
        let expected = vec![
            Subtype::Equipment.stable_id(),
            Subtype::Aura.stable_id(),
            Subtype::Forest.stable_id(),
            Subtype::Goblin.stable_id(),
        ];
        assert_eq!(
            crate::standard_creatures_v1::creature_types(&state, source),
            Some((expected.clone(), 7))
        );
        assert!(!crate::engine::has_effective_subtype(&state, source, alias));
        assert!(!crate::engine::has_effective_subtype(
            &state, source, normal
        ));
        for noncreature in [Subtype::Equipment, Subtype::Aura, Subtype::Forest] {
            assert!(crate::engine::has_effective_subtype(
                &state,
                source,
                noncreature
            ));
        }
        assert_eq!(
            matches_effective_creature(&state, source, canonical_native_type(alias).unwrap()),
            Ok(false)
        );
        assert_eq!(
            matches_effective_creature(&state, source, CreatureTypeV60::Goblin),
            Ok(true)
        );
        state
            .objects
            .get_mut(source)
            .v4
            .effective_subtype_ids
            .push(u16::MAX);
        let mut with_unknown = expected;
        with_unknown.insert(3, u16::MAX);
        assert_eq!(
            crate::standard_creatures_v1::creature_types(&state, source),
            Some((with_unknown, 7))
        );
        let before = state.state_hash();
        assert_eq!(
            matches_effective_creature(&state, source, CreatureTypeV60::Goblin),
            Err(CreatureTypeMatchErrorV60::UnknownNativeSubtype(u16::MAX))
        );
        assert_eq!(state.state_hash(), before);
        let restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(
            matches_effective_creature(&restored, source, CreatureTypeV60::Goblin),
            Err(CreatureTypeMatchErrorV60::UnknownNativeSubtype(u16::MAX))
        );
    }
}

#[cfg(feature = "limited-fdn-fixtures")]
#[test]
fn newer_aura_override_supersedes_typed_replacement_and_older_override_does_not() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = crate::extra_turn_v1::tests::ready(actor);
        let source = put(&mut state, actor, "Juggernaut", Zone::Battlefield);
        state.objects.get_mut(source).v4.creature_upgrade =
            Some(crate::standard_creatures_v1::CreatureUpgradeV1 {
                creature_types: Some((vec![Subtype::Human.stable_id()], 7)),
                ..Default::default()
            });
        let aura = put(&mut state, actor, "Witness Protection", Zone::Battlefield);
        let host = crate::state::ObjectLinkV4 {
            object: source,
            zone_change_count: state.objects.get(source).zone_change_count,
        };
        state.objects.get_mut(source).attachments.push(aura);
        state.objects.get_mut(aura).v4.attached_to = Some(host);
        state.objects.get_mut(aura).v4.layer_timestamp = Some(8);
        assert!(crate::standard_creatures_v1::creature_types(&state, source).is_none());
        assert_eq!(
            crate::engine::effective_subtype_ids(&state, source),
            vec![Subtype::Citizen.stable_id()]
        );
        assert_eq!(
            matches_effective_creature(&state, source, CreatureTypeV60::Citizen),
            Ok(true)
        );
        assert_eq!(
            matches_effective_creature(&state, source, CreatureTypeV60::Human),
            Ok(false)
        );
        assert_eq!(
            matches_effective_creature(&state, source, CreatureTypeV60::Juggernaut),
            Ok(false)
        );
        let snapshot = state.snapshot();
        state.objects.get_mut(aura).v4.layer_timestamp = Some(6);
        assert_eq!(
            crate::standard_creatures_v1::creature_types(&state, source),
            Some((vec![Subtype::Human.stable_id()], 7))
        );
        assert_eq!(
            crate::engine::effective_subtype_ids(&state, source),
            vec![Subtype::Human.stable_id()]
        );
        assert_eq!(
            matches_effective_creature(&state, source, CreatureTypeV60::Human),
            Ok(true)
        );
        assert_eq!(
            matches_effective_creature(&state, source, CreatureTypeV60::Citizen),
            Ok(false)
        );
        state.restore(&snapshot);
        assert_eq!(
            matches_effective_creature(&state, source, CreatureTypeV60::Citizen),
            Ok(true)
        );
        assert_eq!(
            matches_effective_creature(&state, source, CreatureTypeV60::Human),
            Ok(false)
        );
    }
}

#[cfg(feature = "limited-fdn-fixtures")]
#[test]
fn animation_type_composition_refuses_without_universal_set_provenance_and_restores() {
    let mut state = crate::extra_turn_v1::tests::ready(PlayerId::P0);
    let source = put(
        &mut state,
        PlayerId::P0,
        "Soulstone Sanctuary",
        Zone::Battlefield,
    );
    assert_eq!(
        matches_effective_creature(&state, source, CreatureTypeV60::TimeLord),
        Ok(false)
    );
    let snapshot = state.snapshot();
    state.objects.get_mut(source).v4.animation_timestamp = Some(9);
    for &kind in CreatureTypeV60::ALL {
        assert_eq!(
            matches_effective_creature(&state, source, kind),
            Err(CreatureTypeMatchErrorV60::UnsupportedAnimationTypeComposition)
        );
    }
    state.objects.get_mut(source).v4.creature_upgrade =
        Some(crate::standard_creatures_v1::CreatureUpgradeV1 {
            creature_types: Some((vec![Subtype::Human.stable_id()], 10)),
            ..Default::default()
        });
    assert_eq!(
        matches_effective_creature(&state, source, CreatureTypeV60::TimeLord),
        Err(CreatureTypeMatchErrorV60::UnsupportedAnimationTypeComposition)
    );
    state.restore(&snapshot);
    assert_eq!(
        matches_effective_creature(&state, source, CreatureTypeV60::TimeLord),
        Ok(false)
    );
}
