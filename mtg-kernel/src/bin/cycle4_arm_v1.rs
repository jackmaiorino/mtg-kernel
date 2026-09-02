//! Cycle-4 arm launcher CLI (`docs/native_cycle4_arm_launcher_v1.md`
//! Section 4). One invocation runs exactly one refresh interval of one arm
//! and exits; the wrapper loops, never this process.
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
//! `--stop-generation == resume_position + 128` before training.

use mtg_kernel::native_cycle4_arm_v1::{
    run_native_cycle4_arm_v1, Cycle4ArmKindV1, Cycle4ArmRequestV1,
};
use std::ffi::OsString;
use std::path::PathBuf;

fn usage_v1() -> ! {
    eprintln!(
        "usage: cycle4_arm_v1 --arm (control-r|static-rb|treatment-rb) --store-root PATH --run-record PATH --chain-dir PATH --refresh-manifest PATH [--payoff-panel PATH] --slot-locator PATH --stop-generation N --device N"
    );
    std::process::exit(2);
}

struct ParsedArgsV1 {
    arm: Cycle4ArmKindV1,
    store_root: PathBuf,
    run_record: PathBuf,
    chain_dir: PathBuf,
    refresh_manifest: PathBuf,
    payoff_panel: Option<PathBuf>,
    slot_locator: PathBuf,
    stop_generation: u64,
    device: u64,
}

#[allow(clippy::too_many_lines)]
fn parse_args_v1(raw: Vec<OsString>) -> Result<ParsedArgsV1, ()> {
    if raw.is_empty() || !raw.len().is_multiple_of(2) {
        return Err(());
    }
    let mut arm: Option<Cycle4ArmKindV1> = None;
    let mut store_root: Option<PathBuf> = None;
    let mut run_record: Option<PathBuf> = None;
    let mut chain_dir: Option<PathBuf> = None;
    let mut refresh_manifest: Option<PathBuf> = None;
    let mut payoff_panel: Option<PathBuf> = None;
    let mut slot_locator: Option<PathBuf> = None;
    let mut stop_generation: Option<u64> = None;
    let mut device: Option<u64> = None;

    for pair in raw.chunks_exact(2) {
        let flag = pair[0].to_str().ok_or(())?;
        let value = &pair[1];
        match flag {
            "--arm" if arm.is_none() => {
                arm = Some(Cycle4ArmKindV1::from_wire_v1(value.to_str().ok_or(())?).ok_or(())?);
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
            _ => return Err(()),
        }
    }

    Ok(ParsedArgsV1 {
        arm: arm.ok_or(())?,
        store_root: store_root.ok_or(())?,
        run_record: run_record.ok_or(())?,
        chain_dir: chain_dir.ok_or(())?,
        refresh_manifest: refresh_manifest.ok_or(())?,
        payoff_panel,
        slot_locator: slot_locator.ok_or(())?,
        stop_generation: stop_generation.ok_or(())?,
        device: device.ok_or(())?,
    })
}

fn main() {
    let raw: Vec<OsString> = std::env::args_os().skip(1).collect();
    let args = parse_args_v1(raw).unwrap_or_else(|()| usage_v1());

    // Pin this process to exactly one GPU before anything can initialize
    // CUDA. Set, never read: no configuration comes from the environment.
    // SAFETY: single-threaded process start, strictly before any CUDA
    // context, worker thread, or library call exists.
    unsafe {
        std::env::set_var("CUDA_VISIBLE_DEVICES", args.device.to_string());
    }

    let request = Cycle4ArmRequestV1 {
        arm: args.arm,
        store_root: args.store_root,
        run_record: args.run_record,
        chain_dir: args.chain_dir,
        refresh_manifest: args.refresh_manifest,
        payoff_panel: args.payoff_panel,
        slot_locator: args.slot_locator,
        stop_generation: args.stop_generation,
    };

    match run_native_cycle4_arm_v1(&request) {
        Ok(outcome) => {
            println!(
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
            );
        }
        Err(error) => {
            eprintln!("cycle4_arm_v1: {error}");
            std::process::exit(error.exit_code_v1());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_args_v1, Cycle4ArmKindV1};
    use std::ffi::OsString;

    fn args_v1(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    fn complete_v1() -> Vec<&'static str> {
        vec![
            "--arm",
            "treatment-rb",
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

    #[test]
    fn complete_command_line_parses_v1() {
        let parsed = parse_args_v1(args_v1(&complete_v1())).expect("parse");
        assert_eq!(parsed.arm, Cycle4ArmKindV1::TreatmentRb);
        assert_eq!(parsed.stop_generation, 256);
        assert_eq!(parsed.device, 1);
        assert!(parsed.payoff_panel.is_some());
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
        assert!(parsed.payoff_panel.is_none());
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
    fn non_numeric_stop_generation_is_usage_v1() {
        let mut values = complete_v1();
        let index = values
            .iter()
            .position(|value| *value == "--stop-generation")
            .expect("stop flag present");
        values[index + 1] = "256a";
        assert!(parse_args_v1(args_v1(&values)).is_err());
    }
}
