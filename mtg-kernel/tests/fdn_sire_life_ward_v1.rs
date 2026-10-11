//! Unregistered Sire fixtures; gameplay admission remains pending.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardType, Keywords, Subtype, WardCostDef, CARD_DEFS,
};
use mtg_kernel::effect::{EffectBooleanChoicePurpose, PendingEffectChoice};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use mtg_kernel::surface_v2::{
    HarnessSurfaceV2, PriorityModeV1, SuppressionAuditMode, SurfaceAction, SurfaceDecision,
};
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
        name: name.into(),
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

fn surface() -> HarnessSurfaceV2 {
    HarnessSurfaceV2::new_with_priority_mode_v1(
        PriorityModeV1::EngineWindowsV1,
        SuppressionAuditMode::Full,
    )
}

fn next(surface: &mut HarnessSurfaceV2, state: &mut GameState) -> Decision {
    match surface.next_decision(state) {
        SurfaceDecision::Decision(decision @ Decision::Halted { .. }) => panic!("{decision:?}"),
        SurfaceDecision::Decision(decision) => decision,
        other => panic!("expected decision: {other:?}"),
    }
}

fn apply(surface: &mut HarnessSurfaceV2, state: &mut GameState, action: Action) {
    surface.apply(state, SurfaceAction::Action(action)).unwrap();
}

fn drain(surface: &mut HarnessSurfaceV2, state: &mut GameState, payment: Option<bool>) -> usize {
    let mut payment_choices = 0;
    for _ in 0..100 {
        let decision = next(surface, state);
        if matches!(decision, Decision::CastSpellOrPass { .. }) && state.stack.is_empty() {
            assert!(state.engine.pending_triggers.is_empty());
            return payment_choices;
        }
        let action = match decision {
            Decision::OrderTriggers { pending, .. } => {
                Action::OrderTriggers((0..pending.len()).collect())
            }
            Decision::CastSpellOrPass { .. } => Action::Pass,
            Decision::ChooseEffectBoolean { .. } => {
                payment_choices += 1;
                Action::ChooseEffectBoolean(payment.expect("explicit payment answer"))
            }
            other => panic!("unexpected resolution decision {other:?}"),
        };
        apply(surface, state, action);
    }
    panic!("resolution did not finish");
}

fn cast_at(
    surface: &mut HarnessSurfaceV2,
    state: &mut GameState,
    actor: PlayerId,
    name: &str,
    target: ObjectId,
    blue_mana: u8,
) -> ObjectId {
    let spell = put(state, actor, name, Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = blue_mana;
    let decision = next(surface, state);
    if matches!(decision, Decision::CastSpellOrPass { player, .. } if player != actor) {
        apply(surface, state, Action::Pass);
    }
    assert!(
        matches!(next(surface, state), Decision::CastSpellOrPass { player, castable_spells, .. }
        if player == actor && castable_spells.contains(&spell))
    );
    apply(surface, state, Action::CastSpell(spell));
    assert!(
        matches!(next(surface, state), Decision::ChooseTargets { legal_targets, .. }
        if legal_targets.contains(&Target::Object(target)))
    );
    apply(surface, state, Action::ChooseTarget(Target::Object(target)));
    spell
}

#[test]
fn sire_printed_shape_has_six_keywords_and_life_ward() {
    let def = &CARD_DEFS[card_id_by_name("Sire of Seven Deaths").unwrap() as usize];
    assert_eq!(def.capability, CardCapability::Full);
    assert_eq!(def.types, &[CardType::Creature]);
    assert_eq!(def.subtypes, &[Subtype::Eldrazi]);
    assert_eq!(def.cost.generic, 7);
    assert!(def.cost.pips.is_empty());
    assert!(def.colors.is_empty());
    assert_eq!((def.power, def.toughness), (Some(7), Some(7)));
    for keyword in [
        Keywords::FIRST_STRIKE,
        Keywords::VIGILANCE,
        Keywords::MENACE,
        Keywords::TRAMPLE,
        Keywords::REACH,
        Keywords::LIFELINK,
    ] {
        assert!(def.keywords.has(keyword));
    }
    assert!(!def.keywords.has(Keywords::DEATHTOUCH));
    assert_eq!(def.ward_cost, Some(WardCostDef::PayLife(7)));
}

fn pending_life_ward(state: &mut GameState, surf: &mut HarnessSurfaceV2) {
    for _ in 0..32 {
        match next(surf, state) {
            Decision::ChooseEffectBoolean {
                purpose: EffectBooleanChoicePurpose::CounterUnlessPaysLife { life: 7, .. },
                ..
            } => return,
            Decision::CastSpellOrPass { .. } => apply(surf, state, Action::Pass),
            Decision::OrderTriggers { pending, .. } => apply(
                surf,
                state,
                Action::OrderTriggers((0..pending.len()).collect()),
            ),
            other => panic!("unexpected Ward decision {other:?}"),
        }
    }
    panic!("life Ward did not suspend");
}

#[test]
fn pay_decline_and_insufficient_life_preserve_mana_and_restore() {
    for owner in [PlayerId::P0, PlayerId::P1] {
        let payer = owner.opponent();
        for (life, payment, choices, final_life, zone) in [
            (20, Some(true), 1, 13, Zone::Hand),
            (20, Some(false), 1, 20, Zone::Battlefield),
            (6, None, 0, 6, Zone::Battlefield),
        ] {
            let mut state = ready();
            let sire = put(&mut state, owner, "Sire of Seven Deaths", Zone::Battlefield);
            state.players[payer.index()].life = life;
            let mut surf = surface();
            let bounce = cast_at(&mut surf, &mut state, payer, "Snap", sire, 2);
            let mut restored = None;
            if payment.is_some() {
                pending_life_ward(&mut state, &mut surf);
                restored = Some(
                    serde_json::from_slice::<GameState>(&serde_json::to_vec(&state).unwrap())
                        .unwrap(),
                );
            }
            assert_eq!(drain(&mut surf, &mut state, payment), choices);
            if let Some(mut restored) = restored {
                assert_eq!(drain(&mut surface(), &mut restored, payment), choices);
                assert_eq!(state.state_hash(), restored.state_hash());
            }
            assert_eq!(state.objects.get(sire).zone, zone);
            assert_eq!(state.objects.get(bounce).zone, Zone::Graveyard);
            assert_eq!(state.players[payer.index()].life, final_life);
            assert_eq!(state.players[payer.index()].mana_pool, [0; 6]);
        }
    }
}

#[test]
fn own_targeting_has_no_payment_and_exact_life_payment_is_legal() {
    let mut state = ready();
    let sire = put(
        &mut state,
        PlayerId::P0,
        "Sire of Seven Deaths",
        Zone::Battlefield,
    );
    let mut surf = surface();
    cast_at(&mut surf, &mut state, PlayerId::P0, "Snap", sire, 2);
    assert_eq!(drain(&mut surf, &mut state, None), 0);
    assert_eq!(state.objects.get(sire).zone, Zone::Hand);

    let mut state = ready();
    let sire = put(
        &mut state,
        PlayerId::P0,
        "Sire of Seven Deaths",
        Zone::Battlefield,
    );
    state.players[1].life = 7;
    let mut surf = surface();
    cast_at(&mut surf, &mut state, PlayerId::P1, "Snap", sire, 2);
    pending_life_ward(&mut state, &mut surf);
    let mut restored: GameState =
        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    for current in [&mut state, &mut restored] {
        engine::step(current, Action::ChooseEffectBoolean(true)).unwrap();
        assert_eq!(current.players[1].life, 7);
        let answered_bytes = serde_json::to_vec(current).unwrap();
        *current = serde_json::from_slice(&answered_bytes).unwrap();
        assert!(matches!(
            engine::advance_until_decision(current),
            Decision::GameOver {
                winner: Some(PlayerId::P0)
            }
        ));
        assert_eq!(current.players[1].life, 0);
    }
    assert_eq!(state.state_hash(), restored.state_hash());
}

#[test]
fn restored_life_payment_refuses_forged_fee_payer_targeter_and_stage() {
    let mut state = ready();
    let sire = put(
        &mut state,
        PlayerId::P0,
        "Sire of Seven Deaths",
        Zone::Battlefield,
    );
    let mut surf = surface();
    cast_at(&mut surf, &mut state, PlayerId::P1, "Snap", sire, 2);
    pending_life_ward(&mut state, &mut surf);
    let saved = serde_json::to_vec(&state).unwrap();
    for forged in 0..5 {
        let mut restored: GameState = serde_json::from_slice(&saved).unwrap();
        let pending = restored.engine.pending_effect.as_mut().unwrap();
        let Some(PendingEffectChoice::ChooseBoolean {
            purpose:
                EffectBooleanChoicePurpose::CounterUnlessPaysLife {
                    player,
                    life,
                    targeting_stack_item,
                    ..
                },
            ..
        }) = pending.choice.as_mut()
        else {
            panic!("life payment");
        };
        match forged {
            0 => *life = 6,
            1 => *player = PlayerId::P0,
            2 => *targeting_stack_item = Default::default(),
            _ => {}
        }
        if forged == 3 {
            pending.choice = None;
        } else if forged == 4 {
            pending.choice = Some(PendingEffectChoice::ChooseBoolean {
                player: PlayerId::P1,
                path: vec![],
                default: Some(false),
                purpose: EffectBooleanChoicePurpose::ShuffleLibrary {
                    player: PlayerId::P1,
                },
            });
        }
        let before = restored.state_hash();
        assert!(engine::step(&mut restored, Action::ChooseEffectBoolean(true)).is_err());
        assert_eq!(restored.state_hash(), before);
    }
}
