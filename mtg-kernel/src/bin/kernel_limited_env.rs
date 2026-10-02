use mtg_kernel::limited_session_v1::{LimitedJsonlServerV1, MAX_LIMITED_LINE_BYTES_V1};
use std::io::{self, BufRead, Read, Write};

fn main() -> io::Result<()> {
    if std::env::args_os().len() != 1 {
        eprintln!("usage: kernel_limited_env");
        std::process::exit(2);
    }
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    let mut server = LimitedJsonlServerV1::new();
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
