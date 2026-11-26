//! With every game-test switch unset the default `s4a-budget` rows keep
//! their bytes (collab LANES/speed-lead/gametest-20261125,
//! EXECUTOR-SUPPORT.md revision 2, section 5.1).

use super::canon2::KeyMode;
use super::refresh::tests::plain_control_fixture;
use super::refresh::{budget_case_with_executor, budget_plain_case, BudgetExtras, ExecutionMode};
use super::reuse_witness::Recording;
use super::sha;
use crate::training_math_v1::TrainingMathV1;
use serde_json::Value;

/// Drops non-scientific timings.
fn scientific(mut row: Value) -> Value {
    fn strip(row: &mut Value) {
        match row {
            Value::Object(object) => {
                for key in ["wall", "keying_seconds"] {
                    object.remove(key);
                }
                object.values_mut().for_each(strip);
            }
            Value::Array(array) => array.iter_mut().for_each(strip),
            _ => {}
        }
    }
    strip(&mut row);
    row
}

/// The keep-seven plain control and a searched game (2,000 transitions per
/// decision, total 6,000) use the original digest pins, confirmed on the
/// pre-fix executor with portable math bound on both fixture policies.
#[test]
#[ignore = "native qualification: two fixture games, one with small searches; run with cargo test --release"]
fn unset_switches_keep_default_rows_byte_identical() {
    const T: u64 = 2_000;
    let (mut policy, mut sh, src) = plain_control_fixture(0, 0, 100_000, 200_000);
    policy.set_training_math_v1(TrainingMathV1::PortableV1);
    sh.opponents[0].set_training_math_v1(TrainingMathV1::PortableV1);
    let (plain, search) = std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn_scoped(scope, || {
                let extras = BudgetExtras {
                    per_decision: Some((T, T)),
                    ..Default::default()
                };
                (
                    budget_plain_case(&sh, &policy, &src, "fixture").unwrap(),
                    budget_case_with_executor(
                        &sh,
                        &policy,
                        &src,
                        3 * T,
                        KeyMode::Repaired,
                        false,
                        None,
                        Recording::Disabled,
                        "fixture",
                        ExecutionMode::Persistent,
                        extras,
                    )
                    .unwrap(),
                )
            })
            .unwrap()
            .join()
            .unwrap()
    });
    assert!(plain.get("cpu").is_none());
    assert!(search.get("cpu").is_none());
    let digest = |row: Value| sha(serde_json::to_string(&scientific(row)).unwrap().as_bytes());
    let got = [digest(plain), digest(search)];
    assert_eq!(got, DEFAULT_ROW_PINS);
}

const DEFAULT_ROW_PINS: [&str; 2] = [
    "f7affc573c5a08df89f981dba13c86e3e26a769cc88ebbf33d1d8b6b62e348ad",
    "5f28a97a68750cf3e42d79dd76d3a8805e535c2da18388f16af00f4421dcd570",
];
