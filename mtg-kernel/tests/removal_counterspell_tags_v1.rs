//! Live-engine cross-check for `data/pauper_removal_counterspell_tags_v1.json`
//! (design section 3, W4). The tag file is hand-authored data derived from
//! `cards_v1.json` mechanics tags; this test is the ratified defense
//! against it silently drifting from what the engine actually implements
//! (design section 9 risk: "an unreviewed or drifted tag silently corrupts
//! the field with no failing test").

use mtg_kernel::card_def::{CardCapability, TargetSpec, CARD_DEFS};

// Loaded the same way `xmage_counter_reference_windows.rs` loads its fixture:
// `include_str!` relative to this test file, so the committed JSON is
// embedded at compile time rather than read at test-run time (no
// working-directory ambiguity between `cargo test` invocations from the
// crate root or the repo root).
const TAG_FILE_JSON: &str = include_str!("../../data/pauper_removal_counterspell_tags_v1.json");

#[derive(serde::Deserialize)]
struct TagFileV1 {
    schema: String,
    cards: Vec<TagRowV1>,
}

#[derive(serde::Deserialize)]
struct TagRowV1 {
    card_id: usize,
    name: String,
    requires_target: bool,
    is_counterspell: bool,
}

fn load_tag_file() -> TagFileV1 {
    serde_json::from_str(TAG_FILE_JSON).expect("tag file parses as TagFileV1")
}

fn is_counterspell_shaped(target_spec: TargetSpec) -> bool {
    matches!(
        target_spec,
        TargetSpec::AnySpellOnStack
            | TargetSpec::InstantSpellOnStack
            | TargetSpec::BlueSpellOnStack
            | TargetSpec::RedSpellOnStack
            | TargetSpec::ArtifactOrEnchantmentSpellOnStack
            | TargetSpec::SorcerySpellOnStack
            | TargetSpec::NoncreatureSpellOnStack
            | TargetSpec::ArtifactSpellOnStack
    )
}

#[test]
fn schema_is_the_expected_version() {
    let document = load_tag_file();
    assert_eq!(document.schema, "kernel_removal_counterspell_tags/v1");
}

#[test]
fn every_requires_target_row_matches_a_nontrivial_live_target_spec() {
    let document = load_tag_file();
    let mut checked = 0usize;
    for row in &document.cards {
        let Some(def) = CARD_DEFS.get(row.card_id) else {
            panic!("tag file row {:?} (card_id {}) has no CARD_DEFS entry", row.name, row.card_id);
        };
        assert_eq!(def.name, row.name, "card_id {} name drifted between the registry and the tag file", row.card_id);
        if def.capability != CardCapability::Full {
            // A not-yet-implemented card's target_spec is not yet meaningful
            // ground truth; the mechanics-tag claim is unverifiable until
            // the card is implemented, so it is skipped rather than failed.
            continue;
        }
        if row.requires_target {
            assert_ne!(
                def.target_spec, TargetSpec::None,
                "{} is tagged requires_target but CARD_DEFS reports TargetSpec::None",
                row.name
            );
            checked += 1;
        }
    }
    assert!(checked > 0, "at least one fully-implemented requires_target row must exist to exercise this check");
}

#[test]
fn every_is_counterspell_row_has_a_spell_on_stack_target_spec() {
    let document = load_tag_file();
    let mut checked = 0usize;
    for row in &document.cards {
        let def = &CARD_DEFS[row.card_id];
        if def.capability != CardCapability::Full {
            continue;
        }
        if row.is_counterspell {
            assert!(
                is_counterspell_shaped(def.target_spec),
                "{} is tagged is_counterspell but its live target_spec {:?} is not a *SpellOnStack variant",
                row.name, def.target_spec
            );
            checked += 1;
        }
    }
    assert!(checked > 0, "at least one fully-implemented is_counterspell row must exist to exercise this check");
}

#[test]
fn no_fully_implemented_full_targeting_spell_is_missing_from_the_tag_file() {
    // The converse direction: every implemented, non-trivially-targeting
    // spell must appear in the tag file with requires_target true, so the
    // generator's mechanics-tag set (python/tools/generate_removal_counterspell_tags_v1.py)
    // has not under-covered the registry.
    let document = load_tag_file();
    let tagged: std::collections::BTreeSet<usize> = document
        .cards
        .iter()
        .filter(|row| row.requires_target)
        .map(|row| row.card_id)
        .collect();
    let counterspell_specs = [
        TargetSpec::AnySpellOnStack,
        TargetSpec::InstantSpellOnStack,
        TargetSpec::BlueSpellOnStack,
        TargetSpec::RedSpellOnStack,
        TargetSpec::ArtifactOrEnchantmentSpellOnStack,
        TargetSpec::SorcerySpellOnStack,
        TargetSpec::NoncreatureSpellOnStack,
        TargetSpec::ArtifactSpellOnStack,
    ];
    for (card_id, def) in CARD_DEFS.iter().enumerate() {
        if def.capability != CardCapability::Full || def.is_token {
            continue;
        }
        if counterspell_specs.contains(&def.target_spec) && !tagged.contains(&card_id) {
            panic!(
                "{} (card_id {card_id}) has a counterspell-shaped target_spec {:?} but is absent from the tag file",
                def.name, def.target_spec
            );
        }
    }
}
