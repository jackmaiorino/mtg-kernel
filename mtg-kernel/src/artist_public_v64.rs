//! Viewer-public Perforating Artist appendix. The captured native row owns Raid;
//! this read-only value grants no execution or complete-observation capability.
use crate::{
    card_def::TargetSpec,
    effect::{
        self, EffectFrame, EffectObjectBinding, EffectTargetSelectionPurpose, PendingEffectChoice,
    },
    ids::{ObjectId, PlayerId},
    optional_trigger_public_v1 as public,
    rl::PlayerSeatV1,
    rules_meaning_artist_v64 as meaning,
    state::{Counters, GameState, StackItem, StackItemKind, Target, Zone},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const SCHEMA: &str = "mtg-kernel/perforating-artist-public/v64";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CountersV64 {
    pub plus_one: i32,
    pub minus_one: i16,
    pub minus_toughness: i16,
    pub stun: i16,
    pub lore: i16,
    pub oil: i16,
    pub charge: i32,
    pub net: i32,
    pub bait: i32,
    pub soul: i32,
    pub incubation: i32,
    pub revival: i32,
    pub fellowship: i32,
    pub lifelink: i16,
    pub time: u8,
    pub finality: u16,
    pub loyalty: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermanentV64 {
    pub card: public::ReferenceV1,
    pub is_token: bool,
    pub tapped: bool,
    pub summoning_sick: bool,
    pub marked_damage: u32,
    pub counters: CountersV64,
    pub form: public::FormV1,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StageV64 {
    Held,
    Published,
    Resolving,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaymentKindV64 {
    SacrificeControlledNonlandPermanent,
    DiscardCard,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaymentStatusV64 {
    Waiting,
    Answered,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaymentV64 {
    pub kind: PaymentKindV64,
    pub status: PaymentStatusV64,
    pub payer: PlayerSeatV1,
    pub is_target: bool,
    pub may_decline: bool,
    /// A discard roster and its selected prefix remain payer-private until the
    /// original native reducer actually commits the discard.
    pub candidates: Option<Vec<public::ReferenceV1>>,
    pub selected: Option<Vec<public::ReferenceV1>>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerV64 {
    pub stage: StageV64,
    pub position: u32,
    pub source: public::RootV1,
    pub affected_opponent: PlayerSeatV1,
    pub captured_controller_attacked_this_turn: bool,
    pub payment: Option<PaymentV64>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CastV64 {
    pub position: u32,
    pub source: public::ReferenceV1,
    pub original_controller: PlayerSeatV1,
    pub is_uncast_copy: bool,
    pub payment_pending: bool,
    pub genuine_cast_finalized: bool,
    pub current_form: public::FormV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV64 {
    pub schema: String,
    pub producer_sha256: String,
    pub viewer: PlayerSeatV1,
    pub meaning: meaning::MeaningV64,
    pub authorized_printings: Vec<public::ReferenceV1>,
    pub battlefield: [Vec<PermanentV64>; 2],
    pub casts: Vec<CastV64>,
    pub triggers: Vec<TriggerV64>,
    pub complete_ordinary_observation: bool,
    pub owning_consumer_admitted: bool,
}
pub fn producer_sha256() -> String {
    meaning::producer_sha256()
}
fn ordinal(value: usize) -> Result<u32, String> {
    u32::try_from(value).map_err(|_| "V64 public row ordinal overflow".into())
}
fn counters(state: &GameState, id: ObjectId) -> Result<CountersV64, String> {
    let live = state.objects.get(id);
    let Counters {
        plus1_plus1,
        minus1_minus1,
        minus0_minus1,
        stun,
        lore,
        oil,
        charge,
        net,
        bait,
        soul,
        incubation,
        revival,
        fellowship,
        stash,
    } = live.counters;
    if stash != 0 {
        return Err("public projection does not expose stash counters".into());
    }
    let lifelink = live.v4.lifelink_keyword_counters;
    if [
        i64::from(plus1_plus1),
        i64::from(minus1_minus1),
        i64::from(minus0_minus1),
        i64::from(stun),
        i64::from(lore),
        i64::from(oil),
        i64::from(charge),
        i64::from(net),
        i64::from(bait),
        i64::from(soul),
        i64::from(incubation),
        i64::from(revival),
        i64::from(fellowship),
        i64::from(lifelink),
    ]
    .iter()
    .any(|n| *n < 0)
    {
        return Err("V64 negative native public counter".into());
    }
    Ok(CountersV64 {
        plus_one: plus1_plus1,
        minus_one: minus1_minus1,
        minus_toughness: minus0_minus1,
        stun,
        lore,
        oil,
        charge,
        net,
        bait,
        soul,
        incubation,
        revival,
        fellowship,
        lifelink,
        time: live.v4.time_counters_v1,
        finality: live.v4.creature_upgrade.as_ref().map_or(0, |u| u.finality),
        loyalty: crate::planeswalker_v1::loyalty(state, id).unwrap_or(0),
    })
}
fn binding_reference(
    state: &GameState,
    binding: EffectObjectBinding,
    viewer: PlayerId,
) -> Result<public::ReferenceV1, String> {
    let live = state
        .objects
        .try_get(binding.object)
        .ok_or("V64 payment object absent")?;
    if live.zone != binding.expected_zone
        || live.zone_change_count != binding.expected_zone_change_count
    {
        return Err("V64 payment changed its original incarnation".into());
    }
    public::reference(state, binding.object, Some(viewer))
        .ok_or("V64 payment reference hidden".into())
}
fn payment(state: &GameState, viewer: PlayerId) -> Result<Option<PaymentV64>, String> {
    let Some(pending) = state.engine.pending_effect.as_ref() else {
        return Ok(None);
    };
    let payer = pending.resolving_item.controller.opponent();
    if let Some(PendingEffectChoice::SelectTargets {
        purpose,
        selected,
        legal,
        ..
    }) = &pending.choice
    {
        let kind = match purpose {
            EffectTargetSelectionPurpose::NonlandSacrificePaymentV1 { .. } => {
                PaymentKindV64::SacrificeControlledNonlandPermanent
            }
            EffectTargetSelectionPurpose::DiscardPaymentV1 { .. } => PaymentKindV64::DiscardCard,
            _ => return Err("V64 Artist payment lost its native purpose".into()),
        };
        let visible =
            kind == PaymentKindV64::SacrificeControlledNonlandPermanent || viewer == payer;
        let refs =
            |rows: &[effect::EffectTargetCandidate]| -> Result<Vec<public::ReferenceV1>, String> {
                rows.iter()
                    .map(|row| {
                        let binding = row
                            .expected_object
                            .ok_or("V64 non-target payment lost object binding")?;
                        if row.target != Target::Object(binding.object) {
                            return Err("V64 payment target/binding changed".into());
                        }
                        binding_reference(state, binding, viewer)
                    })
                    .collect()
            };
        return Ok(Some(PaymentV64 {
            kind,
            status: PaymentStatusV64::Waiting,
            payer: payer.into(),
            is_target: false,
            may_decline: true,
            candidates: if visible { Some(refs(legal)?) } else { None },
            selected: if visible { Some(refs(selected)?) } else { None },
        }));
    }
    let Some(frame) = pending.frames.first() else {
        return Ok(None);
    };
    let (kind, roster, chosen) = match frame {
        EffectFrame::NonlandSacrificePaymentV1 { choice, selected } => (
            PaymentKindV64::SacrificeControlledNonlandPermanent,
            &choice.permanents,
            selected,
        ),
        EffectFrame::DiscardPaymentV1 { choice, selected } => {
            (PaymentKindV64::DiscardCard, &choice.hand, selected)
        }
        EffectFrame::Program { .. } => return Ok(None),
        _ => return Err("V64 Artist answered payment lost its original frame".into()),
    };
    let visible = kind == PaymentKindV64::SacrificeControlledNonlandPermanent || viewer == payer;
    let refs = |rows: &[EffectObjectBinding]| -> Result<Vec<public::ReferenceV1>, String> {
        rows.iter()
            .map(|&row| binding_reference(state, row, viewer))
            .collect()
    };
    Ok(Some(PaymentV64 {
        kind,
        status: PaymentStatusV64::Answered,
        payer: payer.into(),
        is_target: false,
        may_decline: true,
        candidates: if visible { Some(refs(roster)?) } else { None },
        selected: if visible { Some(refs(chosen)?) } else { None },
    }))
}
fn trigger(
    state: &GameState,
    viewer: PlayerId,
    row: &StackItem,
    stage: StageV64,
    position: usize,
) -> Result<TriggerV64, String> {
    effect::perforating_artist_v1::validate_origin(state, row)?;
    crate::engine::validated_stack_item_target_spec(row, state)?;
    let source = row
        .v4
        .ability_source_contract
        .ok_or("V64 Raid source absent")?;
    Ok(TriggerV64 {
        stage,
        position: ordinal(position)?,
        source: public::root(state, source, Some(viewer))?,
        affected_opponent: source.controller.opponent().into(),
        captured_controller_attacked_this_turn: state
            .player_attacked_this_turn_v1(source.controller),
        payment: if stage == StageV64::Resolving {
            payment(state, viewer)?
        } else {
            None
        },
    })
}
pub fn capture(state: &GameState, viewer: PlayerId) -> Result<FactsV64, String> {
    let id = meaning::selected_id()?;
    public::validate_state_checked(state)?;
    if let Some(pending) = &state.engine.pending_cast {
        crate::engine::validate_pending_cast(state, pending)?;
    }
    if let Some(pending) = &state.engine.pending_activation {
        crate::engine::validate_pending_activation(state, pending)?;
    }
    crate::doubling_season_v48::validate_token_journals(state)?;
    let mut authorized_printings = vec![];
    for (object, live) in state.objects.iter() {
        if (live.v4.is_token && live.zone != Zone::Battlefield)
            || (live.spell_copy_origin.is_some()
                && live.zone == Zone::Stack
                && !state.stack.iter().any(|r| r.source == object))
        {
            continue;
        }
        // Visibility and face-down concealment precede printing selection.
        let Some(reference) = public::reference(state, object, Some(viewer)) else {
            continue;
        };
        if matches!(reference.identity, public::IdentityV1::Known { card_id } if card_id == id) {
            authorized_printings.push(reference);
        }
    }
    let mut battlefield: [Vec<PermanentV64>; 2] = [vec![], vec![]];
    let mut seen = BTreeSet::new();
    for seat in [PlayerId::P0, PlayerId::P1] {
        for &object in &state.players[seat.index()].battlefield {
            let live = state
                .objects
                .try_get(object)
                .ok_or("V64 battlefield member absent")?;
            if live.zone != Zone::Battlefield || live.controller != seat || !seen.insert(object) {
                return Err("V64 battlefield membership changed".into());
            }
            battlefield[seat.index()].push(PermanentV64 {
                card: public::reference(state, object, Some(viewer))
                    .ok_or("V64 battlefield reference absent")?,
                is_token: live.v4.is_token,
                tapped: live.tapped,
                summoning_sick: live.summoning_sick,
                marked_damage: live.damage,
                counters: counters(state, object)?,
                form: public::checked_form(state, object)?,
            });
        }
    }
    if state
        .objects
        .iter()
        .any(|(object, live)| live.zone == Zone::Battlefield && !seen.contains(&object))
    {
        return Err("V64 battlefield roster incomplete".into());
    }
    let mut casts = vec![];
    let mut triggers = vec![];
    for (position, row) in public::held(state).into_iter().enumerate() {
        let Some(source) = row.source_contract.filter(|s| s.card_def == id) else {
            continue;
        };
        crate::engine::validate_pending_trigger_identity(state, row)?;
        if source.source != row.source
            || source.controller != row.controller
            || source.zone != Zone::Battlefield
            || row.effect != effect::perforating_artist_v1::raid()
            || row.is_madness_offer
            || row.kicked
            || row.target_spec != TargetSpec::None
            || !row.targets.is_empty()
            || !row.target_contracts.is_empty()
            || row.granted_by.is_some()
            || row.optional_additional_cost_paid.is_some()
            || !row.paid_cost_refs.is_empty()
            || state.step != crate::state::Step::End
            || !state.player_attacked_this_turn_v1(source.controller)
        {
            return Err("V64 held Raid changed its native source/program".into());
        }
        triggers.push(TriggerV64 {
            stage: StageV64::Held,
            position: ordinal(position)?,
            source: public::root(state, source, Some(viewer))?,
            affected_opponent: source.controller.opponent().into(),
            captured_controller_attacked_this_turn: true,
            payment: None,
        });
    }
    for (position, row) in state.stack.iter().enumerate() {
        if state
            .engine
            .pending_effect
            .as_ref()
            .is_some_and(|p| p.resolving_item == *row)
        {
            continue;
        }
        if row.kind == StackItemKind::Spell {
            let live = state
                .objects
                .try_get(row.source)
                .ok_or("V64 stack source absent")?;
            if live.v4.face_down_v1.is_none() && live.card_def == id {
                casts.push(CastV64 {
                    position: ordinal(position)?,
                    source: public::reference(state, row.source, Some(viewer))
                        .ok_or("V64 spell reference absent")?,
                    original_controller: row.controller.into(),
                    is_uncast_copy: row.is_copy,
                    payment_pending: state
                        .engine
                        .pending_cast
                        .as_ref()
                        .is_some_and(|p| p.spell == row.source),
                    genuine_cast_finalized: !row.is_copy
                        && row
                            .v4
                            .source_contract
                            .and_then(|s| s.spell_cast_origin)
                            .is_some_and(|o| o.finalized_method.is_some()),
                    current_form: public::checked_form(state, row.source)?,
                });
            }
        } else if row
            .v4
            .ability_source_contract
            .is_some_and(|s| s.card_def == id)
        {
            triggers.push(trigger(state, viewer, row, StageV64::Published, position)?);
        }
    }
    if let Some(pending) = &state.engine.pending_effect {
        if pending
            .resolving_item
            .v4
            .ability_source_contract
            .is_some_and(|s| s.card_def == id)
        {
            triggers.push(trigger(
                state,
                viewer,
                &pending.resolving_item,
                StageV64::Resolving,
                0,
            )?);
        }
    }
    Ok(FactsV64 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        viewer: viewer.into(),
        meaning: meaning::printed_meaning()?,
        authorized_printings,
        battlefield,
        casts,
        triggers,
        complete_ordinary_observation: false,
        owning_consumer_admitted: false,
    })
}
pub fn validate(state: &GameState, viewer: PlayerId, value: &FactsV64) -> Result<(), String> {
    if *value == capture(state, viewer)? {
        Ok(())
    } else {
        Err("V64 complete viewer facts changed".into())
    }
}
