//! Development-only intervention. Its receipts are not sampled-policy
//! trajectories and must never be consumed as training or promotion evidence.
use super::*;
use crate::card_def::CARD_DEFS;
use crate::expanded_deck_training_v1::{
    load_expanded_inference_v1, ExpandedDeckListV1, ExpandedModelSourceV1,
};
use crate::mana::{ManaColor, Pip};
use crate::paired_bo1_harness_v1::PlayPolicyGenerationV1;
use crate::rl::{ActionSemanticV1, PlayerSeatV1};
use crate::rl_session::RlSessionErrorCode;
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Baseline,
    UniquePrintedColor,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tags {
    pub requires_target: std::collections::BTreeSet<u16>,
    pub is_counterspell: std::collections::BTreeSet<u16>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub config: LearnedBo3RunConfigV1,
    pub registrations: [ExpandedDeckListV1; 2],
    pub sources: [ExpandedModelSourceV1; 2],
    pub expected_models: [ExpandedInferenceIdentityV1; 2],
    pub tags: Tags,
    pub diagnostic_seat: PlayerSeatV1,
    pub mode: Mode,
}

#[derive(Debug, Serialize)]
pub struct DecisionRecord {
    pub game_index: usize,
    pub decision_index: usize,
    pub actor: PlayerSeatV1,
    pub visible_sha256: String,
    pub raw_index: u32,
    pub applied_index: u32,
    pub raw_action: ActionSemanticV1,
    pub applied_action: ActionSemanticV1,
}

#[derive(Debug, Serialize)]
pub struct ResultRecord {
    pub schema: String,
    pub request: Request,
    pub actual_base_models: [ExpandedInferenceIdentityV1; 2],
    pub printed_mainboard_colors: Vec<ManaColor>,
    pub seed_resets: Vec<[u64; 2]>,
    pub outcome: MatchOutcomeV1,
    pub games: Vec<LearnedBo3GameRecordV1>,
    pub decisions: Vec<DecisionRecord>,
}

fn printed_colors(mainboard: &[u16]) -> Result<Vec<ManaColor>, String> {
    let mut present = [false; 6];
    for id in mainboard {
        let card = CARD_DEFS
            .get(*id as usize)
            .ok_or("unknown registered card")?;
        for pip in card.cost.pips {
            match *pip {
                Pip::Colored(c) | Pip::Phyrexian(c) => present[c.pool_index()] = true,
                Pip::Hybrid(a, b) => {
                    present[a.pool_index()] = true;
                    present[b.pool_index()] = true;
                }
            }
        }
    }
    Ok(ManaColor::ALL
        .into_iter()
        .filter(|c| present[c.pool_index()])
        .collect())
}

/// Only one named Gate source, one actor, and a unique legal registered-cost
/// color may qualify. No arena identity, opponent information or future state
/// participates in choosing the color. Ambiguous menus are unchanged.
fn replacement(
    actions: &[ActionSemanticV1],
    actor: PlayerSeatV1,
    colors: &[ManaColor],
) -> Option<u32> {
    let ActionSemanticV1::ChooseEffectColor {
        actor: first_actor,
        source,
        ..
    } = actions.first()?
    else {
        return None;
    };
    if *first_actor != actor || source.controller != actor {
        return None;
    }
    let name = CARD_DEFS.get(source.card_db_id as usize)?.name;
    if name != "Sea Gate" && name != "Citadel Gate" {
        return None;
    }
    let mut selected = None;
    for (index, action) in actions.iter().enumerate() {
        let ActionSemanticV1::ChooseEffectColor {
            actor: a,
            source: s,
            color,
        } = action
        else {
            return None;
        };
        if *a != actor || s != source {
            return None;
        }
        if colors.contains(color) {
            if selected.is_some() {
                return None;
            }
            selected = Some(index as u32);
        }
    }
    selected
}

struct DiagnosticPolicy<'a> {
    base: &'a mut dyn PairedBo1PolicyV1,
    seat: PlayerSeatV1,
    mode: Mode,
    colors: Vec<ManaColor>,
    resets: Vec<[u64; 2]>,
    decisions: Vec<DecisionRecord>,
    game_decisions: usize,
}

fn diagnostic_error(message: impl ToString) -> RlSessionError {
    RlSessionError {
        code: RlSessionErrorCode::StaleEnvironmentBinding,
        message: message.to_string(),
    }
}

impl PairedBo1PolicyV1 for DiagnosticPolicy<'_> {
    fn uses_observation_successor_v3(&self) -> bool {
        self.base.uses_observation_successor_v3()
    }
    fn feature_generation_v1(&self) -> PlayPolicyGenerationV1 {
        self.base.feature_generation_v1()
    }
    fn reset_for_game_v1(&mut self, seeds: [u64; 2]) -> Result<(), RlSessionError> {
        self.base.reset_for_game_v1(seeds)?;
        self.resets.push(seeds);
        self.game_decisions = 0;
        Ok(())
    }
    fn select_action_v1(
        &mut self,
        input: PairedBo1PolicyInputV1<'_>,
    ) -> Result<u32, RlSessionError> {
        let (observation, actions) = input.diagnostic_visible_v1().map_err(diagnostic_error)?;
        let actor = observation.acting_player;
        let bytes = serde_json::to_vec(&(&observation, &actions)).map_err(diagnostic_error)?;
        let visible_sha256 = format!("{:x}", Sha256::digest(bytes));
        // Consume exactly one base selection even when an intervention applies.
        let raw = self.base.select_action_v1(input)?;
        if raw as usize >= actions.len() {
            return Err(diagnostic_error("base choice outside bound menu"));
        }
        let applied = if self.mode == Mode::UniquePrintedColor && actor == self.seat {
            replacement(&actions, actor, &self.colors).unwrap_or(raw)
        } else {
            raw
        };
        self.decisions.push(DecisionRecord {
            game_index: self.resets.len(),
            decision_index: self.game_decisions,
            actor,
            visible_sha256,
            raw_index: raw,
            applied_index: applied,
            raw_action: actions[raw as usize].clone(),
            applied_action: actions[applied as usize].clone(),
        });
        self.game_decisions += 1;
        Ok(applied)
    }
}

pub fn run(request: Request) -> Result<ResultRecord, String> {
    validate_run_limits_v1(&request.config)?;
    if request.config.opening_protocol != Bo3OpeningProtocolV1::KeepSevenV2 {
        return Err("diagnostic requires explicit keep_seven_v2".into());
    }
    let [(mut p0, m0), (mut p1, m1)] = [
        load_expanded_inference_v1(&request.sources[0])?,
        load_expanded_inference_v1(&request.sources[1])?,
    ];
    let actual = [m0, m1];
    if actual != request.expected_models {
        return Err("actual base models differ from declared models".into());
    }
    if [p0.feature_generation_v1(), p1.feature_generation_v1()] != [PlayPolicyGenerationV1::V4; 2] {
        return Err("diagnostic requires two actual V4 models".into());
    }
    let mut decks = Vec::new();
    for (seat, registration) in request.registrations.iter().enumerate() {
        if registration.label != request.config.deck_ids[seat] {
            return Err("registration label mismatch".into());
        }
        decks.push(
            RegisteredDeckV1::new_executable_v1(
                &registration.label,
                registration.mainboard.clone(),
                registration.sideboard.clone(),
            )
            .map_err(|e| e.to_string())?,
        );
    }
    let colors = printed_colors(
        &request.registrations[if request.diagnostic_seat == PlayerSeatV1::P0 {
            0
        } else {
            1
        }]
        .mainboard,
    )?;
    let session = BestOfThreeDeckMatchV1::new_live_v1(
        decks.try_into().unwrap(),
        request.config.game_one_chooser,
    )
    .map_err(|e| e.to_string())?;
    let mut router = SeatRoutedBo3PlayPolicyV1::new_v1([&mut p0, &mut p1])?;
    let mut policy = DiagnosticPolicy {
        base: &mut router,
        seat: request.diagnostic_seat,
        mode: request.mode,
        colors: colors.clone(),
        resets: vec![],
        decisions: vec![],
        game_decisions: 0,
    };
    let keep = |_: &LearnedSideboardInputV1, current: &DeckConfigurationV1| {
        Ok((current.clone(), vec![SideboardActionV1::Done]))
    };
    let mut keep0 = keep;
    let mut keep1 = keep;
    let played = run_learned_bo3_session_v1(
        request.config.clone(),
        session,
        Bo3ModelProvenanceV1::PerSeat(actual.each_ref().map(|m| m.model.weights_sha256.as_str())),
        &RemovalCounterspellTagsV1 {
            requires_target: request.tags.requires_target.clone(),
            is_counterspell: request.tags.is_counterspell.clone(),
        },
        &mut policy,
        [&mut keep0, &mut keep1],
    )?;
    Ok(ResultRecord {
        schema: "mtg-kernel-gate-color-counterfactual/v1".into(),
        request,
        actual_base_models: actual,
        printed_mainboard_colors: colors,
        seed_resets: policy.resets,
        outcome: played.outcome,
        games: played.games,
        decisions: policy.decisions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy_observation_v6::tests::{put, ready_state};
    use crate::rl::CardStableRefV1;
    use crate::rl_session::{FastActorDecisionV1, FastActorResponseV1};
    use crate::state::Zone;

    #[test]
    #[ignore = "explicit archived full-match request and output paths required"]
    fn public_input_zero_projection_full_match_replay() {
        use crate::native_policy_value_net_v1::public_inputs_v1::{
            PublicInputWeightsV1, ARCHITECTURE,
        };
        use crate::sideboard_play_policy_v1::public_inputs::PublicInputPlayPolicyV1;
        let request_bytes =
            std::fs::read(std::env::var("MTG_PUBLIC_MATCH_REQUEST").unwrap()).unwrap();
        let request: Request = serde_json::from_slice(&request_bytes).unwrap();
        assert_eq!(request.mode, Mode::Baseline);
        assert_eq!(
            request.config.opening_protocol,
            Bo3OpeningProtocolV1::KeepSevenV2
        );
        validate_run_limits_v1(&request.config).unwrap();
        let [(p0, m0), (p1, m1)] = [
            load_expanded_inference_v1(&request.sources[0]).unwrap(),
            load_expanded_inference_v1(&request.sources[1]).unwrap(),
        ];
        let actual = [m0, m1];
        assert_eq!(actual, request.expected_models);
        let mut p0 = PublicInputPlayPolicyV1::new(p0, PublicInputWeightsV1::zero()).unwrap();
        let mut p1 = PublicInputPlayPolicyV1::new(p1, PublicInputWeightsV1::zero()).unwrap();
        let decks: Vec<_> = request
            .registrations
            .iter()
            .enumerate()
            .map(|(seat, r)| {
                assert_eq!(r.label, request.config.deck_ids[seat]);
                RegisteredDeckV1::new_executable_v1(
                    &r.label,
                    r.mainboard.clone(),
                    r.sideboard.clone(),
                )
                .unwrap()
            })
            .collect();
        let session = BestOfThreeDeckMatchV1::new_live_v1(
            decks.try_into().unwrap(),
            request.config.game_one_chooser,
        )
        .unwrap();
        let mut router = SeatRoutedBo3PlayPolicyV1::new_v1([&mut p0, &mut p1]).unwrap();
        let mut policy = DiagnosticPolicy {
            base: &mut router,
            seat: request.diagnostic_seat,
            mode: Mode::Baseline,
            colors: vec![],
            resets: vec![],
            decisions: vec![],
            game_decisions: 0,
        };
        let keep = |_: &LearnedSideboardInputV1, current: &DeckConfigurationV1| {
            Ok((current.clone(), vec![SideboardActionV1::Done]))
        };
        let mut keep0 = keep;
        let mut keep1 = keep;
        let played = run_learned_bo3_session_v1(
            request.config.clone(),
            session,
            Bo3ModelProvenanceV1::PerSeat(
                actual.each_ref().map(|m| m.model.weights_sha256.as_str()),
            ),
            &RemovalCounterspellTagsV1 {
                requires_target: request.tags.requires_target.clone(),
                is_counterspell: request.tags.is_counterspell.clone(),
            },
            &mut policy,
            [&mut keep0, &mut keep1],
        )
        .unwrap();
        let baseline_bytes =
            std::fs::read(std::env::var("MTG_PUBLIC_MATCH_BASELINE").unwrap()).unwrap();
        let baseline: serde_json::Value = serde_json::from_slice(&baseline_bytes).unwrap();
        assert_eq!(baseline["request"], serde_json::to_value(&request).unwrap());
        assert_eq!(
            baseline["actual_base_models"],
            serde_json::to_value(&actual).unwrap()
        );
        assert_eq!(
            baseline["decisions"],
            serde_json::to_value(&policy.decisions).unwrap()
        );
        assert_eq!(
            baseline["seed_resets"],
            serde_json::to_value(&policy.resets).unwrap()
        );
        assert_eq!(
            baseline["games"],
            serde_json::to_value(&played.games).unwrap()
        );
        assert_eq!(
            baseline["outcome"],
            serde_json::to_value(played.outcome).unwrap()
        );
        let report = serde_json::json!({"schema":"public-input-zero-full-match-replay/v1","architecture":ARCHITECTURE,
            "status":"ENGINEERING-PASS","projection":"both matrices all positive zero; both seats",
            "request_sha256":format!("{:x}",Sha256::digest(&request_bytes)),
            "baseline_sha256":format!("{:x}",Sha256::digest(&baseline_bytes)),
            "actual_base_models":actual,"outcome":played.outcome,"games":played.games,
            "seed_resets":policy.resets,"decisions":policy.decisions,
            "all_baseline_gameplay_fields_exact":true,
            "non_claim":"No training or playing-strength evidence; original model provenance plus explicit zero-projection architecture."});
        let output = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(std::env::var("MTG_PUBLIC_MATCH_REPORT").unwrap())
            .unwrap();
        serde_json::to_writer(output, &report).unwrap();
        println!(
            "public-input full-match pass: {} games, {} decisions",
            report["games"].as_array().unwrap().len(),
            report["decisions"].as_array().unwrap().len()
        );
    }

    #[test]
    #[ignore = "explicit archived full-match request and output paths required"]
    fn stack_input_zero_projection_full_match_replay() {
        use crate::native_policy_value_net_v1::stack_inputs_v1::{
            StackInputWeightsV1, ARCHITECTURE,
        };
        use crate::sideboard_play_policy_v1::stack_inputs::StackInputPlayPolicyV1;
        let request_bytes =
            std::fs::read(std::env::var("MTG_STACK_MATCH_REQUEST").unwrap()).unwrap();
        let request: Request = serde_json::from_slice(&request_bytes).unwrap();
        assert_eq!(request.mode, Mode::Baseline);
        assert_eq!(
            request.config.opening_protocol,
            Bo3OpeningProtocolV1::KeepSevenV2
        );
        validate_run_limits_v1(&request.config).unwrap();
        let [(p0, m0), (p1, m1)] = [
            load_expanded_inference_v1(&request.sources[0]).unwrap(),
            load_expanded_inference_v1(&request.sources[1]).unwrap(),
        ];
        let actual = [m0, m1];
        assert_eq!(actual, request.expected_models);
        let mut p0 = StackInputPlayPolicyV1::new(p0, StackInputWeightsV1::zero()).unwrap();
        let mut p1 = StackInputPlayPolicyV1::new(p1, StackInputWeightsV1::zero()).unwrap();
        let decks: Vec<_> = request
            .registrations
            .iter()
            .enumerate()
            .map(|(seat, r)| {
                assert_eq!(r.label, request.config.deck_ids[seat]);
                RegisteredDeckV1::new_executable_v1(
                    &r.label,
                    r.mainboard.clone(),
                    r.sideboard.clone(),
                )
                .unwrap()
            })
            .collect();
        let session = BestOfThreeDeckMatchV1::new_live_v1(
            decks.try_into().unwrap(),
            request.config.game_one_chooser,
        )
        .unwrap();
        let mut router = SeatRoutedBo3PlayPolicyV1::new_v1([&mut p0, &mut p1]).unwrap();
        let mut policy = DiagnosticPolicy {
            base: &mut router,
            seat: request.diagnostic_seat,
            mode: Mode::Baseline,
            colors: vec![],
            resets: vec![],
            decisions: vec![],
            game_decisions: 0,
        };
        let keep = |_: &LearnedSideboardInputV1, current: &DeckConfigurationV1| {
            Ok((current.clone(), vec![SideboardActionV1::Done]))
        };
        let mut keep0 = keep;
        let mut keep1 = keep;
        let played = run_learned_bo3_session_v1(
            request.config.clone(),
            session,
            Bo3ModelProvenanceV1::PerSeat(
                actual.each_ref().map(|m| m.model.weights_sha256.as_str()),
            ),
            &RemovalCounterspellTagsV1 {
                requires_target: request.tags.requires_target.clone(),
                is_counterspell: request.tags.is_counterspell.clone(),
            },
            &mut policy,
            [&mut keep0, &mut keep1],
        )
        .unwrap();
        let baseline_bytes =
            std::fs::read(std::env::var("MTG_STACK_MATCH_BASELINE").unwrap()).unwrap();
        let baseline: serde_json::Value = serde_json::from_slice(&baseline_bytes).unwrap();
        assert_eq!(baseline["request"], serde_json::to_value(&request).unwrap());
        assert_eq!(
            baseline["actual_base_models"],
            serde_json::to_value(&actual).unwrap()
        );
        assert_eq!(
            baseline["decisions"],
            serde_json::to_value(&policy.decisions).unwrap()
        );
        assert_eq!(
            baseline["seed_resets"],
            serde_json::to_value(&policy.resets).unwrap()
        );
        assert_eq!(
            baseline["games"],
            serde_json::to_value(&played.games).unwrap()
        );
        assert_eq!(
            baseline["outcome"],
            serde_json::to_value(played.outcome).unwrap()
        );
        let report = serde_json::json!({"schema":"public-stack-zero-full-match-replay/v1","architecture":ARCHITECTURE,
            "status":"ENGINEERING-PASS","projection":"one stack message matrix all positive zero; both seats",
            "request_sha256":format!("{:x}",Sha256::digest(&request_bytes)),
            "baseline_sha256":format!("{:x}",Sha256::digest(&baseline_bytes)),
            "actual_base_models":actual,"outcome":played.outcome,"games":played.games,
            "seed_resets":policy.resets,"decisions":policy.decisions,
            "all_baseline_gameplay_fields_exact":true,
            "non_claim":"No training or playing-strength evidence; original model provenance plus explicit zero-projection architecture."});
        let output = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(std::env::var("MTG_STACK_MATCH_REPORT").unwrap())
            .unwrap();
        serde_json::to_writer(output, &report).unwrap();
        println!(
            "public-stack full-match pass: {} games, {} decisions",
            report["games"].as_array().unwrap().len(),
            report["decisions"].as_array().unwrap().len()
        );
    }

    fn menu(name: &str, actor: PlayerSeatV1, colors: &[ManaColor]) -> Vec<ActionSemanticV1> {
        colors
            .iter()
            .map(|color| ActionSemanticV1::ChooseEffectColor {
                actor,
                color: *color,
                source: CardStableRefV1 {
                    arena_id: 91,
                    card_db_id: crate::card_def::card_id_by_name(name).unwrap(),
                    owner: actor,
                    controller: actor,
                    zone: Zone::Hand,
                    zone_change_count: 0,
                },
            })
            .collect()
    }
    #[test]
    fn only_unique_gate_color_qualifies_independent_of_menu_order() {
        let mut actions = menu(
            "Sea Gate",
            PlayerSeatV1::P0,
            &[ManaColor::B, ManaColor::W, ManaColor::R, ManaColor::G],
        );
        let palette = [ManaColor::U, ManaColor::W];
        assert_eq!(replacement(&actions, PlayerSeatV1::P0, &palette), Some(1));
        actions.reverse();
        assert_eq!(replacement(&actions, PlayerSeatV1::P0, &palette), Some(2));
        assert_eq!(replacement(&actions, PlayerSeatV1::P1, &palette), None);
        assert_eq!(
            replacement(&actions, PlayerSeatV1::P0, &[ManaColor::W, ManaColor::R]),
            None
        );
        assert_eq!(
            replacement(
                &menu(
                    "Prismatic Strands",
                    PlayerSeatV1::P0,
                    &[ManaColor::W, ManaColor::B]
                ),
                PlayerSeatV1::P0,
                &palette
            ),
            None
        );
        actions.push(ActionSemanticV1::Pass {
            actor: PlayerSeatV1::P0,
        });
        assert_eq!(replacement(&actions, PlayerSeatV1::P0, &palette), None);
    }
    fn decision(session: &FastActorSessionV1) -> FastActorDecisionV1 {
        let FastActorResponseV1::Decision(d) = session.current_response() else {
            panic!("expected live decision")
        };
        d
    }
    fn gate_session(
        seat: PlayerId,
        hidden_variant: bool,
        gate: &str,
    ) -> (FastActorSessionV1, crate::ids::ObjectId) {
        let mut state = ready_state();
        state.active_player = seat;
        state.priority_player = seat;
        let opponent = if seat == PlayerId::P0 {
            PlayerId::P1
        } else {
            PlayerId::P0
        };
        // Change hidden identities/order and the visible Gate's arena allocation.
        let early_gate = if hidden_variant {
            Some(put(&mut state, seat, gate, Zone::Hand))
        } else {
            None
        };
        put(
            &mut state,
            opponent,
            if hidden_variant {
                "Counterspell"
            } else {
                "Lightning Bolt"
            },
            Zone::Hand,
        );
        for name in if hidden_variant {
            ["Forest", "Mountain"]
        } else {
            ["Mountain", "Forest"]
        } {
            put(&mut state, opponent, name, Zone::Library);
        }
        let id = early_gate.unwrap_or_else(|| put(&mut state, seat, gate, Zone::Hand));
        let mut session = FastActorSessionV1::from_v3_fixture_state(state);
        let d = decision(&session);
        let (_, actions) = PairedBo1PolicyInputV1::new(&session, d)
            .diagnostic_visible_v1()
            .unwrap();
        let index = actions.iter().position(|a| matches!(a, ActionSemanticV1::PlayLand { source, .. } if source.card_db_id == crate::card_def::card_id_by_name(gate).unwrap())).unwrap();
        session.step(d.episode_id, d.step, index as u32).unwrap();
        (session, id)
    }
    struct DrawPolicy {
        calls: usize,
        resets: Vec<[u64; 2]>,
        rng: SplitMix64,
    }
    impl PairedBo1PolicyV1 for DrawPolicy {
        fn uses_observation_successor_v3(&self) -> bool {
            true
        }
        fn feature_generation_v1(&self) -> PlayPolicyGenerationV1 {
            PlayPolicyGenerationV1::V4
        }
        fn reset_for_game_v1(&mut self, seeds: [u64; 2]) -> Result<(), RlSessionError> {
            self.resets.push(seeds);
            self.rng = SplitMix64::seed(seeds[0]);
            Ok(())
        }
        fn select_action_v1(
            &mut self,
            input: PairedBo1PolicyInputV1<'_>,
        ) -> Result<u32, RlSessionError> {
            self.calls += 1;
            Ok((self.rng.next_u64() % u64::from(input.decision().legal_action_count)) as u32)
        }
    }
    #[test]
    fn live_gate_binding_and_hidden_state_invariance_both_seats() {
        for seat in [PlayerId::P0, PlayerId::P1] {
            for (gate, expected) in [("Sea Gate", ManaColor::W), ("Citadel Gate", ManaColor::U)] {
                for hidden in [false, true] {
                    let (mut session, id) = gate_session(seat, hidden, gate);
                    let d = decision(&session);
                    let mut base = DrawPolicy {
                        calls: 0,
                        resets: vec![],
                        rng: SplitMix64::seed(0),
                    };
                    let mut policy = DiagnosticPolicy {
                        base: &mut base,
                        seat: seat.into(),
                        mode: Mode::UniquePrintedColor,
                        colors: vec![ManaColor::W, ManaColor::U],
                        resets: vec![],
                        decisions: vec![],
                        game_decisions: 0,
                    };
                    policy.reset_for_game_v1([123, 456]).unwrap();
                    let chosen = policy
                        .select_action_v1(PairedBo1PolicyInputV1::new(&session, d))
                        .unwrap();
                    assert!(
                        matches!(policy.decisions[0].applied_action, ActionSemanticV1::ChooseEffectColor { color, .. } if color == expected)
                    );
                    session.step(d.episode_id, d.step, chosen).unwrap();
                    assert_eq!(
                        session.game_state().objects.get(id).v4.chosen_color,
                        Some(expected)
                    );
                    assert_eq!(session.game_state().objects.get(id).zone, Zone::Battlefield);
                    assert_eq!(base.calls, 1);
                    assert_eq!(base.resets, vec![[123, 456]]);
                }
            }
        }
    }
    #[test]
    fn baseline_and_override_consume_identical_base_draws_and_resets() {
        let (session, _) = gate_session(PlayerId::P0, false, "Sea Gate");
        let d = decision(&session);
        let mut streams = Vec::new();
        for (mode, seat) in [
            (Mode::Baseline, PlayerSeatV1::P0),
            (Mode::UniquePrintedColor, PlayerSeatV1::P0),
            (Mode::UniquePrintedColor, PlayerSeatV1::P1),
        ] {
            let mut base = DrawPolicy {
                calls: 0,
                resets: vec![],
                rng: SplitMix64::seed(0),
            };
            let mut policy = DiagnosticPolicy {
                base: &mut base,
                seat,
                mode,
                colors: vec![ManaColor::W, ManaColor::U],
                resets: vec![],
                decisions: vec![],
                game_decisions: 0,
            };
            for seeds in [[101, 202], [303, 404]] {
                policy.reset_for_game_v1(seeds).unwrap();
                for _ in 0..8 {
                    policy
                        .select_action_v1(PairedBo1PolicyInputV1::new(&session, d))
                        .unwrap();
                }
            }
            let raw: Vec<u32> = policy.decisions.iter().map(|r| r.raw_index).collect();
            if mode == Mode::Baseline || seat == PlayerSeatV1::P1 {
                assert!(policy
                    .decisions
                    .iter()
                    .all(|r| r.raw_index == r.applied_index));
            } else {
                assert!(policy
                    .decisions
                    .iter()
                    .any(|r| r.raw_index != r.applied_index));
            }
            drop(policy);
            assert_eq!(base.calls, 16);
            assert_eq!(base.resets, vec![[101, 202], [303, 404]]);
            streams.push(raw);
        }
        assert_eq!(streams[0], streams[1]);
        assert_eq!(streams[0], streams[2]);
    }

    #[test]
    #[ignore = "requires an explicitly pinned external V4 source via MTG_GATE_DIAGNOSTIC_SOURCE"]
    fn actual_v4_gate_hidden_state_invariance() {
        let source_path =
            std::env::var("MTG_GATE_DIAGNOSTIC_SOURCE").expect("explicit source required");
        let source: ExpandedModelSourceV1 =
            serde_json::from_slice(&std::fs::read(source_path).unwrap()).unwrap();
        let (mut base, identity) = load_expanded_inference_v1(&source).unwrap();
        assert_eq!(base.feature_generation_v1(), PlayPolicyGenerationV1::V4);
        eprintln!(
            "actual V4 state {} Adam {}",
            identity.state_sha256, identity.adam_step
        );
        for seat in [PlayerId::P0, PlayerId::P1] {
            for gate in ["Sea Gate", "Citadel Gate"] {
                let mut choices = Vec::new();
                for hidden in [false, true] {
                    let (session, _) = gate_session(seat, hidden, gate);
                    let d = decision(&session);
                    let mut policy = DiagnosticPolicy {
                        base: &mut base,
                        seat: seat.into(),
                        mode: Mode::UniquePrintedColor,
                        colors: vec![ManaColor::W, ManaColor::U],
                        resets: vec![],
                        decisions: vec![],
                        game_decisions: 0,
                    };
                    policy.reset_for_game_v1([123, 456]).unwrap();
                    policy
                        .select_action_v1(PairedBo1PolicyInputV1::new(&session, d))
                        .unwrap();
                    let raw = &policy.decisions[0].raw_action;
                    let applied = &policy.decisions[0].applied_action;
                    let color = |action: &ActionSemanticV1| match action {
                        ActionSemanticV1::ChooseEffectColor { color, .. } => *color,
                        _ => panic!("expected color"),
                    };
                    choices.push((color(raw), color(applied)));
                }
                assert_eq!(choices[0], choices[1]);
            }
        }
    }
}
