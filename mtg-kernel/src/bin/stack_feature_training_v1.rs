#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn main() {
    use mtg_kernel::expanded_deck_training_v1::stack_features::{run, Command};
    let result = (|| -> Result<(), String> {
        let path = std::env::args()
            .nth(1)
            .ok_or("usage: stack_feature_training_v1 COMMAND.json")?;
        let request: Command =
            serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        let result = run(request)?;
        println!(
            "{}",
            serde_json::to_string(&result).map_err(|e| e.to_string())?
        );
        Ok(())
    })();
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
