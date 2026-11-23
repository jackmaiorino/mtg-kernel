//! Offline checkpoint export and paired fork import. Takes one JSON request path.
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or("usage: card_semantics_e1 REQUEST.json")?;
    if args.next().is_some() {
        return Err("usage: card_semantics_e1 REQUEST.json".into());
    }
    let request = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let result = mtg_kernel::expanded_deck_training_v1::card_semantics_tool_json_v1(&request)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?
    );
    Ok(())
}
