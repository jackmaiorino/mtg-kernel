use crate::{
    CheckedUntrustedMtgoActingPlayerDuelCalibrationCorpusV1, MtgoContractErrorV1,
    MtgoPlayerVisibleDuelActionV1, MtgoPlayerVisibleDuelDecisionInputV1,
    MtgoPlayerVisibleNamedCardV1, MtgoPlayerVisibleObjectRefV1, MtgoPlayerVisibleTargetRefV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

pub const MTGO_PLAYER_VISIBLE_DUEL_ANNOTATION_SET_SCHEMA_V1: u32 = 1;

const ANNOTATION_KIND_V1: &str = "mtgo_player_visible_duel_annotation_set_v1";
const INFORMATION_BOUNDARY_V1: &str = "seated_player_visible_source_frame_pixels_only_v1";
const ANNOTATION_PROTOCOL_ID_V1: &str = "mtgo-player-visible-duel-decision-annotation-protocol/v1";
const ANNOTATION_PROTOCOL_SPEC_V1: &str = concat!(
    "source=exact_checked_acting_player_duel_corpus_frame;",
    "facts=seated_player_visible_source_frame_pixels_only;",
    "payload=mtgo_player_visible_duel_decision_input_v1;",
    "roles=seated_player_or_opponent_only;",
    "own_hand_identity_count=visible_own_hand_count;",
    "known_zone_counts=within_visible_zone_counts;",
    "primary_zone_membership=unique;",
    "object_ordinals=dense_canonical_first_seen_order;",
    "object_names=bounded_consistent_visible_text;",
    "relations_targets_combat_actions=declared_objects_only;",
    "legal_actions=unique_ordered_complete_seated_player_actions;",
    "action_counts_ranges_and_ordinals=internally_consistent;",
    "review=source_state_actions_and_exact_source_only;",
    "authority=checked_untrusted_offline_evaluation_only"
);
const ANNOTATION_PROTOCOL_DOMAIN_V1: &[u8] = b"mtgo-player-visible-duel-annotation-protocol-v1";
const ANNOTATION_SET_COMMITMENT_DOMAIN_V1: &[u8] = b"mtgo-player-visible-duel-annotation-set-v1";
const MIN_REVIEW_UNIX_MILLIS_V1: u64 = 1_577_836_800_000;
const MAX_REVIEW_UNIX_MILLIS_V1: u64 = 4_102_444_800_000;
const MAX_ANNOTATION_ENTRIES_V1: usize = 4_096;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoPlayerVisibleDuelAnnotationEntryV1 {
    pub case_id: String,
    pub source_manifest_sha256: String,
    pub source_canonical_bgra8_sha256: String,
    pub source_preview_png_sha256: String,
    pub expected: MtgoPlayerVisibleDuelDecisionInputV1,
    pub source_frame_visually_reviewed: bool,
    pub visible_state_reviewed: bool,
    pub visible_legal_actions_reviewed: bool,
    pub exact_source_frame_only_used: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoPlayerVisibleDuelAnnotationSetV1 {
    pub schema_version: u32,
    pub annotation_kind: String,
    pub annotation_id: String,
    pub annotation_protocol_sha256: String,
    pub information_boundary: String,
    pub corpus_manifest_sha256: String,
    pub corpus_commitment_sha256: String,
    pub reviewer_alias_sha256: String,
    pub reviewed_at_unix_millis: u64,
    pub entries: Vec<MtgoPlayerVisibleDuelAnnotationEntryV1>,
    pub safe_for_live_semantic_evidence: bool,
    pub safe_for_model_scoring: bool,
    pub safe_for_input: bool,
}

/// Path-free, structurally checked player-visible annotations over one exact
/// acting-player duel corpus.
///
/// The review assertions remain caller-provided. This value can be used only
/// for offline perception evaluation and cannot create live semantic evidence,
/// policy-scoring authority, an action, or input authority.
///
/// It intentionally implements neither `Debug`, `Clone`, nor serde.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoPlayerVisibleDuelAnnotationSetV1;
/// fn cannot_act(value: &CheckedUntrustedMtgoPlayerVisibleDuelAnnotationSetV1) {
///     let _ = value.action_intent_v1();
///     let _ = value.canonical_pixels_v1();
/// }
/// ```
pub struct CheckedUntrustedMtgoPlayerVisibleDuelAnnotationSetV1 {
    manifest: MtgoPlayerVisibleDuelAnnotationSetV1,
    canonical_manifest_sha256: String,
    annotation_set_commitment_sha256: String,
}

impl CheckedUntrustedMtgoPlayerVisibleDuelAnnotationSetV1 {
    pub fn manifest_v1(&self) -> &MtgoPlayerVisibleDuelAnnotationSetV1 {
        &self.manifest
    }

    pub fn annotation_protocol_sha256(&self) -> &str {
        &self.manifest.annotation_protocol_sha256
    }

    pub fn corpus_manifest_sha256(&self) -> &str {
        &self.manifest.corpus_manifest_sha256
    }

    pub fn corpus_commitment_sha256(&self) -> &str {
        &self.manifest.corpus_commitment_sha256
    }

    pub fn canonical_manifest_sha256(&self) -> &str {
        &self.canonical_manifest_sha256
    }

    pub fn annotation_set_commitment_sha256(&self) -> &str {
        &self.annotation_set_commitment_sha256
    }

    pub fn entry_count(&self) -> usize {
        self.manifest.entries.len()
    }

    pub fn safe_for_live_semantic_evidence(&self) -> bool {
        false
    }

    pub fn safe_for_model_scoring(&self) -> bool {
        false
    }

    pub fn safe_for_input(&self) -> bool {
        false
    }
}

pub fn mtgo_player_visible_duel_annotation_protocol_sha256_v1() -> String {
    let mut hasher = Sha256::new();
    hasher.update(ANNOTATION_PROTOCOL_DOMAIN_V1);
    hash_part_v1(&mut hasher, ANNOTATION_PROTOCOL_ID_V1.as_bytes());
    hash_part_v1(&mut hasher, ANNOTATION_PROTOCOL_SPEC_V1.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn check_untrusted_player_visible_duel_annotation_set_v1(
    corpus: &CheckedUntrustedMtgoActingPlayerDuelCalibrationCorpusV1,
    manifest: MtgoPlayerVisibleDuelAnnotationSetV1,
) -> Result<CheckedUntrustedMtgoPlayerVisibleDuelAnnotationSetV1, MtgoContractErrorV1> {
    validate_identifier_v1(&manifest.annotation_id)?;
    if manifest.schema_version != MTGO_PLAYER_VISIBLE_DUEL_ANNOTATION_SET_SCHEMA_V1
        || manifest.annotation_kind != ANNOTATION_KIND_V1
        || manifest.information_boundary != INFORMATION_BOUNDARY_V1
        || manifest.annotation_protocol_sha256
            != mtgo_player_visible_duel_annotation_protocol_sha256_v1()
    {
        return Err(error_v1(
            "player_visible_duel_annotation_header",
            "schema, kind, information boundary, and annotation protocol must be exact",
        ));
    }
    validate_lower_hex_sha256_v1(
        "player_visible_duel_annotation_corpus_manifest",
        &manifest.corpus_manifest_sha256,
    )?;
    validate_lower_hex_sha256_v1(
        "player_visible_duel_annotation_corpus_commitment",
        &manifest.corpus_commitment_sha256,
    )?;
    validate_lower_hex_sha256_v1(
        "player_visible_duel_annotation_reviewer_alias",
        &manifest.reviewer_alias_sha256,
    )?;
    if manifest.corpus_manifest_sha256 != corpus.canonical_manifest_sha256()
        || manifest.corpus_commitment_sha256 != corpus.corpus_commitment_sha256()
    {
        return Err(error_v1(
            "player_visible_duel_annotation_corpus_mismatch",
            "annotation set must bind the exact checked duel corpus",
        ));
    }
    if !(MIN_REVIEW_UNIX_MILLIS_V1..=MAX_REVIEW_UNIX_MILLIS_V1)
        .contains(&manifest.reviewed_at_unix_millis)
    {
        return Err(error_v1(
            "player_visible_duel_annotation_review_time",
            "review timestamp is outside the supported UTC range",
        ));
    }
    if manifest.safe_for_live_semantic_evidence
        || manifest.safe_for_model_scoring
        || manifest.safe_for_input
    {
        return Err(error_v1(
            "player_visible_duel_annotation_authority",
            "checked annotations grant no live evidence, scoring, or input authority",
        ));
    }
    if manifest.entries.is_empty()
        || manifest.entries.len() > MAX_ANNOTATION_ENTRIES_V1
        || manifest.entries.len() != corpus.sample_count()
    {
        return Err(error_v1(
            "player_visible_duel_annotation_coverage",
            "annotation set must contain exactly one entry for every corpus sample",
        ));
    }

    let mut case_ids = HashSet::new();
    for (sample, entry) in corpus.manifest_v1().samples.iter().zip(&manifest.entries) {
        validate_identifier_v1(&entry.case_id)?;
        for (field, value) in [
            (
                "player_visible_duel_annotation_source_manifest",
                entry.source_manifest_sha256.as_str(),
            ),
            (
                "player_visible_duel_annotation_source_pixels",
                entry.source_canonical_bgra8_sha256.as_str(),
            ),
            (
                "player_visible_duel_annotation_source_png",
                entry.source_preview_png_sha256.as_str(),
            ),
        ] {
            validate_lower_hex_sha256_v1(field, value)?;
        }
        if !case_ids.insert(entry.case_id.as_str()) {
            return Err(error_v1(
                "player_visible_duel_annotation_duplicate_case",
                entry.case_id.clone(),
            ));
        }
        if entry.case_id != sample.sample_id
            || entry.source_manifest_sha256 != sample.source_manifest_sha256
            || entry.source_canonical_bgra8_sha256 != sample.source_canonical_bgra8_sha256
            || entry.source_preview_png_sha256 != sample.source_preview_png_sha256
        {
            return Err(error_v1(
                "player_visible_duel_annotation_source_mismatch",
                entry.case_id.clone(),
            ));
        }
        if !entry.source_frame_visually_reviewed
            || !entry.visible_state_reviewed
            || !entry.visible_legal_actions_reviewed
            || !entry.exact_source_frame_only_used
        {
            return Err(error_v1(
                "player_visible_duel_annotation_review_incomplete",
                entry.case_id.clone(),
            ));
        }
        crate::validate_player_visible_expected_v2(&entry.expected)?;
        validate_annotation_payload_v1(&entry.expected)?;
    }

    let canonical_manifest = serde_json::to_vec(&manifest).map_err(|error| {
        error_v1(
            "player_visible_duel_annotation_serialization",
            error.to_string(),
        )
    })?;
    let canonical_manifest_sha256 = format!("{:x}", Sha256::digest(&canonical_manifest));
    let mut hasher = Sha256::new();
    hasher.update(ANNOTATION_SET_COMMITMENT_DOMAIN_V1);
    hash_part_v1(&mut hasher, &canonical_manifest);
    let annotation_set_commitment_sha256 = format!("{:x}", hasher.finalize());

    Ok(CheckedUntrustedMtgoPlayerVisibleDuelAnnotationSetV1 {
        manifest,
        canonical_manifest_sha256,
        annotation_set_commitment_sha256,
    })
}

fn validate_annotation_payload_v1(
    expected: &MtgoPlayerVisibleDuelDecisionInputV1,
) -> Result<(), MtgoContractErrorV1> {
    let state = &expected.current_state;
    if state.turn == 0 || state.turn > 100_000 {
        return Err(error_v1(
            "player_visible_duel_annotation_turn",
            "gameplay annotations require a bounded nonzero turn",
        ));
    }
    if state
        .life_totals
        .iter()
        .any(|life| !(-1_000_000..=1_000_000).contains(life))
        || state.hand_counts.iter().any(|count| *count > 10_000)
        || state.library_counts.iter().any(|count| *count > 10_000)
        || state
            .mana_pools
            .iter()
            .flatten()
            .any(|amount| *amount > 100)
    {
        return Err(error_v1(
            "player_visible_duel_annotation_numeric_bounds",
            "visible life, hand, library, or mana values exceed annotation bounds",
        ));
    }
    if state.own_hand.len() != state.hand_counts[0]
        || state.known_hand_cards[0].len() > state.hand_counts[0]
        || state.known_hand_cards[1].len() > state.hand_counts[1]
        || state.known_library_cards[0].len() > state.library_counts[0]
        || state.known_library_cards[1].len() > state.library_counts[1]
    {
        return Err(error_v1(
            "player_visible_duel_annotation_zone_count",
            "visible card vectors must agree with visible hand and library counts",
        ));
    }
    for count in [
        state.battlefield[0].len(),
        state.battlefield[1].len(),
        state.graveyards[0].len(),
        state.graveyards[1].len(),
        state.exile.len(),
        state.own_hand.len(),
        state.known_library_cards[0].len(),
        state.known_library_cards[1].len(),
        state.known_hand_cards[0].len(),
        state.known_hand_cards[1].len(),
    ] {
        if count > 1_024 {
            return Err(error_v1(
                "player_visible_duel_annotation_zone_bound",
                "one visible card vector exceeds 1024 entries",
            ));
        }
    }
    if state.stack.len() > 128
        || state.visible_object_relations.len() > 2_048
        || state.combat.ordered_attackers.len() > 1_024
        || state.combat.blocker_assignments.len() > 1_024
    {
        return Err(error_v1(
            "player_visible_duel_annotation_surface_bound",
            "visible stack, relation, or combat state exceeds annotation bounds",
        ));
    }
    if state.combat.blockers_declared && !state.combat.attackers_declared {
        return Err(error_v1(
            "player_visible_duel_annotation_combat_state",
            "blockers cannot be declared before attackers",
        ));
    }

    let mut declared_refs = HashSet::new();
    let mut visible_names: HashMap<u32, &str> = HashMap::new();
    for player in 0..2 {
        let mut zone_refs = HashSet::new();
        for card in &state.battlefield[player] {
            register_named_card_v1(
                &mut declared_refs,
                &mut visible_names,
                &mut zone_refs,
                card.object_ref,
                &card.card_name,
            )?;
            if [
                card.counters.plus_one_plus_one,
                card.counters.minus_one_minus_one,
                card.counters.minus_zero_minus_one,
                card.counters.stun,
                card.counters.lore,
            ]
            .iter()
            .any(|count| *count < 0)
            {
                return Err(error_v1(
                    "player_visible_duel_annotation_counter_state",
                    "visible counter counts cannot be negative",
                ));
            }
        }
        register_named_zone_v1(
            &state.graveyards[player],
            &mut declared_refs,
            &mut visible_names,
        )?;
        register_named_zone_v1(
            &state.known_hand_cards[player],
            &mut declared_refs,
            &mut visible_names,
        )?;
        let mut known_positions = HashSet::new();
        let mut known_library_refs = HashSet::new();
        for known in &state.known_library_cards[player] {
            if usize::try_from(known.visible_known_position)
                .map_or(true, |position| position >= state.library_counts[player])
                || !known_positions.insert(known.visible_known_position)
            {
                return Err(error_v1(
                    "player_visible_duel_annotation_library_position",
                    "known library positions must be unique and inside the visible library count",
                ));
            }
            register_named_card_v1(
                &mut declared_refs,
                &mut visible_names,
                &mut known_library_refs,
                known.card.object_ref,
                &known.card.card_name,
            )?;
        }
    }
    register_named_zone_v1(&state.own_hand, &mut declared_refs, &mut visible_names)?;

    let mut exile_refs = HashSet::new();
    for card in &state.exile {
        if !exile_refs.insert(card.object_ref.visible_ordinal) {
            return Err(error_v1(
                "player_visible_duel_annotation_duplicate_zone_object",
                "one visible zone contains the same object more than once",
            ));
        }
        declared_refs.insert(card.object_ref.visible_ordinal);
        if let Some(name) = &card.visible_card_name {
            validate_card_name_v1(name)?;
            register_name_v1(&mut visible_names, card.object_ref, name)?;
        }
    }
    let mut primary_zone_refs = HashSet::new();
    let mut register_primary_zone_ref = |object_ref: MtgoPlayerVisibleObjectRefV1| {
        if !primary_zone_refs.insert(object_ref.visible_ordinal) {
            return Err(error_v1(
                "player_visible_duel_annotation_cross_zone_object",
                "one visible object ordinal cannot occupy more than one primary zone",
            ));
        }
        Ok(())
    };
    for player in 0..2 {
        for card in &state.battlefield[player] {
            register_primary_zone_ref(card.object_ref)?;
        }
        for card in &state.graveyards[player] {
            register_primary_zone_ref(card.object_ref)?;
        }
        for card in &state.known_hand_cards[player] {
            register_primary_zone_ref(card.object_ref)?;
        }
        for known in &state.known_library_cards[player] {
            register_primary_zone_ref(known.card.object_ref)?;
        }
    }
    for card in &state.own_hand {
        register_primary_zone_ref(card.object_ref)?;
    }
    for card in &state.exile {
        register_primary_zone_ref(card.object_ref)?;
    }
    let mut stack_positions = HashSet::new();
    for item in &state.stack {
        if !stack_positions.insert(item.visible_stack_position) {
            return Err(error_v1(
                "player_visible_duel_annotation_stack_position",
                "visible stack positions must be unique",
            ));
        }
        declared_refs.insert(item.source_object_ref.visible_ordinal);
        if let Some(name) = &item.visible_source_name {
            validate_card_name_v1(name)?;
            register_name_v1(&mut visible_names, item.source_object_ref, name)?;
        }
    }
    if declared_refs.len() > 4_096
        || declared_refs
            .iter()
            .copied()
            .max()
            .is_some_and(|maximum| maximum as usize + 1 != declared_refs.len())
    {
        return Err(error_v1(
            "player_visible_duel_annotation_object_ordinals",
            "visible object ordinals must be dense from zero and contain at most 4096 objects",
        ));
    }
    validate_canonical_object_ordinals_v1(expected)?;

    for item in &state.stack {
        for target in &item.visible_targets {
            require_target_declared_v1(target, &declared_refs)?;
        }
    }
    let mut relation_records = HashSet::new();
    for relation in &state.visible_object_relations {
        let (object, related) = match relation {
            crate::MtgoPlayerVisibleObjectRelationV1::AttachedTo {
                object,
                attached_to,
            } => (*object, *attached_to),
            crate::MtgoPlayerVisibleObjectRelationV1::ExiledBy { object, exiled_by } => {
                (*object, *exiled_by)
            }
        };
        require_ref_declared_v1(object, &declared_refs)?;
        require_ref_declared_v1(related, &declared_refs)?;
        if object == related
            || !relation_records.insert(serde_json::to_string(relation).map_err(|error| {
                error_v1(
                    "player_visible_duel_annotation_relation_serialization",
                    error.to_string(),
                )
            })?)
        {
            return Err(error_v1(
                "player_visible_duel_annotation_relation",
                "visible object relations must be unique and cannot be self-referential",
            ));
        }
    }
    validate_combat_v1(expected, &declared_refs)?;
    validate_actions_v1(expected, &declared_refs)?;
    Ok(())
}

fn register_named_zone_v1<'a>(
    cards: &'a [MtgoPlayerVisibleNamedCardV1],
    declared_refs: &mut HashSet<u32>,
    visible_names: &mut HashMap<u32, &'a str>,
) -> Result<(), MtgoContractErrorV1> {
    let mut zone_refs = HashSet::new();
    for card in cards {
        register_named_card_v1(
            declared_refs,
            visible_names,
            &mut zone_refs,
            card.object_ref,
            &card.card_name,
        )?;
    }
    Ok(())
}

fn register_named_card_v1<'a>(
    declared_refs: &mut HashSet<u32>,
    visible_names: &mut HashMap<u32, &'a str>,
    zone_refs: &mut HashSet<u32>,
    object_ref: MtgoPlayerVisibleObjectRefV1,
    card_name: &'a str,
) -> Result<(), MtgoContractErrorV1> {
    validate_card_name_v1(card_name)?;
    if !zone_refs.insert(object_ref.visible_ordinal) {
        return Err(error_v1(
            "player_visible_duel_annotation_duplicate_zone_object",
            "one visible zone contains the same object more than once",
        ));
    }
    declared_refs.insert(object_ref.visible_ordinal);
    register_name_v1(visible_names, object_ref, card_name)
}

fn register_name_v1<'a>(
    visible_names: &mut HashMap<u32, &'a str>,
    object_ref: MtgoPlayerVisibleObjectRefV1,
    card_name: &'a str,
) -> Result<(), MtgoContractErrorV1> {
    if visible_names
        .insert(object_ref.visible_ordinal, card_name)
        .is_some_and(|prior| prior != card_name)
    {
        return Err(error_v1(
            "player_visible_duel_annotation_object_name",
            "one visible object ordinal cannot have conflicting card names",
        ));
    }
    Ok(())
}

fn validate_card_name_v1(card_name: &str) -> Result<(), MtgoContractErrorV1> {
    if card_name.is_empty() || card_name.len() > 256 || card_name.chars().any(char::is_control) {
        return Err(error_v1(
            "player_visible_duel_annotation_card_name",
            "visible card names must be bounded non-control text",
        ));
    }
    Ok(())
}

fn require_ref_declared_v1(
    object_ref: MtgoPlayerVisibleObjectRefV1,
    declared_refs: &HashSet<u32>,
) -> Result<(), MtgoContractErrorV1> {
    if !declared_refs.contains(&object_ref.visible_ordinal) {
        return Err(error_v1(
            "player_visible_duel_annotation_unknown_object",
            "a visible relation, target, combat assignment, or action cites an absent object",
        ));
    }
    Ok(())
}

fn require_target_declared_v1(
    target: &MtgoPlayerVisibleTargetRefV1,
    declared_refs: &HashSet<u32>,
) -> Result<(), MtgoContractErrorV1> {
    if let MtgoPlayerVisibleTargetRefV1::Object { object } = target {
        require_ref_declared_v1(*object, declared_refs)?;
    }
    Ok(())
}

fn validate_combat_v1(
    expected: &MtgoPlayerVisibleDuelDecisionInputV1,
    declared_refs: &HashSet<u32>,
) -> Result<(), MtgoContractErrorV1> {
    let combat = &expected.current_state.combat;
    let mut attackers = HashSet::new();
    for attacker in &combat.ordered_attackers {
        require_ref_declared_v1(*attacker, declared_refs)?;
        if !attackers.insert(attacker.visible_ordinal) {
            return Err(error_v1(
                "player_visible_duel_annotation_combat_state",
                "declared attackers must be unique",
            ));
        }
    }
    let mut assigned_attackers = HashSet::new();
    for assignment in &combat.blocker_assignments {
        require_ref_declared_v1(assignment.attacker, declared_refs)?;
        if !attackers.contains(&assignment.attacker.visible_ordinal)
            || !assigned_attackers.insert(assignment.attacker.visible_ordinal)
        {
            return Err(error_v1(
                "player_visible_duel_annotation_combat_state",
                "blocker assignments must uniquely reference declared attackers",
            ));
        }
        let mut blockers = HashSet::new();
        for blocker in &assignment.ordered_blockers {
            require_ref_declared_v1(*blocker, declared_refs)?;
            if !blockers.insert(blocker.visible_ordinal) {
                return Err(error_v1(
                    "player_visible_duel_annotation_combat_state",
                    "one blocker assignment cannot repeat a blocker",
                ));
            }
        }
    }
    Ok(())
}

fn validate_actions_v1(
    expected: &MtgoPlayerVisibleDuelDecisionInputV1,
    declared_refs: &HashSet<u32>,
) -> Result<(), MtgoContractErrorV1> {
    let mut actions = HashSet::new();
    for (choice_index, action) in expected.ordered_legal_actions.iter().enumerate() {
        let encoded = serde_json::to_string(action).map_err(|error| {
            error_v1(
                "player_visible_duel_annotation_action_serialization",
                error.to_string(),
            )
        })?;
        if !actions.insert(encoded) {
            return Err(error_v1(
                "player_visible_duel_annotation_duplicate_action",
                "visible legal actions must be unique",
            ));
        }
        let visible_choice_ordinal = u32::try_from(choice_index).map_err(|_| {
            error_v1(
                "player_visible_duel_annotation_action_count",
                "visible action index does not fit u32",
            )
        })?;
        validate_action_v1(action, declared_refs, visible_choice_ordinal)?;
    }
    Ok(())
}

fn validate_action_v1(
    action: &MtgoPlayerVisibleDuelActionV1,
    declared_refs: &HashSet<u32>,
    expected_choice_ordinal: u32,
) -> Result<(), MtgoContractErrorV1> {
    use MtgoPlayerVisibleDuelActionV1 as A;
    match action {
        A::Pass { .. } | A::ChooseOptionalCostUse { .. } | A::ChooseOptionalCostWhich { .. } => {}
        A::PlayLand { source, .. }
        | A::CastSpell { source, .. }
        | A::ActivateManaAbility { source, .. }
        | A::PlotSpell { source, .. }
        | A::ChooseCastMode { source, .. }
        | A::ChooseKicker { source, .. }
        | A::FinishEffectSelection { source, .. }
        | A::ChooseEffectColor { source, .. }
        | A::ChooseEffectBoolean { source, .. }
        | A::FinishTargetSelection { source, .. }
        | A::ChooseSpellCopyPayment { source, .. }
        | A::ChooseSpellCopyRetarget { source, .. } => {
            require_ref_declared_v1(*source, declared_refs)?
        }
        A::ActivateAbility {
            source,
            visible_choice_ordinal,
            ..
        }
        | A::ChooseSpellMode {
            source,
            visible_choice_ordinal,
            ..
        }
        | A::ChooseEffectOption {
            source,
            visible_choice_ordinal,
            ..
        } => {
            require_ref_declared_v1(*source, declared_refs)?;
            if *visible_choice_ordinal != expected_choice_ordinal {
                return Err(action_shape_error_v1());
            }
        }
        A::ChooseTarget {
            source,
            remaining,
            target,
            ..
        } => {
            require_ref_declared_v1(*source, declared_refs)?;
            require_target_declared_v1(target, declared_refs)?;
            if *remaining == 0 {
                return Err(action_shape_error_v1());
            }
        }
        A::ChooseCostTarget {
            source,
            remaining,
            candidate,
            ..
        } => {
            require_ref_declared_v1(*source, declared_refs)?;
            require_ref_declared_v1(*candidate, declared_refs)?;
            if *remaining == 0 {
                return Err(action_shape_error_v1());
            }
        }
        A::ChooseEffectTarget {
            source,
            target,
            selected_count,
            min_targets,
            max_targets,
            ..
        } => {
            require_ref_declared_v1(*source, declared_refs)?;
            require_target_declared_v1(target, declared_refs)?;
            if min_targets > max_targets || selected_count >= max_targets {
                return Err(action_shape_error_v1());
            }
        }
        A::ChooseEffectNumber {
            source,
            number,
            minimum,
            maximum,
            ..
        } => {
            require_ref_declared_v1(*source, declared_refs)?;
            if minimum > maximum || number < minimum || number > maximum {
                return Err(action_shape_error_v1());
            }
        }
        A::ChooseMadnessCast { card, .. } => require_ref_declared_v1(*card, declared_refs)?,
        A::Discard { cards, .. } => {
            if cards.is_empty() || !refs_are_unique_v1(cards) {
                return Err(action_shape_error_v1());
            }
            for card in cards {
                require_ref_declared_v1(*card, declared_refs)?;
            }
        }
        A::DeclareAttackers { attackers, .. } => {
            if !refs_are_unique_v1(attackers) {
                return Err(action_shape_error_v1());
            }
            for attacker in attackers {
                require_ref_declared_v1(*attacker, declared_refs)?;
            }
        }
        A::DeclareBlockersForAttacker {
            attacker, blockers, ..
        } => {
            require_ref_declared_v1(*attacker, declared_refs)?;
            if !refs_are_unique_v1(blockers) {
                return Err(action_shape_error_v1());
            }
            for blocker in blockers {
                require_ref_declared_v1(*blocker, declared_refs)?;
            }
        }
        A::ChooseAttackerInclusion { attacker, .. } => {
            require_ref_declared_v1(*attacker, declared_refs)?
        }
        A::ChooseBlockerInclusion {
            attacker, blocker, ..
        } => {
            require_ref_declared_v1(*attacker, declared_refs)?;
            require_ref_declared_v1(*blocker, declared_refs)?;
        }
        A::OrderTriggers {
            pending_sources,
            ordered_sources,
            ..
        } => {
            let pending = pending_sources
                .iter()
                .map(|source| source.visible_ordinal)
                .collect::<HashSet<_>>();
            let ordered = ordered_sources
                .iter()
                .map(|source| source.visible_ordinal)
                .collect::<HashSet<_>>();
            if pending_sources.len() < 2
                || pending.len() != pending_sources.len()
                || ordered.len() != ordered_sources.len()
                || pending != ordered
            {
                return Err(action_shape_error_v1());
            }
            for source in pending_sources {
                require_ref_declared_v1(*source, declared_refs)?;
            }
        }
    }
    Ok(())
}

fn refs_are_unique_v1(refs: &[MtgoPlayerVisibleObjectRefV1]) -> bool {
    let unique = refs
        .iter()
        .map(|object_ref| object_ref.visible_ordinal)
        .collect::<HashSet<_>>();
    unique.len() == refs.len()
}

fn validate_canonical_object_ordinals_v1(
    expected: &MtgoPlayerVisibleDuelDecisionInputV1,
) -> Result<(), MtgoContractErrorV1> {
    let state = &expected.current_state;
    let mut next_expected = 0_u32;
    let mut seen = HashSet::new();
    let mut observe = |object_ref: MtgoPlayerVisibleObjectRefV1| {
        if seen.insert(object_ref.visible_ordinal) {
            if object_ref.visible_ordinal != next_expected {
                return Err(error_v1(
                    "player_visible_duel_annotation_object_order",
                    "first-seen visible object ordinals must follow the canonical traversal order",
                ));
            }
            next_expected = next_expected.checked_add(1).ok_or_else(|| {
                error_v1(
                    "player_visible_duel_annotation_object_order",
                    "visible object ordinal overflow",
                )
            })?;
        }
        Ok(())
    };
    for player in 0..2 {
        for card in &state.battlefield[player] {
            observe(card.object_ref)?;
        }
    }
    for player in 0..2 {
        for card in &state.graveyards[player] {
            observe(card.object_ref)?;
        }
    }
    for card in &state.exile {
        observe(card.object_ref)?;
    }
    for item in &state.stack {
        observe(item.source_object_ref)?;
        for target in &item.visible_targets {
            if let MtgoPlayerVisibleTargetRefV1::Object { object } = target {
                observe(*object)?;
            }
        }
    }
    for card in &state.own_hand {
        observe(card.object_ref)?;
    }
    for player in 0..2 {
        for known in &state.known_library_cards[player] {
            observe(known.card.object_ref)?;
        }
    }
    for player in 0..2 {
        for card in &state.known_hand_cards[player] {
            observe(card.object_ref)?;
        }
    }
    for attacker in &state.combat.ordered_attackers {
        observe(*attacker)?;
    }
    for assignment in &state.combat.blocker_assignments {
        observe(assignment.attacker)?;
        for blocker in &assignment.ordered_blockers {
            observe(*blocker)?;
        }
    }
    for relation in &state.visible_object_relations {
        match relation {
            crate::MtgoPlayerVisibleObjectRelationV1::AttachedTo {
                object,
                attached_to,
            } => {
                observe(*object)?;
                observe(*attached_to)?;
            }
            crate::MtgoPlayerVisibleObjectRelationV1::ExiledBy { object, exiled_by } => {
                observe(*object)?;
                observe(*exiled_by)?;
            }
        }
    }
    Ok(())
}

fn action_shape_error_v1() -> MtgoContractErrorV1 {
    error_v1(
        "player_visible_duel_annotation_action_shape",
        "one visible legal action has inconsistent counts, bounds, or object choices",
    )
}

fn validate_identifier_v1(value: &str) -> Result<(), MtgoContractErrorV1> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(error_v1(
            "player_visible_duel_annotation_id",
            "identifier must contain 1 to 128 safe ASCII characters",
        ));
    }
    Ok(())
}

fn validate_lower_hex_sha256_v1(
    field: &'static str,
    value: &str,
) -> Result<(), MtgoContractErrorV1> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(error_v1(field, "expected lowercase SHA-256"));
    }
    Ok(())
}

fn hash_part_v1(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

fn error_v1(code: &'static str, detail: impl Into<String>) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, detail)
}

#[cfg(test)]
pub(crate) fn checked_untrusted_player_visible_duel_annotation_set_for_test_v1(
    corpus: &CheckedUntrustedMtgoActingPlayerDuelCalibrationCorpusV1,
    expected: MtgoPlayerVisibleDuelDecisionInputV1,
) -> CheckedUntrustedMtgoPlayerVisibleDuelAnnotationSetV1 {
    let sample = &corpus.manifest_v1().samples[0];
    check_untrusted_player_visible_duel_annotation_set_v1(
        corpus,
        MtgoPlayerVisibleDuelAnnotationSetV1 {
            schema_version: MTGO_PLAYER_VISIBLE_DUEL_ANNOTATION_SET_SCHEMA_V1,
            annotation_kind: ANNOTATION_KIND_V1.to_owned(),
            annotation_id: "duel-visible-annotations-test-v1".to_owned(),
            annotation_protocol_sha256: mtgo_player_visible_duel_annotation_protocol_sha256_v1(),
            information_boundary: INFORMATION_BOUNDARY_V1.to_owned(),
            corpus_manifest_sha256: corpus.canonical_manifest_sha256().to_owned(),
            corpus_commitment_sha256: corpus.corpus_commitment_sha256().to_owned(),
            reviewer_alias_sha256: "a".repeat(64),
            reviewed_at_unix_millis: 1_786_000_000_000,
            entries: vec![MtgoPlayerVisibleDuelAnnotationEntryV1 {
                case_id: sample.sample_id.clone(),
                source_manifest_sha256: sample.source_manifest_sha256.clone(),
                source_canonical_bgra8_sha256: sample.source_canonical_bgra8_sha256.clone(),
                source_preview_png_sha256: sample.source_preview_png_sha256.clone(),
                expected,
                source_frame_visually_reviewed: true,
                visible_state_reviewed: true,
                visible_legal_actions_reviewed: true,
                exact_source_frame_only_used: true,
            }],
            safe_for_live_semantic_evidence: false,
            safe_for_model_scoring: false,
            safe_for_input: false,
        },
    )
    .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        build_checked_untrusted_acting_player_duel_calibration_corpus_v1,
        checked_untrusted_dxgi_artifact_for_test_v1, MtgoDxgiCaptureRoleV2,
        MtgoPlayerRelativeRoleV1, MtgoPlayerVisibleCombatStateV1, MtgoPlayerVisibleDuelActionV1,
        MtgoPlayerVisibleDuelStateV1, ZoneIndependentStepV1,
    };

    fn expected_v1() -> MtgoPlayerVisibleDuelDecisionInputV1 {
        MtgoPlayerVisibleDuelDecisionInputV1 {
            current_state: MtgoPlayerVisibleDuelStateV1 {
                acting_player: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                turn: 1,
                phase: ZoneIndependentStepV1::Main1,
                active_player: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                priority_player: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                initiative: None,
                life_totals: [20, 20],
                mana_pools: [[0; 6]; 2],
                hand_counts: [0, 0],
                library_counts: [53, 53],
                battlefield: [Vec::new(), Vec::new()],
                graveyards: [Vec::new(), Vec::new()],
                exile: Vec::new(),
                stack: Vec::new(),
                combat: MtgoPlayerVisibleCombatStateV1 {
                    attackers_declared: false,
                    blockers_declared: false,
                    ordered_attackers: Vec::new(),
                    blocker_assignments: Vec::new(),
                },
                visible_object_relations: Vec::new(),
                own_hand: Vec::new(),
                known_library_cards: [Vec::new(), Vec::new()],
                known_hand_cards: [Vec::new(), Vec::new()],
            },
            ordered_legal_actions: vec![MtgoPlayerVisibleDuelActionV1::Pass {
                actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
            }],
        }
    }

    fn fixture_v1() -> (
        CheckedUntrustedMtgoActingPlayerDuelCalibrationCorpusV1,
        MtgoPlayerVisibleDuelAnnotationSetV1,
    ) {
        let source =
            checked_untrusted_dxgi_artifact_for_test_v1(MtgoDxgiCaptureRoleV2::ActingPlayerDuel);
        let corpus = build_checked_untrusted_acting_player_duel_calibration_corpus_v1(
            "annotation-test-corpus-v1",
            &[&source],
        )
        .unwrap();
        let sample = &corpus.manifest_v1().samples[0];
        let manifest = MtgoPlayerVisibleDuelAnnotationSetV1 {
            schema_version: MTGO_PLAYER_VISIBLE_DUEL_ANNOTATION_SET_SCHEMA_V1,
            annotation_kind: ANNOTATION_KIND_V1.to_owned(),
            annotation_id: "duel-visible-annotations-v1".to_owned(),
            annotation_protocol_sha256: mtgo_player_visible_duel_annotation_protocol_sha256_v1(),
            information_boundary: INFORMATION_BOUNDARY_V1.to_owned(),
            corpus_manifest_sha256: corpus.canonical_manifest_sha256().to_owned(),
            corpus_commitment_sha256: corpus.corpus_commitment_sha256().to_owned(),
            reviewer_alias_sha256: "a".repeat(64),
            reviewed_at_unix_millis: 1_786_000_000_000,
            entries: vec![MtgoPlayerVisibleDuelAnnotationEntryV1 {
                case_id: sample.sample_id.clone(),
                source_manifest_sha256: sample.source_manifest_sha256.clone(),
                source_canonical_bgra8_sha256: sample.source_canonical_bgra8_sha256.clone(),
                source_preview_png_sha256: sample.source_preview_png_sha256.clone(),
                expected: expected_v1(),
                source_frame_visually_reviewed: true,
                visible_state_reviewed: true,
                visible_legal_actions_reviewed: true,
                exact_source_frame_only_used: true,
            }],
            safe_for_live_semantic_evidence: false,
            safe_for_model_scoring: false,
            safe_for_input: false,
        };
        (corpus, manifest)
    }

    #[test]
    fn exact_player_visible_annotations_validate_without_runtime_authority() {
        let (corpus, manifest) = fixture_v1();
        let checked =
            check_untrusted_player_visible_duel_annotation_set_v1(&corpus, manifest).unwrap();
        assert_eq!(checked.entry_count(), 1);
        assert_eq!(
            checked.annotation_protocol_sha256(),
            mtgo_player_visible_duel_annotation_protocol_sha256_v1()
        );
        assert!(!checked.safe_for_live_semantic_evidence());
        assert!(!checked.safe_for_model_scoring());
        assert!(!checked.safe_for_input());
    }

    #[test]
    fn source_substitution_and_incomplete_review_fail_closed() {
        let (corpus, mut wrong_source) = fixture_v1();
        wrong_source.entries[0].source_preview_png_sha256 = "b".repeat(64);
        let error =
            match check_untrusted_player_visible_duel_annotation_set_v1(&corpus, wrong_source) {
                Ok(_) => panic!("source substitution must fail"),
                Err(error) => error,
            };
        assert_eq!(
            error.code(),
            "player_visible_duel_annotation_source_mismatch"
        );

        let (corpus, mut incomplete) = fixture_v1();
        incomplete.entries[0].exact_source_frame_only_used = false;
        let error = match check_untrusted_player_visible_duel_annotation_set_v1(&corpus, incomplete)
        {
            Ok(_) => panic!("incomplete review must fail"),
            Err(error) => error,
        };
        assert_eq!(
            error.code(),
            "player_visible_duel_annotation_review_incomplete"
        );
    }

    #[test]
    fn annotations_reject_authority_and_unknown_fields() {
        let (corpus, mut authority) = fixture_v1();
        authority.safe_for_model_scoring = true;
        let error = match check_untrusted_player_visible_duel_annotation_set_v1(&corpus, authority)
        {
            Ok(_) => panic!("authority claim must fail"),
            Err(error) => error,
        };
        assert_eq!(error.code(), "player_visible_duel_annotation_authority");

        let (_, manifest) = fixture_v1();
        let mut json = serde_json::to_value(manifest).unwrap();
        json.as_object_mut()
            .unwrap()
            .insert("process_memory".to_owned(), serde_json::Value::Bool(true));
        assert!(serde_json::from_value::<MtgoPlayerVisibleDuelAnnotationSetV1>(json).is_err());
    }

    #[test]
    fn impossible_visible_payloads_fail_closed() {
        let (corpus, mut wrong_hand_count) = fixture_v1();
        wrong_hand_count.entries[0]
            .expected
            .current_state
            .hand_counts[0] = 1;
        let error = match check_untrusted_player_visible_duel_annotation_set_v1(
            &corpus,
            wrong_hand_count,
        ) {
            Ok(_) => panic!("hand count without visible own-hand identity must fail"),
            Err(error) => error,
        };
        assert_eq!(error.code(), "player_visible_duel_annotation_zone_count");

        let (corpus, mut unknown_object) = fixture_v1();
        unknown_object.entries[0].expected.ordered_legal_actions =
            vec![MtgoPlayerVisibleDuelActionV1::PlayLand {
                actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                source: MtgoPlayerVisibleObjectRefV1 {
                    visible_ordinal: 42,
                },
            }];
        let error =
            match check_untrusted_player_visible_duel_annotation_set_v1(&corpus, unknown_object) {
                Ok(_) => panic!("action referring to an absent object must fail"),
                Err(error) => error,
            };
        assert_eq!(
            error.code(),
            "player_visible_duel_annotation_unknown_object"
        );

        let (corpus, mut duplicate_action) = fixture_v1();
        duplicate_action.entries[0]
            .expected
            .ordered_legal_actions
            .push(MtgoPlayerVisibleDuelActionV1::Pass {
                actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
            });
        let error = match check_untrusted_player_visible_duel_annotation_set_v1(
            &corpus,
            duplicate_action,
        ) {
            Ok(_) => panic!("duplicate legal actions must fail"),
            Err(error) => error,
        };
        assert_eq!(
            error.code(),
            "player_visible_duel_annotation_duplicate_action"
        );

        let (corpus, mut cross_zone_object) = fixture_v1();
        cross_zone_object.entries[0]
            .expected
            .current_state
            .hand_counts[0] = 1;
        let card = MtgoPlayerVisibleNamedCardV1 {
            object_ref: MtgoPlayerVisibleObjectRefV1 { visible_ordinal: 0 },
            card_name: "Island".to_owned(),
        };
        cross_zone_object.entries[0]
            .expected
            .current_state
            .own_hand
            .push(card.clone());
        cross_zone_object.entries[0]
            .expected
            .current_state
            .graveyards[0]
            .push(card);
        let error =
            match check_untrusted_player_visible_duel_annotation_set_v1(&corpus, cross_zone_object)
            {
                Ok(_) => panic!("one object occupying two primary zones must fail"),
                Err(error) => error,
            };
        assert_eq!(
            error.code(),
            "player_visible_duel_annotation_cross_zone_object"
        );
    }
}
