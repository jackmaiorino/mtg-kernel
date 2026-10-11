//! Generated-definition tables for the MageZero Standard lands batch
//! (`docs/design/standard_lands_v1.md`). Every function is keyed by the
//! registry name and only cards in `data/standard/magezero_v1/cards_v1.json`
//! match, so Pauper and FDN definitions are unaffected.

use super::{AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe};

/// `{T}: Add {C}` plus a separately printed damage ability per color.
const PAINLANDS: [(&str, [&str; 2]); 10] = [
    ("Adarkar Wastes", ["W", "U"]),
    ("Battlefield Forge", ["R", "W"]),
    ("Brushland", ["G", "W"]),
    ("Caves of Koilos", ["W", "B"]),
    ("Karplusan Forest", ["R", "G"]),
    ("Llanowar Wastes", ["B", "G"]),
    ("Shivan Reef", ["U", "R"]),
    ("Sulfurous Springs", ["B", "R"]),
    ("Underground River", ["U", "B"]),
    ("Yavimaya Coast", ["G", "U"]),
];

/// (name, unconditional color, conditional color, either controlled subtype).
const VERGES: [(&str, &str, &str, [&str; 2]); 6] = [
    ("Floodfarm Verge", "W", "U", ["Plains", "Island"]),
    ("Gloomlake Verge", "U", "B", ["Island", "Swamp"]),
    ("Hushwood Verge", "G", "W", ["Forest", "Plains"]),
    ("Riverpyre Verge", "R", "U", ["Island", "Mountain"]),
    ("Thornspire Verge", "R", "G", ["Mountain", "Forest"]),
    ("Wastewood Verge", "G", "B", ["Swamp", "Forest"]),
];

const FASTLANDS: [&str; 9] = [
    "Blackcleave Cliffs",
    "Blooming Marsh",
    "Concealed Courtyard",
    "Copperline Gorge",
    "Darkslick Shores",
    "Inspiring Vantage",
    "Razorverge Thicket",
    "Seachrome Coast",
    "Spirebluff Canal",
];

const SLOWLANDS: [&str; 5] = [
    "Deserted Beach",
    "Dreamroot Cascade",
    "Haunted Ridge",
    "Overgrown Farmland",
    "Rockfall Vale",
];

const TRIOMES: [&str; 3] = [
    "Jetmir's Garden",
    "Spara's Headquarters",
    "Ziatora's Proving Ground",
];

const ANY_COLOR: &str = "&[ManaColor::W, ManaColor::U, ManaColor::B, ManaColor::R, ManaColor::G]";

fn painland(name: &str) -> Option<[&'static str; 2]> {
    PAINLANDS
        .iter()
        .find(|(land, _)| *land == name)
        .map(|(_, colors)| *colors)
}

fn verge(name: &str) -> Option<(&'static str, &'static str, [&'static str; 2])> {
    VERGES
        .iter()
        .find(|(land, ..)| *land == name)
        .map(|(_, primary, conditional, subtypes)| (*primary, *conditional, *subtypes))
}

/// Whether this card's primary (automatic-payment) mana ability differs from
/// its registry `produces_mana`.
pub fn overrides_primary_mana(name: &str) -> bool {
    painland(name).is_some()
        || verge(name).is_some()
        || matches!(name, "Mirrex" | "Rockface Village" | "Starting Town")
}

pub fn primary_mana_ability_colors(name: &str) -> Vec<&'static str> {
    if let Some((primary, ..)) = verge(name) {
        return vec![primary];
    }
    vec!["C"]
}

fn tap_for(colors: &str, cost: &str, controller_damage: u8) -> String {
    format!(
        "AdditionalManaAbilityDef {{ colors: {colors}, mana_cost: Cost {{ pips: &[], generic: 0, x_count: 0 }}, ability: ManaAbilityDef {{ cost: ManaAbilityCostDef::{cost}, amount: ManaAbilityAmountDef::Fixed(1), controller_damage: {controller_damage}, max_activations_per_turn: None }} }}"
    )
}

pub fn additional_mana_abilities(name: &str) -> Option<String> {
    if let Some(colors) = painland(name) {
        let abilities = colors
            .map(|color| tap_for(&format!("&[ManaColor::{color}]"), "TapSelf", 1))
            .join(", ");
        return Some(format!("&[{abilities}]"));
    }
    if let Some((_, conditional, _)) = verge(name) {
        return Some(format!(
            "&[{}]",
            tap_for(&format!("&[ManaColor::{conditional}]"), "TapSelf", 0)
        ));
    }
    match name {
        "Mirrex" => Some(format!("&[{}]", tap_for(ANY_COLOR, "TapSelf", 0))),
        "Starting Town" => Some(format!("&[{}]", tap_for(ANY_COLOR, "TapSelfPayLife(1)", 0))),
        _ => None,
    }
}

/// Generated source for the `CardDef` fields this batch appends.
pub struct StandardLandFields {
    pub enters_tapped_unless_controller: String,
    pub additional_mana_ability_conditions: String,
    pub restricted_mana_abilities: String,
    pub animation: String,
    pub activated_ability_generic_reductions: String,
}

impl StandardLandFields {
    fn empty() -> StandardLandFields {
        StandardLandFields {
            enters_tapped_unless_controller: "None".to_string(),
            additional_mana_ability_conditions: "&[]".to_string(),
            restricted_mana_abilities: "&[]".to_string(),
            animation: "None".to_string(),
            activated_ability_generic_reductions: "&[]".to_string(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.canonical_token() == StandardLandFields::empty().canonical_token()
    }

    /// The exact generated source of every appended field, in field order.
    pub fn canonical_token(&self) -> String {
        format!(
            "enters_tapped_unless_controller={};additional_mana_ability_conditions={};restricted_mana_abilities={};animation={};activated_ability_generic_reductions={}",
            self.enters_tapped_unless_controller,
            self.additional_mana_ability_conditions,
            self.restricted_mana_abilities,
            self.animation,
            self.activated_ability_generic_reductions
        )
    }
}

pub fn fields(name: &str, executable: bool) -> StandardLandFields {
    let mut fields = StandardLandFields::empty();
    if !executable {
        return fields;
    }
    if FASTLANDS.contains(&name) {
        fields.enters_tapped_unless_controller =
            "Some(EntersTappedUnlessControllerDef::ControlsAtMostOtherLands(2))".to_string();
    }
    if SLOWLANDS.contains(&name) {
        fields.enters_tapped_unless_controller =
            "Some(EntersTappedUnlessControllerDef::ControlsAtLeastOtherLands(2))".to_string();
    }
    if let Some((_, _, [first, second])) = verge(name) {
        fields.additional_mana_ability_conditions = format!(
            "&[Some(ManaAbilityConditionDef::ControllerControlsPermanentWithEitherSubtype {{ first: Subtype::{first}, second: Subtype::{second} }})]"
        );
    }
    match name {
        "Starting Town" => {
            fields.enters_tapped_unless_controller =
                "Some(EntersTappedUnlessControllerDef::WithinOwnFirstTurns(3))".to_string();
        }
        "Mirrex" => {
            fields.additional_mana_ability_conditions =
                "&[Some(ManaAbilityConditionDef::SourceEnteredThisTurn)]".to_string();
        }
        "Rockface Village" => {
            fields.restricted_mana_abilities = "&[RestrictedManaAbilityDef { colors: &[ManaColor::R], restriction: ManaSpendRestrictionDef::CreatureSpell }]".to_string();
        }
        "Mishra's Foundry" => {
            fields.animation = "Some(AnimationDef { until_end_of_turn: true, power: 2, toughness: 2, artifact: true, colors: &[], subtypes: &[Subtype::AssemblyWorker], keywords: Keywords::NONE })".to_string();
        }
        "Restless Bivouac" => {
            fields.animation = "Some(AnimationDef { until_end_of_turn: true, power: 2, toughness: 2, artifact: false, colors: &[ManaColor::R, ManaColor::W], subtypes: &[Subtype::Ox], keywords: Keywords::NONE })".to_string();
        }
        "Restless Cottage" => {
            fields.animation = "Some(AnimationDef { until_end_of_turn: true, power: 4, toughness: 4, artifact: false, colors: &[ManaColor::B, ManaColor::G], subtypes: &[Subtype::Horror], keywords: Keywords::NONE })".to_string();
        }
        "Restless Fortress" => {
            fields.animation = "Some(AnimationDef { until_end_of_turn: true, power: 1, toughness: 4, artifact: false, colors: &[ManaColor::W, ManaColor::B], subtypes: &[Subtype::Nightmare], keywords: Keywords::NONE })".to_string();
        }
        "Restless Reef" => {
            fields.animation = "Some(AnimationDef { until_end_of_turn: true, power: 4, toughness: 4, artifact: false, colors: &[ManaColor::U, ManaColor::B], subtypes: &[Subtype::Shark], keywords: Keywords::DEATHTOUCH })".to_string();
        }
        "Restless Ridgeline" => {
            fields.animation = "Some(AnimationDef { until_end_of_turn: true, power: 3, toughness: 4, artifact: false, colors: &[ManaColor::R, ManaColor::G], subtypes: &[Subtype::Dinosaur], keywords: Keywords::NONE })".to_string();
        }
        "Restless Prairie" => {
            fields.animation = "Some(AnimationDef { until_end_of_turn: true, power: 3, toughness: 3, artifact: false, colors: &[ManaColor::G, ManaColor::W], subtypes: &[Subtype::Llama], keywords: Keywords::NONE })".to_string();
        }
        "Restless Vinestalk" => {
            fields.animation = "Some(AnimationDef { until_end_of_turn: true, power: 5, toughness: 5, artifact: false, colors: &[ManaColor::G, ManaColor::U], subtypes: &[Subtype::Plant], keywords: Keywords::TRAMPLE })".to_string();
        }
        "Soulstone Sanctuary" => {
            fields.animation = "Some(AnimationDef { until_end_of_turn: false, power: 3, toughness: 3, artifact: false, colors: &[], subtypes: Subtype::CREATURE_TYPES, keywords: Keywords::VIGILANCE })".to_string();
        }
        "Eiganjo, Seat of the Empire" | "Otawara, Soaring City" => {
            fields.activated_ability_generic_reductions = "&[ActivatedAbilityGenericReductionDef { ability_index: 0, per: ActivatedAbilityReductionCountDef::ControlledLegendaryCreatures }]".to_string();
        }
        _ => {}
    }
    fields
}

pub fn activated_ability_recipes(name: &str) -> Option<&'static [ActivatedAbilityRecipe]> {
    if TRIOMES.contains(&name) {
        return Some(&[ActivatedAbilityRecipe {
            cost: &[
                AbilityCostRecipe::ManaCost("{3}"),
                AbilityCostRecipe::DiscardSelf,
            ],
            effect: AbilityEffectRecipe::DrawCards(1),
            activation_zone: "Hand",
            sorcery_speed_only: false,
            target_spec: "None",
            activation_target_filter: "TargetSpecOnly",
            max_activations_per_turn: None,
        }]);
    }
    match name {
        "Restless Bivouac" => Some(&[ActivatedAbilityRecipe {
            cost: &[AbilityCostRecipe::ManaCost("{1}{R}{W}")],
            effect: AbilityEffectRecipe::AnimateSource,
            activation_zone: "Battlefield",
            sorcery_speed_only: false,
            target_spec: "None",
            activation_target_filter: "TargetSpecOnly",
            max_activations_per_turn: None,
        }]),
        "Restless Cottage" => Some(&[ActivatedAbilityRecipe {
            cost: &[AbilityCostRecipe::ManaCost("{2}{B}{G}")],
            effect: AbilityEffectRecipe::AnimateSource,
            activation_zone: "Battlefield",
            sorcery_speed_only: false,
            target_spec: "None",
            activation_target_filter: "TargetSpecOnly",
            max_activations_per_turn: None,
        }]),
        "Restless Fortress" => Some(&[ActivatedAbilityRecipe {
            cost: &[AbilityCostRecipe::ManaCost("{2}{W}{B}")],
            effect: AbilityEffectRecipe::AnimateSource,
            activation_zone: "Battlefield",
            sorcery_speed_only: false,
            target_spec: "None",
            activation_target_filter: "TargetSpecOnly",
            max_activations_per_turn: None,
        }]),
        "Restless Reef" => Some(&[ActivatedAbilityRecipe {
            cost: &[AbilityCostRecipe::ManaCost("{2}{U}{B}")],
            effect: AbilityEffectRecipe::AnimateSource,
            activation_zone: "Battlefield",
            sorcery_speed_only: false,
            target_spec: "None",
            activation_target_filter: "TargetSpecOnly",
            max_activations_per_turn: None,
        }]),
        "Restless Ridgeline" => Some(&[ActivatedAbilityRecipe {
            cost: &[AbilityCostRecipe::ManaCost("{2}{R}{G}")],
            effect: AbilityEffectRecipe::AnimateSource,
            activation_zone: "Battlefield",
            sorcery_speed_only: false,
            target_spec: "None",
            activation_target_filter: "TargetSpecOnly",
            max_activations_per_turn: None,
        }]),
        "Restless Prairie" => Some(&[ActivatedAbilityRecipe {
            cost: &[AbilityCostRecipe::ManaCost("{2}{G}{W}")],
            effect: AbilityEffectRecipe::AnimateSource,
            activation_zone: "Battlefield",
            sorcery_speed_only: false,
            target_spec: "None",
            activation_target_filter: "TargetSpecOnly",
            max_activations_per_turn: None,
        }]),
        "Restless Vinestalk" => Some(&[ActivatedAbilityRecipe {
            cost: &[AbilityCostRecipe::ManaCost("{3}{G}{U}")],
            effect: AbilityEffectRecipe::AnimateSource,
            activation_zone: "Battlefield",
            sorcery_speed_only: false,
            target_spec: "None",
            activation_target_filter: "TargetSpecOnly",
            max_activations_per_turn: None,
        }]),
        "Soulstone Sanctuary" => Some(&[ActivatedAbilityRecipe {
            cost: &[AbilityCostRecipe::ManaCost("{4}")],
            effect: AbilityEffectRecipe::AnimateSource,
            activation_zone: "Battlefield",
            sorcery_speed_only: false,
            target_spec: "None",
            activation_target_filter: "TargetSpecOnly",
            max_activations_per_turn: None,
        }]),
        "Otawara, Soaring City" => Some(&[ActivatedAbilityRecipe {
            cost: &[AbilityCostRecipe::ManaCost("{3}{U}"), AbilityCostRecipe::DiscardSelf],
            effect: AbilityEffectRecipe::MoveAllTargetsToHand,
            activation_zone: "Hand",
            sorcery_speed_only: false,
            target_spec: "ArtifactCreatureEnchantmentOrPlaneswalker",
            activation_target_filter: "TargetSpecOnly",
            max_activations_per_turn: None,
        }]),
        "Mishra's Foundry" => Some(&[
            ActivatedAbilityRecipe {
                cost: &[AbilityCostRecipe::Mana {
                    colored: None,
                    generic: 2,
                }],
                effect: AbilityEffectRecipe::AnimateSource,
                activation_zone: "Battlefield",
                sorcery_speed_only: false,
                target_spec: "None",
                activation_target_filter: "TargetSpecOnly",
                max_activations_per_turn: None,
            },
            ActivatedAbilityRecipe {
                cost: &[
                    AbilityCostRecipe::Mana {
                        colored: None,
                        generic: 1,
                    },
                    AbilityCostRecipe::Tap,
                ],
                effect: AbilityEffectRecipe::PumpTargetUntilEndOfTurn {
                    power: 2,
                    toughness: 2,
                },
                activation_zone: "Battlefield",
                sorcery_speed_only: false,
                target_spec: "AttackingCreatureWithSubtype(Subtype::AssemblyWorker)",
                activation_target_filter: "TargetSpecOnly",
                max_activations_per_turn: None,
            },
        ]),
        "Eiganjo, Seat of the Empire" => Some(&[ActivatedAbilityRecipe {
            cost: &[
                AbilityCostRecipe::ManaCost("{2}{W}"),
                AbilityCostRecipe::DiscardSelf,
            ],
            effect: AbilityEffectRecipe::DamageTarget(4),
            activation_zone: "Hand",
            sorcery_speed_only: false,
            target_spec: "AttackingOrBlockingCreature",
            activation_target_filter: "TargetSpecOnly",
            max_activations_per_turn: None,
        }]),
        "Mirrex" => Some(&[ActivatedAbilityRecipe {
            cost: &[
                AbilityCostRecipe::Mana {
                    colored: None,
                    generic: 3,
                },
                AbilityCostRecipe::Tap,
            ],
            effect: AbilityEffectRecipe::CreateToken("Phyrexian Mite Token"),
            activation_zone: "Battlefield",
            sorcery_speed_only: false,
            target_spec: "None",
            activation_target_filter: "TargetSpecOnly",
            max_activations_per_turn: None,
        }]),
        "Rockface Village" => Some(&[ActivatedAbilityRecipe {
            cost: &[
                AbilityCostRecipe::Mana {
                    colored: Some("R"),
                    generic: 0,
                },
                AbilityCostRecipe::Tap,
            ],
            effect: AbilityEffectRecipe::PumpTargetAndGrantKeyword {
                power: 1,
                toughness: 0,
                keyword: "HASTE",
            },
            activation_zone: "Battlefield",
            sorcery_speed_only: true,
            target_spec: "ControlledPermanentWithAnySubtype([Subtype::Lizard, Subtype::Mouse, Subtype::Otter, Subtype::Raccoon])",
            activation_target_filter: "TargetSpecOnly",
            max_activations_per_turn: None,
        }]),
        _ => None,
    }
}
