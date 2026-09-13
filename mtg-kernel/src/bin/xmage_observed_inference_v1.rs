use mtg_kernel::xmage_observed_inference_v1::run_observed_inference_config_path_v1;
use std::path::PathBuf;

fn run() -> Result<(), String> {
    let mut arguments = std::env::args_os().skip(1);
    if arguments.next().as_deref() != Some(std::ffi::OsStr::new("--config")) {
        return Err("usage: xmage_observed_inference_v1 --config PATH".into());
    }
    let config = PathBuf::from(arguments.next().ok_or("missing config path")?);
    if arguments.next().is_some() {
        return Err("unexpected argument".into());
    }
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    run_observed_inference_config_path_v1(&config, &mut stdin.lock(), &mut stdout.lock())
}

fn main() {
    if let Err(code) = run() {
        eprintln!("XMage observed inference: {code}");
        std::process::exit(1);
    }
}
