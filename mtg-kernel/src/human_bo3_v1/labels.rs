//! Human labels from validated visible facts and pinned definition metadata.
//! Unsupported meanings reject the complete prompt. Ordinals and Debug text
//! are never substituted for an ability, spell mode, or effect description.

use super::visible::Handles;
use super::HumanDecisionErrorV1 as Error;
use crate::card_def::{self, CostComponent, ManaAbilityAmountDef, ManaAbilityCostDef};
use crate::engine::{CastMode, ChosenCreatureCostZoneV1, CostKind, OptionalCostChoice};
use crate::mana::{Cost, ManaColor, Pip};
use crate::policy_observation_v6::ObservationV6;
use crate::rl::{ActionSemanticV1 as A, CardStableRefV1, PlayerSeatV1, TargetRefV1};

fn color(color: ManaColor) -> &'static str {
    match color {
        ManaColor::W => "W",
        ManaColor::U => "U",
        ManaColor::B => "B",
        ManaColor::R => "R",
        ManaColor::G => "G",
        ManaColor::C => "C",
    }
}

fn mana(cost: Cost) -> String {
    let mut text = String::new();
    for _ in 0..cost.x_count {
        text.push_str("{X}");
    }
    if cost.generic != 0 {
        text.push_str(&format!("{{{}}}", cost.generic));
    }
    for pip in cost.pips {
        text.push_str(&match pip {
            Pip::Colored(c) => format!("{{{}}}", color(*c)),
            Pip::Hybrid(a, b) => format!("{{{}/{}}}", color(*a), color(*b)),
            Pip::Phyrexian(c) => format!("{{{}/P}}", color(*c)),
        });
    }
    if text.is_empty() {
        "{0}".into()
    } else {
        text
    }
}

fn definition(source: &CardStableRefV1) -> Result<&'static card_def::CardDef, Error> {
    card_def::CARD_DEFS
        .get(source.card_db_id as usize)
        .ok_or(Error::InvalidVisibleReference)
}

fn cost_components(components: &[CostComponent]) -> Result<String, Error> {
    let mut parts = Vec::new();
    for component in components {
        parts.push(match component {
            CostComponent::Mana(cost) => format!("pay {}", mana(*cost)),
            CostComponent::PayLife(n) => format!("pay {n} life"),
            CostComponent::SacrificeLands(n) => format!("sacrifice {n} lands"),
            CostComponent::DiscardCards(n) => format!("discard {n} cards"),
            CostComponent::ExileOtherCardsFromOwnGraveyard(n) => {
                format!("exile {n} other cards from your graveyard")
            }
            // Filtered sacrifices and alternate costs need their exact
            // definition constraints. Never call every land a legal Mountain.
            _ => return Err(Error::UnsupportedPrompt),
        });
    }
    if parts.is_empty() {
        return Err(Error::UnsupportedPrompt);
    }
    Ok(parts.join("; "))
}

fn target(target: &TargetRefV1, handles: &Handles, human: PlayerSeatV1) -> Result<String, Error> {
    match target {
        TargetRefV1::Player { player } => Ok(if *player == human {
            "you"
        } else {
            "your opponent"
        }
        .into()),
        TargetRefV1::Object { object } => handles.name(object),
    }
}

fn names(cards: &[CardStableRefV1], handles: &Handles) -> Result<String, Error> {
    if cards.is_empty() {
        return Ok("none".into());
    }
    cards
        .iter()
        .map(|card| handles.name(card))
        .collect::<Result<Vec<_>, _>>()
        .map(|names| names.join(", "))
}

fn optional_source(observation: &ObservationV6) -> Result<&CardStableRefV1, Error> {
    observation
        .projection
        .surface
        .engine_context
        .pending_optional_cost
        .as_ref()
        .and_then(|pending| pending.source.as_ref())
        .ok_or(Error::UnsupportedPrompt)
}

fn actor(action: &A) -> Option<PlayerSeatV1> {
    match action {
        A::Pass { actor }
        | A::PlayLand { actor, .. }
        | A::CastSpell { actor, .. }
        | A::ActivateManaAbility { actor, .. }
        | A::ActivateAbility { actor, .. }
        | A::PlotSpell { actor, .. }
        | A::ChooseTarget { actor, .. }
        | A::ChooseCostTarget { actor, .. }
        | A::ChooseCastMode { actor, .. }
        | A::ChooseKicker { actor, .. }
        | A::ChooseSpellMode { actor, .. }
        | A::ChooseEffectOption { actor, .. }
        | A::ChooseEffectTarget { actor, .. }
        | A::FinishEffectSelection { actor, .. }
        | A::ChooseEffectColor { actor, .. }
        | A::ChooseEffectNumber { actor, .. }
        | A::ChooseEffectBoolean { actor, .. }
        | A::FinishTargetSelection { actor, .. }
        | A::ChooseOptionalCostUse { actor, .. }
        | A::ChooseOptionalCostWhich { actor, .. }
        | A::ChooseSpellCopyPayment { actor, .. }
        | A::ChooseSpellCopyRetarget { actor, .. }
        | A::ChooseMadnessCast { actor, .. }
        | A::Discard { actor, .. }
        | A::DeclareAttackers { actor, .. }
        | A::DeclareBlockersForAttacker { actor, .. }
        | A::ChooseAttackerInclusion { actor, .. }
        | A::ChooseBlockerInclusion { actor, .. }
        | A::OrderTriggers { actor, .. } => Some(*actor),
        A::Ambiguous { .. } => None,
    }
}

pub(super) fn label(
    action: &A,
    observation: &ObservationV6,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<String, Error> {
    if actor(action) != Some(human) {
        return Err(Error::UnsupportedPrompt);
    }
    Ok(match action {
        A::Pass { .. } => "Pass priority".into(),
        A::PlayLand { source, .. } => {
            format!("Play {} as your land for the turn", handles.name(source)?)
        }
        A::CastSpell { source, .. } => format!("Begin casting {}", handles.name(source)?),
        A::PlotSpell { source, .. } => {
            let cost = definition(source)?
                .plot_cost
                .ok_or(Error::UnsupportedPrompt)?;
            format!(
                "Plot {} for {} (exile it to cast on a later turn)",
                handles.name(source)?,
                mana(cost)
            )
        }
        A::ActivateManaAbility {
            source,
            mana_choice,
            cost_target,
            ..
        } => {
            let def = definition(source)?;
            let choice = mana_choice
                .or_else(|| {
                    (def.mana_ability_choices.len() == 1).then(|| def.mana_ability_choices[0])
                })
                .ok_or(Error::UnsupportedPrompt)?;
            let chosen_color = observation
                .projection
                .surface
                .battlefield
                .iter()
                .flatten()
                .find(|card| card.stable == *source)
                .and_then(|card| card.chosen_color);
            let primary = def
                .primary_mana_ability_choices(chosen_color)
                .contains(&choice);
            let additional = if primary {
                None
            } else {
                def.additional_mana_abilities
                    .iter()
                    .find(|ability| ability.colors.contains(&choice))
            };
            if !primary && additional.is_none() {
                return Err(Error::UnsupportedPrompt);
            }
            let (ability, extra_cost) = match additional {
                Some(additional) => (Some(additional.ability), Some(additional.mana_cost)),
                None => (def.mana_ability_def, None),
            };
            let source_name = handles.name(source)?;
            let (mut costs, amount, damage) = match ability {
                None => {
                    if cost_target.is_some() {
                        return Err(Error::UnsupportedPrompt);
                    }
                    (format!("Tap {source_name}"), 1, 0)
                }
                Some(ability) => {
                    let ManaAbilityAmountDef::Fixed(amount) = ability.amount else {
                        return Err(Error::UnsupportedPrompt);
                    };
                    let cost = match ability.cost {
                        ManaAbilityCostDef::TapSelf => format!("Tap {source_name}"),
                        ManaAbilityCostDef::SacrificeSelf => format!("Sacrifice {source_name}"),
                        ManaAbilityCostDef::TapAndSacrificeSelf => {
                            format!("Tap and sacrifice {source_name}")
                        }
                        ManaAbilityCostDef::PutMinus0Minus1CounterOnSelf => {
                            format!("Put a -0/-1 counter on {source_name}")
                        }
                        ManaAbilityCostDef::TapSelfAndOtherUntappedControlledCreature => {
                            format!(
                                "Tap {source_name} and {}",
                                handles
                                    .name(cost_target.as_ref().ok_or(Error::UnsupportedPrompt)?)?
                            )
                        }
                    };
                    if !matches!(
                        ability.cost,
                        ManaAbilityCostDef::TapSelfAndOtherUntappedControlledCreature
                    ) && cost_target.is_some()
                    {
                        return Err(Error::UnsupportedPrompt);
                    }
                    (cost, amount, ability.controller_damage)
                }
            };
            if let Some(extra) = extra_cost {
                costs.push_str(&format!("; pay {}", mana(extra)));
            }
            let suffix = if damage == 0 {
                String::new()
            } else {
                format!("; it deals {damage} damage to you")
            };
            format!("{costs}: add {amount} {{{}}}{suffix}", color(choice))
        }
        A::ChooseTarget {
            source,
            remaining,
            target: chosen,
            ..
        } => {
            format!(
                "Target {} with {} ({remaining} target choices remaining)",
                target(chosen, handles, human)?,
                handles.name(source)?
            )
        }
        A::ChooseCostTarget {
            source,
            cost_kind,
            remaining,
            candidate,
            ..
        } => {
            let verb = match cost_kind {
                CostKind::SacrificeLands
                | CostKind::SacrificePermanents
                | CostKind::SacrificeCreatures
                | CostKind::SacrificeArtifacts => "Sacrifice",
                CostKind::DiscardCards => "Discard",
                CostKind::ExileFromGraveyard => "Exile from your graveyard",
                CostKind::TapPermanents => "Tap",
                CostKind::ReturnPermanentsToHand => "Return to its owner's hand",
                CostKind::ChooseCreatureOrRevealCreature => {
                    match observation
                        .extensions
                        .pending_chosen_creature_cost
                        .as_ref()
                        .ok_or(Error::UnsupportedPrompt)?
                        .selected_zone
                    {
                        ChosenCreatureCostZoneV1::Battlefield => "Choose the creature",
                        ChosenCreatureCostZoneV1::Hand => "Reveal the creature card",
                    }
                }
                CostKind::PayLife | CostKind::RemoveCounters | CostKind::PutCounters => {
                    return Err(Error::UnsupportedPrompt)
                }
            };
            format!(
                "{verb} {} to pay for {} ({remaining} choices remaining)",
                handles.name(candidate)?,
                handles.name(source)?
            )
        }
        A::ChooseCastMode { source, mode, .. } => {
            let def = definition(source)?;
            let cost = match mode {
                CastMode::Normal => format!("pay the printed mana cost {}", mana(def.cost)),
                CastMode::Alternative => cost_components(
                    def.alt_cost
                        .as_ref()
                        .ok_or(Error::UnsupportedPrompt)?
                        .components,
                )?,
            };
            format!("For {}, {cost}", handles.name(source)?)
        }
        A::ChooseKicker { source, pay, .. } => {
            let cost = definition(source)?
                .kicker_cost
                .ok_or(Error::UnsupportedPrompt)?;
            format!(
                "{} the additional kicker cost {} for {}",
                if *pay { "Pay" } else { "Decline" },
                mana(cost),
                handles.name(source)?
            )
        }
        A::ChooseEffectTarget {
            source,
            target: chosen,
            selected_count,
            min_targets,
            max_targets,
            ..
        } => {
            format!("Select {} for {} (already selected {selected_count}; choose {min_targets} to {max_targets})", target(chosen, handles, human)?, handles.name(source)?)
        }
        A::FinishEffectSelection {
            source,
            selected_count,
            ..
        }
        | A::FinishTargetSelection {
            source,
            selected_count,
            ..
        } => {
            format!(
                "Finish selection for {} with {selected_count} selected",
                handles.name(source)?
            )
        }
        A::ChooseEffectColor {
            source,
            color: choice,
            ..
        } => format!(
            "Choose {{{}}} for {}",
            color(*choice),
            handles.name(source)?
        ),
        A::ChooseEffectBoolean { source, value, .. } => {
            let ward = observation
                .extensions
                .pending_ward_payment
                .as_ref()
                .ok_or(Error::UnsupportedPrompt)?;
            if ward.payer != human {
                return Err(Error::UnsupportedPrompt);
            }
            let targeter = observation
                .projection
                .surface
                .stack
                .get(ward.targeting_stack_index as usize)
                .ok_or(Error::InvalidVisibleReference)?;
            format!(
                "{} {{{}}} for Ward from {}, which targets stack item #{} ({}){}",
                if *value { "Pay" } else { "Decline to pay" },
                ward.generic,
                handles.name(source)?,
                ward.targeting_stack_index + 1,
                handles.name(&targeter.source)?,
                if *value {
                    ""
                } else {
                    "; Ward will attempt to counter that item"
                }
            )
        }
        A::ChooseOptionalCostUse { use_cost, .. } => {
            format!(
                "{} the optional cost for {}{}",
                if *use_cost { "Pay" } else { "Decline" },
                handles.name(optional_source(observation)?)?,
                if *use_cost {
                    "; choose payment next"
                } else {
                    ""
                }
            )
        }
        A::ChooseOptionalCostWhich { choice, .. } => {
            let pending = observation
                .projection
                .surface
                .engine_context
                .pending_optional_cost
                .as_ref()
                .ok_or(Error::UnsupportedPrompt)?;
            let payment = match choice {
                OptionalCostChoice::Decline => "Decline payment".into(),
                OptionalCostChoice::Discard => format!("Discard {} cards", pending.discard_cards),
                OptionalCostChoice::SacrificeLand => {
                    format!("Sacrifice {} lands", pending.sacrifice_lands)
                }
                OptionalCostChoice::ReturnPermanent => {
                    "Return a permanent to its owner's hand".into()
                }
            };
            format!(
                "{payment} for {}",
                handles.name(optional_source(observation)?)?
            )
        }
        A::ChooseSpellCopyRetarget {
            source,
            change_target,
            ..
        } => format!(
            "{} for the copy of {}",
            if *change_target {
                "Choose a new target"
            } else {
                "Keep the inherited target"
            },
            handles.name(source)?
        ),
        A::ChooseMadnessCast { card, cast_it, .. } => {
            if *cast_it {
                let cost = definition(card)?
                    .madness_cost
                    .ok_or(Error::UnsupportedPrompt)?;
                format!(
                    "Cast {} for its madness cost {}",
                    handles.name(card)?,
                    mana(cost)
                )
            } else {
                format!(
                    "Decline madness; put {} into its owner's graveyard",
                    handles.name(card)?
                )
            }
        }
        A::Discard { cards, .. } => format!("Discard {}", names(cards, handles)?),
        A::DeclareAttackers { attackers, .. } => {
            format!("Declare attackers: {}", names(attackers, handles)?)
        }
        A::DeclareBlockersForAttacker {
            attacker, blockers, ..
        } => format!(
            "Block {} with {}",
            handles.name(attacker)?,
            names(blockers, handles)?
        ),
        A::ChooseAttackerInclusion {
            attacker, include, ..
        } => format!(
            "{} {}",
            if *include {
                "Attack with"
            } else {
                "Do not attack with"
            },
            handles.name(attacker)?
        ),
        A::ChooseBlockerInclusion {
            attacker,
            blocker,
            include,
            ..
        } => format!(
            "{} {} to block {}",
            if *include { "Assign" } else { "Do not assign" },
            handles.name(blocker)?,
            handles.name(attacker)?
        ),
        // These visible records do not contain enough human meaning. In
        // particular effect option paths do not describe their outcomes, and
        // same-source triggers require frozen ability provenance, not an index.
        A::ActivateAbility { .. }
        | A::ChooseSpellMode { .. }
        | A::ChooseEffectOption { .. }
        | A::ChooseEffectNumber { .. }
        | A::ChooseSpellCopyPayment { .. }
        | A::OrderTriggers { .. }
        | A::Ambiguous { .. } => return Err(Error::UnsupportedPrompt),
    })
}
