//! Prospective whole-game engineering probe. Never changes the frozen V4 probe.
//! Use the guarded Python launcher after serial/parallel qualification. Reports
//! contain actor-visible fingerprints and classified failures, not native state.
use crate::{
    current_mixed_policy_v1::{KaitoSession, NumericTree, Packet, Source},
    current_tree_learner_v1::{AuthenticatedInputV1, ModelV1},
    ids::PlayerId,
    rl::{
        ActionSemanticV1, PlayerSeatV1, TerminalClassificationV1, TerminalOutcomeV1,
        TerminalSafeCodeV2,
    },
    rl_session::{
        EternalActorSessionV1, FastActorDecisionV1, FastActorResponseV1, RlSessionTerminalV1,
    },
    state::SplitMix64,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, time::Instant};

const SCHEMA: &str = "current-actor-coverage-probe/v1";
const SELECTOR: &str = "splitmix64-rejection-uniform/v1;seed=environment_seed^0x454a11;all-current-tree-fixture-logits-checked";

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Deck {
    label: String,
    mainboard_size: usize,
    mainboard: Vec<(usize, String)>,
    sideboard: Vec<(usize, String)>,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Game {
    id: String,
    historical_episode_index: usize,
    seed: u64,
    starting_player: PlayerSeatV1,
    decks: [String; 2],
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Requirement {
    decks: [String; 2],
    kinds: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: String,
    pool: String,
    cohort: String,
    selector: String,
    runtime: String,
    provenance: serde_json::Value,
    max_physical_decisions: u64,
    max_policy_steps: u64,
    decks: Vec<Deck>,
    games: Vec<Game>,
    requirements: Vec<Requirement>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Stage {
    Configuration,
    SessionCreation,
    Engine,
    Encoding,
    Scoring,
    ActionAudit,
    Incomplete,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct Header {
    step: u64,
    physical_decision: u64,
    actor: PlayerSeatV1,
    kind: String,
    legal_actions: u32,
}
impl From<FastActorDecisionV1> for Header {
    fn from(d: FastActorDecisionV1) -> Self {
        Self {
            step: d.step,
            physical_decision: d.physical_decision_id,
            actor: d.acting_player,
            kind: format!("{:?}", d.decision_kind),
            legal_actions: d.legal_action_count,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct Failure {
    stage: Stage,
    detail: String,
    header: Option<Header>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct GameResult {
    id: String,
    historical_episode_index: usize,
    seed: u64,
    starting_player: PlayerSeatV1,
    decks: [String; 2],
    completed: bool,
    decisions: u64,
    decisions_by_kind_and_seat: BTreeMap<String, u64>,
    action_kinds: BTreeMap<String, u64>,
    flags_seen: [[bool; 2]; 6],
    equal_full_ingress_distinct_actions: u64,
    equal_ordinal_neutral_ingress_distinct_actions: u64,
    collision_examples: Vec<[u32; 2]>,
    terminal: Option<RlSessionTerminalV1>,
    failure: Option<Failure>,
    actor_trace_sha256: String,
}
impl GameResult {
    fn new(game: &Game) -> Self {
        Self {
            id: game.id.clone(),
            historical_episode_index: game.historical_episode_index,
            seed: game.seed,
            starting_player: game.starting_player,
            decks: game.decks.clone(),
            completed: false,
            decisions: 0,
            decisions_by_kind_and_seat: BTreeMap::new(),
            action_kinds: BTreeMap::new(),
            flags_seen: [[false; 2]; 6],
            equal_full_ingress_distinct_actions: 0,
            equal_ordinal_neutral_ingress_distinct_actions: 0,
            collision_examples: Vec::new(),
            terminal: None,
            failure: None,
            actor_trace_sha256: String::new(),
        }
    }
    fn fail(&mut self, stage: Stage, detail: impl ToString, header: Option<FastActorDecisionV1>) {
        self.failure = Some(Failure {
            stage,
            detail: detail.to_string(),
            header: header.map(Header::from),
        });
    }
}

fn resolve(deck: &Deck, expected: usize) -> Result<Vec<u16>, String> {
    let mut ids = Vec::new();
    for (count, name) in &deck.mainboard {
        if *count == 0 || *count > expected {
            return Err(format!("invalid count for {name}"));
        }
        let id = crate::card_def::card_id_by_name(name)
            .ok_or_else(|| format!("unknown printing {name}"))?;
        if ids.len() + *count > expected {
            return Err("mainboard too large".into());
        }
        ids.extend(std::iter::repeat_n(id, *count));
    }
    if ids.len() != expected {
        return Err(format!("mainboard length {} != {expected}", ids.len()));
    }
    Ok(ids)
}

fn configured(deck: &Deck, historical: bool) -> Result<Vec<u16>, String> {
    if historical && deck.mainboard_size != 60
        || !historical && !(40..=crate::limited_session_v1::MAX_CUSTOM_DECK_CARDS_V1).contains(&deck.mainboard_size) {
        return Err("declared mainboard size is outside the selected cohort".into());
    }
    let mut main = resolve(deck, deck.mainboard_size)?;
    if historical {
        // Old RT1 sorted registrations before shuffle. Original Limited
        // recipes retain their supplied row order and actual40/41/49 sizes.
        main.sort_unstable();
        let side = resolve(
            &Deck {
                label: deck.label.clone(),
                mainboard_size: 15,
                mainboard: deck.sideboard.clone(),
                sideboard: Vec::new(),
            },
            15,
        )?;
        crate::sideboard::RegisteredDeckV1::new_executable_v1(&deck.label, main.clone(), side)
            .map_err(|error| format!("registered 60/15 preflight: {error:?}"))?;
    } else if !deck.sideboard.is_empty() {
        return Err("original Limited has no constructed sideboard".into());
    }
    Ok(main)
}

fn limbs(v: [u16; 4]) -> u64 {
    v.into_iter()
        .enumerate()
        .fold(0, |out, (i, n)| out | (u64::from(n) << (16 * i)))
}
/// Audit-only neutralization of the root CandidateMeaningV75.ordinal value.
/// It never removes nested reference ordinals, changes scoring inputs or
/// creates authority. All topology, field names and other payloads remain.
fn signature(tree: &NumericTree, neutralize: bool) -> Result<Vec<[u16; 20]>, String> {
    let rows = tree.rows();
    let topology = tree.topology();
    let mut out: Vec<[u16; 20]> = rows
        .iter()
        .zip(topology)
        .map(|(row, (parent, ordinal))| {
            let mut result = [0; 20];
            result[..12].copy_from_slice(row);
            result[12..16].copy_from_slice(parent);
            result[16..].copy_from_slice(ordinal);
            result
        })
        .collect();
    if !neutralize {
        return Ok(out);
    }
    let mut found = 0;
    for (field, row) in rows.iter().enumerate() {
        if row[0] != 7 || limbs(topology[field].0) != 1 {
            continue;
        }
        let Some(key) = rows.get(field + 1) else {
            return Err("missing candidate field key".into());
        };
        if key[0] != 9 {
            return Err("invalid candidate field key".into());
        }
        let length = limbs(key[1..5].try_into().unwrap()) as usize;
        let bytes = rows
            .get(field + 2..field + 2 + length)
            .ok_or("invalid key bytes")?;
        if length != 7
            || !bytes
                .iter()
                .zip(b"ordinal")
                .all(|(row, byte)| row[0] == 8 && row[1] == u16::from(*byte))
        {
            continue;
        }
        let value = field + 2 + length;
        if rows.get(value).is_none_or(|row| row[0] != 2)
            || limbs(topology[value].0) != (field + 1) as u64
        {
            return Err("invalid candidate ordinal".into());
        }
        out[value][1..5].fill(0);
        found += 1;
    }
    if found != 1 {
        return Err(format!(
            "expected one candidate root ordinal, found {found}"
        ));
    }
    Ok(out)
}

fn update_trace(
    hash: &mut Sha256,
    packet: &Packet,
    flags: &[[bool; 6]],
    selected: u32,
    logits: &[f64],
) {
    for tree in std::iter::once(packet.state()).chain(packet.actions()) {
        hash.update((tree.rows().len() as u64).to_le_bytes());
        for (row, (parent, ordinal)) in tree.rows().iter().zip(tree.topology()) {
            for limb in row.iter().chain(parent).chain(ordinal) {
                hash.update(limb.to_le_bytes());
            }
        }
    }
    for row in flags {
        hash.update(row.map(u8::from));
    }
    for value in logits {
        hash.update(value.to_bits().to_le_bytes());
    }
    hash.update(selected.to_le_bytes());
}

fn audit(
    input: &AuthenticatedInputV1,
    actions: &[ActionSemanticV1],
    row: &mut GameResult,
) -> Result<(), String> {
    let packet = input.packet();
    if actions.len() != packet.actions().len() || actions.len() != input.action_flags().len() {
        return Err("action/input count mismatch".into());
    }
    let signatures = packet
        .actions()
        .iter()
        .map(|tree| signature(tree, packet.source() == Source::KaitoV75))
        .collect::<Result<Vec<_>, _>>()?;
    for (i, action) in actions.iter().enumerate() {
        let value = serde_json::to_value(action).map_err(|e| e.to_string())?;
        let kind = value["action_kind"].as_str().ok_or("action kind absent")?;
        *row.action_kinds.entry(kind.into()).or_default() += 1;
        for (slot, &value) in input.action_flags()[i].iter().enumerate() {
            row.flags_seen[slot][usize::from(value)] = true;
        }
        for j in 0..i {
            if action == &actions[j] || input.action_flags()[i] != input.action_flags()[j] {
                continue;
            }
            if packet.actions()[i] == packet.actions()[j] {
                row.equal_full_ingress_distinct_actions += 1;
            }
            if signatures[i] == signatures[j] {
                row.equal_ordinal_neutral_ingress_distinct_actions += 1;
                if row.collision_examples.len() < 8 {
                    row.collision_examples.push([j as u32, i as u32]);
                }
            }
        }
    }
    Ok(())
}

enum Owner {
    Limited(KaitoSession),
    Eternal(EternalActorSessionV1),
}
enum Decision {
    Limited(crate::current_mixed_policy_v1::KaitoDecision),
    Eternal(crate::rl_session::EternalActorDecisionV1),
}
impl Owner {
    fn response(&self) -> Result<FastActorResponseV1, String> {
        match self {
            Self::Limited(s) => Ok(s.current_response()),
            Self::Eternal(s) => s.current_response().map_err(|e| e.to_string()),
        }
    }
    fn encode(
        &self,
        h: FastActorDecisionV1,
    ) -> Result<(Decision, AuthenticatedInputV1, Vec<ActionSemanticV1>), String> {
        match self {
            Self::Limited(s) => {
                let d = s.encode(h).map_err(|e| e.to_string())?;
                let input = s.training_input_v1(&d).map_err(|e| e.to_string())?;
                let actions = d.probe_actions_v1().to_vec();
                Ok((Decision::Limited(d), input, actions))
            }
            Self::Eternal(s) => {
                let d = s.encode(h).map_err(|e| e.to_string())?;
                let input = s.training_input_v1(&d).map_err(|e| e.to_string())?;
                let actions = d.probe_actions_v1();
                Ok((Decision::Eternal(d), input, actions))
            }
        }
    }
    fn apply(&mut self, decision: &Decision, selected: u32) -> Result<(), (Stage, String)> {
        match (self, decision) {
            (Self::Limited(s), Decision::Limited(d)) => s.probe_apply_v1(d, selected),
            (Self::Eternal(s), Decision::Eternal(d)) => s.probe_apply_v1(d, selected),
            _ => Err((Stage::Encoding, "foreign probe decision type".into())),
        }
    }
}

fn play(manifest: &Manifest, game: &Game) -> GameResult {
    let mut row = GameResult::new(game);
    let historical = manifest.cohort == "historical60";
    let boards = game.decks.each_ref().map(|label| {
        manifest
            .decks
            .iter()
            .find(|d| &d.label == label)
            .ok_or_else(|| format!("missing deck {label}"))
            .and_then(|deck| configured(deck, historical))
    });
    let boards = match boards {
        [Ok(a), Ok(b)] => [a, b],
        other => {
            row.fail(Stage::Configuration, format!("{other:?}"), None);
            return row;
        }
    };
    let player = match game.starting_player {
        PlayerSeatV1::P0 => PlayerId::P0,
        PlayerSeatV1::P1 => PlayerId::P1,
    };
    let limits = (manifest.max_physical_decisions, manifest.max_policy_steps);
    let session = if manifest.pool == "limited" {
        KaitoSession::reset_explicit_mainboards(
            1,
            game.seed,
            limits,
            game.decks.clone(),
            boards,
            player,
        )
        .map(Owner::Limited)
    } else {
        EternalActorSessionV1::reset_explicit_mainboards(
            1,
            game.seed,
            limits,
            game.decks.clone(),
            boards,
            player,
        )
        .map(Owner::Eternal)
    };
    let mut owner = match session {
        Ok(s) => s,
        Err(e) => {
            row.fail(Stage::SessionCreation, e, None);
            return row;
        }
    };
    let model = ModelV1::deterministic_fixture();
    let mut rng = SplitMix64::seed(game.seed ^ 0x454a11);
    let mut trace = Sha256::new();
    loop {
        let response = match owner.response() {
            Ok(r) => r,
            Err(e) => {
                row.fail(Stage::Encoding, e, None);
                break;
            }
        };
        let header = match response {
            FastActorResponseV1::Decision(h) => h,
            FastActorResponseV1::Terminal(t) => {
                row.completed = natural(&t);
                if !row.completed {
                    row.fail(
                        if t.terminal_classification == TerminalClassificationV1::Truncated {
                            Stage::Incomplete
                        } else {
                            Stage::Engine
                        },
                        &t.terminal_reason,
                        None,
                    );
                }
                row.terminal = Some(t);
                break;
            }
        };
        let (decision, input, actions) = match owner.encode(header) {
            Ok(v) => v,
            Err(e) => {
                row.fail(Stage::Encoding, e, Some(header));
                break;
            }
        };
        if actions.len() != header.legal_action_count as usize || actions.is_empty() {
            row.fail(Stage::Encoding, "header/action arity differs", Some(header));
            break;
        }
        if let Err(e) = audit(&input, &actions, &mut row) {
            row.fail(Stage::ActionAudit, e, Some(header));
            break;
        }
        let logits = match model.forward(&input) {
            Ok(v) if v.len() == actions.len() && v.iter().all(|x| x.is_finite()) => v,
            other => {
                row.fail(Stage::Scoring, format!("{other:?}"), Some(header));
                break;
            }
        };
        let n = actions.len() as u64;
        let threshold = n.wrapping_neg() % n;
        let selected = loop {
            let value = rng.next_u64();
            if value >= threshold {
                break (value % n) as u32;
            }
        };
        update_trace(
            &mut trace,
            input.packet(),
            input.action_flags(),
            selected,
            &logits,
        );
        row.decisions += 1;
        *row.decisions_by_kind_and_seat
            .entry(format!(
                "{:?}/{:?}",
                header.decision_kind, header.acting_player
            ))
            .or_default() += 1;
        if let Err((stage, e)) = owner.apply(&decision, selected) {
            row.fail(stage, e, Some(header));
            break;
        }
    }
    row.actor_trace_sha256 = format!("{:x}", trace.finalize());
    row
}

fn natural(t: &RlSessionTerminalV1) -> bool {
    t.terminal_classification == TerminalClassificationV1::Natural
        && t.terminal_code == TerminalSafeCodeV2::NaturalGameOver
        && matches!(
            t.terminal_outcome,
            TerminalOutcomeV1::P0Win | TerminalOutcomeV1::P1Win | TerminalOutcomeV1::Draw
        )
}

fn run_case(manifest: &Manifest, game: &Game) -> GameResult {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| play(manifest, game))) {
        Ok(row) => row,
        Err(payload) => {
            let detail = payload
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| payload.downcast_ref::<&str>().copied())
                .unwrap_or("non-string panic");
            let mut row = GameResult::new(game);
            row.fail(Stage::Engine, format!("native probe panic: {detail}"), None);
            row
        }
    }
}

#[test]
fn ordinal_audit_cannot_pass_from_candidate_position_or_erase_reference_flags() {
    let fixture = |ordinal, reference, include| {
        NumericTree::probe_fixture_v1(&serde_json::json!({
        "ordinal":ordinal, "action":{"kind":"include","reference":{"ordinal":reference},"include":include}
    })).unwrap()
    };
    let a = fixture(0, 4, false);
    let b = fixture(1, 4, false);
    assert_ne!(a, b);
    assert_eq!(signature(&a, true).unwrap(), signature(&b, true).unwrap());
    assert_ne!(
        signature(&a, true).unwrap(),
        signature(&fixture(1, 5, false), true).unwrap()
    );
    assert_ne!(
        signature(&a, true).unwrap(),
        signature(&fixture(1, 4, true), true).unwrap()
    );
    let eternal =
        NumericTree::probe_fixture_v1(&serde_json::json!({"action":{"include":true}})).unwrap();
    assert!(signature(&eternal, true).is_err());
    assert!(signature(&eternal, false).is_ok());
}

#[test]
#[ignore = "requires guarded current_actor_probe_v1.py and qualified allocation; no training or production admission"]
fn reported_pool_digest_free_terminal_probe() {
    let bytes =
        std::fs::read(std::env::var("MTG_CURRENT_ACTOR_PROBE_INPUT").expect("manifest path"))
            .unwrap();
    let manifest: Manifest = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(manifest.schema, SCHEMA);
    assert_eq!(manifest.selector, SELECTOR);
    assert!(matches!(manifest.pool.as_str(), "limited" | "eternal"));
    assert!(matches!(
        manifest.cohort.as_str(),
        "original_limited" | "historical60"
    ));
    assert_eq!(manifest.games.len(), 60);
    assert!(manifest.max_policy_steps > 0 && manifest.max_physical_decisions > 0);
    let count: usize = std::env::var("MTG_CURRENT_ACTOR_PROBE_COUNT")
        .unwrap()
        .parse()
        .unwrap();
    let workers: usize = std::env::var("MTG_CURRENT_ACTOR_PROBE_WORKERS")
        .unwrap()
        .parse()
        .unwrap();
    assert!(count > 0 && count <= 60 && workers > 0 && workers <= count);
    let started = Instant::now();
    let games = std::thread::scope(|scope| {
        let handles = (0..workers)
            .map(|worker| {
                let manifest = &manifest;
                scope.spawn(move || {
                    (worker..count)
                        .step_by(workers)
                        .map(|i| (i, run_case(manifest, &manifest.games[i])))
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();
        let mut rows = handles
            .into_iter()
            .flat_map(|handle| handle.join().expect("probe worker panicked"))
            .collect::<Vec<_>>();
        rows.sort_by_key(|(index, _)| *index);
        rows.into_iter().map(|(_, row)| row).collect::<Vec<_>>()
    });
    let completed = games.iter().filter(|g| g.completed).count();
    let collisions: u64 = games
        .iter()
        .map(|g| g.equal_ordinal_neutral_ingress_distinct_actions)
        .sum();
    let missing = manifest
        .requirements
        .iter()
        .flat_map(|requirement| {
            requirement.kinds.iter().filter_map(|kind| {
                let observed = games
                    .iter()
                    .filter(|game| game.decks == requirement.decks)
                    .any(|game| {
                        game.decisions_by_kind_and_seat
                            .keys()
                            .any(|key| key.starts_with(&format!("{kind}/")))
                    });
                (!observed).then(|| {
                    format!(
                        "{} vs {}: {kind}",
                        requirement.decks[0], requirement.decks[1]
                    )
                })
            })
        })
        .collect::<Vec<_>>();
    let historical = manifest.cohort == "historical60";
    let inventory = manifest
        .decks
        .iter()
        .map(|deck| {
            serde_json::json!({"label":deck.label,
        "configuration_error":configured(deck, historical).err()})
        })
        .collect::<Vec<_>>();
    let primary = serde_json::to_vec(&games).unwrap();
    let report = serde_json::json!({"schema":SCHEMA, "input_sha256":format!("{:x}",Sha256::digest(&bytes)),
        "selector":SELECTOR, "runtime":manifest.runtime, "provenance":manifest.provenance,
        "feature_identity":crate::current_tree_learner_v1::feature_identity_v1(),
        "card_db_hash":crate::card_def::KERNEL_CARDDB_HASH, "pool":manifest.pool,"cohort":manifest.cohort,
        "workers":workers,"seconds":started.elapsed().as_secs_f64(),"planned_games":60,"attempted_games":count,
        "completed_games":completed,"ordinal_neutral_collisions":collisions,
        "deck_inventory":inventory, "missing_reported_pair_kinds":missing,
        "terminal_panel_accepted":count==60 && completed==60 && collisions==0 && missing.is_empty(),
        "primary_sha256":format!("{:x}",Sha256::digest(&primary)),"games":games});
    use std::io::Write;
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(std::env::var("MTG_CURRENT_ACTOR_PROBE_OUTPUT").expect("report path"))
        .unwrap();
    output
        .write_all(&serde_json::to_vec_pretty(&report).unwrap())
        .unwrap();
    assert_eq!(
        completed, count,
        "coverage requires natural terminal games; see classified report"
    );
    assert_eq!(
        collisions, 0,
        "distinct typed actions remain indistinguishable after ordinal neutralization"
    );
    if count == 60 {
        assert!(
            missing.is_empty(),
            "reported pair/decision classes were not exercised"
        );
    }
}
