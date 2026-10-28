//! Card-database recipes for the eternal catalog's Vintage graveyard and
//! tempo lists (wave E7b): Dredge, Counter-Vine, Blue Tempo, Lurrus DRS,
//! Doomsday, RUG Control and the artifact cards of Stiflenought, Sphere
//! Shops and Sultai Psychic Frog. `build_eternal_e6b_v1` and the other
//! eternal tables fall through to these; runtime programs live in
//! `src/eternal_vintage_e7b_v1.rs`. Every row is keyed by a name that occurs
//! only in the eternal registry.

use super::{AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special};
use AbilityCostRecipe::{DiscardSelf, Loyalty, ManaCost, SacrificeSelf, Tap};

/// An activated ability from the battlefield, at instant speed.
const fn instant(
    cost: &'static [AbilityCostRecipe],
    effect: AbilityEffectRecipe,
    target_spec: &'static str,
) -> ActivatedAbilityRecipe {
    ActivatedAbilityRecipe {
        cost,
        effect,
        activation_zone: "Battlefield",
        sorcery_speed_only: false,
        target_spec,
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    }
}

/// "Activate only once each turn."
const fn once_each_turn(
    cost: &'static [AbilityCostRecipe],
    effect: AbilityEffectRecipe,
) -> ActivatedAbilityRecipe {
    ActivatedAbilityRecipe {
        cost,
        effect,
        activation_zone: "Battlefield",
        sorcery_speed_only: false,
        target_spec: "None",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: Some(1),
    }
}

/// A planeswalker loyalty ability: sorcery speed, once per turn through its
/// loyalty cost.
const fn loyalty(
    cost: &'static [AbilityCostRecipe],
    program: &'static str,
    target_spec: &'static str,
) -> ActivatedAbilityRecipe {
    ActivatedAbilityRecipe {
        cost,
        effect: AbilityEffectRecipe::StandardProgram(program),
        activation_zone: "Battlefield",
        sorcery_speed_only: true,
        target_spec,
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    }
}

const fn program(path: &'static str) -> AbilityEffectRecipe {
    AbilityEffectRecipe::StandardProgram(path)
}

/// Bazaar of Baghdad: "{T}: Draw two cards, then discard three cards."
const BAZAAR_OF_BAGHDAD: [ActivatedAbilityRecipe; 1] = [instant(
    &[Tap],
    program("crate::eternal_vintage_e7b_v1::bazaar_of_baghdad"),
    "None",
)];

/// Basking Rootwalla: "{1}{G}: This creature gets +2/+2 until end of turn.
/// Activate only once each turn."
const BASKING_ROOTWALLA: [ActivatedAbilityRecipe; 1] = [once_each_turn(
    &[ManaCost("{1}{G}")],
    AbilityEffectRecipe::PumpSourceUntilEndOfTurn {
        power: 2,
        toughness: 2,
    },
)];

/// Blazing Rootwalla: "{R}: This creature gets +2/+0 until end of turn.
/// Activate only once each turn."
const BLAZING_ROOTWALLA: [ActivatedAbilityRecipe; 1] = [once_each_turn(
    &[ManaCost("{R}")],
    AbilityEffectRecipe::PumpSourceUntilEndOfTurn {
        power: 2,
        toughness: 0,
    },
)];

/// Hollow One: "Cycling {2}".
const HOLLOW_ONE: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[ManaCost("{2}"), DiscardSelf],
    effect: AbilityEffectRecipe::DrawCards(1),
    activation_zone: "Hand",
    sorcery_speed_only: false,
    target_spec: "None",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

/// Shambling Shell: "Sacrifice this creature: Put a +1/+1 counter on target
/// creature."
const SHAMBLING_SHELL: [ActivatedAbilityRecipe; 1] = [instant(
    &[SacrificeSelf],
    AbilityEffectRecipe::AddPlusOnePlusOneCounters(1),
    "Creature",
)];

/// Silversmote Ghoul: "{1}{B}, Sacrifice this creature: Draw a card."
const SILVERSMOTE_GHOUL: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{1}{B}"), SacrificeSelf],
    AbilityEffectRecipe::DrawCards(1),
    "None",
)];

/// Golgari Grave-Troll: "{1}, Remove a +1/+1 counter from this creature:
/// Regenerate this creature."
const GOLGARI_GRAVE_TROLL: [ActivatedAbilityRecipe; 1] = [instant(
    &[
        ManaCost("{1}"),
        AbilityCostRecipe::RemovePlusOneCounterFromSelf,
    ],
    program("crate::eternal_vintage_e7b_v1::regenerate_source"),
    "None",
)];

/// Deathrite Shaman: "{T}: Exile target land card from a graveyard. Add one
/// mana of any color. (Activate only as an instant.)", "{B}, {T}: Exile
/// target instant or sorcery card from a graveyard. Each opponent loses 2
/// life." and "{G}, {T}: Exile target creature card from a graveyard. You
/// gain 2 life." Its first ability has a target, so it is not a mana
/// ability (605.1a) and uses the stack.
const DEATHRITE_SHAMAN: [ActivatedAbilityRecipe; 3] = [
    instant(
        &[Tap],
        program("crate::eternal_vintage_e7b_v1::deathrite_mana"),
        "StandardV1(crate::standard_cards_v1::StandardTargetV1::LandCardInAGraveyardV1)",
    ),
    instant(
        &[ManaCost("{B}"), Tap],
        program("crate::eternal_vintage_e7b_v1::deathrite_drain"),
        "StandardV1(crate::standard_cards_v1::StandardTargetV1::InstantOrSorceryCardInAGraveyardV1)",
    ),
    instant(
        &[ManaCost("{G}"), Tap],
        program("crate::eternal_vintage_e7b_v1::deathrite_gain"),
        "StandardV1(crate::standard_cards_v1::StandardTargetV1::CreatureCardInAGraveyardV1)",
    ),
];

/// Golos, Tireless Pilgrim: "{2}{W}{U}{B}{R}{G}: Exile the top three cards
/// of your library. You may play them this turn without paying their mana
/// costs."
const GOLOS: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{2}{W}{U}{B}{R}{G}")],
    program("crate::eternal_vintage_e7b_v1::golos_exile"),
    "None",
)];

/// Dack Fayden: "+1: Target player draws two cards, then discards two
/// cards.", "-2: Gain control of target artifact." and "-6: You get an
/// emblem with 'Whenever you cast a spell that targets one or more
/// permanents, gain control of those permanents.'"
const DACK_FAYDEN: [ActivatedAbilityRecipe; 3] = [
    loyalty(
        &[Loyalty(1)],
        "crate::eternal_vintage_e7b_v1::dack_plus_one",
        "AnyPlayer",
    ),
    loyalty(
        &[Loyalty(-2)],
        "crate::eternal_vintage_e7b_v1::dack_minus_two",
        "ArtifactPermanent",
    ),
    loyalty(
        &[Loyalty(-6)],
        "crate::eternal_vintage_e7b_v1::dack_minus_six",
        "None",
    ),
];

/// Wrenn and Six: "+1: Return up to one target land card from your
/// graveyard to your hand.", "-1: Wrenn and Six deals 1 damage to any
/// target." and "-7: You get an emblem with 'Instant and sorcery cards in
/// your graveyard have retrace.'"
const WRENN_AND_SIX: [ActivatedAbilityRecipe; 3] = [
    loyalty(
        &[Loyalty(1)],
        "crate::eternal_vintage_e7b_v1::wrenn_plus_one",
        "StandardV1(crate::standard_cards_v1::StandardTargetV1::UpToOneLandCardInOwnGraveyardV1)",
    ),
    loyalty(
        &[Loyalty(-1)],
        "crate::eternal_vintage_e7b_v1::wrenn_minus_one",
        "AnyTarget",
    ),
    loyalty(
        &[Loyalty(-7)],
        "crate::eternal_vintage_e7b_v1::wrenn_minus_seven",
        "None",
    ),
];

/// Minsc & Boo, Timeless Heroes: "+1: Put three +1/+1 counters on up to one
/// target creature with trample or haste." and "-2: Sacrifice a creature.
/// When you do, Minsc & Boo deals X damage to any target, where X is that
/// creature's power. If the sacrificed creature was a Hamster, draw X
/// cards."
const MINSC_AND_BOO: [ActivatedAbilityRecipe; 2] = [
    loyalty(
        &[Loyalty(1)],
        "crate::eternal_vintage_e7b_v1::minsc_plus_one",
        "StandardV1(crate::standard_cards_v1::StandardTargetV1::UpToOneCreatureWithTrampleOrHasteV1)",
    ),
    loyalty(
        &[Loyalty(-2)],
        "crate::eternal_vintage_e7b_v1::minsc_minus_two",
        "None",
    ),
];

/// The Jace planeswalker token of empower Jace: "[-1]: Surveil 1." and
/// "[-3]: Draw a card."
const JACE_TOKEN: [ActivatedAbilityRecipe; 2] = [
    loyalty(
        &[Loyalty(-1)],
        "crate::eternal_vintage_e7b_v1::jace_token_surveil",
        "None",
    ),
    loyalty(
        &[Loyalty(-3)],
        "crate::eternal_vintage_e7b_v1::jace_token_draw",
        "None",
    ),
];

/// Avatar of Burgeoning Echoes: 'Planeswalkers you control have "[-10]: Put
/// a +1/+1 counter on target creature for each land you control."' The
/// granted ability is defined here and offered on each planeswalker its
/// controller controls (`eternal_vintage_e7b_v1::granted_avatar_ability`);
/// the Avatar itself has no loyalty, so it can never pay the cost.
const AVATAR_OF_BURGEONING_ECHOES: [ActivatedAbilityRecipe; 1] = [loyalty(
    &[Loyalty(-10)],
    "crate::eternal_vintage_e7b_v1::avatar_granted_counters",
    "Creature",
)];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Bazaar of Baghdad" => &BAZAAR_OF_BAGHDAD,
        "Basking Rootwalla" => &BASKING_ROOTWALLA,
        "Blazing Rootwalla" => &BLAZING_ROOTWALLA,
        "Hollow One" => &HOLLOW_ONE,
        "Shambling Shell" => &SHAMBLING_SHELL,
        "Silversmote Ghoul" => &SILVERSMOTE_GHOUL,
        "Golgari Grave-Troll" => &GOLGARI_GRAVE_TROLL,
        "Deathrite Shaman" => &DEATHRITE_SHAMAN,
        "Golos, Tireless Pilgrim" => &GOLOS,
        "Dack Fayden" => &DACK_FAYDEN,
        "Wrenn and Six" => &WRENN_AND_SIX,
        "Minsc & Boo, Timeless Heroes" => &MINSC_AND_BOO,
        "Jace Token" => &JACE_TOKEN,
        "Avatar of Burgeoning Echoes" => &AVATAR_OF_BURGEONING_ECHOES,
        _ => super::build_eternal_e8b_v1::activated_ability_recipes_for(name),
    }
}

/// A spell whose program is a runtime function of `eternal_vintage_e7b_v1`.
const fn spell(target_spec: &'static str, program: &'static str) -> Special {
    Special::StandardProgram {
        target_spec,
        program,
    }
}

/// Spell programs.
pub(super) fn special_for(name: &str) -> Option<Special> {
    Some(match name {
        // "This spell can't be countered" is its `cant_be_countered`
        // mechanic.
        "Abrupt Decay" => spell(
            "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::NonlandPermanentManaValueAtMostV1(3))",
            "crate::eternal_vintage_e7b_v1::abrupt_decay",
        ),
        "Assassin's Trophy" => spell(
            "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::OpponentPermanentV1)",
            "crate::eternal_vintage_e7b_v1::assassins_trophy",
        ),
        "Chain of Vapor" => spell(
            "TargetSpec::NonlandPermanent",
            "crate::eternal_vintage_e7b_v1::chain_of_vapor",
        ),
        // Its blue-card exile is `alt_cost_for` below.
        "Commandeer" => spell(
            "TargetSpec::NoncreatureSpellOnStack",
            "crate::eternal_vintage_e7b_v1::commandeer",
        ),
        "Creeping Chill" => spell(
            "TargetSpec::None",
            "crate::eternal_vintage_e7b_v1::creeping_chill",
        ),
        "Demonic Consultation" => spell(
            "TargetSpec::None",
            "crate::eternal_vintage_e7b_v1::demonic_consultation",
        ),
        // Carnivorous Cultivator's prepare spell, cast only as a copy.
        "Enroot" => spell("TargetSpec::None", "crate::eternal_vintage_e7b_v1::enroot"),
        // Its free cast is `alt_cost_for` below.
        "Mindbreak Trap" => spell(
            "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::AnyNumberOfSpellsV1)",
            "crate::eternal_vintage_e7b_v1::mindbreak_trap",
        ),
        "Noxious Revival" => spell(
            "TargetSpec::CardInGraveyards",
            "crate::eternal_vintage_e7b_v1::noxious_revival",
        ),
        // "This creature enters with X +1/+1 counters on it."
        "Stonecoil Serpent" => spell(
            "TargetSpec::None",
            "crate::eternal_vintage_e7b_v1::stonecoil_serpent",
        ),
        _ => return super::build_eternal_e8b_v1::special_for(name),
    })
}

/// Printed evergreen keywords, in printed order.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Carnivorous Cultivator" => &["Keywords::DEATHTOUCH"],
        "Grief" => &["Keywords::MENACE"],
        "Ichorid" | "Vengevine" => &["Keywords::HASTE"],
        "Kishla Skimmer" | "Narcomoeba" | "Stinkweed Imp" => &["Keywords::FLYING"],
        "Stonecoil Serpent" => &["Keywords::REACH", "Keywords::TRAMPLE"],
        "Subtlety" => &["Keywords::FLASH", "Keywords::FLYING"],
        "Boo Token" => &["Keywords::TRAMPLE", "Keywords::HASTE"],
        _ => super::build_eternal_e8b_v1::keywords_for(name),
    }
}

/// Alternative costs. Grief: "Evoke--Exile a black card from your hand.";
/// Subtlety: "Evoke--Exile a blue card from your hand." (their sacrifice is
/// an entry trigger); Commandeer: "You may exile two blue cards from your
/// hand rather than pay this spell's mana cost."; Mindbreak Trap: "If an
/// opponent cast three or more spells this turn, you may pay {0} rather
/// than pay this spell's mana cost."; Ragavan: "Dash {1}{R}".
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    match name {
        "Grief" => "Some(AltCostDef { components: &[CostComponent::ExileCardFromHandWithColorV1(ManaColor::B)], condition: AltCostCondition::Always })",
        "Subtlety" => "Some(AltCostDef { components: &[CostComponent::ExileCardFromHandWithColorV1(ManaColor::U)], condition: AltCostCondition::Always })",
        "Commandeer" => "Some(AltCostDef { components: &[CostComponent::ExileCardsFromHandWithColorV1 { count: 2, color: ManaColor::U }], condition: AltCostCondition::Always })",
        "Mindbreak Trap" => "Some(AltCostDef { components: &[CostComponent::Mana(Cost { pips: &[], generic: 0, x_count: 0 })], condition: AltCostCondition::OpponentCastThreeSpellsThisTurnV1 })",
        "Ragavan, Nimble Pilferer" => "Some(AltCostDef { components: &[CostComponent::Mana(Cost { pips: &[Pip::Colored(ManaColor::R)], generic: 1, x_count: 0 })], condition: AltCostCondition::DashV1 })",
        _ => super::build_eternal_e8b_v1::alt_cost_for(name),
    }
}

/// Madness costs: the Rootwallas' "Madness {0}".
pub(super) fn madness_cost_for(name: &str) -> Option<&'static str> {
    match name {
        "Basking Rootwalla" | "Blazing Rootwalla" => Some("{0}"),
        _ => None,
    }
}

/// Generic cost reductions. Hollow One: "This spell costs {2} less to cast
/// for each card you've cycled or discarded this turn."
pub(super) fn generic_cost_reduction_for(name: &str) -> &'static str {
    match name {
        "Hollow One" => "Some(GenericCostReductionDef { generic_per_count: 2, count: DynamicCountDef::CardsCycledOrDiscardedThisTurnV1 })",
        _ => super::build_eternal_e8b_v1::generic_cost_reduction_for(name),
    }
}

/// Escape costs. Uro: "Escape--{G}{G}{U}{U}, Exile five other cards from
/// your graveyard."
pub(super) fn escape_for(name: &str) -> String {
    match name {
        "Uro, Titan of Nature's Wrath" => {
            let (pips, generic, x_count) = super::parse_cost("{G}{G}{U}{U}");
            format!(
                "Some(EscapeDef {{ cost: &[CostComponent::Mana(Cost {{ pips: &[{}], generic: {generic}, x_count: {x_count} }}), CostComponent::ExileOtherCardsFromOwnGraveyard(5)] }})",
                pips.join(", ")
            )
        }
        _ => "None".to_string(),
    }
}

/// Tokens keep their printed names: "create Boo, a legendary 1/1 red
/// Hamster creature token" and the "blue Jace planeswalker token".
pub(super) fn object_name_for(name: &str) -> Option<&'static str> {
    match name {
        "Boo Token" => Some("Boo"),
        "Jace Token" => Some("Jace"),
        _ => None,
    }
}

/// A prepare spell (Carnivorous Cultivator's Enroot): a registry card that is
/// in no deck, since only copies of it are cast.
pub(super) fn is_prepare_spell(name: &str) -> bool {
    name == "Enroot"
}

/// Card-database tokens for the triggered abilities in
/// `eternal_vintage_e7b_v1::triggers_for`, its hooks and its statics.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Assassin's Trophy" => "spell:destroy_target_opponent_permanent:its_controller_may_search_basic_land_untapped",
        "Avatar of Burgeoning Echoes" => {
            "landfall:empower_jace:2;static:controlled_planeswalkers_have_minus10_counter_per_land"
        }
        "Bazaar of Baghdad" => "activated:draw_2_then_discard_3",
        "Carnivorous Cultivator" => {
            "enters_prepared:enroot;combat_damage_player:return_target_land_card_from_graveyard_to_hand"
        }
        "Chain of Vapor" => {
            "spell:bounce_target_nonland_permanent:its_controller_may_sacrifice_land_to_copy_with_new_target"
        }
        "Collector Ouphe" => "static:artifact_activated_abilities_cant_be_activated",
        "Commandeer" => "spell:gain_control_of_target_noncreature_spell:may_choose_new_targets",
        "Creeping Chill" => {
            "spell:damage_each_opponent:3:gain_life:3;milled:may_exile_self:damage_each_opponent:3:gain_life:3"
        }
        "Crucible of Worlds" => "static:play_lands_from_graveyard",
        "Dack Fayden" => "minus2:gain_control_target_artifact;minus6:emblem:cast_spell_targeting_permanents:gain_control",
        "Deathrite Shaman" => {
            "activated:exile_land_card:add_any_color;activated:exile_instant_sorcery:opponent_loses_2;activated:exile_creature:gain_2"
        }
        "Demonic Consultation" => "spell:name_card:exile_top_6:reveal_until_named:hand:exile_rest",
        "Enroot" => "spell:search_library_land_card_to_graveyard:shuffle;prepare_spell_copy_only",
        "Golgari Grave-Troll" => {
            "dredge:6;enters_with_counter_per_creature_card_in_graveyard;activated:regenerate"
        }
        "Golgari Thug" => "dredge:4;dies:target_creature_card_own_graveyard_to_library_top",
        "Golos, Tireless Pilgrim" => {
            "etb:may_search_land_battlefield_tapped;activated:exile_top_3:play_free_this_turn"
        }
        "Grief" => "etb:target_opponent:reveal_hand:choose_nonland_discard;etb_if_evoked:sacrifice_source",
        "Hollow One" => "cost_reduction:2_per_card_cycled_or_discarded_this_turn",
        "Ichorid" => {
            "each_end_step:sacrifice_source;graveyard_upkeep:may_exile_other_black_creature_card:return_to_battlefield"
        }
        "Jace Token" => "minus1:surveil:1;minus3:draw:1",
        "Karn, Argent Defender" => "static:artifacts_and_creatures_entering_dont_trigger_abilities",
        "Kishla Skimmer" => "card_leaves_your_graveyard_your_turn:draw:1:once_each_turn",
        "Lodestone Golem" => "static:nonartifact_spells_cost_1_more",
        "Master of Death" => "etb:surveil:2;graveyard_upkeep:may_pay_1_life:return_to_hand",
        "Mindbreak Trap" => "alt_cost:opponent_cast_three_spells:free;spell:exile_any_number_target_spells",
        "Minsc & Boo, Timeless Heroes" => {
            "etb_and_upkeep:may_create_boo;plus1:three_counters_up_to_one_trample_or_haste_creature;minus2:sacrifice_creature:reflexive_damage_power:hamster_draws"
        }
        "Narcomoeba" => "milled:may_put_onto_battlefield",
        "Noxious Revival" => "spell:target_graveyard_card_to_owners_library_top",
        "Phyrexian Revoker" => "as_enters:choose_nonland_name;static:named_sources_cant_activate",
        "Prized Amalgam" => {
            "graveyard:creature_enters_from_your_graveyard_or_cast_from_it:return_self_tapped_next_end_step"
        }
        "Ragavan, Nimble Pilferer" => {
            "dash:haste:return_next_end_step;combat_damage_player:treasure:exile_top_may_cast_this_turn"
        }
        "Serum Powder" => "mulligan:exile_hand_draw_that_many",
        "Shambling Shell" => "dredge:3",
        "Silversmote Ghoul" => "graveyard_your_end_step_if_gained_3_life:return_self_tapped",
        "Squee, Goblin Nabob" => "graveyard_upkeep:may_return_to_hand",
        "Stinkweed Imp" => "dredge:5;combat_damage_creature:destroy_it",
        "Stonecoil Serpent" => "enters_with_x_counters;protection_from_multicolored",
        "Subtlety" => {
            "etb:up_to_one_target_creature_or_planeswalker_spell:owner_top_or_bottom;etb_if_evoked:sacrifice_source"
        }
        "Thorn of Amethyst" => "static:noncreature_spells_cost_1_more",
        "Torpor Orb" => "static:creatures_entering_dont_trigger_abilities",
        "Uro, Titan of Nature's Wrath" => {
            "etb:sacrifice_unless_escaped;etb_or_attacks:gain_3:draw_1:may_put_land_from_hand"
        }
        "Vengevine" => "graveyard:second_creature_spell_cast_this_turn:may_return_to_battlefield",
        "Wrenn and Six" => "minus7:emblem:instant_sorcery_graveyard_retrace",
        _ => super::build_eternal_e8b_v1::trigger_recipe_for(name),
    }
}
