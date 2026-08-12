use crate::MtgoContractErrorV1;
use serde::Serialize;
use sha2::{Digest, Sha256};

pub const MTGO_VISIBLE_GAME_LOG_FILE_PROJECTION_SCHEMA_V1: u32 = 1;

const MTGO_VISIBLE_GAME_LOG_FILE_VERSION_V1: u16 = 1;
const MTGO_VISIBLE_GAME_LOG_CHANNEL_V1: u16 = 4;
const MAX_VISIBLE_GAME_LOG_FILE_BYTES_V1: usize = 16 * 1_048_576;
const MAX_VISIBLE_GAME_LOG_RECORDS_V1: usize = 65_536;
const MAX_VISIBLE_GAME_LOG_TEXT_BYTES_V1: usize = 65_536;
const MAX_DOTNET_DATETIME_TICKS_V1: u64 = 3_155_378_975_999_999_999;
const VISIBLE_GAME_LOG_SOURCE_ID_DOMAIN_V1: &[u8] = b"mtgo-visible-game-log-source-id-v1";
const VISIBLE_GAME_LOG_RECORD_DOMAIN_V1: &[u8] = b"mtgo-visible-game-log-record-v1";
const VISIBLE_GAME_LOG_PROJECTION_DOMAIN_V1: &[u8] = b"mtgo-visible-game-log-file-projection-v1";

#[derive(Serialize)]
struct MtgoVisibleGameLogProjectedRecordV1 {
    sequence: u32,
    visible_text: String,
    visible_text_sha256: String,
    source_record_commitment_sha256: String,
}

/// Read-only text projected from one persisted MTGO Game Log record.
///
/// The timestamp, source match UUID, record flags, card-link markup, and
/// numeric card or object identifiers are intentionally unavailable.
pub struct MtgoVisibleGameLogTextViewV1<'a> {
    record: &'a MtgoVisibleGameLogProjectedRecordV1,
}

impl<'a> MtgoVisibleGameLogTextViewV1<'a> {
    pub fn sequence_v1(&self) -> u32 {
        self.record.sequence
    }

    pub fn visible_text_v1(&self) -> &str {
        &self.record.visible_text
    }

    pub fn visible_text_sha256_v1(&self) -> &str {
        &self.record.visible_text_sha256
    }

    pub fn source_record_commitment_sha256_v1(&self) -> &str {
        &self.record.source_record_commitment_sha256
    }
}

/// Move-only rendering projection of one MTGO `Match_GameLog_*.dat` payload.
///
/// V1 accepts only the exact framing observed in the local corpus: file
/// version 1, channel 4, two identical canonical match UUID strings, ordered
/// timestamped records with a zero flag, player-link markers, and card links
/// whose hidden numeric metadata is discarded. Unknown framing or markup
/// rejects instead of reaching the projection.
///
/// This type proves only that the retained strings contain the same textual
/// payload that MTGO can render in its Game Log. It does not prove that the
/// file belongs to the current window or game. A later live source binder must
/// establish that lineage before the text becomes semantic evidence.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoVisibleGameLogProjectionV1;
/// fn cannot_act(value: &CheckedUntrustedMtgoVisibleGameLogProjectionV1) {
///     let _ = value.score_model();
///     let _ = value.input_command();
/// }
/// ```
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoVisibleGameLogProjectionV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<CheckedUntrustedMtgoVisibleGameLogProjectionV1>();
/// ```
pub struct CheckedUntrustedMtgoVisibleGameLogProjectionV1 {
    records: Vec<MtgoVisibleGameLogProjectedRecordV1>,
    source_match_id_commitment_sha256: String,
    source_file_sha256: String,
    projection_commitment_sha256: String,
}

impl CheckedUntrustedMtgoVisibleGameLogProjectionV1 {
    pub fn record_count_v1(&self) -> usize {
        self.records.len()
    }

    pub fn record_v1(&self, index: usize) -> Option<MtgoVisibleGameLogTextViewV1<'_>> {
        self.records
            .get(index)
            .map(|record| MtgoVisibleGameLogTextViewV1 { record })
    }

    pub fn source_match_id_commitment_sha256_v1(&self) -> &str {
        &self.source_match_id_commitment_sha256
    }

    pub fn source_file_sha256_v1(&self) -> &str {
        &self.source_file_sha256
    }

    pub fn projection_commitment_sha256_v1(&self) -> &str {
        &self.projection_commitment_sha256
    }

    pub fn nonrendered_source_metadata_discarded_v1(&self) -> bool {
        true
    }

    pub fn safe_for_current_game_semantic_evidence_v1(&self) -> bool {
        false
    }

    pub fn safe_for_model_scoring_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

pub fn parse_checked_untrusted_mtgo_visible_game_log_v1(
    bytes: &[u8],
) -> Result<CheckedUntrustedMtgoVisibleGameLogProjectionV1, MtgoContractErrorV1> {
    if bytes.is_empty() || bytes.len() > MAX_VISIBLE_GAME_LOG_FILE_BYTES_V1 {
        return Err(error_v1(
            "visible_game_log_file_size",
            "the persisted Game Log file is empty or exceeds the fixed byte bound",
        ));
    }
    let mut reader = ByteReaderV1::new(bytes);
    if reader.read_u16_v1()? != MTGO_VISIBLE_GAME_LOG_FILE_VERSION_V1 {
        return Err(error_v1(
            "visible_game_log_file_version",
            "only persisted Game Log file version 1 is accepted",
        ));
    }
    let first_source_id = reader.read_dotnet_string_v1(64)?;
    if !valid_source_match_id_v1(&first_source_id) {
        return Err(error_v1(
            "visible_game_log_source_id",
            "the persisted Game Log source identifier is not canonical",
        ));
    }
    if reader.read_u16_v1()? != MTGO_VISIBLE_GAME_LOG_CHANNEL_V1 {
        return Err(error_v1(
            "visible_game_log_channel",
            "only the persisted player-visible Game Log channel is accepted",
        ));
    }
    let second_source_id = reader.read_dotnet_string_v1(64)?;
    if first_source_id != second_source_id {
        return Err(error_v1(
            "visible_game_log_source_id_mismatch",
            "the two persisted Game Log source identifiers differ",
        ));
    }

    let mut records = Vec::new();
    let mut previous_ticks = None;
    while !reader.is_finished_v1() {
        if records.len() >= MAX_VISIBLE_GAME_LOG_RECORDS_V1 {
            return Err(error_v1(
                "visible_game_log_record_limit",
                "the persisted Game Log exceeds the fixed record bound",
            ));
        }
        let ticks = reader.read_u64_v1()?;
        if ticks == 0
            || ticks > MAX_DOTNET_DATETIME_TICKS_V1
            || previous_ticks.is_some_and(|prior| ticks < prior)
        {
            return Err(error_v1(
                "visible_game_log_record_time",
                "Game Log record timestamps must be valid and nondecreasing",
            ));
        }
        previous_ticks = Some(ticks);
        let flag = reader.read_u8_v1()?;
        if flag != 0 {
            return Err(error_v1(
                "visible_game_log_record_flag",
                "unknown Game Log record flags are not accepted",
            ));
        }
        let raw_text = reader.read_dotnet_string_v1(MAX_VISIBLE_GAME_LOG_TEXT_BYTES_V1)?;
        let visible_text = project_visible_text_v1(&raw_text)?;
        let sequence = u32::try_from(records.len() + 1).map_err(|_| {
            error_v1(
                "visible_game_log_record_sequence",
                "Game Log record sequence overflow",
            )
        })?;
        let visible_text_sha256 = sha256_hex_v1(visible_text.as_bytes());
        let source_record_commitment_sha256 = commitment_v1(
            VISIBLE_GAME_LOG_RECORD_DOMAIN_V1,
            &[
                &ticks.to_le_bytes(),
                &[flag],
                raw_text.as_bytes(),
                visible_text.as_bytes(),
            ],
        );
        records.push(MtgoVisibleGameLogProjectedRecordV1 {
            sequence,
            visible_text,
            visible_text_sha256,
            source_record_commitment_sha256,
        });
    }
    if records.is_empty() {
        return Err(error_v1(
            "visible_game_log_record_empty",
            "the persisted Game Log contains no visible records",
        ));
    }

    let source_match_id_commitment_sha256 = commitment_v1(
        VISIBLE_GAME_LOG_SOURCE_ID_DOMAIN_V1,
        &[first_source_id.as_bytes()],
    );
    let source_file_sha256 = sha256_hex_v1(bytes);
    let projected_json = serde_json::to_vec(&records).map_err(|error| {
        error_v1(
            "visible_game_log_projection_serialization",
            error.to_string(),
        )
    })?;
    let projection_commitment_sha256 = commitment_v1(
        VISIBLE_GAME_LOG_PROJECTION_DOMAIN_V1,
        &[
            &MTGO_VISIBLE_GAME_LOG_FILE_PROJECTION_SCHEMA_V1.to_be_bytes(),
            source_match_id_commitment_sha256.as_bytes(),
            source_file_sha256.as_bytes(),
            &projected_json,
        ],
    );
    Ok(CheckedUntrustedMtgoVisibleGameLogProjectionV1 {
        records,
        source_match_id_commitment_sha256,
        source_file_sha256,
        projection_commitment_sha256,
    })
}

fn project_visible_text_v1(raw: &str) -> Result<String, MtgoContractErrorV1> {
    if raw.is_empty()
        || raw
            .chars()
            .any(|value| value.is_control() && !matches!(value, '\t' | '\r' | '\n'))
    {
        return Err(error_v1(
            "visible_game_log_record_text",
            "Game Log text is empty or contains a non-renderable control character",
        ));
    }
    let mut projected = String::with_capacity(raw.len());
    let mut cursor = 0;
    while cursor < raw.len() {
        let remainder = &raw[cursor..];
        if remainder.starts_with("@P") {
            cursor += 2;
            continue;
        }
        if remainder.starts_with("@[") {
            let close_offset = remainder.find("@]").ok_or_else(|| {
                error_v1(
                    "visible_game_log_card_link",
                    "Game Log card-link markup is unterminated",
                )
            })?;
            let body = &remainder[2..close_offset];
            let metadata_offset = body.find("@:").ok_or_else(|| {
                error_v1(
                    "visible_game_log_card_link",
                    "Game Log card-link markup lacks its metadata delimiter",
                )
            })?;
            let visible_name = &body[..metadata_offset];
            let hidden_metadata = &body[metadata_offset + 2..];
            if !valid_visible_card_name_v1(visible_name)
                || !valid_hidden_card_link_metadata_v1(hidden_metadata)
            {
                return Err(error_v1(
                    "visible_game_log_card_link",
                    "Game Log card-link markup is outside the admitted grammar",
                ));
            }
            projected.push_str(visible_name);
            cursor += close_offset + 2;
            continue;
        }
        let character = remainder.chars().next().expect("nonempty remainder");
        if character == '@' {
            return Err(error_v1(
                "visible_game_log_markup",
                "unknown Game Log markup is not accepted",
            ));
        }
        projected.push(character);
        cursor += character.len_utf8();
        if projected.len() > MAX_VISIBLE_GAME_LOG_TEXT_BYTES_V1 {
            return Err(error_v1(
                "visible_game_log_projected_text_limit",
                "projected Game Log text exceeds the fixed byte bound",
            ));
        }
    }
    if projected.is_empty() {
        return Err(error_v1(
            "visible_game_log_projected_text_empty",
            "Game Log markup produced no visible text",
        ));
    }
    Ok(projected)
}

fn valid_source_match_id_v1(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 36
        && bytes.iter().copied().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_digit() || matches!(byte, b'a'..=b'f')
            }
        })
}

fn valid_visible_card_name_v1(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 512
        && !value.contains('@')
        && !value
            .chars()
            .any(|character| character.is_control() || matches!(character, '[' | ']'))
}

fn valid_hidden_card_link_metadata_v1(value: &str) -> bool {
    let Some(without_suffix) = value.strip_suffix(':') else {
        return false;
    };
    let mut pieces = without_suffix.split(',');
    let first = pieces.next();
    let second = pieces.next();
    pieces.next().is_none()
        && first.is_some_and(valid_positive_decimal_v1)
        && second.is_some_and(valid_positive_decimal_v1)
}

fn valid_positive_decimal_v1(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 20
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && value != "0"
}

struct ByteReaderV1<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> ByteReaderV1<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    fn is_finished_v1(&self) -> bool {
        self.cursor == self.bytes.len()
    }

    fn read_u8_v1(&mut self) -> Result<u8, MtgoContractErrorV1> {
        Ok(self.read_exact_v1(1)?[0])
    }

    fn read_u16_v1(&mut self) -> Result<u16, MtgoContractErrorV1> {
        let bytes: [u8; 2] = self.read_exact_v1(2)?.try_into().expect("two-byte slice");
        Ok(u16::from_le_bytes(bytes))
    }

    fn read_u64_v1(&mut self) -> Result<u64, MtgoContractErrorV1> {
        let bytes: [u8; 8] = self.read_exact_v1(8)?.try_into().expect("eight-byte slice");
        Ok(u64::from_le_bytes(bytes))
    }

    fn read_dotnet_string_v1(
        &mut self,
        maximum_bytes: usize,
    ) -> Result<String, MtgoContractErrorV1> {
        let length = self.read_7bit_u32_v1()?;
        let length = usize::try_from(length).map_err(|_| {
            error_v1(
                "visible_game_log_string_length",
                "Game Log string length does not fit this process",
            )
        })?;
        if length == 0 || length > maximum_bytes {
            return Err(error_v1(
                "visible_game_log_string_length",
                "Game Log string is empty or exceeds its fixed byte bound",
            ));
        }
        let bytes = self.read_exact_v1(length)?;
        std::str::from_utf8(bytes).map(str::to_owned).map_err(|_| {
            error_v1(
                "visible_game_log_utf8",
                "Game Log string is not valid UTF-8",
            )
        })
    }

    fn read_7bit_u32_v1(&mut self) -> Result<u32, MtgoContractErrorV1> {
        let mut value = 0_u32;
        for index in 0..5 {
            let byte = self.read_u8_v1()?;
            if index == 4 && byte > 0x0f {
                return Err(error_v1(
                    "visible_game_log_string_length",
                    "Game Log 7-bit string length overflows u32",
                ));
            }
            value |= u32::from(byte & 0x7f) << (index * 7);
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(error_v1(
            "visible_game_log_string_length",
            "Game Log 7-bit string length is unterminated",
        ))
    }

    fn read_exact_v1(&mut self, length: usize) -> Result<&'a [u8], MtgoContractErrorV1> {
        let end = self.cursor.checked_add(length).ok_or_else(|| {
            error_v1(
                "visible_game_log_truncated",
                "Game Log byte range overflows",
            )
        })?;
        let value = self.bytes.get(self.cursor..end).ok_or_else(|| {
            error_v1(
                "visible_game_log_truncated",
                "Game Log file ends inside a framed value",
            )
        })?;
        self.cursor = end;
        Ok(value)
    }
}

fn sha256_hex_v1(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn commitment_v1(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update((domain.len() as u64).to_be_bytes());
    hasher.update(domain);
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    format!("{:x}", hasher.finalize())
}

fn error_v1(code: &'static str, detail: impl Into<String>) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, detail)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE_ID_V1: &str = "82ba951e-fe1f-423a-9cdb-bfe879ea2c6b";

    fn write_dotnet_string_v1(bytes: &mut Vec<u8>, value: &str) {
        let mut length = u32::try_from(value.len()).unwrap();
        while length >= 0x80 {
            bytes.push((length as u8) | 0x80);
            length >>= 7;
        }
        bytes.push(length as u8);
        bytes.extend_from_slice(value.as_bytes());
    }

    fn file_v1(records: &[(u64, u8, &str)]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        write_dotnet_string_v1(&mut bytes, SOURCE_ID_V1);
        bytes.extend_from_slice(&4_u16.to_le_bytes());
        write_dotnet_string_v1(&mut bytes, SOURCE_ID_V1);
        for (ticks, flag, text) in records {
            bytes.extend_from_slice(&ticks.to_le_bytes());
            bytes.push(*flag);
            write_dotnet_string_v1(&mut bytes, text);
        }
        bytes
    }

    #[test]
    fn current_one_line_sample_projects_exact_painted_text() {
        let bytes = file_v1(&[(
            638_906_000_000_000_000,
            0,
            "@P@PUnbuckledPie joined the game.",
        )]);
        let parsed = parse_checked_untrusted_mtgo_visible_game_log_v1(&bytes).unwrap();
        assert_eq!(parsed.record_count_v1(), 1);
        assert_eq!(
            parsed.record_v1(0).unwrap().visible_text_v1(),
            "UnbuckledPie joined the game."
        );
        assert!(parsed.nonrendered_source_metadata_discarded_v1());
        assert!(!parsed.safe_for_current_game_semantic_evidence_v1());
        assert!(!parsed.safe_for_model_scoring_v1());
        assert!(!parsed.safe_for_input_v1());
    }

    #[test]
    fn card_links_keep_names_and_discard_numeric_metadata() {
        let bytes = file_v1(&[
            (
                638_905_999_999_999_999,
                0,
                "@PAlice casts @[Stock Up@:296710,467:@] from the graveyard.",
            ),
            (
                638_906_000_000_000_000,
                0,
                "@PBob plays @[Blood Crypt@:93010,477:@].",
            ),
        ]);
        let parsed = parse_checked_untrusted_mtgo_visible_game_log_v1(&bytes).unwrap();
        assert_eq!(
            parsed.record_v1(0).unwrap().visible_text_v1(),
            "Alice casts Stock Up from the graveyard."
        );
        assert_eq!(
            parsed.record_v1(1).unwrap().visible_text_v1(),
            "Bob plays Blood Crypt."
        );
        for index in 0..parsed.record_count_v1() {
            let record = parsed.record_v1(index).unwrap();
            let text = record.visible_text_v1();
            assert!(!text.contains("296710"));
            assert!(!text.contains("467"));
            assert!(!text.contains('@'));
        }
    }

    #[test]
    fn unknown_markup_or_card_link_shape_rejects() {
        for text in [
            "@Xhidden marker",
            "@PAlice casts @[Stock Up@:296710,467.",
            "@PAlice casts @[Stock Up@:296710:@].",
            "@PAlice casts @[Stock Up@:0,467:@].",
            "@PAlice casts @[@:296710,467:@].",
        ] {
            let error = parse_checked_untrusted_mtgo_visible_game_log_v1(&file_v1(&[(
                638_905_999_999_999_999,
                0,
                text,
            )]))
            .err()
            .unwrap();
            assert!(matches!(
                error.code(),
                "visible_game_log_markup" | "visible_game_log_card_link"
            ));
        }
    }

    #[test]
    fn version_channel_flag_and_source_identity_are_exact() {
        let baseline = file_v1(&[(638_905_999_999_999_999, 0, "@PAlice draws a card.")]);
        let mut version = baseline.clone();
        version[0] = 2;
        assert_eq!(
            parse_checked_untrusted_mtgo_visible_game_log_v1(&version)
                .err()
                .unwrap()
                .code(),
            "visible_game_log_file_version"
        );
        let mut channel = baseline.clone();
        channel[39] = 5;
        assert_eq!(
            parse_checked_untrusted_mtgo_visible_game_log_v1(&channel)
                .err()
                .unwrap()
                .code(),
            "visible_game_log_channel"
        );
        let mut second_id = baseline.clone();
        second_id[42] = b'9';
        assert_eq!(
            parse_checked_untrusted_mtgo_visible_game_log_v1(&second_id)
                .err()
                .unwrap()
                .code(),
            "visible_game_log_source_id_mismatch"
        );
        let mut flag = baseline;
        flag[86] = 1;
        assert_eq!(
            parse_checked_untrusted_mtgo_visible_game_log_v1(&flag)
                .err()
                .unwrap()
                .code(),
            "visible_game_log_record_flag"
        );
    }

    #[test]
    fn timestamps_must_be_valid_and_nondecreasing() {
        for records in [
            vec![(0, 0, "first")],
            vec![(MAX_DOTNET_DATETIME_TICKS_V1 + 1, 0, "first")],
            vec![(2, 0, "first"), (1, 0, "second")],
        ] {
            assert_eq!(
                parse_checked_untrusted_mtgo_visible_game_log_v1(&file_v1(&records))
                    .err()
                    .unwrap()
                    .code(),
                "visible_game_log_record_time"
            );
        }
    }

    #[test]
    fn truncated_or_invalid_utf8_payloads_reject() {
        let mut truncated = file_v1(&[(638_905_999_999_999_999, 0, "visible")]);
        truncated.pop();
        assert_eq!(
            parse_checked_untrusted_mtgo_visible_game_log_v1(&truncated)
                .err()
                .unwrap()
                .code(),
            "visible_game_log_truncated"
        );

        let mut invalid_utf8 = file_v1(&[(638_905_999_999_999_999, 0, "visible")]);
        *invalid_utf8.last_mut().unwrap() = 0xff;
        assert_eq!(
            parse_checked_untrusted_mtgo_visible_game_log_v1(&invalid_utf8)
                .err()
                .unwrap()
                .code(),
            "visible_game_log_utf8"
        );
    }

    #[test]
    fn commitments_change_with_visible_or_hidden_source_bytes() {
        let first = parse_checked_untrusted_mtgo_visible_game_log_v1(&file_v1(&[(
            638_905_999_999_999_999,
            0,
            "@PAlice draws a card.",
        )]))
        .unwrap();
        let visible_changed = parse_checked_untrusted_mtgo_visible_game_log_v1(&file_v1(&[(
            638_905_999_999_999_999,
            0,
            "@PAlice plays a land.",
        )]))
        .unwrap();
        let timestamp_changed = parse_checked_untrusted_mtgo_visible_game_log_v1(&file_v1(&[(
            638_906_000_000_000_000,
            0,
            "@PAlice draws a card.",
        )]))
        .unwrap();
        assert_ne!(
            first.projection_commitment_sha256_v1(),
            visible_changed.projection_commitment_sha256_v1()
        );
        assert_ne!(
            first.projection_commitment_sha256_v1(),
            timestamp_changed.projection_commitment_sha256_v1()
        );
    }
}
