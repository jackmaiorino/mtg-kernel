//! Human labels from validated visible facts and pinned definition metadata.
//! Unsupported meanings reject the complete prompt. Ordinals and Debug text
//! are never substituted for an ability, spell mode, or effect description.

use super::visible::Handles;
use super::HumanDecisionErrorV1 as Error;
use crate::card_def::{self, CostComponent, ManaAbilityAmountDef, ManaAbilityCostDef};
use crate::engine::{CastMode, ChosenCreatureCostZoneV1, CostKind, OptionalCostChoice};
use crate::effect::{CreatureFilter, EffectCond, EffectOp, ImpulseDuration, ObjectRef, PlayerRef, TargetRef};
use crate::mana::{Cost, ManaColor, Pip};
use crate::policy_observation_v6::{HistoricalSourceContextV6, ObservationV6};
use crate::rl::{ActionSemanticV1 as A, BooleanChoicePurposeV4, CardStableRefV1, PendingEffectChoiceSemanticV4, PendingTriggerKindV2, PlayerSeatV1, SpellCopyStageV2, StackItemKindV2, TargetRefV1};
use crate::state::Zone;

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

fn activation_cost(components: &[CostComponent], source: &str) -> Result<String, Error> {
    if components.is_empty() { return Ok("pay {0}".into()); }
    components.iter().map(|component| match component {
        CostComponent::Tap => Ok(format!("tap {source}")),
        CostComponent::SacrificeSelf => Ok(format!("sacrifice {source}")),
        CostComponent::ExileSelf => Ok(format!("exile {source}")),
        CostComponent::DiscardSelf => Ok(format!("discard {source}")),
        other => cost_components(std::slice::from_ref(other)),
    }).collect::<Result<Vec<_>, _>>().map(|parts| parts.join("; "))
}

// Definition programs contain no runtime objects or hidden card identities.
// Unsupported operations deliberately have no Debug/string fallback.
fn effect_player(player: PlayerRef) -> Result<&'static str, Error> {
    match player {
        PlayerRef::Controller => Ok("its controller"),
        PlayerRef::Opponent => Ok("its controller's opponent"),
        PlayerRef::Target(0) => Ok("the chosen player"),
        _ => Err(Error::UnsupportedPrompt),
    }
}

fn effect_target(target: TargetRef) -> Result<&'static str, Error> {
    match target {
        TargetRef::ThisSource => Ok("this source"),
        TargetRef::Target(0) => Ok("the chosen target"),
        TargetRef::Opponent => Ok("its controller's opponent"),
        _ => Err(Error::UnsupportedPrompt),
    }
}

fn effect_object(object: ObjectRef) -> Result<&'static str, Error> {
    match object {
        ObjectRef::ThisSource => Ok("this source"),
        ObjectRef::Target(0) => Ok("the chosen object"),
        _ => Err(Error::UnsupportedPrompt),
    }
}

fn effect_condition(condition: &EffectCond) -> Result<String, Error> {
    Ok(match condition {
        EffectCond::WasKicked => "this spell was kicked".into(),
        EffectCond::ControlsArtifactCount(n) => format!("its controller controls at least {n} artifacts"),
        EffectCond::TargetIsColor(0, c) => format!("the chosen target is {}", color(*c)),
        EffectCond::TargetInZone(0, Zone::Battlefield) => "the chosen target is on the battlefield".into(),
        EffectCond::TargetInZone(0, Zone::Stack) => "the chosen target is on the stack".into(),
        EffectCond::And(a, b) => format!("{} and {}", effect_condition(a)?, effect_condition(b)?),
        _ => return Err(Error::UnsupportedPrompt),
    })
}

fn describe_effect(effect: &EffectOp) -> Result<String, Error> {
    Ok(match effect {
        EffectOp::Sequence(ops) if ops.is_empty() => "do nothing".into(),
        EffectOp::Sequence(ops) => ops.iter().map(describe_effect).collect::<Result<Vec<_>, _>>()?.join("; then "),
        EffectOp::Conditional { cond, then, else_ } => format!("if {}, {}; otherwise {}", effect_condition(cond)?, describe_effect(then)?, describe_effect(else_)?),
        EffectOp::DealDamage { target, amount } => format!("deal {amount} damage to {}", effect_target(*target)?),
        EffectOp::DrawCards { player, count } => format!("{} draws {count} cards", effect_player(*player)?),
        EffectOp::DiscardCards { player, count } => format!("{} discards {count} cards", effect_player(*player)?),
        EffectOp::GainLife { player, amount } => format!("{} gains {amount} life", effect_player(*player)?),
        EffectOp::LoseLife { player, amount } => format!("{} loses {amount} life", effect_player(*player)?),
        EffectOp::AddMana { player, colors } => format!("{} adds {}", effect_player(*player)?, colors.iter().map(|c| format!("{{{}}}", color(*c))).collect::<String>()),
        EffectOp::CreateToken { token_def, controller } => {
            let token = card_def::CARD_DEFS.get(*token_def as usize).ok_or(Error::UnsupportedPrompt)?;
            if !token.is_token { return Err(Error::UnsupportedPrompt); }
            let stats = match (token.power, token.toughness) {
                (Some(p), Some(t)) => format!(" ({p}/{t})"),
                (None, None) => String::new(),
                _ => return Err(Error::UnsupportedPrompt),
            };
            // The named token is an exact pinned definition, not a hidden
            // runtime card. Include its relevant printed keyword explicitly.
            let keyword = if token.keywords == card_def::Keywords::NONE { "" }
                else if token.keywords == card_def::Keywords::VIGILANCE { " with vigilance" }
                else if token.keywords == card_def::Keywords::HASTE { " with haste" }
                else if token.keywords == card_def::Keywords::FLYING { " with flying" }
                else { return Err(Error::UnsupportedPrompt); };
            format!("{} creates {}{stats}{keyword}", effect_player(*controller)?, token.name)
        }
        EffectOp::ImpulseDraw { count, duration } => format!("exile the top {count} cards of its controller's library; that player may play them {}", match duration {
            ImpulseDuration::EndOfTurn => "until end of turn",
            ImpulseDuration::UntilOwnersNextTurn => "until the end of their next turn",
        }),
        EffectOp::PumpControlled { filter, power, toughness, grant_haste } => {
            let creatures = match filter {
                CreatureFilter::AnyControlled => "creatures its controller controls",
                CreatureFilter::ControlledWithSubtype(card_def::Subtype::Human) => "Humans its controller controls",
                _ => return Err(Error::UnsupportedPrompt),
            };
            format!("{creatures} get {power:+}/{toughness:+}{} until end of turn", if *grant_haste { " and gain haste" } else { "" })
        }
        EffectOp::MoveObject { object, to_zone } => {
            let destination = match to_zone {
                Zone::Graveyard => "its owner's graveyard",
                Zone::Hand => "its owner's hand",
                Zone::Exile => "exile",
                _ => return Err(Error::UnsupportedPrompt),
            };
            format!("put {} into {destination}", effect_object(*object)?)
        }
        EffectOp::DestroyObject { object } => format!("destroy {}", effect_object(*object)?),
        EffectOp::Sacrifice { object } => format!("sacrifice {}", effect_object(*object)?),
        EffectOp::TapObject { object } => format!("tap {}", effect_object(*object)?),
        EffectOp::UntapObject { object } => format!("untap {}", effect_object(*object)?),
        EffectOp::DamageOpponentAndTheirCreatures { amount } => format!("deal {amount} damage to its controller's opponent and each creature that opponent controls"),
        EffectOp::OfferAffectedPlayerSpellCopy { affected: TargetRef::Target(0) } => "the affected player may pay {R}{R} to copy this spell and may choose a new target".into(),
        _ => return Err(Error::UnsupportedPrompt),
    })
}

fn program_at_path<'a>(mut program: &'a EffectOp, path: &[u16]) -> Result<&'a EffectOp, Error> {
    for component in path {
        program = match program {
            EffectOp::Sequence(ops) => ops.get(*component as usize),
            EffectOp::Choice { options, .. } => options.get(*component as usize),
            EffectOp::Conditional { then, else_, .. } => match component {
                0 => Some(then.as_ref()), 1 => Some(else_.as_ref()), _ => None,
            },
            _ => None,
        }.ok_or(Error::UnsupportedPrompt)?;
    }
    Ok(program)
}

// The public DTO does not carry a runtime effect program or an ability
// selector for a resolving effect. Accept definition reconstruction only
// when every possible printed program agrees at this structural path.
fn pending_program(source: &CardStableRefV1, path: &[u16], observation: &ObservationV6) -> Result<EffectOp, Error> {
    if !observation.extensions.historical_public_sources.iter().any(|record|
        record.context == HistoricalSourceContextV6::PendingEffect && record.source == *source && record.stack_item_kind == StackItemKindV2::Spell) {
        return Err(Error::UnsupportedPrompt);
    }
    let def = definition(source)?;
    // Alternate spell forms have distinct programs without a public form
    // selector on the resolving-effect DTO. Do not assume the front face.
    if def.omen.is_some() || def.adventure.is_some() || def.bestow.is_some() { return Err(Error::UnsupportedPrompt); }
    let mut roots = Vec::new();
    if let Some(root) = (def.spell_effect)() { roots.push(root); }
    for mode in [&def.mode2, &def.mode3].into_iter().flatten() { roots.push((mode.effect)()); }
    let mut selected = None;
    for root in roots {
        let candidate = program_at_path(&root, path)?.clone();
        if selected.as_ref().is_some_and(|previous| previous != &candidate) { return Err(Error::UnsupportedPrompt); }
        selected = Some(candidate);
    }
    selected.ok_or(Error::UnsupportedPrompt)
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
        A::ActivateAbility { source, ability_index, .. } => {
            let def = definition(source)?;
            let ability = def.activated_abilities.get(*ability_index as usize)
                .ok_or(Error::UnsupportedPrompt)?;
            if ability.activation_zone != source.zone { return Err(Error::UnsupportedPrompt); }
            let name = handles.name(source)?;
            format!("Activate {name}: {}: {}{}", activation_cost(ability.cost, &name)?, describe_effect(&(ability.effect)())?,
                if ability.sorcery_speed_only { "; activate only as a sorcery" } else { "" })
        }
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
                        // No object cost beyond the ability's own mana cost
                        // (Barrels of Blasting Jelly's "{1}: Add one mana of
                        // any color"); `extra_cost` below supplies the "pay"
                        // phrase, so this base clause stays empty.
                        ManaAbilityCostDef::None => String::new(),
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
                if costs.is_empty() {
                    costs = format!("Pay {}", mana(extra));
                } else {
                    costs.push_str(&format!("; pay {}", mana(extra)));
                }
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
        A::ChooseSpellMode { source, mode_index, mode_count, .. } => {
            let def = definition(source)?;
            let count = 1 + u8::from(def.mode2.is_some()) + u8::from(def.mode3.is_some());
            if count != *mode_count || *mode_index >= count { return Err(Error::UnsupportedPrompt); }
            let effect = match mode_index {
                0 => (def.spell_effect)().ok_or(Error::UnsupportedPrompt)?,
                1 => (def.mode2.as_ref().ok_or(Error::UnsupportedPrompt)?.effect)(),
                2 => (def.mode3.as_ref().ok_or(Error::UnsupportedPrompt)?.effect)(),
                _ => return Err(Error::UnsupportedPrompt),
            };
            format!("For {}, {}", handles.name(source)?, describe_effect(&effect)?)
        }
        A::ChooseEffectOption { source, option_index, option_count, .. } => {
            let pending = observation.projection.surface.engine_context.pending_effect.as_ref()
                .ok_or(Error::UnsupportedPrompt)?;
            let Some(PendingEffectChoiceSemanticV4::Options { player, structural_path, option_count: count }) = &pending.choice else {
                return Err(Error::UnsupportedPrompt);
            };
            if pending.source.as_ref() != Some(source) || *player != human || count != option_count {
                return Err(Error::UnsupportedPrompt);
            }
            let EffectOp::Choice { options, .. } = pending_program(source, structural_path, observation)? else {
                return Err(Error::UnsupportedPrompt);
            };
            if options.len() != *option_count as usize { return Err(Error::UnsupportedPrompt); }
            let selected = options.get(*option_index as usize).ok_or(Error::UnsupportedPrompt)?;
            format!("For {}, {}", handles.name(source)?, describe_effect(selected)?)
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
            let pending = observation.projection.surface.engine_context.pending_effect.as_ref()
                .ok_or(Error::UnsupportedPrompt)?;
            let Some(PendingEffectChoiceSemanticV4::Boolean { player, structural_path, purpose, .. }) = &pending.choice else {
                return Err(Error::UnsupportedPrompt);
            };
            if pending.source.as_ref() != Some(source) || *player != human { return Err(Error::UnsupportedPrompt); }
            if *purpose == BooleanChoicePurposeV4::Shuffle {
                return Ok(format!("{} your library for {}", if *value { "Shuffle" } else { "Do not shuffle" }, handles.name(source)?));
            }
            let Some(ward) = observation.extensions.pending_ward_payment.as_ref() else {
                if *purpose != BooleanChoicePurposeV4::PayCost { return Err(Error::UnsupportedPrompt); }
                let EffectOp::MayPayManaThen { player: PlayerRef::Controller, colored, generic, then } = pending_program(source, structural_path, observation)? else {
                    return Err(Error::UnsupportedPrompt);
                };
                if pending.controller != human { return Err(Error::UnsupportedPrompt); }
                let mut cost = if generic == 0 { String::new() } else { format!("{{{generic}}}") };
                for c in colored { cost.push_str(&format!("{{{}}}", color(c))); }
                if cost.is_empty() { cost.push_str("{0}"); }
                return Ok(format!("{} {cost} for {}{}{}", if *value { "Pay" } else { "Decline to pay" }, handles.name(source)?,
                    if *value { ": " } else { "; do not " }, describe_effect(&then)?));
            };
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
        A::ChooseSpellCopyPayment { source, pay, .. } => {
            let pending = observation.projection.surface.engine_context.pending_spell_copy.as_ref()
                .ok_or(Error::UnsupportedPrompt)?;
            if pending.player != human || pending.parent.as_ref() != Some(source) || pending.stage != SpellCopyStageV2::Payment {
                return Err(Error::UnsupportedPrompt);
            }
            let root = (definition(source)?.spell_effect)().ok_or(Error::UnsupportedPrompt)?;
            let EffectOp::Sequence(ops) = root else { return Err(Error::UnsupportedPrompt); };
            if !matches!(ops.last(), Some(EffectOp::OfferAffectedPlayerSpellCopy { affected: TargetRef::Target(0) })) {
                return Err(Error::UnsupportedPrompt);
            }
            format!("{} {{R}}{{R}} to copy {} (inherited target: {}; a new target may be chosen after payment)",
                if *pay { "Attempt to pay" } else { "Decline to pay" }, handles.name(source)?, target(&pending.inherited_target, handles, human)?)
        }
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
        A::OrderTriggers { pending_sources, order, .. } => {
            let pending = &observation.projection.surface.engine_context.pending_triggers;
            if pending_sources.len() != order.len() || pending_sources.len() < 2 || pending.len() < order.len() {
                return Err(Error::UnsupportedPrompt);
            }
            let mut sorted = order.clone();
            sorted.sort_unstable();
            if sorted != (0..order.len()).collect::<Vec<_>>() { return Err(Error::UnsupportedPrompt); }
            let mut descriptions = Vec::new();
            let mut seen = Vec::new();
            for &index in order {
                let source = &pending_sources[index];
                let trigger = &pending[index];
                if trigger.source.as_ref() != Some(source) || trigger.controller != human || trigger.trigger_kind != PendingTriggerKindV2::TriggeredAbility {
                    return Err(Error::UnsupportedPrompt);
                }
                if seen.contains(source) { return Err(Error::UnsupportedPrompt); }
                seen.push(source.clone());
                descriptions.push(format!("Trigger from {}", handles.name(source)?));
            }
            format!("Put triggers on the stack from bottom to top (last resolves first): {}", descriptions.join("; then "))
        }
        // These visible records do not contain enough human meaning. In
        // particular effect option paths do not describe their outcomes, and
        // same-source triggers require frozen ability provenance, not an index.
        A::ChooseEffectNumber { .. }
        | A::Ambiguous { .. } => return Err(Error::UnsupportedPrompt),
    })
}
