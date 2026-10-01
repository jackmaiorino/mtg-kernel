//! Synthetic public-only teacher domain. No live session or hidden-state input.
use super::*;

#[derive(Clone, Debug, Serialize)]
struct Spec {
    split: &'static str,
    family: &'static str,
    actor: u8,
    spell: &'static str,
    damage: i32,
    attacker: &'static str,
    second_attacker: &'static str,
    attacker_counters: i16,
    cat_counters: i16,
    no_win_control: bool,
}

// All inputs describe public fixture facts. The independent hidden world below
// is manufactured here, never copied from a live game or an opponent deck.
fn position(spec: &Spec, hidden: bool) -> Result<(FastActorSessionV1, Option<ObjectId>), String> {
    let actor = PlayerId(spec.actor);
    let opponent = PlayerId(1 - spec.actor);
    let mut state = GameState::new_from_libraries(&[], &[], crate::rl::card_name, 77_005);
    state.step = Step::DeclareBlockers;
    state.priority_player = actor;
    state.players[actor.index()].life = spec.damage;
    let spell = put(&mut state, actor, spec.spell, Zone::Hand);
    let creature_target;
    if spec.family == "face_required" {
        state.active_player = opponent;
        state.players[opponent.index()].life = spec.damage + i32::from(spec.no_win_control);
        let first = put(&mut state, opponent, spec.attacker, Zone::Battlefield);
        let second = put(
            &mut state,
            opponent,
            spec.second_attacker,
            Zone::Battlefield,
        );
        for id in [first, second] {
            state.objects.get_mut(id).counters.plus1_plus1 = spec.attacker_counters;
            state.objects.get_mut(id).tapped = true;
            let toughness = engine::effective_toughness(&state, id);
            state.objects.get_mut(id).damage = (toughness - spec.damage).try_into().map_err(err)?;
        }
        state.engine.combat.attackers = vec![first, second];
        state.engine.combat.blocked_by = Vec::new();
        creature_target = None;
    } else {
        ensure(
            spec.family == "creature_required" && !spec.no_win_control,
            "unknown teacher family",
        )?;
        state.active_player = actor;
        let attacker = put(&mut state, actor, spec.attacker, Zone::Battlefield);
        state.objects.get_mut(attacker).tapped = true;
        state.objects.get_mut(attacker).counters.plus1_plus1 = spec.attacker_counters;
        state.players[opponent.index()].life = engine::effective_power(&state, attacker);
        let cat = put(&mut state, opponent, "Sacred Cat", Zone::Battlefield);
        state.objects.get_mut(cat).counters.plus1_plus1 = spec.cat_counters;
        let toughness = engine::effective_toughness(&state, cat);
        state.objects.get_mut(cat).damage = (toughness - spec.damage).try_into().map_err(err)?;
        state.engine.combat.attackers = vec![attacker];
        state.engine.combat.blocked_by = vec![(attacker, vec![cat])];
        creature_target = Some(cat);
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
        "teacher root lacks target decision",
    )?;
    Ok((
        FastActorSessionV1::from_public_terminal_fixture_v1(state),
        creature_target,
    ))
}

fn specifications() -> Vec<Spec> {
    let mut specs = Vec::new();
    for split in ["train", "evaluation"] {
        let spells = if split == "train" {
            vec![("Lightning Bolt", 3), ("Lava Dart", 1)]
        } else {
            vec![("Galvanic Blast", 2)]
        };
        for (spell, damage) in spells {
            for family in ["face_required", "creature_required"] {
                for actor in 0..2 {
                    for boost in 0..4 {
                        specs.push(Spec {
                            split,
                            family,
                            actor,
                            spell,
                            damage,
                            attacker: if split == "train" {
                                "Avenging Hunter"
                            } else {
                                "Spinewoods Paladin"
                            },
                            second_attacker: if split == "train" {
                                "Myr Enforcer"
                            } else {
                                "Gurmag Angler"
                            },
                            attacker_counters: boost,
                            cat_counters: if split == "train" { 3 } else { 4 },
                            no_win_control: false,
                        });
                    }
                }
            }
        }
    }
    for actor in 0..2 {
        for boost in 0..4 {
            specs.push(Spec {
                split: "evaluation",
                family: "face_required",
                actor,
                spell: "Galvanic Blast",
                damage: 2,
                attacker: "Spinewoods Paladin",
                second_attacker: "Gurmag Angler",
                attacker_counters: boost,
                cat_counters: 4,
                no_win_control: true,
            });
        }
    }
    specs
}

pub(super) fn run(command: Command) -> Result<Value, String> {
    let (mut policy, _) = initialize(&command.source)?;
    let mut records = Vec::new();
    for (ordinal, spec) in specifications().iter().enumerate() {
        let mut reference = None;
        for hidden in [false, true] {
            let (session, creature_target) = position(spec, hidden)?;
            let FastActorResponseV1::Decision(d) = session.current_response() else {
                return Err("teacher root terminal".into());
            };
            ensure(seat(d.acting_player) == spec.actor, "teacher actor differs")?;
            let (visible, actions) =
                PairedBo1PolicyInputV1::new(&session, d).diagnostic_visible_v4()?;
            ensure(
                d.legal_action_count == 4 && actions.len() == 4,
                "teacher legal menu differs",
            )?;
            let mut face = None;
            let mut creature = None;
            let mut fatal = None;
            for (i, a) in actions.iter().enumerate() {
                if let ActionSemanticV1::ChooseTarget { target, .. } = a {
                    match target {
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
            let face = face.ok_or("missing teacher face target")?;
            let fatal = fatal.ok_or("missing teacher self target")?;
            let mut outcomes = Vec::new();
            for i in 0..d.legal_action_count {
                outcomes.push(combat::combat_witness(&session, d, i)?);
            }
            let winners: Vec<usize> = outcomes
                .iter()
                .enumerate()
                .filter(|(_, o)| o["classification"] == "win")
                .map(|(i, _)| i)
                .collect();
            ensure(
                outcomes[fatal]["classification"] == "loss",
                "teacher self control failed",
            )?;
            if spec.family == "face_required" {
                let expected = if spec.no_win_control {
                    Vec::new()
                } else {
                    vec![face]
                };
                ensure(winners == expected, "teacher face winner differs")?;
                ensure(
                    outcomes
                        .iter()
                        .enumerate()
                        .all(|(i, o)| expected.contains(&i) || o["classification"] == "loss"),
                    "teacher face alternative unresolved",
                )?;
            } else {
                ensure(
                    winners == vec![creature.ok_or("missing teacher Cat target")?],
                    "teacher creature winner differs",
                )?;
                ensure(
                    outcomes[face]["classification"] != "win",
                    "teacher creature contrast failed",
                )?;
            }
            let scores = policy.score_fast_session_v1(&session)?;
            let tensor =
                TensorBitsV1::from_tensor(&policy.last_scored_training_tensor_v4()?.common);
            let signature = json!({"visible":visible,"actions":actions,"tensor":tensor,"outcomes":outcomes,"winning_indices":winners});
            let invariant = json!({"data":signature,"logits":bits(&scores.logits),"value_bits":scores.value.to_bits()});
            if let Some(old) = reference.as_ref() {
                ensure(old == &invariant, "teacher hidden-world invariance failed")?;
            } else {
                records
                    .push(json!({"id":format!("case-{ordinal:03}"),"spec":spec,"data":signature}));
                reference = Some(invariant);
            }
        }
    }
    ensure(records.len() == 56, "teacher dataset cardinality differs")?;
    let result = json!({"schema":"public-terminal-teacher-data/v1","public_positions":56,"hidden_world_checks":112,
        "training_positions":32,"evaluation_positions":24,"records":records,"source":command.source,"model":policy.actual_model_identity_v1(),
        "non_claim":"Restricted synthetic teacher dataset; unresolved branches retain their classification and are never labeled losses. No live-state teacher, training or generalization claim."});
    fs::create_dir(&command.output_directory).map_err(err)?;
    publish_json(&command.output_directory, "result.json", &result)?;
    Ok(result)
}
