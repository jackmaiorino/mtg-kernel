//! Prompt-local Omniscience cast offers and captured permission sources.
use super::{
    muldrotha_v71::{href, root, HumanCallRootV62},
    visible::Handles,
    HumanCardRefV1, HumanDecisionErrorV1 as Error,
};
use crate::{
    omniscience_consumer_v1::ActionV1,
    omniscience_public_v1::{FactsV1, MeaningV1, PaymentTermsV1, RouteV1, StageV1},
    rl::ActionSemanticV1 as Action,
    state::{CastMethodV4, Zone},
};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct HumanFactsV1 {
    meaning: MeaningV1,
    current_permissions: [Option<HumanCardRefV1>; 2],
    hand_offers: Vec<HumanOfferV1>,
    casts: Vec<HumanCastV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanOfferV1 {
    card: HumanCardRefV1,
    permission: HumanCallRootV62,
    ordinary_payment_available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanCastV1 {
    spell: HumanCardRefV1,
    permission: HumanCallRootV62,
    original_permission_active: bool,
    stack_position: Option<u32>,
    stage: StageV1,
    origin_zone: Zone,
    ordinary_payment_was_available: bool,
    route: Option<RouteV1>,
    x: Option<u16>,
    kicked: Option<bool>,
    optional_additional_cost_selected: Option<bool>,
    mandatory_mana_branch: Option<bool>,
    payment_terms_now: Option<PaymentTermsV1>,
    finalized_method: Option<CastMethodV4>,
}

pub(super) fn project(facts: &FactsV1, handles: &mut Handles) -> Result<HumanFactsV1, Error> {
    let mut current_permissions = [None, None];
    for (public, reference) in current_permissions.iter_mut().zip(&facts.current_permissions) {
        *public = reference.as_ref().map(|card| href(card, handles)).transpose()?;
    }
    let hand_offers = facts.hand_offers.iter().map(|offer| {
        Ok(HumanOfferV1 {
            card: href(&offer.card, handles)?,
            permission: root(&offer.permission, handles)?,
            ordinary_payment_available: offer.ordinary_payment_available,
        })
    }).collect::<Result<_, Error>>()?;
    let casts = facts.casts.iter().map(|cast| {
        Ok(HumanCastV1 {
            spell: href(&cast.spell, handles)?,
            permission: root(&cast.permission, handles)?,
            original_permission_active: cast.original_permission_active,
            stack_position: cast.stack_position,
            stage: cast.stage,
            origin_zone: cast.origin_zone,
            ordinary_payment_was_available: cast.ordinary_payment_was_available,
            route: cast.route,
            x: cast.x,
            kicked: cast.kicked,
            optional_additional_cost_selected: cast.optional_additional_cost_selected,
            mandatory_mana_branch: cast.mandatory_mana_branch,
            payment_terms_now: cast.payment_terms_now.clone(),
            finalized_method: cast.finalized_method,
        })
    }).collect::<Result<_, Error>>()?;
    Ok(HumanFactsV1 {
        meaning: facts.meaning.clone(),
        current_permissions,
        hand_offers,
        casts,
    })
}

pub(super) fn label(
    facts: Option<&FactsV1>,
    action: &Action,
    handles: &Handles,
) -> Result<Option<String>, Error> {
    let annotated = crate::omniscience_consumer_v1::action(facts, action)
        .map_err(|_| Error::InvalidVisibleReference)?;
    Ok(match annotated {
        Some(ActionV1::ChooseRoute { route: RouteV1::OrdinaryPayment }) => {
            Some("Pay the spell's ordinary costs".into())
        }
        Some(ActionV1::ChooseRoute { route: RouteV1::ReplaceBaseMana }) => {
            Some("Use Omniscience to skip the base mana cost".into())
        }
        Some(ActionV1::AnnounceHandSpell { ordinary_payment_available }) => {
            let Action::CastSpell { source, .. } = action else {
                return Err(Error::InvalidVisibleReference);
            };
            let card = handles.name(source)?;
            Some(if ordinary_payment_available {
                format!("Cast {card} (Omniscience available)")
            } else {
                format!("Cast {card} using Omniscience")
            })
        }
        None => None,
    })
}
