//! Future real printed acceptance. Requires root-owned FDN registration.
//! No fixture definition or root validation bypass is provided.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use mtg_kernel::card_def::{card_id_by_name, CardCapability, Subtype, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameState, Step, Zone};

#[test]
fn actual_selected_step_and_trigger_meanings_reject_each_changed_fact() {
    use mtg_kernel::rules_meaning_cerberus_v1::{self as selected, RootV1};
    let id = card_id_by_name("Slumbering Cerberus").expect("requires actual printed admission");
    let selector = selected::live_selector().unwrap();
    for root in [
        RootV1::ControllerUntapStepRestriction,
        RootV1::PrintedEndStepTrigger { trigger_index: 0 },
    ] {
        let record = selected::meaning(&selector, id, root).unwrap();
        selected::validate(&record).unwrap();
        let value = serde_json::to_value(&record).unwrap();
        let roundtrip = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(record, roundtrip);
        let mut wrong_hash = record.clone();
        wrong_hash.root_sha256.push('x');
        assert!(selected::validate(&wrong_hash).is_err());
        let variant = match root {
            RootV1::ControllerUntapStepRestriction => "ControllerUntapStepRestriction",
            _ => "PrintedEndStepTrigger",
        };
        let fields: Vec<_> = value["rule"][variant]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        assert_eq!(
            fields.len(),
            if matches!(root, RootV1::ControllerUntapStepRestriction) {
                5
            } else {
                13
            }
        );
        for field in fields {
            let mut forged = value.clone();
            let old = forged["rule"][variant][&field].as_bool().unwrap();
            forged["rule"][variant][&field] = serde_json::Value::Bool(!old);
            let forged = serde_json::from_value(forged).unwrap();
            assert!(
                selected::validate(&forged).is_err(),
                "accepted forged {field}"
            );
        }
        let mut invented_admission = record.clone();
        invented_admission.public_human_sampler_and_model_admitted = true;
        assert!(selected::validate(&invented_admission).is_err());
        let mut unknown = value.clone();
        unknown["rule"][variant]["extra"] = serde_json::Value::Bool(true);
        assert!(serde_json::from_value::<selected::MeaningV1>(unknown).is_err());
        let mut unknown = value;
        unknown["extra"] = serde_json::Value::Bool(true);
        assert!(serde_json::from_value::<selected::MeaningV1>(unknown).is_err());
    }
    assert_eq!(
        selected::meaning(
            &selector,
            id,
            RootV1::PrintedEndStepTrigger { trigger_index: 1 }
        ),
        Err(selected::ErrorV1::WrongRoot)
    );
    let mut forged =
        serde_json::to_value(RootV1::PrintedEndStepTrigger { trigger_index: 0 }).unwrap();
    forged["PrintedEndStepTrigger"]["extra"] = serde_json::Value::Bool(true);
    assert!(serde_json::from_value::<RootV1>(forged).is_err());
}

fn ready(actor: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |id| CARD_DEFS[id as usize].object_name.into(),
        110_094,
        actor,
    );
    state.step = Step::Main1;
    state
}
fn add(state: &mut GameState, owner: PlayerId, name: &str, zone: Zone) -> ObjectId {
    // Physically introduce an actual registered definition from its owner's library.
    // This is a fixture library recipe; gameplay casting below owns spell roots.
    let def = card_id_by_name(name).expect("requires actual printed FDN admission");
    let id = state.objects.push(mtg_kernel::state::GameObject {
        card_def: def,
        name: CARD_DEFS[def as usize].object_name.into(),
        owner,
        controller: owner,
        zone: Zone::Library,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: Vec::new(),
        v4: mtg_kernel::state::ObjectStateV4::from_card_def(def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    state.players[owner.index()].library.push(id);
    event::propose_and_commit(state, ProposedEvent::zone_change(id, zone));
    id
}
fn restore(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}
fn next(state: &mut GameState) -> Decision {
    let d = engine::advance_until_decision(state);
    assert!(!matches!(d, Decision::Halted { .. }), "{d:?}");
    d
}
fn act(state: &mut GameState, d: Decision) {
    let a = match d {
        Decision::CastSpellOrPass { .. } => Action::Pass,
        Decision::OrderTriggers { pending, .. } => {
            Action::OrderTriggers((0..pending.len()).collect())
        }
        Decision::DeclareAttackers { .. } => Action::DeclareAttackers(vec![]),
        Decision::DeclareBlockers { .. } => Action::DeclareBlockers(vec![]),
        other => panic!("unexpected decision {other:?}"),
    };
    engine::step(state, a).unwrap();
}
fn settle(state: &mut GameState) {
    for _ in 0..100 {
        let d = next(state);
        if matches!(d, Decision::CastSpellOrPass { .. })
            && state.stack.is_empty()
            && state.engine.pending_triggers.is_empty()
        {
            return;
        }
        act(state, d);
    }
    panic!("stack did not settle");
}
fn cast(state: &mut GameState, actor: PlayerId, name: &str) -> ObjectId {
    let id = add(state, actor, name, Zone::Hand);
    state.players[actor.index()].mana_pool = [10; 6];
    assert!(
        matches!(next(state),Decision::CastSpellOrPass {castable_spells,..} if castable_spells.contains(&id))
    );
    engine::step(state, Action::CastSpell(id)).unwrap();
    settle(state);
    assert_eq!(state.objects.get(id).zone, Zone::Battlefield);
    id
}
fn end(state: &mut GameState) {
    state.step = Step::Main2;
    for _ in 0..100 {
        let d = next(state);
        if state.step == Step::End {
            return;
        }
        act(state, d);
    }
    panic!("end not reached");
}
fn reach(state: &mut GameState, active: PlayerId, step: Step) {
    for _ in 0..250 {
        let d = next(state);
        if state.active_player == active && state.step == step {
            return;
        }
        act(state, d);
    }
    panic!("real step not reached");
}
fn die(state: &mut GameState, owner: PlayerId) {
    let id = add(state, owner, "Llanowar Elves", Zone::Battlefield);
    event::propose_and_commit(state, ProposedEvent::zone_change(id, Zone::Graveyard));
}
fn source_trigger_count(state: &GameState, source: ObjectId) -> usize {
    state
        .stack
        .iter()
        .filter(|item| item.source == source)
        .count()
        + state
            .engine
            .pending_triggers
            .iter()
            .filter(|item| item.source == source)
            .count()
}

#[test]
fn exact_printed_identity_and_cast_cost_both_seats() {
    let def =
        &CARD_DEFS[card_id_by_name("Slumbering Cerberus").expect("future registration") as usize];
    assert_eq!(def.capability, CardCapability::Full);
    assert_eq!(
        (def.power, def.toughness, def.mana_value),
        (Some(4), Some(2), 2)
    );
    assert_eq!(def.subtypes, &[Subtype::Dog]);
    assert_eq!(def.cost.generic, 1);
    assert_eq!(def.colors, &[ManaColor::R]);
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = add(&mut state, actor, "Slumbering Cerberus", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
        next(&mut state);
        let before = serde_json::to_vec(&state).unwrap();
        assert!(engine::step(&mut state, Action::CastSpell(source)).is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 2;
        engine::step(&mut state, Action::CastSpell(source)).unwrap();
        settle(&mut state);
        assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
    }
}
#[test]
fn every_players_end_step_uses_either_owners_actual_creature_death_and_restore() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for active in [actor, actor.opponent()] {
            for dead_owner in [actor, actor.opponent()] {
                let mut state = ready(actor);
                let source = cast(&mut state, actor, "Slumbering Cerberus");
                if state.active_player != active {
                    reach(&mut state, active, Step::Main1);
                }
                state.objects.get_mut(source).tapped = true;
                state.priority_player = active;
                die(&mut state, dead_owner);
                end(&mut state);
                assert_eq!(source_trigger_count(&state, source), 1);
                let mut replay = restore(&state);
                settle(&mut state);
                settle(&mut replay);
                assert!(!state.objects.get(source).tapped);
                assert_eq!(
                    serde_json::to_vec(&state).unwrap(),
                    serde_json::to_vec(&replay).unwrap()
                );
            }
        }
    }
}
#[test]
fn no_death_and_late_death_do_not_create_retrospective_trigger() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = cast(&mut state, actor, "Slumbering Cerberus");
        state.objects.get_mut(source).tapped = true;
        end(&mut state);
        assert_eq!(source_trigger_count(&state, source), 0);
        die(&mut state, actor.opponent());
        settle(&mut state);
        assert_eq!(source_trigger_count(&state, source), 0);
        assert!(state.objects.get(source).tapped);
    }
}
#[test]
fn actual_untap_step_preserves_stun_then_end_trigger_attempt_consumes_it() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = cast(&mut state, actor, "Slumbering Cerberus");
        state.objects.get_mut(source).tapped = true;
        state.objects.get_mut(source).counters.stun = 1;
        reach(&mut state, actor, Step::Upkeep);
        assert!(state.objects.get(source).tapped);
        assert_eq!(state.objects.get(source).counters.stun, 1);
        die(&mut state, actor);
        end(&mut state);
        let mut replay = restore(&state);
        settle(&mut state);
        settle(&mut replay);
        assert!(state.objects.get(source).tapped);
        assert_eq!(state.objects.get(source).counters.stun, 0);
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}
#[test]
fn captured_source_departure_reentry_and_current_control_are_exact() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for mode in 0..3 {
            let mut state = ready(actor);
            let source = cast(&mut state, actor, "Slumbering Cerberus");
            state.objects.get_mut(source).tapped = true;
            die(&mut state, actor.opponent());
            end(&mut state);
            assert_eq!(source_trigger_count(&state, source), 1);
            match mode {
                0 => state.objects.get_mut(source).controller = actor.opponent(),
                1 | 2 => {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(source, Zone::Graveyard),
                    );
                    if mode == 2 {
                        event::propose_and_commit(
                            &mut state,
                            ProposedEvent::zone_change(source, Zone::Battlefield),
                        );
                        state.objects.get_mut(source).tapped = true;
                    }
                }
                _ => unreachable!(),
            }
            let mut replay = restore(&state);
            settle(&mut state);
            settle(&mut replay);
            if mode == 0 {
                assert!(!state.objects.get(source).tapped);
            }
            if mode == 2 {
                assert!(state.objects.get(source).tapped);
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
}
#[test]
fn real_witness_suppresses_capture_and_step_restriction_but_not_captured_trigger() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for after_capture in [false, true] {
            let mut state = ready(actor);
            let source = cast(&mut state, actor, "Slumbering Cerberus");
            cast(&mut state, actor, "High Fae Trickster");
            state.objects.get_mut(source).tapped = true;
            die(&mut state, actor);
            if after_capture {
                end(&mut state);
                assert_eq!(source_trigger_count(&state, source), 1);
            }
            let witness = add(&mut state, actor, "Witness Protection", Zone::Hand);
            state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 1;
            next(&mut state);
            engine::step(&mut state, Action::CastSpell(witness)).unwrap();
            assert!(
                matches!(next(&mut state),Decision::ChooseTargets{legal_targets,..}
                if legal_targets.contains(&mtg_kernel::state::Target::Object(source)))
            );
            engine::step(
                &mut state,
                Action::ChooseTarget(mtg_kernel::state::Target::Object(source)),
            )
            .unwrap();
            next(&mut state); // Finalize the actual targeted announcement.
            for _ in 0..20 {
                if state.objects.get(witness).zone == Zone::Battlefield {
                    break;
                }
                let d = next(&mut state);
                act(&mut state, d);
            }
            assert_eq!(state.objects.get(witness).zone, Zone::Battlefield);
            assert!(engine::has_effective_subtype(
                &state,
                source,
                Subtype::Citizen
            ));
            let mut replay = restore(&state);
            for branch in [&mut state, &mut replay] {
                if after_capture {
                    settle(branch);
                    assert!(!branch.objects.get(source).tapped);
                } else {
                    end(branch);
                    assert_eq!(source_trigger_count(branch, source), 0);
                    assert!(branch.objects.get(source).tapped);
                }
                reach(branch, actor, Step::Upkeep);
                assert!(!branch.objects.get(source).tapped);
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
}

#[test]
fn resolution_rechecks_morbid_and_restored_source_contract_forgery_cannot_untap() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for case in 0..4 {
            let mut state = ready(actor);
            let source = cast(&mut state, actor, "Slumbering Cerberus");
            state.objects.get_mut(source).tapped = true;
            die(&mut state, actor);
            end(&mut state);
            assert_eq!(state.stack.len(), 1);
            if case == 0 {
                // A stale-turn primitive isolates the resolution-time condition.
                state.creature_death_turn_v1 = None;
                settle(&mut state);
                assert!(state.objects.get(source).tapped);
                continue;
            }
            let contract = state.stack[0].v4.ability_source_contract.as_mut().unwrap();
            match case {
                1 => contract.owner = actor.opponent(),
                2 => contract.controller = actor.opponent(),
                3 => contract.zone_change_count += 1,
                _ => unreachable!(),
            }
            state = restore(&state);
            let mut refused = false;
            for _ in 0..20 {
                match engine::advance_until_decision(&mut state) {
                    Decision::Halted { .. } => {
                        refused = true;
                        break;
                    }
                    Decision::CastSpellOrPass { .. } => {
                        let before = serde_json::to_vec(&state).unwrap();
                        if engine::step(&mut state, Action::Pass).is_err() {
                            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
                            refused = true;
                            break;
                        }
                    }
                    other => panic!("unexpected {other:?}"),
                }
            }
            assert!(refused);
            assert!(state.objects.get(source).tapped);
        }
    }
}
#[test]
fn own_hidden_hand_identity_is_not_exported_to_opponent_at_public_morbid_prompt() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = cast(&mut state, actor, "Slumbering Cerberus");
        state.objects.get_mut(source).tapped = true;
        die(&mut state, actor);
        end(&mut state);
        add(&mut state, actor, "Giant Growth", Zone::Hand);
        let own = mtg_kernel::rl::observe_v1(&state, actor, 1).unwrap();
        let opposing = mtg_kernel::rl::observe_v1(&state, actor.opponent(), 1).unwrap();
        assert_eq!(own.own_hand.len(), 1);
        assert!(opposing.own_hand.is_empty());
        assert!(!serde_json::to_string(&opposing)
            .unwrap()
            .contains("Giant Growth"));
        assert_eq!(source_trigger_count(&state, source), 1);
        let replay = restore(&state);
        assert_eq!(
            serde_json::to_vec(&opposing).unwrap(),
            serde_json::to_vec(&mtg_kernel::rl::observe_v1(&replay, actor.opponent(), 1).unwrap())
                .unwrap()
        );
    }
}

#[test]
fn only_actual_creature_battlefield_to_graveyard_including_token_satisfies_morbid() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for (name, from, to, counts) in [
            ("Forest", Zone::Battlefield, Zone::Graveyard, false),
            ("Llanowar Elves", Zone::Hand, Zone::Graveyard, false),
            ("Llanowar Elves", Zone::Battlefield, Zone::Exile, false),
            (
                "Koma's Coil Token",
                Zone::Battlefield,
                Zone::Graveyard,
                true,
            ),
        ] {
            let mut state = ready(actor);
            let source = cast(&mut state, actor, "Slumbering Cerberus");
            state.objects.get_mut(source).tapped = true;
            let other = add(&mut state, actor.opponent(), name, from);
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(other, to));
            end(&mut state);
            assert_eq!(source_trigger_count(&state, source), usize::from(counts));
            settle(&mut state);
            assert_eq!(state.objects.get(source).tapped, !counts);
            reach(&mut state, actor.opponent(), Step::Upkeep);
            assert!(!state.creature_died_this_turn_v1());
        }
    }
}
#[test]
fn printed_step_restriction_follows_current_controller_in_owner_oriented_vectors() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = cast(&mut state, actor, "Slumbering Cerberus");
        state.objects.get_mut(source).tapped = true;
        // Typed control primitive, not a claimed printed control spell.
        state.objects.get_mut(source).controller = actor.opponent();
        let mut replay = restore(&state);
        for branch in [&mut state, &mut replay] {
            reach(branch, actor.opponent(), Step::Upkeep);
            assert!(branch.objects.get(source).tapped);
            assert!(!branch.objects.get(source).summoning_sick);
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}
