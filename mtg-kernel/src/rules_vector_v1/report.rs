//! Human-readable audit report for one build. Presentation only: card names
//! appear here for readers and never feed back into the table.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use super::sources::{card_rules, PAUPER_REGISTRY_LEN_V1};
use super::*;

fn name(id: u16) -> &'static str {
    CARD_DEFS[usize::from(id)].name
}

fn names(ids: &[u16]) -> String {
    ids.iter()
        .map(|&id| name(id))
        .collect::<Vec<_>>()
        .join(", ")
}

fn pct(n: usize, d: usize) -> String {
    if d == 0 {
        "n/a".to_string()
    } else {
        format!("{n}/{d} ({:.1}%)", 100.0 * n as f64 / d as f64)
    }
}

/// The manifest written next to the table.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ManifestV1 {
    pub schema: &'static str,
    pub build: &'static str,
    pub extractor_sha256: String,
    pub extractor_files: Vec<(String, String)>,
    pub output_sha256: String,
    pub kernel_carddb_hash: String,
    pub cards: usize,
    pub vocabulary: usize,
}

pub fn manifest(table: &RulesTableV1) -> ManifestV1 {
    ManifestV1 {
        schema: table.schema,
        build: table.build,
        extractor_sha256: table.extractor_sha256.clone(),
        extractor_files: EXTRACTOR_SOURCES_V1
            .iter()
            .map(|(path, text)| (path.to_string(), sha256_hex(text.as_bytes())))
            .collect(),
        output_sha256: table.output_sha256(),
        kernel_carddb_hash: table.kernel_carddb_hash.clone(),
        cards: table.cards.len(),
        vocabulary: table.vocabulary.len(),
    }
}

pub fn report_markdown(table: &RulesTableV1) -> String {
    let mut md = String::new();
    let m = manifest(table);
    let decks = runtime_deck_cards();
    let nine: BTreeSet<u16> = decks.values().flatten().copied().collect();

    writeln!(md, "# Rules vector v1 audit ({})\n", table.build).unwrap();
    writeln!(md, "- Extractor sha256: `{}`", m.extractor_sha256).unwrap();
    writeln!(md, "- Output sha256: `{}`", m.output_sha256).unwrap();
    writeln!(md, "- KERNEL_CARDDB_HASH: `{}`", m.kernel_carddb_hash).unwrap();
    writeln!(
        md,
        "- Cards: {}; vocabulary (Pauper registry features): {}\n",
        m.cards, m.vocabulary
    )
    .unwrap();

    // Coverage and opaque rules.
    writeln!(md, "## Coverage\n").unwrap();
    let mut no_abilities = Vec::new();
    let mut opaque = Vec::new();
    for row in &table.cards {
        let rules = card_rules(row.id);
        if rules.abilities.is_empty() {
            no_abilities.push(row.id);
        }
        for rule in &rules.opaque_rules {
            opaque.push(format!("{}: {rule}", name(row.id)));
        }
    }
    writeln!(
        md,
        "Cards with printed facts only (no ability facts): {}\n",
        no_abilities.len()
    )
    .unwrap();
    writeln!(md, "> {}\n", names(&no_abilities)).unwrap();
    writeln!(
        md,
        "Opaque engine rules (implemented in code, not readable as data): {}\n",
        opaque.len()
    )
    .unwrap();
    for line in &opaque {
        writeln!(md, "- {line}").unwrap();
    }

    // Equivalence.
    writeln!(md, "\n## Equivalence (nine-deck cards)\n").unwrap();
    let reprints = reprint_classes(table, &nine);
    writeln!(
        md,
        "Functional reprint classes (identical rules records): {}",
        reprints.len()
    )
    .unwrap();
    for class in &reprints {
        writeln!(md, "- {}", names(class)).unwrap();
    }
    let lossy = lossy_collisions(table, &nine);
    writeln!(
        md,
        "\nLossy collisions (same features, different records): {}",
        lossy.len()
    )
    .unwrap();
    for class in &lossy {
        writeln!(md, "- {}", names(class)).unwrap();
    }

    // Information content.
    writeln!(md, "\n## Transfer and information content\n").unwrap();
    let (unique, total) = unique_part_share(table);
    writeln!(
        md,
        "- Pauper non-token cards holding a feature no other card has: {} (EffectOp-name baseline: 47%)",
        pct(unique, total)
    )
    .unwrap();
    if table.cards.len() > PAUPER_REGISTRY_LEN_V1 {
        let (oov, fdn) = oov_card_share(table);
        writeln!(
            md,
            "- FDN non-token cards with a feature outside the Pauper vocabulary: {} (gate 5%; EffectOp-name baseline 23%)",
            pct(oov, fdn)
        )
        .unwrap();
        oov_breakdown(table, &mut md);
    } else {
        writeln!(
            md,
            "- FDN transfer: run the `limited-fdn-fixtures` build for this row."
        )
        .unwrap();
    }
    writeln!(
        md,
        "\nShare of each deck's features that also occur in some other deck:\n"
    )
    .unwrap();
    writeln!(md, "| Deck | Features | Shared with another deck |").unwrap();
    writeln!(md, "| --- | --- | --- |").unwrap();
    let deck_features: BTreeMap<&str, BTreeSet<&String>> = decks
        .iter()
        .map(|(deck, ids)| {
            let keys = ids
                .iter()
                .flat_map(|&id| table.cards[usize::from(id)].features.keys())
                .collect();
            (*deck, keys)
        })
        .collect();
    for (deck, keys) in &deck_features {
        let shared = keys
            .iter()
            .filter(|k| {
                deck_features
                    .iter()
                    .any(|(other, ks)| other != deck && ks.contains(*k))
            })
            .count();
        writeln!(
            md,
            "| {deck} | {} | {} |",
            keys.len(),
            pct(shared, keys.len())
        )
        .unwrap();
    }

    // Transfer sources for the Spy line.
    writeln!(md, "\n## Transfer sources for the Spy line\n").unwrap();
    writeln!(md, "For each effect, read and target feature of the three combo cards, the other nine-deck cards that share it.\n").unwrap();
    for combo in ["Balustrade Spy", "Dread Return", "Lotleth Giant"] {
        let Some(id) = crate::card_def::card_id_by_name(combo) else {
            continue;
        };
        writeln!(md, "### {combo}\n").unwrap();
        for key in table.cards[usize::from(id)].features.keys() {
            if !(key.starts_with("e:")
                || key.starts_with("r:")
                || key.starts_with("t:")
                || key.starts_with("k:")
                || key.starts_with("cast_from"))
            {
                continue;
            }
            let others: Vec<u16> = nine
                .iter()
                .copied()
                .filter(|&o| o != id && table.cards[usize::from(o)].features.contains_key(key))
                .collect();
            let shown: Vec<&str> = others.iter().take(6).map(|&o| name(o)).collect();
            let more = others.len().saturating_sub(shown.len());
            let tail = if more > 0 {
                format!(" and {more} more")
            } else {
                String::new()
            };
            writeln!(
                md,
                "- `{key}`: {}{tail}",
                if shown.is_empty() {
                    "none".to_string()
                } else {
                    shown.join(", ")
                }
            )
            .unwrap();
        }
        writeln!(md).unwrap();
    }

    // Neighbours (diagnostic only; never a gate to tune against).
    writeln!(md, "## Nearest neighbours (diagnostic only)\n").unwrap();
    writeln!(
        md,
        "Cosine on L2-normalised log counts over the Pauper vocabulary, k = 5, nine-deck cards.\n"
    )
    .unwrap();
    let vectors: Vec<Vec<f64>> = table
        .cards
        .iter()
        .map(|row| vectorize::dense_vector(&row.features, &table.vocabulary).0)
        .collect();
    for &id in &nine {
        let mut sims: Vec<(f64, u16)> = nine
            .iter()
            .copied()
            .filter(|&o| o != id)
            .map(|o| {
                (
                    vectorize::cosine(&vectors[usize::from(id)], &vectors[usize::from(o)]),
                    o,
                )
            })
            .collect();
        sims.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        let top: Vec<String> = sims
            .iter()
            .take(5)
            .map(|(s, o)| format!("{} {:.2}", name(*o), s))
            .collect();
        writeln!(md, "- **{}**: {}", name(id), top.join("; ")).unwrap();
    }

    // Backward chaining.
    writeln!(md, "\n## Backward chaining from \"opponent loses life\"\n").unwrap();
    writeln!(md, "Goal regression over facets, depth 5. Costs are not regressed in v1. A line is scaling when its damage amount is read from the game state.\n").unwrap();
    writeln!(md, "| Deck | Lines | Scaling lines |").unwrap();
    writeln!(md, "| --- | --- | --- |").unwrap();
    let mut spy_lines = Vec::new();
    for (deck, ids) in &decks {
        let ids: Vec<u16> = ids.iter().copied().collect();
        let lines = chain::lines_for_deck(&ids, 5);
        let scaling = lines.iter().filter(|l| l.scaling).count();
        writeln!(md, "| {deck} | {} | {scaling} |", lines.len()).unwrap();
        if *deck == "Spy" {
            spy_lines = lines;
        }
    }
    writeln!(md, "\nSpy deck scaling lines (cards used):\n").unwrap();
    let mut seen = BTreeSet::new();
    for line in spy_lines.iter().filter(|l| l.scaling) {
        if seen.insert(line.cards.clone()) {
            writeln!(md, "- {}", names(&line.cards)).unwrap();
        }
    }
    md
}

/// The family of a feature key: printed fact, ability context, a pair, a
/// full effect tuple, or a marginal.
fn family(key: &str) -> &'static str {
    let key = key.strip_prefix("tok/").unwrap_or(key);
    if key.starts_with("p:") {
        "printed"
    } else if key.starts_with("pair:") {
        "within-ability pair"
    } else if key.starts_with("c:") {
        "context x effect"
    } else if key.starts_with("e:") && key.matches('|').count() >= 5 {
        "full effect tuple"
    } else if key.starts_with("e:") {
        "effect marginal"
    } else if key.starts_with("r:") {
        "read"
    } else if key.starts_with("t:") {
        "target"
    } else if key.starts_with("k:") {
        "cost"
    } else if key.starts_with("g:") {
        "trigger"
    } else {
        "other"
    }
}

/// Where FDN cards leave the Pauper vocabulary, by feature family, and how
/// much of each card's feature mass stays inside it.
fn oov_breakdown(table: &RulesTableV1, md: &mut String) {
    let vocab: BTreeSet<&String> = table.vocabulary.iter().collect();
    let mut cards_by_family: BTreeMap<&str, usize> = BTreeMap::new();
    let mut in_vocab_share = Vec::new();
    let mut examples: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for row in table.cards.iter().skip(PAUPER_REGISTRY_LEN_V1) {
        if CARD_DEFS[usize::from(row.id)].is_token {
            continue;
        }
        let mut families = BTreeSet::new();
        let (mut inside, mut all) = (0u32, 0u32);
        for (key, &n) in &row.features {
            all += n;
            if vocab.contains(key) {
                inside += n;
            } else {
                let f = family(key);
                families.insert(f);
                let list = examples.entry(f).or_default();
                if list.len() < 3 {
                    list.push(format!("{}: `{key}`", name(row.id)));
                }
            }
        }
        for f in families {
            *cards_by_family.entry(f).or_insert(0) += 1;
        }
        if all > 0 {
            in_vocab_share.push(f64::from(inside) / f64::from(all));
        }
    }
    in_vocab_share.sort_by(f64::total_cmp);
    let n = in_vocab_share.len();
    if n > 0 {
        writeln!(
            md,
            "- In-vocabulary share of each FDN card's feature counts: median {:.2}, 10th percentile {:.2}, minimum {:.2}",
            in_vocab_share[n / 2],
            in_vocab_share[n / 10],
            in_vocab_share[0]
        )
        .unwrap();
    }
    writeln!(
        md,
        "\nFDN cards with an out-of-vocabulary feature, by family (a card can count in several):\n"
    )
    .unwrap();
    writeln!(md, "| Family | Cards | Examples |").unwrap();
    writeln!(md, "| --- | --- | --- |").unwrap();
    for (f, count) in &cards_by_family {
        let ex = examples.get(f).map(|v| v.join("; ")).unwrap_or_default();
        writeln!(md, "| {f} | {} | {ex} |", pct(*count, n)).unwrap();
    }
}

/// Writes `rules_vector_v1.<build>.json`, `.manifest.json` and `.report.md`
/// into `dir`, refusing to overwrite existing files.
pub fn write_outputs(dir: &std::path::Path) -> std::io::Result<Vec<std::path::PathBuf>> {
    use std::io::Write;
    let table = build_table();
    let stem = format!("rules_vector_v1.{}", table.build.replace('+', "_"));
    let outputs = [
        (format!("{stem}.json"), table.canonical_bytes()),
        (
            format!("{stem}.manifest.json"),
            serde_json::to_vec_pretty(&manifest(&table)).expect("manifest serializes"),
        ),
        (
            format!("{stem}.report.md"),
            report_markdown(&table).into_bytes(),
        ),
    ];
    let mut written = Vec::new();
    for (file, bytes) in outputs {
        let path = dir.join(file);
        let mut out = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)?;
        out.write_all(&bytes)?;
        out.sync_all()?;
        written.push(path);
    }
    Ok(written)
}
