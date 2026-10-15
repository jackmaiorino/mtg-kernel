use super::*;
use crate::card_def::card_id_by_name;

#[test]
fn precise_catalog_selector_and_representation_refuse_foreign_inputs() {
    let selected = cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ));
    assert_eq!(live_selector().is_ok(), selected);
    let candidate = SelectorV52 {
        schema: SCHEMA.into(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        producer_sha256: producer_sha256(),
    };
    let bolt = card_id_by_name("Lightning Bolt").unwrap();
    let root = RootV52::CreatureSpellAndPrintedStatics;
    let bytes = serde_json::to_vec(&candidate).unwrap();
    for representation in [
        RepresentationV52::FrozenRulesVectorV1,
        RepresentationV52::FrozenModelFeaturesV1,
    ] {
        assert_eq!(
            meaning_for_representation(representation, &candidate, u16::MAX, root),
            Err(ErrorV52::UnsupportedRepresentation)
        );
    }
    if selected {
        assert_eq!(
            meaning(&candidate, bolt, root),
            Err(ErrorV52::UnsupportedDefinition)
        );
        assert_eq!(
            meaning(&candidate, u16::MAX, root),
            Err(ErrorV52::UnsupportedDefinition)
        );
        for changed in 0..3 {
            let mut wrong = candidate.clone();
            match changed {
                0 => wrong.schema.push('x'),
                1 => wrong.card_db_hash ^= 1,
                _ => wrong.producer_sha256.clear(),
            }
            assert_eq!(meaning(&wrong, bolt, root), Err(ErrorV52::WrongSelector));
        }
    } else {
        assert!(card_id_by_name("Elenda, Saint of Dusk").is_none());
        assert!(CARD_DEFS
            .iter()
            .all(|def| def.starting_life_self_bonuses_v1.is_empty()
                && def.source_type_hexproof_v1.is_none()));
        assert_eq!(
            meaning(&candidate, bolt, root),
            Err(ErrorV52::UnsupportedCatalog)
        );
    }
    assert_eq!(serde_json::to_vec(&candidate).unwrap(), bytes);
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod registered {
    use super::*;

    fn genuine() -> MeaningV52 {
        let id = card_id_by_name("Elenda, Saint of Dusk").unwrap();
        printed_meaning(id, RootV52::CreatureSpellAndPrintedStatics).unwrap()
    }

    #[test]
    fn genuine_printed_order_keeps_two_additive_bonuses_and_menace_only_on_first() {
        let real = genuine();
        validate(&real).unwrap();
        assert_eq!(real.capability, CardCapability::Partial);
        assert!(!real.model_admitted);
        assert_eq!(real.printed.generic_mana, 2);
        assert_eq!(real.printed.colored_pips, [ManaColor::W, ManaColor::B]);
        assert_eq!((real.printed.power, real.printed.toughness), (4, 4));
        assert_eq!(real.home, HomeV52::BattlefieldPrintedFrontFace);
        assert!(real.active_printed_ability_required);
        assert_eq!(
            real.evaluation,
            EvaluationV52::CurrentControllerAndWorldOnEveryQuery
        );
        assert_eq!(real.supported_starting_life, 20);
        assert_eq!(
            real.bonus_combination,
            CombinationV52::AddEverySatisfiedSelfBonus
        );
        assert_eq!(
            real.ordered_rules,
            vec![
                RuleV52::Keyword(KeywordV52::Lifelink),
                RuleV52::HexproofFromSourceType {
                    targeter: TargeterV52::OpponentOfCurrentController,
                    required_source_type: CardType::Instant,
                    source_type_basis:
                        SourceTypeBasisV52::EffectiveSelectedFormOrOriginalSourceCharacteristics,
                    ordered_checks: vec![
                        TargetCheckV52::TargetSelection,
                        TargetCheckV52::ResolutionRevalidation
                    ],
                },
                RuleV52::ConditionalSelfBonus {
                    condition:
                        LifeConditionV52::CurrentControllerLifeAtLeastTheirStartingLifePlus {
                            amount: 1
                        },
                    ordered_grants: vec![
                        SelfGrantV52::ModifyPowerToughness {
                            power: 1,
                            toughness: 1
                        },
                        SelfGrantV52::GainKeyword(KeywordV52::Menace)
                    ],
                },
                RuleV52::ConditionalSelfBonus {
                    condition:
                        LifeConditionV52::CurrentControllerLifeAtLeastTheirStartingLifePlus {
                            amount: 10
                        },
                    ordered_grants: vec![SelfGrantV52::ModifyPowerToughness {
                        power: 5,
                        toughness: 5
                    }],
                },
            ]
        );
        let decoded: MeaningV52 =
            serde_json::from_slice(&serde_json::to_vec(&real).unwrap()).unwrap();
        assert_eq!(decoded, real);
        validate(&decoded).unwrap();
        // The predecessor representation is deliberately unchanged and refuses
        // the new typed descriptors rather than inventing coarse features.
        assert!(
            std::panic::catch_unwind(|| crate::rules_vector_v1::sources::card_rules(real.card_id))
                .is_err()
        );
    }

    #[test]
    fn omission_and_reordering_of_actual_definition_descriptors_cannot_supply_meaning() {
        let real = genuine();
        let def = &CARD_DEFS[usize::from(real.card_id)];
        // These are explicit altered descriptor inputs, never catalog aliases.
        assert_eq!(
            ordered_statics(&[], def.source_type_hexproof_v1, def.keywords),
            Err(ErrorV52::UnsupportedDefinition)
        );
        assert_eq!(
            ordered_statics(def.starting_life_self_bonuses_v1, None, def.keywords),
            Err(ErrorV52::UnsupportedDefinition)
        );
        assert_eq!(
            ordered_statics(
                def.starting_life_self_bonuses_v1,
                def.source_type_hexproof_v1,
                Keywords::HEXPROOF
            ),
            Err(ErrorV52::UnsupportedDefinition)
        );
        let mut swapped = def.starting_life_self_bonuses_v1.to_vec();
        swapped.reverse();
        assert_eq!(
            ordered_statics(&swapped, def.source_type_hexproof_v1, def.keywords),
            Err(ErrorV52::UnsupportedDefinition)
        );
        let mut omitted = real.clone();
        omitted.ordered_rules.remove(3);
        assert_eq!(validate(&omitted), Err(ErrorV52::ForgedMeaning));
        let mut missing = serde_json::to_value(&real).unwrap();
        missing.as_object_mut().unwrap().remove("ordered_rules");
        assert!(serde_json::from_value::<MeaningV52>(missing).is_err());
    }

    #[test]
    fn changed_thresholds_grants_order_protection_checks_or_admission_refuse() {
        let real = genuine();
        for mutation in 0..12 {
            let mut wrong = real.clone();
            match mutation {
                0 => wrong.ordered_rules.swap(2, 3),
                1 => {
                    if let RuleV52::ConditionalSelfBonus { condition, .. } =
                        &mut wrong.ordered_rules[2]
                    {
                        *condition =
                            LifeConditionV52::CurrentControllerLifeAtLeastTheirStartingLifePlus {
                                amount: 2,
                            };
                    }
                }
                2 => {
                    if let RuleV52::ConditionalSelfBonus { condition, .. } =
                        &mut wrong.ordered_rules[3]
                    {
                        *condition =
                            LifeConditionV52::CurrentControllerLifeAtLeastTheirStartingLifePlus {
                                amount: 9,
                            };
                    }
                }
                3 => {
                    if let RuleV52::ConditionalSelfBonus { ordered_grants, .. } =
                        &mut wrong.ordered_rules[3]
                    {
                        ordered_grants.push(SelfGrantV52::GainKeyword(KeywordV52::Menace));
                    }
                }
                4 => {
                    if let RuleV52::ConditionalSelfBonus { ordered_grants, .. } =
                        &mut wrong.ordered_rules[2]
                    {
                        ordered_grants.reverse();
                    }
                }
                5 => {
                    if let RuleV52::HexproofFromSourceType {
                        required_source_type,
                        ..
                    } = &mut wrong.ordered_rules[1]
                    {
                        *required_source_type = CardType::Sorcery;
                    }
                }
                6 => {
                    if let RuleV52::HexproofFromSourceType { ordered_checks, .. } =
                        &mut wrong.ordered_rules[1]
                    {
                        ordered_checks.pop();
                    }
                }
                7 => wrong.active_printed_ability_required = false,
                8 => wrong.supported_starting_life = 30,
                9 => wrong.model_admitted = true,
                10 => wrong.printed.power = 5,
                _ => wrong.root_sha256.clear(),
            }
            assert_ne!(wrong, real, "mutation must alter a semantic contract");
            assert_eq!(validate(&wrong), Err(ErrorV52::ForgedMeaning));
        }
    }

    #[test]
    fn strict_typed_rules_refuse_runtime_inputs_and_foreign_controller_aliases() {
        let value = serde_json::to_value(genuine()).unwrap();
        for field in [
            "source_type_input_v1",
            "private_snapshot_json",
            "object_id",
            "execution_index",
            "hidden_world",
        ] {
            let mut foreign = value.clone();
            foreign[field] = serde_json::json!(0);
            assert!(serde_json::from_value::<MeaningV52>(foreign).is_err());
        }
        let mut foreign = value.clone();
        foreign["ordered_rules"][2]["ConditionalSelfBonus"]["private_snapshot_json"] =
            serde_json::json!("invented");
        assert!(serde_json::from_value::<MeaningV52>(foreign).is_err());
        let mut own_targeter = value.clone();
        own_targeter["ordered_rules"][1]["HexproofFromSourceType"]["targeter"] =
            serde_json::json!("OwnController");
        assert!(serde_json::from_value::<MeaningV52>(own_targeter).is_err());
        let mut fixed_owner = value;
        fixed_owner["ordered_rules"][2]["ConditionalSelfBonus"]["condition"] =
            serde_json::json!({"OwnerLifeAtLeastTheirStartingLifePlus": {"amount": 1}});
        assert!(serde_json::from_value::<MeaningV52>(fixed_owner).is_err());
    }

    #[test]
    fn typed_opponent_instant_rule_agrees_with_real_selected_source_forms() {
        use crate::ids::PlayerId;
        use crate::state::{CastMethodV4, GameState};
        let real = genuine();
        let RuleV52::HexproofFromSourceType {
            targeter,
            required_source_type,
            source_type_basis,
            ..
        } = &real.ordered_rules[1]
        else {
            panic!("ordered printed Instant rule")
        };
        assert_eq!(*targeter, TargeterV52::OpponentOfCurrentController);
        assert_eq!(
            *source_type_basis,
            SourceTypeBasisV52::EffectiveSelectedFormOrOriginalSourceCharacteristics
        );
        for actor in [PlayerId::P0, PlayerId::P1] {
            let forest = card_id_by_name("Forest").unwrap();
            let mut state = GameState::new_from_libraries(
                &[forest; 40],
                &[forest; 40],
                |_| "Forest".into(),
                52119,
            );
            let target = crate::policy_observation_v6::tests::put(
                &mut state,
                actor,
                "Elenda, Saint of Dusk",
                Zone::Battlefield,
            );
            assert_eq!(
                crate::source_type_hexproof_v1::active_rule(&state, target, actor),
                None
            );
            assert_eq!(
                crate::source_type_hexproof_v1::active_rule(&state, target, actor.opponent()),
                Some(SourceTypeHexproofDefV1::Instants)
            );
        }
        // Genuine Limited sources. The joint Instant Adventure printing case
        // remains unavailable in this pool; Standard Questing Druid is separate.
        for (name, method, is_instant, is_sorcery) in [
            ("Lightning Bolt", CastMethodV4::Normal, true, false),
            ("Involuntary Employment", CastMethodV4::Normal, false, true),
            ("Fang Dragon", CastMethodV4::Normal, false, false),
            ("Fang Dragon", CastMethodV4::Omen, false, true),
            ("Sagu Wildling", CastMethodV4::Omen, false, true),
        ] {
            let source = &CARD_DEFS[usize::from(card_id_by_name(name).unwrap())];
            let types = crate::source_type_hexproof_v1::spell_types(source, method).unwrap();
            assert_eq!(
                types.contains(*required_source_type),
                is_instant,
                "{name} {method:?}"
            );
            assert_eq!(
                types.contains(CardType::Sorcery),
                is_sorcery,
                "{name} {method:?}"
            );
            assert_eq!(
                crate::source_type_hexproof_v1::protects(SourceTypeHexproofDefV1::Instants, types),
                is_instant
            );
        }
    }
}
