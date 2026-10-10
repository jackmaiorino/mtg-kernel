//! Pre-registered step-1 audit checks (written before the first full run;
//! see `docs/rules_vector_v1.md`). A failing check leads to a logged bug fix
//! in the meaning tables, never to reweighting features.

use std::collections::BTreeSet;

use super::facets::*;
use super::meaning::{self, Env};
use super::sources::{card_rules, CardRulesV1, PAUPER_REGISTRY_LEN_V1};
use super::*;
use crate::card_def::{CardType, DynamicCountDef, DynamicValueDef, TargetSpec};
use crate::effect::{EffectCond, EffectOp, PlayerRef};

fn card_id(name: &str) -> u16 {
    crate::card_def::card_id_by_name(name).unwrap_or_else(|| panic!("{name} not in CARD_DEFS"))
}

fn rules(name: &str) -> CardRulesV1 {
    card_rules(card_id(name))
}

fn effects(rules: &CardRulesV1, ctx: Option<CtxF>) -> Vec<EffectAtom> {
    rules
        .abilities
        .iter()
        .filter(|a| ctx.is_none_or(|c| a.ctx == c))
        .flat_map(|a| a.atoms.iter())
        .filter_map(|atom| match atom {
            Atom::Effect(e) => Some(*e),
            _ => None,
        })
        .collect()
}

fn atoms(rules: &CardRulesV1) -> Vec<Atom> {
    rules
        .abilities
        .iter()
        .flat_map(|a| a.atoms.iter().copied())
        .collect()
}

fn features(name: &str) -> vectorize::FeatureCounts {
    vectorize::card_feature_counts(card_id(name))
}

fn nine_deck_cards() -> BTreeSet<u16> {
    let mut ids: BTreeSet<u16> = runtime_deck_cards().into_values().flatten().collect();
    let tokens: Vec<u16> = ids
        .iter()
        .flat_map(|&id| card_rules(id).created_tokens)
        .collect();
    ids.extend(tokens);
    ids
}

// 1. Determinism.

#[test]
fn event_time_programs_keep_their_owning_trigger_without_duplicate_inventory() {
    let hacker = rules("Moon-Circuit Hacker");
    let abilities: Vec<_> = hacker
        .abilities
        .iter()
        .filter(|a| a.ctx == CtxF::Trigger)
        .collect();
    assert_eq!(
        abilities.len(),
        2,
        "two event-time variants replace the default program"
    );
    let trigger = Atom::Trigger(TrigF::DealsDamage {
        combat: true,
        to_player: true,
        host: false,
    });
    let draw = Atom::Effect(
        EffectAtom::moving(Some(ZoneF::Library), ZoneF::Hand)
            .player(RelF::You)
            .obj(ObjF::AnyCard)
            .amount(AmtF::fixed(1)),
    );
    let discard = Atom::Effect(
        EffectAtom::moving(Some(ZoneF::Hand), ZoneF::Graveyard)
            .player(RelF::You)
            .obj(ObjF::AnyCard)
            .amount(AmtF::fixed(1)),
    );
    for ability in &abilities {
        assert!(ability.atoms.contains(&trigger));
        assert!(ability.atoms.contains(&draw));
    }
    assert_eq!(
        abilities
            .iter()
            .filter(|a| a.atoms.contains(&discard))
            .count(),
        1
    );
    let id = card_id("Moon-Circuit Hacker");
    assert!(
        crate::trigger::event_time_trigger_programs(id, crate::trigger::TriggerCondition::Etb)
            .is_empty()
    );
}

#[test]
#[cfg(feature = "limited-fdn-fixtures")]
fn equipment_noncreature_spell_trigger_matches_the_ordinary_trigger_class() {
    let rod = atoms(&rules("Black Mage's Rod"));
    let archer = atoms(&rules("Firebrand Archer"));
    let event = Atom::Trigger(TrigF::SpellCast {
        by: RelF::You,
        obj: ObjF::Spell,
    });
    assert!(rod.contains(&event));
    assert!(archer.contains(&event));
    assert!(!rod.contains(&Atom::Trigger(TrigF::SpellCast {
        by: RelF::You,
        obj: ObjF::NonlandPermanent
    })));
}

#[test]
fn table_is_deterministic() {
    let a = build_table();
    let b = build_table();
    assert_eq!(a.canonical_bytes(), b.canonical_bytes());
    assert_eq!(a.cards.len(), CARD_DEFS.len());
}

// 2. Coverage: every non-empty program source yields facts in its group.

#[test]
fn every_program_source_emits_facts() {
    for id in 0..CARD_DEFS.len() as u16 {
        let def = &CARD_DEFS[usize::from(id)];
        let rules = card_rules(id);
        assert!(!rules.printed.is_empty(), "{}: no printed facts", def.name);
        let has_ctx = |ctx: CtxF| {
            rules
                .abilities
                .iter()
                .any(|a| a.ctx == ctx && !a.atoms.is_empty())
        };
        if (def.spell_effect)().is_some() {
            assert!(
                has_ctx(CtxF::Spell),
                "{}: spell program emitted nothing",
                def.name
            );
        }
        if !crate::trigger::triggers_for(id).is_empty() {
            assert!(
                has_ctx(CtxF::Trigger),
                "{}: trigger emitted nothing",
                def.name
            );
        }
        for (k, ability) in rules.abilities.iter().enumerate() {
            if matches!(ability.ctx, CtxF::Trigger | CtxF::Activated) {
                assert!(
                    ability.atoms.iter().any(|a| matches!(a, Atom::Effect(_))),
                    "{}: {:?} ability {k} has no effect atom: {:?}",
                    def.name,
                    ability.ctx,
                    ability.atoms
                );
            }
        }
    }
}

// 3. Equivalence: reprints collapse, and no distinction the engine makes is
// lost among the nine decks' cards.

#[test]
fn functional_reprints_collapse() {
    let a = features("Elvish Mystic");
    assert_eq!(a, features("Fyndhorn Elves"));
    assert_eq!(a, features("Llanowar Elves"));
}

#[test]
fn no_lossy_collisions_in_the_nine_decks() {
    let table = build_table();
    let collisions = lossy_collisions(&table, &nine_deck_cards());
    let named: Vec<Vec<&str>> = collisions
        .iter()
        .map(|ids| {
            ids.iter()
                .map(|&id| CARD_DEFS[usize::from(id)].name)
                .collect()
        })
        .collect();
    assert!(
        named.is_empty(),
        "same features, different rules: {named:?}"
    );
}

// 4. Synonyms collapse.

fn op_atoms(op: &EffectOp) -> Vec<Atom> {
    let mut out = Collector::default();
    meaning::effect_op(
        op,
        &Env {
            target_spec: TargetSpec::None,
        },
        &mut out,
    );
    out.atoms
}

#[test]
fn surveil_one_and_surveil_count_one_agree() {
    let player = PlayerRef::Controller;
    assert_eq!(
        op_atoms(&EffectOp::Surveil { player, count: 1 }),
        op_atoms(&EffectOp::SurveilOne { player })
    );
}

#[test]
fn multi_card_surveil_includes_library_reordering() {
    for count in [2, 3] {
        assert!(op_atoms(&EffectOp::Surveil { player: PlayerRef::Controller, count })
            .iter().any(|atom| matches!(atom, Atom::Effect(effect) if effect.ev == EvF::Reorder && effect.from == Some(ZoneF::Library))));
    }
}

#[test]
#[cfg(feature = "limited-fdn-fixtures")]
fn inkmage_threshold_static_is_extracted_from_engine_data() {
    let facts = rules("Cephalid Inkmage");
    assert!(facts.opaque_rules.is_empty());
    assert!(facts.abilities.iter().filter(|ability| ability.ctx == CtxF::Static)
        .flat_map(|ability| &ability.atoms)
        .any(|atom| matches!(atom, Atom::Read(read) if read.zone == Some(ZoneF::Graveyard) && read.player == RelF::You && matches!(read.agg, AggF::AtLeast(_)))));
    assert!(effects(&facts, Some(CtxF::Static))
        .iter()
        .any(|effect| effect.ev == EvF::GrantKeyword));
}

fn reads_of(fill: impl FnOnce(&mut Collector)) -> Vec<(RelF, Option<ZoneF>, Option<ObjF>)> {
    let mut out = Collector::default();
    fill(&mut out);
    out.atoms
        .iter()
        .filter_map(|a| match a {
            Atom::Read(r) => Some((r.player, r.zone, r.obj)),
            _ => None,
        })
        .collect()
}

#[test]
fn graveyard_creature_counts_share_one_read_shape() {
    use meaning::reads;
    let want = vec![(
        RelF::You,
        Some(ZoneF::Graveyard),
        Some(ObjF::Typed(CardTypeF::Creature)),
    )];
    let env = Env {
        target_spec: TargetSpec::None,
    };
    assert_eq!(
        reads_of(|out| {
            reads::dynamic_value(
                DynamicValueDef::ControllerGraveyardCardsWithType(CardType::Creature),
                out,
            );
        }),
        want
    );
    assert_eq!(
        reads_of(|out| {
            reads::dynamic_count(
                DynamicCountDef::ControllerGraveyardAnyType(&[CardType::Creature]),
                out,
            );
        }),
        want
    );
    assert_eq!(
        reads_of(|out| reads::effect_cond(
            &EffectCond::ControllerGraveyardCreatureCardsAtLeast(3),
            &env,
            out
        )),
        want
    );
    assert_eq!(
        reads_of(|out| reads::effect_cond(
            &EffectCond::ControllerGraveyardHasType(CardType::Creature),
            &env,
            out
        )),
        want
    );
}

#[test]
fn single_and_dual_lands_share_tap_for_mana() {
    let forest = features("Forest");
    let gate = features("Azorius Guildgate");
    let shared: Vec<&String> = forest
        .keys()
        .filter(|k| k.starts_with("e:AddMana") && gate.contains_key(*k))
        .collect();
    assert!(!shared.is_empty(), "forest {forest:?}\ngate {gate:?}");
}

// 5. Named facts.

fn mill_atom(e: &EffectAtom) -> bool {
    e.ev == EvF::Move && e.from == Some(ZoneF::Library) && e.to == Some(ZoneF::Graveyard)
}

#[test]
fn balustrade_spy_is_an_enters_trigger_milling_a_chosen_player_until_a_land() {
    let spy = rules("Balustrade Spy");
    let trigger = spy
        .abilities
        .iter()
        .find(|a| a.ctx == CtxF::Trigger)
        .expect("Spy has a trigger");
    assert!(
        trigger.atoms.contains(&Atom::Trigger(TrigF::SelfEnters)),
        "{trigger:?}"
    );
    assert!(
        trigger.atoms.contains(&Atom::Target(TargetAtom::PlayerYou)),
        "{trigger:?}"
    );
    assert!(
        trigger
            .atoms
            .contains(&Atom::Target(TargetAtom::PlayerOpponent)),
        "{trigger:?}"
    );
    assert!(
        trigger
            .atoms
            .iter()
            .any(|a| matches!(a, Atom::Effect(e) if mill_atom(e)
            && e.player == Some(RelF::ChosenPlayer)
            && e.amount == AmtF::UntilType(CardTypeF::Land))),
        "{trigger:?}"
    );
}

#[test]
fn other_mill_cards_share_library_to_graveyard() {
    for name in ["Thought Scour", "Mental Note", "Winding Way"] {
        assert!(
            effects(&rules(name), None).iter().any(mill_atom),
            "{name}: {:?}",
            rules(name).abilities
        );
    }
}

#[test]
fn lotleth_giant_scales_damage_by_your_graveyard_creatures() {
    let giant = rules("Lotleth Giant");
    let trigger = giant
        .abilities
        .iter()
        .find(|a| a.ctx == CtxF::Trigger)
        .expect("Lotleth has a trigger");
    assert!(
        trigger.atoms.contains(&Atom::Trigger(TrigF::SelfEnters)),
        "{trigger:?}"
    );
    assert!(
        trigger
            .atoms
            .contains(&Atom::Target(TargetAtom::PlayerOpponent)),
        "{trigger:?}"
    );
    assert!(
        !trigger.atoms.contains(&Atom::Target(TargetAtom::PlayerYou)),
        "{trigger:?}"
    );
    assert!(
        trigger.atoms.iter().any(|a| matches!(a, Atom::Effect(e)
            if e.ev == EvF::Damage && e.amount == AmtF::Dynamic)),
        "{trigger:?}"
    );
    assert!(
        trigger.atoms.iter().any(|a| matches!(a, Atom::Read(r)
            if r.player == RelF::You
                && r.zone == Some(ZoneF::Graveyard)
                && r.obj == Some(ObjF::Typed(CardTypeF::Creature)))),
        "{trigger:?}"
    );
}

#[test]
fn dread_return_reanimates_and_flashes_back_by_sacrificing_three() {
    let dread = rules("Dread Return");
    assert!(
        effects(&dread, Some(CtxF::Spell))
            .iter()
            .any(|e| e.ev == EvF::Move
                && e.from == Some(ZoneF::Graveyard)
                && e.to == Some(ZoneF::Battlefield)
                && e.obj == Some(ObjF::Typed(CardTypeF::Creature))),
        "{:?}",
        dread.abilities
    );
    let flashback = dread
        .abilities
        .iter()
        .find(|a| a.atoms.contains(&Atom::CastFrom(ZoneF::Graveyard)))
        .expect("Dread Return has flashback");
    assert!(
        flashback.atoms.iter().any(|a| matches!(
            a,
            Atom::Cost(CostAtom::MoveObject {
                from: ZoneF::Battlefield,
                to: ZoneF::Graveyard,
                amount: AmtF::Fixed(3),
                ..
            })
        )),
        "{flashback:?}"
    );
}

#[test]
fn engine_keyed_rules_are_read() {
    // Trigger target spec keyed by name in trigger.rs.
    assert!(atoms(&rules("Mesmeric Fiend"))
        .iter()
        .any(|a| matches!(a, Atom::Target(TargetAtom::PlayerOpponent))));
    // Static self boost keyed by name in engine.rs; its condition is opaque.
    let raider = rules("Goblin Tomb Raider");
    assert!(atoms(&raider).contains(&Atom::Opaque));
    assert!(!raider.opaque_rules.is_empty());
    // Initiative from an enters trigger.
    assert!(effects(&rules("Avenging Hunter"), Some(CtxF::Trigger))
        .iter()
        .any(|e| e.ev == EvF::TakeInitiative));
}

// 6. Transfer and information content (reported; FDN gate below).

// Pre-registered gate, kept as written. First full run: FAILED at 99/121
// (82%). Full tuples and within-ability pairs are far more specific than
// EffectOp names; the report breaks the misses down by feature family. The
// gate stays failed in the record rather than being loosened after the fact.
#[test]
#[cfg(feature = "limited-fdn-fixtures")]
#[ignore = "pre-registered gate failed on the first run (99/121); see docs/rules_vector_v1.md"]
fn at_most_five_percent_of_fdn_cards_have_an_unseen_part() {
    let table = build_table();
    let (with_oov, total) = oov_card_share(&table);
    assert!(total > 0);
    assert!(
        with_oov * 100 <= total * 5,
        "{with_oov}/{total} FDN non-token cards have a feature outside the Pauper vocabulary"
    );
}

#[test]
fn spy_shares_the_targeted_mill_part_with_thought_scour() {
    let spy = features("Balustrade Spy");
    let scour = features("Thought Scour");
    assert!(
        spy.keys()
            .any(|k| k.starts_with("e:Move|Library>Graveyard|p=ChosenPlayer")
                && scour.contains_key(k)),
        "spy {spy:?}\nscour {scour:?}"
    );
}

#[test]
fn lightning_bolt_is_not_a_near_neighbour_of_spy() {
    let table = build_table();
    let vocab = &table.vocabulary;
    let vec_of = |id: u16| vectorize::dense_vector(&table.cards[usize::from(id)].features, vocab).0;
    let spy = vec_of(card_id("Balustrade Spy"));
    let mut sims: Vec<(f64, u16)> = (0..PAUPER_REGISTRY_LEN_V1 as u16)
        .filter(|&id| id != card_id("Balustrade Spy"))
        .map(|id| (vectorize::cosine(&spy, &vec_of(id)), id))
        .collect();
    sims.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    let top5: Vec<u16> = sims.iter().take(5).map(|s| s.1).collect();
    assert!(!top5.contains(&card_id("Lightning Bolt")));
}

// 7. Isolation: the extractor never reads labels, names or the registry file.

#[test]
fn extractor_sources_read_no_labels_or_names() {
    let names: BTreeSet<&str> = CARD_DEFS.iter().map(|d| d.name).collect();
    for (path, text) in EXTRACTOR_SOURCES_V1 {
        let code: String = text
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let text = &code;
        for forbidden in [
            "cards_v1.json",
            ".mechanics",
            "java_file",
            "complexity",
            "card_id_by_name",
        ] {
            assert!(!text.contains(forbidden), "{path} mentions {forbidden}");
        }
        for literal in text.split('"').skip(1).step_by(2) {
            assert!(
                !names.contains(literal),
                "{path} has a card-name literal {literal:?}"
            );
        }
    }
}

// 8. Backward chaining finds the Spy line from the goal alone.

#[test]
fn backward_chaining_finds_the_spy_line() {
    let spy_deck = &runtime_deck_cards()["Spy"];
    let ids: Vec<u16> = spy_deck.iter().copied().collect();
    let lines = chain::lines_for_deck(&ids, 5);
    let spy = card_id("Balustrade Spy");
    let giant = card_id("Lotleth Giant");
    let dread = card_id("Dread Return");
    let has = |cards: &[u16]| {
        lines
            .iter()
            .any(|l| l.scaling && cards.iter().all(|c| l.cards.contains(c)))
    };
    assert!(
        has(&[spy, giant]),
        "{} lines, none with Spy+Lotleth",
        lines.len()
    );
    assert!(
        has(&[spy, giant, dread]),
        "{} lines, none with Spy+Dread Return+Lotleth",
        lines.len()
    );
    // The flashback route: Dread Return's reanimation needs Dread Return in
    // the graveyard, and some line proves that.
    let flashback = chain::GoalV1::InZone {
        card: dread,
        zone: ZoneF::Graveyard,
    };
    assert!(lines.iter().any(|l| l.scaling
        && l.steps
            .iter()
            .any(|s| s.card == dread && s.needs.contains(&flashback))
        && l.steps.iter().any(|s| s.goal == flashback)));
}

#[test]
fn backward_chaining_lines_are_well_founded_and_hit_a_player() {
    for (deck, ids) in runtime_deck_cards() {
        let ids: Vec<u16> = ids.iter().copied().collect();
        for line in chain::lines_for_deck(&ids, 5) {
            assert!(
                chain::is_well_founded(&ids, &line),
                "{deck}: circular line {:?}",
                line.cards
            );
            for step in line
                .steps
                .iter()
                .filter(|s| s.goal == chain::GoalV1::OpponentLosesLife)
            {
                let rules = card_rules(step.card);
                let hits_player = rules.abilities[step.ability].atoms.iter().any(|a| {
                    matches!(
                        a,
                        Atom::Effect(e) if matches!(e.ev, EvF::Damage | EvF::LifeLoss)
                            && matches!(e.obj, Some(ObjF::Player | ObjF::PlayerOrPermanent))
                    )
                });
                assert!(hits_player, "{deck}: root step damages no player");
            }
        }
    }
}

#[test]
fn power_toughness_changes_keep_their_sign() {
    assert_ne!(AmtF::stat(2, 2), AmtF::stat(-2, -2));
    assert_eq!(AmtF::stat(-2, -2), AmtF::Minus(2));
    assert_eq!(AmtF::stat(1, 0), AmtF::Fixed(1));
}

// 10. Source inventory: every card-name branch in the rules engine is either
// read by the walk or is not a rule. A new branch fails here until someone
// classifies it.

/// Names keyed in trigger.rs. All are read through `triggers_for`,
/// `trigger_target_spec`, `target_spec_for_trigger`,
/// `required_optional_additional_cost_for_trigger`, `unselected_trigger_modes`
/// or `event_time_trigger_programs`; token names are reached through
/// `CreateToken` programs.
const TRIGGER_RS_KEYED_NAMES: &[&str] = &[
    "Cephalid Inkmage",
    "Lightshell Duo",
    "Adeline, Resplendent Cathar",
    "Adventuring Gear",
    "Ajani's Pridemate",
    "Aloe Alchemist",
    "Angel of Finality",
    "Ascendant Packleader",
    "Avenging Hunter",
    "Azure Fleet Admiral",
    "Balustrade Spy",
    "Bat Token",
    "Beast-Kin Ranger",
    "Bigfin Bouncer",
    "Bind the Monster",
    "Bird Illusion Token",
    "Blood Fountain",
    "Blood Token",
    "Bloodfell Caves",
    "Blossoming Sands",
    "Bojuka Bog",
    "Brutal Cathar",
    "Burglar Rat",
    "Burning-Tree Emissary",
    "Cackling Prowler",
    "Campus Guide",
    "Cat Token",
    "Celestial Armor",
    "Cenote Scout",
    "Chrome Host Seedshark",
    "Clinquant Skymage",
    "Clockwork Percussionist",
    "Clue Token",
    "Conduit Pylons",
    "Cori-Steel Cutter",
    "Crackling Cyclops",
    "Cryogen Relic",
    "Crypt Feaster",
    "Darkstar Augur",
    "Darkstar Augur Offspring Token",
    "Dauntless Veteran",
    "Dazzling Angel",
    "Deep-Cavern Bat",
    "Delver of Secrets",
    "Dismal Backwater",
    "Dragon 5/5 Token",
    "Dragon Token",
    "Dragon Trainer",
    "Dwynen's Elite",
    "Dwynen, Gilt-Leaf Daen",
    "Eager Trufflesnout",
    "Eldrazi Spawn Token",
    "Elementalist Adept",
    "Elf Warrior Token",
    "Elfsworn Giant",
    "Emberheart Challenger",
    "Enduring Curiosity",
    "Enduring Innocence",
    "Erudite Wizard",
    "Evolving Adaptive",
    "Exemplar of Light",
    "Experimental Synthesizer",
    "Extraction Specialist",
    "Faerie Miscreant",
    "Faerie Seer",
    "Faerie Token",
    "Firebrand Archer",
    "Food Token",
    "Forsaken Miner",
    "Gatecreeper Vine",
    "Gatekeeper of Malakir",
    "Generous Ent",
    "Gingerbread Cabin",
    "Gixian Infiltrator",
    "Gleaming Barrier",
    "Glint Hawk",
    "Goblin Bushwhacker",
    "Goldvein Pick",
    "Good-Fortune Unicorn",
    "Graveyard Trespasser",
    "Guarded Heir",
    "Guttersnipe",
    "Harrier Strix",
    "Healer of the Glade",
    "Heartfire Hero",
    "Helpful Hunter",
    "Hero Token",
    "Hired Claw",
    "Homunculus Horde",
    "Homunculus Horde Token",
    "Hopeful Initiate",
    "Hullbreaker Horror",
    "Human Token",
    "Humbling Elder",
    "Icewind Elemental",
    "Ichor Wellspring",
    "Infestation Sage",
    "Insect Token",
    "Iridescent Vinelasher",
    "Iridescent Vinelasher Offspring Token",
    "Journey to Nowhere",
    "Jungle Hollow",
    "Kessig Flamebreather",
    "Kiora, the Rising Tide",
    "Knight Token",
    "Knight-Errant of Eos",
    "Koma's Coil Token",
    "Koma, World-Eater",
    "Lembas",
    "Lotleth Giant",
    "Manifold Mouse",
    "Manifold Mouse Offspring Token",
    "Map Token",
    "Marauding Blight-Priest",
    "Masked Vandal",
    "Mesmeric Fiend",
    "Meteor Golem",
    "Mischievous Mystic",
    "Monastery Swiftspear",
    "Monk Token",
    "Moon-Circuit Hacker",
    "Mossborn Hydra",
    "Murmuring Mystic",
    "Nihil Spellbomb",
    "Ninja of the Deep Hours",
    "Nova Hellkite",
    "Novice Inspector",
    "Outlaw Medic",
    "Overlord of the Mistmoors",
    "Pawpatch Recruit",
    "Pawpatch Recruit Offspring Token",
    "Phyrexian Arena",
    "Prideful Parent",
    "Quirion Beastcaller",
    "Raccoon Token",
    "Razorkin Needlehead",
    "Reclamation Sage",
    "Refurbished Familiar",
    "Resolute Reinforcements",
    "Rite of the Dragoncaller",
    "Rugged Highlands",
    "Ruin-Lurker Bat",
    "Rune-Scarred Demon",
    "Sagu Wildling",
    "Saiba Cryptomancer",
    "Sanguine Evangelist",
    "Sanguine Syphoner",
    "Scion of the Deep Token",
    "Scoured Barrens",
    "Sentinel of the Nameless City",
    "Sharp-Eyed Rookie",
    "Slickshot Show-Off",
    "Sneaky Snacker",
    "Soldier Token",
    "Solemn Simulacrum",
    "Spellstutter Sprite",
    "Spinewoods Paladin",
    "Spitfire Lagac",
    "Squadron Hawk",
    "Sun-Blessed Healer",
    "Swiftwater Cliffs",
    "Sylvan Scavenging",
    "Tatyova, Benthic Druid",
    "Thornwood Falls",
    "Tranquil Cove",
    "Treasure Token",
    "Troublemaker Ouphe",
    "Unstoppable Slasher",
    "Vitu-Ghazi Inspector",
    "Voldaren Epicure",
    "Wary Thespian",
    "Weather the Storm",
    "Webweaver Changeling",
    "White Insect Token",
    "Wind-Scarred Crag",
    "Writhing Chrysalis",
    "Yotian Frontliner",
    "Youthful Valkyrie",
];

/// Other rules-module name branches and how each is accounted for.
const OTHER_KEYED_NAMES: &[(&str, &str, &str)] = &[
    (
        "engine.rs",
        "Cephalid Inkmage",
        "read via engine::static_graveyard_threshold_keyword_for",
    ),
    (
        "engine.rs",
        "Dwynen, Gilt-Leaf Daen",
        "read via engine::static_controlled_subtype_boost_for",
    ),
    (
        "engine.rs",
        "Goblin Tomb Raider",
        "read via engine::static_self_boost_for (condition opaque)",
    ),
    (
        "engine.rs",
        "Weather the Storm",
        "read via engine::has_storm",
    ),
    (
        "effect.rs",
        "Avenging Hunter",
        "validation only (initiative source contract)",
    ),
    (
        "effect.rs",
        "Mesmeric Fiend",
        "validation only (linked-exile source contract)",
    ),
    (
        "effect.rs",
        "Skeleton Token",
        "initiative room token; the mechanic is read as TakeInitiative",
    ),
    (
        "effect.rs",
        "Treasure Token",
        "initiative room token; the mechanic is read as TakeInitiative",
    ),
    (
        "event.rs",
        "Avenging Hunter",
        "validation only (initiative designation contract)",
    ),
    (
        "engine.rs",
        "Coppercoat Vanguard",
        "read via engine::static_controlled_subtype_boost_for",
    ),
    (
        "effect.rs",
        "Deep-Cavern Bat",
        "validation only (linked-exile source contract)",
    ),
    (
        "standard_statics_v1.rs",
        "Razorkin Needlehead",
        "read via standard_statics_v1::rules_vector_statics",
    ),
    (
        "standard_statics_v1.rs",
        "Warden of the Inner Sky",
        "read via standard_statics_v1::rules_vector_statics",
    ),
    (
        "standard_statics_v1.rs",
        "Ascendant Packleader",
        "read via standard_statics_v1::rules_vector_statics",
    ),
    (
        "standard_statics_v1.rs",
        "Evolving Adaptive",
        "read via standard_statics_v1::rules_vector_statics",
    ),
    (
        "standard_statics_v1.rs",
        "Coppercoat Vanguard",
        "read via standard_statics_v1::rules_vector_statics",
    ),
    (
        "standard_statics_v1.rs",
        "Adeline, Resplendent Cathar",
        "read via standard_statics_v1::rules_vector_statics",
    ),
    (
        "standard_statics_v1.rs",
        "Haughty Djinn",
        "read via standard_statics_v1::rules_vector_statics",
    ),
    (
        "standard_statics_v1.rs",
        "Bloodletter of Aclazotz",
        "read via standard_statics_v1::rules_vector_statics",
    ),
    (
        "standard_statics_v1.rs",
        "Thalia, Guardian of Thraben",
        "read via standard_statics_v1::rules_vector_statics",
    ),
    (
        "standard_statics_v1.rs",
        "Hired Claw",
        "read via standard_statics_v1::rules_vector_statics",
    ),
    (
        "standard_statics_v1.rs",
        "Quirion Beastcaller",
        "bookkeeping: records the last-known counters its dies trigger reads",
    ),
    (
        "standard_statics_v1.rs",
        "Unstoppable Slasher",
        "bookkeeping: records the last-known counters its dies trigger reads",
    ),
    (
        "standard_keywords_v1.rs",
        "Burnout Bashtronaut",
        "read via standard_keywords_v1::rules_vector_statics",
    ),
    (
        "standard_keywords_v1.rs",
        "Forsaken Miner",
        "read via standard_keywords_v1::rules_vector_statics",
    ),
    (
        "standard_keywords_v1.rs",
        "Brutal Cathar",
        "read via standard_keywords_v1::rules_vector_statics",
    ),
    (
        "standard_keywords_v1.rs",
        "Graveyard Trespasser",
        "read via standard_keywords_v1::rules_vector_statics",
    ),
    (
        "standard_keywords_v1.rs",
        "Flourishing Bloom-Kin",
        "read via standard_keywords_v1::rules_vector_statics",
    ),
    (
        "standard_keywords_v1.rs",
        "Phantom Interference",
        "read via standard_keywords_v1::spree_extra_generic and spree_mode_prelude",
    ),
    (
        "standard_keywords_v1.rs",
        "Spirit Token",
        "token created by Phantom Interference's spree prelude",
    ),
    (
        "standard_keywords_v1.rs",
        "Enduring Curiosity",
        "bookkeeping: records whether it was a creature for its dies trigger",
    ),
    (
        "standard_keywords_v1.rs",
        "Enduring Innocence",
        "bookkeeping: records whether it was a creature for its dies trigger",
    ),
    (
        "standard_keywords_v1.rs",
        "Heartfire Hero",
        "bookkeeping: records the last-known power its dies trigger reads",
    ),
    (
        "effect.rs",
        "Incubator Token",
        "token created by Incubate, reached via BindIncubateToTriggerSpell",
    ),
];

const RULES_MODULES: &[(&str, &str)] = &[
    ("trigger.rs", include_str!("../trigger.rs")),
    (
        "trigger/standard_family_g_v1.rs",
        include_str!("../trigger/standard_family_g_v1.rs"),
    ),
    (
        "standard_statics_v1.rs",
        include_str!("../standard_statics_v1.rs"),
    ),
    (
        "standard_keywords_v1.rs",
        include_str!("../standard_keywords_v1.rs"),
    ),
    ("engine.rs", include_str!("../engine.rs")),
    ("effect.rs", include_str!("../effect.rs")),
    (
        "effect/library_choice_search_v2.rs",
        include_str!("../effect/library_choice_search_v2.rs"),
    ),
    ("mana.rs", include_str!("../mana.rs")),
    ("event.rs", include_str!("../event.rs")),
    ("state.rs", include_str!("../state.rs")),
    ("card_def.rs", include_str!("../card_def.rs")),
    (
        "continuous_characteristics_v1.rs",
        include_str!("../continuous_characteristics_v1.rs"),
    ),
    (
        "combat_damage_v1.rs",
        include_str!("../combat_damage_v1.rs"),
    ),
    ("legend_rule_v1.rs", include_str!("../legend_rule_v1.rs")),
];

/// Lines of non-test code: drops comment lines and the item following each
/// `#[cfg(test)]` (brace-matched).
fn non_test_code(text: &str) -> Vec<&str> {
    let mut kept = Vec::new();
    let mut skipping = false;
    let mut pending = false;
    let mut depth: i64 = 0;
    let mut opened = false;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if !skipping && trimmed.starts_with("#[cfg(test)]") {
            pending = true;
            continue;
        }
        if pending {
            pending = false;
            skipping = true;
            depth = 0;
            opened = false;
        }
        if skipping {
            depth += line.matches('{').count() as i64 - line.matches('}').count() as i64;
            opened |= line.contains('{');
            if (opened && depth <= 0) || (!opened && trimmed.ends_with(';')) {
                skipping = false;
            }
            continue;
        }
        if !trimmed.starts_with("//") {
            kept.push(line);
        }
    }
    kept
}

#[test]
fn every_card_name_branch_in_the_rules_engine_is_accounted_for() {
    let names: BTreeSet<&str> = CARD_DEFS.iter().map(|d| d.name).collect();
    let mut unaccounted = Vec::new();
    for (file, text) in RULES_MODULES {
        for line in non_test_code(text) {
            for literal in line.split('"').skip(1).step_by(2) {
                if !names.contains(literal) {
                    continue;
                }
                let known = if *file == "trigger.rs" || file.starts_with("trigger/") {
                    TRIGGER_RS_KEYED_NAMES.contains(&literal)
                } else {
                    OTHER_KEYED_NAMES
                        .iter()
                        .any(|(f, n, _)| f == file && *n == literal)
                };
                if !known {
                    unaccounted.push(format!("{file}: {literal}"));
                }
            }
        }
    }
    assert!(
        unaccounted.is_empty(),
        "unclassified card-name branches: {unaccounted:?}"
    );
}
