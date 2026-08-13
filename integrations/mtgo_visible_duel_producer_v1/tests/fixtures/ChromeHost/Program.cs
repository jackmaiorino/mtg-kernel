using System;
using System.IO.MemoryMappedFiles;
using System.Linq;
using System.Reflection;
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
                    if (!TryParseVisibleTurnFixtureV1("Turn 1", out uint turnOne) ||
                        turnOne != 1 ||
                        !TryParseVisibleTurnFixtureV1(
                            "Turn 42: fixture-visible-player",
                            out uint turnFortyTwo) ||
                        turnFortyTwo != 42 ||
                        TryParseVisibleTurnFixtureV1("Turn 01", out _) ||
                        TryParseVisibleTurnFixtureV1("Turn 1: ", out _) ||
                        TryParseVisibleTurnFixtureV1("Tour 1", out _))
                    {
                        return 28;
                    }
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

                    // A rendered Initiative emblem is recognized from the
                    // public presentation surface, but production remains
                    // closed until its live per-player placement is qualified.
                    seated.Shields.CardItems.Add(new DuelSceneCardViewModel
                    {
                        CardFrameIDFixture = Shiny.Card.Enums.FrameStyle.CLBInitiativeEmblem
                    });
                    if (!TryReadInitiativeHolderFixtureV1(
                            seated,
                            opponent,
                            out string? seatedHolder) ||
                        seatedHolder != "seated_player")
                    {
                        return 22;
                    }
                    int initiativeStatus =
                        VisibleDuelProducerV1.ExportVisibleDecisionOrAbstainV1(channelName);
                    int initiativeLength = view.ReadInt32(0);
                    byte[] initiativeBytes = new byte[initiativeLength];
                    view.ReadArray(8, initiativeBytes, 0, initiativeBytes.Length);
                    if (initiativeStatus != 0 ||
                        Encoding.UTF8.GetString(initiativeBytes) !=
                            "{\"result_kind\":\"abstained\",\"reason\":\"projection_incomplete\"}")
                    {
                        return 18;
                    }
                    seated.Shields.CardItems.Clear();

                    opponent.Shields.CardItems.Add(new DuelSceneCardViewModel
                    {
                        CardFrameIDFixture = Shiny.Card.Enums.FrameStyle.CLBInitiativeEmblem
                    });
                    if (!TryReadInitiativeHolderFixtureV1(
                            seated,
                            opponent,
                            out string? opponentHolder) ||
                        opponentHolder != "opponent")
                    {
                        return 23;
                    }
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 19;
                    }
                    seated.Shields.CardItems.Add(new DuelSceneCardViewModel
                    {
                        CardFrameIDFixture = Shiny.Card.Enums.FrameStyle.CLBInitiativeEmblem
                    });
                    if (TryReadInitiativeHolderFixtureV1(
                            seated,
                            opponent,
                            out _))
                    {
                        return 24;
                    }
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 20;
                    }
                    seated.Shields.CardItems.Clear();
                    opponent.Shields.CardItems.Clear();
                    seated.Shields.CardItems.Add(new DuelSceneCardViewModel
                    {
                        CardFrameIDFixture = Shiny.Card.Enums.FrameStyle.OtherVisibleEmblem
                    });
                    if (TryReadInitiativeHolderFixtureV1(
                            seated,
                            opponent,
                            out _))
                    {
                        return 25;
                    }
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 21;
                    }
                    seated.Shields.CardItems.Clear();

                    // The current player-visible decision schema does not
                    // encode these rendered player-counter badges. Their
                    // presence must therefore abstain instead of silently
                    // presenting an incomplete game state to the model.
                    foreach (Action<bool> setVisibleCounter in new Action<bool>[]
                    {
                        value => seated.HasEnergyCountersFixture = value,
                        value => seated.HasExperienceCountersFixture = value,
                        value => seated.HasPoisonCountersFixture = value,
                        value => seated.HasRadCountersFixture = value,
                        value => opponent.HasEnergyCountersFixture = value,
                        value => opponent.HasExperienceCountersFixture = value,
                        value => opponent.HasPoisonCountersFixture = value,
                        value => opponent.HasRadCountersFixture = value
                    })
                    {
                        setVisibleCounter(true);
                        if (!ExportsProjectionIncompleteV1(channelName, view))
                        {
                            return 29;
                        }
                        setVisibleCounter(false);
                    }
                    seated.Companion.IsVisibleFixture = true;
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 30;
                    }
                    seated.Companion.IsVisibleFixture = false;
                    opponent.Companion.IsVisibleFixture = true;
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 31;
                    }
                    opponent.Companion.IsVisibleFixture = false;

                    // Every currently known visible modal or auxiliary duel
                    // surface that is absent from the semantic schema must
                    // force a fixed abstention.
                    foreach (Action<bool> setVisibleModal in new Action<bool>[]
                    {
                        value => viewModel.IsPileZoneActiveFixture = value,
                        value => viewModel.IsWishingFromSideboardFixture = value,
                        value => viewModel.LocalTriggersPanelEnabledFixture = value,
                        value => viewModel.OpponentTriggersPanelEnabledFixture = value,
                        value => viewModel.StormCounterVisibleFixture = value,
                        value => viewModel.ThreePilePanelEnabledFixture = value,
                        value => viewModel.TwoPilePanelEnabledFixture = value,
                        value => viewModel.CardSelectionFixture.VisibleFixture = value,
                        value => viewModel.CardSelectorDialogFixture.VisibleFixture = value
                    })
                    {
                        setVisibleModal(true);
                        if (!ExportsProjectionIncompleteV1(channelName, view))
                        {
                            return 32;
                        }
                        setVisibleModal(false);
                    }
                    viewModel.CardSelectorsFixture.Items.Add(new object());
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 33;
                    }
                    viewModel.CardSelectorsFixture.Items.Clear();
                    viewModel.TemporaryZoneItems.Add(new TemporaryZoneViewModel
                    {
                        IsVisibleFixture = true
                    });
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 34;
                    }
                    viewModel.TemporaryZoneItems.Clear();

                    viewModel.IsCommanderFixture = true;
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 35;
                    }
                    viewModel.IsCommanderFixture = false;
                    viewModel.IsPlanechaseFixture = true;
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 36;
                    }
                    viewModel.IsPlanechaseFixture = false;

                    // These are public card-presentation mechanics, but the
                    // current decision schema has no corresponding fields.
                    // Any non-default value must therefore abstain.
                    seated.Hand.CardItems[0].CurrentDungeonRoomFixture = 0;
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 37;
                    }
                    seated.Hand.CardItems[0].CurrentDungeonRoomFixture = -1;
                    seated.Hand.CardItems[0].RingTemptationCounterFixture = 1;
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 38;
                    }
                    seated.Hand.CardItems[0].RingTemptationCounterFixture = 0;
                    seated.Hand.CardItems[0].SpeedCounterFixture = 1;
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 39;
                    }
                    seated.Hand.CardItems[0].SpeedCounterFixture = 0;
                    seated.Hand.CardItems[0].IsSpeedEmblemFixture = true;
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 40;
                    }
                    seated.Hand.CardItems[0].IsSpeedEmblemFixture = false;

                    // The semantic turn is derived only from the rendered
                    // GameTurnText. Non-canonical or non-opening visible text
                    // must fail closed before the first supported slice.
                    viewModel.GameTurnTextFixture = "Turn 01";
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 26;
                    }
                    viewModel.GameTurnTextFixture = "Turn 2";
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 27;
                    }
                    viewModel.GameTurnTextFixture =
                        "Turn 1: fixture-visible-local-player";

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
                    if (!VisibleActionJoinGetterProbeV1.SawEveryPrivateJoinRootV1())
                    {
                        return 6;
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

        private static bool ExportsProjectionIncompleteV1(
            string channelName,
            MemoryMappedViewAccessor view)
        {
            int status = VisibleDuelProducerV1.ExportVisibleDecisionOrAbstainV1(channelName);
            int length = view.ReadInt32(0);
            if (status != 0 || length <= 0 || length > Capacity - 8)
            {
                return false;
            }
            byte[] bytes = new byte[length];
            view.ReadArray(8, bytes, 0, bytes.Length);
            return Encoding.UTF8.GetString(bytes) ==
                "{\"result_kind\":\"abstained\",\"reason\":\"projection_incomplete\"}";
        }

        private static bool TryReadInitiativeHolderFixtureV1(
            PlayerViewModel seated,
            PlayerViewModel opponent,
            out string? holder)
        {
            holder = null;
            Type producerType = typeof(VisibleDuelProducerV1);
            MethodInfo? snapshotMethod = producerType.GetMethod(
                "TryBuildPlayerSnapshotV1",
                BindingFlags.Static | BindingFlags.NonPublic);
            MethodInfo? holderMethod = producerType.GetMethod(
                "TryMapVisibleInitiativeHolderV1",
                BindingFlags.Static | BindingFlags.NonPublic);
            if (snapshotMethod == null || holderMethod == null)
            {
                return false;
            }
            object?[] seatedArguments = { seated, null };
            object?[] opponentArguments = { opponent, null };
            if (!(snapshotMethod.Invoke(null, seatedArguments) is bool seatedOk) ||
                !seatedOk || seatedArguments[1] == null ||
                !(snapshotMethod.Invoke(null, opponentArguments) is bool opponentOk) ||
                !opponentOk || opponentArguments[1] == null)
            {
                return false;
            }
            object?[] holderArguments =
            {
                seatedArguments[1],
                opponentArguments[1],
                null
            };
            if (!(holderMethod.Invoke(null, holderArguments) is bool mapped) || !mapped)
            {
                return false;
            }
            holder = holderArguments[2] as string;
            return true;
        }

        private static bool TryParseVisibleTurnFixtureV1(
            string text,
            out uint turn)
        {
            turn = 0;
            MethodInfo? method = typeof(VisibleDuelProducerV1).GetMethod(
                "TryParseVisibleTurnV1",
                BindingFlags.Static | BindingFlags.NonPublic);
            if (method == null)
            {
                return false;
            }
            object?[] arguments = { text, null };
            if (!(method.Invoke(null, arguments) is bool parsed) ||
                !parsed || !(arguments[1] is uint parsedTurn))
            {
                return false;
            }
            turn = parsedTurn;
            return true;
        }
    }
}
