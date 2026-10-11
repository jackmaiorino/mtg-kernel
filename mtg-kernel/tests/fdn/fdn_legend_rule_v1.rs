#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{card_id_by_name, Keywords};
use mtg_kernel::engine::{self, Action, Decision, EffectDuration, Layers, UntilEndOfTurnEffect};
use mtg_kernel::event::{self, CommittedEvent, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::rl::{self, ActionSemanticV1};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Zone};
use mtg_kernel::surface_v2::{
    HarnessSurfaceV2, PriorityModeV1, SuppressionAuditMode, SurfaceDecision,
};
use mtg_kernel::trigger;

const DWYNEN: &str = "Dwynen, Gilt-Leaf Daen";

fn ready() -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state =
        GameState::new_from_libraries(&[forest; 40], &[forest; 40], |_| "Forest".into(), 123);
    state.step = Step::Main1;
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
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        Zone::Hand => state.players[player.index()].hand.push(id),
        _ => panic!("test helper zone"),
    }
    id
}

fn checkpoint(state: &mut GameState) {
    let pending = trigger::collect_and_process(state);
    state.engine.pending_triggers.extend(pending);
}

fn choice(state: &mut GameState, player: PlayerId, expected: &[ObjectId]) {
    assert_eq!(
        engine::advance_until_decision(state),
        Decision::ChooseLegendPermanent {
            player,
            candidates: expected.to_vec(),
        }
    );
}

#[test]
fn either_physical_copy_can_be_kept_from_three_legends() {
    for keep_index in 0..3 {
        let mut state = ready();
        let legends: Vec<_> = (0..3)
            .map(|_| put(&mut state, PlayerId::P0, DWYNEN, Zone::Battlefield))
            .collect();
        checkpoint(&mut state);
        choice(&mut state, PlayerId::P0, &legends);
        engine::step(
            &mut state,
            Action::ChooseLegendPermanent(legends[keep_index]),
        )
        .unwrap();
        assert!(legends
            .iter()
            .all(|&id| state.objects.get(id).zone == Zone::Battlefield));
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
        for (index, id) in legends.into_iter().enumerate() {
            assert_eq!(
                state.objects.get(id).zone,
                if index == keep_index {
                    Zone::Battlefield
                } else {
                    Zone::Graveyard
                }
            );
        }
        assert!(state.pending_legend_rule_v1.is_none());
    }
}

#[test]
fn one_copy_for_each_controller_does_not_invoke_the_legend_rule() {
    let mut state = ready();
    put(&mut state, PlayerId::P0, DWYNEN, Zone::Battlefield);
    put(&mut state, PlayerId::P1, DWYNEN, Zone::Battlefield);
    checkpoint(&mut state);
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert!(state
        .players
        .iter()
        .all(|player| player.battlefield.len() == 1));
}

#[test]
fn both_controllers_choose_in_apnap_order_before_any_moves() {
    let mut state = ready();
    state.active_player = PlayerId::P1;
    let p0: Vec<_> = (0..2)
        .map(|_| put(&mut state, PlayerId::P0, DWYNEN, Zone::Battlefield))
        .collect();
    let p1: Vec<_> = (0..2)
        .map(|_| put(&mut state, PlayerId::P1, DWYNEN, Zone::Battlefield))
        .collect();
    checkpoint(&mut state);
    choice(&mut state, PlayerId::P1, &p1);
    engine::step(&mut state, Action::ChooseLegendPermanent(p1[1])).unwrap();
    choice(&mut state, PlayerId::P0, &p0);
    assert!(p0
        .iter()
        .chain(&p1)
        .all(|&id| state.objects.get(id).zone == Zone::Battlefield));
    engine::step(&mut state, Action::ChooseLegendPermanent(p0[0])).unwrap();
    engine::advance_until_decision(&mut state);
    assert_eq!(state.players[0].battlefield, vec![p0[0]]);
    assert_eq!(state.players[1].battlefield, vec![p1[1]]);
}

#[test]
fn indestructible_legend_goes_to_its_owner_without_sacrifice() {
    let mut state = ready();
    let keep = put(&mut state, PlayerId::P0, DWYNEN, Zone::Battlefield);
    let other = put(&mut state, PlayerId::P1, DWYNEN, Zone::Battlefield);
    state.players[1].battlefield.retain(|&id| id != other);
    state.players[0].battlefield.push(other);
    state.objects.get_mut(other).controller = PlayerId::P0;
    state
        .engine
        .until_end_of_turn
        .push(UntilEndOfTurnEffect::ResolvedObjectKeywordEffect {
            object_id: other,
            object_zone_change_count: 0,
            layer: Layers::ABILITY_ADDING,
            timestamp: 1,
            duration: EffectDuration::EndOfTurn,
            keywords: Keywords::INDESTRUCTIBLE,
        });
    checkpoint(&mut state);
    choice(&mut state, PlayerId::P0, &[keep, other]);
    engine::step(&mut state, Action::ChooseLegendPermanent(keep)).unwrap();
    engine::advance_until_decision(&mut state);
    assert_eq!(state.players[1].graveyard, vec![other]);
    assert!(state.players[0].graveyard.is_empty());
    assert!(!state
        .engine
        .event_history
        .iter()
        .any(|event| matches!(event, CommittedEvent::Sacrificed { .. })));
}

#[test]
fn choosing_a_lethally_damaged_legend_does_not_save_the_other() {
    let mut state = ready();
    let lethal = put(&mut state, PlayerId::P0, DWYNEN, Zone::Battlefield);
    let healthy = put(&mut state, PlayerId::P0, DWYNEN, Zone::Battlefield);
    state.objects.get_mut(lethal).damage = 5;
    assert_eq!(engine::effective_toughness(&state, lethal), 5);
    checkpoint(&mut state);
    choice(&mut state, PlayerId::P0, &[lethal, healthy]);
    engine::step(&mut state, Action::ChooseLegendPermanent(lethal)).unwrap();
    engine::advance_until_decision(&mut state);
    assert!(state.players[0].battlefield.is_empty());
    assert_eq!(state.players[0].graveyard, vec![lethal, healthy]);
}

#[test]
fn removing_a_lord_finishes_secondary_lethal_checks_before_priority() {
    let mut state = ready();
    let keep = put(&mut state, PlayerId::P0, DWYNEN, Zone::Battlefield);
    let other = put(&mut state, PlayerId::P0, DWYNEN, Zone::Battlefield);
    let elf = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    state.objects.get_mut(elf).damage = 2;
    assert_eq!(engine::effective_toughness(&state, elf), 3);
    checkpoint(&mut state);
    choice(&mut state, PlayerId::P0, &[keep, other]);
    engine::step(&mut state, Action::ChooseLegendPermanent(keep)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(state.objects.get(elf).zone, Zone::Graveyard);
    assert_eq!(state.players[0].battlefield, vec![keep]);
}

#[test]
fn entry_triggers_wait_for_the_legend_choice_even_when_the_entry_dies() {
    let mut state = ready();
    let keep = put(&mut state, PlayerId::P0, DWYNEN, Zone::Battlefield);
    let ranger = put(
        &mut state,
        PlayerId::P0,
        "Beast-Kin Ranger",
        Zone::Battlefield,
    );
    let entering = put(&mut state, PlayerId::P0, DWYNEN, Zone::Hand);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(entering, Zone::Battlefield),
    );
    checkpoint(&mut state);
    assert!(state.engine.pending_triggers.is_empty());
    assert!(state.stack.is_empty());
    choice(&mut state, PlayerId::P0, &[keep, entering]);
    let surface = HarnessSurfaceV2::new_with_priority_mode_v1(
        PriorityModeV1::EngineWindowsV1,
        SuppressionAuditMode::Full,
    );
    let observation = rl::observe_v2(&state, &surface, PlayerId::P0, 0).unwrap();
    assert_eq!(
        observation.projection.engine_context.pending_triggers.len(),
        1
    );
    assert_eq!(
        observation.projection.engine_context.pending_triggers[0]
            .source
            .as_ref()
            .unwrap()
            .arena_id,
        ranger.0
    );
    engine::step(&mut state, Action::ChooseLegendPermanent(keep)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(state.stack.len(), 1);
    assert_eq!(state.stack[0].source, ranger);
    engine::step(&mut state, Action::Pass).unwrap();
    engine::advance_until_decision(&mut state);
    engine::step(&mut state, Action::Pass).unwrap();
    engine::advance_until_decision(&mut state);
    assert_eq!(engine::effective_power(&state, ranger), 5);
}

#[test]
fn invalid_answers_and_priority_actions_leave_the_pass_unchanged() {
    let mut state = ready();
    let first = put(&mut state, PlayerId::P0, DWYNEN, Zone::Battlefield);
    let second = put(&mut state, PlayerId::P0, DWYNEN, Zone::Battlefield);
    let elf = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    checkpoint(&mut state);
    choice(&mut state, PlayerId::P0, &[first, second]);
    let before = state.clone();
    for action in [
        Action::Pass,
        Action::ActivateManaAbility(elf),
        Action::ChooseLegendPermanent(elf),
    ] {
        assert!(engine::step(&mut state, action).is_err());
        assert_eq!(state, before);
    }
}

#[test]
fn returned_incarnation_cannot_answer_an_older_choice() {
    let mut state = ready();
    let first = put(&mut state, PlayerId::P0, DWYNEN, Zone::Battlefield);
    let second = put(&mut state, PlayerId::P0, DWYNEN, Zone::Battlefield);
    checkpoint(&mut state);
    choice(&mut state, PlayerId::P0, &[first, second]);
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(first, Zone::Hand));
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(first, Zone::Battlefield),
    );
    let before = state.clone();
    assert!(engine::step(&mut state, Action::ChooseLegendPermanent(first)).is_err());
    assert_eq!(state, before);
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::Halted { .. }
    ));
}

#[test]
fn partial_apnap_choices_restore_with_identical_hash_and_next_transition() {
    let mut state = ready();
    let p0: Vec<_> = (0..2)
        .map(|_| put(&mut state, PlayerId::P0, DWYNEN, Zone::Battlefield))
        .collect();
    let p1: Vec<_> = (0..2)
        .map(|_| put(&mut state, PlayerId::P1, DWYNEN, Zone::Battlefield))
        .collect();
    checkpoint(&mut state);
    choice(&mut state, PlayerId::P0, &p0);
    let before_hash = state.state_hash();
    engine::step(&mut state, Action::ChooseLegendPermanent(p0[1])).unwrap();
    assert_ne!(state.state_hash(), before_hash);
    choice(&mut state, PlayerId::P1, &p1);
    let mut restored: GameState =
        serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    assert_eq!(state, restored);
    assert_eq!(state.state_hash(), restored.state_hash());
    for game in [&mut state, &mut restored] {
        engine::step(game, Action::ChooseLegendPermanent(p1[0])).unwrap();
        engine::advance_until_decision(game);
    }
    assert_eq!(state, restored);
}

#[test]
fn public_candidates_and_chosen_prefix_project_without_hidden_cards() {
    let mut state = ready();
    let p0: Vec<_> = (0..2)
        .map(|_| put(&mut state, PlayerId::P0, DWYNEN, Zone::Battlefield))
        .collect();
    let p1: Vec<_> = (0..2)
        .map(|_| put(&mut state, PlayerId::P1, DWYNEN, Zone::Battlefield))
        .collect();
    checkpoint(&mut state);
    choice(&mut state, PlayerId::P0, &p0);
    engine::step(&mut state, Action::ChooseLegendPermanent(p0[1])).unwrap();
    let decision = SurfaceDecision::Decision(engine::advance_until_decision(&mut state));
    let actions = rl::legal_action_candidates_v1(&decision, &state).unwrap();
    assert_eq!(actions.len(), 2);
    assert!(actions
        .iter()
        .all(|action| matches!(&action.record.semantic,
        ActionSemanticV1::ChooseLegendPermanent { candidates, .. } if candidates.len() == 2)));
    assert_ne!(actions[0].record.stable_id, actions[1].record.stable_id);
    let surface = HarnessSurfaceV2::new_with_priority_mode_v1(
        PriorityModeV1::EngineWindowsV1,
        SuppressionAuditMode::Full,
    );
    let observation = rl::observe_v2(&state, &surface, PlayerId::P1, 1).unwrap();
    let groups = observation
        .projection
        .engine_context
        .pending_legend_rule
        .unwrap();
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].kept.as_ref().unwrap().arena_id, p0[1].0);
    assert!(groups[1].kept.is_none());
    assert_eq!(
        groups[1]
            .candidates
            .iter()
            .map(|card| card.arena_id)
            .collect::<Vec<_>>(),
        p1.iter().map(|id| id.0).collect::<Vec<_>>()
    );
    assert!(groups
        .iter()
        .flat_map(|group| &group.candidates)
        .all(|card| card.zone == Zone::Battlefield));
    assert_eq!(
        rl::acting_player_for_surface_decision(&decision, &state),
        Some(PlayerId::P1)
    );
}

#[test]
fn a_second_cast_reaches_the_legend_choice_before_resolution_priority() {
    let mut state = ready();
    let first = put(&mut state, PlayerId::P0, DWYNEN, Zone::Battlefield);
    let second = put(&mut state, PlayerId::P0, DWYNEN, Zone::Hand);
    state.players[0].mana_pool = [0, 0, 0, 0, 2, 2];
    assert!(
        matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&second))
    );
    engine::step(&mut state, Action::CastSpell(second)).unwrap();
    for _ in 0..2 {
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
        engine::step(&mut state, Action::Pass).unwrap();
    }
    choice(&mut state, PlayerId::P0, &[first, second]);
    assert!(state.stack.is_empty());
    assert_eq!(state.players[0].mana_pool, [0; 6]);
    engine::step(&mut state, Action::ChooseLegendPermanent(second)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass {
            player: PlayerId::P0,
            ..
        }
    ));
    assert_eq!(state.players[0].battlefield, vec![second]);
}

#[test]
fn a_simultaneous_player_loss_is_committed_with_the_legend_moves() {
    let mut state = ready();
    let first = put(&mut state, PlayerId::P0, DWYNEN, Zone::Battlefield);
    let second = put(&mut state, PlayerId::P0, DWYNEN, Zone::Battlefield);
    state.players[0].life = 0;
    checkpoint(&mut state);
    choice(&mut state, PlayerId::P0, &[first, second]);
    assert!(!state.players[0].has_lost);
    engine::step(&mut state, Action::ChooseLegendPermanent(first)).unwrap();
    assert_eq!(
        engine::advance_until_decision(&mut state),
        Decision::GameOver {
            winner: Some(PlayerId::P1)
        }
    );
    assert_eq!(state.objects.get(second).zone, Zone::Graveyard);
}

#[test]
fn absent_legend_continuation_preserves_legacy_json_shape() {
    let state = ready();
    assert!(!serde_json::to_string(&state)
        .unwrap()
        .contains("pending_legend_rule_v1"));
    let restored: GameState =
        serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    assert_eq!(restored.state_hash(), state.state_hash());
}

#[test]
fn current_names_define_independent_legend_groups_for_one_controller() {
    let mut state = ready();
    let named: Vec<_> = (0..2)
        .map(|_| put(&mut state, PlayerId::P0, DWYNEN, Zone::Battlefield))
        .collect();
    let other: Vec<_> = (0..2)
        .map(|_| put(&mut state, PlayerId::P0, DWYNEN, Zone::Battlefield))
        .collect();
    for &id in &other {
        state.objects.get_mut(id).name = "Other legendary name".into();
    }
    checkpoint(&mut state);
    choice(&mut state, PlayerId::P0, &named);
    engine::step(&mut state, Action::ChooseLegendPermanent(named[1])).unwrap();
    choice(&mut state, PlayerId::P0, &other);
    assert_eq!(state.players[0].battlefield.len(), 4);
    engine::step(&mut state, Action::ChooseLegendPermanent(other[0])).unwrap();
    engine::advance_until_decision(&mut state);
    assert_eq!(state.players[0].battlefield, vec![named[1], other[0]]);
}
