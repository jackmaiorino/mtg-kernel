//! Serialize proved immutable Drake rows directly as checkpoint references.
//! The ordinary history serializer and JSON comparison remain the fallback.

use super::{CommittedEvent, GameState, JournalV1, RecordV1};
use crate::event_history_v1::EventHistoryV1;
use serde::{Serialize, Serializer};
use std::{
    cell::RefCell,
    collections::HashMap,
    rc::Rc,
    sync::{Arc, Weak},
};

const PROOF_LIMIT: usize = 4096;

thread_local! {
    static PREFIX: RefCell<Option<Rc<EventHistoryV1>>> = const { RefCell::new(None) };
    static SERIALIZABLE: RefCell<HashMap<*const RecordV1, Weak<RecordV1>>> =
        RefCell::new(HashMap::new());
}

struct RestorePrefix(Option<Rc<EventHistoryV1>>);

impl Drop for RestorePrefix {
    fn drop(&mut self) {
        PREFIX.with(|slot| *slot.borrow_mut() = self.0.take());
    }
}

/// The caller has just serialized this entire event and checked its exact
/// whole-value match. A weak Arc keeps the address reserved. Mutating the
/// record through Arc::make_mut detaches from that weak allocation.
pub(super) fn remember_serializable(record: &Arc<RecordV1>) {
    if EventHistoryV1::serde_omission_active_v2() || PREFIX.with(|slot| slot.borrow().is_some()) {
        return;
    }
    SERIALIZABLE.with(|proofs| {
        let mut proofs = proofs.borrow_mut();
        let address = Arc::as_ptr(record);
        if proofs.contains_key(&address) {
            return;
        }
        if proofs.len() == PROOF_LIMIT {
            proofs.retain(|_, old| old.strong_count() != 0);
            if proofs.len() == PROOF_LIMIT {
                proofs.clear();
            }
        }
        proofs.insert(address, Arc::downgrade(record));
    });
}

pub(super) fn checkpoint_value(state: &GameState) -> serde_json::Result<serde_json::Value> {
    let history = &state.engine.event_history;
    let prefix = if SERIALIZABLE.with(|proofs| proofs.borrow().is_empty()) {
        None
    } else {
        history.drake_compaction_prefix_v2().map(Rc::new)
    };
    let _restore = RestorePrefix(PREFIX.with(|slot| slot.replace(prefix)));
    // serde_json::value::Serializer is stateless, including at nested sequence
    // elements. The memo certifies this serializer alone, with no omissions.
    history.omitted_v2(|| serde_json::to_value(state))
}

/// Full records are opaque to the JSON strip pass. Preserve their ordinary
/// serialization, including histories in their saved worlds, even inside a
/// compact checkpoint write.
pub(super) fn serialize_full_journal<S: Serializer>(
    value: &JournalV1,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let _restore = PREFIX.with(|slot| slot.take()).map(|prefix| RestorePrefix(Some(prefix)));
    value.serialize(serializer)
}

pub(crate) fn serialize_history<S: Serializer>(
    history: &EventHistoryV1,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let prefix = PREFIX.with(|slot| slot.borrow().clone());
    let Some(prefix) = prefix else {
        return serializer.collect_seq(history.iter());
    };

    struct Row<'a> {
        position: usize,
        event: &'a CommittedEvent,
        prefix: &'a EventHistoryV1,
    }

    impl Serialize for Row<'_> {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let CommittedEvent::DrakeJournalV1 { record } = self.event else {
                return self.event.serialize(serializer);
            };
            if u64::try_from(self.position).ok() == Some(record.index)
                && matches!(
                    self.prefix.get(self.position),
                    Some(CommittedEvent::DrakeJournalV1 { record: actual })
                        if Arc::ptr_eq(actual, record)
                )
                && SERIALIZABLE.with(|proofs| proofs.borrow().contains_key(&Arc::as_ptr(record)))
            {
                return CommittedEvent::DrakeHistoryRefV1 {
                    index: record.index,
                }
                .serialize(serializer);
            }
            self.event.serialize(serializer)
        }
    }

    serializer.collect_seq(history.iter().enumerate().map(|(position, event)| Row {
        position,
        event,
        prefix: &prefix,
    }))
}
