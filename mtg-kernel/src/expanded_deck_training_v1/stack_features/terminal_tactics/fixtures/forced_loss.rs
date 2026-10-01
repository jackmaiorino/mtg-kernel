//! A public immediate win versus removal that leaves a lethal unblocked attacker.
use super::*;

fn position(
    actor: u8,
    hidden: bool,
    opponent_life: i32,
) -> Result<(FastActorSessionV1, [ObjectId; 2]), String> {
    let actor = PlayerId(actor);
    let opponent = PlayerId(1 - actor.0);
    let mut state = GameState::new_from_libraries(&[], &[], crate::rl::card_name, 77_004);
    state.step = Step::DeclareBlockers;
    state.active_player = opponent;
    state.priority_player = actor;
    state.players[actor.index()].life = 3;
    state.players[opponent.index()].life = opponent_life;
    let spell = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
    let attackers = [
        put(&mut state, opponent, "Avenging Hunter", Zone::Battlefield),
        put(&mut state, opponent, "Myr Enforcer", Zone::Battlefield),
    ];
    for &attacker in &attackers {
        state.objects.get_mut(attacker).tapped = true;
        state.objects.get_mut(attacker).damage = 1;
    }
    state.engine.combat.attackers_declared = true;
    state.engine.combat.blockers_declared = true;
    state.engine.combat.attackers = attackers.to_vec();
    state.engine.combat.blocked_by = Vec::new();
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
        "forced-loss fixture lacks target decision",
    )?;
    Ok((
        FastActorSessionV1::from_public_terminal_fixture_v1(state),
        attackers,
    ))
}

pub(super) fn run(command: Command) -> Result<Value, String> {
    let (mut policy, _) = initialize(&command.source)?;
    let mut records = Vec::new();
    for opponent_life in [3, 4] {
        for actor in 0..2 {
            let mut invariant = None;
            for hidden in [false, true] {
                let (session, attackers) = position(actor, hidden, opponent_life)?;
                let FastActorResponseV1::Decision(d) = session.current_response() else {
                    return Err("forced-loss root terminal".into());
                };
                ensure(seat(d.acting_player) == actor, "forced-loss actor differs")?;
                let (visible, actions) =
                    PairedBo1PolicyInputV1::new(&session, d).diagnostic_visible_v4()?;
                ensure(
                    actions.len() == 4 && d.legal_action_count == 4,
                    "forced-loss menu differs",
                )?;
                let mut face = None;
                let mut fatal = None;
                let mut creatures = Vec::new();
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
                                if attackers.iter().any(|id| id.0 == object.arena_id) {
                                    creatures.push(i)
                                }
                            }
                        }
                    }
                }
                let face = face.ok_or("missing opponent target")?;
                let fatal = fatal.ok_or("missing self target")?;
                ensure(creatures.len() == 2, "missing attacker targets")?;
                let mut outcomes = Vec::new();
                for i in 0..d.legal_action_count {
                    outcomes.push(combat::combat_witness(&session, d, i)?);
                }
                let expected_face = if opponent_life == 3 { "win" } else { "loss" };
                if outcomes[face]["classification"] != expected_face
                    || outcomes[fatal]["classification"] != "loss"
                    || creatures
                        .iter()
                        .any(|&i| outcomes[i]["classification"] != "loss")
                {
                    return Err(format!("forced-loss terminal controls failed: actor={actor} life={opponent_life} outcomes={outcomes:?}"));
                }
                // Score only after all four choices have natural terminal witnesses.
                let scores = policy.score_fast_session_v1(&session)?;
                let tensor =
                    TensorBitsV1::from_tensor(&policy.last_scored_training_tensor_v4()?.common);
                let p = probability(&scores.logits);
                let mut best = 0;
                for i in 1..scores.logits.len() {
                    if scores.logits[i] > scores.logits[best] {
                        best = i;
                    }
                }
                let signature = json!({"visible":visible,"actions":actions,"tensor":tensor,"logits":bits(&scores.logits),"value_bits":scores.value.to_bits(),"outcomes":outcomes});
                ensure(
                    invariant.as_ref().is_none_or(|old| old == &signature),
                    "forced-loss hidden-order invariance failed",
                )?;
                invariant = Some(signature);
                records.push(json!({"actor":actor,"opponent_life":opponent_life,"hidden_order_variant":hidden,
                "face_index":face,"creature_indices":creatures,"self_index":fatal,"argmax":best,
                "immediate_win_available":opponent_life==3,"argmax_classification":outcomes[best]["classification"],
                "face_softmax_probability":p[face],"probabilities":p,"signature":invariant}));
            }
        }
    }
    let result = json!({"schema":"public-forced-loss-controls/v1","positions":8,"distinct_public_positions":4,
        "records":records,"source":command.source,"model":policy.actual_model_identity_v1(),
        "non_claim":"Synthetic terminal-return contrast only; no natural prevalence, generalization or human-strength claim."});
    fs::create_dir(&command.output_directory).map_err(err)?;
    publish_json(&command.output_directory, "result.json", &result)?;
    Ok(result)
}
