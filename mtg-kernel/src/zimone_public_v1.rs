//! Zimone's exact printed meaning and viewer-visible counter context.
use crate::{
    card_def::{
        self, ActivationTargetFilter, CardCapability, CardDef, CardType, CostComponent, Keywords,
        Subtype, Supertype, TargetSpec, CARD_DEFS,
    },
    effect::{EffectOp, ObjectRef},
    engine,
    ids::PlayerId,
    mana::{ManaColor, Pip},
    optional_trigger_public_v1 as public,
    rl::PlayerSeatV1,
    standard_cards_v1::{StandardOpV1, StandardTargetV1},
    state::{
        AbilitySourceContractV4, GameState, StackItem, StackItemKind, StackTargetContractV4,
        Target, Zone,
    },
    tinybones_public_v72::{self as counters, CountersV72, IncarnationV72},
    trigger::TriggerCondition,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PRINTING: u16 = 465;
pub const NAME: &str = "Zimone, Paradox Sculptor";
pub const SCHEMA: &str = "mtg-kernel/zimone-public/v1";
pub const MEANING_SCHEMA: &str = "rules-meaning/zimone-paradox-sculptor/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AbilityKindV1 {
    CombatPutPlusOne,
    ActivateDoubleEveryKind,
}

impl AbilityKindV1 {
    pub(crate) fn program(self) -> EffectOp {
        EffectOp::StandardV1(match self {
            Self::CombatPutPlusOne => StandardOpV1::PutPlusOneCountersOnTargets,
            Self::ActivateDoubleEveryKind => StandardOpV1::DoubleAllCountersOnTargets,
        })
    }

    fn target_spec(self) -> TargetSpec {
        TargetSpec::StandardV1(match self {
            Self::CombatPutPlusOne => StandardTargetV1::UpToTwoControlledCreatures,
            Self::ActivateDoubleEveryKind => {
                StandardTargetV1::UpToTwoControlledArtifactsOrCreatures
            }
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV1 {
    pub schema: String,
    pub producer_sha256: String,
    pub card_db_hash: u64,
    pub card_id: u16,
    pub generic_mana: u8,
    pub colored_mana: [ManaColor; 2],
    pub mana_value: u16,
    pub types: Vec<CardType>,
    pub subtypes: Vec<Subtype>,
    pub supertypes: Vec<public::SupertypeV1>,
    pub colors: Vec<ManaColor>,
    pub power: i16,
    pub toughness: i16,
    pub ordered_rules: [RuleV1; 2],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum RuleV1 {
    BeginningOfControllerCombat {
        max_distinct_controlled_creatures: u8,
        min_targets: u8,
        plus_one_counters_each: u8,
    },
    Activate {
        colored_mana: [ManaColor; 2],
        tap_source: bool,
        max_distinct_controlled_artifacts_or_creatures: u8,
        min_targets: u8,
        doubles_each_counter_kind_at_resolution: bool,
    },
}

pub fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        let native = crate::rules_meaning_embercleave_v65::producer_sha256();
        for source in [
            SCHEMA,
            MEANING_SCHEMA,
            native.as_str(),
            include_str!("zimone_public_v1.rs"),
            include_str!("zimone_paradox_sculptor_v1.rs"),
            include_str!("counter_target_effects_v1.rs"),
            include_str!("tinybones_public_v72.rs"),
            include_str!("rules_meaning_graveyard_return_v61.rs"),
            include_str!("../build.rs"),
        ] {
            hash.update((source.len() as u64).to_le_bytes());
            hash.update(source.as_bytes());
        }
        format!("{:x}", hash.finalize())
    })
    .clone()
}

fn selected_definition() -> Result<&'static CardDef, String> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) || card_def::card_id_by_name(NAME) != Some(PRINTING)
    {
        return Err("Zimone meaning requires the exact pure Limited printing".into());
    }
    let d = &CARD_DEFS[usize::from(PRINTING)];
    let colors = [ManaColor::G, ManaColor::U];
    let pips = colors.map(Pip::Colored);
    if !crate::rules_meaning_graveyard_return_v61::ordinary_foreign_fields_except_activated_absent(
        d,
    ) || d.name != NAME
        || d.capability != CardCapability::Partial
        || d.cost.generic != 2
        || d.cost.x_count != 0
        || d.cost.pips != pips
        || d.mana_value != 4
        || d.types != [CardType::Creature]
        || d.subtypes != [Subtype::Human, Subtype::Wizard]
        || d.supertypes != [Supertype::Legendary]
        || d.colors != colors
        || d.power != Some(1)
        || d.toughness != Some(4)
        || d.keywords != Keywords::NONE
        || d.is_token
        || d.target_spec != TargetSpec::None
        || (d.spell_effect)()
            != Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
    {
        return Err("complete Zimone printing changed".into());
    }
    let [a] = d.activated_abilities else {
        return Err("Zimone activation roster changed".into());
    };
    let activation = AbilityKindV1::ActivateDoubleEveryKind;
    if !matches!(a.cost, [CostComponent::Mana(c), CostComponent::Tap]
        if c.generic == 0 && c.x_count == 0 && c.pips == pips)
        || a.target_spec != activation.target_spec()
        || (a.effect)() != activation.program()
        || a.activation_zone != Zone::Battlefield
        || a.sorcery_speed_only
        || a.activation_target_filter != ActivationTargetFilter::TargetSpecOnly
        || a.max_activations_per_turn.is_some()
        || a.face.is_some()
    {
        return Err("Zimone paid activation changed".into());
    }
    let [t] = crate::trigger::triggers_for(PRINTING) else {
        return Err("Zimone combat trigger roster changed".into());
    };
    let trigger = AbilityKindV1::CombatPutPlusOne;
    if t.condition != TriggerCondition::BeginningOfControllerCombat
        || t.home_zone != Zone::Battlefield
        || t.intervening_if_kicked
        || t.intervening_if_controls_another_source_card
        || t.face_index != 0
        || (t.effect)() != trigger.program()
        || crate::trigger::target_spec_for_trigger(PRINTING, &(t.effect)())
            != Some(trigger.target_spec())
    {
        return Err("Zimone native combat trigger changed".into());
    }
    Ok(d)
}

pub(crate) fn selected_id() -> Result<u16, String> {
    selected_definition().map(|_| PRINTING)
}

pub fn printed_meaning() -> Result<MeaningV1, String> {
    let d = selected_definition()?;
    let colors = [ManaColor::G, ManaColor::U];
    Ok(MeaningV1 {
        schema: MEANING_SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_db_hash: card_def::KERNEL_CARDDB_HASH,
        card_id: PRINTING,
        generic_mana: 2,
        colored_mana: colors,
        mana_value: 4,
        types: d.types.to_vec(),
        subtypes: d.subtypes.to_vec(),
        supertypes: vec![public::SupertypeV1::Legendary],
        colors: d.colors.to_vec(),
        power: 1,
        toughness: 4,
        ordered_rules: [
            RuleV1::BeginningOfControllerCombat {
                max_distinct_controlled_creatures: 2,
                min_targets: 0,
                plus_one_counters_each: 1,
            },
            RuleV1::Activate {
                colored_mana: colors,
                tap_source: true,
                max_distinct_controlled_artifacts_or_creatures: 2,
                min_targets: 0,
                doubles_each_counter_kind_at_resolution: true,
            },
        ],
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermanentV1 {
    pub card: public::ReferenceV1,
    pub form: public::FormV1,
    pub tapped: bool,
    pub summoning_sick: bool,
    pub counters: CountersV72,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetV1 {
    pub original: IncarnationV72,
    pub current_original: Option<public::ReferenceV1>,
    pub currently_legal: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StageV1 {
    HeldCombat,
    OrderedCombat,
    PendingActivation,
    Published,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbilityV1 {
    pub kind: AbilityKindV1,
    pub stage: StageV1,
    /// Index in the unfiltered native stage, separate from a menu action index.
    pub position: u32,
    pub source: public::RootV1,
    /// Original declaration order is preserved even after targets become illegal.
    pub original_targets: Vec<TargetV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV1 {
    pub schema: String,
    pub viewer: PlayerSeatV1,
    pub meaning: MeaningV1,
    pub visible_printings: Vec<public::ReferenceV1>,
    pub battlefield: [Vec<PermanentV1>; 2],
    pub abilities: Vec<AbilityV1>,
}

#[allow(clippy::too_many_arguments)]
fn project_ability(
    state: &GameState,
    viewer: PlayerId,
    source: AbilitySourceContractV4,
    kind: AbilityKindV1,
    stage: StageV1,
    position: usize,
    targets: &[Target],
    contracts: &[StackTargetContractV4],
    published: Option<&StackItem>,
) -> Result<AbilityV1, String> {
    if targets.len() != contracts.len() || targets.len() > 2 {
        return Err("Zimone target vectors changed".into());
    }
    let spec = kind.target_spec();
    let mut original_targets = Vec::with_capacity(targets.len());
    for (index, (&target, &contract)) in targets.iter().zip(contracts).enumerate() {
        let StackTargetContractV4::Object {
            object,
            owner,
            controller,
            zone,
            zone_change_count,
            ..
        } = contract
        else {
            return Err("Zimone target was not a permanent".into());
        };
        let structurally_valid = crate::state::stack_target_contract_is_structurally_valid(
            state, spec, index, target, contract,
        ) || published.is_some_and(|item| {
            crate::state::stack_target_contract_for_sampled_snapshot(
                state, item, spec, index, target, contract,
            )
        });
        if targets[..index].contains(&target)
            || zone != Zone::Battlefield
            || controller != source.controller
            || !structurally_valid
        {
            return Err("Zimone original target contract changed".into());
        }
        original_targets.push(TargetV1 {
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
                .filter(|o| o.zone == zone && o.zone_change_count == zone_change_count)
                .and_then(|_| public::reference(state, object, Some(viewer))),
            currently_legal: engine::target_contract_matches_live(state, target, contract)
                && engine::effect_target_is_legal_from_ability_source(
                    state,
                    source,
                    source.controller,
                    spec,
                    targets,
                    index,
                ),
        });
    }
    Ok(AbilityV1 {
        kind,
        stage,
        position: u32::try_from(position).map_err(|_| "Zimone native stage position overflow")?,
        source: public::root(state, source, Some(viewer))?,
        original_targets,
    })
}

pub fn capture(state: &GameState, viewer: PlayerId) -> Result<FactsV1, String> {
    capture_scoped(state, viewer, public::ConsumerV1::Legacy)
}

pub(crate) fn capture_scoped(
    state: &GameState,
    viewer: PlayerId,
    consumer: public::ConsumerV1,
) -> Result<FactsV1, String> {
    let meaning = printed_meaning()?;
    if state.engine.halted.is_some() {
        return Err("Zimone native state halted".into());
    }
    public::validate_state_for_consumer(state, true, consumer)?;

    let visible_printings = state
        .objects
        .iter()
        .filter_map(|(id, o)| {
            if o.zone == Zone::Library || (o.zone == Zone::Hand && o.owner != viewer) {
                return None;
            }
            public::reference(state, id, Some(viewer))
                .filter(|card| card.identity == (public::IdentityV1::Known { card_id: PRINTING }))
        })
        .collect();

    let mut battlefield = [vec![], vec![]];
    for player in [PlayerId::P0, PlayerId::P1] {
        for &id in &state.players[player.index()].battlefield {
            let o = state
                .objects
                .try_get(id)
                .ok_or("Zimone battlefield member missing")?;
            if o.zone != Zone::Battlefield || o.controller != player {
                return Err("Zimone battlefield membership changed".into());
            }
            battlefield[player.index()].push(PermanentV1 {
                card: public::reference(state, id, Some(viewer))
                    .ok_or("Zimone battlefield member not visible")?,
                form: public::checked_form(state, id)?,
                tapped: o.tapped,
                summoning_sick: o.summoning_sick,
                counters: counters::counters(state, id)?,
            });
        }
    }

    let combat = AbilityKindV1::CombatPutPlusOne;
    let activation = AbilityKindV1::ActivateDoubleEveryKind;
    let mut abilities = vec![];
    for (position, pending) in public::held(state).into_iter().enumerate() {
        let Some(source) = pending
            .source_contract
            .filter(|source| source.card_def == PRINTING)
        else {
            continue;
        };
        engine::validate_pending_trigger_identity(state, pending)?;
        if pending.placement_ordered {
            engine::validate_pending_trigger(state, pending)?;
        }
        if pending.effect != combat.program()
            || pending.target_spec != combat.target_spec()
            || pending.granted_by.is_some()
        {
            return Err("unsupported held Zimone ability".into());
        }
        abilities.push(project_ability(
            state,
            viewer,
            source,
            combat,
            if pending.placement_ordered {
                StageV1::OrderedCombat
            } else {
                StageV1::HeldCombat
            },
            position,
            &pending.targets,
            &pending.target_contracts,
            None,
        )?);
    }
    if let Some(pending) = &state.engine.pending_activation {
        engine::validate_pending_activation(state, pending)?;
        if public::visible_known(state, pending.source, Some(viewer), |id| id == PRINTING) {
            if pending.ability_index != 0
                || pending.target_spec != activation.target_spec()
                || !pending.cauldron_grant.is_empty()
            {
                return Err("unsupported pending Zimone activation".into());
            }
            abilities.push(project_ability(
                state,
                viewer,
                AbilitySourceContractV4::capture(state, pending.source),
                activation,
                StageV1::PendingActivation,
                0,
                &pending.targets_chosen,
                &pending.target_contracts,
                None,
            )?);
        }
    }
    for (position, item) in state.stack.iter().enumerate() {
        let Some(source) = item
            .v4
            .ability_source_contract
            .filter(|source| source.card_def == PRINTING)
        else {
            continue;
        };
        let kind = match item.kind {
            StackItemKind::TriggeredAbility => combat,
            StackItemKind::ActivatedAbility if item.v4.activated_ability_index == Some(0) => {
                activation
            }
            _ => return Err("unsupported published Zimone ability kind".into()),
        };
        if item.inline_effect.as_ref() != Some(&kind.program())
            || item.v4.granted_by.is_some()
            || !item.v4.cauldron_grant.is_empty()
            || engine::validated_stack_item_target_spec(item, state)? != Some(kind.target_spec())
        {
            return Err("unsupported published Zimone ability".into());
        }
        abilities.push(project_ability(
            state,
            viewer,
            source,
            kind,
            StageV1::Published,
            position,
            &item.targets,
            &item.v4.target_contracts,
            Some(item),
        )?);
    }
    if state.engine.pending_effect.as_ref().is_some_and(|pending| {
        pending
            .ctx
            .ability_source_contract
            .is_some_and(|source| source.card_def == PRINTING)
    }) {
        return Err("Zimone atomic counter placement cannot resume externally".into());
    }
    Ok(FactsV1 {
        schema: SCHEMA.into(),
        viewer: viewer.into(),
        meaning,
        visible_printings,
        battlefield,
        abilities,
    })
}

pub fn validate(state: &GameState, viewer: PlayerId, value: &FactsV1) -> Result<(), String> {
    if *value == capture(state, viewer)? {
        Ok(())
    } else {
        Err("Zimone facts differ from actual viewer-visible state".into())
    }
}
