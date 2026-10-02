//! Targeted FDN spells exercised through cast, target and resolution decisions.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, preflight_fully_supported_deck, CardCapability, Keywords, TargetSpec,
    CARD_DEFS,
};
use mtg_kernel::combat_damage_v1::enable_foundations_combat_v1;
use mtg_kernel::engine::{self, Action, Decision, EffectDuration, Layers, UntilEndOfTurnEffect};
use mtg_kernel::event::{
    self, ActiveReplacement, CommittedEvent, ProposedEvent, ReplacementEffectKind,
};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::planeswalker_v1;
use mtg_kernel::rl;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use mtg_kernel::surface_v2::{
    HarnessSurfaceV2, PriorityModeV1, SuppressionAuditMode, SurfaceAction, SurfaceDecision,
};

const PW: &str = "Ajani, Caller of the Pride";

fn ready() -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state =
        GameState::new_from_libraries(&[forest; 40], &[forest; 40], |_| "Forest".into(), 123);
    state.step = Step::Main1;
    enable_foundations_combat_v1(&mut state).unwrap();
    state.engine.combat.attackers_declared = true;
    state.engine.combat.blockers_declared = true;
    state
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap();
    let id = state.objects.push(GameObject {
        card_def,
        name: name.into(),
        owner: player,
        controller: player,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: Vec::new(),
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[player.index()].hand.push(id),
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        _ => panic!("helper zone"),
    }
    id
}

fn move_to(state: &mut GameState, id: ObjectId, zone: Zone) {
    event::propose_and_commit(state, ProposedEvent::zone_change(id, zone));
}

fn planeswalker(state: &mut GameState, player: PlayerId) -> ObjectId {
    let id = put(state, player, PW, Zone::Hand);
    move_to(state, id, Zone::Battlefield);
    assert_eq!(planeswalker_v1::loyalty(state, id), Some(4));
    id
}

fn surface() -> HarnessSurfaceV2 {
    HarnessSurfaceV2::new_with_priority_mode_v1(
        PriorityModeV1::EngineWindowsV1,
        SuppressionAuditMode::Full,
    )
}

fn next(surface: &mut HarnessSurfaceV2, state: &mut GameState) -> Decision {
    match surface.next_decision(state) {
        SurfaceDecision::Decision(decision) => decision,
        other => panic!("expected decision: {other:?}"),
    }
}

fn priority(surface: &mut HarnessSurfaceV2, state: &mut GameState) -> Decision {
    let decision = next(surface, state);
    assert!(
        matches!(decision, Decision::CastSpellOrPass { .. }),
        "{decision:?}"
    );
    decision
}

fn apply(surface: &mut HarnessSurfaceV2, state: &mut GameState, action: Action) {
    surface.apply(state, SurfaceAction::Action(action)).unwrap();
}

fn cast(
    surface: &mut HarnessSurfaceV2,
    state: &mut GameState,
    name: &str,
    targets: &[ObjectId],
) -> ObjectId {
    let spell = put(state, PlayerId::P0, name, Zone::Hand);
    state.players[0].mana_pool = [10; 6];
    assert!(
        matches!(priority(surface, state), Decision::CastSpellOrPass { castable_spells, .. }
        if castable_spells.contains(&spell))
    );
    apply(surface, state, Action::CastSpell(spell));
    for &target in targets {
        assert!(
            matches!(next(surface, state), Decision::ChooseTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(target)))
        );
        apply(surface, state, Action::ChooseTarget(Target::Object(target)));
    }
    priority(surface, state);
    spell
}

fn resolve(surface: &mut HarnessSurfaceV2, state: &mut GameState) {
    let before = state.stack.len();
    assert!(before > 0);
    apply(surface, state, Action::Pass);
    priority(surface, state);
    apply(surface, state, Action::Pass);
    priority(surface, state);
    assert_eq!(state.stack.len(), before - 1);
    assert!(surface.suppressions().is_empty());
}

fn grant(state: &mut GameState, id: ObjectId, keywords: Keywords) {
    state
        .engine
        .until_end_of_turn
        .push(UntilEndOfTurnEffect::ResolvedObjectKeywordEffect {
            object_id: id,
            object_zone_change_count: state.objects.get(id).zone_change_count,
            layer: Layers::ABILITY_ADDING,
            timestamp: 1,
            duration: EffectDuration::EndOfTurn,
            keywords,
        });
}

fn dealt(state: &GameState, source: ObjectId, target: ObjectId) -> i32 {
    state
        .engine
        .event_history
        .iter()
        .filter_map(|event| match event {
            CommittedEvent::Damage {
                source: actual,
                target: Target::Object(actual_target),
                amount,
            } if *actual == source && *actual_target == target => Some(*amount),
            _ => None,
        })
        .sum()
}

fn combat_damage(state: &mut GameState, source: ObjectId, target: ObjectId, amount: i32) {
    let mut event = ProposedEvent::damage(source, Target::Object(target), amount);
    let ProposedEvent::Damage(damage) = &mut event else {
        unreachable!()
    };
    damage.is_combat = true;
    event::propose_and_commit(state, event);
}

#[test]
fn appended_spell_definitions_and_partial_planeswalker_admission_are_explicit() {
    for (offset, name) in [
        "Bite Down",
        "Felling Blow",
        "Fleeting Flight",
        "Joust Through",
    ]
    .into_iter()
    .enumerate()
    {
        let id = card_id_by_name(name).unwrap();
        assert_eq!(usize::from(id), 211 + offset);
        assert_eq!(CARD_DEFS[usize::from(id)].capability, CardCapability::Full);
        preflight_fully_supported_deck(&[id]).unwrap();
    }
    let id = card_id_by_name(PW).unwrap();
    assert_eq!(id, 215);
    assert_eq!(
        CARD_DEFS[usize::from(id)].capability,
        CardCapability::Partial
    );
    assert!(preflight_fully_supported_deck(&[id]).is_err());
    assert_eq!(
        TargetSpec::ControlledCreatureThenOpponentCreature.stable_id(),
        38
    );
    assert_eq!(
        TargetSpec::ControlledCreatureThenOpponentCreatureOrPlaneswalker.stable_id(),
        39
    );
    assert_eq!(TargetSpec::AttackingOrBlockingCreature.stable_id(), 40);
}

#[test]
fn power_spells_offer_controlled_source_then_only_their_printed_victim_kind() {
    for name in ["Bite Down", "Felling Blow"] {
        let mut state = ready();
        let own = put(
            &mut state,
            PlayerId::P0,
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let other = put(&mut state, PlayerId::P1, "Myr Enforcer", Zone::Battlefield);
        let pw = planeswalker(&mut state, PlayerId::P1);
        let own_pw = planeswalker(&mut state, PlayerId::P0);
        let rock = put(
            &mut state,
            PlayerId::P1,
            "Ichor Wellspring",
            Zone::Battlefield,
        );
        let spell = put(&mut state, PlayerId::P0, name, Zone::Hand);
        state.players[0].mana_pool = [10; 6];
        let mut surface = surface();
        priority(&mut surface, &mut state);
        apply(&mut surface, &mut state, Action::CastSpell(spell));
        let Decision::ChooseTargets { legal_targets, .. } = next(&mut surface, &mut state) else {
            panic!("source choice")
        };
        assert_eq!(legal_targets, vec![Target::Object(own)]);
        assert!(engine::step(&mut state, Action::ChooseTarget(Target::Object(other))).is_err());
        apply(
            &mut surface,
            &mut state,
            Action::ChooseTarget(Target::Object(own)),
        );
        let Decision::ChooseTargets { legal_targets, .. } = next(&mut surface, &mut state) else {
            panic!("victim choice")
        };
        assert!(legal_targets.contains(&Target::Object(other)));
        assert_eq!(
            legal_targets.contains(&Target::Object(pw)),
            name == "Bite Down"
        );
        for target in [
            Target::Object(own),
            Target::Object(own_pw),
            Target::Object(rock),
            Target::Player(PlayerId::P1),
        ] {
            assert!(!legal_targets.contains(&target));
        }
    }
}

#[test]
fn felling_blow_uses_sorcery_timing_and_the_four_spells_pay_exact_costs() {
    for (name, mana, instant) in [
        ("Bite Down", [0, 0, 0, 0, 1, 1], true),
        ("Felling Blow", [0, 0, 0, 0, 1, 2], false),
        ("Fleeting Flight", [1, 0, 0, 0, 0, 0], true),
        ("Joust Through", [1, 0, 0, 0, 0, 0], true),
    ] {
        let mut state = ready();
        let own = put(
            &mut state,
            PlayerId::P0,
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let other = put(&mut state, PlayerId::P1, "Myr Enforcer", Zone::Battlefield);
        state.engine.combat.attackers.push(other);
        let spell = put(&mut state, PlayerId::P0, name, Zone::Hand);
        state.players[0].mana_pool = mana;
        state.active_player = PlayerId::P1;
        let mut surface = surface();
        assert!(
            matches!(priority(&mut surface, &mut state), Decision::CastSpellOrPass { castable_spells, .. }
            if castable_spells.contains(&spell) == instant)
        );
        state.active_player = PlayerId::P0;
        apply(&mut surface, &mut state, Action::CastSpell(spell));
        let targets = if name == "Bite Down" || name == "Felling Blow" {
            vec![own, other]
        } else {
            vec![other]
        };
        for target in targets {
            next(&mut surface, &mut state);
            apply(
                &mut surface,
                &mut state,
                Action::ChooseTarget(Target::Object(target)),
            );
        }
        priority(&mut surface, &mut state);
        assert_eq!(state.players[0].mana_pool, [0; 6]);
    }
}

#[test]
fn bite_down_uses_current_power_and_creature_lifelink_and_deathtouch() {
    for name in ["Guarded Heir", "Treetop Snarespinner"] {
        let mut state = ready();
        let source = put(&mut state, PlayerId::P0, name, Zone::Battlefield);
        let victim = put(&mut state, PlayerId::P1, "Myr Enforcer", Zone::Battlefield);
        let mut surface = surface();
        let spell = cast(&mut surface, &mut state, "Bite Down", &[source, victim]);
        state.objects.get_mut(source).counters.plus1_plus1 = 1;
        let power = engine::effective_power(&state, source);
        resolve(&mut surface, &mut state);
        assert_eq!(dealt(&state, source, victim), power);
        assert_eq!(dealt(&state, spell, victim), 0);
        assert_eq!(
            state.players[0].life,
            if name == "Guarded Heir" {
                20 + power
            } else {
                20
            }
        );
        assert_eq!(
            state.objects.get(victim).zone,
            if name == "Guarded Heir" {
                Zone::Battlefield
            } else {
                Zone::Graveyard
            }
        );
    }
}

#[test]
fn bite_down_nonpositive_power_deals_no_damage() {
    let mut state = ready();
    let source = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let victim = put(&mut state, PlayerId::P1, "Myr Enforcer", Zone::Battlefield);
    let mut surface = surface();
    cast(&mut surface, &mut state, "Bite Down", &[source, victim]);
    state.objects.get_mut(source).counters.minus1_minus1 = 1;
    state.objects.get_mut(source).counters.plus1_plus1 = 0;
    // Keep the 0-power source alive by adding toughness through a resolved boost.
    state
        .engine
        .until_end_of_turn
        .push(UntilEndOfTurnEffect::ResolvedObjectEffect {
            object_id: source,
            object_zone_change_count: state.objects.get(source).zone_change_count,
            layer: Layers::POWER_TOUGHNESS,
            timestamp: 1,
            duration: EffectDuration::EndOfTurn,
            power: 0,
            toughness: 2,
            grant_haste: false,
        });
    resolve(&mut surface, &mut state);
    assert_eq!(dealt(&state, source, victim), 0);
}

#[test]
fn felling_blow_adds_counter_before_reading_power() {
    let mut state = ready();
    let source = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let victim = put(&mut state, PlayerId::P1, "Myr Enforcer", Zone::Battlefield);
    let mut surface = surface();
    cast(&mut surface, &mut state, "Felling Blow", &[source, victim]);
    resolve(&mut surface, &mut state);
    assert_eq!(state.objects.get(source).counters.plus1_plus1, 1);
    assert_eq!(dealt(&state, source, victim), 2);
    assert_eq!(state.objects.get(victim).damage, 2);
}

#[test]
fn felling_blow_checks_targets_individually_and_rejects_returned_incarnations() {
    for invalid_source in [false, true] {
        for invalid_victim in [false, true] {
            for blink in [false, true] {
                let mut state = ready();
                let source = put(
                    &mut state,
                    PlayerId::P0,
                    "Llanowar Elves",
                    Zone::Battlefield,
                );
                let victim = put(&mut state, PlayerId::P1, "Myr Enforcer", Zone::Battlefield);
                let mut surface = surface();
                cast(&mut surface, &mut state, "Felling Blow", &[source, victim]);
                for (invalid, id) in [(invalid_source, source), (invalid_victim, victim)] {
                    if invalid {
                        move_to(&mut state, id, Zone::Hand);
                        if blink {
                            move_to(&mut state, id, Zone::Battlefield);
                        }
                    }
                }
                resolve(&mut surface, &mut state);
                assert_eq!(
                    state.objects.get(source).counters.plus1_plus1,
                    i16::from(!invalid_source)
                );
                assert_eq!(
                    dealt(&state, source, victim),
                    if invalid_source || invalid_victim {
                        0
                    } else {
                        2
                    }
                );
            }
        }
    }
}

#[test]
fn felling_blow_rechecks_control_and_hexproof_when_it_resolves() {
    for change in [0, 1, 2] {
        let mut state = ready();
        let source = put(
            &mut state,
            PlayerId::P0,
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let victim = put(&mut state, PlayerId::P1, "Myr Enforcer", Zone::Battlefield);
        let mut surface = surface();
        cast(&mut surface, &mut state, "Felling Blow", &[source, victim]);
        match change {
            0 => state.objects.get_mut(source).controller = PlayerId::P1,
            1 => state.objects.get_mut(victim).controller = PlayerId::P0,
            _ => grant(&mut state, victim, Keywords::HEXPROOF),
        }
        resolve(&mut surface, &mut state);
        assert_eq!(
            state.objects.get(source).counters.plus1_plus1,
            i16::from(change != 0)
        );
        assert_eq!(dealt(&state, source, victim), 0);
    }
}

#[test]
fn bite_down_removes_loyalty_and_zero_loyalty_ignores_indestructible() {
    for power in [1, 4] {
        let mut state = ready();
        let source = put(
            &mut state,
            PlayerId::P0,
            "Llanowar Elves",
            Zone::Battlefield,
        );
        state.objects.get_mut(source).counters.plus1_plus1 = power - 1;
        let victim = planeswalker(&mut state, PlayerId::P1);
        grant(&mut state, victim, Keywords::INDESTRUCTIBLE);
        let mut surface = surface();
        cast(&mut surface, &mut state, "Bite Down", &[source, victim]);
        resolve(&mut surface, &mut state);
        assert_eq!(state.objects.get(victim).damage, 0);
        assert_eq!(
            planeswalker_v1::loyalty(&state, victim),
            if power == 4 { None } else { Some(3) }
        );
        assert_eq!(
            state.objects.get(victim).zone,
            if power == 4 {
                Zone::Graveyard
            } else {
                Zone::Battlefield
            }
        );
        if power == 4 {
            assert!(state.planeswalkers_v1.is_none());
        }
    }
}

#[test]
fn planeswalker_reentry_resets_loyalty_and_pending_bite_does_not_follow_it() {
    let mut state = ready();
    let source = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let victim = planeswalker(&mut state, PlayerId::P1);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::damage(source, Target::Object(victim), 2),
    );
    assert_eq!(planeswalker_v1::loyalty(&state, victim), Some(2));
    let mut surface = surface();
    cast(&mut surface, &mut state, "Bite Down", &[source, victim]);
    move_to(&mut state, victim, Zone::Hand);
    assert!(state.planeswalkers_v1.is_none());
    move_to(&mut state, victim, Zone::Battlefield);
    resolve(&mut surface, &mut state);
    assert_eq!(planeswalker_v1::loyalty(&state, victim), Some(4));
}

#[test]
fn pending_second_target_restores_with_the_same_decision_and_loyalty_result() {
    let mut state = ready();
    let source = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let victim = planeswalker(&mut state, PlayerId::P1);
    let spell = put(&mut state, PlayerId::P0, "Bite Down", Zone::Hand);
    state.players[0].mana_pool = [10; 6];
    let mut surface = surface();
    priority(&mut surface, &mut state);
    apply(&mut surface, &mut state, Action::CastSpell(spell));
    next(&mut surface, &mut state);
    apply(
        &mut surface,
        &mut state,
        Action::ChooseTarget(Target::Object(source)),
    );
    let decision = next(&mut surface, &mut state);
    let mut restored: GameState =
        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    assert_eq!(state.state_hash(), restored.state_hash());
    let mut restored_surface = self::surface();
    assert_eq!(decision, next(&mut restored_surface, &mut restored));
    for (surface, state) in [
        (&mut surface, &mut state),
        (&mut restored_surface, &mut restored),
    ] {
        apply(surface, state, Action::ChooseTarget(Target::Object(victim)));
        priority(surface, state);
        resolve(surface, state);
        assert_eq!(planeswalker_v1::loyalty(state, victim), Some(3));
    }
    assert_eq!(
        serde_json::to_vec(&state).unwrap(),
        serde_json::to_vec(&restored).unwrap()
    );
}

#[test]
fn public_projection_exposes_planeswalker_loyalty_without_hidden_state() {
    let mut state = ready();
    let victim = planeswalker(&mut state, PlayerId::P1);
    let mut surface = surface();
    priority(&mut surface, &mut state);
    let observation = rl::observe_v2(&state, &surface, PlayerId::P0, 0).unwrap();
    let projected = observation.projection.engine_context.planeswalkers.unwrap();
    assert_eq!(projected.len(), 1);
    assert_eq!(projected[0].permanent.arena_id, victim.0);
    assert_eq!(projected[0].loyalty, 4);
}

#[test]
fn fleeting_flight_adds_counter_and_flying_and_prevents_only_incoming_combat_damage() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready();
        let target = put(&mut state, player, "Myr Enforcer", Zone::Battlefield);
        let source = put(
            &mut state,
            if player == PlayerId::P0 {
                PlayerId::P1
            } else {
                PlayerId::P0
            },
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let mut surface = surface();
        cast(&mut surface, &mut state, "Fleeting Flight", &[target]);
        resolve(&mut surface, &mut state);
        assert_eq!(engine::effective_power(&state, target), 5);
        assert!(engine::has_effective_keyword(
            &state,
            target,
            Keywords::FLYING
        ));
        combat_damage(&mut state, source, target, 9);
        assert_eq!(dealt(&state, source, target), 0);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::damage(source, Target::Object(target), 2),
        );
        assert_eq!(state.objects.get(target).damage, 2);
        combat_damage(&mut state, target, source, 1);
        assert_eq!(dealt(&state, target, source), 1);
    }
}

#[test]
fn fleeting_flight_illegal_target_gets_no_counter_flying_or_prevention() {
    for blink in [false, true] {
        let mut state = ready();
        let target = put(&mut state, PlayerId::P1, "Myr Enforcer", Zone::Battlefield);
        let mut surface = surface();
        cast(&mut surface, &mut state, "Fleeting Flight", &[target]);
        move_to(&mut state, target, Zone::Hand);
        if blink {
            move_to(&mut state, target, Zone::Battlefield);
        }
        resolve(&mut surface, &mut state);
        assert_eq!(state.objects.get(target).counters.plus1_plus1, 0);
        assert!(!engine::has_effective_keyword(
            &state,
            target,
            Keywords::FLYING
        ));
        assert!(state.engine.active_replacements.is_empty());
    }
}

#[test]
fn flight_shield_and_flying_do_not_follow_a_returned_creature() {
    let mut state = ready();
    let target = put(&mut state, PlayerId::P0, "Myr Enforcer", Zone::Battlefield);
    let source = put(
        &mut state,
        PlayerId::P1,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let mut surface = surface();
    cast(&mut surface, &mut state, "Fleeting Flight", &[target]);
    resolve(&mut surface, &mut state);
    move_to(&mut state, target, Zone::Hand);
    move_to(&mut state, target, Zone::Battlefield);
    combat_damage(&mut state, source, target, 2);
    assert_eq!(state.objects.get(target).damage, 2);
    assert_eq!(state.objects.get(target).counters.plus1_plus1, 0);
    assert!(!engine::has_effective_keyword(
        &state,
        target,
        Keywords::FLYING
    ));
    let observation = rl::observe_v2(&state, &surface, PlayerId::P0, 0).unwrap();
    assert!(observation
        .projection
        .engine_context
        .combat_damage_prevention
        .is_none());
}

#[test]
fn flight_cleanup_removes_flying_and_prevention_but_keeps_the_counter() {
    let mut state = ready();
    let target = put(&mut state, PlayerId::P0, "Myr Enforcer", Zone::Battlefield);
    let source = put(
        &mut state,
        PlayerId::P1,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let mut surface = surface();
    cast(&mut surface, &mut state, "Fleeting Flight", &[target]);
    resolve(&mut surface, &mut state);
    state.step = Step::End;
    state.engine.priority_passes = [false, false];
    apply(&mut surface, &mut state, Action::Pass);
    priority(&mut surface, &mut state);
    apply(&mut surface, &mut state, Action::Pass);
    priority(&mut surface, &mut state);
    assert!(state.engine.active_replacements.is_empty());
    assert!(!engine::has_effective_keyword(
        &state,
        target,
        Keywords::FLYING
    ));
    assert_eq!(state.objects.get(target).counters.plus1_plus1, 1);
    combat_damage(&mut state, source, target, 2);
    assert_eq!(state.objects.get(target).damage, 2);
}

#[test]
fn flaring_pain_bypasses_fleeting_flight_prevention() {
    let mut state = ready();
    let target = put(&mut state, PlayerId::P0, "Myr Enforcer", Zone::Battlefield);
    let source = put(
        &mut state,
        PlayerId::P1,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let mut surface = surface();
    cast(&mut surface, &mut state, "Fleeting Flight", &[target]);
    resolve(&mut surface, &mut state);
    cast(&mut surface, &mut state, "Flaring Pain", &[]);
    resolve(&mut surface, &mut state);
    combat_damage(&mut state, source, target, 3);
    assert_eq!(state.objects.get(target).damage, 3);
}

#[test]
fn prevention_snapshot_and_public_projection_restore_exactly() {
    let mut state = ready();
    let target = put(&mut state, PlayerId::P0, "Myr Enforcer", Zone::Battlefield);
    let source = put(
        &mut state,
        PlayerId::P1,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let mut surface = surface();
    cast(&mut surface, &mut state, "Fleeting Flight", &[target]);
    resolve(&mut surface, &mut state);
    let before = rl::observe_v2(&state, &surface, PlayerId::P0, 0).unwrap();
    let shield = before
        .projection
        .engine_context
        .combat_damage_prevention
        .as_ref()
        .unwrap();
    assert_eq!(shield.len(), 1);
    assert_eq!(shield[0].permanent.arena_id, target.0);
    let mut restored: GameState =
        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    assert_eq!(state.state_hash(), restored.state_hash());
    assert_eq!(
        before,
        rl::observe_v2(&restored, &surface, PlayerId::P0, 0).unwrap()
    );
    for game in [&mut state, &mut restored] {
        combat_damage(game, source, target, 10);
    }
    assert_eq!(
        serde_json::to_vec(&state).unwrap(),
        serde_json::to_vec(&restored).unwrap()
    );
}

#[test]
fn actual_foundations_combat_prevents_damage_before_lifelink_and_deathtouch() {
    let mut state = ready();
    let attacker = put(&mut state, PlayerId::P0, "Guarded Heir", Zone::Battlefield);
    grant(&mut state, attacker, Keywords::DEATHTOUCH);
    let blocker = put(&mut state, PlayerId::P1, "Myr Enforcer", Zone::Battlefield);
    let mut surface = surface();
    cast(&mut surface, &mut state, "Fleeting Flight", &[blocker]);
    resolve(&mut surface, &mut state);
    state.step = Step::DeclareBlockers;
    state.engine.combat.attackers = vec![attacker];
    state.engine.combat.blocked_by = vec![(attacker, vec![blocker])];
    state.engine.priority_passes = [false, false];
    priority(&mut surface, &mut state);
    apply(&mut surface, &mut state, Action::Pass);
    priority(&mut surface, &mut state);
    apply(&mut surface, &mut state, Action::Pass);
    priority(&mut surface, &mut state);
    assert_eq!(state.objects.get(blocker).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(blocker).damage, 0);
    assert!(!state.objects.get(blocker).v4.deathtouch_damage);
    assert_eq!(state.players[0].life, 20);
    assert_eq!(dealt(&state, blocker, attacker), 5);
    assert_eq!(state.objects.get(attacker).zone, Zone::Graveyard);
}

#[test]
fn trample_assignment_counts_lethal_despite_flight_prevention_and_lifelink_uses_dealt_damage() {
    let mut state = ready();
    let attacker = put(&mut state, PlayerId::P0, "Myr Enforcer", Zone::Battlefield);
    grant(&mut state, attacker, Keywords::TRAMPLE | Keywords::LIFELINK);
    let blocker = put(
        &mut state,
        PlayerId::P1,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let mut surface = surface();
    cast(&mut surface, &mut state, "Fleeting Flight", &[blocker]);
    resolve(&mut surface, &mut state);
    state.step = Step::DeclareBlockers;
    state.engine.combat.attackers = vec![attacker];
    state.engine.combat.blocked_by = vec![(attacker, vec![blocker])];
    state.engine.priority_passes = [false, false];
    priority(&mut surface, &mut state);
    apply(&mut surface, &mut state, Action::Pass);
    priority(&mut surface, &mut state);
    apply(&mut surface, &mut state, Action::Pass);
    let mut choices = 0;
    for _ in 0..16 {
        match next(&mut surface, &mut state) {
            Decision::ChooseCombatDamageRange {
                source,
                recipient,
                minimum,
                maximum,
                split_at,
                ..
            } => {
                assert_eq!(source, attacker);
                assert_eq!(recipient, Target::Object(blocker));
                assert!(minimum <= 2 && maximum >= 2);
                apply(
                    &mut surface,
                    &mut state,
                    Action::ChooseCombatDamageRange {
                        upper_half: 2 > split_at,
                    },
                );
                choices += 1;
            }
            Decision::CastSpellOrPass { .. } => break,
            other => panic!("unexpected combat choice: {other:?}"),
        }
    }
    assert!(choices > 0);
    assert_eq!(state.step, Step::CombatDamage);
    assert_eq!(dealt(&state, attacker, blocker), 0);
    assert_eq!(state.objects.get(blocker).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(attacker).damage, 2);
    assert_eq!(state.players[0].life, 22);
    assert_eq!(state.players[1].life, 18);
}

#[test]
fn joust_through_offers_only_current_attackers_and_blockers_of_either_player() {
    let mut state = ready();
    let attacker = put(&mut state, PlayerId::P0, "Myr Enforcer", Zone::Battlefield);
    let blocker = put(&mut state, PlayerId::P1, "Myr Enforcer", Zone::Battlefield);
    let idle = put(
        &mut state,
        PlayerId::P1,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    state.engine.combat.attackers.push(attacker);
    state
        .engine
        .combat
        .blocked_by
        .push((attacker, vec![blocker]));
    let spell = put(&mut state, PlayerId::P0, "Joust Through", Zone::Hand);
    state.players[0].mana_pool = [1, 0, 0, 0, 0, 0];
    let mut surface = surface();
    priority(&mut surface, &mut state);
    apply(&mut surface, &mut state, Action::CastSpell(spell));
    let Decision::ChooseTargets { legal_targets, .. } = next(&mut surface, &mut state) else {
        panic!("target choice")
    };
    assert!(legal_targets.contains(&Target::Object(attacker)));
    assert!(legal_targets.contains(&Target::Object(blocker)));
    assert!(!legal_targets.contains(&Target::Object(idle)));
    move_to(&mut state, blocker, Zone::Hand);
    move_to(&mut state, blocker, Zone::Battlefield);
    let Decision::ChooseTargets { legal_targets, .. } = next(&mut surface, &mut state) else {
        panic!("target choice")
    };
    assert!(!legal_targets.contains(&Target::Object(blocker)));
}

#[test]
fn joust_through_deals_three_then_gains_one_for_the_caster() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready();
        let target = put(&mut state, player, "Myr Enforcer", Zone::Battlefield);
        state.engine.combat.attackers.push(target);
        let mut surface = surface();
        let spell = cast(&mut surface, &mut state, "Joust Through", &[target]);
        resolve(&mut surface, &mut state);
        assert_eq!(dealt(&state, spell, target), 3);
        assert_eq!(state.players[0].life, 21);
        assert_eq!(state.players[1].life, 20);
        let damage = state
            .engine
            .event_history
            .iter()
            .position(
                |event| matches!(event, CommittedEvent::Damage { source, .. } if *source == spell),
            )
            .unwrap();
        assert!(
            matches!(state.engine.event_history[damage + 1], CommittedEvent::LifeGain { player: controller, amount: 1 } if controller == PlayerId::P0)
        );
    }
}

#[test]
fn joust_through_fizzles_after_target_leaves_combat_or_returns() {
    for blink in [false, true] {
        let mut state = ready();
        let target = put(&mut state, PlayerId::P1, "Myr Enforcer", Zone::Battlefield);
        state.engine.combat.attackers.push(target);
        let mut surface = surface();
        let spell = cast(&mut surface, &mut state, "Joust Through", &[target]);
        if blink {
            move_to(&mut state, target, Zone::Hand);
            move_to(&mut state, target, Zone::Battlefield);
        } else {
            state.engine.combat.attackers.clear();
        }
        resolve(&mut surface, &mut state);
        assert_eq!(dealt(&state, spell, target), 0);
        assert_eq!(state.players[1].life, 20);
    }
}

#[test]
fn joust_through_still_gains_life_when_damage_is_prevented() {
    let mut state = ready();
    let target = put(&mut state, PlayerId::P1, "Myr Enforcer", Zone::Battlefield);
    state.engine.combat.attackers.push(target);
    state.engine.active_replacements.push(ActiveReplacement {
        id: 1,
        source: target,
        kind: ReplacementEffectKind::PreventNextDamage {
            target: Target::Object(target),
            remaining: 3,
        },
    });
    let mut surface = surface();
    let spell = cast(&mut surface, &mut state, "Joust Through", &[target]);
    resolve(&mut surface, &mut state);
    assert_eq!(dealt(&state, spell, target), 0);
    assert_eq!(state.players[0].life, 21);
    assert_eq!(state.players[1].life, 20);
}
