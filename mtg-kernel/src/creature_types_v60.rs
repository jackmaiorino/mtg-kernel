//! Pinned legal creature types and exact effective-creature matching.
//!
//! This module grants no card, action, model, or snapshot-owner permissions.
//! Native Subtype discriminants and its historical pool remain untouched.

use crate::card_def::{CardType, Subtype, CARD_DEFS};
use crate::ids::ObjectId;
use crate::state::GameState;
use serde::{Deserialize, Serialize};

pub const DOMAIN_SCHEMA_V60: &str = "creature-type-domain-v60";
pub const DOMAIN_SHA256_V60: &str =
    "dff3a69ecf6c53e739cceddb8aa8cc6f8b334e00566d01636863d9ed8ad0cb9c";
pub const DOMAIN_INPUT_V60: &str = include_str!("../../data/rules/creature_types_v60.json");

macro_rules! creature_types {
    ($($variant:ident = $id:literal => $name:literal,)*) => {
        /// Complete CR 205.3m domain pinned to the 2026-09-25 rules.
        /// Explicit indices are independent of visible or hidden card pools.
        #[repr(u16)]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        pub enum CreatureTypeV60 {
            $(#[serde(rename = $name)] $variant = $id,)*
        }
        impl CreatureTypeV60 {
            pub const ALL: &'static [Self] = &[$(Self::$variant,)*];
            pub const fn choice_index(self) -> u16 { self as u16 }
            pub const fn from_choice_index(index: u16) -> Option<Self> {
                match index { $($id => Some(Self::$variant),)* _ => None }
            }
            pub const fn canonical_name(self) -> &'static str {
                match self { $(Self::$variant => $name,)* }
            }
            pub fn from_canonical_name(name: &str) -> Option<Self> {
                match name { $($name => Some(Self::$variant),)* _ => None }
            }
        }
    }
}

creature_types! {
    Advisor = 0 => "Advisor",
    Aetherborn = 1 => "Aetherborn",
    Alien = 2 => "Alien",
    Ally = 3 => "Ally",
    Angel = 4 => "Angel",
    Antelope = 5 => "Antelope",
    Ape = 6 => "Ape",
    Archer = 7 => "Archer",
    Archon = 8 => "Archon",
    Armadillo = 9 => "Armadillo",
    Army = 10 => "Army",
    Artificer = 11 => "Artificer",
    Assassin = 12 => "Assassin",
    AssemblyWorker = 13 => "Assembly-Worker",
    Astartes = 14 => "Astartes",
    Atog = 15 => "Atog",
    Aurochs = 16 => "Aurochs",
    Avatar = 17 => "Avatar",
    Azra = 18 => "Azra",
    Badger = 19 => "Badger",
    Balloon = 20 => "Balloon",
    Barbarian = 21 => "Barbarian",
    Bard = 22 => "Bard",
    Basilisk = 23 => "Basilisk",
    Bat = 24 => "Bat",
    Bear = 25 => "Bear",
    Beast = 26 => "Beast",
    Beaver = 27 => "Beaver",
    Beeble = 28 => "Beeble",
    Beholder = 29 => "Beholder",
    Berserker = 30 => "Berserker",
    Bird = 31 => "Bird",
    Bison = 32 => "Bison",
    Blinkmoth = 33 => "Blinkmoth",
    Boar = 34 => "Boar",
    Bringer = 35 => "Bringer",
    Brushwagg = 36 => "Brushwagg",
    Camarid = 37 => "Camarid",
    Camel = 38 => "Camel",
    Capybara = 39 => "Capybara",
    Caribou = 40 => "Caribou",
    Carrier = 41 => "Carrier",
    Cat = 42 => "Cat",
    Centaur = 43 => "Centaur",
    Child = 44 => "Child",
    Chimera = 45 => "Chimera",
    Citizen = 46 => "Citizen",
    Cleric = 47 => "Cleric",
    Clown = 48 => "Clown",
    Cockatrice = 49 => "Cockatrice",
    Construct = 50 => "Construct",
    Coward = 51 => "Coward",
    Coyote = 52 => "Coyote",
    Crab = 53 => "Crab",
    Crocodile = 54 => "Crocodile",
    Ctan = 55 => "C’tan",
    Custodes = 56 => "Custodes",
    Cyberman = 57 => "Cyberman",
    Cyclops = 58 => "Cyclops",
    Dalek = 59 => "Dalek",
    Dauthi = 60 => "Dauthi",
    Demigod = 61 => "Demigod",
    Demon = 62 => "Demon",
    Deserter = 63 => "Deserter",
    Detective = 64 => "Detective",
    Devil = 65 => "Devil",
    Dinosaur = 66 => "Dinosaur",
    Djinn = 67 => "Djinn",
    Doctor = 68 => "Doctor",
    Dog = 69 => "Dog",
    Dragon = 70 => "Dragon",
    Drake = 71 => "Drake",
    Dreadnought = 72 => "Dreadnought",
    Drix = 73 => "Drix",
    Drone = 74 => "Drone",
    Druid = 75 => "Druid",
    Dryad = 76 => "Dryad",
    Dwarf = 77 => "Dwarf",
    Echidna = 78 => "Echidna",
    Efreet = 79 => "Efreet",
    Egg = 80 => "Egg",
    Elder = 81 => "Elder",
    Eldrazi = 82 => "Eldrazi",
    Elemental = 83 => "Elemental",
    Elephant = 84 => "Elephant",
    Elf = 85 => "Elf",
    Elk = 86 => "Elk",
    Employee = 87 => "Employee",
    Eternal = 88 => "Eternal",
    Eye = 89 => "Eye",
    Faerie = 90 => "Faerie",
    Ferret = 91 => "Ferret",
    Fish = 92 => "Fish",
    Flagbearer = 93 => "Flagbearer",
    Fox = 94 => "Fox",
    Fractal = 95 => "Fractal",
    Frog = 96 => "Frog",
    Fungus = 97 => "Fungus",
    Gamer = 98 => "Gamer",
    Gamma = 99 => "Gamma",
    Gargoyle = 100 => "Gargoyle",
    Germ = 101 => "Germ",
    Giant = 102 => "Giant",
    Giraffe = 103 => "Giraffe",
    Gith = 104 => "Gith",
    Glimmer = 105 => "Glimmer",
    Gnoll = 106 => "Gnoll",
    Gnome = 107 => "Gnome",
    Goat = 108 => "Goat",
    Goblin = 109 => "Goblin",
    God = 110 => "God",
    Golem = 111 => "Golem",
    Gorgon = 112 => "Gorgon",
    Graveborn = 113 => "Graveborn",
    Gremlin = 114 => "Gremlin",
    Griffin = 115 => "Griffin",
    Guest = 116 => "Guest",
    Hag = 117 => "Hag",
    Halfling = 118 => "Halfling",
    Hamster = 119 => "Hamster",
    Harpy = 120 => "Harpy",
    Hedgehog = 121 => "Hedgehog",
    Hellion = 122 => "Hellion",
    Hero = 123 => "Hero",
    Hippo = 124 => "Hippo",
    Hippogriff = 125 => "Hippogriff",
    Homarid = 126 => "Homarid",
    Homunculus = 127 => "Homunculus",
    Horror = 128 => "Horror",
    Horse = 129 => "Horse",
    Human = 130 => "Human",
    Hydra = 131 => "Hydra",
    Hyena = 132 => "Hyena",
    Illusion = 133 => "Illusion",
    Imp = 134 => "Imp",
    Incarnation = 135 => "Incarnation",
    Inhuman = 136 => "Inhuman",
    Inkling = 137 => "Inkling",
    Inquisitor = 138 => "Inquisitor",
    Insect = 139 => "Insect",
    Jackal = 140 => "Jackal",
    Jellyfish = 141 => "Jellyfish",
    Juggernaut = 142 => "Juggernaut",
    Kangaroo = 143 => "Kangaroo",
    Kavu = 144 => "Kavu",
    Kirin = 145 => "Kirin",
    Kithkin = 146 => "Kithkin",
    Knight = 147 => "Knight",
    Kobold = 148 => "Kobold",
    Kor = 149 => "Kor",
    Kraken = 150 => "Kraken",
    Kree = 151 => "Kree",
    Llama = 152 => "Llama",
    Lamia = 153 => "Lamia",
    Lammasu = 154 => "Lammasu",
    Leech = 155 => "Leech",
    Lemur = 156 => "Lemur",
    Leviathan = 157 => "Leviathan",
    Lhurgoyf = 158 => "Lhurgoyf",
    Licid = 159 => "Licid",
    Lizard = 160 => "Lizard",
    Lobster = 161 => "Lobster",
    Manticore = 162 => "Manticore",
    Masticore = 163 => "Masticore",
    Mercenary = 164 => "Mercenary",
    Merfolk = 165 => "Merfolk",
    Metathran = 166 => "Metathran",
    Minion = 167 => "Minion",
    Minotaur = 168 => "Minotaur",
    Mite = 169 => "Mite",
    Mole = 170 => "Mole",
    Monger = 171 => "Monger",
    Mongoose = 172 => "Mongoose",
    Monk = 173 => "Monk",
    Monkey = 174 => "Monkey",
    Moogle = 175 => "Moogle",
    Moonfolk = 176 => "Moonfolk",
    Mount = 177 => "Mount",
    Mouse = 178 => "Mouse",
    Mutant = 179 => "Mutant",
    Myr = 180 => "Myr",
    Mystic = 181 => "Mystic",
    Nautilus = 182 => "Nautilus",
    Necron = 183 => "Necron",
    Nephilim = 184 => "Nephilim",
    Nightmare = 185 => "Nightmare",
    Nightstalker = 186 => "Nightstalker",
    Ninja = 187 => "Ninja",
    Noble = 188 => "Noble",
    Noggle = 189 => "Noggle",
    Nomad = 190 => "Nomad",
    Nymph = 191 => "Nymph",
    Octopus = 192 => "Octopus",
    Ogre = 193 => "Ogre",
    Ooze = 194 => "Ooze",
    Orb = 195 => "Orb",
    Orc = 196 => "Orc",
    Orgg = 197 => "Orgg",
    Otter = 198 => "Otter",
    Ouphe = 199 => "Ouphe",
    Ox = 200 => "Ox",
    Oyster = 201 => "Oyster",
    Pangolin = 202 => "Pangolin",
    Peasant = 203 => "Peasant",
    Pegasus = 204 => "Pegasus",
    Pentavite = 205 => "Pentavite",
    Performer = 206 => "Performer",
    Pest = 207 => "Pest",
    Phelddagrif = 208 => "Phelddagrif",
    Phoenix = 209 => "Phoenix",
    Phyrexian = 210 => "Phyrexian",
    Pilot = 211 => "Pilot",
    Pincher = 212 => "Pincher",
    Pirate = 213 => "Pirate",
    Plant = 214 => "Plant",
    Platypus = 215 => "Platypus",
    Porcupine = 216 => "Porcupine",
    Possum = 217 => "Possum",
    Praetor = 218 => "Praetor",
    Primarch = 219 => "Primarch",
    Prism = 220 => "Prism",
    Processor = 221 => "Processor",
    Qu = 222 => "Qu",
    Rabbit = 223 => "Rabbit",
    Raccoon = 224 => "Raccoon",
    Ranger = 225 => "Ranger",
    Rat = 226 => "Rat",
    Rebel = 227 => "Rebel",
    Reflection = 228 => "Reflection",
    Rhino = 229 => "Rhino",
    Rigger = 230 => "Rigger",
    Robot = 231 => "Robot",
    Rogue = 232 => "Rogue",
    Sable = 233 => "Sable",
    Salamander = 234 => "Salamander",
    Samurai = 235 => "Samurai",
    Sand = 236 => "Sand",
    Saproling = 237 => "Saproling",
    Satyr = 238 => "Satyr",
    Scarecrow = 239 => "Scarecrow",
    Scientist = 240 => "Scientist",
    Scion = 241 => "Scion",
    Scorpion = 242 => "Scorpion",
    Scout = 243 => "Scout",
    Sculpture = 244 => "Sculpture",
    Seal = 245 => "Seal",
    Serf = 246 => "Serf",
    Serpent = 247 => "Serpent",
    Servo = 248 => "Servo",
    Shade = 249 => "Shade",
    Shaman = 250 => "Shaman",
    Shapeshifter = 251 => "Shapeshifter",
    Shark = 252 => "Shark",
    Sheep = 253 => "Sheep",
    Shiar = 254 => "Shi’ar",
    Siren = 255 => "Siren",
    Skeleton = 256 => "Skeleton",
    Skrull = 257 => "Skrull",
    Skunk = 258 => "Skunk",
    Slith = 259 => "Slith",
    Sliver = 260 => "Sliver",
    Sloth = 261 => "Sloth",
    Slug = 262 => "Slug",
    Snail = 263 => "Snail",
    Snake = 264 => "Snake",
    Soldier = 265 => "Soldier",
    Soltari = 266 => "Soltari",
    Sorcerer = 267 => "Sorcerer",
    Spawn = 268 => "Spawn",
    Specter = 269 => "Specter",
    Spellshaper = 270 => "Spellshaper",
    Sphinx = 271 => "Sphinx",
    Spider = 272 => "Spider",
    Spike = 273 => "Spike",
    Spirit = 274 => "Spirit",
    Splinter = 275 => "Splinter",
    Sponge = 276 => "Sponge",
    Spy = 277 => "Spy",
    Squid = 278 => "Squid",
    Squirrel = 279 => "Squirrel",
    Starfish = 280 => "Starfish",
    Surrakar = 281 => "Surrakar",
    Survivor = 282 => "Survivor",
    Symbiote = 283 => "Symbiote",
    Synth = 284 => "Synth",
    Tentacle = 285 => "Tentacle",
    Tetravite = 286 => "Tetravite",
    Thalakos = 287 => "Thalakos",
    Thopter = 288 => "Thopter",
    Thrull = 289 => "Thrull",
    Tiefling = 290 => "Tiefling",
    Toy = 291 => "Toy",
    Treefolk = 292 => "Treefolk",
    Trilobite = 293 => "Trilobite",
    Triskelavite = 294 => "Triskelavite",
    Troll = 295 => "Troll",
    Turtle = 296 => "Turtle",
    Tyranid = 297 => "Tyranid",
    Unicorn = 298 => "Unicorn",
    Utrom = 299 => "Utrom",
    Vampire = 300 => "Vampire",
    Varmint = 301 => "Varmint",
    Vedalken = 302 => "Vedalken",
    Villain = 303 => "Villain",
    Volver = 304 => "Volver",
    Wall = 305 => "Wall",
    Walrus = 306 => "Walrus",
    Warlock = 307 => "Warlock",
    Warrior = 308 => "Warrior",
    Weasel = 309 => "Weasel",
    Weird = 310 => "Weird",
    Werewolf = 311 => "Werewolf",
    Whale = 312 => "Whale",
    Wizard = 313 => "Wizard",
    Wolf = 314 => "Wolf",
    Wolverine = 315 => "Wolverine",
    Wombat = 316 => "Wombat",
    Worm = 317 => "Worm",
    Wraith = 318 => "Wraith",
    Wurm = 319 => "Wurm",
    Yeti = 320 => "Yeti",
    Zombie = 321 => "Zombie",
    Zubera = 322 => "Zubera",
    TimeLord = 323 => "Time Lord",
}

/// A public choice stores no game state. Its closed wire format binds the
/// pinned domain and rejects unknown fields, types, and identity changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ChoiceWireV60", into = "ChoiceWireV60")]
pub struct CreatureTypeChoiceV60(CreatureTypeV60);

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChoiceWireV60 {
    schema: String,
    domain_sha256: String,
    chosen: CreatureTypeV60,
}
impl CreatureTypeChoiceV60 {
    pub const fn new(chosen: CreatureTypeV60) -> Self {
        Self(chosen)
    }
    pub const fn chosen(self) -> CreatureTypeV60 {
        self.0
    }
}
impl From<CreatureTypeChoiceV60> for ChoiceWireV60 {
    fn from(value: CreatureTypeChoiceV60) -> Self {
        Self {
            schema: DOMAIN_SCHEMA_V60.into(),
            domain_sha256: DOMAIN_SHA256_V60.into(),
            chosen: value.0,
        }
    }
}
impl TryFrom<ChoiceWireV60> for CreatureTypeChoiceV60 {
    type Error = &'static str;
    fn try_from(value: ChoiceWireV60) -> Result<Self, Self::Error> {
        if value.schema != DOMAIN_SCHEMA_V60 || value.domain_sha256 != DOMAIN_SHA256_V60 {
            return Err("creature type domain identity differs");
        }
        Ok(Self(value.chosen))
    }
}

/// Exhaustive mapping includes native case aliases and noncreature subtypes.
/// Adding a native enum variant requires deliberate classification here.
pub const fn canonical_native_type(subtype: Subtype) -> Option<CreatureTypeV60> {
    match subtype {
        Subtype::Ape => Some(CreatureTypeV60::Ape),
        Subtype::Aura => None,
        Subtype::BirdAllCaps => Some(CreatureTypeV60::Bird),
        Subtype::Bird => Some(CreatureTypeV60::Bird),
        Subtype::Blood => None,
        Subtype::Cat => Some(CreatureTypeV60::Cat),
        Subtype::Detective => Some(CreatureTypeV60::Detective),
        Subtype::Dragon => Some(CreatureTypeV60::Dragon),
        Subtype::Drone => Some(CreatureTypeV60::Drone),
        Subtype::Druid => Some(CreatureTypeV60::Druid),
        Subtype::Eldrazi => Some(CreatureTypeV60::Eldrazi),
        Subtype::Elf => Some(CreatureTypeV60::Elf),
        Subtype::Equipment => None,
        Subtype::FaerieAllCaps => Some(CreatureTypeV60::Faerie),
        Subtype::Faerie => Some(CreatureTypeV60::Faerie),
        Subtype::Food => None,
        Subtype::Forest => None,
        Subtype::Gate => None,
        Subtype::Goblin => Some(CreatureTypeV60::Goblin),
        Subtype::HumanAllCaps => Some(CreatureTypeV60::Human),
        Subtype::Hero => Some(CreatureTypeV60::Hero),
        Subtype::Human => Some(CreatureTypeV60::Human),
        Subtype::Hydra => Some(CreatureTypeV60::Hydra),
        Subtype::Island => None,
        Subtype::Knight => Some(CreatureTypeV60::Knight),
        Subtype::Monk => Some(CreatureTypeV60::Monk),
        Subtype::Moonfolk => Some(CreatureTypeV60::Moonfolk),
        Subtype::Monkey => Some(CreatureTypeV60::Monkey),
        Subtype::Mountain => None,
        Subtype::Myr => Some(CreatureTypeV60::Myr),
        Subtype::NinjaAllCaps => Some(CreatureTypeV60::Ninja),
        Subtype::Ninja => Some(CreatureTypeV60::Ninja),
        Subtype::Ouphe => Some(CreatureTypeV60::Ouphe),
        Subtype::Pirate => Some(CreatureTypeV60::Pirate),
        Subtype::Plains => None,
        Subtype::RogueAllCaps => Some(CreatureTypeV60::Rogue),
        Subtype::Ranger => Some(CreatureTypeV60::Ranger),
        Subtype::Rat => Some(CreatureTypeV60::Rat),
        Subtype::Rogue => Some(CreatureTypeV60::Rogue),
        Subtype::Serpent => Some(CreatureTypeV60::Serpent),
        Subtype::Saga => None,
        Subtype::Samurai => Some(CreatureTypeV60::Samurai),
        Subtype::Shaman => Some(CreatureTypeV60::Shaman),
        Subtype::Shapeshifter => Some(CreatureTypeV60::Shapeshifter),
        Subtype::Soldier => Some(CreatureTypeV60::Soldier),
        Subtype::Spider => Some(CreatureTypeV60::Spider),
        Subtype::Spirit => Some(CreatureTypeV60::Spirit),
        Subtype::Swamp => None,
        Subtype::Toy => Some(CreatureTypeV60::Toy),
        Subtype::Treefolk => Some(CreatureTypeV60::Treefolk),
        Subtype::Vampire => Some(CreatureTypeV60::Vampire),
        Subtype::WizardAllCaps => Some(CreatureTypeV60::Wizard),
        Subtype::Warrior => Some(CreatureTypeV60::Warrior),
        Subtype::Wizard => Some(CreatureTypeV60::Wizard),
        Subtype::Zombie => Some(CreatureTypeV60::Zombie),
        Subtype::Illusion => Some(CreatureTypeV60::Illusion),
        Subtype::Dryad => Some(CreatureTypeV60::Dryad),
        Subtype::Plant => Some(CreatureTypeV60::Plant),
        Subtype::Wall => Some(CreatureTypeV60::Wall),
        Subtype::Troll => Some(CreatureTypeV60::Troll),
        Subtype::Elemental => Some(CreatureTypeV60::Elemental),
        Subtype::Map => None,
        Subtype::Treasure => None,
        Subtype::Giant => Some(CreatureTypeV60::Giant),
        Subtype::Spawn => Some(CreatureTypeV60::Spawn),
        Subtype::Phyrexian => Some(CreatureTypeV60::Phyrexian),
        Subtype::Horror => Some(CreatureTypeV60::Horror),
        Subtype::Nightmare => Some(CreatureTypeV60::Nightmare),
        Subtype::Clue => None,
        Subtype::Skeleton => Some(CreatureTypeV60::Skeleton),
        Subtype::Squirrel => Some(CreatureTypeV60::Squirrel),
        Subtype::Lesson => None,
        Subtype::Insect => Some(CreatureTypeV60::Insect),
        Subtype::Fish => Some(CreatureTypeV60::Fish),
        Subtype::Urzas => None,
        Subtype::Tower => None,
        Subtype::PowerPlant => None,
        Subtype::Mine => None,
        Subtype::Desert => None,
        Subtype::Angel => Some(CreatureTypeV60::Angel),
        Subtype::Noble => Some(CreatureTypeV60::Noble),
        Subtype::Unicorn => Some(CreatureTypeV60::Unicorn),
        Subtype::Ajani => None,
        Subtype::Beast => Some(CreatureTypeV60::Beast),
        Subtype::Cleric => Some(CreatureTypeV60::Cleric),
        Subtype::Homunculus => Some(CreatureTypeV60::Homunculus),
        Subtype::Merfolk => Some(CreatureTypeV60::Merfolk),
        Subtype::Octopus => Some(CreatureTypeV60::Octopus),
        Subtype::Hyena => Some(CreatureTypeV60::Hyena),
        Subtype::Raccoon => Some(CreatureTypeV60::Raccoon),
        Subtype::Citizen => Some(CreatureTypeV60::Citizen),
        Subtype::Turtle => Some(CreatureTypeV60::Turtle),
        Subtype::Gremlin => Some(CreatureTypeV60::Gremlin),
        Subtype::Dinosaur => Some(CreatureTypeV60::Dinosaur),
        Subtype::Warlock => Some(CreatureTypeV60::Warlock),
        Subtype::Archer => Some(CreatureTypeV60::Archer),
        Subtype::Lizard => Some(CreatureTypeV60::Lizard),
        Subtype::Golem => Some(CreatureTypeV60::Golem),
        Subtype::Boar => Some(CreatureTypeV60::Boar),
        Subtype::Cyclops => Some(CreatureTypeV60::Cyclops),
        Subtype::Shark => Some(CreatureTypeV60::Shark),
        Subtype::Elk => Some(CreatureTypeV60::Elk),
        Subtype::Demon => Some(CreatureTypeV60::Demon),
        Subtype::Dwarf => Some(CreatureTypeV60::Dwarf),
        Subtype::Berserker => Some(CreatureTypeV60::Berserker),
        Subtype::Scout => Some(CreatureTypeV60::Scout),
        Subtype::Bat => Some(CreatureTypeV60::Bat),
        Subtype::Mercenary => Some(CreatureTypeV60::Mercenary),
        Subtype::Assassin => Some(CreatureTypeV60::Assassin),
        Subtype::Wolf => Some(CreatureTypeV60::Wolf),
        Subtype::Kraken => Some(CreatureTypeV60::Kraken),
        Subtype::Djinn => Some(CreatureTypeV60::Djinn),
        Subtype::Mouse => Some(CreatureTypeV60::Mouse),
        Subtype::Incubator => None,
        Subtype::Werewolf => Some(CreatureTypeV60::Werewolf),
        Subtype::Rabbit => Some(CreatureTypeV60::Rabbit),
        Subtype::Avatar => Some(CreatureTypeV60::Avatar),
        Subtype::Glimmer => Some(CreatureTypeV60::Glimmer),
        Subtype::Sheep => Some(CreatureTypeV60::Sheep),
        Subtype::Otter => Some(CreatureTypeV60::Otter),
        Subtype::AssemblyWorker => Some(CreatureTypeV60::AssemblyWorker),
        Subtype::Mite => Some(CreatureTypeV60::Mite),
        Subtype::Sphere => None,
        Subtype::Town => None,
        Subtype::Elephant => Some(CreatureTypeV60::Elephant),
        Subtype::Construct => Some(CreatureTypeV60::Construct),
        Subtype::Dog => Some(CreatureTypeV60::Dog),
        Subtype::Elder => Some(CreatureTypeV60::Elder),
        Subtype::Teferi => None,
        Subtype::God => Some(CreatureTypeV60::God),
        Subtype::Room => None,
        Subtype::Case => None,
        Subtype::Class => None,
        Subtype::Liliana => None,
        Subtype::Gnome => Some(CreatureTypeV60::Gnome),
        Subtype::Chandra => None,
        Subtype::Vehicle => None,
        Subtype::Pilot => Some(CreatureTypeV60::Pilot),
        Subtype::Siren => Some(CreatureTypeV60::Siren),
        Subtype::Praetor => Some(CreatureTypeV60::Praetor),
        Subtype::Ox => Some(CreatureTypeV60::Ox),
        Subtype::Llama => Some(CreatureTypeV60::Llama),
        Subtype::Role => None,
        Subtype::Kaito => None,
        Subtype::Wurm => Some(CreatureTypeV60::Wurm),
        Subtype::Artificer => Some(CreatureTypeV60::Artificer),
        Subtype::Orc => Some(CreatureTypeV60::Orc),
        Subtype::Juggernaut => Some(CreatureTypeV60::Juggernaut),
        Subtype::Minotaur => Some(CreatureTypeV60::Minotaur),
        Subtype::Ooze => Some(CreatureTypeV60::Ooze),
        Subtype::Devil => Some(CreatureTypeV60::Devil),
        Subtype::Bear => Some(CreatureTypeV60::Bear),
        Subtype::Phoenix => Some(CreatureTypeV60::Phoenix),
        Subtype::Drake => Some(CreatureTypeV60::Drake),
        Subtype::Sphinx => Some(CreatureTypeV60::Sphinx),
        Subtype::Vivien => None,
        Subtype::Utrom => Some(CreatureTypeV60::Utrom),
        Subtype::Scientist => Some(CreatureTypeV60::Scientist),
        Subtype::Robot => Some(CreatureTypeV60::Robot),
        Subtype::Army => Some(CreatureTypeV60::Army),
        Subtype::Lhurgoyf => Some(CreatureTypeV60::Lhurgoyf),
        Subtype::Halfling => Some(CreatureTypeV60::Halfling),
        Subtype::Wraith => Some(CreatureTypeV60::Wraith),
        Subtype::Mutant => Some(CreatureTypeV60::Mutant),
        Subtype::Archon => Some(CreatureTypeV60::Archon),
        Subtype::Frog => Some(CreatureTypeV60::Frog),
        Subtype::Sorcerer => Some(CreatureTypeV60::Sorcerer),
        Subtype::Bard => Some(CreatureTypeV60::Bard),
        Subtype::Sand => Some(CreatureTypeV60::Sand),
        Subtype::Scion => Some(CreatureTypeV60::Scion),
        Subtype::Crab => Some(CreatureTypeV60::Crab),
        Subtype::Kor => Some(CreatureTypeV60::Kor),
        Subtype::Tamiyo => None,
        Subtype::Jace => None,
        Subtype::Karn => None,
        Subtype::Tezzeret => None,
        Subtype::Arcane => None,
        Subtype::Oko => None,
        Subtype::Ugin => None,
        Subtype::Cave => None,
        Subtype::Cloud => None,
        Subtype::Lair => None,
        Subtype::Locus => None,
        Subtype::Planet => None,
        Subtype::Imp => Some(CreatureTypeV60::Imp),
        Subtype::Cartouche => None,
        Subtype::Eye => Some(CreatureTypeV60::Eye),
        Subtype::Dreadnought => Some(CreatureTypeV60::Dreadnought),
        Subtype::Dack => None,
        Subtype::Incarnation => Some(CreatureTypeV60::Incarnation),
        Subtype::Trap => None,
        Subtype::Minsc => None,
        Subtype::Snake => Some(CreatureTypeV60::Snake),
        Subtype::Wrenn => None,
        Subtype::Hamster => Some(CreatureTypeV60::Hamster),
    }
}

/// Shared checked native decoding; the public legal domain stays independent.
fn native_subtype(id: u16) -> Option<Subtype> {
    Subtype::from_stable_id(id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreatureTypeMatchErrorV60 {
    MissingObject,
    InvalidDefinition,
    UnknownNativeSubtype(u16),
    /// Current native animation data has no explicit universal-type-set fact.
    /// Do not guess from a finite slice or an older animation record.
    UnsupportedAnimationTypeComposition,
}

/// Match one actual effective creature without mutation. The caller still owns
/// zone, controller, incarnation, visibility, and spell-form permissions.
/// Layer 4 replacements suppress printed changeling. Layer 6 ability removal
/// alone does not undo its earlier type contribution (CR 613, 702.73a).
pub fn matches_effective_creature(
    state: &GameState,
    object: ObjectId,
    chosen: CreatureTypeV60,
) -> Result<bool, CreatureTypeMatchErrorV60> {
    let native = state
        .objects
        .try_get(object)
        .ok_or(CreatureTypeMatchErrorV60::MissingObject)?;
    let definition = CARD_DEFS
        .get(usize::from(native.card_def))
        .ok_or(CreatureTypeMatchErrorV60::InvalidDefinition)?;
    let effective = crate::engine::effective_subtype_ids(state, object);
    // Validate the whole effective set even when an all-types effect applies.
    let mut represented = false;
    for id in effective {
        let subtype =
            native_subtype(id).ok_or(CreatureTypeMatchErrorV60::UnknownNativeSubtype(id))?;
        represented |= canonical_native_type(subtype) == Some(chosen);
    }
    let own_animation_record = native.zone == crate::state::Zone::Battlefield
        && native.v4.animation_timestamp.is_some()
        && definition.animation.is_some();
    if own_animation_record
        || crate::continuous_characteristics_v1::animation(state, object).is_some()
    {
        return Err(CreatureTypeMatchErrorV60::UnsupportedAnimationTypeComposition);
    }
    if !crate::engine::object_has_type(state, object, CardType::Creature) {
        return Ok(false);
    }
    let replaced = crate::standard_creatures_v1::creature_types(state, object).is_some()
        || crate::continuous_characteristics_v1::creature_override(state, object).is_some()
        || crate::standard_cards_v1::kaito_is_creature(state, object);
    let changeling = definition.changeling
        && native.v4.face_index == 0
        && native.v4.face_down_v1.is_none()
        && !replaced;
    Ok(represented || changeling)
}

#[cfg(test)]
mod tests;
