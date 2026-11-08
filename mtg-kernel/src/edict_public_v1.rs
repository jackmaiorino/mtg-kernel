//! Exact actor-visible Blasphemous Edict quotes, menus, and outcomes.
use crate::{
    card_def::{self, AltCostCondition, AltCostDef, CardCapability, CardType, CostComponent, Keywords, TargetSpec},
    edict_receipts_v1 as native,
    effect::{EffectOp, EffectTargetCandidate, EffectTargetSelectionPurpose, PendingEffectChoice},
    engine::{self, CastMode, Decision},
    ids::{ObjectId, PlayerId},
    mana::{Cost, ManaColor, Pip},
    optional_trigger_public_v1 as public,
    policy_surface_v5::PolicyDecisionV5,
    rl::PlayerSeatV1,
    state::{CastMethodV4, GameState, StackItemKind, Target, Zone},
    surface_v2::SurfaceDecision,
    tinybones_public_v72::IncarnationV72,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PRINTING: u16 = native::PRINTING;
pub const NAME: &str = "Blasphemous Edict";
pub const SCHEMA: &str = "mtg-kernel/edict-public/v1";
const MEANING_SCHEMA: &str = "rules-meaning/blasphemous-edict/v1";

pub(crate) fn selected_id() -> Result<u16, String> {
    if card_def::card_id_by_name(NAME) != Some(PRINTING) || !native::is_printing(PRINTING) {
        return Err("Edict requires its exact pure Limited printing".into());
    }
    let def = &card_def::CARD_DEFS[usize::from(PRINTING)];
    let Some(AltCostDef { components, condition }) = def.alt_cost else {
        return Err("Edict alternative payment absent".into());
    };
    if !crate::rules_meaning_graveyard_return_v61::ordinary_foreign_fields_except_alt_cost_absent(def)
        || def.name != NAME
        || def.capability != CardCapability::Partial
        || def.cost.generic != 3
        || def.cost.x_count != 0
        || def.cost.pips != [Pip::Colored(ManaColor::B); 2]
        || condition != AltCostCondition::BattlefieldCreatureCountAtLeast(13)
        || components != [CostComponent::Mana(Cost {
            pips: &[Pip::Colored(ManaColor::B)], generic: 0, x_count: 0,
        })]
        || def.mana_value != 5
        || def.types != [CardType::Sorcery]
        || !def.subtypes.is_empty()
        || !def.supertypes.is_empty()
        || def.colors != [ManaColor::B]
        || def.power.is_some()
        || def.toughness.is_some()
        || def.keywords != Keywords::NONE
        || def.is_token
        || def.target_spec != TargetSpec::None
        || (def.spell_effect)() != Some(EffectOp::BlasphemousEdictV1)
        || !crate::trigger::triggers_for(PRINTING).is_empty()
    {
        return Err("complete Blasphemous Edict printing changed".into());
    }
    Ok(PRINTING)
}

pub fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [
            SCHEMA, MEANING_SCHEMA,
            include_str!("edict_public_v1.rs"),
            include_str!("edict_receipts_v1.rs"),
            include_str!("spell_costs_v1.rs"),
            include_str!("engine.rs"),
            include_str!("effect.rs"),
            include_str!("event.rs"),
            include_str!("state.rs"),
            include_str!("../build.rs"),
        ] {
            hash.update((source.len() as u64).to_le_bytes());
            hash.update(source.as_bytes());
        }
        format!("{:x}", hash.finalize())
    }).clone()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV1 {
    pub schema: String,
    pub producer_sha256: String,
    pub card_db_hash: u64,
    pub card_id: u16,
    pub card_type: CardType,
    pub color: ManaColor,
    pub mana_value: u16,
    pub printed_generic: u8,
    pub printed_black: u8,
    pub alternative_black: u8,
    pub alternative_at_least_effective_creatures: u8,
    pub each_controller_sacrifices_up_to_all_if_fewer: u8,
    pub mandatory_untargeted: bool,
    pub active_nonactive_then_physical_owner_graveyard_order: bool,
    pub simultaneous_sacrifice: bool,
}

pub fn printed_meaning() -> Result<MeaningV1, String> {
    Ok(MeaningV1 {
        schema: MEANING_SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_db_hash: card_def::KERNEL_CARDDB_HASH,
        card_id: selected_id()?,
        card_type: CardType::Sorcery,
        color: ManaColor::B,
        mana_value: 5,
        printed_generic: 3,
        printed_black: 2,
        alternative_black: 1,
        alternative_at_least_effective_creatures: 13,
        each_controller_sacrifices_up_to_all_if_fewer: 13,
        mandatory_untargeted: true,
        active_nonactive_then_physical_owner_graveyard_order: true,
        simultaneous_sacrifice: true,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaymentV1 {
    pub method: CastMethodV4,
    pub creatures_before: u32,
    pub mana_spent: u32,
}

impl From<native::PaymentV1> for PaymentV1 {
    fn from(payment: native::PaymentV1) -> Self {
        Self { method: payment.source.cast_method, creatures_before: payment.creatures_before,
            mana_spent: payment.mana_spent }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CastV1 {
    pub stack_position: u32,
    pub card: public::ReferenceV1,
    pub announcing: bool,
    pub resolving: bool,
    pub uncast_copy: bool,
    pub origin_zone: Option<Zone>,
    pub payment: Option<PaymentV1>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuoteV1 {
    pub mode: CastMode,
    pub payable_mana_now: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuotesV1 {
    pub card: public::ReferenceV1,
    pub modes: Vec<QuoteV1>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ChoiceStageV1 {
    SacrificeSelection,
    PhysicalOwnerGraveyardOrder,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChoiceV1 {
    pub source: public::ReferenceV1,
    pub active: PlayerSeatV1,
    pub chooser: PlayerSeatV1,
    pub stage: ChoiceStageV1,
    pub required: u16,
    pub selected: Vec<public::ReferenceV1>,
    pub legal: Vec<public::ReferenceV1>,
    pub completed_controller_choices: [Vec<public::ReferenceV1>; 2],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VictimV1 {
    pub original: IncarnationV72,
    pub identity: public::IdentityV1,
    pub committed_destination: Option<Zone>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutcomeV1 {
    pub original_spell: IncarnationV72,
    pub copied_spell: bool,
    pub active: PlayerSeatV1,
    pub payment: Option<PaymentV1>,
    pub sacrifices_in_apnap_order: Vec<VictimV1>,
    pub actual_graveyard_order: [Vec<IncarnationV72>; 2],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV1 {
    pub schema: String,
    pub viewer: PlayerSeatV1,
    pub meaning: MeaningV1,
    pub effective_creatures_now: u32,
    pub alternate_condition_now: bool,
    pub casts: Vec<CastV1>,
    pub chooser_cast_quotes: Option<QuotesV1>,
    pub chooser_sacrifice_or_order: Option<ChoiceV1>,
    pub resolved: Vec<OutcomeV1>,
}

fn visible(state: &GameState, viewer: PlayerId, object: ObjectId) -> Result<public::ReferenceV1, String> {
    public::reference(state, object, Some(viewer)).ok_or_else(|| "Edict original card is not visible".into())
}

fn targets(state: &GameState, viewer: PlayerId, list: &[EffectTargetCandidate]) -> Result<Vec<public::ReferenceV1>, String> {
    list.iter().map(|candidate| {
        let binding = candidate.expected_object.ok_or("Edict original object binding absent")?;
        let card = visible(state, viewer, binding.object)?;
        if card.zone != Zone::Battlefield || card.zone != binding.expected_zone
            || card.generation != binding.expected_zone_change_count
            || candidate.target != Target::Object(binding.object)
        {
            return Err("Edict original choice incarnation changed".into());
        }
        Ok(card)
    }).collect()
}

fn identity(printing: u16) -> public::IdentityV1 {
    match card_def::CARD_DEFS[usize::from(printing)].name {
        "Face-down creature" => public::IdentityV1::FaceDown { disguised: true },
        "Face-down card" => public::IdentityV1::FaceDown { disguised: false },
        _ => public::IdentityV1::Known { card_id: printing },
    }
}

fn outcomes(state: &GameState) -> Result<Vec<OutcomeV1>, String> {
    native::outcomes(state)?.into_iter().map(|row| {
        let source = row.item;
        let sacrifices_in_apnap_order = row.victims.into_iter().map(|(victim, committed_destination)| VictimV1 {
            original: IncarnationV72 {
                object: victim.binding.object.0,
                generation: victim.binding.expected_zone_change_count,
                owner: victim.owner.into(),
                controller: victim.controller.into(),
                zone: Zone::Battlefield,
            },
            identity: identity(victim.public_printing),
            committed_destination,
        }).collect::<Vec<_>>();
        let mut actual_graveyard_order = [vec![], vec![]];
        for (seat, ids) in row.graveyard.iter().enumerate() {
            for object in ids {
                let victim = sacrifices_in_apnap_order.iter().find(|victim| victim.original.object == object.0)
                    .ok_or("Edict graveyard order lost original victim")?;
                actual_graveyard_order[seat].push(victim.original.clone());
            }
        }
        Ok(OutcomeV1 {
            original_spell: IncarnationV72 {
                object: source.source.0, generation: source.zone_change_count, owner: source.owner.into(),
                controller: source.controller.into(), zone: source.zone,
            },
            copied_spell: row.copied_spell, active: row.active.into(), payment: row.payment.map(Into::into),
            sacrifices_in_apnap_order, actual_graveyard_order,
        })
    }).collect()
}

pub(crate) fn validate_native(state: &GameState) -> Result<(), String> {
    let pending = state.engine.pending_cast.as_ref();
    for item in &state.stack {
        if item.kind == StackItemKind::Spell
            && item.v4.source_contract.is_some_and(|source| native::is_printing(source.card_def))
            && !pending.is_some_and(|pending| pending.spell == item.source)
        {
            native::payment_for(state, item)?;
        }
    }
    native::outcomes(state)?;
    Ok(())
}

pub(crate) fn capture(
    state: &GameState,
    viewer: PlayerId,
    origin: &PolicyDecisionV5,
) -> Result<FactsV1, String> {
    let meaning = printed_meaning()?;
    let effective_creatures_now = native::creature_count(state)?;
    let pending = state.engine.pending_cast.as_ref();
    let chooser_cast_quotes = if let PolicyDecisionV5::Surface(SurfaceDecision::Decision(
        Decision::ChooseCastMode { player, spell, options }
    )) = origin
    {
        if *player == viewer && public::visible_known(state, *spell, Some(viewer), |id| id == PRINTING) {
            let pending = pending.filter(|pending| pending.spell == *spell && pending.controller == viewer)
                .ok_or("Edict quote lost original announcing cast")?;
            engine::validate_pending_cast(state, pending)?;
            let modes = options.iter().map(|&mode| {
                let payable_mana_now = engine::edict_pending_mana_quote_v1(state, pending, mode)
                    .ok_or("Edict native payment option lost its quote")?;
                Ok(QuoteV1 { mode, payable_mana_now })
            }).collect::<Result<_, String>>()?;
            Some(QuotesV1 { card: visible(state, viewer, *spell)?, modes })
        } else { None }
    } else { None };
    let mut casts = vec![];
    for (position, item) in state.stack.iter().enumerate() {
        if item.kind != StackItemKind::Spell
            || !public::visible_known(state, item.source, Some(viewer), |id| id == PRINTING)
        {
            continue;
        }
        let validated = engine::validated_stack_item_target_spec(item, state)?;
        let source = item.v4.source_contract.ok_or("Edict original stack source absent")?;
        let announcing = pending.is_some_and(|pending| pending.spell == item.source);
        if validated != (!announcing).then_some(TargetSpec::None) {
            return Err("Edict targetless stack shape changed".into());
        }
        let resolving = state.engine.pending_effect.as_ref().is_some_and(|p| p.resolving_item == *item);
        casts.push(CastV1 {
            stack_position: u32::try_from(position).map_err(|_| "Edict stack position exceeds u32")?,
            card: visible(state, viewer, item.source)?,
            announcing, resolving, uncast_copy: item.is_copy,
            origin_zone: source.spell_cast_origin.map(|origin| origin.origin_zone),
            payment: if announcing { None } else { native::payment_for(state, item)?.map(Into::into) },
        });
    }
    let mut chooser_sacrifice_or_order = None;
    if let Some(pending) = state.engine.pending_effect.as_ref() {
        if let Some(PendingEffectChoice::SelectTargets {
            player, selected, legal, min_targets, ordered,
            purpose: EffectTargetSelectionPurpose::BlasphemousEdictV1 { choice }, ..
        }) = pending.choice.as_ref()
        {
            crate::effect::validate_pending_effect_choice(state)?;
            if *player == viewer {
                let (active, graveyard) = choice.stage_for_consumer();
                if graveyard != *ordered {
                    return Err("Edict native choice ordering changed".into());
                }
                let mut completed_controller_choices = [vec![], vec![]];
                for seat in [PlayerId::P0, PlayerId::P1] {
                    for binding in choice.completed_sacrifices_for_consumer(seat) {
                        let card = visible(state, viewer, binding.object)?;
                        if card.generation != binding.expected_zone_change_count
                            || card.zone != Zone::Battlefield
                            || card.controller != seat.into()
                        {
                            return Err("Edict completed controller choice lost its original binding".into());
                        }
                        completed_controller_choices[seat.index()].push(card);
                    }
                }
                chooser_sacrifice_or_order = Some(ChoiceV1 {
                    source: visible(state, viewer, pending.resolving_item.source)?,
                    active: active.into(), chooser: (*player).into(), required: *min_targets,
                    stage: if graveyard { ChoiceStageV1::PhysicalOwnerGraveyardOrder } else { ChoiceStageV1::SacrificeSelection },
                    selected: targets(state, viewer, selected)?,
                    legal: targets(state, viewer, legal)?,
                    completed_controller_choices,
                });
            }
        }
    }
    Ok(FactsV1 {
        schema: SCHEMA.into(), viewer: viewer.into(), meaning, effective_creatures_now,
        alternate_condition_now: effective_creatures_now >= native::LIMIT as u32,
        casts, chooser_cast_quotes, chooser_sacrifice_or_order, resolved: outcomes(state)?,
    })
}
