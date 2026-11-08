//! Sphinx's Tutelage public draw, target, and actual mill receipts.
use crate::{
    card_def::{
        self, ActivationTargetFilter, CardCapability, CardType, CostComponent, Keywords, TargetSpec,
    },
    effect::{EffectOp, EffectTargetSelectionPurpose, ObjectRef, PendingEffectChoice, PlayerRef},
    engine,
    event::{self, CommittedEvent as E},
    ids::{ObjectId, PlayerId},
    mana::{ManaColor, Pip},
    optional_trigger_public_v1 as public,
    rl::PlayerSeatV1,
    state::{GameState, StackItem, StackItemKind, StackTargetContractV4, Target, Zone},
    trigger::TriggerCondition,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const PRINTING: u16 = 466;
pub const NAME: &str = "Sphinx's Tutelage";
pub const SCHEMA: &str = "mtg-kernel/tutelage-public/v1";
const MEANING_SCHEMA: &str = "rules-meaning/sphinxs-tutelage/v1";

pub(crate) fn activation_program(effect: &EffectOp) -> bool {
    matches!(effect, EffectOp::Sequence(ops) if matches!(ops.as_slice(), [
        EffectOp::DrawCards { player: PlayerRef::Controller, count: 1 },
        EffectOp::DiscardCards { player: PlayerRef::Controller, count: 1 },
    ]))
}

pub(crate) fn selected_id() -> Result<u16, String> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) || card_def::card_id_by_name(NAME) != Some(PRINTING)
    {
        return Err("Tutelage requires its exact pure Limited printing".into());
    }
    let d = &card_def::CARD_DEFS[usize::from(PRINTING)];
    if !crate::rules_meaning_graveyard_return_v61::ordinary_foreign_fields_except_activated_absent(
        d,
    ) || d.name != NAME
        || d.capability != CardCapability::Partial
        || d.cost.generic != 2
        || d.cost.x_count != 0
        || d.cost.pips != [Pip::Colored(ManaColor::U)]
        || d.mana_value != 3
        || d.types != [CardType::Enchantment]
        || !d.subtypes.is_empty()
        || !d.supertypes.is_empty()
        || d.colors != [ManaColor::U]
        || d.power.is_some()
        || d.toughness.is_some()
        || d.keywords != Keywords::NONE
        || d.is_token
        || d.target_spec != TargetSpec::None
        || (d.spell_effect)()
            != Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
    {
        return Err("complete Tutelage printing changed".into());
    }
    let [a] = d.activated_abilities else {
        return Err("Tutelage activation roster changed".into());
    };
    if !matches!(a.cost, [CostComponent::Mana(cost)]
        if cost.generic == 5 && cost.x_count == 0 && cost.pips == [Pip::Colored(ManaColor::U)])
        || a.target_spec != TargetSpec::None
        || !activation_program(&(a.effect)())
        || a.activation_zone != Zone::Battlefield
        || a.sorcery_speed_only
        || a.activation_target_filter != ActivationTargetFilter::TargetSpecOnly
        || a.max_activations_per_turn.is_some()
        || a.face.is_some()
    {
        return Err("Tutelage paid draw/discard ability changed".into());
    }
    let [t] = crate::trigger::triggers_for(PRINTING) else {
        return Err("Tutelage draw trigger roster changed".into());
    };
    if t.condition != TriggerCondition::ControllerDraws
        || t.home_zone != Zone::Battlefield
        || t.intervening_if_kicked
        || t.intervening_if_controls_another_source_card
        || t.face_index != 0
        || (t.effect)() != EffectOp::BindSphinxsTutelageMillV1
        || crate::trigger::target_spec_for_trigger(PRINTING, &(t.effect)())
            != Some(TargetSpec::TargetOpponent)
    {
        return Err("Tutelage draw trigger changed".into());
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
            include_str!("tutelage_public_v1.rs"),
            include_str!("event.rs"),
            include_str!("effect.rs"),
            include_str!("trigger.rs"),
            include_str!("engine.rs"),
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
    pub cast_generic_mana: u8,
    pub cast_blue_mana: u8,
    pub mana_value: u16,
    pub activation_generic_mana: u8,
    pub activation_blue_mana: u8,
    pub draw_then_discard: [u8; 2],
    pub observes_each_successful_controller_draw: bool,
    pub opponent_targets: u8,
    pub cards_attempted_per_mill: u8,
    pub repeat_requires_two_nonland_cards_sharing_color: bool,
}

pub fn printed_meaning() -> Result<MeaningV1, String> {
    let card_id = selected_id()?;
    Ok(MeaningV1 {
        schema: MEANING_SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_db_hash: card_def::KERNEL_CARDDB_HASH,
        card_id,
        card_type: CardType::Enchantment,
        color: ManaColor::U,
        cast_generic_mana: 2,
        cast_blue_mana: 1,
        mana_value: 3,
        activation_generic_mana: 5,
        activation_blue_mana: 1,
        draw_then_discard: [1, 1],
        observes_each_successful_controller_draw: true,
        opponent_targets: 1,
        cards_attempted_per_mill: 2,
        repeat_requires_two_nonland_cards_sharing_color: true,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StageV1 {
    Captured,
    Collected,
    Held,
    ChoosingOpponent,
    Published,
    Resolving,
    MillEnded,
    LeftStackBeforeMill,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchV1 {
    pub attempted_count: u8,
    pub visible_milled_in_move_order: Vec<public::ReferenceV1>,
    pub repeated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerV1 {
    pub source: public::RootV1,
    pub stage: StageV1,
    pub native_position: Option<u32>,
    pub original_target: Option<PlayerSeatV1>,
    pub batches: Vec<BatchV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrawV1 {
    pub player: PlayerSeatV1,
    pub observers: Vec<TriggerV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrderV1 {
    pub draw_ordinal: u32,
    pub observer_ordinal: u32,
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
    pub draws: Vec<DrawV1>,
    pub current_order: Option<OrderV1>,
    pub activation_discard: Option<public::RootV1>,
}

fn ord(index: usize) -> Result<u32, String> {
    u32::try_from(index).map_err(|_| "Tutelage public ordinal overflow".into())
}

type Occurrence = (usize, usize);
type DrawRoots = BTreeMap<(u64, ObjectId), Occurrence>;

fn row(draws: &mut [DrawV1], (draw, observer): Occurrence) -> &mut TriggerV1 {
    &mut draws[draw].observers[observer]
}

fn key_for_item(state: &GameState, item: &StackItem) -> Result<(u64, ObjectId), String> {
    let capture = event::validate_tutelage_trigger_v1(state, item)?;
    Ok((capture, item.source))
}

struct MillRun<'a> {
    occurrence: Occurrence,
    library: &'a [crate::effect::EffectObjectBinding],
    target: PlayerId,
    active: bool,
    offset: usize,
    previous: Option<u64>,
    repeat: bool,
}

pub(crate) fn require_receipts(state: &GameState) -> Result<(), String> {
    if state
        .engine
        .event_history
        .iter()
        .any(|e| matches!(e, E::SphinxsTutelageMillBatchV1 { outcome, .. } if outcome.0.is_none()))
    {
        return Err("Tutelage consumer requires event-time public mill evidence".into());
    }
    Ok(())
}

pub(crate) fn capture(state: &GameState, viewer: PlayerId) -> Result<FactsV1, String> {
    let meaning = printed_meaning()?;
    require_receipts(state)?;
    crate::effect::validate_pending_effect_choice(state)?;
    let pending = state.engine.pending_effect.as_ref();
    let active_item = pending.map(|effect| &effect.resolving_item);
    let mut draws = vec![];
    let mut roots = DrawRoots::new();
    let mut published = BTreeMap::new();
    let mut runs = BTreeMap::<u64, MillRun<'_>>::new();

    for (index, entry) in state.engine.event_history.iter().enumerate() {
        match entry {
            E::SphinxsTutelageDrawV1 { .. } => {
                let sources = event::tutelage_observers_v1(state, index as u64)?;
                let mut observers = vec![];
                for &source in sources {
                    if source.card_def != PRINTING
                        || roots
                            .insert(
                                (index as u64, source.source),
                                (draws.len(), observers.len()),
                            )
                            .is_some()
                    {
                        return Err("Tutelage exact draw observer changed".into());
                    }
                    observers.push(TriggerV1 {
                        source: public::root(state, source, Some(viewer))?,
                        stage: StageV1::Captured,
                        native_position: None,
                        original_target: None,
                        batches: vec![],
                    });
                }
                draws.push(DrawV1 {
                    player: sources[0].controller.into(),
                    observers,
                });
            }
            E::SphinxsTutelageCollectedV1 {
                capture_history_index,
            } => {
                for source in event::tutelage_observers_v1(state, *capture_history_index)? {
                    let key = roots
                        .get(&(*capture_history_index, source.source))
                        .ok_or("Tutelage collection preceded its draw")?;
                    let trigger = row(&mut draws, *key);
                    if trigger.stage != StageV1::Captured {
                        return Err("duplicate Tutelage draw collection".into());
                    }
                    trigger.stage = StageV1::Collected;
                }
            }
            E::SphinxsTutelagePublishedV1 {
                item,
                capture_history_index,
            } => {
                event::validate_published_tutelage_v1(state, item)?;
                let key = key_for_item(state, item)?;
                let target = item.controller.opponent();
                if key.0 != *capture_history_index
                    || item.targets != [Target::Player(target)]
                    || item.v4.target_contracts != [StackTargetContractV4::Player(target)]
                {
                    return Err("Tutelage original opponent target changed".into());
                }
                let occurrence = *roots
                    .get(&key)
                    .ok_or("Tutelage publication lacks original draw")?;
                let trigger = row(&mut draws, occurrence);
                if trigger.stage != StageV1::Collected
                    || published
                        .insert(item.v4.stack_item_id, occurrence)
                        .is_some()
                {
                    return Err("duplicate Tutelage publication".into());
                }
                trigger.original_target = Some(target.into());
                trigger.stage = StageV1::LeftStackBeforeMill;
                if let Some(position) = state
                    .stack
                    .iter()
                    .position(|live| live.v4.stack_item_id == item.v4.stack_item_id)
                {
                    if state.stack[position] != **item {
                        return Err("Tutelage original live stack row changed".into());
                    }
                    trigger.stage = StageV1::Published;
                    trigger.native_position = Some(ord(position)?);
                }
            }
            E::SphinxsTutelageMillStartedV1 { item, library } => {
                event::validate_published_tutelage_v1(state, item)?;
                let occurrence = *published
                    .get(&item.v4.stack_item_id)
                    .ok_or("Tutelage mill start lacks original publication")?;
                let trigger = row(&mut draws, occurrence);
                if matches!(trigger.stage, StageV1::Resolving | StageV1::MillEnded) {
                    return Err("duplicate Tutelage mill start".into());
                }
                let active = active_item == Some(item.as_ref());
                trigger.stage = if active {
                    StageV1::Resolving
                } else {
                    StageV1::MillEnded
                };
                runs.insert(
                    index as u64,
                    MillRun {
                        occurrence,
                        library,
                        target: item.controller.opponent(),
                        active,
                        offset: 0,
                        previous: None,
                        repeat: true,
                    },
                );
            }
            E::SphinxsTutelageMillBatchV1 {
                start_history_index,
                previous_batch,
                ordered,
                move_start,
                ..
            } => {
                let run = runs
                    .get_mut(start_history_index)
                    .ok_or("Tutelage batch lacks original start")?;
                let pair = &run.library[run.offset..run.library.len().min(run.offset + 2)];
                if !run.repeat
                    || pair.is_empty()
                    || *previous_batch != run.previous
                    || *move_start <= run.previous.unwrap_or(*start_history_index)
                    || pair.len() != ordered.len()
                    || ordered
                        .iter()
                        .enumerate()
                        .any(|(i, member)| !pair.contains(member) || ordered[..i].contains(member))
                {
                    return Err("Tutelage completed original pair or repeat chain changed".into());
                }
                let outcome = event::tutelage_public_outcome_v1(state, index, run.target)?
                    .ok_or("Tutelage missing actual public batch")?;
                row(&mut draws, run.occurrence).batches.push(BatchV1 {
                    attempted_count: pair.len() as u8,
                    visible_milled_in_move_order: outcome
                        .members
                        .iter()
                        .map(|member| public::ReferenceV1 {
                            object: member.object.0,
                            generation: member.generation,
                            owner: run.target.into(),
                            controller: member.controller.into(),
                            zone: member.zone,
                            identity: public::IdentityV1::Known {
                                card_id: member.card_def,
                            },
                        })
                        .collect(),
                    repeated: outcome.repeat,
                });
                run.offset += pair.len();
                run.previous = Some(index as u64);
                run.repeat = outcome.repeat;
            }
            _ => {}
        }
    }
    for run in runs.values() {
        if !run.active && run.repeat && run.offset < run.library.len() {
            return Err("Tutelage completed receipt ended before its native repeat".into());
        }
    }

    for (position, trigger) in public::held(state).into_iter().enumerate() {
        let EffectOp::SphinxsTutelageMillV1 {
            capture_history_index,
        } = trigger.effect
        else {
            continue;
        };
        engine::validate_pending_trigger_identity(state, trigger)?;
        let occurrence = *roots
            .get(&(capture_history_index, trigger.source))
            .ok_or("Tutelage held trigger lacks its original draw")?;
        let row = row(&mut draws, occurrence);
        if row.stage != StageV1::Collected || !trigger.targets.is_empty() {
            return Err("Tutelage held trigger disagrees with original draw".into());
        }
        row.stage = if trigger.placement_ordered && position == 0 {
            StageV1::ChoosingOpponent
        } else {
            StageV1::Held
        };
        row.native_position = Some(ord(position)?);
    }

    let current_order = if let Some(PendingEffectChoice::SelectTargets {
        player,
        selected,
        legal,
        purpose:
            EffectTargetSelectionPurpose::SphinxsTutelageMillOrderV1 {
                start_history_index,
                ..
            },
        ..
    }) = pending.and_then(|effect| effect.choice.as_ref())
    {
        if *player == viewer {
            let run = runs
                .get(start_history_index)
                .ok_or("Tutelage order lacks original mill start")?;
            let project = |targets: &[crate::effect::EffectTargetCandidate]| {
                targets
                    .iter()
                    .map(|candidate| {
                        let Target::Object(object) = candidate.target else {
                            return Err("Tutelage order is not an object".into());
                        };
                        public::reference(state, object, Some(viewer)).ok_or_else(|| {
                            "Tutelage order card is not visible to its chooser".into()
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()
            };
            Some(OrderV1 {
                draw_ordinal: ord(run.occurrence.0)?,
                observer_ordinal: ord(run.occurrence.1)?,
                chooser: (*player).into(),
                selected: project(selected)?,
                legal: project(legal)?,
            })
        } else {
            None
        }
    } else {
        None
    };

    let activation_discard = if let Some(discard) = &state.engine.pending_discard {
        let id = match discard.resume {
            engine::DiscardResume::FinishAbilityResolution { stack_item_id }
            | engine::DiscardResume::FinishEffectContinuation { stack_item_id, .. } => {
                Some(stack_item_id)
            }
            _ => None,
        };
        let item = id.and_then(|id| state.stack.iter().find(|item| item.v4.stack_item_id == id));
        if let Some(item) = item.filter(|item| {
            item.kind == StackItemKind::ActivatedAbility
                && item
                    .v4
                    .ability_source_contract
                    .is_some_and(|source| source.card_def == PRINTING)
        }) {
            engine::validated_stack_item_target_spec(item, state)?;
            if item.v4.activated_ability_index != Some(0)
                || item.controller != discard.player
                || discard.count != 1
                || !item.inline_effect.as_ref().is_some_and(activation_program)
            {
                return Err("Tutelage draw/discard continuation changed".into());
            }
            Some(public::root(
                state,
                item.v4.ability_source_contract.unwrap(),
                Some(viewer),
            )?)
        } else {
            None
        }
    } else {
        None
    };

    Ok(FactsV1 {
        schema: SCHEMA.into(),
        viewer: viewer.into(),
        meaning,
        draws,
        current_order,
        activation_discard,
    })
}
