//! Spellbench `spellbench/v1` environment-role stdio binary.
//!
//! One compact JSON request object per stdin line, one compact JSON response
//! object per stdout line, in order. Framing follows spec Section 2 through
//! `agent_bridge_v1::read_bridge_input_line_v1` (8 MiB cap, `\r\n` tolerated
//! on read); the protocol itself is served by
//! `agent_bridge_v1::SpellbenchBridgeServerV1`. Closing stdin ends the
//! process with exit code 0; there is no quit message. stderr is diagnostics
//! only and never part of the protocol. Opt-in arguments enable V4 tensors
//! or private metadata for the v2 wrapper; any other argument exits 2.

mtg_kernel::fast_global_allocator!();

use mtg_kernel::agent_bridge_v1::{
    read_bridge_input_line_v1, BridgeInputLineV1, SpellbenchBridgeOptionsV1,
    SpellbenchBridgeServerV1, SpellbenchEngineIdentityV1, SPELLBENCH_FLAG_PAUPER_LONDON_HUMAN_V1,
    SPELLBENCH_FLAG_V2_SUPPORT, SPELLBENCH_FLAG_X_KERNEL_FLAT_V4, SPELLBENCH_FLAG_X_KERNEL_FLAT_V7,
    SPELLBENCH_FLAG_X_KERNEL_INFOSET_V1,
};
use std::io::{self, Write};

fn main() {
    let mut options = SpellbenchBridgeOptionsV1::default();
    let (mut london_human, mut flat_v7) = (false, false);
    for argument in std::env::args_os().skip(1) {
        if argument.to_str() == Some(SPELLBENCH_FLAG_PAUPER_LONDON_HUMAN_V1) {
            london_human = true;
        } else if argument.to_str() == Some(SPELLBENCH_FLAG_X_KERNEL_FLAT_V7) {
            flat_v7 = true;
        } else if argument.to_str() == Some(SPELLBENCH_FLAG_X_KERNEL_FLAT_V4) {
            options.x_kernel_flat_v4 = true;
        } else if argument.to_str() == Some(SPELLBENCH_FLAG_X_KERNEL_INFOSET_V1) {
            // The producer runs on the flat-v4 lockstep copy.
            options.x_kernel_infoset_v1 = true;
            options.x_kernel_flat_v4 = true;
        } else if argument.to_str() == Some(SPELLBENCH_FLAG_V2_SUPPORT) {
            options.private_v2_support = true;
        } else {
            eprintln!("usage: agent_bridge_v1 [{SPELLBENCH_FLAG_X_KERNEL_FLAT_V4}] [{SPELLBENCH_FLAG_X_KERNEL_INFOSET_V1}] [{SPELLBENCH_FLAG_V2_SUPPORT}] | {SPELLBENCH_FLAG_PAUPER_LONDON_HUMAN_V1} {SPELLBENCH_FLAG_X_KERNEL_FLAT_V7} {SPELLBENCH_FLAG_V2_SUPPORT}");
            std::process::exit(2);
        }
    }
    // The human-play London mode is its own process: all three flags, and
    // none of the V4 or infoset ones (that mode owns its model input).
    let london_flags_ok = london_human
        && flat_v7
        && options.private_v2_support
        && !options.x_kernel_flat_v4
        && !options.x_kernel_infoset_v1;
    if (london_human || flat_v7) && !london_flags_ok {
        eprintln!("{SPELLBENCH_FLAG_PAUPER_LONDON_HUMAN_V1} needs {SPELLBENCH_FLAG_X_KERNEL_FLAT_V7} and {SPELLBENCH_FLAG_V2_SUPPORT}, and excludes {SPELLBENCH_FLAG_X_KERNEL_FLAT_V4} and {SPELLBENCH_FLAG_X_KERNEL_INFOSET_V1}");
        std::process::exit(2);
    }
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let mut stdout = io::stdout().lock();
    let mut server = if london_human {
        SpellbenchBridgeServerV1::with_pauper_london_human_v1(SpellbenchEngineIdentityV1::current())
    } else {
        SpellbenchBridgeServerV1::with_options(SpellbenchEngineIdentityV1::current(), options)
    };
    let mut buffer: Vec<u8> = Vec::new();

    loop {
        let response = match read_bridge_input_line_v1(&mut reader, &mut buffer) {
            Ok(BridgeInputLineV1::Eof) => break,
            Ok(BridgeInputLineV1::Oversized) => server.oversized_line_response(),
            Ok(BridgeInputLineV1::Line(bytes)) => match String::from_utf8(bytes) {
                Ok(line) => server.handle_line(&line),
                Err(_) => server.invalid_utf8_line_response(),
            },
            Err(err) => {
                eprintln!("stdin read failed: {err}");
                std::process::exit(1);
            }
        };
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
