using System;
using System.Collections;
using System.Collections.Generic;
using System.IO.MemoryMappedFiles;
using System.Linq;
using System.Reflection;
using System.Text;
using System.Windows;
using System.Windows.Media;
using System.Windows.Threading;

namespace MtgKernel.Mtgo.VisibleDuelProducer.V1
{
    /// <summary>
    /// In-process root seam for the MTGO player-visible duel projection.
    /// V1.14 invokes only exact allowlisted getters for visible chrome, player
    /// panels, public zones, card presentation, and private action joins bound
    /// to player-visible sources. It emits either a fixed abstention or the
    /// bounded sanitized decision slice. It never exports client objects,
    /// client identifiers, or unvalidated raw values.
    /// </summary>
    public static partial class VisibleDuelProducerV1
    {
        private const string DuelRootType = "Shiny.Play.Duel.DuelScene";
        private const string DuelViewModelType = "Shiny.Play.Duel.ViewModel.DuelSceneViewModel";
        private const string ChannelPrefix = "Local\\mtgkernel_mtgo_visible_v1_";
        private const int MaximumVisualNodes = 200000;
        private const int MaximumVisualDepth = 256;
        private const int MaximumOutputBytes = 1048576;
        private const int OutputPayloadOffset = 8;
        private const int MaximumVisibleTextCharacters = 4096;
        private const int MaximumVisibleCollectionItems = 1024;

        private static readonly byte[] DuelSurfaceUnavailable = Encoding.UTF8.GetBytes(
            "{\"result_kind\":\"abstained\",\"reason\":\"duel_surface_unavailable\"}");
        private static readonly byte[] SurfaceShapeMismatch = Encoding.UTF8.GetBytes(
            "{\"result_kind\":\"abstained\",\"reason\":\"surface_shape_mismatch\"}");
        private static readonly byte[] ProjectionIncomplete = Encoding.UTF8.GetBytes(
            "{\"result_kind\":\"abstained\",\"reason\":\"projection_incomplete\"}");
        private static readonly byte[] OutputValidationFailed = Encoding.UTF8.GetBytes(
            "{\"result_kind\":\"abstained\",\"reason\":\"output_validation_failed\"}");

        // Assembly simple name, exact declaring type, exact public getter.
        // This is the complete candidate surface commitment represented as
        // executable source. No fallback property or recursive reflection is
        // allowed.
        private static readonly string[] AllowedGetters =
        {
            "Card|Shiny.Card.ViewModels.CardViewModel|CardFrameID",
            "Card|Shiny.Card.ViewModels.CardViewModel|CurrentDungeonRoom",
            "Card|Shiny.Card.ViewModels.CardViewModel|CurrentDamage",
            "Card|Shiny.Card.ViewModels.CardViewModel|IsAttacking",
            "Card|Shiny.Card.ViewModels.CardViewModel|IsBlocking",
            "Card|Shiny.Card.ViewModels.CardViewModel|IsFaceDown",
            "Card|Shiny.Card.ViewModels.CardViewModel|IsTapped",
            "Card|Shiny.Card.ViewModels.CardViewModel|Name",
            "Card|Shiny.Card.ViewModels.CardViewModel|Power",
            "Card|Shiny.Card.ViewModels.CardViewModel|Toughness",
            "DuelScene|Shiny.Play.Duel.GroupCardAction|GroupName",
            "DuelScene|Shiny.Play.Duel.GroupCardAction|ModeOptions",
            "DuelScene|Shiny.Play.Duel.GroupCardAction|Name",
            "DuelScene|Shiny.Play.Duel.ViewModel.CardSelectionViewModel|Visible",
            "DuelScene|Shiny.Play.Duel.ViewModel.CardSelectorDialogViewModel|Visible",
            "DuelScene|Shiny.Play.Duel.ViewModel.CardSelectorManager|Collection",
            "DuelScene|Shiny.Play.Duel.ViewModel.CardCounterViewModel|Quantity",
            "DuelScene|Shiny.Play.Duel.ViewModel.CardCounterViewModel|Type",
            "DuelScene|Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel|CardAttachedTo",
            "DuelScene|Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel|IsSpeedEmblem",
            "DuelScene|Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel|IsToken",
            "DuelScene|Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel|RingTemptationCounter",
            "DuelScene|Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel|SpeedCounter",
            "DuelScene|Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel|VisibleCounters",
            "DuelScene|Shiny.Play.Duel.ViewModel.DuelSceneViewModel|CardSelection",
            "DuelScene|Shiny.Play.Duel.ViewModel.DuelSceneViewModel|CardSelectorDialog",
            "DuelScene|Shiny.Play.Duel.ViewModel.DuelSceneViewModel|CardSelectors",
            "DuelScene|Shiny.Play.Duel.ViewModel.DuelSceneViewModel|CurrentPhase",
            "DuelScene|Shiny.Play.Duel.ViewModel.DuelSceneViewModel|GameTurnText",
            "DuelScene|Shiny.Play.Duel.ViewModel.DuelSceneViewModel|IsCommander",
            "DuelScene|Shiny.Play.Duel.ViewModel.DuelSceneViewModel|IsPileZoneActive",
            "DuelScene|Shiny.Play.Duel.ViewModel.DuelSceneViewModel|IsPlanechase",
            "DuelScene|Shiny.Play.Duel.ViewModel.DuelSceneViewModel|IsWishingFromSideboard",
            "DuelScene|Shiny.Play.Duel.ViewModel.DuelSceneViewModel|LocalTriggersPanelEnabled",
            "DuelScene|Shiny.Play.Duel.ViewModel.DuelSceneViewModel|OpponentTriggersPanelEnabled",
            "DuelScene|Shiny.Play.Duel.ViewModel.DuelSceneViewModel|Players",
            "DuelScene|Shiny.Play.Duel.ViewModel.DuelSceneViewModel|PromptBox",
            "DuelScene|Shiny.Play.Duel.ViewModel.DuelSceneViewModel|StackZone",
            "DuelScene|Shiny.Play.Duel.ViewModel.DuelSceneViewModel|StormCounterVisible",
            "DuelScene|Shiny.Play.Duel.ViewModel.DuelSceneViewModel|TemporaryZones",
            "DuelScene|Shiny.Play.Duel.ViewModel.DuelSceneViewModel|ThreePilePanelEnabled",
            "DuelScene|Shiny.Play.Duel.ViewModel.DuelSceneViewModel|TwoPilePanelEnabled",
            "DuelScene|Shiny.Play.Duel.ViewModel.ManaPoolItemViewModel|ColorString",
            "DuelScene|Shiny.Play.Duel.ViewModel.ManaPoolItemViewModel|Count",
            "DuelScene|Shiny.Play.Duel.ViewModel.OptionButton|Enabled",
            "DuelScene|Shiny.Play.Duel.ViewModel.OptionButton|Name",
            "DuelScene|Shiny.Play.Duel.ViewModel.OptionButton|Visible",
            "DuelScene|Shiny.Play.Duel.ViewModel.PlayerViewModel|Active",
            "DuelScene|Shiny.Play.Duel.ViewModel.PlayerViewModel|BattlefieldCards",
            "DuelScene|Shiny.Play.Duel.ViewModel.PlayerViewModel|CompanionZone",
            "DuelScene|Shiny.Play.Duel.ViewModel.PlayerViewModel|DeckTotal",
            "DuelScene|Shiny.Play.Duel.ViewModel.PlayerViewModel|ExileZone",
            "DuelScene|Shiny.Play.Duel.ViewModel.PlayerViewModel|GraveyardZone",
            "DuelScene|Shiny.Play.Duel.ViewModel.PlayerViewModel|HasEnergyCounters",
            "DuelScene|Shiny.Play.Duel.ViewModel.PlayerViewModel|HasExperienceCounters",
            "DuelScene|Shiny.Play.Duel.ViewModel.PlayerViewModel|HasPoisonCounters",
            "DuelScene|Shiny.Play.Duel.ViewModel.PlayerViewModel|HasRadCounters",
            "DuelScene|Shiny.Play.Duel.ViewModel.PlayerViewModel|HandTotal",
            "DuelScene|Shiny.Play.Duel.ViewModel.PlayerViewModel|HandZone",
            "DuelScene|Shiny.Play.Duel.ViewModel.PlayerViewModel|Health",
            "DuelScene|Shiny.Play.Duel.ViewModel.PlayerViewModel|LibraryZone",
            "DuelScene|Shiny.Play.Duel.ViewModel.PlayerViewModel|LocalPlayer",
            "DuelScene|Shiny.Play.Duel.ViewModel.PlayerViewModel|ManaPoolItems",
            "DuelScene|Shiny.Play.Duel.ViewModel.PlayerViewModel|MatActive",
            "DuelScene|Shiny.Play.Duel.ViewModel.PlayerViewModel|Name",
            "DuelScene|Shiny.Play.Duel.ViewModel.PlayerViewModel|RevealedZone",
            "DuelScene|Shiny.Play.Duel.ViewModel.PlayerViewModel|ShieldsZone",
            "DuelScene|Shiny.Play.Duel.ViewModel.PromptBoxViewModel|IsPromptBoxActive",
            "DuelScene|Shiny.Play.Duel.ViewModel.PromptBoxViewModel|StandardButtons",
            "DuelScene|Shiny.Play.Duel.ViewModel.PromptBoxViewModel|Text",
            "DuelScene|Shiny.Play.Duel.ViewModel.TemporaryZoneViewModel|IsVisible",
            "DuelScene|Shiny.Play.Duel.ViewModel.ZoneViewModel|Cards",
            "DuelScene|Shiny.Play.Duel.ViewModel.ZoneViewModel|Count",
            "DuelScene|Shiny.Play.Duel.ViewModel.ZoneViewModel|IsVisible"
        };

        // Private join-only getters. These backing objects may be inspected
        // transiently only after their source visible control or seated-player
        // card has been established. No value or client identity is exported.
        private static readonly string[] PrivateVisibleActionJoinGetters =
        {
            "DuelScene|Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel|Actions",
            "DuelScene|Shiny.Play.Duel.ViewModel.DuelSceneViewModel|Game",
            "DuelScene|Shiny.Play.Duel.ViewModel.OptionButton|Action",
            "DuelScene|Shiny.Play.Duel.ViewModel.ManaPoolItemViewModel|Color",
            "DuelScene|Shiny.Play.Duel.ViewModel.PromptBoxViewModel|DoneButton",
            "DuelScene|Shiny.Play.Duel.ViewModel.PromptBoxViewModel|OkPromptButton",
            "WotC.MtGO.Client.Model.Reference|WotC.MtGO.Client.Model.Play.IGameAction|ActionType",
            "WotC.MtGO.Client.Model.Reference|WotC.MtGO.Client.Model.Play.IGameAction|IsDefault",
            "WotC.MtGO.Client.Model.Reference|WotC.MtGO.Client.Model.Play.IGameAction|Name",
            "WotC.MtGO.Client.Model.Reference|WotC.MtGO.Client.Model.Play.ICardAction|ActionChoices",
            "WotC.MtGO.Client.Model.Reference|WotC.MtGO.Client.Model.Play.ICardAction|AltMenuAction",
            "WotC.MtGO.Client.Model.Reference|WotC.MtGO.Client.Model.Play.ICardAction|AttackVictimId",
            "WotC.MtGO.Client.Model.Reference|WotC.MtGO.Client.Model.Play.ICardAction|CanBePerformedLocally",
            "WotC.MtGO.Client.Model.Reference|WotC.MtGO.Client.Model.Play.ICardAction|ConfirmBeforeTargetingOwnCard",
            "WotC.MtGO.Client.Model.Reference|WotC.MtGO.Client.Model.Play.ICardAction|ConfirmModeString",
            "WotC.MtGO.Client.Model.Reference|WotC.MtGO.Client.Model.Play.ICardAction|GroupName",
            "WotC.MtGO.Client.Model.Reference|WotC.MtGO.Client.Model.Play.ICardAction|HasXTarget",
            "WotC.MtGO.Client.Model.Reference|WotC.MtGO.Client.Model.Play.ICardAction|InSideboard",
            "WotC.MtGO.Client.Model.Reference|WotC.MtGO.Client.Model.Play.ICardAction|IsActivatedAbility",
            "WotC.MtGO.Client.Model.Reference|WotC.MtGO.Client.Model.Play.ICardAction|IsCastAction",
            "WotC.MtGO.Client.Model.Reference|WotC.MtGO.Client.Model.Play.ICardAction|IsFakeAction",
            "WotC.MtGO.Client.Model.Reference|WotC.MtGO.Client.Model.Play.ICardAction|IsManaAbility",
            "WotC.MtGO.Client.Model.Reference|WotC.MtGO.Client.Model.Play.ICardAction|IsSubmenuItem",
            "WotC.MtGO.Client.Model.Reference|WotC.MtGO.Client.Model.Play.ICardAction|ModeMaxChoices",
            "WotC.MtGO.Client.Model.Reference|WotC.MtGO.Client.Model.Play.ICardAction|ModeMinChoices",
            "WotC.MtGO.Client.Model.Reference|WotC.MtGO.Client.Model.Play.ICardAction|ModeChoiceMapping",
            "WotC.MtGO.Client.Model.Reference|WotC.MtGO.Client.Model.Play.ICardAction|ModeOptions",
            "WotC.MtGO.Client.Model.Reference|WotC.MtGO.Client.Model.Play.ICardAction|Targets",
            "WotC.MtGO.Client.Model.Reference|WotC.MtGO.Client.Model.Play.ICardAction|XDeterminedByTargetWithGreatestCMC",
            "WotC.MtGO.Client.Model.Reference|WotC.MtGO.Client.Model.Play.ICardAction|XIsAMinimum",
            "WotC.MtGO.Client.Model.Reference|WotC.MtGO.Client.Model.Play.ICardAction|XTargetDivisor"
        };

        /// <summary>
        /// Writes one bounded broker-result JSON value to the broker-created
        /// local memory channel. The integer result is a fixed transport code
        /// and carries no client or game information.
        /// </summary>
        public static int ExportVisibleDecisionOrAbstainV1(string channelName)
        {
            return RunVisibleDecisionTransactionV1(channelName, false);
        }

        /// <summary>
        /// Executes one selected player-visible action only after rebuilding
        /// the exact sanitized decision and matching its SHA-256 and index.
        /// The channel receives only a fixed submitted or rejected receipt.
        /// </summary>
        public static int DispatchSelectedVisibleActionV1(string channelName)
        {
            return RunVisibleDecisionTransactionV1(channelName, true);
        }

        private static int RunVisibleDecisionTransactionV1(
            string channelName,
            bool dispatchRequested)
        {
            if (!IsExactChannelName(channelName))
            {
                return 2;
            }

            SealedVisibleActionDispatchRequestV1? dispatchRequest = null;
            if (dispatchRequested &&
                !TryReadSealedVisibleActionDispatchRequestV1(
                    channelName,
                    out dispatchRequest))
            {
                return WriteBrokerResult(channelName, VisibleActionRejected) ? 0 : 4;
            }
            if (dispatchRequested != (dispatchRequest != null))
            {
                return WriteBrokerResult(channelName, OutputValidationFailed) ? 0 : 4;
            }

            byte[] result;
            try
            {
                Application application = Application.Current;
                if (application == null || application.Dispatcher == null)
                {
                    result = DuelSurfaceUnavailable;
                    return WriteBrokerResult(channelName, result) ? 0 : 4;
                }

                Dispatcher dispatcher = application.Dispatcher;
                if (dispatcher.HasShutdownStarted || dispatcher.HasShutdownFinished)
                {
                    result = DuelSurfaceUnavailable;
                }
                else if (dispatcher.CheckAccess())
                {
                    result = ExportOnUiThread(application, dispatchRequest);
                }
                else
                {
                    result = (byte[])dispatcher.Invoke(
                        DispatcherPriority.Send,
                        new Func<byte[]>(() => ExportOnUiThread(application, dispatchRequest)));
                }
            }
            catch
            {
                result = dispatchRequested ? VisibleActionRejected : OutputValidationFailed;
            }

            return WriteBrokerResult(channelName, result) ? 0 : 4;
        }

        private static byte[] ExportOnUiThread(
            Application application,
            SealedVisibleActionDispatchRequestV1? dispatchRequest)
        {
            var roots = new List<FrameworkElement>(2);
            int visited = 0;
            foreach (Window window in application.Windows)
            {
                if (window == null || !window.IsVisible)
                {
                    continue;
                }

                if (!CollectExactDuelRoots(window, 0, ref visited, roots))
                {
                    return SurfaceShapeMismatch;
                }
                if (roots.Count > 1)
                {
                    return SurfaceShapeMismatch;
                }
            }

            if (roots.Count != 1)
            {
                return DuelSurfaceUnavailable;
            }

            FrameworkElement root = roots[0];
            if (!root.IsLoaded || !root.IsVisible || root.ActualWidth <= 0 || root.ActualHeight <= 0)
            {
                return DuelSurfaceUnavailable;
            }

            object viewModel = root.DataContext;
            if (viewModel == null || viewModel.GetType().FullName != DuelViewModelType)
            {
                return SurfaceShapeMismatch;
            }

            if (!ValidateExactGetterSurface())
            {
                return SurfaceShapeMismatch;
            }

            // V1.14 qualifies exact visible chrome, player-panel, public-zone,
            // card-presentation, and visible-source-bound private action-join
            // routes. Temporary objects and values never leave this call.
            if (!TryValidateVisibleChromeProjectionV1(viewModel))
            {
                return ProjectionIncomplete;
            }
            bool built = TryBuildSanitizedVisibleDecisionAndActionBindingsV1(
                viewModel,
                out byte[] visibleDecision,
                out List<object> boundClientActions);
            if (!built)
            {
                return ProjectionIncomplete;
            }
            if (dispatchRequest == null)
            {
                return visibleDecision;
            }
            return TryExecuteSealedVisibleActionV1(
                viewModel,
                visibleDecision,
                boundClientActions,
                dispatchRequest)
                ? VisibleActionSubmitted
                : VisibleActionRejected;
        }

        private static bool CollectExactDuelRoots(
            DependencyObject node,
            int depth,
            ref int visited,
            List<FrameworkElement> roots)
        {
            if (depth > MaximumVisualDepth || visited >= MaximumVisualNodes)
            {
                return false;
            }
            visited++;

            if (node is FrameworkElement element && element.GetType().FullName == DuelRootType)
            {
                roots.Add(element);
                return roots.Count <= 1;
            }

            int childCount = VisualTreeHelper.GetChildrenCount(node);
            for (int index = 0; index < childCount; index++)
            {
                DependencyObject child = VisualTreeHelper.GetChild(node, index);
                if (child == null || !CollectExactDuelRoots(child, depth + 1, ref visited, roots))
                {
                    return false;
                }
            }
            return true;
        }

        private static bool ValidateExactGetterSurface()
        {
            Assembly[] loadedAssemblies = AppDomain.CurrentDomain.GetAssemblies();
            if (AllowedGetters.Length != 74 ||
                AllowedGetters.Distinct(StringComparer.Ordinal).Count() != 74 ||
                PrivateVisibleActionJoinGetters.Length != 31 ||
                PrivateVisibleActionJoinGetters.Distinct(StringComparer.Ordinal).Count() != 31)
            {
                return false;
            }

            foreach (string allowed in AllowedGetters)
            {
                string[] parts = allowed.Split('|');
                if (parts.Length != 3)
                {
                    return false;
                }

                Assembly[] assemblyMatches = loadedAssemblies
                    .Where(assembly => string.Equals(
                        assembly.GetName().Name,
                        parts[0],
                        StringComparison.Ordinal))
                    .ToArray();
                if (assemblyMatches.Length != 1)
                {
                    return false;
                }

                Type declaringType = assemblyMatches[0].GetType(parts[1], false, false);
                if (declaringType == null)
                {
                    return false;
                }

                PropertyInfo? property = declaringType.GetProperty(
                    parts[2],
                    BindingFlags.Instance | BindingFlags.Public | BindingFlags.FlattenHierarchy);
                MethodInfo? getter = property?.GetGetMethod(false);
                if (property == null || property.GetIndexParameters().Length != 0 || getter == null || !getter.IsPublic)
                {
                    return false;
                }
            }

            foreach (string allowed in PrivateVisibleActionJoinGetters)
            {
                string[] parts = allowed.Split('|');
                if (parts.Length != 3)
                {
                    return false;
                }

                Assembly[] assemblyMatches = loadedAssemblies
                    .Where(assembly => string.Equals(
                        assembly.GetName().Name,
                        parts[0],
                        StringComparison.Ordinal))
                    .ToArray();
                if (assemblyMatches.Length != 1)
                {
                    return false;
                }

                Type declaringType = assemblyMatches[0].GetType(parts[1], false, false);
                PropertyInfo? property = declaringType?.GetProperty(
                    parts[2],
                    BindingFlags.Instance | BindingFlags.Public | BindingFlags.FlattenHierarchy);
                MethodInfo? getter = property?.GetGetMethod(false);
                if (declaringType == null || property == null ||
                    property.GetIndexParameters().Length != 0 || getter == null ||
                    !getter.IsPublic || getter.GetParameters().Length != 0)
                {
                    return false;
                }
            }
            return true;
        }

        private static bool TryValidateVisibleChromeProjectionV1(object viewModel)
        {
            if (!TryRequireSupportedDuelVariantV1(viewModel) ||
                !TryReadExactPropertyV1(
                    viewModel,
                    "DuelScene",
                    DuelViewModelType,
                    "CurrentPhase",
                    out object? currentPhase) ||
                currentPhase == null ||
                currentPhase.GetType().FullName != "WotC.MtGO.Client.Model.Play.GamePhase" ||
                !TryReadExactPropertyV1(
                    viewModel,
                    "DuelScene",
                    DuelViewModelType,
                    "GameTurnText",
                    out object? turnTextValue) ||
                !(turnTextValue is string turnText) ||
                !IsBoundedVisibleStringV1(turnText, 128, false) ||
                !TryReadExactPropertyV1(
                    viewModel,
                    "DuelScene",
                    DuelViewModelType,
                    "Players",
                    out object? playersValue) ||
                !TryBoundedCollectionV1(playersValue, 2, out List<object> players) ||
                players.Count != 2)
            {
                return false;
            }

            int localPlayers = 0;
            int activePlayers = 0;
            int priorityPlayers = 0;
            foreach (object player in players)
            {
                if (!TryValidateVisiblePlayerPanelV1(
                        player,
                        out bool localPlayer,
                        out bool activePlayer,
                        out bool priorityPlayer))
                {
                    return false;
                }
                if (localPlayer)
                {
                    localPlayers++;
                }
                if (activePlayer)
                {
                    activePlayers++;
                }
                if (priorityPlayer)
                {
                    priorityPlayers++;
                }
            }
            if (localPlayers != 1 || activePlayers > 1 || priorityPlayers > 1)
            {
                return false;
            }

            if (!TryReadExactPropertyV1(
                    viewModel,
                    "DuelScene",
                    DuelViewModelType,
                    "PromptBox",
                    out object? promptBox) ||
                promptBox == null ||
                !TryValidateVisiblePromptV1(promptBox))
            {
                return false;
            }
            return TryValidateVisibleZonesAndCardsV1(viewModel, players);
        }

        private static bool TryValidateVisibleZonesAndCardsV1(
            object viewModel,
            List<object> players)
        {
            var battlefieldCards = new List<object>();
            var attachmentTargets = new List<object>();
            var seatedPlayerVisibleActionCards = new List<object>();
            foreach (object player in players)
            {
                const string playerType = "Shiny.Play.Duel.ViewModel.PlayerViewModel";
                if (!TryReadExactPropertyV1(
                        player,
                        "DuelScene",
                        playerType,
                        "LocalPlayer",
                        out object? localValue) ||
                    !(localValue is bool localPlayer) ||
                    !TryReadExactPropertyV1(
                        player,
                        "DuelScene",
                        playerType,
                        "BattlefieldCards",
                        out object? battlefieldValue) ||
                    !TryBoundedCollectionV1(
                        battlefieldValue,
                        512,
                        out List<object> playerBattlefield))
                {
                    return false;
                }
                foreach (object card in playerBattlefield)
                {
                    if (!TryValidateVisibleBattlefieldCardV1(card, out object? attachedTo))
                    {
                        return false;
                    }
                    battlefieldCards.Add(card);
                    if (localPlayer)
                    {
                        seatedPlayerVisibleActionCards.Add(card);
                    }
                    if (attachedTo != null)
                    {
                        attachmentTargets.Add(attachedTo);
                    }
                }

                if (!TryReadExactPropertyV1(
                        player, "DuelScene", playerType, "HandZone", out object? handZone) ||
                    handZone == null ||
                    !TryValidateConditionalVisibleZoneV1(
                        handZone,
                        localPlayer,
                        localPlayer) ||
                    !TryReadExactPropertyV1(
                        player,
                        "DuelScene",
                        playerType,
                        "LibraryZone",
                        out object? libraryZone) ||
                    libraryZone == null ||
                    !TryValidateNeverEnumeratedZoneRootV1(libraryZone) ||
                    !TryReadExactPropertyV1(
                        player,
                        "DuelScene",
                        playerType,
                        "GraveyardZone",
                        out object? graveyardZone) ||
                    graveyardZone == null ||
                    !TryValidateEnumeratedVisibleZoneV1(graveyardZone, false) ||
                    !TryReadExactPropertyV1(
                        player, "DuelScene", playerType, "ExileZone", out object? exileZone) ||
                    exileZone == null ||
                    !TryValidateEnumeratedVisibleZoneV1(exileZone, false) ||
                    !TryReadExactPropertyV1(
                        player,
                        "DuelScene",
                        playerType,
                        "RevealedZone",
                        out object? revealedZone) ||
                    revealedZone == null ||
                    !TryValidateConditionalVisibleZoneV1(revealedZone, false, true) ||
                    !TryReadExactPropertyV1(
                        player,
                        "DuelScene",
                        playerType,
                        "ShieldsZone",
                        out object? shieldsZone) ||
                    shieldsZone == null ||
                    !TryValidateVisibleInitiativeShieldZoneV1(shieldsZone))
                {
                    return false;
                }
                if (localPlayer &&
                    (!TryAppendVisibleZoneCardsForPrivateActionJoinV1(
                        handZone,
                        true,
                        seatedPlayerVisibleActionCards) ||
                    !TryAppendVisibleZoneCardsForPrivateActionJoinV1(
                        graveyardZone,
                        true,
                        seatedPlayerVisibleActionCards) ||
                    !TryAppendVisibleZoneCardsForPrivateActionJoinV1(
                        exileZone,
                        true,
                        seatedPlayerVisibleActionCards) ||
                    !TryAppendVisibleZoneCardsForPrivateActionJoinV1(
                        revealedZone,
                        false,
                        seatedPlayerVisibleActionCards)))
                {
                    return false;
                }
            }

            foreach (object attachedTo in attachmentTargets)
            {
                if (!battlefieldCards.Any(card => ReferenceEquals(card, attachedTo)))
                {
                    return false;
                }
            }

            if (!TryReadExactPropertyV1(
                    viewModel,
                    "DuelScene",
                    DuelViewModelType,
                    "StackZone",
                    out object? stackZone) ||
                stackZone == null ||
                !TryValidateEnumeratedVisibleZoneV1(stackZone, false))
            {
                return false;
            }
            return TryValidatePrivateVisibleCardActionJoinsV1(
                seatedPlayerVisibleActionCards);
        }

        private static bool TryValidateVisibleInitiativeShieldZoneV1(object zone)
        {
            const string zoneType = "Shiny.Play.Duel.ViewModel.ZoneViewModel";
            const string cardType = "Shiny.Card.ViewModels.CardViewModel";
            if (!TryReadExactPropertyV1(
                    zone, "DuelScene", zoneType, "IsVisible", out object? visibleValue) ||
                !(visibleValue is bool visible))
            {
                return false;
            }
            if (!visible)
            {
                return true;
            }
            if (!TryReadExactPropertyV1(
                    zone, "DuelScene", zoneType, "Count", out object? countValue) ||
                !(countValue is int count) || count < 0 ||
                count > MaximumVisibleCollectionItems ||
                !TryReadExactPropertyV1(
                    zone, "DuelScene", zoneType, "Cards", out object? cardsValue) ||
                !TryBoundedCollectionV1(
                    cardsValue,
                    MaximumVisibleCollectionItems,
                    out List<object> cards) ||
                cards.Count != count)
            {
                return false;
            }
            foreach (object card in cards)
            {
                if (!TryRequireNoUnrepresentedVisibleCardStateV1(card) ||
                    !TryReadExactPropertyV1(
                        card, "Card", cardType, "CardFrameID", out object? frameValue) ||
                    frameValue == null ||
                    frameValue.GetType().FullName != "Shiny.Card.Enums.FrameStyle" ||
                    Enum.GetName(frameValue.GetType(), frameValue) == null)
                {
                    return false;
                }
            }
            return true;
        }

        private static bool TryAppendVisibleZoneCardsForPrivateActionJoinV1(
            object zone,
            bool requireEnumeration,
            List<object> destination)
        {
            const string zoneType = "Shiny.Play.Duel.ViewModel.ZoneViewModel";
            if (!requireEnumeration &&
                (!TryReadExactPropertyV1(
                    zone,
                    "DuelScene",
                    zoneType,
                    "IsVisible",
                    out object? visibleValue) ||
                !(visibleValue is bool visible) ||
                !visible))
            {
                return visibleValue is bool;
            }
            if (!TryReadExactPropertyV1(
                    zone,
                    "DuelScene",
                    zoneType,
                    "Cards",
                    out object? cardsValue) ||
                !TryBoundedCollectionV1(
                    cardsValue,
                    MaximumVisibleCollectionItems,
                    out List<object> cards))
            {
                return false;
            }
            destination.AddRange(cards);
            return destination.Count <= MaximumVisibleCollectionItems;
        }

        private static bool TryValidatePrivateVisibleCardActionJoinsV1(
            List<object> seatedPlayerVisibleCards)
        {
            int actionCount = 0;
            foreach (object card in seatedPlayerVisibleCards)
            {
                if (!TryReadExactPrivateVisibleActionPropertyV1(
                        card,
                        "DuelScene",
                        "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel",
                        "Actions",
                        out object? actionsValue) ||
                    !TryBoundedCollectionV1(actionsValue, 256, out List<object> actions))
                {
                    return false;
                }
                foreach (object action in actions)
                {
                    actionCount++;
                    if (actionCount > 256 ||
                        !TryValidatePrivateVisibleActionV1(action, true))
                    {
                        return false;
                    }
                }
            }
            return true;
        }

        private static bool TryValidatePrivateVisibleActionV1(
            object action,
            bool requireCardAction)
        {
            const string referenceAssembly = "WotC.MtGO.Client.Model.Reference";
            const string gameActionType = "WotC.MtGO.Client.Model.Play.IGameAction";
            const string cardActionType = "WotC.MtGO.Client.Model.Play.ICardAction";
            if (!TryReadExactPrivateVisibleActionPropertyV1(
                    action,
                    referenceAssembly,
                    gameActionType,
                    "Name",
                    out object? nameValue) ||
                !(nameValue is string name) ||
                !IsBoundedVisibleStringV1(name, 512, true) ||
                !TryReadExactPrivateVisibleActionPropertyV1(
                    action,
                    referenceAssembly,
                    gameActionType,
                    "ActionType",
                    out object? actionTypeValue) ||
                actionTypeValue == null ||
                actionTypeValue.GetType().FullName !=
                    "WotC.MtGO.Client.Model.Play.ActionType")
            {
                return false;
            }
            if (!requireCardAction)
            {
                return true;
            }

            if (!TryReadExactPrivateVisibleActionPropertyV1(
                    action,
                    referenceAssembly,
                    cardActionType,
                    "CanBePerformedLocally",
                    out object? localValue) ||
                !(localValue is bool local) || !local ||
                !TryReadExactPrivateVisibleActionPropertyV1(
                    action,
                    referenceAssembly,
                    cardActionType,
                    "IsManaAbility",
                    out object? manaValue) ||
                !(manaValue is bool) ||
                !TryReadExactPrivateVisibleActionPropertyV1(
                    action,
                    referenceAssembly,
                    cardActionType,
                    "IsActivatedAbility",
                    out object? activatedValue) ||
                !(activatedValue is bool) ||
                !TryReadExactPrivateVisibleActionPropertyV1(
                    action,
                    referenceAssembly,
                    cardActionType,
                    "IsCastAction",
                    out object? castValue) ||
                !(castValue is bool) ||
                !TryReadExactPrivateVisibleActionPropertyV1(
                    action,
                    referenceAssembly,
                    cardActionType,
                    "ModeOptions",
                    out object? modesValue) ||
                !TryBoundedVisibleStringCollectionV1(modesValue, 64) ||
                !TryRequireBasicVisibleCardActionMenuShapeV1(action) ||
                !TryRequireNoUnrepresentedVisibleCardActionModalV1(action))
            {
                return false;
            }
            return action.GetType().FullName != "Shiny.Play.Duel.GroupCardAction" ||
                TryValidateVisibleGroupCardActionLabelsV1(action);
        }

        // MTGO's Card_View.ProcessMouseUp does not render the raw Actions
        // collection one-for-one. It transforms alternate-menu, grouped,
        // mode-choice, submenu, action-choice, and attack-hover entries. The
        // current semantic slice models none of those transformations, so the
        // corresponding private values are used only as one-way guards and
        // are never serialized. Defaults describe the simple one-action-per-
        // visible-label shape supported by V1.14.
        private static bool TryRequireBasicVisibleCardActionMenuShapeV1(object action)
        {
            const string assembly = "WotC.MtGO.Client.Model.Reference";
            const string cardAction = "WotC.MtGO.Client.Model.Play.ICardAction";
            return TryReadExactPrivateVisibleActionPropertyV1(
                    action, assembly, cardAction, "AltMenuAction", out object? altValue) &&
                altValue is bool alt && !alt &&
                TryReadExactPrivateVisibleActionPropertyV1(
                    action, assembly, cardAction, "GroupName", out object? groupValue) &&
                (groupValue == null || groupValue is string group && group.Length == 0) &&
                TryReadExactPrivateVisibleActionPropertyV1(
                    action, assembly, cardAction, "ActionChoices", out object? choicesValue) &&
                (choicesValue == null || choicesValue is string choices && choices.Length == 0) &&
                TryReadExactPrivateVisibleActionPropertyV1(
                    action, assembly, cardAction, "ModeChoiceMapping", out object? mappingValue) &&
                (mappingValue == null || mappingValue is string mapping && mapping.Length == 0) &&
                TryReadExactPrivateVisibleActionPropertyV1(
                    action, assembly, cardAction, "IsSubmenuItem", out object? submenuValue) &&
                submenuValue is bool submenu && !submenu &&
                TryReadExactPrivateVisibleActionPropertyV1(
                    action, assembly, cardAction, "AttackVictimId", out object? victimValue) &&
                victimValue is int victim && victim == -1;
        }

        // PromptBoxViewModel.Execute turns these private action properties into
        // later player-visible target, X-value, or confirmation UI. The current
        // semantic slice cannot continue those modals, so only their inert
        // defaults are accepted. Values are inspected transiently as one-way
        // guards and never serialized.
        private static bool TryRequireNoUnrepresentedVisibleCardActionModalV1(
            object action)
        {
            const string assembly = "WotC.MtGO.Client.Model.Reference";
            const string cardAction = "WotC.MtGO.Client.Model.Play.ICardAction";
            return TryReadExactPrivateVisibleActionPropertyV1(
                    action, assembly, cardAction, "Targets", out object? targetsValue) &&
                targetsValue is ICollection targets && targets.Count == 0 &&
                TryReadExactPrivateVisibleActionPropertyV1(
                    action, assembly, cardAction, "HasXTarget", out object? hasXValue) &&
                hasXValue is bool hasX && !hasX &&
                TryReadExactPrivateVisibleActionPropertyV1(
                    action, assembly, cardAction, "InSideboard", out object? sideboardValue) &&
                sideboardValue is bool sideboard && !sideboard &&
                TryReadExactPrivateVisibleActionPropertyV1(
                    action, assembly, cardAction, "ConfirmModeString", out object? confirmValue) &&
                confirmValue == null &&
                TryReadExactPrivateVisibleActionPropertyV1(
                    action, assembly, cardAction, "ConfirmBeforeTargetingOwnCard", out object? ownConfirmValue) &&
                ownConfirmValue is bool ownConfirm && !ownConfirm &&
                TryReadExactPrivateVisibleActionPropertyV1(
                    action, assembly, cardAction, "IsFakeAction", out object? fakeValue) &&
                fakeValue is bool fake && !fake &&
                TryReadExactPrivateVisibleActionPropertyV1(
                    action, assembly, cardAction, "ModeMinChoices", out object? modeMinValue) &&
                modeMinValue is uint modeMin && modeMin == 0u &&
                TryReadExactPrivateVisibleActionPropertyV1(
                    action, assembly, cardAction, "ModeMaxChoices", out object? modeMaxValue) &&
                modeMaxValue is uint modeMax && modeMax == 0u &&
                TryReadExactPrivateVisibleActionPropertyV1(
                    action, assembly, cardAction, "XIsAMinimum", out object? xMinimumValue) &&
                xMinimumValue is bool xMinimum && !xMinimum &&
                TryReadExactPrivateVisibleActionPropertyV1(
                    action, assembly, cardAction, "XDeterminedByTargetWithGreatestCMC", out object? xTargetValue) &&
                xTargetValue is bool xTarget && !xTarget &&
                TryReadExactPrivateVisibleActionPropertyV1(
                    action, assembly, cardAction, "XTargetDivisor", out object? divisorValue) &&
                divisorValue is int divisor && divisor == 1;
        }

        private static bool TryValidateVisibleGroupCardActionLabelsV1(object action)
        {
            const string groupType = "Shiny.Play.Duel.GroupCardAction";
            if (!TryReadExactPropertyV1(
                    action, "DuelScene", groupType, "Name", out object? nameValue) ||
                !(nameValue is string name) ||
                !IsBoundedVisibleStringV1(name, 512, true) ||
                !TryReadExactPropertyV1(
                    action,
                    "DuelScene",
                    groupType,
                    "GroupName",
                    out object? groupValue) ||
                (groupValue != null &&
                    (!(groupValue is string groupName) ||
                    !IsBoundedVisibleStringV1(groupName, 512, true))) ||
                !TryReadExactPropertyV1(
                    action,
                    "DuelScene",
                    groupType,
                    "ModeOptions",
                    out object? modesValue) ||
                !TryBoundedVisibleStringCollectionV1(modesValue, 64))
            {
                return false;
            }
            return true;
        }

        private static bool TryValidateNeverEnumeratedZoneRootV1(object zone)
        {
            return zone.GetType().FullName == "Shiny.Play.Duel.ViewModel.ZoneViewModel";
        }

        private static bool TryValidateVisibleBattlefieldCardV1(
            object card,
            out object? attachedTo)
        {
            attachedTo = null;
            const string cardAssembly = "Card";
            const string cardType = "Shiny.Card.ViewModels.CardViewModel";
            const string duelCardType =
                "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel";
            if (!TryReadExactPropertyV1(
                    card, cardAssembly, cardType, "IsFaceDown", out object? faceDownValue) ||
                !(faceDownValue is bool faceDown) ||
                (!faceDown && !TryRequireNoUnrepresentedVisibleCardStateV1(card)) ||
                (!faceDown &&
                    (!TryReadExactPropertyV1(
                        card, cardAssembly, cardType, "Name", out object? nameValue) ||
                    !(nameValue is string name) ||
                    !IsBoundedVisibleStringV1(name, 512, true))) ||
                !TryReadExactPropertyV1(
                    card, cardAssembly, cardType, "IsTapped", out object? tappedValue) ||
                !(tappedValue is bool) ||
                !TryReadExactPropertyV1(
                    card, cardAssembly, cardType, "IsAttacking", out object? attackingValue) ||
                !(attackingValue is bool) ||
                !TryReadExactPropertyV1(
                    card, cardAssembly, cardType, "IsBlocking", out object? blockingValue) ||
                !(blockingValue is bool) ||
                !TryReadExactPropertyV1(
                    card,
                    cardAssembly,
                    cardType,
                    "CurrentDamage",
                    out object? damageValue) ||
                !(damageValue is int damage) || damage < 0 || damage > ushort.MaxValue ||
                !TryReadExactPropertyV1(
                    card, cardAssembly, cardType, "Power", out object? powerValue) ||
                (powerValue != null && !(powerValue is int)) ||
                !TryReadExactPropertyV1(
                    card, cardAssembly, cardType, "Toughness", out object? toughnessValue) ||
                (toughnessValue != null && !(toughnessValue is int)) ||
                !TryReadExactPropertyV1(
                    card, "DuelScene", duelCardType, "IsToken", out object? tokenValue) ||
                !(tokenValue is bool) ||
                !TryReadExactPropertyV1(
                    card,
                    "DuelScene",
                    duelCardType,
                    "CardAttachedTo",
                    out attachedTo) ||
                (attachedTo != null && attachedTo.GetType().FullName != duelCardType) ||
                !TryReadExactPropertyV1(
                    card,
                    "DuelScene",
                    duelCardType,
                    "VisibleCounters",
                    out object? countersValue) ||
                !TryBoundedCollectionV1(countersValue, 128, out List<object> counters))
            {
                return false;
            }

            foreach (object counter in counters)
            {
                const string counterType =
                    "Shiny.Play.Duel.ViewModel.CardCounterViewModel";
                if (!TryReadExactPropertyV1(
                        counter,
                        "DuelScene",
                        counterType,
                        "Quantity",
                        out object? quantityValue) ||
                    !(quantityValue is int quantity) ||
                    quantity < 0 || quantity > short.MaxValue ||
                    !TryReadExactPropertyV1(
                        counter,
                        "DuelScene",
                        counterType,
                        "Type",
                        out object? typeValue) ||
                    typeValue == null ||
                    typeValue.GetType().FullName != "WotC.MtGO.Client.Model.Play.Counter")
                {
                    return false;
                }
            }
            return true;
        }

        private static bool TryValidateConditionalVisibleZoneV1(
            object zone,
            bool enumerateRegardless,
            bool faceDownNameVisible)
        {
            const string zoneType = "Shiny.Play.Duel.ViewModel.ZoneViewModel";
            if (!TryReadExactPropertyV1(
                    zone,
                    "DuelScene",
                    zoneType,
                    "IsVisible",
                    out object? visibleValue) ||
                !(visibleValue is bool visible))
            {
                return false;
            }
            if (!enumerateRegardless && !visible)
            {
                return true;
            }
            return TryValidateEnumeratedVisibleZoneV1(zone, faceDownNameVisible);
        }

        private static bool TryValidateEnumeratedVisibleZoneV1(
            object zone,
            bool faceDownNameVisible)
        {
            const string zoneType = "Shiny.Play.Duel.ViewModel.ZoneViewModel";
            if (!TryReadExactPropertyV1(
                    zone, "DuelScene", zoneType, "IsVisible", out object? visibleValue) ||
                !(visibleValue is bool) ||
                !TryReadExactPropertyV1(
                    zone, "DuelScene", zoneType, "Count", out object? countValue) ||
                !(countValue is int count) || count < 0 ||
                count > MaximumVisibleCollectionItems ||
                !TryReadExactPropertyV1(
                    zone, "DuelScene", zoneType, "Cards", out object? cardsValue) ||
                !TryBoundedCollectionV1(
                    cardsValue,
                    MaximumVisibleCollectionItems,
                    out List<object> cards) ||
                cards.Count != count)
            {
                return false;
            }

            foreach (object card in cards)
            {
                if (!TryValidateVisibleZoneCardV1(card, faceDownNameVisible))
                {
                    return false;
                }
            }
            return true;
        }

        private static bool TryValidateVisibleZoneCardV1(
            object card,
            bool faceDownNameVisible)
        {
            const string cardType = "Shiny.Card.ViewModels.CardViewModel";
            if (!TryReadExactPropertyV1(
                    card, "Card", cardType, "IsFaceDown", out object? faceDownValue) ||
                !(faceDownValue is bool faceDown))
            {
                return false;
            }
            if (faceDown && !faceDownNameVisible)
            {
                return true;
            }
            return TryRequireNoUnrepresentedVisibleCardStateV1(card) &&
                TryReadExactPropertyV1(
                    card, "Card", cardType, "Name", out object? nameValue) &&
                nameValue is string name &&
                IsBoundedVisibleStringV1(name, 512, true);
        }

        private static bool TryRequireNoUnrepresentedVisibleCardStateV1(object card)
        {
            const string cardType = "Shiny.Card.ViewModels.CardViewModel";
            const string duelCardType =
                "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel";
            return TryReadExactPropertyV1(
                    card,
                    "Card",
                    cardType,
                    "CurrentDungeonRoom",
                    out object? dungeonRoomValue) &&
                dungeonRoomValue is int dungeonRoom && dungeonRoom == -1 &&
                TryReadExactPropertyV1(
                    card,
                    "DuelScene",
                    duelCardType,
                    "RingTemptationCounter",
                    out object? ringValue) &&
                ringValue is int ring && ring == 0 &&
                TryReadExactPropertyV1(
                    card,
                    "DuelScene",
                    duelCardType,
                    "SpeedCounter",
                    out object? speedValue) &&
                speedValue is int speed && speed == 0 &&
                TryReadExactPropertyV1(
                    card,
                    "DuelScene",
                    duelCardType,
                    "IsSpeedEmblem",
                    out object? speedEmblemValue) &&
                speedEmblemValue is bool speedEmblem && !speedEmblem;
        }

        private static bool TryValidateVisiblePlayerPanelV1(
            object player,
            out bool localPlayer,
            out bool activePlayer,
            out bool priorityPlayer)
        {
            localPlayer = false;
            activePlayer = false;
            priorityPlayer = false;
            const string typeName = "Shiny.Play.Duel.ViewModel.PlayerViewModel";
            if (!TryReadExactPropertyV1(
                    player, "DuelScene", typeName, "LocalPlayer", out object? localValue) ||
                !(localValue is bool local) ||
                !TryReadExactPropertyV1(
                    player, "DuelScene", typeName, "Active", out object? activeValue) ||
                !(activeValue is bool active) ||
                !TryReadExactPropertyV1(
                    player, "DuelScene", typeName, "MatActive", out object? priorityValue) ||
                !(priorityValue is bool priority) ||
                !TryReadExactPropertyV1(
                    player, "DuelScene", typeName, "Health", out object? lifeValue) ||
                !(lifeValue is int) ||
                !TryReadExactPropertyV1(
                    player, "DuelScene", typeName, "HandTotal", out object? handValue) ||
                !(handValue is int handCount) || handCount < 0 || handCount > 1000000 ||
                !TryReadExactPropertyV1(
                    player, "DuelScene", typeName, "DeckTotal", out object? deckValue) ||
                !(deckValue is int deckCount) || deckCount < 0 || deckCount > 1000000 ||
                !TryReadExactPropertyV1(
                    player,
                    "DuelScene",
                    typeName,
                    "ManaPoolItems",
                    out object? manaItemsValue) ||
                !TryBoundedCollectionV1(manaItemsValue, 16, out List<object> manaItems))
            {
                return false;
            }

            foreach (object manaItem in manaItems)
            {
                const string manaType = "Shiny.Play.Duel.ViewModel.ManaPoolItemViewModel";
                if (!TryReadExactPropertyV1(
                        manaItem,
                        "DuelScene",
                        manaType,
                        "ColorString",
                        out object? colorValue) ||
                    !(colorValue is string color) ||
                    !IsBoundedVisibleStringV1(color, 32, false) ||
                    !TryReadExactPropertyV1(
                        manaItem,
                        "DuelScene",
                        manaType,
                        "Count",
                        out object? countValue) ||
                    !(countValue is int count) || count < 0 || count > 1000000)
                {
                    return false;
                }
            }

            localPlayer = local;
            activePlayer = active;
            priorityPlayer = priority;
            return true;
        }

        private static bool TryValidateVisiblePromptV1(object promptBox)
        {
            const string typeName = "Shiny.Play.Duel.ViewModel.PromptBoxViewModel";
            if (!TryReadExactPropertyV1(
                    promptBox,
                    "DuelScene",
                    typeName,
                    "IsPromptBoxActive",
                    out object? activeValue) ||
                !(activeValue is bool active) ||
                !TryReadExactPropertyV1(
                    promptBox, "DuelScene", typeName, "Text", out object? textValue) ||
                (active && (!(textValue is string text) ||
                    !IsBoundedVisibleStringV1(text, MaximumVisibleTextCharacters, true))) ||
                (!active && textValue != null && !(textValue is string)) ||
                !TryReadExactPropertyV1(
                    promptBox,
                    "DuelScene",
                    typeName,
                    "StandardButtons",
                    out object? buttonsValue) ||
                !TryBoundedCollectionV1(buttonsValue, 64, out List<object> buttons))
            {
                return false;
            }

            foreach (object button in buttons)
            {
                const string buttonType = "Shiny.Play.Duel.ViewModel.OptionButton";
                if (!TryReadExactPropertyV1(
                        button, "DuelScene", buttonType, "Visible", out object? visibleValue) ||
                    !(visibleValue is bool visible) ||
                    !TryReadExactPropertyV1(
                        button, "DuelScene", buttonType, "Enabled", out object? enabledValue) ||
                    !(enabledValue is bool enabled) ||
                    !TryReadExactPropertyV1(
                        button, "DuelScene", buttonType, "Name", out object? nameValue) ||
                    (visible && (!(nameValue is string name) ||
                        !IsBoundedVisibleStringV1(name, 256, true))) ||
                    (!visible && nameValue != null && !(nameValue is string)) ||
                    (visible && enabled &&
                        (!TryReadExactPrivateVisibleActionPropertyV1(
                            button,
                            "DuelScene",
                            buttonType,
                            "Action",
                            out object? action) ||
                        action == null ||
                        !TryValidatePrivateVisibleActionV1(action, false))))
                {
                    return false;
                }
            }
            return true;
        }

        private static bool TryBoundedVisibleStringCollectionV1(
            object? value,
            int maximumItems)
        {
            if (value == null)
            {
                return true;
            }
            if (!TryBoundedCollectionV1(value, maximumItems, out List<object> items))
            {
                return false;
            }
            foreach (object item in items)
            {
                if (!(item is string text) ||
                    !IsBoundedVisibleStringV1(text, 512, true))
                {
                    return false;
                }
            }
            return true;
        }

        private static bool TryReadExactPropertyV1(
            object target,
            string assemblyName,
            string declaringTypeName,
            string propertyName,
            out object? value)
        {
            value = null;
            string exactKey = assemblyName + "|" + declaringTypeName + "|" + propertyName;
            if (!AllowedGetters.Contains(exactKey, StringComparer.Ordinal))
            {
                return false;
            }
            Assembly[] matchingAssemblies = AppDomain.CurrentDomain.GetAssemblies()
                .Where(assembly => string.Equals(
                    assembly.GetName().Name,
                    assemblyName,
                    StringComparison.Ordinal))
                .ToArray();
            if (matchingAssemblies.Length != 1)
            {
                return false;
            }
            Type declaringType = matchingAssemblies[0].GetType(
                declaringTypeName,
                false,
                false);
            if (declaringType == null || !declaringType.IsInstanceOfType(target))
            {
                return false;
            }
            PropertyInfo? property = declaringType.GetProperty(
                propertyName,
                BindingFlags.Instance | BindingFlags.Public | BindingFlags.FlattenHierarchy);
            MethodInfo? getter = property?.GetGetMethod(false);
            if (property == null || property.GetIndexParameters().Length != 0 ||
                getter == null || !getter.IsPublic || getter.GetParameters().Length != 0)
            {
                return false;
            }
            value = property.GetValue(target, null);
            return true;
        }

        private static bool TryReadExactPrivateVisibleActionPropertyV1(
            object target,
            string assemblyName,
            string declaringTypeName,
            string propertyName,
            out object? value)
        {
            value = null;
            string exactKey = assemblyName + "|" + declaringTypeName + "|" + propertyName;
            if (!PrivateVisibleActionJoinGetters.Contains(
                    exactKey,
                    StringComparer.Ordinal))
            {
                return false;
            }
            Assembly[] matchingAssemblies = AppDomain.CurrentDomain.GetAssemblies()
                .Where(assembly => string.Equals(
                    assembly.GetName().Name,
                    assemblyName,
                    StringComparison.Ordinal))
                .ToArray();
            if (matchingAssemblies.Length != 1)
            {
                return false;
            }
            Type declaringType = matchingAssemblies[0].GetType(
                declaringTypeName,
                false,
                false);
            if (declaringType == null || !declaringType.IsInstanceOfType(target))
            {
                return false;
            }
            PropertyInfo? property = declaringType.GetProperty(
                propertyName,
                BindingFlags.Instance | BindingFlags.Public | BindingFlags.FlattenHierarchy);
            MethodInfo? getter = property?.GetGetMethod(false);
            if (property == null || property.GetIndexParameters().Length != 0 ||
                getter == null || !getter.IsPublic || getter.GetParameters().Length != 0)
            {
                return false;
            }
            value = property.GetValue(target, null);
            return true;
        }

        private static bool TryBoundedCollectionV1(
            object? value,
            int maximumItems,
            out List<object> items)
        {
            items = new List<object>();
            if (value == null || value is string || !(value is IEnumerable enumerable) ||
                maximumItems < 0 || maximumItems > MaximumVisibleCollectionItems)
            {
                return false;
            }
            foreach (object? item in enumerable)
            {
                if (item == null || items.Count >= maximumItems)
                {
                    return false;
                }
                items.Add(item);
            }
            return true;
        }

        private static bool IsBoundedVisibleStringV1(
            string value,
            int maximumCharacters,
            bool allowLineBreaks)
        {
            if (value.Length == 0 || value.Length > maximumCharacters)
            {
                return false;
            }
            foreach (char character in value)
            {
                if (character == '\0' ||
                    (char.IsControl(character) &&
                        !(allowLineBreaks &&
                            (character == '\r' || character == '\n' || character == '\t'))))
                {
                    return false;
                }
            }
            return true;
        }

        private static bool IsExactChannelName(string value)
        {
            if (value == null || value.Length != ChannelPrefix.Length + 64 ||
                !value.StartsWith(ChannelPrefix, StringComparison.Ordinal))
            {
                return false;
            }
            for (int index = ChannelPrefix.Length; index < value.Length; index++)
            {
                char character = value[index];
                if (!((character >= '0' && character <= '9') ||
                    (character >= 'a' && character <= 'f')))
                {
                    return false;
                }
            }
            return true;
        }

        private static bool WriteBrokerResult(string channelName, byte[] value)
        {
            if (!ReferenceEquals(value, DuelSurfaceUnavailable) &&
                !ReferenceEquals(value, SurfaceShapeMismatch) &&
                !ReferenceEquals(value, ProjectionIncomplete) &&
                !ReferenceEquals(value, OutputValidationFailed) &&
                !ReferenceEquals(value, VisibleActionSubmitted) &&
                !ReferenceEquals(value, VisibleActionRejected) &&
                !IsSanitizedVisibleDecisionResultV1(value))
            {
                value = OutputValidationFailed;
            }
            if (value.Length <= 0 || value.Length > MaximumOutputBytes - OutputPayloadOffset)
            {
                return false;
            }

            try
            {
                using (MemoryMappedFile channel = MemoryMappedFile.OpenExisting(
                    channelName,
                    MemoryMappedFileRights.ReadWrite))
                using (MemoryMappedViewAccessor view = channel.CreateViewAccessor(
                    0,
                    MaximumOutputBytes,
                    MemoryMappedFileAccess.ReadWrite))
                {
                    view.Write(0, 0);
                    view.Write(4, 1);
                    view.WriteArray(OutputPayloadOffset, value, 0, value.Length);
                    view.Write(0, value.Length);
                    view.Flush();
                    return true;
                }
            }
            catch
            {
                return false;
            }
        }

        private static bool IsSanitizedVisibleDecisionResultV1(byte[] value)
        {
            byte[] prefix = Encoding.UTF8.GetBytes(
                "{\"result_kind\":\"visible_decision\",\"decision\":");
            if (value == null || value.Length <= prefix.Length ||
                value.Length > MaximumOutputBytes - OutputPayloadOffset)
            {
                return false;
            }
            for (int index = 0; index < prefix.Length; index++)
            {
                if (value[index] != prefix[index])
                {
                    return false;
                }
            }
            return value[value.Length - 1] == (byte)'}';
        }
    }
}
