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
//! unreadable or mismatched entry falls back to full extraction. The cache
//! changes cost, never output: nothing it does is recorded in any result.

use super::*;
use std::io::Write as _;

const SCHEMA: &str = "mtg-kernel/archive-bank-tensor-cache/v1";
const MAGIC: &[u8; 8] = b"MTGABC01";

/// The entry path for this binary and these pins, or `None` when the cache
/// is off.
pub(super) fn entry_path_v1(
    parent: &ExpandedModelSourceV1,
    lineage: &str,
    games: &PinnedFileV1,
    bank: &PinnedFileV1,
) -> Result<Option<PathBuf>, String> {
    let Some(directory) = std::env::var_os("MTG_KERNEL_ARCHIVE_BANK_CACHE") else {
        return Ok(None);
    };
    let key = json!({"schema":SCHEMA,"runtime_binary_sha256":runtime_binary_sha256()?,
        "parent":parent,"lineage":lineage,"games":games,"bank":bank});
    let name = sha(&serde_json::to_vec(&key).map_err(err)?);
    Ok(Some(PathBuf::from(directory).join(format!("{name}.bin"))))
}

/// The bank's rows with their cached tensors, or `None` (and a note on
/// stderr) when the entry is missing or does not match the bank exactly.
pub(super) fn load_v1(path: &Path, bank: &ArchiveBankV1) -> Option<ExtractedRowsV1> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => return miss(path, &e.to_string()),
    };
    let tensors = match decode_v1(&bytes) {
        Ok(tensors) => tensors,
        Err(reason) => return miss(path, &reason),
    };
    if tensors.len() != bank.rows.len() {
        return miss(path, "row count differs from the bank");
    }
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
    let mut bytes = Vec::new();
    encode_v1(
        rows.iter().map(|(_, tensor)| tensor),
        rows.len(),
        &mut bytes,
    );
    let partial = path.with_extension(format!("partial-{}", std::process::id()));
    let written = path
        .parent()
        .map_or(Ok(()), fs::create_dir_all)
        .and_then(|()| {
            let mut file = fs::File::create_new(&partial)?;
            file.write_all(&bytes)?;
            file.sync_all()
        })
        .and_then(|()| match fs::hard_link(&partial, path) {
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
            other => other,
        });
    let _ = fs::remove_file(&partial);
    if let Err(e) = written {
        eprintln!("archive bank cache: not stored at {}: {e}", path.display());
    }
}

fn encode_v1<'a>(
    tensors: impl Iterator<Item = &'a NativeFlatDecisionTensorV2>,
    count: usize,
    out: &mut Vec<u8>,
) {
    fn floats(out: &mut Vec<u8>, values: &[f32]) {
        out.extend_from_slice(&(values.len() as u64).to_le_bytes());
        for value in values {
            out.extend_from_slice(&value.to_bits().to_le_bytes());
        }
    }
    fn integers(out: &mut Vec<u8>, values: &[i64]) {
        out.extend_from_slice(&(values.len() as u64).to_le_bytes());
        for value in values {
            out.extend_from_slice(&value.to_le_bytes());
        }
    }
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(count as u64).to_le_bytes());
    for t in tensors {
        floats(out, &t.state);
        floats(out, &t.object_features);
        integers(out, &t.object_card_ids);
        integers(out, &t.object_groups);
        integers(out, &t.object_node_ids);
        floats(out, &t.edge_features);
        integers(out, &t.edge_source_indices);
        integers(out, &t.edge_target_indices);
        floats(out, &t.action_features);
        floats(out, &t.action_ref_features);
        integers(out, &t.action_ref_card_ids);
        integers(out, &t.action_ref_action_indices);
        integers(out, &t.action_ref_node_indices);
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        if self.bytes.len() < n {
            return Err("truncated entry".into());
        }
        let (head, rest) = self.bytes.split_at(n);
        self.bytes = rest;
        Ok(head)
    }
    fn word(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(
            self.take(8)?.try_into().expect("eight bytes"),
        ))
    }
    fn length(&mut self, width: usize) -> Result<usize, String> {
        let n = usize::try_from(self.word()?).map_err(err)?;
        if n.checked_mul(width)
            .is_none_or(|bytes| bytes > self.bytes.len())
        {
            return Err("vector length exceeds the entry".into());
        }
        Ok(n)
    }
    fn floats(&mut self) -> Result<Vec<f32>, String> {
        let n = self.length(4)?;
        Ok(self
            .take(n * 4)?
            .chunks_exact(4)
            .map(|c| f32::from_bits(u32::from_le_bytes(c.try_into().expect("four bytes"))))
            .collect())
    }
    fn integers(&mut self) -> Result<Vec<i64>, String> {
        let n = self.length(8)?;
        Ok(self
            .take(n * 8)?
            .chunks_exact(8)
            .map(|c| i64::from_le_bytes(c.try_into().expect("eight bytes")))
            .collect())
    }
}

fn decode_v1(bytes: &[u8]) -> Result<Vec<NativeFlatDecisionTensorV2>, String> {
    let mut reader = Reader { bytes };
    if reader.take(MAGIC.len())? != MAGIC {
        return Err("not a bank cache entry".into());
    }
    let count = usize::try_from(reader.word()?).map_err(err)?;
    let mut tensors = Vec::with_capacity(count.min(1 << 20));
    for _ in 0..count {
        tensors.push(NativeFlatDecisionTensorV2 {
            state: reader.floats()?,
            object_features: reader.floats()?,
            object_card_ids: reader.integers()?,
            object_groups: reader.integers()?,
            object_node_ids: reader.integers()?,
            edge_features: reader.floats()?,
            edge_source_indices: reader.integers()?,
            edge_target_indices: reader.integers()?,
            action_features: reader.floats()?,
            action_ref_features: reader.floats()?,
            action_ref_card_ids: reader.integers()?,
            action_ref_action_indices: reader.integers()?,
            action_ref_node_indices: reader.integers()?,
        });
    }
    if !reader.bytes.is_empty() {
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

    fn same_bits(a: &NativeFlatDecisionTensorV2, b: &NativeFlatDecisionTensorV2) -> bool {
        tensor_sha256_v1(a).unwrap() == tensor_sha256_v1(b).unwrap()
    }

    #[test]
    fn bank_cache_round_trips_tensor_bits() {
        let tensors: Vec<_> = (0..4).map(tensor).collect();
        let mut bytes = Vec::new();
        encode_v1(tensors.iter(), tensors.len(), &mut bytes);
        let decoded = decode_v1(&bytes).unwrap();
        assert_eq!(decoded.len(), tensors.len());
        assert!(tensors.iter().zip(&decoded).all(|(a, b)| same_bits(a, b)));
    }

    #[test]
    fn bank_cache_rejects_damaged_entries() {
        let tensors: Vec<_> = (0..2).map(tensor).collect();
        let mut bytes = Vec::new();
        encode_v1(tensors.iter(), tensors.len(), &mut bytes);
        assert!(decode_v1(&bytes[..bytes.len() - 1]).is_err());
        let mut extra = bytes.clone();
        extra.push(0);
        assert!(decode_v1(&extra).is_err());
        let mut magic = bytes.clone();
        magic[0] ^= 1;
        assert!(decode_v1(&magic).is_err());
        let mut huge = bytes;
        huge[16..24].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(decode_v1(&huge).is_err());
    }
}
