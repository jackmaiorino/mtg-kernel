//! Immediate captures created while a trigger batch is being placed. They wait
//! here until the entire current batch is on the stack (CR 603.3b). An empty
//! queue writes no `Hash` input and no serialized field, so every state that
//! never holds such a capture keeps its previously pinned bytes.
use crate::trigger::PendingTrigger;
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub(super) struct QueueV1(Vec<PendingTrigger>);

impl QueueV1 {
    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(super) fn take(&mut self) -> Vec<PendingTrigger> {
        std::mem::take(&mut self.0)
    }

    pub(super) fn extend(&mut self, triggers: impl IntoIterator<Item = PendingTrigger>) {
        self.0.extend(triggers);
    }
}

impl Hash for QueueV1 {
    fn hash<H: Hasher>(&self, h: &mut H) {
        if !self.0.is_empty() {
            b"waiting-placement-triggers/v1".hash(h);
            self.0.hash(h);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effect::{EffectOp, PlayerRef};
    use crate::ids::{ObjectId, PlayerId};
    use crate::state::GameState;

    #[derive(Default)]
    struct Writes(Vec<u8>);
    impl Hasher for Writes {
        fn finish(&self) -> u64 {
            0
        }
        fn write(&mut self, bytes: &[u8]) {
            self.0.extend_from_slice(bytes);
        }
    }

    fn hash_input(value: &impl Hash) -> Vec<u8> {
        let mut writes = Writes::default();
        value.hash(&mut writes);
        writes.0
    }

    fn game() -> GameState {
        let mut state =
            GameState::new_from_libraries(&[1, 2, 3], &[4, 5, 6, 7], |c| format!("card-{c}"), 99);
        state.draw_card(PlayerId::P0);
        state.draw_card(PlayerId::P1);
        state
    }

    fn capture() -> PendingTrigger {
        PendingTrigger {
            controller: PlayerId::P1,
            source: ObjectId(4),
            effect: EffectOp::DrawCards {
                player: PlayerRef::Controller,
                count: 1,
            },
            is_madness_offer: false,
            kicked: false,
            target_spec: Default::default(),
            targets: Vec::new(),
            target_contracts: Vec::new(),
            placement_ordered: false,
            source_contract: None,
            granted_by: None,
            optional_additional_cost_paid: None,
            paid_cost_refs: Vec::new(),
        }
    }

    #[test]
    fn empty_queue_writes_no_hash_input_or_serialized_field() {
        assert!(hash_input(&QueueV1::default()).is_empty());
        let engine = serde_json::to_value(crate::engine::EngineState::default()).unwrap();
        assert!(engine.get("waiting_placement_triggers").is_none());

        let queue = QueueV1(vec![capture()]);
        let mut expected = hash_input(b"waiting-placement-triggers/v1");
        expected.extend(hash_input(&vec![capture()]));
        assert_eq!(hash_input(&queue), expected);
    }

    /// Derived `Hash` writes each field in order with no separator, so main's
    /// input for this game is exactly the input of a state without the field.
    /// A waiting capture inserts only its own tagged bytes at one position;
    /// removing them gives the empty-queue input, which is therefore main's.
    /// The Pauper Brainstorm native golden pins the same property on main's
    /// recorded `state_hash` values.
    #[test]
    fn empty_queue_keeps_the_main_state_hash_input() {
        let empty = game();
        let mut waiting = empty.clone();
        waiting
            .engine
            .waiting_placement_triggers
            .extend([capture()]);
        let before = hash_input(&empty);
        let after = hash_input(&waiting);
        let inserted = hash_input(&waiting.engine.waiting_placement_triggers);
        assert!(!inserted.is_empty());
        assert_eq!(after.len(), before.len() + inserted.len());
        let prefix = before
            .iter()
            .zip(&after)
            .take_while(|(a, b)| a == b)
            .count();
        let suffix = before
            .iter()
            .rev()
            .zip(after.iter().rev())
            .take_while(|(a, b)| a == b)
            .count();
        let spliced = (before.len().saturating_sub(suffix)..=prefix.min(before.len())).any(|at| {
            let mut expected = before[..at].to_vec();
            expected.extend_from_slice(&inserted);
            expected.extend_from_slice(&before[at..]);
            expected == after
        });
        assert!(spliced, "a waiting capture must add only its tagged bytes");
    }

    #[test]
    fn waiting_queue_is_hashed_compared_cloned_and_round_tripped() {
        let empty = game();
        let mut waiting = empty.clone();
        waiting
            .engine
            .waiting_placement_triggers
            .extend([capture()]);
        assert_ne!(waiting, empty);
        assert_ne!(waiting.state_hash(), empty.state_hash());

        let cloned = waiting.clone();
        assert_eq!(cloned, waiting);
        assert_eq!(cloned.state_hash(), waiting.state_hash());

        for state in [&empty, &waiting] {
            let bytes = serde_json::to_vec(state).unwrap();
            let restored: GameState = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(&restored, state);
            assert_eq!(restored.state_hash(), state.state_hash());
            assert_eq!(serde_json::to_vec(&restored).unwrap(), bytes);
        }
        let engine = serde_json::to_value(&waiting.engine).unwrap();
        assert_eq!(
            engine["waiting_placement_triggers"],
            serde_json::to_value(vec![capture()]).unwrap()
        );

        let mut drained = waiting.clone();
        assert_eq!(
            drained.engine.waiting_placement_triggers.take(),
            vec![capture()]
        );
        assert_eq!(drained, empty);
        assert_eq!(drained.state_hash(), empty.state_hash());
    }
}
