//! Rules-only admission for Limited self-play (issue #110; the maintainer chose
//! a rules-only tier on 2026-10-26). A Partial printing listed here has
//! executable rules whose natural rules-only games halt no more often than
//! games with only Full cards (see `docs/design/limited_rules_only_v1.md` for
//! the rule and the evidence). Admission is for self-play and search only; it
//! never makes a printing Full and adds no observation, Human, model or
//! transport support.
use crate::card_def::CARD_DEFS;

/// Admitted Partial printings, by exact name.
pub const RULES_ONLY_ADMITTED_V1: &[&str] = &[
    "Abyssal Harvester",
    "Blasphemous Edict",
    "Chandra, Flameshaper",
    "Consuming Aberration",
    "Curator of Destinies",
    "Drakuseth, Maw of Flames",
    "Embercleave",
    "Imprisoned in the Moon",
    "Kykar, Zephyr Awakener",
    "Leyline Axe",
    "Loot, Exuberant Explorer",
    "Muldrotha, the Gravetide",
    "Quilled Greatwurm",
    "Sphinx of Forgotten Lore",
    "Thousand-Year Storm",
    "Tinybones, Bauble Burglar",
    "Valkyrie's Call",
    "Vivien Reid",
    "Zimone, Paradox Sculptor",
];

pub fn admitted_v1(id: u16) -> bool {
    CARD_DEFS
        .get(usize::from(id))
        .is_some_and(|card| card.is_executable() && RULES_ONLY_ADMITTED_V1.contains(&card.name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card_def::card_id_by_name;

    #[test]
    fn every_admitted_name_is_an_executable_partial_printing() {
        for name in RULES_ONLY_ADMITTED_V1 {
            let id = card_id_by_name(name).unwrap_or_else(|| panic!("{name:?} is not a printing"));
            let card = &CARD_DEFS[usize::from(id)];
            assert!(card.is_executable(), "{name:?} has no executable rules");
            assert!(
                !card.capability.is_fully_supported(),
                "{name:?} is Full and needs no admission"
            );
            assert!(admitted_v1(id), "{name:?} is listed but not admitted");
        }
        let mut sorted = RULES_ONLY_ADMITTED_V1.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted, RULES_ONLY_ADMITTED_V1,
            "keep the list sorted and unique"
        );
    }
}
