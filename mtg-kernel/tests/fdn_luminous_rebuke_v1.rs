//! Target-dependent cost, ordinary creature legality and post-cast responses.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, preflight_fully_supported_deck, CardType, DynamicCountDef,
    GenericCostReductionDef, TargetSpec, CARD_DEFS,
};
use mtg_kernel::effect::EffectBooleanChoicePurpose;
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

fn ready() -> GameState {
    let island = card_id_by_name("Island").unwrap();
    let mut state =
        GameState::new_from_libraries(&[island; 40], &[island; 40], |_| "Island".into(), 123);
    state.step = Step::Main1;
    state
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap();
    let object = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.into(),
        owner: player,
        controller: player,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: vec![],
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Battlefield => state.players[player.index()].battlefield.push(object),
        Zone::Hand => state.players[player.index()].hand.push(object),
        _ => panic!("helper zone"),
    }
    object
}

fn creature(state: &mut GameState, player: PlayerId, name: &str, tapped: bool) -> ObjectId {
    let id = put(state, player, name, Zone::Battlefield);
    state.objects.get_mut(id).tapped = tapped;
    id
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

fn pass_or_order(state: &mut GameState, decision: Decision) {
    let action = match decision {
        Decision::CastSpellOrPass { .. } => Action::Pass,
        Decision::OrderTriggers { pending, .. } => {
            Action::OrderTriggers((0..pending.len()).collect())
        }
        other => panic!("unexpected decision {other:?}"),
    };
    engine::step(state, action).unwrap();
}

fn drain(state: &mut GameState, payment: Option<bool>) -> usize {
    let mut choices = 0;
    for _ in 0..100 {
        let decision = next(state);
        if matches!(decision, Decision::CastSpellOrPass { .. }) && state.stack.is_empty() {
            assert!(state.engine.pending_triggers.is_empty());
            return choices;
        }
        if matches!(decision, Decision::ChooseEffectBoolean { .. }) {
            choices += 1;
            engine::step(state, Action::ChooseEffectBoolean(payment.unwrap())).unwrap();
        } else {
            pass_or_order(state, decision);
        }
    }
    panic!("resolution did not finish");
}

fn begin(state: &mut GameState) -> (ObjectId, Vec<Target>) {
    let spell = put(state, PlayerId::P0, "Luminous Rebuke", Zone::Hand);
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&spell))
    );
    engine::step(state, Action::CastSpell(spell)).unwrap();
    let Decision::ChooseTargets { legal_targets, .. } = next(state) else {
        panic!("expected target menu");
    };
    (spell, legal_targets)
}

fn cast(state: &mut GameState, target: ObjectId) -> ObjectId {
    let (spell, choices) = begin(state);
    assert!(choices.contains(&Target::Object(target)));
    engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    assert!(state.engine.pending_cast.is_none());
    spell
}

#[test]
fn printed_definition_and_target_reducer_are_exact() {
    assert_eq!(card_id_by_name("Luminous Rebuke"), Some(230));
    let rebuke = &CARD_DEFS[230];
    assert_eq!(rebuke.mana_value, 5);
    assert_eq!(rebuke.cost.generic, 4);
    assert_eq!(rebuke.cost.pips, &[Pip::Colored(ManaColor::W)]);
    assert_eq!(rebuke.colors, &[ManaColor::W]);
    assert_eq!(rebuke.types, &[CardType::Instant]);
    assert_eq!(rebuke.target_spec, TargetSpec::Creature);
    assert_eq!(
        rebuke.generic_cost_reduction,
        Some(GenericCostReductionDef {
            generic_per_count: 3,
            count: DynamicCountDef::SpellTargetsTappedCreature,
        })
    );
    preflight_fully_supported_deck(&[230]).unwrap();
}

#[test]
fn offer_requires_a_legal_payable_target_and_the_white_pip() {
    for (white, blue, tapped, offered) in [
        (1, 0, true, false),
        (0, 5, true, false),
        (1, 1, true, true),
        (1, 1, false, false),
        (1, 3, false, false),
        (1, 4, false, true),
    ] {
        let mut state = ready();
        creature(&mut state, PlayerId::P1, "Elvish Mystic", tapped);
        let spell = put(&mut state, PlayerId::P0, "Luminous Rebuke", Zone::Hand);
        state.players[0].mana_pool[ManaColor::W.pool_index()] = white;
        state.players[0].mana_pool[ManaColor::U.pool_index()] = blue;
        let Decision::CastSpellOrPass {
            castable_spells, ..
        } = next(&mut state)
        else {
            panic!("casting window");
        };
        assert_eq!(castable_spells.contains(&spell), offered);
        if !offered {
            let hash = state.state_hash();
            assert!(engine::step(&mut state, Action::CastSpell(spell)).is_err());
            assert_eq!(state.state_hash(), hash);
        }
    }
}

#[test]
fn two_mana_menu_excludes_unpayable_untapped_targets_without_mutating_on_rejection() {
    let mut state = ready();
    let tapped = creature(&mut state, PlayerId::P1, "Elvish Mystic", true);
    let untapped = creature(&mut state, PlayerId::P1, "Elvish Mystic", false);
    state.players[0].mana_pool[ManaColor::W.pool_index()] = 2;
    let (spell, choices) = begin(&mut state);
    assert_eq!(choices, vec![Target::Object(tapped)]);
    let hash = state.state_hash();
    assert!(engine::step(&mut state, Action::ChooseTarget(Target::Object(untapped))).is_err());
    assert_eq!(state.state_hash(), hash);
    engine::step(&mut state, Action::ChooseTarget(Target::Object(tapped))).unwrap();
    drain(&mut state, None);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
    assert_eq!(state.objects.get(tapped).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(untapped).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(spell).zone, Zone::Graveyard);
}

#[test]
fn full_cost_admits_untapped_creatures_and_only_discounted_target_pays_two() {
    for tapped_target in [false, true] {
        let mut state = ready();
        let tapped = creature(&mut state, PlayerId::P1, "Elvish Mystic", true);
        let untapped = creature(&mut state, PlayerId::P1, "Elvish Mystic", false);
        state.players[0].mana_pool[ManaColor::W.pool_index()] = 5;
        let (_, choices) = begin(&mut state);
        assert_eq!(
            choices,
            vec![Target::Object(tapped), Target::Object(untapped)]
        );
        engine::step(
            &mut state,
            Action::ChooseTarget(Target::Object(if tapped_target {
                tapped
            } else {
                untapped
            })),
        )
        .unwrap();
        drain(&mut state, None);
        assert_eq!(
            state.players[0].mana_pool[ManaColor::W.pool_index()],
            if tapped_target { 3 } else { 0 }
        );
    }
}

#[test]
fn a_tapped_noncreature_or_protected_creature_cannot_supply_a_discounted_cast() {
    for protected in [false, true] {
        let mut state = ready();
        if protected {
            creature(&mut state, PlayerId::P1, "Saiba Cryptomancer", true);
        } else {
            let land = put(&mut state, PlayerId::P1, "Forest", Zone::Battlefield);
            state.objects.get_mut(land).tapped = true;
        }
        creature(&mut state, PlayerId::P1, "Elvish Mystic", false);
        state.players[0].mana_pool[ManaColor::W.pool_index()] = 2;
        let spell = put(&mut state, PlayerId::P0, "Luminous Rebuke", Zone::Hand);
        assert!(
            matches!(next(&mut state), Decision::CastSpellOrPass { castable_spells, .. } if !castable_spells.contains(&spell))
        );
    }
}

#[test]
fn no_creature_target_prevents_cast_even_with_five_mana() {
    let mut state = ready();
    state.players[0].mana_pool[ManaColor::W.pool_index()] = 5;
    let spell = put(&mut state, PlayerId::P0, "Luminous Rebuke", Zone::Hand);
    assert!(
        matches!(next(&mut state), Decision::CastSpellOrPass { castable_spells, .. } if !castable_spells.contains(&spell))
    );
}

#[test]
fn own_and_legendary_creatures_are_legal_targets() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready();
        let target = creature(&mut state, player, "Dwynen, Gilt-Leaf Daen", true);
        state.players[0].mana_pool[ManaColor::W.pool_index()] = 2;
        cast(&mut state, target);
        drain(&mut state, None);
        assert_eq!(state.objects.get(target).zone, Zone::Graveyard);
    }
}

#[test]
fn land_payment_consumes_two_sources_for_a_tapped_target_and_five_for_untapped() {
    for (tapped, lands) in [(true, 2), (false, 5)] {
        let mut state = ready();
        let target = creature(&mut state, PlayerId::P1, "Elvish Mystic", tapped);
        let plains: Vec<_> = (0..lands)
            .map(|_| put(&mut state, PlayerId::P0, "Plains", Zone::Battlefield))
            .collect();
        cast(&mut state, target);
        assert_eq!(
            plains
                .iter()
                .filter(|id| state.objects.get(**id).tapped)
                .count(),
            lands
        );
        assert_eq!(state.players[0].mana_pool, [0; 6]);
        drain(&mut state, None);
        assert_eq!(state.objects.get(target).zone, Zone::Graveyard);
    }
}

#[test]
fn pending_discounted_target_choice_restores_identically() {
    let mut state = ready();
    let target = creature(&mut state, PlayerId::P1, "Elvish Mystic", true);
    creature(&mut state, PlayerId::P1, "Elvish Mystic", false);
    state.players[0].mana_pool[ManaColor::W.pool_index()] = 2;
    begin(&mut state);
    let bytes = serde_json::to_vec(&state).unwrap();
    let mut restored: GameState = serde_json::from_slice(&bytes).unwrap();
    engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
    engine::step(&mut restored, Action::ChooseTarget(Target::Object(target))).unwrap();
    drain(&mut state, None);
    drain(&mut restored, None);
    assert_eq!(state.state_hash(), restored.state_hash());
}

#[test]
fn losing_the_discount_before_final_payment_aborts_without_spending_mana() {
    let mut state = ready();
    let target = creature(&mut state, PlayerId::P1, "Elvish Mystic", true);
    state.players[0].mana_pool[ManaColor::W.pool_index()] = 2;
    let (spell, _) = begin(&mut state);
    engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
    state.objects.get_mut(target).tapped = false;
    next(&mut state);
    assert_eq!(state.objects.get(spell).zone, Zone::Hand);
    assert_eq!(state.players[0].mana_pool[ManaColor::W.pool_index()], 2);
    assert!(state.engine.pending_cast.is_none());
}

#[test]
fn actual_ranger_response_untaps_the_target_without_invalidating_or_repricing_rebuke() {
    let mut state = ready();
    let target = creature(&mut state, PlayerId::P1, "Elvish Mystic", true);
    let ranger = creature(&mut state, PlayerId::P1, "Quirion Ranger", false);
    let forest = put(&mut state, PlayerId::P1, "Forest", Zone::Battlefield);
    state.players[0].mana_pool[ManaColor::W.pool_index()] = 2;
    cast(&mut state, target);
    let decision = next(&mut state);
    if matches!(
        decision,
        Decision::CastSpellOrPass {
            player: PlayerId::P0,
            ..
        }
    ) {
        pass_or_order(&mut state, decision);
    }
    assert!(
        matches!(next(&mut state), Decision::CastSpellOrPass { player: PlayerId::P1, activatable_abilities, .. } if activatable_abilities.contains(&(ranger, 0)))
    );
    engine::step(&mut state, Action::ActivateAbility(ranger, 0)).unwrap();
    assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
    for _ in 0..100 {
        let decision = next(&mut state);
        if matches!(decision, Decision::CastSpellOrPass { .. }) && state.stack.len() == 1 {
            break;
        }
        pass_or_order(&mut state, decision);
    }
    assert!(!state.objects.get(target).tapped);
    assert_eq!(state.objects.get(forest).zone, Zone::Hand);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
    drain(&mut state, None);
    assert_eq!(state.objects.get(target).zone, Zone::Graveyard);
}

#[test]
fn target_departure_or_reentry_does_not_destroy_a_new_incarnation_or_refund_payment() {
    for returns in [false, true] {
        let mut state = ready();
        let target = creature(&mut state, PlayerId::P1, "Elvish Mystic", true);
        state.players[0].mana_pool[ManaColor::W.pool_index()] = 2;
        let spell = cast(&mut state, target);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(target, Zone::Hand));
        if returns {
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(target, Zone::Battlefield),
            );
        }
        drain(&mut state, None);
        assert_eq!(
            state.objects.get(target).zone,
            if returns {
                Zone::Battlefield
            } else {
                Zone::Hand
            }
        );
        assert_eq!(state.objects.get(spell).zone, Zone::Graveyard);
        assert_eq!(state.players[0].mana_pool, [0; 6]);
    }
}

#[test]
fn ward_is_separate_from_the_discounted_casting_cost() {
    for (mana, payment, choices, destroyed) in [
        (4, Some(true), 1, true),
        (4, Some(false), 1, false),
        (2, None, 0, false),
    ] {
        let mut state = ready();
        let target = creature(&mut state, PlayerId::P1, "Cackling Prowler", true);
        state.players[0].mana_pool[ManaColor::W.pool_index()] = mana;
        cast(&mut state, target);
        assert_eq!(
            state.players[0].mana_pool[ManaColor::W.pool_index()],
            mana - 2
        );
        assert_eq!(drain(&mut state, payment), choices);
        assert_eq!(
            state.objects.get(target).zone,
            if destroyed {
                Zone::Graveyard
            } else {
                Zone::Battlefield
            }
        );
    }
}

#[test]
fn pending_ward_payment_restores_and_own_ward_does_not_trigger() {
    let mut state = ready();
    let target = creature(&mut state, PlayerId::P1, "Cackling Prowler", true);
    state.players[0].mana_pool[ManaColor::W.pool_index()] = 4;
    cast(&mut state, target);
    for _ in 0..2 {
        let decision = next(&mut state);
        pass_or_order(&mut state, decision);
    }
    assert!(matches!(
        next(&mut state),
        Decision::ChooseEffectBoolean {
            player: PlayerId::P0,
            purpose: EffectBooleanChoicePurpose::CounterUnlessPaysGeneric { generic: 2, .. },
            ..
        }
    ));
    let bytes = serde_json::to_vec(&state).unwrap();
    let mut restored: GameState = serde_json::from_slice(&bytes).unwrap();
    engine::step(&mut state, Action::ChooseEffectBoolean(true)).unwrap();
    engine::step(&mut restored, Action::ChooseEffectBoolean(true)).unwrap();
    drain(&mut state, None);
    drain(&mut restored, None);
    assert_eq!(state.state_hash(), restored.state_hash());
    assert_eq!(state.players[0].mana_pool, [0; 6]);

    let mut own = ready();
    let target = creature(&mut own, PlayerId::P0, "Cackling Prowler", true);
    own.players[0].mana_pool[ManaColor::W.pool_index()] = 2;
    cast(&mut own, target);
    assert_eq!(drain(&mut own, None), 0);
    assert_eq!(own.objects.get(target).zone, Zone::Graveyard);
}

#[test]
fn destroyed_creature_enables_the_controllers_prowler_end_step() {
    let mut state = ready();
    let prowler = creature(&mut state, PlayerId::P0, "Cackling Prowler", false);
    let target = creature(&mut state, PlayerId::P1, "Elvish Mystic", true);
    state.players[0].mana_pool[ManaColor::W.pool_index()] = 2;
    cast(&mut state, target);
    drain(&mut state, None);
    assert!(state.creature_died_this_turn_v1());
    state.step = Step::Main2;
    for _ in 0..100 {
        let decision = next(&mut state);
        if state.step == Step::End {
            break;
        }
        pass_or_order(&mut state, decision);
    }
    drain(&mut state, None);
    assert_eq!(state.objects.get(prowler).counters.plus1_plus1, 1);
}
