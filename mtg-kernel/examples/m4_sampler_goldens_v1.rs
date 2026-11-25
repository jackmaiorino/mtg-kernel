use mtg_kernel::m4_sampler_v1::{sample_v1, GRID_SIZE};

#[path = "../src/m4_sampler_v1/golden_cases.rs"]
mod golden_cases;
#[path = "../src/deterministic_math_v1_ln_golden_inputs.rs"]
mod ln_golden_inputs;

fn main() {
    for case in golden_cases::cases() {
        println!("({:?}, {:?}),", case.name, case.record());
    }
    println!("// ln_f64_v1 expected bits, in test order");
    for x in ln_golden_inputs::INPUTS {
        let bits = mtg_kernel::m4_sampler_v1::ln_f64_v1_for_goldens(x).to_bits();
        println!("\"{bits:016x}\", // x={x:?}");
    }
}
