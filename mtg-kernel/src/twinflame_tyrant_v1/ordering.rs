//! Scoped scalar and simultaneous ordering proof, shared with the genuine Crystal removed owner.
use crate::effect::crystal_prevention_v1::{ChoiceV1, PreventionV1};
use crate::state::AbilitySourceContractV4;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AppliedMultipliersV38(pub Vec<AbilitySourceContractV4>);
impl AppliedMultipliersV38 {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
impl std::hash::Hash for AppliedMultipliersV38 {
    fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
        if !self.is_empty() {
            std::hash::Hash::hash(&"twinflame-applied/v38", h);
            std::hash::Hash::hash(&self.0, h);
        }
    }
}
/// Empty captures contribute no predecessor hash bytes or serialized field.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PreparedBatchOptionsV38(pub Vec<BatchPacketInputV38>);
#[derive(Debug, Clone, PartialEq, Eq, std::hash::Hash, Serialize, Deserialize)]
pub struct BatchPacketInputV38 {
    pub options: Vec<PreventionV1>,
    pub lifelink_player: Option<crate::ids::PlayerId>,
}
impl PreparedBatchOptionsV38 {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
impl std::hash::Hash for PreparedBatchOptionsV38 {
    fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
        if !self.is_empty() {
            std::hash::Hash::hash(&"twinflame-batch-options/v38", h);
            std::hash::Hash::hash(&self.0, h);
        }
    }
}
pub(crate) fn has_multiplier(choice: &ChoiceV1) -> bool {
    !choice.applied_multipliers.is_empty()
        || choice
            .options
            .iter()
            .any(|o| matches!(o, PreventionV1::Multiplier(_)))
}
pub(crate) fn amount_bound(choice: &ChoiceV1) -> i32 {
    choice
        .applied_multipliers
        .0
        .iter()
        .fold(choice.original_amount, |n, _| n.saturating_mul(2))
}
pub(crate) fn expected_after(choice: &ChoiceV1, index: u16) -> Result<ChoiceV1, String> {
    let mut after = choice.clone();
    match choice
        .options
        .get(usize::from(index))
        .ok_or("replacement choice outside original menu")?
    {
        PreventionV1::Multiplier(source) => {
            if after.applied_multipliers.0.contains(source) {
                return Err("multiplier applied twice to one event".into());
            }
            after.applied_multipliers.0.push(*source);
            after.remaining_amount = after.remaining_amount.saturating_mul(2);
        }
        PreventionV1::Barricade(_) | PreventionV1::Protection(_) => after.remaining_amount = 0,
        PreventionV1::Replacement(r) => {
            after.remaining_amount = match r.kind {
                crate::event::ReplacementEffectKind::PreventNextDamage { remaining, .. } => {
                    after.remaining_amount - remaining.min(after.remaining_amount)
                }
                _ => 0,
            };
            after.touched.push(r.id);
        }
    }
    after.options.retain(|o| match o {
        PreventionV1::Multiplier(source) => !after.applied_multipliers.0.contains(source),
        PreventionV1::Replacement(r) => !after.touched.contains(&r.id),
        _ => true,
    });
    Ok(after)
}

#[derive(Debug, Clone, PartialEq, Eq, std::hash::Hash, Serialize, Deserialize)]
pub struct AnswerInputV38 {
    pub index: u32,
    pub previous: u32,
    pub before: Box<crate::effect::EffectContinuation>,
    pub option: u16,
    pub lifelink_player: Option<crate::ids::PlayerId>,
}
#[derive(Debug, Clone, PartialEq, Eq, std::hash::Hash, Serialize, Deserialize)]
pub struct AnswerOperationV38 {
    pub index: u32,
    pub input: u32,
    pub after: Box<crate::effect::EffectContinuation>,
    pub damage: Option<u32>,
    pub life_gain: Option<u32>,
}
pub(crate) fn record_input(
    state: &mut crate::state::GameState,
    owner: &crate::effect::crystal_prevention_v1::OwnerV1,
    before: &crate::effect::EffectContinuation,
    option: u16,
) -> Result<u32, String> {
    if !owner.permits_replacement_answer(state, before) {
        return Err("multiplier answer lacks genuine owning row".into());
    }
    let previous = state
        .engine
        .event_history
        .iter()
        .rev()
        .find_map(|e| match e {
            crate::event::CommittedEvent::CrystalPreventionBoundaryV1 { boundary }
                if boundary.item.v4.stack_item_id == before.resolving_item.v4.stack_item_id =>
            {
                Some(boundary.history_index)
            }
            _ => None,
        })
        .ok_or("multiplier answer lost original boundary")?;
    let index = u32::try_from(state.engine.event_history.len())
        .map_err(|_| "multiplier input index overflow")?;
    let choice = match before.choice.as_ref() {
        Some(crate::effect::PendingEffectChoice::ChooseOption {
            purpose: crate::effect::EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
            ..
        }) => choice,
        _ => return Err("not a replacement menu".into()),
    };
    let lifelink_player = crate::event::twinflame_lifelink_player(state, choice);
    state
        .engine
        .event_history
        .push(crate::event::CommittedEvent::TwinflameReplacementInputV38 {
            input: Box::new(AnswerInputV38 {
                index,
                previous,
                before: Box::new(before.clone()),
                option,
                lifelink_player,
            }),
        });
    Ok(index)
}
pub(crate) fn validate(state: &crate::state::GameState) -> Result<(), String> {
    use crate::effect::{EffectOptionChoicePurpose, PendingEffectChoice};
    use crate::event::CommittedEvent as E;
    if state.engine.event_log.iter().any(|e| {
        matches!(
            e,
            E::TwinflameReplacementInputV38 { .. }
                | E::TwinflameReplacementOperationV38 { .. }
                | E::TwinflameDamagePreparationV38 { .. }
        )
    }) {
        return Err("multiplier private operation entered mutable public log".into());
    }
    for (at, event) in state.engine.event_history.iter().enumerate() {
        let E::TwinflameReplacementInputV38 { input } = event else {
            continue;
        };
        let Some(E::CrystalPreventionBoundaryV1 { boundary: previous }) =
            state.engine.event_history.get(input.previous as usize)
        else {
            return Err("multiplier lost previous genuine boundary".into());
        };
        let Some(PendingEffectChoice::ChooseOption {
            purpose: EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
            ..
        }) = input.before.choice.as_ref()
        else {
            return Err("multiplier input lost actual menu".into());
        };
        if input.index as usize != at
            || input.previous as usize >= at
            || previous.pending.as_deref() != Some(input.before.as_ref())
            || !has_multiplier(choice)
        {
            return Err("multiplier input changed captured menu".into());
        }
        let mut operations = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .filter_map(|(at, e)| match e {
                E::TwinflameReplacementOperationV38 { operation }
                    if operation.input == input.index =>
                {
                    Some((at, operation))
                }
                _ => None,
            });
        let (operation_at, operation) = operations
            .next()
            .ok_or("multiplier input lost actual answer operation")?;
        if operations.next().is_some()
            || operation.index as usize != operation_at
            || operation_at <= at
        {
            return Err("multiplier answer operation repeated or reordered".into());
        }
        let expected = expected_after(choice, input.option)?;
        let terminal = expected.remaining_amount <= 0 || expected.options.is_empty();
        let after = crate::effect::crystal_prevention_v1::replacement_answer_pending(
            state,
            &input.before,
            &expected,
        )?;
        let Some(E::CrystalPreventionBoundaryV1 { boundary }) =
            state.engine.event_history.get(operation_at + 1)
        else {
            return Err("multiplier operation lost immediate result boundary".into());
        };
        if operation.after.as_ref() != &after
            || boundary.previous != Some(input.previous)
            || boundary.item != previous.item
            || boundary.pending.as_deref() != Some(&after)
            || boundary.answer.as_ref()
                != Some(&crate::effect::crystal_prevention_v1::AnswerV1 {
                    before: (**choice).clone(),
                    option: input.option,
                })
        {
            return Err("multiplier operation changed remaining packet or continuation".into());
        }
        if choice.batch.is_none() && terminal && expected.remaining_amount > 0 {
            let index = operation
                .damage
                .ok_or("surviving multiplier packet lost actual Damage")?
                as usize;
            if index <= at
                || index >= operation_at
                || !matches!(state.engine.event_history.get(index),Some(E::Damage{source,target,amount,..}) if *source==expected.source && *target==expected.target && *amount==expected.remaining_amount)
            {
                return Err("multiplier result differs from independent actual damage".into());
            }
        } else if operation.damage.is_some() || operation.life_gain.is_some() {
            return Err("uncommitted multiplier packet claimed an outcome".into());
        }
        if choice.batch.is_none()
            && terminal
            && expected.remaining_amount > 0
            && operation.life_gain.is_some() != input.lifelink_player.is_some()
        {
            return Err("multiplier outcome lost mandatory lifelink occurrence".into());
        }
        if let Some(index) = operation.life_gain {
            if operation.damage.is_none_or(|d| d >= index)
                || index as usize >= operation_at
                || !matches!(state.engine.event_history.get(index as usize),Some(E::LifeGain{amount,player,..}) if *amount==expected.remaining_amount && Some(*player)==input.lifelink_player)
            {
                return Err("multiplier lifelink occurrence changed".into());
            }
        }
    }
    for (at, event) in state.engine.event_history.iter().enumerate() {
        let E::TwinflameDamagePreparationV38 { preparation: p } = event else {
            continue;
        };
        let Some(E::CrystalPreventionBoundaryV1 { boundary }) =
            state.engine.event_history.get(p.previous as usize)
        else {
            return Err("multiplier preparation lost original private owner boundary".into());
        };
        if p.index as usize != at
            || p.previous as usize >= at
            || boundary.item != p.item
            || boundary
                .pending
                .as_deref()
                .is_none_or(|before| before.ctx != p.ctx)
            || !p
                .options
                .iter()
                .any(|o| matches!(o, PreventionV1::Multiplier(_)))
        {
            return Err("multiplier preparation changed original owner/context/occurrence".into());
        }
        let mut outputs = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .filter_map(|(index, e)| match e {
                E::CrystalDamageOperationV3 { operation }
                    if operation.ctx == p.ctx
                        && operation.op == p.op
                        && operation.path == p.path =>
                {
                    Some((index, operation))
                }
                _ => None,
            });
        let (output_at, output) = outputs
            .next()
            .ok_or("multiplier preparation lost actual prepare/commit outcome")?;
        if outputs.next().is_some()
            || output_at <= at
            || output.input != p.input
            || output.preventers != p.options
            || output.historical_source != p.historical_source
        {
            return Err("multiplier preparation repeated or changed actual output".into());
        }
        let amount = prepared_amount(p)?;
        let gains = if let Some(damage) = output.damage_history {
            let interval = state
                .engine
                .event_history
                .get(damage as usize + 1..output_at)
                .ok_or("multiplier damage/gain interval is invalid")?;
            interval.iter().filter(|e|matches!(e,E::LifeGain{player,amount:n} if Some(*player)==p.lifelink_player && *n==amount)).count()
        } else {
            0
        };
        if gains != usize::from(amount > 0 && p.lifelink_player.is_some()) {
            return Err("multiplier preparation lost mandatory actual lifelink result".into());
        }
    }
    for event in &state.engine.event_history {
        if let E::TwinflameReplacementOperationV38 { operation } = event {
            if !matches!(state.engine.event_history.get(operation.input as usize),Some(E::TwinflameReplacementInputV38{input}) if input.index==operation.input)
            {
                return Err("orphan multiplier answer operation".into());
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, std::hash::Hash, Serialize, Deserialize)]
pub struct PreparationV38 {
    pub index: u32,
    pub previous: u32,
    pub item: Box<crate::state::StackItem>,
    pub ctx: crate::effect::ExecCtx,
    pub op: crate::effect::EffectOp,
    pub path: Vec<u16>,
    pub input: crate::effect::crystal_prevention_v1::BatchDamageV2,
    pub historical_source: Option<crate::ids::StackItemId>,
    pub options: Vec<PreventionV1>,
    pub lifelink_player: Option<crate::ids::PlayerId>,
}
pub(crate) fn preparation_at<'a>(
    state: &'a crate::state::GameState,
    operation: &crate::effect::crystal_prevention_v1::DamageOperationV3,
    until: usize,
) -> Result<Option<&'a PreparationV38>, String> {
    use crate::event::CommittedEvent as E;
    let mut preparations = state.engine.event_history[..until]
        .iter()
        .filter_map(|e| match e {
            E::TwinflameDamagePreparationV38 { preparation }
                if preparation.ctx == operation.ctx
                    && preparation.op == operation.op
                    && preparation.path == operation.path =>
            {
                Some(preparation.as_ref())
            }
            _ => None,
        });
    let value = preparations.next();
    if preparations.next().is_some() {
        return Err("duplicate multiplier packet preparation".into());
    }
    if operation
        .preventers
        .iter()
        .any(|o| matches!(o, PreventionV1::Multiplier(_)))
        && value.is_none()
    {
        return Err("multiplier output lost independent original preparation".into());
    }
    if let Some(p) = value {
        if p.input != operation.input
            || p.historical_source != operation.historical_source
            || p.options != operation.preventers
        {
            return Err("multiplier changed independent original packet/source capture".into());
        }
    }
    Ok(value)
}
pub(crate) fn prepared_amount(p: &PreparationV38) -> Result<i32, String> {
    commuting_amount(p.input.amount, &p.options)
}
pub(crate) fn commuting_amount(input: i32, options: &[PreventionV1]) -> Result<i32, String> {
    use crate::event::ReplacementEffectKind as R;
    if options.iter().any(|o|matches!(o,PreventionV1::Barricade(_)|PreventionV1::Protection(_)) || matches!(o,PreventionV1::Replacement(r) if matches!(r.kind,R::PreventDamageFromColorUntilEndOfTurn{..}|R::PreventCombatDamageToObjectUntilEndOfTurn{..}))) {return Ok(0);}
    let numeric = options
        .iter()
        .any(|o| matches!(o, PreventionV1::Replacement(_)));
    if numeric
        && options
            .iter()
            .any(|o| matches!(o, PreventionV1::Multiplier(_)))
    {
        return Err("noncommuting packet bypassed actual replacement menu".into());
    }
    if numeric {
        return Ok(options.iter().fold(input, |amount, o| match o {
            PreventionV1::Replacement(r) => match r.kind {
                R::PreventNextDamage { remaining, .. } => amount - remaining.min(amount),
                _ => 0,
            },
            _ => amount,
        }));
    }
    Ok(options.iter().fold(input, |amount, o| {
        if matches!(o, PreventionV1::Multiplier(_)) {
            amount.saturating_mul(2)
        } else {
            amount
        }
    }))
}
