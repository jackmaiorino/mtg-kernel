//! Pure, deterministic, self-contained HTML renderer for D16 owner-view pages.
//!
//! Guarantees:
//! - Byte-identical output for identical [`OwnerViewPageData`].
//! - Zero external assets, scripts, timestamps, host paths or links to other
//!   pages or dose reads.
//! - LF line endings only.
//! - Plain English with no em-dash characters.

use crate::cube_draft_v1::BASIC_LAND_NAMES;
use crate::cube_owner_view_v1::sample_plan::{
    CardSummary, FirstPickRankRow, OwnerViewPageData, PickerMode, FIRST_PICK_MIN_PACKS,
};
use std::fmt::Write;

/// Renders one independent static HTML page from [`OwnerViewPageData`].
pub fn render_owner_view_html(data: &OwnerViewPageData) -> String {
    let mut out = String::with_capacity(256 * 1024);

    out.push_str("<!DOCTYPE html>\n");
    out.push_str("<html lang=\"en\">\n");
    out.push_str("<head>\n");
    out.push_str("<meta charset=\"utf-8\">\n");
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    let _ = writeln!(
        &mut out,
        "<title>Owner View: {} ({})</title>",
        escape_html(&data.run_id),
        escape_html(&data.namespace)
    );
    out.push_str("<style>\n");
    out.push_str(PAGE_CSS);
    out.push_str("</style>\n");
    out.push_str("</head>\n");
    out.push_str("<body>\n");
    out.push_str("<main class=\"container\">\n");

    // Header and shakedown banner.
    out.push_str("<header class=\"page-header\">\n");
    let _ = writeln!(
        &mut out,
        "<h1>Owner View: <code>{}</code></h1>",
        escape_html(&data.run_id)
    );
    let _ = writeln!(
        &mut out,
        "<p class=\"banner banner-shakedown\"><strong>Shakedown label:</strong> {}</p>",
        escape_html(&data.shakedown_label)
    );
    let _ = writeln!(
        &mut out,
        "<p class=\"banner banner-law\"><strong>Law boundary:</strong> {}</p>",
        escape_html(&data.law_boundary_notice)
    );
    if data.used_stand_in_seats {
        out.push_str("<p class=\"banner banner-standin\"><strong>Stand-in notice:</strong> Rendered with fixture stand-in seats through the pinned m4_sampler_v1 routine (D3 learned seats and D6 checkpoint weights not yet wired).</p>\n");
    }
    out.push_str("</header>\n\n");

    // Provenance metadata (zero games, no dose read or other page dependency).
    out.push_str("<section id=\"provenance\">\n");
    out.push_str("<h2>Provenance and Checkpoint</h2>\n");
    out.push_str("<table class=\"kv-table\">\n<tbody>\n");
    write_kv(&mut out, "Namespace", &data.namespace);
    write_kv(&mut out, "Run id", &data.run_id);
    write_kv(&mut out, "Games played", "0 (no owner-view game is played)");
    write_kv(&mut out, "Checkpoint schema", &data.checkpoint.schema);
    write_kv(
        &mut out,
        "Checkpoint segment / update",
        &format!(
            "{} / update {}",
            data.checkpoint.segment, data.checkpoint.update
        ),
    );
    write_kv(
        &mut out,
        "Checkpoint state sha256",
        &data.checkpoint.state_sha256,
    );
    write_kv(&mut out, "Model identity", &data.checkpoint.model_identity);
    write_kv(
        &mut out,
        "Parameter layout sha256",
        &data.checkpoint.parameter_layout_sha256,
    );
    write_kv(
        &mut out,
        "Card index sha256",
        &data.checkpoint.card_index_sha256,
    );
    write_kv(
        &mut out,
        "Cube table sha256",
        &data.checkpoint.cube_table_sha256,
    );
    write_kv(
        &mut out,
        "Card database hash",
        &data.checkpoint.card_database_hash,
    );
    write_kv(&mut out, "Training math", &data.checkpoint.training_math);
    write_kv(&mut out, "Interfaces", &data.interfaces_version);
    write_kv(&mut out, "Sampler", &data.sampler_id);
    write_kv(
        &mut out,
        "Ordered pool",
        &format!(
            "{} cards, sha256 {}",
            data.ordered_pool_len, data.ordered_pool_sha256
        ),
    );
    out.push_str("</tbody>\n</table>\n");
    out.push_str("</section>\n\n");

    // Head training states (from checkpoint head_updates).
    out.push_str("<section id=\"head-states\">\n");
    out.push_str("<h2>Head Training States</h2>\n");
    out.push_str("<p class=\"note\">Each head is labelled directly from the checkpoint header's <code>head_updates</code> count: zero is labelled <code>untrained</code>.</p>\n");
    if let Some(picks_label) = &data.picks_label {
        let _ = writeln!(
            &mut out,
            "<p class=\"callout callout-picks\"><strong>Picks:</strong> {}</p>",
            escape_html(picks_label)
        );
    } else {
        out.push_str("<p class=\"callout callout-picks\"><strong>Picks:</strong> rendered with the checkpoint's pick head (release decoder).</p>\n");
    }
    let _ = writeln!(
        &mut out,
        "<p class=\"callout callout-builds\"><strong>Builds:</strong> {}</p>",
        escape_html(&data.builds_label)
    );
    out.push_str("<table>\n<thead>\n<tr><th>Head</th><th>Key</th><th>head_updates</th><th>Training state</th></tr>\n</thead>\n<tbody>\n");
    for head in &data.head_labels {
        let badge_class = if head.updates == 0 {
            "badge badge-untrained"
        } else {
            "badge badge-trained"
        };
        let _ = writeln!(
            &mut out,
            "<tr><td>{}</td><td><code>{}</code></td><td class=\"num\">{}</td><td><span class=\"{}\">{}</span></td></tr>",
            escape_html(&head.display_name),
            escape_html(&head.head_key),
            head.updates,
            badge_class,
            escape_html(&head.label)
        );
    }
    out.push_str("</tbody>\n</table>\n");
    out.push_str("</section>\n\n");

    // Section 1: First-pick ranking.
    out.push_str("<section id=\"first-pick-ranking\">\n");
    let _ = writeln!(
        &mut out,
        "<h2>1. First-Pick Ranking (Drafts 0 to 99, {} fresh 15-card packs)</h2>",
        data.first_pick_pack_count
    );
    if let Some(picks_label) = &data.picks_label {
        let _ = writeln!(
            &mut out,
            "<p class=\"note\">Picks on this page are {}.</p>",
            escape_html(picks_label)
        );
    }
    let _ = writeln!(
        &mut out,
        "<p class=\"note\">Mean picker probability over packs holding each card, highest first, ties by registry id ascending. Cards in fewer than {} packs are listed apart below.</p>",
        FIRST_PICK_MIN_PACKS
    );
    write_first_pick_table(&mut out, &data.first_pick_ranked, false);

    let _ = writeln!(
        &mut out,
        "<h3>Cards in Fewer Than {} Packs ({} cards)</h3>",
        FIRST_PICK_MIN_PACKS,
        data.first_pick_low_count.len()
    );
    if data.first_pick_low_count.is_empty() {
        out.push_str("<p class=\"note\">None: every pool card appeared in at least 5 of the 800 packs.</p>\n");
    } else {
        write_first_pick_table(&mut out, &data.first_pick_low_count, true);
    }
    out.push_str("</section>\n\n");

    // Section 2: 20 Sample picks.
    out.push_str("<section id=\"sample-picks\">\n");
    out.push_str("<h2>2. Sample Picks (20 Picks, Drafts 100 to 119)</h2>\n");
    if let Some(picks_label) = &data.picks_label {
        let _ = writeln!(
            &mut out,
            "<p class=\"note\">Picks on this page are {}.</p>",
            escape_html(picks_label)
        );
    }
    for entry in &data.sample_picks {
        out.push_str("<article class=\"card-block\">\n");
        let adjust_note = if entry.spec.adjusted_from_forced {
            format!(
                " (adjusted from forced step {} to step {} per D0.21 L4)",
                entry.spec.raw_step, entry.spec.step
            )
        } else {
            String::new()
        };
        let _ = writeln!(
            &mut out,
            "<h3>Sample Pick {} : Draft {}, Chair {}, Step {} (Round {}, Pick {}){}</h3>",
            entry.spec.index,
            entry.spec.draft_index,
            entry.spec.chair,
            entry.spec.step,
            entry.spec.round,
            entry.spec.pick,
            escape_html(&adjust_note)
        );
        let value_text = match entry.value_before_pick {
            Some(v) => format!(" | V(x) = {v:.6}"),
            None => String::new(),
        };
        let _ = writeln!(
            &mut out,
            "<p class=\"meta\">draft_seed = <code>{}</code> | Chosen: <strong>{}</strong> (#{}){}</p>",
            entry.draft_seed,
            escape_html(&entry.chosen_card.name),
            entry.chosen_card.id,
            escape_html(&value_text)
        );

        out.push_str("<div class=\"subhead\">Pack options</div>\n");
        out.push_str("<table>\n<thead>\n<tr><th>Card</th><th>ID</th><th>Colors</th><th>Type</th><th>Probability</th><th>Chosen</th></tr>\n</thead>\n<tbody>\n");
        for opt in &entry.options {
            let row_class = if opt.chosen {
                " class=\"chosen-row\""
            } else {
                ""
            };
            let chosen_mark = if opt.chosen { "CHOSEN" } else { "" };
            let _ = writeln!(
                &mut out,
                "<tr{}><td>{}</td><td class=\"num\">{}</td><td>{}</td><td>{}</td><td class=\"num\">{}</td><td>{}</td></tr>",
                row_class,
                escape_html(&opt.card.name),
                opt.card.id,
                escape_html(&opt.card.colors),
                escape_html(&opt.card.types),
                format_probability(opt.probability),
                chosen_mark
            );
        }
        out.push_str("</tbody>\n</table>\n");

        let _ = writeln!(
            &mut out,
            "<div class=\"subhead\">Pool so far ({} cards, pick order)</div>",
            entry.pool_so_far.len()
        );
        if entry.pool_so_far.is_empty() {
            out.push_str("<p class=\"note\">Empty (first pick of the draft).</p>\n");
        } else {
            write_card_list(&mut out, &entry.pool_so_far);
        }
        out.push_str("</article>\n");
    }
    out.push_str("</section>\n\n");

    // Section 3: 20 Sample pools and builds.
    out.push_str("<section id=\"sample-pools\">\n");
    let _ = writeln!(
        &mut out,
        "<h2>3. Sample Pools and {} (20 Pools, Drafts 120 to 139, Stream k = 0)</h2>",
        escape_html(&data.builds_label)
    );
    let _ = writeln!(
        &mut out,
        "<p class=\"note\">Every build shown below is from the checkpoint's build head ({}).</p>",
        escape_html(&data.builds_label)
    );
    for entry in &data.sample_pools {
        out.push_str("<article class=\"card-block\">\n");
        let _ = writeln!(
            &mut out,
            "<h3>Sample Pool {} : Draft {}, Chair {}, Stream k = {}</h3>",
            entry.spec.index, entry.spec.draft_index, entry.spec.chair, entry.spec.build_k
        );
        let basics_breakdown = format_basics_breakdown(&entry.basic_counts);
        let _ = writeln!(
            &mut out,
            "<p class=\"meta\">draft_seed = <code>{}</code> | build_seed = <code>{}</code> | B = <strong>{}</strong> | |T| = <strong>{}</strong> ({}) | {}</p>",
            entry.draft_seed,
            entry.build_seed,
            entry.basics_count,
            entry.type_set_size,
            escape_html(&format_mask(entry.type_set_mask)),
            escape_html(&basics_breakdown)
        );

        let _ = writeln!(
            &mut out,
            "<div class=\"subhead\">Sampled build: included pool cards ({} cards)</div>",
            entry.included_pool_cards.len()
        );
        if entry.included_pool_cards.is_empty() {
            out.push_str("<p class=\"note\">None (all 40 cards are basic lands).</p>\n");
        } else {
            write_card_list(&mut out, &entry.included_pool_cards);
        }

        let _ = writeln!(
            &mut out,
            "<div class=\"subhead\">Left out of build ({} pool cards)</div>",
            entry.excluded_pool_cards.len()
        );
        if entry.excluded_pool_cards.is_empty() {
            out.push_str("<p class=\"note\">None.</p>\n");
        } else {
            write_card_list(&mut out, &entry.excluded_pool_cards);
        }

        out.push_str("</article>\n");
    }
    out.push_str("</section>\n\n");

    // Section 4: Build distributions.
    out.push_str("<section id=\"build-distributions\">\n");
    let _ = writeln!(
        &mut out,
        "<h2>4. Build Distributions (Drafts 200 to 299, {} {})</h2>",
        data.distributions.total_builds,
        escape_html(&data.builds_label)
    );
    let _ = writeln!(
        &mut out,
        "<p class=\"meta\">Total sampled builds: <strong>{}</strong> (100 drafts x 8 chairs x streams k = 0 to 3) | Mean B = <strong>{:.3}</strong> | Mean |T| = <strong>{:.3}</strong></p>",
        data.distributions.total_builds,
        data.distributions.mean_basics,
        data.distributions.mean_type_set_size
    );

    out.push_str("<h3>Basics Count B (0 to 40)</h3>\n");
    let max_b = data
        .distributions
        .basics_count_hist
        .iter()
        .copied()
        .max()
        .unwrap_or(1)
        .max(1);
    out.push_str("<table>\n<thead>\n<tr><th>B</th><th>Count</th><th>Share</th><th>Bar</th></tr>\n</thead>\n<tbody>\n");
    for (b, &count) in data.distributions.basics_count_hist.iter().enumerate() {
        let share = (count as f64) / (data.distributions.total_builds as f64);
        let width_pct = (count as f64) * 100.0 / (max_b as f64);
        let _ = writeln!(
            &mut out,
            "<tr><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{:.2}%</td><td class=\"bar-cell\"><div class=\"bar\" style=\"width:{:.2}%\"></div></td></tr>",
            b,
            count,
            share * 100.0,
            width_pct
        );
    }
    out.push_str("</tbody>\n</table>\n");

    out.push_str("<h3>Number of Basic Types |T| (0 to 5)</h3>\n");
    let max_t = data
        .distributions
        .type_set_size_hist
        .iter()
        .copied()
        .max()
        .unwrap_or(1)
        .max(1);
    out.push_str("<table>\n<thead>\n<tr><th>|T|</th><th>Count</th><th>Share</th><th>Bar</th></tr>\n</thead>\n<tbody>\n");
    for (t, &count) in data.distributions.type_set_size_hist.iter().enumerate() {
        let share = (count as f64) / (data.distributions.total_builds as f64);
        let width_pct = (count as f64) * 100.0 / (max_t as f64);
        let _ = writeln!(
            &mut out,
            "<tr><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{:.2}%</td><td class=\"bar-cell\"><div class=\"bar\" style=\"width:{:.2}%\"></div></td></tr>",
            t,
            count,
            share * 100.0,
            width_pct
        );
    }
    out.push_str("</tbody>\n</table>\n");

    let picker_summary = match data.picker_mode {
        PickerMode::UniformFixture => "uniform fixture",
        PickerMode::LearnedPickHead => "checkpoint pick head",
    };
    let _ = writeln!(
        &mut out,
        "<footer class=\"page-footer\">Owner view <code>{}</code> in namespace <code>{}</code> | picker: {} | builds: {} | 0 games played.</footer>",
        escape_html(&data.run_id),
        escape_html(&data.namespace),
        picker_summary,
        escape_html(&data.builds_label)
    );
    out.push_str("</section>\n");
    out.push_str("</main>\n");
    out.push_str("</body>\n");
    out.push_str("</html>\n");
    out
}

fn write_kv(out: &mut String, key: &str, value: &str) {
    let _ = writeln!(
        out,
        "<tr><th>{}</th><td><code>{}</code></td></tr>",
        escape_html(key),
        escape_html(value)
    );
}

/// Formats a probability for HTML display so that any positive probability
/// smaller than `1e-4` (including `2^-53`) is printed in scientific notation
/// and never rounds down to zero.
pub fn format_probability(p: f64) -> String {
    if p > 0.0 && p < 1e-4 {
        format!("{p:.3e} ({:.3e}%)", p * 100.0)
    } else {
        format!("{p:.6} ({:.2}%)", p * 100.0)
    }
}

fn write_first_pick_table(out: &mut String, rows: &[FirstPickRankRow], low_count: bool) {
    let table_class = if low_count {
        "low-count-table"
    } else {
        "rank-table"
    };
    let _ = writeln!(out, "<table class=\"{table_class}\">");
    out.push_str("<thead>\n<tr><th>Rank</th><th>Card</th><th>ID</th><th>Colors</th><th>Type</th><th>Mean Prob</th><th>Packs</th></tr>\n</thead>\n<tbody>\n");
    for row in rows {
        let mean_str = match row.mean_probability {
            Some(p) => format_probability(p),
            None => "N/A".to_owned(),
        };
        let _ = writeln!(
            out,
            "<tr><td class=\"num\">{}</td><td>{}</td><td class=\"num\">{}</td><td>{}</td><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td></tr>",
            row.rank,
            escape_html(&row.card.name),
            row.card.id,
            escape_html(&row.card.colors),
            escape_html(&row.card.types),
            mean_str,
            row.pack_count
        );
    }
    out.push_str("</tbody>\n</table>\n");
}

fn write_card_list(out: &mut String, cards: &[CardSummary]) {
    out.push_str("<ul class=\"card-list\">\n");
    for card in cards {
        let _ = writeln!(
            out,
            "<li><span class=\"card-name\">{}</span> <span class=\"card-meta\">(#{}, {}, {})</span></li>",
            escape_html(&card.name),
            card.id,
            escape_html(&card.colors),
            escape_html(&card.types)
        );
    }
    out.push_str("</ul>\n");
}

fn format_mask(mask: u16) -> String {
    if mask == 0 {
        return "none".to_owned();
    }
    let mut s = String::new();
    for (idx, letter) in ['W', 'U', 'B', 'R', 'G'].into_iter().enumerate() {
        if mask & (1u16 << idx) != 0 {
            s.push(letter);
        }
    }
    s
}

fn format_basics_breakdown(counts: &[u16; 5]) -> String {
    let parts: Vec<String> = BASIC_LAND_NAMES
        .iter()
        .zip(counts)
        .filter(|&(_, &c)| c > 0)
        .map(|(name, &c)| format!("{c} {name}"))
        .collect();
    if parts.is_empty() {
        "0 basics".to_owned()
    } else {
        parts.join(", ")
    }
}

fn escape_html(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

const PAGE_CSS: &str = r#"
:root { color-scheme: light dark; }
body { font-family: system-ui, -apple-system, Segoe UI, Roboto, sans-serif; margin: 0; padding: 1.5rem; line-height: 1.45; background: #fafafa; color: #1a1a1a; }
.container { max-width: 1120px; margin: 0 auto; }
h1, h2, h3 { margin-top: 1.6rem; margin-bottom: 0.6rem; }
code { font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; font-size: 0.9em; }
.banner { padding: 0.75rem 1rem; border-radius: 6px; margin: 0.5rem 0; border-left: 4px solid #444; background: #f0f0f0; }
.banner-shakedown { border-left-color: #b45309; background: #fef3c7; color: #78350f; }
.banner-law { border-left-color: #1d4ed8; background: #dbeafe; color: #1e3a8a; }
.banner-standin { border-left-color: #6b21a8; background: #f3e8ff; color: #581c87; }
.callout { padding: 0.5rem 0.75rem; border-radius: 4px; background: #eef2ff; margin: 0.4rem 0; }
.note, .meta { color: #4b5563; font-size: 0.92rem; }
table { border-collapse: collapse; width: 100%; margin: 0.75rem 0 1.25rem; font-size: 0.9rem; background: #ffffff; }
th, td { border: 1px solid #d1d5db; padding: 0.35rem 0.55rem; text-align: left; }
th { background: #f3f4f6; }
td.num { text-align: right; font-variant-numeric: tabular-nums; }
.kv-table th { width: 240px; }
.chosen-row { background: #dcfce7; font-weight: 600; }
.badge { display: inline-block; padding: 0.15rem 0.5rem; border-radius: 4px; font-weight: 600; font-size: 0.85rem; }
.badge-untrained { background: #fee2e2; color: #991b1b; }
.badge-trained { background: #dcfce7; color: #166534; }
.card-block { border: 1px solid #d1d5db; border-radius: 6px; padding: 0.85rem 1rem; margin: 1rem 0; background: #ffffff; }
.subhead { font-weight: 600; margin-top: 0.65rem; margin-bottom: 0.3rem; }
.card-list { columns: 3 240px; column-gap: 1.25rem; padding-left: 1.2rem; margin: 0.3rem 0; font-size: 0.88rem; }
.card-meta { color: #6b7280; font-size: 0.82rem; }
.bar-cell { width: 45%; }
.bar { height: 0.85rem; background: #2563eb; border-radius: 2px; min-width: 1px; }
.page-footer { margin-top: 2rem; padding-top: 0.75rem; border-top: 1px solid #d1d5db; color: #6b7280; font-size: 0.85rem; }
@media (prefers-color-scheme: dark) {
  body { background: #111827; color: #f3f4f6; }
  table, .card-block { background: #1f2937; border-color: #374151; }
  th, td { border-color: #374151; }
  th { background: #111827; }
  .banner-shakedown { background: #451a03; color: #fde68a; }
  .banner-law { background: #172554; color: #bfdbfe; }
  .banner-standin { background: #3b0764; color: #e9d5ff; }
  .callout { background: #1e293b; }
  .chosen-row { background: #14532d; }
  .badge-untrained { background: #7f1d1d; color: #fecaca; }
  .badge-trained { background: #14532d; color: #bbf7d0; }
  .note, .meta, .card-meta, .page-footer { color: #9ca3af; }
}
"#;
