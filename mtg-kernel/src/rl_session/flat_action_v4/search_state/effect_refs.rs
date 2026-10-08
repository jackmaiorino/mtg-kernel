//! Typed admission walk for suspended interpreter references.
use super::*;
use crate::effect::*;
use crate::state::AbilitySourceContractV4;

struct Scan<'a> {
    state: &'a GameState,
    pool: &'a [ObjectId],
}
impl Scan<'_> {
    fn raw(&self, id: ObjectId) -> bool {
        self.pool.contains(&id)
    }
    fn same(&self, id: ObjectId, generation: u32) -> bool {
        self.raw(id) && self.state.objects.get(id).zone_change_count == generation
    }
    fn b(&self, b: &EffectObjectBinding) -> bool {
        self.same(b.object, b.expected_zone_change_count)
    }
    fn bs(&self, bs: &[EffectObjectBinding]) -> bool {
        bs.iter().any(|b| self.b(b))
    }
    fn a(&self, a: &AbilitySourceContractV4) -> bool {
        self.same(a.source, a.zone_change_count)
            || a.attached_to
                .is_some_and(|x| self.same(x.object, x.zone_change_count))
    }
    fn op(&self, op: &EffectOp) -> bool {
        use EffectOp::*;
        match op {
            Sequence(ops) | Choice { options: ops, .. } => ops.iter().any(|x| self.op(x)),
            Conditional { then, else_, .. } => self.op(then) || self.op(else_),
            MayPayCostThen {
                then, otherwise, ..
            } => self.op(then) || otherwise.as_ref().is_some_and(|x| self.op(x)),
            MayExileFromPlayersGraveyardMatchingThen { then, .. }
            | MayPayManaThen { then, .. }
            | LookAtTopMayRevealThen { then, .. } => self.op(then),
            PutBoundObjectInOwnersLibrary { object, .. }
            | MoveBoundObject { object, .. }
            | PutPlusOnePlusOneCounterOnBoundObject { object }
            | DoublePlusOneCountersOnBoundObject { object }
            | PutPlusOnePlusOneCounterOnTriggerEventObject { object }
            | BoostBoundObjectUntilEndOfTurn { object, .. }
            | WarpExileBoundObject { object } => self.b(object),
            PutBoundAuraOntoBattlefieldAttached { aura, host } => self.b(aura) || self.b(host),
            ResolveInitiativeTrigger { binding }
            | EnterUndercityRoom { binding, .. }
            | ResolveUndercityThrone { binding } => self.a(&binding.source),
            ResolveMonarchTrigger { binding } => self.a(&binding.source),
            // Other current leaf programs carry symbolic refs, not physical bindings.
            DealDamage { .. }
            | ReturnTargetPermanentToBattlefield { .. }
            | GainLife { .. }
            | LoseLife { .. }
            | DrawCards { .. }
            | RevealTopAndPartitionByType { .. }
            | DiscardCards { .. }
            | MoveObject { .. }
            | Sacrifice { .. }
            | TapObject { .. }
            | SkipNextUntap { .. }
            | AttachSourceToTarget { .. }
            | AddCountersToTarget { .. }
            | CreateTokenAndAttachSource { .. }
            | AddMana { .. }
            | AddManaDynamic { .. }
            | CreateToken { .. }
            | DamageOpponentAndTheirCreatures { .. }
            | PumpControlled { .. }
            | ImpulseDraw { .. }
            | OfferAffectedPlayerSpellCopy { .. }
            | MillCards { .. }
            | LookAtLibraryTopAndReorder { .. }
            | MayShuffleLibrary { .. }
            | PutCardsFromHandOnLibraryTop { .. }
            | Scry { .. }
            | SearchLibraryToHand { .. }
            | PutObjectInOwnersLibrarySecondOrBottom { .. }
            | PutObjectInOwnersLibraryTopOrBottom { .. }
            | SurveilOne { .. }
            | DestroyObject { .. }
            | CounterUnlessPaysGeneric { .. }
            | CounterUnlessCollectsEvidence { .. }
            | DamageEachCreatureWithoutSubtype { .. }
            | CounterTargetUnlessPaysGeneric { .. }
            | GainLifeDynamic { .. }
            | UntapObject { .. }
            | PumpTargetUntilEndOfTurnDynamic { .. }
            | LookTopSelectByTypeToHandBottomRest { .. }
            | GainLifeEqualToPaidCostManaValue { .. }
            | MoveAllTargets { .. }
            | ExploreTarget { .. }
            | Surveil { .. }
            | EachPlayerControllingDefinitionDrawsCard { .. }
            | GrantKeywordTargetUntilEndOfTurn { .. }
            | PumpTargetByControlledSubtypeCount { .. }
            | SearchLibraryToHandUpTo { .. }
            | DamageAllCreatures { .. }
            | ExilePlayersGraveyard { .. }
            | ExileOneFromPlayersGraveyard { .. }
            | ExileAllGraveyards
            | DamageAllTargets { .. }
            | ExileAllArtifactTargets
            | DealDamageByControlledCreatureCount { .. }
            | ExileTargetPlayersGraveyards
            | RevealUntilCardTypeAndMill { .. }
            | DealDamageDynamic { .. }
            | BindPlusOnePlusOneCounterToTriggerSource
            | BindPlusOnePlusOneCounterToTriggerEventObject
            | BindDoublePlusOneCountersToTriggerSource
            | BindTemporaryBoostToTriggerSource { .. }
            | BoostControlledCreaturesUntilEndOfTurn { .. }
            | GainLifeByAttackingSubtypeCount { .. }
            | CreatureTargetPowerDamage { .. }
            | PreventCombatDamageToTargetThisTurn { .. }
            | PutSourceOntoBattlefieldAttachedToTarget { .. }
            | TapAttachedCreatureAndDamageControllerByPower
            | BoostAttachedCreatureUntilEndOfTurn { .. }
            | BindWarpExileToTriggerSource
            | BackupTarget { .. }
            | PutSourceOntoBattlefieldTappedAndAttacking
            | UntapUpToLands { .. }
            | DestroyTargetLandThenMaySearchBasicTapped { .. }
            | SearchLibraryToBattlefieldTapped { .. }
            | RevealTargetHandChooseNoncreatureNonlandDiscard { .. }
            | ShuffleTriggerSourceIntoOwnersLibrary
            | MaterializeStormCopies
            | CreateStormCopies { .. }
            | DamageCannotBePreventedThisTurn
            | AddMinusOneMinusOneCounter { .. }
            | RevealHandChooseNonlandToLinkedExile { .. }
            | ReturnLinkedExiledCardToOwnersHand
            | ExileTargetLinkedToSource { .. }
            | ReturnObjectsExiledBySource
            | PreventDamageFromChosenColorUntilEndOfTurn { .. }
            | InstallDamagePreventionFromColor { .. }
            | TransformSagaSource
            | TransformSourceInPlace
            | SacrificeCreature { .. }
            | PutPlusOnePlusOneCounter { .. }
            | PutSourceOntoBattlefieldWithXPlusOneCounters
            | PutSourceOntoBattlefieldAttachedToTargetWithXPlusOneCounters { .. }
            | DealDamageToTargetEqualToChosenCostCreaturePower { .. }
            | TakeInitiative { .. }
            | AddPlusOnePlusOneCounters { .. }
            | GoadTargetUntilSourcesNextTurn { .. }
            | BecomeMonarch
            | PumpAllUntilEndOfTurn { .. }
            | DealDamageToControllerOfTarget { .. } => false,
        }
    }
    fn fs(&self, fs: &[EffectFrame]) -> bool {
        fs.iter().any(|f| self.f(f))
    }
    fn f(&self, f: &EffectFrame) -> bool {
        use EffectFrame::*;
        match f {
            Program { op, .. } => self.op(op),
            MoveObjectsBatch { objects, .. }
            | MillLibraryBatch { objects, .. }
            | UntapObjectsBatch { objects, .. } => self.bs(objects),
            ReorderLibraryTop {
                expected_prefix,
                ordered,
                ..
            } => self.bs(expected_prefix) || self.bs(ordered),
            ShuffleLibrary { .. } => false,
            PutCardsFromHandOnLibraryTop {
                expected_hand,
                chosen,
                ..
            } => self.bs(expected_hand) || chosen.as_ref().is_some_and(|b| self.b(b)),
            OwnerLibraryPlacement {
                object,
                expected_remaining_frames,
                ..
            } => self.b(object) || self.fs(expected_remaining_frames),
            ScryLibrary {
                original_prefix,
                progress,
                ..
            } => {
                self.bs(original_prefix)
                    || match progress {
                        ScryProgress::BottomSubsetChosen { bottom_subset } => {
                            self.bs(bottom_subset)
                        }
                        ScryProgress::BottomOrderChosen {
                            bottom_subset,
                            ordered_bottom,
                        } => self.bs(bottom_subset) || self.bs(ordered_bottom),
                        ScryProgress::TopOrderChosen {
                            bottom_subset,
                            ordered_bottom,
                            ordered_top,
                        } => {
                            self.bs(bottom_subset)
                                || self.bs(ordered_bottom)
                                || self.bs(ordered_top)
                        }
                    }
            }
            SearchLibraryToHand {
                original_library,
                selected,
                ..
            }
            | SearchLibraryToBattlefieldTapped {
                original_library,
                selected,
                ..
            } => self.bs(original_library) || selected.as_ref().is_some_and(|b| self.b(b)),
            ResolveCounterUnlessPaysGeneric { .. }
            | ResolveCounterTargetUnlessPaysGeneric { .. } => false, // target contracts cannot bind live hidden objects
            LookTopSelectByTypeToHandBottomRest {
                original_prefix,
                progress,
                ..
            } => {
                self.bs(original_prefix)
                    || match progress {
                        LibraryPartitionProgress::MatchingSubsetChosen { selected } => {
                            self.bs(selected)
                        }
                        LibraryPartitionProgress::RestOrderChosen {
                            selected,
                            ordered_rest,
                        } => self.bs(selected) || self.bs(ordered_rest),
                    }
            }
            SearchLibraryToHandMany {
                original_library,
                selected,
                ..
            } => self.bs(original_library) || self.bs(selected),
            ExileChosenGraveyardCard {
                original_graveyard,
                chosen,
                expected_remaining_frames,
                ..
            } => {
                self.bs(original_graveyard) || self.b(chosen) || self.fs(expected_remaining_frames)
            }
            ExileChosenMatchingGraveyardCard {
                original_graveyard,
                candidates,
                chosen,
                then,
                expected_remaining_frames,
                ..
            } => {
                self.bs(original_graveyard)
                    || self.bs(candidates)
                    || self.b(chosen)
                    || self.op(then)
                    || self.fs(expected_remaining_frames)
            }
            SurveilLibraryOne {
                original_library,
                expected_remaining_frames,
                ..
            } => self.bs(original_library) || self.fs(expected_remaining_frames),
            SacrificeChosenCreature {
                original_candidates,
                chosen,
                expected_remaining_frames,
                ..
            } => {
                self.bs(original_candidates) || self.b(chosen) || self.fs(expected_remaining_frames)
            }
            PayManaThen {
                then,
                expected_remaining_frames,
                ..
            } => self.op(then) || self.fs(expected_remaining_frames),
            RevealedLibraryToGraveyardBatch {
                original_prefix,
                objects,
                ..
            } => self.bs(original_prefix) || self.bs(objects),
            DiscardRevealedHandCard {
                original_hand,
                eligible,
                selected,
                ..
            } => self.bs(original_hand) || self.bs(eligible) || self.b(selected),
            BeginSearchLibraryToBattlefieldTapped {
                expected_remaining_frames,
                ..
            } => self.fs(expected_remaining_frames),
            LinkedExileChosenHandCard {
                original_hand,
                chosen,
                source,
                expected_remaining_frames,
                ..
            } => {
                self.bs(original_hand)
                    || self.b(chosen)
                    || self.a(source)
                    || self.fs(expected_remaining_frames)
            }
            EnterUndercityRoom {
                binding,
                expected_remaining_frames,
                ..
            } => self.a(&binding.source) || self.fs(expected_remaining_frames),
            ResolveUndercityThrone {
                binding,
                original_library,
                revealed_prefix,
                candidates,
                chosen,
                expected_remaining_frames,
                ..
            } => {
                self.a(&binding.source)
                    || self.bs(original_library)
                    || self.bs(revealed_prefix)
                    || self.bs(candidates)
                    || self.b(chosen)
                    || self.fs(expected_remaining_frames)
            }
            LookAtTopMayReveal { top, then, .. } => self.b(top) || self.op(then),
        }
    }
    fn purpose(&self, p: &EffectTargetSelectionPurpose) -> bool {
        use EffectTargetSelectionPurpose::*;
        match p {
            OrderIntoGraveyard { .. } | OrderMilledIntoGraveyard => false,
            AttachReturningAura {
                aura,
                original_candidates,
                ..
            } => self.b(aura) || self.bs(original_candidates),
            OrderLookedLibraryTop {
                original_prefix, ..
            }
            | OrderRevealedIntoGraveyard {
                original_prefix, ..
            } => self.bs(original_prefix),
            PutHandCardOnLibraryTop { original_hand, .. } => self.bs(original_hand),
            SurveilLibraryOne {
                original_library,
                expected_remaining_frames,
                ..
            } => self.bs(original_library) || self.fs(expected_remaining_frames),
            ScryLibrary {
                original_prefix,
                stage,
                ..
            } => {
                self.bs(original_prefix)
                    || match stage {
                        ScrySelectionStage::ChooseBottomSubset => false,
                        ScrySelectionStage::OrderBottom { bottom_subset } => self.bs(bottom_subset),
                        ScrySelectionStage::OrderRetainedTop {
                            bottom_subset,
                            ordered_bottom,
                        } => self.bs(bottom_subset) || self.bs(ordered_bottom),
                    }
            }
            SearchLibraryToHand {
                original_library, ..
            }
            | SearchLibraryToHandMany {
                original_library, ..
            }
            | SearchLibraryToBattlefieldTapped {
                original_library, ..
            } => self.bs(original_library),
            LookTopSelectByTypeToHandBottomRest {
                original_prefix,
                stage,
                ..
            } => {
                self.bs(original_prefix)
                    || match stage {
                        LibraryPartitionSelectionStage::ChooseMatchingSubset => false,
                        LibraryPartitionSelectionStage::OrderRest { selected } => self.bs(selected),
                    }
            }
            ExileOneFromGraveyard {
                original_graveyard, ..
            } => self.bs(original_graveyard),
            ExileOneMatchingFromGraveyard {
                original_graveyard,
                candidates,
                then,
                ..
            } => self.bs(original_graveyard) || self.bs(candidates) || self.op(then),
            SacrificeCreature {
                original_candidates,
                ..
            }
            | UntapLands {
                original_candidates,
                ..
            } => self.bs(original_candidates),
            DuressDiscard {
                original_hand,
                eligible,
                ..
            } => self.bs(original_hand) || self.bs(eligible),
            LinkedExileNonlandFromRevealedHand {
                original_hand,
                source,
                ..
            } => self.bs(original_hand) || self.a(source),
            UndercityThroneCreature {
                binding,
                original_library,
                revealed_prefix,
                candidates,
                ..
            } => {
                self.a(&binding.source)
                    || self.bs(original_library)
                    || self.bs(revealed_prefix)
                    || self.bs(candidates)
            }
        }
    }
    fn choice(&self, c: &PendingEffectChoice) -> bool {
        match c {
            PendingEffectChoice::SelectTargets {
                selected,
                legal,
                purpose,
                ..
            } => {
                self.purpose(purpose)
                    || selected.iter().chain(legal).any(|c| {
                        c.expected_object.as_ref().is_some_and(|b| self.b(b))
                            || matches!(c.target,crate::state::Target::Object(id) if self.raw(id))
                    })
            }
            PendingEffectChoice::ChooseOption {
                options, purpose, ..
            } => {
                options.iter().any(|o| self.op(o)) || {
                    use EffectOptionChoicePurpose::*;
                    match purpose {
                        Generic => false,
                        OwnerLibraryTopOrBottom {
                            object,
                            expected_remaining_frames,
                            ..
                        }
                        | OwnerLibrarySecondOrBottom {
                            object,
                            expected_remaining_frames,
                            ..
                        } => self.b(object) || self.fs(expected_remaining_frames),
                        ExploreNonlandTop { top, .. } | SurveilTopCard { top, .. } => self.b(top),
                        ChooseColor {
                            expected_remaining_frames,
                            ..
                        } => self.fs(expected_remaining_frames),
                        UndercityRoute {
                            binding,
                            expected_remaining_frames,
                            ..
                        } => self.a(&binding.source) || self.fs(expected_remaining_frames),
                    }
                }
            }
            PendingEffectChoice::ChooseBoolean { purpose, .. } => {
                use EffectBooleanChoicePurpose::*;
                match purpose {
                    ShuffleLibrary { .. }
                    | CounterUnlessPaysGeneric { .. }
                    | CounterTargetUnlessPaysGeneric { .. } => false,
                    PayManaThen { then, .. } => self.op(then),
                    SearchLibraryToBattlefieldTapped {
                        expected_remaining_frames,
                        ..
                    } => self.fs(expected_remaining_frames),
                    PayExileFromGraveyardThen {
                        original_graveyard,
                        candidates,
                        then,
                        ..
                    } => self.bs(original_graveyard) || self.bs(candidates) || self.op(then),
                    LookAtTopMayRevealThen { top, then, .. } => self.b(top) || self.op(then),
                }
            }
        }
    }
}

pub(super) fn op_conflicts(state: &GameState, pool: &[ObjectId], op: &EffectOp) -> bool {
    Scan { state, pool }.op(op)
}

pub(super) fn conflicts(
    state: &GameState,
    pool: &[ObjectId],
    plan: Option<&crate::effect::library_choice_search_v2::Plan>,
) -> bool {
    let s = Scan { state, pool };
    if let Some(p) = &state.engine.pending_effect {
        if s.fs(&p.frames)
            || p.choice.as_ref().is_some_and(|c| {
                if !plan.is_some_and(|p| p.matches(c)) {
                    return s.choice(c);
                }
                // Throne exempts only its private full-library snapshot. Public
                // reveal, source and selected/legal references still cannot move.
                if let PendingEffectChoice::SelectTargets {
                    purpose:
                        EffectTargetSelectionPurpose::UndercityThroneCreature {
                            binding,
                            revealed_prefix,
                            candidates,
                            ..
                        },
                    selected,
                    legal,
                    ..
                } = c
                {
                    s.a(&binding.source)
                        || s.bs(revealed_prefix)
                        || s.bs(candidates)
                        || selected.iter().chain(legal).any(|c| {
                            c.expected_object.as_ref().is_some_and(|b| s.b(b))
                                || matches!(c.target,crate::state::Target::Object(id) if s.raw(id))
                        })
                } else {
                    false
                }
            })
            || p.ctx.discarded.iter().any(|id| s.raw(*id))
            || p.ctx
                .targets
                .iter()
                .any(|t| matches!(t,crate::state::Target::Object(id) if s.raw(*id)))
            || p.ctx
                .hidden_ability_source
                .is_some_and(|x| s.same(x.object, x.zone_change_count))
            || p.ctx
                .ability_source_contract
                .as_ref()
                .is_some_and(|a| s.a(a))
        {
            return true;
        }
        if let Some(g) = &p.answered_choice_guard {
            use EffectAnsweredChoiceGuard::*;
            let guard_conflicts = match g {
                OwnerLibrarySecondOrBottom { frame }
                | CounterUnlessPaysGeneric { frame }
                | CounterTargetUnlessPaysGeneric { frame }
                | ExileOneFromGraveyard { frame }
                | SurveilLibraryOne { frame }
                | ExileOneMatchingFromGraveyard { frame }
                | SacrificeCreature { frame }
                | PayManaThen { frame }
                | LinkedExileFromRevealedHand { frame }
                | SearchLibraryToBattlefieldTapped { frame }
                | UndercityRoute { frame }
                | UndercityThrone { frame } => s.f(frame),
                AttachReturningAura {
                    aura,
                    host,
                    remaining_frames,
                    ..
                } => s.b(aura) || s.b(host) || s.fs(remaining_frames),
            };
            if guard_conflicts {
                return true;
            }
        }
    }
    state.engine.event_log.iter().any(|event| {
        use crate::event::CommittedEvent::*;
        match event {
            Damage { source, target, .. } => {
                s.raw(*source) || matches!(target,crate::state::Target::Object(id) if s.raw(*id))
            }
            ZoneChange { object, .. }
            | Tap { object }
            | CreateToken { object, .. }
            | Sacrificed { object, .. }
            | Transformed { object, .. } => s.raw(*object),
            PlusOneCountersAdded { object, .. }
            | PrintedAbilitiesRemovedBeforeZoneChange { object, .. } => s.raw(*object),
            Draw { object, .. } => object.is_some_and(|id| s.raw(id)),
            SpellCast { spell, .. } => s.raw(*spell),
            Targeted { target, .. } => s.raw(*target),
            CombatDamageToPlayer { source, .. }
            | SagaChapter { source, .. }
            | DeclaredAttacker { source, .. } => s.raw(*source),
            OptionalAdditionalCostPaid {
                source,
                paid_cost_refs,
                ..
            } => s.raw(*source) || paid_cost_refs.iter().any(|r| s.raw(r.object)),
            InitiativeTrigger { binding } => s.raw(binding.source.source),
            MonarchTrigger { binding } => s.raw(binding.source.source),
            LifeLoss { .. }
            | LifeGain { .. }
            | ManaAdded { .. }
            | UpkeepBegan { .. }
            | CrimeCommitted { .. }
            | BeginningEndStep { .. } => false,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::PlayerId;
    use crate::policy_observation_v6::tests::{put, ready_state};
    use crate::state::Zone;

    #[test]
    fn bound_counter_doubling_keeps_the_referenced_incarnation_out_of_resampling() {
        let mut state = ready_state();
        let object = put(&mut state, PlayerId::P1, "Forest", Zone::Library);
        let other = put(&mut state, PlayerId::P1, "Island", Zone::Library);
        let binding = EffectObjectBinding {
            object,
            expected_zone: Zone::Library,
            expected_zone_change_count: state.objects.get(object).zone_change_count,
        };
        let op = EffectOp::DoublePlusOneCountersOnBoundObject { object: binding };
        assert!(op_conflicts(&state, &[object], &op));
        assert!(!op_conflicts(&state, &[other], &op));
        state.objects.get_mut(object).zone_change_count += 1;
        assert!(!op_conflicts(&state, &[object], &op));
    }

    #[test]
    fn returning_aura_retains_both_incarnations_and_all_attachment_candidates() {
        let mut state = ready_state();
        let aura_id = put(&mut state, PlayerId::P1, "Forest", Zone::Library);
        let host_id = put(&mut state, PlayerId::P1, "Island", Zone::Library);
        let other = put(&mut state, PlayerId::P1, "Swamp", Zone::Library);
        let bind = |object| EffectObjectBinding {
            object,
            expected_zone: Zone::Library,
            expected_zone_change_count: state.objects.get(object).zone_change_count,
        };
        let aura = bind(aura_id);
        let host = bind(host_id);
        let op = EffectOp::PutBoundAuraOntoBattlefieldAttached { aura, host };
        let purpose = EffectTargetSelectionPurpose::AttachReturningAura {
            aura,
            original_candidates: vec![host],
            canonical_path: vec![],
        };
        for object in [aura_id, host_id] {
            let scan = Scan {
                state: &state,
                pool: &[object],
            };
            assert!(scan.op(&op));
            assert!(scan.purpose(&purpose));
        }
        let scan = Scan {
            state: &state,
            pool: &[other],
        };
        assert!(!scan.op(&op));
        assert!(!scan.purpose(&purpose));
    }

    #[test]
    fn counter_placement_history_keeps_its_physical_object_out_of_resampling() {
        let mut state = ready_state();
        let object = put(&mut state, PlayerId::P1, "Forest", Zone::Library);
        let other = put(&mut state, PlayerId::P1, "Island", Zone::Library);
        state
            .engine
            .event_log
            .push(crate::event::CommittedEvent::PlusOneCountersAdded {
                object,
                zone_change_count: state.objects.get(object).zone_change_count,
                player: PlayerId::P1,
                count: 1,
            });
        assert!(conflicts(&state, &[object], None));
        assert!(!conflicts(&state, &[other], None));
    }
    #[test]
    fn surveil_selection_and_completion_retain_library_and_remaining_frame_bindings() {
        let mut state = ready_state();
        let object = put(&mut state, PlayerId::P1, "Forest", Zone::Library);
        let other = put(&mut state, PlayerId::P1, "Island", Zone::Library);
        let unrelated = put(&mut state, PlayerId::P1, "Mountain", Zone::Library);
        let binding = |id| EffectObjectBinding {
            object: id,
            expected_zone: Zone::Library,
            expected_zone_change_count: state.objects.get(id).zone_change_count,
        };
        let original_library = vec![binding(object)];
        let expected_remaining_frames = vec![EffectFrame::Program {
            op: EffectOp::DoublePlusOneCountersOnBoundObject {
                object: binding(other),
            },
            path: vec![],
        }];
        let frame = EffectFrame::SurveilLibraryOne {
            player: PlayerId::P1,
            original_library: original_library.clone(),
            put_in_graveyard: false,
            path: vec![],
            expected_remaining_frames: expected_remaining_frames.clone(),
        };
        let purpose = EffectTargetSelectionPurpose::SurveilLibraryOne {
            player: PlayerId::P1,
            original_library,
            canonical_path: vec![],
            expected_remaining_frames,
        };
        for id in [object, other] {
            let pool = [id];
            let scan = Scan {
                state: &state,
                pool: &pool,
            };
            assert!(scan.f(&frame));
            assert!(scan.purpose(&purpose));
        }
        let pool = [unrelated];
        let scan = Scan {
            state: &state,
            pool: &pool,
        };
        assert!(!scan.f(&frame));
        assert!(!scan.purpose(&purpose));
        state.objects.get_mut(object).zone_change_count += 1;
        let pool = [object];
        let scan = Scan {
            state: &state,
            pool: &pool,
        };
        assert!(!scan.f(&frame));
        assert!(!scan.purpose(&purpose));
    }
    #[test]
    fn removed_ability_history_keeps_its_physical_object_out_of_resampling() {
        let mut state = ready_state();
        let object = put(&mut state, PlayerId::P1, "Forest", Zone::Library);
        let other = put(&mut state, PlayerId::P1, "Island", Zone::Library);
        state.engine.event_log.push(
            crate::event::CommittedEvent::PrintedAbilitiesRemovedBeforeZoneChange {
                object,
                zone_change_count: state.objects.get(object).zone_change_count,
            },
        );
        assert!(conflicts(&state, &[object], None));
        assert!(!conflicts(&state, &[other], None));
    }
}
