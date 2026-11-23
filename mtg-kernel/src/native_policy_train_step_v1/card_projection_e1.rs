//! Folded card semantics. The only projection arithmetic lives in RulesPathwayV1.
use super::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardSemanticsConfigV1 {
    pub table_path: PathBuf,
    pub table_sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CardSemanticsSectionV1 {
    pub(crate) schema: String,
    pub(crate) binding: String,
    pub(crate) p_step: u64,
    pub(crate) weight: Vec<u32>,
    pub(crate) first_moments: Vec<u32>,
    pub(crate) second_moments: Vec<u32>,
}

#[derive(Clone, Debug)]
pub(crate) struct CardSemanticsV1 {
    pub(crate) pathway: RulesPathwayV1,
    binding: String,
    p_step: u64,
    #[cfg(test)]
    pub(crate) fail_fold_for_test: bool,
}

pub(super) fn unsupported_v1() -> NativePolicyTrainErrorV1 {
    NativePolicyTrainErrorV1::LineBAuxiliary {
        code: "card-semantics-requires-ordinary-net8-cpu-gae",
    }
}

#[derive(Deserialize)]
struct TableV1 {
    schema: String,
    kernel_carddb_hash: String,
    vector_width: usize,
    card_count: usize,
    cards: Vec<RowV1>,
}
#[derive(Deserialize)]
struct RowV1 {
    card_id: usize,
    vector: Vec<i32>,
}

impl CardSemanticsConfigV1 {
    pub(crate) fn load_v1(&self) -> Result<CardSemanticsV1, String> {
        use std::io::Read;
        const CAP: u64 = 64 * 1024 * 1024;
        let metadata = std::fs::metadata(&self.table_path).map_err(|e| e.to_string())?;
        if !metadata.is_file() || metadata.len() > CAP {
            return Err("card semantics table must be a regular file of at most 64 MiB".into());
        }
        let file = std::fs::File::open(&self.table_path).map_err(|e| e.to_string())?;
        if !file.metadata().map_err(|e| e.to_string())?.is_file() {
            return Err("card semantics table must be a regular file".into());
        }
        let mut bytes = Vec::new();
        file.take(CAP + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > CAP {
            return Err("card semantics table exceeds 64 MiB".into());
        }
        self.load_bytes_v1(&bytes)
    }

    fn load_bytes_v1(&self, bytes: &[u8]) -> Result<CardSemanticsV1, String> {
        let digest = format!("{:x}", Sha256::digest(bytes));
        if digest != self.table_sha256 {
            return Err("card semantics table sha256 differs".into());
        }
        let table: TableV1 = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        if table.schema != "card_semantics_table/v1"
            || table.kernel_carddb_hash != format!("{:016x}", crate::card_def::KERNEL_CARDDB_HASH)
        {
            return Err("card semantics schema or kernel_carddb_hash differs".into());
        }
        let width = table.vector_width;
        if width == 0
            || width > 65536
            || table.cards.is_empty()
            || table.card_count != table.cards.len()
        {
            return Err("invalid card semantics dimensions".into());
        }
        let cards = table
            .cards
            .iter()
            .map(|r| r.card_id)
            .max()
            .unwrap()
            .checked_add(1)
            .filter(|&n| n < CARD_VOCAB_SIZE_V1)
            .ok_or("card semantics card id outside vocabulary")?;
        let mut maxima = vec![1u32; width];
        let mut seen = std::collections::BTreeSet::new();
        for row in &table.cards {
            if row.vector.len() != width || !seen.insert(row.card_id) {
                return Err("card semantics row width or duplicate card id".into());
            }
            for (max, &v) in maxima.iter_mut().zip(&row.vector) {
                *max = (*max).max(v.unsigned_abs());
            }
        }
        let divisors: Vec<f32> = maxima
            .into_iter()
            .map(|v| u64::from(v).next_power_of_two() as f32)
            .collect();
        let mut rows = vec![0.0f32; cards * width];
        for row in table.cards {
            for (j, v) in row.vector.into_iter().enumerate() {
                // All table integers must be exactly representable before scaling.
                if (v as f32) as f64 != f64::from(v) {
                    return Err("card semantics integer not exact in f32".into());
                }
                rows[row.card_id * width + j] = v as f32 / divisors[j];
            }
        }
        // Fixed encoding: ASCII lowercase table digest, dense f32 LE bits,
        // then cards and width as u64 LE. No platform-sized fields.
        let mut hash = Sha256::new();
        hash.update(digest.as_bytes());
        for v in &rows {
            hash.update(v.to_bits().to_le_bytes());
        }
        hash.update((cards as u64).to_le_bytes());
        hash.update((width as u64).to_le_bytes());
        Ok(CardSemanticsV1 {
            pathway: RulesPathwayV1::new_v1(rows, cards, width).map_err(|e| e.to_string())?,
            binding: format!("{:x}", hash.finalize()),
            p_step: 0,
            #[cfg(test)]
            fail_fold_for_test: false,
        })
    }
}

impl CardSemanticsV1 {
    pub(crate) fn binding_v1(&self) -> &str {
        &self.binding
    }
    pub(crate) fn p_step_v1(&self) -> u64 {
        self.p_step
    }
    pub(crate) fn section_v1(&self) -> CardSemanticsSectionV1 {
        let state = self.pathway.state_bits_v1();
        CardSemanticsSectionV1 {
            schema: "card_semantics_e1".into(),
            binding: self.binding.clone(),
            p_step: self.p_step,
            weight: state.weight,
            first_moments: state.first_moments,
            second_moments: state.second_moments,
        }
    }
    pub(crate) fn restore_v1(&mut self, saved: &CardSemanticsSectionV1) -> Result<(), String> {
        if saved.schema != "card_semantics_e1" || saved.binding != self.binding {
            return Err("card semantics checkpoint binding differs".into());
        }
        self.pathway
            .restore_v1(&RulesPathwayStateBitsV1 {
                weight: saved.weight.clone(),
                first_moments: saved.first_moments.clone(),
                second_moments: saved.second_moments.clone(),
            })
            .map_err(|e| e.to_string())?;
        self.p_step = saved.p_step;
        Ok(())
    }
    pub(crate) fn next_v1(
        &self,
        gradient: &[f32],
        learning_rate: f32,
        math: TrainingMathV1,
    ) -> Result<Self, NativePolicyTrainErrorV1> {
        let step = self
            .p_step
            .checked_add(1)
            .ok_or(NativePolicyTrainErrorV1::AdamStepOverflow)?;
        let candidate = self
            .pathway
            .adam_step_v1(gradient, step, learning_rate, math)?;
        let mut next = self.clone();
        next.pathway.commit_v1(candidate);
        next.p_step = step;
        Ok(next)
    }
    pub(crate) fn fold_difference_v1(
        &self,
        next: &Self,
        table: &mut [f32],
    ) -> Result<(), NativePolicyTrainErrorV1> {
        #[cfg(test)]
        if self.fail_fold_for_test {
            return Err(NativePolicyTrainErrorV1::LineBAuxiliary {
                code: "injected-post-adam-fold-failure",
            });
        }
        if table.len() != CARD_VOCAB_SIZE_V1 * CARD_EMBEDDING_DIM_V1 {
            return Err(unsupported_v1());
        }
        for token in 1..=self.pathway.cards_v1() {
            let old = self.pathway.delta_v1(token);
            let new = next.pathway.delta_v1(token);
            for k in 0..CARD_EMBEDDING_DIM_V1 {
                let difference = new[k] - old[k];
                if difference != 0.0 {
                    table[token * CARD_EMBEDDING_DIM_V1 + k] += difference;
                }
            }
        }
        validate_finite_slice("card_semantics_folded_table", table)
    }
}

/// The legacy tensor digest is unchanged when the optional section is absent.
pub(crate) fn checkpoint_hash_v1(
    tensors: &[u8; 32],
    section: Option<&CardSemanticsSectionV1>,
) -> Result<String, String> {
    match section {
        None => Ok(tensors.iter().map(|byte| format!("{byte:02x}")).collect()),
        Some(section) => {
            let mut hash = Sha256::new();
            hash.update(b"card_semantics_e1\0");
            hash.update(tensors);
            hash.update(serde_json::to_vec(section).map_err(|e| e.to_string())?);
            Ok(format!("{:x}", hash.finalize()))
        }
    }
}

#[cfg(test)]
impl CardSemanticsV1 {
    pub(crate) fn fixture_v1() -> Self {
        let cards = CARD_VOCAB_SIZE_V1 - 1;
        let mut rows = Vec::new();
        for token in 1..=cards {
            rows.extend([
                if token == cards {
                    0.0
                } else {
                    (token % 3 + 1) as f32 / 4.0
                },
                if token == cards {
                    0.0
                } else {
                    (token % 2) as f32
                },
            ]);
        }
        Self {
            pathway: RulesPathwayV1::new_v1(rows, cards, 2).unwrap(),
            binding: "test-binding".into(),
            p_step: 0,
            #[cfg(test)]
            fail_fold_for_test: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn table_bytes(hash: String) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({"schema":"card_semantics_table/v1", "kernel_carddb_hash":hash,
            "vector_width":2, "card_count":2, "cards":[{"card_id":0,"vector":[112,1]}, {"card_id":2,"vector":[-32,0]}]})).unwrap()
    }
    fn config(bytes: &[u8]) -> CardSemanticsConfigV1 {
        CardSemanticsConfigV1 {
            table_path: PathBuf::new(),
            table_sha256: format!("{:x}", Sha256::digest(bytes)),
        }
    }
    #[test]
    fn pins_sparse_rows_and_exact_column_scaling() {
        let bytes = table_bytes(format!("{:016x}", crate::card_def::KERNEL_CARDDB_HASH));
        let cfg = config(&bytes);
        let mut state = cfg.load_bytes_v1(&bytes).unwrap();
        assert_eq!(state.pathway.cards_v1(), 3);
        let mut weight = vec![0.0; 32];
        weight[0] = 1.0;
        weight[3] = 1.0;
        state.pathway.set_weight_v1(weight);
        assert_eq!(
            state.pathway.delta_v1(1)[0].to_bits(),
            (112.0f32 / 128.0).to_bits()
        );
        assert_eq!(state.pathway.delta_v1(1)[1].to_bits(), 1.0f32.to_bits());
        assert_eq!(state.pathway.delta_v1(2), [0.0; 16]);
        assert_eq!(state.pathway.delta_v1(3)[0], -0.25);
        let mut wrong_pin = cfg;
        wrong_pin.table_sha256 = "00".repeat(32);
        assert!(wrong_pin
            .load_bytes_v1(&bytes)
            .unwrap_err()
            .contains("sha256"));
        let wrong_db = table_bytes("0000000000000000".into());
        assert!(config(&wrong_db)
            .load_bytes_v1(&wrong_db)
            .unwrap_err()
            .contains("kernel_carddb_hash"));
    }
    #[test]
    fn rejects_invalid_table_schema_ids_dimensions_and_inexact_integers() {
        let bytes = table_bytes(format!("{:016x}", crate::card_def::KERNEL_CARDDB_HASH));
        let original: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        for (case, expected) in [
            (0, "schema"),
            (1, "duplicate"),
            (2, "vocabulary"),
            (3, "not exact"),
            (4, "dimensions"),
            (5, "row width"),
        ] {
            let mut value = original.clone();
            match case {
                0 => value["schema"] = "wrong".into(),
                1 => value["cards"][1]["card_id"] = 0.into(),
                2 => value["cards"][1]["card_id"] = (CARD_VOCAB_SIZE_V1 - 1).into(),
                3 => value["cards"][0]["vector"][0] = 16_777_217.into(),
                4 => value["card_count"] = 3.into(),
                5 => value["cards"][0]["vector"] = serde_json::json!([1]),
                _ => unreachable!(),
            }
            let bytes = serde_json::to_vec(&value).unwrap();
            assert!(
                config(&bytes)
                    .load_bytes_v1(&bytes)
                    .unwrap_err()
                    .contains(expected),
                "{case}"
            );
        }
    }

    #[test]
    fn table_file_must_be_regular_and_bounded() {
        let root = std::env::temp_dir().join(format!(
            "projection-size-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let mut cfg = CardSemanticsConfigV1 {
            table_path: root.clone(),
            table_sha256: String::new(),
        };
        assert!(cfg.load_v1().is_err());
        cfg.table_path = root.join("large.json");
        let file = std::fs::File::create(&cfg.table_path).unwrap();
        file.set_len(64 * 1024 * 1024 + 1).unwrap();
        assert!(cfg.load_v1().unwrap_err().contains("at most 64 MiB"));
        drop(file);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn zero_delta_difference_does_not_add_positive_zero() {
        let state = CardSemanticsV1::fixture_v1();
        let mut table = vec![-0.0; CARD_VOCAB_SIZE_V1 * 16];
        state.fold_difference_v1(&state, &mut table).unwrap();
        assert!(table.iter().all(|v| v.to_bits() == (-0.0f32).to_bits()));
    }
}
