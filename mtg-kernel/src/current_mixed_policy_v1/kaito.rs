//! Distinct numeric Kaito V75 owner. No predecessor artifact or executable is encoded.
use super::{normalize, Error, NativeModel, Packet, Result, Source};
use crate::rl_session::{
    FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1, KaitoDecisionV75,
    KaitoSessionV75, KaitoSnapshotV75,
};
use serde_json::{Map, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

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
    packet: Packet,
    owner: Arc<()>,
    generation: u64,
}
impl KaitoDecision {
    pub fn packet(&self) -> &Packet {
        &self.packet
    }
    #[cfg(all(test, feature = "limited-fdn-fixtures", not(feature = "standard-magezero-fixtures")))]
    pub(crate) fn fixture_backend_v75(&self) -> &KaitoDecisionV75 {
        &self.backend
    }
}

enum HandFacts {
    CopyRows,
    NamesV2,
    RevealMinimaV3([Vec<(u16, usize)>; 2]),
}

fn opponent_hand_binding(value: &Value, opponent: &Value) -> bool {
    match value {
        Value::Object(fields) => {
            (fields.contains_key("arena_id")
                && fields.get("zone").is_some_and(|zone| zone == "Hand")
                && fields.get("owner") == Some(opponent))
                || fields.values().any(|value| opponent_hand_binding(value, opponent))
        }
        Value::Array(rows) => rows.iter().any(|value| opponent_hand_binding(value, opponent)),
        _ => false,
    }
}

// Pure normalization of the exact freshly validated backend input. This
// function cannot capture another decision or acquire executable authority.
fn packet_from_current(
    candidates: &crate::kaito_candidates_v75::KaitoCandidatesV75,
    actions: &[crate::rl::ActionSemanticV1],
    hand_facts: &HandFacts,
) -> Result<Packet> {
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
        HandFacts::RevealMinimaV3(minima) => Some(minima.clone()),
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
    if matches!(hand_facts, HandFacts::RevealMinimaV3(_)) {
        let opponent = match candidates.observation.common.common.acting_player {
            crate::rl::PlayerSeatV1::P0 => "p1",
            crate::rl::PlayerSeatV1::P1 => "p0",
        };
        if opponent_hand_binding(&state, &Value::from(opponent)) {
            return Err(Error::Projection(
                "opponent hand binding requires a public name action".into(),
            ));
        }
    }
    let actions = candidates
        .candidates
        .iter()
        .map(serde_json::to_value)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| Error::Projection(e.to_string()))?;
    normalize::encode(Source::KaitoV75, state, actions)
}
impl KaitoSession {
    fn hand_facts(&self) -> HandFacts {
        if let Some(minima) = self.inner.reveal_hand_minima_v3() {
            HandFacts::RevealMinimaV3(minima)
        } else if self.inner.uses_hand_knowledge_by_name_v2() {
            HandFacts::NamesV2
        } else {
            HandFacts::CopyRows
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
        let facts = self.hand_facts();
        if matches!(facts, HandFacts::RevealMinimaV3(_)) {
            return Err(Error::Projection(
                "use the separate closed-list V3 sampler".into(),
            ));
        }
        if packet_from_current(
            decision.backend.candidates_v75(),
            decision.backend.actions(),
            &facts,
        )? != decision.packet
        {
            return Err(Error::StaleBinding);
        }
        let inner = self
            .inner
            .sampled_clone_v75(&decision.backend, seed)
            .map_err(|e| Error::Projection(e.to_string()))?;
        let sampled = Self::from_kaito(inner);
        if sampled.encode(decision.backend.expected())?.packet != decision.packet {
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
        let packet = packet_from_current(
            backend.candidates_v75(),
            backend.actions(),
            &self.hand_facts(),
        )?;
        Ok(KaitoDecision {
            backend,
            packet,
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
        if usize::try_from(selected_index)
            .ok()
            .is_none_or(|i| i >= decision.packet.actions.len())
        {
            return Err(Error::ActionRange);
        }
        let name_facts = self.hand_facts();
        self.inner.step_with_current_check(
            &decision.backend,
            selected_index,
            |candidates, actions| {
                if packet_from_current(candidates, actions, &name_facts)? != decision.packet {
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
            &self.hand_facts(),
        )? != decision.packet
        {
            return Err(Error::StaleBinding);
        }
        crate::current_tree_learner_v1::AuthenticatedInputV1::from_current(
            decision.packet.clone(),
            decision.backend.actions(),
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
        let logits = model.forward(&decision.packet)?;
        if logits.len() != decision.packet.actions.len() || logits.is_empty() {
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
