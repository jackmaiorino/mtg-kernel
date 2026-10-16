//! Genuine casts and native actions; explicit setup/suppression/copy operands
//! are identified in their cases. No free mana pool or fabricated permission.
use super::*;
use crate::{policy_observation_v6::tests::put, state::Step};
const NAME: &str = "Progenitus";
fn id(name: &str) -> u16 {
    card_def::card_id_by_name(name).unwrap()
}
fn ready(p: PlayerId, hidden: &str) -> GameState {
    let library = [id("Forest"); 80];
    let other = [id(hidden); 80];
    let (a, b) = if p == PlayerId::P0 {
        (&library, &other)
    } else {
        (&other, &library)
    };
    let mut s = GameState::new_from_libraries_with_starting_player_v1(
        a,
        b,
        crate::rl::card_name,
        110_480,
        p,
    );
    s.step = Step::Main1;
    // Native initial resource placement, followed by real land-funded casts.
    for player in [p, p.opponent()] {
        for name in ["Island", "Plains", "Mountain", "Forest", "Swamp"] {
            for _ in 0..8 {
                put(&mut s, player, name, Zone::Battlefield);
            }
        }
    }
    s
}
fn restore(s: &GameState) -> GameState {
    let r: GameState = serde_json::from_slice(&serde_json::to_vec(s).unwrap()).unwrap();
    assert_eq!(*s, r);
    assert_eq!(s.state_hash(), r.state_hash());
    r
}
fn next(s: &mut GameState) -> Decision {
    let d = advance_until_decision(s);
    assert!(s.engine.halted.is_none(), "{:?}", s.engine.halted);
    assert!(!matches!(d, Decision::Halted { .. }), "{d:?}");
    d
}
fn pass(s: &mut GameState, d: Decision) {
    let action = match d {
        Decision::CastSpellOrPass { .. } => Action::Pass,
        Decision::DeclareAttackers { .. } => Action::DeclareAttackers(vec![]),
        Decision::DeclareBlockers { .. } => Action::DeclareBlockers(vec![]),
        Decision::ChooseTargets {
            player,
            legal_targets,
            ..
        } => {
            let t = Target::Player(player.opponent());
            assert!(legal_targets.contains(&t));
            Action::ChooseTarget(t)
        }
        Decision::ChooseEffectTargets {
            legal_targets,
            can_finish,
            ..
        } => {
            if can_finish {
                Action::FinishEffectSelection
            } else {
                Action::ChooseEffectTarget(legal_targets[0])
            }
        }
        Decision::ChooseLegendPermanent { candidates, .. } => Action::ChooseLegendPermanent(
            candidates
                .iter()
                .copied()
                .find(|id| !s.objects.get(*id).v4.is_token)
                .unwrap_or(candidates[0]),
        ),
        Decision::OrderTriggers { pending, .. } => {
            Action::OrderTriggers((0..pending.len()).collect())
        }
        Decision::Discard { player, count, .. } => Action::Discard(
            s.players[player.index()]
                .hand
                .iter()
                .copied()
                .take(count as usize)
                .collect(),
        ),
        d => panic!("Progenitus native priority {d:?}"),
    };
    step(s, action).unwrap();
}
fn priority(s: &mut GameState, p: PlayerId) {
    for _ in 0..32 {
        let d = next(s);
        if matches!(d,Decision::CastSpellOrPass{player,..}if player==p) {
            return;
        }
        pass(s, d);
    }
    panic!("actor priority absent");
}
fn finish(s: &mut GameState) {
    for _ in 0..64 {
        let d = next(s);
        if matches!(d, Decision::CastSpellOrPass { .. })
            && s.stack.is_empty()
            && s.engine.pending_effect.is_none()
            && s.engine.pending_triggers.is_empty()
        {
            return;
        }
        pass(s, d);
    }
    panic!("native stack unfinished");
}
fn tapped(s: &GameState, p: PlayerId) -> usize {
    s.players[p.index()]
        .battlefield
        .iter()
        .filter(|id| s.objects.get(**id).tapped)
        .count()
}
fn pay(
    s: &mut GameState,
    p: PlayerId,
    card: ObjectId,
    selection: (bool, u16, bool),
    targets: &[Target],
    discard: Option<ObjectId>,
) -> StackItem {
    let (free, x, kick) = selection;
    priority(s, p);
    assert!(
        matches!(next(s),Decision::CastSpellOrPass{castable_spells,..}if castable_spells.contains(&card))
    );
    step(s, Action::CastSpell(card)).unwrap();
    let mut chosen = 0;
    for _ in 0..48 {
        let action = match next(s) {
            Decision::ChooseEffectOption {
                player,
                source,
                option_count,
            } => {
                assert_eq!((player, source), (p, card));
                let pending = s.engine.pending_cast.as_ref().unwrap();
                let option = if pending.omniscience_v1.is_some_and(|c| c.free.is_none()) {
                    u16::from(free)
                } else if pending
                    .mandatory_additional_cost
                    .is_some_and(|c| c.pay_mana.is_none())
                {
                    1
                } else {
                    x
                };
                assert!(option < option_count);
                Action::ChooseEffectOption(option)
            }
            Decision::ChooseKicker { player, spell } => {
                assert_eq!((player, spell), (p, card));
                Action::ChooseKicker(kick)
            }
            Decision::ChooseTargets {
                player,
                legal_targets,
                ..
            } => {
                assert_eq!(player, p);
                assert!(chosen < targets.len());
                assert!(legal_targets.contains(&targets[chosen]));
                let a = Action::ChooseTarget(targets[chosen]);
                chosen += 1;
                a
            }
            Decision::Discard { player, count, .. } => {
                assert_eq!((player, count), (p, 1));
                let object = discard.unwrap();
                assert!(s.players[p.index()].hand.contains(&object));
                Action::Discard(vec![object])
            }
            Decision::CastSpellOrPass { player, .. } => {
                assert_eq!(player, p);
                assert_eq!(chosen, targets.len());
                assert!(s.engine.pending_cast.is_none());
                assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
                let row = s
                    .stack
                    .iter()
                    .find(|r| r.source == card && r.kind == StackItemKind::Spell)
                    .unwrap()
                    .clone();
                validated_stack_item_target_spec(&row, s).unwrap();
                return row;
            }
            d => panic!("native selected cast {d:?}"),
        };
        // The restored decision is answered independently through native step.
        let mut r = restore(s);
        step(&mut r, action.clone()).unwrap();
        step(s, action).unwrap();
        assert_eq!(*s, r);
    }
    panic!("selected cast not paid");
}
fn cast(
    s: &mut GameState,
    p: PlayerId,
    name: &str,
    selection: (bool, u16, bool),
    targets: &[Target],
    discard: Option<ObjectId>,
    cost: usize,
) -> ObjectId {
    let (free, x, kick) = selection;
    priority(s, p);
    let card = put(s, p, name, Zone::Hand);
    let before = tapped(s, p);
    pay(s, p, card, (free, x, kick), targets, discard);
    assert_eq!(tapped(s, p) - before, cost);
    card
}

fn paid(s: &mut GameState, p: PlayerId) -> ObjectId {
    let c = cast(s, p, NAME, (false, 0, false), &[], None, 10);
    finish(s);
    assert_eq!(s.objects.get(c).zone, Zone::Battlefield);
    assert_eq!((effective_power(s, c), effective_toughness(s, c)), (10, 10));
    c
}
fn primitive_top(s: &mut GameState, p: PlayerId, names: &[&str]) -> Vec<ObjectId> {
    // Explicit native initial-library ordering operand, never a legal rearrange claim.
    let added = names
        .iter()
        .map(|n| put(s, p, n, Zone::Library))
        .collect::<Vec<_>>();
    let library = &mut s.players[p.index()].library;
    library.retain(|id| !added.contains(id));
    library.splice(..0, added.iter().copied());
    added
}
#[test]
fn exact_ten_colored_paid_spell_counter_response_and_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        assert!(crate::progenitus_v1::definition(id(NAME)));
        assert_eq!(id(NAME), 480);
        let card = cast(&mut s, p, NAME, (false, 0, false), &[], None, 10);
        let original = s.objects.get(card).zone_change_count;
        assert!(
            legal_targets_for_controller(TargetSpec::AnySpellOnStack, &[], p.opponent(), &s)
                .contains(&Target::Object(card))
        );
        let counter = cast(
            &mut s,
            p.opponent(),
            "Counterspell",
            (false, 0, false),
            &[Target::Object(card)],
            None,
            2,
        );
        let mut r = restore(&s);
        finish(&mut s);
        finish(&mut r);
        assert_eq!(s, r);
        assert_eq!(s.objects.get(counter).zone, Zone::Graveyard);
        assert_eq!(s.objects.get(card).zone, Zone::Library);
        assert_eq!(s.objects.get(card).zone_change_count, original + 1);
        assert!(!s.players[p.index()].graveyard.contains(&card));
        crate::progenitus_v1::validate(&s).unwrap();
        assert!(crate::progenitus_v1::refuse_legacy(&s).is_err());
    }
}
#[test]
fn paid_battlefield_all_source_targets_damage_block_and_attachment_protection_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        let card = paid(&mut s, p);
        for chooser in [p, p.opponent()] {
            for spec in [TargetSpec::AnyTarget, TargetSpec::Creature] {
                assert!(!legal_targets_for_controller(spec, &[], chooser, &s)
                    .contains(&Target::Object(card)));
            }
        }
        let source = put(&mut s, p.opponent(), "Llanowar Elves", Zone::Battlefield);
        // Non-targeting native damage primitive covers color-independent DEBT prevention.
        let before = s.objects.get(card).damage;
        event::propose_and_commit(
            &mut s,
            event::ProposedEvent::damage(source, Target::Object(card), 37),
        );
        assert_eq!(s.objects.get(card).damage, before);
        assert!(legal_blockers_for(&s, card).is_empty());
        for name in ["Viridian Longbow", "Witness Protection"] {
            let attachment = put(&mut s, p, name, Zone::Battlefield);
            let before = s.clone();
            assert!(s
                .attach_object_exact(attachment, 0, card, s.objects.get(card).zone_change_count)
                .is_err());
            assert_eq!(s, before);
        }
        let before = s.clone();
        assert!(step(&mut s, Action::ChooseTarget(Target::Object(card))).is_err());
        assert_eq!(s, before);
        assert_eq!(restore(&s), s);
    }
}
#[test]
fn paid_original_anywhere_discard_replacement_and_receipt_forgeries_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        let c = put(&mut s, p, NAME, Zone::Hand);
        let gen = s.objects.get(c).zone_change_count;
        let grab = cast(
            &mut s,
            p,
            "Thrill of Possibility",
            (false, 0, false),
            &[],
            Some(c),
            2,
        );
        assert_eq!(s.objects.get(c).zone, Zone::Library);
        assert_eq!(s.objects.get(c).zone_change_count, gen + 1);
        assert_eq!(s.stack.last().unwrap().source, grab);
        let mut r = restore(&s);
        finish(&mut s);
        finish(&mut r);
        assert_eq!(s, r);
        crate::progenitus_v1::validate(&s).unwrap();
        for edit in 0..4 {
            let mut bad = restore(&s);
            let at = bad
                .engine
                .event_history
                .iter()
                .position(|e| matches!(e, event::CommittedEvent::ProgenitusShuffledV1 { .. }))
                .unwrap();
            if edit == 3 {
                bad.engine.event_history.remove(at);
            } else if let event::CommittedEvent::ProgenitusShuffledV1 {
                input_history_index,
                source,
                ..
            } = &mut bad.engine.event_history[at]
            {
                match edit {
                    0 => source.controller = p.opponent(),
                    1 => source.zone_change_count += 1,
                    _ => *input_history_index += 1,
                }
            }
            let before = bad.clone();
            assert!(crate::progenitus_v1::validate(&bad).is_err());
            assert_eq!(bad, before);
        }
    }
}
#[test]
fn simultaneous_original_mill_members_multiple_shuffles_and_no_new_incarnation_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        let top = primitive_top(&mut s, p, &[NAME, "Island", NAME, "Forest"]);
        let start = s.engine.event_history.len();
        let generations = top
            .iter()
            .map(|id| s.objects.get(*id).zone_change_count)
            .collect::<Vec<_>>();
        let mut r = restore(&s);
        let events = top
            .iter()
            .map(|id| event::ProposedEvent::zone_change(*id, Zone::Graveyard))
            .collect::<Vec<_>>();
        event::propose_and_commit_batch(&mut s, events.clone());
        event::propose_and_commit_batch(&mut r, events);
        assert_eq!(s, r);
        assert!(s.engine.halted.is_none());
        assert_eq!(
            &s.players[p.index()].graveyard[s.players[p.index()].graveyard.len() - 2..],
            &[top[1], top[3]]
        );
        for i in [0, 2] {
            assert_eq!(s.objects.get(top[i]).zone, Zone::Library);
            assert_eq!(s.objects.get(top[i]).zone_change_count, generations[i]);
        }
        let suffix = &s.engine.event_history[start..];
        assert_eq!(
            suffix
                .iter()
                .filter(|e| matches!(e, event::CommittedEvent::ProgenitusShuffledV1 { .. }))
                .count(),
            2
        );
        let first_shuffle = suffix
            .iter()
            .position(|e| matches!(e, event::CommittedEvent::ProgenitusShuffledV1 { .. }))
            .unwrap();
        assert_eq!(
            suffix[..first_shuffle]
                .iter()
                .filter(|e| matches!(
                    e,
                    event::CommittedEvent::ZoneChange {
                        from: Zone::Library,
                        to: Zone::Graveyard,
                        ..
                    }
                ))
                .count(),
            2
        );
        crate::progenitus_v1::validate(&s).unwrap();
    }
}
#[test]
fn genuinely_paid_tutelage_draw_mixed_progenitus_pair_stops_after_actual_public_result_both_seats()
{
    for p in [PlayerId::P0, PlayerId::P1] {
        for names in [[NAME, "Think Twice"], [NAME, NAME]] {
            let mut s = ready(p, "Forest");
            let top = primitive_top(&mut s, p.opponent(), &names);
            let t = cast(
                &mut s,
                p,
                "Sphinx's Tutelage",
                (false, 0, false),
                &[],
                None,
                3,
            );
            finish(&mut s);
            assert_eq!(s.objects.get(t).zone, Zone::Battlefield);
            let start = s.engine.event_history.len();
            cast(&mut s, p, "Think Twice", (false, 0, false), &[], None, 2);
            let mut r = restore(&s);
            finish(&mut s);
            finish(&mut r);
            assert_eq!(s, r);
            assert!(s.engine.halted.is_none());
            assert_eq!(
                s.engine.event_history[start..]
                    .iter()
                    .filter(|e| matches!(
                        e,
                        event::CommittedEvent::SphinxsTutelageMillBatchV1 { .. }
                    ))
                    .count(),
                1
            );
            for (i, name) in names.iter().enumerate() {
                assert_eq!(
                    s.objects.get(top[i]).zone,
                    if *name == NAME {
                        Zone::Library
                    } else {
                        Zone::Graveyard
                    }
                );
            }
            crate::progenitus_v1::validate(&s).unwrap();
        }
    }
}
#[test]
fn genuine_native_spell_copy_conversion_season_legend_and_token_cease_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for season in [false, true] {
            let mut s = ready(p, "Forest");
            if season {
                cast(
                    &mut s,
                    p,
                    "Doubling Season",
                    (false, 0, false),
                    &[],
                    None,
                    5,
                );
                finish(&mut s);
            }
            let c = cast(&mut s, p, NAME, (false, 0, false), &[], None, 10);
            // Actual existing native spell-copy primitive on the genuinely paid row.
            // No pure-FDN paid copy-spell producer is claimed.
            let (copy, _) = copy_spell_for(&mut s, c, p).unwrap();
            assert!(s.objects.get(copy).spell_copy_origin.is_some());
            let mut countered = restore(&s);
            cast(
                &mut countered,
                p.opponent(),
                "Counterspell",
                (false, 0, false),
                &[Target::Object(copy)],
                None,
                2,
            );
            finish(&mut countered);
            assert!(!countered.players[p.index()].library.contains(&copy));
            assert!(!countered.engine.event_history.iter().any(|entry|matches!(entry,event::CommittedEvent::ProgenitusShuffledV1{source,..}if source.source==copy)));
            let mut r = restore(&s);
            finish(&mut s);
            finish(&mut r);
            assert_eq!(s, r);
            assert_eq!(
                s.players[p.index()]
                    .battlefield
                    .iter()
                    .filter(|object| s.objects.get(**object).card_def == id(NAME))
                    .count(),
                1
            );
            assert!(s.players[p.index()].battlefield.contains(&c));
            assert!(!s.players[p.index()].library.contains(&copy));
            assert!(s.engine.event_history.iter().any(|entry|matches!(entry,event::CommittedEvent::ProgenitusShuffledV1{source,..}if source.source==copy)));
            crate::progenitus_v1::validate(&s).unwrap();
            let mut raw = ready(p, "Forest");
            event::propose_and_commit(&mut raw, event::ProposedEvent::create_token(id(NAME), p));
            assert!(raw.engine.halted.is_some());
        }
    }
}
#[test]
fn paid_source_control_suppression_and_independent_hidden_worlds_remain_native_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut a = ready(p, "Forest");
        let mut b = ready(p, "Island");
        let c = paid(&mut a, p);
        assert_eq!(paid(&mut b, p), c);
        assert_ne!(a.state_hash(), b.state_hash());
        assert_eq!(next(&mut a), next(&mut b));
        // Neutral face-down and width operands are labelled native primitives,
        // not a paid manifest or billions-of-zone-changes claim.
        let mut neutral = restore(&a);
        neutral.objects.get_mut(c).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
            disguised: false,
            lookers: 0,
            hidden_by: None,
            hidden_by_source: None,
        });
        assert!(!has_effective_keyword(
            &neutral,
            c,
            Keywords::PROTECTION_FROM_EVERYTHING
        ));
        assert!(crate::progenitus_v1::plan(
            &neutral,
            &event::ProposedEvent::zone_change(c, Zone::Graveyard),
            None
        )
        .unwrap()
        .is_none());
        let mut width = restore(&a);
        width.objects.get_mut(c).zone_change_count = u32::MAX;
        let before = width.clone();
        assert!(crate::progenitus_v1::plan(
            &width,
            &event::ProposedEvent::zone_change(c, Zone::Graveyard),
            None
        )
        .is_err());
        assert_eq!(width, before);
        // Explicit native control/suppression primitives on a genuinely paid source.
        crate::standard_cards_v1::gain_control(&mut a, c, p.opponent());
        let gen = a.objects.get(c).zone_change_count;
        event::propose_and_commit(
            &mut a,
            event::ProposedEvent::zone_change(c, Zone::Graveyard),
        );
        assert_eq!(a.objects.get(c).zone, Zone::Library);
        assert!(a.players[p.index()].library.contains(&c));
        assert_eq!(a.objects.get(c).zone_change_count, gen + 1);
        let mut s = ready(p, "Forest");
        let c = paid(&mut s, p);
        let suppressor = put(&mut s, p.opponent(), "Llanowar Elves", Zone::Battlefield);
        let link = crate::state::ObjectLinkV4 {
            object: suppressor,
            zone_change_count: s.objects.get(suppressor).zone_change_count,
        };
        s.objects
            .get_mut(c)
            .v4
            .creature_upgrade
            .get_or_insert_with(Default::default)
            .suppressed_by
            .push(crate::standard_creatures_v1::TidebinderSuppressionV1 {
                source: link,
                timestamp: 1,
            });
        assert!(!has_effective_keyword(
            &s,
            c,
            Keywords::PROTECTION_FROM_EVERYTHING
        ));
        let mut r = restore(&s);
        event::propose_and_commit(
            &mut s,
            event::ProposedEvent::zone_change(c, Zone::Graveyard),
        );
        event::propose_and_commit(
            &mut r,
            event::ProposedEvent::zone_change(c, Zone::Graveyard),
        );
        assert_eq!(s, r);
        assert_eq!(s.objects.get(c).zone, Zone::Graveyard);
    }
}

#[test]
fn paid_inspiration_orders_only_actual_graveyard_entrants_and_restores_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        let top = primitive_top(&mut s, p, &[NAME, "Think Twice", "Lightning Bolt"]);
        cast(
            &mut s,
            p,
            "Inspiration from Beyond",
            (false, 0, false),
            &[],
            None,
            3,
        );
        for _ in 0..20 {
            let d = next(&mut s);
            if matches!(d, Decision::ChooseEffectTargets { .. }) {
                break;
            }
            pass(&mut s, d);
        }
        assert!(s
            .known_library_cards(p.opponent(), p)
            .iter()
            .any(|entry| entry.object == top[0]));
        assert!(!s
            .known_library_cards(p.opponent(), p)
            .iter()
            .any(|entry| entry.object == top[1] || entry.object == top[2]));
        let pending = s.engine.pending_effect.as_ref().unwrap();
        let Some(crate::effect::PendingEffectChoice::SelectTargets {
            legal,
            min_targets,
            max_targets,
            purpose,
            ..
        }) = &pending.choice
        else {
            panic!("actual mill order absent");
        };
        assert!(
            matches!(purpose,crate::effect::EffectTargetSelectionPurpose::OrderProgenitusGraveyardV1{original,..}if original.iter().map(|b|b.object).collect::<Vec<_>>()==top)
        );
        assert_eq!((*min_targets, *max_targets), (2, 2));
        assert_eq!(
            legal.iter().map(|c| c.target).collect::<Vec<_>>(),
            vec![Target::Object(top[1]), Target::Object(top[2])]
        );
        let mut bad = restore(&s);
        if let Some(crate::effect::PendingEffectChoice::SelectTargets {
            purpose:
                crate::effect::EffectTargetSelectionPurpose::OrderProgenitusGraveyardV1 {
                    original, ..
                },
            ..
        }) = bad.engine.pending_effect.as_mut().unwrap().choice.as_mut()
        {
            original[0].expected_zone_change_count += 1;
        }
        let before = bad.clone();
        assert!(crate::effect::validate_pending_effect_choice(&bad).is_err());
        assert_eq!(bad, before);
        let before = s.clone();
        assert!(step(&mut s, Action::ChooseEffectTarget(Target::Object(top[0]))).is_err());
        assert_eq!(s, before);
        let mut r = restore(&s);
        step(&mut s, Action::ChooseEffectTarget(Target::Object(top[2]))).unwrap();
        step(&mut r, Action::ChooseEffectTarget(Target::Object(top[2]))).unwrap();
        assert_eq!(s, r);
        finish(&mut s);
        finish(&mut r);
        assert_eq!(s, r);
        assert_eq!(s.objects.get(top[0]).zone, Zone::Library);
        assert_eq!(s.objects.get(top[0]).zone_change_count, 0);
        assert_eq!(
            [top[1], top[2]]
                .iter()
                .filter(|id| s.objects.get(**id).zone == Zone::Hand)
                .count(),
            1
        );
        crate::progenitus_v1::validate(&s).unwrap();
    }
}

#[test]
fn paid_edict_nontargeting_sacrifice_replaces_without_a_death_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        let c = paid(&mut s, p);
        let gen = s.objects.get(c).zone_change_count;
        let bear = put(&mut s, p.opponent(), "Llanowar Elves", Zone::Battlefield);
        let start = s.engine.event_history.len();
        cast(
            &mut s,
            p,
            "Blasphemous Edict",
            (false, 0, false),
            &[],
            None,
            5,
        );
        let mut r = restore(&s);
        finish(&mut s);
        finish(&mut r);
        assert_eq!(s, r);
        assert_eq!(s.objects.get(c).zone, Zone::Library);
        assert_eq!(s.objects.get(c).zone_change_count, gen + 1);
        assert_eq!(s.objects.get(bear).zone, Zone::Graveyard);
        assert!(!s.engine.event_history[start..].iter().any(|e|matches!(e,event::CommittedEvent::ZoneChange{object,from:Zone::Battlefield,to:Zone::Graveyard,..}if *object==c)));
        crate::progenitus_v1::validate(&s).unwrap();
    }
}
