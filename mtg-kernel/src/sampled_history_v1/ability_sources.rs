use super::{fingerprint, CustodyV1};
use crate::state::{AbilitySourceContractV4, GameState, StackItem, StackItemKind, Zone};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct AbilitySourceProofV1 {
    item: [u8; 32],
    source: AbilitySourceContractV4,
}

fn eligible(state: &GameState, item: &StackItem, contract: AbilitySourceContractV4) -> bool {
    if !matches!(item.kind, StackItemKind::ActivatedAbility | StackItemKind::TriggeredAbility)
        || item.v4.stack_item_id == Default::default()
        || item.v4.ability_source_contract != Some(contract)
        || item.source != contract.source
        || item.controller != contract.controller
        // Hidden printed sources and externally granted abilities have
        // independent visibility and payment provenance. Keep those strict.
        || !matches!(contract.zone, Zone::Battlefield | Zone::Graveyard | Zone::Command)
        || item.v4.hidden_ability_source.is_some()
        || item.v4.granted_by.is_some()
        || !item.v4.cauldron_grant.is_empty()
    {
        return false;
    }
    state.objects.try_get(contract.source).is_some_and(|live| {
        live.owner == contract.owner && live.zone_change_count > contract.zone_change_count
    })
}

fn active(state: &GameState) -> impl Iterator<Item = &StackItem> {
    state.stack.iter().chain(
        state
            .engine
            .pending_effect
            .as_ref()
            .map(|pending| &pending.resolving_item)
            // A native paused resolution retains the same item on the stack.
            // Count that exact mirror once; duplicate stack rows and changed
            // same-ID continuations still reach the uniqueness check.
            .filter(|pending| !state.stack.iter().any(|item| item == *pending)),
    )
}

fn owns_active(state: &GameState, item: &StackItem) -> bool {
    let mut matching = active(state).filter(|row| row.v4.stack_item_id == item.v4.stack_item_id);
    matching.next() == Some(item) && matching.next().is_none()
}

#[cfg(feature = "limited-fdn-fixtures")]
pub(super) fn capture(state: &GameState) -> Result<Vec<AbilitySourceProofV1>, String> {
    let mut proofs = Vec::new();
    for item in active(state) {
        let Some(source) = item.v4.ability_source_contract else {
            continue;
        };
        if !eligible(state, item, source) {
            continue;
        }
        if !owns_active(state, item) {
            return Err("sampled ability source lacks unique native stack custody".into());
        }
        crate::engine::validated_stack_item_target_spec(item, state)?;
        let live = state.objects.get(source.source);
        crate::engine::validate_historical_permanent_copy_source(state, source, live)?;
        proofs.push(AbilitySourceProofV1 {
            item: fingerprint(item)?,
            source,
        });
    }
    Ok(proofs)
}

impl CustodyV1 {
    /// The original public ability face. A later sampled printing is not
    /// evidence about this exact old native row, or about any new activation.
    pub(crate) fn ability_source_face(
        &self,
        state: &GameState,
        item: &StackItem,
        contract: AbilitySourceContractV4,
    ) -> Option<u16> {
        if self.2.is_empty() || !eligible(state, item, contract) {
            return None;
        }
        let hash = fingerprint(item).ok()?;
        if !self
            .2
            .iter()
            .any(|proof| proof.item == hash && proof.source == contract)
            || !owns_active(state, item)
        {
            return None;
        }
        let live = state.objects.get(contract.source);
        crate::engine::validate_historical_permanent_copy_source(state, contract, live).ok()?;
        Some(contract.card_def)
    }
}
