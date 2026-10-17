//! Read-only Tinybones appendix. Native recapture proves its inventory; closed
//! supplied-value checks do not grant execution or a complete family consumer.
use crate::{
    engine::tinybones_v72::{self as native, JournalV72},
    event::CommittedEvent as E,
    ids::{ObjectId, PlayerId},
    optional_trigger_public_v1 as public,
    rl::PlayerSeatV1,
    state::{AbilitySourceContractV4, GameState, StackItemKind, Zone},
};
use serde::{Deserialize, Serialize};
pub const SCHEMA: &str = "mtg-kernel/tinybones-public/v72";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CountersV72 {
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
    pub stash: i32,
    pub lifelink: i16,
    pub time: u8,
    pub finality: u16,
    pub loyalty: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentV72 {
    pub card: public::ReferenceV1,
    pub is_token: bool,
    pub tapped: bool,
    pub summoning_sick: bool,
    pub marked_damage: u32,
    pub counters: CountersV72,
    pub form: public::FormV1,
}
/// A historical physical incarnation deliberately carries no inferred printing.
/// A newer hidden slot is never consulted to fill the original identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncarnationV72 {
    pub object: u32,
    pub generation: u32,
    pub owner: PlayerSeatV1,
    pub controller: PlayerSeatV1,
    pub zone: Zone,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscardV72 {
    pub occurrence: u32,
    pub original_card: IncarnationV72,
    pub discarder: PlayerSeatV1,
    pub outcome_zone: Zone,
    pub current_original_outcome: Option<public::ReferenceV1>,
    pub observers: Vec<public::RootV1>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StageV72 {
    Held,
    Published,
    Resolving,
    Resolved,
    Countered,
    Disposed,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerV72 {
    pub discard: u32,
    pub observer: u32,
    pub source: public::RootV1,
    pub stage: StageV72,
    pub native_position: Option<u32>,
    pub stashed: Option<IncarnationV72>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StashV72 {
    pub card: public::ReferenceV1,
    pub counters: CountersV72,
    pub can_play: [bool; 2],
    pub any_type_mana: [bool; 2],
    pub current_grant_sources: [Vec<public::RootV1>; 2],
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlayStageV72 {
    Announced,
    Paid,
    LandPlayed,
    NativeAborted,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayV72 {
    pub occurrence: u32,
    pub original_card: public::ReferenceV1,
    pub permission_source: public::RootV1,
    pub actor: PlayerSeatV1,
    pub original_turn: u32,
    pub stash: i32,
    pub land: bool,
    pub stage: PlayStageV72,
    /// Exact announcement source contract; not a claim of a later pending choice.
    pub announced_form: Option<crate::state::CastMethodV4>,
    pub paid_form: Option<crate::state::CastMethodV4>,
    /// Exact finalized source binding, not a reconstructed total mana spend.
    pub paid_x: Option<u16>,
    pub variable_targeting_kicked: Option<bool>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationV72 {
    pub native_position: u32,
    pub source: public::RootV1,
    pub generic_mana: u8,
    pub colored_mana: Vec<crate::mana::ManaColor>,
    pub tap_cost: bool,
    pub sorcery_only: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV72 {
    pub schema: String,
    pub producer_sha256: String,
    pub viewer: PlayerSeatV1,
    pub active_player: PlayerSeatV1,
    pub turn_key: u32,
    pub meaning: crate::rules_meaning_tinybones_v72::MeaningV72,
    #[serde(deserialize_with = "strict")]
    pub authorized_printings: Vec<public::ReferenceV1>,
    #[serde(deserialize_with = "strict")]
    pub battlefield: [Vec<CurrentV72>; 2],
    #[serde(deserialize_with = "strict")]
    pub discards: Vec<DiscardV72>,
    #[serde(deserialize_with = "strict")]
    pub triggers: Vec<TriggerV72>,
    #[serde(deserialize_with = "strict")]
    pub stash: Vec<StashV72>,
    #[serde(deserialize_with = "strict")]
    pub plays: Vec<PlayV72>,
    #[serde(deserialize_with = "strict")]
    pub activations: Vec<ActivationV72>,
    pub complete_ordinary_observation: bool,
    pub owning_consumer_admitted: bool,
}
fn strict<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned + Serialize,
{
    let raw = serde_json::Value::deserialize(d)?;
    let v: T = serde_json::from_value(raw.clone()).map_err(serde::de::Error::custom)?;
    if serde_json::to_value(&v).map_err(serde::de::Error::custom)? != raw {
        return Err(serde::de::Error::custom(
            "V72 noncanonical or unknown nested field",
        ));
    }
    Ok(v)
}
pub fn producer_sha256() -> String {
    use sha2::{Digest, Sha256};
    static H: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    H.get_or_init(|| {
        let mut h = Sha256::new();
        let meaning = crate::rules_meaning_tinybones_v72::producer_sha256();
        for s in [
            SCHEMA,
            include_str!("tinybones_public_v72.rs"),
            meaning.as_str(),
        ] {
            h.update((s.len() as u64).to_le_bytes());
            h.update(s.as_bytes());
        }
        format!("{:x}", h.finalize())
    })
    .clone()
}
fn ord(n: usize) -> Result<u32, String> {
    u32::try_from(n).map_err(|_| "V72 public ordinal exceeds u32".into())
}
fn incarnation(s: AbilitySourceContractV4) -> IncarnationV72 {
    IncarnationV72 {
        object: s.source.0,
        generation: s.zone_change_count,
        owner: s.owner.into(),
        controller: s.controller.into(),
        zone: s.zone,
    }
}
fn original_visible(s: AbilitySourceContractV4) -> public::ReferenceV1 {
    public::ReferenceV1 {
        object: s.source.0,
        generation: s.zone_change_count,
        owner: s.owner.into(),
        controller: s.controller.into(),
        zone: s.zone,
        identity: public::IdentityV1::Known {
            card_id: s.card_def,
        },
    }
}
fn root(s: &GameState, c: AbilitySourceContractV4, v: PlayerId) -> Result<public::RootV1, String> {
    let mut r = public::root(s, c, Some(v))?;
    // A native observer or play permission was face-up and active when captured.
    // A later face-down change never rewrites that originally public source.
    r.original_source.identity = public::IdentityV1::Known {
        card_id: c.card_def,
    };
    Ok(r)
}
pub(crate) fn counters(s: &GameState, id: ObjectId) -> Result<CountersV72, String> {
    let o = s.objects.get(id);
    let crate::state::Counters {
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
    } = o.counters;
    let c = CountersV72 {
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
        stash,
        lifelink: o.v4.lifelink_keyword_counters,
        time: o.v4.time_counters_v1,
        finality: o.v4.creature_upgrade.as_ref().map_or(0, |u| u.finality),
        loyalty: crate::planeswalker_v1::loyalty(s, id).unwrap_or(0),
    };
    nonnegative(&c)?;
    Ok(c)
}
pub(crate) fn nonnegative(c: &CountersV72) -> Result<(), String> {
    if [
        c.plus_one,
        i32::from(c.minus_one),
        i32::from(c.minus_toughness),
        i32::from(c.stun),
        i32::from(c.lore),
        i32::from(c.oil),
        c.charge,
        c.net,
        c.bait,
        c.soul,
        c.incubation,
        c.revival,
        c.fellowship,
        c.stash,
        i32::from(c.lifelink),
    ]
    .iter()
    .any(|n| *n < 0)
    {
        return Err("V72 negative native counter".into());
    }
    Ok(())
}
pub fn capture(s: &GameState, v: PlayerId) -> Result<FactsV72, String> {
    native::validate_state(s)?;
    crate::effect::validate_pending_effect_choice(s)?;
    for row in &s.stack {
        crate::engine::validated_stack_item_target_spec(row, s)?;
    }
    let mut out = FactsV72 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        viewer: v.into(),
        active_player: s.active_player.into(),
        turn_key: s.turn_key_v1(),
        meaning: crate::rules_meaning_tinybones_v72::printed_meaning()?,
        authorized_printings: vec![],
        battlefield: [vec![], vec![]],
        discards: vec![],
        triggers: vec![],
        stash: vec![],
        plays: vec![],
        activations: vec![],
        complete_ordinary_observation: false,
        owning_consumer_admitted: false,
    };
    // Authorized printing discovery has its own stable arena scan. The complete
    // battlefield projection follows the actual native per-seat row order instead.
    for (id, _) in s.objects.iter() {
        if let Some(r) = public::reference(s, id, Some(v)) {
            if matches!(r.identity, public::IdentityV1::Known { card_id: 490 }) {
                out.authorized_printings.push(r);
            }
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    for seat in [PlayerId::P0, PlayerId::P1] {
        for &id in &s.players[seat.index()].battlefield {
            let o = s
                .objects
                .try_get(id)
                .ok_or("V72 battlefield member absent")?;
            if o.zone != Zone::Battlefield || o.controller != seat || !seen.insert(id.0) {
                return Err("V72 battlefield membership changed".into());
            }
            let card =
                public::reference(s, id, Some(v)).ok_or("V72 battlefield reference absent")?;
            out.battlefield[seat.index()].push(CurrentV72 {
                card,
                is_token: o.v4.is_token,
                tapped: o.tapped,
                summoning_sick: o.summoning_sick,
                marked_damage: o.damage,
                counters: counters(s, id)?,
                form: public::checked_form(s, id)?,
            });
        }
    }
    if s.objects
        .iter()
        .any(|(id, o)| o.zone == Zone::Battlefield && !seen.contains(&id.0))
    {
        return Err("V72 battlefield roster incomplete".into());
    }
    let mut discard_map = std::collections::BTreeMap::new();
    for (at, e) in s.engine.event_history.iter().enumerate() {
        if let E::TinybonesV72 { journal } = e {
            if let JournalV72::Discarded(d) = journal.as_ref() {
                let occurrence = ord(out.discards.len())?;
                discard_map.insert(at as u64, occurrence);
                let current_original_outcome = s
                    .objects
                    .try_get(d.outcome.object)
                    .filter(|o| {
                        o.zone == d.outcome.expected_zone
                            && o.zone_change_count == d.outcome.expected_zone_change_count
                    })
                    .and_then(|_| public::reference(s, d.outcome.object, Some(v)));
                out.discards.push(DiscardV72 {
                    occurrence,
                    original_card: incarnation(d.card),
                    discarder: d.discarder.into(),
                    outcome_zone: d.outcome.expected_zone,
                    current_original_outcome,
                    observers: d
                        .observers
                        .iter()
                        .map(|c| root(s, *c, v))
                        .collect::<Result<_, _>>()?,
                });
            }
        }
    }
    for (at, e) in s.engine.event_history.iter().enumerate() {
        let E::TinybonesV72 { journal } = e else {
            continue;
        };
        match journal.as_ref() {
            JournalV72::Discarded(d) => {
                for (observer, c) in d.observers.iter().enumerate() {
                    if let Some((position, _)) =
                        s.engine.pending_triggers.iter().enumerate().find(|(_, p)| {
                            p.effect
                                == crate::effect::EffectOp::TinybonesStashV72 {
                                    discard: at as u64,
                                    observer: observer as u32,
                                }
                        })
                    {
                        out.triggers.push(TriggerV72 {
                            discard: discard_map[&(at as u64)],
                            observer: ord(observer)?,
                            source: root(s, *c, v)?,
                            stage: StageV72::Held,
                            native_position: Some(ord(position)?),
                            stashed: None,
                        });
                    }
                }
            }
            JournalV72::Published {
                discard,
                observer,
                item,
            } => {
                let mut stashed = None;
                let mut terminal = None;
                for e in &s.engine.event_history {
                    if let E::TinybonesV72 { journal } = e {
                        match journal.as_ref() {
                            JournalV72::Finished {
                                publication,
                                countered,
                            } if *publication == at as u64 => {
                                terminal = Some(if *countered {
                                    StageV72::Countered
                                } else {
                                    StageV72::Resolved
                                })
                            }
                            JournalV72::Stashed {
                                publication,
                                exiled,
                                ..
                            } if *publication == at as u64 => {
                                let d = &out.discards[discard_map[discard] as usize];
                                stashed = Some(IncarnationV72 {
                                    object: exiled.object.0,
                                    generation: exiled.expected_zone_change_count,
                                    owner: d.original_card.owner,
                                    controller: d.original_card.owner,
                                    zone: Zone::Exile,
                                });
                            }
                            _ => {}
                        }
                    }
                }
                let (stage, native_position) = if let Some(stage) = terminal {
                    (stage, None)
                } else if let Some((i, _)) = s
                    .stack
                    .iter()
                    .enumerate()
                    .find(|(_, r)| *r == item.as_ref())
                {
                    (StageV72::Published, Some(ord(i)?))
                } else if s
                    .engine
                    .pending_effect
                    .as_ref()
                    .is_some_and(|p| p.resolving_item == **item)
                {
                    (StageV72::Resolving, None)
                } else if crate::engine::time_stop_v1::disposed(s, item)? {
                    (StageV72::Disposed, None)
                } else {
                    return Err("V72 publication has no authentic current role".into());
                };
                out.triggers.push(TriggerV72 {
                    discard: discard_map[discard],
                    observer: *observer,
                    source: root(
                        s,
                        item.v4
                            .ability_source_contract
                            .ok_or("V72 source missing")?,
                        v,
                    )?,
                    stage,
                    native_position,
                    stashed,
                });
            }
            JournalV72::Play(p) => {
                let mut stage = PlayStageV72::Announced;
                let mut announced_form = None;
                let mut paid_form = None;
                let mut paid_x = None;
                let mut variable_targeting_kicked = None;
                for e in &s.engine.event_history {
                    if let E::TinybonesV72 { journal } = e {
                        match journal.as_ref() {
                            JournalV72::Announced { play, source, .. } if *play == at as u64 => {
                                announced_form = Some(source.cast_method)
                            }
                            JournalV72::Paid { play, source, .. } if *play == at as u64 => {
                                stage = PlayStageV72::Paid;
                                paid_form = Some(source.cast_method);
                                paid_x =
                                    Some(source.finalized_cast_binding.map_or(0, |b| b.x_value));
                                variable_targeting_kicked = source
                                    .finalized_cast_binding
                                    .and_then(|b| b.variable_targeting_kicked);
                            }
                            JournalV72::Land { play, .. } if *play == at as u64 => {
                                stage = PlayStageV72::LandPlayed
                            }
                            JournalV72::Aborted { play, .. } if *play == at as u64 => {
                                stage = PlayStageV72::NativeAborted
                            }
                            _ => {}
                        }
                    }
                }
                out.plays.push(PlayV72 {
                    occurrence: ord(out.plays.len())?,
                    original_card: original_visible(p.card),
                    permission_source: root(s, p.source, v)?,
                    actor: p.actor.into(),
                    original_turn: p.turn,
                    stash: p.stash,
                    land: p.land,
                    stage,
                    announced_form,
                    paid_form,
                    paid_x,
                    variable_targeting_kicked,
                });
            }
            _ => {}
        }
    }
    for &id in &s.exile {
        let o = s.objects.get(id);
        if o.counters.stash > 0 {
            let card = public::reference(s, id, Some(v)).ok_or("V72 stashed public card absent")?;
            let mut sources: [Vec<public::RootV1>; 2] = [vec![], vec![]];
            for p in [PlayerId::P0, PlayerId::P1] {
                for &source in &s.players[p.index()].battlefield {
                    let c = s.objects.get(source);
                    if c.zone == Zone::Battlefield
                        && c.controller == p
                        && native::definition(c.card_def)
                        && c.v4.face_index == 0
                        && c.v4.face_down_v1.is_none()
                        && crate::continuous_characteristics_v1::printed_abilities_active(s, source)
                    {
                        sources[p.index()].push(root(
                            s,
                            AbilitySourceContractV4::capture(s, source),
                            v,
                        )?);
                    }
                }
            }
            out.stash.push(StashV72 {
                card,
                counters: counters(s, id)?,
                can_play: [
                    native::play_allowed(s, id, PlayerId::P0),
                    native::play_allowed(s, id, PlayerId::P1),
                ],
                any_type_mana: [
                    native::mana_grant(s, id, PlayerId::P0).is_some(),
                    native::mana_grant(s, id, PlayerId::P1).is_some(),
                ],
                current_grant_sources: sources,
            });
        }
    }
    for (i, row) in s.stack.iter().enumerate() {
        if row.kind == StackItemKind::ActivatedAbility
            && row.v4.activated_ability_index == Some(0)
            && row.v4.granted_by.is_none()
            && row.inline_effect == Some(native::activation())
            && row
                .v4
                .ability_source_contract
                .is_some_and(|c| native::definition(c.card_def))
        {
            out.activations.push(ActivationV72 {
                native_position: ord(i)?,
                source: root(s, row.v4.ability_source_contract.unwrap(), v)?,
                generic_mana: 3,
                colored_mana: vec![crate::mana::ManaColor::B],
                tap_cost: true,
                sorcery_only: true,
            });
        }
    }
    validate(&out)?;
    Ok(out)
}
/// Supplied-value shape validation. Inventory completeness requires recapture.
pub fn validate(v: &FactsV72) -> Result<(), String> {
    if v.schema != SCHEMA
        || v.producer_sha256 != producer_sha256()
        || v.complete_ordinary_observation
        || v.owning_consumer_admitted
    {
        return Err("V72 appendix identity/admission changed".into());
    }
    crate::rules_meaning_tinybones_v72::validate(&v.meaning)?;
    if v.authorized_printings
        .iter()
        .any(|r| !matches!(r.identity, public::IdentityV1::Known { card_id: 490 }))
    {
        return Err("V72 selected printing changed".into());
    }
    for (seat, roster) in v.battlefield.iter().enumerate() {
        let mut seen = std::collections::BTreeSet::new();
        for c in roster {
            nonnegative(&c.counters)?;
            if c.card.zone != Zone::Battlefield
                || c.card.controller
                    != PlayerSeatV1::from(if seat == 0 {
                        PlayerId::P0
                    } else {
                        PlayerId::P1
                    })
                || !seen.insert((c.card.object, c.card.generation))
            {
                return Err("V72 battlefield relation changed".into());
            }
        }
    }
    let root_shape = |r: &public::RootV1| {
        r.original_source.zone == Zone::Battlefield
            && r.original_source.controller == r.captured_controller
            && matches!(
                r.original_source.identity,
                public::IdentityV1::Known { card_id: 490 }
            )
            && r.current_original_source.as_ref().is_none_or(|c| {
                c.object == r.original_source.object
                    && c.generation == r.original_source.generation
                    && c.owner == r.original_source.owner
            })
    };
    for (i, d) in v.discards.iter().enumerate() {
        if d.occurrence != ord(i)?
            || d.original_card.zone != Zone::Hand
            || d.original_card.owner != d.discarder
            || d.observers.is_empty()
            || d.observers.iter().any(|r| !root_shape(r))
        {
            return Err("V72 discard relation changed".into());
        }
    }
    for t in &v.triggers {
        let d = v
            .discards
            .get(t.discard as usize)
            .ok_or("V72 trigger discard absent")?;
        if d.observers.get(t.observer as usize) != Some(&t.source)
            || matches!(t.stage, StageV72::Held | StageV72::Published)
                != t.native_position.is_some()
            || t.stashed.is_some() && t.stage != StageV72::Resolved
        {
            return Err("V72 trigger source/position/outcome changed".into());
        }
    }
    for c in &v.stash {
        nonnegative(&c.counters)?;
        if c.card.zone != Zone::Exile || c.counters.stash <= 0 {
            return Err("V72 stash incarnation changed".into());
        }
    }
    for c in &v.stash {
        for (i, sources) in c.current_grant_sources.iter().enumerate() {
            let seat: PlayerSeatV1 = (if i == 0 { PlayerId::P0 } else { PlayerId::P1 }).into();
            if sources
                .iter()
                .any(|r| !root_shape(r) || r.captured_controller != seat)
                || (c.can_play[i] || c.any_type_mana[i])
                    && (c.card.owner == seat || sources.is_empty())
                || c.can_play[i] && seat != v.active_player
            {
                return Err("V72 current grant/controller/turn relation changed".into());
            }
        }
    }
    for (i, p) in v.plays.iter().enumerate() {
        if p.occurrence != ord(i)?
            || p.original_card.zone != Zone::Exile
            || p.original_card.owner == p.actor
            || p.stash <= 0
            || p.permission_source.captured_controller != p.actor
            || !root_shape(&p.permission_source)
            || (p.stage == PlayStageV72::Paid) != p.paid_x.is_some()
            || p.land != (p.stage == PlayStageV72::LandPlayed)
            || p.land == p.announced_form.is_some()
            || (p.stage == PlayStageV72::Paid) != p.paid_form.is_some()
        {
            return Err("V72 play/form/source relation changed".into());
        }
    }
    for a in &v.activations {
        if !root_shape(&a.source)
            || a.generic_mana != 3
            || a.colored_mana != [crate::mana::ManaColor::B]
            || !a.tap_cost
            || !a.sorcery_only
        {
            return Err("V72 printed activation cost changed".into());
        }
    }
    Ok(())
}
/// Fresh native equality, including omissions/reordering, grants no step authority.
pub fn validate_current(s: &GameState, v: PlayerId, f: &FactsV72) -> Result<(), String> {
    validate(f)?;
    if *f != capture(s, v)? {
        return Err("V72 facts differ from complete native appendix".into());
    }
    Ok(())
}
