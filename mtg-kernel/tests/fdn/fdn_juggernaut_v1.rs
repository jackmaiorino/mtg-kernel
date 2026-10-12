//! Future actual printed fixtures. Unregistered and unexecuted source preparation.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use mtg_kernel::card_def::{
    card_id_by_name, CardType, CombatStaticRulesDef, Keywords, Subtype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameState, Step, Zone};
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
        Decision::DeclareAttackers { .. } => Action::DeclareAttackers(
            state.players[state.active_player.index()]
                .battlefield
                .iter()
                .copied()
                .filter(|id| {
                    let o = state.objects.get(*id);
                    !o.tapped
                        && !o.summoning_sick
                        && engine::object_has_type(
                            state,
                            *id,
                            mtg_kernel::card_def::CardType::Creature,
                        )
                })
                .collect(),
        ),
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
fn target_spell(state: &mut GameState, actor: PlayerId, name: &str, target: ObjectId) {
    let spell = add(state, actor, name, Zone::Hand);
    state.players[actor.index()].mana_pool = [10; 6];
    assert!(matches!(next(state), Decision::CastSpellOrPass { player, .. } if player == actor));
    engine::step(state, Action::CastSpell(spell)).unwrap();
    let target = mtg_kernel::state::Target::Object(target);
    assert!(
        matches!(next(state), Decision::ChooseTargets { legal_targets, .. } if legal_targets.contains(&target))
    );
    engine::step(state, Action::ChooseTarget(target)).unwrap();
    settle(state);
}

fn attack_decision(state: &mut GameState) -> Vec<ObjectId> {
    for _ in 0..100 {
        let decision = next(state);
        if let Decision::DeclareAttackers { eligible, .. } = decision {
            return eligible;
        }
        act(state, decision);
    }
    panic!("real declare attackers not reached");
}

fn block_decision(state: &mut GameState, source: ObjectId) -> Vec<ObjectId> {
    for _ in 0..100 {
        let decision = next(state);
        if let Decision::DeclareBlockers { legal_blockers, .. } = decision {
            return legal_blockers
                .into_iter()
                .find(|(id, _)| *id == source)
                .unwrap()
                .1;
        }
        act(state, decision);
    }
    panic!("real declare blockers not reached");
}

#[test]
fn genuine_cast_cost_sickness_and_printed_identity_both_seats() {
    let def =
        &CARD_DEFS[card_id_by_name("Juggernaut").expect("future actual registration") as usize];
    assert_eq!(def.cost.generic, 4);
    assert!(def.cost.pips.is_empty());
    assert_eq!(def.types, &[CardType::Artifact, CardType::Creature]);
    assert_eq!(def.subtypes, &[Subtype::Juggernaut]);
    assert_eq!((def.power, def.toughness), (Some(5), Some(3)));
    assert_eq!(
        def.combat_static_rules,
        CombatStaticRulesDef {
            must_attack_each_combat: true,
            cant_be_blocked_by_subtype: Some(Subtype::Wall),
        }
    );
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = add(&mut state, actor, "Juggernaut", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 3;
        next(&mut state);
        let before = serde_json::to_vec(&state).unwrap();
        assert!(engine::step(&mut state, Action::CastSpell(source)).is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 4;
        engine::step(&mut state, Action::CastSpell(source)).unwrap();
        settle(&mut state);
        assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
        assert_eq!(
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()],
            0
        );
        assert!(state.objects.get(source).summoning_sick);
        assert!(!attack_decision(&mut state).contains(&source));
        engine::step(&mut state, Action::DeclareAttackers(vec![])).unwrap();
    }
}

#[test]
fn actual_eligible_requirement_is_readonly_and_restore_exact_without_goad() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = add(&mut state, actor, "Juggernaut", Zone::Battlefield);
        state.objects.get_mut(source).summoning_sick = false;
        assert!(state.objects.get(source).v4.goaded_by.is_empty());
        let optional = add(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        state.objects.get_mut(optional).summoning_sick = false;
        assert!(attack_decision(&mut state).contains(&source));
        let before = serde_json::to_vec(&state).unwrap();
        for attackers in [vec![], vec![optional], vec![source, source]] {
            assert!(engine::step(&mut state, Action::DeclareAttackers(attackers)).is_err());
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        }
        let mut replay = restore(&state);
        for current in [&mut state, &mut replay] {
            engine::step(current, Action::DeclareAttackers(vec![source])).unwrap();
            assert!(current.objects.get(source).tapped);
        }
        assert_eq!(
            serde_json::to_value(state).unwrap(),
            serde_json::to_value(replay).unwrap()
        );
    }
}

#[test]
fn tapped_defender_and_reentered_incarnation_are_not_forced() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for mode in 0..3 {
            let mut state = ready(actor);
            let source = add(&mut state, actor, "Juggernaut", Zone::Battlefield);
            state.objects.get_mut(source).summoning_sick = false;
            match mode {
                0 => state.objects.get_mut(source).tapped = true,
                // Typed existing continuous keyword boundary, not a purported printed producer.
                1 => {
                    let timestamp = state.engine.next_effect_timestamp;
                    state.engine.next_effect_timestamp += 1;
                    state
                        .objects
                        .get_mut(source)
                        .v4
                        .creature_upgrade
                        .get_or_insert_with(Default::default)
                        .keyword_grants
                        .push((Keywords::DEFENDER, timestamp));
                }
                _ => {
                    let old = state.objects.get(source).zone_change_count;
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(source, Zone::Exile),
                    );
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(source, Zone::Battlefield),
                    );
                    assert!(state.objects.get(source).zone_change_count > old);
                    assert!(state.objects.get(source).summoning_sick);
                }
            }
            assert!(!attack_decision(&mut state).contains(&source));
            let mut replay = restore(&state);
            engine::step(&mut state, Action::DeclareAttackers(vec![])).unwrap();
            engine::step(&mut replay, Action::DeclareAttackers(vec![])).unwrap();
            assert_eq!(
                serde_json::to_value(state).unwrap(),
                serde_json::to_value(replay).unwrap()
            );
        }
    }
}

#[test]
fn actual_wall_and_changeling_block_menus_and_application_agree() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = add(&mut state, actor, "Juggernaut", Zone::Battlefield);
        state.objects.get_mut(source).summoning_sick = false;
        let wall = add(
            &mut state,
            actor.opponent(),
            "Rune-Sealed Wall",
            Zone::Battlefield,
        );
        let changeling = add(
            &mut state,
            actor.opponent(),
            "Webweaver Changeling",
            Zone::Battlefield,
        );
        let ordinary = add(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        attack_decision(&mut state);
        engine::step(&mut state, Action::DeclareAttackers(vec![source])).unwrap();
        let legal = block_decision(&mut state, source);
        assert!(!legal.contains(&wall));
        assert!(!legal.contains(&changeling));
        assert!(legal.contains(&ordinary));
        let before = serde_json::to_vec(&state).unwrap();
        for blocker in [wall, changeling] {
            assert!(
                engine::step(&mut state, Action::DeclareBlockers(vec![(blocker, source)])).is_err()
            );
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        }
        let mut replay = restore(&state);
        engine::step(
            &mut state,
            Action::DeclareBlockers(vec![(ordinary, source)]),
        )
        .unwrap();
        engine::step(
            &mut replay,
            Action::DeclareBlockers(vec![(ordinary, source)]),
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(state).unwrap(),
            serde_json::to_value(replay).unwrap()
        );
    }
}

#[test]
fn genuine_witness_removes_both_rules_and_wall_subtype_replacement_matters() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for suppress_attacker in [true, false] {
            let mut state = ready(actor);
            let source = add(&mut state, actor, "Juggernaut", Zone::Battlefield);
            state.objects.get_mut(source).summoning_sick = false;
            let wall = add(
                &mut state,
                actor.opponent(),
                "Rune-Sealed Wall",
                Zone::Battlefield,
            );
            target_spell(
                &mut state,
                actor,
                "Witness Protection",
                if suppress_attacker { source } else { wall },
            );
            assert!(attack_decision(&mut state).contains(&source));
            if suppress_attacker {
                let mut optional = restore(&state);
                engine::step(&mut optional, Action::DeclareAttackers(vec![])).unwrap();
            }
            engine::step(&mut state, Action::DeclareAttackers(vec![source])).unwrap();
            assert!(block_decision(&mut state, source).contains(&wall));
        }
    }
}

#[test]
fn both_selected_roots_reject_forged_facts_identity_and_unknown_fields() {
    use mtg_kernel::rules_meaning_juggernaut_v1::{self as selected, RootV1};
    let selector = selected::live_selector().unwrap();
    let id = card_id_by_name("Juggernaut").unwrap();
    for root in [RootV1::MustAttackEachCombat, RootV1::CannotBeBlockedByWalls] {
        let record = selected::meaning(&selector, id, root).unwrap();
        selected::validate(&record).unwrap();
        let value = serde_json::to_value(&record).unwrap();
        for field in value["rule"].as_object().unwrap().keys() {
            let mut forged = value.clone();
            forged["rule"][field] =
                serde_json::Value::Bool(!value["rule"][field].as_bool().unwrap());
            assert!(selected::validate(&serde_json::from_value(forged).unwrap()).is_err());
        }
        let mut forged = record.clone();
        forged.model_and_sampler_admitted = true;
        assert!(selected::validate(&forged).is_err());
        let mut unknown = value;
        unknown["rule"]["unexpected"] = true.into();
        assert!(serde_json::from_value::<selected::MeaningV1>(unknown).is_err());
    }
}

#[test]
fn actual_temporary_theft_uses_current_controller_haste_and_cleanup_reverts_owner_board() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let owner = actor.opponent();
        let source = add(&mut state, owner, "Juggernaut", Zone::Battlefield);
        assert_eq!(state.objects.get(source).owner, owner);
        target_spell(&mut state, actor, "Involuntary Employment", source);
        assert_eq!(state.objects.get(source).controller, actor);
        assert!(engine::has_effective_keyword(
            &state,
            source,
            Keywords::HASTE
        ));
        assert!(attack_decision(&mut state).contains(&source));
        let before = serde_json::to_vec(&state).unwrap();
        assert!(engine::step(&mut state, Action::DeclareAttackers(vec![])).is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        engine::step(&mut state, Action::DeclareAttackers(vec![source])).unwrap();
        block_decision(&mut state, source);
        engine::step(&mut state, Action::DeclareBlockers(vec![])).unwrap();
        let mut replay = restore(&state);
        for current in [&mut state, &mut replay] {
            for _ in 0..150 {
                let d = next(current);
                if current.active_player == owner {
                    break;
                }
                act(current, d);
            }
            assert_eq!(current.active_player, owner);
            assert_eq!(current.objects.get(source).controller, owner);
            assert!(!engine::has_effective_keyword(
                current,
                source,
                Keywords::HASTE
            ));
            assert!(current.players[owner.index()].battlefield.contains(&source));
            assert!(attack_decision(current).contains(&source));
            let before = serde_json::to_vec(current).unwrap();
            assert!(engine::step(current, Action::DeclareAttackers(vec![])).is_err());
            assert_eq!(serde_json::to_vec(current).unwrap(), before);
        }
        assert_eq!(
            serde_json::to_value(state).unwrap(),
            serde_json::to_value(replay).unwrap()
        );
    }
}
