#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn main() {
    let result = std::thread::Builder::new()
        .name("own-registration-evaluation".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(|| -> Result<(), String> {
            let mut args = std::env::args().skip(1);
            let first = args
                .next()
                .ok_or("usage: own_registration_evaluation_v1 [--saved-rows] COMMAND.json")?;
            let saved_rows = first == "--saved-rows";
            let path = if saved_rows {
                args.next().ok_or("--saved-rows requires COMMAND.json")?
            } else {
                first
            };
            if args.next().is_some() {
                return Err("unexpected registration evaluation argument".into());
            }
            let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
            let result = if saved_rows {
                let request = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                mtg_kernel::expanded_deck_training_v1::own_registration::training::behavior::run(
                    request,
                )?
            } else {
                let command = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                mtg_kernel::expanded_deck_training_v1::own_registration::evaluation::run(command)?
            };
            println!("{result}");
            Ok(())
        })
        .map_err(|e| e.to_string())
        .and_then(|worker| {
            worker
                .join()
                .map_err(|_| "registration evaluation worker panicked".to_owned())
        })
        .and_then(|result| result);
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(not(feature = "experimental-burn-net8-packed-cuda-v1"))]
fn main() {
    eprintln!("requires experimental-burn-net8-packed-cuda-v1");
    std::process::exit(1);
}
