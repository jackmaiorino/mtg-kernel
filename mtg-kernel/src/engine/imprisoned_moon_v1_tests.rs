//! Genuine casts and native actions; explicit setup/suppression/copy operands
//! are identified in their cases. No free mana pool or fabricated permission.
use super::*;
use crate::{policy_observation_v6::tests::put, state::Step};
const NAME: &str = "Imprisoned in the Moon";
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
        110_484,
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
        d => panic!("Moon native priority {d:?}"),
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

fn spell(s: &mut GameState, p: PlayerId, name: &str, targets: &[Target], cost: usize) -> ObjectId {
    let c = cast(s, p, name, (false, 0, false), targets, None, cost);
    finish(s);
    c
}
fn moon(s: &mut GameState, p: PlayerId, host: ObjectId) -> ObjectId {
    let c = spell(s, p, NAME, &[Target::Object(host)], 3);
    assert_eq!(
        s.objects.get(c).v4.attached_to,
        Some(ObjectLinkV4 {
            object: host,
            zone_change_count: s.objects.get(host).zone_change_count,
        })
    );
    assert!(s.objects.get(host).attachments.contains(&c));
    s.validate_attachment_relations().unwrap();
    c
}
fn only_land(s: &GameState, host: ObjectId) {
    assert!(object_has_type(s, host, CardType::Land));
    for kind in [
        CardType::Creature,
        CardType::Artifact,
        CardType::Enchantment,
        CardType::Planeswalker,
    ] {
        assert!(!object_has_type(s, host, kind), "{kind:?}");
    }
    assert_eq!(object_color_mask(s, host), 0);
    assert!(!crate::continuous_characteristics_v1::printed_abilities_active(s, host));
}
fn replay_finish(s: &mut GameState) {
    let mut r = restore(s);
    finish(s);
    finish(&mut r);
    assert_eq!(*s, r);
}
#[test]
fn paid_creature_actual_land_play_and_full_ajani_targets_preserve_identity_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for kind in 0..3 {
            let mut s = ready(p, "Forest");
            let host = match kind {
                0 => spell(&mut s, p, "Llanowar Elves", &[], 1),
                1 => {
                    priority(&mut s, p);
                    let c = put(&mut s, p, "Forest", Zone::Hand);
                    step(&mut s, Action::PlayLand(c)).unwrap();
                    finish(&mut s);
                    c
                }
                _ => spell(&mut s, p, "Ajani, Caller of the Pride", &[], 3),
            };
            // Native initial noncreature artifact operand is outside the exact union.
            let artifact = put(&mut s, p, "Ravenous Amulet", Zone::Battlefield);
            let before_query = s.clone();
            let targets = legal_targets_for_controller(
                card_def::CARD_DEFS[id(NAME) as usize].target_spec,
                &[],
                p,
                &s,
            );
            assert!(targets.contains(&Target::Object(host)));
            assert!(!targets.contains(&Target::Object(artifact)));
            assert!(!targets.contains(&Target::Player(p)));
            assert_eq!(s, before_query);
            let before = s.objects.get(host).clone();
            let loyalty = crate::planeswalker_v1::loyalty(&s, host);
            let m = cast(
                &mut s,
                p,
                NAME,
                (false, 0, false),
                &[Target::Object(host)],
                None,
                3,
            );
            let row = s.stack.last().unwrap();
            assert_eq!(row.source, m);
            assert_eq!(row.controller, p);
            assert_eq!(row.targets, [Target::Object(host)]);
            assert!(
                matches!(row.v4.target_contracts.as_slice(), [StackTargetContractV4::Object {object,zone:Zone::Battlefield,zone_change_count,..}]
                if *object==host && *zone_change_count==before.zone_change_count)
            );
            replay_finish(&mut s);
            only_land(&s, host);
            let now = s.objects.get(host);
            assert_eq!(
                (
                    now.card_def,
                    now.name.as_str(),
                    now.owner,
                    now.controller,
                    now.zone_change_count,
                    now.v4.face_index
                ),
                (
                    before.card_def,
                    before.name.as_str(),
                    before.owner,
                    before.controller,
                    before.zone_change_count,
                    before.v4.face_index
                )
            );
            assert_eq!(now.counters, before.counters);
            assert_eq!(crate::planeswalker_v1::loyalty(&s, host), loyalty);
            if kind == 1 {
                assert!(has_effective_subtype(&s, host, card_def::Subtype::Forest));
            } else {
                assert!(effective_subtype_ids(&s, host).is_empty());
            }
            if kind == 2 {
                assert!(effective_supertypes(&s, host).contains(&card_def::Supertype::Legendary));
                assert!(!available_activatable_abilities(p, &s)
                    .iter()
                    .any(|(id, _)| *id == host));
            }
            assert_eq!(restore(&s), s);
        }
    }
}
#[test]
fn paid_response_original_target_departure_and_reentry_fizzle_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        let host = spell(&mut s, p, "Llanowar Elves", &[], 1);
        let generation = s.objects.get(host).zone_change_count;
        let m = cast(
            &mut s,
            p,
            NAME,
            (false, 0, false),
            &[Target::Object(host)],
            None,
            3,
        );
        cast(
            &mut s,
            p.opponent(),
            "Lightning Bolt",
            (false, 0, false),
            &[Target::Object(host)],
            None,
            1,
        );
        // Genuine response resolves first; stop at the still-published original Moon.
        for _ in 0..16 {
            let d = next(&mut s);
            if s.objects.get(host).zone == Zone::Graveyard {
                break;
            }
            pass(&mut s, d);
        }
        assert_eq!(s.objects.get(host).zone, Zone::Graveyard);
        // Explicit native reentry primitive, not a claimed paid reanimation.
        event::propose_and_commit(
            &mut s,
            event::ProposedEvent::zone_change(host, Zone::Battlefield),
        );
        assert!(s.objects.get(host).zone_change_count > generation);
        replay_finish(&mut s);
        assert_eq!(s.objects.get(m).zone, Zone::Graveyard);
        assert!(s.objects.get(host).attachments.is_empty());
        assert!(object_has_type(&s, host, CardType::Creature));
        assert_eq!(
            object_color_mask(&s, host),
            card_def::mana_color_mask(ManaColor::G)
        );
    }
}
#[test]
fn paid_witness_then_moon_timestamp_sba_departure_and_new_incarnation_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        let host = spell(&mut s, p, "Llanowar Elves", &[], 1);
        let w = spell(&mut s, p, "Witness Protection", &[Target::Object(host)], 1);
        assert_eq!(effective_name(&s, host), "Legitimate Businessperson");
        let m = moon(&mut s, p, host);
        only_land(&s, host);
        assert_eq!(
            s.objects.get(w).zone,
            Zone::Graveyard,
            "Witness cannot enchant only a land"
        );
        assert_eq!(effective_name(&s, host), "Llanowar Elves");
        assert!(!has_effective_subtype(&s, host, card_def::Subtype::Citizen));
        let old = s.objects.get(m).zone_change_count;
        spell(
            &mut s,
            p.opponent(),
            "Broken Wings",
            &[Target::Object(m)],
            3,
        );
        assert!(object_has_type(&s, host, CardType::Creature));
        assert!(crate::continuous_characteristics_v1::printed_abilities_active(&s, host));
        // Native zone-return operand followed by a second genuine paid cast.
        event::propose_and_commit(&mut s, event::ProposedEvent::zone_change(m, Zone::Hand));
        pay(
            &mut s,
            p,
            m,
            (false, 0, false),
            &[Target::Object(host)],
            None,
        );
        replay_finish(&mut s);
        assert!(s.objects.get(m).zone_change_count > old);
        only_land(&s, host);
        assert_eq!(s.objects.get(host).attachments, [m]);
    }
}
#[test]
fn paid_copy_before_override_is_copiable_original_and_later_native_animation_layers_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        let host = spell(&mut s, p, "Llanowar Elves", &[], 1);
        spell(&mut s, p, "Self-Reflection", &[Target::Object(host)], 6);
        let copy = *s.players[p.index()]
            .battlefield
            .iter()
            .find(|id| {
                s.objects.get(**id).card_def == id_of_elf() && s.objects.get(**id).v4.is_token
            })
            .unwrap();
        moon(&mut s, p, host);
        only_land(&s, host);
        assert!(object_has_type(&s, copy, CardType::Creature));
        assert_eq!(checked_effective_pt(&s, copy).unwrap(), (1, 1));
        assert!(s.objects.get(copy).attachments.is_empty());
        // Native layer operand, no claim of an absent paid land-animation printing.
        let timestamp = next_timestamp(&mut s);
        s.objects
            .get_mut(host)
            .v4
            .creature_upgrade
            .get_or_insert_with(Default::default)
            .temporary_creature = Some((4, 4, timestamp));
        assert!(object_has_type(&s, host, CardType::Land));
        assert!(object_has_type(&s, host, CardType::Creature));
        let w = spell(&mut s, p, "Witness Protection", &[Target::Object(host)], 1);
        assert_eq!(s.objects.get(w).zone, Zone::Battlefield);
        assert!(object_has_type(&s, host, CardType::Creature));
        assert!(!object_has_type(&s, host, CardType::Land));
        assert_eq!(
            object_color_mask(&s, host),
            card_def::mana_colors_mask(&[ManaColor::G, ManaColor::W])
        );
        assert!(!crate::continuous_characteristics_v1::land_grants_colorless_mana(&s, host));
        assert_eq!(checked_effective_pt(&s, host).unwrap(), (1, 1));
        assert_eq!(restore(&s), s);
    }
}
fn id_of_elf() -> u16 {
    id("Llanowar Elves")
}
#[test]
fn paid_young_creature_new_colorless_manual_and_automatic_mana_control_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        let host = spell(&mut s, p, "Llanowar Elves", &[], 1);
        assert!(s.objects.get(host).summoning_sick);
        moon(&mut s, p, host);
        priority(&mut s, p);
        assert_eq!(available_mana_ability_choices(p, host, &s), [ManaColor::C]);
        assert!(available_mana_ability_choices(p.opponent(), host, &s).is_empty());
        let mut r = restore(&s);
        step(&mut s, Action::ActivateManaAbility(host)).unwrap();
        step(&mut r, Action::ActivateManaAbility(host)).unwrap();
        assert_eq!(s, r);
        assert!(s.objects.get(host).tapped);
        assert_eq!(s.players[p.index()].mana_pool, [0, 0, 0, 0, 0, 1]);
        assert!(available_mana_ability_choices(p, host, &s).is_empty());
        // Existing real generic cast spends floating C plus one real land tap.
        spell(&mut s, p, "Ravenous Amulet", &[], 1);
        // A separate automatic-payment scene forces C from the granted ability.
        let mut s = ready(p, "Forest");
        let host = spell(&mut s, p, "Llanowar Elves", &[], 1);
        moon(&mut s, p, host);
        let blue = *s.players[p.index()]
            .battlefield
            .iter()
            .find(|object| {
                s.objects.get(**object).card_def == id("Island") && !s.objects.get(**object).tapped
            })
            .unwrap();
        // Explicit native resource-exhaustion setup, not a paid tap effect.
        for c in s.players[p.index()].battlefield.clone() {
            if c != host && c != blue {
                s.objects.get_mut(c).tapped = true;
            }
        }
        let plan = mana::can_pay(
            &Cost {
                generic: 1,
                pips: &[mana::Pip::Colored(ManaColor::U)],
                x_count: 0,
            },
            0,
            p,
            &s,
        )
        .unwrap();
        assert_eq!(plan.taps.len(), 2);
        assert!(plan.taps.contains(&(host, ManaColor::C)));
        spell(&mut s, p, "Think Twice", &[], 2);
        assert!(s.objects.get(host).tapped && s.objects.get(blue).tapped);
        assert_eq!(restore(&s), s);
        // Native control/untap operands verify grant follows current controller.
        crate::standard_cards_v1::gain_control(&mut s, host, p.opponent());
        s.objects.get_mut(host).tapped = false;
        assert!(available_mana_ability_choices(p, host, &s).is_empty());
        assert_eq!(
            available_mana_ability_choices(p.opponent(), host, &s),
            [ManaColor::C]
        );
        assert_eq!(restore(&s), s);
    }
}
#[test]
fn paid_full_native_counter_and_neutral_face_preservation_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        let host = spell(&mut s, p, "Ajani, Caller of the Pride", &[], 3);
        let loyalty = crate::planeswalker_v1::loyalty(&s, host);
        // Explicit native counter operands, not claimed paid counter placement.
        s.objects.get_mut(host).counters.plus1_plus1 = i32::MAX;
        s.objects.get_mut(host).counters.fellowship = i32::MAX;
        let original = s.objects.get(host).clone();
        moon(&mut s, p, host);
        only_land(&s, host);
        assert_eq!(s.objects.get(host).counters, original.counters);
        assert_eq!(crate::planeswalker_v1::loyalty(&s, host), loyalty);
        assert_eq!(
            (
                s.objects.get(host).name.as_str(),
                s.objects.get(host).card_def,
                s.objects.get(host).v4.face_index
            ),
            (
                original.name.as_str(),
                original.card_def,
                original.v4.face_index
            )
        );
        let mut neutral = ready(p, "Forest");
        let c = spell(&mut neutral, p, "Llanowar Elves", &[], 1);
        // Explicit native neutral face-down operand. Moon changes characteristics
        // in later layers without turning the physical object face up.
        let face = crate::state::FaceDownV1 {
            disguised: true,
            lookers: 0,
            hidden_by: None,
            hidden_by_source: None,
        };
        neutral.objects.get_mut(c).v4.face_down_v1 = Some(face);
        priority(&mut neutral, p);
        let face = neutral.objects.get(c).v4.face_down_v1.unwrap();
        moon(&mut neutral, p, c);
        only_land(&neutral, c);
        assert_eq!(neutral.objects.get(c).v4.face_down_v1, Some(face));
        assert_eq!(effective_name(&neutral, c), "");
        assert_eq!(restore(&neutral), neutral);
        let mut width = ready(p, "Forest");
        let host = spell(&mut width, p, "Llanowar Elves", &[], 1);
        let aura = moon(&mut width, p, host);
        // Explicit native full-width counter operand on a noncreature land.
        width.objects.get_mut(host).counters.plus1_plus1 = i32::MAX;
        let before = width.clone();
        event::propose_and_commit(
            &mut width,
            event::ProposedEvent::zone_change(aura, Zone::Graveyard),
        );
        assert!(width.engine.halted.is_some());
        width.engine.halted = before.engine.halted;
        assert_eq!(
            width, before,
            "revealed out-of-envelope PT must refuse before the zone mutation"
        );
    }
}
#[test]
fn paid_malformed_attachment_readonly_and_independent_hidden_worlds_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut a = ready(p, NAME);
        let mut b = ready(p, "Island");
        let ca = spell(&mut a, p, "Llanowar Elves", &[], 1);
        let cb = spell(&mut b, p, "Llanowar Elves", &[], 1);
        assert_eq!(ca, cb);
        assert!(crate::continuous_characteristics_v1::refuse_land_override_consumer(&a, p).is_ok());
        assert!(crate::continuous_characteristics_v1::refuse_land_override_consumer(&b, p).is_ok());
        let ma = moon(&mut a, p, ca);
        let mb = moon(&mut b, p, cb);
        assert_eq!(ma, mb);
        assert_ne!(a.state_hash(), b.state_hash());
        assert_eq!(effective_subtype_ids(&a, ca), effective_subtype_ids(&b, cb));
        assert_eq!(
            available_mana_ability_choices(p, ca, &a),
            available_mana_ability_choices(p, cb, &b)
        );
        assert_eq!(object_color_mask(&a, ca), object_color_mask(&b, cb));
        assert_eq!(a.objects.get(ca), b.objects.get(cb));
        for edit in 0..4 {
            let mut bad = restore(&a);
            match edit {
                0 => {
                    bad.objects
                        .get_mut(ma)
                        .v4
                        .attached_to
                        .as_mut()
                        .unwrap()
                        .zone_change_count += 1
                }
                1 => bad.objects.get_mut(ca).attachments.clear(),
                2 => bad.objects.get_mut(ca).attachments.push(ma),
                _ => bad.objects.get_mut(ma).v4.attached_to = None,
            }
            let before = bad.clone();
            assert!(bad.validate_attachment_relations().is_err());
            assert_eq!(bad, before);
            assert!(!crate::continuous_characteristics_v1::land_grants_colorless_mana(&bad, ca));
            let d = advance_until_decision(&mut bad);
            assert!(matches!(d, Decision::Halted { .. }));
            bad.engine.halted = before.engine.halted;
            assert_eq!(bad, before);
        }
        // The legacy observation still refuses the real new Partial printing.
        let mut surface = crate::policy_surface_v5::PolicySurfaceV5::new();
        surface.next_decision(&mut a).unwrap();
        let before = a.clone();
        assert!(crate::rl::observe_policy_v6(&a, &surface, p, 0, 0, 0, 1).is_err());
        assert_eq!(a, before);
    }
}

#[test]
fn paid_replication_copied_moon_season_real_host_choice_and_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        let elf = spell(&mut s, p, "Llanowar Elves", &[], 1);
        let original = moon(&mut s, p, elf);
        spell(&mut s, p, "Doubling Season", &[], 5);
        spell(&mut s, p, "Extravagant Replication", &[], 6);
        let mut targeted = false;
        let mut attached = 0;
        for _ in 0..160 {
            let d = next(&mut s);
            match d {
                Decision::ChooseTargets {
                    player,
                    ref legal_targets,
                    ..
                } => {
                    assert_eq!(player, p);
                    assert!(!targeted);
                    assert!(legal_targets.contains(&Target::Object(original)));
                    let mut r = restore(&s);
                    step(&mut s, Action::ChooseTarget(Target::Object(original))).unwrap();
                    step(&mut r, Action::ChooseTarget(Target::Object(original))).unwrap();
                    assert_eq!(s, r);
                    targeted = true;
                }
                Decision::ChooseEffectTargets {
                    player,
                    ref legal_targets,
                    ..
                } if targeted => {
                    assert_eq!(player, p);
                    assert!(legal_targets.contains(&Target::Object(elf)));
                    let mut r = restore(&s);
                    step(&mut s, Action::ChooseEffectTarget(Target::Object(elf))).unwrap();
                    step(&mut r, Action::ChooseEffectTarget(Target::Object(elf))).unwrap();
                    assert_eq!(s, r);
                    attached += 1;
                }
                Decision::CastSpellOrPass { .. }
                    if targeted && s.stack.is_empty() && s.engine.pending_effect.is_none() =>
                {
                    break
                }
                _ => pass(&mut s, d),
            }
        }
        assert!(targeted);
        assert_eq!(attached, 2);
        let tokens = s.players[p.index()]
            .battlefield
            .iter()
            .copied()
            .filter(|c| s.objects.get(*c).card_def == id(NAME) && s.objects.get(*c).v4.is_token)
            .collect::<Vec<_>>();
        assert_eq!(tokens.len(), 2);
        for c in tokens {
            assert_eq!(
                s.objects.get(c).v4.attached_to,
                s.objects.get(original).v4.attached_to
            );
        }
        only_land(&s, elf);
        assert_eq!(s.objects.get(elf).attachments.len(), 3);
        s.validate_attachment_relations().unwrap();
        assert_eq!(restore(&s), s);
    }
}
