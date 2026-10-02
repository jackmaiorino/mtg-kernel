//! Combat fixture cards: exact temporary boosts, continuous Elf bonuses,
//! declaration triggers and their resolution-time counts.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{card_id_by_name, CardCapability, Keywords, CARD_DEFS};
use mtg_kernel::combat_damage_v1::enable_foundations_combat_v1;
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Zone};
use mtg_kernel::surface_v2::{
    HarnessSurfaceV2, PriorityModeV1, SuppressionAuditMode, SurfaceAction, SurfaceDecision,
};
use mtg_kernel::trigger;

fn ready() -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state =
        GameState::new_from_libraries(&[forest; 40], &[forest; 40], |_| "Forest".into(), 123);
    state.step = Step::Main1;
    enable_foundations_combat_v1(&mut state).unwrap();
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

fn queue(state: &mut GameState) {
    let triggers = trigger::collect_and_process(state);
    state.engine.pending_triggers.extend(triggers);
}

fn move_to(state: &mut GameState, object: ObjectId, zone: Zone) {
    event::propose_and_commit(state, ProposedEvent::zone_change(object, zone));
}

fn enter(state: &mut GameState, player: PlayerId, name: &str) -> ObjectId {
    let id = put(state, player, name, Zone::Hand);
    move_to(state, id, Zone::Battlefield);
    queue(state);
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

fn apply(surface: &mut HarnessSurfaceV2, state: &mut GameState, action: Action) {
    surface.apply(state, SurfaceAction::Action(action)).unwrap();
}

fn drain(surface: &mut HarnessSurfaceV2, state: &mut GameState) {
    for _ in 0..100 {
        match next(surface, state) {
            Decision::OrderTriggers { pending, .. } => apply(
                surface,
                state,
                Action::OrderTriggers((0..pending.len()).collect()),
            ),
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return,
            Decision::CastSpellOrPass { .. } => apply(surface, state, Action::Pass),
            other => panic!("unexpected choice: {other:?}"),
        }
    }
    panic!("trigger drain did not finish");
}

fn cast_overrun(surface: &mut HarnessSurfaceV2, state: &mut GameState) {
    let spell = put(state, PlayerId::P0, "Overrun", Zone::Hand);
    state.players[0].mana_pool = [0, 0, 0, 0, 3, 2];
    assert!(
        matches!(next(surface, state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&spell))
    );
    apply(surface, state, Action::CastSpell(spell));
    drain(surface, state);
    assert_eq!(state.objects.get(spell).zone, Zone::Graveyard);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
}

fn declare(surface: &mut HarnessSurfaceV2, state: &mut GameState, attackers: Vec<ObjectId>) {
    state.step = Step::DeclareAttackers;
    assert!(matches!(
        next(surface, state),
        Decision::DeclareAttackers { .. }
    ));
    apply(surface, state, Action::DeclareAttackers(attackers));
}

#[test]
fn definitions_append_without_reusing_prior_card_ids() {
    assert_eq!(CARD_DEFS.len(), 186);
    for (offset, name) in ["Beast-Kin Ranger", "Overrun", "Dwynen, Gilt-Leaf Daen"]
        .into_iter()
        .enumerate()
    {
        let id = card_id_by_name(name).unwrap();
        assert_eq!(usize::from(id), 178 + offset);
        assert_eq!(CARD_DEFS[usize::from(id)].capability, CardCapability::Full);
    }
    let mut state = ready();
    let ranger = put(
        &mut state,
        PlayerId::P0,
        "Beast-Kin Ranger",
        Zone::Battlefield,
    );
    let dwynen = put(
        &mut state,
        PlayerId::P0,
        "Dwynen, Gilt-Leaf Daen",
        Zone::Battlefield,
    );
    assert!(engine::has_effective_keyword(
        &state,
        ranger,
        Keywords::TRAMPLE
    ));
    assert!(engine::has_effective_keyword(
        &state,
        dwynen,
        Keywords::REACH
    ));
    assert_eq!(
        (
            engine::effective_power(&state, dwynen),
            engine::effective_toughness(&state, dwynen)
        ),
        (3, 4)
    );
}

#[test]
fn ranger_observes_each_other_friendly_creature_including_tokens() {
    let mut state = ready();
    let ranger = enter(&mut state, PlayerId::P0, "Beast-Kin Ranger");
    assert!(state.engine.pending_triggers.is_empty());
    enter(&mut state, PlayerId::P1, "Healer's Hawk");
    enter(&mut state, PlayerId::P0, "Forest");
    assert!(state.engine.pending_triggers.is_empty());
    for _ in 0..2 {
        event::propose_and_commit(
            &mut state,
            ProposedEvent::create_token(card_id_by_name("Knight Token").unwrap(), PlayerId::P0),
        );
    }
    queue(&mut state);
    assert_eq!(state.engine.pending_triggers.len(), 2);
    assert_eq!(engine::effective_power(&state, ranger), 3);
    drain(&mut surface(), &mut state);
    assert_eq!(
        (
            engine::effective_power(&state, ranger),
            engine::effective_toughness(&state, ranger)
        ),
        (5, 3)
    );
}

#[test]
fn simultaneous_entries_trigger_each_ranger_for_the_other() {
    let mut state = ready();
    let a = put(&mut state, PlayerId::P0, "Beast-Kin Ranger", Zone::Hand);
    let b = put(&mut state, PlayerId::P0, "Beast-Kin Ranger", Zone::Hand);
    event::propose_and_commit_batch(
        &mut state,
        vec![
            ProposedEvent::zone_change(a, Zone::Battlefield),
            ProposedEvent::zone_change(b, Zone::Battlefield),
        ],
    );
    queue(&mut state);
    assert_eq!(state.engine.pending_triggers.len(), 2);
    drain(&mut surface(), &mut state);
    assert_eq!(
        (
            engine::effective_power(&state, a),
            engine::effective_power(&state, b)
        ),
        (4, 4)
    );
}

#[test]
fn ranger_boost_does_not_follow_a_returned_source() {
    let mut state = ready();
    let ranger = put(
        &mut state,
        PlayerId::P0,
        "Beast-Kin Ranger",
        Zone::Battlefield,
    );
    enter(&mut state, PlayerId::P0, "Healer's Hawk");
    assert_eq!(state.engine.pending_triggers.len(), 1);
    move_to(&mut state, ranger, Zone::Hand);
    move_to(&mut state, ranger, Zone::Battlefield);
    queue(&mut state);
    drain(&mut surface(), &mut state);
    assert_eq!(engine::effective_power(&state, ranger), 3);
}

#[test]
fn pending_ranger_boost_restores_the_same_source_binding_and_transition() {
    let mut state = ready();
    let ranger = put(
        &mut state,
        PlayerId::P0,
        "Beast-Kin Ranger",
        Zone::Battlefield,
    );
    enter(&mut state, PlayerId::P0, "Healer's Hawk");
    let mut restored: GameState =
        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    drain(&mut surface(), &mut state);
    drain(&mut surface(), &mut restored);
    assert_eq!(engine::effective_power(&state, ranger), 4);
    assert_eq!(
        serde_json::to_vec(&state).unwrap(),
        serde_json::to_vec(&restored).unwrap()
    );
}

#[test]
fn overrun_snapshots_only_current_creatures_and_expires_at_cleanup() {
    let mut state = ready();
    let elf = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let enemy = put(
        &mut state,
        PlayerId::P1,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let forest = put(&mut state, PlayerId::P0, "Forest", Zone::Battlefield);
    let mut surface = surface();
    cast_overrun(&mut surface, &mut state);
    let later = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    assert_eq!(
        (
            engine::effective_power(&state, elf),
            engine::effective_toughness(&state, elf)
        ),
        (4, 4)
    );
    assert!(engine::has_effective_keyword(
        &state,
        elf,
        Keywords::TRAMPLE
    ));
    for object in [enemy, later, forest] {
        assert!(!engine::has_effective_keyword(
            &state,
            object,
            Keywords::TRAMPLE
        ));
    }
    assert_eq!(engine::effective_power(&state, later), 1);
    state.step = Step::End;
    state.engine.priority_passes = [false, false];
    apply(&mut surface, &mut state, Action::Pass);
    next(&mut surface, &mut state);
    apply(&mut surface, &mut state, Action::Pass);
    next(&mut surface, &mut state);
    assert!(state.engine.until_end_of_turn.is_empty());
    assert_eq!(engine::effective_power(&state, elf), 1);
    assert!(!engine::has_effective_keyword(
        &state,
        elf,
        Keywords::TRAMPLE
    ));
}

#[test]
fn overrun_does_not_boost_a_later_incarnation_of_the_same_card() {
    let mut state = ready();
    let elf = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    cast_overrun(&mut surface(), &mut state);
    move_to(&mut state, elf, Zone::Hand);
    move_to(&mut state, elf, Zone::Battlefield);
    assert_eq!(engine::effective_power(&state, elf), 1);
    assert!(!engine::has_effective_keyword(
        &state,
        elf,
        Keywords::TRAMPLE
    ));
}

#[test]
fn overrun_bonus_stays_on_an_affected_creature_after_control_changes() {
    let mut state = ready();
    let elf = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    cast_overrun(&mut surface(), &mut state);
    state.players[0].battlefield.retain(|id| *id != elf);
    state.players[1].battlefield.push(elf);
    state.objects.get_mut(elf).controller = PlayerId::P1;
    assert_eq!(engine::effective_power(&state, elf), 4);
    assert!(engine::has_effective_keyword(
        &state,
        elf,
        Keywords::TRAMPLE
    ));
}

#[test]
fn dwynen_lord_continuously_filters_creature_subtype_and_controller() {
    let mut state = ready();
    let dwynen = put(
        &mut state,
        PlayerId::P0,
        "Dwynen, Gilt-Leaf Daen",
        Zone::Battlefield,
    );
    let elf = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let enemy = put(
        &mut state,
        PlayerId::P1,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let hawk = put(&mut state, PlayerId::P0, "Healer's Hawk", Zone::Battlefield);
    assert_eq!(
        (
            engine::effective_power(&state, elf),
            engine::effective_toughness(&state, elf)
        ),
        (2, 2)
    );
    assert_eq!(
        (
            engine::effective_power(&state, dwynen),
            engine::effective_toughness(&state, dwynen)
        ),
        (3, 4)
    );
    assert_eq!(engine::effective_power(&state, enemy), 1);
    assert_eq!(engine::effective_power(&state, hawk), 1);
    move_to(&mut state, dwynen, Zone::Hand);
    assert_eq!(
        (
            engine::effective_power(&state, elf),
            engine::effective_toughness(&state, elf)
        ),
        (1, 1)
    );
}

#[test]
fn losing_dwynens_toughness_bonus_makes_marked_damage_lethal() {
    let mut state = ready();
    let dwynen = put(
        &mut state,
        PlayerId::P0,
        "Dwynen, Gilt-Leaf Daen",
        Zone::Battlefield,
    );
    let elf = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    state.objects.get_mut(elf).damage = 1;
    queue(&mut state);
    assert_eq!(state.objects.get(elf).zone, Zone::Battlefield);
    move_to(&mut state, dwynen, Zone::Graveyard);
    queue(&mut state);
    assert_eq!(state.objects.get(elf).zone, Zone::Graveyard);
}

#[test]
fn dwynen_attack_trigger_counts_remaining_attacking_elves_at_resolution() {
    let mut state = ready();
    let dwynen = put(
        &mut state,
        PlayerId::P0,
        "Dwynen, Gilt-Leaf Daen",
        Zone::Battlefield,
    );
    let elf = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let idle = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let hawk = put(&mut state, PlayerId::P0, "Healer's Hawk", Zone::Battlefield);
    let mut surface = surface();
    declare(&mut surface, &mut state, vec![dwynen, elf, hawk]);
    assert_eq!(state.engine.pending_triggers.len(), 1);
    move_to(&mut state, elf, Zone::Graveyard);
    queue(&mut state);
    drain(&mut surface, &mut state);
    assert_eq!(state.players[0].life, 21);
    assert!(!state.engine.combat.attackers.contains(&idle));
}

#[test]
fn dwynen_trigger_survives_its_source_leaving_and_counts_other_elves() {
    let mut state = ready();
    let dwynen = put(
        &mut state,
        PlayerId::P0,
        "Dwynen, Gilt-Leaf Daen",
        Zone::Battlefield,
    );
    let elf = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let mut surface = surface();
    declare(&mut surface, &mut state, vec![dwynen, elf]);
    move_to(&mut state, dwynen, Zone::Graveyard);
    queue(&mut state);
    drain(&mut surface, &mut state);
    assert_eq!(state.players[0].life, 21);
}

#[test]
fn no_dwynen_attack_trigger_when_only_another_creature_attacks() {
    let mut state = ready();
    put(
        &mut state,
        PlayerId::P0,
        "Dwynen, Gilt-Leaf Daen",
        Zone::Battlefield,
    );
    let elf = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    declare(&mut surface(), &mut state, vec![elf]);
    assert!(state.engine.pending_triggers.is_empty());
    assert_eq!(state.players[0].life, 20);
}

#[test]
fn pending_dwynen_attack_trigger_replays_identically_after_json_restore() {
    let mut state = ready();
    let dwynen = put(
        &mut state,
        PlayerId::P0,
        "Dwynen, Gilt-Leaf Daen",
        Zone::Battlefield,
    );
    let elf = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    declare(&mut surface(), &mut state, vec![dwynen, elf]);
    let mut restored: GameState =
        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    drain(&mut surface(), &mut state);
    drain(&mut surface(), &mut restored);
    assert_eq!(state.players[0].life, 22);
    assert_eq!(
        serde_json::to_vec(&state).unwrap(),
        serde_json::to_vec(&restored).unwrap()
    );
}

#[test]
fn ranger_temporary_power_expires_at_the_actual_cleanup_checkpoint() {
    let mut state = ready();
    let ranger = put(
        &mut state,
        PlayerId::P0,
        "Beast-Kin Ranger",
        Zone::Battlefield,
    );
    enter(&mut state, PlayerId::P0, "Healer's Hawk");
    let mut surface = surface();
    drain(&mut surface, &mut state);
    assert_eq!(engine::effective_power(&state, ranger), 4);
    state.step = Step::End;
    state.engine.priority_passes = [false, false];
    apply(&mut surface, &mut state, Action::Pass);
    next(&mut surface, &mut state);
    apply(&mut surface, &mut state, Action::Pass);
    next(&mut surface, &mut state);
    assert_eq!(engine::effective_power(&state, ranger), 3);
}

#[test]
fn returned_elf_is_removed_from_dwynens_resolution_time_attack_count() {
    let mut state = ready();
    let dwynen = put(
        &mut state,
        PlayerId::P0,
        "Dwynen, Gilt-Leaf Daen",
        Zone::Battlefield,
    );
    let elf = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let mut surface = surface();
    declare(&mut surface, &mut state, vec![dwynen, elf]);
    move_to(&mut state, elf, Zone::Hand);
    move_to(&mut state, elf, Zone::Battlefield);
    queue(&mut state);
    drain(&mut surface, &mut state);
    assert_eq!(state.players[0].life, 21);
    assert!(!state.engine.combat.attackers.contains(&elf));
}

#[test]
fn dwynen_continuous_bonus_follows_its_current_controller() {
    let mut state = ready();
    let dwynen = put(
        &mut state,
        PlayerId::P0,
        "Dwynen, Gilt-Leaf Daen",
        Zone::Battlefield,
    );
    let friendly = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let other = put(
        &mut state,
        PlayerId::P1,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    state.players[0].battlefield.retain(|id| *id != dwynen);
    state.players[1].battlefield.push(dwynen);
    state.objects.get_mut(dwynen).controller = PlayerId::P1;
    assert_eq!(engine::effective_power(&state, friendly), 1);
    assert_eq!(engine::effective_power(&state, other), 2);
}

#[test]
fn materialized_ranger_trigger_authenticates_its_printed_boost_amounts() {
    use mtg_kernel::effect::EffectOp;
    let mut state = ready();
    let ranger = put(
        &mut state,
        PlayerId::P0,
        "Beast-Kin Ranger",
        Zone::Battlefield,
    );
    enter(&mut state, PlayerId::P0, "Healer's Hawk");
    let mut effect = state.engine.pending_triggers[0].effect.clone();
    let card = state.objects.get(ranger).card_def;
    assert!(trigger::trigger_effect_matches(card, &effect));
    let EffectOp::BoostBoundObjectUntilEndOfTurn { power, .. } = &mut effect else {
        panic!("bound source boost");
    };
    *power = 50;
    assert!(!trigger::trigger_effect_matches(card, &effect));
}

#[test]
fn overrun_grants_real_trample_after_blocker_declaration() {
    let mut state = ready();
    let elf = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let blocker = put(
        &mut state,
        PlayerId::P1,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let mut surface = surface();
    cast_overrun(&mut surface, &mut state);
    declare(&mut surface, &mut state, vec![elf]);
    for _ in 0..80 {
        let decision = match surface.next_decision(&mut state) {
            SurfaceDecision::DeclareBlockersForAttacker {
                attacker,
                legal_blockers,
            } => {
                assert_eq!(attacker, elf);
                assert!(legal_blockers.contains(&blocker));
                surface
                    .apply(
                        &mut state,
                        SurfaceAction::DeclareBlockersForAttacker(vec![blocker]),
                    )
                    .unwrap();
                continue;
            }
            SurfaceDecision::Decision(decision) => decision,
        };
        match decision {
            Decision::DeclareBlockers { .. } => apply(
                &mut surface,
                &mut state,
                Action::DeclareBlockers(vec![(blocker, elf)]),
            ),
            Decision::ChooseCombatDamageRange { .. } => apply(
                &mut surface,
                &mut state,
                Action::ChooseCombatDamageRange { upper_half: false },
            ),
            Decision::CastSpellOrPass { .. }
                if state.step == Step::CombatDamage
                    && state.objects.get(blocker).zone == Zone::Graveyard =>
            {
                assert_eq!(state.players[1].life, 17);
                assert_eq!(state.objects.get(elf).damage, 1);
                return;
            }
            Decision::CastSpellOrPass { .. } => apply(&mut surface, &mut state, Action::Pass),
            other => panic!("unexpected combat decision: {other:?}"),
        }
    }
    panic!("combat did not deal damage");
}
