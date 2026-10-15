//! Exact V51 battlefield values are mandatory on every safe Human row.
use super::*;
use crate::rl::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanPublicCardV51 {
    /// The inherited fields retain their shape; these exact fields supersede
    /// its narrow damage and +1/+1 placeholders, including when zero.
    pub card: HumanPublicCardV1,
    pub marked_damage: u32,
    pub plus_one_counters: i32,
    pub soul: i32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanPublicProjectionV51 {
    pub turn: u32,
    pub phase: ZoneIndependentStepV1,
    pub active_player: PlayerSeatV1,
    pub priority_player: PlayerSeatV1,
    pub initiative: Option<PlayerSeatV1>,
    pub life_totals: [i32; 2],
    pub mana_pools: [[u8; 6]; 2],
    pub hand_counts: [usize; 2],
    pub library_counts: [usize; 2],
    pub player_status: [PlayerStatusV1; 2],
    pub battlefield: [Vec<HumanPublicCardV51>; 2],
    pub graveyards: [Vec<HumanPublicCardV1>; 2],
    pub exile: Vec<HumanPublicCardV1>,
    pub stack: Vec<HumanStackItemV1>,
    pub combat: HumanCombatV1,
    pub continuous_effects: Vec<HumanContinuousEffectV1>,
    pub object_relations: Vec<HumanObjectRelationV1>,
    pub exile_play_permissions: Vec<HumanExilePermissionV1>,
    pub engine_context: HumanEngineContextV1,
    pub surface_context: HumanSurfaceContextV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanVisibleStateRavenousV51 {
    pub public: HumanPublicProjectionV51,
    pub own_hand: Vec<HumanPrivateCardV1>,
    pub known_library_cards: [Vec<HumanKnownLibraryCardV1>; 2],
    pub known_hand_cards: [Vec<HumanPrivateCardV1>; 2],
    pub extensions: HumanRavenousExtensionsV51,
    pub policy_context: HumanPolicyContextV1,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub known_face_down_cards: Vec<HumanFaceDownKnowledgeV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poison_counters: Option<[u16; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poison_prevention: Option<[bool; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creatures_attacked_this_turn: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ninja_emblems: Option<[u16; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restricted_mana: Option<[Vec<HumanRestrictedManaV1>; 2]>,
}

pub(super) fn unary(
    observation: &RavenousObservationV51,
    handles: &Handles,
    traits: &mut BTreeMap<String, Vec<String>>,
) -> Result<(), Error> {
    for row in observation.battlefield.iter().flatten() {
        traits
            .entry(handles.get(&row.card.stable)?.handle.clone())
            .or_default()
            .push(format!(
                "ravenous_exact_{}_{}_{}",
                row.marked_damage, row.plus_one_counters, row.soul
            ));
    }
    Ok(())
}

pub(super) fn project(
    state: HumanVisibleStateV1<HumanRavenousExtensionsV51>,
    observation: &RavenousObservationV51,
    handles: &Handles,
) -> Result<HumanVisibleStateRavenousV51, Error> {
    let battlefield = state.public.battlefield.each_ref().map(|cards| {
        cards
            .iter()
            .map(|card| {
                let mut rows = observation.battlefield.iter().flatten().filter(|row| {
                    handles
                        .get(&row.card.stable)
                        .is_ok_and(|reference| reference == &card.stable)
                });
                let row = rows.next().ok_or(Error::InvalidVisibleReference)?;
                if rows.next().is_some() {
                    return Err(Error::InvalidVisibleReference);
                }
                Ok(HumanPublicCardV51 {
                    card: card.clone(),
                    marked_damage: row.marked_damage,
                    plus_one_counters: row.plus_one_counters,
                    soul: row.soul,
                })
            })
            .collect::<Result<Vec<_>, Error>>()
    });
    let [left, right] = battlefield;
    let battlefield = [left?, right?];
    let public = HumanPublicProjectionV51 {
        turn: state.public.turn,
        phase: state.public.phase,
        active_player: state.public.active_player,
        priority_player: state.public.priority_player,
        initiative: state.public.initiative,
        life_totals: state.public.life_totals,
        mana_pools: state.public.mana_pools,
        hand_counts: state.public.hand_counts,
        library_counts: state.public.library_counts,
        player_status: state.public.player_status,
        battlefield,
        graveyards: state.public.graveyards,
        exile: state.public.exile,
        stack: state.public.stack,
        combat: state.public.combat,
        continuous_effects: state.public.continuous_effects,
        object_relations: state.public.object_relations,
        exile_play_permissions: state.public.exile_play_permissions,
        engine_context: state.public.engine_context,
        surface_context: state.public.surface_context,
    };
    Ok(HumanVisibleStateRavenousV51 {
        public,
        own_hand: state.own_hand,
        known_library_cards: state.known_library_cards,
        known_hand_cards: state.known_hand_cards,
        extensions: state.extensions,
        policy_context: state.policy_context,
        known_face_down_cards: state.known_face_down_cards,
        poison_counters: state.poison_counters,
        poison_prevention: state.poison_prevention,
        creatures_attacked_this_turn: state.creatures_attacked_this_turn,
        ninja_emblems: state.ninja_emblems,
        restricted_mana: state.restricted_mana,
    })
}
