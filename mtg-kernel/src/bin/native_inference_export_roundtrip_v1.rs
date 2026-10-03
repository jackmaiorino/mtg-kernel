use mtg_kernel::native_checkpoint_export_v1::roundtrip_check_v1;
use std::ffi::OsString;
use std::path::PathBuf;

fn parse_args_v1(args: Vec<OsString>) -> Result<(PathBuf, PathBuf), ()> {
    if args.len() != 4 {
        return Err(());
    }
    let (mut reference, mut model_source) = (None, None);
    for pair in args.chunks_exact(2) {
        let slot = match pair[0].to_str() {
            Some("--reference") => &mut reference,
            Some("--model-source") => &mut model_source,
            _ => return Err(()),
        };
        if slot.replace(PathBuf::from(&pair[1])).is_some() {
            return Err(());
        }
    }
    Ok((reference.ok_or(())?, model_source.ok_or(())?))
}

fn main() {
    let (reference, model_source) = parse_args_v1(std::env::args_os().skip(1).collect())
        .unwrap_or_else(|()| {
            eprintln!(
                "usage: native_inference_export_roundtrip_v1 --reference STORE_SOURCE.json --model-source EXPANDED_MODEL_SOURCE.json"
            );
            std::process::exit(2);
        });
    // Native policy construction has large stack frames on Windows; match the
    // evaluator's worker-thread convention.
    let worker = std::thread::Builder::new()
        .name("inference-export-roundtrip".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || roundtrip_check_v1(&reference, &model_source))
        .expect("create round-trip worker");
    match worker.join() {
        Ok(Ok(receipt)) => println!("{receipt}"),
        Ok(Err(error)) => {
            eprintln!("INFERENCE_EXPORT_ROUNDTRIP_FAILED {error}");
            std::process::exit(1);
        }
        Err(_) => {
            eprintln!("INFERENCE_EXPORT_ROUNDTRIP_FAILED worker panicked");
            std::process::exit(1);
        }
    }
}
