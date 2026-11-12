//! Exact actor-visible Lathril combat, payment, and realized outcomes.
use crate::{
    card_def::{
        self, ActivationTargetFilter, CardCapability, CardType, CostComponent, Keywords, Subtype,
        Supertype, TargetSpec,
    },
    effect::{EffectObjectBinding, EffectOp, ObjectRef},
    engine,
    event::{self, CommittedEvent as E},
    ids::{ObjectId, PlayerId},
    lathril_receipts_v1::{self as receipts, DrainOutcomeV1, ELVES},
    mana::{ManaColor, Pip},
    optional_trigger_public_v1 as public,
    rl::PlayerSeatV1,
    state::{AbilitySourceContractV4, CastMethodV4, GameState, StackItemKind, Zone},
    tinybones_public_v72::{CountersV72, IncarnationV72},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PRINTING: u16 = receipts::PRINTING;
pub const NAME: &str = "Lathril, Blade of the Elves";
pub const SCHEMA: &str = "mtg-kernel/lathril-public/v1";
const MEANING_SCHEMA: &str = "rules-meaning/lathril/v1";
const TOKEN_NAME: &str = "Elf Warrior Token";

fn token_id() -> Result<u16, String> {
    let id = card_def::card_id_by_name(TOKEN_NAME).ok_or("Lathril Elf Warrior token absent")?;
    let token = &card_def::CARD_DEFS[usize::from(id)];
    if !crate::rules_meaning_graveyard_return_v61::ordinary_foreign_fields_absent(token)
        || token.name != TOKEN_NAME
        || token.capability != CardCapability::Full
        || !token.is_token
        || token.mana_value != 0
        || token.cost.generic != 0
        || token.cost.x_count != 0
        || !token.cost.pips.is_empty()
        || token.types != [CardType::Creature]
        || token.subtypes != [Subtype::Elf, Subtype::Warrior]
        || !token.supertypes.is_empty()
        || token.colors != [ManaColor::G]
        || token.power != Some(1)
        || token.toughness != Some(1)
        || token.keywords != Keywords::NONE
        || token.target_spec != TargetSpec::None
        || (token.spell_effect)().is_some()
        || !crate::trigger::triggers_for(id).is_empty()
    {
        return Err("complete Lathril Elf Warrior token printing changed".into());
    }
    Ok(id)
}

pub(crate) fn selected_id() -> Result<u16, String> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) || card_def::card_id_by_name(NAME) != Some(PRINTING)
    {
        return Err("Lathril requires its exact pure Limited printing".into());
    }
    let def = &card_def::CARD_DEFS[usize::from(PRINTING)];
    let [ability] = def.activated_abilities else {
        return Err("Lathril printed activation count changed".into());
    };
    let [trigger] = crate::trigger::triggers_for(PRINTING) else {
        return Err("Lathril printed combat trigger count changed".into());
    };
    if !crate::rules_meaning_graveyard_return_v61::ordinary_foreign_fields_except_activated_absent(
        def,
    ) || def.name != NAME
        || def.capability != CardCapability::Partial
        || def.cost.generic != 2
        || def.cost.x_count != 0
        || def.cost.pips != [Pip::Colored(ManaColor::B), Pip::Colored(ManaColor::G)]
        || def.mana_value != 4
        || def.types != [CardType::Creature]
        || def.subtypes != [Subtype::Elf, Subtype::Noble]
        || def.supertypes != [Supertype::Legendary]
        || def.colors != [ManaColor::B, ManaColor::G]
        || def.power != Some(2)
        || def.toughness != Some(3)
        || def.keywords != Keywords::MENACE
        || def.is_token
        || def.target_spec != TargetSpec::None
        || (def.spell_effect)()
            != Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
        || ability.cost
            != [
                CostComponent::Tap,
                CostComponent::TapControlled {
                    count: ELVES,
                    filter: receipts::payer_filter(),
                },
            ]
        || ability.activation_zone != Zone::Battlefield
        || ability.sorcery_speed_only
        || ability.activation_target_filter != ActivationTargetFilter::TargetSpecOnly
        || ability.max_activations_per_turn.is_some()
        || ability.face.is_some()
        || ability.target_spec != TargetSpec::None
        || (ability.effect)() != receipts::recipe()
        || trigger.condition != crate::trigger::TriggerCondition::DealsCombatDamageToPlayer
        || trigger.home_zone != Zone::Battlefield
        || trigger.face_index != 0
        || trigger.intervening_if_kicked
        || trigger.intervening_if_controls_another_source_card
        || (trigger.effect)() != EffectOp::BindLathrilCombatTokensV1
        || crate::trigger::target_spec_for_trigger(PRINTING, &(trigger.effect)())
            != Some(TargetSpec::None)
    {
        return Err("complete Lathril printing changed".into());
    }
    token_id()?;
    Ok(PRINTING)
}

pub fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [
            SCHEMA,
            MEANING_SCHEMA,
            include_str!("lathril_public_v1.rs"),
            include_str!("lathril_receipts_v1.rs"),
            include_str!("effect.rs"),
            include_str!("engine.rs"),
            include_str!("event.rs"),
            include_str!("state.rs"),
            include_str!("doubling_season_v48.rs"),
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
    pub mana_value: u16,
    pub generic: u8,
    pub pips: [ManaColor; 2],
    pub card_type: CardType,
    pub subtypes: [Subtype; 2],
    pub legendary: bool,
    pub power: i32,
    pub toughness: i32,
    pub menace: bool,
    pub mandatory_untargeted_tokens_equal_final_combat_damage: bool,
    pub token_id: u16,
    pub token_color: ManaColor,
    pub token_type: CardType,
    pub token_subtypes: [Subtype; 2],
    pub token_power: i32,
    pub token_toughness: i32,
    pub source_tap_cost: bool,
    pub other_untapped_controlled_elves: u8,
    pub no_sorcery_speed_restriction: bool,
    pub non_targeting_drain: bool,
    pub opponent_commanded_loss: u8,
    pub controller_commanded_gain: u8,
}

pub fn printed_meaning() -> Result<MeaningV1, String> {
    Ok(MeaningV1 {
        schema: MEANING_SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_db_hash: card_def::KERNEL_CARDDB_HASH,
        card_id: selected_id()?,
        mana_value: 4,
        generic: 2,
        pips: [ManaColor::B, ManaColor::G],
        card_type: CardType::Creature,
        subtypes: [Subtype::Elf, Subtype::Noble],
        legendary: true,
        power: 2,
        toughness: 3,
        menace: true,
        mandatory_untargeted_tokens_equal_final_combat_damage: true,
        token_id: token_id()?,
        token_color: ManaColor::G,
        token_type: CardType::Creature,
        token_subtypes: [Subtype::Elf, Subtype::Warrior],
        token_power: 1,
        token_toughness: 1,
        source_tap_cost: true,
        other_untapped_controlled_elves: ELVES,
        no_sorcery_speed_restriction: true,
        non_targeting_drain: true,
        opponent_commanded_loss: 10,
        controller_commanded_gain: 10,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentV1 {
    pub card: public::ReferenceV1,
    pub token: bool,
    pub tapped: bool,
    pub summoning_sick: bool,
    pub counters: CountersV72,
    pub form: public::FormV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CastV1 {
    pub stack_position: u32,
    pub spell: public::ReferenceV1,
    pub announcing: bool,
    pub uncast_copy: bool,
    pub origin_zone: Option<Zone>,
    pub finalized_method: Option<CastMethodV4>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum CombatStageV1 {
    Held { position: u32 },
    Published { stack_position: u32 },
    Resolved { tokens: Vec<IncarnationV72> },
    Countered,
    RemovedWithoutResolution,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatV1 {
    pub source: public::RootV1,
    pub damaged_player: PlayerSeatV1,
    pub final_damage: u32,
    pub stage: CombatStageV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaymentV1 {
    pub source: public::ReferenceV1,
    pub chooser: PlayerSeatV1,
    pub selected: Vec<public::ReferenceV1>,
    pub remaining: u8,
    pub candidates: Vec<public::ReferenceV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum DrainStageV1 {
    Published {
        stack_position: u32,
    },
    RemovedWithoutResolution,
    Countered,
    Resolved {
        opponent_lost: u64,
        controller_gained: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrainV1 {
    pub source: public::RootV1,
    pub original_paid_elves: Vec<IncarnationV72>,
    pub stage: DrainStageV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV1 {
    pub schema: String,
    pub viewer: PlayerSeatV1,
    pub meaning: MeaningV1,
    pub battlefield: Vec<CurrentV1>,
    pub casts: Vec<CastV1>,
    pub combat: Vec<CombatV1>,
    pub payment: Option<PaymentV1>,
    pub drains: Vec<DrainV1>,
}

fn ordinal(index: usize) -> Result<u32, String> {
    u32::try_from(index).map_err(|_| "Lathril native position overflow".into())
}

fn visible(
    state: &GameState,
    viewer: PlayerId,
    id: ObjectId,
) -> Result<public::ReferenceV1, String> {
    public::reference(state, id, Some(viewer)).ok_or_else(|| "Lathril visible card missing".into())
}

fn archived_root(
    state: &GameState,
    at: usize,
    viewer: PlayerId,
    source: AbilitySourceContractV4,
) -> Result<public::RootV1, String> {
    let mut root = public::archived_root(state, at, source, Some(viewer))?;
    root.original_source.identity = public::IdentityV1::Known { card_id: PRINTING };
    Ok(root)
}

fn combat(state: &GameState, viewer: PlayerId, token_id: u16) -> Result<Vec<CombatV1>, String> {
    let history = &state.engine.event_history;
    let held = public::held(state);
    let mut combat = vec![];
    for (index, event) in history.iter().enumerate() {
        let E::LathrilCombatDamageV1 { player, .. } = event else {
            continue;
        };
        let capture = u64::try_from(index).map_err(|_| "Lathril capture width")?;
        let (source, final_damage) = event::lathril_damage_v1(state, capture)?;
        if source.card_def != PRINTING {
            return Err("Lathril capture lost its selected exact printing".into());
        }
        let mut publications =
            history
                .iter()
                .enumerate()
                .filter_map(|(position, event)| match event {
                    E::LathrilCombatPublishedV1 {
                        capture_history_index,
                        item,
                    } if *capture_history_index == capture => Some((position, item.as_ref())),
                    _ => None,
                });
        let mut ends = history
            .iter()
            .enumerate()
            .filter_map(|(position, event)| match event {
                E::LathrilCombatResolvedV1 {
                    capture_history_index,
                } if *capture_history_index == capture => Some((position, true)),
                E::LathrilCombatCounteredV1 {
                    capture_history_index,
                } if *capture_history_index == capture => Some((position, false)),
                _ => None,
            });
        let mut pending = held.iter().enumerate().filter(|(_, trigger)| {
            trigger.effect
                == EffectOp::LathrilCombatTokensV1 {
                    capture_history_index: capture,
                }
        });
        let publication = publications.next();
        let end = ends.next();
        let waiting = pending.next();
        if publications.next().is_some() || ends.next().is_some() || pending.next().is_some() {
            return Err("Lathril duplicate combat occurrence".into());
        }
        let stage = match (publication, end, waiting) {
            (None, None, Some((position, trigger))) => {
                engine::validate_pending_trigger_identity(state, trigger)?;
                if trigger.source_contract != Some(source) {
                    return Err("Lathril original held source changed".into());
                }
                CombatStageV1::Held {
                    position: ordinal(position)?,
                }
            }
            (Some((published, original)), end, None) => {
                if published <= index
                    || event::validate_historical_lathril_trigger_v1(state, original)? != capture
                {
                    return Err("Lathril historical publication changed".into());
                }
                let mut live = state
                    .stack
                    .iter()
                    .enumerate()
                    .filter(|(_, row)| row.v4.stack_item_id == original.v4.stack_item_id);
                let current = live.next();
                if live.next().is_some()
                    || current.is_some_and(|(_, row)| row != original || end.is_some())
                    || end.is_some_and(|(ended, _)| ended <= published)
                {
                    return Err("Lathril live stack lifetime changed".into());
                }
                match (current, end) {
                    (Some((position, row)), None) => {
                        event::validate_published_lathril_v1(state, row)?;
                        CombatStageV1::Published {
                            stack_position: ordinal(position)?,
                        }
                    }
                    (None, Some((_, false))) => CombatStageV1::Countered,
                    (None, Some((position, true))) => {
                        let tokens = receipts::token_batch(
                            state,
                            capture,
                            position,
                            source.controller,
                            token_id,
                        )?
                        .into_iter()
                        .map(|object| IncarnationV72 {
                            object: object.0,
                            generation: 0,
                            owner: source.controller.into(),
                            controller: source.controller.into(),
                            zone: Zone::Battlefield,
                        })
                        .collect();
                        CombatStageV1::Resolved { tokens }
                    }
                    (None, None) => CombatStageV1::RemovedWithoutResolution,
                    (Some(_), Some(_)) => {
                        return Err("ended Lathril combat trigger still live".into())
                    }
                }
            }
            _ => {
                return Err(
                    "Lathril combat occurrence lacks exact held or published custody".into(),
                )
            }
        };
        combat.push(CombatV1 {
            source: archived_root(state, index, viewer, source)?,
            damaged_player: (*player).into(),
            final_damage,
            stage,
        });
    }
    Ok(combat)
}

fn choice_binding(
    state: &GameState,
    viewer: PlayerId,
    binding: &EffectObjectBinding,
) -> Result<public::ReferenceV1, String> {
    let card = visible(state, viewer, binding.object)?;
    if card.zone != Zone::Battlefield
        || card.zone != binding.expected_zone
        || card.generation != binding.expected_zone_change_count
        || card.controller != viewer.into()
    {
        return Err("Lathril native Elf candidate changed incarnation".into());
    }
    Ok(card)
}

fn payment(state: &GameState, viewer: PlayerId) -> Result<Option<PaymentV1>, String> {
    let Some(pending) = state.engine.pending_activation.as_ref().filter(|pending| {
        pending.controller == viewer
            && pending.ability_index == 0
            && public::visible_known(state, pending.source, Some(viewer), |id| id == PRINTING)
    }) else {
        return Ok(None);
    };
    engine::validate_pending_activation(state, pending)?;
    if pending.target_spec != TargetSpec::None || !pending.cauldron_grant.is_empty() {
        return Err("Lathril pending payment lost exact printed activation".into());
    }
    let chosen =
        u8::try_from(pending.object_cost_chosen.len()).map_err(|_| "Lathril Elf cost width")?;
    let remaining = ELVES
        .checked_sub(chosen)
        .filter(|left| *left > 0)
        .ok_or("Lathril pending Elf cost complete")?;
    let selected = pending
        .object_cost_chosen
        .iter()
        .map(|binding| choice_binding(state, viewer, binding))
        .collect::<Result<_, _>>()?;
    let candidates = engine::lathril_payment_candidates_v1(state, pending)
        .iter()
        .map(|binding| choice_binding(state, viewer, binding))
        .collect::<Result<Vec<_>, _>>()?;
    if candidates.len() < usize::from(remaining) {
        return Err("Lathril native chooser lost payable Elf roster".into());
    }
    Ok(Some(PaymentV1 {
        source: visible(state, viewer, pending.source)?,
        chooser: viewer.into(),
        selected,
        remaining,
        candidates,
    }))
}

fn require_live_drain_receipts(state: &GameState) -> Result<(), String> {
    for item in &state.stack {
        if receipts::selected(item) && !receipts::validate_live(state, item)? {
            return Err("Lathril live activation requires native paid receipt".into());
        }
    }
    Ok(())
}

pub(crate) fn validate_native_receipts(state: &GameState) -> Result<(), String> {
    require_live_drain_receipts(state)?;
    receipts::validate_combat_links(state)?;
    combat(state, PlayerId::P0, token_id()?)?;
    receipts::drains(state)?;
    Ok(())
}

pub(crate) fn capture(state: &GameState, viewer: PlayerId) -> Result<FactsV1, String> {
    let meaning = printed_meaning()?;
    let mut battlefield = vec![];
    for player in [PlayerId::P0, PlayerId::P1] {
        for &id in &state.players[player.index()].battlefield {
            if !public::visible_known(state, id, Some(viewer), |id| {
                id == PRINTING || id == meaning.token_id
            }) {
                continue;
            }
            let live = state.objects.get(id);
            battlefield.push(CurrentV1 {
                card: visible(state, viewer, id)?,
                token: live.v4.is_token,
                tapped: live.tapped,
                summoning_sick: live.summoning_sick,
                counters: crate::tinybones_public_v72::counters(state, id)?,
                form: public::checked_form(state, id)?,
            });
        }
    }
    let mut casts = vec![];
    for (position, item) in state.stack.iter().enumerate() {
        if item.kind != StackItemKind::Spell
            || !public::visible_known(state, item.source, Some(viewer), |id| id == PRINTING)
        {
            continue;
        }
        let target = engine::validated_stack_item_target_spec(item, state)?;
        let origin = item
            .v4
            .source_contract
            .ok_or("Lathril native spell source absent")?
            .spell_cast_origin;
        let pending = state
            .engine
            .pending_cast
            .as_ref()
            .filter(|pending| pending.spell == item.source);
        if let Some(pending) = pending {
            engine::validate_pending_cast(state, pending)?;
        }
        if target != pending.is_none().then_some(TargetSpec::None) {
            return Err("Lathril stack spell target shape changed".into());
        }
        casts.push(CastV1 {
            stack_position: ordinal(position)?,
            spell: visible(state, viewer, item.source)?,
            announcing: pending.is_some(),
            uncast_copy: item.is_copy,
            origin_zone: origin.map(|origin| origin.origin_zone),
            finalized_method: origin.and_then(|origin| origin.finalized_method),
        });
    }
    require_live_drain_receipts(state)?;
    receipts::validate_combat_links(state)?;
    let drains = receipts::drains(state)?
        .into_iter()
        .map(|(at, item, stage)| {
            let source = archived_root(
                state,
                at,
                viewer,
                item.v4
                    .ability_source_contract
                    .ok_or("Lathril paid source absent")?,
            )?;
            let original_paid_elves = item
                .v4
                .paid_cost_refs
                .iter()
                .map(|paid| IncarnationV72 {
                    object: paid.object.0,
                    generation: paid.zone_change_count,
                    owner: paid.owner.into(),
                    controller: paid.controller.into(),
                    zone: paid.zone,
                })
                .collect();
            let stage = match stage {
                DrainOutcomeV1::Live(position) => DrainStageV1::Published {
                    stack_position: ordinal(position)?,
                },
                DrainOutcomeV1::RemovedWithoutResolution => DrainStageV1::RemovedWithoutResolution,
                DrainOutcomeV1::Countered => DrainStageV1::Countered,
                DrainOutcomeV1::Resolved {
                    opponent_lost,
                    controller_gained,
                } => DrainStageV1::Resolved {
                    opponent_lost,
                    controller_gained,
                },
            };
            Ok(DrainV1 {
                source,
                original_paid_elves,
                stage,
            })
        })
        .collect::<Result<_, String>>()?;
    Ok(FactsV1 {
        schema: SCHEMA.into(),
        viewer: viewer.into(),
        combat: combat(state, viewer, meaning.token_id)?,
        payment: payment(state, viewer)?,
        meaning,
        battlefield,
        casts,
        drains,
    })
}
