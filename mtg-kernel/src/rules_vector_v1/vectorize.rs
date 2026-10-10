//! Turns extracted facets into sparse features with a frozen vocabulary.
//!
//! Each atom contributes its full tuple and a few marginals, so cards that
//! share part of a behaviour (both move library cards to a graveyard) share
//! features even when the rest differs. Feature strings are built only from
//! facet values; no card name or engine variant identifier reaches them.

use std::collections::{BTreeMap, BTreeSet};

use super::facets::*;
use super::sources::{card_rules, CardRulesV1, PrintedF, PAUPER_REGISTRY_LEN_V1};
use crate::card_def::CARD_DEFS;

/// Feature counts for one card, before scaling.
pub type FeatureCounts = BTreeMap<String, u32>;

fn zone(z: Option<ZoneF>) -> String {
    z.map_or_else(|| "-".to_string(), |z| format!("{z:?}"))
}

fn opt<T: std::fmt::Debug>(v: Option<T>) -> String {
    v.map_or_else(|| "-".to_string(), |v| format!("{v:?}"))
}

fn amount_kind(a: AmtF) -> String {
    format!("{a:?}")
}

fn effect_keys(e: &EffectAtom) -> Vec<String> {
    let ev = format!("{:?}", e.ev);
    let mut keys = vec![
        format!("e:{ev}"),
        format!("e:{ev}|p={}", opt(e.player)),
        format!("e:{ev}|o={}", opt(e.obj)),
        format!("e:{ev}|a={}", amount_kind(e.amount)),
        format!(
            "e:{ev}|{}>{}|p={}|o={}|a={}|d={:?}",
            zone(e.from),
            zone(e.to),
            opt(e.player),
            opt(e.obj),
            amount_kind(e.amount),
            e.duration
        ),
    ];
    if e.from.is_some() || e.to.is_some() {
        keys.push(format!("e:{ev}|{}>{}", zone(e.from), zone(e.to)));
        keys.push(format!(
            "e:{ev}|{}>{}|p={}",
            zone(e.from),
            zone(e.to),
            opt(e.player)
        ));
    }
    if e.duration != DurF::Instant {
        keys.push(format!("e:{ev}|d={:?}", e.duration));
    }
    if let Some(color) = e.color {
        keys.push(format!("e:{ev}|color={color:?}"));
    }
    if let Some(bit) = e.keyword {
        keys.push(format!("e:{ev}|kw={bit}"));
        keys.push(format!(
            "e:{ev}|kw={bit}|o={}|d={:?}",
            opt(e.obj),
            e.duration
        ));
    }
    keys
}

fn read_keys(r: &ReadAtom) -> Vec<String> {
    vec![
        format!(
            "r:{:?}|{}|{}|{:?}",
            r.player,
            zone(r.zone),
            opt(r.obj),
            r.agg
        ),
        format!("r:{}|{}", zone(r.zone), opt(r.obj)),
        format!("r:{:?}|{}", r.player, zone(r.zone)),
    ]
}

fn atom_keys(atom: &Atom) -> Vec<String> {
    match atom {
        Atom::Effect(e) => effect_keys(e),
        Atom::Read(r) => read_keys(r),
        Atom::Target(t) => {
            let mut keys = vec![format!("t:{t:?}")];
            if let TargetAtom::Object {
                obj,
                controller,
                zone: z,
                color: _,
                mana_value_at_most: _,
                excludes: _,
            } = t
            {
                keys.push(format!("t:obj={obj:?}|zone={z:?}"));
                keys.push(format!("t:ctl={}", opt(*controller)));
            }
            keys
        }
        Atom::Cost(c) => vec![format!("k:{c:?}")],
        Atom::Trigger(g) => vec![format!("g:{g:?}")],
        Atom::Control(x) => vec![format!("x:{x:?}")],
        Atom::PermanentResolution => vec!["ctx:permanent_spell".to_string()],
        Atom::CastFrom(z) => vec![format!("cast_from:{z:?}")],
        Atom::ActivatedFrom(z) => vec![format!("activated_from:{z:?}")],
        Atom::TriggersFrom(z) => vec![format!("triggers_from:{z:?}")],
        Atom::SorcerySpeed => vec!["sorcery_speed".to_string()],
        Atom::Opaque => vec!["opaque".to_string()],
    }
}

/// The coarse key used to pair parts inside one ability.
fn pair_key(atom: &Atom) -> Option<String> {
    match atom {
        Atom::Effect(e) => Some(format!("e:{:?}|{}>{}", e.ev, zone(e.from), zone(e.to))),
        Atom::Read(r) => Some(format!("r:{}|{}|{:?}", zone(r.zone), opt(r.obj), r.agg)),
        Atom::Trigger(g) => Some(format!("g:{g:?}")),
        Atom::Cost(c) => Some(format!("k:{c:?}")),
        _ => None,
    }
}

fn add(counts: &mut FeatureCounts, key: String) {
    *counts.entry(key).or_insert(0) += 1;
}

fn printed_counts(printed: &[PrintedF], prefix: &str, counts: &mut FeatureCounts) {
    for fact in printed {
        add(counts, format!("{prefix}p:{fact:?}"));
    }
}

fn rules_counts(rules: &CardRulesV1, prefix: &str, counts: &mut FeatureCounts) {
    printed_counts(&rules.printed, prefix, counts);
    for ability in &rules.abilities {
        add(counts, format!("{prefix}ab:{:?}", ability.ctx));
        let mut pairs = BTreeSet::new();
        for atom in &ability.atoms {
            for key in atom_keys(atom) {
                add(counts, format!("{prefix}{key}"));
            }
            if let Atom::Effect(e) = atom {
                add(
                    counts,
                    format!(
                        "{prefix}c:{:?}|e:{:?}|{}>{}",
                        ability.ctx,
                        e.ev,
                        zone(e.from),
                        zone(e.to)
                    ),
                );
            }
        }
        // Parent/child structure: which parts occur together in one ability
        // (an enters trigger that mills; damage scaled by a graveyard count).
        let keys: Vec<String> = ability.atoms.iter().filter_map(pair_key).collect();
        for (i, a) in keys.iter().enumerate() {
            for b in &keys[i + 1..] {
                if a != b {
                    let (lo, hi) = if a < b { (a, b) } else { (b, a) };
                    pairs.insert(format!("{prefix}pair:{lo}&{hi}"));
                }
            }
        }
        for pair in pairs {
            add(counts, pair);
        }
    }
}

/// All features of one card, including the cards it creates (one level).
pub fn card_feature_counts(card_id: u16) -> FeatureCounts {
    let rules = card_rules(card_id);
    let mut counts = FeatureCounts::new();
    rules_counts(&rules, "", &mut counts);
    for &token in &rules.created_tokens {
        if token == card_id {
            continue;
        }
        let token_rules = card_rules(token);
        rules_counts(&token_rules, "tok/", &mut counts);
    }
    counts
}

/// The frozen v1 vocabulary: every feature of a Pauper-registry card.
pub fn pauper_vocabulary() -> Vec<String> {
    let mut vocab = BTreeSet::new();
    for id in 0..PAUPER_REGISTRY_LEN_V1.min(CARD_DEFS.len()) {
        vocab.extend(card_feature_counts(id as u16).into_keys());
    }
    vocab.into_iter().collect()
}

/// A card's dense vector over `vocab`: log(1 + count), L2-normalised.
/// Features outside the vocabulary are returned separately (out of vocab).
pub fn dense_vector(counts: &FeatureCounts, vocab: &[String]) -> (Vec<f64>, Vec<String>) {
    let mut v = vec![0.0f64; vocab.len()];
    let mut oov = Vec::new();
    for (key, &n) in counts {
        match vocab.binary_search(key) {
            Ok(i) => v[i] = (1.0 + f64::from(n)).ln(),
            Err(_) => oov.push(key.clone()),
        }
    }
    let norm = v.iter().map(|x| x * x).sum::<f64>().sqrt();
    if norm > 0.0 {
        for x in &mut v {
            *x /= norm;
        }
    }
    (v, oov)
}

pub fn cosine(a: &[f64], b: &[f64]) -> f64 {
    let dot: f64 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let nb = b.iter().map(|x| x * x).sum::<f64>().sqrt();
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na * nb)
    }
}
