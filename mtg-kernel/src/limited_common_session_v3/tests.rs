//! Source-only V3 RPC cases. The wire never accepts privileged fixture state.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use super::*;
use crate::ids::{ObjectId, PlayerId};
use crate::policy_observation_v6::tests::{advance_to_next_main_for, put};
use crate::state::{GameState, Zone};
use serde_json::{json, Value};

struct Wire {
    server: LimitedCommonJsonlServerV3,
    serial: u64,
    last: Value,
}
impl Wire {
    fn new() -> Self {
        Self {
            server: LimitedCommonJsonlServerV3::new(),
            serial: 0,
            last: Value::Null,
        }
    }
    fn fixture(state: GameState) -> Self {
        let mut wire = Self::new();
        wire.server.active = Some(Active {
            core: FastActorSessionV1::from_ravenous_v51_fixture_state(state),
            owner: Arc::new(()),
        });
        wire.server.generation = 1;
        wire.send(json!({"request_type": "current"}));
        wire
    }
    fn send(&mut self, mut request: Value) -> Value {
        self.serial += 1;
        let row = request.as_object_mut().unwrap();
        row.insert("request_id".into(), json!(format!("wire{}", self.serial)));
        row.insert("schema_version".into(), json!(3));
        let line = serde_json::to_string(&request).unwrap();
        self.last = serde_json::from_str(&self.server.handle_line(&line)).unwrap();
        assert_eq!(self.last["protocol"], PROTOCOL);
        assert_eq!(self.last["schema_version"], 3);
        self.last.clone()
    }
    fn menu(&self) -> &Value {
        assert_eq!(
            self.last["state"]["state_type"], "decision",
            "{}",
            self.last
        );
        &self.last["state"]["decision"]
    }
    fn request(&self, choose: impl Fn(&str) -> bool) -> Value {
        let menu = self.menu();
        let action = menu["actions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| choose(row["label"].as_str().unwrap()))
            .unwrap_or_else(|| panic!("actual V3 action absent: {}", menu["actions"]));
        json!({"request_type": "step", "episode_id": self.last["state"]["episode_id"],
            "authority_generation": self.last["state"]["authority_generation"],
            "actor": menu["actor"], "prompt_seq": menu["prompt_seq"], "action_index": action["action_index"]})
    }
    fn step(&mut self, choose: impl Fn(&str) -> bool) {
        let answer = self.request(choose);
        let reply = self.send(answer);
        assert_eq!(reply["response_type"], "state", "{reply}");
        assert!(reply["receipt"].is_object());
    }
    fn mana(&mut self, land: &str, color: &str) {
        self.step(|label| label.contains(land) && label.contains(color));
    }
    fn state(&self) -> &GameState {
        self.server
            .active
            .as_ref()
            .unwrap()
            .core
            .ravenous_fixture_state()
    }
    fn bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self.state()).unwrap()
    }
    fn snapshot(&mut self) -> u64 {
        self.send(json!({"request_type": "snapshot"}))["snapshot_id"]
            .as_u64()
            .unwrap()
    }
    fn restore(&mut self, id: u64) {
        let reply = self.send(json!({"request_type": "restore", "snapshot_id": id}));
        assert_eq!(reply["response_type"], "state", "{reply}");
    }
    fn reject_unchanged(&mut self, request: Value, code: &str) {
        let before = self.bytes();
        let reply = self.send(request);
        assert_eq!(reply["response_type"], "error");
        assert_eq!(reply["code"], code);
        assert_eq!(self.bytes(), before);
        self.send(json!({"request_type": "current"}));
    }
    fn drain(&mut self) {
        for _ in 0..64 {
            let state = self.state();
            if state.stack.is_empty()
                && state.engine.pending_cast.is_none()
                && state.engine.pending_triggers.is_empty()
                && state.engine.pending_effect.is_none()
            {
                return;
            }
            self.step(|label| label == "Pass priority");
        }
        panic!("actual V3 paid operation did not finish");
    }
}
fn fixture(actor: PlayerId) -> (GameState, ObjectId) {
    let mut state = crate::extra_turn_v1::tests::ready(actor);
    for player in [actor, actor.opponent()] {
        for _ in 0..12 {
            put(&mut state, player, "Forest", Zone::Library);
        }
    }
    put(&mut state, actor, "Island", Zone::Hand);
    put(&mut state, actor.opponent(), "Lightning Bolt", Zone::Hand);
    let amulet = put(&mut state, actor, "Ravenous Amulet", Zone::Hand);
    put(&mut state, actor, "Fake Your Own Death", Zone::Hand);
    for _ in 0..2 {
        put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
    }
    for _ in 0..7 {
        put(&mut state, actor, "Forest", Zone::Battlefield);
    }
    for _ in 0..2 {
        put(&mut state, actor, "Swamp", Zone::Battlefield);
    }
    (state, amulet)
}
fn cast_amulet(wire: &mut Wire) {
    for _ in 0..2 {
        wire.mana("Forest", "{G}");
    }
    wire.step(|label| label.starts_with("Begin casting Ravenous Amulet"));
    wire.drain();
}
fn draw_cost(wire: &mut Wire) {
    wire.mana("Forest", "{G}");
    wire.step(|label| {
        label.starts_with("Activate Ravenous Amulet") && label.contains("draw one card, then")
    });
}
fn safe(reply: &Value) {
    let text = reply.to_string();
    for forbidden in [
        "arena_id",
        "card_db_id",
        "zone_change_count",
        "original_world",
        "output_world",
        "event_history",
        "engine_indexes",
        "payment_history_start",
        "tap_history_index",
        "inline_effect",
        "source_contract",
    ] {
        assert!(!text.contains(forbidden), "private wire field {forbidden}");
    }
    let roster = reply["state"]["decision"]["state"]["public"]["battlefield"]
        .as_array()
        .unwrap();
    assert_eq!(roster.len(), 2);
    for row in roster.iter().flat_map(|seat| seat.as_array().unwrap()) {
        assert!(row["card"]["stable"]["handle"].is_string());
        for field in ["soul", "plus_one_counters", "marked_damage"] {
            assert!(row[field].is_i64() || row[field].is_u64());
        }
    }
}

#[test]
fn actual_v3_wire_paid_cast_draw_soul_loss_restore_stale_and_hidden_values_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (state, amulet) = fixture(actor);
        let mut wire = Wire::fixture(state);
        cast_amulet(&mut wire);
        assert_eq!(wire.state().objects.get(amulet).zone, Zone::Battlefield);
        assert_eq!(
            wire.state().players[actor.index()].mana_pool[crate::mana::ManaColor::G.pool_index()],
            0
        );
        safe(&wire.last);
        draw_cost(&mut wire);
        let cost = wire.snapshot();
        let answer =
            wire.request(|label| label.contains("activation cost: sacrifice Llanowar Elves"));
        wire.step(|label| label.contains("activation cost: sacrifice Llanowar Elves"));
        let paid = wire.snapshot();
        assert_eq!(
            wire.menu()["state"]["extensions"]["ravenous"]["activations"][0]["outcome"],
            "Published"
        );
        let hand = wire.state().players[actor.index()].hand.len();
        wire.drain();
        assert_eq!(wire.state().players[actor.index()].hand.len(), hand + 1);
        assert_eq!(wire.state().objects.get(amulet).counters.soul, 1);
        let finished = wire.menu()["state"].clone();
        let finished_actions = wire.menu()["actions"].clone();
        assert_eq!(
            finished["extensions"]["ravenous"]["activations"][0]["outcome"]["Draw"]["placement"]
                ["after"],
            1
        );
        safe(&wire.last);
        let bytes = wire.bytes();
        wire.restore(paid);
        wire.drain();
        assert_eq!(wire.menu()["state"], finished);
        assert_eq!(wire.menu()["actions"], finished_actions);
        assert_eq!(wire.bytes(), bytes);
        wire.restore(cost);
        wire.reject_unchanged(answer, "stale_decision");
        let mut wrong_actor =
            wire.request(|label| label.contains("activation cost: sacrifice Llanowar Elves"));
        wrong_actor["actor"] = json!(PlayerSeatV1::from(actor.opponent()));
        wire.reject_unchanged(wrong_actor, "stale_decision");
        wire.step(|label| label.contains("activation cost: sacrifice Llanowar Elves"));
        wire.drain();
        assert_eq!(wire.menu()["state"], finished);
        assert_eq!(wire.bytes(), bytes);

        // Explicit numeric operands on the genuinely paid public artifact.
        // They do not create payment owners or claim another printed producer.
        let mut wide = wire.state().clone();
        wide.objects.get_mut(amulet).counters.soul = i32::from(i16::MAX) + 29;
        wide.objects.get_mut(amulet).counters.plus1_plus1 = i32::from(i16::MAX) + 17;
        wide.objects.get_mut(amulet).damage = u32::from(u16::MAX) + 23;
        wide.reveal_library_top(actor, actor, 1);
        let mut changed = wide.clone();
        let hidden = changed.players[actor.opponent().index()].hand[0];
        changed.objects.get_mut(hidden).card_def =
            crate::card_def::card_id_by_name("Mountain").unwrap();
        for seat in 0..2 {
            let hidden = *changed.players[seat].library.last().unwrap();
            changed.objects.get_mut(hidden).card_def =
                crate::card_def::card_id_by_name("Lightning Bolt").unwrap();
        }
        let mut wide_wire = Wire::fixture(wide.clone());
        let changed_wire = Wire::fixture(changed);
        assert_eq!(wide_wire.last, changed_wire.last);
        assert!(
            !wide_wire.menu()["state"]["known_library_cards"][actor.index()]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let card = wide_wire.menu()["state"]["public"]["battlefield"][actor.index()]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["card"]["stable"]["name"] == "Ravenous Amulet")
            .unwrap();
        assert_eq!(card["soul"], i32::from(i16::MAX) + 29);
        assert_eq!(card["plus_one_counters"], i32::from(i16::MAX) + 17);
        assert_eq!(card["marked_damage"], u32::from(u16::MAX) + 23);
        safe(&wide_wire.last);
        let before = wide_wire.bytes();
        let id = wide_wire.snapshot();
        wide_wire.restore(id);
        assert_eq!(wide_wire.bytes(), before);

        // Ordinary engine turns supply the printed untap; retain all genuinely
        // paid records while a fresh V3 owner publishes the resulting state.
        let mut next = wire.state().clone();
        advance_to_next_main_for(&mut next, actor);
        let mut loss = Wire::fixture(next);
        for _ in 0..4 {
            loss.mana("Forest", "{G}");
        }
        let life = loss.state().players[actor.opponent().index()].life;
        loss.step(|label| {
            label.starts_with("Activate Ravenous Amulet")
                && label.contains("each opponent loses life")
        });
        let saved = loss.snapshot();
        loss.drain();
        assert_eq!(
            loss.state().players[actor.opponent().index()].life,
            life - 1
        );
        let completed = loss.menu()["state"].clone();
        let activation = completed["extensions"]["ravenous"]["activations"]
            .as_array()
            .unwrap()
            .last()
            .unwrap();
        assert_eq!(activation["self_sacrifice_soul_count"], 1);
        assert_eq!(activation["outcome"]["Loss"]["amount"], 1);
        assert!(activation["current_original_source"].is_null());
        assert!(activation["sacrificed"].is_string());
        safe(&loss.last);
        let bytes = loss.bytes();
        loss.restore(saved);
        loss.drain();
        assert_eq!(loss.menu()["state"], completed);
        assert_eq!(loss.bytes(), bytes);
    }
}

#[test]
fn v3_wire_keeps_paid_fake_return_and_panda_facts_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (state, amulet) = fixture(actor);
        let mut wire = Wire::fixture(state);
        for _ in 0..2 {
            wire.mana("Swamp", "{B}");
        }
        wire.step(|label| label.starts_with("Begin casting Fake Your Own Death"));
        wire.step(|label| {
            label.starts_with("Fake Your Own Death [")
                && label.contains(": target Llanowar Elves [")
        });
        wire.drain();
        assert_eq!(
            wire.menu()["state"]["extensions"]["fake"]["grants"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        cast_amulet(&mut wire);
        draw_cost(&mut wire);
        let target = wire.menu()["state"]["extensions"]["fake"]["grants"][0]["original_recipient"]
            ["handle"]
            .as_str()
            .unwrap()
            .to_owned();
        wire.step(|label| {
            label.contains("activation cost: sacrifice Llanowar Elves")
                && label.contains(&format!("[{target}]"))
        });
        let saved = wire.snapshot();
        wire.drain();
        let completed = wire.menu()["state"].clone();
        let death = &completed["extensions"]["fake"]["deaths"][0];
        assert!(death["returned_incarnation"].is_string());
        assert_eq!(death["treasures"].as_array().unwrap().len(), 1);
        assert_eq!(wire.state().objects.get(amulet).counters.soul, 1);
        wire.restore(saved);
        wire.drain();
        assert_eq!(wire.menu()["state"], completed);
        safe(&wire.last);

        // Physical cards and mana are source fixture operands; actual V3 cast,
        // payment and trigger producers own all resulting Angel/Panda records.
        let mut state = crate::rl_session::panda_v47::tests::ordinary(actor);
        let angel = put(&mut state, actor, "Dazzling Angel", Zone::Hand);
        let panda = put(&mut state, actor, "Fiendish Panda", Zone::Hand);
        state.players[actor.index()].mana_pool[crate::mana::ManaColor::W.pool_index()] = 2;
        state.players[actor.index()].mana_pool[crate::mana::ManaColor::B.pool_index()] = 1;
        state.players[actor.index()].mana_pool[crate::mana::ManaColor::C.pool_index()] = 4;
        let mut wire = Wire::fixture(state);
        wire.step(|label| label.starts_with("Begin casting Dazzling Angel"));
        wire.drain();
        assert_eq!(wire.state().objects.get(angel).zone, Zone::Battlefield);
        wire.step(|label| label.starts_with("Cast Fiendish Panda ["));
        wire.drain();
        assert_eq!(wire.state().objects.get(panda).zone, Zone::Battlefield);
        assert_eq!(wire.state().objects.get(panda).counters.plus1_plus1, 1);
        let facts = &wire.menu()["state"]["extensions"]["panda"]["occurrences"];
        assert_eq!(facts[0]["life_gained"], 1);
        assert_eq!(facts[0]["actual_counters_added"], 1);
        safe(&wire.last);
    }
}

#[test]
fn v3_strict_schema_full_only_reset_london_and_no_fixture_wire_injection() {
    for actor in [PlayerSeatV1::P0, PlayerSeatV1::P1] {
        let mut wire = Wire::new();
        let reset = json!({"request_type": "reset", "expected_card_db_hash": KERNEL_CARDDB_HASH,
            "episode_id": 7, "env_seed": 912, "starting_player": actor,
            "decks": [{"cards": [{"name": "Forest", "count": 40}]}, {"cards": [{"name": "Mountain", "count": 41}]}],
            "max_physical_decisions": 10000, "max_policy_steps": 10000});
        wire.send(reset.clone());
        assert_eq!(wire.menu()["actor"], json!(actor));
        assert_eq!(
            wire.menu()["state"]["own_hand"].as_array().unwrap().len(),
            7
        );
        assert_eq!(wire.menu()["london_mulligans"]["counts"], json!([0, 0]));
        assert_eq!(
            wire.menu()["state"]["public"]["battlefield"],
            json!([[], []])
        );
        let hidden = if actor == PlayerSeatV1::P0 {
            "Mountain"
        } else {
            "Forest"
        };
        assert!(!wire.last.to_string().contains(hidden));
        let snapshot = wire.snapshot();
        let stale = wire.request(|label| label.starts_with("Keep this hand"));
        wire.step(|label| label.starts_with("Mulligan:"));
        wire.step(|label| label.starts_with("Keep this hand"));
        wire.step(|label| label.starts_with("Put "));
        assert_eq!(
            wire.menu()["state"]["own_hand"].as_array().unwrap().len(),
            6
        );
        wire.step(|label| label.starts_with("Keep this hand"));
        assert!(wire.menu()["actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["label"] == "Pass priority"));
        assert!(wire.state().engine.combat.foundations_v1.is_some());
        wire.restore(snapshot);
        wire.reject_unchanged(stale, "stale_decision");
        wire.reject_unchanged(
            json!({"request_type": "current", "state_spec": {}}),
            "invalid_request",
        );
        assert!(wire
            .server
            .handle_line(r#"{"request_type":"current","schema_version":2,"request_id":"old"}"#)
            .contains("invalid_header"));
        let mut predecessor = crate::limited_common_session_v2::LimitedCommonJsonlServerV2::new();
        assert!(predecessor
            .handle_line(r#"{"request_type":"current","schema_version":3,"request_id":"new"}"#)
            .contains("invalid_header"));
        assert!(wire.server.handle_line(r#"{"request_type":"current","schema_version":3,"schema_version":3,"request_id":"duplicate"}"#).contains("invalid_request"));
    }
}

#[test]
fn v3_wire_actual_combat_scan_range_exact_safe_handles_and_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = crate::extra_turn_v1::tests::ready(actor);
        for player in [actor, actor.opponent()] {
            for _ in 0..8 {
                put(&mut state, player, "Forest", Zone::Library);
            }
        }
        crate::combat_damage_v1::enable_foundations_combat_v1(&mut state).unwrap();
        let attacker = put(&mut state, actor, "Spinewoods Paladin", Zone::Battlefield);
        let blocker = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        state.objects.get_mut(attacker).summoning_sick = false;
        state.step = crate::state::Step::DeclareAttackers;
        state.priority_player = actor;
        state.engine.priority_passes = [false, false];
        assert!(matches!(
            crate::engine::advance_until_decision(&mut state),
            crate::engine::Decision::DeclareAttackers { .. }
        ));
        let mut wire = Wire::fixture(state);
        wire.step(|label| label.starts_with("Attack with"));
        let mut restored_range = false;
        let mut saw_blocker = false;
        let mut finished = false;
        for _ in 0..64 {
            let actions = wire.menu()["actions"].as_array().unwrap();
            let range = actions
                .iter()
                .any(|row| row["label"].as_str().unwrap().contains(" damage to "));
            if range && !restored_range {
                assert!(wire.menu()["foundations_combat"].is_object());
                safe(&wire.last);
                let saved = wire.snapshot();
                let stale = wire.request(|label| label.contains(" damage to "));
                wire.step(|label| label.contains(" damage to "));
                wire.restore(saved);
                wire.reject_unchanged(stale, "stale_decision");
                restored_range = true;
            }
            let actions = wire.menu()["actions"].as_array().unwrap();
            let label = if let Some(row) = actions.iter().find(|row| {
                let label = row["label"].as_str().unwrap();
                label.starts_with("Assign ") && label.contains(" to block ")
            }) {
                saw_blocker = true;
                row["label"].as_str().unwrap().to_owned()
            } else if let Some(row) = actions
                .iter()
                .find(|row| row["label"].as_str().unwrap().contains(" damage to "))
            {
                row["label"].as_str().unwrap().to_owned()
            } else {
                "Pass priority".into()
            };
            wire.step(|offered| offered == label);
            if wire.state().engine.event_history.iter().any(|event| {
                matches!(event,
                crate::event::CommittedEvent::Damage { source, .. } if *source == attacker)
            }) {
                assert!(saw_blocker && restored_range);
                assert!(
                    wire.state().objects.get(blocker).damage > 0
                        || wire.state().objects.get(blocker).zone == Zone::Graveyard
                );
                finished = true;
                break;
            }
        }
        assert!(finished, "actual V3 Foundations combat did not finish");
    }
}
#[test]
fn public_v3_full_fake_reference_reset_owned_restore_and_continued_gameplay_both_seats() {
    for actor in [PlayerSeatV1::P0, PlayerSeatV1::P1] {
        let mut wire = Wire::new();
        let deck = json!({"cards": crate::fake_full_engine_admission_v1_tests::reference_rows()});
        wire.send(json!({"request_type": "reset", "expected_card_db_hash": KERNEL_CARDDB_HASH,
            "episode_id": 77, "env_seed": 912, "starting_player": actor,
            "decks": [deck.clone(), deck], "max_physical_decisions": 10000, "max_policy_steps": 10000}));
        assert_eq!(wire.menu()["actor"], json!(actor));
        assert_eq!(
            wire.menu()["state"]["own_hand"].as_array().unwrap().len(),
            7
        );
        safe(&wire.last);
        let snapshot = wire.snapshot();
        let stale = wire.request(|label| label.starts_with("Keep this hand"));
        for _ in 0..2 {
            wire.step(|label| label.starts_with("Keep this hand"));
        }
        wire.step(|label| label == "Pass priority");
        let continued = (
            wire.bytes(),
            wire.server
                .active
                .as_ref()
                .unwrap()
                .core
                .policy_step_count(),
            wire.server
                .active
                .as_ref()
                .unwrap()
                .core
                .physical_decision_count(),
        );
        let generation = wire.last["state"]["authority_generation"].as_u64().unwrap();
        wire.restore(snapshot);
        assert!(wire.last["state"]["authority_generation"].as_u64().unwrap() > generation);
        wire.reject_unchanged(stale, "stale_decision");
        for _ in 0..2 {
            wire.step(|label| label.starts_with("Keep this hand"));
        }
        wire.step(|label| label == "Pass priority");
        assert_eq!(
            (
                wire.bytes(),
                wire.server
                    .active
                    .as_ref()
                    .unwrap()
                    .core
                    .policy_step_count(),
                wire.server
                    .active
                    .as_ref()
                    .unwrap()
                    .core
                    .physical_decision_count()
            ),
            continued
        );
        assert!(wire.state().engine.combat.foundations_v1.is_some());
        let core = &wire.server.active.as_ref().unwrap().core;
        let bound = wire.server.pending.as_ref().unwrap();
        assert!(core
            .ravenous_current_decision_input_v51(bound.expected, bound.expected.acting_player)
            .is_ok());
        safe(&wire.last);
    }
}

#[test]
fn public_v3_full_ravenous_reference_reset_owned_restore_and_continued_gameplay_both_seats() {
    for actor in [PlayerSeatV1::P0, PlayerSeatV1::P1] {
        let mut wire = Wire::new();
        let deck =
            json!({"cards": crate::ravenous_full_engine_admission_v1_tests::reference_rows()});
        wire.send(json!({"request_type": "reset", "expected_card_db_hash": KERNEL_CARDDB_HASH,
            "episode_id": 77, "env_seed": 912, "starting_player": actor,
            "decks": [deck.clone(), deck], "max_physical_decisions": 10000, "max_policy_steps": 10000}));
        assert_eq!(wire.menu()["actor"], json!(actor));
        assert_eq!(
            wire.menu()["state"]["own_hand"].as_array().unwrap().len(),
            7
        );
        safe(&wire.last);
        let snapshot = wire.snapshot();
        let stale = wire.request(|label| label.starts_with("Keep this hand"));
        for _ in 0..2 {
            wire.step(|label| label.starts_with("Keep this hand"));
        }
        wire.step(|label| label == "Pass priority");
        let continued = (
            wire.bytes(),
            wire.server
                .active
                .as_ref()
                .unwrap()
                .core
                .policy_step_count(),
            wire.server
                .active
                .as_ref()
                .unwrap()
                .core
                .physical_decision_count(),
        );
        let generation = wire.last["state"]["authority_generation"].as_u64().unwrap();
        wire.restore(snapshot);
        assert!(wire.last["state"]["authority_generation"].as_u64().unwrap() > generation);
        wire.reject_unchanged(stale, "stale_decision");
        for _ in 0..2 {
            wire.step(|label| label.starts_with("Keep this hand"));
        }
        wire.step(|label| label == "Pass priority");
        assert_eq!(
            (
                wire.bytes(),
                wire.server
                    .active
                    .as_ref()
                    .unwrap()
                    .core
                    .policy_step_count(),
                wire.server
                    .active
                    .as_ref()
                    .unwrap()
                    .core
                    .physical_decision_count()
            ),
            continued
        );
        assert!(wire.state().engine.combat.foundations_v1.is_some());
        let core = &wire.server.active.as_ref().unwrap().core;
        let bound = wire.server.pending.as_ref().unwrap();
        assert!(core
            .ravenous_current_decision_input_v51(bound.expected, bound.expected.acting_player)
            .is_ok());
        safe(&wire.last);
    }
}
