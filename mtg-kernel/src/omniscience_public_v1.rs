//! Exact Omniscience printing and viewer-visible native hand-cast routes.
use crate::{
    card_def::{self, CardType},
    engine::{omniscience_cast_v1 as native, Decision},
    ids::{ObjectId, PlayerId},
    mana::{Cost, ManaColor, Pip},
    optional_trigger_public_v1 as public,
    policy_surface_v5::PolicyDecisionV5,
    rl::PlayerSeatV1,
    state::{AbilitySourceContractV4, CastMethodV4, GameState, StackItem, Zone},
    surface::SurfaceDecision,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PRINTING: u16 = 478;
pub const NAME: &str = "Omniscience";
pub const SCHEMA: &str = "mtg-kernel/omniscience-public/v1";
const MEANING_SCHEMA: &str = "rules-meaning/omniscience/v1";

pub(crate) fn selected_id() -> Result<u16, String> {
    if card_def::card_id_by_name(NAME) == Some(PRINTING)
        && native::definition(PRINTING)
        && crate::trigger::triggers_for(PRINTING).is_empty()
    {
        Ok(PRINTING)
    } else {
        Err("Omniscience meaning requires the exact pure Limited printing".into())
    }
}

pub fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [
            SCHEMA,
            MEANING_SCHEMA,
            include_str!("omniscience_public_v1.rs"),
            include_str!("engine/omniscience_cast_v1.rs"),
            include_str!("engine.rs"),
            include_str!("spell_costs_v1.rs"),
            include_str!("resolution_cast_v1.rs"),
            include_str!("optional_trigger_public_v1.rs"),
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
    pub mana_cost: ManaCostV1,
    pub mana_value: u16,
    pub types: Vec<CardType>,
    pub colors: Vec<ManaColor>,
    pub controller_only: bool,
    pub permission_zone: Zone,
    pub allowed_origin: Zone,
    pub optional_replace_base_mana: bool,
    pub retains_normal_timing_and_targets: bool,
    pub retains_additional_costs_and_cost_increases: bool,
    pub retains_mana_spending_restrictions: bool,
    pub free_x: u16,
}

pub fn printed_meaning() -> Result<MeaningV1, String> {
    let card_id = selected_id()?;
    let def = &card_def::CARD_DEFS[usize::from(card_id)];
    Ok(MeaningV1 {
        schema: MEANING_SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_db_hash: card_def::KERNEL_CARDDB_HASH,
        card_id,
        mana_cost: def.cost.into(),
        mana_value: def.mana_value,
        types: def.types.to_vec(),
        colors: def.colors.to_vec(),
        controller_only: true,
        permission_zone: Zone::Battlefield,
        allowed_origin: Zone::Hand,
        optional_replace_base_mana: true,
        retains_normal_timing_and_targets: true,
        retains_additional_costs_and_cost_increases: true,
        retains_mana_spending_restrictions: true,
        free_x: 0,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ManaPipV1 {
    Colored(ManaColor),
    Hybrid(ManaColor, ManaColor),
    Phyrexian(ManaColor),
    PhyrexianAnyColor(ManaColor),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManaCostV1 {
    pub generic: u8,
    pub pips: Vec<ManaPipV1>,
    pub x_count: u8,
}

impl From<Cost> for ManaCostV1 {
    fn from(cost: Cost) -> Self {
        Self {
            generic: cost.generic,
            pips: cost
                .pips
                .iter()
                .map(|pip| match *pip {
                    Pip::Colored(color) => ManaPipV1::Colored(color),
                    Pip::Hybrid(first, second) => ManaPipV1::Hybrid(first, second),
                    Pip::Phyrexian(color) => ManaPipV1::Phyrexian(color),
                    Pip::PhyrexianAnyColor(color) => ManaPipV1::PhyrexianAnyColor(color),
                })
                .collect(),
            x_count: cost.x_count,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RouteV1 {
    OrdinaryPayment,
    ReplaceBaseMana,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OfferV1 {
    pub card: public::ReferenceV1,
    pub permission: public::RootV1,
    pub ordinary_payment_available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaymentTermsV1 {
    pub cast_method: CastMethodV4,
    /// Selected native cost components before total cost adjustment and
    /// restricted mana solving. The ordinary public surface owns both pools.
    pub mana_components: Vec<ManaCostV1>,
    pub nonmana_component_count: u32,
    pub discard_required: u16,
}

impl TryFrom<native::PaymentTermsV1> for PaymentTermsV1 {
    type Error = String;

    fn try_from(terms: native::PaymentTermsV1) -> Result<Self, String> {
        Ok(Self {
            cast_method: terms.method,
            mana_components: terms.mana_components.into_iter().map(Into::into).collect(),
            nonmana_component_count: u32::try_from(terms.nonmana_component_count)
                .map_err(|_| "Omniscience cost component count overflow")?,
            discard_required: terms.discard_required,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StageV1 {
    Announcing,
    Published,
    Resolving,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CastV1 {
    pub spell: public::ReferenceV1,
    pub permission: public::RootV1,
    pub original_permission_active: bool,
    pub stack_position: Option<u32>,
    pub stage: StageV1,
    pub origin_zone: Zone,
    pub ordinary_payment_was_available: bool,
    pub route: Option<RouteV1>,
    pub x: Option<u16>,
    pub kicked: Option<bool>,
    pub optional_additional_cost_selected: Option<bool>,
    pub mandatory_mana_branch: Option<bool>,
    pub payment_terms_now: Option<PaymentTermsV1>,
    pub finalized_method: Option<CastMethodV4>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV1 {
    pub schema: String,
    pub viewer: PlayerSeatV1,
    pub meaning: MeaningV1,
    pub current_permissions: [Option<public::ReferenceV1>; 2],
    pub hand_offers: Vec<OfferV1>,
    pub casts: Vec<CastV1>,
}

fn card(
    state: &GameState,
    viewer: PlayerId,
    object: ObjectId,
) -> Result<public::ReferenceV1, String> {
    public::reference(state, object, Some(viewer))
        .ok_or_else(|| "Omniscience public card missing".into())
}

fn root(
    state: &GameState,
    viewer: PlayerId,
    source: AbilitySourceContractV4,
) -> Result<public::RootV1, String> {
    public::root(state, source, Some(viewer))
}

fn route(choice: native::ChoiceV1) -> Option<RouteV1> {
    choice.free.map(|free| {
        if free {
            RouteV1::ReplaceBaseMana
        } else {
            RouteV1::OrdinaryPayment
        }
    })
}

fn published(
    state: &GameState,
    viewer: PlayerId,
    item: &StackItem,
    position: Option<usize>,
) -> Result<Option<CastV1>, String> {
    let Some((choice, options)) = native::published_options(state, item)? else {
        return Ok(None);
    };
    let spell = card(state, viewer, item.source)?;
    if matches!(spell.identity, public::IdentityV1::FaceDown { .. }) && viewer != item.controller {
        // A paid face-down announcement must not reveal its hand permission.
        return Ok(None);
    }
    Ok(Some(CastV1 {
        spell,
        permission: public::archived_root(
            state,
            usize::try_from(choice.options_history_index).map_err(|_| "Omniscience option index width")?,
            options.source,
            Some(viewer),
        )?,
        original_permission_active: native::original_permission_active(state, options.source),
        stack_position: position
            .map(u32::try_from)
            .transpose()
            .map_err(|_| "Omniscience stack position overflow")?,
        stage: if position.is_some() {
            StageV1::Published
        } else {
            StageV1::Resolving
        },
        origin_zone: Zone::Hand,
        ordinary_payment_was_available: options.paid,
        route: route(choice),
        x: Some(item.v4.x_value),
        kicked: Some(item.kicked),
        optional_additional_cost_selected: Some(item.v4.optional_additional_cost_paid.is_some()),
        mandatory_mana_branch: item
            .v4
            .source_contract
            .and_then(|source| source.finalized_cast_binding)
            .and_then(|binding| binding.mandatory_additional_cost)
            .map(|cost| cost.pay_mana),
        payment_terms_now: None,
        finalized_method: item.v4.cast_method,
    }))
}

pub(crate) fn capture(
    state: &GameState,
    viewer: PlayerId,
    origin: &PolicyDecisionV5,
) -> Result<FactsV1, String> {
    let meaning = printed_meaning()?;
    let mut current_permissions = [None, None];
    for player in [PlayerId::P0, PlayerId::P1] {
        current_permissions[player.index()] = native::active_source(state, player)
            .map(|source| card(state, viewer, source.source))
            .transpose()?;
    }
    let mut hand_offers = vec![];
    if let PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::CastSpellOrPass {
        player,
        castable_spells,
        ..
    })) = origin
    {
        if *player == viewer && current_permissions[viewer.index()].is_some() {
            for &spell in castable_spells {
                if let Some((source, ordinary_payment_available)) =
                    native::quoted_offer(state, spell, viewer)
                {
                    hand_offers.push(OfferV1 {
                        card: card(state, viewer, spell)?,
                        permission: root(state, viewer, source)?,
                        ordinary_payment_available,
                    });
                }
            }
        }
    }
    let mut casts = vec![];
    let pending = state.engine.pending_cast.as_ref();
    if let Some(pending) = pending {
        if let Some((choice, options)) = native::pending_options(state, pending)? {
            if pending.controller == viewer {
                casts.push(CastV1 {
                    spell: card(state, viewer, pending.spell)?,
                    permission: root(state, viewer, options.source)?,
                    original_permission_active: native::original_permission_active(
                        state,
                        options.source,
                    ),
                    stack_position: Some(
                        u32::try_from(
                            state
                                .stack
                                .len()
                                .checked_sub(1)
                                .ok_or("Omniscience pending placeholder missing")?,
                        )
                        .map_err(|_| "Omniscience stack position overflow")?,
                    ),
                    stage: StageV1::Announcing,
                    origin_zone: Zone::Hand,
                    ordinary_payment_was_available: options.paid,
                    route: route(choice),
                    x: pending.selected_x_v59(),
                    kicked: pending.kicked,
                    optional_additional_cost_selected: pending.optional_additional_cost_paid,
                    mandatory_mana_branch: pending
                        .mandatory_additional_cost
                        .and_then(|cost| cost.pay_mana),
                    payment_terms_now: native::payment_terms(state, pending)
                        .map(TryInto::try_into)
                        .transpose()?,
                    finalized_method: None,
                });
            }
        }
    }
    for (position, item) in state.stack.iter().enumerate() {
        if pending.is_some_and(|pending| item.source == pending.spell) {
            continue;
        }
        if let Some(row) = published(state, viewer, item, Some(position))? {
            casts.push(row);
        }
    }
    if let Some(effect) = &state.engine.pending_effect {
        let item = &effect.resolving_item;
        if !state
            .stack
            .iter()
            .any(|row| row.v4.stack_item_id == item.v4.stack_item_id)
        {
            if let Some(row) = published(state, viewer, item, None)? {
                casts.push(row);
            }
        }
    }
    Ok(FactsV1 {
        schema: SCHEMA.into(),
        viewer: viewer.into(),
        meaning,
        current_permissions,
        hand_offers,
        casts,
    })
}
