//! Private native entry receipts. No public observation or owner admission.
use super::*;
use crate::{
    card_def::{self, CardCapability, CreatureTypeEntryDefV1, Keywords, TargetSpec},
    doubling_season_v48::ReplacementInputV48,
    effect::{EffectOp, ObjectRef},
    event::CommittedEvent,
    linked_aura_operation_v1::{GeneralAnswerV36, GeneralChoiceV36, OperationV1},
    state::GameObject,
};
use serde::{Deserialize, Serialize};

pub(crate) fn owns_definition(id: u16) -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && id == 463
        && CARD_DEFS.get(usize::from(id)).is_some_and(exact_definition)
        && crate::trigger::triggers_for(id).is_empty()
}

// The exhaustive field closure below is extended deliberately whenever native
// CardDef changes. No name-only dispatch can turn an unrelated definition into Banner.
fn exact_definition(d: &card_def::CardDef) -> bool {
    d.name == "Banner of Kinship"
        && d.object_name == d.name
        && d.capability == CardCapability::Partial
        && d.creature_type_entry_v1 == Some(CreatureTypeEntryDefV1::FellowshipAndChosenTypeAnthem)
        && d.cost.generic == 5
        && d.cost.pips.is_empty()
        && d.cost.x_count == 0
        && d.mana_value == 5
        && d.types == [CardType::Artifact]
        && d.subtypes.is_empty()
        && d.supertypes.is_empty()
        && d.colors.is_empty()
        && d.power.is_none()
        && d.toughness.is_none()
        && d.keywords == Keywords::NONE
        && !d.is_token
        && d.target_spec == TargetSpec::None
        && (d.spell_effect)()
            == Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
        && foreign_fields_absent(d)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChoiceV1 {
    pub ordinal: u32,
    pub proposal: ProposedEvent,
    pub controller: PlayerId,
    pub card_def: u16,
    pub original: Option<GameObject>,
    /// Fixed before any member of this actual entry batch is committed.
    pub counts: Vec<i32>,
    pub replacement: ReplacementInputV48,
}

pub(crate) fn choice(
    state: &GameState,
    proposal: &ProposedEvent,
    ordinal: u32,
) -> Result<Option<ChoiceV1>, String> {
    let (controller, card_def, original) = match proposal {
        ProposedEvent::ZoneChange(z) if z.to_zone == Zone::Battlefield => {
            let live = state
                .objects
                .try_get(z.object)
                .ok_or("Kinship entry object absent")?;
            if !owns_definition(live.card_def) || live.zone == Zone::Battlefield {
                return Ok(None);
            }
            if z.battlefield_face_index.is_some_and(|face| face != 0)
                || live.v4.face_down_v1.is_some()
            {
                return Err("Kinship entry has unsupported face".into());
            }
            (
                z.battlefield_controller
                    .unwrap_or(if live.zone == Zone::Stack {
                        live.controller
                    } else {
                        live.owner
                    }),
                live.card_def,
                Some(live.clone()),
            )
        }
        ProposedEvent::CreateToken(t) if owns_definition(t.token_def) => {
            if t.entry_face_index.is_some_and(|face| face != 0) {
                return Err("Kinship token face changed".into());
            }
            (t.controller, t.token_def, None)
        }
        _ => return Ok(None),
    };
    let mut counts = vec![0i32; CreatureTypeV60::ALL.len()];
    for (id, live) in state.objects.iter() {
        if live.zone != Zone::Battlefield || live.controller != controller {
            continue;
        }
        for chosen in CreatureTypeV60::ALL {
            if creature_types_v60::matches_effective_creature(state, id, *chosen)
                .map_err(|e| format!("Kinship pre-entry type: {e:?}"))?
            {
                let count = &mut counts[usize::from(chosen.choice_index())];
                *count = count
                    .checked_add(1)
                    .ok_or("Kinship pre-entry count overflow")?;
            }
        }
    }
    // Fellowship counters are placed by an effect, independently of the move's origin.
    let replacement = ReplacementInputV48::capture(state, controller);
    Ok(Some(ChoiceV1 {
        ordinal,
        proposal: proposal.clone(),
        controller,
        card_def,
        original,
        counts,
        replacement,
    }))
}

pub(crate) fn amount(choice: &ChoiceV1, chosen: CreatureTypeV60) -> Result<i32, String> {
    if choice.counts.len() != CreatureTypeV60::ALL.len()
        || choice.counts.iter().any(|n| *n < 0)
        || choice.replacement.controller != choice.controller
    {
        return Err("Kinship entry count/replacement shape changed".into());
    }
    let base = choice.counts[usize::from(chosen.choice_index())];
    let doublings = choice
        .replacement
        .entry_talent_doublings
        .ok_or("Kinship missing pre-entry Talent modifiers")?;
    let scaled = (0..doublings).try_fold(base, |n, _| {
        n.checked_mul(2).ok_or("Kinship entry modifier overflow")
    })?;
    choice.replacement.replaced_count(scaled)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnteredV1 {
    pub operation: u32,
    pub capture: u32,
    pub answer: u32,
    pub entry: u32,
    pub source: AbilitySourceContractV4,
    pub chosen: CreatureTypeV60,
    pub count: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JournalV1 {
    Started {
        operation: Box<OperationV1>,
        original_sha256: String,
    },
    Captured {
        operation: u32,
        choice: Box<ChoiceV1>,
    },
    Chosen {
        capture: u32,
        chosen: CreatureTypeV60,
        count: i32,
    },
    Committed {
        entered: EnteredV1,
    },
    Completed {
        operation: u32,
        entries: Vec<u32>,
    },
}
impl std::hash::Hash for JournalV1 {
    fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
        std::hash::Hash::hash(
            &serde_json::to_vec(self).expect("native Kinship receipt serializes"),
            h,
        );
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointV1 {
    pub original: Box<GameState>,
    pub operation: OperationV1,
    pub answers: Vec<GeneralAnswerV36>,
    pub choice: GeneralChoiceV36,
    pub operation_history_index: u32,
    pub used_banner: bool,
}

pub(crate) fn journal(state: &GameState, index: u32) -> Option<&JournalV1> {
    match state.engine.event_history.get(index as usize) {
        Some(CommittedEvent::KinshipEntryV1 { journal }) => Some(journal),
        _ => None,
    }
}

pub(crate) fn validate_entered(
    state: &GameState,
    entered: &EnteredV1,
    at: usize,
) -> Result<(), String> {
    if !(entered.operation < entered.capture
        && entered.capture < entered.answer
        && entered.answer < entered.entry
        && (entered.entry as usize) < at)
    {
        return Err("Kinship entry authority order changed".into());
    }
    if !matches!(
        journal(state, entered.operation),
        Some(JournalV1::Started { .. })
    ) {
        return Err("Kinship entry operation absent".into());
    }
    let Some(JournalV1::Captured { operation, choice }) = journal(state, entered.capture) else {
        return Err("Kinship entry capture absent".into());
    };
    if *operation != entered.operation
        || !owns_definition(choice.card_def)
        || entered.source.card_def != choice.card_def
        || entered.source.controller != choice.controller
        || entered.source.zone != Zone::Battlefield
        || entered.source.attached_to.is_some()
        || entered.count != amount(choice, entered.chosen)?
        || !matches!(journal(state,entered.answer),Some(JournalV1::Chosen{capture,chosen,count}) if *capture==entered.capture && *chosen==entered.chosen && *count==entered.count)
    {
        return Err("Kinship captured definition/controller/type/counters changed".into());
    }
    match (
        &choice.proposal,
        &choice.original,
        state.engine.event_history.get(entered.entry as usize),
    ) {
        (
            ProposedEvent::ZoneChange(z),
            Some(original),
            Some(CommittedEvent::ZoneChange {
                object,
                from,
                to,
                controller_before,
            }),
        ) if *object == z.object
            && *object == entered.source.source
            && *from == original.zone
            && *to == Zone::Battlefield
            && original.card_def == choice.card_def
            && *controller_before == original.controller
            && original.owner == entered.source.owner
            && z.battlefield_controller
                .unwrap_or(if original.zone == Zone::Stack {
                    original.controller
                } else {
                    original.owner
                })
                == choice.controller
            && original.zone_change_count.checked_add(1)
                == Some(entered.source.zone_change_count) => {}
        (
            ProposedEvent::CreateToken(t),
            None,
            Some(CommittedEvent::CreateToken {
                object,
                token_def,
                controller,
                ..
            }),
        ) if *object == entered.source.source
            && *token_def == t.token_def
            && *token_def == choice.card_def
            && *controller == choice.controller
            && entered.source.owner == choice.controller
            && entered.source.zone_change_count == 0 => {}
        _ => return Err("Kinship metadata lost actual corresponding entry".into()),
    }
    let Some(JournalV1::Started { operation, .. }) = journal(state, entered.operation) else {
        return Err("Kinship original operation absent".into());
    };
    match operation.as_ref() {
        OperationV1::Event(event) | OperationV1::Commit(event) if *event != choice.proposal => {
            return Err("Kinship entry differs from original direct proposal".into());
        }
        OperationV1::Batch(events) if !events.contains(&choice.proposal) => {
            return Err("Kinship entry differs from original batch roster".into());
        }
        OperationV1::CopyTokenKinshipV1 { source, controller }
            if !matches!(&choice.proposal, ProposedEvent::CreateToken(token)
                if token.token_def == source.card_def && token.controller == *controller) =>
        {
            return Err("Kinship token differs from original copy template/controller".into());
        }
        OperationV1::Step(_) | OperationV1::Advance
            if matches!(&choice.proposal, ProposedEvent::ZoneChange(change)
                if choice.original.as_ref().is_some_and(|o| o.zone == Zone::Stack)
                    && change.battlefield_controller.is_some()) =>
        {
            return Err("Kinship ordinary spell entry changed original controller".into());
        }
        _ => {}
    }
    Ok(())
}

pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    if crate::linked_aura_operation_v1::locally_owned(state) {
        return Ok(());
    }
    // A different entry family can own a later checkpoint in the same
    // operation. Reconstruct that entire original operation before inspecting
    // its deliberately incomplete synchronous Kinship receipt sequence.
    if crate::linked_aura_operation_v1::has_pending(state) {
        return crate::linked_aura_operation_v1::validate(state);
    }
    if state
        .engine
        .event_log
        .iter()
        .any(|e| matches!(e, CommittedEvent::KinshipEntryV1 { .. }))
    {
        return Err("private Kinship receipt in public event log".into());
    }
    for (at, e) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::KinshipEntryV1 { journal: row } = e else {
            continue;
        };
        match row.as_ref() {
            JournalV1::Committed { entered } => validate_entered(state, entered, at)?,
            JournalV1::Started {
                original_sha256, ..
            } => {
                if original_sha256.len() != 64
                    || !original_sha256.bytes().all(|c| c.is_ascii_hexdigit())
                {
                    return Err("Kinship original input digest malformed".into());
                }
                let closed=state.engine.event_history.iter().enumerate().filter(|(_,e)|matches!(e,CommittedEvent::KinshipEntryV1{journal} if matches!(journal.as_ref(),JournalV1::Completed{operation,..} if *operation as usize==at))).collect::<Vec<_>>();
                if closed.len() != 1 || closed[0].0 <= at {
                    return Err("Kinship operation not completed exactly once".into());
                }
            }
            JournalV1::Completed { operation, entries } => {
                if !matches!(journal(state, *operation), Some(JournalV1::Started { .. }))
                    || entries.is_empty()
                {
                    return Err("Kinship completion has no original operation/entries".into());
                }
                let actual=state.engine.event_history.iter().enumerate().filter_map(|(i,e)|match e{CommittedEvent::KinshipEntryV1{journal} if matches!(journal.as_ref(),JournalV1::Committed{entered} if entered.operation==*operation)=>Some(i as u32),_=>None}).collect::<Vec<_>>();
                if *entries != actual || entries.iter().any(|i| *i as usize >= at) {
                    return Err("Kinship completion entry roster changed".into());
                }
            }
            JournalV1::Captured { operation, choice } => {
                if *operation as usize >= at
                    || !matches!(journal(state, *operation), Some(JournalV1::Started { .. }))
                    || !owns_definition(choice.card_def)
                    || choice.counts.len() != CreatureTypeV60::ALL.len()
                    || choice.counts.iter().any(|n| *n < 0)
                    || choice.replacement.controller != choice.controller
                {
                    return Err("Kinship capture has foreign operation/printing/scene".into());
                }
                let answers=state.engine.event_history.iter().enumerate().filter(|(_,e)|matches!(e,CommittedEvent::KinshipEntryV1{journal} if matches!(journal.as_ref(),JournalV1::Chosen{capture,..} if *capture as usize==at))).collect::<Vec<_>>();
                if answers.len() != 1 || answers[0].0 <= at {
                    return Err("Kinship capture has no unique later answer".into());
                }
            }
            JournalV1::Chosen {
                capture,
                chosen,
                count,
            } => {
                let Some(JournalV1::Captured { choice, .. }) = journal(state, *capture) else {
                    return Err("Kinship answer lost capture".into());
                };
                if *capture as usize >= at || *count != amount(choice, *chosen)? {
                    return Err("Kinship answer count/type changed".into());
                }
                let commits=state.engine.event_history.iter().enumerate().filter(|(_,e)|matches!(e,CommittedEvent::KinshipEntryV1{journal} if matches!(journal.as_ref(),JournalV1::Committed{entered} if entered.answer as usize==at))).collect::<Vec<_>>();
                if commits.len() != 1 || commits[0].0 <= at {
                    return Err("Kinship answer has no unique later actual entry".into());
                }
            }
        }
    }
    for (id, live) in state.objects.iter() {
        if live.zone == Zone::Battlefield && owns_definition(live.card_def) {
            if live.counters.fellowship < 0 {
                return Err("negative live Kinship fellowship counters".into());
            }
            authenticated_type(state, id)?;
        } else if live.v4.kinship_type_v1.is_some() {
            return Err("Kinship type on foreign object/incarnation".into());
        }
    }
    Ok(())
}

pub(crate) fn authenticated_type(
    state: &GameState,
    id: ObjectId,
) -> Result<CreatureTypeV60, String> {
    let live = state.objects.try_get(id).ok_or("Kinship source absent")?;
    if let Some(chosen) = crate::linked_aura_operation_v1::local_kinship_type(state, id) {
        return Ok(chosen);
    }
    let rows = state
        .engine
        .event_history
        .iter()
        .filter_map(|e| match e {
            CommittedEvent::KinshipEntryV1 { journal } => match journal.as_ref() {
                JournalV1::Committed { entered }
                    if entered.source.source == id
                        && entered.source.zone_change_count == live.zone_change_count =>
                {
                    Some(entered)
                }
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>();
    if rows.len() != 1
        || rows[0].source.owner != live.owner
        || live.v4.kinship_type_v1 != Some(rows[0].chosen)
    {
        return Err("Kinship source lost exact entering type binding".into());
    }
    Ok(rows[0].chosen)
}

pub(crate) fn controlled_boost(
    state: &GameState,
    recipient: ObjectId,
) -> Result<(i128, i128), String> {
    let Some(target) = state.objects.try_get(recipient) else {
        return Err("Kinship recipient absent".into());
    };
    if target.zone != Zone::Battlefield
        || !crate::engine::object_has_type(state, recipient, CardType::Creature)
    {
        return Ok((0, 0));
    }
    let mut total = 0i128;
    for (id, live) in state.objects.iter() {
        if live.zone != Zone::Battlefield
            || live.controller != target.controller
            || !owns_definition(live.card_def)
            || !crate::continuous_characteristics_v1::printed_abilities_active(state, id)
        {
            continue;
        }
        let chosen = authenticated_type(state, id)?;
        total = total
            .checked_add(
                quote_controlled_boost(
                    state,
                    ObjectLinkV4 {
                        object: id,
                        zone_change_count: live.zone_change_count,
                    },
                    chosen,
                    recipient,
                )?
                .0,
            )
            .ok_or("Kinship anthem sum overflow")?;
    }
    Ok((total, total))
}

fn foreign_fields_absent(d: &card_def::CardDef) -> bool {
    closed_fields(d);
    d.object_name == d.name
        && !d.is_land
        && d.generic_cost_reduction.is_none()
        && d.ward_cost.is_none()
        && !d.spell_cannot_be_countered
        && !d.records_mana_spent
        && d.equipment.is_none()
        && d.starting_loyalty.is_none()
        && d.enters_with_plus_one_counters.is_none()
        && d.controlled_counter_keyword.is_none()
        && d.game_outcome_protection_v1.is_none()
        && d.controlled_creature_grant.is_none()
        && d.activated_abilities.is_empty()
        && (d.mana_ability)().is_none()
        && d.alt_cost.is_none()
        && d.kicker_cost.is_none()
        && d.additional_cost.is_none()
        && d.flashback.is_none()
        && d.plot_cost.is_none()
        && d.madness_cost.is_none()
        && d.mode2.is_none()
        && d.mode3.is_none()
        && d.additional_modes.is_empty()
        && d.kicked_target_spec.is_none()
        && d.escape.is_none()
        && d.mana_ability_choices.is_empty()
        && !d.enters_battlefield_tapped
        && d.mana_ability_def.is_none()
        && d.minimum_blockers == 1
        && d.omen.is_none()
        && !d.mana_ability_includes_chosen_color
        && d.as_enters_choose_color_other_than.is_none()
        && d.additional_mana_abilities.is_empty()
        && d.enters_battlefield_tapped_unless.is_none()
        && d.attachment.is_none()
        && d.transform_face.is_none()
        && d.saga.is_none()
        && d.optional_additional_cost.is_none()
        && !d.changeling
        && d.bestow.is_none()
        && !d.delve
        && d.adventure.is_none()
        && !d.cant_be_blocked_by_monarchs_creatures
        && d.conditional_tap_yield.is_none()
        && d.enters_tapped_unless_controller.is_none()
        && d.additional_mana_ability_conditions.is_empty()
        && d.animation.is_none()
        && d.activated_ability_generic_reductions.is_empty()
        && d.combat_static_rules == card_def::CombatStaticRulesDef::NONE
        && d.as_enters_color_choice_v1.is_none()
        && d.chosen_color_creature_boost_v1.is_none()
        && d.damage_replacement_v1.is_none()
        && d.starting_life_self_bonuses_v1.is_empty()
        && d.source_type_hexproof_v1.is_none()
        && d.other_controlled_nontoken_death_v1.is_none()
        && d.produces_mana.is_empty()
        && d.restricted_mana_abilities.is_empty()
        && d.token_counter_replacement_v48.is_none()
}
fn closed_fields(d: &card_def::CardDef) {
    let card_def::CardDef {
        name: _,
        capability: _,
        cost: _,
        generic_cost_reduction: _,
        ward_cost: _,
        spell_cannot_be_countered: _,
        records_mana_spent: _,
        equipment: _,
        types: _,
        subtypes: _,
        supertypes: _,
        power: _,
        toughness: _,
        starting_loyalty: _,
        enters_with_plus_one_counters: _,
        controlled_counter_keyword: _,
        game_outcome_protection_v1: _,
        controlled_creature_grant: _,
        is_land: _,
        produces_mana: _,
        colors: _,
        target_spec: _,
        keywords: _,
        spell_effect: _,
        mana_ability: _,
        alt_cost: _,
        kicker_cost: _,
        additional_cost: _,
        flashback: _,
        activated_abilities: _,
        plot_cost: _,
        madness_cost: _,
        mode2: _,
        mode3: _,
        additional_modes: _,
        kicked_target_spec: _,
        is_token: _,
        escape: _,
        mana_ability_choices: _,
        enters_battlefield_tapped: _,
        mana_ability_def: _,
        minimum_blockers: _,
        omen: _,
        mana_value: _,
        mana_ability_includes_chosen_color: _,
        as_enters_choose_color_other_than: _,
        additional_mana_abilities: _,
        object_name: _,
        enters_battlefield_tapped_unless: _,
        attachment: _,
        transform_face: _,
        saga: _,
        optional_additional_cost: _,
        changeling: _,
        bestow: _,
        delve: _,
        adventure: _,
        cant_be_blocked_by_monarchs_creatures: _,
        conditional_tap_yield: _,
        enters_tapped_unless_controller: _,
        additional_mana_ability_conditions: _,
        restricted_mana_abilities: _,
        animation: _,
        activated_ability_generic_reductions: _,
        combat_static_rules: _,
        as_enters_color_choice_v1: _,
        chosen_color_creature_boost_v1: _,
        damage_replacement_v1: _,
        token_counter_replacement_v48: _,
        starting_life_self_bonuses_v1: _,
        source_type_hexproof_v1: _,
        other_controlled_nontoken_death_v1: _,
        creature_type_entry_v1: _,
    } = d;
}
