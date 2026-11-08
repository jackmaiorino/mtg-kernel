//! Native S receipt tools (`mtg_kernel::s_transformer_v1::tools`).
//!
//! usage: s_transformer_v1 parity-dump out=<dir> [seed= batches= groups=]
//!        s_transformer_v1 logits-dump out=<file> model=<spec> states=<spec>
//!        s_transformer_v1 bench [seed= count= reps=]
//!        s_transformer_v1 gae-bench [seed= groups= reps= workers=]
//!        s_transformer_v1 resume-check out=<dir> [seed= steps= split= groups=]

mtg_kernel::fast_global_allocator!();

fn main() {
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
