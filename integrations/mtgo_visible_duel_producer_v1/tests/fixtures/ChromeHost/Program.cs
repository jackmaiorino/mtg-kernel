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
                    Name = "fixture-visible-local",
                    IsLocalFixture = true,
                    IsActiveFixture = true,
                    IsPriorityFixture = true,
                    HandTotalFixture = 7
                };
                seated.ManaItems.Add(new ManaPoolItemViewModel());
                var opponent = new PlayerViewModel
                {
                    Name = "fixture-visible-opponent",
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
                            viewModel.Prompt.OkPromptButtonFixture.ActionFixture!;
                        var timeout = new DispatcherTimer
                        {
                            Interval = TimeSpan.FromMinutes(2)
                        };
                        timeout.Tick += (_, __) => window.Close();
                        timeout.Start();
                        application.Run(window);
                        return 0;
                    }
                    if (args.Length == 1 && string.Equals(
                            args[0],
                            "--wait-for-attacker-broker",
                            StringComparison.Ordinal))
                    {
                        seated.Battlefield.Clear();
                        opponent.Battlefield.Clear();
                        var attacker = new DuelSceneCardViewModel
                        {
                            NameFixture = "fixture-visible-broker-attacker",
                            PowerFixture = 2,
                            ToughnessFixture = 2
                        };
                        var attackAction = new VisibleFixtureCardAction
                        {
                            NameFixture = "Attack fixture-visible-opponent",
                            CastFixture = false,
                            AttackVictimIdFixture = 7
                        };
                        attacker.ActionItems.Add(attackAction);
                        seated.Battlefield.Add(attacker);
                        viewModel.CurrentPhaseFixture = GamePhase.DeclareAttackers;
                        viewModel.Prompt.ShowOkPromptButtonFixture = false;
                        viewModel.Prompt.Buttons.Remove(
                            viewModel.Prompt.OkPromptButtonFixture);
                        var brokerDoneButton = new OptionButton
                        {
                            NameFixture = "Done",
                            VisibleFixture = true,
                            EnabledFixture = true,
                            ActionFixture = new VisibleFixturePromptAction
                            {
                                NameFixture = "Done"
                            }
                        };
                        viewModel.Prompt.DoneButtonFixture = brokerDoneButton;
                        viewModel.Prompt.Buttons.Add(brokerDoneButton);
                        viewModel.GameFixture.ExpectedAction = attackAction;
                        var timeout = new DispatcherTimer
                        {
                            Interval = TimeSpan.FromMinutes(2)
                        };
                        timeout.Tick += (_, __) => window.Close();
                        timeout.Start();
                        application.Run(window);
                        return 0;
                    }
                    if (args.Length == 1 && string.Equals(
                            args[0],
                            "--wait-for-single-blocker-broker",
                            StringComparison.Ordinal))
                    {
                        seated.Battlefield.Clear();
                        opponent.Battlefield.Clear();
                        seated.IsActiveFixture = false;
                        opponent.IsActiveFixture = true;
                        var blocker = new DuelSceneCardViewModel
                        {
                            NameFixture = "fixture-visible-broker-single-blocker",
                            PowerFixture = 2,
                            ToughnessFixture = 2
                        };
                        var blockAction = new VisibleFixtureCardAction
                        {
                            NameFixture = "Block",
                            CastFixture = false
                        };
                        blocker.ActionItems.Add(blockAction);
                        seated.Battlefield.Add(blocker);
                        opponent.Battlefield.Add(new DuelSceneCardViewModel
                        {
                            NameFixture = "fixture-visible-broker-single-attacker",
                            VisuallyAttackingFixture = true,
                            ThrowIfActionsReadFixture = true
                        });
                        viewModel.CurrentPhaseFixture = GamePhase.DeclareBlockers;
                        viewModel.Prompt.ShowOkPromptButtonFixture = false;
                        viewModel.Prompt.Buttons.Remove(
                            viewModel.Prompt.OkPromptButtonFixture);
                        var brokerDoneButton = new OptionButton
                        {
                            NameFixture = "Done",
                            VisibleFixture = true,
                            EnabledFixture = true,
                            ActionFixture = new VisibleFixturePromptAction
                            {
                                NameFixture = "Done"
                            }
                        };
                        viewModel.Prompt.DoneButtonFixture = brokerDoneButton;
                        viewModel.Prompt.Buttons.Add(brokerDoneButton);
                        viewModel.GameFixture.ExpectedAction = blockAction;
                        var timeout = new DispatcherTimer
                        {
                            Interval = TimeSpan.FromMinutes(2)
                        };
                        timeout.Tick += (_, __) => window.Close();
                        timeout.Start();
                        application.Run(window);
                        return 0;
                    }
                    if (args.Length == 1 && string.Equals(
                            args[0],
                            "--wait-for-blocker-broker",
                            StringComparison.Ordinal))
                    {
                        seated.Battlefield.Clear();
                        opponent.Battlefield.Clear();
                        seated.IsActiveFixture = false;
                        opponent.IsActiveFixture = true;
                        var blocker = new DuelSceneCardViewModel
                        {
                            NameFixture = "fixture-visible-broker-blocker",
                            PowerFixture = 2,
                            ToughnessFixture = 2
                        };
                        var blockAction = new VisibleFixtureCardAction
                        {
                            NameFixture = "Block",
                            CastFixture = false
                        };
                        blockAction.TargetItems.Add(new VisibleFixtureTargetSet());
                        blocker.ActionItems.Add(blockAction);
                        seated.Battlefield.Add(blocker);
                        opponent.Battlefield.Add(new DuelSceneCardViewModel
                        {
                            NameFixture = "fixture-visible-broker-attacker-a",
                            VisuallyAttackingFixture = true,
                            ThrowIfActionsReadFixture = true
                        });
                        opponent.Battlefield.Add(new DuelSceneCardViewModel
                        {
                            NameFixture = "fixture-visible-broker-attacker-b",
                            VisuallyAttackingFixture = true,
                            ThrowIfActionsReadFixture = true
                        });
                        viewModel.CurrentPhaseFixture = GamePhase.DeclareBlockers;
                        viewModel.Prompt.ShowOkPromptButtonFixture = false;
                        viewModel.Prompt.Buttons.Remove(
                            viewModel.Prompt.OkPromptButtonFixture);
                        var brokerDoneButton = new OptionButton
                        {
                            NameFixture = "Done",
                            VisibleFixture = true,
                            EnabledFixture = true,
                            ActionFixture = new VisibleFixturePromptAction
                            {
                                NameFixture = "Done"
                            }
                        };
                        viewModel.Prompt.DoneButtonFixture = brokerDoneButton;
                        viewModel.Prompt.Buttons.Add(brokerDoneButton);
                        viewModel.GameFixture.ExpectedAction = blockAction;
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
                    viewModel.Prompt.OkPromptButtonFixture.NameFixture =
                        "fixture-visible-choice";
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 61;
                    }
                    viewModel.Prompt.OkPromptButtonFixture.NameFixture = "OK";
                    ((VisibleFixturePromptAction)viewModel.Prompt.OkPromptButtonFixture
                        .ActionFixture!).ActionFlagsFixture = 0x4000u;
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 62;
                    }
                    ((VisibleFixturePromptAction)viewModel.Prompt.OkPromptButtonFixture
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
                    // broader noncombat priority slice.
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
                    // GameTurnText. Non-canonical text fails closed.
                    viewModel.GameTurnTextFixture = "Turn 01";
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 26;
                    }
                    viewModel.GameTurnTextFixture = "Turn 2";

                    // Empty-stack priority during upkeep, draw, and end step
                    // uses the same complete visible state and action surface.
                    viewModel.CurrentPhaseFixture = GamePhase.Upkeep;
                    if (!ExportsVisiblePhaseV1(channelName, view, "upkeep"))
                    {
                        return 27;
                    }
                    viewModel.CurrentPhaseFixture = GamePhase.Draw;
                    if (!ExportsVisiblePhaseV1(channelName, view, "draw"))
                    {
                        return 59;
                    }
                    viewModel.CurrentPhaseFixture = GamePhase.EndOfTurn;
                    if (!ExportsVisiblePhaseV1(channelName, view, "end"))
                    {
                        return 60;
                    }
                    viewModel.CurrentPhaseFixture = GamePhase.BeginCombat;
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 61;
                    }

                    // MTGO exposes declare attackers as independently
                    // toggled visible cards plus one Done control. The
                    // producer exports that complete visible selection moment
                    // as a distinct multi-step result kind.
                    var firstAttacker = new DuelSceneCardViewModel
                    {
                        NameFixture = "fixture-visible-first-attacker",
                        PowerFixture = 2,
                        ToughnessFixture = 2
                    };
                    var firstAttackAction = new VisibleFixtureCardAction
                    {
                        NameFixture = "Attack fixture-visible-opponent",
                        CastFixture = false,
                        AttackVictimIdFixture = 7
                    };
                    firstAttacker.ActionItems.Add(firstAttackAction);
                    var secondAttacker = new DuelSceneCardViewModel
                    {
                        NameFixture = "fixture-visible-second-attacker",
                        PowerFixture = 3,
                        ToughnessFixture = 3,
                        IsAttackingFixture = true
                    };
                    var secondDontAttackAction = new VisibleFixtureCardAction
                    {
                        NameFixture = "Don't attack",
                        CastFixture = false,
                        AttackVictimIdFixture = -1
                    };
                    secondAttacker.ActionItems.Add(secondDontAttackAction);
                    seated.Battlefield.Add(firstAttacker);
                    seated.Battlefield.Add(secondAttacker);
                    viewModel.CurrentPhaseFixture = GamePhase.DeclareAttackers;
                    viewModel.Prompt.ShowOkPromptButtonFixture = false;
                    viewModel.Prompt.Buttons.Remove(
                        viewModel.Prompt.OkPromptButtonFixture);
                    var doneButton = new OptionButton
                    {
                        NameFixture = "Done",
                        VisibleFixture = true,
                        EnabledFixture = true,
                        ActionFixture = new VisibleFixturePromptAction
                        {
                            NameFixture = "Done"
                        }
                    };
                    viewModel.Prompt.DoneButtonFixture = doneButton;
                    viewModel.Prompt.Buttons.Add(doneButton);

                    int attackersStatus =
                        VisibleDuelProducerV1.ExportVisibleDecisionOrAbstainV1(channelName);
                    int attackersLength = view.ReadInt32(0);
                    byte[] attackersBytes = new byte[attackersLength];
                    view.ReadArray(8, attackersBytes, 0, attackersBytes.Length);
                    string visibleAttackers = Encoding.UTF8.GetString(attackersBytes);
                    if (attackersStatus != 0 ||
                        !visibleAttackers.StartsWith(
                            "{\"result_kind\":\"visible_attacker_selection\",\"selection\":",
                            StringComparison.Ordinal) ||
                        !visibleAttackers.Contains("\"phase\":\"declare_attackers\"") ||
                        !visibleAttackers.Contains("\"ordered_attackers\":[{\"visible_ordinal\":1}]") ||
                        !visibleAttackers.Contains("\"currently_attacking\":false") ||
                        !visibleAttackers.Contains("\"currently_attacking\":true") ||
                        !visibleAttackers.Contains("\"attack_opponent_action_visible\":true") ||
                        !visibleAttackers.Contains("\"dont_attack_action_visible\":true") ||
                        !visibleAttackers.Contains("\"unique_visible_enabled_done_control\":true") ||
                        visibleAttackers.Contains("fixture-visible-opponent") ||
                        visibleAttackers.Contains("attack_victim"))
                    {
                        return 71;
                    }

                    string attackersSha;
                    using (SHA256 sha256 = SHA256.Create())
                    {
                        attackersSha = string.Concat(sha256.ComputeHash(attackersBytes).Select(
                            value => value.ToString("x2")));
                    }
                    byte[] attackersCommand = Encoding.ASCII.GetBytes(
                        "execute_visible_action_v1|" + attackersSha + "|0");
                    view.Write(0, attackersCommand.Length);
                    view.Write(4, 2);
                    view.WriteArray(8, attackersCommand, 0, attackersCommand.Length);
                    view.Flush();
                    int attackersDispatchStatus =
                        VisibleDuelProducerV1.DispatchSelectedVisibleActionV1(channelName);
                    int attackersReceiptLength = view.ReadInt32(0);
                    byte[] attackersReceipt = new byte[attackersReceiptLength];
                    view.ReadArray(8, attackersReceipt, 0, attackersReceipt.Length);
                    if (attackersDispatchStatus != 0 ||
                        Encoding.UTF8.GetString(attackersReceipt) !=
                            "{\"result_kind\":\"action_dispatch_receipt\",\"status\":\"rejected\"}" ||
                        viewModel.GameFixture.ExecutionCount != 0)
                    {
                        return 75;
                    }

                    // A plan selects the first candidate and deselects the
                    // second. Every producer call rebuilds and hashes the
                    // current visible selection. The client objects stay
                    // private, and the only output is a fixed receipt.
                    const string desiredMaskHex = "0000000000000001";
                    string planCommitment = AttackerPlanCommitmentFixtureV1(
                        attackersSha,
                        2,
                        desiredMaskHex);
                    viewModel.GameFixture.ExpectedAction = firstAttackAction;
                    if (!DispatchAttackerStepFixtureV1(
                            channelName,
                            view,
                            attackersSha,
                            2,
                            desiredMaskHex,
                            planCommitment,
                            true) ||
                        viewModel.GameFixture.ExecutionCount != 1)
                    {
                        return 76;
                    }

                    var firstDontAttackAction = new VisibleFixtureCardAction
                    {
                        NameFixture = "Don't attack",
                        CastFixture = false,
                        AttackVictimIdFixture = -1
                    };
                    firstAttacker.IsAttackingFixture = true;
                    firstAttacker.ActionItems.Clear();
                    firstAttacker.ActionItems.Add(firstDontAttackAction);
                    byte[] afterFirstBytes = ExportVisibleBytesFixtureV1(channelName, view);
                    string afterFirstSha = LowerSha256FixtureV1(afterFirstBytes);
                    viewModel.GameFixture.ExpectedAction = secondDontAttackAction;
                    if (!DispatchAttackerStepFixtureV1(
                            channelName,
                            view,
                            afterFirstSha,
                            2,
                            desiredMaskHex,
                            planCommitment,
                            true) ||
                        viewModel.GameFixture.ExecutionCount != 2)
                    {
                        return 78;
                    }

                    var secondAttackAction = new VisibleFixtureCardAction
                    {
                        NameFixture = "Attack fixture-visible-opponent",
                        CastFixture = false,
                        AttackVictimIdFixture = 7
                    };
                    secondAttacker.IsAttackingFixture = false;
                    secondAttacker.ActionItems.Clear();
                    secondAttacker.ActionItems.Add(secondAttackAction);
                    byte[] completedBytes = ExportVisibleBytesFixtureV1(channelName, view);
                    string completedSha = LowerSha256FixtureV1(completedBytes);
                    viewModel.GameFixture.ExpectedAction = doneButton.ActionFixture;
                    if (!DispatchAttackerStepFixtureV1(
                            channelName,
                            view,
                            completedSha,
                            2,
                            desiredMaskHex,
                            planCommitment,
                            true) ||
                        viewModel.GameFixture.ExecutionCount != 3)
                    {
                        return 79;
                    }
                    if (!DispatchAttackerStepFixtureV1(
                            channelName,
                            view,
                            completedSha,
                            2,
                            desiredMaskHex,
                            planCommitment,
                            false) ||
                        viewModel.GameFixture.ExecutionCount != 3)
                    {
                        return 80;
                    }
                    viewModel.GameFixture.ResetExecutionFixture();
                    firstAttacker.IsAttackingFixture = false;
                    firstAttacker.ActionItems.Clear();
                    firstAttacker.ActionItems.Add(firstAttackAction);
                    secondAttacker.IsAttackingFixture = true;
                    secondAttacker.ActionItems.Clear();
                    secondAttacker.ActionItems.Add(secondDontAttackAction);

                    firstAttackAction.NameFixture =
                        "Attack fixture-visible-opponent and exert";
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 72;
                    }
                    firstAttackAction.NameFixture = "Attack fixture-visible-opponent";
                    var secondVictim = new VisibleFixtureCardAction
                    {
                        NameFixture = "Attack fixture-visible-planeswalker",
                        CastFixture = false,
                        AttackVictimIdFixture = 9
                    };
                    firstAttacker.ActionItems.Add(secondVictim);
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 73;
                    }
                    firstAttacker.ActionItems.Remove(secondVictim);
                    viewModel.Prompt.DoneButtonFixture = null;
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 74;
                    }

                    // The first blocker slice deliberately accepts exactly
                    // one visually attacking
                    // opposing creature with an initially empty block lane.
                    // Backing IsAttacking and IsBlocking fixture values are
                    // inverted here to prove the outward state follows the
                    // rendered presentation properties.
                    viewModel.Prompt.DoneButtonFixture = doneButton;
                    seated.Battlefield.Clear();
                    opponent.Battlefield.Clear();
                    seated.IsActiveFixture = false;
                    opponent.IsActiveFixture = true;
                    var firstBlocker = new DuelSceneCardViewModel
                    {
                        NameFixture = "fixture-visible-first-blocker",
                        PowerFixture = 2,
                        ToughnessFixture = 2,
                        IsBlockingFixture = true,
                        VisuallyBlockingFixture = false
                    };
                    var firstBlockAction = new VisibleFixtureCardAction
                    {
                        NameFixture = "Block",
                        CastFixture = false
                    };
                    firstBlocker.ActionItems.Add(firstBlockAction);
                    var secondBlocker = new DuelSceneCardViewModel
                    {
                        NameFixture = "fixture-visible-second-blocker",
                        PowerFixture = 3,
                        ToughnessFixture = 3
                    };
                    var secondBlockAction = new VisibleFixtureCardAction
                    {
                        NameFixture = "Block",
                        CastFixture = false
                    };
                    secondBlocker.ActionItems.Add(secondBlockAction);
                    var singleVisibleAttacker = new DuelSceneCardViewModel
                    {
                        NameFixture = "fixture-visible-single-attacker",
                        PowerFixture = 4,
                        ToughnessFixture = 4,
                        IsAttackingFixture = false,
                        VisuallyAttackingFixture = true,
                        ThrowIfActionsReadFixture = true
                    };
                    seated.Battlefield.Add(firstBlocker);
                    seated.Battlefield.Add(secondBlocker);
                    opponent.Battlefield.Add(singleVisibleAttacker);
                    viewModel.CurrentPhaseFixture = GamePhase.DeclareBlockers;
                    int blockersStatus =
                        VisibleDuelProducerV1.ExportVisibleDecisionOrAbstainV1(channelName);
                    int blockersLength = view.ReadInt32(0);
                    byte[] blockersBytes = new byte[blockersLength];
                    view.ReadArray(8, blockersBytes, 0, blockersBytes.Length);
                    string visibleBlockers = Encoding.UTF8.GetString(blockersBytes);
                    if (blockersStatus != 0 ||
                        !visibleBlockers.StartsWith(
                            "{\"result_kind\":\"visible_single_attacker_blocker_selection\",\"selection\":",
                            StringComparison.Ordinal) ||
                        !visibleBlockers.Contains("\"phase\":\"declare_blockers\"") ||
                        !visibleBlockers.Contains("\"active_player\":\"opponent\"") ||
                        !visibleBlockers.Contains("\"attackers_declared\":true") ||
                        !visibleBlockers.Contains("\"ordered_attackers\":[{\"visible_ordinal\":2}]") ||
                        !visibleBlockers.Contains("\"attacker\":{\"visible_ordinal\":2}") ||
                        !visibleBlockers.Contains("\"blocker\":{\"visible_ordinal\":0}") ||
                        !visibleBlockers.Contains("\"blocker\":{\"visible_ordinal\":1}") ||
                        !visibleBlockers.Contains("\"currently_blocking\":false") ||
                        !visibleBlockers.Contains("\"block_action_visible\":true") ||
                        !visibleBlockers.Contains("\"unique_visible_enabled_done_control\":true") ||
                        visibleBlockers.Contains("target_id") ||
                        visibleBlockers.Contains("GameCard"))
                    {
                        return 81;
                    }

                    string blockersSha = LowerSha256FixtureV1(blockersBytes);
                    byte[] blockersCommand = Encoding.ASCII.GetBytes(
                        "execute_visible_action_v1|" + blockersSha + "|0");
                    view.Write(0, blockersCommand.Length);
                    view.Write(4, 2);
                    view.WriteArray(8, blockersCommand, 0, blockersCommand.Length);
                    view.Flush();
                    if (VisibleDuelProducerV1.DispatchSelectedVisibleActionV1(channelName) != 0 ||
                        Encoding.UTF8.GetString(ReadPayloadFixtureV1(view)) !=
                            "{\"result_kind\":\"action_dispatch_receipt\",\"status\":\"rejected\"}" ||
                        viewModel.GameFixture.ExecutionCount != 0)
                    {
                        return 82;
                    }

                    // Execute a complete monotonic single-attacker blocking
                    // plan. Each next input is locked until the previous
                    // blocker appears in the freshly rendered block lane.
                    const string desiredBlockerMaskHex = "0000000000000003";
                    string blockerPlanCommitment = SingleBlockerPlanCommitmentFixtureV1(
                        blockersSha,
                        2,
                        desiredBlockerMaskHex);
                    viewModel.GameFixture.ExpectedAction = firstBlockAction;
                    if (!DispatchSingleBlockerStepFixtureV1(
                            channelName,
                            view,
                            blockersSha,
                            2,
                            desiredBlockerMaskHex,
                            blockerPlanCommitment,
                            true) ||
                        viewModel.GameFixture.ExecutionCount != 1)
                    {
                        return 181;
                    }
                    firstBlocker.VisuallyBlockingFixture = true;
                    firstBlocker.VisualBlockingOrderItems.Add(
                        new OrderedCombatParticipant
                        {
                            Order = 1,
                            Target = singleVisibleAttacker.GameCardFixture
                        });
                    firstBlocker.ActionItems.Clear();
                    byte[] firstBlockerVisibleBytes = ExportVisibleBytesFixtureV1(
                        channelName,
                        view);
                    string firstBlockerVisible = Encoding.UTF8.GetString(
                        firstBlockerVisibleBytes);
                    if (!firstBlockerVisible.StartsWith(
                            "{\"result_kind\":\"visible_single_attacker_blocker_execution_state\",\"selection\":",
                            StringComparison.Ordinal) ||
                        !firstBlockerVisible.Contains(
                            "\"blocker_assignments\":[{\"attacker\":{\"visible_ordinal\":2},\"ordered_blockers\":[{\"visible_ordinal\":0}]}]") ||
                        !firstBlockerVisible.Contains(
                            "\"blocker\":{\"visible_ordinal\":0},\"currently_blocking\":true") ||
                        firstBlockerVisible.Contains("GameCard"))
                    {
                        return 182;
                    }
                    string firstBlockerVisibleSha = LowerSha256FixtureV1(
                        firstBlockerVisibleBytes);
                    viewModel.GameFixture.ExpectedAction = secondBlockAction;
                    if (!DispatchSingleBlockerStepFixtureV1(
                            channelName,
                            view,
                            blockersSha,
                            2,
                            desiredBlockerMaskHex,
                            blockerPlanCommitment,
                            false) ||
                        viewModel.GameFixture.ExecutionCount != 1 ||
                        !DispatchSingleBlockerStepFixtureV1(
                            channelName,
                            view,
                            firstBlockerVisibleSha,
                            2,
                            desiredBlockerMaskHex,
                            blockerPlanCommitment,
                            true) ||
                        viewModel.GameFixture.ExecutionCount != 2)
                    {
                        return 183;
                    }
                    secondBlocker.VisuallyBlockingFixture = true;
                    secondBlocker.VisualBlockingOrderItems.Add(
                        new OrderedCombatParticipant
                        {
                            Order = 2,
                            Target = singleVisibleAttacker.GameCardFixture
                        });
                    secondBlocker.ActionItems.Clear();
                    byte[] completedBlockersBytes = ExportVisibleBytesFixtureV1(
                        channelName,
                        view);
                    string completedBlockers = Encoding.UTF8.GetString(
                        completedBlockersBytes);
                    if (!completedBlockers.Contains(
                            "\"ordered_blockers\":[{\"visible_ordinal\":0},{\"visible_ordinal\":1}]") ||
                        !completedBlockers.Contains(
                            "\"blocker\":{\"visible_ordinal\":1},\"currently_blocking\":true") ||
                        completedBlockers.Contains("GameCard"))
                    {
                        return 184;
                    }
                    string completedBlockersSha = LowerSha256FixtureV1(
                        completedBlockersBytes);
                    viewModel.GameFixture.ExpectedAction = doneButton.ActionFixture;
                    if (!DispatchSingleBlockerStepFixtureV1(
                            channelName,
                            view,
                            completedBlockersSha,
                            2,
                            desiredBlockerMaskHex,
                            blockerPlanCommitment,
                            true) ||
                        viewModel.GameFixture.ExecutionCount != 3 ||
                        !DispatchSingleBlockerStepFixtureV1(
                            channelName,
                            view,
                            completedBlockersSha,
                            2,
                            desiredBlockerMaskHex,
                            blockerPlanCommitment,
                            false) ||
                        viewModel.GameFixture.ExecutionCount != 3)
                    {
                        return 185;
                    }

                    // Restore the empty visible block lane for the existing
                    // multi-attacker target-selection fixture.
                    firstBlocker.VisualBlockingOrderItems.Clear();
                    secondBlocker.VisualBlockingOrderItems.Clear();
                    firstBlocker.VisuallyBlockingFixture = false;
                    secondBlocker.VisuallyBlockingFixture = false;
                    firstBlocker.ActionItems.Add(firstBlockAction);
                    secondBlocker.ActionItems.Add(secondBlockAction);
                    viewModel.GameFixture.ResetExecutionFixture();

                    var secondVisibleAttacker = new DuelSceneCardViewModel
                    {
                        NameFixture = "fixture-visible-second-opposing-attacker",
                        VisuallyAttackingFixture = true,
                        ThrowIfActionsReadFixture = true
                    };
                    opponent.Battlefield.Add(secondVisibleAttacker);
                    firstBlockAction.TargetItems.Add(new VisibleFixtureTargetSet());
                    secondBlockAction.TargetItems.Add(new VisibleFixtureTargetSet());
                    var firstAttackerCardView = new Card_View
                    {
                        DataContext = singleVisibleAttacker,
                        Width = 80,
                        Height = 112
                    };
                    var secondAttackerCardView = new Card_View
                    {
                        DataContext = secondVisibleAttacker,
                        Width = 80,
                        Height = 112
                    };
                    root.Children.Add(firstAttackerCardView);
                    root.Children.Add(secondAttackerCardView);
                    window.Dispatcher.Invoke(
                        DispatcherPriority.Loaded,
                        new Action(() => { }));
                    root.UpdateLayout();
                    int multiBlockersStatus =
                        VisibleDuelProducerV1.ExportVisibleDecisionOrAbstainV1(channelName);
                    byte[] multiBlockersBytes = ReadPayloadFixtureV1(view);
                    string visibleMultiBlockers = Encoding.UTF8.GetString(
                        multiBlockersBytes);
                    if (multiBlockersStatus != 0 ||
                        !visibleMultiBlockers.StartsWith(
                            "{\"result_kind\":\"visible_multi_attacker_blocker_selection\",\"selection\":",
                            StringComparison.Ordinal) ||
                        !visibleMultiBlockers.Contains(
                            "\"ordered_attackers\":[{\"visible_ordinal\":2},{\"visible_ordinal\":3}]") ||
                        !visibleMultiBlockers.Contains(
                            "\"ordered_available_blockers\":[{\"visible_ordinal\":0},{\"visible_ordinal\":1}]") ||
                        !visibleMultiBlockers.Contains(
                            "\"blocker_assignments\":[]") ||
                        !visibleMultiBlockers.Contains(
                            "\"unique_visible_enabled_done_control\":true") ||
                        visibleMultiBlockers.Contains("GameCard") ||
                        visibleMultiBlockers.Contains("TargetSet"))
                    {
                        return 83;
                    }

                    string multiBlockersSha = LowerSha256FixtureV1(multiBlockersBytes);
                    string chooseBlockerModelCommitment = new string('4', 64);
                    string chooseBlockerStepCommitment = BlockerStepCommitmentFixtureV1(
                        multiBlockersSha,
                        1,
                        "b",
                        "0",
                        "-",
                        chooseBlockerModelCommitment);
                    viewModel.GameFixture.ExpectedAction = firstBlockAction;
                    if (!DispatchBlockerStepFixtureV1(
                            channelName,
                            view,
                            multiBlockersSha,
                            1,
                            "b",
                            "0",
                            "-",
                            chooseBlockerModelCommitment,
                            chooseBlockerStepCommitment,
                            true) ||
                        viewModel.GameFixture.ExecutionCount != 1 ||
                        !DispatchBlockerStepFixtureV1(
                            channelName,
                            view,
                            multiBlockersSha,
                            2,
                            "b",
                            "1",
                            "-",
                            new string('5', 64),
                            BlockerStepCommitmentFixtureV1(
                                multiBlockersSha,
                                2,
                                "b",
                                "1",
                                "-",
                                new string('5', 64)),
                            false) || viewModel.GameFixture.ExecutionCount != 1)
                    {
                        return 86;
                    }

                    firstBlocker.IsTargetingFixture = true;
                    singleVisibleAttacker.IsTargetableFixture = true;
                    secondVisibleAttacker.IsTargetableFixture = true;
                    viewModel.InteractionStateFixture.ModeFixture =
                        Shiny.Play.Duel.Utility.InteractMode.SelectTargets;
                    viewModel.InteractionStateFixture.SourceFixture = firstBlockAction;
                    int targetStatus =
                        VisibleDuelProducerV1.ExportVisibleDecisionOrAbstainV1(channelName);
                    string visibleTargets = Encoding.UTF8.GetString(
                        ReadPayloadFixtureV1(view));
                    if (targetStatus != 0 ||
                        !visibleTargets.StartsWith(
                            "{\"result_kind\":\"visible_blocker_target_selection\",\"selection\":",
                            StringComparison.Ordinal) ||
                        !visibleTargets.Contains("\"blocker\":{\"visible_ordinal\":0}") ||
                        !visibleTargets.Contains(
                            "\"ordered_visible_targetable_attackers\":[{\"visible_ordinal\":2},{\"visible_ordinal\":3}]") ||
                        visibleTargets.Contains("LegalTargets") ||
                        visibleTargets.Contains("TargetSet") ||
                        visibleTargets.Contains("GameCard"))
                    {
                        return 84;
                    }

                    byte[] targetBytes = ReadPayloadFixtureV1(view);
                    string targetSha = LowerSha256FixtureV1(targetBytes);
                    string targetModelCommitment = new string('6', 64);
                    string targetStepCommitment = BlockerStepCommitmentFixtureV1(
                        targetSha,
                        1,
                        "t",
                        "0",
                        "3",
                        targetModelCommitment);
                    viewModel.TargetClickBlockerFixture = firstBlocker;
                    viewModel.TargetClickAttackerFixture = secondVisibleAttacker;
                    viewModel.TargetClickOtherAttackersFixture.Add(singleVisibleAttacker);
                    viewModel.TargetClickActionFixture = firstBlockAction;
                    viewModel.GameFixture.ExpectedAction = firstBlockAction;
                    if (!DispatchBlockerStepFixtureV1(
                            channelName,
                            view,
                            targetSha,
                            1,
                            "t",
                            "0",
                            "3",
                            targetModelCommitment,
                            targetStepCommitment,
                            true))
                    {
                        return 87;
                    }
                    if (viewModel.GameFixture.ExecutionCount != 2)
                    {
                        return 89;
                    }
                    if (viewModel.TargetClickCountFixture != 1)
                    {
                        return 90;
                    }

                    int assignedStatus =
                        VisibleDuelProducerV1.ExportVisibleDecisionOrAbstainV1(channelName);
                    byte[] assignedBytes = ReadPayloadFixtureV1(view);
                    string visibleAssignedBlockers = Encoding.UTF8.GetString(
                        assignedBytes);
                    if (assignedStatus != 0 ||
                        !visibleAssignedBlockers.StartsWith(
                            "{\"result_kind\":\"visible_multi_attacker_blocker_selection\",\"selection\":",
                            StringComparison.Ordinal) ||
                        !visibleAssignedBlockers.Contains(
                            "\"blocker_assignments\":[{\"attacker\":{\"visible_ordinal\":3},\"ordered_blockers\":[{\"visible_ordinal\":0}]}]") ||
                        !visibleAssignedBlockers.Contains(
                            "\"ordered_available_blockers\":[{\"visible_ordinal\":1}]") ||
                        visibleAssignedBlockers.Contains("GameCard"))
                    {
                        return 85;
                    }
                    string assignedSha = LowerSha256FixtureV1(assignedBytes);
                    string finishModelCommitment = new string('7', 64);
                    string finishStepCommitment = BlockerStepCommitmentFixtureV1(
                        assignedSha,
                        0,
                        "f",
                        "-",
                        "-",
                        finishModelCommitment);
                    viewModel.GameFixture.ExpectedAction = doneButton.ActionFixture;
                    if (!DispatchBlockerStepFixtureV1(
                            channelName,
                            view,
                            assignedSha,
                            0,
                            "f",
                            "-",
                            "-",
                            finishModelCommitment,
                            finishStepCommitment,
                            true) ||
                        viewModel.GameFixture.ExecutionCount != 3 ||
                        !DispatchBlockerStepFixtureV1(
                            channelName,
                            view,
                            assignedSha,
                            0,
                            "f",
                            "-",
                            "-",
                            finishModelCommitment,
                            finishStepCommitment,
                            false) || viewModel.GameFixture.ExecutionCount != 3)
                    {
                        return 88;
                    }
                    firstBlocker.VisualBlockingOrderItems.Clear();
                    firstBlocker.VisuallyBlockingFixture = false;
                    viewModel.InteractionStateFixture.SourceFixture = null;
                    opponent.Battlefield.Remove(secondVisibleAttacker);
                    viewModel.GameFixture.ResetExecutionFixture();

                    viewModel.Prompt.Buttons.Remove(doneButton);
                    viewModel.Prompt.DoneButtonFixture = null;
                    viewModel.Prompt.ShowOkPromptButtonFixture = true;
                    viewModel.Prompt.Buttons.Insert(
                        0,
                        viewModel.Prompt.OkPromptButtonFixture);
                    seated.Battlefield.Clear();
                    opponent.Battlefield.Clear();
                    seated.IsActiveFixture = true;
                    opponent.IsActiveFixture = false;
                    viewModel.CurrentPhaseFixture = GamePhase.Upkeep;

                    // The first nonempty-stack slice is intentionally narrow:
                    // one or more face-up, non-copy spells controlled by the
                    // seated player, with no rendered association targets.
                    // Collection order is exported directly and the last item
                    // remains the client-rendered top of stack.
                    viewModel.CurrentPhaseFixture = GamePhase.Upkeep;
                    var lowerStackSpell = new DuelSceneCardViewModel
                    {
                        NameFixture = "fixture-visible-lower-stack-spell"
                    };
                    var topStackSpell = new DuelSceneCardViewModel
                    {
                        NameFixture = "fixture-visible-top-stack-spell"
                    };
                    viewModel.Stack.CardItems.Add(lowerStackSpell);
                    viewModel.Stack.CardItems.Add(topStackSpell);
                    int stackStatus =
                        VisibleDuelProducerV1.ExportVisibleDecisionOrAbstainV1(channelName);
                    int stackLength = view.ReadInt32(0);
                    byte[] stackBytes = new byte[stackLength];
                    view.ReadArray(8, stackBytes, 0, stackBytes.Length);
                    string visibleStack = Encoding.UTF8.GetString(stackBytes);
                    int lowerIndex = visibleStack.IndexOf(
                        "fixture-visible-lower-stack-spell",
                        StringComparison.Ordinal);
                    int topIndex = visibleStack.IndexOf(
                        "fixture-visible-top-stack-spell",
                        StringComparison.Ordinal);
                    if (stackStatus != 0 || lowerIndex < 0 || topIndex <= lowerIndex ||
                        !visibleStack.Contains("\"visible_stack_position\":0") ||
                        !visibleStack.Contains("\"visible_stack_position\":1") ||
                        !visibleStack.Contains("\"controller\":\"seated_player\"") ||
                        !visibleStack.Contains("\"visible_targets\":[]") ||
                        !visibleStack.Contains("\"item_kind\":\"spell\""))
                    {
                        return 65;
                    }
                    topStackSpell.AssociationItems.Add(new object());
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 66;
                    }
                    topStackSpell.AssociationItems.Clear();
                    topStackSpell.IsAbilityOnTheStackFixture = true;
                    topStackSpell.CardFrameIDFixture =
                        Shiny.Card.Enums.FrameStyle.AbilityOrEffect;
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 67;
                    }
                    topStackSpell.IsAbilityOnTheStackFixture = false;
                    topStackSpell.CardFrameIDFixture =
                        Shiny.Card.Enums.FrameStyle.Normal;
                    topStackSpell.IsControllerFixture = false;
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 68;
                    }
                    topStackSpell.IsControllerFixture = true;
                    topStackSpell.IsCloneFixture = true;
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 69;
                    }
                    topStackSpell.IsCloneFixture = false;
                    topStackSpell.IsFaceDownFixture = true;
                    topStackSpell.ThrowIfNameReadFixture = true;
                    if (!ExportsProjectionIncompleteV1(channelName, view))
                    {
                        return 70;
                    }
                    topStackSpell.ThrowIfNameReadFixture = false;
                    topStackSpell.IsFaceDownFixture = false;
                    viewModel.Stack.CardItems.Clear();

                    // Exercise the general noncombat priority slice with
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
                    viewModel.Stack.CardItems.Add(lowerStackSpell);

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
                        !observed.Contains("fixture-visible-lower-stack-spell") ||
                        !observed.Contains("\"item_kind\":\"spell\"") ||
                        !observed.Contains("\"visible_targets\":[]") ||
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
                        viewModel.Prompt.OkPromptButtonFixture.ActionFixture!;
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

        private static byte[] ExportVisibleBytesFixtureV1(
            string channelName,
            MemoryMappedViewAccessor view)
        {
            int status = VisibleDuelProducerV1.ExportVisibleDecisionOrAbstainV1(channelName);
            int length = view.ReadInt32(0);
            if (status != 0 || length <= 0 || length > Capacity - 8)
            {
                return Array.Empty<byte>();
            }
            var bytes = new byte[length];
            view.ReadArray(8, bytes, 0, bytes.Length);
            return bytes;
        }

        private static bool DispatchAttackerStepFixtureV1(
            string channelName,
            MemoryMappedViewAccessor view,
            string currentSelectionSha256,
            int candidateCount,
            string desiredMaskHex,
            string planCommitmentSha256,
            bool expectSubmitted)
        {
            byte[] command = Encoding.ASCII.GetBytes(
                "execute_visible_attacker_step_v1|" + currentSelectionSha256 + "|" +
                candidateCount.ToString() + "|" + desiredMaskHex + "|" +
                planCommitmentSha256);
            view.Write(0, command.Length);
            view.Write(4, 3);
            view.WriteArray(8, command, 0, command.Length);
            view.Flush();
            int status = VisibleDuelProducerV1.DispatchVisibleAttackerStepV1(channelName);
            int length = view.ReadInt32(0);
            if (status != 0 || length <= 0 || length > Capacity - 8)
            {
                return false;
            }
            var receipt = new byte[length];
            view.ReadArray(8, receipt, 0, receipt.Length);
            string expected = expectSubmitted
                ? "{\"result_kind\":\"action_dispatch_receipt\",\"status\":\"submitted\"}"
                : "{\"result_kind\":\"action_dispatch_receipt\",\"status\":\"rejected\"}";
            return string.Equals(Encoding.UTF8.GetString(receipt), expected, StringComparison.Ordinal);
        }

        private static bool DispatchBlockerStepFixtureV1(
            string channelName,
            MemoryMappedViewAccessor view,
            string currentSelectionSha256,
            int selectedIndex,
            string operationKind,
            string blockerOrdinal,
            string attackerOrdinal,
            string modelSelectionCommitmentSha256,
            string stepCommitmentSha256,
            bool expectSubmitted)
        {
            byte[] command = Encoding.ASCII.GetBytes(
                "execute_visible_blocker_step_v1|" + currentSelectionSha256 + "|" +
                selectedIndex.ToString() + "|" + operationKind + "|" +
                blockerOrdinal + "|" + attackerOrdinal + "|" +
                modelSelectionCommitmentSha256 + "|" + stepCommitmentSha256);
            view.Write(0, command.Length);
            view.Write(4, 4);
            view.WriteArray(8, command, 0, command.Length);
            view.Flush();
            int status = VisibleDuelProducerV1.DispatchVisibleBlockerStepV1(channelName);
            int length = view.ReadInt32(0);
            if (status != 0 || length <= 0 || length > Capacity - 8)
            {
                return false;
            }
            var receipt = new byte[length];
            view.ReadArray(8, receipt, 0, receipt.Length);
            string expected = expectSubmitted
                ? "{\"result_kind\":\"action_dispatch_receipt\",\"status\":\"submitted\"}"
                : "{\"result_kind\":\"action_dispatch_receipt\",\"status\":\"rejected\"}";
            return string.Equals(Encoding.UTF8.GetString(receipt), expected, StringComparison.Ordinal);
        }

        private static bool DispatchSingleBlockerStepFixtureV1(
            string channelName,
            MemoryMappedViewAccessor view,
            string currentSelectionSha256,
            int candidateCount,
            string desiredMaskHex,
            string planCommitmentSha256,
            bool expectSubmitted)
        {
            byte[] command = Encoding.ASCII.GetBytes(
                "execute_visible_single_blocker_step_v1|" + currentSelectionSha256 + "|" +
                candidateCount.ToString() + "|" + desiredMaskHex + "|" +
                planCommitmentSha256);
            view.Write(0, command.Length);
            view.Write(4, 5);
            view.WriteArray(8, command, 0, command.Length);
            view.Flush();
            int status = VisibleDuelProducerV1.DispatchVisibleSingleBlockerStepV1(
                channelName);
            int length = view.ReadInt32(0);
            if (status != 0 || length <= 0 || length > Capacity - 8)
            {
                return false;
            }
            var receipt = new byte[length];
            view.ReadArray(8, receipt, 0, receipt.Length);
            string expected = expectSubmitted
                ? "{\"result_kind\":\"action_dispatch_receipt\",\"status\":\"submitted\"}"
                : "{\"result_kind\":\"action_dispatch_receipt\",\"status\":\"rejected\"}";
            return string.Equals(
                Encoding.UTF8.GetString(receipt),
                expected,
                StringComparison.Ordinal);
        }

        private static string LowerSha256FixtureV1(byte[] bytes)
        {
            using (SHA256 sha256 = SHA256.Create())
            {
                return string.Concat(sha256.ComputeHash(bytes).Select(
                    value => value.ToString("x2")));
            }
        }

        private static byte[] ReadPayloadFixtureV1(MemoryMappedViewAccessor view)
        {
            int length = view.ReadInt32(0);
            if (length <= 0 || length > Capacity - 8)
            {
                return Array.Empty<byte>();
            }
            var payload = new byte[length];
            view.ReadArray(8, payload, 0, payload.Length);
            return payload;
        }

        private static string AttackerPlanCommitmentFixtureV1(
            string sourceSelectionSha256,
            int candidateCount,
            string desiredMaskHex)
        {
            var committed = new System.Collections.Generic.List<byte>();
            committed.AddRange(Encoding.ASCII.GetBytes(
                "mtgo-visible-attacker-execution-plan-v1"));
            foreach (string part in new[]
            {
                sourceSelectionSha256,
                candidateCount.ToString(),
                desiredMaskHex
            })
            {
                byte[] bytes = Encoding.ASCII.GetBytes(part);
                ulong count = (ulong)bytes.Length;
                var length = new byte[8];
                for (int index = 7; index >= 0; index--)
                {
                    length[index] = (byte)(count & 0xff);
                    count >>= 8;
                }
                committed.AddRange(length);
                committed.AddRange(bytes);
            }
            return LowerSha256FixtureV1(committed.ToArray());
        }

        private static string SingleBlockerPlanCommitmentFixtureV1(
            string sourceSelectionSha256,
            int candidateCount,
            string desiredMaskHex)
        {
            var committed = new System.Collections.Generic.List<byte>();
            committed.AddRange(Encoding.ASCII.GetBytes(
                "mtgo-visible-single-attacker-blocker-execution-plan-v1"));
            foreach (string part in new[]
            {
                sourceSelectionSha256,
                candidateCount.ToString(),
                desiredMaskHex
            })
            {
                byte[] bytes = Encoding.ASCII.GetBytes(part);
                ulong count = (ulong)bytes.Length;
                var length = new byte[8];
                for (int index = 7; index >= 0; index--)
                {
                    length[index] = (byte)(count & 0xff);
                    count >>= 8;
                }
                committed.AddRange(length);
                committed.AddRange(bytes);
            }
            return LowerSha256FixtureV1(committed.ToArray());
        }

        private static string BlockerStepCommitmentFixtureV1(
            string sourceSelectionSha256,
            int selectedIndex,
            string operationKind,
            string blockerOrdinal,
            string attackerOrdinal,
            string modelSelectionCommitmentSha256)
        {
            var committed = new System.Collections.Generic.List<byte>();
            committed.AddRange(Encoding.ASCII.GetBytes(
                "mtgo-visible-multi-attacker-blocker-execution-step-v1"));
            foreach (string part in new[]
            {
                sourceSelectionSha256,
                selectedIndex.ToString(),
                operationKind,
                blockerOrdinal,
                attackerOrdinal,
                modelSelectionCommitmentSha256
            })
            {
                byte[] bytes = Encoding.ASCII.GetBytes(part);
                ulong count = (ulong)bytes.Length;
                var length = new byte[8];
                for (int index = 7; index >= 0; index--)
                {
                    length[index] = (byte)(count & 0xff);
                    count >>= 8;
                }
                committed.AddRange(length);
                committed.AddRange(bytes);
            }
            return LowerSha256FixtureV1(committed.ToArray());
        }

        private static bool ExportsVisiblePhaseV1(
            string channelName,
            MemoryMappedViewAccessor view,
            string expectedPhase)
        {
            int status = VisibleDuelProducerV1.ExportVisibleDecisionOrAbstainV1(channelName);
            int length = view.ReadInt32(0);
            if (status != 0 || length <= 0 || length > Capacity - 8)
            {
                return false;
            }
            byte[] bytes = new byte[length];
            view.ReadArray(8, bytes, 0, bytes.Length);
            string observed = Encoding.UTF8.GetString(bytes);
            return observed.StartsWith(
                    "{\"result_kind\":\"visible_decision\",\"decision\":",
                    StringComparison.Ordinal) &&
                observed.Contains("\"phase\":\"" + expectedPhase + "\"");
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
