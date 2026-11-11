//! The saved-batch fixture of the ordinary native S update
//! (`mtg_kernel::expanded_deck_training_v1::s_update_fixture_v1`;
//! `docs/design/s_update_fixture_v1.md`).
//!
//! usage: s_update_fixture_v1 generate out=<new dir> [shape=test|s] [episodes=]
//!            [seed=] [collection_workers=] [max_batch_mebibytes=]
//!            [max_prepared_tensor_mebibytes=]
//!        s_update_fixture_v1 command fixture=<dir> batch=initial|warmed
//!            path=serial|prepared [workers=] [verified_replay_tape_bytes=] out=<new dir>
//!        s_update_fixture_v1 verify fixture=<dir> batch=initial|warmed result=<update.json>
//!        s_update_fixture_v1 bench fixture=<dir> batch=initial|warmed
//!            path=serial|prepared [workers=] [verified_replay_tape_bytes=] [reps=] out=<new dir>

mtg_kernel::fast_global_allocator!();

use mtg_kernel::expanded_deck_training_v1::s_update_fixture_v1::main_v1;

fn main() {
    mtg_kernel::opt_out_of_power_throttling_v1();
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Games and updates run on a large stack, as the source test runs them.
    let run = std::thread::Builder::new()
        .name("s-update-fixture".into())
        .stack_size(64 << 20)
        .spawn(move || main_v1(&args))
        .expect("spawn the fixture thread");
    match run.join() {
        Ok(Ok(receipt)) => println!(
            "{}",
            serde_json::to_string_pretty(&receipt).expect("receipt serializes")
        ),
        Ok(Err(error)) => {
            eprintln!("s_update_fixture_v1: {error}");
            std::process::exit(1);
        }
        Err(payload) => std::panic::resume_unwind(payload),
    }
}
