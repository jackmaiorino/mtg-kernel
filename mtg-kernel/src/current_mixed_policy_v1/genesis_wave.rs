//! Distinct numeric Genesis Wave V69 owner. No predecessor artifact or executable is encoded.
use super::{normalize, Error, NativeModel, Packet, Result, Source};
use crate::rl_session::{
    FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1, GenesisWaveDecisionV69,
    GenesisWaveSessionV69, GenesisWaveSnapshotV69,
};
use serde_json::{Map, Value};
use std::{collections::BTreeSet, sync::Arc};

pub struct GenesisWaveSession {
    inner: GenesisWaveSessionV69,
    owner: Arc<()>,
    generation: u64,
}
#[derive(Clone)]
pub struct GenesisWaveSnapshot {
    inner: GenesisWaveSnapshotV69,
    owner: Arc<()>,
}
/// Private executable authority stays in the sealed backend capability.
/// This type deliberately has no Debug, serde or public constructor.
#[derive(Clone)]
pub struct GenesisWaveDecision {
    backend: GenesisWaveDecisionV69,
    packet: Packet,
    owner: Arc<()>,
    generation: u64,
}
impl GenesisWaveDecision {
    pub fn packet(&self) -> &Packet {
        &self.packet
    }
}
// Pure normalization of the exact freshly validated backend input. This
// function cannot capture another decision or acquire executable authority.
fn packet_from_current(
    candidates: &crate::genesis_wave_candidates_v69::GenesisWaveCandidatesV69,
    actions: &[crate::rl::ActionSemanticV1],
) -> Result<Packet> {
    crate::genesis_wave_candidates_v69::validate_artifact(candidates)
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
    normalize::encode(Source::GenesisWaveV69, state, actions)
}
impl GenesisWaveSession {
    pub fn from_session(session: FastActorSessionV1) -> Result<Self> {
        Ok(Self::from_genesis_wave(
            GenesisWaveSessionV69::from_session(session)
                .map_err(|e| Error::Projection(e.to_string()))?,
        ))
    }
    /// Transfer a genuine sealed current owner, never an observation/world DTO.
    pub fn from_genesis_wave(inner: GenesisWaveSessionV69) -> Self {
        Self {
            inner,
            owner: Arc::new(()),
            generation: 0,
        }
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
        self.inner.fixture_state_v69()
    }
    pub fn snapshot(&self) -> GenesisWaveSnapshot {
        GenesisWaveSnapshot {
            inner: self.inner.snapshot_v69(),
            owner: self.owner.clone(),
        }
    }
    pub fn restore(&mut self, snapshot: &GenesisWaveSnapshot) -> Result<()> {
        if !Arc::ptr_eq(&self.owner, &snapshot.owner) {
            return Err(Error::ForeignSnapshot);
        }
        let next = self.generation.checked_add(1).ok_or(Error::Range)?;
        // Backend authenticates the original binding before monotonic rebind.
        self.inner
            .restore_v69(&snapshot.inner)
            .map_err(|_| Error::StaleBinding)?;
        self.generation = next;
        Ok(())
    }
    pub fn encode(&self, expected: FastActorDecisionV1) -> Result<GenesisWaveDecision> {
        let backend = self.inner.decision_v69().map_err(|_| Error::StaleBinding)?;
        if backend.expected() != expected {
            return Err(Error::StaleBinding);
        }
        let packet = packet_from_current(backend.candidates_v69(), backend.actions())?;
        Ok(GenesisWaveDecision {
            backend,
            packet,
            owner: self.owner.clone(),
            generation: self.generation,
        })
    }
    pub fn apply(
        &mut self,
        decision: &GenesisWaveDecision,
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
        decision: &GenesisWaveDecision,
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
        != Some(crate::rules_meaning_genesis_wave_v69::SCHEMA)
    {
        return Ok(false);
    }
    let meaning: crate::rules_meaning_genesis_wave_v69::MeaningV69 =
        serde_json::from_value(Value::Object(value.clone()))
            .map_err(|e| Error::Projection(e.to_string()))?;
    crate::rules_meaning_genesis_wave_v69::validate(&meaning).map_err(Error::Projection)?;
    covered.insert(meaning.card_id);
    Ok(true)
}
pub(super) fn printing(id: u16) -> Result<Option<Value>> {
    if id != 487 {
        return Ok(None);
    }
    serde_json::to_value(
        crate::rules_meaning_genesis_wave_v69::printed_meaning().map_err(Error::Projection)?,
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
