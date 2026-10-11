//! Meaning tables: one exhaustive `match` per rules enum, mapping each
//! variant to mechanical facets (`super::facets`).
//!
//! Rules for every table, checked in review:
//! - No `_` arm on a rules enum and no `..` in a pattern: a new variant or
//!   field fails to compile until someone decides what it means.
//! - Emit only what the engine does (read the executor), never what a human
//!   would call the card. Variant identifiers are never emitted as features.
//! - Written before any benchmark result was seen; the table file hashes are
//!   recorded in the extractor manifest.

mod effect_a;
mod effect_b;
mod effect_c;
mod effect_d;
mod effect_e;
mod effect_f;
mod effect_g;
mod effect_h;
mod effect_i;
pub(crate) mod reads;
pub(crate) mod targets;
pub(crate) mod triggers_costs;

pub(crate) use super::facets::*;
pub(crate) use crate::card_def::{
    CardType, CostComponent, DynamicCountDef, DynamicValueDef, PermanentFilter, PermanentFilterDef,
    TargetSpec,
};
pub(crate) use crate::effect::{
    CardTypePredicate, CreatureFilter, CreatureSacrificeFilter, EffectCond, EffectOp,
    ImpulseDuration, LibraryCardFilter, LibrarySearchDestinationV1, ObjectRef, PlayerRef,
    PumpControllerScope, TargetRef,
};
pub(crate) use crate::trigger::TriggerCondition;

/// What a program's references resolve against.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Env {
    /// The target specification governing `Target(i)` references.
    pub target_spec: TargetSpec,
}

impl Env {
    /// The object class a `Target(slot)` reference can denote.
    pub(crate) fn target_obj(&self, slot: u8) -> ObjF {
        targets::target_slot_obj(self.target_spec, slot)
    }
}

/// A player reference relative to the ability's controller.
pub(crate) fn player_ref(player: PlayerRef) -> RelF {
    match player {
        PlayerRef::Controller => RelF::You,
        PlayerRef::Target(_) => RelF::ChosenPlayer,
        PlayerRef::ObjectController(_) => RelF::ObjectController,
        PlayerRef::Opponent => RelF::Opponent,
    }
}

/// An object reference as an object class.
pub(crate) fn object_ref(object: ObjectRef, env: &Env) -> ObjF {
    match object {
        ObjectRef::ThisSource => ObjF::ThisObject,
        ObjectRef::Target(slot) => env.target_obj(slot),
    }
}

/// A damage/target recipient: either a player relation or an object class.
pub(crate) fn target_ref(target: TargetRef, env: &Env) -> (Option<RelF>, ObjF) {
    match target {
        TargetRef::ThisSource => (None, ObjF::ThisObject),
        TargetRef::Target(slot) => {
            let obj = env.target_obj(slot);
            let player =
                matches!(obj, ObjF::Player | ObjF::PlayerOrPermanent).then_some(RelF::ChosenPlayer);
            (player, obj)
        }
        TargetRef::Opponent => (Some(RelF::Opponent), ObjF::Player),
        TargetRef::Controller => (Some(RelF::You), ObjF::Player),
    }
}

/// Facets of one resolution program. Recurses through nested programs.
pub(crate) fn effect_op(op: &EffectOp, env: &Env, out: &mut Collector) {
    match op {
        EffectOp::StandardV1(_) => {
            // Preserve the exact serialized payload in the source record.
            // Do not invent ordinary facets for new histories, counters,
            // copy/control/type changes or Standard continuation semantics.
            out.atoms.push(Atom::Opaque);
        }
        EffectOp::Sequence(..)
        | EffectOp::Conditional { .. }
        | EffectOp::Choice { .. }
        | EffectOp::DealDamage { .. }
        | EffectOp::GainLife { .. }
        | EffectOp::LoseLife { .. }
        | EffectOp::DrawCards { .. }
        | EffectOp::DrawCardsDynamic { .. }
        | EffectOp::RevealTopAndPartitionByType { .. }
        | EffectOp::DiscardCards { .. }
        | EffectOp::DiscardBasicLandOrCards { .. }
        | EffectOp::MoveObject { .. }
        | EffectOp::Sacrifice { .. }
        | EffectOp::TapObject { .. }
        | EffectOp::SkipNextUntap { .. }
        | EffectOp::AttachSourceToTarget { .. }
        | EffectOp::AddCountersToTarget { .. }
        | EffectOp::CreateTokenAndAttachSource { .. }
        | EffectOp::CreateRoleAttachedToTarget { .. }
        | EffectOp::AddMana { .. }
        | EffectOp::AddManaDynamic { .. }
        | EffectOp::CreateToken { .. }
        | EffectOp::MayPayCostThen { .. } => effect_a::effect_op(op, env, out),
        EffectOp::DamageOpponentAndTheirCreatures { .. }
        | EffectOp::PumpControlled { .. }
        | EffectOp::ImpulseDraw { .. }
        | EffectOp::OfferAffectedPlayerSpellCopy { .. }
        | EffectOp::MillCards { .. }
        | EffectOp::LookAtLibraryTopAndReorder { .. }
        | EffectOp::MayShuffleLibrary { .. }
        | EffectOp::PutCardsFromHandOnLibraryTop { .. }
        | EffectOp::Scry { .. }
        | EffectOp::SearchLibraryToHand { .. }
        | EffectOp::PutObjectInOwnersLibrarySecondOrBottom { .. }
        | EffectOp::PutBoundObjectInOwnersLibrary { .. }
        | EffectOp::DestroyObject { .. }
        | EffectOp::DestroyObjectThenCreateTokens { .. }
        | EffectOp::CounterUnlessPaysGeneric { .. }
        | EffectOp::DamageEachCreatureWithoutSubtype { .. }
        | EffectOp::CounterTargetUnlessPaysGeneric { .. }
        | EffectOp::GainLifeDynamic { .. }
        | EffectOp::UntapObject { .. }
        | EffectOp::PumpTargetUntilEndOfTurnDynamic { .. }
        | EffectOp::LookTopSelectByTypeToHandBottomRest { .. }
        | EffectOp::LookTopPickToHandBottomRest { .. }
        | EffectOp::LookTopSelectV1 { .. } => effect_b::effect_op(op, env, out),
        EffectOp::GainLifeEqualToPaidCostManaValue { .. }
        | EffectOp::MoveAllTargets { .. }
        | EffectOp::ExploreTarget { .. }
        | EffectOp::Surveil { .. }
        | EffectOp::EachPlayerControllingDefinitionDrawsCard { .. }
        | EffectOp::MoveBoundObject { .. }
        | EffectOp::GrantKeywordTargetUntilEndOfTurn { .. }
        | EffectOp::PumpTargetByControlledSubtypeCount { .. }
        | EffectOp::SearchLibraryToHandUpTo { .. }
        | EffectOp::DamageAllCreatures { .. }
        | EffectOp::ExilePlayersGraveyard { .. }
        | EffectOp::ExileOneFromPlayersGraveyard { .. }
        | EffectOp::MayExileFromPlayersGraveyardMatchingThen { .. }
        | EffectOp::ExileAllGraveyards
        | EffectOp::MayPayManaThen { .. }
        | EffectOp::DamageAllTargets { .. }
        | EffectOp::ExileAllArtifactTargets
        | EffectOp::DealDamageByControlledCreatureCount { .. }
        | EffectOp::ExileTargetPlayersGraveyards
        | EffectOp::RevealUntilCardTypeAndMill { .. } => effect_c::effect_op(op, env, out),
        EffectOp::DealDamageDynamic { .. }
        | EffectOp::BindPlusOnePlusOneCounterToTriggerSource
        | EffectOp::PutPlusOnePlusOneCounterOnBoundObject { .. }
        | EffectOp::PutSourceOntoBattlefieldAttachedToTarget { .. }
        | EffectOp::TapAttachedCreatureAndDamageControllerByPower
        | EffectOp::BackupTarget { .. }
        | EffectOp::PutSourceOntoBattlefieldTappedAndAttacking
        | EffectOp::UntapUpToLands { .. }
        | EffectOp::DestroyTargetLandThenMaySearchBasicTapped { .. }
        | EffectOp::SearchLibraryToBattlefieldTapped { .. }
        | EffectOp::RevealTargetHandChooseNoncreatureNonlandDiscard { .. }
        | EffectOp::RevealTargetHandChooseNonlandDiscard { .. }
        | EffectOp::ShuffleTriggerSourceIntoOwnersLibrary
        | EffectOp::MaterializeStormCopies
        | EffectOp::CreateStormCopies { .. }
        | EffectOp::DamageCannotBePreventedThisTurn
        | EffectOp::AddMinusOneMinusOneCounter { .. }
        | EffectOp::RevealHandChooseNonlandToLinkedExile { .. }
        | EffectOp::ReturnLinkedExiledCardToOwnersHand
        | EffectOp::ExileTargetLinkedToSource { .. }
        | EffectOp::ReturnObjectsExiledBySource => effect_d::effect_op(op, env, out),
        EffectOp::PreventDamageFromChosenColorUntilEndOfTurn { .. }
        | EffectOp::InstallDamagePreventionFromColor { .. }
        | EffectOp::TransformSagaSource
        | EffectOp::TransformSourceInPlace
        | EffectOp::LookAtTopMayRevealThen { .. }
        | EffectOp::SacrificeCreature { .. }
        | EffectOp::PutPlusOnePlusOneCounter { .. }
        | EffectOp::PutSourceOntoBattlefieldWithXPlusOneCounters
        | EffectOp::PutSourceOntoBattlefieldAttachedToTargetWithXPlusOneCounters { .. }
        | EffectOp::DealDamageToTargetEqualToChosenCostCreaturePower { .. }
        | EffectOp::TakeInitiative { .. }
        | EffectOp::ResolveInitiativeTrigger { .. }
        | EffectOp::EnterUndercityRoom { .. }
        | EffectOp::AddPlusOnePlusOneCounters { .. }
        | EffectOp::GoadTargetUntilSourcesNextTurn { .. }
        | EffectOp::ResolveUndercityThrone { .. }
        | EffectOp::BecomeMonarch
        | EffectOp::ResolveMonarchTrigger { .. }
        | EffectOp::PumpAllUntilEndOfTurn { .. }
        | EffectOp::DealDamageToControllerOfTarget { .. } => effect_e::effect_op(op, env, out),
        EffectOp::BindPlusOnePlusOneCounterToTriggerEventObject
        | EffectOp::PutPlusOnePlusOneCounterOnTriggerEventObject { .. }
        | EffectOp::BindTemporaryBoostToTriggerSource { .. }
        | EffectOp::BoostBoundObjectUntilEndOfTurn { .. }
        | EffectOp::BoostControlledCreaturesUntilEndOfTurn { .. }
        | EffectOp::BoostPlayerCreaturesUntilEndOfTurn { .. }
        | EffectOp::GainLifeByAttackingSubtypeCount { .. }
        | EffectOp::CreatureTargetPowerDamage { .. }
        | EffectOp::FightObjects { .. }
        | EffectOp::PreventCombatDamageToTargetThisTurn { .. }
        | EffectOp::BindDoublePlusOneCountersToTriggerSource
        | EffectOp::DoublePlusOneCountersOnBoundObject { .. }
        | EffectOp::ReturnTargetPermanentToBattlefield { .. }
        | EffectOp::PutBoundAuraOntoBattlefieldAttached { .. }
        | EffectOp::PutObjectInOwnersLibraryTopOrBottom { .. }
        | EffectOp::SurveilOne { .. }
        | EffectOp::BoostAttachedCreatureUntilEndOfTurn { .. }
        | EffectOp::DestroyAllCreatures
        | EffectOp::SearchLibraryCardsToDestination { .. }
        | EffectOp::CreateTokensDynamic { .. } => effect_f::effect_op(op, env, out),
        EffectOp::CreatureChoiceV1(_)
        | EffectOp::CreatureChoiceAnswerV1 { .. }
        | EffectOp::CreatureUpgrade(_)
        | EffectOp::DistributePlusOneCounters { .. }
        | EffectOp::BindEntrantOutgrowsSourceThen { .. }
        | EffectOp::IfEntrantOutgrowsSourceThen { .. }
        | EffectOp::BindOilCounterToTriggerSource
        | EffectOp::PutOilCounterOnBoundObject { .. }
        | EffectOp::CreateTokenTappedAndAttacking { .. }
        | EffectOp::AddPlusOneCounterToAbilitySource
        | EffectOp::ReturnTargetCreatureCardRestrictedWhileSourceControlled { .. }
        | EffectOp::LoseHalfLifeRoundedUp { .. }
        | EffectOp::ReturnSourceFromGraveyardTappedWithStunCounters { .. }
        | EffectOp::LookTopMayTakeCreatureManaValueAtMostToHandBottomRest { .. } => {
            effect_g::effect_op(op, env, out)
        }
        EffectOp::CastExiledWithoutMana { .. }
        | EffectOp::PlayExiledLand { .. }
        | EffectOp::DiscardUpToThenDraw { .. }
        | EffectOp::ExileRandomGraveyardCardPlayableThisTurn { .. }
        | EffectOp::ExileUntilThenCastV1 { .. }
        | EffectOp::Discover { .. }
        | EffectOp::Hideaway { .. }
        | EffectOp::PlayHideawayIfThreeDistinctPowers
        | EffectOp::CopySpellSnapshot { .. }
        | EffectOp::IncreaseSpeed { .. }
        | EffectOp::CounterUnlessCollectsEvidence { .. }
        | EffectOp::CounterUnlessPaysLife { .. }
        | EffectOp::CounterUnlessDiscardsCard { .. }
        | EffectOp::BindConvokedCreatureCountToLookTop { .. }
        | EffectOp::LookTopTakeCreaturesManaValueAtMostThenShuffle { .. }
        | EffectOp::BindDamageOpponentEqualToSourceLastPower
        | EffectOp::PumpOtherAttackingCreaturesUntilEndOfTurn { .. }
        | EffectOp::RevealTopCardToHandLoseLifeEqualToManaValue
        | EffectOp::BindIncubateToTriggerSpell
        | EffectOp::Incubate { .. }
        | EffectOp::BindWarpExileToTriggerSource
        | EffectOp::WarpExileBoundObject { .. }
        | EffectOp::BindPlusOneCounterOnAnotherTargetToTriggerTarget
        | EffectOp::PutPlusOnePlusOneCounterOnTargetOtherThan { .. }
        | EffectOp::ReturnSourceFromGraveyardUnearthed
        | EffectOp::ReturnAbilitySourceFromGraveyard { .. }
        | EffectOp::LoseOpponentsLifeXThenGainLifeLost
        | EffectOp::ReturnAttackingCreaturesToOwnersHands
        | EffectOp::ReturnOwnGraveyardCreaturesManaValueAtMost { .. }
        | EffectOp::ReturnAllGraveyardCreaturesUnderController
        | EffectOp::CounterTargetSpellThenCreateTokens { .. }
        | EffectOp::ExileGraveyardTargetsDrainPerCreature { .. }
        | EffectOp::RemoveTimeCounterFromSource
        | EffectOp::ReturnSourceAsEnduringEnchantment => effect_h::effect_op(op, env, out),
        EffectOp::StandardLegendV1(_)
        | EffectOp::AnimateSource
        | EffectOp::AnimateSourcePermanentlyV1
        | EffectOp::SetTargetBasePowerToughnessUntilEndOfTurn { .. }
        | EffectOp::BoostOtherControlledCreaturesUntilEndOfTurn { .. }
        | EffectOp::SelectObjectsV1 { .. }
        | EffectOp::DestroyCreaturesPowerAtMostV1 { .. }
        | EffectOp::DestroyPermanentsSharingTargetNameV1 { .. }
        | EffectOp::CreateTokensWithHasteUntilEndOfTurnV1 { .. } => {
            effect_i::effect_op(op, env, out)
        }
    }
}

#[cfg(test)]
mod standard_completion_tests {
    use super::*;

    #[test]
    fn resolution_cast_and_land_play_have_distinct_destinations_without_persistent_permission() {
        let card = crate::effect::EffectObjectBinding {
            object: crate::ids::ObjectId(1),
            expected_zone: crate::state::Zone::Exile,
            expected_zone_change_count: 2,
        };
        for (op, destination) in [
            (
                EffectOp::CastExiledWithoutMana {
                    card,
                    maximum_mana_value: Some(3),
                },
                ZoneF::Stack,
            ),
            (EffectOp::PlayExiledLand { card }, ZoneF::Battlefield),
        ] {
            let mut out = Collector::default();
            effect_op(
                &op,
                &Env {
                    target_spec: TargetSpec::None,
                },
                &mut out,
            );
            assert!(out.atoms.iter().any(|atom| matches!(atom,
                Atom::Effect(effect) if effect.ev == EvF::Move
                    && effect.from == Some(ZoneF::Exile) && effect.to == Some(destination))));
            assert!(!out.atoms.iter().any(|atom| matches!(atom,
                Atom::Effect(effect) if effect.ev == EvF::PlayPermission)));
            assert!(out.atoms.contains(&Atom::Opaque));
        }
    }

    #[test]
    fn crew_members_do_not_inherit_a_current_controller_restriction() {
        use crate::standard_cards_v1::StandardTargetV1 as S;
        for (filter, controller) in [
            (S::CrewedSourceThisTurn, None),
            (S::TappedOpponentCreature, Some(RelF::Opponent)),
        ] {
            let mut out = Collector::default();
            targets::target_spec(TargetSpec::StandardV1(filter), &mut out);
            assert!(out.atoms.iter().any(|atom| matches!(atom,
                Atom::Target(TargetAtom::Object { obj, controller: actual, zone, .. })
                    if *obj == ObjF::Typed(CardTypeF::Creature)
                        && *actual == controller && *zone == ZoneF::Battlefield)));
            assert!(out.atoms.contains(&Atom::Opaque));
        }
    }

    #[test]
    fn crew_and_counter_payments_are_costs_without_changing_legacy_loyalty_atoms() {
        let mut crew = Collector::default();
        triggers_costs::cost_component(CostComponent::Crew(3), &mut crew);
        assert!(crew.atoms.contains(&Atom::Cost(CostAtom::TapOthers)));
        assert!(!crew.atoms.contains(&Atom::Cost(CostAtom::Tap)));
        for cost in [
            CostComponent::LoyaltyX,
            CostComponent::RemoveChargeCounterFromSelf,
        ] {
            let mut out = Collector::default();
            triggers_costs::cost_component(cost, &mut out);
            assert!(out.atoms.contains(&Atom::Cost(CostAtom::RemoveCounters)));
            assert!(!out.atoms.iter().any(|atom| matches!(atom, Atom::Effect(_))));
        }
        let mut legacy = Collector::default();
        triggers_costs::cost_component(CostComponent::Loyalty(-2), &mut legacy);
        assert_eq!(legacy.atoms, [Atom::Opaque]);
    }
}
