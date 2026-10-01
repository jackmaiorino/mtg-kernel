//! Rules behavior for the first six missing names in the pinned FDN fixtures.
#![cfg(feature = "limited-fdn-fixtures")]
use mtg_kernel::card_def::{card_id_by_name, CardCapability, Keywords, TargetSpec, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ActiveReplacement, ProposedEvent, ReplacementEffectKind};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::CustomDeckV1;
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use mtg_kernel::surface_v2::{
    HarnessSurfaceV2, PriorityModeV1, SuppressionAuditMode, SurfaceAction, SurfaceDecision,
};

fn ready(step: Step) -> GameState {
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
        summoning_sick: true,
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

fn surface() -> HarnessSurfaceV2 {
    HarnessSurfaceV2::new_with_priority_mode_v1(
        PriorityModeV1::EngineWindowsV1,
        SuppressionAuditMode::Full,
    )
}

fn priority(surface: &mut HarnessSurfaceV2, state: &mut GameState) -> Decision {
    match surface.next_decision(state) {
        SurfaceDecision::Decision(decision @ Decision::CastSpellOrPass { .. }) => decision,
        other => panic!("expected priority, got {other:?}"),
    }
}

fn apply(surface: &mut HarnessSurfaceV2, state: &mut GameState, action: Action) {
    surface.apply(state, SurfaceAction::Action(action)).unwrap();
}

fn announce(
    surface: &mut HarnessSurfaceV2,
    state: &mut GameState,
    action: Action,
    target: Option<ObjectId>,
) {
    priority(surface, state);
    apply(surface, state, action);
    if let Some(target) = target {
        assert!(matches!(
            surface.next_decision(state),
            SurfaceDecision::Decision(Decision::ChooseTargets { legal_targets, .. })
                if legal_targets.contains(&Target::Object(target))
        ));
        apply(surface, state, Action::ChooseTarget(Target::Object(target)));
    }
    priority(surface, state);
}

fn resolve_one(surface: &mut HarnessSurfaceV2, state: &mut GameState) {
    let before = state.stack.len();
    assert!(before > 0);
    apply(surface, state, Action::Pass);
    priority(surface, state);
    apply(surface, state, Action::Pass);
    priority(surface, state);
    assert_eq!(state.stack.len(), before - 1);
}

fn move_to(state: &mut GameState, object: ObjectId, zone: Zone) {
    event::propose_and_commit(state, ProposedEvent::zone_change(object, zone));
}

#[test]
fn appended_definitions_keep_old_card_ids_and_target_vocabulary() {
    assert_eq!(card_id_by_name("Hero Token"), Some(159));
    assert_eq!(card_id_by_name("Clue Token"), Some(160));
    assert_eq!(card_id_by_name("Skeleton Token"), Some(161));
    for (index, name) in [
        "Plains",
        "Healer's Hawk",
        "Fleeting Distraction",
        "Cathar Commando",
        "Spectral Sailor",
        "Treetop Snarespinner",
    ]
    .into_iter()
    .enumerate()
    {
        let id = card_id_by_name(name).unwrap();
        assert_eq!(id as usize, 162 + index);
        assert_eq!(CARD_DEFS[id as usize].capability, CardCapability::Full);
        assert!(!CARD_DEFS[id as usize].is_token);
    }
    assert_eq!(
        TargetSpec::OpponentArtifactOrEnchantmentPermanent.stable_id(),
        35
    );
    assert_eq!(TargetSpec::ArtifactOrEnchantmentPermanent.stable_id(), 36);
}

#[test]
fn plains_plays_untapped_produces_white_and_allows_limited_copy_counts() {
    let deck: CustomDeckV1 =
        serde_json::from_str(r#"{"cards":[{"name":"Plains","count":40}]}"#).unwrap();
    assert_eq!(deck.resolve().unwrap(), vec![162; 40]);
    let mut state = ready(Step::Main1);
    let plains = put(&mut state, PlayerId::P0, "Plains", Zone::Hand);
    let mut surface = surface();
    priority(&mut surface, &mut state);
    apply(&mut surface, &mut state, Action::PlayLand(plains));
    assert_eq!(state.objects.get(plains).zone, Zone::Battlefield);
    assert!(!state.objects.get(plains).tapped);
    priority(&mut surface, &mut state);
    apply(
        &mut surface,
        &mut state,
        Action::ActivateManaAbility(plains),
    );
    assert!(state.objects.get(plains).tapped);
    assert_eq!(state.players[0].mana_pool, [1, 0, 0, 0, 0, 0]);
}

#[test]
fn flash_creatures_cast_at_opponents_priority_windows_but_ordinary_creatures_do_not() {
    for step in [Step::Upkeep, Step::Main1, Step::DeclareBlockers, Step::End] {
        for (name, mana) in [
            ("Cathar Commando", [1, 0, 0, 0, 0, 1]),
            ("Spectral Sailor", [0, 1, 0, 0, 0, 0]),
        ] {
            let mut state = ready(step);
            state.active_player = PlayerId::P1;
            state.players[0].mana_pool = mana;
            let flash = put(&mut state, PlayerId::P0, name, Zone::Hand);
            let hawk = put(&mut state, PlayerId::P0, "Healer's Hawk", Zone::Hand);
            let spider = put(&mut state, PlayerId::P0, "Treetop Snarespinner", Zone::Hand);
            let mut surface = surface();
            assert!(matches!(priority(&mut surface, &mut state),
                Decision::CastSpellOrPass { castable_spells, .. }
                    if castable_spells.contains(&flash) && !castable_spells.contains(&hawk)
                        && !castable_spells.contains(&spider)));
            announce(&mut surface, &mut state, Action::CastSpell(flash), None);
            assert_eq!(state.objects.get(flash).zone, Zone::Stack);
            assert_eq!(state.players[0].mana_pool, [0; 6]);
            resolve_one(&mut surface, &mut state);
            assert_eq!(state.objects.get(flash).zone, Zone::Battlefield);
            assert!(state.objects.get(flash).summoning_sick);
            assert!(surface.suppressions().is_empty());
        }
    }
}

#[test]
fn ordinary_creatures_require_own_main_and_pay_their_exact_cast_cost() {
    for (name, mana, stats) in [
        ("Healer's Hawk", [1, 0, 0, 0, 0, 0], (1, 1)),
        ("Treetop Snarespinner", [0, 0, 0, 0, 1, 3], (1, 4)),
    ] {
        let mut state = ready(Step::Main1);
        let creature = put(&mut state, PlayerId::P0, name, Zone::Hand);
        assert!(engine::step(&mut state, Action::CastSpell(creature)).is_err());
        state.players[0].mana_pool = mana;
        let mut surface = surface();
        announce(&mut surface, &mut state, Action::CastSpell(creature), None);
        assert_eq!(state.players[0].mana_pool, [0; 6]);
        resolve_one(&mut surface, &mut state);
        assert_eq!(state.objects.get(creature).zone, Zone::Battlefield);
        assert_eq!(
            (
                engine::effective_power(&state, creature),
                engine::effective_toughness(&state, creature)
            ),
            stats
        );
        assert!(state.objects.get(creature).summoning_sick);
    }
}

#[test]
fn fleeting_distraction_reduces_either_players_creature_then_draws_and_expires() {
    for controller in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(Step::Main1);
        let target = put(&mut state, controller, "Cathar Commando", Zone::Battlefield);
        let spell = put(&mut state, PlayerId::P0, "Fleeting Distraction", Zone::Hand);
        let mut surface = surface();
        state.players[0].mana_pool[ManaColor::U.pool_index()] = 1;
        announce(
            &mut surface,
            &mut state,
            Action::CastSpell(spell),
            Some(target),
        );
        let library_before = state.players[0].library.len();
        assert_eq!(engine::effective_power(&state, target), 3);
        resolve_one(&mut surface, &mut state);
        assert_eq!(engine::effective_power(&state, target), 2);
        assert_eq!(engine::effective_toughness(&state, target), 1);
        assert_eq!(state.players[0].library.len(), library_before - 1);
        assert_eq!(state.players[0].hand.len(), 1);
        assert_eq!(state.objects.get(spell).zone, Zone::Graveyard);
        state.step = Step::End;
        state.engine.priority_passes = [false, false];
        apply(&mut surface, &mut state, Action::Pass);
        priority(&mut surface, &mut state);
        apply(&mut surface, &mut state, Action::Pass);
        priority(&mut surface, &mut state);
        assert!(state.engine.until_end_of_turn.is_empty());
        assert_eq!(engine::effective_power(&state, target), 3);
    }
}

#[test]
fn fleeting_distraction_does_not_draw_when_its_target_leaves_or_blinks() {
    for blink in [false, true] {
        let mut state = ready(Step::Main1);
        let target = put(
            &mut state,
            PlayerId::P1,
            "Spectral Sailor",
            Zone::Battlefield,
        );
        let spell = put(&mut state, PlayerId::P0, "Fleeting Distraction", Zone::Hand);
        state.players[0].mana_pool[ManaColor::U.pool_index()] = 1;
        let mut surface = surface();
        announce(
            &mut surface,
            &mut state,
            Action::CastSpell(spell),
            Some(target),
        );
        let library_before = state.players[0].library.len();
        move_to(&mut state, target, Zone::Exile);
        if blink {
            move_to(&mut state, target, Zone::Battlefield);
        }
        resolve_one(&mut surface, &mut state);
        assert_eq!(state.players[0].library.len(), library_before);
        assert!(state.players[0].hand.is_empty());
        assert!(state.engine.until_end_of_turn.is_empty());
        assert_eq!(state.objects.get(spell).zone, Zone::Graveyard);
        assert_eq!(state.players[0].mana_pool, [0; 6]);
    }
}

#[test]
fn fleeting_distraction_rejects_a_noncreature_target_without_spending_mana() {
    let mut state = ready(Step::Main1);
    let land = put(&mut state, PlayerId::P1, "Plains", Zone::Battlefield);
    let target = put(
        &mut state,
        PlayerId::P1,
        "Spectral Sailor",
        Zone::Battlefield,
    );
    let spell = put(&mut state, PlayerId::P0, "Fleeting Distraction", Zone::Hand);
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 1;
    engine::step(&mut state, Action::CastSpell(spell)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::ChooseTargets { .. }
    ));
    let before = state.clone();
    assert!(engine::step(&mut state, Action::ChooseTarget(Target::Object(land))).is_err());
    assert_eq!(state, before);
    engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
}

#[test]
fn cathar_targets_either_players_artifacts_and_enchantments_and_pays_before_resolution() {
    for controller in [PlayerId::P0, PlayerId::P1] {
        for name in ["Great Furnace", "Makeshift Munitions"] {
            let mut state = ready(Step::End);
            let cathar = put(
                &mut state,
                PlayerId::P0,
                "Cathar Commando",
                Zone::Battlefield,
            );
            let target = put(&mut state, controller, name, Zone::Battlefield);
            state.players[0].mana_pool[ManaColor::C.pool_index()] = 1;
            let mut surface = surface();
            announce(
                &mut surface,
                &mut state,
                Action::ActivateAbility(cathar, 0),
                Some(target),
            );
            assert_eq!(state.objects.get(cathar).zone, Zone::Graveyard);
            assert_eq!(state.objects.get(target).zone, Zone::Battlefield);
            assert_eq!(state.players[0].mana_pool, [0; 6]);
            resolve_one(&mut surface, &mut state);
            assert_eq!(state.objects.get(target).zone, Zone::Graveyard);
        }
    }
}

#[test]
fn cathar_destroys_artifacts_but_does_not_destroy_indestructible_lands() {
    for (name, expected_zone) in [
        ("Great Furnace", Zone::Graveyard),
        ("Silverbluff Bridge", Zone::Battlefield),
    ] {
        let mut state = ready(Step::Main1);
        let cathar = put(
            &mut state,
            PlayerId::P0,
            "Cathar Commando",
            Zone::Battlefield,
        );
        let target = put(&mut state, PlayerId::P1, name, Zone::Battlefield);
        state.players[0].mana_pool[ManaColor::C.pool_index()] = 1;
        let mut surface = surface();
        announce(
            &mut surface,
            &mut state,
            Action::ActivateAbility(cathar, 0),
            Some(target),
        );
        resolve_one(&mut surface, &mut state);
        assert_eq!(state.objects.get(target).zone, expected_zone);
        assert_eq!(state.objects.get(cathar).zone, Zone::Graveyard);
    }
}

#[test]
fn cathar_illegal_target_rejects_without_costs_and_lost_target_does_not_refund_costs() {
    let mut state = ready(Step::Main1);
    let cathar = put(
        &mut state,
        PlayerId::P0,
        "Cathar Commando",
        Zone::Battlefield,
    );
    let artifact = put(&mut state, PlayerId::P1, "Great Furnace", Zone::Battlefield);
    let creature = put(&mut state, PlayerId::P1, "Healer's Hawk", Zone::Battlefield);
    let land = put(&mut state, PlayerId::P1, "Plains", Zone::Battlefield);
    assert!(engine::step(&mut state, Action::ActivateAbility(cathar, 0)).is_err());
    state.players[0].mana_pool[ManaColor::C.pool_index()] = 1;
    engine::step(&mut state, Action::ActivateAbility(cathar, 0)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::ChooseTargets { .. }
    ));
    let before = state.clone();
    for illegal in [creature, land] {
        assert!(engine::step(&mut state, Action::ChooseTarget(Target::Object(illegal))).is_err());
        assert_eq!(state, before);
    }
    engine::step(&mut state, Action::ChooseTarget(Target::Object(artifact))).unwrap();
    let mut surface = surface();
    priority(&mut surface, &mut state);
    assert_eq!(state.objects.get(cathar).zone, Zone::Graveyard);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
    move_to(&mut state, artifact, Zone::Exile);
    move_to(&mut state, artifact, Zone::Battlefield);
    resolve_one(&mut surface, &mut state);
    assert_eq!(state.objects.get(artifact).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(cathar).zone, Zone::Graveyard);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
}

#[test]
fn cathar_pending_target_and_sacrificed_source_stack_restore_exactly() {
    let mut state = ready(Step::Main1);
    let cathar = put(
        &mut state,
        PlayerId::P0,
        "Cathar Commando",
        Zone::Battlefield,
    );
    let target = put(&mut state, PlayerId::P1, "Great Furnace", Zone::Battlefield);
    state.players[0].mana_pool[ManaColor::C.pool_index()] = 1;
    let mut surface = surface();
    priority(&mut surface, &mut state);
    apply(&mut surface, &mut state, Action::ActivateAbility(cathar, 0));
    assert!(matches!(
        surface.next_decision(&mut state),
        SurfaceDecision::Decision(Decision::ChooseTargets { .. })
    ));
    let mut restored: GameState =
        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    let mut restored_surface = surface.clone();
    for (branch, branch_surface) in [
        (&mut state, &mut surface),
        (&mut restored, &mut restored_surface),
    ] {
        apply(
            branch_surface,
            branch,
            Action::ChooseTarget(Target::Object(target)),
        );
        priority(branch_surface, branch);
        assert_eq!(branch.objects.get(cathar).zone, Zone::Graveyard);
        let encoded = serde_json::to_vec(branch).unwrap();
        let reloaded: GameState = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(*branch, reloaded);
        resolve_one(branch_surface, branch);
        assert_eq!(branch.objects.get(target).zone, Zone::Graveyard);
    }
    assert_eq!(state, restored);
}

#[test]
fn sailor_draw_is_repeatable_untapped_and_survives_source_removal_and_restore() {
    let mut state = ready(Step::Upkeep);
    state.active_player = PlayerId::P1;
    let sailor = put(
        &mut state,
        PlayerId::P0,
        "Spectral Sailor",
        Zone::Battlefield,
    );
    let mut surface = surface();
    state.players[0].mana_pool = [0, 2, 0, 0, 0, 6];
    for _ in 0..2 {
        announce(
            &mut surface,
            &mut state,
            Action::ActivateAbility(sailor, 0),
            None,
        );
        assert!(!state.objects.get(sailor).tapped);
        assert!(state.objects.get(sailor).summoning_sick);
    }
    assert_eq!(state.players[0].mana_pool, [0; 6]);
    assert_eq!(state.stack.len(), 2);
    move_to(&mut state, sailor, Zone::Graveyard);
    let mut restored: GameState =
        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    let mut restored_surface = surface.clone();
    let before = state.players[0].library.len();
    for (branch, branch_surface) in [
        (&mut state, &mut surface),
        (&mut restored, &mut restored_surface),
    ] {
        resolve_one(branch_surface, branch);
        resolve_one(branch_surface, branch);
        assert_eq!(branch.players[0].library.len(), before - 2);
        assert_eq!(branch.players[0].hand.len(), 2);
    }
    assert_eq!(state, restored);
}

#[test]
fn sailor_requires_both_colored_and_generic_mana_and_empty_library_draw_loses() {
    let mut state = ready(Step::End);
    let sailor = put(
        &mut state,
        PlayerId::P0,
        "Spectral Sailor",
        Zone::Battlefield,
    );
    for mana in [[0, 0, 0, 0, 0, 4], [0, 1, 0, 0, 0, 2]] {
        state.players[0].mana_pool = mana;
        let before = state.clone();
        assert!(engine::step(&mut state, Action::ActivateAbility(sailor, 0)).is_err());
        assert_eq!(state, before);
    }
    // Remove the library objects through the event path to keep zone vectors valid.
    for object in state.players[0].library.clone() {
        move_to(&mut state, object, Zone::Exile);
    }
    state.players[0].mana_pool = [0, 1, 0, 0, 0, 3];
    let mut surface = surface();
    announce(
        &mut surface,
        &mut state,
        Action::ActivateAbility(sailor, 0),
        None,
    );
    apply(&mut surface, &mut state, Action::Pass);
    priority(&mut surface, &mut state);
    apply(&mut surface, &mut state, Action::Pass);
    assert!(matches!(
        surface.next_decision(&mut state),
        SurfaceDecision::Decision(Decision::GameOver {
            winner: Some(PlayerId::P1)
        })
    ));
}

#[test]
fn snarespinner_counters_only_friendly_creatures_repeat_without_tapping_and_persist() {
    let mut state = ready(Step::Main1);
    let spider = put(
        &mut state,
        PlayerId::P0,
        "Treetop Snarespinner",
        Zone::Battlefield,
    );
    let other = put(&mut state, PlayerId::P0, "Healer's Hawk", Zone::Battlefield);
    let enemy = put(&mut state, PlayerId::P1, "Healer's Hawk", Zone::Battlefield);
    state.players[0].mana_pool = [0, 0, 0, 0, 2, 4];
    let mut surface = surface();
    priority(&mut surface, &mut state);
    apply(&mut surface, &mut state, Action::ActivateAbility(spider, 0));
    let targets = surface.next_decision(&mut state);
    assert!(
        matches!(targets, SurfaceDecision::Decision(Decision::ChooseTargets { legal_targets, .. })
        if legal_targets.contains(&Target::Object(spider)) && legal_targets.contains(&Target::Object(other))
            && !legal_targets.contains(&Target::Object(enemy)))
    );
    let before = state.clone();
    assert!(surface
        .apply(
            &mut state,
            SurfaceAction::Action(Action::ChooseTarget(Target::Object(enemy)))
        )
        .is_err());
    assert_eq!(state, before);
    apply(
        &mut surface,
        &mut state,
        Action::ChooseTarget(Target::Object(spider)),
    );
    priority(&mut surface, &mut state);
    resolve_one(&mut surface, &mut state);
    announce(
        &mut surface,
        &mut state,
        Action::ActivateAbility(spider, 0),
        Some(other),
    );
    resolve_one(&mut surface, &mut state);
    assert_eq!(state.objects.get(spider).counters.plus1_plus1, 1);
    assert_eq!(state.objects.get(other).counters.plus1_plus1, 1);
    assert_eq!(engine::effective_power(&state, spider), 2);
    assert_eq!(engine::effective_toughness(&state, spider), 5);
    assert!(!state.objects.get(spider).tapped);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
    state.step = Step::End;
    state.engine.priority_passes = [false, false];
    apply(&mut surface, &mut state, Action::Pass);
    priority(&mut surface, &mut state);
    apply(&mut surface, &mut state, Action::Pass);
    priority(&mut surface, &mut state);
    assert_eq!(state.objects.get(spider).counters.plus1_plus1, 1);
}

#[test]
fn snarespinner_activation_requires_own_main_empty_stack_and_full_mana() {
    let mut state = ready(Step::Main1);
    let spider = put(
        &mut state,
        PlayerId::P0,
        "Treetop Snarespinner",
        Zone::Battlefield,
    );
    state.players[0].mana_pool = [0, 0, 0, 0, 1, 2];
    let mut cases = Vec::new();
    let mut upkeep = state.clone();
    upkeep.step = Step::Upkeep;
    cases.push(upkeep);
    let mut opponent_main = state.clone();
    opponent_main.active_player = PlayerId::P1;
    cases.push(opponent_main);
    let mut insufficient = state.clone();
    insufficient.players[0].mana_pool = [0, 0, 0, 0, 1, 1];
    cases.push(insufficient);
    let mut wrong_color = state.clone();
    wrong_color.players[0].mana_pool = [0, 0, 0, 0, 0, 3];
    cases.push(wrong_color);
    let mut surface = surface();
    announce(
        &mut surface,
        &mut state,
        Action::ActivateAbility(spider, 0),
        Some(spider),
    );
    cases.push(state);
    for mut state in cases {
        let before = state.clone();
        assert!(engine::step(&mut state, Action::ActivateAbility(spider, 0)).is_err());
        assert_eq!(state, before);
    }
}

#[test]
fn snarespinner_lost_target_consumes_mana_and_does_not_counter_a_new_incarnation() {
    let mut state = ready(Step::Main2);
    let spider = put(
        &mut state,
        PlayerId::P0,
        "Treetop Snarespinner",
        Zone::Battlefield,
    );
    let target = put(&mut state, PlayerId::P0, "Healer's Hawk", Zone::Battlefield);
    state.players[0].mana_pool = [0, 0, 0, 0, 1, 2];
    let mut surface = surface();
    announce(
        &mut surface,
        &mut state,
        Action::ActivateAbility(spider, 0),
        Some(target),
    );
    move_to(&mut state, target, Zone::Exile);
    move_to(&mut state, target, Zone::Battlefield);
    resolve_one(&mut surface, &mut state);
    assert_eq!(state.objects.get(target).counters.plus1_plus1, 0);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
}

#[test]
fn snarespinner_target_must_still_be_controlled_at_resolution() {
    let mut state = ready(Step::Main1);
    let spider = put(
        &mut state,
        PlayerId::P0,
        "Treetop Snarespinner",
        Zone::Battlefield,
    );
    let target = put(
        &mut state,
        PlayerId::P0,
        "Spectral Sailor",
        Zone::Battlefield,
    );
    state.players[0].mana_pool = [0, 0, 0, 0, 1, 2];
    let mut surface = surface();
    announce(
        &mut surface,
        &mut state,
        Action::ActivateAbility(spider, 0),
        Some(target),
    );
    state.players[0].battlefield.retain(|&id| id != target);
    state.players[1].battlefield.push(target);
    state.objects.get_mut(target).controller = PlayerId::P1;
    resolve_one(&mut surface, &mut state);
    assert_eq!(state.objects.get(target).counters.plus1_plus1, 0);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
}

#[test]
fn flying_restricts_blockers_and_reach_deathtouch_kills_a_larger_flyer_in_combat() {
    for flyer in ["Healer's Hawk", "Spectral Sailor", "Sagu Wildling"] {
        let mut state = ready(Step::DeclareAttackers);
        state.engine.combat.attackers_declared = false;
        state.engine.combat.blockers_declared = false;
        let attacker = put(&mut state, PlayerId::P0, flyer, Zone::Battlefield);
        state.objects.get_mut(attacker).summoning_sick = false;
        let spider = put(
            &mut state,
            PlayerId::P1,
            "Treetop Snarespinner",
            Zone::Battlefield,
        );
        let ground = put(
            &mut state,
            PlayerId::P1,
            "Cathar Commando",
            Zone::Battlefield,
        );
        let mut surface = surface();
        assert!(matches!(
            surface.next_decision(&mut state),
            SurfaceDecision::Decision(Decision::DeclareAttackers { .. })
        ));
        apply(
            &mut surface,
            &mut state,
            Action::DeclareAttackers(vec![attacker]),
        );
        priority(&mut surface, &mut state);
        apply(&mut surface, &mut state, Action::Pass);
        priority(&mut surface, &mut state);
        apply(&mut surface, &mut state, Action::Pass);
        match surface.next_decision(&mut state) {
            SurfaceDecision::DeclareBlockersForAttacker { legal_blockers, .. } => {
                assert!(legal_blockers.contains(&spider));
                assert!(!legal_blockers.contains(&ground));
            }
            other => panic!("blocker choice: {other:?}"),
        }
        surface
            .apply(
                &mut state,
                SurfaceAction::DeclareBlockersForAttacker(vec![spider]),
            )
            .unwrap();
        priority(&mut surface, &mut state);
        apply(&mut surface, &mut state, Action::Pass);
        priority(&mut surface, &mut state);
        apply(&mut surface, &mut state, Action::Pass);
        priority(&mut surface, &mut state);
        assert_eq!(state.step, Step::CombatDamage);
        assert_eq!(state.objects.get(attacker).zone, Zone::Graveyard);
        assert_eq!(state.objects.get(spider).zone, Zone::Battlefield);
        assert_eq!(state.players[1].life, 20);
        assert_eq!(
            state.players[0].life,
            if flyer == "Healer's Hawk" { 21 } else { 20 }
        );
    }
}

#[test]
fn hawk_lifelink_uses_final_damage_and_the_current_controller() {
    let mut state = ready(Step::Main1);
    let hawk = put(&mut state, PlayerId::P0, "Healer's Hawk", Zone::Battlefield);
    assert!(CARD_DEFS[state.objects.get(hawk).card_def as usize]
        .keywords
        .has(Keywords::LIFELINK));
    state.engine.active_replacements.push(ActiveReplacement {
        id: 1,
        source: hawk,
        kind: ReplacementEffectKind::PreventNextDamage {
            target: Target::Player(PlayerId::P1),
            remaining: 1,
        },
    });
    event::propose_and_commit(
        &mut state,
        ProposedEvent::damage(hawk, Target::Player(PlayerId::P1), 1),
    );
    assert_eq!((state.players[0].life, state.players[1].life), (20, 20));
    // Control changes preserve the same source incarnation. Lifelink follows
    // its controller, rather than the original owner of the physical card.
    state.players[0].battlefield.retain(|&id| id != hawk);
    state.players[1].battlefield.push(hawk);
    state.objects.get_mut(hawk).controller = PlayerId::P1;
    event::propose_and_commit(
        &mut state,
        ProposedEvent::damage(hawk, Target::Player(PlayerId::P0), 1),
    );
    assert_eq!((state.players[0].life, state.players[1].life), (19, 21));
}
