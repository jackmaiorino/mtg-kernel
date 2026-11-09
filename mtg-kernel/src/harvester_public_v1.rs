//! Harvester's exact printed ability, public eligibility and native copy outcomes.
use crate::{
    card_def::{self, CardType, Subtype},
    effect::harvester_v1 as native,
    engine::{self, Decision},
    harvester_receipts_v1 as receipts,
    ids::{ObjectId, PlayerId},
    optional_trigger_public_v1::{self as public, FormV1, IdentityV1, ReferenceV1, RootV1},
    policy_surface_v5::PolicyDecisionV5,
    rl::PlayerSeatV1,
    state::{AbilitySourceContractV4, GameState, StackTargetContractV4, Target, Zone},
    surface_v2::SurfaceDecision,
    tinybones_public_v72::{CountersV72, IncarnationV72},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PRINTING: u16 = 479;
pub const NAME: &str = "Abyssal Harvester";
pub const SCHEMA: &str = "mtg-kernel/harvester-public/v1";
const MEANING_SCHEMA: &str = "rules-meaning/abyssal-harvester/v1";

pub(crate) fn selected_id() -> Result<u16, String> {
    if card_def::card_id_by_name(NAME) != Some(PRINTING)
        || !native::definition(PRINTING)
        || !crate::trigger::triggers_for(PRINTING).is_empty()
    {
        return Err("Harvester requires its exact pure Limited printing and ability".into());
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
            include_str!("harvester_public_v1.rs"),
            include_str!("harvester_receipts_v1.rs"),
            include_str!("effect/harvester_v1.rs"),
            include_str!("event.rs"),
            include_str!("engine.rs"),
            include_str!("state.rs"),
            include_str!("standard_cards_v1.rs"),
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
    pub subtypes: [Subtype; 2],
    pub printed_generic: u8,
    pub printed_black: u8,
    pub mana_value: u16,
    pub power: i16,
    pub toughness: i16,
    pub tap_cost: bool,
    pub target_creature_card_in_any_graveyard_this_turn: bool,
    pub exile_target_then_copy_with_nightmare: bool,
    pub exile_other_controlled_nightmare_tokens: bool,
}

pub fn printed_meaning() -> Result<MeaningV1, String> {
    Ok(MeaningV1 {
        schema: MEANING_SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_db_hash: card_def::KERNEL_CARDDB_HASH,
        card_id: selected_id()?,
        card_type: CardType::Creature,
        subtypes: [Subtype::Demon, Subtype::Warlock],
        printed_generic: 1,
        printed_black: 2,
        mana_value: 3,
        power: 3,
        toughness: 2,
        tap_cost: true,
        target_creature_card_in_any_graveyard_this_turn: true,
        exile_target_then_copy_with_nightmare: true,
        exile_other_controlled_nightmare_tokens: true,
    })
}

pub use crate::harvester_receipts_v1::OutcomeV1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetChoiceV1 {
    pub source: ReferenceV1,
    pub legal: Vec<ReferenceV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenV1 {
    pub card: ReferenceV1,
    pub form: FormV1,
    pub counters: CountersV72,
    pub tapped: bool,
    pub summoning_sick: bool,
    pub copiable_nightmare: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BirthV1 {
    pub original: IncarnationV72,
    pub copied_printing: u16,
    pub added_copiable_nightmare: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MovementV1 {
    pub original: IncarnationV72,
    pub identity: IdentityV1,
    pub actual_destination: Zone,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationV1 {
    pub source: RootV1,
    pub stack_position: Option<u32>,
    pub original_target: ReferenceV1,
    pub entered_turn: u32,
    pub entered_active_player: PlayerSeatV1,
    pub target_currently_legal: Option<bool>,
    pub outcome: Option<OutcomeV1>,
    pub target_movement: Option<MovementV1>,
    pub created: Vec<BirthV1>,
    pub other_nightmare_movements: Vec<MovementV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV1 {
    pub schema: String,
    pub viewer: PlayerSeatV1,
    pub meaning: MeaningV1,
    pub sources: Vec<public::CurrentV1>,
    pub eligible_graveyard_cards: Vec<ReferenceV1>,
    pub chooser: Option<TargetChoiceV1>,
    pub activations: Vec<ActivationV1>,
    pub nightmare_copies: Vec<TokenV1>,
}

fn visible(state: &GameState, viewer: PlayerId, object: ObjectId) -> Result<ReferenceV1, String> {
    public::reference(state, object, Some(viewer))
        .ok_or("Harvester original public object absent".into())
}

fn original(source: AbilitySourceContractV4) -> IncarnationV72 {
    IncarnationV72 {
        object: source.source.0,
        generation: source.zone_change_count,
        owner: source.owner.into(),
        controller: source.controller.into(),
        zone: source.zone,
    }
}

fn public_identity(printing: u16) -> IdentityV1 {
    match card_def::CARD_DEFS[usize::from(printing)].name {
        "Face-down creature" => IdentityV1::FaceDown { disguised: true },
        "Face-down card" => IdentityV1::FaceDown { disguised: false },
        _ => IdentityV1::Known { card_id: printing },
    }
}

fn activations(state: &GameState, viewer: PlayerId) -> Result<Vec<ActivationV1>, String> {
    receipts::operations(state)?
        .into_iter()
        .map(|row| {
            let item = row.publication.item.as_ref();
            let source = item
                .v4
                .ability_source_contract
                .ok_or("Harvester original activation source absent")?;
            let mut root = public::archived_root(state, row.publication_at, source, Some(viewer))?;
            root.original_source.identity = IdentityV1::Known { card_id: PRINTING };
            let [StackTargetContractV4::Object { controller, .. }] =
                item.v4.target_contracts.as_slice()
            else {
                return Err("Harvester original target contract disappeared".into());
            };
            let entered = row.target_entry;
            let target = ReferenceV1 {
                object: entered.card.object.0,
                generation: entered.card.expected_zone_change_count,
                owner: entered.owner.into(),
                controller: (*controller).into(),
                zone: Zone::Graveyard,
                identity: IdentityV1::Known {
                    card_id: entered.card_def,
                },
            };
            let mut target_movement = None;
            let mut other_nightmare_movements = Vec::new();
            for movement in row.moves {
                let fact = MovementV1 {
                    original: original(movement.original),
                    identity: public_identity(movement.public_printing),
                    actual_destination: movement.destination,
                };
                if movement.original.zone == Zone::Graveyard {
                    target_movement = Some(fact);
                } else {
                    other_nightmare_movements.push(fact);
                }
            }
            let stack_position = state
                .stack
                .iter()
                .position(|live| live == item)
                .map(|position| u32::try_from(position).map_err(|_| "Harvester stack width"))
                .transpose()?;
            let target_currently_legal = stack_position
                .map(|_| engine::stack_targets_still_legal(item, state))
                .transpose()?;
            Ok(ActivationV1 {
                source: root,
                stack_position,
                original_target: target,
                entered_turn: entered.turn,
                entered_active_player: entered.active_player.into(),
                target_currently_legal,
                outcome: row.outcome,
                target_movement,
                other_nightmare_movements,
                created: row
                    .created
                    .into_iter()
                    .map(|token| BirthV1 {
                        original: original(token.source),
                        copied_printing: token.source.card_def,
                        added_copiable_nightmare: true,
                    })
                    .collect(),
            })
        })
        .collect()
}

pub(crate) fn capture(
    state: &GameState,
    viewer: PlayerId,
    origin: &PolicyDecisionV5,
) -> Result<FactsV1, String> {
    require_scope(state)?;
    let eligible_graveyard_cards = state
        .players
        .iter()
        .flat_map(|p| &p.graveyard)
        .copied()
        .filter(|&id| native::eligible(state, id))
        .map(|id| visible(state, viewer, id))
        .collect::<Result<Vec<_>, _>>()?;
    let chooser = if let (
        Some(pending),
        PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::ChooseTargets {
            player,
            spell,
            remaining: 1,
            legal_targets,
            can_finish: false,
        })),
    ) = (state.engine.pending_activation.as_ref(), origin)
    {
        if *player == viewer
            && pending.controller == viewer
            && pending.source == *spell
            && pending.ability_index == 0
            && pending.target_spec == native::target_spec()
            && public::visible_known(state, *spell, Some(viewer), native::definition)
        {
            engine::validate_pending_activation(state, pending)?;
            let legal = legal_targets
                .iter()
                .map(|target| {
                    let Target::Object(object) = target else {
                        return Err("Harvester native target option is not a physical card".into());
                    };
                    let exact = visible(state, viewer, *object)?;
                    if !eligible_graveyard_cards.contains(&exact) {
                        return Err(
                            "Harvester native target lost its public turn-entry proof".into()
                        );
                    }
                    Ok(exact)
                })
                .collect::<Result<_, String>>()?;
            Some(TargetChoiceV1 {
                source: visible(state, viewer, *spell)?,
                legal,
            })
        } else {
            None
        }
    } else {
        None
    };
    let mut nightmare_copies = Vec::new();
    for player in &state.players {
        for &id in &player.battlefield {
            if !native::copiable_nightmare(state, id)? {
                continue;
            }
            let live = state.objects.get(id);
            nightmare_copies.push(TokenV1 {
                card: visible(state, viewer, id)?,
                form: public::checked_form(state, id)?,
                counters: crate::tinybones_public_v72::counters(state, id)?,
                tapped: live.tapped,
                summoning_sick: live.summoning_sick,
                copiable_nightmare: true,
            });
        }
    }
    Ok(FactsV1 {
        schema: SCHEMA.into(),
        viewer: viewer.into(),
        meaning: printed_meaning()?,
        sources: public::current_tinybones_compat_v72(state, PRINTING)?,
        eligible_graveyard_cards,
        chooser,
        activations: activations(state, viewer)?,
        nightmare_copies,
    })
}

fn require_scope(state: &GameState) -> Result<(), String> {
    if !state.harvester_commit_receipts_v1 {
        return Err("Harvester owner requires prospective native commit receipts".into());
    }
    Ok(())
}

pub(crate) fn validate_native(state: &GameState) -> Result<(), String> {
    require_scope(state)?;
    native::validate_state(state)?;
    receipts::operations(state)?;
    Ok(())
}
