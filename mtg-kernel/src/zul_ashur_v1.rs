//! Exact selected Zul Ashur activation custody and ordinary graveyard permission.
use crate::card_def::{CardType, Subtype, CARD_DEFS};
use crate::effect::EffectOp;
use crate::event::CommittedEvent;
use crate::ids::{ObjectId, PlayerId, StackItemId};
use crate::state::{GameState, StackItem, StackItemKind, StackTargetContractV4, Target, Zone};
use serde::{Deserialize, Serialize};

pub fn recipe() -> EffectOp {
    EffectOp::StandardV1(crate::standard_cards_v1::StandardOpV1::ZulAshurPermissionV1)
}
pub(crate) fn owns_definition(id: u16) -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && CARD_DEFS
        .get(usize::from(id))
        .is_some_and(|d| d.name == "Zul Ashur, Lich Lord" && d.is_executable())
}
fn selected(item: &StackItem) -> bool {
    item.kind == StackItemKind::ActivatedAbility
        && item.v4.granted_by.is_none()
        && item.v4.cauldron_grant.is_none()
        && item
            .v4
            .ability_source_contract
            .is_some_and(|s| owns_definition(s.card_def))
        && item.v4.activated_ability_index == Some(0)
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PublicationV1 {
    pub index: u32,
    pub payment: u32,
    pub item: Box<StackItem>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OutcomeV1 {
    Resolved,
    NoLegalTarget,
    Countered,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RemovalV1 {
    pub index: u32,
    pub publication: u32,
    pub item: Box<StackItem>,
    pub outcome: OutcomeV1,
    pub target_legal: bool,
    pub turn: u32,
    pub active_player: PlayerId,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InstallationV1 {
    pub index: u32,
    pub removal: u32,
    pub target: StackTargetContractV4,
    pub holder: PlayerId,
    pub turn: u32,
    pub active_player: PlayerId,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CleanupV1 {
    pub index: u32,
    pub occurrence: u32,
    pub turn: u32,
    pub active_player: PlayerId,
    pub installations: Vec<u32>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExpiryV1 {
    pub installation: u32,
    pub cleanup: u32,
}
/// The loan can only be acquired against the genuine sole live published row.
pub(crate) struct OwnerV1 {
    item: StackItem,
    publication: u32,
}
fn index(state: &GameState) -> Result<u32, String> {
    u32::try_from(state.engine.event_history.len()).map_err(|_| "Zul history exhausted".into())
}
fn published(state: &GameState, id: StackItemId) -> Result<&PublicationV1, String> {
    let rows: Vec<_> = state
        .engine
        .event_history
        .iter()
        .filter_map(|e| match e {
            CommittedEvent::ZulAshurPublishedV1 { publication }
                if publication.item.v4.stack_item_id == id =>
            {
                Some(publication.as_ref())
            }
            _ => None,
        })
        .collect();
    match rows.as_slice() {
        [row] => Ok(row),
        _ => Err("Zul publication missing or duplicated".into()),
    }
}
fn metadata(state: &GameState, item: &StackItem) -> Result<(), String> {
    let source = item
        .v4
        .ability_source_contract
        .ok_or("Zul source missing")?;
    crate::engine::validate_historical_ability_source_contract(state, source)?;
    let definition = CARD_DEFS
        .get(usize::from(source.card_def))
        .ok_or("Zul definition missing")?;
    let ability = definition
        .activated_abilities
        .first()
        .ok_or("Zul ability missing")?;
    if !selected(item)
        || item.source != source.source
        || item.controller != source.controller
        || ability.cost != [crate::card_def::CostComponent::Tap]
        || ability.activation_target_filter
            != crate::card_def::ActivationTargetFilter::CreatureSubtype(Subtype::Zombie)
        || ability.activation_zone != Zone::Battlefield
        || ability.sorcery_speed_only
        || ability.max_activations_per_turn.is_some()
        || item.inline_effect != Some(recipe())
        || (ability.effect)() != recipe()
        || item.targets.len() != 1
        || item.v4.target_contracts.len() != 1
        || item.v4.target_spec != Some(crate::card_def::TargetSpec::CreatureCardInOwnGraveyard)
        || !matches!(item.v4.target_contracts[0], StackTargetContractV4::Object {
            object, owner, zone: Zone::Graveyard, .. } if item.targets[0] == Target::Object(object) && owner == item.controller)
    {
        return Err("Zul printed activation changed".into());
    }
    Ok(())
}
pub(crate) fn target_legal(state: &GameState, item: &StackItem) -> bool {
    matches!(item.v4.target_contracts.first(), Some(StackTargetContractV4::Object {
        object, card_def, owner, zone: Zone::Graveyard, zone_change_count, spell_copy_origin, ..
    }) if *owner == item.controller && item.targets == [Target::Object(*object)]
        && state.objects.try_get(*object).is_some_and(|live| live.zone == Zone::Graveyard
            && live.owner == *owner && live.card_def == *card_def && live.zone_change_count == *zone_change_count
            && live.spell_copy_origin == *spell_copy_origin
            && state.players[owner.index()].graveyard.contains(object)
            && CARD_DEFS[usize::from(live.card_def)].types.contains(&CardType::Creature)
            && CARD_DEFS[usize::from(live.card_def)].subtypes.contains(&Subtype::Zombie)))
}
pub(crate) fn publish(state: &mut GameState, id: StackItemId) -> Result<(), String> {
    let Some(item) = state
        .stack
        .iter()
        .find(|i| i.v4.stack_item_id == id)
        .filter(|i| selected(i))
        .cloned()
    else {
        return Ok(());
    };
    metadata(state, &item)?;
    if state.engine.event_history.iter().any(|e| matches!(e, CommittedEvent::ZulAshurPublishedV1{publication} if publication.item.v4.stack_item_id==id)) {
        return Err("Zul publication replay".into());
    }
    let payment = index(state)?
        .checked_sub(1)
        .ok_or("Zul actual Tap payment absent")?;
    if !matches!(state.engine.event_history.get(payment as usize), Some(CommittedEvent::Tap{object}) if *object==item.source)
    {
        return Err("Zul actual Tap payment absent".into());
    }
    let publication = PublicationV1 {
        index: index(state)?,
        payment,
        item: Box::new(item),
    };
    state
        .engine
        .event_history
        .push(CommittedEvent::ZulAshurPublishedV1 {
            publication: Box::new(publication),
        });
    Ok(())
}
pub(crate) fn acquire(state: &GameState, item: &StackItem) -> Result<Option<OwnerV1>, String> {
    if !selected(item) && !state.engine.event_history.iter().any(|e| matches!(e,
        CommittedEvent::ZulAshurPublishedV1 { publication } if publication.item.v4.stack_item_id==item.v4.stack_item_id))
    { return Ok(None); }
    validate_state(state)?;
    let publication = published(state, item.v4.stack_item_id)?;
    if publication.item.as_ref() != item || state.stack.iter().filter(|i| *i == item).count() != 1 {
        return Err("Zul loan lost genuine live row".into());
    }
    Ok(Some(OwnerV1 {
        item: item.clone(),
        publication: publication.index,
    }))
}
pub(crate) fn remove_owned(
    state: &mut GameState,
    owner: OwnerV1,
    outcome: OutcomeV1,
) -> Result<(), String> {
    if state
        .stack
        .iter()
        .any(|i| i.v4.stack_item_id == owner.item.v4.stack_item_id)
        || published(state, owner.item.v4.stack_item_id)?.item.as_ref() != &owner.item
    {
        return Err("Zul removal did not remove original row".into());
    }
    let legal = target_legal(state, &owner.item);
    if (outcome == OutcomeV1::Resolved && !legal) || (outcome == OutcomeV1::NoLegalTarget && legal)
    {
        return Err("Zul outcome changed original target legality".into());
    }
    let removal = RemovalV1 {
        index: index(state)?,
        publication: owner.publication,
        item: Box::new(owner.item.clone()),
        outcome,
        target_legal: legal,
        turn: state.turn_key_v1(),
        active_player: state.active_player,
    };
    let at = removal.index;
    let turn = removal.turn;
    let active_player = removal.active_player;
    state
        .engine
        .event_history
        .push(CommittedEvent::ZulAshurRemovedV1 {
            removal: Box::new(removal),
        });
    if outcome == OutcomeV1::Resolved {
        let installation = InstallationV1 {
            index: index(state)?,
            removal: at,
            target: owner.item.v4.target_contracts[0],
            holder: owner.item.controller,
            turn,
            active_player,
        };
        state
            .engine
            .event_history
            .push(CommittedEvent::ZulAshurInstalledV1 {
                installation: Box::new(installation),
            });
    }
    Ok(())
}
fn installations(state: &GameState) -> impl Iterator<Item = &InstallationV1> {
    state.engine.event_history.iter().filter_map(|e| match e {
        CommittedEvent::ZulAshurInstalledV1 { installation } => Some(installation.as_ref()),
        _ => None,
    })
}
fn expiry_count(state: &GameState, installation: u32) -> usize {
    state
        .engine
        .event_history
        .iter()
        .filter(|e| {
            matches!(e,
        CommittedEvent::ZulAshurExpiredV1 { expiry } if expiry.installation==installation)
        })
        .count()
}
pub(crate) fn permission(
    state: &GameState,
    object: ObjectId,
    holder: PlayerId,
    generation: u32,
) -> bool {
    validate_state(state).is_ok() && installations(state).any(|i|
        i.holder==holder && i.turn==state.turn_key_v1() && i.active_player==state.active_player
            && expiry_count(state,i.index)==0 && matches!(i.target,
                StackTargetContractV4::Object {object: id, zone: Zone::Graveyard, zone_change_count, ..}
                if id==object && zone_change_count==generation))
}
pub(crate) fn has_active(state: &GameState) -> bool {
    installations(state).any(|i| expiry_count(state, i.index) == 0)
}
pub(crate) fn note_cleanup(state: &mut GameState) -> Result<(), String> {
    validate_state(state)?;
    if !has_active(state) {
        return Ok(());
    }
    if state.step != crate::state::Step::Cleanup {
        return Err("Zul Cleanup occurrence outside phase".into());
    }
    let at = index(state)?;
    state
        .engine
        .event_history
        .push(CommittedEvent::ZulAshurCleanupEnteredV1 {
            index: at,
            turn: state.turn_key_v1(),
            active_player: state.active_player,
        });
    Ok(())
}
pub(crate) fn expire(state: &mut GameState) -> Result<(), String> {
    let active: Vec<_> = installations(state)
        .filter(|i| expiry_count(state, i.index) == 0)
        .map(|i| i.index)
        .collect();
    if active.is_empty() {
        return Ok(());
    }
    if state.step != crate::state::Step::Cleanup {
        return Err("Zul expiry outside Cleanup".into());
    }
    let occurrence = index(state)?
        .checked_sub(1)
        .ok_or("Zul Cleanup occurrence missing")?;
    if !matches!(state.engine.event_history.last(), Some(CommittedEvent::ZulAshurCleanupEnteredV1 { index, turn, active_player })
        if *index==occurrence && *turn==state.turn_key_v1() && *active_player==state.active_player)
    {
        return Err("Zul expiry lacks actual phase entry".into());
    }
    let cleanup = CleanupV1 {
        index: index(state)?,
        occurrence,
        turn: state.turn_key_v1(),
        active_player: state.active_player,
        installations: active.clone(),
    };
    let at = cleanup.index;
    state
        .engine
        .event_history
        .push(CommittedEvent::ZulAshurCleanupV1 {
            cleanup: Box::new(cleanup),
        });
    for installation in active {
        state
            .engine
            .event_history
            .push(CommittedEvent::ZulAshurExpiredV1 {
                expiry: ExpiryV1 {
                    installation,
                    cleanup: at,
                },
            });
    }
    Ok(())
}
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    for item in &state.stack {
        if selected(item) {
            metadata(state, item)?;
            if published(state, item.v4.stack_item_id)?.item.as_ref() != item {
                return Err("Zul live row differs from publication".into());
            }
        }
    }
    for (at, event) in state.engine.event_history.iter().enumerate() {
        match event {
            CommittedEvent::ZulAshurPublishedV1 { publication: p } => {
                if p.index as usize != at
                    || p.payment.checked_add(1) != Some(p.index)
                    || !matches!(state.engine.event_history.get(p.payment as usize),Some(CommittedEvent::Tap{object}) if *object==p.item.source)
                {
                    return Err("Zul publication index changed".into());
                }
                metadata(state, &p.item)?;
                let original_target = p.item.v4.target_contracts[0];
                let targeting=state.engine.event_history.iter().enumerate().filter(|(position,e)| matches!(e,
                    CommittedEvent::Targeted{target,target_zone_change_count,targeting_stack_item,targeting_controller}
                    if *position>at && *targeting_stack_item==p.item.v4.stack_item_id && *targeting_controller==p.item.controller
                        && matches!(original_target,StackTargetContractV4::Object{object,zone_change_count,..} if object==*target && zone_change_count==*target_zone_change_count))).count();
                if targeting != 1 {
                    return Err("Zul original targeting occurrence missing or changed".into());
                }

                if published(state, p.item.v4.stack_item_id)?.index != p.index {
                    return Err("Zul publication duplicate".into());
                }
                let live: Vec<_> = state
                    .stack
                    .iter()
                    .filter(|i| i.v4.stack_item_id == p.item.v4.stack_item_id)
                    .collect();
                let removed: Vec<_> = state
                    .engine
                    .event_history
                    .iter()
                    .enumerate()
                    .filter_map(|(i, e)| match e {
                        CommittedEvent::ZulAshurRemovedV1 { removal: r }
                            if r.publication == p.index =>
                        {
                            Some((i, r))
                        }
                        _ => None,
                    })
                    .collect();
                match (live.as_slice(), removed.as_slice()) {
                    ([item], []) if **item == *p.item => {}
                    ([], [(removed_at, r)]) if *removed_at > at && r.item == p.item => {}
                    _ => return Err("Zul mandatory original owner missing or replayed".into()),
                }
            }
            CommittedEvent::ZulAshurRemovedV1 { removal: r } => {
                if r.index as usize != at
                    || r.publication as usize >= at
                    || !matches!(state.engine.event_history.get(r.publication as usize),
                        Some(CommittedEvent::ZulAshurPublishedV1{publication:p}) if p.item==r.item)
                    || (r.outcome == OutcomeV1::Resolved && !r.target_legal)
                    || (r.outcome == OutcomeV1::NoLegalTarget && r.target_legal)
                {
                    return Err("Zul actual removal invalid".into());
                }
                let installed: Vec<_> = installations(state)
                    .filter(|i| i.removal == r.index)
                    .collect();
                if r.outcome == OutcomeV1::Resolved {
                    if !matches!(installed.as_slice(),[i] if i.index==r.index+1 && i.target==r.item.v4.target_contracts[0]
                        && i.holder==r.item.controller && i.turn==r.turn && i.active_player==r.active_player)
                    {
                        return Err("Zul genuine resolution installation missing".into());
                    }
                } else if !installed.is_empty() {
                    return Err("Zul failed/countered removal installed permission".into());
                }
            }
            CommittedEvent::ZulAshurInstalledV1 { installation: i } => {
                if i.index as usize != at
                    || i.removal.checked_add(1) != Some(i.index)
                    || !matches!(state.engine.event_history.get(i.removal as usize),
                        Some(CommittedEvent::ZulAshurRemovedV1{removal:r}) if r.outcome==OutcomeV1::Resolved && r.target_legal
                        && r.item.v4.target_contracts[0]==i.target && r.item.controller==i.holder
                        && r.turn==i.turn && r.active_player==i.active_player)
                    || expiry_count(state, i.index) > 1
                    || (expiry_count(state, i.index) == 0
                        && (i.turn != state.turn_key_v1()
                            || i.active_player != state.active_player))
                {
                    return Err("Zul installation authority changed".into());
                }
            }
            CommittedEvent::ZulAshurCleanupV1 { cleanup: c } => {
                let expected:Vec<_>=installations(state).filter(|i|i.index<c.index && !state.engine.event_history[..at].iter().any(|e|
                    matches!(e,CommittedEvent::ZulAshurExpiredV1{expiry} if expiry.installation==i.index))).map(|i|i.index).collect();
                if c.index as usize!=at || c.occurrence.checked_add(1)!=Some(c.index)
                    || !matches!(state.engine.event_history.get(c.occurrence as usize),
                        Some(CommittedEvent::ZulAshurCleanupEnteredV1 {index,turn,active_player}) if *index==c.occurrence && *turn==c.turn && *active_player==c.active_player)
                    || expected.is_empty() || expected!=c.installations
                    || expected.iter().any(|i| !matches!(state.engine.event_history.get(*i as usize),
                        Some(CommittedEvent::ZulAshurInstalledV1{installation}) if installation.turn==c.turn && installation.active_player==c.active_player))
                {return Err("Zul Cleanup roster changed".into());}
                for (offset, i) in expected.iter().enumerate() {
                    if !matches!(state.engine.event_history.get(at+1+offset),
                        Some(CommittedEvent::ZulAshurExpiredV1{expiry}) if expiry.installation==*i && expiry.cleanup==c.index)
                    {
                        return Err("Zul Cleanup expiry missing".into());
                    }
                }
            }
            CommittedEvent::ZulAshurExpiredV1 { expiry: e } => {
                if e.cleanup as usize >= at
                    || !matches!(state.engine.event_history.get(e.cleanup as usize),
                    Some(CommittedEvent::ZulAshurCleanupV1{cleanup:c}) if c.installations.iter().position(|i|*i==e.installation)
                        .is_some_and(|offset| at==e.cleanup as usize+1+offset))
                {
                    return Err("Zul expiry lacks real Cleanup".into());
                }
            }
            CommittedEvent::ZulAshurCleanupEnteredV1 {
                index,
                turn,
                active_player,
            } => {
                if *index as usize != at
                    || !matches!(state.engine.event_history.get(at+1), Some(CommittedEvent::ZulAshurCleanupV1{cleanup})
                    if cleanup.occurrence==*index && cleanup.turn==*turn && cleanup.active_player==*active_player)
                {
                    return Err("Zul Cleanup occurrence orphaned".into());
                }
            }
            _ => {}
        }
    }
    Ok(())
}
pub(crate) fn requires_successor(state: &GameState) -> bool {
    state
        .objects
        .iter()
        .any(|(_, o)| owns_definition(o.card_def))
        || state.engine.event_history.iter().any(|e| {
            matches!(
                e,
                CommittedEvent::ZulAshurPublishedV1 { .. }
                    | CommittedEvent::ZulAshurRemovedV1 { .. }
                    | CommittedEvent::ZulAshurInstalledV1 { .. }
                    | CommittedEvent::ZulAshurCleanupV1 { .. }
                    | CommittedEvent::ZulAshurExpiredV1 { .. }
                    | CommittedEvent::ZulAshurCleanupEnteredV1 { .. }
            )
        })
}

#[cfg(test)]
#[path = "zul_ashur_v1_tests.rs"]
mod tests;
