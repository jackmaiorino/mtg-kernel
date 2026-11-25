//! D6 drives its real retry loop with these 64 equiprobable cases.
use super::*;

pub const UNLIMITED_MEAN: f64 = -0.25;
pub const CAPPED_MEAN: f64 = -31. / 126.;
pub const DETERMINISTIC_HALT_MEAN: f64 = -1. / 6.;
pub const DROP_MEAN: f64 = -3. / 14.;

#[derive(Clone, Copy, Debug)]
pub struct HaltCase {
    pub actions: [usize; 2],
    /// Bit i supplies the independent halt coin for attempt i, starting at zero.
    pub pattern: u8,
    pub derivative: f64,
}
impl HaltCase {
    pub fn halts(&self, attempt: usize) -> bool {
        assert!(attempt < 4);
        self.actions == [1, 0] && self.pattern & (1 << attempt) != 0
    }
    pub fn deterministic_halt(&self) -> bool {
        self.actions == [1, 0]
    }
}

/// Only the pick scorer's final affine row is used. Its two active columns
/// receive 1 and the action index, so its logits are c and c + theta.
struct TwoAction {
    model: ModelV1,
    rows: ModelCardRowsV1,
}

impl TwoAction {
    const WEIGHT: T = T::PickScorer2Weight;

    fn new(theta: f32) -> Self {
        let mut model = ModelV1::initialise_v1(5, 1, 1, 0).unwrap();
        model.values[Self::WEIGHT.index()][..2].copy_from_slice(&[0.75, theta]);
        let rows = ModelCardRowsV1::new_v1(
            (0..5).collect(),
            1,
            vec![0.; 5],
            1,
            vec![0.; 5],
            [0, 1, 2, 3, 4],
        )
        .unwrap();
        Self { model, rows }
    }
    fn scores(tape: &mut Tape<'_>) -> [usize; 2] {
        [0., 1.].map(|a| {
            let mut input = vec![0.; H];
            input[..2].copy_from_slice(&[1., a]);
            let x = tape.constant(input);
            tape.linear(x, Self::WEIGHT, None, false)
        })
    }
    fn logits(&self) -> [f32; 2] {
        let mut tape = Tape::new(&self.model, &self.rows);
        Self::scores(&mut tape).map(|id| tape.output(id)[0])
    }
    fn menu(&self) -> [SampleV1; 2] {
        let logits = self.logits();
        [0, u64::MAX].map(|word| sample_v1(&logits, &[true; 2], word).unwrap())
    }
    fn outcome(
        &self,
        samples: [&SampleV1; 2],
        returns: [f64; 2],
        baseline: f32,
        denom: usize,
    ) -> (f64, f64) {
        self.weighted_outcome(samples, returns.map(|r| pick_credit(r, baseline, denom).0))
    }
    fn weighted_outcome(&self, samples: [&SampleV1; 2], coefficients: [f64; 2]) -> (f64, f64) {
        let mut tape = Tape::new(&self.model, &self.rows);
        let mut loss = 0.;
        for (s, c) in samples.into_iter().zip(coefficients) {
            let nodes = Self::scores(&mut tape);
            let (term, grad) = policy_term(s, c);
            loss += term;
            for (id, d) in nodes.into_iter().zip(grad) {
                tape.seed(id, &[d]);
            }
        }
        // Only this row participates in the toy network.
        let mut grads = vec![Vec::new(); T::ALL.len()];
        grads[Self::WEIGHT.index()] = vec![0.; H];
        tape.backward(&mut grads);
        (loss, f64::from(grads[Self::WEIGHT.index()][1]))
    }
}
pub fn halt_cases() -> impl Iterator<Item = HaltCase> {
    let net = TwoAction::new(0.);
    let menu = net.menu();
    (0..64).map(move |i| {
        let actions = [i / 32, (i / 16) % 2];
        let [a, b] = actions;
        let r = a as f64 - b as f64;
        HaltCase {
            actions,
            pattern: (i % 16) as u8,
            derivative: net.outcome([&menu[a], &menu[b]], [r, -r], 0., 2).1,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enumerate(
        theta: f32,
        base: f32,
        n: usize,
        pay: impl Fn(usize, usize) -> [f64; 2],
    ) -> (f64, f64) {
        let net = TwoAction::new(theta);
        let menu = net.menu();
        let mut mean = 0.;
        let mut loss = 0.;
        for a in 0..2 {
            for b in 0..2 {
                let mass = (menu[0].counts[a] as f64 / GRID_SIZE as f64)
                    * (menu[0].counts[b] as f64 / GRID_SIZE as f64);
                let (l, g) = net.outcome([&menu[a], &menu[b]], pay(a, b), base, n);
                loss += mass * l;
                mean += mass * g;
            }
        }
        (loss, mean)
    }
    fn payoff(a: usize, b: usize) -> [f64; 2] {
        let r = a as f64 - b as f64;
        [r, -r]
    }
    fn close(g: f64, w: f64, tol: f64) {
        assert!((g - w).abs() <= tol, "{g} versus {w}");
    }
    #[test]
    fn additive_derivative_and_wrong_losses() {
        let m = TwoAction::new(0.).menu();
        let p = m4_exp(m[0].log_probabilities[1].clamp(-120., 0.));
        eprintln!("part 1 backward p bits = {:#018x}", p.to_bits());
        for baseline in [0., 0.5] {
            let (loss, g) = enumerate(0., baseline, 2, payoff);
            close(loss, f64::from(baseline) * 0.5_f64.ln(), 1e-12);
            if p == 0.5 {
                assert_eq!(g.to_bits(), (-0.25f64).to_bits());
            } else {
                close(g, -0.25, 0.25e-12);
            }
        }
        close(
            enumerate(0., 0., 2, |a, b| [payoff(a, b).iter().sum(); 2]).1,
            0.,
            1e-12,
        );
        close(enumerate(0., 0., 2, |a, b| payoff(b, a)).1, 0.25, 1e-12);
        close(enumerate(0., 0., 1, payoff).1, -0.5, 1e-12);
    }
    #[test]
    fn unequal_probabilities_and_declared_joint_draws() {
        let theta = 1.;
        let net = TwoAction::new(theta);
        let m = net.menu();
        let p: Vec<_> = m[0]
            .log_probabilities
            .iter()
            .map(|x| m4_exp(x.clamp(-120., 0.)))
            .collect();
        let target = -p[0] * p[1];
        let enumerated = enumerate(theta, 0., 2, payoff).1;
        close(enumerated, target, 1e-6 * target.abs());
        let mut streams = [
            SplitMix64::seed(0x4d34_4434_0000),
            SplitMix64::seed(0x4d34_4434_0001),
        ];
        let mut sum = 0.;
        let mut sum_sq = 0.;
        let mut ones = 0;
        let n = 1 << 16;
        let logits = net.logits();
        for _ in 0..n {
            let [a, b] = streams
                .each_mut()
                .map(|s| sample_v1(&logits, &[true; 2], s.next_u64()).unwrap());
            let g = net
                .outcome([&a, &b], payoff(a.selected_index, b.selected_index), 0., 2)
                .1;
            sum += g;
            sum_sq += g * g;
            ones += a.selected_index + b.selected_index;
        }
        let n = f64::from(n);
        let mean = sum / n;
        eprintln!("part 2 target {target:.12}, enumerated {enumerated:.12}, sampled {mean:.12}, action 1 share {:.12}", ones as f64/(2.*n));
        close(
            mean,
            target,
            4. * ((sum_sq / n - mean * mean) / (n - 1.)).sqrt(),
        );
        close(
            ones as f64 / (2. * n),
            p[1],
            4. * (p[1] * (1. - p[1]) / (2. * n)).sqrt(),
        );
    }
    #[test]
    fn interaction_and_sibling_baselines() {
        let theta = 0.625;
        let net = TwoAction::new(theta);
        let m = net.menu();
        let q = m[0].counts[1] as f64 / GRID_SIZE as f64;
        // R0 = a - b + ab. The two focal derivatives of ab cancel in the seat mean.
        for baseline in [0., 0.5] {
            close(
                enumerate(theta, baseline, 2, |a, b| {
                    let r = payoff(a, b)[0] + (a * b) as f64;
                    [r, -r]
                })
                .1,
                -q * (1. - q),
                1e-6,
            );
        }
        // Independent builds A,B in both chairs. Each action receives its
        // own build result minus the other sibling's result.
        let mut mean = 0.;
        for a in 0..2 {
            for b in 0..2 {
                for c in 0..2 {
                    for d in 0..2 {
                        let reference = payoff(a, b);
                        let chair0 = sibling_advantages(reference[0], payoff(c, b)[0])
                            .map(|adv| build_coefficient(adv, 2));
                        let chair1 = sibling_advantages(reference[1], payoff(a, d)[1])
                            .map(|adv| build_coefficient(adv, 2));
                        let prob = [a, b, c, d]
                            .map(|k| m[0].counts[k] as f64 / GRID_SIZE as f64)
                            .iter()
                            .product::<f64>();
                        mean += prob
                            * (net.weighted_outcome([&m[a], &m[c]], chair0).1
                                + net.weighted_outcome([&m[b], &m[d]], chair1).1);
                    }
                }
            }
        }
        close(mean, -q * (1. - q), 1e-6);
    }
    #[test]
    fn pass_credit_tracks_the_focal_chair() {
        // Card 2 wins for seat 1 if passed. Seat 0 gets the opposite result.
        let logits = [0.75; 3];
        let counts = crate::m4_sampler_v1::counts_v1(&logits, &[true; 3]).unwrap();
        let mut start = 0;
        let mut correct = 0.;
        let mut swapped = 0.;
        for (a, &count) in counts.iter().enumerate() {
            let s = sample_v1(&logits, &[true; 3], start << 11).unwrap();
            start += count;
            let remaining: Vec<_> = (0..3).filter(|&card| card != a).collect();
            let other = if remaining.contains(&2) { 1. } else { -1. };
            let mass = count as f64 / GRID_SIZE as f64;
            for (result, sum) in [(-other, &mut correct), (other, &mut swapped)] {
                let (c, _, _) = pick_credit(result, 0., 2);
                *sum += mass
                    * f64::from(policy_gradient(&s.log_probabilities, &[(s.selected_index, c)])[2]);
            }
        }
        close(correct, -2. / 9., 1e-7);
        close(swapped, 2. / 9., 1e-7);
    }
    #[test]
    fn retry_enumeration_expected_values() {
        for (mode, expected, n) in [
            (0, UNLIMITED_MEAN, 64),
            (1, CAPPED_MEAN, 63),
            (2, DETERMINISTIC_HALT_MEAN, 48),
            (3, DROP_MEAN, 56),
        ] {
            let keep: Vec<_> = halt_cases()
                .filter(|c| match mode {
                    0 => true,
                    1 => (0..4).any(|i| !c.halts(i)),
                    2 => !c.deterministic_halt(),
                    _ => !c.halts(0),
                })
                .collect();
            assert_eq!(keep.len(), n);
            close(
                keep.iter().map(|c| c.derivative).sum::<f64>() / n as f64,
                expected,
                expected.abs() * 1e-6,
            );
        }
    }
}
