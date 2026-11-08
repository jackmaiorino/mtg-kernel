//! Exact Fiend Artisan meaning and actor-visible native activation/search facts.
use crate::{
    card_def::{self, CardType, Subtype},
    effect::{fiend_artisan_v1 as native, EffectTargetCandidate, PendingEffectChoice},
    engine::{self, Decision, PendingActivation},
    fiend_artisan_receipts_v1 as receipts,
    ids::{ObjectId, PlayerId},
    mana::ManaColor,
    optional_trigger_public_v1::{self as public, IdentityV1, ReferenceV1, RootV1},
    policy_surface_v5::PolicyDecisionV5,
    rl::PlayerSeatV1,
    state::{AbilitySourceContractV4, GameState, Target, Zone},
    surface_v2::SurfaceDecision,
    tinybones_public_v72::CurrentV72,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PRINTING: u16 = 483;
pub const NAME: &str = "Fiend Artisan";
pub const SCHEMA: &str = "mtg-kernel/fiend-artisan-public/v1";

pub(crate) fn selected_id() -> Result<u16, String> {
    if card_def::card_id_by_name(NAME) != Some(PRINTING)
        || !native::definition(PRINTING)
        || !crate::trigger::triggers_for(PRINTING).is_empty()
    {
        return Err("Fiend Artisan requires its exact pure Limited printing".into());
    }
    Ok(PRINTING)
}

pub(crate) fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [
            SCHEMA,
            include_str!("fiend_artisan_public_v1.rs"),
            include_str!("fiend_artisan_receipts_v1.rs"),
            include_str!("effect.rs"),
            include_str!("engine.rs"),
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
    pub hybrid_colors: [ManaColor; 2],
    pub printed_hybrid_pips: u8,
    pub printed_power: i16,
    pub printed_toughness: i16,
    pub graveyard_creatures_boost_both_by_one: bool,
    pub activate_only_as_sorcery: bool,
    pub activation_hybrid_pips: u8,
    pub activation_mana_x: bool,
    pub activation_tap: bool,
    pub activation_sacrifice_other_creature: bool,
    pub search_up_to_one_creature_at_most_paid_x: bool,
    pub searched_card_to_battlefield: bool,
    pub shuffle_even_if_no_card_found: bool,
}

pub fn printed_meaning() -> Result<MeaningV1, String> {
    let card_id = selected_id()?;
    Ok(MeaningV1 {
        schema: "rules-meaning/fiend-artisan/v1".into(),
        producer_sha256: producer_sha256(),
        card_db_hash: card_def::KERNEL_CARDDB_HASH,
        card_id,
        card_type: CardType::Creature,
        subtype: Subtype::Nightmare,
        hybrid_colors: [ManaColor::B, ManaColor::G],
        printed_hybrid_pips: 2,
        printed_power: 1,
        printed_toughness: 1,
        graveyard_creatures_boost_both_by_one: true,
        activate_only_as_sorcery: true,
        activation_hybrid_pips: 1,
        activation_mana_x: true,
        activation_tap: true,
        activation_sacrifice_other_creature: true,
        search_up_to_one_creature_at_most_paid_x: true,
        searched_card_to_battlefield: true,
        shuffle_even_if_no_card_found: true,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermanentV1 {
    pub current: CurrentV72,
    pub graveyard_creature_bonus: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationV1 {
    pub source: RootV1,
    pub stack_position: Option<u32>,
    pub paid_x: u8,
    pub original_sacrifice: ReferenceV1,
    pub searching: bool,
    pub shuffled: bool,
    pub countered: bool,
    pub actual_public_arrival: Option<ReferenceV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ChoiceV1 {
    PayX { source: ReferenceV1, maximum: u8 },
    Sacrifice { source: ReferenceV1, x: u8, legal: Vec<ReferenceV1> },
    Search {
        source: ReferenceV1,
        paid_x: u8,
        selected: Vec<ReferenceV1>,
        legal: Vec<ReferenceV1>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV1 {
    pub schema: String,
    pub viewer: PlayerSeatV1,
    pub meaning: MeaningV1,
    pub battlefield: Vec<PermanentV1>,
    pub activations: Vec<ActivationV1>,
    pub chooser: Option<ChoiceV1>,
}

pub(crate) fn pending_activation_selected(state: &GameState, pending: &PendingActivation) -> bool {
    pending.ability_index == 0 && pending.cauldron_grant.0.is_none()
        && state.objects.try_get(pending.source).is_some_and(|o|
            o.v4.face_down_v1.is_none() && native::definition(o.card_def))
}

fn battlefield_ref(state: &GameState, id: ObjectId) -> Result<ReferenceV1, String> {
    public::reference(state, id, None).filter(|card| card.zone == Zone::Battlefield)
        .ok_or_else(|| "Fiend Artisan original battlefield reference absent".into())
}

fn private_search_ref(
    state: &GameState,
    chooser: PlayerId,
    candidate: &EffectTargetCandidate,
) -> Result<ReferenceV1, String> {
    let (Target::Object(id), Some(binding)) = (candidate.target, candidate.expected_object)
    else { return Err("Fiend Artisan search candidate changed physical object".into()) };
    let live = state.objects.try_get(id).ok_or("Fiend Artisan search candidate absent")?;
    if binding.object != id || binding.expected_zone != Zone::Library
        || live.zone != Zone::Library || live.owner != chooser
        || live.zone_change_count != binding.expected_zone_change_count
        || live.v4.face_down_v1.is_some()
    {
        return Err("Fiend Artisan search candidate lost original private incarnation".into());
    }
    Ok(ReferenceV1 {
        object: id.0, generation: live.zone_change_count, owner: chooser.into(),
        controller: live.controller.into(), zone: Zone::Library,
        identity: IdentityV1::Known { card_id: live.card_def },
    })
}

fn paid_root(state: &GameState, source: AbilitySourceContractV4) -> Result<RootV1, String> {
    let mut root = public::root(state, source, None)?;
    if source.card_def != PRINTING {
        return Err("Fiend Artisan paid row changed exact printing".into());
    }
    // The activating incarnation was face up at payment. Its present face
    // belongs only to the separate current reference.
    root.original_source.identity = IdentityV1::Known { card_id: PRINTING };
    Ok(root)
}

fn choose(
    state: &GameState,
    viewer: PlayerId,
    origin: &PolicyDecisionV5,
) -> Result<Option<ChoiceV1>, String> {
    if origin.actor(state) != Some(viewer) {
        return Ok(None);
    }
    if let Some(pending) = state.engine.pending_activation.as_ref().filter(|pending|
        pending.controller == viewer && pending_activation_selected(state, pending))
    {
        engine::validate_pending_activation(state, pending)?;
        let source = battlefield_ref(state, pending.source)?;
        return match origin {
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(
                Decision::ChooseEffectOption { source: id, option_count, .. },
            )) if *id == pending.source && pending.mana_x_v1.is_none() =>
                Ok(Some(ChoiceV1::PayX {
                    source,
                    maximum: option_count.checked_sub(1).and_then(|n| u8::try_from(n).ok())
                        .ok_or("Fiend Artisan quoted X exceeds native range")?,
                })),
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(
                Decision::ChooseCostTargets { source: id, candidates, remaining: 1, .. },
            )) if *id == pending.source =>
                Ok(Some(ChoiceV1::Sacrifice {
                    source,
                    x: pending.mana_x_v1.ok_or("Fiend Artisan sacrifice has no selected X")?,
                    legal: candidates.iter().map(|&id| battlefield_ref(state, id))
                        .collect::<Result<_, _>>()?,
                })),
            _ => Err("Fiend Artisan activation changed native menu".into()),
        };
    }
    let Some(pending) = state.engine.pending_effect.as_ref().filter(|pending|
        pending.ctx.controller == viewer && native::selected(state, &pending.resolving_item))
    else { return Ok(None) };
    let Some(PendingEffectChoice::SelectTargets { legal, selected, .. }) = &pending.choice
    else { return Ok(None) };
    if !matches!(origin, PolicyDecisionV5::Surface(SurfaceDecision::Decision(
        Decision::ChooseEffectTargets { source, .. }
    )) if *source == pending.resolving_item.source)
    {
        return Err("Fiend Artisan search changed native choice origin".into());
    }
    let item = &pending.resolving_item;
    let source = item.v4.ability_source_contract.ok_or("Fiend Artisan search source absent")?;
    let mut legal = legal.iter().map(|candidate| private_search_ref(state, viewer, candidate))
        .collect::<Result<Vec<_>, _>>()?;
    legal.sort_by_key(|card| {
        let IdentityV1::Known { card_id } = card.identity else { unreachable!() };
        (card_id, card.owner as u8, card.controller as u8, card.object)
    });
    let selected = selected.iter().map(|candidate| private_search_ref(state, viewer, candidate))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(ChoiceV1::Search {
        source: paid_root(state, source)?.original_source,
        paid_x: native::paid_x(state, item)?,
        selected,
        legal,
    }))
}

pub(crate) fn capture(
    state: &GameState,
    viewer: PlayerId,
    origin: &PolicyDecisionV5,
) -> Result<FactsV1, String> {
    let meaning = printed_meaning()?;
    let mut battlefield = Vec::new();
    for (id, live) in state.objects.iter() {
        if live.zone != Zone::Battlefield || live.v4.face_down_v1.is_some()
            || live.card_def != PRINTING
        {
            continue;
        }
        let source = AbilitySourceContractV4::capture(state, id);
        engine::validate_historical_ability_source_contract(state, source)?;
        battlefield.push(PermanentV1 {
            current: CurrentV72 {
                card: battlefield_ref(state, id)?,
                is_token: live.v4.is_token,
                tapped: live.tapped,
                summoning_sick: live.summoning_sick,
                marked_damage: live.damage,
                counters: crate::tinybones_public_v72::counters(state, id)?,
                form: public::checked_form(state, id)?,
            },
            graveyard_creature_bonus: u32::try_from(native::bonus(state, id)?)
                .map_err(|_| "Fiend Artisan graveyard bonus exceeds object index width")?,
        });
    }
    let activations = receipts::activations(state)?.into_iter().map(|row| {
        let source = row.item.v4.ability_source_contract.ok_or("Fiend Artisan paid source absent")?;
        Ok(ActivationV1 {
            source: paid_root(state, source)?,
            stack_position: row.stack_position,
            paid_x: row.x,
            original_sacrifice: row.sacrifice.clone(),
            searching: row.searching,
            shuffled: row.shuffled,
            countered: row.countered,
            actual_public_arrival: row.arrival.cloned(),
        })
    }).collect::<Result<Vec<_>, String>>()?;
    Ok(FactsV1 {
        schema: SCHEMA.into(), viewer: viewer.into(), meaning, battlefield, activations,
        chooser: choose(state, viewer, origin)?,
    })
}
