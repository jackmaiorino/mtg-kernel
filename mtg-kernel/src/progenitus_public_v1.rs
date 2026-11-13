//! Viewer-scoped Progenitus protection, shuffle receipts, and order choices.
use crate::{
    card_def::{self, Keywords, Subtype},
    effect::{EffectTargetCandidate, EffectTargetSelectionPurpose, PendingEffectChoice},
    engine,
    event::CommittedEvent,
    ids::{ObjectId, PlayerId},
    mana::ManaColor,
    optional_trigger_public_v1 as public,
    rl::PlayerSeatV1,
    state::{CastMethodV4, GameState, StackItemKind, Target, Zone},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const NAME: &str = "Progenitus";
pub const PRINTING: u16 = 480;
pub const SCHEMA: &str = "mtg-kernel/progenitus-public/v1";
const MEANING_SCHEMA: &str = "rules-meaning/progenitus/v1";

pub(crate) fn selected_id() -> Result<u16, String> {
    if card_def::card_id_by_name(NAME) != Some(PRINTING)
        || !crate::progenitus_v1::definition(PRINTING)
        || !crate::trigger::triggers_for(PRINTING).is_empty()
    {
        return Err("complete Progenitus printing changed".into());
    }
    Ok(PRINTING)
}

pub fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [
            SCHEMA,
            MEANING_SCHEMA,
            include_str!("progenitus_public_v1.rs"),
            include_str!("progenitus_v1.rs"),
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
    })
    .clone()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV1 {
    pub schema: String,
    pub producer_sha256: String,
    pub card_db_hash: u64,
    pub card_id: u16,
    pub colors: [ManaColor; 5],
    pub colored_pips_per_color: u8,
    pub mana_value: u16,
    pub legendary: bool,
    pub creature_types: [Subtype; 2],
    pub power: i32,
    pub toughness: i32,
    pub protection_prevents_damage: bool,
    pub protection_prevents_targeting: bool,
    pub protection_prevents_blocking: bool,
    pub protection_prevents_enchanting_equipping_and_fortifying: bool,
    pub mandatory_reveal_and_owner_library_shuffle_instead_of_graveyard: bool,
}

pub fn printed_meaning() -> Result<MeaningV1, String> {
    Ok(MeaningV1 {
        schema: MEANING_SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_db_hash: card_def::KERNEL_CARDDB_HASH,
        card_id: selected_id()?,
        colors: [
            ManaColor::W,
            ManaColor::U,
            ManaColor::B,
            ManaColor::R,
            ManaColor::G,
        ],
        colored_pips_per_color: 2,
        mana_value: 10,
        legendary: true,
        creature_types: [Subtype::Hydra, Subtype::Avatar],
        power: 10,
        toughness: 10,
        protection_prevents_damage: true,
        protection_prevents_targeting: true,
        protection_prevents_blocking: true,
        protection_prevents_enchanting_equipping_and_fortifying: true,
        mandatory_reveal_and_owner_library_shuffle_instead_of_graveyard: true,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermanentV1 {
    pub card: public::ReferenceV1,
    pub token: bool,
    pub form: public::FormV1,
    pub protection_from_everything_active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpellV1 {
    pub card: public::ReferenceV1,
    pub announcing: bool,
    pub uncast_copy: bool,
    pub origin_zone: Option<Zone>,
    pub finalized_method: Option<CastMethodV4>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShuffleV1 {
    pub owner: PlayerSeatV1,
    pub controller_before: PlayerSeatV1,
    pub from_zone: Zone,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrderV1 {
    pub chooser: PlayerSeatV1,
    pub selected: Vec<public::ReferenceV1>,
    pub legal: Vec<public::ReferenceV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV1 {
    pub schema: String,
    pub viewer: PlayerSeatV1,
    pub meaning: MeaningV1,
    pub battlefield: Vec<PermanentV1>,
    pub spells: Vec<SpellV1>,
    pub completed_reveals_and_shuffles: Vec<ShuffleV1>,
    pub graveyard_order: Option<OrderV1>,
}

fn visible(
    state: &GameState,
    viewer: PlayerId,
    id: ObjectId,
) -> Result<public::ReferenceV1, String> {
    public::reference(state, id, Some(viewer))
        .ok_or_else(|| "Progenitus source is not visible".into())
}

fn order_refs(
    state: &GameState,
    viewer: PlayerId,
    candidates: &[EffectTargetCandidate],
) -> Result<Vec<public::ReferenceV1>, String> {
    candidates
        .iter()
        .map(|candidate| {
            let binding = candidate
                .expected_object
                .ok_or("Progenitus order lost object binding")?;
            let card = visible(state, viewer, binding.object)?;
            if candidate.target != Target::Object(binding.object)
                || card.generation != binding.expected_zone_change_count
                || card.zone != binding.expected_zone
            {
                return Err("Progenitus order lost original incarnation".into());
            }
            Ok(card)
        })
        .collect()
}

pub(crate) fn capture(state: &GameState, viewer: PlayerId) -> Result<FactsV1, String> {
    let meaning = printed_meaning()?;
    crate::progenitus_v1::validate(state)?;
    let mut battlefield = vec![];
    for player in &state.players {
        for &id in &player.battlefield {
            if !public::visible_known(state, id, Some(viewer), |id| id == PRINTING) {
                continue;
            }
            battlefield.push(PermanentV1 {
                card: visible(state, viewer, id)?,
                token: state.objects.get(id).v4.is_token,
                form: public::checked_form(state, id)?,
                protection_from_everything_active: engine::has_effective_keyword(
                    state,
                    id,
                    Keywords::PROTECTION_FROM_EVERYTHING,
                ),
            });
        }
    }

    let mut spells = vec![];
    for item in &state.stack {
        if item.kind != StackItemKind::Spell
            || !public::visible_known(state, item.source, Some(viewer), |id| id == PRINTING)
        {
            continue;
        }
        let spec = engine::validated_stack_item_target_spec(item, state)?;
        let source = item
            .v4
            .source_contract
            .ok_or("Progenitus lost original spell source")?;
        let pending = state
            .engine
            .pending_cast
            .as_ref()
            .filter(|pending| pending.spell == item.source);
        if let Some(pending) = pending {
            engine::validate_pending_cast(state, pending)?;
        }
        if spec != pending.is_none().then_some(card_def::TargetSpec::None) {
            return Err("Progenitus spell target shape changed".into());
        }
        spells.push(SpellV1 {
            card: visible(state, viewer, item.source)?,
            announcing: pending.is_some(),
            uncast_copy: item.is_copy,
            origin_zone: source.spell_cast_origin.map(|origin| origin.origin_zone),
            finalized_method: source
                .spell_cast_origin
                .and_then(|origin| origin.finalized_method),
        });
    }

    // Native validation paired every public receipt with the original attempt
    // and actual move. The completed reveal conveys the name and originating
    // zone, without exposing the private input, a library position or a handle
    // that can follow the revealed card into its owner's shuffled library.
    let completed_reveals_and_shuffles = state
        .engine
        .event_history
        .iter()
        .filter_map(|event| {
            let CommittedEvent::ProgenitusShuffledV1 { source, .. } = event else {
                return None;
            };
            Some(ShuffleV1 {
                owner: source.owner.into(),
                controller_before: source.controller.into(),
                from_zone: source.zone,
            })
        })
        .collect();
    let graveyard_order = match state
        .engine
        .pending_effect
        .as_ref()
        .and_then(|pending| pending.choice.as_ref())
    {
        Some(PendingEffectChoice::SelectTargets {
            player,
            selected,
            legal,
            purpose: EffectTargetSelectionPurpose::OrderProgenitusGraveyardV1 { .. },
            ..
        }) if *player == viewer => {
            crate::effect::validate_pending_effect_choice(state)?;
            Some(OrderV1 {
                chooser: (*player).into(),
                selected: order_refs(state, viewer, selected)?,
                legal: order_refs(state, viewer, legal)?,
            })
        }
        _ => None,
    };
    Ok(FactsV1 {
        schema: SCHEMA.into(),
        viewer: viewer.into(),
        meaning,
        battlefield,
        spells,
        completed_reveals_and_shuffles,
        graveyard_order,
    })
}
