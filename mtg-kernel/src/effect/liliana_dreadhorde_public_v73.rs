//! Narrow read-only projection owns access to private native payment/choice data.
use super::*;
use crate::{liliana_public_v73::*, optional_trigger_public_v1 as public};

fn root(
    s: &GameState,
    source: AbilitySourceContractV4,
    viewer: PlayerId,
) -> Result<public::RootV1, String> {
    if !definition(source.card_def) {
        return Err("V73 original native source is not Liliana".into());
    }
    let mut root = public::root(s, source, Some(viewer))?;
    // The genuine paid activation or captured printed observer was face up.
    // Preserve that public fact while the current reference remains concealed.
    root.original_source.identity = public::IdentityV1::Known { card_id: 491 };
    Ok(root)
}
fn incarnation(
    s: &GameState,
    b: EffectObjectBinding,
    controller: PlayerId,
) -> Result<IncarnationV73, String> {
    // Owner is immutable. Prefer the recorded departure if the physical token
    // has ceased to exist. Never obtain the old printing from a newer slot.
    let owner = s
        .engine
        .event_history
        .iter()
        .find_map(|e| match e {
            E::NivMizzetVisionaryDamageSourceDepartureV1 { source, .. }
                if source.source == b.object
                    && source.zone_change_count == b.expected_zone_change_count =>
            {
                Some(source.owner)
            }
            _ => None,
        })
        .or_else(|| s.objects.try_get(b.object).map(|o| o.owner))
        .ok_or("V73 original physical owner absent")?;
    Ok(IncarnationV73 {
        object: b.object.0,
        generation: b.expected_zone_change_count,
        owner: owner.into(),
        controller: controller.into(),
        zone: b.expected_zone,
    })
}
fn from_scene(
    s: &GameState,
    c: &ChoiceV1,
    b: EffectObjectBinding,
) -> Result<IncarnationV73, String> {
    let p = [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .find(|p| c.rosters[p.index()].iter().any(|(v, _)| *v == b))
        .ok_or("V73 original scene binding absent")?;
    incarnation(s, b, p)
}
fn scene_list(
    s: &GameState,
    c: &ChoiceV1,
    bs: &[EffectObjectBinding],
) -> Result<Vec<IncarnationV73>, String> {
    bs.iter().map(|b| from_scene(s, c, *b)).collect()
}
fn selection(s: &GameState, c: &ChoiceV1, occurrence: u32) -> Result<SelectionV73, String> {
    validate_choice(s, c, false)?;
    let (actor, candidates, count, ordered) = stage_data(c)?;
    let kind = match stages(c)[c.phase as usize] {
        Stage::Sacrifice(_) => SelectionKindV73::SacrificeCreatures,
        Stage::Keep(_, t) => SelectionKindV73::KeepPermanent(t),
        Stage::Order(_) => SelectionKindV73::OrderGraveyard,
    };
    let pending = s.engine.pending_effect.as_ref();
    let prefix = pending
        .and_then(|p| match p.choice.as_ref() {
            Some(PendingEffectChoice::SelectTargets {
                purpose: EffectTargetSelectionPurpose::LilianaV1 { choice },
                selected,
                ..
            }) if choice == c => Some(
                selected
                    .iter()
                    .map(|v| {
                        v.expected_object
                            .ok_or("V73 selected candidate lost incarnation")
                    })
                    .collect::<Result<Vec<_>, _>>(),
            ),
            _ => None,
        })
        .transpose()?;
    let framed = pending.and_then(|p| match p.frames.as_slice() {
        [EffectFrame::LilianaV1 { choice, selected }] if choice == c => Some(selected.clone()),
        _ => None,
    });
    let current = prefix.is_some() || framed.is_some();
    let selected_prefix = scene_list(s, c, &prefix.or(framed).unwrap_or_default())?;
    Ok(SelectionV73 {
        occurrence,
        phase: c.phase,
        actor: actor.into(),
        kind,
        candidates: scene_list(s, c, &candidates)?,
        exact_count: u16::try_from(count).map_err(|_| "V73 exact count overflow")?,
        ordered,
        selected_prefix,
        current,
    })
}
pub(crate) fn capture(s: &GameState, v: PlayerId) -> Result<FactsV73, String> {
    validate_state(s)?;
    crate::effect::validate_pending_effect_choice(s)?;
    // The committed V72 counterpart supplies every current form and all eighteen
    // native-width counters. It remains an independent read-only appendix.
    let battlefield = crate::tinybones_public_v72::capture(s, v)?.battlefield;
    let authorized_printings = s
        .objects
        .iter()
        .filter_map(|(id, _)| public::reference(s, id, Some(v)))
        .filter(|r| matches!(r.identity, public::IdentityV1::Known { card_id: 491 | 492 }))
        .collect();
    let mut out = FactsV73 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        viewer: v.into(),
        active_player: s.active_player.into(),
        turn_key: s.turn_key_v1(),
        meaning: crate::rules_meaning_liliana_v73::printed_meaning()?,
        authorized_printings,
        battlefield,
        activations: vec![],
        resolutions: vec![],
        deaths: vec![],
        current_draws: vec![],
        complete_ordinary_observation: false,
        owning_consumer_admitted: false,
    };
    let mut paid = std::collections::BTreeMap::new();
    for (_, j) in rows(s) {
        let JournalV1::Payment { input } = j else {
            continue;
        };
        let occurrence = u32::try_from(out.activations.len())
            .map_err(|_| "V73 activation occurrence overflow")?;
        paid.insert(input.index, occurrence);
        out.activations.push(ActivationV73 {
            occurrence,
            source: root(s, input.source, v)?,
            turn: input.turn,
            ability_index: input.pending.ability_index,
            loyalty_before: input.before,
            loyalty_after: input.after,
            stage: ActivationStageV73::Paid,
            created_zombies: vec![],
        });
    }
    for (_, j) in rows(s) {
        let JournalV1::Published { input, item } = j else {
            continue;
        };
        let occurrence = *paid
            .get(&input.index)
            .ok_or("V73 publication lost payment")?;
        let activation = &mut out.activations[occurrence as usize];
        activation.stage = if s.stack.iter().any(|r| r == item.as_ref()) {
            ActivationStageV73::Published
        } else if s
            .engine
            .pending_effect
            .as_ref()
            .is_some_and(|p| p.resolving_item == **item)
        {
            ActivationStageV73::Resolving
        } else if crate::engine::time_stop_v1::disposed(s, item)? {
            ActivationStageV73::Disposed
        } else {
            ActivationStageV73::NoLongerPublished
        };
        for (start, j) in rows(s) {
            match j {
                JournalV1::TokenInput {
                    item: token_item, ..
                } if token_item == item => {
                    if let Some(end) = rows(s).find_map(|(_, j)| match j {
                        JournalV1::TokenFinished { input, history_end }
                            if *input == start as u64 =>
                        {
                            Some(*history_end as usize)
                        }
                        _ => None,
                    }) {
                        activation.stage = ActivationStageV73::NativeCompleted;
                        activation.created_zombies = s.engine.event_history[start + 1..end]
                            .iter()
                            .filter_map(|e| match e {
                                E::CreateToken {
                                    object,
                                    token_def: 492,
                                    controller,
                                    ..
                                } => Some(IncarnationV73 {
                                    object: object.0,
                                    generation: 0,
                                    owner: (*controller).into(),
                                    controller: (*controller).into(),
                                    zone: Zone::Battlefield,
                                }),
                                _ => None,
                            })
                            .collect();
                    }
                }
                JournalV1::Started {
                    item: started,
                    choice: first,
                } if started == item => {
                    let mut final_choice = first.as_ref();
                    let mut finished = false;
                    let mut completed_sacrifices = vec![];
                    let mut selections = vec![];
                    for (at, j) in rows(s) {
                        match j {
                            JournalV1::Issued {
                                start: original,
                                choice,
                            } if *original == start as u64 => {
                                selections.push(selection(
                                    s,
                                    choice,
                                    u32::try_from(selections.len())
                                        .map_err(|_| "V73 selection overflow")?,
                                )?);
                                final_choice = choice;
                            }
                            JournalV1::CommitInput {
                                start: original,
                                choice,
                                sacrificed,
                            } if *original == start as u64 => {
                                final_choice = choice;
                                finished=rows(s).any(|(_,j)|matches!(j,JournalV1::Finished{input,..}if *input==at as u64));
                                if finished {
                                    completed_sacrifices = scene_list(s, choice, sacrificed)?;
                                }
                            }
                            _ => {}
                        }
                    }
                    if finished {
                        activation.stage = ActivationStageV73::NativeCompleted;
                    }
                    let c = final_choice;
                    let roster = |p: PlayerId| {
                        c.rosters[p.index()]
                            .iter()
                            .map(|(b, ts)| {
                                Ok(SceneObjectV73 {
                                    original: incarnation(s, *b, p)?,
                                    permanent_types: ts.clone(),
                                    current_original: s
                                        .objects
                                        .try_get(b.object)
                                        .filter(|o| {
                                            o.zone == b.expected_zone
                                                && o.zone_change_count
                                                    == b.expected_zone_change_count
                                        })
                                        .and_then(|_| public::reference(s, b.object, Some(v))),
                                })
                            })
                            .collect::<Result<Vec<_>, String>>()
                    };
                    out.resolutions.push(ResolutionV73 {
                        activation: occurrence,
                        active_player: c.active.into(),
                        controller: c.controller.into(),
                        ultimate: c.ultimate,
                        rosters: [roster(PlayerId::P0)?, roster(PlayerId::P1)?],
                        selections,
                        chosen_sacrifices: [
                            scene_list(s, c, &c.sacrificed[0])?,
                            scene_list(s, c, &c.sacrificed[1])?,
                        ],
                        keepers: c
                            .kept
                            .iter()
                            .map(|(t, b)| {
                                Ok(KeeperV73 {
                                    permanent_type: *t,
                                    permanent: from_scene(s, c, *b)?,
                                })
                            })
                            .collect::<Result<Vec<_>, String>>()?,
                        graveyard_orders: [
                            scene_list(s, c, &c.orders[0])?,
                            scene_list(s, c, &c.orders[1])?,
                        ],
                        completed_sacrifices,
                        finished,
                    });
                }
                _ => {}
            }
        }
    }
    for (at, e) in s.engine.event_history.iter().enumerate() {
        let E::ControlledCreatureDeathCaptureV1 {
            source,
            ability_index,
            died,
            controller_before,
        } = e
        else {
            continue;
        };
        if !definition(source.card_def) {
            continue;
        }
        if *ability_index != 0 || source.controller != *controller_before {
            return Err("V73 printed controlled-death observer changed".into());
        }
        crate::creature_death_capture_v1::validated_capture_death(s, at)?;
        let collected=s.engine.event_history.iter().filter(|e|matches!(e,E::ControlledCreatureDeathCaptureCollectedV1{capture_history_index}if *capture_history_index==at as u64)).count();
        if collected > 1 {
            return Err("V73 death capture collected more than once".into());
        }
        if collected == 0 {
            let matching = s
                .engine
                .event_log
                .iter()
                .enumerate()
                .filter(|(_, actual)| *actual == e)
                .map(|(i, _)| i)
                .collect::<Vec<_>>();
            let [i] = matching.as_slice() else {
                return Err("V73 uncollected native death capture absent or duplicated".into());
            };
            crate::creature_death_capture_v1::pending(s, &s.engine.event_log, *i)
                .map_err(|_| "V73 original death capture invalid")?;
        }
        out.deaths.push(DeathV73 {
            occurrence: u32::try_from(out.deaths.len())
                .map_err(|_| "V73 death occurrence overflow")?,
            observer: root(s, *source, v)?,
            creature: incarnation(
                s,
                EffectObjectBinding {
                    object: died.object,
                    expected_zone: Zone::Battlefield,
                    expected_zone_change_count: died.zone_change_count,
                },
                *controller_before,
            )?,
            collected: collected == 1,
        });
    }
    for (i, t) in public::held(s).into_iter().enumerate() {
        if t.source_contract.is_some_and(|a| definition(a.card_def)) && t.effect == death_draw() {
            out.current_draws.push(DrawV73 {
                source: root(
                    s,
                    t.source_contract.ok_or("V73 held draw source absent")?,
                    v,
                )?,
                stage: DrawStageV73::Held,
                native_position: u32::try_from(i).map_err(|_| "V73 held position overflow")?,
            });
        }
    }
    for (i, r) in s.stack.iter().enumerate() {
        if r.kind == StackItemKind::TriggeredAbility
            && r.v4
                .ability_source_contract
                .is_some_and(|a| definition(a.card_def))
            && r.inline_effect == Some(death_draw())
        {
            out.current_draws.push(DrawV73 {
                source: root(
                    s,
                    r.v4.ability_source_contract
                        .ok_or("V73 published draw source absent")?,
                    v,
                )?,
                stage: DrawStageV73::Published,
                native_position: u32::try_from(i).map_err(|_| "V73 stack position overflow")?,
            });
        }
    }
    if let Some(p) = &s.engine.pending_effect {
        let r = &p.resolving_item;
        if r.kind == StackItemKind::TriggeredAbility
            && r.v4
                .ability_source_contract
                .is_some_and(|a| definition(a.card_def))
            && r.inline_effect == Some(death_draw())
        {
            out.current_draws.push(DrawV73 {
                source: root(
                    s,
                    r.v4.ability_source_contract
                        .ok_or("V73 resolving draw source absent")?,
                    v,
                )?,
                stage: DrawStageV73::Resolving,
                native_position: 0,
            });
        }
    }
    // Native draw leaves do not store a per-death ordinal. Retain their actual
    // source/position and bound multiplicity by collected original captures.
    for draw in &out.current_draws {
        let captures = out
            .deaths
            .iter()
            .filter(|d| d.collected && d.observer == draw.source)
            .count();
        let current = out
            .current_draws
            .iter()
            .filter(|d| d.source == draw.source)
            .count();
        if current > captures {
            return Err("V73 current draw multiplicity exceeds captured original deaths".into());
        }
    }
    Ok(out)
}
