fn main() {
    if let Err(error) = mtg_kernel::native_cp7_behavior_clone_v1::run_cp7_behavior_clone_cli_v1(
        std::env::args_os().skip(1),
    ) {
        eprintln!("cp7 behavior clone failed: {error}");
        std::process::exit(1);
    }
}
