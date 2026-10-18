mtg_kernel::fast_global_allocator!();

fn main() {
    if let Err(error) = mtg_kernel::gameplay_checkpoint_reconstruction_v1::run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
