using System;
using System.IO.MemoryMappedFiles;
using System.Linq;
using System.Security.Cryptography;
using System.Text;
using System.Windows;
using System.Windows.Threading;
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
        private static int Main(string[] args)
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
                    if (args.Length == 1 && string.Equals(
                            args[0],
                            "--wait-for-broker",
                            StringComparison.Ordinal))
                    {
                        seated.Battlefield.Clear();
                        opponent.Battlefield.Clear();
                        viewModel.GameFixture.ExpectedAction =
                            viewModel.Prompt.OkPromptButton.ActionFixture!;
                        var timeout = new DispatcherTimer
                        {
                            Interval = TimeSpan.FromMinutes(2)
                        };
                        timeout.Tick += (_, __) => window.Close();
                        timeout.Start();
                        application.Run(window);
                        return 0;
                    }
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
                    viewModel.GameFixture.ExpectedAction =
                        viewModel.Prompt.OkPromptButton.ActionFixture!;
                    string sha;
                    using (SHA256 sha256 = SHA256.Create())
                    {
                        sha = string.Concat(sha256.ComputeHash(bytes).Select(
                            value => value.ToString("x2")));
                    }
                    byte[] command = Encoding.ASCII.GetBytes(
                        "execute_visible_action_v1|" + sha + "|1");
                    view.Write(0, command.Length);
                    view.Write(4, 2);
                    view.WriteArray(8, command, 0, command.Length);
                    view.Flush();
                    if (view.ReadInt32(0) != command.Length || view.ReadInt32(4) != 2)
                    {
                        return 16;
                    }
                    int dispatchStatus =
                        VisibleDuelProducerV1.DispatchSelectedVisibleActionV1(channelName);
                    if (dispatchStatus != 0)
                    {
                        return 17;
                    }
                    int receiptLength = view.ReadInt32(0);
                    byte[] receipt = new byte[receiptLength];
                    view.ReadArray(8, receipt, 0, receipt.Length);
                    if (dispatchStatus != 0 || !viewModel.GameFixture.ExecutionObserved ||
                        viewModel.GameFixture.ExecutionCount != 1)
                    {
                        return 10;
                    }
                    string receiptText = Encoding.UTF8.GetString(receipt);
                    if (receiptText ==
                        "{\"result_kind\":\"action_dispatch_receipt\",\"status\":\"rejected\"}")
                    {
                        return 11;
                    }
                    if (receiptText !=
                        "{\"result_kind\":\"action_dispatch_receipt\",\"status\":\"submitted\"}")
                    {
                        if (receiptText.Contains("output_validation_failed"))
                        {
                            return 13;
                        }
                        if (receiptText.Contains("projection_incomplete"))
                        {
                            return 14;
                        }
                        return 12;
                    }
                    byte[] secondCommand = Encoding.ASCII.GetBytes(
                        "execute_visible_action_v1|" + sha + "|0");
                    view.Write(0, secondCommand.Length);
                    view.Write(4, 2);
                    view.WriteArray(8, secondCommand, 0, secondCommand.Length);
                    view.Flush();
                    int secondDispatchStatus =
                        VisibleDuelProducerV1.DispatchSelectedVisibleActionV1(channelName);
                    int secondReceiptLength = view.ReadInt32(0);
                    byte[] secondReceipt = new byte[secondReceiptLength];
                    view.ReadArray(8, secondReceipt, 0, secondReceipt.Length);
                    if (secondDispatchStatus != 0 ||
                        Encoding.UTF8.GetString(secondReceipt) !=
                            "{\"result_kind\":\"action_dispatch_receipt\",\"status\":\"rejected\"}" ||
                        viewModel.GameFixture.ExecutionCount != 1)
                    {
                        return 18;
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
