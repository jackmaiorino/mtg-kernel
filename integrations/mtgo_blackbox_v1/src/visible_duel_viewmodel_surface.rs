use crate::MtgoContractErrorV1;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub const MTGO_VISIBLE_DUEL_VIEWMODEL_SURFACE_SCHEMA_V1: u32 = 1;
pub const MTGO_VISIBLE_DUEL_VIEWMODEL_CANDIDATE_SURFACE_COMMITMENT_V1: &str =
    "7d3ce84b73538fa66d4c755ec21740e5b95680630a15ec5caba8c2ab181851bc";

const SURFACE_KIND_V1: &str = "mtgo_visible_duel_viewmodel_candidate_surface_v1";
const INFORMATION_BOUNDARY_V1: &str = "seated_player_visible_ui_equivalent_only_v1";
const SURFACE_DOMAIN_V1: &[u8] = b"mtgo-visible-duel-viewmodel-candidate-surface-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoVisibleViewModelPropertyContextV1 {
    AlwaysVisibleDuelChrome,
    VisiblePlayerPanel,
    SeatedPlayerOrPublicOrExplicitlyVisibleZone,
    VisibleCardPresentation,
    FaceVisibleCardOnly,
    VisiblePromptOnly,
    VisibleControlOnly,
    VisibleActionMenuOnly,
    TransientTraversalOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleViewModelAssemblyPinV1 {
    pub file_name: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleViewModelPropertyCandidateV1 {
    pub assembly_file_name: String,
    pub declaring_type: String,
    pub property_name: String,
    pub property_type: String,
    pub context: MtgoVisibleViewModelPropertyContextV1,
    pub visible_ui_counterpart: String,
    pub intended_projection_fields: Vec<String>,
    pub exact_ui_corpus_qualification_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoForbiddenViewModelPropertyV1 {
    pub assembly_file_name: String,
    pub declaring_type: String,
    pub property_name: String,
    pub reason: String,
}

/// A version-pinned metadata audit artifact. It identifies getters that may be
/// qualified against reviewed UI frames. It contains no live client value and
/// grants no acquisition, semantic, scoring, input, event, or spending authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleDuelViewModelCandidateSurfaceV1 {
    pub schema_version: u32,
    pub surface_kind: String,
    pub information_boundary: String,
    pub product_version: String,
    pub file_version: String,
    pub executable_sha256: String,
    pub executable_signer_thumbprint: String,
    pub mtgo_exe_manifest_sha256: String,
    pub mtgo_application_manifest_sha256: String,
    pub assemblies: Vec<MtgoVisibleViewModelAssemblyPinV1>,
    pub candidates: Vec<MtgoVisibleViewModelPropertyCandidateV1>,
    pub forbidden_properties: Vec<MtgoForbiddenViewModelPropertyV1>,
    pub complete_duel_projection_demonstrated: bool,
    pub live_producer_attested: bool,
    pub safe_for_live_semantic_evidence: bool,
    pub safe_for_model_scoring: bool,
    pub safe_for_input: bool,
    pub permits_event_entry: bool,
    pub permits_spending: bool,
}

/// Exact checked copy of the compiled candidate surface. This remains
/// checked-untrusted because metadata names do not prove that values match the UI.
pub struct CheckedUntrustedMtgoVisibleDuelViewModelCandidateSurfaceV1 {
    manifest: MtgoVisibleDuelViewModelCandidateSurfaceV1,
    commitment_sha256: String,
}

impl CheckedUntrustedMtgoVisibleDuelViewModelCandidateSurfaceV1 {
    pub fn commitment_sha256_v1(&self) -> &str {
        &self.commitment_sha256
    }

    pub fn candidate_property_count_v1(&self) -> usize {
        self.manifest.candidates.len()
    }

    pub fn forbidden_property_count_v1(&self) -> usize {
        self.manifest.forbidden_properties.len()
    }

    pub fn product_version_v1(&self) -> &str {
        &self.manifest.product_version
    }

    pub fn complete_duel_projection_demonstrated_v1(&self) -> bool {
        false
    }

    pub fn live_producer_attested_v1(&self) -> bool {
        false
    }

    pub fn safe_for_live_semantic_evidence_v1(&self) -> bool {
        false
    }

    pub fn safe_for_model_scoring_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

/// Returns the exact reviewed metadata candidate list for installed client
/// version 3.4.158.4691. Candidate status means only "eligible to test against
/// the UI corpus". It does not mean the property is admitted for live use.
pub fn mtgo_visible_duel_viewmodel_candidate_surface_v1(
) -> MtgoVisibleDuelViewModelCandidateSurfaceV1 {
    let mut candidates = vec![
        candidate(
            "Card.dll",
            "Shiny.Card.ViewModels.CardViewModel",
            "CurrentDungeonRoom",
            "Int32",
            MtgoVisibleViewModelPropertyContextV1::VisibleCardPresentation,
            "rendered dungeon-room highlight, used only to reject an unrepresented state",
            &[],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.DuelSceneViewModel",
            "CardSelection",
            "CardSelectionViewModel",
            MtgoVisibleViewModelPropertyContextV1::TransientTraversalOnly,
            "visible select-from-list surface, used only to reject an unrepresented choice",
            &[],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.DuelSceneViewModel",
            "CardSelectorDialog",
            "CardSelectorDialogViewModel",
            MtgoVisibleViewModelPropertyContextV1::TransientTraversalOnly,
            "visible card-selector dialog, used only to reject an unrepresented choice",
            &[],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.DuelSceneViewModel",
            "CardSelectors",
            "CardSelectorManager",
            MtgoVisibleViewModelPropertyContextV1::TransientTraversalOnly,
            "visible card-selector collection, used only to reject an unrepresented choice",
            &[],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.DuelSceneViewModel",
            "CurrentPhase",
            "WotC.MtGO.Client.Model.Play.GamePhase",
            MtgoVisibleViewModelPropertyContextV1::AlwaysVisibleDuelChrome,
            "highlighted phase ladder step",
            &["current_state.phase"],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.DuelSceneViewModel",
            "GameTurnText",
            "String",
            MtgoVisibleViewModelPropertyContextV1::AlwaysVisibleDuelChrome,
            "rendered turn label",
            &["current_state.turn"],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.DuelSceneViewModel",
            "IsCommander",
            "Boolean",
            MtgoVisibleViewModelPropertyContextV1::AlwaysVisibleDuelChrome,
            "commander duel layout, used only to reject an unsupported duel variant",
            &[],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.DuelSceneViewModel",
            "IsPileZoneActive",
            "Boolean",
            MtgoVisibleViewModelPropertyContextV1::VisiblePromptOnly,
            "visible pile-choice surface, used only to reject an unrepresented choice",
            &[],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.DuelSceneViewModel",
            "IsPlanechase",
            "Boolean",
            MtgoVisibleViewModelPropertyContextV1::AlwaysVisibleDuelChrome,
            "planechase duel layout, used only to reject an unsupported duel variant",
            &[],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.DuelSceneViewModel",
            "IsWishingFromSideboard",
            "Boolean",
            MtgoVisibleViewModelPropertyContextV1::VisiblePromptOnly,
            "visible wish-from-sideboard flow, used only to reject an unrepresented choice",
            &[],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.DuelSceneViewModel",
            "LocalTriggersPanelEnabled",
            "Boolean",
            MtgoVisibleViewModelPropertyContextV1::VisiblePromptOnly,
            "visible seated-player trigger panel, used only to reject unrepresented ordering",
            &[],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.DuelSceneViewModel",
            "OpponentTriggersPanelEnabled",
            "Boolean",
            MtgoVisibleViewModelPropertyContextV1::VisiblePromptOnly,
            "visible opponent trigger panel, used only to reject unrepresented ordering",
            &[],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.DuelSceneViewModel",
            "Players",
            "ObservableCollection<PlayerViewModel>",
            MtgoVisibleViewModelPropertyContextV1::TransientTraversalOnly,
            "the two visible player panels",
            &[],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.DuelSceneViewModel",
            "PromptBox",
            "PromptBoxViewModel",
            MtgoVisibleViewModelPropertyContextV1::TransientTraversalOnly,
            "visible prompt box",
            &[],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.DuelSceneViewModel",
            "StormCounterVisible",
            "Boolean",
            MtgoVisibleViewModelPropertyContextV1::AlwaysVisibleDuelChrome,
            "visible storm counter, used only to reject an unrepresented state",
            &[],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.DuelSceneViewModel",
            "TemporaryZones",
            "ObservableCollection<TemporaryZoneViewModel>",
            MtgoVisibleViewModelPropertyContextV1::TransientTraversalOnly,
            "temporary public-zone windows, used only to reject unrepresented visible surfaces",
            &[],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.DuelSceneViewModel",
            "ThreePilePanelEnabled",
            "Boolean",
            MtgoVisibleViewModelPropertyContextV1::VisiblePromptOnly,
            "visible three-pile chooser, used only to reject an unrepresented choice",
            &[],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.DuelSceneViewModel",
            "TwoPilePanelEnabled",
            "Boolean",
            MtgoVisibleViewModelPropertyContextV1::VisiblePromptOnly,
            "visible two-pile chooser, used only to reject an unrepresented choice",
            &[],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.CardSelectionViewModel",
            "Visible",
            "Boolean",
            MtgoVisibleViewModelPropertyContextV1::VisiblePromptOnly,
            "select-from-list surface visibility",
            &[],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.CardSelectorDialogViewModel",
            "Visible",
            "Boolean",
            MtgoVisibleViewModelPropertyContextV1::VisiblePromptOnly,
            "card-selector dialog visibility",
            &[],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.CardSelectorManager",
            "Collection",
            "ObservableCollection<CardSelectorViewModel>",
            MtgoVisibleViewModelPropertyContextV1::VisiblePromptOnly,
            "rendered card-selector controls",
            &[],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.TemporaryZoneViewModel",
            "IsVisible",
            "Boolean",
            MtgoVisibleViewModelPropertyContextV1::SeatedPlayerOrPublicOrExplicitlyVisibleZone,
            "temporary zone window visibility",
            &[],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.DuelSceneViewModel",
            "StackZone",
            "ZoneViewModel",
            MtgoVisibleViewModelPropertyContextV1::TransientTraversalOnly,
            "visible stack panel",
            &[],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.PlayerViewModel",
            "Name",
            "String",
            MtgoVisibleViewModelPropertyContextV1::TransientTraversalOnly,
            "rendered player name used only to establish relative panel role",
            &[],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.PlayerViewModel",
            "LocalPlayer",
            "Boolean",
            MtgoVisibleViewModelPropertyContextV1::TransientTraversalOnly,
            "the seated player's visibly identifiable panel role",
            &[],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.PlayerViewModel",
            "Active",
            "Boolean",
            MtgoVisibleViewModelPropertyContextV1::VisiblePlayerPanel,
            "active-player panel highlight",
            &["current_state.active_player"],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.PlayerViewModel",
            "MatActive",
            "Boolean",
            MtgoVisibleViewModelPropertyContextV1::VisiblePlayerPanel,
            "priority mat highlight",
            &["current_state.priority_player"],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.PlayerViewModel",
            "Health",
            "Int32",
            MtgoVisibleViewModelPropertyContextV1::VisiblePlayerPanel,
            "visible life total",
            &["current_state.life_totals"],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.PlayerViewModel",
            "HasEnergyCounters",
            "Boolean",
            MtgoVisibleViewModelPropertyContextV1::VisiblePlayerPanel,
            "visible energy-counter badge presence, used only to reject an unrepresented state",
            &[],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.PlayerViewModel",
            "HasExperienceCounters",
            "Boolean",
            MtgoVisibleViewModelPropertyContextV1::VisiblePlayerPanel,
            "visible experience-counter badge presence, used only to reject an unrepresented state",
            &[],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.PlayerViewModel",
            "HasPoisonCounters",
            "Boolean",
            MtgoVisibleViewModelPropertyContextV1::VisiblePlayerPanel,
            "visible poison-counter badge presence, used only to reject an unrepresented state",
            &[],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.PlayerViewModel",
            "HasRadCounters",
            "Boolean",
            MtgoVisibleViewModelPropertyContextV1::VisiblePlayerPanel,
            "visible radiation-counter badge presence, used only to reject an unrepresented state",
            &[],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.PlayerViewModel",
            "HandTotal",
            "Int32",
            MtgoVisibleViewModelPropertyContextV1::VisiblePlayerPanel,
            "visible hand count",
            &["current_state.hand_counts"],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.PlayerViewModel",
            "DeckTotal",
            "Int32",
            MtgoVisibleViewModelPropertyContextV1::VisiblePlayerPanel,
            "visible library count",
            &["current_state.library_counts"],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.PlayerViewModel",
            "CompanionZone",
            "ZoneViewModel",
            MtgoVisibleViewModelPropertyContextV1::TransientTraversalOnly,
            "visible companion panel, used only to reject an unrepresented state",
            &[],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.PlayerViewModel",
            "HandZone",
            "ZoneViewModel",
            MtgoVisibleViewModelPropertyContextV1::TransientTraversalOnly,
            "visible hand zone; card traversal is seated-player-only unless explicitly revealed",
            &[],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.PlayerViewModel",
            "LibraryZone",
            "ZoneViewModel",
            MtgoVisibleViewModelPropertyContextV1::TransientTraversalOnly,
            "visible library count and explicitly revealed library cards only",
            &[],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.PlayerViewModel",
            "GraveyardZone",
            "ZoneViewModel",
            MtgoVisibleViewModelPropertyContextV1::TransientTraversalOnly,
            "public graveyard zone",
            &[],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.PlayerViewModel",
            "ExileZone",
            "ZoneViewModel",
            MtgoVisibleViewModelPropertyContextV1::TransientTraversalOnly,
            "public or visibly face-down exile zone",
            &[],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.PlayerViewModel",
            "RevealedZone",
            "ZoneViewModel",
            MtgoVisibleViewModelPropertyContextV1::TransientTraversalOnly,
            "cards explicitly revealed by the client",
            &[],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.PlayerViewModel",
            "ShieldsZone",
            "ZoneViewModel",
            MtgoVisibleViewModelPropertyContextV1::TransientTraversalOnly,
            "rendered emblem zone used only to derive a visible Initiative holder",
            &[],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.PlayerViewModel",
            "BattlefieldCards",
            "ObservableCollection<DuelSceneCardViewModel>",
            MtgoVisibleViewModelPropertyContextV1::SeatedPlayerOrPublicOrExplicitlyVisibleZone,
            "visible battlefield cards",
            &["current_state.battlefield"],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.PlayerViewModel",
            "ManaPoolItems",
            "ObservableCollection<ManaPoolItemViewModel>",
            MtgoVisibleViewModelPropertyContextV1::VisiblePlayerPanel,
            "visible mana-pool display",
            &["current_state.mana_pools"],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.ManaPoolItemViewModel",
            "ColorString",
            "String",
            MtgoVisibleViewModelPropertyContextV1::VisiblePlayerPanel,
            "visible mana color label",
            &["current_state.mana_pools"],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.ManaPoolItemViewModel",
            "Count",
            "Int32",
            MtgoVisibleViewModelPropertyContextV1::VisiblePlayerPanel,
            "visible mana amount",
            &["current_state.mana_pools"],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.ZoneViewModel",
            "IsVisible",
            "Boolean",
            MtgoVisibleViewModelPropertyContextV1::SeatedPlayerOrPublicOrExplicitlyVisibleZone,
            "zone panel visibility",
            &[],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.ZoneViewModel",
            "Count",
            "Int32",
            MtgoVisibleViewModelPropertyContextV1::SeatedPlayerOrPublicOrExplicitlyVisibleZone,
            "visible zone count",
            &["current_state.hand_counts", "current_state.library_counts"],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.ZoneViewModel",
            "Cards",
            "IObservableEnumerable<DuelSceneCardViewModel>",
            MtgoVisibleViewModelPropertyContextV1::SeatedPlayerOrPublicOrExplicitlyVisibleZone,
            "cards rendered in a public or explicitly open zone",
            &[
                "current_state.own_hand",
                "current_state.graveyards",
                "current_state.exile",
                "current_state.stack",
                "current_state.known_library_cards",
                "current_state.known_hand_cards",
            ],
        ),
        candidate(
            "Card.dll",
            "Shiny.Card.ViewModels.CardViewModel",
            "CardFrameID",
            "Shiny.Card.Enums.FrameStyle",
            MtgoVisibleViewModelPropertyContextV1::VisibleCardPresentation,
            "rendered card-frame style, reduced to the Initiative emblem predicate only",
            &["current_state.initiative"],
        ),
        candidate(
            "Card.dll",
            "Shiny.Card.ViewModels.CardViewModel",
            "IsFaceDown",
            "Boolean",
            MtgoVisibleViewModelPropertyContextV1::VisibleCardPresentation,
            "visible card back or face",
            &[],
        ),
        candidate(
            "Card.dll",
            "Shiny.Card.ViewModels.CardViewModel",
            "Name",
            "String",
            MtgoVisibleViewModelPropertyContextV1::FaceVisibleCardOnly,
            "rendered card title",
            &["current_state visible card names"],
        ),
        candidate(
            "Card.dll",
            "Shiny.Card.ViewModels.CardViewModel",
            "IsTapped",
            "Boolean",
            MtgoVisibleViewModelPropertyContextV1::VisibleCardPresentation,
            "rendered card rotation",
            &["current_state.battlefield.tapped"],
        ),
        candidate(
            "Card.dll",
            "Shiny.Card.ViewModels.CardViewModel",
            "Power",
            "Nullable<Int32>",
            MtgoVisibleViewModelPropertyContextV1::FaceVisibleCardOnly,
            "rendered power",
            &["current_state.battlefield.visible_effective_power"],
        ),
        candidate(
            "Card.dll",
            "Shiny.Card.ViewModels.CardViewModel",
            "Toughness",
            "Nullable<Int32>",
            MtgoVisibleViewModelPropertyContextV1::FaceVisibleCardOnly,
            "rendered toughness",
            &["current_state.battlefield.visible_effective_toughness"],
        ),
        candidate(
            "Card.dll",
            "Shiny.Card.ViewModels.CardViewModel",
            "CurrentDamage",
            "Int32",
            MtgoVisibleViewModelPropertyContextV1::VisibleCardPresentation,
            "visible damage overlay",
            &["current_state.battlefield.marked_damage"],
        ),
        candidate(
            "Card.dll",
            "Shiny.Card.ViewModels.CardViewModel",
            "IsAttacking",
            "Boolean",
            MtgoVisibleViewModelPropertyContextV1::VisibleCardPresentation,
            "card rendered in the attack lane",
            &["current_state.combat"],
        ),
        candidate(
            "Card.dll",
            "Shiny.Card.ViewModels.CardViewModel",
            "IsBlocking",
            "Boolean",
            MtgoVisibleViewModelPropertyContextV1::VisibleCardPresentation,
            "visible blocker association",
            &["current_state.combat"],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel",
            "IsSpeedEmblem",
            "Boolean",
            MtgoVisibleViewModelPropertyContextV1::VisibleCardPresentation,
            "rendered speed emblem, used only to reject an unrepresented state",
            &[],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel",
            "IsToken",
            "Boolean",
            MtgoVisibleViewModelPropertyContextV1::VisibleCardPresentation,
            "visible token presentation",
            &["current_state.battlefield.is_token"],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel",
            "RingTemptationCounter",
            "Int32",
            MtgoVisibleViewModelPropertyContextV1::VisibleCardPresentation,
            "rendered Ring temptation level, used only to reject an unrepresented state",
            &[],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel",
            "SpeedCounter",
            "Int32",
            MtgoVisibleViewModelPropertyContextV1::VisibleCardPresentation,
            "rendered speed level, used only to reject an unrepresented state",
            &[],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel",
            "VisibleCounters",
            "IList<CardCounterViewModel>",
            MtgoVisibleViewModelPropertyContextV1::VisibleCardPresentation,
            "rendered counter badges",
            &["current_state.battlefield.counters"],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.CardCounterViewModel",
            "Quantity",
            "Int32",
            MtgoVisibleViewModelPropertyContextV1::VisibleCardPresentation,
            "rendered counter quantity",
            &["current_state.battlefield.counters"],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.CardCounterViewModel",
            "Type",
            "WotC.MtGO.Client.Model.Play.Counter",
            MtgoVisibleViewModelPropertyContextV1::VisibleCardPresentation,
            "rendered counter type",
            &["current_state.battlefield.counters"],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel",
            "CardAttachedTo",
            "DuelSceneCardViewModel",
            MtgoVisibleViewModelPropertyContextV1::VisibleCardPresentation,
            "visible attachment grouping",
            &["current_state.visible_object_relations"],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.PromptBoxViewModel",
            "IsPromptBoxActive",
            "Boolean",
            MtgoVisibleViewModelPropertyContextV1::VisiblePromptOnly,
            "active visible prompt",
            &[],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.PromptBoxViewModel",
            "Text",
            "String",
            MtgoVisibleViewModelPropertyContextV1::VisiblePromptOnly,
            "rendered prompt text",
            &[],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.PromptBoxViewModel",
            "StandardButtons",
            "IExtendedObservableCollection<OptionButton>",
            MtgoVisibleViewModelPropertyContextV1::VisiblePromptOnly,
            "rendered standard prompt controls",
            &["ordered_legal_actions"],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.OptionButton",
            "Name",
            "String",
            MtgoVisibleViewModelPropertyContextV1::VisibleControlOnly,
            "rendered control label",
            &["ordered_legal_actions"],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.OptionButton",
            "Enabled",
            "Boolean",
            MtgoVisibleViewModelPropertyContextV1::VisibleControlOnly,
            "control enabled state",
            &["ordered_legal_actions"],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.OptionButton",
            "Visible",
            "Boolean",
            MtgoVisibleViewModelPropertyContextV1::VisibleControlOnly,
            "control visible state",
            &["ordered_legal_actions"],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.GroupCardAction",
            "Name",
            "String",
            MtgoVisibleViewModelPropertyContextV1::VisibleActionMenuOnly,
            "rendered card action-menu label",
            &["ordered_legal_actions"],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.GroupCardAction",
            "GroupName",
            "String",
            MtgoVisibleViewModelPropertyContextV1::VisibleActionMenuOnly,
            "rendered action group label",
            &["ordered_legal_actions"],
        ),
        candidate(
            "DuelScene.dll",
            "Shiny.Play.Duel.GroupCardAction",
            "ModeOptions",
            "String[]",
            MtgoVisibleViewModelPropertyContextV1::VisibleActionMenuOnly,
            "rendered mode option labels",
            &["ordered_legal_actions"],
        ),
    ];
    candidates.sort_by_key(property_key);

    let mut forbidden_properties = vec![
        forbidden(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.DuelSceneViewModel",
            "Game",
            "unrestricted backing game object",
        ),
        forbidden(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.DuelSceneViewModel",
            "GameId",
            "internal match identifier",
        ),
        forbidden(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.DuelSceneViewModel",
            "InteractionState",
            "non-rendered interaction state",
        ),
        forbidden(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.DuelSceneViewModel",
            "WaitingForServer",
            "transport and server state",
        ),
        forbidden(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.PlayerViewModel",
            "GamePlayer",
            "unrestricted backing player object",
        ),
        forbidden(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.PlayerViewModel",
            "Owner",
            "backing owner object",
        ),
        forbidden(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.PlayerViewModel",
            "UserViewModel",
            "account and client metadata",
        ),
        forbidden(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.ZoneViewModel",
            "ModelZone",
            "unrestricted backing zone object",
        ),
        forbidden(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel",
            "GameCard",
            "unrestricted backing card object",
        ),
        forbidden(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel",
            "OtherFaceGameCardDefinition",
            "potentially hidden other-face definition",
        ),
        forbidden(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel",
            "Owner",
            "backing owner object",
        ),
        forbidden(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel",
            "Controller",
            "backing controller object",
        ),
        forbidden(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel",
            "Timestamp",
            "internal ordering metadata",
        ),
        forbidden(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel",
            "CorrectedTimestamp",
            "internal ordering metadata",
        ),
        forbidden(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel",
            "PendingTargets",
            "non-rendered target state",
        ),
        forbidden(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel",
            "Actions",
            "raw client action collection",
        ),
        forbidden(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.OptionButton",
            "Action",
            "raw client action object",
        ),
        forbidden(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.PromptBoxViewModel",
            "PromptedPlayer",
            "backing player object",
        ),
        forbidden(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.PromptBoxViewModel",
            "CurrentTargetList",
            "raw target objects",
        ),
        forbidden(
            "DuelScene.dll",
            "Shiny.Play.Duel.ViewModel.PromptBoxViewModel",
            "ActionModifierDictionary",
            "internal action metadata",
        ),
        forbidden(
            "DuelScene.dll",
            "Shiny.Play.Duel.GroupCardAction",
            "Card",
            "unrestricted backing card object",
        ),
        forbidden(
            "DuelScene.dll",
            "Shiny.Play.Duel.GroupCardAction",
            "Targets",
            "raw target objects",
        ),
        forbidden(
            "DuelScene.dll",
            "Shiny.Play.Duel.GroupCardAction",
            "ModeIDs",
            "internal identifiers",
        ),
        forbidden(
            "DuelScene.dll",
            "Shiny.Play.Duel.GroupCardAction",
            "ActionFlags",
            "non-rendered internal flags",
        ),
        forbidden(
            "Card.dll",
            "Shiny.Card.ViewModels.BaseCardViewModel",
            "CardDefinition",
            "card-database backing object",
        ),
        forbidden(
            "Card.dll",
            "Shiny.Card.ViewModels.CardViewModel",
            "CloneSource",
            "backing source object",
        ),
        forbidden(
            "Card.dll",
            "Shiny.Card.ViewModels.CardViewModel",
            "SourceCardForAbilityOrEffect",
            "backing source object",
        ),
        forbidden(
            "Card.dll",
            "Shiny.Card.ViewModels.CardViewModel",
            "Wrapper",
            "unrestricted backing wrapper",
        ),
    ];
    forbidden_properties.sort_by_key(forbidden_key);

    MtgoVisibleDuelViewModelCandidateSurfaceV1 {
        schema_version: MTGO_VISIBLE_DUEL_VIEWMODEL_SURFACE_SCHEMA_V1,
        surface_kind: SURFACE_KIND_V1.to_owned(),
        information_boundary: INFORMATION_BOUNDARY_V1.to_owned(),
        product_version: "3.4.158.4691".to_owned(),
        file_version: "3.4.158.4691".to_owned(),
        executable_sha256: "bb9c1a189674cd7333b1d997259109576cafe78767f0f11badaad2203c388e92"
            .to_owned(),
        executable_signer_thumbprint: "e9d9e2b989f90555b04c506fddf889c7aba7ac30".to_owned(),
        mtgo_exe_manifest_sha256:
            "be149658de7f525c01ec1977bea41d617226a72e30add6e1f2d5597c95398157".to_owned(),
        mtgo_application_manifest_sha256:
            "0d0c91b933022797e2ab2c24a5904867a58918d96217cf0617b4956968ac007a".to_owned(),
        assemblies: vec![
            MtgoVisibleViewModelAssemblyPinV1 {
                file_name: "Card.dll".to_owned(),
                sha256: "071338a98d845d5c8db6ebd2f3c847e38ad548f50ba11d2a36973438cdec2ea8"
                    .to_owned(),
            },
            MtgoVisibleViewModelAssemblyPinV1 {
                file_name: "DuelScene.dll".to_owned(),
                sha256: "72b99e1169f9f9445a510b2dae52f9212fb7300c2483b8bc8e02f5760f11904e"
                    .to_owned(),
            },
        ],
        candidates,
        forbidden_properties,
        complete_duel_projection_demonstrated: false,
        live_producer_attested: false,
        safe_for_live_semantic_evidence: false,
        safe_for_model_scoring: false,
        safe_for_input: false,
        permits_event_entry: false,
        permits_spending: false,
    }
}

pub fn check_untrusted_visible_duel_viewmodel_candidate_surface_v1(
    manifest: MtgoVisibleDuelViewModelCandidateSurfaceV1,
) -> Result<CheckedUntrustedMtgoVisibleDuelViewModelCandidateSurfaceV1, MtgoContractErrorV1> {
    validate_surface_shape_v1(&manifest)?;
    if manifest != mtgo_visible_duel_viewmodel_candidate_surface_v1() {
        return Err(error_v1(
            "visible_duel_viewmodel_surface_mismatch",
            "candidate surface must exactly match the compiled version-pinned metadata audit",
        ));
    }
    let canonical = serde_json::to_vec(&manifest).map_err(|error| {
        error_v1(
            "visible_duel_viewmodel_surface_serialization",
            error.to_string(),
        )
    })?;
    let commitment_sha256 = commitment_v1(SURFACE_DOMAIN_V1, &[&canonical]);
    if commitment_sha256 != MTGO_VISIBLE_DUEL_VIEWMODEL_CANDIDATE_SURFACE_COMMITMENT_V1 {
        return Err(error_v1(
            "visible_duel_viewmodel_surface_commitment",
            "compiled candidate surface changed without an updated reviewed commitment",
        ));
    }
    Ok(CheckedUntrustedMtgoVisibleDuelViewModelCandidateSurfaceV1 {
        manifest,
        commitment_sha256,
    })
}

fn validate_surface_shape_v1(
    manifest: &MtgoVisibleDuelViewModelCandidateSurfaceV1,
) -> Result<(), MtgoContractErrorV1> {
    if manifest.schema_version != MTGO_VISIBLE_DUEL_VIEWMODEL_SURFACE_SCHEMA_V1
        || manifest.surface_kind != SURFACE_KIND_V1
        || manifest.information_boundary != INFORMATION_BOUNDARY_V1
    {
        return Err(error_v1(
            "visible_duel_viewmodel_surface_header",
            "schema, kind, and information boundary must be exact",
        ));
    }
    if manifest.complete_duel_projection_demonstrated
        || manifest.live_producer_attested
        || manifest.safe_for_live_semantic_evidence
        || manifest.safe_for_model_scoring
        || manifest.safe_for_input
        || manifest.permits_event_entry
        || manifest.permits_spending
    {
        return Err(error_v1(
            "visible_duel_viewmodel_surface_authority",
            "a metadata candidate surface grants no live, model, input, event, or spending authority",
        ));
    }
    let candidate_keys = manifest
        .candidates
        .iter()
        .map(property_key)
        .collect::<HashSet<_>>();
    let forbidden_keys = manifest
        .forbidden_properties
        .iter()
        .map(forbidden_key)
        .collect::<HashSet<_>>();
    if candidate_keys.len() != manifest.candidates.len()
        || forbidden_keys.len() != manifest.forbidden_properties.len()
        || !candidate_keys.is_disjoint(&forbidden_keys)
        || manifest
            .candidates
            .iter()
            .any(|candidate| !candidate.exact_ui_corpus_qualification_required)
    {
        return Err(error_v1(
            "visible_duel_viewmodel_surface_property_set",
            "candidate and forbidden properties must be unique, disjoint, and require exact UI qualification",
        ));
    }
    Ok(())
}

fn candidate(
    assembly_file_name: &str,
    declaring_type: &str,
    property_name: &str,
    property_type: &str,
    context: MtgoVisibleViewModelPropertyContextV1,
    visible_ui_counterpart: &str,
    intended_projection_fields: &[&str],
) -> MtgoVisibleViewModelPropertyCandidateV1 {
    MtgoVisibleViewModelPropertyCandidateV1 {
        assembly_file_name: assembly_file_name.to_owned(),
        declaring_type: declaring_type.to_owned(),
        property_name: property_name.to_owned(),
        property_type: property_type.to_owned(),
        context,
        visible_ui_counterpart: visible_ui_counterpart.to_owned(),
        intended_projection_fields: intended_projection_fields
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        exact_ui_corpus_qualification_required: true,
    }
}

fn forbidden(
    assembly_file_name: &str,
    declaring_type: &str,
    property_name: &str,
    reason: &str,
) -> MtgoForbiddenViewModelPropertyV1 {
    MtgoForbiddenViewModelPropertyV1 {
        assembly_file_name: assembly_file_name.to_owned(),
        declaring_type: declaring_type.to_owned(),
        property_name: property_name.to_owned(),
        reason: reason.to_owned(),
    }
}

fn property_key(candidate: &MtgoVisibleViewModelPropertyCandidateV1) -> String {
    format!(
        "{}\0{}\0{}",
        candidate.assembly_file_name, candidate.declaring_type, candidate.property_name
    )
}

fn forbidden_key(forbidden: &MtgoForbiddenViewModelPropertyV1) -> String {
    format!(
        "{}\0{}\0{}",
        forbidden.assembly_file_name, forbidden.declaring_type, forbidden.property_name
    )
}

fn commitment_v1(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    format!("{:x}", hasher.finalize())
}

fn error_v1(code: &'static str, detail: impl Into<String>) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, detail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiled_candidate_surface_is_checked_but_non_authorizing() {
        let manifest = mtgo_visible_duel_viewmodel_candidate_surface_v1();
        let canonical = serde_json::to_vec(&manifest).expect("candidate surface JSON");
        let actual_commitment = commitment_v1(SURFACE_DOMAIN_V1, &[&canonical]);
        assert_eq!(
            actual_commitment,
            MTGO_VISIBLE_DUEL_VIEWMODEL_CANDIDATE_SURFACE_COMMITMENT_V1
        );
        let checked = check_untrusted_visible_duel_viewmodel_candidate_surface_v1(manifest)
            .expect("compiled metadata candidate surface");
        assert_eq!(checked.product_version_v1(), "3.4.158.4691");
        assert_eq!(checked.candidate_property_count_v1(), 74);
        assert_eq!(checked.forbidden_property_count_v1(), 28);
        assert_eq!(
            checked.commitment_sha256_v1(),
            MTGO_VISIBLE_DUEL_VIEWMODEL_CANDIDATE_SURFACE_COMMITMENT_V1
        );
        assert!(!checked.complete_duel_projection_demonstrated_v1());
        assert!(!checked.live_producer_attested_v1());
        assert!(!checked.safe_for_live_semantic_evidence_v1());
        assert!(!checked.safe_for_model_scoring_v1());
        assert!(!checked.safe_for_input_v1());
        assert!(!checked.permits_event_entry_v1());
        assert!(!checked.permits_spending_v1());
    }

    #[test]
    fn version_property_or_authority_drift_rejects() {
        let mut version = mtgo_visible_duel_viewmodel_candidate_surface_v1();
        version.product_version = "3.4.158.4692".to_owned();
        assert_eq!(
            check_untrusted_visible_duel_viewmodel_candidate_surface_v1(version)
                .err()
                .expect("version drift")
                .code(),
            "visible_duel_viewmodel_surface_mismatch"
        );

        let mut property = mtgo_visible_duel_viewmodel_candidate_surface_v1();
        property.candidates[0].property_name.push_str("Unexpected");
        assert_eq!(
            check_untrusted_visible_duel_viewmodel_candidate_surface_v1(property)
                .err()
                .expect("property drift")
                .code(),
            "visible_duel_viewmodel_surface_mismatch"
        );

        let mut authority = mtgo_visible_duel_viewmodel_candidate_surface_v1();
        authority.safe_for_model_scoring = true;
        assert_eq!(
            check_untrusted_visible_duel_viewmodel_candidate_surface_v1(authority)
                .err()
                .expect("authority broadening")
                .code(),
            "visible_duel_viewmodel_surface_authority"
        );
    }

    #[test]
    fn raw_backing_properties_never_enter_candidate_set() {
        let manifest = mtgo_visible_duel_viewmodel_candidate_surface_v1();
        let candidate_keys = manifest
            .candidates
            .iter()
            .map(property_key)
            .collect::<HashSet<_>>();
        for forbidden in &manifest.forbidden_properties {
            assert!(!candidate_keys.contains(&forbidden_key(forbidden)));
        }
        for forbidden_name in [
            "Game",
            "GameCard",
            "ModelZone",
            "CardDefinition",
            "PromptedPlayer",
            "CurrentTargetList",
            "Actions",
            "Action",
            "Targets",
        ] {
            assert!(!manifest
                .candidates
                .iter()
                .any(|candidate| candidate.property_name == forbidden_name));
        }
    }
}
