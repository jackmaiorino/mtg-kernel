//! Export public catalog metadata from the exact compiled Pauper registry.
//! This command performs no game, scoring, training or hidden-state access.
use mtg_kernel::card_def::{CardDef, Keywords, CARD_DEFS, KERNEL_CARDDB_HASH};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::runtime_decks::RUNTIME_DECKS;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

fn normalized(value: impl std::fmt::Debug) -> String {
    let raw = format!("{value:?}");
    let raw = raw.strip_suffix("AllCaps").unwrap_or(&raw);
    let mut out = String::new();
    for (index, ch) in raw.chars().enumerate() {
        if index > 0 && ch.is_ascii_uppercase() {
            out.push('_');
        }
        out.push(ch.to_ascii_lowercase());
    }
    out
}

fn keywords(value: Keywords) -> Vec<&'static str> {
    [
        ("flying", Keywords::FLYING),
        ("reach", Keywords::REACH),
        ("haste", Keywords::HASTE),
        ("vigilance", Keywords::VIGILANCE),
        ("trample", Keywords::TRAMPLE),
        ("first_strike", Keywords::FIRST_STRIKE),
        ("double_strike", Keywords::DOUBLE_STRIKE),
        ("deathtouch", Keywords::DEATHTOUCH),
        ("menace", Keywords::MENACE),
        ("defender", Keywords::DEFENDER),
        ("lifelink", Keywords::LIFELINK),
        ("hexproof", Keywords::HEXPROOF),
        ("indestructible", Keywords::INDESTRUCTIBLE),
        (
            "protection_from_monocolored",
            Keywords::PROTECTION_FROM_MONOCOLORED,
        ),
        ("islandwalk", Keywords::ISLANDWALK),
        ("flash", Keywords::FLASH),
        ("cant_be_blocked", Keywords::CANT_BE_BLOCKED),
    ]
    .into_iter()
    .filter_map(|(name, bit)| value.has(bit).then_some(name))
    .collect()
}

fn color(color: ManaColor) -> Option<&'static str> {
    match color {
        ManaColor::W => Some("white"),
        ManaColor::U => Some("blue"),
        ManaColor::B => Some("black"),
        ManaColor::R => Some("red"),
        ManaColor::G => Some("green"),
        ManaColor::C => None,
    }
}

fn full_name(def: &CardDef) -> String {
    match def.transform_face.as_ref() {
        Some(face) => format!("{} // {}", def.name, face.name),
        None => def.name.to_owned(),
    }
}

fn spell_forms(def: &CardDef, face_index: u8) -> Value {
    let mut forms = serde_json::Map::new();
    if face_index == 0 {
        if let Some((cost, types)) = def.omen_spell_form() {
            let colors: BTreeSet<_> = cost
                .pips
                .iter()
                .flat_map(|pip| match pip {
                    Pip::Colored(c) | Pip::Phyrexian(c) | Pip::PhyrexianAnyColor(c) => vec![*c],
                    Pip::Hybrid(a, b) => vec![*a, *b],
                })
                .filter_map(color)
                .collect();
            forms.insert(
                "omen".to_owned(),
                json!({
                    "cost": cost,
                    "x_count": cost.x_count,
                    "characteristics": {
                        "supertypes": [], "types": types.iter().map(normalized).collect::<Vec<_>>(),
                        "subtypes": [], "colors": colors,
                        "mana_value": u32::from(cost.generic) + cost.pips.len() as u32,
                        "power": null, "toughness": null, "keywords": []
                    }
                }),
            );
        }
    }
    Value::Object(forms)
}

fn main() {
    let mut cards = BTreeMap::<String, Value>::new();
    let mut subtypes = BTreeMap::<u16, String>::new();
    for (card_db_id, def) in CARD_DEFS.iter().enumerate() {
        for face_index in 0..=u8::from(def.transform_face.is_some()) {
            let name = if face_index == 1 {
                def.transform_face.as_ref().unwrap().name
            } else {
                def.name
            };
            for subtype in def.subtypes_for_face(face_index) {
                subtypes.insert(*subtype as u16, normalized(subtype));
            }
            cards.insert(name.to_owned(), json!({
                "full_name": full_name(def),
                "card_db_id": card_db_id,
                "face": face_index,
                "x_count": def.cost.x_count,
                "spell_forms": spell_forms(def, face_index),
                "characteristics": {
                    "supertypes": def.supertypes.iter().map(normalized).collect::<Vec<_>>(),
                    "types": def.types_for_face(face_index).iter().map(normalized).collect::<Vec<_>>(),
                    "subtypes": def.subtypes_for_face(face_index).iter().map(normalized).collect::<BTreeSet<_>>(),
                    "colors": def.colors_for_face(face_index).iter().filter_map(|c| color(*c)).collect::<Vec<_>>(),
                    "mana_value": u32::from(def.cost.generic) + def.cost.pips.len() as u32,
                    "power": def.power_for_face(face_index).map(i32::from),
                    "toughness": def.toughness_for_face(face_index).map(i32::from),
                    "keywords": keywords(def.keywords_for_face(face_index)),
                }
            }));
        }
    }
    let catalog = RUNTIME_DECKS.iter().filter(|deck| deck.id != "Terror").map(|deck| {
        let mut counts = BTreeMap::<String, u32>::new();
        for card_id in deck.card_ids {
            *counts.entry(full_name(&CARD_DEFS[usize::from(*card_id)])).or_default() += 1;
        }
        json!({"catalog_id": deck.id, "name": deck.id,
               "decklist": counts.into_iter().map(|(name, count)| json!({"name": name, "count": count})).collect::<Vec<_>>()})
    }).collect::<Vec<_>>();
    println!(
        "{}",
        json!({"schema": "spellbench-kernel-public-catalog/v1",
        "card_db_hash": format!("{KERNEL_CARDDB_HASH:016x}"),
        "catalog": catalog, "cards": cards, "subtypes": subtypes})
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alternative_forms_export_their_own_cost_and_characteristics() {
        for (name, mana_value, color) in [("Sagu Wildling", 1, "green"), ("Fang Dragon", 2, "red")]
        {
            let def = CARD_DEFS.iter().find(|def| def.name == name).unwrap();
            let forms = spell_forms(def, 0);
            let form = &forms["omen"];
            assert_eq!(form["x_count"], 0);
            assert_eq!(form["characteristics"]["mana_value"], mana_value);
            assert_eq!(form["characteristics"]["types"], json!(["sorcery"]));
            assert_eq!(form["characteristics"]["colors"], json!([color]));
            assert_eq!(form["characteristics"]["subtypes"], json!([]));
            assert_eq!(form["characteristics"]["keywords"], json!([]));
            assert!(form["characteristics"]["power"].is_null());
            assert!(form["characteristics"]["toughness"].is_null());
            assert!(spell_forms(def, 1).as_object().unwrap().is_empty());
        }
        let hydra = CARD_DEFS
            .iter()
            .find(|def| def.name == "Nyxborn Hydra")
            .unwrap();
        assert!(spell_forms(hydra, 0).as_object().unwrap().is_empty());
    }

    #[test]
    fn public_catalog_normalizes_printed_faerie_subtypes_and_mana_indices() {
        for name in ["Faerie Miscreant", "Spellstutter Sprite", "Faerie Seer"] {
            let def = CARD_DEFS.iter().find(|def| def.name == name).unwrap();
            let names: BTreeSet<_> = def.subtypes.iter().map(normalized).collect();
            assert!(names.contains("faerie"), "{name}: {names:?}");
            assert!(names.iter().all(|name| !name.ends_with("_all_caps")));
        }
        let gate = CARD_DEFS
            .iter()
            .find(|def| def.name == "Heap Gate")
            .unwrap();
        assert_eq!(gate.mana_ability_index(ManaColor::C, None), Some(0));
        assert_eq!(gate.mana_ability_index(ManaColor::W, None), Some(1));
    }
}
