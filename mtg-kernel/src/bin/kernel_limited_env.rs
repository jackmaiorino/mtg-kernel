mtg_kernel::fast_global_allocator!();

use mtg_kernel::limited_session_v1::{LimitedJsonlServerV1, MAX_LIMITED_LINE_BYTES_V1};
use std::io::{self, BufRead, Read, Write};

fn main() -> io::Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    #[cfg(feature = "eternal-fixtures")]
    if matches!(args.as_slice(), [flag] if flag == "--premodern-feb-v1") {
        let mut server = mtg_kernel::premodern_ordered_public_v1::feb_jsonl::FebJsonlServerV1::default();
        return run_jsonl(|line| server.handle_line(line));
    }
    let mode = match args.as_slice() {
        [] => 1,
        [flag] if flag == "--engine-priority-v1" => 2,
        [flag] if flag == "--foundations-combat-v1" => 3,
        [flag] if flag == "--london-mulligans-v1" => 4,
        [flag] if flag == "--public-shroud-v1" => 5,
        _ => {
            let feb = if cfg!(feature = "eternal-fixtures") { " | --premodern-feb-v1" } else { "" };
            eprintln!("usage: kernel_limited_env [--engine-priority-v1 | --foundations-combat-v1 | --london-mulligans-v1 | --public-shroud-v1{feb}]");
            std::process::exit(2);
        }
    };
    let mut server = match mode {
        5 => LimitedJsonlServerV1::new_with_public_shroud_v1(),
        4 => LimitedJsonlServerV1::new_with_london_mulligans_v1(),
        3 => LimitedJsonlServerV1::new_with_foundations_combat_v1(),
        2 => LimitedJsonlServerV1::new_with_engine_priority_v1(),
        _ => LimitedJsonlServerV1::new(),
    };
    run_jsonl(|line| server.handle_line(line))
}

fn run_jsonl(mut handle_line: impl FnMut(&str) -> String) -> io::Result<()> {
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
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
            handle_line(text.trim_end_matches(['\r', '\n']))
        )?;
        output.flush()?;
    }
    Ok(())
}
