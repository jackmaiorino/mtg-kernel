fn main() {
    if let Err(error) = mtg_kernel::gameplay_checkpoint_reconstruction_v1::run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
