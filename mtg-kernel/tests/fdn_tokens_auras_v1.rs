//! FDN token makers and static creature Auras: definitions, enter/landfall/
//! combat/cast triggers, token-creating spells and attached bonuses.
#![cfg(feature = "limited-fdn-fixtures")]
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardType, Keywords, Subtype, TargetSpec, CARD_DEFS,
};
use mtg_kernel::combat_damage_v1::enable_foundations_combat_v1;
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::CustomDeckV1;
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use mtg_kernel::surface_v2::{
    HarnessSurfaceV2, PriorityModeV1, SuppressionAuditMode, SurfaceAction, SurfaceDecision,
};
use mtg_kernel::trigger;

const CARDS: [&str; 7] = [
    "Dragon Trainer",
    "Resolute Reinforcements",
    "Elfsworn Giant",
    "Eager Trufflesnout",
    "Rite of the Dragoncaller",
    "Heroic Reinforcements",
    "Goblin Surprise",
];
const AURAS: [&str; 2] = ["Twinblade Blessing", "Blanchwood Armor"];
const TOKENS: [&str; 4] = [
    "Dragon Token",
    "Dragon 5/5 Token",
    "Soldier Token",
    "Goblin Token",
];

fn ready(step: Step) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state =
        GameState::new_from_libraries(&[forest; 40], &[forest; 40], |_| "Forest".into(), 123);
    state.step = step;
    enable_foundations_combat_v1(&mut state).unwrap();
    state
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap();
    let id = state.objects.push(GameObject {
        card_def,
        name: name.into(),
        owner: player,
        controller: player,
        zone,
        tapped: false,
        summoning_sick: zone == Zone::Hand,
        damage: 0,
        counters: Default::default(),
        attachments: Vec::new(),
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[player.index()].hand.push(id),
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        _ => panic!("helper zone"),
    }
    id
}

/// Puts a permanent onto the battlefield from hand and queues its triggers.
fn enter(state: &mut GameState, player: PlayerId, name: &str) -> ObjectId {
    let id = put(state, player, name, Zone::Hand);
    event::propose_and_commit(state, ProposedEvent::zone_change(id, Zone::Battlefield));
    let triggers = trigger::collect_and_process(state);
    state.engine.pending_triggers.extend(triggers);
    id
}

fn surface() -> HarnessSurfaceV2 {
    HarnessSurfaceV2::new_with_priority_mode_v1(
        PriorityModeV1::EngineWindowsV1,
        SuppressionAuditMode::Full,
    )
}

fn next(surface: &mut HarnessSurfaceV2, state: &mut GameState) -> Decision {
    match surface.next_decision(state) {
        SurfaceDecision::Decision(decision) => decision,
        other => panic!("expected decision, got {other:?}"),
    }
}

fn apply(surface: &mut HarnessSurfaceV2, state: &mut GameState, action: Action) {
    surface.apply(state, SurfaceAction::Action(action)).unwrap();
}

/// Orders and resolves every pending trigger and stack object until priority
/// returns with an empty stack.
fn drain(surface: &mut HarnessSurfaceV2, state: &mut GameState) {
    for _ in 0..100 {
        match next(surface, state) {
            Decision::OrderTriggers { pending, .. } => apply(
                surface,
                state,
                Action::OrderTriggers((0..pending.len()).collect()),
            ),
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return
            }
            Decision::CastSpellOrPass { .. } => apply(surface, state, Action::Pass),
            other => panic!("unexpected choice: {other:?}"),
        }
    }
    panic!("drain did not finish");
}

/// Casts `spell` from P0's hand with exactly `mana` in the pool, choosing
/// `mode` when the spell is modal, and resolves everything.
fn cast(
    surface: &mut HarnessSurfaceV2,
    state: &mut GameState,
    spell: ObjectId,
    mana: [u8; 6],
    mode: Option<u8>,
) {
    state.players[0].mana_pool = mana;
    assert!(
        matches!(next(surface, state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&spell))
    );
    apply(surface, state, Action::CastSpell(spell));
    if let Some(mode) = mode {
        assert!(matches!(
            next(surface, state),
            Decision::ChooseSpellMode { mode_count: 2, .. }
        ));
        apply(surface, state, Action::ChooseSpellMode(mode));
    }
    drain(surface, state);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
}

fn tokens(state: &GameState, player: PlayerId, name: &str) -> Vec<ObjectId> {
    let card_def = card_id_by_name(name).unwrap();
    state.players[player.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|id| state.objects.get(*id).card_def == card_def)
        .collect()
}

fn stats(state: &GameState, object: ObjectId) -> (i32, i32) {
    (
        engine::effective_power(state, object),
        engine::effective_toughness(state, object),
    )
}

#[test]
fn appended_definitions_match_their_printed_characteristics() {
    let first = card_id_by_name(CARDS[0]).unwrap() as usize;
    for (offset, name) in CARDS.iter().chain(&AURAS).chain(&TOKENS).enumerate() {
        let id = card_id_by_name(name).unwrap() as usize;
        assert_eq!(id, first + offset, "{name}");
        assert_eq!(CARD_DEFS[id].capability, CardCapability::Full, "{name}");
    }
    assert_eq!(
        CARD_DEFS.len(),
        first + CARDS.len() + AURAS.len() + TOKENS.len()
    );
    for (name, mana_value, colors, subtypes, stats, keywords, triggers) in [
        (
            "Dragon Trainer",
            5,
            &[ManaColor::R][..],
            &[Subtype::Human][..],
            Some((1, 1)),
            Keywords::NONE,
            1,
        ),
        (
            "Resolute Reinforcements",
            2,
            &[ManaColor::W][..],
            &[Subtype::Human, Subtype::Soldier][..],
            Some((1, 1)),
            Keywords::FLASH,
            1,
        ),
        (
            "Elfsworn Giant",
            5,
            &[ManaColor::G][..],
            &[Subtype::Giant][..],
            Some((5, 3)),
            Keywords::REACH,
            1,
        ),
        (
            "Eager Trufflesnout",
            3,
            &[ManaColor::G][..],
            &[Subtype::Boar][..],
            Some((4, 2)),
            Keywords::TRAMPLE,
            1,
        ),
        (
            "Rite of the Dragoncaller",
            6,
            &[ManaColor::R][..],
            &[][..],
            None,
            Keywords::NONE,
            1,
        ),
        (
            "Heroic Reinforcements",
            4,
            &[ManaColor::R, ManaColor::W][..],
            &[][..],
            None,
            Keywords::NONE,
            0,
        ),
        (
            "Goblin Surprise",
            3,
            &[ManaColor::R][..],
            &[][..],
            None,
            Keywords::NONE,
            0,
        ),
        (
            "Dragon Token",
            0,
            &[ManaColor::R][..],
            &[Subtype::Dragon][..],
            Some((4, 4)),
            Keywords::FLYING,
            0,
        ),
        (
            "Dragon 5/5 Token",
            0,
            &[ManaColor::R][..],
            &[Subtype::Dragon][..],
            Some((5, 5)),
            Keywords::FLYING,
            0,
        ),
        (
            "Soldier Token",
            0,
            &[ManaColor::W][..],
            &[Subtype::Soldier][..],
            Some((1, 1)),
            Keywords::NONE,
            0,
        ),
        (
            "Goblin Token",
            0,
            &[ManaColor::R][..],
            &[Subtype::Goblin][..],
            Some((1, 1)),
            Keywords::NONE,
            0,
        ),
    ] {
        let id = card_id_by_name(name).unwrap();
        let def = &CARD_DEFS[id as usize];
        assert_eq!(def.mana_value, mana_value, "{name}");
        assert_eq!(def.colors, colors, "{name}");
        assert_eq!(def.subtypes, subtypes, "{name}");
        assert_eq!(
            (def.power, def.toughness),
            stats.map_or((None, None), |(p, t)| (Some(p), Some(t))),
            "{name}"
        );
        assert_eq!(def.keywords, keywords, "{name}");
        assert_eq!(def.is_token, name.ends_with(" Token"), "{name}");
        assert_eq!(trigger::triggers_for(id).len(), triggers, "{name}");
    }
    assert!(
        CARD_DEFS[card_id_by_name("Rite of the Dragoncaller").unwrap() as usize]
            .types
            .contains(&CardType::Enchantment)
    );
    assert!(
        CARD_DEFS[card_id_by_name("Goblin Surprise").unwrap() as usize]
            .mode2
            .is_some()
    );
    assert!(Subtype::CREATURE_TYPES.contains(&Subtype::Boar));
}

#[test]
fn reference_deck_resolves_all_forty_copies() {
    let deck: CustomDeckV1 = serde_json::from_str(
        r#"{"cards":[{"name":"Dragon Trainer","count":1},{"name":"Resolute Reinforcements","count":1},
            {"name":"Elfsworn Giant","count":1},{"name":"Eager Trufflesnout","count":1},
            {"name":"Rite of the Dragoncaller","count":1},{"name":"Heroic Reinforcements","count":1},
            {"name":"Goblin Surprise","count":1},{"name":"Twinblade Blessing","count":1},
            {"name":"Blanchwood Armor","count":1},{"name":"Mountain","count":11},
            {"name":"Plains","count":10},{"name":"Forest","count":10}]}"#,
    )
    .unwrap();
    let ids = deck.resolve().unwrap();
    assert_eq!(ids.len(), 40);
    let expected: Vec<u16> = CARDS
        .iter()
        .chain(&AURAS)
        .map(|n| card_id_by_name(n).unwrap())
        .collect();
    assert_eq!(&ids[..9], &expected[..]);
}

#[test]
fn dragon_trainer_creates_a_four_four_flying_dragon_on_entry() {
    let mut state = ready(Step::Main1);
    let trainer = put(&mut state, PlayerId::P0, "Dragon Trainer", Zone::Hand);
    let mut surface = surface();
    let mut short = [0, 0, 0, 2, 0, 2];
    state.players[0].mana_pool = short;
    assert!(engine::step(&mut state, Action::CastSpell(trainer)).is_err());
    short = [0, 0, 0, 2, 0, 3];
    cast(&mut surface, &mut state, trainer, short, None);
    assert_eq!(state.objects.get(trainer).zone, Zone::Battlefield);
    let dragons = tokens(&state, PlayerId::P0, "Dragon Token");
    assert_eq!(dragons.len(), 1);
    assert_eq!(stats(&state, dragons[0]), (4, 4));
    assert!(engine::has_effective_keyword(
        &state,
        dragons[0],
        Keywords::FLYING
    ));
    assert_eq!(state.objects.get(dragons[0]).controller, PlayerId::P0);
}

#[test]
fn resolute_reinforcements_has_flash_and_creates_a_soldier() {
    let mut state = ready(Step::DeclareAttackers);
    state.engine.combat.attackers_declared = true;
    let reinforcements = put(
        &mut state,
        PlayerId::P0,
        "Resolute Reinforcements",
        Zone::Hand,
    );
    let trainer = put(&mut state, PlayerId::P0, "Dragon Trainer", Zone::Hand);
    let mut surface = surface();
    state.players[0].mana_pool = [0, 0, 0, 2, 0, 3];
    assert!(
        matches!(next(&mut surface, &mut state), Decision::CastSpellOrPass { castable_spells, .. } if !castable_spells.contains(&trainer))
    );
    cast(
        &mut surface,
        &mut state,
        reinforcements,
        [1, 0, 0, 0, 0, 1],
        None,
    );
    assert_eq!(state.objects.get(reinforcements).zone, Zone::Battlefield);
    let soldiers = tokens(&state, PlayerId::P0, "Soldier Token");
    assert_eq!(soldiers.len(), 1);
    assert_eq!(stats(&state, soldiers[0]), (1, 1));
}

#[test]
fn elfsworn_giant_landfall_creates_elf_warriors_only_for_its_controllers_lands() {
    let mut state = ready(Step::Main1);
    put(
        &mut state,
        PlayerId::P0,
        "Elfsworn Giant",
        Zone::Battlefield,
    );
    let mut surface = surface();
    enter(&mut state, PlayerId::P1, "Forest");
    assert!(state.engine.pending_triggers.is_empty());
    enter(&mut state, PlayerId::P0, "Forest");
    drain(&mut surface, &mut state);
    enter(&mut state, PlayerId::P0, "Jungle Hollow");
    drain(&mut surface, &mut state);
    let elves = tokens(&state, PlayerId::P0, "Elf Warrior Token");
    assert_eq!(elves.len(), 2);
    assert_eq!(stats(&state, elves[0]), (1, 1));
    assert!(tokens(&state, PlayerId::P1, "Elf Warrior Token").is_empty());
    // A nonland permanent entering does not trigger landfall.
    enter(&mut state, PlayerId::P0, "Dragon Trainer");
    drain(&mut surface, &mut state);
    assert_eq!(tokens(&state, PlayerId::P0, "Elf Warrior Token").len(), 2);
}

fn trufflesnout_combat(blocker: Option<&str>) -> (GameState, ObjectId, Option<ObjectId>) {
    let mut state = ready(Step::DeclareBlockers);
    state.engine.combat.attackers_declared = true;
    state.engine.combat.blockers_declared = true;
    let boar = put(
        &mut state,
        PlayerId::P0,
        "Eager Trufflesnout",
        Zone::Battlefield,
    );
    let blocker = blocker.map(|name| put(&mut state, PlayerId::P1, name, Zone::Battlefield));
    state.engine.combat.attackers.push(boar);
    state
        .engine
        .combat
        .blocked_by
        .push((boar, blocker.into_iter().collect()));
    (state, boar, blocker)
}

fn enter_damage(state: &mut GameState) {
    for _ in 0..2 {
        assert!(matches!(
            engine::advance_until_decision(state),
            Decision::CastSpellOrPass { .. }
        ));
        engine::step(state, Action::Pass).unwrap();
    }
}

fn assign(state: &mut GameState, amounts: &[(ObjectId, Target, i32)]) {
    for _ in 0..256 {
        match engine::advance_until_decision(state) {
            Decision::ChooseCombatDamageRange {
                source,
                recipient,
                minimum,
                maximum,
                split_at,
                ..
            } => {
                let amount = amounts
                    .iter()
                    .find(|(s, t, _)| *s == source && *t == recipient)
                    .unwrap_or_else(|| panic!("missing damage {source:?} -> {recipient:?}"))
                    .2;
                assert!((minimum..=maximum).contains(&amount));
                engine::step(
                    state,
                    Action::ChooseCombatDamageRange {
                        upper_half: amount > split_at,
                    },
                )
                .unwrap();
            }
            Decision::CastSpellOrPass { .. } => return,
            other => panic!("unexpected damage decision {other:?}"),
        }
    }
    panic!("damage choices did not finish");
}

#[test]
fn trufflesnout_creates_food_when_it_deals_combat_damage_to_a_player() {
    let (mut state, _, _) = trufflesnout_combat(None);
    enter_damage(&mut state);
    assign(&mut state, &[]);
    assert_eq!(state.players[1].life, 16);
    drain(&mut surface(), &mut state);
    assert_eq!(tokens(&state, PlayerId::P0, "Food Token").len(), 1);
}

#[test]
fn trufflesnout_trample_over_a_blocker_still_creates_food() {
    let (mut state, boar, blocker) = trufflesnout_combat(Some("Llanowar Elves"));
    let blocker = blocker.unwrap();
    enter_damage(&mut state);
    assign(
        &mut state,
        &[
            (boar, Target::Object(blocker), 1),
            (boar, Target::Player(PlayerId::P1), 3),
            (blocker, Target::Object(boar), 1),
        ],
    );
    assert_eq!(state.players[1].life, 17);
    drain(&mut surface(), &mut state);
    assert_eq!(tokens(&state, PlayerId::P0, "Food Token").len(), 1);
}

#[test]
fn fully_blocked_trufflesnout_creates_no_food() {
    let (mut state, boar, blocker) = trufflesnout_combat(Some("Elfsworn Giant"));
    let blocker = blocker.unwrap();
    enter_damage(&mut state);
    assign(
        &mut state,
        &[
            (boar, Target::Object(blocker), 4),
            (blocker, Target::Object(boar), 5),
        ],
    );
    drain(&mut surface(), &mut state);
    assert_eq!(state.players[1].life, 20);
    assert!(tokens(&state, PlayerId::P0, "Food Token").is_empty());
}

#[test]
fn rite_creates_a_dragon_for_each_instant_or_sorcery_its_controller_casts() {
    let mut state = ready(Step::Main1);
    put(
        &mut state,
        PlayerId::P0,
        "Rite of the Dragoncaller",
        Zone::Battlefield,
    );
    let mut surface = surface();
    // A creature spell does not trigger it.
    let trainer = put(&mut state, PlayerId::P0, "Dragon Trainer", Zone::Hand);
    cast(&mut surface, &mut state, trainer, [0, 0, 0, 2, 0, 3], None);
    assert!(tokens(&state, PlayerId::P0, "Dragon 5/5 Token").is_empty());
    let surprise = put(&mut state, PlayerId::P0, "Goblin Surprise", Zone::Hand);
    cast(
        &mut surface,
        &mut state,
        surprise,
        [0, 0, 0, 1, 0, 2],
        Some(1),
    );
    let heroic = put(
        &mut state,
        PlayerId::P0,
        "Heroic Reinforcements",
        Zone::Hand,
    );
    cast(&mut surface, &mut state, heroic, [1, 0, 0, 1, 0, 2], None);
    let dragons = tokens(&state, PlayerId::P0, "Dragon 5/5 Token");
    assert_eq!(dragons.len(), 2);
    // The second dragon entered before Heroic Reinforcements resolved, so it
    // is boosted; both have flying.
    for dragon in &dragons {
        assert!(engine::has_effective_keyword(
            &state,
            *dragon,
            Keywords::FLYING
        ));
    }
    assert_eq!(stats(&state, dragons[0]), (6, 6));
    assert_eq!(stats(&state, dragons[1]), (6, 6));
}

#[test]
fn opponents_instant_does_not_trigger_rite() {
    let mut state = ready(Step::Main1);
    put(
        &mut state,
        PlayerId::P1,
        "Rite of the Dragoncaller",
        Zone::Battlefield,
    );
    let surprise = put(&mut state, PlayerId::P0, "Goblin Surprise", Zone::Hand);
    cast(
        &mut surface(),
        &mut state,
        surprise,
        [0, 0, 0, 1, 0, 2],
        Some(1),
    );
    assert!(tokens(&state, PlayerId::P1, "Dragon 5/5 Token").is_empty());
    assert!(tokens(&state, PlayerId::P0, "Dragon 5/5 Token").is_empty());
}

#[test]
fn heroic_reinforcements_boosts_and_hastes_every_controlled_creature_including_new_soldiers() {
    let mut state = ready(Step::Main1);
    let elf = put(&mut state, PlayerId::P0, "Llanowar Elves", Zone::Hand);
    // Summoning sick by construction; Heroic's haste lets it attack.
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(elf, Zone::Battlefield),
    );
    state.objects.get_mut(elf).summoning_sick = true;
    let enemy = put(
        &mut state,
        PlayerId::P1,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let spell = put(
        &mut state,
        PlayerId::P0,
        "Heroic Reinforcements",
        Zone::Hand,
    );
    let mut surface = surface();
    let mut short = [1, 0, 0, 1, 0, 1];
    state.players[0].mana_pool = short;
    assert!(engine::step(&mut state, Action::CastSpell(spell)).is_err());
    short = [1, 0, 0, 1, 0, 2];
    cast(&mut surface, &mut state, spell, short, None);
    assert_eq!(state.objects.get(spell).zone, Zone::Graveyard);
    let soldiers = tokens(&state, PlayerId::P0, "Soldier Token");
    assert_eq!(soldiers.len(), 2);
    for object in soldiers.iter().copied().chain([elf]) {
        assert_eq!(stats(&state, object), (2, 2));
        assert!(engine::has_effective_keyword(
            &state,
            object,
            Keywords::HASTE
        ));
    }
    assert_eq!(stats(&state, enemy), (1, 1));
    assert!(!engine::has_effective_keyword(
        &state,
        enemy,
        Keywords::HASTE
    ));
    state.step = Step::DeclareAttackers;
    state.engine.priority_passes = [false, false];
    let attackers: Vec<ObjectId> = soldiers.iter().copied().chain([elf]).collect();
    assert!(matches!(
        next(&mut surface, &mut state),
        Decision::DeclareAttackers { .. }
    ));
    apply(
        &mut surface,
        &mut state,
        Action::DeclareAttackers(attackers.clone()),
    );
    assert_eq!(state.engine.combat.attackers, attackers);
}

#[test]
fn heroic_reinforcements_bonus_expires_at_cleanup() {
    let mut state = ready(Step::Main1);
    let spell = put(
        &mut state,
        PlayerId::P0,
        "Heroic Reinforcements",
        Zone::Hand,
    );
    let mut surface = surface();
    cast(&mut surface, &mut state, spell, [1, 0, 0, 1, 0, 2], None);
    let soldiers = tokens(&state, PlayerId::P0, "Soldier Token");
    assert_eq!(stats(&state, soldiers[0]), (2, 2));
    state.step = Step::End;
    state.engine.priority_passes = [false, false];
    apply(&mut surface, &mut state, Action::Pass);
    next(&mut surface, &mut state);
    apply(&mut surface, &mut state, Action::Pass);
    next(&mut surface, &mut state);
    assert!(state.engine.until_end_of_turn.is_empty());
    for soldier in soldiers {
        assert_eq!(stats(&state, soldier), (1, 1));
        assert!(!engine::has_effective_keyword(
            &state,
            soldier,
            Keywords::HASTE
        ));
    }
}

#[test]
fn goblin_surprise_pumps_controlled_creatures_or_creates_two_goblins() {
    let mut state = ready(Step::Main1);
    let elf = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let enemy = put(
        &mut state,
        PlayerId::P1,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let mut surface = surface();
    let first = put(&mut state, PlayerId::P0, "Goblin Surprise", Zone::Hand);
    cast(&mut surface, &mut state, first, [0, 0, 0, 1, 0, 2], Some(0));
    assert_eq!(stats(&state, elf), (3, 1));
    assert_eq!(stats(&state, enemy), (1, 1));
    assert!(tokens(&state, PlayerId::P0, "Goblin Token").is_empty());
    let second = put(&mut state, PlayerId::P0, "Goblin Surprise", Zone::Hand);
    cast(
        &mut surface,
        &mut state,
        second,
        [0, 0, 0, 1, 0, 2],
        Some(1),
    );
    let goblins = tokens(&state, PlayerId::P0, "Goblin Token");
    assert_eq!(goblins.len(), 2);
    for goblin in goblins {
        // Created after the first mode resolved, so not pumped.
        assert_eq!(stats(&state, goblin), (1, 1));
        assert!(!engine::has_effective_keyword(
            &state,
            goblin,
            Keywords::HASTE
        ));
    }
    assert_eq!(stats(&state, elf), (3, 1));
}

#[test]
fn goblin_surprise_is_castable_at_instant_speed() {
    let mut state = ready(Step::DeclareAttackers);
    state.engine.combat.attackers_declared = true;
    let spell = put(&mut state, PlayerId::P0, "Goblin Surprise", Zone::Hand);
    cast(
        &mut surface(),
        &mut state,
        spell,
        [0, 0, 0, 1, 0, 2],
        Some(1),
    );
    assert_eq!(tokens(&state, PlayerId::P0, "Goblin Token").len(), 2);
}

#[test]
fn auras_are_creature_auras_with_exact_costs_and_targets() {
    for (name, mana_value, colors, keywords) in [
        (
            "Twinblade Blessing",
            3,
            &[ManaColor::W][..],
            Keywords::FLASH,
        ),
        ("Blanchwood Armor", 3, &[ManaColor::G][..], Keywords::NONE),
    ] {
        let id = card_id_by_name(name).unwrap();
        let def = &CARD_DEFS[id as usize];
        assert_eq!(def.mana_value, mana_value, "{name}");
        assert_eq!(def.colors, colors, "{name}");
        assert_eq!(def.types, &[CardType::Enchantment], "{name}");
        assert_eq!(def.subtypes, &[Subtype::Aura], "{name}");
        assert_eq!(def.target_spec, TargetSpec::Creature, "{name}");
        assert_eq!(def.keywords, keywords, "{name}");
        assert!(def.attachment.unwrap().is_creature_aura(), "{name}");
        assert!(trigger::triggers_for(id).is_empty(), "{name}");
    }
    for (name, mana, legal) in [
        ("Twinblade Blessing", [1, 0, 0, 0, 0, 2], false),
        ("Twinblade Blessing", [2, 0, 0, 0, 0, 1], true),
        ("Blanchwood Armor", [0, 0, 0, 0, 0, 3], false),
        ("Blanchwood Armor", [0, 0, 0, 0, 1, 2], true),
    ] {
        let mut state = ready(Step::Main1);
        put(
            &mut state,
            PlayerId::P1,
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let aura = put(&mut state, PlayerId::P0, name, Zone::Hand);
        state.players[0].mana_pool = mana;
        assert!(
            matches!(next(&mut surface(), &mut state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&aura) == legal),
            "{name} {mana:?}"
        );
    }
}

/// Casts `name` from P0's hand targeting `host` and resolves it.
fn enchant(state: &mut GameState, name: &str, host: ObjectId) -> ObjectId {
    let aura = put(state, PlayerId::P0, name, Zone::Hand);
    state.players[0].mana_pool = [5; 6];
    let mut surface = surface();
    assert!(
        matches!(next(&mut surface, state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&aura))
    );
    apply(&mut surface, state, Action::CastSpell(aura));
    assert!(matches!(
        next(&mut surface, state),
        Decision::ChooseTargets { .. }
    ));
    apply(
        &mut surface,
        state,
        Action::ChooseTarget(Target::Object(host)),
    );
    drain(&mut surface, state);
    state.players[0].mana_pool = [0; 6];
    assert_eq!(state.objects.get(aura).zone, Zone::Battlefield);
    assert!(state.objects.get(host).attachments.contains(&aura));
    aura
}

#[test]
fn twinblade_blessing_has_flash_and_grants_double_strike_while_attached() {
    let mut state = ready(Step::DeclareAttackers);
    state.engine.combat.attackers_declared = true;
    let elf = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let other = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let aura = enchant(&mut state, "Twinblade Blessing", elf);
    assert!(engine::has_effective_keyword(
        &state,
        elf,
        Keywords::DOUBLE_STRIKE
    ));
    assert!(!engine::has_effective_keyword(
        &state,
        other,
        Keywords::DOUBLE_STRIKE
    ));
    assert_eq!(stats(&state, elf), (1, 1));
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(aura, Zone::Graveyard),
    );
    assert!(!engine::has_effective_keyword(
        &state,
        elf,
        Keywords::DOUBLE_STRIKE
    ));
}

#[test]
fn twinblade_blessing_can_enchant_an_opponents_creature() {
    let mut state = ready(Step::Main1);
    let enemy = put(
        &mut state,
        PlayerId::P1,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    enchant(&mut state, "Twinblade Blessing", enemy);
    assert!(engine::has_effective_keyword(
        &state,
        enemy,
        Keywords::DOUBLE_STRIKE
    ));
}

#[test]
fn double_strike_from_twinblade_deals_combat_damage_twice() {
    let mut state = ready(Step::Main1);
    let boar = put(
        &mut state,
        PlayerId::P0,
        "Eager Trufflesnout",
        Zone::Battlefield,
    );
    enchant(&mut state, "Twinblade Blessing", boar);
    state.step = Step::DeclareBlockers;
    state.engine.combat.attackers_declared = true;
    state.engine.combat.blockers_declared = true;
    state.engine.combat.attackers.push(boar);
    state.engine.combat.blocked_by.push((boar, vec![]));
    for _ in 0..30 {
        match engine::advance_until_decision(&mut state) {
            Decision::CastSpellOrPass { .. } if state.step == Step::EndCombat => break,
            Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
            Decision::OrderTriggers { pending, .. } => engine::step(
                &mut state,
                Action::OrderTriggers((0..pending.len()).collect()),
            )
            .unwrap(),
            other => panic!("unexpected combat decision {other:?}"),
        }
    }
    assert_eq!(state.players[1].life, 12);
    // One Food per combat damage step.
    assert_eq!(tokens(&state, PlayerId::P0, "Food Token").len(), 2);
}

#[test]
fn ability_removal_after_twinblade_removes_its_double_strike_but_not_before() {
    let mut state = ready(Step::Main1);
    let first = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    enchant(&mut state, "Twinblade Blessing", first);
    enchant(&mut state, "Witness Protection", first);
    assert!(!engine::has_effective_keyword(
        &state,
        first,
        Keywords::DOUBLE_STRIKE
    ));
    let second = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    enchant(&mut state, "Witness Protection", second);
    enchant(&mut state, "Twinblade Blessing", second);
    assert!(engine::has_effective_keyword(
        &state,
        second,
        Keywords::DOUBLE_STRIKE
    ));
}

#[test]
fn blanchwood_armor_counts_forests_its_controller_controls() {
    let mut state = ready(Step::Main1);
    let elf = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let enemy = put(
        &mut state,
        PlayerId::P1,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    put(&mut state, PlayerId::P0, "Forest", Zone::Battlefield);
    put(&mut state, PlayerId::P0, "Forest", Zone::Battlefield);
    put(&mut state, PlayerId::P0, "Jungle Hollow", Zone::Battlefield);
    put(&mut state, PlayerId::P1, "Forest", Zone::Battlefield);
    enchant(&mut state, "Blanchwood Armor", elf);
    assert_eq!(stats(&state, elf), (3, 3));
    let forest = put(&mut state, PlayerId::P0, "Forest", Zone::Battlefield);
    assert_eq!(stats(&state, elf), (4, 4));
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(forest, Zone::Graveyard),
    );
    assert_eq!(stats(&state, elf), (3, 3));
    // On an opponent's creature it still counts the Aura controller's Forests.
    enchant(&mut state, "Blanchwood Armor", enemy);
    assert_eq!(stats(&state, enemy), (3, 3));
    assert_eq!(stats(&state, elf), (3, 3));
}

#[test]
fn blanchwood_armor_with_no_forests_gives_nothing_and_stacks_with_witness_protection() {
    let mut state = ready(Step::Main1);
    let elf = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    enchant(&mut state, "Blanchwood Armor", elf);
    assert_eq!(stats(&state, elf), (1, 1));
    put(&mut state, PlayerId::P0, "Forest", Zone::Battlefield);
    enchant(&mut state, "Witness Protection", elf);
    // Witness Protection sets base 1/1 (layer 7b); the Armor's +1/+1 is a
    // layer 7c modification from another permanent and still applies.
    assert_eq!(stats(&state, elf), (2, 2));
}

#[test]
fn aura_goes_to_the_graveyard_when_its_host_dies_and_restores_cleanly() {
    let mut state = ready(Step::Main1);
    let elf = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let aura = enchant(&mut state, "Blanchwood Armor", elf);
    let saved = serde_json::to_vec(&state).unwrap();
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(elf, Zone::Graveyard));
    drain(&mut surface(), &mut state);
    assert_eq!(state.objects.get(aura).zone, Zone::Graveyard);
    let mut restored: GameState = serde_json::from_slice(&saved).unwrap();
    assert_eq!(stats(&restored, elf), (1, 1));
    put(&mut restored, PlayerId::P0, "Forest", Zone::Battlefield);
    assert_eq!(stats(&restored, elf), (2, 2));
}
