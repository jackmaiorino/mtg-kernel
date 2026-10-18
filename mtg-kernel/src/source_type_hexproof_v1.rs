//! Typed effective source types shared by announcement, resolution and LKI.
//! Optional projections are selected by the catalog's source-type hexproof rule.
use crate::card_def::{CardDef, CardType, SourceTypeHexproofDefV1, CARD_DEFS};
use crate::ids::{ObjectId, PlayerId};
use crate::state::{CastMethodV4, GameState, Zone};
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

const TYPES: [CardType; 7] = [
    CardType::Land,
    CardType::Creature,
    CardType::Instant,
    CardType::Sorcery,
    CardType::Artifact,
    CardType::Enchantment,
    CardType::Planeswalker,
];

/// A set of effective card types. Bits follow this module's explicit TYPES
/// table, independently of enum discriminants. Unknown bits are rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SourceTypesV1(u8);
impl SourceTypesV1 {
    pub fn contains(self, kind: CardType) -> bool {
        TYPES
            .iter()
            .position(|t| *t == kind)
            .is_some_and(|i| self.0 & (1 << i) != 0)
    }
    pub fn card_types(self) -> Vec<CardType> {
        TYPES.into_iter().filter(|t| self.contains(*t)).collect()
    }
    fn from_types(types: &[CardType]) -> Self {
        Self(TYPES.iter().enumerate().fold(0, |mask, (i, t)| {
            mask | if types.contains(t) { 1 << i } else { 0 }
        }))
    }
    pub(crate) fn valid(self) -> bool {
        self.0 & !0x7f == 0
    }
}

/// Empty contributes no historical JSON member or Hash input. A present empty
/// set is distinct from absence, as a source may legitimately have no types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SourceTypeProjectionV1(pub Option<SourceTypesV1>);
impl SourceTypeProjectionV1 {
    pub fn is_empty(&self) -> bool {
        self.0.is_none()
    }
}
impl Hash for SourceTypeProjectionV1 {
    fn hash<H: Hasher>(&self, h: &mut H) {
        if let Some(types) = self.0 {
            "effective-targeting-source-types/v1".hash(h);
            types.hash(h);
        }
    }
}

/// Raw inputs to the effective-type query, frozen before a departure batch.
/// The source object itself is independently held by private_snapshot_json.
/// Only transitive attachment and suppression references are copied here.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceTypeInputV1 {
    arena_len: u32,
    dependencies: Vec<(ObjectId, crate::state::GameObject)>,
    active_player: PlayerId,
    planeswalkers: Option<crate::planeswalker_v1::PlaneswalkersV1>,
    crewed: Vec<(ObjectId, u32)>,
}
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SourceTypeInputProjectionV1(pub Option<Box<SourceTypeInputV1>>);
impl SourceTypeInputProjectionV1 {
    pub fn is_empty(&self) -> bool {
        self.0.is_none()
    }
}
impl Hash for SourceTypeInputProjectionV1 {
    fn hash<H: Hasher>(&self, h: &mut H) {
        if let Some(input) = &self.0 {
            "effective-source-type-input/v1".hash(h);
            input.hash(h);
        }
    }
}
fn type_dependencies(object: &crate::state::GameObject) -> Vec<ObjectId> {
    let mut ids = object.attachments.clone();
    if let Some(host) = object.v4.attached_to {
        ids.push(host.object);
    }
    if let Some(upgrade) = &object.v4.creature_upgrade {
        ids.extend(upgrade.suppressed_by.iter().map(|e| e.source.object));
    }
    ids.sort_unstable();
    ids.dedup();
    ids
}
pub(crate) fn capture_type_input(
    state: &GameState,
    source: ObjectId,
) -> Result<SourceTypeInputV1, String> {
    let root = state
        .objects
        .try_get(source)
        .ok_or("type input source missing")?;
    let mut pending = type_dependencies(root);
    let mut seen = std::collections::BTreeSet::from([source]);
    let mut dependencies = std::collections::BTreeMap::new();
    while let Some(id) = pending.pop() {
        if !seen.insert(id) {
            continue;
        }
        let object = state
            .objects
            .try_get(id)
            .ok_or("type input dependency missing")?;
        pending.extend(type_dependencies(object));
        dependencies.insert(id, object.clone());
    }
    Ok(SourceTypeInputV1 {
        arena_len: u32::try_from(state.objects.len()).map_err(|_| "source type arena overflow")?,
        dependencies: dependencies.into_iter().collect(),
        active_player: state.active_player,
        planeswalkers: state.planeswalkers_v1.clone(),
        crewed: crate::standard_cards_v1::source_type_crew_input(state),
    })
}

/// History validation runs this for every retained departure on every
/// decision, and each run replays a scratch arena. The result depends only on
/// the snapshot and, through the single `arena_len > arena_bound` check, on
/// the bound. A snapshot that passed once therefore passes again exactly when
/// its arena length is within the new bound; every other case, including
/// every failure, takes the full check below.
pub(crate) fn validate_snapshot_types(
    snapshot: &crate::controlled_creature_grants_v1::DepartureColorV1,
    arena_bound: usize,
) -> Result<(), String> {
    type Passed = std::collections::HashSet<crate::controlled_creature_grants_v1::DepartureColorV1>;
    thread_local! {
        static PASSED: std::cell::RefCell<Passed> = std::cell::RefCell::default();
    }
    let within_bound = snapshot
        .source_type_input_v1
        .0
        .as_ref()
        .is_some_and(|input| input.arena_len as usize <= arena_bound);
    if within_bound && PASSED.with(|passed| passed.borrow().contains(snapshot)) {
        return Ok(());
    }
    validate_snapshot_types_full(snapshot, arena_bound)?;
    PASSED.with(|passed| {
        let mut passed = passed.borrow_mut();
        if passed.len() >= 4096 {
            passed.clear();
        }
        passed.insert(snapshot.clone());
    });
    Ok(())
}

fn validate_snapshot_types_full(
    snapshot: &crate::controlled_creature_grants_v1::DepartureColorV1,
    arena_bound: usize,
) -> Result<(), String> {
    let input = snapshot
        .source_type_input_v1
        .0
        .as_ref()
        .ok_or("source types lost independent prebatch inputs")?;
    let captured: crate::state::GameObject = serde_json::from_str(&snapshot.private_snapshot_json)
        .map_err(|_| "source type input object JSON malformed")?;
    if serde_json::to_string(&captured).map_err(|_| "source type input serialization")?
        != snapshot.private_snapshot_json
        || usize::from(captured.card_def) >= CARD_DEFS.len()
        || captured.card_def != snapshot.source.card_def
        || captured.zone != snapshot.source.zone
        || captured.owner != snapshot.source.owner
        || captured.controller != snapshot.source.controller
        || captured.zone_change_count != snapshot.source.zone_change_count
        || input.arena_len as usize > arena_bound
        || snapshot.source.source.0 >= input.arena_len
        || !input
            .dependencies
            .windows(2)
            .all(|pair| pair[0].0 < pair[1].0)
        || input.dependencies.iter().any(|(id, object)| {
            *id == snapshot.source.source
                || id.0 >= input.arena_len
                || usize::from(object.card_def) >= CARD_DEFS.len()
        })
    {
        return Err("source type prebatch input changed source or dependency identities".into());
    }
    let nodes = std::iter::once((snapshot.source.source, &captured))
        .chain(input.dependencies.iter().map(|(id, object)| (*id, object)))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut needed = std::collections::BTreeSet::from([snapshot.source.source]);
    let mut pending = type_dependencies(&captured);
    while let Some(id) = pending.pop() {
        if !needed.insert(id) {
            continue;
        }
        let object = nodes
            .get(&id)
            .ok_or("source type input erased a transitive dependency")?;
        pending.extend(type_dependencies(object));
    }
    if needed.len() != nodes.len() {
        return Err("source type input added unrelated dependencies".into());
    }
    let mut replay = GameState::new_from_libraries(&[], &[], |_| String::new(), 0);
    let mut absent = captured.clone();
    absent.zone = Zone::Exile;
    absent.attachments.clear();
    absent.v4 = crate::state::ObjectStateV4::from_card_def(absent.card_def);
    absent.spell_copy_origin = None;
    for _ in 0..input.arena_len {
        replay.objects.push(absent.clone());
    }
    for (id, object) in nodes {
        *replay.objects.get_mut(id) = object.clone();
        if object.zone == Zone::Battlefield {
            replay.players[object.controller.index()]
                .battlefield
                .push(id);
        }
    }
    replay.active_player = input.active_player;
    replay.planeswalkers_v1 = input.planeswalkers.clone();
    crate::standard_cards_v1::restore_source_type_crew_input(&mut replay, input.crewed.clone());
    let expected = current_types(&replay, snapshot.source.source)?;
    if snapshot.source_types_v1.0 != Some(expected) {
        return Err("source type projection differs from original prebatch inputs".into());
    }
    Ok(())
}

pub(crate) fn enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        CARD_DEFS
            .iter()
            .any(|d| d.is_executable() && d.source_type_hexproof_v1.is_some())
    })
}

pub(crate) fn spell_types(def: &CardDef, method: CastMethodV4) -> Result<SourceTypesV1, String> {
    if method == CastMethodV4::Omen {
        return def
            .omen_spell_form()
            .map(|(_, types)| SourceTypesV1::from_types(types))
            .ok_or_else(|| "source-type projection lost selected Adventure/Omen form".into());
    }
    if method == CastMethodV4::Bestow {
        return Ok(SourceTypesV1::from_types(&[CardType::Enchantment]));
    }
    Ok(SourceTypesV1::from_types(def.types))
}

/// Copies bind the copied form in immutable origin provenance, rather than
/// consulting a parent object's later incarnation or a mutable stack flag.
/// Callers must supply a registered definition and its validated selected form.
/// This is characteristic data, not independent copy-creation authentication.
pub fn copy_projection(def: u16, method: CastMethodV4) -> SourceTypeProjectionV1 {
    SourceTypeProjectionV1(enabled().then(|| {
        spell_types(&CARD_DEFS[usize::from(def)], method)
            .expect("validated spell form at genuine copy creation")
    }))
}
pub(crate) fn copy_method(method: CastMethodV4) -> CastMethodV4 {
    copy_method_for_selection(method, enabled())
}
fn copy_method_for_selection(method: CastMethodV4, selected: bool) -> CastMethodV4 {
    if !selected {
        return CastMethodV4::Normal;
    }
    match method {
        CastMethodV4::Omen | CastMethodV4::Bestow => method,
        _ => CastMethodV4::Normal,
    }
}

pub(crate) fn current_types(state: &GameState, object: ObjectId) -> Result<SourceTypesV1, String> {
    let live = state
        .objects
        .try_get(object)
        .ok_or("source-type object missing")?;
    if live.v4.face_down_v1.is_some() {
        return Ok(SourceTypesV1::from_types(
            if live.v4.face_down_v1.is_some_and(|f| f.disguised) {
                &[CardType::Creature]
            } else {
                &[]
            },
        ));
    }
    if live.zone != Zone::Stack {
        return Ok(SourceTypesV1(TYPES.iter().enumerate().fold(
            0,
            |mask, (i, kind)| {
                mask | if crate::engine::object_has_type(state, object, *kind) {
                    1 << i
                } else {
                    0
                }
            },
        )));
    }
    if let Some(copy) = live.spell_copy_origin {
        if let Some(types) = copy.source_types_v1.0 {
            if !types.valid() {
                return Err("copy source type set has unknown bits".into());
            }
            return Ok(types);
        }
        if enabled() {
            return Err("copy lost immutable source-type projection".into());
        }
    }
    let def = &CARD_DEFS[usize::from(live.card_def)];
    if let Some(pending) = state
        .engine
        .pending_cast
        .as_ref()
        .filter(|p| p.spell == object)
    {
        let method = if pending.mode_chosen == Some(1) && def.omen_spell_form().is_some() {
            CastMethodV4::Omen
        } else {
            pending.source_contract.cast_method
        };
        return spell_types(def, method);
    }
    let method = live
        .v4
        .spell_cast_origin
        .and_then(|o| o.finalized_method)
        .or_else(|| {
            state
                .stack
                .iter()
                .find(|i| i.source == object && i.kind == crate::state::StackItemKind::Spell)
                .and_then(|i| i.v4.cast_method)
        })
        .ok_or("stack source lost selected spell-form authority")?;
    spell_types(def, method)
}

/// Original live characteristics win. Departed battlefield or physical spell
/// incarnations read the independently committed, prebatch type snapshot.
pub(crate) fn source_types(
    state: &GameState,
    object: ObjectId,
    card_def: u16,
    zone: Zone,
    generation: u32,
) -> Result<SourceTypesV1, String> {
    if state
        .objects
        .try_get(object)
        .is_some_and(|o| o.zone == zone && o.zone_change_count == generation)
    {
        return current_types(state, object);
    }
    crate::controlled_creature_grants_v1::validate_color_history(state)?;
    let mut found = state.engine.event_history.iter().filter_map(|e| {
        let crate::event::CommittedEvent::ProtectionSourceColorDepartureV1 { snapshot, .. } = e
        else {
            return None;
        };
        (snapshot.source.source == object
            && snapshot.source.zone == zone
            && snapshot.source.zone_change_count == generation
            && (snapshot.source.card_def == card_def
                || crate::standard_cards_v1::had_card_def(state, object, generation, card_def)))
        .then_some(snapshot.source_types_v1.0)
        .flatten()
    });
    let types = found
        .next()
        .ok_or("historical targeting source lost exact effective types")?;
    if found.next().is_some() || !types.valid() {
        return Err("historical source types duplicate or invalid".into());
    }
    Ok(types)
}

/// A retained targeting reflexive also binds a ceased copy to its original
/// spell contract. Arena membership alone cannot authenticate a ceased copy.
pub(crate) fn validate_retained_spell_types(
    state: &GameState,
    source: crate::state::StackSourceContractV4,
) -> Result<(), String> {
    if !enabled() {
        return Ok(());
    }
    let expected = spell_types(&CARD_DEFS[usize::from(source.card_def)], source.cast_method)?;
    if let Some(origin) = source.spell_copy_origin {
        let object = state
            .objects
            .try_get(source.source)
            .ok_or("retained copy source missing")?;
        if object.card_def != source.card_def
            || object.owner != source.owner
            || object.zone != source.zone
            || object.zone_change_count != source.zone_change_count
            || object.spell_copy_origin != Some(origin)
            || origin.source_types_v1.0 != Some(expected)
        {
            return Err("retained copy changed original effective-type provenance".into());
        }
    } else if source_types(
        state,
        source.source,
        source.card_def,
        source.zone,
        source.zone_change_count,
    )? != expected
    {
        return Err("retained spell types differ from original selected form".into());
    }
    Ok(())
}

pub(crate) fn active_rule(
    state: &GameState,
    target: ObjectId,
    targeter: PlayerId,
) -> Option<SourceTypeHexproofDefV1> {
    if !enabled() {
        return None;
    }
    let live = state.objects.try_get(target)?;
    let def = CARD_DEFS.get(usize::from(live.card_def))?;
    active_rule_for_input(state, target, targeter, def.source_type_hexproof_v1)
}

fn active_rule_for_input(
    state: &GameState,
    target: ObjectId,
    targeter: PlayerId,
    rule: Option<SourceTypeHexproofDefV1>,
) -> Option<SourceTypeHexproofDefV1> {
    let live = state.objects.try_get(target)?;
    let def = CARD_DEFS.get(usize::from(live.card_def))?;
    (live.zone == Zone::Battlefield
        && live.controller != targeter
        && live.v4.face_index == 0
        && def.is_executable()
        && crate::continuous_characteristics_v1::printed_abilities_active(state, target))
    .then_some(rule)
    .flatten()
}
pub(crate) fn protects(rule: SourceTypeHexproofDefV1, types: SourceTypesV1) -> bool {
    match rule {
        SourceTypeHexproofDefV1::Instants => types.contains(CardType::Instant),
    }
}
pub(crate) fn protects_from_source(
    state: &GameState,
    target: ObjectId,
    targeter: PlayerId,
    source: crate::state::AbilitySourceContractV4,
) -> Result<bool, String> {
    let Some(rule) = active_rule(state, target, targeter) else {
        return Ok(false);
    };
    Ok(protects(
        rule,
        source_types(
            state,
            source.source,
            source.card_def,
            source.zone,
            source.zone_change_count,
        )?,
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SourceTypeHexproofFactsV1 {
    pub controller: PlayerId,
    pub rule: SourceTypeHexproofDefV1,
}
pub fn characteristics(state: &GameState, target: ObjectId) -> Option<SourceTypeHexproofFactsV1> {
    let controller = state.objects.try_get(target)?.controller;
    active_rule(state, target, controller.opponent())
        .map(|rule| SourceTypeHexproofFactsV1 { controller, rule })
}

#[cfg(test)]
#[path = "source_type_hexproof_v1_tests.rs"]
mod tests;
