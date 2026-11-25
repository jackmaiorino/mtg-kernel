//! Deterministic Vintage Cube draft and deck-building environment (v1).
//!
//! An eight-seat booster draft over the draftable Vintage Cube pool (the
//! cube list minus every card its gap record lists), a deck-building step and
//! a round-robin pod of games through the custom-deck Limited session
//! ([`LimitedJsonlServerV1`], schema 5, public shroud).
//!
//! Everything is reproducible from one `u64` draft seed: the packs, each
//! seat's picker, deck building and each game take explicitly derived
//! sub-seeds ([`derive_seed`]). No result depends on hash-map iteration
//! order, time or thread scheduling.
//!
//! Project law: the only training signal will ever be game win or loss, and
//! no hand-written or heuristic player may ever be an opponent or teacher in
//! training. The pickers ([`UniformPicker`], [`ColourCountPicker`]) and the
//! deck-building rule ([`StatedRuleDeckBuilder`]) in this module are test
//! fixtures and reference baselines only.

use crate::card_def::{card_id_by_name, CardDef, Subtype, CARD_DEFS};
use crate::effect::{EffectOp, LibraryCardFilter};
use crate::limited_session_v1::LimitedJsonlServerV1;
use crate::mana::{ManaColor, Pip};
use crate::state::{SplitMix64, Zone};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Seats at the table.
pub const SEATS: usize = 8;
/// Pack rounds per draft.
pub const ROUNDS: usize = 3;
/// Cards per pack, and so picks per round.
pub const PACK_SIZE: usize = 15;
/// Cards each seat drafts.
pub const POOL_CARDS: usize = ROUNDS * PACK_SIZE;
/// Distinct cube cards opened in one draft.
pub const DRAFTED_CARDS: usize = SEATS * POOL_CARDS;
/// Cards in a built deck.
pub const DECK_CARDS: u32 = 40;
/// Games in a round-robin pod: every pair of seats once.
pub const POD_GAMES: usize = SEATS * (SEATS - 1) / 2;
/// The Limited session schema the games use (public shroud).
pub const SESSION_SCHEMA_VERSION: u32 = 5;
/// Schema tag of [`DraftRecord`].
pub const DRAFT_RECORD_SCHEMA: &str = "cube_draft_record/v1";

/// The five colours in tie-break order (W, U, B, R, G). Bit `i` of a colour
/// mask in this module is `COLOUR_ORDER[i]`.
pub const COLOUR_ORDER: [ManaColor; 5] = [
    ManaColor::W,
    ManaColor::U,
    ManaColor::B,
    ManaColor::R,
    ManaColor::G,
];

/// Basic land names, in [`COLOUR_ORDER`].
pub const BASIC_LAND_NAMES: [&str; 5] = ["Plains", "Island", "Swamp", "Mountain", "Forest"];

const ALL_COLOURS: u8 = 0b1_1111;
const SPELL_TARGET_MAX: usize = 23;
const SPELL_TARGET_MIN: usize = 22;
const NONBASIC_LANDS_MAX: usize = 3;

/// Errors of the draft environment. None of them is a panic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CubeDraftError {
    /// A cube list or gap record text is not the expected JSON.
    Json(String),
    /// A cube or basic land name the registry does not know.
    UnknownCard(String),
    /// A fence record names a card that is not on the cube list.
    FencedCardNotInCube(String),
    /// The draftable pool has fewer distinct cards than one draft opens.
    PoolTooSmall { have: usize, need: usize },
    /// A picker returned an index outside the pack in front of it.
    PickOutOfRange {
        seat: usize,
        round: usize,
        pick: usize,
        index: usize,
        pack_len: usize,
    },
    /// A deck that does not validate, or cannot be built.
    Deck(String),
    /// A pod game index outside `0..POD_GAMES`.
    GameOutOfRange { game: usize },
}

impl fmt::Display for CubeDraftError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CubeDraftError::Json(message) => write!(f, "invalid cube JSON: {message}"),
            CubeDraftError::UnknownCard(name) => write!(f, "unknown card {name:?}"),
            CubeDraftError::FencedCardNotInCube(name) => {
                write!(f, "fenced card {name:?} is not on the cube list")
            }
            CubeDraftError::PoolTooSmall { have, need } => {
                write!(f, "draftable pool has {have} distinct cards, a draft needs {need}")
            }
            CubeDraftError::PickOutOfRange {
                seat,
                round,
                pick,
                index,
                pack_len,
            } => write!(
                f,
                "seat {seat} round {round} pick {pick}: index {index} is outside a pack of {pack_len}"
            ),
            CubeDraftError::Deck(message) => write!(f, "invalid deck: {message}"),
            CubeDraftError::GameOutOfRange { game } => {
                write!(f, "pod game {game} is outside 0..{POD_GAMES}")
            }
        }
    }
}

impl std::error::Error for CubeDraftError {}

// ---------------------------------------------------------------------------
// Seeds
// ---------------------------------------------------------------------------

/// The independent random streams derived from one parent seed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeedStream {
    /// Pack generation (index 0).
    Packs = 1,
    /// A seat's picker (index: seat).
    Picker = 2,
    /// A seat's deck builder (index: seat). The stated-rule builder draws no
    /// randomness; the stream is reserved for builders that do.
    DeckBuild = 3,
    /// The pod of games (index 0).
    Pod = 4,
    /// One pod game (parent: the pod seed; index: game).
    Game = 5,
    /// A game's seat-order coin (parent: the game seed).
    GameCoin = 6,
    /// A game's session `env_seed` (parent: the game seed).
    GameEnv = 7,
    /// A game's uniform action choices (parent: the game seed).
    GameActions = 8,
}

const SEED_DOMAIN_V1: u64 = 0x4355_4245_4452_4654;

/// Derives the sub-seed of `stream` number `index` from `parent`, by chained
/// [`SplitMix64`] outputs.
pub fn derive_seed(parent: u64, stream: SeedStream, index: u64) -> u64 {
    let mut rng = SplitMix64::seed(SEED_DOMAIN_V1 ^ parent);
    let first = rng.next_u64();
    let mut rng = SplitMix64::seed(first ^ (stream as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    let second = rng.next_u64();
    let mut rng = SplitMix64::seed(second ^ index.wrapping_mul(0xD1B5_4A32_D192_ED03));
    rng.next_u64()
}

/// Seed of the pack generation of a draft.
pub fn packs_seed(draft_seed: u64) -> u64 {
    derive_seed(draft_seed, SeedStream::Packs, 0)
}

/// Seed of seat `seat`'s picker in a draft.
pub fn picker_seed(draft_seed: u64, seat: usize) -> u64 {
    derive_seed(draft_seed, SeedStream::Picker, seat as u64)
}

/// Seed of seat `seat`'s deck builder in a draft.
pub fn deck_seed(draft_seed: u64, seat: usize) -> u64 {
    derive_seed(draft_seed, SeedStream::DeckBuild, seat as u64)
}

/// Seed of the pod of games after a draft.
pub fn pod_seed(draft_seed: u64) -> u64 {
    derive_seed(draft_seed, SeedStream::Pod, 0)
}

/// Seed of game `game` of a pod.
pub fn game_seed(pod_seed: u64, game: usize) -> u64 {
    derive_seed(pod_seed, SeedStream::Game, game as u64)
}

/// An unbiased index in `0..bound` (`bound > 0`).
fn uniform_index(rng: &mut SplitMix64, bound: usize) -> usize {
    let bound = bound as u64;
    let threshold = bound.wrapping_neg() % bound;
    loop {
        let value = rng.next_u64();
        if value >= threshold {
            return (value % bound) as usize;
        }
    }
}

// ---------------------------------------------------------------------------
// Colours
// ---------------------------------------------------------------------------

fn colour_bit(color: ManaColor) -> u8 {
    match color {
        ManaColor::W => 1,
        ManaColor::U => 2,
        ManaColor::B => 4,
        ManaColor::R => 8,
        ManaColor::G => 16,
        ManaColor::C => 0,
    }
}

fn pip_bits(pip: Pip) -> u8 {
    match pip {
        // A Phyrexian symbol keeps its printed colour, whatever may pay it.
        Pip::Colored(color) | Pip::Phyrexian(color) | Pip::PhyrexianAnyColor(color) => {
            colour_bit(color)
        }
        Pip::Hybrid(first, second) => colour_bit(first) | colour_bit(second),
    }
}

fn def(id: u16) -> &'static CardDef {
    &CARD_DEFS[usize::from(id)]
}

/// The mana symbols printed on a card: its mana cost and, for a split card
/// or an adventure, the cost of its second half.
fn printed_pips(def: &'static CardDef) -> impl Iterator<Item = Pip> {
    def.cost
        .pips
        .iter()
        .chain(def.adventure.iter().flat_map(|half| half.cost.pips.iter()))
        .copied()
}

/// The colour mask of a card (bit `i`: [`COLOUR_ORDER`]`[i]`): its colour
/// data together with the colours of its mana symbols (both halves of a
/// split card or adventure). Zero for a colourless card.
pub fn card_colour_bits(id: u16) -> u8 {
    let def = def(id);
    let mut bits = def
        .colors
        .iter()
        .fold(0, |bits, color| bits | colour_bit(*color));
    for pip in printed_pips(def) {
        bits |= pip_bits(pip);
    }
    bits
}

/// The colours of mana a land can produce, as a colour mask: its tap
/// abilities, additional and restricted mana abilities and other production.
/// A land that adds a colour chosen as it enters counts as every colour.
pub fn land_mana_bits(id: u16) -> u8 {
    let def = def(id);
    let mut bits = 0u8;
    for color in def
        .mana_ability_choices
        .iter()
        .chain(def.produces_mana.iter())
    {
        bits |= colour_bit(*color);
    }
    for ability in def.additional_mana_abilities {
        for color in ability.colors {
            bits |= colour_bit(*color);
        }
    }
    for ability in def.restricted_mana_abilities {
        for color in ability.colors {
            bits |= colour_bit(*color);
        }
    }
    if def.mana_ability_includes_chosen_color {
        bits |= ALL_COLOURS;
    }
    bits
}

/// The colour of a basic land type, as a colour mask; zero for any other
/// subtype.
fn basic_land_type_bit(subtype: Subtype) -> u8 {
    match subtype {
        Subtype::Plains => 1,
        Subtype::Island => 2,
        Subtype::Swamp => 4,
        Subtype::Mountain => 8,
        Subtype::Forest => 16,
        _ => 0,
    }
}

fn basic_land_types_bits(subtypes: &[Subtype]) -> u8 {
    subtypes
        .iter()
        .fold(0, |bits, subtype| bits | basic_land_type_bit(*subtype))
}

/// The colours of the basic land types a library search filter can find.
/// A filter that can find any basic land counts as every colour; a filter
/// that names no basic land type counts as none.
fn search_filter_bits(filter: LibraryCardFilter) -> u8 {
    match filter {
        LibraryCardFilter::LandWithSubtype(subtype)
        | LibraryCardFilter::CardWithSubtypeV1(subtype) => basic_land_type_bit(subtype),
        LibraryCardFilter::CardWithAnySubtypeV1(subtypes) => basic_land_types_bits(&subtypes),
        LibraryCardFilter::BasicLandWithAnySubtype(subtypes) => basic_land_types_bits(&subtypes),
        LibraryCardFilter::BasicLand
        | LibraryCardFilter::BasicLandOrGate
        | LibraryCardFilter::AnyLand
        | LibraryCardFilter::LandWithBasicLandType => ALL_COLOURS,
        _ => 0,
    }
}

/// The colours of the lands an effect can search for and put onto the
/// battlefield, through sequences and conditionals.
fn effect_fetch_bits(effect: &EffectOp) -> u8 {
    match effect {
        EffectOp::SearchLibraryCardsToDestination { filter, .. } => search_filter_bits(*filter),
        EffectOp::SearchLibraryToBattlefieldTapped { filter, .. } => search_filter_bits(*filter),
        EffectOp::Sequence(effects) => effects
            .iter()
            .fold(0, |bits, effect| bits | effect_fetch_bits(effect)),
        EffectOp::Conditional { then, else_, .. } => {
            effect_fetch_bits(then) | effect_fetch_bits(else_)
        }
        _ => 0,
    }
}

/// The colours a land can fetch: the basic land types the library searches
/// of its battlefield abilities can find, read from the abilities' effects
/// ([`LibraryCardFilter`]), never from card names. A fetchland for "a Swamp
/// or Mountain card" is black and red; one for "a basic land card" is every
/// colour.
pub fn land_fetch_bits(id: u16) -> u8 {
    def(id)
        .activated_abilities
        .iter()
        .filter(|ability| ability.activation_zone == Zone::Battlefield)
        .fold(0, |bits, ability| {
            bits | effect_fetch_bits(&(ability.effect)())
        })
}

/// The colours a land supplies: those it produces ([`land_mana_bits`]) and
/// those it can fetch ([`land_fetch_bits`]).
pub fn land_colour_bits(id: u16) -> u8 {
    land_mana_bits(id) | land_fetch_bits(id)
}

/// The two colours with the highest counts, ties broken in [`COLOUR_ORDER`].
fn top_two_colours(counts: &[u32; 5]) -> [usize; 2] {
    let mut order = [0usize, 1, 2, 3, 4];
    // A stable sort keeps W, U, B, R, G order among equal counts.
    order.sort_by_key(|&colour| std::cmp::Reverse(counts[colour]));
    [order[0], order[1]]
}

fn add_colour_counts(counts: &mut [u32; 5], bits: u8) {
    for (index, count) in counts.iter_mut().enumerate() {
        if bits & (1 << index) != 0 {
            *count += 1;
        }
    }
}

// ---------------------------------------------------------------------------
// Pool
// ---------------------------------------------------------------------------

/// The draftable cube pool: the cube list minus every card the gap record
/// lists and every card the fence record (if any) lists, as registry card ids
/// and cube names in cube-list order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CubePool {
    ids: Vec<u16>,
    names: Vec<String>,
}

impl CubePool {
    /// Builds the pool from the texts of the cube list
    /// (`{"cards": [{"name": ...}, ...]}`), its gap record (every
    /// top-level array of names, such as `partial`, `fdn_only` and
    /// `missing`, is excluded) and an optional fence record (see
    /// [`parse_fence_record`]), whose cards are excluded too. `None` fences
    /// nothing. A name listed twice, or two names of one registry card, enter
    /// the pool once, at the first position. A name outside the gap record
    /// and the fence that the registry does not know is an error, and so is a
    /// fenced name that is not on the cube list.
    pub fn from_json_texts(
        cube_list: &str,
        gap_record: &str,
        fence_record: Option<&str>,
    ) -> Result<Self, CubeDraftError> {
        let cube: Value = serde_json::from_str(cube_list)
            .map_err(|error| CubeDraftError::Json(format!("cube list: {error}")))?;
        let fenced_names = match fence_record {
            Some(text) => parse_fence_record(text)?,
            None => BTreeSet::new(),
        };
        let gaps: Value = serde_json::from_str(gap_record)
            .map_err(|error| CubeDraftError::Json(format!("gap record: {error}")))?;
        let gap_object = gaps
            .as_object()
            .ok_or_else(|| CubeDraftError::Json("gap record is not an object".to_owned()))?;
        let mut gap_names: BTreeSet<&str> = BTreeSet::new();
        for value in gap_object.values() {
            if let Some(names) = value.as_array() {
                for name in names {
                    let name = name.as_str().ok_or_else(|| {
                        CubeDraftError::Json("gap record name is not a string".to_owned())
                    })?;
                    gap_names.insert(name);
                }
            }
        }
        let cards = cube["cards"]
            .as_array()
            .ok_or_else(|| CubeDraftError::Json("cube list has no cards array".to_owned()))?;
        let cube_names: BTreeSet<&str> = cards
            .iter()
            .map(|card| {
                card["name"]
                    .as_str()
                    .ok_or_else(|| CubeDraftError::Json("cube card without a name".to_owned()))
            })
            .collect::<Result<_, _>>()?;
        if let Some(name) = fenced_names
            .iter()
            .find(|name| !cube_names.contains(name.as_str()))
        {
            return Err(CubeDraftError::FencedCardNotInCube(name.clone()));
        }
        let mut seen: BTreeSet<u16> = BTreeSet::new();
        let mut pool = CubePool {
            ids: Vec::with_capacity(cards.len()),
            names: Vec::with_capacity(cards.len()),
        };
        for card in cards {
            let name = card["name"]
                .as_str()
                .ok_or_else(|| CubeDraftError::Json("cube card without a name".to_owned()))?;
            if gap_names.contains(name) || fenced_names.contains(name) {
                continue;
            }
            let id = card_id_by_name(name)
                .ok_or_else(|| CubeDraftError::UnknownCard(name.to_owned()))?;
            if seen.insert(id) {
                pool.ids.push(id);
                pool.names.push(name.to_owned());
            }
        }
        Ok(pool)
    }

    /// Card ids in cube-list order.
    pub fn ids(&self) -> &[u16] {
        &self.ids
    }

    /// Cube names in cube-list order (parallel to [`CubePool::ids`]).
    pub fn names(&self) -> &[String] {
        &self.names
    }

    /// Number of distinct draftable cards.
    pub fn card_count(&self) -> usize {
        self.ids.len()
    }
}

/// The fenced card names of a fence record: `{"cards": [{"name": ...,
/// "reason": ...}, ...]}`, or the bare `[{"name": ..., "reason": ...}, ...]`
/// list that `tools/cube_v1/pool.py` reads. Every entry needs a non-empty
/// string `name` and a non-empty string `reason`; a name listed twice is an
/// error. An empty list fences nothing.
pub fn parse_fence_record(text: &str) -> Result<BTreeSet<String>, CubeDraftError> {
    let record: Value = serde_json::from_str(text)
        .map_err(|error| CubeDraftError::Json(format!("fence record: {error}")))?;
    let entries = match &record {
        Value::Array(entries) => entries,
        Value::Object(object) => object
            .get("cards")
            .and_then(Value::as_array)
            .ok_or_else(|| CubeDraftError::Json("fence record has no cards array".to_owned()))?,
        _ => {
            return Err(CubeDraftError::Json(
                "fence record is neither an object nor a list".to_owned(),
            ))
        }
    };
    let mut names = BTreeSet::new();
    for (index, entry) in entries.iter().enumerate() {
        let name = entry["name"]
            .as_str()
            .filter(|name| !name.trim().is_empty())
            .ok_or_else(|| {
                CubeDraftError::Json(format!("fence entry {index} has no non-empty string name"))
            })?;
        if entry["reason"]
            .as_str()
            .is_none_or(|reason| reason.trim().is_empty())
        {
            return Err(CubeDraftError::Json(format!(
                "fence entry {index} ({name:?}) has no non-empty string reason"
            )));
        }
        if !names.insert(name.to_owned()) {
            return Err(CubeDraftError::Json(format!(
                "fence record lists {name:?} twice"
            )));
        }
    }
    Ok(names)
}

// ---------------------------------------------------------------------------
// Packs and passing
// ---------------------------------------------------------------------------

/// Opened packs, indexed `[round][seat]`, 15 cards each.
pub type OpenedPacks = Vec<Vec<Vec<u16>>>;

/// The 24 packs of a draft: 360 distinct pool cards drawn without
/// replacement (a partial Fisher-Yates shuffle of the pool under
/// [`packs_seed`]); pack `(round, seat)` holds cards
/// `[(round * 8 + seat) * 15, +15)` of the shuffle.
pub fn generate_packs(pool: &CubePool, draft_seed: u64) -> Result<OpenedPacks, CubeDraftError> {
    let size = pool.ids.len();
    if size < DRAFTED_CARDS {
        return Err(CubeDraftError::PoolTooSmall {
            have: size,
            need: DRAFTED_CARDS,
        });
    }
    let mut order: Vec<usize> = (0..size).collect();
    let mut rng = SplitMix64::seed(packs_seed(draft_seed));
    for position in 0..DRAFTED_CARDS {
        let other = position + uniform_index(&mut rng, size - position);
        order.swap(position, other);
    }
    Ok((0..ROUNDS)
        .map(|round| {
            (0..SEATS)
                .map(|seat| {
                    let start = (round * SEATS + seat) * PACK_SIZE;
                    order[start..start + PACK_SIZE]
                        .iter()
                        .map(|&index| pool.ids[index])
                        .collect()
                })
                .collect()
        })
        .collect())
}

/// The seat that receives `seat`'s pack after a pick in `round`: rounds 0
/// and 2 pass to the left (seat + 1), round 1 to the right (seat - 1),
/// modulo 8.
pub fn receiving_seat(seat: usize, round: usize) -> usize {
    if round % 2 == 1 {
        (seat + SEATS - 1) % SEATS
    } else {
        (seat + 1) % SEATS
    }
}

fn pass_packs(packs: Vec<Vec<u16>>, round: usize) -> Vec<Vec<u16>> {
    let mut passed = vec![Vec::new(); SEATS];
    for (seat, pack) in packs.into_iter().enumerate() {
        passed[receiving_seat(seat, round)] = pack;
    }
    passed
}

// ---------------------------------------------------------------------------
// Picking
// ---------------------------------------------------------------------------

/// What a drafter at the table may know when picking: its seat, the round
/// and pick number (both from 0), the pack in front of it and its own picks
/// so far, in order.
#[derive(Debug, Clone, Copy)]
pub struct PickView<'a> {
    pub seat: usize,
    pub round: usize,
    pub pick: usize,
    pub pack: &'a [u16],
    pub picks: &'a [u16],
}

/// A drafter: returns the index in `view.pack` of the card it takes. An
/// index outside the pack makes the draft return
/// [`CubeDraftError::PickOutOfRange`].
pub trait DraftPicker {
    fn pick(&mut self, view: &PickView<'_>) -> usize;
}

/// Test fixture and reference baseline only, never an opponent or teacher in
/// training: picks uniformly over the pack with its own seeded RNG.
#[derive(Debug, Clone)]
pub struct UniformPicker {
    rng: SplitMix64,
}

impl UniformPicker {
    pub fn new(seed: u64) -> Self {
        UniformPicker {
            rng: SplitMix64::seed(seed),
        }
    }
}

impl DraftPicker for UniformPicker {
    fn pick(&mut self, view: &PickView<'_>) -> usize {
        if view.pack.is_empty() {
            return 0;
        }
        uniform_index(&mut self.rng, view.pack.len())
    }
}

/// Test fixture and reference baseline only, never an opponent or teacher in
/// training. Picks uniformly among the pack's cards whose colours lie within
/// its two most-picked colours so far (a picked card counts once for each
/// of its colours; ties broken W, U, B, R, G; colourless cards always fit),
/// else uniformly over the pack. Colours it has not picked yet do not enter
/// the pair, so before its first coloured pick it picks uniformly.
#[derive(Debug, Clone)]
pub struct ColourCountPicker {
    rng: SplitMix64,
}

impl ColourCountPicker {
    pub fn new(seed: u64) -> Self {
        ColourCountPicker {
            rng: SplitMix64::seed(seed),
        }
    }
}

impl DraftPicker for ColourCountPicker {
    fn pick(&mut self, view: &PickView<'_>) -> usize {
        if view.pack.is_empty() {
            return 0;
        }
        let mut counts = [0u32; 5];
        for &id in view.picks {
            add_colour_counts(&mut counts, card_colour_bits(id));
        }
        let mut pair = 0u8;
        for colour in top_two_colours(&counts) {
            if counts[colour] > 0 {
                pair |= 1 << colour;
            }
        }
        if pair != 0 {
            let fitting: Vec<usize> = view
                .pack
                .iter()
                .enumerate()
                .filter(|&(_, &id)| card_colour_bits(id) & !pair == 0)
                .map(|(index, _)| index)
                .collect();
            if !fitting.is_empty() {
                return fitting[uniform_index(&mut self.rng, fitting.len())];
            }
        }
        uniform_index(&mut self.rng, view.pack.len())
    }
}

/// Eight [`UniformPicker`]s seeded from the draft seed ([`picker_seed`]).
pub fn uniform_pickers(draft_seed: u64) -> [Box<dyn DraftPicker>; SEATS] {
    std::array::from_fn(|seat| {
        Box::new(UniformPicker::new(picker_seed(draft_seed, seat))) as Box<dyn DraftPicker>
    })
}

/// Eight [`ColourCountPicker`]s seeded from the draft seed ([`picker_seed`]).
pub fn colour_count_pickers(draft_seed: u64) -> [Box<dyn DraftPicker>; SEATS] {
    std::array::from_fn(|seat| {
        Box::new(ColourCountPicker::new(picker_seed(draft_seed, seat))) as Box<dyn DraftPicker>
    })
}

/// One pick: the pack as the seat saw it and the card it took.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PickRecord {
    pub seat: usize,
    pub round: usize,
    pub pick: usize,
    pub pack: Vec<u16>,
    pub chosen: u16,
}

/// A complete draft: packs as opened (`[round][seat]`), every pick in order
/// (pick step, then seat), and each seat's final pool in pick order. Card
/// values are registry ids of this build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DraftRecord {
    pub schema: &'static str,
    pub draft_seed: u64,
    pub packs: OpenedPacks,
    pub picks: Vec<PickRecord>,
    pub pools: Vec<Vec<u16>>,
}

impl DraftRecord {
    /// The record as JSON.
    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).expect("a draft record is plain data")
    }
}

/// Runs a draft: 3 rounds of 15 picks; at each pick step all 8 seats pick
/// from the pack in front of them, then every pack passes (left, right,
/// left).
pub fn run_draft(
    pool: &CubePool,
    draft_seed: u64,
    pickers: &mut [Box<dyn DraftPicker>; SEATS],
) -> Result<DraftRecord, CubeDraftError> {
    let opened = generate_packs(pool, draft_seed)?;
    let mut pools: Vec<Vec<u16>> = (0..SEATS).map(|_| Vec::with_capacity(POOL_CARDS)).collect();
    let mut picks = Vec::with_capacity(DRAFTED_CARDS);
    for (round, round_packs) in opened.iter().enumerate() {
        let mut in_hand: Vec<Vec<u16>> = round_packs.clone();
        for pick in 0..PACK_SIZE {
            for (seat, picker) in pickers.iter_mut().enumerate() {
                let pack = &in_hand[seat];
                let index = picker.pick(&PickView {
                    seat,
                    round,
                    pick,
                    pack,
                    picks: &pools[seat],
                });
                if index >= pack.len() {
                    return Err(CubeDraftError::PickOutOfRange {
                        seat,
                        round,
                        pick,
                        index,
                        pack_len: pack.len(),
                    });
                }
                picks.push(PickRecord {
                    seat,
                    round,
                    pick,
                    pack: pack.clone(),
                    chosen: pack[index],
                });
                let chosen = in_hand[seat].remove(index);
                pools[seat].push(chosen);
            }
            in_hand = pass_packs(in_hand, round);
        }
    }
    Ok(DraftRecord {
        schema: DRAFT_RECORD_SCHEMA,
        draft_seed,
        packs: opened,
        picks,
        pools,
    })
}

// ---------------------------------------------------------------------------
// Decks
// ---------------------------------------------------------------------------

/// One deck row: a card and its number of copies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeckEntry {
    pub id: u16,
    pub name: String,
    pub count: u32,
}

/// A deck as rows of card and count, in build order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Deck {
    pub entries: Vec<DeckEntry>,
}

impl Deck {
    /// Appends `count` copies of card `id` under its registry name.
    pub fn push(&mut self, id: u16, count: u32) {
        self.entries.push(DeckEntry {
            id,
            name: def(id).name.to_owned(),
            count,
        });
    }

    /// Total cards, or `None` if the total overflows `u32`.
    pub fn checked_card_count(&self) -> Option<u32> {
        self.entries
            .iter()
            .try_fold(0u32, |total, entry| total.checked_add(entry.count))
    }

    /// Total cards, saturating at `u32::MAX` (an overflowing deck never
    /// wraps around to a valid size).
    pub fn card_count(&self) -> u32 {
        self.checked_card_count().unwrap_or(u32::MAX)
    }

    /// Every card id, one per copy, in row order.
    pub fn card_ids(&self) -> Vec<u16> {
        self.entries
            .iter()
            .flat_map(|entry| std::iter::repeat_n(entry.id, entry.count as usize))
            .collect()
    }

    /// The `{"cards": [{"name": ..., "count": ...}, ...]}` deck the Limited
    /// session's reset accepts.
    pub fn to_session_json(&self) -> Value {
        json!({"cards": self
            .entries
            .iter()
            .map(|entry| json!({"name": entry.name, "count": entry.count}))
            .collect::<Vec<_>>()})
    }

    /// Checks the deck against the pool it was built from: exactly 40
    /// cards, only pool cards and basic lands, each non-basic card at most
    /// once, no empty rows, every row's id and name naming one registry
    /// card. Counts are added with overflow checks.
    pub fn validate(&self, pool: &[u16]) -> Result<(), CubeDraftError> {
        let basics = basic_land_ids()?;
        let in_pool: BTreeSet<u16> = pool.iter().copied().collect();
        let mut nonbasics: BTreeSet<u16> = BTreeSet::new();
        let mut count = 0u32;
        for entry in &self.entries {
            if entry.count == 0 {
                return Err(CubeDraftError::Deck(format!(
                    "zero copies of {:?}",
                    entry.name
                )));
            }
            if usize::from(entry.id) >= CARD_DEFS.len() || def(entry.id).name != entry.name {
                return Err(CubeDraftError::Deck(format!(
                    "row {:?} does not name card {}",
                    entry.name, entry.id
                )));
            }
            count = count.checked_add(entry.count).ok_or_else(count_overflow)?;
            if basics.contains(&entry.id) {
                continue;
            }
            if !in_pool.contains(&entry.id) {
                return Err(CubeDraftError::Deck(format!(
                    "{:?} is not in the pool",
                    entry.name
                )));
            }
            if entry.count > 1 || !nonbasics.insert(entry.id) {
                return Err(CubeDraftError::Deck(format!(
                    "{:?} appears more than once",
                    entry.name
                )));
            }
        }
        if count != DECK_CARDS {
            return Err(CubeDraftError::Deck(format!(
                "{count} cards, a deck has exactly {DECK_CARDS}"
            )));
        }
        Ok(())
    }
}

fn count_overflow() -> CubeDraftError {
    CubeDraftError::Deck("the card count overflows u32".to_owned())
}

/// Registry ids of the five basic lands, in [`COLOUR_ORDER`].
pub fn basic_land_ids() -> Result<[u16; 5], CubeDraftError> {
    let mut ids = [0u16; 5];
    for (slot, name) in ids.iter_mut().zip(BASIC_LAND_NAMES) {
        *slot =
            card_id_by_name(name).ok_or_else(|| CubeDraftError::UnknownCard(name.to_owned()))?;
    }
    Ok(ids)
}

/// Builds a deck from a seat's pool (card ids in pick order).
pub trait DeckBuilder {
    fn build(&mut self, pool: &[u16]) -> Deck;
}

/// Builds with `builder` and validates the result against `pool`.
pub fn build_deck(builder: &mut dyn DeckBuilder, pool: &[u16]) -> Result<Deck, CubeDraftError> {
    let deck = builder.build(pool);
    deck.validate(pool)?;
    Ok(deck)
}

/// Test fixture and reference baseline only, never an opponent or teacher in
/// training: the stated deck rule.
///
/// 1. The pair: the two colours with the most coloured cards in the pool (a
///    card counts once for each colour it has), ties broken W, U, B, R, G.
/// 2. Spells: nonland cards whose colours lie within the pair (colourless
///    included), in pick order, up to 23; if fewer than 22, off-colour
///    nonland cards in pick order until 22.
/// 3. Up to 3 nonbasic pool lands, in pick order, that produce or fetch at
///    least one of the pair's colours, or that neither produce nor fetch
///    any colour (colourless utility lands; [`land_colour_bits`] is zero).
///    A fetchland supplies the colours of the basic land types its search
///    can find ([`land_fetch_bits`]), so an off-colour fetchland is left
///    out.
/// 4. Basics of the two colours to reach 40, split in proportion to the
///    pair's coloured mana symbols in the chosen spells' costs (both halves
///    of a split card or adventure; a hybrid symbol counts for each pair
///    colour it shows), by largest remainder with ties to the first colour;
///    with no such symbols, half each, the odd basic to the first colour.
#[derive(Debug, Clone, Copy, Default)]
pub struct StatedRuleDeckBuilder;

impl StatedRuleDeckBuilder {
    /// The pair of colour indices ([`COLOUR_ORDER`]) the rule chooses.
    pub fn colour_pair(pool: &[u16]) -> [usize; 2] {
        let mut counts = [0u32; 5];
        for &id in pool {
            add_colour_counts(&mut counts, card_colour_bits(id));
        }
        top_two_colours(&counts)
    }

    /// Builds and validates the deck, or explains why it cannot.
    pub fn try_build(&self, pool: &[u16]) -> Result<Deck, CubeDraftError> {
        let basics = basic_land_ids()?;
        let [first, second] = Self::colour_pair(pool);
        let pair: u8 = (1 << first) | (1 << second);
        let nonland: Vec<u16> = pool
            .iter()
            .copied()
            .filter(|&id| !def(id).is_land)
            .collect();
        let mut spells: Vec<u16> = nonland
            .iter()
            .copied()
            .filter(|&id| card_colour_bits(id) & !pair == 0)
            .take(SPELL_TARGET_MAX)
            .collect();
        if spells.len() < SPELL_TARGET_MIN {
            let missing = SPELL_TARGET_MIN - spells.len();
            spells.extend(
                nonland
                    .iter()
                    .copied()
                    .filter(|&id| card_colour_bits(id) & !pair != 0)
                    .take(missing),
            );
        }
        let lands: Vec<u16> = pool
            .iter()
            .copied()
            .filter(|&id| def(id).is_land && !basics.contains(&id))
            .filter(|&id| {
                let supplied = land_colour_bits(id);
                supplied & pair != 0 || supplied == 0
            })
            .take(NONBASIC_LANDS_MAX)
            .collect();
        let basic_total = (DECK_CARDS as usize)
            .checked_sub(spells.len() + lands.len())
            .ok_or_else(|| CubeDraftError::Deck("more than 40 spells and lands".to_owned()))?;
        let mut symbols = [0usize; 2];
        for &id in &spells {
            for pip in printed_pips(def(id)) {
                let bits = pip_bits(pip);
                if bits & (1 << first) != 0 {
                    symbols[0] += 1;
                }
                if bits & (1 << second) != 0 {
                    symbols[1] += 1;
                }
            }
        }
        let (first_basics, second_basics) = split_largest_remainder(basic_total, symbols);
        let mut deck = Deck::default();
        for &id in spells.iter().chain(lands.iter()) {
            deck.push(id, 1);
        }
        for (colour, count) in [(first, first_basics), (second, second_basics)] {
            if count > 0 {
                deck.push(basics[colour], count as u32);
            }
        }
        deck.validate(pool)?;
        Ok(deck)
    }
}

impl DeckBuilder for StatedRuleDeckBuilder {
    /// The deck of [`StatedRuleDeckBuilder::try_build`]; when that fails (no
    /// basic land in the registry), an empty deck, which
    /// [`Deck::validate`] and [`build_deck`] refuse.
    fn build(&mut self, pool: &[u16]) -> Deck {
        self.try_build(pool).unwrap_or_default()
    }
}

/// Splits `total` in proportion to `weights` by largest remainder, ties to
/// the first part; equal weights of zero split in half, the odd one first.
fn split_largest_remainder(total: usize, weights: [usize; 2]) -> (usize, usize) {
    let sum = weights[0] + weights[1];
    if sum == 0 {
        let first = total.div_ceil(2);
        return (first, total - first);
    }
    let mut first = total * weights[0] / sum;
    let mut second = total * weights[1] / sum;
    let left = total - first - second;
    if left > 0 {
        if (total * weights[0]) % sum >= (total * weights[1]) % sum {
            first += left;
        } else {
            second += left;
        }
    }
    (first, second)
}

/// A draft with eight [`UniformPicker`]s followed by the
/// [`StatedRuleDeckBuilder`] deck of every seat. Fixtures only.
pub fn uniform_draft_and_build(
    pool: &CubePool,
    draft_seed: u64,
) -> Result<(DraftRecord, [Deck; SEATS]), CubeDraftError> {
    let mut pickers = uniform_pickers(draft_seed);
    let record = run_draft(pool, draft_seed, &mut pickers)?;
    let decks = build_stated_rule_decks(&record)?;
    Ok((record, decks))
}

/// The [`StatedRuleDeckBuilder`] deck of every seat of a draft.
pub fn build_stated_rule_decks(record: &DraftRecord) -> Result<[Deck; SEATS], CubeDraftError> {
    let decks = record
        .pools
        .iter()
        .map(|pool| StatedRuleDeckBuilder.try_build(pool))
        .collect::<Result<Vec<Deck>, CubeDraftError>>()?;
    let count = decks.len();
    decks
        .try_into()
        .map_err(|_| CubeDraftError::Deck(format!("{count} pools, not {SEATS}")))
}

// ---------------------------------------------------------------------------
// Pod play
// ---------------------------------------------------------------------------

/// Bounds of one game. The driver cap counts the driver's decisions; the
/// session caps go into the reset request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameCaps {
    pub max_decisions: u64,
    pub max_physical_decisions: u64,
    pub max_policy_steps: u64,
    pub max_stack_objects: Option<u32>,
}

impl Default for GameCaps {
    fn default() -> Self {
        GameCaps {
            max_decisions: 8000,
            max_physical_decisions: 65_536,
            max_policy_steps: 131_072,
            max_stack_objects: None,
        }
    }
}

/// How a game ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GameEnd {
    /// A natural game over; the winning seat, if the game had one.
    Natural {
        winner_seat: Option<usize>,
        reason: String,
    },
    /// Any other terminal reply (for example a session cap).
    Terminal {
        code: String,
        classification: String,
        reason: String,
    },
    /// The session answered with an error.
    SessionError { code: String, message: String },
    /// The driver's decision cap.
    DecisionCap,
    /// The engine panicked; the message.
    Panic { message: String },
}

impl GameEnd {
    /// A short name of the kind of end.
    pub fn kind(&self) -> &'static str {
        match self {
            GameEnd::Natural { .. } => "natural",
            GameEnd::Terminal { .. } => "terminal",
            GameEnd::SessionError { .. } => "session_error",
            GameEnd::DecisionCap => "decision_cap",
            GameEnd::Panic { .. } => "panic",
        }
    }
}

/// One pod game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GameRecord {
    /// Index in [`pod_pairings`].
    pub game: usize,
    /// The seats as session players: `seats[0]` plays p0, `seats[1]` p1.
    pub seats: [usize; 2],
    pub env_seed: u64,
    /// Decisions the driver made.
    pub decisions: u64,
    pub end: GameEnd,
}

impl GameRecord {
    /// The winning seat of a natural end with a winner.
    pub fn winner(&self) -> Option<usize> {
        match self.end {
            GameEnd::Natural { winner_seat, .. } => winner_seat,
            _ => None,
        }
    }
}

/// How many chosen actions back the earlier halt census looked for an action
/// with a source card (its "last action's source" column).
pub const RECENT_ACTIONS: u64 = 10;

/// What a game's driver saw besides its end, for halt censuses: the
/// session's own counts and the source cards of the actions it chose. A
/// field the game never reached stays `None`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GameTally {
    /// The terminal reply's `policy_step_count`.
    pub policy_steps: Option<u64>,
    /// Largest exposed stack in a decision or terminal reply, or zero before any observation.
    pub max_stack: u32,
    /// The terminal reply's `physical_decision_count`.
    pub physical_decisions: Option<u64>,
    /// `step` of the last decision reply.
    pub last_step: Option<u64>,
    /// `physical_decision_id` of the last decision reply.
    pub last_physical_decision_id: Option<u64>,
    /// Registry id of the source card (`semantic.source.card_db_id`) of the
    /// last action chosen, if it had one.
    pub last_action_source: Option<u16>,
    /// The source card of the latest chosen action that had one.
    latest_source: Option<u16>,
    /// Actions chosen after that one.
    actions_after_latest_source: u64,
}

impl GameTally {
    fn record_stack(&mut self, body: &Value) {
        if let Some(stack) = body["observation"]["projection"]["stack"].as_array() {
            self.max_stack = self
                .max_stack
                .max(u32::try_from(stack.len()).unwrap_or(u32::MAX));
        }
    }

    fn record_action(&mut self, action: &Value) {
        let source = action["semantic"]["source"]["card_db_id"]
            .as_u64()
            .filter(|&id| id < CARD_DEFS.len() as u64)
            .map(|id| id as u16);
        self.last_action_source = source;
        if source.is_some() {
            self.latest_source = source;
            self.actions_after_latest_source = 0;
        } else {
            self.actions_after_latest_source += 1;
        }
    }

    /// The source card of the latest of the last [`RECENT_ACTIONS`] chosen
    /// actions that had one, as the earlier census's "last action's source"
    /// column counted it.
    pub fn recent_action_source(&self) -> Option<u16> {
        self.latest_source
            .filter(|_| self.actions_after_latest_source < RECENT_ACTIONS)
    }
}

/// All games of a pod.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PodResult {
    pub pod_seed: u64,
    pub games: Vec<GameRecord>,
}

impl PodResult {
    /// Games per [`GameEnd::kind`].
    pub fn end_counts(&self) -> BTreeMap<&'static str, u64> {
        let mut counts = BTreeMap::new();
        for game in &self.games {
            *counts.entry(game.end.kind()).or_default() += 1;
        }
        counts
    }

    /// Natural wins per seat.
    pub fn wins_by_seat(&self) -> [u64; SEATS] {
        let mut wins = [0u64; SEATS];
        for seat in self.games.iter().filter_map(GameRecord::winner) {
            wins[seat] += 1;
        }
        wins
    }
}

/// The 28 seat pairs `(i, j)`, `i < j`, in lexicographic order.
pub fn pod_pairings() -> Vec<(usize, usize)> {
    (0..SEATS)
        .flat_map(|first| (first + 1..SEATS).map(move |second| (first, second)))
        .collect()
}

/// Plays every pairing once with uniform-random legal actions.
pub fn play_pod_round_robin(decks: &[Deck; SEATS], seed: u64) -> PodResult {
    play_pod_round_robin_with_caps(decks, seed, &GameCaps::default())
}

/// [`play_pod_round_robin`] with explicit caps.
pub fn play_pod_round_robin_with_caps(
    decks: &[Deck; SEATS],
    seed: u64,
    caps: &GameCaps,
) -> PodResult {
    let games = (0..POD_GAMES)
        .map(|game| play_pod_game_unchecked(decks, seed, game, caps).0)
        .collect();
    PodResult {
        pod_seed: seed,
        games,
    }
}

/// Plays game `game` of the pod with seed `pod_seed`: the pairing
/// [`pod_pairings`]`[game]`, seat order by a seeded coin, uniform-random
/// legal actions.
pub fn play_pod_game(
    decks: &[Deck; SEATS],
    pod_seed: u64,
    game: usize,
    caps: &GameCaps,
) -> Result<GameRecord, CubeDraftError> {
    play_pod_game_tallied(decks, pod_seed, game, caps).map(|(record, _)| record)
}

/// [`play_pod_game`], also returning what the driver saw ([`GameTally`]).
/// The game is the same: the tally draws no randomness.
pub fn play_pod_game_tallied(
    decks: &[Deck; SEATS],
    pod_seed: u64,
    game: usize,
    caps: &GameCaps,
) -> Result<(GameRecord, GameTally), CubeDraftError> {
    if game >= POD_GAMES {
        return Err(CubeDraftError::GameOutOfRange { game });
    }
    Ok(play_pod_game_unchecked(decks, pod_seed, game, caps))
}

fn play_pod_game_unchecked(
    decks: &[Deck; SEATS],
    pod_seed: u64,
    game: usize,
    caps: &GameCaps,
) -> (GameRecord, GameTally) {
    let seats = pod_game_seats_unchecked(pod_seed, game);
    play_seated_game(
        [&decks[seats[0]], &decks[seats[1]]],
        seats,
        game_seed(pod_seed, game),
        game,
        caps,
    )
}

/// The seats of pod game `game` as session players (p0 first): its pairing
/// ([`pod_pairings`]) in the order of the game's seeded coin.
pub fn pod_game_seats(pod_seed: u64, game: usize) -> Result<[usize; 2], CubeDraftError> {
    if game >= POD_GAMES {
        return Err(CubeDraftError::GameOutOfRange { game });
    }
    Ok(pod_game_seats_unchecked(pod_seed, game))
}

fn pod_game_seats_unchecked(pod_seed: u64, game: usize) -> [usize; 2] {
    let (low, high) = pod_pairings()[game];
    let seed = game_seed(pod_seed, game);
    let coin = SplitMix64::seed(derive_seed(seed, SeedStream::GameCoin, 0)).next_u64() & 1;
    if coin == 0 {
        [low, high]
    } else {
        [high, low]
    }
}

/// Plays `decks[0]` as session player p0 against `decks[1]` as p1 from one
/// game seed, with uniform-random legal actions. The session's `env_seed`
/// and the action stream derive from `seed` exactly as a pod game's derive
/// from its [`game_seed`]; there is no seat coin, so the seats of the record
/// are `[0, 1]` (deck indices). `episode` is the session episode id and the
/// record's `game`.
pub fn play_seeded_game_tallied(
    decks: [&Deck; 2],
    seed: u64,
    episode: usize,
    caps: &GameCaps,
) -> (GameRecord, GameTally) {
    play_seated_game(decks, [0, 1], seed, episode, caps)
}

fn play_seated_game(
    decks: [&Deck; 2],
    seats: [usize; 2],
    seed: u64,
    game: usize,
    caps: &GameCaps,
) -> (GameRecord, GameTally) {
    let env_seed = derive_seed(seed, SeedStream::GameEnv, 0);
    let action_seed = derive_seed(seed, SeedStream::GameActions, 0);
    let setup = GameSetup {
        decks,
        seats,
        episode: game as u64,
        env_seed,
        action_seed,
    };
    let mut tally = GameTally::default();
    let (decisions, end) =
        catch_game_panic(|decisions| drive_game(&setup, caps, decisions, &mut tally));
    (
        GameRecord {
            game,
            seats,
            env_seed,
            decisions,
            end,
        },
        tally,
    )
}

/// Runs one game's `play`, which counts its decisions in the counter it is
/// given, and records an unwinding panic as [`GameEnd::Panic`] with the
/// decisions made before it, instead of propagating it. Returns the
/// decision count and the end.
pub fn catch_game_panic(play: impl FnOnce(&mut u64) -> GameEnd) -> (u64, GameEnd) {
    let mut decisions = 0u64;
    let played = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| play(&mut decisions)));
    let end = match played {
        Ok(end) => end,
        Err(payload) => GameEnd::Panic {
            message: panic_message(&*payload),
        },
    };
    (decisions, end)
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else {
        "panic".to_owned()
    }
}

struct GameSetup<'a> {
    decks: [&'a Deck; 2],
    seats: [usize; 2],
    episode: u64,
    env_seed: u64,
    action_seed: u64,
}

fn text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// The session player index (0 for p0, 1 for p1) a terminal `winner` names.
fn winner_index(value: &Value) -> Option<usize> {
    match value {
        Value::String(text) => match text.as_str() {
            "p0" | "P0" => Some(0),
            "p1" | "P1" => Some(1),
            _ => None,
        },
        Value::Number(number) => number.as_u64().filter(|&n| n < 2).map(|n| n as usize),
        _ => None,
    }
}

fn terminal_end(terminal: &Value, seats: [usize; 2]) -> GameEnd {
    let code = text(&terminal["terminal_code"]);
    if code == "natural_game_over" {
        GameEnd::Natural {
            winner_seat: winner_index(&terminal["winner"]).map(|player| seats[player]),
            reason: text(&terminal["terminal_reason"]),
        }
    } else {
        GameEnd::Terminal {
            code,
            classification: text(&terminal["terminal_classification"]),
            reason: text(&terminal["terminal_reason"]),
        }
    }
}

thread_local! {
    static GAME_DECISIONS: Arc<AtomicU64> = Arc::default();
}

/// The calling game thread's driver decision count, for the census heartbeat.
pub fn game_decision_counter() -> Arc<AtomicU64> {
    GAME_DECISIONS.with(Arc::clone)
}

fn reset_request(setup: &GameSetup<'_>, caps: &GameCaps) -> String {
    let mut request = json!({"schema_version": SESSION_SCHEMA_VERSION, "request_type": "reset",
        "request_id": "reset",
        "decks": [setup.decks[0].to_session_json(), setup.decks[1].to_session_json()],
        "episode_id": setup.episode, "env_seed": setup.env_seed,
        "max_physical_decisions": caps.max_physical_decisions,
        "max_policy_steps": caps.max_policy_steps});
    if let Some(limit) = caps.max_stack_objects {
        request["max_stack_objects"] = json!(limit);
    }
    request.to_string()
}

fn drive_game(
    setup: &GameSetup<'_>,
    caps: &GameCaps,
    decisions: &mut u64,
    tally: &mut GameTally,
) -> GameEnd {
    let observed = game_decision_counter();
    observed.store(0, Ordering::Relaxed);
    let mut rng = SplitMix64::seed(setup.action_seed);
    let mut server = LimitedJsonlServerV1::new_with_public_shroud_v1();
    let mut request = reset_request(setup, caps);
    loop {
        let raw = server.handle_line(&request);
        let reply: Value = match serde_json::from_str(&raw) {
            Ok(reply) => reply,
            Err(error) => {
                return GameEnd::SessionError {
                    code: "unparseable_reply".to_owned(),
                    message: error.to_string(),
                };
            }
        };
        match reply["response_type"].as_str() {
            Some("error") => {
                return GameEnd::SessionError {
                    code: text(&reply["error"]["code"]),
                    message: text(&reply["error"]["message"]),
                };
            }
            Some("terminal") => {
                let terminal = &reply["terminal"];
                tally.record_stack(terminal);
                tally.policy_steps = terminal["policy_step_count"].as_u64();
                tally.physical_decisions = terminal["physical_decision_count"].as_u64();
                return terminal_end(terminal, setup.seats);
            }
            _ => {}
        }
        let decision = &reply["decision"];
        tally.record_stack(decision);
        if let Some(step) = decision["step"].as_u64() {
            tally.last_step = Some(step);
        }
        if let Some(id) = decision["physical_decision_id"].as_u64() {
            tally.last_physical_decision_id = Some(id);
        }
        let actions = match decision["legal_actions"].as_array() {
            Some(actions) if !actions.is_empty() => actions,
            _ => {
                return GameEnd::SessionError {
                    code: "no_legal_actions".to_owned(),
                    message: format!("decision without legal actions: {}", reply["response_type"]),
                };
            }
        };
        if *decisions >= caps.max_decisions {
            return GameEnd::DecisionCap;
        }
        let action = &actions[uniform_index(&mut rng, actions.len())];
        *decisions += 1;
        observed.store(*decisions, Ordering::Relaxed);
        tally.record_action(action);
        let step = *decisions;
        request = json!({"schema_version": SESSION_SCHEMA_VERSION, "request_type": "step",
            "request_id": format!("step-{step}"),
            "episode_id": decision["episode_id"], "expected_step": decision["step"],
            "selected_index": action["selected_index"],
            "selected_action_id": action["stable_id"]})
        .to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_bytes_with_and_without_a_stack_cap() {
        let mut deck = Deck::default();
        deck.push(card_id_by_name("Forest").unwrap(), 40);
        let setup = GameSetup {
            decks: [&deck, &deck],
            seats: [0, 1],
            episode: 2,
            env_seed: 42,
            action_seed: 0,
        };
        let legacy = concat!(
            r#"{"decks":[{"cards":[{"count":40,"name":"Forest"}]},"#,
            r#"{"cards":[{"count":40,"name":"Forest"}]}],"env_seed":42,"episode_id":2,"#,
            r#""max_physical_decisions":65536,"max_policy_steps":131072,"#,
            r#""request_id":"reset","request_type":"reset","schema_version":5}"#,
        );
        let mut caps = GameCaps::default();
        assert_eq!(reset_request(&setup, &caps), legacy);
        caps.max_stack_objects = Some(7);
        assert_eq!(
            reset_request(&setup, &caps),
            legacy.replace(r#""request_id""#, r#""max_stack_objects":7,"request_id""#)
        );
    }
}
