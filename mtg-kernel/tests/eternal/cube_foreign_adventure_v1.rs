//! Cube fence: an Adventure spell cast by a player who does not own the card
//! (here from Ragavan's exiled top card, as Fallen Shinobi and the other
//! cast-from-exile grants also allow) grants that player, not the owner, the
//! later creature cast from exile (rule 715.4). The engine records that grant
//! in the Tinybones journal; the schema 5 session refused every such game as
//! Tinybones state ("Tinybones requires a distinct complete V72 consumer").
//! The session must keep playing, show the card face up in exile, and offer
//! the creature cast to the actual caster only.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::card_id_by_name;
use mtg_kernel::limited_session_v1::LimitedJsonlServerV1;
use serde_json::{json, Value};

const SCHEMA: u32 = 5;
const MAX_STEPS: usize = 8192;
const RAGAVAN: &str = "Ragavan, Nimble Pilferer";

fn id(name: &str) -> u64 {
    u64::from(card_id_by_name(name).unwrap_or_else(|| panic!("{name}")))
}

fn deck(rows: &[(&str, u32)]) -> Value {
    json!({"cards": rows.iter().map(|(name, count)| json!({"name": name, "count": count})).collect::<Vec<_>>()})
}

/// Every public card entry (an object with a `stable` reference) in `value`.
fn cards<'a>(value: &'a Value, out: &mut Vec<&'a Value>) {
    match value {
        Value::Object(map) => {
            if let Some(stable) = map.get("stable") {
                if stable.get("arena_id").is_some() {
                    out.push(stable);
                }
            }
            for child in map.values() {
                cards(child, out);
            }
        }
        Value::Array(items) => {
            for child in items {
                cards(child, out);
            }
        }
        _ => {}
    }
}

fn stable_of(decision: &Value, arena: u64) -> Vec<Value> {
    let mut out = Vec::new();
    cards(&decision["observation"], &mut out);
    out.into_iter()
        .filter(|stable| stable["arena_id"] == arena)
        .cloned()
        .collect()
}

fn controls(decision: &Value, seat: &str, card: u64) -> bool {
    let mut out = Vec::new();
    cards(&decision["observation"], &mut out);
    out.iter().any(|stable| {
        stable["zone"] == "Battlefield"
            && stable["controller"] == seat
            && stable["card_db_id"] == card
    })
}

/// What one game showed about the foreign Adventure.
#[derive(Debug, Default)]
struct Outcome {
    /// The terminal reason, if the game ended before the creature resolved.
    terminal: Option<String>,
    /// The Adventure card P0 cast from P1's library and sent on its Adventure.
    on_adventure: Option<u64>,
    /// P0 was offered the creature cast while the card was on its Adventure.
    offered_to_caster: bool,
    /// The creature is on the battlefield under P0, still owned by P1.
    resolved_under_caster: bool,
}

/// P0 (Ragavan) attacks and casts whatever Ragavan exiles, preferring the
/// Adventure half and P1's permanents as targets. P1 never blocks and casts
/// only Sol Ring, so its Adventure cards reach P0 only through Ragavan.
fn rank(action: &Value, adventure: u64, p0_has_ragavan: bool) -> u8 {
    let s = &action["semantic"];
    let p0 = s["actor"] == "p0";
    match s["action_kind"].as_str().unwrap() {
        "choose_london_mulligan" => u8::from(s["mulligan"] != false) * 50,
        "play_land" => 0,
        "cast_spell" if p0 && s["source"]["zone"] == "Exile" => 0,
        "cast_spell" if p0 && s["source"]["card_db_id"] == id(RAGAVAN) && p0_has_ragavan => 30,
        "cast_spell" if p0 => 1,
        "cast_spell" if s["source"]["card_db_id"] == id("Sol Ring") => 1,
        "cast_spell" => 20,
        "choose_spell_mode" if s["source"]["card_db_id"] == adventure => {
            if s["mode_index"] == 1 {
                0
            } else {
                6
            }
        }
        "choose_target" if s["target"]["object"]["owner"] == "p1" => 0,
        "choose_attacker_inclusion" => u8::from(s["include"] != true) * 6,
        "choose_blocker_inclusion" => u8::from(s["include"] == true) * 20,
        "pass" => 10,
        "activate_mana_ability" => 11,
        "activate_ability" => 12,
        _ => 5,
    }
}

fn play(adventure_name: &str, land: &str, seed: u64) -> Outcome {
    let adventure = id(adventure_name);
    let pilferer = deck(&[("Mountain", 9), ("Island", 8), (RAGAVAN, 23)]);
    let owner = deck(&[(land, 17), (adventure_name, 15), ("Sol Ring", 8)]);
    let mut server = LimitedJsonlServerV1::new_with_public_shroud_v1();
    let mut request = json!({"schema_version": SCHEMA, "request_type": "reset",
        "request_id": "reset", "decks": [pilferer, owner],
        "episode_id": 11, "env_seed": seed,
        "max_physical_decisions": 65536, "max_policy_steps": MAX_STEPS})
    .to_string();
    let mut outcome = Outcome::default();
    // The P1-owned Adventure card P0 has cast from exile and its stack
    // generation, until it is seen again in exile (on its Adventure) or on
    // the battlefield (the creature half was cast through Ragavan instead).
    let mut cast_from_exile: Option<(u64, u64)> = None;
    for index in 0..MAX_STEPS {
        let raw = server.handle_line(&request);
        let reply: Value = serde_json::from_str(&raw).unwrap();
        assert_ne!(reply["response_type"], "error", "step {index}: {reply}");
        if reply["response_type"] == "terminal" {
            outcome.terminal = Some(
                reply["terminal"]["terminal_reason"]
                    .as_str()
                    .unwrap()
                    .to_owned(),
            );
            return outcome;
        }
        let decision = &reply["decision"];
        let legal = decision["legal_actions"].as_array().unwrap();
        let actor = legal[0]["semantic"]["actor"].clone();

        if let Some((arena, zcc)) = cast_from_exile {
            for stable in stable_of(decision, arena) {
                if stable["zone_change_count"].as_u64().unwrap() > zcc {
                    if stable["zone"] == "Exile" {
                        assert_eq!(stable["card_db_id"], adventure, "face up in exile");
                        assert_eq!(stable["owner"], "p1");
                        outcome.on_adventure = Some(arena);
                    }
                    cast_from_exile = None;
                }
            }
        }
        if let Some(arena) = outcome.on_adventure {
            let seen = stable_of(decision, arena);
            if seen.iter().any(|s| {
                s["zone"] == "Battlefield" && s["controller"] == "p0" && s["owner"] == "p1"
            }) {
                outcome.resolved_under_caster = true;
                return outcome;
            }
            let on_adventure = seen.iter().any(|s| s["zone"] == "Exile");
            for action in legal {
                let s = &action["semantic"];
                if on_adventure
                    && s["action_kind"] == "cast_spell"
                    && s["source"]["arena_id"] == arena
                {
                    assert_eq!(s["actor"], "p0", "only the actual caster may cast it: {s}");
                    assert_eq!(s["source"]["zone"], "Exile");
                    outcome.offered_to_caster = true;
                }
            }
        }

        let p0_has_ragavan = actor == "p0" && controls(decision, "p0", id(RAGAVAN));
        let action = legal
            .iter()
            .min_by_key(|action| rank(action, adventure, p0_has_ragavan))
            .unwrap();
        let s = &action["semantic"];
        if outcome.on_adventure.is_none()
            && s["action_kind"] == "cast_spell"
            && s["actor"] == "p0"
            && s["source"]["zone"] == "Exile"
            && s["source"]["card_db_id"] == adventure
        {
            cast_from_exile = Some((
                s["source"]["arena_id"].as_u64().unwrap(),
                s["source"]["zone_change_count"].as_u64().unwrap(),
            ));
        }
        request = json!({"schema_version": SCHEMA, "request_type": "step",
            "request_id": format!("step-{index}"), "episode_id": decision["episode_id"],
            "expected_step": decision["step"], "selected_index": action["selected_index"],
            "selected_action_id": action["stable_id"]})
        .to_string();
    }
    panic!("{adventure_name}: game {seed} did not end");
}

/// Plays seeds until one game sends a P1-owned `adventure` on its Adventure
/// under P0, then requires the full cycle: no refusal, the creature offered to
/// P0 only, and the creature resolving under P0.
fn foreign_adventure_cycle(adventure: &str, land: &str) {
    for seed in 0x4643_0000_u64..0x4643_0000 + 40 {
        let outcome = play(adventure, land, seed);
        if let Some(reason) = &outcome.terminal {
            assert!(
                !reason.starts_with("fail_closed"),
                "{adventure} seed {seed:#x}: {reason}"
            );
        }
        if outcome.on_adventure.is_none() {
            continue;
        }
        assert_eq!(
            outcome.terminal, None,
            "{adventure} seed {seed:#x}: {outcome:?}"
        );
        assert!(outcome.offered_to_caster, "{adventure} seed {seed:#x}");
        assert!(outcome.resolved_under_caster, "{adventure} seed {seed:#x}");
        return;
    }
    panic!("{adventure}: no game reached a foreign Adventure");
}

#[test]
fn battle_display_cast_from_ragavans_exile_leaves_the_knight_castable_by_its_caster() {
    foreign_adventure_cycle("Embereth Shieldbreaker", "Mountain");
}

#[test]
fn petty_theft_cast_from_ragavans_exile_leaves_the_borrower_castable_by_its_caster() {
    foreign_adventure_cycle("Brazen Borrower", "Island");
}
