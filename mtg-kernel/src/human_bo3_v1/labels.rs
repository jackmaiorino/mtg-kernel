//! Human labels from validated visible facts and pinned definition metadata.
//! Unsupported meanings reject the complete prompt. Ordinals and Debug text
//! are never substituted for an ability, spell mode, or effect description.

use super::visible::Handles;
use super::HumanDecisionErrorV1 as Error;
use crate::card_def::{self, CostComponent, ManaAbilityAmountDef, ManaAbilityCostDef};
use crate::effect::{
    CreatureFilter, EffectCond, EffectOp, ImpulseDuration, ObjectRef, PlayerRef, TargetRef,
};
use crate::engine::{CastMode, ChosenCreatureCostZoneV1, CostKind, OptionalCostChoice};
use crate::mana::{Cost, ManaColor, Pip};
use crate::policy_observation_v6::{HistoricalSourceContextV6, ObservationV6};
use crate::rl::{
    ActionSemanticV1 as A, BooleanChoicePurposeV4, CardStableRefV1, PendingEffectChoiceSemanticV4,
    PendingTriggerKindV2, PlayerSeatV1, SpellCopyStageV2, StackItemKindV2, TargetRefV1,
};
use crate::state::Zone;

// Borrow only the common typed facts used by labels. A successor observation
// is never converted into a V6 observation or assigned a predecessor identity.
struct LabelInput<'a> {
    projection: &'a crate::rl::PublicObservationProjectionV5,
    pending_chosen_creature_cost:
        Option<&'a crate::policy_observation_v6::PendingChosenCreatureCostV6>,
    pending_ward_payment: Option<&'a crate::policy_observation_v6::WardPaymentV6>,
    resolving_spell_sources: Vec<&'a CardStableRefV1>,
}

impl<'a> LabelInput<'a> {
    fn from_history(
        observation: &'a crate::heartfire_history_v1::HeartfireHistoryObservationV1,
    ) -> Self {
        Self {
            projection: &observation.projection,
            pending_chosen_creature_cost: observation
                .extensions
                .pending_chosen_creature_cost
                .as_ref(),
            pending_ward_payment: observation.extensions.pending_ward_payment.as_ref(),
            resolving_spell_sources: observation
                .extensions
                .historical_public_sources
                .iter()
                .filter(|record| {
                    record.context
                        == crate::policy_observation_v7::HistoricalSourceContextV7::PendingEffect
                        && record.stack_item_kind == StackItemKindV2::Spell
                })
                .map(|record| &record.source)
                .collect(),
        }
    }

    fn from_v6(observation: &'a ObservationV6) -> Self {
        Self {
            projection: &observation.projection,
            pending_chosen_creature_cost: observation
                .extensions
                .pending_chosen_creature_cost
                .as_ref(),
            pending_ward_payment: observation.extensions.pending_ward_payment.as_ref(),
            resolving_spell_sources: observation
                .extensions
                .historical_public_sources
                .iter()
                .filter(|record| {
                    record.context == HistoricalSourceContextV6::PendingEffect
                        && record.stack_item_kind == StackItemKindV2::Spell
                })
                .map(|record| &record.source)
                .collect(),
        }
    }

    fn from_parts_v8(observation: &super::life_ward_v8::ProjectionPartsV8<'a>) -> Self {
        Self {
            projection: observation.projection,
            pending_chosen_creature_cost: observation
                .extensions
                .pending_chosen_creature_cost
                .as_ref(),
            pending_ward_payment: observation.extensions.pending_ward_payment.as_ref(),
            resolving_spell_sources: observation
                .extensions
                .historical_public_sources
                .iter()
                .filter(|record| {
                    record.context
                        == crate::policy_observation_v7::HistoricalSourceContextV7::PendingEffect
                        && record.stack_item_kind == StackItemKindV2::Spell
                })
                .map(|record| &record.source)
                .collect(),
        }
    }
}

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
            Pip::Phyrexian(c) | Pip::PhyrexianAnyColor(c) => format!("{{{}/P}}", color(*c)),
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
    if components.is_empty() {
        return Ok("pay {0}".into());
    }
    components
        .iter()
        .map(|component| match component {
            CostComponent::Tap => Ok(format!("tap {source}")),
            CostComponent::SacrificeSelf => Ok(format!("sacrifice {source}")),
            CostComponent::ExileSelf => Ok(format!("exile {source}")),
            CostComponent::DiscardSelf => Ok(format!("discard {source}")),
            other => cost_components(std::slice::from_ref(other)),
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|parts| parts.join("; "))
}

// The owning V40 union can offer an ordinary Equipment's printed equip
// alongside a later family's granted abilities. Keep this exact supported
// root separate from the frozen common effect-description whitelist.
pub(super) fn label_printed_equip_v40(
    action: &A,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<Option<String>, Error> {
    let A::ActivateAbility {
        actor,
        source,
        ability_index,
        granted_ability,
        ..
    } = action
    else {
        return Ok(None);
    };
    if !granted_ability.is_none() {
        return Ok(None);
    }
    let def = definition(source)?;
    let Some(ability) = def.activated_abilities.get(usize::from(*ability_index)) else {
        return Ok(None);
    };
    if def.equipment.is_none()
        || def.capability != card_def::CardCapability::Full
        || source.zone != Zone::Battlefield
        || ability.activation_zone != Zone::Battlefield
        || ability.target_spec != card_def::TargetSpec::ControlledCreature
        || ability.activation_target_filter != card_def::ActivationTargetFilter::TargetSpecOnly
        || !ability.sorcery_speed_only
        || ability.max_activations_per_turn.is_some()
        || !matches!(ability.cost, [CostComponent::Mana(_)])
        || (ability.effect)()
            != (EffectOp::AttachSourceToTarget {
                object: ObjectRef::Target(0),
            })
    {
        return Ok(None);
    }
    if *actor != human {
        return Err(Error::NotHumanTurn);
    }
    Ok(Some(format!(
        "Activate {} equip: {}: attach this Equipment to target creature you control; activate only as a sorcery",
        handles.name(source)?,
        activation_cost(ability.cost, &handles.name(source)?)?
    )))
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
        EffectCond::ControlsArtifactCount(n) => {
            format!("its controller controls at least {n} artifacts")
        }
        EffectCond::TargetIsColor(0, c) => format!("the chosen target is {}", color(*c)),
        EffectCond::TargetInZone(0, Zone::Battlefield) => {
            "the chosen target is on the battlefield".into()
        }
        EffectCond::TargetInZone(0, Zone::Stack) => "the chosen target is on the stack".into(),
        EffectCond::And(a, b) => format!("{} and {}", effect_condition(a)?, effect_condition(b)?),
        _ => return Err(Error::UnsupportedPrompt),
    })
}

#[cfg(test)]
#[test]
fn frozen_human_labels_refuse_ooze_atomic_program() {
    assert!(matches!(
        describe_effect(&EffectOp::ExileGraveyardTargetThenGrowSourceIfCreature),
        Err(Error::UnsupportedPrompt)
    ));
}

fn describe_effect(effect: &EffectOp) -> Result<String, Error> {
    Ok(match effect {
        EffectOp::StandardLegendV1(crate::standard_legends_v1::LegendEffectV1::GwennaMana(first,second)) => format!("add {{{}}}{{{}}}; spend only on creature spells or abilities of creatures",color(*first),color(*second)),
        EffectOp::GrantKeywordToDeclaredTargetsUntilEndOfTurn { .. } => return Err(Error::UnsupportedPrompt),
        EffectOp::Sequence(ops) if ops.is_empty() => "do nothing".into(),
        EffectOp::Sequence(ops) => ops.iter().map(describe_effect).collect::<Result<Vec<_>, _>>()?.join("; then "),
        EffectOp::Conditional { cond, then, else_ } => format!("if {}, {}; otherwise {}", effect_condition(cond)?, describe_effect(then)?, describe_effect(else_)?),
        EffectOp::DealDamage { target, amount } => format!("deal {amount} damage to {}", effect_target(*target)?),
        EffectOp::DrawCards { player, count } => format!("{} draws {count} cards", effect_player(*player)?),
        EffectOp::DiscardCards { player, count } => format!("{} discards {count} cards", effect_player(*player)?),
        EffectOp::GainLife { player, amount } => format!("{} gains {amount} life", effect_player(*player)?),
        EffectOp::LoseLife { player, amount } => format!("{} loses {amount} life", effect_player(*player)?),
        EffectOp::LoseLifeEqualControllerTurnGainsV1 => {
            "the chosen opponent loses life equal to the total life you have gained this turn".into()
        }
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
            ImpulseDuration::UntilOwnersNextEndStep => "until their next end step",
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
                0 => Some(then.as_ref()),
                1 => Some(else_.as_ref()),
                _ => None,
            },
            _ => None,
        }
        .ok_or(Error::UnsupportedPrompt)?;
    }
    Ok(program)
}

// The public DTO does not carry a runtime effect program or an ability
// selector for a resolving effect. Accept definition reconstruction only
// when every possible printed program agrees at this structural path.
fn pending_program(
    source: &CardStableRefV1,
    path: &[u16],
    observation: &LabelInput<'_>,
) -> Result<EffectOp, Error> {
    if !observation.resolving_spell_sources.contains(&source) {
        return Err(Error::UnsupportedPrompt);
    }
    let def = definition(source)?;
    // Alternate spell forms have distinct programs without a public form
    // selector on the resolving-effect DTO. Do not assume the front face.
    if def.omen.is_some() || def.adventure.is_some() || def.bestow.is_some() {
        return Err(Error::UnsupportedPrompt);
    }
    let mut roots = Vec::new();
    if let Some(root) = (def.spell_effect)() {
        roots.push(root);
    }
    for mode in [&def.mode2, &def.mode3].into_iter().flatten() {
        roots.push((mode.effect)());
    }
    let mut selected = None;
    for root in roots {
        let candidate = program_at_path(&root, path)?.clone();
        if selected
            .as_ref()
            .is_some_and(|previous| previous != &candidate)
        {
            return Err(Error::UnsupportedPrompt);
        }
        selected = Some(candidate);
    }
    selected.ok_or(Error::UnsupportedPrompt)
}

pub(super) fn target(
    target: &TargetRefV1,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<String, Error> {
    match target {
        TargetRefV1::Player { player } => Ok(if *player == human {
            "you"
        } else {
            "your opponent"
        }
        .into()),
        TargetRefV1::Object { object } => handles.name(object),
        TargetRefV1::StackItem {
            source,
            kind,
            stack_item_id,
            stack_index,
            ..
        } => Ok(format!(
            "{:?} of {} ({})",
            kind,
            handles.name(source)?,
            handles.stack_target_label(*stack_item_id, *stack_index)?
        )),
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

fn optional_source<'a>(observation: &LabelInput<'a>) -> Result<&'a CardStableRefV1, Error> {
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
        | A::TurnFaceUp { actor, .. }
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
        | A::OrderTriggers { actor, .. }
        | A::ChooseCombatDamageRange { actor, .. }
        | A::ChooseLondonMulligan { actor, .. }
        | A::ChooseLondonBottom { actor, .. }
        | A::ChooseAttackTarget { actor, .. }
        | A::ChooseLegendPermanent { actor, .. }
        | A::ChooseEntryColorV36 { actor, .. }
        | A::ChooseDivineKickerV44 { actor, .. }
        | A::ChooseDivineTargetV44 { actor, .. }
        | A::FinishDivineTargetsV44 { actor, .. } => Some(*actor),
        #[cfg(feature = "limited-fdn-fixtures")]
        A::ChooseTriggerOrderNext { actor, .. } => Some(*actor),
        A::Ambiguous { .. } => None,
    }
}

pub(super) fn label(
    action: &A,
    observation: &ObservationV6,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<String, Error> {
    label_common(action, &LabelInput::from_v6(observation), handles, human)
}

pub(super) fn label_v8(
    action: &A,
    observation: &crate::policy_observation_v8::ObservationV8,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<String, Error> {
    label_parts_v8(
        action,
        &super::life_ward_v8::ProjectionPartsV8::from(observation),
        handles,
        human,
    )
}

pub(super) fn label_parts_v8(
    action: &A,
    observation: &super::life_ward_v8::ProjectionPartsV8<'_>,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<String, Error> {
    if let Some(payment) = &observation.extensions.pending_ward_life_payment {
        let A::ChooseEffectBoolean {
            actor,
            source,
            value,
        } = action
        else {
            return Err(Error::UnsupportedPrompt);
        };
        let pending = observation
            .projection
            .surface
            .engine_context
            .pending_effect
            .as_ref()
            .ok_or(Error::UnsupportedPrompt)?;
        let Some(PendingEffectChoiceSemanticV4::Boolean {
            player, purpose, ..
        }) = &pending.choice
        else {
            return Err(Error::UnsupportedPrompt);
        };
        if *actor != human
            || *player != human
            || payment.payer != human
            || *purpose != BooleanChoicePurposeV4::PayCost
            || pending.source.as_ref() != Some(source)
            || observation.extensions.pending_ward_payment.is_some()
        {
            return Err(Error::UnsupportedPrompt);
        }
        let targeter = observation
            .projection
            .surface
            .stack
            .get(payment.targeting_stack_index as usize)
            .ok_or(Error::InvalidVisibleReference)?;
        if targeter.stack_index != payment.targeting_stack_index || targeter.controller != human {
            return Err(Error::InvalidVisibleReference);
        }
        return Ok(format!(
            "{} {} life for Ward from {}, which targets stack item #{} ({}){}",
            if *value { "Pay" } else { "Decline to pay" },
            payment.life,
            handles.name(source)?,
            payment.targeting_stack_index + 1,
            handles.name(&targeter.source)?,
            if *value {
                ""
            } else {
                "; Ward will attempt to counter that item"
            },
        ));
    }
    label_common(
        action,
        &LabelInput::from_parts_v8(observation),
        handles,
        human,
    )
}

/// Opt-in successor for the five next-printing labels. Facts are supplied by
/// the V59 owner; predecessor V8 callers never synthesize or reinterpret them.
pub(super) fn label_parts_v59(
    action: &A,
    observation: &super::life_ward_v8::ProjectionPartsV8<'_>,
    handles: &Handles,
    human: PlayerSeatV1,
    facts: &crate::next_five_public_v59::FactsV59,
) -> Result<String, Error> {
    use crate::{
        next_five_public_v59::{self as public, TargetV59, TriggerStageV59},
        optional_trigger_public_v1::{IdentityV1, ReferenceV1},
        rules_meaning_next_five_v59::{self as meaning, FamilyV59},
    };

    if facts.schema != public::SCHEMA
        || facts.producer_sha256 != public::producer_sha256()
        || facts.viewer != human
        || facts.meanings.len() != FamilyV59::ALL.len()
    {
        return Err(Error::UnsupportedPrompt);
    }
    for family in FamilyV59::ALL {
        let mut rows = facts.meanings.iter().filter(|row| row.family == family);
        let Some(row) = rows.next() else {
            return Err(Error::UnsupportedPrompt);
        };
        if rows.next().is_some()
            || meaning::validate(row).is_err()
            || row.producer_sha256 != meaning::producer_sha256()
        {
            return Err(Error::UnsupportedPrompt);
        }
    }
    if actor(action) != Some(human) {
        return Err(Error::NotHumanTurn);
    }

    let ref_matches = |stable: &CardStableRefV1, public: &ReferenceV1| {
        matches!(&public.identity, IdentityV1::Known { card_id }
            if stable.card_db_id == *card_id)
            && stable.arena_id == public.object
            && stable.zone_change_count == public.generation
            && stable.owner == public.owner
            && stable.controller == public.controller
            && stable.zone == public.zone
    };
    let stable_refs_match = |left: &CardStableRefV1, right: &CardStableRefV1| {
        left.arena_id == right.arena_id
            && left.card_db_id == right.card_db_id
            && left.owner == right.owner
            && left.controller == right.controller
            && left.zone == right.zone
            && left.zone_change_count == right.zone_change_count
    };
    let source_authorized = |source: &CardStableRefV1, family: FamilyV59| {
        facts
            .authorized
            .iter()
            .any(|row| row.family == family && ref_matches(source, &row.card))
    };
    let meaning_for = |source: &CardStableRefV1| {
        facts
            .meanings
            .iter()
            .find(|row| row.card_id == source.card_db_id)
    };
    let public_name = |reference: &ReferenceV1| -> Result<String, Error> {
        match &reference.identity {
            IdentityV1::Known { card_id } => handles.name(&CardStableRefV1 {
                arena_id: reference.object,
                card_db_id: *card_id,
                owner: reference.owner,
                controller: reference.controller,
                zone: reference.zone,
                zone_change_count: reference.generation,
            }),
            IdentityV1::FaceDown { .. } => Ok("the face-down permanent".into()),
        }
    };
    let target_name = |target: &TargetRefV1| -> Result<String, Error> {
        match target {
            TargetRefV1::Object { object } => handles.name(object),
            _ => Err(Error::UnsupportedPrompt),
        }
    };
    let root_matches = |source: &CardStableRefV1,
                        root: &crate::optional_trigger_public_v1::RootV1| {
        ref_matches(source, &root.original_source)
            || root
                .current_original_source
                .as_ref()
                .is_some_and(|current| ref_matches(source, current))
    };

    match action {
        A::ChooseTarget { source, target, .. }
            if meaning_for(source).is_some_and(|row| row.family == FamilyV59::Goblin)
                && source_authorized(source, FamilyV59::Goblin) =>
        {
            let name = target_name(target)?;
            let Some(cast) = facts
                .casts
                .iter()
                .find(|row| row.family == FamilyV59::Goblin && ref_matches(source, &row.source))
            else {
                return Err(Error::InvalidVisibleReference);
            };
            if !cast.payment_pending || cast.original_controller != human {
                return Err(Error::InvalidVisibleReference);
            }
            let x = cast.x.map_or_else(
                || "X is not selected yet".to_string(),
                |value| format!("X = {value}"),
            );
            return Ok(format!(
                "Choose {name} as the creature target for Goblin Negotiation ({x})"
            ));
        }
        A::ChooseTarget {
            actor,
            source,
            target,
            ..
        } if *actor == human
            && meaning_for(source).is_some_and(|row| row.family == FamilyV59::Replication)
            && facts.triggers.iter().any(|row| {
                row.family == FamilyV59::Replication
                    && row.stage == TriggerStageV59::Held
                    && row.source.captured_controller == human
                    && root_matches(source, &row.source)
            }) =>
        {
            let TargetRefV1::Object { object } = target else {
                return Err(Error::UnsupportedPrompt);
            };
            if object.controller != human || object.zone != Zone::Battlefield {
                return Err(Error::InvalidVisibleReference);
            }
            return Ok(format!(
                "Choose {} as another nonland permanent you control for the Extravagant Replication trigger",
                handles.name(object)?
            ));
        }
        A::ChooseEffectTarget {
            actor,
            source,
            target,
            selected_count,
            min_targets,
            max_targets,
        } if *actor == human
            && meaning_for(source).is_some_and(|row| row.family == FamilyV59::Replication) =>
        {
            let Some(copy) = facts.aura_copy.as_ref() else {
                return label_parts_v8(action, observation, handles, human);
            };
            if !copy.untargeted
                || copy.chooser != human
                || !root_matches(source, &copy.source)
                || *selected_count != 0
                || *min_targets != 1
                || *max_targets != 1
                || copy.remaining_count == 0
                || copy.accepted_host_awaiting_advance.is_some()
                || !copy.candidate_hosts.iter().any(|host| match target {
                    TargetRefV1::Object { object } => ref_matches(object, host),
                    _ => false,
                })
            {
                return Err(Error::InvalidVisibleReference);
            }
            let TargetRefV1::Object { object } = target else {
                return Err(Error::UnsupportedPrompt);
            };
            return Ok(format!(
                "Choose {} as an Aura host for the Extravagant Replication copy",
                handles.name(object)?
            ));
        }
        A::ChooseEffectOption {
            actor,
            source,
            option_index,
            option_count,
        } if *actor == human
            && meaning_for(source).is_some_and(|row| row.family == FamilyV59::Goblin)
            && facts.goblin_ordering.is_none() =>
        {
            let Some(cast) = facts
                .casts
                .iter()
                .find(|row| row.family == FamilyV59::Goblin && ref_matches(source, &row.source))
            else {
                return Err(Error::InvalidVisibleReference);
            };
            if !cast.payment_pending
                || cast.original_controller != human
                || cast.x.is_some()
                || usize::from(*option_index) >= usize::from(*option_count)
            {
                return Err(Error::InvalidVisibleReference);
            }
            let [TargetV59::Object(target)] = cast.targets.as_slice() else {
                return Err(Error::InvalidVisibleReference);
            };
            return Ok(format!(
                "Cast Goblin Negotiation with X = {option_index}, targeting {}; it deals X damage and creates Goblins only for actual damage in excess of lethal after replacement and prevention",
                public_name(target)?
            ));
        }
        A::ChooseEffectOption {
            actor,
            source,
            option_index,
            option_count,
        } if *actor == human
            && meaning_for(source).is_some_and(|row| row.family == FamilyV59::Goblin)
            && facts.goblin_ordering.is_some() =>
        {
            let stage = facts.goblin_ordering.as_ref().unwrap();
            let Some(option) = facts.goblin_ordering_options.get(*option_index as usize) else {
                return Err(Error::InvalidVisibleReference);
            };
            let quote_matches = facts.goblin_quotes.iter().any(|quote| {
                quote.original_controller == stage.controller
                    && quote.original_x == stage.original_x
                    && quote.target_legal_now
                    && ref_matches(source, &quote.source)
                    && ref_matches(&stage.source, &quote.source)
                    && matches!(&quote.target, TargetV59::Object(target)
                        if ref_matches(&stage.target, target))
            });
            if stage.chooser != human
                || !quote_matches
                || !stable_refs_match(source, &stage.source)
                || usize::from(*option_count) != facts.goblin_ordering_options.len()
                || *option_count == 0
            {
                return Err(Error::InvalidVisibleReference);
            }
            let operation = match option.kind.as_str() {
                "multiplier" => "double the remaining damage",
                "barricade" => "prevent all remaining damage",
                "protection" => "prevent all remaining damage through protection",
                "shield" => "use the remaining damage shield",
                "color" => "apply color-based damage prevention",
                _ => return Err(Error::UnsupportedPrompt),
            };
            let shield = option
                .shield_remaining
                .map(|amount| format!(" ({amount} shield damage remains)"))
                .unwrap_or_default();
            return Ok(format!(
                "For Goblin Negotiation (X = {}), {operation} from {}{shield} against the {} remaining damage to {}",
                stage.original_x,
                handles.name(&option.source)?,
                stage.remaining_damage,
                handles.name(&stage.target)?
            ));
        }
        A::OrderTriggers {
            actor,
            pending_sources,
            order,
        } if *actor == human => {
            let mut sorted = order.clone();
            sorted.sort_unstable();
            if sorted != (0..order.len()).collect::<Vec<_>>()
                || pending_sources.len() != order.len()
                || pending_sources.len() < 2
            {
                return Err(Error::InvalidVisibleReference);
            }
            let held = facts
                .triggers
                .iter()
                .filter(|row| {
                    row.stage == TriggerStageV59::Held && row.source.captured_controller == human
                })
                .collect::<Vec<_>>();
            if held.len() != pending_sources.len() {
                return label_parts_v8(action, observation, handles, human);
            }
            let assigned = pending_sources
                .iter()
                .zip(&held)
                .map(|(source, row)| {
                    if !root_matches(source, &row.source) {
                        return Err(Error::InvalidVisibleReference);
                    }
                    Ok(*row)
                })
                .collect::<Result<Vec<_>, _>>()?;
            if assigned
                .iter()
                .any(|row| !matches!(row.family, FamilyV59::Replication | FamilyV59::Infernal))
            {
                return label_parts_v8(action, observation, handles, human);
            }
            let mut descriptions = Vec::with_capacity(order.len());
            for &position in order {
                let row = assigned
                    .get(position)
                    .ok_or(Error::InvalidVisibleReference)?;
                let source = pending_sources
                    .get(position)
                    .ok_or(Error::InvalidVisibleReference)?;
                let name = handles.name(source)?;
                let description = match row.family {
                    FamilyV59::Replication => {
                        if !row.targets.is_empty() {
                            return Err(Error::UnsupportedPrompt);
                        }
                        format!(
                            "{name} trigger: create a token copy of a nonland permanent you control"
                        )
                    }
                    FamilyV59::Infernal => {
                        let death = facts
                            .infernal_deaths
                            .iter()
                            .find(|death| {
                                death.source.original_source == row.source.original_source
                            })
                            .ok_or(Error::UnsupportedPrompt)?;
                        if !death.captured_not_demon || !death.collection_completed {
                            return Err(Error::UnsupportedPrompt);
                        }
                        if death.return_incarnation_available_now {
                            format!(
                                "{name} trigger: return the original non-token card from its owner's graveyard to the battlefield under its owner's control with two +1/+1 counters, entering as a Demon if that same incarnation is still there on resolution"
                            )
                        } else {
                            format!(
                                "{name} trigger: the original non-token card is no longer returnable from its owner's graveyard, so this trigger will do nothing"
                            )
                        }
                    }
                    _ => return Err(Error::UnsupportedPrompt),
                };
                descriptions.push(description);
            }
            return Ok(format!(
                "Put triggers on the stack from bottom to top (last resolves first): {}",
                descriptions.join("; then ")
            ));
        }
        _ => {}
    }

    label_parts_v8(action, observation, handles, human)
}

/// Opt-in label successor. Predecessor entry points never call this function.
pub(super) fn label_ooze_v11(
    action: &A,
    observation: &crate::policy_observation_v8::ObservationV8,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<String, Error> {
    if let A::ActivateAbility {
        source,
        ability_index,
        granted_ability,
        ..
    } = action
    {
        let (card_id, index) = granted_ability
            .as_ref()
            .map(|(donor, local)| (donor.card_db_id, u32::from(*local)))
            .unwrap_or((source.card_db_id, u32::from(*ability_index)));
        if crate::rules_meaning_ooze_v1::root_meaning(card_id, index).is_ok() {
            if actor(action) != Some(human) || source.zone != Zone::Battlefield {
                return Err(Error::UnsupportedPrompt);
            }
            let name = handles.name(source)?;
            return Ok(format!("Activate {name}: pay {{G}}: exile one target card from either graveyard; if it was a creature card before the move, put a +1/+1 counter on this same permanent and gain 1 life. You still gain life if this permanent has left the battlefield"));
        }
    }
    label_v8(action, observation, handles, human)
}

/// Definition-owned Sower opt-in. No predecessor entry point calls this.
pub(super) fn label_frenzied_v15(
    action: &A,
    observation: &crate::frenzied_public_v1::FrenziedObservationV1,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<String, Error> {
    if let A::ChooseEffectBoolean {
        actor,
        source,
        value,
    } = action
    {
        let rows: Vec<_> = observation
            .resolutions
            .iter()
            .filter(|row| row.phase == "optional_red_payment")
            .collect();
        if let [row] = rows.as_slice() {
            let pending = observation
                .projection
                .surface
                .engine_context
                .pending_effect
                .as_ref()
                .ok_or(Error::UnsupportedPrompt)?;
            let Some(PendingEffectChoiceSemanticV4::Boolean {
                player, purpose, ..
            }) = &pending.choice
            else {
                return Err(Error::UnsupportedPrompt);
            };
            if *actor != human
                || row.captured_payer != human
                || *player != human
                || *purpose != BooleanChoicePurposeV4::PayCost
                || source.arena_id != row.source_at_attack.arena_id
                || pending.controller != human
                || row.live_stack_index.is_none()
                || observation.extensions.pending_ward_life_payment.is_some()
                || observation.extensions.pending_ward_payment.is_some()
            {
                return Err(Error::UnsupportedPrompt);
            }
            let selector = crate::rules_meaning_frenzied_goblin_v1::live_selector()
                .map_err(|_| Error::UnsupportedPrompt)?;
            crate::rules_meaning_frenzied_goblin_v1::meaning(
                &selector,
                row.source_at_attack.card_db_id,
                0,
            )
            .map_err(|_| Error::UnsupportedPrompt)?;
            return Ok(format!(
                "{} {{R}} for {}: {}",
                if *value { "Pay" } else { "Decline to pay" },
                handles.name(&row.source_at_attack)?,
                if *value {
                    format!(
                        "{} cannot block until the next cleanup",
                        handles.name(&row.target_at_placement)?
                    )
                } else {
                    "do not install a restriction".into()
                }
            ));
        }
    }
    let parts = super::life_ward_v8::ProjectionPartsV8 {
        projection: &observation.projection,
        own_hand: &observation.own_hand,
        known_library_cards: &observation.known_library_cards,
        known_hand_cards: &observation.known_hand_cards,
        known_face_down_cards: &observation.known_face_down_cards,
        extensions: &observation.extensions,
    };
    label_parts_v8(action, &parts, handles, human)
}

pub(super) fn label_librarian_v20(
    action: &A,
    observation: &super::life_ward_v8::ProjectionPartsV8<'_>,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<String, Error> {
    let selected = match action {
        A::CastSpell { source, .. } if definition(source)?.name == "Mild-Mannered Librarian" => {
            Some((
                source,
                crate::rules_meaning_librarian_v1::RootV1::PrintedCreatureSpell,
            ))
        }
        A::ActivateAbility {
            source,
            ability_index,
            granted_ability,
            ..
        } if granted_ability.is_none() && definition(source)?.name == "Mild-Mannered Librarian" => {
            Some((
                source,
                crate::rules_meaning_librarian_v1::RootV1::PrintedActivation {
                    ability_index: u16::from(*ability_index),
                },
            ))
        }
        _ => None,
    };
    if let Some((source, root)) = selected {
        let selector = crate::rules_meaning_librarian_v1::live_selector()
            .map_err(|_| Error::UnsupportedPrompt)?;
        crate::rules_meaning_librarian_v1::meaning(&selector, source.card_db_id, root)
            .map_err(|_| Error::UnsupportedPrompt)?;
        if actor(action) != Some(human) {
            return Err(Error::UnsupportedPrompt);
        }
        return Ok(match root {
            crate::rules_meaning_librarian_v1::RootV1::PrintedCreatureSpell => format!(
                "Begin casting {} (printed cost {{G}}): a green 1/1 Human creature",
                handles.name(source)?
            ),
            crate::rules_meaning_librarian_v1::RootV1::PrintedActivation { .. } => {
                if source.zone != Zone::Battlefield {
                    return Err(Error::UnsupportedPrompt);
                }
                format!("Activate {} (printed cost {{3}}{{G}}; activate only once): replace its creature types with Werewolf and put two +1/+1 counters on the original permanent, then you draw a card. The captured controller draws even if the source departs. Payment consumes the once allowance even if countered", handles.name(source)?)
            }
        });
    }
    label_parts_v8(action, observation, handles, human)
}

pub(super) fn label_sower_v14(
    action: &A,
    observation: &super::life_ward_v8::ProjectionPartsV8<'_>,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<String, Error> {
    if let A::ActivateAbility {
        source,
        ability_index,
        granted_ability,
        ..
    } = action
    {
        if granted_ability.is_none() && definition(source)?.name == "Sower of Chaos" {
            let selector = crate::rules_meaning_sower_v1::live_selector_v1()
                .map_err(|_| Error::UnsupportedPrompt)?;
            crate::rules_meaning_sower_v1::meaning_v1(
                &selector,
                source.card_db_id,
                u16::from(*ability_index),
            )
            .map_err(|_| Error::UnsupportedPrompt)?;
            if actor(action) != Some(human) || source.zone != Zone::Battlefield {
                return Err(Error::UnsupportedPrompt);
            }
            return Ok(format!("Activate {}: pay {{2}}{{R}}; one target creature cannot block until the next cleanup. Control changes and ability removal do not end the restriction; this source need not remain on the battlefield", handles.name(source)?));
        }
    }
    label_parts_v8(action, observation, handles, human)
}

/// Definition-owned Heartfire label opt-in. Predecessors retain their refusal.
pub(super) fn label_heartfire_v12(
    action: &A,
    observation: &crate::policy_observation_v8::ObservationV8,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<String, Error> {
    if let A::ActivateAbility {
        source,
        ability_index,
        granted_ability,
        ..
    } = action
    {
        if granted_ability.is_none()
            && crate::rules_meaning_heartfire_v1::root_meaning(
                source.card_db_id,
                u32::from(*ability_index),
            )
            .is_ok()
        {
            if actor(action) != Some(human) || source.zone != Zone::Battlefield {
                return Err(Error::UnsupportedPrompt);
            }
            let name = handles.name(source)?;
            return Ok(format!("Activate {name}: pay {{R}} and sacrifice this permanent: it deals damage equal to its power as it left the battlefield to one target creature or planeswalker. Negative power deals no damage; use its captured controller, color and damage keywords"));
        }
    }
    label_v8(action, observation, handles, human)
}

#[cfg(test)]
#[test]
fn frozen_human_labels_refuse_heartfire_historical_damage_program() {
    assert_eq!(
        describe_effect(&EffectOp::StandardV1(
            crate::standard_cards_v1::StandardOpV1::SourceLastPowerDamageV1
        )),
        Err(Error::UnsupportedPrompt)
    );
}

pub(super) fn label_heartfire_history_v1(
    action: &A,
    observation: &crate::heartfire_history_v1::HeartfireHistoryObservationV1,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<String, Error> {
    if let A::ActivateAbility {
        source,
        ability_index,
        granted_ability,
        ..
    } = action
    {
        if granted_ability.is_none()
            && crate::rules_meaning_heartfire_v1::root_meaning(
                source.card_db_id,
                u32::from(*ability_index),
            )
            .is_ok()
        {
            if actor(action) != Some(human) || source.zone != Zone::Battlefield {
                return Err(Error::UnsupportedPrompt);
            }
            let name = handles.name(source)?;
            return Ok(format!("Activate {name}: pay {{R}} and sacrifice this permanent: it deals damage equal to its power as it left the battlefield to one target creature or planeswalker. Negative power deals no damage; use its captured controller, color and damage keywords"));
        }
    }
    label_common(
        action,
        &LabelInput::from_history(observation),
        handles,
        human,
    )
}

fn label_common(
    action: &A,
    observation: &LabelInput<'_>,
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
        A::TurnFaceUp { source, .. } => format!("Turn {} face up", handles.name(source)?),
        A::ActivateAbility {
            source,
            ability_index,
            granted_ability,
            ..
        } => {
            let def = definition(source)?;
            let ability = if let Some((donor, local)) = granted_ability.as_ref() {
                crate::standard_cards_v1::cauldron_ability_of(donor.card_db_id, *local)
            } else {
                def.activated_abilities
                    .get(*ability_index as usize)
                    .copied()
            }
            .ok_or(Error::UnsupportedPrompt)?;
            if ability.activation_zone != source.zone {
                return Err(Error::UnsupportedPrompt);
            }
            let name = handles.name(source)?;
            format!(
                "Activate {name}: {}: {}{}",
                activation_cost(ability.cost, &name)?,
                describe_effect(&(ability.effect)())?,
                if ability.sorcery_speed_only {
                    "; activate only as a sorcery"
                } else {
                    ""
                }
            )
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
            let restricted = if primary || additional.is_some() {
                None
            } else {
                def.restricted_mana_abilities
                    .iter()
                    .find(|ability| ability.colors.contains(&choice))
            };
            if !primary && additional.is_none() && restricted.is_none() {
                return Err(Error::UnsupportedPrompt);
            }
            if let Some(restricted) = restricted {
                if cost_target.is_some() {
                    return Err(Error::UnsupportedPrompt);
                }
                let restriction = match restricted.restriction {
                    card_def::ManaSpendRestrictionDef::SpellWithSubtype(
                        card_def::Subtype::Angel,
                    ) => "spend this mana only to cast Angel spells",
                    _ => return Err(Error::UnsupportedPrompt),
                };
                return Ok(format!(
                    "Tap {}: add {{{}}}; {restriction}",
                    handles.name(source)?,
                    color(choice)
                ));
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
                        ManaAbilityCostDef::TapSelfPayLife(life) => {
                            format!("Tap {source_name} and pay {life} life")
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
                CostKind::ExileCraftMaterial => "Exile as craft material",
                CostKind::TapPermanents => "Tap",
                CostKind::ReturnPermanentsToHand => "Return to its owner's hand",
                CostKind::ChooseCreatureOrRevealCreature => {
                    match observation
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
        A::ChooseSpellMode {
            source,
            mode_index,
            mode_count,
            ..
        } => {
            let def = definition(source)?;
            let count = 1 + u8::from(def.mode2.is_some()) + u8::from(def.mode3.is_some());
            if count != *mode_count || *mode_index >= count {
                return Err(Error::UnsupportedPrompt);
            }
            let effect = match mode_index {
                0 => (def.spell_effect)().ok_or(Error::UnsupportedPrompt)?,
                1 => (def.mode2.as_ref().ok_or(Error::UnsupportedPrompt)?.effect)(),
                2 => (def.mode3.as_ref().ok_or(Error::UnsupportedPrompt)?.effect)(),
                _ => return Err(Error::UnsupportedPrompt),
            };
            format!(
                "For {}, {}",
                handles.name(source)?,
                describe_effect(&effect)?
            )
        }
        A::ChooseEffectOption {
            source,
            option_index,
            option_count,
            ..
        } => {
            let pending = observation
                .projection
                .surface
                .engine_context
                .pending_effect
                .as_ref()
                .ok_or(Error::UnsupportedPrompt)?;
            let Some(PendingEffectChoiceSemanticV4::Options {
                player,
                structural_path,
                option_count: count,
                creature_options,
            }) = &pending.choice
            else {
                return Err(Error::UnsupportedPrompt);
            };
            if pending.source.as_ref() != Some(source) || *player != human || count != option_count
            {
                return Err(Error::UnsupportedPrompt);
            }
            if let Some(options) = creature_options {
                let option = options
                    .get(*option_index as usize)
                    .ok_or(Error::UnsupportedPrompt)?;
                return Ok(format!("For {}, {}", handles.name(source)?, option.label()));
            }
            let EffectOp::Choice { options, .. } =
                pending_program(source, structural_path, observation)?
            else {
                return Err(Error::UnsupportedPrompt);
            };
            if options.len() != *option_count as usize {
                return Err(Error::UnsupportedPrompt);
            }
            let selected = options
                .get(*option_index as usize)
                .ok_or(Error::UnsupportedPrompt)?;
            format!(
                "For {}, {}",
                handles.name(source)?,
                describe_effect(selected)?
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
            let pending = observation
                .projection
                .surface
                .engine_context
                .pending_effect
                .as_ref()
                .ok_or(Error::UnsupportedPrompt)?;
            let Some(PendingEffectChoiceSemanticV4::Boolean {
                player,
                structural_path,
                purpose,
                ..
            }) = &pending.choice
            else {
                return Err(Error::UnsupportedPrompt);
            };
            if pending.source.as_ref() != Some(source) || *player != human {
                return Err(Error::UnsupportedPrompt);
            }
            if *purpose == BooleanChoicePurposeV4::Shuffle {
                return Ok(format!(
                    "{} your library for {}",
                    if *value { "Shuffle" } else { "Do not shuffle" },
                    handles.name(source)?
                ));
            }
            let Some(ward) = observation.pending_ward_payment.as_ref() else {
                if *purpose != BooleanChoicePurposeV4::PayCost {
                    return Err(Error::UnsupportedPrompt);
                }
                let EffectOp::MayPayManaThen {
                    player: PlayerRef::Controller,
                    colored,
                    generic,
                    then,
                } = pending_program(source, structural_path, observation)?
                else {
                    return Err(Error::UnsupportedPrompt);
                };
                if pending.controller != human {
                    return Err(Error::UnsupportedPrompt);
                }
                let mut cost = if generic == 0 {
                    String::new()
                } else {
                    format!("{{{generic}}}")
                };
                for c in colored {
                    cost.push_str(&format!("{{{}}}", color(c)));
                }
                if cost.is_empty() {
                    cost.push_str("{0}");
                }
                return Ok(format!(
                    "{} {cost} for {}{}{}",
                    if *value { "Pay" } else { "Decline to pay" },
                    handles.name(source)?,
                    if *value { ": " } else { "; do not " },
                    describe_effect(&then)?
                ));
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
            let pending = observation
                .projection
                .surface
                .engine_context
                .pending_spell_copy
                .as_ref()
                .ok_or(Error::UnsupportedPrompt)?;
            if pending.player != human
                || pending.parent.as_ref() != Some(source)
                || pending.stage != SpellCopyStageV2::Payment
            {
                return Err(Error::UnsupportedPrompt);
            }
            let root = (definition(source)?.spell_effect)().ok_or(Error::UnsupportedPrompt)?;
            let EffectOp::Sequence(ops) = root else {
                return Err(Error::UnsupportedPrompt);
            };
            if !matches!(
                ops.last(),
                Some(EffectOp::OfferAffectedPlayerSpellCopy {
                    affected: TargetRef::Target(0)
                })
            ) {
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
        A::OrderTriggers {
            pending_sources,
            order,
            ..
        } => {
            let pending = &observation
                .projection
                .surface
                .engine_context
                .pending_triggers;
            if pending_sources.len() != order.len()
                || pending_sources.len() < 2
                || pending.len() < order.len()
            {
                return Err(Error::UnsupportedPrompt);
            }
            let mut sorted = order.clone();
            sorted.sort_unstable();
            if sorted != (0..order.len()).collect::<Vec<_>>() {
                return Err(Error::UnsupportedPrompt);
            }
            let mut descriptions = Vec::new();
            let mut seen = Vec::new();
            for &index in order {
                let source = &pending_sources[index];
                let trigger = &pending[index];
                if trigger.source.as_ref() != Some(source)
                    || trigger.controller != human
                    || trigger.trigger_kind != PendingTriggerKindV2::TriggeredAbility
                {
                    return Err(Error::UnsupportedPrompt);
                }
                if seen.contains(source) {
                    return Err(Error::UnsupportedPrompt);
                }
                seen.push(source.clone());
                descriptions.push(format!("Trigger from {}", handles.name(source)?));
            }
            format!(
                "Put triggers on the stack from bottom to top (last resolves first): {}",
                descriptions.join("; then ")
            )
        }
        #[cfg(feature = "limited-fdn-fixtures")]
        A::ChooseTriggerOrderNext { .. } => return Err(Error::UnsupportedPrompt),
        // These visible records do not contain enough human meaning. In
        // particular effect option paths do not describe their outcomes, and
        // same-source triggers require frozen ability provenance, not an index.
        // Foundations custom-game mulligan, combat damage and legend-rule choices
        // never reach a Pauper Bo3 human prompt.
        A::ChooseEffectNumber { .. }
        | A::ChooseCombatDamageRange { .. }
        | A::ChooseLondonMulligan { .. }
        | A::ChooseLondonBottom { .. }
        | A::ChooseLegendPermanent { .. }
        | A::ChooseAttackTarget { .. }
        | A::Ambiguous { .. }
        | A::ChooseEntryColorV36 { .. }
        | A::ChooseDivineKickerV44 { .. }
        | A::ChooseDivineTargetV44 { .. }
        | A::FinishDivineTargetsV44 { .. } => return Err(Error::UnsupportedPrompt),
    })
}

/// Separate exact printed control label; predecessor labels are unchanged.
pub(super) fn label_employment_v18(
    action: &A,
    observation: &super::life_ward_v8::ProjectionPartsV8<'_>,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<String, Error> {
    if let A::CastSpell { source, .. } = action {
        if definition(source)?.name == "Involuntary Employment" {
            let selector = crate::rules_meaning_employment_v1::live_selector()
                .map_err(|_| Error::UnsupportedPrompt)?;
            crate::rules_meaning_employment_v1::meaning(
                &selector,
                source.card_db_id,
                crate::rules_meaning_employment_v1::RootV1::PrintedSpell,
            )
            .map_err(|_| Error::UnsupportedPrompt)?;
            if actor(action) != Some(human) {
                return Err(Error::UnsupportedPrompt);
            }
            return Ok(format!("Begin casting {} (printed cost {{3}}{{R}}): gain control of one target creature until cleanup, untap it, grant haste until cleanup, then create a Treasure. An illegal target stops the entire spell", handles.name(source)?));
        }
    }
    label_parts_v8(action, observation, handles, human)
}

/// Exact optional sacrifice wording is available only through the V29 producer.
pub(super) fn label_gourmand_v29(
    action: &A,
    observation: &crate::gourmand_public_v1::GourmandObservationV1,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<String, Error> {
    if let Some(menu) = &observation.sacrifice_menu {
        let selector = crate::rules_meaning_gourmand_v1::live_selector()
            .map_err(|_| Error::UnsupportedPrompt)?;
        crate::rules_meaning_gourmand_v1::meaning(&selector, menu.source_at_attack.card_db_id, 0)
            .map_err(|_| Error::UnsupportedPrompt)?;
        match action {
            A::ChooseEffectTarget {actor,source,target:crate::rl::TargetRefV1::Object {object:card},selected_count,min_targets,max_targets}
                if *actor==human && menu.payer==human && *source==menu.source_at_attack
                    && *selected_count as usize==menu.selected.len() && *min_targets==menu.minimum && *max_targets==menu.maximum
                    && menu.legal.contains(card) => return Ok(format!("Sacrifice {} for {}: draw one card; the original attacking incarnation cannot be blocked until cleanup",handles.name(card)?,handles.name(source)?)),
            A::FinishEffectSelection {actor,source,selected_count}
                if *actor==human && menu.payer==human && *source==menu.source_at_attack && *selected_count as usize==menu.selected.len()
                    && menu.can_finish && menu.selected.is_empty() => return Ok(format!("Decline the sacrifice for {}: do not draw or add an evasion rule",handles.name(source)?)),
            A::ChooseEffectTarget { .. } | A::FinishEffectSelection { .. }=>return Err(Error::UnsupportedPrompt),
            _=>{},
        }
    }
    if let A::CastSpell { actor, source } = action {
        if definition(source)?.name == "Vampire Gourmand" {
            if *actor != human {
                return Err(Error::UnsupportedPrompt);
            }
            let selector = crate::rules_meaning_gourmand_v1::live_selector()
                .map_err(|_| Error::UnsupportedPrompt)?;
            crate::rules_meaning_gourmand_v1::meaning(&selector, source.card_db_id, 0)
                .map_err(|_| Error::UnsupportedPrompt)?;
            return Ok(format!("Begin casting {} (printed cost {{1}}{{B}}): its attack offers an optional sacrifice of another controlled creature for one draw and original-incarnation evasion until cleanup",handles.name(source)?));
        }
    }
    let parts = super::life_ward_v8::ProjectionPartsV8 {
        projection: &observation.projection,
        own_hand: &observation.own_hand,
        known_library_cards: &observation.known_library_cards,
        known_hand_cards: &observation.known_hand_cards,
        known_face_down_cards: &observation.known_face_down_cards,
        extensions: &observation.extensions,
    };
    label_parts_v8(action, &parts, handles, human)
}

pub(super) fn label_strongbox_v32(
    action: &A,
    observation: &crate::strongbox_public_v1::StrongboxObservationV1,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<String, Error> {
    if let Some(menu) = &observation.selection_menu {
        let selector = crate::rules_meaning_strongbox_v1::live_selector()
            .map_err(|_| Error::UnsupportedPrompt)?;
        crate::rules_meaning_strongbox_v1::meaning(&selector, menu.source_at_entry.card_db_id, 0)
            .map_err(|_| Error::UnsupportedPrompt)?;
        match action {
            A::ChooseEffectTarget {actor,source,target:crate::rl::TargetRefV1::Object {object:card},selected_count,min_targets,max_targets}
                if *actor==human && menu.chooser==human && *source==menu.source_at_entry && *selected_count as usize==menu.selected.len()
                    && *min_targets==menu.minimum && *max_targets==menu.maximum && menu.legal.contains(card) =>
                return Ok(format!("Choose {} from {}'s newly exiled cards: may play it with ordinary timing and costs until the end of your next turn",handles.name(card)?,handles.name(source)?)),
            A::FinishEffectSelection {actor,source,selected_count}
                if *actor==human && menu.chooser==human && *source==menu.source_at_entry && menu.can_finish && *selected_count==1 && menu.selected.len()==1 =>
                return Ok(format!("Confirm {} for {}: ordinary play permission until the end of your next turn",handles.name(&menu.selected[0])?,handles.name(source)?)),
            A::ChooseEffectTarget {..}|A::FinishEffectSelection {..}=>return Err(Error::UnsupportedPrompt),_=>{}
        }
    }
    if let A::CastSpell { actor, source } = action {
        if definition(source)?.name == "Strongbox Raider" {
            if *actor != human {
                return Err(Error::UnsupportedPrompt);
            }
            let selector = crate::rules_meaning_strongbox_v1::live_selector()
                .map_err(|_| Error::UnsupportedPrompt)?;
            crate::rules_meaning_strongbox_v1::meaning(&selector, source.card_db_id, 0)
                .map_err(|_| Error::UnsupportedPrompt)?;
            return Ok(format!("Begin casting {} (printed cost {{2}}{{R}}{{R}}): its Raid entry exiles two cards and grants ordinary play permission to one until the end of your next turn",handles.name(source)?));
        }
    }
    let parts = super::life_ward_v8::ProjectionPartsV8 {
        projection: &observation.projection,
        own_hand: &observation.own_hand,
        known_library_cards: &observation.known_library_cards,
        known_hand_cards: &observation.known_hand_cards,
        known_face_down_cards: &observation.known_face_down_cards,
        extensions: &observation.extensions,
    };
    label_parts_v8(action, &parts, handles, human)
}

pub(super) fn label_fiery_v33(
    action: &A,
    observation: &crate::fiery_observation_v1::FieryObservationV1,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<String, Error> {
    if let Some(menu) = &observation.cast_menu {
        match action {
            A::ChooseTarget {
                actor,
                source,
                remaining,
                target: crate::rl::TargetRefV1::Object { object },
            } if *actor == human
                && menu.caster == human
                && *source == menu.source
                && *remaining == menu.remaining
                && menu.legal.contains(object) =>
            {
                return Ok(match menu.phase {
                    crate::fiery_public_v1::CastTargetPhaseV1::MandatoryCreature=>format!("Choose {} as the creature target of {}: five damage; if it would die this turn, exile it instead",handles.name(object)?,handles.name(source)?),
                    crate::fiery_public_v1::CastTargetPhaseV1::OptionalAttachedEquipment=>format!("Choose {} as the optional attached Equipment target of {}: exile only this selected Equipment",handles.name(object)?,handles.name(source)?),
                    _=>return Err(Error::UnsupportedPrompt),
                });
            }
            A::FinishTargetSelection {
                actor,
                source,
                selected_count: 1,
            } if *actor == human
                && menu.caster == human
                && *source == menu.source
                && menu.can_finish =>
            {
                return Ok(format!(
                    "Finish {} without an Equipment target",
                    handles.name(source)?
                ))
            }
            A::ChooseTarget { .. } | A::FinishTargetSelection { .. } => {
                return Err(Error::UnsupportedPrompt)
            }
            _ => {}
        }
    }
    if let A::CastSpell { actor, source } = action {
        if observation
            .roots
            .iter()
            .any(|root| root.card_id == source.card_db_id)
        {
            if *actor != human {
                return Err(Error::UnsupportedPrompt);
            }
            return Ok(format!("Begin casting {} (printed cost {{2}}{{R}}): five damage to one creature, exile up to one targeted Equipment attached to it, and exile that creature if it would die this turn",handles.name(source)?));
        }
    }
    let parts = super::life_ward_v8::ProjectionPartsV8 {
        projection: &observation.projection,
        own_hand: &observation.own_hand,
        known_library_cards: &observation.known_library_cards,
        known_hand_cards: &observation.known_hand_cards,
        known_face_down_cards: &observation.known_face_down_cards,
        extensions: &observation.extensions,
    };
    label_parts_v8(action, &parts, handles, human)
}

pub(super) fn label_krenko_v34(
    action: &A,
    observation: &crate::krenko_observation_v1::KrenkoObservationV1,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<String, Error> {
    match action {
        A::CastSpell { actor, source }
            if observation.roots.iter().any(|r| {
                r.card_id == source.card_db_id
                    && r.root == crate::rules_meaning_krenko_v1::RootV1::PrintedCreatureSpell
            }) =>
        {
            if *actor != human {
                return Err(Error::UnsupportedPrompt);
            }
            return Ok(format!(
                "Begin casting {} (printed cost {{2}}{{R}}{{R}}): legendary 3/3 Goblin Warrior",
                handles.name(source)?
            ));
        }
        A::ActivateAbility {
            actor,
            source,
            ability_index: 0,
            granted_ability,
        } if granted_ability.is_none()
            && observation.roots.iter().any(|r| {
                r.card_id == source.card_db_id
                    && r.root
                        == crate::rules_meaning_krenko_v1::RootV1::PrintedTapActivation {
                            ability_index: 0,
                        }
            }) =>
        {
            if *actor != human {
                return Err(Error::UnsupportedPrompt);
            }
            let row = observation
                .facts
                .sources
                .iter()
                .find(|r| r.source == *source)
                .ok_or(Error::UnsupportedPrompt)?;
            return Ok(format!("Tap {}: create 1/1 red Goblin tokens equal to your Goblin permanents at resolution (currently {})", handles.name(source)?, row.current_controlled_goblin_permanents));
        }
        _ => {}
    }
    let parts = super::life_ward_v8::ProjectionPartsV8 {
        projection: &observation.projection,
        own_hand: &observation.own_hand,
        known_library_cards: &observation.known_library_cards,
        known_hand_cards: &observation.known_hand_cards,
        known_face_down_cards: &observation.known_face_down_cards,
        extensions: &observation.extensions,
    };
    label_parts_v8(action, &parts, handles, human)
}

pub(super) fn label_fishing_pole_v39(
    action: &A,
    grant: Option<&crate::rl_session::fishing_pole_v39::GrantedLabelV39>,
    observation: &crate::fishing_pole_observation_v39::FishingPoleObservationV1,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<String, Error> {
    let pending_label = |index: usize,
                         source: &crate::rl::CardStableRefV1,
                         sources: &[crate::rl::CardStableRefV1]|
     -> Result<String, Error> {
        let occurrence = sources[..index]
            .iter()
            .filter(|previous| {
                previous.arena_id == source.arena_id
                    && previous.zone_change_count == source.zone_change_count
            })
            .count();
        let original = observation
            .facts
            .operations
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                row.trigger
                    && row.stage == crate::fishing_pole_public_v39::StageV39::Pending
                    && row.original_pole.arena_id == source.arena_id
                    && row.original_pole.zone_change_count == source.zone_change_count
            })
            .nth(occurrence);
        if let Some((ordinal, _)) = original {
            Ok(format!(
                "{} untap occurrence {}",
                handles.name(source)?,
                ordinal + 1
            ))
        } else {
            handles.name(source)
        }
    };
    if let A::OrderTriggers {
        actor,
        pending_sources,
        order,
    } = action
    {
        if *actor != human {
            return Err(Error::UnsupportedPrompt);
        }
        let names = order
            .iter()
            .map(|index| {
                pending_sources
                    .get(*index)
                    .ok_or(Error::InvalidVisibleReference)
                    .and_then(|source| pending_label(*index, source, pending_sources))
            })
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(format!(
            "Put triggers on the stack in this order (last resolves first): {}",
            names.join(", ")
        ));
    }
    if let A::CastSpell { actor, source } = action {
        if observation.roots.iter().any(|root| {
            root.card_id == source.card_db_id
                && root.root == crate::rules_meaning_fishing_pole_v39::RootV39::PrintedArtifactSpell
        }) {
            if *actor != human {
                return Err(Error::UnsupportedPrompt);
            }
            return Ok(format!("Begin casting {} (printed cost {{1}}): Equipment with equip {{2}}, a granted bait ability, and mandatory untap Fish trigger",handles.name(source)?));
        }
    }
    if let A::ActivateAbility {
        actor,
        source,
        ability_index,
        granted_ability,
        ..
    } = action
    {
        if granted_ability.is_none()
            && grant.is_none()
            && observation.roots.iter().any(|root| {
                root.card_id == source.card_db_id
                    && root.root
                        == crate::rules_meaning_fishing_pole_v39::RootV39::PrintedEquip {
                            ability_index: u16::from(*ability_index),
                        }
            })
        {
            if *actor != human {
                return Err(Error::UnsupportedPrompt);
            }
            return Ok(format!("Activate {} equip (printed cost {{2}}): attach this Equipment to target creature you control", handles.name(source)?));
        }
    }
    if let A::ActivateAbility { actor, source, .. } = action {
        if let Some(grant) = grant {
            if *actor != human {
                return Err(Error::UnsupportedPrompt);
            }
            let name = handles.name(source)?;
            if grant.fishing {
                return Ok(format!("Activate Fishing Pole-granted ability of {name}: printed cost {{1}}, tap this creature, and tap the chosen actual attached Fishing Pole; put one bait counter on that original Pole at resolution"));
            }
            return Ok(format!(
                "Activate granted ability of {name}: {}: {}",
                activation_cost(&grant.cost, &name)?,
                describe_effect(&grant.effect)?
            ));
        }
    }
    let parts = super::life_ward_v8::ProjectionPartsV8 {
        projection: &observation.projection,
        own_hand: &observation.own_hand,
        known_library_cards: &observation.known_library_cards,
        known_hand_cards: &observation.known_hand_cards,
        known_face_down_cards: &observation.known_face_down_cards,
        extensions: &observation.extensions,
    };
    label_parts_v8(action, &parts, handles, human)
}

#[cfg(test)]
mod variable_target_refusal_tests {
    use super::*;

    #[test]
    fn human_legacy_prompt_refuses_variable_declared_target_grant() {
        let op = EffectOp::GrantKeywordToDeclaredTargetsUntilEndOfTurn {
            target_spec: card_def::TargetSpec::KickerControlledCreatures,
            keyword: card_def::Keywords::INDESTRUCTIBLE,
        };
        assert!(matches!(
            describe_effect(&op),
            Err(Error::UnsupportedPrompt)
        ));
    }
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod next_five_native_label_tests {
    use super::*;
    use crate::{
        extra_turn_v1::tests::ready,
        ids::PlayerId,
        policy_observation_v6::tests::put,
        rl_session::{FastActorResponseV1, FastActorSessionV1},
        state::Zone,
    };

    fn fake_rows(
        session: &FastActorSessionV1,
    ) -> (
        crate::fake_observation_v50::FakeObservationV50,
        crate::rl_session::FastActorDecisionV1,
        Vec<A>,
    ) {
        let FastActorResponseV1::Decision(expected) = session.current_response() else {
            panic!("fixture did not expose an actual decision")
        };
        let (observation, actions, _) = session
            .fake_current_decision_input_v50(expected, expected.acting_player)
            .unwrap();
        (observation, expected, actions)
    }

    fn human_label(
        observation: &crate::fake_observation_v50::FakeObservationV50,
        action: &A,
        human: PlayerSeatV1,
    ) -> Result<String, Error> {
        let handles = Handles::from_parts(
            &observation.projection,
            &observation.own_hand,
            &observation.known_hand_cards,
            &observation.known_library_cards,
            None,
        )?;
        let parts = super::super::life_ward_v8::ProjectionPartsV8 {
            projection: &observation.projection,
            own_hand: &observation.own_hand,
            known_library_cards: &observation.known_library_cards,
            known_hand_cards: &observation.known_hand_cards,
            known_face_down_cards: &observation.known_face_down_cards,
            extensions: &observation.extensions,
        };
        label_parts_v8(action, &parts, &handles, human)
    }

    fn human_label_v59(
        observation: &crate::fake_observation_v50::FakeObservationV50,
        action: &A,
        human: PlayerSeatV1,
        facts: &crate::next_five_public_v59::FactsV59,
    ) -> Result<String, Error> {
        let handles = Handles::from_parts(
            &observation.projection,
            &observation.own_hand,
            &observation.known_hand_cards,
            &observation.known_library_cards,
            None,
        )?;
        let parts = super::super::life_ward_v8::ProjectionPartsV8 {
            projection: &observation.projection,
            own_hand: &observation.own_hand,
            known_library_cards: &observation.known_library_cards,
            known_hand_cards: &observation.known_hand_cards,
            known_face_down_cards: &observation.known_face_down_cards,
            extensions: &observation.extensions,
        };
        label_parts_v59(action, &parts, &handles, human, facts)
    }

    fn activate_and_restore(
        session: &mut FastActorSessionV1,
        expected: crate::rl_session::FastActorDecisionV1,
        index: usize,
    ) {
        let saved = session.snapshot_v1();
        session
            .step(expected.episode_id, expected.step, index as u32)
            .unwrap();
        session.restore_v1(&saved);
    }

    fn quiet_actual_step(state: &mut crate::state::GameState, decision: crate::engine::Decision) {
        use crate::engine::{Action, Decision};
        let action = match decision {
            Decision::CastSpellOrPass { .. } => Action::Pass,
            Decision::DeclareAttackers { .. } => Action::DeclareAttackers(vec![]),
            Decision::DeclareBlockers { .. } => Action::DeclareBlockers(vec![]),
            Decision::OrderTriggers { pending, .. } => {
                Action::OrderTriggers((0..pending.len()).collect())
            }
            other => panic!("unexpected native Replication fixture decision: {other:?}"),
        };
        crate::engine::step(state, action).unwrap();
    }

    fn paid_replication_at_own_upkeep(
        actor: PlayerId,
    ) -> (
        crate::state::GameState,
        crate::ids::ObjectId,
        crate::ids::ObjectId,
        crate::ids::ObjectId,
        crate::ids::ObjectId,
    ) {
        use crate::{
            engine::{self, Action, Decision},
            state::{Step, Target},
        };

        fn cast_native_fixture_spell(
            state: &mut crate::state::GameState,
            actor: PlayerId,
            spell: crate::ids::ObjectId,
            target: Option<crate::ids::ObjectId>,
        ) {
            let before = state.players[actor.index()]
                .battlefield
                .iter()
                .filter(|id| state.objects.get(**id).tapped)
                .count();
            let definition =
                &crate::card_def::CARD_DEFS[usize::from(state.objects.get(spell).card_def)];
            let (mana_value, spell_name) = (definition.mana_value, definition.name.to_string());
            assert!(matches!(
                engine::advance_until_decision(state),
                Decision::CastSpellOrPass { player, castable_spells, .. }
                    if player == actor && castable_spells.contains(&spell)
            ));
            engine::step(state, Action::CastSpell(spell)).unwrap();
            if let Some(target) = target {
                assert!(matches!(
                    engine::advance_until_decision(state),
                    Decision::ChooseTargets { spell: source, legal_targets, .. }
                        if source == spell && legal_targets.contains(&Target::Object(target))
                ));
                engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
            }
            assert!(matches!(
                engine::advance_until_decision(state),
                Decision::CastSpellOrPass { player, .. } if player == actor
            ));
            assert!(state.engine.pending_cast.is_none());
            assert_eq!(state.objects.get(spell).zone, Zone::Stack);

            for _ in 0..48 {
                let decision = engine::advance_until_decision(state);
                if state.objects.get(spell).zone == Zone::Battlefield
                    && state.stack.is_empty()
                    && state.engine.pending_cast.is_none()
                    && state.engine.pending_effect.is_none()
                    && state.engine.pending_triggers.is_empty()
                    && matches!(decision, Decision::CastSpellOrPass { player, .. } if player == actor)
                {
                    let after = state.players[actor.index()]
                        .battlefield
                        .iter()
                        .filter(|id| state.objects.get(**id).tapped)
                        .count();
                    assert_eq!(after - before, usize::from(mana_value));
                    return;
                }
                quiet_actual_step(state, decision);
            }
            panic!("paid fixture spell did not resolve: {spell_name}");
        }

        let mut state = ready(actor);
        crate::combat_damage_v1::enable_foundations_combat_v1(&mut state).unwrap();
        for _ in 0..3 {
            put(&mut state, actor, "Island", Zone::Battlefield);
        }
        for _ in 0..10 {
            put(&mut state, actor, "Forest", Zone::Battlefield);
        }
        let host = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let other_host = put(&mut state, actor, "Axgard Cavalry", Zone::Battlefield);
        let aura = put(&mut state, actor, "Bind the Monster", Zone::Hand);
        cast_native_fixture_spell(&mut state, actor, aura, Some(host));
        assert_eq!(state.objects.get(aura).zone, Zone::Battlefield);
        assert_eq!(state.objects.get(aura).v4.attached_to.unwrap().object, host);
        assert!(state.objects.get(host).attachments.contains(&aura));

        let season = put(&mut state, actor, "Doubling Season", Zone::Hand);
        cast_native_fixture_spell(&mut state, actor, season, None);

        let target = aura;
        let source = put(&mut state, actor, "Extravagant Replication", Zone::Hand);
        cast_native_fixture_spell(&mut state, actor, source, None);

        let mut found_upkeep_target = false;
        for _ in 0..256 {
            let decision = engine::advance_until_decision(&mut state);
            if matches!(decision, Decision::ChooseTargets { spell, .. } if spell == source)
                && state.active_player == actor
                && state.step == Step::Upkeep
            {
                found_upkeep_target = true;
                break;
            }
            quiet_actual_step(&mut state, decision);
        }
        assert!(
            found_upkeep_target,
            "paid Replication upkeep target did not appear"
        );
        (state, source, target, host, other_host)
    }

    fn paid_infernal_pair(actor: PlayerId) -> (crate::state::GameState, [crate::ids::ObjectId; 2]) {
        use crate::engine::{self, Action, Decision};
        let mut state = ready(actor);
        crate::combat_damage_v1::enable_foundations_combat_v1(&mut state).unwrap();
        for _ in 0..12 {
            put(&mut state, actor, "Swamp", Zone::Battlefield);
        }
        let mut vessels = Vec::new();
        for _ in 0..2 {
            let vessel = put(&mut state, actor, "Infernal Vessel", Zone::Hand);
            let mana_value = crate::card_def::CARD_DEFS
                [usize::from(state.objects.get(vessel).card_def)]
            .mana_value;
            let tapped_before = state.players[actor.index()]
                .battlefield
                .iter()
                .filter(|id| state.objects.get(**id).tapped)
                .count();
            assert!(matches!(
                engine::advance_until_decision(&mut state),
                Decision::CastSpellOrPass { player, castable_spells, .. }
                    if player == actor && castable_spells.contains(&vessel)
            ));
            engine::step(&mut state, Action::CastSpell(vessel)).unwrap();
            let mut resolved = false;
            for _ in 0..32 {
                let decision = engine::advance_until_decision(&mut state);
                if state.objects.get(vessel).zone == Zone::Battlefield
                    && state.stack.is_empty()
                    && state.engine.pending_cast.is_none()
                    && matches!(decision, Decision::CastSpellOrPass { player, .. } if player == actor)
                {
                    resolved = true;
                    break;
                }
                quiet_actual_step(&mut state, decision);
            }
            assert!(resolved, "paid Infernal Vessel did not resolve");
            let tapped_after = state.players[actor.index()]
                .battlefield
                .iter()
                .filter(|id| state.objects.get(**id).tapped)
                .count();
            assert_eq!(tapped_after - tapped_before, usize::from(mana_value));
            vessels.push(vessel);
        }
        let [first, second] = vessels.as_slice() else {
            unreachable!("exactly two paid Vessels were cast")
        };
        (state, [*first, *second])
    }

    #[test]
    fn restricted_giada_white_mana_labels_actual_paid_rows_and_restore_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            let giada = put(&mut state, actor, "Giada, Font of Hope", Zone::Battlefield);
            let mut session = FastActorSessionV1::from_fake_v50_fixture_state(state);
            let (observation, expected, actions) = fake_rows(&session);
            let human = expected.acting_player;
            let (index, action) = actions
                .iter()
                .enumerate()
                .find(|(_, action)| matches!(action,
                    A::ActivateManaAbility { actor: owner, source, mana_choice: Some(ManaColor::W), .. }
                    if *owner == human && source.arena_id == giada.0
                ))
                .expect("native Giada white-mana row");
            let label = human_label(&observation, action, human).unwrap();
            assert!(label.contains("add {W}"), "{label}");
            assert!(label.contains("only to cast Angel spells"), "{label}");

            activate_and_restore(&mut session, expected, index);
            let (restored_observation, restored_expected, restored_actions) = fake_rows(&session);
            let restored_human = restored_expected.acting_player;
            let (restored_index, restored_action) = restored_actions
                .iter()
                .enumerate()
                .find(|(_, action)| matches!(action,
                    A::ActivateManaAbility { actor: owner, source, mana_choice: Some(ManaColor::W), .. }
                    if *owner == restored_human && source.arena_id == giada.0
                ))
                .expect("restored native Giada white-mana row");
            assert_eq!(
                human_label(&restored_observation, restored_action, restored_human).unwrap(),
                label
            );
            activate_and_restore(&mut session, restored_expected, restored_index);
        }
    }

    #[test]
    fn midnight_snack_turn_gain_loss_labels_actual_activation_rows_and_restore_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            let snack = put(&mut state, actor, "Midnight Snack", Zone::Battlefield);
            state.players[actor.index()].mana_pool[ManaColor::B.pool_index()] = 1;
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 2;
            let mut session = FastActorSessionV1::from_fake_v50_fixture_state(state);
            let (observation, expected, actions) = fake_rows(&session);
            let human = expected.acting_player;
            let (index, action) = actions
                .iter()
                .enumerate()
                .find(|(_, action)| {
                    matches!(action,
                        A::ActivateAbility { actor: owner, source, ability_index: 0, .. }
                        if *owner == human && source.arena_id == snack.0
                    )
                })
                .expect("native Midnight Snack activation row");
            let label = human_label(&observation, action, human).unwrap();
            assert!(label.contains("sacrifice Midnight Snack"), "{label}");
            assert!(
                label.contains(
                    "chosen opponent loses life equal to the total life you have gained this turn"
                ),
                "{label}"
            );

            activate_and_restore(&mut session, expected, index);
            let (restored_observation, restored_expected, restored_actions) = fake_rows(&session);
            let restored_human = restored_expected.acting_player;
            let (restored_index, restored_action) = restored_actions
                .iter()
                .enumerate()
                .find(|(_, action)| {
                    matches!(action,
                        A::ActivateAbility { actor: owner, source, ability_index: 0, .. }
                        if *owner == restored_human && source.arena_id == snack.0
                    )
                })
                .expect("restored native Midnight Snack activation row");
            assert_eq!(
                human_label(&restored_observation, restored_action, restored_human).unwrap(),
                label
            );
            activate_and_restore(&mut session, restored_expected, restored_index);
        }
    }

    #[test]
    fn goblin_pending_target_and_x_labels_bind_to_actual_paid_cast_both_seats_and_restore() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            for _ in 0..10 {
                put(&mut state, actor, "Mountain", Zone::Battlefield);
            }
            let target = put(
                &mut state,
                actor.opponent(),
                "Axgard Cavalry",
                Zone::Battlefield,
            );
            let spell = put(&mut state, actor, "Goblin Negotiation", Zone::Hand);
            let mut session = FastActorSessionV1::from_fake_v50_fixture_state(state);

            let (_, cast_decision, cast_actions) = fake_rows(&session);
            let cast_index = cast_actions
                .iter()
                .position(|action| {
                    matches!(action,
                        A::CastSpell { actor: owner, source }
                        if *owner == cast_decision.acting_player && source.arena_id == spell.0
                    )
                })
                .expect("native Goblin Negotiation cast row");
            session
                .step(
                    cast_decision.episode_id,
                    cast_decision.step,
                    cast_index as u32,
                )
                .unwrap();

            let (target_observation, target_decision, target_actions) = fake_rows(&session);
            let target_facts =
                crate::next_five_public_v59::capture(session.game_state(), actor).unwrap();
            let target_index = target_actions
                .iter()
                .position(|action| matches!(action,
                    A::ChooseTarget { actor: owner, source, target: TargetRefV1::Object { object }, .. }
                    if *owner == target_decision.acting_player
                        && source.arena_id == spell.0
                        && object.arena_id == target.0
                ))
                .expect("native Goblin creature target row");
            let target_label = human_label_v59(
                &target_observation,
                &target_actions[target_index],
                target_decision.acting_player,
                &target_facts,
            )
            .unwrap();
            assert!(target_label.contains("creature target"), "{target_label}");
            assert!(
                target_label.contains("X is not selected yet"),
                "{target_label}"
            );
            session
                .step(
                    target_decision.episode_id,
                    target_decision.step,
                    target_index as u32,
                )
                .unwrap();

            let (x_observation, x_decision, x_actions) = fake_rows(&session);
            let x_facts =
                crate::next_five_public_v59::capture(session.game_state(), actor).unwrap();
            let x_index = x_actions
                .iter()
                .position(|action| {
                    matches!(action,
                        A::ChooseEffectOption { actor: owner, source, option_index: 3, .. }
                        if *owner == x_decision.acting_player && source.arena_id == spell.0
                    )
                })
                .expect("native Goblin X=3 row");
            let x_label = human_label_v59(
                &x_observation,
                &x_actions[x_index],
                x_decision.acting_player,
                &x_facts,
            )
            .unwrap();
            assert!(x_label.contains("X = 3"), "{x_label}");
            assert!(x_label.contains("Axgard Cavalry"), "{x_label}");
            assert!(
                x_label.contains("actual damage in excess of lethal"),
                "{x_label}"
            );

            let saved = session.snapshot_v1();
            session
                .step(x_decision.episode_id, x_decision.step, x_index as u32)
                .unwrap();
            assert!(session.game_state().engine.pending_cast.is_none());
            assert_eq!(session.game_state().objects.get(spell).zone, Zone::Stack);
            let paid_facts =
                crate::next_five_public_v59::capture(session.game_state(), actor).unwrap();
            let paid_cast = paid_facts
                .casts
                .iter()
                .find(|row| row.source.object == spell.0)
                .expect("paid Goblin cast fact");
            assert!(!paid_cast.payment_pending);
            assert_eq!(paid_cast.x, Some(3));

            session.restore_v1(&saved);
            let (restored_observation, restored_decision, restored_actions) = fake_rows(&session);
            let restored_facts =
                crate::next_five_public_v59::capture(session.game_state(), actor).unwrap();
            let restored_index = restored_actions
                .iter()
                .position(|action| {
                    matches!(action,
                        A::ChooseEffectOption { actor: owner, source, option_index: 3, .. }
                        if *owner == restored_decision.acting_player && source.arena_id == spell.0
                    )
                })
                .expect("restored native Goblin X=3 row");
            assert_eq!(
                human_label_v59(
                    &restored_observation,
                    &restored_actions[restored_index],
                    restored_decision.acting_player,
                    &restored_facts,
                )
                .unwrap(),
                x_label
            );
            session
                .step(
                    restored_decision.episode_id,
                    restored_decision.step,
                    restored_index as u32,
                )
                .unwrap();
            assert!(session.game_state().engine.pending_cast.is_none());
        }
    }

    #[test]
    fn replication_target_and_aura_host_label_actual_paid_upkeep_rows_and_restore_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (state, source, copy_target, host, other_host) =
                paid_replication_at_own_upkeep(actor);
            let mut session = FastActorSessionV1::from_fake_v50_fixture_state(state);
            let (target_observation, target_decision, target_actions) = fake_rows(&session);
            let target_facts =
                crate::next_five_public_v59::capture(session.game_state(), actor).unwrap();
            let target_index = target_actions
                .iter()
                .position(|action| matches!(action,
                    A::ChooseTarget { actor: owner, source: actual_source, target: TargetRefV1::Object { object }, .. }
                    if *owner == target_decision.acting_player
                        && actual_source.arena_id == source.0
                        && object.arena_id == copy_target.0
                ))
                .expect("native Replication upkeep target row");
            let target_label = human_label_v59(
                &target_observation,
                &target_actions[target_index],
                target_decision.acting_player,
                &target_facts,
            )
            .unwrap();
            assert!(
                target_label.contains("another nonland permanent"),
                "{target_label}"
            );
            assert!(target_label.contains("Bind the Monster"), "{target_label}");

            let saved_target = session.snapshot_v1();
            session
                .step(
                    target_decision.episode_id,
                    target_decision.step,
                    target_index as u32,
                )
                .unwrap();
            session.restore_v1(&saved_target);
            let (restored_observation, restored_decision, restored_actions) = fake_rows(&session);
            let restored_facts =
                crate::next_five_public_v59::capture(session.game_state(), actor).unwrap();
            let restored_target_index = restored_actions
                .iter()
                .position(|action| matches!(action,
                    A::ChooseTarget { actor: owner, source: actual_source, target: TargetRefV1::Object { object }, .. }
                    if *owner == restored_decision.acting_player
                        && actual_source.arena_id == source.0
                        && object.arena_id == copy_target.0
                ))
                .expect("restored native Replication upkeep target row");
            assert_eq!(
                human_label_v59(
                    &restored_observation,
                    &restored_actions[restored_target_index],
                    restored_decision.acting_player,
                    &restored_facts,
                )
                .unwrap(),
                target_label
            );
            session
                .step(
                    restored_decision.episode_id,
                    restored_decision.step,
                    restored_target_index as u32,
                )
                .unwrap();

            for host_prompt in 0usize..2 {
                let mut found_host = false;
                for _ in 0..16 {
                    let (observation, decision, actions) = fake_rows(&session);
                    let facts =
                        crate::next_five_public_v59::capture(session.game_state(), actor).unwrap();
                    if let Some((index, action)) = actions.iter().enumerate().find(|(_, action)| {
                        matches!(action,
                            A::ChooseEffectTarget {
                                actor: owner,
                                source: actual_source,
                                target: TargetRefV1::Object { object },
                                ..
                            }
                            if *owner == decision.acting_player
                                && actual_source.arena_id == source.0
                                && object.arena_id == host.0
                        )
                    }) {
                        let copy = facts.aura_copy.as_ref().expect("public Aura-copy stage");
                        assert_eq!(copy.expanded_count, 2);
                        assert_eq!(copy.chosen_hosts.len(), host_prompt);
                        assert_eq!(copy.remaining_count, 2 - host_prompt as u64);
                        assert_eq!(copy.chooser, actor.into());
                        assert!(copy
                            .candidate_hosts
                            .iter()
                            .any(|candidate| candidate.object == other_host.0));
                        let other_host_index = actions
                            .iter()
                            .position(|candidate| {
                                matches!(candidate,
                                    A::ChooseEffectTarget {
                                        source: actual_source,
                                        target: TargetRefV1::Object { object },
                                        ..
                                    }
                                    if actual_source.arena_id == source.0
                                        && object.arena_id == other_host.0
                                )
                            })
                            .expect("other legal Aura-host row remains available");
                        assert!(human_label_v59(
                            &observation,
                            &actions[other_host_index],
                            decision.acting_player,
                            &facts,
                        )
                        .unwrap()
                        .contains("Axgard Cavalry"));
                        if host_prompt == 1 {
                            assert!(copy
                                .chosen_hosts
                                .iter()
                                .any(|chosen| chosen.object == host.0));
                        }
                        let label =
                            human_label_v59(&observation, action, decision.acting_player, &facts)
                                .unwrap();
                        assert!(label.contains("Aura host"), "{label}");

                        let saved = session.snapshot_v1();
                        session
                            .step(decision.episode_id, decision.step, index as u32)
                            .unwrap();
                        if host_prompt == 1 {
                            session.restore_v1(&saved);
                            let (restored_observation, restored_decision, restored_actions) =
                                fake_rows(&session);
                            let restored_facts =
                                crate::next_five_public_v59::capture(session.game_state(), actor)
                                    .unwrap();
                            let restored_index = restored_actions
                                .iter()
                                .position(|candidate| candidate == action)
                                .expect("restored second native Aura-host row");
                            assert_eq!(
                                human_label_v59(
                                    &restored_observation,
                                    &restored_actions[restored_index],
                                    restored_decision.acting_player,
                                    &restored_facts,
                                )
                                .unwrap(),
                                label
                            );
                            session
                                .step(
                                    restored_decision.episode_id,
                                    restored_decision.step,
                                    restored_index as u32,
                                )
                                .unwrap();
                        }
                        found_host = true;
                        break;
                    }
                    let pass_index = actions
                        .iter()
                        .position(|action| matches!(action, A::Pass { actor } if *actor == decision.acting_player))
                        .expect("priority pass before Replication resolves");
                    session
                        .step(decision.episode_id, decision.step, pass_index as u32)
                        .unwrap();
                }
                assert!(found_host, "native Replication Aura-host choice absent");
            }
            let copied_aura = crate::card_def::card_id_by_name("Bind the Monster").unwrap();
            let copies = session
                .game_state()
                .objects
                .iter()
                .filter(|(_, object)| {
                    object.zone == Zone::Battlefield
                        && object.v4.is_token
                        && object.card_def == copied_aura
                        && object
                            .v4
                            .attached_to
                            .is_some_and(|link| link.object == host)
                })
                .count();
            assert_eq!(
                copies, 2,
                "both Season-expanded Aura copies use the chosen host"
            );
        }
    }

    #[test]
    fn infernal_death_order_labels_two_paid_native_triggers_and_restores_both_seats() {
        use crate::{
            engine::{self, Action, Decision},
            event::{self, ProposedEvent},
        };

        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, vessels) = paid_infernal_pair(actor);
            for vessel in vessels {
                let object = state.objects.get(vessel);
                assert_eq!(object.owner, actor);
                assert_eq!(object.controller, actor);
                assert_eq!(object.zone, Zone::Battlefield);
            }

            event::propose_and_commit_batch(
                &mut state,
                vessels
                    .into_iter()
                    .map(|vessel| ProposedEvent::zone_change(vessel, Zone::Graveyard))
                    .collect(),
            );
            assert!(vessels
                .iter()
                .all(|vessel| state.objects.get(*vessel).zone == Zone::Graveyard));
            assert!(matches!(
                engine::advance_until_decision(&mut state),
                Decision::OrderTriggers { player, pending } if player == actor && pending.len() == 2
            ));

            let mut session = FastActorSessionV1::from_fake_v50_fixture_state(state);
            let (observation, decision, actions) = fake_rows(&session);
            let facts = crate::next_five_public_v59::capture(session.game_state(), actor).unwrap();
            let held = facts
                .triggers
                .iter()
                .filter(|row| {
                    row.family == crate::rules_meaning_next_five_v59::FamilyV59::Infernal
                        && row.stage == crate::next_five_public_v59::TriggerStageV59::Held
                })
                .collect::<Vec<_>>();
            assert_eq!(held.len(), 2);
            for vessel in vessels {
                let trigger = held
                    .iter()
                    .find(|row| row.source.original_source.object == vessel.0)
                    .expect("actual Infernal death produced a held source-bound trigger");
                assert_eq!(trigger.source.captured_controller, actor.into());
                let death = facts
                    .infernal_deaths
                    .iter()
                    .find(|row| row.source.original_source.object == vessel.0)
                    .expect("actual Infernal death public fact");
                assert!(death.captured_not_demon && death.collection_completed);
                assert!(death.return_incarnation_available_now);
                assert_eq!(death.owner_on_return, actor.into());
                assert_eq!(death.source.original_source.owner, actor.into());
            }

            let order_index = actions
                .iter()
                .position(|action| matches!(action,
                    A::OrderTriggers { actor: chooser, pending_sources, order }
                    if *chooser == actor.into()
                        && pending_sources.len() == 2
                        && pending_sources.iter().map(|source| source.arena_id).collect::<Vec<_>>()
                            == vessels.iter().map(|vessel| vessel.0).collect::<Vec<_>>()
                        && order == &[0, 1]
                ))
                .expect("authentic native Infernal trigger-order action");
            let label = human_label_v59(
                &observation,
                &actions[order_index],
                decision.acting_player,
                &facts,
            )
            .unwrap();
            assert!(label.contains("bottom to top"), "{label}");
            assert_eq!(
                label.matches("Infernal Vessel trigger:").count(),
                2,
                "{label}"
            );
            assert!(label.contains("under its owner's control"), "{label}");

            let saved = session.snapshot_v1();
            session
                .step(decision.episode_id, decision.step, order_index as u32)
                .unwrap();
            let placed = crate::next_five_public_v59::capture(session.game_state(), actor).unwrap();
            assert_eq!(placed.triggers.len(), 2);
            assert!(placed.triggers.iter().all(|row| row.family
                == crate::rules_meaning_next_five_v59::FamilyV59::Infernal
                && row.stage == crate::next_five_public_v59::TriggerStageV59::Published
                && row.placement_ordered
                && row.source.captured_controller == actor.into()));

            session.restore_v1(&saved);
            let (restored_observation, restored_decision, restored_actions) = fake_rows(&session);
            let restored_facts =
                crate::next_five_public_v59::capture(session.game_state(), actor).unwrap();
            let restored_index = restored_actions
                .iter()
                .position(|action| action == &actions[order_index])
                .expect("restored native Infernal order action");
            assert_eq!(
                human_label_v59(
                    &restored_observation,
                    &restored_actions[restored_index],
                    restored_decision.acting_player,
                    &restored_facts,
                )
                .unwrap(),
                label
            );
            session
                .step(
                    restored_decision.episode_id,
                    restored_decision.step,
                    restored_index as u32,
                )
                .unwrap();
            let restored_placed =
                crate::next_five_public_v59::capture(session.game_state(), actor).unwrap();
            assert_eq!(restored_placed.triggers, placed.triggers);
        }
    }
}
