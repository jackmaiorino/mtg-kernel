//! Mechanical facets: the closed vocabulary that the meaning tables emit.
//!
//! Every facet names a rules-level fact (a zone, a player relation, an event
//! kind), never a card, an EffectOp variant or a human label. Cards share a
//! feature exactly when their programs do the same thing in these terms.

use serde::Serialize;

use crate::card_def::CardType;

/// Zones a program moves objects between or reads from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum ZoneF {
    Library,
    Hand,
    Battlefield,
    Graveyard,
    Stack,
    Exile,
    Command,
}

impl From<crate::state::Zone> for ZoneF {
    fn from(zone: crate::state::Zone) -> Self {
        use crate::state::Zone;
        match zone {
            Zone::Library => ZoneF::Library,
            Zone::Hand => ZoneF::Hand,
            Zone::Battlefield => ZoneF::Battlefield,
            Zone::Graveyard => ZoneF::Graveyard,
            Zone::Stack => ZoneF::Stack,
            Zone::Exile => ZoneF::Exile,
            Zone::Command => ZoneF::Command,
        }
    }
}

/// A player relative to the ability's controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum RelF {
    /// The ability's controller.
    You,
    /// An opponent, fixed by the rules (no choice).
    Opponent,
    /// A player chosen as a target; legality is in the governing target facts.
    ChosenPlayer,
    /// Every player.
    EachPlayer,
    /// Every opponent.
    EachOpponent,
    /// The controller of an object the program refers to.
    ObjectController,
    /// The owner of an object the program refers to.
    ObjectOwner,
    /// The player bound by the triggering event.
    EventPlayer,
}

/// The kind of object a program acts on or reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum ObjF {
    /// The ability's own source object.
    ThisObject,
    /// Any card or object, unfiltered.
    AnyCard,
    Typed(CardTypeF),
    /// A permanent of any type.
    Permanent,
    /// A nonland permanent.
    NonlandPermanent,
    /// A spell on the stack.
    Spell,
    /// A token.
    Token,
    /// The object the source is attached to (Aura or Equipment host).
    AttachedObject,
    /// The object bound by the triggering event.
    EventObject,
    /// One specific card definition (searching for a card by identity).
    SpecificCard,
    /// A basic land card.
    BasicLand,
    /// An activated or triggered ability on the stack.
    Ability,
    /// A player as the recipient (damage, life, cards).
    Player,
    /// A player or a permanent ("any target").
    PlayerOrPermanent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum CardTypeF {
    Land,
    Creature,
    Instant,
    Sorcery,
    Artifact,
    Enchantment,
    Planeswalker,
}

impl From<CardType> for CardTypeF {
    fn from(card_type: CardType) -> Self {
        match card_type {
            CardType::Land => CardTypeF::Land,
            CardType::Creature => CardTypeF::Creature,
            CardType::Instant => CardTypeF::Instant,
            CardType::Sorcery => CardTypeF::Sorcery,
            CardType::Artifact => CardTypeF::Artifact,
            CardType::Enchantment => CardTypeF::Enchantment,
            CardType::Planeswalker => CardTypeF::Planeswalker,
        }
    }
}

impl From<CardType> for ObjF {
    fn from(card_type: CardType) -> Self {
        ObjF::Typed(card_type.into())
    }
}

/// What happens to the game. Zone changes are always `Move` with explicit
/// from/to zones, so "mill", "discard", "destroy", "bounce" and "reanimate"
/// share structure instead of being separate tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum EvF {
    /// An object changes zone (`from` and `to` set).
    Move,
    /// Damage is dealt.
    Damage,
    LifeGain,
    LifeLoss,
    /// Mana is added to a pool.
    AddMana,
    Tap,
    Untap,
    /// Power/toughness changes for a duration.
    StatChange,
    /// A counter is put on an object (sign carried by `amount`).
    PlaceCounter,
    /// A counter is removed from an object.
    RemoveCounter,
    /// A keyword ability is granted for a duration.
    GrantKeyword,
    /// A token is created (its own facts are recorded as a nested token).
    CreateToken,
    /// A copy of a spell or object is made.
    Copy,
    /// Cards are revealed to all players.
    Reveal,
    /// Cards are looked at privately.
    Look,
    /// A library is shuffled.
    Shuffle,
    /// Library cards are reordered.
    Reorder,
    /// Two objects deal damage to each other.
    Fight,
    /// Damage is prevented.
    PreventDamage,
    /// Control of an object changes.
    GainControl,
    /// A permanent cannot untap, attack, block or similar for a duration.
    Restrict,
    /// Permission to play a card from a non-hand zone is granted.
    PlayPermission,
    /// The cost of spells changes.
    CostChange,
    /// The player takes the initiative.
    TakeInitiative,
    /// The player becomes the monarch.
    BecomeMonarch,
    /// The player ventures into a dungeon room.
    VentureRoom,
    /// The object transforms to another face.
    Transform,
    /// The object becomes attached to another.
    Attach,
    /// The object's power/toughness is set or its characteristics change.
    SetCharacteristic,
    /// A player chooses a color, mode or similar value.
    ChooseValue,
    /// Counters on an object are doubled.
    DoubleCounters,
    /// The game is won or lost directly.
    GameOutcome,
    /// A player may pay mana during resolution ("unless that player pays").
    PayMana,
    /// A spell is countered (moved from the stack without resolving).
    CounterSpell,
}

/// How large an effect is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum AmtF {
    /// No quantity (a single object or an unquantified event).
    Unit,
    /// A fixed number, bucketed so 2 and 3 are near and 1 and 20 are not.
    Fixed(u8),
    /// Paid as X.
    X,
    /// Read from the game state; the read itself is emitted as a `ReadAtom`.
    Dynamic,
    /// Every matching object.
    All,
    /// Repeats until a card of the given type is found.
    UntilType(CardTypeF),
    /// Half of something, rounded.
    Half,
    /// A negative fixed number (a -N/-N change), bucketed like `Fixed`.
    Minus(u8),
}

impl AmtF {
    /// Log-scale bucket: 0, 1, 2, 3, 4-5, 6-9, 10+.
    /// A power/toughness change: the larger-magnitude side, with its sign,
    /// so +2/+2 and -2/-2 differ.
    pub fn stat(power: i64, toughness: i64) -> AmtF {
        AmtF::fixed(if power.abs() >= toughness.abs() {
            power
        } else {
            toughness
        })
    }

    pub fn fixed(n: i64) -> AmtF {
        let negative = n < 0;
        let n = n.unsigned_abs();
        let bucket = match n {
            0 => 0,
            1 => 1,
            2 => 2,
            3 => 3,
            4..=5 => 4,
            6..=9 => 6,
            _ => 10,
        };
        if negative {
            AmtF::Minus(bucket)
        } else {
            AmtF::Fixed(bucket)
        }
    }
}

/// How long a continuous effect lasts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum DurF {
    Instant,
    EndOfTurn,
    UntilYourNextTurn,
    WhileOnBattlefield,
    Permanent,
}

/// One thing that happens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct EffectAtom {
    pub ev: EvF,
    pub from: Option<ZoneF>,
    pub to: Option<ZoneF>,
    /// Whose objects or which player the event applies to.
    pub player: Option<RelF>,
    pub obj: Option<ObjF>,
    pub amount: AmtF,
    pub duration: DurF,
    /// The mana color added (`AddMana`).
    pub color: Option<ColorF>,
    /// The keyword granted, as its `Keywords` bit index (`GrantKeyword`).
    pub keyword: Option<u8>,
}

/// Bit indices of the keywords set in `keywords`.
pub fn keyword_bits(keywords: crate::card_def::Keywords) -> Vec<u8> {
    (0..32u8)
        .filter(|bit| keywords.0 & (1 << bit) != 0)
        .collect()
}

impl EffectAtom {
    pub fn new(ev: EvF) -> Self {
        EffectAtom {
            ev,
            from: None,
            to: None,
            player: None,
            obj: None,
            amount: AmtF::Unit,
            duration: DurF::Instant,
            color: None,
            keyword: None,
        }
    }
    pub fn moving(from: Option<ZoneF>, to: ZoneF) -> Self {
        EffectAtom {
            from,
            to: Some(to),
            ..EffectAtom::new(EvF::Move)
        }
    }
    pub fn player(mut self, player: RelF) -> Self {
        self.player = Some(player);
        self
    }
    pub fn maybe_player(mut self, player: Option<RelF>) -> Self {
        self.player = player;
        self
    }
    pub fn obj(mut self, obj: ObjF) -> Self {
        self.obj = Some(obj);
        self
    }
    pub fn amount(mut self, amount: AmtF) -> Self {
        self.amount = amount;
        self
    }
    pub fn duration(mut self, duration: DurF) -> Self {
        self.duration = duration;
        self
    }
    pub fn color(mut self, color: ColorF) -> Self {
        self.color = Some(color);
        self
    }
    pub fn keyword(mut self, bit: u8) -> Self {
        self.keyword = Some(bit);
        self
    }
}

/// How a read aggregates what it finds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum AggF {
    /// The number of matching objects.
    Count,
    /// Whether any matching object exists.
    Any,
    /// Whether at least k exist (k bucketed like `AmtF::fixed`).
    AtLeast(u8),
    /// A numeric characteristic (power, mana value) of an object.
    Characteristic,
    /// Whether an event happened this turn.
    EventThisTurn,
}

/// A quantity or condition the program reads from the game state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct ReadAtom {
    pub player: RelF,
    pub zone: Option<ZoneF>,
    pub obj: Option<ObjF>,
    pub agg: AggF,
}

/// Which players and objects an ability may target, as legality facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum TargetAtom {
    /// The ability's controller is a legal player target.
    PlayerYou,
    /// An opponent is a legal player target.
    PlayerOpponent,
    /// An object of this class, controlled by this relation, in this zone.
    Object {
        obj: ObjF,
        controller: Option<RelF>,
        zone: ZoneF,
        /// A color the object must have, when the spec filters by color.
        color: Option<ColorF>,
        /// An upper bound on mana value (bucketed), when the spec has one.
        mana_value_at_most: Option<u8>,
        /// A card type the object must not have ("noncreature").
        excludes: Option<CardTypeF>,
    },
    /// The spec picks more than one target.
    MultipleTargets,
    /// Targets are optional ("up to").
    UpTo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum ColorF {
    White,
    Blue,
    Black,
    Red,
    Green,
    Colorless,
}

impl From<crate::mana::ManaColor> for ColorF {
    fn from(color: crate::mana::ManaColor) -> Self {
        use crate::mana::ManaColor;
        match color {
            ManaColor::W => ColorF::White,
            ManaColor::U => ColorF::Blue,
            ManaColor::B => ColorF::Black,
            ManaColor::R => ColorF::Red,
            ManaColor::G => ColorF::Green,
            ManaColor::C => ColorF::Colorless,
        }
    }
}

/// A cost component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum CostAtom {
    Mana,
    Tap,
    PayLife(u8),
    /// An object moves as part of paying (sacrifice, discard, exile).
    MoveObject {
        from: ZoneF,
        to: ZoneF,
        obj: ObjF,
        amount: AmtF,
    },
    /// The cost changes with game state (reductions, delve, affinity).
    Reduced,
    /// Removes counters from the source.
    RemoveCounters,
    /// Taps untapped creatures or artifacts you control.
    TapOthers,
}

/// The canonical event class a triggered ability waits for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum TrigF {
    /// The source enters the battlefield.
    SelfEnters,
    /// The source leaves the battlefield (to `to`, when known).
    SelfLeaves { to: Option<ZoneF> },
    /// Another object enters under your control.
    OtherEnters { obj: ObjF },
    /// A spell is cast.
    SpellCast { by: RelF, obj: ObjF },
    /// The source itself is cast.
    SelfCast,
    /// A card is drawn.
    Draw { by: RelF, nth: u8 },
    /// Damage is dealt by the source or its host.
    DealsDamage {
        combat: bool,
        to_player: bool,
        host: bool,
    },
    /// The source attacks.
    Attacks,
    /// A step begins.
    StepBegins { step: StepF, yours_only: bool },
    /// A permanent is sacrificed.
    Sacrifice { obj: ObjF },
    /// Life is gained.
    LifeGained { by: RelF },
    /// Counters are put on the source.
    CountersPlaced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum StepF {
    Upkeep,
    EndStep,
}

/// Program structure that matters for play: choices and conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum ControlF {
    /// The controller may choose not to do part of the program.
    Optional,
    /// Part of the program depends on a condition.
    Conditional,
    /// The controller chooses one of several branches.
    ChooseBranch,
    /// The controller chooses objects as the program resolves (not targets).
    ChooseObjects,
    /// Part of the program repeats.
    Repeat,
}

/// Where in a card an ability lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum CtxF {
    /// The resolution program of the card cast from hand.
    Spell,
    /// An alternative printed mode of the spell.
    Mode,
    /// A triggered ability.
    Trigger,
    /// An activated ability.
    Activated,
    /// A mana ability.
    Mana,
    /// A static ability.
    Static,
    /// A cast from a zone other than hand (flashback, escape, plot).
    AltZoneCast,
    /// An alternative spell form (adventure, omen, bestow).
    AltForm,
    /// A chapter of a saga.
    Chapter,
    /// A room or similar program granted by a game mechanic.
    Granted,
}

/// One emitted fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum Atom {
    Effect(EffectAtom),
    Read(ReadAtom),
    Target(TargetAtom),
    Cost(CostAtom),
    Trigger(TrigF),
    Control(ControlF),
    /// A permanent-spell resolution that only puts the card onto the
    /// battlefield. Carried as context, not as an effect part.
    PermanentResolution,
    /// The card can be cast from this zone (flashback, escape, plot).
    CastFrom(ZoneF),
    /// The ability is activated from this zone (when not the battlefield).
    ActivatedFrom(ZoneF),
    /// The trigger functions while its source is in this zone.
    TriggersFrom(ZoneF),
    /// Activation is limited to sorcery timing.
    SorcerySpeed,
    /// A rule the engine implements in code that no lookup returns as data.
    Opaque,
}

/// The ordered list of atoms one ability produced, with its context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AbilityFacts {
    pub ctx: CtxF,
    pub atoms: Vec<Atom>,
}

/// Collector the meaning tables write into.
#[derive(Debug, Default)]
pub struct Collector {
    pub atoms: Vec<Atom>,
    /// Programs that create tokens; the extractor recurses into them.
    pub created_tokens: Vec<u16>,
    /// Specific card definitions referenced by identity (not recursed).
    pub referenced_cards: Vec<u16>,
}

impl Collector {
    pub fn effect(&mut self, atom: EffectAtom) {
        self.atoms.push(Atom::Effect(atom));
    }
    pub fn read(&mut self, player: RelF, zone: Option<ZoneF>, obj: Option<ObjF>, agg: AggF) {
        self.atoms.push(Atom::Read(ReadAtom {
            player,
            zone,
            obj,
            agg,
        }));
    }
    pub fn target(&mut self, atom: TargetAtom) {
        self.atoms.push(Atom::Target(atom));
    }
    pub fn cost(&mut self, atom: CostAtom) {
        self.atoms.push(Atom::Cost(atom));
    }
    pub fn trigger(&mut self, trig: TrigF) {
        self.atoms.push(Atom::Trigger(trig));
    }
    pub fn control(&mut self, control: ControlF) {
        self.atoms.push(Atom::Control(control));
    }
    pub fn permanent_resolution(&mut self) {
        self.atoms.push(Atom::PermanentResolution);
    }
}
