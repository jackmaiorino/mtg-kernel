//! Explicit, narrow successor for Limited's AnyTarget recipient meaning.
//! No vector vocabulary, tensor, model or whole-card completeness is admitted.

use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::card_def::{TargetSpec, CARD_DEFS};
use crate::rules_vector_v1::facets::{CardTypeF, ObjF, TargetAtom, ZoneF};

pub const RULES_MEANING_SCHEMA_V2: &str = "mtg-kernel/rules-meaning/any-target/v2";
pub const CATALOG_SCOPE_V2: &str = "pauper+fdn/limited-without-standard";
pub const MEANING_SCOPE_V2: &str = "any-target-recipient-union-only";

/// Definition-owned origin, never an index into flattened predecessor atoms.
/// Trigger program indices select the engine's event-time alternatives, or zero
/// for its ordinary printed root. A modal trigger branch is not inferred.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum AbilityRootV2 {
    SpellMode {
        mode: u8,
    },
    Omen,
    Adventure,
    Activated {
        ability_index: u16,
    },
    EquipmentGrantedActivated,
    Triggered {
        ability_index: u16,
        program_index: u16,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum RecipientClassV2 {
    PlayerYou,
    PlayerOpponent,
    Creature,
    Planeswalker,
}

/// Explicit caller selection. All fields must match the current producer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MeaningSelectorV2 {
    pub schema: String,
    pub catalog_scope: String,
    pub kernel_carddb_hash: u64,
    pub producer_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeaningErrorV2 {
    UnsupportedCatalog,
    WrongSelector,
    UnknownCard,
    NonExecutableSource,
    MissingAbilityRoot,
    UnsupportedTargetSpec,
}

/// Target facts associated with one authenticated printed ability root.
/// Source/root hashes are provenance, not mechanical features. This record
/// deliberately contains no inherited effect facts or encoded feature vector.
#[derive(Debug, Clone, Serialize)]
pub struct AnyTargetMeaningV2 {
    pub schema: &'static str,
    pub catalog_scope: &'static str,
    pub meaning_scope: &'static str,
    pub kernel_carddb_hash: u64,
    pub producer_sha256: String,
    pub predecessor_extractor_sha256: String,
    pub card_id: u16,
    pub ability_root: AbilityRootV2,
    pub root_sha256: String,
    pub target_spec: TargetSpec,
    pub target_count: u8,
    pub recipients: Vec<RecipientClassV2>,
    /// Existing facet vocabulary serialized by its own serializer; its module
    /// stays crate-private and every predecessor source file stays unchanged.
    pub target_facts: Value,
}

impl AnyTargetMeaningV2 {
    pub fn canonical_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("typed target meaning serializes")
    }

    pub fn output_sha256(&self) -> String {
        digest(&self.canonical_bytes())
    }
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Actual source digest, not a fabricated generated catalog identity. Include
/// the resolver, engine recipient rules, definitions/programs and facet types.
pub fn producer_sha256_v2() -> String {
    let sources = [
        ("rules_meaning_v2.rs", include_str!("rules_meaning_v2.rs")),
        ("engine.rs", include_str!("engine.rs")),
        ("card_def.rs", include_str!("card_def.rs")),
        ("trigger.rs", include_str!("trigger.rs")),
        ("effect.rs", include_str!("effect.rs")),
        ("build.rs", include_str!("../build.rs")),
        ("facets.rs", include_str!("rules_vector_v1/facets.rs")),
    ];
    let mut hash = Sha256::new();
    hash.update(RULES_MEANING_SCHEMA_V2.as_bytes());
    for (path, source) in sources {
        hash.update([0]);
        hash.update(path.as_bytes());
        hash.update([0]);
        hash.update(source.as_bytes());
    }
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn live_selector_v2() -> Result<MeaningSelectorV2, MeaningErrorV2> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Err(MeaningErrorV2::UnsupportedCatalog);
    }
    Ok(MeaningSelectorV2 {
        schema: RULES_MEANING_SCHEMA_V2.into(),
        catalog_scope: CATALOG_SCOPE_V2.into(),
        kernel_carddb_hash: crate::card_def::KERNEL_CARDDB_HASH,
        producer_sha256: producer_sha256_v2(),
    })
}

/// Resolve from live definition-owned programs, not a caller-supplied effect,
/// name label, old aggregate record, or stored predecessor vector.
pub fn any_target_meaning_v2(
    selector: &MeaningSelectorV2,
    card_id: u16,
    root: AbilityRootV2,
) -> Result<AnyTargetMeaningV2, MeaningErrorV2> {
    let live = live_selector_v2()?;
    if selector != &live {
        return Err(MeaningErrorV2::WrongSelector);
    }
    let def = CARD_DEFS
        .get(usize::from(card_id))
        .ok_or(MeaningErrorV2::UnknownCard)?;
    if !def.is_executable() {
        return Err(MeaningErrorV2::NonExecutableSource);
    }
    let missing = MeaningErrorV2::MissingAbilityRoot;
    let (spec, program, additional_identity) = match root {
        AbilityRootV2::SpellMode { mode: 0 } => (
            def.target_spec,
            (def.spell_effect)().ok_or(missing)?,
            Value::Null,
        ),
        AbilityRootV2::SpellMode { mode } => {
            let mode = match mode {
                1 => def.mode2.as_ref(),
                2 => def.mode3.as_ref(),
                _ => None,
            }
            .ok_or(missing)?;
            (mode.target_spec, (mode.effect)(), Value::Null)
        }
        AbilityRootV2::Omen => {
            let form = def.omen.as_ref().ok_or(missing)?;
            (form.target_spec, (form.effect)(), Value::Null)
        }
        AbilityRootV2::Adventure => {
            let form = def.adventure.as_ref().ok_or(missing)?;
            (form.target_spec, (form.effect)(), Value::Null)
        }
        AbilityRootV2::Activated { ability_index } => {
            let ability = def
                .activated_abilities
                .get(usize::from(ability_index))
                .ok_or(missing)?;
            (
                ability.target_spec,
                (ability.effect)(),
                json!({"activation_zone": ability.activation_zone}),
            )
        }
        AbilityRootV2::EquipmentGrantedActivated => {
            let ability = def
                .equipment
                .as_ref()
                .and_then(|equipment| equipment.granted_activated_ability.as_ref())
                .ok_or(missing)?;
            (ability.target_spec, (ability.effect)(), Value::Null)
        }
        AbilityRootV2::Triggered {
            ability_index,
            program_index,
        } => {
            let ability = crate::trigger::triggers_for(card_id)
                .get(usize::from(ability_index))
                .ok_or(missing)?;
            let mut programs =
                crate::trigger::event_time_trigger_programs(card_id, ability.condition);
            if programs.is_empty() {
                programs.push((ability.effect)());
            }
            let program = programs
                .into_iter()
                .nth(usize::from(program_index))
                .ok_or(missing)?;
            let spec = crate::trigger::target_spec_for_trigger(card_id, &program)
                .unwrap_or(TargetSpec::None);
            (
                spec,
                program,
                json!({
                    "condition": format!("{:?}", ability.condition),
                    "home_zone": ability.home_zone,
                    "face_index": ability.face_index,
                    "intervening_if_kicked": ability.intervening_if_kicked,
                    "intervening_if_controls_another_source_card": ability.intervening_if_controls_another_source_card,
                }),
            )
        }
    };
    if spec != TargetSpec::AnyTarget {
        return Err(MeaningErrorV2::UnsupportedTargetSpec);
    }
    let root_bytes = serde_json::to_vec(&json!({
        "card_id": card_id,
        "ability_root": root,
        "target_spec": spec,
        "program": program,
        "additional_identity": additional_identity,
    }))
    .expect("definition-owned root serializes");
    let object = |card_type| TargetAtom::Object {
        obj: ObjF::Typed(card_type),
        controller: None,
        zone: ZoneF::Battlefield,
        color: None,
        mana_value_at_most: None,
        excludes: None,
    };
    Ok(AnyTargetMeaningV2 {
        schema: RULES_MEANING_SCHEMA_V2,
        catalog_scope: CATALOG_SCOPE_V2,
        meaning_scope: MEANING_SCOPE_V2,
        kernel_carddb_hash: live.kernel_carddb_hash,
        producer_sha256: live.producer_sha256,
        predecessor_extractor_sha256: crate::rules_vector_v1::extractor_sha256(),
        card_id,
        ability_root: root,
        root_sha256: digest(&root_bytes),
        target_spec: spec,
        target_count: crate::engine::target_count(spec),
        recipients: vec![
            RecipientClassV2::PlayerYou,
            RecipientClassV2::PlayerOpponent,
            RecipientClassV2::Creature,
            RecipientClassV2::Planeswalker,
        ],
        target_facts: serde_json::to_value([
            TargetAtom::PlayerYou,
            TargetAtom::PlayerOpponent,
            object(CardTypeF::Creature),
            object(CardTypeF::Planeswalker),
        ])
        .expect("existing typed target facets serialize"),
    })
}

#[cfg(test)]
mod tests;
