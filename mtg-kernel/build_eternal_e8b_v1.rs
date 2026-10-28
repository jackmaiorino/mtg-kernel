//! Card-database recipes for the eternal catalog's Pauper Ruby Storm,
//! Golgari Gardens, Familiars, Jeskai Ephemerate and smaller lists (wave
//! E8b). `build_eternal_e7a_v1` and the other eternal tables fall through to
//! these; runtime programs live in `src/eternal_pauper_e8b_v1.rs`. Every row
//! is keyed by a name that occurs only in the eternal registry.

use super::{
    AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, LibrarySearchFilterRecipe,
    Special,
};
use AbilityCostRecipe::{DiscardSelf, ManaCost, SacrificeSelf, Tap};

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

/// A cycling ability: activated from hand, discarding the card.
const fn cycling(cost: &'static [AbilityCostRecipe]) -> ActivatedAbilityRecipe {
    ActivatedAbilityRecipe {
        cost,
        effect: AbilityEffectRecipe::DrawCards(1),
        activation_zone: "Hand",
        sorcery_speed_only: false,
        target_spec: "None",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    }
}

/// A program of `eternal_pauper_e8b_v1`.
const fn program(path: &'static str) -> AbilityEffectRecipe {
    AbilityEffectRecipe::StandardProgram(path)
}

/// Contaminated Landscape: "{T}, Sacrifice this land: Search your library
/// for a basic Plains, Island, or Swamp card, put it onto the battlefield
/// tapped, then shuffle." and "Cycling {W}{U}{B}". Its `{T}: Add {C}` is the
/// ordinary mana ability.
const CONTAMINATED_LANDSCAPE: [ActivatedAbilityRecipe; 2] = [
    instant(
        &[Tap, SacrificeSelf],
        AbilityEffectRecipe::SearchLibraryToBattlefieldTapped {
            filter: LibrarySearchFilterRecipe::BasicLandWithAnySubtype([
                "Plains", "Island", "Swamp",
            ]),
        },
        "None",
    ),
    cycling(&[ManaCost("{W}{U}{B}"), DiscardSelf]),
];

/// Perilous Landscape: the same for a basic Island, Mountain, or Plains
/// card, and "Cycling {U}{R}{W}".
const PERILOUS_LANDSCAPE: [ActivatedAbilityRecipe; 2] = [
    instant(
        &[Tap, SacrificeSelf],
        AbilityEffectRecipe::SearchLibraryToBattlefieldTapped {
            filter: LibrarySearchFilterRecipe::BasicLandWithAnySubtype([
                "Island", "Mountain", "Plains",
            ]),
        },
        "None",
    ),
    cycling(&[ManaCost("{U}{R}{W}"), DiscardSelf]),
];

/// Desert: "{T}: This land deals 1 damage to target attacking creature.
/// Activate only during the end of combat step." The timing is
/// `eternal_pauper_e8b_v1::activation_allowed`; its `{T}: Add {C}` is the
/// ordinary mana ability.
const DESERT: [ActivatedAbilityRecipe; 1] = [instant(
    &[Tap],
    AbilityEffectRecipe::DamageTarget(1),
    "AttackingCreature",
)];

/// Crypt Rats: "{X}: This creature deals X damage to each creature and each
/// player. Spend only black mana on X." Activations have no free X, so each
/// X from 1 to 10 is its own ability costing X black mana (only black mana
/// pays a {B} pip). X = 0 deals no damage.
const CRYPT_RATS: [ActivatedAbilityRecipe; 10] = [
    instant(
        &[ManaCost("{B}")],
        program("crate::eternal_pauper_e8b_v1::crypt_rats_1"),
        "None",
    ),
    instant(
        &[ManaCost("{B}{B}")],
        program("crate::eternal_pauper_e8b_v1::crypt_rats_2"),
        "None",
    ),
    instant(
        &[ManaCost("{B}{B}{B}")],
        program("crate::eternal_pauper_e8b_v1::crypt_rats_3"),
        "None",
    ),
    instant(
        &[ManaCost("{B}{B}{B}{B}")],
        program("crate::eternal_pauper_e8b_v1::crypt_rats_4"),
        "None",
    ),
    instant(
        &[ManaCost("{B}{B}{B}{B}{B}")],
        program("crate::eternal_pauper_e8b_v1::crypt_rats_5"),
        "None",
    ),
    instant(
        &[ManaCost("{B}{B}{B}{B}{B}{B}")],
        program("crate::eternal_pauper_e8b_v1::crypt_rats_6"),
        "None",
    ),
    instant(
        &[ManaCost("{B}{B}{B}{B}{B}{B}{B}")],
        program("crate::eternal_pauper_e8b_v1::crypt_rats_7"),
        "None",
    ),
    instant(
        &[ManaCost("{B}{B}{B}{B}{B}{B}{B}{B}")],
        program("crate::eternal_pauper_e8b_v1::crypt_rats_8"),
        "None",
    ),
    instant(
        &[ManaCost("{B}{B}{B}{B}{B}{B}{B}{B}{B}")],
        program("crate::eternal_pauper_e8b_v1::crypt_rats_9"),
        "None",
    ),
    instant(
        &[ManaCost("{B}{B}{B}{B}{B}{B}{B}{B}{B}{B}")],
        program("crate::eternal_pauper_e8b_v1::crypt_rats_10"),
        "None",
    ),
];

/// Heaped Harvest: "{2}, {T}, Sacrifice this artifact: You gain 3 life."
const HEAPED_HARVEST: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{2}"), Tap, SacrificeSelf],
    AbilityEffectRecipe::GainLife(3),
    "None",
)];

/// Mirran Banesplitter: "Equip {3}".
const MIRRAN_BANESPLITTER: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[ManaCost("{3}")],
    effect: AbilityEffectRecipe::AttachSourceToTarget,
    activation_zone: "Battlefield",
    sorcery_speed_only: true,
    target_spec: "ControlledCreature",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

/// Reckless Lackey: "{2}{R}, Sacrifice this creature: Draw a card and create
/// a Treasure token."
const RECKLESS_LACKEY: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{2}{R}"), SacrificeSelf],
    program("crate::eternal_pauper_e8b_v1::reckless_lackey"),
    "None",
)];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Contaminated Landscape" => &CONTAMINATED_LANDSCAPE,
        "Perilous Landscape" => &PERILOUS_LANDSCAPE,
        "Desert" => &DESERT,
        "Crypt Rats" => &CRYPT_RATS,
        "Heaped Harvest" => &HEAPED_HARVEST,
        "Mirran Banesplitter" => &MIRRAN_BANESPLITTER,
        "Reckless Lackey" => &RECKLESS_LACKEY,
        _ => super::build_eternal_e8a_v1::activated_ability_recipes_for(name),
    }
}

/// A spell whose program is a runtime function of `eternal_pauper_e8b_v1`.
const fn spell(target_spec: &'static str, program: &'static str) -> Special {
    Special::StandardProgram {
        target_spec,
        program,
    }
}

/// Spell programs.
pub(super) fn special_for(name: &str) -> Option<Special> {
    Some(match name {
        // Teamwork is the alternative cost (`alt_cost_for` below).
        "Crossover Collaboration" => spell(
            "TargetSpec::None",
            "crate::eternal_pauper_e8b_v1::crossover_collaboration",
        ),
        "Defile" => spell(
            "TargetSpec::Creature",
            "crate::eternal_pauper_e8b_v1::defile",
        ),
        // Splice onto Arcane is announced like X (`eternal_pauper_e8b_v1`).
        "Desperate Ritual" => spell(
            "TargetSpec::None",
            "crate::eternal_pauper_e8b_v1::desperate_ritual",
        ),
        "First Day of Class" => spell(
            "TargetSpec::None",
            "crate::eternal_pauper_e8b_v1::first_day_of_class",
        ),
        "Glimpse the Impossible" => spell(
            "TargetSpec::None",
            "crate::eternal_pauper_e8b_v1::glimpse_the_impossible",
        ),
        "Glorious Gale" => spell(
            "TargetSpec::CreatureSpellOnStack",
            "crate::eternal_pauper_e8b_v1::glorious_gale",
        ),
        "Manamorphose" => spell(
            "TargetSpec::None",
            "crate::eternal_pauper_e8b_v1::manamorphose",
        ),
        // Convoke is the alternative cost (`alt_cost_for` below).
        "Meeting of Minds" => Special::DrawCards(2),
        // Flashback is `flashback_for` below.
        "Rally the Peasants" => Special::BoostControlledCreatures {
            power: 2,
            toughness: 0,
            keyword: "NONE",
        },
        "Rancid Earth" => spell(
            "TargetSpec::Land",
            "crate::eternal_pauper_e8b_v1::rancid_earth",
        ),
        // Its tapped-target reduction is `generic_cost_reduction_for` below.
        "Ride's End" => spell(
            "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::CreatureOrVehicleV1)",
            "crate::eternal_pauper_e8b_v1::rides_end",
        ),
        "Seething Song" => spell(
            "TargetSpec::None",
            "crate::eternal_pauper_e8b_v1::seething_song",
        ),
        // Flashback is `flashback_for` below; the token's power and
        // toughness are a characteristic-defining ability.
        "Seize the Storm" => Special::CreateTokens {
            token: "Red Elemental Token",
            count: 1,
        },
        "Skred" => spell(
            "TargetSpec::Creature",
            "crate::eternal_pauper_e8b_v1::skred",
        ),
        // Flashback (with its discard) is `flashback_for` and
        // `additional_cost_for` below.
        "Twinned Vision" => spell(
            "TargetSpec::None",
            "crate::eternal_pauper_e8b_v1::twinned_vision",
        ),
        // "Exile the top two cards of your library. Until the end of your
        // next turn, you may play those cards." Reckless Impulse's program.
        "Wrenn's Resolve" => Special::RecklessImpulse,
        _ => return super::build_eternal_e8a_v1::special_for(name),
    })
}

/// Printed evergreen keywords, in printed order. Sunscape Familiar's
/// defender comes from its registry mechanics.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Crimson Fleet Commodore" => &["Keywords::TRAMPLE"],
        "Goliath Paladin" => &["Keywords::VIGILANCE"],
        "Mirran Banesplitter" => &["Keywords::FLASH"],
        "Reckless Lackey" => &["Keywords::FIRST_STRIKE", "Keywords::HASTE"],
        "Red Elemental Token" => &["Keywords::TRAMPLE"],
        _ => super::build_eternal_e8a_v1::keywords_for(name),
    }
}

/// Alternative costs. Crossover Collaboration's "Teamwork 2" (an optional
/// additional cost of tapping creatures with total power 2 or more) is an
/// alternative cost of the same mana plus that tapping, chosen through the
/// convoke selection (`standard_cast_convoke`); Meeting of Minds: convoke;
/// Oroku Saki: "Sneak {1}{B}".
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    match name {
        "Crossover Collaboration" => "Some(AltCostDef { components: &[CostComponent::Mana(Cost { pips: &[Pip::Colored(ManaColor::R)], generic: 2, x_count: 0 })], condition: AltCostCondition::TeamworkV1(2) })",
        "Meeting of Minds" => "Some(AltCostDef { components: &[CostComponent::ConvokeMana(Cost { pips: &[Pip::Colored(ManaColor::U)], generic: 3, x_count: 0 })], condition: AltCostCondition::Always })",
        "Oroku Saki, Shredder Rising" => "Some(AltCostDef { components: &[CostComponent::Mana(Cost { pips: &[Pip::Colored(ManaColor::B)], generic: 1, x_count: 0 }), CostComponent::ReturnControlledPermanentToOwnersHand(PermanentFilterDef::UnblockedAttackingCreatureV1)], condition: AltCostCondition::SneakV1 })",
        _ => super::build_eternal_e8a_v1::alt_cost_for(name),
    }
}

/// Mandatory additional costs. Twinned Vision's "Discard a card" belongs to
/// its flashback cost only (`eternal_pauper_e8b_v1::skips_additional_cost`
/// skips it for every other cast).
pub(super) fn additional_cost_for(name: &str) -> &'static str {
    match name {
        "Twinned Vision" => "Some(&[CostComponent::DiscardCards(1)])",
        _ => super::build_eternal_e8a_v1::additional_cost_for(name),
    }
}

/// A mana flashback cost.
fn mana_flashback(cost: &str) -> String {
    let (pips, generic, x_count) = super::parse_cost(cost);
    format!(
        "Some(FlashbackDef {{ cost: &[CostComponent::Mana(Cost {{ pips: &[{}], generic: {generic}, x_count: {x_count} }})] }})",
        pips.join(", ")
    )
}

/// Flashback costs. Rally the Peasants: "Flashback {2}{R}"; Seize the Storm:
/// "Flashback {6}{R}"; Twinned Vision: "Flashback--{1}{U/R}{U/R}, Discard a
/// card" (the discard is its flashback-only additional cost).
pub(super) fn flashback_for(name: &str) -> String {
    match name {
        "Rally the Peasants" => mana_flashback("{2}{R}"),
        "Seize the Storm" => mana_flashback("{6}{R}"),
        "Twinned Vision" => mana_flashback("{1}{U/R}{U/R}"),
        _ => super::build_eternal_e9a_v1::flashback_for(name),
    }
}

/// Equipment statics.
pub(super) fn equipment_for(name: &str) -> &'static str {
    match name {
        "Mirran Banesplitter" => "Some(EquipmentDef { power_delta: 2, toughness_delta: 0, add_subtype: None, controller_turn_keywords: Keywords::NONE, other_turn_keywords: Keywords::NONE, noncreature_spell_damage_to_each_opponent: 0, job_select: false, granted_activated_ability: None, pt_controller_turn_only: false })",
        _ => super::build_eternal_e8a_v1::equipment_for(name),
    }
}

/// Ride's End: "This spell costs {3} less to cast if it targets a tapped
/// permanent."
pub(super) fn generic_cost_reduction_for(name: &str) -> &'static str {
    match name {
        "Ride's End" => "Some(GenericCostReductionDef { generic_per_count: 3, count: DynamicCountDef::SpellTargetsTappedPermanentV1 })",
        _ => super::build_eternal_e8a_v1::generic_cost_reduction_for(name),
    }
}

/// Fixed colors of the primary printed mana ability, where they differ from
/// `produces_mana`: the karoos' one action identifies their whole bundle,
/// and Geothermal Crevice's `{T}: Add {R}` is its primary ability.
pub(super) fn primary_mana_ability_colors(name: &str) -> Option<Vec<&'static str>> {
    match name {
        "Azorius Chancery" => Some(vec!["W"]),
        "Golgari Rot Farm" => Some(vec!["B"]),
        "Orzhov Basilica" => Some(vec!["W"]),
        "Geothermal Crevice" => Some(vec!["R"]),
        _ => None,
    }
}

/// Single printed mana abilities that are not "tap: add one".
pub(super) fn mana_ability_def_for(name: &str) -> &'static str {
    match name {
        // "{T}: Add {W}{U}." One activation adds both colors at once.
        "Azorius Chancery" => "Some(ManaAbilityDef { cost: ManaAbilityCostDef::TapSelf, amount: ManaAbilityAmountDef::FixedColorsV1(&[ManaColor::W, ManaColor::U]), controller_damage: 0, max_activations_per_turn: None })",
        // "{T}: Add {B}{G}."
        "Golgari Rot Farm" => "Some(ManaAbilityDef { cost: ManaAbilityCostDef::TapSelf, amount: ManaAbilityAmountDef::FixedColorsV1(&[ManaColor::B, ManaColor::G]), controller_damage: 0, max_activations_per_turn: None })",
        // "{T}: Add {W}{B}."
        "Orzhov Basilica" => "Some(ManaAbilityDef { cost: ManaAbilityCostDef::TapSelf, amount: ManaAbilityAmountDef::FixedColorsV1(&[ManaColor::W, ManaColor::B]), controller_damage: 0, max_activations_per_turn: None })",
        // "{T}, Remove a depletion counter from this land: Add {G}{G}." The
        // counter and the sacrifice are `eternal_pauper_e8b_v1` hooks.
        "Hickory Woodlot" | "Sandstone Needle" => "Some(ManaAbilityDef { cost: ManaAbilityCostDef::TapSelf, amount: ManaAbilityAmountDef::Fixed(2), controller_damage: 0, max_activations_per_turn: None })",
        _ => super::build_eternal_e9a_v1::mana_ability_def_for(name),
    }
}

/// Additional printed mana abilities. Geothermal Crevice: "{T}, Sacrifice
/// this land: Add {B}{G}." Its action color is {B}.
pub(super) fn additional_mana_abilities_for(name: &str) -> &'static str {
    match name {
        "Geothermal Crevice" => "&[AdditionalManaAbilityDef { colors: &[ManaColor::B], mana_cost: Cost { pips: &[], generic: 0, x_count: 0 }, ability: ManaAbilityDef { cost: ManaAbilityCostDef::TapAndSacrificeSelf, amount: ManaAbilityAmountDef::FixedColorsV1(&[ManaColor::B, ManaColor::G]), controller_damage: 0, max_activations_per_turn: None } }]",
        _ => super::build_eternal_e9a_v1::additional_mana_abilities_for(name),
    }
}

/// Witch's Cottage: "This land enters tapped unless you control three or
/// more other Swamps."
pub(super) fn enters_battlefield_tapped_unless_for(name: &str) -> &'static str {
    match name {
        "Witch's Cottage" => "Some(EntersBattlefieldTappedUnlessDef { controller_controls_other_subtype: Subtype::Swamp, minimum_count: 3 })",
        _ => super::build_eternal_e8a_v1::enters_battlefield_tapped_unless_for(name),
    }
}

/// Card-database tokens for the triggered abilities in
/// `eternal_pauper_e8b_v1::triggers_for`, its hooks and its statics.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Accursed Marauder" => "etb:each_player_apnap_sacrifices_nontoken_creature",
        "Azorius Chancery" | "Golgari Rot Farm" | "Orzhov Basilica" => {
            "etb:return_chosen_controlled_land_to_owners_hand:untargeted"
        }
        "Crimson Fleet Commodore" => "etb:become_monarch",
        "Crossover Collaboration" => {
            "spell:impulse_2_until_end_of_next_turn;teamwork_2:alt_cost_taps_total_power:create_treasure"
        }
        "Desperate Ritual" => "spell:add_rrr;splice_onto_arcane:{1}{R}:announced_as_x:add_rrr_per_splice",
        "First Day of Class" => {
            "spell:this_turn_controlled_creature_enters:plus_one_counter_and_haste_eot;learn:may_discard_draw"
        }
        "Glimpse the Impossible" => {
            "spell:impulse_3_this_turn;next_end_step:remaining_to_graveyard_then_eldrazi_spawn_per_card"
        }
        "Glorious Gale" => {
            "spell:counter_target_creature_spell:if_legendary_ring_tempts_you;ring:level1_legendary_unblockable_by_greater_power;level2_attacks_loot;level3_blocked_by_sacrifice_end_of_combat;level4_combat_damage_each_opponent_loses_3"
        }
        "God-Pharaoh's Faithful" => "cast_blue_black_or_red_spell:gain_life:1",
        "Goblin Anarchomancer" => "static:controller_red_or_green_spells_cost_generic_less:1",
        "Sunscape Familiar" => "static:controller_green_or_blue_spells_cost_generic_less:1",
        "Thornscape Familiar" => "static:controller_red_or_white_spells_cost_generic_less:1",
        "Goliath Paladin" => "etb:take_initiative",
        "Heaped Harvest" => "etb_and_sacrificed:may_search_basic_land_battlefield_tapped",
        "Hickory Woodlot" | "Sandstone Needle" => {
            "enters_with_depletion_counters:2;mana:remove_depletion_counter_then_sacrifice_if_none"
        }
        "Khalni Garden" => "etb:create_token:Plant Token",
        "Lonely Arroyo" => "etb:target_opponent:damage:1",
        "Mirran Banesplitter" => "etb:attach_source_to_target_controlled_creature",
        "Mortuary Mire" => "etb:may_put_target_own_graveyard_creature_card_on_library_top",
        "Witch's Cottage" => {
            "etb_untapped:may_put_target_own_graveyard_creature_card_on_library_top"
        }
        "Oroku Saki, Shredder Rising" => {
            "sneak:declare_blockers_step:enters_tapped_and_attacking;combat_damage_player:draw:1:lose_life:1"
        }
        "Rancid Earth" => "spell:destroy_target_land;threshold:also_damage_each_creature_and_player:1",
        "Sea Gate Oracle" => "etb:look_top2:one_to_hand:other_bottom",
        "Red Elemental Token" => {
            "cda:power_toughness:instant_sorcery_cards_in_controller_graveyard_plus_owned_flashback_cards_in_exile"
        }
        "Twinned Vision" => "spell:draw_1_or_2_if_not_cast_from_hand",
        "Desert" => "activate_only_during_end_of_combat_step",
        "Geothermal Crevice" => "mana:tap_sacrifice:add_bg",
        _ => super::build_eternal_e8a_v1::trigger_recipe_for(name),
    }
}
