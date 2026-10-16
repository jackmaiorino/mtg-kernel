use super::*;
use crate::current_mixed_policy_v1::{normalize, NumericTree, Source};
use crate::ids::PlayerId;
use serde_json::json;

fn actual_projection(actor: PlayerId) -> PublicObservationProjectionV5 {
    let state = crate::extra_turn_v1::tests::ready(actor);
    let before = serde_json::to_value(&state).unwrap();
    let projection = crate::rl::observe_policy_v5(
        &state,
        &crate::policy_surface_v5::PolicySurfaceV5::new(),
        actor,
        0,
        0,
        0,
        1,
    )
    .unwrap()
    .projection;
    assert_eq!(serde_json::to_value(&state).unwrap(), before);
    projection
}

fn expected(projection: &PublicObservationProjectionV5) -> Value {
    let mut value = serde_json::to_value(projection).unwrap();
    for seat in 0..2 {
        let dungeon = value["player_status"][seat]["dungeon"]
            .as_object_mut()
            .unwrap();
        let id = dungeon.remove("dungeon_id").unwrap();
        let room = dungeon.remove("room_id").unwrap();
        dungeon.insert("dungeon_kind".into(), id);
        dungeon.insert("room_kind".into(), room);
    }
    value
}

#[test]
fn actual_complete_projection_preserves_every_member_through_all_common_envelope_depths() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let projection = actual_projection(actor);
        assert!(projection.surface.player_status.iter().all(|status| {
            status.dungeon.dungeon_id.is_none()
                && status.dungeon.room_id.is_none()
                && status.dungeon.completed_dungeons.is_empty()
        }));
        for depth in 0..=3 {
            let mut observation = json!({"projection":projection});
            let mut wanted = json!({"projection":expected(&projection)});
            for _ in 0..depth {
                observation = json!({"common":observation,"other_ordered_operand":[19,2,31]});
                wanted = json!({"common":wanted,"other_ordered_operand":[19,2,31]});
            }
            let original = observation.clone();
            prepare(&mut observation).unwrap();
            assert_eq!(observation, wanted);
            for source in [
                Source::FakeV50,
                Source::RavenousV51,
                Source::ElendaV52,
                Source::PreparedV57,
            ] {
                normalize::encode(source, original.clone(), vec![json!({"kind":"Pass"})]).unwrap();
            }
        }
    }
}

#[test]
fn typed_dungeon_kind_width_presence_and_completed_order_survive_numeric_encoding() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut projection = actual_projection(actor);
        let mut trees = Vec::new();
        // Typed operand-width fixtures, not claims of admitted dungeon gameplay.
        for (dungeon, room, completed) in [
            (None, None, vec![]),
            (Some(0), Some(u16::MAX), vec![0, 17, u16::MAX]),
            (Some(u16::MAX), Some(0), vec![0, 17, u16::MAX]),
            (Some(u16::MAX), Some(0), vec![u16::MAX, 17, 0]),
        ] {
            for status in &mut projection.surface.player_status {
                status.dungeon.dungeon_id = dungeon;
                status.dungeon.room_id = room;
                status.dungeon.completed_dungeons = completed.clone();
            }
            let mut observation = json!({"projection":projection});
            let packet = normalize::encode(
                Source::FakeV50,
                observation.clone(),
                vec![json!({"kind":"Pass"})],
            )
            .unwrap();
            prepare(&mut observation).unwrap();
            assert_eq!(observation["projection"], expected(&projection));
            for seat in 0..2 {
                let value = &observation["projection"]["player_status"][seat]["dungeon"];
                assert_eq!(
                    value,
                    &json!({"dungeon_kind":dungeon,"room_kind":room,"completed_dungeons":completed})
                );
                let tree = NumericTree::encode(value).unwrap();
                assert!(tree.tensors().iter().flatten().all(|v| v.is_finite()));
                for kind in [dungeon, room].into_iter().flatten() {
                    assert!(tree
                        .rows()
                        .iter()
                        .any(|row| row[0] == 2 && row[1..5] == [kind, 0, 0, 0]));
                }
            }
            trees.push(packet.state().clone());
        }
        for (index, tree) in trees.iter().enumerate() {
            assert!(trees[..index].iter().all(|prior| prior != tree));
        }
    }
}

#[test]
fn partial_unknown_and_forged_projection_shapes_refuse_without_raw_identity_exceptions() {
    let original = json!({"projection":actual_projection(PlayerId::P0)});
    for change in 0..9 {
        let mut forged = original.clone();
        match change {
            0 => forged["projection"]["unknown_raw_id"] = json!(23),
            1 => forged["projection"]["player_status"][0]["unknown"] = json!(true),
            2 => forged["projection"]["player_status"][0]["dungeon"]["unknown_raw_id"] = json!(23),
            3 => forged["projection"]["player_status"][0]["dungeon"]["dungeon_id"] = json!(65536),
            4 => forged["projection"]["player_status"][0]["dungeon"]["room_id"] = json!(-1),
            5 => {
                forged["projection"]["player_status"][0]["dungeon"]["completed_dungeons"] =
                    json!([1, 65536])
            }
            6 => {
                forged["projection"]
                    .as_object_mut()
                    .unwrap()
                    .remove("stack");
            }
            7 => {
                forged["projection"]["player_status"] =
                    json!([original["projection"]["player_status"][0]])
            }
            _ => {
                forged["projection"] = json!({"player_status":[
                    {"dungeon":{"dungeon_id":null,"room_id":null,"completed_dungeons":[]}},
                    {"dungeon":{"dungeon_id":null,"room_id":null,"completed_dungeons":[]}}
                ]})
            }
        }
        let before = forged.clone();
        assert!(prepare(&mut forged).is_err());
        assert_eq!(forged, before);
        assert!(normalize::encode(Source::FakeV50, forged, vec![json!({"kind":"Pass"})]).is_err());
    }
    for key in [
        "dungeon_id",
        "room_id",
        "unknown_raw_id",
        "unknown_generation",
    ] {
        for raw in [
            json!({(key):null}),
            json!({(key):23}),
            json!({"outside":{(key):23}}),
        ] {
            assert!(normalize::encode(Source::FakeV50, raw, vec![json!({"kind":"Pass"})]).is_err());
        }
        let mut elsewhere = original.clone();
        elsewhere["unrelated"] = json!({(key):23});
        assert!(
            normalize::encode(Source::FakeV50, elsewhere, vec![json!({"kind":"Pass"})]).is_err()
        );
        assert!(
            normalize::encode(Source::FakeV50, original.clone(), vec![json!({(key):23})]).is_err()
        );
    }
    assert!(normalize::encode(
        Source::FakeV50,
        json!({"unrelated":{"projection":original["projection"]}}),
        vec![json!({"kind":"Pass"})]
    )
    .is_err());
}
