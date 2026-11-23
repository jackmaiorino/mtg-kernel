//! One or more en-Kor shields can redirect damage from different
//! simultaneous sources. The affected controller allocates one charge at a time.
use super::*;
use crate::eternal_legacy_e9a_v1::{read_state, statics::applicable_redirects, RedirectV1};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceChoiceV1 {
    pub receiver: ObjectLinkV4,
    pub shields: Vec<RedirectV1>,
    pub sources: Vec<ObjectLinkV4>,
}

/// All live redirect records apply to damage at this recipient. The chosen
/// packet then offers its usual alternatives, including protection and Hallow.
fn applicable_shields(
    state: &GameState,
    damage: &event::DamageProposed,
) -> Option<Vec<RedirectV1>> {
    let (source, zone) = damage_source_incarnation(state, damage)?;
    // A public object choice must still identify this source incarnation.
    let live = state.objects.try_get(source.object)?;
    if live.zone_change_count != source.zone_change_count
        || live.zone != zone
        || !matches!(
            zone,
            Zone::Battlefield | Zone::Stack | Zone::Graveyard | Zone::Command
        )
    {
        return None;
    }
    let shields: Vec<_> = applicable_redirects(state, damage)
        .map(|(_, record)| record)
        .collect();
    // Redirecting to itself spends that record and leaves the original damage
    // in place. At least one different recipient is needed to allocate damage.
    shields
        .iter()
        .any(|record| record.from != record.to)
        .then_some(shields)
}

fn points_that_can_move(shields: &[RedirectV1]) -> usize {
    shields
        .iter()
        .filter(|record| record.from != record.to)
        .count()
}

/// A single-source event can take its usual path. Admit all competing original
/// sources together before any one of them spends a shared shield.
pub(super) fn shared_targets(state: &GameState, events: &[ProposedEvent]) -> Vec<Target> {
    let mut targets = Vec::new();
    if events.len() < 2 || crate::linked_aura_operation_v1::legacy_damage_order_v1() {
        return targets;
    }
    let Some(records) = read_state(state) else {
        return targets;
    };
    for record in &records.redirects {
        let target = Target::Object(record.from.object);
        if !targets.contains(&target) {
            targets.push(target);
        }
    }
    targets.retain(|target| {
        let mut first = None;
        let mut multiple_sources = false;
        let mut total = 0u64;
        let mut charges = 0;
        for event in events {
            if let ProposedEvent::Damage(damage) = event {
                if damage.target != *target || damage.amount <= 0 {
                    continue;
                }
                let Some(shields) = applicable_shields(state, damage) else {
                    return false;
                };
                charges = points_that_can_move(&shields);
                total += damage.amount as u64;
                multiple_sources |= *first.get_or_insert(damage.source) != damage.source;
            }
        }
        multiple_sources && total > charges as u64
    });
    targets
}

/// Keep the original event ordinal and historical damage source with every
/// portion. Selecting a source changes only which packet spends the next charge.
pub(super) fn choose(
    state: &mut GameState,
    work: &[HallowPacketV1],
    first: usize,
    batch: u32,
    step: u32,
) -> Result<Option<usize>, String> {
    let item = &work[first];
    if work.len() < 2 || !item.shared_redirect {
        return Ok(Some(first));
    }
    let target = item.packet.damage.target;
    let Some(shields) = applicable_shields(state, &item.packet.damage) else {
        return Ok(Some(first));
    };
    let mut total = 0u64;
    let mut sources = Vec::new();
    for other in work.iter().filter(|p| p.packet.damage.target == target) {
        if !other.shared_redirect
            || applicable_shields(state, &other.packet.damage).as_ref() != Some(&shields)
        {
            return Ok(Some(first));
        }
        total += other.packet.damage.amount as u64;
        if !sources.contains(&other.source) {
            // The public object menu identifies one incarnation per ObjectId.
            if sources
                .iter()
                .any(|s: &ObjectLinkV4| s.object == other.source.object)
            {
                return Ok(Some(first));
            }
            sources.push(other.source);
        }
    }
    if sources.len() < 2 || total <= points_that_can_move(&shields) as u64 {
        return Ok(Some(first));
    }
    sources.sort_by_key(|source| source.object.0);
    let receiver = shields[0].from;
    let choice = HallowChoiceV1 {
        cohort: Some((
            batch,
            u32::try_from(item.event).map_err(|_| "damage source batch ordinal overflow")?,
            step,
        )),
        damage: item.packet.damage.clone(),
        source: item.source,
        source_zone: item.source_zone,
        chooser: state.objects.get(receiver.object).controller,
        beneficiaries: Vec::new(),
        circle: None,
        ordinary: Vec::new(),
        redirect_sources: Some(SourceChoiceV1 {
            receiver,
            shields,
            sources,
        }),
    };
    match crate::linked_aura_operation_v1::choose_hallow(state, choice)? {
        None => Ok(None),
        Some(HallowPreventionV1::RedirectSource(source)) => work
            .iter()
            .position(|p| p.source == source && p.packet.damage.target == target)
            .map(Some)
            .ok_or_else(|| "chosen redirect source left its damage batch".into()),
        Some(_) => Err("redirect source choice returned a prevention effect".into()),
    }
}
