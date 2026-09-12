use mtg_kernel::native_expanded_training_run_v1::{
    run_native_expanded_training_v1, NativeExpandedTrainingRunV1,
};
use std::path::PathBuf;

fn main() {
    if let Err(error) = run() {
        eprintln!("native expanded training: {error}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    let path = PathBuf::from(
        args.next()
            .ok_or("usage: native_expanded_training_run_v1 CONFIG.json [--max-new-iterations N]")?,
    );
    let limit = if let Some(flag) = args.next() {
        if flag != "--max-new-iterations" {
            return Err("unknown option".into());
        }
        Some(
            args.next()
                .ok_or("missing iteration limit")?
                .to_str()
                .ok_or("invalid iteration limit")?
                .parse::<usize>()
                .map_err(|e| e.to_string())?,
        )
    } else {
        None
    };
    if args.next().is_some() {
        return Err("unexpected arguments".into());
    }
    if !path.is_absolute() {
        return Err("absolute config path required".into());
    }
    let metadata = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() > 16 * 1024 * 1024 {
        return Err("config exceeds 16 MiB".into());
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("config exceeds 16 MiB".into());
    }
    let config: NativeExpandedTrainingRunV1 =
        serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let result = run_native_expanded_training_v1(&config, limit)?;
    println!(
        "{}",
        serde_json::to_string(&result).map_err(|e| e.to_string())?
    );
    Ok(())
}
