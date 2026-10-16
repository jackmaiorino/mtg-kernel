//! Standalone current Snack facts from validated native inputs. No private DTO history.
use crate::{
    ids::{ObjectId, PlayerId},
    optional_trigger_public_v1 as public,
    rl::PlayerSeatV1,
    rules_meaning_midnight_snack_v58 as meaning,
    state::{AbilitySourceContractV4, GameState, StackItemKind, Target},
};
use serde::{Deserialize, Serialize};
pub const SCHEMA: &str = "mtg-kernel/midnight-snack-public/v58";
/// Exact nonnegative native-history sum. Little-endian 32-bit limbs keep every
/// operand integral in serde_json and the existing numeric-tree transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GainTotalV58 {
    pub limbs: [u32; 4],
}
impl From<u128> for GainTotalV58 {
    fn from(value: u128) -> Self {
        let bytes = value.to_le_bytes();
        Self {
            limbs: std::array::from_fn(|i| {
                u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().expect("four-byte limb"))
            }),
        }
    }
}
impl GainTotalV58 {
    pub fn value(self) -> u128 {
        self.limbs.iter().enumerate().fold(0, |value, (i, limb)| {
            value | (u128::from(*limb) << (i * 32))
        })
    }
}
impl PartialEq<u128> for GainTotalV58 {
    fn eq(&self, other: &u128) -> bool {
        self.value() == *other
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CastV58 {
    pub source: public::ReferenceV1,
    pub original_controller: PlayerSeatV1,
    pub is_copy: bool,
    pub payment_pending: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RaidV58 {
    pub root: public::RootV1,
    pub placement_ordered: bool,
    pub on_stack: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActivationStageV58 {
    ChoosingOpponentBeforeCosts,
    OpponentChosenAwaitingCosts,
    PaidAndSacrificedOnStack,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationV58 {
    pub root: public::RootV1,
    pub stage: ActivationStageV58,
    pub target: Option<PlayerSeatV1>,
    pub target_legal_now: Option<bool>,
    /// A current input to the future resolution, never a captured payable amount.
    pub original_controller_gained_this_turn_now: GainTotalV58,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV58 {
    pub schema: String,
    pub producer_sha256: String,
    pub viewer: PlayerSeatV1,
    pub meaning: meaning::MeaningV58,
    pub attacked_this_turn: [bool; 2],
    pub gained_this_turn: [GainTotalV58; 2],
    pub authorized_printings: Vec<public::ReferenceV1>,
    pub current: Vec<public::CurrentV1>,
    /// All visible Food tokens, without claiming they were made by Snack.
    pub current_food: Vec<public::CurrentV1>,
    pub casts: Vec<CastV58>,
    pub held_raid: Vec<RaidV58>,
    pub published_raid: Vec<RaidV58>,
    pub activations: Vec<ActivationV58>,
}
fn is_known(reference: &public::ReferenceV1, id: u16) -> bool {
    reference.identity == public::IdentityV1::Known { card_id: id }
}
fn source_visible(state: &GameState, id: ObjectId, viewer: PlayerId, printing: u16) -> bool {
    public::reference(state, id, Some(viewer)).is_some_and(|r| is_known(&r, printing))
}
pub fn producer_sha256() -> String {
    meaning::producer_sha256()
}
fn turn_inputs(state: &GameState) -> Result<([bool; 2], [GainTotalV58; 2]), String> {
    // The existing validated turn boundary and committed declarations/gains are
    // input authority. Neither current attackers nor a caller supplied total is.
    crate::life_loss_turn_v1::validate(state)?;
    let boundary = state
        .engine
        .event_history
        .iter()
        .rposition(|e| matches!(e, crate::event::CommittedEvent::LifeLossTurnBeganV1 { .. }))
        .ok_or("Snack original turn boundary absent")?;
    let mut attacked = [false; 2];
    for e in &state.engine.event_history[boundary + 1..] {
        if let crate::event::CommittedEvent::DeclaredAttacker { controller, .. } = e {
            attacked[controller.index()] = true;
        }
    }
    if attacked
        != [
            state.player_attacked_this_turn_v1(PlayerId::P0),
            state.player_attacked_this_turn_v1(PlayerId::P1),
        ]
    {
        return Err("Snack Raid flag disagrees with actual current-turn declarations".into());
    }
    Ok((
        attacked,
        [
            crate::midnight_snack_v1::gained_this_turn(state, PlayerId::P0)?.into(),
            crate::midnight_snack_v1::gained_this_turn(state, PlayerId::P1)?.into(),
        ],
    ))
}
pub fn capture(state: &GameState, viewer: PlayerId) -> Result<FactsV58, String> {
    let id = crate::card_def::card_id_by_name("Midnight Snack").ok_or("Snack printing absent")?;
    let meaning = meaning::printed_meaning(id)?;
    public::validate_state_checked(state)?;
    let (attacked_this_turn, gained_this_turn) = turn_inputs(state)?;
    let mut authorized_printings = vec![];
    for (object, _) in state.objects.iter() {
        if let Some(reference) =
            public::reference(state, object, Some(viewer)).filter(|r| is_known(r, id))
        {
            authorized_printings.push(reference);
        }
    }
    let mut casts = vec![];
    let mut held_raid = vec![];
    for p in public::held(state) {
        let Some(source) = p.source_contract.filter(|s| s.card_def == id) else {
            continue;
        };
        if p.granted_by.is_some() {
            continue;
        }
        if p.effect != crate::midnight_snack_v1::raid() {
            return Err("Snack held Raid original root changed".into());
        }
        held_raid.push(RaidV58 {
            root: public::root(state, source, Some(viewer))?,
            placement_ordered: p.placement_ordered,
            on_stack: false,
        });
    }
    let mut published_raid = vec![];
    let mut activations = vec![];
    for item in &state.stack {
        if item.kind == StackItemKind::Spell {
            if let Some(source) = item.v4.source_contract.filter(|s| s.card_def == id) {
                if !source_visible(state, source.source, viewer, id) {
                    continue;
                }
                let reference = public::reference(state, source.source, Some(viewer))
                    .ok_or("Snack public spell source absent")?;
                if reference.generation != source.zone_change_count {
                    return Err("Snack spell source changed incarnation".into());
                }
                let payment_pending = state
                    .engine
                    .pending_cast
                    .as_ref()
                    .is_some_and(|p| p.spell == source.source);
                if let Some(p) = state
                    .engine
                    .pending_cast
                    .as_ref()
                    .filter(|p| p.spell == source.source)
                {
                    crate::engine::validate_pending_cast(state, p)?;
                }
                casts.push(CastV58 {
                    source: reference,
                    original_controller: item.controller.into(),
                    is_copy: item.is_copy,
                    payment_pending,
                });
            }
            continue;
        }
        let Some(source) = item.v4.ability_source_contract.filter(|s| s.card_def == id) else {
            continue;
        };
        // Equipment and Cauldron grants remain generic authority, never Snack roots.
        if item.v4.granted_by.is_some() || !item.v4.cauldron_grant.is_empty() {
            continue;
        }
        if item.source != source.source || item.controller != source.controller {
            return Err("Snack live original source/controller changed".into());
        }
        if item.kind == StackItemKind::TriggeredAbility
            && item.inline_effect.as_ref() == Some(&crate::midnight_snack_v1::raid())
        {
            published_raid.push(RaidV58 {
                root: public::root(state, source, Some(viewer))?,
                placement_ordered: true,
                on_stack: true,
            });
        } else if item.kind == StackItemKind::ActivatedAbility
            && item.v4.activated_ability_index == Some(0)
            && item.inline_effect.as_ref() == Some(&crate::midnight_snack_v1::activation())
        {
            let [Target::Player(target)] = item.targets.as_slice() else {
                return Err("Snack original target absent".into());
            };
            if *target != item.controller.opponent() {
                return Err("Snack original opponent changed".into());
            }
            activations.push(ActivationV58 {
                root: public::root(state, source, Some(viewer))?,
                stage: ActivationStageV58::PaidAndSacrificedOnStack,
                target: Some((*target).into()),
                target_legal_now: Some(crate::engine::stack_targets_still_legal(item, state)?),
                original_controller_gained_this_turn_now: gained_this_turn[item.controller.index()],
            });
        } else if item.kind == StackItemKind::ActivatedAbility
            || item.kind == StackItemKind::TriggeredAbility
        {
            return Err("Snack printed live root changed".into());
        }
    }
    if let Some(p) = state
        .engine
        .pending_activation
        .as_ref()
        .filter(|p| source_visible(state, p.source, viewer, id))
    {
        crate::engine::validate_pending_activation(state, p)?;
        if p.ability_index == 0 && p.cauldron_grant.is_empty() {
            let target = match p.targets_chosen.as_slice() {
                [] => None,
                [Target::Player(player)] if *player == p.controller.opponent() => {
                    Some((*player).into())
                }
                _ => return Err("Snack staged opponent target changed".into()),
            };
            activations.push(ActivationV58 {
                root: public::root(
                    state,
                    AbilitySourceContractV4::capture(state, p.source),
                    Some(viewer),
                )?,
                stage: if target.is_some() {
                    ActivationStageV58::OpponentChosenAwaitingCosts
                } else {
                    ActivationStageV58::ChoosingOpponentBeforeCosts
                },
                target,
                target_legal_now: None,
                original_controller_gained_this_turn_now: gained_this_turn[p.controller.index()],
            });
        }
    }
    // A continuation cannot silently disappear behind live stack-only facts.
    // Snack's two roots contain no player choice; the native atomic resolution
    // owns them, so no externally held executable/resolving operand is projected.
    if state.engine.pending_effect.as_ref().is_some_and(|p| {
        p.resolving_item
            .v4
            .ability_source_contract
            .is_some_and(|s| s.card_def == id)
            && p.resolving_item.v4.granted_by.is_none()
            && p.resolving_item.v4.cauldron_grant.is_empty()
            && (p.resolving_item.inline_effect.as_ref() == Some(&crate::midnight_snack_v1::raid())
                || p.resolving_item.inline_effect.as_ref()
                    == Some(&crate::midnight_snack_v1::activation()))
    }) {
        return Err("Snack has no standalone suspended printed resolution stage".into());
    }
    let current = public::current_checked(state, id)?;
    let current_food = public::current_checked(state, meaning.food.card_id)?;
    Ok(FactsV58 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        viewer: viewer.into(),
        meaning,
        attacked_this_turn,
        gained_this_turn,
        authorized_printings,
        current,
        current_food,
        casts,
        held_raid,
        published_raid,
        activations,
    })
}
pub fn validate(state: &GameState, viewer: PlayerId, value: &FactsV58) -> Result<(), String> {
    if *value == capture(state, viewer)? {
        Ok(())
    } else {
        Err("Snack public facts changed".into())
    }
}
#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[path = "midnight_snack_public_v58_tests.rs"]
mod tests;
