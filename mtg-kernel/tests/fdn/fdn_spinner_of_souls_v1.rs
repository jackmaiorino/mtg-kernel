//! Prepared genuine Partial-printing gameplay. Uncompiled and unrun.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use mtg_kernel::{
    card_def::{
        card_id_by_name, CardCapability, Keywords, OtherControlledNontokenDeathDefV1, Subtype,
        CARD_DEFS,
    },
    effect::{EffectFrame, EffectOp, PendingEffectChoice},
    engine::{self, Action, Decision},
    event::{self, CommittedEvent, ProposedEvent},
    ids::{ObjectId, PlayerId},
    mana::ManaColor,
    state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone},
};

fn ready(actor: PlayerId, v2: bool) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let name = |id: u16| CARD_DEFS[usize::from(id)].object_name.into();
    let mut state = if v2 {
        GameState::new_from_libraries_environment_v2_with_starting_player_v1(
            &[forest; 40],
            &[forest; 40],
            name,
            110_515,
            actor,
        )
    } else {
        GameState::new_from_libraries_with_starting_player_v1(
            &[forest; 40],
            &[forest; 40],
            name,
            110_515,
            actor,
        )
    };
    state.step = Step::Main1;
    state
}

fn put(state: &mut GameState, actor: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).expect("actual printing required");
    let id = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[usize::from(card_def)].object_name.into(),
        owner: actor,
        controller: actor,
        zone: Zone::Hand,
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
    state.players[actor.index()].hand.push(id);
    if zone != Zone::Hand {
        event::propose_and_commit(state, ProposedEvent::zone_change(id, zone));
    }
    id
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

fn step_priority(state: &mut GameState, decision: Decision) {
    match decision {
        Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
        Decision::OrderTriggers { pending, .. } => {
            engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
        }
        other => panic!("unexpected priority boundary {other:?}"),
    }
}

fn settle(state: &mut GameState) {
    for _ in 0..80 {
        let decision = next(state);
        if state.stack.is_empty()
            && state.engine.pending_triggers.is_empty()
            && matches!(decision, Decision::CastSpellOrPass { .. })
        {
            return;
        }
        step_priority(state, decision);
    }
    panic!("did not settle");
}

fn cast(
    state: &mut GameState,
    actor: PlayerId,
    name: &str,
    colored: ManaColor,
    colored_count: u8,
    generic: u8,
    target: Option<ObjectId>,
) -> ObjectId {
    let spell = put(state, actor, name, Zone::Hand);
    let mut found = false;
    for _ in 0..4 {
        let decision = next(state);
        if matches!(decision, Decision::CastSpellOrPass { player, .. } if player == actor) {
            found = true;
            break;
        }
        step_priority(state, decision);
    }
    assert!(found, "missing casting priority");
    state.players[actor.index()].mana_pool = [0; 6];
    state.players[actor.index()].mana_pool[colored.pool_index()] = colored_count;
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = generic;
    engine::step(state, Action::CastSpell(spell)).unwrap();
    if CARD_DEFS[usize::from(state.objects.get(spell).card_def)]
        .mode2
        .is_some()
    {
        assert!(matches!(
            next(state),
            Decision::ChooseSpellMode { mode_count: 2, .. }
        ));
        engine::step(state, Action::ChooseSpellMode(1)).unwrap();
    }
    if let Some(target) = target {
        assert!(
            matches!(next(state), Decision::ChooseTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(target)))
        );
        engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    }
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    assert_eq!(
        state.players[actor.index()].mana_pool,
        [0; 6],
        "the real spell paid its cost"
    );
    spell
}

fn source(state: &mut GameState, actor: PlayerId) -> ObjectId {
    let id = cast(state, actor, "Spinner of Souls", ManaColor::G, 1, 2, None);
    settle(state);
    assert_eq!(state.objects.get(id).zone, Zone::Battlefield);
    assert!(state.objects.get(id).summoning_sick);
    assert_eq!(
        (
            engine::effective_power(state, id),
            engine::effective_toughness(state, id)
        ),
        (4, 3)
    );
    assert!(engine::has_effective_keyword(state, id, Keywords::REACH));
    id
}

fn victim(state: &mut GameState, actor: PlayerId) -> ObjectId {
    let id = cast(state, actor, "Llanowar Elves", ManaColor::G, 1, 0, None);
    settle(state);
    id
}

fn library(state: &mut GameState, actor: PlayerId, names: &[&str]) -> Vec<ObjectId> {
    // Arrange physical library input before the paid death, never a reveal
    // result, trigger, effect alias, generated definition or RNG outcome.
    for id in state.players[actor.index()].library.clone() {
        event::propose_and_commit(state, ProposedEvent::zone_change(id, Zone::Exile));
    }
    names
        .iter()
        .map(|name| put(state, actor, name, Zone::Library))
        .collect()
}

fn await_choice(state: &mut GameState, actor: PlayerId, spinner: ObjectId) {
    for _ in 0..80 {
        let decision = next(state);
        if let Decision::ChooseEffectOption {
            player,
            source,
            option_count,
        } = decision
        {
            assert_eq!((player, source, option_count), (actor, spinner, 2));
            return;
        }
        step_priority(state, decision);
    }
    panic!("no genuine optional death reveal");
}

fn restore(state: &GameState) -> GameState {
    let bytes = serde_json::to_vec(state).unwrap();
    let restored = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(serde_json::to_vec(&restored).unwrap(), bytes);
    restored
}

fn rng(state: &GameState) -> (serde_json::Value, serde_json::Value) {
    let value = serde_json::to_value(state).unwrap();
    assert_ne!(
        value.get("rng").is_some(),
        value.get("environment_randomization_v2").is_some()
    );
    (
        value["rng"].clone(),
        value["environment_randomization_v2"].clone(),
    )
}

fn knowledge(state: &GameState, actor: PlayerId) -> serde_json::Value {
    serde_json::to_value([
        state.known_library_cards(PlayerId::P0, actor),
        state.known_library_cards(PlayerId::P1, actor),
    ])
    .unwrap()
}

fn bolt(state: &mut GameState, actor: PlayerId, target: ObjectId) {
    cast(
        state,
        actor,
        "Lightning Bolt",
        ManaColor::R,
        1,
        0,
        Some(target),
    );
}

#[test]
fn genuine_partial_printing_and_typed_recipe() {
    let def = &CARD_DEFS[usize::from(card_id_by_name("Spinner of Souls").unwrap())];
    assert_eq!(def.capability, CardCapability::Partial);
    assert_eq!(def.cost.generic, 2);
    assert_eq!((def.power, def.toughness), (Some(4), Some(3)));
    assert_eq!(def.subtypes, &[Subtype::Spider, Subtype::Spirit]);
    assert!(def.keywords.has(Keywords::REACH));
    assert_eq!(
        def.other_controlled_nontoken_death_v1,
        Some(OtherControlledNontokenDeathDefV1::MayRevealCreatureToHandRandomBottom)
    );
}

#[test]
fn paid_death_accepts_top_deeper_empty_and_no_creature_with_restore_both_seats_and_rng_modes() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for v2 in [false, true] {
            for names in [
                vec![],
                vec!["Llanowar Elves"],
                vec!["Forest", "Llanowar Elves"],
                vec!["Forest", "Mountain", "Llanowar Elves", "Island"],
                vec!["Forest", "Mountain"],
            ] {
                let mut state = ready(actor, v2);
                let spinner = source(&mut state, actor);
                let dead = victim(&mut state, actor);
                let original = library(&mut state, actor, &names);
                for observer in [PlayerId::P0, PlayerId::P1] {
                    state.reveal_library_top(observer, actor, original.len());
                }
                let creature = names.iter().position(|name| *name == "Llanowar Elves");
                let prefix_len = creature.map_or(original.len(), |index| index + 1);
                let rest_len = prefix_len - usize::from(creature.is_some());
                bolt(&mut state, actor.opponent(), dead);
                await_choice(&mut state, actor, spinner);
                assert_eq!(state.objects.get(dead).zone, Zone::Graveyard);
                let before_rng = rng(&state);
                let before_hand_len = state.players[actor.index()].hand.len();
                let mut restored = restore(&state);
                for branch in [&mut state, &mut restored] {
                    engine::step(branch, Action::ChooseEffectOption(1)).unwrap();
                    *branch = restore(branch); // answered, before the operation executes
                    settle(branch);
                    assert_eq!(
                        branch.players[actor.index()].hand.len(),
                        before_hand_len + usize::from(creature.is_some())
                    );
                    if let Some(index) = creature {
                        let selected = original[index];
                        assert_eq!(branch.objects.get(selected).zone, Zone::Hand);
                        assert!(branch
                            .known_hand_cards(actor.opponent(), actor)
                            .iter()
                            .any(|entry| entry.object == selected));
                    }
                    let live = &branch.players[actor.index()].library;
                    assert_eq!(
                        &live[..original.len() - prefix_len],
                        &original[prefix_len..]
                    );
                    let mut expected_rest = original[..rest_len].to_vec();
                    let mut actual_rest = live[live.len() - rest_len..].to_vec();
                    expected_rest.sort();
                    actual_rest.sort();
                    assert_eq!(actual_rest, expected_rest);
                    if rest_len < 2 {
                        assert_eq!(rng(branch), before_rng);
                    } else {
                        assert_ne!(rng(branch), before_rng);
                        for observer in [PlayerId::P0, PlayerId::P1] {
                            assert!(branch
                                .known_library_cards(observer, actor)
                                .iter()
                                .all(|entry| (entry.position as usize) < live.len() - rest_len));
                        }
                    }
                }
                assert_eq!(
                    serde_json::to_vec(&state).unwrap(),
                    serde_json::to_vec(&restored).unwrap()
                );
            }
        }
    }
}

#[test]
fn decline_preserves_library_knowledge_and_rng_before_and_after_answer_restore() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, true);
        let spinner = source(&mut state, actor);
        let dead = victim(&mut state, actor);
        library(&mut state, actor, &["Forest", "Mountain", "Llanowar Elves"]);
        bolt(&mut state, actor.opponent(), dead);
        await_choice(&mut state, actor, spinner);
        let before_library = state.players[actor.index()].library.clone();
        let before_rng = rng(&state);
        let before_knowledge = knowledge(&state, actor);
        state = restore(&state);
        engine::step(&mut state, Action::ChooseEffectOption(0)).unwrap();
        state = restore(&state);
        settle(&mut state);
        assert_eq!(state.players[actor.index()].library, before_library);
        assert_eq!(rng(&state), before_rng);
        assert_eq!(knowledge(&state, actor), before_knowledge);
    }
}

#[test]
fn paid_opponent_token_and_spinner_deaths_do_not_offer_a_reveal() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for which in 0..3 {
            let mut state = ready(actor, false);
            let spinner = source(&mut state, actor);
            let dead = match which {
                0 => put(
                    &mut state,
                    actor.opponent(),
                    "Llanowar Elves",
                    Zone::Battlefield,
                ),
                1 => {
                    cast(
                        &mut state,
                        actor,
                        "Goblin Surprise",
                        ManaColor::R,
                        1,
                        2,
                        None,
                    );
                    settle(&mut state);
                    state.players[actor.index()]
                        .battlefield
                        .iter()
                        .copied()
                        .find(|id| state.objects.get(*id).v4.is_token)
                        .unwrap()
                }
                _ => spinner,
            };
            bolt(&mut state, actor.opponent(), dead);
            settle(&mut state); // Any reveal would fail this helper.
            assert_ne!(state.objects.get(dead).zone, Zone::Battlefield);
        }
    }
}

#[test]
fn paid_simultaneous_wipe_keeps_departed_spinner_and_exactly_two_other_controlled_deaths() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, false);
        let spinner = source(&mut state, actor);
        victim(&mut state, actor);
        victim(&mut state, actor);
        put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        cast(
            &mut state,
            actor,
            "Goblin Surprise",
            ManaColor::R,
            1,
            2,
            None,
        );
        settle(&mut state);
        let start = state.engine.event_history.len();
        cast(
            &mut state,
            actor,
            "Day of Judgment",
            ManaColor::W,
            2,
            2,
            None,
        );
        for _ in 0..2 {
            await_choice(&mut state, actor, spinner);
            assert_eq!(state.objects.get(spinner).zone, Zone::Graveyard);
            state = restore(&state);
            engine::step(&mut state, Action::ChooseEffectOption(0)).unwrap();
        }
        settle(&mut state);
        assert_eq!(state.engine.event_history[start..].iter().filter(|event| matches!(event,
            CommittedEvent::ControlledCreatureDeathCaptureV1 { source, .. } if source.source == spinner)).count(), 2);
    }
}

#[test]
fn tampered_optional_and_answered_payloads_refuse_without_mutation() {
    let actor = PlayerId::P0;
    let mut state = ready(actor, false);
    let spinner = source(&mut state, actor);
    let dead = victim(&mut state, actor);
    bolt(&mut state, actor.opponent(), dead);
    await_choice(&mut state, actor, spinner);
    let mut forged = restore(&state);
    let Some(PendingEffectChoice::ChooseOption { options, .. }) = forged
        .engine
        .pending_effect
        .as_mut()
        .unwrap()
        .choice
        .as_mut()
    else {
        panic!("choice");
    };
    options[1] = EffectOp::Sequence(vec![]);
    let bytes = serde_json::to_vec(&forged).unwrap();
    assert!(engine::step(&mut forged, Action::ChooseEffectOption(1)).is_err());
    assert_eq!(serde_json::to_vec(&forged).unwrap(), bytes);
    engine::step(&mut state, Action::ChooseEffectOption(1)).unwrap();
    let EffectFrame::Program { op, .. } = state
        .engine
        .pending_effect
        .as_mut()
        .unwrap()
        .frames
        .last_mut()
        .unwrap()
    else {
        panic!("answer");
    };
    *op = EffectOp::Sequence(vec![]);
    assert!(mtg_kernel::effect::validate_pending_effect_choice(&state).is_err());
}
