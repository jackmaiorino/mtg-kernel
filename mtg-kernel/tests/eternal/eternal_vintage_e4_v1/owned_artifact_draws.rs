use super::*;
use mtg_kernel::event::CommittedEvent;
use mtg_kernel::state::{ObjectLinkV4, StackItemKind};

fn artifact(state: &mut GameState, drawer: PlayerId, name: &str, cost: u8) -> ObjectId {
    let object = put(state, drawer, name, Zone::Hand);
    pool(state, drawer, &[], cost);
    cast(state, object, &[]);
    settled(state);
    assert_eq!(state.objects.get(object).zone, Zone::Battlefield);
    assert_eq!(state.players[drawer.index()].mana_pool, [0; 6]);
    object
}

fn paid_tap(state: &mut GameState, source: ObjectId, index: u8) {
    activate(state, source, index, &[]);
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    assert!(state.objects.get(source).tapped);
    assert!(state.stack.last().is_some_and(|item| {
        item.kind == StackItemKind::ActivatedAbility && item.source == source
    }));
}

fn bounce_waiting_artifact(state: &mut GameState, drawer: PlayerId, source: ObjectId) {
    let opponent = drawer.opponent();
    if state.priority_player != opponent {
        act(state, Action::Pass);
    }
    let bounce = put(state, opponent, "Echoing Truth", Zone::Hand);
    pool(state, opponent, &[ManaColor::U], 1);
    cast(state, bounce, &[Target::Object(source)]);
    pass_until(state, |state| {
        state.objects.get(source).zone == Zone::Hand
            && state.stack.last().is_some_and(|item| item.source == source)
    });
    assert_eq!(state.players[opponent.index()].mana_pool, [0; 6]);
}

fn owned_draw_records(state: &GameState, start: usize) -> usize {
    state.engine.event_history[start..]
        .iter()
        .filter(|event| matches!(event, CommittedEvent::EternalOwnedDrawV1 { .. }))
        .count()
}

fn top_return_position(state: &GameState, top: ObjectId, start: usize) -> Option<usize> {
    state.engine.event_history[start..].iter().position(|event| {
        matches!(
            event,
            CommittedEvent::ZoneChange { object, from: Zone::Battlefield, to: Zone::Library, .. }
            if *object == top
        )
    })
}

#[test]
fn paid_top_draws_then_returns_and_offers_dredge_or_hullbreacher_both_seats() {
    for drawer in [P0, P1] {
        let opponent = drawer.opponent();
        let mut base = game(&["Island"; 8]);
        base.active_player = drawer;
        base.priority_player = drawer;
        let top = artifact(&mut base, drawer, "Sensei's Divining Top", 1);
        let library = base.players[drawer.index()].library.clone();
        let mut ordinary = base.clone();
        let start = ordinary.engine.event_history.len();
        paid_tap(&mut ordinary, top, 1);
        settled(&mut ordinary);
        let drawn = ordinary.engine.event_history[start..]
            .iter()
            .position(|event| {
                matches!(event, CommittedEvent::Draw { player, object: Some(card) }
                if *player == drawer && *card == library[0])
            })
            .unwrap();
        assert!(drawn < top_return_position(&ordinary, top, start).unwrap());
        assert_eq!(ordinary.players[drawer.index()].library[0], top);
        assert_eq!(owned_draw_records(&ordinary, start), 0);

        put(&mut base, opponent, "Hullbreacher", Zone::Battlefield);
        let thug = put(&mut base, drawer, "Golgari Thug", Zone::Graveyard);
        for departed in [false, true] {
            let mut state = base.clone();
            let start = state.engine.event_history.len();
            let generation = state.objects.get(top).zone_change_count;
            paid_tap(&mut state, top, 1);
            if departed {
                bounce_waiting_artifact(&mut state, drawer, top);
                assert!(state.objects.get(top).zone_change_count > generation);
            }
            let decision = settle(&mut state).expect("Top waits for the draw replacement");
            assert_draw_replacement_prompt(&decision, drawer, top, thug);
            assert_eq!(owned_draw_records(&state, start), 1);
            assert_eq!(top_return_position(&state, top, start), None);
            let saved = serde_json::to_vec(&state).unwrap();
            for dredge in [true, false] {
                let mut branch: GameState = serde_json::from_slice(&saved).unwrap();
                assert_eq!(next(&mut branch), decision);
                let answer = if dredge {
                    Action::ChooseEffectTarget(Target::Object(thug))
                } else {
                    Action::FinishEffectSelection
                };
                act(&mut branch, answer);
                let mut restored = restore_draw_answer(&branch);
                for state in [&mut branch, &mut restored] {
                    settled(state);
                    assert_eq!(owned_draw_records(state, start), 2);
                    assert_eq!(treasure_count(state, opponent), usize::from(!dredge));
                    assert_eq!(
                        state.objects.get(thug).zone,
                        if dredge { Zone::Hand } else { Zone::Graveyard }
                    );
                    let mut expected = library[if dredge { 4 } else { 0 }..].to_vec();
                    if departed {
                        assert_eq!(state.objects.get(top).zone, Zone::Hand);
                        assert_eq!(top_return_position(state, top, start), None);
                    } else {
                        expected.insert(0, top);
                        let completed = state.engine.event_history[start..]
                            .iter()
                            .rposition(|event| {
                                matches!(event, CommittedEvent::EternalOwnedDrawV1 { .. })
                            })
                            .unwrap();
                        assert!(completed < top_return_position(state, top, start).unwrap());
                    }
                    assert_eq!(state.players[drawer.index()].library, expected);
                }
                assert_eq!(branch.state_hash(), restored.state_hash());
            }
        }
    }
}

fn burden(state: &GameState, source: ObjectLinkV4) -> u32 {
    state
        .eternal_v1
        .as_ref()
        .and_then(|eternal| eternal.e3.as_ref())
        .and_then(|e3| {
            e3.burden_counters
                .iter()
                .find(|(ring, _)| *ring == source)
                .map(|(_, count)| *count)
        })
        .unwrap_or(0)
}

fn accumulate_ring_burden(state: &mut GameState, drawer: PlayerId, count: u32) -> ObjectId {
    let ring = artifact(state, drawer, "The One Ring", 4);
    for _ in 0..count {
        paid_tap(state, ring, 0);
        settled(state);
        let key = artifact(state, drawer, "Voltaic Key", 1);
        pool(state, drawer, &[], 1);
        activate(state, key, 0, &[Target::Object(ring)]);
        settled(state);
        assert!(!state.objects.get(ring).tapped);
    }
    ring
}

#[test]
fn paid_ring_freezes_live_increment_or_departed_burden_both_seats() {
    for drawer in [P0, P1] {
        let opponent = drawer.opponent();
        let mut base = game(&["Island"; 9]);
        base.active_player = drawer;
        base.priority_player = drawer;
        let ring = accumulate_ring_burden(&mut base, drawer, 1);
        let original = ObjectLinkV4 {
            object: ring,
            zone_change_count: base.objects.get(ring).zone_change_count,
        };
        assert_eq!(burden(&base, original), 1);
        assert_eq!(base.players[drawer.index()].library.len(), 8);
        let library = base.players[drawer.index()].library.clone();
        put(&mut base, opponent, "Hullbreacher", Zone::Battlefield);
        let thug = put(&mut base, drawer, "Golgari Thug", Zone::Graveyard);

        for departed in [false, true] {
            let mut state = base.clone();
            let start = state.engine.event_history.len();
            paid_tap(&mut state, ring, 0);
            if departed {
                bounce_waiting_artifact(&mut state, drawer, ring);
            }
            let decision = settle(&mut state).expect("Ring waits for the draw replacement");
            assert_draw_replacement_prompt(&decision, drawer, ring, thug);
            let total = if departed { 1 } else { 2 };
            assert_eq!(burden(&state, original), total);
            let saved = serde_json::to_value(&state).unwrap();
            let started_at = saved["eternal_owned_draw_progress_v1"]["started_at"]
                .as_u64()
                .unwrap() as usize;
            let mut forged = saved.clone();
            forged["engine"]["event_history"][started_at]["EternalOwnedDrawV1"]["journal"]
                ["Started"]["total"] = serde_json::json!(total + 1);
            let forged: GameState = serde_json::from_value(forged).unwrap();
            assert_eq!(
                mtg_kernel::effect::validate_pending_effect_choice(&forged).unwrap_err(),
                "Eternal draw changed its original count",
            );
            for dredge in [true, false] {
                let mut branch: GameState = serde_json::from_value(saved.clone()).unwrap();
                assert_eq!(next(&mut branch), decision);
                let answer = if dredge {
                    Action::ChooseEffectTarget(Target::Object(thug))
                } else {
                    Action::FinishEffectSelection
                };
                act(&mut branch, answer);
                let mut restored = restore_draw_answer(&branch);
                for state in [&mut branch, &mut restored] {
                    if !dredge && !departed {
                        let second = settle(state).expect("second Ring draw still offers dredge");
                        assert_draw_replacement_prompt(&second, drawer, ring, thug);
                        assert_eq!(treasure_count(state, opponent), 1);
                        act(state, Action::FinishEffectSelection);
                    }
                    settled(state);
                    assert_eq!(owned_draw_records(state, start), total as usize + 1);
                    assert_eq!(burden(state, original), total);
                    assert_eq!(
                        treasure_count(state, opponent),
                        total as usize - usize::from(dredge)
                    );
                    assert_eq!(
                        state.players[drawer.index()].library,
                        library[if dredge { 4 } else { 0 }..]
                    );
                    assert_eq!(
                        state.objects.get(ring).zone,
                        if departed {
                            Zone::Hand
                        } else {
                            Zone::Battlefield
                        }
                    );
                }
                assert_eq!(branch.state_hash(), restored.state_hash());
            }
        }
    }
}

fn cast_jace(state: &mut GameState, drawer: PlayerId) -> ObjectId {
    let jace = put(state, drawer, "Jace, Wielder of Mysteries", Zone::Hand);
    pool(state, drawer, &[ManaColor::U; 3], 1);
    cast(state, jace, &[]);
    settled(state);
    jace
}

#[test]
fn paid_artifact_draws_stop_after_jace_wins_both_seats() {
    for drawer in [P0, P1] {
        let opponent = drawer.opponent();
        let mut state = game(&[]);
        state.active_player = drawer;
        state.priority_player = drawer;
        let top = artifact(&mut state, drawer, "Sensei's Divining Top", 1);
        let jace = cast_jace(&mut state, drawer);
        let mut ordinary = state.clone();
        paid_tap(&mut ordinary, top, 1);
        assert!(
            matches!(settle(&mut ordinary), Some(Decision::GameOver { winner: Some(player) }) if player == drawer)
        );
        assert_eq!(ordinary.objects.get(top).zone, Zone::Battlefield);

        put(&mut state, opponent, "Hullbreacher", Zone::Battlefield);
        paid_tap(&mut state, top, 1);
        let prompt = settle(&mut state).expect("Jace and Hullbreacher compete for Top's draw");
        assert_draw_replacement_prompt(&prompt, drawer, top, jace);
        let mut decline = state.clone();
        act(&mut decline, Action::FinishEffectSelection);
        settled(&mut decline);
        assert_eq!(decline.players[drawer.index()].library, vec![top]);
        assert_eq!(treasure_count(&decline, opponent), 1);
        act(&mut state, Action::ChooseEffectTarget(Target::Object(jace)));
        let mut restored = restore_draw_answer(&state);
        for branch in [&mut state, &mut restored] {
            assert!(
                matches!(next(branch), Decision::GameOver { winner: Some(player) } if player == drawer)
            );
            assert_eq!(branch.objects.get(top).zone, Zone::Battlefield);
            assert!(branch.players[drawer.index()].library.is_empty());
            assert_eq!(treasure_count(branch, opponent), 0);
        }
        assert_eq!(state.state_hash(), restored.state_hash());

        let mut state = game(&["Island"; 3]);
        state.active_player = drawer;
        state.priority_player = drawer;
        let ring = accumulate_ring_burden(&mut state, drawer, 2);
        let link = ObjectLinkV4 {
            object: ring,
            zone_change_count: state.objects.get(ring).zone_change_count,
        };
        assert_eq!(burden(&state, link), 2);
        assert!(state.players[drawer.index()].library.is_empty());
        let jace = cast_jace(&mut state, drawer);
        let mut ordinary = state.clone();
        let start = ordinary.engine.event_history.len();
        paid_tap(&mut ordinary, ring, 0);
        assert!(
            matches!(settle(&mut ordinary), Some(Decision::GameOver { winner: Some(player) }) if player == drawer)
        );
        assert_eq!(burden(&ordinary, link), 3);
        assert_eq!(ordinary.engine.event_history[start..].iter().filter(|event| {
            matches!(event, CommittedEvent::Draw { player, object: None } if *player == drawer)
        }).count(), 1);
        assert_eq!(owned_draw_records(&ordinary, start), 0);

        put(&mut state, opponent, "Hullbreacher", Zone::Battlefield);
        let start = state.engine.event_history.len();
        paid_tap(&mut state, ring, 0);
        let first =
            settle(&mut state).expect("Jace and Hullbreacher compete for Ring's first draw");
        assert_draw_replacement_prompt(&first, drawer, ring, jace);
        act(&mut state, Action::FinishEffectSelection);
        let second = settle(&mut state).expect("Ring offers the second draw");
        assert_draw_replacement_prompt(&second, drawer, ring, jace);
        act(&mut state, Action::ChooseEffectTarget(Target::Object(jace)));
        let mut restored = restore_draw_answer(&state);
        for branch in [&mut state, &mut restored] {
            assert!(
                matches!(next(branch), Decision::GameOver { winner: Some(player) } if player == drawer)
            );
            assert_eq!(owned_draw_records(branch, start), 3);
            assert_eq!(burden(branch, link), 3);
            assert_eq!(treasure_count(branch, opponent), 1);
            assert!(serde_json::to_value(&*branch)
                .unwrap()
                .get("eternal_owned_draw_progress_v1")
                .is_none());
        }
        assert_eq!(state.state_hash(), restored.state_hash());
    }
}
