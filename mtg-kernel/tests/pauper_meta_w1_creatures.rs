//! Focused rules coverage for four pauper-meta wave 1 triggered creatures:
//! Kessig Flamebreather, Gixian Infiltrator, Webweaver Changeling, and
//! Glint Hawk.
//!
//! The bounded rules oracle is the Mage fork at `72a08a3b`:
//! `KessigFlamebreather.java` blob
//! `5d75485ae885426fd2a55076b8c45f681be20fe4`, `GixianInfiltrator.java`
//! blob `741efdfca7731e5b09e70375ebdbfe54ea0b5b42`, `WebweaverChangeling.java`
//! blob `393acd489453ed8db4050163b4d39d0a339652d9`, `GlintHawk.java` blob
//! `6d46fa629cf74045519a092ba0f976ef835d643f`. Together they establish:
//! Kessig Flamebreather deals 1 damage to each opponent whenever you cast a
//! noncreature spell; Gixian Infiltrator gets a +1/+1 counter whenever you
//! sacrifice another permanent; Webweaver Changeling is a changeling with
//! reach that gains you 5 life on ETB if there are three or more creature
//! cards in your graveyard (intervening if, rechecked on resolution); Glint
//! Hawk has flying and, on ETB, is sacrificed unless you return an artifact
//! you control to its owner's hand.

use mtg_kernel::card_def::{card_id_by_name, CardType, Keywords, Subtype, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{Counters, GameObject, GameState, Step, Target, Zone};

fn card_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

fn card_name(card_def: u16) -> String {
    CARD_DEFS[card_def as usize].name.to_string()
}

// Copied verbatim from `tests/deep_analysis.rs:24-60` / `tests/pauper_meta_w1_spells.rs`.
fn put_object(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id(name);
    let id = state.objects.push(GameObject {
        card_def,
        name: name.to_string(),
        owner: player,
        controller: player,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Counters::default(),
        attachments: Vec::new(),
        v4: mtg_kernel::state::ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[player.index()].hand.push(id),
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        Zone::Library => state.players[player.index()].library.push(id),
        Zone::Graveyard => state.players[player.index()].graveyard.push(id),
        Zone::Exile => state.exile.push(id),
        Zone::Command => state.command.push(id),
        Zone::Stack => panic!("test helper does not construct stack objects"),
    }
    id
}

/// Mirrors `pauper_meta_w1_spells.rs`'s `ready_main1`: Main1, P0 active with
/// priority, both libraries as given.
fn ready_main1(p0_library: &[&str], p1_library: &[&str]) -> GameState {
    let p0_defs = p0_library.iter().map(|name| card_id(name)).collect::<Vec<_>>();
    let p1_defs = p1_library.iter().map(|name| card_id(name)).collect::<Vec<_>>();
    let mut state =
        GameState::new_from_libraries(&p0_defs, &p1_defs, card_name, 0x43_52_45_41_54_55_52_45);
    state.step = Step::Main1;
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    state
}

/// `pass_until_stack_len(state, 0)`, copied from `pauper_meta_w1_spells.rs`.
/// `pass_until_stack_len` from `pauper_meta_w1_spells.rs`, extended to also
/// drive Ponder-shaped resumable-effect choices (reorder the revealed
/// cards in the order offered, then decline the optional shuffle) so a
/// noncreature spell with its own interactive resolution can fully resolve
/// like any other, not just the ordinary priority-pass path.
fn pass_until_stack_len(state: &mut GameState, wanted: usize) -> Decision {
    for _ in 0..64 {
        let decision = engine::advance_until_decision(state);
        if state.stack.len() <= wanted {
            return decision;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::ChooseEffectTargets { legal_targets, .. } => {
                let target = *legal_targets
                    .first()
                    .expect("a legal target remains to select");
                engine::step(state, Action::ChooseEffectTarget(target)).unwrap();
            }
            Decision::ChooseEffectBoolean { .. } => {
                engine::step(state, Action::ChooseEffectBoolean(false)).unwrap();
            }
            Decision::GameOver { .. } | Decision::Halted { .. } => return decision,
            other => panic!("unexpected decision while resolving the stack: {other:?}"),
        }
    }
    panic!("stack did not reach length {wanted}")
}

fn pass_until_stack_empty(state: &mut GameState) -> Decision {
    pass_until_stack_len(state, 0)
}

/// Passes at every `CastSpellOrPass` decision until a different decision (or
/// a terminal one) is reached. Copied from `pauper_meta_w1_spells.rs`.
fn pass_until_next_decision(state: &mut GameState) -> Decision {
    loop {
        let decision = engine::advance_until_decision(state);
        match decision {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => return other,
        }
    }
}

/// Passes with both players until `creature`'s own casting spell has
/// resolved (entered the battlefield) and its ETB trigger has been placed
/// on the stack in its place, stopping right there -- one priority round
/// before the trigger itself resolves.
fn pass_until_etb_trigger_is_pending(state: &mut GameState, creature: ObjectId) {
    for _ in 0..16 {
        match engine::advance_until_decision(state) {
            Decision::CastSpellOrPass { .. } => {
                if state.objects.get(creature).zone == Zone::Battlefield && state.stack.len() == 1
                {
                    return;
                }
                engine::step(state, Action::Pass).unwrap();
            }
            other => panic!("unexpected decision while resolving the ETB: {other:?}"),
        }
    }
    panic!("ETB trigger did not reach the stack within the bounded priority walk");
}

// covers: Kessig Flamebreather: noncreature_spell_cast_damages_each_opponent, creature_spell_cast_no_trigger
#[test]
fn kessig_flamebreather_pings_each_opponent_on_noncreature_casts_only() {
    let mut state = ready_main1(&["Mountain"; 8], &["Mountain"; 8]);
    put_object(&mut state, PlayerId::P0, "Kessig Flamebreather", Zone::Battlefield);
    let ponder = put_object(&mut state, PlayerId::P0, "Ponder", Zone::Hand);
    let miscreant = put_object(&mut state, PlayerId::P0, "Faerie Miscreant", Zone::Hand);
    let spellbomb = put_object(&mut state, PlayerId::P0, "Nihil Spellbomb", Zone::Hand);
    let starting_life = state.players[1].life;
    // Plenty of blue floating mana covers every cost below ({U}, {U}, {1}):
    // blue pays its own colored pips and any generic portion.
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 10;

    engine::step(&mut state, Action::CastSpell(ponder)).unwrap();
    pass_until_stack_empty(&mut state);
    assert_eq!(
        state.players[1].life,
        starting_life - 1,
        "Ponder is a noncreature spell"
    );

    engine::step(&mut state, Action::CastSpell(miscreant)).unwrap();
    pass_until_stack_empty(&mut state);
    assert_eq!(
        state.players[1].life,
        starting_life - 1,
        "casting a creature spell doesn't trigger Kessig Flamebreather"
    );

    engine::step(&mut state, Action::CastSpell(spellbomb)).unwrap();
    pass_until_stack_empty(&mut state);
    assert_eq!(
        state.players[1].life,
        starting_life - 2,
        "Nihil Spellbomb is a noncreature (artifact) spell"
    );
}

// covers: Gixian Infiltrator: counter_on_sacrifice_of_another_permanent, self_sacrifice_does_not_trigger
#[test]
fn gixian_infiltrator_grows_when_another_permanent_is_sacrificed() {
    let mut state = ready_main1(&["Island"; 8], &["Island"; 8]);
    let gixian = put_object(&mut state, PlayerId::P0, "Gixian Infiltrator", Zone::Battlefield);
    let blood = put_object(&mut state, PlayerId::P0, "Blood Token", Zone::Battlefield);
    // The single other card in hand makes Blood Token's mandatory discard
    // auto-resolve (a real choice would need `Decision::Discard`, out of
    // scope here -- the same shortcut `discard_cost_activation_pays_every_
    // component_in_release_builds` in `engine.rs` uses).
    put_object(&mut state, PlayerId::P0, "Lightning Bolt", Zone::Hand);
    state.players[0].mana_pool[ManaColor::C.pool_index()] = 1;

    engine::step(&mut state, Action::ActivateAbility(blood, 0)).unwrap();
    pass_until_stack_empty(&mut state);

    assert_eq!(state.objects.get(blood).zone, Zone::Graveyard);
    assert_eq!(
        state.objects.get(gixian).counters.plus1_plus1,
        1,
        "Gixian Infiltrator gets a +1/+1 counter when another permanent is sacrificed"
    );

    // Gixian Infiltrator being sacrificed itself doesn't trigger its own
    // ability (the trigger requires *another* permanent, and its home zone
    // gate requires the source to still be on the battlefield): sacrifice
    // it as Fanatical Offering's additional cost (the sole
    // artifact-or-creature on the battlefield, so the sacrifice choice
    // auto-resolves) and confirm the resolution completes cleanly with no
    // halted mechanic and nothing left to receive a counter.
    let offering = put_object(&mut state, PlayerId::P0, "Fanatical Offering", Zone::Hand);
    state.players[0].mana_pool[ManaColor::B.pool_index()] = 1;
    state.players[0].mana_pool[ManaColor::C.pool_index()] = 1;
    engine::step(&mut state, Action::CastSpell(offering)).unwrap();
    pass_until_stack_empty(&mut state);
    assert_eq!(state.objects.get(gixian).zone, Zone::Graveyard);
    assert!(
        state.engine.halted.is_none(),
        "no spurious self-trigger should halt the game"
    );
}

// covers: Webweaver Changeling: etb_no_life_gain_below_three_creature_cards, etb_gains_five_life_with_three_or_more_creature_cards, intervening_if_rechecked_on_resolution
#[test]
fn webweaver_changeling_gains_five_only_with_three_creature_cards_in_graveyard() {
    // Two creature cards: the intervening if fails at trigger time, so no
    // ETB trigger is even queued -- no life gain.
    let mut two_cards = ready_main1(&["Forest"; 8], &["Forest"; 8]);
    for _ in 0..2 {
        put_object(&mut two_cards, PlayerId::P0, "Faerie Seer", Zone::Graveyard);
    }
    let webweaver = put_object(&mut two_cards, PlayerId::P0, "Webweaver Changeling", Zone::Hand);
    let starting_life = two_cards.players[0].life;
    two_cards.players[0].mana_pool[ManaColor::G.pool_index()] = 2;
    two_cards.players[0].mana_pool[ManaColor::C.pool_index()] = 3;
    engine::step(&mut two_cards, Action::CastSpell(webweaver)).unwrap();
    pass_until_stack_empty(&mut two_cards);
    assert_eq!(
        two_cards.players[0].life, starting_life,
        "only two creature cards in the graveyard: no life gain"
    );

    // Three creature cards: the intervening if holds both at trigger time
    // and on resolution -- 5 life gained.
    let mut three_cards = ready_main1(&["Forest"; 8], &["Forest"; 8]);
    for _ in 0..3 {
        put_object(&mut three_cards, PlayerId::P0, "Faerie Seer", Zone::Graveyard);
    }
    let webweaver =
        put_object(&mut three_cards, PlayerId::P0, "Webweaver Changeling", Zone::Hand);
    let starting_life = three_cards.players[0].life;
    three_cards.players[0].mana_pool[ManaColor::G.pool_index()] = 2;
    three_cards.players[0].mana_pool[ManaColor::C.pool_index()] = 3;
    engine::step(&mut three_cards, Action::CastSpell(webweaver)).unwrap();
    pass_until_stack_empty(&mut three_cards);
    assert_eq!(
        three_cards.players[0].life,
        starting_life + 5,
        "three creature cards in the graveyard: gain 5 life"
    );

    // Three creature cards at trigger time, but one is exiled (via Relic of
    // Progenitus's "exile one" ability, targeting P0's own graveyard) in
    // response, before the trigger resolves: the intervening if is
    // rechecked on resolution and now fails -- no life gain.
    let mut rechecked = ready_main1(&["Forest"; 8], &["Forest"; 8]);
    for _ in 0..3 {
        put_object(&mut rechecked, PlayerId::P0, "Faerie Seer", Zone::Graveyard);
    }
    let relic = put_object(&mut rechecked, PlayerId::P0, "Relic of Progenitus", Zone::Battlefield);
    let webweaver = put_object(&mut rechecked, PlayerId::P0, "Webweaver Changeling", Zone::Hand);
    let starting_life = rechecked.players[0].life;
    rechecked.players[0].mana_pool[ManaColor::G.pool_index()] = 2;
    rechecked.players[0].mana_pool[ManaColor::C.pool_index()] = 3;

    engine::step(&mut rechecked, Action::CastSpell(webweaver)).unwrap();
    pass_until_etb_trigger_is_pending(&mut rechecked, webweaver);
    assert_eq!(
        rechecked.stack.last().map(|item| item.source),
        Some(webweaver),
        "Webweaver Changeling's own ETB trigger is on top of the stack"
    );

    engine::step(&mut rechecked, Action::ActivateAbility(relic, 0)).unwrap();
    engine::advance_until_decision(&mut rechecked);
    engine::step(
        &mut rechecked,
        Action::ChooseTarget(Target::Player(PlayerId::P0)),
    )
    .unwrap();
    let chosen = match pass_until_next_decision(&mut rechecked) {
        Decision::ChooseEffectTargets { legal_targets, .. } => {
            assert_eq!(legal_targets.len(), 3, "all three graveyard cards are candidates");
            legal_targets[0]
        }
        other => panic!("expected ChooseEffectTargets, got {other:?}"),
    };
    engine::step(&mut rechecked, Action::ChooseEffectTarget(chosen)).unwrap();

    pass_until_stack_empty(&mut rechecked);
    assert_eq!(
        rechecked.players[0].life, starting_life,
        "the intervening if is rechecked on resolution: the count dropped to 2"
    );
}

// covers: Webweaver Changeling: changeling_counts_as_every_creature_type
#[test]
fn webweaver_changeling_has_every_creature_type() {
    let id = card_id("Webweaver Changeling");
    let def = &CARD_DEFS[id as usize];
    assert!(def.changeling, "Webweaver Changeling is a changeling");
    assert!(def.has_type(CardType::Creature));
    assert_eq!(def.keywords, Keywords::REACH);

    // Elvish Mystic's own controlled-Elf count treats a changeling as
    // every creature type, same query an Elves tribal count uses.
    let mut state = ready_main1(&["Forest"; 8], &["Forest"; 8]);
    let webweaver = put_object(&mut state, PlayerId::P0, "Webweaver Changeling", Zone::Battlefield);
    let elf_count = state.players[0]
        .battlefield
        .iter()
        .filter(|&&id| {
            engine::effective_subtype_ids(&state, id)
                .binary_search(&Subtype::Elf.stable_id())
                .is_ok()
        })
        .count();
    assert_eq!(elf_count, 1, "the changeling counts as an Elf");
    let _ = webweaver;
}

// covers: Glint Hawk: etb_sacrificed_with_no_artifact_to_return, etb_accept_returns_artifact_keeps_hawk, etb_decline_sacrifices_hawk_even_with_artifact_available
#[test]
fn glint_hawk_is_sacrificed_unless_an_artifact_is_returned() {
    // No artifact to return: the ETB resolves by sacrificing the Hawk, with
    // no decision offered at all (nothing is payable).
    let mut no_artifact = ready_main1(&["Island"; 8], &["Island"; 8]);
    let hawk = put_object(&mut no_artifact, PlayerId::P0, "Glint Hawk", Zone::Hand);
    no_artifact.players[0].mana_pool[ManaColor::W.pool_index()] = 1;
    engine::step(&mut no_artifact, Action::CastSpell(hawk)).unwrap();
    pass_until_stack_empty(&mut no_artifact);
    assert_eq!(
        no_artifact.objects.get(hawk).zone,
        Zone::Graveyard,
        "sacrificed: no artifact to return"
    );

    // With an artifact: accepting returns it and keeps the Hawk.
    let mut accepted = ready_main1(&["Island"; 8], &["Island"; 8]);
    let hawk = put_object(&mut accepted, PlayerId::P0, "Glint Hawk", Zone::Hand);
    let wellspring =
        put_object(&mut accepted, PlayerId::P0, "Ichor Wellspring", Zone::Battlefield);
    accepted.players[0].mana_pool[ManaColor::W.pool_index()] = 1;
    engine::step(&mut accepted, Action::CastSpell(hawk)).unwrap();
    match pass_until_next_decision(&mut accepted) {
        Decision::ChooseOptionalCost {
            player,
            return_permanent_payable,
            ..
        } => {
            assert_eq!(player, PlayerId::P0);
            assert!(return_permanent_payable);
        }
        other => panic!("expected ChooseOptionalCost, got {other:?}"),
    }
    engine::step(
        &mut accepted,
        Action::ChooseOptionalCost(OptionalCostChoice::ReturnPermanent),
    )
    .unwrap();
    pass_until_stack_empty(&mut accepted);
    assert!(
        accepted.players[0].hand.contains(&wellspring),
        "Ichor Wellspring returned to hand"
    );
    assert_eq!(
        accepted.objects.get(hawk).zone,
        Zone::Battlefield,
        "Glint Hawk stays"
    );

    // Declining still sacrifices the Hawk, even though returning was legal.
    let mut declined = ready_main1(&["Island"; 8], &["Island"; 8]);
    let hawk = put_object(&mut declined, PlayerId::P0, "Glint Hawk", Zone::Hand);
    let wellspring =
        put_object(&mut declined, PlayerId::P0, "Ichor Wellspring", Zone::Battlefield);
    declined.players[0].mana_pool[ManaColor::W.pool_index()] = 1;
    engine::step(&mut declined, Action::CastSpell(hawk)).unwrap();
    pass_until_next_decision(&mut declined);
    engine::step(
        &mut declined,
        Action::ChooseOptionalCost(OptionalCostChoice::Decline),
    )
    .unwrap();
    pass_until_stack_empty(&mut declined);
    assert_eq!(
        declined.objects.get(hawk).zone,
        Zone::Graveyard,
        "declined: Glint Hawk is sacrificed"
    );
    assert_eq!(
        declined.objects.get(wellspring).zone,
        Zone::Battlefield,
        "Ichor Wellspring stays on the battlefield when declined"
    );
}
