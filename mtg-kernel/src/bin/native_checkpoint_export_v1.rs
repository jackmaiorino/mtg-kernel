use mtg_kernel::native_checkpoint_export_v1::export_native_checkpoint_v1;
use std::ffi::OsString;
use std::path::PathBuf;

fn parse_args_v1(args: Vec<OsString>) -> Result<(PathBuf, PathBuf), ()> {
    if args.len() != 4 {
        return Err(());
    }
    let (mut reference, mut output) = (None, None);
    for pair in args.chunks_exact(2) {
        let slot = match pair[0].to_str() {
            Some("--reference") => &mut reference,
            Some("--output-dir") => &mut output,
            _ => return Err(()),
        };
        if slot.replace(PathBuf::from(&pair[1])).is_some() {
            return Err(());
        }
    }
    Ok((reference.ok_or(())?, output.ok_or(())?))
}

fn main() {
    let (reference, output) =
        parse_args_v1(std::env::args_os().skip(1).collect()).unwrap_or_else(|()| {
            eprintln!(
                "usage: native_checkpoint_export_v1 --reference PATH --output-dir FRESH_DIRECTORY"
            );
            std::process::exit(2);
        });
    match export_native_checkpoint_v1(&reference, &output) {
        Ok(receipt) => println!("{receipt}"),
        Err(error) => {
            eprintln!("CHECKPOINT_EXPORT_FAILED {error}");
            std::process::exit(1);
        }
    }
}
