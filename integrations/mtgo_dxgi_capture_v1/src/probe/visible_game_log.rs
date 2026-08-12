use super::duel_perception_runtime::parse_competitive_duel_window_title_v1;
use super::*;
use mtgo_blackbox_v1::{
    classify_checked_untrusted_mtgo_visible_game_log_semantics_v1,
    mtgo_visible_game_log_source_id_commitment_v1,
    parse_checked_untrusted_mtgo_visible_game_log_v1,
    CheckedUntrustedMtgoVisibleGameLogProjectionV1,
    CheckedUntrustedMtgoVisibleGameLogSemanticProjectionV1, MtgoCompetitiveEventKindV1,
    MtgoVisibleGameLogEventKindV1, MtgoVisibleGameLogSemanticEventViewV1,
    MtgoVisibleGameLogTextViewV1,
};
use sha2::{Digest, Sha256};
use std::fs::Metadata;

const MAX_VISIBLE_GAME_LOG_BIND_BRACKET_MILLIS_V1: u128 = 5_000;
const MAX_VISIBLE_GAME_LOG_TREE_ENTRIES_V1: usize = 100_000;
const MAX_VISIBLE_GAME_LOG_TREE_DEPTH_V1: usize = 12;
const VISIBLE_GAME_LOG_BINDING_DOMAIN_V1: &[u8] = b"mtgo-visible-game-log-binding-v1";
const VISIBLE_GAME_LOG_DUEL_TITLE_IDENTITY_DOMAIN_V1: &[u8] =
    b"mtgo-visible-game-log-duel-title-identity-v1";
const COMPETITIVE_VISIBLE_GAME_LOG_BINDING_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-visible-game-log-binding-v1";
const WINDOWS_TO_UNIX_EPOCH_100NS_V1: u64 = 116_444_736_000_000_000;

#[derive(Clone, Copy, PartialEq, Eq)]
struct StableFileIdentityV1 {
    length: u64,
    creation_unix_millis: u128,
    modification_unix_millis: u128,
}

struct BoundVisibleGameLogDuelTitleIdentityV1 {
    opponent_alias_sha256: String,
    visible_match_id_sha256: String,
    visible_game_id_sha256: String,
    identity_commitment_sha256: String,
}

/// One exact persisted Game Log projection bound to the sole file created by
/// the admitted MTGO process epoch and bracketed by two stable game-window
/// captures.
///
/// The path, source UUID, timestamps, raw bytes, markup, and numeric link
/// metadata remain private. Only the text that MTGO renders in its Game Log is
/// exposed. MTGO documents that players can scroll the Game Log to review
/// previous game actions, so the complete persisted record is within the
/// player-view information boundary even when an older line is not inside the
/// current scroll viewport.
///
/// This v1 source admits only the sole Game Log file created since the current
/// MTGO process started. It therefore rejects after a second match or game log
/// is created in the same process. A future competitive lifecycle binder must
/// supply an opaque game-start boundary for multi-game sessions.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoProcessEpochVisibleGameLogV1;
/// fn cannot_act(value: &OpaqueMtgoProcessEpochVisibleGameLogV1) {
///     let _ = value.input_command();
///     let _ = value.score_model();
/// }
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoProcessEpochVisibleGameLogV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoProcessEpochVisibleGameLogV1>();
/// ```
pub struct OpaqueMtgoProcessEpochVisibleGameLogV1 {
    _before_frame: OpaqueMtgoDxgiFrameCandidateV3,
    _after_frame: OpaqueMtgoDxgiFrameCandidateV3,
    projection: CheckedUntrustedMtgoVisibleGameLogProjectionV1,
    process_continuity_commitment_sha256: String,
    duel_title_identity: Option<BoundVisibleGameLogDuelTitleIdentityV1>,
    binding_commitment_sha256: String,
}

/// Player-role semantic events that retain their opaque process/window-bound
/// Game Log source. The checked offline semantic projection cannot be
/// substituted for this type.
pub struct OpaqueMtgoProcessEpochVisibleGameLogSemanticsV1 {
    _source: OpaqueMtgoProcessEpochVisibleGameLogV1,
    semantics: CheckedUntrustedMtgoVisibleGameLogSemanticProjectionV1,
}

/// Exact player-visible Game Log events bound to the opaque competitive
/// launch identity that supplied the current event, match, and game lineage.
/// Raw visible IDs and player aliases remain private; only their commitments
/// and player-relative public events cross this boundary.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveVisibleGameLogSemanticsV1;
/// let _forged = OpaqueMtgoCompetitiveVisibleGameLogSemanticsV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveVisibleGameLogSemanticsV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoCompetitiveVisibleGameLogSemanticsV1>();
/// ```
pub struct OpaqueMtgoCompetitiveVisibleGameLogSemanticsV1 {
    source: OpaqueMtgoProcessEpochVisibleGameLogSemanticsV1,
    event_kind: MtgoCompetitiveEventKindV1,
    event_identity_sha256: String,
    match_identity_sha256: String,
    game_number: u8,
    competitive_launch_identity_commitment_sha256: String,
    current_game_event_start_index: usize,
    current_game_event_count: usize,
    binding_commitment_sha256: String,
}

impl OpaqueMtgoCompetitiveVisibleGameLogSemanticsV1 {
    pub fn event_count_v1(&self) -> usize {
        self.current_game_event_count
    }

    pub fn event_v1(&self, index: usize) -> Option<MtgoVisibleGameLogSemanticEventViewV1<'_>> {
        if index >= self.current_game_event_count {
            return None;
        }
        self.current_game_event_start_index
            .checked_add(index)
            .and_then(|source_index| self.source.event_v1(source_index))
    }

    pub fn event_kind_v1(&self) -> MtgoCompetitiveEventKindV1 {
        self.event_kind
    }

    pub fn event_identity_sha256_v1(&self) -> &str {
        &self.event_identity_sha256
    }

    pub fn match_identity_sha256_v1(&self) -> &str {
        &self.match_identity_sha256
    }

    pub fn game_number_v1(&self) -> u8 {
        self.game_number
    }

    pub fn acting_player_alias_sha256_v1(&self) -> &str {
        self.source.acting_player_alias_sha256_v1()
    }

    pub fn semantic_projection_commitment_sha256_v1(&self) -> &str {
        self.source.semantic_projection_commitment_sha256_v1()
    }

    pub fn competitive_launch_identity_commitment_sha256_v1(&self) -> &str {
        &self.competitive_launch_identity_commitment_sha256
    }

    pub fn binding_commitment_sha256_v1(&self) -> &str {
        &self.binding_commitment_sha256
    }

    pub fn complete_for_current_state_reconstruction_v1(&self) -> bool {
        false
    }

    pub fn safe_for_model_scoring_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

impl OpaqueMtgoProcessEpochVisibleGameLogSemanticsV1 {
    pub fn event_count_v1(&self) -> usize {
        self.semantics.event_count_v1()
    }

    pub fn event_v1(&self, index: usize) -> Option<MtgoVisibleGameLogSemanticEventViewV1<'_>> {
        self.semantics.event_v1(index)
    }

    pub fn classified_source_record_count_v1(&self) -> usize {
        self.semantics.classified_source_record_count_v1()
    }

    pub fn unclassified_source_record_count_v1(&self) -> usize {
        self.semantics.unclassified_source_record_count_v1()
    }

    pub fn semantic_projection_commitment_sha256_v1(&self) -> &str {
        self.semantics.projection_commitment_sha256_v1()
    }

    pub fn acting_player_alias_sha256_v1(&self) -> &str {
        self.semantics.acting_player_alias_sha256_v1()
    }

    pub fn opponent_alias_sha256_v1(&self) -> Option<&str> {
        self.semantics.opponent_alias_sha256_v1()
    }

    pub fn process_continuity_commitment_sha256_v1(&self) -> &str {
        &self._source.process_continuity_commitment_sha256
    }

    pub fn competitive_duel_title_identity_commitment_sha256_v1(&self) -> Option<&str> {
        self._source
            .duel_title_identity
            .as_ref()
            .map(|identity| identity.identity_commitment_sha256.as_str())
    }

    pub fn visible_match_id_sha256_v1(&self) -> Option<&str> {
        self._source
            .duel_title_identity
            .as_ref()
            .map(|identity| identity.visible_match_id_sha256.as_str())
    }

    pub fn visible_game_id_sha256_v1(&self) -> Option<&str> {
        self._source
            .duel_title_identity
            .as_ref()
            .map(|identity| identity.visible_game_id_sha256.as_str())
    }

    pub fn source_bound_to_stable_game_window_and_process_epoch_v1(&self) -> bool {
        true
    }

    pub fn complete_for_current_state_reconstruction_v1(&self) -> bool {
        false
    }

    pub fn safe_for_model_scoring_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

impl OpaqueMtgoProcessEpochVisibleGameLogV1 {
    pub fn record_count_v1(&self) -> usize {
        self.projection.record_count_v1()
    }

    pub fn record_v1(&self, index: usize) -> Option<MtgoVisibleGameLogTextViewV1<'_>> {
        self.projection.record_v1(index)
    }

    pub fn binding_commitment_sha256_v1(&self) -> &str {
        &self.binding_commitment_sha256
    }

    pub fn source_bound_to_stable_game_window_and_process_epoch_v1(&self) -> bool {
        true
    }

    pub fn safe_for_visible_text_classification_v1(&self) -> bool {
        true
    }

    pub fn safe_for_model_scoring_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    /// Reduces the bound rendered lines to conservative public event kinds.
    /// The supplied alias must be the authorized seated account. Names are
    /// hashed or reduced to acting-player/opponent roles in the result.
    pub fn into_visible_semantics_v1(
        self,
        acting_player_alias: &str,
    ) -> Result<OpaqueMtgoProcessEpochVisibleGameLogSemanticsV1, String> {
        let semantics = classify_checked_untrusted_mtgo_visible_game_log_semantics_v1(
            &self.projection,
            acting_player_alias,
        )
        .map_err(|error| format!("{}: {}", error.code(), error.detail()))?;
        if semantics.source_projection_commitment_sha256_v1()
            != self.projection.projection_commitment_sha256_v1()
        {
            return Err("visible Game Log semantic projection lost its bound source".to_owned());
        }
        if let Some(identity) = self.duel_title_identity.as_ref() {
            if semantics
                .opponent_alias_sha256_v1()
                .is_some_and(|observed| observed != identity.opponent_alias_sha256)
            {
                return Err(
                    "visible Game Log player roles do not match the exact duel window opponent"
                        .to_owned(),
                );
            }
        }
        Ok(OpaqueMtgoProcessEpochVisibleGameLogSemanticsV1 {
            _source: self,
            semantics,
        })
    }
}

/// Joins the public Game Log history to the exact opaque League or Challenge
/// launch visible on the same signed MTGO process. This compares the hashed
/// opponent plus visible match and game IDs parsed independently from the
/// stable duel title. It grants neither scoring nor input authority.
pub fn bind_process_epoch_visible_game_log_semantics_to_competitive_launch_identity_v1(
    source: OpaqueMtgoProcessEpochVisibleGameLogSemanticsV1,
    launch: &OpaqueMtgoCompetitiveLaunchIdentityV1,
) -> Result<OpaqueMtgoCompetitiveVisibleGameLogSemanticsV1, String> {
    let title = source
        ._source
        .duel_title_identity
        .as_ref()
        .ok_or("competitive Game Log binding requires a duel-window title identity")?;
    let launch_commitments = launch.commitments_v1();
    let launch_opponent_alias_sha256 = sha256_hex_v1(launch.opponent_display_name_v1().as_bytes());
    let launch_visible_match_id_sha256 = sha256_hex_v1(launch.visible_match_id_v1().as_bytes());
    let launch_visible_game_id_sha256 = sha256_hex_v1(launch.visible_game_id_v1().as_bytes());
    validate_competitive_visible_game_log_lineage_v1(
        &source._source.process_continuity_commitment_sha256,
        source.semantics.opponent_alias_sha256_v1(),
        title,
        &launch_commitments.process_continuity_commitment_sha256,
        &launch_opponent_alias_sha256,
        &launch_visible_match_id_sha256,
        &launch_visible_game_id_sha256,
    )?;
    let (current_game_event_start_index, current_game_event_count) =
        current_game_event_range_from_kinds_v1(
            (0..source.event_count_v1()).map(|index| {
                source
                    .event_v1(index)
                    .expect("semantic event count and lookup are consistent")
                    .kind_v1()
            }),
            launch_commitments.game_number,
        )?;
    let binding_commitment_sha256 = commitment_v1(
        COMPETITIVE_VISIBLE_GAME_LOG_BINDING_DOMAIN_V1,
        &[
            source._source.binding_commitment_sha256.as_bytes(),
            source
                .semantics
                .projection_commitment_sha256_v1()
                .as_bytes(),
            title.identity_commitment_sha256.as_bytes(),
            launch_commitments
                .launch_identity_commitment_sha256
                .as_bytes(),
            launch.event_identity_sha256_v1().as_bytes(),
            launch.match_identity_sha256_v1().as_bytes(),
            &[launch_commitments.game_number],
            &(current_game_event_start_index as u64).to_be_bytes(),
            &(current_game_event_count as u64).to_be_bytes(),
            b"same_visible_duel_process_opponent_match_and_game_no_scoring_no_input",
        ],
    );
    Ok(OpaqueMtgoCompetitiveVisibleGameLogSemanticsV1 {
        source,
        event_kind: launch_commitments.event_kind,
        event_identity_sha256: launch.event_identity_sha256_v1().to_owned(),
        match_identity_sha256: launch.match_identity_sha256_v1().to_owned(),
        game_number: launch_commitments.game_number,
        competitive_launch_identity_commitment_sha256: launch_commitments
            .launch_identity_commitment_sha256,
        current_game_event_start_index,
        current_game_event_count,
        binding_commitment_sha256,
    })
}

fn current_game_event_range_from_kinds_v1(
    kinds: impl IntoIterator<Item = MtgoVisibleGameLogEventKindV1>,
    current_game_number: u8,
) -> Result<(usize, usize), String> {
    if !(1..=3).contains(&current_game_number) {
        return Err("competitive Game Log current game number is invalid".to_owned());
    }
    let mut total = 0_usize;
    let mut current_start = 0_usize;
    let mut completed_games = 0_u8;
    for kind in kinds {
        if matches!(
            kind,
            MtgoVisibleGameLogEventKindV1::WonMatch | MtgoVisibleGameLogEventKindV1::ForcedComplete
        ) {
            return Err(
                "competitive in-progress Game Log already contains a terminal match event"
                    .to_owned(),
            );
        }
        total = total
            .checked_add(1)
            .ok_or("competitive Game Log event count overflow")?;
        if kind == MtgoVisibleGameLogEventKindV1::WonGame {
            completed_games = completed_games
                .checked_add(1)
                .ok_or("competitive Game Log completed-game count overflow")?;
            current_start = total;
        }
    }
    let derived_game_number = completed_games
        .checked_add(1)
        .ok_or("competitive Game Log game number overflow")?;
    if derived_game_number != current_game_number {
        return Err(
            "competitive Game Log completed-game boundaries do not match the visible game number"
                .to_owned(),
        );
    }
    let current_count = total
        .checked_sub(current_start)
        .ok_or("competitive Game Log current-game range underflow")?;
    Ok((current_start, current_count))
}

fn validate_competitive_visible_game_log_lineage_v1(
    source_process_continuity_commitment_sha256: &str,
    source_semantic_opponent_alias_sha256: Option<&str>,
    source_title: &BoundVisibleGameLogDuelTitleIdentityV1,
    launch_process_continuity_commitment_sha256: &str,
    launch_opponent_alias_sha256: &str,
    launch_visible_match_id_sha256: &str,
    launch_visible_game_id_sha256: &str,
) -> Result<(), String> {
    if source_process_continuity_commitment_sha256 != launch_process_continuity_commitment_sha256
        || source_semantic_opponent_alias_sha256
            .is_some_and(|observed| observed != launch_opponent_alias_sha256)
        || source_title.opponent_alias_sha256 != launch_opponent_alias_sha256
        || source_title.visible_match_id_sha256 != launch_visible_match_id_sha256
        || source_title.visible_game_id_sha256 != launch_visible_game_id_sha256
    {
        return Err(
            "visible Game Log does not match the exact competitive process, opponent, match, or game"
                .to_owned(),
        );
    }
    Ok(())
}

/// Captures the admitted foreground game window, reads the sole canonical
/// persisted Game Log created after this MTGO process started, and captures the
/// unchanged window again. No process memory, network traffic, replay state,
/// database, or unrelated client file is read.
pub fn probe_mtgo_process_epoch_visible_game_log_v1(
    request: MtgoDxgiCaptureRequestV3,
) -> Result<OpaqueMtgoProcessEpochVisibleGameLogV1, String> {
    validate_capture_request_v3(&request)?;
    if !matches!(
        request.window_mode,
        CaptureWindowModeV2::SolitaireGame | CaptureWindowModeV2::DuelGame
    ) {
        return Err(
            "visible Game Log binding requires an admitted seated-player game window".to_owned(),
        );
    }
    let before_frame = capture_mtgo_dxgi_frame_candidate_v3(request.clone())?;
    let before = &before_frame.manifest;
    let process_start_unix_millis =
        filetime_100ns_to_unix_millis_v1(before.pre.process_start_filetime_100ns)?;
    let data_root = clickonce_data_root_v1(Path::new(&before.pre.process_image))?;
    let deployment_name = Path::new(&before.pre.process_image)
        .parent()
        .and_then(Path::file_name)
        .and_then(|value| value.to_str())
        .ok_or("MTGO deployment directory is not valid UTF-8")?;
    validate_deployment_name_v1(deployment_name)?;
    let candidate_path = select_process_epoch_game_log_v1(
        &data_root,
        deployment_name,
        process_start_unix_millis,
        before.captured_at_unix_millis,
    )?;
    let before_metadata = fs::metadata(&candidate_path)
        .map_err(|error| format!("stat persisted visible Game Log before read: {error}"))?;
    let bytes = fs::read(&candidate_path)
        .map_err(|error| format!("read persisted visible Game Log: {error}"))?;
    let after_metadata = fs::metadata(&candidate_path)
        .map_err(|error| format!("stat persisted visible Game Log after read: {error}"))?;
    require_stable_file_v1(&before_metadata, &after_metadata, bytes.len())?;
    let projection = parse_checked_untrusted_mtgo_visible_game_log_v1(&bytes)
        .map_err(|error| format!("{}: {}", error.code(), error.detail()))?;
    let source_id = source_id_from_game_log_filename_v1(&candidate_path)?;
    let filename_source_commitment = mtgo_visible_game_log_source_id_commitment_v1(source_id)
        .map_err(|error| format!("{}: {}", error.code(), error.detail()))?;
    if projection.source_match_id_commitment_sha256_v1() != filename_source_commitment {
        return Err("persisted Game Log filename and payload source identifiers differ".to_owned());
    }

    let after_frame = capture_mtgo_dxgi_frame_candidate_v3(request)?;
    let after = &after_frame.manifest;
    if before.pre != before.post
        || after.pre != after.post
        || before.pre != after.pre
        || before.output != after.output
        || before.window_mode != after.window_mode
        || before.capture_role != after.capture_role
        || before.expected_game_format != after.expected_game_format
        || before.title_rule_version != after.title_rule_version
        || before.captured_at_unix_millis > after.captured_at_unix_millis
        || after.captured_at_unix_millis - before.captured_at_unix_millis
            > MAX_VISIBLE_GAME_LOG_BIND_BRACKET_MILLIS_V1
    {
        return Err(
            "MTGO window, process, geometry, output, cursor, or title changed during Game Log binding"
                .to_owned(),
        );
    }
    let post_capture_metadata = fs::metadata(&candidate_path)
        .map_err(|error| format!("stat persisted visible Game Log after capture: {error}"))?;
    require_stable_file_v1(&after_metadata, &post_capture_metadata, bytes.len())?;
    let post_capture_bytes = fs::read(&candidate_path)
        .map_err(|error| format!("reread persisted visible Game Log after capture: {error}"))?;
    let final_metadata = fs::metadata(&candidate_path)
        .map_err(|error| format!("final stat of persisted visible Game Log: {error}"))?;
    require_stable_file_v1(
        &post_capture_metadata,
        &final_metadata,
        post_capture_bytes.len(),
    )?;
    if bytes != post_capture_bytes {
        return Err("persisted Game Log bytes changed across the capture bracket".to_owned());
    }
    let last_write_unix_millis = metadata_modified_unix_millis_v1(&final_metadata)?;
    if last_write_unix_millis > after.captured_at_unix_millis {
        return Err("persisted Game Log changed after the binding capture bracket".to_owned());
    }

    let process_continuity_commitment_sha256 =
        mtgo_process_continuity_commitment_for_frame_v1(&before_frame);
    let duel_title_identity = if before.window_mode == "duel_game" {
        Some(bind_visible_game_log_duel_title_identity_v1(
            &before.pre.title,
            &before.expected_game_format,
            &process_continuity_commitment_sha256,
        )?)
    } else {
        None
    };
    let binding_commitment_sha256 = commitment_v1(
        VISIBLE_GAME_LOG_BINDING_DOMAIN_V1,
        &[
            before_frame.capture_commitment_sha256.as_bytes(),
            after_frame.capture_commitment_sha256.as_bytes(),
            projection.projection_commitment_sha256_v1().as_bytes(),
            filename_source_commitment.as_bytes(),
            &process_start_unix_millis.to_be_bytes(),
            &last_write_unix_millis.to_be_bytes(),
            process_continuity_commitment_sha256.as_bytes(),
            duel_title_identity
                .as_ref()
                .map(|identity| identity.identity_commitment_sha256.as_str())
                .unwrap_or("")
                .as_bytes(),
        ],
    );
    Ok(OpaqueMtgoProcessEpochVisibleGameLogV1 {
        _before_frame: before_frame,
        _after_frame: after_frame,
        projection,
        process_continuity_commitment_sha256,
        duel_title_identity,
        binding_commitment_sha256,
    })
}

fn bind_visible_game_log_duel_title_identity_v1(
    title: &str,
    expected_game_format: &str,
    process_continuity_commitment_sha256: &str,
) -> Result<BoundVisibleGameLogDuelTitleIdentityV1, String> {
    let (opponent, visible_match_id, visible_game_id) =
        parse_competitive_duel_window_title_v1(title, expected_game_format)?;
    let opponent_alias_sha256 = sha256_hex_v1(opponent.as_bytes());
    let visible_match_id_sha256 = sha256_hex_v1(visible_match_id.as_bytes());
    let visible_game_id_sha256 = sha256_hex_v1(visible_game_id.as_bytes());
    let identity_commitment_sha256 = commitment_v1(
        VISIBLE_GAME_LOG_DUEL_TITLE_IDENTITY_DOMAIN_V1,
        &[
            process_continuity_commitment_sha256.as_bytes(),
            expected_game_format.as_bytes(),
            opponent_alias_sha256.as_bytes(),
            visible_match_id_sha256.as_bytes(),
            visible_game_id_sha256.as_bytes(),
            b"exact_stable_player_visible_duel_title_identity_no_hidden_client_state",
        ],
    );
    Ok(BoundVisibleGameLogDuelTitleIdentityV1 {
        opponent_alias_sha256,
        visible_match_id_sha256,
        visible_game_id_sha256,
        identity_commitment_sha256,
    })
}

fn clickonce_data_root_v1(process_image: &Path) -> Result<PathBuf, String> {
    let canonical_image = process_image
        .canonicalize()
        .map_err(|error| format!("canonicalize MTGO image path: {error}"))?;
    let two_point_zero = canonical_image
        .ancestors()
        .find(|ancestor| {
            ancestor
                .file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value == "2.0")
                && ancestor
                    .parent()
                    .and_then(Path::file_name)
                    .and_then(|value| value.to_str())
                    .is_some_and(|value| value.eq_ignore_ascii_case("Apps"))
        })
        .ok_or("MTGO image is not inside the expected ClickOnce Apps\\2.0 tree")?;
    let root = two_point_zero.join("Data");
    let canonical_root = root
        .canonicalize()
        .map_err(|error| format!("canonicalize ClickOnce data root: {error}"))?;
    if canonical_image.starts_with(&canonical_root) {
        return Err("MTGO executable unexpectedly resides inside its data root".to_owned());
    }
    Ok(canonical_root)
}

fn select_process_epoch_game_log_v1(
    data_root: &Path,
    deployment_name: &str,
    process_start_unix_millis: u128,
    captured_at_unix_millis: u128,
) -> Result<PathBuf, String> {
    if process_start_unix_millis > captured_at_unix_millis {
        return Err("MTGO process starts after the source capture".to_owned());
    }
    let canonical_root = data_root
        .canonicalize()
        .map_err(|error| format!("canonicalize Game Log search root: {error}"))?;
    let mut pending = vec![(canonical_root.clone(), 0_usize)];
    let mut examined = 0_usize;
    let mut candidates = Vec::new();
    while let Some((directory, depth)) = pending.pop() {
        if depth > MAX_VISIBLE_GAME_LOG_TREE_DEPTH_V1 {
            return Err("ClickOnce Game Log search exceeded the fixed depth bound".to_owned());
        }
        let entries = fs::read_dir(&directory)
            .map_err(|error| format!("enumerate ClickOnce Game Log directory: {error}"))?;
        for entry in entries {
            let entry =
                entry.map_err(|error| format!("read ClickOnce directory entry: {error}"))?;
            examined = examined
                .checked_add(1)
                .ok_or("ClickOnce Game Log entry count overflow")?;
            if examined > MAX_VISIBLE_GAME_LOG_TREE_ENTRIES_V1 {
                return Err("ClickOnce Game Log search exceeded the fixed entry bound".to_owned());
            }
            let file_type = entry
                .file_type()
                .map_err(|error| format!("read ClickOnce entry type: {error}"))?;
            if file_type.is_symlink() {
                return Err("ClickOnce Game Log search does not follow symbolic links".to_owned());
            }
            let path = entry.path();
            if file_type.is_dir() {
                pending.push((path, depth + 1));
                continue;
            }
            if !file_type.is_file()
                || !path.components().any(|component| {
                    component
                        .as_os_str()
                        .to_str()
                        .is_some_and(|value| value == deployment_name)
                })
                || source_id_from_game_log_filename_v1(&path).is_err()
            {
                continue;
            }
            let canonical_path = path
                .canonicalize()
                .map_err(|error| format!("canonicalize Game Log candidate: {error}"))?;
            if !canonical_path.starts_with(&canonical_root) {
                return Err("Game Log candidate escapes the ClickOnce data root".to_owned());
            }
            let metadata = fs::metadata(&canonical_path)
                .map_err(|error| format!("stat Game Log candidate: {error}"))?;
            let creation = metadata_created_unix_millis_v1(&metadata)?;
            if creation >= process_start_unix_millis && creation <= captured_at_unix_millis {
                candidates.push(canonical_path);
            }
        }
    }
    if candidates.len() != 1 {
        return Err(format!(
            "expected exactly one persisted Game Log created in the MTGO process epoch, found {}",
            candidates.len()
        ));
    }
    Ok(candidates.pop().expect("one candidate"))
}

fn source_id_from_game_log_filename_v1(path: &Path) -> Result<&str, String> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("persisted Game Log filename is not valid UTF-8")?;
    let source_id = name
        .strip_prefix("Match_GameLog_")
        .and_then(|value| value.strip_suffix(".dat"))
        .ok_or("persisted Game Log filename is not canonical")?;
    mtgo_visible_game_log_source_id_commitment_v1(source_id)
        .map_err(|error| format!("{}: {}", error.code(), error.detail()))?;
    Ok(source_id)
}

fn require_stable_file_v1(
    before: &Metadata,
    after: &Metadata,
    bytes_read: usize,
) -> Result<(), String> {
    let bytes_read = u64::try_from(bytes_read).map_err(|_| "Game Log byte length overflow")?;
    if !before.is_file() || !after.is_file() {
        return Err("persisted Game Log is no longer a regular file".to_owned());
    }
    let before_identity = stable_file_identity_v1(before)?;
    let after_identity = stable_file_identity_v1(after)?;
    if before_identity.length != bytes_read || before_identity != after_identity {
        return Err("persisted Game Log changed while it was read".to_owned());
    }
    Ok(())
}

fn stable_file_identity_v1(metadata: &Metadata) -> Result<StableFileIdentityV1, String> {
    Ok(StableFileIdentityV1 {
        length: metadata.len(),
        creation_unix_millis: metadata_created_unix_millis_v1(metadata)?,
        modification_unix_millis: metadata_modified_unix_millis_v1(metadata)?,
    })
}

fn metadata_created_unix_millis_v1(metadata: &Metadata) -> Result<u128, String> {
    system_time_unix_millis_v1(
        metadata
            .created()
            .map_err(|error| format!("read Game Log creation time: {error}"))?,
    )
}

fn metadata_modified_unix_millis_v1(metadata: &Metadata) -> Result<u128, String> {
    system_time_unix_millis_v1(
        metadata
            .modified()
            .map_err(|error| format!("read Game Log modification time: {error}"))?,
    )
}

fn system_time_unix_millis_v1(value: SystemTime) -> Result<u128, String> {
    value
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .map_err(|_| "Game Log filesystem time is before the Unix epoch".to_owned())
}

fn filetime_100ns_to_unix_millis_v1(value: u64) -> Result<u128, String> {
    let unix_ticks = value
        .checked_sub(WINDOWS_TO_UNIX_EPOCH_100NS_V1)
        .ok_or("MTGO process start FILETIME precedes the Unix epoch")?;
    Ok(u128::from(unix_ticks / 10_000))
}

fn validate_deployment_name_v1(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 256
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err("MTGO deployment directory name is not canonical".to_owned());
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_root_v1(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "mtgo-visible-game-log-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn candidate_v1(root: &Path, deployment: &str, source_id: &str) -> PathBuf {
        root.join("a")
            .join("b")
            .join(deployment)
            .join("Data")
            .join("AppFiles")
            .join("fixture")
            .join(format!("Match_GameLog_{source_id}.dat"))
    }

    #[test]
    fn sole_process_epoch_candidate_is_selected_without_exposing_source_id() {
        let root = unique_root_v1("sole");
        let deployment = "mtgo..tion_fixture_0003.0004_fixture";
        let path = candidate_v1(&root, deployment, "82ba951e-fe1f-423a-9cdb-bfe879ea2c6b");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"fixture").unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let selected =
            select_process_epoch_game_log_v1(&root, deployment, now - 10_000, now + 10_000)
                .unwrap();
        assert_eq!(selected, path.canonicalize().unwrap());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn zero_or_multiple_process_epoch_candidates_reject() {
        let root = unique_root_v1("cardinality");
        let deployment = "mtgo..tion_fixture_0003.0004_fixture";
        fs::create_dir_all(&root).unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        assert!(
            select_process_epoch_game_log_v1(&root, deployment, now - 10_000, now + 10_000)
                .is_err()
        );
        for source_id in [
            "82ba951e-fe1f-423a-9cdb-bfe879ea2c6b",
            "66f96e47-a373-44d1-bf37-e92dea9cf73b",
        ] {
            let path = candidate_v1(&root, deployment, source_id);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"fixture").unwrap();
        }
        assert!(
            select_process_epoch_game_log_v1(&root, deployment, now - 10_000, now + 10_000)
                .is_err()
        );
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn pre_process_or_cross_deployment_candidates_do_not_admit() {
        let root = unique_root_v1("epoch");
        let path = candidate_v1(
            &root,
            "another_deployment",
            "82ba951e-fe1f-423a-9cdb-bfe879ea2c6b",
        );
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"fixture").unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        assert!(select_process_epoch_game_log_v1(
            &root,
            "expected_deployment",
            now + 10_000,
            now + 20_000
        )
        .is_err());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn filename_and_filetime_conversions_are_exact() {
        let path = Path::new("Match_GameLog_82ba951e-fe1f-423a-9cdb-bfe879ea2c6b.dat");
        assert_eq!(
            source_id_from_game_log_filename_v1(path).unwrap(),
            "82ba951e-fe1f-423a-9cdb-bfe879ea2c6b"
        );
        assert!(source_id_from_game_log_filename_v1(Path::new("mtgo_game_history")).is_err());
        assert_eq!(
            filetime_100ns_to_unix_millis_v1(WINDOWS_TO_UNIX_EPOCH_100NS_V1 + 12_340_000).unwrap(),
            1_234
        );
        assert!(filetime_100ns_to_unix_millis_v1(1).is_err());
    }

    #[test]
    fn duel_title_identity_binds_only_visible_hashed_fields_and_process() {
        let process = "1".repeat(64);
        let title = "(1-on-1): Modern: Vs. VisibleOpponent Match #289088121 - Game #958808276";
        let identity =
            bind_visible_game_log_duel_title_identity_v1(title, "Modern", &process).unwrap();
        assert_eq!(
            identity.opponent_alias_sha256,
            sha256_hex_v1(b"VisibleOpponent")
        );
        assert_eq!(
            identity.visible_match_id_sha256,
            sha256_hex_v1(b"289088121")
        );
        assert_eq!(identity.visible_game_id_sha256, sha256_hex_v1(b"958808276"));
        assert_ne!(identity.identity_commitment_sha256, process);

        let another_process =
            bind_visible_game_log_duel_title_identity_v1(title, "Modern", &"2".repeat(64)).unwrap();
        assert_ne!(
            identity.identity_commitment_sha256,
            another_process.identity_commitment_sha256
        );
        assert!(bind_visible_game_log_duel_title_identity_v1(
            "(Solitaire): Freeform: Vs. VisibleOpponent",
            "Modern",
            &process
        )
        .is_err());
    }

    #[test]
    fn competitive_log_lineage_rejects_every_cross_match_substitution() {
        let process = "1".repeat(64);
        let opponent = sha256_hex_v1(b"VisibleOpponent");
        let match_id = sha256_hex_v1(b"289088121");
        let game_id = sha256_hex_v1(b"958808276");
        let source = bind_visible_game_log_duel_title_identity_v1(
            "(1-on-1): Modern: Vs. VisibleOpponent Match #289088121 - Game #958808276",
            "Modern",
            &process,
        )
        .unwrap();
        let validate = |source_process: &str,
                        semantic_opponent: Option<&str>,
                        launch_process: &str,
                        launch_opponent: &str,
                        launch_match: &str,
                        launch_game: &str| {
            validate_competitive_visible_game_log_lineage_v1(
                source_process,
                semantic_opponent,
                &source,
                launch_process,
                launch_opponent,
                launch_match,
                launch_game,
            )
        };
        validate(
            &process,
            Some(&opponent),
            &process,
            &opponent,
            &match_id,
            &game_id,
        )
        .unwrap();
        validate(&process, None, &process, &opponent, &match_id, &game_id).unwrap();
        for substitution in 0..5 {
            let other = "2".repeat(64);
            let result = match substitution {
                0 => validate(
                    &other,
                    Some(&opponent),
                    &process,
                    &opponent,
                    &match_id,
                    &game_id,
                ),
                1 => validate(
                    &process,
                    Some(&other),
                    &process,
                    &opponent,
                    &match_id,
                    &game_id,
                ),
                2 => validate(
                    &process,
                    Some(&opponent),
                    &process,
                    &other,
                    &match_id,
                    &game_id,
                ),
                3 => validate(
                    &process,
                    Some(&opponent),
                    &process,
                    &opponent,
                    &other,
                    &game_id,
                ),
                _ => validate(
                    &process,
                    Some(&opponent),
                    &process,
                    &opponent,
                    &match_id,
                    &other,
                ),
            };
            assert!(result.is_err());
        }
    }

    #[test]
    fn current_game_range_segments_match_log_and_rejects_terminal_or_wrong_game() {
        use MtgoVisibleGameLogEventKindV1::{
            CastSpell, OpeningHand, PlayedCard, WonGame, WonMatch,
        };
        assert_eq!(
            current_game_event_range_from_kinds_v1([OpeningHand, PlayedCard], 1).unwrap(),
            (0, 2)
        );
        assert_eq!(
            current_game_event_range_from_kinds_v1(
                [OpeningHand, PlayedCard, WonGame, OpeningHand, CastSpell],
                2,
            )
            .unwrap(),
            (3, 2)
        );
        assert!(current_game_event_range_from_kinds_v1(
            [OpeningHand, PlayedCard, WonGame, OpeningHand],
            1,
        )
        .is_err());
        assert!(current_game_event_range_from_kinds_v1(
            [OpeningHand, WonGame, OpeningHand, WonGame, WonMatch],
            3,
        )
        .is_err());
    }
}
