use super::*;

const A6_PART1_SHA256: &str = "fd189cb58321d6b8401b1eaab244759ee122f438d02948584716a18506d4fbd1";
const A6_PART1_HEADING: &str = "## Signature, part 1";
const A6_PART2_HEADING: &str = "## Signature, part 2";
const PIN_KEYS: [&str; 5] = [
    "binary",
    "cube_list",
    "fence_record",
    "card_index",
    "cube_table",
];
const INPUT_KEYS: [&str; 2] = ["pool_sha256", "constants_sha256"];

pub(super) fn is_timestamp(date: &str, time: &str) -> bool {
    let matches = |text: &str, mask: &str| {
        text.len() == mask.len()
            && text.bytes().zip(mask.bytes()).all(|(byte, expected)| {
                if expected == b'0' {
                    byte.is_ascii_digit()
                } else {
                    byte == expected
                }
            })
    };
    matches(date, "0000-00-00") && matches(time, "00:00")
}

fn a6_attestation(line: &str) -> bool {
    if let Some(tail) = line.strip_prefix("Research lead,") {
        let mut words = tail.split_whitespace();
        if let (Some(date), Some(time), Some("ET." | "UTC."), Some(signature)) =
            (words.next(), words.next(), words.next(), words.next())
        {
            return is_timestamp(date, time)
                && signature
                    .strip_prefix("Countersigned")
                    .is_some_and(|suffix| {
                        suffix
                            .chars()
                            .next()
                            .is_none_or(|c| !c.is_alphanumeric() && c != '_')
                    });
        }
    }
    if let Some(tail) = line.strip_prefix("Countersigned,") {
        let mut words = tail.split_whitespace();
        if let (Some("research"), Some("lead,"), Some(date), Some(time), Some(zone)) = (
            words.next(),
            words.next(),
            words.next(),
            words.next(),
            words.next(),
        ) {
            return is_timestamp(date, time)
                && ["ET", "UTC"].iter().any(|prefix| {
                    zone.strip_prefix(prefix).is_some_and(|suffix| {
                        suffix
                            .chars()
                            .next()
                            .is_none_or(|c| !c.is_alphanumeric() && c != '_')
                    })
                });
        }
    }
    false
}

fn json_blocks(text: &str) -> impl Iterator<Item = &str> {
    text.match_indices("```json").filter_map(|(start, tag)| {
        let (rest, body) = text[start + tag.len()..].split_once('\n')?;
        if !rest.trim().is_empty() {
            return None;
        }
        let (body, _) = body.split_once("\n```")?;
        Some(body)
    })
}

pub(super) fn verify_countersigned_a6(
    raw: &[u8],
) -> Result<(PodPins, Option<PlanInputs>), PodRunnerError> {
    let text = std::str::from_utf8(raw)
        .map_err(|_| plan_err("A6 declaration file is not valid UTF-8"))?
        .replace("\r\n", "\n");
    let (part1, tail) = text
        .split_once(A6_PART1_HEADING)
        .ok_or_else(|| plan_err("A6 file lacks the signed part 1 signature block"))?;
    let part1 = format!("{}\n", part1.trim_end_matches('\n'));
    if sha256_hex(part1.as_bytes()) != A6_PART1_SHA256 {
        return Err(plan_err("A6 part 1 does not match the signed text hash"));
    }
    let part2_offsets: Vec<_> = tail
        .split_inclusive('\n')
        .scan(0, |offset, line| {
            let start = *offset;
            *offset += line.len();
            Some((start, line.trim_end_matches('\n')))
        })
        .filter_map(|(offset, line)| {
            line.strip_prefix(A6_PART2_HEADING)
                .filter(|suffix| suffix.is_empty() || suffix.starts_with(':'))
                .map(|_| offset)
        })
        .collect();
    let [offset] = part2_offsets.as_slice() else {
        return Err(plan_err(
            "A6 requires exactly one Signature, part 2 section",
        ));
    };
    let (part1_sig, part2) = tail.split_at(*offset);
    if !part1_sig
        .lines()
        .any(|line| line.trim_end() == "Signed, research lead.")
    {
        return Err(plan_err("A6 lacks the signed part 1 closing line"));
    }
    let lower = part2.to_ascii_lowercase();
    // The immutable part 1 explicitly defers these pins. Only part 2 attests that they are filled.
    if [
        "to fill at signing",
        "not signed",
        "no countersignature",
        "not countersigned",
        "not yet countersigned",
        "pending countersignature",
    ]
    .iter()
    .any(|phrase| lower.contains(phrase))
        || lower
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .any(|word| word == "unsigned")
    {
        return Err(plan_err(
            "A6 part 2 contains an unsigned statement or placeholder",
        ));
    }
    if !part2.lines().any(a6_attestation) {
        return Err(plan_err(
            "A6 part 2 lacks a timestamped research-lead countersignature",
        ));
    }
    if !part2.lines().any(|line| {
        matches!(
            line.trim_end(),
            "Signed, research lead." | "Countersigned, research lead."
        )
    }) {
        return Err(plan_err("A6 part 2 lacks the research-lead closing line"));
    }

    let mut fills = BTreeMap::new();
    let mut record = |key: &str, digest: &str| -> Result<(), PodRunnerError> {
        if !is_sha256_hex(digest) {
            return Err(plan_err(format!("invalid A6 part 2 pin {key}")));
        }
        if fills
            .insert(key.to_owned(), digest.to_owned())
            .is_some_and(|old| old != digest)
        {
            return Err(plan_err(format!("conflicting A6 part 2 pin {key}")));
        }
        Ok(())
    };
    let mut policies = BTreeMap::new();
    for block in json_blocks(part2) {
        let doc: Value = serde_json::from_str(block)
            .map_err(|error| plan_err(format!("invalid A6 part 2 JSON: {error}")))?;
        let object = doc
            .as_object()
            .ok_or_else(|| plan_err("A6 part 2 JSON must be an object"))?;
        for (section, keys) in [
            ("pins", PIN_KEYS.as_slice()),
            ("inputs", INPUT_KEYS.as_slice()),
        ] {
            let Some(value) = object.get(section) else {
                continue;
            };
            if section == "inputs" && value.is_null() {
                continue;
            }
            let values = value
                .as_object()
                .ok_or_else(|| plan_err(format!("A6 part 2 {section} must be an object")))?;
            for &key in keys {
                if let Some(value) = values.get(key) {
                    if section == "inputs" && value.is_null() {
                        continue;
                    }
                    record(
                        key,
                        value
                            .as_str()
                            .ok_or_else(|| plan_err(format!("invalid A6 part 2 pin {key}")))?,
                    )?;
                }
            }
        }
        if let Some(value) = object.get("policies").filter(|value| !value.is_null()) {
            let values = value
                .as_object()
                .ok_or_else(|| plan_err("A6 part 2 policies must be an object"))?;
            for (key, value) in values {
                if policies
                    .insert(key.clone(), value.clone())
                    .is_some_and(|old| old != *value)
                {
                    return Err(plan_err(format!("conflicting A6 part 2 policy {key}")));
                }
            }
        }
    }
    for line in part2.lines().map(str::trim) {
        let (line, separator) = if let Some(line) = line
            .strip_prefix(['-', '*'])
            .filter(|tail| tail.starts_with([' ', '\t']))
        {
            (line, ':')
        } else if let Some(line) = line.strip_prefix('|') {
            (line, '|')
        } else {
            continue;
        };
        if let Some((key, value)) = line.split_once(separator) {
            let key = key.trim().trim_matches('`');
            let value = value.trim().trim_end_matches('|').trim().trim_matches('`');
            if (PIN_KEYS.contains(&key) || INPUT_KEYS.contains(&key)) && is_sha256_hex(value) {
                record(key, value)?;
            }
        }
    }
    if let Some(key) = PIN_KEYS.iter().find(|key| !fills.contains_key(**key)) {
        return Err(plan_err(format!("A6 part 2 lacks required pin fill {key}")));
    }
    let inputs = match (
        fills.remove("pool_sha256"),
        fills.remove("constants_sha256"),
    ) {
        (Some(pool), Some(constants)) => Some(PlanInputs {
            pool_sha256: Some(pool),
            constants_sha256: Some(constants),
        }),
        (None, None) => None,
        _ => {
            return Err(plan_err(
                "A6 part 2 must fill both pool and constants hashes together",
            ))
        }
    };
    Ok((
        PodPins {
            binary: fills.remove("binary"),
            cube_list: fills.remove("cube_list"),
            fence_record: fills.remove("fence_record"),
            card_index: fills.remove("card_index"),
            cube_table: fills.remove("cube_table"),
        },
        inputs,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIGNED_PART1: &[u8] =
        include_bytes!("../../../tools/cube_v1/tests/fixtures/a6_part1_signed.md");

    #[test]
    fn a6_refuses_part1_only_and_negative_countersignatures() {
        assert!(verify_countersigned_a6(SIGNED_PART1).is_err());
        let pins: BTreeMap<_, _> = PIN_KEYS.iter().map(|key| (*key, "a".repeat(64))).collect();
        let suffix = format!(
            "\n\n## Signature, part 2\nResearch lead, 2026-11-25 12:00 ET. Countersigned.\n```json\n{}\n```\nCountersigned, research lead.\n",
            serde_json::json!({"pins": pins})
        );
        let signed = [SIGNED_PART1, suffix.as_bytes()].concat();
        assert_eq!(
            verify_countersigned_a6(&signed).unwrap().0.binary,
            Some("a".repeat(64))
        );
        for text in [
            "Not signed yet",
            "TO FILL AT SIGNING",
            "No countersignature",
            "## Signature, part 2: duplicate",
        ] {
            assert!(
                verify_countersigned_a6(&[signed.as_slice(), text.as_bytes()].concat()).is_err(),
                "{text}"
            );
        }
    }
}
