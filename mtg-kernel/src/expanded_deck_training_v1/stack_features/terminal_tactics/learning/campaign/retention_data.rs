//! Validate copied V4 retention inputs against their immutable parent model.
use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    source: ExpandedModelSourceV1,
    dataset: PinnedFileV1,
    teacher: PinnedFileV1,
    output_directory: PathBuf,
}

fn tensor_key(t: &Value) -> Result<String,String> {
    // Value objects have sorted keys in this build. Use the same canonical
    // representation for both sides of local equality, not a foreign digest.
    Ok(sha(&serde_json::to_vec(t).map_err(err)?))
}

pub(super) fn run(c: Command) -> Result<Value,String> {
    let data:Value=serde_json::from_slice(&read_pinned_bytes(&c.dataset)?).map_err(err)?;
    let teacher:Value=serde_json::from_slice(&read_pinned_bytes(&c.teacher)?).map_err(err)?;
    ensure(data["schema"]=="terminal-parent-retention-data/v1" && data["parent_source"]==serde_json::to_value(&c.source).map_err(err)? && teacher["source"]==data["parent_source"],"retention data parent identity differs")?;
    let teacher_rows=teacher["records"].as_array().ok_or("missing retention teacher rows")?;
    ensure(teacher_rows.len()==32 && teacher_rows.iter().all(|r|r["spec"]["split"]=="train"),"retention check rejects heldout teacher")?;
    let forbidden=teacher_rows.iter().map(|r|tensor_key(&r["data"]["tensor"])).collect::<Result<BTreeSet<_>,_>>()?;
    let groups=data["groups"].as_array().ok_or("missing retention groups")?;
    ensure(groups.len()==256,"retention check requires256 groups")?;
    let (parent,state)=initialize(&c.source)?;
    ensure(parent.feature_identity_v1().generation==FreshLineageGenerationV1::V4,"retention data requires V4")?;
    let state_hash=hex(&state.state_sha256_v1().map_err(err)?);
    let mut seen=BTreeSet::new();let mut tensors=BTreeSet::new();let mut coverage=BTreeMap::<(String,u64),usize>::new();
    let mut total=0;let mut widths=BTreeSet::new();let mut multiple=0;
    for group in groups {
        let selection=&group["selection"];
        ensure(seen.insert(selection["id"].as_str().ok_or("missing retained group id")?),"duplicate retention id")?;
        let actor=selection["actor"].as_u64().ok_or("missing retention actor")?;
        ensure(actor<2,"invalid retention actor")?;
        let deck=selection["deck"].as_str().ok_or("missing retention deck")?.to_string();
        *coverage.entry((deck,actor)).or_default()+=1;
        let rows=group["rows"].as_array().ok_or("missing retained rows")?;
        ensure(!rows.is_empty() && rows.len()<=64 && selection["archive_rows"].as_array().ok_or("missing row indices")?.len()==rows.len(),"retention physical group bounds differ")?;
        let mut keys=Vec::new();multiple+=usize::from(rows.len()>1);
        for row in rows {
            ensure(row.get("reward").is_none() && row.get("selected").is_none(),"retention must not carry rewards/actions")?;
            let key=tensor_key(&row["tensor"])?;ensure(!forbidden.contains(&key),"retention overlaps teacher tensor")?;keys.push(key);
            let bits_tensor:TensorBitsV1=serde_json::from_value(row["tensor"].clone()).map_err(err)?;
            let scores=parent.score_training_tensor_v4(&NativeFlatDecisionTensorV4{common:bits_tensor.tensor()})?;
            let expected:Vec<u32>=serde_json::from_value(row["parent_logits_bits"].clone()).map_err(err)?;
            let value:u32=serde_json::from_value(row["parent_value_bits"].clone()).map_err(err)?;
            ensure(bits(&scores.logits)==expected && scores.value.to_bits()==value,"retention g115 replay differs")?;
            widths.insert(scores.logits.len());total+=1;
        }
        ensure(tensors.insert(keys),"duplicate retention tensor group")?;
    }
    ensure(coverage.len()==16 && coverage.values().all(|&n|n==16),"retention deck/seat coverage differs")?;
    ensure(state_hash==hex(&state.state_sha256_v1().map_err(err)?),"retention check mutated optimizer")?;
    let coverage:Vec<_>=coverage.into_iter().map(|((deck,actor),groups)|json!({"deck":deck,"actor":actor,"groups":groups})).collect();
    let result=json!({"schema":"terminal-retention-data-check/v1","complete":true,"dataset":c.dataset,"teacher":c.teacher,
        "groups":256,"rows":total,"multirow_groups":multiple,"action_widths":widths,"coverage":coverage,
        "parent_state":state_hash,"exact_parent_rows":total,"updates":0,"validation_positions_read":0,
        "non_claim":"Exact saved-input parent replay, not policy preservation or playing-strength evidence."});
    fs::create_dir(&c.output_directory).map_err(err)?;publish_json(&c.output_directory,"result.json",&result)?;Ok(result)
}
