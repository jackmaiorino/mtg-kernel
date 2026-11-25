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
//! `json_writer_v1/tests.rs` compares the result with `serde_json::to_vec`
//! (the previous writer, kept there as the reference).

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

#[cfg(test)]
mod tests;
