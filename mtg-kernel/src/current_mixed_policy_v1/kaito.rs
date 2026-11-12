//! Distinct numeric Kaito V75 owner. No predecessor artifact or executable is encoded.
use super::{normalize, Error, NativeModel, Packet, Result, Source};
use crate::rl::{ActionSemanticV1, PendingEffectChoiceSemanticV4, PlayerSeatV1, TargetRefV1};
use crate::rl_session::{
    FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1, KaitoDecisionV75,
    KaitoSessionV75, KaitoSnapshotV75,
};
use serde_json::{Map, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod full_pool_sampler_tests;

pub struct KaitoSession {
    inner: KaitoSessionV75,
    owner: Arc<()>,
    generation: u64,
}
#[derive(Clone)]
pub struct KaitoSnapshot {
    inner: KaitoSnapshotV75,
    owner: Arc<()>,
}
/// Private executable authority stays in the sealed backend capability.
/// This type deliberately has no Debug, serde or public constructor.
#[derive(Clone)]
pub struct KaitoDecision {
    backend: KaitoDecisionV75,
    current: NumericCurrent,
    owner: Arc<()>,
    generation: u64,
}
impl KaitoDecision {
    pub fn packet(&self) -> &Packet {
        &self.current.packet
    }
    #[cfg(feature = "limited-fdn-fixtures")]
    pub(crate) fn public_turn(&self) -> u32 {
        self.backend
            .observation()
            .common
            .common
            .projection
            .surface
            .turn
    }
    #[cfg(all(
        test,
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    pub(crate) fn fixture_backend_v75(&self) -> &KaitoDecisionV75 {
        &self.backend
    }
}

/// Audit-only proof from one authenticated native reveal/discard prompt.
/// It is never serialized or supplied to the model. Private physical references
/// remain only in the source operand so different sources cannot share a proof.
#[cfg(test)]
#[derive(Clone)]
pub(crate) struct RevealedDiscardFamilyV2 {
    prompt: Arc<()>,
    meaning: RevealedDiscardMeaningV2,
}
#[cfg(test)]
#[derive(Clone, PartialEq, Eq)]
struct RevealedDiscardMeaningV2 {
    actor: PlayerSeatV1,
    source: crate::rl::CardStableRefV1,
    target_owner: PlayerSeatV1,
    printing: u16,
}
#[cfg(test)]
impl RevealedDiscardFamilyV2 {
    fn describes(&self, action: &ActionSemanticV1) -> bool {
        matches!(action, ActionSemanticV1::ChooseEffectTarget {
            actor, source, target: TargetRefV1::Object { object },
            selected_count: 0, min_targets: 1, max_targets: 1,
        } if *actor == self.meaning.actor && *source == self.meaning.source
            && object.owner == self.meaning.target_owner
            && object.zone == crate::state::Zone::Hand
            && object.card_db_id == self.meaning.printing)
    }
    pub(crate) fn equivalent_actions(
        &self,
        action: &ActionSemanticV1,
        other: &Self,
        other_action: &ActionSemanticV1,
    ) -> bool {
        Arc::ptr_eq(&self.prompt, &other.prompt)
            && self.meaning == other.meaning
            && self.describes(action)
            && other.describes(other_action)
    }
}

#[cfg(test)]
pub(crate) struct ProbeCaptureV2 {
    pub(crate) input: crate::current_tree_learner_v1::AuthenticatedInputV1,
    pub(crate) actions: Vec<ActionSemanticV1>,
    pub(crate) families: Vec<Option<RevealedDiscardFamilyV2>>,
}

enum HandFacts {
    CopyRows,
    NamesV2,
    RevealMinimaV3 {
        minima: [Vec<(u16, usize)>; 2],
        discard_owner: Option<PlayerSeatV1>,
    },
}

#[derive(Clone, PartialEq, Eq)]
struct NumericCurrent {
    packet: Packet,
    /// Present only for a revealed-hand menu in public name order.
    name_order: Option<Vec<u32>>,
}

fn opponent_hand_binding(value: &Value, opponent: &Value) -> bool {
    match value {
        Value::Object(fields) => {
            (fields.contains_key("arena_id")
                && fields.get("zone").is_some_and(|zone| zone == "Hand")
                && fields.get("owner") == Some(opponent))
                || fields
                    .values()
                    .any(|value| opponent_hand_binding(value, opponent))
        }
        Value::Array(rows) => rows
            .iter()
            .any(|value| opponent_hand_binding(value, opponent)),
        _ => false,
    }
}

/// Normalize only the exact current public reveal/discard targets. The backend
/// still owns every original physical answer, including equal-printing copies.
fn revealed_discard_by_name(
    candidates: &crate::kaito_candidates_v75::KaitoCandidatesV75,
    actions: &[ActionSemanticV1],
    owner: PlayerSeatV1,
    minima: &[(u16, usize)],
    state: &mut Value,
) -> Result<Vec<u32>> {
    let observation = &candidates.observation.common.common;
    let invalid =
        || Error::Projection("revealed discard menu differs from public hand names".into());
    let Some(PendingEffectChoiceSemanticV4::Targets {
        player,
        legal_targets,
        ..
    }) = observation
        .projection
        .surface
        .engine_context
        .pending_effect
        .as_ref()
        .and_then(|effect| effect.choice.as_ref())
    else {
        return Err(invalid());
    };
    if *player != observation.acting_player || legal_targets.len() != actions.len() {
        return Err(invalid());
    }
    let mut remaining: BTreeMap<_, _> = minima.iter().copied().collect();
    let mut names = Vec::with_capacity(actions.len());
    for (index, (action, expected)) in actions.iter().zip(legal_targets).enumerate() {
        let ActionSemanticV1::ChooseEffectTarget {
            actor,
            target: target @ TargetRefV1::Object { object },
            selected_count: 0,
            min_targets: 1,
            max_targets: 1,
            ..
        } = action
        else {
            return Err(invalid());
        };
        if actor != player
            || target != expected
            || object.owner != owner
            || object.zone != crate::state::Zone::Hand
        {
            return Err(invalid());
        }
        let copies = remaining.get_mut(&object.card_db_id).ok_or_else(invalid)?;
        *copies = copies.checked_sub(1).ok_or_else(invalid)?;
        names.push((object.card_db_id, index));
    }
    names.sort_by_key(|&(printing, _)| printing);
    let name_rows: Vec<_> = names
        .iter()
        .map(|&(printing, _)| {
            serde_json::json!({
                "target_kind": "revealed_hand_name",
                "card_db_id": printing,
                "card_name": crate::card_def::CARD_DEFS[usize::from(printing)].name,
                "owner": owner,
            })
        })
        .collect();
    *state.pointer_mut("/observation/common/common/projection/engine_context/pending_effect/choice/legal_targets")
        .ok_or_else(invalid)? = serde_json::json!(name_rows);
    let rows = state["candidates"].as_array_mut().ok_or_else(invalid)?;
    let mut native_rows = std::mem::take(rows);
    let mut order = Vec::with_capacity(names.len());
    for (ordinal, ((_, index), target)) in names.into_iter().zip(name_rows).enumerate() {
        let mut row = std::mem::take(&mut native_rows[index]);
        row["ordinal"] = Value::from(u32::try_from(ordinal).map_err(|_| Error::Range)?);
        row["action"]["target"] = target;
        rows.push(row);
        order.push(u32::try_from(index).map_err(|_| Error::Range)?);
    }
    Ok(order)
}

// Pure normalization of the exact freshly validated backend input. This
// function cannot capture another decision or acquire executable authority.
fn packet_from_current(
    candidates: &crate::kaito_candidates_v75::KaitoCandidatesV75,
    actions: &[crate::rl::ActionSemanticV1],
    hand_facts: &HandFacts,
) -> Result<NumericCurrent> {
    crate::kaito_candidates_v75::validate_artifact(candidates)
        .map_err(|e| Error::Projection(e.to_string()))?;
    if candidates.candidates.len() != actions.len()
        || candidates
            .candidates
            .iter()
            .zip(actions)
            .any(|(row, action)| &row.action != action)
    {
        return Err(Error::StaleBinding);
    }
    // The complete observation and every typed per-action program/counter/LKI
    // annotation remain present. Raw executable pairs and index maps are never encoded.
    let mut state =
        serde_json::to_value(candidates).map_err(|e| Error::Projection(e.to_string()))?;
    if let Some(counts) = super::roots::elf_lord_printing::visible_counts(
        &candidates.observation.common.common.projection.surface,
    ) {
        state["elf_lord_visible_counts"] = counts;
    }
    let minima = match hand_facts {
        HandFacts::CopyRows => None,
        HandFacts::NamesV2 => Some(
            candidates
                .observation
                .common
                .common
                .known_hand_cards
                .each_ref()
                .map(|cards| {
                    let mut counts = BTreeMap::<u16, usize>::new();
                    for card in cards {
                        *counts.entry(card.stable.card_db_id).or_default() += 1;
                    }
                    counts.into_iter().collect::<Vec<_>>()
                }),
        ),
        HandFacts::RevealMinimaV3 { minima, .. } => Some(minima.clone()),
    };
    if let Some(minima) = minima {
        let counts = minima.map(|cards| {
            cards
                .into_iter()
                .map(|(id, copies)| {
                    serde_json::json!({
                        "card_db_id": id,
                        "card_name": crate::card_def::CARD_DEFS[usize::from(id)].name,
                        "minimum_copies": copies,
                    })
                })
                .collect::<Vec<_>>()
        });
        *state
            .pointer_mut("/observation/common/common/known_hand_cards")
            .ok_or_else(|| Error::Projection("missing known hand facts".into()))? =
            serde_json::json!(counts);
    }
    let (packet, name_order) =
        normalize::encode_with_action_rows(Source::KaitoV75, state, |state| {
            let mut name_order = None;
            if let HandFacts::RevealMinimaV3 {
                minima,
                discard_owner,
            } = hand_facts
            {
                if let Some(owner) = *discard_owner {
                    let seat = usize::from(owner == PlayerSeatV1::P1);
                    name_order = Some(revealed_discard_by_name(
                        candidates,
                        actions,
                        owner,
                        &minima[seat],
                        state,
                    )?);
                }
                let opponent = match candidates.observation.common.common.acting_player {
                    crate::rl::PlayerSeatV1::P0 => "p1",
                    crate::rl::PlayerSeatV1::P1 => "p0",
                };
                if opponent_hand_binding(state, &Value::from(opponent)) {
                    return Err(Error::Projection(
                        "opponent hand binding requires a public name action".into(),
                    ));
                }
            }
            let actions = state["candidates"]
                .as_array()
                .ok_or_else(|| Error::Projection("missing numeric candidate rows".into()))?
                .clone();
            Ok((actions, name_order))
        })?;
    Ok(NumericCurrent { packet, name_order })
}
impl KaitoSession {
    #[cfg(test)]
    pub(crate) fn probe_apply_v1(
        &mut self,
        decision: &KaitoDecision,
        index: u32,
    ) -> std::result::Result<(), (crate::current_actor_probe_v1::Stage, String)> {
        use crate::current_actor_probe_v1::Stage;
        if !Arc::ptr_eq(&self.owner, &decision.owner)
            || self.generation != decision.generation
            || index as usize >= decision.packet().actions.len()
        {
            return Err((Stage::Encoding, "stale probe owner or action range".into()));
        }
        let backend_index = decision
            .current
            .name_order
            .as_ref()
            .map_or(index, |order| order[index as usize]);
        let facts = self
            .hand_facts()
            .map_err(|error| (Stage::Encoding, error.to_string()))?;
        self.inner
            .step_with_current_check(
                &decision.backend,
                backend_index,
                |candidates, actions| {
                    let fresh = packet_from_current(candidates, actions, &facts)
                        .map_err(|e| (Stage::Encoding, e.to_string()))?;
                    if fresh != decision.current {
                        return Err((Stage::Encoding, "changed probe packet".into()));
                    }
                    Ok(())
                },
                |error| (Stage::Encoding, error.to_string()),
                |error| (Stage::Engine, error.to_string()),
            )
            .map(|_| ())
    }

    #[cfg(test)]
    pub(crate) fn probe_capture_v2(&self, decision: &KaitoDecision) -> Result<ProbeCaptureV2> {
        // Validates current owner, generation, native menu, typed operands and
        // public hand facts before any audit equivalence can be issued.
        let input = self.training_input_v1(decision)?;
        let raw = decision.backend.actions();
        let actions: Vec<_> = match &decision.current.name_order {
            Some(order) => order
                .iter()
                .map(|&index| raw[index as usize].clone())
                .collect(),
            None => raw.to_vec(),
        };
        let mut families = vec![None; actions.len()];
        if decision.current.name_order.is_some() {
            // This fact is supplied only by the FullPool native
            // DuressDiscard continuation validator, not by numeric equality.
            let HandFacts::RevealMinimaV3 {
                discard_owner: Some(owner),
                ..
            } = self.hand_facts()?
            else {
                return Err(Error::StaleBinding);
            };
            let prompt = Arc::new(());
            for (index, action) in actions.iter().enumerate() {
                let ActionSemanticV1::ChooseEffectTarget {
                    actor,
                    source,
                    target: TargetRefV1::Object { object },
                    selected_count: 0,
                    min_targets: 1,
                    max_targets: 1,
                } = action
                else {
                    return Err(Error::StaleBinding);
                };
                if object.owner != owner || object.zone != crate::state::Zone::Hand {
                    return Err(Error::StaleBinding);
                }
                families[index] = Some(RevealedDiscardFamilyV2 {
                    prompt: prompt.clone(),
                    meaning: RevealedDiscardMeaningV2 {
                        actor: *actor,
                        source: source.clone(),
                        target_owner: owner,
                        printing: object.card_db_id,
                    },
                });
            }
        }
        Ok(ProbeCaptureV2 {
            input,
            actions,
            families,
        })
    }

    fn hand_facts(&self) -> Result<HandFacts> {
        if let Some(mut minima) = self.inner.reveal_hand_minima_v3() {
            let reveal = self
                .inner
                .revealed_discard_hand_v1()
                .map_err(Error::Projection)?;
            let discard_owner = if let Some((owner, names)) = reveal {
                let mut counts = BTreeMap::<u16, usize>::new();
                for name in names {
                    *counts.entry(name).or_default() += 1;
                }
                minima[owner.index()] = counts.into_iter().collect();
                Some(owner.into())
            } else {
                None
            };
            Ok(HandFacts::RevealMinimaV3 {
                minima,
                discard_owner,
            })
        } else if self.inner.uses_hand_knowledge_by_name_v2() {
            Ok(HandFacts::NamesV2)
        } else {
            Ok(HandFacts::CopyRows)
        }
    }
    pub fn from_session(session: FastActorSessionV1) -> Result<Self> {
        Ok(Self::from_kaito(
            KaitoSessionV75::from_session(session).map_err(|e| Error::Projection(e.to_string()))?,
        ))
    }
    /// Transfer a genuine sealed current owner, never an observation/world DTO.
    pub fn from_kaito(inner: KaitoSessionV75) -> Self {
        Self {
            inner,
            owner: Arc::new(()),
            generation: 0,
        }
    }
    /// Ordinary production reset keeps the native Full-only declared-deck preflight.
    pub fn reset_explicit_mainboards(
        episode_id: u64,
        env_seed: u64,
        limits: (u64, u64),
        deck_ids: crate::rl_session::SessionDeckIdsV1,
        mainboards: [Vec<u16>; 2],
        starting_player: crate::ids::PlayerId,
    ) -> Result<Self> {
        KaitoSessionV75::reset_explicit_mainboards_v75(
            episode_id,
            env_seed,
            limits,
            deck_ids,
            mainboards,
            starting_player,
        )
        .map(Self::from_kaito)
        .map_err(|e| Error::Projection(format!("{e:?}")))
    }

    /// Full-only reset for a fresh training game with name-level reveal facts.
    pub fn reset_explicit_mainboards_hand_facts_v2(
        episode_id: u64,
        env_seed: u64,
        limits: (u64, u64),
        deck_ids: crate::rl_session::SessionDeckIdsV1,
        mainboards: [Vec<u16>; 2],
        starting_player: crate::ids::PlayerId,
    ) -> Result<Self> {
        let mut session = Self::reset_explicit_mainboards(
            episode_id,
            env_seed,
            limits,
            deck_ids,
            mainboards,
            starting_player,
        )?;
        session
            .inner
            .enable_hand_knowledge_by_name_v2()
            .map_err(Error::Projection)?;
        Ok(session)
    }

    pub fn sampled_clone(&self, decision: &KaitoDecision, seed: u64) -> Result<Self> {
        if !Arc::ptr_eq(&self.owner, &decision.owner) || self.generation != decision.generation {
            return Err(Error::StaleBinding);
        }
        self.inner
            .validate_decision_v75(&decision.backend)
            .map_err(|_| Error::StaleBinding)?;
        let facts = self.hand_facts()?;
        if matches!(facts, HandFacts::RevealMinimaV3 { .. }) {
            return Err(Error::Projection(
                "use the separate closed-list V3 sampler".into(),
            ));
        }
        if packet_from_current(
            decision.backend.candidates_v75(),
            decision.backend.actions(),
            &facts,
        )? != decision.current
        {
            return Err(Error::StaleBinding);
        }
        let inner = self
            .inner
            .sampled_clone_v75(&decision.backend, seed)
            .map_err(|e| Error::Projection(e.to_string()))?;
        let sampled = Self::from_kaito(inner);
        if sampled.encode(decision.backend.expected())?.packet() != decision.packet() {
            return Err(Error::StaleBinding);
        }
        Ok(sampled)
    }
    /// A separate corpus-driven path for the full-pool reveal-minima owner.
    /// The live opponent deck and physical revealed-copy identities are not
    /// supplied as a belief. The caller supplies the public belief corpus.
    #[cfg(feature = "limited-fdn-fixtures")]
    pub fn sampled_full_pool_closed_list_v1(
        &self,
        decision: &KaitoDecision,
        seed: u64,
        belief: &crate::limited_match_v1::LimitedBeliefV1,
    ) -> Result<Self> {
        if !Arc::ptr_eq(&self.owner, &decision.owner) || self.generation != decision.generation {
            return Err(Error::StaleBinding);
        }
        let facts = self.hand_facts()?;
        if !matches!(facts, HandFacts::RevealMinimaV3 { .. }) {
            return Err(Error::Projection(
                "full-pool sampling requires public reveal minima".into(),
            ));
        }
        if packet_from_current(
            decision.backend.candidates_v75(),
            decision.backend.actions(),
            &facts,
        )? != decision.current
        {
            return Err(Error::StaleBinding);
        }
        let inner = self
            .inner
            .sampled_full_pool_closed_list_v1(&decision.backend, seed, belief)
            .map_err(|e| Error::Projection(e.to_string()))?;
        let sampled = Self::from_kaito(inner);
        if sampled.encode(decision.backend.expected())?.packet() != decision.packet() {
            return Err(Error::StaleBinding);
        }
        Ok(sampled)
    }

    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.response()
    }
    #[cfg(all(
        test,
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    pub(crate) fn fixture_state(&self) -> &crate::state::GameState {
        self.inner.fixture_state_v75()
    }
    #[cfg(all(
        test,
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    pub(crate) fn fixture_projection_error(&self) -> Option<String> {
        self.inner
            .decision_v75()
            .err()
            .map(|error| error.to_string())
    }
    #[cfg(all(
        test,
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    pub(crate) fn fixture_counts(&self) -> (u64, u64) {
        self.inner.fixture_counts_v75()
    }
    pub fn snapshot(&self) -> KaitoSnapshot {
        KaitoSnapshot {
            inner: self.inner.snapshot_v75(),
            owner: self.owner.clone(),
        }
    }
    pub fn restore(&mut self, snapshot: &KaitoSnapshot) -> Result<()> {
        if !Arc::ptr_eq(&self.owner, &snapshot.owner) {
            return Err(Error::ForeignSnapshot);
        }
        let next = self.generation.checked_add(1).ok_or(Error::Range)?;
        // Backend authenticates the original binding before monotonic rebind.
        self.inner
            .restore_v75(&snapshot.inner)
            .map_err(|_| Error::StaleBinding)?;
        self.generation = next;
        Ok(())
    }
    pub fn encode(&self, expected: FastActorDecisionV1) -> Result<KaitoDecision> {
        let backend = self.inner.decision_v75().map_err(|_| Error::StaleBinding)?;
        if backend.expected() != expected {
            return Err(Error::StaleBinding);
        }
        let current = packet_from_current(
            backend.candidates_v75(),
            backend.actions(),
            &self.hand_facts()?,
        )?;
        Ok(KaitoDecision {
            backend,
            current,
            owner: self.owner.clone(),
            generation: self.generation,
        })
    }
    pub fn apply(
        &mut self,
        decision: &KaitoDecision,
        selected_index: u32,
    ) -> Result<FastActorResponseV1> {
        if !Arc::ptr_eq(&self.owner, &decision.owner) || self.generation != decision.generation {
            return Err(Error::StaleBinding);
        }
        let index = usize::try_from(selected_index).map_err(|_| Error::ActionRange)?;
        if index >= decision.packet().actions.len() {
            return Err(Error::ActionRange);
        }
        let backend_index = decision
            .current
            .name_order
            .as_ref()
            .map_or(selected_index, |order| order[index]);
        let name_facts = self.hand_facts()?;
        self.inner.step_with_current_check(
            &decision.backend,
            backend_index,
            |candidates, actions| {
                if packet_from_current(candidates, actions, &name_facts)? != decision.current {
                    return Err(Error::StaleBinding);
                }
                Ok(())
            },
            |_| Error::StaleBinding,
            |error| Error::Projection(error.to_string()),
        )
    }

    /// Capture a learner input only from this owner's genuine current decision.
    /// The immutable capture may outlive the decision for synchronous learning;
    /// it carries no authority to execute an action after the owner advances.
    pub fn training_input_v1(
        &self,
        decision: &KaitoDecision,
    ) -> Result<crate::current_tree_learner_v1::AuthenticatedInputV1> {
        if !Arc::ptr_eq(&self.owner, &decision.owner) || self.generation != decision.generation {
            return Err(Error::StaleBinding);
        }
        self.inner
            .validate_decision_v75(&decision.backend)
            .map_err(|_| Error::StaleBinding)?;
        if packet_from_current(
            decision.backend.candidates_v75(),
            decision.backend.actions(),
            &self.hand_facts()?,
        )? != decision.current
        {
            return Err(Error::StaleBinding);
        }
        let actions = decision.backend.actions();
        let ordered;
        let actions = if let Some(order) = &decision.current.name_order {
            ordered = order
                .iter()
                .map(|&index| actions[index as usize].clone())
                .collect::<Vec<_>>();
            &ordered
        } else {
            actions
        };
        crate::current_tree_learner_v1::AuthenticatedInputV1::from_current(
            decision.packet().clone(),
            actions,
        )
    }

    pub fn score_and_apply_tree_v1(
        &mut self,
        decision: &KaitoDecision,
        model: &crate::current_tree_learner_v1::ModelV1,
    ) -> Result<(Vec<f64>, u32, FastActorResponseV1)> {
        let logits = model.forward(&self.training_input_v1(decision)?)?;
        let selected =
            logits.iter().enumerate().fold(
                0,
                |best, (i, value)| {
                    if *value > logits[best] {
                        i
                    } else {
                        best
                    }
                },
            );
        let selected = u32::try_from(selected).map_err(|_| Error::Range)?;
        let response = self.apply(decision, selected)?;
        Ok((logits, selected, response))
    }
    pub fn score_and_apply(
        &mut self,
        decision: &KaitoDecision,
        model: &NativeModel,
    ) -> Result<(Vec<f32>, u32, FastActorResponseV1)> {
        let logits = model.forward(decision.packet())?;
        if logits.len() != decision.packet().actions.len() || logits.is_empty() {
            return Err(Error::ModelContract);
        }
        let mut selected = 0;
        for index in 1..logits.len() {
            if logits[index] > logits[selected] {
                selected = index;
            }
        }
        let selected = u32::try_from(selected).map_err(|_| Error::Range)?;
        let response = self.apply(decision, selected)?;
        Ok((logits, selected, response))
    }
}

pub(super) fn typed_meaning(
    value: &Map<String, Value>,
    covered: &mut BTreeSet<u16>,
) -> Result<bool> {
    if value.get("schema").and_then(Value::as_str) != Some(crate::rules_meaning_kaito_v75::SCHEMA) {
        return Ok(false);
    }
    let meaning: crate::rules_meaning_kaito_v75::MeaningV75 =
        serde_json::from_value(Value::Object(value.clone()))
            .map_err(|e| Error::Projection(e.to_string()))?;
    crate::rules_meaning_kaito_v75::validate(&meaning).map_err(Error::Projection)?;
    covered.insert(meaning.card_id);
    Ok(true)
}
pub(super) fn printing(id: u16) -> Result<Option<Value>> {
    if id == 495 {
        if !crate::effect::kaito_cunning_v75::token_definition(id) {
            return Err(Error::Projection(
                "exact Kaito Ninja definition changed".into(),
            ));
        }
        let m = crate::rules_meaning_kaito_v75::printed_meaning().map_err(Error::Projection)?;
        return Ok(Some(
            serde_json::json!({"schema":"rules-meaning/kaito-ninja-token/v75","card_id":495,"is_token":true,"not_cast_permission":true,"mana_value":0,"supertypes":[],"triggers":[],"printed":m.ninja}),
        ));
    }
    if id != 494 {
        return Ok(None);
    }
    serde_json::to_value(
        crate::rules_meaning_kaito_v75::printed_meaning().map_err(Error::Projection)?,
    )
    .map(Some)
    .map_err(|e| Error::Projection(e.to_string()))
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
pub(crate) mod tests;
