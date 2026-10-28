//! Card-database recipes for the eternal catalog's wave E8a: Legacy Death
//! and Taxes (Yorion), Hogaak, the Jeskai, Azorius and Dimir control lists,
//! Beseech and Saga Storm, Reanimator and 8-Cast. `build_eternal_e7b_v1` and
//! the other eternal tables fall through to these; runtime programs live in
//! `src/eternal_legacy_e8a_v1.rs`. Every row is keyed by a name that occurs
//! only in the eternal registry.

use super::{
    AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, PermanentFilterRecipe, Special,
};
use AbilityCostRecipe::{Loyalty, ManaCost, PayLife, SacrificeSelf, Tap};

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

/// An activated ability from the battlefield, only as a sorcery.
const fn sorcery(
    cost: &'static [AbilityCostRecipe],
    effect: AbilityEffectRecipe,
    target_spec: &'static str,
) -> ActivatedAbilityRecipe {
    ActivatedAbilityRecipe {
        sorcery_speed_only: true,
        ..instant(cost, effect, target_spec)
    }
}

/// "Equip [cost]" (702.6a): attach to target creature you control, only as
/// a sorcery.
const fn equip(cost: &'static [AbilityCostRecipe]) -> ActivatedAbilityRecipe {
    sorcery(
        cost,
        AbilityEffectRecipe::AttachSourceToTarget,
        "ControlledCreature",
    )
}

const fn program(path: &'static str) -> AbilityEffectRecipe {
    AbilityEffectRecipe::StandardProgram(path)
}

/// Aether Vial: "{T}: You may put a creature card with mana value equal to
/// the number of charge counters on this artifact from your hand onto the
/// battlefield."
const AETHER_VIAL: [ActivatedAbilityRecipe; 1] = [instant(
    &[Tap],
    program("crate::eternal_legacy_e8a_v1::aether_vial_put"),
    "None",
)];

/// Chandra, Chill of Compliance: "+1: Surveil 1. If you put a noncreature,
/// nonland card into your graveyard this way, put that card into your
/// hand.", "+1: Add {U}. Spend this mana only to cast a noncreature
/// spell.", "-X: Tap target artifact or creature. Put X stun counters on
/// it." and "-6: You get an emblem with 'Whenever you cast a spell, draw a
/// card.'" A loyalty ability is never a mana ability (605.1a), so the
/// second one uses the stack.
const CHANDRA: [ActivatedAbilityRecipe; 4] = [
    sorcery(
        &[Loyalty(1)],
        program("crate::eternal_legacy_e8a_v1::chandra_surveil"),
        "None",
    ),
    sorcery(
        &[Loyalty(1)],
        program("crate::eternal_legacy_e8a_v1::chandra_mana"),
        "None",
    ),
    sorcery(
        &[AbilityCostRecipe::LoyaltyX],
        program("crate::eternal_legacy_e8a_v1::chandra_stun"),
        "StandardV1(crate::standard_cards_v1::StandardTargetV1::ArtifactOrCreatureV1)",
    ),
    sorcery(
        &[Loyalty(-6)],
        program("crate::eternal_legacy_e8a_v1::chandra_emblem"),
        "None",
    ),
];

/// Emperor of Bones: "{1}{B}: Adapt 2."
const EMPEROR_OF_BONES: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{1}{B}")],
    program("crate::eternal_legacy_e8a_v1::adapt_two"),
    "None",
)];

/// Lich's Relic: "Equip {2}".
const LICHS_RELIC: [ActivatedAbilityRecipe; 1] = [equip(&[ManaCost("{2}")])];

/// Lion Sash: "{W}: Exile target card from a graveyard. If it was a
/// permanent card, put a +1/+1 counter on this permanent." and "Reconfigure
/// {2}" (702.151): "{2}: Attach to target creature you control" and "{2}:
/// Unattach", each only as a sorcery. The second is offered only while it
/// is attached (`eternal_legacy_e8a_v1::statics::activation_allowed`).
const LION_SASH: [ActivatedAbilityRecipe; 3] = [
    instant(
        &[ManaCost("{W}")],
        program("crate::eternal_legacy_e8a_v1::lion_sash_exile"),
        "CardInGraveyards",
    ),
    sorcery(
        &[ManaCost("{2}")],
        AbilityEffectRecipe::AttachSourceToTarget,
        "AnotherControlledCreature",
    ),
    sorcery(
        &[ManaCost("{2}")],
        program("crate::eternal_legacy_e8a_v1::lion_sash_unattach"),
        "None",
    ),
];

/// Loran of the Third Path: "{T}: You and target opponent each draw a
/// card."
const LORAN: [ActivatedAbilityRecipe; 1] = [instant(
    &[Tap],
    program("crate::eternal_legacy_e8a_v1::loran_draw"),
    "TargetOpponent",
)];

/// Meteor Sword: "Equip {3}".
const METEOR_SWORD: [ActivatedAbilityRecipe; 1] = [equip(&[ManaCost("{3}")])];

/// Pre-War Formalwear: "Equip {3}".
const PRE_WAR_FORMALWEAR: [ActivatedAbilityRecipe; 1] = [equip(&[ManaCost("{3}")])];

/// Prismatic Vista: "{T}, Pay 1 life, Sacrifice this land: Search your
/// library for a basic land card, put it onto the battlefield, then
/// shuffle."
const PRISMATIC_VISTA: [ActivatedAbilityRecipe; 1] = [instant(
    &[Tap, PayLife(1), SacrificeSelf],
    AbilityEffectRecipe::Program {
        name: "fetch_basic_land_untapped_e8a",
        effect: "EffectOp::SearchLibraryCardsToDestination { player: PlayerRef::Controller, filter: crate::effect::LibraryCardFilter::BasicLand, max_targets: 1, destination: crate::effect::LibrarySearchDestinationV1::Battlefield { tapped: false } }",
    },
    "None",
)];

/// Rain of Filth's granted ability: 'Until end of turn, lands you control
/// gain "Sacrifice this land: Add {B}."' The engine offers it on each land
/// the resolved spell named, one index past that land's printed abilities,
/// and performs it without the stack (a mana ability); an instant never
/// activates it itself.
const RAIN_OF_FILTH: [ActivatedAbilityRecipe; 1] = [instant(
    &[SacrificeSelf],
    program("crate::eternal_legacy_e8a_v1::rain_of_filth_mana"),
    "None",
)];

/// Stoneforge Mystic: "{1}{W}, {T}: You may put an Equipment card from your
/// hand onto the battlefield."
const STONEFORGE_MYSTIC: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{1}{W}"), Tap],
    program("crate::eternal_legacy_e8a_v1::stoneforge_put"),
    "None",
)];

/// Sylvan Safekeeper: "Sacrifice a land: Target creature you control gains
/// shroud until end of turn."
const SYLVAN_SAFEKEEPER: [ActivatedAbilityRecipe; 1] = [instant(
    &[AbilityCostRecipe::SacrificeControlled {
        count: 1,
        filter: PermanentFilterRecipe::Land,
    }],
    AbilityEffectRecipe::GrantTargetKeywordUntilEndOfTurn("SHROUD"),
    "ControlledCreature",
)];

/// Teferi, Time Raveler: "+1: Until your next turn, you may cast sorcery
/// spells as though they had flash." and "-3: Return up to one target
/// artifact, creature, or enchantment to its owner's hand. Draw a card."
const TEFERI: [ActivatedAbilityRecipe; 2] = [
    sorcery(
        &[Loyalty(1)],
        program("crate::eternal_legacy_e8a_v1::teferi_plus_one"),
        "None",
    ),
    sorcery(
        &[Loyalty(-3)],
        program("crate::eternal_legacy_e8a_v1::teferi_minus_three"),
        "StandardV1(crate::standard_cards_v1::StandardTargetV1::UpToOneArtifactCreatureOrEnchantmentV1)",
    ),
];

/// Wight of the Reliquary: "{T}, Sacrifice another creature: Search your
/// library for a land card, put it onto the battlefield tapped, then
/// shuffle."
const WIGHT_OF_THE_RELIQUARY: [ActivatedAbilityRecipe; 1] = [instant(
    &[Tap, AbilityCostRecipe::SacrificeOtherControlledCreatures(1)],
    AbilityEffectRecipe::Program {
        name: "fetch_land_tapped_e8a",
        effect: "EffectOp::SearchLibraryCardsToDestination { player: PlayerRef::Controller, filter: crate::effect::LibraryCardFilter::AnyLand, max_targets: 1, destination: crate::effect::LibrarySearchDestinationV1::Battlefield { tapped: true } }",
    },
    "None",
)];

/// Wishclaw Talisman: "{1}, {T}, Remove a wish counter from this artifact:
/// Search your library for a card, put it into your hand, then shuffle. An
/// opponent gains control of this artifact. Activate only during your
/// turn." Its wish counters, their removal as the ability is paid and the
/// turn restriction are `eternal_legacy_e8a_v1::statics`.
const WISHCLAW_TALISMAN: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{1}"), Tap],
    program("crate::eternal_legacy_e8a_v1::wishclaw_search"),
    "None",
)];

/// Yorion, Sky Nomad's companion reminder: "you may put it into your hand
/// from outside the game for {3} as a sorcery." The engine performs it as
/// a special action (702.139c) from the companion's place in exile.
const YORION: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[ManaCost("{3}")],
    effect: AbilityEffectRecipe::StandardProgram("crate::eternal_lurrus_v1::companion_to_hand"),
    activation_zone: "Exile",
    sorcery_speed_only: true,
    target_spec: "None",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Aether Vial" => &AETHER_VIAL,
        "Chandra, Chill of Compliance" => &CHANDRA,
        "Emperor of Bones" => &EMPEROR_OF_BONES,
        "Lich's Relic" => &LICHS_RELIC,
        "Lion Sash" => &LION_SASH,
        "Loran of the Third Path" => &LORAN,
        "Meteor Sword" => &METEOR_SWORD,
        "Pre-War Formalwear" => &PRE_WAR_FORMALWEAR,
        "Prismatic Vista" => &PRISMATIC_VISTA,
        "Rain of Filth" => &RAIN_OF_FILTH,
        "Stoneforge Mystic" => &STONEFORGE_MYSTIC,
        "Sylvan Safekeeper" => &SYLVAN_SAFEKEEPER,
        "Teferi, Time Raveler" => &TEFERI,
        "Wight of the Reliquary" => &WIGHT_OF_THE_RELIQUARY,
        "Wishclaw Talisman" => &WISHCLAW_TALISMAN,
        "Yorion, Sky Nomad" => &YORION,
        _ => super::build_eternal_e9a_v1::activated_ability_recipes_for(name),
    }
}

/// A spell whose program is a runtime function of `eternal_legacy_e8a_v1`.
const fn spell(target_spec: &'static str, program: &'static str) -> Special {
    Special::StandardProgram {
        target_spec,
        program,
    }
}

/// Spell programs.
pub(super) fn special_for(name: &str) -> Option<Special> {
    Some(match name {
        "Ad Nauseam" => spell(
            "TargetSpec::None",
            "crate::eternal_legacy_e8a_v1::ad_nauseam",
        ),
        // Its discard-or-life additional cost is `additional_cost_for` and
        // `alt_cost_for` below.
        "Bitter Triumph" => spell(
            "TargetSpec::CreatureOrPlaneswalker",
            "crate::eternal_legacy_e8a_v1::destroy_target",
        ),
        "Cabal Ritual" => spell(
            "TargetSpec::None",
            "crate::eternal_legacy_e8a_v1::cabal_ritual",
        ),
        "Erode" => spell(
            "TargetSpec::CreatureOrPlaneswalker",
            "crate::eternal_legacy_e8a_v1::destroy_then_search_basic_tapped",
        ),
        "Infernal Tutor" => spell(
            "TargetSpec::None",
            "crate::eternal_legacy_e8a_v1::infernal_tutor",
        ),
        // Its free cast is `alt_cost_for` below.
        "Once Upon a Time" => spell(
            "TargetSpec::None",
            "crate::eternal_legacy_e8a_v1::once_upon_a_time",
        ),
        "Rain of Filth" => spell(
            "TargetSpec::None",
            "crate::eternal_legacy_e8a_v1::rain_of_filth",
        ),
        // "This spell can't be countered" is its `cant_be_countered`
        // mechanic.
        "Supreme Verdict" => spell(
            "TargetSpec::None",
            "crate::eternal_legacy_e8a_v1::supreme_verdict",
        ),
        _ => return super::build_eternal_e9a_v1::special_for(name),
    })
}

/// Printed evergreen keywords, in printed order.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Canoptek Scarab Swarm" | "Flickerwisp" | "Thought Monitor" | "Yorion, Sky Nomad" => {
            &["Keywords::FLYING"]
        }
        "Cloak and Dagger, Entwined" => &["Keywords::DEATHTOUCH", "Keywords::LIFELINK"],
        "Hogaak, Arisen Necropolis" => &["Keywords::TRAMPLE"],
        "Loran of the Third Path" | "Wight of the Reliquary" => &["Keywords::VIGILANCE"],
        "Solitude" => &["Keywords::FLASH", "Keywords::LIFELINK"],
        "Valgavoth's Lair" => &["Keywords::HEXPROOF"],
        "White Orchid Phantom" => &["Keywords::FLYING", "Keywords::FIRST_STRIKE"],
        "Insect Artifact Token" => &["Keywords::FLYING"],
        _ => super::build_eternal_e9a_v1::keywords_for(name),
    }
}

/// Alternative costs. Solitude: "Evoke--Exile a white card from your hand."
/// (its sacrifice is wave E7b's evoked entry trigger); Overlord of the
/// Balemurk: "Impending 5--{1}{B}"; Once Upon a Time: "If this spell is the
/// first spell you've cast this game, you may cast it without paying its
/// mana cost."; Bitter Triumph's "pay 3 life" branch of its additional
/// cost, which replaces the discard
/// (`eternal_legacy_e8a_v1::alternative_skips_additional_cost`); Hogaak,
/// Arisen Necropolis: "You can't spend mana to cast this spell. Convoke,
/// delve": its mana cost is paid only by convoking creatures and, for the
/// generic rest, delving graveyard cards (`eternal_legacy_e8a_v1::hogaak`).
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    match name {
        "Solitude" => "Some(AltCostDef { components: &[CostComponent::ExileCardFromHandWithColorV1(ManaColor::W)], condition: AltCostCondition::Always })",
        "Overlord of the Balemurk" => "Some(AltCostDef { components: &[CostComponent::Mana(Cost { pips: &[Pip::Colored(ManaColor::B)], generic: 1, x_count: 0 })], condition: AltCostCondition::ImpendingFromHand { time_counters: 5 } })",
        "Once Upon a Time" => "Some(AltCostDef { components: &[CostComponent::Mana(Cost { pips: &[], generic: 0, x_count: 0 })], condition: AltCostCondition::FirstSpellThisGameV1 })",
        "Bitter Triumph" => "Some(AltCostDef { components: &[CostComponent::Mana(Cost { pips: &[Pip::Colored(ManaColor::B)], generic: 1, x_count: 0 }), CostComponent::PayLife(3)], condition: AltCostCondition::Always })",
        "Hogaak, Arisen Necropolis" => "Some(AltCostDef { components: &[CostComponent::ConvokeMana(Cost { pips: &[Pip::Hybrid(ManaColor::B, ManaColor::G), Pip::Hybrid(ManaColor::B, ManaColor::G)], generic: 5, x_count: 0 })], condition: AltCostCondition::Always })",
        _ => super::build_eternal_e9a_v1::alt_cost_for(name),
    }
}

/// Mandatory additional costs. Bitter Triumph: "As an additional cost to
/// cast this spell, discard a card or pay 3 life." The discard is this
/// cost; paying life is the alternative cost above.
pub(super) fn additional_cost_for(name: &str) -> &'static str {
    match name {
        "Bitter Triumph" => "Some(&[CostComponent::DiscardCards(1)])",
        _ => "None",
    }
}

/// Generic cost reductions. Thought Monitor: "Affinity for artifacts".
pub(super) fn generic_cost_reduction_for(name: &str) -> &'static str {
    match name {
        "Thought Monitor" => "Some(GenericCostReductionDef { generic_per_count: 1, count: DynamicCountDef::ControllerBattlefieldAnyType(&[CardType::Artifact]) })",
        _ => "None",
    }
}

/// Equipment statics. Lich's Relic: "Equipped creature gets +2/+1." Meteor
/// Sword: "Equipped creature gets +3/+3." Pre-War Formalwear: "Equipped
/// creature gets +2/+2 and has vigilance." Lion Sash: "Equipped creature
/// gets +1/+1 for each +1/+1 counter on this Equipment" is an engine hook
/// (`eternal_legacy_e8a_v1::statics::controlled_boost`).
pub(super) fn equipment_for(name: &str) -> &'static str {
    match name {
        "Lich's Relic" => "Some(EquipmentDef { power_delta: 2, toughness_delta: 1, add_subtype: None, controller_turn_keywords: Keywords::NONE, other_turn_keywords: Keywords::NONE, noncreature_spell_damage_to_each_opponent: 0, job_select: false, granted_activated_ability: None, pt_controller_turn_only: false })",
        "Lion Sash" => "Some(EquipmentDef { power_delta: 0, toughness_delta: 0, add_subtype: None, controller_turn_keywords: Keywords::NONE, other_turn_keywords: Keywords::NONE, noncreature_spell_damage_to_each_opponent: 0, job_select: false, granted_activated_ability: None, pt_controller_turn_only: false })",
        "Meteor Sword" => "Some(EquipmentDef { power_delta: 3, toughness_delta: 3, add_subtype: None, controller_turn_keywords: Keywords::NONE, other_turn_keywords: Keywords::NONE, noncreature_spell_damage_to_each_opponent: 0, job_select: false, granted_activated_ability: None, pt_controller_turn_only: false })",
        "Pre-War Formalwear" => "Some(EquipmentDef { power_delta: 2, toughness_delta: 2, add_subtype: None, controller_turn_keywords: Keywords::VIGILANCE, other_turn_keywords: Keywords::VIGILANCE, noncreature_spell_damage_to_each_opponent: 0, job_select: false, granted_activated_ability: None, pt_controller_turn_only: false })",
        _ => super::build_eternal_e9a_v1::equipment_for(name),
    }
}

/// Mystic Sanctuary: "This land enters tapped unless you control three or
/// more other Islands."
pub(super) fn enters_battlefield_tapped_unless_for(name: &str) -> &'static str {
    match name {
        "Mystic Sanctuary" => "Some(EntersBattlefieldTappedUnlessDef { controller_controls_other_subtype: Subtype::Island, minimum_count: 3 })",
        _ => "None",
    }
}

/// Lands whose mana ability the runtime grants: Valgavoth's Lair adds the
/// color chosen as it entered (`eternal_legacy_e8a_v1::statics`).
pub(super) fn static_only_land(name: &str) -> bool {
    name == "Valgavoth's Lair"
}

/// Card-database tokens for the triggered abilities in
/// `eternal_legacy_e8a_v1::triggers_for`, its hooks and its statics.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Ad Nauseam" => "spell:reveal_top_to_hand_lose_life_mana_value:may_repeat",
        "Aether Vial" => {
            "controller_upkeep:may_charge_counter;activated:may_put_hand_creature_mana_value_equals_charge"
        }
        "Back to Basics" => "static:nonbasic_lands_dont_untap_in_untap_step",
        "Bitter Triumph" => {
            "additional_cost:discard_card_or_pay_3_life;spell:destroy_target_creature_or_planeswalker"
        }
        "Cabal Ritual" => "spell:add_bbb:threshold_add_bbbbb",
        "Canoptek Scarab Swarm" => {
            "etb:target_player:exile_graveyard:insect_token_per_artifact_or_land"
        }
        "Chandra, Chill of Compliance" => {
            "plus1:surveil_1:noncreature_nonland_to_hand;plus1:add_u_noncreature_spells_only;minus_x:tap_artifact_or_creature:x_stun;minus6:emblem:cast_spell:draw_1"
        }
        "Cloak and Dagger, Entwined" => {
            "etb:target_opponent_up_to_one_their_creature:reveal_hand:may_exile_nonland_card_or_creature_until_leaves"
        }
        "Dark Matter Manipulator" => "etb:mill:3;static:plus_2_plus_0_per_7_graveyard_cards",
        "Emperor of Bones" => {
            "controller_combat:exile_up_to_one_graveyard_card;activated:adapt_2;plus_one_counters_put_on_self:exiled_creature_to_battlefield_finality_haste:sacrifice_next_end_step"
        }
        "Erode" => {
            "spell:destroy_target_creature_or_planeswalker:its_controller_may_search_basic_land_tapped"
        }
        "Flagstones of Trokair" => {
            "put_into_graveyard_from_battlefield:may_search_plains_battlefield_tapped"
        }
        "Flickerwisp" => "etb:exile_another_target_permanent:return_next_end_step_owner",
        "Harbinger of the Seas" => "static:nonbasic_lands_are_islands",
        "Hogaak, Arisen Necropolis" => {
            "cant_spend_mana:convoke:delve;static:may_cast_from_your_graveyard"
        }
        "Infernal Tutor" => "spell:reveal_hand_card:search_same_name;hellbent:search_any_card",
        "Lich's Relic" => {
            "etb:may_pay_2:reflexive:destroy_up_to_one_opponent_creature_or_planeswalker"
        }
        "Lion Sash" => {
            "activated:exile_graveyard_card:permanent_card_plus_one_counter;static:equipped_plus_1_plus_1_per_counter;reconfigure:2"
        }
        "Loran of the Third Path" => {
            "etb:destroy_up_to_one_artifact_or_enchantment;activated:you_and_target_opponent_draw_1"
        }
        "Meteor Sword" => "etb:destroy_target_permanent",
        "Mystic Sanctuary" => {
            "enters_tapped_unless_three_other_islands;etb_untapped:may_target_instant_sorcery_card_to_library_top"
        }
        "Once Upon a Time" => {
            "alt_cost:first_spell_this_game:free;spell:look_top_5:may_reveal_creature_or_land_to_hand:rest_bottom_random"
        }
        "Overlord of the Balemurk" => {
            "impending:5;etb_or_attacks:mill_4:may_return_non_avatar_creature_or_planeswalker_card;controller_end_step_with_time_counter:remove_time_counter"
        }
        "Pre-War Formalwear" => {
            "etb:return_creature_card_mana_value_at_most_3:attach_self"
        }
        "Rain of Filth" => "spell:controlled_lands_gain_sacrifice_add_b_until_end_of_turn",
        "Recruiter of the Guard" => "etb:may_search_creature_toughness_at_most_2:reveal:hand",
        "Skyclave Apparition" => {
            "etb:exile_up_to_one_opponent_nonland_nontoken_mana_value_at_most_4;leaves:exiled_card_owner_creates_illusion_x"
        }
        "Solitude" => {
            "etb:exile_up_to_one_other_creature:its_controller_gains_life_power;evoke:exile_white_card;etb_if_evoked:sacrifice_source"
        }
        "Stitcher's Supplier" => "etb:mill:3;dies:mill:3",
        "Stoneforge Mystic" => {
            "etb:may_search_equipment:reveal:hand;activated:may_put_equipment_from_hand"
        }
        "Supreme Verdict" => "spell:destroy_all_creatures",
        "Teferi, Time Raveler" => {
            "static:opponents_cast_only_at_sorcery_speed;plus1:sorceries_have_flash_until_your_next_turn;minus3:return_up_to_one_artifact_creature_enchantment:draw_1"
        }
        "Thought Monitor" => "affinity:artifacts;etb:draw:2",
        "Valgavoth's Lair" => "as_enters_choose_color;mana:chosen_color",
        "White Orchid Phantom" => {
            "etb:destroy_up_to_one_nonbasic_land:its_controller_may_search_basic_land_tapped"
        }
        "Wight of the Reliquary" => {
            "static:plus_1_plus_1_per_creature_card_in_graveyard;activated:search_land_battlefield_tapped"
        }
        "Wishclaw Talisman" => {
            "enters_with_wish_counters:3;activated:your_turn:remove_wish_counter:search_any_card:opponent_gains_control"
        }
        "Yorion, Sky Nomad" => {
            "companion:starting_deck_at_least_80_cards:special_action_pay_3_to_hand_as_sorcery_from_exile;etb:exile_any_number_other_own_nonland_permanents:return_next_end_step"
        }
        "Illusion Token" => "characteristic:power_toughness_equal_exiled_mana_value",
        _ => super::build_eternal_e9a_v1::trigger_recipe_for(name),
    }
}
