//! Genuine paid native cases. Initial libraries and battlefield setup are
//! declared before play. Explicit mutations below are negative/boundary
//! primitives, never substitutes for casting or native entry.
use super::*;
use crate::{
    engine::{self, Action, Decision},
    policy_observation_v6::tests::put,
    state::Step,
};

fn id(name: &str) -> u16 {
    card_def::card_id_by_name(name).unwrap()
}
fn ready(player: PlayerId, library: &[&str], lands: usize, hidden: &str) -> GameState {
    let own = library.iter().map(|n| id(n)).collect::<Vec<_>>();
    let other = vec![id(hidden); 32];
    let (a, b) = if player == PlayerId::P0 {
        (&own, &other)
    } else {
        (&other, &own)
    };
    let mut s = GameState::new_from_libraries_with_starting_player_v1(
        a,
        b,
        crate::rl::card_name,
        110_487,
        player,
    );
    s.step = Step::Main1;
    for _ in 0..lands {
        put(&mut s, player, "Forest", Zone::Battlefield);
    }
    for _ in 0..8 {
        put(&mut s, player.opponent(), "Island", Zone::Battlefield);
    }
    s
}
fn restore(s: &GameState) -> GameState {
    let copy: GameState = serde_json::from_slice(&serde_json::to_vec(s).unwrap()).unwrap();
    assert_eq!(s, &copy);
    assert_eq!(s.state_hash(), copy.state_hash());
    copy
}
fn next(s: &mut GameState) -> Decision {
    let d = engine::advance_until_decision(s);
    assert!(s.engine.halted.is_none(), "{:?}", s.engine.halted);
    assert!(!matches!(d, Decision::Halted { .. }), "{d:?}");
    d
}
fn pass(s: &mut GameState, d: Decision) {
    let a = match d {
        Decision::CastSpellOrPass { .. } => Action::Pass,
        Decision::OrderTriggers { pending, .. } => {
            Action::OrderTriggers((0..pending.len()).collect())
        }
        _ => panic!("unexpected native Wave decision {d:?}"),
    };
    engine::step(s, a).unwrap();
}
fn priority(s: &mut GameState, p: PlayerId) {
    for _ in 0..32 {
        let d = next(s);
        if matches!(d,Decision::CastSpellOrPass{player,..}if player==p) {
            return;
        }
        pass(s, d);
    }
    panic!("priority absent")
}
fn tapped(s: &GameState, p: PlayerId) -> usize {
    s.players[p.index()]
        .battlefield
        .iter()
        .filter(|o| s.objects.get(**o).tapped)
        .count()
}
fn paid(s: &mut GameState, p: PlayerId, name: &str, x: Option<u16>, cost: usize) -> ObjectId {
    // Multi-spell scenes declare all their hand operands before play. Other
    // scenes install this initial hand card before their first native action.
    let existing = s.players[p.index()]
        .hand
        .iter()
        .copied()
        .find(|o| s.objects.get(*o).card_def == id(name));
    let spell = existing.unwrap_or_else(|| put(s, p, name, Zone::Hand));
    priority(s, p);
    let before = tapped(s, p);
    assert!(
        matches!(next(s),Decision::CastSpellOrPass{castable_spells,..}if castable_spells.contains(&spell))
    );
    engine::step(s, Action::CastSpell(spell)).unwrap();
    let mut chose = false;
    for _ in 0..16 {
        match next(s) {
            Decision::ChooseEffectOption {
                player,
                source,
                option_count,
            } => {
                let x = x.unwrap();
                assert!(!chose);
                assert_eq!((player, source), (p, spell));
                assert!(x < option_count);
                engine::step(s, Action::ChooseEffectOption(x)).unwrap();
                chose = true;
            }
            Decision::ChooseFullRangeCastXValueV59 {
                player,
                spell: source,
            } => {
                assert!(!chose);
                assert_eq!((player, source), (p, spell));
                engine::step(s, Action::ChooseEffectOption(x.unwrap())).unwrap();
                chose = true;
            }
            Decision::CastSpellOrPass { player, .. } => {
                assert_eq!(player, p);
                assert!(s.engine.pending_cast.is_none());
                assert_eq!(tapped(s, p) - before, cost);
                assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
                let row = s.stack.iter().find(|r| r.source == spell).unwrap();
                assert_eq!(row.kind, crate::state::StackItemKind::Spell);
                assert_eq!(row.controller, p);
                assert_eq!(row.v4.x_value, x.unwrap_or(0));
                assert!(row.v4.source_contract.is_some());
                assert!(s.engine.event_history.iter().any(|e|matches!(e,event::CommittedEvent::SpellCast{spell:source,..}if *source==spell)));
                return spell;
            }
            d => panic!("paid spell did not publish: {d:?}"),
        }
    }
    panic!("paid spell never published")
}
fn subset(s: &mut GameState, p: PlayerId) -> Vec<ObjectId> {
    for _ in 0..32 {
        let d = next(s);
        if let Decision::ChooseEffectTargets {
            player,
            min_targets,
            max_targets,
            legal_targets,
            can_finish,
            ..
        } = d
        {
            let pending = s.engine.pending_effect.as_ref().unwrap();
            assert!(matches!(
                pending.choice,
                Some(PendingEffectChoice::SelectTargets {
                    purpose: EffectTargetSelectionPurpose::GenesisWaveCardsV1 {
                        choice: ChoiceV1 {
                            phase: PhaseV1::Subset,
                            ..
                        }
                    },
                    ..
                })
            ));
            assert_eq!(player, p);
            assert_eq!(min_targets, 0);
            assert_eq!(usize::from(max_targets), legal_targets.len());
            assert!(can_finish);
            validate_pending_effect_choice(s).unwrap();
            return legal_targets
                .into_iter()
                .map(|t| {
                    if let Target::Object(o) = t {
                        o
                    } else {
                        panic!()
                    }
                })
                .collect();
        }
        pass(s, d);
    }
    panic!("Wave subset absent")
}
fn choose(s: &mut GameState, objects: &[ObjectId]) {
    for o in objects {
        assert!(
            matches!(next(s),Decision::ChooseEffectTargets{legal_targets,..}if legal_targets.contains(&Target::Object(*o)))
        );
        engine::step(s, Action::ChooseEffectTarget(Target::Object(*o))).unwrap();
    }
    if s.engine.pending_effect.as_ref().unwrap().choice.is_some() {
        engine::step(s, Action::FinishEffectSelection).unwrap();
    }
    validate_pending_effect_choice(s).unwrap();
}
fn finish(s: &mut GameState, wave: ObjectId) {
    for _ in 0..512 {
        let d = next(s);
        if s.objects.get(wave).zone == Zone::Graveyard && s.engine.pending_effect.is_none() {
            return;
        }
        match d {
            Decision::ChooseEffectTargets { legal_targets, .. } => {
                assert!(matches!(
                    s.engine.pending_effect.as_ref().unwrap().choice,
                    Some(PendingEffectChoice::SelectTargets {
                        purpose: EffectTargetSelectionPurpose::GenesisWaveCardsV1 { .. },
                        ..
                    })
                ));
                if let Some(target) = legal_targets.last() {
                    engine::step(s, Action::ChooseEffectTarget(*target)).unwrap();
                } else {
                    engine::step(s, Action::FinishEffectSelection).unwrap();
                }
            }
            Decision::ChooseEntryColorV36 { .. } | Decision::ChooseEntryCreatureTypeV1 { .. } => {
                engine::step(s, Action::ChooseEffectOption(0)).unwrap()
            }
            d => pass(s, d),
        }
    }
    panic!("Wave did not finish")
}
fn players() -> [PlayerId; 2] {
    [PlayerId::P0, PlayerId::P1]
}

#[test]
fn genuine_paid_wave_selects_all_permanent_types_by_printed_mana_value_both_seats() {
    for p in players() {
        let mut s = ready(
            p,
            &[
                "Forest",
                "Llanowar Elves",
                "Ravenous Amulet",
                "Midnight Snack",
                "Ajani, Caller of the Pride",
                "Giant Growth",
            ],
            12,
            "Island",
        );
        let original = s.players[p.index()].library.clone();
        let wave = paid(&mut s, p, "Genesis Wave", Some(6), 9);
        let candidates = subset(&mut s, p);
        assert_eq!(candidates, original[..5]);
        let mut replay = restore(&s);
        choose(&mut s, &candidates);
        choose(&mut replay, &candidates);
        finish(&mut s, wave);
        finish(&mut replay, wave);
        assert_eq!(s, replay);
        for o in &original[..5] {
            assert_eq!(s.objects.get(*o).zone, Zone::Battlefield);
            assert_eq!(s.objects.get(*o).controller, p);
        }
        assert_eq!(s.objects.get(original[5]).zone, Zone::Graveyard);
        assert!(crate::rl::observe_v1(&s, p, 0).is_err());
        assert!(definition(id("Genesis Wave")));
    }
}
#[test]
fn genuine_paid_wave_zero_x_zero_selection_and_empty_or_short_library_both_seats() {
    for p in players() {
        for (library, x) in [
            (vec![], 0),
            (vec!["Forest", "Giant Growth"], 0),
            (vec!["Forest"], 5),
            (vec!["Giant Growth"], 4),
        ] {
            let mut s = ready(p, &library, 12, "Island");
            let original = s.players[p.index()].library.clone();
            let wave = paid(&mut s, p, "Genesis Wave", Some(x), usize::from(x) + 3);
            subset(&mut s, p);
            choose(&mut s, &[]);
            s = restore(&s);
            finish(&mut s, wave);
            for o in original {
                assert_eq!(
                    s.objects.get(o).zone,
                    if x == 0 {
                        Zone::Library
                    } else {
                        Zone::Graveyard
                    }
                );
            }
        }
    }
}
#[test]
fn genuine_paid_wave_subset_and_remaining_public_graveyard_order_restore_both_seats() {
    for p in players() {
        let mut s = ready(
            p,
            &["Forest", "Llanowar Elves", "Giant Growth", "Island"],
            12,
            "Swamp",
        );
        let original = s.players[p.index()].library.clone();
        let wave = paid(&mut s, p, "Genesis Wave", Some(3), 6);
        let candidates = subset(&mut s, p);
        assert_eq!(candidates, original[..2]);
        choose(&mut s, &[original[1]]);
        assert!(matches!(
            next(&mut s),
            Decision::ChooseEffectTargets {
                min_targets: 2,
                max_targets: 2,
                ..
            }
        ));
        assert_eq!(s.objects.get(original[1]).zone, Zone::Battlefield);
        assert_eq!(s.objects.get(original[0]).zone, Zone::Library);
        assert_eq!(s.objects.get(original[2]).zone, Zone::Library);
        // Mutable-checkpoint boundary primitive: changing both duplicated
        // choice records cannot rewrite the independently committed BF batch.
        let mut bad = restore(&s);
        if let Some(PendingEffectChoice::SelectTargets {
            purpose: EffectTargetSelectionPurpose::GenesisWaveCardsV1 { choice },
            ..
        }) = &mut bad.engine.pending_effect.as_mut().unwrap().choice
        {
            choice.selected = vec![choice.prefix[0]];
        } else {
            panic!("Genesis Wave remainder checkpoint absent");
        }
        let event::CommittedEvent::GenesisWaveProgressV1 { choice, .. } = bad
            .engine
            .event_history
            .iter_mut()
            .rev()
            .find(|e| matches!(e, event::CommittedEvent::GenesisWaveProgressV1 { .. }))
            .unwrap()
        else {
            unreachable!()
        };
        choice.selected = vec![choice.prefix[0]];
        let frozen = bad.clone();
        assert!(validate_pending_effect_choice(&bad).is_err());
        assert!(engine::step(
            &mut bad,
            Action::ChooseEffectTarget(Target::Object(original[2]))
        )
        .is_err());
        assert_eq!(bad, frozen);
        s = restore(&s);
        finish(&mut s, wave);
        assert_eq!(s.objects.get(original[1]).zone, Zone::Battlefield);
        assert_eq!(s.players[p.index()].library, vec![original[3]]);
        assert_eq!(
            s.players[p.index()]
                .graveyard
                .iter()
                .filter(|o| original[..3].contains(o))
                .copied()
                .collect::<Vec<_>>(),
            vec![original[2], original[0]]
        );
    }
}
#[test]
fn genuine_paid_wave_aura_uses_preexisting_untargeted_host_and_unattachable_aura_goes_graveyard_both_seats(
) {
    for p in players() {
        for host_exists in [false, true] {
            let mut s = ready(p, &["Witness Protection", "Llanowar Elves"], 10, "Island");
            let host = host_exists.then(|| {
                put(
                    &mut s,
                    p.opponent(),
                    "Elenda, Saint of Dusk",
                    Zone::Battlefield,
                )
            });
            let original = s.players[p.index()].library.clone();
            let wave = paid(&mut s, p, "Genesis Wave", Some(2), 5);
            let candidates = subset(&mut s, p);
            assert_eq!(candidates, original);
            choose(&mut s, &candidates);
            if let Some(host) = host {
                let d = next(&mut s);
                assert!(
                    matches!(d,Decision::ChooseEffectTargets{min_targets:1,max_targets:1,legal_targets,..}if legal_targets.contains(&Target::Object(host))&&!legal_targets.contains(&Target::Object(original[1])))
                );
                s = restore(&s);
                engine::step(&mut s, Action::ChooseEffectTarget(Target::Object(host))).unwrap();
                s = restore(&s);
            }
            finish(&mut s, wave);
            assert_eq!(s.objects.get(original[1]).zone, Zone::Battlefield);
            assert_eq!(
                s.objects.get(original[0]).zone,
                if host_exists {
                    Zone::Battlefield
                } else {
                    Zone::Graveyard
                }
            );
            if let Some(host) = host {
                assert_eq!(
                    s.objects
                        .get(original[0])
                        .v4
                        .attached_to
                        .map(|link| link.object),
                    Some(host)
                );
            }
        }
    }
}
#[test]
fn genuine_paid_wave_as_enters_and_season_replacements_finish_one_entry_batch_both_seats() {
    for p in players() {
        let mut s = ready(p, &["Heraldic Banner", "Nine-Lives Familiar"], 20, "Island");
        put(&mut s, p, "Genesis Wave", Zone::Hand);
        let season = paid(&mut s, p, "Doubling Season", None, 5);
        for _ in 0..20 {
            let d = next(&mut s);
            if s.objects.get(season).zone == Zone::Battlefield {
                break;
            }
            pass(&mut s, d);
        }
        assert_eq!(s.objects.get(season).zone, Zone::Battlefield);
        let original = s.players[p.index()].library.clone();
        let wave = paid(&mut s, p, "Genesis Wave", Some(3), 6);
        let candidates = subset(&mut s, p);
        choose(&mut s, &candidates);
        let d = next(&mut s);
        assert!(matches!(d,Decision::ChooseEntryColorV36{player,..}if player==p));
        for o in &original {
            assert_eq!(s.objects.get(*o).zone, Zone::Library);
        }
        let mut replay = restore(&s);
        engine::step(&mut s, Action::ChooseEffectOption(0)).unwrap();
        engine::step(&mut replay, Action::ChooseEffectOption(0)).unwrap();
        finish(&mut s, wave);
        finish(&mut replay, wave);
        assert_eq!(s, replay);
        assert_eq!(s.objects.get(original[1]).counters.revival, 16);
    }
}
#[test]
fn genuine_paid_wave_progenitus_remainder_uses_real_shuffle_and_actual_graveyard_members_both_seats(
) {
    for p in players() {
        let mut s = ready(p, &["Progenitus", "Giant Growth", "Forest"], 15, "Island");
        let original = s.players[p.index()].library.clone();
        let before_rng = s.state_hash();
        let wave = paid(&mut s, p, "Genesis Wave", Some(2), 5);
        assert!(subset(&mut s, p).is_empty());
        choose(&mut s, &[]);
        s = restore(&s);
        finish(&mut s, wave);
        assert_eq!(s.objects.get(original[0]).zone, Zone::Library);
        assert_eq!(s.objects.get(original[0]).zone_change_count, 0);
        assert_eq!(s.objects.get(original[1]).zone, Zone::Graveyard);
        assert_ne!(s.state_hash(), before_rng);
        assert!(s.engine.event_history.iter().any(|e|matches!(e,event::CommittedEvent::ProgenitusShuffledV1{source,..}if source.source==original[0])));
    }
}
#[test]
fn genuine_paid_wave_x256_real_automatic_sources_and_full_native_subset_width_both_seats() {
    for p in players() {
        let library = vec!["Forest"; 257];
        let mut s = ready(p, &library, 259, "Island");
        let original = s.players[p.index()].library.clone();
        let wave = paid(&mut s, p, "Genesis Wave", Some(256), 259);
        let candidates = subset(&mut s, p);
        assert_eq!(candidates, original[..256]);
        assert!(matches!(
            next(&mut s),
            Decision::ChooseEffectTargets {
                max_targets: 256,
                ..
            }
        ));
        let mut replay = restore(&s);
        choose(&mut s, &[original[255]]);
        choose(&mut replay, &[original[255]]);
        finish(&mut s, wave);
        finish(&mut replay, wave);
        assert_eq!(s, replay);
        assert_eq!(s.objects.get(original[255]).zone, Zone::Battlefield);
        assert_eq!(s.players[p.index()].library, vec![original[256]]);
        assert_eq!(
            s.players[p.index()]
                .graveyard
                .iter()
                .filter(|o| original[..256].contains(o))
                .count(),
            255
        );
    }
}
#[test]
fn genuine_paid_wave_changed_pending_library_context_orphan_and_public_execution_refuse_readonly_both_seats(
) {
    for p in players() {
        let mut s = ready(p, &["Forest", "Llanowar Elves"], 10, "Island");
        let wave = paid(&mut s, p, "Genesis Wave", Some(2), 5);
        subset(&mut s, p);
        let pending = s.engine.pending_effect.as_ref().unwrap().clone();
        let before = s.clone();
        assert!(begin_resumable_resolution(
            &EffectOp::GenesisWaveV1,
            &pending.ctx,
            pending.resolving_item.clone(),
            &mut s
        )
        .is_err());
        assert_eq!(s, before);
        assert!(resume_resumable_resolution(&mut s).is_err());
        assert_eq!(s, before);
        for id in [pending.ctx.stack_item_id, Some(StackItemId(u64::MAX)), None] {
            let mut direct = restore(&s);
            let mut ctx = pending.ctx.clone();
            ctx.stack_item_id = id;
            execute(&EffectOp::GenesisWaveV1, &ctx, &mut direct);
            let mut expected = s.clone();
            expected.engine.halted = Some((
                crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                ctx.source,
            ));
            assert_eq!(direct, expected);
        }
        for variant in 0..7 {
            let mut bad = restore(&s);
            match variant {
                0 => bad.players[p.index()].library.swap(0, 1),
                1 => bad.engine.pending_effect.as_mut().unwrap().ctx.controller = p.opponent(),
                2 => bad.engine.pending_effect = None,
                3 => {
                    let old = bad
                        .engine
                        .event_history
                        .iter()
                        .find(|e| matches!(e, event::CommittedEvent::GenesisWaveProgressV1 { .. }))
                        .unwrap()
                        .clone();
                    bad.engine.event_history.push(old);
                }
                4 => {
                    if let Some(PendingEffectChoice::SelectTargets { legal, .. }) =
                        &mut bad.engine.pending_effect.as_mut().unwrap().choice
                    {
                        legal.reverse();
                    }
                }
                5 => bad.objects.get_mut(wave).zone_change_count += 1,
                _ => {
                    let old = bad
                        .engine
                        .event_history
                        .iter()
                        .find(|e| matches!(e, event::CommittedEvent::GenesisWaveProgressV1 { .. }))
                        .unwrap()
                        .clone();
                    bad.engine.event_log.push(old);
                }
            }
            let frozen = bad.clone();
            assert!(engine::step(&mut bad, Action::FinishEffectSelection).is_err());
            assert_eq!(bad, frozen);
        }
        choose(&mut s, &[]);
        finish(&mut s, wave);
    }
}
#[test]
fn independent_prepaid_wave_hidden_worlds_public_reveal_and_foreign_continuation_refusal_both_seats(
) {
    for p in players() {
        let mut a = ready(p, &["Forest", "Llanowar Elves"], 10, "Island");
        let mut b = ready(p, &["Forest", "Llanowar Elves"], 10, "Swamp");
        // Independent hidden hand operands are declared before any action.
        put(&mut a, p.opponent(), "Genesis Wave", Zone::Hand);
        put(&mut b, p.opponent(), "Giant Growth", Zone::Hand);
        assert!(refuse_legacy(&a, Some(p.opponent())).is_err());
        assert!(refuse_legacy(&a, Some(p)).is_ok());
        assert_eq!(
            crate::rl::observe_v1(&a, p, 0).unwrap(),
            crate::rl::observe_v1(&b, p, 0).unwrap()
        );
        let wa = paid(&mut a, p, "Genesis Wave", Some(2), 5);
        let wb = paid(&mut b, p, "Genesis Wave", Some(2), 5);
        let ca = subset(&mut a, p);
        let cb = subset(&mut b, p);
        assert_eq!(ca, cb);
        for viewer in players() {
            assert_eq!(
                a.known_library_cards(viewer, p),
                b.known_library_cards(viewer, p)
            );
            assert_eq!(a.known_library_cards(viewer, p).len(), 2);
        }
        // A changed initial own library creates a genuinely distinct issued
        // checkpoint. Replacing only the pending packet cannot replace it.
        let mut foreign = ready(p, &["Island", "Llanowar Elves", "Forest"], 10, "Island");
        paid(&mut foreign, p, "Genesis Wave", Some(2), 5);
        subset(&mut foreign, p);
        let mut forged = restore(&a);
        forged.engine.pending_effect = foreign.engine.pending_effect.clone();
        let before = forged.clone();
        assert!(engine::step(&mut forged, Action::FinishEffectSelection).is_err());
        assert_eq!(forged, before);
        choose(&mut a, &ca);
        choose(&mut b, &cb);
        finish(&mut a, wa);
        finish(&mut b, wb);
        assert_eq!(a.players[p.index()], b.players[p.index()]);
    }
}
#[test]
fn genuine_paid_wave_entry_draw_trigger_waits_for_remainder_and_spell_departure_both_seats() {
    for p in players() {
        let mut s = ready(
            p,
            &["Helpful Hunter", "Giant Growth", "Forest"],
            10,
            "Island",
        );
        let original = s.players[p.index()].library.clone();
        let wave = paid(&mut s, p, "Genesis Wave", Some(2), 5);
        assert_eq!(subset(&mut s, p), vec![original[0]]);
        choose(&mut s, &[original[0]]);
        let d = next(&mut s);
        assert!(matches!(d, Decision::CastSpellOrPass { .. }));
        assert_eq!(s.objects.get(wave).zone, Zone::Graveyard);
        assert_eq!(s.objects.get(original[1]).zone, Zone::Graveyard);
        assert_eq!(s.objects.get(original[2]).zone, Zone::Library);
        assert!(s
            .stack
            .iter()
            .any(|r| r.source == original[0]
                && r.kind == crate::state::StackItemKind::TriggeredAbility));
        let mut replay = restore(&s);
        for state in [&mut s, &mut replay] {
            for _ in 0..32 {
                let d = next(state);
                if state.objects.get(original[2]).zone == Zone::Hand {
                    break;
                }
                pass(state, d);
            }
            assert_eq!(state.objects.get(original[2]).zone, Zone::Hand);
        }
        assert_eq!(s, replay);
    }
}
#[test]
fn genuine_paid_wave_counter_response_retains_x_source_and_never_reveals_both_seats() {
    for p in players() {
        let mut s = ready(p, &["Forest", "Llanowar Elves"], 10, "Island");
        let counter = put(&mut s, p.opponent(), "Counterspell", Zone::Hand);
        let original = s.players[p.index()].library.clone();
        let wave = paid(&mut s, p, "Genesis Wave", Some(2), 5);
        engine::step(&mut s, Action::Pass).unwrap();
        priority(&mut s, p.opponent());
        let before = tapped(&s, p.opponent());
        assert!(
            matches!(next(&mut s),Decision::CastSpellOrPass{castable_spells,..}if castable_spells.contains(&counter))
        );
        engine::step(&mut s, Action::CastSpell(counter)).unwrap();
        let d = next(&mut s);
        assert!(
            matches!(d,Decision::ChooseTargets{legal_targets,..}if legal_targets.contains(&Target::Object(wave)))
        );
        engine::step(&mut s, Action::ChooseTarget(Target::Object(wave))).unwrap();
        assert!(matches!(next(&mut s),Decision::CastSpellOrPass{player,..}if player==p.opponent()));
        assert_eq!(tapped(&s, p.opponent()) - before, 2);
        assert_eq!(s.players[p.opponent().index()].mana_pool, [0; 6]);
        let mut replay = restore(&s);
        for state in [&mut s, &mut replay] {
            for _ in 0..32 {
                let d = next(state);
                if state.objects.get(wave).zone == Zone::Graveyard {
                    break;
                }
                pass(state, d);
            }
            assert_eq!(state.objects.get(wave).zone, Zone::Graveyard);
            assert_eq!(state.players[p.index()].library, original);
            assert!(state.known_library_cards(p.opponent(), p).is_empty());
            assert!(!state
                .engine
                .event_history
                .has_any(&[crate::event_history_v1::EventKindV1::GenesisWaveProgressV1]));
        }
        assert_eq!(s, replay);
    }
}
