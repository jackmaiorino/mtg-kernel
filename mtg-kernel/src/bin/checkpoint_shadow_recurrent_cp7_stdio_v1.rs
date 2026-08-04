use mtg_kernel::native_checkpoint_shadow_stdio_v1::run_checkpoint_shadow_stdio_with_recurrent_cp7_v1;
use std::ffi::OsString;
use std::path::PathBuf;

fn usage_v1() -> ! {
    eprintln!(
        "usage: checkpoint_shadow_recurrent_cp7_stdio_v1 --xmage-cp7-outcome-root PATH"
    );
    std::process::exit(2);
}

fn parse_args_v1(raw: Vec<OsString>) -> Result<PathBuf, ()> {
    if raw.len() != 2 || raw[0] != "--xmage-cp7-outcome-root" {
        return Err(());
    }
    Ok(PathBuf::from(&raw[1]))
}

fn main() {
    let root = parse_args_v1(std::env::args_os().skip(1).collect())
        .unwrap_or_else(|()| usage_v1());
    let python = std::env::var_os("MTG_KERNEL_RECURRENT_CP7_PYTHON")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            eprintln!("MTG_KERNEL_RECURRENT_CP7_PYTHON is required");
            std::process::exit(2);
        });
    if let Err(error) = run_checkpoint_shadow_stdio_with_recurrent_cp7_v1(root, python) {
        eprintln!("recurrent CP7 shadow scorer failed: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_are_exact_v1() {
        assert_eq!(
            parse_args_v1(vec!["--xmage-cp7-outcome-root".into(), "root".into()]),
            Ok(PathBuf::from("root"))
        );
        assert!(parse_args_v1(vec!["--wrong".into(), "root".into()]).is_err());
        assert!(parse_args_v1(Vec::new()).is_err());
    }
}
