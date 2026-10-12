use super::*;

#[test]
fn catalog_selector_and_exact_representation_are_required_before_definition_lookup() {
    let Ok(selector) = live_selector() else {
        assert_eq!(live_selector(), Err(ErrorV1::UnsupportedCatalog));
        return;
    };
    for representation in [
        MeaningRepresentationV1::FrozenRulesVectorV1,
        MeaningRepresentationV1::FrozenModelFeaturesV1,
    ] {
        assert_eq!(
            meaning_for_representation(representation, &selector, u16::MAX, CastRootV1::Normal),
            Err(ErrorV1::UnsupportedRepresentation)
        );
    }
    for field in 0..3 {
        let mut stale = selector.clone();
        match field {
            0 => stale.schema.push('x'),
            1 => stale.card_db_hash ^= 1,
            _ => stale.producer_sha256.push('x'),
        }
        assert_eq!(
            meaning(&stale, u16::MAX, CastRootV1::Normal),
            Err(ErrorV1::WrongSelector)
        );
    }
    assert_eq!(
        meaning(&selector, u16::MAX, CastRootV1::Normal),
        Err(ErrorV1::UnsupportedDefinition)
    );
    let bolt = card_def::card_id_by_name("Lightning Bolt").unwrap();
    assert_eq!(
        meaning(&selector, bolt, CastRootV1::Normal),
        Err(ErrorV1::UnsupportedDefinition)
    );
}

#[test]
fn frozen_selection_facets_cannot_distinguish_the_union_or_mandatory_lower_bound() {
    use crate::rules_vector_v1::facets::Collector;
    use crate::rules_vector_v1::meaning::{effect_op, Env};
    let exact = selection_rule();
    let collect = |rule| {
        let mut out = Collector::default();
        effect_op(
            &EffectOp::SelectObjectsV1 { rule },
            &Env {
                target_spec: TargetSpec::None,
            },
            &mut out,
        );
        out.atoms
    };
    let mut different_filter = exact;
    different_filter.filter = ObjectSelectionFilterV1::Creature;
    let mut optional = exact;
    optional.min = 0;
    assert_eq!(collect(exact), collect(different_filter));
    assert_eq!(collect(exact), collect(optional));
    assert_ne!(exact, different_filter);
    assert_ne!(exact, optional);
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod registered {
    use super::*;
    use crate::engine::{self, Action, Decision};
    use crate::ids::PlayerId;
    use crate::policy_observation_v6::tests::{put, ready_state};

    #[test]
    fn genuine_paid_normal_and_flashback_rows_bind_distinct_exact_roots_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut hashes = Vec::new();
            for root in [CastRootV1::Normal, CastRootV1::Flashback] {
                let selector = live_selector().unwrap();
                let id = card_def::card_id_by_name("Inspiration from Beyond").unwrap();
                let record = meaning_for_representation(
                    MeaningRepresentationV1::ExactInspirationV1,
                    &selector,
                    id,
                    root,
                )
                .unwrap();
                let mut state = ready_state();
                state.active_player = actor;
                state.priority_player = actor;
                state.step = crate::state::Step::Main1;
                let flashback = root == CastRootV1::Flashback;
                let zone = if flashback {
                    Zone::Graveyard
                } else {
                    Zone::Hand
                };
                let spell = put(&mut state, actor, "Inspiration from Beyond", zone);
                state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] =
                    if flashback { 5 } else { 2 };
                state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] =
                    if flashback { 2 } else { 1 };
                engine::step(&mut state, Action::CastSpell(spell)).unwrap();
                assert!(matches!(
                    engine::advance_until_decision(&mut state),
                    Decision::CastSpellOrPass { .. }
                ));
                assert!(state.engine.pending_cast.is_none());
                let item = state.stack.last().unwrap();
                assert_eq!(item.source, spell);
                assert_eq!(item.controller, actor);
                assert_eq!(item.is_flashback, flashback);
                assert_eq!(
                    engine::validated_stack_item_target_spec(item, &state).unwrap(),
                    TargetSpec::None
                );
                assert!(item.targets.is_empty());
                assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
                assert_eq!(
                    record.rule.printed_cast_cost,
                    if flashback { "{5}{U}{U}" } else { "{2}{U}" }
                );
                assert_eq!(record.rule.cast_from_own_graveyard, flashback);
                assert_eq!(record.rule.flashback_exiles_when_leaving_stack, flashback);
                assert_eq!(record.rule.selection_rule, selection_rule());
                assert_eq!(
                    record.rule.availability_bounds,
                    AvailabilityBoundsV1::OneWhenAvailableElseZero
                );
                assert!(
                    record.rule.resolving_choice_is_not_a_target
                        && record.rule.spell_remains_on_stack_during_return_choice
                );
                validate(&record).unwrap();
                let decoded: MeaningV1 =
                    serde_json::from_slice(&serde_json::to_vec(&record).unwrap()).unwrap();
                assert_eq!(record, decoded);
                hashes.push(record.root_sha256);
            }
            assert_ne!(hashes[0], hashes[1]);
        }
    }

    #[test]
    fn full_record_reconstruction_rejects_union_bounds_order_root_and_metadata_forgeries() {
        let selector = live_selector().unwrap();
        let id = card_def::card_id_by_name("Inspiration from Beyond").unwrap();
        for root in [CastRootV1::Normal, CastRootV1::Flashback] {
            let good = meaning(&selector, id, root).unwrap();
            let json = serde_json::to_value(&good).unwrap();
            for field in json["rule"]
                .as_object()
                .unwrap()
                .iter()
                .filter_map(|(k, v)| v.as_bool().map(|_| k))
            {
                let mut bad = json.clone();
                bad["rule"][field] = serde_json::json!(!json["rule"][field].as_bool().unwrap());
                let bad: MeaningV1 = serde_json::from_value(bad).unwrap();
                assert_eq!(validate(&bad), Err(ErrorV1::ForgedMeaning));
            }
            for mutation in 0..10 {
                let mut bad = good.clone();
                match mutation {
                    0 => bad.rule.selection_rule.filter = ObjectSelectionFilterV1::Creature,
                    1 => bad.rule.selection_rule.min = 0,
                    2 => bad.rule.selection_rule.max = 2,
                    3 => bad.rule.selection_rule.any_player = true,
                    4 => bad.rule.selection_rule.player = PlayerRef::Opponent,
                    5 => bad.rule.mill_count = 2,
                    6 => bad.rule.printed_cast_cost = "{0}".into(),
                    7 => {
                        bad.cast_root = if root == CastRootV1::Normal {
                            CastRootV1::Flashback
                        } else {
                            CastRootV1::Normal
                        }
                    }
                    8 => bad.native_model_sampler_admitted = true,
                    _ => bad.root_sha256.clear(),
                }
                assert_eq!(validate(&bad), Err(ErrorV1::ForgedMeaning));
            }
            let mut bad = json;
            bad["rule"]["hidden_library_identity"] = serde_json::json!(42);
            assert!(serde_json::from_value::<MeaningV1>(bad).is_err());
        }
    }
}
