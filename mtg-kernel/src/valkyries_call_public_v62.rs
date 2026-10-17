//! Viewer-public native Call appendix. No complete observation or executable authority.
//! Death relations carry no reconstructed printing identity or raw history index.
use crate::{
    effect::EffectOp,
    event::CommittedEvent as E,
    ids::{ObjectId, PlayerId},
    optional_trigger_public_v1 as public,
    rl::PlayerSeatV1,
    rules_meaning_valkyries_call_v62 as meaning,
    state::{AbilitySourceContractV4, Counters, GameState, ObjectLinkV4, StackItemKind, Zone},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
pub const SCHEMA: &str = "mtg-kernel/valkyries-call-public/v62";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CountersV62 {
    pub plus_one: i32,
    pub minus_one: i16,
    pub minus_toughness: i16,
    pub stun: i16,
    pub lore: i16,
    pub oil: i16,
    pub charge: i32,
    pub net: i32,
    pub bait: i32,
    pub soul: i32,
    pub incubation: i32,
    pub revival: i32,
    pub fellowship: i32,
    pub lifelink: i16,
    pub time: u8,
    pub finality: u16,
    pub loyalty: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentV62 {
    pub card: public::ReferenceV1,
    pub is_token: bool,
    pub tapped: bool,
    pub summoning_sick: bool,
    pub marked_damage: u32,
    pub counters: CountersV62,
    pub form: public::FormV1,
}
/// An original public relation, not printing authority. A later hidden incarnation
/// is never looked up to fill an original identity or a current reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncarnationV62 {
    pub object: u32,
    pub generation: u32,
    pub owner: PlayerSeatV1,
    pub controller: PlayerSeatV1,
    pub zone: Zone,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeathV62 {
    pub occurrence: u32,
    pub source: public::RootV1,
    pub departure: IncarnationV62,
    pub original_graveyard: IncarnationV62,
    pub collection_completed: bool,
    pub current_exact_graveyard_card: Option<public::ReferenceV1>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StageV62 {
    Held,
    Published,
    Resolving,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReturnV62 {
    pub stage: StageV62,
    /// Original position in the complete native stage vector, never renumbered.
    pub position: u32,
    pub source: public::RootV1,
    pub death_occurrence: u32,
    pub original_graveyard: IncarnationV62,
    pub current_exact_graveyard_card: Option<public::ReferenceV1>,
    pub return_eligible_now: bool,
    pub under_physical_owner: PlayerSeatV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CastV62 {
    pub position: u32,
    pub source: public::ReferenceV1,
    pub original_controller: PlayerSeatV1,
    pub is_uncast_copy: bool,
    pub payment_pending: bool,
    pub genuine_cast_finalized: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV62 {
    pub schema: String,
    pub producer_sha256: String,
    pub viewer: PlayerSeatV1,
    pub meaning: meaning::MeaningV62,
    pub visible_printings: Vec<public::ReferenceV1>,
    pub battlefield: [Vec<CurrentV62>; 2],
    pub casts: Vec<CastV62>,
    pub deaths: Vec<DeathV62>,
    pub returns: Vec<ReturnV62>,
    pub complete_ordinary_observation: bool,
    pub owning_consumer_admitted: bool,
}
pub fn producer_sha256() -> String {
    meaning::producer_sha256()
}
fn ordinal(value: usize) -> Result<u32, String> {
    u32::try_from(value).map_err(|_| "V62 public ordinal overflow".into())
}
fn counters(
    state: &GameState,
    id: ObjectId,
    scope: public::CounterScopeV61,
) -> Result<CountersV62, String> {
    let live = state.objects.get(id);
    let Counters {
        plus1_plus1,
        minus1_minus1,
        minus0_minus1,
        stun,
        lore,
        oil,
        charge,
        net,
        bait,
        soul,
        incubation,
        revival,
        fellowship,
        stash,
    } = live.counters;
    if scope == public::CounterScopeV61::TinybonesV72 {
        crate::tinybones_public_v72::counters(state, id)?;
    }
    if stash != 0 && scope != public::CounterScopeV61::TinybonesV72 {
        return Err("public projection does not expose stash counters".into());
    }
    let lifelink = live.v4.lifelink_keyword_counters;
    if [
        i64::from(plus1_plus1),
        i64::from(minus1_minus1),
        i64::from(minus0_minus1),
        i64::from(stun),
        i64::from(lore),
        i64::from(oil),
        i64::from(charge),
        i64::from(net),
        i64::from(bait),
        i64::from(soul),
        i64::from(incubation),
        i64::from(revival),
        i64::from(fellowship),
        i64::from(lifelink),
    ]
    .iter()
    .any(|n| *n < 0)
    {
        return Err("V62 negative native counter".into());
    }
    Ok(CountersV62 {
        plus_one: plus1_plus1,
        minus_one: minus1_minus1,
        minus_toughness: minus0_minus1,
        stun,
        lore,
        oil,
        charge,
        net,
        bait,
        soul,
        incubation,
        revival,
        fellowship,
        lifelink,
        time: live.v4.time_counters_v1,
        finality: live.v4.creature_upgrade.as_ref().map_or(0, |u| u.finality),
        loyalty: crate::planeswalker_v1::loyalty(state, id).unwrap_or(0),
    })
}
struct Binding {
    index: usize,
    source: AbilitySourceContractV4,
    graveyard: ObjectLinkV4,
    owner: PlayerId,
    public: DeathV62,
}
fn exact_graveyard(
    state: &GameState,
    link: ObjectLinkV4,
    owner: PlayerId,
    viewer: PlayerId,
) -> Option<public::ReferenceV1> {
    state
        .objects
        .try_get(link.object)
        .filter(|live| {
            live.zone == Zone::Graveyard
                && live.zone_change_count == link.zone_change_count
                && live.owner == owner
                && state.players[owner.index()]
                    .graveyard
                    .contains(&link.object)
        })
        .and_then(|_| public::reference(state, link.object, Some(viewer)))
}
fn bindings(state: &GameState, selected: u16, viewer: PlayerId) -> Result<Vec<Binding>, String> {
    let mut rows = vec![];
    let mut seen = BTreeSet::new();
    for (index, event) in state.engine.event_history.iter().enumerate() {
        let E::ControlledCreatureDeathCaptureV1 {
            source,
            ability_index,
            died,
            controller_before,
        } = event
        else {
            continue;
        };
        if source.card_def != selected {
            continue;
        }
        if *ability_index != 0
            || source.zone != Zone::Battlefield
            || source.controller != *controller_before
            || !seen.insert((
                source.source.0,
                source.zone_change_count,
                died.object.0,
                died.zone_change_count,
            ))
        {
            return Err("V62 altered or duplicate original Call death capture".into());
        }
        crate::engine::validate_historical_ability_source_contract(state, *source)?;
        crate::creature_death_capture_v1::validated_capture_death(state, index)?;
        let dead = state
            .objects
            .try_get(died.object)
            .ok_or("V62 deceased physical object absent")?;
        let generation = died
            .zone_change_count
            .checked_add(1)
            .ok_or("V62 graveyard incarnation overflow")?;
        if dead.v4.is_token
            || crate::card_def::CARD_DEFS[usize::from(dead.card_def)].is_token
            || dead.zone_change_count < generation
        {
            return Err("V62 invalid original nontoken death incarnation".into());
        }
        let collected = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .filter_map(|(at, row)| match row {
                E::ControlledCreatureDeathCaptureCollectedV1 {
                    capture_history_index,
                } if usize::try_from(*capture_history_index).ok() == Some(index) => Some(at),
                _ => None,
            })
            .collect::<Vec<_>>();
        if collected.len() > 1 || collected.first().is_some_and(|at| *at <= index) {
            return Err("V62 death capture collection order or multiplicity changed".into());
        }
        if collected.is_empty() {
            let mut logged = state
                .engine
                .event_log
                .iter()
                .enumerate()
                .filter_map(|(at, row)| (row == event).then_some(at));
            let at = logged
                .next()
                .ok_or("V62 uncollected death lost its original native log")?;
            if logged.next().is_some() {
                return Err("V62 original uncollected death log duplicated".into());
            }
            let (_, original) =
                crate::creature_death_capture_v1::pending(state, &state.engine.event_log, at)
                    .map_err(|_| "V62 original native death collection guard refused")?
                    .ok_or("V62 original native death collection absent")?;
            if original != index {
                return Err("V62 original native death collection index changed".into());
            }
        }
        let graveyard = ObjectLinkV4 {
            object: died.object,
            zone_change_count: generation,
        };
        let relation = |zone, generation, controller: PlayerId| IncarnationV62 {
            object: died.object.0,
            generation,
            owner: dead.owner.into(),
            controller: controller.into(),
            zone,
        };
        rows.push(Binding {
            index,
            source: *source,
            graveyard,
            owner: dead.owner,
            public: DeathV62 {
                occurrence: ordinal(rows.len())?,
                source: public::root(state, *source, Some(viewer))?,
                departure: relation(
                    Zone::Battlefield,
                    died.zone_change_count,
                    *controller_before,
                ),
                original_graveyard: relation(Zone::Graveyard, generation, dead.owner),
                collection_completed: collected.len() == 1,
                current_exact_graveyard_card: exact_graveyard(state, graveyard, dead.owner, viewer),
            },
        });
    }
    Ok(rows)
}
fn return_row(
    source: AbilitySourceContractV4,
    op: &EffectOp,
    stage: StageV62,
    position: usize,
    bindings: &[Binding],
) -> Result<ReturnV62, String> {
    let EffectOp::ReturnValkyriesCallCreatureV1 {
        source: captured,
        capture_history_index,
        graveyard,
        owner,
    } = op
    else {
        return Err("V62 Call continuation is not its exact bound return".into());
    };
    let binding = bindings
        .iter()
        .find(|binding| u64::try_from(binding.index).ok() == Some(*capture_history_index))
        .ok_or("V62 original death occurrence absent")?;
    if source != *captured
        || source != binding.source
        || *graveyard != binding.graveyard
        || *owner != binding.owner
        || !binding.public.collection_completed
    {
        return Err("V62 return changed its original source/death/owner relation".into());
    }
    Ok(ReturnV62 {
        stage,
        position: ordinal(position)?,
        source: binding.public.source.clone(),
        death_occurrence: binding.public.occurrence,
        original_graveyard: binding.public.original_graveyard.clone(),
        current_exact_graveyard_card: binding.public.current_exact_graveyard_card.clone(),
        return_eligible_now: binding.public.current_exact_graveyard_card.is_some(),
        under_physical_owner: (*owner).into(),
    })
}
pub fn capture(state: &GameState, viewer: PlayerId) -> Result<FactsV62, String> {
    capture_scoped(state, viewer, public::CounterScopeV61::CallV62)
}
pub(crate) fn capture_tinybones_compat_v72(
    state: &GameState,
    viewer: PlayerId,
) -> Result<FactsV62, String> {
    capture_scoped(state, viewer, public::CounterScopeV61::TinybonesV72)
}
fn capture_scoped(
    state: &GameState,
    viewer: PlayerId,
    scope: public::CounterScopeV61,
) -> Result<FactsV62, String> {
    let selected = meaning::selected_id()?;
    if state.engine.halted.is_some() {
        return Err("V62 native state is halted".into());
    }
    public::validate_state_checked(state)?;
    if let Some(pending) = &state.engine.pending_cast {
        crate::engine::validate_pending_cast(state, pending)?;
    }
    if let Some(pending) = &state.engine.pending_activation {
        crate::engine::validate_pending_activation(state, pending)?;
    }
    crate::doubling_season_v48::validate_token_journals(state)?;
    // The collector visits the complete current log, including a marker that
    // has no matching independent history row. Checking only markers already
    // found in history would silently ignore an appended altered/orphan row.
    for (position, entry) in state.engine.event_log.iter().enumerate() {
        if matches!(entry, E::ControlledCreatureDeathCaptureV1 { source, .. } if source.card_def == selected)
        {
            crate::creature_death_capture_v1::pending(state, &state.engine.event_log, position)
                .map_err(|_| "V62 current Call log lost its native death authority")?
                .ok_or("V62 current Call log lost its native collection program")?;
        }
    }
    let mut visible_printings = vec![];
    for (id, live) in state.objects.iter() {
        if (live.v4.is_token && live.zone != Zone::Battlefield)
            || (live.spell_copy_origin.is_some()
                && live.zone == Zone::Stack
                && !state.stack.iter().any(|item| item.source == id))
        {
            continue;
        }
        // Visibility and face-down normalization precede selected-printing lookup.
        if let Some(card) = public::reference(state, id, Some(viewer)) {
            if matches!(card.identity, public::IdentityV1::Known { card_id } if card_id == selected)
            {
                visible_printings.push(card);
            }
        }
    }
    let mut battlefield = [vec![], vec![]];
    let mut seen = BTreeSet::new();
    for seat in [PlayerId::P0, PlayerId::P1] {
        for &id in &state.players[seat.index()].battlefield {
            let live = state
                .objects
                .try_get(id)
                .ok_or("V62 battlefield member absent")?;
            if live.zone != Zone::Battlefield || live.controller != seat || !seen.insert(id.0) {
                return Err("V62 battlefield membership changed".into());
            }
            battlefield[seat.index()].push(CurrentV62 {
                card: public::reference(state, id, Some(viewer))
                    .ok_or("V62 public battlefield reference absent")?,
                is_token: live.v4.is_token,
                tapped: live.tapped,
                summoning_sick: live.summoning_sick,
                marked_damage: live.damage,
                counters: counters(state, id, scope)?,
                form: public::checked_form(state, id)?,
            });
        }
    }
    if state
        .objects
        .iter()
        .any(|(id, live)| live.zone == Zone::Battlefield && !seen.contains(&id.0))
    {
        return Err("V62 battlefield roster incomplete".into());
    }
    let bindings = bindings(state, selected, viewer)?;
    let mut returns = vec![];
    for (position, pending) in public::held(state).into_iter().enumerate() {
        if let Some(source) = pending.source_contract.filter(|s| s.card_def == selected) {
            returns.push(return_row(
                source,
                &pending.effect,
                StageV62::Held,
                position,
                &bindings,
            )?);
        }
    }
    let mut casts = vec![];
    for (position, item) in state.stack.iter().enumerate() {
        if state
            .engine
            .pending_effect
            .as_ref()
            .is_some_and(|pending| pending.resolving_item == *item)
        {
            continue;
        }
        if item.kind == StackItemKind::Spell {
            if let Some(card) = public::reference(state, item.source, Some(viewer)).filter(|r| matches!(r.identity, public::IdentityV1::Known { card_id } if card_id == selected)) {
                casts.push(CastV62 { position: ordinal(position)?, source: card, original_controller: item.controller.into(), is_uncast_copy: item.is_copy,
                    payment_pending: state.engine.pending_cast.as_ref().is_some_and(|p| p.spell == item.source),
                    genuine_cast_finalized: !item.is_copy && item.v4.source_contract.and_then(|s| s.spell_cast_origin).is_some_and(|o| o.finalized_method.is_some()) });
            }
        } else if let Some(source) = item
            .v4
            .ability_source_contract
            .filter(|s| s.card_def == selected)
        {
            let op = item
                .inline_effect
                .as_ref()
                .ok_or("V62 published Call return absent")?;
            returns.push(return_row(
                source,
                op,
                StageV62::Published,
                position,
                &bindings,
            )?);
        }
    }
    if let Some(pending) = &state.engine.pending_effect {
        let item = &pending.resolving_item;
        if let Some(source) = item
            .v4
            .ability_source_contract
            .filter(|s| s.card_def == selected)
        {
            crate::engine::validated_stack_item_target_spec(item, state)?;
            returns.push(return_row(
                source,
                item.inline_effect
                    .as_ref()
                    .ok_or("V62 resolving Call return absent")?,
                StageV62::Resolving,
                0,
                &bindings,
            )?);
        }
    }
    let mut owners = BTreeSet::new();
    if returns
        .iter()
        .any(|row| !owners.insert(row.death_occurrence))
    {
        return Err("V62 original death has duplicate live return owners".into());
    }
    Ok(FactsV62 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        viewer: viewer.into(),
        meaning: meaning::printed_meaning()?,
        visible_printings,
        battlefield,
        casts,
        deaths: bindings.into_iter().map(|binding| binding.public).collect(),
        returns,
        complete_ordinary_observation: false,
        owning_consumer_admitted: false,
    })
}
pub fn validate(state: &GameState, viewer: PlayerId, value: &FactsV62) -> Result<(), String> {
    if *value == capture(state, viewer)? {
        Ok(())
    } else {
        Err("V62 facts differ from the actual viewer-public state".into())
    }
}
#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;
