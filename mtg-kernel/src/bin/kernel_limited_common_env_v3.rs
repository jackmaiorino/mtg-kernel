//! Separate executable opt-in. The frozen kernel_limited_env stays unchanged.
mtg_kernel::fast_global_allocator!();

use mtg_kernel::limited_common_session_v3::LimitedCommonJsonlServerV3;
use mtg_kernel::limited_session_v1::MAX_LIMITED_LINE_BYTES_V1;
use std::io::{self, BufRead, Read, Write};

fn main() -> io::Result<()> {
    if std::env::args_os().len() != 1 {
        eprintln!("usage: kernel_limited_common_env_v3");
        std::process::exit(2);
    }
    let mut server = LimitedCommonJsonlServerV3::new();
    if std::env::var_os("MTG_LIMITED_COMMON_TIMING_V3").as_deref()
        == Some(std::ffi::OsStr::new("1"))
    {
        server.enable_timings_v3();
    }
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
    if let Some(report) = server.timing_report_v3() {
        eprintln!("LIMITED_COMMON_TIMING_V3 {report}");
    }
    Ok(())
}
