//! Meaning tables for trigger conditions and cost components.
//!
//! Trigger conditions are read against `trigger::trigger_matches` (the event
//! each condition accepts) and the collection loop that calls it (per-turn
//! limits). An intervening-if threshold checked at trigger time is emitted as
//! `ControlF::Conditional` plus the read it performs, using the same read
//! shape `reads::effect_cond` uses for the matching resolution-time recheck.
//!
//! Cost components are read against `engine::can_pay_components` (what must
//! exist to pay) and `engine::pay_cost_components` (what payment does). Every
//! cost is paid by the ability's controller, so `CostAtom` carries no player.

use super::*;

/// The bucket `AmtF::fixed` assigns to `n`.
fn bucket(n: i64) -> u8 {
    match AmtF::fixed(n) {
        AmtF::Fixed(bucket) => bucket,
        AmtF::Unit
        | AmtF::X
        | AmtF::Dynamic
        | AmtF::All
        | AmtF::UntilType(_)
        | AmtF::Half
        | AmtF::Minus(_) => unreachable!("AmtF::fixed returns Fixed for a non-negative count"),
    }
}

/// An intervening-if threshold: the trigger only fires when the read meets
/// at least `minimum`.
fn threshold(out: &mut Collector, player: RelF, zone: ZoneF, obj: ObjF, minimum: u8) {
    out.control(ControlF::Conditional);
    out.read(
        player,
        Some(zone),
        Some(obj),
        AggF::AtLeast(bucket(i64::from(minimum))),
    );
}

/// The event a triggered ability waits for, plus any intervening reads.
pub(crate) fn trigger_condition(condition: TriggerCondition, out: &mut Collector) {
    match condition {
        TriggerCondition::Etb => {
            // Any zone change of the source itself to the battlefield.
            out.trigger(TrigF::SelfEnters);
        }
        TriggerCondition::EtbControlsOtherSubtypeCount {
            subtype,
            minimum_count,
        } => {
            // The source enters, and at that moment the controller's
            // battlefield holds at least `minimum_count` other permanents
            // with the effective subtype. Same read as
            // `EffectCond::ControlsOtherSubtypeCount`.
            // Vocabulary gap: ObjF has no subtype class.
            let _ = subtype;
            out.trigger(TrigF::SelfEnters);
            threshold(
                out,
                RelF::You,
                ZoneF::Battlefield,
                ObjF::Permanent,
                minimum_count,
            );
        }
        TriggerCondition::EtbIfGraveyardCreatureCardsAtLeast(minimum) => {
            // The source enters while the controller's graveyard holds at
            // least `minimum` nontoken creature cards.
            out.trigger(TrigF::SelfEnters);
            threshold(
                out,
                RelF::You,
                ZoneF::Graveyard,
                ObjF::Typed(CardTypeF::Creature),
                minimum,
            );
        }
        TriggerCondition::DealsDamage => {
            // Any damage event whose source is this object: combat or not,
            // to a player or a permanent.
            out.trigger(TrigF::DealsDamage {
                combat: false,
                to_player: false,
                host: false,
            });
        }
        TriggerCondition::CastInstantOrSorcery => {
            // A spell the controller casts whose selected types include
            // Instant or Sorcery. Vocabulary gap: no two-type union class;
            // nearest is the instant class, as in
            // `reads::card_type_predicate`.
            out.trigger(TrigF::SpellCast {
                by: RelF::You,
                obj: ObjF::Typed(CardTypeF::Instant),
            });
        }
        TriggerCondition::CastNoncreatureSpell => {
            // A spell the controller casts whose selected types exclude
            // Creature. Vocabulary gap: no noncreature class; nearest is
            // any spell.
            out.trigger(TrigF::SpellCast {
                by: RelF::You,
                obj: ObjF::Spell,
            });
        }
        TriggerCondition::CastSelf => {
            // The cast event's spell is the source itself, cast by its
            // controller. The stack home zone is emitted by the caller.
            out.trigger(TrigF::SelfCast);
        }
        TriggerCondition::DrawNth(n) => {
            // A card draw by the controller that is exactly their `n`th
            // draw this turn.
            out.trigger(TrigF::Draw {
                by: RelF::You,
                nth: bucket(i64::from(n)),
            });
        }
        TriggerCondition::LeftBattlefieldToGraveyard => {
            // The source's own battlefield -> graveyard zone change.
            out.trigger(TrigF::SelfLeaves {
                to: Some(ZoneF::Graveyard),
            });
        }
        TriggerCondition::LeftBattlefield => {
            // The source's own zone change from the battlefield to any zone.
            out.trigger(TrigF::SelfLeaves { to: None });
        }
        TriggerCondition::SacrificeAnotherWithSubtype(subtype) => {
            // A sacrifice event for a permanent other than the source that
            // the controller controlled, carrying the subtype just before
            // it left. Vocabulary gap: ObjF has no subtype class.
            let _ = subtype;
            out.trigger(TrigF::Sacrifice {
                obj: ObjF::Permanent,
            });
        }
        TriggerCondition::SacrificeAnotherPermanent => {
            // A sacrifice event for any permanent other than the source
            // that the controller controlled.
            out.trigger(TrigF::Sacrifice {
                obj: ObjF::Permanent,
            });
        }
        TriggerCondition::DealsCombatDamageToPlayer => {
            // The source's exact incarnation dealt combat damage to a player.
            out.trigger(TrigF::DealsDamage {
                combat: true,
                to_player: true,
                host: false,
            });
        }
        TriggerCondition::BeginningOfUpkeep { controller_only } => {
            out.trigger(TrigF::StepBegins {
                step: StepF::Upkeep,
                yours_only: controller_only,
            });
        }
        TriggerCondition::ControllerDraws => {
            // Every card the controller draws, each card of a multi-card
            // draw separately. `nth: 0` marks "every draw" (no ordinal).
            out.trigger(TrigF::Draw {
                by: RelF::You,
                nth: 0,
            });
        }
        TriggerCondition::OtherControlledCreatureEnters { subtype } => {
            // A creature other than the source enters under the controller's
            // control. Vocabulary gap: ObjF has no subtype class, so the
            // optional subtype restriction on the entrant is not represented.
            let _ = subtype;
            out.trigger(TrigF::OtherEnters {
                obj: ObjF::Typed(CardTypeF::Creature),
            });
        }
        TriggerCondition::Attacks => {
            // The source's exact incarnation is declared as an attacker.
            out.trigger(TrigF::Attacks);
        }
        TriggerCondition::ControlledLandEnters => {
            // A land enters under the controller's control. The matcher does
            // not exclude the source, but no other class is nearer.
            out.trigger(TrigF::OtherEnters {
                obj: ObjF::Typed(CardTypeF::Land),
            });
        }
        TriggerCondition::ControllerGainsLife => {
            // A positive life gain event for the controller.
            out.trigger(TrigF::LifeGained { by: RelF::You });
        }
        TriggerCondition::ControllerAddedPlusOneCountersToSelf { max_per_turn } => {
            // The controller put one or more +1/+1 counters on the source's
            // current incarnation. `max_per_turn` caps how many times the
            // collection loop lets this ability trigger per source
            // incarnation per turn. Vocabulary gap: no per-turn trigger
            // limit facet.
            let _ = max_per_turn;
            out.trigger(TrigF::CountersPlaced);
        }
        TriggerCondition::AttacksWithControllerGraveyardCardCountAtLeast(minimum) => {
            // The source is declared as an attacker while its controller's
            // graveyard holds at least `minimum` nontoken, noncopy cards they
            // own. Same read as `EffectCond::ControllerGraveyardCardCountAtLeast`.
            out.trigger(TrigF::Attacks);
            threshold(out, RelF::You, ZoneF::Graveyard, ObjF::AnyCard, minimum);
        }
        TriggerCondition::BeginningControllerEndStepIfCreatureDied => {
            // The controller's end step begins, gated on the event's flag
            // that a creature (either player's) died this turn. Same read
            // as `EffectCond::CreatureDiedThisTurn`.
            out.trigger(TrigF::StepBegins {
                step: StepF::EndStep,
                yours_only: true,
            });
            out.control(ControlF::Conditional);
            out.read(
                RelF::EachPlayer,
                Some(ZoneF::Graveyard),
                Some(ObjF::Typed(CardTypeF::Creature)),
                AggF::EventThisTurn,
            );
        }
        TriggerCondition::BeginningControllerEndStep => {
            out.trigger(TrigF::StepBegins {
                step: StepF::EndStep,
                yours_only: true,
            });
        }
        TriggerCondition::EquippedCreatureDealsCombatDamageToPlayer => {
            // The source's exact current host dealt combat damage to a player.
            out.trigger(TrigF::DealsDamage {
                combat: true,
                to_player: true,
                host: true,
            });
        }
    }
}

/// `count` chosen objects moved as part of paying.
fn move_cost(out: &mut Collector, from: ZoneF, to: ZoneF, obj: ObjF, count: u8) {
    out.cost(CostAtom::MoveObject {
        from,
        to,
        obj,
        amount: AmtF::fixed(i64::from(count)),
    });
}

/// The source itself moved as part of paying. No quantity: `AmtF::Unit`,
/// the same atom the mana-ability sacrifice-self cost emits in `sources.rs`,
/// so the same engine act encodes identically in both places.
fn move_self_cost(out: &mut Collector, from: ZoneF, to: ZoneF) {
    out.cost(CostAtom::MoveObject {
        from,
        to,
        obj: ObjF::ThisObject,
        amount: AmtF::Unit,
    });
}

/// One component of an activation, flashback or additional cost.
pub(crate) fn cost_component(component: CostComponent, out: &mut Collector) {
    match component {
        CostComponent::Tap => {
            // Taps the source (summoning-sickness rule checked to pay).
            out.cost(CostAtom::Tap);
        }
        CostComponent::SacrificeSelf => {
            // Logs a sacrifice of the source and moves it to the graveyard.
            move_self_cost(out, ZoneF::Battlefield, ZoneF::Graveyard);
        }
        CostComponent::ExileSelf => {
            // Moves the source to exile from whatever zone it is activated
            // from. Vocabulary gap: this table does not receive the
            // activation zone; Battlefield is used, and an ability activated
            // elsewhere (e.g. from the graveyard) also carries its
            // `ActivatedFrom` zone. Every pool ability with this component
            // is activated from the battlefield (build.rs recipes).
            move_self_cost(out, ZoneF::Battlefield, ZoneF::Exile);
        }
        CostComponent::DiscardSelf => {
            // The source, which must be in its owner's hand, moves to the
            // graveyard.
            move_self_cost(out, ZoneF::Hand, ZoneF::Graveyard);
        }
        CostComponent::DiscardCards(n) => {
            // `n` other cards from hand, chosen by the payer through the
            // pending-discard staging, move to the graveyard.
            move_cost(out, ZoneF::Hand, ZoneF::Graveyard, ObjF::AnyCard, n);
        }
        CostComponent::SacrificeLands(n) => {
            move_cost(
                out,
                ZoneF::Battlefield,
                ZoneF::Graveyard,
                ObjF::Typed(CardTypeF::Land),
                n,
            );
        }
        CostComponent::SacrificeControlled { count, filter } => {
            // The component itself restricts candidates to the payer's
            // permanents, so the filter's relation adds nothing.
            let (obj, relation) = reads::permanent_filter(filter);
            let _ = relation;
            move_cost(out, ZoneF::Battlefield, ZoneF::Graveyard, obj, count);
        }
        CostComponent::Mana(cost) => {
            // An ordinary mana payment solved by `mana::solve`. A zero cost
            // pays nothing. Vocabulary gap: `CostAtom::Mana` carries no
            // amount, colors or X, so the size of the payment is not
            // represented.
            let crate::mana::Cost {
                pips,
                generic,
                x_count,
            } = cost;
            if !pips.is_empty() || generic > 0 || x_count > 0 {
                out.cost(CostAtom::Mana);
            }
        }
        CostComponent::PayLife(amount) => {
            // A life-loss event of exactly `amount` for the payer.
            out.cost(CostAtom::PayLife(bucket(i64::from(amount))));
        }
        CostComponent::ExileOtherCardsFromOwnGraveyard(n) => {
            // `n` nontoken cards the payer owns, other than the source, move
            // from their graveyard to exile.
            move_cost(out, ZoneF::Graveyard, ZoneF::Exile, ObjF::AnyCard, n);
        }
        CostComponent::ReturnControlledPermanentToOwnersHand(filter) => {
            // One permanent the payer controls matching the filter moves to
            // its owner's hand (and leaves combat).
            let (obj, relation) = reads::permanent_filter_def(filter);
            let _ = relation; // Control is checked by this component.
            move_cost(out, ZoneF::Battlefield, ZoneF::Hand, obj, 1);
        }
        CostComponent::TapOtherUntappedControlledPermanentWithSubtype(subtype) => {
            // Taps one other untapped permanent the payer controls with the
            // subtype. Vocabulary gap: `TapOthers` carries no object class.
            let _ = subtype;
            out.cost(CostAtom::TapOthers);
        }
        CostComponent::TapUntappedControlledPermanent(filter) => {
            // Taps one untapped permanent the payer controls matching the
            // filter. Vocabulary gap: `TapOthers` carries no object class.
            let _ = filter;
            out.cost(CostAtom::TapOthers);
        }
        CostComponent::RevealHandIfNoCardsWithType(card_type) => {
            // Payable only while the payer's hand has no card of the type;
            // paying reveals every card in that hand to both players.
            // Vocabulary gap: `CostAtom` has no reveal or hand-condition
            // form; nearest are the read and the reveal event themselves.
            out.control(ControlF::Conditional);
            out.read(
                RelF::You,
                Some(ZoneF::Hand),
                Some(ObjF::from(card_type)),
                AggF::AtLeast(bucket(1)),
            );
            let mut reveal = EffectAtom::new(EvF::Reveal)
                .player(RelF::You)
                .obj(ObjF::AnyCard)
                .amount(AmtF::All);
            reveal.from = Some(ZoneF::Hand);
            out.effect(reveal);
        }
        CostComponent::ReturnControlledUnblockedAttackerToOwnersHand => {
            // One unblocked attacking creature the payer controls moves to
            // its owner's hand (and leaves combat). The post-blockers timing
            // gate this component carries has no facet.
            move_cost(
                out,
                ZoneF::Battlefield,
                ZoneF::Hand,
                ObjF::Typed(CardTypeF::Creature),
                1,
            );
        }
        CostComponent::ChooseControlledCreatureOrRevealCreatureCardFromHand => {
            // The caster chooses between two zones, then one creature: a
            // creature they control (nothing happens to it) or a creature
            // card in their hand, which is revealed to both players. The
            // chosen object is frozen on the spell for its effect to read.
            // Vocabulary gap: `CostAtom` has no choose or reveal form.
            out.control(ControlF::ChooseBranch);
            out.control(ControlF::ChooseObjects);
            let mut reveal = EffectAtom::new(EvF::Reveal)
                .player(RelF::You)
                .obj(ObjF::Typed(CardTypeF::Creature))
                .amount(AmtF::fixed(1));
            reveal.from = Some(ZoneF::Hand);
            out.effect(reveal);
        }
    }
}
