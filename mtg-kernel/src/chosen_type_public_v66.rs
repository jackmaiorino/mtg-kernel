//! Read-only viewer-public chosen-type appendix. No complete observation or authority.
//! Native checkpoints and historical indices remain private. Earlier unrevealed
//! APNAP answers are never projected. Floating units identify their original
//! entry, not a later hidden incarnation or an invented production receipt.
use crate::{
    banner_of_kinship_v1::{
        self,
        entry::{self, JournalV1},
    },
    card_def::ManaSpendRestrictionDef,
    creature_types_v60::CreatureTypeV60,
    event::CommittedEvent,
    ids::{ObjectId, PlayerId},
    linked_aura_operation_v1::GeneralChoiceV36,
    mana::ManaColor,
    optional_trigger_public_v1 as public,
    rl::PlayerSeatV1,
    rules_meaning_chosen_type_v66::{self as meaning, FamilyV66},
    state::{Counters, GameState, ObjectLinkV4, StackItemKind, Zone},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
pub const SCHEMA: &str = "mtg-kernel/chosen-creature-type-public/v66";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CountersV66 {
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
pub struct CurrentV66 {
    pub card: public::ReferenceV1,
    pub is_token: bool,
    pub tapped: bool,
    pub summoning_sick: bool,
    pub marked_damage: u32,
    pub counters: CountersV66,
    pub form: public::FormV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChoiceV66 {
    pub controller: PlayerSeatV1,
    pub entry_ordinal: u32,
    pub domain: Vec<CreatureTypeV60>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntryV66 {
    pub occurrence: u32,
    pub batch_entry_ordinal: u32,
    pub family: FamilyV66,
    pub source: public::RootV1,
    pub chosen: CreatureTypeV60,
    /// Actual native CreateToken entry. A converted spell copy can instead
    /// enter through ZoneChange; its current token status is in battlefield.
    pub created_by_token_event: bool,
    pub fellowship: Option<FellowshipEntryV66>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FellowshipEntryV66 {
    pub preentry_chosen_creature_count: i32,
    pub entry_fellowship_after_replacements: i32,
    pub captured_talent_doublings: u32,
    pub captured_seasons: Vec<public::RootV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoostV66 {
    pub recipient: public::ReferenceV1,
    pub power: i32,
    pub toughness: i32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermanentV66 {
    pub card: public::ReferenceV1,
    pub family: FamilyV66,
    pub entry_occurrence: u32,
    pub chosen: CreatureTypeV60,
    pub printed_abilities_active: bool,
    pub current_fellowship: i32,
    pub current_controlled_creature_bonuses: Vec<BoostV66>,
    /// Native feasibility, not a grant of an action or priority permission.
    pub currently_payable_mana_colors: Vec<ManaColor>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FloatingV66 {
    pub holder: PlayerSeatV1,
    /// Position in the complete native restricted pool, not a filtered ordinal.
    pub pool_ordinal: u32,
    pub color: ManaColor,
    pub chosen: CreatureTypeV60,
    pub original_entry_occurrence: u32,
    pub original_entry_controller: PlayerSeatV1,
    pub current_original_source: Option<public::ReferenceV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CastV66 {
    pub native_stack_position: u32,
    pub source: public::ReferenceV1,
    pub controller: PlayerSeatV1,
    pub payment_pending: bool,
    pub is_uncast_copy: bool,
    pub genuine_cast_finalized: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV66 {
    pub schema: String,
    pub producer_sha256: String,
    pub viewer: PlayerSeatV1,
    pub meanings: Vec<meaning::MeaningV66>,
    pub visible_printings: Vec<public::ReferenceV1>,
    pub battlefield: [Vec<CurrentV66>; 2],
    pub casts: Vec<CastV66>,
    pub active_entry_choice: Option<ChoiceV66>,
    pub completed_entries: Vec<EntryV66>,
    pub permanents: Vec<PermanentV66>,
    pub floating_chosen_type_mana: Vec<FloatingV66>,
    pub complete_ordinary_observation: bool,
    pub owning_consumer_admitted: bool,
}
pub fn producer_sha256() -> String {
    meaning::producer_sha256()
}
fn ordinal(value: usize) -> Result<u32, String> {
    u32::try_from(value).map_err(|_| "V66 public ordinal overflow".into())
}
fn family(id: u16) -> Option<FamilyV66> {
    if entry::owns_definition(id) {
        Some(FamilyV66::BannerOfKinship)
    } else if entry::owns_courtyard(id) {
        Some(FamilyV66::SecludedCourtyard)
    } else {
        None
    }
}
fn counters(state: &GameState, id: ObjectId) -> Result<CountersV66, String> {
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
    if stash != 0 {
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
        return Err("V66 negative native counter".into());
    }
    Ok(CountersV66 {
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
pub fn capture(state: &GameState, viewer: PlayerId) -> Result<FactsV66, String> {
    if !matches!(viewer, PlayerId::P0 | PlayerId::P1) {
        return Err("V66 unsupported viewer".into());
    }
    if state.engine.halted.is_some() {
        return Err("V66 cannot project an invalid native continuation".into());
    }
    let meanings = [FamilyV66::BannerOfKinship, FamilyV66::SecludedCourtyard]
        .into_iter()
        .map(meaning::printed_meaning)
        .collect::<Result<Vec<_>, _>>()?;
    public::validate_state_checked(state)?;
    // This authenticates the complete enclosing checkpoint when one exists,
    // otherwise the complete entry journal and all current selected sources.
    entry::validate_state(state)?;
    let active_entry_choice = state
        .engine
        .linked_aura_operation_v1
        .pending_kinship_v1
        .as_ref()
        .and_then(|p| match &p.choice {
            GeneralChoiceV36::KinshipV1 { choice, .. } => Some(ChoiceV66 {
                controller: choice.controller.into(),
                entry_ordinal: choice.ordinal,
                domain: CreatureTypeV60::ALL.to_vec(),
            }),
            // A later color/attachment choice can own the same actual batch.
            // Do not publish any already answered creature-type choices.
            _ => None,
        });
    let mut completed_entries = vec![];
    // Retain original native source contracts privately for exact joins below.
    let mut originals = vec![];
    for (at, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::KinshipEntryV1 { journal } = event else {
            continue;
        };
        let JournalV1::Committed { entered } = journal.as_ref() else {
            continue;
        };
        entry::validate_entered(state, entered, at)?;
        let Some(family) = family(entered.source.card_def) else {
            return Err("V66 foreign chosen-type entry".into());
        };
        let Some(JournalV1::Captured { choice, .. }) = entry::journal(state, entered.capture)
        else {
            return Err("V66 entry lost native capture".into());
        };
        completed_entries.push(EntryV66 {
            occurrence: ordinal(completed_entries.len())?,
            batch_entry_ordinal: choice.ordinal,
            family,
            source: public::root(state, entered.source, Some(viewer))?,
            chosen: entered.chosen,
            created_by_token_event: choice.original.is_none(),
            fellowship: if family == FamilyV66::BannerOfKinship {
                Some(FellowshipEntryV66 {
                    preentry_chosen_creature_count: choice.counts
                        [usize::from(entered.chosen.choice_index())],
                    entry_fellowship_after_replacements: entered.count,
                    captured_talent_doublings: choice
                        .replacement
                        .entry_talent_doublings
                        .ok_or("V66 missing native entry modifiers")?,
                    captured_seasons: choice
                        .replacement
                        .seasons
                        .iter()
                        .map(|s| public::root(state, *s, Some(viewer)))
                        .collect::<Result<Vec<_>, _>>()?,
                })
            } else {
                None
            },
        });
        originals.push(entered.source);
    }
    let mut visible_printings = vec![];
    let mut battlefield = [vec![], vec![]];
    let mut permanents = vec![];
    for (id, live) in state.objects.iter() {
        if (live.v4.is_token && live.zone != Zone::Battlefield)
            || (live.spell_copy_origin.is_some()
                && live.zone == Zone::Stack
                && !state.stack.iter().any(|item| item.source == id))
        {
            continue;
        }
        // Resolve visibility and neutral face-down identity before selecting a printing.
        let Some(card) = public::reference(state, id, Some(viewer)) else {
            continue;
        };
        if let public::IdentityV1::Known { card_id } = card.identity {
            if family(card_id).is_some() {
                visible_printings.push(card.clone());
            }
        }
    }
    let mut seen = BTreeSet::new();
    for seat in [PlayerId::P0, PlayerId::P1] {
        for &id in &state.players[seat.index()].battlefield {
            let live = state
                .objects
                .try_get(id)
                .ok_or("V66 battlefield member absent")?;
            if live.zone != Zone::Battlefield || live.controller != seat || !seen.insert(id.0) {
                return Err("V66 battlefield membership changed".into());
            }
            let card = public::reference(state, id, Some(viewer))
                .ok_or("V66 public battlefield absent")?;
            battlefield[seat.index()].push(CurrentV66 {
                card: card.clone(),
                is_token: live.v4.is_token,
                tapped: live.tapped,
                summoning_sick: live.summoning_sick,
                marked_damage: live.damage,
                counters: counters(state, id)?,
                form: public::checked_form(state, id)?,
            });
            let public::IdentityV1::Known { card_id } = card.identity else {
                continue;
            };
            let Some(family) = family(card_id) else {
                continue;
            };
            let indices = originals
                .iter()
                .enumerate()
                .filter(|(_, s)| s.source == id && s.zone_change_count == live.zone_change_count)
                .map(|(i, _)| i)
                .collect::<Vec<_>>();
            if indices.len() != 1 {
                return Err("V66 current permanent lacks unique original entry".into());
            }
            let at = indices[0];
            let chosen = entry::authenticated_type(state, id)?;
            let mut bonuses = vec![];
            if family == FamilyV66::BannerOfKinship {
                for &recipient in &state.players[live.controller.index()].battlefield {
                    if !crate::engine::object_has_type(
                        state,
                        recipient,
                        crate::card_def::CardType::Creature,
                    ) {
                        continue;
                    }
                    let (power, toughness) = banner_of_kinship_v1::quote_controlled_boost(
                        state,
                        ObjectLinkV4 {
                            object: id,
                            zone_change_count: live.zone_change_count,
                        },
                        chosen,
                        recipient,
                    )?;
                    bonuses.push(BoostV66 {
                        recipient: public::reference(state, recipient, Some(viewer))
                            .ok_or("V66 public creature absent")?,
                        power: i32::try_from(power).map_err(|_| "V66 native anthem width")?,
                        toughness: i32::try_from(toughness)
                            .map_err(|_| "V66 native anthem width")?,
                    });
                }
            }
            permanents.push(PermanentV66 {
                card,
                family,
                entry_occurrence: ordinal(at)?,
                chosen,
                printed_abilities_active:
                    crate::continuous_characteristics_v1::printed_abilities_active(state, id),
                current_fellowship: live.counters.fellowship,
                current_controlled_creature_bonuses: bonuses,
                currently_payable_mana_colors: if family == FamilyV66::SecludedCourtyard {
                    crate::engine::available_mana_ability_choices(live.controller, id, state)
                } else {
                    vec![]
                },
            });
        }
    }
    if state
        .objects
        .iter()
        .any(|(id, live)| live.zone == Zone::Battlefield && !seen.contains(&id.0))
    {
        return Err("V66 battlefield roster incomplete".into());
    }
    let mut floating_chosen_type_mana = vec![];
    for holder in [PlayerId::P0, PlayerId::P1] {
        for (pool_ordinal, unit) in state.players[holder.index()]
            .restricted_mana_pool
            .0
            .iter()
            .enumerate()
        {
            let courtyard_source = entry::owns_courtyard(unit.source_card_def)
                || originals.iter().any(|s| {
                    s.source == unit.source.object
                        && s.zone_change_count == unit.source.zone_change_count
                        && entry::owns_courtyard(s.card_def)
                });
            let chosen = match unit.restriction {
                ManaSpendRestrictionDef::ChosenCreatureTypeV60(Some(chosen)) => chosen,
                ManaSpendRestrictionDef::ChosenCreatureTypeV60(None) => {
                    return Err("V66 unresolved floating type".into())
                }
                _ if courtyard_source => {
                    return Err("V66 Courtyard unit changed its native restriction".into())
                }
                _ => continue,
            };
            if unit.color == ManaColor::C
                || entry::historical_courtyard_type(state, unit.source, unit.source_card_def)?
                    != chosen
            {
                return Err("V66 floating mana lost original colored restriction".into());
            }
            let indices = originals
                .iter()
                .enumerate()
                .filter(|(_, s)| {
                    s.source == unit.source.object
                        && s.zone_change_count == unit.source.zone_change_count
                        && s.card_def == unit.source_card_def
                })
                .map(|(i, _)| i)
                .collect::<Vec<_>>();
            if indices.len() != 1 {
                return Err("V66 floating mana lost unique original entry".into());
            }
            let at = indices[0];
            floating_chosen_type_mana.push(FloatingV66 {
                holder: holder.into(),
                pool_ordinal: ordinal(pool_ordinal)?,
                color: unit.color,
                chosen,
                original_entry_occurrence: ordinal(at)?,
                original_entry_controller: originals[at].controller.into(),
                current_original_source: completed_entries[at]
                    .source
                    .current_original_source
                    .clone(),
            });
        }
    }
    let mut casts = vec![];
    for (position, item) in state.stack.iter().enumerate() {
        if state
            .engine
            .pending_effect
            .as_ref()
            .is_some_and(|p| p.resolving_item == *item)
        {
            continue;
        }
        let Some(card) = public::reference(state, item.source, Some(viewer)) else {
            continue;
        };
        if item.kind == StackItemKind::Spell
            && matches!(card.identity, public::IdentityV1::Known { card_id: 463 })
        {
            casts.push(CastV66 {
                native_stack_position: ordinal(position)?,
                source: card,
                controller: item.controller.into(),
                payment_pending: state
                    .engine
                    .pending_cast
                    .as_ref()
                    .is_some_and(|p| p.spell == item.source),
                is_uncast_copy: item.is_copy,
                genuine_cast_finalized: !item.is_copy
                    && item
                        .v4
                        .source_contract
                        .and_then(|s| s.spell_cast_origin)
                        .is_some_and(|o| o.finalized_method.is_some()),
            });
        }
    }
    Ok(FactsV66 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        viewer: viewer.into(),
        meanings,
        visible_printings,
        battlefield,
        casts,
        active_entry_choice,
        completed_entries,
        permanents,
        floating_chosen_type_mana,
        complete_ordinary_observation: false,
        owning_consumer_admitted: false,
    })
}
pub fn validate(state: &GameState, viewer: PlayerId, value: &FactsV66) -> Result<(), String> {
    if *value == capture(state, viewer)? {
        Ok(())
    } else {
        Err("V66 exact viewer-public appendix changed".into())
    }
}
#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[path = "chosen_type_public_v66/tests.rs"]
mod tests;
