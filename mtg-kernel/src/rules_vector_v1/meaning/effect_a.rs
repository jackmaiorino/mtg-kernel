//! Meaning table, EffectOp slice A: composition primitives, damage, life,
//! draw/discard, single-object zone changes, tapping, attachment, counters,
//! mana and tokens.
//!
//! Every arm below was written against the variant's executor in
//! `effect::execute` (and, for the deferred leaves, the engine's
//! `pending_discard` / `pending_optional_cost` consumers). Every variant in
//! this slice is reachable from an authored program (build.rs card programs,
//! trigger.rs templates or card_def.rs), so none is runtime-only.

use super::*;

/// Where the object behind `Target(slot)` lives when the program refers to
/// it, and whose it is when the spec restricts that. `None` zone: the slot
/// denotes a player (or nothing). `None` relation: either player's object.
///
/// Local to this slice because `targets::target_slot_obj` only exposes the
/// object class; zone changes need the origin zone (a stack target moved
/// away is a countered spell, a graveyard target moved to the battlefield is
/// a reanimation).
fn target_slot_origin(spec: TargetSpec, slot: u8) -> (Option<ZoneF>, Option<RelF>) {
    const BATTLEFIELD: Option<ZoneF> = Some(ZoneF::Battlefield);
    const STACK: Option<ZoneF> = Some(ZoneF::Stack);
    const GRAVEYARD: Option<ZoneF> = Some(ZoneF::Graveyard);
    match spec {
        TargetSpec::StandardV1(filter) => targets::standard_target_origin(filter),
        TargetSpec::None => (None, None),
        TargetSpec::CardInOwnGraveyardWithAnySubtype(_) => (GRAVEYARD, Some(RelF::You)),
        TargetSpec::OpponentArtifactEnchantmentOrNonbasicLand => {
            (BATTLEFIELD, Some(RelF::Opponent))
        }
        TargetSpec::LegendaryCreature
        | TargetSpec::AnotherArtifactOrCreature
        | TargetSpec::UpToTwoOtherCreaturesDifferentControllers => (BATTLEFIELD, None),
        // Slot 0 is a creature or a player; only the creature has a zone.
        TargetSpec::AnyTarget => (BATTLEFIELD, None),
        // Slot 0 is the player, slot 1 a creature that player controls.
        TargetSpec::PlayerThenTheirCreature => match slot {
            0 => (None, None),
            1..=u8::MAX => (BATTLEFIELD, Some(RelF::ChosenPlayer)),
        },
        TargetSpec::AnotherCreatureOrPlaneswalker => (BATTLEFIELD, None),
        TargetSpec::StackObject | TargetSpec::StackAbility => (STACK, None),
        TargetSpec::UpToOneStackAbility
        | TargetSpec::AnySpellOnStack
        | TargetSpec::InstantSpellOnStack
        | TargetSpec::BlueSpellOnStack
        | TargetSpec::RedSpellOnStack
        | TargetSpec::ArtifactOrEnchantmentSpellOnStack
        | TargetSpec::SorcerySpellOnStack
        | TargetSpec::NoncreatureSpellOnStack
        | TargetSpec::ArtifactSpellOnStack
        | TargetSpec::CreatureSpellOnStack => (STACK, None),
        TargetSpec::SpellYouDontControl => (STACK, Some(RelF::Opponent)),
        TargetSpec::SpellManaValueAtMostControlledSubtypes { first, second } => {
            // The subtypes bound the legal mana value; that is a target
            // legality fact owned by the targets table, not an origin.
            let _ = (first, second);
            (STACK, None)
        }
        TargetSpec::AnyPermanent
        | TargetSpec::BluePermanent
        | TargetSpec::RedPermanent
        | TargetSpec::NonlandPermanent
        | TargetSpec::Creature
        | TargetSpec::NonlegendaryCreature
        | TargetSpec::ArtifactPermanent
        | TargetSpec::UpToTwoCreatures
        | TargetSpec::ExactlyTwoArtifactPermanents
        | TargetSpec::EnchantmentPermanent
        | TargetSpec::CreatureOtherThanSource
        | TargetSpec::UpToOneTappedCreature
        | TargetSpec::ControlledNoncreatureArtifactPermanent
        | TargetSpec::CreatureWithStunCounter
        | TargetSpec::NoncreatureArtifactPermanent
        | TargetSpec::Land
        | TargetSpec::NonblackCreature
        | TargetSpec::ArtifactOrEnchantmentPermanent
        | TargetSpec::AttackingOrBlockingCreature
        | TargetSpec::CreatureOrPlaneswalker
        | TargetSpec::ArtifactEnchantmentOrFlyingCreature
        | TargetSpec::ArtifactEnchantmentOrCreaturePowerAtLeastFour
        | TargetSpec::CreaturePowerPlusToughnessAtMostFive
        | TargetSpec::NonartifactCreature
        | TargetSpec::UpToOneOtherCreature
        | TargetSpec::AnotherAttackingCreature
        | TargetSpec::NonOutlawCreature
        | TargetSpec::CreatureToughnessAtLeastFour
        | TargetSpec::ArtifactCreatureEnchantmentOrPlaneswalker
        | TargetSpec::CreatureEnchantmentOrPlaneswalker => (BATTLEFIELD, None),
        TargetSpec::ControlledNoncreatureArtifactPermanent
        | TargetSpec::ControlledCreature
        | TargetSpec::CounterDistribution
        | TargetSpec::AnotherControlledCreature
        | TargetSpec::UpToTwoOtherControlledCreatures
        | TargetSpec::UpToOneOtherControlledPermanent => (BATTLEFIELD, Some(RelF::You)),
        TargetSpec::ControlledCreatureWithSubtype(subtype) => {
            // The subtype is a target legality fact (targets table).
            let _ = subtype;
            (BATTLEFIELD, Some(RelF::You))
        }
        TargetSpec::AttackingCreatureWithSubtype(subtype) => {
            // The subtype is a target legality fact (targets table).
            let _ = subtype;
            (BATTLEFIELD, None)
        }
        TargetSpec::ControlledPermanentWithAnySubtype(subtypes) => {
            // The subtypes are a target legality fact (targets table).
            let _ = subtypes;
            (BATTLEFIELD, Some(RelF::You))
        }
        TargetSpec::OpponentControlledCreature
        | TargetSpec::OpponentArtifactOrEnchantmentPermanent
        | TargetSpec::OpponentNonlandPermanent => (BATTLEFIELD, Some(RelF::Opponent)),
        TargetSpec::ControlledCreatureThenOpponentCreature
        | TargetSpec::ControlledCreatureThenOpponentCreatureOrPlaneswalker => match slot {
            0 => (BATTLEFIELD, Some(RelF::You)),
            1..=u8::MAX => (BATTLEFIELD, Some(RelF::Opponent)),
        },
        TargetSpec::AnyPlayer | TargetSpec::UpToTwoPlayers | TargetSpec::TargetOpponent => {
            (None, None)
        }
        TargetSpec::CreatureOrLandCardInGraveyard
        | TargetSpec::UpToTwoCardsInGraveyards
        | TargetSpec::UpToOneCardInGraveyards => (GRAVEYARD, None),
        TargetSpec::UpToTwoCreatureCardsInOwnGraveyard | TargetSpec::CreatureCardInOwnGraveyard => {
            (GRAVEYARD, Some(RelF::You))
        }
        TargetSpec::CreatureCardInOwnGraveyardManaValueAtMost(mana_value) => {
            // The mana-value bound is a target legality fact (targets table).
            let _ = mana_value;
            (GRAVEYARD, Some(RelF::You))
        }
        TargetSpec::PermanentCardInOwnGraveyard => (GRAVEYARD, Some(RelF::You)),
        TargetSpec::NonlandPermanentCardInOwnGraveyardManaValueAtMost(mana_value) => {
            // The mana-value bound is a target legality fact (targets table).
            let _ = mana_value;
            (GRAVEYARD, Some(RelF::You))
        }
    }
}

/// The origin zone and the relation of the player whose object `object`
/// names. `default_rel` is used when the reference does not fix it.
fn object_origin(object: ObjectRef, env: &Env, default_rel: RelF) -> (Option<ZoneF>, RelF) {
    match object {
        // The source's zone depends on the ability's context (a resolving
        // spell is on the stack, a graveyard trigger's source is in the
        // graveyard), which the program alone does not carry.
        ObjectRef::ThisSource => (None, RelF::You),
        ObjectRef::Target(slot) => {
            let (zone, rel) = target_slot_origin(env.target_spec, slot);
            (zone, rel.unwrap_or(default_rel))
        }
    }
}

/// Facts of a counter of one kind placed on a creature, plus the rules
/// consequence the engine attaches to that kind of counter.
fn place_counters(
    count: i16,
    obj: ObjF,
    player: RelF,
    consequence: Option<EffectAtom>,
    out: &mut Collector,
) {
    if count == 0 {
        return;
    }
    out.effect(
        EffectAtom::new(EvF::PlaceCounter)
            .player(player)
            .obj(obj)
            .amount(AmtF::fixed(i64::from(count)))
            .duration(DurF::Permanent),
    );
    if let Some(atom) = consequence {
        out.effect(atom);
    }
}

pub(super) fn effect_op(op: &EffectOp, env: &Env, out: &mut Collector) {
    match op {
        EffectOp::Sequence(ops) => {
            for inner in ops {
                super::effect_op(inner, env, out);
            }
        }
        EffectOp::Conditional { cond, then, else_ } => {
            out.control(ControlF::Conditional);
            reads::effect_cond(cond, env, out);
            super::effect_op(then, env, out);
            super::effect_op(else_, env, out);
        }
        EffectOp::Choice {
            controller,
            options,
        } => {
            // The resumable interpreter yields a policy-visible decision
            // for `controller`, who picks exactly one option. With zero
            // options nothing runs and a single option runs with no
            // decision, so only two or more options are a choice.
            let chooser = player_ref(*controller);
            // The control vocabulary has no chooser slot; every authored
            // Choice is controller-chosen (see report: proposed addition).
            let _ = chooser;
            if options.len() >= 2 {
                out.control(ControlF::ChooseBranch);
            }
            for option in options {
                super::effect_op(option, env, out);
            }
        }
        EffectOp::DealDamage { target, amount } => {
            let (player, obj) = target_ref(*target, env);
            out.effect(
                EffectAtom::new(EvF::Damage)
                    .maybe_player(player)
                    .obj(obj)
                    .amount(AmtF::fixed(i64::from(*amount))),
            );
        }
        EffectOp::GainLife { player, amount } => {
            out.effect(
                EffectAtom::new(EvF::LifeGain)
                    .player(player_ref(*player))
                    .obj(ObjF::Player)
                    .amount(AmtF::fixed(i64::from(*amount))),
            );
        }
        EffectOp::LoseLife { player, amount } => {
            out.effect(
                EffectAtom::new(EvF::LifeLoss)
                    .player(player_ref(*player))
                    .obj(ObjF::Player)
                    .amount(AmtF::fixed(i64::from(*amount))),
            );
        }
        EffectOp::DrawCards { player, count } => {
            out.effect(
                EffectAtom::moving(Some(ZoneF::Library), ZoneF::Hand)
                    .player(player_ref(*player))
                    .obj(ObjF::AnyCard)
                    .amount(AmtF::fixed(i64::from(*count))),
            );
        }
        EffectOp::DrawCardsDynamic { player, count } => {
            // The count is sampled once at resolution.
            let amount = reads::dynamic_value(*count, out);
            out.effect(
                EffectAtom::moving(Some(ZoneF::Library), ZoneF::Hand)
                    .player(player_ref(*player))
                    .obj(ObjF::AnyCard)
                    .amount(amount),
            );
        }
        EffectOp::RevealTopAndPartitionByType {
            player,
            count,
            card_type,
            matching_to,
            rest_to,
        } => {
            let player = player_ref(*player);
            // Public reveal of a fixed snapshot of the top `count` cards.
            let mut reveal = EffectAtom::new(EvF::Reveal)
                .player(player)
                .obj(ObjF::AnyCard)
                .amount(AmtF::fixed(i64::from(*count)));
            reveal.from = Some(ZoneF::Library);
            out.effect(reveal);
            // Every revealed card of the type moves to `matching_to`, every
            // other revealed card to `rest_to`. The owner's ordering of a
            // multi-card graveyard group has no rules consequence here.
            out.effect(
                EffectAtom::moving(Some(ZoneF::Library), ZoneF::from(*matching_to))
                    .player(player)
                    .obj(ObjF::from(*card_type))
                    .amount(AmtF::All),
            );
            out.effect(
                EffectAtom::moving(Some(ZoneF::Library), ZoneF::from(*rest_to))
                    .player(player)
                    .obj(ObjF::AnyCard)
                    .amount(AmtF::All),
            );
        }
        EffectOp::DiscardCards { player, count } => {
            // Staged as `pending_discard`: the discarding player chooses
            // which cards from hand go to the graveyard.
            out.control(ControlF::ChooseObjects);
            out.effect(
                EffectAtom::moving(Some(ZoneF::Hand), ZoneF::Graveyard)
                    .player(player_ref(*player))
                    .obj(ObjF::AnyCard)
                    .amount(AmtF::fixed(i64::from(*count))),
            );
        }
        EffectOp::DiscardBasicLandOrCards { player, otherwise } => {
            // The player privately chooses zero or one basic land card from
            // hand to discard; declining falls through to an ordinary staged
            // discard of `otherwise` cards. Vocabulary gap: no basic-land
            // class; nearest is "a land".
            let player = player_ref(*player);
            out.control(ControlF::ChooseObjects);
            out.effect(
                EffectAtom::moving(Some(ZoneF::Hand), ZoneF::Graveyard)
                    .player(player)
                    .obj(ObjF::Typed(CardTypeF::Land))
                    .amount(AmtF::fixed(1)),
            );
            out.effect(
                EffectAtom::moving(Some(ZoneF::Hand), ZoneF::Graveyard)
                    .player(player)
                    .obj(ObjF::AnyCard)
                    .amount(AmtF::fixed(i64::from(*otherwise))),
            );
        }
        EffectOp::MoveObject { object, to_zone } => {
            let obj = object_ref(*object, env);
            // Zone changes put the card in its owner's zone.
            let (from, player) = object_origin(*object, env, RelF::ObjectOwner);
            let to = ZoneF::from(*to_zone);
            // A live spell leaving the stack for any other zone goes
            // through the engine's spell-departure path without
            // resolving: it is countered.
            if from == Some(ZoneF::Stack) && to != ZoneF::Stack {
                out.effect(EffectAtom::new(EvF::CounterSpell).player(player).obj(obj));
            }
            out.effect(EffectAtom::moving(from, to).player(player).obj(obj));
        }
        EffectOp::Sacrifice { object } => {
            // A no-op unless the object is on the battlefield; its
            // controller moves it to its owner's graveyard.
            let obj = object_ref(*object, env);
            let (from, player) = object_origin(*object, env, RelF::ObjectController);
            let _ = from; // the executor only acts on a battlefield object
            out.effect(
                EffectAtom::moving(Some(ZoneF::Battlefield), ZoneF::Graveyard)
                    .player(player)
                    .obj(obj),
            );
        }
        EffectOp::TapObject { object } => {
            let obj = object_ref(*object, env);
            let (from, player) = object_origin(*object, env, RelF::ObjectController);
            let _ = from; // tapping does not change zones
            out.effect(EffectAtom::new(EvF::Tap).player(player).obj(obj));
        }
        EffectOp::SkipNextUntap { object } => {
            // Marks the permanent so its current controller's next untap
            // step skips it; cleared by any zone change.
            let obj = object_ref(*object, env);
            let (from, player) = object_origin(*object, env, RelF::ObjectController);
            let _ = from; // no zone change
            out.effect(
                EffectAtom::new(EvF::Restrict)
                    .player(player)
                    .obj(obj)
                    .duration(DurF::UntilYourNextTurn),
            );
        }
        EffectOp::AttachSourceToTarget { object } => {
            // Attaches the live Equipment source to the targeted
            // battlefield creature (the host is the event's object).
            let obj = object_ref(*object, env);
            let (from, player) = object_origin(*object, env, RelF::ObjectController);
            let _ = from; // no zone change
            out.effect(
                EffectAtom::new(EvF::Attach)
                    .player(player)
                    .obj(obj)
                    .duration(DurF::WhileOnBattlefield),
            );
        }
        EffectOp::AddCountersToTarget {
            target_index,
            optional,
            plus1_plus1,
            lifelink,
            stun,
        } => {
            // An absent optional target is the target spec's "up to"
            // legality fact (targets table), not a resolution choice.
            let _ = optional;
            let obj = env.target_obj(*target_index);
            let (zone, rel) = target_slot_origin(env.target_spec, *target_index);
            let _ = zone; // the executor requires a battlefield creature
            let player = rel.unwrap_or(RelF::ObjectController);
            // +1/+1 counters raise power and toughness while they remain.
            place_counters(
                *plus1_plus1,
                obj,
                player,
                Some(
                    EffectAtom::new(EvF::StatChange)
                        .player(player)
                        .obj(obj)
                        .amount(AmtF::fixed(i64::from(*plus1_plus1)))
                        .duration(DurF::Permanent),
                ),
                out,
            );
            // Lifelink keyword counters grant lifelink while they remain.
            place_counters(
                *lifelink,
                obj,
                player,
                Some(
                    EffectAtom::new(EvF::GrantKeyword)
                        .player(player)
                        .obj(obj)
                        .duration(DurF::Permanent)
                        .keyword(keyword_bits(crate::card_def::Keywords::LIFELINK)[0]),
                ),
                out,
            );
            // A stun counter is removed instead of the tapped permanent
            // untapping in its controller's untap step.
            place_counters(
                *stun,
                obj,
                player,
                Some(
                    EffectAtom::new(EvF::Restrict)
                        .player(player)
                        .obj(obj)
                        .amount(AmtF::fixed(i64::from(*stun)))
                        .duration(DurF::UntilYourNextTurn),
                ),
                out,
            );
        }
        EffectOp::CreateTokenAndAttachSource { token_def }
        | EffectOp::CreateRoleAttachedToTarget { token_def, .. } => {
            // The token is created under the ability's controller even if
            // the Equipment left; the live source then attaches to it.
            out.effect(
                EffectAtom::new(EvF::CreateToken)
                    .player(RelF::You)
                    .obj(ObjF::Token)
                    .amount(AmtF::fixed(1)),
            );
            out.created_tokens.push(*token_def);
            out.effect(
                EffectAtom::new(EvF::Attach)
                    .player(RelF::You)
                    .obj(ObjF::Token)
                    .duration(DurF::WhileOnBattlefield),
            );
        }
        EffectOp::AddMana { player, colors } => {
            // One mana of each listed color.
            for &color in colors {
                out.effect(
                    EffectAtom::new(EvF::AddMana)
                        .player(player_ref(*player))
                        .amount(AmtF::fixed(1))
                        .color(color.into()),
                );
            }
        }
        EffectOp::AddManaDynamic {
            player,
            color,
            amount,
        } => {
            let amount = reads::dynamic_value(*amount, out);
            out.effect(
                EffectAtom::new(EvF::AddMana)
                    .player(player_ref(*player))
                    .amount(amount)
                    .color((*color).into()),
            );
        }
        EffectOp::CreateToken {
            token_def,
            controller,
        } => {
            out.effect(
                EffectAtom::new(EvF::CreateToken)
                    .player(player_ref(*controller))
                    .obj(ObjF::Token)
                    .amount(AmtF::fixed(1)),
            );
            out.created_tokens.push(*token_def);
        }
        EffectOp::MayPayCostThen {
            discard,
            sacrifice_lands,
            return_permanent,
            then,
            otherwise,
        } => {
            // The controller may pay one currently payable option, or
            // decline; paying runs `then`, declining (or nothing payable)
            // runs `otherwise`.
            out.control(ControlF::Optional);
            let options = usize::from(*discard > 0)
                + usize::from(*sacrifice_lands > 0)
                + usize::from(return_permanent.is_some());
            if options > 1 || otherwise.is_some() {
                out.control(ControlF::ChooseBranch);
            }
            if *discard > 0 {
                // Staged through `pending_discard`: the controller picks.
                out.control(ControlF::ChooseObjects);
                out.effect(
                    EffectAtom::moving(Some(ZoneF::Hand), ZoneF::Graveyard)
                        .player(RelF::You)
                        .obj(ObjF::AnyCard)
                        .amount(AmtF::fixed(i64::from(*discard))),
                );
            }
            if *sacrifice_lands > 0 {
                // The controller picks which controlled lands to sacrifice.
                out.control(ControlF::ChooseObjects);
                out.effect(
                    EffectAtom::moving(Some(ZoneF::Battlefield), ZoneF::Graveyard)
                        .player(RelF::You)
                        .obj(ObjF::Typed(CardTypeF::Land))
                        .amount(AmtF::fixed(i64::from(*sacrifice_lands))),
                );
            }
            if let Some(filter) = return_permanent {
                // The engine returns a matching permanent the controller
                // controls to its owner's hand (deterministic pick).
                let (obj, rel) = reads::permanent_filter_def(*filter);
                // Candidates are always the controller's own permanents.
                let _ = rel;
                out.effect(
                    EffectAtom::moving(Some(ZoneF::Battlefield), ZoneF::Hand)
                        .player(RelF::You)
                        .obj(obj),
                );
            }
            // Both continuations run with `ExecCtx::no_targets`.
            let nested = Env {
                target_spec: TargetSpec::None,
            };
            super::effect_op(then, &nested, out);
            if let Some(otherwise) = otherwise {
                super::effect_op(otherwise, &nested, out);
            }
        }
        _ => unreachable!("dispatched to the wrong slice"),
    }
}
