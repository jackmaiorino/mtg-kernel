//! Card-database recipes for the eternal catalog's Vintage Cube wave C1c
//! (red and multicolor instants and sorceries). Wave C1b's tables fall through to these, and
//! this module is the tail of the recipe chains: its own
//! defaults are the chains' base values. Runtime programs live in
//! `src/eternal_cube_c1c_v1.rs`. Every row is keyed by a name that occurs
//! only in the eternal registry.

use super::{AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special};

/// Delayed Blast Fireball: "Foretell {4}{R}{R} (During your turn, you may
/// pay {2} and exile this card from your hand face down. Cast it on a later
/// turn for its foretell cost.)" Foretelling is a special action (702.143a):
/// the engine performs it from the hand without the stack, as Behold the
/// Multiverse's (`eternal_pauper_e6a_v1::foretell`), offered as the card's
/// ability 0. Its foretell cost is `alt_cost_for`.
const DELAYED_BLAST_FIREBALL: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[AbilityCostRecipe::ManaCost("{2}")],
    effect: AbilityEffectRecipe::StandardProgram("crate::eternal_pauper_e6a_v1::foretell_action"),
    activation_zone: "Hand",
    sorcery_speed_only: false,
    target_spec: "None",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Delayed Blast Fireball" => &DELAYED_BLAST_FIREBALL,
        _ => &[],
    }
}

/// A spell whose program is a runtime function of `eternal_cube_c1c_v1`.
const fn spell(target_spec: &'static str, program: &'static str) -> Special {
    Special::StandardProgram {
        target_spec,
        program,
    }
}

/// The target filter of Kolaghan's Command's pair of modes `first` and
/// `second` (`eternal_cube_c1c_v1::targets`), as source text.
macro_rules! pair_spec {
    ($first:literal, $second:literal) => {
        concat!(
            "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::C1cV1(crate::eternal_cube_c1c_v1::targets::C1cTargetV1::KolaghanPairV1(crate::eternal_cube_c1c_v1::targets::KolaghanModeV1::",
            $first,
            ", crate::eternal_cube_c1c_v1::targets::KolaghanModeV1::",
            $second,
            ")))"
        )
    };
}

/// A target filter of `eternal_cube_c1c_v1::targets`, as source text.
macro_rules! c1c_spec {
    ($filter:literal) => {
        concat!(
            "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::C1cV1(crate::eternal_cube_c1c_v1::targets::C1cTargetV1::",
            $filter,
            "))"
        )
    };
}

/// Spell programs. Kolaghan's Command's six pairs of modes are modes 0 to 5
/// (`mode2_for`, `mode3_for`, `additional_modes_for`).
pub(super) fn special_for(name: &str) -> Option<Special> {
    Some(match name {
        // Dead; Gone is the split card's second half (`adventure_for`).
        "Dead // Gone" => spell("TargetSpec::Creature", "crate::eternal_cube_c1c_v1::dead"),
        // Its foretell is the hand ability and `alt_cost_for`.
        "Delayed Blast Fireball" => spell(
            "TargetSpec::None",
            "crate::eternal_cube_c1c_v1::delayed_blast_fireball",
        ),
        "Escape to the Wilds" => spell(
            "TargetSpec::None",
            "crate::eternal_cube_c1c_v1::escape_to_the_wilds",
        ),
        "Expressive Iteration" => spell(
            "TargetSpec::None",
            "crate::eternal_cube_c1c_v1::expressive_iteration",
        ),
        // "Pay X life" is `eternal_cube_c1c_v1::pays_life_for_x`; the
        // division is announced with the targets.
        "Fire Covenant" => spell(
            c1c_spec!("CreaturePicksPerLifeV1"),
            "crate::eternal_cube_c1c_v1::fire_covenant",
        ),
        // Snapcaster Mage's grant, as a spell.
        "Flashback" => spell(
            "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::InstantOrSorceryCardInOwnGraveyard)",
            "crate::eternal_cube_c1c_v1::flashback",
        ),
        "Flame Slash" => spell(
            "TargetSpec::Creature",
            "crate::eternal_cube_c1c_v1::flame_slash",
        ),
        "Impractical Joke" => spell(
            c1c_spec!("UpToOneCreatureOrPlaneswalkerV1"),
            "crate::eternal_cube_c1c_v1::impractical_joke",
        ),
        "Kolaghan's Command" => spell(
            pair_spec!("ReturnCreatureCardV1", "DiscardV1"),
            "crate::eternal_cube_c1c_v1::kolaghans_command_return_discard",
        ),
        "Lingering Souls" => spell(
            "TargetSpec::None",
            "crate::eternal_cube_c1c_v1::lingering_souls",
        ),
        // Mana Leak's program: the countered spell's controller may pay {3}.
        // Where the spell goes is `eternal_cube_c1c_v1::counter_destination`.
        "No More Lies" => Special::CounterUnlessPaysGeneric {
            filter: super::StackSpellFilter::Any,
            generic: 3,
        },
        "Mine Collapse" => spell(
            "TargetSpec::CreatureOrPlaneswalker",
            "crate::eternal_cube_c1c_v1::mine_collapse",
        ),
        // Its plot cost is `build.rs`'s `plot_cost_for`.
        "Pillage the Bog" => spell(
            "TargetSpec::None",
            "crate::eternal_cube_c1c_v1::pillage_the_bog",
        ),
        "Sear" => spell(
            "TargetSpec::CreatureOrPlaneswalker",
            "crate::eternal_cube_c1c_v1::sear",
        ),
        // Shock's shape: damage to any target.
        "Tarfire" => Special::BurnAnyTarget(2),
        "Traumatic Critique" => spell(
            "TargetSpec::AnyTarget",
            "crate::eternal_cube_c1c_v1::traumatic_critique",
        ),
        "Vicious Rivalry" => spell(
            "TargetSpec::None",
            "crate::eternal_cube_c1c_v1::vicious_rivalry",
        ),
        "Wheel of Fortune" => spell(
            "TargetSpec::None",
            "crate::eternal_cube_c1c_v1::wheel_of_fortune",
        ),
        _ => return None,
    })
}

/// Kolaghan's Command's second pair: return a creature card and destroy an
/// artifact.
pub(super) fn mode2_for(name: &str) -> &'static str {
    match name {
        "Kolaghan's Command" => concat!(
            "Some(ModeDef { target_spec: ",
            pair_spec!("ReturnCreatureCardV1", "DestroyArtifactV1"),
            ", effect: crate::eternal_cube_c1c_v1::kolaghans_command_return_destroy })"
        ),
        _ => "None",
    }
}

/// Kolaghan's Command's third pair: return a creature card and 2 damage.
pub(super) fn mode3_for(name: &str) -> &'static str {
    match name {
        "Kolaghan's Command" => concat!(
            "Some(ModeDef { target_spec: ",
            pair_spec!("ReturnCreatureCardV1", "DamageV1"),
            ", effect: crate::eternal_cube_c1c_v1::kolaghans_command_return_damage })"
        ),
        _ => "None",
    }
}

/// Kolaghan's Command's other three pairs: discard and destroy, discard and
/// damage, destroy and damage.
pub(super) fn additional_modes_for(name: &str) -> &'static str {
    match name {
        "Kolaghan's Command" => concat!("&[ModeDef { target_spec: ", pair_spec!("DiscardV1", "DestroyArtifactV1"), ", effect: crate::eternal_cube_c1c_v1::kolaghans_command_discard_destroy }, ModeDef { target_spec: ", pair_spec!("DiscardV1", "DamageV1"), ", effect: crate::eternal_cube_c1c_v1::kolaghans_command_discard_damage }, ModeDef { target_spec: ", pair_spec!("DestroyArtifactV1", "DamageV1"), ", effect: crate::eternal_cube_c1c_v1::kolaghans_command_destroy_damage }]"),
        _ => "&[]",
    }
}

/// No C1c card has kicker yet.
pub(super) fn kicker_cost_for(_name: &str) -> Option<&'static str> {
    None
}

/// No C1c card is an Aura or an Equipment yet.
pub(super) fn attachment_for(_name: &str) -> &'static str {
    "None"
}

/// Lingering Souls: "Flashback {1}{B}".
pub(super) fn flashback_for(name: &str) -> String {
    match name {
        "Lingering Souls" => {
            let (pips, generic, x_count) = super::parse_cost("{1}{B}");
            format!(
                "Some(FlashbackDef {{ cost: &[CostComponent::Mana(Cost {{ pips: &[{}], generic: {generic}, x_count: {x_count} }})] }})",
                pips.join(", ")
            )
        }
        _ => "None".to_string(),
    }
}

/// Dead // Gone's second half, Gone {2}{R}: "Return target creature you
/// don't control to its owner's hand." Split cards (708) reuse the Adventure
/// form, as Fire // Ice does (`build_eternal_p1a_v1::adventure_for`).
pub(super) fn adventure_for(name: &str) -> &'static str {
    match name {
        "Dead // Gone" => "Some(AdventureDef { name: \"Gone\", cost: Cost { pips: &[Pip::Colored(ManaColor::R)], generic: 2, x_count: 0 }, types: &[CardType::Instant], target_spec: TargetSpec::OpponentControlledCreature, effect: ability_effect_program_eternal_cube_c1c_v1_gone })",
        _ => "None",
    }
}

pub(super) fn adventure_effect_recipe_for(name: &str) -> Option<AbilityEffectRecipe> {
    match name {
        "Dead // Gone" => Some(AbilityEffectRecipe::StandardProgram(
            "crate::eternal_cube_c1c_v1::gone",
        )),
        _ => None,
    }
}

pub(super) fn adventure_face_name_for(name: &str) -> Option<&'static str> {
    match name {
        "Dead // Gone" => Some("Gone"),
        _ => None,
    }
}

/// Mine Collapse: "If it's your turn, you may sacrifice a Mountain rather
/// than pay this spell's mana cost." Delayed Blast Fireball: "Foretell
/// {4}{R}{R}", payable only for the card foretold from exile.
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    match name {
        "Delayed Blast Fireball" => "Some(AltCostDef { components: &[CostComponent::Mana(Cost { pips: &[Pip::Colored(ManaColor::R), Pip::Colored(ManaColor::R)], generic: 4, x_count: 0 })], condition: AltCostCondition::ForetoldV1 })",
        "Mine Collapse" => "Some(AltCostDef { components: &[CostComponent::SacrificeControlled { count: 1, filter: PermanentFilter::WithSubtypeV1(Subtype::Mountain) }], condition: AltCostCondition::YourTurnV1 })",
        _ => "None",
    }
}

/// Printed evergreen keywords, in printed order.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "White Flying Spirit Token" => &["Keywords::FLYING"],
        _ => &[],
    }
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Dead // Gone" => {
            "split:dead:damage_target_creature:2;gone:return_target_creature_you_dont_control_to_owners_hand"
        }
        "Delayed Blast Fireball" => {
            "spell:damage_each_opponent_and_their_creatures:2:5_if_cast_from_exile;foretell:{4}{R}{R}"
        }
        "Escape to the Wilds" => {
            "spell:exile_top_5:may_play_until_end_of_your_next_turn;additional_land_this_turn"
        }
        "Expressive Iteration" => {
            "spell:look_top_3:one_to_hand:one_to_bottom:one_exiled_may_play_this_turn"
        }
        "Fire Covenant" => {
            "additional_cost:pay_x_life;spell:damage_x_divided_among_any_number_of_target_creatures"
        }
        "Flame Slash" => "spell:damage_target_creature:4",
        "Flashback" => {
            "spell:target_instant_or_sorcery_card_in_own_graveyard:gains_flashback_equal_to_mana_cost_until_end_of_turn"
        }
        "Impractical Joke" => {
            "spell:damage_cant_be_prevented_this_turn;damage_up_to_one_target_creature_or_planeswalker:3"
        }
        "Kolaghan's Command" => {
            "modes:choose_two:return_target_creature_card_from_your_graveyard_to_hand,target_player_discards_1,destroy_target_artifact,damage_any_target:2"
        }
        "Lingering Souls" => "spell:create_2_tokens:White Flying Spirit Token;flashback:{1}{B}",
        "Mine Collapse" => {
            "alt_cost:if_your_turn:sacrifice_a_mountain;spell:damage_target_creature_or_planeswalker:5"
        }
        "No More Lies" => {
            "spell:counter_target_spell_unless_controller_pays_3;if_countered:exile_instead_of_graveyard"
        }
        "Pillage the Bog" => {
            "spell:look_top_twice_lands:one_to_hand:rest_bottom_random;plot:{1}{B}{G}"
        }
        "Sear" => "spell:damage_target_creature_or_planeswalker:4",
        "Tarfire" => "spell:damage_any_target:2",
        "Traumatic Critique" => "spell:damage_any_target_x;draw_2_then_discard_1",
        "Vicious Rivalry" => {
            "additional_cost:pay_x_life;spell:destroy_all_artifacts_and_creatures_mana_value_x_or_less"
        }
        "Wheel of Fortune" => "spell:each_player_discards_hand_then_draws_7",
        _ => "none",
    }
}
