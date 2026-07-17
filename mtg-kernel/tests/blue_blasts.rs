//! Integration coverage for the symmetric Blue Elemental Blast / Hydroblast
//! implementation and the already-supported red blast pair.
//!
//! The bounded rules oracle is XMage commit
//! `0723fc0c2be922af47b0ef0539f28114cc23b998`: `BlueElementalBlast.java`
//! blob `55d1601c2021bd5238b6e739a0d952560dbbfd4f`, `Hydroblast.java` blob
//! `c256286f9bed6f201dd0af4bf6dc5b6aee3dd7af`, `Pyroblast.java` blob
//! `538356cc4a861dcace3f1959521cfe0fd29fa07f`, and
//! `RedElementalBlast.java` blob `0f0c804fe49c919e18e8950cc4bd3437fc1177b0`.
//! The shared `CounterTargetEffect`, `DestroyTargetEffect`, and
//! `ColorPredicate` blobs are respectively
//! `7f49db9876aa44ce8396a6693b2fc6e1f3e2977f`,
//! `5271b8fcb5f51cf28444fa8b93a2a664fb1ce287`, and
//! `6ec89935618501a6ec24d877309d0f0d36b05eb4`.
//!
//! Blue Elemental Blast filters for red while targets are chosen;
//! Hydroblast can target any spell/permanent and checks red only when it
//! resolves. XMage's color predicate reads dynamic `getColor(game)` and its
//! destroy effect honors indestructible/regeneration. This kernel slice is
//! deliberately bounded to the frozen pool's static card-definition colors
//! and its existing shared destroy-as-zone-change behavior: no executable
//! effect can change color, and no valid red target exercises either destroy
//! exception.

use mtg_kernel::card_def::{card_id_by_name, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::CommittedEvent;
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::rl::{legal_action_candidates_v1, ActionSemanticV1, TargetRefV1};
use mtg_kernel::state::{
    CastMethodV4, Counters, GameObject, GameState, StackItem, StackItemKind, StackStateV4, Step,
    Target, Zone,
};
use mtg_kernel::surface_v2::SurfaceDecision;

fn card_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

fn card_name(card_def: u16) -> String {
    CARD_DEFS[card_def as usize].name.to_string()
}

fn ready_game() -> GameState {
    let p0_library = [card_id("Island")];
    let p1_library = [card_id("Snow-Covered Forest")];
    let mut state =
        GameState::new_from_libraries(&p0_library, &p1_library, card_name, 0x424C_4153_5453);
    state.step = Step::Main1;
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    put_object(&mut state, PlayerId::P0, "Island", Zone::Battlefield);
    state
}

fn put_object(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id(name);
    let id = state.objects.push(GameObject {
        card_def,
        name: name.to_string(),
        owner: player,
        controller: player,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Counters::default(),
        attachments: Vec::new(),
        v4: mtg_kernel::state::ObjectStateV4::from_card_def(card_def),
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[player.index()].hand.push(id),
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        Zone::Library => state.players[player.index()].library.push(id),
        Zone::Graveyard => state.players[player.index()].graveyard.push(id),
        Zone::Exile => state.exile.push(id),
        Zone::Command => state.command.push(id),
        Zone::Stack => panic!("put_spell_on_stack owns stack objects"),
    }
    id
}

fn put_spell_on_stack(
    state: &mut GameState,
    player: PlayerId,
    name: &str,
    is_copy: bool,
    is_flashback: bool,
) -> ObjectId {
    let card_def = card_id(name);
    let id = state.objects.push(GameObject {
        card_def,
        name: name.to_string(),
        owner: player,
        controller: player,
        zone: Zone::Stack,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Counters::default(),
        attachments: Vec::new(),
        v4: mtg_kernel::state::ObjectStateV4::from_card_def(card_def),
        plotted_turn: None,
        zone_change_count: 0,
    });
    state.stack.push(StackItem {
        kind: StackItemKind::Spell,
        source: id,
        controller: player,
        targets: Vec::new(),
        is_copy,
        inline_effect: None,
        discarded: Vec::new(),
        is_flashback,
        mode_chosen: 0,
        madness_offer: false,
        kicked: false,
        v4: StackStateV4::spell(CastMethodV4::Normal),
    });
    id
}

fn mode_actions(state: &GameState, decision: &Decision) -> Vec<(u8, String)> {
    legal_action_candidates_v1(&SurfaceDecision::Decision(decision.clone()), state)
        .unwrap()
        .into_iter()
        .map(|candidate| match candidate.record.semantic {
            ActionSemanticV1::ChooseSpellMode { mode_index, .. } => {
                (mode_index, candidate.record.stable_id)
            }
            other => panic!("unexpected mode action: {other:?}"),
        })
        .collect()
}

fn target_actions(state: &GameState, decision: &Decision) -> Vec<(ObjectId, String)> {
    legal_action_candidates_v1(&SurfaceDecision::Decision(decision.clone()), state)
        .unwrap()
        .into_iter()
        .map(|candidate| match candidate.record.semantic {
            ActionSemanticV1::ChooseTarget {
                target: TargetRefV1::Object { object },
                ..
            } => (ObjectId(object.arena_id), candidate.record.stable_id),
            other => panic!("unexpected target action: {other:?}"),
        })
        .collect()
}

fn cast_for_mode(state: &mut GameState, blast: ObjectId, mode: u8) -> Decision {
    engine::step(state, Action::CastSpell(blast)).unwrap();
    let decision = engine::advance_until_decision(state);
    match decision {
        Decision::ChooseSpellMode {
            player: PlayerId::P0,
            spell,
            mode_count: 2,
        } if spell == blast => {
            engine::step(state, Action::ChooseSpellMode(mode)).unwrap();
            engine::advance_until_decision(state)
        }
        decision @ Decision::ChooseTargets { spell, .. } if spell == blast => {
            assert_eq!(
                state.engine.pending_cast.as_ref().unwrap().mode_chosen,
                Some(mode),
                "the sole viable mode must retain its printed index"
            );
            decision
        }
        other => panic!("expected a Blast mode/target decision, got {other:?}"),
    }
}

fn choose_target(state: &mut GameState, decision: &Decision, target: ObjectId) {
    assert!(matches!(
        decision,
        Decision::ChooseTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(target))
    ));
    engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
}

fn resolve_blast(state: &mut GameState, blast: ObjectId) -> Decision {
    loop {
        let decision = engine::advance_until_decision(state);
        if state.objects.get(blast).zone != Zone::Stack {
            return decision;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected decision while resolving Blast: {other:?}"),
        }
    }
}

#[test]
fn blue_elemental_blast_filters_red_targets_and_stabilizes_rl_actions() {
    let mut state = ready_game();
    let red_spell = put_spell_on_stack(&mut state, PlayerId::P1, "Lightning Bolt", false, false);
    let blue_spell = put_spell_on_stack(&mut state, PlayerId::P1, "Counterspell", false, false);
    let red_permanent = put_object(&mut state, PlayerId::P1, "Guttersnipe", Zone::Battlefield);
    let blue_permanent = put_object(
        &mut state,
        PlayerId::P1,
        "Cryptic Serpent",
        Zone::Battlefield,
    );
    let blast = put_object(&mut state, PlayerId::P0, "Blue Elemental Blast", Zone::Hand);

    engine::step(&mut state, Action::CastSpell(blast)).unwrap();
    let mode_decision = engine::advance_until_decision(&mut state);
    assert!(matches!(
        mode_decision,
        Decision::ChooseSpellMode {
            spell,
            mode_count: 2,
            ..
        } if spell == blast
    ));
    let mode_snapshot = state.snapshot();
    let mode_hash = state.state_hash();
    let first_mode_actions = mode_actions(&state, &mode_decision);
    assert_eq!(
        first_mode_actions,
        vec![
            (0, "legal-action-v4:b582861bd901b918".to_string()),
            (1, "legal-action-v4:134dd78a2ef743b3".to_string()),
        ]
    );

    engine::step(&mut state, Action::ChooseSpellMode(0)).unwrap();
    let counter_targets = engine::advance_until_decision(&mut state);
    let first_counter_actions = target_actions(&state, &counter_targets);
    assert_eq!(
        first_counter_actions,
        vec![(red_spell, "legal-action-v4:fe3c70e0ebc2eeed".to_string(),)]
    );
    assert!(!first_counter_actions
        .iter()
        .any(|(id, _)| *id == blue_spell));

    state.restore(&mode_snapshot);
    assert_eq!(state.state_hash(), mode_hash);
    let restored_mode = engine::advance_until_decision(&mut state);
    assert_eq!(mode_actions(&state, &restored_mode), first_mode_actions);
    engine::step(&mut state, Action::ChooseSpellMode(1)).unwrap();
    let destroy_targets = engine::advance_until_decision(&mut state);
    let first_destroy_actions = target_actions(&state, &destroy_targets);
    assert_eq!(
        first_destroy_actions,
        vec![(
            red_permanent,
            "legal-action-v4:351b4f2ab6d707db".to_string(),
        )]
    );
    assert!(!first_destroy_actions
        .iter()
        .any(|(id, _)| *id == blue_permanent));

    let target_snapshot = state.snapshot();
    let target_hash = state.state_hash();
    choose_target(&mut state, &destroy_targets, red_permanent);
    state.restore(&target_snapshot);
    assert_eq!(state.state_hash(), target_hash);
    let restored_targets = engine::advance_until_decision(&mut state);
    assert_eq!(
        target_actions(&state, &restored_targets),
        first_destroy_actions
    );
}

#[test]
fn hydroblast_can_target_any_color_but_only_counters_red() {
    for (target_name, should_counter) in [("Lightning Bolt", true), ("Counterspell", false)] {
        let mut state = ready_game();
        let target = put_spell_on_stack(&mut state, PlayerId::P1, target_name, false, false);
        let hydro = put_object(&mut state, PlayerId::P0, "Hydroblast", Zone::Hand);
        let decision = cast_for_mode(&mut state, hydro, 0);
        choose_target(&mut state, &decision, target);
        resolve_blast(&mut state, hydro);

        assert_eq!(state.objects.get(hydro).zone, Zone::Graveyard);
        assert_eq!(
            state.objects.get(target).zone,
            if should_counter {
                Zone::Graveyard
            } else {
                Zone::Stack
            },
            "Hydroblast target {target_name}"
        );
    }
}

#[test]
fn both_blue_blasts_counter_red_copies_and_flashback_spells_correctly() {
    for blast_name in ["Blue Elemental Blast", "Hydroblast"] {
        let mut copied = ready_game();
        let target = put_spell_on_stack(&mut copied, PlayerId::P1, "Lightning Bolt", true, false);
        let blast = put_object(&mut copied, PlayerId::P0, blast_name, Zone::Hand);
        let decision = cast_for_mode(&mut copied, blast, 0);
        choose_target(&mut copied, &decision, target);
        resolve_blast(&mut copied, blast);
        assert_eq!(copied.objects.get(target).zone, Zone::Stack, "{blast_name}");
        assert!(!copied.players[1].graveyard.contains(&target));
        assert!(!copied.exile.contains(&target));
        assert!(!copied.engine.event_history.iter().any(|event| matches!(
            event,
            CommittedEvent::ZoneChange { object, .. } if *object == target
        )));

        let mut flashback = ready_game();
        let target = put_spell_on_stack(&mut flashback, PlayerId::P1, "Lava Dart", false, true);
        let blast = put_object(&mut flashback, PlayerId::P0, blast_name, Zone::Hand);
        let decision = cast_for_mode(&mut flashback, blast, 0);
        choose_target(&mut flashback, &decision, target);
        resolve_blast(&mut flashback, blast);
        assert_eq!(
            flashback.objects.get(target).zone,
            Zone::Exile,
            "{blast_name}"
        );
        assert!(flashback.exile.contains(&target));
        assert!(!flashback.players[1].graveyard.contains(&target));
    }
}

#[test]
fn blue_blast_destroy_modes_are_filtered_at_targeting_vs_checked_at_resolution() {
    for (blast_name, target_name, should_destroy) in [
        ("Blue Elemental Blast", "Guttersnipe", true),
        ("Hydroblast", "Guttersnipe", true),
        ("Hydroblast", "Cryptic Serpent", false),
    ] {
        let mut state = ready_game();
        let target = put_object(&mut state, PlayerId::P1, target_name, Zone::Battlefield);
        let blast = put_object(&mut state, PlayerId::P0, blast_name, Zone::Hand);
        let decision = cast_for_mode(&mut state, blast, 1);
        choose_target(&mut state, &decision, target);
        resolve_blast(&mut state, blast);
        assert_eq!(state.objects.get(blast).zone, Zone::Graveyard);
        assert_eq!(
            state.objects.get(target).zone,
            if should_destroy {
                Zone::Graveyard
            } else {
                Zone::Battlefield
            },
            "{blast_name} targeting {target_name}"
        );
    }

    let mut filtered = ready_game();
    let blue = put_object(
        &mut filtered,
        PlayerId::P1,
        "Cryptic Serpent",
        Zone::Battlefield,
    );
    let red = put_object(
        &mut filtered,
        PlayerId::P1,
        "Guttersnipe",
        Zone::Battlefield,
    );
    let blast = put_object(
        &mut filtered,
        PlayerId::P0,
        "Blue Elemental Blast",
        Zone::Hand,
    );
    let decision = cast_for_mode(&mut filtered, blast, 1);
    let targets = target_actions(&filtered, &decision)
        .into_iter()
        .map(|(id, _)| id)
        .collect::<Vec<_>>();
    assert_eq!(targets, vec![red]);
    assert!(!targets.contains(&blue));
}

#[test]
fn stale_red_spell_targets_fizzle_both_blue_blasts_without_double_moving() {
    for blast_name in ["Blue Elemental Blast", "Hydroblast"] {
        let mut state = ready_game();
        let target = put_spell_on_stack(&mut state, PlayerId::P1, "Lightning Bolt", false, false);
        let blast = put_object(&mut state, PlayerId::P0, blast_name, Zone::Hand);
        let decision = cast_for_mode(&mut state, blast, 0);
        choose_target(&mut state, &decision, target);

        state.stack.retain(|item| item.source != target);
        state.objects.get_mut(target).zone = Zone::Graveyard;
        state.players[PlayerId::P1.index()].graveyard.push(target);
        resolve_blast(&mut state, blast);

        assert_eq!(
            state.objects.get(blast).zone,
            Zone::Graveyard,
            "{blast_name}"
        );
        assert_eq!(
            state.objects.get(target).zone,
            Zone::Graveyard,
            "{blast_name}"
        );
        assert_eq!(
            state.players[PlayerId::P1.index()]
                .graveyard
                .iter()
                .filter(|&&id| id == target)
                .count(),
            1,
            "{blast_name} must not move a stale target twice"
        );
    }
}

#[test]
fn existing_red_blasts_retain_the_same_modal_targeting_contract() {
    let mut state = ready_game();
    put_object(&mut state, PlayerId::P0, "Mountain", Zone::Battlefield);
    let blue_spell = put_spell_on_stack(&mut state, PlayerId::P1, "Counterspell", false, false);
    let red_spell = put_spell_on_stack(&mut state, PlayerId::P1, "Lightning Bolt", false, false);
    let pyro = put_object(&mut state, PlayerId::P0, "Pyroblast", Zone::Hand);
    let pyro_decision = cast_for_mode(&mut state, pyro, 0);
    let pyro_targets = target_actions(&state, &pyro_decision)
        .into_iter()
        .map(|(id, _)| id)
        .collect::<Vec<_>>();
    assert!(pyro_targets.contains(&blue_spell));
    assert!(pyro_targets.contains(&red_spell));

    let mut reb_state = ready_game();
    put_object(&mut reb_state, PlayerId::P0, "Mountain", Zone::Battlefield);
    let blue_spell = put_spell_on_stack(&mut reb_state, PlayerId::P1, "Counterspell", false, false);
    let red_spell =
        put_spell_on_stack(&mut reb_state, PlayerId::P1, "Lightning Bolt", false, false);
    let reb = put_object(
        &mut reb_state,
        PlayerId::P0,
        "Red Elemental Blast",
        Zone::Hand,
    );
    let reb_decision = cast_for_mode(&mut reb_state, reb, 0);
    assert_eq!(
        target_actions(&reb_state, &reb_decision)
            .into_iter()
            .map(|(id, _)| id)
            .collect::<Vec<_>>(),
        vec![blue_spell]
    );
    assert_ne!(blue_spell, red_spell);
}
