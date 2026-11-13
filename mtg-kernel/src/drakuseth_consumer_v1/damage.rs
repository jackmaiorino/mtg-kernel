//! Public projection of the actual suspended mixed-recipient damage batch.
use super::*;
use crate::{
    effect::{crystal_prevention_v1 as native, EffectOptionChoicePurpose, PendingEffectChoice},
    fdn_family_source_v76 as source, optional_trigger_public_v1 as public,
    state::Target,
};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum DamageOutcomeV1 {
    Waiting,
    Prevented,
    Damage { amount: i32 },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DamageRowV1 {
    pub original_slot: u32,
    pub proposed_amount: i32,
    pub outcome: DamageOutcomeV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DamageChoiceV1 {
    pub publication: u32,
    pub chooser: PlayerSeatV1,
    pub active_original_slot: u32,
    pub original_amount: i32,
    pub remaining_amount: i32,
    /// Original damage order, not APNAP replacement-choice order.
    pub rows: Vec<DamageRowV1>,
    /// Remaining public APNAP choice order, expressed as original target slots.
    pub remaining_choice_slots: Vec<u32>,
    pub options: Vec<crate::chandra_public_v74::PreventionV74>,
}
pub(super) fn capture(
    s: &GameState,
    v: PlayerId,
    facts: &source::FactsV76,
) -> Result<Option<DamageChoiceV1>, String> {
    let Some(pending) = s
        .engine
        .pending_effect
        .as_ref()
        .filter(|p| crate::drakuseth_damage_source_v76::selected(&p.resolving_item))
    else {
        return Ok(None);
    };
    let Some(PendingEffectChoice::ChooseOption {
        purpose: EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
        ..
    }) = &pending.choice
    else {
        return Ok(None);
    };
    native::validate(s)?;
    let progress = choice
        .batch
        .as_ref()
        .ok_or("Drakuseth damage lost simultaneous batch")?;
    let capture = native::batch_capture_at(s, progress.capture, &choice.path)?;
    let [source::LaterFamilyV76::Drakuseth(f)] = facts.families.as_slice() else {
        return Err("Drakuseth family absent".into());
    };
    let rows = f
        .triggers
        .iter()
        .filter(|r| r.stage == source::StageV76::Resolving)
        .collect::<Vec<_>>();
    let [trigger] = rows.as_slice() else {
        return Err("Drakuseth resolving publication is not unique".into());
    };
    let slot = |target: Target| -> Result<u32, String> {
        // The original target vector is the validated public publication. IDs
        // are used only to bind its slot, never emitted as current hidden refs.
        let i = pending
            .resolving_item
            .targets
            .iter()
            .position(|t| *t == target)
            .ok_or("damage endpoint not in original publication")?;
        u32::try_from(i).map_err(|_| "damage slot overflow".into())
    };
    if capture.proposals.len() != progress.outcomes.len() {
        return Err("damage progress cardinality changed".into());
    }
    let rows = capture
        .proposals
        .iter()
        .zip(&progress.outcomes)
        .map(|(proposal, outcome)| {
            Ok(DamageRowV1 {
                original_slot: slot(proposal.target)?,
                proposed_amount: proposal.amount,
                outcome: match outcome {
                    native::BatchOutcomeV2::Waiting => DamageOutcomeV1::Waiting,
                    native::BatchOutcomeV2::Prevented => DamageOutcomeV1::Prevented,
                    native::BatchOutcomeV2::Damage(d) => {
                        DamageOutcomeV1::Damage { amount: d.amount }
                    }
                },
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let remaining_choice_slots = capture
        .choice_order
        .iter()
        .skip(progress.cursor)
        .map(|&i| {
            slot(
                capture
                    .proposals
                    .get(i)
                    .ok_or("damage choice index absent")?
                    .target,
            )
        })
        .collect::<Result<Vec<_>, String>>()?;
    let options = choice.options.iter().enumerate().map(|(i,p)| {
        use crate::chandra_public_v74::{PreventionKindV74 as K,PreventionV74};
        let (kind,reference,shield_remaining) = match p {
            native::PreventionV1::Multiplier(a) | native::PreventionV1::Barricade(a) => {
                let mut r = public::root(s,*a,Some(v))?.original_source;
                // Both native prevention sources were publicly active when the
                // batch was captured; current hidden contents are not consulted.
                r.identity = public::IdentityV1::Known { card_id:a.card_def };
                (if matches!(p,native::PreventionV1::Multiplier(_)) { K::Multiplier } else { K::Barricade },Some(r),None)
            }
            native::PreventionV1::Protection(b) => (K::Protection,s.objects.try_get(b.object)
                .filter(|o|o.zone==b.expected_zone && o.zone_change_count==b.expected_zone_change_count)
                .and_then(|_|public::reference(s,b.object,Some(v))),None),
            native::PreventionV1::Replacement(r) => {
                let (kind,amount) = match r.kind {
                    crate::event::ReplacementEffectKind::PreventNextDamage { remaining,.. } => (K::Shield,Some(remaining)),
                    crate::event::ReplacementEffectKind::PreventDamageFromColorUntilEndOfTurn {..} => (K::Color,None),
                    _=>return Err("Drakuseth damage contains non-noncombat replacement".into())
                };
                (kind,public::reference(s,r.source,Some(v)),amount)
            }
        };
        Ok(PreventionV74 {menu_index:u16::try_from(i).map_err(|_|"damage menu overflow")?,kind,source:reference,shield_remaining})
    }).collect::<Result<Vec<_>,String>>()?;
    Ok(Some(DamageChoiceV1 {
        publication: trigger.publication.ok_or("damage publication absent")?,
        chooser: choice.chooser.into(),
        active_original_slot: slot(choice.target)?,
        original_amount: choice.original_amount,
        remaining_amount: choice.remaining_amount,
        rows,
        remaining_choice_slots,
        options,
    }))
}
