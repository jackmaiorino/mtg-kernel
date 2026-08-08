//! Build-time validated runtime deck catalog.
//!
//! `build.rs` reads the checked-in `data/runtime_decks_v1.json`, resolves
//! every materialized card copy against the stable `cards_v1.json` array,
//! verifies the frozen deck hash, and emits the static definitions included
//! below. Ordinary release execution uses only those generated definitions;
//! the explicitly feature-gated Store V2 production capture guard reopens the
//! raw catalog no-follow solely to prove its exact digest against them.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeDeckDefinition {
    pub canonical_pool_order: u32,
    pub id: &'static str,
    pub source_path: &'static str,
    pub source_sha256: &'static str,
    pub mainboard_count: usize,
    pub runtime_deck_hash: u64,
    pub card_ids: &'static [u16],
}

impl RuntimeDeckDefinition {
    pub fn is_fully_materialized(self) -> bool {
        self.mainboard_count == self.card_ids.len()
    }
}

include!(concat!(env!("OUT_DIR"), "/runtime_decks.rs"));

pub fn runtime_deck_by_id(id: &str) -> Option<&'static RuntimeDeckDefinition> {
    RUNTIME_DECKS.iter().find(|deck| deck.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card_def::preflight_fully_supported_deck;

    #[test]
    fn generated_runtime_catalog_is_exact_and_fully_supported() {
        assert_eq!(RUNTIME_DECK_CATALOG_SCHEMA, "kernel_runtime_decks/v1");
        assert_eq!(RUNTIME_DECK_PROTOCOL, "canonical-mainboard-bo1/v1");
        assert_eq!(
            RUNTIME_DECK_CATALOG_FILE_SHA256,
            "5ea19e8a08f0e9c9657e9a6a90382329785f27eeabbbe066e80e7025e8ee62c0"
        );
        assert_eq!(
            RUNTIME_DECK_MATERIALIZATION_PROTOCOL,
            "xmage_xml_row_then_copy_ordinal/v1"
        );
        assert_eq!(
            RUNTIME_DECK_HASH_ALGORITHM,
            "fnv1a64-serde-json-u16-array/v1"
        );
        assert_eq!(
            RUNTIME_DECKS.iter().map(|deck| deck.id).collect::<Vec<_>>(),
            ["Rally", "Burn"]
        );

        for deck in RUNTIME_DECKS {
            assert_eq!(deck.mainboard_count, 60, "{}", deck.id);
            assert!(deck.is_fully_materialized(), "{}", deck.id);
            assert_eq!(deck.source_sha256.len(), 64, "{}", deck.id);
            assert_ne!(deck.runtime_deck_hash, 0, "{}", deck.id);
            preflight_fully_supported_deck(deck.card_ids).unwrap();
        }
    }

    #[test]
    fn lookup_is_exact_case_and_hashes_are_frozen() {
        assert_eq!(
            runtime_deck_by_id("Burn").unwrap().runtime_deck_hash,
            0x5fdb_7b92_986b_6fc1
        );
        assert_eq!(
            runtime_deck_by_id("Rally").unwrap().runtime_deck_hash,
            0x0c9f_01c2_5444_12bf
        );
        for unsupported in ["burn", "BURN", "rally", "RALLY", "Terror", ""] {
            assert!(runtime_deck_by_id(unsupported).is_none(), "{unsupported:?}");
        }
    }
}
