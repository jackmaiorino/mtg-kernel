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
            if source == Source::KaitoV75 && super::kaito::typed_meaning(values, covered)? {
                return Ok(());
            }
            if matches!(source, Source::ChandraV74 | Source::KaitoV75)
                && super::chandra::typed_meaning(values, covered)?
            {
                return Ok(());
            }
            if matches!(
                source,
                Source::LilianaV73 | Source::ChandraV74 | Source::KaitoV75
            ) && super::liliana::typed_meaning(values, covered)?
            {
                return Ok(());
            }
            if matches!(
                source,
                Source::TinybonesV72 | Source::LilianaV73 | Source::ChandraV74 | Source::KaitoV75
            ) && super::tinybones::typed_meaning(values, covered)?
            {
                return Ok(());
            }
            if matches!(
                source,
                Source::MuldrothaV71
                    | Source::TinybonesV72
                    | Source::LilianaV73
                    | Source::ChandraV74
                    | Source::KaitoV75
            ) && super::muldrotha::typed_meaning(values, covered)?
            {
                return Ok(());
            }
            if matches!(
                source,
                Source::VivienV70
                    | Source::MuldrothaV71
                    | Source::TinybonesV72
                    | Source::LilianaV73
                    | Source::ChandraV74
                    | Source::KaitoV75
            ) && super::vivien::typed_meaning(values, covered)?
            {
                return Ok(());
            }
            if matches!(
                source,
                Source::GenesisWaveV69
                    | Source::VivienV70
                    | Source::MuldrothaV71
                    | Source::TinybonesV72
                    | Source::LilianaV73
                    | Source::ChandraV74
                    | Source::KaitoV75
            ) && super::genesis_wave::typed_meaning(values, covered)?
            {
                return Ok(());
            }
            if matches!(
                source,
                Source::EndTurnV68
                    | Source::GenesisWaveV69
                    | Source::VivienV70
                    | Source::MuldrothaV71
                    | Source::TinybonesV72
                    | Source::LilianaV73
                    | Source::ChandraV74
                    | Source::KaitoV75
            ) && super::end_turn::typed_meaning(values, covered)?
            {
                return Ok(());
            }
            if matches!(
                source,
                Source::CallV62
                    | Source::EndTurnV68
                    | Source::GenesisWaveV69
                    | Source::VivienV70
                    | Source::MuldrothaV71
                    | Source::TinybonesV72
                    | Source::LilianaV73
                    | Source::ChandraV74
                    | Source::KaitoV75
            ) && super::call::typed_meaning(values, covered)?
            {
                return Ok(());
            }
            if matches!(
                source,
                Source::GraveyardReturnV61
                    | Source::CallV62
                    | Source::EndTurnV68
                    | Source::GenesisWaveV69
                    | Source::VivienV70
                    | Source::MuldrothaV71
                    | Source::TinybonesV72
                    | Source::LilianaV73
                    | Source::ChandraV74
                    | Source::KaitoV75
            ) && super::graveyard_return::typed_meaning(values, covered)?
            {
                return Ok(());
            }
            if matches!(
                source,
                Source::NextFiveV59
                    | Source::GraveyardReturnV61
                    | Source::CallV62
                    | Source::EndTurnV68
                    | Source::GenesisWaveV69
                    | Source::VivienV70
                    | Source::MuldrothaV71
                    | Source::TinybonesV72
                    | Source::LilianaV73
                    | Source::ChandraV74
                    | Source::KaitoV75
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
                    | Source::EndTurnV68
                    | Source::GenesisWaveV69
                    | Source::VivienV70
                    | Source::MuldrothaV71
                    | Source::TinybonesV72
                    | Source::LilianaV73
                    | Source::ChandraV74
                    | Source::KaitoV75
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
    if matches!(
        source,
        Source::GraveyardReturnV61
            | Source::CallV62
            | Source::EndTurnV68
            | Source::GenesisWaveV69
            | Source::VivienV70
            | Source::MuldrothaV71
            | Source::TinybonesV72
            | Source::LilianaV73
            | Source::ChandraV74
            | Source::KaitoV75
    ) {
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
        Source::PreparedV57
            | Source::NextFiveV59
            | Source::GraveyardReturnV61
            | Source::CallV62
            | Source::EndTurnV68
            | Source::GenesisWaveV69
            | Source::VivienV70
            | Source::MuldrothaV71
            | Source::TinybonesV72
            | Source::LilianaV73
            | Source::ChandraV74
            | Source::KaitoV75
    ) {
        super::prepared::references(&mut state)?;
    }
    if matches!(
        source,
        Source::GraveyardReturnV61
            | Source::CallV62
            | Source::EndTurnV68
            | Source::GenesisWaveV69
            | Source::VivienV70
            | Source::MuldrothaV71
            | Source::TinybonesV72
            | Source::LilianaV73
            | Source::ChandraV74
            | Source::KaitoV75
    ) {
        // V61 actions carry the complete typed candidate annotations, including
        // their public references, in addition to the ordinary action semantic.
        for action in &mut actions {
            super::prepared::references(action)?;
        }
    }
    let full_pool = source == Source::KaitoV75
        && state["schema"] == crate::limited_full_pool_consumer_v1::CANDIDATES_SCHEMA;
    let mut covered = BTreeSet::new();
    if source == Source::KaitoV75
        && (full_pool
            || state["schema"] == crate::thousand_year_storm_consumer_v1::CANDIDATES_SCHEMA)
    {
        let meaning: crate::thousand_year_storm_public_v1::MeaningV1 =
            serde_json::from_value(state["observation"]["thousand_year_storm"]["meaning"].clone())
                .map_err(|e| Error::Projection(e.to_string()))?;
        if meaning
            != crate::thousand_year_storm_public_v1::printed_meaning().map_err(Error::Projection)?
        {
            return Err(Error::Projection("Storm exact meaning changed".into()));
        }
        covered.insert(meaning.card_id);
    }
    if source == Source::KaitoV75
        && (full_pool
            || state["schema"] == crate::etali_primal_storm_consumer_v1::CANDIDATES_SCHEMA)
    {
        let meaning: crate::etali_primal_storm_public_v1::MeaningV1 =
            serde_json::from_value(state["observation"]["etali"]["meaning"].clone())
                .map_err(|error| Error::Projection(error.to_string()))?;
        if meaning
            != crate::etali_primal_storm_public_v1::printed_meaning().map_err(Error::Projection)?
        {
            return Err(Error::Projection(
                "Etali meaning differs from exact printing".into(),
            ));
        }
        covered.insert(meaning.card_id);
    }
    if source == Source::KaitoV75
        && (full_pool || state["schema"] == crate::harvester_consumer_v1::CANDIDATES_SCHEMA)
    {
        let meaning: crate::harvester_public_v1::MeaningV1 =
            serde_json::from_value(state["observation"]["harvester"]["meaning"].clone())
                .map_err(|error| Error::Projection(error.to_string()))?;
        if meaning != crate::harvester_public_v1::printed_meaning().map_err(Error::Projection)? {
            return Err(Error::Projection(
                "Harvester meaning differs from exact printing".into(),
            ));
        }
        covered.insert(meaning.card_id);
    }
    if source == Source::KaitoV75
        && (full_pool || state["schema"] == crate::greatwurm_consumer_v1::CANDIDATES_SCHEMA)
    {
        let meaning: crate::greatwurm_public_v1::MeaningV1 =
            serde_json::from_value(state["observation"]["greatwurm"]["meaning"].clone())
                .map_err(|error| Error::Projection(error.to_string()))?;
        if meaning != crate::greatwurm_public_v1::printed_meaning().map_err(Error::Projection)? {
            return Err(Error::Projection(
                "Greatwurm meaning differs from exact printing".into(),
            ));
        }
        covered.insert(meaning.card_id);
    }
    if source == Source::KaitoV75
        && (full_pool || state["schema"] == crate::fiend_artisan_consumer_v1::CANDIDATES_SCHEMA)
    {
        let meaning: crate::fiend_artisan_public_v1::MeaningV1 =
            serde_json::from_value(state["observation"]["fiend_artisan"]["meaning"].clone())
                .map_err(|error| Error::Projection(error.to_string()))?;
        if meaning
            != crate::fiend_artisan_public_v1::printed_meaning().map_err(Error::Projection)?
        {
            return Err(Error::Projection(
                "Fiend Artisan meaning differs from exact printing".into(),
            ));
        }
        covered.insert(meaning.card_id);
    }
    if source == Source::KaitoV75
        && (full_pool || state["schema"] == crate::curator_consumer_v1::CANDIDATES_SCHEMA)
    {
        let meaning: crate::curator_public_v1::MeaningV1 =
            serde_json::from_value(state["observation"]["curator"]["meaning"].clone())
                .map_err(|error| Error::Projection(error.to_string()))?;
        if meaning != crate::curator_public_v1::printed_meaning().map_err(Error::Projection)? {
            return Err(Error::Projection(
                "Curator meaning differs from exact printing".into(),
            ));
        }
        covered.insert(meaning.card_id);
    }
    if source == Source::KaitoV75
        && (full_pool || state["schema"] == crate::edict_consumer_v1::CANDIDATES_SCHEMA)
    {
        let meaning: crate::edict_public_v1::MeaningV1 =
            serde_json::from_value(state["observation"]["edict"]["meaning"].clone())
                .map_err(|error| Error::Projection(error.to_string()))?;
        if meaning != crate::edict_public_v1::printed_meaning().map_err(Error::Projection)? {
            return Err(Error::Projection(
                "Edict meaning differs from exact printing".into(),
            ));
        }
        covered.insert(meaning.card_id);
    }
    if source == Source::KaitoV75
        && (full_pool || state["schema"] == crate::lathril_consumer_v1::CANDIDATES_SCHEMA)
    {
        let meaning: crate::lathril_public_v1::MeaningV1 =
            serde_json::from_value(state["observation"]["lathril"]["meaning"].clone())
                .map_err(|error| Error::Projection(error.to_string()))?;
        if meaning != crate::lathril_public_v1::printed_meaning().map_err(Error::Projection)? {
            return Err(Error::Projection(
                "Lathril meaning differs from exact printing".into(),
            ));
        }
        covered.insert(meaning.card_id);
        covered.insert(meaning.token_id);
    }
    if source == Source::KaitoV75
        && (full_pool || state["schema"] == crate::leyline_axe_consumer_v1::CANDIDATES_SCHEMA)
    {
        let meaning: crate::leyline_axe_public_v1::MeaningV1 =
            serde_json::from_value(state["observation"]["leyline_axe"]["meaning"].clone())
                .map_err(|error| Error::Projection(error.to_string()))?;
        if meaning != crate::leyline_axe_public_v1::printed_meaning().map_err(Error::Projection)? {
            return Err(Error::Projection(
                "Leyline Axe meaning differs from exact printing".into(),
            ));
        }
        covered.insert(meaning.card_id);
    }
    if source == Source::KaitoV75
        && (full_pool || state["schema"] == crate::moon_consumer_v1::CANDIDATES_SCHEMA)
    {
        let meaning: crate::moon_public_v1::MeaningV1 =
            serde_json::from_value(state["observation"]["moon"]["meaning"].clone())
                .map_err(|e| Error::Projection(e.to_string()))?;
        if meaning != crate::moon_public_v1::printed_meaning().map_err(Error::Projection)? {
            return Err(Error::Projection(
                "Moon meaning differs from exact printing".into(),
            ));
        }
        covered.insert(meaning.card_id);
    }
    if source == Source::KaitoV75
        && (state["schema"] == crate::tutelage_consumer_v1::CANDIDATES_SCHEMA
            || state["schema"] == crate::limited_mixed_consumer_v1::CANDIDATES_SCHEMA
            || full_pool)
    {
        let meaning: crate::tutelage_public_v1::MeaningV1 =
            serde_json::from_value(state["observation"]["tutelage"]["meaning"].clone())
                .map_err(|e| Error::Projection(e.to_string()))?;
        if meaning != crate::tutelage_public_v1::printed_meaning().map_err(Error::Projection)? {
            return Err(Error::Projection(
                "Tutelage meaning differs from exact printing".into(),
            ));
        }
        covered.insert(meaning.card_id);
    }
    if source == Source::KaitoV75
        && (state["schema"] == crate::omniscience_consumer_v1::CANDIDATES_SCHEMA
            || state["schema"] == crate::limited_mixed_consumer_v1::CANDIDATES_SCHEMA
            || full_pool)
    {
        let meaning: crate::omniscience_public_v1::MeaningV1 =
            serde_json::from_value(state["observation"]["omniscience"]["meaning"].clone())
                .map_err(|e| Error::Projection(e.to_string()))?;
        if meaning != crate::omniscience_public_v1::printed_meaning().map_err(Error::Projection)? {
            return Err(Error::Projection(
                "Omniscience meaning differs from exact printing".into(),
            ));
        }
        covered.insert(meaning.card_id);
    }
    if source == Source::KaitoV75
        && (full_pool || state["schema"] == crate::kykar_consumer_v1::CANDIDATES_SCHEMA)
    {
        let meaning: crate::kykar_public_v1::MeaningV1 =
            serde_json::from_value(state["observation"]["kykar"]["meaning"].clone())
                .map_err(|e| Error::Projection(e.to_string()))?;
        if meaning != crate::kykar_public_v1::printed_meaning().map_err(Error::Projection)? {
            return Err(Error::Projection(
                "Kykar meaning differs from exact printing".into(),
            ));
        }
        covered.insert(meaning.card_id);
        covered.insert(meaning.spirit_card_id);
    }
    if source == Source::KaitoV75
        && (full_pool || state["schema"] == crate::sphinx_lore_consumer_v1::CANDIDATES_SCHEMA)
    {
        let meaning: crate::sphinx_lore_public_v1::MeaningV1 =
            serde_json::from_value(state["observation"]["sphinx_lore"]["meaning"].clone())
                .map_err(|e| Error::Projection(e.to_string()))?;
        if meaning != crate::sphinx_lore_public_v1::printed_meaning().map_err(Error::Projection)? {
            return Err(Error::Projection(
                "SphinxLore meaning differs from exact printing".into(),
            ));
        }
        covered.insert(meaning.card_id);
    }
    if source == Source::KaitoV75
        && (full_pool || state["schema"] == crate::loot_consumer_v1::CANDIDATES_SCHEMA)
    {
        let meaning: crate::loot_public_v1::MeaningV1 =
            serde_json::from_value(state["observation"]["loot"]["meaning"].clone())
                .map_err(|e| Error::Projection(e.to_string()))?;
        if meaning != crate::loot_public_v1::printed_meaning().map_err(Error::Projection)? {
            return Err(Error::Projection(
                "Loot meaning differs from exact printing".into(),
            ));
        }
        covered.insert(meaning.card_id);
    }
    if source == Source::KaitoV75
        && (full_pool || state["schema"] == crate::progenitus_consumer_v1::CANDIDATES_SCHEMA)
    {
        let meaning: crate::progenitus_public_v1::MeaningV1 =
            serde_json::from_value(state["observation"]["progenitus"]["meaning"].clone())
                .map_err(|e| Error::Projection(e.to_string()))?;
        if meaning != crate::progenitus_public_v1::printed_meaning().map_err(Error::Projection)? {
            return Err(Error::Projection(
                "Progenitus meaning differs from exact printing".into(),
            ));
        }
        covered.insert(meaning.card_id);
    }
    if source == Source::KaitoV75
        && (full_pool || state["schema"] == crate::aberration_consumer_v1::CANDIDATES_SCHEMA)
    {
        let meaning: crate::aberration_public_v1::MeaningV1 =
            serde_json::from_value(state["observation"]["aberration"]["meaning"].clone())
                .map_err(|e| Error::Projection(e.to_string()))?;
        if meaning != crate::aberration_public_v1::printed_meaning().map_err(Error::Projection)? {
            return Err(Error::Projection(
                "Aberration meaning differs from exact printing".into(),
            ));
        }
        covered.insert(meaning.card_id);
    }
    if source == Source::KaitoV75
        && (full_pool || state["schema"] == crate::electroduplicate_consumer_v1::CANDIDATES_SCHEMA)
    {
        let meaning: crate::electroduplicate_public_v1::MeaningV1 =
            serde_json::from_value(state["observation"]["electroduplicate"]["meaning"].clone())
                .map_err(|e| Error::Projection(e.to_string()))?;
        if meaning
            != crate::electroduplicate_public_v1::printed_meaning().map_err(Error::Projection)?
        {
            return Err(Error::Projection(
                "Electroduplicate meaning differs from exact printing".into(),
            ));
        }
        covered.insert(meaning.card_id);
    }
    if source == Source::KaitoV75
        && (state["schema"] == crate::zimone_consumer_v1::CANDIDATES_SCHEMA
            || state["schema"] == crate::limited_mixed_consumer_v1::CANDIDATES_SCHEMA
            || full_pool)
    {
        let meaning: crate::zimone_public_v1::MeaningV1 =
            serde_json::from_value(state["observation"]["zimone"]["meaning"].clone())
                .map_err(|e| Error::Projection(e.to_string()))?;
        if meaning != crate::zimone_public_v1::printed_meaning().map_err(Error::Projection)? {
            return Err(Error::Projection(
                "Zimone meaning differs from exact printing".into(),
            ));
        }
        covered.insert(meaning.card_id);
    }
    if source == Source::KaitoV75
        && (state["schema"] == crate::embercleave_consumer_v1::CANDIDATES_SCHEMA
            || state["schema"] == crate::limited_mixed_consumer_v1::CANDIDATES_SCHEMA
            || full_pool)
    {
        let meaning: crate::rules_meaning_embercleave_v65::MeaningV65 =
            serde_json::from_value(state["observation"]["embercleave"]["meaning"].clone())
                .map_err(|e| Error::Projection(e.to_string()))?;
        crate::rules_meaning_embercleave_v65::validate(&meaning).map_err(Error::Projection)?;
        covered.insert(meaning.card_id);
    }
    if source == Source::KaitoV75
        && (state["schema"] == crate::chosen_type_consumer_v1::CANDIDATES_SCHEMA
            || state["schema"] == crate::limited_mixed_consumer_v1::CANDIDATES_SCHEMA
            || full_pool)
    {
        let meanings: Vec<crate::rules_meaning_chosen_type_v66::MeaningV66> =
            serde_json::from_value(
                state["observation"]["chosen_type"]["native"]["meanings"].clone(),
            )
            .map_err(|e| Error::Projection(e.to_string()))?;
        for meaning in &meanings {
            crate::rules_meaning_chosen_type_v66::validate(meaning).map_err(Error::Projection)?;
            covered.insert(meaning.card_id);
        }
    }
    if source == Source::KaitoV75
        && (state["schema"] == crate::artist_consumer_v1::CANDIDATES_SCHEMA
            || state["schema"] == crate::limited_mixed_consumer_v1::CANDIDATES_SCHEMA
            || full_pool)
    {
        // Only the distinct sealed actor schema supplies this printed root.
        let meaning: crate::rules_meaning_artist_v64::MeaningV64 =
            serde_json::from_value(state["observation"]["artist"]["meaning"].clone())
                .map_err(|e| Error::Projection(e.to_string()))?;
        crate::rules_meaning_artist_v64::validate(&meaning).map_err(Error::Projection)?;
        covered.insert(meaning.card_id);
    }
    if source == Source::KaitoV75
        && (state["schema"] == crate::niv_consumer_v1::CANDIDATES_SCHEMA
            || state["schema"] == crate::limited_mixed_consumer_v1::CANDIDATES_SCHEMA
            || full_pool)
    {
        let meaning: crate::rules_meaning_niv_mizzet_v63::MeaningV63 =
            serde_json::from_value(state["observation"]["niv"]["appendix"]["meaning"].clone())
                .map_err(|e| Error::Projection(e.to_string()))?;
        crate::rules_meaning_niv_mizzet_v63::validate(&meaning).map_err(Error::Projection)?;
        covered.insert(meaning.card_id);
    }
    if source == Source::KaitoV75
        && (state["schema"] == crate::drakuseth_consumer_v1::CANDIDATES_SCHEMA
            || state["schema"] == crate::limited_mixed_consumer_v1::CANDIDATES_SCHEMA
            || full_pool)
    {
        let meaning: crate::rules_meaning_drakuseth_v67::MeaningV67 =
            serde_json::from_value(state["observation"]["drakuseth"]["meaning"].clone())
                .map_err(|e| Error::Projection(e.to_string()))?;
        crate::rules_meaning_drakuseth_v67::validate(&meaning).map_err(Error::Projection)?;
        covered.insert(meaning.card_id);
    }
    expand(&mut state, &mut covered, source)?;
    let mut cards = BTreeSet::new();
    collect_cards(&state, &mut cards)?;
    for action in &actions {
        collect_cards(action, &mut cards)?;
    }
    let mut meanings = Vec::new();
    for id in cards.difference(&covered) {
        if source == Source::KaitoV75 && super::kaito::printing(*id)?.is_some() {
            continue;
        }
        if matches!(source, Source::ChandraV74 | Source::KaitoV75)
            && super::chandra::printing(*id)?.is_some()
        {
            continue;
        }
        if matches!(
            source,
            Source::LilianaV73 | Source::ChandraV74 | Source::KaitoV75
        ) && super::liliana::printing(*id)?.is_some()
        {
            continue;
        }
        if matches!(
            source,
            Source::TinybonesV72 | Source::LilianaV73 | Source::ChandraV74 | Source::KaitoV75
        ) && super::tinybones::printing(*id)?.is_some()
        {
            continue;
        }
        if matches!(
            source,
            Source::MuldrothaV71
                | Source::TinybonesV72
                | Source::LilianaV73
                | Source::ChandraV74
                | Source::KaitoV75
        ) && super::muldrotha::printing(*id)?.is_some()
        {
            continue;
        }
        if matches!(
            source,
            Source::VivienV70
                | Source::MuldrothaV71
                | Source::TinybonesV72
                | Source::LilianaV73
                | Source::ChandraV74
                | Source::KaitoV75
        ) && super::vivien::printing(*id)?.is_some()
        {
            continue;
        }
        if matches!(
            source,
            Source::GenesisWaveV69
                | Source::VivienV70
                | Source::MuldrothaV71
                | Source::TinybonesV72
                | Source::LilianaV73
                | Source::ChandraV74
                | Source::KaitoV75
        ) && super::genesis_wave::printing(*id)?.is_some()
        {
            continue;
        }
        if matches!(
            source,
            Source::EndTurnV68
                | Source::GenesisWaveV69
                | Source::VivienV70
                | Source::MuldrothaV71
                | Source::TinybonesV72
                | Source::LilianaV73
                | Source::ChandraV74
                | Source::KaitoV75
        ) && super::end_turn::printing(*id)?.is_some()
        {
            continue;
        }
        if matches!(
            source,
            Source::CallV62
                | Source::EndTurnV68
                | Source::GenesisWaveV69
                | Source::VivienV70
                | Source::MuldrothaV71
                | Source::TinybonesV72
                | Source::LilianaV73
                | Source::ChandraV74
                | Source::KaitoV75
        ) && super::call::printing(*id)?.is_some()
        {
            continue;
        }
        if matches!(
            source,
            Source::GraveyardReturnV61
                | Source::CallV62
                | Source::EndTurnV68
                | Source::GenesisWaveV69
                | Source::VivienV70
                | Source::MuldrothaV71
                | Source::TinybonesV72
                | Source::LilianaV73
                | Source::ChandraV74
                | Source::KaitoV75
        ) && super::graveyard_return::printing(*id)?.is_some()
        {
            continue;
        }
        if matches!(
            source,
            Source::NextFiveV59
                | Source::GraveyardReturnV61
                | Source::CallV62
                | Source::EndTurnV68
                | Source::GenesisWaveV69
                | Source::VivienV70
                | Source::MuldrothaV71
                | Source::TinybonesV72
                | Source::LilianaV73
                | Source::ChandraV74
                | Source::KaitoV75
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
                | Source::EndTurnV68
                | Source::GenesisWaveV69
                | Source::VivienV70
                | Source::MuldrothaV71
                | Source::TinybonesV72
                | Source::LilianaV73
                | Source::ChandraV74
                | Source::KaitoV75
        ) && super::prepared::printing(*id)?.is_some()
        {
            continue;
        }
        // These complete ordinary printings are added once by the mandatory
        // supplements below, even if a catalog family already covered an ID.
        if !roots::is_gain_land(*id)
            && !roots::is_simple_trigger_permanent(*id)
            && !roots::is_ordinary_equipment(*id)
            && !roots::is_ordinary_aura(*id)
            && !roots::is_team_static_permanent(*id)
            && !roots::is_shivan_permanent(*id)
            && !roots::is_sacrifice_permanent(*id)
            && !roots::is_sailor_permanent(*id)
            && !roots::is_lookout_permanent(*id)
            && !roots::is_evolving_permanent(*id)
            && !roots::is_surveil_one_permanent(*id)
            && !roots::is_paladin_permanent(*id)
            && !roots::is_mandatory_ordinary_spell(*id)
        {
            meanings.push(roots::baseline(*id)?);
        }
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
        Source::PreparedV57
            | Source::NextFiveV59
            | Source::GraveyardReturnV61
            | Source::CallV62
            | Source::EndTurnV68
            | Source::GenesisWaveV69
            | Source::VivienV70
            | Source::MuldrothaV71
            | Source::TinybonesV72
            | Source::LilianaV73
            | Source::ChandraV74
            | Source::KaitoV75
    ) {
        for id in &cards {
            if let Some(printing) = super::prepared::printing(*id)? {
                meanings.push(printing);
            }
        }
    }
    if matches!(
        source,
        Source::NextFiveV59
            | Source::GraveyardReturnV61
            | Source::CallV62
            | Source::EndTurnV68
            | Source::GenesisWaveV69
            | Source::VivienV70
            | Source::MuldrothaV71
            | Source::TinybonesV72
            | Source::LilianaV73
            | Source::ChandraV74
            | Source::KaitoV75
    ) {
        for id in &cards {
            if let Some(printing) = super::next_five::printing(*id)? {
                meanings.push(printing);
            }
        }
    }
    if matches!(
        source,
        Source::GraveyardReturnV61
            | Source::CallV62
            | Source::EndTurnV68
            | Source::GenesisWaveV69
            | Source::VivienV70
            | Source::MuldrothaV71
            | Source::TinybonesV72
            | Source::LilianaV73
            | Source::ChandraV74
            | Source::KaitoV75
    ) {
        for id in &cards {
            if let Some(printing) = super::graveyard_return::printing(*id)? {
                meanings.push(printing);
            }
        }
    }
    if matches!(
        source,
        Source::CallV62
            | Source::EndTurnV68
            | Source::GenesisWaveV69
            | Source::VivienV70
            | Source::MuldrothaV71
            | Source::TinybonesV72
            | Source::LilianaV73
            | Source::ChandraV74
            | Source::KaitoV75
    ) {
        for id in &cards {
            if let Some(printing) = super::call::printing(*id)? {
                meanings.push(printing);
            }
        }
    }
    if matches!(
        source,
        Source::GenesisWaveV69
            | Source::VivienV70
            | Source::MuldrothaV71
            | Source::TinybonesV72
            | Source::LilianaV73
            | Source::ChandraV74
            | Source::KaitoV75
    ) {
        for id in &cards {
            if let Some(printing) = super::genesis_wave::printing(*id)? {
                meanings.push(printing);
            }
        }
    }
    if matches!(
        source,
        Source::EndTurnV68
            | Source::GenesisWaveV69
            | Source::VivienV70
            | Source::MuldrothaV71
            | Source::TinybonesV72
            | Source::LilianaV73
            | Source::ChandraV74
            | Source::KaitoV75
    ) {
        for id in &cards {
            if let Some(printing) = super::end_turn::printing(*id)? {
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
    if matches!(
        source,
        Source::VivienV70
            | Source::MuldrothaV71
            | Source::TinybonesV72
            | Source::LilianaV73
            | Source::ChandraV74
            | Source::KaitoV75
    ) {
        for id in &cards {
            if let Some(p) = super::vivien::printing(*id)? {
                meanings.push(p);
            }
        }
    }
    if matches!(
        source,
        Source::MuldrothaV71
            | Source::TinybonesV72
            | Source::LilianaV73
            | Source::ChandraV74
            | Source::KaitoV75
    ) {
        for id in &cards {
            if let Some(p) = super::muldrotha::printing(*id)? {
                meanings.push(p);
            }
        }
    }
    if matches!(
        source,
        Source::TinybonesV72 | Source::LilianaV73 | Source::ChandraV74 | Source::KaitoV75
    ) {
        for id in &cards {
            if let Some(printed) = super::tinybones::printing(*id)? {
                meanings.push(printed);
            }
        }
    }
    if matches!(
        source,
        Source::LilianaV73 | Source::ChandraV74 | Source::KaitoV75
    ) {
        for id in &cards {
            if let Some(printed) = super::liliana::printing(*id)? {
                meanings.push(printed);
            }
        }
    }
    if matches!(source, Source::ChandraV74 | Source::KaitoV75) {
        for id in &cards {
            if let Some(p) = super::chandra::printing(*id)? {
                meanings.push(p);
            }
        }
    }
    if source == Source::KaitoV75 {
        for id in &cards {
            if let Some(p) = super::kaito::printing(*id)? {
                meanings.push(p);
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

/// This distinct source accepts only the sealed owner's typed public parts.
/// Every visible or publicly named printing receives its full finite meaning;
/// the Limited meaning allowlist remains unchanged.
pub(super) fn encode_eternal(
    public: &super::eternal::EternalPublicV1,
    actions: &[(
        crate::rl::ActionSemanticV1,
        Option<crate::eternal_actor_public_v1::EntryOptionV1>,
    )],
) -> Result<Packet> {
    let conversion = |error: serde_json::Error| Error::Projection(error.to_string());
    let mut observation = serde_json::to_value(&public.observation).map_err(conversion)?;
    super::public_projection_operands::prepare(&mut observation)?;
    let public = serde_json::to_value(public).map_err(conversion)?;
    let actions = actions
        .iter()
        .map(|(action, option)| {
            Ok(
                json!({"action": serde_json::to_value(action).map_err(conversion)?,
            "entry_option": serde_json::to_value(option).map_err(conversion)?}),
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let mut state = public;
    state["observation"] = observation;
    let mut cards = BTreeSet::new();
    collect_cards(&state, &mut cards)?;
    fn named(value: &Value, cards: &mut BTreeSet<u16>) -> Result<()> {
        match value {
            Value::Object(fields) => {
                for (key, value) in fields {
                    if matches!(key.as_str(), "named_card_id" | "printing") {
                        cards.insert(u16::try_from(uint(value)?).map_err(|_| Error::Range)?);
                    } else {
                        named(value, cards)?;
                    }
                }
            }
            Value::Array(values) => {
                for value in values {
                    named(value, cards)?;
                }
            }
            _ => (),
        }
        Ok(())
    }
    named(&state, &mut cards)?;
    for action in &actions {
        collect_cards(action, &mut cards)?;
        named(action, &mut cards)?;
    }
    let meanings = cards
        .into_iter()
        .map(|printing| {
            crate::eternal_actor_printed_v1::printing_v1(printing)
                .map(|meaning| json!({"printing":printing,"meaning":meaning}))
                .map_err(|error| Error::Projection(error.to_string()))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut refs = References::default();
    let mut times = BTreeSet::new();
    timestamps(&state, "", &mut times);
    for action in &actions {
        timestamps(action, "", &mut times);
    }
    refs.timestamps = times
        .into_iter()
        .enumerate()
        .map(|(i, t)| (t, i as u64))
        .collect();
    // Establish object correspondence from the ordinary public observation
    // before visiting supplemental ledger rows or action operands.
    rewrite(&state["observation"], &mut refs, "")?;
    state["printing_meanings"] = Value::Array(meanings);
    let mut state = rewrite(&state, &mut refs, "")?;
    // These are sets of public facts, unlike stack/pending order or per-seat
    // arrays. Native ledger insertion order must not become an actor feature.
    for key in [
        "cavern_types",
        "needle_names",
        "uncounterable_spells",
        "cast_mana",
        "saga_abilities",
        "prepare_copies",
        "animate_dead_waiting",
        "animate_dead_attached",
        "granted_opponent_draw_on_leave",
    ] {
        let rows = state["dynamic"][key]
            .as_array_mut()
            .ok_or_else(|| Error::UnsupportedOperand(key.into()))?;
        rows.sort_by_cached_key(Value::to_string);
    }
    let actions = actions
        .iter()
        .map(|action| NumericTree::encode(&rewrite(action, &mut refs, "")?))
        .collect::<Result<Vec<_>>>()?;
    if actions.is_empty() {
        return Err(Error::NoDecision);
    }
    Ok(Packet {
        source: Source::EternalActorV1,
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
