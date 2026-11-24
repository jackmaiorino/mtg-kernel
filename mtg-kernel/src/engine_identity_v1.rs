//! The engine a build runs, as training records report it (mtg-kernel#921).
//!
//! A rules correction can change model inputs without changing any feature
//! contract or encoding digest, so a run's records name the engine that wrote
//! them: the build's Git commit, the tracked-tree SHA-256 that `build.rs`
//! embeds for it, whether the build's worktree was clean, and
//! [`ENGINE_RULES_VERSION_V1`].
//!
//! The identity is provenance only. It never enters a state digest, an
//! after-state hash, a model or inference identity, a checkpoint or any
//! update receipt, and nothing refuses on it. Its field names reuse the build
//! provenance names run manifests already carry (`git_commit`,
//! `tracked_tree_sha256`).

use serde_json::{json, Value};

/// The engine's game-rules version. Bump it in the same change as any rules
/// correction that can change play, menus or an encoded value for some game
/// state, even when every feature digest stays the same.
///
/// 1: the first recorded version. It includes "Complete dungeons after their
/// room abilities leave the stack" (`5a8491a24`, merged by #566); engines that
/// predate that change record no rules version.
pub const ENGINE_RULES_VERSION_V1: u32 = 1;

pub const ENGINE_IDENTITY_SCHEMA_V1: &str = "mtg-kernel-engine-identity/v1";

/// This build's engine identity.
pub fn engine_identity_v1() -> Value {
    json!({
        "schema": ENGINE_IDENTITY_SCHEMA_V1,
        "git_commit": env!("MTG_KERNEL_BUILD_GIT_HEAD"),
        "tracked_tree_sha256": env!("MTG_KERNEL_BUILD_TRACKED_TREE_SHA256"),
        "git_clean": env!("MTG_KERNEL_BUILD_GIT_CLEAN") == "true",
        "rules_version": ENGINE_RULES_VERSION_V1,
    })
}

/// One plain phrase for an identity, recorded or running. Missing fields
/// read as `unknown`, so a malformed record still prints.
pub fn describe_engine_identity_v1(identity: &Value) -> String {
    let text = |key: &str| {
        identity[key]
            .as_str()
            .map_or_else(|| "unknown".to_owned(), str::to_owned)
    };
    let clean = match identity["git_clean"].as_bool() {
        Some(true) => "clean",
        Some(false) => "dirty",
        None => "clean status unknown",
    };
    let rules = identity["rules_version"]
        .as_u64()
        .map_or_else(|| "unknown".to_owned(), |version| version.to_string());
    format!(
        "commit {}, tracked tree {}, {clean}, rules version {rules}",
        text("git_commit"),
        text("tracked_tree_sha256"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_identity_names_this_build_and_the_rules_version() {
        let identity = engine_identity_v1();
        assert_eq!(identity["schema"], ENGINE_IDENTITY_SCHEMA_V1);
        assert_eq!(identity["git_commit"], env!("MTG_KERNEL_BUILD_GIT_HEAD"));
        assert_eq!(
            identity["tracked_tree_sha256"],
            env!("MTG_KERNEL_BUILD_TRACKED_TREE_SHA256")
        );
        assert_eq!(
            identity["git_clean"],
            env!("MTG_KERNEL_BUILD_GIT_CLEAN") == "true"
        );
        assert_eq!(identity["rules_version"], ENGINE_RULES_VERSION_V1);
        assert_eq!(identity, engine_identity_v1(), "deterministic");
        let described = describe_engine_identity_v1(&identity);
        assert!(described.contains(env!("MTG_KERNEL_BUILD_GIT_HEAD")));
        assert!(described.contains(&format!("rules version {ENGINE_RULES_VERSION_V1}")));
        assert_eq!(
            describe_engine_identity_v1(&Value::Null),
            "commit unknown, tracked tree unknown, clean status unknown, rules version unknown"
        );
    }
}
