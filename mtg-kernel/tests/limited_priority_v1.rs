use mtg_kernel::card_def::card_id_by_name;
use mtg_kernel::engine::{Action, Decision};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::policy_surface_v5::{PolicyActionV5, PolicyDecisionV5, PolicySurfaceV5};
use mtg_kernel::rl::observe_policy_v5;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use mtg_kernel::surface_v2::{
    HarnessSurfaceV2, PriorityModeV1, SuppressionAuditMode, SurfaceAction, SurfaceDecision,
};

fn state_at(step: Step) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state =
        GameState::new_from_libraries(&[forest; 40], &[forest; 40], |_| "Forest".into(), 123);
    state.step = step;
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
        _ => panic!("test helper zone"),
    }
    id
}

fn full_surface() -> HarnessSurfaceV2 {
    HarnessSurfaceV2::new_with_priority_mode_v1(
        PriorityModeV1::EngineWindowsV1,
        SuppressionAuditMode::Full,
    )
}

fn priority(surface: &mut HarnessSurfaceV2, state: &mut GameState, player: PlayerId) -> Decision {
    let surfaced = surface.next_decision(state);
    match surfaced {
        SurfaceDecision::Decision(decision @ Decision::CastSpellOrPass { player: actual, .. }) => {
            assert_eq!(actual, player);
            decision
        }
        other => panic!("expected priority for {player:?}, got {other:?}"),
    }
}

fn apply(surface: &mut HarnessSurfaceV2, state: &mut GameState, action: Action) {
    surface.apply(state, SurfaceAction::Action(action)).unwrap();
}

fn cast_bolt(
    surface: &mut HarnessSurfaceV2,
    state: &mut GameState,
    bolt: ObjectId,
    target: Target,
) {
    apply(surface, state, Action::CastSpell(bolt));
    assert!(matches!(
        surface.next_decision(state),
        SurfaceDecision::Decision(Decision::ChooseTargets { .. })
    ));
    apply(surface, state, Action::ChooseTarget(target));
}

#[test]
fn every_engine_priority_step_exposes_pass_only_windows() {
    for step in [
        Step::Upkeep,
        Step::Draw,
        Step::Main1,
        Step::BeginCombat,
        Step::DeclareAttackers,
        Step::DeclareBlockers,
        Step::CombatDamage,
        Step::EndCombat,
        Step::Main2,
        Step::End,
    ] {
        let mut state = state_at(step);
        let mut surface = full_surface();
        let decision = priority(&mut surface, &mut state, PlayerId::P0);
        assert_eq!(state.step, step);
        assert!(
            matches!(decision, Decision::CastSpellOrPass { castable_spells, mana_abilities,
            land_drops, activatable_abilities, .. } if castable_spells.is_empty() && mana_abilities.is_empty()
            && land_drops.is_empty() && activatable_abilities.is_empty())
        );
        apply(&mut surface, &mut state, Action::Pass);
        priority(&mut surface, &mut state, PlayerId::P1);
        assert_eq!(state.step, step);
        assert!(surface.suppressions().is_empty());
    }
}

#[test]
fn natural_turn_walk_skips_untap_and_ordinary_cleanup_and_draws_before_priority() {
    let mut state = state_at(Step::Untap);
    let mut surface = full_surface();
    let mut saw_opponent_draw = false;
    for _ in 0..100 {
        let decision = surface.next_decision(&mut state);
        let SurfaceDecision::Decision(Decision::CastSpellOrPass { .. }) = decision else {
            panic!("{decision:?}");
        };
        assert!(!matches!(state.step, Step::Untap | Step::Cleanup));
        if state.active_player == PlayerId::P1 && state.step == Step::Draw {
            assert_eq!(state.players[1].hand.len(), 1);
            saw_opponent_draw = true;
            break;
        }
        apply(&mut surface, &mut state, Action::Pass);
    }
    assert!(saw_opponent_draw);
}

#[test]
fn either_caster_can_hold_priority_and_stack_resolves_one_item_at_a_time_after_two_passes() {
    for caster in [PlayerId::P0, PlayerId::P1] {
        let mut state = state_at(Step::Main1);
        state.priority_player = caster;
        state.players[caster.index()].mana_pool[3] = 2;
        let first = put(&mut state, caster, "Lightning Bolt", Zone::Hand);
        let second = put(&mut state, caster, "Lightning Bolt", Zone::Hand);
        let opponent = caster.opponent();
        let mut surface = full_surface();
        priority(&mut surface, &mut state, caster);
        cast_bolt(&mut surface, &mut state, first, Target::Player(opponent));
        let decision = priority(&mut surface, &mut state, caster);
        assert!(
            matches!(decision, Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&second))
        );
        cast_bolt(&mut surface, &mut state, second, Target::Player(opponent));
        priority(&mut surface, &mut state, caster);
        assert_eq!(state.stack.len(), 2);
        assert_eq!(state.players[opponent.index()].life, 20);

        // Restore a serialized rules state plus the cloned surface while
        // holding priority over two unresolved spells; both paths agree.
        let encoded = serde_json::to_string(&state).unwrap();
        let mut restored: GameState = serde_json::from_str(&encoded).unwrap();
        let mut restored_surface = surface.clone();
        for (branch, branch_surface) in [
            (&mut state, &mut surface),
            (&mut restored, &mut restored_surface),
        ] {
            apply(branch_surface, branch, Action::Pass);
            priority(branch_surface, branch, opponent);
            assert_eq!(branch.stack.len(), 2);
            apply(branch_surface, branch, Action::Pass);
            priority(branch_surface, branch, PlayerId::P0);
            assert_eq!(branch.stack.len(), 1);
            assert_eq!(branch.stack[0].source, first);
            assert_eq!(branch.players[opponent.index()].life, 17);
            apply(branch_surface, branch, Action::Pass);
            priority(branch_surface, branch, PlayerId::P1);
            apply(branch_surface, branch, Action::Pass);
            priority(branch_surface, branch, PlayerId::P0);
            assert!(branch.stack.is_empty());
            assert_eq!(branch.players[opponent.index()].life, 14);
        }
        assert_eq!(
            serde_json::to_string(&state).unwrap(),
            serde_json::to_string(&restored).unwrap()
        );
        assert!(surface.suppressions().is_empty());
    }
}

#[test]
fn combat_allows_repeated_mana_actions_and_removal_after_blockers_before_damage() {
    let mut state = state_at(Step::DeclareAttackers);
    state.engine.combat.attackers_declared = false;
    state.engine.combat.blockers_declared = false;
    let attacker = put(
        &mut state,
        PlayerId::P0,
        "Voldaren Epicure",
        Zone::Battlefield,
    );
    let blocker = put(
        &mut state,
        PlayerId::P1,
        "Voldaren Epicure",
        Zone::Battlefield,
    );
    let bolt = put(&mut state, PlayerId::P0, "Lightning Bolt", Zone::Hand);
    let lands = [
        put(&mut state, PlayerId::P0, "Mountain", Zone::Battlefield),
        put(&mut state, PlayerId::P0, "Mountain", Zone::Battlefield),
    ];
    let mut surface = full_surface();
    assert!(matches!(
        surface.next_decision(&mut state),
        SurfaceDecision::Decision(Decision::DeclareAttackers { .. })
    ));
    apply(
        &mut surface,
        &mut state,
        Action::DeclareAttackers(vec![attacker]),
    );
    priority(&mut surface, &mut state, PlayerId::P0);
    apply(&mut surface, &mut state, Action::Pass);
    priority(&mut surface, &mut state, PlayerId::P1);
    apply(&mut surface, &mut state, Action::Pass);
    assert!(matches!(
        surface.next_decision(&mut state),
        SurfaceDecision::DeclareBlockersForAttacker { .. }
    ));
    surface
        .apply(
            &mut state,
            SurfaceAction::DeclareBlockersForAttacker(vec![blocker]),
        )
        .unwrap();
    for land in lands {
        priority(&mut surface, &mut state, PlayerId::P0);
        assert_eq!(state.step, Step::DeclareBlockers);
        apply(&mut surface, &mut state, Action::ActivateManaAbility(land));
    }
    priority(&mut surface, &mut state, PlayerId::P0);
    cast_bolt(&mut surface, &mut state, bolt, Target::Object(blocker));
    priority(&mut surface, &mut state, PlayerId::P0);
    apply(&mut surface, &mut state, Action::Pass);
    priority(&mut surface, &mut state, PlayerId::P1);
    apply(&mut surface, &mut state, Action::Pass);
    priority(&mut surface, &mut state, PlayerId::P0);
    assert_eq!(state.objects.get(blocker).zone, Zone::Graveyard);
    assert_eq!(state.step, Step::DeclareBlockers);
    apply(&mut surface, &mut state, Action::Pass);
    priority(&mut surface, &mut state, PlayerId::P1);
    apply(&mut surface, &mut state, Action::Pass);
    priority(&mut surface, &mut state, PlayerId::P0);
    assert_eq!(state.step, Step::CombatDamage);
    assert_eq!(
        state.players[1].life, 20,
        "a blocked nontrampling creature stays blocked"
    );
    assert!(surface.suppressions().is_empty());
}

#[test]
fn engine_priority_identity_is_in_public_hashes_and_default_context_bytes_stay_unchanged() {
    let state = state_at(Step::Main1);
    let legacy = PolicySurfaceV5::new();
    let full = PolicySurfaceV5::new_with_engine_priority_v1();
    let old = observe_policy_v5(&state, &legacy, PlayerId::P0, 0, 0, 0, 1).unwrap();
    let new = observe_policy_v5(&state, &full, PlayerId::P0, 0, 0, 0, 1).unwrap();
    assert_ne!(old.visible_projection_hash, new.visible_projection_hash);
    assert!(!serde_json::to_string(&old)
        .unwrap()
        .contains("engine_priority_version"));
    assert!(serde_json::to_string(&new)
        .unwrap()
        .contains("\"engine_priority_version\":1"));
    assert!(!serde_json::to_string(&legacy.harness_public_context())
        .unwrap()
        .contains("engine_priority_version"));
}

#[test]
fn binary_combat_scan_retains_engine_priority_mode_across_a_pending_choice_clone() {
    let mut state = state_at(Step::DeclareAttackers);
    state.engine.combat.attackers_declared = false;
    let attacker = put(
        &mut state,
        PlayerId::P0,
        "Voldaren Epicure",
        Zone::Battlefield,
    );
    let mut surface = PolicySurfaceV5::new_with_engine_priority_v1();
    assert!(matches!(
        surface.next_decision(&mut state).unwrap(),
        PolicyDecisionV5::AttackerInclusion { .. }
    ));
    let mut restored: GameState =
        serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    let mut restored_surface = surface.clone();
    for (branch, branch_surface) in [
        (&mut state, &mut surface),
        (&mut restored, &mut restored_surface),
    ] {
        branch_surface
            .apply(
                branch,
                PolicyActionV5::ChooseAttackerInclusion {
                    actor: PlayerId::P0,
                    attacker,
                    include: true,
                },
            )
            .unwrap();
        assert!(matches!(
            branch_surface.next_decision(branch).unwrap(),
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::CastSpellOrPass {
                player: PlayerId::P0,
                ..
            }))
        ));
        assert_eq!(
            branch_surface
                .harness_public_context()
                .engine_priority_version,
            Some(1)
        );
    }
    assert_eq!(
        serde_json::to_string(&state).unwrap(),
        serde_json::to_string(&restored).unwrap()
    );
}
