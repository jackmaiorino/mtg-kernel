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
    ready_for(PlayerId::P0)
}
fn ready_for(active: PlayerId) -> GameState {
    let island = card_id_by_name("Island").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[island; 40],
        &[island; 40],
        |_| "Island".into(),
        123,
        active,
    );
    state.step = Step::Main1;
    state.priority_player = PlayerId::P0;
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
    assert_eq!(
        ObjectStateV4::from_card_def(card_id_by_name("Sire of Seven Deaths").unwrap()).ward_generic,
        0
    );
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
                let observation = mtg_kernel::rl::observe_policy_v8(
                    &state,
                    &mtg_kernel::policy_surface_v5::PolicySurfaceV5::new(),
                    payer,
                    0,
                    0,
                    0,
                    1,
                )
                .unwrap();
                assert_eq!(observation.schema_version, 8);
                let typed = observation
                    .extensions
                    .pending_ward_life_payment
                    .as_ref()
                    .unwrap();
                assert_eq!(typed.life, 7);
                assert_eq!(typed.payer, payer.into());
                assert!(observation.extensions.pending_ward_payment.is_none());
                let decoded: mtg_kernel::policy_observation_v8::ObservationV8 =
                    serde_json::from_slice(&serde_json::to_vec(&observation).unwrap()).unwrap();
                assert_eq!(observation, decoded);
                assert!(mtg_kernel::rl::observe_policy_v6(
                    &state,
                    &mtg_kernel::policy_surface_v5::PolicySurfaceV5::new(),
                    payer,
                    0,
                    0,
                    0,
                    1,
                )
                .is_err());
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

#[test]
fn typed_public_life_ward_keeps_two_targeters_distinct() {
    let mut state = ready();
    let sire = put(
        &mut state,
        PlayerId::P1,
        "Sire of Seven Deaths",
        Zone::Battlefield,
    );
    let spells = ["Snap", "Snap"].map(|name| put(&mut state, PlayerId::P0, name, Zone::Hand));
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 4;
    for spell in spells {
        engine::step(&mut state, Action::CastSpell(spell)).unwrap();
        engine::step(&mut state, Action::ChooseTarget(Target::Object(sire))).unwrap();
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
    }
    let observation = mtg_kernel::rl::observe_policy_v8(
        &state,
        &mtg_kernel::policy_surface_v5::PolicySurfaceV5::new(),
        PlayerId::P0,
        0,
        0,
        0,
        1,
    )
    .unwrap();
    let wards = &observation.extensions.queued_ward_life_payments;
    assert_eq!(wards.len(), 2);
    assert!(observation.extensions.queued_ward_payments.is_empty());
    assert_ne!(
        wards[0].payment.targeting_stack_index,
        wards[1].payment.targeting_stack_index
    );
    for (ward, spell) in wards.iter().zip(spells) {
        assert_eq!(ward.payment.life, 7);
        let targeter =
            &observation.projection.surface.stack[ward.payment.targeting_stack_index as usize];
        assert_eq!(targeter.source.arena_id, spell.0);
        assert!(ward.payment.targeting_stack_index < ward.stack_index);
    }
    let mut surf = surface();
    pending_life_ward(&mut state, &mut surf);
    let Decision::ChooseEffectBoolean {
        purpose:
            EffectBooleanChoicePurpose::CounterUnlessPaysLife {
                targeting_stack_item,
                ..
            },
        ..
    } = next(&mut surf, &mut state)
    else {
        panic!("life Ward");
    };
    let exact = state
        .stack
        .iter()
        .find(|item| item.v4.stack_item_id == targeting_stack_item)
        .unwrap()
        .source;
    let other = *spells.iter().find(|&&spell| spell != exact).unwrap();
    engine::step(&mut state, Action::ChooseEffectBoolean(false)).unwrap();
    assert!(!matches!(
        engine::advance_until_decision(&mut state),
        Decision::Halted { .. }
    ));
    assert_eq!(state.objects.get(exact).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(other).zone, Zone::Stack);
    assert_eq!(state.players[0].life, 20);
}

#[test]
fn pre_target_life_ward_traits_preserve_visibility_and_actual_aura_suppression() {
    use mtg_kernel::policy_observation_v8::{ObservationV8, WardLifePrintedFaceV8};
    use mtg_kernel::policy_surface_v5::PolicySurfaceV5;
    let observe = |state: &GameState, actor| -> ObservationV8 {
        mtg_kernel::rl::observe_policy_v8(state, &PolicySurfaceV5::new(), actor, 0, 0, 0, 1)
            .unwrap()
    };
    for owner in [PlayerId::P0, PlayerId::P1] {
        let payer = owner.opponent();
        let mut state = ready_for(payer);
        state.priority_player = payer;
        let sire = put(&mut state, owner, "Sire of Seven Deaths", Zone::Battlefield);
        let own = put(&mut state, payer, "Sire of Seven Deaths", Zone::Hand);
        let hidden = put(&mut state, owner, "Sire of Seven Deaths", Zone::Hand);
        let library = put(&mut state, payer, "Sire of Seven Deaths", Zone::Hand);
        mtg_kernel::event::propose_and_commit(
            &mut state,
            mtg_kernel::event::ProposedEvent::zone_change(library, Zone::Library),
        );
        let before = observe(&state, payer);
        let traits = &before.extensions.ward_life_card_traits;
        assert_eq!(traits.len(), 2);
        assert!(traits
            .iter()
            .all(|row| row.life == 7 && row.printed_face == WardLifePrintedFaceV8::Front));
        assert!(
            traits
                .iter()
                .find(|row| row.source.arena_id == sire.0)
                .unwrap()
                .effective
        );
        assert!(
            !traits
                .iter()
                .find(|row| row.source.arena_id == own.0)
                .unwrap()
                .effective
        );
        assert!(traits
            .iter()
            .all(|row| row.source.arena_id != hidden.0 && row.source.arena_id != library.0));

        let mut surf = surface();
        let aura = cast_at(&mut surf, &mut state, payer, "Witness Protection", sire, 1);
        pending_life_ward(&mut state, &mut surf);
        let mut replay: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(observe(&state, payer), observe(&replay, payer));
        assert_eq!(drain(&mut surf, &mut state, Some(true)), 1);
        assert_eq!(drain(&mut surface(), &mut replay, Some(true)), 1);
        assert_eq!(state.state_hash(), replay.state_hash());
        assert_eq!(state.objects.get(aura).zone, Zone::Battlefield);
        assert_eq!(state.players[payer.index()].life, 13);
        for actor in [owner, payer] {
            let suppressed = observe(&state, actor);
            let row = suppressed
                .extensions
                .ward_life_card_traits
                .iter()
                .find(|row| row.source.arena_id == sire.0)
                .unwrap();
            assert_eq!(
                (row.life, row.printed_face),
                (7, WardLifePrintedFaceV8::Front)
            );
            assert!(!row.effective);
            let decoded: ObservationV8 =
                serde_json::from_slice(&serde_json::to_vec(&suppressed).unwrap()).unwrap();
            assert_eq!(decoded, suppressed);
        }
        mtg_kernel::event::propose_and_commit(
            &mut state,
            mtg_kernel::event::ProposedEvent::zone_change(aura, Zone::Graveyard),
        );
        assert!(
            observe(&state, payer)
                .extensions
                .ward_life_card_traits
                .iter()
                .find(|row| row.source.arena_id == sire.0)
                .unwrap()
                .effective
        );
    }
}
