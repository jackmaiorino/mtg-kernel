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
        uint ActionFlags { get; }
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

        public void ResetExecutionFixture()
        {
            ExpectedAction = null;
            ExecutionObserved = false;
            ExecutionCount = 0;
        }
    }

    public interface ICardAction : IGameAction
    {
        string ActionChoices { get; }
        bool AltMenuAction { get; }
        int AttackVictimId { get; }
        bool CanBePerformedLocally { get; }
        bool ConfirmBeforeTargetingOwnCard { get; }
        string? ConfirmModeString { get; }
        string GroupName { get; }
        bool HasXTarget { get; }
        bool InSideboard { get; }
        bool IsManaAbility { get; }
        bool IsActivatedAbility { get; }
        bool IsCastAction { get; }
        bool IsFakeAction { get; }
        bool IsSubmenuItem { get; }
        uint ModeMaxChoices { get; }
        uint ModeMinChoices { get; }
        string ModeChoiceMapping { get; }
        string[] ModeOptions { get; }
        System.Collections.Generic.IList<object> Targets { get; }
        bool XDeterminedByTargetWithGreatestCMC { get; }
        bool XIsAMinimum { get; }
        int XTargetDivisor { get; }
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
                "Action.ActionFlags",
                "Action.IsDefault",
                "CardAction.ActionChoices",
                "CardAction.AltMenuAction",
                "CardAction.AttackVictimId",
                "CardAction.CanBePerformedLocally",
                "CardAction.ConfirmBeforeTargetingOwnCard",
                "CardAction.ConfirmModeString",
                "CardAction.GroupName",
                "CardAction.HasXTarget",
                "CardAction.InSideboard",
                "CardAction.IsManaAbility",
                "CardAction.IsActivatedAbility",
                "CardAction.IsCastAction",
                "CardAction.IsFakeAction",
                "CardAction.IsSubmenuItem",
                "CardAction.ModeMaxChoices",
                "CardAction.ModeMinChoices",
                "CardAction.ModeChoiceMapping",
                "CardAction.ModeOptions",
                "CardAction.Targets",
                "CardAction.XDeterminedByTargetWithGreatestCMC",
                "CardAction.XIsAMinimum",
                "CardAction.XTargetDivisor"
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

        public uint ActionFlags
        {
            get
            {
                PrivateVisibleActionGetterProbeV1.Record("Action.ActionFlags");
                return ActionFlagsFixture;
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

        public bool ConfirmBeforeTargetingOwnCard
        {
            get
            {
                PrivateVisibleActionGetterProbeV1.Record(
                    "CardAction.ConfirmBeforeTargetingOwnCard");
                return ConfirmBeforeTargetingOwnCardFixture;
            }
        }

        public string? ConfirmModeString
        {
            get
            {
                PrivateVisibleActionGetterProbeV1.Record("CardAction.ConfirmModeString");
                return ConfirmModeStringFixture;
            }
        }

        public bool HasXTarget
        {
            get
            {
                PrivateVisibleActionGetterProbeV1.Record("CardAction.HasXTarget");
                return HasXTargetFixture;
            }
        }

        public bool InSideboard
        {
            get
            {
                PrivateVisibleActionGetterProbeV1.Record("CardAction.InSideboard");
                return InSideboardFixture;
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

        public bool IsFakeAction
        {
            get
            {
                PrivateVisibleActionGetterProbeV1.Record("CardAction.IsFakeAction");
                return IsFakeActionFixture;
            }
        }

        public uint ModeMaxChoices
        {
            get
            {
                PrivateVisibleActionGetterProbeV1.Record("CardAction.ModeMaxChoices");
                return ModeMaxChoicesFixture;
            }
        }

        public uint ModeMinChoices
        {
            get
            {
                PrivateVisibleActionGetterProbeV1.Record("CardAction.ModeMinChoices");
                return ModeMinChoicesFixture;
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

        public System.Collections.Generic.IList<object> Targets
        {
            get
            {
                PrivateVisibleActionGetterProbeV1.Record("CardAction.Targets");
                return TargetItems;
            }
        }

        public bool XDeterminedByTargetWithGreatestCMC
        {
            get
            {
                PrivateVisibleActionGetterProbeV1.Record(
                    "CardAction.XDeterminedByTargetWithGreatestCMC");
                return XDeterminedByTargetWithGreatestCMCFixture;
            }
        }

        public bool XIsAMinimum
        {
            get
            {
                PrivateVisibleActionGetterProbeV1.Record("CardAction.XIsAMinimum");
                return XIsAMinimumFixture;
            }
        }

        public int XTargetDivisor
        {
            get
            {
                PrivateVisibleActionGetterProbeV1.Record("CardAction.XTargetDivisor");
                return XTargetDivisorFixture;
            }
        }

        public string NameFixture { get; set; } = "fixture-visible-action";
        public string ActionChoicesFixture { get; set; } = string.Empty;
        public bool AltMenuActionFixture { get; set; }
        public int AttackVictimIdFixture { get; set; } = -1;
        public bool ConfirmBeforeTargetingOwnCardFixture { get; set; }
        public string? ConfirmModeStringFixture { get; set; }
        public string GroupNameFixture { get; set; } = string.Empty;
        public bool HasXTargetFixture { get; set; }
        public bool InSideboardFixture { get; set; }
        public bool ManaFixture { get; set; }
        public bool ActivatedFixture { get; set; }
        public bool CastFixture { get; set; } = true;
        public bool IsFakeActionFixture { get; set; }
        public bool IsSubmenuItemFixture { get; set; }
        public uint ModeMaxChoicesFixture { get; set; }
        public uint ModeMinChoicesFixture { get; set; }
        public string ModeChoiceMappingFixture { get; set; } = string.Empty;
        public bool IsDefaultFixture { get; set; }
        public uint ActionFlagsFixture { get; set; }
        public System.Collections.Generic.IList<object> TargetItems { get; } =
            new System.Collections.Generic.List<object>();
        public bool XDeterminedByTargetWithGreatestCMCFixture { get; set; }
        public bool XIsAMinimumFixture { get; set; }
        public int XTargetDivisorFixture { get; set; } = 1;
    }

    public sealed class VisibleFixturePromptAction : IGameAction
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
                return ActionType.ChooseOption;
            }
        }

        public uint ActionFlags
        {
            get
            {
                PrivateVisibleActionGetterProbeV1.Record("Action.ActionFlags");
                return ActionFlagsFixture;
            }
        }

        public bool IsDefault
        {
            get
            {
                PrivateVisibleActionGetterProbeV1.Record("Action.IsDefault");
                return true;
            }
        }

        public uint ActionFlagsFixture { get; set; } = 1u;
        public string NameFixture { get; set; } = "OK";
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
