//! Card-database recipes for the eternal catalog's Vintage Cube wave C2a
//! (lands with an entry choice or a typed tap). Wave C1c's tables fall through to these, and
//! these fall through to wave C2b's
//! (`build_eternal_c2b_v1.rs`). Runtime programs live in
//! `src/eternal_cube_c2a_v1.rs`. Every row is keyed by a name that occurs
//! only in the eternal registry.

use super::{ActivatedAbilityRecipe, Special};

/// No C2a card has an activated ability yet.
pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    super::build_eternal_c2b_v1::activated_ability_recipes_for(name)
}

/// No C2a spell program yet.
pub(super) fn special_for(name: &str) -> Option<Special> {
    super::build_eternal_c2b_v1::special_for(name)
}

/// No C2a card has kicker yet.
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    super::build_eternal_c2b_v1::kicker_cost_for(name)
}

/// No C2a card is an Aura or an Equipment yet.
pub(super) fn attachment_for(name: &str) -> &'static str {
    super::build_eternal_c2b_v1::attachment_for(name)
}

/// No C2a card has flashback yet.
pub(super) fn flashback_for(name: &str) -> String {
    super::build_eternal_c2b_v1::flashback_for(name)
}

/// No C2a card has an alternative cost yet.
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    super::build_eternal_c2b_v1::alt_cost_for(name)
}

/// No C2a card has a printed evergreen keyword yet.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    super::build_eternal_c2b_v1::keywords_for(name)
}

/// Card-database tokens for this wave's abilities. Every C2a land is data
/// driven: the shocklands and Multiversal Passage ask their as-enters life
/// payment at run time (`eternal_cube_c2a_v1`), the Triomes' cycling, the
/// Verges' conditional mana and Botanical Sanctum's entry condition come from
/// `build_support/standard_lands_v1.rs`, and Commercial District's surveil is
/// `src/trigger.rs`' shared land trigger.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Blood Crypt" | "Breeding Pool" | "Godless Shrine" | "Hallowed Fountain"
        | "Overgrown Tomb" | "Sacred Foundry" | "Steam Vents" | "Stomping Ground"
        | "Temple Garden" | "Watery Grave" => {
            "as_enters:may_pay_2_life_else_tapped;mana:either_basic_land_type_color"
        }
        "Multiversal Passage" => {
            "as_enters:choose_basic_land_type;as_enters:may_pay_2_life_else_tapped;static:is_chosen_basic_land_type"
        }
        "Indatha Triome" | "Ketria Triome" | "Raugrin Triome" | "Savai Triome"
        | "Zagoth Triome" | "Raffine's Tower" | "Xander's Lounge" => {
            "enters_tapped;mana:three_basic_land_type_colors;cycling:3"
        }
        "Blazemire Verge" => "mana:b;mana:r_if_controls_swamp_or_mountain",
        "Bleachbone Verge" => "mana:b;mana:w_if_controls_plains_or_swamp",
        "Botanical Sanctum" => "enters_tapped_unless_two_or_fewer_other_lands;mana:g_or_u",
        "Commercial District" => "enters_tapped;etb:surveil:1",
        _ => super::build_eternal_c2b_v1::trigger_recipe_for(name),
    }
}
