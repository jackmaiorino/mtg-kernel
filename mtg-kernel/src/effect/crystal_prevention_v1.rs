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
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExecutionStepV1 {
    pub frame: EffectFrame,
    pub conditional_result: Option<bool>,
    pub offered_menu: Option<PendingEffectChoice>,
    pub damage_history: Vec<u32>,
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
        EffectOp::DrawCards { .. }
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
        EffectOp::DealDamage { target, amount } => (ctx.resolve_target(*target), *amount, None),
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
        && boundary.root
            == EffectOp::StandardV1(crate::standard_cards_v1::StandardOpV1::SourceLastPowerDamageV1)
}
fn has_damage(op: &EffectOp) -> bool {
    match op {
        EffectOp::DealDamage { .. }
        | EffectOp::StandardV1(crate::standard_cards_v1::StandardOpV1::SourceLastPowerDamageV1) => {
            true
        }
        EffectOp::Sequence(ops) => ops.iter().any(has_damage),
        EffectOp::Conditional { then, else_, .. } => has_damage(then) || has_damage(else_),
        _ => false,
    }
}
fn supported(op: &EffectOp) -> bool {
    match op {
        EffectOp::DealDamage { .. }
        | EffectOp::StandardV1(crate::standard_cards_v1::StandardOpV1::SourceLastPowerDamageV1) => {
            true
        }
        EffectOp::Sequence(ops) => ops.iter().all(supported),
        EffectOp::Conditional { then, else_, .. } => supported(then) && supported(else_),
        _ => false,
    }
}
pub(crate) fn requires_successor(state: &GameState) -> bool {
    crate::crystal_barricade_v1::active(state)
        || boundaries(state).next().is_some()
        || state.engine.event_history.iter().any(|event| {
            matches!(
                event,
                CommittedEvent::CrystalShieldOperationV1 { .. }
                    | CommittedEvent::CrystalExecutionV1 { .. }
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
    if !started && !mixed_environment(state) {
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
    let root = *validated_definition_owned_root_effect(state, &probe(item))?;
    if !started && ordinary_nondamage(&root) {
        return Ok(None);
    }
    if !supported(&root) {
        return Err("mixed Crystal prevention needs a supported fixed-damage producer".into());
    }
    if !has_damage(&root) {
        return Ok(None);
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
    Ok(Some(OwnerV1 {
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
    if state.engine.event_log.iter().any(|event| {
        matches!(
            event,
            CommittedEvent::CrystalPreventionBoundaryV1 { .. }
                | CommittedEvent::CrystalShieldOperationV1 { .. }
                | CommittedEvent::CrystalExecutionV1 { .. }
        )
    }) {
        return Err("private Crystal ownership cannot enter the public trigger log".into());
    }
    validate_shields(state)?;
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
                if old.choice.is_some() && b.pending.as_deref() != Some(old) {
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
            next.choice = (expected.remaining_amount > 0).then(|| menu(expected));
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
                || *validated_definition_owned_root_effect(state, &initial)? != b.root
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
                let op = effect_op_at_structural_path(&b.root, &choice.path)
                    .ok_or("Crystal historical menu lost its printed instruction")?;
                let damage = original_damage(state, &p.ctx, op)?;
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
                crate::event::propose_and_commit_crystal_owned(
                    state,
                    crate::event::ProposedEvent::Damage(damage),
                    owner,
                );
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
    };
    state
        .engine
        .event_history
        .push(CommittedEvent::CrystalExecutionV1 {
            execution: Box::new(execution),
        });
    record(state, owner, None)?;
    Ok(if suspended {
        ResumableProgress::Suspended
    } else {
        ResumableProgress::Complete(Box::new(owner.expected.clone()))
    })
}
fn validate_executions(state: &GameState) -> Result<(), String> {
    for (index, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::CrystalExecutionV1 { execution } = event else {
            continue;
        };
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
            match op {
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
        _ => return Err("Crystal path traverses an unsupported instruction".into()),
    };
    let mut next = prefix.to_vec();
    next.push(head);
    remaining(child, tail, &next, frames)
}
fn validate_choice(state: &GameState, pending: &EffectContinuation) -> Result<(), String> {
    let Some(PendingEffectChoice::ChooseOption {
        purpose: EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
        ..
    }) = pending.choice.as_ref()
    else {
        return Ok(());
    };
    let root = validated_definition_owned_root_effect(state, pending)?;
    let op = effect_op_at_structural_path(&root, &choice.path)
        .ok_or("Crystal choice lost its actual damage instruction")?;
    let damage = original_damage(state, &pending.ctx, op)?;
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
    } else {
        None
    };
    state.engine.pending_effect.as_mut().unwrap().choice = next;
    record(state, &owner, Some(answer))?;
    Ok(())
}

#[cfg(test)]
#[path = "crystal_prevention_v1_tests.rs"]
mod tests;
