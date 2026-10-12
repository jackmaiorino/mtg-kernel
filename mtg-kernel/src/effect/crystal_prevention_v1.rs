//! Scoped affected-player ordering for fixed, definition-owned noncombat damage.
//! No journal row or stack field is materialized on predecessor paths.
use super::*;
use crate::event::{ActiveReplacement, CommittedEvent, DamageProposed, ReplacementEffectKind};
use crate::state::CrystalPreventionStartedV1;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PreventionV1 {
    Barricade(crate::state::AbilitySourceContractV4),
    Protection(EffectObjectBinding),
    Replacement(ActiveReplacement),
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChoiceV1 {
    pub source: ObjectId,
    /// Only the authenticated finalized source-power receipt may supply LKI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub historical_source: Option<StackItemId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch: Option<BatchProgressV2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creature_power: Option<u32>,
    pub target: Target,
    pub original_amount: i32,
    pub remaining_amount: i32,
    pub touched: Vec<u32>,
    pub chooser: PlayerId,
    pub path: Vec<u16>,
    pub options: Vec<PreventionV1>,
    pub remaining_frames: Vec<EffectFrame>,
}
impl ChoiceV1 {
    fn damage(&self) -> DamageProposed {
        DamageProposed {
            source: self.source,
            target: self.target,
            amount: self.remaining_amount,
            touched_by: self.touched.clone(),
            is_combat: false,
            historical_source: self.historical_source,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AnswerV1 {
    pub before: ChoiceV1,
    pub option: u16,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BoundaryV1 {
    pub history_index: u32,
    pub previous: Option<u32>,
    pub item: Box<StackItem>,
    pub root: EffectOp,
    pub pending: Option<Box<EffectContinuation>>,
    pub answer: Option<AnswerV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ShieldOperationV1 {
    pub history_index: u32,
    pub before: ActiveReplacement,
    pub damage_amount: i32,
    pub after: Option<ActiveReplacement>,
}
/// Produced by the actual owning interpreter before its resulting boundary.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExecutionV1 {
    pub history_index: u32,
    pub previous: u32,
    pub item: Box<StackItem>,
    pub before: Box<EffectContinuation>,
    pub after: Option<Box<EffectContinuation>>,
    pub steps: Vec<ExecutionStepV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch_commit: Option<BatchCommitV2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discard_commit: Option<DiscardCommitV2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program_commit: Option<u32>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProgramChoiceOperationV3 {
    pub history_index: u32,
    pub previous: u32,
    pub item: Box<StackItem>,
    pub before: Box<EffectContinuation>,
    pub option: u16,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExecutionStepV1 {
    pub frame: EffectFrame,
    pub conditional_result: Option<bool>,
    pub offered_menu: Option<PendingEffectChoice>,
    pub damage_history: Vec<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nondamage_history: Vec<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch_capture: Option<BatchCaptureV2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creature_power_capture: Option<CreaturePowerCaptureV2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discard_capture: Option<DiscardCaptureV2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub damage_operation: Option<u32>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BatchDamageV2 {
    pub source: ObjectId,
    pub target: Target,
    pub amount: i32,
    pub touched: Vec<u32>,
}
impl BatchDamageV2 {
    fn proposed(&self) -> DamageProposed {
        DamageProposed {
            source: self.source,
            target: self.target,
            amount: self.amount,
            touched_by: self.touched.clone(),
            is_combat: false,
            historical_source: None,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BatchOutcomeV2 {
    Waiting,
    Prevented,
    Damage(BatchDamageV2),
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BatchRecipientV2 {
    pub object: EffectObjectBinding,
    pub owner: PlayerId,
    pub controller: PlayerId,
    pub card_def: u16,
    pub selected: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BatchCaptureV2 {
    pub ctx: ExecCtx,
    pub op: EffectOp,
    pub path: Vec<u16>,
    pub roster: Vec<BatchRecipientV2>,
    pub proposals: Vec<BatchDamageV2>,
    pub options: Vec<Option<Vec<PreventionV1>>>,
    pub initial_outcomes: Vec<BatchOutcomeV2>,
    pub choice_order: Vec<usize>,
    pub active_player: PlayerId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fight: Option<FightCaptureV3>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preparation: Option<u32>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FightCaptureV3 {
    pub creatures: [Option<crate::state::GameObject>; 2],
    pub signed_powers: [i32; 2],
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BatchProgressV2 {
    /// Exact independent owning-execution occurrence, not a pending digest.
    pub capture: u32,
    pub cursor: usize,
    pub outcomes: Vec<BatchOutcomeV2>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BatchCommitV2 {
    pub capture: u32,
    pub outcomes: Vec<BatchOutcomeV2>,
    pub damage_history: Vec<u32>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CreaturePowerCaptureV2 {
    pub ctx: ExecCtx,
    pub op: EffectOp,
    pub path: Vec<u16>,
    pub source_legal: bool,
    pub recipient_legal: bool,
    pub source_before: Option<crate::state::GameObject>,
    pub source_after: Option<crate::state::GameObject>,
    pub signed_power: i32,
    pub counter_history: Vec<u32>,
    pub damage: Option<BatchDamageV2>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DamageOperationV3 {
    pub history_index: u32,
    pub ctx: ExecCtx,
    pub op: EffectOp,
    pub path: Vec<u16>,
    pub input: BatchDamageV2,
    pub historical_source: Option<StackItemId>,
    pub preventers: Vec<PreventionV1>,
    pub output: Option<BatchDamageV2>,
    pub damage_history: Option<u32>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DiscardCaptureV2 {
    pub ctx: ExecCtx,
    pub op: EffectOp,
    pub path: Vec<u16>,
    pub hand: Vec<EffectObjectBinding>,
    pub library: Vec<EffectObjectBinding>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DiscardCommitV2 {
    pub capture: u32,
    pub selected: Option<EffectObjectBinding>,
    pub discard_history: Option<u32>,
    pub draw_history: Option<u32>,
}
pub(crate) fn shield_operation(
    state: &mut GameState,
    replacement: &ActiveReplacement,
    amount: i32,
) -> Result<(), String> {
    if !boundaries(state).any(|b| b.pending.as_deref().and_then(|p| p.choice.as_ref())
        .is_some_and(|c| matches!(c, PendingEffectChoice::ChooseOption {
            purpose: EffectOptionChoicePurpose::CrystalPreventionV1 { choice }, ..
        } if choice.options.iter().any(|o| matches!(o, PreventionV1::Replacement(r) if r.id == replacement.id))))) {
        return Ok(());
    }
    let ReplacementEffectKind::PreventNextDamage { target, remaining } = replacement.kind else {
        return Ok(());
    };
    if amount <= 0 || remaining <= 0 {
        return Err("Crystal shield operation has no positive damage".into());
    }
    let left = remaining - amount.min(remaining);
    let after = (left > 0).then(|| ActiveReplacement {
        id: replacement.id,
        source: replacement.source,
        kind: ReplacementEffectKind::PreventNextDamage {
            target,
            remaining: left,
        },
    });
    let operation = ShieldOperationV1 {
        history_index: u32::try_from(state.engine.event_history.len())
            .map_err(|_| "Crystal history exceeds u32")?,
        before: replacement.clone(),
        damage_amount: amount,
        after,
    };
    state
        .engine
        .event_history
        .push(CommittedEvent::CrystalShieldOperationV1 { operation });
    Ok(())
}
fn validate_shields(state: &GameState) -> Result<(), String> {
    let mut expected = std::collections::BTreeMap::<u32, Option<ActiveReplacement>>::new();
    for (index, event) in state.engine.event_history.iter().enumerate() {
        match event {
            CommittedEvent::CrystalPreventionBoundaryV1 { boundary } => {
                let Some(PendingEffectChoice::ChooseOption {
                    purpose: EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
                    ..
                }) = boundary.pending.as_deref().and_then(|p| p.choice.as_ref())
                else {
                    continue;
                };
                for option in &choice.options {
                    if let PreventionV1::Replacement(r) = option {
                        if !matches!(r.kind, ReplacementEffectKind::PreventNextDamage { .. }) {
                            continue;
                        }
                        if let Some(before) = expected.get(&r.id) {
                            if before.as_ref() != Some(r) {
                                return Err(
                                    "Crystal shield capture changed its operation history".into()
                                );
                            }
                        } else {
                            expected.insert(r.id, Some(r.clone()));
                        }
                    }
                }
            }
            CommittedEvent::CrystalShieldOperationV1 { operation: op } => {
                if usize::try_from(op.history_index).ok() != Some(index)
                    || expected.get(&op.before.id).and_then(|r| r.as_ref()) != Some(&op.before)
                {
                    return Err(
                        "Crystal shield operation lost its exact previous installation".into(),
                    );
                }
                let ReplacementEffectKind::PreventNextDamage { target, remaining } = op.before.kind
                else {
                    return Err("Crystal shield operation changed replacement kind".into());
                };
                if remaining <= 0 || op.damage_amount <= 0 {
                    return Err("Crystal shield consumed nonpositive damage".into());
                }
                let left = remaining - remaining.min(op.damage_amount);
                let after = (left > 0).then(|| ActiveReplacement {
                    id: op.before.id,
                    source: op.before.source,
                    kind: ReplacementEffectKind::PreventNextDamage {
                        target,
                        remaining: left,
                    },
                });
                if op.after != after {
                    return Err("Crystal shield arithmetic changed".into());
                }
                expected.insert(op.before.id, after);
            }
            _ => {}
        }
    }
    for (id, replacement) in expected {
        let current = state
            .engine
            .active_replacements
            .iter()
            .filter(|r| r.id == id)
            .collect::<Vec<_>>();
        if match replacement {
            Some(expected) => current.len() != 1 || *current[0] != expected,
            None => !current.is_empty(),
        } {
            return Err("Crystal shield installation was erased or resurrected".into());
        }
    }
    Ok(())
}
/// A private loan acquired against the complete, genuine live row before pop.
pub(crate) struct OwnerV1 {
    original: StackItem,
    expected: StackItem,
    root: EffectOp,
    initial: bool,
    gorehorn: Option<crate::gorehorn_damage_v1::OwnerV1>,
    equipment: Option<crate::equipment_damage_source_v4::OwnerV4>,
}
impl OwnerV1 {
    pub(crate) fn permits_damage(
        &self,
        state: &GameState,
        event: &crate::event::ProposedEvent,
    ) -> bool {
        matches!(event, crate::event::ProposedEvent::Damage(d)
            if d.source == self.expected.source && !d.is_combat
                && (d.historical_source.is_none()
                    || d.historical_source == Some(self.expected.v4.stack_item_id)
                        && crate::source_power_activation_v1::damage_source(
                            state, d.source, self.expected.v4.stack_item_id).is_ok()))
            && !state
                .stack
                .iter()
                .any(|i| i.v4.stack_item_id == self.expected.v4.stack_item_id)
            && state.engine.pending_effect.is_none()
    }
    pub(crate) fn permits_batch(
        &self,
        state: &GameState,
        capture: &BatchCaptureV2,
        events: &[crate::event::ProposedEvent],
    ) -> bool {
        let removed = !state
            .stack
            .iter()
            .any(|i| i.v4.stack_item_id == self.expected.v4.stack_item_id)
            && state.engine.pending_effect.is_none();
        let live = state
            .stack
            .iter()
            .filter(|i| i.v4.stack_item_id == self.expected.v4.stack_item_id)
            .collect::<Vec<_>>();
        let waiting = live.as_slice() == [&self.expected]
            && state.engine.pending_effect.as_ref().is_some_and(|p| {
                p.resolving_item == self.expected
                    && last(state, &self.expected).and_then(|b| b.pending.as_deref()) == Some(p)
            });
        (removed || waiting)
            && capture.ctx == context(&self.expected)
            && effect_op_at_structural_path(&self.root, &capture.path) == Some(&capture.op)
            && (matches!(capture.op, EffectOp::DamageAllCreatures { .. })
                || fight_recipe(&capture.op))
            && events.iter().all(|e| {
                matches!(e, crate::event::ProposedEvent::Damage(d)
                if !d.is_combat && d.historical_source.is_none()
                    && capture.proposals.iter().any(|original| original.source == d.source && original.target == d.target
                        && d.amount > 0 && d.amount <= original.amount))
            })
    }
    pub(crate) fn permits_creature_power(
        &self,
        state: &GameState,
        capture: &CreaturePowerCaptureV2,
        event: &crate::event::ProposedEvent,
    ) -> bool {
        let damage = capture.damage.as_ref().map(BatchDamageV2::proposed);
        capture.ctx == context(&self.expected)
            && effect_op_at_structural_path(&self.root, &capture.path) == Some(&capture.op)
            && matches!(event, crate::event::ProposedEvent::Damage(d) if damage.as_ref() == Some(d))
            && !state
                .stack
                .iter()
                .any(|i| i.v4.stack_item_id == self.expected.v4.stack_item_id)
            && state.engine.pending_effect.is_none()
            && validate_live_creature_power(state, capture).is_ok()
    }
    pub(crate) fn permits_discard_draw(
        &self,
        state: &GameState,
        capture: &DiscardCaptureV2,
        selected: EffectObjectBinding,
    ) -> bool {
        let live = state
            .stack
            .iter()
            .filter(|i| i.v4.stack_item_id == self.expected.v4.stack_item_id)
            .collect::<Vec<_>>();
        live.as_slice() == [&self.expected]
            && state.engine.pending_effect.as_ref().is_some_and(|p| {
                p.resolving_item == self.expected
                    && last(state, &self.expected).and_then(|b| b.pending.as_deref()) == Some(p)
            })
            && capture.ctx == context(&self.expected)
            && effect_op_at_structural_path(&self.root, &capture.path) == Some(&capture.op)
            && optional_discard_draw(&capture.op)
            && capture.hand.contains(&selected)
            && state
                .engine
                .event_history
                .iter()
                .rev()
                .take_while(|e| !matches!(e, CommittedEvent::CrystalPreventionBoundaryV1 { .. }))
                .any(|e| {
                    matches!(e, CommittedEvent::ZoneChange { object, from: Zone::Hand,
                    to: Zone::Graveyard | Zone::Exile, .. } if *object == selected.object)
                })
    }
    pub(crate) fn controller(&self) -> PlayerId {
        self.expected.controller
    }
    pub(crate) fn permits_life_gain(
        &self,
        state: &GameState,
        event: &crate::event::ProposedEvent,
        player: PlayerRef,
        amount: i32,
    ) -> bool {
        matches!(player, PlayerRef::Controller | PlayerRef::Opponent)
            && matches!(event, crate::event::ProposedEvent::LifeGain(gain)
                if gain.player == context(&self.expected).resolve_player(player, state)
                    && gain.amount == amount && gain.touched_by.is_empty())
            && !state
                .stack
                .iter()
                .any(|i| i.v4.stack_item_id == self.expected.v4.stack_item_id)
            && state.engine.pending_effect.is_none()
    }
    pub(crate) fn completion_matches(&self, item: &StackItem) -> bool {
        *item == self.expected
    }
}
fn context(item: &StackItem) -> ExecCtx {
    ExecCtx {
        stack_item_id: Some(item.v4.stack_item_id),
        source: item.source,
        controller: item.controller,
        targets: item.targets.clone(),
        target_contracts: item.v4.target_contracts.clone(),
        discarded: item.discarded.clone(),
        paid_cost_refs: item.v4.paid_cost_refs.clone(),
        hidden_ability_source: item.v4.hidden_ability_source,
        ability_source_contract: item.v4.ability_source_contract,
        cauldron_grant: item.v4.cauldron_grant,
        kicked: item.kicked,
        optional_additional_cost_paid: item.v4.optional_additional_cost_paid,
        x_value: item.v4.x_value,
    }
}
fn probe(item: &StackItem) -> EffectContinuation {
    EffectContinuation {
        resolving_item: item.clone(),
        ctx: context(item),
        frames: Vec::new(),
        choice: None,
        answered_choice_guard: None,
    }
}
/// A positive classification, never an absence-of-known-damage guess. These
/// leaves cannot emit damage, and every nested branch must also be classified.
/// Generated Ward roots retain the ordinary stack and payment validators.
pub(crate) fn ordinary_nondamage(op: &EffectOp) -> bool {
    match op {
        EffectOp::Sequence(ops) => ops.iter().all(ordinary_nondamage),
        EffectOp::Conditional { then, else_, .. } => {
            ordinary_nondamage(then) && ordinary_nondamage(else_)
        }
        EffectOp::Choice { options, .. } => options.iter().all(ordinary_nondamage),
        EffectOp::MayPayCostThen {
            then, otherwise, ..
        } => ordinary_nondamage(then) && otherwise.as_deref().is_none_or(ordinary_nondamage),
        EffectOp::MayPayManaThen { then, .. }
        | EffectOp::MayExileFromPlayersGraveyardMatchingThen { then, .. } => {
            ordinary_nondamage(then)
        }
        // These typed leaves change zones, counters, continuous effects, mana,
        // or life without proposing damage. Any resulting trigger is separately
        // published and must pass its own root classification when it resolves.
        // Keep custom wrappers and unmaterialized Bind roots fail closed.
        EffectOp::MoveObject { .. }
        | EffectOp::MoveBoundObject { .. }
        | EffectOp::MoveAllTargets { .. }
        | EffectOp::Sacrifice { .. }
        | EffectOp::SacrificeCreature { .. }
        | EffectOp::RevealTopAndPartitionByType { .. }
        | EffectOp::MillCards { .. }
        | EffectOp::LookAtLibraryTopAndReorder { .. }
        | EffectOp::MayShuffleLibrary { .. }
        | EffectOp::PutCardsFromHandOnLibraryTop { .. }
        | EffectOp::Scry { .. }
        | EffectOp::Surveil { .. }
        | EffectOp::SurveilOne { .. }
        | EffectOp::SearchLibraryToHand { .. }
        | EffectOp::SearchLibraryToHandUpTo { .. }
        | EffectOp::SearchLibraryToBattlefieldTapped { .. }
        | EffectOp::SearchLibraryCardsToDestination { .. }
        | EffectOp::LookTopSelectByTypeToHandBottomRest { .. }
        | EffectOp::LookTopPickToHandBottomRest { .. }
        | EffectOp::LookTopMayTakeCreatureManaValueAtMostToHandBottomRest { .. }
        | EffectOp::LookTopTakeCreaturesManaValueAtMostThenShuffle { .. }
        | EffectOp::RevealUntilCardTypeAndMill { .. }
        | EffectOp::PutObjectInOwnersLibrarySecondOrBottom { .. }
        | EffectOp::PutObjectInOwnersLibraryTopOrBottom { .. }
        | EffectOp::PutBoundObjectInOwnersLibrary { .. }
        | EffectOp::RevealTargetHandChooseNoncreatureNonlandDiscard { .. }
        | EffectOp::RevealTargetHandChooseNonlandDiscard { .. }
        | EffectOp::RevealHandChooseNonlandToLinkedExile { .. }
        | EffectOp::ReturnLinkedExiledCardToOwnersHand
        | EffectOp::ImpulseDraw { .. }
        | EffectOp::ExilePlayersGraveyard { .. }
        | EffectOp::ExileOneFromPlayersGraveyard { .. }
        | EffectOp::ExileAllGraveyards
        | EffectOp::ExileAllArtifactTargets
        | EffectOp::ExileTargetPlayersGraveyards
        | EffectOp::DestroyTargetLandThenMaySearchBasicTapped { .. }
        | EffectOp::TapObject { .. }
        | EffectOp::UntapObject { .. }
        | EffectOp::SkipNextUntap { .. }
        | EffectOp::AttachSourceToTarget { .. }
        | EffectOp::AddMana { .. }
        | EffectOp::AddManaDynamic { .. }
        | EffectOp::CreateToken { .. }
        | EffectOp::CreateTokenAndAttachSource { .. }
        | EffectOp::CreateTokensDynamic { .. }
        | EffectOp::DestroyObject { .. }
        | EffectOp::DestroyObjectThenCreateTokens { .. }
        | EffectOp::DestroyAllCreatures
        | EffectOp::AddCountersToTarget { .. }
        | EffectOp::AddMinusOneMinusOneCounter { .. }
        | EffectOp::PutPlusOnePlusOneCounter { .. }
        | EffectOp::PutPlusOnePlusOneCounterOnBoundObject { .. }
        | EffectOp::PutPlusOnePlusOneCounterOnTargetOtherThan { .. }
        | EffectOp::PutPlusOnePlusOneCounterOnTriggerEventObject { .. }
        | EffectOp::AddPlusOnePlusOneCounters { .. }
        | EffectOp::AddPlusOneCounterToAbilitySource
        | EffectOp::PumpControlled { .. }
        | EffectOp::PumpTargetUntilEndOfTurnDynamic { .. }
        | EffectOp::PumpTargetByControlledSubtypeCount { .. }
        | EffectOp::PumpAllUntilEndOfTurn { .. }
        | EffectOp::BoostControlledCreaturesUntilEndOfTurn { .. }
        | EffectOp::BoostBoundObjectUntilEndOfTurn { .. }
        | EffectOp::GrantKeywordTargetUntilEndOfTurn { .. }
        | EffectOp::GrantKeywordToDeclaredTargetsUntilEndOfTurn { .. }
        | EffectOp::PreventTargetBlockingUntilEndOfTurn { .. }
        | EffectOp::PutSourceOntoBattlefieldAttachedToTarget { .. }
        | EffectOp::PutSourceOntoBattlefieldWithXPlusOneCounters
        | EffectOp::ReturnTargetPermanentToBattlefield { .. }
        | EffectOp::ReturnTwoDifferentControllerCreaturesToOwnersHands
        | EffectOp::ReturnOwnGraveyardCreaturesManaValueAtMost { .. }
        | EffectOp::ReturnAllGraveyardCreaturesUnderController
        | EffectOp::ExileTargetLinkedToSource { .. }
        | EffectOp::ReturnObjectsExiledBySource
        | EffectOp::ExileGraveyardTargetThenGrowSourceIfCreature
        | EffectOp::ExileSingleGraveyardTargetsThenDrainTwoV1
        | EffectOp::AuthorityEntryGainLifeV1 { .. }
        | EffectOp::WildwoodCounterFromReceiptV1 { .. }
        | EffectOp::CerberusUntapV1 { .. }
        | EffectOp::GainControlOfTargetCreatureUntilEndOfTurn { .. }
        | EffectOp::DrawCards { .. }
        | EffectOp::DiscardCards { .. }
        | EffectOp::GainLifeDynamic { .. }
        | EffectOp::GainLife { .. }
        | EffectOp::LoseLife { .. }
        | EffectOp::CounterUnlessPaysGeneric { .. }
        | EffectOp::CounterUnlessPaysLifeWithChoice { .. }
        | EffectOp::CounterTargetUnlessPaysGeneric { .. } => true,
        _ => false,
    }
}
/// Reconstruct immutable source/amount from the actual definition-owned leaf.
/// Current target legality is checked by the normal stack resolution boundary;
/// historical replay must not sample a later battlefield incarnation.
fn original_damage(
    state: &GameState,
    ctx: &ExecCtx,
    op: &EffectOp,
) -> Result<DamageProposed, String> {
    let (target, amount, historical_source) = match op {
        EffectOp::DealDamage { target, amount } => {
            let historical = ctx.stack_item_id.filter(|id| {
                crate::fixed_source_damage_v3::has_payment(state, *id)
                    || crate::gorehorn_damage_v1::has_publication(state, *id)
                    || crate::equipment_damage_source_v4::has_publication(state, *id)
            });
            if let Some(id) = historical {
                if crate::equipment_damage_source_v4::has_publication(state, id) {
                    if *op != crate::equipment_damage_source_v4::published_root(state, id)? {
                        return Err("Crystal equipment damage changed granted leaf".into());
                    }
                    crate::equipment_damage_source_v4::damage_source(state, ctx.source, id)?;
                } else if crate::gorehorn_damage_v1::has_publication(state, id) {
                    if !matches!(
                        op,
                        EffectOp::DealDamage {
                            target: TargetRef::Target(0),
                            amount: 2
                        }
                    ) {
                        return Err("Crystal Gorehorn damage changed printed leaf".into());
                    }
                    crate::gorehorn_damage_v1::damage_source(state, id, ctx.source)?;
                } else if *op != crate::fixed_source_damage_v3::printed_root() {
                    return Err("fixed historical damage changed printed instruction".into());
                }
                if crate::fixed_source_damage_v3::has_payment(state, id) {
                    crate::fixed_source_damage_v3::damage_source(state, ctx.source, id)?;
                }
            }
            (ctx.resolve_target(*target), *amount, historical)
        }
        EffectOp::StandardV1(crate::standard_cards_v1::StandardOpV1::SourceLastPowerDamageV1) => {
            let id = ctx
                .stack_item_id
                .ok_or("Crystal historical damage lost stack identity")?;
            crate::source_power_activation_v1::damage_source(state, ctx.source, id)?;
            let mut matches = state.engine.event_history.iter().filter_map(|e| match e {
                CommittedEvent::SourcePowerActivationCommittedV1 { item, power, .. }
                    if item.v4.stack_item_id == id
                        && item.source == ctx.source
                        && item.controller == ctx.controller
                        && item.v4.ability_source_contract == ctx.ability_source_contract =>
                {
                    Some(*power)
                }
                _ => None,
            });
            let power = matches
                .next()
                .ok_or("Crystal historical damage lost committed power")?;
            if matches.next().is_some() {
                return Err("Crystal historical damage duplicated committed power".into());
            }
            let target = ctx
                .targets
                .first()
                .copied()
                .ok_or("Crystal historical damage lost target")?;
            (target, power.max(0), Some(id))
        }
        _ => return Err("Crystal damage instruction is not supported".into()),
    };
    Ok(DamageProposed {
        source: ctx.source,
        target,
        amount,
        touched_by: Vec::new(),
        is_combat: false,
        historical_source,
    })
}
/// Equipment owns the grant recipe; the host owns the resulting trigger.
/// Reuse the actual historical grant validator, not the host's printed list.
fn equipment_damage_grant(item: &StackItem) -> bool {
    item.kind == crate::state::StackItemKind::TriggeredAbility
        && item.v4.granted_by.is_some_and(|grant| {
            crate::card_def::CARD_DEFS
                .get(usize::from(grant.card_def))
                .and_then(|def| def.equipment)
                .is_some_and(|equipment| equipment.noncreature_spell_damage_to_each_opponent > 0)
        })
}
fn owned_root(state: &GameState, pending: &EffectContinuation) -> Result<Box<EffectOp>, String> {
    if crate::equipment_damage_source_v4::tracked(state, &pending.resolving_item) {
        crate::equipment_damage_source_v4::validate_published_metadata(
            state,
            &pending.resolving_item,
        )?;
        return Ok(Box::new(crate::equipment_damage_source_v4::printed_root(
            &pending.resolving_item,
        )?));
    }
    if !equipment_damage_grant(&pending.resolving_item) {
        return validated_definition_owned_root_effect(state, pending);
    }
    let item = &pending.resolving_item;
    if pending.ctx != context(item)
        || crate::engine::validated_stack_item_target_spec(item, state)?.is_some()
    {
        return Err("Crystal equipment damage lost actual historical grant/context".into());
    }
    let grant = item
        .v4
        .granted_by
        .ok_or("Crystal equipment trigger lost grant source")?;
    let profile = crate::card_def::CARD_DEFS[usize::from(grant.card_def)]
        .equipment
        .ok_or("Crystal equipment damage lost printed descriptor")?;
    let root = EffectOp::DealDamage {
        target: TargetRef::Opponent,
        amount: i32::from(profile.noncreature_spell_damage_to_each_opponent),
    };
    if item.inline_effect.as_ref() != Some(&root) {
        return Err("Crystal equipment trigger changed its exact granted damage program".into());
    }
    Ok(Box::new(root))
}
fn live_equipment_damage_source(state: &GameState, item: &StackItem) -> bool {
    item.v4.ability_source_contract.is_some_and(|source| {
        state.objects.try_get(source.source).is_some_and(|live| {
            live.zone == crate::state::Zone::Battlefield
                && live.owner == source.owner
                && live.zone_change_count == source.zone_change_count
                && live.card_def == source.card_def
                && live.v4.face_down_v1.is_none()
                && live.spell_copy_origin.is_none()
        })
    })
}
/// Source-power payment authority remains byte-exact except for this module's
/// independently published start marker. This is not a generic stack rewrite.
pub(crate) fn source_power_payment_matches(
    state: &GameState,
    item: &StackItem,
    saved: &StackItem,
) -> bool {
    if item == saved {
        return true;
    }
    let Some(index) = item.v4.crystal_prevention_started_v1.0 else {
        return false;
    };
    let Some(CommittedEvent::CrystalPreventionBoundaryV1 { boundary }) =
        state.engine.event_history.get(index as usize)
    else {
        return false;
    };
    let mut original = item.clone();
    original.v4.crystal_prevention_started_v1 = CrystalPreventionStartedV1::default();
    let mut initial = probe(item);
    initial.frames.push(EffectFrame::Program {
        op: boundary.root.clone(),
        path: Vec::new(),
    });
    original == *saved
        && boundary.history_index == index
        && boundary.previous.is_none()
        && boundary.answer.is_none()
        && *boundary.item == *item
        && boundary.pending.as_deref() == Some(&initial)
        && (boundary.root
            == EffectOp::StandardV1(
                crate::standard_cards_v1::StandardOpV1::SourceLastPowerDamageV1,
            )
            || (crate::fixed_source_damage_v3::selected(saved)
                && boundary.root == crate::fixed_source_damage_v3::printed_root())
            || (crate::equipment_damage_source_v4::selected(saved)
                && crate::equipment_damage_source_v4::printed_root(saved)
                    .is_ok_and(|root| boundary.root == root))
            || (crate::gorehorn_damage_v1::selected(saved)
                && boundary.root == crate::trigger::gorehorn_raider_effect()))
}
fn fight_recipe(op: &EffectOp) -> bool {
    source_fight_recipe(op)
        || matches!(
            op,
            EffectOp::FightObjects {
                first: ObjectRef::Target(0),
                second: ObjectRef::Target(1),
                target_spec: crate::card_def::TargetSpec::ControlledCreatureThenOpponentCreature,
            }
        )
}
fn source_fight_recipe(op: &EffectOp) -> bool {
    matches!(
        op,
        EffectOp::FightObjects {
            first: ObjectRef::ThisSource,
            second: ObjectRef::Target(0),
            target_spec: crate::card_def::TargetSpec::OpponentControlledCreature
        }
    )
}
fn fight_refs(op: &EffectOp) -> Result<[ObjectRef; 2], String> {
    let EffectOp::FightObjects { first, second, .. } = op else {
        return Err("Crystal fight lost printed recipe".into());
    };
    if !fight_recipe(op) {
        return Err("Crystal fight carries unsupported references".into());
    }
    Ok([*first, *second])
}
fn optional_source_fight(op: &EffectOp) -> bool {
    matches!(op, EffectOp::Choice { controller: PlayerRef::Controller, options }
        if options.len() == 2 && options[0] == EffectOp::Sequence(Vec::new())
            && source_fight_recipe(&options[1]))
}
fn batch_recipe(op: &EffectOp) -> bool {
    matches!(op, EffectOp::DamageAllCreatures { .. }) || fight_recipe(op)
}
fn has_damage(op: &EffectOp) -> bool {
    if fight_recipe(op) {
        return true;
    }
    match op {
        EffectOp::DealDamage { .. }
        | EffectOp::DamageAllCreatures { .. }
        | EffectOp::CreatureTargetPowerDamage {
            plus1_plus1: 0.., ..
        }
        | EffectOp::StandardV1(crate::standard_cards_v1::StandardOpV1::SourceLastPowerDamageV1) => {
            true
        }
        EffectOp::Sequence(ops) => ops.iter().any(has_damage),
        EffectOp::Choice { options, .. } => options.iter().any(has_damage),
        EffectOp::Conditional { then, else_, .. } => has_damage(then) || has_damage(else_),
        _ => false,
    }
}
fn optional_discard_draw(op: &EffectOp) -> bool {
    matches!(op, EffectOp::MayPayCostThen {
        discard: 1, sacrifice_lands: 0, return_permanent: None, then, otherwise: None
    } if **then == (EffectOp::DrawCards { player: PlayerRef::Controller, count: 1 }))
}
fn supported(op: &EffectOp) -> bool {
    if optional_discard_draw(op) || fight_recipe(op) {
        return true;
    }
    match op {
        EffectOp::GainLife {
            player: PlayerRef::Controller | PlayerRef::Opponent,
            ..
        } => true,
        EffectOp::DealDamage { .. }
        | EffectOp::DamageAllCreatures { .. }
        | EffectOp::CreatureTargetPowerDamage {
            plus1_plus1: 0.., ..
        }
        | EffectOp::StandardV1(crate::standard_cards_v1::StandardOpV1::SourceLastPowerDamageV1) => {
            true
        }
        EffectOp::Sequence(ops) => ops.iter().all(supported),
        EffectOp::Choice { .. } => optional_source_fight(op),
        EffectOp::Conditional { then, else_, .. } => supported(then) && supported(else_),
        _ => false,
    }
}
pub(crate) fn requires_successor(state: &GameState) -> bool {
    crate::crystal_barricade_v1::active(state)
        || crate::equipment_damage_source_v4::requires_successor(state)
        || crate::fixed_source_damage_v3::requires_successor(state)
        || boundaries(state).next().is_some()
        || state.engine.event_history.iter().any(|event| {
            matches!(
                event,
                CommittedEvent::CrystalShieldOperationV1 { .. }
                    | CommittedEvent::CrystalExecutionV1 { .. }
                    | CommittedEvent::CrystalDamageOperationV3 { .. }
                    | CommittedEvent::CrystalBatchPreparedV3 { .. }
                    | CommittedEvent::CrystalProgramChoiceV3 { .. }
            )
        })
        || state
            .stack
            .iter()
            .any(|i| i.v4.crystal_prevention_started_v1.0.is_some())
        || is_choice(state)
}
pub(crate) fn mixed_environment(state: &GameState) -> bool {
    crate::crystal_barricade_v1::active(state)
        && state.engine.active_replacements.iter().any(|r|
            matches!(r.kind, ReplacementEffectKind::PreventNextDamage { remaining, .. } if remaining > 0))
}
/// Unsupported damage programs refuse before pop. Positively classified
/// nondamage programs keep their genuine predecessor resolution authority,
/// including generated Generic/Life Ward payment continuations.
pub(crate) fn acquire_owner(
    state: &GameState,
    item: &StackItem,
    initial: bool,
) -> Result<Option<OwnerV1>, String> {
    let started = item.v4.crystal_prevention_started_v1.0.is_some();
    if !started
        && !mixed_environment(state)
        && !crate::fixed_source_damage_v3::tracked(state, item)
        && !crate::equipment_damage_source_v4::tracked(state, item)
        && !(crate::gorehorn_damage_v1::selected(item)
            && crate::crystal_barricade_v1::active(state))
    {
        return Ok(None);
    }
    validate(state)?;
    if state
        .stack
        .iter()
        .filter(|i| i.v4.stack_item_id == item.v4.stack_item_id)
        .count()
        != 1
        || !state.stack.iter().any(|i| i == item)
    {
        return Err("Crystal owner did not acquire the genuine complete live item".into());
    }
    crate::engine::validated_stack_item_target_spec(item, state)?;
    // Generated Ward programs have their own definition/source/targeter
    // authentication. They are not printed trigger-list entries.
    if !started && item.inline_effect.as_ref().is_some_and(ordinary_nondamage) {
        return Ok(None);
    }
    let root = *owned_root(state, &probe(item))?;
    if !started && ordinary_nondamage(&root) {
        return Ok(None);
    }
    if !supported(&root) {
        return Err("mixed Crystal prevention needs a supported fixed-damage producer".into());
    }
    if !has_damage(&root) {
        return Ok(None);
    }
    if initial
        && equipment_damage_grant(item)
        && !crate::equipment_damage_source_v4::has_publication(state, item.v4.stack_item_id)
        && !live_equipment_damage_source(state, item)
    {
        return Err("Crystal equipment damage needs an original public live source; historical grant capture remains a successor prerequisite".into());
    }
    let mut expected = item.clone();
    if initial {
        if started
            || state.engine.pending_effect.is_some()
            || boundaries(state).any(|b| b.item.v4.stack_item_id == item.v4.stack_item_id)
        {
            return Err("Crystal resolution cannot restart".into());
        }
        expected.v4.crystal_prevention_started_v1 = CrystalPreventionStartedV1(Some(
            u32::try_from(state.engine.event_history.len())
                .map_err(|_| "Crystal history exceeds u32")?,
        ));
    } else if !started
        || state
            .engine
            .pending_effect
            .as_ref()
            .is_none_or(|p| p.resolving_item != *item)
    {
        return Err("Crystal resume lost its published owner".into());
    }
    let gorehorn = crate::gorehorn_damage_v1::acquire_owner(state, item)?;
    let equipment = crate::equipment_damage_source_v4::acquire_owner(state, item)?;
    Ok(Some(OwnerV1 {
        gorehorn,
        equipment,
        original: item.clone(),
        expected,
        root,
        initial,
    }))
}
fn boundaries(state: &GameState) -> impl Iterator<Item = &BoundaryV1> {
    state.engine.event_history.iter().filter_map(|e| match e {
        CommittedEvent::CrystalPreventionBoundaryV1 { boundary } => Some(boundary.as_ref()),
        _ => None,
    })
}
fn last<'a>(state: &'a GameState, item: &StackItem) -> Option<&'a BoundaryV1> {
    boundaries(state)
        .filter(|b| b.item.v4.stack_item_id == item.v4.stack_item_id)
        .last()
}
fn record(state: &mut GameState, owner: &OwnerV1, answer: Option<AnswerV1>) -> Result<(), String> {
    let index = u32::try_from(state.engine.event_history.len())
        .map_err(|_| "Crystal history exceeds u32")?;
    let boundary = BoundaryV1 {
        history_index: index,
        previous: last(state, &owner.expected).map(|b| b.history_index),
        item: Box::new(owner.expected.clone()),
        root: owner.root.clone(),
        pending: state.engine.pending_effect.clone().map(Box::new),
        answer,
    };
    state
        .engine
        .event_history
        .push(CommittedEvent::CrystalPreventionBoundaryV1 {
            boundary: Box::new(boundary),
        });
    Ok(())
}
pub(crate) fn validate(state: &GameState) -> Result<(), String> {
    validate_with_owner(state, None)
}
fn validate_with_owner(state: &GameState, owner: Option<&OwnerV1>) -> Result<(), String> {
    // This selected environment authenticates actual inherited trigger roots
    // before priority or automatic work. Generated Ward remains on its engine
    // validator; equipment grants retain the separate historical donor contract.
    if crate::crystal_barricade_v1::active(state) || boundaries(state).next().is_some() {
        for item in state
            .stack
            .iter()
            .filter(|item| item.kind == crate::state::StackItemKind::TriggeredAbility)
        {
            crate::engine::validated_stack_item_target_spec(item, state)?;
            if item.v4.crystal_prevention_started_v1.is_empty()
                && equipment_damage_grant(item)
                && !crate::equipment_damage_source_v4::has_publication(state, item.v4.stack_item_id)
                && !live_equipment_damage_source(state, item)
            {
                return Err("Crystal legacy equipment damage needs original live source".into());
            }
        }
    }
    if state.engine.event_log.iter().any(|event| {
        matches!(
            event,
            CommittedEvent::CrystalPreventionBoundaryV1 { .. }
                | CommittedEvent::CrystalShieldOperationV1 { .. }
                | CommittedEvent::CrystalExecutionV1 { .. }
                | CommittedEvent::CrystalDamageOperationV3 { .. }
                | CommittedEvent::CrystalBatchPreparedV3 { .. }
                | CommittedEvent::CrystalProgramChoiceV3 { .. }
        )
    }) {
        return Err("private Crystal ownership cannot enter the public trigger log".into());
    }
    validate_shields(state)?;
    validate_damage_operations(state)?;
    validate_batch_preparations(state)?;
    validate_program_operations(state)?;
    validate_executions(state)?;
    for (index, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::CrystalPreventionBoundaryV1 { boundary: b } = event else {
            continue;
        };
        if usize::try_from(b.history_index).ok() != Some(index) {
            return Err("Crystal boundary moved in history".into());
        }
        let previous = state.engine.event_history[..index]
            .iter()
            .rev()
            .find_map(|e| match e {
                CommittedEvent::CrystalPreventionBoundaryV1 { boundary }
                    if boundary.item.v4.stack_item_id == b.item.v4.stack_item_id =>
                {
                    Some(boundary.as_ref())
                }
                _ => None,
            });
        if b.previous != previous.map(|p| p.history_index)
            || previous.is_some_and(|p| p.pending.is_none() || p.item != b.item || p.root != b.root)
            || !supported(&b.root)
            || !has_damage(&b.root)
        {
            return Err("Crystal resolution history chain changed or restarted".into());
        }
        if b.answer.is_none() {
            if let Some(previous) = previous {
                let execution = state.engine.event_history.get(
                    index
                        .checked_sub(1)
                        .ok_or("Crystal execution boundary index")?,
                );
                if !matches!(execution, Some(CommittedEvent::CrystalExecutionV1 { execution })
                    if execution.previous == previous.history_index
                        && execution.item == b.item && execution.after == b.pending)
                {
                    return Err(
                        "Crystal boundary lost its actual owned execution occurrence".into(),
                    );
                }
            }
            if let Some(old) = previous.and_then(|p| p.pending.as_deref()) {
                let actual_discard = matches!(state.engine.event_history.get(index - 1),
                    Some(CommittedEvent::CrystalExecutionV1 { execution }) if (execution.discard_commit.is_some() || execution.program_commit.is_some())
                        && execution.previous == previous.unwrap().history_index && execution.after == b.pending);
                if old.choice.is_some() && b.pending.as_deref() != Some(old) && !actual_discard {
                    return Err(
                        "Crystal published choice advanced without an authenticated answer".into(),
                    );
                }
            }
            if let Some(PendingEffectChoice::ChooseOption {
                purpose: EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
                ..
            }) = b.pending.as_deref().and_then(|p| p.choice.as_ref())
            {
                let repeated = previous
                    .and_then(|p| p.pending.as_deref())
                    .is_some_and(|p| p.choice == b.pending.as_deref().unwrap().choice);
                if !repeated
                    && (choice.remaining_amount != choice.original_amount
                        || !choice.touched.is_empty())
                {
                    return Err("Crystal first damage proposal changed its original amount".into());
                }
            }
        }
        if let Some(answer) = b.answer.as_ref() {
            let previous = previous.ok_or("Crystal answer has no earlier published menu")?;
            let old = previous
                .pending
                .as_deref()
                .ok_or("Crystal answer follows closure")?;
            if old.choice.as_ref() != Some(&menu(answer.before.clone())) {
                return Err("Crystal answer changed its independent prior menu".into());
            }
            let mut expected = answer.before.clone();
            match expected
                .options
                .get(usize::from(answer.option))
                .ok_or("Crystal answer exceeds menu")?
            {
                PreventionV1::Barricade(_) | PreventionV1::Protection(_) => {
                    expected.remaining_amount = 0
                }
                PreventionV1::Replacement(r) => {
                    if matches!(r.kind, ReplacementEffectKind::PreventNextDamage { .. }) {
                        let operations = state.engine.event_history[usize::try_from(previous.history_index).map_err(|_| "Crystal history index")? + 1..index].iter()
                            .filter(|e| matches!(e, CommittedEvent::CrystalShieldOperationV1 { operation }
                                if operation.before == *r && operation.damage_amount == expected.remaining_amount)).count();
                        if operations != 1 {
                            return Err("Crystal answer lost its independently committed shield consumption".into());
                        }
                    }
                    expected.remaining_amount = match r.kind {
                        ReplacementEffectKind::PreventNextDamage { remaining, .. } => {
                            expected.remaining_amount - remaining.min(expected.remaining_amount)
                        }
                        _ => 0,
                    };
                    expected.touched.push(r.id);
                }
            }
            expected.options.retain(
                |o| !matches!(o, PreventionV1::Replacement(r) if expected.touched.contains(&r.id)),
            );
            let mut next = old.clone();
            if expected.remaining_amount > 0 {
                next.choice = Some(menu(expected));
            } else if expected.batch.is_some() {
                let (_, next_choice) = advance_batch_choice(state, &expected)?;
                next.choice = next_choice.map(menu);
                if next.choice.is_none()
                    && !matches!(state.engine.event_history.get(index - 1),
                    Some(CommittedEvent::CrystalExecutionV1 { execution })
                        if execution.batch_commit.is_some() && execution.previous == previous.history_index
                            && execution.before.as_ref() == old && execution.after.as_deref() == Some(&next))
                {
                    return Err(
                        "Crystal final batch answer lost its independent simultaneous commit"
                            .into(),
                    );
                }
            } else {
                next.choice = None;
            }
            if b.pending.as_deref() != Some(&next) {
                return Err("Crystal answer receipt lost its exact prevention/remainder".into());
            }
        }
        if previous.is_none() {
            let mut initial = probe(&b.item);
            initial.frames.push(EffectFrame::Program {
                op: b.root.clone(),
                path: Vec::new(),
            });
            if b.item.v4.crystal_prevention_started_v1.0 != Some(b.history_index)
                || b.answer.is_some()
                || b.pending.as_deref() != Some(&initial)
                || *owned_root(state, &initial)? != b.root
            {
                return Err("Crystal initial root/owner authority changed".into());
            }
        }
        if let Some(p) = b.pending.as_deref() {
            if let Some(PendingEffectChoice::ChooseOption {
                purpose: EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
                ..
            }) = p.choice.as_ref()
            {
                if choice.batch.is_some() {
                    validate_batch_choice_record(state, p, &b.root, choice)?;
                } else {
                    let op = effect_op_at_structural_path(&b.root, &choice.path)
                        .ok_or("Crystal historical menu lost its printed instruction")?;
                    let damage = choice_original_damage(state, &p.ctx, op, choice)?;
                    let mut frames = Vec::new();
                    remaining(&b.root, &choice.path, &[], &mut frames)?;
                    if p.choice.as_ref() != Some(&menu((**choice).clone()))
                        || choice.source != damage.source
                        || choice.historical_source != damage.historical_source
                        || choice.target != damage.target
                        || choice.original_amount != damage.amount
                        || choice.remaining_amount <= 0
                        || choice.remaining_amount > damage.amount
                        || p.frames != frames
                        || choice.remaining_frames != frames
                        || p.answered_choice_guard.is_some()
                    {
                        return Err(
                            "Crystal historical menu changed its captured root/remainder".into(),
                        );
                    }
                }
            }
            if let Some(PendingEffectChoice::SelectTargets {
                purpose: EffectTargetSelectionPurpose::CrystalDiscardAfterDamageV2 { capture },
                ..
            }) = p.choice.as_ref()
            {
                validate_discard_record(state, p, &b.root, *capture)?;
            }
            if matches!(
                p.choice.as_ref(),
                Some(PendingEffectChoice::ChooseOption {
                    purpose: EffectOptionChoicePurpose::Generic,
                    ..
                })
            ) {
                validate_program_menu_record(p, &b.root)?;
            }
            if p.resolving_item != *b.item || p.ctx != context(&b.item) {
                return Err("Crystal boundary lost the complete captured context".into());
            }
        }
    }
    for b in boundaries(state) {
        if last(state, &b.item) != Some(b) {
            continue;
        }
        let live = state
            .stack
            .iter()
            .filter(|i| i.v4.stack_item_id == b.item.v4.stack_item_id)
            .collect::<Vec<_>>();
        let removed = owner.is_some_and(|o| o.expected == *b.item);
        match b.pending.as_deref() {
            Some(p) => {
                if (!removed && (live.len() != 1 || *live[0] != *b.item))
                    || (removed && !live.is_empty())
                    || state.engine.pending_effect.as_ref() != Some(p)
                {
                    return Err(
                        "Crystal boundary lost its mandatory complete live/pending owner".into(),
                    );
                }
                validate_choice(state, p)?;
            }
            None => {
                if !live.is_empty()
                    || state.engine.pending_effect.as_ref().is_some_and(|p| {
                        p.resolving_item.v4.stack_item_id == b.item.v4.stack_item_id
                    })
                {
                    return Err("closed Crystal resolution was reintroduced".into());
                }
            }
        }
    }
    for item in &state.stack {
        if item.v4.crystal_prevention_started_v1.0.is_some() && last(state, item).is_none() {
            return Err("Crystal started owner lost every receipt".into());
        }
    }
    if let Some(p) = state.engine.pending_effect.as_ref() {
        if p.resolving_item
            .v4
            .crystal_prevention_started_v1
            .0
            .is_some()
            && last(state, &p.resolving_item).is_none()
        {
            return Err("Crystal pending owner lost every receipt".into());
        }
    }
    Ok(())
}
pub(crate) fn begin_owned(
    state: &mut GameState,
    op: &EffectOp,
    ctx: &ExecCtx,
    owner: &OwnerV1,
) -> Result<ResumableProgress, String> {
    if !owner.initial
        || *op != owner.root
        || *ctx != context(&owner.original)
        || state.engine.pending_effect.is_some()
        || state
            .stack
            .iter()
            .any(|i| i.v4.stack_item_id == owner.original.v4.stack_item_id)
    {
        return Err("Crystal removed-item loan changed".into());
    }
    let mut p = probe(&owner.expected);
    p.frames.push(EffectFrame::Program {
        op: op.clone(),
        path: Vec::new(),
    });
    state.engine.pending_effect = Some(p);
    record(state, owner, None)?;
    if let Some(gorehorn) = owner.gorehorn.as_ref() {
        crate::gorehorn_damage_v1::crystal::begin_owned(state, gorehorn, &owner.expected)?;
    }
    drive_owned(state, owner)
}
pub(crate) fn resume_owned(
    state: &mut GameState,
    owner: &OwnerV1,
) -> Result<ResumableProgress, String> {
    if owner.initial {
        return Err("Crystal initial loan cannot resume".into());
    }
    drive_owned(state, owner)
}
fn drive_owned(state: &mut GameState, owner: &OwnerV1) -> Result<ResumableProgress, String> {
    validate_with_owner(state, Some(owner))?;
    let previous = last(state, &owner.expected)
        .ok_or("Crystal execution lost publication")?
        .history_index;
    let before = state
        .engine
        .pending_effect
        .take()
        .ok_or("Crystal execution lost pending owner")?;
    if before.choice.is_some() {
        state.engine.pending_effect = Some(before);
        return Err("Crystal waiting choice cannot execute before its actual answer".into());
    }
    let mut pending = before.clone();
    let mut steps = Vec::new();
    while let Some(frame) = pending.frames.pop() {
        let EffectFrame::Program { op, path } = &frame else {
            return Err("Crystal fixed interpreter received a foreign frame".into());
        };
        let mut step = ExecutionStepV1 {
            frame: frame.clone(),
            conditional_result: None,
            offered_menu: None,
            damage_history: Vec::new(),
            nondamage_history: Vec::new(),
            batch_capture: None,
            creature_power_capture: None,
            discard_capture: None,
            damage_operation: None,
        };
        match op {
            EffectOp::Sequence(ops) => {
                for (index, inner) in ops.iter().enumerate().rev() {
                    let mut child = path.clone();
                    child.push(u16::try_from(index).map_err(|_| "Crystal sequence exceeds u16")?);
                    pending.frames.push(EffectFrame::Program {
                        op: inner.clone(),
                        path: child,
                    });
                }
            }
            EffectOp::Choice { options, .. } if optional_source_fight(op) => {
                pending.choice = Some(program_menu(&pending.ctx, path, options));
                step.offered_menu = pending.choice.clone();
                steps.push(step);
                break;
            }
            EffectOp::Conditional { cond, then, else_ } => {
                let met = eval_cond(cond, &pending.ctx, state);
                step.conditional_result = Some(met);
                let mut child = path.clone();
                child.push(if met { 0 } else { 1 });
                pending.frames.push(EffectFrame::Program {
                    op: if met {
                        (**then).clone()
                    } else {
                        (**else_).clone()
                    },
                    path: child,
                });
            }
            EffectOp::DealDamage { .. }
            | EffectOp::StandardV1(
                crate::standard_cards_v1::StandardOpV1::SourceLastPowerDamageV1,
            ) => {
                if let Some(gorehorn) = owner.gorehorn.as_ref() {
                    crate::gorehorn_damage_v1::crystal::prepare_owned(
                        state,
                        gorehorn,
                        &pending.ctx,
                        op,
                    )?;
                }
                if let Some(equipment) = owner.equipment.as_ref() {
                    crate::equipment_damage_source_v4::prepare_owned(
                        state,
                        equipment,
                        &pending.ctx,
                        op,
                    )?;
                }
                let damage = original_damage(state, &pending.ctx, op)?;
                if damage.amount <= 0 {
                    steps.push(step);
                    continue;
                }
                if offer(state, &mut pending, op, path)? {
                    step.offered_menu = pending.choice.clone();
                    steps.push(step);
                    break;
                }
                let start = state.engine.event_history.len();
                step.damage_operation = Some(crate::event::propose_and_commit_crystal_owned(
                    state,
                    damage,
                    &pending.ctx,
                    op,
                    path,
                    owner,
                )?);
                if state.engine.halted.is_some() {
                    state.engine.pending_effect = Some(pending);
                    return Err("Crystal owned damage execution halted".into());
                }
                for (offset, event) in state.engine.event_history[start..].iter().enumerate() {
                    if matches!(event, CommittedEvent::Damage { .. }) {
                        step.damage_history.push(
                            u32::try_from(start + offset)
                                .map_err(|_| "Crystal damage occurrence exceeds u32")?,
                        );
                    }
                }
            }
            EffectOp::MayPayCostThen { .. } if optional_discard_draw(op) => {
                let capture = DiscardCaptureV2 {
                    ctx: pending.ctx.clone(),
                    op: op.clone(),
                    path: path.clone(),
                    hand: bindings_for(
                        state,
                        &state.players[pending.ctx.controller.index()].hand,
                        Zone::Hand,
                    )?,
                    library: bindings_for(
                        state,
                        &state.players[pending.ctx.controller.index()].library,
                        Zone::Library,
                    )?,
                };
                if !capture.hand.is_empty() {
                    let at = u32::try_from(state.engine.event_history.len())
                        .map_err(|_| "Crystal discard capture exceeds u32")?;
                    pending.choice = Some(discard_menu(&capture, at));
                    step.offered_menu = pending.choice.clone();
                    step.discard_capture = Some(capture);
                    steps.push(step);
                    break;
                }
                step.discard_capture = Some(capture);
            }
            EffectOp::CreatureTargetPowerDamage { .. } => {
                let capture = capture_creature_power(state, &pending.ctx, op, path)?;
                if let Some(damage) = capture.damage.as_ref() {
                    let occurrence = u32::try_from(state.engine.event_history.len())
                        .map_err(|_| "Crystal power capture exceeds u32")?;
                    if offer_damage(
                        state,
                        &mut pending,
                        damage.proposed(),
                        path,
                        Some(occurrence),
                    )? {
                        step.offered_menu = pending.choice.clone();
                        step.creature_power_capture = Some(capture);
                        steps.push(step);
                        break;
                    }
                    let start = state.engine.event_history.len();
                    step.damage_operation =
                        Some(crate::event::propose_and_commit_crystal_power_owned(
                            state,
                            damage.proposed(),
                            &capture,
                            owner,
                        )?);
                    if state.engine.halted.is_some() {
                        state.engine.pending_effect = Some(pending);
                        return Err("Crystal chosen-creature damage halted".into());
                    }
                    for (offset, event) in state.engine.event_history[start..].iter().enumerate() {
                        if matches!(event, CommittedEvent::Damage { .. }) {
                            step.damage_history.push(
                                u32::try_from(start + offset)
                                    .map_err(|_| "Crystal power damage history exceeds u32")?,
                            );
                        }
                    }
                }
                step.creature_power_capture = Some(capture);
            }
            op if batch_recipe(op) => {
                let capture = capture_batch(state, owner, &pending.ctx, op, path)?;
                if !capture.choice_order.is_empty() {
                    let progress = BatchProgressV2 {
                        capture: u32::try_from(state.engine.event_history.len())
                            .map_err(|_| "Crystal batch capture exceeds u32")?,
                        cursor: 0,
                        outcomes: capture.initial_outcomes.clone(),
                    };
                    pending.choice = Some(menu(batch_choice(
                        &capture,
                        progress,
                        pending.frames.clone(),
                    )?));
                    step.offered_menu = pending.choice.clone();
                    step.batch_capture = Some(capture);
                    steps.push(step);
                    break;
                }
                let start = state.engine.event_history.len();
                let survivors = batch_survivors(&capture.initial_outcomes)?;
                crate::event::commit_crystal_prepared_batch(state, survivors, &capture, owner);
                if state.engine.halted.is_some() {
                    state.engine.pending_effect = Some(pending);
                    return Err("Crystal complete batch commit halted".into());
                }
                for (offset, event) in state.engine.event_history[start..].iter().enumerate() {
                    if matches!(event, CommittedEvent::Damage { .. }) {
                        step.damage_history.push(
                            u32::try_from(start + offset)
                                .map_err(|_| "Crystal batch damage occurrence exceeds u32")?,
                        );
                    }
                }
                step.batch_capture = Some(capture);
            }
            EffectOp::GainLife {
                player: player @ (PlayerRef::Controller | PlayerRef::Opponent),
                amount,
            } => {
                let start = state.engine.event_history.len();
                crate::event::propose_and_commit_crystal_life_gain_owned(
                    state, *player, *amount, owner,
                );
                if state.engine.halted.is_some() {
                    state.engine.pending_effect = Some(pending);
                    return Err("Crystal owned following life gain halted".into());
                }
                for (offset, event) in state.engine.event_history[start..].iter().enumerate() {
                    if matches!(event, CommittedEvent::LifeGain { .. }) {
                        step.nondamage_history.push(
                            u32::try_from(start + offset)
                                .map_err(|_| "Crystal life gain occurrence exceeds u32")?,
                        );
                    }
                }
            }
            _ => return Err("Crystal fixed interpreter received an unsupported producer".into()),
        }
        steps.push(step);
    }
    let suspended = pending.choice.is_some();
    let after = suspended.then_some(Box::new(pending));
    state.engine.pending_effect = after.as_deref().cloned();
    let execution = ExecutionV1 {
        history_index: u32::try_from(state.engine.event_history.len())
            .map_err(|_| "Crystal history exceeds u32")?,
        previous,
        item: Box::new(owner.expected.clone()),
        before: Box::new(before),
        after,
        steps,
        batch_commit: None,
        discard_commit: None,
        program_commit: None,
    };
    state
        .engine
        .event_history
        .push(CommittedEvent::CrystalExecutionV1 {
            execution: Box::new(execution),
        });
    record(state, owner, None)?;
    if !suspended {
        if let Some(gorehorn) = owner.gorehorn.as_ref() {
            crate::gorehorn_damage_v1::crystal::finish_owned(state, gorehorn, &owner.expected)?;
        }
    }
    if let Some(equipment) = owner.equipment.as_ref().filter(|_| !suspended) {
        crate::equipment_damage_source_v4::finish_owned(state, equipment, &owner.expected)?;
    }
    Ok(if suspended {
        ResumableProgress::Suspended
    } else {
        ResumableProgress::Complete(Box::new(owner.expected.clone()))
    })
}
fn validate_damage_operation_ref(
    state: &GameState,
    step: &ExecutionStepV1,
    ctx: &ExecCtx,
    op: &EffectOp,
    path: &[u16],
    expected: &DamageProposed,
    interval: (usize, usize),
) -> Result<usize, String> {
    let (previous, end) = interval;
    let at = step
        .damage_operation
        .ok_or("Crystal positive leaf lost independent prepare/commit outcome")?
        as usize;
    let Some(CommittedEvent::CrystalDamageOperationV3 { operation }) =
        state.engine.event_history.get(at)
    else {
        return Err("Crystal damage operation reference is missing or foreign".into());
    };
    let input = BatchDamageV2 {
        source: expected.source,
        target: expected.target,
        amount: expected.amount,
        touched: expected.touched_by.clone(),
    };
    if at <= previous
        || at >= end
        || operation.history_index as usize != at
        || operation.ctx != *ctx
        || operation.op != *op
        || operation.path != path
        || operation.input != input
        || operation.historical_source != expected.historical_source
        || step.damage_history != operation.damage_history.into_iter().collect::<Vec<_>>()
    {
        return Err("Crystal damage completion changed actual operation/context/occurrence".into());
    }
    Ok(at)
}
fn validate_damage_operations(state: &GameState) -> Result<(), String> {
    for (at, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::CrystalDamageOperationV3 { operation } = event else {
            continue;
        };
        if operation.history_index as usize != at
            || operation.input.amount <= 0
            || !operation.input.touched.is_empty()
        {
            return Err("Crystal independent damage input/header changed".into());
        }
        let mut owners = state
            .engine
            .event_history
            .iter()
            .filter_map(|e| match e {
                CommittedEvent::CrystalExecutionV1 { execution } => Some(execution.as_ref()),
                _ => None,
            })
            .flat_map(|e| {
                e.steps
                    .iter()
                    .filter(move |s| s.damage_operation == Some(operation.history_index))
                    .map(move |s| (e, s))
            });
        let (execution, step) = owners
            .next()
            .ok_or("Crystal damage outcome lost executing owner")?;
        if owners.next().is_some()
            || execution.history_index as usize <= at
            || execution.item.v4.stack_item_id != operation.ctx.stack_item_id.unwrap_or_default()
            || operation.ctx != context(&execution.item)
            || step.frame
                != (EffectFrame::Program {
                    op: operation.op.clone(),
                    path: operation.path.clone(),
                })
            || step.offered_menu.is_some()
        {
            return Err("Crystal damage outcome repeated or changed exact executing owner".into());
        }
        match (&operation.output,operation.damage_history) {
            (Some(output),Some(index)) if output.source==operation.input.source && output.target==operation.input.target
                && output.amount>0 && output.amount<=operation.input.amount && (index as usize)<at
                && (index as usize)>execution.previous as usize
                && matches!(state.engine.event_history.get(index as usize),Some(CommittedEvent::Damage {source,target,amount,..})
                    if *source==output.source && *target==output.target && *amount==output.amount)=>{},
            (None,None) if operation.preventers.iter().any(|p| matches!(p,
                PreventionV1::Barricade(_)|PreventionV1::Protection(_))
                || matches!(p,PreventionV1::Replacement(r)
                    if matches!(r.kind,ReplacementEffectKind::PreventDamageFromColorUntilEndOfTurn {..})))
                || operation.preventers.iter().filter_map(|p| match p {
                    PreventionV1::Replacement(r)=>match r.kind {
                        ReplacementEffectKind::PreventNextDamage {target,remaining}
                            if target==operation.input.target=>Some(remaining.max(0)),
                        _=>None,
                    },_=>None,
                }).fold(0_i32,i32::saturating_add)>=operation.input.amount=>{},
            _=>return Err("Crystal damage outcome lost actual positive or fully-prevented result".into()),
        }
    }
    Ok(())
}

fn validate_executions(state: &GameState) -> Result<(), String> {
    for (index, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::CrystalExecutionV1 { execution } = event else {
            continue;
        };
        if execution.program_commit.is_some() {
            validate_program_commit(state, index, execution)?;
            continue;
        }
        if execution.discard_commit.is_some() {
            validate_discard_commit(state, index, execution)?;
            continue;
        }
        if execution.batch_commit.is_some() {
            validate_batch_commit_execution(state, index, execution)?;
            continue;
        }
        let previous_index =
            usize::try_from(execution.previous).map_err(|_| "Crystal previous index")?;
        let Some(CommittedEvent::CrystalPreventionBoundaryV1 { boundary: previous }) =
            state.engine.event_history.get(previous_index)
        else {
            return Err("Crystal execution lost original boundary".into());
        };
        let Some(CommittedEvent::CrystalPreventionBoundaryV1 { boundary: next }) =
            state.engine.event_history.get(index + 1)
        else {
            return Err("Crystal execution lost its immediate resulting boundary".into());
        };
        if usize::try_from(execution.history_index).ok() != Some(index)
            || previous_index >= index
            || previous.item != execution.item
            || previous.pending.as_deref() != Some(execution.before.as_ref())
            || execution.before.choice.is_some()
            || next.previous != Some(execution.previous)
            || next.item != execution.item
            || next.root != previous.root
            || next.answer.is_some()
            || next.pending != execution.after
        {
            return Err("Crystal execution changed its actual owner or remainder".into());
        }
        let mut pending = (*execution.before).clone();
        let mut last_damage = previous_index;
        for (step_index, step) in execution.steps.iter().enumerate() {
            if pending.choice.is_some() || pending.frames.pop().as_ref() != Some(&step.frame) {
                return Err(
                    "Crystal execution skipped or reordered its exact remaining frames".into(),
                );
            }
            let EffectFrame::Program { op, path } = &step.frame else {
                return Err("Crystal execution committed a foreign frame".into());
            };
            if step.damage_operation.is_some()
                && (!matches!(
                    op,
                    EffectOp::DealDamage { .. }
                        | EffectOp::CreatureTargetPowerDamage { .. }
                        | EffectOp::StandardV1(
                            crate::standard_cards_v1::StandardOpV1::SourceLastPowerDamageV1
                        )
                ) || step.offered_menu.is_some())
            {
                return Err("Crystal operation appears on a foreign leaf or pending menu".into());
            }
            if !optional_discard_draw(op) && step.discard_capture.is_some() {
                return Err("Crystal unrelated instruction carries discard capture".into());
            }
            if !matches!(op, EffectOp::CreatureTargetPowerDamage { .. })
                && step.creature_power_capture.is_some()
            {
                return Err(
                    "Crystal other instruction carries foreign creature-power capture".into(),
                );
            }
            if !batch_recipe(op) && step.batch_capture.is_some() {
                return Err("Crystal nonbatch instruction carries foreign batch authority".into());
            }
            if !matches!(op, EffectOp::GainLife { .. }) && !step.nondamage_history.is_empty() {
                return Err(
                    "Crystal damage/structural instruction carries foreign life gain".into(),
                );
            }
            match op {
                EffectOp::Choice { options, .. } if optional_source_fight(op) => {
                    if step.conditional_result.is_some()
                        || !step.damage_history.is_empty()
                        || step_index + 1 != execution.steps.len()
                        || step.offered_menu.as_ref()
                            != Some(&program_menu(&pending.ctx, path, options))
                    {
                        return Err("Crystal optional fight lost its genuine initial menu".into());
                    }
                    pending.choice = step.offered_menu.clone();
                }
                EffectOp::Sequence(ops) => {
                    if step.conditional_result.is_some()
                        || step.offered_menu.is_some()
                        || !step.damage_history.is_empty()
                    {
                        return Err("Crystal sequence carries foreign execution facts".into());
                    }
                    for (offset, inner) in ops.iter().enumerate().rev() {
                        let mut child = path.clone();
                        child.push(
                            u16::try_from(offset).map_err(|_| "Crystal sequence exceeds u16")?,
                        );
                        pending.frames.push(EffectFrame::Program {
                            op: inner.clone(),
                            path: child,
                        });
                    }
                }
                EffectOp::Conditional { then, else_, .. } => {
                    let met = step
                        .conditional_result
                        .ok_or("Crystal conditional lost actual branch")?;
                    if step.offered_menu.is_some() || !step.damage_history.is_empty() {
                        return Err("Crystal conditional carries foreign execution facts".into());
                    }
                    let mut child = path.clone();
                    child.push(if met { 0 } else { 1 });
                    pending.frames.push(EffectFrame::Program {
                        op: if met {
                            (**then).clone()
                        } else {
                            (**else_).clone()
                        },
                        path: child,
                    });
                }
                EffectOp::DealDamage { .. }
                | EffectOp::StandardV1(
                    crate::standard_cards_v1::StandardOpV1::SourceLastPowerDamageV1,
                ) => {
                    let damage = original_damage(state, &pending.ctx, op)?;
                    if step.conditional_result.is_some() {
                        return Err("Crystal damage carries a foreign branch".into());
                    }
                    if let Some(menu) = step.offered_menu.as_ref() {
                        if step_index + 1 != execution.steps.len()
                            || !step.damage_history.is_empty()
                        {
                            return Err(
                                "Crystal offered choice executed another instruction".into()
                            );
                        }
                        let PendingEffectChoice::ChooseOption {
                            purpose: EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
                            ..
                        } = menu
                        else {
                            return Err("Crystal execution offered a foreign menu".into());
                        };
                        if choice.path != *path
                            || choice.source != damage.source
                            || choice.historical_source != damage.historical_source
                            || choice.target != damage.target
                            || choice.original_amount != damage.amount
                            || choice.remaining_amount != damage.amount
                            || !choice.touched.is_empty()
                            || choice.remaining_frames != pending.frames
                        {
                            return Err(
                                "Crystal execution changed offered damage or exact remainder"
                                    .into(),
                            );
                        }
                        pending.choice = Some(menu.clone());
                    } else {
                        let before_damage = last_damage;
                        if step.damage_history.len() > 1 {
                            return Err(
                                "Crystal fixed damage committed more than one damage event".into(),
                            );
                        }
                        for saved in &step.damage_history {
                            let at = usize::try_from(*saved).map_err(|_| "Crystal damage index")?;
                            if at <= last_damage || at >= index {
                                return Err("Crystal damage occurrence moved or replayed".into());
                            }
                            if !matches!(state.engine.event_history.get(at),
                                Some(CommittedEvent::Damage { source, target: recipient, amount: dealt, .. })
                                if *source == damage.source
                                    && *recipient == damage.target
                                    && *dealt > 0 && *dealt <= damage.amount)
                            {
                                return Err(
                                    "Crystal execution lost genuine committed damage".into()
                                );
                            }
                            last_damage = at;
                        }
                        if damage.amount > 0 {
                            last_damage = validate_damage_operation_ref(
                                state,
                                step,
                                &pending.ctx,
                                op,
                                path,
                                &damage,
                                (before_damage, index),
                            )?;
                        } else if step.damage_operation.is_some() {
                            return Err(
                                "Crystal zero damage carries an unexpected positive operation"
                                    .into(),
                            );
                        }
                    }
                }
                EffectOp::MayPayCostThen { .. } if optional_discard_draw(op) => {
                    let capture = step
                        .discard_capture
                        .as_ref()
                        .ok_or("Crystal optional discard lost actual capture")?;
                    validate_discard_capture(capture, &pending.ctx, op, path)?;
                    if step.conditional_result.is_some()
                        || !step.damage_history.is_empty()
                        || !step.nondamage_history.is_empty()
                    {
                        return Err(
                            "Crystal discard offer carries foreign committed effects".into()
                        );
                    }
                    if let Some(offered) = step.offered_menu.as_ref() {
                        if capture.hand.is_empty()
                            || step_index + 1 != execution.steps.len()
                            || *offered != discard_menu(capture, execution.history_index)
                        {
                            return Err("Crystal discard offer changed exact private menu".into());
                        }
                        pending.choice = Some(offered.clone());
                    } else if !capture.hand.is_empty() {
                        return Err("Crystal optional discard erased a payable prompt".into());
                    }
                }
                EffectOp::CreatureTargetPowerDamage { .. } => {
                    let capture = step
                        .creature_power_capture
                        .as_ref()
                        .ok_or("Crystal power lost actual capture")?;
                    validate_creature_power_capture(
                        state,
                        capture,
                        &pending.ctx,
                        op,
                        path,
                        last_damage,
                        index,
                    )?;
                    if let Some(at) = capture.counter_history.last() {
                        last_damage = *at as usize;
                    }
                    if step.conditional_result.is_some() {
                        return Err("Crystal creature power has foreign branch".into());
                    }
                    if let Some(offered) = step.offered_menu.as_ref() {
                        let damage = capture
                            .damage
                            .as_ref()
                            .ok_or("Crystal power menu without damage")?;
                        if step_index + 1 != execution.steps.len()
                            || !step.damage_history.is_empty()
                        {
                            return Err(
                                "Crystal creature-power menu skipped exact suspended instruction"
                                    .into(),
                            );
                        }
                        let PendingEffectChoice::ChooseOption {
                            purpose: EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
                            ..
                        } = offered
                        else {
                            return Err("Crystal power offered foreign menu".into());
                        };
                        if choice.creature_power != Some(execution.history_index)
                            || choice.batch.is_some()
                            || choice.path != *path
                            || choice.source != damage.source
                            || choice.target != damage.target
                            || choice.original_amount != damage.amount
                            || choice.remaining_amount != damage.amount
                            || choice.historical_source.is_some()
                            || !choice.touched.is_empty()
                            || choice.remaining_frames != pending.frames
                        {
                            return Err(
                                "Crystal power offered changed source/amount/remainder".into()
                            );
                        }
                        pending.choice = Some(offered.clone());
                    } else {
                        let before_damage = last_damage;
                        if step.damage_history.len() > usize::from(capture.damage.is_some()) {
                            return Err("Crystal power repeated genuine damage".into());
                        }
                        for saved in &step.damage_history {
                            let at = *saved as usize;
                            let damage = capture
                                .damage
                                .as_ref()
                                .ok_or("Crystal illegal power nevertheless dealt damage")?;
                            if at <= last_damage
                                || at >= index
                                || !matches!(state.engine.event_history.get(at),
                                Some(CommittedEvent::Damage { source, target, amount, .. })
                                    if *source == damage.source && *target == damage.target
                                        && *amount > 0 && *amount <= damage.amount)
                            {
                                return Err("Crystal power lost genuine damage occurrence".into());
                            }
                            last_damage = at;
                        }
                        if let Some(damage) = capture.damage.as_ref() {
                            last_damage = validate_damage_operation_ref(
                                state,
                                step,
                                &pending.ctx,
                                op,
                                path,
                                &damage.proposed(),
                                (before_damage, index),
                            )?;
                        } else if step.damage_operation.is_some() {
                            return Err(
                                "Crystal illegal or zero-power source carries a damage operation"
                                    .into(),
                            );
                        }
                    }
                }
                op if batch_recipe(op) => {
                    let capture = step
                        .batch_capture
                        .as_ref()
                        .ok_or("Crystal simultaneous damage lost its capture")?;
                    validate_batch_capture(capture, &pending.ctx, op, path)?;
                    validate_batch_preparation_ref(state, capture, previous_index, index)?;
                    if step.conditional_result.is_some() {
                        return Err("Crystal batch carries foreign branch".into());
                    }
                    if let Some(offered) = step.offered_menu.as_ref() {
                        if capture.choice_order.is_empty()
                            || step_index + 1 != execution.steps.len()
                            || !step.damage_history.is_empty()
                        {
                            return Err(
                                "Crystal batch committed damage before all affected choices".into(),
                            );
                        }
                        let progress = BatchProgressV2 {
                            capture: execution.history_index,
                            cursor: 0,
                            outcomes: capture.initial_outcomes.clone(),
                        };
                        let expected =
                            menu(batch_choice(capture, progress, pending.frames.clone())?);
                        if *offered != expected {
                            return Err("Crystal batch first menu lost exact captured scene".into());
                        }
                        pending.choice = Some(expected);
                    } else {
                        if !capture.choice_order.is_empty() {
                            return Err("Crystal batch erased affected choices".into());
                        }
                        validate_batch_damage_occurrences(
                            state,
                            &capture.initial_outcomes,
                            &step.damage_history,
                            last_damage,
                            index,
                        )?;
                        if let Some(at) = step.damage_history.last() {
                            last_damage = *at as usize;
                        }
                    }
                }
                EffectOp::GainLife {
                    player: player @ (PlayerRef::Controller | PlayerRef::Opponent),
                    amount,
                } => {
                    if step.conditional_result.is_some()
                        || step.offered_menu.is_some()
                        || !step.damage_history.is_empty()
                        || step.nondamage_history.len() != usize::from(*amount > 0)
                    {
                        return Err("Crystal life gain changed its actual operation count".into());
                    }
                    let beneficiary = match player {
                        PlayerRef::Controller => pending.ctx.controller,
                        PlayerRef::Opponent => pending.ctx.controller.opponent(),
                        _ => unreachable!("matched direct player reference"),
                    };
                    for saved in &step.nondamage_history {
                        let at = usize::try_from(*saved).map_err(|_| "Crystal life gain index")?;
                        if at <= last_damage
                            || at >= index
                            || !matches!(state.engine.event_history.get(at),
                                Some(CommittedEvent::LifeGain { player, amount: gained })
                                    if *player == beneficiary && gained == amount)
                        {
                            return Err(
                                "Crystal following life gain lost genuine committed occurrence"
                                    .into(),
                            );
                        }
                        last_damage = at;
                    }
                }
                _ => return Err("Crystal execution contains an unsupported producer".into()),
            }
        }
        let after = if pending.choice.is_some() {
            Some(Box::new(pending))
        } else {
            if !pending.frames.is_empty() {
                return Err("Crystal completion skipped remaining printed instructions".into());
            }
            None
        };
        if after != execution.after {
            return Err("Crystal execution changed completion or suspended remainder".into());
        }
    }
    Ok(())
}

fn discard_capture_at(state: &GameState, at: u32) -> Result<&DiscardCaptureV2, String> {
    let Some(CommittedEvent::CrystalExecutionV1 { execution }) =
        state.engine.event_history.get(at as usize)
    else {
        return Err("Crystal discard lost independent execution capture".into());
    };
    let step = execution
        .steps
        .last()
        .ok_or("Crystal discard lost offered printed cost leaf")?;
    let capture = step
        .discard_capture
        .as_ref()
        .ok_or("Crystal discard lost printed cost leaf")?;
    if step.offered_menu.as_ref() != Some(&discard_menu(capture, at)) {
        return Err("Crystal discard capture is not the offered final operation".into());
    }
    Ok(capture)
}
fn bindings_for(
    state: &GameState,
    objects: &[ObjectId],
    zone: Zone,
) -> Result<Vec<EffectObjectBinding>, String> {
    objects
        .iter()
        .map(|id| {
            let live = state
                .objects
                .try_get(*id)
                .ok_or("Crystal private card absent")?;
            if live.zone != zone {
                return Err("Crystal private roster has wrong zone".into());
            }
            Ok(EffectObjectBinding {
                object: *id,
                expected_zone: zone,
                expected_zone_change_count: live.zone_change_count,
            })
        })
        .collect()
}
fn discard_menu(capture: &DiscardCaptureV2, at: u32) -> PendingEffectChoice {
    PendingEffectChoice::SelectTargets {
        player: capture.ctx.controller,
        path: capture.path.clone(),
        selected: Vec::new(),
        legal: capture
            .hand
            .iter()
            .map(|b| EffectTargetCandidate {
                target: Target::Object(b.object),
                expected_object: Some(*b),
            })
            .collect(),
        min_targets: 0,
        max_targets: 1,
        ordered: false,
        purpose: EffectTargetSelectionPurpose::CrystalDiscardAfterDamageV2 { capture: at },
    }
}
pub(crate) fn is_discard_choice(state: &GameState) -> bool {
    matches!(
        state
            .engine
            .pending_effect
            .as_ref()
            .and_then(|p| p.choice.as_ref()),
        Some(PendingEffectChoice::SelectTargets {
            purpose: EffectTargetSelectionPurpose::CrystalDiscardAfterDamageV2 { .. },
            ..
        })
    )
}
fn validate_discard_capture(
    capture: &DiscardCaptureV2,
    ctx: &ExecCtx,
    op: &EffectOp,
    path: &[u16],
) -> Result<(), String> {
    if capture.ctx != *ctx
        || capture.op != *op
        || capture.path != path
        || !optional_discard_draw(op)
    {
        return Err("Crystal discard changed definition-owned cost/draw root".into());
    }
    let mut all = std::collections::BTreeSet::new();
    for (cards, zone) in [
        (&capture.hand, Zone::Hand),
        (&capture.library, Zone::Library),
    ] {
        for card in cards {
            if card.expected_zone != zone || !all.insert(card.object) {
                return Err("Crystal discard repeated private incarnation or changed zone".into());
            }
        }
    }
    Ok(())
}
fn validate_discard_record(
    state: &GameState,
    pending: &EffectContinuation,
    root: &EffectOp,
    at: u32,
) -> Result<(), String> {
    let capture = discard_capture_at(state, at)?;
    let op = effect_op_at_structural_path(root, &capture.path)
        .ok_or("Crystal discard lost structural path")?;
    validate_discard_capture(capture, &pending.ctx, op, &capture.path)?;
    let Some(CommittedEvent::CrystalExecutionV1 { execution }) =
        state.engine.event_history.get(at as usize)
    else {
        unreachable!()
    };
    let mut frames = Vec::new();
    remaining(root, &capture.path, &[], &mut frames)?;
    if execution.item.as_ref() != &pending.resolving_item
        || capture.hand.is_empty()
        || pending.choice.as_ref() != Some(&discard_menu(capture, at))
        || pending.frames != frames
        || pending.answered_choice_guard.is_some()
    {
        return Err("Crystal discard changed captured hand/menu/remainder".into());
    }
    Ok(())
}
pub(crate) fn validate_discard_choice(
    state: &GameState,
    pending: &EffectContinuation,
) -> Result<(), String> {
    let Some(PendingEffectChoice::SelectTargets {
        purpose: EffectTargetSelectionPurpose::CrystalDiscardAfterDamageV2 { capture: at },
        ..
    }) = pending.choice.as_ref()
    else {
        return Err("not a Crystal discard menu".into());
    };
    let root = owned_root(state, pending)?;
    validate_discard_record(state, pending, &root, *at)?;
    let capture = discard_capture_at(state, *at)?;
    let player = capture.ctx.controller;
    if bindings_for(state, &state.players[player.index()].hand, Zone::Hand)? != capture.hand
        || bindings_for(state, &state.players[player.index()].library, Zone::Library)?
            != capture.library
        || capture
            .hand
            .iter()
            .any(|b| state.objects.get(b.object).owner != player)
        || capture
            .library
            .iter()
            .any(|b| state.objects.get(b.object).owner != player)
    {
        return Err("Crystal waiting private hand or library changed".into());
    }
    Ok(())
}
fn validate_discard_commit(
    state: &GameState,
    index: usize,
    execution: &ExecutionV1,
) -> Result<(), String> {
    let commit = execution
        .discard_commit
        .as_ref()
        .ok_or("Crystal discard operation absent")?;
    let previous_index = execution.previous as usize;
    let Some(CommittedEvent::CrystalPreventionBoundaryV1 { boundary: previous }) =
        state.engine.event_history.get(previous_index)
    else {
        return Err("Crystal discard answer lost prior owner".into());
    };
    let Some(CommittedEvent::CrystalPreventionBoundaryV1 { boundary: next }) =
        state.engine.event_history.get(index + 1)
    else {
        return Err("Crystal discard operation lost immediate boundary".into());
    };
    let old = previous
        .pending
        .as_deref()
        .ok_or("Crystal discard followed closure")?;
    validate_discard_record(state, old, &previous.root, commit.capture)?;
    let capture = discard_capture_at(state, commit.capture)?;
    let mut after = old.clone();
    after.choice = None;
    if execution.history_index as usize != index
        || previous_index >= index
        || previous.item != execution.item
        || next.item != execution.item
        || next.previous != Some(execution.previous)
        || next.root != previous.root
        || next.answer.is_some()
        || execution.before.as_ref() != old
        || execution.after.as_deref() != Some(&after)
        || next.pending != execution.after
        || !execution.steps.is_empty()
        || execution.batch_commit.is_some()
    {
        return Err("Crystal discard operation skipped owning remainder".into());
    }
    let interval = &state.engine.event_history[previous_index + 1..index];
    let cost_occurrences = interval
        .iter()
        .enumerate()
        .filter_map(|(offset, event)| {
            matches!(event, CommittedEvent::ZoneChange { object, from: Zone::Hand,
            to: Zone::Graveyard | Zone::Exile, .. }
            if capture.hand.iter().any(|b| b.object == *object))
            .then_some(previous_index + 1 + offset)
        })
        .collect::<Vec<_>>();
    let draw_occurrences = interval.iter().enumerate().filter_map(|(offset, event)| {
        matches!(event, CommittedEvent::Draw { player, .. } if *player == capture.ctx.controller)
            .then_some(previous_index + 1 + offset)
    }).collect::<Vec<_>>();
    if let Some(selected) = commit.selected {
        if !capture.hand.contains(&selected) {
            return Err("Crystal discard selected foreign incarnation".into());
        }
        let discard = commit
            .discard_history
            .ok_or("Crystal discard lost actual cost occurrence")? as usize;
        let draw = commit
            .draw_history
            .ok_or("Crystal discard lost actual draw occurrence")? as usize;
        if cost_occurrences != vec![discard]
            || draw_occurrences != vec![draw]
            || discard <= previous_index
            || draw <= discard
            || draw >= index
            || !matches!(state.engine.event_history.get(discard),
                Some(CommittedEvent::ZoneChange { object, from: Zone::Hand,
                    to: Zone::Graveyard | Zone::Exile, .. }) if *object == selected.object)
            || !matches!(state.engine.event_history.get(draw),
                Some(CommittedEvent::Draw { player, object }) if *player == capture.ctx.controller
                    && *object == capture.library.first().map(|b| b.object))
        {
            return Err("Crystal discard/draw lost genuine ordered cost and beneficiary".into());
        }
    } else if commit.discard_history.is_some()
        || commit.draw_history.is_some()
        || !cost_occurrences.is_empty()
        || !draw_occurrences.is_empty()
    {
        return Err("Crystal declined discard nevertheless paid or drew".into());
    }
    Ok(())
}
pub(crate) fn answer_discard(state: &mut GameState, target: Option<Target>) -> Result<(), String> {
    validate(state)?;
    let pending = state
        .engine
        .pending_effect
        .as_ref()
        .ok_or("Crystal discard absent")?;
    validate_discard_choice(state, pending)?;
    let Some(PendingEffectChoice::SelectTargets {
        purpose: EffectTargetSelectionPurpose::CrystalDiscardAfterDamageV2 { capture: at },
        ..
    }) = pending.choice.as_ref()
    else {
        unreachable!()
    };
    let at = *at;
    let capture = discard_capture_at(state, at)?.clone();
    let selected = if let Some(target) = target {
        Some(
            *capture
                .hand
                .iter()
                .find(|b| target == Target::Object(b.object))
                .ok_or("Crystal discard card is not own current hand")?,
        )
    } else {
        None
    };
    let before = pending.clone();
    let boundary = last(state, &before.resolving_item).ok_or("Crystal discard lost publication")?;
    let previous = boundary.history_index;
    let owner = OwnerV1 {
        original: before.resolving_item.clone(),
        expected: before.resolving_item.clone(),
        root: boundary.root.clone(),
        initial: false,
        gorehorn: None,
        equipment: None,
    };
    let mut commit = DiscardCommitV2 {
        capture: at,
        selected,
        discard_history: None,
        draw_history: None,
    };
    if let Some(selected) = selected {
        let start = state.engine.event_history.len();
        crate::engine::commit_discarded_card(state, selected.object);
        if state.engine.halted.is_some() {
            return Err("Crystal accepted discard halted".into());
        }
        let mut occurrences = state.engine.event_history[start..].iter().enumerate().filter_map(|(offset,e)|
            matches!(e,CommittedEvent::ZoneChange { object,from: Zone::Hand,to: Zone::Graveyard | Zone::Exile,.. }
                if *object == selected.object).then_some(start+offset));
        let discarded = occurrences
            .next()
            .ok_or("Crystal accepted discard lost actual departure")?;
        if occurrences.next().is_some() {
            return Err("Crystal discard repeated departure".into());
        }
        commit.discard_history =
            Some(u32::try_from(discarded).map_err(|_| "Crystal discard history exceeds u32")?);
        let draw_start = state.engine.event_history.len();
        crate::event::propose_and_commit_crystal_discard_draw_owned(
            state, &capture, selected, &owner,
        );
        if state.engine.halted.is_some() {
            return Err("Crystal following draw halted".into());
        }
        let mut draws = state.engine.event_history[draw_start..]
            .iter()
            .enumerate()
            .filter_map(|(offset, e)| {
                matches!(e,CommittedEvent::Draw { player,.. } if *player == capture.ctx.controller)
                    .then_some(draw_start + offset)
            });
        let drawn = draws
            .next()
            .ok_or("Crystal accepted discard did not attempt draw")?;
        if draws.next().is_some() {
            return Err("Crystal accepted discard repeated draw".into());
        }
        commit.draw_history =
            Some(u32::try_from(drawn).map_err(|_| "Crystal draw history exceeds u32")?);
    }
    let mut after = before.clone();
    after.choice = None;
    state.engine.pending_effect = Some(after.clone());
    let execution = ExecutionV1 {
        history_index: u32::try_from(state.engine.event_history.len())
            .map_err(|_| "Crystal history exceeds u32")?,
        previous,
        item: Box::new(owner.expected.clone()),
        before: Box::new(before),
        after: Some(Box::new(after)),
        steps: Vec::new(),
        batch_commit: None,
        discard_commit: Some(commit),
        program_commit: None,
    };
    state
        .engine
        .event_history
        .push(CommittedEvent::CrystalExecutionV1 {
            execution: Box::new(execution),
        });
    record(state, &owner, None)?;
    Ok(())
}

fn creature_power_at<'a>(
    state: &'a GameState,
    index: u32,
    path: &[u16],
) -> Result<&'a CreaturePowerCaptureV2, String> {
    let Some(CommittedEvent::CrystalExecutionV1 { execution }) =
        state.engine.event_history.get(index as usize)
    else {
        return Err("Crystal creature power lost actual execution capture".into());
    };
    let mut captures = execution
        .steps
        .iter()
        .filter_map(|s| s.creature_power_capture.as_ref())
        .filter(|capture| capture.path == path);
    let capture = captures
        .next()
        .ok_or("Crystal creature power lost printed leaf")?;
    if captures.next().is_some() {
        return Err("Crystal creature power repeated leaf".into());
    }
    Ok(capture)
}
pub(crate) fn creature_power_signed(
    state: &GameState,
    choice: &ChoiceV1,
) -> Result<Option<i32>, String> {
    choice
        .creature_power
        .map(|at| creature_power_at(state, at, &choice.path).map(|c| c.signed_power))
        .transpose()
}
fn choice_original_damage(
    state: &GameState,
    ctx: &ExecCtx,
    op: &EffectOp,
    choice: &ChoiceV1,
) -> Result<DamageProposed, String> {
    if let Some(at) = choice.creature_power {
        let capture = creature_power_at(state, at, &choice.path)?;
        if capture.ctx != *ctx || capture.op != *op {
            return Err("Crystal chosen power borrowed foreign printed instruction".into());
        }
        let Some(CommittedEvent::CrystalExecutionV1 { execution }) =
            state.engine.event_history.get(at as usize)
        else {
            unreachable!()
        };
        if context(&execution.item) != *ctx {
            return Err("Crystal chosen power borrowed foreign stack incarnation".into());
        }
        return capture
            .damage
            .as_ref()
            .map(BatchDamageV2::proposed)
            .ok_or_else(|| "Crystal creature-power menu has no genuine damage".into());
    }
    original_damage(state, ctx, op)
}
fn capture_creature_power(
    state: &mut GameState,
    ctx: &ExecCtx,
    op: &EffectOp,
    path: &[u16],
) -> Result<CreaturePowerCaptureV2, String> {
    let EffectOp::CreatureTargetPowerDamage {
        source_index,
        target_index,
        plus1_plus1,
        target_spec,
    } = op
    else {
        return Err("Crystal chosen power has foreign leaf".into());
    };
    if *plus1_plus1 < 0 {
        return Err("Crystal chosen power has negative counter".into());
    }
    let legal = |slot: u8| {
        ctx.target_incarnation_matches(usize::from(slot), state)
            && crate::engine::effect_target_is_legal(
                state,
                ctx.source,
                ctx.controller,
                *target_spec,
                &ctx.targets,
                usize::from(slot),
            )
    };
    let source_legal = legal(*source_index);
    let recipient_legal = legal(*target_index);
    let mut capture = CreaturePowerCaptureV2 {
        ctx: ctx.clone(),
        op: op.clone(),
        path: path.to_vec(),
        source_legal,
        recipient_legal,
        source_before: None,
        source_after: None,
        signed_power: 0,
        counter_history: Vec::new(),
        damage: None,
    };
    if !source_legal {
        return Ok(capture);
    }
    let Some(Target::Object(source)) = ctx.targets.get(usize::from(*source_index)).copied() else {
        return Err("Crystal creature-power source is not original object target".into());
    };
    capture.source_before = Some(state.objects.get(source).clone());
    if recipient_legal {
        let target = *ctx
            .targets
            .get(usize::from(*target_index))
            .ok_or("Crystal power recipient absent")?;
        let probe = crate::event::ProposedEvent::damage(source, target, i32::MAX);
        let crate::event::ProposedEvent::Damage(probe) = probe else {
            unreachable!()
        };
        u16::try_from(applicable(state, &probe).len())
            .map_err(|_| "Crystal power prevention menu exceeds u16")?;
    }
    let start = state.engine.event_history.len();
    if *plus1_plus1 > 0 {
        // The same checked production primitive commits counter observers.
        crate::event::add_plus_one_counters(
            state,
            source,
            ctx.controller,
            i32::from(*plus1_plus1),
        )?;
    }
    capture.counter_history = state.engine.event_history[start..]
        .iter()
        .enumerate()
        .filter_map(|(offset, e)| {
            matches!(e, CommittedEvent::PlusOneCountersAdded { .. }).then_some(start + offset)
        })
        .map(|at| u32::try_from(at).map_err(|_| "Crystal counter index exceeds u32"))
        .collect::<Result<Vec<_>, _>>()?;
    capture.source_after = Some(state.objects.get(source).clone());
    capture.signed_power = crate::engine::effective_power(state, source);
    if recipient_legal && capture.signed_power > 0 {
        capture.damage = Some(BatchDamageV2 {
            source,
            target: *ctx
                .targets
                .get(usize::from(*target_index))
                .ok_or("Crystal power recipient absent")?,
            amount: capture.signed_power,
            touched: Vec::new(),
        });
    }
    Ok(capture)
}
fn validate_creature_power_capture(
    state: &GameState,
    capture: &CreaturePowerCaptureV2,
    ctx: &ExecCtx,
    op: &EffectOp,
    path: &[u16],
    previous: usize,
    until: usize,
) -> Result<(), String> {
    let EffectOp::CreatureTargetPowerDamage {
        source_index,
        target_index,
        plus1_plus1,
        ..
    } = op
    else {
        return Err("Crystal creature power carries foreign recipe".into());
    };
    if capture.ctx != *ctx || capture.op != *op || capture.path != path || *plus1_plus1 < 0 {
        return Err("Crystal creature power changed exact original context/root".into());
    }
    if !capture.source_legal {
        if capture.source_before.is_some()
            || capture.source_after.is_some()
            || capture.signed_power != 0
            || capture.damage.is_some()
            || !capture.counter_history.is_empty()
        {
            return Err("Crystal illegal source nevertheless executed creature power".into());
        }
        return Ok(());
    }
    let Some(Target::Object(source)) = ctx.targets.get(usize::from(*source_index)) else {
        return Err("Crystal power source lost target".into());
    };
    let before = capture
        .source_before
        .as_ref()
        .ok_or("Crystal power lost source before counters")?;
    let after = capture
        .source_after
        .as_ref()
        .ok_or("Crystal power lost source after counters")?;
    let contract = ctx
        .target_contracts
        .get(usize::from(*source_index))
        .ok_or("Crystal power lost announced contract")?;
    if !matches!(contract, crate::state::StackTargetContractV4::Object { object, owner, zone, zone_change_count, .. }
        if object == source && *owner == before.owner && *zone == before.zone
            && *zone_change_count == before.zone_change_count)
        || before.zone != Zone::Battlefield
        || before.controller != ctx.controller
    {
        return Err("Crystal power snapshot lost original legal source incarnation".into());
    }
    let mut expected_after = before.clone();
    expected_after.counters.plus1_plus1 = expected_after
        .counters
        .plus1_plus1
        .checked_add(i32::from(*plus1_plus1))
        .ok_or("Crystal counter overflow")?;
    if *after != expected_after || capture.counter_history.len() != usize::from(*plus1_plus1 > 0) {
        return Err("Crystal power sampled before its precise counter placement".into());
    }
    for at in &capture.counter_history {
        let at = *at as usize;
        if at <= previous
            || at >= until
            || !matches!(state.engine.event_history.get(at),
            Some(CommittedEvent::PlusOneCountersAdded { object, zone_change_count, player, count })
                if object == source && *zone_change_count == before.zone_change_count
                    && *player == ctx.controller && *count == i32::from(*plus1_plus1))
        {
            return Err("Crystal creature power lost genuine counter occurrence".into());
        }
    }
    let expected_damage = if capture.recipient_legal && capture.signed_power > 0 {
        Some(BatchDamageV2 {
            source: *source,
            target: *ctx
                .targets
                .get(usize::from(*target_index))
                .ok_or("Crystal creature power lost recipient")?,
            amount: capture.signed_power,
            touched: Vec::new(),
        })
    } else {
        None
    };
    if capture.damage != expected_damage {
        return Err("Crystal power changed damage source/recipient/signed amount".into());
    }
    Ok(())
}
fn validate_live_creature_power(
    state: &GameState,
    capture: &CreaturePowerCaptureV2,
) -> Result<(), String> {
    let Some(damage) = capture.damage.as_ref() else {
        return Err("Crystal power menu lost damage".into());
    };
    if state.objects.try_get(damage.source) != capture.source_after.as_ref()
        || crate::engine::effective_power(state, damage.source) != capture.signed_power
    {
        return Err("Crystal waiting chosen-creature source changed after power capture".into());
    }
    Ok(())
}

fn batch_capture_at<'a>(
    state: &'a GameState,
    index: u32,
    path: &[u16],
) -> Result<&'a BatchCaptureV2, String> {
    let Some(CommittedEvent::CrystalExecutionV1 { execution }) =
        state.engine.event_history.get(index as usize)
    else {
        return Err("Crystal batch lost independent owning execution".into());
    };
    let mut captures = execution
        .steps
        .iter()
        .filter_map(|step| step.batch_capture.as_ref())
        .filter(|capture| capture.path == path);
    let capture = captures.next().ok_or("Crystal batch lost captured leaf")?;
    if captures.next().is_some() {
        return Err("Crystal batch repeated captured leaf".into());
    }
    Ok(capture)
}
fn batch_choice(
    capture: &BatchCaptureV2,
    progress: BatchProgressV2,
    frames: Vec<EffectFrame>,
) -> Result<ChoiceV1, String> {
    let at = *capture
        .choice_order
        .get(progress.cursor)
        .ok_or("Crystal batch cursor exceeds choices")?;
    let damage = capture
        .proposals
        .get(at)
        .ok_or("Crystal batch choice lost proposal")?;
    let Target::Object(id) = damage.target else {
        return Err("Crystal creature batch lost recipient".into());
    };
    let recipient = capture
        .roster
        .iter()
        .find(|r| r.object.object == id)
        .ok_or("Crystal batch choice lost recipient capture")?;
    let options = capture
        .options
        .get(at)
        .and_then(|o| o.clone())
        .ok_or("Crystal batch choice lost original options")?;
    u16::try_from(options.len()).map_err(|_| "Crystal batch menu exceeds u16")?;
    Ok(ChoiceV1 {
        source: damage.source,
        historical_source: None,
        batch: Some(progress),
        creature_power: None,
        target: damage.target,
        original_amount: damage.amount,
        remaining_amount: damage.amount,
        touched: Vec::new(),
        chooser: recipient.controller,
        path: capture.path.clone(),
        options,
        remaining_frames: frames,
    })
}
fn resolve_fighter(
    state: &GameState,
    ctx: &ExecCtx,
    op: &EffectOp,
    slot: usize,
) -> Option<ObjectId> {
    let refs = fight_refs(op).ok()?;
    let id = match *refs.get(slot)? {
        ObjectRef::ThisSource => {
            let contract = ctx.ability_source_contract?;
            let live = state.objects.try_get(contract.source)?;
            if contract.source != ctx.source
                || contract.zone != Zone::Battlefield
                || live.zone != contract.zone
                || live.owner != contract.owner
                || live.zone_change_count != contract.zone_change_count
            {
                return None;
            }
            ctx.source
        }
        ObjectRef::Target(index) => {
            let slot = usize::from(index);
            let EffectOp::FightObjects { target_spec, .. } = op else {
                return None;
            };
            if !ctx.target_incarnation_matches(slot, state)
                || !crate::engine::effect_target_is_legal(
                    state,
                    ctx.source,
                    ctx.controller,
                    *target_spec,
                    &ctx.targets,
                    slot,
                )
            {
                return None;
            }
            let Target::Object(id) = *ctx.targets.get(slot)? else {
                return None;
            };
            id
        }
    };
    let live = state.objects.try_get(id)?;
    (live.zone == Zone::Battlefield
        && crate::engine::object_has_type(state, id, crate::card_def::CardType::Creature))
    .then_some(id)
}
fn fight_proposals(
    ctx: &ExecCtx,
    op: &EffectOp,
    fight: &FightCaptureV3,
) -> Result<Vec<BatchDamageV2>, String> {
    let refs = fight_refs(op)?;
    let mut ids = [ctx.source; 2];
    for (slot, snapshot) in fight.creatures.iter().enumerate() {
        ids[slot] = match refs[slot] {
            ObjectRef::ThisSource => ctx.source,
            ObjectRef::Target(index) => {
                let Some(Target::Object(id)) = ctx.targets.get(usize::from(index)) else {
                    return Err("Crystal fight lost ordered creature targets".into());
                };
                *id
            }
        };
        let Some(snapshot) = snapshot.as_ref() else {
            continue;
        };
        let bound = match refs[slot] {
            ObjectRef::ThisSource => ctx.ability_source_contract.is_some_and(|source| {
                source.source == ids[slot]
                    && source.zone == snapshot.zone
                    && source.owner == snapshot.owner
                    && source.zone_change_count == snapshot.zone_change_count
            }),
            ObjectRef::Target(index) => {
                matches!(ctx.target_contracts.get(usize::from(index)),
                Some(crate::state::StackTargetContractV4::Object { object, owner, zone, zone_change_count, .. })
                if *object == ids[slot] && *owner == snapshot.owner && *zone == snapshot.zone
                    && *zone_change_count == snapshot.zone_change_count)
                    && snapshot.controller
                        == if slot == 0 {
                            ctx.controller
                        } else {
                            ctx.controller.opponent()
                        }
            }
        };
        if snapshot.zone != Zone::Battlefield || !bound {
            return Err("Crystal fight changed original legal source incarnation".into());
        }
    }
    if fight.creatures.iter().any(Option::is_none) {
        if fight.signed_powers != [0, 0] {
            return Err("Crystal incomplete fight nevertheless captured power".into());
        }
        return Ok(Vec::new());
    }
    if ids[0] == ids[1] {
        if !source_fight_recipe(op)
            || fight.creatures[0] != fight.creatures[1]
            || fight.signed_powers[0] != fight.signed_powers[1]
        {
            return Err("Crystal self-fight changed its single physical source".into());
        }
        let amount = fight.signed_powers[0]
            .max(0)
            .checked_mul(2)
            .ok_or("Crystal self-fight damage overflow")?;
        return Ok(if amount > 0 {
            vec![BatchDamageV2 {
                source: ids[0],
                target: Target::Object(ids[0]),
                amount,
                touched: Vec::new(),
            }]
        } else {
            Vec::new()
        });
    }
    Ok((0..2)
        .filter(|slot| fight.signed_powers[*slot] > 0)
        .map(|slot| BatchDamageV2 {
            source: ids[slot],
            target: Target::Object(ids[1 - slot]),
            amount: fight.signed_powers[slot],
            touched: Vec::new(),
        })
        .collect())
}
fn capture_batch(
    state: &mut GameState,
    owner: &OwnerV1,
    ctx: &ExecCtx,
    op: &EffectOp,
    path: &[u16],
) -> Result<BatchCaptureV2, String> {
    if !batch_recipe(op) {
        return Err("Crystal batch is not a supported printed recipe".into());
    }
    let fight = if fight_recipe(op) {
        let ids = [
            resolve_fighter(state, ctx, op, 0),
            resolve_fighter(state, ctx, op, 1),
        ];
        let creatures = ids.map(|id| id.map(|id| state.objects.get(id).clone()));
        let signed_powers = if ids.iter().all(Option::is_some) {
            ids.map(|id| crate::engine::effective_power(state, id.unwrap()))
        } else {
            [0, 0]
        };
        Some(FightCaptureV3 {
            creatures,
            signed_powers,
        })
    } else {
        None
    };
    let proposals = fight
        .as_ref()
        .map(|f| fight_proposals(ctx, op, f))
        .transpose()?;
    let roster = [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(|p| state.players[p.index()].battlefield.iter().copied())
        .map(|id| {
            let object = state.objects.get(id);
            BatchRecipientV2 {
                object: EffectObjectBinding {
                    object: id,
                    expected_zone: Zone::Battlefield,
                    expected_zone_change_count: object.zone_change_count,
                },
                owner: object.owner,
                controller: object.controller,
                card_def: object.card_def,
                selected: match op {
                    EffectOp::DamageAllCreatures { filter, .. } => {
                        creature_matches_filter(state, id, filter, ctx.controller)
                    }
                    _ => proposals
                        .as_ref()
                        .unwrap()
                        .iter()
                        .any(|p| p.target == Target::Object(id)),
                },
            }
        })
        .collect::<Vec<_>>();
    let mut unique = std::collections::BTreeSet::new();
    if roster.iter().any(|r| !unique.insert(r.object.object)) {
        return Err("Crystal prebatch battlefield has duplicate physical recipients".into());
    }
    let proposals = proposals.unwrap_or_else(|| {
        let EffectOp::DamageAllCreatures { amount, .. } = op else {
            unreachable!()
        };
        roster
            .iter()
            .filter(|r| r.selected)
            .map(|r| BatchDamageV2 {
                source: ctx.source,
                target: Target::Object(r.object.object),
                amount: *amount,
                touched: Vec::new(),
            })
            .collect()
    });
    // All menus and both fight powers are fixed before any shield is consumed.
    // Each current shield has one fixed recipient, so these distinct recipients
    // cannot consume one another's shield or require cross-packet allocation.
    let options = proposals
        .iter()
        .map(|p| {
            let d = p.proposed();
            needs_choice(state, &d).then(|| applicable(state, &d))
        })
        .collect::<Vec<_>>();
    for menu in options.iter().flatten() {
        u16::try_from(menu.len()).map_err(|_| "Crystal batch menu exceeds u16")?;
    }
    let mut choice_order = options
        .iter()
        .enumerate()
        .filter_map(|(i, o)| o.as_ref().map(|_| i))
        .collect::<Vec<_>>();
    choice_order.sort_by_key(|at| {
        let Target::Object(id) = proposals[*at].target else {
            unreachable!()
        };
        usize::from(state.objects.get(id).controller != state.active_player)
    });
    let mut capture = BatchCaptureV2 {
        ctx: ctx.clone(),
        op: op.clone(),
        path: path.to_vec(),
        roster,
        proposals,
        options,
        initial_outcomes: Vec::new(),
        choice_order,
        active_player: state.active_player,
        fight,
        preparation: None,
    };
    let events = capture
        .proposals
        .iter()
        .map(|p| crate::event::ProposedEvent::Damage(p.proposed()))
        .collect::<Vec<_>>();
    if !owner.permits_batch(state, &capture, &events) {
        return Err("Crystal batch lost exact pre-pop source loan".into());
    }
    for (proposal, options) in capture.proposals.iter().zip(&capture.options) {
        if options.is_some() {
            capture.initial_outcomes.push(BatchOutcomeV2::Waiting);
        } else if proposal.amount <= 0 {
            capture.initial_outcomes.push(BatchOutcomeV2::Prevented);
        } else {
            let outcome = crate::event::apply_replacements(
                state,
                crate::event::ProposedEvent::Damage(proposal.proposed()),
            );
            if state.engine.halted.is_some() {
                return Err("Crystal batch replacement preparation halted".into());
            }
            capture.initial_outcomes.push(match outcome {
                Some(crate::event::ProposedEvent::Damage(d)) => {
                    BatchOutcomeV2::Damage(BatchDamageV2 {
                        source: d.source,
                        target: d.target,
                        amount: d.amount,
                        touched: d.touched_by,
                    })
                }
                None => BatchOutcomeV2::Prevented,
                _ => return Err("Crystal batch changed proposal kind".into()),
            });
        }
    }
    let at = u32::try_from(state.engine.event_history.len())
        .map_err(|_| "Crystal batch preparation exceeds u32")?;
    capture.preparation = Some(at);
    state
        .engine
        .event_history
        .push(CommittedEvent::CrystalBatchPreparedV3 {
            capture: Box::new(capture.clone()),
        });
    Ok(capture)
}
fn validate_batch_preparations(state: &GameState) -> Result<(), String> {
    for (at, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::CrystalBatchPreparedV3 { capture } = event else {
            continue;
        };
        if capture.preparation.map(|p| p as usize) != Some(at) {
            return Err("Crystal batch preparation lost exact occurrence".into());
        }
        let mut owners = state
            .engine
            .event_history
            .iter()
            .filter_map(|event| match event {
                CommittedEvent::CrystalExecutionV1 { execution } => Some(execution.as_ref()),
                _ => None,
            })
            .flat_map(|execution| {
                execution.steps.iter().filter_map(move |step| {
                    step.batch_capture
                        .as_ref()
                        .filter(|c| c.preparation == capture.preparation)
                        .map(|c| (execution, c))
                })
            });
        let (execution, saved) = owners
            .next()
            .ok_or("Crystal actual batch preparation lost executing owner")?;
        if owners.next().is_some()
            || saved != capture.as_ref()
            || at <= execution.previous as usize
            || at >= execution.history_index as usize
            || capture.ctx != execution.before.ctx
        {
            return Err("Crystal batch preparation changed its unique original execution".into());
        }
        validate_batch_capture(capture, &execution.before.ctx, &capture.op, &capture.path)?;
    }
    Ok(())
}
fn validate_batch_preparation_ref(
    state: &GameState,
    capture: &BatchCaptureV2,
    previous: usize,
    end: usize,
) -> Result<(), String> {
    let at = capture
        .preparation
        .ok_or("Crystal batch lost actual preparation")? as usize;
    if at <= previous
        || at >= end
        || !matches!(state.engine.event_history.get(at),
        Some(CommittedEvent::CrystalBatchPreparedV3 { capture: original })
            if original.as_ref() == capture)
    {
        return Err("Crystal batch completion skipped or changed actual preparation".into());
    }
    Ok(())
}
fn validate_batch_capture(
    capture: &BatchCaptureV2,
    ctx: &ExecCtx,
    op: &EffectOp,
    path: &[u16],
) -> Result<(), String> {
    if !batch_recipe(op)
        || capture.ctx != *ctx
        || capture.op != *op
        || capture.path != path
        || capture.proposals.len() != capture.options.len()
        || capture.proposals.len() != capture.initial_outcomes.len()
        || capture
            .roster
            .iter()
            .any(|r| r.object.expected_zone != Zone::Battlefield)
    {
        return Err("Crystal batch changed captured recipe/context".into());
    }
    let mut ids = std::collections::BTreeSet::new();
    if capture.roster.iter().any(|r| !ids.insert(r.object.object)) {
        return Err("Crystal batch repeated battlefield recipient".into());
    }
    let expected = match op {
        EffectOp::DamageAllCreatures { amount, .. } => {
            if capture.fight.is_some() {
                return Err("Crystal area damage carries foreign fight".into());
            }
            capture
                .roster
                .iter()
                .filter(|r| r.selected)
                .map(|r| BatchDamageV2 {
                    source: ctx.source,
                    target: Target::Object(r.object.object),
                    amount: *amount,
                    touched: Vec::new(),
                })
                .collect::<Vec<_>>()
        }
        _ => {
            let fight = capture
                .fight
                .as_ref()
                .ok_or("Crystal fight lost both source captures")?;
            let proposals = fight_proposals(ctx, op, fight)?;
            if capture.roster.iter().any(|r| {
                r.selected
                    != proposals
                        .iter()
                        .any(|p| p.target == Target::Object(r.object.object))
            }) {
                return Err("Crystal fight changed actual selected recipients".into());
            }
            for (slot, creature) in fight.creatures.iter().enumerate() {
                if let Some(creature) = creature {
                    let id = match fight_refs(op)?[slot] {
                        ObjectRef::ThisSource => ctx.source,
                        ObjectRef::Target(index) => match ctx.targets.get(usize::from(index)) {
                            Some(Target::Object(id)) => *id,
                            _ => return Err("Crystal fight snapshot lost target".into()),
                        },
                    };
                    if capture
                        .roster
                        .iter()
                        .find(|r| r.object.object == id)
                        .is_none_or(|r| {
                            r.owner != creature.owner
                                || r.controller != creature.controller
                                || r.card_def != creature.card_def
                                || r.object.expected_zone_change_count != creature.zone_change_count
                        })
                    {
                        return Err("Crystal fight lost prebatch physical source".into());
                    }
                }
            }
            proposals
        }
    };
    if expected != capture.proposals {
        return Err("Crystal batch changed complete ordered proposals".into());
    }
    let mut choices = capture
        .options
        .iter()
        .enumerate()
        .filter_map(|(i, o)| o.as_ref().map(|_| i))
        .collect::<Vec<_>>();
    if capture.proposals.iter().any(|p| {
        let Target::Object(id) = p.target else {
            return true;
        };
        !capture.roster.iter().any(|r| r.object.object == id)
    }) {
        return Err("Crystal batch proposal lost captured recipient".into());
    }
    choices.sort_by_key(|at| {
        let Target::Object(id) = capture.proposals[*at].target else {
            unreachable!()
        };
        usize::from(
            capture
                .roster
                .iter()
                .find(|r| r.object.object == id)
                .unwrap()
                .controller
                != capture.active_player,
        )
    });
    if choices != capture.choice_order {
        return Err("Crystal batch changed APNAP choice order".into());
    }
    for ((proposal, options), outcome) in capture
        .proposals
        .iter()
        .zip(&capture.options)
        .zip(&capture.initial_outcomes)
    {
        match (options, outcome) {
            (Some(options), BatchOutcomeV2::Waiting)
                if !options.is_empty()
                    && options
                        .iter()
                        .any(|o| matches!(o, PreventionV1::Barricade(_)))
                    && options.iter().any(|o| {
                        matches!(o, PreventionV1::Replacement(r)
                    if matches!(r.kind, ReplacementEffectKind::PreventNextDamage { .. }))
                    }) => {}
            (None, BatchOutcomeV2::Prevented) => {}
            (None, BatchOutcomeV2::Damage(d))
                if d.source == proposal.source
                    && d.target == proposal.target
                    && d.amount > 0
                    && d.amount <= proposal.amount => {}
            _ => return Err("Crystal batch changed genuine initial replacement outcomes".into()),
        }
    }
    Ok(())
}
fn validate_live_batch(state: &GameState, capture: &BatchCaptureV2) -> Result<(), String> {
    let ids = [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(|p| state.players[p.index()].battlefield.iter().copied())
        .collect::<Vec<_>>();
    if state.active_player != capture.active_player
        || ids
            != capture
                .roster
                .iter()
                .map(|r| r.object.object)
                .collect::<Vec<_>>()
        || capture.roster.iter().any(|r| {
            state.objects.try_get(r.object.object).is_none_or(|live| {
                live.zone != r.object.expected_zone
                    || live.zone_change_count != r.object.expected_zone_change_count
                    || live.owner != r.owner
                    || live.controller != r.controller
                    || live.card_def != r.card_def
                    || match &capture.op {
                        EffectOp::DamageAllCreatures { filter, .. } => {
                            creature_matches_filter(
                                state,
                                r.object.object,
                                filter,
                                capture.ctx.controller,
                            ) != r.selected
                        }
                        _ => false,
                    }
            })
        })
    {
        return Err("Crystal waiting batch changed prebatch battlefield".into());
    }
    if let Some(fight) = capture.fight.as_ref() {
        for slot in 0..2 {
            let id = resolve_fighter(state, &capture.ctx, &capture.op, slot);
            if id.and_then(|id| state.objects.try_get(id)) != fight.creatures[slot].as_ref()
                || id.map(|id| crate::engine::effective_power(state, id))
                    != fight.creatures[slot]
                        .as_ref()
                        .map(|_| fight.signed_powers[slot])
            {
                return Err(
                    "Crystal waiting fight changed source incarnation/control/power".into(),
                );
            }
        }
    }
    Ok(())
}
fn advance_batch_choice(
    state: &GameState,
    choice: &ChoiceV1,
) -> Result<(BatchProgressV2, Option<ChoiceV1>), String> {
    let mut progress = choice
        .batch
        .clone()
        .ok_or("Crystal batch progress absent")?;
    let capture = batch_capture_at(state, progress.capture, &choice.path)?;
    let at = *capture
        .choice_order
        .get(progress.cursor)
        .ok_or("Crystal batch cursor changed")?;
    if progress.outcomes.len() != capture.proposals.len()
        || progress.outcomes.get(at) != Some(&BatchOutcomeV2::Waiting)
    {
        return Err("Crystal batch repeated a completed recipient".into());
    }
    progress.outcomes[at] = BatchOutcomeV2::Prevented;
    progress.cursor = progress
        .cursor
        .checked_add(1)
        .ok_or("Crystal batch cursor overflow")?;
    let next = if progress.cursor < capture.choice_order.len() {
        Some(batch_choice(
            capture,
            progress.clone(),
            choice.remaining_frames.clone(),
        )?)
    } else {
        None
    };
    Ok((progress, next))
}
fn batch_survivors(
    outcomes: &[BatchOutcomeV2],
) -> Result<Vec<crate::event::ProposedEvent>, String> {
    outcomes
        .iter()
        .filter_map(|o| match o {
            BatchOutcomeV2::Waiting => Some(Err(
                "Crystal batch committed before all affected choices".into(),
            )),
            BatchOutcomeV2::Prevented => None,
            BatchOutcomeV2::Damage(d) => {
                Some(Ok(crate::event::ProposedEvent::Damage(d.proposed())))
            }
        })
        .collect()
}
fn validate_batch_choice_record(
    state: &GameState,
    pending: &EffectContinuation,
    root: &EffectOp,
    choice: &ChoiceV1,
) -> Result<(), String> {
    let progress = choice
        .batch
        .as_ref()
        .ok_or("Crystal batch choice lost progress")?;
    let capture = batch_capture_at(state, progress.capture, &choice.path)?;
    let op = effect_op_at_structural_path(root, &choice.path)
        .ok_or("Crystal batch lost printed leaf")?;
    validate_batch_capture(capture, &pending.ctx, op, &choice.path)?;
    let Some(CommittedEvent::CrystalExecutionV1 { execution }) =
        state.engine.event_history.get(progress.capture as usize)
    else {
        unreachable!()
    };
    if execution.item.as_ref() != &pending.resolving_item {
        return Err("Crystal batch borrowed another stack incarnation".into());
    }
    let mut expected_outcomes = capture.initial_outcomes.clone();
    if progress.cursor >= capture.choice_order.len() {
        return Err("Crystal batch cursor reached closure inside menu".into());
    }
    for at in capture.choice_order.iter().take(progress.cursor) {
        expected_outcomes[*at] = BatchOutcomeV2::Prevented;
    }
    if progress.outcomes != expected_outcomes {
        return Err("Crystal batch erased or rewrote prior recipient outcomes".into());
    }
    let original = batch_choice(capture, progress.clone(), pending.frames.clone())?;
    let mut frames = Vec::new();
    remaining(root, &choice.path, &[], &mut frames)?;
    let mut expected_options = original.options.clone();
    let mut touched = std::collections::BTreeSet::new();
    for id in &choice.touched {
        if !touched.insert(*id)
            || !original
                .options
                .iter()
                .any(|o| matches!(o, PreventionV1::Replacement(r) if r.id == *id))
        {
            return Err("Crystal batch changed applied replacement identities".into());
        }
    }
    expected_options
        .retain(|o| !matches!(o, PreventionV1::Replacement(r) if touched.contains(&r.id)));
    if choice.source != original.source
        || choice.target != original.target
        || choice.historical_source.is_some()
        || choice.creature_power.is_some()
        || choice.original_amount != original.original_amount
        || choice.remaining_amount <= 0
        || choice.remaining_amount > choice.original_amount
        || choice.chooser != original.chooser
        || choice.options != expected_options
        || choice.remaining_frames != frames
        || pending.frames != frames
        || pending.answered_choice_guard.is_some()
        || pending.choice.as_ref() != Some(&menu(choice.clone()))
    {
        return Err("Crystal batch changed its captured menu or printed remainder".into());
    }
    Ok(())
}
fn validate_batch_damage_occurrences(
    state: &GameState,
    outcomes: &[BatchOutcomeV2],
    saved: &[u32],
    previous: usize,
    until: usize,
) -> Result<(), String> {
    let expected = outcomes
        .iter()
        .filter_map(|o| match o {
            BatchOutcomeV2::Damage(d) => Some(d),
            _ => None,
        })
        .collect::<Vec<_>>();
    if expected.len() != saved.len()
        || outcomes
            .iter()
            .any(|o| matches!(o, BatchOutcomeV2::Waiting))
    {
        return Err("Crystal batch changed simultaneous survivor count".into());
    }
    let mut last = previous;
    for (damage, index) in expected.into_iter().zip(saved) {
        let at = usize::try_from(*index).map_err(|_| "Crystal batch damage index")?;
        if at <= last
            || at >= until
            || !matches!(state.engine.event_history.get(at),
            Some(CommittedEvent::Damage { source, target, amount, .. })
                if *source == damage.source && *target == damage.target && *amount == damage.amount)
        {
            return Err("Crystal batch lost exact ordered simultaneous damage occurrences".into());
        }
        last = at;
    }
    Ok(())
}
fn validate_batch_commit_execution(
    state: &GameState,
    index: usize,
    execution: &ExecutionV1,
) -> Result<(), String> {
    if execution.discard_commit.is_some() {
        return Err("Crystal simultaneous commit carries foreign discard operation".into());
    }
    let commit = execution
        .batch_commit
        .as_ref()
        .ok_or("Crystal batch commit absent")?;
    let previous_index = execution.previous as usize;
    let Some(CommittedEvent::CrystalPreventionBoundaryV1 { boundary: previous }) =
        state.engine.event_history.get(previous_index)
    else {
        return Err("Crystal batch commit lost prior boundary".into());
    };
    let Some(CommittedEvent::CrystalPreventionBoundaryV1 { boundary: next }) =
        state.engine.event_history.get(index + 1)
    else {
        return Err("Crystal batch commit lost immediate answered boundary".into());
    };
    let answer = next
        .answer
        .as_ref()
        .ok_or("Crystal batch completion is not an actual answer")?;
    let old = previous
        .pending
        .as_deref()
        .ok_or("Crystal batch commit follows closure")?;
    if execution.history_index as usize != index
        || previous_index >= index
        || previous.item != execution.item
        || next.item != execution.item
        || next.root != previous.root
        || next.previous != Some(execution.previous)
        || execution.before.as_ref() != old
        || next.pending != execution.after
        || !execution.steps.is_empty()
        || old.choice.as_ref() != Some(&menu(answer.before.clone()))
    {
        return Err("Crystal batch commit changed complete owning continuation".into());
    }
    validate_batch_choice_record(state, old, &previous.root, &answer.before)?;
    let (progress, next_choice) = advance_batch_choice(state, &answer.before)?;
    if next_choice.is_some()
        || progress.capture != commit.capture
        || progress.outcomes != commit.outcomes
    {
        return Err("Crystal batch committed before its final affected choice".into());
    }
    let mut expected = old.clone();
    expected.choice = None;
    if execution.after.as_deref() != Some(&expected) {
        return Err("Crystal batch commit skipped printed remainder".into());
    }
    validate_batch_damage_occurrences(
        state,
        &commit.outcomes,
        &commit.damage_history,
        previous_index,
        index,
    )
}
fn cannot_prevent(state: &GameState) -> bool {
    state.engine.until_end_of_turn.iter().any(|e| {
        matches!(
            e,
            crate::engine::UntilEndOfTurnEffect::DamageCannotBePrevented { .. }
        )
    })
}
pub(crate) fn applicable(state: &GameState, damage: &DamageProposed) -> Vec<PreventionV1> {
    if cannot_prevent(state) || damage.amount <= 0 || damage.is_combat {
        return Vec::new();
    }
    let Target::Object(target) = damage.target else {
        return Vec::new();
    };
    let mut options = crate::crystal_barricade_v1::sources_protecting(state, target)
        .into_iter()
        .map(PreventionV1::Barricade)
        .collect::<Vec<_>>();
    let protected = match damage.historical_source {
        Some(id) => crate::source_power_activation_v1::damage_source(state, damage.source, id)
            .is_ok_and(|facts| crate::source_power_activation_v1::protected(state, facts, target)),
        None => {
            crate::engine::damage_is_prevented_by_protection(state, damage.source, damage.target)
        }
    };
    if protected {
        let o = state.objects.get(target);
        options.push(PreventionV1::Protection(EffectObjectBinding {
            object: target,
            expected_zone: o.zone,
            expected_zone_change_count: o.zone_change_count,
        }));
    }
    options.extend(
        state
            .engine
            .active_replacements
            .iter()
            .filter(|r| {
                !damage.touched_by.contains(&r.id)
                    && crate::event::replacement_applies(
                        r,
                        &crate::event::ProposedEvent::Damage(damage.clone()),
                        state,
                    )
            })
            .cloned()
            .map(PreventionV1::Replacement),
    );
    options
}
pub(crate) fn needs_choice(state: &GameState, damage: &DamageProposed) -> bool {
    let options = applicable(state, damage);
    options
        .iter()
        .any(|o| matches!(o, PreventionV1::Barricade(_)))
        && options.iter().any(|o| {
            matches!(o, PreventionV1::Replacement(r)
            if matches!(r.kind, ReplacementEffectKind::PreventNextDamage { .. }))
        })
        && options.len() > 1
}
fn menu(choice: ChoiceV1) -> PendingEffectChoice {
    PendingEffectChoice::ChooseOption {
        player: choice.chooser,
        path: choice.path.clone(),
        options: vec![EffectOp::Sequence(Vec::new()); choice.options.len()],
        purpose: EffectOptionChoicePurpose::CrystalPreventionV1 {
            choice: Box::new(choice),
        },
    }
}
pub(crate) fn offer(
    state: &GameState,
    continuation: &mut EffectContinuation,
    op: &EffectOp,
    path: &[u16],
) -> Result<bool, String> {
    let damage = original_damage(state, &continuation.ctx, op)?;
    offer_damage(state, continuation, damage, path, None)
}
fn offer_damage(
    state: &GameState,
    continuation: &mut EffectContinuation,
    damage: DamageProposed,
    path: &[u16],
    creature_power: Option<u32>,
) -> Result<bool, String> {
    if !needs_choice(state, &damage) {
        return Ok(false);
    }
    if continuation
        .resolving_item
        .v4
        .crystal_prevention_started_v1
        .0
        .is_none()
    {
        return Err("mixed Crystal damage lacks a pre-pop owner".into());
    }
    let Target::Object(target) = damage.target else {
        return Err("Crystal recipient is not a permanent".into());
    };
    let choice = ChoiceV1 {
        source: damage.source,
        historical_source: damage.historical_source,
        batch: None,
        creature_power,
        target: damage.target,
        original_amount: damage.amount,
        remaining_amount: damage.amount,
        touched: Vec::new(),
        chooser: state.objects.get(target).controller,
        path: path.to_vec(),
        options: applicable(state, &damage),
        remaining_frames: continuation.frames.clone(),
    };
    u16::try_from(choice.options.len())
        .map_err(|_| "Crystal menu exceeds u16 decision transport")?;
    continuation.choice = Some(menu(choice));
    Ok(true)
}
fn remaining(
    root: &EffectOp,
    path: &[u16],
    prefix: &[u16],
    frames: &mut Vec<EffectFrame>,
) -> Result<(), String> {
    let Some((&head, tail)) = path.split_first() else {
        return Ok(());
    };
    let child = match root {
        EffectOp::Sequence(ops) => {
            let index = usize::from(head);
            let child = ops
                .get(index)
                .ok_or("Crystal instruction path exceeds sequence")?;
            for (offset, op) in ops.iter().enumerate().skip(index + 1).rev() {
                let mut p = prefix.to_vec();
                p.push(u16::try_from(offset).map_err(|_| "Crystal sequence exceeds u16")?);
                frames.push(EffectFrame::Program {
                    op: op.clone(),
                    path: p,
                });
            }
            child
        }
        EffectOp::Conditional { then, else_, .. } => match head {
            0 => then.as_ref(),
            1 => else_.as_ref(),
            _ => return Err("Crystal conditional path changed".into()),
        },
        EffectOp::Choice { options, .. } => options
            .get(usize::from(head))
            .ok_or("Crystal optional program path changed")?,
        _ => return Err("Crystal path traverses an unsupported instruction".into()),
    };
    let mut next = prefix.to_vec();
    next.push(head);
    remaining(child, tail, &next, frames)
}
fn validate_choice(state: &GameState, pending: &EffectContinuation) -> Result<(), String> {
    if pending
        .resolving_item
        .v4
        .crystal_prevention_started_v1
        .0
        .is_some()
        && matches!(
            pending.choice.as_ref(),
            Some(PendingEffectChoice::ChooseOption {
                purpose: EffectOptionChoicePurpose::Generic,
                ..
            })
        )
    {
        let root = owned_root(state, pending)?;
        return validate_program_menu_record(pending, &root);
    }
    if matches!(
        pending.choice.as_ref(),
        Some(PendingEffectChoice::SelectTargets {
            purpose: EffectTargetSelectionPurpose::CrystalDiscardAfterDamageV2 { .. },
            ..
        })
    ) {
        return validate_discard_choice(state, pending);
    }
    let Some(PendingEffectChoice::ChooseOption {
        purpose: EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
        ..
    }) = pending.choice.as_ref()
    else {
        return Ok(());
    };
    let root = owned_root(state, pending)?;
    if let Some(progress) = choice.batch.as_ref() {
        validate_batch_choice_record(state, pending, &root, choice)?;
        let capture = batch_capture_at(state, progress.capture, &choice.path)?;
        validate_live_batch(state, capture)?;
        if choice.options != applicable(state, &choice.damage()) {
            return Err("Crystal simultaneous menu changed applicable prevention".into());
        }
        return Ok(());
    }
    let op = effect_op_at_structural_path(&root, &choice.path)
        .ok_or("Crystal choice lost its actual damage instruction")?;
    let damage = choice_original_damage(state, &pending.ctx, op, choice)?;
    if let Some(index) = choice.creature_power {
        validate_live_creature_power(state, creature_power_at(state, index, &choice.path)?)?;
    }
    let mut expected_frames = Vec::new();
    remaining(&root, &choice.path, &[], &mut expected_frames)?;
    let Target::Object(recipient) = choice.target else {
        return Err("Crystal choice lost its affected permanent".into());
    };
    if pending.choice.as_ref() != Some(&menu((**choice).clone()))
        || choice.source != damage.source
        || choice.historical_source != damage.historical_source
        || choice.target != damage.target
        || choice.original_amount != damage.amount
        || choice.remaining_amount <= 0
        || choice.remaining_amount > damage.amount
        || pending.frames != choice.remaining_frames
        || pending.frames != expected_frames
        || pending.answered_choice_guard.is_some()
        || state.objects.try_get(recipient).is_none_or(|o| {
            o.zone != crate::state::Zone::Battlefield || o.controller != choice.chooser
        })
        || choice.options != applicable(state, &choice.damage())
        || !choice
            .options
            .iter()
            .any(|o| matches!(o, PreventionV1::Barricade(_)))
    {
        return Err("Crystal affected-player menu or captured instruction changed".into());
    }
    Ok(())
}
pub(crate) fn is_choice(state: &GameState) -> bool {
    matches!(
        state
            .engine
            .pending_effect
            .as_ref()
            .and_then(|p| p.choice.as_ref()),
        Some(PendingEffectChoice::ChooseOption {
            purpose: EffectOptionChoicePurpose::CrystalPreventionV1 { .. },
            ..
        })
    )
}
pub(crate) fn answer(state: &mut GameState, index: u16) -> Result<(), String> {
    validate(state)?;
    let pending = state
        .engine
        .pending_effect
        .as_ref()
        .ok_or("Crystal choice absent")?;
    validate_choice(state, pending)?;
    let Some(PendingEffectChoice::ChooseOption {
        purpose: EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
        ..
    }) = pending.choice.as_ref()
    else {
        return Err("not a Crystal choice".into());
    };
    let before = pending.clone();
    let mut choice = (**choice).clone();
    let selected = choice
        .options
        .get(usize::from(index))
        .cloned()
        .ok_or("Crystal option outside menu")?;
    let item = pending.resolving_item.clone();
    let root = last(state, &item)
        .ok_or("Crystal choice lost initial authority")?
        .root
        .clone();
    let owner = OwnerV1 {
        original: item.clone(),
        expected: item,
        root,
        initial: false,
        gorehorn: None,
        equipment: None,
    };
    let answer = AnswerV1 {
        before: choice.clone(),
        option: index,
    };
    match selected {
        PreventionV1::Barricade(_) | PreventionV1::Protection(_) => choice.remaining_amount = 0,
        PreventionV1::Replacement(replacement) => {
            let mut damage = choice.damage();
            damage.touched_by.push(replacement.id);
            let after = crate::event::replacement_apply(
                &replacement,
                crate::event::ProposedEvent::Damage(damage),
                state,
            );
            choice.touched.push(replacement.id);
            choice.remaining_amount = match after {
                Some(crate::event::ProposedEvent::Damage(d)) => d.amount,
                None => 0,
                _ => return Err("Crystal replacement changed event kind".into()),
            };
        }
    }
    let next = if choice.remaining_amount > 0 {
        choice.options = applicable(state, &choice.damage());
        if choice.options.is_empty() {
            return Err("Crystal static prevention disappeared during resolution".into());
        }
        Some(menu(choice))
    } else if choice.batch.is_some() {
        let (progress, next_choice) = advance_batch_choice(state, &choice)?;
        if next_choice.is_none() {
            let capture = batch_capture_at(state, progress.capture, &choice.path)?.clone();
            let previous = last(state, &owner.expected)
                .ok_or("Crystal batch answer lost prior boundary")?
                .history_index;
            let start = state.engine.event_history.len();
            let survivors = batch_survivors(&progress.outcomes)?;
            crate::event::commit_crystal_prepared_batch(state, survivors, &capture, &owner);
            if state.engine.halted.is_some() {
                return Err("Crystal answered simultaneous batch halted".into());
            }
            let damage_history = state.engine.event_history[start..]
                .iter()
                .enumerate()
                .filter_map(|(offset, event)| {
                    matches!(event, CommittedEvent::Damage { .. }).then_some(start + offset)
                })
                .map(|at| {
                    u32::try_from(at).map_err(|_| "Crystal simultaneous damage history exceeds u32")
                })
                .collect::<Result<Vec<_>, _>>()?;
            let mut after = before.clone();
            after.choice = None;
            let execution = ExecutionV1 {
                history_index: u32::try_from(state.engine.event_history.len())
                    .map_err(|_| "Crystal history exceeds u32")?,
                previous,
                item: Box::new(owner.expected.clone()),
                before: Box::new(before),
                after: Some(Box::new(after)),
                steps: Vec::new(),
                discard_commit: None,
                program_commit: None,
                batch_commit: Some(BatchCommitV2 {
                    capture: progress.capture,
                    outcomes: progress.outcomes,
                    damage_history,
                }),
            };
            state
                .engine
                .event_history
                .push(CommittedEvent::CrystalExecutionV1 {
                    execution: Box::new(execution),
                });
        }
        next_choice.map(menu)
    } else {
        None
    };
    state.engine.pending_effect.as_mut().unwrap().choice = next;
    record(state, &owner, Some(answer))?;
    Ok(())
}

fn program_menu(ctx: &ExecCtx, path: &[u16], options: &[EffectOp]) -> PendingEffectChoice {
    PendingEffectChoice::ChooseOption {
        player: ctx.controller,
        path: path.to_vec(),
        options: options.to_vec(),
        purpose: EffectOptionChoicePurpose::Generic,
    }
}
fn validate_program_menu_record(
    pending: &EffectContinuation,
    root: &EffectOp,
) -> Result<(), String> {
    let Some(PendingEffectChoice::ChooseOption {
        player,
        path,
        options,
        purpose: EffectOptionChoicePurpose::Generic,
    }) = pending.choice.as_ref()
    else {
        return Err("Crystal optional fight lost its program choice".into());
    };
    let op = effect_op_at_structural_path(root, path)
        .ok_or("Crystal optional fight lost original printed path")?;
    let EffectOp::Choice {
        options: original, ..
    } = op
    else {
        return Err("Crystal optional fight lost printed choice instruction".into());
    };
    let mut frames = Vec::new();
    remaining(root, path, &[], &mut frames)?;
    if !optional_source_fight(op)
        || *player != pending.ctx.controller
        || options != original
        || pending.frames != frames
        || pending.answered_choice_guard.is_some()
    {
        return Err("Crystal optional fight changed original controller/options/remainder".into());
    }
    Ok(())
}
pub(crate) fn is_program_choice(state: &GameState) -> bool {
    state.engine.pending_effect.as_ref().is_some_and(|pending| {
        pending
            .resolving_item
            .v4
            .crystal_prevention_started_v1
            .0
            .is_some()
            && matches!(
                pending.choice.as_ref(),
                Some(PendingEffectChoice::ChooseOption {
                    purpose: EffectOptionChoicePurpose::Generic,
                    ..
                })
            )
    })
}
pub(crate) fn answer_program_choice(state: &mut GameState, option: u16) -> Result<(), String> {
    validate(state)?;
    let before = state
        .engine
        .pending_effect
        .as_ref()
        .ok_or("Crystal optional choice is absent")?
        .clone();
    let boundary = last(state, &before.resolving_item)
        .ok_or("Crystal optional choice lost owner")?
        .clone();
    validate_program_menu_record(&before, &boundary.root)?;
    let Some(PendingEffectChoice::ChooseOption { path, options, .. }) = before.choice.as_ref()
    else {
        return Err("Crystal optional choice is absent".into());
    };
    let selected = options
        .get(usize::from(option))
        .ok_or("Crystal optional choice exceeds printed options")?
        .clone();
    let at = u32::try_from(state.engine.event_history.len())
        .map_err(|_| "Crystal program choice exceeds u32")?;
    let mut after = before.clone();
    after.choice = None;
    let mut selected_path = path.clone();
    selected_path.push(option);
    after.frames.push(EffectFrame::Program {
        op: selected,
        path: selected_path,
    });
    let owner = OwnerV1 {
        original: before.resolving_item.clone(),
        expected: before.resolving_item.clone(),
        root: boundary.root,
        initial: false,
        gorehorn: None,
        equipment: None,
    };
    state
        .engine
        .event_history
        .push(CommittedEvent::CrystalProgramChoiceV3 {
            operation: Box::new(ProgramChoiceOperationV3 {
                history_index: at,
                previous: boundary.history_index,
                item: Box::new(owner.expected.clone()),
                before: Box::new(before.clone()),
                option,
            }),
        });
    let execution = ExecutionV1 {
        history_index: u32::try_from(state.engine.event_history.len())
            .map_err(|_| "Crystal execution exceeds u32")?,
        previous: boundary.history_index,
        item: Box::new(owner.expected.clone()),
        before: Box::new(before),
        after: Some(Box::new(after.clone())),
        steps: Vec::new(),
        batch_commit: None,
        discard_commit: None,
        program_commit: Some(at),
    };
    state
        .engine
        .event_history
        .push(CommittedEvent::CrystalExecutionV1 {
            execution: Box::new(execution),
        });
    state.engine.pending_effect = Some(after);
    record(state, &owner, None)?;
    Ok(())
}
fn validate_program_operations(state: &GameState) -> Result<(), String> {
    for (at, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::CrystalProgramChoiceV3 { operation } = event else {
            continue;
        };
        let mut owners = state.engine.event_history.iter().filter_map(|e| match e {
            CommittedEvent::CrystalExecutionV1 { execution }
                if execution.program_commit == Some(operation.history_index) =>
            {
                Some(execution.as_ref())
            }
            _ => None,
        });
        let execution = owners
            .next()
            .ok_or("Crystal actual program answer lost execution")?;
        if operation.history_index as usize != at
            || owners.next().is_some()
            || execution.previous != operation.previous
            || execution.item != operation.item
            || execution.before != operation.before
            || at <= operation.previous as usize
            || at >= execution.history_index as usize
        {
            return Err("Crystal actual program answer changed unique owning occurrence".into());
        }
    }
    Ok(())
}
fn validate_program_commit(
    state: &GameState,
    at: usize,
    execution: &ExecutionV1,
) -> Result<(), String> {
    let index = execution
        .program_commit
        .ok_or("Crystal program commit absent")? as usize;
    let Some(CommittedEvent::CrystalProgramChoiceV3 { operation }) =
        state.engine.event_history.get(index)
    else {
        return Err("Crystal optional program lost actual accepted answer".into());
    };
    let Some(CommittedEvent::CrystalPreventionBoundaryV1 { boundary: previous }) =
        state.engine.event_history.get(execution.previous as usize)
    else {
        return Err("Crystal optional program lost previous published menu".into());
    };
    let Some(CommittedEvent::CrystalPreventionBoundaryV1 { boundary: next }) =
        state.engine.event_history.get(at + 1)
    else {
        return Err("Crystal optional program lost resulting boundary".into());
    };
    validate_program_menu_record(&execution.before, &previous.root)?;
    let Some(PendingEffectChoice::ChooseOption { path, options, .. }) =
        execution.before.choice.as_ref()
    else {
        return Err("Crystal optional program lost captured printed options".into());
    };
    let selected = options
        .get(usize::from(operation.option))
        .ok_or("Crystal accepted program answer exceeds menu")?;
    let mut expected = (*execution.before).clone();
    expected.choice = None;
    let mut path = path.clone();
    path.push(operation.option);
    expected.frames.push(EffectFrame::Program {
        op: selected.clone(),
        path,
    });
    if execution.history_index as usize != at
        || index >= at
        || index <= execution.previous as usize
        || operation.history_index as usize != index
        || operation.previous != execution.previous
        || operation.item != execution.item
        || operation.before != execution.before
        || previous.item != execution.item
        || previous.pending.as_deref() != Some(execution.before.as_ref())
        || execution.after.as_deref() != Some(&expected)
        || !execution.steps.is_empty()
        || execution.batch_commit.is_some()
        || execution.discard_commit.is_some()
        || next.previous != Some(execution.previous)
        || next.item != execution.item
        || next.root != previous.root
        || next.answer.is_some()
        || next.pending != execution.after
    {
        return Err("Crystal optional program answer changed exact owner/branch/remainder".into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "crystal_prevention_v1_tests.rs"]
mod tests;
