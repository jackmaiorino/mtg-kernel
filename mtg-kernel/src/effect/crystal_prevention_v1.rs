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
            historical_source: None,
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
            if d.source == self.expected.source && !d.is_combat && d.historical_source.is_none())
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
fn has_damage(op: &EffectOp) -> bool {
    match op {
        EffectOp::DealDamage { .. } => true,
        EffectOp::Sequence(ops) => ops.iter().any(has_damage),
        EffectOp::Conditional { then, else_, .. } => has_damage(then) || has_damage(else_),
        _ => false,
    }
}
fn supported(op: &EffectOp) -> bool {
    match op {
        EffectOp::DealDamage { .. } => true,
        EffectOp::Sequence(ops) => ops.iter().all(supported),
        EffectOp::Conditional { then, else_, .. } => supported(then) && supported(else_),
        _ => false,
    }
}
pub(crate) fn requires_successor(state: &GameState) -> bool {
    crate::crystal_barricade_v1::active(state)
        || boundaries(state).next().is_some()
        || state
            .engine
            .event_history
            .iter()
            .any(|event| matches!(event, CommittedEvent::CrystalShieldOperationV1 { .. }))
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
/// Deliberately conservative coverage boundary. No unsupported synchronous
/// program may begin in this opt-in mixed environment. Ward/payment and other
/// producers need a separate composable owned interpreter before admission.
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
    let root = *validated_definition_owned_root_effect(state, &probe(item))?;
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
    validate_shields(state)?;
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
                let Some(EffectOp::DealDamage { target, amount }) =
                    effect_op_at_structural_path(&b.root, &choice.path)
                else {
                    return Err("Crystal historical menu lost its printed instruction".into());
                };
                let mut frames = Vec::new();
                remaining(&b.root, &choice.path, &[], &mut frames)?;
                if p.choice.as_ref() != Some(&menu((**choice).clone()))
                    || choice.source != p.ctx.source
                    || choice.target != p.ctx.resolve_target(*target)
                    || choice.original_amount != *amount
                    || choice.remaining_amount <= 0
                    || choice.remaining_amount > *amount
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
    let result = drive_resumable_inner(state, Some(owner))?;
    if let ResumableProgress::Complete(item) = &result {
        if !owner.completion_matches(item) {
            return Err("Crystal completion changed owner".into());
        }
    }
    record(state, owner, None)?;
    Ok(result)
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
    if crate::engine::damage_is_prevented_by_protection(state, damage.source, damage.target) {
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
    let EffectOp::DealDamage { target, amount } = op else {
        return Ok(false);
    };
    let damage = DamageProposed {
        source: continuation.ctx.source,
        target: continuation.ctx.resolve_target(*target),
        amount: *amount,
        touched_by: Vec::new(),
        is_combat: false,
        historical_source: None,
    };
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
    let Some(EffectOp::DealDamage { target, amount }) =
        effect_op_at_structural_path(&root, &choice.path)
    else {
        return Err("Crystal choice lost its actual fixed-damage instruction".into());
    };
    let mut expected_frames = Vec::new();
    remaining(&root, &choice.path, &[], &mut expected_frames)?;
    let Target::Object(recipient) = choice.target else {
        return Err("Crystal choice lost its affected permanent".into());
    };
    if pending.choice.as_ref() != Some(&menu((**choice).clone()))
        || choice.source != pending.ctx.source
        || choice.target != pending.ctx.resolve_target(*target)
        || choice.original_amount != *amount
        || choice.remaining_amount <= 0
        || choice.remaining_amount > *amount
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
