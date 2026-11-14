//! Native S receipt tools (`mtg_kernel::s_transformer_v1::tools`).
//!
//! usage: s_transformer_v1 parity-dump out=<dir> [seed= batches= groups=]
//!        s_transformer_v1 logits-dump out=<file> model=<spec> states=<spec>
//!        s_transformer_v1 bench [seed= count= reps=]
//!        s_transformer_v1 gae-bench [seed= groups= reps= workers=]
//!        s_transformer_v1 resume-check out=<dir> [seed= steps= split= groups=]

#[cfg(not(feature = "s-learner-profile-v1"))]
mtg_kernel::fast_global_allocator!();

// Measurement-only builds count allocator events for `gae-bench`'s profile.
#[cfg(feature = "s-learner-profile-v1")]
#[global_allocator]
static GLOBAL_ALLOCATOR: mtg_kernel::s_transformer_v1::profile::SProfileAllocatorV1 =
    mtg_kernel::s_transformer_v1::profile::SProfileAllocatorV1;

fn main() {
    #[cfg(not(feature = "s-learner-profile-v1"))]
    // SAFETY: first statement of main; no other thread exists yet.
    unsafe {
        mtg_kernel::retain_freed_memory_v1();
    }
    mtg_kernel::opt_out_of_power_throttling_v1();
    let args: Vec<String> = std::env::args().skip(1).collect();
    match mtg_kernel::s_transformer_v1::tools::main_v1(&args) {
        Ok(receipt) => println!(
            "{}",
            serde_json::to_string_pretty(&receipt).expect("receipt serializes")
        ),
        Err(error) => {
            eprintln!("s_transformer_v1: {error}");
            std::process::exit(1);
        }
    }
}
