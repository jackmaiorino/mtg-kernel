// Shared by the unit tests and the paste-ready golden generator.
use super::{sample_v1, GRID_SIZE};
use sha2::{Digest, Sha256};

pub(crate) struct Case {
    pub name: &'static str,
    pub logits: Vec<f32>,
    pub mask: Vec<bool>,
}

impl Case {
    fn all(name: &'static str, logits: Vec<f32>) -> Self {
        Self {
            name,
            mask: vec![true; logits.len()],
            logits,
        }
    }

    pub fn record(&self, weights: &[f64]) -> String {
        const K: [u64; 8] = [
            0,
            GRID_SIZE / 32,
            GRID_SIZE / 7,
            GRID_SIZE / 2 - 1,
            GRID_SIZE / 2,
            3 * GRID_SIZE / 4,
            GRID_SIZE - 2,
            GRID_SIZE - 1,
        ];
        let first = sample_v1(&self.logits, &self.mask, 0).unwrap();
        let mut hash = Sha256::new();
        for q in &first.counts {
            hash.update(q.to_le_bytes());
        }
        let selected = K.map(|k| {
            sample_v1(&self.logits, &self.mask, k << 11)
                .unwrap()
                .selected_index
        });
        // At a maximum, the loss returns 0 - ln(S). Recover the bits of
        // ln(S), including +0 for S = 1, from the public sampler output.
        let max_log_p = first
            .log_probabilities
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let ln_bits = (0.0 - max_log_p).to_bits();
        // Hash one binary64 per option in canonical order, including masked zeros.
        let mut weight_hash = Sha256::new();
        for weight in weights {
            weight_hash.update(weight.to_bits().to_le_bytes());
        }
        format!(
            "{:x} {selected:?} {ln_bits:016x} {:x}",
            hash.finalize(),
            weight_hash.finalize()
        )
    }
}

pub(crate) fn cases() -> Vec<Case> {
    let mut single = Case::all("one_unmasked_of_45", vec![12.0; 45]);
    single.mask.fill(false);
    single.mask[23] = true;
    single.logits[23] = -3.0;

    let mut small_gap = vec![-81.0 / GRID_SIZE as f32; 32];
    small_gap[0] = 0.0;
    // M4 council, dispute D2, seat F's first turn: l_0 = 0,
    // l_1 = -0.71335 and l_{2..44} = -36.716 (inclusive).
    let mut seat_f = vec![-36.716; 45];
    seat_f[0] = 0.0;
    seat_f[1] = -0.71335;
    let mut quarters = vec![libm::log(0.25000000000000033) as f32; 6];
    quarters[0] = 0.0;
    vec![
        Case::all("two_logits", vec![-0.34, 0.0]),
        Case::all("three_equal", vec![0.75; 3]),
        Case::all("seven_equal", vec![0.75; 7]),
        Case::all("forty_five_equal", vec![-9.0; 45]),
        Case::all(
            "near_ties",
            vec![1.0, 1.0_f32.next_up(), 1.0_f32.next_down(), 1.0],
        ),
        single,
        Case::all(
            "large_and_small",
            vec![f32::MIN, 0.0, f32::from_bits(1), f32::MAX, -120.0],
        ),
        Case::all("thirty_two_small_gaps", small_gap),
        Case::all("seat_f_forty_five", seat_f),
        Case::all("one_and_five_quarters", quarters),
        Case::all(
            "exp_floor",
            vec![
                0.0,
                (-120.0_f32).next_up(),
                -120.0,
                (-120.0_f32).next_down(),
            ],
        ),
        Case::all("note_10a_equal", vec![0.75, 0.75]),
        Case::all("note_10a_unequal", vec![0.75, 1.75]),
    ]
}
