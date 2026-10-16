//! Source-only V7 wire cases with genuine paid native histories. Initial lands
//! and concealed cards are fixture setup, never accepted through the wire.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use super::*;
use crate::{
    graveyard_return_public_v61::tests as native,
    ids::{ObjectId, PlayerId},
    policy_observation_v6::tests::put,
    rl::{ActionSemanticV1, TargetRefV1},
    state::{GameState, Zone},
};
use serde_json::{json, Value};

fn taps(w: &Wire, actor: PlayerId) -> usize {
    w.state().players[actor.index()]
        .battlefield
        .iter()
        .filter(|&&id| {
            let o = w.state().objects.get(id);
            o.tapped && CARD_DEFS[usize::from(o.card_def)].is_land
        })
        .count()
}
fn cast(w: &mut Wire, actor: PlayerId, source: ObjectId) {
    let before = taps(w, actor);
    let mana =
        usize::from(CARD_DEFS[usize::from(w.state().objects.get(source).card_def)].mana_value);
    let bound = w.server.pending.as_ref().unwrap();
    let ordinal = bound
        .decision
        .actions()
        .iter()
        .position(|a| matches!(a,ActionSemanticV1::CastSpell{source:r,..} if r.arena_id==source.0))
        .unwrap();
    let visible_index = bound
        .indexes
        .iter()
        .position(|i| *i as usize == ordinal)
        .unwrap();
    let retry = json!({"request_type":"step", "episode_id":w.last["state"]["episode_id"],
        "authority_generation":w.last["state"]["authority_generation"], "actor":w.menu()["actor"],
        "prompt_seq":w.menu()["prompt_seq"], "action_index":visible_index});
    w.native_step(|a| matches!(a,ActionSemanticV1::CastSpell{source:r,..} if r.arena_id==source.0));
    assert!(w.state().engine.pending_cast.is_none());
    assert_eq!(w.state().objects.get(source).zone, Zone::Stack);
    assert_eq!(w.state().stack.last().unwrap().source, source);
    assert_eq!(taps(w, actor) - before, mana);
    assert_eq!(w.state().players[actor.index()].mana_pool, [0; 6]);
    let paid = w.bytes();
    let reply = w.send(retry.clone());
    assert_eq!(reply["response_type"], "state");
    assert_eq!(reply["receipt"]["prompt_seq"], retry["prompt_seq"]);
    assert_eq!(reply["receipt"]["action_index"], retry["action_index"]);
    assert_eq!(w.bytes(), paid);
}
fn safe(value: &Value) {
    if let Some(row) = value.as_object() {
        for forbidden in [
            "arena_id",
            "card_db_id",
            "zone_change_count",
            "original_index",
            "event_history",
            "private_world",
            "executable_binding",
        ] {
            assert!(!row.contains_key(forbidden), "{forbidden}");
        }
        for child in row.values() {
            safe(child)
        }
    } else if let Some(rows) = value.as_array() {
        for child in rows {
            safe(child)
        }
    }
}
#[test]
fn v7_paid_both_printings_complete_human_facts_counters_native_parity_and_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut world = native::ready(actor);
        let alesha = put(&mut world, actor, "Alesha, Who Laughs at Fate", Zone::Hand);
        let cat = put(&mut world, actor, "Nine-Lives Familiar", Zone::Hand);
        let mut wire = Wire::fixture(world);
        assert_eq!(
            wire.menu()["state"]["next_five"]["meanings"]
                .as_array()
                .unwrap()
                .len(),
            5
        );
        assert_eq!(
            wire.menu()["state"]["graveyard_return"]["meanings"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        cast(&mut wire, actor, alesha);
        wire.drain();
        cast(&mut wire, actor, cat);
        wire.drain();
        assert_eq!(wire.state().objects.get(cat).counters.revival, 8);
        let rows = &wire.menu()["state"]["graveyard_return"]["battlefield_counters"][actor.index()];
        assert!(rows
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["card"]["name"] == "Nine-Lives Familiar" && r["counters"]["revival"] == 8));
        assert!(rows
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["counters"].as_object().unwrap().len() == 16));
        let saved = wire.snapshot();
        let before = wire.bytes();
        wire.restore(saved);
        assert_eq!(wire.bytes(), before);
        safe(&wire.last);
    }
}
#[test]
fn v7_paid_raid_exact_target_resolution_may_forgery_and_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for accept in [false, true] {
            let (world, _, target) = native::raid_ready(actor);
            let mut wire = Wire::fixture(world);
            assert!(wire.menu()["action_meanings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["selected_graveyard_target"].is_object()));
            wire.native_step(|a|matches!(a,ActionSemanticV1::ChooseTarget{target:TargetRefV1::Object{object},..}if object.arena_id==target.0));
            for _ in 0..40 {
                if wire
                    .server
                    .pending
                    .as_ref()
                    .unwrap()
                    .decision
                    .actions()
                    .iter()
                    .any(|a| matches!(a, ActionSemanticV1::ChooseEffectBoolean { .. }))
                {
                    break;
                }
                wire.quiet();
            }
            let saved = wire.snapshot();
            let stale = wire.request(|l| l.starts_with("Accept returning"));
            let before = wire.bytes();
            wire.server
                .pending
                .as_mut()
                .unwrap()
                .visible
                .action_meanings[0]
                .programs
                .clear();
            wire.reject_unchanged(stale.clone(), "changed_safe_prompt");
            wire.server.pending.as_mut().unwrap().indexes.swap(0, 1);
            let forged = wire.request(|l| l.starts_with("Accept returning"));
            wire.reject_unchanged(forged, "changed_safe_prompt");
            assert_eq!(wire.bytes(), before);
            wire.native_step(
                |a| matches!(a,ActionSemanticV1::ChooseEffectBoolean{value,..}if *value==accept),
            );
            wire.drain();
            assert_eq!(
                wire.state().objects.get(target).zone,
                if accept {
                    Zone::Battlefield
                } else {
                    Zone::Graveyard
                }
            );
            let outcome = wire.bytes();
            wire.restore(saved);
            wire.reject_unchanged(stale, "stale_decision");
            wire.native_step(
                |a| matches!(a,ActionSemanticV1::ChooseEffectBoolean{value,..}if *value==accept),
            );
            wire.drain();
            assert_eq!(wire.bytes(), outcome);
            safe(&wire.last);
        }
    }
}
#[test]
fn v7_paid_familiar_death_schedule_delayed_return_replays_exact_native_world_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut world = native::ready(actor);
        let cat = native::paid(&mut world, actor, "Nine-Lives Familiar", None);
        let breath = put(&mut world, actor, "Breath Weapon", Zone::Hand);
        let mut wire = Wire::fixture(world);
        cast(&mut wire, actor, breath);
        wire.drain();
        let schedule = &wire.menu()["state"]["graveyard_return"]["nine_lives_schedules"][0];
        assert_eq!(schedule["departure_revival"], 8);
        assert_eq!(schedule["return_base_before_replacements"], 7);
        assert_eq!(schedule["next_future_any_player_end_step"], true);
        assert!(schedule["exact_graveyard_card_now"].is_object());
        let saved = wire.snapshot();
        let mut outcome = None;
        for replay in [false, true] {
            if replay {
                wire.restore(saved)
            }
            for _ in 0..300 {
                if wire.state().objects.get(cat).zone == Zone::Battlefield {
                    break;
                }
                wire.quiet()
            }
            assert_eq!(wire.state().objects.get(cat).zone, Zone::Battlefield);
            assert_eq!(wire.state().objects.get(cat).counters.revival, 7);
            if let Some(prior) = &outcome {
                assert_eq!(&wire.bytes(), prior)
            } else {
                outcome = Some(wire.bytes())
            }
            safe(&wire.last);
        }
    }
}
#[test]
fn v7_paid_independent_hidden_worlds_have_equal_public_prompt_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut prompts = vec![];
        for secret in ["Forest", "Island"] {
            let mut world = native::ready(actor);
            put(&mut world, actor.opponent(), secret, Zone::Hand);
            native::paid(&mut world, actor, "Nine-Lives Familiar", None);
            let wire = Wire::fixture(world);
            safe(&wire.last);
            prompts.push(wire.menu().clone());
        }
        assert_eq!(prompts[0], prompts[1]);
    }
}

#[test]
fn v7_paid_raid_original_current_and_departure_lki_remain_distinct_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut world, alesha, target) = native::raid_ready(actor);
        let current = Wire::fixture(world.clone());
        let raid = current.menu()["state"]["graveyard_return"]["triggers"]
            .as_array()
            .unwrap()
            .iter()
            .find_map(|r| r["program"].get("AleshaRaid"))
            .unwrap();
        assert_eq!(raid["power"]["amount"], 3);
        assert_eq!(raid["power"]["source"], "CurrentOriginalIncarnation");
        crate::engine::step(
            &mut world,
            crate::engine::Action::ChooseTarget(crate::state::Target::Object(target)),
        )
        .unwrap();
        let removal = put(&mut world, actor.opponent(), "Abrade", Zone::Hand);
        native::announce(&mut world, actor.opponent(), removal, Some(alesha));
        for _ in 0..12 {
            let decision = crate::engine::advance_until_decision(&mut world);
            if world.objects.get(alesha).zone == Zone::Graveyard {
                break;
            }
            native::quiet(&mut world, decision);
        }
        assert_eq!(world.objects.get(alesha).zone, Zone::Graveyard);
        let mut departed = Wire::fixture(world);
        let row = departed.menu()["state"]["graveyard_return"]["triggers"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["program"].get("AleshaRaid").is_some())
            .unwrap();
        assert_eq!(row["program"]["AleshaRaid"]["power"]["amount"], 3);
        assert_eq!(
            row["program"]["AleshaRaid"]["power"]["source"],
            "ExactDepartureLki"
        );
        assert!(row["source"]["current_original_source"].is_null());
        assert_eq!(row["targets"][0]["legal_now"], true);
        assert_eq!(
            departed.menu()["state"]["graveyard_return"]["alesha_departures"][0]["power"],
            3
        );
        let saved = departed.snapshot();
        let before = departed.bytes();
        departed.restore(saved);
        assert_eq!(departed.bytes(), before);
        safe(&departed.last);
    }
}
#[test]
fn v7_strict_header_no_state_injection_snapshot_owner_reset_and_partial_deck_refusal() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut world = native::ready(actor);
        native::paid(&mut world, actor, "Nine-Lives Familiar", None);
        let mut wire = Wire::fixture(world);
        let before = wire.bytes();
        for line in [
            r#"{"request_type":"current","schema_version":6,"request_id":"old"}"#,
            r#"{"request_type":"current","schema_version":7,"request_id":"x","world":{}}"#,
            r#"{"request_type":"current","schema_version":7,"request_id":"x","request_id":"y"}"#,
        ] {
            let reply: Value = serde_json::from_str(&wire.server.handle_line(line)).unwrap();
            assert_eq!(reply["response_type"], "error");
            assert_eq!(wire.bytes(), before);
        }
        let saved = wire.snapshot();
        let stale = wire.request(|_| true);
        wire.restore(saved);
        wire.reject_unchanged(stale, "stale_decision");
        let mut other = Wire::fixture(native::ready(actor));
        other.reject_unchanged(
            json!({"request_type":"restore","snapshot_id":saved}),
            "unknown_snapshot",
        );
        let foreign = other.snapshot();
        let foreign_saved = other.server.snapshots.remove(&foreign).unwrap();
        wire.server.snapshots.insert(foreign, foreign_saved);
        wire.reject_unchanged(
            json!({"request_type":"restore","snapshot_id":foreign}),
            "foreign_snapshot",
        );
        let request = json!({"request_type":"reset","expected_card_db_hash":KERNEL_CARDDB_HASH,"episode_id":9,"env_seed":1,
            "starting_player":PlayerSeatV1::from(actor),"max_physical_decisions":4096,"max_policy_steps":4096,
            "decks":[{"cards":[{"name":"Alesha, Who Laughs at Fate","count":40}]},{"cards":[{"name":"Forest","count":40}]}]});
        wire.reject_unchanged(request, "unsupported_deck");
        assert_eq!(wire.bytes(), before);
        safe(&wire.last);
    }
    let mut predecessor = crate::limited_next_five_session_v6::LimitedNextFiveJsonlServerV6::new();
    assert!(predecessor
        .handle_line(r#"{"request_type":"current","schema_version":7,"request_id":"v7"}"#)
        .contains("invalid_header"));
    for actor in [PlayerSeatV1::P0, PlayerSeatV1::P1] {
        let mut wire = Wire::new();
        let reset = json!({"request_type":"reset", "expected_card_db_hash":KERNEL_CARDDB_HASH,
            "episode_id":7, "env_seed":123, "starting_player":actor,
            "decks":[{"cards":[{"name":"Forest","count":40}]},{"cards":[{"name":"Island","count":40}]}],
            "max_physical_decisions":10000, "max_policy_steps":10000});
        wire.send(reset.clone());
        assert_eq!(wire.menu()["actor"], json!(actor));
        assert!(wire.menu()["london_mulligans"].is_object());
        let saved = wire.snapshot();
        let stale = wire.request(|label| label.starts_with("Keep this hand"));
        wire.restore(saved);
        wire.reject_unchanged(stale, "stale_decision");
        wire.send(reset);
        wire.reject_unchanged(
            json!({"request_type":"restore","snapshot_id":saved}),
            "unknown_snapshot",
        );
    }
}

struct Wire {
    server: LimitedGraveyardReturnJsonlServerV7,
    serial: u64,
    last: Value,
}
impl Wire {
    fn new() -> Self {
        Self {
            server: LimitedGraveyardReturnJsonlServerV7::new(),
            serial: 0,
            last: Value::Null,
        }
    }
    fn fixture(state: GameState) -> Self {
        let mut wire = Self::new();
        wire.server.active = Some(Active {
            core: GraveyardReturnSessionV61::from_session(
                crate::rl_session::FastActorSessionV1::from_graveyard_return_v61_fixture_state(
                    state,
                ),
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
        row.insert("schema_version".into(), json!(7));
        let line = serde_json::to_string(&request).unwrap();
        self.last = serde_json::from_str(&self.server.handle_line(&line)).unwrap();
        assert_eq!(self.last["protocol"], PROTOCOL);
        assert_eq!(self.last["schema_version"], 7);
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
            .unwrap_or_else(|| panic!("actual schema-7 action absent: {}", menu["actions"]));
        json!({"request_type": "step", "episode_id": self.last["state"]["episode_id"],
            "authority_generation": self.last["state"]["authority_generation"],
            "actor": menu["actor"], "prompt_seq": menu["prompt_seq"], "action_index": action["action_index"]})
    }
    fn state(&self) -> &GameState {
        self.server
            .active
            .as_ref()
            .unwrap()
            .core
            .fixture_state_v61()
    }
    fn bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self.state()).unwrap()
    }
    fn native_step(&mut self, choose: impl Fn(&ActionSemanticV1) -> bool) {
        let bound = self
            .server
            .pending
            .as_ref()
            .expect("actual executable V7 menu");
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
            .expect("original native index retained in V7 prompt");
        let original = bound
            .decision
            .fixture_original_index_v61(native_index as u32)
            .unwrap();
        let expected = bound.decision.expected();
        let mut reference = self
            .server
            .active
            .as_ref()
            .unwrap()
            .core
            .fixture_native_v61();
        let response = reference
            .step(expected.episode_id, expected.step, original)
            .unwrap();
        let request = json!({"request_type":"step",
            "episode_id":self.last["state"]["episode_id"],
            "authority_generation":self.last["state"]["authority_generation"],
            "actor":self.menu()["actor"],"prompt_seq":self.menu()["prompt_seq"],
            "action_index":visible_index});
        let reply = self.send(request);
        assert_eq!(reply["response_type"], "state", "{reply}");
        assert_eq!(reply["receipt"]["action_index"], visible_index);
        assert_eq!(self.state(), reference.graveyard_return_fixture_state());
        assert_eq!(
            self.server.active.as_ref().unwrap().core.response(),
            response
        );
        assert_eq!(
            self.server
                .active
                .as_ref()
                .unwrap()
                .core
                .fixture_counts_v61(),
            (
                reference.policy_step_count(),
                reference.physical_decision_count()
            )
        );
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
    fn quiet(&mut self) {
        self.native_step(|a| {
            matches!(
                a,
                ActionSemanticV1::Pass { .. }
                    | ActionSemanticV1::OrderTriggers { .. }
                    | ActionSemanticV1::ChooseTriggerOrderNext { .. }
                    | ActionSemanticV1::ChooseAttackerInclusion { include: false, .. }
                    | ActionSemanticV1::ChooseBlockerInclusion { include: false, .. }
                    | ActionSemanticV1::FinishEffectSelection { .. }
            )
        });
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
            self.quiet();
        }
        panic!("actual schema-7 paid operation did not finish");
    }
}
