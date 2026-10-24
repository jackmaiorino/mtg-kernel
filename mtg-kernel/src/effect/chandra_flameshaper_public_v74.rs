//! Immutable access to private native payment/exile journals. Public ordinals
//! never grant an interpreter, replacement, or copied-token execution permit.
use super::*;
use crate::{chandra_public_v74::*, optional_trigger_public_v1 as public};
fn root(
    s: &GameState,
    source: AbilitySourceContractV4,
    v: PlayerId,
) -> Result<public::RootV1, String> {
    let mut r = public::root(s, source, Some(v))?;
    // These native activation/copy-entry/end-step captures were public. Preserve
    // that original identity while leaving the current reference concealed.
    r.original_source.identity = public::IdentityV1::Known {
        card_id: source.card_def,
    };
    Ok(r)
}
fn current(s: &GameState, b: EffectObjectBinding, v: PlayerId) -> Option<public::ReferenceV1> {
    s.objects
        .try_get(b.object)
        .filter(|o| {
            o.zone == b.expected_zone && o.zone_change_count == b.expected_zone_change_count
        })
        .and_then(|_| public::reference(s, b.object, Some(v)))
}
fn targets(
    s: &GameState,
    p: &crate::engine::PendingActivation,
    source: AbilitySourceContractV4,
    v: PlayerId,
) -> Result<Vec<TargetV74>, String> {
    if p.target_contracts.len() != p.targets_chosen.len() {
        return Err("V74 original target vector changed".into());
    }
    if p.chandra_damage_v74
        .as_ref()
        .is_some_and(|d| d.allocations.len() != p.targets_chosen.len())
    {
        return Err("V74 original target allocation length changed".into());
    }
    p.target_contracts
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let StackTargetContractV4::Object {
                object,
                owner,
                controller,
                zone,
                zone_change_count,
                ..
            } = c
            else {
                return Err("V74 target is not an original permanent".into());
            };
            let b = EffectObjectBinding {
                object: *object,
                expected_zone: *zone,
                expected_zone_change_count: *zone_change_count,
            };
            let current_original = current(s, b, v);
            let legal_now = current_original.is_some()
                && crate::engine::effect_target_is_legal_from_ability_source(
                    s,
                    source,
                    p.controller,
                    p.target_spec,
                    &p.targets_chosen,
                    i,
                );
            Ok(TargetV74 {
                original: IncarnationV74 {
                    object: object.0,
                    generation: *zone_change_count,
                    owner: (*owner).into(),
                    controller: (*controller).into(),
                    zone: *zone,
                },
                current_original,
                allocated_damage: p.chandra_damage_v74.as_ref().map(|d| d.allocations[i]),
                legal_now,
            })
        })
        .collect()
}
fn reference(r: &crate::rl::CardStableRefV1) -> public::ReferenceV1 {
    public::ReferenceV1 {
        object: r.arena_id,
        generation: r.zone_change_count,
        owner: r.owner,
        controller: r.controller,
        zone: r.zone,
        identity: public::IdentityV1::Known {
            card_id: r.card_db_id,
        },
    }
}
fn prevention(
    s: &GameState,
    choice: &super::super::crystal_prevention_v1::ChoiceV1,
    v: PlayerId,
) -> Result<Vec<PreventionV74>, String> {
    use super::super::crystal_prevention_v1::PreventionV1 as P;
    choice
        .options
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let (kind, source, shield_remaining) = match p {
                P::Multiplier(a) => (
                    PreventionKindV74::Multiplier,
                    Some(root(s, *a, v)?.original_source),
                    None,
                ),
                P::Barricade(a) => (
                    PreventionKindV74::Barricade,
                    Some(root(s, *a, v)?.original_source),
                    None,
                ),
                P::Protection(b) => (PreventionKindV74::Protection, current(s, *b, v), None),
                P::Replacement(r) => {
                    let (kind, amount) = match r.kind {
                        event::ReplacementEffectKind::PreventNextDamage { remaining, .. } => {
                            (PreventionKindV74::Shield, Some(remaining))
                        }
                        event::ReplacementEffectKind::PreventDamageFromColorUntilEndOfTurn {
                            ..
                        } => (PreventionKindV74::Color, None),
                        _ => {
                            return Err(
                                "V74 pending damage contains a non-noncombat replacement".into()
                            )
                        }
                    };
                    (kind, public::reference(s, r.source, Some(v)), amount)
                }
            };
            Ok(PreventionV74 {
                menu_index: u16::try_from(i).map_err(|_| "V74 prevention index overflow")?,
                kind,
                source,
                shield_remaining,
            })
        })
        .collect()
}
pub(crate) fn capture(s: &GameState, v: PlayerId) -> Result<FactsV74, String> {
    validate_state(s)?;
    validate_pending(s)?;
    crate::effect::validate_pending_effect_choice(s)?;
    crate::event::validate_copied_token_abilities_v1(s)?;
    let battlefield = crate::tinybones_public_v72::capture(s, v)?.battlefield;
    let authorized_printings = s
        .objects
        .iter()
        .filter_map(|(id, _)| public::reference(s, id, Some(v)))
        .filter(|r| matches!(r.identity, public::IdentityV1::Known { card_id: 493 }))
        .collect();
    let mut out = FactsV74 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        viewer: v.into(),
        active_player: s.active_player.into(),
        turn_key: s.turn_key_v1(),
        meaning: crate::rules_meaning_chandra_v74::printed_meaning()?,
        authorized_printings,
        battlefield,
        pending_allocation: None,
        activations: vec![],
        exiles: vec![],
        pending_damage: None,
        copied_tokens: vec![],
        end_steps: vec![],
        complete_ordinary_observation: false,
        owning_consumer_admitted: false,
    };
    if let Some(p) = s.engine.pending_activation.as_ref().filter(|p| {
        s.objects
            .try_get(p.source)
            .is_some_and(|o| definition(o.card_def))
            && p.ability_index == 2
    }) {
        crate::engine::validate_pending_activation(s, p)?;
        let source = AbilitySourceContractV4::capture(s, p.source);
        let d = p
            .chandra_damage_v74
            .as_ref()
            .ok_or("V74 pending distribution absent")?;
        let candidates = match allocation_decision(s, p) {
            Some(crate::engine::Decision::ChooseEffectTargets { legal_targets, .. }) => {
                legal_targets
                    .into_iter()
                    .map(|t| match t {
                        Target::Object(id) => public::reference(s, id, Some(v))
                            .ok_or("V74 public allocation candidate absent"),
                        _ => Err("V74 allocation candidate is not permanent"),
                    })
                    .collect::<Result<Vec<_>, _>>()?
            }
            _ => vec![],
        };
        out.pending_allocation = Some(AllocationV74 {
            source: root(s, source, v)?,
            loyalty_before: crate::planeswalker_v1::loyalty(s, p.source)
                .ok_or("V74 original loyalty absent")?,
            targets: targets(s, p, source, v)?,
            remaining_units: 8 - d.allocations.iter().copied().sum::<u8>(),
            finished: d.finished,
            can_finish_zero: !d.finished && d.allocations.is_empty(),
            candidates,
        });
    }
    let mut paid = std::collections::BTreeMap::new();
    let mut publications = std::collections::BTreeMap::new();
    for (_, j) in rows(s) {
        let JournalV74::Payment { input } = j else {
            continue;
        };
        let occurrence =
            u32::try_from(out.activations.len()).map_err(|_| "V74 occurrence overflow")?;
        paid.insert(input.index, occurrence);
        out.activations.push(ActivationV74 {
            occurrence,
            source: root(s, input.source, v)?,
            ability_index: input.pending.ability_index,
            loyalty_before: input.before,
            loyalty_after: input.after,
            targets: targets(s, &input.pending, input.source, v)?,
            stage: ActivationStageV74::Paid,
            damage_source: None,
        });
    }
    for (at, j) in rows(s) {
        let JournalV74::Published { input, item } = j else {
            continue;
        };
        let occurrence = *paid
            .get(&input.index)
            .ok_or("V74 original payment absent")?;
        publications.insert(at as u64, occurrence);
        let a = &mut out.activations[occurrence as usize];
        a.stage = if rows(s)
            .any(|(_, j)| matches!(j,JournalV74::Completed{publication}if *publication==at as u64))
        {
            ActivationStageV74::NativeCompleted
        } else if let Some(countered) = rows(s).find_map(|(_, j)| match j {
            JournalV74::NotResolved {
                publication,
                countered,
            } if *publication == at as u64 => Some(*countered),
            _ => None,
        }) {
            if countered {
                ActivationStageV74::Countered
            } else {
                ActivationStageV74::NotResolved
            }
        } else if crate::engine::time_stop_v1::disposed(s, item)? {
            ActivationStageV74::Disposed
        } else if s
            .engine
            .pending_effect
            .as_ref()
            .is_some_and(|p| p.resolving_item.v4.stack_item_id == item.v4.stack_item_id)
        {
            ActivationStageV74::Resolving
        } else {
            ActivationStageV74::Published
        };
        if input.pending.ability_index == 2 {
            let f = damage_source(s, item.source, item.v4.stack_item_id)?;
            a.damage_source = Some(DamageSourceV74 {
                source: root(s, f.source, v)?,
                historical: s.objects.try_get(f.source.source).is_none_or(|o| {
                    o.zone != Zone::Battlefield || o.zone_change_count != f.source.zone_change_count
                }),
                color_mask: f.color_mask,
                lifelink: f.lifelink,
                deathtouch: f.deathtouch,
                werewolf: f.werewolf,
            });
        }
    }
    for (cap, j) in rows(s) {
        let JournalV74::Exiled {
            cards,
            public: refs,
            ..
        } = j
        else {
            continue;
        };
        let (publication, _) = super::capture(s, cap as u64)?;
        let activation = *publications
            .get(&publication)
            .ok_or("V74 exile original activation absent")?;
        let mut selected_prefix = vec![];
        let mut choice_current = false;
        if let Some(p) = &s.engine.pending_effect {
            match p.choice.as_ref() {
                Some(PendingEffectChoice::SelectTargets {
                    purpose: EffectTargetSelectionPurpose::ChandraExileV74 { capture },
                    selected,
                    ..
                }) if *capture == cap as u64 => {
                    choice_current = true;
                    selected_prefix = selected
                        .iter()
                        .map(|c| {
                            cards
                                .iter()
                                .position(|b| c.expected_object == Some(*b))
                                .and_then(|i| u16::try_from(i).ok())
                                .ok_or("V74 chosen exile prefix changed")
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                }
                _ => {}
            }
            if let [EffectFrame::ChandraExileGrantV74 { capture, chosen }] = p.frames.as_slice() {
                if *capture == cap as u64 {
                    choice_current = true;
                    selected_prefix.push(
                        u16::try_from(
                            cards
                                .iter()
                                .position(|b| b == chosen)
                                .ok_or("V74 answered exile card absent")?,
                        )
                        .map_err(|_| "V74 exile index overflow")?,
                    );
                }
            }
        }
        let grant = rows(s).find_map(|(at, j)| match j {
            JournalV74::Granted {
                capture,
                chosen,
                permission,
            } if *capture == cap as u64 => Some((at, *chosen, permission)),
            _ => None,
        });
        let (chosen, grant_for_land, permission_present, expired) = if let Some((
            at,
            chosen,
            permission,
        )) = grant
        {
            (Some(u16::try_from(cards.iter().position(|b|*b==chosen).ok_or("V74 grant selected card absent")?).map_err(|_|"V74 exile index overflow")?),Some(permission.play_or_cast==crate::engine::PlayOrCast::Play),s.engine.exile_play_permissions.contains(permission),rows(s).any(|(_,j)|matches!(j,JournalV74::Expired{grants,..}if grants.contains(&(at as u64)))))
        } else {
            (None, None, false, false)
        };
        out.exiles.push(ExileV74 {
            activation,
            cards: cards
                .iter()
                .zip(refs)
                .map(|(b, r)| ExiledCardV74 {
                    original: reference(r),
                    current_original: current(s, *b, v),
                })
                .collect(),
            choice_current,
            selected_prefix,
            chosen,
            grant_for_land,
            permission_present,
            expired,
        });
    }
    if let Some(p) = s
        .engine
        .pending_effect
        .as_ref()
        .filter(|p| selected(s, &p.resolving_item))
    {
        if let Some(PendingEffectChoice::ChooseOption {
            purpose: EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
            ..
        }) = &p.choice
        {
            super::super::crystal_prevention_v1::validate(s)?;
            let (pub_at, _) = publication(s, &p.resolving_item)?;
            let Target::Object(recipient) = choice.target else {
                return Err("V74 affected recipient is not permanent".into());
            };
            out.pending_damage = Some(DamageChoiceV74 {
                activation: *publications
                    .get(&pub_at)
                    .ok_or("V74 damage activation absent")?,
                chooser: choice.chooser.into(),
                recipient: public::reference(s, recipient, Some(v))
                    .ok_or("V74 current affected recipient absent")?,
                original_amount: choice.original_amount,
                remaining_amount: choice.remaining_amount,
                simultaneous_waiting: choice.batch.as_ref().map_or(1, |b| {
                    b.outcomes
                        .iter()
                        .filter(|o| {
                            matches!(
                                o,
                                super::super::crystal_prevention_v1::BatchOutcomeV2::Waiting
                            )
                        })
                        .count()
                }),
                options: prevention(s, choice, v)?,
            });
        }
    }
    let mut token_entries = std::collections::BTreeMap::new();
    for e in &s.engine.event_history {
        if let E::CopiedTokenAbilitiesEnteredV1 {
            source, instances, ..
        } = e
        {
            let occurrence =
                u32::try_from(out.copied_tokens.len()).map_err(|_| "V74 copied entry overflow")?;
            token_entries.insert((source.source, source.zone_change_count), occurrence);
            out.copied_tokens.push(CopiedTokenV74 {
                occurrence,
                source: root(s, *source, v)?,
                sacrifice_instances: *instances,
                current_haste: s
                    .objects
                    .try_get(source.source)
                    .filter(|o| {
                        o.zone == Zone::Battlefield
                            && o.zone_change_count == source.zone_change_count
                    })
                    .map(|_| {
                        crate::engine::has_effective_keyword(s, source.source, Keywords::HASTE)
                    }),
            });
        }
    }
    for (at, e) in s.engine.event_history.iter().enumerate() {
        if let E::CopiedTokenEndStepV1 { .. } = e {
            let sources = crate::event::copied_end_sources_v1(s, at as u64)?;
            let observers = sources
                .iter()
                .map(|(source, count)| {
                    Ok(EndStepObserverV74 {
                        token: *token_entries
                            .get(&(source.source, source.zone_change_count))
                            .ok_or("V74 end-step source lost actual copied entry")?,
                        source: root(s, *source, v)?,
                        instances: *count,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let (published_count, finished_count) =
                crate::event::copied_token_receipt_counts_v74(s, at as u64)?;
            out.end_steps.push(EndStepV74{
                occurrence:u32::try_from(out.end_steps.len()).map_err(|_|"V74 end-step occurrence overflow")?,
                observers,
                collected:s.engine.event_history.iter().any(|e|matches!(e,E::CopiedTokenEndStepCollectedV1{capture_history_index}if *capture_history_index==at as u64)),
                published_count,
                finished_count,
            });
        }
    }
    Ok(out)
}
