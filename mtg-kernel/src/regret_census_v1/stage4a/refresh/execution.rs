//! Execution choice only. Search selection, support and allocation stay shared.
use super::{json, KeyMode, Recording, Step, Value};

pub(super) const ROOT_ONLY_IDENTITY: &str = "root-only-v1";
const ROOT_ONLY_ARM: &str = "search-root-only-v1";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::regret_census_v1::stage4a) enum ExecutionMode {
    #[default]
    Persistent,
    RootOnly,
}

impl ExecutionMode {
    pub(in crate::regret_census_v1::stage4a) fn parse(
        mode: &str,
        setting: Result<String, std::env::VarError>,
    ) -> Result<Self, String> {
        let setting = match setting {
            Err(std::env::VarError::NotPresent) => return Ok(Self::Persistent),
            Err(error) => return Err(format!("invalid S4A_EXECUTOR: {error}")),
            Ok(value) => value,
        };
        if mode != "s4a-budget" {
            return Err("S4A_EXECUTOR is supported only for s4a-budget".into());
        }
        match setting.as_str() {
            "persistent-v1" => Ok(Self::Persistent),
            ROOT_ONLY_IDENTITY => Ok(Self::RootOnly),
            _ => Err(format!("unsupported S4A_EXECUTOR {setting:?}")),
        }
    }

    pub(in crate::regret_census_v1::stage4a) fn validate_budget(
        self,
        plain: bool,
        mode: KeyMode,
        interchange: bool,
        bestline: Option<u64>,
        recording: Recording,
    ) -> Result<(), String> {
        if self == Self::RootOnly
            && (plain
                || mode != KeyMode::Repaired
                || interchange
                || bestline.is_some()
                || recording != Recording::Disabled)
        {
            return Err("root-only-v1 requires searched s4a-budget, repaired keys, mean backup, no interchange and no reuse-witness recording".into());
        }
        Ok(())
    }

    pub(super) fn annotate_row(self, row: &mut Value) {
        if self == Self::RootOnly {
            // Existing SD1 deliberately refuses every explicit arm field.
            row["arm"] = json!(ROOT_ONLY_ARM);
            row["executor"] = json!(ROOT_ONLY_IDENTITY);
            row["execution_convention"] = json!("supported-new-root-tree; supported-reuse-one-live-raw-draw; advance-actual-action/v1");
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Route {
    Tree(usize),
    RawInsteadOfReuse(usize),
    Plain,
}

pub(super) fn route(mode: ExecutionMode, new_root: bool, step: Step) -> Route {
    match step {
        Step::Execute(edge) if mode == ExecutionMode::RootOnly && !new_root => {
            Route::RawInsteadOfReuse(edge)
        }
        Step::Execute(edge) => Route::Tree(edge),
        _ => Route::Plain,
    }
}

impl Route {
    pub(super) fn chooser(self, new_root: bool) -> &'static str {
        match self {
            Self::Tree(_) if new_root => "new_root",
            Self::Tree(_) => "reused",
            Self::RawInsteadOfReuse(_) => "raw_instead_of_reuse",
            Self::Plain => "plain",
        }
    }
}

pub(super) fn choose(
    route: Route,
    tree: impl FnOnce(usize) -> Result<u32, String>,
    raw: impl FnOnce() -> Result<u32, String>,
) -> Result<u32, String> {
    match route {
        Route::Tree(edge) => tree(edge),
        Route::RawInsteadOfReuse(_) | Route::Plain => raw(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::VarError;

    #[test]
    fn executor_setting_is_explicit_budget_only_and_strict() {
        for mode in ["s4a-budget", "decisions", "s4a-diag", "s4a-reused-action"] {
            assert_eq!(
                ExecutionMode::parse(mode, Err(VarError::NotPresent)).unwrap(),
                ExecutionMode::Persistent
            );
        }
        assert_eq!(
            ExecutionMode::parse("s4a-budget", Ok("persistent-v1".into())).unwrap(),
            ExecutionMode::Persistent
        );
        assert_eq!(
            ExecutionMode::parse("s4a-budget", Ok("root-only-v1".into())).unwrap(),
            ExecutionMode::RootOnly
        );
        for mode in [
            "decisions",
            "s4a-refresh",
            "s4a-root-rebuild",
            "s4a-reuse-prefix",
        ] {
            for setting in ["persistent-v1", "root-only-v1"] {
                assert!(ExecutionMode::parse(mode, Ok(setting.into())).is_err());
            }
        }
        for setting in ["", "0", "root-only", "unknown"] {
            assert!(ExecutionMode::parse("s4a-budget", Ok(setting.into())).is_err());
        }
        assert!(
            ExecutionMode::parse("s4a-budget", Err(VarError::NotUnicode("opaque".into()))).is_err()
        );
    }

    #[test]
    fn executor_refuses_unqualified_combinations() {
        let root = ExecutionMode::RootOnly;
        assert!(root
            .validate_budget(false, KeyMode::Repaired, false, None, Recording::Disabled)
            .is_ok());
        for (plain, mode, interchange, bestline, recording) in [
            (true, KeyMode::Repaired, false, None, Recording::Disabled),
            (false, KeyMode::Legacy, false, None, Recording::Disabled),
            (false, KeyMode::Repaired, true, None, Recording::Disabled),
            (
                false,
                KeyMode::Repaired,
                false,
                Some(8),
                Recording::Disabled,
            ),
            (false, KeyMode::Repaired, false, None, Recording::Enabled),
        ] {
            assert!(root
                .validate_budget(plain, mode, interchange, bestline, recording)
                .is_err());
            assert!(ExecutionMode::Persistent
                .validate_budget(plain, mode, interchange, bestline, recording)
                .is_ok());
        }
    }

    #[test]
    fn executor_routes_supported_reuse_to_exactly_one_raw_selection() {
        for mode in [ExecutionMode::Persistent, ExecutionMode::RootOnly] {
            for new_root in [false, true] {
                let route = route(mode, new_root, Step::Execute(7));
                let (mut tree_calls, mut raw_calls) = (0, 0);
                let action = choose(
                    route,
                    |edge| {
                        tree_calls += 1;
                        assert_eq!(edge, 7);
                        Ok(19)
                    },
                    || {
                        raw_calls += 1;
                        Ok(3)
                    },
                )
                .unwrap();
                let raw = mode == ExecutionMode::RootOnly && !new_root;
                assert_eq!((tree_calls, raw_calls), if raw { (0, 1) } else { (1, 0) });
                assert_eq!(action, if raw { 3 } else { 19 });
                assert_eq!(
                    route.chooser(new_root),
                    if raw {
                        "raw_instead_of_reuse"
                    } else if new_root {
                        "new_root"
                    } else {
                        "reused"
                    }
                );
            }
        }
        for mode in [ExecutionMode::Persistent, ExecutionMode::RootOnly] {
            for step in [Step::Plain, Step::Refresh(64000)] {
                let route = route(mode, false, step);
                assert_eq!(route, Route::Plain);
                assert_eq!(route.chooser(false), "plain");
                assert_eq!(
                    choose(route, |_| panic!("fallback called tree"), || Ok(5)).unwrap(),
                    5
                );
            }
        }
        let mut calls = 0;
        assert!(choose(
            Route::RawInsteadOfReuse(0),
            |_| panic!("shadow tree selection"),
            || {
                calls += 1;
                Err("raw failed".into())
            }
        )
        .is_err());
        assert_eq!(calls, 1);
    }

    #[test]
    fn executor_identity_leaves_persistent_rows_byte_identical() {
        let original = serde_json::json!({"kind":"budget_game","schema":"search-total-budget-v1/game","game":{"decisions":[]}});
        let mut row = original.clone();
        ExecutionMode::Persistent.annotate_row(&mut row);
        assert_eq!(
            serde_json::to_vec(&row).unwrap(),
            serde_json::to_vec(&original).unwrap()
        );
        ExecutionMode::RootOnly.annotate_row(&mut row);
        assert_eq!(row["arm"], ROOT_ONLY_ARM);
        assert_eq!(row["executor"], ROOT_ONLY_IDENTITY);
    }

    #[test]
    fn root_only_rows_are_refused_by_unchanged_sd1_reader() {
        use crate::regret_census_v1::stage4a::{
            bank::{BankSpec, SPEC_SCHEMA},
            root_rebuild,
        };
        let spec = BankSpec {
            schema: SPEC_SCHEMA.into(),
            namespace: "spy-reset-pilot-v1".into(),
            lineage: "fixture".into(),
            runtime_rules: "resolution-boundary-v1".into(),
            select_rule: "fpu-1.5".into(),
            max_physical_decisions: 128,
            max_policy_steps: 256,
            decks: vec![],
            sources: vec![],
        };
        let mut row = json!({"kind":"budget_game","schema":"search-total-budget-v1/game"});
        ExecutionMode::RootOnly.annotate_row(&mut row);
        // This gate runs before replay or policy access, even with no deck input.
        let rebuilt = root_rebuild::game(&spec, &row, None);
        assert_eq!(rebuilt["void"], "not a search-arm record");
    }
}
