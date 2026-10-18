//! Opt-in exact cache of anchor rows and of passed anchor pre-flights, in the
//! same directory as the bank cache (`MTG_KERNEL_ARCHIVE_BANK_CACHE`).
//!
//! Every phase-1 run reads its anchor trajectories (parsing and validating
//! each JSON file and building every learner row's tensor) and runs the
//! anchor pre-flight (one training step over every anchor-training row on a
//! copy of the fresh trainer). Both are pure functions of this binary, the
//! parent and the pinned inputs, so an entry is keyed by this binary's
//! SHA-256, the parent's identity and state digest, and the exact pins (and,
//! for the pre-flight, the recipe values and update scope that step reads).
//!
//! A row hit still reads every anchor file and checks its size and SHA-256
//! against its pin, and still records every episode id in pin order with the
//! same duplicate check, so a changed, missing or repeated input fails exactly
//! as before; only the parsing, validation and tensor building are skipped.
//! A pre-flight entry is written only after the step passed. Every entry
//! starts with its key's digest, and a rows entry ends with the SHA-256 of
//! everything before it, checked before any row is used; strings and counts
//! are bounded before anything is allocated. A missing, unreadable, damaged
//! or mismatched entry, or a key that cannot be built, falls back to the full
//! path. Nothing here is recorded in any result.

use super::bank_cache::{publish_v1, write_tensor_v1, write_words_v1, Reader};
use super::*;
use std::io::{BufReader, Read, Write};

const ROWS_SCHEMA: &str = "mtg-kernel/archive-anchor-rows-cache/v1";
const PREFLIGHT_SCHEMA: &str = "mtg-kernel/archive-anchor-preflight-cache/v1";
const ROWS_MAGIC: &[u8; 8] = b"MTGAAR01";
const PREFLIGHT_MAGIC: &[u8; 8] = b"MTGAPF01";
/// Longest episode id or deck label an entry may hold.
const STRING_LIMIT: u64 = 4096;

/// An entry's path and the digest of its key, which the entry also stores.
pub(super) struct EntryV1 {
    path: PathBuf,
    key: [u8; 32],
}

impl EntryV1 {
    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

/// The rows of one trajectory: its episode id and its learner rows.
pub(super) type TrajectoryRowsV1 = (String, Vec<AnchorRowV1>);

fn entry(key: Value, extension: &str) -> Option<EntryV1> {
    let directory = std::env::var_os("MTG_KERNEL_ARCHIVE_BANK_CACHE")?;
    let key = runtime_binary_sha256().and_then(|binary| {
        let mut key = key;
        key["runtime_binary_sha256"] = json!(binary);
        serde_json::to_vec(&key).map_err(err)
    });
    match key {
        Ok(key) => {
            let key: [u8; 32] = Sha256::digest(&key).into();
            Some(EntryV1 {
                path: PathBuf::from(directory).join(format!("{}.{extension}", hex(&key))),
                key,
            })
        }
        Err(e) => {
            eprintln!("archive anchor cache: off for this run, no key: {e}");
            None
        }
    }
}

/// The entry for one anchor split's rows, or `None` when the cache is off.
pub(super) fn rows_path_v1(
    pins: &[PinnedFileV1],
    policy: &FrozenPlayPolicyV1,
    parent_state_sha256: &str,
) -> Option<EntryV1> {
    entry(
        json!({"schema":ROWS_SCHEMA,"pins":pins,"parent_state_sha256":parent_state_sha256,
            "policy":format!("{:?}", policy.identity_v1()),
            "generation":format!("{:?}", policy.feature_identity_v1().generation)}),
        "anchor",
    )
}

/// The entry for a passed pre-flight, or `None` when the cache is off.
pub(super) fn preflight_path_v1(
    training_pins: &[PinnedFileV1],
    parent: &ExpandedModelSourceV1,
    parent_state_sha256: &str,
    recipe: &RecipeV1,
    update_scope: ArchiveDistillationUpdateScopeV1,
) -> Option<EntryV1> {
    entry(
        json!({"schema":PREFLIGHT_SCHEMA,"pins":training_pins,"parent":parent,
            "parent_state_sha256":parent_state_sha256,
            "learning_rate_bits":recipe.learning_rate.to_bits(),
            "anchor_coefficient_bits":recipe.anchor_coefficient.to_bits(),
            "update_scope":update_scope}),
        "preflight",
    )
}

/// The cached rows of `trajectories` trajectories, or `None` (noted on
/// stderr when the entry exists but does not decode).
pub(super) fn load_rows_v1(entry: &EntryV1, trajectories: usize) -> Option<Vec<TrajectoryRowsV1>> {
    let path = entry.path();
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => return miss(path, &e.to_string()),
    };
    let decoded = file.metadata().map_err(err).and_then(|meta| {
        let mut reader = Reader {
            input: Hashing {
                inner: BufReader::new(file),
                hash: Sha256::new(),
            },
            remaining: meta.len().checked_sub(32).ok_or("the entry is too short")?,
        };
        let rows = decode_rows_v1(&mut reader, &entry.key, trajectories)?;
        let digest: [u8; 32] = reader.input.hash.finalize().into();
        let mut stored = [0; 32];
        reader.input.inner.read_exact(&mut stored).map_err(err)?;
        if stored != digest {
            return Err("the entry's checksum differs".into());
        }
        Ok(rows)
    });
    match decoded {
        Ok(rows) => Some(rows),
        Err(reason) => miss(path, &reason),
    }
}

fn miss<T>(path: &Path, reason: &str) -> Option<T> {
    eprintln!(
        "archive anchor cache: ignoring {}: {reason}; reading the trajectories",
        path.display()
    );
    None
}

/// A reader that hashes every byte it passes on.
struct Hashing<R> {
    inner: R,
    hash: Sha256,
}

impl<R: Read> Read for Hashing<R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.read(buffer)?;
        self.hash.update(&buffer[..read]);
        Ok(read)
    }
}

fn decode_rows_v1<R: Read>(
    reader: &mut Reader<R>,
    key: &[u8; 32],
    trajectories: usize,
) -> Result<Vec<TrajectoryRowsV1>, String> {
    if &reader.bytes::<8>()? != ROWS_MAGIC {
        return Err("not an anchor rows entry".into());
    }
    if &reader.bytes::<32>()? != key {
        return Err("the entry belongs to another key".into());
    }
    if reader.word()? != trajectories as u64 {
        return Err("trajectory count differs from the pins".into());
    }
    let mut out = Vec::new();
    out.try_reserve_exact(trajectories).map_err(err)?;
    for _ in 0..trajectories {
        let episode = reader.string(STRING_LIMIT)?;
        let count = reader.word()?;
        // Each row takes at least its fixed fields and a tensor's lengths.
        if count.saturating_mul(8 + 8 + 8 + 4 + 13 * 8) > reader.remaining {
            return Err("the entry is too short for its row count".into());
        }
        let count = usize::try_from(count).map_err(err)?;
        let mut rows = Vec::new();
        rows.try_reserve_exact(count).map_err(err)?;
        for _ in 0..count {
            let step = reader.word()?;
            let learner_deck = reader.string(STRING_LIMIT)?;
            let logits = reader.u32s()?;
            let value = u32::from_le_bytes(reader.bytes::<4>()?);
            let tensor = reader.tensor()?;
            rows.push(AnchorRowV1 {
                episode: episode.clone(),
                step,
                learner_deck,
                logits,
                value,
                tensor,
            });
        }
        out.push((episode, rows));
    }
    if reader.remaining != 0 {
        return Err("trailing bytes after the last trajectory".into());
    }
    Ok(out)
}

/// Stores freshly read rows; a failure only skips the cache.
pub(super) fn store_rows_v1(entry: &EntryV1, trajectories: &[TrajectoryRowsV1]) {
    if entry.path().exists() {
        return;
    }
    let mut payload = Vec::new();
    if encode_rows_v1(&mut payload, &entry.key, trajectories).is_err() {
        return;
    }
    let digest: [u8; 32] = Sha256::digest(&payload).into();
    publish_v1(entry.path(), |out| {
        out.write_all(&payload)?;
        out.write_all(&digest)
    });
}

fn encode_rows_v1(
    out: &mut impl Write,
    key: &[u8; 32],
    trajectories: &[TrajectoryRowsV1],
) -> std::io::Result<()> {
    let string = |out: &mut dyn Write, s: &str| {
        out.write_all(&(s.len() as u64).to_le_bytes())?;
        out.write_all(s.as_bytes())
    };
    out.write_all(ROWS_MAGIC)?;
    out.write_all(key)?;
    out.write_all(&(trajectories.len() as u64).to_le_bytes())?;
    for (episode, rows) in trajectories {
        string(out, episode)?;
        out.write_all(&(rows.len() as u64).to_le_bytes())?;
        for row in rows {
            debug_assert_eq!(&row.episode, episode);
            out.write_all(&row.step.to_le_bytes())?;
            string(out, &row.learner_deck)?;
            write_words_v1(out, &row.logits, |v| v.to_le_bytes())?;
            out.write_all(&row.value.to_le_bytes())?;
            write_tensor_v1(out, &row.tensor)?;
        }
    }
    Ok(())
}

/// The cached pre-flight row count if the entry records a pass over exactly
/// `rows` rows.
pub(super) fn load_preflight_v1(entry: &EntryV1, rows: usize) -> Option<usize> {
    let file = fs::File::open(entry.path()).ok()?;
    // One byte past the record's size, to reject anything longer.
    let mut bytes = Vec::with_capacity(49);
    file.take(49).read_to_end(&mut bytes).ok()?;
    if bytes == preflight_record_v1(&entry.key, rows) {
        Some(rows)
    } else {
        miss(
            entry.path(),
            "the entry does not record a pass over these rows",
        )
    }
}

fn preflight_record_v1(key: &[u8; 32], rows: usize) -> Vec<u8> {
    let mut record = PREFLIGHT_MAGIC.to_vec();
    record.extend_from_slice(key);
    record.extend_from_slice(&(rows as u64).to_le_bytes());
    record
}

/// Records a passed pre-flight over `rows` rows.
pub(super) fn store_preflight_v1(entry: &EntryV1, rows: usize) {
    if entry.path().exists() {
        return;
    }
    publish_v1(entry.path(), |out| {
        out.write_all(&preflight_record_v1(&entry.key, rows))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(episode: &str, step: u64) -> AnchorRowV1 {
        AnchorRowV1 {
            episode: episode.into(),
            step,
            learner_deck: format!("deck-{step}"),
            logits: vec![1, u32::MAX, 0x7fc0_0000],
            value: 0x8000_0000,
            tensor: NativeFlatDecisionTensorV2 {
                state: vec![f32::from_bits(3), -0.0],
                object_features: vec![],
                object_card_ids: vec![-1, 7],
                object_groups: vec![2],
                object_node_ids: vec![],
                edge_features: vec![f32::NAN],
                edge_source_indices: vec![0],
                edge_target_indices: vec![1],
                action_features: vec![1.5],
                action_ref_features: vec![f32::INFINITY],
                action_ref_card_ids: vec![9],
                action_ref_action_indices: vec![0],
                action_ref_node_indices: vec![3],
            },
        }
    }

    fn same(a: &AnchorRowV1, b: &AnchorRowV1) -> bool {
        a.episode == b.episode
            && a.step == b.step
            && a.learner_deck == b.learner_deck
            && a.logits == b.logits
            && a.value == b.value
            && tensor_sha256_v1(&a.tensor).unwrap() == tensor_sha256_v1(&b.tensor).unwrap()
    }

    const KEY: [u8; 32] = [7; 32];

    fn decode(bytes: &[u8], trajectories: usize) -> Result<Vec<TrajectoryRowsV1>, String> {
        let mut reader = Reader {
            input: bytes,
            remaining: bytes.len() as u64,
        };
        decode_rows_v1(&mut reader, &KEY, trajectories)
    }

    #[test]
    fn anchor_rows_round_trip_exactly() {
        let trajectories = vec![
            ("e1".to_string(), vec![row("e1", 3), row("e1", 9)]),
            ("e2".to_string(), vec![]),
            ("e3".to_string(), vec![row("e3", 1)]),
        ];
        let mut bytes = Vec::new();
        encode_rows_v1(&mut bytes, &KEY, &trajectories).unwrap();
        let decoded = decode(&bytes, 3).unwrap();
        assert_eq!(decoded.len(), 3);
        for ((ea, ra), (eb, rb)) in trajectories.iter().zip(&decoded) {
            assert_eq!(ea, eb);
            assert_eq!(ra.len(), rb.len());
            assert!(ra.iter().zip(rb).all(|(a, b)| same(a, b)));
        }
        assert!(decode(&bytes, 2).is_err());
        assert!(decode(&bytes[..bytes.len() - 1], 3).is_err());
        let mut extra = bytes.clone();
        extra.push(0);
        assert!(decode(&extra, 3).is_err());
        let mut huge = bytes.clone();
        // The first trajectory's row count follows the magic, the key, the
        // trajectory count and the 2-byte episode id with its length.
        let at = 8 + 32 + 8 + 8 + 2;
        huge[at..at + 8].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(decode(&huge, 3).is_err());
        let mut other_key = bytes.clone();
        other_key[8] ^= 1;
        assert!(decode(&other_key, 3).is_err());
        let mut long_id = bytes.clone();
        long_id[8 + 32 + 8..8 + 32 + 8 + 8].copy_from_slice(&(STRING_LIMIT + 1).to_le_bytes());
        assert!(decode(&long_id, 3).is_err());
    }

    #[test]
    fn anchor_entries_check_their_checksum_and_preflight_records_their_size() {
        let directory =
            std::env::temp_dir().join(format!("anchor-cache-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        let entry = EntryV1 {
            path: directory.join("rows.anchor"),
            key: KEY,
        };
        let trajectories = vec![("e1".to_string(), vec![row("e1", 3)])];
        store_rows_v1(&entry, &trajectories);
        assert!(load_rows_v1(&entry, 1).is_some());
        // Flip a byte of the payload (the row's step): the checksum rejects it.
        let mut bytes = fs::read(entry.path()).unwrap();
        bytes[8 + 32 + 8 + 8 + 2 + 8] ^= 1;
        fs::write(entry.path(), &bytes).unwrap();
        assert!(load_rows_v1(&entry, 1).is_none());
        let preflight = EntryV1 {
            path: directory.join("pass.preflight"),
            key: KEY,
        };
        store_preflight_v1(&preflight, 5);
        assert_eq!(load_preflight_v1(&preflight, 5), Some(5));
        assert_eq!(load_preflight_v1(&preflight, 6), None);
        let mut longer = fs::read(preflight.path()).unwrap();
        longer.push(0);
        fs::write(preflight.path(), &longer).unwrap();
        assert_eq!(load_preflight_v1(&preflight, 5), None);
        let _ = fs::remove_dir_all(&directory);
    }
}
