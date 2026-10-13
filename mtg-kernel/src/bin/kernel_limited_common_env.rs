//! Separate executable opt-in. The frozen kernel_limited_env stays unchanged.
use mtg_kernel::limited_common_session_v1::LimitedCommonJsonlServerV1;
use mtg_kernel::limited_session_v1::MAX_LIMITED_LINE_BYTES_V1;
use std::io::{self, BufRead, Read, Write};

fn main() -> io::Result<()> {
    if std::env::args_os().len() != 1 {
        eprintln!("usage: kernel_limited_common_env");
        std::process::exit(2);
    }
    let mut server = LimitedCommonJsonlServerV1::new();
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    loop {
        let mut line = Vec::new();
        let count = input
            .by_ref()
            .take((MAX_LIMITED_LINE_BYTES_V1 + 1) as u64)
            .read_until(b'\n', &mut line)?;
        if count == 0 {
            break;
        }
        if line.len() > MAX_LIMITED_LINE_BYTES_V1 {
            eprintln!("Limited common request exceeds the line-size limit");
            std::process::exit(2);
        }
        let text = std::str::from_utf8(&line)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "request is not UTF-8"))?;
        writeln!(
            output,
            "{}",
            server.handle_line(text.trim_end_matches(['\r', '\n']))
        )?;
        output.flush()?;
    }
    Ok(())
}
