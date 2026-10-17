//! Read-only native Kaito facts. Public ordinals describe authenticated occurrences;
//! they grant neither execution nor an ordinary complete consumer.
use crate::{
    ids::PlayerId,
    optional_trigger_public_v1 as public,
    rl::PlayerSeatV1,
    state::{GameState, Zone},
};
use serde::{Deserialize, Serialize};
pub const SCHEMA: &str = "mtg-kernel/kaito-public/v75";
pub type CountersV75 = crate::tinybones_public_v72::CountersV72;
pub type CurrentV75 = crate::tinybones_public_v72::CurrentV72;
pub type IncarnationV75 = crate::tinybones_public_v72::IncarnationV72;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StageV75 {
    Captured,
    Held,
    Published,
    Resolving,
    WaitingDiscard,
    Resolved,
    Countered,
    Fizzled,
    Disposed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DrawV75 {
    NotReached,
    Drawn,
    EmptyLibrary,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetV75 {
    /// Original public physical incarnation, without an inferred concealed printing.
    pub original: IncarnationV75,
    pub current_original: Option<public::ReferenceV1>,
    pub legal_now: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscardV75 {
    pub count: u32,
    pub answered: bool,
    /// Only the original controller can see the issued hand or original answer.
    pub private_choices: Option<Vec<public::ReferenceV1>>,
    pub private_chosen: Option<Vec<IncarnationV75>>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvasionV75 {
    pub target: Option<IncarnationV75>,
    pub active_on_original: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationV75 {
    pub occurrence: u32,
    pub source: public::RootV1,
    pub original_turn: u32,
    pub ability_index: u8,
    pub loyalty_before: u32,
    pub loyalty_after: u32,
    pub targets: Vec<TargetV75>,
    pub stage: StageV75,
    pub draw: DrawV75,
    pub discard: Option<DiscardV75>,
    pub evasion: Option<EvasionV75>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatV75 {
    pub occurrence: u32,
    pub observer: public::RootV1,
    /// No printing is inferred for a formerly face-down damaging creature.
    pub original_creature: IncarnationV75,
    pub current_original_creature: Option<public::ReferenceV1>,
    pub damaged_player: PlayerSeatV1,
    pub final_damage: i32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmblemV75 {
    pub occurrence: u32,
    pub activation: u32,
    pub controller: PlayerSeatV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CastV75 {
    pub occurrence: u32,
    pub emblem: u32,
    /// One actual cast can be observed independently by several emblems.
    pub actual_cast: u32,
    pub original_spell: IncarnationV75,
    pub current_original_spell: Option<public::ReferenceV1>,
    pub x_value: u16,
    pub kicked: bool,
    pub cast_method: crate::state::CastMethodV4,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum OriginV75 {
    Combat(u32),
    EmblemCast(u32),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerV75 {
    pub occurrence: u32,
    pub origin: OriginV75,
    pub source: public::RootV1,
    pub stage: StageV75,
    /// Current held-trigger or public-stack position, never a history offset.
    pub native_position: Option<u32>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum PublicationV75 {
    Activation(u32),
    Trigger(u32),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NinjaV75 {
    pub original: public::ReferenceV1,
    pub current_original: Option<public::ReferenceV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NinjaBatchV75 {
    pub occurrence: u32,
    pub publication: PublicationV75,
    pub controller: PlayerSeatV1,
    pub created: Vec<NinjaV75>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV75 {
    pub schema: String,
    pub producer_sha256: String,
    pub viewer: PlayerSeatV1,
    pub active_player: PlayerSeatV1,
    pub turn_key: u32,
    #[serde(deserialize_with = "strict")]
    pub meaning: crate::rules_meaning_kaito_v75::MeaningV75,
    #[serde(deserialize_with = "strict")]
    pub authorized_printings: Vec<public::ReferenceV1>,
    #[serde(deserialize_with = "strict")]
    pub battlefield: [Vec<CurrentV75>; 2],
    #[serde(deserialize_with = "strict")]
    pub activations: Vec<ActivationV75>,
    #[serde(deserialize_with = "strict")]
    pub combats: Vec<CombatV75>,
    #[serde(deserialize_with = "strict")]
    pub emblems: Vec<EmblemV75>,
    #[serde(deserialize_with = "strict")]
    pub casts: Vec<CastV75>,
    #[serde(deserialize_with = "strict")]
    pub triggers: Vec<TriggerV75>,
    #[serde(deserialize_with = "strict")]
    pub ninja_batches: Vec<NinjaBatchV75>,
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
            "V75 noncanonical or unknown nested field",
        ));
    }
    Ok(v)
}
pub fn producer_sha256() -> String {
    use sha2::{Digest, Sha256};
    static H: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    H.get_or_init(|| {
        let mut h = Sha256::new();
        let meaning = crate::rules_meaning_kaito_v75::producer_sha256();
        for s in [
            SCHEMA,
            include_str!("kaito_public_v75.rs"),
            meaning.as_str(),
        ] {
            h.update((s.len() as u64).to_le_bytes());
            h.update(s.as_bytes());
        }
        format!("{:x}", h.finalize())
    })
    .clone()
}
pub fn capture(s: &GameState, v: PlayerId) -> Result<FactsV75, String> {
    crate::effect::kaito_cunning_v75::public_facts_v75(s, v)
}
/// Shape and internal ordinal checks only. Complete inventory/freshness requires recapture.
pub fn validate(f: &FactsV75) -> Result<(), String> {
    if f.schema != SCHEMA
        || f.producer_sha256 != producer_sha256()
        || f.complete_ordinary_observation
        || f.owning_consumer_admitted
    {
        return Err("V75 facts identity/admission changed".into());
    }
    crate::rules_meaning_kaito_v75::validate(&f.meaning)?;
    for r in &f.authorized_printings {
        if !matches!(r.identity, public::IdentityV1::Known { card_id: 494 | 495 }) {
            return Err("V75 authorized printing changed".into());
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    for (p, side) in f.battlefield.iter().enumerate() {
        for c in side {
            if c.card.zone != Zone::Battlefield
                || c.card.controller
                    != PlayerSeatV1::from(if p == 0 { PlayerId::P0 } else { PlayerId::P1 })
                || !seen.insert((c.card.object, c.card.generation))
            {
                return Err("V75 battlefield roster changed".into());
            }
            crate::tinybones_public_v72::nonnegative(&c.counters)?;
        }
    }
    for (i, a) in f.activations.iter().enumerate() {
        let after = match a.ability_index {
            0 => a.loyalty_before.checked_add(1),
            1 => a.loyalty_before.checked_sub(2),
            2 => a.loyalty_before.checked_sub(9),
            _ => None,
        };
        if a.occurrence as usize != i
            || after != Some(a.loyalty_after)
            || a.source.original_source.zone != Zone::Battlefield
            || !matches!(
                a.source.original_source.identity,
                public::IdentityV1::Known { card_id: 494 }
            )
            || a.source.original_source.controller != a.source.captured_controller
            || a.targets.len() > usize::from(a.ability_index == 0)
            || matches!(a.stage, StageV75::Captured | StageV75::Held)
            || a.ability_index != 0
                && (a.draw != DrawV75::NotReached || a.discard.is_some() || a.evasion.is_some())
        {
            return Err("V75 paid activation relation changed".into());
        }
        for t in &a.targets {
            if t.original.zone != Zone::Battlefield
                || t.original.controller != a.source.captured_controller
                || t.legal_now && t.current_original.is_none()
            {
                return Err("V75 original target changed".into());
            }
            current(&t.original, t.current_original.as_ref())?;
        }
        if let Some(e) = &a.evasion {
            if e.target != a.targets.first().map(|t| t.original.clone())
                || e.active_on_original && e.target.is_none()
            {
                return Err("V75 evasion relation changed".into());
            }
        }
        if let Some(d) = &a.discard {
            if d.count != 1
                || a.draw != DrawV75::Drawn
                || d.private_choices.is_some()
                    && (f.viewer != a.source.captured_controller || d.answered)
                || d.private_chosen.is_some()
                    && (f.viewer != a.source.captured_controller || !d.answered)
                || d.private_chosen
                    .as_ref()
                    .is_some_and(|v| v.len() != 1 || v[0].zone != Zone::Hand)
                || a.stage == StageV75::WaitingDiscard && d.answered
            {
                return Err("V75 discard role/privacy changed".into());
            }
        }
    }
    for (i, c) in f.combats.iter().enumerate() {
        if c.occurrence as usize != i
            || c.final_damage <= 0
            || c.original_creature.zone != Zone::Battlefield
            || c.original_creature.controller != c.observer.captured_controller
            || !matches!(
                c.observer.original_source.identity,
                public::IdentityV1::Known { card_id: 494 }
            )
        {
            return Err("V75 combat relation changed".into());
        }
        current(&c.original_creature, c.current_original_creature.as_ref())?;
    }
    for (i, e) in f.emblems.iter().enumerate() {
        let a = f
            .activations
            .get(e.activation as usize)
            .ok_or("V75 emblem activation absent")?;
        if e.occurrence as usize != i
            || a.ability_index != 2
            || a.stage != StageV75::Resolved
            || e.controller != a.source.captured_controller
        {
            return Err("V75 emblem relation changed".into());
        }
    }
    for (i, c) in f.casts.iter().enumerate() {
        if c.occurrence as usize != i
            || c.emblem as usize >= f.emblems.len()
            || c.original_spell.zone != Zone::Stack
        {
            return Err("V75 cast relation changed".into());
        }
        current(&c.original_spell, c.current_original_spell.as_ref())?;
    }
    let mut origins = std::collections::BTreeSet::new();
    for (i, t) in f.triggers.iter().enumerate() {
        let source = match t.origin {
            OriginV75::Combat(k) => {
                &f.combats
                    .get(k as usize)
                    .ok_or("V75 combat origin absent")?
                    .observer
            }
            OriginV75::EmblemCast(k) => {
                let c = f.casts.get(k as usize).ok_or("V75 cast origin absent")?;
                &f.activations[f.emblems[c.emblem as usize].activation as usize].source
            }
        };
        let key = match t.origin {
            OriginV75::Combat(k) => (0, k),
            OriginV75::EmblemCast(k) => (1, k),
        };
        if !origins.insert(key)
            || t.occurrence as usize != i
            || t.source != *source
            || t.stage == StageV75::WaitingDiscard
            || t.native_position.is_some()
                != matches!(t.stage, StageV75::Held | StageV75::Published)
        {
            return Err("V75 trigger role changed".into());
        }
    }
    if origins.len() != f.combats.len() + f.casts.len() {
        return Err("V75 missing native trigger role".into());
    }
    let mut batches = std::collections::BTreeSet::new();
    for (i, b) in f.ninja_batches.iter().enumerate() {
        let controller = match b.publication {
            PublicationV75::Activation(k) => {
                let a = f
                    .activations
                    .get(k as usize)
                    .ok_or("V75 Ninja activation absent")?;
                if a.ability_index != 1 || a.stage != StageV75::Resolved {
                    return Err("V75 Ninja activation not resolved".into());
                }
                a.source.captured_controller
            }
            PublicationV75::Trigger(k) => {
                let t = f
                    .triggers
                    .get(k as usize)
                    .ok_or("V75 Ninja trigger absent")?;
                if !matches!(t.origin, OriginV75::EmblemCast(_)) || t.stage != StageV75::Resolved {
                    return Err("V75 Ninja trigger not resolved".into());
                }
                t.source.captured_controller
            }
        };
        let key = match b.publication {
            PublicationV75::Activation(k) => (0, k),
            PublicationV75::Trigger(k) => (1, k),
        };
        if !batches.insert(key)
            || b.occurrence as usize != i
            || b.controller != controller
            || b.created.is_empty()
        {
            return Err("V75 Ninja batch changed".into());
        }
        for n in &b.created {
            if !matches!(
                n.original.identity,
                public::IdentityV1::Known { card_id: 495 }
            ) || n.original.zone != Zone::Battlefield
                || n.original.generation != 0
                || n.original.controller != controller
                || n.original.owner != controller
            {
                return Err("V75 original Ninja changed".into());
            }
            let c = IncarnationV75 {
                object: n.original.object,
                generation: 0,
                owner: controller,
                controller,
                zone: Zone::Battlefield,
            };
            current(&c, n.current_original.as_ref())?;
        }
    }
    for a in &f.activations {
        if a.stage == StageV75::Resolved
            && a.ability_index == 1
            && !batches.contains(&(0, a.occurrence))
        {
            return Err("V75 resolved Ninja activation lost its batch".into());
        }
        if f.emblems
            .iter()
            .filter(|e| e.activation == a.occurrence)
            .count()
            != usize::from(a.stage == StageV75::Resolved && a.ability_index == 2)
        {
            return Err("V75 emblem inventory differs from resolved activations".into());
        }
    }
    for t in &f.triggers {
        if t.stage == StageV75::Resolved
            && matches!(t.origin, OriginV75::EmblemCast(_))
            && !batches.contains(&(1, t.occurrence))
        {
            return Err("V75 resolved emblem trigger lost its Ninja batch".into());
        }
    }
    let mut actual = std::collections::BTreeMap::new();
    let mut observed = std::collections::BTreeSet::new();
    for c in &f.casts {
        if !observed.insert((c.emblem, c.actual_cast)) {
            return Err("V75 duplicate emblem cast role".into());
        }
        if let Some(old) = actual.get(&c.actual_cast) {
            let old: &&CastV75 = old;
            if old.original_spell != c.original_spell
                || old.x_value != c.x_value
                || old.kicked != c.kicked
                || old.cast_method != c.cast_method
            {
                return Err("V75 shared actual cast contracts disagree".into());
            }
        } else {
            if c.actual_cast as usize != actual.len() {
                return Err("V75 actual cast ordinal not dense".into());
            }
            actual.insert(c.actual_cast, c);
        }
    }
    Ok(())
}
fn current(o: &IncarnationV75, c: Option<&public::ReferenceV1>) -> Result<(), String> {
    if c.is_some_and(|c| {
        c.object != o.object
            || c.generation != o.generation
            || c.zone != o.zone
            || c.owner != o.owner
    }) {
        return Err("V75 current original incarnation changed".into());
    }
    Ok(())
}
pub fn validate_current(s: &GameState, v: PlayerId, f: &FactsV75) -> Result<(), String> {
    validate(f)?;
    if *f != capture(s, v)? {
        return Err("V75 facts do not match exact current native inventory/viewer".into());
    }
    Ok(())
}
