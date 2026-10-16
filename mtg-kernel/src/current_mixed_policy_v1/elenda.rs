//! Numeric V52 entry reuses the existing sealed owner, without privileged fields.
use super::{normalize, Error, NativeModel, Packet, Result, Source};
use crate::rl_session::{
    ElendaDecisionV52, ElendaSessionV52, ElendaSnapshotV52, FastActorDecisionV1,
    FastActorResponseV1, FastActorSessionV1,
};

pub struct ElendaSession {
    inner: ElendaSessionV52,
}
#[derive(Clone)]
pub struct ElendaSnapshot {
    inner: ElendaSnapshotV52,
}
/// Private executable authority stays in the sealed backend capability.
/// This type deliberately has no Debug, serde or public constructor.
#[derive(Clone)]
pub struct ElendaDecision {
    backend: ElendaDecisionV52,
    packet: Packet,
}
impl ElendaDecision {
    pub fn packet(&self) -> &Packet {
        &self.packet
    }
    #[cfg(all(
        test,
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    pub(super) fn fixture_packet_mut(&mut self) -> &mut Packet {
        &mut self.packet
    }
}
impl ElendaSession {
    pub fn from_session(session: FastActorSessionV1) -> Result<Self> {
        Ok(Self {
            inner: ElendaSessionV52::from_session(session)
                .map_err(|e| Error::Projection(e.to_string()))?,
        })
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.current_response()
    }
    #[cfg(all(
        test,
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    pub(super) fn fixture_state(&self) -> &crate::state::GameState {
        self.inner.fixture_state_v52()
    }
    #[cfg(all(
        test,
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    pub(super) fn fixture_counts(&self) -> (u64, u64) {
        self.inner.fixture_counts_v52()
    }
    pub fn snapshot(&self) -> ElendaSnapshot {
        ElendaSnapshot {
            inner: self.inner.snapshot_v52(),
        }
    }
    pub fn restore(&mut self, snapshot: &ElendaSnapshot) -> Result<()> {
        // The sealed owner authenticates original input before rebinding, and
        // advances both revision and Arc-associated generation on success.
        self.inner
            .restore_v52(&snapshot.inner)
            .map_err(|_| Error::ForeignSnapshot)
    }
    pub fn encode(&self, expected: FastActorDecisionV1) -> Result<ElendaDecision> {
        let backend = self.inner.decision_v52().map_err(|_| Error::StaleBinding)?;
        if backend.expected() != expected {
            return Err(Error::StaleBinding);
        }
        let state = serde_json::to_value(backend.observation())
            .map_err(|e| Error::Projection(e.to_string()))?;
        let actions = backend
            .actions()
            .iter()
            .map(serde_json::to_value)
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Projection(e.to_string()))?;
        let packet = normalize::encode(Source::ElendaV52, state, actions)?;
        Ok(ElendaDecision { backend, packet })
    }
    pub fn apply(
        &mut self,
        decision: &ElendaDecision,
        selected_index: u32,
    ) -> Result<FastActorResponseV1> {
        if usize::try_from(selected_index)
            .ok()
            .is_none_or(|i| i >= decision.packet.actions.len())
        {
            return Err(Error::ActionRange);
        }
        self.inner
            .validate_decision_v52(&decision.backend)
            .map_err(|_| Error::StaleBinding)?;
        if self.encode(decision.backend.expected())?.packet != decision.packet {
            return Err(Error::StaleBinding);
        }
        // The real owning step repeats complete current private validation
        // immediately before applying the original native ordered index.
        self.inner
            .step_v52(&decision.backend, selected_index)
            .map_err(|e| Error::Projection(e.to_string()))
    }
    pub fn score_and_apply(
        &mut self,
        decision: &ElendaDecision,
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

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;
