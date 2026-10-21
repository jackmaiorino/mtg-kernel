//! Actual paid native actions. Initial libraries/lands/cards and explicitly
//! named adversarial/control/permission operands are setup primitives.
use super::*;
use crate::policy_observation_v6::tests::put;
const NAME: &str = "Tinybones, Bauble Burglar";
fn id(name: &str) -> u16 {
    card_def::card_id_by_name(name).unwrap_or_else(|| panic!("registered fixture {name}"))
}
fn ready(p: PlayerId, hidden: &str) -> GameState {
    let a = [id("Forest"); 80];
    let b = [id(hidden); 80];
    let (a, b) = if p == PlayerId::P0 {
        (&a, &b)
    } else {
        (&b, &a)
    };
    let mut s = GameState::new_from_libraries_with_starting_player_v1(
        a,
        b,
        crate::rl::card_name,
        110_490,
        p,
    );
    s.step = Step::Main1;
    for n in ["Swamp", "Forest"] {
        for _ in 0..16 {
            put(&mut s, p, n, Zone::Battlefield);
        }
    }
    for n in ["Plains", "Island", "Swamp", "Mountain", "Forest"] {
        for _ in 0..8 {
            put(&mut s, p.opponent(), n, Zone::Battlefield);
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
    let mut r = restore(s);
    let a = advance_until_decision(s);
    let b = advance_until_decision(&mut r);
    assert_eq!(a, b);
    assert_eq!(*s, r);
    assert!(s.engine.halted.is_none(), "{:?}", s.engine.halted);
    a
}
fn act(s: &mut GameState, a: Action) {
    let mut r = restore(s);
    step(&mut r, a.clone()).unwrap();
    step(s, a).unwrap();
    assert_eq!(*s, r);
    assert_eq!(s.state_hash(), r.state_hash());
}
fn pass(s: &mut GameState, d: Decision) {
    let a = match d {
        Decision::CastSpellOrPass { .. } => Action::Pass,
        Decision::DeclareAttackers { .. } => Action::DeclareAttackers(vec![]),
        Decision::DeclareBlockers { .. } => Action::DeclareBlockers(vec![]),
        Decision::OrderTriggers { pending, .. } => {
            Action::OrderTriggers((0..pending.len()).collect())
        }
        Decision::ChooseMadnessCast { .. } => Action::ChooseMadnessCast(false),
        Decision::Discard { count, choices, .. } => {
            Action::Discard(choices.into_iter().take(count as usize).collect())
        }
        d => panic!("unexpected native phase {d:?}"),
    };
    act(s, a);
}
fn priority(s: &mut GameState, p: PlayerId) {
    for _ in 0..256 {
        let d = next(s);
        if matches!(d,Decision::CastSpellOrPass{player,..}if player==p) {
            return;
        }
        pass(s, d);
    }
    panic!("priority absent");
}
fn finish(s: &mut GameState) {
    for _ in 0..128 {
        let d = next(s);
        if s.stack.is_empty()
            && s.engine.pending_effect.is_none()
            && s.engine.pending_discard.is_none()
        {
            return;
        }
        pass(s, d);
    }
    panic!("stack unfinished");
}
fn turn(s: &mut GameState, p: PlayerId) {
    let old = (s.turn_key_v1(), s.active_player);
    for _ in 0..512 {
        let d = next(s);
        if (s.turn_key_v1(), s.active_player) != old
            && s.active_player == p
            && s.step == Step::Main1
            && s.stack.is_empty()
            && matches!(d,Decision::CastSpellOrPass{player,..}if player==p)
        {
            return;
        }
        pass(s, d);
    }
    panic!("real own main phase absent");
}
fn tapped(s: &GameState, p: PlayerId) -> usize {
    s.players[p.index()]
        .battlefield
        .iter()
        .filter(|id| s.objects.get(**id).tapped && object_has_type(s, **id, CardType::Land))
        .count()
}
fn cast(s: &mut GameState, p: PlayerId, c: ObjectId, t: Option<Target>) -> StackItem {
    cast_mode(s, p, c, t, 0)
}
fn cast_mode(
    s: &mut GameState,
    p: PlayerId,
    c: ObjectId,
    t: Option<Target>,
    mode: u8,
) -> StackItem {
    priority(s, p);
    assert!(castable_spells(p, s).contains(&c));
    act(s, Action::CastSpell(c));
    for _ in 0..64 {
        let d = next(s);
        let a = match d {
            Decision::ChooseTargets { legal_targets, .. } => {
                let t = t.unwrap();
                assert!(legal_targets.contains(&t));
                Action::ChooseTarget(t)
            }
            Decision::ChooseSpellMode { legal_modes, .. } => {
                assert!(legal_modes.contains(&mode));
                Action::ChooseSpellMode(mode)
            }
            Decision::ChooseKicker { .. } => Action::ChooseKicker(false),
            Decision::ChooseEffectOption { option_count, .. } => {
                assert!(option_count > 0);
                Action::ChooseEffectOption(0)
            }
            Decision::CastSpellOrPass { player, .. } => {
                assert_eq!(player, p);
                assert!(s.engine.pending_cast.is_none());
                let row = s
                    .stack
                    .iter()
                    .find(|r| r.kind == StackItemKind::Spell && r.source == c)
                    .unwrap()
                    .clone();
                validated_stack_item_target_spec(&row, s).unwrap();
                return row;
            }
            d => panic!("native cast {d:?}"),
        };
        act(s, a);
    }
    panic!("cast unfinished");
}
fn paid(s: &mut GameState, p: PlayerId, c: ObjectId) {
    let before = tapped(s, p);
    let row = cast(s, p, c, None);
    assert_eq!(row.controller, p);
    assert_eq!(row.v4.source_contract.unwrap().owner, p);
    assert_eq!(tapped(s, p) - before, 2);
    finish(s);
    assert_eq!(s.objects.get(c).zone, Zone::Battlefield);
}
fn activate_to_discard(s: &mut GameState, p: PlayerId, c: ObjectId, victim: ObjectId) {
    priority(s, p);
    assert!(available_activatable_abilities(p, s).contains(&(c, 0)));
    let before = tapped(s, p);
    act(s, Action::ActivateAbility(c, 0));
    let d = next(s);
    assert!(matches!(d,Decision::CastSpellOrPass{player,..}if player==p));
    assert_eq!(tapped(s, p) - before, 4);
    assert!(s.objects.get(c).tapped);
    assert!(s.players[p.index()].mana_pool.iter().all(|n| *n == 0));
    let row = s
        .stack
        .iter()
        .find(|r| r.kind == StackItemKind::ActivatedAbility && r.source == c)
        .unwrap();
    assert_eq!(row.controller, p);
    assert_eq!(row.v4.ability_source_contract.unwrap().source, c);
    for _ in 0..32 {
        let d = next(s);
        if let Decision::Discard {
            player,
            count,
            choices,
        } = d
        {
            assert_eq!(player, p.opponent());
            assert_eq!(count, 1);
            assert!(choices.contains(&victim));
            return;
        }
        pass(s, d);
    }
    panic!("real opponent discard absent");
}
fn trigger(s: &mut GameState, p: PlayerId, c: ObjectId, victim: ObjectId) -> StackItem {
    activate_to_discard(s, p, c, victim);
    act(s, Action::Discard(vec![victim]));
    for _ in 0..32 {
        let d = next(s);
        if let Some(row) = s
            .stack
            .iter()
            .find(|r| matches!(r.inline_effect, Some(EffectOp::TinybonesStashV72 { .. })))
        {
            return row.clone();
        }
        pass(s, d);
    }
    panic!("real discard trigger absent");
}
fn stashed(s: &mut GameState, p: PlayerId, c: ObjectId, victim: ObjectId) {
    trigger(s, p, c, victim);
    finish(s);
    assert_eq!(s.objects.get(victim).zone, Zone::Exile);
    assert_eq!(s.objects.get(victim).counters.stash, 1);
    validate_state(&restore(s)).unwrap();
}
fn setup(p: PlayerId, name: &str) -> (GameState, ObjectId, ObjectId) {
    let mut s = ready(p, "Forest");
    let tiny = put(&mut s, p, NAME, Zone::Hand);
    let card = put(&mut s, p.opponent(), name, Zone::Hand);
    paid(&mut s, p, tiny);
    turn(&mut s, p);
    (s, tiny, card)
}

#[test]
fn exact_printing_paid_cast_sickness_sorcery_cost_and_opponent_choice_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        let tiny = put(&mut s, p, NAME, Zone::Hand);
        let card = put(&mut s, p.opponent(), "Think Twice", Zone::Hand);
        paid(&mut s, p, tiny);
        assert_eq!(id(NAME), 490);
        assert!(definition(490));
        assert_eq!(
            card_def::CARD_DEFS[490].capability,
            card_def::CardCapability::Partial
        );
        assert!(card_def::preflight_fully_supported_deck(&[490]).is_err());
        assert!(!available_activatable_abilities(p, &s).contains(&(tiny, 0)));
        let before = s.clone();
        assert!(step(&mut s, Action::ActivateAbility(tiny, 0)).is_err());
        assert_eq!(s, before);
        turn(&mut s, p);
        stashed(&mut s, p, tiny, card);
        assert!(play_allowed(&s, card, p));
        assert!(!play_allowed(&s, card, p.opponent()));
        assert!(require_legacy(&s, Some(p)).is_err());
        assert!(crate::rl::observe_policy_v6(
            &s,
            &crate::policy_surface_v5::PolicySurfaceV5::new_for_session(),
            p,
            0,
            0,
            0,
            1,
        )
        .is_err());
        // The same real ability is legal against an empty hand and still pays.
        let mut empty = ready(p, "Forest");
        let t = put(&mut empty, p, NAME, Zone::Hand);
        paid(&mut empty, p, t);
        turn(&mut empty, p.opponent());
        let drawn = empty.players[p.opponent().index()].hand.clone();
        assert_eq!(drawn.len(), 1);
        act(&mut empty, Action::PlayLand(drawn[0]));
        turn(&mut empty, p);
        assert!(empty.players[p.opponent().index()].hand.is_empty());
        let old = tapped(&empty, p);
        priority(&mut empty, p);
        act(&mut empty, Action::ActivateAbility(t, 0));
        next(&mut empty);
        assert_eq!(tapped(&empty, p) - old, 4);
        assert!(empty.objects.get(t).tapped);
        finish(&mut empty);
        assert!(!empty
            .engine
            .event_history
            .has_any(&[EventKindV1::TinybonesV72]));
        validate_state(&restore(&empty)).unwrap();
    }
}
#[test]
fn actual_stash_foreign_spell_pays_original_colored_cost_with_other_mana_and_restores_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for countered in [false, true] {
            let mut s = ready(p, "Forest");
            let tiny = put(&mut s, p, NAME, Zone::Hand);
            let card = put(&mut s, p.opponent(), "Lightning Bolt", Zone::Hand);
            let counter = put(&mut s, p.opponent(), "Counterspell", Zone::Hand);
            paid(&mut s, p, tiny);
            turn(&mut s, p);
            stashed(&mut s, p, tiny, card);
            let original = s.objects.get(card).zone_change_count;
            let before = tapped(&s, p);
            let history = s.engine.event_history.len();
            let row = cast(&mut s, p, card, Some(Target::Player(p.opponent())));
            assert_eq!(tapped(&s, p) - before, 1);
            assert_eq!(row.v4.source_contract.unwrap().owner, p.opponent());
            assert_eq!(row.controller, p);
            assert_eq!(s.objects.get(card).counters.stash, 0);
            assert_eq!(s.counter_lki_for(card, original).unwrap().stash, 1);
            assert!(s.engine.event_history[history..].iter().any(|e|matches!(e,CommittedEvent::ManaAdded{player,colors}if *player==p && !colors.contains(&ManaColor::R))));
            let life = s.players[p.opponent().index()].life;
            if countered {
                let old = tapped(&s, p.opponent());
                cast(&mut s, p.opponent(), counter, Some(Target::Object(card)));
                assert_eq!(tapped(&s, p.opponent()) - old, 2);
            }
            finish(&mut s);
            assert_eq!(
                s.players[p.opponent().index()].life,
                life - if countered { 0 } else { 3 }
            );
            assert_eq!(s.objects.get(card).zone, Zone::Graveyard);
            assert!(s.players[p.opponent().index()].graveyard.contains(&card));
            validate_state(&restore(&s)).unwrap();
        }
    }
}
#[test]
fn paid_source_removal_response_lki_and_different_current_issuer_grants_old_stash_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        let first = put(&mut s, p, NAME, Zone::Hand);
        let second = put(&mut s, p, NAME, Zone::Hand);
        let card = put(&mut s, p.opponent(), "Think Twice", Zone::Hand);
        let bolt = put(&mut s, p.opponent(), "Lightning Bolt", Zone::Hand);
        paid(&mut s, p, first);
        turn(&mut s, p);
        let row = trigger(&mut s, p, first, card);
        let original = row.v4.ability_source_contract.unwrap();
        cast(&mut s, p.opponent(), bolt, Some(Target::Object(first)));
        finish(&mut s);
        assert_eq!(s.objects.get(first).zone, Zone::Graveyard);
        assert_eq!(s.objects.get(card).zone, Zone::Exile);
        assert_eq!(original.controller, p);
        assert!(!play_allowed(&s, card, p));
        paid(&mut s, p, second);
        assert!(play_allowed(&s, card, p));
        let before = tapped(&s, p);
        cast(&mut s, p, card, None);
        assert_eq!(tapped(&s, p) - before, 2);
        finish(&mut s);
        validate_state(&restore(&s)).unwrap();
    }
}
#[test]
fn original_graveyard_incarnation_left_and_returned_is_not_exiled_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut s, tiny, card) = setup(p, "Think Twice");
        trigger(&mut s, p, tiny, card);
        let original = s.objects.get(card).zone_change_count;
        // Explicit native zone-operation primitives after a genuinely paid discard.
        event::propose_and_commit(&mut s, ProposedEvent::zone_change(card, Zone::Hand));
        event::propose_and_commit(&mut s, ProposedEvent::zone_change(card, Zone::Graveyard));
        assert_eq!(s.objects.get(card).zone_change_count, original + 2);
        finish(&mut s);
        assert_eq!(s.objects.get(card).zone, Zone::Graveyard);
        assert_eq!(s.objects.get(card).counters.stash, 0);
        validate_state(&restore(&s)).unwrap();
    }
}
#[test]
fn actual_madness_discard_is_captured_but_later_graveyard_card_is_not_original_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut s, tiny, card) = setup(p, "Fiery Temper");
        trigger(&mut s, p, tiny, card);
        assert_eq!(s.objects.get(card).zone, Zone::Exile);
        finish(&mut s);
        assert_eq!(s.objects.get(card).zone, Zone::Graveyard);
        assert_eq!(s.objects.get(card).counters.stash, 0);
        assert!(s.engine.event_history.iter().any(|e|matches!(e,CommittedEvent::TinybonesV72{journal} if matches!(journal.as_ref(),JournalV72::Discarded(d)if d.card.source==card && d.outcome.expected_zone==Zone::Exile))));
        validate_state(&restore(&s)).unwrap();

        // Real opponent-turn additional discard cost: Madness is active-player
        // controlled and Tinybones is nonactive-player controlled (native APNAP).
        let mut s = ready(p, "Forest");
        let tiny = put(&mut s, p, NAME, Zone::Hand);
        let card = put(&mut s, p.opponent(), "Fiery Temper", Zone::Hand);
        let grab = put(&mut s, p.opponent(), "Grab the Prize", Zone::Hand);
        paid(&mut s, p, tiny);
        turn(&mut s, p.opponent());
        priority(&mut s, p.opponent());
        let before = tapped(&s, p.opponent());
        act(&mut s, Action::CastSpell(grab));
        let mut paid_row = false;
        for _ in 0..64 {
            let d = next(&mut s);
            if s.engine.pending_cast.is_none()
                && s.stack
                    .iter()
                    .any(|r| r.source == grab && r.kind == StackItemKind::Spell)
            {
                paid_row = true;
                break;
            }
            match d {
                Decision::Discard {
                    player,
                    count,
                    choices,
                } => {
                    assert_eq!(player, p.opponent());
                    assert_eq!(count, 1);
                    assert!(choices.contains(&card));
                    act(&mut s, Action::Discard(vec![card]));
                }
                Decision::ChooseSpellMode { .. } => act(&mut s, Action::ChooseSpellMode(0)),
                other => pass(&mut s, other),
            }
        }
        assert!(paid_row);
        assert_eq!(tapped(&s, p.opponent()) - before, 2);
        assert!(s.engine.event_history.iter().any(|e|matches!(e,CommittedEvent::TinybonesV72{journal} if matches!(journal.as_ref(),JournalV72::Discarded(d) if d.card.source==card && d.observers.iter().all(|o|o.controller==p)))));
        finish(&mut s);
        assert_eq!(s.objects.get(card).zone, Zone::Graveyard);
        assert_eq!(s.objects.get(card).counters.stash, 0);
        validate_state(&restore(&s)).unwrap();
    }
}
#[test]
fn actual_progenitus_discard_replacement_shuffles_without_a_stash_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut s, tiny, card) = setup(p, "Progenitus");
        trigger(&mut s, p, tiny, card);
        finish(&mut s);
        assert_eq!(s.objects.get(card).zone, Zone::Library);
        assert!(s.players[p.opponent().index()].library.contains(&card));
        assert_eq!(s.objects.get(card).counters.stash, 0);
        validate_state(&restore(&s)).unwrap();
    }
}
#[test]
fn foreign_stashed_land_keeps_owner_enters_under_grantee_and_uses_normal_quota_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        let tiny = put(&mut s, p, NAME, Zone::Hand);
        let first = put(&mut s, p.opponent(), "Forest", Zone::Hand);
        let second = put(&mut s, p.opponent(), "Island", Zone::Hand);
        paid(&mut s, p, tiny);
        turn(&mut s, p);
        stashed(&mut s, p, tiny, first);
        turn(&mut s, p);
        stashed(&mut s, p, tiny, second);
        priority(&mut s, p);
        assert!(land_drop_candidates(p, &s).contains(&first));
        act(&mut s, Action::PlayLand(first));
        assert_eq!(s.objects.get(first).owner, p.opponent());
        assert_eq!(s.objects.get(first).controller, p);
        assert!(s.players[p.index()].battlefield.contains(&first));
        assert_eq!(s.players[p.index()].lands_played_this_turn, 1);
        assert!(!land_drop_candidates(p, &s).contains(&second));
        let before = s.clone();
        assert!(step(&mut s, Action::PlayLand(second)).is_err());
        assert_eq!(s, before);
        validate_state(&restore(&s)).unwrap();
    }
}
#[test]
fn current_source_control_and_suppression_operands_do_not_bind_to_original_issuer_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut s, tiny, card) = setup(p, "Think Twice");
        stashed(&mut s, p, tiny, card);
        assert!(play_allowed(&s, card, p));
        // Genuine Involuntary Employment changes current control, while the
        // physical stash card remains owned by that new controller.
        let employment = put(&mut s, p.opponent(), "Involuntary Employment", Zone::Hand);
        turn(&mut s, p.opponent());
        cast(&mut s, p.opponent(), employment, Some(Target::Object(tiny)));
        finish(&mut s);
        assert_eq!(s.objects.get(tiny).owner, p);
        assert_eq!(s.objects.get(tiny).controller, p.opponent());
        assert!(!play_allowed(&s, card, p));
        assert!(!play_allowed(&s, card, p.opponent()));
        turn(&mut s, p);
        assert_eq!(s.objects.get(tiny).controller, p);
        assert!(play_allowed(&s, card, p));
        // A real paid Witness Protection removes the printed static ability.
        let witness = put(&mut s, p.opponent(), "Witness Protection", Zone::Hand);
        turn(&mut s, p.opponent());
        cast(&mut s, p.opponent(), witness, Some(Target::Object(tiny)));
        finish(&mut s);
        turn(&mut s, p);
        assert!(!play_allowed(&s, card, p));
        assert!(mana_grant(&s, card, p).is_none());
        validate_state(&restore(&s)).unwrap();
    }
}
#[test]
fn native_trigger_restore_rejects_erased_duplicate_foreign_and_raw_context_inputs_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut s, tiny, card) = setup(p, "Think Twice");
        activate_to_discard(&mut s, p, tiny, card);
        let waiting = restore(&s);
        act(&mut s, Action::Discard(vec![card]));
        let row = trigger_row_after_answer(&mut s);
        let index = publication(&s, &row).unwrap().unwrap();
        let assert_refusal = |mut bad: GameState| {
            let before = bad.clone();
            assert!(step(&mut bad, Action::Pass).is_err());
            assert_eq!(bad, before);
        };
        let mut erased = restore(&s);
        erased
            .stack
            .retain(|r| r.v4.stack_item_id != row.v4.stack_item_id);
        assert_refusal(erased);
        let mut changed = restore(&s);
        if let CommittedEvent::TinybonesV72 { journal } =
            &mut changed.engine.event_history[index as usize]
        {
            if let JournalV72::Published { item, .. } = journal.as_mut() {
                item.controller = p.opponent();
            }
        }
        assert_refusal(changed);
        let mut duplicate = restore(&s);
        duplicate
            .engine
            .event_history
            .push(duplicate.engine.event_history[index as usize].clone());
        assert_refusal(duplicate);
        let mut missing = restore(&s);
        missing.engine.event_history.retain(|e|!matches!(e,CommittedEvent::TinybonesV72{journal}if matches!(journal.as_ref(),JournalV72::Discarded(_))));
        assert_refusal(missing);
        let mut raw = restore(&s);
        let before = raw.clone();
        let ctx = ExecCtx {
            stack_item_id: Some(StackItemId(u64::MAX)),
            ability_source_contract: row.v4.ability_source_contract,
            ..ExecCtx::no_targets(tiny, p)
        };
        effect::execute(row.inline_effect.as_ref().unwrap(), &ctx, &mut raw);
        assert!(raw.engine.halted.is_some());
        raw.engine.halted = before.engine.halted;
        assert_eq!(raw, before);
        let mut replay = waiting;
        act(&mut replay, Action::Discard(vec![card]));
        trigger_row_after_answer(&mut replay);
        assert_eq!(s, replay);
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
    }
}
fn trigger_row_after_answer(s: &mut GameState) -> StackItem {
    for _ in 0..32 {
        let d = next(s);
        if let Some(row) = s
            .stack
            .iter()
            .find(|r| matches!(r.inline_effect, Some(EffectOp::TinybonesStashV72 { .. })))
        {
            return row.clone();
        }
        pass(s, d);
    }
    panic!("native trigger missing");
}
#[test]
fn independent_permission_operand_keeps_any_type_matching_and_native_abort_restores_stash_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut s, tiny, card) = setup(p, "Lightning Bolt");
        stashed(&mut s, p, tiny, card);
        let generation = s.objects.get(card).zone_change_count;
        // Explicit existing exile-permission input primitive, independent of Tinybones.
        turn(&mut s, p.opponent());
        s.engine.exile_play_permissions.push(PlayPermission {
            without_mana_cost: FreeCastV1(false),
            object: card,
            holder: p,
            zone_change_generation: generation,
            play_or_cast: PlayOrCast::Cast,
            expiry: PlayPermissionExpiry::UntilHoldersNextEndStep,
        });
        priority(&mut s, p);
        assert!(!play_allowed(&s, card, p));
        assert!(mana_grant(&s, card, p).is_some());
        let before = tapped(&s, p);
        let row = cast(&mut s, p, card, Some(Target::Player(p.opponent())));
        assert_eq!(tapped(&s, p) - before, 1);
        assert!(matches!(
            row.v4
                .source_contract
                .unwrap()
                .spell_cast_origin
                .unwrap()
                .route,
            SpellCastRouteV4::ExilePermission { .. }
        ));
        finish(&mut s);
        // Separate genuine stashed input. Native abort is a labelled cast primitive.
        let (mut a, tiny, other) = setup(p, "Lightning Bolt");
        stashed(&mut a, p, tiny, other);
        act(&mut a, Action::CastSpell(other));
        let pending = a.engine.pending_cast.clone().unwrap();
        assert_eq!(a.objects.get(other).counters.stash, 0);
        a.engine.pending_cast = None;
        abort_cast(&mut a, pending, CastMethodV4::Normal);
        assert_eq!(a.objects.get(other).zone, Zone::Exile);
        assert_eq!(a.objects.get(other).counters.stash, 1);
        validate_state(&restore(&a)).unwrap();
        assert!(play_allowed(&a, other, p));
    }
}
#[test]
fn independent_prepaid_hidden_worlds_and_stash_counter_forgery_refusals_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut outputs = vec![];
        for hidden in ["Forest", "Imprisoned in the Moon"] {
            let mut s = ready(p, hidden);
            let tiny = put(&mut s, p, NAME, Zone::Hand);
            let card = put(&mut s, p.opponent(), "Think Twice", Zone::Hand);
            paid(&mut s, p, tiny);
            turn(&mut s, p);
            stashed(&mut s, p, tiny, card);
            outputs.push((
                require_legacy(&s, Some(p)).unwrap_err(),
                s.objects.get(card).counters.stash,
                play_allowed(&s, card, p),
                s.players[p.index()].life,
            ));
            let mut changed = restore(&s);
            changed.objects.get_mut(card).counters.stash = 2;
            let before = changed.clone();
            assert!(step(&mut changed, Action::Pass).is_err());
            assert_eq!(changed, before);
            let mut negative = restore(&s);
            negative.objects.get_mut(card).counters.stash = -1;
            let before = negative.clone();
            assert!(step(&mut negative, Action::Pass).is_err());
            assert_eq!(negative, before);
        }
        assert_eq!(outputs[0], outputs[1]);
    }
}

#[test]
fn foreign_paid_adventure_retains_actual_caster_permission_after_stash_and_source_leave_both_seats()
{
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        for _ in 0..8 {
            put(&mut s, p, "Mountain", Zone::Battlefield);
        }
        let tiny = put(&mut s, p, NAME, Zone::Hand);
        let dragon = put(&mut s, p.opponent(), "Fang Dragon", Zone::Hand);
        let bolt = put(&mut s, p.opponent(), "Lightning Bolt", Zone::Hand);
        paid(&mut s, p, tiny);
        turn(&mut s, p);
        stashed(&mut s, p, tiny, dragon);
        let before = tapped(&s, p);
        let row = cast_mode(&mut s, p, dragon, None, 1);
        assert_eq!(row.v4.cast_method, Some(CastMethodV4::Omen));
        assert_eq!(row.v4.source_contract.unwrap().owner, p.opponent());
        assert_eq!(tapped(&s, p) - before, 2);
        finish(&mut s);
        assert_eq!(s.objects.get(dragon).zone, Zone::Exile);
        assert_eq!(s.objects.get(dragon).counters.stash, 0);
        assert!(adventure_allowed(&s, dragon, p));
        assert!(!adventure_allowed(&s, dragon, p.opponent()));
        // Real paid source removal; Adventure permission survives independently.
        cast(&mut s, p.opponent(), bolt, Some(Target::Object(tiny)));
        finish(&mut s);
        assert_eq!(s.objects.get(tiny).zone, Zone::Graveyard);
        priority(&mut s, p);
        assert!(castable_spells(p, &s).contains(&dragon));
        let mut bad = restore(&s);
        let receipt = bad
            .engine
            .event_history
            .iter_mut()
            .find_map(|e| match e {
                CommittedEvent::TinybonesV72 { journal } => match journal.as_mut() {
                    JournalV72::ForeignAdventure { original, .. } => Some(original),
                    _ => None,
                },
                _ => None,
            })
            .unwrap();
        receipt.controller = p.opponent();
        let unchanged = bad.clone();
        assert!(step(&mut bad, Action::Pass).is_err());
        assert_eq!(bad, unchanged);
        // Native illegal-announcement reversal is a labelled primitive, followed
        // by a genuine paid creature cast from its newly rebound exile generation.
        act(&mut s, Action::CastSpell(dragon));
        let pending = s.engine.pending_cast.take().unwrap();
        abort_cast(&mut s, pending, CastMethodV4::Normal);
        validate_state(&restore(&s)).unwrap();
        assert!(adventure_allowed(&s, dragon, p));
        assert!(!adventure_allowed(&s, dragon, p.opponent()));
        let before = tapped(&s, p);
        let row = cast(&mut s, p, dragon, None);
        assert_eq!(row.v4.cast_method, Some(CastMethodV4::Normal));
        assert_eq!(tapped(&s, p) - before, 7);
        finish(&mut s);
        assert_eq!(s.objects.get(dragon).zone, Zone::Battlefield);
        assert_eq!(s.objects.get(dragon).controller, p);
        assert_eq!(s.objects.get(dragon).owner, p.opponent());
        validate_state(&restore(&s)).unwrap();
    }
}

#[test]
fn paid_stash_independent_permission_native_abort_rebinds_restore_and_own_turn_retry_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut s, tiny, card) = setup(p, "Lightning Bolt");
        stashed(&mut s, p, tiny, card);
        let original_generation = s.objects.get(card).zone_change_count;
        turn(&mut s, p.opponent());
        // An independent exile-permission input primitive, not a Tinybones
        // own-turn permission. The following cast announcement is genuine.
        s.engine.exile_play_permissions.push(PlayPermission {
            without_mana_cost: FreeCastV1(false),
            object: card,
            holder: p,
            zone_change_generation: original_generation,
            play_or_cast: PlayOrCast::Cast,
            expiry: PlayPermissionExpiry::UntilHoldersNextEndStep,
        });
        priority(&mut s, p);
        let taps = tapped(&s, p);
        let mana = s.players[p.index()].mana_pool;
        let casts = s.players[p.index()].spells_cast_this_turn;
        act(&mut s, Action::CastSpell(card));
        let pending = s.engine.pending_cast.take().unwrap();
        assert!(
            matches!(pending.source_contract.spell_cast_origin.unwrap().route,SpellCastRouteV4::ExilePermission{holder,..}if holder==p)
        );
        assert_eq!(s.objects.get(card).controller, p);
        assert_eq!(s.objects.get(card).owner, p.opponent());
        assert_eq!(s.objects.get(card).counters.stash, 0);
        // Explicit native illegal-announcement reversal primitive. No public
        // cancellation action or normal abort reachability is claimed here.
        abort_cast(&mut s, pending, CastMethodV4::Normal);
        assert_eq!(s.objects.get(card).zone, Zone::Exile);
        assert_eq!(s.objects.get(card).controller, p.opponent());
        assert_eq!(
            s.objects.get(card).zone_change_count,
            original_generation + 2
        );
        assert_eq!(s.objects.get(card).counters.stash, 1);
        assert_eq!(
            s.counter_lki_for(card, original_generation).unwrap().stash,
            1
        );
        assert_eq!(tapped(&s, p), taps);
        assert_eq!(s.players[p.index()].mana_pool, mana);
        assert_eq!(s.players[p.index()].spells_cast_this_turn, casts);
        validate_state(&restore(&s)).unwrap();
        assert!(mana_grant(&s, card, p).is_some());
        assert!(!play_allowed(&s, card, p));
        // The old independent permission remains generation-bound and cannot
        // be silently revived. Tinybones will grant a fresh own-turn route.
        assert!(!castable_spells(p, &s).contains(&card));
        let mut forged = restore(&s);
        let returned = forged
            .engine
            .event_history
            .iter_mut()
            .find_map(|e| match e {
                CommittedEvent::TinybonesV72 { journal } => match journal.as_mut() {
                    JournalV72::IndependentStashAborted { returned, .. } => Some(returned),
                    _ => None,
                },
                _ => None,
            })
            .unwrap();
        returned.expected_zone_change_count += 1;
        let unchanged = forged.clone();
        assert!(step(&mut forged, Action::Pass).is_err());
        assert_eq!(forged, unchanged);
        let mut missing = restore(&s);
        missing.engine.event_history.retain(|e| !matches!(e,CommittedEvent::TinybonesV72{journal}if matches!(journal.as_ref(),JournalV72::IndependentStashAborted{..})));
        let unchanged = missing.clone();
        assert!(step(&mut missing, Action::Pass).is_err());
        assert_eq!(missing, unchanged);
        let mut restored = restore(&s);
        turn(&mut s, p);
        turn(&mut restored, p);
        assert_eq!(s, restored);
        assert!(play_allowed(&s, card, p));
        let before = tapped(&s, p);
        let row = cast(&mut s, p, card, Some(Target::Player(p.opponent())));
        assert!(matches!(
            row.v4
                .source_contract
                .unwrap()
                .spell_cast_origin
                .unwrap()
                .route,
            SpellCastRouteV4::TinybonesExileV72 { .. }
        ));
        assert_eq!(row.controller, p);
        assert_eq!(row.v4.source_contract.unwrap().owner, p.opponent());
        assert_eq!(tapped(&s, p) - before, 1);
        finish(&mut s);
        assert_eq!(s.objects.get(card).zone, Zone::Graveyard);
        assert!(s.players[p.opponent().index()].graveyard.contains(&card));
        validate_state(&restore(&s)).unwrap();
    }
}

#[path = "tinybones_public_v72_tests.rs"]
mod public_foundation;

#[path = "tinybones_observation_v72_tests.rs"]
pub(crate) mod complete_observation;
