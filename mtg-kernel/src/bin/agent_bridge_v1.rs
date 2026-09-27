//! Spellbench `spellbench/v1` environment-role stdio binary.
//!
//! One compact JSON request object per stdin line, one compact JSON response
//! object per stdout line, in order. Framing follows spec Section 2 through
//! `agent_bridge_v1::read_bridge_input_line_v1` (8 MiB cap, `\r\n` tolerated
//! on read); the protocol itself is served by
//! `agent_bridge_v1::SpellbenchBridgeServerV1`. Closing stdin ends the
//! process with exit code 0; there is no quit message. stderr is diagnostics
//! only and never part of the protocol. The only accepted argument is
//! `--x-kernel-flat-v4` (bridge contract Section 11); any other exits 2.

use mtg_kernel::agent_bridge_v1::{
    read_bridge_input_line_v1, BridgeInputLineV1, SpellbenchBridgeOptionsV1,
    SpellbenchBridgeServerV1, SpellbenchEngineIdentityV1, SPELLBENCH_FLAG_X_KERNEL_FLAT_V4,
};
use std::io::{self, Write};

fn main() {
    let mut options = SpellbenchBridgeOptionsV1::default();
    for argument in std::env::args_os().skip(1) {
        if argument.to_str() == Some(SPELLBENCH_FLAG_X_KERNEL_FLAT_V4) {
            options.x_kernel_flat_v4 = true;
        } else {
            eprintln!("usage: agent_bridge_v1 [{SPELLBENCH_FLAG_X_KERNEL_FLAT_V4}]");
            std::process::exit(2);
        }
    }
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let mut stdout = io::stdout().lock();
    let mut server =
        SpellbenchBridgeServerV1::with_options(SpellbenchEngineIdentityV1::current(), options);
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
