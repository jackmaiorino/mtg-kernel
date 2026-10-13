//! V40 family parts. No predecessor observation or model identity is exported by this union.
use crate::ids::PlayerId;
use crate::policy_surface_v5::PolicySurfaceV5;
use crate::rl::RlContractError;
use crate::state::GameState;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SowerFactsV40 {
    pub restrictions: Vec<crate::sower_public_v1::RestrictionV1>,
    pub roots: Vec<crate::sower_public_v1::RootV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrenziedFactsV40 {
    pub restrictions: Vec<crate::frenzied_public_v1::RestrictionV1>,
    pub resolutions: Vec<crate::frenzied_public_v1::ResolutionV1>,
    pub roots: Vec<crate::frenzied_public_v1::RootV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WildwoodFactsV40 {
    pub counter_occurrences: Vec<crate::wildwood_public_v1::CounterOccurrenceV1>,
    pub roots: Vec<crate::wildwood_public_v1::RootV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorityFactsV40 {
    pub entry_occurrences: Vec<crate::authority_public_v1::EntryOccurrenceV1>,
    pub active_sources: Vec<crate::authority_public_v1::ActiveSourceV1>,
    pub roots: Vec<crate::authority_public_v1::RootV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmploymentFactsV40 {
    pub installations: Vec<crate::employment_public_v1::InstallationV1>,
    pub roots: Vec<crate::employment_public_v1::RootV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CerberusFactsV40 {
    pub creature_died_this_turn: bool,
    pub untap_restrictions: Vec<crate::cerberus_public_v1::UntapRestrictionV1>,
    pub triggers: Vec<crate::cerberus_public_v1::TriggerV1>,
    pub roots: Vec<crate::cerberus_public_v1::RootV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LibrarianFactsV40 {
    pub activations: Vec<crate::librarian_public_v1::ActivationV1>,
    pub roots: Vec<crate::librarian_public_v1::RootV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtraTurnFactsV40 {
    pub creature_died_this_turn: bool,
    pub turn_schedule: Option<crate::extra_turn_public_v1::TurnScheduleV1>,
    pub roots: Vec<crate::extra_turn_public_v1::RootV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JuggernautFactsV40 {
    pub combat_rules: Vec<crate::juggernaut_public_v1::CombatRuleV1>,
    pub roots: Vec<crate::juggernaut_public_v1::RootV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AshrootFactsV40 {
    pub power_contexts: Vec<crate::ashroot_public_v1::PowerContextV1>,
    pub roots: Vec<crate::ashroot_public_v1::RootV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BanishingFactsV40 {
    pub triggers: Vec<crate::banishing_public_v1::TriggerV1>,
    pub links: Vec<crate::banishing_public_v1::LinkV1>,
    pub attachment_choice: Option<crate::banishing_public_v1::AttachmentChoiceV1>,
    pub confirmed_attachments: Vec<crate::banishing_public_v1::ConfirmedAttachmentV1>,
    pub roots: Vec<crate::banishing_public_v1::RootV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StromkirkFactsV40 {
    pub lost_life_this_turn: Option<[bool; 2]>,
    pub loss_contexts: Vec<crate::stromkirk_public_v1::LossContextV1>,
    pub roots: Vec<crate::stromkirk_public_v1::RootV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GourmandFactsV40 {
    pub restrictions: Vec<crate::gourmand_public_v1::RestrictionV1>,
    pub resolutions: Vec<crate::gourmand_public_v1::ResolutionV1>,
    pub sacrifice_menu: Option<crate::gourmand_public_v1::SacrificeMenuV1>,
    pub roots: Vec<crate::gourmand_public_v1::RootV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GarrukFactsV40 {
    pub entry_occurrences: Vec<crate::garruk_public_v1::EntryOccurrenceV1>,
    pub active_sources: Vec<crate::garruk_public_v1::ActiveSourceV1>,
    pub roots: Vec<crate::garruk_public_v1::RootV1>,
    pub linked_exile: crate::garruk_public_v1::LinkedExileFactsV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrystalFactsV40 {
    pub static_sources: Vec<crate::crystal_public_v1::StaticSourceV1>,
    pub equipment_damage_sources: Vec<crate::equipment_damage_source_v4::PublicSourceV4>,
    pub prevention_menu: Option<crate::crystal_public_v1::MenuV1>,
    pub optional_discard_menu: Option<crate::crystal_public_v1::DiscardMenuV2>,
    pub optional_fight_menu: Option<crate::crystal_public_v1::ProgramMenuV3>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArahboFactsV40 {
    pub entry_occurrences: Vec<crate::arahbo_public_v1::EntryOccurrenceV1>,
    pub active_sources: Vec<crate::arahbo_public_v1::ActiveSourceV1>,
    pub roots: Vec<crate::arahbo_public_v1::RootV1>,
    pub linked_exile: crate::arahbo_public_v1::LinkedExileFactsV1,
    pub garruk_occurrences: Vec<crate::garruk_public_v1::EntryOccurrenceV1>,
    pub garruk_active_sources: Vec<crate::garruk_public_v1::ActiveSourceV1>,
    pub garruk_roots: Vec<crate::garruk_public_v1::RootV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrongboxFactsV40 {
    pub entries: Vec<crate::strongbox_public_v1::EntryV1>,
    pub selection_menu: Option<crate::strongbox_public_v1::SelectionMenuV1>,
    pub roots: Vec<crate::strongbox_public_v1::RootV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieryFactsV40 {
    pub facts: crate::fiery_public_v1::FactsV1,
    pub cast_menu: Option<crate::fiery_observation_v1::CastMenuV1>,
    pub roots: Vec<crate::fiery_observation_v1::RootV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KrenkoFactsV40 {
    pub facts: crate::krenko_public_v1::FactsV1,
    pub roots: Vec<crate::krenko_observation_v1::RootV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ZulAshurFactsV40 {
    pub facts: Vec<crate::zul_ashur_public_v1::PermissionV1>,
    pub roots: Vec<crate::zul_ashur_observation_v1::RootV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BannerFactsV40 {
    pub entry_choice: Option<crate::banner_observation_v1::EntryChoiceV36>,
    pub chosen_colors: Vec<crate::banner_observation_v1::ChosenColorV36>,
    pub prior_entry_colors: Vec<crate::banner_observation_v1::PriorEntryColorV36>,
    pub roots: Vec<crate::banner_observation_v1::RootV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspirationFactsV40 {
    pub meanings: Vec<crate::rules_meaning_inspiration_v1::MeaningV1>,
    pub choice: Option<crate::inspiration_observation_v1::ChoiceV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FishingFactsV40 {
    pub poles: Vec<crate::fishing_pole_public_v39::PoleV39>,
    pub operations: Vec<crate::fishing_pole_public_v39::OperationV39>,
    pub roots: Vec<crate::fishing_pole_observation_v39::RootV1>,
}
// Actual ordered action classifications stay private and participate in binding.
#[derive(Clone, PartialEq)]
pub(crate) struct FishingProjectionV40 {
    pub(crate) parts: crate::fishing_pole_observation_v39::FishingPoleObservationV1,
    pub(crate) actions: Vec<crate::rl_session::fishing_pole_v39::ClassifiedActionV39>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TwinflameFactsV40 {
    pub static_sources: Vec<crate::twinflame_observation_v1::StaticSourceV1>,
    pub multiplier_sources: Vec<crate::twinflame_observation_v1::StaticSourceV1>,
    pub meanings: Vec<crate::twinflame_tyrant_v1::PrintedMeaningV1>,
    pub prevention_menu: Option<crate::twinflame_observation_v1::MenuV1>,
    pub combat_allocation: Option<crate::twinflame_observation_v1::AllocationMenuV38>,
    pub optional_discard_menu: Option<crate::twinflame_observation_v1::DiscardMenuV2>,
    pub optional_fight_menu: Option<crate::twinflame_observation_v1::ProgramMenuV3>,
    pub equipment_damage_sources: Vec<crate::equipment_damage_source_v4::PublicSourceV4>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "family", content = "facts")]
pub enum FamilyFactsV40 {
    Early(Box<crate::akroma_early_public_v40::EarlyFactsV40>),
    Sower(Box<SowerFactsV40>),
    Frenzied(Box<FrenziedFactsV40>),
    Wildwood(Box<WildwoodFactsV40>),
    Authority(Box<AuthorityFactsV40>),
    Employment(Box<EmploymentFactsV40>),
    Cerberus(Box<CerberusFactsV40>),
    Librarian(Box<LibrarianFactsV40>),
    ExtraTurn(Box<ExtraTurnFactsV40>),
    Juggernaut(Box<JuggernautFactsV40>),
    Ashroot(Box<AshrootFactsV40>),
    Banishing(Box<BanishingFactsV40>),
    Stromkirk(Box<StromkirkFactsV40>),
    Gourmand(Box<GourmandFactsV40>),
    Garruk(Box<GarrukFactsV40>),
    Crystal(Box<CrystalFactsV40>),
    Arahbo(Box<ArahboFactsV40>),
    Strongbox(Box<StrongboxFactsV40>),
    Fiery(Box<FieryFactsV40>),
    Krenko(Box<KrenkoFactsV40>),
    ZulAshur(Box<ZulAshurFactsV40>),
    Banner(Box<BannerFactsV40>),
    Inspiration(Box<InspirationFactsV40>),
    Fishing(Box<FishingFactsV40>),
    GameOutcome(Box<GameOutcomeFactsV40>),
    Twinflame(Box<TwinflameFactsV40>),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameOutcomeFactsV40 {
    pub protection: crate::game_outcome_protection_public_v1::PublicFactsV1,
    pub roots: Vec<crate::rules_meaning_game_outcome_protection_v1::MeaningV1>,
}
#[derive(Clone, PartialEq)]
pub(crate) struct GameOutcomeProjectionV40 {
    pub(crate) facts: GameOutcomeFactsV40,
    projection: crate::rl::PublicObservationProjectionV5,
}
// Backend-only temporaries; never exported as standalone predecessor artifacts.
#[derive(Clone, PartialEq)]
pub(crate) enum FamilyProjectionV40 {
    Early(Box<crate::akroma_early_public_v40::EarlyProjectionV40>),
    Sower(Box<crate::sower_public_v1::SowerObservationV1>),
    Frenzied(Box<crate::frenzied_public_v1::FrenziedObservationV1>),
    Wildwood(Box<crate::wildwood_public_v1::WildwoodObservationV1>),
    Authority(Box<crate::authority_public_v1::AuthorityObservationV1>),
    Employment(Box<crate::employment_public_v1::EmploymentObservationV1>),
    Cerberus(Box<crate::cerberus_public_v1::CerberusObservationV1>),
    Librarian(Box<crate::librarian_public_v1::LibrarianObservationV1>),
    ExtraTurn(Box<crate::extra_turn_public_v1::ExtraTurnObservationV1>),
    Juggernaut(Box<crate::juggernaut_public_v1::JuggernautObservationV1>),
    Ashroot(Box<crate::ashroot_public_v1::AshrootObservationV1>),
    Banishing(Box<crate::banishing_public_v1::BanishingObservationV1>),
    Stromkirk(Box<crate::stromkirk_public_v1::StromkirkObservationV1>),
    Gourmand(Box<crate::gourmand_public_v1::GourmandObservationV1>),
    Garruk(Box<crate::garruk_public_v1::GarrukObservationV1>),
    Crystal(Box<crate::crystal_public_v1::CrystalObservationV1>),
    Arahbo(Box<crate::arahbo_public_v1::ArahboObservationV1>),
    Strongbox(Box<crate::strongbox_public_v1::StrongboxObservationV1>),
    Fiery(Box<crate::fiery_observation_v1::FieryObservationV1>),
    Krenko(Box<crate::krenko_observation_v1::KrenkoObservationV1>),
    ZulAshur(Box<crate::zul_ashur_observation_v1::ZulAshurObservationV1>),
    Banner(Box<crate::banner_observation_v1::BannerObservationV1>),
    Inspiration(Box<crate::inspiration_observation_v1::InspirationObservationV1>),
    Fishing(Box<FishingProjectionV40>),
    GameOutcome(Box<GameOutcomeProjectionV40>),
    Twinflame(Box<crate::twinflame_observation_v1::TwinflameObservationV1>),
}
impl FamilyProjectionV40 {
    pub(crate) fn facts(&self) -> FamilyFactsV40 {
        match self {
            Self::GameOutcome(o) => FamilyFactsV40::GameOutcome(Box::new(o.facts.clone())),
            Self::Fishing(o) => FamilyFactsV40::Fishing(Box::new(FishingFactsV40 {
                poles: o.parts.facts.poles.clone(),
                operations: o.parts.facts.operations.clone(),
                roots: o.parts.roots.clone(),
            })),
            Self::Early(o) => FamilyFactsV40::Early(Box::new(o.facts.clone())),
            Self::Sower(o) => FamilyFactsV40::Sower(Box::new(SowerFactsV40 {
                restrictions: o.restrictions.clone(),
                roots: o.roots.clone(),
            })),
            Self::Frenzied(o) => FamilyFactsV40::Frenzied(Box::new(FrenziedFactsV40 {
                restrictions: o.restrictions.clone(),
                resolutions: o.resolutions.clone(),
                roots: o.roots.clone(),
            })),
            Self::Wildwood(o) => FamilyFactsV40::Wildwood(Box::new(WildwoodFactsV40 {
                counter_occurrences: o.counter_occurrences.clone(),
                roots: o.roots.clone(),
            })),
            Self::Authority(o) => FamilyFactsV40::Authority(Box::new(AuthorityFactsV40 {
                entry_occurrences: o.entry_occurrences.clone(),
                active_sources: o.active_sources.clone(),
                roots: o.roots.clone(),
            })),
            Self::Employment(o) => FamilyFactsV40::Employment(Box::new(EmploymentFactsV40 {
                installations: o.installations.clone(),
                roots: o.roots.clone(),
            })),
            Self::Cerberus(o) => FamilyFactsV40::Cerberus(Box::new(CerberusFactsV40 {
                creature_died_this_turn: o.creature_died_this_turn,
                untap_restrictions: o.untap_restrictions.clone(),
                triggers: o.triggers.clone(),
                roots: o.roots.clone(),
            })),
            Self::Librarian(o) => FamilyFactsV40::Librarian(Box::new(LibrarianFactsV40 {
                activations: o.activations.clone(),
                roots: o.roots.clone(),
            })),
            Self::ExtraTurn(o) => FamilyFactsV40::ExtraTurn(Box::new(ExtraTurnFactsV40 {
                creature_died_this_turn: o.creature_died_this_turn,
                turn_schedule: o.turn_schedule.clone(),
                roots: o.roots.clone(),
            })),
            Self::Juggernaut(o) => FamilyFactsV40::Juggernaut(Box::new(JuggernautFactsV40 {
                combat_rules: o.combat_rules.clone(),
                roots: o.roots.clone(),
            })),
            Self::Ashroot(o) => FamilyFactsV40::Ashroot(Box::new(AshrootFactsV40 {
                power_contexts: o.power_contexts.clone(),
                roots: o.roots.clone(),
            })),
            Self::Banishing(o) => FamilyFactsV40::Banishing(Box::new(BanishingFactsV40 {
                triggers: o.triggers.clone(),
                links: o.links.clone(),
                attachment_choice: o.attachment_choice.clone(),
                confirmed_attachments: o.confirmed_attachments.clone(),
                roots: o.roots.clone(),
            })),
            Self::Stromkirk(o) => FamilyFactsV40::Stromkirk(Box::new(StromkirkFactsV40 {
                lost_life_this_turn: o.lost_life_this_turn,
                loss_contexts: o.loss_contexts.clone(),
                roots: o.roots.clone(),
            })),
            Self::Gourmand(o) => FamilyFactsV40::Gourmand(Box::new(GourmandFactsV40 {
                restrictions: o.restrictions.clone(),
                resolutions: o.resolutions.clone(),
                sacrifice_menu: o.sacrifice_menu.clone(),
                roots: o.roots.clone(),
            })),
            Self::Garruk(o) => FamilyFactsV40::Garruk(Box::new(GarrukFactsV40 {
                entry_occurrences: o.entry_occurrences.clone(),
                active_sources: o.active_sources.clone(),
                roots: o.roots.clone(),
                linked_exile: o.linked_exile.clone(),
            })),
            Self::Crystal(o) => FamilyFactsV40::Crystal(Box::new(CrystalFactsV40 {
                static_sources: o.static_sources.clone(),
                equipment_damage_sources: o.equipment_damage_sources.clone(),
                prevention_menu: o.prevention_menu.clone(),
                optional_discard_menu: o.optional_discard_menu.clone(),
                optional_fight_menu: o.optional_fight_menu.clone(),
            })),
            Self::Arahbo(o) => FamilyFactsV40::Arahbo(Box::new(ArahboFactsV40 {
                entry_occurrences: o.entry_occurrences.clone(),
                active_sources: o.active_sources.clone(),
                roots: o.roots.clone(),
                linked_exile: o.linked_exile.clone(),
                garruk_occurrences: o.garruk_occurrences.clone(),
                garruk_active_sources: o.garruk_active_sources.clone(),
                garruk_roots: o.garruk_roots.clone(),
            })),
            Self::Strongbox(o) => FamilyFactsV40::Strongbox(Box::new(StrongboxFactsV40 {
                entries: o.entries.clone(),
                selection_menu: o.selection_menu.clone(),
                roots: o.roots.clone(),
            })),
            Self::Fiery(o) => FamilyFactsV40::Fiery(Box::new(FieryFactsV40 {
                facts: o.facts.clone(),
                cast_menu: o.cast_menu.clone(),
                roots: o.roots.clone(),
            })),
            Self::Krenko(o) => FamilyFactsV40::Krenko(Box::new(KrenkoFactsV40 {
                facts: o.facts.clone(),
                roots: o.roots.clone(),
            })),
            Self::ZulAshur(o) => FamilyFactsV40::ZulAshur(Box::new(ZulAshurFactsV40 {
                facts: o.facts.clone(),
                roots: o.roots.clone(),
            })),
            Self::Banner(o) => FamilyFactsV40::Banner(Box::new(BannerFactsV40 {
                entry_choice: o.entry_choice.clone(),
                chosen_colors: o.chosen_colors.clone(),
                prior_entry_colors: o.prior_entry_colors.clone(),
                roots: o.roots.clone(),
            })),
            Self::Inspiration(o) => FamilyFactsV40::Inspiration(Box::new(InspirationFactsV40 {
                meanings: o.meanings.clone(),
                choice: o.choice.clone(),
            })),
            Self::Twinflame(o) => FamilyFactsV40::Twinflame(Box::new(TwinflameFactsV40 {
                static_sources: o.static_sources.clone(),
                multiplier_sources: o.multiplier_sources.clone(),
                meanings: o.meanings.clone(),
                prevention_menu: o.prevention_menu.clone(),
                combat_allocation: o.combat_allocation.clone(),
                optional_discard_menu: o.optional_discard_menu.clone(),
                optional_fight_menu: o.optional_fight_menu.clone(),
                equipment_damage_sources: o.equipment_damage_sources.clone(),
            })),
        }
    }
    pub(crate) fn projection(&self) -> &crate::rl::PublicObservationProjectionV5 {
        match self {
            Self::GameOutcome(o) => &o.projection,
            Self::Fishing(o) => &o.parts.projection,
            Self::Early(o) => &o.projection,
            Self::Sower(o) => &o.projection,
            Self::Frenzied(o) => &o.projection,
            Self::Wildwood(o) => &o.projection,
            Self::Authority(o) => &o.projection,
            Self::Employment(o) => &o.projection,
            Self::Cerberus(o) => &o.projection,
            Self::Librarian(o) => &o.projection,
            Self::ExtraTurn(o) => &o.projection,
            Self::Juggernaut(o) => &o.projection,
            Self::Ashroot(o) => &o.projection,
            Self::Banishing(o) => &o.projection,
            Self::Stromkirk(o) => &o.projection,
            Self::Gourmand(o) => &o.projection,
            Self::Garruk(o) => &o.projection,
            Self::Crystal(o) => &o.projection,
            Self::Arahbo(o) => &o.projection,
            Self::Strongbox(o) => &o.projection,
            Self::Fiery(o) => &o.projection,
            Self::Krenko(o) => &o.projection,
            Self::ZulAshur(o) => &o.projection,
            Self::Banner(o) => &o.projection,
            Self::Inspiration(o) => &o.projection,
            Self::Twinflame(o) => &o.projection,
        }
    }
}
impl FamilyProjectionV40 {
    pub(crate) fn set_projection(&mut self, projection: &crate::rl::PublicObservationProjectionV5) {
        match self {
            Self::GameOutcome(o) => o.projection = projection.clone(),
            Self::Fishing(o) => o.parts.projection = projection.clone(),
            Self::Early(o) => o.projection = projection.clone(),
            Self::Sower(o) => o.projection = projection.clone(),
            Self::Frenzied(o) => o.projection = projection.clone(),
            Self::Wildwood(o) => o.projection = projection.clone(),
            Self::Authority(o) => o.projection = projection.clone(),
            Self::Employment(o) => o.projection = projection.clone(),
            Self::Cerberus(o) => o.projection = projection.clone(),
            Self::Librarian(o) => o.projection = projection.clone(),
            Self::ExtraTurn(o) => o.projection = projection.clone(),
            Self::Juggernaut(o) => o.projection = projection.clone(),
            Self::Ashroot(o) => o.projection = projection.clone(),
            Self::Banishing(o) => o.projection = projection.clone(),
            Self::Stromkirk(o) => o.projection = projection.clone(),
            Self::Gourmand(o) => o.projection = projection.clone(),
            Self::Garruk(o) => o.projection = projection.clone(),
            Self::Crystal(o) => o.projection = projection.clone(),
            Self::Arahbo(o) => o.projection = projection.clone(),
            Self::Strongbox(o) => o.projection = projection.clone(),
            Self::Fiery(o) => o.projection = projection.clone(),
            Self::Krenko(o) => o.projection = projection.clone(),
            Self::ZulAshur(o) => o.projection = projection.clone(),
            Self::Banner(o) => o.projection = projection.clone(),
            Self::Inspiration(o) => o.projection = projection.clone(),
            Self::Twinflame(o) => o.projection = projection.clone(),
        }
    }
}
pub(crate) fn build(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step: u64,
    physical: u64,
    substep: (u32, u32),
) -> Result<Vec<FamilyProjectionV40>, RlContractError> {
    let base = crate::rl::akroma_family_observation_storage_v40(
        state, surface, actor, step, physical, substep,
    )?;
    let game_outcome_projection = base.projection.clone();
    let mut parts = vec![
        FamilyProjectionV40::Early(Box::new(crate::akroma_early_public_v40::build(
            state,
            base.projection,
            actor,
        )?)),
        FamilyProjectionV40::Sower(Box::new(crate::sower_public_v1::parts_for_akroma_v40(
            state, surface, actor, step, physical, substep,
        )?)),
        FamilyProjectionV40::Frenzied(Box::new(crate::frenzied_public_v1::parts_for_akroma_v40(
            state, surface, actor, step, physical, substep,
        )?)),
        FamilyProjectionV40::Wildwood(Box::new(crate::wildwood_public_v1::parts_for_akroma_v40(
            state, surface, actor, step, physical, substep,
        )?)),
        FamilyProjectionV40::Authority(Box::new(crate::authority_public_v1::parts_for_akroma_v40(
            state, surface, actor, step, physical, substep,
        )?)),
        FamilyProjectionV40::Employment(Box::new(
            crate::employment_public_v1::parts_for_akroma_v40(
                state, surface, actor, step, physical, substep,
            )?,
        )),
        FamilyProjectionV40::Cerberus(Box::new(crate::cerberus_public_v1::parts_for_akroma_v40(
            state, surface, actor, step, physical, substep,
        )?)),
        FamilyProjectionV40::Librarian(Box::new(crate::librarian_public_v1::parts_for_akroma_v40(
            state, surface, actor, step, physical, substep,
        )?)),
        FamilyProjectionV40::ExtraTurn(Box::new(
            crate::extra_turn_public_v1::parts_for_akroma_v40(
                state, surface, actor, step, physical, substep,
            )?,
        )),
        FamilyProjectionV40::Juggernaut(Box::new(
            crate::juggernaut_public_v1::parts_for_akroma_v40(
                state, surface, actor, step, physical, substep,
            )?,
        )),
        FamilyProjectionV40::Ashroot(Box::new(crate::ashroot_public_v1::parts_for_akroma_v40(
            state, surface, actor, step, physical, substep,
        )?)),
        FamilyProjectionV40::Banishing(Box::new(crate::banishing_public_v1::parts_for_akroma_v40(
            state, surface, actor, step, physical, substep,
        )?)),
        FamilyProjectionV40::Stromkirk(Box::new(crate::stromkirk_public_v1::parts_for_akroma_v40(
            state, surface, actor, step, physical, substep,
        )?)),
        FamilyProjectionV40::Gourmand(Box::new(crate::gourmand_public_v1::parts_for_akroma_v40(
            state, surface, actor, step, physical, substep,
        )?)),
        FamilyProjectionV40::Garruk(Box::new(crate::garruk_public_v1::parts_for_akroma_v40(
            state, surface, actor, step, physical, substep,
        )?)),
        FamilyProjectionV40::Crystal(Box::new(crate::crystal_public_v1::parts_for_akroma_v40(
            state, surface, actor, step, physical, substep,
        )?)),
        FamilyProjectionV40::Arahbo(Box::new(crate::arahbo_public_v1::parts_for_akroma_v40(
            state, surface, actor, step, physical, substep,
        )?)),
        FamilyProjectionV40::Strongbox(Box::new(crate::strongbox_public_v1::parts_for_akroma_v40(
            state, surface, actor, step, physical, substep,
        )?)),
        FamilyProjectionV40::Fiery(Box::new(crate::fiery_observation_v1::parts_for_akroma_v40(
            state, surface, actor, step, physical, substep,
        )?)),
        FamilyProjectionV40::Krenko(Box::new(
            crate::krenko_observation_v1::parts_for_akroma_v40(
                state, surface, actor, step, physical, substep,
            )?,
        )),
        FamilyProjectionV40::ZulAshur(Box::new(
            crate::zul_ashur_observation_v1::parts_for_akroma_v40(
                state, surface, actor, step, physical, substep,
            )?,
        )),
        FamilyProjectionV40::Banner(Box::new(
            crate::banner_observation_v1::parts_for_akroma_v40(
                state, surface, actor, step, physical, substep,
            )?,
        )),
        FamilyProjectionV40::Inspiration(Box::new(
            crate::inspiration_observation_v1::parts_for_akroma_v40(
                state, surface, actor, step, physical, substep,
            )?,
        )),
        FamilyProjectionV40::Fishing(Box::new(FishingProjectionV40 {
            parts: crate::fishing_pole_observation_v39::parts_for_akroma_v40(
                state, surface, actor, step, physical, substep,
            )?,
            actions: Vec::new(),
        })),
    ];
    if let Ok(selector) = crate::rules_meaning_game_outcome_protection_v1::live_selector() {
        let protection = crate::game_outcome_protection_public_v1::capture(state, &selector)
            .map_err(|_| RlContractError("invalid current game-outcome facts".into()))?;
        let roots = crate::card_def::CARD_DEFS
            .iter()
            .enumerate()
            .filter(|(_, def)| def.is_executable() && def.game_outcome_protection_v1.is_some())
            .map(|(index, _)| {
                let card_id = u16::try_from(index)
                    .map_err(|_| RlContractError("game-outcome definition index".into()))?;
                crate::rules_meaning_game_outcome_protection_v1::meaning(&selector, card_id)
                    .map_err(|_| RlContractError("unsupported game-outcome printed root".into()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        parts.push(FamilyProjectionV40::GameOutcome(Box::new(
            GameOutcomeProjectionV40 {
                facts: GameOutcomeFactsV40 { protection, roots },
                projection: game_outcome_projection,
            },
        )));
    }
    parts.push(FamilyProjectionV40::Twinflame(Box::new(
        crate::twinflame_observation_v1::parts_for_akroma_v40(
            state, surface, actor, step, physical, substep,
        )?,
    )));
    Ok(parts)
}
pub(crate) fn build_for_ajani_v42(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step: u64,
    physical: u64,
    substep: (u32, u32),
) -> Result<Vec<FamilyProjectionV40>, RlContractError> {
    let base = crate::rl::common_family_observation_storage_scoped(
        (crate::rl::CommonProjectionScopeV42::FullAjaniV42, state),
        surface,
        actor,
        step,
        physical,
        substep,
    )?;
    let game_outcome_projection = base.projection.clone();
    let mut parts = vec![
        FamilyProjectionV40::Early(Box::new(crate::akroma_early_public_v40::build(
            state,
            base.projection,
            actor,
        )?)),
        FamilyProjectionV40::Sower(Box::new(crate::sower_public_v1::parts_for_ajani_v42(
            state, surface, actor, step, physical, substep,
        )?)),
        FamilyProjectionV40::Frenzied(Box::new(crate::frenzied_public_v1::parts_for_ajani_v42(
            state, surface, actor, step, physical, substep,
        )?)),
        FamilyProjectionV40::Wildwood(Box::new(crate::wildwood_public_v1::parts_for_ajani_v42(
            state, surface, actor, step, physical, substep,
        )?)),
        FamilyProjectionV40::Authority(Box::new(crate::authority_public_v1::parts_for_ajani_v42(
            state, surface, actor, step, physical, substep,
        )?)),
        FamilyProjectionV40::Employment(Box::new(
            crate::employment_public_v1::parts_for_ajani_v42(
                state, surface, actor, step, physical, substep,
            )?,
        )),
        FamilyProjectionV40::Cerberus(Box::new(crate::cerberus_public_v1::parts_for_ajani_v42(
            state, surface, actor, step, physical, substep,
        )?)),
        FamilyProjectionV40::Librarian(Box::new(crate::librarian_public_v1::parts_for_ajani_v42(
            state, surface, actor, step, physical, substep,
        )?)),
        FamilyProjectionV40::ExtraTurn(Box::new(crate::extra_turn_public_v1::parts_for_ajani_v42(
            state, surface, actor, step, physical, substep,
        )?)),
        FamilyProjectionV40::Juggernaut(Box::new(
            crate::juggernaut_public_v1::parts_for_ajani_v42(
                state, surface, actor, step, physical, substep,
            )?,
        )),
        FamilyProjectionV40::Ashroot(Box::new(crate::ashroot_public_v1::parts_for_ajani_v42(
            state, surface, actor, step, physical, substep,
        )?)),
        FamilyProjectionV40::Banishing(Box::new(crate::banishing_public_v1::parts_for_ajani_v42(
            state, surface, actor, step, physical, substep,
        )?)),
        FamilyProjectionV40::Stromkirk(Box::new(crate::stromkirk_public_v1::parts_for_ajani_v42(
            state, surface, actor, step, physical, substep,
        )?)),
        FamilyProjectionV40::Gourmand(Box::new(crate::gourmand_public_v1::parts_for_ajani_v42(
            state, surface, actor, step, physical, substep,
        )?)),
        FamilyProjectionV40::Garruk(Box::new(crate::garruk_public_v1::parts_for_ajani_v42(
            state, surface, actor, step, physical, substep,
        )?)),
        FamilyProjectionV40::Crystal(Box::new(crate::crystal_public_v1::parts_for_ajani_v42(
            state, surface, actor, step, physical, substep,
        )?)),
        FamilyProjectionV40::Arahbo(Box::new(crate::arahbo_public_v1::parts_for_ajani_v42(
            state, surface, actor, step, physical, substep,
        )?)),
        FamilyProjectionV40::Strongbox(Box::new(crate::strongbox_public_v1::parts_for_ajani_v42(
            state, surface, actor, step, physical, substep,
        )?)),
        FamilyProjectionV40::Fiery(Box::new(crate::fiery_observation_v1::parts_for_ajani_v42(
            state, surface, actor, step, physical, substep,
        )?)),
        FamilyProjectionV40::Krenko(Box::new(crate::krenko_observation_v1::parts_for_ajani_v42(
            state, surface, actor, step, physical, substep,
        )?)),
        FamilyProjectionV40::ZulAshur(Box::new(
            crate::zul_ashur_observation_v1::parts_for_ajani_v42(
                state, surface, actor, step, physical, substep,
            )?,
        )),
        FamilyProjectionV40::Banner(Box::new(crate::banner_observation_v1::parts_for_ajani_v42(
            state, surface, actor, step, physical, substep,
        )?)),
        FamilyProjectionV40::Inspiration(Box::new(
            crate::inspiration_observation_v1::parts_for_ajani_v42(
                state, surface, actor, step, physical, substep,
            )?,
        )),
        FamilyProjectionV40::Fishing(Box::new(FishingProjectionV40 {
            parts: crate::fishing_pole_observation_v39::parts_for_ajani_v42(
                state, surface, actor, step, physical, substep,
            )?,
            actions: Vec::new(),
        })),
    ];
    if let Ok(selector) = crate::rules_meaning_game_outcome_protection_v1::live_selector() {
        let protection = crate::game_outcome_protection_public_v1::capture(state, &selector)
            .map_err(|_| RlContractError("invalid current game-outcome facts".into()))?;
        let roots = crate::card_def::CARD_DEFS
            .iter()
            .enumerate()
            .filter(|(_, def)| def.is_executable() && def.game_outcome_protection_v1.is_some())
            .map(|(index, _)| {
                let card_id = u16::try_from(index)
                    .map_err(|_| RlContractError("game-outcome definition index".into()))?;
                crate::rules_meaning_game_outcome_protection_v1::meaning(&selector, card_id)
                    .map_err(|_| RlContractError("unsupported game-outcome printed root".into()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        parts.push(FamilyProjectionV40::GameOutcome(Box::new(
            GameOutcomeProjectionV40 {
                facts: GameOutcomeFactsV40 { protection, roots },
                projection: game_outcome_projection,
            },
        )));
    }
    parts.push(FamilyProjectionV40::Twinflame(Box::new(
        crate::twinflame_observation_v1::parts_for_ajani_v42(
            state, surface, actor, step, physical, substep,
        )?,
    )));
    Ok(parts)
}
pub(crate) fn build_for_scope(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step: u64,
    physical: u64,
    substep: (u32, u32),
    scope: crate::rl::CommonProjectionScopeV42,
) -> Result<Vec<FamilyProjectionV40>, RlContractError> {
    match scope {
        crate::rl::CommonProjectionScopeV42::ExistingV40 => {
            return build(state, surface, actor, step, physical, substep)
        }
        crate::rl::CommonProjectionScopeV42::FullAjaniV42 => {
            return build_for_ajani_v42(state, surface, actor, step, physical, substep)
        }
        crate::rl::CommonProjectionScopeV42::DivineV44 => {}
    }
    let base = crate::rl::common_family_observation_storage_scoped(
        (scope, state),
        surface,
        actor,
        step,
        physical,
        substep,
    )?;
    let game_outcome_projection = base.projection.clone();
    let mut parts = vec![
        FamilyProjectionV40::Early(Box::new(crate::akroma_early_public_v40::build(
            state,
            base.projection,
            actor,
        )?)),
        FamilyProjectionV40::Sower(Box::new(crate::sower_public_v1::parts_for_common_scope(
            state, surface, actor, step, physical, substep, scope,
        )?)),
        FamilyProjectionV40::Frenzied(Box::new(crate::frenzied_public_v1::parts_for_common_scope(
            state, surface, actor, step, physical, substep, scope,
        )?)),
        FamilyProjectionV40::Wildwood(Box::new(crate::wildwood_public_v1::parts_for_common_scope(
            state, surface, actor, step, physical, substep, scope,
        )?)),
        FamilyProjectionV40::Authority(Box::new(
            crate::authority_public_v1::parts_for_common_scope(
                state, surface, actor, step, physical, substep, scope,
            )?,
        )),
        FamilyProjectionV40::Employment(Box::new(
            crate::employment_public_v1::parts_for_common_scope(
                state, surface, actor, step, physical, substep, scope,
            )?,
        )),
        FamilyProjectionV40::Cerberus(Box::new(crate::cerberus_public_v1::parts_for_common_scope(
            state, surface, actor, step, physical, substep, scope,
        )?)),
        FamilyProjectionV40::Librarian(Box::new(
            crate::librarian_public_v1::parts_for_common_scope(
                state, surface, actor, step, physical, substep, scope,
            )?,
        )),
        FamilyProjectionV40::ExtraTurn(Box::new(
            crate::extra_turn_public_v1::parts_for_common_scope(
                state, surface, actor, step, physical, substep, scope,
            )?,
        )),
        FamilyProjectionV40::Juggernaut(Box::new(
            crate::juggernaut_public_v1::parts_for_common_scope(
                state, surface, actor, step, physical, substep, scope,
            )?,
        )),
        FamilyProjectionV40::Ashroot(Box::new(crate::ashroot_public_v1::parts_for_common_scope(
            state, surface, actor, step, physical, substep, scope,
        )?)),
        FamilyProjectionV40::Banishing(Box::new(
            crate::banishing_public_v1::parts_for_common_scope(
                state, surface, actor, step, physical, substep, scope,
            )?,
        )),
        FamilyProjectionV40::Stromkirk(Box::new(
            crate::stromkirk_public_v1::parts_for_common_scope(
                state, surface, actor, step, physical, substep, scope,
            )?,
        )),
        FamilyProjectionV40::Gourmand(Box::new(crate::gourmand_public_v1::parts_for_common_scope(
            state, surface, actor, step, physical, substep, scope,
        )?)),
        FamilyProjectionV40::Garruk(Box::new(crate::garruk_public_v1::parts_for_common_scope(
            state, surface, actor, step, physical, substep, scope,
        )?)),
        FamilyProjectionV40::Crystal(Box::new(crate::crystal_public_v1::parts_for_common_scope(
            state, surface, actor, step, physical, substep, scope,
        )?)),
        FamilyProjectionV40::Arahbo(Box::new(crate::arahbo_public_v1::parts_for_common_scope(
            state, surface, actor, step, physical, substep, scope,
        )?)),
        FamilyProjectionV40::Strongbox(Box::new(
            crate::strongbox_public_v1::parts_for_common_scope(
                state, surface, actor, step, physical, substep, scope,
            )?,
        )),
        FamilyProjectionV40::Fiery(Box::new(
            crate::fiery_observation_v1::parts_for_common_scope(
                state, surface, actor, step, physical, substep, scope,
            )?,
        )),
        FamilyProjectionV40::Krenko(Box::new(
            crate::krenko_observation_v1::parts_for_common_scope(
                state, surface, actor, step, physical, substep, scope,
            )?,
        )),
        FamilyProjectionV40::ZulAshur(Box::new(
            crate::zul_ashur_observation_v1::parts_for_common_scope(
                state, surface, actor, step, physical, substep, scope,
            )?,
        )),
        FamilyProjectionV40::Banner(Box::new(
            crate::banner_observation_v1::parts_for_common_scope(
                state, surface, actor, step, physical, substep, scope,
            )?,
        )),
        FamilyProjectionV40::Inspiration(Box::new(
            crate::inspiration_observation_v1::parts_for_common_scope(
                state, surface, actor, step, physical, substep, scope,
            )?,
        )),
        FamilyProjectionV40::Fishing(Box::new(FishingProjectionV40 {
            parts: crate::fishing_pole_observation_v39::parts_for_common_scope(
                state, surface, actor, step, physical, substep, scope,
            )?,
            actions: Vec::new(),
        })),
    ];
    if let Ok(selector) = crate::rules_meaning_game_outcome_protection_v1::live_selector() {
        let protection = crate::game_outcome_protection_public_v1::capture(state, &selector)
            .map_err(|_| RlContractError("invalid current game-outcome facts".into()))?;
        let roots = crate::card_def::CARD_DEFS
            .iter()
            .enumerate()
            .filter(|(_, def)| def.is_executable() && def.game_outcome_protection_v1.is_some())
            .map(|(index, _)| {
                let card_id = u16::try_from(index)
                    .map_err(|_| RlContractError("game-outcome definition index".into()))?;
                crate::rules_meaning_game_outcome_protection_v1::meaning(&selector, card_id)
                    .map_err(|_| RlContractError("unsupported game-outcome printed root".into()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        parts.push(FamilyProjectionV40::GameOutcome(Box::new(
            GameOutcomeProjectionV40 {
                facts: GameOutcomeFactsV40 { protection, roots },
                projection: game_outcome_projection,
            },
        )));
    }
    parts.push(FamilyProjectionV40::Twinflame(Box::new(
        crate::twinflame_observation_v1::parts_for_common_scope(
            state, surface, actor, step, physical, substep, scope,
        )?,
    )));
    Ok(parts)
}
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    if let Ok(selector) = crate::rules_meaning_game_outcome_protection_v1::live_selector() {
        crate::game_outcome_protection_public_v1::capture(state, &selector)
            .map_err(|_| "invalid game-outcome current public authority".to_string())?;
    }
    crate::fishing_pole_v39::lifecycle::validate_state(state)?;
    crate::planeswalker_v1::validate_heartfire_recipients_v12(state)?;
    crate::twinflame_observation_v1::validate_projection_authority(state).map_err(|e| e.0)?;
    crate::effect::validate_pending_effect_choice(state)?;
    crate::sower_restriction_capture_v1::active(state).map_err(|e| e.0)?;
    crate::effect::frenzied_goblin_resolution_v1::validate(state)?;
    crate::wildwood_capture_v1::validate_state(state)?;
    crate::authority_public_capture_v1::validate_state(state)?;
    crate::effect::temporary_control_v1::authority::validate(state)?;
    crate::cerberus_capture_v1::validate_state(state)?;
    crate::librarian_activation_v1::validate_state(state)?;
    crate::raid_entry_v1::validate_state(state)?;
    crate::gorehorn_damage_v1::validate_state(state)?;
    crate::soul_shackled_zombie_v1::validate_state(state)?;
    crate::extra_turn_v1::validate(state)?;
    crate::ashroot_animist_v1::validate_state(state)?;
    crate::banishing_light_v1::validate_state(state)?;
    crate::garruks_uprising_v1::validate_state(state)?;
    crate::arahbo_v1::validate_state(state)?;
    crate::effect::crystal_prevention_v1::validate(state)?;
    crate::equipment_damage_source_v4::validate_state(state)?;
    crate::krenko_v1::validate_state(state)?;
    crate::zul_ashur_v1::validate_state(state)?;
    crate::heraldic_banner_v1::validate_state(state)?;
    crate::engine::validate_legacy_variable_cast_targets(state)?;
    Ok(())
}
pub(crate) fn validate_state_for_ajani_v42(state: &GameState) -> Result<(), String> {
    validate_state_full_scope(state, false)
}
pub(crate) fn validate_state_for_divine_v44(state: &GameState) -> Result<(), String> {
    validate_state_full_scope(state, true)
}
fn validate_state_full_scope(state: &GameState, divine: bool) -> Result<(), String> {
    if let Ok(selector) = crate::rules_meaning_game_outcome_protection_v1::live_selector() {
        crate::game_outcome_protection_public_v1::capture(state, &selector)
            .map_err(|_| "invalid game-outcome current public authority".to_string())?;
    }
    crate::ajani_full_v42::lifecycle::validate_state(state)?;
    crate::fishing_pole_v39::lifecycle::validate_state(state)?;
    crate::planeswalker_v1::validate_full_ajani_recipients_v42(state)?;
    crate::twinflame_observation_v1::validate_projection_authority(state).map_err(|e| e.0)?;
    crate::effect::validate_pending_effect_choice(state)?;
    crate::sower_restriction_capture_v1::active(state).map_err(|e| e.0)?;
    crate::effect::frenzied_goblin_resolution_v1::validate(state)?;
    crate::wildwood_capture_v1::validate_state(state)?;
    crate::authority_public_capture_v1::validate_state(state)?;
    crate::effect::temporary_control_v1::authority::validate(state)?;
    crate::cerberus_capture_v1::validate_state(state)?;
    crate::librarian_activation_v1::validate_state(state)?;
    crate::raid_entry_v1::validate_state(state)?;
    crate::gorehorn_damage_v1::validate_state(state)?;
    crate::soul_shackled_zombie_v1::validate_state(state)?;
    crate::extra_turn_v1::validate(state)?;
    crate::ashroot_animist_v1::validate_state(state)?;
    crate::banishing_light_v1::validate_state(state)?;
    crate::garruks_uprising_v1::validate_state(state)?;
    crate::arahbo_v1::validate_state(state)?;
    crate::effect::crystal_prevention_v1::validate(state)?;
    crate::equipment_damage_source_v4::validate_state(state)?;
    crate::krenko_v1::validate_state(state)?;
    crate::zul_ashur_v1::validate_state(state)?;
    crate::heraldic_banner_v1::validate_state(state)?;
    if divine {
        crate::divine_public_v44::validate_state(state)?;
    } else {
        crate::engine::validate_legacy_variable_cast_targets(state)?;
    }
    Ok(())
}

pub(crate) fn merge_projection(
    base: &crate::rl::PublicObservationProjectionV5,
    parts: &[FamilyProjectionV40],
) -> Result<crate::rl::PublicObservationProjectionV5, RlContractError> {
    let mut merged = base.clone();
    for part in parts {
        let incoming = part.projection();
        let raw = &base.surface.engine_context;
        let proposed = &incoming.surface.engine_context;
        if raw.pending_triggers.len() != proposed.pending_triggers.len() {
            return Err(RlContractError(
                "mixed family changed pending roster shape".into(),
            ));
        }
        let mut permitted = base.clone();
        permitted.surface.engine_context.current_stage = proposed.current_stage;
        for (row, next) in permitted
            .surface
            .engine_context
            .pending_triggers
            .iter_mut()
            .zip(&proposed.pending_triggers)
        {
            row.source = next.source.clone();
        }
        if permitted != *incoming {
            return Err(RlContractError(
                "mixed family needs an additional explicit projection field".into(),
            ));
        }
        if proposed.current_stage != raw.current_stage {
            let stage = &mut merged.surface.engine_context.current_stage;
            if *stage != raw.current_stage && *stage != proposed.current_stage {
                return Err(RlContractError(
                    "mixed families disagree on current stage".into(),
                ));
            }
            *stage = proposed.current_stage;
        }
        for (index, next) in proposed.pending_triggers.iter().enumerate() {
            if next.source != raw.pending_triggers[index].source {
                let current = &mut merged.surface.engine_context.pending_triggers[index].source;
                if *current != raw.pending_triggers[index].source && *current != next.source {
                    return Err(RlContractError(
                        "mixed families disagree on captured observer".into(),
                    ));
                }
                *current = next.source.clone();
            }
        }
    }
    Ok(merged)
}
