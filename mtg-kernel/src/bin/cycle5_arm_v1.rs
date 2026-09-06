//! Cycle-5 arm launcher CLI (`docs/native_cycle5_arm_launcher_v1.md`
//! Section 4). One invocation seeds one arm's Store, or runs exactly one
//! refresh interval of one arm, and exits; the wrapper loops, never this
//! process.
//!
//! Strict flag parsing into a typed request, following
//! `checkpoint_shadow_stdio_v1.rs`: order-independent name/value pairs, no
//! positional arguments, no defaults, every flag at most once, and no
//! environment variable is ever read for configuration. `--device` is the
//! single exception in kind, not in direction: it WRITES
//! `CUDA_VISIBLE_DEVICES` for this process only, before any CUDA
//! initialization can happen (there is no library device parameter to
//! inherit), and it is never read back as configuration.
//!
//! Exit codes: 0 complete, 2 usage, 3 contract rejection, 1 runtime failure.
//!
//! Generation numbering: `--stop-generation` is a STORE generation
//! (`0 ..= 2048`), not the contract's trainee-local numbering. The arm's
//! Store genesis is published at generation 0 carrying the cycle-3 g896
//! weights, so trainee-local 896 is store generation 0 and trainee-local
//! 2944 is store generation 2048. The launcher proves
//! `--stop-generation` names a whole interval (a multiple of 128 at or below
//! 2048) and that the Store resumes at a checkpoint-segment boundary inside
//! that interval before training. An interrupted attempt is restarted against
//! the SAME `--stop-generation` it was first given, whatever boundary its
//! Store reached; a Store already at the stop completes idempotently.
//!
//! Bounded preflight provision (Section 6's CONTROL preflight ladder): the
//! ladder needs two SHORT updates per throwaway Store prefix, which the
//! 128-generation interval check forbids. `--preflight-updates N` relaxes it
//! to `--stop-generation == resume_position + N` for `N` in 1..=8, and is
//! accepted ONLY together with the value-less `--preflight` marker flag --
//! two independent statements of the same intent, so no single typo can
//! weaken a pre-registered constant. The library additionally pins each Store
//! prefix to one mode: a prefix a formal run trained refuses the relaxed
//! check, and a prefix a preflight trained refuses to become formal.
//!
//! Build identity (`--print-build-identity`, value-less, accepted ALONE):
//! writes this binary's embedded build tuple as canonical JSON to stdout and
//! exits 0, having read nothing and touched no device. It exists so
//! `cycle5_run_record_v1` can refuse to build a record naming an arm binary
//! from a different build than its own; the arm separately requires, at
//! every launch, that the run record's provenance is exactly this build's.
//!
//! Genesis bootstrap (`--bootstrap-genesis`, value-less, mutually exclusive
//! with every interval flag): the genesis refresh manifest's own-run slot has
//! to bind the arm's own generation-0 checkpoint, which cannot exist until
//! the Store does, and an interval invocation cannot open a Store without a
//! manifest. This mode breaks that circularity: it validates the run and
//! device contracts exactly as an interval would, seeds genesis from the
//! pinned parent through the locator's `genesis_parent_store_root`, publishes
//! `arm-origin.record.json` carrying that checkpoint's identity, claims the
//! Store prefix, runs the final-store validation, and exits 0 having trained
//! nothing. The refresh builder then authors `refresh-00.manifest.json` from
//! that identity. On a Store that already holds a genesis it is exit 3.

use mtg_kernel::native_cycle5_arm_v1::{
    cycle5_arm_build_identity_json_v1, run_native_cycle5_arm_bootstrap_genesis_v1,
    run_native_cycle5_arm_check_slot_locator_v1, run_native_cycle5_arm_v1,
    Cycle5ArmBootstrapRequestV1, Cycle5ArmKindV1, Cycle5ArmRequestV1,
};
use std::ffi::OsString;
use std::path::PathBuf;

fn usage_v1() -> ! {
    eprintln!(
        "usage: cycle5_arm_v1 --arm (control-v3|centered-v5) --store-root PATH --run-record PATH --chain-dir PATH --slot-locator PATH --device N (--bootstrap-genesis | --refresh-manifest PATH [--payoff-panel PATH] --stop-generation N [--preflight --preflight-updates N])\n   or: cycle5_arm_v1 --check-slot-locator PATH\n   or: cycle5_arm_v1 --print-build-identity"
    );
    std::process::exit(2);
}

/// The two things this bin can be asked to do. A bootstrap takes none of the
/// interval flags, and an interval takes none of the bootstrap's absence of
/// them; mixing the two is usage, never a silently-preferred default.
enum ModeV1 {
    BootstrapGenesis,
    Interval {
        refresh_manifest: PathBuf,
        payoff_panel: Option<PathBuf>,
        stop_generation: u64,
        preflight_updates: Option<u64>,
    },
}

struct ParsedArgsV1 {
    arm: Cycle5ArmKindV1,
    store_root: PathBuf,
    run_record: PathBuf,
    chain_dir: PathBuf,
    slot_locator: PathBuf,
    device: u64,
    mode: ModeV1,
}

#[allow(clippy::too_many_lines)]
fn parse_args_v1(raw: Vec<OsString>) -> Result<ParsedArgsV1, ()> {
    if raw.is_empty() {
        return Err(());
    }
    let mut bootstrap_genesis = false;
    let mut preflight = false;
    let mut preflight_updates: Option<u64> = None;
    let mut arm: Option<Cycle5ArmKindV1> = None;
    let mut store_root: Option<PathBuf> = None;
    let mut run_record: Option<PathBuf> = None;
    let mut chain_dir: Option<PathBuf> = None;
    let mut refresh_manifest: Option<PathBuf> = None;
    let mut payoff_panel: Option<PathBuf> = None;
    let mut slot_locator: Option<PathBuf> = None;
    let mut stop_generation: Option<u64> = None;
    let mut device: Option<u64> = None;

    // `--preflight` and `--bootstrap-genesis` are the value-less flags, so the
    // command line is walked by index (the `cycle5_refresh_build_v1.rs`
    // `--genesis` shape) rather than by fixed name/value pairs; every other
    // flag still consumes exactly one following value and may appear at most
    // once.
    let mut index = 0;
    while index < raw.len() {
        let flag = raw[index].to_str().ok_or(())?;
        if flag == "--preflight" {
            if preflight {
                return Err(());
            }
            preflight = true;
            index += 1;
            continue;
        }
        if flag == "--bootstrap-genesis" {
            if bootstrap_genesis {
                return Err(());
            }
            bootstrap_genesis = true;
            index += 1;
            continue;
        }
        let value = raw.get(index + 1).ok_or(())?;
        match flag {
            "--arm" if arm.is_none() => {
                arm = Some(Cycle5ArmKindV1::from_wire_v1(value.to_str().ok_or(())?).ok_or(())?);
            }
            "--store-root" if store_root.is_none() => store_root = Some(PathBuf::from(value)),
            "--run-record" if run_record.is_none() => run_record = Some(PathBuf::from(value)),
            "--chain-dir" if chain_dir.is_none() => chain_dir = Some(PathBuf::from(value)),
            "--refresh-manifest" if refresh_manifest.is_none() => {
                refresh_manifest = Some(PathBuf::from(value));
            }
            "--payoff-panel" if payoff_panel.is_none() => {
                payoff_panel = Some(PathBuf::from(value));
            }
            "--slot-locator" if slot_locator.is_none() => slot_locator = Some(PathBuf::from(value)),
            "--stop-generation" if stop_generation.is_none() => {
                stop_generation = Some(value.to_str().ok_or(())?.parse::<u64>().map_err(|_| ())?);
            }
            "--device" if device.is_none() => {
                device = Some(value.to_str().ok_or(())?.parse::<u64>().map_err(|_| ())?);
            }
            "--preflight-updates" if preflight_updates.is_none() => {
                preflight_updates = Some(value.to_str().ok_or(())?.parse::<u64>().map_err(|_| ())?);
            }
            _ => return Err(()),
        }
        index += 2;
    }

    let mode = if bootstrap_genesis {
        // A bootstrap trains nothing, so every interval flag is meaningless
        // to it. Rejecting them is what keeps "seed the Store" and "train one
        // interval" from ever being the same command line with a typo.
        if refresh_manifest.is_some()
            || payoff_panel.is_some()
            || stop_generation.is_some()
            || preflight
            || preflight_updates.is_some()
        {
            return Err(());
        }
        ModeV1::BootstrapGenesis
    } else {
        // The relaxed interval check is only ever reachable when the operator
        // asked for it twice, in two different ways. Either half alone is a
        // truncated or accidental command line, never an intent to relax a
        // pre-registered constant.
        if preflight != preflight_updates.is_some() {
            return Err(());
        }
        ModeV1::Interval {
            refresh_manifest: refresh_manifest.ok_or(())?,
            payoff_panel,
            stop_generation: stop_generation.ok_or(())?,
            preflight_updates,
        }
    };

    Ok(ParsedArgsV1 {
        arm: arm.ok_or(())?,
        store_root: store_root.ok_or(())?,
        run_record: run_record.ok_or(())?,
        chain_dir: chain_dir.ok_or(())?,
        slot_locator: slot_locator.ok_or(())?,
        device: device.ok_or(())?,
        mode,
    })
}

fn main() {
    // Handled before argument parsing proper: `--print-build-identity` is a
    // whole-command-line mode, not a flag that combines with others, and it
    // must not require any of the mandatory flags.
    let raw_first: Vec<OsString> = std::env::args_os().skip(1).collect();
    if raw_first.len() == 1 && raw_first[0] == *"--print-build-identity" {
        match cycle5_arm_build_identity_json_v1() {
            Ok(json) => {
                // `print!`, not `println!`: the canonical encoding already
                // ends with the LF it requires, and a second one would make
                // the bytes non-canonical for the reader.
                print!("{json}");
                std::process::exit(0);
            }
            Err(error) => {
                eprintln!("cycle5_arm_v1: {error}");
                std::process::exit(error.exit_code_v1());
            }
        }
    }

    // `--check-slot-locator PATH` is likewise a whole-command-line mode
    // accepted ALONE, for the same reason and with the same shape. Round F
    // item 3: it decodes the eight slot Stores' run records and the genesis
    // parent's and exits, so a launcher can prove the inputs a later
    // opponent-slot resolution depends on BEFORE it spends two five-minute
    // genesis bootstraps discovering one of them is unreadable. Accepting it
    // alone is what makes "this touched no Store and no GPU" a property of
    // the command line rather than a claim about the code: none of the
    // mandatory flags is present, so none of the mutating paths is
    // reachable, and no `CUDA_VISIBLE_DEVICES` is written below.
    if raw_first.len() == 2 && raw_first[0] == *"--check-slot-locator" {
        match run_native_cycle5_arm_check_slot_locator_v1(&PathBuf::from(&raw_first[1])) {
            Ok(outcome) => {
                println!(
                    "check_slot_locator=1 decoded_run_records={} genesis_parent_checked={}",
                    outcome.decoded_run_record_count,
                    u8::from(outcome.genesis_parent_checked),
                );
                std::process::exit(0);
            }
            Err(error) => {
                eprintln!("cycle5_arm_v1: {error}");
                std::process::exit(error.exit_code_v1());
            }
        }
    }

    let raw: Vec<OsString> = std::env::args_os().skip(1).collect();
    let args = parse_args_v1(raw).unwrap_or_else(|()| usage_v1());

    // Pin this process to exactly one GPU before anything can initialize
    // CUDA. Set, never read: no configuration comes from the environment.
    // SAFETY: single-threaded process start, strictly before any CUDA
    // context, worker thread, or library call exists.
    unsafe {
        std::env::set_var("CUDA_VISIBLE_DEVICES", args.device.to_string());
    }

    let result = match args.mode {
        ModeV1::BootstrapGenesis => {
            let request = Cycle5ArmBootstrapRequestV1 {
                arm: args.arm,
                store_root: args.store_root,
                run_record: args.run_record,
                chain_dir: args.chain_dir,
                slot_locator: args.slot_locator,
            };
            run_native_cycle5_arm_bootstrap_genesis_v1(&request).map(|outcome| {
                format!(
                    "arm={} bootstrap_genesis=1 genesis_generation={} trainee_local_generation={} run_sha256={} base_seed={} checkpoint_manifest_sha256={} checkpoint_payload_sha256={} model_parameter_sha256={} train_state_sha256={}",
                    outcome.arm.wire_v1(),
                    outcome.genesis_generation_index,
                    outcome.trainee_local_generation,
                    outcome.run_sha256,
                    outcome.base_seed,
                    outcome.genesis.checkpoint_manifest_sha256,
                    outcome.genesis.checkpoint_payload_sha256,
                    outcome.genesis.model_parameter_sha256,
                    outcome.genesis.train_state_sha256,
                )
            })
        }
        ModeV1::Interval {
            refresh_manifest,
            payoff_panel,
            stop_generation,
            preflight_updates,
        } => {
            let request = Cycle5ArmRequestV1 {
                arm: args.arm,
                store_root: args.store_root,
                run_record: args.run_record,
                chain_dir: args.chain_dir,
                refresh_manifest,
                payoff_panel,
                slot_locator: args.slot_locator,
                stop_generation,
                preflight_updates,
            };
            run_native_cycle5_arm_v1(&request).map(|outcome| {
                format!(
                    "arm={} refresh_index={} resume_generation={} latest_generation={} trainee_local_generation={} refresh_manifest_sha256={} baseline_chain_generation={}",
                    outcome.arm.wire_v1(),
                    outcome.refresh_index,
                    outcome.resume_generation_index,
                    outcome.latest_generation_index,
                    outcome.trainee_local_generation,
                    outcome.refresh_manifest_sha256,
                    outcome
                        .baseline_chain_generation
                        .map_or_else(|| "none".to_owned(), |value| value.to_string()),
                )
            })
        }
    };

    match result {
        Ok(line) => println!("{line}"),
        Err(error) => {
            eprintln!("cycle5_arm_v1: {error}");
            std::process::exit(error.exit_code_v1());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_args_v1, Cycle5ArmKindV1, ModeV1, ParsedArgsV1, PathBuf};
    use std::ffi::OsString;

    fn args_v1(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    fn complete_v1() -> Vec<&'static str> {
        vec![
            "--arm",
            "control-v3",
            "--store-root",
            "D:/arm/store",
            "--run-record",
            "D:/arm/run.json",
            "--chain-dir",
            "D:/arm/chain",
            "--refresh-manifest",
            "D:/arm/refresh/refresh-01.manifest.json",
            "--payoff-panel",
            "D:/arm/refresh/refresh-01.panel.json",
            "--slot-locator",
            "D:/arm/locator.json",
            "--stop-generation",
            "256",
            "--device",
            "1",
        ]
    }

    /// The interval fields of a parsed command line, or `None` when it parsed
    /// as a bootstrap. Keeps every assertion below reading the same way it did
    /// before the mode enum existed.
    fn interval_v1(
        parsed: &ParsedArgsV1,
    ) -> Option<(&PathBuf, &Option<PathBuf>, u64, Option<u64>)> {
        match &parsed.mode {
            ModeV1::BootstrapGenesis => None,
            ModeV1::Interval {
                refresh_manifest,
                payoff_panel,
                stop_generation,
                preflight_updates,
            } => Some((
                refresh_manifest,
                payoff_panel,
                *stop_generation,
                *preflight_updates,
            )),
        }
    }

    fn bootstrap_v1() -> Vec<&'static str> {
        vec![
            "--arm",
            "control-v3",
            "--store-root",
            "D:/arm/store",
            "--run-record",
            "D:/arm/run.json",
            "--chain-dir",
            "D:/arm/chain",
            "--slot-locator",
            "D:/arm/locator.json",
            "--device",
            "1",
            "--bootstrap-genesis",
        ]
    }

    #[test]
    fn complete_command_line_parses_v1() {
        let parsed = parse_args_v1(args_v1(&complete_v1())).expect("parse");
        assert_eq!(parsed.arm, Cycle5ArmKindV1::TreatmentRb);
        let (_, payoff_panel, stop_generation, _) = interval_v1(&parsed).expect("interval mode");
        assert_eq!(stop_generation, 256);
        assert_eq!(parsed.device, 1);
        assert!(payoff_panel.is_some());
    }

    #[test]
    fn the_bootstrap_command_line_parses_and_takes_no_interval_flags_v1() {
        let parsed = parse_args_v1(args_v1(&bootstrap_v1())).expect("parse");
        assert_eq!(parsed.arm, Cycle5ArmKindV1::ControlR);
        assert_eq!(parsed.device, 1);
        assert!(matches!(parsed.mode, ModeV1::BootstrapGenesis));

        for extra in [
            vec![
                "--refresh-manifest",
                "D:/arm/refresh/refresh-00.manifest.json",
            ],
            vec!["--payoff-panel", "D:/arm/refresh/refresh-01.panel.json"],
            vec!["--stop-generation", "128"],
            vec!["--preflight-updates", "4"],
        ] {
            let mut values = bootstrap_v1();
            values.extend(extra.iter().copied());
            assert!(
                parse_args_v1(args_v1(&values)).is_err(),
                "--bootstrap-genesis must reject {extra:?}"
            );
        }
        let mut with_preflight = bootstrap_v1();
        with_preflight.push("--preflight");
        assert!(parse_args_v1(args_v1(&with_preflight)).is_err());
        let mut with_preflight_pair = bootstrap_v1();
        with_preflight_pair.extend(["--preflight", "--preflight-updates", "4"]);
        assert!(parse_args_v1(args_v1(&with_preflight_pair)).is_err());
    }

    #[test]
    fn the_bootstrap_marker_still_requires_every_shared_flag_v1() {
        for flag in [
            "--arm",
            "--store-root",
            "--run-record",
            "--chain-dir",
            "--slot-locator",
            "--device",
        ] {
            let mut values = bootstrap_v1();
            let index = values
                .iter()
                .position(|value| *value == flag)
                .expect("flag present");
            values.drain(index..index + 2);
            assert!(
                parse_args_v1(args_v1(&values)).is_err(),
                "{flag} must be required by a bootstrap too"
            );
        }
        let mut duplicated = bootstrap_v1();
        duplicated.push("--bootstrap-genesis");
        assert!(parse_args_v1(args_v1(&duplicated)).is_err());
    }

    #[test]
    fn an_interval_command_line_still_requires_its_own_flags_v1() {
        // Dropping only the bootstrap marker from a bootstrap command line
        // leaves an interval request with no manifest and no stop generation.
        let mut values = bootstrap_v1();
        let index = values
            .iter()
            .position(|value| *value == "--bootstrap-genesis")
            .expect("marker present");
        values.remove(index);
        assert!(parse_args_v1(args_v1(&values)).is_err());
    }

    #[test]
    fn genesis_command_line_omits_the_panel_v1() {
        let mut values = complete_v1();
        let index = values
            .iter()
            .position(|value| *value == "--payoff-panel")
            .expect("panel flag present");
        values.drain(index..index + 2);
        let parsed = parse_args_v1(args_v1(&values)).expect("parse");
        let (_, payoff_panel, _, _) = interval_v1(&parsed).expect("interval mode");
        assert!(payoff_panel.is_none());
    }

    #[test]
    fn unknown_flag_is_usage_v1() {
        let mut values = complete_v1();
        values.push("--unexpected");
        values.push("1");
        assert!(parse_args_v1(args_v1(&values)).is_err());
    }

    #[test]
    fn duplicate_flag_is_usage_v1() {
        let mut values = complete_v1();
        values.push("--device");
        values.push("0");
        assert!(parse_args_v1(args_v1(&values)).is_err());
    }

    #[test]
    fn missing_required_flag_is_usage_v1() {
        for flag in [
            "--arm",
            "--store-root",
            "--run-record",
            "--chain-dir",
            "--refresh-manifest",
            "--slot-locator",
            "--stop-generation",
            "--device",
        ] {
            let mut values = complete_v1();
            let index = values
                .iter()
                .position(|value| *value == flag)
                .expect("flag present");
            values.drain(index..index + 2);
            assert!(
                parse_args_v1(args_v1(&values)).is_err(),
                "{flag} must be required"
            );
        }
    }

    #[test]
    fn odd_arity_and_empty_command_lines_are_usage_v1() {
        assert!(parse_args_v1(args_v1(&[])).is_err());
        let mut values = complete_v1();
        values.push("--device");
        assert!(parse_args_v1(args_v1(&values)).is_err());
    }

    #[test]
    fn unknown_arm_kind_is_usage_v1() {
        let mut values = complete_v1();
        let index = values
            .iter()
            .position(|value| *value == "--arm")
            .expect("arm flag present");
        values[index + 1] = "treatment-r";
        assert!(parse_args_v1(args_v1(&values)).is_err());
    }

    #[test]
    fn the_preflight_pair_is_accepted_only_together_v1() {
        // Both halves: the only accepted shape.
        let mut both = complete_v1();
        both.push("--preflight");
        both.push("--preflight-updates");
        both.push("8");
        let parsed = parse_args_v1(args_v1(&both)).expect("parse");
        assert_eq!(interval_v1(&parsed).expect("interval mode").3, Some(8));

        // Neither half: the formal path, unchanged.
        let formal = parse_args_v1(args_v1(&complete_v1())).expect("parse");
        assert_eq!(interval_v1(&formal).expect("interval mode").3, None);

        // `--preflight-updates` alone never relaxes anything.
        let mut updates_only = complete_v1();
        updates_only.push("--preflight-updates");
        updates_only.push("8");
        assert!(parse_args_v1(args_v1(&updates_only)).is_err());

        // `--preflight` alone is an incomplete command line, not a request
        // to run a formal interval.
        let mut marker_only = complete_v1();
        marker_only.push("--preflight");
        assert!(parse_args_v1(args_v1(&marker_only)).is_err());
    }

    #[test]
    fn duplicate_or_malformed_preflight_flags_are_usage_v1() {
        let mut duplicate_marker = complete_v1();
        duplicate_marker.extend(["--preflight", "--preflight", "--preflight-updates", "8"]);
        assert!(parse_args_v1(args_v1(&duplicate_marker)).is_err());

        let mut duplicate_updates = complete_v1();
        duplicate_updates.extend([
            "--preflight",
            "--preflight-updates",
            "8",
            "--preflight-updates",
            "4",
        ]);
        assert!(parse_args_v1(args_v1(&duplicate_updates)).is_err());

        let mut non_numeric = complete_v1();
        non_numeric.extend(["--preflight", "--preflight-updates", "two"]);
        assert!(parse_args_v1(args_v1(&non_numeric)).is_err());

        let mut truncated = complete_v1();
        truncated.extend(["--preflight", "--preflight-updates"]);
        assert!(parse_args_v1(args_v1(&truncated)).is_err());
    }

    #[test]
    fn the_value_less_marker_does_not_shift_later_flags_v1() {
        // `--preflight` consumes no value, so a flag/value pair after it must
        // still parse exactly as it does before it.
        let mut leading = vec!["--preflight", "--preflight-updates", "4"];
        leading.extend(complete_v1());
        let parsed = parse_args_v1(args_v1(&leading)).expect("parse");
        let (_, _, stop_generation, preflight_updates) =
            interval_v1(&parsed).expect("interval mode");
        assert_eq!(preflight_updates, Some(4));
        assert_eq!(stop_generation, 256);
        assert_eq!(parsed.device, 1);
        assert_eq!(parsed.arm, Cycle5ArmKindV1::TreatmentRb);
    }

    #[test]
    fn non_numeric_stop_generation_is_usage_v1() {
        let mut values = complete_v1();
        let index = values
            .iter()
            .position(|value| *value == "--stop-generation")
            .expect("stop flag present");
        values[index + 1] = "256a";
        assert!(parse_args_v1(args_v1(&values)).is_err());
    }

    /// Round F item 3. `--check-slot-locator` is a whole-command-line mode
    /// handled in `main` before parsing, exactly like
    /// `--print-build-identity`. The strict parser must therefore refuse it
    /// as an ordinary flag, so no command line can ever mix "just check the
    /// inputs" with a mode that opens or writes a Store.
    #[test]
    fn check_slot_locator_is_not_an_ordinary_flag_v1() {
        assert!(parse_args_v1(args_v1(&["--check-slot-locator", "D:/arm/locator.json"])).is_err());

        let mut mixed = complete_v1();
        mixed.extend(["--check-slot-locator", "D:/arm/locator.json"]);
        assert!(parse_args_v1(args_v1(&mixed)).is_err());

        let mut with_bootstrap = vec!["--check-slot-locator", "D:/arm/locator.json"];
        with_bootstrap.extend(["--bootstrap-genesis"]);
        assert!(parse_args_v1(args_v1(&with_bootstrap)).is_err());
    }
}
