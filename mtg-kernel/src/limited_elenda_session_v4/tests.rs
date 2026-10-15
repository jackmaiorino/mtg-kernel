//! Source-only schema-4 RPC cases. The wire never accepts privileged fixture state.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use super::*;
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::put;
use crate::state::{GameState, Zone};
use serde_json::{json, Value};

struct Wire {
    server: LimitedElendaJsonlServerV4,
    serial: u64,
    last: Value,
}
impl Wire {
    fn new() -> Self {
        Self {
            server: LimitedElendaJsonlServerV4::new(),
            serial: 0,
            last: Value::Null,
        }
    }
    fn fixture(state: GameState) -> Self {
        let mut wire = Self::new();
        wire.server.active = Some(Active {
            core: ElendaSessionV52::from_session(
                crate::rl_session::FastActorSessionV1::from_elenda_v52_fixture_state(state),
            )
            .unwrap(),
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
        row.insert("schema_version".into(), json!(4));
        let line = serde_json::to_string(&request).unwrap();
        self.last = serde_json::from_str(&self.server.handle_line(&line)).unwrap();
        assert_eq!(self.last["protocol"], PROTOCOL);
        assert_eq!(self.last["schema_version"], 4);
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
            .unwrap_or_else(|| panic!("actual schema-4 action absent: {}", menu["actions"]));
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
    fn state(&self) -> &GameState {
        self.server
            .active
            .as_ref()
            .unwrap()
            .core
            .fixture_state_v52()
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
        panic!("actual schema-4 paid operation did not finish");
    }
}

const ELENDA: &str = "Elenda, Saint of Dusk";
fn raw(actor: PlayerId, life: i32) -> GameState {
    let mut state = crate::extra_turn_v1::tests::ready(actor);
    state.players[actor.index()].life = life;
    for player in [actor, actor.opponent()] {
        state.players[player.index()].mana_pool = [10; 6];
    }
    crate::combat_damage_v1::enable_foundations_combat_v1(&mut state).unwrap();
    state
}
fn priority(wire: &mut Wire, actor: PlayerId) {
    for _ in 0..8 {
        if wire.menu()["actor"] == json!(PlayerSeatV1::from(actor)) {
            return;
        }
        wire.step(|label| label == "Pass priority");
    }
    panic!("actual wire priority absent");
}
fn cast(wire: &mut Wire, actor: PlayerId, name: &str, target: Option<&str>) {
    priority(wire, actor);
    let before: u32 = wire.state().players[actor.index()]
        .mana_pool
        .iter()
        .map(|&v| u32::from(v))
        .sum();
    wire.step(|label| label.starts_with(&format!("Begin casting {name} [")));
    if let Some(target) = target {
        wire.step(|label| {
            label.starts_with(&format!("Target {target} ["))
                || (label.starts_with(&format!("{name} ["))
                    && label.contains(&format!(": target {target} [")))
        });
    }
    let after: u32 = wire.state().players[actor.index()]
        .mana_pool
        .iter()
        .map(|&v| u32::from(v))
        .sum();
    assert_eq!(
        before - after,
        u32::from(
            CARD_DEFS[usize::from(crate::card_def::card_id_by_name(name).unwrap())].mana_value
        )
    );
}
fn drain_to(wire: &mut Wire, remaining: usize) {
    for _ in 0..64 {
        if wire.state().stack.len() == remaining && wire.state().engine.pending_triggers.is_empty()
        {
            return;
        }
        wire.step(|label| label == "Pass priority");
    }
    panic!("actual wire stack did not finish");
}
fn stats(wire: &Wire, power: i32, menace: bool) {
    let facts = &wire.menu()["state"]["elenda"];
    let row = &facts["current"][0]["traits"];
    assert_eq!(row["effective_power"], power);
    assert_eq!(row["effective_toughness"], power);
    assert_eq!(row["effective_menace"], menace);
    assert_eq!(row["effective_lifelink"], true);
    assert_eq!(row["hexproof_from_opponents_instants"], true);
    assert_eq!(
        facts["meaning"]["ordered_rules"].as_array().unwrap().len(),
        4
    );
}
fn safe(value: &Value) {
    let text = value.to_string();
    for private in [
        "arena_id",
        "card_db_id",
        "zone_change_count",
        "private_snapshot",
        "prebatch",
        "event_history",
        "original_world",
        "engine_indexes",
    ] {
        assert!(!text.contains(private), "private wire field:{private}");
    }
}
#[test]
fn v4_actual_paid_thresholds_both_actor_routing_owning_restore_stale_and_retry_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for (life, power, menace) in [(20, 4, false), (21, 5, true), (29, 5, true), (30, 10, true)]
        {
            let mut state = raw(actor, life);
            let elenda = put(&mut state, actor, ELENDA, Zone::Hand);
            let mut wire = Wire::fixture(state);
            assert_eq!(wire.menu()["actor"], json!(PlayerSeatV1::from(actor)));
            let request =
                wire.request(|label| label.starts_with("Begin casting Elenda, Saint of Dusk ["));
            let before: u32 = wire.state().players[actor.index()]
                .mana_pool
                .iter()
                .map(|&v| u32::from(v))
                .sum();
            let reply = wire.send(request.clone());
            assert!(reply["receipt"].is_object());
            let after: u32 = wire.state().players[actor.index()]
                .mana_pool
                .iter()
                .map(|&v| u32::from(v))
                .sum();
            assert_eq!(before - after, 4);
            assert!(wire.state().stack.iter().any(|item| item.source == elenda
                && item.v4.cast_method == Some(crate::state::CastMethodV4::Normal)));
            let paid = wire.snapshot();
            let stale = wire.request(|label| label == "Pass priority");
            drain_to(&mut wire, 0);
            stats(&wire, power, menace);
            safe(&wire.last);
            assert_eq!(wire.state().objects.get(elenda).zone, Zone::Battlefield);
            let public = wire.menu()["state"].clone();
            let bytes = wire.bytes();
            wire.restore(paid);
            wire.reject_unchanged(stale, "stale_decision");
            drain_to(&mut wire, 0);
            assert_eq!(wire.menu()["state"], public);
            assert_eq!(wire.bytes(), bytes);
            // Exact accepted retries never execute again; generation still binds them.
            let retry = wire.request(|label| label == "Pass priority");
            wire.send(retry.clone());
            let after = wire.bytes();
            let accepted = wire.send(retry.clone());
            assert!(accepted["receipt"].is_object());
            assert_eq!(wire.bytes(), after);
            let mut conflict = retry;
            conflict["action_index"] = json!(u32::MAX);
            wire.reject_unchanged(conflict, "conflicting_retry");
        }
    }
}
#[test]
fn v4_genuine_paid_pending_target_control_suppression_and_instant_menu_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = raw(actor, 30);
        let elenda = put(&mut state, actor, ELENDA, Zone::Hand);
        put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        put(&mut state, actor.opponent(), "Lightning Bolt", Zone::Hand);
        put(
            &mut state,
            actor.opponent(),
            "Involuntary Employment",
            Zone::Hand,
        );
        put(
            &mut state,
            actor.opponent(),
            "Witness Protection",
            Zone::Hand,
        );
        put(
            &mut state,
            actor.opponent(),
            "High Fae Trickster",
            Zone::Battlefield,
        );
        let mut wire = Wire::fixture(state);
        cast(&mut wire, actor, ELENDA, None);
        wire.drain();
        priority(&mut wire, actor.opponent());
        wire.step(|label| label.starts_with("Begin casting Lightning Bolt ["));
        assert!(!wire.menu()["actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["label"]
                .as_str()
                .unwrap()
                .starts_with("Target Elenda, Saint of Dusk [")));
        wire.step(|label| label.starts_with("Target you with Lightning Bolt ["));
        wire.drain();
        cast(&mut wire, actor, "Lightning Bolt", Some(ELENDA));
        let paid = wire.snapshot();
        cast(
            &mut wire,
            actor.opponent(),
            "Involuntary Employment",
            Some(ELENDA),
        );
        drain_to(&mut wire, 1);
        stats(&wire, 4, false);
        assert_eq!(
            wire.menu()["state"]["elenda"]["current"][0]["source"]["controller"],
            json!(PlayerSeatV1::from(actor.opponent()))
        );
        let changed = wire.snapshot();
        cast(
            &mut wire,
            actor.opponent(),
            "Witness Protection",
            Some(ELENDA),
        );
        drain_to(&mut wire, 1);
        let lost = &wire.menu()["state"]["elenda"]["current"][0]["traits"];
        assert_eq!(lost["printed_abilities_active"], false);
        assert_eq!(lost["hexproof_from_opponents_instants"], false);
        assert_eq!(lost["effective_power"], 1);
        wire.drain();
        assert_eq!(wire.state().objects.get(elenda).zone, Zone::Graveyard);
        safe(&wire.last);
        wire.restore(changed);
        wire.drain();
        assert_eq!(wire.state().objects.get(elenda).zone, Zone::Battlefield);
        assert_eq!(wire.state().objects.get(elenda).damage, 0);
        wire.restore(paid);
        cast(
            &mut wire,
            actor.opponent(),
            "Involuntary Employment",
            Some(ELENDA),
        );
        wire.drain();
        assert_eq!(wire.state().objects.get(elenda).damage, 0);
    }
}
#[test]
fn v4_paid_copy_legend_choice_suppression_and_restored_exact_menu_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = raw(actor, 30);
        let elenda = put(&mut state, actor, ELENDA, Zone::Hand);
        for name in ["Self-Reflection", "Witness Protection", "Broken Wings"] {
            put(&mut state, actor, name, Zone::Hand);
        }
        let mut wire = Wire::fixture(state);
        cast(&mut wire, actor, ELENDA, None);
        wire.drain();
        cast(&mut wire, actor, "Self-Reflection", Some(ELENDA));
        for _ in 0..16 {
            if wire.menu()["actions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|row| row["label"].as_str().unwrap().starts_with("Keep "))
            {
                break;
            }
            wire.step(|label| label == "Pass priority");
        }
        let saved = wire.snapshot();
        let keep_copy = |wire: &mut Wire| {
            let copy = wire.menu()["state"]["elenda"]["current"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| row["traits"]["token"] == true)
                .unwrap()["source"]["handle"]
                .as_str()
                .unwrap()
                .to_owned();
            wire.step(|label| label.starts_with("Keep ") && label.contains(&format!("[{copy}]")));
            wire.drain();
        };
        keep_copy(&mut wire);
        stats(&wire, 10, true);
        assert_eq!(wire.state().objects.get(elenda).zone, Zone::Graveyard);
        let public = wire.menu()["state"].clone();
        let bytes = wire.bytes();
        wire.restore(saved);
        keep_copy(&mut wire);
        assert_eq!(wire.menu()["state"], public);
        assert_eq!(wire.bytes(), bytes);
        cast(&mut wire, actor, "Witness Protection", Some(ELENDA));
        wire.drain();
        assert_eq!(
            wire.menu()["state"]["elenda"]["current"][0]["traits"]["effective_lifelink"],
            false
        );
        cast(&mut wire, actor, "Broken Wings", Some("Witness Protection"));
        wire.drain();
        stats(&wire, 10, true);
    }
}
#[test]
fn v4_genuine_paid_fake_elenda_coexistence_wide_values_and_independent_hidden_worlds_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let build = |hand: &str, library: &str| {
            let mut state = raw(actor, 30);
            put(&mut state, actor.opponent(), hand, Zone::Hand);
            put(&mut state, actor.opponent(), library, Zone::Library);
            put(&mut state, actor, ELENDA, Zone::Hand);
            put(&mut state, actor, "Fake Your Own Death", Zone::Hand);
            let amulet = put(&mut state, actor, "Ravenous Amulet", Zone::Battlefield);
            // Native widths are explicit initial primitive inputs.
            state.objects.get_mut(amulet).counters.soul = 65537;
            state.objects.get_mut(amulet).counters.plus1_plus1 = 65538;
            state.objects.get_mut(amulet).damage = 70000;
            let mut wire = Wire::fixture(state);
            cast(&mut wire, actor, ELENDA, None);
            wire.drain();
            cast(&mut wire, actor, "Fake Your Own Death", Some(ELENDA));
            wire.drain();
            priority(&mut wire, actor);
            safe(&wire.last);
            wire
        };
        let a = build("Lightning Bolt", "Forest");
        let b = build("Witness Protection", ELENDA);
        assert_ne!(a.bytes(), b.bytes());
        assert_eq!(a.menu(), b.menu());
        let predecessor = &a.menu()["state"]["predecessor"];
        assert!(!predecessor["extensions"]["fake"]["grants"]
            .as_array()
            .unwrap()
            .is_empty());
        let wide = predecessor["public"]["battlefield"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|rows| rows.as_array().unwrap())
            .find(|row| row["card"]["stable"]["name"] == "Ravenous Amulet")
            .unwrap();
        assert_eq!(wide["soul"], 65537);
        assert_eq!(wide["plus_one_counters"], 65538);
        assert_eq!(wide["marked_damage"], 70000);
    }
}
#[test]
fn v4_strict_wire_types_no_state_injection_owner_restore_and_original_menu_forgery_refusal() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = raw(actor, 30);
        put(&mut state, actor, ELENDA, Zone::Hand);
        let mut wire = Wire::fixture(state.clone());
        let mut other = Wire::fixture(state);
        let id = other.snapshot();
        let saved = other.server.snapshots.remove(&id).unwrap();
        wire.server.snapshots.insert(id, saved);
        wire.reject_unchanged(
            json!({"request_type":"restore","snapshot_id":id}),
            "foreign_snapshot",
        );
        let request =
            wire.request(|label| label.starts_with("Begin casting Elenda, Saint of Dusk ["));
        let before = wire.bytes();
        wire.server.pending.as_mut().unwrap().indexes.reverse();
        let reply = wire.send(request);
        assert_eq!(reply["code"], "changed_safe_prompt");
        assert_eq!(wire.bytes(), before);
        for text in [
            r#"{"request_type":"current","schema_version":3,"request_id":"old"}"#,
            r#"{"request_type":"step","schema_version":4,"request_id":"bool","episode_id":23,"authority_generation":true,"actor":"p0","prompt_seq":1,"action_index":0}"#,
            r#"{"request_type":"restore","schema_version":4,"request_id":"state","snapshot_id":1,"state":{}}"#,
            r#"{"request_type":"current","schema_version":4,"schema_version":4,"request_id":"duplicate"}"#,
        ] {
            let reply: Value = serde_json::from_str(&wire.server.handle_line(text)).unwrap();
            assert_eq!(reply["response_type"], "error");
            assert_eq!(wire.bytes(), before);
        }
    }
    let mut old = crate::limited_common_session_v3::LimitedCommonJsonlServerV3::new();
    assert!(old
        .handle_line(r#"{"request_type":"current","schema_version":4,"request_id":"v4"}"#)
        .contains("invalid_header"));
    for actor in [PlayerSeatV1::P0, PlayerSeatV1::P1] {
        let mut wire = Wire::new();
        let reset = json!({"request_type":"reset","expected_card_db_hash":KERNEL_CARDDB_HASH,"episode_id":7,"env_seed":123,"starting_player":actor,"decks":[{"cards":[{"name":"Forest","count":40}]},{"cards":[{"name":"Island","count":40}]}],"max_physical_decisions":10000,"max_policy_steps":10000});
        wire.send(reset.clone());
        assert_eq!(wire.menu()["actor"], json!(actor));
        assert!(wire.menu()["london_mulligans"].is_object());
        let saved = wire.snapshot();
        let stale = wire.request(|label| label.starts_with("Keep this hand"));
        wire.restore(saved);
        wire.reject_unchanged(stale, "stale_decision");
        let before = wire.bytes();
        let mut partial = reset;
        partial["decks"][0]["cards"] =
            json!([{"name":"Forest","count":39},{"name":ELENDA,"count":1}]);
        let reply = wire.send(partial);
        assert_eq!(reply["code"], "unsupported_deck");
        assert_eq!(wire.bytes(), before);
    }
}

#[test]
fn v4_paid_elenda_natural_combat_lifelink_current_thresholds_and_restored_replay_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = raw(actor, 20);
        put(&mut state, actor, ELENDA, Zone::Hand);
        let mut wire = Wire::fixture(state);
        cast(&mut wire, actor, ELENDA, None);
        wire.drain();
        let saved = wire.snapshot();
        let play = |wire: &mut Wire| {
            let mut seen = Vec::new();
            let mut combat = false;
            for _ in 0..1024 {
                let life = wire.state().players[actor.index()].life;
                if seen.last().map(|&(previous, _)| previous) != Some(life) && life > 20 {
                    seen.push((
                        life,
                        wire.menu()["state"]["elenda"]["current"][0]["traits"]["effective_power"]
                            .as_i64()
                            .unwrap(),
                    ));
                }
                combat |= wire.menu()["foundations_combat"].is_object();
                if life >= 34 {
                    assert!(combat);
                    return seen;
                }
                let actions = wire.menu()["actions"].as_array().unwrap();
                let chosen = actions
                    .iter()
                    .find(|row| {
                        let label = row["label"].as_str().unwrap();
                        label.starts_with("Attack with Elenda, Saint of Dusk [")
                            || label
                                .starts_with("Attack your opponent with Elenda, Saint of Dusk [")
                            || label.starts_with("Declare attackers: Elenda, Saint of Dusk [")
                    })
                    .or_else(|| actions.iter().find(|row| row["label"] == "Pass priority"))
                    .or_else(|| (actions.len() == 1).then(|| &actions[0]))
                    .expect("actual wire combat choice")["label"]
                    .as_str()
                    .unwrap()
                    .to_owned();
                wire.step(|label| label == chosen);
            }
            panic!("natural wire combat did not reach thresholds");
        };
        let thresholds = play(&mut wire);
        assert_eq!(thresholds, [(24, 5), (29, 5), (34, 10)]);
        stats(&wire, 10, true);
        assert_eq!(wire.state().players[actor.opponent().index()].life, 6);
        safe(&wire.last);
        let public = wire.menu()["state"].clone();
        let bytes = wire.bytes();
        wire.restore(saved);
        assert_eq!(play(&mut wire), thresholds);
        assert_eq!(wire.menu()["state"], public);
        assert_eq!(wire.bytes(), bytes);
    }
}
