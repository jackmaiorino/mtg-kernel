//! JSON-config entry point for frozen-play, visible-input sideboarding research.

use mtg_kernel::expanded_deck_training_v1::{
    load_expanded_inference_v1, ExpandedDeckListV1, ExpandedModelSourceV1,
};
use mtg_kernel::game_summary_v1::RemovalCounterspellTagsV1;
use mtg_kernel::learned_bo3_v1::{
    run_learned_bo3_v1, run_learned_bo3_with_registrations_v1, LearnedBo3RunConfigV1,
    SideboardSelectionV1, VisibleSideboardPolicyV1,
};
use mtg_kernel::learned_sideboard_v1::{
    actions_between_configurations_v1, FrozenSideboardEmbeddingsV1, LearnedSideboardInputV1,
    LearnedSideboardModelV1, SideboardActionV1, SideboardImitationExampleV1,
    SideboardPlayIdentityV1, SideboardTrainingConfigV1,
};
use mtg_kernel::phase1_v4_decision_search_v1::EndSeatSearchRequestV1;
use mtg_kernel::sideboard::{
    checked_in_pauper_registered_deck_by_id_v1, CardCountV1, DeckConfigurationV1, RegisteredDeckV1,
    SideboardPlanV1,
};
use mtg_kernel::sideboard_play_policy_v1::{
    FrozenPlayObservationTransferV3, FrozenPlayPolicyImportV1, FrozenPlayPolicyV1,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const TAG_BYTES: &[u8] = include_bytes!("../../../data/pauper_removal_counterspell_tags_v1.json");
const MAX_JSON_BYTES: usize = 16 * 1024 * 1024;
const MAX_EXAMPLE_BYTES: usize = 128 * 1024 * 1024;

#[path = "learned_sideboard_v1/evaluation.rs"]
mod evaluation;
#[path = "learned_sideboard_v1/population.rs"]
mod population;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PinnedFileV1 {
    path: PathBuf,
    sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TeacherSourceKindV1 {
    HandAuthoredWarmStart,
    SearchGenerated,
    PolicyImitation,
}

/// These are declared teacher provenance facts, not a ratification certificate.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TeacherProvenanceV1 {
    source_kind: TeacherSourceKindV1,
    description: String,
    artifact: PinnedFileV1,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum SeatPolicyV1 {
    Keep,
    Learned {
        checkpoint: PinnedFileV1,
    },
    StaticPlanRows {
        table: PinnedFileV1,
        teacher_provenance: TeacherProvenanceV1,
        carry_game_three_forward: bool,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum InitializationV1 {
    Fresh { seed: u64 },
    Checkpoint { checkpoint: PinnedFileV1 },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpandedBo3MatchV1 {
    config: LearnedBo3RunConfigV1,
    registered: [ExpandedDeckListV1; 2],
}

impl ExpandedBo3MatchV1 {
    fn registrations(&self) -> Result<[RegisteredDeckV1; 2], String> {
        validate_match(&self.config)?;
        let build = |seat: usize| {
            let list = &self.registered[seat];
            if list.label != self.config.deck_ids[seat] {
                return Err(format!(
                    "seat {seat} registration label differs from match metadata"
                ));
            }
            RegisteredDeckV1::new_executable_v1(
                &list.label,
                list.mainboard.clone(),
                list.sideboard.clone(),
            )
            .map_err(|e| e.to_string())
        };
        Ok([build(0)?, build(1)?])
    }
}

/// `skip_serializing_if` predicate for a plain `bool` field that defaults to
/// `false`, so a config that never set it keeps an unchanged wire shape.
fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
enum CommandV1 {
    ValidateImport {
        play_import: PathBuf,
        output_directory: PathBuf,
    },
    RunBatch {
        play_import: PathBuf,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        play_observation_transfer_v3: Option<FrozenPlayObservationTransferV3>,
        output_directory: PathBuf,
        policies: [SeatPolicyV1; 2],
        matches: Vec<LearnedBo3RunConfigV1>,
        collect_static_imitation_examples: bool,
    },
    RunExpandedBatch {
        model_source: ExpandedModelSourceV1,
        output_directory: PathBuf,
        policies: [SeatPolicyV1; 2],
        matches: Vec<ExpandedBo3MatchV1>,
    },
    RunPopulationBatch {
        model_sources: [ExpandedModelSourceV1; 2],
        output_directory: PathBuf,
        policies: [SeatPolicyV1; 2],
        matches: Vec<ExpandedBo3MatchV1>,
        /// Evaluation-only opt-in (design doc section 6): lets the two seats
        /// carry different feature generations (for example a V3 incumbent
        /// against a fresh V4 checkpoint), each still scoring its own
        /// decisions with its own generation's encoder and policy. Defaults
        /// to `false` and is omitted from the wire at that default, so every
        /// existing config keeps decoding and behaving exactly as before:
        /// a mismatched seat pair is still rejected.
        #[serde(default, skip_serializing_if = "is_false")]
        cross_generation_evaluation: bool,
        /// Optional decision-time search wrapper (design ruling 2026-09-01)
        /// for exactly one seat's play policy: the "end" seat under
        /// evaluation in a yardstick read. Additive and config-driven;
        /// omitted (the default) keeps every existing config's wire shape
        /// and behavior byte-identical. See `phase1_v4_decision_search_v1`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        end_seat_search: Option<EndSeatSearchRequestV1>,
    },
    TrainImitation {
        play_import: PathBuf,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expanded_model_source: Option<ExpandedModelSourceV1>,
        output_directory: PathBuf,
        examples: PinnedFileV1,
        teacher_provenance: TeacherProvenanceV1,
        measured_value_provenance: Option<PinnedFileV1>,
        initialization: InitializationV1,
        training: SideboardTrainingConfigV1,
    },
    EvaluateImitation {
        play_import: PathBuf,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expanded_model_source: Option<ExpandedModelSourceV1>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        example_embedding_transfer: Option<evaluation::ImitationEmbeddingTransferV1>,
        output_directory: PathBuf,
        checkpoint: PinnedFileV1,
        dataset_inventory: PinnedFileV1,
        split: evaluation::EvaluationSplitV1,
    },
}

impl CommandV1 {
    fn expanded_source(&self) -> Result<Option<&ExpandedModelSourceV1>, String> {
        match self {
            Self::RunExpandedBatch { model_source, .. } => Ok(Some(model_source)),
            Self::TrainImitation {
                play_import,
                expanded_model_source: Some(source),
                ..
            }
            | Self::EvaluateImitation {
                play_import,
                expanded_model_source: Some(source),
                ..
            } => {
                if play_import != &source.play_import.path {
                    return Err(
                        "expanded model source and play_import must name the same import".into(),
                    );
                }
                Ok(Some(source))
            }
            _ => Ok(None),
        }
    }

    fn paths(&self) -> (&Path, &Path) {
        match self {
            Self::RunPopulationBatch {
                model_sources,
                output_directory,
                ..
            } => (&model_sources[0].play_import.path, output_directory),
            Self::RunExpandedBatch {
                model_source,
                output_directory,
                ..
            } => (&model_source.play_import.path, output_directory),
            Self::ValidateImport {
                play_import,
                output_directory,
            }
            | Self::RunBatch {
                play_import,
                output_directory,
                ..
            }
            | Self::TrainImitation {
                play_import,
                output_directory,
                ..
            }
            | Self::EvaluateImitation {
                play_import,
                output_directory,
                ..
            } => (play_import, output_directory),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StaticRowV1 {
    self_deck_id: String,
    opponent_deck_id: String,
    game_index: u8,
    cards_in: Vec<CardCountV1>,
    cards_out: Vec<CardCountV1>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StaticRowsV1 {
    rows: Vec<StaticRowV1>,
}

enum PreparedPolicyV1 {
    Keep,
    Learned(LearnedSideboardModelV1),
    Static {
        rows: Vec<StaticRowV1>,
        table_sha256: [u8; 32],
        carry: bool,
    },
}

/// Each seat receives a distinct bound policy. Only the static teacher binder
/// sees a matchup key; no such key enters the learned policy or its input.
enum BoundPolicyV1<'a> {
    Keep,
    Learned {
        model: LearnedSideboardModelV1,
        embeddings: &'a FrozenSideboardEmbeddingsV1<'a>,
    },
    Static {
        targets: BTreeMap<u8, DeckConfigurationV1>,
    },
}

impl VisibleSideboardPolicyV1 for BoundPolicyV1<'_> {
    fn select_v1(
        &mut self,
        input: &LearnedSideboardInputV1,
        current: &DeckConfigurationV1,
    ) -> Result<SideboardSelectionV1, String> {
        match self {
            Self::Keep => Ok((current.clone(), vec![SideboardActionV1::Done])),
            Self::Learned { model, embeddings } => {
                let result = model
                    .deliberate_v1(input, current, embeddings)
                    .map_err(|e| e.to_string())?;
                Ok((result.configuration, result.actions))
            }
            Self::Static { targets } => {
                let target = targets
                    .get(&input.next_game_number)
                    .ok_or("no explicitly bound static row for this game")?;
                let actions = actions_between_configurations_v1(current, target)
                    .map_err(|e| e.to_string())?;
                Ok((target.clone(), actions))
            }
        }
    }
}

impl PreparedPolicyV1 {
    fn bind_explicit<'a>(
        &self,
        config: &LearnedBo3RunConfigV1,
        registered: &[RegisteredDeckV1; 2],
        seat: usize,
        embeddings: &'a FrozenSideboardEmbeddingsV1<'a>,
    ) -> Result<BoundPolicyV1<'a>, String> {
        // Existing teacher rows describe a particular canonical matchup.
        // Verify both actual 75s before using its label-indexed table.
        if matches!(self, Self::Static { .. }) {
            for (index, actual) in registered.iter().enumerate() {
                let canonical = checked_in_pauper_registered_deck_by_id_v1(&config.deck_ids[index])
                    .map_err(|_| {
                        "static teacher requires canonical registrations for both seats".to_owned()
                    })?;
                if canonical.registered_configuration() != actual.registered_configuration() {
                    return Err(format!(
                        "static teacher is not bound to seat {index}'s explicit registration"
                    ));
                }
            }
        }
        self.bind(config, seat, embeddings)
    }

    fn bind<'a>(
        &self,
        config: &LearnedBo3RunConfigV1,
        seat: usize,
        embeddings: &'a FrozenSideboardEmbeddingsV1<'a>,
    ) -> Result<BoundPolicyV1<'a>, String> {
        match self {
            Self::Keep => Ok(BoundPolicyV1::Keep),
            Self::Learned(model) => {
                model
                    .validate_frozen_embeddings_v1(embeddings)
                    .map_err(|e| format!("seat {seat} learned checkpoint preflight: {e}"))?;
                Ok(BoundPolicyV1::Learned {
                    model: model.clone(),
                    embeddings,
                })
            }
            Self::Static {
                rows,
                table_sha256,
                carry,
            } => {
                let registered = checked_in_pauper_registered_deck_by_id_v1(&config.deck_ids[seat])
                    .map_err(|e| e.to_string())?;
                let mut targets = BTreeMap::new();
                for game in 2..=config.max_physical_games {
                    let key = if *carry && game >= 4 { 3 } else { game };
                    let matching: Vec<_> = rows
                        .iter()
                        .filter(|row| {
                            row.self_deck_id == config.deck_ids[seat]
                                && row.opponent_deck_id == config.deck_ids[1 - seat]
                                && row.game_index == key
                        })
                        .collect();
                    if matching.len() != 1 {
                        return Err(format!(
                            "static table requires exactly one row: {} vs {}, game {key}; found {}",
                            config.deck_ids[seat],
                            config.deck_ids[1 - seat],
                            matching.len()
                        ));
                    }
                    let row = matching[0];
                    let plan = SideboardPlanV1::new_v1(
                        &row.self_deck_id,
                        &row.opponent_deck_id,
                        key,
                        row.cards_in.clone(),
                        row.cards_out.clone(),
                    )
                    .map_err(|e| e.to_string())?;
                    let (target, _) = registered
                        .apply_plan_v1(&plan, "explicit-research-teacher-v1", *table_sha256)
                        .map_err(|e| e.to_string())?;
                    targets.insert(game, target);
                }
                Ok(BoundPolicyV1::Static { targets })
            }
        }
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("learned_sideboard_v1: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    let flag = args
        .next()
        .ok_or("usage: learned_sideboard_v1 --config ABSOLUTE_CONFIG.json")?;
    if flag != "--config" {
        return Err("usage: learned_sideboard_v1 --config ABSOLUTE_CONFIG.json".into());
    }
    let path = PathBuf::from(args.next().ok_or("--config requires a path")?);
    if args.next().is_some() {
        return Err("unexpected extra arguments".into());
    }
    absolute(&path)?;
    let config_bytes = read_bounded(&path, MAX_JSON_BYTES)?;
    let command: CommandV1 = serde_json::from_slice(&config_bytes).map_err(|e| e.to_string())?;
    if matches!(&command, CommandV1::RunPopulationBatch { .. }) {
        return population::run_population_command_v1(&path, &config_bytes, &command);
    }
    let (import_path, output) = command.paths();
    absolute(import_path)?;
    absolute(output)?;
    let import_bytes = read_bounded(import_path, MAX_JSON_BYTES)?;
    let expanded_source = command.expanded_source()?;
    let (mut play, expanded_identity) = if let Some(source) = expanded_source {
        if hash(&import_bytes) != source.play_import.sha256 {
            return Err("expanded source descriptor bytes differ from their pin".into());
        }
        let (play, receipt) = load_expanded_inference_v1(source)?;
        (play, Some(receipt))
    } else {
        // Only the legacy route interprets this path as a frozen Store import.
        // Expanded sources select their own strict, versioned descriptor reader.
        let import: FrozenPlayPolicyImportV1 =
            serde_json::from_slice(&import_bytes).map_err(|e| e.to_string())?;
        (
            match &command {
                CommandV1::RunBatch {
                    play_observation_transfer_v3: Some(transfer),
                    ..
                } => FrozenPlayPolicyV1::load_feature_transfer_v3(&import, transfer)?,
                _ => FrozenPlayPolicyV1::load_v1(&import)?,
            },
            None,
        )
    };
    let copied_embeddings = play.embedding_rows_v1().to_vec();
    let identity = SideboardPlayIdentityV1 {
        weights_sha256: expanded_identity.as_ref().map_or_else(
            || play.identity_v1().initial_weights_sha256_v1().to_owned(),
            |receipt| receipt.model.weights_sha256.clone(),
        ),
        // This field records the imported or initialized origin's producer.
        // The expanded receipt separately binds the installed model/state.
        git_head: play.identity_v1().origin_git_commit_v1().to_owned(),
    };
    let embeddings = FrozenSideboardEmbeddingsV1::new_v1(&copied_embeddings, identity)
        .map_err(|e| e.to_string())?;
    let mut inputs = vec![
        file_receipt(&path, &config_bytes),
        file_receipt(import_path, &import_bytes),
    ];
    if let (Some(model_source), Some(receipt)) = (expanded_source, &expanded_identity) {
        if let Some(pin) = &model_source.checkpoint {
            // The strict loader already checked these bytes with its own
            // 512 MiB bound. Do not reread them through the 16 MiB JSON path.
            inputs.push(json!({"path":pin.path,"sha256":receipt.checkpoint_sha256,
                "validated_by":"load_expanded_inference_v1"}));
        }
    }
    let output = output.to_path_buf();
    // A failed/interrupted directory is retained for inspection. Resuming means
    // a new directory and a new declared run, never replacing existing results.
    fs::create_dir(&output)
        .map_err(|e| format!("fresh output directory {}: {e}", output.display()))?;
    write_new(&output.join("config.json"), &config_bytes)?;
    write_json(&output.join("play-transfer.json"), play.identity_v1())?;
    if let Some(receipt) = &expanded_identity {
        write_json(&output.join("expanded-play-identity.json"), receipt)?;
    }
    write_json(
        &output.join("run-start.json"),
        &json!({
            "schema":"kernel-learned-sideboard-cli-start/v1", "command":command,
            "inputs":inputs, "binary":binary_receipt()?, "compiled_sources":compiled_sources(),
            "build": {
                "git_commit": env!("MTG_KERNEL_BUILD_GIT_HEAD"),
                "git_clean": env!("MTG_KERNEL_BUILD_GIT_CLEAN"),
                "tracked_tree_sha256": env!("MTG_KERNEL_BUILD_TRACKED_TREE_SHA256"),
                "toolchain_pin": include_str!("../../../rust-toolchain.toml")
            },
            "execution": {"device":"cpu", "gpu_ordinal":null},
            "tag_file_sha256":hash(TAG_BYTES), "embedding_table_sha256":embeddings.table_sha256_v1(),
            "expanded_play_identity":expanded_identity,
            "nonclaims":["research implementation, no strength or promotion claim", "play weights and embeddings remain frozen"]
        }),
    )?;
    let result = execute(&command, &output, &mut play, &embeddings, &mut inputs);
    match result {
        Ok(completion) => {
            write_json(&output.join("completion.json"), &completion)?;
            println!(
                "{}",
                json!({"status":"completed", "output_directory":output, "completion":completion})
            );
            Ok(())
        }
        Err(error) => {
            write_json(
                &output.join("failure.json"),
                &json!({"schema":"kernel-learned-sideboard-cli-failure/v1", "error":error, "inputs":inputs}),
            )?;
            Err(error)
        }
    }
}

fn execute(
    command: &CommandV1,
    output: &Path,
    play: &mut FrozenPlayPolicyV1,
    embeddings: &FrozenSideboardEmbeddingsV1<'_>,
    inputs: &mut Vec<Value>,
) -> Result<Value, String> {
    match command {
        CommandV1::RunPopulationBatch { .. } => {
            Err("population commands require their per-seat model loader".into())
        }
        CommandV1::ValidateImport { .. } => {
            Ok(json!({"mode":"validate_import", "inference_executed":false,
            "training_executed":false, "inputs":inputs, "play_transfer":play.identity_v1()}))
        }
        CommandV1::EvaluateImitation {
            checkpoint,
            dataset_inventory,
            split,
            expanded_model_source,
            example_embedding_transfer,
            ..
        } => {
            if example_embedding_transfer.is_some() && expanded_model_source.is_none() {
                return Err(
                    "example_embedding_transfer requires an explicit expanded_model_source".into(),
                );
            }
            evaluation::execute_evaluation(
                checkpoint,
                dataset_inventory,
                *split,
                example_embedding_transfer.as_ref(),
                output,
                embeddings,
                inputs,
            )
        }
        CommandV1::RunBatch {
            policies,
            matches,
            collect_static_imitation_examples,
            ..
        } => {
            if matches.is_empty() || matches.len() > 1024 {
                return Err("match count must be 1..1024".into());
            }
            let prepared = [
                prepare_policy(&policies[0], inputs)?,
                prepare_policy(&policies[1], inputs)?,
            ];
            if *collect_static_imitation_examples
                && !policies
                    .iter()
                    .any(|p| matches!(p, SeatPolicyV1::StaticPlanRows { .. }))
            {
                return Err(
                    "collect_static_imitation_examples requires at least one static teacher seat"
                        .into(),
                );
            }
            // Validate both learned-checkpoint bindings, every registration,
            // and every requested static row before the first episode.
            for config in matches {
                validate_match(config)?;
                for seat in 0..2 {
                    checked_in_pauper_registered_deck_by_id_v1(&config.deck_ids[seat])
                        .map_err(|e| e.to_string())?;
                    let _ = prepared[seat].bind(config, seat, embeddings)?;
                }
            }
            write_json(&output.join("inputs.json"), &inputs)?;
            let tags = checked_in_tags()?;
            let policy_identity = hash(&serde_json::to_vec(policies).map_err(|e| e.to_string())?);
            let play_weights = play.identity_v1().initial_weights_sha256_v1().to_owned();
            let mut example_files = Vec::new();
            let mut example_count = 0usize;
            let mut example_bytes = 0usize;
            let mut total_games = 0usize;
            for (index, config) in matches.iter().enumerate() {
                let mut seat0 = prepared[0].bind(config, 0, embeddings)?;
                let mut seat1 = prepared[1].bind(config, 1, embeddings)?;
                let result = run_learned_bo3_v1(
                    config.clone(),
                    &play_weights,
                    &policy_identity,
                    &tags,
                    play,
                    [&mut seat0, &mut seat1],
                )?;
                total_games += result.games.len();
                write_json(&output.join(format!("match-{index:06}.json")), &result)?;
                if *collect_static_imitation_examples {
                    let examples: Vec<_> = result
                        .sideboard_decisions
                        .iter()
                        .filter(|row| {
                            matches!(
                                policies[row.acting_player.index()],
                                SeatPolicyV1::StaticPlanRows { .. }
                            )
                        })
                        .map(|row| SideboardImitationExampleV1 {
                            input: row.input.clone(),
                            initial_mainboard: row.initial_mainboard.clone(),
                            initial_sideboard: row.initial_sideboard.clone(),
                            target_actions: row.selected_actions.clone(),
                            target_value: None,
                        })
                        .collect();
                    let mut bytes = Vec::new();
                    for example in &examples {
                        serde_json::to_writer(&mut bytes, example).map_err(|e| e.to_string())?;
                        bytes.push(b'\n');
                    }
                    let example_path = output.join(format!("examples-{index:06}.jsonl"));
                    write_new(&example_path, &bytes)?;
                    example_count += examples.len();
                    example_files.push(example_path);
                    // Preserve this completed match, then stop before another
                    // match if its examples exceed the final dataset bound.
                    example_bytes = example_bytes
                        .checked_add(bytes.len())
                        .ok_or("example bytes overflow")?;
                    if example_bytes > MAX_EXAMPLE_BYTES {
                        return Err(
                            "collected examples exceed 128 MiB; use smaller declared batches"
                                .into(),
                        );
                    }
                }
                println!(
                    "{}",
                    json!({"completed_match":index, "physical_games":result.games.len(), "artifact":format!("match-{index:06}.json")})
                );
            }
            let examples_pin = if *collect_static_imitation_examples {
                let final_path = output.join("imitation-examples.jsonl");
                concatenate_new(&final_path, &example_files)?;
                let examples_bytes = read_bounded(&final_path, MAX_EXAMPLE_BYTES)?;
                let provenance = json!({"schema":"kernel-static-sideboard-teaching-provenance/v1",
                    "teacher_kind":"explicit_static_table", "policies":policies, "matches":matches,
                    "examples":file_receipt(&final_path, &examples_bytes), "example_count":example_count,
                    "play_transfer":play.identity_v1(), "source_inputs":inputs,
                    "target_values":"all absent; selected table actions are imitation targets",
                    "nonclaims":["not automatically ratified search tables", "not RL training", "not evidence of improved win rate"]});
                write_json(
                    &output.join("imitation-examples.provenance.json"),
                    &provenance,
                )?;
                Some(file_receipt(&final_path, &examples_bytes))
            } else {
                None
            };
            Ok(
                json!({"mode":"run_batch", "completed_matches":matches.len(), "physical_games":total_games,
                "static_imitation_examples":example_count, "examples":examples_pin, "inputs":inputs,
                "no_training_performed":true, "strength_claim":false}),
            )
        }
        CommandV1::RunExpandedBatch {
            policies, matches, ..
        } => {
            if matches.is_empty() || matches.len() > 1024 {
                return Err("match count must be 1..1024".into());
            }
            let prepared = [
                prepare_policy(&policies[0], inputs)?,
                prepare_policy(&policies[1], inputs)?,
            ];
            let registrations = matches
                .iter()
                .map(ExpandedBo3MatchV1::registrations)
                .collect::<Result<Vec<_>, _>>()?;
            for (item, registered) in matches.iter().zip(&registrations) {
                for seat in 0..2 {
                    let _ =
                        prepared[seat].bind_explicit(&item.config, registered, seat, embeddings)?;
                }
            }
            write_json(&output.join("inputs.json"), &inputs)?;
            let tags = checked_in_tags()?;
            let policy_identity = hash(&serde_json::to_vec(policies).map_err(|e| e.to_string())?);
            let actual_identity = play.actual_model_identity_v1();
            let mut total_games = 0usize;
            for (index, (item, registered)) in matches.iter().zip(registrations).enumerate() {
                let mut seat0 =
                    prepared[0].bind_explicit(&item.config, &registered, 0, embeddings)?;
                let mut seat1 =
                    prepared[1].bind_explicit(&item.config, &registered, 1, embeddings)?;
                let mut result = run_learned_bo3_with_registrations_v1(
                    item.config.clone(),
                    registered,
                    &actual_identity.weights_sha256,
                    &policy_identity,
                    &tags,
                    play,
                    [&mut seat0, &mut seat1],
                )?;
                result.play_observation_contract =
                    Some("rich-v6-flat-v3-expanded-inference".into());
                total_games += result.games.len();
                write_json(&output.join(format!("match-{index:06}.json")), &result)?;
                println!(
                    "{}",
                    json!({"completed_match":index,"physical_games":result.games.len(),
                    "artifact":format!("match-{index:06}.json")})
                );
            }
            Ok(
                json!({"mode":"run_expanded_batch","completed_matches":matches.len(),
                "physical_games":total_games,"inputs":inputs,"actual_play_model":actual_identity,
                "no_training_performed":true,"strength_claim":false}),
            )
        }
        CommandV1::TrainImitation {
            examples,
            teacher_provenance,
            measured_value_provenance,
            initialization,
            training,
            ..
        } => {
            check_provenance(teacher_provenance, inputs)?;
            let bytes = read_pin(examples, MAX_EXAMPLE_BYTES, inputs)?;
            let text = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
            let examples: Vec<SideboardImitationExampleV1> = text
                .lines()
                .enumerate()
                .map(|(i, line)| {
                    serde_json::from_str(line).map_err(|e| format!("imitation row {}: {e}", i + 1))
                })
                .collect::<Result<_, _>>()?;
            if examples.is_empty() || examples.len() > 100_000 {
                return Err("imitation example count must be 1..100000".into());
            }
            let measured = examples.iter().filter(|e| e.target_value.is_some()).count();
            if measured > 0 && measured_value_provenance.is_none() {
                return Err("non-null target_value requires an independently pinned measured_value_provenance artifact".into());
            }
            if let Some(pin) = measured_value_provenance {
                let _ = read_pin(pin, MAX_EXAMPLE_BYTES, inputs)?;
            }
            let mut model = match initialization {
                InitializationV1::Fresh { seed } => {
                    LearnedSideboardModelV1::new_v1(*seed, embeddings)
                }
                InitializationV1::Checkpoint { checkpoint } => {
                    load_sideboard_checkpoint(checkpoint, inputs)?
                }
            };
            write_json(&output.join("inputs.json"), &inputs)?;
            let before = model.checkpoint_sha256_v1().map_err(|e| e.to_string())?;
            let metrics = model
                .train_imitation_v1(&examples, embeddings, *training)
                .map_err(|e| e.to_string())?;
            let checkpoint_bytes = model.to_json_v1().map_err(|e| e.to_string())?.into_bytes();
            write_new(&output.join("sideboard-checkpoint.json"), &checkpoint_bytes)?;
            write_json(&output.join("training-metrics.json"), &metrics)?;
            Ok(
                json!({"mode":"train_imitation", "learning_method":"supervised imitation with optional supplied measured value targets",
                "checkpoint_sha256":hash(&checkpoint_bytes), "initial_checkpoint_sha256":before,
                "teacher_provenance":teacher_provenance, "measured_value_provenance":measured_value_provenance,
                "metrics":metrics, "inputs":inputs, "play_weights_unchanged":true,
                "nonclaims":["training-set fit is not held-out playing strength", "not reinforcement learning", "no candidate promotion"]}),
            )
        }
    }
}

fn prepare_policy(
    spec: &SeatPolicyV1,
    inputs: &mut Vec<Value>,
) -> Result<PreparedPolicyV1, String> {
    match spec {
        SeatPolicyV1::Keep => Ok(PreparedPolicyV1::Keep),
        SeatPolicyV1::Learned { checkpoint } => Ok(PreparedPolicyV1::Learned(
            load_sideboard_checkpoint(checkpoint, inputs)?,
        )),
        SeatPolicyV1::StaticPlanRows {
            table,
            teacher_provenance,
            carry_game_three_forward,
        } => {
            check_provenance(teacher_provenance, inputs)?;
            let bytes = read_pin(table, MAX_JSON_BYTES, inputs)?;
            let document: StaticRowsV1 =
                serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            if document.rows.is_empty() {
                return Err("static row table is empty".into());
            }
            let mut keys = std::collections::BTreeSet::new();
            for row in &document.rows {
                if !keys.insert((&row.self_deck_id, &row.opponent_deck_id, row.game_index)) {
                    return Err("duplicate static row key".into());
                }
                SideboardPlanV1::new_v1(
                    &row.self_deck_id,
                    &row.opponent_deck_id,
                    row.game_index,
                    row.cards_in.clone(),
                    row.cards_out.clone(),
                )
                .map_err(|e| e.to_string())?;
            }
            Ok(PreparedPolicyV1::Static {
                rows: document.rows,
                table_sha256: Sha256::digest(&bytes).into(),
                carry: *carry_game_three_forward,
            })
        }
    }
}

fn load_sideboard_checkpoint(
    pin: &PinnedFileV1,
    inputs: &mut Vec<Value>,
) -> Result<LearnedSideboardModelV1, String> {
    let bytes = read_pin(pin, MAX_JSON_BYTES, inputs)?;
    LearnedSideboardModelV1::from_json_v1(std::str::from_utf8(&bytes).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

fn validate_match(config: &LearnedBo3RunConfigV1) -> Result<(), String> {
    if config.game_one_chooser.0 > 1
        || !(3..=32).contains(&config.max_physical_games)
        || !(1..=10_000).contains(&config.max_physical_decisions)
        || !(1..=1_000_000).contains(&config.max_policy_steps)
    {
        return Err("invalid bounded match settings (chooser 0/1, games 3..32, decisions 1..10000, policy steps 1..1000000)".into());
    }
    Ok(())
}

fn checked_in_tags() -> Result<RemovalCounterspellTagsV1, String> {
    let value: Value = serde_json::from_slice(TAG_BYTES).map_err(|e| e.to_string())?;
    if value["schema"] != "kernel_removal_counterspell_tags/v1" {
        return Err("checked-in tag schema differs".into());
    }
    let rows = value["cards"]
        .as_array()
        .ok_or("checked-in tag rows missing")?;
    let mut tags = RemovalCounterspellTagsV1 {
        requires_target: Default::default(),
        is_counterspell: Default::default(),
    };
    for row in rows {
        let id = u16::try_from(row["card_id"].as_u64().ok_or("invalid tag id")?)
            .map_err(|e| e.to_string())?;
        if row["requires_target"]
            .as_bool()
            .ok_or("invalid requires_target tag")?
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

fn check_provenance(
    provenance: &TeacherProvenanceV1,
    inputs: &mut Vec<Value>,
) -> Result<(), String> {
    if provenance.description.trim().is_empty() || provenance.description.len() > 4096 {
        return Err("teacher description must contain 1..4096 bytes".into());
    }
    let _ = read_pin(&provenance.artifact, MAX_JSON_BYTES, inputs)?;
    Ok(())
}

fn read_pin(pin: &PinnedFileV1, cap: usize, inputs: &mut Vec<Value>) -> Result<Vec<u8>, String> {
    absolute(&pin.path)?;
    let bytes = read_bounded(&pin.path, cap)?;
    if hash(&bytes) != pin.sha256 {
        return Err(format!("input SHA differs: {}", pin.path.display()));
    }
    inputs.push(file_receipt(&pin.path, &bytes));
    Ok(bytes)
}

fn read_bounded(path: &Path, cap: usize) -> Result<Vec<u8>, String> {
    let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() > cap as u64 {
        return Err("input is not a bounded regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(cap as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > cap {
        return Err("input grew beyond its bound".into());
    }
    Ok(bytes)
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    write_new(path, &bytes)
}

/// A synced complete file is published with an exclusive hard link in the
/// same directory. Existing destinations are never replaced. This provides
/// process-crash consistency, not a cross-platform power-loss guarantee.
fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let stage = staging_path(path)?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&stage)
        .map_err(|e| e.to_string())?;
    file.write_all(bytes).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    fs::hard_link(&stage, path)
        .map_err(|e| format!("exclusive publication {}: {e}", path.display()))?;
    fs::remove_file(stage).map_err(|e| e.to_string())
}

fn concatenate_new(path: &Path, sources: &[PathBuf]) -> Result<(), String> {
    let stage = staging_path(path)?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&stage)
        .map_err(|e| e.to_string())?;
    let mut total = 0usize;
    for source in sources {
        let bytes = read_bounded(source, MAX_EXAMPLE_BYTES)?;
        total = total
            .checked_add(bytes.len())
            .ok_or("example bytes overflow")?;
        if total > MAX_EXAMPLE_BYTES {
            return Err("collected examples exceed 128 MiB; use smaller declared batches".into());
        }
        file.write_all(&bytes).map_err(|e| e.to_string())?;
    }
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    fs::hard_link(&stage, path).map_err(|e| e.to_string())?;
    fs::remove_file(stage).map_err(|e| e.to_string())
}

fn staging_path(path: &Path) -> Result<PathBuf, String> {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("invalid output filename")?;
    Ok(path.with_file_name(format!(".{name}.staging")))
}
fn absolute(path: &Path) -> Result<(), String> {
    if path.is_absolute() {
        Ok(())
    } else {
        Err(format!("absolute path required: {}", path.display()))
    }
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn file_receipt(path: &Path, bytes: &[u8]) -> Value {
    json!({"path":path,"sha256":hash(bytes),"bytes":bytes.len()})
}
fn binary_receipt() -> Result<Value, String> {
    let path = std::env::current_exe().map_err(|e| e.to_string())?;
    let bytes = read_bounded(&path, 512 * 1024 * 1024)?;
    Ok(file_receipt(&path, &bytes))
}
fn compiled_sources() -> Value {
    json!({"scope":"compiled integration sources; binary SHA additionally pins the executable",
        "learned_sideboard_v1.rs":hash(include_bytes!("learned_sideboard_v1.rs")),
        "learned_sideboard_model":hash(include_bytes!("../learned_sideboard_v1.rs")),
        "imitation_evaluator":hash(include_bytes!("learned_sideboard_v1/evaluation.rs")),
        "population_cli":hash(include_bytes!("learned_sideboard_v1/population.rs")),
        "learned_bo3":hash(include_bytes!("../learned_bo3_v1.rs")),
        "play_policy":hash(include_bytes!("../sideboard_play_policy_v1.rs")),
        "expanded_training_and_loader":hash(include_bytes!("../expanded_deck_training_v1.rs")),
        "registration":hash(include_bytes!("../sideboard.rs")),
        "bo3_session":hash(include_bytes!("../bo3_session.rs")),
        "observation_v6":hash(include_bytes!("../policy_observation_v6.rs")),
        "flat_policy_v3":hash(include_bytes!("../flat_policy_v3.rs")),
        "flat_action_v3":hash(include_bytes!("../rl_session/flat_action_v3.rs")),
        "tensorizer_v3":hash(include_bytes!("../native_flat_tensorizer_v3.rs")),
        "feature_descriptor_v3":hash(include_bytes!("../../../data/flat_policy_v3/feature_contract_v3.json")),
        "paired_harness":hash(include_bytes!("../paired_bo1_harness_v1.rs")),
        "decision_time_search":hash(include_bytes!("../phase1_v4_decision_search_v1.rs")),
        "game_summary":hash(include_bytes!("../game_summary_v1.rs")),
        "library_registration":hash(include_bytes!("../lib.rs")),
        "card_registry":hash(include_bytes!("../../../data/cards_v1.json")),
        "cargo_lock":hash(include_bytes!("../../../Cargo.lock"))})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_match() -> LearnedBo3RunConfigV1 {
        LearnedBo3RunConfigV1 {
            deck_ids: ["Rally".into(), "Burn".into()],
            seed: 1,
            game_one_chooser: mtg_kernel::ids::PlayerId::P0,
            max_physical_games: 4,
            max_physical_decisions: 1024,
            max_policy_steps: 2048,
            opening_protocol: Default::default(),
        }
    }

    fn test_play_identity() -> SideboardPlayIdentityV1 {
        SideboardPlayIdentityV1 {
            weights_sha256: "a".repeat(64),
            git_head: "b".repeat(40),
        }
    }

    #[test]
    fn learned_checkpoint_preflight_accepts_exact_binding() {
        let values = vec![0.0; 65537 * 16];
        let embeddings =
            FrozenSideboardEmbeddingsV1::new_v1(&values, test_play_identity()).unwrap();
        let checkpoint = LearnedSideboardModelV1::new_v1(7, &embeddings)
            .to_json_v1()
            .unwrap();
        let policy =
            PreparedPolicyV1::Learned(LearnedSideboardModelV1::from_json_v1(&checkpoint).unwrap());
        for seat in 0..2 {
            assert!(policy.bind(&test_match(), seat, &embeddings).is_ok());
        }
    }

    #[test]
    fn learned_checkpoint_preflight_rejects_changed_play_identity() {
        let values = vec![0.0; 65537 * 16];
        let embeddings =
            FrozenSideboardEmbeddingsV1::new_v1(&values, test_play_identity()).unwrap();
        let policy = PreparedPolicyV1::Learned(LearnedSideboardModelV1::new_v1(7, &embeddings));
        for field in 0..2 {
            let mut identity = test_play_identity();
            if field == 0 {
                identity.weights_sha256 = "c".repeat(64);
            } else {
                identity.git_head = "d".repeat(40);
            }
            let different = FrozenSideboardEmbeddingsV1::new_v1(&values, identity).unwrap();
            for seat in 0..2 {
                let error = policy.bind(&test_match(), seat, &different).err().unwrap();
                assert!(error.contains(&format!("seat {seat} learned checkpoint preflight")));
                assert!(error.contains("frozen play embeddings disagree"));
            }
        }
    }

    #[test]
    fn learned_checkpoint_preflight_rejects_changed_embedding_bytes() {
        let values = vec![0.0; 65537 * 16];
        let embeddings =
            FrozenSideboardEmbeddingsV1::new_v1(&values, test_play_identity()).unwrap();
        let policy = PreparedPolicyV1::Learned(LearnedSideboardModelV1::new_v1(7, &embeddings));
        let mut different_values = values.clone();
        different_values[16] = 0.5;
        let different =
            FrozenSideboardEmbeddingsV1::new_v1(&different_values, test_play_identity()).unwrap();
        for seat in 0..2 {
            let error = policy.bind(&test_match(), seat, &different).err().unwrap();
            assert!(error.contains("frozen play embeddings disagree"));
        }
    }

    #[test]
    fn config_rejects_unknown_fields() {
        let value = json!({"mode":"validate_import","play_import":"C:/inputs.json","output_directory":"C:/new","silent_fallback":true});
        assert!(serde_json::from_value::<CommandV1>(value).is_err());
    }

    #[test]
    fn successor_fit_source_cannot_disagree_with_import_and_is_optional_for_legacy() {
        let mut value = json!({"mode":"evaluate_imitation","play_import":"C:/input.json",
            "output_directory":"C:/new", "checkpoint":{"path":"C:/head.json","sha256":"a".repeat(64)},
            "dataset_inventory":{"path":"C:/inventory.json","sha256":"b".repeat(64)},
            "split":"imitation_eval"});
        let legacy: CommandV1 = serde_json::from_value(value.clone()).unwrap();
        assert!(legacy.expanded_source().unwrap().is_none());
        value["expanded_model_source"] = json!({
            "play_import":{"path":"C:/different.json","sha256":"c".repeat(64)},
            "feature_transfer":{"expected_feature_contract_digest":"d".repeat(64),
                "expected_feature_encoding_digest":"e".repeat(64)}, "checkpoint":null });
        let wrong: CommandV1 = serde_json::from_value(value.clone()).unwrap();
        assert!(wrong.expanded_source().unwrap_err().contains("same import"));
        value["expanded_model_source"]["play_import"]["path"] = json!("C:/input.json");
        let valid: CommandV1 = serde_json::from_value(value).unwrap();
        assert!(valid.expanded_source().unwrap().is_some());
    }

    #[test]
    fn explicit_teacher_checks_both_actual_registrations() {
        let values = vec![0.0; 65537 * 16];
        let embeddings =
            FrozenSideboardEmbeddingsV1::new_v1(&values, test_play_identity()).unwrap();
        let config = test_match();
        let canonical = config
            .deck_ids
            .clone()
            .map(|id| checked_in_pauper_registered_deck_by_id_v1(&id).unwrap());
        let policy = PreparedPolicyV1::Static {
            rows: (2..=3)
                .map(|game| StaticRowV1 {
                    self_deck_id: "Rally".into(),
                    opponent_deck_id: "Burn".into(),
                    game_index: game,
                    cards_in: vec![],
                    cards_out: vec![],
                })
                .collect(),
            table_sha256: [0; 32],
            carry: true,
        };
        assert!(policy
            .bind_explicit(&config, &canonical, 0, &embeddings)
            .is_ok());
        for changed_seat in 0..2 {
            let mut modified = canonical.clone();
            let original = modified[changed_seat].registered_configuration();
            let mut main = original.mainboard().to_vec();
            let mut side = original.sideboard().to_vec();
            let pair = main
                .iter()
                .enumerate()
                .find_map(|(i, card)| side.iter().position(|other| other != card).map(|j| (i, j)))
                .unwrap();
            std::mem::swap(&mut main[pair.0], &mut side[pair.1]);
            modified[changed_seat] =
                RegisteredDeckV1::new_executable_v1(&config.deck_ids[changed_seat], main, side)
                    .unwrap();
            let error = policy
                .bind_explicit(&config, &modified, 0, &embeddings)
                .err()
                .unwrap();
            assert!(error.contains(&format!("seat {changed_seat}'s explicit registration")));
            assert!(PreparedPolicyV1::Keep
                .bind_explicit(&config, &modified, 0, &embeddings)
                .is_ok());
        }
    }

    #[test]
    fn explicit_match_requires_matching_labels_and_executable_lists() {
        let config = test_match();
        let lists = config.deck_ids.clone().map(|label| {
            let deck = checked_in_pauper_registered_deck_by_id_v1(&label).unwrap();
            let cards = deck.registered_configuration();
            ExpandedDeckListV1 {
                label,
                mainboard: cards.mainboard().to_vec(),
                sideboard: cards.sideboard().to_vec(),
            }
        });
        let mut item = ExpandedBo3MatchV1 {
            config,
            registered: lists,
        };
        assert!(item.registrations().is_ok());
        item.registered[0].label = "Rally custom".into();
        assert!(item.registrations().unwrap_err().contains("label differs"));
        item.config.deck_ids[0] = "Rally custom".into();
        assert!(item.registrations().is_ok());
        item.registered[0].mainboard.pop();
        assert!(item.registrations().is_err());
    }

    #[test]
    fn missing_static_matchup_is_an_error() {
        let embedding_values = vec![0.0; 65537 * 16];
        let embeddings = FrozenSideboardEmbeddingsV1::new_v1(
            &embedding_values,
            SideboardPlayIdentityV1 {
                weights_sha256: "a".repeat(64),
                git_head: "b".repeat(40),
            },
        )
        .unwrap();
        let policy = PreparedPolicyV1::Static {
            rows: vec![],
            table_sha256: [0; 32],
            carry: true,
        };
        assert!(policy.bind(&test_match(), 0, &embeddings).is_err());
    }
}
