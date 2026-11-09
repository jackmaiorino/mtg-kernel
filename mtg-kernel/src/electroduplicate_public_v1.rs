//! Exact printed copy/flashback meaning and viewer-public Electroduplicate casts.
//! Existing V74 copied-token and end-step facts retain complete native lifecycle receipts.
pub use crate::moon_public_v1::{CastStageV1, CastV1, TargetV1};
use crate::{
    card_def::{self, CardType, TargetSpec},
    engine, event,
    ids::{ObjectId, PlayerId},
    mana::ManaColor,
    optional_trigger_public_v1 as public,
    rl::PlayerSeatV1,
    state::{GameState, StackItem, StackItemKind, StackTargetContractV4, Target, Zone},
    tinybones_public_v72::{CountersV72, IncarnationV72},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const NAME: &str = "Electroduplicate";
pub const PRINTING: u16 = 471;
pub const SCHEMA: &str = "mtg-kernel/electroduplicate-public/v1";
const MEANING_SCHEMA: &str = "rules-meaning/electroduplicate/v1";
const TARGET: TargetSpec = TargetSpec::ControlledCreature;

pub(crate) fn selected_id() -> Result<u16, String> {
    if card_def::card_id_by_name(NAME) != Some(PRINTING)
        || !crate::effect::electroduplicate_definition_v1(PRINTING)
    {
        return Err("complete Electroduplicate printing changed".into());
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
            include_str!("electroduplicate_public_v1.rs"),
            include_str!("moon_public_v1.rs"),
            include_str!("effect.rs"),
            include_str!("event.rs"),
            include_str!("engine.rs"),
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
    pub color: ManaColor,
    pub generic_mana: u8,
    pub red_mana: u8,
    pub mana_value: u16,
    pub target_controlled_creature: bool,
    pub token_copies_copiable_values_at_resolution: bool,
    pub adds_copiable_haste: bool,
    pub adds_copiable_each_end_step_sacrifice_ability: bool,
    pub flashback_generic_mana: u8,
    pub flashback_red_mana: u8,
    pub flashback_casts_from_own_graveyard: bool,
    pub flashback_exiles_on_stack_departure: bool,
}
pub fn printed_meaning() -> Result<MeaningV1, String> {
    Ok(MeaningV1 {
        schema: MEANING_SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_db_hash: card_def::KERNEL_CARDDB_HASH,
        card_id: selected_id()?,
        card_type: CardType::Sorcery,
        color: ManaColor::R,
        generic_mana: 2,
        red_mana: 1,
        mana_value: 3,
        target_controlled_creature: true,
        token_copies_copiable_values_at_resolution: true,
        adds_copiable_haste: true,
        adds_copiable_each_end_step_sacrifice_ability: true,
        flashback_generic_mana: 2,
        flashback_red_mana: 2,
        flashback_casts_from_own_graveyard: true,
        flashback_exiles_on_stack_departure: true,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenV1 {
    pub card: public::ReferenceV1,
    pub form: public::FormV1,
    pub counters: CountersV72,
    pub copiable_haste_and_each_end_step_sacrifice_instances: u32,
    pub tapped: bool,
    pub summoning_sick: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV1 {
    pub schema: String,
    pub viewer: PlayerSeatV1,
    pub meaning: MeaningV1,
    pub casts: Vec<CastV1>,
    pub copied_tokens: Vec<TokenV1>,
}
fn ordinal(position: usize) -> Result<u32, String> {
    u32::try_from(position).map_err(|_| "Electroduplicate native position overflow".into())
}

fn card(state: &GameState, viewer: PlayerId, id: ObjectId) -> Result<public::ReferenceV1, String> {
    public::reference(state, id, Some(viewer))
        .ok_or_else(|| "Electroduplicate visible card missing".into())
}

fn target(
    state: &GameState,
    viewer: PlayerId,
    target: Target,
    contract: StackTargetContractV4,
    currently_legal: bool,
    published: Option<&StackItem>,
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
        return Err("Electroduplicate original target is not an object".into());
    };
    let captured_target = published.is_some_and(|item| {
        crate::state::stack_target_contract_for_storm_snapshot(
            state, item, TARGET, 0, target, contract,
        )
    });
    let requires_capture = state.thousand_year_storm_consumer_v1
        && published.is_some_and(|item| !item.is_copy);
    if zone != Zone::Battlefield
        || !captured_target
            && (requires_capture || !crate::state::stack_target_contract_is_structurally_valid(
                state, TARGET, 0, target, contract,
            ))
    {
        return Err("Electroduplicate original target incarnation changed".into());
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

pub(crate) fn capture(state: &GameState, viewer: PlayerId) -> Result<FactsV1, String> {
    let meaning = printed_meaning()?;
    crate::effect::validate_electroduplicate_live_stack_v1(state)?;
    event::validate_copied_token_abilities_v1(state)?;
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
            .ok_or("Electroduplicate native spell source missing")?;
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
                return Err("Electroduplicate pending native cast changed".into());
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
                    None,
                )?),
                _ => return Err("Electroduplicate pending target prefix changed".into()),
            };
            (CastStageV1::Announcing, original)
        } else {
            let ([selected], [contract]) =
                (item.targets.as_slice(), item.v4.target_contracts.as_slice())
            else {
                return Err("Electroduplicate published target binding changed".into());
            };
            if validated != Some(TARGET) {
                return Err("Electroduplicate published target spec changed".into());
            }
            let original = target(
                state,
                viewer,
                *selected,
                *contract,
                engine::stack_targets_still_legal(item, state)?,
                Some(item),
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

    let mut copied_tokens = vec![];
    for player in &state.players {
        for &id in &player.battlefield {
            let live = state.objects.get(id);
            if live.v4.copied_end_step_sacrifices_v1 == 0 || live.v4.face_down_v1.is_some() {
                continue;
            }
            let instances = event::copiable_token_abilities_v1(state, id)?;
            copied_tokens.push(TokenV1 {
                card: card(state, viewer, id)?,
                form: public::checked_form(state, id)?,
                counters: crate::tinybones_public_v72::counters(state, id)?,
                copiable_haste_and_each_end_step_sacrifice_instances: instances,
                tapped: live.tapped,
                summoning_sick: live.summoning_sick,
            });
        }
    }
    Ok(FactsV1 {
        schema: SCHEMA.into(),
        viewer: viewer.into(),
        meaning,
        casts,
        copied_tokens,
    })
}
