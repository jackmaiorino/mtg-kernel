using System;
using System.IO.MemoryMappedFiles;
using System.Text;
using System.Windows;
using MtgKernel.Mtgo.VisibleDuelProducer.V1;
using Shiny.Card.ViewModels;
using Shiny.Play.Duel;
using Shiny.Play.Duel.ViewModel;
using WotC.MtGO.Client.Model.Play;

namespace MtgKernel.Mtgo.VisibleChromeFixtureHost.V1
{
    internal static class Program
    {
        private const int Capacity = 1048576;

        [STAThread]
        private static int Main()
        {
            string channelName = "Local\\mtgkernel_mtgo_visible_v1_" + new string('b', 64);
            using (var channel = MemoryMappedFile.CreateNew(channelName, Capacity))
            using (var view = channel.CreateViewAccessor(
                0,
                Capacity,
                MemoryMappedFileAccess.ReadWrite))
            {
                var application = new Application();
                var viewModel = new DuelSceneViewModel();
                var seated = new PlayerViewModel
                {
                    IsLocalFixture = true,
                    IsActiveFixture = true,
                    IsPriorityFixture = true,
                    HandTotalFixture = 7
                };
                seated.ManaItems.Add(new ManaPoolItemViewModel());
                var opponent = new PlayerViewModel
                {
                    HandTotalFixture = 7
                };
                opponent.ManaItems.Add(new ManaPoolItemViewModel());
                var localPermanent = new DuelSceneCardViewModel
                {
                    NameFixture = "fixture-visible-permanent",
                    PowerFixture = 2,
                    ToughnessFixture = 3
                };
                localPermanent.CounterItems.Add(new CardCounterViewModel
                {
                    QuantityFixture = 1
                });
                localPermanent.ActionItems.Add(new VisibleFixtureCardAction
                {
                    NameFixture = "Add White",
                    ManaFixture = true,
                    CastFixture = false
                });
                seated.Battlefield.Add(localPermanent);
                opponent.Battlefield.Add(new DuelSceneCardViewModel
                {
                    NameFixture = "fixture-visible-opponent-permanent",
                    PowerFixture = 2,
                    ToughnessFixture = 2,
                    ThrowIfActionsReadFixture = true
                });
                seated.Hand.IsVisibleFixture = true;
                seated.Hand.CardItems.Add(localPermanent);
                for (int index = 1; index < 7; index++)
                {
                    seated.Hand.CardItems.Add(new DuelSceneCardViewModel
                    {
                        NameFixture = "fixture-visible-own-hand-card-" + index
                    });
                }
                opponent.Hand.ThrowIfCardsReadFixture = true;
                opponent.Hand.CardItems.Add(new DuelSceneCardViewModel
                {
                    ThrowIfNameReadFixture = true
                });
                seated.Library.ThrowIfAnyGetterFixture = true;
                opponent.Library.ThrowIfAnyGetterFixture = true;
                seated.Revealed.ThrowIfCardsReadFixture = true;
                opponent.Revealed.ThrowIfCardsReadFixture = true;
                viewModel.PlayerItems.Add(seated);
                viewModel.PlayerItems.Add(opponent);
                viewModel.Prompt.Buttons.Add(new OptionButton
                {
                    ActionFixture = new VisibleFixtureCardAction()
                });
                var root = new DuelScene
                {
                    DataContext = viewModel,
                    Width = 640,
                    Height = 480
                };
                var window = new Window
                {
                    Content = root,
                    Width = 640,
                    Height = 480,
                    Left = -10000,
                    Top = -10000,
                    ShowInTaskbar = false,
                    WindowStyle = WindowStyle.None
                };
                window.Show();
                try
                {
                    // First exercise the full battlefield and counter getter
                    // surface while the opening-only sanitizer must abstain.
                    int incompleteStatus =
                        VisibleDuelProducerV1.ExportVisibleDecisionOrAbstainV1(channelName);
                    int incompleteLength = view.ReadInt32(0);
                    byte[] incompleteBytes = new byte[incompleteLength];
                    view.ReadArray(8, incompleteBytes, 0, incompleteBytes.Length);
                    if (incompleteStatus != 0 ||
                        Encoding.UTF8.GetString(incompleteBytes) !=
                            "{\"result_kind\":\"abstained\",\"reason\":\"projection_incomplete\"}")
                    {
                        return 8;
                    }
                    seated.Battlefield.Clear();
                    opponent.Battlefield.Clear();

                    int status = VisibleDuelProducerV1.ExportVisibleDecisionOrAbstainV1(
                        channelName);
                    int length = view.ReadInt32(0);
                    int schema = view.ReadInt32(4);
                    if (status != 0 || schema != 1 || length <= 0 || length > Capacity - 8)
                    {
                        return 2;
                    }
                    byte[] bytes = new byte[length];
                    view.ReadArray(8, bytes, 0, bytes.Length);
                    string observed = Encoding.UTF8.GetString(bytes);
                    if (!observed.StartsWith(
                            "{\"result_kind\":\"visible_decision\",\"decision\":",
                            StringComparison.Ordinal) ||
                        !observed.Contains("\"current_state\"") ||
                        !observed.Contains("\"ordered_legal_actions\"") ||
                        observed.Contains("fixture-player") ||
                        observed.Contains("fixture-prompt"))
                    {
                        return 3;
                    }
                    if (!VisibleGetterProbeV1.SawEveryVisibleChromeGetterV1())
                    {
                        return 4;
                    }
                    if (!VisibleZoneGetterProbeV1.SawEveryVisibleZoneAndCardGetterV1() ||
                        !VisibleCardGetterProbeV1.SawEveryVisibleCardGetterV1())
                    {
                        return 5;
                    }
                    if (!VisibleActionJoinGetterProbeV1.SawEveryPrivateJoinRootV1())
                    {
                        return 6;
                    }
                    if (!PrivateVisibleActionGetterProbeV1
                        .SawEveryPrivateVisibleActionGetterV1())
                    {
                        return 7;
                    }
                    Console.WriteLine(Convert.ToBase64String(bytes));
                    return 0;
                }
                finally
                {
                    window.Close();
                    application.Shutdown();
                }
            }
        }
    }
}
