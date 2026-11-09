//! Content-addressed output for the ordinary fresh native training chain.
//! The package root supplies the context for every relative pin.

use super::*;
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;

const SCHEMA: &str = "mtg-kernel-native-expanded-portable-chain/v1";
const MANIFEST: &str = "chain.json";
const OBJECTS: &str = "objects";

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Chain {
    schema: String,
    source: ExpandedModelSourceV1,
}

#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
enum Kind {
    Import,
    Checkpoint,
    Trajectory,
    Blob,
}

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
struct Object {
    kind: Kind,
    path: PathBuf,
    sha256: String,
}

impl Object {
    fn new(kind: Kind, pin: &PinnedFileV1) -> Result<Self, String> {
        ensure(
            pin.sha256.len() == 64
                && pin
                    .sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "portable chain requires a lowercase SHA-256 pin",
        )?;
        Ok(Self {
            kind,
            path: pin.path.clone(),
            sha256: pin.sha256.clone(),
        })
    }

    fn resolve(&self, root: &Path) -> Result<PathBuf, String> {
        ensure(
            self.path.as_os_str() == OsStr::new(&object_path(&self.sha256)),
            "portable pin does not name its content address",
        )?;
        let path = root.join(OBJECTS).join(&self.sha256);
        ensure(
            fs::canonicalize(&path).map_err(err)? == path,
            "portable pin follows a link",
        )?;
        Ok(path)
    }

    fn read(&self, root: Option<&Path>) -> Result<Record, String> {
        let path = if let Some(root) = root {
            self.resolve(root)?
        } else {
            ensure(self.path.is_absolute(), "source pin is not absolute")?;
            self.path.clone()
        };
        let bytes = read_pinned_bytes(&PinnedFileV1 {
            path,
            sha256: self.sha256.clone(),
        })
        .map_err(|error| format!("portable chain input {}: {error}", self.path.display()))?;
        Record::decode(self.kind, bytes)
    }
}

fn object_path(sha256: &str) -> String {
    format!("{OBJECTS}/{sha256}")
}

fn visit_source(
    source: &mut ExpandedModelSourceV1,
    visit: &mut impl FnMut(Kind, &mut PinnedFileV1) -> Result<(), String>,
) -> Result<(), String> {
    ensure(
        source.s_learner.is_none(),
        "portable chain requires an ordinary fresh model source",
    )?;
    visit(Kind::Import, &mut source.play_import)?;
    if let Some(checkpoint) = &mut source.checkpoint {
        visit(Kind::Checkpoint, checkpoint)?;
    }
    Ok(())
}

enum Record {
    Import(Box<ExpandedFreshInitializationSourceV1>),
    Checkpoint(Box<ExpandedCheckpointV1>),
    Trajectory(Box<ExpandedTrajectoryV1>),
    Blob(Vec<u8>),
}

impl Record {
    fn decode(kind: Kind, bytes: Vec<u8>) -> Result<Self, String> {
        match kind {
            Kind::Import => {
                let descriptor: ExpandedFreshInitializationSourceV1 =
                    serde_json::from_slice(&bytes).map_err(err)?;
                ensure(
                    descriptor.schema == fresh_initialization_source::SOURCE_SCHEMA,
                    "portable chain requires a fresh initialization descriptor",
                )?;
                Ok(Self::Import(Box::new(descriptor)))
            }
            Kind::Checkpoint => {
                let checkpoint = checkpoint_container_v1::parse_checkpoint_v1(&bytes)?;
                ensure(
                    checkpoint.schema == FRESH_CHECKPOINT_SCHEMA
                        && checkpoint.source_import.is_fresh_v1()
                        && checkpoint.input_contract.is_none()
                        && checkpoint.e_supported.is_none()
                        && checkpoint.soft_q.is_none()
                        && checkpoint.search_targets.is_none()
                        && checkpoint.ex1.is_none()
                        && checkpoint.registry_transfer.is_none()
                        && checkpoint.archive_distillation.is_none(),
                    "portable chain requires an ordinary fresh checkpoint",
                )?;
                Ok(Self::Checkpoint(Box::new(checkpoint)))
            }
            Kind::Trajectory => {
                let record = record_envelope_v1::open_v1(&bytes, MAX_FILE_BYTES)?;
                let trajectory: ExpandedTrajectoryV1 =
                    serde_json::from_slice(&record).map_err(err)?;
                ensure(
                    trajectory.schema == FRESH_TRAJECTORY_SCHEMA
                        && trajectory.source_import.is_fresh_v1()
                        && trajectory.seat_behaviors.is_some()
                        && trajectory.input_contract.is_none()
                        && trajectory.search_played.is_none()
                        && trajectory.episode.opponent_search.is_none()
                        && trajectory.episode.opponent_kind.is_none()
                        && trajectory.episode.reset.is_none(),
                    "portable chain requires a fresh trajectory with both source bindings",
                )?;
                validate_behavior_shape_v1(&trajectory)?;
                Ok(Self::Trajectory(Box::new(trajectory)))
            }
            Kind::Blob => Ok(Self::Blob(bytes)),
        }
    }

    fn visit_pins(
        &mut self,
        mut visit: impl FnMut(Kind, &mut PinnedFileV1) -> Result<(), String>,
    ) -> Result<(), String> {
        match self {
            Self::Import(descriptor) => {
                visit(Kind::Blob, &mut descriptor.initialization)?;
                visit(Kind::Blob, &mut descriptor.parameters)?;
            }
            Self::Checkpoint(checkpoint) => {
                for pin in &mut checkpoint.trajectories {
                    visit(Kind::Trajectory, pin)?;
                }
            }
            Self::Trajectory(trajectory) => {
                // Admission above requires both the episode and seat bindings.
                visit_source(trajectory.episode.opponent.as_mut().unwrap(), &mut visit)?;
                for behavior in trajectory.seat_behaviors.as_mut().unwrap() {
                    let prior_pin = behavior.source.checkpoint.as_ref();
                    ensure(
                        behavior.identity.checkpoint_sha256.as_deref()
                            == prior_pin.map(|pin| pin.sha256.as_str()),
                        "trajectory checkpoint hash differs from its source pin",
                    )?;
                    visit_source(&mut behavior.source, &mut visit)?;
                    behavior.identity.checkpoint_sha256 = behavior
                        .source
                        .checkpoint
                        .as_ref()
                        .map(|pin| pin.sha256.clone());
                }
            }
            Self::Blob(_) => {}
        }
        Ok(())
    }

    fn encode(self) -> Result<Vec<u8>, String> {
        match self {
            Self::Import(descriptor) => serde_json::to_vec(&descriptor).map_err(err),
            Self::Checkpoint(checkpoint) => {
                if checkpoint_container_v1::admit_writer_v1(&checkpoint).is_ok() {
                    checkpoint_container_v1::encode_v1(&checkpoint)
                } else {
                    serde_json::to_vec(&checkpoint).map_err(err)
                }
            }
            Self::Trajectory(trajectory) => serde_json::to_vec(&trajectory).map_err(err),
            Self::Blob(bytes) => Ok(bytes),
        }
    }
}

enum Work {
    Discover(Object),
    Publish(Object),
}

/// Publish a complete, independent copy of a pinned fresh training graph.
/// The caller supplies an unused absolute output directory. A write failure
/// leaves that directory without the completion manifest.
pub fn write_v1(source: &ExpandedModelSourceV1, output: &Path) -> Result<PinnedFileV1, String> {
    ensure(
        output.is_absolute(),
        "absolute portable output directory required",
    )?;
    let mut source = source.clone();
    let mut pending = Vec::new();
    visit_source(&mut source, &mut |kind, pin| {
        pending.push(Work::Discover(Object::new(kind, pin)?));
        Ok(())
    })?;
    fs::create_dir(output).map_err(err)?;
    let objects = output.join(OBJECTS);
    fs::create_dir(&objects).map_err(err)?;
    let mut active = HashSet::new();
    let mut published = HashMap::<Object, PinnedFileV1>::new();
    let mut addresses = HashSet::new();
    let rewrite = |kind,
                   pin: &mut PinnedFileV1,
                   published: &HashMap<Object, PinnedFileV1>|
     -> Result<(), String> {
        *pin = published
            .get(&Object::new(kind, pin)?)
            .cloned()
            .ok_or("portable dependency was not published")?;
        Ok(())
    };
    // Keep only graph keys on the work stack. A long chain may have large
    // checkpoints, so retaining one decoded record per ancestor is unsafe.
    while let Some(work) = pending.pop() {
        match work {
            Work::Discover(object) => {
                if published.contains_key(&object) {
                    continue;
                }
                ensure(active.insert(object.clone()), "cyclic portable dependency")?;
                let mut record = object.read(None)?;
                pending.push(Work::Publish(object));
                record.visit_pins(|kind, pin| {
                    pending.push(Work::Discover(Object::new(kind, pin)?));
                    Ok(())
                })?;
            }
            Work::Publish(object) => {
                let mut record = object.read(None)?;
                record.visit_pins(|kind, pin| rewrite(kind, pin, &published))?;
                let bytes = record.encode()?;
                let sha256 = sha(&bytes);
                if addresses.insert(sha256.clone()) {
                    publish_json_bytes_v1(&objects, &sha256, &bytes)?;
                }
                active.remove(&object);
                published.insert(
                    object,
                    PinnedFileV1 {
                        path: object_path(&sha256).into(),
                        sha256,
                    },
                );
            }
        }
    }
    visit_source(&mut source, &mut |kind, pin| rewrite(kind, pin, &published))?;
    let root = fs::canonicalize(output).map_err(err)?;
    restore_final(&root, &source)?;
    publish_json(
        output,
        MANIFEST,
        &Chain {
            schema: SCHEMA.into(),
            source,
        },
    )
}

/// Validate all package links after relocation and restore the final policy and
/// Adam state through the same checks as an ordinary fresh continuation.
pub fn inspect_v1(
    output: &Path,
    expected_manifest_sha256: &str,
) -> Result<ExpandedInferenceIdentityV1, String> {
    ensure(output.is_absolute(), "absolute portable directory required")?;
    let root = fs::canonicalize(output).map_err(err)?;
    let manifest = root.join(MANIFEST);
    ensure(
        fs::canonicalize(&manifest).map_err(err)? == manifest,
        "portable manifest follows a link",
    )?;
    use std::io::Read;
    const MANIFEST_CAP: u64 = 1024 * 1024;
    let metadata = fs::metadata(&manifest).map_err(err)?;
    ensure(
        metadata.is_file() && metadata.len() <= MANIFEST_CAP,
        "portable manifest is not a bounded regular file",
    )?;
    let file = fs::File::open(manifest).map_err(err)?;
    let mut bytes = Vec::new();
    file.take(MANIFEST_CAP + 1)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    ensure(
        bytes.len() as u64 <= MANIFEST_CAP,
        "portable manifest exceeds byte cap",
    )?;
    ensure(
        sha(&bytes) == expected_manifest_sha256,
        "portable manifest SHA differs",
    )?;
    let mut chain: Chain = serde_json::from_slice(&bytes).map_err(err)?;
    ensure(chain.schema == SCHEMA, "unknown portable chain schema")?;
    let mut pending = Vec::new();
    visit_source(&mut chain.source, &mut |kind, pin| {
        pending.push(Object::new(kind, pin)?);
        Ok(())
    })?;
    let mut seen = HashSet::new();
    while let Some(object) = pending.pop() {
        if seen.insert(object.clone()) {
            object.read(Some(&root))?.visit_pins(|kind, pin| {
                pending.push(Object::new(kind, pin)?);
                Ok(())
            })?;
        }
    }

    restore_final(&root, &chain.source)
}

fn restore_final(
    root: &Path,
    source: &ExpandedModelSourceV1,
) -> Result<ExpandedInferenceIdentityV1, String> {
    let Record::Import(mut descriptor) =
        Object::new(Kind::Import, &source.play_import)?.read(Some(root))?
    else {
        unreachable!("import decoder returns an import");
    };
    descriptor.initialization.path =
        Object::new(Kind::Blob, &descriptor.initialization)?.resolve(root)?;
    descriptor.parameters.path = Object::new(Kind::Blob, &descriptor.parameters)?.resolve(root)?;
    let mut policy =
        fresh_initialization_source::load_policy_v1(&descriptor, &source.feature_transfer)?;
    let model = policy.training_model_v3();
    let state = if let Some(pin) = &source.checkpoint {
        let Record::Checkpoint(checkpoint) =
            Object::new(Kind::Checkpoint, pin)?.read(Some(root))?
        else {
            unreachable!("checkpoint decoder returns a checkpoint");
        };
        restore_checkpoint_state_v1(&checkpoint, &mut policy, model)?
    } else {
        NativePolicyValueTrainStateV1::new_v1(model).map_err(err)?
    };
    inference_identity_v1(source, &policy, &state)
}

#[cfg(test)]
mod tests;
