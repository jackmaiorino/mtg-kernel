//! CLI for task D16: render one independent static owner-view HTML page (or
//! write small fixture checkpoints for stand-in testing before D3 and D6 merge).
//!
//! Usage:
//! ```text
//! cube_owner_view_v1 --run b1-u0032 --checkpoint CKPT.json \
//!     --ordered-pool POOL_IDS.json [--ordered-pool-sha256 SHA256] \
//!     [--fixture] --out PAGE.html [--json-out DATA.json]
//!
//! cube_owner_view_v1 --write-fixture-checkpoints OUT_DIR
//! ```
//!
//! Real checkpoint rendering is refused until D6's verified checkpoint loader
//! and D3's learned seats are wired. Stand-in fixture rendering requires the
//! explicit `--fixture` flag.
mtg_kernel::fast_global_allocator!();

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(err) = mtg_kernel::cube_owner_view_v1::cli::run_cli_args(&args) {
        eprintln!("cube_owner_view_v1: {err}");
        std::process::exit(1);
    }
}
