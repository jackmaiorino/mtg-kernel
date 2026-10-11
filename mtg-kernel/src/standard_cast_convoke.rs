//! Explicit convoker subset selection. The ordinary mana solver pays the remainder.
use super::*;
use spell_costs_v1::SpellManaPaymentV1;
pub(super) fn active(state: &GameState, p: &PendingCast) -> bool {
    p.cast_mode == Some(CastMode::Alternative)
        && card_def::CARD_DEFS[state.objects.get(p.spell).card_def as usize]
            .alt_cost
            .is_some_and(|alt| {
                alt.components
                    .iter()
                    .any(|c| matches!(c, CostComponent::ConvokeMana(_)))
            })
}
fn selected(
    state: &GameState,
    p: &PendingCast,
) -> Option<spell_costs_v1::SelectedSpellManaCostsV1> {
    let def = &card_def::CARD_DEFS[state.objects.get(p.spell).card_def as usize];
    spell_costs_v1::selected_spell_mana_costs_v1(
        def,
        CastMethodV4::Alternative,
        p.kicked == Some(true),
        p.mode_chosen.unwrap_or(0),
        &p.targets_chosen,
        p.controller,
        state,
    )
}
fn cost(state: &GameState, p: &PendingCast) -> Option<(Vec<mana::Pip>, u32, bool)> {
    let s = selected(state, p)?;
    let (increase, reduction) = spell_cost_generic_modifiers_v1(state, s.types, p.controller);
    let pips = s
        .costs
        .iter()
        .flat_map(|c| c.pips.iter().copied())
        .collect();
    let generic = s
        .costs
        .iter()
        .map(|c| u32::from(c.generic) + u32::from(c.x_count) * u32::from(p.x_value.unwrap_or(0)))
        .sum::<u32>()
        .saturating_add(increase)
        .saturating_sub(reduction.saturating_add(s.own_generic_reduction));
    Some((pips, generic, s.types.contains(&CardType::Creature)))
}
fn pool(state: &GameState, p: &PendingCast) -> Vec<EffectObjectBinding> {
    state.players[p.controller.index()]
        .battlefield
        .iter()
        .copied()
        .filter_map(|id| {
            let o = state.objects.get(id);
            (o.controller == p.controller
                && o.zone == Zone::Battlefield
                && !o.tapped
                && object_has_type(state, id, CardType::Creature))
            .then_some(EffectObjectBinding {
                object: id,
                expected_zone: Zone::Battlefield,
                expected_zone_change_count: o.zone_change_count,
            })
        })
        .collect()
}
pub(super) fn payment(
    state: &GameState,
    p: &PendingCast,
    chosen: &[EffectObjectBinding],
) -> Option<SpellManaPaymentV1> {
    if chosen.is_empty() {
        return None;
    }
    let (pips, generic, creature_spell) = cost(state, p)?;
    let live = pool(state, p);
    if chosen
        .iter()
        .enumerate()
        .any(|(i, b)| !live.contains(b) || chosen[..i].contains(b))
    {
        return None;
    }
    let excluded = chosen.iter().map(|b| b.object).collect::<Vec<_>>();
    fn assign(
        state: &GameState,
        p: &PendingCast,
        chosen: &[EffectObjectBinding],
        i: usize,
        remaining_cost: (Vec<mana::Pip>, u32),
        creature_spell: bool,
        excluded: &[ObjectId],
    ) -> Option<mana::PaymentPlan> {
        let (pips, generic) = remaining_cost;
        if i == chosen.len() {
            return mana::plan_spell_mana_total_v1(
                &pips,
                generic,
                p.controller,
                state,
                creature_spell,
                excluded,
                0,
            );
        }
        let colors = object_color_mask(state, chosen[i].object);
        for (j, pip) in pips.iter().enumerate() {
            if matches!(pip, mana::Pip::Colored(c) if colors & card_def::mana_color_mask(*c) != 0) {
                let mut rest = pips.clone();
                rest.remove(j);
                if let Some(plan) = assign(
                    state,
                    p,
                    chosen,
                    i + 1,
                    (rest, generic),
                    creature_spell,
                    excluded,
                ) {
                    return Some(plan);
                }
            }
        }
        if generic > 0 {
            assign(
                state,
                p,
                chosen,
                i + 1,
                (pips, generic - 1),
                creature_spell,
                excluded,
            )
        } else {
            None
        }
    }
    let mana = assign(
        state,
        p,
        chosen,
        0,
        (pips, generic),
        creature_spell,
        &excluded,
    )?;
    Some(SpellManaPaymentV1 {
        mana,
        convoke_tapped: excluded,
        delve_exiled: Vec::new(),
    })
}
fn can_complete(state: &GameState, p: &PendingCast, prefix: &[EffectObjectBinding]) -> bool {
    let Some((pips, generic, _)) = cost(state, p) else {
        return false;
    };
    let max = pips.len() + generic as usize;
    if prefix.len() > max {
        return false;
    }
    let remaining = pool(state, p)
        .into_iter()
        .filter(|b| !prefix.contains(b))
        .collect::<Vec<_>>();
    fn find(
        state: &GameState,
        p: &PendingCast,
        remaining: &[EffectObjectBinding],
        chosen: &mut Vec<EffectObjectBinding>,
        max: usize,
    ) -> bool {
        if payment(state, p, chosen).is_some() {
            return true;
        }
        if chosen.len() >= max {
            return false;
        }
        for (i, b) in remaining.iter().enumerate() {
            chosen.push(*b);
            if find(state, p, &remaining[i + 1..], chosen, max) {
                return true;
            }
            chosen.pop();
        }
        false
    }
    find(state, p, &remaining, &mut prefix.to_vec(), max)
}
pub(super) fn validate(state: &GameState, p: &PendingCast) -> Result<(), String> {
    if !active(state, p) {
        if !p.convoke_chosen.is_empty() || p.convoke_finished {
            return Err("nonconvoke cast carries convokers".into());
        }
        return Ok(());
    }
    let live = pool(state, p);
    if p.convoke_chosen
        .iter()
        .enumerate()
        .any(|(i, b)| !live.contains(b) || p.convoke_chosen[..i].contains(b))
    {
        return Err("convoker changed incarnation or eligibility".into());
    }
    if (!p.convoke_chosen.is_empty() || p.convoke_finished)
        && (p.x_value.is_none() || !can_complete(state, p, &p.convoke_chosen))
    {
        return Err("convoke selection cannot complete".into());
    }
    if p.convoke_finished && payment(state, p, &p.convoke_chosen).is_none() {
        return Err("finished convoke cannot pay".into());
    }
    Ok(())
}
fn candidates(state: &GameState, p: &PendingCast) -> Vec<ObjectId> {
    pool(state, p)
        .into_iter()
        .filter(|b| !p.convoke_chosen.contains(b))
        .filter(|b| {
            let mut chosen = p.convoke_chosen.clone();
            chosen.push(*b);
            can_complete(state, p, &chosen)
        })
        .map(|b| b.object)
        .collect()
}
pub(super) fn decision(state: &GameState, p: &PendingCast) -> Decision {
    let (pips, generic, _) = cost(state, p).expect("validated convoke cost");
    Decision::ChooseEffectTargets {
        player: p.controller,
        source: p.spell,
        selected_count: p.convoke_chosen.len() as u16,
        min_targets: 1,
        max_targets: u16::try_from(pips.len() + generic as usize).unwrap_or(u16::MAX),
        legal_targets: candidates(state, p)
            .into_iter()
            .map(Target::Object)
            .collect(),
        can_finish: payment(state, p, &p.convoke_chosen).is_some(),
    }
}
pub(super) fn choose(state: &mut GameState, target: Target) -> Result<(), String> {
    let p = state.engine.pending_cast.clone().ok_or("no convoke cast")?;
    if pending_cast_action_stage(state, &p)? != PendingCastActionStage::ChooseConvoke {
        return Err("convoke is not current stage".into());
    }
    let Target::Object(id) = target else {
        return Err("convoker is not a creature".into());
    };
    if !candidates(state, &p).contains(&id) {
        return Err("illegal convoker".into());
    }
    let binding = pool(state, &p)
        .into_iter()
        .find(|b| b.object == id)
        .unwrap();
    state
        .engine
        .pending_cast
        .as_mut()
        .unwrap()
        .convoke_chosen
        .push(binding);
    Ok(())
}
pub(super) fn finish(state: &mut GameState) -> Result<(), String> {
    let p = state.engine.pending_cast.clone().ok_or("no convoke cast")?;
    if pending_cast_action_stage(state, &p)? != PendingCastActionStage::ChooseConvoke
        || payment(state, &p, &p.convoke_chosen).is_none()
    {
        return Err("convoke cannot finish".into());
    }
    state.engine.pending_cast.as_mut().unwrap().convoke_finished = true;
    Ok(())
}
