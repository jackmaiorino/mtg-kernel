//! Exact Imprisoned in the Moon cast, attachment, layer, and mana facts.
use crate::{
    card_def::{self, AttachmentDef, CardCapability, CardType, Keywords, Subtype, TargetSpec},
    continuous_characteristics_v1 as layers,
    effect::{EffectOp, EffectTargetSelectionPurpose, ObjectRef, PendingEffectChoice},
    engine,
    ids::{ObjectId, PlayerId},
    mana::{ManaColor, Pip},
    optional_trigger_public_v1 as public,
    rl::PlayerSeatV1,
    standard_cards_v1::StandardTargetV1,
    state::{CastMethodV4, GameState, StackItemKind, StackTargetContractV4, Target, Zone},
    tinybones_public_v72::{CountersV72, IncarnationV72},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PRINTING: u16 = 484;
pub const NAME: &str = "Imprisoned in the Moon";
pub const SCHEMA: &str = "mtg-kernel/moon-public/v1";
const MEANING_SCHEMA: &str = "rules-meaning/imprisoned-in-the-moon/v1";
const TARGET: TargetSpec = TargetSpec::StandardV1(StandardTargetV1::CreatureLandOrPlaneswalkerV1);
const TYPES: [CardType; 3] = [CardType::Creature, CardType::Land, CardType::Planeswalker];

pub(crate) fn selected_id() -> Result<u16, String> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) || card_def::card_id_by_name(NAME) != Some(PRINTING)
    {
        return Err("Moon requires its exact pure Limited printing".into());
    }
    let def = &card_def::CARD_DEFS[usize::from(PRINTING)];
    if !crate::rules_meaning_graveyard_return_v61::ordinary_foreign_fields_except_attachment_absent(
        def,
    ) || def.name != NAME
        || def.capability != CardCapability::Partial
        || def.cost.generic != 2
        || def.cost.x_count != 0
        || def.cost.pips != [Pip::Colored(ManaColor::U)]
        || def.mana_value != 3
        || def.types != [CardType::Enchantment]
        || def.subtypes != [Subtype::Aura]
        || !def.supertypes.is_empty()
        || def.colors != [ManaColor::U]
        || def.power.is_some()
        || def.toughness.is_some()
        || def.keywords != Keywords::NONE
        || def.is_token
        || def.attachment != Some(AttachmentDef::AuraLandOverride)
        || def.target_spec != TARGET
        || (def.spell_effect)()
            != Some(EffectOp::PutSourceOntoBattlefieldAttachedToTarget {
                target: ObjectRef::Target(0),
            })
        || !crate::trigger::triggers_for(PRINTING).is_empty()
    {
        return Err("complete Moon printing changed".into());
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
            include_str!("moon_public_v1.rs"),
            include_str!("continuous_characteristics_v1.rs"),
            include_str!("effect/nonland_copy_v1.rs"),
            include_str!("engine.rs"),
            include_str!("effect.rs"),
            include_str!("mana.rs"),
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
    pub generic_mana: u8,
    pub blue_mana: u8,
    pub mana_value: u16,
    pub color: ManaColor,
    pub card_type: CardType,
    pub subtype: Subtype,
    pub enchant_types: [CardType; 3],
    pub original_target_required_at_resolution: bool,
    pub layer_four_sets_land_retaining_land_subtypes_and_supertypes: bool,
    pub layer_five_sets_colorless: bool,
    pub layer_six_removes_other_abilities_and_grants_tap_for_colorless: bool,
}

pub fn printed_meaning() -> Result<MeaningV1, String> {
    let card_id = selected_id()?;
    Ok(MeaningV1 {
        schema: MEANING_SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_db_hash: card_def::KERNEL_CARDDB_HASH,
        card_id,
        generic_mana: 2,
        blue_mana: 1,
        mana_value: 3,
        color: ManaColor::U,
        card_type: CardType::Enchantment,
        subtype: Subtype::Aura,
        enchant_types: TYPES,
        original_target_required_at_resolution: true,
        layer_four_sets_land_retaining_land_subtypes_and_supertypes: true,
        layer_five_sets_colorless: true,
        layer_six_removes_other_abilities_and_grants_tap_for_colorless: true,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetV1 {
    pub original: IncarnationV72,
    pub current_original: Option<public::ReferenceV1>,
    pub currently_legal: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CastStageV1 {
    Announcing,
    Published,
    UncastCopy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CastV1 {
    pub stack_position: u32,
    pub spell: public::ReferenceV1,
    pub stage: CastStageV1,
    pub origin_zone: Option<Zone>,
    pub finalized_method: Option<CastMethodV4>,
    pub target: Option<TargetV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuraV1 {
    pub card: public::ReferenceV1,
    pub token: bool,
    pub layer_timestamp: Option<u64>,
    pub supplies_current_override: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostV1 {
    pub card: public::ReferenceV1,
    pub auras: Vec<AuraV1>,
    pub form: public::FormV1,
    pub counters: CountersV72,
    pub tapped: bool,
    pub summoning_sick: bool,
    pub moon_sets_land: bool,
    pub moon_sets_colorless: bool,
    pub moon_grants_tap_for_colorless: bool,
    pub granted_mana_available_now: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttachmentKindV1 {
    CopiedAura,
    ReturningAura,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachmentMenuV1 {
    pub kind: AttachmentKindV1,
    pub chooser: PlayerSeatV1,
    pub aura: public::ReferenceV1,
    pub selected_hosts: Vec<public::ReferenceV1>,
    pub total_hosts: u32,
    pub legal_hosts: Vec<public::ReferenceV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV1 {
    pub schema: String,
    pub viewer: PlayerSeatV1,
    pub meaning: MeaningV1,
    pub casts: Vec<CastV1>,
    pub hosts: Vec<HostV1>,
    pub attachment_menu: Option<AttachmentMenuV1>,
}

fn ordinal(position: usize) -> Result<u32, String> {
    u32::try_from(position).map_err(|_| "Moon native position overflow".into())
}

fn card(state: &GameState, viewer: PlayerId, id: ObjectId) -> Result<public::ReferenceV1, String> {
    public::reference(state, id, Some(viewer)).ok_or_else(|| "Moon visible card missing".into())
}

fn target(
    state: &GameState,
    viewer: PlayerId,
    target: Target,
    contract: StackTargetContractV4,
    currently_legal: bool,
) -> Result<TargetV1, String> {
    let StackTargetContractV4::Object {
        object,
        owner,
        controller,
        zone,
        zone_change_count,
        ..
    } = contract
    else {
        return Err("Moon original target is not an object".into());
    };
    if zone != Zone::Battlefield
        || !crate::state::stack_target_contract_is_structurally_valid(
            state, TARGET, 0, target, contract,
        )
    {
        return Err("Moon original target incarnation changed".into());
    }
    Ok(TargetV1 {
        original: IncarnationV72 {
            object: object.0,
            generation: zone_change_count,
            owner: owner.into(),
            controller: controller.into(),
            zone,
        },
        current_original: state
            .objects
            .try_get(object)
            .filter(|live| live.zone_change_count == zone_change_count && live.zone == zone)
            .and_then(|_| public::reference(state, object, Some(viewer))),
        currently_legal,
    })
}

pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    state
        .validate_attachment_relations()
        .map_err(|_| "Moon native attachment relation changed".into())
}

pub(crate) fn capture(state: &GameState, viewer: PlayerId) -> Result<FactsV1, String> {
    let meaning = printed_meaning()?;
    validate_state(state)?;
    let mut casts = vec![];
    for (position, item) in state.stack.iter().enumerate() {
        if item.kind != StackItemKind::Spell
            || !public::visible_known(state, item.source, Some(viewer), |id| id == PRINTING)
        {
            continue;
        }
        let validated = engine::validated_stack_item_target_spec(item, state)?;
        let source = item
            .v4
            .source_contract
            .ok_or("Moon native spell source missing")?;
        let spell = card(state, viewer, item.source)?;
        let origin_zone = source.spell_cast_origin.map(|origin| origin.origin_zone);
        let finalized_method = source
            .spell_cast_origin
            .and_then(|origin| origin.finalized_method);
        let (stage, target) = if let Some(pending) = state
            .engine
            .pending_cast
            .as_ref()
            .filter(|p| p.spell == item.source)
        {
            engine::validate_pending_cast(state, pending)?;
            if validated.is_some() || pending.target_spec != TARGET || item.is_copy {
                return Err("Moon pending native cast changed".into());
            }
            let original = match (
                pending.targets_chosen.as_slice(),
                pending.target_contracts.as_slice(),
            ) {
                ([], []) => None,
                ([selected], [contract]) => Some(target(
                    state,
                    viewer,
                    *selected,
                    *contract,
                    engine::target_contract_matches_live(state, *selected, *contract)
                        && engine::effect_target_is_legal(
                            state,
                            item.source,
                            item.controller,
                            TARGET,
                            &pending.targets_chosen,
                            0,
                        ),
                )?),
                _ => return Err("Moon pending target prefix changed".into()),
            };
            (CastStageV1::Announcing, original)
        } else {
            let ([selected], [contract]) =
                (item.targets.as_slice(), item.v4.target_contracts.as_slice())
            else {
                return Err("Moon published target binding changed".into());
            };
            if validated != Some(TARGET) {
                return Err("Moon published target spec changed".into());
            }
            let original = target(
                state,
                viewer,
                *selected,
                *contract,
                engine::stack_targets_still_legal(item, state)?,
            )?;
            (
                if item.is_copy {
                    CastStageV1::UncastCopy
                } else {
                    CastStageV1::Published
                },
                Some(original),
            )
        };
        casts.push(CastV1 {
            stack_position: ordinal(position)?,
            spell,
            stage,
            origin_zone,
            finalized_method,
            target,
        });
    }

    let mut hosts = vec![];
    for player in [PlayerId::P0, PlayerId::P1] {
        for &id in &state.players[player.index()].battlefield {
            let live = state.objects.get(id);
            let current = layers::land_override_timestamp(state, id);
            let mut auras = vec![];
            for &aura in &live.attachments {
                if !public::visible_known(state, aura, Some(viewer), |id| id == PRINTING) {
                    continue;
                }
                let source = state.objects.get(aura);
                let applies = layers::land_override_source_timestamp(state, id, aura);
                auras.push(AuraV1 {
                    card: card(state, viewer, aura)?,
                    token: source.v4.is_token,
                    layer_timestamp: source.v4.layer_timestamp,
                    supplies_current_override: applies.is_some() && applies == current,
                });
            }
            if auras.is_empty() {
                continue;
            }
            let grants = layers::land_grants_colorless_mana(state, id);
            hosts.push(HostV1 {
                card: card(state, viewer, id)?,
                auras,
                form: public::checked_form(state, id)?,
                counters: crate::tinybones_public_v72::counters(state, id)?,
                tapped: live.tapped,
                summoning_sick: live.summoning_sick,
                moon_sets_land: layers::land_type_timestamp(state, id).is_some(),
                moon_sets_colorless: layers::land_colorless(state, id),
                moon_grants_tap_for_colorless: grants,
                granted_mana_available_now: grants
                    && engine::available_mana_ability_choices(player, id, state)
                        .contains(&ManaColor::C),
            });
        }
    }

    let attachment_menu = state
        .engine
        .pending_effect
        .as_ref()
        .map(|pending| -> Result<Option<AttachmentMenuV1>, String> {
            let Some(PendingEffectChoice::SelectTargets {
                player,
                legal,
                purpose,
                ..
            }) = pending.choice.as_ref()
            else {
                return Ok(None);
            };
            let (kind, aura, selected_hosts, total_hosts) = match purpose {
                EffectTargetSelectionPurpose::AttachCopiedAuraV1 { choice }
                    if choice.template.card_def == PRINTING =>
                {
                    let [StackTargetContractV4::Object {
                        object,
                        owner,
                        controller,
                        zone,
                        zone_change_count,
                        ..
                    }] = pending.ctx.target_contracts.as_slice()
                    else {
                        return Err("Moon copy lost its original public Aura model".into());
                    };
                    if *zone != Zone::Battlefield {
                        return Err("Moon copy original model is outside the battlefield".into());
                    }
                    let original = public::ReferenceV1 {
                        object: object.0,
                        generation: *zone_change_count,
                        owner: (*owner).into(),
                        controller: (*controller).into(),
                        zone: *zone,
                        identity: public::IdentityV1::Known { card_id: PRINTING },
                    };
                    let selected = choice
                        .selected_hosts
                        .iter()
                        .map(|host| card(state, viewer, host.object))
                        .collect::<Result<Vec<_>, _>>()?;
                    (
                        AttachmentKindV1::CopiedAura,
                        original,
                        selected,
                        ordinal(choice.replacement.replaced_token_count(1)?)?,
                    )
                }
                EffectTargetSelectionPurpose::AttachReturningAura { aura, .. }
                    if public::visible_known(state, aura.object, Some(viewer), |id| {
                        id == PRINTING
                    }) =>
                {
                    (
                        AttachmentKindV1::ReturningAura,
                        card(state, viewer, aura.object)?,
                        vec![],
                        1,
                    )
                }
                _ => return Ok(None),
            };
            crate::effect::validate_pending_effect_choice(state)?;
            let legal_hosts = legal
                .iter()
                .map(|candidate| {
                    let Target::Object(host) = candidate.target else {
                        return Err("Moon native attachment choice is not a permanent".into());
                    };
                    card(state, viewer, host)
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(Some(AttachmentMenuV1 {
                kind,
                chooser: (*player).into(),
                aura,
                selected_hosts,
                total_hosts,
                legal_hosts,
            }))
        })
        .transpose()?
        .flatten();

    Ok(FactsV1 {
        schema: SCHEMA.into(),
        viewer: viewer.into(),
        meaning,
        casts,
        hosts,
        attachment_menu,
    })
}
