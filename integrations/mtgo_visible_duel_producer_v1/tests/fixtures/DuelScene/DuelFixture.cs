using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Windows.Controls;
using Shiny.Card.ViewModels;
using WotC.MtGO.Client.Model.Play;

namespace WotC.MtGO.Client.Model.Play
{
    public enum GamePhase
    {
        Invalid = 0,
        PreCombatMain = 4
    }

    public enum Counter
    {
        PlusOnePlusOne
    }
}

namespace Shiny.Play.Duel
{
    public sealed class DuelScene : UserControl
    {
    }

    public sealed class GroupCardAction : ICardAction
    {
        public string Name
        {
            get
            {
                VisibleActionMenuGetterProbeV1.Record("Group.Name");
                return "fixture-visible-group-action";
            }
        }
        public string GroupName
        {
            get
            {
                VisibleActionMenuGetterProbeV1.Record("Group.GroupName");
                return "fixture-visible-group";
            }
        }
        public string[] ModeOptions
        {
            get
            {
                VisibleActionMenuGetterProbeV1.Record("Group.ModeOptions");
                return new string[0];
            }
        }
        public ActionType ActionType => ActionType.CardAction;
        public bool IsDefault => false;
        public bool CanBePerformedLocally => true;
        public bool IsManaAbility => false;
        public bool IsActivatedAbility => true;
        public bool IsCastAction => false;
    }

    public static class VisibleActionMenuGetterProbeV1
    {
        private static readonly HashSet<string> Calls = new HashSet<string>();

        public static void Record(string name)
        {
            Calls.Add(name);
        }

        public static bool SawEveryVisibleActionMenuGetterV1()
        {
            return Calls.SetEquals(new[]
            {
                "Group.Name",
                "Group.GroupName",
                "Group.ModeOptions"
            });
        }
    }
}

namespace Shiny.Play.Duel.ViewModel
{
    public static class VisibleGetterProbeV1
    {
        private static readonly HashSet<string> Calls = new HashSet<string>();

        public static void Record(string name)
        {
            Calls.Add(name);
        }

        public static bool SawEveryVisibleChromeGetterV1()
        {
            string[] expected =
            {
                "Duel.CurrentPhase",
                "Duel.GameTurnText",
                "Duel.Players",
                "Duel.PromptBox",
                "Player.LocalPlayer",
                "Player.Active",
                "Player.MatActive",
                "Player.Health",
                "Player.HandTotal",
                "Player.DeckTotal",
                "Player.ManaPoolItems",
                "Mana.ColorString",
                "Mana.Count",
                "Prompt.IsPromptBoxActive",
                "Prompt.Text",
                "Prompt.StandardButtons",
                "Button.Name",
                "Button.Enabled",
                "Button.Visible"
            };
            foreach (string name in expected)
            {
                if (!Calls.Contains(name))
                {
                    return false;
                }
            }
            return Calls.Count == expected.Length;
        }
    }

    public static class VisibleZoneGetterProbeV1
    {
        private static readonly HashSet<string> Calls = new HashSet<string>();

        public static void Record(string name)
        {
            Calls.Add(name);
        }

        public static bool SawEveryVisibleZoneAndCardGetterV1()
        {
            string[] expected =
            {
                "Duel.StackZone",
                "Player.BattlefieldCards",
                "Player.HandZone",
                "Player.LibraryZone",
                "Player.GraveyardZone",
                "Player.ExileZone",
                "Player.RevealedZone",
                "Player.ShieldsZone",
                "Zone.IsVisible",
                "Zone.Count",
                "Zone.Cards",
                "DuelCard.CardAttachedTo",
                "DuelCard.IsToken",
                "DuelCard.VisibleCounters",
                "Counter.Quantity",
                "Counter.Type"
            };
            foreach (string name in expected)
            {
                if (!Calls.Contains(name))
                {
                    return false;
                }
            }
            return Calls.Count == expected.Length;
        }
    }

    public sealed class DuelSceneViewModel
    {
        public WotC.MtGO.Client.Model.Play.GamePhase CurrentPhase
        {
            get
            {
                VisibleGetterProbeV1.Record("Duel.CurrentPhase");
                return WotC.MtGO.Client.Model.Play.GamePhase.PreCombatMain;
            }
        }

        public string GameTurnText
        {
            get
            {
                VisibleGetterProbeV1.Record("Duel.GameTurnText");
                return "fixture-turn";
            }
        }

        public ObservableCollection<PlayerViewModel> Players
        {
            get
            {
                VisibleGetterProbeV1.Record("Duel.Players");
                return PlayerItems;
            }
        }

        public PromptBoxViewModel PromptBox
        {
            get
            {
                VisibleGetterProbeV1.Record("Duel.PromptBox");
                return Prompt;
            }
        }

        public ZoneViewModel StackZone
        {
            get
            {
                VisibleZoneGetterProbeV1.Record("Duel.StackZone");
                return Stack;
            }
        }

        public IGame Game
        {
            get
            {
                VisibleActionJoinGetterProbeV1.Record("Duel.Game");
                return GameFixture;
            }
        }

        public ObservableCollection<PlayerViewModel> PlayerItems { get; } =
            new ObservableCollection<PlayerViewModel>();
        public PromptBoxViewModel Prompt { get; } = new PromptBoxViewModel();
        public ZoneViewModel Stack { get; } = new ZoneViewModel
        {
            IsVisibleFixture = true
        };
        public VisibleFixtureGame GameFixture { get; } = new VisibleFixtureGame();
    }

    public sealed class PlayerViewModel
    {
        public bool LocalPlayer
        {
            get
            {
                VisibleGetterProbeV1.Record("Player.LocalPlayer");
                return IsLocalFixture;
            }
        }

        public bool Active
        {
            get
            {
                VisibleGetterProbeV1.Record("Player.Active");
                return IsActiveFixture;
            }
        }

        public bool MatActive
        {
            get
            {
                VisibleGetterProbeV1.Record("Player.MatActive");
                return IsPriorityFixture;
            }
        }

        public int Health
        {
            get
            {
                VisibleGetterProbeV1.Record("Player.Health");
                return 20;
            }
        }

        public int HandTotal
        {
            get
            {
                VisibleGetterProbeV1.Record("Player.HandTotal");
                return HandTotalFixture;
            }
        }

        public int DeckTotal
        {
            get
            {
                VisibleGetterProbeV1.Record("Player.DeckTotal");
                return DeckTotalFixture;
            }
        }

        public ObservableCollection<ManaPoolItemViewModel> ManaPoolItems
        {
            get
            {
                VisibleGetterProbeV1.Record("Player.ManaPoolItems");
                return ManaItems;
            }
        }

        public string Name { get; set; } = "fixture-player";
        public ZoneViewModel HandZone
        {
            get
            {
                VisibleZoneGetterProbeV1.Record("Player.HandZone");
                return Hand;
            }
        }
        public ZoneViewModel LibraryZone
        {
            get
            {
                VisibleZoneGetterProbeV1.Record("Player.LibraryZone");
                return Library;
            }
        }
        public ZoneViewModel GraveyardZone
        {
            get
            {
                VisibleZoneGetterProbeV1.Record("Player.GraveyardZone");
                return Graveyard;
            }
        }
        public ZoneViewModel ExileZone
        {
            get
            {
                VisibleZoneGetterProbeV1.Record("Player.ExileZone");
                return Exile;
            }
        }
        public ZoneViewModel RevealedZone
        {
            get
            {
                VisibleZoneGetterProbeV1.Record("Player.RevealedZone");
                return Revealed;
            }
        }
        public ZoneViewModel ShieldsZone
        {
            get
            {
                VisibleZoneGetterProbeV1.Record("Player.ShieldsZone");
                return Shields;
            }
        }
        public ObservableCollection<DuelSceneCardViewModel> BattlefieldCards
        {
            get
            {
                VisibleZoneGetterProbeV1.Record("Player.BattlefieldCards");
                return Battlefield;
            }
        }
        public ObservableCollection<ManaPoolItemViewModel> ManaItems { get; } =
            new ObservableCollection<ManaPoolItemViewModel>();
        public bool IsLocalFixture { get; set; }
        public bool IsActiveFixture { get; set; }
        public bool IsPriorityFixture { get; set; }
        public int HandTotalFixture { get; set; }
        public int DeckTotalFixture { get; set; } = 53;
        public ZoneViewModel Hand { get; } = new ZoneViewModel();
        public ZoneViewModel Library { get; } = new ZoneViewModel();
        public ZoneViewModel Graveyard { get; } = new ZoneViewModel
        {
            IsVisibleFixture = true
        };
        public ZoneViewModel Exile { get; } = new ZoneViewModel
        {
            IsVisibleFixture = true
        };
        public ZoneViewModel Revealed { get; } = new ZoneViewModel();
        public ZoneViewModel Shields { get; } = new ZoneViewModel
        {
            IsVisibleFixture = true
        };
        public ObservableCollection<DuelSceneCardViewModel> Battlefield { get; } =
            new ObservableCollection<DuelSceneCardViewModel>();
    }

    public sealed class ManaPoolItemViewModel
    {
        public string ColorString
        {
            get
            {
                VisibleGetterProbeV1.Record("Mana.ColorString");
                return "fixture-color";
            }
        }

        public int Count
        {
            get
            {
                VisibleGetterProbeV1.Record("Mana.Count");
                return CountFixture;
            }
        }

        public WotC.MtGO.Client.Model.MagicColors Color
        {
            get
            {
                VisibleActionJoinGetterProbeV1.Record("Mana.Color");
                return WotC.MtGO.Client.Model.MagicColors.White;
            }
        }

        public int CountFixture { get; set; }
    }

    public sealed class PromptBoxViewModel
    {
        public PromptBoxViewModel()
        {
            OkPromptButtonFixture = new OptionButton
            {
                NameFixture = "OK",
                VisibleFixture = true,
                EnabledFixture = true,
                ActionFixture = new VisibleFixtureCardAction
                {
                    IsDefaultFixture = true
                }
            };
        }

        public bool IsPromptBoxActive
        {
            get
            {
                VisibleGetterProbeV1.Record("Prompt.IsPromptBoxActive");
                return false;
            }
        }

        public string Text
        {
            get
            {
                VisibleGetterProbeV1.Record("Prompt.Text");
                return "fixture-prompt";
            }
        }

        public ObservableCollection<OptionButton> StandardButtons
        {
            get
            {
                VisibleGetterProbeV1.Record("Prompt.StandardButtons");
                return Buttons;
            }
        }

        public ObservableCollection<OptionButton> Buttons { get; } =
            new ObservableCollection<OptionButton>();

        public OptionButton OkPromptButton
        {
            get
            {
                VisibleActionJoinGetterProbeV1.Record("Prompt.OkPromptButton");
                return OkPromptButtonFixture;
            }
        }

        public OptionButton? DoneButton
        {
            get
            {
                VisibleActionJoinGetterProbeV1.Record("Prompt.DoneButton");
                return null;
            }
        }

        private OptionButton OkPromptButtonFixture { get; }
    }

    public sealed class OptionButton
    {
        public string Name
        {
            get
            {
                VisibleGetterProbeV1.Record("Button.Name");
                return NameFixture;
            }
        }

        public bool Enabled
        {
            get
            {
                VisibleGetterProbeV1.Record("Button.Enabled");
                return EnabledFixture;
            }
        }

        public bool Visible
        {
            get
            {
                VisibleGetterProbeV1.Record("Button.Visible");
                VisibleActionJoinGetterProbeV1.Record("Button.VisibleRead");
                return VisibleFixture;
            }
        }

        public IGameAction? Action
        {
            get
            {
                VisibleActionJoinGetterProbeV1.Record("Button.Action");
                VisibleActionJoinGetterProbeV1.Record("Button.ActionRead");
                return ActionFixture;
            }
        }

        public IGameAction? ActionFixture { get; set; }
        public string NameFixture { get; set; } = "fixture-button";
        public bool EnabledFixture { get; set; } = true;
        public bool VisibleFixture { get; set; }
    }

    public static class VisibleActionJoinGetterProbeV1
    {
        private static readonly HashSet<string> Calls = new HashSet<string>();

        public static void Record(string name)
        {
            Calls.Add(name);
        }

        public static bool SawEveryPrivateJoinRootV1()
        {
            string[] required =
            {
                "Duel.Game",
                "DuelCard.Actions",
                "Mana.Color",
                "Prompt.DoneButton",
                "Prompt.OkPromptButton"
            };
            foreach (string name in required)
            {
                if (!Calls.Contains(name))
                {
                    return false;
                }
            }
            return true;
        }
    }

    public sealed class ZoneViewModel
    {
        public bool IsVisible
        {
            get
            {
                if (ThrowIfAnyGetterFixture)
                {
                    throw new System.InvalidOperationException("hidden library was inspected");
                }
                VisibleZoneGetterProbeV1.Record("Zone.IsVisible");
                return IsVisibleFixture;
            }
        }
        public int Count
        {
            get
            {
                if (ThrowIfAnyGetterFixture)
                {
                    throw new System.InvalidOperationException("hidden library was inspected");
                }
                VisibleZoneGetterProbeV1.Record("Zone.Count");
                return CardItems.Count;
            }
        }
        public ObservableCollection<DuelSceneCardViewModel> Cards
        {
            get
            {
                if (ThrowIfAnyGetterFixture || ThrowIfCardsReadFixture)
                {
                    throw new System.InvalidOperationException("hidden zone cards were inspected");
                }
                VisibleZoneGetterProbeV1.Record("Zone.Cards");
                return CardItems;
            }
        }
        public bool IsVisibleFixture { get; set; }
        public bool ThrowIfAnyGetterFixture { get; set; }
        public bool ThrowIfCardsReadFixture { get; set; }
        public ObservableCollection<DuelSceneCardViewModel> CardItems { get; } =
            new ObservableCollection<DuelSceneCardViewModel>();
    }

    public sealed class DuelSceneCardViewModel : CardViewModel
    {
        public DuelSceneCardViewModel? CardAttachedTo
        {
            get
            {
                VisibleZoneGetterProbeV1.Record("DuelCard.CardAttachedTo");
                return CardAttachedToFixture;
            }
        }
        public bool IsToken
        {
            get
            {
                VisibleZoneGetterProbeV1.Record("DuelCard.IsToken");
                return IsTokenFixture;
            }
        }
        public IList<CardCounterViewModel> VisibleCounters
        {
            get
            {
                VisibleZoneGetterProbeV1.Record("DuelCard.VisibleCounters");
                return CounterItems;
            }
        }
        public IEnumerable<IGameAction> Actions
        {
            get
            {
                if (ThrowIfActionsReadFixture)
                {
                    throw new System.InvalidOperationException(
                        "opponent visible-card action collection was inspected");
                }
                VisibleActionJoinGetterProbeV1.Record("DuelCard.Actions");
                return ActionItems;
            }
        }
        public DuelSceneCardViewModel? CardAttachedToFixture { get; set; }
        public bool IsTokenFixture { get; set; }
        public IList<CardCounterViewModel> CounterItems { get; } =
            new List<CardCounterViewModel>();
        public bool ThrowIfActionsReadFixture { get; set; }
        public IList<IGameAction> ActionItems { get; } = new List<IGameAction>();
    }

    public sealed class CardCounterViewModel
    {
        public int Quantity
        {
            get
            {
                VisibleZoneGetterProbeV1.Record("Counter.Quantity");
                return QuantityFixture;
            }
        }
        public WotC.MtGO.Client.Model.Play.Counter Type
        {
            get
            {
                VisibleZoneGetterProbeV1.Record("Counter.Type");
                return TypeFixture;
            }
        }
        public int QuantityFixture { get; set; }
        public WotC.MtGO.Client.Model.Play.Counter TypeFixture { get; set; }
    }
}
