use super::*;
use crate::native_policy_train_step_v1::{
    compute_live_train_state_sha256_dims_v1, compute_owned_train_snapshot_sha256_v1,
    NativePolicyValueTrainStateV1,
};
use crate::native_policy_value_net_v1::{NativePolicyValueModelConfigV1, NativePolicyValueNetV1};
use std::sync::atomic::Ordering;

/// A small digest stream with a stand-in digest: SHA-256 of a plain encoding
/// of every item, so any changed item changes it.
#[derive(Clone)]
struct FakeV1 {
    adam_step: u64,
    anchor: u32,
    sections: Vec<(usize, Vec<(&'static str, Vec<usize>, Vec<f32>)>)>,
}

impl DigestStreamV1 for FakeV1 {
    fn adam_step_v1(&self) -> u64 {
        self.adam_step
    }

    fn scorer_bias_anchor_bits_v1(&self) -> u32 {
        self.anchor
    }

    fn visit_stream_v1(&self, visitor: &mut dyn FnMut(StreamItemV1<'_>)) {
        for (count, tensors) in &self.sections {
            visitor(StreamItemV1::Section { count: *count });
            for (name, shape, values) in tensors {
                visitor(StreamItemV1::Tensor {
                    name,
                    shape,
                    values,
                });
            }
        }
    }
}

fn fake_digest_v1(fake: &FakeV1) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(fake.adam_step.to_le_bytes());
    hasher.update(fake.anchor.to_le_bytes());
    for (count, tensors) in &fake.sections {
        hasher.update((*count as u64).to_le_bytes());
        for (name, shape, values) in tensors {
            hasher.update(name.as_bytes());
            for dimension in shape {
                hasher.update((*dimension as u64).to_le_bytes());
            }
            for value in values {
                hasher.update(value.to_bits().to_le_bytes());
            }
        }
    }
    hasher.finalize().into()
}

fn fake_v1() -> FakeV1 {
    FakeV1 {
        adam_step: 7,
        anchor: 1.5f32.to_bits(),
        sections: vec![
            (
                2,
                vec![
                    ("a.weight", vec![2, 3], vec![0.0, 1.0, -2.0, 3.5, 4.0, 5.0]),
                    ("a.bias", vec![3], vec![0.25, 0.0, -0.0]),
                ],
            ),
            (2, vec![("a.weight", vec![2, 3], vec![1.0; 6])]),
        ],
    }
}

fn digest_counted_v1(memo: &MemoV1, fake: &FakeV1) -> [u8; 32] {
    memo.digest_v1(fake, || fake_digest_v1(fake))
}

#[test]
fn a_held_stream_returns_its_digest_without_hashing() {
    let memo = MemoV1::new_v1(2);
    let fake = fake_v1();
    let first = digest_counted_v1(&memo, &fake);
    assert_eq!(first, fake_digest_v1(&fake));
    assert_eq!(memo.computed.load(Ordering::Relaxed), 1);
    let again = digest_counted_v1(&memo, &fake.clone());
    assert_eq!(again, first);
    assert_eq!(memo.computed.load(Ordering::Relaxed), 1);
}

#[test]
fn any_changed_item_is_hashed_again() {
    let base = fake_v1();
    let mut changes: Vec<FakeV1> = Vec::new();
    let mut fake = base.clone();
    fake.adam_step += 1;
    changes.push(fake);
    let mut fake = base.clone();
    fake.anchor ^= 1;
    changes.push(fake);
    let mut fake = base.clone();
    fake.sections[0].0 = 3;
    changes.push(fake);
    let mut fake = base.clone();
    fake.sections[0].1[1].0 = "b.bias";
    changes.push(fake);
    let mut fake = base.clone();
    fake.sections[0].1[0].1 = vec![3, 2];
    changes.push(fake);
    // A signed zero differs only in its bits.
    let mut fake = base.clone();
    fake.sections[0].1[1].2[2] = 0.0;
    changes.push(fake);
    // One value in the last tensor of the last section.
    let mut fake = base.clone();
    fake.sections[1].1[0].2[5] = 2.0;
    changes.push(fake);
    // A missing tensor, an extra tensor and an extra section.
    let mut fake = base.clone();
    fake.sections[0].1.pop();
    changes.push(fake);
    let mut fake = base.clone();
    fake.sections[1].1.push(("a.bias", vec![3], vec![0.0; 3]));
    changes.push(fake);
    let mut fake = base.clone();
    fake.sections.push((0, Vec::new()));
    changes.push(fake);
    for (index, changed) in changes.iter().enumerate() {
        let memo = MemoV1::new_v1(2);
        digest_counted_v1(&memo, &base);
        let digest = digest_counted_v1(&memo, changed);
        assert_eq!(
            digest,
            fake_digest_v1(changed),
            "change {index} returned a held digest"
        );
        assert_eq!(memo.computed.load(Ordering::Relaxed), 2, "change {index}");
    }
}

#[test]
fn least_recently_used_streams_are_dropped_and_zero_entries_turn_it_off() {
    let first = fake_v1();
    let mut second = fake_v1();
    second.adam_step = 8;
    let mut third = fake_v1();
    third.adam_step = 9;
    let memo = MemoV1::new_v1(2);
    for fake in [&first, &second, &first, &third] {
        digest_counted_v1(&memo, fake);
    }
    // `first` was used after `second`, so `second` went.
    assert_eq!(memo.computed.load(Ordering::Relaxed), 3);
    digest_counted_v1(&memo, &first);
    digest_counted_v1(&memo, &third);
    assert_eq!(memo.computed.load(Ordering::Relaxed), 3);
    digest_counted_v1(&memo, &second);
    assert_eq!(memo.computed.load(Ordering::Relaxed), 4);

    let off = MemoV1::new_v1(0);
    digest_counted_v1(&off, &first);
    digest_counted_v1(&off, &first);
    assert_eq!(off.entries.lock().unwrap().len(), 0);
}

/// The live state and its owned snapshot write one stream, so the memo
/// serves either from the other, and every digest equals the unmemoized one.
#[test]
fn live_and_snapshot_digests_equal_the_unmemoized_hash() {
    let model =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .unwrap();
    let state = NativePolicyValueTrainStateV1::new_v1(model).unwrap();
    let snapshot = state.snapshot_v1().unwrap();
    let expected = compute_live_train_state_sha256_dims_v1(&state);
    assert_eq!(compute_owned_train_snapshot_sha256_v1(&snapshot), expected);
    let memo = MemoV1::new_v1(2);
    let live = memo.digest_v1(&state, || compute_live_train_state_sha256_dims_v1(&state));
    let owned = memo.digest_v1(&snapshot, || {
        compute_owned_train_snapshot_sha256_v1(&snapshot)
    });
    assert_eq!((live, owned), (expected, expected));
    assert_eq!(memo.computed.load(Ordering::Relaxed), 1);
    // The production path agrees.
    assert_eq!(state.state_sha256_v1().unwrap(), expected);
    assert_eq!(snapshot.state_sha256_v1().unwrap(), expected);

    // One moment bit changed: a new digest, equal to the unmemoized hash.
    let mut changed = snapshot.clone();
    let values = &mut changed.second_moments[3].values;
    values[0] = f32::from_bits(values[0].to_bits() ^ 1);
    let changed_state = NativePolicyValueTrainStateV1::from_snapshot_v1(
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .unwrap(),
        &changed,
    )
    .unwrap();
    let changed_expected = compute_live_train_state_sha256_dims_v1(&changed_state);
    assert_ne!(changed_expected, expected);
    assert_eq!(
        memo.digest_v1(&changed_state, || {
            compute_live_train_state_sha256_dims_v1(&changed_state)
        }),
        changed_expected
    );
    assert_eq!(changed_state.state_sha256_v1().unwrap(), changed_expected);
    assert_eq!(changed.state_sha256_v1().unwrap(), changed_expected);
}
