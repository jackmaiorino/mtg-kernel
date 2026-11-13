//! Card-database recipes for the eternal catalog's Pauper sideboard cards
//! (wave S1d). These tables are the tail of every fall-through chain (the
//! last wave's tables end here); runtime programs live in
//! `src/eternal_sideboard_s1d_v1.rs`. Every row is keyed by a name that
//! occurs only in the eternal registry.

use super::{
    AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special, StackSpellFilter,
};
use AbilityCostRecipe::{ManaCost, SacrificeSelf, Tap};

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

/// A program of `eternal_sideboard_s1d_v1`.
const fn program(path: &'static str) -> AbilityEffectRecipe {
    AbilityEffectRecipe::StandardProgram(path)
}

/// Scattershot Archer: "{T}: This creature deals 1 damage to each creature
/// with flying."
const SCATTERSHOT_ARCHER: [ActivatedAbilityRecipe; 1] = [instant(
    &[Tap],
    program("crate::eternal_sideboard_s1d_v1::scattershot_archer"),
    "None",
)];

/// Circle of Protection: Blue: "{1}: The next time a blue source of your
/// choice would deal damage to you this turn, prevent that damage."
const CIRCLE_OF_PROTECTION_BLUE: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{1}")],
    program("crate::eternal_sideboard_s1d_v1::cop_choose"),
    "None",
)];

/// Navigator's Compass: "{T}: Until end of turn, target land you control
/// becomes the basic land type of your choice in addition to its other
/// types."
const NAVIGATORS_COMPASS: [ActivatedAbilityRecipe; 1] = [instant(
    &[Tap],
    program("crate::eternal_sideboard_s1d_v1::navigators_compass"),
    "StandardV1(crate::standard_cards_v1::StandardTargetV1::S1dV1(crate::eternal_sideboard_s1d_v1::targets::S1dTargetV1::ControlledLandV1))",
)];

/// Martyr of Sands: "{1}, Reveal X white cards from your hand, Sacrifice
/// this creature: You gain three times X life." The native marker owns the
/// private reveal selection and derives X from the actual paid subset.
const MARTYR_OF_SANDS: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{1}"), SacrificeSelf],
    program("crate::eternal_sideboard_s1d_v1::martyr_of_sands"),
    "None",
)];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Scattershot Archer" => &SCATTERSHOT_ARCHER,
        "Circle of Protection: Blue" => &CIRCLE_OF_PROTECTION_BLUE,
        "Navigator's Compass" => &NAVIGATORS_COMPASS,
        "Martyr of Sands" => &MARTYR_OF_SANDS,
        _ => super::build_eternal_s1c_v1::activated_ability_recipes_for(name),
    }
}

/// A spell whose program is a runtime function.
const fn spell(target_spec: &'static str, program: &'static str) -> Special {
    Special::StandardProgram {
        target_spec,
        program,
    }
}

/// Spell programs.
pub(super) fn special_for(name: &str) -> Option<Special> {
    Some(match name {
        "Vandalblast" => spell(
            "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::OpponentArtifactV1)",
            "crate::eternal_sideboard_s1d_v1::vandalblast",
        ),
        "Electrickery" => spell(
            "TargetSpec::OpponentControlledCreature",
            "crate::eternal_sideboard_s1d_v1::electrickery",
        ),
        "Crypt Incursion" => spell(
            "TargetSpec::AnyPlayer",
            "crate::eternal_sideboard_s1d_v1::crypt_incursion",
        ),
        "Call Damage Control" => spell(
            "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::S1dV1(crate::eternal_sideboard_s1d_v1::targets::S1dTargetV1::GraveyardCardsOfDistinctTypesV1))",
            "crate::eternal_sideboard_s1d_v1::call_damage_control",
        ),
        // "Enchant creature" Auras enter attached to their target.
        "Sentinel's Eyes" => Special::BindTheMonster,
        "Firebending Lesson" => spell(
            "TargetSpec::Creature",
            "crate::eternal_sideboard_s1d_v1::firebending_lesson",
        ),
        "It'll Quench Ya!" => Special::CounterUnlessPaysGeneric {
            filter: StackSpellFilter::Any,
            generic: 2,
        },
        "Origin of Metalbending" => spell(
            "TargetSpec::ArtifactOrEnchantmentPermanent",
            "crate::eternal_sideboard_s1d_v1::origin_of_metalbending_destroy",
        ),
        "Waterbending Lesson" => spell(
            "TargetSpec::None",
            "crate::eternal_sideboard_s1d_v1::waterbending_lesson",
        ),
        "Drown in Sorrow" => spell(
            "TargetSpec::None",
            "crate::eternal_sideboard_s1d_v1::drown_in_sorrow",
        ),
        "Deglamer" => spell(
            "TargetSpec::ArtifactOrEnchantmentPermanent",
            "crate::eternal_sideboard_s1d_v1::deglamer",
        ),
        "Last Breath" => spell(
            "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::S1dV1(crate::eternal_sideboard_s1d_v1::targets::S1dTargetV1::CreaturePowerAtMostV1(2)))",
            "crate::eternal_sideboard_s1d_v1::last_breath",
        ),
        "Faerie Trickery" => spell(
            "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::S1dV1(crate::eternal_sideboard_s1d_v1::targets::S1dTargetV1::NonFaerieSpellV1))",
            "crate::eternal_sideboard_s1d_v1::faerie_trickery",
        ),
        "Sagittars' Volley" => spell(
            "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::S1dV1(crate::eternal_sideboard_s1d_v1::targets::S1dTargetV1::CreatureWithFlyingV1))",
            "crate::eternal_sideboard_s1d_v1::sagittars_volley",
        ),
        // "Enchant land".
        "Spreading Seas" => spell("TargetSpec::Land", "crate::standard_cards_v1::aura_enters"),
        "Holy Light" => spell(
            "TargetSpec::None",
            "crate::eternal_sideboard_s1d_v1::holy_light",
        ),
        "Ultimate Alliance" => spell(
            "TargetSpec::Creature",
            "crate::eternal_sideboard_s1d_v1::ultimate_alliance",
        ),
        "Return to Dust" => spell(
            "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::S1dV1(crate::eternal_sideboard_s1d_v1::targets::S1dTargetV1::ArtifactOrEnchantmentThenUpToOneOtherV1))",
            "crate::eternal_sideboard_s1d_v1::return_to_dust",
        ),
        _ => return super::build_eternal_s1c_v1::special_for(name),
    })
}

/// Origin of Metalbending's second mode.
pub(super) fn mode2_for(name: &str) -> &'static str {
    match name {
        "Origin of Metalbending" => "Some(ModeDef { target_spec: TargetSpec::ControlledCreature, effect: crate::eternal_sideboard_s1d_v1::origin_of_metalbending_counter })",
        _ => super::build_eternal_s1a_v1::mode2_for(name),
    }
}

pub(super) fn mode3_for(name: &str) -> &'static str {
    super::build_eternal_s1a_v1::mode3_for(name)
}

/// Overload: the shared second form uses the paid overload cost.
/// Vandalblast {4}{R}; Electrickery {1}{R}.
pub(super) fn adventure_for(name: &str) -> &'static str {
    match name {
        "Vandalblast" => "Some(AdventureDef { name: \"Vandalblast Overload\", cost: Cost { pips: &[Pip::Colored(ManaColor::R)], generic: 4, x_count: 0 }, types: &[CardType::Sorcery], target_spec: TargetSpec::None, effect: ability_effect_program_eternal_sideboard_s1d_v1_vandalblast_overload })",
        "Electrickery" => "Some(AdventureDef { name: \"Electrickery Overload\", cost: Cost { pips: &[Pip::Colored(ManaColor::R)], generic: 1, x_count: 0 }, types: &[CardType::Instant], target_spec: TargetSpec::None, effect: ability_effect_program_eternal_sideboard_s1d_v1_electrickery_overload })",
        _ => "None",
    }
}

pub(super) fn adventure_effect_recipe_for(name: &str) -> Option<AbilityEffectRecipe> {
    match name {
        "Vandalblast" => Some(program(
            "crate::eternal_sideboard_s1d_v1::vandalblast_overload",
        )),
        "Electrickery" => Some(program(
            "crate::eternal_sideboard_s1d_v1::electrickery_overload",
        )),
        _ => None,
    }
}

pub(super) fn adventure_face_name_for(name: &str) -> Option<&'static str> {
    match name {
        "Vandalblast" => Some("Vandalblast Overload"),
        "Electrickery" => Some("Electrickery Overload"),
        _ => None,
    }
}

/// Foot Ninjas: "Sneak {3}{W/B}".
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    match name {
        "Foot Ninjas" => "Some(AltCostDef { components: &[CostComponent::Mana(Cost { pips: &[Pip::Hybrid(ManaColor::W, ManaColor::B)], generic: 3, x_count: 0 }), CostComponent::ReturnControlledPermanentToOwnersHand(PermanentFilterDef::UnblockedAttackingCreatureV1)], condition: AltCostCondition::SneakV1 })",
        _ => "None",
    }
}

/// Firebending Lesson: "Kicker {4}".
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    match name {
        "Firebending Lesson" => Some("{4}"),
        _ => super::build_eternal_s1b_v1::kicker_cost_for(name),
    }
}

/// Sentinel's Eyes: "Escape--{W}, Exile two other cards from your graveyard."
pub(super) fn escape_for(name: &str) -> String {
    match name {
        "Sentinel's Eyes" => {
            let (pips, generic, x_count) = super::parse_cost("{W}");
            format!(
                "Some(EscapeDef {{ cost: &[CostComponent::Mana(Cost {{ pips: &[{}], generic: {generic}, x_count: {x_count} }}), CostComponent::ExileOtherCardsFromOwnGraveyard(2)] }})",
                pips.join(", ")
            )
        }
        _ => "None".to_string(),
    }
}

/// Sentinel's Eyes: "Enchanted creature gets +1/+1 and has vigilance."
pub(super) fn attachment_for(name: &str) -> &'static str {
    match name {
        "Sentinel's Eyes" => "Some(AttachmentDef::AuraCreatureStatic(AuraCreatureStaticDef { power: 1, toughness: 1, keywords: Keywords::VIGILANCE, per_controlled_subtype: None }))",
        _ => "None",
    }
}

/// Fixed colors of the primary printed mana ability: Gruul Turf's one action
/// identifies its whole bundle.
pub(super) fn primary_mana_ability_colors(name: &str) -> Option<Vec<&'static str>> {
    match name {
        "Gruul Turf" => Some(vec!["R"]),
        _ => super::build_eternal_e9b_v1::primary_mana_ability_colors(name),
    }
}

/// Single printed mana abilities that are not "tap: add one".
pub(super) fn mana_ability_def_for(name: &str) -> &'static str {
    match name {
        // "{T}: Add {R}{G}." One activation adds both colors at once.
        "Gruul Turf" => "Some(ManaAbilityDef { cost: ManaAbilityCostDef::TapSelf, amount: ManaAbilityAmountDef::FixedColorsV1(&[ManaColor::R, ManaColor::G]), controller_damage: 0, max_activations_per_turn: None })",
        _ => super::build_eternal_e9b_v1::mana_ability_def_for(name),
    }
}

/// Card-database tokens for the triggered abilities in
/// `eternal_sideboard_s1d_v1::triggers_for`, its hooks and its statics.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Arashin Cleric" | "Foot Ninjas" => "etb:gain_life:3",
        "Navigator's Compass" => "etb:gain_life:3;ability:target_controlled_land:becomes_chosen_basic_land_type_eot_in_addition",
        "Palace Sentinels" => "etb:become_monarch",
        "Spreading Seas" => "etb:draw:1;static:enchanted_land_is_island",
        "Nylea's Disciple" => "etb:gain_life_equal_to_devotion_to_green",
        "Stonehorn Dignitary" => "etb:target_opponent_skips_next_combat_phase",
        "Gruul Turf" => "etb:return_chosen_controlled_land_to_owners_hand:untargeted",
        "Standard Bearer" | "Coalition Honor Guard" => {
            "static:opponents_must_choose_a_flagbearer_when_casting_or_activating_if_able"
        }
        "Sentinel's Eyes" => "static:plus_1_plus_1_vigilance;escape:{W}:exile_two_other_cards",
        "Vandalblast" | "Electrickery" => "overload:paid_alternative_form_from_authorized_zone;free_cast_excludes_overload",
        "Martyr of Sands" => "ability:reveal_x_white_cards_from_hand_sacrifice:gain_three_times_x",
        "Circle of Protection: Blue" => {
            "ability:choose_blue_source:prevent_next_damage_to_you_from_it_this_turn"
        }
        "Waterbending Lesson" => "spell:draw_3:discard_unless_waterbend_2",
        _ => super::build_eternal_s1c_v1::trigger_recipe_for(name),
    }
}
