use mtg_kernel::limited_session_v1::{LimitedJsonlServerV1, MAX_LIMITED_LINE_BYTES_V1};
use std::io::{self, BufRead, Read, Write};

fn main() -> io::Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let mode = match args.as_slice() {
        [] => 1,
        [flag] if flag == "--engine-priority-v1" => 2,
        [flag] if flag == "--foundations-combat-v1" => 3,
        [flag] if flag == "--london-mulligans-v1" => 4,
        _ => {
            eprintln!("usage: kernel_limited_env [--engine-priority-v1 | --foundations-combat-v1 | --london-mulligans-v1]");
            std::process::exit(2);
        }
    };
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    let mut server = match mode {
        4 => LimitedJsonlServerV1::new_with_london_mulligans_v1(),
        3 => LimitedJsonlServerV1::new_with_foundations_combat_v1(),
        2 => LimitedJsonlServerV1::new_with_engine_priority_v1(),
        _ => LimitedJsonlServerV1::new(),
    };
    loop {
        let mut line = Vec::new();
        let read = input
            .by_ref()
            .take((MAX_LIMITED_LINE_BYTES_V1 + 1) as u64)
            .read_until(b'\n', &mut line)?;
        if read == 0 {
            break;
        }
        if line.len() > MAX_LIMITED_LINE_BYTES_V1 {
            eprintln!("Limited request exceeds the line-size limit");
            std::process::exit(2);
        }
        let text = match std::str::from_utf8(&line) {
            Ok(text) => text,
            Err(_) => {
                eprintln!("Limited request is not UTF-8");
                std::process::exit(2);
            }
        };
        writeln!(
            output,
            "{}",
            server.handle_line(text.trim_end_matches(['\r', '\n']))
        )?;
        output.flush()?;
    }
    Ok(())
}
