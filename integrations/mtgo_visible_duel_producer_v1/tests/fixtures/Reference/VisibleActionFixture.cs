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
    }

    public interface ICardAction : IGameAction
    {
        bool CanBePerformedLocally { get; }
        bool IsManaAbility { get; }
        bool IsActivatedAbility { get; }
        bool IsCastAction { get; }
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
                "CardAction.CanBePerformedLocally",
                "CardAction.IsManaAbility",
                "CardAction.IsActivatedAbility",
                "CardAction.IsCastAction",
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
                return "fixture-visible-action";
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

        public bool CanBePerformedLocally
        {
            get
            {
                PrivateVisibleActionGetterProbeV1.Record(
                    "CardAction.CanBePerformedLocally");
                return true;
            }
        }

        public bool IsManaAbility
        {
            get
            {
                PrivateVisibleActionGetterProbeV1.Record("CardAction.IsManaAbility");
                return false;
            }
        }

        public bool IsActivatedAbility
        {
            get
            {
                PrivateVisibleActionGetterProbeV1.Record(
                    "CardAction.IsActivatedAbility");
                return false;
            }
        }

        public bool IsCastAction
        {
            get
            {
                PrivateVisibleActionGetterProbeV1.Record("CardAction.IsCastAction");
                return true;
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
    }
}
