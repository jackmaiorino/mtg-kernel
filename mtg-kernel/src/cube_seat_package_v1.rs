//! Verified operator inputs for an M6 human pod. The manifest is pinned by the
//! caller. Each referenced artifact is checked before the pod can start.

use crate::cube_draft_build_v1::card_index::{CardIndexV1, CubeTableSourceV1, CubeTableV1};
use crate::cube_draft_v1::{basic_land_ids, CubePool};
use crate::cube_owner_view_v1::{
    cube_pool_from_ordered_ids, validate_head_updates_raw_json, CheckpointHeader,
    LearnedCheckpointSeats, OwnerViewSeatFactory,
};
use crate::cube_pilot_adapter_v1::D9PilotConfig;
use crate::cube_pod_runner_v1::{is_sha256_hex, PilotKind};
use crate::cube_read_plan_v1::sha256_hex;
use crate::cube_seat_v1::{SeatBuilder, SeatPicker, UniformFourStepBuilder, UniformSeatPicker};
use crate::cube_training_driver_v1::{load_checkpoint_read_only_bytes_v1, CheckpointMetadata};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::path::{Path, PathBuf};

pub const SEAT_PACKAGE_SCHEMA_V1: &str = "mtg-kernel-cube-seat-package/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeatPackageKind {
    UniformStandIn,
    ShakedownStandIn,
    Release,
}

impl SeatPackageKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::UniformStandIn => "Uniform stand-in",
            Self::ShakedownStandIn => "Shakedown stand-in under random play",
            Self::Release => "Release",
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FilePin {
    path: PathBuf,
    sha256: String,
}

impl FilePin {
    fn read(&self, directory: &Path, field: &str) -> Result<Vec<u8>, String> {
        read_verified(&directory.join(&self.path), &self.sha256, field)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftBuildPin {
    checkpoint: FilePin,
    state_sha256: String,
    sampler: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlayPin {
    checkpoint: PathBuf,
    config: D9PilotConfig,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Seats {
    UniformStandIn {},
    ShakedownStandIn {
        draft_build: DraftBuildPin,
    },
    Release {
        draft_build: DraftBuildPin,
        play: Box<PlayPin>,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: String,
    binary_sha256: String,
    cube_list: FilePin,
    fence: FilePin,
    ordered_pool_sha256: String,
    cube_table: FilePin,
    card_index_sha256: String,
    dense_fields: Vec<String>,
    column_scales: Vec<u32>,
    seats: Seats,
}

pub struct SeatPackage {
    kind: SeatPackageKind,
    pool: CubePool,
    draft_build: Option<LoadedDraftBuild>,
    play: Option<PinnedPlayInput>,
}

struct LoadedDraftBuild {
    seats: LearnedCheckpointSeats,
    pick_trained: bool,
}

/// Bytes for the release pilot loader. The pilot loader must still verify
/// decoded metadata against the D9 config before running inference.
pub struct PinnedPlayInput {
    pub config: D9PilotConfig,
    pub checkpoint: Vec<u8>,
}

impl SeatPackage {
    pub fn load(path: &Path, expected_sha256: &str) -> Result<Self, String> {
        let binary = std::env::current_exe().map_err(|e| format!("current executable: {e}"))?;
        let mut file = File::open(binary).map_err(|e| format!("open current executable: {e}"))?;
        let mut hash = Sha256::new();
        std::io::copy(&mut file, &mut hash).map_err(|e| format!("hash current executable: {e}"))?;
        Self::load_bound(path, expected_sha256, &format!("{:x}", hash.finalize()))
    }

    fn load_bound(path: &Path, expected_sha256: &str, binary_sha256: &str) -> Result<Self, String> {
        let manifest: Manifest =
            serde_json::from_slice(&read_verified(path, expected_sha256, "manifest")?)
                .map_err(|e| format!("seat package manifest: {e}"))?;
        if manifest.schema != SEAT_PACKAGE_SCHEMA_V1 {
            return Err("seat package schema mismatch".into());
        }
        check_pin("binary", &manifest.binary_sha256, binary_sha256)?;
        let dir = path.parent().ok_or("manifest has no parent directory")?;
        let list = manifest.cube_list.read(dir, "cube list")?;
        let list = std::str::from_utf8(&list).map_err(|e| format!("cube list: {e}"))?;
        let fence = manifest.fence.read(dir, "fence")?;
        let fence = std::str::from_utf8(&fence).map_err(|e| format!("fence: {e}"))?;
        let fenced_pool =
            CubePool::from_json_texts(list, "{}", Some(fence)).map_err(|e| e.to_string())?;

        let source = CubeTableSourceV1::read_v1(
            &dir.join(&manifest.cube_table.path),
            &manifest.cube_table.sha256,
        )
        .map_err(|e| e.to_string())?;
        let release = matches!(&manifest.seats, Seats::Release { .. });
        let index = if release {
            CardIndexV1::full_v1(list)
        } else {
            CardIndexV1::provisional_v1(list, &source).map(|(index, _)| index)
        }
        .map_err(|e| e.to_string())?;
        check_pin(
            "card index",
            &manifest.card_index_sha256,
            &index.pin_sha256(),
        )?;
        let table = source.bind_v1(&index).map_err(|e| e.to_string())?;
        if !table
            .dense_columns()
            .iter()
            .map(|c| &c.token)
            .eq(&manifest.dense_fields)
        {
            return Err("cube table dense fields mismatch".into());
        }
        if !table
            .divisors()
            .iter()
            .map(|&scale| scale as u32)
            .eq(manifest.column_scales.iter().copied())
        {
            return Err("cube table column scales mismatch".into());
        }

        let basics = basic_land_ids().map_err(|e| e.to_string())?;
        let mut ids: Vec<_> = fenced_pool
            .ids()
            .iter()
            .copied()
            .filter(|id| index.contains(*id) && !basics.contains(id))
            .collect();
        ids.sort_unstable();
        let (pool, pool_sha256) = cube_pool_from_ordered_ids(&ids).map_err(|e| e.to_string())?;
        check_pin("ordered pool", &manifest.ordered_pool_sha256, &pool_sha256)?;

        let (kind, draft_build, play) = match manifest.seats {
            Seats::UniformStandIn {} => (SeatPackageKind::UniformStandIn, None, None),
            Seats::ShakedownStandIn { draft_build } => (
                SeatPackageKind::ShakedownStandIn,
                Some(load_draft_build(
                    &draft_build,
                    dir,
                    &table,
                    &manifest.dense_fields,
                    &manifest.column_scales,
                    false,
                )?),
                None,
            ),
            Seats::Release { draft_build, play } => {
                play.config.validate().map_err(|e| e.to_string())?;
                let checkpoint = read_verified(
                    &dir.join(&play.checkpoint),
                    &play.config.pin.checkpoint_sha256,
                    "play checkpoint",
                )?;
                (
                    SeatPackageKind::Release,
                    Some(load_draft_build(
                        &draft_build,
                        dir,
                        &table,
                        &manifest.dense_fields,
                        &manifest.column_scales,
                        true,
                    )?),
                    Some(PinnedPlayInput {
                        config: play.config,
                        checkpoint,
                    }),
                )
            }
        };
        Ok(Self {
            kind,
            pool,
            draft_build,
            play,
        })
    }

    pub fn kind(&self) -> SeatPackageKind {
        self.kind
    }

    pub fn pool(&self) -> &CubePool {
        &self.pool
    }

    pub fn play_input(&self) -> Option<&PinnedPlayInput> {
        self.play.as_ref()
    }

    pub fn make_picker(&self) -> Box<dyn SeatPicker> {
        match &self.draft_build {
            Some(loaded) if loaded.pick_trained => loaded.seats.make_learned_picker(),
            _ => Box::new(UniformSeatPicker),
        }
    }

    pub fn make_builder(&self) -> Box<dyn SeatBuilder> {
        match &self.draft_build {
            Some(loaded) => loaded.seats.make_learned_builder(),
            None => Box::new(UniformFourStepBuilder),
        }
    }
}

fn load_draft_build(
    pin: &DraftBuildPin,
    directory: &Path,
    table: &CubeTableV1,
    dense_fields: &[String],
    column_scales: &[u32],
    release: bool,
) -> Result<LoadedDraftBuild, String> {
    let bytes = pin.checkpoint.read(directory, "draft/build checkpoint")?;
    let text = std::str::from_utf8(&bytes).map_err(|e| format!("draft/build checkpoint: {e}"))?;
    validate_head_updates_raw_json(text).map_err(|e| e.to_string())?;
    let (checkpoint, model, rows) =
        load_checkpoint_read_only_bytes_v1(&bytes, table).map_err(|e| e.to_string())?;
    let header = CheckpointHeader::from_d6_checkpoint(&checkpoint).map_err(|e| e.to_string())?;
    let expected = CheckpointMetadata::default();
    check_pin(
        "draft/build checkpoint state",
        &pin.state_sha256,
        &header.state_sha256,
    )?;
    if header.model_identity != expected.model_identity
        || header.training_math != expected.training_math
        || header.sampler != pin.sampler
        || checkpoint.dense_fields != dense_fields
        || checkpoint.column_scales != column_scales
    {
        return Err("draft/build checkpoint inference metadata mismatch".into());
    }
    if checkpoint.lineage.is_empty()
        || checkpoint.lineage.iter().any(|segment| {
            if release {
                segment.pilots.is_empty()
                    || segment
                        .pilots
                        .iter()
                        .any(|&pilot| pilot != PilotKind::Checkpoint)
            } else {
                segment.pilots != [PilotKind::Uniform]
            }
        })
    {
        return Err("draft/build checkpoint pilot lineage does not match the package kind".into());
    }
    let heads = header.head_updates;
    if release
        && [
            heads.pick,
            heads.count,
            heads.pointer,
            heads.type_set,
            heads.split,
        ]
        .contains(&0)
    {
        return Err("release draft/build checkpoint has an untrained inference head".into());
    }
    Ok(LoadedDraftBuild {
        seats: LearnedCheckpointSeats::from_model(model, rows, 1).map_err(|e| e.to_string())?,
        pick_trained: heads.pick > 0,
    })
}

fn read_verified(path: &Path, expected_sha256: &str, field: &str) -> Result<Vec<u8>, String> {
    if !is_sha256_hex(expected_sha256) {
        return Err(format!("{field}: invalid SHA-256 pin"));
    }
    let bytes = fs::read(path).map_err(|e| format!("{field}: {}: {e}", path.display()))?;
    check_pin(field, expected_sha256, &sha256_hex(&bytes))?;
    Ok(bytes)
}

fn check_pin(field: &str, expected: &str, actual: &str) -> Result<(), String> {
    if !is_sha256_hex(expected) || expected != actual {
        return Err(format!("{field} SHA-256 mismatch"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card_def::{CARD_DEFS, KERNEL_CARDDB_HASH};
    use crate::cube_draft_build_v1::card_index::TABLE_SCHEMA_V1;
    use crate::cube_draft_v1::{POOL_CARDS, SEATS};
    use serde_json::{json, Value};

    fn write_pinned(directory: &Path, name: &str, bytes: &[u8]) -> Value {
        fs::write(directory.join(name), bytes).unwrap();
        json!({ "path": name, "sha256": sha256_hex(bytes) })
    }

    fn uniform_manifest(directory: &Path, fenced: bool) -> Value {
        let basics = basic_land_ids().unwrap();
        let pool: Vec<_> = (0..CARD_DEFS.len() as u16)
            .filter(|id| !basics.contains(id))
            .take(SEATS * POOL_CARDS + usize::from(fenced))
            .collect();
        let mut index = pool.clone();
        index.extend(basics);
        index.sort_unstable();
        let cube = json!({
            "cards": pool.iter().map(|&id| json!({
                "name": CARD_DEFS[id as usize].name,
            })).collect::<Vec<_>>(),
        });
        let table = json!({
            "schema": TABLE_SCHEMA_V1,
            "pool_path": "synthetic package fixture",
            "kernel_carddb_hash": format!("{KERNEL_CARDDB_HASH:016x}"),
            "vector_width": 1,
            "card_count": index.len(),
            "vocabulary": [{"index": 0, "token": "cost:generic", "kind": "count", "carriers": index.len()}],
            "cards": index.iter().map(|&id| json!({
                "card_id": id, "name": CARD_DEFS[id as usize].name, "vector": [3],
            })).collect::<Vec<_>>(),
        });
        let fence = if fenced {
            json!([{"name": CARD_DEFS[pool[0] as usize].name, "reason": "fixture"}])
        } else {
            json!([])
        };
        json!({
            "schema": SEAT_PACKAGE_SCHEMA_V1,
            "binary_sha256": "a".repeat(64),
            "cube_list": write_pinned(directory, "cube.json", &serde_json::to_vec(&cube).unwrap()),
            "fence": write_pinned(directory, "fence.json", &serde_json::to_vec(&fence).unwrap()),
            "ordered_pool_sha256": crate::cube_owner_view_v1::ordered_pool_sha256(&pool[usize::from(fenced)..]),
            "cube_table": write_pinned(directory, "table.json", &serde_json::to_vec(&table).unwrap()),
            "card_index_sha256": CardIndexV1::from_ids(index).unwrap().pin_sha256(),
            "dense_fields": ["cost:generic"],
            "column_scales": [4],
            "seats": {"kind": "uniform_stand_in"},
        })
    }

    fn load_manifest(directory: &Path, manifest: &Value) -> Result<SeatPackage, String> {
        let bytes = serde_json::to_vec(manifest).unwrap();
        let path = directory.join("package.json");
        fs::write(&path, &bytes).unwrap();
        SeatPackage::load_bound(&path, &sha256_hex(&bytes), &"a".repeat(64))
    }

    fn test_directory() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "cube-seat-package-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&dir).unwrap();
        dir
    }

    #[test]
    fn loads_labelled_uniform_fixture_and_refuses_pin_mismatches() {
        let dir = test_directory();
        let manifest = uniform_manifest(&dir, false);
        let loaded = load_manifest(&dir, &manifest).unwrap();
        assert_eq!(loaded.kind().label(), "Uniform stand-in");
        assert_eq!(loaded.pool().card_count(), SEATS * POOL_CARDS);
        assert!(loaded.play_input().is_none());

        for (key, value, error) in [
            (
                "binary_sha256",
                json!("b".repeat(64)),
                "binary SHA-256 mismatch",
            ),
            (
                "card_index_sha256",
                json!("b".repeat(64)),
                "card index SHA-256 mismatch",
            ),
            (
                "dense_fields",
                json!([]),
                "cube table dense fields mismatch",
            ),
            (
                "column_scales",
                json!([3]),
                "cube table column scales mismatch",
            ),
            (
                "ordered_pool_sha256",
                json!("b".repeat(64)),
                "ordered pool SHA-256 mismatch",
            ),
            ("seats", json!({"kind": "release"}), "missing field"),
        ] {
            let mut changed = manifest.clone();
            changed[key] = value;
            let result = load_manifest(&dir, &changed);
            assert!(
                matches!(result, Err(message) if message.contains(error)),
                "{key}"
            );
        }

        for field in ["draft_build", "play"] {
            let mut changed = manifest.clone();
            changed["seats"][field] = json!({});
            let result = load_manifest(&dir, &changed);
            assert!(
                matches!(result, Err(message) if message.contains("unknown field")),
                "{field}"
            );
        }

        fs::write(dir.join("fence.json"), b"{").unwrap();
        assert!(
            matches!(load_manifest(&dir, &manifest), Err(message) if message == "fence SHA-256 mismatch")
        );
        let result =
            SeatPackage::load_bound(&dir.join("package.json"), &"b".repeat(64), &"a".repeat(64));
        assert!(matches!(result, Err(message) if message == "manifest SHA-256 mismatch"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn verifies_fenced_learned_packages_and_rejects_mixed_artifacts() {
        use crate::cube_owner_view_v1::HeadUpdates;
        use crate::cube_pilot_adapter_v1::{D9DecodingMode, M3_SAMPLE_TEMP1_WIDE_V1};
        use crate::cube_seat_v1::{PassDirection, SeatPickView, LEARNED_SAMPLER_ID};
        use crate::cube_training_driver_v1::{
            seat_package_fixture_checkpoint, LineageSegment, PilotPin, Segment,
        };
        use crate::state::SplitMix64;

        let dir = test_directory();
        let mut manifest = uniform_manifest(&dir, true);
        let list = fs::read_to_string(dir.join("cube.json")).unwrap();
        let index = CardIndexV1::full_v1(&list).unwrap();
        let table = CubeTableV1::load_v1(
            &dir.join("table.json"),
            manifest["cube_table"]["sha256"].as_str().unwrap(),
            &index,
        )
        .unwrap();
        let heads = HeadUpdates {
            pick: 1,
            value: 1,
            count: 1,
            pointer: 1,
            type_set: 1,
            split: 1,
        };
        let play = b"synthetic pilot input; native decoder is a separate boundary";
        let pilot = D9PilotConfig::for_compiled_kernel(
            PilotPin {
                checkpoint_sha256: sha256_hex(play),
                after_state_sha256: "1".repeat(64),
                table_sha256: "2".repeat(64),
                card_database_sha256: format!("{KERNEL_CARDDB_HASH:016x}"),
                training_math: "portable-math-v1".into(),
                source_binary_sha256: "3".repeat(64),
                decoding: M3_SAMPLE_TEMP1_WIDE_V1.into(),
            },
            D9DecodingMode::SampleTemp1Wide,
            0.0,
        );
        fs::write(dir.join("play.ckpt"), play).unwrap();

        let make_pin = |name: &str, lineage: Vec<LineageSegment>, heads: HeadUpdates| {
            let checkpoint = seat_package_fixture_checkpoint(&dir, &table, lineage, heads);
            json!({
                "state_sha256": checkpoint.state_sha256,
                "sampler": LEARNED_SAMPLER_ID,
                "checkpoint": write_pinned(&dir, name, &serde_json::to_vec(&checkpoint).unwrap()),
            })
        };
        let uniform = LineageSegment {
            segment: Segment::BuilderOnly,
            run: "b1".into(),
            pilots: vec![PilotKind::Uniform],
        };
        let learned = LineageSegment {
            segment: Segment::PickPhase,
            run: "k1".into(),
            pilots: vec![PilotKind::Checkpoint],
        };
        let shakedown_pin = make_pin(
            "shakedown.json",
            vec![uniform.clone()],
            HeadUpdates {
                pick: 0,
                value: 0,
                ..heads
            },
        );
        let release_pin = make_pin("release.json", vec![learned.clone()], heads);
        let mixed_pin = make_pin("mixed.json", vec![uniform, learned.clone()], heads);
        let untrained_pin = make_pin("untrained.json", vec![learned], HeadUpdates::default());
        manifest["seats"] = json!({"kind": "shakedown_stand_in", "draft_build": shakedown_pin});
        let loaded = load_manifest(&dir, &manifest).unwrap();
        assert_eq!(loaded.kind(), SeatPackageKind::ShakedownStandIn);
        assert!(loaded.play_input().is_none());
        let expected: Vec<_> = index
            .ids()
            .iter()
            .copied()
            .filter(|id| !basic_land_ids().unwrap().contains(id))
            .skip(1)
            .collect();
        assert_eq!(loaded.pool().ids(), expected);
        let view = SeatPickView {
            round: 0,
            pick: 0,
            direction: PassDirection::Left,
            pack: &expected[..15],
            pool: &[],
            seen_not_taken: &[],
        };
        assert_eq!(
            loaded
                .make_picker()
                .pick(&view, &mut SplitMix64::seed(5))
                .unwrap(),
            UniformSeatPicker
                .pick(&view, &mut SplitMix64::seed(5))
                .unwrap()
        );
        for disallowed_pin in [&mixed_pin, &release_pin] {
            let mut changed = manifest.clone();
            changed["seats"]["draft_build"] = disallowed_pin.clone();
            let result = load_manifest(&dir, &changed);
            assert!(matches!(result, Err(message) if message.contains("pilot lineage")));
        }

        manifest["seats"] = json!({
            "kind": "release",
            "draft_build": release_pin,
            "play": {"checkpoint": "play.ckpt", "config": pilot},
        });
        let loaded = load_manifest(&dir, &manifest).unwrap();
        assert_eq!(loaded.kind(), SeatPackageKind::Release);
        assert_eq!(loaded.play_input().unwrap().checkpoint, play);
        assert_eq!(loaded.play_input().unwrap().config, pilot);

        for (key, sample, error) in [
            (
                "/seats/draft_build/state_sha256",
                json!("f".repeat(64)),
                "checkpoint state SHA-256 mismatch",
            ),
            (
                "/seats/draft_build/sampler",
                json!("greedy"),
                "checkpoint inference metadata mismatch",
            ),
            (
                "/seats/draft_build",
                untrained_pin,
                "untrained inference head",
            ),
            ("/seats/draft_build", shakedown_pin, "pilot lineage"),
            (
                "/seats/play/config/pin/decoding",
                json!("greedy"),
                "decoding",
            ),
        ] {
            let mut changed = manifest.clone();
            *changed.pointer_mut(key).unwrap() = sample;
            let result = load_manifest(&dir, &changed);
            assert!(
                matches!(result, Err(message) if message.contains(error)),
                "{key}"
            );
        }

        fs::write(dir.join("play.ckpt"), b"wrong").unwrap();
        let result = load_manifest(&dir, &manifest);
        assert!(matches!(result, Err(message) if message == "play checkpoint SHA-256 mismatch"));
        fs::write(dir.join("play.ckpt"), play).unwrap();
        let checkpoint = dir.join("release.json");
        let mut bytes = fs::read(&checkpoint).unwrap();
        bytes.push(b' ');
        fs::write(checkpoint, bytes).unwrap();
        let result = load_manifest(&dir, &manifest);
        assert!(
            matches!(result, Err(message) if message == "draft/build checkpoint SHA-256 mismatch")
        );
        fs::remove_dir_all(&dir).unwrap();
    }
}
