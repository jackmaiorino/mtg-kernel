use super::surgical_extraction;
use crate::card_def::{TargetSpec, CARD_DEFS};
use crate::effect::{
    EffectContinuation, EffectFrame, EffectObjectBinding, EffectTargetSelectionPurpose, ExecCtx,
    PendingEffectChoice,
};
use crate::eternal_cards_v1::e3::{self, E3ChoiceKindV1, E3ChoiceV1};
use crate::event::{self, ProposedEvent};
use crate::ids::PlayerId;
use crate::standard_cards_v1::StandardTargetV1;
use crate::state::{GameState, StackItemKind, Target, Zone};
use std::collections::BTreeSet;

struct SearchV1 {
    owner: PlayerId,
    matches: Vec<EffectObjectBinding>,
}

/// Names the caster may inspect while searching the owner's hidden zones.
/// A library search confers no position knowledge after the shuffle.
#[derive(Debug, PartialEq, Eq)]
pub struct SurgicalSearchLookV1 {
    pub owner: PlayerId,
    pub hand_printings: Vec<u16>,
    pub library_printings: Vec<u16>,
}

fn active_choice(state: &GameState) -> Option<(&EffectContinuation, &E3ChoiceV1)> {
    let pending = state.engine.pending_effect.as_ref()?;
    let Some(PendingEffectChoice::SelectTargets {
        purpose: EffectTargetSelectionPurpose::EternalV1 { choice }, ..
    }) = pending.choice.as_ref()
    else {
        return None;
    };
    choice.is_surgical_extraction().then_some((pending, choice))
}

pub(crate) fn search_active(state: &GameState) -> bool {
    active_choice(state).is_some()
        || state.engine.pending_effect.as_ref().is_some_and(|pending| {
            pending.frames.iter().any(|frame| {
                matches!(frame, EffectFrame::EternalSelectionV1 { choice, .. }
                    if choice.is_surgical_extraction())
            })
        })
}

pub fn surgical_search_look_v1(
    state: &GameState,
    viewer: PlayerId,
) -> Result<Option<SurgicalSearchLookV1>, String> {
    let Some((pending, choice)) = active_choice(state) else {
        return Ok(None);
    };
    if choice.player != viewer || pending.resolving_item.controller != viewer {
        return Ok(None);
    }
    crate::effect::validate_pending_effect_choice(state)?;
    let SearchV1 { owner, .. } = search(state, &pending.ctx)?
        .ok_or("Surgical search look lost its legal target")?;
    let seat = &state.players[owner.index()];
    let printings = |cards: &[crate::ids::ObjectId]| {
        let mut result: Vec<_> = cards.iter().map(|&card| state.objects.get(card).card_def).collect();
        result.sort_unstable();
        result
    };
    Ok(Some(SurgicalSearchLookV1 {
        owner,
        hand_printings: printings(&seat.hand),
        library_printings: printings(&seat.library),
    }))
}

fn search(state: &GameState, ctx: &ExecCtx) -> Result<Option<SearchV1>, String> {
    let [Target::Object(target)] = ctx.targets.as_slice() else {
        return Ok(None);
    };
    let Some(live) = state.objects.try_get(*target) else {
        return Ok(None);
    };
    let spec = TargetSpec::StandardV1(StandardTargetV1::NonbasicLandCardInAGraveyardV1);
    if !ctx.target_incarnation_matches(0, state)
        || !crate::engine::effect_target_is_legal(
            state,
            ctx.source,
            ctx.controller,
            spec,
            &ctx.targets,
            0,
        )
    {
        return Ok(None);
    }
    let name = CARD_DEFS.get(usize::from(live.card_def))
        .ok_or("Surgical target has no registered printing")?.object_name;
    let owner = live.owner;
    let seat = state.players.get(owner.index()).ok_or("Surgical target has no valid owner")?;
    let mut visited = BTreeSet::new();
    let mut matches = Vec::new();
    for (zone, cards) in [(Zone::Graveyard, &seat.graveyard), (Zone::Hand, &seat.hand), (Zone::Library, &seat.library)] {
        for &object in cards {
            let card = state.objects.try_get(object).ok_or("Surgical search lost an original card")?;
            let printing = CARD_DEFS.get(usize::from(card.card_def)).ok_or("Surgical search has an unregistered printing")?;
            if !visited.insert(object) || card.owner != owner || card.zone != zone {
                return Err("Surgical search changed original zone membership".into());
            }
            if !card.v4.is_token && card.spell_copy_origin.is_none() && printing.object_name == name {
                matches.push(EffectObjectBinding {
                    object,
                    expected_zone: zone,
                    expected_zone_change_count: card.zone_change_count,
                });
            }
        }
    }
    if !matches.iter().any(|card| card.object == *target) {
        return Err("Surgical search lost its original graveyard target".into());
    }
    Ok(Some(SearchV1 { owner, matches }))
}

fn prompt(
    state: &GameState,
    continuation: &EffectContinuation,
    path: Vec<u16>,
) -> Result<Option<(PlayerId, E3ChoiceV1)>, String> {
    let Some(SearchV1 { owner, matches }) = search(state, &continuation.ctx)? else {
        return Ok(None);
    };
    let max = u16::try_from(matches.len()).map_err(|_| "Surgical search exceeds choice width")?;
    state.preflight_library_shuffle(owner).map_err(|e| e.to_string())?;
    Ok(Some((
        owner,
        E3ChoiceV1 {
            kind: E3ChoiceKindV1::SurgicalExtractionV1,
            player: continuation.ctx.controller,
            candidates: matches.iter().map(|card| Target::Object(card.object)).collect(),
            bindings: matches,
            min: 0,
            max,
            ordered: false,
            path,
            remaining: Vec::new(),
        },
    )))
}

pub(crate) fn drive(
    state: &GameState,
    continuation: &mut EffectContinuation,
    path: Vec<u16>,
) -> Result<bool, String> {
    let Some((_, choice)) = prompt(state, continuation, path)? else {
        return Ok(false);
    };
    e3::offer(continuation, choice);
    Ok(true)
}

pub(crate) fn validate_continuation(
    state: &GameState,
    pending: &EffectContinuation,
) -> Result<(), String> {
    let item = &pending.resolving_item;
    let selected = item.kind == StackItemKind::Spell
        && item.v4.source_contract.is_some_and(|contract| {
            CARD_DEFS.get(usize::from(contract.card_def)).is_some_and(|def| def.name == "Surgical Extraction")
        });
    let attached = search_active(state);
    if !selected && !attached {
        return Ok(());
    }
    if !selected
        || pending.ctx.source != item.source
        || pending.ctx.controller != item.controller
        || pending.ctx.stack_item_id != Some(item.v4.stack_item_id)
        || pending.ctx.targets != item.targets
        || pending.ctx.target_contracts != item.v4.target_contracts
    {
        return Err("Surgical search lost its original spell and target".into());
    }
    match (&pending.choice, pending.frames.as_slice()) {
        (None, [EffectFrame::Program { op, path }])
            if path.is_empty() && *op == surgical_extraction() => Ok(()),
        (Some(PendingEffectChoice::SelectTargets {
            purpose: EffectTargetSelectionPurpose::EternalV1 { choice },
            selected, legal, min_targets: 0, ..
        }), []) => {
            let (_, expected) = prompt(state, pending, Vec::new())?
                .ok_or("Surgical search lost its legal target")?;
            if **choice != expected || !legal.iter().map(|candidate| candidate.target).eq(
                expected.candidates.iter().copied().filter(|target| {
                    !selected.iter().any(|candidate| candidate.target == *target)
                }),
            ) {
                return Err("Surgical search changed its matching roster or action order".into());
            }
            Ok(())
        }
        (None, [EffectFrame::EternalSelectionV1 { choice, .. }]) => {
            let (_, expected) = prompt(state, pending, Vec::new())?
                .ok_or("Surgical search lost its legal target")?;
            if **choice != expected {
                return Err("Surgical search answer changed its original roster".into());
            }
            Ok(())
        }
        _ => Err("Surgical search lost its native resolution stage".into()),
    }
}

pub(crate) fn finish(
    state: &mut GameState,
    continuation: &EffectContinuation,
    choice: &E3ChoiceV1,
    picked: &[EffectObjectBinding],
) -> Result<bool, String> {
    let (owner, original) = prompt(state, continuation, Vec::new())?
        .ok_or("Surgical search lost its legal target")?;
    if original != *choice || picked.iter().any(|card| !original.bindings.contains(card)) {
        return Err("Surgical search answer is outside its original roster".into());
    }
    let shuffle = state.preflight_library_shuffle(owner).map_err(|e| e.to_string())?;
    event::propose_and_commit_batch(
        state,
        original.bindings.iter().filter(|card| picked.contains(card))
            .map(|card| ProposedEvent::zone_change(card.object, Zone::Exile)).collect(),
    );
    state.commit_library_shuffle(owner, shuffle).map_err(|e| e.to_string())?;
    Ok(false)
}
