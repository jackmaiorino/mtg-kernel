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
    /// V1.1 invokes only the exact allowlisted visible chrome and player-panel
    /// getters and can emit only fixed abstentions. It proves the exact WPF
    /// root and bounded public-value route without heap scanning or exporting
    /// client objects or values.
    /// </summary>
    public static class VisibleDuelProducerV1
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
            "DuelScene|Shiny.Play.Duel.ViewModel.CardCounterViewModel|Quantity",
            "DuelScene|Shiny.Play.Duel.ViewModel.CardCounterViewModel|Type",
            "DuelScene|Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel|CardAttachedTo",
            "DuelScene|Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel|IsToken",
            "DuelScene|Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel|VisibleCounters",
            "DuelScene|Shiny.Play.Duel.ViewModel.DuelSceneViewModel|CurrentPhase",
            "DuelScene|Shiny.Play.Duel.ViewModel.DuelSceneViewModel|GameTurnText",
            "DuelScene|Shiny.Play.Duel.ViewModel.DuelSceneViewModel|Players",
            "DuelScene|Shiny.Play.Duel.ViewModel.DuelSceneViewModel|PromptBox",
            "DuelScene|Shiny.Play.Duel.ViewModel.DuelSceneViewModel|StackZone",
            "DuelScene|Shiny.Play.Duel.ViewModel.ManaPoolItemViewModel|ColorString",
            "DuelScene|Shiny.Play.Duel.ViewModel.ManaPoolItemViewModel|Count",
            "DuelScene|Shiny.Play.Duel.ViewModel.OptionButton|Enabled",
            "DuelScene|Shiny.Play.Duel.ViewModel.OptionButton|Name",
            "DuelScene|Shiny.Play.Duel.ViewModel.OptionButton|Visible",
            "DuelScene|Shiny.Play.Duel.ViewModel.PlayerViewModel|Active",
            "DuelScene|Shiny.Play.Duel.ViewModel.PlayerViewModel|BattlefieldCards",
            "DuelScene|Shiny.Play.Duel.ViewModel.PlayerViewModel|DeckTotal",
            "DuelScene|Shiny.Play.Duel.ViewModel.PlayerViewModel|ExileZone",
            "DuelScene|Shiny.Play.Duel.ViewModel.PlayerViewModel|GraveyardZone",
            "DuelScene|Shiny.Play.Duel.ViewModel.PlayerViewModel|HandTotal",
            "DuelScene|Shiny.Play.Duel.ViewModel.PlayerViewModel|HandZone",
            "DuelScene|Shiny.Play.Duel.ViewModel.PlayerViewModel|Health",
            "DuelScene|Shiny.Play.Duel.ViewModel.PlayerViewModel|LibraryZone",
            "DuelScene|Shiny.Play.Duel.ViewModel.PlayerViewModel|LocalPlayer",
            "DuelScene|Shiny.Play.Duel.ViewModel.PlayerViewModel|ManaPoolItems",
            "DuelScene|Shiny.Play.Duel.ViewModel.PlayerViewModel|MatActive",
            "DuelScene|Shiny.Play.Duel.ViewModel.PlayerViewModel|Name",
            "DuelScene|Shiny.Play.Duel.ViewModel.PlayerViewModel|RevealedZone",
            "DuelScene|Shiny.Play.Duel.ViewModel.PromptBoxViewModel|IsPromptBoxActive",
            "DuelScene|Shiny.Play.Duel.ViewModel.PromptBoxViewModel|StandardButtons",
            "DuelScene|Shiny.Play.Duel.ViewModel.PromptBoxViewModel|Text",
            "DuelScene|Shiny.Play.Duel.ViewModel.ZoneViewModel|Cards",
            "DuelScene|Shiny.Play.Duel.ViewModel.ZoneViewModel|Count",
            "DuelScene|Shiny.Play.Duel.ViewModel.ZoneViewModel|IsVisible"
        };

        /// <summary>
        /// Writes one bounded broker-result JSON value to the broker-created
        /// local memory channel. The integer result is a fixed transport code
        /// and carries no client or game information.
        /// </summary>
        public static int ExportVisibleDecisionOrAbstainV1(string channelName)
        {
            if (!IsExactChannelName(channelName))
            {
                return 2;
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
                    result = ExportOnUiThread(application);
                }
                else
                {
                    result = (byte[])dispatcher.Invoke(
                        DispatcherPriority.Send,
                        new Func<byte[]>(() => ExportOnUiThread(application)));
                }
            }
            catch
            {
                result = OutputValidationFailed;
            }

            return WriteBrokerResult(channelName, result) ? 0 : 4;
        }

        private static byte[] ExportOnUiThread(Application application)
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

            // V1.1 qualifies the exact visible chrome and player-panel getter
            // route. The temporary values never leave this call, and the
            // producer still emits only projection_incomplete.
            if (!TryValidateVisibleChromeProjectionV1(viewModel))
            {
                return ProjectionIncomplete;
            }
            return ProjectionIncomplete;
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
            if (AllowedGetters.Length != 46 || AllowedGetters.Distinct(StringComparer.Ordinal).Count() != 46)
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
            return true;
        }

        private static bool TryValidateVisibleChromeProjectionV1(object viewModel)
        {
            if (!TryReadExactPropertyV1(
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
            return true;
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
                    !(enabledValue is bool) ||
                    !TryReadExactPropertyV1(
                        button, "DuelScene", buttonType, "Name", out object? nameValue) ||
                    (visible && (!(nameValue is string name) ||
                        !IsBoundedVisibleStringV1(name, 256, true))) ||
                    (!visible && nameValue != null && !(nameValue is string)))
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
                !ReferenceEquals(value, OutputValidationFailed))
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
    }
}
