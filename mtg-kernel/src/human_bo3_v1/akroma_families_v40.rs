//! Safe typed family extensions and labels for V40 only.
use super::visible::Handles;
use super::*;
use crate::akroma_family_public_v40::FamilyProjectionV40;
use crate::rl::ActionSemanticV1;
type Error = HumanDecisionErrorV1;
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "family", content = "facts")]
pub enum HumanFamilyV40 {
    Early(Box<super::akroma_early_v40::HumanEarlyV40>),
    Sower(Box<super::sower_v14::HumanSowerExtensionsV14>),
    Frenzied(Box<super::frenzied_v15::HumanFrenziedExtensionsV15>),
    Wildwood(Box<super::wildwood_v16::HumanWildwoodExtensionsV16>),
    Authority(Box<super::authority_v17::HumanAuthorityExtensionsV17>),
    Employment(Box<super::employment_v18::HumanEmploymentExtensionsV18>),
    Cerberus(Box<super::cerberus_v19::HumanCerberusExtensionsV19>),
    Librarian(Box<super::librarian_v20::HumanLibrarianExtensionsV20>),
    ExtraTurn(Box<super::extra_turn_v23::HumanExtraTurnExtensionsV23>),
    Juggernaut(Box<super::juggernaut_v25::HumanJuggernautExtensionsV25>),
    Ashroot(Box<super::ashroot_v27::HumanAshrootExtensionsV27>),
    Banishing(Box<super::banishing_v26::HumanBanishingExtensionsV26>),
    Stromkirk(Box<super::stromkirk_v28::HumanStromkirkExtensionsV28>),
    Gourmand(Box<super::gourmand_v29::HumanGourmandExtensionsV29>),
    Garruk(Box<super::garruk_v30::HumanGarrukExtensionsV30>),
    Crystal(Box<super::crystal_v22::HumanCrystalExtensionsV22>),
    Arahbo(Box<super::arahbo_v31::HumanArahboExtensionsV31>),
    Strongbox(Box<super::strongbox_v32::HumanStrongboxExtensionsV32>),
    Fiery(Box<super::fiery_v33::HumanFieryExtensionsV33>),
    Krenko(Box<super::krenko_v34::HumanKrenkoExtensionsV34>),
    ZulAshur(Box<super::zul_ashur_v35::HumanZulAshurExtensionsV35>),
    Banner(Box<super::banner_v36::HumanBannerExtensionsV36>),
    Inspiration(Box<super::inspiration_v37::HumanInspirationExtensionsV37>),
    Fishing(Box<super::fishing_pole_v39::HumanFishingPoleExtensionsV39>),
}
pub(super) fn register(
    projections: &[FamilyProjectionV40],
    handles: &mut Handles,
) -> Result<(), Error> {
    for projection in projections {
        match projection {
            FamilyProjectionV40::Fishing(o) => {
                super::fishing_pole_v39::register_for_akroma_parts(&o.parts, handles)?
            }
            FamilyProjectionV40::Early(o) => super::akroma_early_v40::register(o, handles)?,
            FamilyProjectionV40::Sower(o) => {
                super::sower_v14::register_for_akroma_parts(o, handles)?
            }
            FamilyProjectionV40::Frenzied(o) => {
                super::frenzied_v15::register_for_akroma_parts(o, handles)?
            }
            FamilyProjectionV40::Wildwood(o) => {
                super::wildwood_v16::register_for_akroma_parts(o, handles)?
            }
            FamilyProjectionV40::Authority(o) => {
                super::authority_v17::register_for_akroma_parts(o, handles)?
            }
            FamilyProjectionV40::Employment(o) => {
                super::employment_v18::register_for_akroma_parts(o, handles)?
            }
            FamilyProjectionV40::Cerberus(o) => {
                super::cerberus_v19::register_for_akroma_parts(o, handles)?
            }
            FamilyProjectionV40::Librarian(o) => {
                super::librarian_v20::register_for_akroma_parts(o, handles)?
            }
            FamilyProjectionV40::ExtraTurn(o) => {
                super::extra_turn_v23::register_for_akroma_parts(o, handles)?
            }
            FamilyProjectionV40::Juggernaut(o) => {
                super::juggernaut_v25::register_for_akroma_parts(o, handles)?
            }
            FamilyProjectionV40::Ashroot(o) => {
                super::ashroot_v27::register_for_akroma_parts(o, handles)?
            }
            FamilyProjectionV40::Banishing(o) => {
                super::banishing_v26::register_for_akroma_parts(o, handles)?
            }
            FamilyProjectionV40::Stromkirk(o) => {
                super::stromkirk_v28::register_for_akroma_parts(o, handles)?
            }
            FamilyProjectionV40::Gourmand(o) => {
                super::gourmand_v29::register_for_akroma_parts(o, handles)?
            }
            FamilyProjectionV40::Garruk(o) => {
                super::garruk_v30::register_for_akroma_parts(o, handles)?
            }
            FamilyProjectionV40::Crystal(o) => {
                super::crystal_v22::register_for_akroma_parts(o, handles)?
            }
            FamilyProjectionV40::Arahbo(o) => {
                super::arahbo_v31::register_for_akroma_parts(o, handles)?
            }
            FamilyProjectionV40::Strongbox(o) => {
                super::strongbox_v32::register_for_akroma_parts(o, handles)?
            }
            FamilyProjectionV40::Fiery(o) => {
                super::fiery_v33::register_for_akroma_parts(o, handles)?
            }
            FamilyProjectionV40::Krenko(o) => {
                super::krenko_v34::register_for_akroma_parts(o, handles)?
            }
            FamilyProjectionV40::ZulAshur(o) => {
                super::zul_ashur_v35::register_for_akroma_parts(o, handles)?
            }
            FamilyProjectionV40::Banner(o) => {
                super::banner_v36::register_for_akroma_parts(o, handles)?
            }
            FamilyProjectionV40::Inspiration(o) => {
                super::inspiration_v37::register_for_akroma_parts(o, handles)?
            }
        }
    }
    Ok(())
}
pub(super) fn states(
    projections: &[FamilyProjectionV40],
    handles: &mut Handles,
    human: PlayerSeatV1,
) -> Result<Vec<HumanFamilyV40>, Error> {
    projections
        .iter()
        .map(|projection| match projection {
            FamilyProjectionV40::Fishing(o) => Ok(HumanFamilyV40::Fishing(Box::new(
                super::fishing_pole_v39::state_for_akroma_parts(&o.parts, handles, human)?,
            ))),
            FamilyProjectionV40::Early(o) => Ok(HumanFamilyV40::Early(Box::new(
                super::akroma_early_v40::state(o, handles)?,
            ))),
            FamilyProjectionV40::Sower(o) => Ok(HumanFamilyV40::Sower(Box::new(
                super::sower_v14::state_for_akroma_parts(o, handles, human)?,
            ))),
            FamilyProjectionV40::Frenzied(o) => Ok(HumanFamilyV40::Frenzied(Box::new(
                super::frenzied_v15::state_for_akroma_parts(o, handles, human)?,
            ))),
            FamilyProjectionV40::Wildwood(o) => Ok(HumanFamilyV40::Wildwood(Box::new(
                super::wildwood_v16::state_for_akroma_parts(o, handles, human)?,
            ))),
            FamilyProjectionV40::Authority(o) => Ok(HumanFamilyV40::Authority(Box::new(
                super::authority_v17::state_for_akroma_parts(o, handles, human)?,
            ))),
            FamilyProjectionV40::Employment(o) => Ok(HumanFamilyV40::Employment(Box::new(
                super::employment_v18::state_for_akroma_parts(o, handles, human)?,
            ))),
            FamilyProjectionV40::Cerberus(o) => Ok(HumanFamilyV40::Cerberus(Box::new(
                super::cerberus_v19::state_for_akroma_parts(o, handles, human)?,
            ))),
            FamilyProjectionV40::Librarian(o) => Ok(HumanFamilyV40::Librarian(Box::new(
                super::librarian_v20::state_for_akroma_parts(o, handles, human)?,
            ))),
            FamilyProjectionV40::ExtraTurn(o) => Ok(HumanFamilyV40::ExtraTurn(Box::new(
                super::extra_turn_v23::state_for_akroma_parts(o, handles, human)?,
            ))),
            FamilyProjectionV40::Juggernaut(o) => Ok(HumanFamilyV40::Juggernaut(Box::new(
                super::juggernaut_v25::state_for_akroma_parts(o, handles, human)?,
            ))),
            FamilyProjectionV40::Ashroot(o) => Ok(HumanFamilyV40::Ashroot(Box::new(
                super::ashroot_v27::state_for_akroma_parts(o, handles, human)?,
            ))),
            FamilyProjectionV40::Banishing(o) => Ok(HumanFamilyV40::Banishing(Box::new(
                super::banishing_v26::state_for_akroma_parts(o, handles, human)?,
            ))),
            FamilyProjectionV40::Stromkirk(o) => Ok(HumanFamilyV40::Stromkirk(Box::new(
                super::stromkirk_v28::state_for_akroma_parts(o, handles, human)?,
            ))),
            FamilyProjectionV40::Gourmand(o) => Ok(HumanFamilyV40::Gourmand(Box::new(
                super::gourmand_v29::state_for_akroma_parts(o, handles, human)?,
            ))),
            FamilyProjectionV40::Garruk(o) => Ok(HumanFamilyV40::Garruk(Box::new(
                super::garruk_v30::state_for_akroma_parts(o, handles, human)?,
            ))),
            FamilyProjectionV40::Crystal(o) => Ok(HumanFamilyV40::Crystal(Box::new(
                super::crystal_v22::state_for_akroma_parts(o, handles, human)?,
            ))),
            FamilyProjectionV40::Arahbo(o) => Ok(HumanFamilyV40::Arahbo(Box::new(
                super::arahbo_v31::state_for_akroma_parts(o, handles, human)?,
            ))),
            FamilyProjectionV40::Strongbox(o) => Ok(HumanFamilyV40::Strongbox(Box::new(
                super::strongbox_v32::state_for_akroma_parts(o, handles, human)?,
            ))),
            FamilyProjectionV40::Fiery(o) => Ok(HumanFamilyV40::Fiery(Box::new(
                super::fiery_v33::state_for_akroma_parts(o, handles, human)?,
            ))),
            FamilyProjectionV40::Krenko(o) => Ok(HumanFamilyV40::Krenko(Box::new(
                super::krenko_v34::state_for_akroma_parts(o, handles, human)?,
            ))),
            FamilyProjectionV40::ZulAshur(o) => Ok(HumanFamilyV40::ZulAshur(Box::new(
                super::zul_ashur_v35::state_for_akroma_parts(o, handles, human)?,
            ))),
            FamilyProjectionV40::Banner(o) => Ok(HumanFamilyV40::Banner(Box::new(
                super::banner_v36::state_for_akroma_parts(o, handles, human)?,
            ))),
            FamilyProjectionV40::Inspiration(o) => Ok(HumanFamilyV40::Inspiration(Box::new(
                super::inspiration_v37::state_for_akroma_parts(o, handles, human)?,
            ))),
        })
        .collect()
}
pub(super) fn unary(
    facts: &[HumanFamilyV40],
    unary: &mut std::collections::BTreeMap<String, Vec<String>>,
) -> Result<(), Error> {
    for fact in facts {
        match fact {
            HumanFamilyV40::Fishing(o) => {
                super::fishing_pole_v39::unary_for_akroma_parts(o, unary)?
            }
            HumanFamilyV40::Early(o) => super::akroma_early_v40::unary(o, unary)?,
            HumanFamilyV40::Sower(o) => super::sower_v14::unary_for_akroma_parts(o, unary)?,
            HumanFamilyV40::Frenzied(o) => super::frenzied_v15::unary_for_akroma_parts(o, unary)?,
            HumanFamilyV40::Wildwood(o) => super::wildwood_v16::unary_for_akroma_parts(o, unary)?,
            HumanFamilyV40::Authority(o) => super::authority_v17::unary_for_akroma_parts(o, unary)?,
            HumanFamilyV40::Employment(o) => {
                super::employment_v18::unary_for_akroma_parts(o, unary)?
            }
            HumanFamilyV40::Cerberus(o) => super::cerberus_v19::unary_for_akroma_parts(o, unary)?,
            HumanFamilyV40::Librarian(o) => super::librarian_v20::unary_for_akroma_parts(o, unary)?,
            HumanFamilyV40::ExtraTurn(o) => {
                super::extra_turn_v23::unary_for_akroma_parts(o, unary)?
            }
            HumanFamilyV40::Juggernaut(o) => {
                super::juggernaut_v25::unary_for_akroma_parts(o, unary)?
            }
            HumanFamilyV40::Ashroot(o) => super::ashroot_v27::unary_for_akroma_parts(o, unary)?,
            HumanFamilyV40::Banishing(o) => super::banishing_v26::unary_for_akroma_parts(o, unary)?,
            HumanFamilyV40::Stromkirk(o) => super::stromkirk_v28::unary_for_akroma_parts(o, unary)?,
            HumanFamilyV40::Gourmand(o) => super::gourmand_v29::unary_for_akroma_parts(o, unary)?,
            HumanFamilyV40::Garruk(o) => super::garruk_v30::unary_for_akroma_parts(o, unary)?,
            HumanFamilyV40::Crystal(o) => super::crystal_v22::unary_for_akroma_parts(o, unary)?,
            HumanFamilyV40::Arahbo(o) => super::arahbo_v31::unary_for_akroma_parts(o, unary)?,
            HumanFamilyV40::Strongbox(o) => super::strongbox_v32::unary_for_akroma_parts(o, unary)?,
            HumanFamilyV40::Fiery(o) => super::fiery_v33::unary_for_akroma_parts(o, unary)?,
            HumanFamilyV40::Krenko(o) => super::krenko_v34::unary_for_akroma_parts(o, unary)?,
            HumanFamilyV40::ZulAshur(o) => super::zul_ashur_v35::unary_for_akroma_parts(o, unary)?,
            HumanFamilyV40::Banner(o) => super::banner_v36::unary_for_akroma_parts(o, unary)?,
            HumanFamilyV40::Inspiration(o) => {
                super::inspiration_v37::unary_for_akroma_parts(o, unary)?
            }
        }
    }
    Ok(())
}
pub(super) fn label(
    card_id: Option<u16>,
    action: &ActionSemanticV1,
    projections: &[FamilyProjectionV40],
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<Option<String>, Error> {
    for projection in projections {
        if let FamilyProjectionV40::Fishing(o) = projection {
            if let Some(label) =
                super::fishing_pole_v39::label_for_akroma_parts(action, o, handles, human)?
            {
                return Ok(Some(label));
            }
        }
    }
    for projection in projections {
        let selected = match projection {
            FamilyProjectionV40::Fishing(_) => None,

            FamilyProjectionV40::Early(o) => {
                super::akroma_early_v40::label(action, o, handles, human)?
            }
            FamilyProjectionV40::Sower(o) => {
                if o.roots.iter().any(|root| Some(root.card_id) == card_id) {
                    Some(super::sower_v14::label_for_akroma_parts(
                        action, o, handles, human,
                    )?)
                } else {
                    None
                }
            }
            FamilyProjectionV40::Frenzied(o) => {
                if o.roots.iter().any(|root| Some(root.card_id) == card_id) {
                    Some(super::frenzied_v15::label_for_akroma_parts(
                        action, o, handles, human,
                    )?)
                } else {
                    None
                }
            }
            FamilyProjectionV40::Wildwood(o) => {
                if o.roots.iter().any(|root| Some(root.card_id) == card_id) {
                    Some(super::wildwood_v16::label_for_akroma_parts(
                        action, o, handles, human,
                    )?)
                } else {
                    None
                }
            }
            FamilyProjectionV40::Authority(o) => {
                if o.roots.iter().any(|root| Some(root.card_id) == card_id) {
                    Some(super::authority_v17::label_for_akroma_parts(
                        action, o, handles, human,
                    )?)
                } else {
                    None
                }
            }
            FamilyProjectionV40::Employment(o) => {
                if o.roots.iter().any(|root| Some(root.card_id) == card_id) {
                    Some(super::employment_v18::label_for_akroma_parts(
                        action, o, handles, human,
                    )?)
                } else {
                    None
                }
            }
            FamilyProjectionV40::Cerberus(o) => {
                if o.roots.iter().any(|root| Some(root.card_id) == card_id) {
                    Some(super::cerberus_v19::label_for_akroma_parts(
                        action, o, handles, human,
                    )?)
                } else {
                    None
                }
            }
            FamilyProjectionV40::Librarian(o) => {
                if o.roots.iter().any(|root| Some(root.card_id) == card_id) {
                    Some(super::librarian_v20::label_for_akroma_parts(
                        action, o, handles, human,
                    )?)
                } else {
                    None
                }
            }
            FamilyProjectionV40::ExtraTurn(o) => {
                if o.roots.iter().any(|root| Some(root.card_id) == card_id) {
                    Some(super::extra_turn_v23::label_for_akroma_parts(
                        action, o, handles, human,
                    )?)
                } else {
                    None
                }
            }
            FamilyProjectionV40::Juggernaut(o) => {
                if o.roots.iter().any(|root| Some(root.card_id) == card_id) {
                    Some(super::juggernaut_v25::label_for_akroma_parts(
                        action, o, handles, human,
                    )?)
                } else {
                    None
                }
            }
            FamilyProjectionV40::Ashroot(o) => {
                if o.roots.iter().any(|root| Some(root.card_id) == card_id) {
                    Some(super::ashroot_v27::label_for_akroma_parts(
                        action, o, handles, human,
                    )?)
                } else {
                    None
                }
            }
            FamilyProjectionV40::Banishing(o) => {
                if o.roots.iter().any(|root| Some(root.card_id) == card_id)
                    || o.attachment_choice.is_some()
                {
                    Some(super::banishing_v26::label_for_akroma_parts(
                        action, o, handles, human,
                    )?)
                } else {
                    None
                }
            }
            FamilyProjectionV40::Stromkirk(o) => {
                if o.roots.iter().any(|root| Some(root.card_id) == card_id) {
                    Some(super::stromkirk_v28::label_for_akroma_parts(
                        action, o, handles, human,
                    )?)
                } else {
                    None
                }
            }
            FamilyProjectionV40::Gourmand(o) => {
                if o.roots.iter().any(|root| Some(root.card_id) == card_id) {
                    Some(super::gourmand_v29::label_for_akroma_parts(
                        action, o, handles, human,
                    )?)
                } else {
                    None
                }
            }
            FamilyProjectionV40::Garruk(o) => {
                if o.roots.iter().any(|root| Some(root.card_id) == card_id) {
                    Some(super::garruk_v30::label_for_akroma_parts(
                        action, o, handles, human,
                    )?)
                } else {
                    None
                }
            }
            FamilyProjectionV40::Crystal(o) => {
                if card_id.is_some_and(|id| {
                    crate::card_def::CARD_DEFS
                        .get(usize::from(id))
                        .is_some_and(|d| d.name == "Crystal Barricade")
                }) || o.prevention_menu.is_some()
                    || o.optional_discard_menu.is_some()
                    || o.optional_fight_menu.is_some()
                {
                    Some(super::crystal_v22::label_for_akroma_parts(
                        action, o, handles, human,
                    )?)
                } else {
                    None
                }
            }
            FamilyProjectionV40::Arahbo(o) => {
                if o.roots.iter().any(|root| Some(root.card_id) == card_id) {
                    Some(super::arahbo_v31::label_for_akroma_parts(
                        action, o, handles, human,
                    )?)
                } else {
                    None
                }
            }
            FamilyProjectionV40::Strongbox(o) => {
                if o.roots.iter().any(|root| Some(root.card_id) == card_id) {
                    Some(super::strongbox_v32::label_for_akroma_parts(
                        action, o, handles, human,
                    )?)
                } else {
                    None
                }
            }
            FamilyProjectionV40::Fiery(o) => {
                if o.roots.iter().any(|root| Some(root.card_id) == card_id) {
                    Some(super::fiery_v33::label_for_akroma_parts(
                        action, o, handles, human,
                    )?)
                } else {
                    None
                }
            }
            FamilyProjectionV40::Krenko(o) => {
                if o.roots.iter().any(|root| Some(root.card_id) == card_id) {
                    Some(super::krenko_v34::label_for_akroma_parts(
                        action, o, handles, human,
                    )?)
                } else {
                    None
                }
            }
            FamilyProjectionV40::ZulAshur(o) => {
                if o.roots.iter().any(|root| Some(root.card_id) == card_id) {
                    Some(super::zul_ashur_v35::label_for_akroma_parts(
                        action, o, handles, human,
                    )?)
                } else {
                    None
                }
            }
            FamilyProjectionV40::Banner(o) => {
                if o.roots.iter().any(|root| Some(root.card_id) == card_id)
                    || o.entry_choice.is_some()
                {
                    Some(super::banner_v36::label_for_akroma_parts(
                        action, o, handles, human,
                    )?)
                } else {
                    None
                }
            }
            FamilyProjectionV40::Inspiration(o) => {
                if o.meanings.iter().any(|root| Some(root.card_id) == card_id) {
                    Some(super::inspiration_v37::label_for_akroma_parts(
                        action, o, handles, human,
                    )?)
                } else {
                    None
                }
            }
        };
        if selected.is_some() {
            return Ok(selected);
        }
    }
    Ok(None)
}
