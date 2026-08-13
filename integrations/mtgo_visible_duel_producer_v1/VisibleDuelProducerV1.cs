using System;
using System.Collections.Generic;
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
    /// V1 deliberately calls no MTGO property getter and can emit only one
    /// fixed abstention. It proves the exact WPF root and compile-time getter
    /// allowlist can be located without heap scanning or unrestricted state.
    /// </summary>
    public static class VisibleDuelProducerV1
    {
        private const string DuelRootType = "Shiny.Play.Duel.DuelScene";
        private const string DuelViewModelType = "Shiny.Play.Duel.ViewModel.DuelSceneViewModel";
        private const int MaximumVisualNodes = 200000;
        private const int MaximumVisualDepth = 256;

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
        /// Returns one bounded broker-result JSON value. This method never
        /// returns raw property values, paths, identifiers, or diagnostics.
        /// </summary>
        public static byte[] ExportVisibleDecisionOrAbstainV1()
        {
            try
            {
                Application application = Application.Current;
                if (application == null || application.Dispatcher == null)
                {
                    return Clone(DuelSurfaceUnavailable);
                }

                Dispatcher dispatcher = application.Dispatcher;
                if (dispatcher.HasShutdownStarted || dispatcher.HasShutdownFinished)
                {
                    return Clone(DuelSurfaceUnavailable);
                }

                if (dispatcher.CheckAccess())
                {
                    return ExportOnUiThread(application);
                }
                return (byte[])dispatcher.Invoke(
                    DispatcherPriority.Send,
                    new Func<byte[]>(() => ExportOnUiThread(application)));
            }
            catch
            {
                return Clone(OutputValidationFailed);
            }
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
                    return Clone(SurfaceShapeMismatch);
                }
                if (roots.Count > 1)
                {
                    return Clone(SurfaceShapeMismatch);
                }
            }

            if (roots.Count != 1)
            {
                return Clone(DuelSurfaceUnavailable);
            }

            FrameworkElement root = roots[0];
            if (!root.IsLoaded || !root.IsVisible || root.ActualWidth <= 0 || root.ActualHeight <= 0)
            {
                return Clone(DuelSurfaceUnavailable);
            }

            object viewModel = root.DataContext;
            if (viewModel == null || viewModel.GetType().FullName != DuelViewModelType)
            {
                return Clone(SurfaceShapeMismatch);
            }

            return ValidateExactGetterSurface()
                ? Clone(ProjectionIncomplete)
                : Clone(SurfaceShapeMismatch);
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

        private static byte[] Clone(byte[] value)
        {
            return (byte[])value.Clone();
        }
    }
}
