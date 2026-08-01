fn main() {
    if let Err(error) =
        mtg_kernel::native_xmage_cp7_outcome_reinforce_v1::run_xmage_cp7_outcome_reinforce_cli_v1(
            std::env::args_os().skip(1),
        )
    {
        eprintln!("XMage CP7 outcome trainer failed: {error}");
        std::process::exit(1);
    }
}
