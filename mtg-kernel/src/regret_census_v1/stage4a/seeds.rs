//! Seed namespace `stage4a-v1` (RUNNER.md section 4): SHA-256 over a
//! length-delimited tuple, first eight digest bytes little-endian.
//!
//! Tuple: (namespace, model, root ID, purpose, arm, sample index, node path).
//! Every element is an 8-byte little-endian length followed by its bytes; the
//! sample index is its 8-byte little-endian value. `arm` is empty where the
//! seed is shared across arms (selection worlds and policy streams, and every
//! evaluation world), so arms see the same draws for the same index.

use sha2::{Digest, Sha256};

pub(crate) const NAMESPACE: &str = "stage4a-v1";

/// Seed purposes. Each is a distinct string in the tuple.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Purpose {
    Select,
    EvalWorld,
    EvalInner,
    Policy,
    Ties,
    #[allow(dead_code, reason = "listed by RUNNER.md; the analysis bootstraps with its own frozen seed")]
    Bootstrap,
}

impl Purpose {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Select => "select",
            Self::EvalWorld => "eval-world",
            Self::EvalInner => "eval-inner",
            Self::Policy => "policy",
            Self::Ties => "ties",
            Self::Bootstrap => "bootstrap",
        }
    }
}

fn put(hash: &mut Sha256, bytes: &[u8]) {
    hash.update((bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
}

/// The encoder: `seed(model, root, purpose, arm, index, path)`.
pub(crate) fn seed(
    model: &str,
    root: &str,
    purpose: Purpose,
    arm: &str,
    index: u64,
    path: &[u8],
) -> u64 {
    let mut hash = Sha256::new();
    put(&mut hash, NAMESPACE.as_bytes());
    put(&mut hash, model.as_bytes());
    put(&mut hash, root.as_bytes());
    put(&mut hash, purpose.label().as_bytes());
    put(&mut hash, arm.as_bytes());
    put(&mut hash, &index.to_le_bytes());
    put(&mut hash, path);
    let digest = hash.finalize();
    u64::from_le_bytes(digest[..8].try_into().expect("eight digest bytes"))
}

/// The per-root seed context: model and root ID are fixed for a root.
#[derive(Clone, Debug)]
pub(crate) struct RootSeeds {
    pub(crate) model: String,
    pub(crate) root: String,
}

impl RootSeeds {
    pub(crate) fn get(&self, purpose: Purpose, arm: &str, index: u64, path: &[u8]) -> u64 {
        seed(&self.model, &self.root, purpose, arm, index, path)
    }

    /// Two policy sampling seeds for one role (`reset_sampling_v1`).
    pub(crate) fn policy(&self, arm: &str, index: u64, role: &str) -> [u64; 2] {
        [
            self.get(Purpose::Policy, arm, index, format!("{role}/0").as_bytes()),
            self.get(Purpose::Policy, arm, index, format!("{role}/1").as_bytes()),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoder_is_length_delimited_and_purpose_separated() {
        let a = seed("r1", "x", Purpose::Select, "", 0, b"");
        assert_eq!(a, seed("r1", "x", Purpose::Select, "", 0, b""));
        // Moving bytes between elements changes the seed.
        assert_ne!(
            seed("r1", "ab", Purpose::Select, "", 0, b""),
            seed("r1a", "b", Purpose::Select, "", 0, b"")
        );
        let purposes = [
            Purpose::Select,
            Purpose::EvalWorld,
            Purpose::EvalInner,
            Purpose::Policy,
            Purpose::Ties,
            Purpose::Bootstrap,
        ];
        let mut all: Vec<u64> = purposes
            .iter()
            .map(|&p| seed("r1", "x", p, "", 0, b""))
            .collect();
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), purposes.len());
        assert_ne!(a, seed("r1", "x", Purpose::Select, "E", 0, b""));
        assert_ne!(a, seed("r1", "x", Purpose::Select, "", 1, b""));
        assert_ne!(a, seed("r2", "x", Purpose::Select, "", 0, b""));
    }

    #[test]
    fn encoder_matches_its_documented_byte_layout() {
        // Recompute by hand: namespace, model, root, purpose, arm, index, path.
        let mut h = Sha256::new();
        for part in [&b"stage4a-v1"[..], b"r2", b"root-7", b"eval-world", b""] {
            h.update((part.len() as u64).to_le_bytes());
            h.update(part);
        }
        h.update(8u64.to_le_bytes());
        h.update(5u64.to_le_bytes());
        h.update(0u64.to_le_bytes());
        let d = h.finalize();
        let want = u64::from_le_bytes(d[..8].try_into().unwrap());
        assert_eq!(seed("r2", "root-7", Purpose::EvalWorld, "", 5, b""), want);
    }
}
