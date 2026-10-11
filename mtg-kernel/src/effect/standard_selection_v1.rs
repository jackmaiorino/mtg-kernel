//! Public choices made while an effect resolves, with incarnation-bound candidates.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ObjectSelectionFilterV1 {
    Creature,
    CreatureOrPlaneswalker,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ObjectSelectionActionV1 {
    MoveTo(Zone),
    CountersAndKeyword { counters: u8, keyword: Keywords },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ObjectSelectionRuleV1 {
    pub player: PlayerRef,
    pub zone: Zone,
    pub any_player: bool,
    pub filter: ObjectSelectionFilterV1,
    pub min: u8,
    pub max: u8,
    pub action: ObjectSelectionActionV1,
}

fn candidates(
    state: &GameState,
    player: PlayerId,
    rule: ObjectSelectionRuleV1,
) -> Result<Vec<EffectObjectBinding>, String> {
    if !matches!(rule.zone, Zone::Battlefield | Zone::Graveyard) || rule.min > rule.max {
        return Err("object selection requires a public zone and ordered bounds".into());
    }
    let mut candidates = Vec::new();
    for owner in [PlayerId::P0, PlayerId::P1] {
        if !rule.any_player && owner != player {
            continue;
        }
        let objects = if rule.zone == Zone::Battlefield {
            &state.players[owner.index()].battlefield
        } else {
            &state.players[owner.index()].graveyard
        };
        for &id in objects {
            let live = state.objects.get(id);
            let creature = crate::engine::object_has_type(state, id, CardType::Creature);
            let matches = match rule.filter {
                ObjectSelectionFilterV1::Creature => creature,
                ObjectSelectionFilterV1::CreatureOrPlaneswalker => {
                    creature || crate::engine::object_has_type(state, id, CardType::Planeswalker)
                }
            };
            if matches {
                candidates.push(EffectObjectBinding {
                    object: id,
                    expected_zone: rule.zone,
                    expected_zone_change_count: live.zone_change_count,
                });
            }
        }
    }
    Ok(candidates)
}

fn validate_origin(
    state: &GameState,
    pending: &EffectContinuation,
    rule: ObjectSelectionRuleV1,
    path: &[u16],
) -> Result<(), String> {
    let root = validated_definition_owned_root_effect(state, pending)?;
    if !matches!(effect_op_at_structural_path(&root, path), Some(EffectOp::SelectObjectsV1 { rule: original }) if *original == rule)
    {
        return Err("object selection lost its definition-owned operation".into());
    }
    Ok(())
}

pub(super) fn begin(
    state: &GameState,
    pending: &mut EffectContinuation,
    rule: ObjectSelectionRuleV1,
    path: Vec<u16>,
) -> Result<(), String> {
    let player = pending.ctx.resolve_player(rule.player, state);
    let original_candidates = candidates(state, player, rule)?;
    let available = u16::try_from(original_candidates.len()).unwrap_or(u16::MAX);
    pending.choice = Some(PendingEffectChoice::SelectTargets {
        player,
        path: path.clone(),
        selected: Vec::new(),
        legal: original_candidates
            .iter()
            .copied()
            .map(|binding| EffectTargetCandidate {
                target: Target::Object(binding.object),
                expected_object: Some(binding),
            })
            .collect(),
        min_targets: u16::from(rule.min).min(available),
        max_targets: u16::from(rule.max).min(available),
        ordered: false,
        purpose: EffectTargetSelectionPurpose::SelectObjectsV1 {
            rule,
            original_candidates,
            canonical_path: path,
        },
    });
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn validate_prompt(
    state: &GameState,
    pending: &EffectContinuation,
    rule: ObjectSelectionRuleV1,
    original: &[EffectObjectBinding],
    canonical_path: &[u16],
    chooser: PlayerId,
    path: &[u16],
    selected: &[EffectTargetCandidate],
    legal: &[EffectTargetCandidate],
    min: u16,
    max: u16,
    ordered: bool,
) -> Result<(), String> {
    validate_origin(state, pending, rule, path)?;
    let player = pending.ctx.resolve_player(rule.player, state);
    let current = candidates(state, player, rule)?;
    let available = u16::try_from(current.len()).unwrap_or(u16::MAX);
    if current != original
        || chooser != player
        || path != canonical_path
        || ordered
        || min != u16::from(rule.min).min(available)
        || max != u16::from(rule.max).min(available)
    {
        return Err("object selection prompt metadata changed".into());
    }
    let all = selected
        .iter()
        .chain(legal)
        .map(|candidate| {
            candidate
                .expected_object
                .ok_or_else(|| "object selection missing incarnation".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate_exact_binding_permutation(original, &all, "object selection candidates")
}

pub(super) fn apply_selection(
    state: &mut GameState,
    pending: &EffectContinuation,
    rule: ObjectSelectionRuleV1,
    original: &[EffectObjectBinding],
    selected: &[EffectObjectBinding],
    path: &[u16],
) -> Result<(), String> {
    validate_origin(state, pending, rule, path)?;
    let player = pending.ctx.resolve_player(rule.player, state);
    if candidates(state, player, rule)? != original
        || selected.len() < usize::from(rule.min).min(original.len())
        || selected.len() > usize::from(rule.max).min(original.len())
    {
        return Err("object selection answer changed".into());
    }
    canonicalize_binding_subset(original, selected)?;
    match rule.action {
        ObjectSelectionActionV1::MoveTo(zone) => event::propose_and_commit_batch(
            state,
            selected
                .iter()
                .map(|binding| event::ProposedEvent::zone_change(binding.object, zone))
                .collect(),
        ),
        ObjectSelectionActionV1::CountersAndKeyword { counters, keyword } => {
            for binding in selected {
                event::add_plus_one_counters(
                    state,
                    binding.object,
                    pending.ctx.controller,
                    i32::from(counters),
                )?;
                let timestamp = crate::engine::next_timestamp(state);
                state.engine.until_end_of_turn.push(
                    crate::engine::UntilEndOfTurnEffect::ResolvedObjectKeywordEffect {
                        object_id: binding.object,
                        object_zone_change_count: binding.expected_zone_change_count,
                        layer: crate::engine::Layers::ABILITY_ADDING,
                        timestamp,
                        duration: crate::engine::EffectDuration::EndOfTurn,
                        keywords: keyword,
                    },
                );
            }
        }
    }
    Ok(())
}

/// Six pairs preserve Gix's printed mode order; the caster selects a pair.
pub fn gix_command_pair(pair: u8) -> EffectOp {
    let boost = EffectOp::SelectObjectsV1 {
        rule: ObjectSelectionRuleV1 {
            player: PlayerRef::Controller,
            zone: Zone::Battlefield,
            any_player: true,
            filter: ObjectSelectionFilterV1::Creature,
            min: 0,
            max: 1,
            action: ObjectSelectionActionV1::CountersAndKeyword {
                counters: 2,
                keyword: Keywords::LIFELINK,
            },
        },
    };
    let destroy = EffectOp::DestroyCreaturesPowerAtMostV1 { power: 2 };
    let restore = EffectOp::SelectObjectsV1 {
        rule: ObjectSelectionRuleV1 {
            player: PlayerRef::Controller,
            zone: Zone::Graveyard,
            any_player: false,
            filter: ObjectSelectionFilterV1::Creature,
            min: 0,
            max: 2,
            action: ObjectSelectionActionV1::MoveTo(Zone::Hand),
        },
    };
    let sacrifice = EffectOp::SacrificeCreature {
        player: PlayerRef::Opponent,
        filter: CreatureSacrificeFilter::GreatestPower,
    };
    let modes = [boost, destroy, restore, sacrifice];
    let (a, b) = [(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)][usize::from(pair)];
    EffectOp::Sequence(vec![modes[a].clone(), modes[b].clone()])
}

pub fn gix_pair_1() -> EffectOp {
    gix_command_pair(1)
}
pub fn gix_pair_2() -> EffectOp {
    gix_command_pair(2)
}
pub fn gix_pair_3() -> EffectOp {
    gix_command_pair(3)
}
pub fn gix_pair_4() -> EffectOp {
    gix_command_pair(4)
}
pub fn gix_pair_5() -> EffectOp {
    gix_command_pair(5)
}
