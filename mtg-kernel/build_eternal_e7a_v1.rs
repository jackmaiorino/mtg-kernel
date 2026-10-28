//! Card-database recipes for the eternal catalog's wave E7a (Legacy Lands,
//! Sewer-Cam combo, Rakdos Reanimator, The Epic Storm, Stiflenought, Mardu
//! Energy and Affinity Stompy). `build_eternal_e6a_v1` and the other eternal
//! tables fall through to these; runtime programs live in
//! `src/eternal_legacy_e7a_v1.rs`. Every row is keyed by a name that occurs
//! only in the eternal registry.

use super::{
    AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, PermanentFilterRecipe, Special,
};
use AbilityCostRecipe::{ManaCost, PayLife, SacrificeSelf, Tap};

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

/// "Equip {N}": attach to target creature you control, as a sorcery.
const fn equip(cost: &'static [AbilityCostRecipe]) -> ActivatedAbilityRecipe {
    ActivatedAbilityRecipe {
        cost,
        effect: AbilityEffectRecipe::AttachSourceToTarget,
        activation_zone: "Battlefield",
        sorcery_speed_only: true,
        target_spec: "ControlledCreature",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    }
}

/// A program of `eternal_legacy_e7a_v1`.
const fn program(path: &'static str) -> AbilityEffectRecipe {
    AbilityEffectRecipe::StandardProgram(path)
}

/// Blast Zone: "{X}{X}, {T}: Put X charge counters on this land." The
/// engine has no free X for an activation, so each X from 0 to 7 is its own
/// ability (X = 7 already costs fourteen mana), then "{3}, {T}, Sacrifice
/// this land: Destroy each nonland permanent with mana value equal to the
/// number of charge counters on this land." Its `{T}: Add {C}` is the
/// ordinary mana ability.
const BLAST_ZONE: [ActivatedAbilityRecipe; 9] = [
    instant(
        &[Tap],
        program("crate::eternal_legacy_e7a_v1::blast_zone_charge_0"),
        "None",
    ),
    instant(
        &[ManaCost("{2}"), Tap],
        program("crate::eternal_legacy_e7a_v1::blast_zone_charge_1"),
        "None",
    ),
    instant(
        &[ManaCost("{4}"), Tap],
        program("crate::eternal_legacy_e7a_v1::blast_zone_charge_2"),
        "None",
    ),
    instant(
        &[ManaCost("{6}"), Tap],
        program("crate::eternal_legacy_e7a_v1::blast_zone_charge_3"),
        "None",
    ),
    instant(
        &[ManaCost("{8}"), Tap],
        program("crate::eternal_legacy_e7a_v1::blast_zone_charge_4"),
        "None",
    ),
    instant(
        &[ManaCost("{10}"), Tap],
        program("crate::eternal_legacy_e7a_v1::blast_zone_charge_5"),
        "None",
    ),
    instant(
        &[ManaCost("{12}"), Tap],
        program("crate::eternal_legacy_e7a_v1::blast_zone_charge_6"),
        "None",
    ),
    instant(
        &[ManaCost("{14}"), Tap],
        program("crate::eternal_legacy_e7a_v1::blast_zone_charge_7"),
        "None",
    ),
    instant(
        &[ManaCost("{3}"), Tap, SacrificeSelf],
        program("crate::eternal_legacy_e7a_v1::blast_zone_destroy"),
        "None",
    ),
];

/// Dark Depths: "{3}: Remove an ice counter from Dark Depths."
const DARK_DEPTHS: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{3}")],
    program("crate::eternal_legacy_e7a_v1::dark_depths_remove_ice"),
    "None",
)];

/// Ghost Quarter: "{T}, Sacrifice this land: Destroy target land. Its
/// controller may search their library for a basic land card, put it onto
/// the battlefield, then shuffle."
const GHOST_QUARTER: [ActivatedAbilityRecipe; 1] = [instant(
    &[Tap, SacrificeSelf],
    program("crate::eternal_legacy_e7a_v1::ghost_quarter"),
    "Land",
)];

/// Maze of Ith: "{T}: Untap target attacking creature. Prevent all combat
/// damage that would be dealt to and dealt by that creature this turn."
const MAZE_OF_ITH: [ActivatedAbilityRecipe; 1] = [instant(
    &[Tap],
    program("crate::eternal_legacy_e7a_v1::maze_of_ith"),
    "AttackingCreature",
)];

/// Thespian's Stage: "{2}, {T}: This land becomes a copy of target land,
/// except it has this ability."
const THESPIANS_STAGE: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{2}"), Tap],
    program("crate::eternal_legacy_e7a_v1::thespians_stage"),
    "Land",
)];

/// Silent Clearing: "{1}, {T}, Sacrifice this land: Draw a card." Its "{T},
/// Pay 1 life: Add {W} or {B}." is an additional mana ability.
const SILENT_CLEARING: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{1}"), Tap, SacrificeSelf],
    AbilityEffectRecipe::DrawCards(1),
    "None",
)];

/// A fetchland: "{T}, Pay 1 life, Sacrifice this land: Search your library
/// for a <first> or <second> card, put it onto the battlefield, then
/// shuffle."
macro_rules! fetchland {
    ($name:literal, $first:literal, $second:literal) => {
        [instant(
            &[Tap, PayLife(1), SacrificeSelf],
            AbilityEffectRecipe::Program {
                name: $name,
                effect: concat!(
                    "EffectOp::SearchLibraryCardsToDestination { player: PlayerRef::Controller, filter: crate::effect::LibraryCardFilter::CardWithAnySubtypeV1([Subtype::",
                    $first,
                    ", Subtype::",
                    $second,
                    "]), max_targets: 1, destination: crate::effect::LibrarySearchDestinationV1::Battlefield { tapped: false } }"
                ),
            },
            "None",
        )]
    };
}

const VERDANT_CATACOMBS: [ActivatedAbilityRecipe; 1] =
    fetchland!("fetch_swamp_or_forest", "Swamp", "Forest");
const WOODED_FOOTHILLS: [ActivatedAbilityRecipe; 1] =
    fetchland!("fetch_mountain_or_forest", "Mountain", "Forest");

/// Griselbrand: "Pay 7 life: Draw seven cards."
const GRISELBRAND: [ActivatedAbilityRecipe; 1] = [instant(
    &[PayLife(7)],
    AbilityEffectRecipe::DrawCards(7),
    "None",
)];

/// Goblin Engineer: "{R}, {T}, Sacrifice an artifact: Return target
/// artifact card with mana value 3 or less from your graveyard to the
/// battlefield."
const GOBLIN_ENGINEER: [ActivatedAbilityRecipe; 1] = [instant(
    &[
        ManaCost("{R}"),
        Tap,
        AbilityCostRecipe::SacrificeControlled {
            count: 1,
            filter: PermanentFilterRecipe::Artifact,
        },
    ],
    program("crate::eternal_legacy_e7a_v1::goblin_engineer_return"),
    "StandardV1(crate::standard_cards_v1::StandardTargetV1::ArtifactCardInOwnGraveyardManaValueAtMostV1(3))",
)];

/// Goblin Welder: "{T}: Choose target artifact a player controls and target
/// artifact card in that player's graveyard. If both targets are still
/// legal as this ability resolves, that player simultaneously sacrifices
/// the artifact and returns the artifact card to the battlefield."
const GOBLIN_WELDER: [ActivatedAbilityRecipe; 1] = [instant(
    &[Tap],
    program("crate::eternal_legacy_e7a_v1::goblin_welder"),
    "StandardV1(crate::standard_cards_v1::StandardTargetV1::ArtifactThenArtifactCardInItsControllersGraveyardV1)",
)];

/// Emry, Lurker of the Loch: "{T}: Choose target artifact card in your
/// graveyard. You may cast that card this turn."
const EMRY: [ActivatedAbilityRecipe; 1] = [instant(
    &[Tap],
    program("crate::eternal_legacy_e7a_v1::emry_choose_artifact"),
    "StandardV1(crate::standard_cards_v1::StandardTargetV1::ArtifactCardInOwnGraveyardV1)",
)];

/// Legion Extruder: "{2}, {T}, Sacrifice another artifact: Create a 3/3
/// colorless Golem artifact creature token."
const LEGION_EXTRUDER: [ActivatedAbilityRecipe; 1] = [instant(
    &[
        ManaCost("{2}"),
        Tap,
        AbilityCostRecipe::SacrificeControlled {
            count: 1,
            filter: PermanentFilterRecipe::AnotherArtifact,
        },
    ],
    AbilityEffectRecipe::CreateToken("Golem Token"),
    "None",
)];

/// Phyrexian Dragon Engine: "Unearth {3}{R}{R}".
const PHYREXIAN_DRAGON_ENGINE: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[ManaCost("{3}{R}{R}")],
    effect: AbilityEffectRecipe::Unearth,
    activation_zone: "Graveyard",
    sorcery_speed_only: true,
    target_spec: "None",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

/// Skateboard: "Equip {1}".
const SKATEBOARD: [ActivatedAbilityRecipe; 1] = [equip(&[ManaCost("{1}")])];

/// Shadowspear: "{1}: Permanents your opponents control lose hexproof and
/// indestructible until end of turn." and "Equip {2}".
const SHADOWSPEAR: [ActivatedAbilityRecipe; 2] = [
    instant(
        &[ManaCost("{1}")],
        program("crate::eternal_legacy_e7a_v1::shadowspear_strip"),
        "None",
    ),
    equip(&[ManaCost("{2}")]),
];

/// Curie, Emergent Intelligence: "{1}{U}, Exile another nontoken artifact
/// creature you control: Curie becomes a copy of the exiled creature,
/// except it has 'Whenever this creature deals combat damage to a player,
/// draw cards equal to its base power.'" The exiled creature is the
/// activation's material cost, restricted by
/// `eternal_legacy_e7a_v1::craft_material_allowed`.
const CURIE: [ActivatedAbilityRecipe; 1] = [instant(
    &[
        ManaCost("{1}{U}"),
        AbilityCostRecipe::ExileCraftArtifactMaterial,
    ],
    program("crate::eternal_legacy_e7a_v1::curie_copy"),
    "None",
)];

/// Arena of Glory: "{R}, {T}, Exert this land: Add {R}{R}. If any of that
/// mana is spent on a creature spell, it gains haste until end of turn."
/// A mana ability: the engine performs it without the stack
/// (`eternal_legacy_e7a_v1::activate_arena`), offered as ability 0, since
/// its plain `{T}: Add {R}` already uses the red mana action.
const ARENA_OF_GLORY: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{R}"), Tap],
    program("crate::eternal_legacy_e7a_v1::arena_exert"),
    "None",
)];

/// Gaea's Will: "Suspend 4--{G}" (702.62a): a special action from the hand
/// any time its owner could cast it, performed without the stack
/// (`eternal_legacy_e7a_v1::suspend`).
const GAEAS_WILL: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[ManaCost("{G}")],
    effect: program("crate::eternal_legacy_e7a_v1::suspend_action"),
    activation_zone: "Hand",
    sorcery_speed_only: true,
    target_spec: "None",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Blast Zone" => &BLAST_ZONE,
        "Dark Depths" => &DARK_DEPTHS,
        "Ghost Quarter" => &GHOST_QUARTER,
        "Maze of Ith" => &MAZE_OF_ITH,
        "Thespian's Stage" => &THESPIANS_STAGE,
        "Silent Clearing" => &SILENT_CLEARING,
        "Verdant Catacombs" => &VERDANT_CATACOMBS,
        "Wooded Foothills" => &WOODED_FOOTHILLS,
        "Griselbrand" => &GRISELBRAND,
        "Goblin Engineer" => &GOBLIN_ENGINEER,
        "Goblin Welder" => &GOBLIN_WELDER,
        "Emry, Lurker of the Loch" => &EMRY,
        "Legion Extruder" => &LEGION_EXTRUDER,
        "Phyrexian Dragon Engine" => &PHYREXIAN_DRAGON_ENGINE,
        "Skateboard" => &SKATEBOARD,
        "Shadowspear" => &SHADOWSPEAR,
        "Curie, Emergent Intelligence" => &CURIE,
        "Arena of Glory" => &ARENA_OF_GLORY,
        "Gaea's Will" => &GAEAS_WILL,
        _ => super::build_eternal_e7b_v1::activated_ability_recipes_for(name),
    }
}

/// A spell whose program is a runtime function of `eternal_legacy_e7a_v1`.
const fn spell(target_spec: &'static str, program: &'static str) -> Special {
    Special::StandardProgram {
        target_spec,
        program,
    }
}

/// Spell programs, including the permanent spells that do more than enter.
pub(super) fn special_for(name: &str) -> Option<Special> {
    Some(match name {
        // An Aura spell: it enters, then its entry trigger returns the
        // targeted card.
        "Animate Dead" => spell(
            "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::CreatureCardInAGraveyardV1)",
            "crate::eternal_legacy_e7a_v1::animate_dead",
        ),
        "Burning Wish" => spell(
            "TargetSpec::None",
            "crate::eternal_legacy_e7a_v1::burning_wish",
        ),
        // Flashback is `flashback_for` below.
        "Cabal Therapy" => spell(
            "TargetSpec::AnyPlayer",
            "crate::eternal_legacy_e7a_v1::cabal_therapy",
        ),
        // Flashback is `flashback_for` below.
        "Echo of Eons" => spell(
            "TargetSpec::None",
            "crate::eternal_legacy_e7a_v1::echo_of_eons",
        ),
        // Cast only through suspend's free cast.
        "Gaea's Will" => spell(
            "TargetSpec::None",
            "crate::eternal_legacy_e7a_v1::gaeas_will",
        ),
        // Storm is its cast trigger (`eternal_cards_v1::e3`).
        "Galvanic Relay" => spell(
            "TargetSpec::None",
            "crate::eternal_legacy_e7a_v1::galvanic_relay",
        ),
        "Gamble" => spell("TargetSpec::None", "crate::eternal_legacy_e7a_v1::gamble"),
        "Life from the Loam" => spell(
            "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::UpToThreeLandCardsInOwnGraveyardV1)",
            "crate::eternal_legacy_e7a_v1::life_from_the_loam",
        ),
        "Reanimate" => spell(
            "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::CreatureCardInAGraveyardV1)",
            "crate::eternal_legacy_e7a_v1::reanimate",
        ),
        "Shallow Grave" => spell(
            "TargetSpec::None",
            "crate::eternal_legacy_e7a_v1::shallow_grave",
        ),
        // Its alternative cost is `alt_cost_for` below.
        "Unmask" => spell("TargetSpec::AnyPlayer", "crate::eternal_legacy_e7a_v1::unmask"),
        _ => return super::build_eternal_e7b_v1::special_for(name),
    })
}

/// Printed evergreen keywords, in printed order.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Abhorrent Oculus" => &["Keywords::FLYING"],
        "Archon of Cruelty" => &["Keywords::FLYING"],
        "Dress Down" => &["Keywords::FLASH"],
        "Griselbrand" => &["Keywords::FLYING", "Keywords::LIFELINK"],
        "Memory Guardian" => &["Keywords::FLYING"],
        "Phyrexian Dragon Engine" => &["Keywords::DOUBLE_STRIKE"],
        "Phyrexian Dreadnought" => &["Keywords::TRAMPLE"],
        "Marit Lage Token" => &["Keywords::FLYING", "Keywords::INDESTRUCTIBLE"],
        "Drone Token" => &["Keywords::FLYING"],
        _ => super::build_eternal_e7b_v1::keywords_for(name),
    }
}

/// Additional mana abilities beyond the primary one.
pub(super) fn additional_mana_abilities_for(name: &str) -> &'static str {
    match name {
        // Silent Clearing: "{T}, Pay 1 life: Add {W} or {B}."
        "Silent Clearing" => "&[AdditionalManaAbilityDef { colors: &[ManaColor::W, ManaColor::B], mana_cost: Cost { pips: &[], generic: 0, x_count: 0 }, ability: ManaAbilityDef { cost: ManaAbilityCostDef::TapSelfPayLife(1), amount: ManaAbilityAmountDef::Fixed(1), controller_damage: 0, max_activations_per_turn: None } }]",
        // Chromatic Star: "{1}, {T}, Sacrifice this artifact: Add one mana
        // of any color."
        "Chromatic Star" => "&[AdditionalManaAbilityDef { colors: &[ManaColor::W, ManaColor::U, ManaColor::B, ManaColor::R, ManaColor::G], mana_cost: Cost { pips: &[], generic: 1, x_count: 0 }, ability: ManaAbilityDef { cost: ManaAbilityCostDef::TapAndSacrificeSelf, amount: ManaAbilityAmountDef::Fixed(1), controller_damage: 0, max_activations_per_turn: None } }]",
        _ => "&[]",
    }
}

/// Alternative costs. Unmask: "You may exile a black card from your hand
/// rather than pay this spell's mana cost." Pinnacle Emissary: "Warp
/// {U/R}". Kappa Cannoneer's improvise is its convoke-style alternative
/// payment (`standard_cast_convoke`, artifacts for generic mana).
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    match name {
        "Unmask" => "Some(AltCostDef { components: &[CostComponent::ExileCardFromHandWithColorV1(ManaColor::B)], condition: AltCostCondition::Always })",
        "Pinnacle Emissary" => "Some(AltCostDef { components: &[CostComponent::Mana(Cost { pips: &[Pip::Hybrid(ManaColor::U, ManaColor::R)], generic: 0, x_count: 0 })], condition: AltCostCondition::WarpFromHand })",
        "Kappa Cannoneer" => "Some(AltCostDef { components: &[CostComponent::ConvokeMana(Cost { pips: &[Pip::Colored(ManaColor::U)], generic: 5, x_count: 0 })], condition: AltCostCondition::Always })",
        _ => super::build_eternal_e7b_v1::alt_cost_for(name),
    }
}

/// Mandatory additional costs. Abhorrent Oculus: "As an additional cost to
/// cast this spell, exile six cards from your graveyard."
pub(super) fn additional_cost_for(name: &str) -> &'static str {
    match name {
        "Abhorrent Oculus" => "Some(&[CostComponent::ExileOtherCardsFromOwnGraveyard(6)])",
        _ => super::build_eternal_e8a_v1::additional_cost_for(name),
    }
}

/// Flashback costs. Cabal Therapy: "Flashback--Sacrifice a creature." Echo
/// of Eons: "Flashback {2}{U}".
pub(super) fn flashback_for(name: &str) -> String {
    match name {
        "Cabal Therapy" => "Some(FlashbackDef { cost: &[CostComponent::SacrificeControlled { count: 1, filter: PermanentFilter::Creature }] })".to_string(),
        "Echo of Eons" => {
            let (pips, generic, x_count) = super::parse_cost("{2}{U}");
            format!(
                "Some(FlashbackDef {{ cost: &[CostComponent::Mana(Cost {{ pips: &[{}], generic: {generic}, x_count: {x_count} }})] }})",
                pips.join(", ")
            )
        }
        _ => "None".to_string(),
    }
}

/// Animate Dead: "Enchanted creature gets -1/-0."
pub(super) fn attachment_for(name: &str) -> &'static str {
    match name {
        "Animate Dead" => "Some(AttachmentDef::AuraCreatureStatic(AuraCreatureStaticDef { power: -1, toughness: 0, keywords: Keywords::NONE, per_controlled_subtype: None }))",
        _ => "None",
    }
}

/// Equipment statics. Skateboard: "Equipped creature gets +1/+0 and has
/// haste." Shadowspear: "Equipped creature gets +1/+1 and has trample and
/// lifelink."
pub(super) fn equipment_for(name: &str) -> &'static str {
    match name {
        "Skateboard" => "Some(EquipmentDef { power_delta: 1, toughness_delta: 0, add_subtype: None, controller_turn_keywords: Keywords::HASTE, other_turn_keywords: Keywords::HASTE, noncreature_spell_damage_to_each_opponent: 0, job_select: false, granted_activated_ability: None, pt_controller_turn_only: false })",
        "Shadowspear" => "Some(EquipmentDef { power_delta: 1, toughness_delta: 1, add_subtype: None, controller_turn_keywords: Keywords(Keywords::TRAMPLE.0 | Keywords::LIFELINK.0), other_turn_keywords: Keywords(Keywords::TRAMPLE.0 | Keywords::LIFELINK.0), noncreature_spell_damage_to_each_opponent: 0, job_select: false, granted_activated_ability: None, pt_controller_turn_only: false })",
        _ => super::build_eternal_e8a_v1::equipment_for(name),
    }
}

/// Ward costs. Kappa Cannoneer: "Ward {4}". Patchwork Automaton: "Ward {2}".
pub(super) fn ward_cost_for(name: &str) -> &'static str {
    match name {
        "Kappa Cannoneer" => "Some(WardCostDef::Generic(4))",
        "Patchwork Automaton" => "Some(WardCostDef::Generic(2))",
        _ => "None",
    }
}

/// Lands whose only printed ability is a static the runtime applies (The
/// Tabernacle at Pendrell Vale's granted upkeep trigger).
pub(super) fn static_only_land(name: &str) -> bool {
    name == "The Tabernacle at Pendrell Vale" || super::build_eternal_e8a_v1::static_only_land(name)
}

/// The tokens keep their printed names.
pub(super) fn object_name_for(name: &str) -> Option<&'static str> {
    match name {
        "Marit Lage Token" => Some("Marit Lage"),
        _ => super::build_eternal_e7b_v1::object_name_for(name),
    }
}

/// Card-database tokens for the triggered abilities in
/// `eternal_legacy_e7a_v1::triggers_for`, its hooks and its statics.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Abhorrent Oculus" => {
            "additional_cost:exile_six_own_graveyard_cards;each_opponent_upkeep:manifest_dread"
        }
        "Animate Dead" => {
            "enchant_creature_card_in_graveyard;etb:return_enchanted_card_under_your_control_and_attach;leaves:controller_sacrifices_creature"
        }
        "Arena of Glory" => {
            "enters_tapped_unless_controls_mountain;mana_exert:pay_r_tap_exert_add_rr:creature_spell_paid_gains_haste"
        }
        "Archon of Cruelty" => {
            "etb_or_attacks:target_opponent:sacrifice_creature_or_planeswalker_discard_lose_3:you_draw_1_gain_3"
        }
        "Blast Zone" => {
            "enters_with_charge_counter;activated:x_x_tap:add_x_charge_counters:x_0_to_7;activated:destroy_nonland_mana_value_equals_charge"
        }
        "Burning Wish" => "spell:wish_sorcery_outside_game:nothing_outside_game_in_best_of_one;exile_self",
        "Cabal Therapy" => {
            "spell:name_nonland_card:target_player_reveals_hand_discards_all_named;flashback:sacrifice_creature"
        }
        "Chromatic Star" => "put_into_graveyard_from_battlefield:draw:1",
        "Curie, Emergent Intelligence" => {
            "combat_damage_player:draw_base_power;activated:become_copy_of_exiled_creature_keeping_trigger"
        }
        "Dark Depths" => {
            "enters_with_ice_counters:10;state:no_ice_counters:sacrifice_create_marit_lage"
        }
        "Dress Down" => {
            "etb:draw:1;static:creatures_lose_all_abilities;each_end_step:sacrifice_source"
        }
        "Echo of Eons" => "spell:each_player_shuffles_hand_and_graveyard_into_library_draws_7",
        "Emry, Lurker of the Loch" => "etb:mill:4;activated:may_cast_target_artifact_card_this_turn",
        "Exploration" => "static:additional_land_play",
        "Gaea's Will" => {
            "suspend:4:G;spell:until_end_of_turn_play_lands_and_cast_spells_from_graveyard;replacement:cards_to_graveyard_exiled_instead"
        }
        "Galvanic Relay" => "cast:storm;spell:exile_top_card:play_it_during_your_next_turn",
        "Gamble" => "spell:search_any_card_to_hand_hidden:discard_random:shuffle",
        "Goblin Engineer" => {
            "etb:may_search_artifact_to_graveyard_hidden:shuffle;activated:return_artifact_card_mana_value_at_most_3"
        }
        "Goblin Welder" => "activated:swap_artifact_with_artifact_card_in_its_controllers_graveyard",
        "Grafdigger's Cage" => {
            "static:creature_cards_in_graveyards_and_libraries_cant_enter;static:no_casting_from_graveyards_or_libraries"
        }
        "Icetill Explorer" => {
            "static:additional_land_play;static:play_lands_from_graveyard;landfall:mill:1"
        }
        "Kappa Cannoneer" => {
            "improvise;ward:4;this_or_another_controlled_artifact_enters:plus_one_counter_unblockable_eot"
        }
        "Legion Extruder" => "etb:any_target_damage:2",
        "Life from the Loam" => "spell:return_up_to_three_land_cards_to_hand;dredge:3:draw_step_draws",
        "Maze of Ith" => "activated:untap_attacking_creature_prevent_its_combat_damage_both_ways",
        "Mox Diamond" => "replacement:as_enters_discard_land_card_or_graveyard",
        "Patchwork Automaton" => "ward:2;controller_casts_artifact_spell:plus_one_counter",
        "Phyrexian Dragon Engine" => {
            "enters_from_your_graveyard:may_discard_hand_draw_3;unearth_next_end_step:exile"
        }
        "Phyrexian Dreadnought" => {
            "etb:sacrifice_unless_sacrifice_creatures_total_power_at_least:12"
        }
        "Pinnacle Emissary" => {
            "controller_casts_artifact_spell:create_drone;warp:{U/R};warp_next_end_step:exile"
        }
        "Reanimate" => "spell:target_creature_card_any_graveyard_to_battlefield:lose_life_mana_value",
        "Shadowspear" => "activated:opponent_permanents_lose_hexproof_and_indestructible_eot",
        "Shallow Grave" => {
            "spell:top_creature_card_own_graveyard_to_battlefield_haste:exile_next_end_step"
        }
        "Skateboard" => "etb:tap_target_permanent",
        "Song of Creation" => {
            "static:additional_land_play;controller_casts_spell:draw:2;controller_end_step:discard_hand"
        }
        "Sphere of Resistance" => "static:spells_cost_generic_more:1",
        "The Tabernacle at Pendrell Vale" => {
            "static:all_creatures_have_controller_upkeep:destroy_unless_pay_1"
        }
        "Thespian's Stage" => "activated:become_copy_of_target_land_keeping_this_ability",
        "Unmask" => "alt_cost:exile_black_card_from_hand;spell:reveal_hand_choose_nonland_discard",
        "Yavimaya, Cradle of Growth" => "static:each_land_is_a_forest",
        "Drone Token" => "static:can_block_only_flying",
        _ => super::build_eternal_e7b_v1::trigger_recipe_for(name),
    }
}
