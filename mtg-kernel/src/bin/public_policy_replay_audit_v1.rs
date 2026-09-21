#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn main() {
    use mtg_kernel::expanded_deck_training_v1::public_features::replay_audit::{run, AuditCommand};
    let result = (|| -> Result<(), String> {
        let path = std::env::args()
            .nth(1)
            .ok_or("usage: public_policy_replay_audit_v1 COMMAND.json")?;
        let command: AuditCommand =
            serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        println!("{}", run(command)?);
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
