// Shared by the ln bit test and its paste-ready example.
const SEAM: u64 = std::f64::consts::SQRT_2.to_bits();
pub(super) const INPUTS: [f64; 8] = [
    1.0,
    f64::from_bits(SEAM - 1),
    f64::from_bits(SEAM),
    f64::from_bits(SEAM + 1),
    2.0,
    1.5,
    45.0,
    f64::from_bits(1.0_f64.to_bits() + 4), // 1 + 2^-50
];
