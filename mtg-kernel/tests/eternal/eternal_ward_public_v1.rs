//! Limited schema 5 (the public shroud contract) follows Ward payments: its
//! policy observation reports each pending or queued Ward payment
//! (`ward_payments`) with the Ward source, the exact targeting stack item,
//! the payer and the typed cost. Schema 4 keeps refusing the states the
//! legacy observation cannot represent: a life-payment Ward (Sedgemoor
//! Witch) and a generic Ward whose source two live stack items target
//! (Kappa Cannoneer).
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::CARD_DEFS;
use mtg_kernel::limited_session_v1::LimitedJsonlServerV1;
use serde_json::{json, Value};

const MAX_STEPS: usize = 3000;

fn deck(land: &str, card: &str) -> Value {
    json!({"cards": [{"name": land, "count": 17}, {"name": card, "count": 23}]})
}

fn name_of(card: &Value) -> Option<&'static str> {
    card["card_db_id"]
        .as_u64()
        .map(|id| CARD_DEFS[id as usize].name)
}

fn other(seat: &str) -> &'static str {
    if seat == "p0" {
        "p1"
    } else {
        "p0"
    }
}

/// Whether `seat` controls a battlefield permanent named `name`.
fn controls(decision: &Value, seat: &str, name: &str) -> bool {
    decision["observation"]["projection"]["battlefield"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|cards| cards.as_array().into_iter().flatten())
        .any(|card| card["stable"]["controller"] == seat && name_of(&card["stable"]) == Some(name))
}

fn stack(decision: &Value) -> &Vec<Value> {
    decision["observation"]["projection"]["stack"]
        .as_array()
        .unwrap()
}

/// The public stack rows whose targets include a card named `name`.
fn stack_rows_targeting(decision: &Value, name: &str) -> Vec<u64> {
    stack(decision)
        .iter()
        .filter(|row| {
            row["targets"].as_array().unwrap().iter().any(|target| {
                target["target_kind"] == "object" && name_of(&target["object"]) == Some(name)
            })
        })
        .map(|row| row["stack_index"].as_u64().unwrap())
        .collect()
}

/// A scripted game: the warded deck plays its lands and casts its card; the
/// burn deck holds its spells until the warded card is on the opposing
/// battlefield, then casts them at it (as often as it can), and answers
/// every Ward payment with `pay`.
struct Script {
    warded: &'static str,
    burn: &'static str,
    pay: bool,
}

impl Script {
    fn rank(&self, decision: &Value, action: &Value) -> u8 {
        let s = &action["semantic"];
        let actor = s["actor"].as_str().unwrap_or("p0");
        match s["action_kind"].as_str().unwrap() {
            "choose_london_mulligan" if s["mulligan"] == false => 0,
            "choose_effect_boolean" if s["value"] == self.pay => 0,
            "choose_effect_boolean" => 20,
            "choose_target" | "choose_effect_target"
                if s["target"]["target_kind"] == "object"
                    && name_of(&s["target"]["object"]) == Some(self.warded) =>
            {
                0
            }
            "choose_target" | "choose_effect_target" => 6,
            "play_land" => 0,
            "cast_spell" if name_of(&s["source"]) == Some(self.burn) => {
                if controls(decision, other(actor), self.warded) {
                    1
                } else {
                    30
                }
            }
            "cast_spell" => 1,
            "choose_attacker_inclusion" if s["include"] == false => 2,
            "pass" => 10,
            "activate_mana_ability" | "activate_ability" => 11,
            _ => 5,
        }
    }
}

struct Outcome {
    /// Every decision, in order.
    decisions: Vec<Value>,
    /// The action taken at each decision.
    actions: Vec<Value>,
    /// The terminal reason, if the game ended within the step bound.
    terminal_reason: Option<String>,
}

fn play(schema: u32, decks: [Value; 2], script: &Script) -> Outcome {
    let mut server = match schema {
        4 => LimitedJsonlServerV1::new_with_london_mulligans_v1(),
        5 => LimitedJsonlServerV1::new_with_public_shroud_v1(),
        _ => unreachable!(),
    };
    let mut request = json!({"schema_version": schema, "request_type": "reset",
        "request_id": "reset", "decks": decks,
        "episode_id": 11, "env_seed": 0x5741_5244_u64,
        "max_physical_decisions": 65536, "max_policy_steps": MAX_STEPS})
    .to_string();
    let mut outcome = Outcome {
        decisions: Vec::new(),
        actions: Vec::new(),
        terminal_reason: None,
    };
    for index in 0..MAX_STEPS {
        let reply: Value = serde_json::from_str(&server.handle_line(&request)).unwrap();
        assert_ne!(reply["response_type"], "error", "step {index}: {reply}");
        if reply["response_type"] == "terminal" {
            outcome.terminal_reason = reply["terminal"]["terminal_reason"]
                .as_str()
                .map(str::to_owned);
            return outcome;
        }
        let decision = reply["decision"].clone();
        let action = decision["legal_actions"]
            .as_array()
            .unwrap()
            .iter()
            .min_by_key(|action| script.rank(&decision, action))
            .unwrap()
            .clone();
        request = json!({"schema_version": schema, "request_type": "step",
            "request_id": format!("step-{index}"), "episode_id": decision["episode_id"],
            "expected_step": decision["step"], "selected_index": action["selected_index"],
            "selected_action_id": action["stable_id"]})
        .to_string();
        outcome.decisions.push(decision);
        outcome.actions.push(action);
    }
    outcome
}

fn assert_followed(outcome: &Outcome) {
    if let Some(reason) = &outcome.terminal_reason {
        assert!(!reason.starts_with("fail_closed"), "{reason}");
    }
}

fn ward_payments(decision: &Value) -> &Value {
    &decision["observation"]["projection"]["ward_payments"]
}

/// Checks one Ward payment record against the public stack: its source is
/// the warded card, its targeting stack item targets that card and is
/// controlled by the payer, and its cost is `cost`.
fn check_payment(decision: &Value, payment: &Value, warded: &str, cost: &Value) {
    assert_eq!(name_of(&payment["ward_source"]), Some(warded), "{payment}");
    let index = payment["targeting_stack_index"].as_u64().unwrap();
    let row = stack(decision)
        .iter()
        .find(|row| row["stack_index"] == index)
        .unwrap_or_else(|| panic!("no stack row {index}: {payment}"));
    assert!(stack_rows_targeting(decision, warded).contains(&index));
    assert_eq!(row["controller"], payment["payer"]);
    assert_ne!(payment["payer"], payment["ward_source"]["controller"]);
    assert_eq!(&payment["cost"], cost);
}

#[test]
fn schema_5_follows_a_life_payment_ward_that_schema_4_refuses() {
    let decks = || {
        [
            deck("Swamp", "Sedgemoor Witch"),
            deck("Mountain", "Lightning Bolt"),
        ]
    };
    let life = json!({"kind": "life", "amount": 3});
    for pay in [true, false] {
        let script = Script {
            warded: "Sedgemoor Witch",
            burn: "Lightning Bolt",
            pay,
        };
        let public = play(5, decks(), &script);
        assert_followed(&public);
        let mut queued_seen = false;
        let mut answered = 0;
        for (index, decision) in public.decisions.iter().enumerate() {
            let payments = ward_payments(decision);
            for queued in payments["queued"].as_array().into_iter().flatten() {
                let trigger = queued["trigger_stack_index"].as_u64().unwrap();
                let row = stack(decision)
                    .iter()
                    .find(|row| row["stack_index"] == trigger)
                    .unwrap();
                assert_eq!(row["stack_item_kind"], "triggered_ability");
                assert_eq!(row["source"], queued["payment"]["ward_source"]);
                check_payment(decision, &queued["payment"], "Sedgemoor Witch", &life);
                queued_seen = true;
            }
            let action = &public.actions[index];
            if action["semantic"]["action_kind"] != "choose_effect_boolean" {
                continue;
            }
            // The payer chooses between paying and declining.
            let pending = &payments["pending"];
            check_payment(decision, pending, "Sedgemoor Witch", &life);
            let values: Vec<&Value> = decision["legal_actions"]
                .as_array()
                .unwrap()
                .iter()
                .map(|action| &action["semantic"]["value"])
                .collect();
            assert_eq!(values, [&json!(false), &json!(true)]);
            let payer = usize::from(pending["payer"] == "p1");
            let targeter = pending["targeting_stack_index"].as_u64().unwrap();
            let life_before = decision["observation"]["projection"]["life_totals"][payer]
                .as_i64()
                .unwrap();
            let after = &public.decisions[index + 1];
            let life_after = after["observation"]["projection"]["life_totals"][payer]
                .as_i64()
                .unwrap();
            let graveyard = &after["observation"]["projection"]["graveyards"][payer];
            let bolts_before = decision["observation"]["projection"]["graveyards"][payer]
                .as_array()
                .unwrap()
                .len();
            if pay {
                // Paying 3 life lets the Bolt resolve.
                assert_eq!(life_after, life_before - 3);
            } else {
                // Declining counters the Bolt: no life is paid, and the
                // countered Bolt goes to its owner's graveyard.
                assert_eq!(life_after, life_before);
                assert!(graveyard.as_array().unwrap().len() > bolts_before);
                assert!(!stack_rows_targeting(after, "Sedgemoor Witch").contains(&targeter));
            }
            answered += 1;
        }
        assert!(queued_seen, "pay={pay}: no queued life Ward");
        assert!(answered > 0, "pay={pay}: no life Ward answered");
        // No Ward record outside Ward states.
        for decision in &public.decisions {
            if stack_rows_targeting(decision, "Sedgemoor Witch").is_empty() {
                assert!(
                    ward_payments(decision).is_null(),
                    "{}",
                    ward_payments(decision)
                );
            }
        }

        let legacy = play(4, decks(), &script);
        assert_eq!(
            legacy.terminal_reason.as_deref(),
            Some("fail_closed:observation:life-payment Ward requires its typed observation successor")
        );
        for decision in &legacy.decisions {
            assert!(!decision.to_string().contains("ward_payments"));
        }
    }
}

#[test]
fn schema_5_binds_each_generic_ward_trigger_when_two_spells_target_its_source() {
    let decks = || {
        [
            deck("Island", "Kappa Cannoneer"),
            deck("Mountain", "Lightning Bolt"),
        ]
    };
    let script = Script {
        warded: "Kappa Cannoneer",
        burn: "Lightning Bolt",
        pay: false,
    };
    let generic = json!({"kind": "generic", "amount": 4});
    let public = play(5, decks(), &script);
    assert_followed(&public);
    let mut doubly_targeted = 0;
    for decision in &public.decisions {
        let targeters = stack_rows_targeting(decision, "Kappa Cannoneer");
        let queued = ward_payments(decision)["queued"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        for record in &queued {
            check_payment(decision, &record["payment"], "Kappa Cannoneer", &generic);
            // Each trigger sits directly above the spell it is bound to.
            assert_eq!(
                record["trigger_stack_index"].as_u64().unwrap(),
                record["payment"]["targeting_stack_index"].as_u64().unwrap() + 1
            );
        }
        if targeters.len() >= 2 && queued.len() >= 2 {
            let mut bound: Vec<u64> = queued
                .iter()
                .map(|record| record["payment"]["targeting_stack_index"].as_u64().unwrap())
                .collect();
            bound.sort_unstable();
            bound.dedup();
            assert_eq!(bound.len(), queued.len(), "one trigger per targeter");
            doubly_targeted += 1;
        }
    }
    assert!(doubly_targeted > 0, "no position with two targeting spells");

    let legacy = play(4, decks(), &script);
    assert_eq!(
        legacy.terminal_reason.as_deref(),
        Some(
            "fail_closed:observation:legacy observation cannot identify the Ward-bound stack item unambiguously; the Ward observation successor is required"
        )
    );
}
