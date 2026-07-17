//! Codegen: `data/cards_v1.json` -> `$OUT_DIR/card_defs.rs`, included
//! verbatim by `src/card_def.rs`.
//!
//! Fails the build on:
//! - schema version mismatch (`cards_v1.json`'s `"version"` != what this
//!   codegen understands)
//! - duplicate card names
//! - a card with empty deck coverage (`"decks": []`)
//!
//! `u16` ids are assigned in JSON array order (stable: the file is a
//! checked-in fixed pool, not regenerated per build). Executability and
//! full-support status come from each record's fail-closed
//! `engine_capability`; ordinary supported permanents and intrinsic basic-
//! land mana are generated from metadata, while exceptional rules text is
//! still composed explicitly below.

use serde::Deserialize;
use std::collections::HashSet;
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

const EXPECTED_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Deserialize)]
struct CardJson {
    name: String,
    /// Absent means `no_effect`: adding a registry record can never make a
    /// card playable accidentally. `partial` is executable but is rejected
    /// by the full-deck preflight; `full` is both executable and accepted.
    #[serde(default)]
    engine_capability: EngineCapabilityJson,
    mana_cost: String,
    #[serde(default)]
    types: Vec<String>,
    #[serde(default)]
    subtypes: Vec<String>,
    #[serde(default)]
    supertypes: Vec<String>,
    power: Option<i32>,
    toughness: Option<i32>,
    is_land: bool,
    #[serde(default)]
    produces_mana: Vec<String>,
    #[serde(default)]
    colors: Vec<String>,
    decks: Vec<String>,
    /// A permanent token (e.g. Blood), not itself a deck card: exempt from
    /// the empty-deck-coverage check (see `main`'s validation loop) and
    /// never castable (its `Special` falls through to `Special::None`,
    /// giving it `no_effect`/`no_effect`/`TargetSpec::None` -- correct,
    /// since tokens are never cast, only created by another card's effect).
    #[serde(default)]
    is_token: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
enum EngineCapabilityJson {
    #[default]
    NoEffect,
    Partial,
    Full,
}

#[derive(Debug, Deserialize)]
struct CardsFile {
    version: u32,
    cards: Vec<CardJson>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeDeckCatalogJson {
    schema: String,
    protocol: String,
    source_hash_normalization: String,
    materialization: RuntimeDeckMaterializationJson,
    card_ids: RuntimeDeckCardIdsJson,
    decks: Vec<RuntimeDeckJson>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeDeckMaterializationJson {
    order: String,
    source_row_ordinal_base: u32,
    copy_ordinal_base: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeDeckCardIdsJson {
    assignment: String,
    deck_hash_algorithm: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeDeckJson {
    canonical_pool_order: u32,
    id: String,
    source_path: String,
    source_sha256: String,
    mainboard_copy_count: usize,
    unique_mainboard_cards: usize,
    runtime_deck_hash: String,
    materialized_mainboard: Vec<RuntimeDeckCopyJson>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeDeckCopyJson {
    source_row_ordinal: u32,
    copy_ordinal: u32,
    name: String,
    card_id: u16,
}

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR set by cargo");
    let json_path = Path::new(&manifest_dir)
        .join("..")
        .join("data")
        .join("cards_v1.json");
    let runtime_decks_path = Path::new(&manifest_dir)
        .join("..")
        .join("data")
        .join("runtime_decks_v1.json");
    println!("cargo:rerun-if-changed={}", json_path.display());
    println!("cargo:rerun-if-changed={}", runtime_decks_path.display());
    println!("cargo:rerun-if-changed=build.rs");

    let text = fs::read_to_string(&json_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", json_path.display()));
    let data: CardsFile = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("failed to parse {}: {e}", json_path.display()));

    if data.version != EXPECTED_SCHEMA_VERSION {
        panic!(
            "cards_v1.json schema version mismatch: codegen expects version {EXPECTED_SCHEMA_VERSION}, \
             file has version {}. Update build.rs's CardJson/codegen for the new schema before building.",
            data.version
        );
    }

    let mut seen_names = HashSet::new();
    for c in &data.cards {
        if !seen_names.insert(c.name.clone()) {
            panic!("cards_v1.json: duplicate card name {:?}", c.name);
        }
        if c.decks.is_empty() && !c.is_token {
            panic!("cards_v1.json: card {:?} has empty deck coverage", c.name);
        }
        if c.is_token && c.engine_capability != EngineCapabilityJson::Full {
            panic!(
                "cards_v1.json: token {:?} must be explicitly full so CreateToken cannot materialize unsupported behavior",
                c.name
            );
        }
    }

    let runtime_decks_text = fs::read_to_string(&runtime_decks_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", runtime_decks_path.display()));
    let runtime_decks: RuntimeDeckCatalogJson = serde_json::from_str(&runtime_decks_text)
        .unwrap_or_else(|e| panic!("failed to parse {}: {e}", runtime_decks_path.display()));

    let out = codegen(&data.cards);
    let runtime_decks_out = runtime_decks_codegen(&runtime_decks, &data.cards);

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR set by cargo");
    let dest = Path::new(&out_dir).join("card_defs.rs");
    fs::write(&dest, out).unwrap_or_else(|e| panic!("failed to write {}: {e}", dest.display()));
    let runtime_decks_dest = Path::new(&out_dir).join("runtime_decks.rs");
    fs::write(&runtime_decks_dest, runtime_decks_out)
        .unwrap_or_else(|e| panic!("failed to write {}: {e}", runtime_decks_dest.display()));
}

fn runtime_decks_codegen(catalog: &RuntimeDeckCatalogJson, cards: &[CardJson]) -> String {
    const EXPECTED_SCHEMA: &str = "kernel_runtime_decks/v1";
    const EXPECTED_PROTOCOL: &str = "canonical-mainboard-bo1/v1";
    const EXPECTED_SOURCE_HASH_NORMALIZATION: &str = "utf8_text_crlf_v1";
    const EXPECTED_MATERIALIZATION: &str = "xmage_xml_row_then_copy_ordinal/v1";
    const EXPECTED_CARD_ID_ASSIGNMENT: &str = "zero_based_data_cards_v1_json_cards_array_index/v1";
    const EXPECTED_DECK_HASH_ALGORITHM: &str = "fnv1a64-serde-json-u16-array/v1";
    const EXPECTED_DECKS: [(&str, u32, &str, &str, u64); 2] = [
        (
            "Rally",
            2,
            "oracle/xmage/decks/Pauper/Deck - Mono Red Rally.dek",
            "4b5019bd08f9387aeabebdca0d90aaa10dfd75fc75ed3a87c95a2fabf4dba834",
            0x0c9f01c2544412bf,
        ),
        (
            "Burn",
            6,
            "oracle/xmage/decks/Pauper/Deck - Mono-Red Burn.dek",
            "4ebba6b42bb27a0ea55001cee133aada81f0dffd8661b46b012fc5026675aa32",
            0x5fdb7b92986b6fc1,
        ),
    ];

    if catalog.schema != EXPECTED_SCHEMA {
        panic!(
            "runtime_decks_v1.json: schema mismatch: expected {EXPECTED_SCHEMA:?}, got {:?}",
            catalog.schema
        );
    }
    if catalog.protocol != EXPECTED_PROTOCOL {
        panic!(
            "runtime_decks_v1.json: protocol mismatch: expected {EXPECTED_PROTOCOL:?}, got {:?}",
            catalog.protocol
        );
    }
    if catalog.source_hash_normalization != EXPECTED_SOURCE_HASH_NORMALIZATION {
        panic!(
            "runtime_decks_v1.json: source hash normalization mismatch: expected {EXPECTED_SOURCE_HASH_NORMALIZATION:?}, got {:?}",
            catalog.source_hash_normalization
        );
    }
    if catalog.materialization.order != EXPECTED_MATERIALIZATION
        || catalog.materialization.source_row_ordinal_base != 1
        || catalog.materialization.copy_ordinal_base != 1
    {
        panic!(
            "runtime_decks_v1.json: unsupported materialization contract {:?} with row/copy bases {}/{}",
            catalog.materialization.order,
            catalog.materialization.source_row_ordinal_base,
            catalog.materialization.copy_ordinal_base
        );
    }
    if catalog.card_ids.assignment != EXPECTED_CARD_ID_ASSIGNMENT
        || catalog.card_ids.deck_hash_algorithm != EXPECTED_DECK_HASH_ALGORITHM
    {
        panic!(
            "runtime_decks_v1.json: unsupported card-id/hash contract {:?} / {:?}",
            catalog.card_ids.assignment, catalog.card_ids.deck_hash_algorithm
        );
    }
    if catalog.decks.len() != EXPECTED_DECKS.len() {
        panic!(
            "runtime_decks_v1.json: expected exactly {} runnable decks, got {}",
            EXPECTED_DECKS.len(),
            catalog.decks.len()
        );
    }

    let mut generated_decks = Vec::with_capacity(catalog.decks.len());
    let mut seen_ids = HashSet::new();
    for (deck, &(expected_id, expected_pool_order, expected_path, expected_sha, expected_hash)) in
        catalog.decks.iter().zip(EXPECTED_DECKS.iter())
    {
        if !seen_ids.insert(deck.id.as_str()) {
            panic!("runtime_decks_v1.json: duplicate deck id {:?}", deck.id);
        }
        if deck.id != expected_id
            || deck.canonical_pool_order != expected_pool_order
            || deck.source_path != expected_path
            || deck.source_sha256 != expected_sha
        {
            panic!(
                "runtime_decks_v1.json: deck slot for {expected_id:?} does not match its frozen id/order/source provenance"
            );
        }
        if deck.source_sha256.len() != 64
            || !deck
                .source_sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            panic!(
                "runtime_decks_v1.json: deck {:?} source_sha256 is not 64 lowercase hexadecimal characters",
                deck.id
            );
        }
        if deck.mainboard_copy_count != 60
            || deck.materialized_mainboard.len() != deck.mainboard_copy_count
        {
            panic!(
                "runtime_decks_v1.json: deck {:?} must materialize exactly 60 mainboard copies, declared {}, found {}",
                deck.id,
                deck.mainboard_copy_count,
                deck.materialized_mainboard.len()
            );
        }

        let mut last_row = 0u32;
        let mut expected_copy_ordinal = 0u32;
        let mut current_row_identity: Option<(&str, u16)> = None;
        let mut unique_names = HashSet::new();
        let mut card_ids = Vec::with_capacity(deck.mainboard_copy_count);
        for (materialized_index, copy) in deck.materialized_mainboard.iter().enumerate() {
            if copy.source_row_ordinal == 0 || copy.copy_ordinal == 0 {
                panic!(
                    "runtime_decks_v1.json: deck {:?} materialized copy {materialized_index} uses a zero ordinal",
                    deck.id
                );
            }
            if copy.source_row_ordinal == last_row {
                expected_copy_ordinal += 1;
                let (row_name, row_card_id) = current_row_identity
                    .expect("a repeated materialized row has a first copy identity");
                if copy.name != row_name || copy.card_id != row_card_id {
                    panic!(
                        "runtime_decks_v1.json: deck {:?} row {} changes card identity from {:?}/{} to {:?}/{}",
                        deck.id,
                        copy.source_row_ordinal,
                        row_name,
                        row_card_id,
                        copy.name,
                        copy.card_id
                    );
                }
            } else {
                if copy.source_row_ordinal <= last_row {
                    panic!(
                        "runtime_decks_v1.json: deck {:?} materialized rows are not strictly ordered at copy {materialized_index}",
                        deck.id
                    );
                }
                last_row = copy.source_row_ordinal;
                expected_copy_ordinal = 1;
                current_row_identity = Some((copy.name.as_str(), copy.card_id));
            }
            if copy.copy_ordinal != expected_copy_ordinal {
                panic!(
                    "runtime_decks_v1.json: deck {:?} row {} copy ordinal {}, expected {}",
                    deck.id, copy.source_row_ordinal, copy.copy_ordinal, expected_copy_ordinal
                );
            }

            let card_index = usize::from(copy.card_id);
            let Some(card) = cards.get(card_index) else {
                panic!(
                    "runtime_decks_v1.json: deck {:?} materialized copy {materialized_index} references unknown card id {}",
                    deck.id, copy.card_id
                );
            };
            if card.name != copy.name {
                panic!(
                    "runtime_decks_v1.json: deck {:?} materialized copy {materialized_index} card id {} resolves to {:?}, not {:?}",
                    deck.id, copy.card_id, card.name, copy.name
                );
            }
            if card.is_token || card.engine_capability != EngineCapabilityJson::Full {
                panic!(
                    "runtime_decks_v1.json: deck {:?} materialized copy {materialized_index} ({:?}) is not a full, non-token card definition",
                    deck.id, copy.name
                );
            }
            unique_names.insert(copy.name.as_str());
            card_ids.push(copy.card_id);
        }
        if unique_names.len() != deck.unique_mainboard_cards {
            panic!(
                "runtime_decks_v1.json: deck {:?} declares {} unique mainboard cards, materialization has {}",
                deck.id,
                deck.unique_mainboard_cards,
                unique_names.len()
            );
        }

        let declared_hash = parse_runtime_deck_hash(&deck.id, &deck.runtime_deck_hash);
        let serialized_ids = serde_json::to_vec(&card_ids)
            .expect("runtime deck card-id sequence serializes as JSON");
        let computed_hash = fnv1a64(&serialized_ids);
        if declared_hash != expected_hash || computed_hash != expected_hash {
            panic!(
                "runtime_decks_v1.json: deck {:?} hash mismatch: declared {declared_hash:#018x}, computed {computed_hash:#018x}, expected {expected_hash:#018x}",
                deck.id
            );
        }
        generated_decks.push((deck, card_ids, declared_hash));
    }

    let mut out = String::new();
    writeln!(
        out,
        "// GENERATED by build.rs from data/runtime_decks_v1.json. Do not edit by hand."
    )
    .unwrap();
    writeln!(out).unwrap();
    for (name, value) in [
        ("RUNTIME_DECK_CATALOG_SCHEMA", catalog.schema.as_str()),
        ("RUNTIME_DECK_PROTOCOL", catalog.protocol.as_str()),
        (
            "RUNTIME_DECK_SOURCE_HASH_NORMALIZATION",
            catalog.source_hash_normalization.as_str(),
        ),
        (
            "RUNTIME_DECK_MATERIALIZATION_PROTOCOL",
            catalog.materialization.order.as_str(),
        ),
        (
            "RUNTIME_CARD_ID_ASSIGNMENT",
            catalog.card_ids.assignment.as_str(),
        ),
        (
            "RUNTIME_DECK_HASH_ALGORITHM",
            catalog.card_ids.deck_hash_algorithm.as_str(),
        ),
    ] {
        writeln!(out, "pub const {name}: &str = {value:?};").unwrap();
    }
    writeln!(
        out,
        "pub const RUNTIME_DECK_SOURCE_ROW_ORDINAL_BASE: u32 = {};",
        catalog.materialization.source_row_ordinal_base
    )
    .unwrap();
    writeln!(
        out,
        "pub const RUNTIME_DECK_COPY_ORDINAL_BASE: u32 = {};",
        catalog.materialization.copy_ordinal_base
    )
    .unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "pub static RUNTIME_DECKS: &[RuntimeDeckDefinition] = &["
    )
    .unwrap();
    for (deck, card_ids, runtime_deck_hash) in generated_decks {
        writeln!(out, "    RuntimeDeckDefinition {{").unwrap();
        writeln!(
            out,
            "        canonical_pool_order: {},",
            deck.canonical_pool_order
        )
        .unwrap();
        writeln!(out, "        id: {:?},", deck.id).unwrap();
        writeln!(out, "        source_path: {:?},", deck.source_path).unwrap();
        writeln!(out, "        source_sha256: {:?},", deck.source_sha256).unwrap();
        writeln!(
            out,
            "        mainboard_count: {},",
            deck.mainboard_copy_count
        )
        .unwrap();
        writeln!(out, "        runtime_deck_hash: {runtime_deck_hash:#018x},").unwrap();
        write!(out, "        card_ids: &[").unwrap();
        for (index, card_id) in card_ids.iter().enumerate() {
            if index != 0 {
                write!(out, ", ").unwrap();
            }
            write!(out, "{card_id}u16").unwrap();
        }
        writeln!(out, "],").unwrap();
        writeln!(out, "    }},").unwrap();
    }
    writeln!(out, "];").unwrap();
    out
}

fn parse_runtime_deck_hash(deck_id: &str, value: &str) -> u64 {
    let Some(digits) = value.strip_prefix("0x") else {
        panic!("runtime_decks_v1.json: deck {deck_id:?} hash must start with 0x");
    };
    if digits.len() != 16
        || !digits
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        panic!(
            "runtime_decks_v1.json: deck {deck_id:?} hash must contain exactly 16 lowercase hexadecimal digits"
        );
    }
    u64::from_str_radix(digits, 16)
        .unwrap_or_else(|_| panic!("runtime_decks_v1.json: deck {deck_id:?} hash is invalid"))
}

/// The Mono-Red Burn cards that get a real effect program. Relic of
/// Progenitus is the sole remaining deferred card -- present in `CARD_DEFS`
/// with correct metadata, not castable, per the kernel's fail-closed
/// invariant -- graveyard-card targeting doesn't fit any `TargetSpec` shape
/// built so far and it's sideboard-only, so it's lower priority than the 5
/// cards this increment adds.
#[derive(Clone, Copy)]
enum Special {
    None,
    /// Great Furnace's explicit `{T}: Add {R}` program. Basic-land mana is
    /// not a name special: it is derived from Basic + Land + one
    /// `produces_mana` color in `codegen`.
    GreatFurnace,
    /// A plain "draw N cards" spell. Lorien Revealed is the first consumer;
    /// keeping the generated recipe parameterized avoids a runtime card-name
    /// special while leaving room for later draw spells to share it.
    DrawCards(u8),
    /// Deals `amount` damage to any target (Lightning Bolt, Fiery Temper,
    /// Fireblast, Lava Dart). Fireblast's/Lava Dart's real alt cost /
    /// flashback, and Fiery Temper's Madness, are modeled via the separate
    /// `alt_cost_for`/`flashback_for`/`madness_cost_for` tables below
    /// (independent of `Special`, since a card's targeting/damage shape and
    /// its cost shape are orthogonal).
    BurnAnyTarget(i32),
    /// "Deals 3 damage to any target. Then that player or that permanent's
    /// controller may pay {R}{R}. If the player does, they may copy this
    /// spell and may choose a new target for that copy." (Chain Lightning,
    /// Rally-only). The mandatory damage is byte-for-byte
    /// `BurnAnyTarget(3)`'s shape; the optional-copy continuation suspends
    /// resolution in the engine's dedicated payment/copy/retarget state
    /// machine -- see `effect::EffectOp::OfferAffectedPlayerSpellCopy`.
    ChainLightning,
    /// Target player draws `draw` cards. Deep Analysis is the first consumer;
    /// `PlayerRef::Target(0)` keeps self- and opponent-targeting on the same
    /// generic effect path.
    TargetPlayerDraw {
        draw: u8,
    },
    /// "Draw `draw` cards, then discard `discard` cards" (Faithless
    /// Looting: 2 and 2). The discard is a resolution effect (not a cost),
    /// so it's `EffectOp::DiscardCards`, staged via
    /// `engine::EngineState::pending_discard` same as cleanup.
    DrawThenDiscard {
        draw: i32,
        discard: i32,
    },
    /// Grab the Prize: draw two cards, then (if the card discarded to pay
    /// the mandatory additional cost -- see `additional_cost_for` --
    /// wasn't a land) deal 2 damage to the opponent.
    GrabThePrize,
    /// "You may discard a card or sacrifice a land. If you do, draw two
    /// cards." (Highway Robbery's resolution effect; its Plot ability is
    /// modeled separately via `plot_cost_for`, since Plot is a cast-time
    /// alternative, not a resolution effect).
    HighwayRobbery,
    /// 1 damage to target player/planeswalker's controller and 1 damage to
    /// a creature that player controls; 3/3 instead with landfall this
    /// turn (Searing Blaze).
    SearingBlaze,
    /// Counter target spell, with the target pool selected independently
    /// from the shared generated counter effect. Counterspell accepts any
    /// spell; Dispel accepts only instant spells.
    CounterTarget(StackSpellFilter),
    /// Symmetric Elemental Blast recipe. `checked_color` is the color the
    /// target must have; `filter_timing` distinguishes the Elemental Blasts'
    /// targeting restriction from Pyroblast/Hydroblast's resolution-time
    /// "if it's [color]" check without introducing card-named runtime logic.
    ColorBlast {
        checked_color: BlastColor,
        filter_timing: BlastFilterTiming,
    },
    /// "Deals 1 damage to each opponent and each creature and planeswalker
    /// they control." (End the Festivities, Rally-only). No planeswalker
    /// card exists in this 132-card pool, so the planeswalker half of the
    /// text is vacuously satisfied by construction, not modeled separately
    /// -- see `EffectOp::DamageOpponentAndTheirCreatures`.
    EndTheFestivities,
    /// "Deals 2 damage to any target. Metalcraft -- 4 instead if you
    /// control three or more artifacts." (Galvanic Blast, Rally-only).
    GalvanicBlast,
    /// "Create two 1/1 white Human Soldier creature tokens. Humans you
    /// control gain haste until end of turn." (Rally at the Hornburg,
    /// Rally-only -- the card the deck is named for).
    RallyAtTheHornburg,
    /// "Exile the top two cards of your library. Until the end of your next
    /// turn, you may play those cards." (Reckless Impulse, Rally-only).
    RecklessImpulse,
    /// "Choose creature or land. Reveal the top four cards of your
    /// library. Put all cards of the chosen type revealed this way into
    /// your hand and the rest into your graveyard." The choice happens
    /// during resolution, so it is the first real consumer of the generic
    /// resumable `EffectOp::Choice` interpreter.
    WindingWay,
    /// Mill `mill` cards from `player`'s library, then draw `draw` cards.
    /// Mental Note mills its controller without targeting; Thought Scour
    /// mills its chosen player in target slot zero. Both use the same
    /// generated recipe so the runtime behavior is not card-name-specific.
    MillThenDraw {
        player: MillPlayer,
        mill: u8,
        draw: u8,
    },
    /// Privately look at and reorder the top `look` cards of the controller's
    /// library, optionally shuffle that library, then draw `draw` cards.
    /// Ponder is the first consumer; keeping this as a parameterized recipe
    /// prevents card-name behavior from leaking into the runtime engine.
    LookReorderMayShuffleThenDraw {
        look: u8,
        draw: u8,
    },
    /// Draw `draw` cards, then put up to `put` cards from the controller's
    /// hand on top of their library through sequential private choices.
    /// Brainstorm is the first consumer; the engine owns the generic hand and
    /// library semantics rather than branching on the printed card name.
    DrawThenPutHandOnLibraryTop {
        draw: u8,
        put: u8,
    },
    /// Privately scry `scry`, then draw `draw` cards. Preordain is the first
    /// consumer; the engine owns subset selection, both ordering directions,
    /// hidden information, and the atomic final library transition.
    ScryThenDraw {
        scry: u8,
        draw: u8,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum MillPlayer {
    Controller,
    Target0,
}

#[derive(Clone, Copy)]
enum StackSpellFilter {
    Any,
    Instant,
}

impl MillPlayer {
    fn canonical_token(self) -> &'static str {
        match self {
            MillPlayer::Controller => "controller",
            MillPlayer::Target0 => "target0",
        }
    }
}

impl StackSpellFilter {
    fn canonical_token(self) -> &'static str {
        match self {
            StackSpellFilter::Any => "any",
            StackSpellFilter::Instant => "instant",
        }
    }
}

#[derive(Clone, Copy)]
enum BlastColor {
    Blue,
    Red,
}

impl BlastColor {
    fn mana_variant(self) -> &'static str {
        match self {
            BlastColor::Blue => "ManaColor::U",
            BlastColor::Red => "ManaColor::R",
        }
    }

    fn suffix(self) -> &'static str {
        match self {
            BlastColor::Blue => "blue",
            BlastColor::Red => "red",
        }
    }

    fn symbol(self) -> &'static str {
        match self {
            BlastColor::Blue => "U",
            BlastColor::Red => "R",
        }
    }

    fn filtered_spell_target_spec(self) -> &'static str {
        match self {
            BlastColor::Blue => "TargetSpec::BlueSpellOnStack",
            BlastColor::Red => "TargetSpec::RedSpellOnStack",
        }
    }

    fn filtered_permanent_target_spec(self) -> &'static str {
        match self {
            BlastColor::Blue => "TargetSpec::BluePermanent",
            BlastColor::Red => "TargetSpec::RedPermanent",
        }
    }
}

#[derive(Clone, Copy)]
enum BlastFilterTiming {
    Targeting,
    Resolution,
}

impl BlastFilterTiming {
    fn canonical_token(self) -> &'static str {
        match self {
            BlastFilterTiming::Targeting => "targeting",
            BlastFilterTiming::Resolution => "resolution",
        }
    }
}

impl Special {
    /// Stable semantic token derived from the same recipe enum that drives
    /// code generation. This deliberately avoids hashing emitted Rust text:
    /// formatting-only codegen edits must not churn the card-database hash.
    fn canonical_token(self) -> String {
        match self {
            Special::None => "none".to_string(),
            Special::GreatFurnace => "great_furnace:add_r".to_string(),
            Special::DrawCards(count) => format!("draw_cards:{count}"),
            Special::BurnAnyTarget(amount) => format!("burn_any_target:{amount}"),
            Special::ChainLightning => "chain_lightning".to_string(),
            Special::TargetPlayerDraw { draw } => format!("target_player_draw:{draw}"),
            Special::DrawThenDiscard { draw, discard } => {
                format!("draw_then_discard:{draw}:{discard}")
            }
            Special::GrabThePrize => "grab_the_prize".to_string(),
            Special::HighwayRobbery => "highway_robbery".to_string(),
            Special::SearingBlaze => "searing_blaze".to_string(),
            Special::CounterTarget(filter) => {
                format!("counter_target:{}", filter.canonical_token())
            }
            Special::ColorBlast {
                checked_color,
                filter_timing,
            } => format!(
                "color_blast:{}:{}",
                checked_color.suffix(),
                filter_timing.canonical_token()
            ),
            Special::EndTheFestivities => "end_the_festivities".to_string(),
            Special::GalvanicBlast => "galvanic_blast".to_string(),
            Special::RallyAtTheHornburg => "rally_at_the_hornburg".to_string(),
            Special::RecklessImpulse => "reckless_impulse".to_string(),
            Special::WindingWay => "winding_way".to_string(),
            Special::MillThenDraw { player, mill, draw } => {
                format!("mill_then_draw:{}:{mill}:{draw}", player.canonical_token())
            }
            Special::LookReorderMayShuffleThenDraw { look, draw } => {
                format!("look_reorder_may_shuffle_then_draw:{look}:{draw}")
            }
            Special::DrawThenPutHandOnLibraryTop { draw, put } => {
                format!("draw_then_put_hand_on_library_top:{draw}:{put}")
            }
            Special::ScryThenDraw { scry, draw } => {
                format!("scry_then_draw:{scry}:{draw}")
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AbilityCostRecipe {
    Mana {
        colored: Option<&'static str>,
        generic: u8,
    },
    Tap,
    DiscardCards(u8),
    DiscardSelf,
    SacrificeSelf,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AbilityEffectRecipe {
    DrawCards(u8),
    CreateToken(&'static str),
    /// The interpreter currently supports exactly this typecycling search
    /// contract. Keeping all semantic knobs in the recipe makes codegen fail
    /// closed if a future caller asks for a different cardinality/reveal/
    /// shuffle shape without first extending `EffectOp`.
    SearchLibraryToHand {
        card_type: &'static str,
        subtype: &'static str,
        min_targets: u8,
        max_targets: u8,
        reveal_selected: bool,
        shuffle: bool,
    },
}

#[derive(Clone, Copy)]
struct ActivatedAbilityRecipe {
    cost: &'static [AbilityCostRecipe],
    effect: AbilityEffectRecipe,
    activation_zone: &'static str,
    sorcery_speed_only: bool,
}

fn special_for(name: &str) -> Special {
    match name {
        // Great Furnace is intentionally explicit: unlike a basic land, its
        // mana ability is rules text, not intrinsic to a basic land type.
        "Great Furnace" => Special::GreatFurnace,
        "Lorien Revealed" => Special::DrawCards(3),
        "Lightning Bolt" => Special::BurnAnyTarget(3),
        "Fiery Temper" => Special::BurnAnyTarget(3),
        "Fireblast" => Special::BurnAnyTarget(4),
        "Lava Dart" => Special::BurnAnyTarget(1),
        "Chain Lightning" => Special::ChainLightning,
        "Deep Analysis" => Special::TargetPlayerDraw { draw: 2 },
        "Faithless Looting" => Special::DrawThenDiscard {
            draw: 2,
            discard: 2,
        },
        "Grab the Prize" => Special::GrabThePrize,
        "Highway Robbery" => Special::HighwayRobbery,
        "Searing Blaze" => Special::SearingBlaze,
        "Counterspell" => Special::CounterTarget(StackSpellFilter::Any),
        "Dispel" => Special::CounterTarget(StackSpellFilter::Instant),
        "Blue Elemental Blast" => Special::ColorBlast {
            checked_color: BlastColor::Red,
            filter_timing: BlastFilterTiming::Targeting,
        },
        "Hydroblast" => Special::ColorBlast {
            checked_color: BlastColor::Red,
            filter_timing: BlastFilterTiming::Resolution,
        },
        "Pyroblast" => Special::ColorBlast {
            checked_color: BlastColor::Blue,
            filter_timing: BlastFilterTiming::Resolution,
        },
        "Red Elemental Blast" => Special::ColorBlast {
            checked_color: BlastColor::Blue,
            filter_timing: BlastFilterTiming::Targeting,
        },
        "End the Festivities" => Special::EndTheFestivities,
        "Galvanic Blast" => Special::GalvanicBlast,
        "Rally at the Hornburg" => Special::RallyAtTheHornburg,
        "Reckless Impulse" => Special::RecklessImpulse,
        "Winding Way" => Special::WindingWay,
        "Mental Note" => Special::MillThenDraw {
            player: MillPlayer::Controller,
            mill: 2,
            draw: 1,
        },
        "Thought Scour" => Special::MillThenDraw {
            player: MillPlayer::Target0,
            mill: 2,
            draw: 1,
        },
        "Ponder" => Special::LookReorderMayShuffleThenDraw { look: 3, draw: 1 },
        "Brainstorm" => Special::DrawThenPutHandOnLibraryTop { draw: 3, put: 2 },
        "Preordain" => Special::ScryThenDraw { scry: 2, draw: 1 },
        _ => Special::None,
    }
}

/// Stable, gameplay-semantic description of the generated spell target,
/// effect, and mana-ability recipe selected for a card. The card-database hash
/// includes this alongside every other generated gameplay selector below.
/// Runtime implementation changes remain source/version gated; these tokens
/// bind the per-card program selection that was previously absent from the
/// data hash.
fn effect_recipe_for(card: &CardJson) -> String {
    match special_for(&card.name) {
        Special::None => {
            let executable = card.engine_capability != EngineCapabilityJson::NoEffect;
            let spell = if executable && is_ordinary_permanent(card) {
                "MoveObject(Battlefield)"
            } else {
                "None"
            };
            let mana = if executable {
                intrinsic_basic_mana_color(card)
                    .map(|color| format!("AddMana({color})"))
                    .unwrap_or_else(|| "None".to_string())
            } else {
                "None".to_string()
            };
            format!("target=None;spell={spell};mana={mana}")
        }
        Special::GreatFurnace => "target=None;spell=None;mana=AddMana(R)".to_string(),
        Special::DrawCards(count) => {
            format!("target=None;spell=DrawCards(Controller,{count});mana=None")
        }
        Special::BurnAnyTarget(amount) => {
            format!("target=AnyTarget;spell=DealDamage({amount});mana=None")
        }
        Special::ChainLightning => "target=AnyTarget;spell=ChainLightning;mana=None".to_string(),
        Special::TargetPlayerDraw { draw } => {
            format!("target=AnyPlayer;spell=DrawCards(Target0,{draw});mana=None")
        }
        Special::DrawThenDiscard { draw, discard } => {
            format!("target=None;spell=DrawThenDiscard(Controller,{draw},{discard});mana=None")
        }
        Special::GrabThePrize => "target=None;spell=GrabThePrize;mana=None".to_string(),
        Special::HighwayRobbery => "target=None;spell=HighwayRobbery;mana=None".to_string(),
        Special::SearingBlaze => {
            "target=PlayerThenTheirCreature;spell=SearingBlaze;mana=None".to_string()
        }
        Special::CounterTarget(StackSpellFilter::Any) => {
            "target=AnySpellOnStack;spell=CounterTarget;mana=None".to_string()
        }
        Special::CounterTarget(StackSpellFilter::Instant) => {
            "target=InstantSpellOnStack;spell=CounterTarget;mana=None".to_string()
        }
        Special::ColorBlast {
            checked_color,
            filter_timing: BlastFilterTiming::Targeting,
        } => format!(
            "target={};spell=CounterTarget;mana=None",
            checked_color
                .filtered_spell_target_spec()
                .trim_start_matches("TargetSpec::")
        ),
        Special::ColorBlast {
            checked_color,
            filter_timing: BlastFilterTiming::Resolution,
        } => format!(
            "target=AnySpellOnStack;spell=CounterTargetIfColor({});mana=None",
            checked_color.symbol()
        ),
        Special::EndTheFestivities => {
            "target=None;spell=DamageOpponentAndTheirCreatures(1);mana=None".to_string()
        }
        Special::GalvanicBlast => "target=AnyTarget;spell=GalvanicBlast;mana=None".to_string(),
        Special::RallyAtTheHornburg => "target=None;spell=RallyAtTheHornburg;mana=None".to_string(),
        Special::RecklessImpulse => "target=None;spell=RecklessImpulse;mana=None".to_string(),
        Special::WindingWay => "target=None;spell=WindingWay;mana=None".to_string(),
        Special::MillThenDraw { player, mill, draw } => {
            let (target, player) = match player {
                MillPlayer::Controller => ("None", "Controller"),
                MillPlayer::Target0 => ("AnyPlayer", "Target0"),
            };
            format!("target={target};spell=MillThenDraw({player},{mill},{draw});mana=None")
        }
        Special::LookReorderMayShuffleThenDraw { look, draw } => format!(
            "target=None;spell=LookReorderMayShuffleThenDraw(Controller,{look},{draw});mana=None"
        ),
        Special::DrawThenPutHandOnLibraryTop { draw, put } => format!(
            "target=None;spell=DrawThenPutHandOnLibraryTop(Controller,{draw},{put});mana=None"
        ),
        Special::ScryThenDraw { scry, draw } => {
            format!("target=None;spell=ScryThenDraw(Controller,{scry},{draw});mana=None")
        }
    }
}

/// Static combat/summoning-sickness keywords, verified against each card's
/// Java source (see the increment-3 report for the exact files read).
/// Masked Meower/Sneaky Snacker (Burn) and Clockwork Percussionist/Samurai
/// Token (Rally) carry an *unconditional* static keyword this way. Goblin
/// Bushwhacker's/Goblin Tomb Raider's haste is conditional (Kicker-gated,
/// or "as long as you control an artifact") and temporary/derived, so it is
/// deliberately NOT here -- see `engine::static_self_boost_for` and
/// `EffectOp::PumpControlled`'s `grant_haste` instead.
fn keywords_for(name: &str) -> &'static str {
    match name {
        "Masked Meower" | "Clockwork Percussionist" => "Keywords::HASTE",
        "Sneaky Snacker" => "Keywords::FLYING",
        "Samurai Token" => "Keywords::VIGILANCE",
        _ => "Keywords::NONE",
    }
}

/// `Some` Kicker cost source text (`CardDef::kicker_cost`), verified against
/// Java (`KickerAbility`). Only Goblin Bushwhacker has one this increment
/// ("Kicker {R}").
fn kicker_cost_for(name: &str) -> String {
    match name {
        "Goblin Bushwhacker" => cost_src("{R}"),
        _ => "None".to_string(),
    }
}

/// `Some` alternative cost source text (`CardDef::alt_cost`), verified
/// against Java. Only Fireblast has one this increment ("You may
/// sacrifice two Mountains rather than pay Fireblast's mana cost.").
fn alt_cost_for(name: &str) -> &'static str {
    match name {
        "Fireblast" => "Some(&[CostComponent::SacrificeLands(2)])",
        _ => "None",
    }
}

/// `Some` mandatory additional cost text (`CardDef::additional_cost`).
/// Only Grab the Prize has one this increment ("As an additional cost to
/// cast this spell, discard a card.").
fn additional_cost_for(name: &str) -> &'static str {
    match name {
        "Grab the Prize" => "Some(&[CostComponent::DiscardCards(1)])",
        _ => "None",
    }
}

/// `Some` ordered flashback-cost definition, verified against Java. Faithless
/// Looting ("Flashback {2}{R}") pays mana; Lava Dart ("Flashback --
/// Sacrifice a Mountain.") sacrifices a land; Deep Analysis pays `{1}{U}`
/// followed by 3 life. All three use the same composable `CostComponent`
/// substrate as alternative, additional, and activated-ability costs.
fn flashback_for(name: &str) -> String {
    match name {
        "Faithless Looting" => {
            let (pips, generic, x_count) = parse_cost("{2}{R}");
            format!(
                "Some(FlashbackDef {{ cost: &[CostComponent::Mana(Cost {{ pips: &[{}], generic: {generic}, x_count: {x_count} }})] }})",
                pips.join(", ")
            )
        }
        "Lava Dart" => {
            "Some(FlashbackDef { cost: &[CostComponent::SacrificeLands(1)] })".to_string()
        }
        "Deep Analysis" => {
            let (pips, generic, x_count) = parse_cost("{1}{U}");
            format!(
                "Some(FlashbackDef {{ cost: &[CostComponent::Mana(Cost {{ pips: &[{}], generic: {generic}, x_count: {x_count} }}), CostComponent::PayLife(3)] }})",
                pips.join(", ")
            )
        }
        _ => "None".to_string(),
    }
}

/// Non-mana activated abilities, verified against Java. Masked Meower
/// ("Discard a card, Sacrifice this creature: Draw a card.") and the Blood
/// token ("{1}, {T}, Discard a card, Sacrifice this artifact: Draw a
/// card.") both reduce to "discard/sacrifice/[cost]: draw a card", so both
/// share `ability_effect_draw_one`. Experimental Synthesizer's ("{2}{R},
/// Sacrifice Experimental Synthesizer: Create a 2/2 white Samurai creature
/// token with vigilance. Activate only as a sorcery.") is Rally's only
/// activated ability and the only one so far with `sorcery_speed_only:
/// true` -- see that field's doc in `card_def.rs`. Lorien Revealed adds the
/// hand-zone Islandcycling `{1}` shape: pay mana, discard the exact source,
/// then resolve the reusable typed library search.
fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Masked Meower" => &[ActivatedAbilityRecipe {
            cost: &[
                AbilityCostRecipe::DiscardCards(1),
                AbilityCostRecipe::SacrificeSelf,
            ],
            effect: AbilityEffectRecipe::DrawCards(1),
            activation_zone: "Battlefield",
            sorcery_speed_only: false,
        }],
        "Blood Token" => &[ActivatedAbilityRecipe {
            cost: &[
                AbilityCostRecipe::Mana {
                    colored: None,
                    generic: 1,
                },
                AbilityCostRecipe::Tap,
                AbilityCostRecipe::DiscardCards(1),
                AbilityCostRecipe::SacrificeSelf,
            ],
            effect: AbilityEffectRecipe::DrawCards(1),
            activation_zone: "Battlefield",
            sorcery_speed_only: false,
        }],
        "Experimental Synthesizer" => &[ActivatedAbilityRecipe {
            cost: &[
                AbilityCostRecipe::Mana {
                    colored: Some("R"),
                    generic: 2,
                },
                AbilityCostRecipe::SacrificeSelf,
            ],
            effect: AbilityEffectRecipe::CreateToken("Samurai Token"),
            activation_zone: "Battlefield",
            sorcery_speed_only: true,
        }],
        "Lorien Revealed" => &[ActivatedAbilityRecipe {
            cost: &[
                AbilityCostRecipe::Mana {
                    colored: None,
                    generic: 1,
                },
                AbilityCostRecipe::DiscardSelf,
            ],
            effect: AbilityEffectRecipe::SearchLibraryToHand {
                card_type: "Land",
                subtype: "Island",
                min_targets: 0,
                max_targets: 1,
                reveal_selected: true,
                shuffle: true,
            },
            activation_zone: "Hand",
            sorcery_speed_only: false,
        }],
        _ => &[],
    }
}

fn ability_cost_src(cost: AbilityCostRecipe) -> String {
    match cost {
        AbilityCostRecipe::Mana { colored, generic } => {
            let pips = colored
                .map(|color| format!("Pip::Colored(ManaColor::{color})"))
                .unwrap_or_default();
            format!(
                "CostComponent::Mana(Cost {{ pips: &[{pips}], generic: {generic}, x_count: 0 }})"
            )
        }
        AbilityCostRecipe::Tap => "CostComponent::Tap".to_string(),
        AbilityCostRecipe::DiscardCards(count) => {
            format!("CostComponent::DiscardCards({count})")
        }
        AbilityCostRecipe::DiscardSelf => "CostComponent::DiscardSelf".to_string(),
        AbilityCostRecipe::SacrificeSelf => "CostComponent::SacrificeSelf".to_string(),
    }
}

fn ability_cost_token(cost: AbilityCostRecipe) -> String {
    match cost {
        AbilityCostRecipe::Mana { colored, generic } => {
            format!("mana:{}:{generic}:0", colored.unwrap_or("-"))
        }
        AbilityCostRecipe::Tap => "tap".to_string(),
        AbilityCostRecipe::DiscardCards(count) => format!("discard_cards:{count}"),
        AbilityCostRecipe::DiscardSelf => "discard_self".to_string(),
        AbilityCostRecipe::SacrificeSelf => "sacrifice_self".to_string(),
    }
}

fn ability_effect_token(effect: AbilityEffectRecipe) -> String {
    match effect {
        AbilityEffectRecipe::DrawCards(count) => format!("draw_cards:{count}"),
        AbilityEffectRecipe::CreateToken(name) => format!("create_token:{name}"),
        AbilityEffectRecipe::SearchLibraryToHand {
            card_type,
            subtype,
            min_targets,
            max_targets,
            reveal_selected,
            shuffle,
        } => format!(
            "search_library_to_hand:{card_type}:{subtype}:min={min_targets}:max={max_targets}:reveal={reveal_selected}:shuffle={shuffle}"
        ),
    }
}

fn ability_effect_fn_name(effect: AbilityEffectRecipe) -> String {
    match effect {
        AbilityEffectRecipe::DrawCards(count) => format!("ability_effect_draw_{count}"),
        AbilityEffectRecipe::CreateToken("Samurai Token") => {
            "ability_effect_create_samurai_token".to_string()
        }
        AbilityEffectRecipe::CreateToken(name) => {
            panic!("no generated activated-ability token function for {name:?}")
        }
        AbilityEffectRecipe::SearchLibraryToHand {
            card_type: "Land",
            subtype: "Island",
            min_targets: 0,
            max_targets: 1,
            reveal_selected: true,
            shuffle: true,
        } => "ability_effect_islandcycle".to_string(),
        AbilityEffectRecipe::SearchLibraryToHand { .. } => panic!(
            "SearchLibraryToHand currently supports only optional single Land/Island reveal+shuffle"
        ),
    }
}

fn activated_abilities_for(name: &str) -> String {
    let recipes = activated_ability_recipes_for(name);
    if recipes.is_empty() {
        return "&[]".to_string();
    }
    let abilities = recipes
        .iter()
        .map(|recipe| {
            let costs = recipe
                .cost
                .iter()
                .copied()
                .map(ability_cost_src)
                .collect::<Vec<_>>()
                .join(", ");
            let effect = ability_effect_fn_name(recipe.effect);
            let zone = recipe.activation_zone;
            let sorcery = recipe.sorcery_speed_only;
            format!(
                "ActivatedAbilityDef {{ cost: &[{costs}], target_spec: TargetSpec::None, effect: {effect}, activation_zone: Zone::{zone}, sorcery_speed_only: {sorcery} }}"
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("&[{abilities}]")
}

fn activated_abilities_token(name: &str) -> String {
    activated_ability_recipes_for(name)
        .iter()
        .map(|recipe| {
            let costs = recipe
                .cost
                .iter()
                .copied()
                .map(ability_cost_token)
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "zone={};sorcery={};target=none;cost=[{}];effect={}",
                recipe.activation_zone.to_ascii_lowercase(),
                recipe.sorcery_speed_only,
                costs,
                ability_effect_token(recipe.effect)
            )
        })
        .collect::<Vec<_>>()
        .join("|")
}

/// `Some` Plot cost source text (`CardDef::plot_cost`), verified against
/// Java (`PlotAbility`). Only Highway Robbery has one this increment
/// ("Plot {1}{R}").
fn plot_cost_for(name: &str) -> String {
    match name {
        "Highway Robbery" => cost_src("{1}{R}"),
        _ => "None".to_string(),
    }
}

/// `Some` Madness cost source text (`CardDef::madness_cost`), verified
/// against Java (`MadnessAbility`). Only Fiery Temper has one this
/// increment ("Madness {R}").
fn madness_cost_for(name: &str) -> String {
    match name {
        "Fiery Temper" => cost_src("{R}"),
        _ => "None".to_string(),
    }
}

/// `Some` second-mode source text (`CardDef::mode2`) for the symmetric
/// Elemental Blast/Pyroblast/Hydroblast family.
fn mode2_for(name: &str) -> String {
    match special_for(name) {
        Special::ColorBlast {
            checked_color,
            filter_timing: BlastFilterTiming::Targeting,
        } => format!(
            "Some(ModeDef {{ target_spec: {}, effect: mode2_effect_destroy_target_permanent }})",
            checked_color.filtered_permanent_target_spec()
        ),
        Special::ColorBlast {
            checked_color,
            filter_timing: BlastFilterTiming::Resolution,
        } => format!(
            "Some(ModeDef {{ target_spec: TargetSpec::AnyPermanent, effect: mode2_effect_destroy_target_permanent_if_{} }})",
            checked_color.suffix()
        ),
        _ => "None".to_string(),
    }
}

/// Renders a mana cost string straight to a `Cost { .. }` literal wrapped in
/// `Some(..)`, for the one-off cost tables above (`plot_cost_for`/
/// `madness_cost_for`) that aren't full `CardJson` records.
fn cost_src(mana_cost: &str) -> String {
    let (pips, generic, x_count) = parse_cost(mana_cost);
    format!(
        "Some(Cost {{ pips: &[{}], generic: {generic}, x_count: {x_count} }})",
        pips.join(", ")
    )
}

fn generic_cost_reduction_for(name: &str) -> &'static str {
    match name {
        "Cryptic Serpent" => {
            "Some(GenericCostReductionDef { generic_per_count: 1, count: DynamicCountDef::ControllerGraveyardAnyType(&[CardType::Instant, CardType::Sorcery]) })"
        }
        _ => "None",
    }
}

fn intrinsic_basic_mana_color(card: &CardJson) -> Option<&'static str> {
    let is_basic_land = card.is_land
        && card.types.iter().any(|card_type| card_type == "Land")
        && card.supertypes.iter().any(|supertype| supertype == "Basic");
    if !is_basic_land {
        return None;
    }

    let intrinsic: Vec<&'static str> = card
        .subtypes
        .iter()
        .filter_map(|subtype| match subtype.as_str() {
            "Plains" => Some("W"),
            "Island" => Some("U"),
            "Swamp" => Some("B"),
            "Mountain" => Some("R"),
            "Forest" => Some("G"),
            _ => None,
        })
        .collect();
    if intrinsic.len() != 1 {
        panic!(
            "cards_v1.json: intrinsic basic land {:?} must have exactly one basic land subtype, got {:?}",
            card.name, card.subtypes
        );
    }
    let expected = intrinsic[0];
    if card.produces_mana.len() != 1 || card.produces_mana[0] != expected {
        panic!(
            "cards_v1.json: intrinsic basic land {:?} subtype requires produces_mana {:?}, got {:?}",
            card.name, expected, card.produces_mana
        );
    }
    Some(expected)
}

fn is_ordinary_permanent(card: &CardJson) -> bool {
    !card.is_land
        && !card.is_token
        && card
            .types
            .iter()
            .any(|card_type| matches!(card_type.as_str(), "Artifact" | "Creature" | "Enchantment"))
}

fn capability_src(capability: EngineCapabilityJson) -> &'static str {
    match capability {
        EngineCapabilityJson::NoEffect => "CardCapability::NoEffect",
        EngineCapabilityJson::Partial => "CardCapability::Partial",
        EngineCapabilityJson::Full => "CardCapability::Full",
    }
}

fn codegen(cards: &[CardJson]) -> String {
    let mut out = String::new();
    writeln!(
        out,
        "// GENERATED by build.rs from data/cards_v1.json. Do not edit by hand."
    )
    .unwrap();
    writeln!(out).unwrap();

    let mut draw_card_counts = Vec::new();
    for card in cards {
        if let Special::DrawCards(count) = special_for(&card.name) {
            if !draw_card_counts.contains(&count) {
                draw_card_counts.push(count);
            }
        }
    }
    for count in draw_card_counts {
        writeln!(out, "fn spell_effect_draw_{count}() -> Option<EffectOp> {{").unwrap();
        writeln!(
            out,
            "    Some(EffectOp::DrawCards {{ player: PlayerRef::Controller, count: {count} }})"
        )
        .unwrap();
        writeln!(out, "}}").unwrap();
        writeln!(out).unwrap();
    }

    // Shared/one-off effect-program functions. Function *pointers* (not
    // owned EffectOp values) are what make a `static [CardDef; N]` array
    // possible: EffectOp contains Vec/Box and can't live in a const
    // initializer directly, but a `fn() -> Option<EffectOp>` can, and it
    // builds the (small) tree fresh each call.
    for (suffix, color) in [
        ("w", "W"),
        ("u", "U"),
        ("b", "B"),
        ("r", "R"),
        ("g", "G"),
        ("c", "C"),
    ] {
        let used = cards.iter().any(|card| {
            card.engine_capability != EngineCapabilityJson::NoEffect
                && (intrinsic_basic_mana_color(card) == Some(color)
                    || (color == "R" && matches!(special_for(&card.name), Special::GreatFurnace)))
        });
        if !used {
            continue;
        }
        writeln!(out, "fn mana_ability_add_{suffix}() -> Option<EffectOp> {{").unwrap();
        writeln!(out, "    Some(EffectOp::Sequence(vec![").unwrap();
        writeln!(
            out,
            "        EffectOp::TapObject {{ object: ObjectRef::ThisSource }},"
        )
        .unwrap();
        writeln!(out, "        EffectOp::AddMana {{ player: PlayerRef::Controller, colors: vec![ManaColor::{color}] }},").unwrap();
        writeln!(out, "    ]))").unwrap();
        writeln!(out, "}}").unwrap();
        writeln!(out).unwrap();
    }

    writeln!(
        out,
        "fn spell_effect_ordinary_permanent() -> Option<EffectOp> {{"
    )
    .unwrap();
    writeln!(out, "    Some(EffectOp::MoveObject {{ object: ObjectRef::ThisSource, to_zone: Zone::Battlefield }})").unwrap();
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();

    // Emit each structured activated-ability effect once. These are the same
    // recipes used for CardDef source and the v3 card-database hash tokens,
    // so a cost/zone/effect change cannot update one surface and leave the
    // others silently stale.
    let mut activated_effects = Vec::new();
    for card in cards {
        for recipe in activated_ability_recipes_for(&card.name) {
            if !activated_effects.contains(&recipe.effect) {
                activated_effects.push(recipe.effect);
            }
        }
    }
    for effect in activated_effects {
        let function_name = ability_effect_fn_name(effect);
        writeln!(out, "fn {function_name}() -> EffectOp {{").unwrap();
        match effect {
            AbilityEffectRecipe::DrawCards(count) => {
                writeln!(
                    out,
                    "    EffectOp::DrawCards {{ player: PlayerRef::Controller, count: {count} }}"
                )
                .unwrap();
            }
            AbilityEffectRecipe::CreateToken(name) => {
                writeln!(out, "    let token = crate::card_def::card_id_by_name({name:?}).expect(\"{name} in CARD_DEFS\");").unwrap();
                writeln!(out, "    EffectOp::CreateToken {{ token_def: token, controller: PlayerRef::Controller }}").unwrap();
            }
            AbilityEffectRecipe::SearchLibraryToHand {
                card_type: "Land",
                subtype,
                min_targets: 0,
                max_targets: 1,
                reveal_selected: true,
                shuffle: true,
            } => {
                writeln!(out, "    EffectOp::SearchLibraryToHand {{ player: PlayerRef::Controller, filter: LibraryCardFilter::LandWithSubtype(Subtype::{subtype}) }}").unwrap();
            }
            AbilityEffectRecipe::SearchLibraryToHand { .. } => {
                unreachable!("ability_effect_fn_name rejects unsupported search recipes")
            }
        }
        writeln!(out, "}}").unwrap();
        writeln!(out).unwrap();
    }

    if cards
        .iter()
        .any(|c| matches!(special_for(&c.name), Special::ChainLightning))
    {
        // "Deals 3 damage to any target. Then that player or that
        // permanent's controller may pay {R}{R}. If the player does, they
        // may copy this spell...": mandatory damage first (identical shape
        // to `BurnAnyTarget(3)`), then the resolution-suspending copy offer
        // -- see `EffectOp::OfferAffectedPlayerSpellCopy`'s doc.
        writeln!(
            out,
            "fn spell_effect_chain_lightning() -> Option<EffectOp> {{"
        )
        .unwrap();
        writeln!(out, "    Some(EffectOp::Sequence(vec![").unwrap();
        writeln!(
            out,
            "        EffectOp::DealDamage {{ target: TargetRef::Target(0), amount: 3 }},"
        )
        .unwrap();
        writeln!(
            out,
            "        EffectOp::OfferAffectedPlayerSpellCopy {{ affected: TargetRef::Target(0) }},"
        )
        .unwrap();
        writeln!(out, "    ]))").unwrap();
        writeln!(out, "}}").unwrap();
        writeln!(out).unwrap();
    }

    let mut target_player_draw_shapes = Vec::new();
    for card in cards {
        if let Special::TargetPlayerDraw { draw } = special_for(&card.name) {
            if !target_player_draw_shapes.contains(&draw) {
                target_player_draw_shapes.push(draw);
            }
        }
    }
    for draw in target_player_draw_shapes {
        writeln!(
            out,
            "fn spell_effect_target_player_draw_{draw}() -> Option<EffectOp> {{"
        )
        .unwrap();
        writeln!(
            out,
            "    Some(EffectOp::DrawCards {{ player: PlayerRef::Target(0), count: {draw} }})"
        )
        .unwrap();
        writeln!(out, "}}").unwrap();
        writeln!(out).unwrap();
    }

    if cards
        .iter()
        .any(|c| matches!(special_for(&c.name), Special::EndTheFestivities))
    {
        // "Deals 1 damage to each opponent and each creature and
        // planeswalker they control." No planeswalker exists in this pool,
        // so `DamageOpponentAndTheirCreatures` (which only hits the
        // opponent + their creatures) already covers 100% of the reachable
        // text -- see `EffectOp::DamageOpponentAndTheirCreatures`'s doc.
        writeln!(
            out,
            "fn spell_effect_end_the_festivities() -> Option<EffectOp> {{"
        )
        .unwrap();
        writeln!(
            out,
            "    Some(EffectOp::DamageOpponentAndTheirCreatures {{ amount: 1 }})"
        )
        .unwrap();
        writeln!(out, "}}").unwrap();
        writeln!(out).unwrap();
    }

    if cards
        .iter()
        .any(|c| matches!(special_for(&c.name), Special::GalvanicBlast))
    {
        // Metalcraft -- 4 damage instead of 2 if you control 3+ artifacts.
        writeln!(
            out,
            "fn spell_effect_galvanic_blast() -> Option<EffectOp> {{"
        )
        .unwrap();
        writeln!(out, "    Some(EffectOp::Conditional {{").unwrap();
        writeln!(out, "        cond: EffectCond::ControlsArtifactCount(3),").unwrap();
        writeln!(out, "        then: Box::new(EffectOp::DealDamage {{ target: TargetRef::Target(0), amount: 4 }}),").unwrap();
        writeln!(out, "        else_: Box::new(EffectOp::DealDamage {{ target: TargetRef::Target(0), amount: 2 }}),").unwrap();
        writeln!(out, "    }})").unwrap();
        writeln!(out, "}}").unwrap();
        writeln!(out).unwrap();
    }

    if cards
        .iter()
        .any(|c| matches!(special_for(&c.name), Special::RallyAtTheHornburg))
    {
        // "Create two 1/1 white Human Soldier creature tokens. Humans you
        // control gain haste until end of turn." The two just-created
        // tokens are themselves Human, so they're inside the "Humans you
        // control" set the pump snapshots -- see `EffectOp::PumpControlled`'s
        // doc for why sequencing (create both tokens, *then* pump) is what
        // makes that true.
        writeln!(
            out,
            "fn spell_effect_rally_at_the_hornburg() -> Option<EffectOp> {{"
        )
        .unwrap();
        writeln!(out, "    let human_soldier = crate::card_def::card_id_by_name(\"Human Soldier Token\").expect(\"Human Soldier Token in CARD_DEFS\");").unwrap();
        writeln!(out, "    Some(EffectOp::Sequence(vec![").unwrap();
        writeln!(out, "        EffectOp::CreateToken {{ token_def: human_soldier, controller: PlayerRef::Controller }},").unwrap();
        writeln!(out, "        EffectOp::CreateToken {{ token_def: human_soldier, controller: PlayerRef::Controller }},").unwrap();
        writeln!(
            out,
            "        EffectOp::PumpControlled {{ filter: CreatureFilter::ControlledWithSubtype(Subtype::Human), power: 0, toughness: 0, grant_haste: true }},"
        )
        .unwrap();
        writeln!(out, "    ]))").unwrap();
        writeln!(out, "}}").unwrap();
        writeln!(out).unwrap();
    }

    if cards
        .iter()
        .any(|c| matches!(special_for(&c.name), Special::RecklessImpulse))
    {
        // "Exile the top two cards of your library. Until the end of your
        // next turn, you may play those cards."
        writeln!(
            out,
            "fn spell_effect_reckless_impulse() -> Option<EffectOp> {{"
        )
        .unwrap();
        writeln!(out, "    Some(EffectOp::ImpulseDraw {{ count: 2, duration: ImpulseDuration::UntilOwnersNextTurn }})").unwrap();
        writeln!(out, "}}").unwrap();
        writeln!(out).unwrap();
    }

    if cards
        .iter()
        .any(|c| matches!(special_for(&c.name), Special::WindingWay))
    {
        // Printed order is policy semantics: zero is Creature, one is Land.
        // The choice is made during resolution before the public reveal.
        writeln!(out, "fn spell_effect_winding_way() -> Option<EffectOp> {{").unwrap();
        writeln!(out, "    Some(EffectOp::Choice {{").unwrap();
        writeln!(out, "        controller: PlayerRef::Controller,").unwrap();
        writeln!(out, "        options: vec![").unwrap();
        for card_type in ["Creature", "Land"] {
            writeln!(out, "            EffectOp::RevealTopAndPartitionByType {{").unwrap();
            writeln!(out, "                player: PlayerRef::Controller,").unwrap();
            writeln!(out, "                count: 4,").unwrap();
            writeln!(out, "                card_type: CardType::{card_type},").unwrap();
            writeln!(out, "                matching_to: Zone::Hand,").unwrap();
            writeln!(out, "                rest_to: Zone::Graveyard,").unwrap();
            writeln!(out, "            }},").unwrap();
        }
        writeln!(out, "        ],").unwrap();
        writeln!(out, "    }})").unwrap();
        writeln!(out, "}}").unwrap();
        writeln!(out).unwrap();
    }

    let mut look_reorder_may_shuffle_then_draw_shapes = Vec::new();
    for card in cards {
        if let Special::LookReorderMayShuffleThenDraw { look, draw } = special_for(&card.name) {
            let shape = (look, draw);
            if !look_reorder_may_shuffle_then_draw_shapes.contains(&shape) {
                look_reorder_may_shuffle_then_draw_shapes.push(shape);
            }
        }
    }
    for (look, draw) in look_reorder_may_shuffle_then_draw_shapes {
        writeln!(
            out,
            "fn spell_effect_look_reorder_may_shuffle_then_draw_{look}_{draw}() -> Option<EffectOp> {{"
        )
        .unwrap();
        writeln!(out, "    Some(EffectOp::Sequence(vec![").unwrap();
        writeln!(
            out,
            "        EffectOp::LookAtLibraryTopAndReorder {{ player: PlayerRef::Controller, count: {look} }},"
        )
        .unwrap();
        writeln!(
            out,
            "        EffectOp::MayShuffleLibrary {{ player: PlayerRef::Controller }},"
        )
        .unwrap();
        writeln!(
            out,
            "        EffectOp::DrawCards {{ player: PlayerRef::Controller, count: {draw} }},"
        )
        .unwrap();
        writeln!(out, "    ]))").unwrap();
        writeln!(out, "}}").unwrap();
        writeln!(out).unwrap();
    }

    let mut draw_then_put_hand_on_library_top_shapes = Vec::new();
    for card in cards {
        if let Special::DrawThenPutHandOnLibraryTop { draw, put } = special_for(&card.name) {
            let shape = (draw, put);
            if !draw_then_put_hand_on_library_top_shapes.contains(&shape) {
                draw_then_put_hand_on_library_top_shapes.push(shape);
            }
        }
    }
    for (draw, put) in draw_then_put_hand_on_library_top_shapes {
        writeln!(
            out,
            "fn spell_effect_draw_then_put_hand_on_library_top_{draw}_{put}() -> Option<EffectOp> {{"
        )
        .unwrap();
        writeln!(out, "    Some(EffectOp::Sequence(vec![").unwrap();
        writeln!(
            out,
            "        EffectOp::DrawCards {{ player: PlayerRef::Controller, count: {draw} }},"
        )
        .unwrap();
        writeln!(
            out,
            "        EffectOp::PutCardsFromHandOnLibraryTop {{ player: PlayerRef::Controller, count: {put} }},"
        )
        .unwrap();
        writeln!(out, "    ]))").unwrap();
        writeln!(out, "}}").unwrap();
        writeln!(out).unwrap();
    }

    let mut scry_then_draw_shapes = Vec::new();
    for card in cards {
        if let Special::ScryThenDraw { scry, draw } = special_for(&card.name) {
            let shape = (scry, draw);
            if !scry_then_draw_shapes.contains(&shape) {
                scry_then_draw_shapes.push(shape);
            }
        }
    }
    for (scry, draw) in scry_then_draw_shapes {
        writeln!(
            out,
            "fn spell_effect_scry_then_draw_{scry}_{draw}() -> Option<EffectOp> {{"
        )
        .unwrap();
        writeln!(out, "    Some(EffectOp::Sequence(vec![").unwrap();
        writeln!(
            out,
            "        EffectOp::Scry {{ player: PlayerRef::Controller, count: {scry} }},"
        )
        .unwrap();
        writeln!(
            out,
            "        EffectOp::DrawCards {{ player: PlayerRef::Controller, count: {draw} }},"
        )
        .unwrap();
        writeln!(out, "    ]))").unwrap();
        writeln!(out, "}}").unwrap();
        writeln!(out).unwrap();
    }

    let mut mill_then_draw_shapes = Vec::new();
    for card in cards {
        if let Special::MillThenDraw { player, mill, draw } = special_for(&card.name) {
            let shape = (player, mill, draw);
            if !mill_then_draw_shapes.contains(&shape) {
                mill_then_draw_shapes.push(shape);
            }
        }
    }
    for (player, mill, draw) in mill_then_draw_shapes {
        let (player_suffix, player_src) = match player {
            MillPlayer::Controller => ("controller", "PlayerRef::Controller"),
            MillPlayer::Target0 => ("target_0", "PlayerRef::Target(0)"),
        };
        writeln!(
            out,
            "fn spell_effect_mill_then_draw_{player_suffix}_{mill}_{draw}() -> Option<EffectOp> {{"
        )
        .unwrap();
        writeln!(out, "    Some(EffectOp::Sequence(vec![").unwrap();
        writeln!(
            out,
            "        EffectOp::MillCards {{ player: {player_src}, count: {mill} }},"
        )
        .unwrap();
        writeln!(
            out,
            "        EffectOp::DrawCards {{ player: PlayerRef::Controller, count: {draw} }},"
        )
        .unwrap();
        writeln!(out, "    ]))").unwrap();
        writeln!(out, "}}").unwrap();
        writeln!(out).unwrap();
    }

    let draw_then_discard_shapes: Vec<(i32, i32)> = cards
        .iter()
        .filter_map(|c| match special_for(&c.name) {
            Special::DrawThenDiscard { draw, discard } => Some((draw, discard)),
            _ => None,
        })
        .collect();
    for (draw, discard) in draw_then_discard_shapes {
        writeln!(
            out,
            "fn spell_effect_draw_then_discard_{draw}_{discard}() -> Option<EffectOp> {{"
        )
        .unwrap();
        writeln!(out, "    Some(EffectOp::Sequence(vec![").unwrap();
        writeln!(
            out,
            "        EffectOp::DrawCards {{ player: PlayerRef::Controller, count: {draw} }},"
        )
        .unwrap();
        writeln!(
            out,
            "        EffectOp::DiscardCards {{ player: PlayerRef::Controller, count: {discard} }},"
        )
        .unwrap();
        writeln!(out, "    ]))").unwrap();
        writeln!(out, "}}").unwrap();
        writeln!(out).unwrap();
    }

    if cards
        .iter()
        .any(|c| matches!(special_for(&c.name), Special::GrabThePrize))
    {
        writeln!(
            out,
            "fn spell_effect_grab_the_prize() -> Option<EffectOp> {{"
        )
        .unwrap();
        writeln!(out, "    Some(EffectOp::Sequence(vec![").unwrap();
        writeln!(
            out,
            "        EffectOp::DrawCards {{ player: PlayerRef::Controller, count: 2 }},"
        )
        .unwrap();
        writeln!(out, "        EffectOp::Conditional {{").unwrap();
        writeln!(
            out,
            "            cond: EffectCond::DiscardedNonLandForCost,"
        )
        .unwrap();
        writeln!(out, "            then: Box::new(EffectOp::DealDamage {{ target: TargetRef::Opponent, amount: 2 }}),").unwrap();
        writeln!(
            out,
            "            else_: Box::new(EffectOp::Sequence(vec![])),"
        )
        .unwrap();
        writeln!(out, "        }},").unwrap();
        writeln!(out, "    ]))").unwrap();
        writeln!(out, "}}").unwrap();
        writeln!(out).unwrap();
    }

    if cards
        .iter()
        .any(|c| matches!(special_for(&c.name), Special::HighwayRobbery))
    {
        // "You may discard a card or sacrifice a land. If you do, draw two
        // cards." -- DoIfCostPaid(OrCost(DiscardCardCost, SacrificeTargetCost)).
        writeln!(
            out,
            "fn spell_effect_highway_robbery() -> Option<EffectOp> {{"
        )
        .unwrap();
        writeln!(out, "    Some(EffectOp::MayPayCostThen {{").unwrap();
        writeln!(out, "        discard: 1,").unwrap();
        writeln!(out, "        sacrifice_lands: 1,").unwrap();
        writeln!(out, "        then: Box::new(EffectOp::DrawCards {{ player: PlayerRef::Controller, count: 2 }}),").unwrap();
        writeln!(out, "    }})").unwrap();
        writeln!(out, "}}").unwrap();
        writeln!(out).unwrap();
    }

    if cards
        .iter()
        .any(|c| matches!(special_for(&c.name), Special::SearingBlaze))
    {
        // 1 damage to target player + 1 damage to target creature that
        // player controls; landfall bumps both to 3. The creature-damage
        // leaf is individually fizzle-guarded (608.2b: Searing Blaze still
        // hits the player even if the creature target became illegal --
        // the player target can't, so the whole spell can never fully
        // fizzle in this pool).
        writeln!(
            out,
            "fn spell_effect_searing_blaze() -> Option<EffectOp> {{"
        )
        .unwrap();
        writeln!(out, "    Some(EffectOp::Conditional {{").unwrap();
        writeln!(out, "        cond: EffectCond::LandfallThisTurn,").unwrap();
        writeln!(out, "        then: Box::new(EffectOp::Sequence(vec![").unwrap();
        writeln!(
            out,
            "            EffectOp::DealDamage {{ target: TargetRef::Target(0), amount: 3 }},"
        )
        .unwrap();
        writeln!(out, "            EffectOp::Conditional {{").unwrap();
        writeln!(
            out,
            "                cond: EffectCond::TargetInZone(1, Zone::Battlefield),"
        )
        .unwrap();
        writeln!(out, "                then: Box::new(EffectOp::DealDamage {{ target: TargetRef::Target(1), amount: 3 }}),").unwrap();
        writeln!(
            out,
            "                else_: Box::new(EffectOp::Sequence(vec![])),"
        )
        .unwrap();
        writeln!(out, "            }},").unwrap();
        writeln!(out, "        ])),").unwrap();
        writeln!(out, "        else_: Box::new(EffectOp::Sequence(vec![").unwrap();
        writeln!(
            out,
            "            EffectOp::DealDamage {{ target: TargetRef::Target(0), amount: 1 }},"
        )
        .unwrap();
        writeln!(out, "            EffectOp::Conditional {{").unwrap();
        writeln!(
            out,
            "                cond: EffectCond::TargetInZone(1, Zone::Battlefield),"
        )
        .unwrap();
        writeln!(out, "                then: Box::new(EffectOp::DealDamage {{ target: TargetRef::Target(1), amount: 1 }}),").unwrap();
        writeln!(
            out,
            "                else_: Box::new(EffectOp::Sequence(vec![])),"
        )
        .unwrap();
        writeln!(out, "            }},").unwrap();
        writeln!(out, "        ])),").unwrap();
        writeln!(out, "    }})").unwrap();
        writeln!(out, "}}").unwrap();
        writeln!(out).unwrap();
    }

    // Reusable "counter target spell" resolution program. Target filters
    // belong to `TargetSpec`; the effect itself is identical for
    // Counterspell, Dispel, and both Elemental Blasts, and is nested under
    // Pyroblast/Hydroblast's resolution-time color check. The zone guard
    // makes a stale target a no-op, while `EffectOp::MoveObject` owns
    // physical-card, flashback, and virtual-copy departure semantics.
    writeln!(out, "fn counter_target_spell_effect() -> EffectOp {{").unwrap();
    writeln!(out, "    EffectOp::Conditional {{").unwrap();
    writeln!(
        out,
        "        cond: EffectCond::TargetInZone(0, Zone::Stack),"
    )
    .unwrap();
    writeln!(out, "        then: Box::new(EffectOp::MoveObject {{ object: ObjectRef::Target(0), to_zone: Zone::Graveyard }}),").unwrap();
    writeln!(out, "        else_: Box::new(EffectOp::Sequence(vec![])),").unwrap();
    writeln!(out, "    }}").unwrap();
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "fn spell_effect_counter_target() -> Option<EffectOp> {{"
    )
    .unwrap();
    writeln!(out, "    Some(counter_target_spell_effect())").unwrap();
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();

    if cards
        .iter()
        .any(|c| matches!(special_for(&c.name), Special::ColorBlast { .. }))
    {
        // The "if it's [color]" half used by Pyroblast/Hydroblast. Color is
        // evaluated during resolution; their TargetSpec remains permissive.
        writeln!(
            out,
            "fn counter_target_spell_if_color_effect(color: ManaColor) -> EffectOp {{"
        )
        .unwrap();
        writeln!(out, "    EffectOp::Conditional {{").unwrap();
        writeln!(out, "        cond: EffectCond::TargetIsColor(0, color),").unwrap();
        writeln!(
            out,
            "        then: Box::new(counter_target_spell_effect()),"
        )
        .unwrap();
        writeln!(out, "        else_: Box::new(EffectOp::Sequence(vec![])),").unwrap();
        writeln!(out, "    }}").unwrap();
        writeln!(out, "}}").unwrap();
        writeln!(out).unwrap();

        for color in [BlastColor::Blue, BlastColor::Red] {
            writeln!(
                out,
                "fn spell_effect_counter_target_if_{}() -> Option<EffectOp> {{",
                color.suffix()
            )
            .unwrap();
            writeln!(
                out,
                "    Some(counter_target_spell_if_color_effect({}))",
                color.mana_variant()
            )
            .unwrap();
            writeln!(out, "}}").unwrap();
            writeln!(out).unwrap();
        }

        // Shared destroy program for the Elemental Blasts, whose color is a
        // targeting restriction and therefore already revalidated by the
        // engine before resolution.
        writeln!(
            out,
            "fn mode2_effect_destroy_target_permanent() -> EffectOp {{"
        )
        .unwrap();
        writeln!(out, "    EffectOp::Conditional {{").unwrap();
        writeln!(
            out,
            "        cond: EffectCond::TargetInZone(0, Zone::Battlefield),"
        )
        .unwrap();
        writeln!(out, "        then: Box::new(EffectOp::MoveObject {{ object: ObjectRef::Target(0), to_zone: Zone::Graveyard }}),").unwrap();
        writeln!(out, "        else_: Box::new(EffectOp::Sequence(vec![])),").unwrap();
        writeln!(out, "    }}").unwrap();
        writeln!(out, "}}").unwrap();
        writeln!(out).unwrap();

        writeln!(
            out,
            "fn destroy_target_permanent_if_color_effect(color: ManaColor) -> EffectOp {{"
        )
        .unwrap();
        writeln!(out, "    EffectOp::Conditional {{").unwrap();
        writeln!(out, "        cond: EffectCond::And(").unwrap();
        writeln!(
            out,
            "            Box::new(EffectCond::TargetInZone(0, Zone::Battlefield)),"
        )
        .unwrap();
        writeln!(
            out,
            "            Box::new(EffectCond::TargetIsColor(0, color)),"
        )
        .unwrap();
        writeln!(out, "        ),").unwrap();
        writeln!(out, "        then: Box::new(EffectOp::MoveObject {{ object: ObjectRef::Target(0), to_zone: Zone::Graveyard }}),").unwrap();
        writeln!(out, "        else_: Box::new(EffectOp::Sequence(vec![])),").unwrap();
        writeln!(out, "    }}").unwrap();
        writeln!(out, "}}").unwrap();
        writeln!(out).unwrap();

        for color in [BlastColor::Blue, BlastColor::Red] {
            writeln!(
                out,
                "fn mode2_effect_destroy_target_permanent_if_{}() -> EffectOp {{",
                color.suffix()
            )
            .unwrap();
            writeln!(
                out,
                "    destroy_target_permanent_if_color_effect({})",
                color.mana_variant()
            )
            .unwrap();
            writeln!(out, "}}").unwrap();
            writeln!(out).unwrap();
        }
    }

    let burn_amounts: BTreeSetLike = cards
        .iter()
        .filter_map(|c| match special_for(&c.name) {
            Special::BurnAnyTarget(a) => Some(a),
            _ => None,
        })
        .collect();
    for amount in burn_amounts.values() {
        writeln!(
            out,
            "fn spell_effect_burn_any_target_{amount}() -> Option<EffectOp> {{"
        )
        .unwrap();
        writeln!(
            out,
            "    Some(EffectOp::DealDamage {{ target: TargetRef::Target(0), amount: {amount} }})"
        )
        .unwrap();
        writeln!(out, "}}").unwrap();
        writeln!(out).unwrap();
    }

    // ---- CARD_DEFS -------------------------------------------------
    writeln!(out, "pub static CARD_DEFS: [CardDef; {}] = [", cards.len()).unwrap();
    for c in cards {
        let (pips, generic, x_count) = parse_cost(&c.mana_cost);
        let special = special_for(&c.name);

        let types_src = c
            .types
            .iter()
            .map(|t| format!("CardType::{}", card_type_variant(t)))
            .collect::<Vec<_>>()
            .join(", ");
        let subtypes_src = c
            .subtypes
            .iter()
            .map(|s| subtype_variant(s))
            .collect::<Vec<_>>()
            .join(", ");
        let supertypes_src = c
            .supertypes
            .iter()
            .map(|t| format!("Supertype::{}", supertype_variant(t)))
            .collect::<Vec<_>>()
            .join(", ");
        let produces_src = c
            .produces_mana
            .iter()
            .map(|m| format!("ManaColor::{}", color_variant(m)))
            .collect::<Vec<_>>()
            .join(", ");
        let colors_src = c
            .colors
            .iter()
            .map(|m| format!("ManaColor::{}", color_variant(m)))
            .collect::<Vec<_>>()
            .join(", ");
        let pips_src = pips.join(", ");
        let power_src = match c.power {
            Some(p) => format!("Some({p})"),
            None => "None".to_string(),
        };
        let toughness_src = match c.toughness {
            Some(t) => format!("Some({t})"),
            None => "None".to_string(),
        };

        let (target_spec_src, mut spell_effect_src, mut mana_ability_src) = match special {
            Special::None => (
                "TargetSpec::None",
                "no_effect".to_string(),
                "no_effect".to_string(),
            ),
            Special::GreatFurnace => (
                "TargetSpec::None",
                "no_effect".to_string(),
                "mana_ability_add_r".to_string(),
            ),
            Special::DrawCards(count) => (
                "TargetSpec::None",
                format!("spell_effect_draw_{count}"),
                "no_effect".to_string(),
            ),
            Special::BurnAnyTarget(amount) => (
                "TargetSpec::AnyTarget",
                format!("spell_effect_burn_any_target_{amount}"),
                "no_effect".to_string(),
            ),
            Special::ChainLightning => (
                "TargetSpec::AnyTarget",
                "spell_effect_chain_lightning".to_string(),
                "no_effect".to_string(),
            ),
            Special::TargetPlayerDraw { draw } => (
                "TargetSpec::AnyPlayer",
                format!("spell_effect_target_player_draw_{draw}"),
                "no_effect".to_string(),
            ),
            Special::DrawThenDiscard { draw, discard } => (
                "TargetSpec::None",
                format!("spell_effect_draw_then_discard_{draw}_{discard}"),
                "no_effect".to_string(),
            ),
            Special::GrabThePrize => (
                "TargetSpec::None",
                "spell_effect_grab_the_prize".to_string(),
                "no_effect".to_string(),
            ),
            Special::HighwayRobbery => (
                "TargetSpec::None",
                "spell_effect_highway_robbery".to_string(),
                "no_effect".to_string(),
            ),
            Special::SearingBlaze => (
                "TargetSpec::PlayerThenTheirCreature",
                "spell_effect_searing_blaze".to_string(),
                "no_effect".to_string(),
            ),
            Special::CounterTarget(StackSpellFilter::Any) => (
                "TargetSpec::AnySpellOnStack",
                "spell_effect_counter_target".to_string(),
                "no_effect".to_string(),
            ),
            Special::CounterTarget(StackSpellFilter::Instant) => (
                "TargetSpec::InstantSpellOnStack",
                "spell_effect_counter_target".to_string(),
                "no_effect".to_string(),
            ),
            Special::ColorBlast {
                checked_color,
                filter_timing: BlastFilterTiming::Targeting,
            } => (
                checked_color.filtered_spell_target_spec(),
                "spell_effect_counter_target".to_string(),
                "no_effect".to_string(),
            ),
            Special::ColorBlast {
                checked_color,
                filter_timing: BlastFilterTiming::Resolution,
            } => (
                "TargetSpec::AnySpellOnStack",
                format!("spell_effect_counter_target_if_{}", checked_color.suffix()),
                "no_effect".to_string(),
            ),
            Special::EndTheFestivities => (
                "TargetSpec::None",
                "spell_effect_end_the_festivities".to_string(),
                "no_effect".to_string(),
            ),
            Special::GalvanicBlast => (
                "TargetSpec::AnyTarget",
                "spell_effect_galvanic_blast".to_string(),
                "no_effect".to_string(),
            ),
            Special::RallyAtTheHornburg => (
                "TargetSpec::None",
                "spell_effect_rally_at_the_hornburg".to_string(),
                "no_effect".to_string(),
            ),
            Special::RecklessImpulse => (
                "TargetSpec::None",
                "spell_effect_reckless_impulse".to_string(),
                "no_effect".to_string(),
            ),
            Special::WindingWay => (
                "TargetSpec::None",
                "spell_effect_winding_way".to_string(),
                "no_effect".to_string(),
            ),
            Special::LookReorderMayShuffleThenDraw { look, draw } => (
                "TargetSpec::None",
                format!("spell_effect_look_reorder_may_shuffle_then_draw_{look}_{draw}"),
                "no_effect".to_string(),
            ),
            Special::DrawThenPutHandOnLibraryTop { draw, put } => (
                "TargetSpec::None",
                format!("spell_effect_draw_then_put_hand_on_library_top_{draw}_{put}"),
                "no_effect".to_string(),
            ),
            Special::ScryThenDraw { scry, draw } => (
                "TargetSpec::None",
                format!("spell_effect_scry_then_draw_{scry}_{draw}"),
                "no_effect".to_string(),
            ),
            Special::MillThenDraw { player, mill, draw } => {
                let (target_spec, player_suffix) = match player {
                    MillPlayer::Controller => ("TargetSpec::None", "controller"),
                    MillPlayer::Target0 => ("TargetSpec::AnyPlayer", "target_0"),
                };
                (
                    target_spec,
                    format!("spell_effect_mill_then_draw_{player_suffix}_{mill}_{draw}"),
                    "no_effect".to_string(),
                )
            }
        };

        let executable = c.engine_capability != EngineCapabilityJson::NoEffect;
        if executable && matches!(special, Special::None) && is_ordinary_permanent(c) {
            spell_effect_src = "spell_effect_ordinary_permanent".to_string();
        }

        let intrinsic_basic_color = intrinsic_basic_mana_color(c);
        if let (true, Some(color)) = (executable, intrinsic_basic_color) {
            let suffix = color.to_ascii_lowercase();
            // Validate the metadata symbol through the same closed color set
            // used to render `CardDef::produces_mana` before naming the
            // generated one-color ability.
            color_variant(color);
            mana_ability_src = format!("mana_ability_add_{suffix}");
        }

        let has_spell_program = spell_effect_src != "no_effect";
        let has_mana_program = mana_ability_src != "no_effect";
        if executable && !c.is_token {
            if c.is_land && !has_mana_program {
                panic!(
                    "cards_v1.json: executable land {:?} has no generated mana program",
                    c.name
                );
            }
            if !c.is_land && !has_spell_program {
                panic!(
                    "cards_v1.json: executable nonland {:?} has no generated spell program",
                    c.name
                );
            }
        }
        if !executable && (has_spell_program || has_mana_program) {
            panic!(
                "cards_v1.json: no-effect card {:?} unexpectedly received an executable program",
                c.name
            );
        }

        writeln!(out, "    CardDef {{").unwrap();
        writeln!(out, "        name: {:?},", c.name).unwrap();
        writeln!(
            out,
            "        capability: {},",
            capability_src(c.engine_capability)
        )
        .unwrap();
        writeln!(
            out,
            "        cost: Cost {{ pips: &[{pips_src}], generic: {generic}, x_count: {x_count} }},"
        )
        .unwrap();
        writeln!(
            out,
            "        generic_cost_reduction: {},",
            generic_cost_reduction_for(&c.name)
        )
        .unwrap();
        writeln!(out, "        types: &[{types_src}],").unwrap();
        writeln!(out, "        subtypes: &[{subtypes_src}],").unwrap();
        writeln!(out, "        supertypes: &[{supertypes_src}],").unwrap();
        writeln!(out, "        power: {power_src},").unwrap();
        writeln!(out, "        toughness: {toughness_src},").unwrap();
        writeln!(out, "        is_land: {},", c.is_land).unwrap();
        writeln!(out, "        produces_mana: &[{produces_src}],").unwrap();
        writeln!(out, "        colors: &[{colors_src}],").unwrap();
        writeln!(out, "        target_spec: {target_spec_src},").unwrap();
        writeln!(out, "        keywords: {},", keywords_for(&c.name)).unwrap();
        writeln!(out, "        spell_effect: {spell_effect_src},").unwrap();
        writeln!(out, "        mana_ability: {mana_ability_src},").unwrap();
        writeln!(out, "        alt_cost: {},", alt_cost_for(&c.name)).unwrap();
        writeln!(out, "        kicker_cost: {},", kicker_cost_for(&c.name)).unwrap();
        writeln!(
            out,
            "        additional_cost: {},",
            additional_cost_for(&c.name)
        )
        .unwrap();
        writeln!(out, "        flashback: {},", flashback_for(&c.name)).unwrap();
        writeln!(
            out,
            "        activated_abilities: {},",
            activated_abilities_for(&c.name)
        )
        .unwrap();
        writeln!(out, "        plot_cost: {},", plot_cost_for(&c.name)).unwrap();
        writeln!(out, "        madness_cost: {},", madness_cost_for(&c.name)).unwrap();
        writeln!(out, "        mode2: {},", mode2_for(&c.name)).unwrap();
        writeln!(out, "        is_token: {},", c.is_token).unwrap();
        writeln!(out, "    }},").unwrap();
    }
    writeln!(out, "];").unwrap();
    writeln!(out).unwrap();

    // ---- name -> id --------------------------------------------------
    writeln!(out, "pub fn card_id_by_name(name: &str) -> Option<u16> {{").unwrap();
    writeln!(out, "    match name {{").unwrap();
    for (i, c) in cards.iter().enumerate() {
        writeln!(out, "        {:?} => Some({i}),", c.name).unwrap();
    }
    writeln!(out, "        _ => None,").unwrap();
    writeln!(out, "    }}").unwrap();
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();

    // ---- content + executable-recipe hash ------------------------------
    // v5 hashes every generated CardDef selector plus semantic tokens from
    // the same `Special` and structured activated-ability recipes that emit
    // executable definitions. Lorien's Draw3/search and Deep Analysis's
    // target-player draw/ordered flashback remain bound alongside each
    // Blast's checked color and targeting-versus-resolution filter timing.
    // Metadata-only registry fields (timestamps, java_file paths, complexity
    // tags) remain intentionally outside the contract.
    let mut canon = String::from("kernel_carddb/v5\n");
    for c in cards {
        canon.push_str(&c.name);
        canon.push('|');
        canon.push_str(&c.mana_cost);
        canon.push('|');
        canon.push_str(&c.types.join(","));
        canon.push('|');
        canon.push_str(&c.subtypes.join(","));
        canon.push('|');
        canon.push_str(&c.supertypes.join(","));
        canon.push('|');
        canon.push_str(&c.power.map(|p| p.to_string()).unwrap_or_default());
        canon.push('|');
        canon.push_str(&c.toughness.map(|t| t.to_string()).unwrap_or_default());
        canon.push('|');
        canon.push_str(if c.is_land { "L" } else { "-" });
        canon.push('|');
        canon.push_str(&c.produces_mana.join(","));
        canon.push('|');
        canon.push_str(&c.colors.join(","));
        canon.push('|');
        canon.push_str(if c.is_token { "T" } else { "-" });
        canon.push('|');
        canon.push_str(match c.engine_capability {
            EngineCapabilityJson::NoEffect => "no_effect",
            EngineCapabilityJson::Partial => "partial",
            EngineCapabilityJson::Full => "full",
        });
        canon.push('|');
        // Reuse the exact source fragment that generates CardDef so a change
        // to the generated reducer definition necessarily changes the frozen
        // database identity. Evaluator semantics remain source/version gated.
        // `None` is included too, preserving positional separation and making
        // addition/removal equally visible.
        canon.push_str(generic_cost_reduction_for(&c.name));
        canon.push('|');
        // Reuse the exact generated source fragments plus the stable spell
        // recipe token. This covers every field selected by the generator for
        // `CardDef`; runtime primitive implementation changes remain pinned
        // separately by the source revision.
        canon.push_str(keywords_for(&c.name));
        canon.push('|');
        canon.push_str(alt_cost_for(&c.name));
        canon.push('|');
        canon.push_str(&kicker_cost_for(&c.name));
        canon.push('|');
        canon.push_str(additional_cost_for(&c.name));
        canon.push('|');
        canon.push_str(&flashback_for(&c.name));
        canon.push('|');
        canon.push_str(&activated_abilities_for(&c.name));
        canon.push('|');
        canon.push_str(&plot_cost_for(&c.name));
        canon.push('|');
        canon.push_str(&madness_cost_for(&c.name));
        canon.push('|');
        canon.push_str(&mode2_for(&c.name));
        canon.push('|');
        canon.push_str(&effect_recipe_for(c));
        canon.push('|');
        canon.push_str(&c.decks.join(","));
        canon.push('|');
        canon.push_str("special=");
        canon.push_str(&special_for(&c.name).canonical_token());
        canon.push('|');
        canon.push_str("activated=");
        canon.push_str(&activated_abilities_token(&c.name));
        canon.push('\n');
    }
    let hash = fnv1a64(canon.as_bytes());
    writeln!(out, "pub const KERNEL_CARDDB_HASH: u64 = 0x{hash:016x};").unwrap();

    out
}

/// Minimal ordered-unique-values collection so this file doesn't need a
/// `BTreeSet` import just for one dedup+sort of a handful of i32s.
struct BTreeSetLike(Vec<i32>);
impl FromIterator<i32> for BTreeSetLike {
    fn from_iter<T: IntoIterator<Item = i32>>(iter: T) -> Self {
        let mut v: Vec<i32> = iter.into_iter().collect();
        v.sort_unstable();
        v.dedup();
        BTreeSetLike(v)
    }
}
impl BTreeSetLike {
    fn values(&self) -> &[i32] {
        &self.0
    }
}

fn card_type_variant(t: &str) -> &'static str {
    match t {
        "Land" => "Land",
        "Creature" => "Creature",
        "Instant" => "Instant",
        "Sorcery" => "Sorcery",
        "Artifact" => "Artifact",
        "Enchantment" => "Enchantment",
        other => panic!("cards_v1.json: unknown card type {other:?}"),
    }
}

fn supertype_variant(t: &str) -> &'static str {
    match t {
        "Basic" => "Basic",
        "Snow" => "Snow",
        other => panic!("cards_v1.json: unknown supertype {other:?}"),
    }
}

/// Maps a `cards_v1.json` subtype string to a `card_def::Subtype` variant --
/// a fully closed set (one variant per distinct string across the whole
/// 135-card pool this increment's data covers), so this panics on an
/// unrecognized value same as `card_type_variant`/`supertype_variant`/
/// `color_variant` -- see `Subtype`'s own doc for why it's closed rather
/// than named-variants-plus-string-fallback (a `&'static str` payload can't
/// derive `Deserialize`, and `Subtype` is embedded in `effect::EffectOp`,
/// which needs to).
fn subtype_variant(t: &str) -> &'static str {
    match t {
        "Ape" => "Subtype::Ape",
        "Aura" => "Subtype::Aura",
        "BIRD" => "Subtype::BirdAllCaps",
        "Bird" => "Subtype::Bird",
        "Blood" => "Subtype::Blood",
        "Cat" => "Subtype::Cat",
        "Detective" => "Subtype::Detective",
        "Dragon" => "Subtype::Dragon",
        "Drone" => "Subtype::Drone",
        "Druid" => "Subtype::Druid",
        "Eldrazi" => "Subtype::Eldrazi",
        "Elf" => "Subtype::Elf",
        "Equipment" => "Subtype::Equipment",
        "FAERIE" => "Subtype::FaerieAllCaps",
        "Faerie" => "Subtype::Faerie",
        "Food" => "Subtype::Food",
        "Forest" => "Subtype::Forest",
        "Gate" => "Subtype::Gate",
        "Goblin" => "Subtype::Goblin",
        "HUMAN" => "Subtype::HumanAllCaps",
        "Hero" => "Subtype::Hero",
        "Human" => "Subtype::Human",
        "Hydra" => "Subtype::Hydra",
        "Island" => "Subtype::Island",
        "Knight" => "Subtype::Knight",
        "MONK" => "Subtype::Monk",
        "MOONFOLK" => "Subtype::Moonfolk",
        "Monkey" => "Subtype::Monkey",
        "Mountain" => "Subtype::Mountain",
        "Myr" => "Subtype::Myr",
        "NINJA" => "Subtype::NinjaAllCaps",
        "Ninja" => "Subtype::Ninja",
        "Ouphe" => "Subtype::Ouphe",
        "Pirate" => "Subtype::Pirate",
        "Plains" => "Subtype::Plains",
        "ROGUE" => "Subtype::RogueAllCaps",
        "Ranger" => "Subtype::Ranger",
        "Rat" => "Subtype::Rat",
        "Rogue" => "Subtype::Rogue",
        "SERPENT" => "Subtype::Serpent",
        "Saga" => "Subtype::Saga",
        "Samurai" => "Subtype::Samurai",
        "Shaman" => "Subtype::Shaman",
        "Shapeshifter" => "Subtype::Shapeshifter",
        "Soldier" => "Subtype::Soldier",
        "Spider" => "Subtype::Spider",
        "Spirit" => "Subtype::Spirit",
        "Swamp" => "Subtype::Swamp",
        "Toy" => "Subtype::Toy",
        "Treefolk" => "Subtype::Treefolk",
        "Vampire" => "Subtype::Vampire",
        "WIZARD" => "Subtype::WizardAllCaps",
        "Warrior" => "Subtype::Warrior",
        "Wizard" => "Subtype::Wizard",
        "Zombie" => "Subtype::Zombie",
        other => panic!("cards_v1.json: unknown subtype {other:?}"),
    }
}

fn color_variant(c: &str) -> &'static str {
    match c {
        "W" => "W",
        "U" => "U",
        "B" => "B",
        "R" => "R",
        "G" => "G",
        "C" => "C",
        other => panic!("cards_v1.json: unknown mana color {other:?}"),
    }
}

/// Parses a mana cost string like `"{1}{R}{R}"` into (pip expressions,
/// generic amount, X count). Pip expressions are emitted as literal Rust
/// source (e.g. `"Pip::Colored(ManaColor::R)"`) ready to splice into a
/// `&[...]` slice literal.
fn parse_cost(mana_cost: &str) -> (Vec<String>, u8, u8) {
    let mut pips = Vec::new();
    let mut generic: u32 = 0;
    let mut x_count: u8 = 0;

    let mut chars = mana_cost.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '{' {
            continue;
        }
        let mut tok = String::new();
        for c2 in chars.by_ref() {
            if c2 == '}' {
                break;
            }
            tok.push(c2);
        }

        if let Ok(n) = tok.parse::<u32>() {
            generic += n;
            continue;
        }
        if tok.eq_ignore_ascii_case("X") {
            x_count += 1;
            continue;
        }
        if let Some((a, b)) = tok.split_once('/') {
            if b.eq_ignore_ascii_case("P") {
                let color = color_variant(a);
                pips.push(format!("Pip::Phyrexian(ManaColor::{color})"));
            } else if a.chars().all(|ch| ch.is_ascii_digit()) {
                // Twobrid ({2/R}): not in the current pool. Approximate as
                // a colored pip; a future increment that needs twobrid
                // costs should give this its own Pip variant instead.
                let color = color_variant(b);
                pips.push(format!("Pip::Colored(ManaColor::{color})"));
            } else {
                let ca = color_variant(a);
                let cb = color_variant(b);
                pips.push(format!("Pip::Hybrid(ManaColor::{ca}, ManaColor::{cb})"));
            }
            continue;
        }
        let color = color_variant(tok.as_str());
        pips.push(format!("Pip::Colored(ManaColor::{color})"));
    }

    if generic > u8::MAX as u32 {
        panic!("cards_v1.json: generic cost {generic} overflows u8 in {mana_cost:?}");
    }
    (pips, generic as u8, x_count)
}

fn fnv1a64(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &b in data {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}
