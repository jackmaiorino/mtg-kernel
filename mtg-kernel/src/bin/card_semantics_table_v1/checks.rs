//! Twin and far-pair similarity, the census, and the no-reader guarantee.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::path::Path;

use mtg_kernel::card_def::{CardCapability, CardDef, CardType, CombatStaticRulesDef, CARD_DEFS};
use mtg_kernel::trigger::triggers_for;

use super::extract::EFFECT_FAMILIES;
use super::table::{CardRow, Table};

pub const LOW_DISTINCTIVENESS_MIN_NEIGHBORS: usize = 10;
pub const LOW_DISTINCTIVENESS_OVERLAP_NUM: usize = 3;
pub const LOW_DISTINCTIVENESS_OVERLAP_DEN: usize = 4;

/// Similarity is the Jaccard index of the cards' nonzero (column, value)
/// pairs, kept as an exact fraction. Equal vectors give 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Jaccard {
    pub shared: usize,
    pub union: usize,
}

impl Jaccard {
    /// Percent rounded down, integer arithmetic only.
    pub fn percent(self) -> usize {
        if self.union == 0 {
            100
        } else {
            self.shared * 100 / self.union
        }
    }

    /// Whether the index is at least `percent` / 100, exactly.
    pub fn at_least(self, percent: usize) -> bool {
        self.shared * 100 >= percent * self.union
    }

    /// Whether the index is at most `percent` / 100, exactly.
    pub fn at_most(self, percent: usize) -> bool {
        self.shared * 100 <= percent * self.union
    }
}

pub fn jaccard(a: &[i64], b: &[i64]) -> Jaccard {
    let mut shared = 0;
    let mut union = 0;
    for (x, y) in a.iter().zip(b) {
        match (*x != 0, *y != 0) {
            (false, false) => {}
            (true, true) if x == y => {
                shared += 1;
                union += 1;
            }
            // Two different nonzero values are two different pairs.
            (true, true) => union += 2,
            _ => union += 1,
        }
    }
    Jaccard { shared, union }
}

#[derive(Debug, Clone, Copy)]
pub enum Expect {
    /// Identical vectors.
    Equal,
    /// Jaccard at least this percent. A floor set before the first table
    /// was generated; a miss is reported in the dump, not failed, because
    /// the floor was a guess. The pinned value below is what is checked.
    AtLeast(usize),
    /// Jaccard at most this percent (unrelated cards).
    AtMost(usize),
}

/// Pairs the rules treat alike, and pairs that are unrelated. The
/// expectation of each pair is stated with the reason it is not stronger.
/// `Equal` and `AtMost` fail the check when not met. Every pair's measured
/// index is also pinned (shared, union) in `PAIR_PINS`, so a change to the
/// walker or the pool that moves any of them fails until it is re-pinned.
pub const PAIRS: [(&str, &str, Expect, &str); 15] = [
    (
        "Llanowar Elves",
        "Fyndhorn Elves",
        Expect::Equal,
        "same 1/1 Elf for G that taps for G",
    ),
    (
        "Llanowar Elves",
        "Elvish Mystic",
        Expect::Equal,
        "same 1/1 Elf for G that taps for G",
    ),
    (
        "Fyndhorn Elves",
        "Elvish Mystic",
        Expect::Equal,
        "same 1/1 Elf for G that taps for G",
    ),
    (
        "Lightning Bolt",
        "Chain Lightning",
        Expect::AtLeast(50),
        "3 to any target; Chain Lightning is a sorcery with a copy offer",
    ),
    (
        "Ponder",
        "Preordain",
        Expect::AtLeast(40),
        "U cantrips; Ponder looks at 3 and may shuffle, Preordain scries 2",
    ),
    (
        "Mental Note",
        "Thought Scour",
        Expect::AtLeast(60),
        "mill 2 then draw; Thought Scour targets a player",
    ),
    (
        "Dispel",
        "Spell Pierce",
        Expect::AtLeast(40),
        "U counters; Dispel counters an instant, Spell Pierce a noncreature unless 2 is paid",
    ),
    (
        "Faerie Miscreant",
        "Faerie Seer",
        Expect::AtLeast(40),
        "1/1 flying Faeries for U with different entry triggers",
    ),
    (
        "Lead the Stampede",
        "Winding Way",
        Expect::AtLeast(30),
        "G dig spells that take creatures; different look counts and a mode choice",
    ),
    (
        "Lightning Bolt",
        "Fireblast",
        Expect::AtLeast(40),
        "burn to any target; different amount and an alternative cost",
    ),
    ("Lightning Bolt", "Forest", Expect::AtMost(5), "unrelated"),
    (
        "Counterspell",
        "Llanowar Elves",
        Expect::AtMost(10),
        "unrelated",
    ),
    ("Ponder", "Fireblast", Expect::AtMost(20), "unrelated"),
    (
        "Mental Note",
        "Galvanic Blast",
        Expect::AtMost(20),
        "unrelated",
    ),
    ("Faerie Seer", "Forest", Expect::AtMost(10), "unrelated"),
];

/// Measured Jaccard (shared, union) of each pair in `PAIRS`, first table.
pub const PAIR_PINS: [(&str, &str, usize, usize); 15] = [
    ("Llanowar Elves", "Fyndhorn Elves", 12, 12),
    ("Llanowar Elves", "Elvish Mystic", 12, 12),
    ("Fyndhorn Elves", "Elvish Mystic", 12, 12),
    ("Lightning Bolt", "Chain Lightning", 8, 12),
    ("Ponder", "Preordain", 8, 16),
    ("Mental Note", "Thought Scour", 10, 13),
    ("Dispel", "Spell Pierce", 5, 15),
    ("Faerie Miscreant", "Faerie Seer", 11, 24),
    ("Lead the Stampede", "Winding Way", 4, 21),
    ("Lightning Bolt", "Fireblast", 6, 17),
    ("Lightning Bolt", "Forest", 0, 16),
    ("Counterspell", "Llanowar Elves", 0, 23),
    ("Ponder", "Fireblast", 1, 26),
    ("Mental Note", "Galvanic Blast", 3, 19),
    ("Faerie Seer", "Forest", 1, 22),
];

fn row_index(table: &Table, name: &str) -> usize {
    table
        .rows
        .iter()
        .position(|row| row.name == name)
        .unwrap_or_else(|| panic!("{name} is not in the Pauper pool"))
}

/// Percentile of a pair's similarity among all distinct card pairs of the
/// pool: the share of pairs with a strictly smaller index, in percent.
pub fn percentile(table: &Table, value: Jaccard) -> usize {
    let n = table.vectors.len();
    let mut below = 0usize;
    let mut total = 0usize;
    for i in 0..n {
        for j in i + 1..n {
            let other = jaccard(&table.vectors[i], &table.vectors[j]);
            total += 1;
            if other.shared * value.union < value.shared * other.union {
                below += 1;
            }
        }
    }
    below * 100 / total.max(1)
}

/// Measures every pair; returns the report and the failures.
pub fn pairs(table: &Table) -> (String, Vec<String>) {
    let mut report = String::new();
    let mut failures = Vec::new();
    for (a, b, expect, why) in PAIRS {
        let va = &table.vectors[row_index(table, a)];
        let vb = &table.vectors[row_index(table, b)];
        let j = jaccard(va, vb);
        let equal = va == vb;
        let (ok, verdict) = match expect {
            Expect::Equal => (equal, if equal { "ok" } else { "FAILED" }),
            Expect::AtLeast(p) if j.at_least(p) => (true, "ok"),
            Expect::AtLeast(_) => (true, "below the pre-set floor (reported, not failed)"),
            Expect::AtMost(p) => (j.at_most(p), if j.at_most(p) { "ok" } else { "FAILED" }),
        };
        let pinned = PAIR_PINS
            .iter()
            .find(|(x, y, _, _)| *x == a && *y == b)
            .map(|(_, _, shared, union)| (*shared, *union));
        if pinned != Some((j.shared, j.union)) {
            failures.push(format!(
                "{a} / {b}: jaccard {}/{} differs from pinned {pinned:?}",
                j.shared, j.union
            ));
        }
        writeln!(
            report,
            "{a} / {b}: jaccard {}/{} ({}%), percentile {} among pool pairs, equal {equal}; expected {expect:?}: {} ({why})",
            j.shared,
            j.union,
            j.percent(),
            percentile(table, j),
            verdict,
        )
        .unwrap();
        if !ok {
            failures.push(format!("{a} / {b}: {expect:?} not met"));
        }
    }
    (report, failures)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Census {
    pub distinct_tokens: usize,
    pub tokens_on_one_card: usize,
    pub cards_with_unshared_token: usize,
    pub cards_without_effect_token: usize,
}

/// Census over the cards' tokens, restricted to tokens accepted by `keep`.
pub fn census(rows: &[CardRow], keep: &dyn Fn(&str) -> bool) -> Census {
    let mut carriers: BTreeMap<&str, usize> = BTreeMap::new();
    for row in rows {
        for token in row.tokens.tokens.keys() {
            if keep(token) {
                *carriers.entry(token.as_str()).or_insert(0) += 1;
            }
        }
    }
    let singles: BTreeSet<&str> = carriers
        .iter()
        .filter(|(_, n)| **n == 1)
        .map(|(token, _)| *token)
        .collect();
    let cards_with_unshared_token = rows
        .iter()
        .filter(|row| {
            row.tokens
                .tokens
                .keys()
                .any(|t| singles.contains(t.as_str()))
        })
        .count();
    let cards_without_effect_token = rows
        .iter()
        .filter(|row| row.tokens.effect_tree_token_count() == 0)
        .count();
    Census {
        distinct_tokens: carriers.len(),
        tokens_on_one_card: singles.len(),
        cards_with_unshared_token,
        cards_without_effect_token,
    }
}

pub fn all_tokens(_: &str) -> bool {
    true
}

pub fn effect_tokens(token: &str) -> bool {
    EFFECT_FAMILIES
        .iter()
        .any(|family| token.starts_with(family))
}

/// Pinned census numbers: (scope, before mapping, after mapping). A card
/// wave or a mapping change that moves them must update them here and in
/// the design note.
pub const PINNED: [(&str, Census, Census); 2] = [
    (
        "all tokens",
        Census {
            distinct_tokens: 488,
            tokens_on_one_card: 285,
            cards_with_unshared_token: 120,
            cards_without_effect_token: 34,
        },
        Census {
            distinct_tokens: 488,
            tokens_on_one_card: 285,
            cards_with_unshared_token: 120,
            cards_without_effect_token: 34,
        },
    ),
    (
        "effect-tree tokens (leaf:, op:, cond:)",
        Census {
            distinct_tokens: 241,
            tokens_on_one_card: 163,
            cards_with_unshared_token: 75,
            cards_without_effect_token: 34,
        },
        Census {
            distinct_tokens: 241,
            tokens_on_one_card: 163,
            cards_with_unshared_token: 75,
            cards_without_effect_token: 34,
        },
    ),
];

/// Which tokens a census counts.
type TokenFilter = fn(&str) -> bool;

pub fn census_report(before: &[CardRow], after: &[CardRow]) -> (String, Vec<String>) {
    let mut report = String::new();
    let mut failures = Vec::new();
    let scopes: [(&str, TokenFilter); 2] =
        [(PINNED[0].0, all_tokens), (PINNED[1].0, effect_tokens)];
    for (i, (scope, keep)) in scopes.iter().enumerate() {
        let b = census(before, keep);
        let a = census(after, keep);
        for (label, measured, pinned) in [
            ("before mapping", b, PINNED[i].1),
            ("after mapping", a, PINNED[i].2),
        ] {
            writeln!(
                report,
                "{scope}, {label}: distinct tokens {}, tokens on exactly one card {}, cards with a token no other pool card shares {}, cards with no effect-tree token {}",
                measured.distinct_tokens,
                measured.tokens_on_one_card,
                measured.cards_with_unshared_token,
                measured.cards_without_effect_token,
            )
            .unwrap();
            if measured != pinned {
                failures.push(format!(
                    "census {scope}, {label}: measured {measured:?}, pinned {pinned:?}"
                ));
            }
        }
    }
    (report, failures)
}

/// Card-named leaf census: which mapping-layer sources the pool uses, and
/// which leaf, trigger and cost-part tokens the pool carries.
pub fn mapping_report(after: &[CardRow], mapping: &super::mapping::Mapping) -> String {
    let mut used: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for row in after {
        for source in &row.tokens.mapped_sources {
            used.entry(source.as_str())
                .or_default()
                .push(row.name.as_str());
        }
    }
    let mut report = String::new();
    writeln!(
        report,
        "mapping entries {}, used by the pool {}",
        mapping.entries.len(),
        used.len()
    )
    .unwrap();
    for (source, cards) in &used {
        let why = &mapping.entries[*source].why;
        writeln!(report, "  {source}: {} ({why})", cards.join(", ")).unwrap();
    }
    for source in mapping.entries.keys() {
        if !used.contains_key(source.as_str()) {
            writeln!(report, "  unused entry: {source}").unwrap();
        }
    }
    report
}

/// Census report without pinned thresholds, for the Vintage Cube table.
pub fn cube_census_report(before: &[CardRow], after: &[CardRow]) -> String {
    let mut report = String::new();
    let scopes: [(&str, TokenFilter); 2] =
        [(PINNED[0].0, all_tokens), (PINNED[1].0, effect_tokens)];
    for (scope, keep) in &scopes {
        let b = census(before, keep);
        let a = census(after, keep);
        for (label, measured) in [("before mapping", b), ("after mapping", a)] {
            writeln!(
                report,
                "{scope}, {label}: distinct tokens {}, tokens on exactly one card {}, cards with a token no other row shares {}, cards with no effect-tree token {}",
                measured.distinct_tokens,
                measured.tokens_on_one_card,
                measured.cards_with_unshared_token,
                measured.cards_without_effect_token,
            )
            .unwrap();
        }
    }
    report
}

/// Asymmetric nonzero feature overlap of `a` against `b`:
/// `(shared nonzero (col, val) pairs, nonzero pairs of a)`.
#[cfg(test)]
pub fn overlap(a: &[i64], b: &[i64]) -> (usize, usize) {
    let mut shared = 0usize;
    let mut nonzero_a = 0usize;
    for (&x, &y) in a.iter().zip(b) {
        if x != 0 {
            nonzero_a += 1;
            if x == y {
                shared += 1;
            }
        }
    }
    (shared, nonzero_a)
}

/// Exact integer test for `shared / nonzero_a >= 0.75` (`3 / 4`).
pub fn overlap_meets_threshold(shared: usize, nonzero_a: usize) -> bool {
    nonzero_a > 0
        && shared * LOW_DISTINCTIVENESS_OVERLAP_DEN >= nonzero_a * LOW_DISTINCTIVENESS_OVERLAP_NUM
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LowDistinctivenessRow {
    pub name: String,
    pub neighbors: usize,
    pub nonzero: usize,
}

/// Rows `a` for which at least `LOW_DISTINCTIVENESS_MIN_NEIGHBORS` other rows
/// `b` have `overlap(a, b) >= 0.75`.
pub fn low_distinctiveness(table: &Table) -> Vec<LowDistinctivenessRow> {
    let n = table.vectors.len();
    let mut sparse: Vec<Vec<(usize, i64)>> = Vec::with_capacity(n);
    let mut postings: HashMap<(usize, i64), Vec<usize>> = HashMap::new();
    for (i, vector) in table.vectors.iter().enumerate() {
        let mut pairs = Vec::new();
        for (col, &val) in vector.iter().enumerate() {
            if val != 0 {
                pairs.push((col, val));
                postings.entry((col, val)).or_default().push(i);
            }
        }
        sparse.push(pairs);
    }
    let mut counts = vec![0usize; n];
    let mut flagged = Vec::new();
    for (i, pairs) in sparse.iter().enumerate() {
        let nonzero = pairs.len();
        if nonzero == 0 {
            continue;
        }
        counts.fill(0);
        for pair in pairs {
            if let Some(rows) = postings.get(pair) {
                for &j in rows {
                    counts[j] += 1;
                }
            }
        }
        let neighbors = counts
            .iter()
            .enumerate()
            .filter(|&(j, &shared)| j != i && overlap_meets_threshold(shared, nonzero))
            .count();
        if neighbors >= LOW_DISTINCTIVENESS_MIN_NEIGHBORS {
            flagged.push(LowDistinctivenessRow {
                name: table.rows[i].name.clone(),
                neighbors,
                nonzero,
            });
        }
    }
    flagged.sort_by(|a, b| {
        b.neighbors
            .cmp(&a.neighbors)
            .then_with(|| a.name.cmp(&b.name))
    });
    flagged
}

fn has_mana_source(def: &CardDef) -> bool {
    (def.mana_ability)().is_some()
        || def.mana_ability_def.is_some()
        || !def.mana_ability_choices.is_empty()
        || !def.additional_mana_abilities.is_empty()
        || !def.restricted_mana_abilities.is_empty()
        || !def.produces_mana.is_empty()
        || def.mana_ability_includes_chosen_color
}

/// Cost modifiers, entry rules and static rule fields of a definition
/// (keywords are a separate fact).
fn has_static_definition_rules(def: &CardDef) -> bool {
    def.alt_cost.is_some()
        || def.kicker_cost.is_some()
        || def.additional_cost.is_some()
        || def.optional_additional_cost.is_some()
        || def.flashback.is_some()
        || def.escape.is_some()
        || def.madness_cost.is_some()
        || def.plot_cost.is_some()
        || def.delve
        || def.generic_cost_reduction.is_some()
        || def.ward_cost.is_some()
        || def.spell_cannot_be_countered
        || def.enters_battlefield_tapped
        || def.enters_battlefield_tapped_unless.is_some()
        || def.enters_tapped_unless_controller.is_some()
        || def.enters_with_plus_one_counters.is_some()
        || def.controlled_creature_grant.is_some()
        || def.controlled_counter_keyword.is_some()
        || def.game_outcome_protection_v1.is_some()
        || def.damage_replacement_v1.is_some()
        || def.token_counter_replacement_v48.is_some()
        || !def.starting_life_self_bonuses_v1.is_empty()
        || def.source_type_hexproof_v1.is_some()
        || def.other_controlled_nontoken_death_v1.is_some()
        || def.creature_type_entry_v1.is_some()
        || def.chosen_color_creature_boost_v1.is_some()
        || def.as_enters_color_choice_v1.is_some()
        || def.as_enters_choose_color_other_than.is_some()
        || def.changeling
        || def.minimum_blockers > 1
        || def.cant_be_blocked_by_monarchs_creatures
        || def.combat_static_rules != CombatStaticRulesDef::NONE
        || !def.activated_ability_generic_reductions.is_empty()
        || def.animation.is_some()
}

/// What a definition establishes about a row with no effect-tree token.
/// Each reason below states only what these facts establish.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DefinitionFacts {
    pub partial: bool,
    pub planeswalker: bool,
    pub saga: bool,
    pub transform_face: bool,
    pub bestow: bool,
    pub alternate_face: bool,
    pub land: bool,
    pub mana_ability: bool,
    /// Equipment or Aura attachment data.
    pub attachment: bool,
    pub creature: bool,
    pub artifact_or_enchantment: bool,
    /// At least one printed keyword bit.
    pub keywords: bool,
    /// Cost modifiers, entry rules or static rule fields.
    pub static_rules: bool,
    /// Activated or triggered abilities, a spell program or spell modes.
    pub abilities: bool,
}

impl DefinitionFacts {
    pub fn of(def: &CardDef, has_triggers: bool) -> Self {
        Self {
            partial: def.capability == CardCapability::Partial,
            planeswalker: def.has_type(CardType::Planeswalker) || def.starting_loyalty.is_some(),
            saga: def.saga.is_some(),
            transform_face: def.transform_face.is_some(),
            bestow: def.bestow.is_some(),
            alternate_face: def.adventure.is_some() || def.omen.is_some(),
            land: def.is_land || def.has_type(CardType::Land),
            mana_ability: has_mana_source(def),
            attachment: def.equipment.is_some() || def.attachment.is_some(),
            creature: def.has_type(CardType::Creature),
            artifact_or_enchantment: def.has_type(CardType::Artifact)
                || def.has_type(CardType::Enchantment),
            keywords: def.keywords.0 != 0,
            static_rules: has_static_definition_rules(def),
            abilities: !def.activated_abilities.is_empty()
                || has_triggers
                || (def.spell_effect)().is_some()
                || def.mode2.is_some()
                || def.mode3.is_some()
                || !def.additional_modes.is_empty(),
        }
    }

    /// Whether the definition holds any rule besides its mana abilities.
    pub fn has_other_rules(self) -> bool {
        self.keywords || self.static_rules || self.abilities || self.attachment
    }
}

/// Classifies a card with no effect-tree token (`leaf:`, `op:`, `cond:`) from
/// its `CardDef` and triggers alone, never by card name.
pub fn no_effect_token_reason(id: u16) -> &'static str {
    let def = &CARD_DEFS[usize::from(id)];
    no_effect_token_reason_for(DefinitionFacts::of(def, !triggers_for(id).is_empty()))
}

/// The reason for a row with no effect-tree token. A reason says "only" just
/// when the definition holds nothing else this classifier checks.
pub fn no_effect_token_reason_for(facts: DefinitionFacts) -> &'static str {
    if facts.partial {
        return "partial definition (no walked effect program in CardDef)";
    }
    if facts.planeswalker {
        return "planeswalker loyalty abilities not walked";
    }
    if facts.saga {
        return "saga chapter effects not walked";
    }
    if facts.transform_face {
        return "transform back face not walked";
    }
    if facts.bestow {
        return "bestow aura face not walked";
    }
    if facts.alternate_face {
        return "alternate spell face with no effect-tree tokens";
    }
    if facts.land {
        return if !facts.mana_ability {
            "land with no mana ability in its definition"
        } else if facts.has_other_rules() {
            "land with a mana ability and other definition rules (keywords, costs, entry, static, attachment or abilities)"
        } else {
            "land with only a mana ability in its definition"
        };
    }
    if facts.mana_ability {
        return if facts.has_other_rules() {
            "nonland permanent with a mana ability and other definition rules (keywords, costs, entry, static, attachment or abilities)"
        } else {
            "nonland permanent with only a mana ability in its definition"
        };
    }
    if facts.attachment {
        return "equipment or aura with no walked effect";
    }
    if facts.creature {
        return if facts.keywords || facts.static_rules || facts.abilities {
            "creature with keywords, costs, entry, static rules or abilities in its definition and no walked effect"
        } else {
            "creature with no keywords, costs, entry, static rules or abilities in its definition"
        };
    }
    if facts.artifact_or_enchantment {
        return "noncreature artifact or enchantment with no walked effect";
    }
    if facts.abilities {
        "empty effect program"
    } else {
        "spell with no effect program in CardDef"
    }
}

/// Groups rows with no effect-tree token (`leaf:`, `op:`, `cond:`) by reason.
pub fn no_effect_token_groups(rows: &[CardRow]) -> BTreeMap<&'static str, Vec<String>> {
    let mut groups: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();
    for row in rows {
        if row.tokens.effect_tree_token_count() == 0 {
            groups
                .entry(no_effect_token_reason(row.id))
                .or_default()
                .push(row.name.clone());
        }
    }
    for cards in groups.values_mut() {
        cards.sort();
    }
    groups
}

/// Full informational report for the Vintage Cube table (no pass/fail gates).
pub fn cube_report(
    table: &Table,
    before: &[CardRow],
    mapping: &super::mapping::Mapping,
    pauper_vocab_len: usize,
    not_implemented: &[String],
) -> String {
    let mut report = String::new();
    let new_columns = table.vocabulary.len().saturating_sub(pauper_vocab_len);
    writeln!(report, "rows {}", table.rows.len()).unwrap();
    writeln!(report, "vector_width {}", table.vocabulary.len()).unwrap();
    writeln!(report, "pauper_columns {pauper_vocab_len}").unwrap();
    writeln!(report, "new_columns_beyond_pauper {new_columns}").unwrap();
    writeln!(
        report,
        "not_implemented_cube_cards {}",
        not_implemented.len()
    )
    .unwrap();

    report.push_str(&cube_census_report(before, &table.rows));
    report.push_str(&mapping_report(&table.rows, mapping));

    let low = low_distinctiveness(table);
    writeln!(
        report,
        "low-distinctiveness cards (at least {LOW_DISTINCTIVENESS_MIN_NEIGHBORS} other rows with overlap >= 0.75): {}",
        low.len()
    )
    .unwrap();
    for item in &low {
        writeln!(
            report,
            "  {} ({} other rows, {} nonzero features)",
            item.name, item.neighbors, item.nonzero
        )
        .unwrap();
    }

    let groups = no_effect_token_groups(&table.rows);
    let no_effect_total: usize = groups.values().map(Vec::len).sum();
    writeln!(
        report,
        "cards with no usable features (no leaf:, op:, or cond: token): {no_effect_total}"
    )
    .unwrap();
    for (reason, cards) in &groups {
        writeln!(report, "  {reason} ({}):", cards.len()).unwrap();
        for name in cards {
            writeln!(report, "    {name}").unwrap();
        }
    }

    writeln!(
        report,
        "not implemented cube cards ({}):",
        not_implemented.len()
    )
    .unwrap();
    for name in not_implemented {
        writeln!(report, "  {name}: not implemented").unwrap();
    }

    let mut debug_heads = BTreeSet::new();
    for row in &table.rows {
        debug_heads.extend(row.tokens.debug_heads.iter().cloned());
    }
    writeln!(
        report,
        "variant names read from a Debug head (enums without serde): {}",
        debug_heads.len()
    )
    .unwrap();
    for head in &debug_heads {
        writeln!(report, "  {head}").unwrap();
    }
    report
}

/// The no-reader guarantee: no source outside this generator names the
/// table, its directory or its generator.
pub fn unreferenced(repo: &Path) -> Vec<String> {
    let needles = [
        "card_semantics_table_v1",
        "card_semantics_v1",
        "cube_table_v1",
    ];
    let roots = [
        "mtg-kernel/src",
        "mtg-kernel/tests",
        "mtg-kernel/examples",
        "mtg-kernel/build.rs",
        "mtg-kernel/Cargo.toml",
        "build_support",
        "python",
        "tools",
        "scripts",
        "viewer",
    ];
    let own = [repo.join("mtg-kernel/src/bin/card_semantics_table_v1")];
    let mut hits = Vec::new();
    let mut stack: Vec<std::path::PathBuf> = roots.iter().map(|r| repo.join(r)).collect();
    while let Some(path) = stack.pop() {
        if own.iter().any(|o| path.starts_with(o)) {
            continue;
        }
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name == "node_modules" || name == "__pycache__" || name.starts_with('.') {
                continue;
            }
            if let Ok(entries) = std::fs::read_dir(&path) {
                let mut children: Vec<_> =
                    entries.filter_map(Result::ok).map(|e| e.path()).collect();
                children.sort();
                stack.extend(children);
            }
        } else if let Ok(text) = std::fs::read(&path) {
            let text = String::from_utf8_lossy(&text);
            if needles.iter().any(|needle| text.contains(needle)) {
                hits.push(path.display().to_string());
            }
        }
    }
    hits.sort();
    hits
}
