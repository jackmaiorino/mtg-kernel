use mtg_kernel::phase_profile::{
    RlPhaseProfileV1, RlPhaseV1, RlProfileTimerV1, RL_PHASE_PROFILE_PREFIX_V1,
};
use mtg_kernel::rl_session::KernelRlJsonlServerV1;
use std::io::{self, BufRead, Write};

fn main() {
    let mut args = std::env::args_os();
    let _program = args.next();
    let phase_profile = match (args.next(), args.next()) {
        (None, None) => false,
        (Some(flag), None) if flag == "--phase-profile-v1" => true,
        _ => {
            eprintln!("usage: kernel_rl_env [--phase-profile-v1]");
            std::process::exit(2);
        }
    };
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    let mut server = KernelRlJsonlServerV1::new();
    let mut profile = phase_profile.then(RlPhaseProfileV1::default);

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(err) => {
                eprintln!("stdin read failed: {err}");
                std::process::exit(1);
            }
        };
        let response = match profile.as_mut() {
            Some(profile) => server.handle_line_profiled(&line, profile),
            None => server.handle_line(&line),
        };
        if let Some(profile) = profile.as_mut() {
            let timer = RlProfileTimerV1::start();
            if let Err(err) = writeln!(stdout, "{response}") {
                profile.record_elapsed(RlPhaseV1::WriteFlush, timer);
                eprintln!("stdout write failed: {err}");
                std::process::exit(1);
            }
            if let Err(err) = stdout.flush() {
                profile.record_elapsed(RlPhaseV1::WriteFlush, timer);
                eprintln!("stdout flush failed: {err}");
                std::process::exit(1);
            }
            profile.record_elapsed(RlPhaseV1::WriteFlush, timer);
        } else {
            if let Err(err) = writeln!(stdout, "{response}") {
                eprintln!("stdout write failed: {err}");
                std::process::exit(1);
            }
            if let Err(err) = stdout.flush() {
                eprintln!("stdout flush failed: {err}");
                std::process::exit(1);
            }
        }
    }
    if let Some(profile) = profile {
        eprintln!("{RL_PHASE_PROFILE_PREFIX_V1}{}", profile.canonical_json());
    }
}
