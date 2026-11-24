use mtg_kernel::m4_sampler_v1::{sample_v1, GRID_SIZE};

#[path = "../src/m4_sampler_v1/golden_cases.rs"]
mod golden_cases;

fn main() {
    for case in golden_cases::cases() {
        println!("({:?}, {:?}),", case.name, case.record());
    }
}
