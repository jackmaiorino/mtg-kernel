//! Pauper, Legacy and Vintage catalog: the frozen Pauper prefix, the Standard
//! registry unchanged, then `data/eternal_v1/cards_v1.json`. Snapshot decks
//! whose whole mainboard and companion are Full resolve and play to natural
//! terminals, with both a fixed action preference and seeded random actions.
//! Games run on the Limited schema 5 session, whose policy observation
//! reports shroud on permanents and players.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, preflight_fully_supported_deck, CardCapability, DeckPreflightError,
    TargetSpec, CARD_DEFS, KERNEL_CARDDB_HASH,
};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1, LimitedJsonlServerV1};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Snapshot archetypes the eternal build plays, by format. A wave that
/// completes a deck adds it here; a regression removes one and fails. A
/// deck's companion is part of the deck the session plays.
const PLAYABLE: [(&str, &str); 136] = [
    ("pauper", "pauper-mono-red-madness"),
    (
        "pauper",
        "pauper-blue-terror-2b4f6710-ccb2-4660-b5f9-ea85c8875ec5",
    ),
    ("pauper", "elves-67156d3a-d400-4269-8400-3a0258a274b3"),
    (
        "pauper",
        "pauper-dimir-faeries-625747e4-1ba6-4401-af7b-5c8033a12c6c",
    ),
    ("pauper", "pauper-grixis-affinity"),
    ("pauper", "pauper-spy-combo"),
    ("pauper", "pauper-mono-red-rally"),
    (
        "pauper",
        "pauper-grixis-control-334e14c5-6fe9-46da-9c9b-799ef8bd1f0f",
    ),
    ("pauper", "pauper-jund-wildfire"),
    (
        "pauper",
        "pauper-dimir-control-1e2279c1-d135-4609-8b99-83f2ab98c424",
    ),
    ("pauper", "pauper-dimir-terror"),
    ("pauper", "pauper-grixis-faeries"),
    ("pauper", "tron-c244ab01-c9e3-400d-8db1-6d717616b7e2"),
    ("pauper", "pauper-esper-affinity"),
    ("pauper", "pauper-esper-ephemerate"),
    (
        "pauper",
        "pauper-mono-blue-faeries-f7d232c7-2c3c-4459-b219-f3e8d9abbec6",
    ),
    ("pauper", "pauper-dimir-affinity"),
    ("pauper", "pauper-mardu-synthesizer"),
    ("pauper", "pauper-4c-affinity"),
    ("pauper", "pauper-azorius-gates"),
    ("pauper", "bogles-df219f8f-1b57-437c-b4d8-dd4a5c7bdd7c"),
    ("pauper", "pauper-white-aggro"),
    ("pauper", "pauper-rakdos-madness"),
    ("pauper", "pauper-boros-moxite"),
    (
        "pauper",
        "pauper-mono-white-aggro-6b67ea64-5bde-42ea-8d1a-93578d3e3f3c",
    ),
    ("pauper", "pauper-snacker-gates"),
    ("pauper", "pauper-boros-synthesizer"),
    ("pauper", "pauper-azorius-faeries"),
    ("pauper", "pauper-ruby-storm"),
    ("pauper", "pauper-golgari-gardens"),
    ("pauper", "pauper-familiars"),
    ("pauper", "pauper-jeskai-ephemerate"),
    ("pauper", "pauper-glintblade"),
    ("pauper", "pauper-izzet-terror"),
    ("pauper", "burn-3854fe58-5ff9-4046-bac9-ae9617c3caf5"),
    ("pauper", "pauper-boros-bully"),
    ("pauper", "pauper-izzet-faeries"),
    (
        "pauper",
        "pauper-4c-synthesizer-f5246896-123e-417c-9560-63c3c542a0da",
    ),
    ("legacy", "legacy-dimir-tempo"),
    ("legacy", "legacy-izzet-delver"),
    ("legacy", "legacy-doomsday"),
    ("legacy", "legacy-sneak-and-show"),
    ("legacy", "legacy-aluren"),
    ("legacy", "legacy-omni-tell"),
    ("legacy", "legacy-temur-delver"),
    ("legacy", "legacy-boros-energy"),
    ("legacy", "legacy-beanstalk-control-non-yorion"),
    (
        "legacy",
        "legacy-azorius-tempo-9bd90024-e52f-4284-821d-f037ed910a44",
    ),
    (
        "legacy",
        "legacy-jeskai-tempo-7f301e99-6af3-4ead-a85a-5ebb92d56c0f",
    ),
    ("legacy", "legacy-esper-control"),
    ("legacy", "legacy-azorius-tempo"),
    ("legacy", "legacy-mystic-forge-combo"),
    ("legacy", "legacy-eldrazi"),
    ("legacy", "legacy-tron"),
    ("legacy", "legacy-sewer-cam-combo"),
    ("legacy", "legacy-rakdos-reanimator"),
    ("legacy", "legacy-the-epic-storm"),
    ("legacy", "legacy-stiflenought"),
    ("legacy", "legacy-mardu-energy"),
    ("legacy", "legacy-affinity-stompy"),
    ("legacy", "legacy-lands"),
    (
        "legacy",
        "legacy-death-and-taxes-yorion-black-white-468d6491-e6a9-46c8-b9fb-da35bfca78fa",
    ),
    ("legacy", "legacy-hogaak-moonshadow"),
    ("legacy", "legacy-jeskai-control"),
    ("legacy", "legacy-beseech-storm"),
    (
        "legacy",
        "legacy-saga-storm-8f40fd61-e011-4e94-8cd2-4e63013604ce",
    ),
    (
        "legacy",
        "legacy-death-and-taxes-yorion-mono-white-d867cd0f-702e-45a2-a765-734174ae83d3",
    ),
    ("legacy", "legacy-reanimator"),
    ("legacy", "legacy-dimir-control"),
    ("legacy", "legacy-azorius-control"),
    ("legacy", "legacy-8-cast"),
    ("legacy", "legacy-selesnya-depths"),
    ("legacy", "legacy-red-stompy"),
    ("legacy", "legacy-led-dredge"),
    ("legacy", "legacy-cephalid-breakfast"),
    ("legacy", "legacy-painter"),
    ("vintage", "vintage-jewel-shops"),
    ("vintage", "vintage-mono-white-initiative"),
    ("vintage", "vintage-esper-lurrus-control"),
    ("vintage", "vintage-dimir-lurrus-control"),
    ("vintage", "vintage-lurrus-po"),
    (
        "vintage",
        "vintage-lurrus-breach-9d1cc5d1-52c8-436c-8711-ee292b96e9d4",
    ),
    ("vintage", "vintage-jeskai-lurrus"),
    ("vintage", "vintage-4c-psychic-frog"),
    ("vintage", "vintage-raker-shops"),
    ("vintage", "vintage-oath-of-druids"),
    ("vintage", "vintage-paradoxical-outcome"),
    ("vintage", "vintage-blue-tinker"),
    ("vintage", "vintage-beseech-storm"),
    ("vintage", "vintage-dredge"),
    ("vintage", "vintage-counter-vine"),
    ("vintage", "vintage-blue-tempo"),
    ("vintage", "vintage-lurrus-drs"),
    ("vintage", "vintage-doomsday"),
    ("vintage", "vintage-rug-control"),
    ("vintage", "vintage-stiflenought"),
    ("vintage", "vintage-sphere-shops"),
    ("vintage", "vintage-sultai-psychic-frog"),
    (
        "pauper",
        "pauper-turbo-fog-fe589501-15e6-4511-b259-e066f9f36c51",
    ),
    ("pauper", "pauper-gruul-ponza"),
    ("pauper", "pauper-gruul-ramp"),
    ("pauper", "pauper-food-gardens"),
    ("pauper", "pauper-4c-pestilence"),
    ("pauper", "pauper-esper-faeries"),
    ("vintage", "vintage-scam"),
    ("vintage", "vintage-grixis-lurrus-control"),
    ("vintage", "vintage-underworld-breach"),
    ("vintage", "vintage-eldrazi"),
    ("vintage", "vintage-red-prison"),
    ("pauper", "pauper-4c-ephemerate"),
    ("legacy", "legacy-naya-initiative"),
    ("legacy", "legacy-mono-black-midrange"),
    ("legacy", "legacy-death-and-taxes-60-card"),
    ("premodern", "premodern-replenish"),
    ("premodern", "premodern-sligh-rdw"),
    ("premodern", "premodern-terrageddon"),
    ("premodern", "premodern-goblins"),
    ("premodern", "premodern-mono-black-aggro"),
    ("premodern", "premodern-rg-aggro"),
    ("premodern", "premodern-elves"),
    ("premodern", "premodern-stiflenought"),
    ("premodern", "premodern-oath-of-druids"),
    ("premodern", "premodern-landstill-uw-control"),
    ("premodern", "premodern-psychatog"),
    ("premodern", "premodern-grow"),
    ("premodern", "premodern-enchantress"),
    ("premodern", "premodern-white-weenie"),
    ("premodern", "premodern-mono-red-control"),
    ("premodern", "premodern-the-rock"),
    ("premodern", "premodern-mono-black-control"),
    ("premodern", "premodern-bw-control"),
    ("premodern", "premodern-stasis"),
    ("premodern", "premodern-other-control"),
    ("premodern", "premodern-survival"),
    ("premodern", "premodern-madness-threshold"),
    ("premodern", "premodern-reanimator"),
];

fn read_json(relative: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(relative);
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

fn registry_names(relative: &str) -> Vec<String> {
    read_json(relative)["cards"]
        .as_array()
        .unwrap()
        .iter()
        .map(|card| card["name"].as_str().unwrap().to_owned())
        .collect()
}

pub(crate) fn fully_supported(name: &str) -> bool {
    card_id_by_name(name)
        .is_some_and(|id| CARD_DEFS[id as usize].capability == CardCapability::Full)
}

/// Standard definitions precede the eternal registry. A Standard append
/// shifts every eternal id, which is allowed only before an eternal model
/// trains; change this with the eternal catalog version.
const STANDARD_DEFINITIONS: usize = 254;

pub(crate) struct SnapshotDeck {
    pub(crate) format: String,
    pub(crate) archetype: String,
    /// The mainboard and the companion, as the session takes them.
    pub(crate) deck: CustomDeckV1,
    /// The sideboard rows, the companion among them.
    pub(crate) sideboard: Vec<CustomCardCountV1>,
}

/// Metagame snapshots the catalog plays: mtggoldfish for Pauper, Legacy and
/// Vintage, mtgtop8 for Premodern.
const SNAPSHOTS: [&str; 2] = [
    "data/eternal_v1/meta_snapshot_mtggoldfish_20261026.json",
    "data/eternal_v1/meta_snapshot_mtgtop8_premodern_20261101.json",
];

/// Executable S1 implementations with known missing reachable branches.
const PARTIAL_S1: [&str; 6] = [
    "Coalition Honor Guard",
    "Hullbreacher",
    "Lavinia, Azorius Renegade",
    "Martyr of Sands",
    "Meddling Mage",
    "Standard Bearer",
];

/// Volrath's Shapeshifter and the S1 cards above are the registry's only
/// Partial definitions.
pub(crate) fn expected_capability(name: &str) -> CardCapability {
    if name == "Volrath's Shapeshifter" || PARTIAL_S1.contains(&name) {
        CardCapability::Partial
    } else {
        CardCapability::Full
    }
}

/// One zone's decklist rows.
fn snapshot_rows(rows: &Value) -> Vec<CustomCardCountV1> {
    rows.as_array()
        .unwrap()
        .iter()
        .map(|row| CustomCardCountV1 {
            name: row[1].as_str().unwrap().to_owned(),
            count: row[0].as_u64().unwrap() as _,
        })
        .collect()
}

/// Every snapshot deck, in snapshot order.
pub(crate) fn snapshot_decks() -> Vec<SnapshotDeck> {
    let mut decks = Vec::new();
    for snapshot in SNAPSHOTS.map(read_json) {
        for (format, archetypes) in snapshot["formats"].as_object().unwrap() {
            for archetype in archetypes.as_array().unwrap() {
                decks.push(SnapshotDeck {
                    format: format.clone(),
                    archetype: archetype["archetype"].as_str().unwrap().to_owned(),
                    deck: CustomDeckV1 {
                        cards: snapshot_rows(&archetype["mainboard"]),
                        companion: archetype["companion"].as_str().map(str::to_owned),
                    },
                    sideboard: snapshot_rows(&archetype["sideboard"]),
                });
            }
        }
    }
    decks
}

#[test]
fn eternal_registry_follows_the_pauper_and_standard_ids() {
    let pauper = registry_names("data/cards_v1.json");
    let standard = registry_names("data/standard/magezero_v1/cards_v1.json");
    let eternal = registry_names("data/eternal_v1/cards_v1.json");
    assert_eq!(standard.len(), STANDARD_DEFINITIONS);
    let expected: Vec<&String> = pauper.iter().chain(&standard).chain(&eternal).collect();
    assert_eq!(CARD_DEFS.len(), expected.len());
    for (id, name) in expected.into_iter().enumerate() {
        assert_eq!(CARD_DEFS[id].name, name, "id {id}");
        assert_eq!(
            CARD_DEFS[id].capability,
            expected_capability(name),
            "{name}"
        );
    }
    for name in PARTIAL_S1 {
        let id = card_id_by_name(name).unwrap();
        assert!(CARD_DEFS[usize::from(id)].is_executable());
        assert!(matches!(
            preflight_fully_supported_deck(&[id]),
            Err(DeckPreflightError::NotFullySupported {
                index: 0,
                card_def,
                name: rejected,
                capability: CardCapability::Partial,
            }) if card_def == id && rejected == name
        ));
    }
    println!("eternal catalog hash {KERNEL_CARDDB_HASH:#018x}");
}

#[test]
fn playable_snapshot_decks_match_the_tracked_list() {
    let decks = snapshot_decks();
    let playable: BTreeSet<(String, String)> = decks
        .iter()
        .filter(|deck| {
            deck.deck
                .cards
                .iter()
                .map(|card| card.name.as_str())
                .chain(deck.deck.companion.as_deref())
                .all(fully_supported)
        })
        .map(|deck| (deck.format.clone(), deck.archetype.clone()))
        .collect();
    let expected: BTreeSet<(String, String)> = PLAYABLE
        .iter()
        .map(|(format, archetype)| (format.to_string(), archetype.to_string()))
        .collect();
    assert_eq!(playable, expected);
}

fn rank(action: &Value) -> u8 {
    let s = &action["semantic"];
    match s["action_kind"].as_str().unwrap() {
        "play_land" => 0,
        "cast_spell" => 1,
        "choose_attacker_inclusion" | "choose_blocker_inclusion" if s["include"] == true => 2,
        "choose_combat_damage_range" if s["upper_half"] == false => 3,
        "pass" => 10,
        "activate_mana_ability" | "activate_ability" => 11,
        _ => 5,
    }
}

/// Whether `seat` controls a battlefield permanent named `name` in the
/// decision's public projection.
fn controls_named(decision: &Value, seat: &Value, name: &str) -> bool {
    decision["observation"]["projection"]["battlefield"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|cards| cards.as_array().into_iter().flatten())
        .any(|card| {
            &card["stable"]["controller"] == seat
                && card["stable"]["card_db_id"]
                    .as_u64()
                    .is_some_and(|id| CARD_DEFS[id as usize].name == name)
        })
}

/// Whether the opponent of `seat` controls a Hexing Squelcher, whose own
/// "Ward--Pay 2 life" and the one it grants their other creatures policy
/// observation v5 does not represent.
fn opponent_has_life_ward(decision: &Value, seat: &Value) -> bool {
    decision["observation"]["projection"]["battlefield"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|cards| cards.as_array().into_iter().flatten())
        .any(|card| {
            &card["stable"]["controller"] != seat
                && card["stable"]["card_db_id"]
                    .as_u64()
                    .is_some_and(|id| CARD_DEFS[id as usize].name == "Hexing Squelcher")
        })
}

/// Whether starting this spell or ability may target a permanent: its card
/// or ability has a target, or its entry trigger targets one.
fn may_target(action: &Value) -> bool {
    let s = &action["semantic"];
    let Some(id) = s["source"]["card_db_id"].as_u64() else {
        return false;
    };
    let def = &CARD_DEFS[id as usize];
    match s["action_kind"].as_str() {
        Some("cast_spell") => {
            def.target_spec != TargetSpec::None
                || def.mode2.is_some()
                || matches!(def.name, "Portable Hole" | "Witch Enchanter")
        }
        Some("activate_ability") => s["ability_index"]
            .as_u64()
            .and_then(|index| def.activated_abilities.get(index as usize))
            .is_some_and(|ability| ability.target_spec != TargetSpec::None),
        _ => false,
    }
}

/// Whether this action puts a companion into its owner's hand.
fn is_companion_action(action: &Value) -> bool {
    let s = &action["semantic"];
    s["action_kind"] == "activate_ability"
        && s["source"]["zone"] == serde_json::to_value(mtg_kernel::state::Zone::Exile).unwrap()
        && s["source"]["card_db_id"].as_u64().is_some_and(|id| {
            matches!(
                CARD_DEFS[id as usize].name,
                "Lurrus of the Dream-Den" | "Yorion, Sky Nomad"
            )
        })
}

/// Whether the public session can follow this action. Policy observation v5
/// fails closed while an Escape cast stages its graveyard exile (Sleep of the
/// Dead, and every nonland card under Underworld Breach) and once an extra
/// turn is scheduled (Emrakul, the Aeons Torn's cast trigger, Time Walk,
/// Time Vault's ability), so these games never begin either; the engine
/// paths are covered by the rules tests. Every other action is admitted:
/// schema 5 reports shroud (Sylvan Safekeeper's ability, Argothian
/// Enchantress, Sterling Grove, Solitary Confinement).
fn session_can_follow(action: &Value, decision: &Value) -> bool {
    let s = &action["semantic"];
    if !matches!(
        s["action_kind"].as_str(),
        Some("cast_spell" | "activate_ability")
    ) {
        return true;
    }
    let Some(id) = s["source"]["card_db_id"].as_u64() else {
        return true;
    };
    let id = id as usize;
    if matches!(
        CARD_DEFS[id].name,
        "Emrakul, the Aeons Torn" | "Time Walk" | "Time Vault"
    ) {
        return false;
    }
    s["action_kind"] != "cast_spell"
        || s["source"]["zone"] != serde_json::to_value(mtg_kernel::state::Zone::Graveyard).unwrap()
        || (CARD_DEFS[id].escape.is_none()
            && !controls_named(decision, &s["actor"], "Underworld Breach"))
}

/// Whether this action chooses a Ward permanent as a target.
fn targets_ward_permanent(action: &Value) -> bool {
    let target = &action["semantic"]["target"];
    target["target_kind"] == "object"
        && target["object"]["card_db_id"]
            .as_u64()
            .is_some_and(|id| CARD_DEFS[id as usize].ward_cost.is_some())
}

/// Whether this action chooses as a target an object whose controller
/// controls a Hexing Squelcher, which grants its other creatures "Ward--Pay 2
/// life" (policy observation v5 does not represent it).
fn targets_life_warded_object(action: &Value, decision: &Value) -> bool {
    let target = &action["semantic"]["target"];
    target["target_kind"] == "object"
        && controls_named(
            decision,
            &target["object"]["controller"],
            "Hexing Squelcher",
        )
}

/// Whether any public stack item's targets include a Ward permanent.
fn public_targets_ward_permanent(value: &Value) -> bool {
    match value {
        Value::Object(map) => {
            map.get("targets")
                .and_then(Value::as_array)
                .is_some_and(|targets| {
                    targets.iter().any(|target| {
                        target["target_kind"] == "object"
                            && target["object"]["card_db_id"]
                                .as_u64()
                                .is_some_and(|id| CARD_DEFS[id as usize].ward_cost.is_some())
                    })
                })
                || map.values().any(public_targets_ward_permanent)
        }
        Value::Array(items) => items.iter().any(public_targets_ward_permanent),
        _ => false,
    }
}

/// Whether any public stack item is controlled by `seat`.
fn controls_stack_item(value: &Value, seat: &Value) -> bool {
    match value {
        Value::Object(map) => {
            map.get("stack")
                .and_then(Value::as_array)
                .is_some_and(|items| items.iter().any(|item| &item["controller"] == seat))
                || map.values().any(|child| controls_stack_item(child, seat))
        }
        Value::Array(items) => items.iter().any(|child| controls_stack_item(child, seat)),
        _ => false,
    }
}

/// How many items on the public stack `seat` controls.
fn stack_items_controlled(value: &Value, seat: &Value) -> usize {
    match value {
        Value::Object(map) => match map.get("stack").and_then(Value::as_array) {
            Some(items) => items
                .iter()
                .filter(|item| &item["controller"] == seat)
                .count(),
            None => map
                .values()
                .map(|child| stack_items_controlled(child, seat))
                .max()
                .unwrap_or(0),
        },
        Value::Array(items) => items
            .iter()
            .map(|child| stack_items_controlled(child, seat))
            .max()
            .unwrap_or(0),
        _ => 0,
    }
}

/// Whether `action` answers an attacker scan step with `include`.
fn is_attacker_inclusion(action: &Value, include: bool) -> bool {
    action["semantic"]["action_kind"] == "choose_attacker_inclusion"
        && action["semantic"]["include"] == include
}

/// Whether the player `actor` attacks controls an Archon of Absolution on
/// the public battlefield (wave S1j).
fn archon_taxes(decision: &Value, actor: &Value) -> bool {
    let defender = match actor.as_str() {
        Some("p0") => 1,
        Some("p1") => 0,
        _ => return false,
    };
    decision["observation"]["projection"]["battlefield"][defender]
        .as_array()
        .is_some_and(|cards| {
            cards
                .iter()
                .any(|card| card["card_name"] == "Archon of Absolution")
        })
}

/// Adds each overloaded spell on the decision's public stack, by its
/// source's arena id and zone change count. An overload card's overload form
/// is cast with the Omen method, the paid alternative form of the card.
fn collect_overloaded_spells(decision: &Value, spells: &mut BTreeSet<(u64, u64)>) {
    let stack = decision["observation"]["projection"]["stack"].as_array();
    for item in stack.into_iter().flatten() {
        let source = &item["source"];
        let overload = source["card_db_id"].as_u64().is_some_and(|id| {
            matches!(CARD_DEFS[id as usize].name, "Vandalblast" | "Electrickery")
        });
        if let (true, Some("omen"), Some(arena), Some(changes)) = (
            overload,
            item["cast_method"].as_str(),
            source["arena_id"].as_u64(),
            source["zone_change_count"].as_u64(),
        ) {
            spells.insert((arena, changes));
        }
    }
}

/// How a test game picks among legal actions. `Ranked` casts and attacks
/// but never activates an ability; `Random` reaches activations, so lands,
/// mana rocks and activated abilities run too.
#[derive(Clone, Copy)]
pub(crate) enum Policy {
    Ranked,
    Random(u64),
}

pub(crate) fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

pub(crate) struct Game {
    pub(crate) transcript_sha256: String,
    pub(crate) action_counts: BTreeMap<String, usize>,
    pub(crate) casts_by_seat: [usize; 2],
    pub(crate) companion_actions: usize,
    /// The winning seat, or `None` for a drawn game.
    pub(crate) winner: Option<usize>,
    /// Spells cast per seat, by card definition id.
    pub(crate) cast_cards: [BTreeMap<u16, usize>; 2],
    /// Spell mode choices, by card definition id and chosen mode index.
    pub(crate) spell_modes: BTreeMap<(u16, u64), usize>,
    /// Replies carrying the chooser's Surgical Extraction and Extirpate
    /// search looks.
    pub(crate) search_looks: [usize; 2],
    /// Overloaded spells seen on the public stack, each counted once.
    pub(crate) overloaded_spells: usize,
    /// Attacker scan answers that include a creature while the defending
    /// player controls an Archon of Absolution (wave S1j), each part of a
    /// declaration whose cost is paid.
    pub(crate) taxed_inclusions: usize,
    /// Attacker scan steps that offer only to leave the creature out while
    /// an Archon of Absolution taxes its attack.
    pub(crate) unpaid_exclusions: usize,
    /// Circle of Protection source choices answered from the action list
    /// (waves S1d and P3g).
    pub(crate) circle_choices: usize,
}

/// Plays one game to a natural terminal and returns its transcript hash,
/// chosen action kinds, spells cast per seat and winner.
pub(crate) fn play(decks: [CustomDeckV1; 2], seed: u64, policy: Policy) -> Game {
    play_with_outside(decks, None, seed, policy)
}

/// `play` with each seat's cards outside the game, when given.
pub(crate) fn play_with_outside(
    decks: [CustomDeckV1; 2],
    outside_the_game: Option<[Vec<CustomCardCountV1>; 2]>,
    seed: u64,
    policy: Policy,
) -> Game {
    const MAX_STEPS: usize = 131_072;
    let mut server = LimitedJsonlServerV1::new_with_public_shroud_v1();
    let mut reset = json!({"schema_version":5,"request_type":"reset","request_id":"reset",
        "decks":decks,"episode_id":seed,"env_seed":seed,
        "max_physical_decisions":65536,"max_policy_steps":MAX_STEPS});
    if let Some(outside) = outside_the_game {
        reset["outside_the_game"] = json!(outside);
    }
    let mut request = reset.to_string();
    let mut transcript = Sha256::new();
    let mut action_counts = BTreeMap::new();
    let mut casts_by_seat = [0; 2];
    let mut cast_cards: [BTreeMap<u16, usize>; 2] = Default::default();
    let mut spell_modes = BTreeMap::new();
    let mut search_looks = [0; 2];
    let mut overloaded_spells = BTreeSet::new();
    let (mut taxed_inclusions, mut unpaid_exclusions) = (0, 0);
    let mut circle_choices = 0;
    let mut companion_actions = 0;
    // The last chosen actions, reported if a game fails closed.
    let mut recent: Vec<String> = Vec::new();
    let mut random_state = match policy {
        Policy::Random(state) => state,
        Policy::Ranked => 0,
    };
    // A game that loops without ending fails here with its last actions
    // instead of running until the CI job times out.
    let started = std::time::Instant::now();
    for index in 0..MAX_STEPS {
        assert!(
            started.elapsed() < std::time::Duration::from_secs(600),
            "seed={seed} random={} step={index}: game still running after 600 s\nlast actions:\n{}",
            matches!(policy, Policy::Random(_)),
            recent.join("\n")
        );
        let raw = server.handle_line(&request);
        transcript.update(request.as_bytes());
        transcript.update(b"\n");
        transcript.update(raw.as_bytes());
        transcript.update(b"\n");
        let reply: Value = serde_json::from_str(&raw).unwrap();
        assert_ne!(
            reply["response_type"], "error",
            "seed={seed} step={index}: {reply}"
        );
        for (count, look) in search_looks
            .iter_mut()
            .zip(["surgical_search_look_v1", "extirpate_search_look_v1"])
        {
            *count += usize::from(!reply[look].is_null());
        }
        if reply["response_type"] == "terminal" {
            assert_eq!(
                reply["terminal"]["terminal_code"],
                "natural_game_over",
                "seed={seed}: {reply}
last actions:
{}",
                recent.join(
                    "
"
                )
            );
            let winner = match reply["terminal"]["winner"].as_str() {
                Some("p0") => Some(0),
                Some("p1") => Some(1),
                None => None,
                Some(other) => panic!("seed={seed}: unexpected winner {other:?}"),
            };
            return Game {
                transcript_sha256: format!("{:x}", transcript.finalize()),
                action_counts,
                casts_by_seat,
                companion_actions,
                winner,
                cast_cards,
                spell_modes,
                search_looks,
                overloaded_spells: overloaded_spells.len(),
                taxed_inclusions,
                unpaid_exclusions,
                circle_choices,
            };
        }
        let decision = &reply["decision"];
        collect_overloaded_spells(decision, &mut overloaded_spells);
        let actions: Vec<&Value> = decision["legal_actions"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|action| session_can_follow(action, decision))
            .collect();
        if let [only] = actions[..] {
            if is_attacker_inclusion(only, false)
                && archon_taxes(decision, &only["semantic"]["actor"])
            {
                unpaid_exclusions += 1;
            }
        }
        // Policy observation v5 fails closed while two stack items target
        // one Ward permanent, so these games avoid targeting a Ward
        // permanent whenever another action is available, and start no new
        // spell or ability while something on the stack targets one. It
        // does not represent "Ward--Pay N life" at all, so while a seat's
        // opponent controls a Hexing Squelcher that seat starts no spell or
        // ability that may target a permanent.
        let ward_targeted = public_targets_ward_permanent(decision);
        let unwarded: Vec<&Value> = actions
            .iter()
            .copied()
            .filter(|action| {
                if targets_ward_permanent(action) || targets_life_warded_object(action, decision) {
                    return false;
                }
                let starts_new = matches!(
                    action["semantic"]["action_kind"].as_str(),
                    Some("cast_spell" | "activate_ability")
                );
                if starts_new
                    && may_target(action)
                    && opponent_has_life_ward(decision, &action["semantic"]["actor"])
                {
                    return false;
                }
                !ward_targeted || !starts_new
            })
            .collect();
        let actions = if unwarded.is_empty() {
            actions
        } else {
            unwarded
        };
        let action = match policy {
            // A ranked seat passes while its own spell or ability is on the
            // stack, so free recasts (Acererak under Aluren) cannot pile up
            // above a trigger forever.
            Policy::Ranked => match actions
                .iter()
                .find(|a| a["semantic"]["action_kind"] == "pass")
            {
                Some(pass) if controls_stack_item(decision, &pass["semantic"]["actor"]) => *pass,
                _ => *actions.iter().min_by_key(|a| rank(a)).unwrap(),
            },
            // A random seat starts no new spell or ability while it controls
            // two stack items, so a free ability (Nomads en-Kor's) cannot
            // pile up faster than the stack resolves.
            Policy::Random(_) => {
                let open: Vec<&Value> = actions
                    .iter()
                    .copied()
                    .filter(|action| {
                        !matches!(
                            action["semantic"]["action_kind"].as_str(),
                            Some("cast_spell" | "activate_ability")
                        ) || stack_items_controlled(decision, &action["semantic"]["actor"]) < 2
                    })
                    .collect();
                let actions = if open.is_empty() { &actions } else { &open };
                actions[(splitmix64(&mut random_state) % actions.len() as u64) as usize]
            }
        };
        if is_companion_action(action) {
            companion_actions += 1;
        }
        if is_attacker_inclusion(action, true)
            && archon_taxes(decision, &action["semantic"]["actor"])
        {
            taxed_inclusions += 1;
        }
        recent.push(action["semantic"].to_string());
        if recent.len() > 8 {
            recent.remove(0);
        }
        if action["semantic"]["action_kind"] == "cast_spell" {
            let actor: mtg_kernel::rl::PlayerSeatV1 =
                serde_json::from_value(action["semantic"]["actor"].clone()).unwrap();
            let seat = matches!(actor, mtg_kernel::rl::PlayerSeatV1::P1) as usize;
            casts_by_seat[seat] += 1;
            if let Some(id) = action["semantic"]["source"]["card_db_id"].as_u64() {
                *cast_cards[seat].entry(id as u16).or_default() += 1;
            }
        }
        if action["semantic"]["action_kind"] == "choose_effect_option"
            && action["semantic"]["source"]["card_db_id"]
                .as_u64()
                .is_some_and(|id| {
                    CARD_DEFS[id as usize]
                        .name
                        .starts_with("Circle of Protection")
                })
        {
            circle_choices += 1;
        }
        if action["semantic"]["action_kind"] == "choose_spell_mode" {
            if let (Some(source), Some(mode)) = (
                action["semantic"]["source"]["card_db_id"].as_u64(),
                action["semantic"]["mode_index"].as_u64(),
            ) {
                *spell_modes.entry((source as u16, mode)).or_default() += 1;
            }
        }
        *action_counts
            .entry(
                action["semantic"]["action_kind"]
                    .as_str()
                    .unwrap()
                    .to_owned(),
            )
            .or_default() += 1;
        request =
            json!({"schema_version":5,"request_type":"step","request_id":format!("step-{index}"),
            "episode_id":decision["episode_id"],"expected_step":decision["step"],
            "selected_index":action["selected_index"],"selected_action_id":action["stable_id"]})
            .to_string();
    }
    panic!("seed {seed} did not reach a natural terminal");
}

#[test]
fn premodern_elves_snapshot_finishes_and_replays_in_both_seats() {
    let decks = snapshot_decks();
    let lookup = |archetype| {
        decks
            .iter()
            .find(|deck| deck.format == "premodern" && deck.archetype == archetype)
            .unwrap_or_else(|| panic!("missing Premodern snapshot {archetype}"))
            .deck
            .clone()
    };
    let elves = lookup("premodern-elves");
    let goblins = lookup("premodern-goblins");
    let (ids, companion) = elves.resolve_with_companion().unwrap();
    assert_eq!(ids.len(), 60);
    assert!(companion.is_none());
    preflight_fully_supported_deck(&ids).unwrap();
    for printing in ["Birchlore Rangers", "Thornscape Battlemage"] {
        let id = card_id_by_name(printing).unwrap();
        assert_eq!(ids.iter().filter(|&&card| card == id).count(), 1);
    }

    for elves_seat in [0, 1] {
        let decks = if elves_seat == 0 {
            [elves.clone(), goblins.clone()]
        } else {
            [goblins.clone(), elves.clone()]
        };
        let seed = 0x454c_5645_5300 + elves_seat as u64;
        let game = play(decks.clone(), seed, Policy::Ranked);
        let replay = play(decks, seed, Policy::Ranked);
        assert!(
            game.casts_by_seat[elves_seat] > 0,
            "Elves must cast a spell"
        );
        assert_eq!(game.transcript_sha256, replay.transcript_sha256);
    }
}

#[test]
fn playable_snapshot_decks_finish_and_replay_through_public_sessions() {
    let decks: Vec<SnapshotDeck> = snapshot_decks()
        .into_iter()
        .filter(|deck| PLAYABLE.contains(&(deck.format.as_str(), deck.archetype.as_str())))
        .collect();
    assert_eq!(decks.len(), PLAYABLE.len());
    for deck in &decks {
        let (ids, companion) = deck
            .deck
            .resolve_with_companion()
            .unwrap_or_else(|error| panic!("{}: {error}", deck.archetype));
        preflight_fully_supported_deck(&ids)
            .unwrap_or_else(|error| panic!("{}: {error:?}", deck.archetype));
        assert_eq!(companion.is_some(), deck.deck.companion.is_some());
    }
    // Each deck plays the next one, so every deck takes both seats, once
    // with ranked actions and once with seeded random actions.
    let mut random_activations = 0;
    let mut companion_actions = 0;
    // Every deck casts a spell in its two ranked games (one per seat); a
    // single shuffle can leave one seat without a castable spell.
    let mut ranked_casts: BTreeMap<&str, usize> = BTreeMap::new();
    let mut failures: Vec<String> = Vec::new();
    for index in 0..decks.len() {
        let (first, second) = (&decks[index], &decks[(index + 1) % decks.len()]);
        let pair = [first.deck.clone(), second.deck.clone()];
        let seed = 0x4554_4552_0000 + index as u64;
        for policy in [Policy::Ranked, Policy::Random(seed)] {
            // Every pairing runs even after one fails, so a single run
            // reports all failing games (each panic message is printed).
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let game = play(pair.clone(), seed, policy);
                let replay = play(pair.clone(), seed, policy);
                assert_eq!(
                    game.transcript_sha256, replay.transcript_sha256,
                    "{} / {}",
                    first.archetype, second.archetype
                );
                game
            }));
            let Ok(game) = outcome else {
                failures.push(format!(
                    "{} / {} seed={seed} random={}",
                    first.archetype,
                    second.archetype,
                    matches!(policy, Policy::Random(_))
                ));
                continue;
            };
            companion_actions += game.companion_actions;
            if let Policy::Ranked = policy {
                *ranked_casts.entry(first.archetype.as_str()).or_default() += game.casts_by_seat[0];
                *ranked_casts.entry(second.archetype.as_str()).or_default() +=
                    game.casts_by_seat[1];
            } else {
                random_activations += game.action_counts.get("activate_ability").unwrap_or(&0);
            }
            println!(
                "{} / {} seed={seed} random={} transcript_sha256={} actions={:?}",
                first.archetype,
                second.archetype,
                matches!(policy, Policy::Random(_)),
                game.transcript_sha256,
                game.action_counts
            );
        }
    }
    assert!(
        failures.is_empty(),
        "failing games:
{}",
        failures.join(
            "
"
        )
    );
    assert!(
        ranked_casts.values().all(|casts| *casts > 0),
        "{ranked_casts:?}"
    );
    assert!(
        random_activations > 0,
        "random games activated no abilities"
    );
    assert!(
        companion_actions > 0,
        "no game put a companion into its owner's hand"
    );
}

/// A deck of `lands` and four copies of each of nine spells.
fn stress_deck(lands: &[(&str, u32)], spells: [&str; 9]) -> CustomDeckV1 {
    let count = |name: &str, count: u32| CustomCardCountV1 {
        name: name.to_owned(),
        count: count as _,
    };
    CustomDeckV1 {
        cards: lands
            .iter()
            .map(|(name, copies)| count(name, *copies))
            .chain(spells.iter().map(|name| count(name, 4)))
            .collect(),
        companion: None,
    }
}

/// Premodern wave P2a's statics (Stasis, Contamination, Root Maze, Forsaken
/// City, Future Sight, Black Vise) are rare in the snapshot decks, so two
/// decks full of them play each other through public sessions, both seats
/// with ranked and with seeded random actions.
#[test]
fn p2a_statics_decks_finish_and_replay_through_public_sessions() {
    let blue_green = stress_deck(
        &[("Island", 10), ("Forest", 10), ("Forsaken City", 4)],
        [
            "Future Sight",
            "Stasis",
            "Root Maze",
            "Upheaval",
            "Mana Short",
            "Genesis",
            "Wall of Blossoms",
            "Gaea's Blessing",
            "Veteran Explorer",
        ],
    );
    let white_black = stress_deck(
        &[("Swamp", 12), ("Plains", 8), ("Tainted Field", 4)],
        [
            "Contamination",
            "Black Vise",
            "Bane of the Living",
            "Nevinyrral's Disk",
            "Vindicate",
            "Gerrard's Verdict",
            "Eternal Dragon",
            "Innocent Blood",
            "Zombie Infestation",
        ],
    );
    for (pair, seed) in [
        ([blue_green.clone(), white_black.clone()], 0x5032_4100_u64),
        ([white_black, blue_green], 0x5032_4101),
    ] {
        for policy in [
            Policy::Ranked,
            Policy::Random(seed),
            Policy::Random(seed + 16),
            Policy::Random(seed + 32),
        ] {
            let game = play(pair.clone(), seed, policy);
            let replay = play(pair.clone(), seed, policy);
            assert_eq!(game.transcript_sha256, replay.transcript_sha256);
        }
    }
}

/// Wave S1h's overload and extraction cards are sideboard cards, which the
/// best-of-three harness draws too rarely to reach each branch, so a red
/// deck with both overload cards and a white-black deck with both
/// extractions play each other through public sessions, both seats with
/// ranked and with seeded random actions. The games choose an overload form
/// from the JSONL action list, show overloaded spells on the public stack
/// and reach both search looks in the replies.
#[test]
fn s1h_overload_and_extraction_decks_finish_and_replay_through_public_sessions() {
    let red = stress_deck(
        &[("Mountain", 24)],
        [
            "Vandalblast",
            "Electrickery",
            "Sol Ring",
            "Skred",
            "Mogg Fanatic",
            "Grim Lavamancer",
            "Galvanic Relay",
            "Chromatic Star",
            "Mishra's Bauble",
        ],
    );
    let white_black = stress_deck(
        &[("Swamp", 12), ("Plains", 12)],
        [
            "Surgical Extraction",
            "Extirpate",
            "Thraben Inspector",
            "Kor Skyfisher",
            "Prophetic Prism",
            "Tithing Blade",
            "Swords to Plowshares",
            "Thoughtseize",
            "Battle Screech",
        ],
    );
    let overload_cards = ["Vandalblast", "Electrickery"].map(|name| card_id_by_name(name).unwrap());
    let (mut chosen, mut overloaded, mut looks) = (0, 0, [0; 2]);
    for (pair, seed) in [
        ([red.clone(), white_black.clone()], 0x5331_4800_u64),
        ([white_black, red], 0x5331_4801),
    ] {
        for policy in [
            Policy::Ranked,
            Policy::Random(seed),
            Policy::Random(seed + 16),
            Policy::Random(seed + 32),
        ] {
            let game = play(pair.clone(), seed, policy);
            let replay = play(pair.clone(), seed, policy);
            assert_eq!(game.transcript_sha256, replay.transcript_sha256);
            // An overload card's second spell mode is its overload form.
            chosen += overload_cards
                .iter()
                .filter_map(|&card| game.spell_modes.get(&(card, 1)))
                .sum::<usize>();
            overloaded += game.overloaded_spells;
            for (total, count) in looks.iter_mut().zip(game.search_looks) {
                *total += count;
            }
        }
    }
    println!(
        "overload forms chosen: {chosen}, overloaded spells: {overloaded}, search looks: {looks:?}"
    );
    assert!(chosen > 0, "no overload form was chosen");
    assert!(
        overloaded > 0,
        "no overloaded spell reached the public stack"
    );
    assert!(
        looks.iter().all(|&count| count > 0),
        "search looks {looks:?}"
    );
}

/// Wave S1j's Archon of Absolution is a sideboard card, which the
/// best-of-three harness draws too rarely to tax many attacks, so a white
/// deck and a green-white deck with mana creatures, each with four Archons
/// in its mainboard, play each other through public sessions, both seats
/// with ranked and with seeded random actions. Either player's attacks are
/// taxed once the other has an Archon. The games include attackers whose
/// cost is paid, and reach attacker scan steps that offer only to leave a
/// creature out because its attack couldn't be paid for.
#[test]
fn s1j_archon_decks_finish_and_replay_through_public_sessions() {
    let white = stress_deck(
        &[("Plains", 24)],
        [
            "Archon of Absolution",
            "Thraben Inspector",
            "Kor Skyfisher",
            "Thalia, Guardian of Thraben",
            "Ocelot Pride",
            "Containment Priest",
            "Swords to Plowshares",
            "Battle Screech",
            "Sol Ring",
        ],
    );
    let green_white = stress_deck(
        &[("Forest", 14), ("Plains", 10)],
        [
            "Archon of Absolution",
            "Llanowar Elves",
            "Elvish Mystic",
            "Quirion Ranger",
            "Basking Rootwalla",
            "Wild Mongrel",
            "Rancor",
            "Sol Ring",
            "Mishra's Bauble",
        ],
    );
    let (mut taxed, mut unpaid) = (0, 0);
    for (pair, seed) in [
        ([white.clone(), green_white.clone()], 0x5331_4a00_u64),
        ([green_white, white], 0x5331_4a01),
    ] {
        for policy in [
            Policy::Ranked,
            Policy::Random(seed),
            Policy::Random(seed + 16),
            Policy::Random(seed + 32),
        ] {
            let game = play(pair.clone(), seed, policy);
            let replay = play(pair.clone(), seed, policy);
            assert_eq!(game.transcript_sha256, replay.transcript_sha256);
            taxed += game.taxed_inclusions;
            unpaid += game.unpaid_exclusions;
        }
    }
    println!("taxed inclusions: {taxed}, unpaid exclusions: {unpaid}");
    assert!(taxed > 0, "no attack was paid for against an Archon");
    assert!(
        unpaid > 0,
        "no attacker scan step left out a creature it couldn't pay for"
    );
}

/// Wave P3g's Circle of Protection: Red is a sideboard card, which the
/// best-of-three harness rarely draws with a red source to choose, so a
/// white-blue deck with four Circles, Arcane Laboratories and Akroma's
/// Vengeances in its mainboard and a black-red deck with Seal of Fire,
/// Infest and other red sources play each other through public sessions,
/// both seats with ranked and with seeded random actions. The games answer
/// a Circle's source choice from the JSONL action list.
#[test]
fn p3g_circle_decks_finish_and_replay_through_public_sessions() {
    let white_blue = stress_deck(
        &[("Plains", 14), ("Island", 10)],
        [
            "Circle of Protection: Red",
            "Arcane Laboratory",
            "Akroma's Vengeance",
            "Thraben Inspector",
            "Kor Skyfisher",
            "Swords to Plowshares",
            "Exalted Angel",
            "Opt",
            "Counterspell",
        ],
    );
    let black_red = stress_deck(
        &[("Mountain", 14), ("Swamp", 10)],
        [
            "Seal of Fire",
            "Infest",
            "Mogg Fanatic",
            "Grim Lavamancer",
            "Jackal Pup",
            "Lightning Bolt",
            "Siege-Gang Commander",
            "Twinshot Sniper",
            "Dark Ritual",
        ],
    );
    let mut choices = 0;
    for (pair, seed) in [
        ([white_blue.clone(), black_red.clone()], 0x5033_4700_u64),
        ([black_red, white_blue], 0x5033_4701),
    ] {
        for policy in [
            Policy::Ranked,
            Policy::Random(seed),
            Policy::Random(seed + 16),
            Policy::Random(seed + 32),
        ] {
            let game = play(pair.clone(), seed, policy);
            let replay = play(pair.clone(), seed, policy);
            assert_eq!(game.transcript_sha256, replay.transcript_sha256);
            choices += game.circle_choices;
        }
    }
    println!("Circle source choices: {choices}");
    assert!(
        choices > 0,
        "no Circle source was chosen from the action list"
    );
}
