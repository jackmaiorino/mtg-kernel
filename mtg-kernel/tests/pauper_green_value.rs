//! Exact Mage parity for the coherent green/value future tranche.
//!
//! Gingerbread Cabin and Writhing Chrysalis are fully executable here.
//! The latter includes its two Eldrazi Spawn cast trigger and its
//! incarnation-bound sacrifice trigger.

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardType, Keywords, Subtype, CARD_DEFS,
};
use mtg_kernel::effect::{EffectOp, PlayerRef};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, CommittedEvent, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{Cost, ManaColor, Pip};
use mtg_kernel::rl::{legal_action_candidates_v1, ActionSemanticV1};
use mtg_kernel::state::{Counters, GameObject, GameState, ObjectStateV4, Step, Zone};
use mtg_kernel::surface_v2::SurfaceDecision;
use mtg_kernel::trigger;

fn card_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

fn put_object(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id(name);
    let object = state.objects.push(GameObject {
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
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[player.index()].hand.push(object),
        Zone::Battlefield => state.players[player.index()].battlefield.push(object),
        Zone::Library => state.players[player.index()].library.push(object),
        Zone::Graveyard => state.players[player.index()].graveyard.push(object),
        Zone::Exile => state.exile.push(object),
        Zone::Command => state.command.push(object),
        Zone::Stack => panic!("test helper does not construct stack objects"),
    }
    object
}

fn ready_main(seed: u64) -> GameState {
    let mut state = GameState::new_from_libraries(&[], &[], |_| String::new(), seed);
    state.step = Step::Main1;
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    state
}

fn pass_until<F>(state: &mut GameState, mut done: F) -> Decision
where
    F: FnMut(&GameState, &Decision) -> bool,
{
    for _ in 0..64 {
        let decision = engine::advance_until_decision(state);
        if done(state, &decision) {
            return decision;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!(
                "unexpected decision during bounded resolution: {other:?}; stack={:?}; objects={:?}",
                state.stack,
                state
                    .objects
                    .iter()
                    .map(|(id, object)| (id, object.name.as_str(), object.zone))
                    .collect::<Vec<_>>()
            ),
        }
    }
    panic!("bounded resolution did not reach the expected state");
}

fn live_battlefield_count(state: &GameState, player: PlayerId, name: &str) -> usize {
    let wanted = card_id(name);
    state.players[player.index()]
        .battlefield
        .iter()
        .filter(|&&object| state.objects.get(object).card_def == wanted)
        .count()
}

#[test]
fn generated_definitions_match_checked_in_mage_text() {
    assert_eq!(card_id("Gingerbread Cabin"), 43);
    assert_eq!(card_id("Writhing Chrysalis"), 131);
    assert_eq!(card_id("Eldrazi Spawn Token"), 154);
    assert_eq!(CARD_DEFS.len(), 155);
    assert_eq!(Subtype::Spawn.stable_id(), 64);

    let cabin = &CARD_DEFS[card_id("Gingerbread Cabin") as usize];
    assert_eq!(cabin.capability, CardCapability::Full);
    assert_eq!(cabin.types, &[CardType::Land]);
    assert_eq!(cabin.subtypes, &[Subtype::Forest]);
    assert_eq!(cabin.produces_mana, &[ManaColor::G]);
    assert!(!cabin.enters_battlefield_tapped);
    let entry = cabin
        .enters_battlefield_tapped_unless
        .expect("Cabin has its three-other-Forests condition");
    assert_eq!(entry.controller_controls_other_subtype, Subtype::Forest);
    assert_eq!(entry.minimum_count, 3);

    let chrysalis = &CARD_DEFS[card_id("Writhing Chrysalis") as usize];
    assert_eq!(chrysalis.capability, CardCapability::Full);
    assert_eq!(chrysalis.types, &[CardType::Creature]);
    assert_eq!(chrysalis.subtypes, &[Subtype::Eldrazi, Subtype::Drone]);
    assert_eq!((chrysalis.power, chrysalis.toughness), (Some(2), Some(3)));
    assert_eq!(
        chrysalis.cost,
        Cost {
            pips: &[Pip::Colored(ManaColor::R), Pip::Colored(ManaColor::G)],
            generic: 2,
            x_count: 0,
        }
    );
    assert!(
        chrysalis.colors.is_empty(),
        "Devoid makes Chrysalis colorless"
    );
    assert!(chrysalis.keywords.has(Keywords::REACH));

    let spawn = &CARD_DEFS[card_id("Eldrazi Spawn Token") as usize];
    assert_eq!(spawn.capability, CardCapability::Full);
    assert!(spawn.is_token);
    assert_eq!(spawn.types, &[CardType::Creature]);
    assert_eq!(spawn.subtypes, &[Subtype::Eldrazi, Subtype::Spawn]);
    assert_eq!((spawn.power, spawn.toughness), (Some(0), Some(1)));
    assert_eq!(spawn.produces_mana, &[ManaColor::C]);

    let triggers = trigger::triggers_for(card_id("Writhing Chrysalis"));
    assert_eq!(triggers.len(), 2);
    assert_eq!(
        (triggers[0].effect)(),
        EffectOp::Sequence(vec![
            EffectOp::CreateToken {
                token_def: card_id("Eldrazi Spawn Token"),
                controller: PlayerRef::Controller,
            },
            EffectOp::CreateToken {
                token_def: card_id("Eldrazi Spawn Token"),
                controller: PlayerRef::Controller,
            },
        ])
    );
    assert_eq!(
        (triggers[1].effect)(),
        EffectOp::BindPlusOnePlusOneCounterToTriggerSource
    );
}

#[test]
fn cabin_counts_other_forests_and_rechecks_the_condition_at_resolution() {
    let mut short = ready_main(0x4341_4249_4e00_0001);
    for _ in 0..2 {
        put_object(&mut short, PlayerId::P0, "Forest", Zone::Battlefield);
    }
    let short_cabin = put_object(&mut short, PlayerId::P0, "Gingerbread Cabin", Zone::Hand);
    engine::step(&mut short, Action::PlayLand(short_cabin)).unwrap();
    assert!(short.objects.get(short_cabin).tapped);
    assert!(short.engine.pending_triggers.is_empty());
    assert_eq!(
        live_battlefield_count(&short, PlayerId::P0, "Food Token"),
        0
    );

    let mut lost_condition = ready_main(0x4341_4249_4e00_0003);
    let mut forests = Vec::new();
    for _ in 0..3 {
        forests.push(put_object(
            &mut lost_condition,
            PlayerId::P0,
            "Forest",
            Zone::Battlefield,
        ));
    }
    let conditional_cabin = put_object(
        &mut lost_condition,
        PlayerId::P0,
        "Gingerbread Cabin",
        Zone::Hand,
    );
    engine::step(&mut lost_condition, Action::PlayLand(conditional_cabin)).unwrap();
    assert_eq!(lost_condition.engine.pending_triggers.len(), 1);
    assert!(matches!(
        engine::advance_until_decision(&mut lost_condition),
        Decision::CastSpellOrPass { .. }
    ));
    event::propose_and_commit(
        &mut lost_condition,
        ProposedEvent::zone_change(forests[0], Zone::Graveyard),
    );
    pass_until(&mut lost_condition, |state, _| state.stack.is_empty());
    assert_eq!(
        live_battlefield_count(&lost_condition, PlayerId::P0, "Food Token"),
        0,
        "the intervening-if condition is false at resolution"
    );

    let mut exact = ready_main(0x4341_4249_4e00_0002);
    for _ in 0..3 {
        put_object(&mut exact, PlayerId::P0, "Forest", Zone::Battlefield);
    }
    let cabin = put_object(&mut exact, PlayerId::P0, "Gingerbread Cabin", Zone::Hand);
    engine::step(&mut exact, Action::PlayLand(cabin)).unwrap();
    assert!(!exact.objects.get(cabin).tapped);
    assert_eq!(exact.engine.pending_triggers.len(), 1);
    let decision = pass_until(&mut exact, |state, _| {
        state.stack.is_empty() && live_battlefield_count(state, PlayerId::P0, "Food Token") == 1
    });

    let candidates = legal_action_candidates_v1(&SurfaceDecision::Decision(decision), &exact)
        .expect("Cabin mana action projects to the RL surface");
    assert!(candidates.iter().any(|candidate| matches!(
        &candidate.record.semantic,
        ActionSemanticV1::ActivateManaAbility { source, mana_choice: Some(ManaColor::G), .. }
            if source.arena_id == cabin.0
    )));

    engine::step(&mut exact, Action::ActivateManaAbility(cabin)).unwrap();
    assert!(exact.objects.get(cabin).tapped);
    assert_eq!(exact.players[0].mana_pool[ManaColor::G.pool_index()], 1);

    let restored: GameState =
        serde_json::from_str(&serde_json::to_string(&exact).unwrap()).unwrap();
    assert_eq!(restored, exact);
}

#[test]
fn chrysalis_cast_trigger_precedes_the_spell_and_creates_two_spawn() {
    let mut state = ready_main(0x4348_5259_5300_0001);
    state.players[0].mana_pool[ManaColor::R.pool_index()] = 1;
    state.players[0].mana_pool[ManaColor::G.pool_index()] = 3;
    let chrysalis = put_object(&mut state, PlayerId::P0, "Writhing Chrysalis", Zone::Hand);

    engine::step(&mut state, Action::CastSpell(chrysalis)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(state.stack.len(), 2);
    assert_eq!(
        state.stack[1].v4.source_contract, state.stack[0].v4.source_contract,
        "the cast trigger authenticates its exact producing spell"
    );
    let restored: GameState =
        serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    assert_eq!(restored, state);

    let mut tampered = state.clone();
    tampered.stack[1].v4.source_contract = None;
    engine::step(&mut tampered, Action::Pass).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut tampered),
        Decision::CastSpellOrPass { .. }
    ));
    engine::step(&mut tampered, Action::Pass).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut tampered),
        Decision::Halted { .. }
    ));

    pass_until(&mut state, |state, _| {
        state.objects.get(chrysalis).zone == Zone::Stack
            && live_battlefield_count(state, PlayerId::P0, "Eldrazi Spawn Token") == 2
    });
    assert_eq!(state.objects.get(chrysalis).zone, Zone::Stack);
    assert_eq!(state.stack.len(), 1, "only the Chrysalis spell remains");

    pass_until(&mut state, |state, _| {
        state.stack.is_empty() && state.objects.get(chrysalis).zone == Zone::Battlefield
    });
    assert_eq!(state.objects.get(chrysalis).counters.plus1_plus1, 0);
    assert_eq!(engine::effective_power(&state, chrysalis), 2);
    assert_eq!(engine::effective_toughness(&state, chrysalis), 3);
}

#[test]
fn spawn_mana_sacrifice_uses_lki_projects_to_rl_and_grows_chrysalis() {
    let mut state = ready_main(0x5350_4157_4e00_0001);
    let chrysalis = put_object(
        &mut state,
        PlayerId::P0,
        "Writhing Chrysalis",
        Zone::Battlefield,
    );
    let spawn = put_object(
        &mut state,
        PlayerId::P0,
        "Eldrazi Spawn Token",
        Zone::Battlefield,
    );

    let decision = engine::advance_until_decision(&mut state);
    let candidates = legal_action_candidates_v1(&SurfaceDecision::Decision(decision), &state)
        .expect("Spawn mana action projects to the RL surface");
    assert!(candidates.iter().any(|candidate| matches!(
        &candidate.record.semantic,
        ActionSemanticV1::ActivateManaAbility { source, mana_choice: Some(ManaColor::C), .. }
            if source.arena_id == spawn.0
    )));

    engine::step(&mut state, Action::ActivateManaAbility(spawn)).unwrap();
    assert_eq!(state.players[0].mana_pool[ManaColor::C.pool_index()], 1);
    assert_eq!(state.objects.get(spawn).zone, Zone::Graveyard);

    let sacrificed = state
        .engine
        .event_history
        .iter()
        .find(
            |event| matches!(event, CommittedEvent::Sacrificed { object, .. } if *object == spawn),
        )
        .expect("sacrifice event records last-known information")
        .clone();
    let CommittedEvent::Sacrificed {
        controller_before,
        effective_subtype_ids_before,
        ..
    } = &sacrificed
    else {
        unreachable!()
    };
    assert_eq!(*controller_before, PlayerId::P0);
    assert!(effective_subtype_ids_before.contains(&Subtype::Eldrazi.stable_id()));
    assert!(effective_subtype_ids_before.contains(&Subtype::Spawn.stable_id()));
    let round_trip: CommittedEvent =
        serde_json::from_str(&serde_json::to_string(&sacrificed).unwrap()).unwrap();
    assert_eq!(round_trip, sacrificed);

    pass_until(&mut state, |state, _| {
        state.stack.is_empty() && state.objects.get(chrysalis).counters.plus1_plus1 == 1
    });
    assert_eq!(engine::effective_power(&state, chrysalis), 3);
    assert_eq!(engine::effective_toughness(&state, chrysalis), 4);
    assert!(!state.players[0].graveyard.contains(&spawn));
}

#[test]
fn chrysalis_trigger_rejects_wrong_controller_and_stale_source_incarnation() {
    let mut wrong_controller = ready_main(0x5350_4157_4e00_0002);
    put_object(
        &mut wrong_controller,
        PlayerId::P0,
        "Writhing Chrysalis",
        Zone::Battlefield,
    );
    let opposing_spawn = put_object(
        &mut wrong_controller,
        PlayerId::P1,
        "Eldrazi Spawn Token",
        Zone::Battlefield,
    );
    event::log_sacrifice(&mut wrong_controller, opposing_spawn);
    event::propose_and_commit(
        &mut wrong_controller,
        ProposedEvent::zone_change(opposing_spawn, Zone::Graveyard),
    );
    assert!(trigger::collect_and_process(&mut wrong_controller).is_empty());

    let mut stale = ready_main(0x5350_4157_4e00_0003);
    let chrysalis = put_object(
        &mut stale,
        PlayerId::P0,
        "Writhing Chrysalis",
        Zone::Battlefield,
    );
    let spawn = put_object(
        &mut stale,
        PlayerId::P0,
        "Eldrazi Spawn Token",
        Zone::Battlefield,
    );
    engine::step(&mut stale, Action::ActivateManaAbility(spawn)).unwrap();
    let decision = engine::advance_until_decision(&mut stale);
    assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
    assert_eq!(stale.stack.len(), 1, "the bound growth trigger is on stack");

    event::propose_and_commit(
        &mut stale,
        ProposedEvent::zone_change(chrysalis, Zone::Exile),
    );
    pass_until(&mut stale, |state, _| state.stack.is_empty());
    assert_eq!(stale.objects.get(chrysalis).zone, Zone::Exile);
    assert_eq!(stale.objects.get(chrysalis).counters.plus1_plus1, 0);
}
