//! Exact Loot meaning, public land allowance and chooser-private paid look.
use crate::{
    card_def::{self, CardType, Subtype},
    effect::{
        self, loot_v1 as native, EffectObjectBinding, EffectTargetCandidate,
        EffectTargetSelectionPurpose, LibraryPartitionSelectionStage, PendingEffectChoice,
    },
    engine,
    event::CommittedEvent,
    ids::PlayerId,
    optional_trigger_public_v1 as public,
    rl::PlayerSeatV1,
    state::{GameState, StackItem, Zone},
    tinybones_public_v72::CountersV72,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PRINTING: u16 = 473;
pub const NAME: &str = "Loot, Exuberant Explorer";
pub const SCHEMA: &str = "mtg-kernel/loot-public/v1";
const MEANING_SCHEMA: &str = "rules-meaning/loot/v1";

pub(crate) fn selected_id() -> Result<u16, String> {
    if card_def::card_id_by_name(NAME) == Some(PRINTING)
        && native::definition(PRINTING)
        && crate::trigger::triggers_for(PRINTING).is_empty()
    {
        Ok(PRINTING)
    } else {
        Err("complete Loot printing changed".into())
    }
}

pub fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [
            SCHEMA,
            MEANING_SCHEMA,
            include_str!("loot_public_v1.rs"),
            include_str!("effect/loot_v1.rs"),
            include_str!("effect/squad_rallier_resolution_v1.rs"),
            include_str!("effect.rs"),
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
    pub mana_cost: crate::omniscience_public_v1::ManaCostV1,
    pub mana_value: u16,
    pub legendary: bool,
    pub card_type: CardType,
    pub subtypes: Vec<Subtype>,
    pub power: i16,
    pub toughness: i16,
    pub additional_land_each_turn_per_active_source: u8,
    pub losing_source_never_refunds_play_count: bool,
    pub activation_generic: u8,
    pub activation_green: u8,
    pub activation_taps: bool,
    pub private_look_up_to: u8,
    pub optional_creature_to_battlefield: bool,
    pub maximum_mana_value_is_resolving_controllers_current_land_count: bool,
    pub rest_to_bottom_in_random_order: bool,
}

pub fn printed_meaning() -> Result<MeaningV1, String> {
    let id = selected_id()?;
    Ok(MeaningV1 {
        schema: MEANING_SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_db_hash: card_def::KERNEL_CARDDB_HASH,
        card_id: id,
        mana_cost: card_def::CARD_DEFS[usize::from(id)].cost.into(),
        mana_value: 3,
        legendary: true,
        card_type: CardType::Creature,
        subtypes: vec![Subtype::Beast, Subtype::Noble],
        power: 1,
        toughness: 4,
        additional_land_each_turn_per_active_source: 1,
        losing_source_never_refunds_play_count: true,
        activation_generic: 4,
        activation_green: 2,
        activation_taps: true,
        private_look_up_to: 6,
        optional_creature_to_battlefield: true,
        maximum_mana_value_is_resolving_controllers_current_land_count: true,
        rest_to_bottom_in_random_order: true,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermanentV1 {
    pub card: public::ReferenceV1,
    pub form: public::FormV1,
    pub counters: CountersV72,
    pub tapped: bool,
    pub summoning_sick: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LandsV1 {
    pub player: PlayerSeatV1,
    pub controlled_lands: u32,
    pub total_plays_allowed: u32,
    pub plays_used: u8,
    pub remaining_plays: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChoiceV1 {
    pub chooser: PlayerSeatV1,
    pub looked_in_original_order: Vec<public::ReferenceV1>,
    pub legal: Vec<public::ReferenceV1>,
    pub selected: Vec<public::ReferenceV1>,
    pub maximum_mana_value: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StageV1 {
    Published,
    Resolving,
    Completed,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbilityV1 {
    pub source: public::RootV1,
    pub stage: StageV1,
    pub native_stack_position: Option<u32>,
    pub choice: Option<ChoiceV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV1 {
    pub schema: String,
    pub viewer: PlayerSeatV1,
    pub meaning: MeaningV1,
    pub battlefield: Vec<PermanentV1>,
    pub lands: [LandsV1; 2],
    pub abilities: Vec<AbilityV1>,
}

fn width(value: usize) -> Result<u32, String> {
    u32::try_from(value).map_err(|_| "Loot public count exceeds u32".into())
}
fn library_ref(
    state: &GameState,
    viewer: PlayerId,
    binding: EffectObjectBinding,
) -> Result<public::ReferenceV1, String> {
    let card = public::reference(state, binding.object, Some(viewer))
        .ok_or("Loot looked card is not known to chooser")?;
    if card.zone != Zone::Library
        || card.owner != viewer.into()
        || binding.expected_zone != Zone::Library
        || card.generation != binding.expected_zone_change_count
    {
        return Err("Loot original library incarnation changed".into());
    }
    Ok(card)
}
fn candidates(
    state: &GameState,
    viewer: PlayerId,
    list: &[EffectTargetCandidate],
) -> Result<Vec<public::ReferenceV1>, String> {
    list.iter()
        .map(|candidate| {
            let binding = candidate
                .expected_object
                .ok_or("Loot original candidate binding missing")?;
            if candidate.target != crate::state::Target::Object(binding.object) {
                return Err("Loot candidate target changed".into());
            }
            library_ref(state, viewer, binding)
        })
        .collect()
}
fn original(
    state: &GameState,
    viewer: PlayerId,
    item: &StackItem,
) -> Result<public::RootV1, String> {
    let source = item
        .v4
        .ability_source_contract
        .ok_or("Loot original ability source absent")?;
    if !native::definition(source.card_def) {
        return Err("Loot original ability printing changed".into());
    }
    public::root(state, source, Some(viewer))
}

pub(crate) fn capture(state: &GameState, viewer: PlayerId) -> Result<FactsV1, String> {
    let meaning = printed_meaning()?;
    effect::validate_pending_effect_choice(state)?;
    let mut battlefield = vec![];
    for (id, live) in state.objects.iter() {
        if live.zone == Zone::Battlefield
            && public::visible_known(state, id, Some(viewer), |id| id == PRINTING)
        {
            battlefield.push(PermanentV1 {
                card: public::reference(state, id, Some(viewer))
                    .ok_or("Loot public permanent missing")?,
                form: public::checked_form(state, id)?,
                counters: crate::tinybones_public_v72::counters(state, id)?,
                tapped: live.tapped,
                summoning_sick: live.summoning_sick,
            });
        }
    }
    let lands = [PlayerId::P0, PlayerId::P1].map(|player| {
        let total = width(native::land_allowance(state, player))?;
        let used = state.players[player.index()].lands_played_this_turn;
        Ok::<_, String>(LandsV1 {
            player: player.into(),
            controlled_lands: width(native::controlled_lands(state, player))?,
            total_plays_allowed: total,
            plays_used: used,
            remaining_plays: total.saturating_sub(u32::from(used)),
        })
    });
    let [p0, p1] = lands;
    let mut abilities = vec![];
    let mut tracked = std::collections::BTreeMap::new();
    // Only source-owned boundary status is public. Never serialize the private
    // native continuation, prefix, RNG, random bottom order, or receipt ordinal.
    for event in state
        .engine
        .event_history
        .scan_for_v1(&[crate::event_history_v1::EventKindV1::SquadRallierResolutionBoundaryV1])
    {
        if let CommittedEvent::SquadRallierResolutionBoundaryV1 { receipt } = event {
            if native::definition(receipt.source.card_def) {
                tracked.insert(receipt.stack_item_id, receipt);
            }
        }
    }
    for (id, receipt) in &tracked {
        let mut choice = None;
        if let Some(pending) = state
            .engine
            .pending_effect
            .as_ref()
            .filter(|pending| pending.resolving_item.v4.stack_item_id == *id)
        {
            native::validate_private_program(state, pending)?;
            if let Some(PendingEffectChoice::SelectTargets {
                player,
                selected,
                legal,
                purpose:
                    EffectTargetSelectionPurpose::LookTopPickToHandBottomRest {
                        requested_count: 6,
                        original_prefix,
                        stage: LibraryPartitionSelectionStage::ChooseMatchingSubset,
                        ..
                    },
                ..
            }) = pending.choice.as_ref()
            {
                if *player == viewer {
                    choice = Some(ChoiceV1 {
                        chooser: (*player).into(),
                        looked_in_original_order: original_prefix
                            .iter()
                            .map(|binding| library_ref(state, viewer, *binding))
                            .collect::<Result<_, _>>()?,
                        legal: candidates(state, viewer, legal)?,
                        selected: candidates(state, viewer, selected)?,
                        maximum_mana_value: width(native::controlled_lands(state, *player))?,
                    });
                }
            }
        }
        abilities.push(AbilityV1 {
            source: public::root(state, receipt.source, Some(viewer))?,
            stage: if receipt.pending.is_none() {
                StageV1::Completed
            } else {
                StageV1::Resolving
            },
            native_stack_position: None,
            choice,
        });
    }
    for (position, item) in state.stack.iter().enumerate() {
        if !native::item_selected(state, item) || tracked.contains_key(&item.v4.stack_item_id) {
            continue;
        }
        engine::validated_stack_item_target_spec(item, state)?;
        abilities.push(AbilityV1 {
            source: original(state, viewer, item)?,
            stage: StageV1::Published,
            native_stack_position: Some(width(position)?),
            choice: None,
        });
    }
    Ok(FactsV1 {
        schema: SCHEMA.into(),
        viewer: viewer.into(),
        meaning,
        battlefield,
        lands: [p0?, p1?],
        abilities,
    })
}
