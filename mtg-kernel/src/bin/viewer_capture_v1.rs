fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 1 {
        eprintln!("usage: viewer_capture_v1 CONFIG.json");
        std::process::exit(2);
    }
    if let Err(error) = mtg_kernel::viewer_capture_v1::run(std::path::Path::new(&args[0])) {
        eprintln!("viewer capture: {error}");
        std::process::exit(1);
    }
}
