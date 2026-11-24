//! Layer-3 color-word text replacement for Alter Reality (issue #460,
//! `docs/design/premodern_shapeshifter_alter_reality_v1.md` §4).
//!
//! Stores per-object color-word changes, stack-item captures (CR 113.7a),
//! changed token printed colors and changed land animation colors in a single
//! optional `GameState::text_color_changes_v1` field. Games without the card
//! leave that field `None`, write nothing, consume no timestamps, and keep
//! byte-identical state serialization, hashes, observations and legal actions.

use crate::card_def::{mana_color_mask, CARD_DEFS};
use crate::effect::ExecCtx;
use crate::engine::UnsupportedMechanic;
use crate::ids::{ObjectId, StackItemId};
use crate::mana::ManaColor;
use crate::state::{GameState, ObjectLinkV4, Zone};
use serde::{Deserialize, Serialize};

/// The five MTG color words in WUBRG order. Colorless (`ManaColor::C`) is not
/// a color word.
pub const COLOR_WORDS: [ManaColor; 5] = [
    ManaColor::W,
    ManaColor::U,
    ManaColor::B,
    ManaColor::R,
    ManaColor::G,
];

/// The 36 distinct cards in the Premodern snapshot whose Oracle text contains
/// a color word whose replacement alters play (design §4.4). Sorted for binary
/// search.
pub const COLOR_WORD_CARDS_V1: [&str; 36] = [
    "Absolute Law",
    "Akroma, Angel of Wrath",
    "Anarchy",
    "Blue Elemental Blast",
    "Call of the Herd",
    "Caller of the Claw",
    "Chatter of the Squirrel",
    "Chill",
    "Circle of Protection: Red",
    "Crimson Acolyte",
    "Decree of Justice",
    "Deranged Hermit",
    "Dystopia",
    "Ghitu Encampment",
    "Gloom",
    "Goblin Piledriver",
    "Hydroblast",
    "Ihsan's Shade",
    "Karmic Guide",
    "Knight of Stromgald",
    "Light of Day",
    "Mystic Enforcer",
    "Phyrexian Processor",
    "Plaguebearer",
    "Pyroblast",
    "Quirion Dryad",
    "Red Elemental Blast",
    "Siege-Gang Commander",
    "Snuff Out",
    "Soltari Monk",
    "Soltari Priest",
    "Sphere of Law",
    "Treetop Village",
    "Verdant Force",
    "Warmth",
    "Zombie Infestation",
];

/// The 5 distinct cards in the Premodern snapshot whose only color word sits
/// in a cost paid from hand or graveyard before Alter Reality can target the
/// spell (design §4.4). Recorded with no gameplay effect. Sorted for binary
/// search.
pub const COST_ONLY_COLOR_WORD_CARDS_V1: [&str; 5] = [
    "Contagion",
    "Flash of Insight",
    "Misdirection",
    "Pyrokinesis",
    "Unmask",
];

/// The 41 distinct Premodern snapshot cards whose Oracle text contains a color
/// word (the 36 active cards plus the 5 cost-only cards, design §4.4). Sorted
/// for binary search.
pub const AUDITED_COLOR_WORD_CARDS_V1: [&str; 41] = [
    "Absolute Law",
    "Akroma, Angel of Wrath",
    "Anarchy",
    "Blue Elemental Blast",
    "Call of the Herd",
    "Caller of the Claw",
    "Chatter of the Squirrel",
    "Chill",
    "Circle of Protection: Red",
    "Contagion",
    "Crimson Acolyte",
    "Decree of Justice",
    "Deranged Hermit",
    "Dystopia",
    "Flash of Insight",
    "Ghitu Encampment",
    "Gloom",
    "Goblin Piledriver",
    "Hydroblast",
    "Ihsan's Shade",
    "Karmic Guide",
    "Knight of Stromgald",
    "Light of Day",
    "Misdirection",
    "Mystic Enforcer",
    "Phyrexian Processor",
    "Plaguebearer",
    "Pyroblast",
    "Pyrokinesis",
    "Quirion Dryad",
    "Red Elemental Blast",
    "Siege-Gang Commander",
    "Snuff Out",
    "Soltari Monk",
    "Soltari Priest",
    "Sphere of Law",
    "Treetop Village",
    "Unmask",
    "Verdant Force",
    "Warmth",
    "Zombie Infestation",
];

/// The 264 distinct cards in the Premodern snapshot audited as having no color
/// word in their rules text (design §4.4). Sorted for binary search.
pub const NO_COLOR_WORD_CARDS_V1: [&str; 264] = [
    "Abeyance",
    "Academy Rector",
    "Accumulated Knowledge",
    "Adarkar Wastes",
    "Akroma's Vengeance",
    "Alter Reality",
    "Ancient Tomb",
    "Anger",
    "Animate Dead",
    "Annul",
    "Anurid Brushhopper",
    "Arcane Laboratory",
    "Argothian Enchantress",
    "Armageddon",
    "Arrogant Wurm",
    "Attunement",
    "Aura of Silence",
    "Auramancer",
    "Avalanche Riders",
    "Avatar of Woe",
    "Back to Basics",
    "Ball Lightning",
    "Bane of the Living",
    "Barbarian Ring",
    "Basking Rootwalla",
    "Battlefield Forge",
    "Battlefield Scrounger",
    "Beloved Chaplain",
    "Birchlore Rangers",
    "Birds of Paradise",
    "Black Vise",
    "Blastoderm",
    "Bloodstained Mire",
    "Brain Freeze",
    "Brushland",
    "Cabal Therapy",
    "Careful Study",
    "Caves of Koilos",
    "Chain of Vapor",
    "Choke",
    "City of Brass",
    "City of Traitors",
    "Coastal Tower",
    "Contamination",
    "Counterspell",
    "Crop Rotation",
    "Crumble",
    "Cursed Scroll",
    "Cursed Totem",
    "Dark Ritual",
    "Daze",
    "Decree of Pain",
    "Decree of Silence",
    "Devoted Caretaker",
    "Diabolic Edict",
    "Diabolic Tutor",
    "Disenchant",
    "Dominate",
    "Dromar's Charm",
    "Druid Lyrist",
    "Duress",
    "Dust Bowl",
    "Earthquake",
    "Elvish Spirit Guide",
    "Enchantress's Presence",
    "Engineered Plague",
    "Enlightened Tutor",
    "Ensnaring Bridge",
    "Envelop",
    "Essence Flare",
    "Eternal Dragon",
    "Exalted Angel",
    "Exhume",
    "Exploration",
    "Fact or Fiction",
    "Fire // Ice",
    "Fireblast",
    "Firebolt",
    "Flaring Pain",
    "Flooded Strand",
    "Foil",
    "Forest",
    "Forsaken City",
    "Frantic Search",
    "Funeral Charm",
    "Future Sight",
    "Fyndhorn Elves",
    "Gaea's Blessing",
    "Gaea's Cradle",
    "Gempalm Incinerator",
    "Gemstone Mine",
    "Genesis",
    "Gerrard's Verdict",
    "Ghitu Slinger",
    "Glory",
    "Glowrider",
    "Goblin Bombardment",
    "Goblin Goon",
    "Goblin Lackey",
    "Goblin Matron",
    "Goblin Ringleader",
    "Goblin Sharpshooter",
    "Goblin Tinkerer",
    "Goblin Vandal",
    "Goblin Warchief",
    "Graveborn Muse",
    "Grim Lavamancer",
    "Gush",
    "Haunting Echoes",
    "Hermit Druid",
    "High Market",
    "Hypnotic Specter",
    "Impulse",
    "Incinerate",
    "Infest",
    "Innocent Blood",
    "Intuition",
    "Iridescent Angel",
    "Island",
    "Jackal Pup",
    "Karmic Justice",
    "Karplusan Forest",
    "Kor Haven",
    "Krosan Reclamation",
    "Krosan Tusker",
    "Lava Dart",
    "Legacy's Allure",
    "Life // Death",
    "Lightning Bolt",
    "Llanowar Elves",
    "Llanowar Wastes",
    "Lotus Petal",
    "Lurking Skirge",
    "Mana Leak",
    "Mana Short",
    "Masticore",
    "Meddling Mage",
    "Mirri's Guile",
    "Mishra's Factory",
    "Mogg Fanatic",
    "Mogg Salvage",
    "Monk Realist",
    "Mother of Runes",
    "Mountain",
    "Mox Diamond",
    "Multani's Acolyte",
    "Multani, Maro-Sorcerer",
    "Nantuko Shade",
    "Nantuko Vigilante",
    "Naturalize",
    "Nether Spirit",
    "Nevinyrral's Disk",
    "Nova Cleric",
    "Oath of Druids",
    "Opalescence",
    "Opt",
    "Orim's Chant",
    "Overgrown Estate",
    "Overload",
    "Palinchron",
    "Parallax Wave",
    "Pernicious Deed",
    "Petradon",
    "Phantom Nishoba",
    "Phyrexian Devourer",
    "Phyrexian Dreadnought",
    "Phyrexian Furnace",
    "Phyrexian Tower",
    "Pillage",
    "Plains",
    "Polluted Delta",
    "Portent",
    "Powder Keg",
    "Price of Progress",
    "Priest of Titania",
    "Prohibit",
    "Psychatog",
    "Putrid Imp",
    "Pyroclasm",
    "Pyrostatic Pillar",
    "Quirion Ranger",
    "Rancid Earth",
    "Rancor",
    "Ravenous Baloth",
    "Ravenous Rats",
    "Ray of Revelation",
    "Raze",
    "Reanimate",
    "Reflecting Pool",
    "Replenish",
    "Rewind",
    "Rishadan Port",
    "Rith's Grove",
    "Root Maze",
    "Sacred Ground",
    "Seal of Cleansing",
    "Seal of Fire",
    "Secluded Steppe",
    "Serra's Sanctum",
    "Shallow Grave",
    "Shared Triumph",
    "Shivan Reef",
    "Sickening Dreams",
    "Skeletal Scrying",
    "Skirk Prospector",
    "Skycloud Expanse",
    "Smother",
    "Solitary Confinement",
    "Sphere of Resistance",
    "Spike Feeder",
    "Squee, Goblin Nabob",
    "Standstill",
    "Stasis",
    "Sterling Grove",
    "Stifle",
    "Stone Rain",
    "Sulfuric Vortex",
    "Survival of the Fittest",
    "Swamp",
    "Swords to Plowshares",
    "Sylvan Library",
    "Tainted Field",
    "Tainted Wood",
    "Terravore",
    "Thermokarst",
    "Thornscape Battlemage",
    "Thwart",
    "Tireless Tribe",
    "Tormod's Crypt",
    "Tranquil Domain",
    "Tranquil Thicket",
    "Treva's Ruins",
    "Triskelion",
    "True Believer",
    "Tsabo's Web",
    "Uktabi Orangutan",
    "Underground River",
    "Undiscovered Paradise",
    "Unearth",
    "Upheaval",
    "Veteran Brawlers",
    "Veteran Explorer",
    "Vindicate",
    "Vintara Snapper",
    "Vision Charm",
    "Volrath's Shapeshifter",
    "Wall of Blossoms",
    "Wall of Roots",
    "Wasteland",
    "Weathered Wayfarer",
    "Wild Growth",
    "Wild Mongrel",
    "Windswept Heath",
    "Winter Orb",
    "Winter's Grasp",
    "Wirewood Symbiote",
    "Withered Wretch",
    "Wooded Foothills",
    "Wrath of God",
    "Xantid Swarm",
    "Yavimaya Coast",
    "Yavimaya Granger",
    "Yavimaya Hollow",
    "Zuran Orb",
];

/// The 9 token definitions created by Premodern snapshot cards (design §4.4).
/// None has rules text; all are audited as having no color words. Sorted for
/// binary search.
pub const AUDITED_POOL_TOKENS_V1: [&str; 9] = [
    "Angel Token",
    "Bear Token",
    "Elephant Token",
    "Goblin Token",
    "Phyrexian Minion Token",
    "Saproling Token",
    "Soldier Token",
    "Squirrel Token",
    "Zombie Token",
];

/// Which color-word choice stage Alter Reality is currently presenting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextColorWordStageV1 {
    /// First stage: choosing the color word to replace (five options in WUBRG order).
    From,
    /// Second stage: choosing the replacement color word (four other colors in WUBRG order).
    To,
}

/// One live Layer-3 color-word replacement on a battlefield permanent or stack
/// spell (CR 612, 613.1c, 613.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextColorChangeV1 {
    pub object: ObjectLinkV4,
    pub from: ManaColor,
    pub to: ManaColor,
    pub timestamp: u64,
}

/// Captured printed-to-read color-word mapping for one stack ability at the
/// moment it was put on the stack (CR 113.7a).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextColorCaptureV1 {
    pub item: StackItemId,
    pub read: Vec<(ManaColor, ManaColor)>,
}

/// Printed color mask of a token whose creating spell or ability had its color
/// word changed. Read at the printed-color fallback of
/// `engine::unpainted_object_color_mask` so Layer-5 setters override it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenColorV1 {
    pub token: ObjectLinkV4,
    pub color_mask: u8,
}

/// Effective color mask of a resolved land animation (Treetop Village, Ghitu
/// Encampment) whose animating ability had a changed color word (CR 611.2c).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnimationColorV1 {
    pub land: ObjectLinkV4,
    pub animation_timestamp: u64,
    pub color_mask: u8,
}

/// Per-game color-word replacement state stored in
/// `GameState::text_color_changes_v1`. Absent (`None`) whenever all four lists
/// are empty.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextColorChangesV1 {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changes: Vec<TextColorChangeV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub captures: Vec<TextColorCaptureV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub token_colors: Vec<TokenColorV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub animation_colors: Vec<AnimationColorV1>,
}

impl TextColorChangesV1 {
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
            && self.captures.is_empty()
            && self.token_colors.is_empty()
            && self.animation_colors.is_empty()
    }
}

/// Origin from which a target-legality query reads color words: live object
/// changes for spells and announced activations, or stack-item capture only
/// for an ability already on the stack (CR 113.7a).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetOriginV1 {
    Object(ObjectLinkV4),
    Ability(StackItemId),
}

/// Whether `color` is one of the five MTG color words (W, U, B, R, G).
pub const fn is_color_word(color: ManaColor) -> bool {
    !matches!(color, ManaColor::C)
}

/// The ordered color options for a `ChooseTextColorWordV1` stage, or `None` if
/// `(stage, from)` is noncanonical.
pub fn choice_colors(
    stage: TextColorWordStageV1,
    from: Option<ManaColor>,
) -> Option<Vec<ManaColor>> {
    match (stage, from) {
        (TextColorWordStageV1::From, None) => Some(COLOR_WORDS.to_vec()),
        (TextColorWordStageV1::To, Some(from)) if is_color_word(from) => Some(
            COLOR_WORDS
                .into_iter()
                .filter(|&color| color != from)
                .collect(),
        ),
        _ => None,
    }
}

/// Whether `name` is in the audited Premodern pool (the 36 active color-word
/// cards, the 5 cost-only color-word cards, the 264 no-color-word cards, or
/// the 9 pool tokens).
pub fn card_name_audited(name: &str) -> bool {
    AUDITED_COLOR_WORD_CARDS_V1.binary_search(&name).is_ok()
        || NO_COLOR_WORD_CARDS_V1.binary_search(&name).is_ok()
        || AUDITED_POOL_TOKENS_V1.binary_search(&name).is_ok()
}

/// Whether `target` is accepted by the Alter Reality audit. Face-down objects
/// are accepted without inspecting their hidden identity (the audit runs when
/// they are turned face up); a face-up Volrath's Shapeshifter is audited by
/// its active Layer-3 full text (design §4.2, §4.4).
pub fn target_audited(state: &GameState, target: ObjectId) -> bool {
    let Some(live) = state.objects.try_get(target) else {
        return false;
    };
    if live.v4.face_down_v1.is_some() {
        return true;
    }
    let definition = crate::full_text_v1::active_definition(state, target)
        .or_else(|| CARD_DEFS.get(usize::from(live.card_def)));
    definition.is_some_and(|def| card_name_audited(def.name))
}

fn live_link(state: &GameState, object: ObjectId) -> Option<ObjectLinkV4> {
    let live = state.objects.try_get(object)?;
    Some(ObjectLinkV4 {
        object,
        zone_change_count: live.zone_change_count,
    })
}

fn prune_if_empty(changes: &mut Option<TextColorChangesV1>) {
    if changes.as_ref().is_some_and(TextColorChangesV1::is_empty) {
        *changes = None;
    }
}

/// Applies every live color-word change on `link` in timestamp order (CR 613.7).
pub fn text_color_for_link(state: &GameState, link: ObjectLinkV4, printed: ManaColor) -> ManaColor {
    if !is_color_word(printed) {
        return printed;
    }
    let Some(record) = state.text_color_changes_v1.as_ref() else {
        return printed;
    };
    if record.changes.is_empty() || !record.changes.iter().any(|entry| entry.object == link) {
        return printed;
    }
    let mut matching = record
        .changes
        .iter()
        .filter(|entry| entry.object == link)
        .map(|entry| (entry.timestamp, entry.from, entry.to))
        .collect::<Vec<_>>();
    matching.sort_by_key(|(timestamp, _, _)| *timestamp);
    let mut current = printed;
    for (_, from, to) in matching {
        if current == from {
            current = to;
        }
    }
    current
}

/// Applies every live color-word change on `object`'s current incarnation in
/// timestamp order (CR 613.7).
pub fn text_color(state: &GameState, object: ObjectId, printed: ManaColor) -> ManaColor {
    if !is_color_word(printed) || state.text_color_changes_v1.is_none() {
        return printed;
    }
    let Some(link) = live_link(state, object) else {
        return printed;
    };
    text_color_for_link(state, link, printed)
}

/// Maps each color bit in `printed_mask` through [`text_color_for_link`].
pub fn text_color_mask_for_link(state: &GameState, link: ObjectLinkV4, printed_mask: u8) -> u8 {
    if printed_mask == 0
        || state
            .text_color_changes_v1
            .as_ref()
            .is_none_or(|record| record.changes.is_empty())
    {
        return printed_mask;
    }
    let mut mapped = printed_mask & !0x1f;
    for color in COLOR_WORDS {
        if printed_mask & mana_color_mask(color) != 0 {
            mapped |= mana_color_mask(text_color_for_link(state, link, color));
        }
    }
    mapped
}

/// Maps each color bit in `printed_mask` through [`text_color`].
pub fn text_color_mask(state: &GameState, object: ObjectId, printed_mask: u8) -> u8 {
    if printed_mask == 0
        || state
            .text_color_changes_v1
            .as_ref()
            .is_none_or(|record| record.changes.is_empty())
    {
        return printed_mask;
    }
    let Some(link) = live_link(state, object) else {
        return printed_mask;
    };
    text_color_mask_for_link(state, link, printed_mask)
}

/// Returns the color word stack item `item` read when it was put on the stack,
/// or `printed` when `item` has no capture (CR 113.7a).
pub fn captured_color(state: &GameState, item: StackItemId, printed: ManaColor) -> ManaColor {
    if !is_color_word(printed) {
        return printed;
    }
    let Some(record) = state.text_color_changes_v1.as_ref() else {
        return printed;
    };
    let Some(capture) = record.captures.iter().find(|entry| entry.item == item) else {
        return printed;
    };
    capture
        .read
        .iter()
        .find(|(original, _)| *original == printed)
        .map_or(printed, |(_, mapped)| *mapped)
}

/// Maps each color bit in `printed_mask` through [`captured_color`].
pub fn captured_color_mask(state: &GameState, item: StackItemId, printed_mask: u8) -> u8 {
    if printed_mask == 0
        || state
            .text_color_changes_v1
            .as_ref()
            .is_none_or(|record| record.captures.is_empty())
    {
        return printed_mask;
    }
    let mut mapped = printed_mask & !0x1f;
    for color in COLOR_WORDS {
        if printed_mask & mana_color_mask(color) != 0 {
            mapped |= mana_color_mask(captured_color(state, item, color));
        }
    }
    mapped
}

/// Whether stack item `item` holds a captured color-word mapping.
pub fn has_capture(state: &GameState, item: StackItemId) -> bool {
    state
        .text_color_changes_v1
        .as_ref()
        .is_some_and(|record| record.captures.iter().any(|entry| entry.item == item))
}

/// Resolves a target specification's color word from its origin.
pub fn target_origin_color(
    state: &GameState,
    origin: Option<TargetOriginV1>,
    printed: ManaColor,
) -> ManaColor {
    if state.text_color_changes_v1.is_none() || !is_color_word(printed) {
        return printed;
    }
    match origin {
        Some(TargetOriginV1::Object(link)) => text_color_for_link(state, link, printed),
        Some(TargetOriginV1::Ability(item)) => captured_color(state, item, printed),
        None => printed,
    }
}

/// Resolves a color word during effect execution. Abilities (those with an
/// `ability_source_contract`, or any stack item holding a capture) read only
/// their captured mapping (CR 113.7a); spells read live changes on `ctx.source`.
pub fn resolving_color(state: &GameState, ctx: &ExecCtx, printed: ManaColor) -> ManaColor {
    if state.text_color_changes_v1.is_none() || !is_color_word(printed) {
        return printed;
    }
    if ctx.ability_source_contract.is_some() {
        return ctx
            .stack_item_id
            .map_or(printed, |item| captured_color(state, item, printed));
    }
    if let Some(item) = ctx.stack_item_id.filter(|&item| has_capture(state, item)) {
        return captured_color(state, item, printed);
    }
    text_color(state, ctx.source, printed)
}

/// Maps each color bit in `printed_mask` through [`resolving_color`].
pub fn resolving_color_mask(state: &GameState, ctx: &ExecCtx, printed_mask: u8) -> u8 {
    if printed_mask == 0 || state.text_color_changes_v1.is_none() {
        return printed_mask;
    }
    let mut mapped = printed_mask & !0x1f;
    for color in COLOR_WORDS {
        if printed_mask & mana_color_mask(color) != 0 {
            mapped |= mana_color_mask(resolving_color(state, ctx, color));
        }
    }
    mapped
}

/// Printed color mask override of a battlefield token whose creator's color
/// word was changed, if any.
pub fn token_color_mask(state: &GameState, token: ObjectId) -> Option<u8> {
    let record = state.text_color_changes_v1.as_ref()?;
    if record.token_colors.is_empty() {
        return None;
    }
    let live = state.objects.try_get(token)?;
    if live.zone != Zone::Battlefield {
        return None;
    }
    let link = ObjectLinkV4 {
        object: token,
        zone_change_count: live.zone_change_count,
    };
    record
        .token_colors
        .iter()
        .find(|entry| entry.token == link)
        .map(|entry| entry.color_mask)
}

/// Changed color mask of an active land animation matching `animation_timestamp`.
pub fn animation_color_mask(
    state: &GameState,
    land: ObjectId,
    animation_timestamp: u64,
) -> Option<u8> {
    let record = state.text_color_changes_v1.as_ref()?;
    if record.animation_colors.is_empty() {
        return None;
    }
    let live = state.objects.try_get(land)?;
    if live.zone != Zone::Battlefield {
        return None;
    }
    let link = ObjectLinkV4 {
        object: land,
        zone_change_count: live.zone_change_count,
    };
    record
        .animation_colors
        .iter()
        .find(|entry| entry.land == link && entry.animation_timestamp == animation_timestamp)
        .map(|entry| entry.color_mask)
}

/// Audits `target` and records a Layer-3 color-word change `from -> to` with a
/// fresh 613.7 timestamp. Halts with
/// `UnsupportedMechanic::TextColorChangeOutsideAuditV1` (source: `source`) and
/// returns `false` if `target` is outside the audited pool, even if `from == to`.
pub fn apply_change(
    state: &mut GameState,
    source: ObjectId,
    target: ObjectId,
    from: ManaColor,
    to: ManaColor,
) -> bool {
    if !target_audited(state, target) {
        state.engine.halted = Some((UnsupportedMechanic::TextColorChangeOutsideAuditV1, source));
        return false;
    }
    if !is_color_word(from) || !is_color_word(to) || from == to {
        return true;
    }
    let Some(link) = live_link(state, target) else {
        return false;
    };
    let timestamp = crate::engine::next_timestamp(state);
    state
        .text_color_changes_v1
        .get_or_insert_with(Default::default)
        .changes
        .push(TextColorChangeV1 {
            object: link,
            from,
            to,
            timestamp,
        });
    true
}

/// Records the changed printed color mask of a newly created battlefield token.
pub fn record_token_color(state: &mut GameState, token: ObjectId, color_mask: u8) {
    let Some(live) = state.objects.try_get(token) else {
        return;
    };
    if live.zone != Zone::Battlefield {
        return;
    }
    let link = ObjectLinkV4 {
        object: token,
        zone_change_count: live.zone_change_count,
    };
    let entries = &mut state
        .text_color_changes_v1
        .get_or_insert_with(Default::default)
        .token_colors;
    entries.retain(|entry| entry.token.object != token);
    entries.push(TokenColorV1 {
        token: link,
        color_mask,
    });
}

/// Removes any prior animation color override for `land`.
pub fn clear_animation_color(state: &mut GameState, land: ObjectId) {
    let Some(record) = state.text_color_changes_v1.as_mut() else {
        return;
    };
    if record.animation_colors.is_empty() {
        return;
    }
    record
        .animation_colors
        .retain(|entry| entry.land.object != land);
    prune_if_empty(&mut state.text_color_changes_v1);
}

/// Records the changed color mask of a resolved land animation keyed by its
/// `animation_timestamp` (CR 611.2c).
pub fn record_animation_color(
    state: &mut GameState,
    land: ObjectId,
    animation_timestamp: u64,
    color_mask: u8,
) {
    let Some(live) = state.objects.try_get(land) else {
        return;
    };
    if live.zone != Zone::Battlefield {
        return;
    }
    let link = ObjectLinkV4 {
        object: land,
        zone_change_count: live.zone_change_count,
    };
    let entries = &mut state
        .text_color_changes_v1
        .get_or_insert_with(Default::default)
        .animation_colors;
    entries.retain(|entry| entry.land.object != land);
    entries.push(AnimationColorV1 {
        land: link,
        animation_timestamp,
        color_mask,
    });
}

/// Captures `source`'s live color-word mapping onto stack item `item` at the
/// moment the ability is put on the stack (CR 113.7a). Writes nothing when
/// `source` has no live change.
pub fn capture_for_stack_item(state: &mut GameState, item: StackItemId, source: ObjectLinkV4) {
    let has_live = state
        .text_color_changes_v1
        .as_ref()
        .is_some_and(|record| record.changes.iter().any(|entry| entry.object == source));
    if !has_live {
        return;
    }
    let read = COLOR_WORDS
        .into_iter()
        .filter_map(|printed| {
            let mapped = text_color_for_link(state, source, printed);
            (mapped != printed).then_some((printed, mapped))
        })
        .collect::<Vec<_>>();
    if read.is_empty() {
        return;
    }
    let captures = &mut state
        .text_color_changes_v1
        .get_or_insert_with(Default::default)
        .captures;
    captures.retain(|entry| entry.item != item);
    captures.push(TextColorCaptureV1 { item, read });
}

/// Removes stack item `item`'s color-word capture once it has finished
/// resolving, fizzled, or been countered/removed.
pub fn remove_capture(state: &mut GameState, item: StackItemId) {
    let Some(record) = state.text_color_changes_v1.as_mut() else {
        return;
    };
    if record.captures.is_empty() {
        return;
    }
    record.captures.retain(|entry| entry.item != item);
    prune_if_empty(&mut state.text_color_changes_v1);
}

/// Halts with `UnsupportedMechanic::TextColorChangeOutsideAuditV1` if
/// `original` has a captured color-word mapping (design §4.2: copying an item
/// that holds a capture halts rather than guessing). Returns `true` when it
/// halted.
pub fn refuse_copied_capture(
    state: &mut GameState,
    original: StackItemId,
    source: ObjectId,
) -> bool {
    if has_capture(state, original) {
        state.engine.halted = Some((UnsupportedMechanic::TextColorChangeOutsideAuditV1, source));
        true
    } else {
        false
    }
}

/// Updates `text_color_changes_v1` across a zone change of `object` from `from`
/// to `to`. Permanent spells resolving from `Stack` to `Battlefield` carry
/// their live color-word changes to the permanent incarnation (CR 400.7a);
/// every other zone change drops `object`'s entries (CR 400.7).
pub fn on_zone_change_field(
    field: &mut Option<TextColorChangesV1>,
    object: ObjectId,
    from: Zone,
    to: Zone,
    old_zone_change_count: u32,
    new_zone_change_count: u32,
) {
    let Some(record) = field.as_mut() else {
        return;
    };
    if from == Zone::Stack && to == Zone::Battlefield {
        let old_link = ObjectLinkV4 {
            object,
            zone_change_count: old_zone_change_count,
        };
        let new_link = ObjectLinkV4 {
            object,
            zone_change_count: new_zone_change_count,
        };
        record.changes.retain_mut(|entry| {
            if entry.object == old_link {
                entry.object = new_link;
                true
            } else {
                entry.object.object != object
            }
        });
    } else {
        record.changes.retain(|entry| entry.object.object != object);
    }
    record
        .token_colors
        .retain(|entry| entry.token.object != object);
    record
        .animation_colors
        .retain(|entry| entry.land.object != object);
    prune_if_empty(field);
}

/// Drops expired `animation_colors` entries during the cleanup step once their
/// `animation_timestamp` has been cleared.
pub fn cleanup_animations(state: &mut GameState) {
    let Some(record) = state.text_color_changes_v1.as_ref() else {
        return;
    };
    if record.animation_colors.is_empty() {
        return;
    }
    let retained = record
        .animation_colors
        .iter()
        .copied()
        .filter(|entry| {
            state
                .objects
                .try_get(entry.land.object)
                .is_some_and(|live| {
                    live.zone == Zone::Battlefield
                        && live.zone_change_count == entry.land.zone_change_count
                        && live.v4.animation_timestamp == Some(entry.animation_timestamp)
                })
        })
        .collect::<Vec<_>>();
    if let Some(record) = state.text_color_changes_v1.as_mut() {
        record.animation_colors = retained;
    }
    prune_if_empty(&mut state.text_color_changes_v1);
}

/// Runs the audit check when a face-down permanent is turned face up
/// (CR 708.8, design §4.2). Halts with
/// `UnsupportedMechanic::TextColorChangeOutsideAuditV1` and returns `false` if
/// it carries a live change and its face-up identity is outside the audit.
pub fn on_turned_face_up(state: &mut GameState, object: ObjectId) -> bool {
    let Some(link) = live_link(state, object) else {
        return true;
    };
    let has_change = state
        .text_color_changes_v1
        .as_ref()
        .is_some_and(|record| record.changes.iter().any(|entry| entry.object == link));
    if has_change && !target_audited(state, object) {
        state.engine.halted = Some((UnsupportedMechanic::TextColorChangeOutsideAuditV1, object));
        return false;
    }
    true
}

/// Returns the live `(from, to)` color-word changes on `object`'s current
/// incarnation, ordered by timestamp (CR 613.7), for Schema 5 projection.
pub fn live_changes_for_object(state: &GameState, object: ObjectId) -> Vec<(ManaColor, ManaColor)> {
    let Some(record) = state.text_color_changes_v1.as_ref() else {
        return Vec::new();
    };
    if record.changes.is_empty() {
        return Vec::new();
    }
    let Some(link) = live_link(state, object) else {
        return Vec::new();
    };
    let mut matching = record
        .changes
        .iter()
        .filter(|entry| entry.object == link)
        .map(|entry| (entry.timestamp, entry.from, entry.to))
        .collect::<Vec<_>>();
    matching.sort_by_key(|(timestamp, _, _)| *timestamp);
    matching
        .into_iter()
        .map(|(_, from, to)| (from, to))
        .collect()
}

/// Whether `state` is suspended on a `ChooseTextColorWordV1` option choice.
pub fn has_pending_text_color_word_choice(state: &GameState) -> bool {
    matches!(
        state
            .engine
            .pending_effect
            .as_ref()
            .and_then(|pending| pending.choice.as_ref()),
        Some(crate::effect::PendingEffectChoice::ChooseOption {
            purpose: crate::effect::EffectOptionChoicePurpose::ChooseTextColorWordV1 { .. },
            ..
        })
    )
}

/// Whether `state` holds text color-word state or a pending text color-word
/// choice that legacy observation modes and non-Schema-5 consumers must refuse.
pub fn requires_successor(state: &GameState) -> bool {
    state.text_color_changes_v1.is_some() || has_pending_text_color_word_choice(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audited_lists_are_sorted_disjoint_and_exact_sizes() {
        assert_eq!(COLOR_WORD_CARDS_V1.len(), 36);
        assert_eq!(COST_ONLY_COLOR_WORD_CARDS_V1.len(), 5);
        assert_eq!(AUDITED_COLOR_WORD_CARDS_V1.len(), 41);
        assert_eq!(NO_COLOR_WORD_CARDS_V1.len(), 264);
        assert_eq!(AUDITED_POOL_TOKENS_V1.len(), 9);

        for list in [
            COLOR_WORD_CARDS_V1.as_slice(),
            COST_ONLY_COLOR_WORD_CARDS_V1.as_slice(),
            AUDITED_COLOR_WORD_CARDS_V1.as_slice(),
            NO_COLOR_WORD_CARDS_V1.as_slice(),
            AUDITED_POOL_TOKENS_V1.as_slice(),
        ] {
            assert!(
                list.windows(2).all(|pair| pair[0] < pair[1]),
                "audited list must be strictly sorted"
            );
        }

        let mut combined_words = COLOR_WORD_CARDS_V1
            .into_iter()
            .chain(COST_ONLY_COLOR_WORD_CARDS_V1)
            .collect::<Vec<_>>();
        combined_words.sort_unstable();
        assert_eq!(combined_words, AUDITED_COLOR_WORD_CARDS_V1);

        for name in AUDITED_COLOR_WORD_CARDS_V1 {
            assert!(
                NO_COLOR_WORD_CARDS_V1.binary_search(&name).is_err(),
                "{name} in both color-word and no-word tables"
            );
        }
        for token in AUDITED_POOL_TOKENS_V1 {
            assert!(AUDITED_COLOR_WORD_CARDS_V1.binary_search(&token).is_err());
            assert!(NO_COLOR_WORD_CARDS_V1.binary_search(&token).is_err());
        }
        assert!(card_name_audited("Circle of Protection: Red"));
        assert!(!card_name_audited("Circle of Protection: Blue"));
    }

    #[test]
    fn choice_colors_offer_wubrg_and_four_others() {
        assert_eq!(
            choice_colors(TextColorWordStageV1::From, None),
            Some(vec![
                ManaColor::W,
                ManaColor::U,
                ManaColor::B,
                ManaColor::R,
                ManaColor::G,
            ])
        );
        assert_eq!(
            choice_colors(TextColorWordStageV1::From, Some(ManaColor::R)),
            None
        );
        assert_eq!(choice_colors(TextColorWordStageV1::To, None), None);
        assert_eq!(
            choice_colors(TextColorWordStageV1::To, Some(ManaColor::C)),
            None
        );
        let mut pairs = 0usize;
        for from in COLOR_WORDS {
            let tos = choice_colors(TextColorWordStageV1::To, Some(from)).expect("valid from");
            assert_eq!(tos.len(), 4);
            assert!(!tos.contains(&from));
            pairs += tos.len();
        }
        assert_eq!(pairs, 20);
    }

    #[cfg(feature = "eternal-fixtures")]
    #[test]
    fn phasing_preserves_live_color_word_change() {
        use crate::card_def::{card_id_by_name, CARD_DEFS};
        use crate::ids::PlayerId;
        use crate::state::{GameObject, ObjectStateV4, Step};

        let island = card_id_by_name("Island").unwrap();
        let piledriver_def = card_id_by_name("Goblin Piledriver").unwrap();
        let ids = vec![island; 15];
        let mut state = GameState::new_from_libraries(
            &ids,
            &ids,
            |id| CARD_DEFS[id as usize].object_name.into(),
            0x7e47_0001,
        );
        state.active_player = PlayerId::P0;
        state.priority_player = PlayerId::P0;
        state.step = Step::Main1;
        let piledriver = state.objects.push(GameObject {
            card_def: piledriver_def,
            name: CARD_DEFS[piledriver_def as usize].object_name.into(),
            owner: PlayerId::P0,
            controller: PlayerId::P0,
            zone: Zone::Battlefield,
            tapped: false,
            summoning_sick: false,
            damage: 0,
            counters: Default::default(),
            attachments: Vec::new(),
            v4: ObjectStateV4::from_card_def(piledriver_def),
            spell_copy_origin: None,
            plotted_turn: None,
            zone_change_count: 0,
        });
        state.players[0].battlefield.push(piledriver);

        assert!(apply_change(
            &mut state,
            piledriver,
            piledriver,
            ManaColor::U,
            ManaColor::G
        ));
        assert_eq!(text_color(&state, piledriver, ManaColor::U), ManaColor::G);

        // Phasing is not a zone change (CR 702.26): the record stays intact.
        crate::eternal_e5a_v1::phasing::phase_out(&mut state, piledriver);
        assert_eq!(state.objects.get(piledriver).zone, Zone::Command);
        assert_eq!(text_color(&state, piledriver, ManaColor::U), ManaColor::G);

        crate::eternal_e5a_v1::phasing::phase_in(&mut state, PlayerId::P0);
        assert_eq!(state.objects.get(piledriver).zone, Zone::Battlefield);
        assert_eq!(text_color(&state, piledriver, ManaColor::U), ManaColor::G);
        assert_eq!(
            crate::eternal_premodern_p1a_v1::statics::protection_mask(&state, piledriver),
            mana_color_mask(ManaColor::G)
        );
    }

    #[cfg(not(any(
        feature = "limited-fdn-fixtures",
        feature = "standard-magezero-fixtures"
    )))]
    #[test]
    fn spell_copy_excludes_live_text_change_and_copy_of_captured_item_halts() {
        use crate::card_def::{card_id_by_name, CARD_DEFS};
        use crate::engine::{self, Action};
        use crate::ids::PlayerId;
        use crate::state::{GameObject, ObjectStateV4, Step, Target};

        let island = card_id_by_name("Island").unwrap();
        let ids = vec![island; 15];
        let mut state = GameState::new_from_libraries(
            &ids,
            &ids,
            |id| CARD_DEFS[id as usize].object_name.into(),
            0x7e47_0002,
        );
        state.active_player = PlayerId::P0;
        state.priority_player = PlayerId::P0;
        state.step = Step::Main1;
        let mountain_def = card_id_by_name("Mountain").unwrap();
        let mountain = state.objects.push(GameObject {
            card_def: mountain_def,
            name: CARD_DEFS[mountain_def as usize].object_name.into(),
            owner: PlayerId::P1,
            controller: PlayerId::P1,
            zone: Zone::Battlefield,
            tapped: false,
            summoning_sick: false,
            damage: 0,
            counters: Default::default(),
            attachments: Vec::new(),
            v4: ObjectStateV4::from_card_def(mountain_def),
            spell_copy_origin: None,
            plotted_turn: None,
            zone_change_count: 0,
        });
        state.players[1].battlefield.push(mountain);
        let hydro_def = card_id_by_name("Hydroblast").unwrap();
        let hydro = state.objects.push(GameObject {
            card_def: hydro_def,
            name: CARD_DEFS[hydro_def as usize].object_name.into(),
            owner: PlayerId::P0,
            controller: PlayerId::P0,
            zone: Zone::Hand,
            tapped: false,
            summoning_sick: false,
            damage: 0,
            counters: Default::default(),
            attachments: Vec::new(),
            v4: ObjectStateV4::from_card_def(hydro_def),
            spell_copy_origin: None,
            plotted_turn: None,
            zone_change_count: 0,
        });
        state.players[0].hand.push(hydro);
        state.players[0].mana_pool[ManaColor::U.pool_index()] = 1;
        engine::step(&mut state, Action::CastSpell(hydro)).unwrap();
        engine::step(&mut state, Action::ChooseSpellMode(1)).unwrap();
        engine::step(&mut state, Action::ChooseTarget(Target::Object(mountain))).unwrap();
        let stack_item_id = state.stack.last().unwrap().v4.stack_item_id;

        // Live change on the original spell is NOT copied (CR 707.2).
        assert!(apply_change(
            &mut state,
            hydro,
            hydro,
            ManaColor::R,
            ManaColor::G
        ));
        let (copy_obj, _copy_item) =
            engine::copy_spell_for(&mut state, hydro, PlayerId::P0).expect("copy spell");
        assert!(state.engine.halted.is_none());
        assert_eq!(text_color(&state, hydro, ManaColor::R), ManaColor::G);
        assert_eq!(text_color(&state, copy_obj, ManaColor::R), ManaColor::R);

        // Copying a stack item that holds a capture halts with TextColorChangeOutsideAuditV1.
        let hydro_link = live_link(&state, hydro).unwrap();
        capture_for_stack_item(&mut state, stack_item_id, hydro_link);
        assert!(has_capture(&state, stack_item_id));
        let denied = engine::copy_spell_for(&mut state, hydro, PlayerId::P0);
        assert!(denied.is_none());
        assert_eq!(
            state.engine.halted,
            Some((UnsupportedMechanic::TextColorChangeOutsideAuditV1, hydro))
        );
    }
}
