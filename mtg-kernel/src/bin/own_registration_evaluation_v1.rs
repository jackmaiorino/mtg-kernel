#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn main() {
    let result = std::thread::Builder::new()
        .name("own-registration-evaluation".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(|| -> Result<(), String> {
            let path = std::env::args()
                .nth(1)
                .ok_or("usage: own_registration_evaluation_v1 COMMAND.json")?;
            let command = serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            let result =
                mtg_kernel::expanded_deck_training_v1::own_registration::evaluation::run(command)?;
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
