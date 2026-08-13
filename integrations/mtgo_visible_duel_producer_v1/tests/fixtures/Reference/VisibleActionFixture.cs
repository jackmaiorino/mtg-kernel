namespace WotC.MtGO.Client.Model.Play
{
    public enum ActionType
    {
        Invalid = 0,
        ChooseOption = 1,
        CardAction = 2
    }

    public interface IGameAction
    {
        string Name { get; }
        ActionType ActionType { get; }
        bool IsDefault { get; }
    }

    public interface IGame
    {
        void ExecuteAction(IGameAction action);
    }

    public sealed class VisibleFixtureGame : IGame
    {
        public IGameAction? ExpectedAction { get; set; }
        public bool ExecutionObserved { get; private set; }
        public int ExecutionCount { get; private set; }

        public void ExecuteAction(IGameAction action)
        {
            ExecutionCount++;
            if (!object.ReferenceEquals(action, ExpectedAction))
            {
                throw new System.InvalidOperationException(
                    "unexpected private visible action binding");
            }
            ExecutionObserved = true;
        }
    }

    public interface ICardAction : IGameAction
    {
        string ActionChoices { get; }
        bool AltMenuAction { get; }
        int AttackVictimId { get; }
        bool CanBePerformedLocally { get; }
        string GroupName { get; }
        bool IsManaAbility { get; }
        bool IsActivatedAbility { get; }
        bool IsCastAction { get; }
        bool IsSubmenuItem { get; }
        string ModeChoiceMapping { get; }
        string[] ModeOptions { get; }
    }

    public static class PrivateVisibleActionGetterProbeV1
    {
        private static readonly System.Collections.Generic.HashSet<string> Calls =
            new System.Collections.Generic.HashSet<string>();

        public static void Record(string name)
        {
            Calls.Add(name);
        }

        public static bool SawEveryPrivateVisibleActionGetterV1()
        {
            string[] expected =
            {
                "Action.Name",
                "Action.ActionType",
                "Action.IsDefault",
                "CardAction.ActionChoices",
                "CardAction.AltMenuAction",
                "CardAction.AttackVictimId",
                "CardAction.CanBePerformedLocally",
                "CardAction.GroupName",
                "CardAction.IsManaAbility",
                "CardAction.IsActivatedAbility",
                "CardAction.IsCastAction",
                "CardAction.IsSubmenuItem",
                "CardAction.ModeChoiceMapping",
                "CardAction.ModeOptions"
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

    public sealed class VisibleFixtureCardAction : ICardAction
    {
        public string Name
        {
            get
            {
                PrivateVisibleActionGetterProbeV1.Record("Action.Name");
                return NameFixture;
            }
        }

        public ActionType ActionType
        {
            get
            {
                PrivateVisibleActionGetterProbeV1.Record("Action.ActionType");
                return ActionType.CardAction;
            }
        }

        public bool IsDefault
        {
            get
            {
                PrivateVisibleActionGetterProbeV1.Record("Action.IsDefault");
                return IsDefaultFixture;
            }
        }

        public bool CanBePerformedLocally
        {
            get
            {
                PrivateVisibleActionGetterProbeV1.Record(
                    "CardAction.CanBePerformedLocally");
                return true;
            }
        }

        public string ActionChoices
        {
            get
            {
                PrivateVisibleActionGetterProbeV1.Record("CardAction.ActionChoices");
                return ActionChoicesFixture;
            }
        }

        public bool AltMenuAction
        {
            get
            {
                PrivateVisibleActionGetterProbeV1.Record("CardAction.AltMenuAction");
                return AltMenuActionFixture;
            }
        }

        public int AttackVictimId
        {
            get
            {
                PrivateVisibleActionGetterProbeV1.Record("CardAction.AttackVictimId");
                return AttackVictimIdFixture;
            }
        }

        public string GroupName
        {
            get
            {
                PrivateVisibleActionGetterProbeV1.Record("CardAction.GroupName");
                return GroupNameFixture;
            }
        }

        public bool IsManaAbility
        {
            get
            {
                PrivateVisibleActionGetterProbeV1.Record("CardAction.IsManaAbility");
                return ManaFixture;
            }
        }

        public bool IsActivatedAbility
        {
            get
            {
                PrivateVisibleActionGetterProbeV1.Record(
                    "CardAction.IsActivatedAbility");
                return ActivatedFixture;
            }
        }

        public bool IsCastAction
        {
            get
            {
                PrivateVisibleActionGetterProbeV1.Record("CardAction.IsCastAction");
                return CastFixture;
            }
        }

        public bool IsSubmenuItem
        {
            get
            {
                PrivateVisibleActionGetterProbeV1.Record("CardAction.IsSubmenuItem");
                return IsSubmenuItemFixture;
            }
        }

        public string ModeChoiceMapping
        {
            get
            {
                PrivateVisibleActionGetterProbeV1.Record("CardAction.ModeChoiceMapping");
                return ModeChoiceMappingFixture;
            }
        }

        public string[] ModeOptions
        {
            get
            {
                PrivateVisibleActionGetterProbeV1.Record("CardAction.ModeOptions");
                return new string[0];
            }
        }

        public string NameFixture { get; set; } = "fixture-visible-action";
        public string ActionChoicesFixture { get; set; } = string.Empty;
        public bool AltMenuActionFixture { get; set; }
        public int AttackVictimIdFixture { get; set; } = -1;
        public string GroupNameFixture { get; set; } = string.Empty;
        public bool ManaFixture { get; set; }
        public bool ActivatedFixture { get; set; }
        public bool CastFixture { get; set; } = true;
        public bool IsSubmenuItemFixture { get; set; }
        public string ModeChoiceMappingFixture { get; set; } = string.Empty;
        public bool IsDefaultFixture { get; set; }
    }
}

namespace WotC.MtGO.Client.Model
{
    public enum MagicColors
    {
        Invalid = 0,
        White = 1,
        Blue = 2,
        Black = 4,
        Red = 8,
        Green = 16,
        Colorless = 32
    }
}
