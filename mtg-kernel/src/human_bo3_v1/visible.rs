//! Typed human projection. Each field is selected explicitly; no serde Value
//! tree, engine object, generic debug text, or raw transport reference crosses
//! this boundary. Handles are local to one prompt and never track hidden cards
//! from a previous prompt.

use super::HumanDecisionErrorV1 as Error;
use crate::engine::{CastMode, ChosenCreatureCostZoneV1, CostKind};
use crate::mana::ManaColor;
use crate::policy_observation_v6 as v6;
use crate::rl::*;
use crate::state::{CastMethodV4, Zone};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanDecisionV1 {
    pub schema: String,
    pub human_seat: PlayerSeatV1,
    pub prompt_seq: u64,
    pub state: HumanVisibleStateV1,
    pub actions: Vec<HumanLegalActionV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanLegalActionV1 {
    pub action_index: u32,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanVisibleStateV1 {
    pub public: HumanPublicProjectionV1,
    pub own_hand: Vec<HumanPrivateCardV1>,
    pub known_library_cards: [Vec<HumanKnownLibraryCardV1>; 2],
    pub known_hand_cards: [Vec<HumanPrivateCardV1>; 2],
    pub extensions: HumanExtensionsV1,
    pub policy_context: HumanPolicyContextV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanCardRefV1 {
    pub handle: String,
    pub name: String,
    pub owner: PlayerSeatV1,
    pub controller: PlayerSeatV1,
    pub zone: Zone,
}

// Only values already established by the current V6 observation may be used
// in labels. Canonical action references cannot silently invent visible rows.
pub(super) struct Handles {
    rows: Vec<(CardStableRefV1, HumanCardRefV1)>,
    effect_timestamps: Vec<u64>,
}

impl Handles {
    fn new(observation: &v6::ObservationV6) -> Result<Self, Error> {
        let mut effect_timestamps: Vec<_> = observation
            .projection
            .surface
            .continuous_effects
            .iter()
            .filter(|effect| effect.duration != EffectDurationV2::WhileAttached)
            .map(|effect| effect.timestamp)
            .collect();
        effect_timestamps.sort_unstable();
        effect_timestamps.dedup();
        let mut handles = Self {
            rows: Vec::new(),
            effect_timestamps,
        };
        // Register the complete visible zone inventory before attachments,
        // including forward references to a later battlefield row.
        for cards in &observation.projection.surface.battlefield {
            for card in cards {
                handles.insert(&card.stable)?;
            }
        }
        for cards in &observation.projection.surface.graveyards {
            for card in cards {
                handles.insert(&card.stable)?;
            }
        }
        for card in &observation.projection.surface.exile {
            handles.insert(&card.stable)?;
        }
        for card in &observation.own_hand {
            handles.insert(&card.stable)?;
        }
        for cards in &observation.known_hand_cards {
            for card in cards {
                handles.insert(&card.stable)?;
            }
        }
        for cards in &observation.known_library_cards {
            for card in cards {
                handles.insert(&card.card.stable)?;
            }
        }
        if let Some(search) = &observation.extensions.decision_local_library {
            for card in &search.cards {
                handles.insert(&card.stable)?;
            }
        }
        Ok(handles)
    }

    fn insert(&mut self, reference: &CardStableRefV1) -> Result<HumanCardRefV1, Error> {
        if let Some((_, visible)) = self.rows.iter().find(|(key, _)| key == reference) {
            return Ok(visible.clone());
        }
        let name = crate::card_def::CARD_DEFS
            .get(reference.card_db_id as usize)
            .ok_or(Error::InvalidVisibleReference)?
            .object_name
            .to_owned();
        let visible = HumanCardRefV1 {
            handle: format!("c{}", self.rows.len() + 1),
            name,
            owner: reference.owner,
            controller: reference.controller,
            zone: reference.zone,
        };
        self.rows.push((reference.clone(), visible.clone()));
        Ok(visible)
    }

    pub(super) fn get(&self, reference: &CardStableRefV1) -> Result<&HumanCardRefV1, Error> {
        self.rows
            .iter()
            .find(|(key, _)| key == reference)
            .map(|(_, visible)| visible)
            .ok_or(Error::InvalidVisibleReference)
    }

    pub(super) fn name(&self, reference: &CardStableRefV1) -> Result<String, Error> {
        let card = self.get(reference)?;
        Ok(format!("{} [{}]", card.name, card.handle))
    }

    fn attachment(&self, arena_id: u32) -> Result<HumanCardRefV1, Error> {
        let mut matches = self
            .rows
            .iter()
            .filter(|(key, _)| key.arena_id == arena_id && key.zone == Zone::Battlefield);
        let visible = matches
            .next()
            .ok_or(Error::InvalidVisibleReference)?
            .1
            .clone();
        if matches.next().is_some() {
            return Err(Error::InvalidVisibleReference);
        }
        Ok(visible)
    }

    fn canonicalize(&mut self, state: &HumanVisibleStateV1) -> Result<(), Error> {
        // This JSON is produced solely from the explicit safe DTO above.
        // It is used internally to order safe fields, never to sanitize an
        // arbitrary engine or V6 serialization.
        let safe = serde_json::to_value(state).map_err(|_| Error::InvalidVisibleReference)?;
        let mut roles = BTreeMap::<String, Vec<String>>::new();
        let mut graph_refs = BTreeSet::<String>::new();
        collect_roles(&safe, &mut Vec::new(), &mut roles, &mut graph_refs);
        let mut attributes = BTreeMap::<String, String>::new();
        for cards in state
            .public
            .battlefield
            .iter()
            .chain(state.public.graveyards.iter())
        {
            for card in cards {
                attributes.insert(card.stable.handle.clone(), without_handles(card)?);
                if !card.attachments.is_empty() {
                    graph_refs.insert(card.stable.handle.clone());
                }
            }
        }
        for card in &state.public.exile {
            attributes.insert(card.stable.handle.clone(), without_handles(card)?);
            if !card.attachments.is_empty() {
                graph_refs.insert(card.stable.handle.clone());
            }
        }
        let mut keyed = Vec::with_capacity(self.rows.len());
        for (reference, visible) in self.rows.drain(..) {
            let mut visible_roles = roles.remove(&visible.handle).unwrap_or_default();
            visible_roles.sort();
            let key = (
                without_handles(&visible)?,
                attributes.remove(&visible.handle),
                visible_roles,
            );
            keyed.push((
                key,
                graph_refs.contains(&visible.handle),
                reference,
                visible,
            ));
        }
        keyed.sort_by(|a, b| a.0.cmp(&b.0));
        for pair in keyed.windows(2) {
            if pair[0].0 == pair[1].0 && (pair[0].1 || pair[1].1) {
                // Unreferenced identical copies are interchangeable. A tied
                // class inside an unordered reference graph needs a fuller
                // graph canonicalizer; do not choose a hidden arena tie-break.
                return Err(Error::UnsupportedPrompt);
            }
        }
        for (index, (_, _, reference, mut visible)) in keyed.into_iter().enumerate() {
            visible.handle = format!("c{}", index + 1);
            self.rows.push((reference, visible));
        }
        Ok(())
    }
}

fn without_handles<T: Serialize>(value: &T) -> Result<String, Error> {
    fn remove(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(object) => {
                object.remove("handle");
                for value in object.values_mut() {
                    remove(value);
                }
                if let Some(serde_json::Value::Array(attachments)) = object.get_mut("attachments") {
                    attachments.sort_by_cached_key(serde_json::Value::to_string);
                }
            }
            serde_json::Value::Array(array) => {
                for value in array {
                    remove(value);
                }
            }
            _ => {}
        }
    }
    let mut value = serde_json::to_value(value).map_err(|_| Error::InvalidVisibleReference)?;
    remove(&mut value);
    Ok(value.to_string())
}

fn collect_roles(
    value: &serde_json::Value,
    path: &mut Vec<String>,
    roles: &mut BTreeMap<String, Vec<String>>,
    graph: &mut BTreeSet<String>,
) {
    match value {
        serde_json::Value::Object(object) => {
            if let Some(handle) = object.get("handle").and_then(serde_json::Value::as_str) {
                let location = path.join(".");
                let inventory = location.starts_with("public.battlefield.")
                    || location.starts_with("public.exile.")
                    || location.starts_with("own_hand.")
                    || location.starts_with("known_hand_cards.")
                    || location.starts_with("extensions.decision_local_library.cards.");
                let unordered_graph = location.starts_with("public.object_relations.")
                    || location.starts_with("public.continuous_effects.")
                    || location.starts_with("public.exile_play_permissions.")
                    || location.contains(".attachments.");
                if unordered_graph {
                    graph.insert(handle.to_owned());
                }
                if !inventory && !unordered_graph {
                    let role = if let Some((prefix, _)) = location.split_once(".legal_targets.") {
                        format!("{prefix}.legal_targets")
                    } else if let Some((prefix, _)) =
                        location.split_once(".unordered_selected_targets.")
                    {
                        format!("{prefix}.selected_targets")
                    } else if let Some((prefix, _)) = location.split_once(".remaining_choices.") {
                        format!("{prefix}.remaining_choices")
                    } else {
                        location
                    };
                    roles.entry(handle.to_owned()).or_default().push(role);
                }
                return;
            }
            for (key, child) in object {
                let key = if key == "selected_targets"
                    && object.get("ordered").and_then(serde_json::Value::as_bool) == Some(false)
                {
                    "unordered_selected_targets".to_owned()
                } else {
                    key.clone()
                };
                path.push(key);
                collect_roles(child, path, roles, graph);
                path.pop();
            }
        }
        serde_json::Value::Array(array) => {
            for (index, child) in array.iter().enumerate() {
                path.push(index.to_string());
                collect_roles(child, path, roles, graph);
                path.pop();
            }
        }
        _ => {}
    }
}

fn sort_visible<T: Serialize>(values: &mut [T]) {
    // Serialization of these concrete DTOs is infallible; choosing no output
    // is preferable to putting a hidden reference into a sort key.
    values.sort_by_cached_key(|value| serde_json::to_string(value).expect("human DTO serializes"));
}

fn project_state(
    observation: &v6::ObservationV6,
    handles: &mut Handles,
) -> Result<HumanVisibleStateV1, Error> {
    let mut state = HumanVisibleStateV1 {
        public: observation.projection.surface.project(handles)?,
        own_hand: observation.own_hand.project(handles)?,
        known_library_cards: observation.known_library_cards.project(handles)?,
        known_hand_cards: observation.known_hand_cards.project(handles)?,
        extensions: observation.extensions.project(handles)?,
        policy_context: observation
            .projection
            .policy_surface_context
            .project(handles)?,
    };
    for cards in &mut state.public.battlefield {
        sort_visible(cards);
    }
    for cards in state
        .public
        .battlefield
        .iter_mut()
        .chain(state.public.graveyards.iter_mut())
    {
        for card in cards {
            sort_visible(&mut card.attachments);
        }
    }
    for card in &mut state.public.exile {
        sort_visible(&mut card.attachments);
    }
    sort_visible(&mut state.public.exile);
    sort_visible(&mut state.own_hand);
    for cards in &mut state.known_hand_cards {
        sort_visible(cards);
    }
    for cards in &mut state.known_library_cards {
        cards.sort_by_key(|card| card.position);
    }
    if let Some(search) = &mut state.extensions.decision_local_library {
        sort_visible(&mut search.cards);
    }
    sort_visible(&mut state.public.object_relations);
    sort_visible(&mut state.public.exile_play_permissions);
    if let Some(discard) = &mut state.public.surface_context.private_discard {
        sort_visible(&mut discard.remaining_choices);
    }
    if let Some(HumanEffectChoiceV1::Targets {
        legal_targets,
        selected_targets,
        ordered,
        ..
    }) = state
        .public
        .engine_context
        .pending_effect
        .as_mut()
        .and_then(|pending| pending.choice.as_mut())
    {
        sort_visible(legal_targets);
        if !*ordered {
            sort_visible(selected_targets);
        }
    }
    for effect in &mut state.public.continuous_effects {
        sort_visible(&mut effect.affected_objects);
        sort_visible(&mut effect.affected_players);
    }
    state
        .public
        .continuous_effects
        .sort_by_cached_key(|effect| {
            (
                effect.timestamp_order,
                serde_json::to_string(effect).expect("human effect serializes"),
            )
        });
    Ok(state)
}

trait Project {
    type Output;
    fn project(&self, handles: &mut Handles) -> Result<Self::Output, Error>;
}

macro_rules! leaf {
    ($($ty:ty),* $(,)?) => {$(
        impl Project for $ty {
            type Output = Self;
            fn project(&self, _: &mut Handles) -> Result<Self, Error> { Ok(self.clone()) }
        }
    )*};
}
leaf!(
    bool,
    u8,
    u16,
    u32,
    usize,
    i16,
    i32,
    PlayerSeatV1,
    Zone,
    ManaColor,
    CountersV1,
    CardCharacteristicsV2,
    AbilityUsePublicV4,
    GoadPublicV4,
    PlayerStatusV1,
    StackItemKindV2,
    EffectDurationV2,
    PlayOrCastV2,
    PlayPermissionExpiryV2,
    EngineDecisionStageV2,
    SurfaceDecisionStageV2,
    crate::policy_surface_v5::PolicySurfaceStageV5,
    CastMode,
    CastMethodV4,
    DiscardResumeSemanticV2,
    SpellCopyStageV2,
    TargetSelectionPurposeV4,
    BooleanChoicePurposeV4,
    PendingTriggerKindV2,
    CostKind,
    ChosenCreatureCostZoneV1,
    v6::HistoricalSourceContextV6,
    v6::WardPaymentV6,
    v6::QueuedWardPaymentV6,
    ZoneIndependentStepV1
);

impl<T: Project> Project for Vec<T> {
    type Output = Vec<T::Output>;
    fn project(&self, handles: &mut Handles) -> Result<Self::Output, Error> {
        self.iter().map(|value| value.project(handles)).collect()
    }
}
impl<T: Project> Project for Option<T> {
    type Output = Option<T::Output>;
    fn project(&self, handles: &mut Handles) -> Result<Self::Output, Error> {
        self.as_ref()
            .map(|value| value.project(handles))
            .transpose()
    }
}
impl<T: Project, const N: usize> Project for [T; N] {
    type Output = [T::Output; N];
    fn project(&self, handles: &mut Handles) -> Result<Self::Output, Error> {
        self.iter()
            .map(|value| value.project(handles))
            .collect::<Result<Vec<_>, _>>()?
            .try_into()
            .map_err(|_| Error::InvalidVisibleReference)
    }
}
impl<A: Project, B: Project> Project for (A, B) {
    type Output = (A::Output, B::Output);
    fn project(&self, handles: &mut Handles) -> Result<Self::Output, Error> {
        Ok((self.0.project(handles)?, self.1.project(handles)?))
    }
}
impl Project for CardStableRefV1 {
    type Output = HumanCardRefV1;
    fn project(&self, handles: &mut Handles) -> Result<Self::Output, Error> {
        handles.insert(self)
    }
}

// Explicit field lists make additions to privileged source records opt-in.
// The macro changes only reference-bearing types, never their game meaning.
macro_rules! record {
    ($output:ident, $input:ty, {$($field:ident: $ty:ty),* $(,)?}) => {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
        pub struct $output { $(pub $field: $ty),* }
        impl Project for $input {
            type Output = $output;
            fn project(&self, handles: &mut Handles) -> Result<Self::Output, Error> {
                Ok($output { $($field: self.$field.project(handles)?),* })
            }
        }
    };
}

record!(HumanPrivateCardV1, CardPrivateV1, { stable: HumanCardRefV1 });
record!(HumanKnownLibraryCardV1, KnownLibraryCardV4, { position: u32, card: HumanPrivateCardV1 });

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanPublicCardV1 {
    pub stable: HumanCardRefV1,
    pub tapped: bool,
    pub summoning_sick: bool,
    pub damage: u16,
    pub counters: CountersV1,
    pub attachments: Vec<HumanCardRefV1>,
    pub plotted_turn: Option<u32>,
    pub is_token: bool,
    pub face_index: u8,
    pub chosen_color: Option<ManaColor>,
    pub entered_battlefield_turn: Option<u32>,
    pub ability_uses_this_turn: Vec<AbilityUsePublicV4>,
    pub skip_next_untap: bool,
    pub goaded_by: Vec<GoadPublicV4>,
    pub characteristics: CardCharacteristicsV2,
}
impl Project for CardPublicV2 {
    type Output = HumanPublicCardV1;
    fn project(&self, handles: &mut Handles) -> Result<Self::Output, Error> {
        Ok(HumanPublicCardV1 {
            stable: self.stable.project(handles)?,
            tapped: self.tapped,
            summoning_sick: self.summoning_sick,
            damage: self.damage,
            counters: self.counters.clone(),
            attachments: self
                .attachments
                .iter()
                .map(|id| handles.attachment(*id))
                .collect::<Result<_, _>>()?,
            plotted_turn: self.plotted_turn,
            is_token: self.is_token,
            face_index: self.face_index,
            chosen_color: self.chosen_color,
            entered_battlefield_turn: self.entered_battlefield_turn,
            ability_uses_this_turn: self.ability_uses_this_turn.clone(),
            skip_next_untap: self.skip_next_untap,
            goaded_by: self.goaded_by.clone(),
            characteristics: self.characteristics.clone(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "target_kind", rename_all = "snake_case")]
pub enum HumanTargetV1 {
    Player { player: PlayerSeatV1 },
    Object { object: HumanCardRefV1 },
}
impl Project for TargetRefV1 {
    type Output = HumanTargetV1;
    fn project(&self, handles: &mut Handles) -> Result<Self::Output, Error> {
        Ok(match self {
            Self::Player { player } => HumanTargetV1::Player { player: *player },
            Self::Object { object } => HumanTargetV1::Object {
                object: object.project(handles)?,
            },
        })
    }
}
record!(HumanStackItemV1, StackItemPublicV2, {
    stack_index: u32, source: HumanCardRefV1, controller: PlayerSeatV1,
    targets: Vec<HumanTargetV1>, stack_item_kind: StackItemKindV2,
    is_copy: bool, is_flashback: bool, mode_chosen: u8, madness_offer: bool,
    kicked: bool, cast_method: Option<CastMethodV4>, face_index: u8, x_value: u16,
    paid_cost_refs: Vec<HumanCardRefV1>,
});
record!(HumanCombatV1, CombatStatePublicV2, {
    attackers_declared: bool, blockers_declared: bool,
    ordered_attackers: Vec<HumanCardRefV1>,
    attacker_to_ordered_blockers: Vec<(HumanCardRefV1, Vec<HumanCardRefV1>)>,
});

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanContinuousEffectV1 {
    pub source: Option<HumanCardRefV1>,
    pub controller: Option<PlayerSeatV1>,
    pub affected_objects: Vec<HumanCardRefV1>,
    pub affected_players: Vec<PlayerSeatV1>,
    pub global: bool,
    pub layers: u8,
    /// Relative chronology only. WhileAttached equipment rows have no
    /// chronology: their producer timestamp is synthesized from arena identity
    /// (rl.rs::continuous_effects_public_v2), so even ranking it would leak.
    /// Their current additive modifiers and attachment relation remain visible.
    pub timestamp_order: Option<usize>,
    pub duration: EffectDurationV2,
    pub power_delta: i32,
    pub toughness_delta: i32,
    pub grants_haste: bool,
    pub set_power: Option<i32>,
    pub set_toughness: Option<i32>,
    pub add_color_mask: u8,
    pub remove_color_mask: u8,
    pub add_subtype_ids: Vec<u16>,
    pub remove_subtype_ids: Vec<u16>,
    pub add_keyword_mask: u32,
    pub remove_keyword_mask: u32,
    pub ward_generic_delta: i16,
    pub minimum_blockers: Option<u8>,
    pub add_landwalk_mask: u8,
    pub remove_landwalk_mask: u8,
    pub prevent_damage_from_color_mask: u8,
    pub damage_cannot_be_prevented: bool,
}
impl Project for ContinuousEffectPublicV2 {
    type Output = HumanContinuousEffectV1;
    fn project(&self, handles: &mut Handles) -> Result<Self::Output, Error> {
        Ok(HumanContinuousEffectV1 {
            source: self.source.project(handles)?,
            controller: self.controller,
            affected_objects: self.affected_objects.project(handles)?,
            affected_players: self.affected_players.clone(),
            global: self.global,
            layers: self.layers,
            timestamp_order: if self.duration == EffectDurationV2::WhileAttached {
                None
            } else {
                Some(
                    handles
                        .effect_timestamps
                        .binary_search(&self.timestamp)
                        .map_err(|_| Error::InvalidVisibleReference)?,
                )
            },
            duration: self.duration,
            power_delta: self.power_delta,
            toughness_delta: self.toughness_delta,
            grants_haste: self.grants_haste,
            set_power: self.set_power,
            set_toughness: self.set_toughness,
            add_color_mask: self.add_color_mask,
            remove_color_mask: self.remove_color_mask,
            add_subtype_ids: self.add_subtype_ids.clone(),
            remove_subtype_ids: self.remove_subtype_ids.clone(),
            add_keyword_mask: self.add_keyword_mask,
            remove_keyword_mask: self.remove_keyword_mask,
            ward_generic_delta: self.ward_generic_delta,
            minimum_blockers: self.minimum_blockers,
            add_landwalk_mask: self.add_landwalk_mask,
            remove_landwalk_mask: self.remove_landwalk_mask,
            prevent_damage_from_color_mask: self.prevent_damage_from_color_mask,
            damage_cannot_be_prevented: self.damage_cannot_be_prevented,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "relation_kind", rename_all = "snake_case")]
pub enum HumanObjectRelationV1 {
    AttachedTo {
        object: HumanCardRefV1,
        attached_to: HumanCardRefV1,
    },
    ExiledBy {
        object: HumanCardRefV1,
        exiled_by: HumanCardRefV1,
    },
}
impl Project for ObjectRelationPublicV4 {
    type Output = HumanObjectRelationV1;
    fn project(&self, handles: &mut Handles) -> Result<Self::Output, Error> {
        Ok(match self {
            Self::AttachedTo {
                object,
                attached_to,
            } => HumanObjectRelationV1::AttachedTo {
                object: object.project(handles)?,
                attached_to: attached_to.project(handles)?,
            },
            Self::ExiledBy { object, exiled_by } => HumanObjectRelationV1::ExiledBy {
                object: object.project(handles)?,
                exiled_by: exiled_by.project(handles)?,
            },
        })
    }
}
// zone_change_generation is private incarnation authority, not a human field.
record!(HumanExilePermissionV1, ExilePlayPermissionPublicV2, {
    object: HumanCardRefV1, holder: PlayerSeatV1,
    play_or_cast: PlayOrCastV2, expiry: PlayPermissionExpiryV2,
});

record!(HumanPendingCastV1, PendingCastSemanticV2, {
    source: Option<HumanCardRefV1>, controller: PlayerSeatV1,
    chosen_targets: Vec<HumanTargetV1>, is_flashback: bool, cast_mode: Option<CastMode>,
    additional_cost_discarded: Option<Vec<HumanCardRefV1>>, mode_chosen: Option<u8>,
    origin_zone: Zone, sacrifice_chosen: Vec<HumanCardRefV1>, kicked: Option<bool>,
});
record!(HumanPendingActivationV1, PendingActivationSemanticV2, {
    source: Option<HumanCardRefV1>, controller: PlayerSeatV1, ability_index: u8,
    chosen_targets: Vec<HumanTargetV1>, cost_discard_paid: Option<Vec<HumanCardRefV1>>,
    object_cost_chosen: Vec<HumanCardRefV1>,
});
record!(HumanPendingDiscardV1, PendingDiscardSemanticV2, {
    player: PlayerSeatV1, count: u32, resume_stage: DiscardResumeSemanticV2,
    resume_source: Option<HumanCardRefV1>,
});
record!(HumanPendingOptionalCostV1, PendingOptionalCostSemanticV2, {
    player: PlayerSeatV1, source: Option<HumanCardRefV1>, discard_cards: u8,
    sacrifice_lands: u8, discard_payable: bool, sacrifice_payable: bool,
    spell_resume_source: Option<HumanCardRefV1>, spell_resume_zone: Option<Zone>,
});
record!(HumanPendingOptionalSacrificeV1, PendingOptionalCostSacrificeSemanticV2, {
    player: PlayerSeatV1, source: Option<HumanCardRefV1>, remaining: u8,
    chosen: Vec<HumanCardRefV1>, spell_resume_source: Option<HumanCardRefV1>, spell_resume_zone: Option<Zone>,
});
record!(HumanPendingSpellCopyV1, PendingSpellCopySemanticV2, {
    parent: Option<HumanCardRefV1>, player: PlayerSeatV1, inherited_target: HumanTargetV1,
    stage: SpellCopyStageV2, copy: Option<HumanCardRefV1>,
});

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "choice_kind", rename_all = "snake_case")]
pub enum HumanEffectChoiceV1 {
    Options {
        player: PlayerSeatV1,
        option_count: u16,
    },
    Targets {
        player: PlayerSeatV1,
        selected_targets: Vec<HumanTargetV1>,
        legal_targets: Vec<HumanTargetV1>,
        min_targets: u16,
        max_targets: u16,
        can_finish: bool,
        ordered: bool,
        purpose: TargetSelectionPurposeV4,
    },
    Color {
        player: PlayerSeatV1,
        legal_colors: Vec<ManaColor>,
    },
    Number {
        player: PlayerSeatV1,
        minimum: i32,
        maximum: i32,
    },
    Boolean {
        player: PlayerSeatV1,
        default: Option<bool>,
        purpose: BooleanChoicePurposeV4,
    },
}
impl Project for PendingEffectChoiceSemanticV4 {
    type Output = HumanEffectChoiceV1;
    fn project(&self, handles: &mut Handles) -> Result<Self::Output, Error> {
        Ok(match self {
            Self::Options {
                player,
                option_count,
                ..
            } => HumanEffectChoiceV1::Options {
                player: *player,
                option_count: *option_count,
            },
            Self::Targets {
                player,
                selected_targets,
                legal_targets,
                min_targets,
                max_targets,
                can_finish,
                ordered,
                purpose,
                ..
            } => HumanEffectChoiceV1::Targets {
                player: *player,
                selected_targets: selected_targets.project(handles)?,
                legal_targets: legal_targets.project(handles)?,
                min_targets: *min_targets,
                max_targets: *max_targets,
                can_finish: *can_finish,
                ordered: *ordered,
                purpose: *purpose,
            },
            Self::Color {
                player,
                legal_colors,
                ..
            } => HumanEffectChoiceV1::Color {
                player: *player,
                legal_colors: legal_colors.clone(),
            },
            Self::Number {
                player,
                minimum,
                maximum,
                ..
            } => HumanEffectChoiceV1::Number {
                player: *player,
                minimum: *minimum,
                maximum: *maximum,
            },
            Self::Boolean {
                player,
                default,
                purpose,
                ..
            } => HumanEffectChoiceV1::Boolean {
                player: *player,
                default: *default,
                purpose: *purpose,
            },
        })
    }
}
record!(HumanPendingEffectV1, PendingEffectSemanticV4, {
    source: Option<HumanCardRefV1>, controller: PlayerSeatV1, choice: Option<HumanEffectChoiceV1>,
});
record!(HumanPendingTriggerV1, PendingTriggerSemanticV2, {
    source: Option<HumanCardRefV1>, controller: PlayerSeatV1, trigger_kind: PendingTriggerKindV2, kicked: bool,
});
record!(HumanEngineContextV1, EngineContextV2, {
    current_stage: EngineDecisionStageV2,
    pending_cast: Option<HumanPendingCastV1>, pending_activation: Option<HumanPendingActivationV1>,
    pending_discard: Option<HumanPendingDiscardV1>, pending_optional_cost: Option<HumanPendingOptionalCostV1>,
    pending_optional_cost_sacrifice: Option<HumanPendingOptionalSacrificeV1>,
    pending_spell_copy: Option<HumanPendingSpellCopyV1>, pending_effect: Option<HumanPendingEffectV1>,
    pending_triggers: Vec<HumanPendingTriggerV1>,
});
record!(HumanPrivateBlockersV1, PrivateBlockersContextV2, {
    current_attacker: Option<HumanCardRefV1>, accumulated: Vec<(HumanCardRefV1, HumanCardRefV1)>,
    remaining: Vec<(HumanCardRefV1, Vec<HumanCardRefV1>)>,
});
record!(HumanPrivateDiscardV1, PrivateDiscardContextV2, {
    chosen: Vec<HumanCardRefV1>, remaining_choices: Vec<HumanCardRefV1>, remaining_needed: u32,
});
record!(HumanPrivateOptionalCostV1, PrivateOptionalCostContextV2, {
    discard_payable: bool, sacrifice_payable: bool, stage: SurfaceDecisionStageV2,
});
record!(HumanSurfaceContextV1, HarnessSurfaceContextV2, {
    current_stage: SurfaceDecisionStageV2, madness_cast_reprompt_source: Option<HumanCardRefV1>,
    private_blockers: Option<HumanPrivateBlockersV1>, private_discard: Option<HumanPrivateDiscardV1>,
    private_optional_cost: Option<HumanPrivateOptionalCostV1>,
});
record!(HumanCombatSelectionV1, PrivateCombatSelectionV5, {
    attacker: Option<HumanCardRefV1>, candidate_index: u32, candidate_count: u32,
    selected: Vec<HumanCardRefV1>, current_candidate: HumanCardRefV1,
    remaining_after_current: Vec<HumanCardRefV1>,
});
record!(HumanPolicyContextV1, PolicySurfaceContextV5, {
    current_stage: crate::policy_surface_v5::PolicySurfaceStageV5,
    private_combat_selection: Option<HumanCombatSelectionV1>,
});
record!(HumanPublicProjectionV1, PublicObservationProjectionV2, {
    turn: u32, phase: ZoneIndependentStepV1, active_player: PlayerSeatV1,
    priority_player: PlayerSeatV1, initiative: Option<PlayerSeatV1>,
    life_totals: [i32; 2], mana_pools: [[u8; 6]; 2], hand_counts: [usize; 2], library_counts: [usize; 2],
    player_status: [PlayerStatusV1; 2], battlefield: [Vec<HumanPublicCardV1>; 2],
    graveyards: [Vec<HumanPublicCardV1>; 2], exile: Vec<HumanPublicCardV1>,
    stack: Vec<HumanStackItemV1>, combat: HumanCombatV1,
    continuous_effects: Vec<HumanContinuousEffectV1>, object_relations: Vec<HumanObjectRelationV1>,
    exile_play_permissions: Vec<HumanExilePermissionV1>, engine_context: HumanEngineContextV1,
    surface_context: HumanSurfaceContextV1,
});
record!(HumanCastObjectCostV1, v6::PendingCastObjectCostV6, {
    source: HumanCardRefV1, controller: PlayerSeatV1, cast_method: CastMethodV4,
    cost_kind: CostKind, required_count: u32, selected: Vec<HumanCardRefV1>, remaining_count: u32,
});
record!(HumanChosenCreatureCostV1, v6::PendingChosenCreatureCostV6, {
    source: HumanCardRefV1, controller: PlayerSeatV1, selected_zone: ChosenCreatureCostZoneV1,
});
record!(HumanFinalizedCreatureCostV1, v6::FinalizedChosenCreatureCostV6, {
    stack_index: u32, source: HumanCardRefV1, chosen: HumanCardRefV1, power_lki: i32,
});
record!(HumanLibraryChoiceV1, v6::DecisionLocalLibraryV6, {
    chooser: PlayerSeatV1, library_owner: PlayerSeatV1, cards: Vec<HumanPrivateCardV1>,
});
record!(HumanHistoricalSourceV1, v6::HistoricalPublicSourceV6, {
    context: v6::HistoricalSourceContextV6, source: HumanCardRefV1, stack_item_kind: StackItemKindV2,
});
record!(HumanExtensionsV1, v6::PolicyObservationExtensionsV6, {
    pending_cast_object_cost: Option<HumanCastObjectCostV1>, decision_local_library: Option<HumanLibraryChoiceV1>,
    historical_public_sources: Vec<HumanHistoricalSourceV1>, pending_chosen_creature_cost: Option<HumanChosenCreatureCostV1>,
    finalized_chosen_creature_costs: Vec<HumanFinalizedCreatureCostV1>,
    pending_ward_payment: Option<v6::WardPaymentV6>, queued_ward_payments: Vec<v6::QueuedWardPaymentV6>,
});

pub(super) fn project_decision(
    observation: &v6::ObservationV6,
    actions: &[ActionSemanticV1],
    human: PlayerSeatV1,
    prompt_seq: u64,
) -> Result<(HumanDecisionV1, Vec<u32>), Error> {
    if observation.acting_player != human {
        return Err(Error::NotHumanTurn);
    }
    if actions.is_empty() {
        return Err(Error::UnsupportedPrompt);
    }
    let mut handles = Handles::new(observation)?;
    let initial = project_state(observation, &mut handles)?;
    handles.canonicalize(&initial)?;
    let state = project_state(observation, &mut handles)?;
    let mut choices = Vec::with_capacity(actions.len());
    for (index, action) in actions.iter().enumerate() {
        choices.push((
            super::labels::label(action, observation, &handles, human)?,
            u32::try_from(index).map_err(|_| Error::InvalidAction)?,
        ));
    }
    choices.sort_by(|a, b| a.0.cmp(&b.0));
    if choices.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err(Error::UnsupportedPrompt);
    }
    let engine_indexes = choices
        .iter()
        .map(|(_, engine_index)| *engine_index)
        .collect();
    let actions = choices
        .into_iter()
        .enumerate()
        .map(|(index, (label, _))| HumanLegalActionV1 {
            action_index: index as u32,
            label,
        })
        .collect();
    // A complete menu is published atomically; one unsupported label rejects
    // the whole prompt and never silently removes an engine-legal choice.
    Ok((
        HumanDecisionV1 {
            schema: "mtg-kernel-human-decision/v1".into(),
            human_seat: human,
            prompt_seq,
            state,
            actions,
        },
        engine_indexes,
    ))
}
