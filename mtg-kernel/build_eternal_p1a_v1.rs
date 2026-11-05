//! Card-database recipes for the eternal catalog's Premodern wave P1a:
//! Stiflenought, Sligh, RG Aggro, Goblins, Terrageddon and Mono-Black Aggro,
//! with the cards they share with wave P1b. Wave E9a's tables (and E9b's
//! modes, E8a's additional costs and E2's Adventures) fall through to these;
//! runtime programs live in `src/eternal_premodern_p1a_v1.rs`. Every row is
//! keyed by a name that occurs only in the eternal registry.

use super::{
    AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, LibrarySearchFilterRecipe,
    PermanentFilterRecipe, Special, StackSpellFilter,
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

/// "Cycling {cost}": discard this card from hand, draw a card.
const fn cycling(cost: &'static [AbilityCostRecipe]) -> ActivatedAbilityRecipe {
    ActivatedAbilityRecipe {
        activation_zone: "Hand",
        ..instant(cost, AbilityEffectRecipe::DrawCards(1), "None")
    }
}

/// A program of `eternal_premodern_p1a_v1`.
const fn program(path: &'static str) -> AbilityEffectRecipe {
    AbilityEffectRecipe::StandardProgram(path)
}

/// Anurid Brushhopper: "Discard two cards: Exile this creature. Return it
/// to the battlefield under its owner's control at the beginning of the
/// next end step."
const ANURID_BRUSHHOPPER: [ActivatedAbilityRecipe; 1] = [instant(
    &[AbilityCostRecipe::DiscardCards(2)],
    program("crate::eternal_premodern_p1a_v1::brushhopper_exile"),
    "None",
)];

/// Barbarian Ring: "Threshold -- {R}, {T}, Sacrifice this land: It deals 2
/// damage to any target. Activate only if seven or more cards are in your
/// graveyard." The threshold is
/// `eternal_premodern_p1a_v1::statics::activation_allowed`; "{T}: Add {R}.
/// This land deals 1 damage to you." is the mana ability.
const BARBARIAN_RING: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{R}"), Tap, SacrificeSelf],
    AbilityEffectRecipe::DealDamageAnyTarget(2),
    "AnyTarget",
)];

/// Cursed Scroll: "{3}, {T}: Name a card. Reveal a card at random from your
/// hand. If it's the named card, Cursed Scroll deals 2 damage to any
/// target."
const CURSED_SCROLL: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{3}"), Tap],
    program("crate::eternal_premodern_p1a_v1::cursed_scroll"),
    "AnyTarget",
)];

/// Gempalm Incinerator: "Cycling {1}{R}". Its cycle trigger is a hook
/// (`eternal_premodern_p1a_v1::note_cycled`).
const GEMPALM_INCINERATOR: [ActivatedAbilityRecipe; 1] =
    [cycling(&[ManaCost("{1}{R}"), DiscardSelf])];

/// Goblin Sharpshooter: "{T}: This creature deals 1 damage to any target."
const GOBLIN_SHARPSHOOTER: [ActivatedAbilityRecipe; 1] = [instant(
    &[Tap],
    AbilityEffectRecipe::DealDamageAnyTarget(1),
    "AnyTarget",
)];

/// Grim Lavamancer: "{R}, {T}, Exile two cards from your graveyard: This
/// creature deals 2 damage to any target."
const GRIM_LAVAMANCER: [ActivatedAbilityRecipe; 1] = [instant(
    &[
        ManaCost("{R}"),
        Tap,
        AbilityCostRecipe::ExileGraveyardCards(2),
    ],
    AbilityEffectRecipe::DealDamageAnyTarget(2),
    "AnyTarget",
)];

/// Knight of Stromgald: "{B}: This creature gains first strike until end of
/// turn." and "{B}{B}: This creature gets +1/+0 until end of turn."
const KNIGHT_OF_STROMGALD: [ActivatedAbilityRecipe; 2] = [
    instant(
        &[ManaCost("{B}")],
        program("crate::eternal_premodern_p1a_v1::knight_first_strike"),
        "None",
    ),
    instant(
        &[ManaCost("{B}{B}")],
        AbilityEffectRecipe::PumpSourceUntilEndOfTurn {
            power: 1,
            toughness: 0,
        },
        "None",
    ),
];

/// Mogg Fanatic: "Sacrifice this creature: It deals 1 damage to any
/// target."
const MOGG_FANATIC: [ActivatedAbilityRecipe; 1] = [instant(
    &[SacrificeSelf],
    AbilityEffectRecipe::DealDamageAnyTarget(1),
    "AnyTarget",
)];

/// Nantuko Shade: "{B}: This creature gets +1/+1 until end of turn."
const NANTUKO_SHADE: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{B}")],
    AbilityEffectRecipe::PumpSourceUntilEndOfTurn {
        power: 1,
        toughness: 1,
    },
    "None",
)];

/// Powder Keg: "{T}, Sacrifice this artifact: Destroy each artifact and
/// creature with mana value equal to the number of fuse counters on this
/// artifact."
const POWDER_KEG: [ActivatedAbilityRecipe; 1] = [instant(
    &[Tap, SacrificeSelf],
    program("crate::eternal_premodern_p1a_v1::powder_keg"),
    "None",
)];

/// Rishadan Port: "{1}, {T}: Tap target land." Its "{T}: Add {C}." is the
/// mana ability.
const RISHADAN_PORT: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{1}"), Tap],
    AbilityEffectRecipe::Program {
        name: "tap_target_land_p1a",
        effect: "EffectOp::TapObject { object: ObjectRef::Target(0) }",
    },
    "Land",
)];

/// Seal of Fire: "Sacrifice this enchantment: It deals 2 damage to any
/// target."
const SEAL_OF_FIRE: [ActivatedAbilityRecipe; 1] = [instant(
    &[SacrificeSelf],
    AbilityEffectRecipe::DealDamageAnyTarget(2),
    "AnyTarget",
)];

/// Secluded Steppe: "Cycling {W}".
const SECLUDED_STEPPE: [ActivatedAbilityRecipe; 1] = [cycling(&[ManaCost("{W}"), DiscardSelf])];

/// Siege-Gang Commander: "{1}{R}, Sacrifice a Goblin: It deals 2 damage to
/// any target."
const SIEGE_GANG_COMMANDER: [ActivatedAbilityRecipe; 1] = [instant(
    &[
        ManaCost("{1}{R}"),
        AbilityCostRecipe::SacrificeControlled {
            count: 1,
            filter: PermanentFilterRecipe::Goblin,
        },
    ],
    AbilityEffectRecipe::DealDamageAnyTarget(2),
    "AnyTarget",
)];

/// Tranquil Thicket: "Cycling {G}".
const TRANQUIL_THICKET: [ActivatedAbilityRecipe; 1] = [cycling(&[ManaCost("{G}"), DiscardSelf])];

/// Weathered Wayfarer: "{W}, {T}: Search your library for a land card,
/// reveal it, put it into your hand, then shuffle. Activate only if an
/// opponent controls more lands than you." The condition is
/// `eternal_premodern_p1a_v1::statics::activation_allowed`.
const WEATHERED_WAYFARER: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{W}"), Tap],
    AbilityEffectRecipe::SearchLibraryToHand {
        filter: LibrarySearchFilterRecipe::AnyLand,
        min_targets: 0,
        max_targets: 1,
        reveal_selected: true,
        shuffle: true,
    },
    "None",
)];

/// Wild Mongrel: "Discard a card: This creature gets +1/+1 and becomes the
/// color of your choice until end of turn."
const WILD_MONGREL: [ActivatedAbilityRecipe; 1] = [instant(
    &[AbilityCostRecipe::DiscardCards(1)],
    program("crate::eternal_premodern_p1a_v1::wild_mongrel"),
    "None",
)];

/// Withered Wretch: "{1}: Exile target card from a graveyard."
const WITHERED_WRETCH: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{1}")],
    AbilityEffectRecipe::Program {
        name: "exile_target_graveyard_card_p1a",
        effect: "EffectOp::MoveObject { object: ObjectRef::Target(0), to_zone: Zone::Exile }",
    },
    "StandardV1(crate::standard_cards_v1::StandardTargetV1::CardInAGraveyard)",
)];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Anurid Brushhopper" => &ANURID_BRUSHHOPPER,
        "Barbarian Ring" => &BARBARIAN_RING,
        "Cursed Scroll" => &CURSED_SCROLL,
        "Gempalm Incinerator" => &GEMPALM_INCINERATOR,
        "Goblin Sharpshooter" => &GOBLIN_SHARPSHOOTER,
        "Grim Lavamancer" => &GRIM_LAVAMANCER,
        "Knight of Stromgald" => &KNIGHT_OF_STROMGALD,
        "Mogg Fanatic" => &MOGG_FANATIC,
        "Nantuko Shade" => &NANTUKO_SHADE,
        "Powder Keg" => &POWDER_KEG,
        "Rishadan Port" => &RISHADAN_PORT,
        "Seal of Fire" => &SEAL_OF_FIRE,
        "Secluded Steppe" => &SECLUDED_STEPPE,
        "Siege-Gang Commander" => &SIEGE_GANG_COMMANDER,
        "Tranquil Thicket" => &TRANQUIL_THICKET,
        "Weathered Wayfarer" => &WEATHERED_WAYFARER,
        "Wild Mongrel" => &WILD_MONGREL,
        "Withered Wretch" => &WITHERED_WRETCH,
        _ => &[],
    }
}

/// A spell whose program is a runtime function of `eternal_premodern_p1a_v1`.
const fn spell(target_spec: &'static str, program: &'static str) -> Special {
    Special::StandardProgram {
        target_spec,
        program,
    }
}

/// Spell programs.
pub(super) fn special_for(name: &str) -> Option<Special> {
    Some(match name {
        "Armageddon" => spell(
            "TargetSpec::None",
            "crate::eternal_premodern_p1a_v1::armageddon",
        ),
        // Its flashback is `flashback_for` below.
        "Call of the Herd" => spell(
            "TargetSpec::None",
            "crate::eternal_premodern_p1a_v1::call_of_the_herd",
        ),
        "Diabolic Edict" => spell(
            "TargetSpec::AnyPlayer",
            "crate::eternal_premodern_p1a_v1::diabolic_edict",
        ),
        "Disenchant" | "Naturalize" => spell(
            "TargetSpec::ArtifactOrEnchantmentPermanent",
            "crate::eternal_premodern_p1a_v1::destroy_target",
        ),
        // Fire; Ice is the split card's second half (`adventure_for`
        // below), and the division of Fire's damage is announced with its
        // targets (`eternal_premodern_p1a_v1::targets`).
        "Fire // Ice" => spell(
            "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::DividedTwoDamageAnyTargetsV1)",
            "crate::eternal_premodern_p1a_v1::fire",
        ),
        // Its flashback is `flashback_for` below.
        "Flash of Insight" => spell(
            "TargetSpec::None",
            "crate::eternal_premodern_p1a_v1::flash_of_insight",
        ),
        // Their alternative costs are `alt_cost_for` below.
        "Foil" | "Thwart" => Special::CounterTarget(StackSpellFilter::Any),
        // Modes two and three are `mode2_for` and `mode3_for` below.
        "Funeral Charm" => spell(
            "TargetSpec::AnyPlayer",
            "crate::eternal_premodern_p1a_v1::funeral_charm_discard",
        ),
        "Incinerate" => spell(
            "TargetSpec::AnyTarget",
            "crate::eternal_premodern_p1a_v1::incinerate",
        ),
        "Portent" => spell(
            "TargetSpec::AnyPlayer",
            "crate::eternal_premodern_p1a_v1::portent",
        ),
        "Price of Progress" => spell(
            "TargetSpec::None",
            "crate::eternal_premodern_p1a_v1::price_of_progress",
        ),
        // Its additional cost is `additional_cost_for` below.
        "Skeletal Scrying" => spell(
            "TargetSpec::None",
            "crate::eternal_premodern_p1a_v1::skeletal_scrying",
        ),
        "Smother" => spell(
            "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::CreatureManaValueAtMostV1(3))",
            "crate::eternal_premodern_p1a_v1::smother",
        ),
        // Modes two and three are `mode2_for` and `mode3_for` below.
        "Vision Charm" => spell(
            "TargetSpec::AnyPlayer",
            "crate::eternal_premodern_p1a_v1::vision_charm_mill",
        ),
        _ => return None,
    })
}

/// Printed evergreen keywords, in printed order.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Ball Lightning" => &["Keywords::TRAMPLE", "Keywords::HASTE"],
        "Exalted Angel" => &["Keywords::FLYING", "Keywords::LIFELINK"],
        "Goblin Ringleader" => &["Keywords::HASTE"],
        "Hypnotic Specter" => &["Keywords::FLYING"],
        "Terravore" => &["Keywords::TRAMPLE"],
        _ => &[],
    }
}

/// Alternative costs. Foil: "You may discard an Island card and another
/// card rather than pay this spell's mana cost." Thwart: "You may return
/// three Islands you control to their owner's hand rather than pay this
/// spell's mana cost." Exalted Angel: "Morph {2}{W}{W}" (702.37): cast face
/// down as a 2/2 creature for {3}, through the face-down cast that disguise
/// uses, without disguise's ward; turning it face up is
/// `eternal_premodern_p1a_v1::morph_face_up_cost`.
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    match name {
        "Foil" => "Some(AltCostDef { components: &[CostComponent::DiscardIslandAndAnotherCardV1], condition: AltCostCondition::Always })",
        "Thwart" => "Some(AltCostDef { components: &[CostComponent::ReturnControlledPermanentsToOwnersHandV1 { count: 3, filter: PermanentFilterDef::LandWithSubtype(Subtype::Island) }], condition: AltCostCondition::Always })",
        "Exalted Angel" => "Some(AltCostDef { components: &[CostComponent::Mana(Cost { pips: &[], generic: 3, x_count: 0 })], condition: AltCostCondition::DisguiseFromHand })",
        _ => "None",
    }
}

/// Skeletal Scrying: "As an additional cost to cast this spell, exile X
/// cards from your graveyard."
pub(super) fn additional_cost_for(name: &str) -> &'static str {
    match name {
        "Skeletal Scrying" => {
            "Some(&[CostComponent::ExileXCardsFromOwnGraveyardV1 { blue_only: false }])"
        }
        _ => "None",
    }
}

/// Call of the Herd: "Flashback {3}{G}". Flash of Insight: "Flashback--
/// {1}{U}, Exile X blue cards from your graveyard."
pub(super) fn flashback_for(name: &str) -> String {
    let flashback = |cost: &str, extra: &str| {
        let (pips, generic, x_count) = super::parse_cost(cost);
        format!(
            "Some(FlashbackDef {{ cost: &[CostComponent::Mana(Cost {{ pips: &[{}], generic: {generic}, x_count: {x_count} }}){extra}] }})",
            pips.join(", ")
        )
    };
    match name {
        "Call of the Herd" => flashback("{3}{G}", ""),
        "Flash of Insight" => flashback(
            "{1}{U}",
            ", CostComponent::ExileXCardsFromOwnGraveyardV1 { blue_only: true }",
        ),
        _ => "None".to_string(),
    }
}

/// Single printed mana abilities that are not "tap: add one". Barbarian
/// Ring: "{T}: Add {R}. This land deals 1 damage to you." Skirk Prospector:
/// "Sacrifice a Goblin: Add {R}."
pub(super) fn mana_ability_def_for(name: &str) -> &'static str {
    match name {
        "Barbarian Ring" => "Some(ManaAbilityDef { cost: ManaAbilityCostDef::TapSelf, amount: ManaAbilityAmountDef::Fixed(1), controller_damage: 1, max_activations_per_turn: None })",
        "Skirk Prospector" => "Some(ManaAbilityDef { cost: ManaAbilityCostDef::SacrificeControlledWithSubtypeV1(Subtype::Goblin), amount: ManaAbilityAmountDef::Fixed(1), controller_damage: 0, max_activations_per_turn: None })",
        _ => "None",
    }
}

/// Fire // Ice's second half, Ice {1}{U}: "Tap target permanent. Draw a
/// card." Split cards (708) reuse the Adventure form: the second half is
/// cast from hand by choosing it as the spell's form, and
/// `eternal_premodern_p1a_v1::split_card` sends it to the graveyard rather
/// than exile.
pub(super) fn adventure_for(name: &str) -> &'static str {
    match name {
        "Fire // Ice" => "Some(AdventureDef { name: \"Ice\", cost: Cost { pips: &[Pip::Colored(ManaColor::U)], generic: 1, x_count: 0 }, types: &[CardType::Instant], target_spec: TargetSpec::AnyPermanent, effect: ability_effect_program_eternal_premodern_p1a_v1_ice })",
        _ => "None",
    }
}

pub(super) fn adventure_effect_recipe_for(name: &str) -> Option<AbilityEffectRecipe> {
    match name {
        "Fire // Ice" => Some(program("crate::eternal_premodern_p1a_v1::ice")),
        _ => None,
    }
}

pub(super) fn adventure_face_name_for(name: &str) -> Option<&'static str> {
    match name {
        "Fire // Ice" => Some("Ice"),
        _ => None,
    }
}

/// Funeral Charm's second mode: "Target creature gets +2/-1 until end of
/// turn." Vision Charm's second mode: "Choose a land type and a basic land
/// type. Each land of the first chosen type becomes the second chosen type
/// until end of turn."
pub(super) fn mode2_for(name: &str) -> &'static str {
    match name {
        "Funeral Charm" => "Some(ModeDef { target_spec: TargetSpec::Creature, effect: crate::eternal_premodern_p1a_v1::funeral_charm_pump })",
        "Vision Charm" => "Some(ModeDef { target_spec: TargetSpec::None, effect: crate::eternal_premodern_p1a_v1::vision_charm_lands })",
        _ => "None",
    }
}

/// Funeral Charm's third mode: "Target creature gains swampwalk until end of
/// turn." Vision Charm's third mode: "Target artifact phases out."
pub(super) fn mode3_for(name: &str) -> &'static str {
    match name {
        "Funeral Charm" => "Some(ModeDef { target_spec: TargetSpec::Creature, effect: crate::eternal_premodern_p1a_v1::funeral_charm_swampwalk })",
        "Vision Charm" => "Some(ModeDef { target_spec: TargetSpec::ArtifactPermanent, effect: crate::eternal_premodern_p1a_v1::vision_charm_phase })",
        _ => "None",
    }
}

/// Card-database tokens for the triggered abilities and statics in
/// `eternal_premodern_p1a_v1`.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Anurid Brushhopper" => "activated:discard_2:exile_self:return_next_end_step",
        "Armageddon" => "spell:destroy_all_lands",
        "Ball Lightning" => "each_end_step:sacrifice_source",
        "Barbarian Ring" => "mana:r:damage_1;threshold:sacrifice:damage_2_any_target",
        "Call of the Herd" => "spell:elephant_3_3",
        "Cursed Scroll" => "activated:name_card:reveal_random_hand_card:damage_2_if_named",
        "Diabolic Edict" => "spell:target_player_sacrifices_creature",
        "Disenchant" | "Naturalize" => "spell:destroy_target_artifact_or_enchantment",
        "Exalted Angel" => "morph:2ww:face_down_2_2_no_ward",
        "Fire // Ice" => {
            "split:fire:2_damage_divided_one_or_two_any_targets;ice:tap_target_permanent:draw_1"
        }
        "Flash of Insight" => {
            "spell:look_top_x:one_to_hand:rest_bottom_any_order;flashback:exile_x_blue_graveyard"
        }
        "Funeral Charm" => "modal:target_player_discards_1|pump_2_minus_1|swampwalk",
        "Gempalm Incinerator" => "cycled:may_damage_target_creature_goblin_count",
        "Gemstone Mine" => "enters_with_3_mining:mana_any_remove_counter:sacrifice_at_zero",
        "Goblin Goon" => "static:attack_and_block_need_more_creatures",
        "Goblin Lackey" => "deals_damage_to_player:may_put_goblin_permanent_from_hand",
        "Goblin Matron" => "etb:may_search_goblin_card_to_hand",
        "Goblin Piledriver" => "protection_from_blue;attacks:plus_2_per_other_attacking_goblin",
        "Goblin Ringleader" => "etb:reveal_top_4:goblins_to_hand:rest_bottom_any_order",
        "Goblin Sharpshooter" => "doesnt_untap;creature_dies:untap_source;activated:damage_1",
        "Goblin Vandal" => {
            "attacks_unblocked:may_pay_r:destroy_defender_artifact:assign_no_combat_damage"
        }
        "Goblin Warchief" => "static:goblin_spells_cost_1_less;goblins_have_haste",
        "Graveborn Muse" => "upkeep:draw_x_lose_x:zombies_you_control",
        "Hypnotic Specter" => "deals_damage_to_opponent:random_discard",
        "Incinerate" => "spell:damage_3:cant_be_regenerated",
        "Jackal Pup" => "dealt_damage:damage_controller_that_much",
        "Knight of Stromgald" => "protection_from_white;activated:first_strike;activated:pump_1_0",
        "Mystic Enforcer" => "protection_from_black;threshold:plus_3_plus_3_flying",
        "Portent" => "spell:reorder_top_3_target:may_shuffle;next_upkeep:draw_1",
        "Powder Keg" => {
            "upkeep:may_fuse_counter;activated:destroy_artifacts_creatures_mana_value_fuse"
        }
        "Price of Progress" => "spell:damage_each_player_twice_nonbasic_lands",
        "Rith's Grove" => "etb:sacrifice_unless_return_non_lair_land;mana:r_g_w",
        "Siege-Gang Commander" => "etb:three_goblin_tokens;activated:sacrifice_goblin:damage_2",
        "Skeletal Scrying" => "spell:additional_exile_x_graveyard:draw_x_lose_x",
        "Skirk Prospector" => "mana:sacrifice_goblin:r",
        "Smother" => "spell:destroy_creature_mana_value_at_most_3:cant_be_regenerated",
        "Sulfuric Vortex" => "each_upkeep:damage_2_active_player;static:players_gain_no_life",
        "Sylvan Library" => {
            "draw_step:may_draw_2:choose_2_drawn_this_turn:pay_4_life_or_put_on_top"
        }
        "Terravore" => "static:power_toughness_land_cards_in_all_graveyards",
        "Vision Charm" => {
            "modal:target_player_mills_4|land_type_becomes_basic_type|artifact_phases_out"
        }
        "Weathered Wayfarer" => "activated:opponent_more_lands:search_land_to_hand",
        "Wild Mongrel" => "activated:discard:pump_1_1:becomes_chosen_color",
        "Foil" => "alt:discard_island_and_another;spell:counter_target",
        "Thwart" => "alt:return_three_islands;spell:counter_target",
        _ => "none",
    }
}
