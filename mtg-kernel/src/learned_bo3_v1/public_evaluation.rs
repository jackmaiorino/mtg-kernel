//! Explicit successor evaluation through the existing BO3 session runner.
//! This module never constructs a GPU device, updates weights or emits training data.
use super::*;
use crate::durable_publication_v1::{
    capture_existing_publication_parent_v1, publish_new_file_v1, DurableFileExpectationV1,
};
use crate::expanded_deck_training_v1::{
    load_expanded_inference_v1, ExpandedDeckListV1, ExpandedModelSourceV1, PinnedFileV1,
};
use crate::learned_sideboard_v1::actions_between_configurations_v1;
use crate::native_policy_value_net_v1::public_inputs_v1::{PublicInputWeightsV1, ARCHITECTURE};
use crate::paired_bo1_harness_v1::PlayPolicyGenerationV1;
use crate::sideboard_play_policy_v1::public_inputs::{
    select_forced_v3_for_evaluation, select_spell_adapter_v3_for_evaluation,
    PublicInputPlayPolicyV1,
};
use crate::native_policy_value_net_v1::stack_inputs_v1::{StackInputWeightsV1, ARCHITECTURE as STACK_ARCHITECTURE};
use crate::public_stack_features_v1::StackInputModeV1;
use crate::sideboard_play_policy_v1::stack_inputs::StackInputPlayPolicyV1;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ModelSource {
    StackWarmStart { source: ExpandedModelSourceV1, input_mode: StackInputModeV1 },
    StackCheckpoint { config: PinnedFileV1, checkpoint: PinnedFileV1 },
    Legacy {
        source: ExpandedModelSourceV1,
        v3_forced_actions: bool,
        #[serde(default, skip_serializing_if = "is_false")]
        v3_spell_target_reference_adapter: bool,
    },
    PublicWarmStart {
        source: ExpandedModelSourceV1,
    },
    PublicCheckpoint {
        config: PinnedFileV1,
        checkpoint: PinnedFileV1,
    },
}

fn is_false(value: &bool) -> bool {
    !value
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Match {
    pub config: LearnedBo3RunConfigV1,
    pub registered: [ExpandedDeckListV1; 2],
    /// Explicit fixed game-two configurations, carried into game three.
    /// None keeps the registered configuration for every game.
    pub postboard: Option<[ExpandedDeckListV1; 2]>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub sources: [ModelSource; 2],
    pub matches: Vec<Match>,
    pub cross_generation_evaluation: bool,
    pub capture_decisions: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub terminal_audit_v1: bool,
    pub output_directory: PathBuf,
}

enum Play {
    Legacy {
        policy: FrozenPlayPolicyV1,
        forced: bool,
        count: u64,
        spell_adapter: bool,
        repairs: u64,
    },
    Public(PublicInputPlayPolicyV1),
    Stack(StackInputPlayPolicyV1),
}

impl Play {
    fn base(&mut self) -> &mut dyn PairedBo1PolicyV1 {
        match self {
            Self::Legacy { policy, .. } => policy,
            Self::Public(policy) => policy,
            Self::Stack(policy) => policy,
        }
    }
    fn forced_count(&self) -> u64 {
        match self {
            Self::Legacy { count, .. } => *count,
            _ => 0,
        }
    }
    fn repair_count(&self) -> u64 {
        match self {
            Self::Legacy { repairs, .. } => *repairs,
            _ => 0,
        }
    }
}

impl PairedBo1PolicyV1 for Play {
    fn uses_observation_successor_v3(&self) -> bool {
        true
    }
    fn feature_generation_v1(&self) -> PlayPolicyGenerationV1 {
        match self {
            Self::Legacy { policy, .. } => policy.feature_generation_v1(),
            Self::Public(policy) => policy.feature_generation_v1(),
            Self::Stack(policy) => policy.feature_generation_v1(),
        }
    }
    fn reset_for_game_v1(&mut self, seeds: [u64; 2]) -> Result<(), RlSessionError> {
        self.base().reset_for_game_v1(seeds)
    }
    fn select_action_v1(
        &mut self,
        input: PairedBo1PolicyInputV1<'_>,
    ) -> Result<u32, RlSessionError> {
        if let Self::Legacy {
            policy,
            forced: true,
            count,
            ..
        } = self
        {
            if input.decision().legal_action_count == 1 {
                let selected = select_forced_v3_for_evaluation(policy, &input)?;
                *count += 1;
                return Ok(selected);
            }
        }
        if let Self::Legacy {
            policy,
            spell_adapter: true,
            repairs,
            ..
        } = self
        {
            let (selected, _, repaired) = select_spell_adapter_v3_for_evaluation(policy, &input)?;
            *repairs += u64::from(repaired);
            return Ok(selected);
        }
        self.base().select_action_v1(input)
    }
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn load(source: &ModelSource) -> Result<(Play, Value), String> {
    match source {
        ModelSource::Legacy {
            source,
            v3_forced_actions,
            v3_spell_target_reference_adapter,
        } => {
            let (policy, identity) = load_expanded_inference_v1(source)?;
            if !policy.uses_observation_successor_v3()
                || ((*v3_forced_actions || *v3_spell_target_reference_adapter)
                    && policy.feature_generation_v1() != PlayPolicyGenerationV1::V3)
            {
                return Err(
                    "legacy evaluation requires V3/V4; authority adapters require V3".into(),
                );
            }
            let receipt = json!({"schema":"legacy-evaluation-model/v1","weights_sha256":identity.model.weights_sha256,
                "identity":identity,"v3_forced_actions":v3_forced_actions,
                "v3_spell_target_reference_adapter":v3_spell_target_reference_adapter});
            Ok((
                Play::Legacy {
                    policy,
                    forced: *v3_forced_actions,
                    count: 0,
                    spell_adapter: *v3_spell_target_reference_adapter,
                    repairs: 0,
                },
                receipt,
            ))
        }
        ModelSource::StackWarmStart { source, input_mode } => {
            let (base, identity) = load_expanded_inference_v1(source)?;
            let weights = StackInputWeightsV1::zero();
            let weights_hash = hash(&serde_json::to_vec(&(STACK_ARCHITECTURE,
                &identity.model.weights_sha256, weights.values.iter().map(|v|v.to_bits()).collect::<Vec<_>>(), input_mode)).map_err(error)?);
            let receipt=json!({"schema":"public-stack-warm-start-model/v1","architecture":STACK_ARCHITECTURE,
                "weights_sha256":weights_hash,"initial_base":identity,"input_mode":input_mode,"stack_adam_step":0});
            Ok((Play::Stack(StackInputPlayPolicyV1::new(base,weights)?.with_mode(*input_mode)),receipt))
        }
        ModelSource::StackCheckpoint { config, checkpoint } => {
            let (policy,identity)=crate::expanded_deck_training_v1::stack_features::load_for_evaluation(config,checkpoint)?;
            Ok((Play::Stack(policy),identity))
        }
        ModelSource::PublicWarmStart { source } => {
            let (base, identity) = load_expanded_inference_v1(source)?;
            let weights_hash = hash(
                &serde_json::to_vec(&(
                    ARCHITECTURE,
                    &identity.model.weights_sha256,
                    vec![0u32; 2048],
                    vec![0u32; 384],
                    true,
                ))
                .map_err(error)?,
            );
            let receipt = json!({"schema":"public-input-warm-start-model/v1","architecture":ARCHITECTURE,
                "weights_sha256":weights_hash,"initial_base":identity,"inputs_enabled":true,"public_adam_step":0});
            Ok((
                Play::Public(PublicInputPlayPolicyV1::new(
                    base,
                    PublicInputWeightsV1::zero(),
                )?),
                receipt,
            ))
        }
        ModelSource::PublicCheckpoint { config, checkpoint } => {
            let (policy, identity) =
                crate::expanded_deck_training_v1::public_features::load_for_evaluation(
                    config, checkpoint,
                )?;
            Ok((Play::Public(policy), identity))
        }
    }
}

struct Trace<'a> {
    base: &'a mut dyn PairedBo1PolicyV1,
    capture: bool,
    spell_adapter: [bool; 2],
    generations: [PlayPolicyGenerationV1; 2],
    diagnostic_repairs: [u64; 2],
    resets: Vec<[u64; 2]>,
    rows: Vec<Value>,
    decisions: u64,
    terminal_audit: bool,
    terminal_counts: std::collections::BTreeMap<String,u64>,
    terminal_roots: Vec<Value>,
    terminal_branches: u64,
}
impl PairedBo1PolicyV1 for Trace<'_> {
    fn uses_observation_successor_v3(&self) -> bool {
        self.base.uses_observation_successor_v3()
    }
    fn feature_generation_v1(&self) -> PlayPolicyGenerationV1 {
        self.base.feature_generation_v1()
    }
    fn reset_for_game_v1(&mut self, seeds: [u64; 2]) -> Result<(), RlSessionError> {
        self.base.reset_for_game_v1(seeds)?;
        self.resets.push(seeds);
        Ok(())
    }
    fn select_action_v1(
        &mut self,
        input: PairedBo1PolicyInputV1<'_>,
    ) -> Result<u32, RlSessionError> {
        let fail = |message: String| RlSessionError {
            code: crate::rl_session::RlSessionErrorCode::StaleEnvironmentBinding,
            message,
        };
        let root=input.decision();
        let acting=usize::from(root.acting_player==crate::rl::PlayerSeatV1::P1);
        let mut terminal_row=if self.terminal_audit && self.generations[acting]==PlayPolicyGenerationV1::V4 {
            let record=input.diagnostic_terminal_targets_v1().map_err(fail)?;
            let status=record["status"].as_str().ok_or_else(||fail("shadow status missing".into()))?;
            *self.terminal_counts.entry(status.into()).or_default()+=1;
            if status=="audited" {
                self.terminal_branches+=u64::from(root.legal_action_count);
                if self.terminal_roots.len()>=512 || self.terminal_branches>4096 {return Err(fail("shadow per-match budget exceeded".into()));}
                Some(json!({"game_index":self.resets.len(),"step":root.step,"physical_decision_id":root.physical_decision_id,
                    "actor":root.acting_player,"audit":record}))
            } else {None}
        } else {None};
        let mut row = if self.capture {
            let seat = usize::from(input.decision().acting_player == crate::rl::PlayerSeatV1::P1);
            let diagnostic_error = |error: String| fail(format!(
                "diagnostic seat={seat} generation={:?} decision={:?}: {error}",
                self.generations[seat], input.decision()));
            let (observation, actions) = if self.generations[seat] == PlayPolicyGenerationV1::V4 {
                input.diagnostic_visible_v4().map_err(diagnostic_error)?
            } else if self.spell_adapter[seat] {
                let (observation, actions, repaired) =
                    input.diagnostic_visible_spell_adapter_v1().map_err(diagnostic_error)?;
                self.diagnostic_repairs[seat] += u64::from(repaired);
                (observation, actions)
            } else {
                input.diagnostic_visible_v1().map_err(diagnostic_error)?
            };
            let visible =
                hash(&serde_json::to_vec(&(&observation, &actions)).map_err(|e| fail(error(e)))?);
            let d = input.decision();
            Some(
                json!({"game_index":self.resets.len(),"step":d.step,"physical_decision_id":d.physical_decision_id,
                "substep_index":d.substep_index,"actor":d.acting_player,"visible_sha256":visible,"legal_action_count":actions.len()}),
            )
        } else {
            None
        };
        let legal = input.decision().legal_action_count;
        let selected = self.base.select_action_v1(input)?;
        if selected >= legal {
            return Err(fail("evaluation selected outside legal menu".into()));
        }
        if let Some(ref mut r)=terminal_row {
            r["selected"]=json!(selected);
            self.terminal_roots.push(r.take());
        }
        self.decisions += 1;
        if let Some(ref mut r) = row {
            r["selected"] = json!(selected);
            self.rows.push(r.take());
        }
        Ok(selected)
    }
}

fn prepare(item: &Match) -> Result<([RegisteredDeckV1; 2], [DeckConfigurationV1; 2]), String> {
    validate_run_limits_v1(&item.config)?;
    if item.config.game_one_chooser.0 > 1
        || item.config.max_physical_games > 32
        || item.config.max_physical_decisions > 10000
        || item.config.max_policy_steps > 1_000_000
        || item.config.opening_protocol != Bo3OpeningProtocolV1::KeepSevenV2
    {
        return Err("public evaluation requires bounded KeepSevenV2 matches".into());
    }
    let build = |seat: usize| -> Result<_, String> {
        let deck = &item.registered[seat];
        if deck.label != item.config.deck_ids[seat] {
            return Err("registration label differs".into());
        }
        let registered = RegisteredDeckV1::new_executable_v1(
            &deck.label,
            deck.mainboard.clone(),
            deck.sideboard.clone(),
        )
        .map_err(error)?;
        let selected = if let Some(postboard) = &item.postboard {
            let target = &postboard[seat];
            if target.label != deck.label {
                return Err("postboard label differs".into());
            }
            let target = DeckConfigurationV1::new_exact_v1(
                target.mainboard.clone(),
                target.sideboard.clone(),
            )
            .map_err(error)?;
            actions_between_configurations_v1(registered.registered_configuration(), &target)
                .map_err(error)?;
            target
        } else {
            registered.registered_configuration().clone()
        };
        Ok((registered, selected))
    };
    let (r0, t0) = build(0)?;
    let (r1, t1) = build(1)?;
    Ok(([r0, r1], [t0, t1]))
}

fn tags() -> Result<RemovalCounterspellTagsV1, String> {
    let doc: Value = serde_json::from_slice(include_bytes!(
        "../../../data/pauper_removal_counterspell_tags_v1.json"
    ))
    .map_err(error)?;
    if doc["schema"] != "kernel_removal_counterspell_tags/v1" {
        return Err("tag schema differs".into());
    }
    let mut tags = RemovalCounterspellTagsV1 {
        requires_target: Default::default(),
        is_counterspell: Default::default(),
    };
    for row in doc["cards"].as_array().ok_or("missing tag rows")? {
        let id = u16::try_from(row["card_id"].as_u64().ok_or("invalid tag id")?).map_err(error)?;
        if row["requires_target"]
            .as_bool()
            .ok_or("invalid target tag")?
        {
            tags.requires_target.insert(id);
        }
        if row["is_counterspell"]
            .as_bool()
            .ok_or("invalid counterspell tag")?
        {
            tags.is_counterspell.insert(id);
        }
    }
    Ok(tags)
}
fn save(directory: &Path, name: &str, value: &Value) -> Result<String, String> {
    let bytes = serde_json::to_vec(value).map_err(error)?;
    let parent = capture_existing_publication_parent_v1(directory).map_err(error)?;
    publish_new_file_v1(
        &parent,
        format!(".{name}.stage"),
        name,
        &bytes,
        DurableFileExpectationV1::from_bytes(&bytes).map_err(error)?,
    )
    .map_err(error)?;
    Ok(hash(&bytes))
}

pub fn run(command: Command) -> Result<Value, String> {
    if command.matches.is_empty()
        || command.matches.len() > 1024
        || !command.output_directory.is_absolute()
    {
        return Err(
            "evaluation requires 1..1024 matches and absolute fresh output directory".into(),
        );
    }
    let prepared = command
        .matches
        .iter()
        .map(prepare)
        .collect::<Result<Vec<_>, _>>()?;
    let [(mut p0, i0), (mut p1, i1)] = [load(&command.sources[0])?, load(&command.sources[1])?];
    let identities = [i0, i1];
    let generation_ids = [p0.feature_generation_v1(), p1.feature_generation_v1()];
    let generations = generation_ids.map(feature_generation_label_v1);
    if !command.cross_generation_evaluation && generations[0] != generations[1] {
        return Err("cross-generation evaluation requires explicit opt-in".into());
    }
    let weights = [
        identities[0]["weights_sha256"]
            .as_str()
            .ok_or("missing seat0 model hash")?,
        identities[1]["weights_sha256"]
            .as_str()
            .ok_or("missing seat1 model hash")?,
    ];
    let tags = tags()?;
    std::fs::create_dir(&command.output_directory).map_err(error)?;
    save(
        &command.output_directory,
        "start.json",
        &json!({"schema":"public-input-evaluation-start/v1","command":command,"models":identities,
        "git_head":env!("MTG_KERNEL_BUILD_GIT_HEAD"),"execution":"cpu","training":false}),
    )?;
    let execution = (|| -> Result<Value, String> {
        let mut hashes = Vec::new();
        let mut games = 0;
        let mut decisions = 0;
        for (index, (item, (registered, targets))) in
            command.matches.iter().zip(prepared).enumerate()
        {
            let registrations = registered
                .each_ref()
                .map(LearnedBo3RegistrationRecordV1::from_registered_v1);
            let session =
                BestOfThreeDeckMatchV1::new_live_v1(registered, item.config.game_one_chooser)
                    .map_err(error)?;
            let before = [p0.forced_count(), p1.forced_count()];
            let repairs_before = [p0.repair_count(), p1.repair_count()];
            let spell_adapter = command.sources.each_ref().map(|source| matches!(source,
                ModelSource::Legacy { v3_spell_target_reference_adapter: true, .. }));
            let mut router = if command.cross_generation_evaluation {
                SeatRoutedBo3PlayPolicyV1::new_cross_generation_evaluation_v1([&mut p0, &mut p1])?
            } else {
                SeatRoutedBo3PlayPolicyV1::new_v1([&mut p0, &mut p1])?
            };
            let mut trace = Trace {
                base: &mut router,
                capture: command.capture_decisions,
                spell_adapter,
                generations: generation_ids,
                diagnostic_repairs: [0; 2],
                resets: vec![],
                rows: vec![],
                decisions: 0,
                terminal_audit: command.terminal_audit_v1,
                terminal_counts: Default::default(),
                terminal_roots: vec![],
                terminal_branches: 0,
            };
            let fixed = |seat: usize| {
                let target = targets[seat].clone();
                move |_: &LearnedSideboardInputV1, current: &DeckConfigurationV1| {
                    let actions =
                        actions_between_configurations_v1(current, &target).map_err(error)?;
                    Ok((target.clone(), actions))
                }
            };
            let mut s0 = fixed(0);
            let mut s1 = fixed(1);
            let played = run_learned_bo3_session_v1(
                item.config.clone(),
                session,
                Bo3ModelProvenanceV1::PerSeat(weights),
                &tags,
                &mut trace,
                [&mut s0, &mut s1],
            )?;
            games += played.games.len();
            decisions += trace.decisions;
            let mut result = json!({"schema":"public-input-evaluation-match/v1","match":item,"models":identities,
                "registrations":registrations,"seat_generations":generations,"cross_generation_evaluation":command.cross_generation_evaluation,
                "outcome":played.outcome,"games":played.games,"sideboard_decisions":played.sideboard_decisions,
                "seed_resets":trace.resets,"decision_count":trace.decisions,"decisions":trace.rows,
                "diagnostic_spell_target_repairs":trace.diagnostic_repairs});
            if command.terminal_audit_v1 {
                result["terminal_audit_v1"]=json!({"counts":trace.terminal_counts,"branches":trace.terminal_branches,"roots":trace.terminal_roots});
            }
            drop(trace);
            drop(router);
            result["v3_forced_actions"] =
                json!([p0.forced_count() - before[0], p1.forced_count() - before[1]]);
            result["v3_spell_target_repairs"] = json!([
                p0.repair_count() - repairs_before[0],
                p1.repair_count() - repairs_before[1]
            ]);
            hashes.push(save(
                &command.output_directory,
                &format!("match-{index:06}.json"),
                &result,
            )?);
        }
        Ok(
            json!({"schema":"public-input-evaluation-completion/v1","matches":hashes.len(),"natural_games":games,"decisions":decisions,"match_sha256":hashes,
            "non_claim":"Development evaluation; no human-strength or promotion claim from engineering qualification."}),
        )
    })();
    match execution {
        Ok(result) => {
            save(&command.output_directory, "completion.json", &result)?;
            Ok(result)
        }
        Err(message) => {
            save(
                &command.output_directory,
                "failure.json",
                &json!({"error":message}),
            )?;
            Err(message)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v3_spell_target_trace_requires_opt_in_and_preserves_selection() {
        use crate::paired_bo1_harness_v1::PairedBo1PolicyInputV1;
        use crate::rl_session::{FastActorSessionV1, FastActorResponseV1};
        let (state, _, _) = crate::rl_session::pyroblast_target_fixture_v1();
        let session = FastActorSessionV1::from_v3_fixture_state(state);
        let response = session.current_response();
        let FastActorResponseV1::Decision(decision) = response else { panic!() };
        let mut base = Play::Legacy { policy: FrozenPlayPolicyV1::training_fixture_v3(),
            forced: true, count: 0, spell_adapter: true, repairs: 0 };
        let mut reference = FrozenPlayPolicyV1::training_fixture_v3();
        reference.reset_sampling_v1([123, 456]);
        let mut trace = Trace { base: &mut base, capture: true, spell_adapter: [false; 2],
            generations: [PlayPolicyGenerationV1::V3; 2],
            diagnostic_repairs: [0; 2], resets: vec![], rows: vec![], decisions: 0,
            terminal_audit:false, terminal_counts:Default::default(), terminal_roots:vec![],terminal_branches:0 };
        trace.reset_for_game_v1([123, 456]).unwrap();
        assert!(trace.select_action_v1(PairedBo1PolicyInputV1::new(&session, decision)).is_err());
        assert_eq!(trace.decisions, 0);
        trace.spell_adapter = [true, false];
        for _ in 0..8 {
            let expected = select_spell_adapter_v3_for_evaluation(&mut reference,
                &PairedBo1PolicyInputV1::new(&session, decision)).unwrap().0;
            assert_eq!(trace.select_action_v1(PairedBo1PolicyInputV1::new(&session, decision)).unwrap(), expected);
        }
        assert_eq!(trace.diagnostic_repairs, [8, 0]);
        assert_eq!(trace.rows.len(), 8);
        assert_eq!(trace.decisions, 8);
        assert_eq!(session.current_response(), response);
        drop(trace);
        assert_eq!(base.repair_count(), 8);
        let mut v4 = FrozenPlayPolicyV1::training_fixture_v4();
        let mut reference = FrozenPlayPolicyV1::training_fixture_v4();
        reference.reset_sampling_v1([123, 456]);
        let mut trace = Trace { base: &mut v4, capture: true, spell_adapter: [false; 2],
            generations: [PlayPolicyGenerationV1::V4; 2],
            diagnostic_repairs: [0; 2], resets: vec![], rows: vec![], decisions: 0,
            terminal_audit:false, terminal_counts:Default::default(), terminal_roots:vec![],terminal_branches:0 };
        trace.reset_for_game_v1([123, 456]).unwrap();
        for _ in 0..8 {
            let expected = reference.select_action_v1(PairedBo1PolicyInputV1::new(&session, decision)).unwrap();
            assert_eq!(trace.select_action_v1(PairedBo1PolicyInputV1::new(&session, decision)).unwrap(), expected);
        }
        assert_eq!(trace.rows.len(), 8);
        assert_eq!(trace.diagnostic_repairs, [0; 2]);
        assert_eq!(session.current_response(), response);
    }

    #[test]
    fn postboard_preflight_rejects_changed_registration_and_unbounded_match() {
        let registered = ["Burn", "Affinity"].map(|name| {
            let deck = crate::sideboard::checked_in_pauper_registered_deck_by_id_v1(name).unwrap();
            ExpandedDeckListV1 {
                label: name.into(),
                mainboard: deck.registered_configuration().mainboard().to_vec(),
                sideboard: deck.registered_configuration().sideboard().to_vec(),
            }
        });
        let item = Match {
            config: LearnedBo3RunConfigV1 {
                deck_ids: ["Burn".into(), "Affinity".into()],
                seed: 1,
                game_one_chooser: PlayerId::P0,
                max_physical_games: 3,
                max_physical_decisions: 4000,
                max_policy_steps: 40000,
                opening_protocol: Bo3OpeningProtocolV1::KeepSevenV2,
            },
            registered: registered.clone(),
            postboard: Some(registered),
        };
        prepare(&item).unwrap();
        let mut changed = item.clone();
        changed.postboard.as_mut().unwrap()[0].sideboard.pop();
        assert!(prepare(&changed).is_err());
        changed = item.clone();
        changed.postboard.as_mut().unwrap()[0].label = "wrong".into();
        assert!(prepare(&changed).is_err());
        changed = item.clone();
        changed.config.max_physical_games = 33;
        assert!(prepare(&changed).is_err());
        changed = item;
        changed.config.opening_protocol = Bo3OpeningProtocolV1::LegacyKeepSevenV1;
        assert!(prepare(&changed).is_err());
    }
}
