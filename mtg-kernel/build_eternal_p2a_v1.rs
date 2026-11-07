//! Card-database recipes for the eternal catalog's Premodern wave P2a: The
//! Rock, Mono-Black Control, BW Control, Stasis and Other Control.
//! `build_eternal_e9a_v1` and the other eternal tables fall through to these;
//! runtime programs live in `src/eternal_premodern_p2a_v1.rs`. Every row is
//! keyed by a name that occurs only in the eternal registry.

use super::{
    AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, LibrarySearchFilterRecipe,
    PermanentFilterRecipe, Special,
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

/// An activated ability from another zone.
const fn from_zone(
    activation_zone: &'static str,
    cost: &'static [AbilityCostRecipe],
    effect: AbilityEffectRecipe,
) -> ActivatedAbilityRecipe {
    ActivatedAbilityRecipe {
        activation_zone,
        ..instant(cost, effect, "None")
    }
}

/// A program of `eternal_premodern_p2a_v1`.
const fn program(path: &'static str) -> AbilityEffectRecipe {
    AbilityEffectRecipe::StandardProgram(path)
}

/// "Cycling {cost}" (702.29): discard this card from hand, draw a card.
/// Krosan Tusker's "When you cycle" trigger is queued as the cost is paid
/// (`eternal_premodern_p2a_v1::statics::activation_paid`).
const fn cycling(cost: &'static [AbilityCostRecipe]) -> ActivatedAbilityRecipe {
    from_zone("Hand", cost, AbilityEffectRecipe::DrawCards(1))
}

/// "Plainscycling {cost}" (702.29e): discard this card from hand to search
/// for a Plains card.
const fn plainscycling(cost: &'static [AbilityCostRecipe]) -> ActivatedAbilityRecipe {
    from_zone(
        "Hand",
        cost,
        AbilityEffectRecipe::SearchLibraryToHand {
            filter: LibrarySearchFilterRecipe::LandWithSubtype("Plains"),
            min_targets: 0,
            max_targets: 1,
            reveal_selected: true,
            shuffle: true,
        },
    )
}

/// Pernicious Deed: "{X}, Sacrifice this enchantment: Destroy each artifact,
/// creature, and enchantment with mana value X or less." The engine's
/// announced X belongs to a few Standard cards, so each X from 0 to 8 (the
/// largest mana value in the Premodern pool) is its own ability, and the
/// controller picks one.
macro_rules! deed {
    ($cost:literal, $effect:literal) => {
        instant(&[ManaCost($cost), SacrificeSelf], program($effect), "None")
    };
}

const PERNICIOUS_DEED: [ActivatedAbilityRecipe; 9] = [
    deed!("{0}", "crate::eternal_premodern_p2a_v1::pernicious_deed_0"),
    deed!("{1}", "crate::eternal_premodern_p2a_v1::pernicious_deed_1"),
    deed!("{2}", "crate::eternal_premodern_p2a_v1::pernicious_deed_2"),
    deed!("{3}", "crate::eternal_premodern_p2a_v1::pernicious_deed_3"),
    deed!("{4}", "crate::eternal_premodern_p2a_v1::pernicious_deed_4"),
    deed!("{5}", "crate::eternal_premodern_p2a_v1::pernicious_deed_5"),
    deed!("{6}", "crate::eternal_premodern_p2a_v1::pernicious_deed_6"),
    deed!("{7}", "crate::eternal_premodern_p2a_v1::pernicious_deed_7"),
    deed!("{8}", "crate::eternal_premodern_p2a_v1::pernicious_deed_8"),
];

/// Ravenous Baloth: "Sacrifice a Beast: You gain 4 life."
const RAVENOUS_BALOTH: [ActivatedAbilityRecipe; 1] = [instant(
    &[AbilityCostRecipe::SacrificeControlled {
        count: 1,
        filter: PermanentFilterRecipe::Beast,
    }],
    AbilityEffectRecipe::GainLife(4),
    "None",
)];

/// High Market: "{T}, Sacrifice a creature: You gain 1 life." Its
/// "{T}: Add {C}." is the ordinary mana ability.
const HIGH_MARKET: [ActivatedAbilityRecipe; 1] = [instant(
    &[
        Tap,
        AbilityCostRecipe::SacrificeControlled {
            count: 1,
            filter: PermanentFilterRecipe::Creature,
        },
    ],
    AbilityEffectRecipe::GainLife(1),
    "None",
)];

/// Nevinyrral's Disk: "{1}, {T}: Destroy all artifacts, creatures, and
/// enchantments." It enters tapped by its registry mechanics.
const NEVINYRRALS_DISK: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{1}"), Tap],
    program("crate::eternal_premodern_p2a_v1::nevinyrrals_disk"),
    "None",
)];

/// Zombie Infestation: "Discard two cards: Create a 2/2 black Zombie
/// creature token."
const ZOMBIE_INFESTATION: [ActivatedAbilityRecipe; 1] = [instant(
    &[AbilityCostRecipe::DiscardCards(2)],
    AbilityEffectRecipe::CreateToken("Zombie Token"),
    "None",
)];

/// Eternal Dragon: "{3}{W}{W}: Return this card from your graveyard to your
/// hand. Activate only during your upkeep." (the upkeep restriction is
/// `eternal_premodern_p2a_v1::statics::activation_allowed`) and
/// "Plainscycling {2}".
const ETERNAL_DRAGON: [ActivatedAbilityRecipe; 2] = [
    from_zone(
        "Graveyard",
        &[ManaCost("{3}{W}{W}")],
        program("crate::eternal_premodern_p2a_v1::eternal_dragon_return"),
    ),
    plainscycling(&[ManaCost("{2}"), DiscardSelf]),
];

/// Krosan Tusker: "Cycling {2}{G}".
const KROSAN_TUSKER: [ActivatedAbilityRecipe; 1] = [cycling(&[ManaCost("{2}{G}"), DiscardSelf])];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Eternal Dragon" => &ETERNAL_DRAGON,
        "High Market" => &HIGH_MARKET,
        "Krosan Tusker" => &KROSAN_TUSKER,
        "Nevinyrral's Disk" => &NEVINYRRALS_DISK,
        "Pernicious Deed" => &PERNICIOUS_DEED,
        "Ravenous Baloth" => &RAVENOUS_BALOTH,
        "Zombie Infestation" => &ZOMBIE_INFESTATION,
        _ => super::build_eternal_p1b_v1::activated_ability_recipes_for(name),
    }
}

/// A spell whose program is a runtime function of `eternal_premodern_p2a_v1`.
const fn spell(target_spec: &'static str, program: &'static str) -> Special {
    Special::StandardProgram {
        target_spec,
        program,
    }
}

/// Spell programs.
pub(super) fn special_for(name: &str) -> Option<Special> {
    Some(match name {
        // Dromar's Charm: "Choose one -- You gain 5 life; or counter target
        // spell; or target creature gets -2/-2 until end of turn." Modes two
        // and three are `mode2_for` and `mode3_for` below.
        "Dromar's Charm" => spell(
            "TargetSpec::None",
            "crate::eternal_premodern_p2a_v1::dromars_charm_life",
        ),
        // Demonic Tutor's program under another name and cost.
        "Diabolic Tutor" => spell("TargetSpec::None", "crate::eternal_lurrus_v1::demonic_tutor"),
        "Gaea's Blessing" => spell(
            "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::PlayerThenUpToThreeOfTheirGraveyardCardsV1)",
            "crate::eternal_premodern_p2a_v1::gaeas_blessing",
        ),
        "Gerrard's Verdict" => spell(
            "TargetSpec::AnyPlayer",
            "crate::eternal_premodern_p2a_v1::gerrards_verdict",
        ),
        "Innocent Blood" => spell(
            "TargetSpec::None",
            "crate::eternal_premodern_p2a_v1::innocent_blood",
        ),
        "Mana Short" => spell(
            "TargetSpec::AnyPlayer",
            "crate::eternal_premodern_p2a_v1::mana_short",
        ),
        "Upheaval" => spell("TargetSpec::None", "crate::eternal_premodern_p2a_v1::upheaval"),
        "Vindicate" => spell(
            "TargetSpec::AnyPermanent",
            "crate::eternal_premodern_p2a_v1::vindicate",
        ),
        _ => return super::build_eternal_p1b_v1::special_for(name),
    })
}

/// Printed evergreen keywords, in printed order.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Eternal Dragon" => &["Keywords::FLYING"],
        _ => super::build_eternal_p1b_v1::keywords_for(name),
    }
}

/// Bane of the Living: "Morph {X}{B}{B}" (702.37): cast face down as a 2/2
/// creature for {3}, through the face-down cast that disguise uses, without
/// disguise's ward; turning it face up is
/// `eternal_premodern_p2a_v1::statics::morph_face_up_cost`.
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    match name {
        "Bane of the Living" => "Some(AltCostDef { components: &[CostComponent::Mana(Cost { pips: &[], generic: 3, x_count: 0 })], condition: AltCostCondition::DisguiseFromHand })",
        _ => super::build_eternal_p1a_v1::alt_cost_for(name),
    }
}

/// Dromar's Charm's second mode: "Counter target spell."
pub(super) fn mode2_for(name: &str) -> &'static str {
    match name {
        "Dromar's Charm" => "Some(ModeDef { target_spec: TargetSpec::AnySpellOnStack, effect: crate::eternal_premodern_p2a_v1::dromars_charm_counter })",
        _ => super::build_eternal_p1a_v1::mode2_for(name),
    }
}

/// Dromar's Charm's third mode: "Target creature gets -2/-2 until end of
/// turn."
pub(super) fn mode3_for(name: &str) -> &'static str {
    match name {
        "Dromar's Charm" => "Some(ModeDef { target_spec: TargetSpec::Creature, effect: crate::eternal_premodern_p2a_v1::dromars_charm_shrink })",
        _ => super::build_eternal_p1a_v1::mode3_for(name),
    }
}

/// Additional printed mana abilities. Tainted Wood: "{T}: Add {B} or {G}.
/// Activate only if you control a Swamp." and Tainted Field's {W} or {B}
/// (the condition is `build_support/standard_lands_v1.rs`). Phyrexian Tower:
/// "{T}, Sacrifice a creature: Add {B}{B}." (a doubled single color).
pub(super) fn additional_mana_abilities_for(name: &str) -> &'static str {
    match name {
        "Tainted Wood" => "&[AdditionalManaAbilityDef { colors: &[ManaColor::B, ManaColor::G], mana_cost: Cost { pips: &[], generic: 0, x_count: 0 }, ability: ManaAbilityDef { cost: ManaAbilityCostDef::TapSelf, amount: ManaAbilityAmountDef::Fixed(1), controller_damage: 0, max_activations_per_turn: None } }]",
        "Tainted Field" => "&[AdditionalManaAbilityDef { colors: &[ManaColor::W, ManaColor::B], mana_cost: Cost { pips: &[], generic: 0, x_count: 0 }, ability: ManaAbilityDef { cost: ManaAbilityCostDef::TapSelf, amount: ManaAbilityAmountDef::Fixed(1), controller_damage: 0, max_activations_per_turn: None } }]",
        "Phyrexian Tower" => "&[AdditionalManaAbilityDef { colors: &[ManaColor::B], mana_cost: Cost { pips: &[], generic: 0, x_count: 0 }, ability: ManaAbilityDef { cost: ManaAbilityCostDef::TapSelfSacrificeControlledCreatureV1, amount: ManaAbilityAmountDef::Fixed(2), controller_damage: 0, max_activations_per_turn: None } }]",
        _ => super::build_eternal_p1b_v1::additional_mana_abilities_for(name),
    }
}

/// Fixed colors of the primary printed mana ability, where they differ from
/// `produces_mana`: the Tainted lands' free ability adds {C}.
pub(super) fn primary_mana_ability_colors(name: &str) -> Option<Vec<&'static str>> {
    match name {
        "Tainted Wood" | "Tainted Field" => Some(vec!["C"]),
        _ => super::build_eternal_p1b_v1::primary_mana_ability_colors(name),
    }
}

/// Card-database tokens for the triggered abilities, statics and hooks in
/// `eternal_premodern_p2a_v1`.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Academy Rector" => "dies:may_exile_self:search_enchantment_to_battlefield_shuffle",
        "Bane of the Living" => {
            "morph:xbb:face_down_2_2_no_ward;turned_face_up:all_creatures_minus_x_minus_x"
        }
        "Black Vise" => "enters:choose_opponent;chosen_upkeep:damage_hand_minus_4",
        "Contamination" => {
            "controller_upkeep:sacrifice_unless_sacrifice_creature;static:lands_tapped_for_mana_produce_b"
        }
        "Dromar's Charm" => "modal:gain_5|counter_target_spell|target_creature_minus_2_minus_2",
        "Eternal Dragon" => "activated:graveyard_upkeep_only:return_self_to_hand;plainscycling:2",
        "Forsaken City" => {
            "doesnt_untap;controller_upkeep:may_exile_card_from_hand:untap_self;mana:any_color"
        }
        "Future Sight" => "static:top_card_revealed;static:may_play_top_card",
        "Gaea's Blessing" => {
            "spell:target_player_shuffles_up_to_three_target_graveyard_cards:draw_1;milled:shuffle_graveyard_into_library"
        }
        "Genesis" => {
            "graveyard_upkeep:may_pay_2g:return_target_creature_card_from_graveyard_to_hand"
        }
        "Gerrard's Verdict" => "spell:target_player_discards_2:gain_3_per_land_discarded",
        "High Market" => "mana:c;activated:tap_sacrifice_creature:gain_1",
        "Krosan Tusker" => "cycling:2g;cycled:may_search_basic_land_to_hand",
        "Mana Short" => "spell:tap_target_players_lands:empty_their_mana_pool",
        "Nether Spirit" => "graveyard_upkeep:if_only_creature_card:may_return_self_to_battlefield",
        "Nevinyrral's Disk" => {
            "enters_tapped;activated:1_tap:destroy_all_artifacts_creatures_enchantments"
        }
        "Pernicious Deed" => {
            "activated:x_sacrifice_self:destroy_each_artifact_creature_enchantment_mana_value_at_most_x"
        }
        "Phyrexian Tower" => "mana:c;mana:tap_sacrifice_creature:bb",
        "Ravenous Baloth" => "activated:sacrifice_beast:gain_4",
        "Ravenous Rats" => "etb:target_opponent_discards_1",
        "Root Maze" => "static:artifacts_and_lands_enter_tapped",
        "Stasis" => "static:skip_untap_steps;controller_upkeep:sacrifice_unless_pay_u",
        "Tainted Field" => "mana:c;mana:w_or_b_if_controls_swamp",
        "Tainted Wood" => "mana:c;mana:b_or_g_if_controls_swamp",
        "Upheaval" => "spell:return_all_permanents_to_owners_hands",
        "Veteran Explorer" => "dies:each_player_may_search_two_basic_lands_to_battlefield",
        "Vindicate" => "spell:destroy_target_permanent",
        "Wall of Blossoms" => "defender;etb:draw_1",
        "Zombie Infestation" => "activated:discard_2:zombie_2_2",
        _ => super::build_eternal_p1b_v1::trigger_recipe_for(name),
    }
}
