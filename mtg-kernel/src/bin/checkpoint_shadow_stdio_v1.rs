use mtg_kernel::native_checkpoint_shadow_stdio_v1::{
    run_checkpoint_shadow_stdio_v1, ShadowCheckpointAuthorityV1,
};
use std::path::PathBuf;

fn usage_v1() -> ! {
    eprintln!(
        "usage: checkpoint_shadow_stdio_v1 (--original-store-root PATH | --portable-derivative-root PATH)"
    );
    std::process::exit(2);
}

fn main() {
    let mut args = std::env::args_os();
    let _program = args.next();
    let authority = match (args.next(), args.next(), args.next()) {
        (Some(flag), Some(root), None) if flag == "--original-store-root" => {
            ShadowCheckpointAuthorityV1::OriginalPromoted2Generation384Store {
                root: PathBuf::from(root),
            }
        }
        (Some(flag), Some(root), None) if flag == "--portable-derivative-root" => {
            ShadowCheckpointAuthorityV1::PortablePromoted2WeightsGenesis {
                root: PathBuf::from(root),
            }
        }
        _ => usage_v1(),
    };
    if let Err(error) = run_checkpoint_shadow_stdio_v1(authority) {
        eprintln!("checkpoint shadow scorer failed: {error}");
        std::process::exit(1);
    }
}
