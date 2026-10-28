//! The B width pre-screen driver; see
//! `mtg_kernel::expanded_deck_training_v1::b_width_prescreen_v1`.
mtg_kernel::fast_global_allocator!();

use mtg_kernel::expanded_deck_training_v1::b_width_prescreen_v1::run_b_width_prescreen_cli_v1;

fn main() {
    // SAFETY: first statement of main; no other thread exists yet.
    unsafe { mtg_kernel::retain_freed_memory_v1() };
    mtg_kernel::opt_out_of_power_throttling_v1();
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run_b_width_prescreen_cli_v1(&args) {
        Ok(summary) => println!("{summary}"),
        Err(error) => {
            eprintln!("b width prescreen: {error}");
            std::process::exit(1);
        }
    }
}
