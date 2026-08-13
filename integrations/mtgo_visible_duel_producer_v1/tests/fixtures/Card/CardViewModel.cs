namespace Shiny.Card.ViewModels
{
    using System;
    using System.Collections.Generic;

    public static class VisibleCardGetterProbeV1
    {
        private static readonly HashSet<string> Calls = new HashSet<string>();

        public static void Record(string name)
        {
            Calls.Add(name);
        }

        public static bool SawEveryVisibleCardGetterV1()
        {
            string[] expected =
            {
                "Card.CardFrameID",
                "Card.CurrentDungeonRoom",
                "Card.CurrentDamage",
                "Card.IsAttacking",
                "Card.IsBlocking",
                "Card.IsFaceDown",
                "Card.IsTapped",
                "Card.Name",
                "Card.Power",
                "Card.Toughness"
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

    public class CardViewModel
    {
        public Shiny.Card.Enums.FrameStyle CardFrameID
        {
            get
            {
                VisibleCardGetterProbeV1.Record("Card.CardFrameID");
                return CardFrameIDFixture;
            }
        }

        public int CurrentDamage
        {
            get
            {
                VisibleCardGetterProbeV1.Record("Card.CurrentDamage");
                return CurrentDamageFixture;
            }
        }

        public int CurrentDungeonRoom
        {
            get
            {
                VisibleCardGetterProbeV1.Record("Card.CurrentDungeonRoom");
                return CurrentDungeonRoomFixture;
            }
        }

        public bool IsAttacking
        {
            get
            {
                VisibleCardGetterProbeV1.Record("Card.IsAttacking");
                return IsAttackingFixture;
            }
        }

        public bool IsBlocking
        {
            get
            {
                VisibleCardGetterProbeV1.Record("Card.IsBlocking");
                return IsBlockingFixture;
            }
        }

        public bool IsFaceDown
        {
            get
            {
                VisibleCardGetterProbeV1.Record("Card.IsFaceDown");
                return IsFaceDownFixture;
            }
        }

        public bool IsTapped
        {
            get
            {
                VisibleCardGetterProbeV1.Record("Card.IsTapped");
                return IsTappedFixture;
            }
        }

        public string Name
        {
            get
            {
                VisibleCardGetterProbeV1.Record("Card.Name");
                if (ThrowIfNameReadFixture)
                {
                    throw new InvalidOperationException("hidden fixture card name was read");
                }
                return NameFixture;
            }
        }

        public int? Power
        {
            get
            {
                VisibleCardGetterProbeV1.Record("Card.Power");
                return PowerFixture;
            }
        }

        public int? Toughness
        {
            get
            {
                VisibleCardGetterProbeV1.Record("Card.Toughness");
                return ToughnessFixture;
            }
        }

        public int CurrentDamageFixture { get; set; }
        public int CurrentDungeonRoomFixture { get; set; } = -1;
        public Shiny.Card.Enums.FrameStyle CardFrameIDFixture { get; set; }
        public bool IsAttackingFixture { get; set; }
        public bool IsBlockingFixture { get; set; }
        public bool IsFaceDownFixture { get; set; }
        public bool IsTappedFixture { get; set; }
        public bool ThrowIfNameReadFixture { get; set; }
        public string NameFixture { get; set; } = "fixture-card";
        public int? PowerFixture { get; set; }
        public int? ToughnessFixture { get; set; }
    }
}

namespace Shiny.Card.Enums
{
    public enum FrameStyle
    {
        Normal = 0,
        CLBInitiativeEmblem = 1,
        OtherVisibleEmblem = 2
    }
}
