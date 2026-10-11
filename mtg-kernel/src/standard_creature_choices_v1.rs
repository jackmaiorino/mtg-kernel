//! Resolution choices and reflexive triggers for the remaining Standard creatures.
use crate::card_def::TargetSpec;
use crate::effect::{EffectOp, ExecCtx, ObjectRef, PlayerRef};
use crate::mana::ManaColor;
use crate::state::{GameState, Target};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CreatureChoiceV1 {
    FrillbackPayment,
    ZoralinePayment,
    GlissaCounters(u8),
    AegisCopy {
        host: crate::state::ObjectLinkV4,
        attachment_timestamp: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CounterKindV1 {
    PlusOne,
    MinusOne,
    MinusToughness,
    Stun,
    Lore,
    Oil,
    Lifelink,
    Time,
    Finality,
    Charge,
    Net,
    Loyalty,
}

impl CounterKindV1 {
    pub const ALL: [Self; 12] = [
        Self::PlusOne,
        Self::MinusOne,
        Self::MinusToughness,
        Self::Stun,
        Self::Lore,
        Self::Oil,
        Self::Lifelink,
        Self::Time,
        Self::Finality,
        Self::Charge,
        Self::Net,
        Self::Loyalty,
    ];
}

pub(crate) fn counter_count(
    state: &GameState,
    id: crate::ids::ObjectId,
    kind: CounterKindV1,
) -> i32 {
    let o = state.objects.get(id);
    match kind {
        CounterKindV1::PlusOne => i32::from(o.counters.plus1_plus1),
        CounterKindV1::MinusOne => i32::from(o.counters.minus1_minus1),
        CounterKindV1::MinusToughness => i32::from(o.counters.minus0_minus1),
        CounterKindV1::Stun => i32::from(o.counters.stun),
        CounterKindV1::Lore => i32::from(o.counters.lore),
        CounterKindV1::Oil => i32::from(o.counters.oil),
        CounterKindV1::Lifelink => i32::from(o.v4.lifelink_keyword_counters),
        CounterKindV1::Time => i32::from(o.v4.time_counters_v1),
        CounterKindV1::Finality => {
            i32::from(o.v4.creature_upgrade.as_ref().map_or(0, |u| u.finality))
        }
        #[cfg(feature = "standard-magezero-fixtures")]
        CounterKindV1::Charge => o.counters.charge,
        #[cfg(feature = "standard-magezero-fixtures")]
        CounterKindV1::Net => o.counters.net,
        #[cfg(feature = "standard-magezero-fixtures")]
        CounterKindV1::Loyalty => crate::planeswalker_v1::loyalty(state, id)
            .unwrap_or(0)
            .min(i32::MAX as u32) as i32,
        #[cfg(not(feature = "standard-magezero-fixtures"))]
        CounterKindV1::Charge | CounterKindV1::Net | CounterKindV1::Loyalty => 0,
    }
}

fn remove_counter(state: &mut GameState, id: crate::ids::ObjectId, kind: CounterKindV1) {
    if counter_count(state, id, kind) <= 0 {
        return;
    }
    let o = state.objects.get_mut(id);
    match kind {
        CounterKindV1::PlusOne => o.counters.plus1_plus1 -= 1,
        CounterKindV1::MinusOne => o.counters.minus1_minus1 -= 1,
        CounterKindV1::MinusToughness => o.counters.minus0_minus1 -= 1,
        CounterKindV1::Stun => o.counters.stun -= 1,
        CounterKindV1::Lore => o.counters.lore -= 1,
        CounterKindV1::Oil => o.counters.oil -= 1,
        CounterKindV1::Lifelink => o.v4.lifelink_keyword_counters -= 1,
        CounterKindV1::Time => o.v4.time_counters_v1 -= 1,
        CounterKindV1::Finality => o.v4.creature_upgrade.as_mut().unwrap().finality -= 1,
        #[cfg(feature = "standard-magezero-fixtures")]
        CounterKindV1::Charge => o.counters.charge -= 1,
        #[cfg(feature = "standard-magezero-fixtures")]
        CounterKindV1::Net => o.counters.net -= 1,
        #[cfg(feature = "standard-magezero-fixtures")]
        CounterKindV1::Loyalty => crate::planeswalker_v1::change_loyalty(state, id, -1),
        #[cfg(not(feature = "standard-magezero-fixtures"))]
        CounterKindV1::Charge | CounterKindV1::Net | CounterKindV1::Loyalty => {}
    }
}

pub(crate) fn options(kind: CreatureChoiceV1, ctx: &ExecCtx, state: &GameState) -> Vec<EffectOp> {
    let mut answers = vec![0];
    match kind {
        CreatureChoiceV1::AegisCopy {
            host,
            attachment_timestamp,
        } => {
            answers = (0..crate::standard_cards_v1::aegis_copy_candidates(
                state,
                ctx,
                host,
                attachment_timestamp,
            )
            .len())
                .map(|index| u8::try_from(index).expect("Aegis choices fit the action vocabulary"))
                .collect();
        }
        CreatureChoiceV1::FrillbackPayment => {
            answers.extend((1..=3).filter(|n| {
                crate::engine::can_pay_effect_mana(
                    ctx.controller,
                    &vec![ManaColor::G; *n as usize],
                    0,
                    state,
                )
            }));
        }
        CreatureChoiceV1::ZoralinePayment => {
            // Mana sources with life costs must remain affordable after reserving the two life.
            let mut projected = state.clone();
            if projected.players[ctx.controller.index()].life >= 2 {
                projected.players[ctx.controller.index()].life -= 2;
                if crate::engine::can_pay_effect_mana(
                    ctx.controller,
                    &[ManaColor::W, ManaColor::B],
                    0,
                    &projected,
                ) {
                    answers.push(1);
                }
            }
        }
        CreatureChoiceV1::GlissaCounters(remaining) => {
            if remaining > 0 && ctx.target_incarnation_matches(0, state) {
                if let Some(Target::Object(id)) = ctx.targets.first() {
                    answers.extend(CounterKindV1::ALL.iter().enumerate().filter_map(
                        |(i, counter)| {
                            (counter_count(state, *id, *counter) > 0).then_some(i as u8 + 1)
                        },
                    ));
                }
            }
        }
    }
    answers
        .into_iter()
        .map(|answer| EffectOp::CreatureChoiceAnswerV1 { kind, answer })
        .collect()
}

pub(crate) fn answer(
    kind: CreatureChoiceV1,
    answer: u8,
    ctx: &ExecCtx,
    state: &mut GameState,
) -> Result<Option<CreatureChoiceV1>, String> {
    if !options(kind, ctx, state).contains(&EffectOp::CreatureChoiceAnswerV1 { kind, answer }) {
        return Err("creature choice answer is no longer legal".into());
    }
    if answer == 0 && !matches!(kind, CreatureChoiceV1::AegisCopy { .. }) {
        return Ok(None);
    }
    match kind {
        CreatureChoiceV1::AegisCopy {
            host,
            attachment_timestamp,
        } => {
            let choices = crate::standard_cards_v1::aegis_copy_candidates(
                state,
                ctx,
                host,
                attachment_timestamp,
            );
            let chosen = choices[usize::from(answer)];
            crate::standard_cards_v1::apply_aegis_copy(
                state,
                ctx,
                host,
                attachment_timestamp,
                chosen,
            );
        }
        CreatureChoiceV1::GlissaCounters(remaining) => {
            let Some(Target::Object(id)) = ctx.targets.first() else {
                return Err("counter choice lost its target".into());
            };
            remove_counter(state, *id, CounterKindV1::ALL[usize::from(answer - 1)]);
            return Ok((remaining > 1).then_some(CreatureChoiceV1::GlissaCounters(remaining - 1)));
        }
        CreatureChoiceV1::FrillbackPayment | CreatureChoiceV1::ZoralinePayment => {
            let mut projected = state.clone();
            let colored = if kind == CreatureChoiceV1::FrillbackPayment {
                vec![ManaColor::G; usize::from(answer)]
            } else {
                vec![ManaColor::W, ManaColor::B]
            };
            if kind == CreatureChoiceV1::ZoralinePayment {
                crate::event::propose_and_commit(
                    &mut projected,
                    crate::event::ProposedEvent::life_payment(ctx.controller, 2),
                );
            }
            if !crate::engine::pay_effect_mana(ctx.controller, &colored, 0, &mut projected) {
                return Err("creature payment became unaffordable".into());
            }
            let source = ctx
                .ability_source_contract
                .ok_or("reflexive trigger lost its source contract")?;
            let effect = if kind == CreatureChoiceV1::FrillbackPayment {
                frillback_marker(answer)
            } else {
                zoraline_return()
            };
            let spec = trigger_target_spec(source.card_def, &effect)
                .ok_or("reflexive trigger is not definition-owned")?;
            projected
                .engine
                .pending_triggers
                .push(crate::trigger::PendingTrigger {
                    controller: ctx.controller,
                    source: ctx.source,
                    effect,
                    target_spec: spec,
                    is_madness_offer: false,
                    kicked: false,
                    targets: vec![],
                    target_contracts: vec![],
                    placement_ordered: false,
                    source_contract: Some(source),
                    granted_by: None,
                    optional_additional_cost_paid: None,
                    paid_cost_refs: vec![],
                });
            *state = projected;
        }
    }
    Ok(None)
}

pub(crate) fn glissa_modes() -> Vec<(TargetSpec, EffectOp)> {
    vec![
        (
            TargetSpec::None,
            EffectOp::Sequence(vec![
                EffectOp::DrawCards {
                    player: PlayerRef::Controller,
                    count: 1,
                },
                EffectOp::LoseLife {
                    player: PlayerRef::Controller,
                    amount: 1,
                },
            ]),
        ),
        (
            TargetSpec::EnchantmentPermanent,
            EffectOp::DestroyObject {
                object: ObjectRef::Target(0),
            },
        ),
        (
            TargetSpec::AnyPermanent,
            EffectOp::CreatureChoiceV1(CreatureChoiceV1::GlissaCounters(3)),
        ),
    ]
}
pub(crate) fn glissa_marker() -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: glissa_modes().into_iter().map(|(_, op)| op).collect(),
    }
}
pub(crate) fn frillback_modes(paid: u8) -> Vec<(TargetSpec, EffectOp)> {
    (0u8..8)
        .filter(|mask| mask.count_ones() <= u32::from(paid))
        .map(|mask| {
            let spec = match mask & 3 {
                0 => TargetSpec::None,
                1 => TargetSpec::ArtifactOrEnchantmentPermanent,
                2 => TargetSpec::AnyPlayer,
                _ => TargetSpec::ArtifactOrEnchantmentThenPlayer,
            };
            let mut ops = vec![];
            if mask & 1 != 0 {
                ops.push(EffectOp::Conditional {
                    cond: crate::effect::EffectCond::TargetIsLegalForAbility {
                        index: 0,
                        spec: TargetSpec::ArtifactOrEnchantmentPermanent,
                    },
                    then: Box::new(EffectOp::DestroyObject {
                        object: ObjectRef::Target(0),
                    }),
                    else_: Box::new(EffectOp::Sequence(vec![])),
                });
            }
            if mask & 2 != 0 {
                ops.push(EffectOp::ExilePlayersGraveyard {
                    player: PlayerRef::Target(if mask & 1 != 0 { 1 } else { 0 }),
                });
            }
            if mask & 4 != 0 {
                ops.push(EffectOp::GainLife {
                    player: PlayerRef::Controller,
                    amount: 4,
                });
            }
            (spec, EffectOp::Sequence(ops))
        })
        .collect()
}
pub(crate) fn frillback_marker(paid: u8) -> EffectOp {
    EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: frillback_modes(paid)
            .into_iter()
            .map(|(_, op)| op)
            .collect(),
    }
}
pub(crate) fn zoraline_return() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::ReturnTargetPermanentToBattlefield { target_index: 0 },
        EffectOp::CreatureUpgrade(
            crate::standard_creatures_v1::CreatureEffectV1::FinalityReturnedTarget,
        ),
    ])
}

pub(crate) fn trigger_modes(
    card_def: u16,
    effect: &EffectOp,
) -> Option<Vec<(TargetSpec, EffectOp)>> {
    match crate::card_def::CARD_DEFS.get(card_def as usize)?.name {
        "Glissa Sunslayer" if *effect == glissa_marker() => Some(glissa_modes()),
        "Tranquil Frillback" => (1..=3)
            .find(|paid| *effect == frillback_marker(*paid))
            .map(frillback_modes),
        _ => None,
    }
}
pub(crate) fn trigger_target_spec(card_def: u16, effect: &EffectOp) -> Option<TargetSpec> {
    if trigger_modes(card_def, effect).is_some() {
        return Some(TargetSpec::None);
    }
    match crate::card_def::CARD_DEFS.get(card_def as usize)?.name {
        "Assimilation Aegis"
            if matches!(
                effect,
                EffectOp::CreatureChoiceV1(CreatureChoiceV1::AegisCopy { .. })
            ) =>
        {
            Some(TargetSpec::None)
        }
        "Glissa Sunslayer" => glissa_modes()
            .into_iter()
            .find(|(_, op)| op == effect)
            .map(|(spec, _)| spec),
        "Tranquil Frillback" => frillback_modes(3)
            .into_iter()
            .find(|(_, op)| op == effect)
            .map(|(spec, _)| spec),
        "Zoraline, Cosmos Caller" if *effect == zoraline_return() => {
            Some(TargetSpec::NonlandPermanentCardInOwnGraveyardManaValueAtMost(3))
        }
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CreatureChoiceOptionV1 {
    Decline,
    PayGreen(u8),
    PayWhiteBlackAndTwoLife,
    RemoveCounter(CounterKindV1),
    CopyExiledCreature { card_def: u16 },
}
impl CreatureChoiceOptionV1 {
    pub fn label(self) -> String {
        match self {
            Self::CopyExiledCreature { card_def } => format!(
                "copy {}",
                crate::card_def::CARD_DEFS[usize::from(card_def)].name
            ),
            Self::Decline => "decline or stop".into(),
            Self::PayGreen(n) => format!("pay {}", "{G}".repeat(usize::from(n))),
            Self::PayWhiteBlackAndTwoLife => "pay {W}{B} and 2 life".into(),
            Self::RemoveCounter(counter) => format!(
                "remove one {} counter",
                match counter {
                    CounterKindV1::PlusOne => "+1/+1",
                    CounterKindV1::MinusOne => "-1/-1",
                    CounterKindV1::MinusToughness => "-0/-1",
                    CounterKindV1::Stun => "stun",
                    CounterKindV1::Lore => "lore",
                    CounterKindV1::Oil => "oil",
                    CounterKindV1::Lifelink => "lifelink",
                    CounterKindV1::Time => "time",
                    CounterKindV1::Finality => "finality",
                    CounterKindV1::Charge => "charge",
                    CounterKindV1::Net => "net",
                    CounterKindV1::Loyalty => "loyalty",
                }
            ),
        }
    }
}
pub(crate) fn public_option(
    op: &EffectOp,
    ctx: &ExecCtx,
    state: &GameState,
) -> Option<CreatureChoiceOptionV1> {
    let EffectOp::CreatureChoiceAnswerV1 { kind, answer } = *op else {
        return None;
    };
    if answer == 0 && !matches!(kind, CreatureChoiceV1::AegisCopy { .. }) {
        return Some(CreatureChoiceOptionV1::Decline);
    }
    match kind {
        CreatureChoiceV1::AegisCopy {
            host,
            attachment_timestamp,
        } => {
            crate::standard_cards_v1::aegis_copy_candidates(state, ctx, host, attachment_timestamp)
                .get(usize::from(answer))
                .map(|chosen| CreatureChoiceOptionV1::CopyExiledCreature {
                    card_def: state.objects.get(chosen.object).card_def,
                })
        }
        CreatureChoiceV1::FrillbackPayment => Some(CreatureChoiceOptionV1::PayGreen(answer)),
        CreatureChoiceV1::ZoralinePayment => Some(CreatureChoiceOptionV1::PayWhiteBlackAndTwoLife),
        CreatureChoiceV1::GlissaCounters(_) => CounterKindV1::ALL
            .get(usize::from(answer - 1))
            .copied()
            .map(CreatureChoiceOptionV1::RemoveCounter),
    }
}
