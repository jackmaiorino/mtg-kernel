//! Opt-in exact cache of a bank's extracted training tensors.
//!
//! Every phase-1 run and every probe re-extracts its bank: it replays each
//! archive game and re-tensorizes every row, then requires the rows to equal
//! the pinned bank. With `MTG_KERNEL_ARCHIVE_BANK_CACHE=<directory>` the first
//! run to reproduce a bank also stores its tensors there, keyed by this
//! binary's SHA-256 and the exact parent, lineage, games and bank pins. A
//! later run with the same key reads the tensors back, takes the rows from
//! the pinned bank (which the storing run proved equal to its extraction),
//! and requires every tensor's `TensorBitsV1` SHA-256 to equal the row's
//! recorded `tensor_sha256` before anything trains on it. A missing,
//! unreadable or mismatched entry, or a key that cannot be built, falls back
//! to full extraction. The cache changes cost, never output: nothing it does
//! is recorded in any result.

use super::*;
use std::io::{BufReader, BufWriter, Read, Write};

const SCHEMA: &str = "mtg-kernel/archive-bank-tensor-cache/v1";
const MAGIC: &[u8; 8] = b"MTGABC01";
/// Each tensor stores 13 vector lengths, so it takes at least this many bytes.
const MIN_TENSOR_BYTES: u64 = 13 * 8;

/// The entry path for this binary and these pins, or `None` when the cache
/// is off or its key cannot be built (noted on stderr).
pub(super) fn entry_path_v1(
    parent: &ExpandedModelSourceV1,
    lineage: &str,
    games: &PinnedFileV1,
    bank: &PinnedFileV1,
) -> Option<PathBuf> {
    let directory = std::env::var_os("MTG_KERNEL_ARCHIVE_BANK_CACHE")?;
    let key = runtime_binary_sha256().and_then(|binary| {
        let key = json!({"schema":SCHEMA,"runtime_binary_sha256":binary,
            "parent":parent,"lineage":lineage,"games":games,"bank":bank});
        serde_json::to_vec(&key).map_err(err)
    });
    match key {
        Ok(key) => Some(PathBuf::from(directory).join(format!("{}.bin", sha(&key)))),
        Err(e) => {
            eprintln!("archive bank cache: off for this run, no key: {e}");
            None
        }
    }
}

/// The bank's rows with their cached tensors, or `None` (and a note on
/// stderr) when the entry is missing or does not match the bank exactly.
pub(super) fn load_v1(path: &Path, bank: &ArchiveBankV1) -> Option<ExtractedRowsV1> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => return miss(path, &e.to_string()),
    };
    let tensors = match file
        .metadata()
        .map_err(err)
        .and_then(|meta| decode_v1(BufReader::new(file), meta.len(), bank.rows.len()))
    {
        Ok(tensors) => tensors,
        Err(reason) => return miss(path, &reason),
    };
    let mut rows = Vec::with_capacity(tensors.len());
    for (row, tensor) in bank.rows.iter().zip(tensors) {
        match tensor_sha256_v1(&tensor) {
            Ok(digest) if digest == row.tensor_sha256 => rows.push((row.clone(), tensor)),
            _ => {
                return miss(
                    path,
                    "a tensor differs from its row's recorded tensor_sha256",
                )
            }
        }
    }
    Some(rows)
}

fn miss(path: &Path, reason: &str) -> Option<ExtractedRowsV1> {
    eprintln!(
        "archive bank cache: ignoring {}: {reason}; extracting",
        path.display()
    );
    None
}

/// Stores freshly reproduced tensors; a failure only skips the cache.
pub(super) fn store_v1(path: &Path, rows: &ExtractedRowsV1) {
    if path.exists() {
        return;
    }
    publish_v1(path, |out| {
        encode_v1(rows.iter().map(|(_, tensor)| tensor), rows.len(), out)
    });
}

/// Writes an entry through a per-process partial file, syncs it and links it
/// into place, never replacing an existing entry; a failure only skips it.
pub(super) fn publish_v1(
    path: &Path,
    write: impl FnOnce(&mut BufWriter<fs::File>) -> std::io::Result<()>,
) {
    let partial = path.with_extension(format!("partial-{}", std::process::id()));
    let written = path
        .parent()
        .map_or(Ok(()), fs::create_dir_all)
        .and_then(|()| {
            let mut out = BufWriter::new(fs::File::create_new(&partial)?);
            write(&mut out)?;
            out.into_inner().map_err(|e| e.into_error())?.sync_all()
        })
        .and_then(|()| match fs::hard_link(&partial, path) {
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
            other => other,
        });
    let _ = fs::remove_file(&partial);
    if let Err(e) = written {
        eprintln!("archive cache: not stored at {}: {e}", path.display());
    }
}

fn encode_v1<'a>(
    tensors: impl Iterator<Item = &'a NativeFlatDecisionTensorV2>,
    count: usize,
    out: &mut impl Write,
) -> std::io::Result<()> {
    out.write_all(MAGIC)?;
    out.write_all(&(count as u64).to_le_bytes())?;
    for t in tensors {
        write_tensor_v1(out, t)?;
    }
    Ok(())
}

/// A length-prefixed little-endian vector.
pub(super) fn write_words_v1<const N: usize, T>(
    out: &mut impl Write,
    values: &[T],
    bytes: fn(&T) -> [u8; N],
) -> std::io::Result<()> {
    out.write_all(&(values.len() as u64).to_le_bytes())?;
    values
        .iter()
        .try_for_each(|value| out.write_all(&bytes(value)))
}

/// One tensor's 13 vectors, f32 as their bits.
pub(super) fn write_tensor_v1(
    out: &mut impl Write,
    t: &NativeFlatDecisionTensorV2,
) -> std::io::Result<()> {
    fn floats(out: &mut impl Write, values: &[f32]) -> std::io::Result<()> {
        write_words_v1(out, values, |v| v.to_bits().to_le_bytes())
    }
    fn integers(out: &mut impl Write, values: &[i64]) -> std::io::Result<()> {
        write_words_v1(out, values, |v| v.to_le_bytes())
    }
    {
        floats(out, &t.state)?;
        floats(out, &t.object_features)?;
        integers(out, &t.object_card_ids)?;
        integers(out, &t.object_groups)?;
        integers(out, &t.object_node_ids)?;
        floats(out, &t.edge_features)?;
        integers(out, &t.edge_source_indices)?;
        integers(out, &t.edge_target_indices)?;
        floats(out, &t.action_features)?;
        floats(out, &t.action_ref_features)?;
        integers(out, &t.action_ref_card_ids)?;
        integers(out, &t.action_ref_action_indices)?;
        integers(out, &t.action_ref_node_indices)?;
    }
    Ok(())
}

/// Reads an entry's bytes in order, never past the size it was given, so a
/// declared length can be checked before anything is allocated for it.
pub(super) struct Reader<R> {
    pub(super) input: R,
    pub(super) remaining: u64,
}

impl<R: Read> Reader<R> {
    pub(super) fn bytes<const N: usize>(&mut self) -> Result<[u8; N], String> {
        let mut buffer = [0; N];
        self.claim(N as u64, 1)?;
        self.input.read_exact(&mut buffer).map_err(err)?;
        Ok(buffer)
    }
    pub(super) fn word(&mut self) -> Result<u64, String> {
        self.bytes::<8>().map(u64::from_le_bytes)
    }
    /// Takes `count * width` bytes from the remaining budget.
    fn claim(&mut self, count: u64, width: u64) -> Result<(), String> {
        let bytes = count
            .checked_mul(width)
            .filter(|&bytes| bytes <= self.remaining)
            .ok_or("a length exceeds the entry")?;
        self.remaining -= bytes;
        Ok(())
    }
    fn vector<T, const N: usize>(&mut self, decode: fn([u8; N]) -> T) -> Result<Vec<T>, String> {
        let count = self.word()?;
        self.claim(count, N as u64)?;
        let count = usize::try_from(count).map_err(err)?;
        let mut values = Vec::new();
        values.try_reserve_exact(count).map_err(err)?;
        let mut buffer = [0; N];
        for _ in 0..count {
            self.input.read_exact(&mut buffer).map_err(err)?;
            values.push(decode(buffer));
        }
        Ok(values)
    }
    fn floats(&mut self) -> Result<Vec<f32>, String> {
        self.vector(|b: [u8; 4]| f32::from_bits(u32::from_le_bytes(b)))
    }
    fn integers(&mut self) -> Result<Vec<i64>, String> {
        self.vector(i64::from_le_bytes)
    }
    pub(super) fn u32s(&mut self) -> Result<Vec<u32>, String> {
        self.vector(u32::from_le_bytes)
    }
    pub(super) fn string(&mut self) -> Result<String, String> {
        String::from_utf8(self.vector(|b: [u8; 1]| b[0])?).map_err(err)
    }
    /// One tensor written by `write_tensor_v1`.
    pub(super) fn tensor(&mut self) -> Result<NativeFlatDecisionTensorV2, String> {
        Ok(NativeFlatDecisionTensorV2 {
            state: self.floats()?,
            object_features: self.floats()?,
            object_card_ids: self.integers()?,
            object_groups: self.integers()?,
            object_node_ids: self.integers()?,
            edge_features: self.floats()?,
            edge_source_indices: self.integers()?,
            edge_target_indices: self.integers()?,
            action_features: self.floats()?,
            action_ref_features: self.floats()?,
            action_ref_card_ids: self.integers()?,
            action_ref_action_indices: self.integers()?,
            action_ref_node_indices: self.integers()?,
        })
    }
}

/// Decodes exactly `expected` tensors from an entry of `size` bytes.
fn decode_v1(
    input: impl Read,
    size: u64,
    expected: usize,
) -> Result<Vec<NativeFlatDecisionTensorV2>, String> {
    let mut reader = Reader {
        input,
        remaining: size,
    };
    if &reader.bytes::<8>()? != MAGIC {
        return Err("not a bank cache entry".into());
    }
    let count = reader.word()?;
    if count != expected as u64 {
        return Err("row count differs from the bank".into());
    }
    if count.saturating_mul(MIN_TENSOR_BYTES) > reader.remaining {
        return Err("the entry is too short for its row count".into());
    }
    let mut tensors = Vec::new();
    tensors.try_reserve_exact(expected).map_err(err)?;
    for _ in 0..expected {
        tensors.push(reader.tensor()?);
    }
    if reader.remaining != 0 {
        return Err("trailing bytes after the last tensor".into());
    }
    Ok(tensors)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tensor(seed: u32) -> NativeFlatDecisionTensorV2 {
        let f = |n: u32| -> Vec<f32> {
            (0..n)
                .map(|i| f32::from_bits(seed.wrapping_mul(2_654_435_761).wrapping_add(i)))
                .collect()
        };
        let g = |n: i64| -> Vec<i64> { (0..n).map(|i| i - i64::from(seed)).collect() };
        NativeFlatDecisionTensorV2 {
            state: f(3),
            object_features: f(5),
            object_card_ids: g(2),
            object_groups: g(2),
            object_node_ids: g(0),
            edge_features: f(0),
            edge_source_indices: g(1),
            edge_target_indices: g(1),
            action_features: f(4),
            action_ref_features: vec![f32::NAN, -0.0, f32::INFINITY],
            action_ref_card_ids: g(3),
            action_ref_action_indices: g(3),
            action_ref_node_indices: g(3),
        }
    }

    fn encoded(tensors: &[NativeFlatDecisionTensorV2]) -> Vec<u8> {
        let mut bytes = Vec::new();
        encode_v1(tensors.iter(), tensors.len(), &mut bytes).unwrap();
        bytes
    }

    fn decode(bytes: &[u8], expected: usize) -> Result<Vec<NativeFlatDecisionTensorV2>, String> {
        decode_v1(bytes, bytes.len() as u64, expected)
    }

    fn same_bits(a: &NativeFlatDecisionTensorV2, b: &NativeFlatDecisionTensorV2) -> bool {
        tensor_sha256_v1(a).unwrap() == tensor_sha256_v1(b).unwrap()
    }

    #[test]
    fn bank_cache_round_trips_tensor_bits() {
        let tensors: Vec<_> = (0..4).map(tensor).collect();
        let decoded = decode(&encoded(&tensors), tensors.len()).unwrap();
        assert_eq!(decoded.len(), tensors.len());
        assert!(tensors.iter().zip(&decoded).all(|(a, b)| same_bits(a, b)));
    }

    #[test]
    fn bank_cache_rejects_damaged_entries() {
        let tensors: Vec<_> = (0..2).map(tensor).collect();
        let bytes = encoded(&tensors);
        assert!(decode(&bytes, 3).is_err());
        assert!(decode(&bytes[..bytes.len() - 1], 2).is_err());
        let mut extra = bytes.clone();
        extra.push(0);
        assert!(decode(&extra, 2).is_err());
        let mut magic = bytes.clone();
        magic[0] ^= 1;
        assert!(decode(&magic, 2).is_err());
        let mut huge_vector = bytes.clone();
        huge_vector[16..24].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(decode(&huge_vector, 2).is_err());
        let mut huge_count = bytes[..16].to_vec();
        huge_count[8..16].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(decode(&huge_count, usize::MAX).is_err());
    }
}
