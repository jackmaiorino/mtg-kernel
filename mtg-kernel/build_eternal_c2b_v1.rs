//! Card-database recipes for the eternal catalog's Vintage Cube wave C2b
//! (utility lands). Wave C2a's tables fall through to these, and
//! these fall through to wave C2c's
//! (`build_eternal_c2c_v1.rs`). Runtime programs live in
//! `src/eternal_cube_c2b_v1.rs`. Every row is keyed by a name that occurs
//! only in the eternal registry.

use super::{AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special};
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

/// A program of `eternal_cube_c2b_v1`.
const fn program(path: &'static str) -> AbilityEffectRecipe {
    AbilityEffectRecipe::StandardProgram(path)
}

/// Sunbaked Canyon and Waterlogged Grove: "{1}, {T}, Sacrifice this land:
/// Draw a card." Their "{T}, Pay 1 life: Add {R} or {W}" (or {G} or {U}) is an
/// additional mana ability (Horizon Canopy's shape).
const SACRIFICE_DRAW_LAND: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{1}"), Tap, SacrificeSelf],
    AbilityEffectRecipe::DrawCards(1),
    "None",
)];

/// Library of Alexandria: "{T}: Draw a card. Activate only if you have exactly
/// seven cards in hand." The hand-size condition is
/// `eternal_cube_c2b_v1::activation_allowed`; "{T}: Add {C}" is the mana
/// ability.
const LIBRARY_OF_ALEXANDRIA: [ActivatedAbilityRecipe; 1] =
    [instant(&[Tap], AbilityEffectRecipe::DrawCards(1), "None")];

/// Celestial Colonnade: "{3}{W}{U}: Until end of turn, this land becomes a
/// 4/4 white and blue Elemental creature with flying and vigilance. It's still
/// a land." The animation is `build_support/standard_lands_v1.rs`'s.
const CELESTIAL_COLONNADE: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{3}{W}{U}")],
    AbilityEffectRecipe::AnimateSource,
    "None",
)];

/// Creeping Tar Pit: "{1}{U}{B}: Until end of turn, this land becomes a 3/2
/// blue and black Elemental creature. It's still a land. It can't be blocked
/// this turn."
const CREEPING_TAR_PIT: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{1}{U}{B}")],
    AbilityEffectRecipe::AnimateSource,
    "None",
)];

/// Fabled Passage: "{T}, Sacrifice this land: Search your library for a basic
/// land card, put it onto the battlefield tapped, then shuffle. Then if you
/// control four or more lands, untap that land."
const FABLED_PASSAGE: [ActivatedAbilityRecipe; 1] = [instant(
    &[Tap, SacrificeSelf],
    AbilityEffectRecipe::Program {
        name: "fetch_basic_land_tapped_untap_c2b",
        effect: "EffectOp::SearchLibraryCardsToDestination { player: PlayerRef::Controller, filter: crate::effect::LibraryCardFilter::BasicLand, max_targets: 1, destination: crate::effect::LibrarySearchDestinationV1::BattlefieldTappedUntapWithFourLandsV1 }",
    },
    "None",
)];

/// Abandoned Air Temple: "{3}{W}, {T}: Put a +1/+1 counter on each creature
/// you control."
const ABANDONED_AIR_TEMPLE: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{3}{W}"), Tap],
    program("crate::eternal_cube_c2b_v1::temple_counters"),
    "None",
)];

/// Shelldock Isle: "{U}, {T}: You may play the exiled card without paying its
/// mana cost if a library has twenty or fewer cards in it."
const SHELLDOCK_ISLE: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{U}"), Tap],
    program("crate::eternal_cube_c2b_v1::shelldock_play"),
    "None",
)];

/// Activated abilities of this wave's cards (index = ability index).
pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Sunbaked Canyon" | "Waterlogged Grove" => &SACRIFICE_DRAW_LAND,
        "Library of Alexandria" => &LIBRARY_OF_ALEXANDRIA,
        "Celestial Colonnade" => &CELESTIAL_COLONNADE,
        "Creeping Tar Pit" => &CREEPING_TAR_PIT,
        "Fabled Passage" => &FABLED_PASSAGE,
        "Abandoned Air Temple" => &ABANDONED_AIR_TEMPLE,
        "Shelldock Isle" => &SHELLDOCK_ISLE,
        _ => super::build_eternal_c2c_v1::activated_ability_recipes_for(name),
    }
}

/// No C2b spell program: every card of this wave is a land.
pub(super) fn special_for(name: &str) -> Option<Special> {
    super::build_eternal_c2c_v1::special_for(name)
}

/// No C2b card has kicker.
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    super::build_eternal_c2c_v1::kicker_cost_for(name)
}

/// No C2b card is an Aura or an Equipment.
pub(super) fn attachment_for(name: &str) -> &'static str {
    super::build_eternal_c2c_v1::attachment_for(name)
}

/// No C2b card has flashback.
pub(super) fn flashback_for(name: &str) -> String {
    super::build_eternal_c2c_v1::flashback_for(name)
}

/// No C2b card has an alternative cost.
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    super::build_eternal_c2c_v1::alt_cost_for(name)
}

/// No C2b card has a printed evergreen keyword (the creature lands gain
/// theirs from their animation).
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    super::build_eternal_c2c_v1::keywords_for(name)
}

/// Additional printed mana abilities (the tail of the
/// `additional_mana_abilities_for` chain calls this). Sunbaked Canyon:
/// "{T}, Pay 1 life: Add {R} or {W}." Waterlogged Grove: "{T}, Pay 1 life: Add
/// {G} or {U}." Mana Confluence: "{T}, Pay 1 life: Add one mana of any color."
pub(super) fn additional_mana_abilities_for(name: &str) -> &'static str {
    match name {
        "Sunbaked Canyon" => "&[AdditionalManaAbilityDef { colors: &[ManaColor::R, ManaColor::W], mana_cost: Cost { pips: &[], generic: 0, x_count: 0 }, ability: ManaAbilityDef { cost: ManaAbilityCostDef::TapSelfPayLife(1), amount: ManaAbilityAmountDef::Fixed(1), controller_damage: 0, max_activations_per_turn: None } }]",
        "Waterlogged Grove" => "&[AdditionalManaAbilityDef { colors: &[ManaColor::G, ManaColor::U], mana_cost: Cost { pips: &[], generic: 0, x_count: 0 }, ability: ManaAbilityDef { cost: ManaAbilityCostDef::TapSelfPayLife(1), amount: ManaAbilityAmountDef::Fixed(1), controller_damage: 0, max_activations_per_turn: None } }]",
        "Mana Confluence" => "&[AdditionalManaAbilityDef { colors: &[ManaColor::W, ManaColor::U, ManaColor::B, ManaColor::R, ManaColor::G], mana_cost: Cost { pips: &[], generic: 0, x_count: 0 }, ability: ManaAbilityDef { cost: ManaAbilityCostDef::TapSelfPayLife(1), amount: ManaAbilityAmountDef::Fixed(1), controller_damage: 0, max_activations_per_turn: None } }]",
        _ => "&[]",
    }
}

/// Lands whose only mana ability is an additional one and that have no
/// activated ability (the tail of the `static_only_land` chain calls this):
/// Mana Confluence.
pub(super) fn static_only_land(name: &str) -> bool {
    name == "Mana Confluence"
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Sunbaked Canyon" => "mana:r_or_w_tap_pay_1_life;activated:1_tap_sacrifice_self:draw_1",
        "Waterlogged Grove" => "mana:g_or_u_tap_pay_1_life;activated:1_tap_sacrifice_self:draw_1",
        "Mana Confluence" => "mana:any_color_tap_pay_1_life",
        "Library of Alexandria" => {
            "mana:c;activated:tap:draw_1:only_if_exactly_seven_cards_in_hand"
        }
        "Celestial Colonnade" => {
            "enters_tapped;mana:w_or_u;activated:animate_4_4_white_blue_elemental_flying_vigilance"
        }
        "Creeping Tar Pit" => {
            "enters_tapped;mana:u_or_b;activated:animate_3_2_blue_black_elemental_cant_be_blocked"
        }
        "Fabled Passage" => {
            "activated:tap_sacrifice_self:search_basic_land_tapped:shuffle:untap_it_if_four_or_more_lands"
        }
        "Abandoned Air Temple" => {
            "enters_tapped_unless_controls_basic_land;mana:w;activated:3w_tap:plus_one_counter_on_each_creature_you_control"
        }
        "Shelldock Isle" => {
            "enters_tapped;etb:hideaway_4;mana:u;activated:u_tap:may_play_exiled_free_if_a_library_has_20_or_fewer_cards"
        }
        _ => super::build_eternal_c2c_v1::trigger_recipe_for(name),
    }
}
