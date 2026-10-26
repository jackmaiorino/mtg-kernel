//! Rules-only admission for Limited self-play (issue #110; the maintainer chose
//! a rules-only tier on 2026-10-26). A Partial printing listed here has
//! executable rules that played natural rules-only games without engine halts
//! (see `docs/design/limited_rules_only_v1.md` for the evidence). Admission
//! is for self-play and search only; it never makes a printing Full and adds
//! no observation, Human, model or transport support.
use crate::card_def::CARD_DEFS;

/// Admitted Partial printings, by exact name.
pub const RULES_ONLY_ADMITTED_V1: &[&str] = &[];

pub fn admitted_v1(id: u16) -> bool {
    CARD_DEFS
        .get(usize::from(id))
        .is_some_and(|card| card.is_executable() && RULES_ONLY_ADMITTED_V1.contains(&card.name))
}
