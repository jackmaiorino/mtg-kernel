//! Exact Leyline Axe meaning and actor-visible opening, spell, and equipment facts.
use crate::{
    card_def::{self, CardType, Keywords, Subtype, TargetSpec},
    effect::{EffectOp, ObjectRef},
    engine,
    ids::{ObjectId, PlayerId},
    leyline_axe_v1 as native, optional_trigger_public_v1 as public,
    rl::PlayerSeatV1,
    state::{
        AbilitySourceContractV4, CastMethodV4, GameState, StackItemKind, StackTargetContractV4,
        Target, Zone,
    },
    tinybones_public_v72::{CountersV72, IncarnationV72},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const NAME: &str = "Leyline Axe";
pub const PRINTING: u16 = 482;
pub const SCHEMA: &str = "mtg-kernel/leyline-axe-public/v1";
const MEANING_SCHEMA: &str = "rules-meaning/leyline-axe/v1";

pub(crate) fn selected_id() -> Result<u16, String> {
    if card_def::card_id_by_name(NAME) != Some(PRINTING)
        || !native::definition(PRINTING)
        || !crate::trigger::triggers_for(PRINTING).is_empty()
    {
        return Err("complete Leyline Axe printing changed".into());
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
            include_str!("leyline_axe_public_v1.rs"),
            include_str!("leyline_axe_v1.rs"),
            include_str!("london_mulligan_v1.rs"),
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
    pub card_type: CardType,
    pub subtype: Subtype,
    pub colorless: bool,
    pub cast_generic_mana: u8,
    pub mana_value: u16,
    pub equip_generic_mana: u8,
    pub equip_sorcery_speed_only: bool,
    pub equip_targets_creature_you_control: bool,
    pub power_bonus: i32,
    pub toughness_bonus: i32,
    pub granted_keyword_bits: u32,
    pub optional_start_on_battlefield_from_kept_opening_hand: bool,
    pub opening_entry_uses_stack: bool,
}

pub fn printed_meaning() -> Result<MeaningV1, String> {
    Ok(MeaningV1 {
        schema: MEANING_SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_db_hash: card_def::KERNEL_CARDDB_HASH,
        card_id: selected_id()?,
        card_type: CardType::Artifact,
        subtype: Subtype::Equipment,
        colorless: true,
        cast_generic_mana: 4,
        mana_value: 4,
        equip_generic_mana: 3,
        equip_sorcery_speed_only: true,
        equip_targets_creature_you_control: true,
        power_bonus: 1,
        toughness_bonus: 1,
        granted_keyword_bits: Keywords::DOUBLE_STRIKE.0 | Keywords::TRAMPLE.0,
        optional_start_on_battlefield_from_kept_opening_hand: true,
        opening_entry_uses_stack: false,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpeningEntryV1 {
    pub original: IncarnationV72,
    pub current_original: Option<public::ReferenceV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CastV1 {
    pub stack_position: u32,
    pub card: public::ReferenceV1,
    pub announcing: bool,
    pub uncast_copy: bool,
    pub origin_zone: Option<Zone>,
    pub finalized_method: Option<CastMethodV4>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetV1 {
    pub original: IncarnationV72,
    pub current_original: Option<public::ReferenceV1>,
    pub currently_legal: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquipV1 {
    pub stack_position: Option<u32>,
    pub source: public::RootV1,
    pub target: Option<TargetV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostV1 {
    pub card: public::ReferenceV1,
    pub form: public::FormV1,
    pub counters: CountersV72,
    pub receives_printed_bonus: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquipmentV1 {
    pub card: public::ReferenceV1,
    pub token: bool,
    pub host: Option<HostV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV1 {
    pub schema: String,
    pub viewer: PlayerSeatV1,
    pub meaning: MeaningV1,
    pub opening_choice: Option<public::ReferenceV1>,
    pub opening_entries: Vec<OpeningEntryV1>,
    pub casts: Vec<CastV1>,
    pub equip: Vec<EquipV1>,
    pub battlefield: Vec<EquipmentV1>,
}

fn ordinal(position: usize) -> Result<u32, String> {
    u32::try_from(position).map_err(|_| "Leyline Axe native position overflow".into())
}

fn visible(
    state: &GameState,
    viewer: PlayerId,
    id: ObjectId,
) -> Result<public::ReferenceV1, String> {
    public::reference(state, id, Some(viewer))
        .ok_or_else(|| "Leyline Axe visible card missing".into())
}

fn original_battlefield(
    state: &GameState,
    viewer: PlayerId,
    original: &IncarnationV72,
) -> Option<public::ReferenceV1> {
    let id = ObjectId(original.object);
    state
        .objects
        .try_get(id)
        .filter(|o| o.zone == Zone::Battlefield && o.zone_change_count == original.generation)
        .and_then(|_| public::reference(state, id, Some(viewer)))
}

pub(crate) fn is_attachment_program(effect: &EffectOp) -> bool {
    *effect
        == EffectOp::AttachSourceToTarget {
            object: ObjectRef::Target(0),
        }
}

fn equip_target(
    state: &GameState,
    viewer: PlayerId,
    source: AbilitySourceContractV4,
    targets: &[Target],
    contracts: &[StackTargetContractV4],
    published: Option<&crate::state::StackItem>,
) -> Result<Option<TargetV1>, String> {
    let spec = TargetSpec::ControlledCreature;
    let (chosen, contract) = match (targets, contracts) {
        ([], []) => return Ok(None),
        ([target], [contract]) => (*target, *contract),
        _ => return Err("Leyline Axe native equip target count changed".into()),
    };
    if !crate::state::stack_target_contract_is_structurally_valid(state, spec, 0, chosen, contract)
        && !published.is_some_and(|item| {
            crate::state::stack_target_contract_for_sampled_snapshot(
                state, item, spec, 0, chosen, contract,
            )
        })
    {
        return Err("Leyline Axe native equip target contract changed".into());
    }
    let StackTargetContractV4::Object {
        object,
        owner,
        controller,
        zone,
        zone_change_count,
        ..
    } = contract
    else {
        return Err("Leyline Axe equip target is not an original creature".into());
    };
    let original = IncarnationV72 {
        object: object.0,
        generation: zone_change_count,
        owner: owner.into(),
        controller: controller.into(),
        zone,
    };
    Ok(Some(TargetV1 {
        current_original: original_battlefield(state, viewer, &original),
        original,
        currently_legal: engine::target_contract_matches_live(state, chosen, contract)
            && engine::effect_target_is_legal_from_ability_source(
                state,
                source,
                source.controller,
                spec,
                targets,
                0,
            ),
    }))
}

pub(crate) fn capture(state: &GameState, viewer: PlayerId) -> Result<FactsV1, String> {
    let meaning = printed_meaning()?;
    let (opening, entries) = native::visible_opening_v1(state, viewer)?;
    state
        .validate_attachment_relations()
        .map_err(|_| "Leyline Axe native attachment relation changed")?;
    let opening_choice = opening
        .map(|binding| {
            visible(state, viewer, binding.object).and_then(|card| {
                if card.owner != viewer.into()
                    || card.zone != Zone::Hand
                    || card.generation != binding.zone_change_count
                {
                    return Err("Leyline Axe opening chooser lost original private source".into());
                }
                Ok(card)
            })
        })
        .transpose()?;
    let opening_entries = entries
        .into_iter()
        .map(|(player, entry)| {
            let original = IncarnationV72 {
                object: entry.object.0,
                generation: entry.zone_change_count,
                owner: player.into(),
                controller: player.into(),
                zone: Zone::Battlefield,
            };
            let current_original = original_battlefield(state, viewer, &original)
                .filter(|card| card.identity == (public::IdentityV1::Known { card_id: PRINTING }));
            OpeningEntryV1 {
                current_original,
                original,
            }
        })
        .collect();
    let mut battlefield = vec![];
    for player in [PlayerId::P0, PlayerId::P1] {
        for &id in &state.players[player.index()].battlefield {
            if !public::visible_known(state, id, Some(viewer), |id| id == PRINTING) {
                continue;
            }
            let live = state.objects.get(id);
            let host = live
                .v4
                .attached_to
                .map(|link| {
                    let card = visible(state, viewer, link.object)?;
                    if card.zone != Zone::Battlefield || card.generation != link.zone_change_count {
                        return Err(
                            "Leyline Axe host lost original attachment incarnation".to_string()
                        );
                    }
                    Ok(HostV1 {
                        card,
                        form: public::checked_form(state, link.object)?,
                        counters: crate::tinybones_public_v72::counters(state, link.object)?,
                        receives_printed_bonus: engine::attached_equipment_profiles(
                            state,
                            link.object,
                        )
                        .any(|(equipment, _)| equipment == id),
                    })
                })
                .transpose()?;
            battlefield.push(EquipmentV1 {
                card: visible(state, viewer, id)?,
                token: live.v4.is_token,
                host,
            });
        }
    }
    let mut equip = vec![];
    if let Some(pending) = state.engine.pending_activation.as_ref().filter(|pending| {
        public::visible_known(state, pending.source, Some(viewer), |id| id == PRINTING)
    }) {
        engine::validate_pending_activation(state, pending)?;
        if pending.ability_index != 0
            || pending.target_spec != TargetSpec::ControlledCreature
            || !pending.cauldron_grant.is_empty()
        {
            return Err("Leyline Axe pending equip lost its printed route".into());
        }
        let source = AbilitySourceContractV4::capture(state, pending.source);
        equip.push(EquipV1 {
            stack_position: None,
            source: public::root(state, source, Some(viewer))?,
            target: equip_target(
                state,
                viewer,
                source,
                &pending.targets_chosen,
                &pending.target_contracts,
                None,
            )?,
        });
    }
    let mut casts = vec![];
    for (position, item) in state.stack.iter().enumerate() {
        if item.kind == StackItemKind::Spell
            && public::visible_known(state, item.source, Some(viewer), |id| id == PRINTING)
        {
            let target_spec = engine::validated_stack_item_target_spec(item, state)?;
            let origin = item
                .v4
                .source_contract
                .ok_or("Leyline Axe stack source absent")?
                .spell_cast_origin;
            let pending = state
                .engine
                .pending_cast
                .as_ref()
                .filter(|pending| pending.spell == item.source);
            if let Some(pending) = pending {
                engine::validate_pending_cast(state, pending)?;
            }
            let announcing = pending.is_some();
            if target_spec != (!announcing).then_some(TargetSpec::None) {
                return Err("Leyline Axe stack spell target shape changed".into());
            }
            casts.push(CastV1 {
                stack_position: ordinal(position)?,
                card: visible(state, viewer, item.source)?,
                announcing,
                uncast_copy: item.is_copy,
                origin_zone: origin.map(|origin| origin.origin_zone),
                finalized_method: origin.and_then(|origin| origin.finalized_method),
            });
        } else if let Some(source) = item
            .v4
            .ability_source_contract
            .filter(|source| source.card_def == PRINTING)
        {
            if item.kind != StackItemKind::ActivatedAbility
                || item.v4.activated_ability_index != Some(0)
                || item.v4.granted_by.is_some()
                || !item
                    .inline_effect
                    .as_ref()
                    .is_some_and(is_attachment_program)
                || engine::validated_stack_item_target_spec(item, state)?
                    != Some(TargetSpec::ControlledCreature)
            {
                return Err("Leyline Axe published equip lost its printed route".into());
            }
            let target = equip_target(
                state,
                viewer,
                source,
                &item.targets,
                &item.v4.target_contracts,
                Some(item),
            )?
            .ok_or("Leyline Axe published equip lost its target")?;
            equip.push(EquipV1 {
                stack_position: Some(ordinal(position)?),
                source: public::root(state, source, Some(viewer))?,
                target: Some(target),
            });
        }
    }
    Ok(FactsV1 {
        schema: SCHEMA.into(),
        viewer: viewer.into(),
        meaning,
        opening_choice,
        opening_entries,
        casts,
        equip,
        battlefield,
    })
}
