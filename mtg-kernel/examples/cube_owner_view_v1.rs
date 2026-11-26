//! CLI for task D16: render one independent static owner-view HTML page from a
//! verified D6 checkpoint (or run the explicit `--fixture` test path).
//!
//! Usage:
//! ```text
//! cube_owner_view_v1 --run k1-u0064 --checkpoint CKPT.json \
//!     --ordered-pool POOL_IDS.json [--ordered-pool-sha256 SHA256] \
//!     --cube-table data/card_semantics_v1/cube_table_v1.json \
//!     --cube-table-sha256 778c12a127071fdc2170730e4f0e932d296f47b6ac2805b3eee331e00f5c46c0 \
//!     (--card-index INDEX_IDS.json | --cube-list data/cube_v1/mtgo_vintage_cube_20261007.json) \
//!     [--card-index-sha256 SHA256] [--checkpoint-state-sha256 SHA256] \
//!     --out PAGE.html [--json-out DATA.json]
//!
//! cube_owner_view_v1 --fixture --run b1-u0032 --checkpoint FIXTURE_CKPT.json \
//!     --ordered-pool POOL_IDS.json [--ordered-pool-sha256 SHA256] \
//!     --out PAGE.html [--json-out DATA.json]
//!
//! cube_owner_view_v1 --write-fixture-checkpoints OUT_DIR
//! ```
mtg_kernel::fast_global_allocator!();

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(err) = mtg_kernel::cube_owner_view_v1::cli::run_cli_args(&args) {
        eprintln!("cube_owner_view_v1: {err}");
        std::process::exit(1);
    }
}
