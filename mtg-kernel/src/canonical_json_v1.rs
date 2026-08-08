//! Fail-closed canonical JSON for Native Training Store V2 records.
//!
//! This module deliberately separates byte-level canonicalization from typed
//! record schemas. It enforces the store-wide JSON invariants: compact UTF-8,
//! printable ASCII strings, recursively ASCII-byte-sorted object keys, exact
//! integer numbers, bounded nesting/strings/objects, and exactly one final LF.
//! A raw parser rejects duplicate keys before any value can be represented as
//! [`serde_json::Value`].
//!
//! Depth is the number of containing arrays/objects: a scalar root has depth
//! zero and a root array/object has depth one. String bounds count decoded
//! bytes; printable ASCII makes that count unambiguous. Null is forbidden by
//! default. A typed schema may use [`CanonicalJsonNullPolicyV1::AllowOnly`] to
//! enumerate its exact null-bearing paths; every other null still fails.

use serde::{
    de::DeserializeOwned,
    ser::{
        Impossible, SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant,
        SerializeTuple, SerializeTupleStruct, SerializeTupleVariant,
    },
    Serialize, Serializer,
};
use std::{alloc::Layout, collections::BTreeMap, error::Error, fmt};

pub const CANONICAL_JSON_MAX_DEPTH_V1: usize = 32;
pub const CANONICAL_JSON_MAX_STRING_BYTES_V1: usize = 4096;
pub const CANONICAL_JSON_MAX_OBJECT_KEYS_V1: usize = 256;

/// A segment in a typed schema's exact null-bearing path declaration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanonicalJsonNullPathSegmentV1 {
    ObjectKey(&'static str),
    ArrayIndex(usize),
    AnyArrayElement,
}

/// Codec-level null admission. Production callers use `Forbid` unless their
/// typed schema enumerates every nullable path with `AllowOnly`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CanonicalJsonNullPolicyV1 {
    #[default]
    Forbid,
    AllowOnly(&'static [&'static [CanonicalJsonNullPathSegmentV1]]),
}

/// Stable, privacy-safe canonical JSON failure categories.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanonicalJsonErrorKindV1 {
    Serialization,
    EncodedLengthOverflow,
    Deserialization,
    InvalidSyntax,
    MissingFinalLf,
    TrailingBytes,
    NonCanonicalBytes,
    DuplicateObjectKey,
    FloatingPointForbidden,
    IntegerOutOfRange,
    NullForbidden,
    NonPrintableAscii,
    StringTooLong,
    ObjectTooLarge,
    DepthTooDeep,
}

impl CanonicalJsonErrorKindV1 {
    /// Stable machine-readable code. Codes contain no input-derived content.
    pub const fn code(self) -> &'static str {
        match self {
            Self::Serialization => "canonical_json_serialization",
            Self::EncodedLengthOverflow => "canonical_json_encoded_length_overflow",
            Self::Deserialization => "canonical_json_deserialization",
            Self::InvalidSyntax => "canonical_json_invalid_syntax",
            Self::MissingFinalLf => "canonical_json_missing_final_lf",
            Self::TrailingBytes => "canonical_json_trailing_bytes",
            Self::NonCanonicalBytes => "canonical_json_noncanonical_bytes",
            Self::DuplicateObjectKey => "canonical_json_duplicate_object_key",
            Self::FloatingPointForbidden => "canonical_json_floating_point_forbidden",
            Self::IntegerOutOfRange => "canonical_json_integer_out_of_range",
            Self::NullForbidden => "canonical_json_null_forbidden",
            Self::NonPrintableAscii => "canonical_json_non_printable_ascii",
            Self::StringTooLong => "canonical_json_string_too_long",
            Self::ObjectTooLarge => "canonical_json_object_too_large",
            Self::DepthTooDeep => "canonical_json_depth_too_deep",
        }
    }
}

/// Canonical JSON error with no source bytes, key names, values, or parser text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CanonicalJsonErrorV1 {
    kind: CanonicalJsonErrorKindV1,
}

impl CanonicalJsonErrorV1 {
    const fn new(kind: CanonicalJsonErrorKindV1) -> Self {
        Self { kind }
    }

    pub const fn kind(self) -> CanonicalJsonErrorKindV1 {
        self.kind
    }

    pub const fn code(self) -> &'static str {
        self.kind.code()
    }
}

impl fmt::Display for CanonicalJsonErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl Error for CanonicalJsonErrorV1 {}

impl serde::ser::Error for CanonicalJsonErrorV1 {
    fn custom<T: fmt::Display>(_message: T) -> Self {
        Self::new(CanonicalJsonErrorKindV1::Serialization)
    }
}

type Result<T> = std::result::Result<T, CanonicalJsonErrorV1>;

#[derive(Debug, Eq, PartialEq)]
enum CanonicalJsonNumberV1 {
    Signed(i64),
    Unsigned(u64),
}

#[derive(Debug, Eq, PartialEq)]
enum CanonicalJsonValueV1 {
    Null,
    Bool(bool),
    Number(CanonicalJsonNumberV1),
    String(String),
    Array(Vec<Self>),
    Object(BTreeMap<String, Self>),
}

/// Conservative architecture-dependent products for the private canonical
/// tree. A complete JSON token contributes at least one byte, so charging the
/// full token ceiling independently to values, strings, and object entries
/// dominates every array/string/key population reached by that document.
pub(crate) fn canonical_json_tree_allocation_layout_bytes_v1(
    max_json_token_count: usize,
) -> Option<[u64; 3]> {
    Some([
        allocation_layout_bytes_v1::<CanonicalJsonValueV1>(max_json_token_count)?,
        allocation_layout_bytes_v1::<String>(max_json_token_count)?,
        allocation_layout_bytes_v1::<(String, CanonicalJsonValueV1)>(max_json_token_count)?,
    ])
}

fn allocation_layout_bytes_v1<T>(count: usize) -> Option<u64> {
    u64::try_from(Layout::array::<T>(count).ok()?.size()).ok()
}

/// Serializes a serde value and emits its exact canonical Store V2 JSON bytes.
///
/// A dedicated serde serializer rejects floats (including non-finite floats),
/// duplicate map keys, invalid strings, and bounds violations while building
/// the private canonical tree. Consequently, even a custom `Serialize`
/// implementation that emits the same map key twice fails closed before any
/// `serde_json::Value` conversion.
pub fn to_canonical_json_bytes_v1<T: Serialize + ?Sized>(
    value: &T,
    null_policy: CanonicalJsonNullPolicyV1,
) -> Result<Vec<u8>> {
    let parsed = serialize_checked_value_v1(value, null_policy)?;
    encode_value_with_final_lf(&parsed)
}

/// Returns the exact canonical Store V2 JSON byte count, including the final
/// LF, without allocating the encoded byte vector.
///
/// This uses the same checked serialization and chunk traversal as
/// [`to_canonical_json_bytes_v1`]. The input's `Serialize` implementation is
/// invoked exactly once; malformed custom implementations therefore fail with
/// the same first error as the byte-emitting path.
pub fn count_canonical_json_bytes_v1<T: Serialize + ?Sized>(
    value: &T,
    null_policy: CanonicalJsonNullPolicyV1,
) -> Result<u64> {
    let parsed = serialize_checked_value_v1(value, null_policy)?;
    let mut sink = CanonicalJsonCountSinkV1::default();
    emit_value_with_final_lf(&parsed, &mut sink)?;
    Ok(sink.encoded_len)
}

/// Allocation-free closed-grammar maximum used by the Store V2 preflight.
///
/// `token_bytes` excludes a document's final LF. Depth follows the codec's
/// convention: a scalar root is zero and a root container is one. The other
/// two fields retain the largest reachable object/string bounds so composing
/// a closed shape cannot hide a codec-limit violation in a child.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CanonicalJsonClosedMaxV1 {
    token_bytes: u64,
    depth: usize,
    max_object_keys: usize,
    max_string_bytes: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CanonicalJsonClosedMaxErrorV1 {
    Arithmetic,
    InvalidLiteral,
    UnsortedOrDuplicateKey,
    Depth,
    ObjectKeys,
    StringBytes,
}

impl CanonicalJsonClosedMaxV1 {
    pub(crate) const fn token_bytes(self) -> u64 {
        self.token_bytes
    }

    #[cfg(test)]
    pub(crate) const fn depth(self) -> usize {
        self.depth
    }

    #[cfg(test)]
    pub(crate) const fn max_object_keys(self) -> usize {
        self.max_object_keys
    }

    #[cfg(test)]
    pub(crate) const fn max_string_bytes(self) -> usize {
        self.max_string_bytes
    }

    pub(crate) const fn null_v1() -> Self {
        Self::scalar_v1(4)
    }

    pub(crate) const fn bool_v1(value: bool) -> Self {
        Self::scalar_v1(if value { 4 } else { 5 })
    }

    pub(crate) const fn max_u63_v1() -> Self {
        Self::scalar_v1(19)
    }

    pub(crate) const fn max_u32_v1() -> Self {
        Self::scalar_v1(10)
    }

    pub(crate) const fn terminal_return_i8_v1() -> Self {
        Self::scalar_v1(2)
    }

    pub(crate) fn exact_unsigned_decimal_digits_v1(
        digits: u8,
    ) -> std::result::Result<Self, CanonicalJsonClosedMaxErrorV1> {
        if !(1..=19).contains(&digits) {
            return Err(CanonicalJsonClosedMaxErrorV1::InvalidLiteral);
        }
        Ok(Self::scalar_v1(u64::from(digits)))
    }

    pub(crate) fn exact_u64_v1(mut value: u64) -> Self {
        let mut digits = 1_u64;
        while value >= 10 {
            value /= 10;
            digits += 1;
        }
        Self::scalar_v1(digits)
    }

    const fn scalar_v1(token_bytes: u64) -> Self {
        Self {
            token_bytes,
            depth: 0,
            max_object_keys: 0,
            max_string_bytes: 0,
        }
    }

    pub(crate) fn fixed_ascii_string_v1(
        literal: &str,
    ) -> std::result::Result<Self, CanonicalJsonClosedMaxErrorV1> {
        if literal.len() > CANONICAL_JSON_MAX_STRING_BYTES_V1 {
            return Err(CanonicalJsonClosedMaxErrorV1::StringBytes);
        }
        let mut escaped_bytes = 0_u64;
        for byte in literal.bytes() {
            if !(0x20..=0x7e).contains(&byte) {
                return Err(CanonicalJsonClosedMaxErrorV1::InvalidLiteral);
            }
            escaped_bytes = escaped_bytes
                .checked_add(if matches!(byte, b'"' | b'\\') { 2 } else { 1 })
                .ok_or(CanonicalJsonClosedMaxErrorV1::Arithmetic)?;
        }
        Ok(Self {
            token_bytes: escaped_bytes
                .checked_add(2)
                .ok_or(CanonicalJsonClosedMaxErrorV1::Arithmetic)?,
            depth: 0,
            max_object_keys: 0,
            max_string_bytes: literal.len(),
        })
    }

    pub(crate) fn fixed_ascii_string_bytes_v1(
        decoded_bytes: u64,
    ) -> std::result::Result<Self, CanonicalJsonClosedMaxErrorV1> {
        let decoded = usize::try_from(decoded_bytes)
            .map_err(|_| CanonicalJsonClosedMaxErrorV1::StringBytes)?;
        if decoded > CANONICAL_JSON_MAX_STRING_BYTES_V1 {
            return Err(CanonicalJsonClosedMaxErrorV1::StringBytes);
        }
        Ok(Self {
            token_bytes: decoded_bytes
                .checked_add(2)
                .ok_or(CanonicalJsonClosedMaxErrorV1::Arithmetic)?,
            depth: 0,
            max_object_keys: 0,
            max_string_bytes: decoded,
        })
    }

    pub(crate) fn choice_v1(
        left: Self,
        right: Self,
    ) -> std::result::Result<Self, CanonicalJsonClosedMaxErrorV1> {
        Self::validate_composed_v1(Self {
            token_bytes: left.token_bytes.max(right.token_bytes),
            depth: left.depth.max(right.depth),
            max_object_keys: left.max_object_keys.max(right.max_object_keys),
            max_string_bytes: left.max_string_bytes.max(right.max_string_bytes),
        })
    }

    pub(crate) fn array_v1(
        count: u64,
        child: Self,
    ) -> std::result::Result<Self, CanonicalJsonClosedMaxErrorV1> {
        let contents = if count == 0 {
            0
        } else {
            count
                .checked_mul(child.token_bytes)
                .and_then(|value| value.checked_add(count - 1))
                .ok_or(CanonicalJsonClosedMaxErrorV1::Arithmetic)?
        };
        let depth = if count == 0 {
            1
        } else {
            child
                .depth
                .checked_add(1)
                .ok_or(CanonicalJsonClosedMaxErrorV1::Arithmetic)?
        };
        Self::validate_composed_v1(Self {
            token_bytes: contents
                .checked_add(2)
                .ok_or(CanonicalJsonClosedMaxErrorV1::Arithmetic)?,
            depth,
            max_object_keys: child.max_object_keys,
            max_string_bytes: child.max_string_bytes,
        })
    }

    /// Exact fixed array whose positions have distinct closed semantic roles.
    pub(crate) fn fixed_array_v1(
        children: &[Self],
    ) -> std::result::Result<Self, CanonicalJsonClosedMaxErrorV1> {
        let mut token_bytes = 2_u64;
        let mut max_child_depth = 0_usize;
        let mut max_object_keys = 0_usize;
        let mut max_string_bytes = 0_usize;
        for (index, child) in children.iter().enumerate() {
            if index != 0 {
                token_bytes = token_bytes
                    .checked_add(1)
                    .ok_or(CanonicalJsonClosedMaxErrorV1::Arithmetic)?;
            }
            token_bytes = token_bytes
                .checked_add(child.token_bytes)
                .ok_or(CanonicalJsonClosedMaxErrorV1::Arithmetic)?;
            max_child_depth = max_child_depth.max(child.depth);
            max_object_keys = max_object_keys.max(child.max_object_keys);
            max_string_bytes = max_string_bytes.max(child.max_string_bytes);
        }
        Self::validate_composed_v1(Self {
            token_bytes,
            depth: if children.is_empty() {
                1
            } else {
                max_child_depth
                    .checked_add(1)
                    .ok_or(CanonicalJsonClosedMaxErrorV1::Arithmetic)?
            },
            max_object_keys,
            max_string_bytes,
        })
    }

    /// Builds an object from fields already arranged in canonical ASCII key
    /// order. Requiring the order here makes an omitted/renamed wire field a
    /// visible change beside the private wire that owns its grammar.
    pub(crate) fn object_v1(
        fields: &[(&str, Self)],
    ) -> std::result::Result<Self, CanonicalJsonClosedMaxErrorV1> {
        if fields.len() > CANONICAL_JSON_MAX_OBJECT_KEYS_V1 {
            return Err(CanonicalJsonClosedMaxErrorV1::ObjectKeys);
        }
        let mut previous_key = None;
        let mut token_bytes = 2_u64;
        let mut max_child_depth = 0_usize;
        let mut max_object_keys = fields.len();
        let mut max_string_bytes = 0_usize;
        for (index, (key, child)) in fields.iter().enumerate() {
            if previous_key.is_some_and(|previous: &str| previous.as_bytes() >= key.as_bytes()) {
                return Err(CanonicalJsonClosedMaxErrorV1::UnsortedOrDuplicateKey);
            }
            let key_shape = Self::fixed_ascii_string_v1(key)?;
            if index != 0 {
                token_bytes = token_bytes
                    .checked_add(1)
                    .ok_or(CanonicalJsonClosedMaxErrorV1::Arithmetic)?;
            }
            token_bytes = token_bytes
                .checked_add(key_shape.token_bytes)
                .and_then(|value| value.checked_add(1))
                .and_then(|value| value.checked_add(child.token_bytes))
                .ok_or(CanonicalJsonClosedMaxErrorV1::Arithmetic)?;
            max_child_depth = max_child_depth.max(child.depth);
            max_object_keys = max_object_keys.max(child.max_object_keys);
            max_string_bytes = max_string_bytes
                .max(key_shape.max_string_bytes)
                .max(child.max_string_bytes);
            previous_key = Some(*key);
        }
        Self::validate_composed_v1(Self {
            token_bytes,
            depth: if fields.is_empty() {
                1
            } else {
                max_child_depth
                    .checked_add(1)
                    .ok_or(CanonicalJsonClosedMaxErrorV1::Arithmetic)?
            },
            max_object_keys,
            max_string_bytes,
        })
    }

    pub(crate) fn canonical_document_bytes_v1(
        self,
    ) -> std::result::Result<u64, CanonicalJsonClosedMaxErrorV1> {
        Self::validate_composed_v1(self)?
            .token_bytes
            .checked_add(1)
            .ok_or(CanonicalJsonClosedMaxErrorV1::Arithmetic)
    }

    fn validate_composed_v1(
        value: Self,
    ) -> std::result::Result<Self, CanonicalJsonClosedMaxErrorV1> {
        if value.depth > CANONICAL_JSON_MAX_DEPTH_V1 {
            return Err(CanonicalJsonClosedMaxErrorV1::Depth);
        }
        if value.max_object_keys > CANONICAL_JSON_MAX_OBJECT_KEYS_V1 {
            return Err(CanonicalJsonClosedMaxErrorV1::ObjectKeys);
        }
        if value.max_string_bytes > CANONICAL_JSON_MAX_STRING_BYTES_V1 {
            return Err(CanonicalJsonClosedMaxErrorV1::StringBytes);
        }
        Ok(value)
    }
}

/// Validates canonical bytes and returns a JSON value only after duplicate-key
/// rejection, invariant validation, canonical re-encoding, and exact equality.
pub fn parse_canonical_json_bytes_v1(
    bytes: &[u8],
    null_policy: CanonicalJsonNullPolicyV1,
) -> Result<serde_json::Value> {
    let parsed = parse_and_require_canonical(bytes, null_policy)?;
    Ok(parsed.into_serde_json_value())
}

/// Validates canonical bytes without constructing a public JSON value.
pub fn validate_canonical_json_bytes_v1(
    bytes: &[u8],
    null_policy: CanonicalJsonNullPolicyV1,
) -> Result<()> {
    parse_and_require_canonical(bytes, null_policy).map(drop)
}

/// Deserializes a typed value only after byte-level canonical validation.
/// Unknown-field and field-specific scalar/null constraints belong to `T`.
pub fn from_canonical_json_bytes_v1<T: DeserializeOwned>(
    bytes: &[u8],
    null_policy: CanonicalJsonNullPolicyV1,
) -> Result<T> {
    parse_and_require_canonical(bytes, null_policy)?;
    serde_json::from_slice(&bytes[..bytes.len() - 1])
        .map_err(|_| CanonicalJsonErrorV1::new(CanonicalJsonErrorKindV1::Deserialization))
}

fn parse_and_require_canonical(
    bytes: &[u8],
    null_policy: CanonicalJsonNullPolicyV1,
) -> Result<CanonicalJsonValueV1> {
    let Some(payload) = bytes.strip_suffix(b"\n") else {
        return Err(CanonicalJsonErrorV1::new(
            CanonicalJsonErrorKindV1::MissingFinalLf,
        ));
    };
    let parsed = ParserV1::parse_document(payload, null_policy)?;
    require_allowed_null_paths(&parsed, null_policy)?;
    if encode_value_with_final_lf(&parsed)? != bytes {
        return Err(CanonicalJsonErrorV1::new(
            CanonicalJsonErrorKindV1::NonCanonicalBytes,
        ));
    }
    Ok(parsed)
}

fn serialize_checked_value_v1<T: Serialize + ?Sized>(
    value: &T,
    null_policy: CanonicalJsonNullPolicyV1,
) -> Result<CanonicalJsonValueV1> {
    let parsed = value.serialize(ValueSerializerV1 {
        null_policy,
        containing_depth: 0,
    })?;
    require_allowed_null_paths(&parsed, null_policy)?;
    Ok(parsed)
}

trait CanonicalJsonChunkSinkV1 {
    fn emit_chunk(&mut self, chunk: &[u8]) -> Result<()>;
}

impl CanonicalJsonChunkSinkV1 for Vec<u8> {
    fn emit_chunk(&mut self, chunk: &[u8]) -> Result<()> {
        self.extend_from_slice(chunk);
        Ok(())
    }
}

#[derive(Default)]
struct CanonicalJsonCountSinkV1 {
    encoded_len: u64,
}

impl CanonicalJsonChunkSinkV1 for CanonicalJsonCountSinkV1 {
    fn emit_chunk(&mut self, chunk: &[u8]) -> Result<()> {
        let chunk_len = u64::try_from(chunk.len()).map_err(|_| {
            CanonicalJsonErrorV1::new(CanonicalJsonErrorKindV1::EncodedLengthOverflow)
        })?;
        self.encoded_len = self.encoded_len.checked_add(chunk_len).ok_or_else(|| {
            CanonicalJsonErrorV1::new(CanonicalJsonErrorKindV1::EncodedLengthOverflow)
        })?;
        Ok(())
    }
}

fn emit_value_with_final_lf<S: CanonicalJsonChunkSinkV1>(
    value: &CanonicalJsonValueV1,
    sink: &mut S,
) -> Result<()> {
    value.emit(sink)?;
    sink.emit_chunk(b"\n")
}

fn encode_value_with_final_lf(value: &CanonicalJsonValueV1) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    emit_value_with_final_lf(value, &mut bytes)?;
    Ok(bytes)
}

impl CanonicalJsonValueV1 {
    fn emit<S: CanonicalJsonChunkSinkV1>(&self, sink: &mut S) -> Result<()> {
        match self {
            Self::Null => sink.emit_chunk(b"null")?,
            Self::Bool(false) => sink.emit_chunk(b"false")?,
            Self::Bool(true) => sink.emit_chunk(b"true")?,
            Self::Number(CanonicalJsonNumberV1::Signed(value)) => {
                let encoded = value.to_string();
                sink.emit_chunk(encoded.as_bytes())?;
            }
            Self::Number(CanonicalJsonNumberV1::Unsigned(value)) => {
                let encoded = value.to_string();
                sink.emit_chunk(encoded.as_bytes())?;
            }
            Self::String(value) => emit_string(value, sink)?,
            Self::Array(values) => {
                sink.emit_chunk(b"[")?;
                for (index, value) in values.iter().enumerate() {
                    if index != 0 {
                        sink.emit_chunk(b",")?;
                    }
                    value.emit(sink)?;
                }
                sink.emit_chunk(b"]")?;
            }
            Self::Object(values) => {
                sink.emit_chunk(b"{")?;
                for (index, (key, value)) in values.iter().enumerate() {
                    if index != 0 {
                        sink.emit_chunk(b",")?;
                    }
                    emit_string(key, sink)?;
                    sink.emit_chunk(b":")?;
                    value.emit(sink)?;
                }
                sink.emit_chunk(b"}")?;
            }
        }
        Ok(())
    }

    fn into_serde_json_value(self) -> serde_json::Value {
        match self {
            Self::Null => serde_json::Value::Null,
            Self::Bool(value) => serde_json::Value::Bool(value),
            Self::Number(CanonicalJsonNumberV1::Signed(value)) => {
                serde_json::Value::Number(value.into())
            }
            Self::Number(CanonicalJsonNumberV1::Unsigned(value)) => {
                serde_json::Value::Number(value.into())
            }
            Self::String(value) => serde_json::Value::String(value),
            Self::Array(values) => serde_json::Value::Array(
                values
                    .into_iter()
                    .map(CanonicalJsonValueV1::into_serde_json_value)
                    .collect(),
            ),
            Self::Object(values) => serde_json::Value::Object(
                values
                    .into_iter()
                    .map(|(key, value)| (key, value.into_serde_json_value()))
                    .collect(),
            ),
        }
    }
}

fn emit_string<S: CanonicalJsonChunkSinkV1>(value: &str, sink: &mut S) -> Result<()> {
    debug_assert!(value.bytes().all(|byte| (0x20..=0x7e).contains(&byte)));
    sink.emit_chunk(b"\"")?;
    let bytes = value.as_bytes();
    let mut chunk_start = 0;
    for (index, byte) in bytes.iter().copied().enumerate() {
        match byte {
            b'"' | b'\\' => {
                sink.emit_chunk(&bytes[chunk_start..index])?;
                sink.emit_chunk(if byte == b'"' { b"\\\"" } else { b"\\\\" })?;
                chunk_start = index + 1;
            }
            _ => {}
        }
    }
    sink.emit_chunk(&bytes[chunk_start..])?;
    sink.emit_chunk(b"\"")
}

#[derive(Clone, Copy)]
struct ValueSerializerV1 {
    null_policy: CanonicalJsonNullPolicyV1,
    containing_depth: usize,
}

impl ValueSerializerV1 {
    fn nested(self, containing_depth: usize) -> Self {
        Self {
            null_policy: self.null_policy,
            containing_depth,
        }
    }

    fn null(self) -> Result<CanonicalJsonValueV1> {
        Ok(CanonicalJsonValueV1::Null)
    }

    fn string(self, value: &str) -> Result<CanonicalJsonValueV1> {
        validate_printable_ascii(value)?;
        Ok(CanonicalJsonValueV1::String(value.to_owned()))
    }
}

/// First-error state for serde compound builders.
///
/// Serde callers are expected to propagate serializer errors, but this codec is
/// an authority boundary: a hand-written `Serialize` implementation must not be
/// able to catch an error and then publish a partial value. Once any compound
/// operation fails, every later operation and `end` return that original error.
#[derive(Default)]
struct CompoundSerializerStateV1 {
    first_error: Option<CanonicalJsonErrorV1>,
}

impl CompoundSerializerStateV1 {
    fn require_usable(&self) -> Result<()> {
        match self.first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    fn record<T>(&mut self, result: Result<T>) -> Result<T> {
        if let Some(error) = self.first_error {
            return Err(error);
        }
        match result {
            Ok(value) => Ok(value),
            Err(error) => {
                self.first_error = Some(error);
                Err(error)
            }
        }
    }

    fn fail<T>(&mut self, kind: CanonicalJsonErrorKindV1) -> Result<T> {
        self.record(Err(CanonicalJsonErrorV1::new(kind)))
    }
}

impl Serializer for ValueSerializerV1 {
    type Ok = CanonicalJsonValueV1;
    type Error = CanonicalJsonErrorV1;
    type SerializeSeq = SequenceSerializerV1;
    type SerializeTuple = SequenceSerializerV1;
    type SerializeTupleStruct = SequenceSerializerV1;
    type SerializeTupleVariant = TupleVariantSerializerV1;
    type SerializeMap = ObjectSerializerV1;
    type SerializeStruct = ObjectSerializerV1;
    type SerializeStructVariant = StructVariantSerializerV1;

    fn serialize_bool(self, value: bool) -> Result<Self::Ok> {
        Ok(CanonicalJsonValueV1::Bool(value))
    }

    fn serialize_i8(self, value: i8) -> Result<Self::Ok> {
        self.serialize_i64(i64::from(value))
    }

    fn serialize_i16(self, value: i16) -> Result<Self::Ok> {
        self.serialize_i64(i64::from(value))
    }

    fn serialize_i32(self, value: i32) -> Result<Self::Ok> {
        self.serialize_i64(i64::from(value))
    }

    fn serialize_i64(self, value: i64) -> Result<Self::Ok> {
        Ok(CanonicalJsonValueV1::Number(CanonicalJsonNumberV1::Signed(
            value,
        )))
    }

    fn serialize_i128(self, value: i128) -> Result<Self::Ok> {
        let value = i64::try_from(value)
            .map_err(|_| CanonicalJsonErrorV1::new(CanonicalJsonErrorKindV1::IntegerOutOfRange))?;
        self.serialize_i64(value)
    }

    fn serialize_u8(self, value: u8) -> Result<Self::Ok> {
        self.serialize_u64(u64::from(value))
    }

    fn serialize_u16(self, value: u16) -> Result<Self::Ok> {
        self.serialize_u64(u64::from(value))
    }

    fn serialize_u32(self, value: u32) -> Result<Self::Ok> {
        self.serialize_u64(u64::from(value))
    }

    fn serialize_u64(self, value: u64) -> Result<Self::Ok> {
        Ok(CanonicalJsonValueV1::Number(
            CanonicalJsonNumberV1::Unsigned(value),
        ))
    }

    fn serialize_u128(self, value: u128) -> Result<Self::Ok> {
        let value = u64::try_from(value)
            .map_err(|_| CanonicalJsonErrorV1::new(CanonicalJsonErrorKindV1::IntegerOutOfRange))?;
        self.serialize_u64(value)
    }

    fn serialize_f32(self, _value: f32) -> Result<Self::Ok> {
        Err(CanonicalJsonErrorV1::new(
            CanonicalJsonErrorKindV1::FloatingPointForbidden,
        ))
    }

    fn serialize_f64(self, _value: f64) -> Result<Self::Ok> {
        Err(CanonicalJsonErrorV1::new(
            CanonicalJsonErrorKindV1::FloatingPointForbidden,
        ))
    }

    fn serialize_char(self, value: char) -> Result<Self::Ok> {
        let mut encoded = [0_u8; 4];
        self.serialize_str(value.encode_utf8(&mut encoded))
    }

    fn serialize_str(self, value: &str) -> Result<Self::Ok> {
        self.string(value)
    }

    fn serialize_bytes(self, value: &[u8]) -> Result<Self::Ok> {
        checked_container_depth(self.containing_depth)?;
        Ok(CanonicalJsonValueV1::Array(
            value
                .iter()
                .map(|&byte| {
                    CanonicalJsonValueV1::Number(CanonicalJsonNumberV1::Unsigned(u64::from(byte)))
                })
                .collect(),
        ))
    }

    fn serialize_none(self) -> Result<Self::Ok> {
        self.null()
    }

    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<Self::Ok> {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<Self::Ok> {
        self.null()
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<Self::Ok> {
        self.null()
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<Self::Ok> {
        self.string(variant)
    }

    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<Self::Ok> {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Self::Ok> {
        validate_printable_ascii(variant)?;
        let object_depth = checked_container_depth(self.containing_depth)?;
        let nested = value.serialize(self.nested(object_depth))?;
        Ok(singleton_object(variant.to_owned(), nested))
    }

    fn serialize_seq(self, _length: Option<usize>) -> Result<Self::SerializeSeq> {
        let depth = checked_container_depth(self.containing_depth)?;
        Ok(SequenceSerializerV1 {
            values: Vec::new(),
            null_policy: self.null_policy,
            depth,
            state: CompoundSerializerStateV1::default(),
        })
    }

    fn serialize_tuple(self, length: usize) -> Result<Self::SerializeTuple> {
        self.serialize_seq(Some(length))
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        length: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        self.serialize_seq(Some(length))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        _length: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        validate_printable_ascii(variant)?;
        let object_depth = checked_container_depth(self.containing_depth)?;
        let array_depth = checked_container_depth(object_depth)?;
        Ok(TupleVariantSerializerV1 {
            variant: variant.to_owned(),
            values: Vec::new(),
            null_policy: self.null_policy,
            array_depth,
            state: CompoundSerializerStateV1::default(),
        })
    }

    fn serialize_map(self, _length: Option<usize>) -> Result<Self::SerializeMap> {
        let depth = checked_container_depth(self.containing_depth)?;
        Ok(ObjectSerializerV1::new(self.null_policy, depth))
    }

    fn serialize_struct(
        self,
        _name: &'static str,
        _length: usize,
    ) -> Result<Self::SerializeStruct> {
        self.serialize_map(None)
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        _length: usize,
    ) -> Result<Self::SerializeStructVariant> {
        validate_printable_ascii(variant)?;
        let object_depth = checked_container_depth(self.containing_depth)?;
        let fields_depth = checked_container_depth(object_depth)?;
        Ok(StructVariantSerializerV1 {
            variant: variant.to_owned(),
            fields: ObjectSerializerV1::new(self.null_policy, fields_depth),
        })
    }
}

struct SequenceSerializerV1 {
    values: Vec<CanonicalJsonValueV1>,
    null_policy: CanonicalJsonNullPolicyV1,
    depth: usize,
    state: CompoundSerializerStateV1,
}

impl SequenceSerializerV1 {
    fn push<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        self.state.require_usable()?;
        let value = value.serialize(ValueSerializerV1 {
            null_policy: self.null_policy,
            containing_depth: self.depth,
        });
        let value = self.state.record(value)?;
        self.values.push(value);
        Ok(())
    }

    fn finish(self) -> Result<CanonicalJsonValueV1> {
        self.state.require_usable()?;
        Ok(CanonicalJsonValueV1::Array(self.values))
    }
}

impl SerializeSeq for SequenceSerializerV1 {
    type Ok = CanonicalJsonValueV1;
    type Error = CanonicalJsonErrorV1;

    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        self.push(value)
    }

    fn end(self) -> Result<Self::Ok> {
        self.finish()
    }
}

impl SerializeTuple for SequenceSerializerV1 {
    type Ok = CanonicalJsonValueV1;
    type Error = CanonicalJsonErrorV1;

    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        self.push(value)
    }

    fn end(self) -> Result<Self::Ok> {
        self.finish()
    }
}

impl SerializeTupleStruct for SequenceSerializerV1 {
    type Ok = CanonicalJsonValueV1;
    type Error = CanonicalJsonErrorV1;

    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        self.push(value)
    }

    fn end(self) -> Result<Self::Ok> {
        self.finish()
    }
}

struct TupleVariantSerializerV1 {
    variant: String,
    values: Vec<CanonicalJsonValueV1>,
    null_policy: CanonicalJsonNullPolicyV1,
    array_depth: usize,
    state: CompoundSerializerStateV1,
}

impl SerializeTupleVariant for TupleVariantSerializerV1 {
    type Ok = CanonicalJsonValueV1;
    type Error = CanonicalJsonErrorV1;

    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        self.state.require_usable()?;
        let value = value.serialize(ValueSerializerV1 {
            null_policy: self.null_policy,
            containing_depth: self.array_depth,
        });
        let value = self.state.record(value)?;
        self.values.push(value);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok> {
        self.state.require_usable()?;
        Ok(singleton_object(
            self.variant,
            CanonicalJsonValueV1::Array(self.values),
        ))
    }
}

struct ObjectSerializerV1 {
    values: BTreeMap<String, CanonicalJsonValueV1>,
    pending_key: Option<String>,
    null_policy: CanonicalJsonNullPolicyV1,
    depth: usize,
    state: CompoundSerializerStateV1,
}

impl ObjectSerializerV1 {
    fn new(null_policy: CanonicalJsonNullPolicyV1, depth: usize) -> Self {
        Self {
            values: BTreeMap::new(),
            pending_key: None,
            null_policy,
            depth,
            state: CompoundSerializerStateV1::default(),
        }
    }

    fn accept_key(&self, key: &str) -> Result<()> {
        if self.values.contains_key(key) || self.pending_key.as_deref() == Some(key) {
            return Err(CanonicalJsonErrorV1::new(
                CanonicalJsonErrorKindV1::DuplicateObjectKey,
            ));
        }
        if self.values.len() == CANONICAL_JSON_MAX_OBJECT_KEYS_V1 {
            return Err(CanonicalJsonErrorV1::new(
                CanonicalJsonErrorKindV1::ObjectTooLarge,
            ));
        }
        Ok(())
    }

    fn insert<T: Serialize + ?Sized>(&mut self, key: String, value: &T) -> Result<()> {
        self.state.require_usable()?;
        if self.pending_key.is_some() {
            return self.state.fail(CanonicalJsonErrorKindV1::Serialization);
        }
        let accepted = self.accept_key(&key);
        self.state.record(accepted)?;
        let value = value.serialize(ValueSerializerV1 {
            null_policy: self.null_policy,
            containing_depth: self.depth,
        });
        let value = self.state.record(value)?;
        self.values.insert(key, value);
        Ok(())
    }

    fn finish(mut self) -> Result<CanonicalJsonValueV1> {
        self.state.require_usable()?;
        if self.pending_key.is_some() {
            return self.state.fail(CanonicalJsonErrorKindV1::Serialization);
        }
        Ok(CanonicalJsonValueV1::Object(self.values))
    }
}

impl SerializeMap for ObjectSerializerV1 {
    type Ok = CanonicalJsonValueV1;
    type Error = CanonicalJsonErrorV1;

    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<()> {
        self.state.require_usable()?;
        if self.pending_key.is_some() {
            return self.state.fail(CanonicalJsonErrorKindV1::Serialization);
        }
        let key = key.serialize(MapKeySerializerV1);
        let key = self.state.record(key)?;
        let accepted = self.accept_key(&key);
        self.state.record(accepted)?;
        self.pending_key = Some(key);
        Ok(())
    }

    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        self.state.require_usable()?;
        let Some(key) = self.pending_key.take() else {
            return self.state.fail(CanonicalJsonErrorKindV1::Serialization);
        };
        let value = value.serialize(ValueSerializerV1 {
            null_policy: self.null_policy,
            containing_depth: self.depth,
        });
        let value = self.state.record(value)?;
        self.values.insert(key, value);
        Ok(())
    }

    fn serialize_entry<K: Serialize + ?Sized, V: Serialize + ?Sized>(
        &mut self,
        key: &K,
        value: &V,
    ) -> Result<()> {
        self.state.require_usable()?;
        let key = key.serialize(MapKeySerializerV1);
        let key = self.state.record(key)?;
        self.insert(key, value)
    }

    fn end(self) -> Result<Self::Ok> {
        self.finish()
    }
}

impl SerializeStruct for ObjectSerializerV1 {
    type Ok = CanonicalJsonValueV1;
    type Error = CanonicalJsonErrorV1;

    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<()> {
        self.state.require_usable()?;
        let key_validation = validate_printable_ascii(key);
        self.state.record(key_validation)?;
        self.insert(key.to_owned(), value)
    }

    fn end(self) -> Result<Self::Ok> {
        self.finish()
    }
}

struct StructVariantSerializerV1 {
    variant: String,
    fields: ObjectSerializerV1,
}

impl SerializeStructVariant for StructVariantSerializerV1 {
    type Ok = CanonicalJsonValueV1;
    type Error = CanonicalJsonErrorV1;

    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<()> {
        SerializeStruct::serialize_field(&mut self.fields, key, value)
    }

    fn end(self) -> Result<Self::Ok> {
        Ok(singleton_object(self.variant, self.fields.finish()?))
    }
}

struct MapKeySerializerV1;

impl MapKeySerializerV1 {
    fn checked(value: String) -> Result<String> {
        validate_printable_ascii(&value)?;
        Ok(value)
    }

    fn unsupported<T>() -> Result<T> {
        Err(CanonicalJsonErrorV1::new(
            CanonicalJsonErrorKindV1::Serialization,
        ))
    }
}

impl Serializer for MapKeySerializerV1 {
    type Ok = String;
    type Error = CanonicalJsonErrorV1;
    type SerializeSeq = Impossible<String, CanonicalJsonErrorV1>;
    type SerializeTuple = Impossible<String, CanonicalJsonErrorV1>;
    type SerializeTupleStruct = Impossible<String, CanonicalJsonErrorV1>;
    type SerializeTupleVariant = Impossible<String, CanonicalJsonErrorV1>;
    type SerializeMap = Impossible<String, CanonicalJsonErrorV1>;
    type SerializeStruct = Impossible<String, CanonicalJsonErrorV1>;
    type SerializeStructVariant = Impossible<String, CanonicalJsonErrorV1>;

    fn serialize_bool(self, value: bool) -> Result<Self::Ok> {
        Self::checked(value.to_string())
    }

    fn serialize_i8(self, value: i8) -> Result<Self::Ok> {
        self.serialize_i64(i64::from(value))
    }

    fn serialize_i16(self, value: i16) -> Result<Self::Ok> {
        self.serialize_i64(i64::from(value))
    }

    fn serialize_i32(self, value: i32) -> Result<Self::Ok> {
        self.serialize_i64(i64::from(value))
    }

    fn serialize_i64(self, value: i64) -> Result<Self::Ok> {
        Self::checked(value.to_string())
    }

    fn serialize_i128(self, value: i128) -> Result<Self::Ok> {
        let value = i64::try_from(value)
            .map_err(|_| CanonicalJsonErrorV1::new(CanonicalJsonErrorKindV1::IntegerOutOfRange))?;
        self.serialize_i64(value)
    }

    fn serialize_u8(self, value: u8) -> Result<Self::Ok> {
        self.serialize_u64(u64::from(value))
    }

    fn serialize_u16(self, value: u16) -> Result<Self::Ok> {
        self.serialize_u64(u64::from(value))
    }

    fn serialize_u32(self, value: u32) -> Result<Self::Ok> {
        self.serialize_u64(u64::from(value))
    }

    fn serialize_u64(self, value: u64) -> Result<Self::Ok> {
        Self::checked(value.to_string())
    }

    fn serialize_u128(self, value: u128) -> Result<Self::Ok> {
        let value = u64::try_from(value)
            .map_err(|_| CanonicalJsonErrorV1::new(CanonicalJsonErrorKindV1::IntegerOutOfRange))?;
        self.serialize_u64(value)
    }

    fn serialize_f32(self, _value: f32) -> Result<Self::Ok> {
        Err(CanonicalJsonErrorV1::new(
            CanonicalJsonErrorKindV1::FloatingPointForbidden,
        ))
    }

    fn serialize_f64(self, _value: f64) -> Result<Self::Ok> {
        Err(CanonicalJsonErrorV1::new(
            CanonicalJsonErrorKindV1::FloatingPointForbidden,
        ))
    }

    fn serialize_char(self, value: char) -> Result<Self::Ok> {
        Self::checked(value.to_string())
    }

    fn serialize_str(self, value: &str) -> Result<Self::Ok> {
        Self::checked(value.to_owned())
    }

    fn serialize_bytes(self, _value: &[u8]) -> Result<Self::Ok> {
        Self::unsupported()
    }

    fn serialize_none(self) -> Result<Self::Ok> {
        Self::unsupported()
    }

    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<Self::Ok> {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<Self::Ok> {
        Self::unsupported()
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<Self::Ok> {
        Self::unsupported()
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<Self::Ok> {
        Self::checked(variant.to_owned())
    }

    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<Self::Ok> {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _value: &T,
    ) -> Result<Self::Ok> {
        Self::unsupported()
    }

    fn serialize_seq(self, _length: Option<usize>) -> Result<Self::SerializeSeq> {
        Self::unsupported()
    }

    fn serialize_tuple(self, _length: usize) -> Result<Self::SerializeTuple> {
        Self::unsupported()
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _length: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        Self::unsupported()
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _length: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        Self::unsupported()
    }

    fn serialize_map(self, _length: Option<usize>) -> Result<Self::SerializeMap> {
        Self::unsupported()
    }

    fn serialize_struct(
        self,
        _name: &'static str,
        _length: usize,
    ) -> Result<Self::SerializeStruct> {
        Self::unsupported()
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _length: usize,
    ) -> Result<Self::SerializeStructVariant> {
        Self::unsupported()
    }
}

fn singleton_object(key: String, value: CanonicalJsonValueV1) -> CanonicalJsonValueV1 {
    CanonicalJsonValueV1::Object([(key, value)].into_iter().collect())
}

fn validate_printable_ascii(value: &str) -> Result<()> {
    if value.len() > CANONICAL_JSON_MAX_STRING_BYTES_V1 {
        return Err(CanonicalJsonErrorV1::new(
            CanonicalJsonErrorKindV1::StringTooLong,
        ));
    }
    if !value.bytes().all(|byte| (0x20..=0x7e).contains(&byte)) {
        return Err(CanonicalJsonErrorV1::new(
            CanonicalJsonErrorKindV1::NonPrintableAscii,
        ));
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum ActualPathSegmentV1<'a> {
    ObjectKey(&'a str),
    ArrayIndex(usize),
}

fn require_allowed_null_paths(
    value: &CanonicalJsonValueV1,
    policy: CanonicalJsonNullPolicyV1,
) -> Result<()> {
    let allowed_paths = match policy {
        CanonicalJsonNullPolicyV1::Forbid => &[][..],
        CanonicalJsonNullPolicyV1::AllowOnly(paths) => paths,
    };
    require_allowed_null_paths_at(value, allowed_paths, &mut Vec::new())
}

fn require_allowed_null_paths_at<'a>(
    value: &'a CanonicalJsonValueV1,
    allowed_paths: &[&[CanonicalJsonNullPathSegmentV1]],
    path: &mut Vec<ActualPathSegmentV1<'a>>,
) -> Result<()> {
    match value {
        CanonicalJsonValueV1::Null => {
            if allowed_paths
                .iter()
                .any(|allowed| null_path_matches(path, allowed))
            {
                Ok(())
            } else {
                Err(CanonicalJsonErrorV1::new(
                    CanonicalJsonErrorKindV1::NullForbidden,
                ))
            }
        }
        CanonicalJsonValueV1::Array(values) => {
            for (index, child) in values.iter().enumerate() {
                path.push(ActualPathSegmentV1::ArrayIndex(index));
                require_allowed_null_paths_at(child, allowed_paths, path)?;
                path.pop();
            }
            Ok(())
        }
        CanonicalJsonValueV1::Object(values) => {
            for (key, child) in values {
                path.push(ActualPathSegmentV1::ObjectKey(key));
                require_allowed_null_paths_at(child, allowed_paths, path)?;
                path.pop();
            }
            Ok(())
        }
        CanonicalJsonValueV1::Bool(_)
        | CanonicalJsonValueV1::Number(_)
        | CanonicalJsonValueV1::String(_) => Ok(()),
    }
}

fn null_path_matches(
    actual: &[ActualPathSegmentV1<'_>],
    allowed: &[CanonicalJsonNullPathSegmentV1],
) -> bool {
    actual.len() == allowed.len()
        && actual
            .iter()
            .zip(allowed)
            .all(|(actual, allowed)| match (actual, allowed) {
                (
                    ActualPathSegmentV1::ObjectKey(actual),
                    CanonicalJsonNullPathSegmentV1::ObjectKey(allowed),
                ) => actual == allowed,
                (
                    ActualPathSegmentV1::ArrayIndex(actual),
                    CanonicalJsonNullPathSegmentV1::ArrayIndex(allowed),
                ) => actual == allowed,
                (
                    ActualPathSegmentV1::ArrayIndex(_),
                    CanonicalJsonNullPathSegmentV1::AnyArrayElement,
                ) => true,
                _ => false,
            })
}

struct ParserV1<'a> {
    bytes: &'a [u8],
    position: usize,
    null_policy: CanonicalJsonNullPolicyV1,
}

impl<'a> ParserV1<'a> {
    fn parse_document(
        bytes: &'a [u8],
        null_policy: CanonicalJsonNullPolicyV1,
    ) -> Result<CanonicalJsonValueV1> {
        let mut parser = Self {
            bytes,
            position: 0,
            null_policy,
        };
        parser.skip_whitespace();
        let value = parser.parse_value(0)?;
        parser.skip_whitespace();
        if parser.position != parser.bytes.len() {
            return Err(CanonicalJsonErrorV1::new(
                CanonicalJsonErrorKindV1::TrailingBytes,
            ));
        }
        Ok(value)
    }

    fn parse_value(&mut self, containing_depth: usize) -> Result<CanonicalJsonValueV1> {
        match self.peek() {
            Some(b'n') => self.parse_null(),
            Some(b'f') => {
                self.require_literal(b"false")?;
                Ok(CanonicalJsonValueV1::Bool(false))
            }
            Some(b't') => {
                self.require_literal(b"true")?;
                Ok(CanonicalJsonValueV1::Bool(true))
            }
            Some(b'"') => self.parse_string().map(CanonicalJsonValueV1::String),
            Some(b'[') => self.parse_array(containing_depth),
            Some(b'{') => self.parse_object(containing_depth),
            Some(b'-' | b'0'..=b'9') => self.parse_number(),
            _ => Err(CanonicalJsonErrorV1::new(
                CanonicalJsonErrorKindV1::InvalidSyntax,
            )),
        }
    }

    fn parse_null(&mut self) -> Result<CanonicalJsonValueV1> {
        self.require_literal(b"null")?;
        if self.null_policy == CanonicalJsonNullPolicyV1::Forbid {
            return Err(CanonicalJsonErrorV1::new(
                CanonicalJsonErrorKindV1::NullForbidden,
            ));
        }
        Ok(CanonicalJsonValueV1::Null)
    }

    fn parse_array(&mut self, containing_depth: usize) -> Result<CanonicalJsonValueV1> {
        let depth = checked_container_depth(containing_depth)?;
        self.position += 1;
        self.skip_whitespace();
        let mut values = Vec::new();
        if self.consume(b']') {
            return Ok(CanonicalJsonValueV1::Array(values));
        }
        loop {
            values.push(self.parse_value(depth)?);
            self.skip_whitespace();
            if self.consume(b']') {
                return Ok(CanonicalJsonValueV1::Array(values));
            }
            self.require_byte(b',')?;
            self.skip_whitespace();
        }
    }

    fn parse_object(&mut self, containing_depth: usize) -> Result<CanonicalJsonValueV1> {
        let depth = checked_container_depth(containing_depth)?;
        self.position += 1;
        self.skip_whitespace();
        let mut values = BTreeMap::new();
        if self.consume(b'}') {
            return Ok(CanonicalJsonValueV1::Object(values));
        }
        loop {
            if self.peek() != Some(b'"') {
                return Err(CanonicalJsonErrorV1::new(
                    CanonicalJsonErrorKindV1::InvalidSyntax,
                ));
            }
            let key = self.parse_string()?;
            if values.contains_key(&key) {
                return Err(CanonicalJsonErrorV1::new(
                    CanonicalJsonErrorKindV1::DuplicateObjectKey,
                ));
            }
            if values.len() == CANONICAL_JSON_MAX_OBJECT_KEYS_V1 {
                return Err(CanonicalJsonErrorV1::new(
                    CanonicalJsonErrorKindV1::ObjectTooLarge,
                ));
            }
            self.skip_whitespace();
            self.require_byte(b':')?;
            self.skip_whitespace();
            let value = self.parse_value(depth)?;
            values.insert(key, value);
            self.skip_whitespace();
            if self.consume(b'}') {
                return Ok(CanonicalJsonValueV1::Object(values));
            }
            self.require_byte(b',')?;
            self.skip_whitespace();
        }
    }

    fn parse_number(&mut self) -> Result<CanonicalJsonValueV1> {
        let start = self.position;
        let negative = self.consume(b'-');
        match self.peek() {
            Some(b'0') => {
                self.position += 1;
                if matches!(self.peek(), Some(b'0'..=b'9')) {
                    return Err(CanonicalJsonErrorV1::new(
                        CanonicalJsonErrorKindV1::InvalidSyntax,
                    ));
                }
            }
            Some(b'1'..=b'9') => {
                self.position += 1;
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.position += 1;
                }
            }
            _ => {
                return Err(CanonicalJsonErrorV1::new(
                    CanonicalJsonErrorKindV1::InvalidSyntax,
                ));
            }
        }

        let mut floating_point = false;
        if self.consume(b'.') {
            floating_point = true;
            self.require_digits()?;
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            floating_point = true;
            self.position += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.position += 1;
            }
            self.require_digits()?;
        }
        if floating_point {
            return Err(CanonicalJsonErrorV1::new(
                CanonicalJsonErrorKindV1::FloatingPointForbidden,
            ));
        }

        let digits_start = start + usize::from(negative);
        let magnitude = parse_u64_decimal(&self.bytes[digits_start..self.position])?;
        let number = if negative {
            let signed = if magnitude == (i64::MAX as u64) + 1 {
                i64::MIN
            } else {
                let magnitude = i64::try_from(magnitude).map_err(|_| {
                    CanonicalJsonErrorV1::new(CanonicalJsonErrorKindV1::IntegerOutOfRange)
                })?;
                -magnitude
            };
            CanonicalJsonNumberV1::Signed(signed)
        } else {
            CanonicalJsonNumberV1::Unsigned(magnitude)
        };
        Ok(CanonicalJsonValueV1::Number(number))
    }

    fn parse_string(&mut self) -> Result<String> {
        self.require_byte(b'"')?;
        let mut decoded = String::new();
        loop {
            let byte = self.next().ok_or_else(|| {
                CanonicalJsonErrorV1::new(CanonicalJsonErrorKindV1::InvalidSyntax)
            })?;
            match byte {
                b'"' => return Ok(decoded),
                b'\\' => {
                    let escaped = self.next().ok_or_else(|| {
                        CanonicalJsonErrorV1::new(CanonicalJsonErrorKindV1::InvalidSyntax)
                    })?;
                    let decoded_byte = match escaped {
                        b'"' => b'"',
                        b'/' => b'/',
                        b'\\' => b'\\',
                        b'b' => 0x08,
                        b'f' => 0x0c,
                        b'n' => b'\n',
                        b'r' => b'\r',
                        b't' => b'\t',
                        b'u' => {
                            let scalar = self.parse_unicode_escape()?;
                            if !(0x20..=0x7e).contains(&scalar) {
                                return Err(CanonicalJsonErrorV1::new(
                                    CanonicalJsonErrorKindV1::NonPrintableAscii,
                                ));
                            }
                            scalar as u8
                        }
                        _ => {
                            return Err(CanonicalJsonErrorV1::new(
                                CanonicalJsonErrorKindV1::InvalidSyntax,
                            ));
                        }
                    };
                    push_printable_ascii(&mut decoded, decoded_byte)?;
                }
                0x20..=0x7e => push_printable_ascii(&mut decoded, byte)?,
                0x00..=0x1f => {
                    return Err(CanonicalJsonErrorV1::new(
                        CanonicalJsonErrorKindV1::InvalidSyntax,
                    ));
                }
                _ => {
                    return Err(CanonicalJsonErrorV1::new(
                        CanonicalJsonErrorKindV1::NonPrintableAscii,
                    ));
                }
            }
        }
    }

    fn parse_unicode_escape(&mut self) -> Result<u32> {
        let first = self.parse_hex_quad()?;
        match first {
            0xd800..=0xdbff => {
                self.require_byte(b'\\')?;
                self.require_byte(b'u')?;
                let second = self.parse_hex_quad()?;
                if !(0xdc00..=0xdfff).contains(&second) {
                    return Err(CanonicalJsonErrorV1::new(
                        CanonicalJsonErrorKindV1::InvalidSyntax,
                    ));
                }
                Ok(0x1_0000 + ((first - 0xd800) << 10) + (second - 0xdc00))
            }
            0xdc00..=0xdfff => Err(CanonicalJsonErrorV1::new(
                CanonicalJsonErrorKindV1::InvalidSyntax,
            )),
            _ => Ok(first),
        }
    }

    fn parse_hex_quad(&mut self) -> Result<u32> {
        let mut value = 0_u32;
        for _ in 0..4 {
            let byte = self.next().ok_or_else(|| {
                CanonicalJsonErrorV1::new(CanonicalJsonErrorKindV1::InvalidSyntax)
            })?;
            let digit = match byte {
                b'0'..=b'9' => u32::from(byte - b'0'),
                b'a'..=b'f' => u32::from(byte - b'a') + 10,
                b'A'..=b'F' => u32::from(byte - b'A') + 10,
                _ => {
                    return Err(CanonicalJsonErrorV1::new(
                        CanonicalJsonErrorKindV1::InvalidSyntax,
                    ));
                }
            };
            value = (value << 4) | digit;
        }
        Ok(value)
    }

    fn require_digits(&mut self) -> Result<()> {
        if !matches!(self.peek(), Some(b'0'..=b'9')) {
            return Err(CanonicalJsonErrorV1::new(
                CanonicalJsonErrorKindV1::InvalidSyntax,
            ));
        }
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.position += 1;
        }
        Ok(())
    }

    fn require_literal(&mut self, literal: &[u8]) -> Result<()> {
        if self.bytes.get(self.position..self.position + literal.len()) != Some(literal) {
            return Err(CanonicalJsonErrorV1::new(
                CanonicalJsonErrorKindV1::InvalidSyntax,
            ));
        }
        self.position += literal.len();
        Ok(())
    }

    fn require_byte(&mut self, expected: u8) -> Result<()> {
        if self.consume(expected) {
            Ok(())
        } else {
            Err(CanonicalJsonErrorV1::new(
                CanonicalJsonErrorKindV1::InvalidSyntax,
            ))
        }
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.position += 1;
        }
    }

    fn consume(&mut self, expected: u8) -> bool {
        if self.peek() == Some(expected) {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.position).copied()
    }

    fn next(&mut self) -> Option<u8> {
        let byte = self.peek()?;
        self.position += 1;
        Some(byte)
    }
}

fn checked_container_depth(containing_depth: usize) -> Result<usize> {
    if containing_depth == CANONICAL_JSON_MAX_DEPTH_V1 {
        return Err(CanonicalJsonErrorV1::new(
            CanonicalJsonErrorKindV1::DepthTooDeep,
        ));
    }
    Ok(containing_depth + 1)
}

fn push_printable_ascii(output: &mut String, byte: u8) -> Result<()> {
    if !(0x20..=0x7e).contains(&byte) {
        return Err(CanonicalJsonErrorV1::new(
            CanonicalJsonErrorKindV1::NonPrintableAscii,
        ));
    }
    if output.len() == CANONICAL_JSON_MAX_STRING_BYTES_V1 {
        return Err(CanonicalJsonErrorV1::new(
            CanonicalJsonErrorKindV1::StringTooLong,
        ));
    }
    output.push(char::from(byte));
    Ok(())
}

fn parse_u64_decimal(digits: &[u8]) -> Result<u64> {
    let mut value = 0_u64;
    for &digit in digits {
        value = value
            .checked_mul(10)
            .and_then(|value| value.checked_add(u64::from(digit - b'0')))
            .ok_or_else(|| {
                CanonicalJsonErrorV1::new(CanonicalJsonErrorKindV1::IntegerOutOfRange)
            })?;
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{
        ser::{
            SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant,
            SerializeTupleVariant,
        },
        Deserialize, Serializer,
    };
    use std::{cell::Cell, collections::BTreeMap};

    #[test]
    fn closed_max_recurrence_checks_escaping_shapes_and_codec_limits() {
        let escaped = CanonicalJsonClosedMaxV1::fixed_ascii_string_v1("a\"\\z").unwrap();
        assert_eq!(escaped.token_bytes(), 8);
        assert_eq!(escaped.max_string_bytes(), 4);

        let child = CanonicalJsonClosedMaxV1::fixed_ascii_string_v1("a").unwrap();
        let empty = CanonicalJsonClosedMaxV1::array_v1(0, child).unwrap();
        assert_eq!(empty.token_bytes(), 2);
        assert_eq!(empty.depth(), 1);
        assert_eq!(empty.max_string_bytes(), 1);
        let pair = CanonicalJsonClosedMaxV1::array_v1(2, child).unwrap();
        assert_eq!(pair.token_bytes(), 9);
        assert_eq!(pair.depth(), 1);

        assert_eq!(CanonicalJsonClosedMaxV1::exact_u64_v1(0).token_bytes(), 1);
        assert_eq!(CanonicalJsonClosedMaxV1::exact_u64_v1(33).token_bytes(), 2);
        let heterogeneous = CanonicalJsonClosedMaxV1::fixed_array_v1(&[
            CanonicalJsonClosedMaxV1::fixed_ascii_string_v1("a").unwrap(),
            CanonicalJsonClosedMaxV1::null_v1(),
        ])
        .unwrap();
        assert_eq!(heterogeneous.token_bytes(), 10);
        assert_eq!(heterogeneous.depth(), 1);

        let object = CanonicalJsonClosedMaxV1::object_v1(&[
            ("a", CanonicalJsonClosedMaxV1::null_v1()),
            ("b", CanonicalJsonClosedMaxV1::bool_v1(false)),
        ])
        .unwrap();
        assert_eq!(object.token_bytes(), 20);
        assert_eq!(object.canonical_document_bytes_v1().unwrap(), 21);
        assert_eq!(object.max_object_keys(), 2);

        assert_eq!(
            CanonicalJsonClosedMaxV1::object_v1(&[
                ("b", CanonicalJsonClosedMaxV1::null_v1()),
                ("a", CanonicalJsonClosedMaxV1::null_v1()),
            ])
            .unwrap_err(),
            CanonicalJsonClosedMaxErrorV1::UnsortedOrDuplicateKey
        );
        assert_eq!(
            CanonicalJsonClosedMaxV1::object_v1(&[
                ("a", CanonicalJsonClosedMaxV1::null_v1()),
                ("a", CanonicalJsonClosedMaxV1::null_v1()),
            ])
            .unwrap_err(),
            CanonicalJsonClosedMaxErrorV1::UnsortedOrDuplicateKey
        );

        let accepted = "a".repeat(CANONICAL_JSON_MAX_STRING_BYTES_V1);
        assert!(CanonicalJsonClosedMaxV1::fixed_ascii_string_v1(&accepted).is_ok());
        let rejected = format!("{accepted}a");
        assert_eq!(
            CanonicalJsonClosedMaxV1::fixed_ascii_string_v1(&rejected).unwrap_err(),
            CanonicalJsonClosedMaxErrorV1::StringBytes
        );

        let mut depth = CanonicalJsonClosedMaxV1::null_v1();
        for _ in 0..CANONICAL_JSON_MAX_DEPTH_V1 {
            depth = CanonicalJsonClosedMaxV1::array_v1(1, depth).unwrap();
        }
        assert_eq!(depth.depth(), CANONICAL_JSON_MAX_DEPTH_V1);
        assert_eq!(
            CanonicalJsonClosedMaxV1::array_v1(1, depth).unwrap_err(),
            CanonicalJsonClosedMaxErrorV1::Depth
        );
        assert_eq!(
            CanonicalJsonClosedMaxV1::array_v1(u64::MAX, CanonicalJsonClosedMaxV1::max_u63_v1(),)
                .unwrap_err(),
            CanonicalJsonClosedMaxErrorV1::Arithmetic
        );
        assert_eq!(
            CanonicalJsonClosedMaxV1 {
                token_bytes: u64::MAX,
                depth: 0,
                max_object_keys: 0,
                max_string_bytes: 0,
            }
            .canonical_document_bytes_v1()
            .unwrap_err(),
            CanonicalJsonClosedMaxErrorV1::Arithmetic
        );
    }

    #[test]
    fn closed_max_rejects_object_key_and_literal_boundaries() {
        let keys = (0..=CANONICAL_JSON_MAX_OBJECT_KEYS_V1)
            .map(|index| format!("k{index:03}"))
            .collect::<Vec<_>>();
        let fields = keys
            .iter()
            .map(|key| (key.as_str(), CanonicalJsonClosedMaxV1::null_v1()))
            .collect::<Vec<_>>();
        assert_eq!(
            CanonicalJsonClosedMaxV1::object_v1(&fields).unwrap_err(),
            CanonicalJsonClosedMaxErrorV1::ObjectKeys
        );
        assert_eq!(
            CanonicalJsonClosedMaxV1::fixed_ascii_string_v1("not\nprintable").unwrap_err(),
            CanonicalJsonClosedMaxErrorV1::InvalidLiteral
        );
    }

    fn assert_kind<T>(result: Result<T>, expected: CanonicalJsonErrorKindV1) {
        match result {
            Ok(_) => panic!("expected canonical JSON error {expected:?}"),
            Err(error) => assert_eq!(error.kind(), expected),
        }
    }

    fn assert_count_matches_bytes<T: Serialize + ?Sized>(
        value: &T,
        null_policy: CanonicalJsonNullPolicyV1,
    ) -> Vec<u8> {
        let bytes = to_canonical_json_bytes_v1(value, null_policy).unwrap();
        let count = count_canonical_json_bytes_v1(value, null_policy).unwrap();
        assert_eq!(count, u64::try_from(bytes.len()).unwrap());
        bytes
    }

    fn assert_emit_count_same_error<T: Serialize + ?Sized>(
        value: &T,
        null_policy: CanonicalJsonNullPolicyV1,
        expected: CanonicalJsonErrorKindV1,
    ) {
        let emit_error = to_canonical_json_bytes_v1(value, null_policy).unwrap_err();
        let count_error = count_canonical_json_bytes_v1(value, null_policy).unwrap_err();
        assert_eq!(emit_error.kind(), expected);
        assert_eq!(count_error, emit_error);
    }

    struct SerializationCallCounter<'a> {
        calls: &'a Cell<u32>,
    }

    impl Serialize for SerializationCallCounter<'_> {
        fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let call = self.calls.get().checked_add(1).unwrap();
            self.calls.set(call);
            serializer.serialize_u32(call)
        }
    }

    #[test]
    fn byte_and_count_paths_each_invoke_checked_serialization_once() {
        let byte_calls = Cell::new(0);
        assert_eq!(
            to_canonical_json_bytes_v1(
                &SerializationCallCounter { calls: &byte_calls },
                CanonicalJsonNullPolicyV1::Forbid,
            )
            .unwrap(),
            b"1\n"
        );
        assert_eq!(byte_calls.get(), 1);

        let count_calls = Cell::new(0);
        assert_eq!(
            count_canonical_json_bytes_v1(
                &SerializationCallCounter {
                    calls: &count_calls,
                },
                CanonicalJsonNullPolicyV1::Forbid,
            )
            .unwrap(),
            2
        );
        assert_eq!(count_calls.get(), 1);
    }

    #[test]
    fn count_sink_matches_exact_wire_bytes_including_escapes_and_final_lf() {
        for value in [
            serde_json::json!(false),
            serde_json::json!(i64::MIN),
            serde_json::json!(u64::MAX),
            serde_json::json!([]),
            serde_json::json!({}),
            serde_json::json!({
                "z\\\"": [true, "quote\"slash\\/", -17],
                "a": {"nested": ""}
            }),
        ] {
            let bytes = assert_count_matches_bytes(&value, CanonicalJsonNullPolicyV1::Forbid);
            assert_eq!(bytes.last(), Some(&b'\n'));
        }

        let escaped = serde_json::json!({
            "key\"\\": "value\"\\",
        });
        assert_eq!(
            assert_count_matches_bytes(&escaped, CanonicalJsonNullPolicyV1::Forbid),
            b"{\"key\\\"\\\\\":\"value\\\"\\\\\"}\n"
        );
    }

    #[test]
    fn count_sink_checked_add_rejects_final_lf_overflow_without_wrapping() {
        let mut sink = CanonicalJsonCountSinkV1 {
            encoded_len: u64::MAX - 4,
        };
        assert_kind(
            emit_value_with_final_lf(&CanonicalJsonValueV1::Bool(true), &mut sink),
            CanonicalJsonErrorKindV1::EncodedLengthOverflow,
        );
        assert_eq!(sink.encoded_len, u64::MAX);
        assert_kind(
            sink.emit_chunk(b"x"),
            CanonicalJsonErrorKindV1::EncodedLengthOverflow,
        );
        assert_eq!(sink.encoded_len, u64::MAX);
    }

    #[test]
    fn nested_objects_use_ascii_byte_order_compact_escaping_and_array_order() {
        let value = serde_json::json!({
            "z": 9_223_372_036_854_775_807_u64,
            "arr": [{"b": 2, "a": 1}, true],
            "a": {"z": -7, "a": "quote\"slash\\/"}
        });
        let actual = assert_count_matches_bytes(&value, CanonicalJsonNullPolicyV1::Forbid);
        let golden = b"{\"a\":{\"a\":\"quote\\\"slash\\\\/\",\"z\":-7},\"arr\":[{\"a\":1,\"b\":2},true],\"z\":9223372036854775807}\n";
        assert_eq!(actual, golden);
        validate_canonical_json_bytes_v1(&actual, CanonicalJsonNullPolicyV1::Forbid).unwrap();
    }

    #[test]
    fn ascii_byte_key_order_is_not_locale_or_insertion_order() {
        let mut value = serde_json::Map::new();
        for (key, number) in [("~", 5), ("a", 4), ("_", 3), ("A", 2), ("0", 1), ("!", 0)] {
            value.insert(key.to_owned(), serde_json::Value::from(number));
        }
        assert_eq!(
            to_canonical_json_bytes_v1(
                &serde_json::Value::Object(value),
                CanonicalJsonNullPolicyV1::Forbid,
            )
            .unwrap(),
            b"{\"!\":0,\"0\":1,\"A\":2,\"_\":3,\"a\":4,\"~\":5}\n"
        );
    }

    #[test]
    fn exact_signed_and_unsigned_integer_forms_have_literal_goldens() {
        let values = serde_json::json!([
            i64::MIN,
            -9_223_372_036_854_775_807_i64,
            -1,
            0,
            1,
            i64::MAX,
            u64::MAX
        ]);
        let actual =
            to_canonical_json_bytes_v1(&values, CanonicalJsonNullPolicyV1::Forbid).unwrap();
        assert_eq!(
            actual,
            b"[-9223372036854775808,-9223372036854775807,-1,0,1,9223372036854775807,18446744073709551615]\n"
        );
        validate_canonical_json_bytes_v1(&actual, CanonicalJsonNullPolicyV1::Forbid).unwrap();

        assert_kind(
            validate_canonical_json_bytes_v1(
                b"18446744073709551616\n",
                CanonicalJsonNullPolicyV1::Forbid,
            ),
            CanonicalJsonErrorKindV1::IntegerOutOfRange,
        );
        assert_kind(
            validate_canonical_json_bytes_v1(
                b"-9223372036854775809\n",
                CanonicalJsonNullPolicyV1::Forbid,
            ),
            CanonicalJsonErrorKindV1::IntegerOutOfRange,
        );
        assert_kind(
            to_canonical_json_bytes_v1(
                &(u128::from(u64::MAX) + 1),
                CanonicalJsonNullPolicyV1::Forbid,
            ),
            CanonicalJsonErrorKindV1::IntegerOutOfRange,
        );
        assert_kind(
            to_canonical_json_bytes_v1(
                &(i128::from(i64::MIN) - 1),
                CanonicalJsonNullPolicyV1::Forbid,
            ),
            CanonicalJsonErrorKindV1::IntegerOutOfRange,
        );
    }

    struct DuplicateMap;

    impl Serialize for DuplicateMap {
        fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let mut map = serializer.serialize_map(Some(2))?;
            map.serialize_entry("same", &1)?;
            map.serialize_entry("same", &2)?;
            map.end()
        }
    }

    #[test]
    fn duplicate_keys_fail_before_any_value_round_trip_at_every_depth() {
        for bytes in [
            b"{\"same\":1,\"same\":2}\n".as_slice(),
            b"{\"same\":1,\"\\u0073ame\":2}\n".as_slice(),
            b"{\"outer\":{\"same\":1,\"same\":2}}\n".as_slice(),
            b"[{\"same\":1,\"same\":2}]\n".as_slice(),
        ] {
            assert_kind(
                parse_canonical_json_bytes_v1(bytes, CanonicalJsonNullPolicyV1::Forbid),
                CanonicalJsonErrorKindV1::DuplicateObjectKey,
            );
        }
        assert_kind(
            to_canonical_json_bytes_v1(&DuplicateMap, CanonicalJsonNullPolicyV1::Forbid),
            CanonicalJsonErrorKindV1::DuplicateObjectKey,
        );
    }

    enum SwallowedCompoundError {
        DuplicateMapEntry,
        InvalidMapValue,
        InvalidSequenceElement,
        InvalidTupleVariantField,
        InvalidStructVariantField,
        NestedInvalidStructInMap,
    }

    struct SwallowedInnerStructError;

    impl Serialize for SwallowedInnerStructError {
        fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let mut structure = serializer.serialize_struct("Inner", 2)?;
            assert!(structure.serialize_field("invalid", &1.5_f64).is_err());
            assert!(structure.serialize_field("after", &1_u8).is_err());
            structure.end()
        }
    }

    impl Serialize for SwallowedCompoundError {
        fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            match self {
                Self::DuplicateMapEntry => {
                    let mut map = serializer.serialize_map(Some(3))?;
                    map.serialize_entry("same", &1_u8)?;
                    assert!(map.serialize_entry("same", &2_u8).is_err());
                    assert!(map.serialize_entry("after", &3_u8).is_err());
                    map.end()
                }
                Self::InvalidMapValue => {
                    let mut map = serializer.serialize_map(Some(2))?;
                    map.serialize_key("invalid")?;
                    assert!(map.serialize_value(&1.5_f64).is_err());
                    assert!(map.serialize_entry("after", &1_u8).is_err());
                    map.end()
                }
                Self::InvalidSequenceElement => {
                    let mut sequence = serializer.serialize_seq(Some(2))?;
                    assert!(sequence.serialize_element(&1.5_f64).is_err());
                    assert!(sequence.serialize_element(&1_u8).is_err());
                    sequence.end()
                }
                Self::InvalidTupleVariantField => {
                    let mut variant =
                        serializer.serialize_tuple_variant("Fixture", 0, "Tuple", 2)?;
                    assert!(variant.serialize_field(&1.5_f64).is_err());
                    assert!(variant.serialize_field(&1_u8).is_err());
                    variant.end()
                }
                Self::InvalidStructVariantField => {
                    let mut variant =
                        serializer.serialize_struct_variant("Fixture", 0, "Struct", 2)?;
                    assert!(variant.serialize_field("invalid", &1.5_f64).is_err());
                    assert!(variant.serialize_field("after", &1_u8).is_err());
                    variant.end()
                }
                Self::NestedInvalidStructInMap => {
                    let mut map = serializer.serialize_map(Some(2))?;
                    assert!(map
                        .serialize_entry("nested", &SwallowedInnerStructError)
                        .is_err());
                    assert!(map.serialize_entry("after", &1_u8).is_err());
                    map.end()
                }
            }
        }
    }

    #[test]
    fn compound_builders_keep_first_error_when_custom_serialize_tries_to_continue() {
        for (fixture, expected) in [
            (
                SwallowedCompoundError::DuplicateMapEntry,
                CanonicalJsonErrorKindV1::DuplicateObjectKey,
            ),
            (
                SwallowedCompoundError::InvalidMapValue,
                CanonicalJsonErrorKindV1::FloatingPointForbidden,
            ),
            (
                SwallowedCompoundError::InvalidSequenceElement,
                CanonicalJsonErrorKindV1::FloatingPointForbidden,
            ),
            (
                SwallowedCompoundError::InvalidTupleVariantField,
                CanonicalJsonErrorKindV1::FloatingPointForbidden,
            ),
            (
                SwallowedCompoundError::InvalidStructVariantField,
                CanonicalJsonErrorKindV1::FloatingPointForbidden,
            ),
            (
                SwallowedCompoundError::NestedInvalidStructInMap,
                CanonicalJsonErrorKindV1::FloatingPointForbidden,
            ),
        ] {
            assert_emit_count_same_error(&fixture, CanonicalJsonNullPolicyV1::Forbid, expected);
        }
    }

    #[test]
    fn compound_builder_subsequent_calls_and_end_repeat_exact_first_error() {
        let serializer = ValueSerializerV1 {
            null_policy: CanonicalJsonNullPolicyV1::Forbid,
            containing_depth: 0,
        };
        let mut map = serializer.serialize_map(Some(3)).unwrap();
        map.serialize_entry("same", &1_u8).unwrap();
        let map_first = map.serialize_entry("same", &2_u8).unwrap_err();
        assert_eq!(
            map_first.kind(),
            CanonicalJsonErrorKindV1::DuplicateObjectKey
        );
        let map_later = map.serialize_key("after").unwrap_err();
        assert_eq!(map_later, map_first);
        assert_eq!(SerializeMap::end(map).unwrap_err(), map_first);

        let serializer = ValueSerializerV1 {
            null_policy: CanonicalJsonNullPolicyV1::Forbid,
            containing_depth: 0,
        };
        let mut sequence = serializer.serialize_seq(Some(2)).unwrap();
        let sequence_first = SerializeSeq::serialize_element(&mut sequence, &1.5_f64).unwrap_err();
        assert_eq!(
            sequence_first.kind(),
            CanonicalJsonErrorKindV1::FloatingPointForbidden
        );
        let sequence_later = SerializeSeq::serialize_element(&mut sequence, &1_u8).unwrap_err();
        assert_eq!(sequence_later, sequence_first);
        assert_eq!(SerializeSeq::end(sequence).unwrap_err(), sequence_first);
    }

    #[test]
    fn every_json_float_syntax_fails_closed() {
        for bytes in [
            b"0.0\n".as_slice(),
            b"-1.25\n".as_slice(),
            b"1e3\n".as_slice(),
            b"1E-3\n".as_slice(),
            b"-2E+9\n".as_slice(),
        ] {
            assert_kind(
                validate_canonical_json_bytes_v1(bytes, CanonicalJsonNullPolicyV1::Forbid),
                CanonicalJsonErrorKindV1::FloatingPointForbidden,
            );
        }
        assert_kind(
            to_canonical_json_bytes_v1(&1.5_f64, CanonicalJsonNullPolicyV1::Forbid),
            CanonicalJsonErrorKindV1::FloatingPointForbidden,
        );
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_kind(
                to_canonical_json_bytes_v1(&value, CanonicalJsonNullPolicyV1::AllowOnly(&[&[]])),
                CanonicalJsonErrorKindV1::FloatingPointForbidden,
            );
        }
    }

    #[test]
    fn strings_and_keys_must_decode_to_printable_ascii() {
        for bytes in [
            b"\"\\u00e9\"\n".as_slice(),
            b"\"\\u0080\"\n".as_slice(),
            b"\"\\u007f\"\n".as_slice(),
            b"\"\\u001f\"\n".as_slice(),
            b"\"\\n\"\n".as_slice(),
            b"{\"\\u00e9\":1}\n".as_slice(),
            b"\"\xc3\xa9\"\n".as_slice(),
        ] {
            assert_kind(
                validate_canonical_json_bytes_v1(bytes, CanonicalJsonNullPolicyV1::Forbid),
                CanonicalJsonErrorKindV1::NonPrintableAscii,
            );
        }
        assert_kind(
            to_canonical_json_bytes_v1(&"caf\u{e9}", CanonicalJsonNullPolicyV1::Forbid),
            CanonicalJsonErrorKindV1::NonPrintableAscii,
        );
        assert_kind(
            to_canonical_json_bytes_v1(
                &[("caf\u{e9}", 1_u8)]
                    .into_iter()
                    .collect::<BTreeMap<_, _>>(),
                CanonicalJsonNullPolicyV1::Forbid,
            ),
            CanonicalJsonErrorKindV1::NonPrintableAscii,
        );
        for rejected in ['\u{001f}', '\u{007f}', '\u{0080}'] {
            assert_kind(
                to_canonical_json_bytes_v1(
                    &rejected.to_string(),
                    CanonicalJsonNullPolicyV1::Forbid,
                ),
                CanonicalJsonErrorKindV1::NonPrintableAscii,
            );
        }
    }

    #[test]
    fn every_printable_ascii_byte_emits_and_rereads_symmetrically() {
        let printable = (0x20_u8..=0x7e).map(char::from).collect::<String>();
        let bytes = assert_count_matches_bytes(&printable, CanonicalJsonNullPolicyV1::Forbid);
        assert!(bytes.windows(2).any(|pair| pair == b"\\\""));
        assert!(bytes.windows(2).any(|pair| pair == b"\\\\"));
        assert!(bytes.ends_with(b"\"\n"));
        let parsed =
            parse_canonical_json_bytes_v1(&bytes, CanonicalJsonNullPolicyV1::Forbid).unwrap();
        assert_eq!(parsed, serde_json::Value::String(printable));
        assert_eq!(
            to_canonical_json_bytes_v1(&parsed, CanonicalJsonNullPolicyV1::Forbid).unwrap(),
            bytes
        );
    }

    #[test]
    fn null_is_forbidden_by_default_and_allowed_only_at_enumerated_paths() {
        const NULLABLE: &[CanonicalJsonNullPathSegmentV1] =
            &[CanonicalJsonNullPathSegmentV1::ObjectKey("nullable")];
        const NESTED_ARRAY_NULLABLE: &[CanonicalJsonNullPathSegmentV1] = &[
            CanonicalJsonNullPathSegmentV1::ObjectKey("rows"),
            CanonicalJsonNullPathSegmentV1::AnyArrayElement,
            CanonicalJsonNullPathSegmentV1::ObjectKey("nullable"),
        ];
        const PATHS: &[&[CanonicalJsonNullPathSegmentV1]] = &[NULLABLE, NESTED_ARRAY_NULLABLE];
        const POLICY: CanonicalJsonNullPolicyV1 = CanonicalJsonNullPolicyV1::AllowOnly(PATHS);

        let bytes = b"{\"nullable\":null,\"rows\":[{\"nullable\":null},{\"nullable\":null}]}\n";
        assert_kind(
            validate_canonical_json_bytes_v1(bytes, CanonicalJsonNullPolicyV1::Forbid),
            CanonicalJsonErrorKindV1::NullForbidden,
        );
        validate_canonical_json_bytes_v1(bytes, POLICY).unwrap();
        assert_eq!(
            to_canonical_json_bytes_v1(
                &serde_json::json!({
                    "nullable": null,
                    "rows": [{"nullable": null}, {"nullable": null}]
                }),
                POLICY,
            )
            .unwrap(),
            bytes
        );
        for forbidden in [
            b"null\n".as_slice(),
            b"{\"other\":null}\n".as_slice(),
            b"{\"rows\":[null]}\n".as_slice(),
            b"{\"rows\":[{\"other\":null}]}\n".as_slice(),
        ] {
            assert_kind(
                validate_canonical_json_bytes_v1(forbidden, POLICY),
                CanonicalJsonErrorKindV1::NullForbidden,
            );
        }

        const SECOND_ONLY: &[CanonicalJsonNullPathSegmentV1] = &[
            CanonicalJsonNullPathSegmentV1::ObjectKey("rows"),
            CanonicalJsonNullPathSegmentV1::ArrayIndex(1),
        ];
        const SECOND_POLICY: CanonicalJsonNullPolicyV1 =
            CanonicalJsonNullPolicyV1::AllowOnly(&[SECOND_ONLY]);
        validate_canonical_json_bytes_v1(b"{\"rows\":[0,null]}\n", SECOND_POLICY).unwrap();
        assert_kind(
            validate_canonical_json_bytes_v1(b"{\"rows\":[null,0]}\n", SECOND_POLICY),
            CanonicalJsonErrorKindV1::NullForbidden,
        );

        const ROOT_POLICY: CanonicalJsonNullPolicyV1 = CanonicalJsonNullPolicyV1::AllowOnly(&[&[]]);
        validate_canonical_json_bytes_v1(b"null\n", ROOT_POLICY).unwrap();
    }

    fn nested_array(depth: usize) -> Vec<u8> {
        let mut bytes = vec![b'['; depth];
        bytes.push(b'0');
        bytes.extend(std::iter::repeat_n(b']', depth));
        bytes.push(b'\n');
        bytes
    }

    fn nested_object(depth: usize) -> Vec<u8> {
        let mut bytes = Vec::new();
        for _ in 0..depth {
            bytes.extend_from_slice(b"{\"a\":");
        }
        bytes.push(b'0');
        bytes.extend(std::iter::repeat_n(b'}', depth));
        bytes.push(b'\n');
        bytes
    }

    #[test]
    fn depth_bound_counts_root_container_as_one() {
        for accepted in [
            nested_array(CANONICAL_JSON_MAX_DEPTH_V1),
            nested_object(CANONICAL_JSON_MAX_DEPTH_V1),
        ] {
            validate_canonical_json_bytes_v1(&accepted, CanonicalJsonNullPolicyV1::Forbid).unwrap();
            let accepted_value =
                parse_canonical_json_bytes_v1(&accepted, CanonicalJsonNullPolicyV1::Forbid)
                    .unwrap();
            assert_eq!(
                to_canonical_json_bytes_v1(&accepted_value, CanonicalJsonNullPolicyV1::Forbid,)
                    .unwrap(),
                accepted
            );
        }
        for rejected in [
            nested_array(CANONICAL_JSON_MAX_DEPTH_V1 + 1),
            nested_object(CANONICAL_JSON_MAX_DEPTH_V1 + 1),
        ] {
            assert_kind(
                validate_canonical_json_bytes_v1(&rejected, CanonicalJsonNullPolicyV1::Forbid),
                CanonicalJsonErrorKindV1::DepthTooDeep,
            );
        }

        for use_object in [false, true] {
            let mut rejected_value = serde_json::Value::from(0);
            for _ in 0..=CANONICAL_JSON_MAX_DEPTH_V1 {
                rejected_value = if use_object {
                    serde_json::json!({"a": rejected_value})
                } else {
                    serde_json::Value::Array(vec![rejected_value])
                };
            }
            assert_kind(
                to_canonical_json_bytes_v1(&rejected_value, CanonicalJsonNullPolicyV1::Forbid),
                CanonicalJsonErrorKindV1::DepthTooDeep,
            );
        }
    }

    #[test]
    fn decoded_string_and_key_bound_is_inclusive() {
        let accepted = "a".repeat(CANONICAL_JSON_MAX_STRING_BYTES_V1);
        let accepted_bytes =
            to_canonical_json_bytes_v1(&accepted, CanonicalJsonNullPolicyV1::Forbid).unwrap();
        validate_canonical_json_bytes_v1(&accepted_bytes, CanonicalJsonNullPolicyV1::Forbid)
            .unwrap();

        let rejected = "a".repeat(CANONICAL_JSON_MAX_STRING_BYTES_V1 + 1);
        assert_kind(
            to_canonical_json_bytes_v1(&rejected, CanonicalJsonNullPolicyV1::Forbid),
            CanonicalJsonErrorKindV1::StringTooLong,
        );

        let escaped_at_limit = format!("\"{}\"\n", "\\u0041".repeat(4096));
        assert_kind(
            validate_canonical_json_bytes_v1(
                escaped_at_limit.as_bytes(),
                CanonicalJsonNullPolicyV1::Forbid,
            ),
            CanonicalJsonErrorKindV1::NonCanonicalBytes,
        );
        let escaped_over_limit = format!("\"{}\"\n", "\\u0041".repeat(4097));
        assert_kind(
            validate_canonical_json_bytes_v1(
                escaped_over_limit.as_bytes(),
                CanonicalJsonNullPolicyV1::Forbid,
            ),
            CanonicalJsonErrorKindV1::StringTooLong,
        );

        let mut object = BTreeMap::new();
        object.insert(rejected, 1_u8);
        assert_kind(
            to_canonical_json_bytes_v1(&object, CanonicalJsonNullPolicyV1::Forbid),
            CanonicalJsonErrorKindV1::StringTooLong,
        );
    }

    fn bounded_object(keys: usize) -> BTreeMap<String, u64> {
        (0..keys)
            .map(|index| (format!("k{index:03}"), index as u64))
            .collect()
    }

    #[test]
    fn byte_and_count_paths_match_at_null_depth_string_and_object_boundaries() {
        const NULLABLE: &[CanonicalJsonNullPathSegmentV1] = &[
            CanonicalJsonNullPathSegmentV1::ObjectKey("rows"),
            CanonicalJsonNullPathSegmentV1::AnyArrayElement,
            CanonicalJsonNullPathSegmentV1::ObjectKey("nullable"),
        ];
        const POLICY: CanonicalJsonNullPolicyV1 = CanonicalJsonNullPolicyV1::AllowOnly(&[NULLABLE]);

        let allowed_nulls = serde_json::json!({
            "rows": [{"nullable": null}, {"nullable": null}],
        });
        assert_count_matches_bytes(&allowed_nulls, POLICY);
        assert_emit_count_same_error(
            &serde_json::json!({"rows": [{"other": null}]}),
            POLICY,
            CanonicalJsonErrorKindV1::NullForbidden,
        );

        for use_object in [false, true] {
            let mut boundary = serde_json::Value::from(0);
            for _ in 0..CANONICAL_JSON_MAX_DEPTH_V1 {
                boundary = if use_object {
                    serde_json::json!({"a": boundary})
                } else {
                    serde_json::Value::Array(vec![boundary])
                };
            }
            assert_count_matches_bytes(&boundary, CanonicalJsonNullPolicyV1::Forbid);

            let too_deep = if use_object {
                serde_json::json!({"a": boundary})
            } else {
                serde_json::Value::Array(vec![boundary])
            };
            assert_emit_count_same_error(
                &too_deep,
                CanonicalJsonNullPolicyV1::Forbid,
                CanonicalJsonErrorKindV1::DepthTooDeep,
            );
        }

        let escaped_at_string_bound = "\"\\".repeat(CANONICAL_JSON_MAX_STRING_BYTES_V1 / 2);
        assert_eq!(
            escaped_at_string_bound.len(),
            CANONICAL_JSON_MAX_STRING_BYTES_V1
        );
        assert_count_matches_bytes(&escaped_at_string_bound, CanonicalJsonNullPolicyV1::Forbid);
        let escaped_over_string_bound = format!("{escaped_at_string_bound}x");
        assert_emit_count_same_error(
            &escaped_over_string_bound,
            CanonicalJsonNullPolicyV1::Forbid,
            CanonicalJsonErrorKindV1::StringTooLong,
        );

        let mut key_at_string_bound = BTreeMap::new();
        key_at_string_bound.insert(escaped_at_string_bound, 1_u8);
        assert_count_matches_bytes(&key_at_string_bound, CanonicalJsonNullPolicyV1::Forbid);
        let mut key_over_string_bound = BTreeMap::new();
        key_over_string_bound.insert(escaped_over_string_bound, 1_u8);
        assert_emit_count_same_error(
            &key_over_string_bound,
            CanonicalJsonNullPolicyV1::Forbid,
            CanonicalJsonErrorKindV1::StringTooLong,
        );

        assert_count_matches_bytes(
            &bounded_object(CANONICAL_JSON_MAX_OBJECT_KEYS_V1),
            CanonicalJsonNullPolicyV1::Forbid,
        );
        assert_emit_count_same_error(
            &bounded_object(CANONICAL_JSON_MAX_OBJECT_KEYS_V1 + 1),
            CanonicalJsonNullPolicyV1::Forbid,
            CanonicalJsonErrorKindV1::ObjectTooLarge,
        );
    }

    #[test]
    fn object_key_count_bound_is_inclusive_at_every_depth() {
        let accepted = bounded_object(CANONICAL_JSON_MAX_OBJECT_KEYS_V1);
        let accepted_bytes =
            to_canonical_json_bytes_v1(&accepted, CanonicalJsonNullPolicyV1::Forbid).unwrap();
        validate_canonical_json_bytes_v1(&accepted_bytes, CanonicalJsonNullPolicyV1::Forbid)
            .unwrap();

        let rejected = bounded_object(CANONICAL_JSON_MAX_OBJECT_KEYS_V1 + 1);
        let mut rejected_bytes = serde_json::to_vec(&rejected).unwrap();
        rejected_bytes.push(b'\n');
        assert_kind(
            validate_canonical_json_bytes_v1(&rejected_bytes, CanonicalJsonNullPolicyV1::Forbid),
            CanonicalJsonErrorKindV1::ObjectTooLarge,
        );
        assert_kind(
            to_canonical_json_bytes_v1(&rejected, CanonicalJsonNullPolicyV1::Forbid),
            CanonicalJsonErrorKindV1::ObjectTooLarge,
        );
        let nested = serde_json::json!({"outer": rejected});
        assert_kind(
            to_canonical_json_bytes_v1(&nested, CanonicalJsonNullPolicyV1::Forbid),
            CanonicalJsonErrorKindV1::ObjectTooLarge,
        );
    }

    #[test]
    fn readers_require_exactly_one_final_lf_and_no_trailing_value_bytes() {
        assert_kind(
            validate_canonical_json_bytes_v1(b"{}", CanonicalJsonNullPolicyV1::Forbid),
            CanonicalJsonErrorKindV1::MissingFinalLf,
        );
        assert_kind(
            validate_canonical_json_bytes_v1(b"{}\n{}\n", CanonicalJsonNullPolicyV1::Forbid),
            CanonicalJsonErrorKindV1::TrailingBytes,
        );
        assert_kind(
            validate_canonical_json_bytes_v1(
                b"\xef\xbb\xbf{}\n",
                CanonicalJsonNullPolicyV1::Forbid,
            ),
            CanonicalJsonErrorKindV1::InvalidSyntax,
        );
        for bytes in [
            b"{}\n\n".as_slice(),
            b"{}\r\n".as_slice(),
            b" {}\n".as_slice(),
            b"{} \n".as_slice(),
        ] {
            assert_kind(
                validate_canonical_json_bytes_v1(bytes, CanonicalJsonNullPolicyV1::Forbid),
                CanonicalJsonErrorKindV1::NonCanonicalBytes,
            );
        }
    }

    #[test]
    fn valid_but_noncanonical_order_number_and_escapes_are_rejected() {
        for bytes in [
            b"{\"z\":1,\"a\":2}\n".as_slice(),
            b"-0\n".as_slice(),
            b"\"\\/\"\n".as_slice(),
            b"\"\\u0041\"\n".as_slice(),
        ] {
            assert_kind(
                validate_canonical_json_bytes_v1(bytes, CanonicalJsonNullPolicyV1::Forbid),
                CanonicalJsonErrorKindV1::NonCanonicalBytes,
            );
        }
    }

    #[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
    #[serde(deny_unknown_fields)]
    struct TypedFixture {
        a: i64,
        b: Vec<bool>,
    }

    #[test]
    fn canonical_reread_and_typed_decode_preserve_exact_bytes_and_values() {
        let fixture = TypedFixture {
            a: -17,
            b: vec![true, false],
        };
        let bytes =
            to_canonical_json_bytes_v1(&fixture, CanonicalJsonNullPolicyV1::Forbid).unwrap();
        assert_eq!(bytes, b"{\"a\":-17,\"b\":[true,false]}\n");

        let parsed =
            parse_canonical_json_bytes_v1(&bytes, CanonicalJsonNullPolicyV1::Forbid).unwrap();
        let reencoded =
            to_canonical_json_bytes_v1(&parsed, CanonicalJsonNullPolicyV1::Forbid).unwrap();
        assert_eq!(reencoded, bytes);

        let decoded: TypedFixture =
            from_canonical_json_bytes_v1(&bytes, CanonicalJsonNullPolicyV1::Forbid).unwrap();
        assert_eq!(decoded, fixture);
    }

    #[test]
    fn syntax_and_typed_errors_expose_only_stable_privacy_safe_codes() {
        let syntax = validate_canonical_json_bytes_v1(
            b"{\"secret-token\":]\n",
            CanonicalJsonNullPolicyV1::Forbid,
        )
        .unwrap_err();
        assert_eq!(syntax.kind(), CanonicalJsonErrorKindV1::InvalidSyntax);
        assert_eq!(syntax.to_string(), "canonical_json_invalid_syntax");
        assert!(!syntax.to_string().contains("secret-token"));
        assert!(syntax.source().is_none());

        let typed = from_canonical_json_bytes_v1::<TypedFixture>(
            b"{\"a\":1,\"b\":[],\"secret-token\":1}\n",
            CanonicalJsonNullPolicyV1::Forbid,
        )
        .unwrap_err();
        assert_eq!(typed.kind(), CanonicalJsonErrorKindV1::Deserialization);
        assert_eq!(typed.to_string(), "canonical_json_deserialization");
        assert!(!typed.to_string().contains("secret-token"));
    }

    struct SecretSerializationFailure;

    impl Serialize for SecretSerializationFailure {
        fn serialize<S>(&self, _serializer: S) -> std::result::Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            Err(<S::Error as serde::ser::Error>::custom(
                "secret-token-raw-content",
            ))
        }
    }

    #[test]
    fn every_error_code_is_stable_unique_and_input_free() {
        let cases = [
            (
                CanonicalJsonErrorKindV1::Serialization,
                "canonical_json_serialization",
            ),
            (
                CanonicalJsonErrorKindV1::EncodedLengthOverflow,
                "canonical_json_encoded_length_overflow",
            ),
            (
                CanonicalJsonErrorKindV1::Deserialization,
                "canonical_json_deserialization",
            ),
            (
                CanonicalJsonErrorKindV1::InvalidSyntax,
                "canonical_json_invalid_syntax",
            ),
            (
                CanonicalJsonErrorKindV1::MissingFinalLf,
                "canonical_json_missing_final_lf",
            ),
            (
                CanonicalJsonErrorKindV1::TrailingBytes,
                "canonical_json_trailing_bytes",
            ),
            (
                CanonicalJsonErrorKindV1::NonCanonicalBytes,
                "canonical_json_noncanonical_bytes",
            ),
            (
                CanonicalJsonErrorKindV1::DuplicateObjectKey,
                "canonical_json_duplicate_object_key",
            ),
            (
                CanonicalJsonErrorKindV1::FloatingPointForbidden,
                "canonical_json_floating_point_forbidden",
            ),
            (
                CanonicalJsonErrorKindV1::IntegerOutOfRange,
                "canonical_json_integer_out_of_range",
            ),
            (
                CanonicalJsonErrorKindV1::NullForbidden,
                "canonical_json_null_forbidden",
            ),
            (
                CanonicalJsonErrorKindV1::NonPrintableAscii,
                "canonical_json_non_printable_ascii",
            ),
            (
                CanonicalJsonErrorKindV1::StringTooLong,
                "canonical_json_string_too_long",
            ),
            (
                CanonicalJsonErrorKindV1::ObjectTooLarge,
                "canonical_json_object_too_large",
            ),
            (
                CanonicalJsonErrorKindV1::DepthTooDeep,
                "canonical_json_depth_too_deep",
            ),
        ];
        let mut codes = std::collections::BTreeSet::new();
        for (kind, expected) in cases {
            assert_eq!(kind.code(), expected);
            assert!(codes.insert(kind.code()));
        }

        let error = to_canonical_json_bytes_v1(
            &SecretSerializationFailure,
            CanonicalJsonNullPolicyV1::Forbid,
        )
        .unwrap_err();
        assert_eq!(error.kind(), CanonicalJsonErrorKindV1::Serialization);
        assert_eq!(error.to_string(), "canonical_json_serialization");
        assert!(!format!("{error:?}").contains("secret-token-raw-content"));
    }
}
