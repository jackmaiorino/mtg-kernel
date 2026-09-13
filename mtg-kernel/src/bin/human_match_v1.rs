use mtg_kernel::human_match_v1::serve_human_match_v1;

fn main() {
    if let Err(error) = run() { eprintln!("human match: {error}"); std::process::exit(1); }
}
fn run() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new("--config")) { return Err("usage: human_match_v1 --config CONFIG.json".into()); }
    let config = args.next().ok_or("A server-local config path is required.")?;
    if args.next().is_some() { return Err("Unexpected command-line argument.".into()); }
    serve_human_match_v1(std::path::Path::new(&config), std::io::stdin().lock(), std::io::stdout().lock())
}
