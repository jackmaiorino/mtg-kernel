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
    let preparing = args.len() == 6
        && args[0] == "--prepare-source"
        && args[2] == "--toolchain"
        && args[4] == "--output";
    if !preparing && (args.len() != 4 || args[0] != "--request" || args[2] != "--output") {
        return Err(
            "usage: phase1_bo3_collect_v1 --request request.json --output new-result.json; or --prepare-source source.json --toolchain rust-toolchain.toml --output new-package.json".into(),
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
    let (bytes, summary) = if preparing {
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
        let result = collect_bo3_trajectory_v1(request.config, request.packages)?;
        let summary = format!(
            "{} {:?}",
            result.collected.trajectory_sha256, result.collected.trajectory.ending
        );
        (
            serde_json::to_vec(&result).map_err(|e| e.to_string())?,
            summary,
        )
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
