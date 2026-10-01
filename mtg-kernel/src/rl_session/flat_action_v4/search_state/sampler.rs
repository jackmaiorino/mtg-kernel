//! Whole-object determinization. This is a sampling convention, not a posterior.
use super::*;
use crate::state::{AbilitySourceContractV4, SplitMix64, StackItem};

fn conflicts(
    state: &GameState,
    pool: &[ObjectId],
    plan: Option<&crate::effect::library_choice_search_v2::Plan>,
) -> bool {
    if super::effect_refs::conflicts(state, pool, plan) {
        return true;
    }
    let same = |id: ObjectId, generation: u32| {
        pool.contains(&id) && state.objects.get(id).zone_change_count == generation
    };
    let ability = |c: &AbilitySourceContractV4| {
        same(c.source, c.zone_change_count)
            || c.attached_to
                .is_some_and(|x| same(x.object, x.zone_change_count))
    };
    let stack = |s: &StackItem| {
        s.inline_effect
            .as_ref()
            .is_some_and(|op| super::effect_refs::op_conflicts(state, pool, op))
            || s.v4.ability_source_contract.as_ref().is_some_and(&ability)
            || s.v4.granted_by.as_ref().is_some_and(&ability)
            || s.v4
                .hidden_ability_source
                .is_some_and(|x| same(x.object, x.zone_change_count))
            || s.v4
                .madness_source_contract
                .is_some_and(|x| same(x.source, x.zone_change_count))
            || s.v4
                .source_contract
                .as_ref()
                .is_some_and(|x| same(x.source, x.zone_change_count))
    };
    let e = &state.engine;
    if e.pending_triggers.iter().any(|t| {
        super::effect_refs::op_conflicts(state, pool, &t.effect)
            || t.source_contract.as_ref().is_some_and(&ability)
            || t.granted_by.as_ref().is_some_and(&ability)
    }) || state.stack.iter().any(&stack)
        || e.pending_effect
            .as_ref()
            .is_some_and(|p| stack(&p.resolving_item))
        || e.initiative_source.as_ref().is_some_and(&ability)
        || e.monarch_source.as_ref().is_some_and(&ability)
        || e.pending_activation
            .as_ref()
            .is_some_and(|p| same(p.source, p.source_zone_change_count))
        || e.pending_land_play
            .as_ref()
            .is_some_and(|p| same(p.source, p.source_zone_change_count))
        || e.pending_cast.as_ref().is_some_and(|p| {
            same(
                p.source_contract.source,
                p.source_contract.zone_change_count,
            )
        })
        || e.pending_spell_copy
            .as_ref()
            .is_some_and(|p| same(p.resolving_source, p.resolving_source_zone_change_count))
        || e.linked_exile_records
            .iter()
            .any(|r| ability(&r.source) || same(r.exiled, r.exiled_zone_change_count))
        || e.until_next_turn_keywords
            .iter()
            .any(|r| same(r.object_id, r.object_zone_change_count))
        || e.exile_play_permissions
            .iter()
            .any(|r| same(r.object, r.zone_change_generation))
        || e.pending_kicked_source.is_some_and(|id| pool.contains(&id))
    {
        return true;
    }
    // Conservative exclusions for unversioned references. Do not pin or redraw.
    if e.active_replacements.iter().any(|r| {
        pool.contains(&r.source)
            || match r.kind {
                crate::event::ReplacementEffectKind::PreventNextDamage {
                    target: crate::state::Target::Object(id),
                    ..
                } => pool.contains(&id),
                _ => false,
            }
    }) {
        return true;
    }
    e.until_end_of_turn.iter().any(|r| {
        use crate::engine::UntilEndOfTurnEffect::*;
        match r {
            SyntheticMarker(id) => pool.contains(id),
            ResolvedSetEffect { object_ids, .. } => object_ids.iter().any(|id| pool.contains(id)),
            ResolvedObjectEffect {
                object_id,
                object_zone_change_count,
                ..
            }
            | ResolvedObjectKeywordEffect {
                object_id,
                object_zone_change_count,
                ..
            } => same(*object_id, *object_zone_change_count),
            DamageCannotBePrevented { .. } => false,
        }
    })
}

fn draw(rng: &mut SplitMix64, bound: u64) -> usize {
    let threshold = bound.wrapping_neg() % bound;
    loop {
        let value = rng.next_u64();
        if value >= threshold {
            return (value % bound) as usize;
        }
    }
}

/// Called only on a disposable cloned state. Failure never permits retrying a seed.
pub(super) fn redeterminize(
    state: &mut GameState,
    actor: PlayerId,
    seed: u64,
) -> Result<(), Error> {
    redeterminize_with_library_plan(state, actor, seed, None)
}
pub(super) fn redeterminize_with_library_plan(
    state: &mut GameState,
    actor: PlayerId,
    seed: u64,
    plan: Option<&crate::effect::library_choice_search_v2::Plan>,
) -> Result<(), Error> {
    let mut rng = SplitMix64::seed(seed);
    for owner in [PlayerId::P0, PlayerId::P1] {
        // Slots are canonical: owner order, then hand indices, then library indices.
        let mut slots = Vec::new();
        if owner != actor {
            for (i, &id) in state.players[owner.index()].hand.iter().enumerate() {
                let obj = state.objects.get(id);
                if !state
                    .known_hand_cards(actor, owner)
                    .iter()
                    .any(|k| k.object == id && k.zone_change_count == obj.zone_change_count)
                {
                    slots.push((Zone::Hand, i, id));
                }
            }
        }
        for (i, &id) in state.players[owner.index()].library.iter().enumerate() {
            let obj = state.objects.get(id);
            if !state.known_library_cards(actor, owner).iter().any(|k| {
                k.position as usize == i
                    && k.object == id
                    && k.zone_change_count == obj.zone_change_count
            }) {
                slots.push((Zone::Library, i, id));
            }
        }
        let mut pool: Vec<_> = slots.iter().map(|x| x.2).collect();
        pool.sort_unstable();
        if pool.windows(2).any(|x| x[0] == x[1]) || conflicts(state, &pool, plan) {
            return Err(Error::HiddenStateContract);
        }
        for &(zone, _, id) in &slots {
            let obj = state.objects.get(id);
            if obj.owner != owner
                || obj.zone != zone
                || obj.spell_copy_origin.is_some()
                || !obj.attachments.is_empty()
                || obj.v4.attached_to.is_some()
                || obj.controller != owner
                || obj.tapped
                || obj.summoning_sick
                || obj.damage != 0
                || obj.counters != crate::state::Counters::default()
                || obj.plotted_turn.is_some()
                || obj.v4.is_token
                || obj.v4 != crate::state::ObjectStateV4::from_card_def(obj.card_def)
            {
                return Err(Error::HiddenStateContract);
            }
        }
        for i in (1..pool.len()).rev() {
            let j = draw(&mut rng, (i + 1) as u64);
            pool.swap(i, j);
        }
        // Only library knowledge needs rebinding: own-hand knowledge is implicit,
        // and the other observer's knowledge of actor's hand is never pooled.
        let observer = actor.opponent();
        if !state.hand_knowledge[observer.index()][observer.index()].is_empty() {
            return Err(Error::HiddenStateContract);
        }
        for ((zone, i, old), new) in slots.iter().copied().zip(pool.iter().copied()) {
            let old_generation = state.objects.get(old).zone_change_count;
            let new_generation = state.objects.get(new).zone_change_count;
            match zone {
                Zone::Hand => {
                    state.players[owner.index()].hand[i] = new;
                }
                Zone::Library => {
                    for k in &mut state.library_knowledge[observer.index()][owner.index()] {
                        if k.position as usize == i
                            && k.object == old
                            && k.zone_change_count == old_generation
                        {
                            k.object = new;
                            k.zone_change_count = new_generation;
                        }
                    }
                    state.players[owner.index()].library[i] = new;
                }
                _ => return Err(Error::HiddenStateContract),
            }
            state.objects.get_mut(new).zone = zone;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn v4_search_sampler_identity_repeatability_and_slot_knowledge() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = super::super::tests::state(actor);
            let other = actor.opponent();
            assert!(state.hand_knowledge[other.index()][other.index()].is_empty());
            state.reveal_library_top(other, other, 2);
            state.reveal_library_top(other, actor, 2);
            let original = state.clone();
            let mut repeat = state.clone();
            redeterminize(&mut state, actor, 8172).unwrap();
            redeterminize(&mut repeat, actor, 8172).unwrap();
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&repeat).unwrap()
            );
            for owner in [PlayerId::P0, PlayerId::P1] {
                for id in original.players[owner.index()]
                    .hand
                    .iter()
                    .chain(&original.players[owner.index()].library)
                {
                    let mut before = original.objects.get(*id).clone();
                    let after = state.objects.get(*id);
                    before.zone = after.zone;
                    assert_eq!(
                        serde_json::to_vec(&before).unwrap(),
                        serde_json::to_vec(after).unwrap()
                    );
                }
            }
            assert!(state.hand_knowledge[other.index()][other.index()].is_empty());
            for owner in [actor, other] {
                assert_eq!(
                    state.library_knowledge[other.index()][owner.index()].len(),
                    2
                );
                for k in &state.library_knowledge[other.index()][owner.index()] {
                    assert_eq!(
                        k.object,
                        state.players[owner.index()].library[k.position as usize]
                    );
                    assert_eq!(
                        k.zone_change_count,
                        state.objects.get(k.object).zone_change_count
                    );
                }
            }
            // Canonical pool makes a fixed object set independent of its unknown slot arrangement.
            let mut reordered = original.clone();
            reordered.players[actor.index()].library.reverse();
            redeterminize(&mut reordered, actor, 8172).unwrap();
            assert_eq!(
                state.players[actor.index()].library,
                reordered.players[actor.index()].library
            );
        }
    }
    #[test]
    fn v4_search_sampler_rejects_same_incarnation_without_pinning() {
        let mut state = super::super::tests::state(PlayerId::P0);
        let id = state.players[0].library[0];
        state.engine.initiative_source = Some(AbilitySourceContractV4::capture(&state, id));
        let original = serde_json::to_vec(&state).unwrap();
        assert_eq!(
            redeterminize(&mut state, PlayerId::P0, 1),
            Err(Error::HiddenStateContract)
        );
        assert_eq!(original, serde_json::to_vec(&state).unwrap());
    }
    #[test]
    fn v4_search_sampler_rejects_hidden_residue_without_normalizing() {
        let mut state = super::super::tests::state(PlayerId::P0);
        let id = state.players[0].library[0];
        state.objects.get_mut(id).v4.entered_battlefield_turn = Some(state.turn);
        let before = state.objects.get(id).clone();
        assert_eq!(
            redeterminize(&mut state, PlayerId::P0, 1),
            Err(Error::HiddenStateContract)
        );
        assert_eq!(&before, state.objects.get(id));
        let obj = state.objects.get_mut(id);
        obj.v4
            .reset_for_zone_change(obj.card_def, Zone::Library, state.turn);
        assert!(redeterminize(&mut state, PlayerId::P0, 1).is_ok());
    }
}
