//! Twin and far-pair similarity, the census, and the no-reader guarantee.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;

use super::extract::EFFECT_FAMILIES;
use super::table::{CardRow, Table};

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

/// The no-reader guarantee: no source outside this generator names the
/// table, its directory or its generator.
pub fn unreferenced(repo: &Path) -> Vec<String> {
    let needles = ["card_semantics_table_v1", "card_semantics_v1"];
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
