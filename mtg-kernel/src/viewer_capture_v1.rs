//! Bounded illustrative self-play capture. Records real steps, never search clones.
use crate::expanded_deck_training_v1::{load_expanded_inference_v1, ExpandedModelSourceV1};
use crate::ids::PlayerId;
use crate::kernel_native_search_opponent_v1::KernelNativeSearchActionStatV1;
use crate::model_guided_search_core_v1::ModelGuidedSearchDecisionV1;
use crate::model_guided_search_core_v4::{search_inner, search_viewer, Limits, ViewerEvent};
use crate::model_guided_search_outcome_v4::{lower_hex_sha256_v4, root_statistics_digest_v4};
use crate::paired_bo1_harness_v1::paired_policy_seeds_v1;
use crate::rl::{PlayerSeatV1, TerminalClassificationV1};
use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
use crate::runtime_decks::RUNTIME_DECKS;
use crate::sideboard_play_policy_v1::search_leaf_v4::V4SearchLeafEvaluatorV1;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

const SCHEMA: &str = "mtg-kernel-viewer-capture/v1";
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    source: PathBuf,
    output: PathBuf,
    primary: PathBuf,
    seed: u64,
    episode: u64,
    decks: [usize; 2],
    #[serde(default = "record_default")]
    record: bool,
    #[serde(default)]
    search_replay_steps: Vec<u64>,
    #[serde(default = "max_steps_default")]
    max_steps: u64,
    #[serde(default = "max_seconds_default")]
    max_seconds: u64,
}
fn record_default() -> bool {
    false
}
fn max_steps_default() -> u64 {
    100000
}
fn max_seconds_default() -> u64 {
    5400
}

struct Recorder {
    file: File,
    bytes: usize,
    written: u64,
    stopped: Option<String>,
    natural_complete: bool,
}
impl Recorder {
    fn open(path: &Path, header: Value) -> std::io::Result<Self> {
        let file = OpenOptions::new().write(true).create_new(true).open(path)?;
        let mut r = Self {
            file,
            bytes: 0,
            written: 0,
            stopped: None,
            natural_complete: false,
        };
        r.write(header);
        if let Some(reason) = &r.stopped {
            return Err(std::io::Error::other(reason.clone()));
        }
        Ok(r)
    }
    fn write(&mut self, mut row: Value) {
        if self.stopped.is_some() {
            return;
        }
        row["schema"] = json!(SCHEMA);
        let result = (|| -> Result<(), String> {
            let mut bytes = serde_json::to_vec(&row).map_err(|e| e.to_string())?;
            bytes.push(b'\n');
            if bytes.len() > 8 * 1024 * 1024 || self.bytes + bytes.len() > 512 * 1024 * 1024 - 4096
            {
                return Err("capture byte limit".into());
            }
            self.file.write_all(&bytes).map_err(|e| e.to_string())?;
            self.bytes += bytes.len();
            if row["kind"] == "decision" {
                self.written += 1;
            }
            Ok(())
        })();
        if let Err(e) = result {
            self.stopped = Some(e);
            eprintln!("viewer recorder disabled");
        }
    }
}
impl Drop for Recorder {
    fn drop(&mut self) {
        let footer = json!({"schema":SCHEMA,"kind":"footer","written":self.written,
            "complete":self.natural_complete && self.stopped.is_none(),
            "stop_reason":self.stopped.as_deref().or(if self.natural_complete {None} else {Some("capture ended without a natural terminal")})});
        let _ = writeln!(self.file, "{footer}");
        let _ = self.file.flush();
    }
}

pub fn run(path: &Path) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let config: Config = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if !config.source.is_absolute()
        || !config.output.is_absolute()
        || !config.primary.is_absolute()
        || config.max_steps == 0
        || config.max_steps > 100000
        || config.max_seconds == 0
        || config.max_seconds > 5400
        || config.decks.iter().any(|&i| i >= RUNTIME_DECKS.len())
        || config.search_replay_steps.len() > 3
    {
        return Err("invalid bounded capture configuration".into());
    }
    let source_bytes = std::fs::read(&config.source).map_err(|e| e.to_string())?;
    let source: ExpandedModelSourceV1 =
        serde_json::from_slice(&source_bytes).map_err(|e| e.to_string())?;
    let (mut policy, identity) = load_expanded_inference_v1(&source)?;
    policy.reset_sampling_v1(paired_policy_seeds_v1(config.seed));
    let decks = [
        &RUNTIME_DECKS[config.decks[0]],
        &RUNTIME_DECKS[config.decks[1]],
    ];
    let mut session = FastActorSessionV1::reset_with_explicit_decks_and_limits_flat_action_v3_environment_v2_with_starting_player_v1(
        config.episode, config.seed, 5000, 640000,
        [decks[0].id.to_owned(), decks[1].id.to_owned()],
        [decks[0].card_ids.to_vec(), decks[1].card_ids.to_vec()], PlayerId::P0,
    ).map_err(|e| format!("reset: {e:?}"))?;
    let cards = crate::card_def::CARD_DEFS
        .iter()
        .enumerate()
        .map(|(i, c)| {
            (
                i.to_string(),
                json!({"name":c.name,"type":format!("{:?}",c.types),"mana_cost":mana_cost(c.cost)}),
            )
        })
        .collect::<serde_json::Map<String, Value>>();
    let mut recorder = if config.record {
        let initialize = (|| -> std::io::Result<Recorder> {
            let runtime =
                crate::gameplay_trace_v1::runtime_identity().map_err(std::io::Error::other)?;
            Recorder::open(
                &config.output,
                json!({"kind":"header","identity":identity,
            "source_sha256":format!("{:x}",Sha256::digest(&source_bytes)),
            "runtime":runtime,"perspective":"p0",
            "decks":[decks[0].id,decks[1].id],"cards":cards,"search":{"tier":"T512","stability_halves":false,"depth":64}}),
            )
        })();
        match initialize {
            Ok(recorder) => Some(recorder),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err("capture destination already exists".into());
            }
            Err(error) => {
                eprintln!("viewer capture incomplete: recorder unavailable ({error})");
                None
            }
        }
    } else {
        None
    };
    let mut primary = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&config.primary)
        .map_err(|e| e.to_string())?;
    let start = Instant::now();
    let mut count = 0;
    loop {
        let decision = match session.current_response() {
            FastActorResponseV1::Terminal(t) => {
                let natural = t.terminal_classification == TerminalClassificationV1::Natural;
                writeln!(
                    primary,
                    "{}",
                    json!({"terminal":t,"core_hash":session.privileged_core_environment_hash()})
                )
                .map_err(|e| e.to_string())?;
                eprintln!(
                    "capture terminal: natural={natural}, decisions={count}, seconds={:.3}",
                    start.elapsed().as_secs_f64()
                );
                if !natural {
                    return Err("game did not reach a natural terminal".into());
                }
                if let Some(r) = &mut recorder {
                    r.natural_complete = true;
                }
                break;
            }
            FastActorResponseV1::Decision(d) => d,
        };
        if count >= config.max_steps || start.elapsed().as_secs() >= config.max_seconds {
            if let Some(r) = &mut recorder {
                r.stopped = Some("development bound reached".into());
            }
            writeln!(primary, "{}", json!({"bounded_stop":true,"decisions":count,"core_hash":session.privileged_core_environment_hash()})).map_err(|e| e.to_string())?;
            break;
        }
        let capture = if recorder.as_ref().is_some_and(|r| r.stopped.is_none()) {
            let fixed = session.viewer_observation_v1(PlayerSeatV1::P0);
            let actor = session.gameplay_trace_visible_v1(decision, true);
            match (fixed, actor) {
                (Ok(fixed), Ok((observation, semantics))) => {
                    match crate::human_bo3_v1::gameplay_trace_labels_v1(&observation, &semantics) {
                        Ok(labels) => Some((fixed, semantics, labels)),
                        Err(_) => {
                            recorder.as_mut().unwrap().stopped = Some("labels unavailable".into());
                            None
                        }
                    }
                }
                _ => {
                    recorder.as_mut().unwrap().stopped = Some("projection unavailable".into());
                    None
                }
            }
        } else {
            None
        };
        let (sampled, scores, masses) = policy.select_viewer_receipt_v1(&session)?;
        let evaluator = V4SearchLeafEvaluatorV1::new(&policy)?;
        let limits = Limits {
            simulations: 512,
            transitions: 512,
            depth: 64,
            seed: config.seed ^ decision.step.wrapping_mul(0x9e3779b97f4a7c15) ^ 0x564945574552,
        };
        let mut contexts = Vec::new();
        let mut context_bytes = 0;
        let mut replay_error = None;
        let replay_enabled = capture.is_some()
            && config.search_replay_steps.contains(&decision.step)
            && decision.acting_player == PlayerSeatV1::P0;
        let mut observe = |event: ViewerEvent<'_>| {
            if replay_error.is_some() {
                return;
            }
            let result = (|| -> Result<Value, String> {
                match event {
                    ViewerEvent::Choice {
                        session: sample,
                        simulation,
                        node,
                        action,
                    } => {
                        let d = match sample.current_response() {
                            FastActorResponseV1::Decision(d) => d,
                            _ => return Err("search context has no decision".into()),
                        };
                        let fixed = sample
                            .viewer_observation_v1(PlayerSeatV1::P0)
                            .map_err(|e| format!("{e:?}"))?;
                        let (actor, actions) = sample
                            .gameplay_trace_visible_v1(d, true)
                            .map_err(|e| format!("{e:?}"))?;
                        let labels =
                            crate::human_bo3_v1::gameplay_trace_labels_v1(&actor, &actions)
                                .map_err(|e| format!("{e:?}"))?;
                        Ok(
                            json!({"simulation":simulation,"node":node,"action":action,"actor":d.acting_player,
                            "board":fixed,"actions":actions,"labels":labels}),
                        )
                    }
                    ViewerEvent::Leaf {
                        session: sample,
                        simulation,
                        reason,
                    } => {
                        let board = sample
                            .viewer_observation_v1(PlayerSeatV1::P0)
                            .map_err(|e| format!("{e:?}"))?;
                        Ok(
                            json!({"simulation":simulation,"leaf":true,"reason":reason,"board":board}),
                        )
                    }
                }
            })();
            match result {
                Ok(row) => {
                    context_bytes += row.to_string().len();
                    if context_bytes > 6 * 1024 * 1024 {
                        replay_error = Some("search replay byte bound".into());
                    } else {
                        contexts.push(row);
                    }
                }
                Err(error) => replay_error = Some(error),
            }
        };
        let outcome = if replay_enabled {
            search_viewer(&session, limits, &evaluator, Some(&mut observe))
        } else {
            search_inner(&session, limits, &evaluator, None)
        }
        .map_err(|e| format!("search at {}: {e:?}", decision.step))?;
        if replay_error.is_some() {
            recorder.as_mut().unwrap().stopped =
                Some("search replay projection unavailable".into());
        }
        let replay = replay_enabled.then(|| json!({"schema":"mtg-kernel-search-capture/v1",
            "complete":replay_error.is_none(),"nodes":outcome.tree,"trace":outcome.trace,"contexts":contexts}));
        let selected = outcome.selected;
        let stats: Vec<_> = outcome
            .root_visits
            .iter()
            .zip(&outcome.root_value_sums)
            .enumerate()
            .map(
                |(i, (&visits, &value_sum))| KernelNativeSearchActionStatV1 {
                    flat_action_index: i as u32,
                    visits,
                    value_sum,
                    mean_value: if visits == 0 {
                        0
                    } else {
                        value_sum / i64::from(visits)
                    },
                },
            )
            .collect();
        let result = ModelGuidedSearchDecisionV1 {
            selected_index: selected,
            transitions_used: outcome.transitions,
            simulations: outcome.simulations,
            tree_node_count: outcome.nodes as u32,
            root_action_stats: stats,
            leaf_census: Default::default(),
        };
        let next = session
            .step(decision.episode_id, decision.step, selected)
            .map_err(|e| format!("step: {e:?}"))?;
        writeln!(
            primary,
            "{}",
            json!({"step":decision.step,"actor":decision.acting_player,"sampled":sampled,
            "selected":selected,"core_hash":session.privileged_core_environment_hash(),
            "root_digest":lower_hex_sha256_v4(root_statistics_digest_v4(&result))})
        )
        .map_err(|e| e.to_string())?;
        if let Some((before, semantics, labels)) = capture {
            let after = session.viewer_observation_v1(PlayerSeatV1::P0);
            if let Ok(after) = after {
                let terminal = match &next {
                    FastActorResponseV1::Terminal(t) => Some(t),
                    _ => None,
                };
                recorder.as_mut().unwrap().write(json!({"kind":"decision","decision":{
                    "episode_id":decision.episode_id,"step":decision.step,"physical_decision_id":decision.physical_decision_id,
                    "substep_index":decision.substep_index,"acting_player":decision.acting_player},"before":before,"after":after,
                    "actions":semantics,"labels":labels,"policy":{"sampled":sampled,"masses":masses,"value":scores.value},
                    "search":{"result":result,"digest":lower_hex_sha256_v4(root_statistics_digest_v4(&result)),"replay":replay},
                    "selected":selected,"terminal":terminal}));
            } else {
                recorder.as_mut().unwrap().stopped =
                    Some("transition projection unavailable".into());
            }
        }
        count += 1;
        if count % 25 == 0 {
            eprintln!("{count} decisions, {:.1}s", start.elapsed().as_secs_f64());
        }
    }
    primary.flush().map_err(|e| e.to_string())?;
    Ok(())
}

fn mana_cost(cost: crate::mana::Cost) -> String {
    use crate::mana::Pip;
    let mut s = "{X}".repeat(cost.x_count as usize);
    if cost.generic > 0 {
        s.push_str(&format!("{{{}}}", cost.generic));
    }
    for pip in cost.pips {
        let symbol = match pip {
            Pip::Colored(c) => format!("{c:?}"),
            Pip::Hybrid(a, b) => format!("{a:?}/{b:?}"),
            Pip::Phyrexian(c) | Pip::PhyrexianAnyColor(c) => format!("{c:?}/P"),
        };
        s.push_str(&format!("{{{symbol}}}"));
    }
    s
}
