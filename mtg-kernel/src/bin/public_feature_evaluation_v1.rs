#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn main() {
    // Native debug policy construction has large stack frames on Windows.
    // Match the existing fresh-initialization and training CLI convention.
    let worker = std::thread::Builder::new()
        .name("public-feature-evaluation".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(run)
        .expect("create evaluator worker");
    if worker.join().is_err() {
        eprintln!("public evaluator worker panicked");
        std::process::exit(1);
    }
}

#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn run() {
    let result = (|| -> Result<(), String> {
        let path = std::env::args()
            .nth(1)
            .ok_or("usage: public_feature_evaluation_v1 COMMAND.json")?;
        let command = serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let result = mtg_kernel::learned_bo3_v1::public_evaluation::run(command)?;
        println!("{}", result);
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
