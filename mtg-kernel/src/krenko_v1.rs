//! Definition-owned Krenko activation and simultaneous token-entry authority.
use crate::card_def::{CostComponent, DynamicValueDef, Subtype, TargetSpec, CARD_DEFS};
use crate::effect::{EffectObjectBinding, EffectOp, ExecCtx};
use crate::event::{CommittedEvent, ProposedEvent};
use crate::ids::StackItemId;
use crate::state::{GameState, StackItem, StackItemKind, Zone};
use serde::{Deserialize, Serialize};

pub const COUNT: DynamicValueDef =
    DynamicValueDef::ControlledPermanentsWithSubtype(Subtype::Goblin);
pub fn program() -> EffectOp {
    EffectOp::KrenkoCreateGoblinsV1
}
pub(crate) fn selected_definition(id: u16) -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && CARD_DEFS
        .get(usize::from(id))
        .is_some_and(|d| d.is_executable() && d.name == "Krenko, Mob Boss")
}
fn selected(item: &StackItem) -> bool {
    item.v4.activated_ability_index == Some(0)
        && item
            .v4
            .ability_source_contract
            .is_some_and(|s| selected_definition(s.card_def))
}
pub(crate) fn token_definition() -> Result<u16, String> {
    let id = crate::card_def::card_id_by_name("Goblin Token")
        .ok_or("Krenko requires actual Goblin Token")?;
    let d = &CARD_DEFS[usize::from(id)];
    if !d.is_executable()
        || !d.has_full_support()
        || !d.is_token
        || d.is_land
        || d.types != [crate::card_def::CardType::Creature]
        || d.subtypes != [Subtype::Goblin]
        || d.colors != [crate::mana::ManaColor::R]
        || (d.power, d.toughness) != (Some(1), Some(1))
        || d.keywords != crate::card_def::Keywords::NONE
        || !d.supertypes.is_empty()
        || d.cost.generic != 0
        || d.cost.x_count != 0
        || !d.cost.pips.is_empty()
        || !d.activated_abilities.is_empty()
        || !crate::trigger::triggers_for(id).is_empty()
        || d.changeling
        || d.transform_face.is_some()
        || d.animation.is_some()
    {
        return Err("Krenko token differs from printed 1/1 red Goblin".into());
    }
    Ok(id)
}
fn shape(state: &GameState, item: &StackItem) -> Result<(), String> {
    let source = item
        .v4
        .ability_source_contract
        .ok_or("Krenko source lost")?;
    let def = CARD_DEFS
        .get(usize::from(source.card_def))
        .ok_or("Krenko definition lost")?;
    let [ability] = def.activated_abilities else {
        return Err("Krenko activation count changed".into());
    };
    let expected = crate::state::StackStateV4 {
        stack_item_id: item.v4.stack_item_id,
        ability_source_contract: Some(source),
        activated_ability_index: Some(0),
        target_spec: Some(TargetSpec::None),
        ..Default::default()
    };
    if !selected(item)
        || item.kind != StackItemKind::ActivatedAbility
        || item.source != source.source
        || item.controller != source.controller
        || source.zone != Zone::Battlefield
        || item.v4.activated_ability_index != Some(0)
        || ability.cost != [CostComponent::Tap]
        || ability.target_spec != TargetSpec::None
        || (ability.effect)() != program()
        || ability.activation_zone != Zone::Battlefield
        || ability.sorcery_speed_only
        || ability.max_activations_per_turn.is_some()
        || ability.face.is_some()
        || ability.activation_target_filter
            != crate::card_def::ActivationTargetFilter::TargetSpecOnly
        || item.inline_effect != Some(program())
        || item.v4.stack_item_id == StackItemId::default()
        || item.is_copy
        || item.is_flashback
        || item.kicked
        || item.madness_offer
        || !item.targets.is_empty()
        || !item.discarded.is_empty()
        || item.v4.target_spec != Some(TargetSpec::None)
        || !item.v4.target_contracts.is_empty()
        || !item.v4.paid_cost_refs.is_empty()
        || item.v4.granted_by.is_some()
        || item.v4.cauldron_grant.0.is_some()
        || item.v4.source_contract.is_some()
        || item.mode_chosen != 0
        || item.v4 != expected
    {
        return Err("Krenko paid activation metadata changed".into());
    }
    crate::engine::validate_historical_ability_source_contract(state, source)
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PaidV1 {
    pub index: u64,
    pub input: u64,
    pub tap_history_index: u64,
    pub item: Box<StackItem>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PaymentInputV1 {
    pub index: u64,
    pub stack_item_id: StackItemId,
    pub source: crate::state::AbilitySourceContractV4,
    pub tap_history_index: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RemovalV1 {
    Resolving,
    Countered,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RemovedV1 {
    pub index: u64,
    pub paid: u64,
    pub item: Box<StackItem>,
    pub kind: RemovalV1,
    pub input: u64,
}
/// Independent original input captured only by the actual engine removal hook.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RemovalInputV1 {
    pub index: u64,
    pub paid: u64,
    pub item: Box<StackItem>,
    pub kind: RemovalV1,
    pub members: Option<Vec<EffectObjectBinding>>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SampleV1 {
    pub index: u64,
    pub removed: u64,
    pub members: Vec<EffectObjectBinding>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CompletedV1 {
    pub index: u64,
    pub removed: u64,
    pub sample: Option<u64>,
    pub token_history: Vec<u64>,
}
/// The genuine full live row is borrowed privately before the engine removes it.
pub(crate) struct OwnerV1 {
    item: StackItem,
    paid: u64,
}
fn paid(state: &GameState, id: StackItemId) -> Result<&PaidV1, String> {
    let rows = state
        .engine
        .event_history
        .iter()
        .filter_map(|e| match e {
            CommittedEvent::KrenkoPaidV1 { paid } if paid.item.v4.stack_item_id == id => {
                Some(paid.as_ref())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [p] = rows.as_slice() else {
        return Err("Krenko lost unique actual payment".into());
    };
    let inputs = state
        .engine
        .event_history
        .iter()
        .filter_map(|e| match e {
            CommittedEvent::KrenkoPaymentInputV1 { input } if input.stack_item_id == id => {
                Some(input.as_ref())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [input] = inputs.as_slice() else {
        return Err("Krenko payment lost independent original source input".into());
    };
    if input.index != p.input
        || input.index.checked_add(1) != Some(p.index)
        || input.tap_history_index >= input.index
        || input.tap_history_index != p.tap_history_index
        || Some(input.source) != p.item.v4.ability_source_contract
        || !matches!(state.engine.event_history.get(input.index as usize),Some(CommittedEvent::KrenkoPaymentInputV1 { input: original }) if original.as_ref() == *input)
    {
        return Err("Krenko payment rewrote original captured source/controller".into());
    }
    if !matches!(state.engine.event_history.get(p.index as usize),Some(CommittedEvent::KrenkoPaidV1 {paid}) if paid.as_ref()==*p)
        || p.tap_history_index >= p.index
        || !matches!(state.engine.event_history.get(p.tap_history_index as usize),Some(CommittedEvent::Tap {object}) if *object==p.item.source)
    {
        return Err("Krenko payment lost actual tap occurrence".into());
    }
    shape(state, &p.item)?;
    Ok(p)
}
pub(crate) fn record_paid(
    state: &mut GameState,
    id: StackItemId,
    start: usize,
    captured_source: crate::state::AbilitySourceContractV4,
) -> Result<(), String> {
    let item = state
        .stack
        .iter()
        .find(|i| i.v4.stack_item_id == id)
        .ok_or("Krenko publication missing")?;
    if !selected(item) {
        return Ok(());
    }
    shape(state, item)?;
    if item.v4.ability_source_contract != Some(captured_source) {
        return Err("Krenko full publication changed the private prepayment source".into());
    }
    let taps = state.engine.event_history[start..]
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            matches!(e,CommittedEvent::Tap {object} if *object==item.source).then_some(start + i)
        })
        .collect::<Vec<_>>();
    let [tap] = taps.as_slice() else {
        return Err("Krenko full payment lacks unique tap".into());
    };
    let source = item.v4.ability_source_contract.unwrap();
    let live = state.objects.get(item.source);
    if live.zone != Zone::Battlefield
        || live.zone_change_count != source.zone_change_count
        || !live.tapped
    {
        return Err("Krenko publication did not pay tap on original source".into());
    }
    let original_item = item.clone();
    let input = state.engine.event_history.len() as u64;
    state
        .engine
        .event_history
        .push(CommittedEvent::KrenkoPaymentInputV1 {
            input: Box::new(PaymentInputV1 {
                index: input,
                stack_item_id: id,
                source: captured_source,
                tap_history_index: *tap as u64,
            }),
        });
    let p = PaidV1 {
        index: state.engine.event_history.len() as u64,
        input,
        tap_history_index: *tap as u64,
        item: Box::new(original_item),
    };
    state
        .engine
        .event_history
        .push(CommittedEvent::KrenkoPaidV1 { paid: Box::new(p) });
    Ok(())
}
pub(crate) fn tracked(state: &GameState, item: &StackItem) -> bool {
    selected(item)
        || item.inline_effect == Some(program())
        || state.engine.event_history.iter().any(|e| match e {
            CommittedEvent::KrenkoPaidV1 { paid } => {
                paid.item.v4.stack_item_id == item.v4.stack_item_id
            }
            CommittedEvent::KrenkoPaymentInputV1 { input } => {
                input.stack_item_id == item.v4.stack_item_id
            }
            _ => false,
        })
}
pub(crate) fn validate_item(state: &GameState, item: &StackItem) -> Result<(), String> {
    if !tracked(state, item) {
        return Ok(());
    }
    let p = paid(state, item.v4.stack_item_id)?;
    if p.item.as_ref() != item
        || state
            .engine
            .event_history
            .iter()
            .any(|e| matches!(e,CommittedEvent::KrenkoRemovedV1 {removed} if removed.paid==p.index))
    {
        return Err("Krenko live row changed or replayed".into());
    }
    Ok(())
}
pub(crate) fn acquire(state: &GameState, item: &StackItem) -> Result<Option<OwnerV1>, String> {
    if !tracked(state, item) {
        return Ok(None);
    }
    validate_item(state, item)?;
    let rows = state
        .stack
        .iter()
        .filter(|i| i.v4.stack_item_id == item.v4.stack_item_id)
        .collect::<Vec<_>>();
    if rows.as_slice() != [item] {
        return Err("Krenko private loan lacks exact unique live row".into());
    }
    Ok(Some(OwnerV1 {
        item: item.clone(),
        paid: paid(state, item.v4.stack_item_id)?.index,
    }))
}
pub(crate) fn record_input(
    state: &mut GameState,
    owner: Option<&OwnerV1>,
    kind: RemovalV1,
) -> Result<(), String> {
    let Some(owner) = owner else {
        return Ok(());
    };
    if state
        .stack
        .iter()
        .any(|i| i.v4.stack_item_id == owner.item.v4.stack_item_id)
        || paid(state, owner.item.v4.stack_item_id)?.item.as_ref() != &owner.item
    {
        return Err("Krenko input lacks actual removed private owner".into());
    }
    let input = RemovalInputV1 {
        index: state.engine.event_history.len() as u64,
        paid: owner.paid,
        item: Box::new(owner.item.clone()),
        kind,
        members: (kind == RemovalV1::Resolving).then(|| members(state, owner.item.controller)),
    };
    state
        .engine
        .event_history
        .push(CommittedEvent::KrenkoRemovalInputV1 {
            input: Box::new(input),
        });
    Ok(())
}
fn original_input<'a>(
    state: &'a GameState,
    owner: &OwnerV1,
    kind: RemovalV1,
) -> Result<&'a RemovalInputV1, String> {
    let inputs = state
        .engine
        .event_history
        .iter()
        .filter_map(|e| match e {
            CommittedEvent::KrenkoRemovalInputV1 { input } if input.paid == owner.paid => {
                Some(input.as_ref())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [input] = inputs.as_slice() else {
        return Err("Krenko operation lost independent actual removal input".into());
    };
    if input.item.as_ref() != &owner.item || input.kind != kind {
        return Err("Krenko input changed original full row or removal kind".into());
    }
    Ok(input)
}
fn removed(state: &mut GameState, owner: &OwnerV1, kind: RemovalV1) -> Result<u64, String> {
    if state
        .stack
        .iter()
        .any(|i| i.v4.stack_item_id == owner.item.v4.stack_item_id)
        || paid(state, owner.item.v4.stack_item_id)?.item.as_ref() != &owner.item
    {
        return Err("Krenko operation lacks genuine removed row".into());
    }
    let input = original_input(state, owner, kind)?.index;
    let index = state.engine.event_history.len() as u64;
    state
        .engine
        .event_history
        .push(CommittedEvent::KrenkoRemovedV1 {
            removed: Box::new(RemovedV1 {
                index,
                paid: owner.paid,
                item: Box::new(owner.item.clone()),
                kind,
                input,
            }),
        });
    Ok(index)
}
pub(crate) fn members(
    state: &GameState,
    controller: crate::ids::PlayerId,
) -> Vec<EffectObjectBinding> {
    let DynamicValueDef::ControlledPermanentsWithSubtype(subtype) = COUNT else {
        unreachable!()
    };
    crate::engine::controlled_permanents_with_subtype(state, controller, subtype)
        .map(|object| EffectObjectBinding {
            object,
            expected_zone: Zone::Battlefield,
            expected_zone_change_count: state.objects.get(object).zone_change_count,
        })
        .collect()
}
/// Actual token requests form one entry batch. Every observer sees the complete
/// roster, with all entry characteristics installed, before the caller runs SBA.
pub(crate) fn resolve(state: &mut GameState, owner: OwnerV1, ctx: &ExecCtx) -> Result<(), String> {
    if ctx.stack_item_id != Some(owner.item.v4.stack_item_id)
        || ctx.controller != owner.item.controller
        || ctx.source != owner.item.source
        || ctx.ability_source_contract != owner.item.v4.ability_source_contract
        || !ctx.targets.is_empty()
        || !ctx.target_contracts.is_empty()
    {
        return Err("Krenko removed context changed".into());
    }
    let token = token_definition()?;
    let removed = removed(state, &owner, RemovalV1::Resolving)?;
    let roster = original_input(state, &owner, RemovalV1::Resolving)?
        .members
        .clone()
        .ok_or("Krenko resolution input lost actual Goblin roster")?;
    if roster
        .iter()
        .enumerate()
        .any(|(i, m)| roster[..i].contains(m))
    {
        return Err("Krenko Goblin membership duplicated".into());
    }
    let sample = state.engine.event_history.len() as u64;
    let count = roster.len();
    state
        .engine
        .event_history
        .push(CommittedEvent::KrenkoSampleV1 {
            sample: Box::new(SampleV1 {
                index: sample,
                removed,
                members: roster,
            }),
        });
    let start = state.engine.event_history.len();
    crate::event::batch_without_linked_aura_v1(
        state,
        (0..count)
            .map(|_| ProposedEvent::create_token(token, owner.item.controller))
            .collect(),
    );
    if state.engine.halted.is_some() {
        return Err("Krenko entry batch halted".into());
    }
    let token_history=state.engine.event_history[start..].iter().enumerate().filter_map(|(i,e)|
        matches!(e,CommittedEvent::CreateToken {token_def,controller,..} if *token_def==token && *controller==owner.item.controller).then_some((start+i) as u64)).collect::<Vec<_>>();
    if token_history.len()
        != crate::doubling_season_v48::expected_tokens(
            state,
            start,
            state.engine.event_history.len(),
            token,
            owner.item.controller,
            count,
        )?
    {
        return Err("Krenko actual token batch differs from sampled count".into());
    }
    let index = state.engine.event_history.len() as u64;
    state
        .engine
        .event_history
        .push(CommittedEvent::KrenkoCompletedV1 {
            completed: Box::new(CompletedV1 {
                index,
                removed,
                sample: Some(sample),
                token_history,
            }),
        });
    validate_state(state)
}
pub(crate) fn countered(state: &mut GameState, owner: Option<OwnerV1>) -> Result<(), String> {
    let Some(owner) = owner else {
        return Ok(());
    };
    let removed = removed(state, &owner, RemovalV1::Countered)?;
    let index = state.engine.event_history.len() as u64;
    state
        .engine
        .event_history
        .push(CommittedEvent::KrenkoCompletedV1 {
            completed: Box::new(CompletedV1 {
                index,
                removed,
                sample: None,
                token_history: Vec::new(),
            }),
        });
    validate_state(state)
}
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    if state.engine.event_log.iter().any(|e| {
        matches!(
            e,
            CommittedEvent::KrenkoPaidV1 { .. }
                | CommittedEvent::KrenkoRemovedV1 { .. }
                | CommittedEvent::KrenkoSampleV1 { .. }
                | CommittedEvent::KrenkoCompletedV1 { .. }
                | CommittedEvent::KrenkoRemovalInputV1 { .. }
                | CommittedEvent::KrenkoPaymentInputV1 { .. }
        )
    }) {
        return Err("private Krenko journal entered transient log".into());
    }
    for item in &state.stack {
        validate_item(state, item)?;
    }
    for (at, e) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::KrenkoPaidV1 { paid: p } = e else {
            continue;
        };
        if p.index != at as u64 || paid(state, p.item.v4.stack_item_id)? != p.as_ref() {
            return Err("Krenko paid index changed".into());
        }
        let removals = state
            .engine
            .event_history
            .iter()
            .filter_map(|e| match e {
                CommittedEvent::KrenkoRemovedV1 { removed: r } if r.paid == p.index => {
                    Some(r.as_ref())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let inputs = state
            .engine
            .event_history
            .iter()
            .filter_map(|e| match e {
                CommittedEvent::KrenkoRemovalInputV1 { input } if input.paid == p.index => {
                    Some(input.as_ref())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let live = state
            .stack
            .iter()
            .filter(|i| i.v4.stack_item_id == p.item.v4.stack_item_id)
            .collect::<Vec<_>>();
        match (removals.as_slice(), live.as_slice()) {
            ([], [item]) if *item == p.item.as_ref() && inputs.is_empty() => {}
            ([r], []) => {
                let [input] = inputs.as_slice() else {
                    return Err("Krenko closure lost original actual removal input".into());
                };
                if input.item != p.item
                    || input.paid != p.index
                    || input.index <= p.index
                    || input.index >= r.index
                    || input.kind != r.kind
                    || r.input != input.index
                    || !matches!(state.engine.event_history.get(input.index as usize), Some(CommittedEvent::KrenkoRemovalInputV1 { input: original }) if original.as_ref() == *input)
                {
                    return Err("Krenko closure redirected original removal input".into());
                }

                if r.item != p.item
                    || r.index <= p.index
                    || !matches!(state.engine.event_history.get(r.index as usize),Some(CommittedEvent::KrenkoRemovedV1 {removed}) if removed.as_ref()==*r)
                {
                    return Err("Krenko removal lost original publication".into());
                }
                let finishes = state
                    .engine
                    .event_history
                    .iter()
                    .filter_map(|e| match e {
                        CommittedEvent::KrenkoCompletedV1 { completed: c }
                            if c.removed == r.index =>
                        {
                            Some(c.as_ref())
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                let [c] = finishes.as_slice() else {
                    return Err("Krenko removed operation lost unique completion".into());
                };
                if c.index <= r.index
                    || !matches!(state.engine.event_history.get(c.index as usize),Some(CommittedEvent::KrenkoCompletedV1 {completed}) if completed.as_ref()==*c)
                {
                    return Err("Krenko completion index changed".into());
                }
                let samples = state
                    .engine
                    .event_history
                    .iter()
                    .filter_map(|e| match e {
                        CommittedEvent::KrenkoSampleV1 { sample: s } if s.removed == r.index => {
                            Some(s.as_ref())
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                match r.kind {
                    RemovalV1::Countered
                        if c.sample.is_none()
                            && c.token_history.is_empty()
                            && samples.is_empty()
                            && input.members.is_none()
                            && r.index.checked_add(1) == Some(c.index) => {}
                    RemovalV1::Resolving => {
                        let [s] = samples.as_slice() else {
                            return Err("Krenko resolution lost original count sample".into());
                        };
                        if input.members.as_ref() != Some(&s.members)
                            || c.sample != Some(s.index)
                            || s.index <= r.index
                            || s.index >= c.index
                            || !matches!(state.engine.event_history.get(s.index as usize),Some(CommittedEvent::KrenkoSampleV1 {sample}) if sample.as_ref()==*s)
                            || s.members.iter().enumerate().any(|(i, m)| {
                                s.members[..i].contains(m) || m.expected_zone != Zone::Battlefield
                            })
                            || c.token_history.len()
                                != crate::doubling_season_v48::expected_tokens(
                                    state,
                                    s.index as usize,
                                    c.index as usize,
                                    token_definition()?,
                                    p.item.controller,
                                    s.members.len(),
                                )?
                        {
                            return Err("Krenko completion redirected count sample".into());
                        }
                        let token = token_definition()?;
                        let mut objects = Vec::new();
                        for (i, index) in c.token_history.iter().enumerate() {
                            if *index <= s.index
                                || *index >= c.index
                                || (i > 0 && c.token_history[i - 1] >= *index)
                            {
                                return Err("Krenko token order changed".into());
                            }
                            let Some(CommittedEvent::CreateToken {
                                object,
                                token_def,
                                controller,
                            }) = state.engine.event_history.get(*index as usize)
                            else {
                                return Err("Krenko actual token occurrence lost".into());
                            };
                            if *token_def != token
                                || *controller != p.item.controller
                                || objects.contains(object)
                            {
                                return Err("Krenko token recipient/definition changed".into());
                            }
                            objects.push(*object);
                        }
                    }
                    _ => return Err("Krenko counter closure hid token execution".into()),
                }
            }
            _ => return Err("Krenko mandatory paid owner erased or replayed".into()),
        }
    }
    for e in &state.engine.event_history {
        match e {
            CommittedEvent::KrenkoPaymentInputV1 { input } => {
                paid(state, input.stack_item_id)?;
            }
            CommittedEvent::KrenkoRemovalInputV1 { input } => {
                let original = paid(state, input.item.v4.stack_item_id)?;
                if input.paid != original.index || input.item != original.item {
                    return Err("orphan Krenko original removal input".into());
                }
            }
            CommittedEvent::KrenkoRemovedV1 { removed: r } => {
                let p = paid(state, r.item.v4.stack_item_id)?;
                if r.paid != p.index || r.item != p.item {
                    return Err("orphan Krenko removal".into());
                }
            }
            CommittedEvent::KrenkoSampleV1 { sample: s }
                if !matches!(
                    state.engine.event_history.get(s.removed as usize),
                    Some(CommittedEvent::KrenkoRemovedV1 { .. })
                ) =>
            {
                return Err("orphan Krenko count sample".into())
            }
            CommittedEvent::KrenkoCompletedV1 { completed: c }
                if !matches!(
                    state.engine.event_history.get(c.removed as usize),
                    Some(CommittedEvent::KrenkoRemovedV1 { .. })
                ) =>
            {
                return Err("orphan Krenko completion".into())
            }
            _ => {}
        }
    }
    Ok(())
}
pub(crate) fn requires_successor(state: &GameState) -> bool {
    state.objects.iter().any(|(id, object)| {
        object.zone == Zone::Battlefield
            && selected_definition(object.card_def)
            && crate::continuous_characteristics_v1::printed_abilities_active(state, id)
    }) || state.stack.iter().any(|i| tracked(state, i))
        || state.engine.event_history.iter().any(|e| {
            matches!(
                e,
                CommittedEvent::KrenkoPaidV1 { .. }
                    | CommittedEvent::KrenkoRemovedV1 { .. }
                    | CommittedEvent::KrenkoSampleV1 { .. }
                    | CommittedEvent::KrenkoCompletedV1 { .. }
                    | CommittedEvent::KrenkoRemovalInputV1 { .. }
                    | CommittedEvent::KrenkoPaymentInputV1 { .. }
            )
        })
}

#[cfg(test)]
#[path = "krenko_v1/tests.rs"]
mod tests;
