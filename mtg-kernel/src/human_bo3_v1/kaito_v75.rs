//! Fixed-seat Human facade for the actual sealed Kaito V75 owner.
//! Anonymous historical roles never reconstruct a later hidden printing.
use super::chandra_v74::{chandra_facts, HumanChandraActionV74, HumanChandraFactsV74};
use super::liliana_v73::{
    liliana_facts, liliana_root, HumanLilianaActionV73, HumanLilianaFactsV73, HumanLilianaRootV73,
};
use super::muldrotha_v71::*;
use super::tinybones_v72::{HumanTinybonesCurrentV72, HumanTinybonesRelationV72, Incarnations};
use super::visible::Handles;
use super::*;
use crate::{
    graveyard_return_public_v61 as public, kaito_observation_v75 as observation,
    rl::{ActionSemanticV1, RlContractError},
    rl_session::{
        FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1, KaitoDecisionV75,
        KaitoSessionV75, KaitoSnapshotV75,
    },
};
use std::sync::Arc;
type Error = HumanDecisionErrorV1;
macro_rules! record {
    ($name:ident { $($(#[$meta:meta])* $field:ident:$ty:ty),* $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
        pub struct $name { $($(#[$meta])* pub $field:$ty),* }
    };
}
record!(HumanVisibleStateKaitoV75 {
    common: super::elenda_v52::HumanVisibleStateElendaV52,
    prepared: super::prepared_v57::HumanPreparedFactsV57,
    next_five: super::next_five_v59::HumanNextFiveFactsV59,
    graveyard_return: HumanCallFactsV62,
    call: HumanNativeCallFactsV62,
    end_turn: HumanEndTurnFactsV68,
    genesis_wave: HumanGenesisWaveCompatibilityV71,
    vivien: HumanLoyaltyFactsV71,
    muldrotha: HumanMuldrothaFactsV71,
    tinybones: super::tinybones_v72::HumanTinybonesFactsV72,
    liliana: HumanLilianaFactsV73,
    chandra: HumanChandraFactsV74,
    kaito: HumanKaitoFactsV75,
    #[serde(skip_serializing_if = "Option::is_none")]
    artist: Option<HumanArtistFactsV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chosen_type: Option<super::chosen_type_v1::HumanChosenTypeFactsV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    niv: Option<HumanNivFactsV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    drakuseth: Option<super::drakuseth_consumer_v1::HumanFactsV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    embercleave: Option<super::embercleave_consumer_v1::HumanFactsV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    zimone: Option<super::zimone_consumer_v1::HumanFactsV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    omniscience: Option<super::omniscience_consumer_v1::HumanFactsV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tutelage: Option<super::tutelage_consumer_v1::HumanFactsV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    moon: Option<super::moon_consumer_v1::HumanFactsV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    leyline_axe: Option<super::leyline_axe_consumer_v1::HumanFactsV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    progenitus: Option<super::progenitus_consumer_v1::HumanFactsV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    aberration: Option<super::aberration_consumer_v1::HumanFactsV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lathril: Option<super::lathril_consumer_v1::HumanFactsV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    electroduplicate: Option<super::electroduplicate_consumer_v1::HumanFactsV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    edict: Option<super::edict_consumer_v1::HumanFactsV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    loot: Option<super::loot_consumer_v1::HumanFactsV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    curator: Option<super::curator_consumer_v1::HumanFactsV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    kykar: Option<super::kykar_consumer_v1::HumanFactsV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sphinx_lore: Option<super::sphinx_lore_consumer_v1::HumanFactsV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    greatwurm: Option<super::greatwurm_consumer_v1::HumanFactsV1>
});
record!(HumanDecisionKaitoV75 {
    schema: String, human_seat: PlayerSeatV1, prompt_seq: u64,
    state: HumanVisibleStateKaitoV75,
    actions: Vec<HumanLegalActionV1>, action_meanings: Vec<HumanKaitoCandidateV75>
});
record!(HumanKaitoCandidateV75 {
    tinybones: super::tinybones_v72::HumanTinybonesActionV72,
    liliana: HumanLilianaActionV73,
    chandra: HumanChandraActionV74,
    kaito: HumanKaitoActionV75,
    muldrotha_permission: Option<HumanMuldrothaPermissionActionV71>,
    drake_activation: Option<super::next_five_v59::HumanNextFiveActivationV59>,
    programs: Vec<HumanCallTriggerV62>,
    selected_graveyard_target: Option<HumanCallTargetV62>,
    nine_lives_deaths: Vec<HumanCallDeathV62>,
    nine_lives_schedules: Vec<HumanCallScheduleV62>,
    call_returns: Vec<HumanNativeCallReturnV62>,
    end_turn_cleanup: Option<crate::time_stop_public_v68::CleanupV68>,
    disposed_stack_outcomes: Vec<HumanEndTurnDisposalV68>,
    compatibility_terminals: Vec<crate::time_stop_public_v68::CompatibilityTerminalV68>,
    genesis_waves: Vec<HumanWaveContextV71>,
    vivien_choices:Vec<crate::vivien_public_v70::ChoiceContextV70>,
    vivien_activation:Option<HumanLoyaltyCostV71>,
    #[serde(skip_serializing_if = "Option::is_none")]
    artist_payment: Option<crate::artist_consumer_v1::PaymentActionV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chosen_type_answer: Option<crate::chosen_type_consumer_v1::AnswerV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    drakuseth_target: Option<crate::drakuseth_consumer_v1::TargetActionV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    embercleave_action: Option<crate::embercleave_consumer_v1::ActionV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    zimone_action: Option<crate::zimone_consumer_v1::ActionV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    omniscience_action: Option<crate::omniscience_consumer_v1::ActionV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tutelage_action: Option<crate::tutelage_consumer_v1::ActionV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    moon_action: Option<crate::moon_consumer_v1::ActionV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    leyline_axe_action: Option<crate::leyline_axe_consumer_v1::ActionV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    progenitus_action: Option<crate::progenitus_consumer_v1::ActionV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    aberration_action: Option<crate::aberration_consumer_v1::ActionV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lathril_action: Option<crate::lathril_consumer_v1::ActionV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    electroduplicate_action: Option<crate::electroduplicate_consumer_v1::ActionV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    edict_action: Option<crate::edict_consumer_v1::ActionV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    loot_action: Option<crate::loot_consumer_v1::ActionV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    curator_action: Option<crate::curator_consumer_v1::ActionV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    kykar_action: Option<crate::kykar_consumer_v1::ActionV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sphinx_lore_action: Option<crate::sphinx_lore_consumer_v1::ActionV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    greatwurm_action: Option<crate::greatwurm_consumer_v1::ActionV1>
});
record!(HumanArtistFactsV1 {
    meaning: crate::rules_meaning_artist_v64::MeaningV64,
    authorized_printings: Vec<HumanCardRefV1>,
    casts: Vec<HumanArtistCastV1>,
    triggers: Vec<HumanArtistTriggerV1>
});
record!(HumanArtistCastV1 {
    position: u32,
    source: HumanCardRefV1,
    original_controller: PlayerSeatV1,
    is_uncast_copy: bool,
    payment_pending: bool,
    genuine_cast_finalized: bool,
    current_form: crate::optional_trigger_public_v1::FormV1
});
record!(HumanArtistTriggerV1 {
    stage: crate::artist_public_v64::StageV64,
    position: u32,
    source: HumanCallRootV62,
    affected_opponent: PlayerSeatV1,
    captured_controller_attacked_this_turn: bool,
    payment: Option<HumanArtistPaymentV1>
});
record!(HumanArtistPaymentV1 {
    kind: crate::artist_public_v64::PaymentKindV64,
    status: crate::artist_public_v64::PaymentStatusV64,
    payer: PlayerSeatV1,
    is_target: bool,
    may_decline: bool,
    candidates: Option<Vec<HumanCardRefV1>>,
    selected: Option<Vec<HumanCardRefV1>>
});
record!(HumanNivFactsV1 {
    meaning: crate::rules_meaning_niv_mizzet_v63::MeaningV63,
    authorized: Vec<HumanCardRefV1>,
    current: Vec<HumanNivCurrentV1>,
    casts: Vec<HumanNivCastV1>,
    damage_draws: Vec<HumanNivDrawV1>,
    results: Vec<crate::niv_mizzet_public_v63::DrawResultV1>,
    no_maximum_hand_size: [bool; 2]
});
record!(HumanNivCurrentV1 {
    card: HumanCardRefV1,
    is_token: bool,
    tapped: bool,
    summoning_sick: bool,
    marked_damage: u32,
    form: crate::optional_trigger_public_v1::FormV1
});
record!(HumanNivCastV1 {
    position: u32,
    source: HumanCardRefV1,
    original_controller: PlayerSeatV1,
    is_uncast_copy: bool,
    payment_pending: bool,
    genuine_cast_finalized: bool
});
record!(HumanNivDrawV1 {
    occurrence: u32,
    observer: HumanCallRootV62,
    damage_source: HumanTinybonesRelationV72,
    damage_source_controller_basis: crate::niv_mizzet_public_v63::ControllerBasisV63,
    current_original_damage_source: Option<HumanCardRefV1>,
    recipient: PlayerSeatV1,
    committed_damage: i32,
    commanded_draw_count: i32,
    stage: crate::niv_mizzet_public_v63::StageV63,
    native_position: Option<u32>
});

fn niv_facts(
    value: &crate::niv_mizzet_public_v63::ConsumerFactsV1,
    handles: &mut Handles,
    roles: &mut Incarnations,
) -> Result<HumanNivFactsV1, Error> {
    let appendix = &value.appendix;
    Ok(HumanNivFactsV1 {
        meaning: appendix.meaning.clone(),
        authorized: appendix
            .authorized
            .iter()
            .map(|r| href(r, handles))
            .collect::<Result<_, _>>()?,
        current: appendix
            .current
            .iter()
            .map(|row| {
                Ok(HumanNivCurrentV1 {
                    card: href(&row.card, handles)?,
                    is_token: row.is_token,
                    tapped: row.tapped,
                    summoning_sick: row.summoning_sick,
                    marked_damage: row.marked_damage,
                    form: row.form.clone(),
                })
            })
            .collect::<Result<_, Error>>()?,
        casts: appendix
            .casts
            .iter()
            .map(|row| {
                Ok(HumanNivCastV1 {
                    position: row.position,
                    source: href(&row.source, handles)?,
                    original_controller: row.original_controller,
                    is_uncast_copy: row.is_uncast_copy,
                    payment_pending: row.payment_pending,
                    genuine_cast_finalized: row.genuine_cast_finalized,
                })
            })
            .collect::<Result<_, Error>>()?,
        damage_draws: appendix
            .damage_draws
            .iter()
            .map(|row| {
                let damage = &row.damage_source;
                Ok(HumanNivDrawV1 {
                    occurrence: row.occurrence,
                    observer: root(&row.observer, handles)?,
                    damage_source: roles.relation(
                        &crate::tinybones_public_v72::IncarnationV72 {
                            object: damage.object,
                            generation: damage.generation,
                            owner: damage.owner,
                            controller: damage.damage_controller,
                            zone: damage.original_zone,
                        },
                    )?,
                    damage_source_controller_basis: damage.controller_basis.clone(),
                    current_original_damage_source: damage
                        .current_original_source
                        .as_ref()
                        .map(|r| href(r, handles))
                        .transpose()?,
                    recipient: row.recipient,
                    committed_damage: row.committed_damage,
                    commanded_draw_count: row.draw_count,
                    stage: row.stage,
                    native_position: row.native_position,
                })
            })
            .collect::<Result<_, Error>>()?,
        results: value.results.clone(),
        no_maximum_hand_size: appendix.no_maximum_hand_size,
    })
}

fn artist_facts(
    value: &crate::artist_public_v64::FactsV64,
    handles: &mut Handles,
) -> Result<HumanArtistFactsV1, Error> {
    let refs = |rows: &[crate::optional_trigger_public_v1::ReferenceV1], h: &mut Handles| {
        rows.iter()
            .map(|r| href(r, h))
            .collect::<Result<Vec<_>, Error>>()
    };
    Ok(HumanArtistFactsV1 {
        meaning: value.meaning.clone(),
        authorized_printings: refs(&value.authorized_printings, handles)?,
        casts: value
            .casts
            .iter()
            .map(|cast| {
                Ok(HumanArtistCastV1 {
                    position: cast.position,
                    source: href(&cast.source, handles)?,
                    original_controller: cast.original_controller,
                    is_uncast_copy: cast.is_uncast_copy,
                    payment_pending: cast.payment_pending,
                    genuine_cast_finalized: cast.genuine_cast_finalized,
                    current_form: cast.current_form.clone(),
                })
            })
            .collect::<Result<_, Error>>()?,
        triggers: value
            .triggers
            .iter()
            .map(|trigger| {
                Ok(HumanArtistTriggerV1 {
                    stage: trigger.stage,
                    position: trigger.position,
                    source: root(&trigger.source, handles)?,
                    affected_opponent: trigger.affected_opponent,
                    captured_controller_attacked_this_turn: trigger
                        .captured_controller_attacked_this_turn,
                    payment: trigger
                        .payment
                        .as_ref()
                        .map(|payment| {
                            Ok::<_, Error>(HumanArtistPaymentV1 {
                                kind: payment.kind,
                                status: payment.status,
                                payer: payment.payer,
                                is_target: payment.is_target,
                                may_decline: payment.may_decline,
                                candidates: payment
                                    .candidates
                                    .as_deref()
                                    .map(|rows| refs(rows, handles))
                                    .transpose()?,
                                selected: payment
                                    .selected
                                    .as_deref()
                                    .map(|rows| refs(rows, handles))
                                    .transpose()?,
                            })
                        })
                        .transpose()?,
                })
            })
            .collect::<Result<_, Error>>()?,
    })
}

pub(super) fn choice_label(
    obs: &observation::KaitoObservationV75,
    a: &ActionSemanticV1,
    h: &Handles,
) -> Result<Option<String>, Error> {
    use crate::rl::TargetRefV1;
    if let Some(label) = super::greatwurm_consumer_v1::label(obs.greatwurm.as_ref(), a, h)? { return Ok(Some(label)); }
    if let Some(label) = super::sphinx_lore_consumer_v1::label(obs.sphinx_lore.as_ref(), a, h)? { return Ok(Some(label)); }
    if let Some(label) = super::kykar_consumer_v1::label(obs.kykar.as_ref(), a, h)? { return Ok(Some(label)); }
    if let Some(label) = super::loot_consumer_v1::label(obs.loot.as_ref(), a, h)? { return Ok(Some(label)); }
    if let Some(label) = super::edict_consumer_v1::label(obs.edict.as_ref(), a, h)? {
        return Ok(Some(label));
    }
    if let Some(label) = super::curator_consumer_v1::label(obs.curator.as_ref(), a, h)? {
        return Ok(Some(label));
    }
    if let Some(label) = super::lathril_consumer_v1::label(obs.lathril.as_ref(), a, h)? {
        return Ok(Some(label));
    }
    if let Some(label) = super::leyline_axe_consumer_v1::label(obs.leyline_axe.as_ref(), a, h)? {
        return Ok(Some(label));
    }
    if let Some(label) = super::moon_consumer_v1::label(obs.moon.as_ref(), a, h)? {
        return Ok(Some(label));
    }
    if let Some(label) = super::progenitus_consumer_v1::label(obs.progenitus.as_ref(), a, h)? {
        return Ok(Some(label));
    }
    if let Some(label) = super::aberration_consumer_v1::label(obs.aberration.as_ref(), a, h)? {
        return Ok(Some(label));
    }
    if let Some(label) =
        super::electroduplicate_consumer_v1::label(obs.electroduplicate.as_ref(), a, h)?
    {
        return Ok(Some(label));
    }
    if let Some(label) = super::tutelage_consumer_v1::label(obs.tutelage.as_ref(), a, h)? {
        return Ok(Some(label));
    }
    if let Some(label) = super::omniscience_consumer_v1::label(obs.omniscience.as_ref(), a, h)? {
        return Ok(Some(label));
    }
    if let Some(label) = super::zimone_consumer_v1::label(obs.zimone.as_ref(), a, h)? {
        return Ok(Some(label));
    }
    if let Some(embercleave) = crate::embercleave_consumer_v1::action(obs.embercleave.as_ref(), a)
        .map_err(|_| Error::InvalidVisibleReference)?
    {
        use crate::embercleave_consumer_v1::ActionV1;
        use crate::embercleave_public_v65::AbilityKindV65;
        return Ok(Some(match embercleave {
            ActionV1::CastQuote {
                generic_due_now,
                retained_red_pips,
                ..
            } => {
                format!("Cast Embercleave (current printed quote {generic_due_now} generic, {retained_red_pips} red)")
            }
            ActionV1::ActivateEquip {
                printed_generic_cost,
            } => {
                format!("Activate Embercleave equip {printed_generic_cost}")
            }
            ActionV1::AttachTarget { kind, .. } => {
                let ActionSemanticV1::ChooseTarget {
                    target: TargetRefV1::Object { object },
                    ..
                } = a
                else {
                    return Err(Error::InvalidVisibleReference);
                };
                let route = match kind {
                    AbilityKindV65::EntryAttach => "entry attachment",
                    AbilityKindV65::Equip => "equip",
                };
                format!("Choose {} for Embercleave {route}", h.name(object)?)
            }
        }));
    }
    if let Some(answer) = crate::chosen_type_consumer_v1::answer(obs.chosen_type.as_ref(), a)
        .map_err(|_| Error::InvalidVisibleReference)?
    {
        let printing = crate::card_def::CARD_DEFS[usize::from(answer.printing.card_id())].name;
        return Ok(Some(format!(
            "{printing}: choose {}",
            answer.creature_type.canonical_name()
        )));
    }
    if let Some(answer) = crate::drakuseth_consumer_v1::target_action(obs.drakuseth.as_ref(), a)
        .map_err(|_| Error::InvalidVisibleReference)?
    {
        return Ok(Some(match answer {
            crate::drakuseth_consumer_v1::TargetActionV1::Finish { chosen_slots } => {
                format!("Finish Drakuseth targets with {chosen_slots} original slots")
            }
            crate::drakuseth_consumer_v1::TargetActionV1::Choose {
                original_slot,
                printed_damage,
            } => {
                let ActionSemanticV1::ChooseTarget { target, .. } = a else {
                    return Err(Error::InvalidVisibleReference);
                };
                let label = match target {
                    TargetRefV1::Player { player } => format!("player {player:?}"),
                    TargetRefV1::Object { object } => h.name(object)?,
                    _ => return Err(Error::InvalidVisibleReference),
                };
                format!(
                    "Drakuseth target slot {}: {printed_damage} damage to {label}",
                    original_slot + 1
                )
            }
        }));
    }
    if let (
        Some(menu),
        ActionSemanticV1::ChooseEffectOption {
            source,
            option_index,
            ..
        },
    ) = (
        obs.drakuseth
            .as_ref()
            .and_then(|f| f.pending_damage.as_ref()),
        a,
    ) {
        if source.card_db_id == crate::drakuseth_consumer_v1::PRINTING_V1 {
            let option = menu
                .options
                .get(usize::from(*option_index))
                .ok_or(Error::InvalidVisibleReference)?;
            return Ok(Some(format!(
                "Apply {:?} to Drakuseth target slot {} ({} damage remaining)",
                option.kind,
                menu.active_original_slot + 1,
                menu.remaining_amount
            )));
        }
    }
    if let Some(payment) = crate::artist_consumer_v1::payment_action(obs.artist.as_ref(), a)
        .map_err(|_| Error::InvalidVisibleReference)?
    {
        use crate::artist_consumer_v1::AnswerV1;
        use crate::artist_public_v64::PaymentKindV64;
        let label = match (payment.kind, payment.answer, a) {
            (
                PaymentKindV64::SacrificeControlledNonlandPermanent,
                AnswerV1::SelectPayment,
                ActionSemanticV1::ChooseEffectTarget {
                    target: TargetRefV1::Object { object },
                    ..
                },
            ) => {
                format!("Select {} to sacrifice", h.name(object)?)
            }
            (
                PaymentKindV64::DiscardCard,
                AnswerV1::SelectPayment,
                ActionSemanticV1::ChooseEffectTarget {
                    target: TargetRefV1::Object { object },
                    ..
                },
            ) => {
                format!("Select {} to discard", h.name(object)?)
            }
            (PaymentKindV64::SacrificeControlledNonlandPermanent, AnswerV1::ConfirmPayment, _) => {
                "Confirm sacrifice".into()
            }
            (PaymentKindV64::DiscardCard, AnswerV1::ConfirmPayment, _) => "Confirm discard".into(),
            (PaymentKindV64::SacrificeControlledNonlandPermanent, AnswerV1::DeclinePayment, _) => {
                "Decline sacrifice".into()
            }
            (PaymentKindV64::DiscardCard, AnswerV1::DeclinePayment, _) => {
                "Decline discard and lose 3 life".into()
            }
            _ => return Err(Error::InvalidVisibleReference),
        };
        return Ok(Some(label));
    }
    if let Some(c) = crate::kaito_public_v75::activation_cost(&obs.kaito, a)
        .map_err(|_| Error::InvalidVisibleReference)?
    {
        let delta = [1, -2, -9][usize::from(c.ability_index)];
        let effect=["up to one creature you control cannot be blocked this turn; then draw one and privately discard one","create a blue 2/1 Ninja","create a persistent emblem that creates a Ninja whenever any player casts a spell"][usize::from(c.ability_index)];
        return Ok(Some(format!(
            "Activate {}: {delta:+} loyalty ({} to {}); {effect}",
            h.name(&c.source)?,
            c.before,
            c.after
        )));
    }
    match a {
        ActionSemanticV1::ChooseTarget {
            source,
            target: TargetRefV1::Object { object },
            ..
        }
        | ActionSemanticV1::ChooseEffectTarget {
            source,
            target: TargetRefV1::Object { object },
            ..
        } if source.card_db_id == 494 => {
            return Ok(Some(format!(
                "Choose {}: cannot be blocked this turn; then draw and discard",
                h.name(object)?
            )))
        }
        ActionSemanticV1::FinishTargetSelection { source, .. }
        | ActionSemanticV1::FinishEffectSelection { source, .. }
            if source.card_db_id == 494 =>
        {
            return Ok(Some(
                "Choose no Kaito target; then draw and privately discard one".into(),
            ))
        }
        _ => {}
    }
    if let Some(c) = crate::chandra_public_v74::activation_cost(&obs.chandra, a)
        .map_err(|_| Error::InvalidVisibleReference)?
    {
        let delta = [2, 1, -4][usize::from(c.ability_index)];
        let effect=["add three red mana; exile the top three cards and choose one to play this turn with ordinary costs","copy a creature you control with copiable haste and an end-step sacrifice ability","divide eight damage among zero through eight creatures or planeswalkers before payment"][usize::from(c.ability_index)];
        return Ok(Some(format!(
            "Activate {}: {delta:+} loyalty ({} to {}); {effect}",
            h.name(&c.source)?,
            c.before,
            c.after
        )));
    }
    match a {
        ActionSemanticV1::ChooseTarget {
            source,
            target: TargetRefV1::Object { object },
            ..
        }
        | ActionSemanticV1::ChooseEffectTarget {
            source,
            target: TargetRefV1::Object { object },
            ..
        } if source.card_db_id == 493 => {
            if let Some(allocation) = &obs.chandra.pending_allocation {
                return Ok(Some(format!(
                    "Allocate one damage to {} ({} units remaining before payment)",
                    h.name(object)?,
                    allocation.remaining_units
                )));
            }
            if obs.chandra.exiles.iter().any(|e| e.choice_current) {
                return Ok(Some(format!(
                    "Choose {} to play this turn with ordinary costs and timing",
                    h.name(object)?
                )));
            }
            return Ok(Some(format!(
                "Copy {} with haste and an end-step sacrifice ability",
                h.name(object)?
            )));
        }
        ActionSemanticV1::FinishEffectSelection { source, .. }
            if source.card_db_id == 493 && obs.chandra.pending_allocation.is_some() =>
        {
            return Ok(Some("Choose zero damage targets and pay -4 loyalty".into()))
        }
        ActionSemanticV1::ChooseEffectOption {
            source,
            option_index,
            ..
        } if source.card_db_id == 493 && obs.chandra.pending_damage.is_some() => {
            let damage = obs
                .chandra
                .pending_damage
                .as_ref()
                .ok_or(Error::InvalidVisibleReference)?;
            let option = damage
                .options
                .iter()
                .find(|r| r.menu_index == *option_index)
                .ok_or(Error::InvalidVisibleReference)?;
            return Ok(Some(format!(
                "Apply {:?} to {} ({} damage remains)",
                option.kind,
                h.name(&stable(&damage.recipient)?)?,
                damage.remaining_amount
            )));
        }
        _ => {}
    }
    if let Some(c) = crate::liliana_public_v73::activation_cost(&obs.liliana, a)
        .map_err(|_| Error::InvalidVisibleReference)?
    {
        let delta = [1, -4, -9][usize::from(c.ability_index)];
        let effect = [
            "create a black 2/2 Zombie",
            "each player sacrifices two creatures",
            "each opponent keeps one permanent of each permanent type and sacrifices the rest",
        ][usize::from(c.ability_index)];
        return Ok(Some(format!(
            "Activate {}: {delta:+} loyalty ({} to {}); {effect}",
            h.name(&c.source)?,
            c.before,
            c.after
        )));
    }
    match a {
        ActionSemanticV1::ActivateAbility {
            source,
            ability_index: 0,
            ..
        } if source.card_db_id == 490 => {
            return Ok(Some(format!(
                "Activate {}: pay {{3}}{{B}}, tap; each opponent discards a card (sorcery only)",
                h.name(source)?
            )))
        }
        ActionSemanticV1::ChooseEffectTarget {
            source,
            target: TargetRefV1::Object { object },
            ..
        } if source.card_db_id == 491 => {
            let choices = crate::liliana_public_v73::choice_contexts(&obs.liliana);
            let [choice] = choices.as_slice() else {
                return Err(Error::InvalidVisibleReference);
            };
            if !choice
                .candidates
                .iter()
                .any(|r| r.object == object.arena_id && r.generation == object.zone_change_count)
            {
                return Err(Error::InvalidVisibleReference);
            }
            return Ok(Some(match choice.kind {
                crate::liliana_public_v73::SelectionKindV73::SacrificeCreatures => {
                    format!("Choose {} to sacrifice", h.name(object)?)
                }
                crate::liliana_public_v73::SelectionKindV73::KeepPermanent(t) => {
                    format!("Keep {} for {t:?}", h.name(object)?)
                }
                crate::liliana_public_v73::SelectionKindV73::OrderGraveyard => format!(
                    "Put {} next in its owner's graveyard order",
                    h.name(object)?
                ),
            }));
        }
        ActionSemanticV1::FinishEffectSelection { source, .. } if source.card_db_id == 491 => {
            return Ok(Some("Finish Liliana's current selection".into()))
        }
        _ => {}
    }

    if let Some(p) = crate::muldrotha_public_v71::action_permission(&obs.muldrotha, a)
        .map_err(|_| Error::InvalidVisibleReference)?
    {
        return Ok(Some(match p.permission {
            None => "Use the ordinary graveyard play route".into(),
            Some(p) => format!(
                "Use the {:?} permission from {}",
                p.permanent_type,
                h.name(&stable(&p.source.original_source)?)?
            ),
        }));
    }

    match a {
        ActionSemanticV1::CastSpell{source,..} if source.card_db_id==487=>return Ok(Some(format!("Cast {}: pay X and {{G}}{{G}}{{G}}; reveal X and choose permanent cards with mana value at most X",h.name(source)?))),
        ActionSemanticV1::ChooseEffectOption{source,option_index,..} if source.card_db_id==487=>return Ok(Some(format!("Genesis Wave: choose X={option_index}"))),
        ActionSemanticV1::ChooseFullRangeCastXValueV59{source,x_value,..} if source.card_db_id==487=>return Ok(Some(format!("Genesis Wave: choose X={x_value}"))),
        ActionSemanticV1::ChooseEffectTarget{source,target:TargetRefV1::Object{object},..} if source.card_db_id==487=>return Ok(Some(format!("Genesis Wave: choose {}",h.name(object)?))),
        ActionSemanticV1::FinishEffectSelection{source,..} if source.card_db_id==487=>return Ok(Some("Genesis Wave: finish the current revealed-card selection".into())),
        _=>{}
    }
    if let ActionSemanticV1::ChooseEffectBoolean {
        actor,
        source,
        value,
    } = a
    {
        let matching = obs
            .graveyard_return
            .triggers
            .iter()
            .filter(|r| {
                r.stage == public::StageV61::Resolving
                    && r.source.captured_controller == *actor
                    && stable(&r.source.original_source).is_ok_and(|s| s == *source)
                    && matches!(
                        r.program,
                        public::ProgramV61::AleshaRaid {
                            may_return_at_resolution: true,
                            optional_choice: public::AleshaChoiceV61::AwaitingCostlessReturnChoice,
                            ..
                        }
                    )
            })
            .collect::<Vec<_>>();
        if matching.len() == 1 {
            let [target] = matching[0].targets.as_slice() else {
                return Err(Error::InvalidVisibleReference);
            };
            return Ok(Some(format!(
                "{} returning {} with {} (no mana payment)",
                if *value { "Accept" } else { "Decline" },
                h.name(&stable(&target.original)?)?,
                h.name(source)?
            )));
        }
    }
    super::prepared_v57::choice_label_kaito(obs, a, h)
}
pub(super) fn project_with_handles(
    decision: &KaitoDecisionV75,
    human: PlayerSeatV1,
    prompt_seq: u64,
) -> Result<(HumanDecisionKaitoV75, Vec<u32>, Handles), Error> {
    let obs = decision.observation();
    if obs.graveyard_return.viewer != human || obs.common.common.acting_player != human {
        return Err(Error::NotHumanTurn);
    }
    crate::kaito_candidates_v75::validate_artifact(decision.candidates_v75())
        .map_err(|_| Error::InvalidVisibleReference)?;
    let (common, actions, indexes, mut handles) =
        super::ravenous_v51::project_kaito_base_with_handles(
            decision.candidates_v75(),
            decision.actions(),
            human,
            decision.family_projections_v75(),
        )?;
    let prepared = super::prepared_v57::facts_parts(
        &obs.drake,
        &obs.spinner,
        &obs.quandary,
        &obs.phoenix,
        &mut handles,
        false,
    )?;
    let next_five = super::next_five_v59::register(&obs.next_five, &mut handles)?;
    let graveyard_return = inherited_facts(&obs.graveyard_return, &mut handles)?;
    let call = call_facts(&obs.call, &mut handles)?;
    let end_turn = end_turn_facts(&obs.end_turn, &mut handles)?;
    let genesis_wave = genesis_wave_facts(&obs.genesis_wave, &mut handles)?;
    let vivien = loyalty_facts(&obs.vivien, &mut handles)?;
    let muldrotha = muldrotha_facts(&obs.muldrotha, &mut handles)?;
    let (tinybones, mut roles) =
        super::tinybones_v72::tinybones_facts_with_roles(&obs.tinybones, &mut handles)?;
    let liliana = liliana_facts(
        &obs.liliana,
        &tinybones.battlefield,
        &mut handles,
        &mut roles,
    )?;
    let chandra = chandra_facts(
        &obs.chandra,
        &tinybones.battlefield,
        &mut handles,
        &mut roles,
    )?;
    let kaito = kaito_facts(&obs.kaito, &tinybones.battlefield, &mut handles, &mut roles)?;
    let artist = obs
        .artist
        .as_ref()
        .map(|v| artist_facts(v, &mut handles))
        .transpose()?;
    let chosen_type = obs
        .chosen_type
        .as_ref()
        .map(|facts| super::chosen_type_v1::project(facts, &mut handles))
        .transpose()?;
    let niv = obs
        .niv
        .as_ref()
        .map(|v| niv_facts(v, &mut handles, &mut roles))
        .transpose()?;
    let drakuseth = obs
        .drakuseth
        .as_ref()
        .map(|facts| super::drakuseth_consumer_v1::capture(facts, &mut handles, &mut roles))
        .transpose()?;
    let embercleave = obs
        .embercleave
        .as_ref()
        .map(|facts| super::embercleave_consumer_v1::project(facts, &mut handles, &mut roles))
        .transpose()?;
    let zimone = obs
        .zimone
        .as_ref()
        .map(|facts| super::zimone_consumer_v1::project(facts, &mut handles, &mut roles))
        .transpose()?;
    let omniscience = obs
        .omniscience
        .as_ref()
        .map(|facts| super::omniscience_consumer_v1::project(facts, &mut handles))
        .transpose()?;
    let tutelage = obs
        .tutelage
        .as_ref()
        .map(|facts| super::tutelage_consumer_v1::project(facts, &mut handles))
        .transpose()?;
    let lathril = obs
        .lathril
        .as_ref()
        .map(|facts| super::lathril_consumer_v1::project(facts, &mut handles, &mut roles))
        .transpose()?;
    let greatwurm = obs.greatwurm.as_ref().map(|facts| super::greatwurm_consumer_v1::project(facts, &mut handles, &mut roles)).transpose()?;
    let sphinx_lore = obs.sphinx_lore.as_ref().map(|facts| super::sphinx_lore_consumer_v1::project(facts, &mut handles, &mut roles)).transpose()?;
    let kykar = obs.kykar.as_ref().map(|facts| super::kykar_consumer_v1::project(facts, &mut handles, &mut roles)).transpose()?;
    let loot = obs.loot.as_ref().map(|facts| super::loot_consumer_v1::project(facts, &mut handles)).transpose()?;
    let edict = obs.edict.as_ref()
        .map(|facts| super::edict_consumer_v1::project(facts, &mut handles, &mut roles))
        .transpose()?;
    let leyline_axe = obs
        .leyline_axe
        .as_ref()
        .map(|facts| super::leyline_axe_consumer_v1::project(facts, &mut handles, &mut roles))
        .transpose()?;
    let moon = obs
        .moon
        .as_ref()
        .map(|facts| super::moon_consumer_v1::project(facts, &mut handles, &mut roles))
        .transpose()?;
    let progenitus = obs
        .progenitus
        .as_ref()
        .map(|facts| super::progenitus_consumer_v1::project(facts, &mut handles))
        .transpose()?;
    let aberration = obs
        .aberration
        .as_ref()
        .map(|facts| super::aberration_consumer_v1::project(facts, &mut handles))
        .transpose()?;
    let electroduplicate = obs
        .electroduplicate
        .as_ref()
        .map(|facts| super::electroduplicate_consumer_v1::project(facts, &mut handles, &mut roles))
        .transpose()?;
    let curator = obs.curator.as_ref()
        .map(|facts| super::curator_consumer_v1::project(facts, &mut handles, &mut roles))
        .transpose()?;
    let action_meanings = indexes
        .iter()
        .map(|i| {
            let r = decision
                .candidates_v75()
                .candidates
                .get(*i as usize)
                .ok_or(Error::InvalidAction)?;
            Ok(HumanKaitoCandidateV75 {
                artist_payment: r.artist_payment,
                chosen_type_answer: r.chosen_type_answer,
                drakuseth_target: r.drakuseth_target,
                embercleave_action: r.embercleave_action,
                zimone_action: r.zimone_action,
                omniscience_action: r.omniscience_action,
                tutelage_action: r.tutelage_action,
                moon_action: r.moon_action,
                leyline_axe_action: r.leyline_axe_action,
                progenitus_action: r.progenitus_action,
                aberration_action: r.aberration_action,
                lathril_action: r.lathril_action,
                electroduplicate_action: r.electroduplicate_action,
                edict_action: r.edict_action,
                loot_action: r.loot_action,
                curator_action: r.curator_action,
                kykar_action: r.kykar_action,
                sphinx_lore_action: r.sphinx_lore_action,
                greatwurm_action: r.greatwurm_action,
                tinybones: super::tinybones_v72::HumanTinybonesActionV72 {
                    triggers: tinybones.triggers.clone(),
                    plays: tinybones.plays.clone(),
                },
                liliana: HumanLilianaActionV73 {
                    choices: liliana
                        .resolutions
                        .iter()
                        .flat_map(|r| r.selections.iter().filter(|s| s.current).cloned())
                        .collect(),
                    activation: r
                        .liliana_activation
                        .as_ref()
                        .map(|c| {
                            Ok(HumanLoyaltyCostV71 {
                                source: handles.get(&c.source)?.clone(),
                                ability_index: c.ability_index,
                                before: c.before,
                                after: c.after,
                                paid_before_publication: c.paid_before_publication,
                                countering_refunds_cost: c.countering_refunds_cost,
                            })
                        })
                        .transpose()?,
                },
                kaito: HumanKaitoActionV75 {
                    activation: r
                        .kaito_activation
                        .as_ref()
                        .map(|c| {
                            Ok::<_, Error>(HumanLoyaltyCostV71 {
                                source: handles.get(&c.source)?.clone(),
                                ability_index: c.ability_index,
                                before: c.before,
                                after: c.after,
                                paid_before_publication: c.paid_before_publication,
                                countering_refunds_cost: c.countering_refunds_cost,
                            })
                        })
                        .transpose()?,
                    activations: kaito.activations.clone(),
                    triggers: kaito.triggers.clone(),
                    selected_target: match &r.action {
                        ActionSemanticV1::ChooseTarget {
                            source,
                            target: crate::rl::TargetRefV1::Object { object },
                            ..
                        }
                        | ActionSemanticV1::ChooseEffectTarget {
                            source,
                            target: crate::rl::TargetRefV1::Object { object },
                            ..
                        } if source.card_db_id == 494 => Some(handles.get(object)?.clone()),
                        _ => None,
                    },
                    selected_discard: match &r.action {
                        ActionSemanticV1::Discard { cards, .. }
                            if kaito.activations.iter().any(|a| {
                                a.stage == crate::kaito_public_v75::StageV75::WaitingDiscard
                            }) =>
                        {
                            Some(
                                cards
                                    .iter()
                                    .map(|c| handles.get(c).cloned())
                                    .collect::<Result<_, _>>()?,
                            )
                        }
                        _ => None,
                    },
                },
                chandra: HumanChandraActionV74 {
                    selected_damage_option: match &r.action {
                        ActionSemanticV1::ChooseEffectOption {
                            source,
                            option_index,
                            ..
                        } if source.card_db_id == 493 && obs.chandra.pending_damage.is_some() => {
                            Some(*option_index)
                        }
                        _ => None,
                    },
                    selected_allocation_target: match &r.action {
                        ActionSemanticV1::ChooseEffectTarget {
                            source,
                            target: crate::rl::TargetRefV1::Object { object },
                            ..
                        }
                        | ActionSemanticV1::ChooseTarget {
                            source,
                            target: crate::rl::TargetRefV1::Object { object },
                            ..
                        } if source.card_db_id == 493
                            && obs.chandra.pending_allocation.is_some() =>
                        {
                            Some(handles.get(object)?.clone())
                        }
                        _ => None,
                    },
                    selected_exile: match &r.action {
                        ActionSemanticV1::ChooseEffectTarget {
                            source,
                            target: crate::rl::TargetRefV1::Object { object },
                            ..
                        } if source.card_db_id == 493 => obs
                            .chandra
                            .exiles
                            .iter()
                            .find(|e| e.choice_current)
                            .and_then(|e| {
                                e.cards.iter().position(|c| {
                                    c.original.object == object.arena_id
                                        && c.original.generation == object.zone_change_count
                                })
                            })
                            .map(|i| u16::try_from(i).map_err(|_| Error::InvalidVisibleReference))
                            .transpose()?,
                        _ => None,
                    },
                    copy_target: match &r.action {
                        ActionSemanticV1::ChooseTarget {
                            source,
                            target: crate::rl::TargetRefV1::Object { object },
                            ..
                        } if source.card_db_id == 493
                            && obs.chandra.pending_allocation.is_none() =>
                        {
                            Some(handles.get(object)?.clone())
                        }
                        _ => None,
                    },

                    activation: r
                        .chandra_activation
                        .as_ref()
                        .map(|c| {
                            Ok(HumanLoyaltyCostV71 {
                                source: handles.get(&c.source)?.clone(),
                                ability_index: c.ability_index,
                                before: c.before,
                                after: c.after,
                                paid_before_publication: c.paid_before_publication,
                                countering_refunds_cost: c.countering_refunds_cost,
                            })
                        })
                        .transpose()?,
                    allocation: chandra.pending_allocation.clone(),
                    exiles: chandra
                        .exiles
                        .iter()
                        .filter(|e| e.choice_current)
                        .cloned()
                        .collect(),
                    damage: chandra.pending_damage.clone(),
                },
                muldrotha_permission: r
                    .muldrotha_permission
                    .as_ref()
                    .map(|a| {
                        Ok::<_, Error>(HumanMuldrothaPermissionActionV71 {
                            occurrence: a.occurrence,
                            menu_index: a.menu_index,
                            permission: a
                                .permission
                                .as_ref()
                                .map(|p| permission(p, &mut handles))
                                .transpose()?,
                        })
                    })
                    .transpose()?,
                vivien_choices: r.vivien_choices.clone(),
                vivien_activation: r
                    .vivien_activation
                    .as_ref()
                    .map(|c| {
                        Ok(HumanLoyaltyCostV71 {
                            source: handles.get(&c.source)?.clone(),
                            ability_index: c.ability_index,
                            before: c.before,
                            after: c.after,
                            paid_before_publication: c.paid_before_publication,
                            countering_refunds_cost: c.countering_refunds_cost,
                        })
                    })
                    .transpose()?,
                genesis_waves: r
                    .genesis_waves
                    .iter()
                    .map(|w| {
                        Ok(HumanWaveContextV71 {
                            occurrence: w.occurrence,
                            stage: w.stage,
                            source: handles.get(&w.source)?.clone(),
                            x_value: w.x_value,
                        })
                    })
                    .collect::<Result<_, Error>>()?,
                call_returns: call.returns.clone(),
                end_turn_cleanup: end_turn.cleanup.clone(),
                disposed_stack_outcomes: end_turn.disposals.clone(),
                compatibility_terminals: end_turn.compatibility_terminals.clone(),
                drake_activation: r
                    .drake_activation
                    .as_ref()
                    .map(|v| {
                        Ok(super::next_five_v59::HumanNextFiveActivationV59 {
                            source: handles.get(&v.source)?.clone(),
                            kind: v.kind,
                            count: v.count,
                            before: v.before,
                            after: v.after,
                            any_priority: v.any_priority,
                            costs_paid_before_stack: v.costs_paid_before_stack,
                            countering_refunds_costs: v.countering_refunds_costs,
                        })
                    })
                    .transpose()?,
                programs: r
                    .programs
                    .iter()
                    .map(|r| trigger(r, &mut handles))
                    .collect::<Result<_, Error>>()?,
                selected_graveyard_target: r
                    .selected_graveyard_target
                    .as_ref()
                    .map(|r| target(r, &mut handles))
                    .transpose()?,
                nine_lives_deaths: r
                    .nine_lives_deaths
                    .iter()
                    .map(|r| death(r, &mut handles))
                    .collect::<Result<_, Error>>()?,
                nine_lives_schedules: r
                    .nine_lives_schedules
                    .iter()
                    .map(|r| schedule(r, &mut handles))
                    .collect::<Result<_, Error>>()?,
            })
        })
        .collect::<Result<_, Error>>()?;
    Ok((
        HumanDecisionKaitoV75 {
            schema: if obs.schema == crate::limited_mixed_consumer_v1::OBSERVATION_SCHEMA {
                "mtg-kernel-human-mixed-fdn-decision/v1"
            } else if chosen_type.is_some() {
                "mtg-kernel-human-chosen-type-decision/v1"
            } else if drakuseth.is_some() {
                "mtg-kernel-human-drakuseth-decision/v1"
            } else if artist.is_some() {
                "mtg-kernel-human-artist-decision/v1"
            } else if niv.is_some() {
                "mtg-kernel-human-niv-decision/v1"
            } else if embercleave.is_some() {
                "mtg-kernel-human-embercleave-decision/v1"
            } else if zimone.is_some() {
                "mtg-kernel-human-zimone-decision/v1"
            } else if omniscience.is_some() {
                "mtg-kernel-human-omniscience-decision/v1"
            } else if tutelage.is_some() {
                "mtg-kernel-human-tutelage-decision/v1"
            } else if moon.is_some() {
                "mtg-kernel-human-moon-decision/v1"
            } else if leyline_axe.is_some() {
                "mtg-kernel-human-leyline-axe-decision/v1"
            } else if progenitus.is_some() {
                "mtg-kernel-human-progenitus-decision/v1"
            } else if aberration.is_some() {
                "mtg-kernel-human-aberration-decision/v1"
            } else if lathril.is_some() {
                "mtg-kernel-human-lathril-decision/v1"
            } else if electroduplicate.is_some() {
                "mtg-kernel-human-electroduplicate-decision/v1"
            } else if greatwurm.is_some() {
                "mtg-kernel-human-greatwurm-decision/v1"
            } else if sphinx_lore.is_some() {
                "mtg-kernel-human-sphinx-lore-decision/v1"
            } else if kykar.is_some() {
                "mtg-kernel-human-kykar-decision/v1"
            } else if loot.is_some() {
                "mtg-kernel-human-loot-decision/v1"
            } else if edict.is_some() {
                "mtg-kernel-human-edict-decision/v1"
            } else if curator.is_some() {
                "mtg-kernel-human-curator-decision/v1"
            } else {
                "mtg-kernel-human-kaito-decision/v75"
            }
            .into(),
            human_seat: human,
            prompt_seq,
            state: HumanVisibleStateKaitoV75 {
                common,
                prepared,
                next_five,
                graveyard_return,
                call,
                end_turn,
                genesis_wave,
                vivien,
                muldrotha,
                tinybones,
                liliana,
                chandra,
                kaito,
                artist,
                chosen_type,
                niv,
                drakuseth,
                embercleave,
                zimone,
                omniscience,
                tutelage,
                moon,
                leyline_axe,
                progenitus,
                aberration,
                lathril,
                electroduplicate,
                edict,
                loot,
                curator,
                kykar,
                sphinx_lore,
                greatwurm,
            },
            actions,
            action_meanings,
        },
        indexes,
        handles,
    ))
}
fn project(
    decision: &KaitoDecisionV75,
    human: PlayerSeatV1,
    prompt_seq: u64,
) -> Result<(HumanDecisionKaitoV75, Vec<u32>), Error> {
    project_with_handles(decision, human, prompt_seq)
        .map(|(visible, indexes, _)| (visible, indexes))
}
#[derive(Clone)]
struct AuthorityV75 {
    owner: Arc<()>,
    generation: u64,
}
impl PartialEq for AuthorityV75 {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && Arc::ptr_eq(&self.owner, &other.owner)
    }
}
impl Eq for AuthorityV75 {}

pub struct HumanKaitoSessionV75 {
    inner: KaitoSessionV75,
    authority: AuthorityV75,
    human_seat: PlayerSeatV1,
}
#[derive(Clone)]
pub struct HumanKaitoSnapshotV75 {
    inner: KaitoSnapshotV75,
    owner: Arc<()>,
}
impl HumanKaitoSessionV75 {
    pub fn from_session(
        inner: FastActorSessionV1,
        human_seat: PlayerSeatV1,
    ) -> Result<Self, RlContractError> {
        Ok(Self {
            inner: KaitoSessionV75::from_session(inner)?,
            human_seat,
            authority: AuthorityV75 {
                owner: Arc::new(()),
                generation: 0,
            },
        })
    }
    pub fn reset(
        episode_id: u64,
        env_seed: u64,
        limits: (u64, u64),
        deck_ids: crate::rl_session::SessionDeckIdsV1,
        mainboards: [Vec<u16>; 2],
        starting_player: crate::ids::PlayerId,
        human_seat: PlayerSeatV1,
    ) -> Result<Self, crate::rl_session::RlSessionError> {
        let inner = KaitoSessionV75::reset_explicit_mainboards_v75(
            episode_id,
            env_seed,
            limits,
            deck_ids,
            mainboards,
            starting_player,
        )?;
        Ok(Self {
            inner,
            human_seat,
            authority: AuthorityV75 {
                owner: Arc::new(()),
                generation: 0,
            },
        })
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.response()
    }
    pub fn snapshot_v75(&self) -> HumanKaitoSnapshotV75 {
        HumanKaitoSnapshotV75 {
            inner: self.inner.snapshot_v75(),
            owner: self.authority.owner.clone(),
        }
    }
    pub fn restore_v75(&mut self, snapshot: &HumanKaitoSnapshotV75) -> Result<(), Error> {
        if !Arc::ptr_eq(&self.authority.owner, &snapshot.owner) {
            return Err(Error::StaleDecision);
        }
        let generation = self
            .authority
            .generation
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        self.inner
            .restore_v75(&snapshot.inner)
            .map_err(|_| Error::StaleDecision)?;
        self.authority.generation = generation;
        Ok(())
    }
    fn decision(&self, expected: FastActorDecisionV1) -> Result<KaitoDecisionV75, Error> {
        if expected.acting_player != self.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let decision = self
            .inner
            .decision_v75()
            .map_err(|_| Error::StaleDecision)?;
        if decision.expected() != expected {
            return Err(Error::StaleDecision);
        }
        self.inner
            .validate_decision_v75(&decision)
            .map_err(|_| Error::StaleDecision)?;
        Ok(decision)
    }
    pub fn public_observation(
        &self,
        expected: FastActorDecisionV1,
    ) -> Result<observation::KaitoObservationV75, Error> {
        Ok(self.decision(expected)?.observation().clone())
    }
    /// Host-only opponent callback. It receives the actor-authorized public
    /// observation and legal actions, not a Human DTO or opponent private data.
    pub fn step_opponent_with<F>(
        &mut self,
        expected: FastActorDecisionV1,
        mut policy: F,
    ) -> Result<FastActorResponseV1, RlContractError>
    where
        F: FnMut(
            &observation::KaitoObservationV75,
            &[ActionSemanticV1],
        ) -> Result<u32, RlContractError>,
    {
        if expected.acting_player == self.human_seat {
            return Err(RlContractError(
                "opponent callback cannot act for Human seat".into(),
            ));
        }
        let decision = self
            .inner
            .decision_v75()
            .map_err(|_| RlContractError("stale Call opponent decision".into()))?;
        if decision.expected() != expected {
            return Err(RlContractError("stale Call opponent decision".into()));
        }
        self.inner.validate_decision_v75(&decision)?;
        let index = policy(decision.observation(), decision.actions())?;
        self.inner.step_v75(&decision, index)
    }
}

struct BoundKaitoV75 {
    authority: AuthorityV75,
    decision: KaitoDecisionV75,
    visible: HumanDecisionKaitoV75,
    indexes: Vec<u32>,
}
pub struct HumanKaitoProjectorV75 {
    human_seat: PlayerSeatV1,
    next_prompt_seq: u64,
    pending: Option<BoundKaitoV75>,
    accepted: Option<(HumanActionReceiptV1, AuthorityV75)>,
}
impl HumanKaitoProjectorV75 {
    pub fn new(human_seat: PlayerSeatV1) -> Self {
        Self {
            human_seat,
            next_prompt_seq: 1,
            pending: None,
            accepted: None,
        }
    }
    pub fn project_current(
        &mut self,
        session: &HumanKaitoSessionV75,
        expected: FastActorDecisionV1,
    ) -> Result<HumanDecisionKaitoV75, Error> {
        if expected.acting_player != self.human_seat || session.human_seat != self.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let decision = session.decision(expected)?;
        if let Some(pending) = &self.pending {
            if pending.authority == session.authority
                && pending.decision.expected() == expected
                && session
                    .inner
                    .validate_decision_v75(&pending.decision)
                    .is_ok()
            {
                let (visible, indexes) =
                    project(&decision, self.human_seat, pending.visible.prompt_seq)?;
                if visible == pending.visible && indexes == pending.indexes {
                    return Ok(visible);
                }
            }
        }
        let next = self
            .next_prompt_seq
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        let (visible, indexes) = project(&decision, self.human_seat, self.next_prompt_seq)?;
        self.pending = Some(BoundKaitoV75 {
            authority: session.authority.clone(),
            decision,
            visible: visible.clone(),
            indexes,
        });
        self.next_prompt_seq = next;
        Ok(visible)
    }
    pub fn submit(
        &mut self,
        session: &mut HumanKaitoSessionV75,
        request: HumanActionRequestV1,
    ) -> Result<HumanActionReceiptV1, Error> {
        if session.human_seat != self.human_seat {
            return Err(Error::NotHumanTurn);
        }
        if let Some((receipt, authority)) = &self.accepted {
            if receipt.prompt_seq == request.prompt_seq {
                if *authority != session.authority {
                    return Err(Error::StaleDecision);
                }
                return if receipt.action_index == request.action_index {
                    Ok(receipt.clone())
                } else {
                    Err(Error::ConflictingRetry)
                };
            }
        }
        let pending = self.pending.as_ref().ok_or(Error::StaleDecision)?;
        if pending.authority != session.authority
            || request.prompt_seq != pending.visible.prompt_seq
        {
            return Err(Error::StaleDecision);
        }
        if request.action_index as usize >= pending.visible.actions.len() {
            return Err(Error::InvalidAction);
        }
        session
            .inner
            .validate_decision_v75(&pending.decision)
            .map_err(|_| Error::StaleDecision)?;
        let engine_index = *pending
            .indexes
            .get(request.action_index as usize)
            .ok_or(Error::InvalidAction)?;
        session.inner.step_with_current_check(
            &pending.decision,
            engine_index,
            |_, _| {
                let (visible, indexes) = project(
                    &pending.decision,
                    self.human_seat,
                    pending.visible.prompt_seq,
                )?;
                if visible != pending.visible || indexes != pending.indexes {
                    return Err(Error::StaleDecision);
                }
                Ok(())
            },
            |_| Error::StaleDecision,
            |_| Error::ExecutionFailed,
        )?;
        self.pending = None;
        let receipt = HumanActionReceiptV1 {
            prompt_seq: request.prompt_seq,
            action_index: request.action_index,
        };
        self.accepted = Some((receipt.clone(), session.authority.clone()));
        Ok(receipt)
    }
}

pub type HumanKaitoRootV75 = HumanLilianaRootV73;
record!(HumanKaitoTargetV75 {original:HumanTinybonesRelationV72,current_original:Option<HumanCardRefV1>,legal_now:bool});
record!(HumanKaitoDiscardV75 {count:u32,answered:bool,private_choices:Option<Vec<HumanCardRefV1>>,private_chosen:Option<Vec<HumanTinybonesRelationV72>>});
record!(HumanKaitoEvasionV75 {target:Option<HumanTinybonesRelationV72>,active_on_original:bool});
record!(HumanKaitoActivationV75 {occurrence:u32,source:HumanKaitoRootV75,original_turn:u32,ability_index:u8,loyalty_before:u32,loyalty_after:u32,targets:Vec<HumanKaitoTargetV75>,stage:crate::kaito_public_v75::StageV75,draw:crate::kaito_public_v75::DrawV75,discard:Option<HumanKaitoDiscardV75>,evasion:Option<HumanKaitoEvasionV75>});
record!(HumanKaitoCombatV75 {occurrence:u32,observer:HumanKaitoRootV75,original_creature:HumanTinybonesRelationV72,current_original_creature:Option<HumanCardRefV1>,damaged_player:PlayerSeatV1,final_damage:i32});
record!(HumanKaitoCastV75 {occurrence:u32,emblem:u32,actual_cast:u32,original_spell:HumanTinybonesRelationV72,current_original_spell:Option<HumanCardRefV1>,x_value:u16,kicked:bool,cast_method:crate::state::CastMethodV4});
record!(HumanKaitoTriggerV75 {occurrence:u32,origin:crate::kaito_public_v75::OriginV75,source:HumanKaitoRootV75,stage:crate::kaito_public_v75::StageV75,native_position:Option<u32>});
record!(HumanKaitoNinjaV75 {original:HumanCardRefV1,current_original:Option<HumanCardRefV1>});
record!(HumanKaitoNinjaBatchV75 {occurrence:u32,publication:crate::kaito_public_v75::PublicationV75,controller:PlayerSeatV1,created:Vec<HumanKaitoNinjaV75>});
record!(HumanKaitoFactsV75 {viewer:PlayerSeatV1,active_player:PlayerSeatV1,turn_key:u32,meaning:crate::rules_meaning_kaito_v75::MeaningV75,authorized_printings:Vec<HumanCardRefV1>,battlefield:[Vec<HumanTinybonesCurrentV72>;2],activations:Vec<HumanKaitoActivationV75>,combats:Vec<HumanKaitoCombatV75>,emblems:Vec<crate::kaito_public_v75::EmblemV75>,casts:Vec<HumanKaitoCastV75>,triggers:Vec<HumanKaitoTriggerV75>,ninja_batches:Vec<HumanKaitoNinjaBatchV75>});
record!(HumanKaitoActionV75 {activation:Option<HumanLoyaltyCostV71>,activations:Vec<HumanKaitoActivationV75>,triggers:Vec<HumanKaitoTriggerV75>,selected_target:Option<HumanCardRefV1>,selected_discard:Option<Vec<HumanCardRefV1>>});
fn kaito_facts(
    v: &crate::kaito_public_v75::FactsV75,
    battlefield: &[Vec<HumanTinybonesCurrentV72>; 2],
    h: &mut Handles,
    roles: &mut Incarnations,
) -> Result<HumanKaitoFactsV75, Error> {
    crate::kaito_public_v75::validate(v).map_err(|_| Error::InvalidVisibleReference)?;
    let authorized_printings = v
        .authorized_printings
        .iter()
        .map(|r| href(r, h))
        .collect::<Result<_, _>>()?;
    let activations = v
        .activations
        .iter()
        .map(|a| {
            Ok::<_, Error>(HumanKaitoActivationV75 {
                occurrence: a.occurrence,
                source: liliana_root(&a.source, h, roles)?,
                original_turn: a.original_turn,
                ability_index: a.ability_index,
                loyalty_before: a.loyalty_before,
                loyalty_after: a.loyalty_after,
                targets: a
                    .targets
                    .iter()
                    .map(|t| {
                        Ok::<_, Error>(HumanKaitoTargetV75 {
                            original: roles.relation(&t.original)?,
                            current_original: t
                                .current_original
                                .as_ref()
                                .map(|r| href(r, h))
                                .transpose()?,
                            legal_now: t.legal_now,
                        })
                    })
                    .collect::<Result<_, _>>()?,
                stage: a.stage,
                draw: a.draw,
                discard: a
                    .discard
                    .as_ref()
                    .map(|d| {
                        Ok::<_, Error>(HumanKaitoDiscardV75 {
                            count: d.count,
                            answered: d.answered,
                            private_choices: d
                                .private_choices
                                .as_ref()
                                .map(|rs| {
                                    rs.iter().map(|r| href(r, h)).collect::<Result<Vec<_>, _>>()
                                })
                                .transpose()?,
                            private_chosen: d
                                .private_chosen
                                .as_ref()
                                .map(|rs| {
                                    rs.iter()
                                        .map(|r| roles.relation(r))
                                        .collect::<Result<Vec<_>, _>>()
                                })
                                .transpose()?,
                        })
                    })
                    .transpose()?,
                evasion: a
                    .evasion
                    .as_ref()
                    .map(|e| {
                        Ok::<_, Error>(HumanKaitoEvasionV75 {
                            target: e.target.as_ref().map(|r| roles.relation(r)).transpose()?,
                            active_on_original: e.active_on_original,
                        })
                    })
                    .transpose()?,
            })
        })
        .collect::<Result<_, _>>()?;
    let combats = v
        .combats
        .iter()
        .map(|c| {
            Ok::<_, Error>(HumanKaitoCombatV75 {
                occurrence: c.occurrence,
                observer: liliana_root(&c.observer, h, roles)?,
                original_creature: roles.relation(&c.original_creature)?,
                current_original_creature: c
                    .current_original_creature
                    .as_ref()
                    .map(|r| href(r, h))
                    .transpose()?,
                damaged_player: c.damaged_player,
                final_damage: c.final_damage,
            })
        })
        .collect::<Result<_, _>>()?;
    let casts = v
        .casts
        .iter()
        .map(|c| {
            Ok::<_, Error>(HumanKaitoCastV75 {
                occurrence: c.occurrence,
                emblem: c.emblem,
                actual_cast: c.actual_cast,
                original_spell: roles.relation(&c.original_spell)?,
                current_original_spell: c
                    .current_original_spell
                    .as_ref()
                    .map(|r| href(r, h))
                    .transpose()?,
                x_value: c.x_value,
                kicked: c.kicked,
                cast_method: c.cast_method,
            })
        })
        .collect::<Result<_, _>>()?;
    let triggers = v
        .triggers
        .iter()
        .map(|t| {
            Ok::<_, Error>(HumanKaitoTriggerV75 {
                occurrence: t.occurrence,
                origin: t.origin,
                source: liliana_root(&t.source, h, roles)?,
                stage: t.stage,
                native_position: t.native_position,
            })
        })
        .collect::<Result<_, _>>()?;
    let ninja_batches = v
        .ninja_batches
        .iter()
        .map(|b| {
            Ok::<_, Error>(HumanKaitoNinjaBatchV75 {
                occurrence: b.occurrence,
                publication: b.publication,
                controller: b.controller,
                created: b
                    .created
                    .iter()
                    .map(|n| {
                        Ok::<_, Error>(HumanKaitoNinjaV75 {
                            original: href(&n.original, h)?,
                            current_original: n
                                .current_original
                                .as_ref()
                                .map(|r| href(r, h))
                                .transpose()?,
                        })
                    })
                    .collect::<Result<_, _>>()?,
            })
        })
        .collect::<Result<_, _>>()?;
    Ok(HumanKaitoFactsV75 {
        viewer: v.viewer,
        active_player: v.active_player,
        turn_key: v.turn_key,
        meaning: v.meaning.clone(),
        authorized_printings,
        battlefield: battlefield.clone(),
        activations,
        combats,
        emblems: v.emblems.clone(),
        casts,
        triggers,
        ninja_batches,
    })
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
pub(crate) mod tests;
