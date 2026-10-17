//! Distinct numeric Tinybones V72 owner. No predecessor artifact or executable is encoded.
use super::{normalize, Error, NativeModel, Packet, Result, Source};
use crate::rl_session::{
    FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1, TinybonesDecisionV72,
    TinybonesSessionV72, TinybonesSnapshotV72,
};
use serde_json::{Map, Value};
use std::{collections::BTreeSet, sync::Arc};

pub struct TinybonesSession {
    inner: TinybonesSessionV72,
    owner: Arc<()>,
    generation: u64,
}
#[derive(Clone)]
pub struct TinybonesSnapshot {
    inner: TinybonesSnapshotV72,
    owner: Arc<()>,
}
/// Private executable authority stays in the sealed backend capability.
/// This type deliberately has no Debug, serde or public constructor.
#[derive(Clone)]
pub struct TinybonesDecision {
    backend: TinybonesDecisionV72,
    packet: Packet,
    owner: Arc<()>,
    generation: u64,
}
impl TinybonesDecision {
    pub fn packet(&self) -> &Packet {
        &self.packet
    }
}
// Pure normalization of the exact freshly validated backend input. This
// function cannot capture another decision or acquire executable authority.
fn packet_from_current(
    candidates: &crate::tinybones_candidates_v72::TinybonesCandidatesV72,
    actions: &[crate::rl::ActionSemanticV1],
) -> Result<Packet> {
    crate::tinybones_candidates_v72::validate_artifact(candidates)
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
    let state = serde_json::to_value(candidates).map_err(|e| Error::Projection(e.to_string()))?;
    let actions = candidates
        .candidates
        .iter()
        .map(serde_json::to_value)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| Error::Projection(e.to_string()))?;
    normalize::encode(Source::TinybonesV72, state, actions)
}
impl TinybonesSession {
    pub fn from_session(session: FastActorSessionV1) -> Result<Self> {
        Ok(Self::from_tinybones(
            TinybonesSessionV72::from_session(session)
                .map_err(|e| Error::Projection(e.to_string()))?,
        ))
    }
    /// Transfer a genuine sealed current owner, never an observation/world DTO.
    pub fn from_tinybones(inner: TinybonesSessionV72) -> Self {
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
        TinybonesSessionV72::reset_explicit_mainboards_v72(
            episode_id,
            env_seed,
            limits,
            deck_ids,
            mainboards,
            starting_player,
        )
        .map(Self::from_tinybones)
        .map_err(|e| Error::Projection(format!("{e:?}")))
    }

    /// A derived numeric owner always has a fresh lineage and freshly encodes
    /// the genuine sampled backend. Original decisions never authorize it.
    pub fn sampled_clone(&self, decision: &TinybonesDecision, seed: u64) -> Result<Self> {
        if !Arc::ptr_eq(&self.owner, &decision.owner) || self.generation != decision.generation {
            return Err(Error::StaleBinding);
        }
        self.inner
            .validate_decision_v72(&decision.backend)
            .map_err(|_| Error::StaleBinding)?;
        if packet_from_current(
            decision.backend.candidates_v72(),
            decision.backend.actions(),
        )? != decision.packet
        {
            return Err(Error::StaleBinding);
        }
        let inner = self
            .inner
            .sampled_clone_v72(&decision.backend, seed)
            .map_err(|e| Error::Projection(e.to_string()))?;
        let sampled = Self::from_tinybones(inner);
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
    pub(super) fn fixture_state(&self) -> &crate::state::GameState {
        self.inner.fixture_state_v72()
    }
    #[cfg(all(
        test,
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    pub(super) fn fixture_counts(&self) -> (u64, u64) {
        self.inner.fixture_counts_v72()
    }
    pub fn snapshot(&self) -> TinybonesSnapshot {
        TinybonesSnapshot {
            inner: self.inner.snapshot_v72(),
            owner: self.owner.clone(),
        }
    }
    pub fn restore(&mut self, snapshot: &TinybonesSnapshot) -> Result<()> {
        if !Arc::ptr_eq(&self.owner, &snapshot.owner) {
            return Err(Error::ForeignSnapshot);
        }
        let next = self.generation.checked_add(1).ok_or(Error::Range)?;
        // Backend authenticates the original binding before monotonic rebind.
        self.inner
            .restore_v72(&snapshot.inner)
            .map_err(|_| Error::StaleBinding)?;
        self.generation = next;
        Ok(())
    }
    pub fn encode(&self, expected: FastActorDecisionV1) -> Result<TinybonesDecision> {
        let backend = self.inner.decision_v72().map_err(|_| Error::StaleBinding)?;
        if backend.expected() != expected {
            return Err(Error::StaleBinding);
        }
        let packet = packet_from_current(backend.candidates_v72(), backend.actions())?;
        Ok(TinybonesDecision {
            backend,
            packet,
            owner: self.owner.clone(),
            generation: self.generation,
        })
    }
    pub fn apply(
        &mut self,
        decision: &TinybonesDecision,
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
        self.inner.step_with_current_check(
            &decision.backend,
            selected_index,
            |candidates, actions| {
                if packet_from_current(candidates, actions)? != decision.packet {
                    return Err(Error::StaleBinding);
                }
                Ok(())
            },
            |_| Error::StaleBinding,
            |error| Error::Projection(error.to_string()),
        )
    }
    pub fn score_and_apply(
        &mut self,
        decision: &TinybonesDecision,
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
    if value.get("schema").and_then(Value::as_str)
        != Some(crate::rules_meaning_tinybones_v72::SCHEMA)
    {
        return Ok(false);
    }
    let meaning: crate::rules_meaning_tinybones_v72::MeaningV72 =
        serde_json::from_value(Value::Object(value.clone()))
            .map_err(|e| Error::Projection(e.to_string()))?;
    crate::rules_meaning_tinybones_v72::validate(&meaning).map_err(Error::Projection)?;
    covered.insert(meaning.card_id);
    Ok(true)
}
pub(super) fn printing(id: u16) -> Result<Option<Value>> {
    if id != 490 {
        return Ok(None);
    }
    serde_json::to_value(
        crate::rules_meaning_tinybones_v72::printed_meaning().map_err(Error::Projection)?,
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
