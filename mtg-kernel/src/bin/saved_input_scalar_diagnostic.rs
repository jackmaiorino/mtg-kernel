//! Offline tensor scoring only. No environment, RNG, or policy mutation.
fn main() {
    if let Err(error) = mtg_kernel::saved_input_scalar_diagnostic::run() {
        eprintln!("saved input scoring refused: {error}");
        std::process::exit(1);
    }
}
