use mtg_kernel::card_def::{card_id_by_name, Keywords};
use mtg_kernel::combat_damage_v1::enable_foundations_combat_v1;
use mtg_kernel::engine::{self, Action, Decision, EffectDuration, Layers, UntilEndOfTurnEffect};
use mtg_kernel::event::{self, CommittedEvent, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::policy_surface_v5::{PolicyActionV5, PolicyDecisionV5, PolicySurfaceV5};
use mtg_kernel::rl::{legal_action_candidates_v5, ActionSemanticV1};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use mtg_kernel::surface_v2::SurfaceAction;

fn ready() -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state =
        GameState::new_from_libraries(&[forest; 40], &[forest; 40], |_| "Forest".into(), 123);
    state.step = Step::DeclareBlockers;
    enable_foundations_combat_v1(&mut state).unwrap();
    state.engine.combat.attackers_declared = true;
    state.engine.combat.blockers_declared = true;
    state
}

fn put(state: &mut GameState, player: PlayerId, name: &str) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap();
    let id = state.objects.push(GameObject {
        card_def,
        name: name.into(),
        owner: player,
        controller: player,
        zone: Zone::Battlefield,
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
    state.players[player.index()].battlefield.push(id);
    id
}

fn grant(state: &mut GameState, object: ObjectId, keywords: Keywords) {
    state
        .engine
        .until_end_of_turn
        .push(UntilEndOfTurnEffect::ResolvedObjectKeywordEffect {
            object_id: object,
            object_zone_change_count: state.objects.get(object).zone_change_count,
            layer: Layers::ABILITY_ADDING,
            timestamp: 1,
            duration: EffectDuration::EndOfTurn,
            keywords,
        });
}

fn pair(state: &mut GameState, attacker: ObjectId, blockers: &[ObjectId]) {
    state.engine.combat.attackers.push(attacker);
    state
        .engine
        .combat
        .blocked_by
        .push((attacker, blockers.to_vec()));
}

fn enter_damage(state: &mut GameState) -> Decision {
    assert!(matches!(
        engine::advance_until_decision(state),
        Decision::CastSpellOrPass { .. }
    ));
    engine::step(state, Action::Pass).unwrap();
    assert!(matches!(
        engine::advance_until_decision(state),
        Decision::CastSpellOrPass { .. }
    ));
    engine::step(state, Action::Pass).unwrap();
    let decision = engine::advance_until_decision(state);
    assert_eq!(state.step, Step::CombatDamage);
    decision
}

fn choose(state: &mut GameState, desired: &[(ObjectId, Target, i32)]) -> usize {
    for count in 0..256 {
        match engine::advance_until_decision(state) {
            Decision::ChooseCombatDamageRange {
                source,
                recipient,
                minimum,
                maximum,
                split_at,
                ..
            } => {
                let amount = desired
                    .iter()
                    .find(|(id, target, _)| *id == source && *target == recipient)
                    .unwrap_or_else(|| {
                        panic!("missing intended amount for {source:?} -> {recipient:?}")
                    })
                    .2;
                assert!(
                    (minimum..=maximum).contains(&amount),
                    "{amount} outside {minimum}..={maximum}"
                );
                engine::step(
                    state,
                    Action::ChooseCombatDamageRange {
                        upper_half: amount > split_at,
                    },
                )
                .unwrap();
            }
            Decision::CastSpellOrPass { .. } | Decision::GameOver { .. } => return count,
            other => panic!("unexpected assignment decision {other:?}"),
        }
    }
    panic!("damage assignment did not finish");
}

fn dealt(state: &GameState, source: ObjectId, target: Target) -> i32 {
    state
        .engine
        .event_history
        .iter()
        .filter_map(|event| match event {
            CommittedEvent::Damage {
                source: actual,
                target: actual_target,
                amount,
            } if *actual == source && *actual_target == target => Some(*amount),
            _ => None,
        })
        .sum()
}

#[test]
fn arbitrary_gang_block_allocation_can_skip_lethal_on_the_first_blocker() {
    let mut state = ready();
    let attacker = put(&mut state, PlayerId::P0, "Myr Enforcer");
    state.objects.get_mut(attacker).counters.plus1_plus1 = 2;
    let first = put(&mut state, PlayerId::P1, "Myr Enforcer");
    let second = put(&mut state, PlayerId::P1, "Myr Enforcer");
    pair(&mut state, attacker, &[first, second]);
    assert!(matches!(
        enter_damage(&mut state),
        Decision::ChooseCombatDamageRange {
            minimum: 0,
            maximum: 6,
            ..
        }
    ));
    assert_eq!(state.objects.get(first).damage, 0);
    assert_eq!(state.objects.get(second).damage, 0);
    choose(&mut state, &[(attacker, Target::Object(first), 2)]);
    assert_eq!(dealt(&state, attacker, Target::Object(first)), 2);
    assert_eq!(dealt(&state, attacker, Target::Object(second)), 4);
    assert_eq!(state.objects.get(first).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(second).zone, Zone::Graveyard);
    assert_eq!(dealt(&state, first, Target::Object(attacker)), 4);
    assert_eq!(dealt(&state, second, Target::Object(attacker)), 4);
}

#[test]
fn trample_can_assign_excess_to_player_or_overassign_the_blocker() {
    for amount in [4, 5, 6] {
        let mut state = ready();
        let attacker = put(&mut state, PlayerId::P0, "Myr Enforcer");
        state.objects.get_mut(attacker).counters.plus1_plus1 = 2;
        grant(&mut state, attacker, Keywords::TRAMPLE);
        let blocker = put(&mut state, PlayerId::P1, "Myr Enforcer");
        pair(&mut state, attacker, &[blocker]);
        assert!(matches!(
            enter_damage(&mut state),
            Decision::ChooseCombatDamageRange {
                minimum: 4,
                maximum: 6,
                ..
            }
        ));
        choose(&mut state, &[(attacker, Target::Object(blocker), amount)]);
        assert_eq!(dealt(&state, attacker, Target::Object(blocker)), amount);
        assert_eq!(state.players[1].life, 20 - (6 - amount));
    }
}

#[test]
fn trample_requires_lethal_to_every_blocker_but_allows_player_zero() {
    for first_amount in [0, 1, 2, 3, 4, 5, 6] {
        let mut state = ready();
        let attacker = put(&mut state, PlayerId::P0, "Myr Enforcer");
        state.objects.get_mut(attacker).counters.plus1_plus1 = 2;
        grant(&mut state, attacker, Keywords::TRAMPLE);
        let first = put(&mut state, PlayerId::P1, "Myr Enforcer");
        let second = put(&mut state, PlayerId::P1, "Myr Enforcer");
        pair(&mut state, attacker, &[first, second]);
        enter_damage(&mut state);
        choose(
            &mut state,
            &[(attacker, Target::Object(first), first_amount)],
        );
        assert_eq!(dealt(&state, attacker, Target::Object(first)), first_amount);
        assert_eq!(
            dealt(&state, attacker, Target::Object(second)),
            6 - first_amount
        );
        assert_eq!(state.players[1].life, 20);
    }
}

#[test]
fn trample_and_deathtouch_require_one_for_each_blocker_before_excess() {
    let mut state = ready();
    let attacker = put(&mut state, PlayerId::P0, "Myr Enforcer");
    state.objects.get_mut(attacker).counters.plus1_plus1 = 2;
    grant(&mut state, attacker, Keywords::TRAMPLE);
    grant(&mut state, attacker, Keywords::DEATHTOUCH);
    let first = put(&mut state, PlayerId::P1, "Myr Enforcer");
    let second = put(&mut state, PlayerId::P1, "Myr Enforcer");
    pair(&mut state, attacker, &[first, second]);
    enter_damage(&mut state);
    choose(
        &mut state,
        &[
            (attacker, Target::Object(first), 1),
            (attacker, Target::Object(second), 1),
        ],
    );
    assert_eq!(dealt(&state, attacker, Target::Object(first)), 1);
    assert_eq!(dealt(&state, attacker, Target::Object(second)), 1);
    assert_eq!(dealt(&state, attacker, Target::Player(PlayerId::P1)), 4);
    assert_eq!(state.objects.get(first).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(second).zone, Zone::Graveyard);
}

#[test]
fn trample_uses_marked_damage_and_deathtouch_for_lethal() {
    for deathtouch in [false, true] {
        let mut state = ready();
        let attacker = put(&mut state, PlayerId::P0, "Myr Enforcer");
        state.objects.get_mut(attacker).counters.plus1_plus1 = 2;
        grant(&mut state, attacker, Keywords::TRAMPLE);
        if deathtouch {
            grant(&mut state, attacker, Keywords::DEATHTOUCH);
        }
        let blocker = put(&mut state, PlayerId::P1, "Myr Enforcer");
        state.objects.get_mut(blocker).damage = 2;
        pair(&mut state, attacker, &[blocker]);
        enter_damage(&mut state);
        let lethal = if deathtouch { 1 } else { 2 };
        choose(&mut state, &[(attacker, Target::Object(blocker), lethal)]);
        assert_eq!(
            dealt(&state, attacker, Target::Player(PlayerId::P1)),
            6 - lethal
        );
    }
}

#[test]
fn trample_ignores_prevention_when_assigning_but_lifelink_uses_actual_damage() {
    use mtg_kernel::event::{ActiveReplacement, ReplacementEffectKind};
    let mut state = ready();
    let attacker = put(&mut state, PlayerId::P0, "Myr Enforcer");
    state.objects.get_mut(attacker).counters.plus1_plus1 = 2;
    grant(&mut state, attacker, Keywords::TRAMPLE);
    grant(&mut state, attacker, Keywords::LIFELINK);
    let blocker = put(&mut state, PlayerId::P1, "Myr Enforcer");
    pair(&mut state, attacker, &[blocker]);
    state.engine.active_replacements.push(ActiveReplacement {
        id: 1,
        source: blocker,
        kind: ReplacementEffectKind::PreventNextDamage {
            target: Target::Object(blocker),
            remaining: 100,
        },
    });
    enter_damage(&mut state);
    choose(&mut state, &[(attacker, Target::Object(blocker), 4)]);
    assert_eq!(dealt(&state, attacker, Target::Object(blocker)), 0);
    assert_eq!(state.objects.get(blocker).zone, Zone::Battlefield);
    assert_eq!(state.players[0].life, 22); // Only two dealt, so lifelink gains two.
    assert_eq!(state.objects.get(attacker).damage, 4);
    assert_eq!(state.players[1].life, 18);
}

#[test]
fn indestructibility_does_not_change_lethal_and_already_lethal_damage_requires_zero() {
    for already_marked in [0, 4] {
        for deathtouch in [false, true] {
            let mut state = ready();
            let attacker = put(&mut state, PlayerId::P0, "Myr Enforcer");
            state.objects.get_mut(attacker).counters.plus1_plus1 = 2;
            grant(&mut state, attacker, Keywords::TRAMPLE);
            if deathtouch {
                grant(&mut state, attacker, Keywords::DEATHTOUCH);
            }
            let blocker = put(&mut state, PlayerId::P1, "Myr Enforcer");
            grant(&mut state, blocker, Keywords::INDESTRUCTIBLE);
            state.objects.get_mut(blocker).damage = already_marked;
            pair(&mut state, attacker, &[blocker]);
            enter_damage(&mut state);
            let lethal = if already_marked == 4 {
                0
            } else if deathtouch {
                1
            } else {
                4
            };
            choose(&mut state, &[(attacker, Target::Object(blocker), lethal)]);
            assert_eq!(
                dealt(&state, attacker, Target::Player(PlayerId::P1)),
                6 - lethal
            );
            assert_eq!(state.objects.get(blocker).zone, Zone::Battlefield);
        }
    }
}

#[test]
fn stale_source_and_recipient_incarnations_refuse_answers_without_mutation() {
    for change_source in [false, true] {
        let mut state = ready();
        let attacker = put(&mut state, PlayerId::P0, "Myr Enforcer");
        let first = put(&mut state, PlayerId::P1, "Myr Enforcer");
        let second = put(&mut state, PlayerId::P1, "Myr Enforcer");
        pair(&mut state, attacker, &[first, second]);
        enter_damage(&mut state);
        let changed = if change_source { attacker } else { first };
        state.objects.get_mut(changed).zone_change_count += 1;
        let before = state.clone();
        assert!(engine::step(
            &mut state,
            Action::ChooseCombatDamageRange { upper_half: false }
        )
        .is_err());
        assert_eq!(state, before);
        assert_eq!(dealt(&state, attacker, Target::Object(first)), 0);
    }
}

#[test]
fn blocked_creature_with_no_remaining_blockers_only_hits_player_with_trample() {
    for trample in [false, true] {
        let mut state = ready();
        let attacker = put(&mut state, PlayerId::P0, "Myr Enforcer");
        if trample {
            grant(&mut state, attacker, Keywords::TRAMPLE);
        }
        let blocker = put(&mut state, PlayerId::P1, "Myr Enforcer");
        pair(&mut state, attacker, &[blocker]);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(blocker, Zone::Graveyard),
        );
        assert_eq!(state.engine.combat.blocked_by[0].1, Vec::<ObjectId>::new());
        enter_damage(&mut state);
        assert_eq!(state.players[1].life, if trample { 16 } else { 20 });
    }
}

#[test]
fn pending_assignment_rejects_priority_and_restores_exactly() {
    let mut state = ready();
    let attacker = put(&mut state, PlayerId::P0, "Myr Enforcer");
    let first = put(&mut state, PlayerId::P1, "Myr Enforcer");
    let second = put(&mut state, PlayerId::P1, "Myr Enforcer");
    pair(&mut state, attacker, &[first, second]);
    let original_decision = enter_damage(&mut state);
    let original_bytes = serde_json::to_vec(&state).unwrap();
    let original_hash = state.state_hash();
    assert!(engine::step(&mut state, Action::Pass).is_err());
    assert_eq!(state.state_hash(), original_hash);
    let snapshot = state.snapshot();
    let desired = [(attacker, Target::Object(first), 1)];
    choose(&mut state, &desired);
    let result = state.clone();
    state.restore(&snapshot);
    assert_eq!(
        engine::advance_until_decision(&mut state),
        original_decision
    );
    assert_eq!(serde_json::to_vec(&state).unwrap(), original_bytes);
    choose(&mut state, &desired);
    assert_eq!(state, result);
    let mut serialized: GameState = serde_json::from_slice(&original_bytes).unwrap();
    choose(&mut serialized, &desired);
    assert_eq!(serialized, result);
}

#[test]
fn large_power_has_two_choices_per_prompt_and_logarithmic_depth() {
    let mut state = ready();
    let attacker = put(&mut state, PlayerId::P0, "Myr Enforcer");
    state.objects.get_mut(attacker).counters.plus1_plus1 = i32::from(i16::MAX);
    let first = put(&mut state, PlayerId::P1, "Myr Enforcer");
    let second = put(&mut state, PlayerId::P1, "Myr Enforcer");
    pair(&mut state, attacker, &[first, second]);
    enter_damage(&mut state);
    let count = choose(&mut state, &[(attacker, Target::Object(first), 12345)]);
    assert!(count <= 16);
    assert_eq!(dealt(&state, attacker, Target::Object(first)), 12345);
    assert_eq!(
        dealt(&state, attacker, Target::Object(second)),
        32771 - 12345
    );
}

#[test]
fn first_strike_has_a_real_priority_window_before_normal_damage() {
    let mut state = ready();
    let attacker = put(&mut state, PlayerId::P0, "Myr Enforcer");
    let blocker = put(&mut state, PlayerId::P1, "Myr Enforcer");
    state.objects.get_mut(blocker).counters.plus1_plus1 = 1;
    grant(&mut state, attacker, Keywords::FIRST_STRIKE);
    pair(&mut state, attacker, &[blocker]);
    assert!(matches!(
        enter_damage(&mut state),
        Decision::CastSpellOrPass {
            player: PlayerId::P0,
            ..
        }
    ));
    assert_eq!(dealt(&state, attacker, Target::Object(blocker)), 4);
    assert_eq!(dealt(&state, blocker, Target::Object(attacker)), 0);
    engine::step(&mut state, Action::Pass).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass {
            player: PlayerId::P1,
            ..
        }
    ));
    engine::step(&mut state, Action::Pass).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass {
            player: PlayerId::P0,
            ..
        }
    ));
    assert_eq!(state.step, Step::CombatDamage);
    assert_eq!(dealt(&state, blocker, Target::Object(attacker)), 5);
    assert_eq!(state.objects.get(attacker).zone, Zone::Graveyard);
}

#[test]
fn first_strike_kill_removes_the_blocker_before_normal_damage() {
    let mut state = ready();
    let attacker = put(&mut state, PlayerId::P0, "Myr Enforcer");
    grant(&mut state, attacker, Keywords::FIRST_STRIKE);
    let blocker = put(&mut state, PlayerId::P1, "Myr Enforcer");
    pair(&mut state, attacker, &[blocker]);
    enter_damage(&mut state);
    assert_eq!(state.objects.get(blocker).zone, Zone::Graveyard);
    engine::step(&mut state, Action::Pass).unwrap();
    engine::advance_until_decision(&mut state);
    engine::step(&mut state, Action::Pass).unwrap();
    engine::advance_until_decision(&mut state);
    assert_eq!(dealt(&state, blocker, Target::Object(attacker)), 0);
    assert_eq!(state.objects.get(attacker).damage, 0);
}

#[test]
fn gaining_or_losing_first_strike_between_waves_does_not_change_normal_eligibility() {
    let mut state = ready();
    let first = put(&mut state, PlayerId::P0, "Myr Enforcer");
    grant(&mut state, first, Keywords::FIRST_STRIKE);
    let normal = put(&mut state, PlayerId::P0, "Myr Enforcer");
    state.engine.combat.attackers.extend([first, normal]);
    enter_damage(&mut state);
    assert_eq!(dealt(&state, first, Target::Player(PlayerId::P1)), 4);
    assert_eq!(dealt(&state, normal, Target::Player(PlayerId::P1)), 0);
    state.engine.until_end_of_turn.clear();
    grant(&mut state, normal, Keywords::FIRST_STRIKE);
    engine::step(&mut state, Action::Pass).unwrap();
    engine::advance_until_decision(&mut state);
    engine::step(&mut state, Action::Pass).unwrap();
    engine::advance_until_decision(&mut state);
    assert_eq!(dealt(&state, first, Target::Player(PlayerId::P1)), 4);
    assert_eq!(dealt(&state, normal, Target::Player(PlayerId::P1)), 4);
}

#[test]
fn double_strike_trample_hits_in_both_waves_and_lifelink_uses_dealt_damage() {
    let mut state = ready();
    let attacker = put(&mut state, PlayerId::P0, "Myr Enforcer");
    grant(&mut state, attacker, Keywords::TRAMPLE);
    grant(&mut state, attacker, Keywords::DOUBLE_STRIKE);
    grant(&mut state, attacker, Keywords::LIFELINK);
    let blocker = put(&mut state, PlayerId::P1, "Llanowar Elves");
    pair(&mut state, attacker, &[blocker]);
    enter_damage(&mut state);
    choose(&mut state, &[(attacker, Target::Object(blocker), 1)]);
    assert_eq!(state.players[0].life, 24);
    assert_eq!(state.players[1].life, 17);
    engine::step(&mut state, Action::Pass).unwrap();
    engine::advance_until_decision(&mut state);
    engine::step(&mut state, Action::Pass).unwrap();
    engine::advance_until_decision(&mut state);
    assert_eq!(state.players[0].life, 28);
    assert_eq!(state.players[1].life, 13);
}

#[test]
fn policy_actions_show_the_exact_source_recipient_and_amount_ranges() {
    let mut state = ready();
    let attacker = put(&mut state, PlayerId::P0, "Myr Enforcer");
    grant(&mut state, attacker, Keywords::TRAMPLE);
    let blocker = put(&mut state, PlayerId::P1, "Llanowar Elves");
    pair(&mut state, attacker, &[blocker]);
    enter_damage(&mut state);
    let mut surface = PolicySurfaceV5::new_with_engine_priority_v1();
    let decision = surface.next_decision(&mut state).unwrap();
    assert!(matches!(decision, PolicyDecisionV5::Surface(_)));
    let actions = legal_action_candidates_v5(&decision, &state).unwrap();
    assert_eq!(actions.len(), 2);
    assert_ne!(actions[0].record.stable_id, actions[1].record.stable_id);
    for (index, candidate) in actions.iter().enumerate() {
        assert!(
            matches!(&candidate.record.semantic, ActionSemanticV1::ChooseCombatDamageRange {
            source, minimum: 1, maximum: 4, split_at: 2, upper_half, ..
        } if source.arena_id == attacker.0 && *upper_half == (index == 1))
        );
    }
    surface
        .apply(&mut state, actions[0].policy_action.clone())
        .unwrap();
    let next = surface.next_decision(&mut state).unwrap();
    let next_actions = legal_action_candidates_v5(&next, &state).unwrap();
    assert_ne!(
        next_actions[0].record.stable_id,
        actions[0].record.stable_id
    );
    assert_eq!(dealt(&state, attacker, Target::Object(blocker)), 0);
    assert!(surface
        .apply(
            &mut state,
            PolicyActionV5::Surface(SurfaceAction::Action(Action::Pass))
        )
        .is_err());
}

#[test]
fn public_context_remembers_prior_allocations_without_dealing_them() {
    let mut state = ready();
    let attacker = put(&mut state, PlayerId::P0, "Myr Enforcer");
    state.objects.get_mut(attacker).counters.plus1_plus1 = 2;
    let first = put(&mut state, PlayerId::P1, "Myr Enforcer");
    let second = put(&mut state, PlayerId::P1, "Myr Enforcer");
    let third = put(&mut state, PlayerId::P1, "Myr Enforcer");
    pair(&mut state, attacker, &[first, second, third]);
    enter_damage(&mut state);
    let mut reached_second = false;
    for _ in 0..32 {
        match engine::advance_until_decision(&mut state) {
            Decision::ChooseCombatDamageRange {
                recipient: Target::Object(id),
                split_at,
                ..
            } if id == first => {
                engine::step(
                    &mut state,
                    Action::ChooseCombatDamageRange {
                        upper_half: 3 > split_at,
                    },
                )
                .unwrap();
            }
            Decision::ChooseCombatDamageRange {
                recipient: Target::Object(id),
                minimum: 0,
                maximum: 3,
                ..
            } if id == second => {
                reached_second = true;
                break;
            }
            other => panic!("unexpected partial allocation {other:?}"),
        }
    }
    assert!(reached_second);
    let mut surface = PolicySurfaceV5::new_with_engine_priority_v1();
    surface.next_decision(&mut state).unwrap();
    let observation =
        mtg_kernel::rl::observe_policy_v5(&state, &surface, PlayerId::P0, 0, 0, 0, 1).unwrap();
    let context = observation.projection.foundations_combat.unwrap();
    assert_eq!(context.phase, "normal");
    assert_eq!(context.assignments.len(), 1);
    assert_eq!(context.assignments[0].source.arena_id, attacker.0);
    assert_eq!(context.assignments[0].amount, 3);
    assert_eq!(dealt(&state, attacker, Target::Object(first)), 0);
    assert_eq!(state.objects.get(first).damage, 0);
}

#[test]
fn legacy_combat_keeps_its_serialized_shape_and_default_allocation() {
    let mut state = ready();
    state.engine.combat.foundations_v1 = None;
    let attacker = put(&mut state, PlayerId::P0, "Myr Enforcer");
    state.objects.get_mut(attacker).counters.plus1_plus1 = 2;
    let first = put(&mut state, PlayerId::P1, "Myr Enforcer");
    let second = put(&mut state, PlayerId::P1, "Myr Enforcer");
    pair(&mut state, attacker, &[first, second]);
    let value = serde_json::to_value(&state.engine.combat).unwrap();
    assert_eq!(value.as_object().unwrap().len(), 4);
    assert!(value.get("foundations_v1").is_none());
    enter_damage(&mut state);
    assert_eq!(dealt(&state, attacker, Target::Object(first)), 4);
    assert_eq!(dealt(&state, attacker, Target::Object(second)), 2);
}

#[test]
fn binary_games_exercise_casts_and_damage_choices_and_replay_identically() {
    use sha2::{Digest, Sha256};
    use std::collections::BTreeMap;
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Command, Stdio};
    fn play() -> (String, serde_json::Value, BTreeMap<String, usize>) {
        let mut child = Command::new(env!("CARGO_BIN_EXE_kernel_limited_env"))
            .arg("--foundations-combat-v1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut input = child.stdin.take().unwrap();
        let mut output = BufReader::new(child.stdout.take().unwrap());
        let deck = serde_json::json!({"cards":[
            {"name":"Forest","count":20}, {"name":"Llanowar Elves","count":12},
            {"name":"Spinewoods Paladin","count":8}
        ]});
        let mut request = serde_json::json!({
            "schema_version":3,"request_type":"reset","request_id":"reset",
            "decks":[deck.clone(),deck],"episode_id":7,"env_seed":123,
            "max_physical_decisions":8192,"max_policy_steps":16384
        });
        let mut transcript = Sha256::new();
        let mut counts = BTreeMap::<String, usize>::new();
        for index in 0..16384 {
            let raw = format!("{request}\n");
            input.write_all(raw.as_bytes()).unwrap();
            input.flush().unwrap();
            let mut line = String::new();
            assert!(output.read_line(&mut line).unwrap() > 0);
            transcript.update(raw.as_bytes());
            transcript.update(line.as_bytes());
            let reply: serde_json::Value = serde_json::from_str(&line).unwrap();
            assert_eq!(reply["schema_version"], 3);
            assert_eq!(reply["priority_mode"], "engine_windows_v1");
            assert_eq!(reply["combat_rules"], "foundations_v1");
            assert_ne!(reply["response_type"], "error", "{reply}");
            if reply["response_type"] == "terminal" {
                drop(input);
                assert!(child.wait().unwrap().success());
                return (format!("{:x}", transcript.finalize()), reply, counts);
            }
            let decision = &reply["decision"];
            let actions = decision["legal_actions"].as_array().unwrap();
            let rank = |action: &serde_json::Value| -> u8 {
                let semantic = &action["semantic"];
                match semantic["action_kind"].as_str().unwrap() {
                    "play_land" => 0,
                    "cast_spell" => 1,
                    "choose_attacker_inclusion" | "choose_blocker_inclusion"
                        if semantic["include"] == true =>
                    {
                        2
                    }
                    "choose_combat_damage_range" if semantic["upper_half"] == false => 3,
                    "pass" => 10,
                    "activate_mana_ability" => 11,
                    _ => 5,
                }
            };
            let action = actions.iter().min_by_key(|action| rank(action)).unwrap();
            let kind = action["semantic"]["action_kind"].as_str().unwrap();
            *counts.entry(kind.to_string()).or_default() += 1;
            if kind == "choose_combat_damage_range" {
                assert_eq!(actions.len(), 2);
                assert_eq!(
                    decision["observation"]["projection"]["foundations_combat"]["phase"],
                    "normal"
                );
            }
            request = serde_json::json!({
                "schema_version":3,"request_type":"step","request_id":format!("step-{index}"),
                "episode_id":decision["episode_id"],"expected_step":decision["step"],
                "selected_index":action["selected_index"],"selected_action_id":action["stable_id"]
            });
        }
        child.kill().unwrap();
        child.wait().unwrap();
        panic!("bounded gameplay did not terminate");
    }
    let first = play();
    let second = play();
    assert_eq!(first, second);
    assert_eq!(first.1["terminal"]["terminal_classification"], "natural");
    assert!(
        first.2.get("cast_spell").copied().unwrap_or(0) >= 4,
        "{:?}",
        first.2
    );
    assert!(
        first
            .2
            .get("choose_combat_damage_range")
            .copied()
            .unwrap_or(0)
            > 0,
        "{:?}",
        first.2
    );
}

#[test]
fn foundations_schema_is_explicit_and_refused_by_older_servers() {
    use mtg_kernel::limited_session_v1::LimitedJsonlServerV1;
    let deck = serde_json::json!({"cards":[{"name":"Forest","count":40}]});
    let request = serde_json::json!({
        "schema_version":3,"request_type":"reset","request_id":"reset",
        "decks":[deck.clone(),deck],"episode_id":7,"env_seed":123,
        "max_physical_decisions":4096,"max_policy_steps":8192
    });
    for mut server in [
        LimitedJsonlServerV1::new(),
        LimitedJsonlServerV1::new_with_engine_priority_v1(),
    ] {
        let reply: serde_json::Value =
            serde_json::from_str(&server.handle_line(&request.to_string())).unwrap();
        assert_eq!(reply["error"]["code"], "schema_version_mismatch");
    }
    let mut server = LimitedJsonlServerV1::new_with_foundations_combat_v1();
    let raw = request.to_string();
    let reply = server.handle_line(&raw);
    assert_eq!(server.handle_line(&raw), reply);
    let parsed: serde_json::Value = serde_json::from_str(&reply).unwrap();
    assert_eq!(parsed["schema_version"], 3);
    assert_eq!(parsed["combat_rules"], "foundations_v1");
    assert_eq!(
        parsed["decision"]["observation"]["projection"]["foundations_combat"]["phase"],
        "before_damage"
    );
}
