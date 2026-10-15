use super::*;
#[test]
fn exact_scope_and_frozen_representations_refuse_without_catalog_widening() {
    let pure = cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ));
    assert_eq!(live_selector().is_ok(), pure);
    let selector = SelectorV56 {
        schema: SCHEMA.into(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        producer_sha256: producer_sha256(),
    };
    for representation in [
        RepresentationV56::FrozenRulesVectorV1,
        RepresentationV56::FrozenModelFeaturesV1,
    ] {
        assert!(meaning_for_representation(representation, &selector, u16::MAX).is_err());
    }
    let elf = crate::card_def::card_id_by_name("Llanowar Elves").unwrap();
    assert!(meaning(&selector, elf).is_err());
    assert!(meaning(&selector, u16::MAX).is_err());
    if !pure {
        assert!(crate::card_def::card_id_by_name("Flamewake Phoenix").is_none());
    }
}
#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn genuine_printed_cost_intervening_checks_and_irreversible_payment_order() {
    let id = crate::card_def::card_id_by_name("Flamewake Phoenix").unwrap();
    let real = printed_meaning(id).unwrap();
    validate(&real).unwrap();
    assert_eq!((real.printed.generic_mana, real.printed.mana_value), (1, 3));
    assert_eq!(real.printed.colored_pips, [ManaColor::R, ManaColor::R]);
    assert_eq!((real.printed.power, real.printed.toughness), (2, 2));
    assert_eq!(real.printed.subtypes, [Subtype::Phoenix]);
    assert_eq!(real.capability, CardCapability::Full);
    assert_eq!(
        real.battlefield_entry,
        EntryV56::UntappedByDefaultSubjectToEntryReplacements
    );
    assert_eq!(
        real.full_engine_admitted,
        real.capability == CardCapability::Full
    );
    assert!(!real.model_admitted && !real.sampler_admitted && !real.fair_search_admitted);
    assert_eq!(
        &real.ordered_rules[..4],
        &[
            RuleV56::CreatureSpellEntersBattlefield,
            RuleV56::Keyword(KeywordV56::Flying),
            RuleV56::Keyword(KeywordV56::Haste),
            RuleV56::AttacksEachCombatIfAble
        ]
    );
    let RuleV56::GraveyardReturn {
        home,
        event,
        actor,
        intervening_if,
        optional_colored_cost,
        optional_generic_cost,
        untargeted,
        active_printed_ability_required_at_collection,
        recheck_ferocious_after_payment_offer,
        payment_allowed_after_original_source_departure,
        return_failure_refunds_payment,
        ordered_steps,
    } = &real.ordered_rules[4]
    else {
        panic!("graveyard rule")
    };
    assert_eq!(
        (*home, *event, *actor),
        (
            Zone::Graveyard,
            EventV56::BeginningOfCombatOnYourTurn,
            ActorV56::OriginalGraveyardOwnerAndController
        )
    );
    assert_eq!(intervening_if.minimum_effective_creature_power, 4);
    assert_eq!(
        intervening_if.checks,
        [
            CheckV56::TriggerCollection,
            CheckV56::ResolutionBeforeOptionalPayment
        ]
    );
    assert_eq!(optional_colored_cost, &[ManaColor::R]);
    assert_eq!(*optional_generic_cost, 0);
    assert!(
        *untargeted
            && *active_printed_ability_required_at_collection
            && *payment_allowed_after_original_source_departure
    );
    assert!(!*recheck_ferocious_after_payment_offer && !*return_failure_refunds_payment);
    assert_eq!(
        ordered_steps,
        &[
            ReturnStepV56::OfferOptionalRedMana,
            ReturnStepV56::OnAcceptancePayRedMana,
            ReturnStepV56::AfterPaymentCheckOriginalGraveyardIncarnation,
            ReturnStepV56::IfStillPresentReturnToBattlefieldUnderOriginalOwner
        ]
    );
    assert!(!crate::rules_vector_v1::sources::card_rules(id)
        .opaque_rules
        .is_empty());
    for mutation in 0..6 {
        let mut changed = real.clone();
        match mutation {
            0 => changed.printed.generic_mana = 2,
            1 => changed.ordered_rules.swap(1, 2),
            2 => {
                changed.ordered_rules.remove(3);
            }
            3 => {
                let RuleV56::GraveyardReturn { intervening_if, .. } = &mut changed.ordered_rules[4]
                else {
                    unreachable!()
                };
                intervening_if.checks.pop();
            }
            4 => {
                let RuleV56::GraveyardReturn { ordered_steps, .. } = &mut changed.ordered_rules[4]
                else {
                    unreachable!()
                };
                ordered_steps.swap(1, 2);
            }
            _ => {
                let RuleV56::GraveyardReturn {
                    return_failure_refunds_payment,
                    ..
                } = &mut changed.ordered_rules[4]
                else {
                    unreachable!()
                };
                *return_failure_refunds_payment = true;
            }
        };
        assert!(validate(&changed).is_err());
    }
    for field in 0..3 {
        let mut wrong = real.selector.clone();
        match field {
            0 => wrong.schema.clear(),
            1 => wrong.card_db_hash ^= 1,
            _ => wrong.producer_sha256.clear(),
        };
        assert!(meaning(&wrong, id).is_err());
    }
    let text = serde_json::to_string(&real).unwrap();
    for private in [
        "EffectOp",
        "ThisSource",
        "stack_item_id",
        "private_snapshot",
        "event_history",
        "canonical_path",
    ] {
        assert!(!text.contains(private));
    }
}
