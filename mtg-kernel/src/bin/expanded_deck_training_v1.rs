use mtg_kernel::expanded_deck_training_v1::{execute_v1, ExpandedTrainingCommandV1};

fn main() {
    if let Err(error) = run() {
        eprintln!("expanded-deck training: {error}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    let path = args
        .next()
        .ok_or("usage: expanded_deck_training_v1 CONFIG.json")?;
    if args.next().is_some() {
        return Err("expected one config path".into());
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("config exceeds 16 MiB".into());
    }
    let command: ExpandedTrainingCommandV1 =
        serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let result = execute_v1(command)?;
    println!(
        "{}",
        serde_json::to_string(&result).map_err(|e| e.to_string())?
    );
    Ok(())
}
