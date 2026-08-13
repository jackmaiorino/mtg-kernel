using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Windows.Controls;
using Shiny.Card.ViewModels;

namespace WotC.MtGO.Client.Model.Play
{
    public enum GamePhase
    {
        Main
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

    public sealed class GroupCardAction
    {
        public string Name { get; set; } = "fixture-action";
        public string GroupName { get; set; } = "fixture-group";
        public string[] ModeOptions { get; set; } = new string[0];
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

    public sealed class DuelSceneViewModel
    {
        public WotC.MtGO.Client.Model.Play.GamePhase CurrentPhase
        {
            get
            {
                VisibleGetterProbeV1.Record("Duel.CurrentPhase");
                return WotC.MtGO.Client.Model.Play.GamePhase.Main;
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

        public ZoneViewModel StackZone { get; } = new ZoneViewModel();

        public ObservableCollection<PlayerViewModel> PlayerItems { get; } =
            new ObservableCollection<PlayerViewModel>();
        public PromptBoxViewModel Prompt { get; } = new PromptBoxViewModel();
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
                return 7;
            }
        }

        public int DeckTotal
        {
            get
            {
                VisibleGetterProbeV1.Record("Player.DeckTotal");
                return 53;
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
        public ZoneViewModel HandZone { get; } = new ZoneViewModel();
        public ZoneViewModel LibraryZone { get; } = new ZoneViewModel();
        public ZoneViewModel GraveyardZone { get; } = new ZoneViewModel();
        public ZoneViewModel ExileZone { get; } = new ZoneViewModel();
        public ZoneViewModel RevealedZone { get; } = new ZoneViewModel();
        public ObservableCollection<DuelSceneCardViewModel> BattlefieldCards { get; } =
            new ObservableCollection<DuelSceneCardViewModel>();
        public ObservableCollection<ManaPoolItemViewModel> ManaItems { get; } =
            new ObservableCollection<ManaPoolItemViewModel>();
        public bool IsLocalFixture { get; set; }
        public bool IsActiveFixture { get; set; }
        public bool IsPriorityFixture { get; set; }
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
                return 1;
            }
        }
    }

    public sealed class PromptBoxViewModel
    {
        public bool IsPromptBoxActive
        {
            get
            {
                VisibleGetterProbeV1.Record("Prompt.IsPromptBoxActive");
                return true;
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
    }

    public sealed class OptionButton
    {
        public string Name
        {
            get
            {
                VisibleGetterProbeV1.Record("Button.Name");
                return "fixture-button";
            }
        }

        public bool Enabled
        {
            get
            {
                VisibleGetterProbeV1.Record("Button.Enabled");
                return true;
            }
        }

        public bool Visible
        {
            get
            {
                VisibleGetterProbeV1.Record("Button.Visible");
                return true;
            }
        }
    }

    public sealed class ZoneViewModel
    {
        public bool IsVisible { get; set; }
        public int Count { get; set; }
        public ObservableCollection<DuelSceneCardViewModel> Cards { get; } =
            new ObservableCollection<DuelSceneCardViewModel>();
    }

    public sealed class DuelSceneCardViewModel : CardViewModel
    {
        public DuelSceneCardViewModel? CardAttachedTo { get; set; }
        public bool IsToken { get; set; }
        public IList<CardCounterViewModel> VisibleCounters { get; } =
            new List<CardCounterViewModel>();
    }

    public sealed class CardCounterViewModel
    {
        public int Quantity { get; set; }
        public WotC.MtGO.Client.Model.Play.Counter Type { get; set; }
    }
}
