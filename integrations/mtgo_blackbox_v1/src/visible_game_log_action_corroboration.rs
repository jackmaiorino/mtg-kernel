use crate::{
    CheckedUntrustedMtgoVisibleGameLogSemanticProjectionV1, MtgoContractErrorV1,
    MtgoPlayerRelativeRoleV1, MtgoPlayerVisibleConfirmedDuelDecisionV1,
    MtgoPlayerVisibleDuelActionV1, MtgoPlayerVisibleDuelStateV1, MtgoPlayerVisibleObjectRefV1,
    MtgoVisibleGameLogEventKindV1, MtgoVisibleGameLogPlayerRoleV1,
    MtgoVisibleGameLogSemanticEventViewV1,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashSet};

const ACTION_BASELINE_DOMAIN_V1: &[u8] = b"mtgo-player-visible-game-log-action-baseline-v1";
const ACTION_CORROBORATION_DOMAIN_V1: &[u8] =
    b"mtgo-player-visible-game-log-action-corroboration-v1";

/// Read-only access to the typed facts that the MTGO Game Log renders for the
/// seated player. Implementations must not add transport or client-private
/// metadata to these views.
pub trait MtgoVisibleGameLogSemanticSequenceV1 {
    fn visible_source_record_count_v1(&self) -> usize;

    fn visible_source_record_prefix_commitment_v1(&self, record_count: usize) -> Option<String>;

    fn visible_event_count_v1(&self) -> usize;

    fn visible_event_v1(&self, index: usize) -> Option<MtgoVisibleGameLogSemanticEventViewV1<'_>>;
}

impl MtgoVisibleGameLogSemanticSequenceV1
    for CheckedUntrustedMtgoVisibleGameLogSemanticProjectionV1
{
    fn visible_source_record_count_v1(&self) -> usize {
        self.classified_source_record_count_v1()
            .checked_add(self.unclassified_source_record_count_v1())
            .expect("validated visible Game Log record counts must fit usize")
    }

    fn visible_source_record_prefix_commitment_v1(&self, record_count: usize) -> Option<String> {
        CheckedUntrustedMtgoVisibleGameLogSemanticProjectionV1::visible_source_record_prefix_commitment_v1(
            self,
            record_count,
        )
    }

    fn visible_event_count_v1(&self) -> usize {
        self.event_count_v1()
    }

    fn visible_event_v1(&self, index: usize) -> Option<MtgoVisibleGameLogSemanticEventViewV1<'_>> {
        self.event_v1(index)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoVisibleGameLogActionCorroborationKindV1 {
    PlayedCard,
    CastSpell,
    DiscardedCards,
    DeclaredAttackers,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct PublicEventV1 {
    source_sequence: u32,
    kind: MtgoVisibleGameLogEventKindV1,
    actor_role: Option<MtgoVisibleGameLogPlayerRoleV1>,
    turn_number: Option<u32>,
    primary_count: Option<u8>,
    secondary_count: Option<u8>,
    visible_card_names: Vec<String>,
}

enum PrivateExpectedVisibleEventV1 {
    OneCard {
        kind: MtgoVisibleGameLogEventKindV1,
        card_name: String,
    },
    CardMultiset {
        kind: MtgoVisibleGameLogEventKindV1,
        card_names: Vec<String>,
    },
}

/// Opaque public-fact baseline taken immediately before one selected action.
/// Card names and source ordering remain private to the adapter. The baseline
/// has no capture, model, event-entry, spending, or input authority.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoPlayerVisibleGameLogActionBaselineV1;
/// fn cannot_read_private_facts(value: &CheckedUntrustedMtgoPlayerVisibleGameLogActionBaselineV1) {
///     let _ = value.prior_events;
///     let _ = value.expected;
///     let _ = value.input_command();
/// }
/// ```
pub struct CheckedUntrustedMtgoPlayerVisibleGameLogActionBaselineV1 {
    prior_events: Vec<PublicEventV1>,
    prior_source_record_count: usize,
    prior_visible_source_prefix_commitment_sha256: String,
    expected: PrivateExpectedVisibleEventV1,
    kind: MtgoVisibleGameLogActionCorroborationKindV1,
    baseline_commitment_sha256: String,
}

impl CheckedUntrustedMtgoPlayerVisibleGameLogActionBaselineV1 {
    pub fn kind_v1(&self) -> MtgoVisibleGameLogActionCorroborationKindV1 {
        self.kind
    }

    pub fn baseline_commitment_sha256_v1(&self) -> &str {
        &self.baseline_commitment_sha256
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

/// Checked evidence that a strictly appended rendered Game Log fact matched
/// the exact player-visible action. It corroborates an action but never
/// authorizes that action, another input, event entry, or spending.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoPlayerVisibleGameLogActionCorroborationV1;
/// fn cannot_recover_transport_or_act(value: &CheckedUntrustedMtgoPlayerVisibleGameLogActionCorroborationV1) {
///     let _ = value.source_path();
///     let _ = value.raw_text();
///     let _ = value.input_command();
/// }
/// ```
pub struct CheckedUntrustedMtgoPlayerVisibleGameLogActionCorroborationV1 {
    _baseline: CheckedUntrustedMtgoPlayerVisibleGameLogActionBaselineV1,
    kind: MtgoVisibleGameLogActionCorroborationKindV1,
    appended_visible_event_count: usize,
    matched_visible_event_count: usize,
    corroboration_commitment_sha256: String,
}

impl CheckedUntrustedMtgoPlayerVisibleGameLogActionCorroborationV1 {
    pub fn kind_v1(&self) -> MtgoVisibleGameLogActionCorroborationKindV1 {
        self.kind
    }

    pub fn appended_visible_event_count_v1(&self) -> usize {
        self.appended_visible_event_count
    }

    pub fn matched_visible_event_count_v1(&self) -> usize {
        self.matched_visible_event_count
    }

    pub fn corroboration_commitment_sha256_v1(&self) -> &str {
        &self.corroboration_commitment_sha256
    }

    pub fn safe_for_additional_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

pub fn begin_checked_untrusted_player_visible_game_log_action_baseline_v1<
    S: MtgoVisibleGameLogSemanticSequenceV1,
>(
    before: &S,
    decision: &MtgoPlayerVisibleConfirmedDuelDecisionV1,
) -> Result<CheckedUntrustedMtgoPlayerVisibleGameLogActionBaselineV1, MtgoContractErrorV1> {
    if decision.current_state.acting_player != MtgoPlayerRelativeRoleV1::SeatedPlayer
        || action_actor_v1(&decision.selected_action) != MtgoPlayerRelativeRoleV1::SeatedPlayer
    {
        return Err(error_v1(
            "visible_game_log_action_actor",
            "Game Log corroboration is only defined for the seated player's action",
        ));
    }
    let (kind, expected) = expected_visible_event_v1(decision)?;
    let prior_events = public_events_v1(before)?;
    let prior_source_record_count = before.visible_source_record_count_v1();
    if prior_source_record_count < prior_events.len() {
        return Err(error_v1(
            "visible_game_log_action_source_count",
            "the public source-record count is smaller than its classified event count",
        ));
    }
    let prior_visible_source_prefix_commitment_sha256 = before
        .visible_source_record_prefix_commitment_v1(prior_source_record_count)
        .ok_or_else(|| {
            error_v1(
                "visible_game_log_action_source_prefix",
                "the pre-action source cannot commit its complete rendered prefix",
            )
        })?;
    let decision_bytes = serde_json::to_vec(decision)
        .map_err(|error| error_v1("visible_game_log_action_serialization", error.to_string()))?;
    let prior_bytes = serde_json::to_vec(&prior_events)
        .map_err(|error| error_v1("visible_game_log_action_serialization", error.to_string()))?;
    let baseline_commitment_sha256 = commitment_v1(
        ACTION_BASELINE_DOMAIN_V1,
        &[
            &decision_bytes,
            &prior_bytes,
            &(prior_source_record_count as u64).to_be_bytes(),
            prior_visible_source_prefix_commitment_sha256.as_bytes(),
            b"player_visible_game_log_facts_only_no_input",
        ],
    );
    Ok(CheckedUntrustedMtgoPlayerVisibleGameLogActionBaselineV1 {
        prior_events,
        prior_source_record_count,
        prior_visible_source_prefix_commitment_sha256,
        expected,
        kind,
        baseline_commitment_sha256,
    })
}

pub fn corroborate_checked_untrusted_player_visible_game_log_action_v1<
    S: MtgoVisibleGameLogSemanticSequenceV1,
>(
    baseline: CheckedUntrustedMtgoPlayerVisibleGameLogActionBaselineV1,
    after: &S,
) -> Result<CheckedUntrustedMtgoPlayerVisibleGameLogActionCorroborationV1, MtgoContractErrorV1> {
    let after_events = public_events_v1(after)?;
    if after_events.len() <= baseline.prior_events.len()
        || !after_events.starts_with(&baseline.prior_events)
    {
        return Err(error_v1(
            "visible_game_log_action_not_strict_append",
            "the post-action public Game Log must retain the exact prior visible prefix and append at least one event",
        ));
    }
    let appended = &after_events[baseline.prior_events.len()..];
    let after_source_record_count = after.visible_source_record_count_v1();
    let retained_prefix_commitment = after
        .visible_source_record_prefix_commitment_v1(baseline.prior_source_record_count)
        .ok_or_else(|| {
            error_v1(
                "visible_game_log_action_source_prefix",
                "the post-action source cannot retain the complete pre-action rendered prefix",
            )
        })?;
    if retained_prefix_commitment != baseline.prior_visible_source_prefix_commitment_sha256 {
        return Err(error_v1(
            "visible_game_log_action_source_prefix_changed",
            "the post-action source changed a previously rendered Game Log record",
        ));
    }
    let appended_source_record_count = after_source_record_count
        .checked_sub(baseline.prior_source_record_count)
        .ok_or_else(|| {
            error_v1(
                "visible_game_log_action_source_count",
                "the post-action visible source-record count moved backward",
            )
        })?;
    if appended_source_record_count != appended.len() {
        return Err(error_v1(
            "visible_game_log_action_unclassified_append",
            "every newly appended rendered Game Log record must be recognized before it can corroborate an action",
        ));
    }
    let previous_source_sequence = baseline
        .prior_events
        .last()
        .map(|event| event.source_sequence)
        .unwrap_or(0);
    if appended
        .first()
        .is_none_or(|event| event.source_sequence <= previous_source_sequence)
    {
        return Err(error_v1(
            "visible_game_log_action_source_order",
            "the corroborating visible event must have a newer source sequence",
        ));
    }
    let matched_visible_event_count = match &baseline.expected {
        PrivateExpectedVisibleEventV1::OneCard { kind, card_name } => {
            let candidates = action_events_v1(appended, *kind);
            if candidates.len() != 1
                || candidates[0].visible_card_names.as_slice() != [card_name.as_str()]
            {
                return Err(error_v1(
                    "visible_game_log_action_fact_mismatch",
                    "the appended seated-player Game Log fact does not exactly match the selected visible card action",
                ));
            }
            1
        }
        PrivateExpectedVisibleEventV1::CardMultiset { kind, card_names } => {
            let candidates = action_events_v1(appended, *kind);
            let mut observed = candidates
                .iter()
                .flat_map(|event| event.visible_card_names.iter().cloned())
                .collect::<Vec<_>>();
            observed.sort();
            if candidates.is_empty() || observed != *card_names {
                return Err(error_v1(
                    "visible_game_log_action_fact_mismatch",
                    "the appended seated-player Game Log card multiset does not exactly match the selected visible action",
                ));
            }
            candidates.len()
        }
    };
    let appended_bytes = serde_json::to_vec(appended)
        .map_err(|error| error_v1("visible_game_log_action_serialization", error.to_string()))?;
    let corroboration_commitment_sha256 = commitment_v1(
        ACTION_CORROBORATION_DOMAIN_V1,
        &[
            baseline.baseline_commitment_sha256.as_bytes(),
            &appended_bytes,
            &(matched_visible_event_count as u64).to_be_bytes(),
            b"checked_untrusted_visible_corroboration_no_additional_input",
        ],
    );
    let kind = baseline.kind;
    Ok(
        CheckedUntrustedMtgoPlayerVisibleGameLogActionCorroborationV1 {
            _baseline: baseline,
            kind,
            appended_visible_event_count: appended.len(),
            matched_visible_event_count,
            corroboration_commitment_sha256,
        },
    )
}

fn public_events_v1<S: MtgoVisibleGameLogSemanticSequenceV1>(
    source: &S,
) -> Result<Vec<PublicEventV1>, MtgoContractErrorV1> {
    let mut result = Vec::with_capacity(source.visible_event_count_v1());
    let mut previous_sequence = None;
    for index in 0..source.visible_event_count_v1() {
        let event = source.visible_event_v1(index).ok_or_else(|| {
            error_v1(
                "visible_game_log_action_event_missing",
                "a visible semantic event disappeared during corroboration",
            )
        })?;
        if event.source_sequence_v1() == 0
            || previous_sequence.is_some_and(|prior| event.source_sequence_v1() <= prior)
        {
            return Err(error_v1(
                "visible_game_log_action_source_order",
                "visible Game Log source sequences must be strictly increasing",
            ));
        }
        previous_sequence = Some(event.source_sequence_v1());
        result.push(PublicEventV1 {
            source_sequence: event.source_sequence_v1(),
            kind: event.kind_v1(),
            actor_role: event.actor_role_v1(),
            turn_number: event.turn_number_v1(),
            primary_count: event.primary_count_v1(),
            secondary_count: event.secondary_count_v1(),
            visible_card_names: (0..event.visible_card_name_count_v1())
                .filter_map(|name_index| event.visible_card_name_v1(name_index))
                .map(str::to_owned)
                .collect(),
        });
    }
    Ok(result)
}

fn action_events_v1(
    events: &[PublicEventV1],
    kind: MtgoVisibleGameLogEventKindV1,
) -> Vec<&PublicEventV1> {
    events
        .iter()
        .filter(|event| {
            event.kind == kind
                && event.actor_role == Some(MtgoVisibleGameLogPlayerRoleV1::ActingPlayer)
        })
        .collect()
}

fn expected_visible_event_v1(
    decision: &MtgoPlayerVisibleConfirmedDuelDecisionV1,
) -> Result<
    (
        MtgoVisibleGameLogActionCorroborationKindV1,
        PrivateExpectedVisibleEventV1,
    ),
    MtgoContractErrorV1,
> {
    use MtgoPlayerVisibleDuelActionV1 as Action;
    use MtgoVisibleGameLogActionCorroborationKindV1 as Corroboration;
    use MtgoVisibleGameLogEventKindV1 as Event;

    let one_card = |kind, event_kind, object| {
        let card_name = visible_card_name_v1(&decision.current_state, object)?;
        Ok((
            kind,
            PrivateExpectedVisibleEventV1::OneCard {
                kind: event_kind,
                card_name,
            },
        ))
    };
    match &decision.selected_action {
        Action::PlayLand { source, .. } => {
            one_card(Corroboration::PlayedCard, Event::PlayedCard, source)
        }
        Action::CastSpell { source, .. } => {
            one_card(Corroboration::CastSpell, Event::CastSpell, source)
        }
        Action::Discard { cards, .. } if !cards.is_empty() => Ok((
            Corroboration::DiscardedCards,
            PrivateExpectedVisibleEventV1::CardMultiset {
                kind: Event::DiscardedCard,
                card_names: visible_card_names_v1(&decision.current_state, cards)?,
            },
        )),
        Action::DeclareAttackers { attackers, .. } if !attackers.is_empty() => Ok((
            Corroboration::DeclaredAttackers,
            PrivateExpectedVisibleEventV1::CardMultiset {
                kind: Event::AttackedPlayer,
                card_names: visible_card_names_v1(&decision.current_state, attackers)?,
            },
        )),
        _ => Err(error_v1(
            "visible_game_log_action_unsupported",
            "the rendered Game Log does not state this selected action precisely enough for corroboration",
        )),
    }
}

fn visible_card_names_v1(
    state: &MtgoPlayerVisibleDuelStateV1,
    objects: &[MtgoPlayerVisibleObjectRefV1],
) -> Result<Vec<String>, MtgoContractErrorV1> {
    let mut seen = HashSet::new();
    let mut names = Vec::with_capacity(objects.len());
    for object in objects {
        if !seen.insert(object.visible_ordinal) {
            return Err(error_v1(
                "visible_game_log_action_object_duplicate",
                "a selected visible card object appears more than once",
            ));
        }
        names.push(visible_card_name_v1(state, object)?);
    }
    names.sort();
    Ok(names)
}

fn visible_card_name_v1(
    state: &MtgoPlayerVisibleDuelStateV1,
    object: &MtgoPlayerVisibleObjectRefV1,
) -> Result<String, MtgoContractErrorV1> {
    let mut names = BTreeSet::new();
    for card in &state.own_hand {
        if card.object_ref == *object {
            names.insert(card.card_name.as_str());
        }
    }
    for card in state.battlefield.iter().flatten() {
        if card.object_ref == *object {
            names.insert(card.card_name.as_str());
        }
    }
    for card in state.graveyards.iter().flatten() {
        if card.object_ref == *object {
            names.insert(card.card_name.as_str());
        }
    }
    for card in &state.exile {
        if card.object_ref == *object {
            if let Some(name) = card.visible_card_name.as_deref() {
                names.insert(name);
            }
        }
    }
    for card in state
        .known_library_cards
        .iter()
        .flatten()
        .map(|known| &known.card)
        .chain(state.known_hand_cards.iter().flatten())
    {
        if card.object_ref == *object {
            names.insert(card.card_name.as_str());
        }
    }
    for item in &state.stack {
        if item.source_object_ref == *object {
            if let Some(name) = item.visible_source_name.as_deref() {
                names.insert(name);
            }
        }
    }
    match names.len() {
        1 => Ok(names
            .into_iter()
            .next()
            .expect("one visible name")
            .to_owned()),
        0 => Err(error_v1(
            "visible_game_log_action_card_name_missing",
            "the selected object has no player-visible card name",
        )),
        _ => Err(error_v1(
            "visible_game_log_action_card_name_ambiguous",
            "the selected object maps to conflicting player-visible card names",
        )),
    }
}

fn action_actor_v1(action: &MtgoPlayerVisibleDuelActionV1) -> MtgoPlayerRelativeRoleV1 {
    use MtgoPlayerVisibleDuelActionV1::*;
    match action {
        Pass { actor }
        | PlayLand { actor, .. }
        | CastSpell { actor, .. }
        | ActivateManaAbility { actor, .. }
        | ActivateAbility { actor, .. }
        | PlotSpell { actor, .. }
        | ChooseTarget { actor, .. }
        | ChooseCostTarget { actor, .. }
        | ChooseCastMode { actor, .. }
        | ChooseKicker { actor, .. }
        | ChooseSpellMode { actor, .. }
        | ChooseEffectOption { actor, .. }
        | ChooseEffectTarget { actor, .. }
        | FinishEffectSelection { actor, .. }
        | ChooseEffectColor { actor, .. }
        | ChooseEffectNumber { actor, .. }
        | ChooseEffectBoolean { actor, .. }
        | FinishTargetSelection { actor, .. }
        | ChooseOptionalCostUse { actor, .. }
        | ChooseOptionalCostWhich { actor, .. }
        | ChooseSpellCopyPayment { actor, .. }
        | ChooseSpellCopyRetarget { actor, .. }
        | ChooseMadnessCast { actor, .. }
        | Discard { actor, .. }
        | DeclareAttackers { actor, .. }
        | DeclareBlockersForAttacker { actor, .. }
        | ChooseAttackerInclusion { actor, .. }
        | ChooseBlockerInclusion { actor, .. }
        | OrderTriggers { actor, .. } => *actor,
    }
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
        classify_checked_untrusted_mtgo_visible_game_log_semantics_v1,
        parse_checked_untrusted_mtgo_visible_game_log_v1, MtgoPlayerVisibleBattlefieldCardV1,
        MtgoPlayerVisibleCombatStateV1, MtgoPlayerVisibleCounterStateV1,
        MtgoPlayerVisibleNamedCardV1, ZoneIndependentStepV1,
    };

    const SOURCE_ID_V1: &str = "82ba951e-fe1f-423a-9cdb-bfe879ea2c6b";

    fn object_v1(visible_ordinal: u32) -> MtgoPlayerVisibleObjectRefV1 {
        MtgoPlayerVisibleObjectRefV1 { visible_ordinal }
    }

    fn state_v1() -> MtgoPlayerVisibleDuelStateV1 {
        MtgoPlayerVisibleDuelStateV1 {
            acting_player: MtgoPlayerRelativeRoleV1::SeatedPlayer,
            turn: 1,
            phase: ZoneIndependentStepV1::Main1,
            active_player: MtgoPlayerRelativeRoleV1::SeatedPlayer,
            priority_player: MtgoPlayerRelativeRoleV1::SeatedPlayer,
            initiative: None,
            life_totals: [20, 20],
            mana_pools: [[0; 6]; 2],
            hand_counts: [3, 7],
            library_counts: [50, 53],
            battlefield: [
                vec![MtgoPlayerVisibleBattlefieldCardV1 {
                    object_ref: object_v1(3),
                    card_name: "Goblin Guide".to_owned(),
                    tapped: false,
                    marked_damage: 0,
                    counters: MtgoPlayerVisibleCounterStateV1 {
                        plus_one_plus_one: 0,
                        minus_one_minus_one: 0,
                        minus_zero_minus_one: 0,
                        stun: 0,
                        lore: 0,
                    },
                    is_token: false,
                    visible_effective_power: Some(2),
                    visible_effective_toughness: Some(2),
                }],
                Vec::new(),
            ],
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
            own_hand: vec![
                MtgoPlayerVisibleNamedCardV1 {
                    object_ref: object_v1(1),
                    card_name: "Mountain".to_owned(),
                },
                MtgoPlayerVisibleNamedCardV1 {
                    object_ref: object_v1(2),
                    card_name: "Lightning Bolt".to_owned(),
                },
                MtgoPlayerVisibleNamedCardV1 {
                    object_ref: object_v1(4),
                    card_name: "Lava Spike".to_owned(),
                },
            ],
            known_library_cards: [Vec::new(), Vec::new()],
            known_hand_cards: [Vec::new(), Vec::new()],
        }
    }

    fn decision_v1(
        action: MtgoPlayerVisibleDuelActionV1,
    ) -> MtgoPlayerVisibleConfirmedDuelDecisionV1 {
        MtgoPlayerVisibleConfirmedDuelDecisionV1 {
            current_state: state_v1(),
            selected_action: action,
        }
    }

    fn write_dotnet_string_v1(target: &mut Vec<u8>, value: &str) {
        let length = value.len();
        assert!(length < 0x80);
        target.push(length as u8);
        target.extend_from_slice(value.as_bytes());
    }

    fn semantics_v1(lines: &[&str]) -> CheckedUntrustedMtgoVisibleGameLogSemanticProjectionV1 {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        write_dotnet_string_v1(&mut bytes, SOURCE_ID_V1);
        bytes.extend_from_slice(&4_u16.to_le_bytes());
        write_dotnet_string_v1(&mut bytes, SOURCE_ID_V1);
        for (index, line) in lines.iter().enumerate() {
            bytes.extend_from_slice(&(638_905_999_999_999_999 + index as u64).to_le_bytes());
            bytes.push(0);
            write_dotnet_string_v1(&mut bytes, line);
        }
        let source = parse_checked_untrusted_mtgo_visible_game_log_v1(&bytes).unwrap();
        classify_checked_untrusted_mtgo_visible_game_log_semantics_v1(&source, "UnbuckledPie")
            .unwrap()
    }

    #[test]
    fn exact_appended_played_card_corroborates_without_authority() {
        let before = semantics_v1(&["@PUnbuckledPie joined the game.", "Turn 1: @PUnbuckledPie"]);
        let after = semantics_v1(&[
            "@PUnbuckledPie joined the game.",
            "Turn 1: @PUnbuckledPie",
            "@PUnbuckledPie plays @[Mountain@:296710,467:@].",
        ]);
        let baseline = begin_checked_untrusted_player_visible_game_log_action_baseline_v1(
            &before,
            &decision_v1(MtgoPlayerVisibleDuelActionV1::PlayLand {
                actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                source: object_v1(1),
            }),
        )
        .unwrap();
        let corroboration =
            corroborate_checked_untrusted_player_visible_game_log_action_v1(baseline, &after)
                .unwrap();
        assert_eq!(
            corroboration.kind_v1(),
            MtgoVisibleGameLogActionCorroborationKindV1::PlayedCard
        );
        assert_eq!(corroboration.appended_visible_event_count_v1(), 1);
        assert_eq!(corroboration.matched_visible_event_count_v1(), 1);
        assert_eq!(corroboration.corroboration_commitment_sha256_v1().len(), 64);
        assert!(!corroboration.safe_for_additional_input_v1());
        assert!(!corroboration.permits_event_entry_v1());
        assert!(!corroboration.permits_spending_v1());
    }

    #[test]
    fn visible_card_name_and_actor_must_match_exactly() {
        let before = semantics_v1(&["@PUnbuckledPie joined the game."]);
        let wrong_card = semantics_v1(&[
            "@PUnbuckledPie joined the game.",
            "@PUnbuckledPie plays @[Forest@:296710,467:@].",
        ]);
        let baseline = begin_checked_untrusted_player_visible_game_log_action_baseline_v1(
            &before,
            &decision_v1(MtgoPlayerVisibleDuelActionV1::PlayLand {
                actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                source: object_v1(1),
            }),
        )
        .unwrap();
        assert_eq!(
            corroborate_checked_untrusted_player_visible_game_log_action_v1(baseline, &wrong_card,)
                .err()
                .unwrap()
                .code(),
            "visible_game_log_action_fact_mismatch"
        );

        let opponent = semantics_v1(&[
            "@PUnbuckledPie joined the game.",
            "@POpponent plays @[Mountain@:296710,467:@].",
        ]);
        let baseline = begin_checked_untrusted_player_visible_game_log_action_baseline_v1(
            &before,
            &decision_v1(MtgoPlayerVisibleDuelActionV1::PlayLand {
                actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                source: object_v1(1),
            }),
        )
        .unwrap();
        assert_eq!(
            corroborate_checked_untrusted_player_visible_game_log_action_v1(baseline, &opponent)
                .err()
                .unwrap()
                .code(),
            "visible_game_log_action_fact_mismatch"
        );
    }

    #[test]
    fn prior_visible_semantics_must_be_an_exact_prefix() {
        let before = semantics_v1(&["@PUnbuckledPie joined the game.", "Turn 1: @PUnbuckledPie"]);
        let rewritten = semantics_v1(&[
            "@PUnbuckledPie joined the game.",
            "Turn 2: @PUnbuckledPie",
            "@PUnbuckledPie plays @[Mountain@:296710,467:@].",
        ]);
        let baseline = begin_checked_untrusted_player_visible_game_log_action_baseline_v1(
            &before,
            &decision_v1(MtgoPlayerVisibleDuelActionV1::PlayLand {
                actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                source: object_v1(1),
            }),
        )
        .unwrap();
        assert_eq!(
            corroborate_checked_untrusted_player_visible_game_log_action_v1(baseline, &rewritten)
                .err()
                .unwrap()
                .code(),
            "visible_game_log_action_not_strict_append"
        );
    }

    #[test]
    fn unknown_rendered_records_appended_with_the_action_fail_closed() {
        let before = semantics_v1(&["@PUnbuckledPie joined the game."]);
        let after = semantics_v1(&[
            "@PUnbuckledPie joined the game.",
            "A future visible MTGO action detail.",
            "@PUnbuckledPie plays @[Mountain@:296710,467:@].",
        ]);
        let baseline = begin_checked_untrusted_player_visible_game_log_action_baseline_v1(
            &before,
            &decision_v1(MtgoPlayerVisibleDuelActionV1::PlayLand {
                actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                source: object_v1(1),
            }),
        )
        .unwrap();
        assert_eq!(
            corroborate_checked_untrusted_player_visible_game_log_action_v1(baseline, &after)
                .err()
                .unwrap()
                .code(),
            "visible_game_log_action_unclassified_append"
        );
    }

    #[test]
    fn previously_unrecognized_rendered_text_cannot_be_rewritten() {
        let before = semantics_v1(&[
            "@PUnbuckledPie joined the game.",
            "Previously visible wording A.",
        ]);
        let after = semantics_v1(&[
            "@PUnbuckledPie joined the game.",
            "Previously visible wording B.",
            "@PUnbuckledPie plays @[Mountain@:296710,467:@].",
        ]);
        let baseline = begin_checked_untrusted_player_visible_game_log_action_baseline_v1(
            &before,
            &decision_v1(MtgoPlayerVisibleDuelActionV1::PlayLand {
                actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                source: object_v1(1),
            }),
        )
        .unwrap();
        assert_eq!(
            corroborate_checked_untrusted_player_visible_game_log_action_v1(baseline, &after)
                .err()
                .unwrap()
                .code(),
            "visible_game_log_action_source_prefix_changed"
        );
    }

    #[test]
    fn unsupported_actions_and_invisible_names_fail_closed() {
        let before = semantics_v1(&["@PUnbuckledPie joined the game."]);
        let pass = decision_v1(MtgoPlayerVisibleDuelActionV1::Pass {
            actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
        });
        assert_eq!(
            begin_checked_untrusted_player_visible_game_log_action_baseline_v1(&before, &pass)
                .err()
                .unwrap()
                .code(),
            "visible_game_log_action_unsupported"
        );
        let missing = decision_v1(MtgoPlayerVisibleDuelActionV1::CastSpell {
            actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
            source: object_v1(99),
        });
        assert_eq!(
            begin_checked_untrusted_player_visible_game_log_action_baseline_v1(&before, &missing)
                .err()
                .unwrap()
                .code(),
            "visible_game_log_action_card_name_missing"
        );
    }

    #[test]
    fn declared_attackers_match_the_exact_visible_card_multiset() {
        let before = semantics_v1(&["@PUnbuckledPie joined the game."]);
        let after = semantics_v1(&[
            "@PUnbuckledPie joined the game.",
            "@POpponent is being attacked by @[Goblin Guide@:1,2:@]",
        ]);
        let baseline = begin_checked_untrusted_player_visible_game_log_action_baseline_v1(
            &before,
            &decision_v1(MtgoPlayerVisibleDuelActionV1::DeclareAttackers {
                actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                attackers: vec![object_v1(3)],
            }),
        )
        .unwrap();
        let corroboration =
            corroborate_checked_untrusted_player_visible_game_log_action_v1(baseline, &after)
                .unwrap();
        assert_eq!(
            corroboration.kind_v1(),
            MtgoVisibleGameLogActionCorroborationKindV1::DeclaredAttackers
        );
    }

    #[test]
    fn cast_and_multi_card_discard_use_only_exact_rendered_facts() {
        let before = semantics_v1(&["@PUnbuckledPie joined the game."]);
        let cases = [
            (
                MtgoPlayerVisibleDuelActionV1::CastSpell {
                    actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                    source: object_v1(2),
                },
                vec![
                    "@PUnbuckledPie joined the game.",
                    "@PUnbuckledPie casts @[Lightning Bolt@:1,2:@].",
                ],
                MtgoVisibleGameLogActionCorroborationKindV1::CastSpell,
                1,
            ),
            (
                MtgoPlayerVisibleDuelActionV1::Discard {
                    actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                    cards: vec![object_v1(2), object_v1(4)],
                },
                vec![
                    "@PUnbuckledPie joined the game.",
                    "@PUnbuckledPie discards @[Lava Spike@:5,6:@].",
                    "@PUnbuckledPie discards @[Lightning Bolt@:7,8:@].",
                ],
                MtgoVisibleGameLogActionCorroborationKindV1::DiscardedCards,
                2,
            ),
        ];
        for (action, lines, expected_kind, expected_matches) in cases {
            let after = semantics_v1(&lines);
            let baseline = begin_checked_untrusted_player_visible_game_log_action_baseline_v1(
                &before,
                &decision_v1(action),
            )
            .unwrap();
            let result =
                corroborate_checked_untrusted_player_visible_game_log_action_v1(baseline, &after)
                    .unwrap();
            assert_eq!(result.kind_v1(), expected_kind);
            assert_eq!(result.matched_visible_event_count_v1(), expected_matches);
        }
    }

    #[test]
    fn ability_activation_is_not_precise_enough_to_corroborate() {
        let before = semantics_v1(&["@PUnbuckledPie joined the game."]);
        let decision = decision_v1(MtgoPlayerVisibleDuelActionV1::ActivateAbility {
            actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
            source: object_v1(3),
            visible_choice_ordinal: 0,
        });
        assert_eq!(
            begin_checked_untrusted_player_visible_game_log_action_baseline_v1(&before, &decision,)
                .err()
                .unwrap()
                .code(),
            "visible_game_log_action_unsupported"
        );
    }
}
