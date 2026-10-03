//! Separate diagnostic identity; never produces training trajectories.
use mtg_kernel::learned_bo3_v1::gate_color_diagnostic::{run, Request};
use std::{fs, io::Write};

fn execute() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        return Err("usage: gate_color_diagnostic_v1 REQUEST.json OUTPUT.json".into());
    }
    if std::path::Path::new(&args[2]).exists() {
        return Err("output already exists".into());
    }
    let request: Request = serde_json::from_slice(&fs::read(&args[1])?)?;
    let result = run(request)?;
    let bytes = serde_json::to_vec_pretty(&result)?;
    let temp = format!("{}.partial", args[2]);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(temp, &args[2])?;
    Ok(())
}
fn main() {
    if let Err(error) = execute() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
