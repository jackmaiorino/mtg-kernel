//! Prospective tactical validation constructed from public facts only.
use super::*;

#[derive(Serialize)]
struct Spec {
    split: &'static str,
    family: &'static str,
    actor: u8,
    spell: &'static str,
    damage: i32,
    attacker_counters: i16,
    inactive_friendly_creatures: usize,
    no_win_control: bool,
}

fn position(
    s: &Spec,
    hidden: bool,
    cat_counters: i16,
) -> Result<(FastActorSessionV1, Option<ObjectId>), String> {
    let actor = PlayerId(s.actor);
    let opponent = PlayerId(1 - s.actor);
    let mut state = GameState::new_from_libraries(&[], &[], crate::rl::card_name, 77_006);
    state.step = Step::DeclareBlockers;
    state.priority_player = actor;
    state.players[actor.index()].life = s.damage;
    let spell = put(&mut state, actor, s.spell, Zone::Hand);
    let creature;
    if s.family == "face_required" {
        state.active_player = opponent;
        state.players[opponent.index()].life = s.damage + i32::from(s.no_win_control);
        let mut attackers = Vec::new();
        for name in ["Avenging Hunter", "Myr Enforcer", "Myr Enforcer"] {
            let id = put(&mut state, opponent, name, Zone::Battlefield);
            state.objects.get_mut(id).counters.plus1_plus1 = i32::from(s.attacker_counters);
            state.objects.get_mut(id).tapped = true;
            let toughness = engine::effective_toughness(&state, id);
            state.objects.get_mut(id).damage = (toughness - s.damage).try_into().map_err(err)?;
            ensure(
                engine::effective_power(&state, id) >= s.damage,
                "validation attacker not independently lethal",
            )?;
            attackers.push(id);
        }
        state.engine.combat.attackers = attackers;
        state.engine.combat.blocked_by = Vec::new();
        creature = None;
    } else {
        ensure(
            s.family == "creature_required" && !s.no_win_control,
            "unknown retention validation family",
        )?;
        state.active_player = actor;
        let hunter = put(&mut state, actor, "Avenging Hunter", Zone::Battlefield);
        state.objects.get_mut(hunter).tapped = true;
        state.objects.get_mut(hunter).counters.plus1_plus1 = i32::from(s.attacker_counters);
        state.players[opponent.index()].life = engine::effective_power(&state, hunter);
        let cat = put(&mut state, opponent, "Sacred Cat", Zone::Battlefield);
        state.objects.get_mut(cat).counters.plus1_plus1 = i32::from(cat_counters);
        let toughness = engine::effective_toughness(&state, cat);
        state.objects.get_mut(cat).damage = (toughness - s.damage).try_into().map_err(err)?;
        state.engine.combat.attackers = vec![hunter];
        state.engine.combat.blocked_by = vec![(hunter, vec![cat])];
        creature = Some(cat);
    }
    for _ in 0..s.inactive_friendly_creatures {
        put(&mut state, actor, "Myr Enforcer", Zone::Battlefield);
    }
    state.engine.combat.attackers_declared = true;
    state.engine.combat.blockers_declared = true;
    for owner in [PlayerId::P0, PlayerId::P1] {
        for name in ["Forest", "Island", "Mountain", "Swamp"] {
            put(&mut state, owner, name, Zone::Library);
        }
        if hidden {
            state.players[owner.index()].library.reverse();
        }
    }
    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
    engine::step(&mut state, Action::CastSpell(spell)).map_err(err)?;
    ensure(
        matches!(
            engine::advance_until_decision(&mut state),
            Decision::ChooseTargets { .. }
        ),
        "validation root lacks target decision",
    )?;
    Ok((
        FastActorSessionV1::from_public_terminal_fixture_v1(state),
        creature,
    ))
}

pub(super) fn run(command: Command) -> Result<Value, String> {
    let mut specs = Vec::new();
    let next = command.equal_budget_validation;
    let counters = if next { [6, 7] } else { [4, 5] };
    let distractors = if next { [3, 4] } else { [1, 2] };
    for (spell, damage) in [("Lightning Bolt", 3), ("Lava Dart", 1)] {
        for actor in 0..2 {
            for attacker_counters in counters {
                for inactive_friendly_creatures in distractors {
                    for (family, no_win_control) in [
                        ("face_required", false),
                        ("creature_required", false),
                        ("face_required", true),
                    ] {
                        specs.push(Spec {
                            split: "evaluation",
                            family,
                            actor,
                            spell,
                            damage,
                            attacker_counters,
                            inactive_friendly_creatures,
                            no_win_control,
                        });
                    }
                }
            }
        }
    }
    ensure(specs.len() == 48, "validation cardinality differs")?;
    let (mut policy, _) = initialize(&command.source)?;
    let mut records = Vec::new();
    for (ordinal, spec) in specs.iter().enumerate() {
        let mut reference = None;
        for hidden in [false, true] {
            let (session, creature_target) = position(spec, hidden, if next { 7 } else { 5 })?;
            let FastActorResponseV1::Decision(d) = session.current_response() else {
                return Err("validation root terminal".into());
            };
            ensure(
                seat(d.acting_player) == spec.actor,
                "validation actor differs",
            )?;
            let (visible, actions) =
                PairedBo1PolicyInputV1::new(&session, d).diagnostic_visible_v4()?;
            let expected = if spec.family == "face_required" { 5 } else { 4 }
                + spec.inactive_friendly_creatures;
            ensure(
                d.legal_action_count as usize == expected && actions.len() == expected,
                "validation legal menu differs",
            )?;
            let mut face = None;
            let mut fatal = None;
            let mut creature = None;
            for (i, a) in actions.iter().enumerate() {
                if let ActionSemanticV1::ChooseTarget { target, .. } = a {
                    match target {
                        crate::rl::TargetRefV1::StackItem { .. } => {
                            panic!("fixture does not support stack ability targets")
                        }
                        crate::rl::TargetRefV1::Player { player } => {
                            if *player == d.acting_player {
                                fatal = Some(i)
                            } else {
                                face = Some(i)
                            }
                        }
                        crate::rl::TargetRefV1::Object { object } => {
                            if creature_target.is_some_and(|id| id.0 == object.arena_id) {
                                creature = Some(i)
                            }
                        }
                    }
                }
            }
            let face = face.ok_or("missing validation face")?;
            let fatal = fatal.ok_or("missing validation self")?;
            let outcomes = (0..d.legal_action_count)
                .map(|i| combat::combat_witness(&session, d, i))
                .collect::<Result<Vec<_>, _>>()?;
            let winners: Vec<_> = outcomes
                .iter()
                .enumerate()
                .filter(|(_, v)| v["classification"] == "win")
                .map(|(i, _)| i)
                .collect();
            ensure(
                outcomes[fatal]["classification"] == "loss",
                "validation self control differs",
            )?;
            if spec.family == "face_required" {
                let wanted = if spec.no_win_control {
                    Vec::new()
                } else {
                    vec![face]
                };
                ensure(
                    winners == wanted
                        && outcomes
                            .iter()
                            .enumerate()
                            .all(|(i, v)| wanted.contains(&i) || v["classification"] == "loss"),
                    "validation face witnesses differ",
                )?;
            } else {
                ensure(
                    winners == vec![creature.ok_or("missing validation Cat")?]
                        && outcomes[face]["classification"] != "win",
                    "validation creature witnesses differ",
                )?;
            }
            let scores = policy.score_fast_session_v1(&session)?;
            let data = json!({"visible":visible,"actions":actions,"tensor":TensorBitsV1::from_tensor(&policy.last_scored_training_tensor_v4()?.common),"outcomes":outcomes,"winning_indices":winners});
            let invariant = json!({"data":data,"logits":bits(&scores.logits),"value_bits":scores.value.to_bits()});
            if let Some(old) = &reference {
                ensure(
                    old == &invariant,
                    "retention validation hidden-state invariance differs",
                )?;
            } else {
                records.push(json!({"id":format!("{}-{ordinal:03}",if next {"equal128-eval"} else {"retention-eval"}),"spec":spec,"data":data}));
                reference = Some(invariant);
            }
        }
    }
    let result = json!({"schema":if next {"public-terminal-equal-budget-validation/v1"} else {"public-terminal-retention-validation/v1"},"public_positions":48,"hidden_world_checks":96,
        "winning_positions":32,"no_win_controls":16,"records":records,"source":command.source,"model":policy.actual_model_identity_v1(),
        "non_claim":"Prospective synthetic public tactical validation. No candidate scoring, parent-performance summary or independent-game strength inference."});
    fs::create_dir(&command.output_directory).map_err(err)?;
    publish_json(&command.output_directory, "result.json", &result)?;
    Ok(result)
}
