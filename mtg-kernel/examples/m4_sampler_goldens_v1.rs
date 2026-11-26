use mtg_kernel::m4_sampler_v1::{sample_v1, GRID_SIZE};

// The production exp is crate-private. Compile its pinned source into the
// example, while counts, draws and ln come from the linked library.
#[allow(dead_code, unused_imports)]
#[path = "../src/deterministic_math_v1.rs"]
mod deterministic_math_v1;
mod line_b_teacher_target_v1 {
    pub(crate) use mtg_kernel::m4_sampler_v1::ln_f64_v1_for_goldens as ln_v1;
}

#[path = "../src/m4_sampler_v1/golden_cases.rs"]
mod golden_cases;
#[path = "../src/deterministic_math_v1_ln_golden_inputs.rs"]
mod ln_golden_inputs;

fn main() {
    use deterministic_math_v1::{exp_f64_v1, EXP_DOMAIN_FLOOR_V1};

    for case in golden_cases::cases() {
        let mut enabled = case
            .logits
            .iter()
            .zip(&case.mask)
            .filter_map(|(&logit, &on)| on.then_some(f64::from(logit)));
        let max = enabled.next().unwrap();
        let max = enabled.fold(max, f64::max);
        let weights: Vec<_> = case
            .logits
            .iter()
            .zip(&case.mask)
            .map(|(&logit, &on)| {
                if on {
                    exp_f64_v1((f64::from(logit) - max).max(EXP_DOMAIN_FLOOR_V1))
                } else {
                    0.0
                }
            })
            .collect();
        println!("({:?}, {:?}),", case.name, case.record(&weights));
    }
    println!("// ln_f64_v1 expected bits, in test order");
    for x in ln_golden_inputs::INPUTS {
        let bits = mtg_kernel::m4_sampler_v1::ln_f64_v1_for_goldens(x).to_bits();
        println!("\"{bits:016x}\", // x={x:?}");
    }
}
