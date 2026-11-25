//! Learned-seat call boundary for the shared collection and loss sampler.

pub const SAMPLER_ID: &str = "softmax-cdf-splitmix64-v2";

/// `true` enables an option. Index and f64 log probabilities use input order.
#[allow(unused_variables)]
pub fn draw(logits: &[f32], mask: &[bool], stream_value: u64) -> (usize, Vec<f64>) {
    // D2a supplies the shared masked softmax and integer-count draw.
    unimplemented!()
}
