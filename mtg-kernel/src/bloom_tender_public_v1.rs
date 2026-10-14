//! Public bundle descriptors attached to genuine, complete V8 input.
use crate::{
    card_def::{ManaAbilityAmountDef, ManaAbilityCostDef, CARD_DEFS},
    ids::{ObjectId, PlayerId},
    mana::ManaColor,
    policy_observation_v8::ObservationV8,
    rl::{ActionSemanticV1, CardStableRefV1, RlContractError},
    state::GameState,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BloomTenderManaBundleV1 {
    pub source: CardStableRefV1,
    pub colors: Vec<ManaColor>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BloomTenderObservationV1 {
    pub schema: String,
    pub producer_sha256: String,
    pub predecessor: ObservationV8,
    pub bundles: Vec<BloomTenderManaBundleV1>,
}
pub fn producer_sha256_v1() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut h = Sha256::new();
        for source in [
            include_str!("bloom_tender_public_v1.rs"),
            include_str!("bloom_tender_mana_v1.rs"),
            include_str!("engine.rs"),
            include_str!("event.rs"),
            include_str!("card_def.rs"),
            include_str!("state.rs"),
            include_str!("continuous_characteristics_v1.rs"),
            include_str!("standard_creatures_v1.rs"),
            include_str!("standard_cards_v1.rs"),
            include_str!("standard_legends_v1.rs"),
            include_str!("rl.rs"),
            include_str!("rl_session/human_v8.rs"),
            include_str!("rl_session/bloom_tender_v24.rs"),
            include_str!("rl_session/flat_action_v3.rs"),
            include_str!("rl_session/flat_action_v4.rs"),
            include_str!("policy_observation_v8.rs"),
            include_str!("../build.rs"),
        ] {
            h.update(source.as_bytes());
            h.update([0]);
        }
        for source in [
            include_str!("rules_meaning_bloom_tender_v1.rs"),
            include_str!("human_bo3_v1/bloom_tender_v24.rs"),
            include_str!("human_bo3_v1/life_ward_v8.rs"),
            include_str!("human_bo3_v1/visible.rs"),
            include_str!("human_bo3_v1/labels.rs"),
            include_str!("rl_session.rs"),
            include_str!("policy_observation_v6.rs"),
            include_str!("policy_observation_v7.rs"),
        ] {
            h.update(source.as_bytes());
            h.update([0]);
        }
        h.finalize().iter().map(|b| format!("{b:02x}")).collect()
    })
    .clone()
}
pub(crate) fn from_input(
    state: &GameState,
    predecessor: ObservationV8,
    actions: &[ActionSemanticV1],
) -> Result<BloomTenderObservationV1, RlContractError> {
    let mut bundles = Vec::new();
    for action in actions {
        let ActionSemanticV1::ActivateManaAbility {
            actor,
            source,
            mana_choice,
            cost_target,
        } = action
        else {
            continue;
        };
        let Some(def) = CARD_DEFS.get(source.card_db_id as usize) else {
            return Err(RlContractError("invalid mana definition".into()));
        };
        let Some(ability) = def.mana_ability_def else {
            continue;
        };
        if ability.amount != ManaAbilityAmountDef::EachControlledPermanentColor {
            continue;
        }
        if ability.cost != ManaAbilityCostDef::TapSelf
            || ability.controller_damage != 0
            || ability.max_activations_per_turn.is_some()
            || cost_target.is_some()
            || mana_choice.is_some_and(|c| c != ManaColor::G)
            || def.mana_ability_choices != [ManaColor::G]
            || *actor != predecessor.acting_player
        {
            return Err(RlContractError("unsupported bundle mana recipe".into()));
        }
        let id = ObjectId(source.arena_id);
        let player = if *actor == crate::rl::PlayerSeatV1::P0 {
            PlayerId::P0
        } else {
            PlayerId::P1
        };
        let projected = predecessor.projection.surface.battlefield[player.index()]
            .iter()
            .find(|card| card.stable == *source)
            .ok_or_else(|| {
                RlContractError("bundle source is not public controlled battlefield".into())
            })?;
        let live = state
            .objects
            .try_get(id)
            .ok_or_else(|| RlContractError("bundle source absent".into()))?;
        if live.zone_change_count != source.zone_change_count
            || live.card_def != source.card_db_id
            || crate::rl::PlayerSeatV1::from(live.owner) != source.owner
            || projected.face_index != live.v4.face_index
            || !crate::engine::available_mana_ability_choices(player, id, state)
                .contains(&ManaColor::G)
        {
            return Err(RlContractError(
                "bundle source incarnation or ability changed".into(),
            ));
        }
        let colors = crate::bloom_tender_mana_v1::colors(state, player);
        let mask = predecessor.projection.surface.battlefield[player.index()]
            .iter()
            .fold(0, |m, card| m | card.characteristics.effective_color_mask);
        let public_colors = [
            ManaColor::W,
            ManaColor::U,
            ManaColor::B,
            ManaColor::R,
            ManaColor::G,
        ]
        .into_iter()
        .filter(|color| mask & (1 << color.pool_index()) != 0)
        .collect::<Vec<_>>();
        if public_colors != colors {
            return Err(RlContractError(
                "bundle public colors disagree with actual output".into(),
            ));
        }
        if bundles
            .iter()
            .any(|row: &BloomTenderManaBundleV1| row.source == *source)
        {
            return Err(RlContractError("duplicate bundle action".into()));
        }
        bundles.push(BloomTenderManaBundleV1 {
            source: source.clone(),
            colors,
        });
    }
    Ok(BloomTenderObservationV1 {
        schema: "mtg-kernel-bloom-tender-observation/v1".into(),
        producer_sha256: producer_sha256_v1(),
        predecessor,
        bundles,
    })
}
