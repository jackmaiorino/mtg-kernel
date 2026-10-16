//! Distinct numeric Prepared V57 entry. No predecessor artifact or executable is encoded.
use super::{normalize, Error, NativeModel, Packet, Result, Source};
use crate::rl_session::{
    FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1, PreparedDecisionV57,
    PreparedSessionV57, PreparedSnapshotV57,
};
use serde_json::{json, Map, Value};
use std::{collections::BTreeSet, sync::Arc};

pub struct PreparedSession {
    inner: PreparedSessionV57,
    owner: Arc<()>,
    generation: u64,
}
#[derive(Clone)]
pub struct PreparedSnapshot {
    inner: PreparedSnapshotV57,
    owner: Arc<()>,
}
/// Private executable authority stays in the sealed backend capability.
/// This type deliberately has no Debug, serde or public constructor.
#[derive(Clone)]
pub struct PreparedDecision {
    backend: PreparedDecisionV57,
    packet: Packet,
    owner: Arc<()>,
    generation: u64,
}
impl PreparedDecision {
    pub fn packet(&self) -> &Packet {
        &self.packet
    }
    #[cfg(all(
        test,
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    pub(super) fn fixture_packet_mut(&mut self) -> &mut Packet {
        &mut self.packet
    }
}
impl PreparedSession {
    pub fn from_session(session: FastActorSessionV1) -> Result<Self> {
        Ok(Self::from_prepared(
            PreparedSessionV57::from_session(session)
                .map_err(|e| Error::Projection(e.to_string()))?,
        ))
    }
    /// Transfer a genuine sealed current owner, never an observation/world DTO.
    pub fn from_prepared(inner: PreparedSessionV57) -> Self {
        Self {
            inner,
            owner: Arc::new(()),
            generation: 0,
        }
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.current_response()
    }
    #[cfg(all(
        test,
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    pub(super) fn fixture_state(&self) -> &crate::state::GameState {
        self.inner.fixture_state_v57()
    }
    #[cfg(all(
        test,
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    pub(super) fn fixture_counts(&self) -> (u64, u64) {
        self.inner.fixture_counts_v57()
    }
    pub fn snapshot(&self) -> PreparedSnapshot {
        PreparedSnapshot {
            inner: self.inner.snapshot_v57(),
            owner: self.owner.clone(),
        }
    }
    pub fn restore(&mut self, snapshot: &PreparedSnapshot) -> Result<()> {
        if !Arc::ptr_eq(&self.owner, &snapshot.owner) {
            return Err(Error::ForeignSnapshot);
        }
        let next = self.generation.checked_add(1).ok_or(Error::Range)?;
        // Backend authenticates the original binding before monotonic rebind.
        self.inner
            .restore_v57(&snapshot.inner)
            .map_err(|_| Error::StaleBinding)?;
        self.generation = next;
        Ok(())
    }
    pub fn encode(&self, expected: FastActorDecisionV1) -> Result<PreparedDecision> {
        let backend = self.inner.decision_v57().map_err(|_| Error::StaleBinding)?;
        if backend.expected() != expected {
            return Err(Error::StaleBinding);
        }
        let state = serde_json::to_value(backend.observation())
            .map_err(|e| Error::Projection(e.to_string()))?;
        let actions = backend
            .actions()
            .iter()
            .map(serde_json::to_value)
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Projection(e.to_string()))?;
        let packet = normalize::encode(Source::PreparedV57, state, actions)?;
        Ok(PreparedDecision {
            backend,
            packet,
            owner: self.owner.clone(),
            generation: self.generation,
        })
    }
    pub fn apply(
        &mut self,
        decision: &PreparedDecision,
        selected_index: u32,
    ) -> Result<FastActorResponseV1> {
        if !Arc::ptr_eq(&self.owner, &decision.owner) || self.generation != decision.generation {
            return Err(Error::StaleBinding);
        }
        if usize::try_from(selected_index)
            .ok()
            .is_none_or(|i| i >= decision.packet.actions.len())
        {
            return Err(Error::ActionRange);
        }
        self.inner
            .validate_decision_v57(&decision.backend)
            .map_err(|_| Error::StaleBinding)?;
        if self.encode(decision.backend.expected())?.packet != decision.packet {
            return Err(Error::StaleBinding);
        }
        // The real owning step repeats complete current private validation
        // immediately before applying the original native ordered index.
        self.inner
            .step_v57(&decision.backend, selected_index)
            .map_err(|e| Error::Projection(e.to_string()))
    }
    pub fn score_and_apply(
        &mut self,
        decision: &PreparedDecision,
        model: &NativeModel,
    ) -> Result<(Vec<f32>, u32, FastActorResponseV1)> {
        let logits = model.forward(&decision.packet)?;
        if logits.len() != decision.packet.actions.len() || logits.is_empty() {
            return Err(Error::ModelContract);
        }
        let mut selected = 0;
        for index in 1..logits.len() {
            if logits[index] > logits[selected] {
                selected = index;
            }
        }
        let selected = u32::try_from(selected).map_err(|_| Error::Range)?;
        let response = self.apply(decision, selected)?;
        Ok((logits, selected, response))
    }
}

/// Recognize complete new meaning types by their actual schema, never the
/// number of fields or an integrity digest. Their full ordered bodies survive.
pub(super) fn typed_meaning(
    value: &Map<String, Value>,
    covered: &mut BTreeSet<u16>,
) -> Result<bool> {
    macro_rules! typed {
        ($ty:ty,$validate:path) => {{
            let root: $ty = serde_json::from_value(Value::Object(value.clone()))
                .map_err(|e| Error::Projection(e.to_string()))?;
            $validate(&root).map_err(|e| Error::Projection(format!("typed meaning:{e:?}")))?;
            covered.insert(root.card_id);
            return Ok(true);
        }};
    }
    match value.get("schema").and_then(Value::as_str) {
        Some(crate::rules_meaning_drake_hatcher_v53::SCHEMA) => typed!(
            crate::rules_meaning_drake_hatcher_v53::MeaningV53,
            crate::rules_meaning_drake_hatcher_v53::validate
        ),
        Some(crate::rules_meaning_spinner_v54::SCHEMA) => typed!(
            crate::rules_meaning_spinner_v54::MeaningV54,
            crate::rules_meaning_spinner_v54::validate
        ),
        Some(crate::rules_meaning_quandary_v55::SCHEMA) => typed!(
            crate::rules_meaning_quandary_v55::MeaningV55,
            crate::rules_meaning_quandary_v55::validate
        ),
        _ => {}
    }
    // Phoenix places its schema in its typed selector.
    if value
        .get("selector")
        .and_then(|s| s.get("schema"))
        .and_then(Value::as_str)
        == Some(crate::rules_meaning_phoenix_v56::SCHEMA)
    {
        typed!(
            crate::rules_meaning_phoenix_v56::MeaningV56,
            crate::rules_meaning_phoenix_v56::validate
        );
    }
    Ok(false)
}

/// Convert only the three closed public reference types into the same private
/// normalization operand as common CardStableRef. No raw stamp reaches a tree;
/// shared object/incarnation ranks still distinguish genuine reentries.
pub(super) fn references(value: &mut Value) -> Result<()> {
    let Value::Object(fields) = value else {
        if let Value::Array(rows) = value {
            for row in rows {
                references(row)?;
            }
        }
        return Ok(());
    };
    if fields.contains_key("identity")
        && fields.contains_key("owner")
        && fields.contains_key("controller")
        && fields.contains_key("zone")
    {
        let stable = if fields.contains_key("incarnation") {
            let r: crate::drake_public_v53::ReferenceV53 =
                serde_json::from_value(Value::Object(fields.clone()))
                    .map_err(|e| Error::Projection(e.to_string()))?;
            let printing = match r.identity {
                crate::drake_public_v53::IdentityV53::Known { card_id } => card_id,
                crate::drake_public_v53::IdentityV53::FaceDown { disguised } => {
                    face_down(disguised)?
                }
            };
            Some(
                json!({"arena_id":r.incarnation.object,"zone_change_count":r.incarnation.generation,
                "card_db_id":printing,"owner":r.owner,"controller":r.controller,"zone":r.zone}),
            )
        } else if fields.contains_key("object") && fields.contains_key("generation") {
            // Optional/Phoenix references have exactly the same closed public
            // grammar. Deserialization rejects extra private/executable fields.
            let r: crate::optional_trigger_public_v1::ReferenceV1 =
                serde_json::from_value(Value::Object(fields.clone()))
                    .map_err(|e| Error::Projection(e.to_string()))?;
            let printing = match r.identity {
                crate::optional_trigger_public_v1::IdentityV1::Known { card_id } => card_id,
                crate::optional_trigger_public_v1::IdentityV1::FaceDown { disguised } => {
                    face_down(disguised)?
                }
            };
            Some(
                json!({"arena_id":r.object,"zone_change_count":r.generation,"card_db_id":printing,
                "owner":r.owner,"controller":r.controller,"zone":r.zone}),
            )
        } else {
            None
        };
        if let Some(stable) = stable {
            let identity = fields["identity"].clone();
            *value = json!({"public_reference":stable,"public_identity":identity});
            return Ok(());
        }
    }
    for child in fields.values_mut() {
        references(child)?;
    }
    Ok(())
}
fn face_down(disguised: bool) -> Result<u16> {
    crate::card_def::card_id_by_name(if disguised {
        "Face-down creature"
    } else {
        "Face-down card"
    })
    .ok_or_else(|| Error::UnsupportedOperand("public_face_down_identity".into()))
}

#[derive(serde::Serialize)]
enum CastStep {
    PayPrintedMana(crate::mana::Cost),
    MoveOriginalSpellFromStackToBattlefieldUnderCasterControl,
}
#[derive(serde::Serialize)]
enum Timing {
    CasterMainPhaseEmptyStackWithPriority,
}
#[derive(serde::Serialize)]
enum Departure {
    BattlefieldOnResolutionOtherwiseOwnersGraveyard,
}
#[derive(serde::Serialize)]
struct CastRoot {
    method: crate::state::CastMethodV4,
    route: crate::state::SpellCastRouteV4,
    origin: crate::state::Zone,
    timing: Timing,
    untargeted: bool,
    ordered_instructions: [CastStep; 2],
    physical_departure: Departure,
    other_permissions_require_actual_candidate: bool,
    zone_replacements_apply: bool,
}
/// Mandatory for referenced prepared printings even if an ability/static root
/// already put their ID into `covered`. A changed recipe fails its selector.
pub(super) fn printing(id: u16) -> Result<Option<Value>> {
    use crate::card_def::{TargetSpec, CARD_DEFS};
    use crate::effect::{EffectOp, ObjectRef};
    use crate::state::Zone;
    let d = CARD_DEFS
        .get(usize::from(id))
        .ok_or(Error::UnsupportedPrintedRoot(id))?;
    let ordered_rules = match d.name {
        "Elenda, Saint of Dusk" => return super::roots::elenda_printing(id).map(Some),
        "Drake Hatcher" => {
            if !hatcher_foreign_fields_absent(d) {
                return Err(Error::UnsupportedPrintedRoot(id));
            }
            use crate::rules_meaning_drake_hatcher_v53::{printed_meaning, RootV53};
            let roots = [
                RootV53::Prowess,
                RootV53::CombatIncubation,
                RootV53::RemoveIncubationForDrake,
            ]
            .into_iter()
            .map(|root| printed_meaning(id, root).map_err(|_| Error::UnsupportedPrintedRoot(id)))
            .collect::<Result<Vec<_>>>()?;
            serde_json::to_value(roots).map_err(|e| Error::Projection(e.to_string()))?
        }
        "Spinner of Souls" => serde_json::to_value(
            crate::rules_meaning_spinner_v54::printed_meaning(id)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?,
        )
        .map_err(|e| Error::Projection(e.to_string()))?,
        "Painful Quandary" => serde_json::to_value(
            crate::rules_meaning_quandary_v55::printed_meaning(id)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?,
        )
        .map_err(|e| Error::Projection(e.to_string()))?,
        "Flamewake Phoenix" => serde_json::to_value(
            crate::rules_meaning_phoenix_v56::printed_meaning(id)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?,
        )
        .map_err(|e| Error::Projection(e.to_string()))?,
        _ => return Ok(None),
    };
    if d.target_spec != TargetSpec::None
        || d.cost.x_count != 0
        || (d.spell_effect)()
            != Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    // Complete typed characteristics and independently authenticated ordered
    // special roots. No function pointer, EffectOp or private world is serialized.
    Ok(Some(json!({"card_id":id,"prepared_printing":{
        "characteristics":{"types":d.types,"subtypes":d.subtypes,"supertypes":d.supertypes.iter().map(|value| *value as u8).collect::<Vec<_>>(),
            "colors":d.colors,"mana_value":d.mana_value,"power":d.power,"toughness":d.toughness,
            "keyword_bits":d.keywords.0,"minimum_blockers":d.minimum_blockers,"is_token":d.is_token,
            "is_land":d.is_land,"enters_tapped":d.enters_battlefield_tapped},
        "ordered_cast_roots":[CastRoot{method:crate::state::CastMethodV4::Normal,
            route:crate::state::SpellCastRouteV4::Hand,origin:Zone::Hand,
            timing:Timing::CasterMainPhaseEmptyStackWithPriority,untargeted:true,
            ordered_instructions:[CastStep::PayPrintedMana(d.cost),CastStep::MoveOriginalSpellFromStackToBattlefieldUnderCasterControl],
            physical_departure:Departure::BattlefieldOnResolutionOtherwiseOwnersGraveyard,
            other_permissions_require_actual_candidate:true,zone_replacements_apply:true}],
        "ordered_special_roots":ordered_rules
    }})))
}

fn hatcher_foreign_fields_absent(d: &crate::card_def::CardDef) -> bool {
    let crate::card_def::CardDef {
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
    (d.other_controlled_nontoken_death_v1.is_none() && d.creature_type_entry_v1.is_none())
        && d.ward_cost.is_none()
        && d.alt_cost.is_none()
        && d.mode2.is_none()
        && d.mode3.is_none()
        && d.additional_modes.is_empty()
        && d.flashback.is_none()
        && d.kicker_cost.is_none()
        && d.additional_cost.is_none()
        && d.optional_additional_cost.is_none()
        && d.equipment.is_none()
        && d.attachment.is_none()
        && d.produces_mana.is_empty()
        && d.mana_ability_choices.is_empty()
        && (d.mana_ability)().is_none()
        && d.starting_loyalty.is_none()
        && d.enters_with_plus_one_counters.is_none()
        && d.controlled_creature_grant.is_none()
        && d.controlled_counter_keyword.is_none()
        && d.game_outcome_protection_v1.is_none()
        && d.generic_cost_reduction.is_none()
        && !d.spell_cannot_be_countered
        && !d.records_mana_spent
        && d.plot_cost.is_none()
        && d.madness_cost.is_none()
        && d.kicked_target_spec.is_none()
        && d.escape.is_none()
        && !d.enters_battlefield_tapped
        && d.mana_ability_def.is_none()
        && d.minimum_blockers == 1
        && d.omen.is_none()
        && !d.mana_ability_includes_chosen_color
        && d.as_enters_choose_color_other_than.is_none()
        && d.additional_mana_abilities.is_empty()
        && d.enters_battlefield_tapped_unless.is_none()
        && d.transform_face.is_none()
        && d.saga.is_none()
        && !d.changeling
        && d.bestow.is_none()
        && !d.delve
        && d.adventure.is_none()
        && !d.cant_be_blocked_by_monarchs_creatures
        && d.conditional_tap_yield.is_none()
        && d.enters_tapped_unless_controller.is_none()
        && d.additional_mana_ability_conditions.is_empty()
        && d.restricted_mana_abilities.is_empty()
        && d.animation.is_none()
        && d.activated_ability_generic_reductions.is_empty()
        && d.combat_static_rules == crate::card_def::CombatStaticRulesDef::NONE
        && d.as_enters_color_choice_v1.is_none()
        && d.chosen_color_creature_boost_v1.is_none()
        && d.damage_replacement_v1.is_none()
        && d.token_counter_replacement_v48.is_none()
        && d.starting_life_self_bonuses_v1.is_empty()
        && d.source_type_hexproof_v1.is_none()
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;
