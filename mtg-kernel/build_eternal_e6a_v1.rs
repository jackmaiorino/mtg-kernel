//! Card-database recipes for the eternal catalog's Pauper Affinity, Tron,
//! Ephemerate and Faeries lists (wave E6a). `build_eternal_e5b_v1` and the
//! other eternal tables fall through to these; runtime programs live in
//! `src/eternal_pauper_e6a_v1.rs`. Every row is keyed by a name that occurs
//! only in the eternal registry.

use super::{
    AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special, StackSpellFilter,
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

/// A cycling ability (702.29): activated from hand, discarding the card as
/// part of its cost, to draw a card.
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

/// Remote Isle: "Cycling {2}".
const REMOTE_ISLE: [ActivatedAbilityRecipe; 1] = [cycling(&[ManaCost("{2}"), DiscardSelf])];

/// Shattered Landscape: "{T}, Sacrifice this land: Search your library for
/// a basic Mountain, Plains, or Swamp card, put it onto the battlefield
/// tapped, then shuffle." and "Cycling {R}{W}{B}". Its `{T}: Add {C}` is
/// the ordinary mana ability.
const SHATTERED_LANDSCAPE: [ActivatedAbilityRecipe; 2] = [
    instant(
        &[Tap, SacrificeSelf],
        AbilityEffectRecipe::SearchLibraryToBattlefieldTapped {
            filter: super::LibrarySearchFilterRecipe::BasicLandWithAnySubtype([
                "Mountain", "Plains", "Swamp",
            ]),
        },
        "None",
    ),
    cycling(&[ManaCost("{R}{W}{B}"), DiscardSelf]),
];

/// Bramble Wurm: "{2}{G}, Exile this card from your graveyard: You gain 5
/// life."
const BRAMBLE_WURM: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[ManaCost("{2}{G}"), AbilityCostRecipe::ExileSelf],
    effect: AbilityEffectRecipe::GainLife(5),
    activation_zone: "Graveyard",
    sorcery_speed_only: false,
    target_spec: "None",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

/// Gearseeker Serpent: "{5}{U}: This creature can't be blocked this turn."
const GEARSEEKER_SERPENT: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{5}{U}")],
    AbilityEffectRecipe::StandardProgram("crate::eternal_pauper_e6a_v1::gearseeker_unblockable"),
    "None",
)];

/// Grim Bauble: "{2}{B}, {T}, Sacrifice this artifact: Surveil 2."
const GRIM_BAUBLE: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{2}{B}"), Tap, SacrificeSelf],
    AbilityEffectRecipe::Surveil(2),
    "None",
)];

/// Omen of the Dead: "{2}{B}, Sacrifice this enchantment: Scry 2."
const OMEN_OF_THE_DEAD: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{2}{B}"), SacrificeSelf],
    AbilityEffectRecipe::Program {
        name: "omen_of_the_dead_scry_2",
        effect: "EffectOp::Scry { player: PlayerRef::Controller, count: 2 }",
    },
    "None",
)];

/// Sewer-veillance Cam: "{3}{U}, Sacrifice this artifact: Draw two cards."
const SEWER_VEILLANCE_CAM: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{3}{U}"), SacrificeSelf],
    AbilityEffectRecipe::DrawCards(2),
    "None",
)];

/// Tithing Blade: "Craft with creature {4}{B}" (702.167a): exile this and a
/// creature you control or a creature card from your graveyard, then return
/// this card transformed. Craft only as a sorcery. The engine takes the
/// material's card type from `eternal_pauper_e6a_v1::craft_material_type`.
const TITHING_BLADE: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[
        ManaCost("{4}{B}"),
        AbilityCostRecipe::ExileSelf,
        AbilityCostRecipe::ExileCraftArtifactMaterial,
    ],
    effect: AbilityEffectRecipe::StandardProgram(
        "crate::standard_cards_v1::craft_return_transformed",
    ),
    activation_zone: "Battlefield",
    sorcery_speed_only: true,
    target_spec: "None",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

/// Behold the Multiverse: "Foretell {1}{U} (During your turn, you may pay
/// {2} and exile this card from your hand face down. Cast it on a later turn
/// for its foretell cost.)" Foretelling is a special action (702.143a): the
/// engine performs it from the hand without the stack
/// (`eternal_pauper_e6a_v1::foretell`), offered as the card's ability 0.
const BEHOLD_THE_MULTIVERSE: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[ManaCost("{2}")],
    effect: AbilityEffectRecipe::StandardProgram("crate::eternal_pauper_e6a_v1::foretell_action"),
    activation_zone: "Hand",
    sorcery_speed_only: false,
    target_spec: "None",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Remote Isle" => &REMOTE_ISLE,
        "Shattered Landscape" => &SHATTERED_LANDSCAPE,
        "Bramble Wurm" => &BRAMBLE_WURM,
        "Gearseeker Serpent" => &GEARSEEKER_SERPENT,
        "Grim Bauble" => &GRIM_BAUBLE,
        "Omen of the Dead" => &OMEN_OF_THE_DEAD,
        "Sewer-veillance Cam" => &SEWER_VEILLANCE_CAM,
        "Tithing Blade" => &TITHING_BLADE,
        "Behold the Multiverse" => &BEHOLD_THE_MULTIVERSE,
        _ => &[],
    }
}

/// Activated abilities printed on one face only: Tithing Blade's craft is
/// its front face's.
pub(super) fn activated_ability_face_for(name: &str, _index: usize) -> Option<u8> {
    (name == "Tithing Blade").then_some(0)
}

/// A spell whose program is a runtime function of `eternal_pauper_e6a_v1`.
const fn program(target_spec: &'static str, program: &'static str) -> Special {
    Special::StandardProgram {
        target_spec,
        program,
    }
}

/// Spell programs.
pub(super) fn special_for(name: &str) -> Option<Special> {
    Some(match name {
        // Its blue-permanent sacrifice is `additional_cost_for` below.
        "Abjure" => Special::CounterTarget(StackSpellFilter::Any),
        // Foretell is its hand ability and `alt_cost_for` below.
        "Behold the Multiverse" => Special::ScryThenDraw { scry: 2, draw: 2 },
        // Its land sacrifice is `additional_cost_for` below.
        "Crop Rotation" => program(
            "TargetSpec::None",
            "crate::eternal_pauper_e6a_v1::crop_rotation",
        ),
        // The Aura enters attached to its target; its static is
        // `attachment_for` below and its trigger an engine hook.
        "Cryoshatter" => Special::BindTheMonster,
        "Dispatch" => program(
            "TargetSpec::Creature",
            "crate::eternal_pauper_e6a_v1::dispatch",
        ),
        // Rebound is part of its program and an engine hook.
        "Ephemerate" => program(
            "TargetSpec::ControlledCreature",
            "crate::eternal_pauper_e6a_v1::ephemerate",
        ),
        "Ethereal Haze" => program(
            "TargetSpec::None",
            "crate::eternal_pauper_e6a_v1::ethereal_haze",
        ),
        "Ghostly Flicker" => program(
            "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::TwoControlledArtifactsCreaturesOrLandsV1)",
            "crate::eternal_pauper_e6a_v1::ghostly_flicker",
        ),
        "Hard Evidence" => program(
            "TargetSpec::None",
            "crate::eternal_pauper_e6a_v1::hard_evidence",
        ),
        // Flashback is `flashback_for` below.
        "Moment's Peace" => program(
            "TargetSpec::None",
            "crate::eternal_pauper_e6a_v1::moments_peace",
        ),
        "Mystical Teachings" => program(
            "TargetSpec::None",
            "crate::eternal_pauper_e6a_v1::mystical_teachings",
        ),
        // Kicker is `kicker_cost_for` below.
        "Prohibit" => program(
            "TargetSpec::AnySpellOnStack",
            "crate::eternal_pauper_e6a_v1::prohibit",
        ),
        // Replicate is `eternal_pauper_e6a_v1::replicate_cost`.
        "Stream of Thought" => program(
            "TargetSpec::AnyPlayer",
            "crate::eternal_pauper_e6a_v1::stream_of_thought",
        ),
        "Tangle" => program("TargetSpec::None", "crate::eternal_pauper_e6a_v1::tangle"),
        "Union of the Third Path" => program(
            "TargetSpec::None",
            "crate::eternal_pauper_e6a_v1::union_of_the_third_path",
        ),
        _ => return None,
    })
}

/// Printed evergreen keywords, in printed order.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Bramble Wurm" => &["Keywords::REACH", "Keywords::TRAMPLE"],
        "Brinebarrow Intruder" => &["Keywords::FLASH"],
        "Kor Skyfisher" => &["Keywords::FLYING"],
        "Mulldrifter" => &["Keywords::FLYING"],
        "Omen of the Dead" => &["Keywords::FLASH"],
        "Sewer-veillance Cam" => &["Keywords::FLASH"],
        _ => &[],
    }
}

/// Additional mana abilities beyond the primary one.
pub(super) fn additional_mana_abilities_for(name: &str) -> &'static str {
    match name {
        // Prophetic Prism: "{1}, {T}: Add one mana of any color."
        "Prophetic Prism" => "&[AdditionalManaAbilityDef { colors: &[ManaColor::W, ManaColor::U, ManaColor::B, ManaColor::R, ManaColor::G], mana_cost: Cost { pips: &[], generic: 1, x_count: 0 }, ability: ManaAbilityDef { cost: ManaAbilityCostDef::TapSelf, amount: ManaAbilityAmountDef::Fixed(1), controller_damage: 0, max_activations_per_turn: None } }]",
        // Energy Refractor: "{2}: Add one mana of any color." No tap.
        "Energy Refractor" => "&[AdditionalManaAbilityDef { colors: &[ManaColor::W, ManaColor::U, ManaColor::B, ManaColor::R, ManaColor::G], mana_cost: Cost { pips: &[], generic: 2, x_count: 0 }, ability: ManaAbilityDef { cost: ManaAbilityCostDef::None, amount: ManaAbilityAmountDef::Fixed(1), controller_damage: 0, max_activations_per_turn: None } }]",
        _ => "&[]",
    }
}

/// Alternative costs. Mulldrifter: "Evoke {2}{U}" (its sacrifice is an
/// entry trigger). Behold the Multiverse: "Foretell {1}{U}", payable only
/// for the card foretold from exile.
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    match name {
        "Mulldrifter" => "Some(AltCostDef { components: &[CostComponent::Mana(Cost { pips: &[Pip::Colored(ManaColor::U)], generic: 2, x_count: 0 })], condition: AltCostCondition::Always })",
        "Behold the Multiverse" => "Some(AltCostDef { components: &[CostComponent::Mana(Cost { pips: &[Pip::Colored(ManaColor::U)], generic: 1, x_count: 0 })], condition: AltCostCondition::ForetoldV1 })",
        _ => "None",
    }
}

/// Mandatory additional costs. Abjure: "sacrifice a blue permanent." Crop
/// Rotation: "sacrifice a land."
pub(super) fn additional_cost_for(name: &str) -> &'static str {
    match name {
        "Abjure" => {
            "Some(&[CostComponent::SacrificeControlled { count: 1, filter: PermanentFilter::BlueV1 }])"
        }
        "Crop Rotation" => {
            "Some(&[CostComponent::SacrificeControlled { count: 1, filter: PermanentFilter::Land }])"
        }
        _ => "None",
    }
}

/// Prohibit: "Kicker {2}".
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    match name {
        "Prohibit" => Some("{2}"),
        _ => None,
    }
}

/// Flashback costs. Moment's Peace: "Flashback {2}{G}". Mystical Teachings:
/// "Flashback {5}{B}".
pub(super) fn flashback_for(name: &str) -> String {
    let cost = match name {
        "Moment's Peace" => "{2}{G}",
        "Mystical Teachings" => "{5}{B}",
        _ => return "None".to_string(),
    };
    let (pips, generic, x_count) = super::parse_cost(cost);
    format!(
        "Some(FlashbackDef {{ cost: &[CostComponent::Mana(Cost {{ pips: &[{}], generic: {generic}, x_count: {x_count} }})] }})",
        pips.join(", ")
    )
}

/// Cryoshatter: "Enchant creature. Enchanted creature gets -5/-0."
pub(super) fn attachment_for(name: &str) -> &'static str {
    match name {
        "Cryoshatter" => "Some(AttachmentDef::AuraCreatureStatic(AuraCreatureStaticDef { power: -5, toughness: 0, keywords: Keywords::NONE, per_controlled_subtype: None }))",
        _ => "None",
    }
}

/// Tithing Blade's back face, Consuming Sepulcher: a black artifact.
pub(super) fn transform_face_for(name: &str) -> &'static str {
    match name {
        "Tithing Blade" => "Some(TransformFaceDef { name: \"Consuming Sepulcher\", types: &[CardType::Artifact], subtypes: &[], colors: &[ManaColor::B], power: None, toughness: None, keywords: Keywords::NONE })",
        _ => "None",
    }
}

pub(super) fn transform_face_name_for(name: &str) -> Option<&'static str> {
    match name {
        "Tithing Blade" => Some("Consuming Sepulcher"),
        _ => None,
    }
}

/// Card-database tokens for the triggered abilities in
/// `eternal_pauper_e6a_v1::triggers_for`, its hooks and its statics.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Archaeomancer" => "etb:target_instant_or_sorcery_card_in_own_graveyard:to_hand",
        "Bramble Wurm" => "etb:gain_life:5",
        "Brinebarrow Intruder" => "etb:target_opponent_creature:minus_2_minus_0_eot",
        "Energy Refractor" | "Prophetic Prism" => "etb:draw:1",
        "Grim Bauble" => "etb:target_opponent_creature:minus_2_minus_2_eot",
        "Kor Skyfisher" => "etb:return_chosen_controlled_permanent_to_owners_hand:untargeted",
        "Mulldrifter" => "etb:draw:2;etb_if_evoked:sacrifice_source",
        "Omen of the Dead" => "etb:target_creature_card_in_own_graveyard:to_hand",
        "Sewer-veillance Cam" => {
            "etb:target_creature:may_tap_or_untap;leaves:target_creature:may_tap_or_untap"
        }
        "Tithing Blade" => {
            "etb:each_opponent_sacrifices_creature;craft_material:creature;face1_controller_upkeep:each_opponent_loses_1_you_gain_1"
        }
        "Cryoshatter" => {
            "enchanted_creature_becomes_tapped_or_is_dealt_damage:destroy_it"
        }
        "Bender's Waterskin" => "static:untaps_during_each_other_players_untap_step",
        "Behold the Multiverse" => {
            "foretell:special_action_pay_2_exile_face_down_your_turn:cast_later_turn_for_foretell_cost"
        }
        "Ephemerate" => {
            "spell:flicker_target_owner_control;rebound:cast_from_hand_exile_on_resolution:next_upkeep_may_cast_free"
        }
        "Ghostly Flicker" => "spell:flicker_two_targets_your_control",
        "Moment's Peace" => "spell:prevent_all_combat_damage_this_turn",
        "Ethereal Haze" => "spell:prevent_all_damage_by_creatures_this_turn",
        "Tangle" => {
            "spell:prevent_all_combat_damage_this_turn:attacking_creatures_skip_next_untap"
        }
        "Stream of Thought" => {
            "replicate:{2}{U}{U}:announced_as_x;spell:target_player_mills_4:shuffle_up_to_4_own_graveyard_cards_into_library"
        }
        "Prohibit" => "spell:counter_target_spell_mana_value_at_most_2_or_4_if_kicked",
        "Union of the Third Path" => "spell:draw_1:gain_life_per_card_in_hand",
        _ => "none",
    }
}
