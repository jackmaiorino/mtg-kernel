//! CLI for task D16: render one independent static owner-view HTML page (or
//! write small fixture checkpoints for local testing before D3 and D6 merge).
//!
//! Usage:
//! ```text
//! cube_owner_view_v1 --run b1-u0032 --checkpoint CKPT.json \
//!     (--ordered-pool POOL_IDS.json | --cube CUBE.json --gaps GAPS.json [--fence FENCE.json]) \
//!     --out PAGE.html [--json-out DATA.json]
//!
//! cube_owner_view_v1 --write-fixture-checkpoints OUT_DIR
//! ```
//!
//! Each page is rendered independently from its checkpoint and its own
//! `owner-view` seeds. No game is played and no dose read or other page is read.
mtg_kernel::fast_global_allocator!();

use mtg_kernel::card_def::KERNEL_CARDDB_HASH;
use mtg_kernel::cube_draft_v1::CubePool;
use mtg_kernel::cube_owner_view_v1::{
    cube_pool_from_ordered_ids, render_owner_view_page, FixtureCheckpointFile, HeadUpdates,
    PickerMode, StandInCheckpoint, SHAKEDOWN_OWNER_VIEW_RUNS,
};
use std::fs;
use std::path::{Path, PathBuf};

type Fallible<T> = Result<T, String>;

const USAGE: &str = "usage:\n  cube_owner_view_v1 --run RUN_ID --checkpoint CKPT.json (--ordered-pool IDS.json | --cube CUBE.json --gaps GAPS.json [--fence FENCE.json]) --out PAGE.html [--json-out DATA.json]\n  cube_owner_view_v1 --write-fixture-checkpoints OUT_DIR";

enum Command {
    Render {
        run_id: String,
        checkpoint_path: PathBuf,
        pool_source: PoolSource,
        out_html: PathBuf,
        out_json: Option<PathBuf>,
    },
    WriteFixtures {
        out_dir: PathBuf,
    },
}

enum PoolSource {
    OrderedIdsJson(PathBuf),
    CubeFiles {
        cube_list: PathBuf,
        gaps: PathBuf,
        fence: Option<PathBuf>,
    },
}

fn parse_args() -> Fallible<Command> {
    let args: Vec<String> = std::env::args().skip(1).collect();
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
    let mut ordered_pool: Option<PathBuf> = None;
    let mut cube_list: Option<PathBuf> = None;
    let mut gaps: Option<PathBuf> = None;
    let mut fence: Option<PathBuf> = None;
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
                ordered_pool = Some(PathBuf::from(next(i)?));
                i += 2;
            }
            "--cube" => {
                cube_list = Some(PathBuf::from(next(i)?));
                i += 2;
            }
            "--gaps" => {
                gaps = Some(PathBuf::from(next(i)?));
                i += 2;
            }
            "--fence" => {
                fence = Some(PathBuf::from(next(i)?));
                i += 2;
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
    let out_html = out_html.ok_or_else(|| format!("missing --out\n{USAGE}"))?;

    let pool_source = match (ordered_pool, cube_list, gaps) {
        (Some(path), None, None) => {
            if fence.is_some() {
                return Err("--fence cannot be combined with --ordered-pool".to_owned());
            }
            PoolSource::OrderedIdsJson(path)
        }
        (None, Some(cube), Some(gaps_path)) => PoolSource::CubeFiles {
            cube_list: cube,
            gaps: gaps_path,
            fence,
        },
        _ => {
            return Err(format!(
            "supply either --ordered-pool IDS.json or --cube CUBE.json --gaps GAPS.json\n{USAGE}"
        ))
        }
    };

    Ok(Command::Render {
        run_id,
        checkpoint_path,
        pool_source,
        out_html,
        out_json,
    })
}

fn load_pool(source: &PoolSource) -> Fallible<(CubePool, String)> {
    match source {
        PoolSource::OrderedIdsJson(path) => {
            let text = fs::read_to_string(path)
                .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
            let ids: Vec<u16> = serde_json::from_str(&text)
                .map_err(|err| format!("failed to parse ordered pool {}: {err}", path.display()))?;
            cube_pool_from_ordered_ids(&ids).map_err(|err| err.to_string())
        }
        PoolSource::CubeFiles {
            cube_list,
            gaps,
            fence,
        } => {
            let cube_text = fs::read_to_string(cube_list)
                .map_err(|err| format!("failed to read {}: {err}", cube_list.display()))?;
            let gaps_text = fs::read_to_string(gaps)
                .map_err(|err| format!("failed to read {}: {err}", gaps.display()))?;
            let fence_text = match fence {
                Some(path) => Some(
                    fs::read_to_string(path)
                        .map_err(|err| format!("failed to read {}: {err}", path.display()))?,
                ),
                None => None,
            };
            let raw = CubePool::from_json_texts(&cube_text, &gaps_text, fence_text.as_deref())
                .map_err(|err| err.to_string())?;
            let mut ids = raw.ids().to_vec();
            ids.sort_unstable();
            ids.dedup();
            cube_pool_from_ordered_ids(&ids).map_err(|err| err.to_string())
        }
    }
}

fn write_fixture_checkpoints(out_dir: &Path) -> Fallible<()> {
    fs::create_dir_all(out_dir)
        .map_err(|err| format!("failed to create {}: {err}", out_dir.display()))?;
    let carddb = format!("{KERNEL_CARDDB_HASH:016x}");
    let specs: [(&str, &str, u64, HeadUpdates); 6] = [
        (
            SHAKEDOWN_OWNER_VIEW_RUNS[0],
            "b1",
            32,
            HeadUpdates {
                pick: 0,
                value: 0,
                count: 32,
                pointer: 32,
                type_set: 32,
                split: 32,
            },
        ),
        (
            SHAKEDOWN_OWNER_VIEW_RUNS[1],
            "b1",
            64,
            HeadUpdates {
                pick: 0,
                value: 0,
                count: 64,
                pointer: 64,
                type_set: 64,
                split: 64,
            },
        ),
        (
            SHAKEDOWN_OWNER_VIEW_RUNS[2],
            "b1",
            96,
            HeadUpdates {
                pick: 0,
                value: 0,
                count: 96,
                pointer: 96,
                type_set: 96,
                split: 96,
            },
        ),
        (
            SHAKEDOWN_OWNER_VIEW_RUNS[3],
            "b1",
            128,
            HeadUpdates {
                pick: 0,
                value: 0,
                count: 128,
                pointer: 128,
                type_set: 128,
                split: 128,
            },
        ),
        (
            SHAKEDOWN_OWNER_VIEW_RUNS[4],
            "k1",
            64,
            HeadUpdates {
                pick: 64,
                value: 64,
                count: 192,
                pointer: 192,
                type_set: 192,
                split: 192,
            },
        ),
        (
            SHAKEDOWN_OWNER_VIEW_RUNS[5],
            "k1",
            128,
            HeadUpdates {
                pick: 128,
                value: 128,
                count: 256,
                pointer: 256,
                type_set: 256,
                split: 256,
            },
        ),
    ];
    for (run_id, segment, update, head_updates) in specs {
        let fixture =
            FixtureCheckpointFile::for_shakedown_fixture(segment, update, head_updates, &carddb);
        let mut text = serde_json::to_string_pretty(&fixture).map_err(|err| err.to_string())?;
        text.push('\n');
        let path = out_dir.join(format!("{run_id}.checkpoint.json"));
        fs::write(&path, text.as_bytes())
            .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    }
    Ok(())
}

fn run() -> Fallible<()> {
    match parse_args()? {
        Command::WriteFixtures { out_dir } => write_fixture_checkpoints(&out_dir),
        Command::Render {
            run_id,
            checkpoint_path,
            pool_source,
            out_html,
            out_json,
        } => {
            let ckpt_text = fs::read_to_string(&checkpoint_path)
                .map_err(|err| format!("failed to read {}: {err}", checkpoint_path.display()))?;
            let fixture = FixtureCheckpointFile::from_json_str(&ckpt_text).map_err(|err| {
                format!("invalid checkpoint {}: {err}", checkpoint_path.display())
            })?;
            let header = fixture.header();
            let standin = StandInCheckpoint::from_fixture(&fixture);
            let picker_mode = PickerMode::for_run_id(&run_id).map_err(|err| err.to_string())?;
            let (pool, pool_sha) = load_pool(&pool_source)?;
            let (data, html) =
                render_owner_view_page(&run_id, picker_mode, &pool, &pool_sha, &header, &standin)
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

fn main() {
    if let Err(err) = run() {
        eprintln!("cube_owner_view_v1: {err}");
        std::process::exit(1);
    }
}
