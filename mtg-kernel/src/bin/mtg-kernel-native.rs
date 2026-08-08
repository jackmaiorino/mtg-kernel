//! Frozen platform-gate entry point for the standalone native CLI.
//!
//! The product commands remain unimplemented. This slice supplies the real
//! syntax/lexical parser and the required non-Windows refusal without opening
//! a path or constructing any Store authority.

use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;
use std::process::ExitCode;

const INVALID_SYNTAX: &str = "mtg-kernel-native-cli-v2-invalid-syntax";
#[cfg(not(target_os = "windows"))]
const UNSUPPORTED_PLATFORM: &str = "native-training-store-v2-unsupported-platform";
#[cfg(target_os = "windows")]
const COMMAND_NOT_IMPLEMENTED: &str = "mtg-kernel-native-cli-v2-command-not-implemented";

#[derive(Clone, Copy)]
enum ValueKind {
    Path,
    Sha256,
    Git40,
    U63,
    GenerationIndex,
    PositiveU63,
    U64Hex,
    F32Hex,
    Literal(&'static [&'static str]),
}

struct ParsedCommand {
    command: &'static str,
    values: BTreeMap<&'static str, OsString>,
}

fn main() -> ExitCode {
    let parsed = match parse_command(env::args_os().skip(1).collect()) {
        Ok(parsed) => parsed,
        Err(()) => {
            eprintln!("{INVALID_SYNTAX}");
            return ExitCode::from(2);
        }
    };
    run_platform_entry(parsed)
}

#[cfg(not(target_os = "windows"))]
fn run_platform_entry(parsed: ParsedCommand) -> ExitCode {
    let ParsedCommand { command, values } = parsed;
    let _ = (command, values);
    eprintln!("{UNSUPPORTED_PLATFORM}");
    ExitCode::from(5)
}

#[cfg(target_os = "windows")]
fn run_platform_entry(parsed: ParsedCommand) -> ExitCode {
    // Capture is exposed by the library for the later command implementation.
    // Calling it here would create a guard that cannot yet be checked against a
    // validated run, so the capture-only slice fails before path access.
    let _ = parsed.command;
    let _ = parsed.values;
    eprintln!("{COMMAND_NOT_IMPLEMENTED}");
    ExitCode::from(3)
}

fn parse_command(arguments: Vec<OsString>) -> Result<ParsedCommand, ()> {
    let mut arguments = arguments.into_iter();
    let first = unicode(arguments.next())?;
    let second = match first.as_str() {
        "train" | "evaluate" | "experiment" => Some(unicode(arguments.next())?),
        "validate-store" | "run" => None,
        _ => return Err(()),
    };
    let command = match (first.as_str(), second.as_deref()) {
        ("train", Some("new")) => "train-new",
        ("train", Some("resume")) => "train-resume",
        ("validate-store", None) => "validate-store",
        ("run", None) => "run",
        ("evaluate", Some("pair")) => "evaluate-pair",
        ("evaluate", Some("learning-quality")) => "evaluate-learning-quality",
        ("experiment", Some("execute")) => "experiment-execute",
        _ => return Err(()),
    };
    let specification = command_specification(command);
    let mut values = BTreeMap::new();
    while let Some(raw_flag) = arguments.next() {
        let flag = unicode(Some(raw_flag))?;
        let &(canonical_flag, kind, _required) = specification
            .iter()
            .find(|(candidate, _, _)| *candidate == flag)
            .ok_or(())?;
        let value = arguments.next().ok_or(())?;
        validate_value(kind, &value)?;
        if values.insert(canonical_flag, value).is_some() {
            return Err(());
        }
    }
    if specification
        .iter()
        .any(|(flag, _, required)| *required && !values.contains_key(flag))
    {
        return Err(());
    }
    validate_optional_groups(command, &values)?;
    Ok(ParsedCommand { command, values })
}

fn unicode(value: Option<OsString>) -> Result<String, ()> {
    value.ok_or(())?.into_string().map_err(|_| ())
}

fn validate_value(kind: ValueKind, value: &OsString) -> Result<(), ()> {
    let value = value.to_str().ok_or(())?;
    match kind {
        ValueKind::Path => validate_windows_path_lexical(value),
        ValueKind::Sha256 => validate_lower_hex(value, 64),
        ValueKind::Git40 => validate_lower_hex(value, 40),
        ValueKind::U63 => validate_decimal_u63(value, false),
        ValueKind::GenerationIndex => {
            validate_decimal_u63(value, false)?;
            (value.parse::<u64>().map_err(|_| ())? <= 99_999_999)
                .then_some(())
                .ok_or(())
        }
        ValueKind::PositiveU63 => validate_decimal_u63(value, true),
        ValueKind::U64Hex => validate_lower_hex(value, 16),
        ValueKind::F32Hex => validate_lower_hex(value, 8),
        ValueKind::Literal(values) if values.contains(&value) => Ok(()),
        ValueKind::Literal(_) => Err(()),
    }
}

fn validate_lower_hex(value: &str, length: usize) -> Result<(), ()> {
    (value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
    .then_some(())
    .ok_or(())
}

fn validate_decimal_u63(value: &str, positive: bool) -> Result<(), ()> {
    if value.is_empty()
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(());
    }
    let parsed = value.parse::<u64>().map_err(|_| ())?;
    if parsed > i64::MAX as u64 || (positive && parsed == 0) {
        return Err(());
    }
    Ok(())
}

fn validate_windows_path_lexical(value: &str) -> Result<(), ()> {
    let bytes = value.as_bytes();
    if bytes.len() < 4
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1] != b':'
        || bytes[2] != b'\\'
        || bytes.contains(&b'/')
        || bytes[2..].contains(&b':')
        || value.starts_with("\\\\")
    {
        return Err(());
    }
    for component in value[3..].split('\\') {
        if component.is_empty()
            || matches!(component, "." | "..")
            || component.ends_with('.')
            || component.ends_with(' ')
            || component.chars().any(char::is_control)
            || component
                .bytes()
                .any(|byte| matches!(byte, b'<' | b'>' | b'"' | b'|' | b'?' | b'*'))
            || is_reserved_windows_component(component)
        {
            return Err(());
        }
    }
    Ok(())
}

fn is_reserved_windows_component(component: &str) -> bool {
    let base = component.split('.').next().unwrap_or(component);
    let upper = base.to_ascii_uppercase();
    if matches!(
        upper.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$" | "CONIN$" | "CONOUT$"
    ) {
        return true;
    }
    for prefix in ["COM", "LPT"] {
        if let Some(suffix) = upper.strip_prefix(prefix) {
            if matches!(
                suffix,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            ) {
                return true;
            }
        }
    }
    false
}

fn validate_optional_groups(
    command: &str,
    values: &BTreeMap<&'static str, OsString>,
) -> Result<(), ()> {
    if command == "run" {
        for (policy, root, reference) in [
            ("--p0-policy", "--p0-store-root", "--p0-checkpoint-ref"),
            ("--p1-policy", "--p1-store-root", "--p1-checkpoint-ref"),
        ] {
            let policy = values
                .get(policy)
                .and_then(|value| value.to_str())
                .ok_or(())?;
            let has_root = values.contains_key(root);
            let has_reference = values.contains_key(reference);
            if has_root != has_reference || ((policy == "uniform") == has_root) {
                return Err(());
            }
        }
    }
    if command == "experiment-execute" {
        let optional = [
            "--predeclaration",
            "--expected-predeclaration-sha256",
            "--kstar-selector",
            "--expected-kstar-selector-sha256",
        ];
        let present = optional
            .iter()
            .filter(|flag| values.contains_key(**flag))
            .count();
        if present != 0 && present != optional.len() {
            return Err(());
        }
    }
    Ok(())
}

type FlagSpec = (&'static str, ValueKind, bool);

fn command_specification(command: &str) -> &'static [FlagSpec] {
    match command {
        "train-new" => TRAIN_NEW,
        "train-resume" => TRAIN_RESUME,
        "validate-store" => VALIDATE_STORE,
        "run" => RUN,
        "evaluate-pair" => EVALUATE_PAIR,
        "evaluate-learning-quality" => EVALUATE_LEARNING_QUALITY,
        "experiment-execute" => EXPERIMENT_EXECUTE,
        _ => &[],
    }
}

const PATH: ValueKind = ValueKind::Path;
const SHA: ValueKind = ValueKind::Sha256;
const GIT: ValueKind = ValueKind::Git40;
const U63: ValueKind = ValueKind::U63;
const GENERATION: ValueKind = ValueKind::GenerationIndex;
const COUNT: ValueKind = ValueKind::PositiveU63;
const U64_HEX: ValueKind = ValueKind::U64Hex;
const F32_HEX: ValueKind = ValueKind::F32Hex;
const RALLY: ValueKind = ValueKind::Literal(&["Rally"]);
const POLICY: ValueKind = ValueKind::Literal(&["uniform", "greedy", "sampled-b"]);

const TRAIN_NEW: &[FlagSpec] = &[
    ("--store-root", PATH, true),
    ("--source-root", PATH, true),
    ("--expected-git-commit", GIT, true),
    ("--expected-source-tree-sha256", SHA, true),
    ("--expected-binary-sha256", SHA, true),
    ("--snapshot-manifest", PATH, true),
    ("--snapshot-payload", PATH, true),
    ("--expected-snapshot-sha256", SHA, true),
    ("--expected-snapshot-payload-sha256", SHA, true),
    ("--expected-runtime-catalog-sha256", SHA, true),
    ("--expected-card-db-hash-u64-hex", U64_HEX, true),
    ("--deck-p0", RALLY, true),
    ("--deck-p1", RALLY, true),
    ("--expected-deck-p0-hash-u64-hex", U64_HEX, true),
    ("--expected-deck-p1-hash-u64-hex", U64_HEX, true),
    ("--base-seed", U63, true),
    ("--batch-episodes", COUNT, true),
    ("--checkpoint-segment-updates", COUNT, true),
    ("--successful-updates", COUNT, true),
    ("--max-physical-decisions", COUNT, true),
    ("--max-policy-steps", COUNT, true),
    ("--worker-count", COUNT, true),
    ("--sessions-per-worker", COUNT, true),
    ("--broker-batch-target", COUNT, true),
    ("--scheduler-timeout-ms", COUNT, true),
    ("--learning-rate-f32-bits", F32_HEX, true),
    ("--value-coefficient-f32-bits", F32_HEX, true),
    ("--beta1-f32-bits", F32_HEX, true),
    ("--beta2-f32-bits", F32_HEX, true),
    ("--epsilon-f32-bits", F32_HEX, true),
    ("--weight-decay-f32-bits", F32_HEX, true),
];

const TRAIN_RESUME: &[FlagSpec] = &[
    ("--store-root", PATH, true),
    ("--source-root", PATH, true),
    ("--expected-run-sha256", SHA, true),
    ("--expected-generation-index", GENERATION, true),
    ("--expected-head-sha256", SHA, true),
    ("--no-op-proof", PATH, true),
];

const VALIDATE_STORE: &[FlagSpec] = &[
    ("--store-root", PATH, true),
    ("--source-root", PATH, true),
    ("--expected-run-sha256", SHA, true),
    ("--expected-generation-index", GENERATION, true),
    ("--expected-head-sha256", SHA, true),
];

const RUN: &[FlagSpec] = &[
    ("--output-root", PATH, true),
    ("--source-root", PATH, true),
    ("--expected-git-commit", GIT, true),
    ("--expected-source-tree-sha256", SHA, true),
    ("--expected-binary-sha256", SHA, true),
    ("--episodes", COUNT, true),
    ("--base-seed", U63, true),
    ("--expected-runtime-catalog-sha256", SHA, true),
    ("--expected-card-db-hash-u64-hex", U64_HEX, true),
    ("--deck-p0", RALLY, true),
    ("--deck-p1", RALLY, true),
    ("--expected-deck-p0-hash-u64-hex", U64_HEX, true),
    ("--expected-deck-p1-hash-u64-hex", U64_HEX, true),
    ("--p0-policy", POLICY, true),
    ("--p1-policy", POLICY, true),
    ("--p0-store-root", PATH, false),
    ("--p0-checkpoint-ref", PATH, false),
    ("--p1-store-root", PATH, false),
    ("--p1-checkpoint-ref", PATH, false),
    ("--max-physical-decisions", COUNT, true),
    ("--max-policy-steps", COUNT, true),
    ("--worker-count", COUNT, true),
    ("--sessions-per-worker", COUNT, true),
    ("--broker-batch-target", COUNT, true),
    ("--scheduler-timeout-ms", COUNT, true),
];

const EVALUATE_PAIR: &[FlagSpec] = &[
    ("--output-root", PATH, true),
    ("--source-root", PATH, true),
    ("--expected-git-commit", GIT, true),
    ("--expected-source-tree-sha256", SHA, true),
    ("--expected-binary-sha256", SHA, true),
    ("--candidate-store-root", PATH, true),
    ("--candidate-checkpoint-ref", PATH, true),
    ("--comparator-store-root", PATH, true),
    ("--comparator-checkpoint-ref", PATH, true),
    (
        "--comparator-kind",
        ValueKind::Literal(&["update_zero", "explicit"]),
        true,
    ),
    (
        "--selection",
        ValueKind::Literal(&["greedy", "sampled-b"]),
        true,
    ),
    ("--pairs", COUNT, true),
    ("--base-seed", U63, true),
    ("--bootstrap-replicates", COUNT, true),
    ("--expected-runtime-catalog-sha256", SHA, true),
    ("--expected-card-db-hash-u64-hex", U64_HEX, true),
    ("--deck-p0", RALLY, true),
    ("--deck-p1", RALLY, true),
    ("--expected-deck-p0-hash-u64-hex", U64_HEX, true),
    ("--expected-deck-p1-hash-u64-hex", U64_HEX, true),
    ("--max-physical-decisions", COUNT, true),
    ("--max-policy-steps", COUNT, true),
    ("--worker-count", COUNT, true),
    ("--sessions-per-worker", COUNT, true),
    ("--broker-batch-target", COUNT, true),
    ("--scheduler-timeout-ms", COUNT, true),
];

const EVALUATE_LEARNING_QUALITY: &[FlagSpec] = &[
    ("--output-root", PATH, true),
    ("--source-root", PATH, true),
    ("--expected-git-commit", GIT, true),
    ("--expected-source-tree-sha256", SHA, true),
    ("--expected-binary-sha256", SHA, true),
    ("--predeclaration", PATH, true),
    ("--expected-predeclaration-sha256", SHA, true),
    ("--kstar-selector", PATH, true),
    ("--expected-kstar-selector-sha256", SHA, true),
    ("--inputs-manifest", PATH, true),
    ("--expected-inputs-manifest-sha256", SHA, true),
    ("--operational-locator-map", PATH, true),
];

const EXPERIMENT_EXECUTE: &[FlagSpec] = &[
    ("--store-root", PATH, true),
    ("--experiment-root", PATH, true),
    ("--source-root", PATH, true),
    ("--plan", PATH, true),
    ("--expected-plan-sha256", SHA, true),
    ("--snapshot-manifest", PATH, true),
    ("--snapshot-payload", PATH, true),
    ("--predeclaration", PATH, false),
    ("--expected-predeclaration-sha256", SHA, false),
    ("--kstar-selector", PATH, false),
    ("--expected-kstar-selector-sha256", SHA, false),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn os(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    fn valid_value(kind: ValueKind) -> OsString {
        match kind {
            ValueKind::Path => OsString::from("C:\\authority"),
            ValueKind::Sha256 => OsString::from("a".repeat(64)),
            ValueKind::Git40 => OsString::from("b".repeat(40)),
            ValueKind::U63 | ValueKind::GenerationIndex => OsString::from("0"),
            ValueKind::PositiveU63 => OsString::from("1"),
            ValueKind::U64Hex => OsString::from("c".repeat(16)),
            ValueKind::F32Hex => OsString::from("3f800000"),
            ValueKind::Literal(values) => OsString::from(values[0]),
        }
    }

    fn complete_command(command: &str) -> Vec<OsString> {
        let mut arguments: Vec<OsString> = match command {
            "train-new" => os(&["train", "new"]),
            "train-resume" => os(&["train", "resume"]),
            "validate-store" => os(&["validate-store"]),
            "run" => os(&["run"]),
            "evaluate-pair" => os(&["evaluate", "pair"]),
            "evaluate-learning-quality" => os(&["evaluate", "learning-quality"]),
            "experiment-execute" => os(&["experiment", "execute"]),
            _ => panic!("unknown test command"),
        };
        for &(flag, kind, required) in command_specification(command) {
            if required {
                arguments.push(OsString::from(flag));
                arguments.push(valid_value(kind));
            }
        }
        arguments
    }

    #[test]
    fn every_active_command_has_one_complete_lexical_parse() {
        for command in [
            "train-new",
            "train-resume",
            "validate-store",
            "run",
            "evaluate-pair",
            "evaluate-learning-quality",
            "experiment-execute",
        ] {
            let parsed = parse_command(complete_command(command)).unwrap();
            assert_eq!(parsed.command, command);
        }
    }

    #[test]
    fn validate_store_parses_without_filesystem_access() {
        let parsed = parse_command(os(&[
            "validate-store",
            "--store-root",
            "C:\\store",
            "--source-root",
            "C:\\source",
            "--expected-run-sha256",
            &"a".repeat(64),
            "--expected-generation-index",
            "0",
            "--expected-head-sha256",
            &"b".repeat(64),
        ]))
        .unwrap();
        assert_eq!(parsed.command, "validate-store");
    }

    #[test]
    fn syntax_rejects_unknown_duplicate_and_noncanonical_values() {
        assert!(parse_command(os(&["unknown"])).is_err());
        let mut duplicate = complete_command("validate-store");
        duplicate.extend(os(&["--store-root", "C:\\other"]));
        assert!(parse_command(duplicate).is_err());

        let mut unknown_flag = complete_command("validate-store");
        unknown_flag.extend(os(&["--unknown", "value"]));
        assert!(parse_command(unknown_flag).is_err());

        assert!(validate_decimal_u63("01", false).is_err());
        assert!(validate_lower_hex(&"A".repeat(64), 64).is_err());
        assert!(validate_windows_path_lexical("relative\\path").is_err());
        assert!(validate_windows_path_lexical("C:\\COM1.txt").is_err());
        assert!(validate_windows_path_lexical("C:\\lpt³").is_err());
        assert!(validate_windows_path_lexical("C:\\del\u{7f}").is_err());
        assert!(validate_windows_path_lexical("C:\\c1\u{85}").is_err());
        assert!(validate_value(ValueKind::GenerationIndex, &OsString::from("100000000")).is_err());
    }

    #[test]
    fn run_policy_pairs_are_closed() {
        let mut values = BTreeMap::new();
        values.insert("--p0-policy", OsString::from("uniform"));
        values.insert("--p1-policy", OsString::from("greedy"));
        values.insert("--p1-store-root", OsString::from("C:\\store"));
        values.insert("--p1-checkpoint-ref", OsString::from("C:\\ref.json"));
        assert!(validate_optional_groups("run", &values).is_ok());
        values.insert("--p0-store-root", OsString::from("C:\\store"));
        values.insert("--p0-checkpoint-ref", OsString::from("C:\\ref.json"));
        assert!(validate_optional_groups("run", &values).is_err());
    }

    #[test]
    fn experiment_optional_authority_group_is_all_or_none() {
        let mut values = BTreeMap::new();
        assert!(validate_optional_groups("experiment-execute", &values).is_ok());
        values.insert("--predeclaration", OsString::from("C:\\pre.json"));
        assert!(validate_optional_groups("experiment-execute", &values).is_err());
        values.insert(
            "--expected-predeclaration-sha256",
            OsString::from("a".repeat(64)),
        );
        values.insert("--kstar-selector", OsString::from("C:\\selector.json"));
        values.insert(
            "--expected-kstar-selector-sha256",
            OsString::from("b".repeat(64)),
        );
        assert!(validate_optional_groups("experiment-execute", &values).is_ok());
    }
}
