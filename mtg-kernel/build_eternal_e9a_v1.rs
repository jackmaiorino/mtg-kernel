//! Card-database recipes for the eternal catalog's wave E9a, the Legacy
//! long tail: Selesnya Depths, Red Stompy, LED Dredge, Painter and Cephalid
//! Breakfast, and the Initiative, Mono-Black and Death and Taxes cards of
//! the decks that also need wave E9b. `build_eternal_e8a_v1` and the other
//! eternal tables fall through to these; runtime programs live in
//! `src/eternal_legacy_e9a_v1.rs`. Every row is keyed by a name that occurs
//! only in the eternal registry.

use super::{
    AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, PermanentFilterRecipe, Special,
};
use AbilityCostRecipe::{ExileSelf, ManaCost, SacrificeSelf, Tap};

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

/// A program of `eternal_legacy_e9a_v1`.
const fn program(path: &'static str) -> AbilityEffectRecipe {
    AbilityEffectRecipe::StandardProgram(path)
}

/// Broadside Bombardiers: "Boast -- Sacrifice another creature or artifact:
/// This creature deals damage equal to 2 plus the sacrificed permanent's
/// mana value to any target. (Activate only if this creature attacked this
/// turn and only once each turn.)" The attack condition is
/// `eternal_legacy_e9a_v1::statics::activation_allowed`.
const BROADSIDE_BOMBARDIERS: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    max_activations_per_turn: Some(1),
    ..instant(
        &[AbilityCostRecipe::SacrificeControlled {
            count: 1,
            filter: PermanentFilterRecipe::AnotherArtifactOrCreature,
        }],
        program("crate::eternal_legacy_e9a_v1::bombardiers_boast"),
        "AnyTarget",
    )
}];

/// Cephalid Coliseum: "Threshold -- {U}, {T}, Sacrifice this land: Target
/// player draws three cards, then discards three cards. Activate only if
/// there are seven or more cards in your graveyard." Its "{T}: Add {U}.
/// This land deals 1 damage to you." is the mana ability.
const CEPHALID_COLISEUM: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{U}"), Tap, SacrificeSelf],
    program("crate::eternal_legacy_e9a_v1::coliseum_threshold"),
    "AnyPlayer",
)];

/// Cephalid Illusionist: "{2}{U}, {T}: Prevent all combat damage that would
/// be dealt to and dealt by target creature you control this turn."
const CEPHALID_ILLUSIONIST: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{2}{U}"), Tap],
    program("crate::eternal_legacy_e9a_v1::illusionist_prevent"),
    "ControlledCreature",
)];

/// Elvish Reclaimer: "{2}, {T}, Sacrifice a land: Search your library for a
/// land card, put it onto the battlefield tapped, then shuffle."
const ELVISH_RECLAIMER: [ActivatedAbilityRecipe; 1] = [instant(
    &[
        ManaCost("{2}"),
        Tap,
        AbilityCostRecipe::SacrificeControlled {
            count: 1,
            filter: PermanentFilterRecipe::Land,
        },
    ],
    AbilityEffectRecipe::Program {
        name: "fetch_land_tapped_e9a",
        effect: "EffectOp::SearchLibraryCardsToDestination { player: PlayerRef::Controller, filter: crate::effect::LibraryCardFilter::AnyLand, max_targets: 1, destination: crate::effect::LibrarySearchDestinationV1::Battlefield { tapped: true } }",
    },
    "None",
)];

/// Grindstone: "{3}, {T}: Target player mills two cards. If two cards that
/// share a color were milled this way, repeat this process."
const GRINDSTONE: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{3}"), Tap],
    program("crate::eternal_legacy_e9a_v1::grindstone"),
    "AnyPlayer",
)];

/// Horizon Canopy: "{1}, {T}, Sacrifice this land: Draw a card." Its "{T},
/// Pay 1 life: Add {G} or {W}." is an additional mana ability.
const HORIZON_CANOPY: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{1}"), Tap, SacrificeSelf],
    AbilityEffectRecipe::DrawCards(1),
    "None",
)];

/// Knight of the Reliquary: "{T}, Sacrifice a Forest or Plains: Search your
/// library for a land card, put it onto the battlefield, then shuffle."
const KNIGHT_OF_THE_RELIQUARY: [ActivatedAbilityRecipe; 1] = [instant(
    &[
        Tap,
        AbilityCostRecipe::SacrificeControlled {
            count: 1,
            filter: PermanentFilterRecipe::ForestOrPlains,
        },
    ],
    AbilityEffectRecipe::Program {
        name: "fetch_land_untapped_e9a",
        effect: "EffectOp::SearchLibraryCardsToDestination { player: PlayerRef::Controller, filter: crate::effect::LibraryCardFilter::AnyLand, max_targets: 1, destination: crate::effect::LibrarySearchDestinationV1::Battlefield { tapped: false } }",
    },
    "None",
)];

/// Mishra's Research Desk: "{1}, {T}, Sacrifice this artifact: Exile the
/// top two cards of your library. Choose one of them. Until the end of your
/// next turn, you may play that card." and "Unearth {1}{R}".
const MISHRAS_RESEARCH_DESK: [ActivatedAbilityRecipe; 2] = [
    instant(
        &[ManaCost("{1}"), Tap, SacrificeSelf],
        program("crate::eternal_legacy_e9a_v1::research_desk"),
        "None",
    ),
    ActivatedAbilityRecipe {
        cost: &[ManaCost("{1}{R}")],
        effect: AbilityEffectRecipe::Unearth,
        activation_zone: "Graveyard",
        sorcery_speed_only: true,
        target_spec: "None",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    },
];

/// Nomads en-Kor: "{0}: The next 1 damage that would be dealt to this
/// creature this turn is dealt to target creature you control instead."
const NOMADS_EN_KOR: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{0}")],
    program("crate::eternal_legacy_e9a_v1::nomads_redirect"),
    "ControlledCreature",
)];

/// Shuko: "Equip {0}" (702.6a): attach to target creature you control, only
/// as a sorcery.
const SHUKO: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    sorcery_speed_only: true,
    ..instant(
        &[ManaCost("{0}")],
        AbilityEffectRecipe::AttachSourceToTarget,
        "ControlledCreature",
    )
}];

/// Simian Spirit Guide: "Exile this card from your hand: Add {R}." A mana
/// ability: the engine performs it without the stack
/// (`eternal_legacy_e9a_v1::statics::special_activation`), offered as
/// ability 0.
const SIMIAN_SPIRIT_GUIDE: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[ExileSelf],
    effect: program("crate::eternal_legacy_e9a_v1::spirit_guide_mana"),
    activation_zone: "Hand",
    sorcery_speed_only: false,
    target_spec: "None",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

/// Tolaria: "{T}: Target creature loses banding and all 'bands with other'
/// abilities until end of turn. Activate only during any upkeep step." No
/// card in this catalog has banding, so the ability changes nothing; the
/// upkeep restriction is `eternal_legacy_e9a_v1::statics::activation_allowed`.
const TOLARIA: [ActivatedAbilityRecipe; 1] = [instant(
    &[Tap],
    program("crate::eternal_legacy_e9a_v1::tolaria_banding"),
    "Creature",
)];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Broadside Bombardiers" => &BROADSIDE_BOMBARDIERS,
        "Cephalid Coliseum" => &CEPHALID_COLISEUM,
        "Cephalid Illusionist" => &CEPHALID_ILLUSIONIST,
        "Elvish Reclaimer" => &ELVISH_RECLAIMER,
        "Grindstone" => &GRINDSTONE,
        "Horizon Canopy" => &HORIZON_CANOPY,
        "Knight of the Reliquary" => &KNIGHT_OF_THE_RELIQUARY,
        "Mishra's Research Desk" => &MISHRAS_RESEARCH_DESK,
        "Nomads en-Kor" => &NOMADS_EN_KOR,
        "Shuko" => &SHUKO,
        "Simian Spirit Guide" => &SIMIAN_SPIRIT_GUIDE,
        "Tolaria" => &TOLARIA,
        _ => super::build_eternal_p1a_v1::activated_ability_recipes_for(name),
    }
}

/// A spell whose program is a runtime function of `eternal_legacy_e9a_v1`.
const fn spell(target_spec: &'static str, program: &'static str) -> Special {
    Special::StandardProgram {
        target_spec,
        program,
    }
}

/// Spell programs.
pub(super) fn special_for(name: &str) -> Option<Special> {
    Some(match name {
        "Breakthrough" => spell(
            "TargetSpec::None",
            "crate::eternal_legacy_e9a_v1::breakthrough",
        ),
        "Green Sun's Zenith" => spell(
            "TargetSpec::None",
            "crate::eternal_legacy_e9a_v1::green_suns_zenith",
        ),
        "Inquisition of Kozilek" => spell(
            "TargetSpec::AnyPlayer",
            "crate::eternal_legacy_e9a_v1::inquisition_of_kozilek",
        ),
        // Split second is `eternal_legacy_e9a_v1::statics::split_second`.
        "Legolas's Quick Reflexes" => spell(
            "TargetSpec::Creature",
            "crate::eternal_legacy_e9a_v1::legolas_quick_reflexes",
        ),
        // Its kicker is `kicker_cost_for` below.
        "Orim's Chant" => spell(
            "TargetSpec::AnyPlayer",
            "crate::eternal_legacy_e9a_v1::orims_chant",
        ),
        // Its flashback is `flashback_for` below.
        "Otherworldly Gaze" => spell(
            "TargetSpec::None",
            "crate::eternal_legacy_e9a_v1::otherworldly_gaze",
        ),
        // The land faces are `transform_face_for` below. The division of the
        // damage is announced with the targets
        // (`eternal_legacy_e9a_v1::targets`).
        "Shatterskull Smashing" => spell(
            "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::DividedXDamageUpToTwoV1)",
            "crate::eternal_legacy_e9a_v1::shatterskull_smashing",
        ),
        "Sundering Eruption" => spell(
            "TargetSpec::Land",
            "crate::eternal_legacy_e9a_v1::sundering_eruption",
        ),
        _ => return super::build_eternal_p1a_v1::special_for(name),
    })
}

/// Printed evergreen keywords, in printed order.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Anger" => &["Keywords::HASTE"],
        "Ashen Rider" => &["Keywords::FLYING"],
        "Broadside Bombardiers" => &["Keywords::MENACE", "Keywords::HASTE"],
        "Caves of Chaos Adventurer" | "Chaos Defiler" => &["Keywords::TRAMPLE"],
        "Endurance" => &["Keywords::FLASH", "Keywords::REACH"],
        "Fury" => &["Keywords::DOUBLE_STRIKE"],
        "Poxwalkers" => &["Keywords::DEATHTOUCH"],
        "Samwise the Stouthearted" => &["Keywords::FLASH"],
        "Super Shredder" => &["Keywords::MENACE"],
        "Undermountain Adventurer" => &["Keywords::VIGILANCE"],
        "Goblin Haste Token" => &["Keywords::HASTE"],
        "Black Insect Token" => &["Keywords::FLYING"],
        _ => super::build_eternal_p1a_v1::keywords_for(name),
    }
}

/// Alternative costs. Fury: "Evoke--Exile a red card from your hand."
/// Endurance: "Evoke--Exile a green card from your hand." Their sacrifice
/// is wave E7b's evoked entry trigger.
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    match name {
        "Fury" => "Some(AltCostDef { components: &[CostComponent::ExileCardFromHandWithColorV1(ManaColor::R)], condition: AltCostCondition::Always })",
        "Endurance" => "Some(AltCostDef { components: &[CostComponent::ExileCardFromHandWithColorV1(ManaColor::G)], condition: AltCostCondition::Always })",
        _ => super::build_eternal_p1a_v1::alt_cost_for(name),
    }
}

/// Orim's Chant: "Kicker {W}".
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    match name {
        "Orim's Chant" => Some("{W}"),
        _ => None,
    }
}

/// Otherworldly Gaze: "Flashback {1}{U}".
pub(super) fn flashback_for(name: &str) -> String {
    match name {
        "Otherworldly Gaze" => {
            let (pips, generic, x_count) = super::parse_cost("{1}{U}");
            format!(
                "Some(FlashbackDef {{ cost: &[CostComponent::Mana(Cost {{ pips: &[{}], generic: {generic}, x_count: {x_count} }})] }})",
                pips.join(", ")
            )
        }
        _ => super::build_eternal_p1a_v1::flashback_for(name),
    }
}

/// Ox of Agonas: "Escape--{R}{R}, Exile eight other cards from your
/// graveyard." "This creature escapes with a +1/+1 counter on it" is
/// `eternal_legacy_e9a_v1::statics::apply_entry_counters`.
pub(super) fn escape_for(name: &str) -> String {
    match name {
        "Ox of Agonas" => {
            let (pips, generic, x_count) = super::parse_cost("{R}{R}");
            format!(
                "Some(EscapeDef {{ cost: &[CostComponent::Mana(Cost {{ pips: &[{}], generic: {generic}, x_count: {x_count} }}), CostComponent::ExileOtherCardsFromOwnGraveyard(8)] }})",
                pips.join(", ")
            )
        }
        _ => "None".to_string(),
    }
}

/// Equipment statics. Shuko: "Equipped creature gets +1/+0."
pub(super) fn equipment_for(name: &str) -> &'static str {
    match name {
        "Shuko" => "Some(EquipmentDef { power_delta: 1, toughness_delta: 0, add_subtype: None, controller_turn_keywords: Keywords::NONE, other_turn_keywords: Keywords::NONE, noncreature_spell_damage_to_each_opponent: 0, job_select: false, granted_activated_ability: None, pt_controller_turn_only: false })",
        _ => "None",
    }
}

/// Single printed mana abilities that are not "tap: add one". Cephalid
/// Coliseum: "{T}: Add {U}. This land deals 1 damage to you." Eumidian
/// Hatchery: "{T}, Pay 1 life: Add {B}. Put a hatchling counter on this
/// land." (the counter is `eternal_legacy_e9a_v1::statics::mana_ability_paid`).
pub(super) fn mana_ability_def_for(name: &str) -> &'static str {
    match name {
        "Cephalid Coliseum" => "Some(ManaAbilityDef { cost: ManaAbilityCostDef::TapSelf, amount: ManaAbilityAmountDef::Fixed(1), controller_damage: 1, max_activations_per_turn: None })",
        "Eumidian Hatchery" => "Some(ManaAbilityDef { cost: ManaAbilityCostDef::TapSelfPayLife(1), amount: ManaAbilityAmountDef::Fixed(1), controller_damage: 0, max_activations_per_turn: None })",
        _ => super::build_eternal_p1a_v1::mana_ability_def_for(name),
    }
}

/// Additional printed mana abilities. Horizon Canopy: "{T}, Pay 1 life: Add
/// {G} or {W}."
pub(super) fn additional_mana_abilities_for(name: &str) -> &'static str {
    match name {
        "Horizon Canopy" => "&[AdditionalManaAbilityDef { colors: &[ManaColor::G, ManaColor::W], mana_cost: Cost { pips: &[], generic: 0, x_count: 0 }, ability: ManaAbilityDef { cost: ManaAbilityCostDef::TapSelfPayLife(1), amount: ManaAbilityAmountDef::Fixed(1), controller_damage: 0, max_activations_per_turn: None } }]",
        _ => "&[]",
    }
}

/// Undermountain Adventurer: "{T}: Add {G}{G}. If you've completed a
/// dungeon, add six {G} instead."
pub(super) fn conditional_tap_yield_for(name: &str) -> &'static str {
    match name {
        "Undermountain Adventurer" => "Some(DynamicValueDef::SixIfDungeonCompletedElseTwoV1)",
        _ => "None",
    }
}

pub(super) fn conditional_tap_yield_program_for(name: &str) -> Option<&'static str> {
    match name {
        "Undermountain Adventurer" => Some("mana_ability_add_undermountain_adventurer"),
        _ => None,
    }
}

/// Modal double-faced cards' land faces, played from hand: "As this land
/// enters, you may pay 3 life. If you don't, it enters tapped. {T}: Add
/// {R}."
pub(super) fn transform_face_for(name: &str) -> &'static str {
    match name {
        "Shatterskull Smashing" => "Some(TransformFaceDef { name: \"Shatterskull, the Hammer Pass\", types: &[CardType::Land], subtypes: &[], colors: &[], power: None, toughness: None, keywords: Keywords::NONE })",
        "Sundering Eruption" => "Some(TransformFaceDef { name: \"Volcanic Fissure\", types: &[CardType::Land], subtypes: &[], colors: &[], power: None, toughness: None, keywords: Keywords::NONE })",
        _ => "None",
    }
}

pub(super) fn transform_face_name_for(name: &str) -> Option<&'static str> {
    match name {
        "Shatterskull Smashing" => Some("Shatterskull, the Hammer Pass"),
        "Sundering Eruption" => Some("Volcanic Fissure"),
        _ => None,
    }
}

/// Card-database tokens for the triggered abilities in
/// `eternal_legacy_e9a_v1::triggers_for`, its hooks and its statics.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Anger" => "graveyard_static:controls_mountain:controlled_creatures_have_haste",
        "Ashen Rider" => "etb_or_dies:exile_target_permanent",
        "Blood Moon" | "Magus of the Moon" => "static:nonbasic_lands_are_mountains",
        "Breakthrough" => "spell:draw_4:keep_x_cards_discard_rest",
        "Bridge from Below" => {
            "graveyard:own_nontoken_creature_dies:zombie_token;graveyard:opponent_creature_dies:exile_self"
        }
        "Broadside Bombardiers" => {
            "boast:sacrifice_another_creature_or_artifact:damage_2_plus_mana_value_any_target"
        }
        "Caves of Chaos Adventurer" => {
            "etb:take_initiative;attacks:exile_top:play_this_turn:free_if_completed_dungeon"
        }
        "Cephalid Coliseum" => {
            "mana:u:damage_1;threshold:target_player_draws_3_discards_3"
        }
        "Cephalid Illusionist" => {
            "becomes_target:mill_3;activated:prevent_combat_damage_to_and_by_controlled_creature"
        }
        "Chaos Defiler" => "etb_or_dies:destroy_chosen_opponent_nonland_permanent",
        "Echoing Deeps" => "as_enters:may_copy_graveyard_land_tapped:also_cave",
        "Elvish Reclaimer" => {
            "static:plus_2_plus_2_if_three_land_cards_in_graveyard;activated:sacrifice_land:search_land_tapped"
        }
        "Endurance" => {
            "etb:up_to_one_target_player:graveyard_to_library_bottom_random;evoke:exile_green_card;etb_if_evoked:sacrifice_source"
        }
        "Ensnaring Bridge" => "static:power_greater_than_hand_size_cant_attack",
        "Eumidian Hatchery" => {
            "mana:pay_1_life:b:hatchling_counter;put_into_graveyard_from_battlefield:insect_per_hatchling_counter"
        }
        "Fury" => {
            "etb:4_damage_divided_creatures_or_planeswalkers;evoke:exile_red_card;etb_if_evoked:sacrifice_source"
        }
        "Goblin Rabblemaster" => {
            "static:other_goblins_attack_each_combat;controller_combat:goblin_haste_token;attacks:plus_1_per_other_attacking_goblin"
        }
        "Green Sun's Zenith" => {
            "spell:search_green_creature_mana_value_at_most_x:battlefield;shuffle_self_into_library"
        }
        "Grindstone" => "activated:target_player_mills_2:repeat_while_sharing_color",
        "Inquisition of Kozilek" => {
            "spell:target_player_reveals_hand:discard_nonland_mana_value_at_most_3"
        }
        "Knight of the Reliquary" => {
            "static:plus_1_plus_1_per_land_card_in_graveyard;activated:sacrifice_forest_or_plains:search_land"
        }
        "Legolas's Quick Reflexes" => {
            "split_second;spell:untap_target_creature:reach_hexproof:tapped_trigger_damage_power_up_to_one_creature"
        }
        "Loki, God of Mischief" => "player_or_permanent_targeted_by_your_ability:draw_1:once_each_turn",
        "Mishra's Research Desk" => {
            "activated:exile_top_2:choose_one:play_until_end_of_next_turn;unearth_next_end_step:exile"
        }
        "Nomads en-Kor" => "activated:redirect_next_1_damage_to_target_controlled_creature",
        "Orim's Chant" => "spell:target_player_cant_cast_spells_this_turn;kicked:creatures_cant_attack",
        "Otherworldly Gaze" => "spell:surveil_3",
        "Ox of Agonas" => "etb:discard_hand:draw_3;escapes_with_plus_one_counter",
        "Painter's Servant" => "as_enters_choose_color;static:all_cards_spells_permanents_chosen_color",
        "Poxwalkers" => "graveyard:cast_spell_not_from_hand:return_tapped",
        "Samwise the Stouthearted" => {
            "etb:up_to_one_permanent_card_put_into_graveyard_from_battlefield_this_turn:to_hand:ring_tempts"
        }
        "Sejiri Steppe" => "enters_tapped;etb:controlled_creature_protection_from_chosen_color",
        "Shatterskull Smashing" => {
            "spell:x_damage_divided_up_to_two:twice_if_x_at_least_6;land_face_play_from_hand:as_enters_may_pay_3_life_else_tapped"
        }
        "Simian Spirit Guide" => "hand_mana:exile_self:add_r",
        "Sundering Eruption" => {
            "spell:destroy_target_land:its_controller_may_search_basic_tapped:nonflying_cant_block;land_face_play_from_hand:as_enters_may_pay_3_life_else_tapped"
        }
        "Super Shredder" => "another_permanent_leaves_battlefield:plus_one_counter",
        "Tolaria" => "activated:upkeep_only:target_creature_loses_banding",
        "Undermountain Adventurer" => "etb:take_initiative;mana:gg_or_six_if_completed_dungeon",
        _ => super::build_eternal_p1a_v1::trigger_recipe_for(name),
    }
}
