use super::*;
use crate::rules_vector_v1::{facets::Collector, meaning::targets};

#[test]
fn frozen_anytarget_table_retains_exact_three_predecessor_target_atoms() {
    let mut collector = Collector::default();
    targets::target_spec(TargetSpec::AnyTarget, &mut collector);
    assert_eq!(
        serde_json::to_value(&collector.atoms).unwrap(),
        json!([
            {"Target": "PlayerYou"},
            {"Target": "PlayerOpponent"},
            {"Target": {"Object": {
                "obj": {"Typed": "Creature"}, "controller": null,
                "zone": "Battlefield", "color": null,
                "mana_value_at_most": null, "excludes": null
            }}}
        ])
    );
}

#[cfg(any(
    not(feature = "limited-fdn-fixtures"),
    feature = "standard-magezero-fixtures"
))]
#[test]
fn successor_refuses_default_and_standard_including_combined_feature_precedence() {
    assert_eq!(live_selector_v2(), Err(MeaningErrorV2::UnsupportedCatalog));
    let selector = MeaningSelectorV2 {
        schema: RULES_MEANING_SCHEMA_V2.into(),
        catalog_scope: CATALOG_SCOPE_V2.into(),
        kernel_carddb_hash: crate::card_def::KERNEL_CARDDB_HASH,
        producer_sha256: producer_sha256_v2(),
    };
    assert_eq!(
        any_target_meaning_v2(&selector, 0, AbilityRootV2::SpellMode { mode: 0 }).unwrap_err(),
        MeaningErrorV2::UnsupportedCatalog
    );
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod fdn {
    use super::*;
    use crate::card_def::{card_id_by_name, CardType};
    use crate::engine::{self, Action, Decision};
    use crate::event::{self, ProposedEvent};
    use crate::ids::PlayerId;
    use crate::mana::ManaColor;
    use crate::policy_observation_v6::tests::{put, ready_state};
    use crate::state::{Target, Zone};
    use std::collections::BTreeSet;

    #[test]
    fn anytarget_successor_matches_actual_bolt_recipient_menu_both_seats_and_restore() {
        let selector = live_selector_v2().unwrap();
        let bolt_id = card_id_by_name("Lightning Bolt").unwrap();
        let meaning =
            any_target_meaning_v2(&selector, bolt_id, AbilityRootV2::SpellMode { mode: 0 })
                .unwrap();
        assert_eq!(meaning.target_count, 1);
        assert_eq!(meaning.target_facts.as_array().unwrap().len(), 4);
        assert_eq!(meaning.root_sha256.len(), 64);
        assert_ne!(
            meaning.schema,
            crate::rules_vector_v1::RULES_VECTOR_SCHEMA_V1
        );
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state();
            state.active_player = actor;
            state.priority_player = actor;
            let elf = put(
                &mut state,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            let ajani = put(&mut state, actor, "Ajani, Caller of the Pride", Zone::Hand);
            // Genuine entry initializes starting loyalty. This tests its Partial
            // registered recipient identity, not casting or loyalty abilities.
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(ajani, Zone::Battlefield),
            );
            let land = put(&mut state, actor, "Forest", Zone::Battlefield);
            let protected = put(
                &mut state,
                actor.opponent(),
                "Guardian of the Guildpact",
                Zone::Battlefield,
            );
            let bolt = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
            state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
            engine::step(&mut state, Action::CastSpell(bolt)).unwrap();
            let mut restored = serde_json::from_slice::<crate::state::GameState>(
                &serde_json::to_vec(&state).unwrap(),
            )
            .unwrap();
            for current in [&mut state, &mut restored] {
                let legal_targets = match engine::advance_until_decision(current) {
                    Decision::ChooseTargets { legal_targets, .. } => legal_targets,
                    other => panic!("expected actual AnyTarget cast menu, got {other:?}"),
                };
                assert_eq!(
                    &legal_targets[..2],
                    &[Target::Player(PlayerId::P0), Target::Player(PlayerId::P1)]
                );
                assert!(legal_targets.contains(&Target::Object(elf)));
                assert!(legal_targets.contains(&Target::Object(ajani)));
                assert!(!legal_targets.contains(&Target::Object(land)));
                assert!(!legal_targets.contains(&Target::Object(protected)));
                let classes: BTreeSet<_> = legal_targets
                    .iter()
                    .map(|target| match target {
                        Target::Player(player) if *player == actor => RecipientClassV2::PlayerYou,
                        Target::Player(_) => RecipientClassV2::PlayerOpponent,
                        Target::Object(object)
                            if engine::object_has_type(current, *object, CardType::Creature) =>
                        {
                            RecipientClassV2::Creature
                        }
                        Target::Object(object)
                            if engine::object_has_type(
                                current,
                                *object,
                                CardType::Planeswalker,
                            ) =>
                        {
                            RecipientClassV2::Planeswalker
                        }
                        other => panic!("unexpected actual recipient: {other:?}"),
                    })
                    .collect();
                assert_eq!(classes, meaning.recipients.iter().copied().collect());
            }
            assert_eq!(
                state.diagnostic_state_hash(),
                restored.diagnostic_state_hash()
            );
        }
    }

    #[test]
    fn explicit_source_selection_refuses_stale_catalog_schema_and_wrong_ability_roots() {
        let selector = live_selector_v2().unwrap();
        let bolt = card_id_by_name("Lightning Bolt").unwrap();
        for field in 0..4 {
            let mut forged = selector.clone();
            match field {
                0 => forged.schema = crate::rules_vector_v1::RULES_VECTOR_SCHEMA_V1.into(),
                1 => forged.catalog_scope = "pauper+standard".into(),
                2 => forged.kernel_carddb_hash ^= 1,
                _ => forged.producer_sha256.replace_range(..1, "x"),
            }
            assert_eq!(
                any_target_meaning_v2(&forged, bolt, AbilityRootV2::SpellMode { mode: 0 })
                    .unwrap_err(),
                MeaningErrorV2::WrongSelector
            );
        }
        assert_eq!(
            any_target_meaning_v2(&selector, u16::MAX, AbilityRootV2::SpellMode { mode: 0 })
                .unwrap_err(),
            MeaningErrorV2::UnknownCard
        );
        for root in [
            AbilityRootV2::SpellMode { mode: 3 },
            AbilityRootV2::Activated { ability_index: 0 },
            AbilityRootV2::Triggered {
                ability_index: 0,
                program_index: 0,
            },
            AbilityRootV2::Adventure,
            AbilityRootV2::Omen,
        ] {
            assert_eq!(
                any_target_meaning_v2(&selector, bolt, root).unwrap_err(),
                MeaningErrorV2::MissingAbilityRoot
            );
        }
        let longbow = card_id_by_name("Viridian Longbow").unwrap();
        let granted =
            any_target_meaning_v2(&selector, longbow, AbilityRootV2::EquipmentGrantedActivated)
                .unwrap();
        let spell =
            any_target_meaning_v2(&selector, bolt, AbilityRootV2::SpellMode { mode: 0 }).unwrap();
        assert_eq!(granted.recipients, spell.recipients);
        assert_ne!(granted.root_sha256, spell.root_sha256);
        assert_ne!(granted.canonical_bytes(), spell.canonical_bytes());
        assert_eq!(
            any_target_meaning_v2(&selector, longbow, AbilityRootV2::SpellMode { mode: 0 })
                .unwrap_err(),
            MeaningErrorV2::UnsupportedTargetSpec
        );
    }

    #[test]
    fn successor_extraction_does_not_rewrite_any_serialized_predecessor_record_or_vector() {
        let excluded: Vec<u16> = crate::card_def::CARD_DEFS
            .iter()
            .enumerate()
            .filter(|(_, def)| {
                def.target_spec == TargetSpec::KickerControlledCreatures
                    || def.kicked_target_spec == Some(TargetSpec::KickerControlledCreatures)
            })
            .map(|(id, _)| id as u16)
            .collect();
        assert_eq!(
            excluded,
            vec![card_id_by_name("Divine Resilience").unwrap()]
        );
        assert!(matches!(
            (crate::card_def::CARD_DEFS[excluded[0] as usize].spell_effect)(),
            Some(crate::effect::EffectOp::GrantKeywordToDeclaredTargetsUntilEndOfTurn { .. })
        ));
        // V1 deliberately refuses the new variable-target definition. Compare
        // every supported record and vector rather than invoking its whole-table
        // builder on an incompatible current catalog or weakening that refusal.
        let snapshot = || {
            (0..crate::card_def::CARD_DEFS.len())
                .map(|id| id as u16)
                .filter(|id| !excluded.contains(id))
                .map(|id| {
                    (
                        id,
                        serde_json::to_vec(&crate::rules_vector_v1::sources::card_rules(id))
                            .unwrap(),
                        serde_json::to_vec(
                            &crate::rules_vector_v1::vectorize::card_feature_counts(id),
                        )
                        .unwrap(),
                    )
                })
                .collect::<Vec<_>>()
        };
        let records_before = snapshot();
        let vocabulary_before = crate::rules_vector_v1::vectorize::pauper_vocabulary();
        let extractor_before = crate::rules_vector_v1::extractor_sha256();
        let selector = live_selector_v2().unwrap();
        for (name, root) in [
            ("Lightning Bolt", AbilityRootV2::SpellMode { mode: 0 }),
            ("Viridian Longbow", AbilityRootV2::EquipmentGrantedActivated),
        ] {
            let card = card_id_by_name(name).unwrap();
            let before =
                serde_json::to_vec(&crate::rules_vector_v1::sources::card_rules(card)).unwrap();
            let meaning = any_target_meaning_v2(&selector, card, root).unwrap();
            assert_eq!(meaning.predecessor_extractor_sha256, extractor_before);
            let again = any_target_meaning_v2(&selector, card, root).unwrap();
            assert_eq!(meaning.canonical_bytes(), again.canonical_bytes());
            assert_eq!(meaning.output_sha256(), again.output_sha256());
            assert_eq!(
                before,
                serde_json::to_vec(&crate::rules_vector_v1::sources::card_rules(card)).unwrap()
            );
        }
        assert_eq!(extractor_before, crate::rules_vector_v1::extractor_sha256());
        assert_eq!(records_before, snapshot());
        assert_eq!(
            vocabulary_before,
            crate::rules_vector_v1::vectorize::pauper_vocabulary()
        );
    }

    #[test]
    #[should_panic(expected = "variable cast target effects require a RulesMeaning successor")]
    fn divine_resilience_still_refuses_frozen_predecessor_extraction() {
        crate::rules_vector_v1::sources::card_rules(card_id_by_name("Divine Resilience").unwrap());
    }
}
