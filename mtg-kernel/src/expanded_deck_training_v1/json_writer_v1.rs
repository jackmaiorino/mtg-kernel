//! The writer behind `serialize_json_v1`: the bytes of `serde_json::to_vec`,
//! produced with less work per number.
//!
//! `serde_json::to_vec(value)` serializes through `Serializer::new` into a
//! `Vec<u8>`: the compact formatter, every method of which is the `Formatter`
//! trait's default, writing through `impl Write for Vec<u8>`. A trajectory is
//! almost all integer arrays (binary32 bit patterns and indices). In a
//! recorded c1 trajectory about three numbers in four are one digit and most
//! of the rest are ten digits; every one of them went through itoa and a
//! libc `memcpy` call.
//!
//! Two changes, neither of which changes a written byte:
//! - [`CompactFastIntegersV1`] is the compact formatter with its integer
//!   writers answering a value from 0 to 9 with that one ASCII digit, which
//!   is the text itoa gives it. Every other value goes to the compact
//!   formatter's own method (itoa), and every other method (floats, strings,
//!   escapes, punctuation) is the trait default the compact formatter uses.
//! - [`VecSinkV1`] appends to the vector exactly as `impl Write for Vec<u8>`
//!   does (`extend_from_slice`, so the same growth and capacity), with writes
//!   of 1 to 16 bytes copied at a fixed length instead of through a call to
//!   `memcpy`.
//!
//! The serializer's type changes, so a `Serialize` implementation that
//! looked at its serializer's type name could tell the two apart. None of
//! the documents written here does: their implementations are derived,
//! serde's own, or hand-written ones that only delegate to serializer
//! methods (for example the policy origin's, and the borrowed checkpoint
//! tensors' `collect_seq` over the values' bits in
//! `checkpoint_container_v1::serialize_bits_v1`).
//!
//! A trajectory, the hot document, has its own writer,
//! [`trajectory_into_v1`]: the same bytes again, with the integer arrays of
//! its decisions written without serde's per-number calls.
//!
//! `json_writer_v1/tests.rs` compares the result with `serde_json::to_vec`
//! (the previous writer, kept there as the reference): bytes, errors and the
//! buffer's final capacity.

use super::{DecisionRecordV1, ExpandedTrajectoryV1, TensorBitsV1};
use serde::Serialize;
use serde_json::ser::{CompactFormatter, Formatter};
use std::io;

/// `serde_json::to_writer(&mut bytes, value)` with [`CompactFastIntegersV1`]
/// and [`VecSinkV1`]: appends the bytes `serde_json::to_vec(value)` returns,
/// and fails with its error.
pub(super) fn to_vec_into_v1<T: ?Sized + Serialize>(
    bytes: &mut Vec<u8>,
    value: &T,
) -> serde_json::Result<()> {
    let mut serializer =
        serde_json::Serializer::with_formatter(VecSinkV1(bytes), CompactFastIntegersV1);
    value.serialize(&mut serializer)
}

/// `serde_json`'s compact formatter, with one-digit integers written
/// directly. See the module documentation.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct CompactFastIntegersV1;

macro_rules! one_digit_unsigned_v1 {
    ($($method:ident: $ty:ty),* $(,)?) => {$(
        #[inline]
        fn $method<W: ?Sized + io::Write>(&mut self, writer: &mut W, value: $ty) -> io::Result<()> {
            if value < 10 {
                writer.write_all(&[b'0' + value as u8])
            } else {
                CompactFormatter.$method(writer, value)
            }
        }
    )*};
}

macro_rules! one_digit_signed_v1 {
    ($($method:ident: $ty:ty),* $(,)?) => {$(
        #[inline]
        fn $method<W: ?Sized + io::Write>(&mut self, writer: &mut W, value: $ty) -> io::Result<()> {
            if (0..10).contains(&value) {
                writer.write_all(&[b'0' + value as u8])
            } else {
                CompactFormatter.$method(writer, value)
            }
        }
    )*};
}

impl Formatter for CompactFastIntegersV1 {
    one_digit_unsigned_v1!(write_u8: u8, write_u16: u16, write_u32: u32, write_u64: u64);
    one_digit_signed_v1!(write_i8: i8, write_i16: i16, write_i32: i32, write_i64: i64);
}

/// Appends to a `Vec<u8>` with the result of `impl Write for Vec<u8>`.
pub(super) struct VecSinkV1<'a>(pub(super) &'a mut Vec<u8>);

impl io::Write for VecSinkV1<'_> {
    #[inline]
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        append_v1(self.0, bytes);
        Ok(bytes.len())
    }

    #[inline]
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        append_v1(self.0, bytes);
        Ok(())
    }

    /// `Vec<u8>`'s own (serde_json does not call it).
    fn write_vectored(&mut self, bytes: &[io::IoSlice<'_>]) -> io::Result<usize> {
        io::Write::write_vectored(&mut *self.0, bytes)
    }

    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// `out.extend_from_slice(bytes)`. For 1 to 16 bytes the length is a
/// constant in each arm, so the copy is a few moves rather than a call; the
/// reservation, and so the capacity, is `extend_from_slice`'s in every arm.
#[inline(always)]
fn append_v1(out: &mut Vec<u8>, bytes: &[u8]) {
    macro_rules! fixed_lengths_v1 {
        ($($n:literal)*) => {
            match bytes.len() {
                $($n => {
                    let fixed: &[u8; $n] = bytes.try_into().expect("the arm's length");
                    out.extend_from_slice(fixed)
                })*
                _ => out.extend_from_slice(bytes),
            }
        };
    }
    fixed_lengths_v1!(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16)
}

/// `to_vec_into_v1(bytes, trajectory)`: the same bytes, the same error,
/// and the same final capacity from any starting length and capacity.
///
/// The derived serialization of a trajectory goes through serde for every
/// number of its tensors. This writes the three hot structures (the
/// trajectory, its decisions and their tensors) itself, in the order and
/// with the presence rules of their derived implementations, and every
/// other member through [`to_vec_into_v1`] on the member's own value, which
/// is what the derived implementation serializes there (serde_json's compact
/// output of a value does not depend on where it is nested).
///
/// Every piece is appended as the same `write_all` call serde_json makes
/// (`{`, `,`, `"`, the key, `"`, `:`, a number, `[`, `]`), so the vector
/// grows exactly as it did. An integer array's values are formatted on the
/// stack and appended a segment at a time only while the vector's spare
/// capacity holds the segment's longest possible text, so that neither
/// writer grows the vector; otherwise they are appended piece by piece.
pub(super) fn trajectory_into_v1(
    bytes: &mut Vec<u8>,
    trajectory: &ExpandedTrajectoryV1,
) -> serde_json::Result<()> {
    // No `..`: a new member fails to compile here until it is written.
    let ExpandedTrajectoryV1 {
        schema,
        feature_contract_digest,
        feature_encoding_digest,
        card_db_hash,
        source_import,
        behavior_state_sha256,
        seat_behaviors,
        episode,
        configuration_sha256,
        decisions,
        terminal,
        learner_sampler,
        training_math,
        input_contract,
        opening_record,
        search_played,
    } = trajectory;
    let out = bytes;
    append_v1(out, b"{");
    key_v1(out, true, "schema");
    to_vec_into_v1(out, schema)?;
    key_v1(out, false, "feature_contract_digest");
    to_vec_into_v1(out, feature_contract_digest)?;
    key_v1(out, false, "feature_encoding_digest");
    to_vec_into_v1(out, feature_encoding_digest)?;
    key_v1(out, false, "card_db_hash");
    to_vec_into_v1(out, card_db_hash)?;
    key_v1(out, false, "source_import");
    to_vec_into_v1(out, source_import)?;
    key_v1(out, false, "behavior_state_sha256");
    to_vec_into_v1(out, behavior_state_sha256)?;
    if seat_behaviors.is_some() {
        key_v1(out, false, "seat_behaviors");
        to_vec_into_v1(out, seat_behaviors)?;
    }
    key_v1(out, false, "episode");
    to_vec_into_v1(out, episode)?;
    key_v1(out, false, "configuration_sha256");
    to_vec_into_v1(out, configuration_sha256)?;
    key_v1(out, false, "decisions");
    append_v1(out, b"[");
    for (index, decision) in decisions.iter().enumerate() {
        if index > 0 {
            append_v1(out, b",");
        }
        decision_into_v1(out, decision)?;
    }
    append_v1(out, b"]");
    key_v1(out, false, "terminal");
    to_vec_into_v1(out, terminal)?;
    if learner_sampler.is_some() {
        key_v1(out, false, "learner_sampler");
        to_vec_into_v1(out, learner_sampler)?;
    }
    if !training_math.is_platform_libm() {
        key_v1(out, false, "training_math");
        to_vec_into_v1(out, training_math)?;
    }
    if input_contract.is_some() {
        key_v1(out, false, "input_contract");
        to_vec_into_v1(out, input_contract)?;
    }
    if opening_record.is_some() {
        key_v1(out, false, "opening_record");
        to_vec_into_v1(out, opening_record)?;
    }
    if search_played.is_some() {
        key_v1(out, false, "search_played");
        to_vec_into_v1(out, search_played)?;
    }
    append_v1(out, b"}");
    Ok(())
}

/// A `DecisionRecordV1` as its derived implementation writes it.
fn decision_into_v1(out: &mut Vec<u8>, decision: &DecisionRecordV1) -> serde_json::Result<()> {
    let DecisionRecordV1 {
        step,
        physical_decision_id,
        substep_index,
        substep_count,
        actor,
        selected,
        logits,
        value,
        tensor,
        sampler_identity,
        pass_offset_action,
    } = decision;
    append_v1(out, b"{");
    key_v1(out, true, "step");
    integer_v1(out, *step);
    key_v1(out, false, "physical_decision_id");
    integer_v1(out, *physical_decision_id);
    key_v1(out, false, "substep_index");
    integer_v1(out, *substep_index);
    key_v1(out, false, "substep_count");
    integer_v1(out, *substep_count);
    key_v1(out, false, "actor");
    integer_v1(out, *actor);
    key_v1(out, false, "selected");
    integer_v1(out, *selected);
    key_v1(out, false, "logits");
    integer_array_v1(out, logits);
    key_v1(out, false, "value");
    integer_v1(out, *value);
    key_v1(out, false, "tensor");
    tensor_into_v1(out, tensor);
    if sampler_identity.is_some() {
        key_v1(out, false, "sampler_identity");
        to_vec_into_v1(out, sampler_identity)?;
    }
    if pass_offset_action.is_some() {
        key_v1(out, false, "pass_offset_action");
        to_vec_into_v1(out, pass_offset_action)?;
    }
    append_v1(out, b"}");
    Ok(())
}

/// A `TensorBitsV1` as its derived implementation writes it. Nothing in it
/// can fail.
fn tensor_into_v1(out: &mut Vec<u8>, tensor: &TensorBitsV1) {
    let TensorBitsV1 {
        state,
        object_features,
        object_card_ids,
        object_groups,
        object_node_ids,
        edge_features,
        edge_source_indices,
        edge_target_indices,
        action_features,
        action_ref_features,
        action_ref_card_ids,
        action_ref_action_indices,
        action_ref_node_indices,
        pairing_node_indices,
    } = tensor;
    append_v1(out, b"{");
    key_v1(out, true, "state");
    integer_array_v1(out, state);
    key_v1(out, false, "object_features");
    integer_array_v1(out, object_features);
    key_v1(out, false, "object_card_ids");
    integer_array_v1(out, object_card_ids);
    key_v1(out, false, "object_groups");
    integer_array_v1(out, object_groups);
    key_v1(out, false, "object_node_ids");
    integer_array_v1(out, object_node_ids);
    key_v1(out, false, "edge_features");
    integer_array_v1(out, edge_features);
    key_v1(out, false, "edge_source_indices");
    integer_array_v1(out, edge_source_indices);
    key_v1(out, false, "edge_target_indices");
    integer_array_v1(out, edge_target_indices);
    key_v1(out, false, "action_features");
    integer_array_v1(out, action_features);
    key_v1(out, false, "action_ref_features");
    integer_array_v1(out, action_ref_features);
    key_v1(out, false, "action_ref_card_ids");
    integer_array_v1(out, action_ref_card_ids);
    key_v1(out, false, "action_ref_action_indices");
    integer_array_v1(out, action_ref_action_indices);
    key_v1(out, false, "action_ref_node_indices");
    integer_array_v1(out, action_ref_node_indices);
    if !pairing_node_indices.is_empty() {
        key_v1(out, false, "pairing_node_indices");
        integer_array_v1(out, pairing_node_indices);
    }
    append_v1(out, b"}");
}

/// A struct member's key as serde_json writes it: `,` unless first, then
/// the quoted name in three writes (the names here need no escape), then
/// `:`.
#[inline]
fn key_v1(out: &mut Vec<u8>, first: bool, name: &'static str) {
    if !first {
        append_v1(out, b",");
    }
    append_v1(out, b"\"");
    append_v1(out, name.as_bytes());
    append_v1(out, b"\"");
    append_v1(out, b":");
}

/// An integer type written as its decimal text (the text of itoa, which
/// serde_json uses): an optional `-` and the digits of its magnitude.
pub(super) trait DecimalV1: Copy {
    /// The longest text of a value: digits and sign.
    const MAX_TEXT: usize;
    /// Whether the value is negative, and its magnitude.
    fn sign_magnitude_v1(self) -> (bool, u64);
}

macro_rules! unsigned_decimal_v1 {
    ($($ty:ty: $max:literal),* $(,)?) => {$(
        impl DecimalV1 for $ty {
            const MAX_TEXT: usize = $max;
            #[inline(always)]
            fn sign_magnitude_v1(self) -> (bool, u64) {
                (false, u64::from(self))
            }
        }
    )*};
}

unsigned_decimal_v1!(u8: 3, u32: 10, u64: 20);

impl DecimalV1 for i64 {
    const MAX_TEXT: usize = 20;
    #[inline(always)]
    fn sign_magnitude_v1(self) -> (bool, u64) {
        (self < 0, self.unsigned_abs())
    }
}

/// The two-digit texts of 0 to 99.
const DIGIT_PAIRS_V1: &[u8; 200] = b"\
    0001020304050607080910111213141516171819\
    2021222324252627282930313233343536373839\
    4041424344454647484950515253545556575859\
    6061626364656667686970717273747576777879\
    8081828384858687888990919293949596979899";

/// The length of `value`'s decimal text.
#[inline(always)]
fn decimal_len_v1<T: DecimalV1>(value: T) -> usize {
    let (negative, magnitude) = value.sign_magnitude_v1();
    usize::from(negative) + magnitude.checked_ilog10().map_or(1, |log| log as usize + 1)
}

/// Writes `value`'s decimal text at the start of `out` and returns its
/// length.
#[inline(always)]
fn write_decimal_v1<T: DecimalV1>(out: &mut [u8; 20], value: T) -> usize {
    let (negative, magnitude) = value.sign_magnitude_v1();
    if !negative && magnitude < 10 {
        out[0] = b'0' + magnitude as u8;
        return 1;
    }
    let sign = usize::from(negative);
    // The sign; for a value that is not negative the first digit overwrites
    // it below.
    out[0] = b'-';
    if magnitude < 100 {
        // Ten to 99, or a negative of at most two digits.
        let pair = magnitude as usize * 2;
        if magnitude < 10 {
            out[1] = b'0' + magnitude as u8;
            return 2;
        }
        out[sign] = DIGIT_PAIRS_V1[pair];
        out[sign + 1] = DIGIT_PAIRS_V1[pair + 1];
        return sign + 2;
    }
    if (100_000_000..=u64::from(u32::MAX)).contains(&magnitude) {
        // Nine or ten digits, as most binary32 bit patterns are: the top one
        // or two digits, then eight in four pairs.
        let top = (magnitude / 100_000_000) as usize;
        let bottom = (magnitude % 100_000_000) as u32;
        let at = if top < 10 {
            out[sign] = b'0' + top as u8;
            sign + 1
        } else {
            out[sign] = DIGIT_PAIRS_V1[top * 2];
            out[sign + 1] = DIGIT_PAIRS_V1[top * 2 + 1];
            sign + 2
        };
        let high = bottom / 10_000;
        let low = bottom % 10_000;
        for (offset, pair) in [high / 100, high % 100, low / 100, low % 100]
            .into_iter()
            .enumerate()
        {
            let pair = pair as usize * 2;
            out[at + offset * 2] = DIGIT_PAIRS_V1[pair];
            out[at + offset * 2 + 1] = DIGIT_PAIRS_V1[pair + 1];
        }
        return at + 8;
    }
    let length = decimal_len_v1(value);
    let mut end = length;
    let mut rest = magnitude;
    // Above u32: peel pairs in u64 arithmetic until the rest fits in u32.
    while rest > u64::from(u32::MAX) {
        let pair = (rest % 100) as usize * 2;
        rest /= 100;
        out[end - 2] = DIGIT_PAIRS_V1[pair];
        out[end - 1] = DIGIT_PAIRS_V1[pair + 1];
        end -= 2;
    }
    let mut rest = rest as u32;
    while rest >= 10_000 {
        let quad = rest % 10_000;
        rest /= 10_000;
        let high = (quad / 100) as usize * 2;
        let low = (quad % 100) as usize * 2;
        out[end - 4] = DIGIT_PAIRS_V1[high];
        out[end - 3] = DIGIT_PAIRS_V1[high + 1];
        out[end - 2] = DIGIT_PAIRS_V1[low];
        out[end - 1] = DIGIT_PAIRS_V1[low + 1];
        end -= 4;
    }
    if rest >= 100 {
        let pair = (rest % 100) as usize * 2;
        rest /= 100;
        out[end - 2] = DIGIT_PAIRS_V1[pair];
        out[end - 1] = DIGIT_PAIRS_V1[pair + 1];
        end -= 2;
    }
    if rest >= 10 {
        let pair = rest as usize * 2;
        out[end - 2] = DIGIT_PAIRS_V1[pair];
        out[end - 1] = DIGIT_PAIRS_V1[pair + 1];
    } else {
        out[end - 1] = b'0' + rest as u8;
    }
    length
}

/// One integer, in the one write serde_json makes of it.
#[inline]
fn integer_v1<T: DecimalV1>(out: &mut Vec<u8>, value: T) {
    let mut text = [0u8; 20];
    let length = write_decimal_v1(&mut text, value);
    append_v1(out, &text[..length]);
}

/// An integer array as serde_json writes a `Vec` of it, with the vector's
/// resulting length, contents and capacity.
///
/// serde_json appends `[`, then each value's text preceded by `,` after the
/// first, then `]`, each as its own `extend_from_slice`. The values are taken
/// [`ARRAY_SEGMENT_V1`] at a time. When the spare capacity holds the longest
/// possible text of a segment, none of serde_json's appends for it grows the
/// vector, so the segment is formatted on the stack and appended at once.
/// Otherwise its pieces are appended one by one, growing the vector exactly
/// as serde_json did. Either way the vector is in serde_json's state after
/// every segment.
pub(super) fn integer_array_v1<T: DecimalV1>(out: &mut Vec<u8>, values: &[T]) {
    append_v1(out, b"[");
    let mut block = [0u8; ARRAY_SEGMENT_V1 * 21];
    for (segment_index, segment) in values.chunks(ARRAY_SEGMENT_V1).enumerate() {
        let first = segment_index == 0;
        if out.capacity() - out.len() >= segment.len() * (T::MAX_TEXT + 1) {
            let mut at = 0;
            for (index, value) in segment.iter().enumerate() {
                if !(first && index == 0) {
                    block[at] = b',';
                    at += 1;
                }
                let text: &mut [u8; 20] = (&mut block[at..at + 20]).try_into().expect("20 bytes");
                at += write_decimal_v1(text, *value);
            }
            out.extend_from_slice(&block[..at]);
        } else {
            for (index, value) in segment.iter().enumerate() {
                if !(first && index == 0) {
                    append_v1(out, b",");
                }
                integer_v1(out, *value);
            }
        }
    }
    append_v1(out, b"]");
}

/// Values formatted at a time by [`integer_array_v1`].
const ARRAY_SEGMENT_V1: usize = 256;

#[cfg(test)]
mod tests;
