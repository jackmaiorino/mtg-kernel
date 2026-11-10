//! Public, position-bound program labels for exact FullPool triggers.
use crate::{
    card_def::CARD_DEFS,
    effect::{EffectOp, PlayerRef},
    rl::{
        CardStableRefV1, PendingTriggerKindV2, PublicObservationProjectionV2, StackItemKindV2,
        TriggerUseSemanticV1,
    },
    state::{AbilitySourceContractV4, GameState, Zone},
};
use serde::{Deserialize, Serialize};

pub(crate) const EXEMPLAR: u16 = 218;
pub(crate) const VANGUARD: u16 = 332;
pub(crate) const COLLECTOR: u16 = 333;
pub(crate) const KIORA: u16 = 227;
pub(crate) const SYLVAN: u16 = 232;
pub(crate) const WARDENS: u16 = 361;

#[path = "modal_programs.rs"]
pub(crate) mod modal_programs;

pub(crate) fn is_printing(id: u16) -> bool {
    CARD_DEFS.get(usize::from(id)).is_some_and(|def| {
        matches!(
            (id, def.name),
            (EXEMPLAR, "Exemplar of Light")
                | (VANGUARD, "Vanguard Seraph")
                | (COLLECTOR, "Cat Collector")
                | (KIORA, "Kiora, the Rising Tide")
                | (SYLVAN, "Sylvan Scavenging")
                | (WARDENS, "Wardens of the Cycle")
        )
    })
}

fn cap(card_def: u16, ability_index: u16) -> Option<u16> {
    let ability = crate::trigger::triggers_for(card_def).get(usize::from(ability_index))?;
    crate::trigger::per_turn_trigger_cap(ability.condition)
}

/// A cap only constrains this battlefield incarnation, even if the arena
/// object has since become a private card or a different permanent.
pub(crate) fn project_live_uses(
    state: &GameState,
) -> Result<Option<Vec<TriggerUseSemanticV1>>, String> {
    let mut public = Vec::new();
    let mut previous = None;
    for entry in state.trigger_uses_v1.iter().flatten() {
        let Some(live) = state.objects.try_get(entry.source.object) else { continue };
        if entry.turn != state.turn_key_v1()
            || entry.active_player != state.active_player
            || live.zone != Zone::Battlefield
            || live.zone_change_count != entry.source.zone_change_count
            || live.v4.face_down_v1.is_some()
            || !state.players.get(live.controller.index()).is_some_and(|player| {
                player.battlefield.contains(&entry.source.object)
            })
        {
            continue;
        }
        // If a public permanent stops copying a capped ability, its old
        // slot may remain in native history without a current cap to show.
        let Some(maximum) = cap(live.card_def, entry.ability_index) else { continue };
        let key = (entry.source.object, entry.ability_index);
        if entry.uses == 0 || entry.uses > maximum || previous.is_some_and(|last| key <= last) {
            return Err("invalid FullPool native trigger cap ledger".into());
        }
        previous = Some(key);
        public.push(TriggerUseSemanticV1 {
            source: crate::rl::card_ref(state, entry.source.object).map_err(|error| error.0)?,
            ability_index: entry.ability_index,
            turn: entry.turn,
            active_player: entry.active_player.into(),
            uses: entry.uses,
        });
    }
    Ok((!public.is_empty()).then_some(public))
}

fn validate_public_uses(board: &PublicObservationProjectionV2) -> Result<(), String> {
    let Some(uses) = &board.engine_context.trigger_uses else { return Ok(()) };
    let Some(first) = uses.first() else { return Err("empty FullPool trigger cap ledger".into()) };
    let mut previous = None;
    for entry in uses {
        let key = (entry.source.arena_id, entry.ability_index);
        if entry.active_player != board.active_player
            || entry.turn != first.turn
            || entry.source.zone != Zone::Battlefield
            || !board.battlefield.iter().flatten().any(|card| card.stable == entry.source)
            || cap(entry.source.card_db_id, entry.ability_index)
                .is_none_or(|maximum| entry.uses == 0 || entry.uses > maximum)
            || previous.is_some_and(|last| key <= last)
        {
            return Err("invalid FullPool public trigger cap ledger".into());
        }
        previous = Some(key);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgramV1 {
    ExemplarCounter,
    ExemplarDraw,
    VanguardSurveil,
    CollectorFood,
    CollectorCat,
    KioraDrawDiscard,
    KioraThreshold,
    SylvanChooseMode,
    SylvanCounter,
    SylvanRaccoon,
    WardensChooseMode,
    WardensGainLife,
    WardensDrawLoseLife,
    Granted,
}

impl ProgramV1 {
    pub(crate) fn template(self) -> Option<EffectOp> {
        Some(match self {
            Self::ExemplarCounter => EffectOp::BindPlusOnePlusOneCounterToTriggerSource,
            Self::ExemplarDraw => EffectOp::DrawCards {
                player: PlayerRef::Controller,
                count: 1,
            },
            Self::VanguardSurveil => EffectOp::Surveil {
                player: PlayerRef::Controller,
                count: 1,
            },
            Self::CollectorFood | Self::CollectorCat => EffectOp::CreateToken {
                token_def: crate::card_def::card_id_by_name(self.token_name()?)?,
                controller: PlayerRef::Controller,
            },
            Self::Granted => return None,
            program => return modal_programs::template(program),
        })
    }

    pub(crate) fn token_name(self) -> Option<&'static str> {
        match self {
            Self::CollectorFood => Some("Food Token"),
            Self::CollectorCat => Some("Cat Token"),
            _ => None,
        }
    }

    fn belongs_to(self, id: u16) -> bool {
        match self {
            Self::ExemplarCounter | Self::ExemplarDraw => id == EXEMPLAR,
            Self::VanguardSurveil => id == VANGUARD,
            Self::CollectorFood | Self::CollectorCat => id == COLLECTOR,
            Self::Granted => is_printing(id),
            Self::KioraDrawDiscard | Self::KioraThreshold => id == KIORA,
            Self::SylvanChooseMode | Self::SylvanCounter | Self::SylvanRaccoon => id == SYLVAN,
            Self::WardensChooseMode | Self::WardensGainLife | Self::WardensDrawLoseLife => {
                id == WARDENS
            }
        }
    }
}

pub(crate) fn printed_program(
    source: AbilitySourceContractV4,
    effect: &EffectOp,
) -> Option<ProgramV1> {
    if source.zone != Zone::Battlefield || !is_printing(source.card_def) {
        return None;
    }
    if source.card_def == EXEMPLAR
        && matches!(effect, EffectOp::PutPlusOnePlusOneCounterOnBoundObject { object }
            if object.object == source.source && object.expected_zone == Zone::Battlefield
                && object.expected_zone_change_count == source.zone_change_count)
    {
        return Some(ProgramV1::ExemplarCounter);
    }
    [
        ProgramV1::ExemplarDraw,
        ProgramV1::VanguardSurveil,
        ProgramV1::CollectorFood,
        ProgramV1::CollectorCat,
        ProgramV1::KioraDrawDiscard,
        ProgramV1::KioraThreshold,
        ProgramV1::SylvanChooseMode,
        ProgramV1::SylvanCounter,
        ProgramV1::SylvanRaccoon,
        ProgramV1::WardensChooseMode,
        ProgramV1::WardensGainLife,
        ProgramV1::WardensDrawLoseLife,
    ]
    .into_iter()
    .find(|program| {
        program.belongs_to(source.card_def) && program.template().as_ref() == Some(effect)
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerRowV1 {
    /// Ordinal in the corresponding ordinary public stack or pending list.
    pub ordinal: u32,
    pub program: ProgramV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV1 {
    /// Whether each physical seat has already gained positive life this turn.
    pub first_positive_gain_consumed: [bool; 2],
    pub stack: Vec<TriggerRowV1>,
    pub pending: Vec<TriggerRowV1>,
}

fn bind(
    ordinal: usize,
    public: &CardStableRefV1,
    contract: Option<AbilitySourceContractV4>,
    effect: Option<&EffectOp>,
    granted: bool,
) -> Result<TriggerRowV1, String> {
    let source = contract.ok_or("life trigger is missing its native source")?;
    if public.arena_id != source.source.0
        || public.card_db_id != source.card_def
        || public.zone != source.zone
        || public.zone_change_count != source.zone_change_count
        || public.owner != source.owner.into()
        || public.controller != source.controller.into()
    {
        return Err("life trigger source does not match its public occurrence".into());
    }
    let effect = effect.ok_or("life trigger is missing its native program")?;
    let program = if granted {
        ProgramV1::Granted
    } else {
        printed_program(source, effect).ok_or("unknown native life trigger program")?
    };
    Ok(TriggerRowV1 {
        ordinal: u32::try_from(ordinal).map_err(|_| "life trigger ordinal overflow")?,
        program,
    })
}

pub(crate) fn capture(
    state: &GameState,
    board: &PublicObservationProjectionV2,
) -> Result<FactsV1, String> {
    let mut facts = FactsV1 {
        first_positive_gain_consumed: crate::life_gain_turn_v1::positive_gain_consumed(state)?,
        stack: Vec::new(),
        pending: Vec::new(),
    };
    if state.stack.len() != board.stack.len() {
        return Err("life trigger stack and public roster have different lengths".into());
    }
    for (ordinal, (native, row)) in state.stack.iter().zip(&board.stack).enumerate() {
        if row.stack_item_kind == StackItemKindV2::TriggeredAbility
            && is_printing(row.source.card_db_id)
        {
            facts.stack.push(bind(
                ordinal,
                &row.source,
                native.v4.ability_source_contract,
                native.inline_effect.as_ref(),
                native.v4.granted_by.is_some(),
            )?);
        }
    }
    let mut native_pending = state.engine.pending_triggers.iter().chain(
        state
            .pending_legend_rule_v1
            .iter()
            .flat_map(|p| p.waiting_triggers()),
    );
    for (ordinal, row) in board.engine_context.pending_triggers.iter().enumerate() {
        let native = native_pending
            .next()
            .ok_or("life trigger pending roster lost a native row")?;
        if let Some(source) = row.source.as_ref().filter(|source| {
            row.trigger_kind == PendingTriggerKindV2::TriggeredAbility
                && is_printing(source.card_db_id)
        }) {
            facts.pending.push(bind(
                ordinal,
                source,
                native.source_contract,
                Some(&native.effect),
                native.granted_by.is_some(),
            )?);
        }
    }
    if native_pending.next().is_some() {
        return Err("life trigger pending roster lost a public row".into());
    }
    Ok(facts)
}

pub(crate) fn validate(
    facts: &FactsV1,
    board: &PublicObservationProjectionV2,
) -> Result<(), String> {
    validate_public_uses(board)?;
    fn check<'a>(
        mut facts: impl Iterator<Item = &'a TriggerRowV1>,
        roster: impl Iterator<Item = (usize, u16)>,
    ) -> Result<(), String> {
        for (ordinal, id) in roster {
            if !facts.next().is_some_and(|row| {
                usize::try_from(row.ordinal) == Ok(ordinal) && row.program.belongs_to(id)
            }) {
                return Err("life trigger program and public roster disagree".into());
            }
        }
        if facts.next().is_some() {
            return Err("life trigger program has no public occurrence".into());
        }
        Ok(())
    }
    check(
        facts.stack.iter(),
        board.stack.iter().enumerate().filter_map(|(ordinal, row)| {
            (row.stack_item_kind == StackItemKindV2::TriggeredAbility
                && is_printing(row.source.card_db_id))
            .then_some((ordinal, row.source.card_db_id))
        }),
    )?;
    check(
        facts.pending.iter(),
        board
            .engine_context
            .pending_triggers
            .iter()
            .enumerate()
            .filter_map(|(ordinal, row)| {
                let id = row.source.as_ref()?.card_db_id;
                (row.trigger_kind == PendingTriggerKindV2::TriggeredAbility && is_printing(id))
                    .then_some((ordinal, id))
            }),
    )
}
