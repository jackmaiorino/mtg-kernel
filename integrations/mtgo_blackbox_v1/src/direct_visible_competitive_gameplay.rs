use crate::{
    competitive_match_gameplay_authorization_commitment_v1,
    competitive_mode_authorization_commitment_v1,
    validate_competitive_match_gameplay_authorization_v1,
    CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    CheckedUntrustedMtgoRefreshedDirectVisibleSelectionV1, MtgoAuthorizationScopeV1,
    MtgoCompetitiveEventKindV1, MtgoCompetitiveLifecyclePhaseV1,
    MtgoCompetitiveMatchGameplayAuthorizationV1, MtgoContractErrorV1,
    MtgoPlayerVisibleConfirmedDuelDecisionV1, MtgoPlayerVisibleDuelActionV1, MtgoSizePxV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MTGO_DIRECT_VISIBLE_COMPETITIVE_OBSERVATION_BRACKET_SCHEMA_V1: u32 = 1;

const DIRECT_VISIBLE_COMPETITIVE_SCOPE_DOMAIN_V1: &[u8] =
    b"mtgo-direct-visible-competitive-gameplay-scope-v1";
const INFORMATION_BOUNDARY_V1: &str = "seated_player_visible_ui_equivalent_only_v1";
const CAPTURE_ROLE_V1: &str = "acting_player_duel";

/// Commitments for the composed visible frames surrounding one direct-source
/// observe, score, and exact-refresh transaction. The after frame is the
/// competitive lifecycle source frame. Decision-relevant visible regions must
/// be byte-identical across the bracket, while unrelated chrome such as clocks
/// may change in the whole-frame hashes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoDirectVisibleCompetitiveObservationBracketV1 {
    pub schema_version: u32,
    pub information_boundary: String,
    pub capture_role: String,
    pub before_frame_id: u64,
    pub before_frame_sequence: u64,
    pub before_frame_sha256: String,
    pub after_frame_id: u64,
    pub after_frame_sequence: u64,
    pub after_frame_sha256: String,
    pub client_size_px: MtgoSizePxV1,
    pub before_decision_regions_sha256: String,
    pub after_decision_regions_sha256: String,
    pub client_identity_commitment_sha256: String,
    pub broker_binary_sha256: String,
    pub producer_binary_sha256: String,
    pub visible_equivalence_profile_commitment_sha256: String,
    pub producer_first_export_is_player_visible_schema: bool,
    pub raw_source_values_emitted: bool,
    pub internal_identifiers_emitted: bool,
    pub bracket_complete: bool,
}

/// One refreshed direct-source model choice bound to the exact approved
/// account, permission, League or Challenge match, game, deployment, and a
/// stable player-visible frame bracket.
///
/// Client objects and identifiers do not enter this value. It is an offline,
/// checked-untrusted ownership join only. It has no dispatch, input, process,
/// event-entry, spending, or postcondition conversion.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoDirectVisibleCompetitiveActionPlanV1;
/// fn cannot_act(value: CheckedUntrustedMtgoDirectVisibleCompetitiveActionPlanV1) {
///     let _ = value.dispatch();
///     let _ = value.client_action();
///     let _ = value.process_handle();
/// }
/// ```
pub struct CheckedUntrustedMtgoDirectVisibleCompetitiveActionPlanV1 {
    _selection: CheckedUntrustedMtgoRefreshedDirectVisibleSelectionV1,
    _lifecycle: CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    event_kind: MtgoCompetitiveEventKindV1,
    game_number: u8,
    selected_index: usize,
    selected_action: MtgoPlayerVisibleDuelActionV1,
    confirmed_decision: MtgoPlayerVisibleConfirmedDuelDecisionV1,
    mode_authorization_commitment_sha256: String,
    gameplay_authorization_commitment_sha256: String,
    observation_bracket_commitment_sha256: String,
    direct_competitive_scope_commitment_sha256: String,
}

impl CheckedUntrustedMtgoDirectVisibleCompetitiveActionPlanV1 {
    pub fn event_kind_v1(&self) -> MtgoCompetitiveEventKindV1 {
        self.event_kind
    }

    pub fn game_number_v1(&self) -> u8 {
        self.game_number
    }

    pub fn selected_index_v1(&self) -> usize {
        self.selected_index
    }

    pub fn selected_action_v1(&self) -> &MtgoPlayerVisibleDuelActionV1 {
        &self.selected_action
    }

    pub fn player_visible_confirmed_decision_v1(
        &self,
    ) -> &MtgoPlayerVisibleConfirmedDuelDecisionV1 {
        &self.confirmed_decision
    }

    pub fn mode_authorization_commitment_sha256_v1(&self) -> &str {
        &self.mode_authorization_commitment_sha256
    }

    pub fn gameplay_authorization_commitment_sha256_v1(&self) -> &str {
        &self.gameplay_authorization_commitment_sha256
    }

    pub fn observation_bracket_commitment_sha256_v1(&self) -> &str {
        &self.observation_bracket_commitment_sha256
    }

    pub fn direct_competitive_scope_commitment_sha256_v1(&self) -> &str {
        &self.direct_competitive_scope_commitment_sha256
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

pub fn bind_refreshed_direct_visible_selection_to_competitive_match_v1(
    selection: CheckedUntrustedMtgoRefreshedDirectVisibleSelectionV1,
    bracket: MtgoDirectVisibleCompetitiveObservationBracketV1,
    lifecycle: CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    mode_authorization: &MtgoAuthorizationScopeV1,
    gameplay_authorization: &MtgoCompetitiveMatchGameplayAuthorizationV1,
) -> Result<CheckedUntrustedMtgoDirectVisibleCompetitiveActionPlanV1, MtgoContractErrorV1> {
    if lifecycle.phase() != MtgoCompetitiveLifecyclePhaseV1::MatchInProgress {
        return Err(error_v1(
            "direct_visible_competitive_lifecycle_phase",
            "direct model actions require the visible match-in-progress phase",
        ));
    }
    let event_kind = lifecycle.event_kind();
    let game_number = lifecycle
        .game_number_v1()
        .expect("validated match-in-progress lifecycle has a game number");
    let mode_authorization_commitment_sha256 =
        competitive_mode_authorization_commitment_v1(mode_authorization, event_kind)?;
    validate_competitive_match_gameplay_authorization_v1(
        &lifecycle,
        mode_authorization,
        gameplay_authorization,
    )?;
    validate_bracket_v1(&bracket, &lifecycle)?;

    let gameplay_authorization_commitment_sha256 =
        competitive_match_gameplay_authorization_commitment_v1(gameplay_authorization)?;
    let bracket_json = serde_json::to_vec(&bracket).map_err(|error| {
        error_v1(
            "direct_visible_competitive_bracket_serialization",
            error.to_string(),
        )
    })?;
    let observation_bracket_commitment_sha256 = commitment_v1(
        b"mtgo-direct-visible-competitive-observation-bracket-v1",
        &[&bracket_json],
    );
    let selected_action = selection.selected_action_v1().clone();
    let confirmed_decision = selection.player_visible_confirmed_decision_v1().clone();
    let selected_index = selection.selected_index_v1();
    let direct_competitive_scope_commitment_sha256 = commitment_v1(
        DIRECT_VISIBLE_COMPETITIVE_SCOPE_DOMAIN_V1,
        &[
            selection.exact_producer_result_sha256_v1().as_bytes(),
            selection.model_input_commitment_sha256_v1().as_bytes(),
            selection.deployment_commitment_sha256_v1().as_bytes(),
            selection.selection_commitment_sha256_v1().as_bytes(),
            selection.refresh_commitment_sha256_v1().as_bytes(),
            lifecycle.snapshot_commitment_sha256().as_bytes(),
            observation_bracket_commitment_sha256.as_bytes(),
            mode_authorization_commitment_sha256.as_bytes(),
            gameplay_authorization_commitment_sha256.as_bytes(),
            b"checked_untrusted_direct_visible_no_dispatch_or_entry_authority",
        ],
    );

    Ok(CheckedUntrustedMtgoDirectVisibleCompetitiveActionPlanV1 {
        _selection: selection,
        _lifecycle: lifecycle,
        event_kind,
        game_number,
        selected_index,
        selected_action,
        confirmed_decision,
        mode_authorization_commitment_sha256,
        gameplay_authorization_commitment_sha256,
        observation_bracket_commitment_sha256,
        direct_competitive_scope_commitment_sha256,
    })
}

fn validate_bracket_v1(
    bracket: &MtgoDirectVisibleCompetitiveObservationBracketV1,
    lifecycle: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
) -> Result<(), MtgoContractErrorV1> {
    if bracket.schema_version != MTGO_DIRECT_VISIBLE_COMPETITIVE_OBSERVATION_BRACKET_SCHEMA_V1
        || bracket.information_boundary != INFORMATION_BOUNDARY_V1
        || bracket.capture_role != CAPTURE_ROLE_V1
        || !bracket.producer_first_export_is_player_visible_schema
        || bracket.raw_source_values_emitted
        || bracket.internal_identifiers_emitted
        || !bracket.bracket_complete
    {
        return Err(error_v1(
            "direct_visible_competitive_bracket_header",
            "the bracket must retain the exact acting-player visible-only boundary",
        ));
    }
    for digest in [
        bracket.before_frame_sha256.as_str(),
        bracket.after_frame_sha256.as_str(),
        bracket.before_decision_regions_sha256.as_str(),
        bracket.after_decision_regions_sha256.as_str(),
        bracket.client_identity_commitment_sha256.as_str(),
        bracket.broker_binary_sha256.as_str(),
        bracket.producer_binary_sha256.as_str(),
        bracket
            .visible_equivalence_profile_commitment_sha256
            .as_str(),
    ] {
        require_sha256_v1(digest)?;
    }
    if bracket.before_frame_id == 0
        || bracket.after_frame_id == 0
        || bracket.before_frame_id == bracket.after_frame_id
        || bracket.before_frame_sequence == 0
        || bracket.after_frame_sequence <= bracket.before_frame_sequence
        || bracket.before_frame_sha256 == bracket.after_frame_sha256
        || bracket.before_decision_regions_sha256 != bracket.after_decision_regions_sha256
    {
        return Err(error_v1(
            "direct_visible_competitive_bracket_order",
            "the direct observation must be surrounded by distinct ordered frames with unchanged decision regions",
        ));
    }
    let bounds = lifecycle.client_bounds_v1();
    if bounds.x != 0
        || bounds.y != 0
        || bounds.width != bracket.client_size_px.width
        || bounds.height != bracket.client_size_px.height
        || lifecycle.frame_id_v1() != bracket.after_frame_id
        || lifecycle.frame_sequence() != bracket.after_frame_sequence
        || lifecycle.frame_sha256_v1() != bracket.after_frame_sha256
    {
        return Err(error_v1(
            "direct_visible_competitive_bracket_lifecycle",
            "the newest bracket frame must be the exact competitive lifecycle source frame",
        ));
    }
    Ok(())
}

fn require_sha256_v1(value: &str) -> Result<(), MtgoContractErrorV1> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(error_v1(
            "direct_visible_competitive_digest",
            "direct competitive commitments must be lowercase SHA-256",
        ));
    }
    Ok(())
}

fn commitment_v1(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
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
    use crate::{
        refresh_direct_visible_selection_before_dispatch_v1,
        score_and_select_strict_visible_duel_producer_result_v1,
        validate_visible_competitive_lifecycle_snapshot_v1, MtgoLifecycleVisibleFactKindV1,
        MtgoLifecycleVisibleFactV1, MtgoPlayerRelativeRoleV1, MtgoPlayerVisibleCombatStateV1,
        MtgoPlayerVisibleDuelDecisionInputV1, MtgoPlayerVisibleDuelScoreResponseV1,
        MtgoPlayerVisibleDuelScorerV1, MtgoPlayerVisibleDuelStateV1, MtgoPlayerVisibleNamedCardV1,
        MtgoPlayerVisibleObjectRefV1, MtgoRectPxV1, MtgoVisibleCompetitiveLifecycleSnapshotV1,
        MtgoVisibleDuelViewModelBrokerResultV1, ZoneIndependentStepV1,
        MTGO_AUTHORIZATION_SCHEMA_V1, MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1,
        MTGO_COMPETITIVE_MATCH_GAMEPLAY_AUTHORIZATION_SCHEMA_V1,
        MTGO_PLAYER_VISIBLE_DUEL_SCORING_SCHEMA_V1,
    };

    struct SelectLandV1;

    impl MtgoPlayerVisibleDuelScorerV1 for SelectLandV1 {
        fn score_player_visible_duel_v1(
            &mut self,
            input: &MtgoPlayerVisibleDuelDecisionInputV1,
        ) -> Result<MtgoPlayerVisibleDuelScoreResponseV1, String> {
            assert_eq!(input.ordered_legal_actions.len(), 2);
            Ok(MtgoPlayerVisibleDuelScoreResponseV1 {
                schema_version: MTGO_PLAYER_VISIBLE_DUEL_SCORING_SCHEMA_V1,
                ordered_action_logits_f32_bits: vec![0.0_f32.to_bits(), 1.0_f32.to_bits()],
                value_f32_bits: 0.25_f32.to_bits(),
            })
        }
    }

    fn digest_v1(character: char) -> String {
        character.to_string().repeat(64)
    }

    fn visible_result_v1() -> Vec<u8> {
        let object_ref = MtgoPlayerVisibleObjectRefV1 { visible_ordinal: 0 };
        let decision = MtgoPlayerVisibleDuelDecisionInputV1 {
            current_state: MtgoPlayerVisibleDuelStateV1 {
                acting_player: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                turn: 1,
                phase: ZoneIndependentStepV1::Main1,
                active_player: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                priority_player: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                initiative: None,
                life_totals: [20, 20],
                mana_pools: [[0; 6]; 2],
                hand_counts: [1, 0],
                library_counts: [59, 60],
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
                own_hand: vec![MtgoPlayerVisibleNamedCardV1 {
                    object_ref,
                    card_name: "Plains".to_owned(),
                }],
                known_library_cards: [Vec::new(), Vec::new()],
                known_hand_cards: [Vec::new(), Vec::new()],
            },
            ordered_legal_actions: vec![
                MtgoPlayerVisibleDuelActionV1::Pass {
                    actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                },
                MtgoPlayerVisibleDuelActionV1::PlayLand {
                    actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                    source: object_ref,
                },
            ],
        };
        serde_json::to_vec(&MtgoVisibleDuelViewModelBrokerResultV1::VisibleDecision {
            decision: Box::new(decision),
        })
        .unwrap()
    }

    fn refreshed_selection_v1(
        bytes: &[u8],
    ) -> CheckedUntrustedMtgoRefreshedDirectVisibleSelectionV1 {
        let mut scorer = SelectLandV1;
        let crate::CheckedUntrustedMtgoDirectVisibleScoringOutcomeV1::Selected(selection) =
            score_and_select_strict_visible_duel_producer_result_v1(
                bytes,
                &digest_v1('d'),
                &mut scorer,
            )
            .unwrap()
        else {
            panic!("visible decision unexpectedly abstained");
        };
        refresh_direct_visible_selection_before_dispatch_v1(*selection, bytes).unwrap()
    }

    fn lifecycle_v1(
        event_kind: MtgoCompetitiveEventKindV1,
    ) -> CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1 {
        let bounds = MtgoRectPxV1 {
            x: 0,
            y: 0,
            width: 1_240,
            height: 740,
        };
        let kinds = [
            MtgoLifecycleVisibleFactKindV1::MatchSurfaceVisible,
            MtgoLifecycleVisibleFactKindV1::LocalClockVisible,
            MtgoLifecycleVisibleFactKindV1::OpponentClockVisible,
        ];
        validate_visible_competitive_lifecycle_snapshot_v1(
            MtgoVisibleCompetitiveLifecycleSnapshotV1 {
                schema_version: MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1,
                snapshot_id: "direct-visible-competitive-source-v1".to_owned(),
                event_kind,
                phase: MtgoCompetitiveLifecyclePhaseV1::MatchInProgress,
                frame_id: 102,
                frame_sequence: 202,
                frame_sha256: digest_v1('2'),
                client_bounds: bounds,
                event_identity_sha256: Some(digest_v1('6')),
                match_identity_sha256: Some(digest_v1('7')),
                game_number: Some(1),
                entry_terms: None,
                visible_state_complete: true,
                facts: kinds
                    .into_iter()
                    .enumerate()
                    .map(|(index, kind)| MtgoLifecycleVisibleFactV1 {
                        kind,
                        rect_client_px: MtgoRectPxV1 {
                            x: u32::try_from(index).unwrap() * 10,
                            y: 0,
                            width: 10,
                            height: 10,
                        },
                        content_sha256: digest_v1(char::from(b'a' + index as u8)),
                        confidence_bps: 10_000,
                    })
                    .collect(),
            },
        )
        .unwrap()
    }

    fn bracket_v1() -> MtgoDirectVisibleCompetitiveObservationBracketV1 {
        MtgoDirectVisibleCompetitiveObservationBracketV1 {
            schema_version: MTGO_DIRECT_VISIBLE_COMPETITIVE_OBSERVATION_BRACKET_SCHEMA_V1,
            information_boundary: INFORMATION_BOUNDARY_V1.to_owned(),
            capture_role: CAPTURE_ROLE_V1.to_owned(),
            before_frame_id: 101,
            before_frame_sequence: 201,
            before_frame_sha256: digest_v1('1'),
            after_frame_id: 102,
            after_frame_sequence: 202,
            after_frame_sha256: digest_v1('2'),
            client_size_px: MtgoSizePxV1 {
                width: 1_240,
                height: 740,
            },
            before_decision_regions_sha256: digest_v1('3'),
            after_decision_regions_sha256: digest_v1('3'),
            client_identity_commitment_sha256: digest_v1('4'),
            broker_binary_sha256: digest_v1('5'),
            producer_binary_sha256: digest_v1('a'),
            visible_equivalence_profile_commitment_sha256: digest_v1('b'),
            producer_first_export_is_player_visible_schema: true,
            raw_source_values_emitted: false,
            internal_identifiers_emitted: false,
            bracket_complete: true,
        }
    }

    fn mode_v1(event_kind: MtgoCompetitiveEventKindV1) -> MtgoAuthorizationScopeV1 {
        let mut mode = MtgoAuthorizationScopeV1 {
            schema_version: MTGO_AUTHORIZATION_SCHEMA_V1,
            account_alias_sha256: digest_v1('c'),
            written_permission_sha256: digest_v1('e'),
            visible_channels_only: true,
            ..MtgoAuthorizationScopeV1::default()
        };
        match event_kind {
            MtgoCompetitiveEventKindV1::League => mode.league_input = true,
            MtgoCompetitiveEventKindV1::Challenge => mode.challenge_input = true,
        }
        mode
    }

    fn gameplay_authorization_v1(
        event_kind: MtgoCompetitiveEventKindV1,
    ) -> MtgoCompetitiveMatchGameplayAuthorizationV1 {
        MtgoCompetitiveMatchGameplayAuthorizationV1 {
            schema_version: MTGO_COMPETITIVE_MATCH_GAMEPLAY_AUTHORIZATION_SCHEMA_V1,
            account_alias_sha256: digest_v1('c'),
            written_permission_sha256: digest_v1('e'),
            event_kind,
            event_identity_sha256: digest_v1('6'),
            match_identity_sha256: digest_v1('7'),
            game_number: 1,
            entry_authorization_sha256: digest_v1('8'),
            owner_launch_authorization_sha256: digest_v1('9'),
            exact_match_gameplay_authorized: true,
            valid_through_frame_sequence: 250,
        }
    }

    #[test]
    fn league_and_challenge_bind_the_same_visible_only_direct_path() {
        let bytes = visible_result_v1();
        for event_kind in [
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEventKindV1::Challenge,
        ] {
            let plan = bind_refreshed_direct_visible_selection_to_competitive_match_v1(
                refreshed_selection_v1(&bytes),
                bracket_v1(),
                lifecycle_v1(event_kind),
                &mode_v1(event_kind),
                &gameplay_authorization_v1(event_kind),
            )
            .unwrap();
            assert_eq!(plan.event_kind_v1(), event_kind);
            assert_eq!(plan.game_number_v1(), 1);
            assert_eq!(plan.selected_index_v1(), 1);
            assert_eq!(
                plan.selected_action_v1(),
                &plan.player_visible_confirmed_decision_v1().selected_action
            );
            assert_eq!(
                plan.direct_competitive_scope_commitment_sha256_v1().len(),
                64
            );
            assert!(!plan.safe_for_live_input_v1());
            assert!(!plan.permits_event_entry_v1());
            assert!(!plan.permits_spending_v1());
        }
    }

    #[test]
    fn hidden_output_unstable_visible_regions_and_crossed_match_reject() {
        let bytes = visible_result_v1();
        let event_kind = MtgoCompetitiveEventKindV1::League;

        let mut hidden = bracket_v1();
        hidden.internal_identifiers_emitted = true;
        assert_eq!(
            bind_refreshed_direct_visible_selection_to_competitive_match_v1(
                refreshed_selection_v1(&bytes),
                hidden,
                lifecycle_v1(event_kind),
                &mode_v1(event_kind),
                &gameplay_authorization_v1(event_kind),
            )
            .err()
            .expect("hidden output rejects")
            .code(),
            "direct_visible_competitive_bracket_header"
        );

        let mut changed_regions = bracket_v1();
        changed_regions.after_decision_regions_sha256 = digest_v1('f');
        assert_eq!(
            bind_refreshed_direct_visible_selection_to_competitive_match_v1(
                refreshed_selection_v1(&bytes),
                changed_regions,
                lifecycle_v1(event_kind),
                &mode_v1(event_kind),
                &gameplay_authorization_v1(event_kind),
            )
            .err()
            .expect("changed decision regions reject")
            .code(),
            "direct_visible_competitive_bracket_order"
        );

        let mut crossed = gameplay_authorization_v1(event_kind);
        crossed.match_identity_sha256 = digest_v1('f');
        assert_eq!(
            bind_refreshed_direct_visible_selection_to_competitive_match_v1(
                refreshed_selection_v1(&bytes),
                bracket_v1(),
                lifecycle_v1(event_kind),
                &mode_v1(event_kind),
                &crossed,
            )
            .err()
            .expect("crossed match rejects")
            .code(),
            "competitive_gameplay_authorization_binding"
        );
    }

    #[test]
    fn exact_mode_scope_and_newest_lifecycle_frame_are_required() {
        let bytes = visible_result_v1();
        let event_kind = MtgoCompetitiveEventKindV1::Challenge;
        let mut wrong_mode = mode_v1(MtgoCompetitiveEventKindV1::League);
        wrong_mode.league_input = false;
        assert_eq!(
            bind_refreshed_direct_visible_selection_to_competitive_match_v1(
                refreshed_selection_v1(&bytes),
                bracket_v1(),
                lifecycle_v1(event_kind),
                &wrong_mode,
                &gameplay_authorization_v1(event_kind),
            )
            .err()
            .expect("wrong mode rejects")
            .code(),
            "mode_not_authorized"
        );

        let mut stale = bracket_v1();
        stale.after_frame_id += 1;
        assert_eq!(
            bind_refreshed_direct_visible_selection_to_competitive_match_v1(
                refreshed_selection_v1(&bytes),
                stale,
                lifecycle_v1(event_kind),
                &mode_v1(event_kind),
                &gameplay_authorization_v1(event_kind),
            )
            .err()
            .expect("stale lifecycle frame rejects")
            .code(),
            "direct_visible_competitive_bracket_lifecycle"
        );
    }
}
