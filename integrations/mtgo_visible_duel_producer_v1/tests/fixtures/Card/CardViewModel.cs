namespace Shiny.Card.ViewModels
{
    public class CardViewModel
    {
        public int CurrentDamage { get; set; }
        public bool IsAttacking { get; set; }
        public bool IsBlocking { get; set; }
        public bool IsFaceDown { get; set; }
        public bool IsTapped { get; set; }
        public string Name { get; set; } = "fixture-card";
        public int? Power { get; set; }
        public int? Toughness { get; set; }
    }
}
