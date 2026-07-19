//! Portable golden-artifact consumer for the full-episode trajectory contract.
//!
//! The artifact is produced by an independent stdlib-only Python generator.
//! Its exact canonical-file and semantic-vector-stream SHA-256 values are pinned
//! here so either producer or consumer drift fails closed.

use super::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

const GOLDEN_SCHEMA_V1: &str = "mtg_kernel_native_full_episode_trajectory_goldens/v1";
const GOLDEN_GENERATOR_IDENTITY_V1: &str =
    "mtg-kernel-native-full-episode-trajectory-goldens-stdlib-python-v1";
const GOLDEN_VECTOR_STREAM_IDENTITY_V1: &str =
    "mtg-kernel-native-full-episode-trajectory-golden-vector-stream-sha256-v1";
const GOLDEN_ARTIFACT_SHA256_V1: &str =
    "502a1b4ba296fdc4b2f4e8fd61cc5b4d64f152c9b84b4e11a85967f76c3bde8b";
const GOLDEN_VECTOR_STREAM_SHA256_V1: &str =
    "f5230cbbc0b87735e7aa14c89ce31e41ce769de3f4292cafe63dad4733168d7a";
const MAX_GOLDEN_ARTIFACT_BYTES_V1: usize = 4 * 1_024 * 1_024;
const MAX_GOLDEN_CASES_V1: usize = 256;
const MAX_GOLDEN_DECISIONS_V1: usize = 4_096;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct GoldenArtifactV1 {
    schema: String,
    generator_identity: String,
    trajectory_identity: String,
    vector_stream_identity: String,
    positive_cases: Vec<PositiveCaseV1>,
    reject_cases: Vec<RejectCaseV1>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PositiveCaseV1 {
    name: String,
    input: GoldenInputV1,
    stream_hex: String,
    expected_sha256: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RejectCaseV1 {
    name: String,
    input: GoldenInputV1,
    expected_rejection: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct GoldenInputV1 {
    episode_index_u64_hex: String,
    environment_seed_u64_hex: String,
    deck_p0_id: String,
    deck_p0_hash_u64_hex: String,
    deck_p1_id: String,
    deck_p1_hash_u64_hex: String,
    learner_seat: String,
    decisions: Vec<GoldenDecisionV1>,
    terminal: GoldenTerminalV1,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct GoldenDecisionV1 {
    row_ordinal_u64_hex: String,
    actor_seat: String,
    actor_role: String,
    physical_decision_ordinal_u64_hex: String,
    actor_physical_decision_ordinal_u64_hex: String,
    substep_index_u32: u32,
    substep_count_u32: u32,
    action_seed_u64_hex: String,
    legal_action_count_u32: u32,
    selected_index_u32: u32,
    flat_action_v2_commitment_hex: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct GoldenTerminalV1 {
    episode_index_u64_hex: String,
    deck_p0_hash_u64_hex: String,
    deck_p1_hash_u64_hex: String,
    outcome: String,
    winner: String,
    classification: String,
    terminal_code: String,
    policy_step_count_u64_hex: String,
    physical_decision_count_u64_hex: String,
}

#[derive(Debug, PartialEq, Eq)]
enum GoldenRunErrorV1 {
    Contract(NativeFullEpisodeTrajectoryErrorV1),
    MalformedCommitment,
    InvalidFixture(&'static str),
}

fn golden_artifact_path_v1() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("data")
        .join("native_full_episode_trajectory_v1_goldens.json")
}

fn parse_lower_hex_v1<const N: usize>(value: &str) -> Option<[u8; N]> {
    if value.len() != N * 2
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return None;
    }
    let mut output = [0_u8; N];
    for (index, chunk) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = hex_nibble_v1(chunk[0])?;
        let low = hex_nibble_v1(chunk[1])?;
        output[index] = (high << 4) | low;
    }
    Some(output)
}

fn hex_nibble_v1(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}

fn parse_u64_hex_v1(value: &str) -> Result<u64, GoldenRunErrorV1> {
    let bytes =
        parse_lower_hex_v1::<8>(value).ok_or(GoldenRunErrorV1::InvalidFixture("invalid u64hex"))?;
    Ok(u64::from_be_bytes(bytes))
}

fn player_seat_v1(value: &str) -> Result<PlayerSeatV1, GoldenRunErrorV1> {
    match value {
        "p0" => Ok(PlayerSeatV1::P0),
        "p1" => Ok(PlayerSeatV1::P1),
        _ => Err(GoldenRunErrorV1::InvalidFixture("invalid seat")),
    }
}

fn actor_role_v1(value: &str) -> Result<NativeTrajectoryActorRoleV1, GoldenRunErrorV1> {
    match value {
        "learner" => Ok(NativeTrajectoryActorRoleV1::Learner),
        "opponent" => Ok(NativeTrajectoryActorRoleV1::Opponent),
        _ => Err(GoldenRunErrorV1::InvalidFixture("invalid actor role")),
    }
}

fn terminal_outcome_v1(value: &str) -> Result<TerminalOutcomeV1, GoldenRunErrorV1> {
    match value {
        "p0-win" => Ok(TerminalOutcomeV1::P0Win),
        "p1-win" => Ok(TerminalOutcomeV1::P1Win),
        "draw" => Ok(TerminalOutcomeV1::Draw),
        "truncated" => Ok(TerminalOutcomeV1::Truncated),
        "halted" => Ok(TerminalOutcomeV1::Halted),
        _ => Err(GoldenRunErrorV1::InvalidFixture("invalid terminal outcome")),
    }
}

fn winner_v1(value: &str) -> Result<Option<PlayerSeatV1>, GoldenRunErrorV1> {
    match value {
        "none" => Ok(None),
        "p0" => Ok(Some(PlayerSeatV1::P0)),
        "p1" => Ok(Some(PlayerSeatV1::P1)),
        _ => Err(GoldenRunErrorV1::InvalidFixture("invalid winner")),
    }
}

fn terminal_classification_v1(value: &str) -> Result<TerminalClassificationV1, GoldenRunErrorV1> {
    match value {
        "natural" => Ok(TerminalClassificationV1::Natural),
        "truncated" => Ok(TerminalClassificationV1::Truncated),
        "halted" => Ok(TerminalClassificationV1::Halted),
        _ => Err(GoldenRunErrorV1::InvalidFixture(
            "invalid terminal classification",
        )),
    }
}

fn terminal_safe_code_v1(value: &str) -> Result<TerminalSafeCodeV2, GoldenRunErrorV1> {
    match value {
        "natural-game-over" => Ok(TerminalSafeCodeV2::NaturalGameOver),
        "decision-cap" => Ok(TerminalSafeCodeV2::DecisionCap),
        "fail-closed" => Ok(TerminalSafeCodeV2::FailClosed),
        _ => Err(GoldenRunErrorV1::InvalidFixture("invalid terminal code")),
    }
}

fn terminal_reward_v1(outcome: TerminalOutcomeV1) -> [i32; 2] {
    match outcome {
        TerminalOutcomeV1::P0Win => [1, -1],
        TerminalOutcomeV1::P1Win => [-1, 1],
        TerminalOutcomeV1::Draw | TerminalOutcomeV1::Truncated | TerminalOutcomeV1::Halted => {
            [0, 0]
        }
    }
}

fn printable_ascii_with_max_v1(value: &str, maximum: usize) -> bool {
    value.len() <= maximum && value.bytes().all(|byte| (0x20..=0x7e).contains(&byte))
}

fn validate_outer_input_v1(input: &GoldenInputV1) -> Result<(), GoldenRunErrorV1> {
    let episode_index = parse_u64_hex_v1(&input.episode_index_u64_hex)?;
    if episode_index > i64::MAX as u64 {
        return Err(GoldenRunErrorV1::InvalidFixture(
            "episode index exceeds u63",
        ));
    }
    for value in [
        &input.environment_seed_u64_hex,
        &input.deck_p0_hash_u64_hex,
        &input.deck_p1_hash_u64_hex,
        &input.terminal.episode_index_u64_hex,
        &input.terminal.deck_p0_hash_u64_hex,
        &input.terminal.deck_p1_hash_u64_hex,
        &input.terminal.policy_step_count_u64_hex,
        &input.terminal.physical_decision_count_u64_hex,
    ] {
        parse_u64_hex_v1(value)?;
    }
    if !printable_ascii_with_max_v1(&input.deck_p0_id, 65)
        || !printable_ascii_with_max_v1(&input.deck_p1_id, 65)
    {
        return Err(GoldenRunErrorV1::InvalidFixture(
            "deck ID violates outer ASCII bounds",
        ));
    }
    player_seat_v1(&input.learner_seat)?;
    if input.decisions.len() > MAX_GOLDEN_DECISIONS_V1 {
        return Err(GoldenRunErrorV1::InvalidFixture("decision cap exceeded"));
    }
    for row in &input.decisions {
        for value in [
            &row.row_ordinal_u64_hex,
            &row.physical_decision_ordinal_u64_hex,
            &row.actor_physical_decision_ordinal_u64_hex,
            &row.action_seed_u64_hex,
        ] {
            parse_u64_hex_v1(value)?;
        }
        player_seat_v1(&row.actor_seat)?;
        actor_role_v1(&row.actor_role)?;
        if !printable_ascii_with_max_v1(&row.flat_action_v2_commitment_hex, 34) {
            return Err(GoldenRunErrorV1::InvalidFixture(
                "commitment violates outer ASCII bounds",
            ));
        }
    }
    terminal_outcome_v1(&input.terminal.outcome)?;
    winner_v1(&input.terminal.winner)?;
    terminal_classification_v1(&input.terminal.classification)?;
    terminal_safe_code_v1(&input.terminal.terminal_code)?;
    Ok(())
}

fn run_golden_input_v1(
    input: &GoldenInputV1,
) -> Result<NativeFullEpisodeTrajectoryReceiptV1, GoldenRunErrorV1> {
    validate_outer_input_v1(input)?;
    let episode_index = parse_u64_hex_v1(&input.episode_index_u64_hex)?;
    let environment_seed = parse_u64_hex_v1(&input.environment_seed_u64_hex)?;
    let deck_hashes = [
        parse_u64_hex_v1(&input.deck_p0_hash_u64_hex)?,
        parse_u64_hex_v1(&input.deck_p1_hash_u64_hex)?,
    ];
    let deck_ids = [input.deck_p0_id.clone(), input.deck_p1_id.clone()];
    let learner_seat = player_seat_v1(&input.learner_seat)?;
    let mut accumulator = NativeFullEpisodeTrajectoryAccumulatorV1::new_v1(
        episode_index,
        environment_seed,
        &deck_ids,
        deck_hashes,
        learner_seat,
    )
    .map_err(GoldenRunErrorV1::Contract)?;

    for row in &input.decisions {
        let commitment = parse_lower_hex_v1::<16>(&row.flat_action_v2_commitment_hex)
            .ok_or(GoldenRunErrorV1::MalformedCommitment)?;
        let decision = NativeFullEpisodeTrajectoryDecisionRowV1 {
            row_ordinal: parse_u64_hex_v1(&row.row_ordinal_u64_hex)?,
            actor_seat: player_seat_v1(&row.actor_seat)?,
            actor_role: actor_role_v1(&row.actor_role)?,
            physical_decision_ordinal: parse_u64_hex_v1(&row.physical_decision_ordinal_u64_hex)?,
            actor_physical_decision_ordinal: parse_u64_hex_v1(
                &row.actor_physical_decision_ordinal_u64_hex,
            )?,
            substep_index: row.substep_index_u32,
            substep_count: row.substep_count_u32,
            action_seed: parse_u64_hex_v1(&row.action_seed_u64_hex)?,
            legal_action_count: row.legal_action_count_u32,
            selected_index: row.selected_index_u32,
            flat_action_v2_commitment: commitment,
        };
        accumulator
            .record_accepted_v1(decision)
            .map_err(GoldenRunErrorV1::Contract)?;
    }

    let terminal_outcome = terminal_outcome_v1(&input.terminal.outcome)?;
    let terminal = AsyncRolloutTerminalV1 {
        episode_id: parse_u64_hex_v1(&input.terminal.episode_index_u64_hex)?,
        terminal_outcome,
        terminal_classification: terminal_classification_v1(&input.terminal.classification)?,
        terminal_code: terminal_safe_code_v1(&input.terminal.terminal_code)?,
        winner: winner_v1(&input.terminal.winner)?,
        terminal_reward: terminal_reward_v1(terminal_outcome),
        policy_step_count: parse_u64_hex_v1(&input.terminal.policy_step_count_u64_hex)?,
        physical_decision_count: parse_u64_hex_v1(&input.terminal.physical_decision_count_u64_hex)?,
    };
    let terminal_deck_hashes = [
        parse_u64_hex_v1(&input.terminal.deck_p0_hash_u64_hex)?,
        parse_u64_hex_v1(&input.terminal.deck_p1_hash_u64_hex)?,
    ];
    accumulator
        .finish_natural_v1(terminal, terminal_deck_hashes)
        .map_err(GoldenRunErrorV1::Contract)
}

fn portable_rejection_code_v1(error: GoldenRunErrorV1) -> &'static str {
    match error {
        GoldenRunErrorV1::MalformedCommitment => "malformed-commitment",
        GoldenRunErrorV1::Contract(error) => match error {
            NativeFullEpisodeTrajectoryErrorV1::InvalidDeckId => "invalid-deck-id",
            NativeFullEpisodeTrajectoryErrorV1::EpisodeMismatch => "episode-mismatch",
            NativeFullEpisodeTrajectoryErrorV1::EmptyDecisionStream => "empty-decision-stream",
            NativeFullEpisodeTrajectoryErrorV1::RowOrdinalMismatch => "row-ordinal-mismatch",
            NativeFullEpisodeTrajectoryErrorV1::ActorRoleMismatch => "actor-role-mismatch",
            NativeFullEpisodeTrajectoryErrorV1::MalformedPhysicalGroup => {
                "malformed-physical-group"
            }
            NativeFullEpisodeTrajectoryErrorV1::InvalidLegalActionCount => {
                "invalid-legal-action-count"
            }
            NativeFullEpisodeTrajectoryErrorV1::SelectedIndexOutOfRange => {
                "selected-index-out-of-range"
            }
            NativeFullEpisodeTrajectoryErrorV1::NonNaturalTerminal => "non-natural-terminal",
            NativeFullEpisodeTrajectoryErrorV1::TerminalProvenanceMismatch => {
                "terminal-provenance-mismatch"
            }
            NativeFullEpisodeTrajectoryErrorV1::TerminalCountMismatch => "terminal-count-mismatch",
            NativeFullEpisodeTrajectoryErrorV1::CounterOverflow => {
                panic!("bounded golden input cannot overflow trajectory counters")
            }
        },
        GoldenRunErrorV1::InvalidFixture(reason) => {
            panic!("golden artifact violated its outer schema: {reason}")
        }
    }
}

fn canonical_json_bytes_v1<T: Serialize>(value: &T) -> Vec<u8> {
    let canonical_value =
        serde_json::to_value(value).expect("golden fixture contains serializable scalars");
    let mut bytes = serde_json::to_vec(&canonical_value)
        .expect("golden fixture canonical JSON serialization succeeds");
    bytes.push(b'\n');
    bytes
}

fn append_atom_v1(stream: &mut Vec<u8>, tag: &str, payload: &[u8]) {
    stream.extend_from_slice(
        &u32::try_from(tag.len())
            .expect("golden atom tag length fits u32")
            .to_be_bytes(),
    );
    stream.extend_from_slice(tag.as_bytes());
    stream.extend_from_slice(
        &u64::try_from(payload.len())
            .expect("golden atom payload length fits u64")
            .to_be_bytes(),
    );
    stream.extend_from_slice(payload);
}

fn append_nested_atom_v1(stream: &mut Vec<u8>, tag: &str, payload: Vec<u8>) {
    append_atom_v1(stream, tag, &payload);
}

fn golden_vector_stream_v1(artifact: &GoldenArtifactV1) -> Vec<u8> {
    let mut stream = Vec::new();
    append_atom_v1(
        &mut stream,
        "domain",
        GOLDEN_VECTOR_STREAM_IDENTITY_V1.as_bytes(),
    );
    append_atom_v1(
        &mut stream,
        "positive_case_count_u64be",
        &u64::try_from(artifact.positive_cases.len())
            .expect("golden positive count fits u64")
            .to_be_bytes(),
    );
    for case in &artifact.positive_cases {
        let mut payload = Vec::new();
        append_atom_v1(&mut payload, "name_utf8", case.name.as_bytes());
        append_atom_v1(
            &mut payload,
            "input_canonical_json",
            &canonical_json_bytes_v1(&case.input),
        );
        let stream_bytes = parse_hex_vec_v1(&case.stream_hex)
            .expect("positive stream is nonempty even lowercase hex");
        append_atom_v1(&mut payload, "stream_bytes", &stream_bytes);
        let expected_sha256 = parse_lower_hex_v1::<32>(&case.expected_sha256)
            .expect("positive expected SHA-256 is lowercase raw32");
        append_atom_v1(&mut payload, "expected_sha256", &expected_sha256);
        append_nested_atom_v1(&mut stream, "positive_case", payload);
    }
    append_atom_v1(
        &mut stream,
        "reject_case_count_u64be",
        &u64::try_from(artifact.reject_cases.len())
            .expect("golden reject count fits u64")
            .to_be_bytes(),
    );
    for case in &artifact.reject_cases {
        let mut payload = Vec::new();
        append_atom_v1(&mut payload, "name_utf8", case.name.as_bytes());
        append_atom_v1(
            &mut payload,
            "input_canonical_json",
            &canonical_json_bytes_v1(&case.input),
        );
        append_atom_v1(
            &mut payload,
            "expected_rejection_ascii",
            case.expected_rejection.as_bytes(),
        );
        append_nested_atom_v1(&mut stream, "reject_case", payload);
    }
    stream
}

fn parse_hex_vec_v1(value: &str) -> Option<Vec<u8>> {
    if value.is_empty()
        || (value.len() & 1) != 0
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return None;
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|chunk| Some((hex_nibble_v1(chunk[0])? << 4) | hex_nibble_v1(chunk[1])?))
        .collect()
}

fn valid_case_name_v1(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 128
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

fn assert_strict_case_order_v1<'a>(names: impl Iterator<Item = &'a str>) {
    let names = names.collect::<Vec<_>>();
    assert!(!names.is_empty());
    assert!(names.len() <= MAX_GOLDEN_CASES_V1);
    assert!(names.iter().all(|name| valid_case_name_v1(name)));
    assert!(names
        .windows(2)
        .all(|pair| pair[0].as_bytes() < pair[1].as_bytes()));
}

fn assert_receipt_matches_input_v1(
    case: &PositiveCaseV1,
    receipt: NativeFullEpisodeTrajectoryReceiptV1,
) {
    let input = &case.input;
    assert_eq!(
        receipt.episode_index,
        parse_u64_hex_v1(&input.episode_index_u64_hex).unwrap()
    );
    assert_eq!(
        receipt.environment_seed,
        parse_u64_hex_v1(&input.environment_seed_u64_hex).unwrap()
    );
    assert_eq!(
        receipt.deck_hashes,
        [
            parse_u64_hex_v1(&input.deck_p0_hash_u64_hex).unwrap(),
            parse_u64_hex_v1(&input.deck_p1_hash_u64_hex).unwrap(),
        ]
    );
    assert_eq!(
        receipt.learner_seat,
        player_seat_v1(&input.learner_seat).unwrap()
    );
    assert_eq!(
        receipt.policy_step_count,
        parse_u64_hex_v1(&input.terminal.policy_step_count_u64_hex).unwrap()
    );
    assert_eq!(
        receipt.physical_decision_count,
        parse_u64_hex_v1(&input.terminal.physical_decision_count_u64_hex).unwrap()
    );
    let learner_policy = input
        .decisions
        .iter()
        .filter(|row| row.actor_role == "learner")
        .count() as u64;
    let opponent_policy = input.decisions.len() as u64 - learner_policy;
    let learner_physical = input
        .decisions
        .iter()
        .filter(|row| {
            row.actor_role == "learner"
                && row.substep_index_u32.checked_add(1) == Some(row.substep_count_u32)
        })
        .count() as u64;
    let opponent_physical = input
        .decisions
        .iter()
        .filter(|row| {
            row.actor_role == "opponent"
                && row.substep_index_u32.checked_add(1) == Some(row.substep_count_u32)
        })
        .count() as u64;
    assert_eq!(receipt.learner_policy_step_count, learner_policy);
    assert_eq!(receipt.opponent_policy_step_count, opponent_policy);
    assert_eq!(receipt.learner_physical_decision_count, learner_physical);
    assert_eq!(receipt.opponent_physical_decision_count, opponent_physical);
}

#[test]
fn portable_full_episode_trajectory_goldens_match_production() {
    let artifact_path = golden_artifact_path_v1();
    let artifact_bytes = std::fs::read(&artifact_path).unwrap_or_else(|error| {
        panic!(
            "{} is missing; run the stdlib trajectory generator first: {error}",
            artifact_path.display()
        )
    });
    assert!(artifact_bytes.len() <= MAX_GOLDEN_ARTIFACT_BYTES_V1);
    let artifact: GoldenArtifactV1 =
        serde_json::from_slice(&artifact_bytes).expect("strict typed golden artifact parses");
    assert_eq!(artifact_bytes, canonical_json_bytes_v1(&artifact));
    assert_eq!(artifact.schema, GOLDEN_SCHEMA_V1);
    assert_eq!(artifact.generator_identity, GOLDEN_GENERATOR_IDENTITY_V1);
    assert_eq!(
        artifact.trajectory_identity,
        NATIVE_FULL_EPISODE_TRAJECTORY_IDENTITY_V1
    );
    assert_eq!(
        artifact.vector_stream_identity,
        GOLDEN_VECTOR_STREAM_IDENTITY_V1
    );
    assert_strict_case_order_v1(
        artifact
            .positive_cases
            .iter()
            .map(|case| case.name.as_str()),
    );
    assert_strict_case_order_v1(artifact.reject_cases.iter().map(|case| case.name.as_str()));

    let artifact_sha256: [u8; 32] = Sha256::digest(&artifact_bytes).into();
    let vector_stream_sha256: [u8; 32] = Sha256::digest(golden_vector_stream_v1(&artifact)).into();
    assert_eq!(
        artifact_sha256,
        parse_lower_hex_v1::<32>(GOLDEN_ARTIFACT_SHA256_V1)
            .expect("artifact SHA-256 pin is lowercase raw32")
    );
    assert_eq!(
        vector_stream_sha256,
        parse_lower_hex_v1::<32>(GOLDEN_VECTOR_STREAM_SHA256_V1)
            .expect("vector-stream SHA-256 pin is lowercase raw32")
    );

    for case in &artifact.positive_cases {
        let stream = parse_hex_vec_v1(&case.stream_hex)
            .expect("positive stream must be nonempty even lowercase hex");
        let expected_sha256 = parse_lower_hex_v1::<32>(&case.expected_sha256)
            .expect("positive expected SHA-256 must be lowercase raw32");
        let stream_sha256: [u8; 32] = Sha256::digest(&stream).into();
        assert_eq!(stream_sha256, expected_sha256);
        let receipt = run_golden_input_v1(&case.input)
            .unwrap_or_else(|error| panic!("positive case {} rejected: {error:?}", case.name));
        assert_eq!(receipt.trajectory_sha256, expected_sha256, "{}", case.name);
        assert_receipt_matches_input_v1(case, receipt);
    }
    for case in &artifact.reject_cases {
        let error = run_golden_input_v1(&case.input)
            .expect_err("declared reject case was unexpectedly admitted");
        assert_eq!(
            portable_rejection_code_v1(error),
            case.expected_rejection,
            "{}",
            case.name
        );
    }
}
