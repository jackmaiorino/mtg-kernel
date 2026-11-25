//! `json_writer_v1` against the previous writer, `serde_json::to_writer` into
//! a `Vec<u8>` (`reference_v1`, kept here verbatim), which is what
//! `serde_json::to_vec` does. Any differing byte fails, and so does a
//! different final capacity from the same starting capacity (the
//! parse-ahead cache charges a plain record's capacity).

use super::*;
use crate::expanded_deck_training_v1::no_state_digest_v1::InputContractRecordV1;
use crate::expanded_deck_training_v1::tests::{checkpoint_fixture_v1, replay_fixture};
use crate::expanded_deck_training_v1::{
    record_envelope_v1, serialize_and_seal_trajectory_v1, serialize_json_v1,
    serialize_trajectory_json_v1, ExpandedTrajectoryV1, PinnedFileV1, RecordEnvelopeV1,
    SearchPlayedRowV1, MAX_FILE_BYTES,
};
use crate::rl_session::pauper_london_v1::{
    PauperMulliganModeV1, PauperOpeningRecordV1, PauperOpeningSeatRecordV1, PauperOpeningV1,
};
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use crate::training_math_v1::TrainingMathV1;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::Write as _;

/// The previous `serialize_json_v1` writer, into a buffer of `capacity`.
fn reference_with_v1<T: ?Sized + Serialize>(value: &T, capacity: usize) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(capacity);
    serde_json::to_writer(&mut bytes, value).unwrap();
    bytes
}

fn reference_v1<T: ?Sized + Serialize>(value: &T) -> Vec<u8> {
    reference_with_v1(value, 128)
}

fn written_with_v1<T: ?Sized + Serialize>(value: &T, capacity: usize) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(capacity);
    to_vec_into_v1(&mut bytes, value).unwrap();
    bytes
}

fn written_v1<T: ?Sized + Serialize>(value: &T) -> Vec<u8> {
    written_with_v1(value, 128)
}

/// Equal final capacities of the two writers from the starting capacities
/// `serialize_json_v1` uses: 128 for a first document, and the last length
/// plus a sixteenth (also exactly the length, and one byte short of it).
fn assert_same_capacity_v1<T: ?Sized + Serialize>(value: &T) {
    let length = reference_v1(value).len();
    for start in [
        0,
        128,
        length.saturating_sub(1),
        length,
        length + length / 16,
    ] {
        let expected = reference_with_v1(value, start);
        let got = written_with_v1(value, start);
        assert!(got == expected, "bytes from capacity {start}");
        assert_eq!(got.capacity(), expected.capacity(), "capacity from {start}");
    }
}

fn first_difference_v1(a: &[u8], b: &[u8]) -> Option<usize> {
    a.iter()
        .zip(b)
        .position(|(x, y)| x != y)
        .or_else(|| (a.len() != b.len()).then_some(a.len().min(b.len())))
}

fn assert_same_bytes_v1<T: ?Sized + Serialize>(value: &T) {
    let expected = reference_v1(value);
    assert_eq!(expected, serde_json::to_vec(value).unwrap());
    let got = written_v1(value);
    assert!(
        got == expected,
        "the writers differ first at byte {:?}",
        first_difference_v1(&got, &expected)
    );
    assert_same_capacity_v1(value);
}

/// xorshift64*: a fixed stream of test values.
struct ValuesV1(u64);

impl ValuesV1 {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
}

/// The integers each width is checked over: every value of a width up to 16
/// bits; otherwise 0 to 100,000 (and their negatives), every power of ten and
/// its neighbours, the extremes, and 200,000 values of the fixed stream at
/// every digit length.
fn integer_cases_v1(bits: u32, signed: bool) -> Vec<i128> {
    let (min, max): (i128, i128) = if signed {
        (-(1i128 << (bits - 1)), (1i128 << (bits - 1)) - 1)
    } else {
        (0, (1i128 << bits) - 1)
    };
    if bits <= 16 {
        return (min..=max).collect();
    }
    let mut cases: Vec<i128> = (0..=100_000).collect();
    if signed {
        cases.extend((1..=100_000).map(|x| -x));
    }
    let mut power: i128 = 1;
    while power <= max {
        for x in [power - 1, power, power + 1] {
            cases.push(x);
            cases.push(-x);
        }
        power *= 10;
    }
    cases.extend([min, min + 1, max - 1, max]);
    let mut stream = ValuesV1(0x9e37_79b9_7f4a_7c15 ^ u64::from(bits));
    for k in 0..200_000u32 {
        let raw = i128::from(stream.next() >> (64 - bits));
        // Shift some values down so every digit length is drawn.
        let shifted = raw >> (k % bits);
        cases.push(if signed {
            shifted + min / (1 << (k % bits))
        } else {
            shifted
        });
    }
    cases.retain(|x| (min..=max).contains(x));
    cases
}

macro_rules! check_integer_writer_v1 {
    ($method:ident, $ty:ty, $bits:expr, $signed:expr) => {{
        let mut fast = Vec::new();
        let mut compact = Vec::new();
        for case in integer_cases_v1($bits, $signed) {
            let value = <$ty>::try_from(case).unwrap();
            fast.clear();
            compact.clear();
            CompactFastIntegersV1.$method(&mut fast, value).unwrap();
            CompactFormatter.$method(&mut compact, value).unwrap();
            assert!(
                fast == compact && fast == value.to_string().as_bytes(),
                "{}({value}): {:?} against {:?}",
                stringify!($method),
                String::from_utf8_lossy(&fast),
                String::from_utf8_lossy(&compact)
            );
        }
    }};
}

#[test]
fn integer_writers_give_the_compact_formatter_text_v1() {
    check_integer_writer_v1!(write_u8, u8, 8, false);
    check_integer_writer_v1!(write_u16, u16, 16, false);
    check_integer_writer_v1!(write_u32, u32, 32, false);
    check_integer_writer_v1!(write_u64, u64, 64, false);
    check_integer_writer_v1!(write_i8, i8, 8, true);
    check_integer_writer_v1!(write_i16, i16, 16, true);
    check_integer_writer_v1!(write_i32, i32, 32, true);
    check_integer_writer_v1!(write_i64, i64, 64, true);
}

#[test]
fn one_digit_integers_are_their_ascii_digit_v1() {
    for digit in 0..10u8 {
        let expected = [b'0' + digit];
        let mut out = Vec::new();
        CompactFastIntegersV1
            .write_u32(&mut out, u32::from(digit))
            .unwrap();
        CompactFastIntegersV1
            .write_i64(&mut out, i64::from(digit))
            .unwrap();
        assert_eq!(out, [expected, expected].concat());
        assert_same_bytes_v1(&(u32::from(digit), i64::from(digit), digit));
    }
    // Just outside the one-digit range on both sides.
    assert_eq!(written_v1(&(10u32, -1i64, 10i64, -10i8)), b"[10,-1,10,-10]");
}

/// `VecSinkV1` against `impl Write for Vec<u8>`: the same bytes and the same
/// capacity after every write, for every write length from 0 to 40 at every
/// fill level of vectors of several capacities, and over a long run of mixed
/// writes.
#[test]
fn vec_sink_appends_like_vec_write_v1() {
    let source: Vec<u8> = (0..64u8)
        .map(|i| i.wrapping_mul(37).wrapping_add(11))
        .collect();
    let start = |capacity: usize, fill: usize| {
        let mut v = Vec::with_capacity(capacity);
        v.extend_from_slice(&source[..fill]);
        v
    };
    for capacity in [0, 1, 7, 15, 16, 17, 31, 40, 64, 200] {
        for fill in 0..=capacity.min(40) {
            for n in 0..=40 {
                let mut expected = start(capacity, fill);
                expected.write_all(&source[..n]).unwrap();
                let mut sunk = start(capacity, fill);
                VecSinkV1(&mut sunk).write_all(&source[..n]).unwrap();
                assert_eq!(sunk, expected, "write_all {n} at {fill}/{capacity}");
                assert_eq!(sunk.capacity(), expected.capacity());
                let mut written = start(capacity, fill);
                assert_eq!(VecSinkV1(&mut written).write(&source[..n]).unwrap(), n);
                assert_eq!(written, expected, "write {n} at {fill}/{capacity}");
                assert_eq!(written.capacity(), expected.capacity());
            }
        }
    }
    let mut expected = Vec::new();
    let mut sunk = Vec::new();
    let mut stream = ValuesV1(17);
    for _ in 0..20_000 {
        let n = (stream.next() % 41) as usize;
        let at = (stream.next() % (64 - 40)) as usize;
        expected.write_all(&source[at..at + n]).unwrap();
        VecSinkV1(&mut sunk).write_all(&source[at..at + n]).unwrap();
        assert_eq!(sunk.len(), expected.len());
        assert_eq!(sunk.capacity(), expected.capacity());
    }
    assert_eq!(sunk, expected);
    VecSinkV1(&mut sunk).flush().unwrap();
    // `Vec`'s vectored write reserves the total once (17 here); separate
    // appends would grow 8 to 16 to 32.
    let slices = [
        io::IoSlice::new(&source[..9]),
        io::IoSlice::new(&[]),
        io::IoSlice::new(&source[20..28]),
    ];
    let mut expected = Vec::with_capacity(8);
    let mut sunk = Vec::with_capacity(8);
    assert_eq!(
        VecSinkV1(&mut sunk).write_vectored(&slices).unwrap(),
        expected.write_vectored(&slices).unwrap()
    );
    assert_eq!(sunk, expected);
    assert_eq!(sunk.capacity(), expected.capacity());
}

/// Strings needing every kind of escape: every ASCII character (all
/// controls, quote, backslash, DEL), multi-byte UTF-8 of every length, and
/// the line and paragraph separators (written raw by serde_json).
fn escape_cases_v1() -> Vec<String> {
    let ascii: String = (0u8..0x80).map(char::from).collect();
    vec![
        String::new(),
        ascii,
        "a\"b\\c/d\ne\rf\tg\u{8}h\u{c}i\u{0}j\u{1f}k\u{7f}".into(),
        "\u{80}\u{7ff}\u{800}\u{ffff}\u{10000}\u{10ffff}é漢😀".into(),
        "\u{2028}\u{2029}".into(),
        "0123456789".into(),
        "x".repeat(100),
    ]
}

fn f64_cases_v1() -> Vec<f64> {
    let mut cases = vec![
        0.0,
        -0.0,
        f64::from_bits(1),
        f64::MIN_POSITIVE,
        f64::MIN_POSITIVE / 2.0,
        -f64::MIN_POSITIVE / 3.0,
        1.0,
        -1.0,
        2.0,
        9.0,
        10.0,
        9_007_199_254_740_992.0,
        9_007_199_254_740_994.0,
        1e15,
        1e16,
        1e17,
        1e21,
        1e22,
        1e-5,
        1e-6,
        1e-7,
        0.1,
        1.0 / 3.0,
        -2.0 / 3.0,
        123_456_789.123_456_79,
        1.5e300,
        -2.5e-300,
        f64::MAX,
        f64::MIN,
        f64::EPSILON,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ];
    let mut stream = ValuesV1(0xf64);
    cases.extend((0..20_000).map(|_| f64::from_bits(stream.next())));
    cases
}

fn f32_cases_v1() -> Vec<f32> {
    let mut cases = vec![
        0.0,
        -0.0,
        f32::from_bits(1),
        f32::MIN_POSITIVE,
        f32::MIN_POSITIVE / 2.0,
        1.0,
        -1.0,
        9.0,
        16_777_216.0,
        16_777_218.0,
        0.1,
        1.0 / 3.0,
        1e-45,
        3.0e38,
        f32::MAX,
        f32::MIN,
        f32::EPSILON,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
    ];
    let mut stream = ValuesV1(0xf32);
    cases.extend((0..20_000).map(|_| f32::from_bits((stream.next() >> 32) as u32)));
    cases
}

/// `serialize_bytes`, which serde_json writes as an array of `write_u8`.
struct BytesV1(Vec<u8>);

impl Serialize for BytesV1 {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(&self.0)
    }
}

#[derive(Serialize)]
struct NewtypeV1(i64);

#[derive(Serialize)]
struct UnitV1;

#[derive(Serialize)]
enum KindV1 {
    Unit,
    Newtype(u32),
    Tuple(u8, i8),
    Struct { x: u64, y: Vec<i32> },
}

/// Every kind of value of serde's data model, with the cases above.
#[derive(Serialize)]
struct EveryKindV1 {
    unit: (),
    unit_struct: UnitV1,
    booleans: [bool; 2],
    u8s: Vec<u8>,
    u16s: Vec<u16>,
    u32s: Vec<u32>,
    u64s: Vec<u64>,
    u128s: Vec<u128>,
    i8s: Vec<i8>,
    i16s: Vec<i16>,
    i32s: Vec<i32>,
    i64s: Vec<i64>,
    i128s: Vec<i128>,
    f32s: Vec<f32>,
    f64s: Vec<f64>,
    chars: Vec<char>,
    strings: Vec<String>,
    bytes: BytesV1,
    none: Option<u32>,
    some: Option<u32>,
    empty_seq: Vec<u32>,
    empty_map: BTreeMap<String, u32>,
    u8_keys: BTreeMap<u8, u32>,
    u32_keys: BTreeMap<u32, i64>,
    u64_keys: BTreeMap<u64, String>,
    i8_keys: BTreeMap<i8, u8>,
    i64_keys: BTreeMap<i64, f64>,
    string_keys: BTreeMap<String, Vec<i64>>,
    nested: Vec<Vec<Vec<i64>>>,
    tuple: (u32, i64, f32, String, ()),
    newtype: NewtypeV1,
    variants: Vec<KindV1>,
}

fn every_kind_v1() -> EveryKindV1 {
    let ints = |bits, signed| integer_cases_v1(bits, signed).into_iter().step_by(7);
    let strings = escape_cases_v1();
    EveryKindV1 {
        unit: (),
        unit_struct: UnitV1,
        booleans: [false, true],
        u8s: ints(8, false).map(|x| x as u8).collect(),
        u16s: ints(16, false).map(|x| x as u16).collect(),
        u32s: ints(32, false).map(|x| x as u32).collect(),
        u64s: ints(64, false).map(|x| x as u64).collect(),
        u128s: vec![0, 9, 10, u128::from(u64::MAX) + 1, u128::MAX],
        i8s: ints(8, true).map(|x| x as i8).collect(),
        i16s: ints(16, true).map(|x| x as i16).collect(),
        i32s: ints(32, true).map(|x| x as i32).collect(),
        i64s: ints(64, true).map(|x| x as i64).collect(),
        i128s: vec![i128::MIN, -10, -1, 0, 9, 10, i128::MAX],
        f32s: f32_cases_v1(),
        f64s: f64_cases_v1(),
        chars: escape_cases_v1()
            .concat()
            .chars()
            .chain(['\0', '9', '"'])
            .collect(),
        strings: strings.clone(),
        bytes: BytesV1((0..=255).chain([0, 9, 10]).collect()),
        none: None,
        some: Some(7),
        empty_seq: Vec::new(),
        empty_map: BTreeMap::new(),
        u8_keys: (0..=255).map(|k| (k, u32::from(k) * 1_000)).collect(),
        u32_keys: [0, 1, 9, 10, 11, 99, 100, u32::MAX]
            .into_iter()
            .map(|k| (k, -i64::from(k)))
            .collect(),
        u64_keys: [0, 9, 10, u64::MAX]
            .into_iter()
            .zip(strings.iter().cloned())
            .collect(),
        i8_keys: (i8::MIN..=i8::MAX).map(|k| (k, k as u8)).collect(),
        i64_keys: [i64::MIN, -10, -9, -1, 0, 1, 9, 10, i64::MAX]
            .into_iter()
            .zip(f64_cases_v1())
            .collect(),
        string_keys: strings
            .iter()
            .enumerate()
            .map(|(i, s)| (s.clone(), vec![i as i64, -(i as i64)]))
            .collect(),
        nested: vec![vec![], vec![vec![]], vec![vec![0, -1, 10], vec![i64::MAX]]],
        tuple: (0, -9, -0.0, "\"".into(), ()),
        newtype: NewtypeV1(-5),
        variants: vec![
            KindV1::Unit,
            KindV1::Newtype(3),
            KindV1::Tuple(9, -9),
            KindV1::Struct {
                x: 10,
                y: vec![0, -2_147_483_648, 2_147_483_647],
            },
        ],
    }
}

#[test]
fn every_kind_of_value_has_the_previous_bytes_v1() {
    assert_same_bytes_v1(&every_kind_v1());
    let strings = escape_cases_v1();
    let value = json!({
        "": [],
        "empty_object": {},
        "numbers": [0, 9, 10, -1, -10, u64::MAX, i64::MIN, 0.0, -0.0, 1.5, 1e300, 5e-324],
        "strings": strings,
        "nested": [[[]], {"a": {"b": [null, true, false]}}],
    });
    assert_same_bytes_v1(&value);
    // A parsed document holds its numbers as u64, i64 or f64.
    let parsed: Value = serde_json::from_slice(&reference_v1(&every_kind_v1())).unwrap();
    assert_same_bytes_v1(&parsed);
}

#[test]
fn errors_are_the_previous_errors_v1() {
    let refused: BTreeMap<Vec<u8>, u32> = [(vec![1, 2], 3)].into_iter().collect();
    let previous = serde_json::to_vec(&refused).unwrap_err().to_string();
    let mut bytes = Vec::new();
    let now = to_vec_into_v1(&mut bytes, &refused)
        .unwrap_err()
        .to_string();
    assert_eq!(now, previous);
    let message = serialize_json_v1("json-writer-test-refused.json", &refused).unwrap_err();
    assert_eq!(message, previous);
    // Non-finite float keys are refused; a Serialize implementation's own
    // error is passed through.
    for key in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let refused = MapOfV1(vec![(FloatKeyV1(key), 1)]);
        let previous = serde_json::to_vec(&refused).unwrap_err().to_string();
        let now = to_vec_into_v1(&mut Vec::new(), &refused)
            .unwrap_err()
            .to_string();
        assert_eq!(now, previous);
    }
    let previous = serde_json::to_vec(&[FailsV1]).unwrap_err().to_string();
    let now = to_vec_into_v1(&mut Vec::new(), &[FailsV1])
        .unwrap_err()
        .to_string();
    assert_eq!(now, previous);
    assert!(now.contains("refused by its implementation"));
}

/// A map from float keys, which serde_json writes as quoted numbers.
struct MapOfV1(Vec<(FloatKeyV1, u32)>);

impl Serialize for MapOfV1 {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_map(self.0.iter().map(|(k, v)| (k, v)))
    }
}

struct FloatKeyV1(f64);

impl Serialize for FloatKeyV1 {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_f64(self.0)
    }
}

struct FailsV1;

impl Serialize for FailsV1 {
    fn serialize<S: serde::Serializer>(&self, _serializer: S) -> Result<S::Ok, S::Error> {
        Err(serde::ser::Error::custom("refused by its implementation"))
    }
}

/// `collect_str`, which serde_json writes through its own `fmt::Write`
/// adapter, escaping as it goes.
struct DisplayedV1(String);

impl Serialize for DisplayedV1 {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&format_args!("<{}|{}>", self.0, 10))
    }
}

#[test]
fn collected_strings_and_float_keys_have_the_previous_bytes_v1() {
    let displayed: Vec<DisplayedV1> = escape_cases_v1().into_iter().map(DisplayedV1).collect();
    assert_same_bytes_v1(&displayed);
    let keys = f64_cases_v1()
        .into_iter()
        .filter(|x| x.is_finite())
        .take(5_000)
        .map(|x| (FloatKeyV1(x), 9u32))
        .collect();
    assert_same_bytes_v1(&MapOfV1(keys));
}

/// `serialize_json_v1` and `serialize_trajectory_json_v1` against the
/// previous writer, on the first call for a name and on a second one, whose
/// buffer is presized from the first; and `trajectory_into_v1`'s bytes and
/// capacity against the previous writer's from many starting buffers.
fn assert_trajectory_bytes_v1(name: &str, trajectory: &ExpandedTrajectoryV1) {
    let expected = reference_v1(trajectory);
    for _ in 0..2 {
        let got = serialize_json_v1(name, trajectory).unwrap();
        assert!(
            got == expected,
            "{name}: the writers differ first at byte {:?}",
            first_difference_v1(&got, &expected)
        );
        let got = serialize_trajectory_json_v1(&format!("{name}.trajectory"), trajectory).unwrap();
        assert!(
            got == expected,
            "{name}: the trajectory writer differs first at byte {:?}",
            first_difference_v1(&got, &expected)
        );
    }
    assert_same_capacity_v1(trajectory);
    assert_trajectory_writer_capacity_v1(name, trajectory, &expected);
    let parsed: ExpandedTrajectoryV1 = serde_json::from_slice(&expected).unwrap();
    assert_eq!(&parsed, trajectory);
}

/// `trajectory_into_v1` from every starting capacity up to 300 bytes (up to
/// 40 and around 128 for a document over 64 KiB) and around the document's
/// length (one byte short, exact, and with the presizing headroom), each
/// also after a 7-byte prefix: the bytes and the final capacity are the
/// previous writer's from the same buffer.
fn assert_trajectory_writer_capacity_v1(
    name: &str,
    trajectory: &ExpandedTrajectoryV1,
    expected: &[u8],
) {
    let length = expected.len();
    let mut starts: Vec<usize> = if length <= 1 << 16 {
        (0..=300).collect()
    } else {
        (0..=40).chain(126..=130).collect()
    };
    starts.extend([
        length.saturating_sub(1),
        length,
        length + 1,
        length + length / 16,
    ]);
    for prefix in [&b""[..], b"prefix:"] {
        for &start in &starts {
            let mut reference = Vec::with_capacity(start);
            reference.extend_from_slice(prefix);
            serde_json::to_writer(&mut reference, trajectory).unwrap();
            let mut got = Vec::with_capacity(start);
            got.extend_from_slice(prefix);
            trajectory_into_v1(&mut got, trajectory).unwrap();
            assert!(
                got == reference,
                "{name}: from capacity {start}, the writers differ first at byte {:?}",
                first_difference_v1(&got, &reference)
            );
            assert_eq!(
                got.capacity(),
                reference.capacity(),
                "{name}: capacity from {start} after {} prefix bytes",
                prefix.len()
            );
        }
    }
}

/// `integer_array_v1` against serde_json writing the same slice, appended to
/// a buffer of `prefix` bytes and spare capacity `spare`: the same bytes and
/// the same final capacity.
fn assert_integer_array_v1<T>(values: &[T], prefix: usize, spare: usize)
where
    T: DecimalV1 + Serialize,
{
    let mut reference = Vec::with_capacity(prefix + spare);
    reference.resize(prefix, b'p');
    serde_json::to_writer(&mut reference, values).unwrap();
    let mut got = Vec::with_capacity(prefix + spare);
    got.resize(prefix, b'p');
    integer_array_v1(&mut got, values);
    assert!(
        got == reference,
        "{} values after {prefix} bytes with {spare} spare: first difference at byte {:?}",
        values.len(),
        first_difference_v1(&got, &reference)
    );
    assert_eq!(
        got.capacity(),
        reference.capacity(),
        "{} values after {prefix} bytes with {spare} spare",
        values.len()
    );
}

/// Every spare capacity that decides the array writer's path: none, every
/// one up to 40 bytes, around the exact text, and around the longest text of
/// one, two and three segments and of the whole array.
fn assert_integer_array_paths_v1<T>(values: &[T])
where
    T: DecimalV1 + Serialize,
{
    let exact = serde_json::to_vec(values).unwrap().len();
    let widest = T::MAX_TEXT + 1;
    let mut spares: Vec<usize> = (0..=40).collect();
    let mut around = vec![exact, values.len() * widest];
    around.extend((1..=3).map(|segments| segments * ARRAY_SEGMENT_V1 * widest));
    for centre in around {
        spares.extend([centre.saturating_sub(1), centre, centre + 1]);
    }
    for prefix in [0, 1, 9, 100] {
        for &spare in &spares {
            assert_integer_array_v1(values, prefix, spare);
        }
    }
}

/// The integer arrays of a decision: every value of `integer_cases_v1` for
/// the record's widths (u32 bit patterns, i64 indices, u64 and u8 scalars'
/// types) in one array and one at a time, empty arrays, single values, and
/// every spare capacity that changes the path taken.
#[test]
fn integer_arrays_have_the_previous_bytes_and_capacity_v1() {
    let u32_values: Vec<u32> = integer_cases_v1(32, false)
        .into_iter()
        .map(|x| u32::try_from(x).unwrap())
        .collect();
    let i64_values: Vec<i64> = integer_cases_v1(64, true)
        .into_iter()
        .map(|x| i64::try_from(x).unwrap())
        .collect();
    let u64_values: Vec<u64> = integer_cases_v1(64, false)
        .into_iter()
        .map(|x| u64::try_from(x).unwrap())
        .collect();
    let u8_values: Vec<u8> = (0..=u8::MAX).collect();
    assert!(i64_values.contains(&i64::MIN) && i64_values.contains(&i64::MAX));
    assert!(u32_values.contains(&u32::MAX) && u64_values.contains(&u64::MAX));
    for spare in [0, 128, 1 << 24] {
        assert_integer_array_v1(&u32_values, 3, spare);
        assert_integer_array_v1(&i64_values, 3, spare);
        assert_integer_array_v1(&u64_values, 3, spare);
        assert_integer_array_v1(&u8_values, 3, spare);
    }
    // Each value alone, through the exact-length path and the piecewise one.
    for &value in u32_values.iter().step_by(97) {
        assert_integer_array_paths_v1(&[value]);
    }
    for &value in i64_values.iter().step_by(97) {
        assert_integer_array_paths_v1(&[value]);
    }
    for values in [
        &[][..],
        &[0],
        &[9],
        &[10],
        &[u32::MAX],
        &[0, 0, 0],
        &[1_065_353_216, 0, 0x8000_0000, 9, 10, u32::MAX],
    ] {
        assert_integer_array_paths_v1(values);
    }
    for values in [
        &[][..],
        &[i64::MIN],
        &[i64::MAX],
        &[-1],
        &[-10, -9, 0, 9, 10],
        &[i64::MIN, -1, 0, 1, i64::MAX],
        &[3, 1, 4, 1, 5, 9, 2, 6],
    ] {
        assert_integer_array_paths_v1(values);
    }
    // The edges of the writer's digit paths (one digit, two, three to eight,
    // nine or ten within u32, and above u32), alone and as one long array.
    let edges: [u64; 16] = [
        0,
        9,
        10,
        99,
        100,
        99_999_999,
        100_000_000,
        999_999_999,
        1_000_000_000,
        1_065_353_216,
        2_147_483_648,
        u64::from(u32::MAX) - 1,
        u64::from(u32::MAX),
        u64::from(u32::MAX) + 1,
        9_999_999_999,
        u64::MAX,
    ];
    let u32_edges: Vec<u32> = edges
        .iter()
        .filter_map(|&x| u32::try_from(x).ok())
        .collect();
    let i64_edges: Vec<i64> = edges
        .iter()
        .filter_map(|&x| i64::try_from(x).ok())
        .flat_map(|x| [x, -x, x.saturating_add(1).saturating_neg()])
        .chain([i64::MIN, i64::MIN + 1])
        .collect();
    assert_integer_array_paths_v1(&edges);
    assert_integer_array_paths_v1(&u32_edges);
    assert_integer_array_paths_v1(&i64_edges);
    let long_edges: Vec<u32> = u32_edges.iter().copied().cycle().take(1_000).collect();
    assert_integer_array_paths_v1(&long_edges);
    // A long array of short values, as most decision tensors are.
    let short: Vec<i64> = (0..10_000).map(|x| x % 37 - 3).collect();
    assert_integer_array_paths_v1(&short);
    // Scalars: one write of the value's text, as serde_json makes.
    for &value in &u64_values {
        let mut got = Vec::new();
        integer_v1(&mut got, value);
        assert_eq!(got, serde_json::to_vec(&value).unwrap());
    }
    for &value in &i64_values {
        let mut text = [0u8; 20];
        let length = write_decimal_v1(&mut text, value);
        assert_eq!(&text[..length], value.to_string().as_bytes());
        assert_eq!(length, decimal_len_v1(value));
    }
}

/// Real decision tensors (`replay_fixture`, with and without seat
/// behaviors), then the same trajectory with every optional member of the
/// trajectory and its decisions present (training math at its non-default
/// value), boundary values in every integer member of the decisions, the
/// search rows and the opening record, escapes in its strings, then one
/// decision of 200,000-element arrays, then empty collections.
#[test]
fn trajectories_have_the_previous_bytes_v1() {
    let mut learner = FrozenPlayPolicyV1::training_fixture_v3();
    let mut opponent = FrozenPlayPolicyV1::training_fixture_v3();
    let actors: Vec<u8> = [0, 1, 1, 0, 1, 0, 0, 1].repeat(2);
    let (paired, _, _) = replay_fixture(&mut learner, Some(&mut opponent), 1, &actors);
    assert!(paired.seat_behaviors.is_some());
    assert_trajectory_bytes_v1("json-writer-test-paired.json", &paired);
    let (trajectory, _, _) = replay_fixture(&mut learner, None, 0, &actors);
    assert!(trajectory
        .decisions
        .iter()
        .any(|d| d.tensor.state.len() > 100));
    assert_trajectory_bytes_v1("json-writer-test-fixture.json", &trajectory);

    let u32_cases = [
        0,
        1,
        9,
        10,
        99,
        100,
        1_065_353_216, // 1.0f32
        0x8000_0000,   // -0.0f32
        1,             // the smallest subnormal
        0x7fc0_0000,   // a quiet NaN
        u32::MAX,
    ];
    let i64_cases = [i64::MIN, -10, -9, -1, 0, 1, 9, 10, 1 << 40, i64::MAX];
    let u64_cases = [0, 9, 10, u64::from(u32::MAX) + 1, u64::MAX];
    let strings = escape_cases_v1();
    let mut every = paired.clone();
    every.learner_sampler = Some(strings[2].clone());
    every.training_math = TrainingMathV1::PortableV1;
    every.input_contract = Some(InputContractRecordV1 {
        contract: strings[2].clone(),
        state_digest_columns: [0, usize::MAX],
        conversion: PinnedFileV1 {
            path: strings[3].clone().into(),
            sha256: "0".repeat(64),
        },
    });
    every.opening_record = Some(PauperOpeningRecordV1 {
        opening: PauperOpeningV1::LondonV1,
        seats: [
            PauperOpeningSeatRecordV1 {
                mode: PauperMulliganModeV1::Learned,
                mulligans_taken: 0,
                kept_hand_size: 9,
                exposed_pregame_decisions: 10,
                forced_zero_card_keep: false,
            },
            PauperOpeningSeatRecordV1 {
                mode: PauperMulliganModeV1::KeepSeven,
                mulligans_taken: u8::MAX,
                kept_hand_size: 10,
                exposed_pregame_decisions: u32::MAX,
                forced_zero_card_keep: true,
            },
        ],
    });
    every.episode.seed = u64::MAX;
    every.episode.max_physical_decisions = 10;
    every.episode.max_policy_steps = 9;
    every.search_played = Some(
        u64_cases
            .iter()
            .zip(u32_cases)
            .map(|(&step, policy_selected)| SearchPlayedRowV1 {
                step,
                policy_selected,
            })
            .collect(),
    );
    every.episode.id = strings[3].clone();
    every.terminal.terminal_reason = strings[1].clone();
    every.terminal.terminal_reward = [i32::MIN, i32::MAX];
    every.terminal.episode_id = u64::MAX;
    for (index, decision) in every.decisions.iter_mut().enumerate() {
        decision.step = u64_cases[index % u64_cases.len()];
        decision.physical_decision_id = u64_cases[(index + 1) % u64_cases.len()];
        decision.substep_index = u32_cases[index % u32_cases.len()];
        decision.substep_count = u32::MAX;
        decision.actor = [0, 1, 9, 10, u8::MAX][index % 5];
        decision.selected = u32_cases[(index + 3) % u32_cases.len()];
        decision.value = u32_cases[(index + 5) % u32_cases.len()];
        decision.logits.extend(u32_cases);
        decision.sampler_identity = Some(strings[index % strings.len()].clone());
        let tensor = &mut decision.tensor;
        for bits in [
            &mut tensor.state,
            &mut tensor.object_features,
            &mut tensor.edge_features,
            &mut tensor.action_features,
            &mut tensor.action_ref_features,
        ] {
            bits.extend(u32_cases);
        }
        for indices in [
            &mut tensor.object_card_ids,
            &mut tensor.object_groups,
            &mut tensor.object_node_ids,
            &mut tensor.edge_source_indices,
            &mut tensor.edge_target_indices,
            &mut tensor.action_ref_card_ids,
            &mut tensor.action_ref_action_indices,
            &mut tensor.action_ref_node_indices,
            &mut tensor.pairing_node_indices,
        ] {
            indices.extend(i64_cases);
        }
    }
    assert_trajectory_bytes_v1("json-writer-test-every.json", &every);

    let mut maximal = every.clone();
    maximal.decisions.truncate(1);
    let decision = &mut maximal.decisions[0];
    let cycle_u32 = |n: usize| -> Vec<u32> { u32_cases.iter().copied().cycle().take(n).collect() };
    let cycle_i64 = |n: usize| -> Vec<i64> { i64_cases.iter().copied().cycle().take(n).collect() };
    decision.logits = cycle_u32(4_096);
    decision.tensor.object_features = cycle_u32(200_000);
    decision.tensor.action_features = cycle_u32(200_000);
    decision.tensor.object_card_ids = cycle_i64(200_000);
    assert_trajectory_bytes_v1("json-writer-test-maximal.json", &maximal);

    let mut bare = trajectory.clone();
    bare.decisions.truncate(1);
    let decision = &mut bare.decisions[0];
    decision.logits.clear();
    let tensor = &mut decision.tensor;
    for bits in [
        &mut tensor.state,
        &mut tensor.object_features,
        &mut tensor.edge_features,
        &mut tensor.action_features,
        &mut tensor.action_ref_features,
    ] {
        bits.clear();
    }
    for indices in [
        &mut tensor.object_card_ids,
        &mut tensor.object_groups,
        &mut tensor.object_node_ids,
        &mut tensor.edge_source_indices,
        &mut tensor.edge_target_indices,
        &mut tensor.action_ref_card_ids,
        &mut tensor.action_ref_action_indices,
        &mut tensor.action_ref_node_indices,
    ] {
        indices.clear();
    }
    bare.search_played = Some(Vec::new());
    assert_trajectory_bytes_v1("json-writer-test-bare.json", &bare);

    let mut empty = trajectory;
    empty.decisions.clear();
    assert_trajectory_bytes_v1("json-writer-test-empty.json", &empty);
}

/// A worker's records written through one reused plaintext buffer, which
/// starts out holding stale bytes and passes from longer records to shorter
/// ones and back: every published record is the previous writer's bytes
/// sealed as before, and a sealed record's plaintext is left in the buffer.
#[test]
fn sealed_trajectories_reuse_the_plaintext_buffer_with_the_previous_bytes_v1() {
    let mut learner = FrozenPlayPolicyV1::training_fixture_v3();
    let mut opponent = FrozenPlayPolicyV1::training_fixture_v3();
    let actors: Vec<u8> = [0, 1, 1, 0, 1, 0, 0, 1].repeat(2);
    let (paired, _, _) = replay_fixture(&mut learner, Some(&mut opponent), 1, &actors);
    let (single, _, _) = replay_fixture(&mut learner, None, 0, &actors);
    let mut short = single.clone();
    short.decisions.truncate(1);
    let mut empty = single.clone();
    empty.decisions.clear();
    let records = [&paired, &short, &single, &empty, &paired];
    for envelope in [RecordEnvelopeV1::None, RecordEnvelopeV1::Zstd1] {
        let stale = b"stale bytes of an earlier record ".repeat(50_000);
        let mut plaintext = stale.clone();
        for (index, trajectory) in records.iter().enumerate() {
            let plain = reference_v1(*trajectory);
            let expected = envelope.seal_owned_v1(plain.clone()).unwrap();
            let name = format!("json-writer-test-reused-{index}.json");
            let got = serialize_and_seal_trajectory_v1(envelope, &name, trajectory, &mut plaintext)
                .unwrap();
            assert!(
                got == expected,
                "{envelope:?} record {index}: first difference at byte {:?}",
                first_difference_v1(&got, &expected)
            );
            match envelope {
                RecordEnvelopeV1::None => assert!(plaintext == stale),
                RecordEnvelopeV1::Zstd1 => assert!(plaintext == plain),
            }
        }
    }
}

/// The JSON checkpoint (`checkpoint_fixture_v1`: real parameters and Adam
/// moments), written by `serialize_json_v1` as the update publishes it.
#[test]
fn checkpoints_have_the_previous_bytes_v1() {
    let (_, _, checkpoint) = checkpoint_fixture_v1();
    let expected = reference_v1(&checkpoint);
    assert!(expected.len() > 100_000);
    for _ in 0..2 {
        let got = serialize_json_v1("json-writer-test-checkpoint.json", &checkpoint).unwrap();
        assert!(
            got == expected,
            "the writers differ first at byte {:?}",
            first_difference_v1(&got, &expected)
        );
    }
    assert_same_capacity_v1(&checkpoint);
}

/// Recorded trajectory files (plain or sealed) in the directory
/// `MTG_KERNEL_JSON_WRITER_RECORDS`: each is parsed as its readers parse it
/// and must be written back byte for byte by both writers.
#[test]
#[ignore = "reads recorded trajectories from MTG_KERNEL_JSON_WRITER_RECORDS"]
fn recorded_trajectories_are_written_back_byte_for_byte_v1() {
    let directory = std::env::var_os("MTG_KERNEL_JSON_WRITER_RECORDS")
        .expect("MTG_KERNEL_JSON_WRITER_RECORDS names a directory of trajectory files");
    let mut paths: Vec<_> = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("episode-") && name.ends_with(".json"))
        })
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "no episode-*.json files");
    for path in &paths {
        let file = std::fs::read(path).unwrap();
        let record = record_envelope_v1::open_v1(&file, MAX_FILE_BYTES).unwrap();
        let trajectory: ExpandedTrajectoryV1 = serde_json::from_slice(&record).unwrap();
        assert!(reference_v1(&trajectory) == *record, "{path:?}: reference");
        let got = serialize_json_v1("json-writer-test-recorded.json", &trajectory).unwrap();
        assert!(
            got == *record,
            "{path:?}: the writers differ first at byte {:?}",
            first_difference_v1(&got, &record)
        );
    }
    eprintln!("{} recorded trajectories written back exactly", paths.len());
}
