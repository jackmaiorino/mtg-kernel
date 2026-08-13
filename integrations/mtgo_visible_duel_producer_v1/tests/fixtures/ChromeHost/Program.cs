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
                    // surface while an unsupported combat state must abstain.
                    localPermanent.IsAttackingFixture = true;
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
                    localPermanent.IsAttackingFixture = false;
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

                    // Ordinary local priority makes MTGO's prompt box active.
                    // The only admitted visible prompt shape is the same
                    // single enabled default OK control bound as Pass.
                    viewModel.Prompt.IsPromptBoxActiveFixture = false;
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 59;
                    }
                    viewModel.Prompt.IsPromptBoxActiveFixture = true;
                    viewModel.Prompt.Buttons.Add(new OptionButton
                    {
                        NameFixture = "fixture-visible-choice",
                        VisibleFixture = true,
                        EnabledFixture = true,
                        ActionFixture = new VisibleFixturePromptAction()
                    });
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 60;
                    }
                    viewModel.Prompt.Buttons.RemoveAt(
                        viewModel.Prompt.Buttons.Count - 1);
                    viewModel.Prompt.OkPromptButton.NameFixture =
                        "fixture-visible-choice";
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 61;
                    }
                    viewModel.Prompt.OkPromptButton.NameFixture = "OK";
                    ((VisibleFixturePromptAction)viewModel.Prompt.OkPromptButton
                        .ActionFixture!).ActionFlagsFixture = 0x4000u;
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 62;
                    }
                    ((VisibleFixturePromptAction)viewModel.Prompt.OkPromptButton
                        .ActionFixture!).ActionFlagsFixture = 1u;
                    viewModel.Prompt.NumberEntryFixture.EnabledFixture = true;
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 63;
                    }
                    viewModel.Prompt.NumberEntryFixture.EnabledFixture = false;
                    viewModel.Prompt.ManaButtonItems.Add(new OptionButtonMana());
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 64;
                    }
                    viewModel.Prompt.ManaButtonItems.Clear();

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

                    // Card_View.ProcessMouseUp transforms these private action
                    // fields into distinct visible menu shapes. Until those
                    // shapes are represented exactly, each non-default value
                    // must force the same generic abstention.
                    var simpleAction =
                        (VisibleFixtureCardAction)localPermanent.ActionItems[0];
                    simpleAction.AltMenuActionFixture = true;
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 41;
                    }
                    simpleAction.AltMenuActionFixture = false;
                    simpleAction.GroupNameFixture = "fixture-visible-group";
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 42;
                    }
                    simpleAction.GroupNameFixture = string.Empty;
                    simpleAction.ActionChoicesFixture = "fixture-visible-choice";
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 43;
                    }
                    simpleAction.ActionChoicesFixture = string.Empty;
                    simpleAction.ModeChoiceMappingFixture = "fixture-visible-mode";
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 44;
                    }
                    simpleAction.ModeChoiceMappingFixture = string.Empty;
                    simpleAction.IsSubmenuItemFixture = true;
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 45;
                    }
                    simpleAction.IsSubmenuItemFixture = false;
                    simpleAction.AttackVictimIdFixture = 1;
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 46;
                    }
                    simpleAction.AttackVictimIdFixture = -1;

                    simpleAction.TargetItems.Add(new object());
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 47;
                    }
                    simpleAction.TargetItems.Clear();
                    simpleAction.HasXTargetFixture = true;
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 48;
                    }
                    simpleAction.HasXTargetFixture = false;
                    simpleAction.InSideboardFixture = true;
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 49;
                    }
                    simpleAction.InSideboardFixture = false;
                    simpleAction.ConfirmModeStringFixture = "fixture-visible-confirm";
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 50;
                    }
                    simpleAction.ConfirmModeStringFixture = null;
                    simpleAction.ConfirmBeforeTargetingOwnCardFixture = true;
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 51;
                    }
                    simpleAction.ConfirmBeforeTargetingOwnCardFixture = false;
                    simpleAction.IsFakeActionFixture = true;
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 52;
                    }
                    simpleAction.IsFakeActionFixture = false;
                    simpleAction.ModeMinChoicesFixture = 1;
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 53;
                    }
                    simpleAction.ModeMinChoicesFixture = 0;
                    simpleAction.ModeMaxChoicesFixture = 1;
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 54;
                    }
                    simpleAction.ModeMaxChoicesFixture = 0;
                    simpleAction.XIsAMinimumFixture = true;
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 55;
                    }
                    simpleAction.XIsAMinimumFixture = false;
                    simpleAction.XDeterminedByTargetWithGreatestCMCFixture = true;
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 56;
                    }
                    simpleAction.XDeterminedByTargetWithGreatestCMCFixture = false;
                    simpleAction.XTargetDivisorFixture = 2;
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 57;
                    }
                    simpleAction.XTargetDivisorFixture = 1;

                    // The original untouched opening remains inside the
                    // broader noncombat main-phase slice.
                    int openingStatus =
                        VisibleDuelProducerV1.ExportVisibleDecisionOrAbstainV1(channelName);
                    int openingLength = view.ReadInt32(0);
                    byte[] openingBytes = new byte[openingLength];
                    view.ReadArray(8, openingBytes, 0, openingBytes.Length);
                    string opening = Encoding.UTF8.GetString(openingBytes);
                    if (openingStatus != 0 ||
                        !opening.StartsWith(
                            "{\"result_kind\":\"visible_decision\",\"decision\":",
                            StringComparison.Ordinal) ||
                        !opening.Contains("\"turn\":1") ||
                        !opening.Contains("\"phase\":\"main1\"") ||
                        !opening.Contains("\"life_totals\":[20,20]") ||
                        !opening.Contains("\"hand_counts\":[7,7]") ||
                        !opening.Contains("\"library_counts\":[53,53]"))
                    {
                        return 58;
                    }

                    // The semantic turn is derived only from the rendered
                    // GameTurnText. Non-canonical text and unsupported
                    // non-main phases fail closed.
                    viewModel.GameTurnTextFixture = "Turn 01";
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 26;
                    }
                    viewModel.GameTurnTextFixture = "Turn 2";
                    viewModel.CurrentPhaseFixture = GamePhase.Upkeep;
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 27;
                    }

                    // Exercise the general noncombat main-phase slice with
                    // visibly changed game values and populated public zones.
                    viewModel.CurrentPhaseFixture = GamePhase.PostCombatMain;
                    viewModel.GameTurnTextFixture =
                        "Turn 4: fixture-visible-opponent";
                    seated.IsActiveFixture = false;
                    opponent.IsActiveFixture = true;
                    seated.HealthFixture = 13;
                    opponent.HealthFixture = 7;
                    seated.DeckTotalFixture = 49;
                    opponent.DeckTotalFixture = 46;
                    seated.ManaItems[0].CountFixture = 2;
                    opponent.ManaItems[0].CountFixture = 1;
                    seated.Hand.CardItems.Remove(localPermanent);
                    seated.HandTotalFixture = 6;
                    opponent.HandTotalFixture = 4;
                    localPermanent.IsTappedFixture = true;
                    seated.Battlefield.Add(localPermanent);
                    opponent.Battlefield.Add(new DuelSceneCardViewModel
                    {
                        NameFixture = "fixture-visible-opponent-battlefield",
                        PowerFixture = 4,
                        ToughnessFixture = 4,
                        ThrowIfActionsReadFixture = true
                    });
                    seated.Graveyard.CardItems.Add(new DuelSceneCardViewModel
                    {
                        NameFixture = "fixture-visible-seated-graveyard"
                    });
                    opponent.Graveyard.CardItems.Add(new DuelSceneCardViewModel
                    {
                        NameFixture = "fixture-visible-opponent-graveyard",
                        ThrowIfActionsReadFixture = true
                    });
                    seated.Exile.CardItems.Add(new DuelSceneCardViewModel
                    {
                        NameFixture = "fixture-visible-seated-exile"
                    });
                    opponent.Exile.CardItems.Add(new DuelSceneCardViewModel
                    {
                        NameFixture = "fixture-visible-opponent-exile",
                        ThrowIfActionsReadFixture = true
                    });

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
                        !observed.Contains("\"turn\":4") ||
                        !observed.Contains("\"phase\":\"main2\"") ||
                        !observed.Contains("\"active_player\":\"opponent\"") ||
                        !observed.Contains("\"priority_player\":\"seated_player\"") ||
                        !observed.Contains("\"life_totals\":[13,7]") ||
                        !observed.Contains("\"hand_counts\":[6,4]") ||
                        !observed.Contains("\"library_counts\":[49,46]") ||
                        !observed.Contains("fixture-visible-permanent") ||
                        !observed.Contains("fixture-visible-seated-graveyard") ||
                        !observed.Contains("fixture-visible-opponent-graveyard") ||
                        !observed.Contains("fixture-visible-seated-exile") ||
                        !observed.Contains("fixture-visible-opponent-exile") ||
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
