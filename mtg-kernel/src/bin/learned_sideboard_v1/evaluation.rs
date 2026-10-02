//! Fixed-checkpoint development diagnostics; inventory metadata never enters a model input.
use super::*;
use mtg_kernel::learned_sideboard_v1::{
    SideboardDeliberationStateV1, SideboardImitationEvaluationV1,
};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum EvaluationSplitV1 {
    Train,
    ImitationEval,
}
impl EvaluationSplitV1 {
    fn name(self) -> &'static str {
        match self {
            Self::Train => "train",
            Self::ImitationEval => "imitation_eval",
        }
    }
}

/// Re-encode unchanged public imitation examples with a successor player's
/// embeddings. Their behavior-player provenance and grouped split stay intact.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ImitationEmbeddingTransferV1 {
    expected_behavior_play_weights_sha256: String,
    expected_embedding_play_weights_sha256: String,
}

fn validate_example_play_identity(
    records: &[Value],
    embedding_weights: &str,
    transfer: Option<&ImitationEmbeddingTransferV1>,
) -> Result<(), String> {
    let expected_behavior = if let Some(transfer) = transfer {
        for pin in [
            &transfer.expected_behavior_play_weights_sha256,
            &transfer.expected_embedding_play_weights_sha256,
        ] {
            require(
                pin.len() == 64 && pin.bytes().all(|b| b.is_ascii_hexdigit()),
                "imitation embedding transfer requires exact SHA-256 identities",
            )?;
        }
        require(
            transfer.expected_embedding_play_weights_sha256 == embedding_weights,
            "imitation embedding transfer target differs from actual loaded player",
        )?;
        transfer.expected_behavior_play_weights_sha256.as_str()
    } else {
        embedding_weights
    };
    for record in records {
        require(
            string(record, "play_weights_sha256")? == expected_behavior,
            "dataset behavior play identity differs from expected example source",
        )?;
    }
    Ok(())
}

fn field<'a>(value: &'a Value, key: &str) -> Result<&'a Value, String> {
    value
        .get(key)
        .ok_or_else(|| format!("inventory missing {key}"))
}
fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    field(value, key)?
        .as_str()
        .ok_or_else(|| format!("inventory {key} must be a string"))
}
fn array<'a>(value: &'a Value, key: &str) -> Result<&'a Vec<Value>, String> {
    field(value, key)?
        .as_array()
        .ok_or_else(|| format!("inventory {key} must be an array"))
}
fn integer(value: &Value, key: &str) -> Result<usize, String> {
    field(value, key)?
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| format!("inventory {key} must be a nonnegative integer"))
}
fn canonical_hash(value: &Value) -> Result<String, String> {
    Ok(hash(&serde_json::to_vec(value).map_err(|e| e.to_string())?))
}
fn require(ok: bool, message: &str) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        Err(message.into())
    }
}

// Verify every explicit provenance pin, deduplicating repeated references. This
// checks pinned local artifact consistency, not independent authentication.
fn verify_pins(
    value: &Value,
    cache: &mut BTreeMap<PathBuf, Vec<u8>>,
    inputs: &mut Vec<Value>,
) -> Result<(), String> {
    match value {
        Value::Object(object) => {
            if let (Some(path), Some(sha)) = (
                object.get("path").and_then(Value::as_str),
                object.get("sha256").and_then(Value::as_str),
            ) {
                let path = PathBuf::from(path);
                let bytes = if let Some(bytes) = cache.get(&path) {
                    bytes.clone()
                } else {
                    let bytes = read_pin(
                        &PinnedFileV1 {
                            path: path.clone(),
                            sha256: sha.into(),
                        },
                        MAX_EXAMPLE_BYTES,
                        inputs,
                    )?;
                    cache.insert(path.clone(), bytes.clone());
                    bytes
                };
                require(hash(&bytes) == sha, "conflicting provenance SHA pins")?;
                if let Some(size) = object.get("bytes") {
                    require(
                        size.as_u64() == Some(bytes.len() as u64),
                        "provenance byte count differs",
                    )?;
                }
            }
            for child in object.values() {
                verify_pins(child, cache, inputs)?;
            }
        }
        Value::Array(values) => {
            for child in values {
                verify_pins(child, cache, inputs)?;
            }
        }
        _ => (),
    }
    Ok(())
}

fn output_bytes<'a>(
    inventory: &Value,
    name: &str,
    cache: &'a BTreeMap<PathBuf, Vec<u8>>,
) -> Result<&'a [u8], String> {
    let pin = field(field(inventory, "outputs")?, name)?;
    cache
        .get(&PathBuf::from(string(pin, "path")?))
        .map(Vec::as_slice)
        .ok_or("unverified inventory output".into())
}

#[derive(Debug)]
struct AuditedDataset {
    examples: Vec<SideboardImitationExampleV1>,
    records: Vec<Value>,
    component_ids: Vec<String>,
    components: usize,
}

fn audit_dataset(
    inventory: &Value,
    split: EvaluationSplitV1,
    cache: &BTreeMap<PathBuf, Vec<u8>>,
) -> Result<AuditedDataset, String> {
    require(
        string(inventory, "schema")? == "sideboard-dataset-development-inventory/v1",
        "unsupported inventory schema",
    )?;
    let records = array(inventory, "records")?;
    require(
        !records.is_empty() && records.len() <= 100_000,
        "inventory record count must be 1..100000",
    )?;
    let lines: Vec<_> = std::str::from_utf8(output_bytes(inventory, "all_examples", cache)?)
        .map_err(|e| e.to_string())?
        .lines()
        .collect();
    require(
        lines.len() == records.len(),
        "all-examples count differs from inventory",
    )?;
    require(
        integer(field(inventory, "counts")?, "examples")? == records.len(),
        "inventory example count differs",
    )?;
    let mut component_by_record = BTreeMap::new();
    let mut component_splits = BTreeMap::new();
    let mut declared_groups = BTreeMap::new();
    for component in array(inventory, "components")? {
        let id = string(component, "component_id")?.to_owned();
        let assigned = string(component, "split")?;
        require(
            matches!(assigned, "train" | "imitation_eval"),
            "unknown component split",
        )?;
        require(
            component_splits.insert(id.clone(), assigned).is_none(),
            "duplicate component id",
        )?;
        let ids = array(component, "record_ids")?;
        require(
            ids.len() == integer(component, "examples")?,
            "component example count differs",
        )?;
        let groups = field(component, "groups")?;
        for group in ["match_group", "teacher_plan_group", "example_sha256"] {
            let members: BTreeSet<_> = array(groups, group)?
                .iter()
                .map(|v| {
                    v.as_str()
                        .map(str::to_owned)
                        .ok_or("invalid component group".to_string())
                })
                .collect::<Result<_, _>>()?;
            declared_groups.insert((id.clone(), group), members);
        }
        for record_id in ids {
            let record_id = record_id.as_str().ok_or("invalid component record id")?;
            require(
                component_by_record
                    .insert(record_id.to_owned(), id.clone())
                    .is_none(),
                "record occurs in multiple components",
            )?;
        }
    }
    require(
        component_by_record.len() == records.len(),
        "component coverage differs",
    )?;
    let mut seen_ids = BTreeSet::new();
    let mut actual_groups: BTreeMap<(String, &str), BTreeSet<String>> = BTreeMap::new();
    let mut group_owners = BTreeMap::new();
    let mut split_bytes: BTreeMap<&str, Vec<u8>> =
        BTreeMap::from([("train", Vec::new()), ("imitation_eval", Vec::new())]);
    let mut result = AuditedDataset {
        examples: Vec::new(),
        records: Vec::new(),
        component_ids: Vec::new(),
        components: 0,
    };
    for (record, line) in records.iter().zip(&lines) {
        let id = string(record, "record_id")?;
        require(seen_ids.insert(id), "duplicate record id")?;
        let component = component_by_record
            .get(id)
            .ok_or("record missing component")?;
        let assigned = string(record, "split")?;
        require(
            component_splits.get(component) == Some(&assigned),
            "record/component split disagreement",
        )?;
        let value: Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
        require(
            canonical_hash(&value)? == string(record, "example_sha256")?,
            "example content differs from inventory hash",
        )?;
        let id_parts: Vec<_> = id.split(':').collect();
        require(id_parts.len() == 3, "invalid source record identity")?;
        let match_index = id_parts[1].parse::<usize>().map_err(|e| e.to_string())?;
        let source_row = id_parts[2].parse::<usize>().map_err(|e| e.to_string())?;
        require(
            match_index == integer(record, "match_index")?,
            "record source match index differs",
        )?;
        let batches = array(field(inventory, "inputs")?, "batches")?;
        let batches: Vec<_> = batches
            .iter()
            .filter(|batch| {
                batch
                    .get("dataset")
                    .and_then(|pin| pin.get("sha256"))
                    .and_then(Value::as_str)
                    == Some(id_parts[0])
            })
            .collect();
        require(
            batches.len() == 1,
            "record requires exactly one pinned source batch",
        )?;
        let source_match = array(batches[0], "matches")?
            .get(match_index)
            .ok_or("source match absent")?;
        let source_bytes = cache
            .get(&PathBuf::from(string(
                field(source_match, "examples")?,
                "path",
            )?))
            .ok_or("unverified source examples")?;
        let source_text = std::str::from_utf8(source_bytes).map_err(|e| e.to_string())?;
        require(
            source_text.lines().nth(source_row) == Some(*line),
            "split row differs from original pinned source row",
        )?;
        let match_bytes = cache
            .get(&PathBuf::from(string(
                field(source_match, "match")?,
                "path",
            )?))
            .ok_or("unverified source match")?;
        let match_value: Value = serde_json::from_slice(match_bytes).map_err(|e| e.to_string())?;
        let match_config = field(&match_value, "config")?;
        let seats = array(match_config, "deck_ids")?;
        let seat = integer(record, "acting_player")?;
        require(seat < 2 && seats.len() == 2, "invalid source match seat")?;
        require(
            field(record, "registration_pair")? == &json!([seats[seat], seats[1 - seat]]),
            "matchup stratum differs from source",
        )?;
        require(
            field(record, "seed")? == field(match_config, "seed")?,
            "source match seed differs",
        )?;
        require(
            field(record, "play_weights_sha256")? == field(&match_value, "play_weights_sha256")?,
            "source play identity differs",
        )?;
        let registrations = field(inventory, "registrations")?;
        let source_registrations: Vec<_> = seats
            .iter()
            .map(|deck| field(registrations, deck.as_str().ok_or("invalid source deck")?))
            .collect::<Result<_, String>>()?;
        let match_group = canonical_hash(
            &json!({"deck_ids":seats,"seed":field(match_config,"seed")?,
            "game_one_chooser":field(match_config,"game_one_chooser")?,"registrations":source_registrations}),
        )?;
        require(
            match_group == string(record, "match_group")?
                && match_group == string(source_match, "match_group")?,
            "match group differs from source identity",
        )?;
        let example: SideboardImitationExampleV1 =
            serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
        require(
            example.target_value.is_none()
                && record.get("target_value_present") == Some(&Value::Bool(false)),
            "evaluation requires absent value labels",
        )?;
        require(
            example.input.next_game_number as usize == integer(record, "next_game_number")?,
            "game stratum differs from example",
        )?;
        require(
            example.target_actions.len() == integer(record, "action_targets")?,
            "action target count differs",
        )?;
        let moves = example
            .target_actions
            .iter()
            .filter(|&&a| a != SideboardActionV1::Done)
            .count();
        require(
            moves == integer(record, "movement_targets")?,
            "movement count differs",
        )?;
        let pair = array(record, "registration_pair")?;
        require(pair.len() == 2, "registration pair requires two decks")?;
        let own = pair[0].as_str().ok_or("invalid own registration")?;
        let registration = field(field(inventory, "registrations")?, own)?;
        let main: Vec<u16> = serde_json::from_value(field(registration, "mainboard")?.clone())
            .map_err(|e| e.to_string())?;
        let side: Vec<u16> = serde_json::from_value(field(registration, "sideboard")?.clone())
            .map_err(|e| e.to_string())?;
        let registered =
            DeckConfigurationV1::new_exact_v1(main, side).map_err(|e| e.to_string())?;
        require(
            registered.combined_card_counts_v1() == example.input.registered_cards,
            "model input differs from own registration",
        )?;
        require(
            canonical_hash(registration)? == string(record, "own_registration_sha256")?,
            "own registration hash differs",
        )?;
        let initial = DeckConfigurationV1::new_exact_v1(
            example.initial_mainboard.clone(),
            example.initial_sideboard.clone(),
        )
        .map_err(|e| e.to_string())?;
        let mut teacher = SideboardDeliberationStateV1::new_v1(&initial);
        for &action in &example.target_actions {
            teacher.apply_v1(action).map_err(|e| e.to_string())?;
        }
        require(teacher.is_done_v1(), "teacher does not terminate")?;
        let target = teacher.configuration_v1().map_err(|e| e.to_string())?;
        let plan_key = canonical_hash(
            &json!({"registration":registration, "target":{"mainboard":target.mainboard(), "sideboard":target.sideboard()}}),
        )?;
        require(
            plan_key == string(record, "teacher_plan_group")?,
            "teacher plan group differs from actual configuration",
        )?;
        for group in ["match_group", "teacher_plan_group", "example_sha256"] {
            let key = string(record, group)?.to_owned();
            actual_groups
                .entry((component.clone(), group))
                .or_default()
                .insert(key.clone());
            if let Some(previous) = group_owners.insert((group, key), component.clone()) {
                require(
                    previous == *component,
                    "a grouping relationship crosses components",
                )?;
            }
        }
        // The split rows must be byte-identical ordered subsets of all-examples.
        let bytes = split_bytes
            .get_mut(assigned)
            .ok_or("unknown record split")?;
        bytes.extend_from_slice(line.as_bytes());
        bytes.push(b'\n');
        if assigned == split.name() {
            result.examples.push(example);
            result.records.push(record.clone());
            result.component_ids.push(component.clone());
        }
    }
    require(
        actual_groups == declared_groups,
        "component group membership differs from records",
    )?;
    for (name, bytes) in split_bytes {
        require(
            bytes == output_bytes(inventory, name, cache)?,
            "split output bytes/ordering differ from assignments",
        )?;
    }
    require(!result.examples.is_empty(), "requested split is empty")?;
    result.components = result.component_ids.iter().collect::<BTreeSet<_>>().len();
    Ok(result)
}

#[derive(Default)]
struct Counts {
    examples: usize,
    actions: usize,
    correct: usize,
    moves: usize,
    moves_correct: usize,
    done: usize,
    done_correct: usize,
    loss: f64,
    legal: usize,
    configuration: usize,
    exchange: usize,
    sequence: usize,
    target_cards: usize,
    predicted_cards: usize,
    correct_cards: usize,
}
fn ratio(n: usize, d: usize) -> Option<f64> {
    (d > 0).then(|| n as f64 / d as f64)
}
impl Counts {
    fn add(&mut self, row: &SideboardImitationEvaluationV1) {
        self.examples += 1;
        self.actions += row.teacher_actions;
        self.correct += row.teacher_action_correct;
        self.moves += row.teacher_movement_actions;
        self.moves_correct += row.teacher_movement_correct;
        self.done += row.teacher_done_actions;
        self.done_correct += row.teacher_done_correct;
        self.loss += row.teacher_cross_entropy_sum;
        self.legal += usize::from(row.greedy_legal_terminated);
        self.configuration += usize::from(row.greedy_exact_configuration);
        self.exchange += usize::from(row.greedy_exact_exchange);
        self.sequence += usize::from(row.greedy_exact_action_sequence);
        self.target_cards += row.target_exchange_cards;
        self.predicted_cards += row.greedy_exchange_cards;
        self.correct_cards += row.correct_exchange_cards;
    }
    fn json(&self) -> Value {
        json!({"examples":self.examples, "teacher_forced":{"actions":self.actions, "correct":self.correct,
            "action_agreement":ratio(self.correct,self.actions), "cross_entropy":if self.actions > 0 {Some(self.loss/self.actions as f64)}else{None},
            "movement_actions":self.moves,"movement_correct":self.moves_correct,"movement_agreement":ratio(self.moves_correct,self.moves),
            "done_actions":self.done,"done_correct":self.done_correct,"done_agreement":ratio(self.done_correct,self.done)},
            "greedy_free_running":{"legal_terminated":self.legal,"legal_termination_rate":ratio(self.legal,self.examples),
            "exact_configurations":self.configuration,"configuration_accuracy":ratio(self.configuration,self.examples),
            "exact_exchanges":self.exchange,"exchange_accuracy":ratio(self.exchange,self.examples),
            "exact_action_sequences":self.sequence,"action_sequence_accuracy":ratio(self.sequence,self.examples),
            "target_exchange_card_movements":self.target_cards,"predicted_exchange_card_movements":self.predicted_cards,
            "correct_exchange_card_movements":self.correct_cards,"exchange_card_precision":ratio(self.correct_cards,self.predicted_cards),
            "exchange_card_recall":ratio(self.correct_cards,self.target_cards)}})
    }
}

pub(super) fn execute_evaluation(
    checkpoint: &PinnedFileV1,
    inventory_pin: &PinnedFileV1,
    split: EvaluationSplitV1,
    embedding_transfer: Option<&ImitationEmbeddingTransferV1>,
    output: &Path,
    embeddings: &FrozenSideboardEmbeddingsV1<'_>,
    inputs: &mut Vec<Value>,
) -> Result<Value, String> {
    let model = load_sideboard_checkpoint(checkpoint, inputs)?;
    let before = model.checkpoint_sha256_v1().map_err(|e| e.to_string())?;
    let inventory_bytes = read_pin(inventory_pin, MAX_JSON_BYTES, inputs)?;
    let inventory: Value = serde_json::from_slice(&inventory_bytes).map_err(|e| e.to_string())?;
    let mut cache = BTreeMap::new();
    verify_pins(&inventory, &mut cache, inputs)?;
    let dataset = audit_dataset(&inventory, split, &cache)?;
    validate_example_play_identity(
        &dataset.records,
        &embeddings.identity_v1().weights_sha256,
        embedding_transfer,
    )?;
    let evaluated = model
        .evaluate_imitation_v1(&dataset.examples, embeddings)
        .map_err(|e| e.to_string())?;
    require(
        before == model.checkpoint_sha256_v1().map_err(|e| e.to_string())?,
        "evaluation changed model",
    )?;
    let checkpoint_after = read_bounded(&checkpoint.path, MAX_JSON_BYTES)?;
    require(
        hash(&checkpoint_after) == checkpoint.sha256,
        "checkpoint file changed during evaluation",
    )?;
    let mut total = Counts::default();
    let mut strata: BTreeMap<String, BTreeMap<String, Counts>> = BTreeMap::new();
    // Explicit empty strata preserve zero denominators, represented by null rates.
    for name in ["done_only", "movement"] {
        strata
            .entry("target_kind".into())
            .or_default()
            .entry(name.into())
            .or_default();
    }
    for name in ["2", "3"] {
        strata
            .entry("game_number".into())
            .or_default()
            .entry(name.into())
            .or_default();
    }
    let mut rows = Vec::new();
    for (index, ((row, record), component)) in evaluated
        .iter()
        .zip(&dataset.records)
        .zip(&dataset.component_ids)
        .enumerate()
    {
        total.add(row);
        let pair = array(record, "registration_pair")?;
        let matchup = format!(
            "{} / {}",
            pair[0].as_str().ok_or("invalid own deck")?,
            pair[1].as_str().ok_or("invalid opponent deck")?
        );
        let game = integer(record, "next_game_number")?.to_string();
        for (kind, name) in [
            (
                "target_kind",
                if row.done_only_target {
                    "done_only"
                } else {
                    "movement"
                },
            ),
            ("game_number", game.as_str()),
            ("connected_group", component.as_str()),
            ("matchup", matchup.as_str()),
            ("match_group", string(record, "match_group")?),
            ("teacher_plan_group", string(record, "teacher_plan_group")?),
        ] {
            strata
                .entry(kind.into())
                .or_default()
                .entry(name.into())
                .or_default()
                .add(row);
        }
        rows.push(
            json!({"split_row":index,"record":record,"component_id":component,"metrics":row}),
        );
    }
    let strata: BTreeMap<_, BTreeMap<_, _>> = strata
        .into_iter()
        .map(|(kind, groups)| {
            (
                kind,
                groups
                    .into_iter()
                    .map(|(name, count)| (name, count.json()))
                    .collect(),
            )
        })
        .collect();
    let metrics = json!({"schema":"kernel-sideboard-imitation-evaluation/v1", "split":split,
        "example_embedding_transfer":embedding_transfer,
        "independent_connected_groups":dataset.components,"aggregate":total.json(),"strata":strata,
        "metric_definitions":{"teacher_forced":"greedy action at each teacher-replayed state; action-weighted agreement and cross entropy",
            "greedy_free_running":"greedy rollout starts from the actual initial configuration and uses only its own chosen actions",
            "configuration_accuracy":"exact multiset equality of both final mainboard 60 and sideboard 15; failures remain in denominator",
            "exchange_accuracy":"exact out/in card-count multisets relative to actual initial configuration; independent of movement order",
            "exchange_card_precision_recall":"micro counted out and in movements, including duplicates; zero denominator is null",
            "uncertainty":"descriptive development diagnostic; no confidence interval or playing-strength gate"}});
    write_json(&output.join("inputs.json"), inputs)?;
    write_json(&output.join("evaluation-examples.json"), &rows)?;
    write_json(&output.join("evaluation-metrics.json"), &metrics)?;
    Ok(
        json!({"mode":"evaluate_imitation","split":split,"metrics":metrics,"inputs":inputs,
        "checkpoint_input_sha256":checkpoint.sha256,"model_state_sha256_before":before,"model_state_sha256_after":before,
        "training_steps":model.training_steps_v1(),"training_executed":false,"optimizer_mutated":false,
        "play_weights_unchanged":true,"checkpoint_file_unchanged":true,
        "example_embedding_transfer":embedding_transfer,
        "nonclaims":["hand-authored imitation agreement is not playing strength", "checkpoint prior teacher exposure must be audited separately",
            "inspected grouped development split is not a secret final test", "no checkpoint selection, tuning or promotion", "not brewing or value learning"]}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cross_player_examples_require_exact_source_and_target_opt_in() {
        let old = "a".repeat(64);
        let current = "b".repeat(64);
        let records = vec![json!({"play_weights_sha256":old})];
        assert!(validate_example_play_identity(&records, &old, None).is_ok());
        assert!(validate_example_play_identity(&records, &current, None).is_err());
        let mut transfer = ImitationEmbeddingTransferV1 {
            expected_behavior_play_weights_sha256: old.clone(),
            expected_embedding_play_weights_sha256: current.clone(),
        };
        assert!(validate_example_play_identity(&records, &current, Some(&transfer)).is_ok());
        assert!(validate_example_play_identity(&records, &old, Some(&transfer)).is_err());
        let mixed = vec![records[0].clone(), json!({"play_weights_sha256":current})];
        assert!(validate_example_play_identity(&mixed, &current, Some(&transfer)).is_err());
        transfer.expected_behavior_play_weights_sha256 = "c".repeat(64);
        assert!(validate_example_play_identity(&records, &current, Some(&transfer)).is_err());
        transfer.expected_behavior_play_weights_sha256 = "not-a-hash".into();
        assert!(validate_example_play_identity(&records, &current, Some(&transfer)).is_err());
    }

    fn dataset_fixture() -> (Value, BTreeMap<PathBuf, Vec<u8>>) {
        let registrations = json!({"A":{"mainboard":vec![0u16;60],"sideboard":vec![1u16;15]},
            "B":{"mainboard":vec![2u16;60],"sideboard":vec![3u16;15]}});
        let mut inventory = json!({"schema":"sideboard-dataset-development-inventory/v1", "counts":{"examples":2},
            "registrations":registrations,"records":[],"components":[],"inputs":{"batches":[{"dataset":{"sha256":"source"},"matches":[]}]},
            "outputs":{"all_examples":{"path":"all"},"train":{"path":"train"},"imitation_eval":{"path":"eval"}}});
        let mut cache = BTreeMap::new();
        let mut all = Vec::new();
        for (index, name) in ["A", "B"].into_iter().enumerate() {
            let registration = &registrations[name];
            let configuration = DeckConfigurationV1::new_exact_v1(
                serde_json::from_value(registration["mainboard"].clone()).unwrap(),
                serde_json::from_value(registration["sideboard"].clone()).unwrap(),
            )
            .unwrap();
            let example = SideboardImitationExampleV1 {
                input: LearnedSideboardInputV1 {
                    registered_cards: configuration.combined_card_counts_v1(),
                    own_card_outcomes: vec![],
                    opponent_evidence: vec![],
                    resource_summaries: vec![],
                    next_game_number: 2,
                    acting_player_games_won: 1,
                    opponent_games_won: 0,
                },
                initial_mainboard: configuration.mainboard().to_vec(),
                initial_sideboard: configuration.sideboard().to_vec(),
                target_actions: vec![SideboardActionV1::Done],
                target_value: None,
            };
            let line = serde_json::to_string(&example).unwrap();
            let mut bytes = line.as_bytes().to_vec();
            bytes.push(b'\n');
            let value: Value = serde_json::from_str(&line).unwrap();
            let example_sha = canonical_hash(&value).unwrap();
            let teacher_group =
                canonical_hash(&json!({"registration":registration,"target":registration}))
                    .unwrap();
            let match_config = json!({"deck_ids":[name,name],"seed":index,"game_one_chooser":0});
            let match_group = canonical_hash(
                &json!({"deck_ids":[name,name],"seed":index,"game_one_chooser":0,
                "registrations":[registration,registration]}),
            )
            .unwrap();
            let split = if index == 0 {
                "train"
            } else {
                "imitation_eval"
            };
            let record_id = format!("source:{index}:0");
            let component = format!("component-{index}");
            inventory["records"].as_array_mut().unwrap().push(json!({"record_id":record_id,"example_sha256":example_sha,
                "match_index":index,"acting_player":0,"seed":index,"registration_pair":[name,name],"play_weights_sha256":"weights",
                "match_group":match_group,"teacher_plan_group":teacher_group,"own_registration_sha256":canonical_hash(registration).unwrap(),
                "next_game_number":2,"target_value_present":false,"action_targets":1,"movement_targets":0,"split":split}));
            inventory["components"].as_array_mut().unwrap().push(json!({"component_id":component,"record_ids":[record_id],"examples":1,"split":split,
                "groups":{"match_group":[match_group],"teacher_plan_group":[teacher_group],"example_sha256":[example_sha]}}));
            let examples_path = format!("source-examples-{index}");
            let match_path = format!("source-match-{index}");
            inventory["inputs"]["batches"][0]["matches"].as_array_mut().unwrap().push(json!({
                "match_group":match_group,"examples":{"path":examples_path},"match":{"path":match_path}}));
            cache.insert(PathBuf::from(examples_path), bytes.clone());
            cache.insert(
                PathBuf::from(match_path),
                serde_json::to_vec(&json!({"config":match_config,"play_weights_sha256":"weights"}))
                    .unwrap(),
            );
            cache.insert(
                PathBuf::from(if index == 0 { "train" } else { "eval" }),
                bytes.clone(),
            );
            all.extend(bytes);
        }
        cache.insert(PathBuf::from("all"), all);
        (inventory, cache)
    }

    #[test]
    fn split_audit_rejects_assignment_source_and_teacher_group_changes() {
        let (inventory, cache) = dataset_fixture();
        let dataset = audit_dataset(&inventory, EvaluationSplitV1::ImitationEval, &cache).unwrap();
        assert_eq!(dataset.examples.len(), 1);
        assert_eq!(dataset.components, 1);
        let mut changed = inventory.clone();
        changed["records"][1]["split"] = json!("train");
        assert!(
            audit_dataset(&changed, EvaluationSplitV1::ImitationEval, &cache)
                .unwrap_err()
                .contains("split disagreement")
        );
        let mut changed_cache = cache.clone();
        changed_cache.insert(PathBuf::from("source-examples-1"), b"{}\n".to_vec());
        assert!(
            audit_dataset(&inventory, EvaluationSplitV1::ImitationEval, &changed_cache)
                .unwrap_err()
                .contains("original pinned source row")
        );
        let mut changed = inventory.clone();
        changed["records"][1]["teacher_plan_group"] =
            inventory["records"][0]["teacher_plan_group"].clone();
        assert!(
            audit_dataset(&changed, EvaluationSplitV1::ImitationEval, &cache)
                .unwrap_err()
                .contains("teacher plan group differs")
        );
        let mut changed_cache = cache;
        changed_cache.insert(PathBuf::from("eval"), b"{}\n".to_vec());
        assert!(
            audit_dataset(&inventory, EvaluationSplitV1::ImitationEval, &changed_cache)
                .unwrap_err()
                .contains("split output bytes")
        );
    }
    #[test]
    fn evaluator_config_has_no_training_or_selection_parameters() {
        let config = json!({"mode":"evaluate_imitation", "play_import":"C:/play.json", "output_directory":"C:/fresh",
            "checkpoint":{"path":"C:/model.json","sha256":"a".repeat(64)},
            "dataset_inventory":{"path":"C:/inventory.json","sha256":"b".repeat(64)},"split":"imitation_eval"});
        assert!(serde_json::from_value::<CommandV1>(config.clone()).is_ok());
        let mut mutated = config.clone();
        mutated["epochs"] = json!(1);
        assert!(serde_json::from_value::<CommandV1>(mutated).is_err());
        let mut mutated = config;
        mutated["split"] = json!("all");
        assert!(serde_json::from_value::<CommandV1>(mutated).is_err());
    }
    #[test]
    fn empty_strata_and_exchange_denominators_remain_null() {
        let value = Counts::default().json();
        assert_eq!(value["examples"], 0);
        assert!(value["teacher_forced"]["action_agreement"].is_null());
        assert!(value["greedy_free_running"]["exchange_card_precision"].is_null());
        assert!(value["greedy_free_running"]["configuration_accuracy"].is_null());
    }
}
