fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 1 {
        eprintln!("usage: viewer_spy_capture_v1 CONFIG.json");
        std::process::exit(2);
    }
    if let Err(e) =
        mtg_kernel::regret_census_v1::run_spy_viewer_capture_v1(std::path::Path::new(&args[0]))
    {
        eprintln!("Spy viewer capture: {e}");
        std::process::exit(1);
    }
}
