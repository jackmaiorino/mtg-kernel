//! Writes the rules vector v1 table, manifest and audit report for the
//! current build into an output directory (new files only).

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::args()
        .nth(1)
        .ok_or("usage: rules_vector_v1 OUTPUT_DIR")?;
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir)?;
    for path in mtg_kernel::rules_vector_v1::report::write_outputs(&dir)? {
        println!("{}", path.display());
    }
    Ok(())
}
