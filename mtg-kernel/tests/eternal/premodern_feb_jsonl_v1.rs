use mtg_kernel::{
    card_def::KERNEL_CARDDB_HASH,
    premodern_ordered_public_v1::{
        feb::PROFILE_V1,
        feb_jsonl::{PROTOCOL, SCHEMA_VERSION},
    },
    rl::PlayerSeatV1,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};

struct Process {
    child: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
    request_id: u64,
}

impl Process {
    fn start() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_kernel_limited_env"))
            .arg("--premodern-feb-v1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take();
        let output = BufReader::new(child.stdout.take().unwrap());
        Self {
            child,
            input,
            output,
            request_id: 0,
        }
    }

    fn request(&mut self, mut body: Value) -> Value {
        let id = self.request_id.to_string();
        self.request_id += 1;
        body["request_id"] = json!(id);
        body["schema_version"] = json!(SCHEMA_VERSION);
        let input = self.input.as_mut().unwrap();
        writeln!(input, "{body}").unwrap();
        input.flush().unwrap();
        let mut line = String::new();
        assert_ne!(
            self.output.read_line(&mut line).unwrap(),
            0,
            "FEB exited before reply {id}"
        );
        assert!(line.len() <= mtg_kernel::limited_session_v1::MAX_LIMITED_LINE_BYTES_V1);
        let reply: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(reply["protocol"], PROTOCOL);
        assert_eq!(reply["profile"], PROFILE_V1);
        assert_eq!(reply["card_db_hash"], KERNEL_CARDDB_HASH);
        assert_eq!(reply["request_id"], id);
        reply
    }

    fn finish(mut self) {
        drop(self.input.take());
        assert!(self.child.wait().unwrap().success());
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn rank(label: &str, ordinal: usize) -> (u8, usize) {
    let attacks = label.starts_with("Attack with ") && label != "Attack with none";
    let rank = if label == "Keep this hand"
        || label.starts_with("Play ")
        || attacks
        || (label.starts_with("Block ") && !label.ends_with(" with none"))
    {
        0
    } else if label.starts_with("Begin casting ") {
        1
    } else if label == "Pass priority" {
        3
    } else if label == "Take a mulligan"
        || label.starts_with("Activate the mana ability")
        || label.contains(": ")
        || label.starts_with("Tap ")
        || label.starts_with("Put a -0/-1 counter")
    {
        4
    } else {
        2
    };
    (
        rank,
        if attacks {
            usize::MAX - ordinal
        } else {
            ordinal
        },
    )
}

type TraceRow = ([u8; 32], usize);

fn play(starting: PlayerSeatV1, replay: Option<&[TraceRow]>) -> (Value, Vec<TraceRow>) {
    const MAX_ACTIONS: usize = 65_536;
    let mut process = Process::start();
    let reset = json!({"request_type": "reset", "expected_card_db_hash": KERNEL_CARDDB_HASH,
        "expected_profile": PROFILE_V1, "episode_id": 460, "seed": 20261108,
        "starting_player": starting});
    let mut reply = process.request(reset.clone());
    assert_eq!(reply["state"]["prompt"]["actor"], json!(starting));
    assert_eq!(
        process.request(json!({"request_type": "current"}))["state"],
        reply["state"]
    );
    let saved = process.request(json!({"request_type": "snapshot"}));
    assert_eq!(saved["state"], reply["state"]);
    let restore = json!({"request_type": "restore", "snapshot_id": saved["snapshot_id"]});
    let refreshed = process.request(restore.clone());
    assert_ne!(refreshed["state"]["prompt"], reply["state"]["prompt"]);
    for field in ["numeric", "human"] {
        assert_eq!(refreshed["state"][field], reply["state"][field]);
    }
    let stale = process.request(
        json!({"request_type": "step", "prompt": reply["state"]["prompt"],
        "native_ordinal": 0, "submission": "human"}),
    );
    assert_eq!(stale["code"], "stale_decision");
    reply = refreshed;

    let mut trace = Vec::new();
    let mut cast = [false; 2];
    for step in 0..=MAX_ACTIONS {
        let state = &reply["state"];
        if state["state_type"] == "terminal" {
            assert!(cast.into_iter().all(|value| value));
            if let Some(replay) = replay {
                assert_eq!(trace.as_slice(), replay);
            }
            let winner = state["winner"].clone();
            let terminal = process.request(json!({"request_type": "current"}));
            assert_eq!(terminal["state"], *state);
            let _ = process.request(reset);
            assert_eq!(process.request(restore)["code"], "unknown_snapshot");
            process.finish();
            return (winner, trace);
        }
        assert_eq!(
            state["state_type"], "decision",
            "FEB stopped at step {step}: {reply}"
        );
        assert!(step < MAX_ACTIONS, "FEB reached the terminal replay cap");
        let prompt = &state["prompt"];
        assert_eq!(prompt["prompt_seq"], step);
        let human = state["human"]["actions"].as_array().unwrap();
        assert_eq!(
            state["numeric"]["actions"].as_array().unwrap().len(),
            human.len()
        );
        let labels = human
            .iter()
            .enumerate()
            .map(|(ordinal, row)| {
                assert_eq!(row["native_ordinal"], ordinal);
                assert_eq!(
                    row["kind"], "supported",
                    "unsupported FEB meaning at step {step}: {row}"
                );
                row["label"].as_str().unwrap()
            })
            .collect::<Vec<_>>();
        let ordinal = match replay {
            Some(rows) => rows[step].1,
            None => {
                labels
                    .iter()
                    .enumerate()
                    .min_by_key(|&(ordinal, label)| rank(label, ordinal))
                    .unwrap()
                    .0
            }
        };
        let digest =
            Sha256::digest(serde_json::to_vec(&(&state["numeric"], &state["human"])).unwrap())
                .into();
        let row = (digest, ordinal);
        if let Some(replay) = replay {
            assert_eq!(row, replay[step], "FEB replay diverged at step {step}");
        }
        trace.push(row);
        if labels[ordinal].starts_with("Begin casting ") {
            cast[usize::from(prompt["actor"] == "p1")] = true;
        }
        let action = json!({"request_type": "step", "prompt": prompt,
            "native_ordinal": ordinal, "submission": if replay.is_some() { "numeric" } else { "human" }});
        reply = process.request(action.clone());
        assert_eq!(reply["receipt"]["native_ordinal"], ordinal);
        if step == 0 {
            let retry = process.request(action);
            assert_eq!(retry["receipt"], reply["receipt"]);
            assert_eq!(retry["state"], reply["state"]);
        }
    }
    unreachable!("the loop fails at the action cap")
}

#[test]
fn feb_jsonl_subprocess_reaches_terminal_and_replays_both_starting_seats() {
    for starting in [PlayerSeatV1::P0, PlayerSeatV1::P1] {
        let (winner, trace) = play(starting, None);
        let (replayed, _) = play(starting, Some(&trace));
        assert_eq!(replayed, winner);
    }
}
