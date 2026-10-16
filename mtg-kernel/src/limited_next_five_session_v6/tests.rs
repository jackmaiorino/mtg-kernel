//! Source-only schema-6 RPC cases. The wire never accepts privileged fixture state.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use super::*;
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::put;
use crate::rl::ActionSemanticV1;
use crate::state::{GameState, Zone};
use serde_json::{json, Value};

#[test]
fn v6_all_five_paid_native_casts_exact_wire_indices_and_restored_resolution_both_seats() {
    use crate::next_five_public_v59::tests as native;
    use crate::rl::TargetRefV1;
    use crate::rules_meaning_next_five_v59::FamilyV59;

    fn drain(wire: &mut Wire) {
        for _ in 0..64 {
            let state = wire.state();
            if state.stack.is_empty()
                && state.engine.pending_cast.is_none()
                && state.engine.pending_activation.is_none()
                && state.engine.pending_effect.is_none()
                && state.engine.pending_triggers.is_empty()
            {
                return;
            }
            wire.native_step(|a| matches!(a, ActionSemanticV1::Pass { .. }));
        }
        panic!("genuine V6 paid cast did not resolve");
    }

    for actor in [PlayerId::P0, PlayerId::P1] {
        for family in FamilyV59::ALL {
            let mut state = native::ready(actor);
            for player in [actor, actor.opponent()] {
                put(&mut state, player, "Forest", Zone::Hand);
            }
            let target = put(
                &mut state,
                actor.opponent(),
                "Axgard Cavalry",
                Zone::Battlefield,
            );
            let source = put(&mut state, actor, family.name(), Zone::Hand);
            let tapped = |s: &GameState| {
                s.players[actor.index()]
                    .battlefield
                    .iter()
                    .filter(|id| s.objects.get(**id).tapped)
                    .count()
            };
            let before = tapped(&state);
            let mut wire = Wire::fixture(state);
            assert_eq!(
                wire.menu()["state"]["next_five"]["viewer"],
                json!(PlayerSeatV1::from(actor))
            );
            assert_eq!(
                wire.menu()["state"]["next_five"]["meanings"]
                    .as_array()
                    .unwrap()
                    .len(),
                5
            );
            wire.native_step(|a| {
                matches!(a, ActionSemanticV1::CastSpell { source: r, .. }
                if r.arena_id == source.0)
            });
            if family == FamilyV59::Goblin {
                let cast = wire.menu()["state"]["next_five"]["casts"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|r| r["family"] == "Goblin")
                    .unwrap();
                assert!(cast["payment_pending"].as_bool().unwrap());
                assert!(cast["x"].is_null());
                assert!(cast["targets"].as_array().unwrap().is_empty());
                wire.native_step(|a| {
                    matches!(a, ActionSemanticV1::ChooseTarget {
                    target: TargetRefV1::Object { object }, .. } if object.arena_id == target.0)
                });
                let saved_x = wire.snapshot();
                let stale_x = wire.request(|label| label.contains("X = 3"));
                wire.native_step(|a| {
                    matches!(
                        a,
                        ActionSemanticV1::ChooseEffectOption {
                            option_index: 3,
                            ..
                        }
                    )
                });
                let paid = wire.bytes();
                wire.restore(saved_x);
                wire.reject_unchanged(stale_x, "stale_decision");
                wire.native_step(|a| {
                    matches!(
                        a,
                        ActionSemanticV1::ChooseEffectOption {
                            option_index: 3,
                            ..
                        }
                    )
                });
                assert_eq!(wire.bytes(), paid);
            }
            assert!(wire.state().engine.pending_cast.is_none());
            assert_eq!(wire.state().objects.get(source).zone, Zone::Stack);
            let definition = &CARD_DEFS[usize::from(wire.state().objects.get(source).card_def)];
            assert_eq!(
                tapped(wire.state()) - before,
                usize::from(definition.mana_value)
                    + if family == FamilyV59::Goblin { 3 } else { 0 }
            );
            let facts = &wire.menu()["state"]["next_five"];
            let cast = facts["casts"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["family"] == json!(family))
                .unwrap();
            assert_eq!(cast["source"]["name"], family.name());
            assert_eq!(
                cast["original_controller"],
                json!(PlayerSeatV1::from(actor))
            );
            assert_eq!(cast["payment_pending"], false);
            assert_eq!(
                cast["x"],
                if family == FamilyV59::Goblin {
                    json!(3)
                } else {
                    json!(0)
                }
            );
            safe(&wire.last);
            let saved = wire.snapshot();
            drain(&mut wire);
            let resolved = wire.bytes();
            if family == FamilyV59::Goblin {
                assert_eq!(wire.state().objects.get(source).zone, Zone::Graveyard);
                assert_eq!(wire.state().objects.get(target).zone, Zone::Graveyard);
                let completion = &wire.menu()["state"]["next_five"]["goblin_completed"][0];
                assert_eq!(completion["actual_damage"], 3);
                assert_eq!(completion["tokens_created"], 1);
            } else {
                assert_eq!(wire.state().objects.get(source).zone, Zone::Battlefield);
            }
            wire.restore(saved);
            drain(&mut wire);
            assert_eq!(wire.bytes(), resolved);
            safe(&wire.last);
        }
    }
}

struct Wire {
    server: LimitedNextFiveJsonlServerV6,
    serial: u64,
    last: Value,
}
impl Wire {
    fn new() -> Self {
        Self {
            server: LimitedNextFiveJsonlServerV6::new(),
            serial: 0,
            last: Value::Null,
        }
    }
    fn fixture(state: GameState) -> Self {
        let mut wire = Self::new();
        wire.server.active = Some(Active {
            core: NextFiveSessionV59::from_session(
                crate::rl_session::FastActorSessionV1::from_next_five_v59_fixture_state(state),
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
        row.insert("schema_version".into(), json!(6));
        let line = serde_json::to_string(&request).unwrap();
        self.last = serde_json::from_str(&self.server.handle_line(&line)).unwrap();
        assert_eq!(self.last["protocol"], PROTOCOL);
        assert_eq!(self.last["schema_version"], 6);
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
            .unwrap_or_else(|| panic!("actual schema-6 action absent: {}", menu["actions"]));
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
            .fixture_state_v59()
    }
    fn bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self.state()).unwrap()
    }
    fn native_step(&mut self, choose: impl Fn(&ActionSemanticV1) -> bool) {
        let bound = self
            .server
            .pending
            .as_ref()
            .expect("actual executable V6 menu");
        let native_index = bound
            .decision
            .actions()
            .iter()
            .position(choose)
            .expect("required genuine native action");
        let visible_index = bound
            .indexes
            .iter()
            .position(|index| *index as usize == native_index)
            .expect("original native index retained in V6 prompt");
        let mut reference = NextFiveSessionV59::from_session(
            crate::rl_session::FastActorSessionV1::from_next_five_v59_fixture_state(
                self.state().clone(),
            ),
        )
        .unwrap();
        let expected = reference.decision_v59().unwrap();
        assert_eq!(expected.actions(), bound.decision.actions());
        reference.step_v59(&expected, native_index as u32).unwrap();
        let request = json!({"request_type":"step",
            "episode_id":self.last["state"]["episode_id"],
            "authority_generation":self.last["state"]["authority_generation"],
            "actor":self.menu()["actor"],"prompt_seq":self.menu()["prompt_seq"],
            "action_index":visible_index});
        let reply = self.send(request);
        assert_eq!(reply["response_type"], "state", "{reply}");
        assert_eq!(reply["receipt"]["action_index"], visible_index);
        assert_eq!(self.state(), reference.fixture_state_v59());
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
        panic!("actual schema-6 paid operation did not finish");
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
    let facts = &wire.menu()["state"]["common"]["elenda"];
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
        "prebatch_snapshot",
        "prebatch_world",
        "event_history",
        "original_world",
        "engine_indexes",
    ] {
        assert!(!text.contains(private), "private wire field:{private}");
    }
}
#[test]
fn v6_actual_paid_thresholds_both_actor_routing_owning_restore_stale_and_retry_both_seats() {
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
fn v6_genuine_paid_pending_target_control_suppression_and_instant_menu_both_seats() {
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
            wire.menu()["state"]["common"]["elenda"]["current"][0]["source"]["controller"],
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
        let lost = &wire.menu()["state"]["common"]["elenda"]["current"][0]["traits"];
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
fn v6_paid_copy_legend_choice_suppression_and_restored_exact_menu_both_seats() {
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
            let copy = wire.menu()["state"]["common"]["elenda"]["current"]
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
            wire.menu()["state"]["common"]["elenda"]["current"][0]["traits"]["effective_lifelink"],
            false
        );
        cast(&mut wire, actor, "Broken Wings", Some("Witness Protection"));
        wire.drain();
        stats(&wire, 10, true);
    }
}
#[test]
fn v6_genuine_paid_fake_elenda_coexistence_wide_values_and_independent_hidden_worlds_both_seats() {
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
            // Primitive native-width counter input on an unrelated artifact.
            state.objects.get_mut(amulet).counters.incubation = 65539;
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
        let predecessor = &a.menu()["state"]["common"]["predecessor"];
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
        let full = a.menu()["state"]["prepared"]["drake"]["battlefield"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|rows| rows.as_array().unwrap())
            .find(|row| row["source"]["name"] == "Ravenous Amulet")
            .unwrap();
        assert_eq!(full["counters"]["incubation"], 65539);
        assert_eq!(full["counters"]["soul"], 65537);
        assert_eq!(full["counters"]["plus_one"], 65538);
        assert_eq!(full["marked_damage"], 70000);
    }
}
#[test]
fn v6_strict_wire_types_no_state_injection_owner_restore_and_original_menu_forgery_refusal() {
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
            r#"{"request_type":"step","schema_version":6,"request_id":"bool","episode_id":23,"authority_generation":true,"actor":"p0","prompt_seq":1,"action_index":0}"#,
            r#"{"request_type":"restore","schema_version":6,"request_id":"state","snapshot_id":1,"state":{}}"#,
            r#"{"request_type":"current","schema_version":6,"schema_version":6,"request_id":"duplicate"}"#,
        ] {
            let reply: Value = serde_json::from_str(&wire.server.handle_line(text)).unwrap();
            assert_eq!(reply["response_type"], "error");
            assert_eq!(wire.bytes(), before);
        }
    }
    let mut old = crate::limited_common_session_v3::LimitedCommonJsonlServerV3::new();
    assert!(old
        .handle_line(r#"{"request_type":"current","schema_version":6,"request_id":"v6"}"#)
        .contains("invalid_header"));
    let mut predecessor = crate::limited_prepared_session_v5::LimitedPreparedJsonlServerV5::new();
    assert!(predecessor
        .handle_line(r#"{"request_type":"current","schema_version":6,"request_id":"v6"}"#)
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
            json!([{"name":"Forest","count":39},{"name":"Abyssal Harvester","count":1}]);
        let reply = wire.send(partial);
        assert_eq!(reply["code"], "unsupported_deck");
        assert_eq!(wire.bytes(), before);
    }
}

#[test]
fn v6_paid_elenda_natural_combat_lifelink_current_thresholds_and_restored_replay_both_seats() {
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
                        wire.menu()["state"]["common"]["elenda"]["current"][0]["traits"]
                            ["effective_power"]
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

#[test]
fn v6_paid_hatcher_natural_combat_exact_debit_and_created_token_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = raw(actor, 20);
        let hatcher = put(&mut state, actor, "Drake Hatcher", Zone::Hand);
        let mut wire = Wire::fixture(state);
        cast(&mut wire, actor, "Drake Hatcher", None);
        wire.drain();
        // Counters come from real declarations and player combat damage through
        // this wire. No damage batch, counters or pending roots are injected.
        for _ in 0..1024 {
            if wire.state().objects.get(hatcher).counters.incubation >= 3 {
                break;
            }
            let actions = wire.menu()["actions"].as_array().unwrap();
            let selected = actions
                .iter()
                .find(|row| {
                    let label = row["label"].as_str().unwrap();
                    label.contains("Drake Hatcher [")
                        && (label.starts_with("Attack ") || label.starts_with("Declare attackers:"))
                })
                .or_else(|| actions.iter().find(|row| row["label"] == "Pass priority"))
                .or_else(|| (actions.len() == 1).then(|| &actions[0]))
                .expect("genuine Hatcher combat menu")["label"]
                .as_str()
                .unwrap()
                .to_owned();
            wire.step(|label| label == selected);
        }
        assert_eq!(wire.state().objects.get(hatcher).counters.incubation, 3);
        wire.drain();
        priority(&mut wire, actor);
        let facts = &wire.menu()["state"]["prepared"]["drake"];
        assert_eq!(facts["combat"].as_array().unwrap().len(), 3);
        assert!(facts["combat"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["final_positive_damage"] == 1));
        let saved = wire.snapshot();
        let annotations = wire.menu()["action_meanings"].as_array().unwrap();
        let index = annotations.iter().position(|row| row.is_object()).unwrap();
        assert_eq!(annotations[index]["count"], 3);
        assert_eq!(annotations[index]["before"], 3);
        assert_eq!(annotations[index]["after"], 0);
        assert!(!annotations[index]
            .as_object()
            .unwrap()
            .contains_key("ability_index"));
        let label = wire.menu()["actions"][index]["label"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(label.contains("remove 3 incubation counters (3 to 0)"));
        let stale = wire.request(|row| row == label);
        // Mutating an issued annotation must refuse without changing gameplay.
        wire.server
            .pending
            .as_mut()
            .unwrap()
            .visible
            .action_meanings[index]
            .as_mut()
            .unwrap()
            .count = 2;
        wire.reject_unchanged(stale.clone(), "changed_safe_prompt");
        wire.step(|row| row == label);
        assert_eq!(wire.state().objects.get(hatcher).counters.incubation, 0);
        wire.drain();
        let tokens = wire.state().players[actor.index()]
            .battlefield
            .iter()
            .filter(|id| {
                let object = wire.state().objects.get(**id);
                object.v4.is_token && CARD_DEFS[usize::from(object.card_def)].name == "Drake Token"
            })
            .count();
        assert_eq!(tokens, 1);
        assert_eq!(
            wire.menu()["state"]["prepared"]["drake"]["activations"][0]["payment"]["removed"],
            3
        );
        safe(&wire.last);
        let bytes = wire.bytes();
        let visible = wire.menu()["state"].clone();
        wire.restore(saved);
        wire.reject_unchanged(stale, "stale_decision");
        wire.step(|row| row == label);
        wire.drain();
        assert_eq!(wire.bytes(), bytes);
        assert_eq!(wire.menu()["state"], visible);
    }
}

fn until_label(wire: &mut Wire, prefix: &str) {
    for _ in 0..128 {
        if wire.menu()["actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["label"].as_str().unwrap().starts_with(prefix))
        {
            return;
        }
        wire.step(|label| label == "Pass priority");
    }
    panic!("actual prepared choice absent: {prefix}");
}
#[test]
fn v6_paid_spinner_death_and_phoenix_graveyard_return_have_typed_choices_and_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = raw(actor, 20);
        let phoenix = put(&mut state, actor, "Flamewake Phoenix", Zone::Hand);
        put(&mut state, actor, "Spinner of Souls", Zone::Hand);
        // Supporting victim and later actual red-payment sources are primitive
        // inputs. Printed Phoenix/Spinner casts and both Bolts are paid below.
        put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        for _ in 0..2 {
            put(&mut state, actor, "Mountain", Zone::Battlefield);
            put(&mut state, actor.opponent(), "Lightning Bolt", Zone::Hand);
        }
        let mut wire = Wire::fixture(state);
        cast(&mut wire, actor, "Flamewake Phoenix", None);
        wire.drain();
        cast(&mut wire, actor, "Spinner of Souls", None);
        wire.drain();
        cast(
            &mut wire,
            actor.opponent(),
            "Lightning Bolt",
            Some("Llanowar Elves"),
        );
        until_label(&mut wire, "Decline Spinner reveal");
        let saved = wire.snapshot();
        assert!(!wire.menu()["state"]["prepared"]["spinner"]["stack"]
            .as_array()
            .unwrap()
            .is_empty());
        let before = wire.bytes();
        wire.server
            .pending
            .as_mut()
            .unwrap()
            .visible
            .state
            .prepared
            .spinner
            .stack[0]
            .stage = crate::spinner_public_v54::StageV54::OnStack;
        let forged = wire.request(|label| label.starts_with("Decline Spinner reveal"));
        wire.reject_unchanged(forged, "changed_safe_prompt");
        assert_eq!(wire.bytes(), before);
        wire.step(|label| label.starts_with("Decline Spinner reveal"));
        wire.drain();
        let declined = wire.bytes();
        wire.restore(saved);
        wire.step(|label| label.starts_with("Decline Spinner reveal"));
        wire.drain();
        assert_eq!(wire.bytes(), declined);
        cast(
            &mut wire,
            actor.opponent(),
            "Lightning Bolt",
            Some("Flamewake Phoenix"),
        );
        until_label(&mut wire, "Decline Spinner reveal");
        wire.step(|label| label.starts_with("Decline Spinner reveal"));
        wire.drain();
        assert_eq!(wire.state().objects.get(phoenix).zone, Zone::Graveyard);
        until_label(&mut wire, "Pay {R}; return the original Phoenix");
        let saved = wire.snapshot();
        let request =
            wire.request(|label| label.starts_with("Pay {R}; return the original Phoenix"));
        let untapped = |wire: &Wire| {
            wire.state().players[actor.index()]
                .battlefield
                .iter()
                .filter(|id| {
                    let o = wire.state().objects.get(**id);
                    !o.tapped && CARD_DEFS[usize::from(o.card_def)].name == "Mountain"
                })
                .count()
        };
        let before = untapped(&wire);
        assert!(
            wire.menu()["state"]["prepared"]["phoenix"]["ferocious_now"][actor.index()]
                .as_bool()
                .unwrap()
        );
        wire.step(|label| label.starts_with("Pay {R}; return the original Phoenix"));
        wire.drain();
        assert_eq!(wire.state().objects.get(phoenix).zone, Zone::Battlefield);
        assert_eq!(untapped(&wire), before - 1);
        let bytes = wire.bytes();
        let facts = wire.menu()["state"].clone();
        safe(&wire.last);
        wire.restore(saved);
        wire.reject_unchanged(request, "stale_decision");
        wire.step(|label| label.starts_with("Pay {R}; return the original Phoenix"));
        wire.drain();
        assert_eq!(wire.bytes(), bytes);
        assert_eq!(wire.menu()["state"], facts);
    }
}
#[test]
fn v6_paid_quandary_routes_original_opponent_payer_private_hand_and_answer_restore_both_seats() {
    for controller in [PlayerId::P0, PlayerId::P1] {
        for discard in [false, true] {
            let payer = controller.opponent();
            let mut state = raw(controller, 20);
            put(&mut state, controller, "Painful Quandary", Zone::Hand);
            let spell = put(&mut state, payer, "Think Twice", Zone::Hand);
            let forest = put(&mut state, payer, "Forest", Zone::Hand);
            let mut wire = Wire::fixture(state);
            cast(&mut wire, controller, "Painful Quandary", None);
            wire.drain();
            cast(&mut wire, payer, "Think Twice", None);
            until_label(&mut wire, "Decline Quandary discard");
            assert_eq!(wire.menu()["actor"], json!(PlayerSeatV1::from(payer)));
            let facts = &wire.menu()["state"]["prepared"]["quandary"];
            assert_eq!(facts["viewer"], json!(PlayerSeatV1::from(payer)));
            let row = &facts["stack"][0]["stage"]["AwaitingPayment"];
            assert_eq!(row["payer"], json!(PlayerSeatV1::from(payer)));
            assert!(row["payer_hand"]["original_hand"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["name"] == "Forest"));
            let label = if discard {
                "Discard Forest ["
            } else {
                "Decline Quandary discard"
            };
            let saved = wire.snapshot();
            let original = wire.menu()["state"].clone();
            wire.step(|actual| actual.starts_with(label));
            wire.drain();
            assert_eq!(
                wire.state().players[payer.index()].life,
                if discard { 20 } else { 15 }
            );
            assert_eq!(
                wire.state().objects.get(forest).zone,
                if discard { Zone::Graveyard } else { Zone::Hand }
            );
            assert_eq!(wire.state().objects.get(spell).zone, Zone::Graveyard);
            let bytes = wire.bytes();
            safe(&wire.last);
            wire.restore(saved);
            assert_eq!(wire.menu()["state"], original);
            wire.step(|actual| actual.starts_with(label));
            wire.drain();
            assert_eq!(wire.bytes(), bytes);
        }
    }
}

#[test]
fn v6_paid_hatcher_independent_unknown_hand_and_library_preserve_actor_dto_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let build = |hidden: &str| {
            let mut state = raw(actor, 20);
            put(&mut state, actor.opponent(), hidden, Zone::Hand);
            put(&mut state, actor.opponent(), hidden, Zone::Library);
            put(&mut state, actor, "Drake Hatcher", Zone::Hand);
            let mut wire = Wire::fixture(state);
            cast(&mut wire, actor, "Drake Hatcher", None);
            wire.drain();
            priority(&mut wire, actor);
            wire
        };
        let a = build("Drake Hatcher");
        let b = build("Llanowar Elves");
        assert_ne!(a.bytes(), b.bytes());
        assert_eq!(a.menu(), b.menu());
        safe(&a.last);
    }
}

fn paid_spinner_wire_death_group(actor: PlayerId, count: usize, hunter: bool) -> GameState {
    let mut raw = raw(actor, 20);
    put(&mut raw, actor, "Spinner of Souls", Zone::Hand);
    let mut paid = Wire::fixture(raw);
    cast(&mut paid, actor, "Spinner of Souls", None);
    paid.drain();
    let mut raw = paid.state().clone();
    // Same genuine simultaneous-death producer as the owning core fixture;
    // the victims and optional earlier-family Hunter are primitive support.
    if hunter {
        put(&mut raw, actor, "High-Society Hunter", Zone::Battlefield);
    }
    let victims = (0..count)
        .map(|_| put(&mut raw, actor, "Llanowar Elves", Zone::Battlefield))
        .collect::<Vec<_>>();
    crate::event::propose_and_commit_batch(
        &mut raw,
        victims
            .into_iter()
            .map(|object| crate::event::ProposedEvent::zone_change(object, Zone::Graveyard))
            .collect(),
    );
    raw
}
#[test]
fn v6_paid_spinner_identical_and_mixed_whole_order_choices_keep_native_indices_restore_both_seats()
{
    use crate::rl::ActionSemanticV1;
    for actor in [PlayerId::P0, PlayerId::P1] {
        for hunter in [false, true] {
            let mut wire = Wire::fixture(paid_spinner_wire_death_group(actor, 2, hunter));
            let count = if hunter { 4 } else { 2 };
            let saved = wire.snapshot();
            for order in [
                (0..count).collect::<Vec<_>>(),
                (0..count).rev().collect::<Vec<_>>(),
            ] {
                wire.restore(saved);
                let native = wire
                    .server
                    .active
                    .as_ref()
                    .unwrap()
                    .core
                    .decision_v59()
                    .unwrap();
                let native_index=native.actions().iter().position(|action|matches!(action,ActionSemanticV1::OrderTriggers{order:actual,..} if *actual==order)).unwrap() as u32;
                let visible_index = wire
                    .server
                    .pending
                    .as_ref()
                    .unwrap()
                    .indexes
                    .iter()
                    .position(|index| *index == native_index)
                    .unwrap();
                let label = wire.menu()["actions"][visible_index]["label"]
                    .as_str()
                    .unwrap()
                    .to_owned();
                assert!(
                    label.starts_with("Put trigger occurrences on the stack from bottom to top")
                );
                for occurrence in 1..=count {
                    assert_eq!(
                        label
                            .matches(&format!("occurrence #{occurrence} from "))
                            .count(),
                        1
                    );
                }
                if hunter {
                    assert!(label.contains("High-Society Hunter"));
                } else {
                    assert_eq!(wire.menu()["actions"].as_array().unwrap().len(), 2);
                }
                let request = wire.request(|actual| actual == label);
                wire.server.pending.as_mut().unwrap().indexes.reverse();
                wire.reject_unchanged(request, "changed_safe_prompt");
                let stale = wire.request(|actual| actual == label);
                wire.step(|actual| actual == label);
                assert_eq!(wire.state().stack.len(), count);
                safe(&wire.last);
                let bytes = wire.bytes();
                let public = wire.menu()["state"].clone();
                wire.restore(saved);
                wire.reject_unchanged(stale, "stale_decision");
                wire.step(|actual| actual == label);
                assert_eq!(wire.bytes(), bytes);
                assert_eq!(wire.menu()["state"], public);
            }
        }
    }
}
#[test]
fn v6_paid_spinner_eight_identical_staged_occurrences_bind_real_prefix_and_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut wire = Wire::fixture(paid_spinner_wire_death_group(actor, 8, false));
        let saved = wire.snapshot();
        let play = |wire: &mut Wire| {
            for (placed, index) in (0..8).rev().enumerate() {
                assert_eq!(wire.menu()["actions"].as_array().unwrap().len(), 8 - placed);
                let prefix = format!("Place Triggered ability occurrence #{} from ", index + 1);
                let label = wire.menu()["actions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find_map(|row| {
                        row["label"]
                            .as_str()
                            .filter(|label| label.starts_with(&prefix))
                    })
                    .unwrap()
                    .to_owned();
                if placed == 0 {
                    assert!(label.contains("already ordered: none"));
                } else {
                    assert!(label.contains("already ordered: #8"));
                }
                wire.step(|actual| actual == label);
                if placed < 7 {
                    assert!(wire.state().stack.is_empty());
                }
            }
        };
        play(&mut wire);
        assert_eq!(wire.state().stack.len(), 8);
        safe(&wire.last);
        let bytes = wire.bytes();
        let public = wire.menu()["state"].clone();
        wire.restore(saved);
        play(&mut wire);
        assert_eq!(wire.bytes(), bytes);
        assert_eq!(wire.menu()["state"], public);
    }
}

#[test]
fn v6_paid_exact_wide_snack_gain_and_panda_public_outcome_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, snack, _) = crate::snack_wide_life_v59::tests::scene(actor, true);
        crate::snack_wide_life_v59::tests::gain_input(&mut state, actor);
        for player in [actor, actor.opponent()] {
            put(&mut state, player, "Forest", Zone::Hand);
        }
        let mut wire = Wire::fixture(state);
        wire.native_step(|a|matches!(a,ActionSemanticV1::ActivateAbility{source,ability_index:0,..}if source.arena_id==snack.0));
        wire.native_step(|a|matches!(a,ActionSemanticV1::ChooseTarget{target:crate::rl::TargetRefV1::Player{player},..}if *player==actor.opponent().into()));
        let saved = wire.snapshot();
        let stale = wire.request(|label| label == "Pass priority");
        wire.drain();
        let after = wire.bytes();
        let wide = &wire.menu()["state"]["next_five"]["wide_life"];
        assert_eq!(
            wide["conqueror"][0]["saved_actual_amount"],
            2_200_000_000u32
        );
        assert_eq!(wide["conqueror"][0]["actual_gain"], 2_200_000_000u32);
        assert_eq!(wide["panda"][0]["life_gained"], 2_200_000_000u32);
        assert_eq!(wide["panda"][0]["actual_counters_added"], 1);
        let text = serde_json::to_string(wide).unwrap();
        for forbidden in [
            "arena_id",
            "history_index",
            "private_world",
            "linked_borrow",
            "inline_effect",
        ] {
            assert!(!text.contains(forbidden));
        }
        wire.restore(saved);
        let before = wire.bytes();
        let rejected = wire.send(stale);
        assert_eq!(rejected["response_type"], "error");
        assert_eq!(wire.bytes(), before);
        wire.send(json!({"request_type":"current"}));
        wire.drain();
        assert_eq!(wire.bytes(), after);
    }
}

#[test]
fn v6_production_five_full_candidate_real_decks_paid_reset_and_restore_both_seats() {
    use crate::next_five_public_v59::tests::production as p;
    use crate::rules_meaning_next_five_v59::FamilyV59;
    for family in FamilyV59::ALL {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut wire = Wire::new();
            let deck = json!({"cards":p::rows(family)});
            let reset = json!({"request_type":"reset","expected_card_db_hash":KERNEL_CARDDB_HASH,
            "episode_id":p::EPISODE,"env_seed":p::SEED,"starting_player":PlayerSeatV1::from(actor),
            "decks":[deck.clone(),deck],"max_physical_decisions":p::LIMITS.0,"max_policy_steps":p::LIMITS.1});
            let reply = wire.send(reset.clone());
            assert_eq!(reply["response_type"], "state", "{reply}");
            let mut native = p::native(family, actor);
            // Failed production reset is atomic, including the executable prompt.
            for name in ["Alesha, Who Laughs at Fate", "Goblin Token"] {
                let mut invalid = reset.clone();
                invalid["decks"][0]["cards"] =
                    json!([{"name":"Forest","count":39},{"name":name,"count":1}]);
                let before = wire.bytes();
                let prompt = wire.last["state"].clone();
                let reply = wire.send(invalid);
                assert_eq!(reply["code"], "unsupported_deck");
                assert_eq!(wire.bytes(), before);
                wire.send(json!({"request_type":"current"}));
                assert_eq!(wire.last["state"], prompt);
            }
            let mut source = None;
            let mut before = 0;
            let mut saved = None;
            let mut completed_once = None;
            let mut replay_completed = false;
            for _ in 0..4096 {
                assert_eq!(wire.state(), native.next_five_fixture_state());
                assert_eq!(
                    wire.server.active.as_ref().unwrap().core.current_response(),
                    native.current_response()
                );
                assert_eq!(
                    wire.server
                        .active
                        .as_ref()
                        .unwrap()
                        .core
                        .fixture_counts_v59(),
                    (native.policy_step_count(), native.physical_decision_count())
                );
                if let Some(id) = source {
                    if p::completed(wire.state(), family, id) {
                        p::assert_completed(wire.state(), family, id);
                        let bytes = wire.bytes();
                        if let Some(first) = &completed_once {
                            assert_eq!(&bytes, first);
                            replay_completed = true;
                            break;
                        }
                        completed_once = Some(bytes);
                        let (snapshot, native_snapshot) = saved.as_ref().unwrap();
                        let stale = wire.request(|_| true);
                        wire.restore(*snapshot);
                        native.restore_v1(native_snapshot);
                        wire.reject_unchanged(stale, "stale_decision");
                        continue;
                    }
                }
                let bound = wire.server.pending.as_ref().unwrap();
                let expected = bound.decision.expected();
                let i = p::choose(
                    wire.state(),
                    actor,
                    family,
                    source,
                    bound.decision.actions(),
                );
                if let Some(id) = p::selected_source(&bound.decision.actions()[i], family) {
                    source = Some(id);
                    before = p::tapped(wire.state(), actor);
                }
                let visible = bound
                    .indexes
                    .iter()
                    .position(|index| usize::try_from(*index) == Ok(i))
                    .unwrap();
                let request = json!({"request_type":"step","episode_id":wire.last["state"]["episode_id"],
                "authority_generation":wire.last["state"]["authority_generation"],"actor":wire.menu()["actor"],
                "prompt_seq":wire.menu()["prompt_seq"],"action_index":visible});
                let reply = wire.send(request.clone());
                assert_eq!(reply["response_type"], "state", "{reply}");
                assert_eq!(reply["receipt"]["action_index"], visible);
                let after = wire.bytes();
                let retried = wire.send(request);
                assert_eq!(retried["receipt"], reply["receipt"]);
                assert_eq!(retried["state"], reply["state"]);
                assert_eq!(wire.bytes(), after);
                native
                    .step(
                        expected.episode_id,
                        expected.step,
                        u32::try_from(i).unwrap(),
                    )
                    .unwrap();
                if saved.is_none()
                    && source.is_some_and(|id| p::paid(wire.state(), actor, family, id, before))
                {
                    saved = Some((wire.snapshot(), native.snapshot_v1()));
                }
                safe(&wire.last);
            }
            assert!(replay_completed);
        }
    }
}
