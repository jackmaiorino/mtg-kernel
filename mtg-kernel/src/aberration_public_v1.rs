//! Exact Consuming Aberration meaning and viewer-authorized native state.
use crate::{
    card_def::{self, CardCapability, CardType, Keywords, Subtype, TargetSpec},
    effect::{EffectObjectBinding, EffectOp, EffectTargetCandidate, EffectTargetSelectionPurpose, ObjectRef, PendingEffectChoice, PlayerRef},
    engine,
    ids::PlayerId,
    mana::{ManaColor, Pip},
    optional_trigger_public_v1 as public,
    rl::PlayerSeatV1,
    state::{GameState, Target, Zone},
    trigger::TriggerCondition,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const NAME: &str = "Consuming Aberration";
pub const PRINTING: u16 = 467;
pub const SCHEMA: &str = "mtg-kernel/aberration-public/v1";
const MEANING_SCHEMA: &str = "rules-meaning/consuming-aberration/v1";
const MILL: EffectOp = EffectOp::RevealUntilCardTypeAndMill {
    player: PlayerRef::Opponent,
    card_type: CardType::Land,
};

pub(crate) fn selected_id() -> Result<u16, String> {
    if !cfg!(all(feature = "limited-fdn-fixtures", not(feature = "standard-magezero-fixtures")))
        || card_def::card_id_by_name(NAME) != Some(PRINTING)
    {
        return Err("Aberration requires its exact pure Limited printing".into());
    }
    let def = &card_def::CARD_DEFS[usize::from(PRINTING)];
    let [trigger] = crate::trigger::triggers_for(PRINTING) else {
        return Err("Aberration printed trigger count changed".into());
    };
    if !crate::rules_meaning_graveyard_return_v61::ordinary_foreign_fields_absent(def)
        || !engine::consuming_aberration_cda_for_v1(def.name)
        || def.capability != CardCapability::Partial
        || def.cost.generic != 3
        || def.cost.x_count != 0
        || def.cost.pips != [Pip::Colored(ManaColor::U), Pip::Colored(ManaColor::B)]
        || def.colors != [ManaColor::U, ManaColor::B]
        || def.mana_value != 5
        || def.types != [CardType::Creature]
        || def.subtypes != [Subtype::Horror]
        || !def.supertypes.is_empty()
        || def.power != Some(0)
        || def.toughness != Some(0)
        || def.keywords != Keywords::NONE
        || def.is_token
        || def.target_spec != TargetSpec::None
        || (def.spell_effect)() != Some(EffectOp::MoveObject {
            object: ObjectRef::ThisSource,
            to_zone: Zone::Battlefield,
        })
        || trigger.condition != TriggerCondition::CastSpell
        || trigger.home_zone != Zone::Battlefield
        || trigger.intervening_if_kicked
        || trigger.intervening_if_controls_another_source_card
        || trigger.face_index != 0
        || (trigger.effect)() != MILL
    {
        return Err("complete Aberration printing changed".into());
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
            include_str!("aberration_public_v1.rs"),
            include_str!("engine.rs"),
            include_str!("effect.rs"),
            include_str!("trigger.rs"),
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
    pub mana_value: u16,
    pub generic_mana: u8,
    pub colored_pips: [ManaColor; 2],
    pub creature_subtype: Subtype,
    pub power_and_toughness_count_opponents_graveyard_cards: bool,
    pub cda_applies_in_every_zone: bool,
    pub controller_on_battlefield_and_stack_else_owner: bool,
    pub cda_counts_physical_cards_excluding_tokens_and_spell_copies: bool,
    pub controller_cast_triggers_only_from_battlefield: bool,
    pub copies_without_casting_do_not_trigger: bool,
    pub each_opponent_reveals_through_first_land_inclusive_or_library_end: bool,
    pub no_targets_or_optional_decline: bool,
    pub each_owner_orders_actual_graveyard_entries_after_replacements: bool,
}

pub fn printed_meaning() -> Result<MeaningV1, String> {
    Ok(MeaningV1 {
        schema: MEANING_SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_db_hash: card_def::KERNEL_CARDDB_HASH,
        card_id: selected_id()?,
        mana_value: 5,
        generic_mana: 3,
        colored_pips: [ManaColor::U, ManaColor::B],
        creature_subtype: Subtype::Horror,
        power_and_toughness_count_opponents_graveyard_cards: true,
        cda_applies_in_every_zone: true,
        controller_on_battlefield_and_stack_else_owner: true,
        cda_counts_physical_cards_excluding_tokens_and_spell_copies: true,
        controller_cast_triggers_only_from_battlefield: true,
        copies_without_casting_do_not_trigger: true,
        each_opponent_reveals_through_first_land_inclusive_or_library_end: true,
        no_targets_or_optional_decline: true,
        each_owner_orders_actual_graveyard_entries_after_replacements: true,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CdaV1 {
    pub card: public::ReferenceV1,
    pub relative_player: PlayerSeatV1,
    pub base_from_cda: Option<i32>,
    pub form: public::FormV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StageV1 {
    Collected,
    Published,
    Revealed,
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
pub struct TriggerV1 {
    pub source: public::RootV1,
    pub stage: StageV1,
    pub stack_position: Option<u32>,
    pub library_owner: PlayerSeatV1,
    pub revealed_prefix: Vec<public::ReferenceV1>,
    pub order: Option<OrderV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV1 {
    pub schema: String,
    pub viewer: PlayerSeatV1,
    pub meaning: MeaningV1,
    pub characteristics: Vec<CdaV1>,
    pub triggers: Vec<TriggerV1>,
}

fn library_ref(
    state: &GameState,
    viewer: PlayerId,
    owner: PlayerId,
    binding: EffectObjectBinding,
) -> Result<public::ReferenceV1, String> {
    let card = public::reference(state, binding.object, Some(viewer))
        .ok_or("Aberration original library card was not revealed to the viewer")?;
    if card.owner != owner.into()
        || binding.expected_zone != Zone::Library
        || card.zone != Zone::Library
        || card.generation != binding.expected_zone_change_count
    {
        return Err("Aberration original library incarnation changed".into());
    }
    Ok(card)
}

fn order_refs(
    state: &GameState,
    viewer: PlayerId,
    owner: PlayerId,
    candidates: &[EffectTargetCandidate],
) -> Result<Vec<public::ReferenceV1>, String> {
    candidates.iter().map(|candidate| {
        let binding = candidate.expected_object.ok_or("Aberration order lost object binding")?;
        if candidate.target != Target::Object(binding.object) {
            return Err("Aberration order lost original target".into());
        }
        library_ref(state, viewer, owner, binding)
    }).collect()
}

pub(crate) fn capture(state: &GameState, viewer: PlayerId) -> Result<FactsV1, String> {
    let meaning = printed_meaning()?;
    let mut characteristics = vec![];
    for (id, live) in state.objects.iter() {
        if !public::visible_known(state, id, Some(viewer), |id| id == PRINTING) {
            continue;
        }
        let card = public::reference(state, id, Some(viewer)).ok_or("Aberration visible card absent")?;
        let relative_player = if matches!(live.zone, Zone::Battlefield | Zone::Stack) {
            live.controller
        } else {
            live.owner
        };
        characteristics.push(CdaV1 {
            card,
            relative_player: relative_player.into(),
            base_from_cda: engine::consuming_aberration_base_v1(state, id)?,
            form: public::checked_form(state, id)?,
        });
    }

    let mut triggers = vec![];
    for held in public::held(state) {
        let Some(source) = held.source_contract.filter(|source| source.card_def == PRINTING) else {
            continue;
        };
        if held.effect != MILL {
            continue;
        }
        engine::validate_pending_trigger_identity(state, held)?;
        triggers.push(TriggerV1 {
            source: public::root(state, source, Some(viewer))?,
            stage: StageV1::Collected,
            stack_position: None,
            library_owner: source.controller.opponent().into(),
            revealed_prefix: vec![],
            order: None,
        });
    }
    for (index, item) in state.stack.iter().enumerate() {
        if !crate::effect::aberration_item_v1(item) {
            continue;
        }
        if engine::validated_stack_item_target_spec(item, state)?.is_some() {
            return Err("Aberration trigger became targeted".into());
        }
        let source = item.v4.ability_source_contract.ok_or("Aberration original trigger source absent")?;
        let owner = source.controller.opponent();
        let pending = state.engine.pending_effect.as_ref()
            .filter(|pending| pending.resolving_item.v4.stack_item_id == item.v4.stack_item_id);
        let (stage, revealed_prefix, order) = if let Some(pending) = pending {
            crate::effect::validate_pending_effect_choice(state)?;
            let Some(PendingEffectChoice::SelectTargets {
                player, selected, legal,
                purpose: EffectTargetSelectionPurpose::OrderRevealedIntoGraveyard { original_prefix, .. },
                ..
            }) = pending.choice.as_ref() else {
                return Err("Aberration lost its native revealed order".into());
            };
            let revealed = original_prefix.iter().map(|&binding| {
                library_ref(state, viewer, owner, binding)
            }).collect::<Result<Vec<_>, _>>()?;
            let order = if *player == viewer {
                Some(OrderV1 {
                    chooser: (*player).into(),
                    selected: order_refs(state, viewer, owner, selected)?,
                    legal: order_refs(state, viewer, owner, legal)?,
                })
            } else {
                None
            };
            (StageV1::Revealed, revealed, order)
        } else {
            (StageV1::Published, vec![], None)
        };
        triggers.push(TriggerV1 {
            source: public::root(state, source, Some(viewer))?,
            stage,
            stack_position: Some(u32::try_from(index).map_err(|_| "Aberration stack position overflow")?),
            library_owner: owner.into(),
            revealed_prefix,
            order,
        });
    }
    Ok(FactsV1 { schema: SCHEMA.into(), viewer: viewer.into(), meaning, characteristics, triggers })
}
