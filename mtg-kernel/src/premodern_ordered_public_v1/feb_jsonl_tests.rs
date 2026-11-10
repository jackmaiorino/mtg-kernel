use super::*;

fn request(server: &mut FebJsonlServerV1, id: &str, mut body: Value) -> Value {
    body["request_id"] = json!(id);
    body["schema_version"] = json!(SCHEMA_VERSION);
    serde_json::from_str(&server.handle_line(&body.to_string())).unwrap()
}

fn reset(actor: PlayerSeatV1) -> Value {
    json!({"request_type": "reset", "episode_id": 460, "seed": 20261108,
        "expected_profile": PROFILE_V1, "expected_card_db_hash": KERNEL_CARDDB_HASH,
        "starting_player": actor})
}

#[test]
fn feb_jsonl_retries_exact_actions_and_refuses_changed_or_stale_authority() {
    for actor in [PlayerSeatV1::P0, PlayerSeatV1::P1] {
        let mut server = FebJsonlServerV1::default();
        let initial = request(&mut server, "reset", reset(actor));
        assert_eq!(initial["state"]["prompt"]["actor"], json!(actor));
        assert_eq!(
            initial["state"]["human"]["scene"]["own_hand"]
                .as_array()
                .unwrap()
                .len(),
            7
        );
        assert_eq!(
            initial["state"]["human"]["scene"]["hand_counts"],
            json!([7, 7])
        );
        assert_eq!(request(&mut server, "reset", reset(actor)), initial);

        let saved = request(&mut server, "save", json!({"request_type": "snapshot"}));
        assert_eq!(saved["state"], initial["state"]);
        assert_eq!(
            request(&mut server, "save", json!({"request_type": "snapshot"})),
            saved
        );
        let restore = json!({"request_type": "restore", "snapshot_id": saved["snapshot_id"]});
        let step = json!({"request_type": "step", "prompt": initial["state"]["prompt"],
            "native_ordinal": 0, "submission": "human"});
        let mut wrong_actor = step.clone();
        wrong_actor["prompt"]["actor"] = json!(match actor {
            PlayerSeatV1::P0 => PlayerSeatV1::P1,
            PlayerSeatV1::P1 => PlayerSeatV1::P0,
        });
        assert_eq!(
            request(&mut server, "wrong-actor", wrong_actor)["code"],
            "stale_decision"
        );
        let mut wrong_ordinal = step.clone();
        wrong_ordinal["native_ordinal"] = json!(u32::MAX);
        assert_eq!(
            request(&mut server, "wrong-ordinal", wrong_ordinal)["code"],
            "invalid_action"
        );

        let current = json!({"request_type": "current"});
        assert_eq!(
            request(&mut server, "read", current.clone())["state"],
            initial["state"]
        );
        let accepted = request(&mut server, "step", step.clone());
        assert_eq!(accepted["state"]["prompt"]["prompt_seq"], 1);
        assert_ne!(accepted["state"]["prompt"]["actor"], json!(actor));
        assert_eq!(accepted["receipt"]["prompt"], initial["state"]["prompt"]);
        assert_eq!(request(&mut server, "step", step.clone()), accepted);
        let saved_later = request(&mut server, "later", json!({"request_type": "snapshot"}));
        assert_eq!(saved_later["state"], accepted["state"]);
        let mut changed = step.clone();
        changed["submission"] = json!("numeric");
        assert_eq!(
            request(&mut server, "changed", changed)["code"],
            "conflicting_retry"
        );
        assert_eq!(
            request(&mut server, "step", current.clone())["code"],
            "conflicting_retry"
        );
        let retry = request(&mut server, "retry", step.clone());
        assert_eq!(retry["receipt"], accepted["receipt"]);
        assert_eq!(retry["state"], accepted["state"]);

        let restored = request(&mut server, "restore", restore.clone());
        assert_ne!(
            restored["state"]["authority_generation"],
            initial["state"]["authority_generation"]
        );
        for field in ["numeric", "human"] {
            assert_eq!(restored["state"][field], initial["state"][field]);
        }
        assert_eq!(
            request(&mut server, "old-step", step.clone())["code"],
            "stale_decision"
        );
        let mut replay = step.clone();
        replay["prompt"] = restored["state"]["prompt"].clone();
        replay["submission"] = json!("numeric");
        let replay = request(&mut server, "replay", replay);
        for field in ["numeric", "human"] {
            assert_eq!(replay["state"][field], accepted["state"][field]);
        }
        let reset_again = request(&mut server, "reset-again", reset(actor));
        assert_eq!(
            request(&mut server, "foreign", restore)["code"],
            "unknown_snapshot"
        );
        assert_eq!(
            request(&mut server, "earlier", step)["code"],
            "stale_decision"
        );
        assert_eq!(
            request(&mut server, "still-current", current)["state"],
            reset_again["state"]
        );
    }
}

#[test]
fn feb_jsonl_rejects_foreign_profiles_decks_and_excess_snapshots() {
    let mut server = FebJsonlServerV1::default();
    let current = json!({"request_type": "current"});
    assert_eq!(
        request(&mut server, "empty", current.clone())["code"],
        "no_session"
    );
    let mut bad = reset(PlayerSeatV1::P0);
    bad["expected_card_db_hash"] = json!(KERNEL_CARDDB_HASH ^ 1);
    assert_eq!(
        request(&mut server, "bad-catalog", bad)["code"],
        "catalog_mismatch"
    );
    let mut bad = reset(PlayerSeatV1::P0);
    bad["expected_profile"] = json!("limited");
    assert_eq!(
        request(&mut server, "bad-profile", bad)["code"],
        "profile_mismatch"
    );
    let mut bad = reset(PlayerSeatV1::P0);
    bad["decks"] = json!([[], []]);
    assert_eq!(
        request(&mut server, "decks", bad)["code"],
        "invalid_request"
    );
    let duplicate: Value = serde_json::from_str(&server.handle_line(
        r#"{"request_type":"current","schema_version":1,"request_id":"a","request_id":"b"}"#,
    ))
    .unwrap();
    assert_eq!(duplicate["code"], "invalid_request");
    let opening = request(&mut server, "opening", reset(PlayerSeatV1::P0));
    let mut handles = std::collections::BTreeSet::new();
    for index in 1..=MAX_SNAPSHOTS {
        let snapshot = request(
            &mut server,
            &format!("save-{index}"),
            json!({"request_type": "snapshot"}),
        );
        let handle = snapshot["snapshot_id"].as_str().unwrap();
        assert_eq!(handle.len(), 32);
        assert!(handles.insert(handle.to_owned()));
    }
    assert_eq!(
        request(&mut server, "too-many", json!({"request_type": "snapshot"}))["code"],
        "snapshot_limit"
    );
    assert_eq!(
        request(&mut server, "still-current", current)["state"],
        opening["state"]
    );
    let mut other = FebJsonlServerV1::default();
    let _ = request(&mut other, "opening", reset(PlayerSeatV1::P0));
    let snapshot = request(&mut other, "save", json!({"request_type": "snapshot"}));
    assert!(!handles.contains(snapshot["snapshot_id"].as_str().unwrap()));
    let foreign = json!({"request_type": "restore", "snapshot_id": snapshot["snapshot_id"]});
    assert_eq!(
        request(&mut server, "foreign-server", foreign)["code"],
        "unknown_snapshot"
    );
    for raw in [
        "arena_id",
        "zone_change_count",
        "pending_effect",
        "continuation",
        "native_candidates",
        "owner_pointer",
        "known_hand_cards",
    ] {
        assert!(!opening.to_string().contains(raw), "{raw} crossed the wire");
    }
}
