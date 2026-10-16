//! Viewer-specific next-five facts. Native executable operands stay private.
use crate::{
    card_def::{self, Subtype},
    effect,
    ids::PlayerId,
    optional_trigger_public_v1 as public,
    rl::PlayerSeatV1,
    rules_meaning_next_five_v59::{self as meaning, FamilyV59},
    state::{GameState, StackItemKind, StackTargetContractV4, Target, Zone},
};
use serde::{Deserialize, Serialize};
pub const SCHEMA: &str = "mtg-kernel/next-five-public/v59";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum TargetV59 {
    Player(PlayerSeatV1),
    Object(public::ReferenceV1),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorizedV59 {
    pub family: FamilyV59,
    pub card: public::ReferenceV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentV59 {
    pub family: FamilyV59,
    pub card: public::CurrentV1,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerStageV59 {
    Held,
    Published,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerV59 {
    pub family: FamilyV59,
    pub occurrence: u32,
    pub source: public::RootV1,
    pub stage: TriggerStageV59,
    pub placement_ordered: bool,
    pub targets: Vec<TargetV59>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CastV59 {
    pub family: FamilyV59,
    pub source: public::ReferenceV1,
    pub original_controller: PlayerSeatV1,
    pub is_copy: bool,
    pub payment_pending: bool,
    /// None while the authentic cast has not selected X; non-X spells select zero.
    pub x: Option<u16>,
    pub targets: Vec<TargetV59>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuraCopyV59 {
    pub source: public::RootV1,
    pub target: TargetV59,
    pub template_card_id: u16,
    pub template_face: u8,
    pub chooser: PlayerSeatV1,
    pub expanded_count: u64,
    pub chosen_hosts: Vec<public::ReferenceV1>,
    pub candidate_hosts: Vec<public::ReferenceV1>,
    pub accepted_host_awaiting_advance: Option<public::ReferenceV1>,
    /// Includes an accepted host until the native continuation commits that answer.
    pub remaining_count: u64,
    pub untargeted: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeathV59 {
    pub occurrence: u32,
    pub source: public::RootV1,
    pub captured_not_demon: bool,
    pub collection_completed: bool,
    pub exact_graveyard_card_now: Option<public::ReferenceV1>,
    pub owner_on_return: PlayerSeatV1,
    /// Physical-card/incarnation condition only; not a live trigger or action permission.
    pub return_incarnation_available_now: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AngelReplacementV59 {
    pub source: public::ReferenceV1,
    pub controller: PlayerSeatV1,
    pub current_angels: Vec<public::ReferenceV1>,
    pub original_angel_count_now: u32,
    pub self_entry_excluded: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestrictedManaV59 {
    pub color: crate::mana::ManaColor,
    pub only_cast_angel_spell: bool,
    pub current_original_source: Option<public::ReferenceV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoblinQuoteV59 {
    pub source: public::ReferenceV1,
    pub original_controller: PlayerSeatV1,
    pub original_x: u16,
    pub target: TargetV59,
    pub target_legal_now: bool,
    pub creature_lethal_damage_now: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intersection: Option<crate::goblin_negotiation_v1::IntersectionV1>,
    pub quote_can_change_before_resolution: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV59 {
    pub schema: String,
    pub producer_sha256: String,
    pub viewer: PlayerSeatV1,
    pub meanings: Vec<meaning::MeaningV59>,
    pub snack: crate::midnight_snack_public_v58::FactsV58,
    pub wide_life: crate::snack_wide_public_v59::FactsV59,
    pub authorized: Vec<AuthorizedV59>,
    pub current: Vec<CurrentV59>,
    pub casts: Vec<CastV59>,
    pub triggers: Vec<TriggerV59>,
    pub aura_copy: Option<AuraCopyV59>,
    pub infernal_deaths: Vec<DeathV59>,
    pub angel_replacements: Vec<AngelReplacementV59>,
    pub angel_mana: [Vec<RestrictedManaV59>; 2],
    pub goblin_quotes: Vec<GoblinQuoteV59>,
    pub goblin_ordering: Option<crate::goblin_negotiation_v1::PublicOrderingStageV1>,
    /// Exact native option order for the authenticated current Goblin stage.
    pub goblin_ordering_options: Vec<crate::crystal_public_v1::OptionV1>,
    /// Order is the validated public completion occurrence order, not a history index.
    pub goblin_completed: Vec<crate::goblin_negotiation_v1::PublicCompletedV1>,
}
pub fn producer_sha256() -> String {
    meaning::producer_sha256()
}
/// Frozen consumers cannot authorize rules they do not project. Unknown private
/// identities do not select a refusal; retained originally public roots do.
pub(crate) fn require_legacy_visible(
    state: &GameState,
    viewer: Option<PlayerId>,
) -> Result<(), String> {
    let ids = FamilyV59::ALL
        .into_iter()
        .filter_map(|f| card_def::card_id_by_name(f.name()))
        .collect::<std::collections::BTreeSet<_>>();
    if ids.is_empty() {
        return Ok(());
    }
    let visible = state.objects.iter().any(|(id, _)| {
        public::reference(state, id, viewer).is_some_and(
            |r| matches!(r.identity,public::IdentityV1::Known{card_id}if ids.contains(&card_id)),
        )
    });
    let item_selected = |item: &crate::state::StackItem| {
        item.v4
            .ability_source_contract
            .is_some_and(|s| ids.contains(&s.card_def))
            || item
                .v4
                .source_contract
                .is_some_and(|s| ids.contains(&s.card_def))
    };
    let retained = state.stack.iter().any(item_selected)
        || state
            .engine
            .pending_effect
            .as_ref()
            .is_some_and(|p| item_selected(&p.resolving_item))
        || public::held(state)
            .iter()
            .any(|p| p.source_contract.is_some_and(|s| ids.contains(&s.card_def)))
        || state.players.iter().any(|p| {
            p.restricted_mana_pool
                .0
                .iter()
                .any(|m| ids.contains(&m.source_card_def))
        })
        || state.engine.event_history.iter().any(|e| match e {
            crate::event::CommittedEvent::ControlledCreatureDeathCaptureV1 { source, .. } => {
                ids.contains(&source.card_def)
            }
            crate::event::CommittedEvent::CrystalExecutionV1 { execution } => {
                execution.steps.iter().any(|s| s.excess_capture.is_some())
            }
            _ => false,
        });
    if visible || retained {
        Err("next-five rules require the distinct V59 consumer".into())
    } else {
        Ok(())
    }
}
fn family(id: u16, ids: &[(FamilyV59, u16)]) -> Option<FamilyV59> {
    ids.iter().find_map(|(f, i)| (*i == id).then_some(*f))
}
fn identity(id: u16) -> public::IdentityV1 {
    if card_def::card_id_by_name("Face-down creature") == Some(id) {
        public::IdentityV1::FaceDown { disguised: true }
    } else if card_def::card_id_by_name("Face-down card") == Some(id) {
        public::IdentityV1::FaceDown { disguised: false }
    } else {
        public::IdentityV1::Known { card_id: id }
    }
}
fn target(state: &GameState, value: StackTargetContractV4) -> Result<TargetV59, String> {
    match value {
        StackTargetContractV4::Player(p) => Ok(TargetV59::Player(p.into())),
        StackTargetContractV4::Object {
            object,
            card_def,
            owner,
            controller,
            zone,
            zone_change_count,
            ..
        } => {
            let id = crate::rl::projected_captured_card_def(
                state,
                object,
                zone_change_count,
                zone,
                card_def,
            );
            Ok(TargetV59::Object(public::ReferenceV1 {
                object: object.0,
                generation: zone_change_count,
                owner: owner.into(),
                controller: controller.into(),
                zone,
                identity: identity(id),
            }))
        }
        StackTargetContractV4::StackItem { .. } => {
            Err("V59 printed recipes do not target a stack row".into())
        }
    }
}
fn binding_ref(
    state: &GameState,
    b: effect::EffectObjectBinding,
    viewer: PlayerId,
) -> Result<public::ReferenceV1, String> {
    public::reference(state, b.object, Some(viewer))
        .filter(|r| r.zone == b.expected_zone && r.generation == b.expected_zone_change_count)
        .ok_or_else(|| "V59 candidate host lost its public original incarnation".into())
}
fn aura_copy(state: &GameState, viewer: PlayerId) -> Result<Option<AuraCopyV59>, String> {
    let Some(p) = &state.engine.pending_effect else {
        return Ok(None);
    };
    let (c, accepted) = match (&p.choice, &p.answered_choice_guard) {
        (
            Some(effect::PendingEffectChoice::SelectTargets {
                purpose: effect::EffectTargetSelectionPurpose::AttachCopiedAuraV1 { choice },
                ..
            }),
            None,
        ) => (choice, None),
        (None, Some(effect::EffectAnsweredChoiceGuard::AttachCopiedAuraV1 { choice, host })) => {
            (choice, Some(*host))
        }
        _ => return Ok(None),
    };
    effect::validate_pending_effect_choice(state)?;
    let s = p
        .ctx
        .ability_source_contract
        .ok_or("V59 copied Aura lost native source")?;
    if s.card_def != meaning::selected_id(FamilyV59::Replication)? {
        return Err("V59 Aura copy has foreign printed source".into());
    }
    let count = c.replacement.replaced_token_count(1)?;
    let selected = u64::try_from(c.selected_hosts.len()).map_err(|_| "V59 host count overflow")?;
    let expanded = u64::try_from(count).map_err(|_| "V59 expanded count overflow")?;
    Ok(Some(AuraCopyV59 {
        source: public::root(state, s, Some(viewer))?,
        target: target(
            state,
            *p.ctx
                .target_contracts
                .first()
                .ok_or("V59 copy target absent")?,
        )?,
        template_card_id: c.template.card_def,
        template_face: c.template.face_index,
        chooser: p.ctx.controller.into(),
        expanded_count: expanded,
        chosen_hosts: c
            .selected_hosts
            .iter()
            .map(|b| binding_ref(state, *b, viewer))
            .collect::<Result<_, _>>()?,
        candidate_hosts: c
            .candidates
            .iter()
            .map(|b| binding_ref(state, *b, viewer))
            .collect::<Result<_, _>>()?,
        accepted_host_awaiting_advance: accepted
            .map(|b| binding_ref(state, b, viewer))
            .transpose()?,
        remaining_count: expanded
            .checked_sub(selected)
            .ok_or("V59 selected copy count exceeds roster")?,
        untargeted: true,
    }))
}
fn deaths(state: &GameState, viewer: PlayerId, id: u16) -> Result<Vec<DeathV59>, String> {
    use crate::event::CommittedEvent as E;
    let mut rows = vec![];
    let mut originals = std::collections::BTreeSet::new();
    for (index, e) in state.engine.event_history.iter().enumerate() {
        let E::ControlledCreatureDeathCaptureV1 {
            source,
            ability_index,
            died,
            controller_before,
        } = e
        else {
            continue;
        };
        if source.card_def != id {
            continue;
        }
        if *ability_index != 0
            || source.source != died.object
            || source.zone_change_count != died.zone_change_count
            || source.controller != *controller_before
            || !originals.insert((source.source.0, source.zone_change_count))
        {
            return Err("V59 Infernal original death capture changed or duplicated".into());
        }
        let prior = Some(crate::creature_death_capture_v1::validated_capture_death(
            state, index,
        )?);
        if !matches!(prior,Some(E::ZoneChange{object,from:Zone::Battlefield,to:Zone::Graveyard,controller_before:actual})if *object==died.object&&actual==controller_before)
        {
            return Err("V59 Infernal capture lost its actual adjacent death".into());
        }
        let collections=state.engine.event_history.iter().filter(|e|matches!(e,E::ControlledCreatureDeathCaptureCollectedV1{capture_history_index}if *capture_history_index==index as u64)).count();
        if collections > 1 {
            return Err("V59 Infernal original death collected repeatedly".into());
        }
        let current = state.objects.try_get(source.source).filter(|o| {
            o.zone == Zone::Graveyard
                && o.owner == source.owner
                && source.zone_change_count.checked_add(1) == Some(o.zone_change_count)
        });
        let exact = current.and_then(|_| public::reference(state, source.source, Some(viewer)));
        rows.push(DeathV59 {
            occurrence: u32::try_from(rows.len()).map_err(|_| "V59 death ordinal overflow")?,
            source: public::root(state, *source, Some(viewer))?,
            captured_not_demon: true,
            collection_completed: collections == 1,
            exact_graveyard_card_now: exact,
            owner_on_return: source.owner.into(),
            return_incarnation_available_now: current.is_some_and(|o| !o.v4.is_token),
        });
    }
    Ok(rows)
}
pub fn capture(state: &GameState, viewer: PlayerId) -> Result<FactsV59, String> {
    capture_scoped(state, viewer, public::CounterScopeV61::Legacy)
}
pub(crate) fn capture_complete_v61(
    state: &GameState,
    viewer: PlayerId,
) -> Result<FactsV59, String> {
    capture_scoped(state, viewer, public::CounterScopeV61::CompleteV61)
}
pub(crate) fn capture_complete_call_v62(
    state: &GameState,
    viewer: PlayerId,
) -> Result<FactsV59, String> {
    capture_scoped(state, viewer, public::CounterScopeV61::CallV62)
}
fn capture_scoped(
    state: &GameState,
    viewer: PlayerId,
    scope: public::CounterScopeV61,
) -> Result<FactsV59, String> {
    public::validate_state_checked(state)?;
    if let Some(p) = &state.engine.pending_cast {
        crate::engine::validate_pending_cast(state, p)?;
    }
    if let Some(p) = &state.engine.pending_activation {
        crate::engine::validate_pending_activation(state, p)?;
    }
    crate::doubling_season_v48::validate_token_journals(state)?;
    let ids = FamilyV59::ALL
        .into_iter()
        .map(|f| Ok((f, meaning::selected_id(f)?)))
        .collect::<Result<Vec<_>, String>>()?;
    let meanings = FamilyV59::ALL
        .into_iter()
        .map(meaning::printed_meaning)
        .collect::<Result<_, _>>()?;
    let mut authorized = vec![];
    let mut current = vec![];
    for (f, id) in &ids {
        for (object, _) in state.objects.iter() {
            if let Some(card) = public::reference(state, object, Some(viewer))
                .filter(|r| r.identity == public::IdentityV1::Known { card_id: *id })
            {
                authorized.push(AuthorizedV59 { family: *f, card });
            }
        }
        current.extend(
            if scope == public::CounterScopeV61::CallV62 {
                public::current_complete_call_v62(state, *id)?
            } else if scope == public::CounterScopeV61::CompleteV61 {
                public::current_complete_v61(state, *id)?
            } else {
                public::current_checked(state, *id)?
            }
            .into_iter()
            .map(|card| CurrentV59 { family: *f, card }),
        );
    }
    let mut casts = vec![];
    let mut triggers = vec![];
    let mut goblin_quotes = vec![];
    for p in public::held(state) {
        let Some(s) = p.source_contract else {
            continue;
        };
        let Some(f) = family(s.card_def, &ids) else {
            continue;
        };
        if p.granted_by.is_some() {
            continue;
        }
        if !matches!(f, FamilyV59::Replication | FamilyV59::Infernal) {
            continue;
        }
        triggers.push(TriggerV59 {
            family: f,
            occurrence: u32::try_from(triggers.len())
                .map_err(|_| "V59 trigger ordinal overflow")?,
            source: public::root(state, s, Some(viewer))?,
            stage: TriggerStageV59::Held,
            placement_ordered: p.placement_ordered,
            targets: p
                .target_contracts
                .iter()
                .map(|c| target(state, *c))
                .collect::<Result<_, _>>()?,
        });
    }
    for item in &state.stack {
        if item.kind == StackItemKind::Spell {
            let Some(s) = public::reference(state, item.source, Some(viewer)) else {
                continue;
            };
            let public::IdentityV1::Known { card_id } = &s.identity else {
                continue;
            };
            let Some(f) = family(*card_id, &ids) else {
                continue;
            };
            let pending = state
                .engine
                .pending_cast
                .as_ref()
                .filter(|p| p.spell == item.source);
            let contracts = pending.map_or(item.v4.target_contracts.as_slice(), |p| {
                p.target_contracts.as_slice()
            });
            let targets = contracts
                .iter()
                .map(|c| target(state, *c))
                .collect::<Result<Vec<_>, _>>()?;
            casts.push(CastV59 {
                family: f,
                source: s.clone(),
                original_controller: item.controller.into(),
                is_copy: item.is_copy,
                payment_pending: pending.is_some(),
                x: pending.map_or(Some(item.v4.x_value), |p| p.x_value.map(u16::from)),
                targets: targets.clone(),
            });
            // The native cast placeholder deliberately has no finalized targets.
            // Its authenticated staged selections are public above; a resolution
            // quote exists only after actual announcement/payment finalization.
            if f == FamilyV59::Goblin && pending.is_none() {
                let legal = crate::engine::stack_targets_still_legal(item, state)?;
                let lethal = if legal {
                    match item.targets.first() {
                        Some(Target::Object(id)) => {
                            Some(crate::goblin_negotiation_v1::creature_lethal_threshold(
                                state,
                                item.source,
                                *id,
                            ))
                        }
                        _ => None,
                    }
                } else {
                    None
                };
                let intersection = if let (Some(creature), Some(Target::Object(id))) =
                    (lethal, item.targets.first())
                {
                    crate::goblin_negotiation_v1::intersection(state, *id, creature)?
                } else {
                    None
                };
                goblin_quotes.push(GoblinQuoteV59 {
                    source: s,
                    original_controller: item.controller.into(),
                    original_x: item.v4.x_value,
                    target: targets
                        .first()
                        .cloned()
                        .ok_or("V59 Goblin original target missing")?,
                    target_legal_now: legal,
                    creature_lethal_damage_now: lethal,
                    intersection,
                    quote_can_change_before_resolution: true,
                });
            }
        } else if item.kind == StackItemKind::TriggeredAbility {
            let Some(s) = item.v4.ability_source_contract else {
                continue;
            };
            let Some(f) = family(s.card_def, &ids) else {
                continue;
            };
            if !matches!(f, FamilyV59::Replication | FamilyV59::Infernal)
                || item.v4.granted_by.is_some()
            {
                continue;
            }
            triggers.push(TriggerV59 {
                family: f,
                occurrence: u32::try_from(triggers.len())
                    .map_err(|_| "V59 trigger ordinal overflow")?,
                source: public::root(state, s, Some(viewer))?,
                stage: TriggerStageV59::Published,
                placement_ordered: true,
                targets: item
                    .v4
                    .target_contracts
                    .iter()
                    .map(|c| target(state, *c))
                    .collect::<Result<_, _>>()?,
            });
        }
    }
    let giada = meaning::selected_id(FamilyV59::Giada)?;
    let mut angel_replacements = vec![];
    for (object, o) in state.objects.iter() {
        if o.zone != Zone::Battlefield
            || o.v4.face_down_v1.is_some()
            || o.card_def != giada
            || !crate::continuous_characteristics_v1::printed_abilities_active(state, object)
        {
            continue;
        }
        let angels = state
            .objects
            .iter()
            .filter(|(id, a)| {
                a.zone == Zone::Battlefield
                    && a.controller == o.controller
                    && crate::engine::has_effective_subtype(state, *id, Subtype::Angel)
            })
            .map(|(id, _)| {
                public::reference(state, id, Some(viewer))
                    .ok_or("V59 Angel public member absent".to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        angel_replacements.push(AngelReplacementV59 {
            source: public::reference(state, object, Some(viewer))
                .ok_or("V59 Giada source absent")?,
            controller: o.controller.into(),
            original_angel_count_now: u32::try_from(angels.len())
                .map_err(|_| "V59 Angel count overflow")?,
            current_angels: angels,
            self_entry_excluded: true,
        });
    }
    let mut angel_mana = [vec![], vec![]];
    for (seat, rows) in angel_mana.iter_mut().enumerate() {
        for m in &state.players[seat].restricted_mana_pool.0 {
            if m.source_card_def == giada {
                if m.color != crate::mana::ManaColor::W
                    || m.restriction
                        != card_def::ManaSpendRestrictionDef::SpellWithSubtype(Subtype::Angel)
                {
                    return Err("V59 Giada issued mana restriction changed".into());
                }
                rows.push(RestrictedManaV59 {
                    color: m.color,
                    only_cast_angel_spell: true,
                    current_original_source: public::reference(
                        state,
                        m.source.object,
                        Some(viewer),
                    )
                    .filter(|r| {
                        r.generation == m.source.zone_change_count && r.zone == Zone::Battlefield
                    }),
                });
            }
        }
    }
    let goblin_ordering =
        crate::goblin_negotiation_v1::public_ordering_stage(state).map_err(|e| e.0)?;
    let goblin_ordering_options = if goblin_ordering.is_some() {
        let Some(effect::PendingEffectChoice::ChooseOption {
            purpose: effect::EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
            ..
        }) = state
            .engine
            .pending_effect
            .as_ref()
            .and_then(|p| p.choice.as_ref())
        else {
            return Err("V59 authenticated Goblin ordering lost its native choice".into());
        };
        crate::crystal_public_v1::prevention_options(state, choice).map_err(|e| e.0)?
    } else {
        vec![]
    };
    Ok(FactsV59 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        viewer: viewer.into(),
        meanings,
        snack: crate::midnight_snack_public_v58::capture(state, viewer)?,
        wide_life: crate::snack_wide_public_v59::capture(state).map_err(|e| e.0)?,
        authorized,
        current,
        casts,
        triggers,
        aura_copy: aura_copy(state, viewer)?,
        infernal_deaths: deaths(state, viewer, meaning::selected_id(FamilyV59::Infernal)?)?,
        angel_replacements,
        angel_mana,
        goblin_quotes,
        goblin_ordering,
        goblin_ordering_options,
        goblin_completed: crate::goblin_negotiation_v1::public_completed(state).map_err(|e| e.0)?,
    })
}
pub fn validate(state: &GameState, viewer: PlayerId, value: &FactsV59) -> Result<(), String> {
    if *value == capture(state, viewer)? {
        Ok(())
    } else {
        Err("V59 public facts changed from current authorized state".into())
    }
}
#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
pub(crate) mod tests;
