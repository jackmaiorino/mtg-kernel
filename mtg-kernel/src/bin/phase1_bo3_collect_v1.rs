//! Explicit one-match collector. This command never updates weights or starts
//! a training campaign. Current-executable package verification is mandatory.
use mtg_kernel::durable_publication_v1::{
    capture_existing_publication_parent_v1, publish_new_file_v1, DurableFileExpectationV1,
};
use mtg_kernel::phase1_bo3_collection_v1::{
    collect_bo3_trajectory_v1, Bo3CollectionRequestV1, MAX_BO3_COLLECTION_REQUEST_BYTES_V1,
};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::PathBuf;

/// Build an explicit fixed-opening/keep-sideboard package from the actual
/// loaded inference source and this executable's compiled runtime identity.
/// This does not play a match, migrate an objective or attach a learned head.
fn prepare_package(
    text: &str,
    toolchain: PathBuf,
) -> Result<mtg_kernel::phase1_agent_v1::CompleteAgentPackageV1, String> {
    use mtg_kernel::expanded_deck_training_v1::{
        load_expanded_inference_v1, ExpandedModelSourceV1, ExpandedSeatBehaviorV1, PinnedFileV1,
    };
    use mtg_kernel::phase1_agent_v1::*;
    let source: ExpandedModelSourceV1 = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let (policy, identity) = load_expanded_inference_v1(&source)?;
    let pinned = |path: PathBuf| -> Result<PinnedFileV1, String> {
        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
        Ok(PinnedFileV1 {
            path,
            sha256: format!("{:x}", Sha256::digest(bytes)),
        })
    };
    let runtime = AgentRuntimeIdentityV1 {
        executable: pinned(std::env::current_exe().map_err(|e| e.to_string())?)?,
        toolchain: pinned(toolchain)?,
        engine_commit: env!("MTG_KERNEL_BUILD_GIT_HEAD").into(),
        tracked_tree_sha256: env!("MTG_KERNEL_BUILD_TRACKED_TREE_SHA256").into(),
        tracked_tree_contract: env!("MTG_KERNEL_BUILD_TRACKED_TREE_CONTRACT").into(),
        build_git_clean: env!("MTG_KERNEL_BUILD_GIT_CLEAN") == "true",
        card_db_hash: identity.model.card_db_hash.clone(),
        card_registry_sha256: format!(
            "{:x}",
            Sha256::digest(include_bytes!("../../../data/cards_v1.json"))
        ),
        feature_contract_digest: identity.model.feature_contract_digest.clone(),
        feature_encoding_digest: identity.model.feature_encoding_digest.clone(),
        features_source_sha256: identity.features_source_sha256.clone(),
        feature_descriptor_sha256: identity.feature_descriptor_sha256.clone(),
    };
    let package = CompleteAgentPackageV1 {
        schema: COMPLETE_AGENT_PACKAGE_SCHEMA_V1.into(),
        runtime,
        gameplay: ExpandedSeatBehaviorV1 { source, identity },
        gameplay_sampler_identity: policy.runtime_sampler_identity_v1().into(),
        opening: AgentOpeningPolicyV1::Existing {
            protocol: mtg_kernel::learned_bo3_v1::Bo3OpeningProtocolV1::KeepSevenV2,
        },
        play_draw: AgentPlayDrawPolicyV1::Fixed {
            choice: mtg_kernel::bo3_match::PlayDrawChoiceV1::Play,
        },
        sideboard: AgentSideboardPolicyV1::Keep,
        search: AgentSearchPolicyV1::Disabled,
    };
    package.load_supported_components_v1()?;
    Ok(package)
}

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let hashing = args.len() == 4 && args[0] == "--hash-record" && args[2] == "--output";
    let preparing = args.len() == 6
        && args[0] == "--prepare-source"
        && args[2] == "--toolchain"
        && args[4] == "--output";
    let reporting = args.len() == 8 && args[0] == "--request" && args[2] == "--output"
        && args[4] == "--report-search" && args[6] == "--archive";
    let auditing = args.len() == 6
        && args[0] == "--request"
        && args[2] == "--output"
        && (args[4] == "--combat-audit" || args[4] == "--burn-audit" || args[4] == "--continuation" || args[4] == "--evaluate-search");
    if !preparing
        && !hashing
        && !auditing
        && !reporting
        && (args.len() != 4 || args[0] != "--request" || args[2] != "--output")
    {
        return Err(
            "usage: phase1_bo3_collect_v1 --request request.json --output new-result.json [--combat-audit options.json | --burn-audit options.json | --continuation options.json | --evaluate-search options.json | --report-search options.json --archive archive.json]; or --prepare-source source.json --toolchain rust-toolchain.toml --output new-package.json".into(),
        );
    }
    let output = PathBuf::from(&args[if preparing { 5 } else { 3 }]);
    let parent_path = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    let parent = capture_existing_publication_parent_v1(parent_path).map_err(|e| e.to_string())?;
    let final_name = output.file_name().ok_or("output requires a filename")?;
    let mut stage_name = final_name.to_os_string();
    stage_name.push(".stage");
    if output.try_exists().map_err(|e| e.to_string())?
        || parent_path
            .join(&stage_name)
            .try_exists()
            .map_err(|e| e.to_string())?
    {
        return Err(
            "output or staging file already exists; preserve and inspect it before replay".into(),
        );
    }
    let mut bytes = Vec::new();
    std::fs::File::open(&args[1])
        .map_err(|e| e.to_string())?
        .take(MAX_BO3_COLLECTION_REQUEST_BYTES_V1 as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let text = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
    let (bytes, summary) = if hashing {
        let hash = mtg_kernel::phase1_bo3_collection_v1::continuation_record_sha256_v1(text)?;
        (serde_json::to_vec(&serde_json::json!({"record_sha256":hash})).map_err(|e| e.to_string())?,
            "typed continuation record hash; no games".into())
    } else if preparing {
        if bytes.len() > MAX_BO3_COLLECTION_REQUEST_BYTES_V1 {
            return Err("package source exceeds 4 MiB".into());
        }
        let package = prepare_package(text, PathBuf::from(&args[3]))?;
        let digest = package.package_sha256_v1()?;
        (
            serde_json::to_vec(&package).map_err(|e| e.to_string())?,
            format!("verified package {digest}; no games"),
        )
    } else {
        let request = Bo3CollectionRequestV1::from_json_v1(text)?;
        if reporting {
            #[cfg(feature="experimental-burn-net8-packed-cuda-v1")]
            {
                use mtg_kernel::phase1_bo3_collection_v1::{report_bo3_v4,Bo3ReportOptionsV1,Bo3ReportArchiveV1};
                fn bounded(path:&std::ffi::OsStr,limit:u64)->Result<Vec<u8>,String> {
                    let mut bytes=Vec::new();
                    std::fs::File::open(path).map_err(|e|e.to_string())?.take(limit+1)
                        .read_to_end(&mut bytes).map_err(|e|e.to_string())?;
                    if bytes.len() as u64>limit {return Err(format!("Report input exceeds {limit} bytes"));}
                    Ok(bytes)
                }
                // Strict parsing rejects duplicate keys even inside nested maps.
                // Archive is data only; current packages still bind this executable.
                let options_bytes=bounded(&args[5],32768)?;
                let options=Bo3ReportOptionsV1::from_json_v1(std::str::from_utf8(&options_bytes).map_err(|e|e.to_string())?)?;
                let archive_bytes=bounded(&args[7],64*1024*1024)?;
                let archive=Bo3ReportArchiveV1::from_json_v1(std::str::from_utf8(&archive_bytes).map_err(|e|e.to_string())?)?;
                let result=report_bo3_v4(request.config,request.packages,options,archive)?;
                (serde_json::to_vec(&result).map_err(|e|e.to_string())?,
                    "V4 Report observation; inspect usable_job and every assigned row".into())
            }
            #[cfg(not(feature="experimental-burn-net8-packed-cuda-v1"))]
            { return Err("Report search requires experimental-burn-net8-packed-cuda-v1".into()); }
        } else if auditing && args[4] == "--evaluate-search" {
            #[cfg(feature="experimental-burn-net8-packed-cuda-v1")]
            {
                use mtg_kernel::phase1_bo3_collection_v1::{evaluate_bo3_v4,Bo3EvaluationOptionsV1};
                let mut options=String::new();
                std::fs::File::open(&args[5]).map_err(|e|e.to_string())?
                    .take(4097).read_to_string(&mut options).map_err(|e|e.to_string())?;
                if options.len()>4096 {return Err("evaluation options exceed 4 KiB".into());}
                // This one-field typed struct rejects unknown/duplicate fields.
                let options:Bo3EvaluationOptionsV1=serde_json::from_str(&options).map_err(|e|e.to_string())?;
                let result=evaluate_bo3_v4(request.config,request.packages,options)?;
                (serde_json::to_vec(&result).map_err(|e|e.to_string())?,
                    "V4 evaluation result; inspect ending and typed abort".into())
            }
            #[cfg(not(feature="experimental-burn-net8-packed-cuda-v1"))]
            { return Err("search evaluation requires experimental-burn-net8-packed-cuda-v1".into()); }
        } else if auditing && args[4] == "--continuation" {
            use mtg_kernel::phase1_bo3_collection_v1::{collect_bo3_with_continuation_v1, Bo3ContinuationOptionsV1};
            let mut options = String::new();
            std::fs::File::open(&args[5]).map_err(|e| e.to_string())?
                .take(32769).read_to_string(&mut options).map_err(|e| e.to_string())?;
            let result = collect_bo3_with_continuation_v1(request.config, request.packages,
                Bo3ContinuationOptionsV1::from_json_v1(&options)?)?;
            let summary = format!("continuation complete={}", result.continuation["complete"]);
            (serde_json::to_vec(&result).map_err(|e| e.to_string())?, summary)
        } else if auditing && args[4] == "--burn-audit" {
            use mtg_kernel::phase1_bo3_collection_v1::{collect_bo3_with_burn_audit_v1, Bo3BurnAuditOptionsV1};
            let mut options = String::new();
            std::fs::File::open(&args[5]).map_err(|e| e.to_string())?
                .take(4097).read_to_string(&mut options).map_err(|e| e.to_string())?;
            let result = collect_bo3_with_burn_audit_v1(request.config, request.packages,
                Bo3BurnAuditOptionsV1::from_json_v1(&options)?)?;
            let summary = format!("burn audit complete={}", result.burn_audit["complete"]);
            (serde_json::to_vec(&result).map_err(|e| e.to_string())?, summary)
        } else if auditing {
            use mtg_kernel::phase1_bo3_collection_v1::{
                collect_bo3_with_combat_audit_v1, Bo3CombatAuditOptionsV1,
            };
            let mut options = String::new();
            std::fs::File::open(&args[5])
                .map_err(|e| e.to_string())?
                .take(4097)
                .read_to_string(&mut options)
                .map_err(|e| e.to_string())?;
            let options = Bo3CombatAuditOptionsV1::from_json_v1(&options)?;
            let result =
                collect_bo3_with_combat_audit_v1(request.config, request.packages, options)?;
            let summary = format!(
                "{} {:?}; combat audit complete={}",
                result.collection.collected.trajectory_sha256,
                result.collection.collected.trajectory.ending,
                result.combat_audit["complete"]
            );
            (
                serde_json::to_vec(&result).map_err(|e| e.to_string())?,
                summary,
            )
        } else {
            let result = collect_bo3_trajectory_v1(request.config, request.packages)?;
            let summary = format!(
                "{} {:?}",
                result.collected.trajectory_sha256, result.collected.trajectory.ending
            );
            (
                serde_json::to_vec(&result).map_err(|e| e.to_string())?,
                summary,
            )
        }
    };
    let expected = DurableFileExpectationV1::from_bytes(&bytes).map_err(|e| e.to_string())?;
    publish_new_file_v1(&parent, &stage_name, final_name, &bytes, expected)
        .map_err(|e| e.to_string())?;
    println!("{} {}", output.display(), summary);
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
