//! Private normalization of authenticated typed transport, never arbitrary user JSON.
use super::{numeric::NumericTree, roots, Error, Packet, Result, Source};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
struct References {
    objects: BTreeMap<u64, u64>,
    incarnations: BTreeMap<(u64, u64), u64>,
    stack: BTreeMap<u64, u64>,
    timestamps: BTreeMap<u64, u64>,
}
fn ordinal<K: Ord>(map: &mut BTreeMap<K, u64>, key: K) -> u64 {
    let next = map.len() as u64;
    *map.entry(key).or_insert(next)
}
fn omitted(key: &str) -> bool {
    matches!(
        key,
        "schema"
            | "producer_sha256"
            | "kernel_version"
            | "card_db_hash"
            | "kernel_carddb_hash"
            | "projection_sha256"
            | "root_sha256"
            | "model_admitted"
            | "public_human_model_sampler_admitted"
            | "card_name"
            | "name"
            | "object_name"
            | "label"
            | "description"
            | "text"
            | "rules_text"
            | "display_text"
            | "prompt"
            | "prompt_seq"
            | "episode_id"
            | "environment_revision"
            | "policy_step_count"
            | "physical_decision_count"
            | "step_index"
            | "physical_decision_id"
            | "surface_version"
            | "policy_surface_version"
    ) || key.ends_with("_name")
        || key.ends_with("_sha256")
        || key.ends_with("_digest")
        || key.ends_with("_hash")
        || key == "revision"
        || key.ends_with("_revision")
}

fn timestamps(value: &Value, key: &str, times: &mut BTreeSet<u64>) {
    match value {
        Value::Object(values) => {
            for (key, value) in values {
                timestamps(value, key, times);
            }
        }
        Value::Array(values) => {
            for value in values {
                timestamps(value, key, times);
            }
        }
        Value::Number(_) if key == "timestamp" || key == "layer_timestamp" => {
            if let Some(value) = value.as_u64() {
                times.insert(value);
            }
        }
        _ => {}
    }
}
fn uint(value: &Value) -> Result<u64> {
    value.as_u64().ok_or(Error::Range)
}
fn stable(value: &Map<String, Value>) -> bool {
    [
        "arena_id",
        "card_db_id",
        "owner",
        "controller",
        "zone",
        "zone_change_count",
    ]
    .iter()
    .all(|k| value.contains_key(*k))
}
fn expand(value: &mut Value, covered: &mut BTreeSet<u16>, source: Source) -> Result<()> {
    match value {
        Value::Array(values) => {
            for value in values {
                expand(value, covered, source)?;
            }
        }
        Value::Object(values) => {
            if source == Source::CallV62 && super::call::typed_meaning(values, covered)? {
                return Ok(());
            }
            if matches!(source, Source::GraveyardReturnV61 | Source::CallV62)
                && super::graveyard_return::typed_meaning(values, covered)?
            {
                return Ok(());
            }
            if matches!(
                source,
                Source::NextFiveV59 | Source::GraveyardReturnV61 | Source::CallV62
            ) && super::next_five::typed_meaning(values, covered)?
            {
                return Ok(());
            }
            if matches!(
                source,
                Source::PreparedV57
                    | Source::NextFiveV59
                    | Source::GraveyardReturnV61
                    | Source::CallV62
            ) && super::prepared::typed_meaning(values, covered)?
            {
                return Ok(());
            }
            if values.contains_key("root_sha256") && values.contains_key("card_id") {
                if values.len() <= 5 {
                    let full = roots::expand_thin(values)?;
                    *values = full.as_object().ok_or(Error::Range)?.clone();
                }
                covered.insert(u16::try_from(uint(&values["card_id"])?).map_err(|_| Error::Range)?);
            }
            for child in values.values_mut() {
                expand(child, covered, source)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn collect_cards(value: &Value, cards: &mut BTreeSet<u16>) -> Result<()> {
    match value {
        Value::Object(values) => {
            if stable(values) {
                cards
                    .insert(u16::try_from(uint(&values["card_db_id"])?).map_err(|_| Error::Range)?);
            }
            // Explicitly authorized face-down knowledge is a separate printing operand.
            if values.contains_key("card_name") && values.contains_key("card_db_id") {
                cards
                    .insert(u16::try_from(uint(&values["card_db_id"])?).map_err(|_| Error::Range)?);
            }
            for value in values.values() {
                collect_cards(value, cards)?;
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_cards(value, cards)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn rewrite(value: &Value, refs: &mut References, key: &str) -> Result<Value> {
    Ok(match value {
        Value::Object(values) if stable(values) => {
            if values.len() != 6 {
                return Err(Error::UnsupportedOperand("extended_card_reference".into()));
            }
            let object = uint(&values["arena_id"])?;
            let generation = uint(&values["zone_change_count"])?;
            json!({"object_ordinal":ordinal(&mut refs.objects,object),
                "incarnation_ordinal":ordinal(&mut refs.incarnations,(object,generation)),
                "printing":values["card_db_id"],"owner":values["owner"],
                "controller":values["controller"],"zone":values["zone"]})
        }
        Value::Object(values) if matches!(key, "selected_sacrifice" | "sacrificed_incarnation") => {
            // These exact Eaten V46 fields use a distinct typed wire spelling.
            // Preserve the same physical object/incarnation binding as stable
            // references; unknown fields and unpaired generations still refuse.
            let bound: crate::eaten_public_v46::IncarnationV46 =
                serde_json::from_value(Value::Object(values.clone()))
                    .map_err(|_| Error::UnsupportedOperand(key.into()))?;
            json!({"object_ordinal":ordinal(&mut refs.objects,u64::from(bound.object)),
                "incarnation_ordinal":ordinal(&mut refs.incarnations,(u64::from(bound.object),u64::from(bound.zone_change_count)))})
        }
        Value::Object(values)
            if values.contains_key("object")
                && values.contains_key("generation")
                && values["object"].is_number()
                && values["generation"].is_number() =>
        {
            let object = uint(&values["object"])?;
            let generation = uint(&values["generation"])?;
            let mut out = Map::new();
            out.insert(
                "object_ordinal".into(),
                ordinal(&mut refs.objects, object).into(),
            );
            out.insert(
                "incarnation_ordinal".into(),
                ordinal(&mut refs.incarnations, (object, generation)).into(),
            );
            for (key, value) in values {
                if key != "object" && key != "generation" && !omitted(key) {
                    out.insert(key.clone(), rewrite(value, refs, key)?);
                }
            }
            Value::Object(out)
        }
        Value::Object(values) => {
            let mut out = Map::new();
            let mut fields: Vec<_> = values.iter().collect();
            fields.sort_by(|a, b| a.0.cmp(b.0));
            for (key, value) in fields {
                if omitted(key) {
                    continue;
                }
                if key.contains("history_index")
                    || key == "handle"
                    || key.contains("debug")
                    || key.contains("generation")
                    || key.contains("zone_change_count")
                    || key == "id"
                    || (key.ends_with("_id")
                        && !key.ends_with("card_id")
                        && !key.ends_with("card_db_id")
                        && !key.ends_with("stack_item_id")
                        && !key.ends_with("_stack_id"))
                {
                    return Err(Error::UnsupportedOperand(key.clone()));
                }
                out.insert(key.clone(), rewrite(value, refs, key)?);
            }
            Value::Object(out)
        }
        Value::Array(values) => Value::Array(
            values
                .iter()
                .map(|v| rewrite(v, refs, key))
                .collect::<Result<_>>()?,
        ),
        Value::Number(_)
            if matches!(
                key,
                "arena_id"
                    | "object"
                    | "source_object"
                    | "target_object"
                    | "host_object"
                    | "recipient_object"
                    | "source"
                    | "target"
                    | "host"
                    | "recipient"
                    | "attacker"
                    | "blocker"
                    | "permanent"
                    | "attachments"
            ) =>
        {
            ordinal(&mut refs.objects, uint(value)?).into()
        }
        Value::Number(_) if key.ends_with("stack_item_id") || key.ends_with("_stack_id") => {
            ordinal(&mut refs.stack, uint(value)?).into()
        }
        Value::Number(_) if key == "timestamp" || key == "layer_timestamp" => {
            ordinal(&mut refs.timestamps, uint(value)?).into()
        }
        Value::Number(_) if key == "zone_change_count" || key == "generation" => {
            return Err(Error::UnsupportedOperand(key.into()))
        }
        value => value.clone(),
    })
}

pub(super) fn encode(source: Source, mut state: Value, mut actions: Vec<Value>) -> Result<Packet> {
    #[cfg(test)]
    let _timing = crate::rl_session::test_timing_v1::span(
        crate::rl_session::test_timing_v1::Phase::NumericNormalizeInclusive,
    );
    if matches!(source, Source::GraveyardReturnV61 | Source::CallV62) {
        let observation = state
            .get_mut("observation")
            .ok_or_else(|| Error::UnsupportedOperand("graveyard_return_observation".into()))?;
        super::public_projection_operands::prepare(observation)?;
    } else {
        super::public_projection_operands::prepare(&mut state)?;
    }
    super::kellan_public_operands::prepare(&mut state)?;
    super::fiery_public_operands::prepare(&mut state)?;
    super::fishing_public_operands::prepare(&mut state)?;
    if matches!(
        source,
        Source::PreparedV57 | Source::NextFiveV59 | Source::GraveyardReturnV61 | Source::CallV62
    ) {
        super::prepared::references(&mut state)?;
    }
    if matches!(source, Source::GraveyardReturnV61 | Source::CallV62) {
        // V61 actions carry the complete typed candidate annotations, including
        // their public references, in addition to the ordinary action semantic.
        for action in &mut actions {
            super::prepared::references(action)?;
        }
    }
    let mut covered = BTreeSet::new();
    expand(&mut state, &mut covered, source)?;
    let mut cards = BTreeSet::new();
    collect_cards(&state, &mut cards)?;
    for action in &actions {
        collect_cards(action, &mut cards)?;
    }
    let mut meanings = Vec::new();
    for id in cards.difference(&covered) {
        if source == Source::CallV62 && super::call::printing(*id)?.is_some() {
            continue;
        }
        if matches!(source, Source::GraveyardReturnV61 | Source::CallV62)
            && super::graveyard_return::printing(*id)?.is_some()
        {
            continue;
        }
        if matches!(
            source,
            Source::NextFiveV59 | Source::GraveyardReturnV61 | Source::CallV62
        ) && super::next_five::printing(*id)?.is_some()
        {
            continue;
        }
        if matches!(
            source,
            Source::PreparedV57
                | Source::NextFiveV59
                | Source::GraveyardReturnV61
                | Source::CallV62
        ) && super::prepared::printing(*id)?.is_some()
        {
            continue;
        }
        meanings.push(roots::baseline(*id)?);
    }
    // A family fact/root is not a complete ordinary printing descriptor.
    // These mandatory supplements are independent of `covered`, including
    // when the public catalog already supplied every trigger/replacement root.
    for id in &cards {
        if let Some(printing) = roots::registered_permanent_printing(*id)? {
            meanings.push(printing);
        }
        if let Some(printing) = roots::registered_spell_printing(*id)? {
            meanings.push(printing);
        }
    }
    // V52's existing typed meaning covers the static rules. Preserve its
    // complete ordered body and add the immutable ordinary spell route only
    // for an actually actor-authorized referenced printing.
    if matches!(
        source,
        Source::PreparedV57 | Source::NextFiveV59 | Source::GraveyardReturnV61 | Source::CallV62
    ) {
        for id in &cards {
            if let Some(printing) = super::prepared::printing(*id)? {
                meanings.push(printing);
            }
        }
    }
    if matches!(
        source,
        Source::NextFiveV59 | Source::GraveyardReturnV61 | Source::CallV62
    ) {
        for id in &cards {
            if let Some(printing) = super::next_five::printing(*id)? {
                meanings.push(printing);
            }
        }
    }
    if matches!(source, Source::GraveyardReturnV61 | Source::CallV62) {
        for id in &cards {
            if let Some(printing) = super::graveyard_return::printing(*id)? {
                meanings.push(printing);
            }
        }
    }
    if source == Source::CallV62 {
        for id in &cards {
            if let Some(printing) = super::call::printing(*id)? {
                meanings.push(printing);
            }
        }
    }
    if source == Source::ElendaV52 {
        if let Some(id) = crate::card_def::card_id_by_name("Elenda, Saint of Dusk") {
            if cards.contains(&id) {
                meanings.push(roots::elenda_printing(id)?);
            }
        }
    }
    let mut refs = References::default();
    let mut times = BTreeSet::new();
    timestamps(&state, "", &mut times);
    for action in &actions {
        timestamps(action, "", &mut times);
    }
    refs.timestamps = times
        .into_iter()
        .enumerate()
        .map(|(index, time)| (time, index as u64))
        .collect();
    let mut state = json!({"observation":state,"baseline_printed_roots":meanings});
    // Canonical meaning booleans are not runtime raw-identity operands. This
    // conversion validates each whole typed program before changing its exact
    // private key spelling. The generic identity refusal below is unchanged.
    super::canonical_semantic_booleans::prepare(&mut state)?;
    let state = rewrite(&state, &mut refs, "")?;
    let actions = actions
        .iter()
        .map(|action| NumericTree::encode(&rewrite(action, &mut refs, "")?))
        .collect::<Result<Vec<_>>>()?;
    if actions.is_empty() {
        return Err(Error::NoDecision);
    }
    Ok(Packet {
        source,
        state: NumericTree::encode(&state)?,
        actions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_integer_width_and_reference_renaming() {
        let make = |id, generation| json!({"arena_id":id,"card_db_id":0,"owner":"P0","controller":"P0","zone":"Battlefield","zone_change_count":generation});
        let mut a = References::default();
        let mut b = References::default();
        assert_eq!(
            rewrite(&make(4, 7), &mut a, "").unwrap(),
            rewrite(&make(99, 102), &mut b, "").unwrap()
        );
        assert_ne!(
            NumericTree::encode(&json!(16777216u32)).unwrap(),
            NumericTree::encode(&json!(16777217u32)).unwrap()
        );
        assert_ne!(
            NumericTree::encode(&json!(i32::MIN)).unwrap(),
            NumericTree::encode(&json!(i32::MAX)).unwrap()
        );
        let tree = NumericTree::encode(&json!(u64::MAX)).unwrap();
        assert!(tree.tensors().iter().flatten().all(|v| v.is_finite()));
    }
}
