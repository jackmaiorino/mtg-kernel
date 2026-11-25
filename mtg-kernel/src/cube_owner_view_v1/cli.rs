//! Command-line driver for task D16 (owner view).
//!
//! Real checkpoint rendering is explicitly refused until D6's verified
//! checkpoint loader and D3's learned seats are wired. Stand-in fixture
//! rendering requires the explicit `--fixture` flag AND a fixture checkpoint
//! file carrying `fixture_standin` and `FIXTURE_MODEL_IDENTITY_V1`.
//!
//! Only the pinned ascending ordered pool JSON array (`--ordered-pool`, D0.4)
//! is accepted, with optional `--ordered-pool-sha256` pin enforcement. The
//! unpinned cube-file pool path has been removed so cards lacking a cube table
//! row can never enter an owner-view draft.

use crate::card_def::KERNEL_CARDDB_HASH;
use crate::cube_draft_v1::CubePool;
use crate::cube_owner_view_v1::{
    cube_pool_from_ordered_ids, render_owner_view_page, resolve_page_spec, CheckpointHeader,
    FixtureCheckpointFile, HeadUpdates, StandInCheckpoint, SHAKEDOWN_PAGE_SPECS,
};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

pub type Fallible<T> = Result<T, String>;

pub const USAGE: &str = "usage:\n  cube_owner_view_v1 --run RUN_ID --checkpoint CKPT.json --ordered-pool POOL_IDS.json [--ordered-pool-sha256 SHA256] [--fixture] --out PAGE.html [--json-out DATA.json]\n  cube_owner_view_v1 --write-fixture-checkpoints OUT_DIR";

enum Command {
    Render {
        run_id: String,
        checkpoint_path: PathBuf,
        ordered_pool_path: PathBuf,
        expected_pool_sha256: Option<String>,
        fixture_mode: bool,
        out_html: PathBuf,
        out_json: Option<PathBuf>,
    },
    WriteFixtures {
        out_dir: PathBuf,
    },
}

fn parse_args(args: &[String]) -> Fallible<Command> {
    if args.is_empty() {
        return Err(USAGE.to_owned());
    }
    if args.len() == 2 && args[0] == "--write-fixture-checkpoints" {
        return Ok(Command::WriteFixtures {
            out_dir: PathBuf::from(&args[1]),
        });
    }

    let mut run_id: Option<String> = None;
    let mut checkpoint_path: Option<PathBuf> = None;
    let mut ordered_pool_path: Option<PathBuf> = None;
    let mut expected_pool_sha256: Option<String> = None;
    let mut fixture_mode = false;
    let mut out_html: Option<PathBuf> = None;
    let mut out_json: Option<PathBuf> = None;

    let mut i = 0usize;
    while i < args.len() {
        let flag = args[i].as_str();
        let next = |idx: usize| -> Fallible<String> {
            args.get(idx + 1)
                .cloned()
                .ok_or_else(|| format!("missing value for {flag}\n{USAGE}"))
        };
        match flag {
            "--run" => {
                run_id = Some(next(i)?);
                i += 2;
            }
            "--checkpoint" => {
                checkpoint_path = Some(PathBuf::from(next(i)?));
                i += 2;
            }
            "--ordered-pool" => {
                ordered_pool_path = Some(PathBuf::from(next(i)?));
                i += 2;
            }
            "--ordered-pool-sha256" => {
                expected_pool_sha256 = Some(next(i)?);
                i += 2;
            }
            "--fixture" => {
                fixture_mode = true;
                i += 1;
            }
            "--out" => {
                out_html = Some(PathBuf::from(next(i)?));
                i += 2;
            }
            "--json-out" => {
                out_json = Some(PathBuf::from(next(i)?));
                i += 2;
            }
            other => return Err(format!("unknown flag {other:?}\n{USAGE}")),
        }
    }

    let run_id = run_id.ok_or_else(|| format!("missing --run\n{USAGE}"))?;
    let checkpoint_path =
        checkpoint_path.ok_or_else(|| format!("missing --checkpoint\n{USAGE}"))?;
    let ordered_pool_path =
        ordered_pool_path.ok_or_else(|| format!("missing --ordered-pool\n{USAGE}"))?;
    let out_html = out_html.ok_or_else(|| format!("missing --out\n{USAGE}"))?;

    Ok(Command::Render {
        run_id,
        checkpoint_path,
        ordered_pool_path,
        expected_pool_sha256,
        fixture_mode,
        out_html,
        out_json,
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(&mut hex, "{byte:02x}");
    }
    hex
}

fn load_ordered_pool(path: &Path, expected_sha256: Option<&str>) -> Fallible<(CubePool, String)> {
    let raw_bytes =
        fs::read(path).map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    let text = std::str::from_utf8(&raw_bytes)
        .map_err(|err| format!("ordered pool {} is not valid UTF-8: {err}", path.display()))?;
    let ids: Vec<u16> = serde_json::from_str(text)
        .map_err(|err| format!("failed to parse ordered pool {}: {err}", path.display()))?;
    let (pool, canonical_sha) = cube_pool_from_ordered_ids(&ids).map_err(|err| err.to_string())?;
    if let Some(expected) = expected_sha256 {
        let file_sha = sha256_hex(&raw_bytes);
        if file_sha != expected || canonical_sha != expected {
            return Err(format!(
                "ordered pool sha256 mismatch for {}: expected {expected:?}, file bytes {file_sha:?}, canonical array+LF {canonical_sha:?}",
                path.display()
            ));
        }
    }
    Ok((pool, canonical_sha))
}

fn write_fixture_checkpoints(out_dir: &Path) -> Fallible<()> {
    fs::create_dir_all(out_dir)
        .map_err(|err| format!("failed to create {}: {err}", out_dir.display()))?;
    let carddb = format!("{KERNEL_CARDDB_HASH:016x}");
    for spec in SHAKEDOWN_PAGE_SPECS {
        let head_updates = if spec.expected_segment == "b1" {
            HeadUpdates {
                pick: 0,
                value: 0,
                count: spec.expected_update,
                pointer: spec.expected_update,
                type_set: spec.expected_update,
                split: spec.expected_update,
            }
        } else {
            let build_updates = 128 + spec.expected_update;
            HeadUpdates {
                pick: spec.expected_update,
                value: spec.expected_update,
                count: build_updates,
                pointer: build_updates,
                type_set: build_updates,
                split: build_updates,
            }
        };
        let fixture = FixtureCheckpointFile::for_shakedown_fixture(
            spec.expected_segment,
            spec.expected_update,
            head_updates,
            &carddb,
        );
        let mut text = serde_json::to_string_pretty(&fixture).map_err(|err| err.to_string())?;
        text.push('\n');
        let path = out_dir.join(format!("{}.checkpoint.json", spec.run_id));
        fs::write(&path, text.as_bytes())
            .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    }
    Ok(())
}

/// Runs the D16 CLI on an argument slice (excluding `argv[0]`).
pub fn run_cli_args(args: &[String]) -> Fallible<()> {
    match parse_args(args)? {
        Command::WriteFixtures { out_dir } => write_fixture_checkpoints(&out_dir),
        Command::Render {
            run_id,
            checkpoint_path,
            ordered_pool_path,
            expected_pool_sha256,
            fixture_mode,
            out_html,
            out_json,
        } => {
            let _page_spec = resolve_page_spec(&run_id).map_err(|err| err.to_string())?;
            let ckpt_text = fs::read_to_string(&checkpoint_path)
                .map_err(|err| format!("failed to read {}: {err}", checkpoint_path.display()))?;

            if !fixture_mode {
                let _header = CheckpointHeader::from_json_str(&ckpt_text).map_err(|err| {
                    format!("invalid checkpoint {}: {err}", checkpoint_path.display())
                })?;
                return Err(
                    "real checkpoint rendering is refused until D6's verified checkpoint loader and D3's learned seats are wired; pass --fixture with a fixture checkpoint JSON to render with stand-in seats"
                        .to_owned(),
                );
            }

            let fixture = FixtureCheckpointFile::from_json_str(&ckpt_text).map_err(|err| {
                format!(
                    "invalid fixture checkpoint {}: {err}",
                    checkpoint_path.display()
                )
            })?;
            let header = fixture.header();
            let standin = StandInCheckpoint::from_fixture(&fixture);
            let (pool, pool_sha) =
                load_ordered_pool(&ordered_pool_path, expected_pool_sha256.as_deref())?;
            let (data, html) = render_owner_view_page(&run_id, &pool, &pool_sha, &header, &standin)
                .map_err(|err| err.to_string())?;

            if let Some(parent) = out_html.parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent)
                        .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
                }
            }
            fs::write(&out_html, html.as_bytes())
                .map_err(|err| format!("failed to write {}: {err}", out_html.display()))?;
            if let Some(json_path) = out_json {
                if let Some(parent) = json_path.parent() {
                    if !parent.as_os_str().is_empty() {
                        fs::create_dir_all(parent).map_err(|err| {
                            format!("failed to create {}: {err}", parent.display())
                        })?;
                    }
                }
                let mut json_text =
                    serde_json::to_string_pretty(&data).map_err(|err| err.to_string())?;
                json_text.push('\n');
                fs::write(&json_path, json_text.as_bytes())
                    .map_err(|err| format!("failed to write {}: {err}", json_path.display()))?;
            }
            Ok(())
        }
    }
}
